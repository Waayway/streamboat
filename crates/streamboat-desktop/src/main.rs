//! `streamboat`: the desktop shell (iced) and its CLI subcommands. Running
//! with no subcommand launches the GUI (`ui::run`); every subcommand below
//! keeps working exactly as it did before the shell existed.

use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context as _, bail};
use clap::{Parser, Subcommand};
use gstreamer::prelude::*;
use streamboat_core::auth::device_code::{start_device_flow, wait_for_device_token};
use streamboat_core::auth::pkce::{PkceSession, capture_code_loopback, code_from_redirect};
use streamboat_core::bootstrap::Context;
use streamboat_core::proto::{Command, Event, OutputConfig, PlayItem};
use streamboat_core::{AudioQuality, StreamSource};
use streamboat_player::{GstEngine, Player, PlayerConfig, PlayerDeps};

mod ui;

#[derive(Parser)]
#[command(
    name = "streamboat",
    version,
    about = "An open-source TIDAL client for subscribers"
)]
struct Cli {
    /// No subcommand launches the iced desktop shell.
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Log in. Device-code flow by default (works on a headless box);
    /// --pkce uses the browser flow that unlocks hi-res.
    Login {
        /// Authorization-code flow with PKCE (desktop default per D-024).
        #[arg(long)]
        pkce: bool,
        /// How to get the code back from the browser: paste the redirected
        /// URL, or listen on 127.0.0.1:<port>/callback (needs a client id
        /// that allows a loopback redirect; unverified for the ecosystem id).
        #[arg(long, value_enum, default_value_t = Capture::Paste, requires = "pkce")]
        capture: Capture,
        #[arg(long, default_value_t = 17893, requires = "pkce")]
        port: u16,
        /// Override the redirect URI (default: TIDAL's own Android callback).
        #[arg(long, requires = "pkce")]
        redirect_uri: Option<String>,
        /// Do not try to open the browser automatically.
        #[arg(long)]
        no_open: bool,
    },
    /// Where the token-file master key lives, and moving it.
    Keyring {
        #[command(subcommand)]
        action: KeyringAction,
    },
    /// Forget the stored tokens.
    Logout,
    /// Show the session TIDAL reports for the stored login.
    Whoami,
    /// Search tracks.
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },
    /// Resolve a track's stream without playing it.
    Resolve {
        track_id: u64,
        #[arg(long)]
        quality: Option<AudioQuality>,
    },
    /// Play one or more tracks from the CLI.
    Play {
        /// Track ids (numbers from a TIDAL URL such as tidal.com/track/12345).
        track_ids: Vec<u64>,
        /// Play the first search result instead of an id.
        #[arg(long, conflicts_with = "track_ids")]
        search: Option<String>,
        #[arg(long)]
        quality: Option<AudioQuality>,
        /// ALSA device (for example `hw:1,0`); shared mode unless --exclusive.
        #[arg(long)]
        device: Option<String>,
        /// Open the device exclusively (bit-perfect: no mixing, no volume).
        #[arg(long, requires = "device")]
        exclusive: bool,
        #[arg(long, default_value_t = 1.0)]
        volume: f32,
    },
    /// List audio output devices GStreamer can see.
    Devices,
    /// Show where settings, tokens and caches live.
    Paths,
    /// Open a TIDAL/streamboat content link in the desktop shell (task item
    /// 5) — `tidal.com`, `listen.tidal.com`, `tidal://` or `streamboat://`.
    /// A bare link as `streamboat`'s only argument is rewritten into this
    /// subcommand by [`main`] before `clap` ever sees it, so both `streamboat
    /// open <url>` and `streamboat <url>` work.
    Open { url: String },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Capture {
    Paste,
    Loopback,
}

#[derive(Subcommand)]
enum KeyringAction {
    /// Show where the master key is stored.
    Status,
    /// Move a file-held key into the OS keyring and delete the file.
    Migrate,
    /// Move a keyring-held key into a 0600 key file and delete the entry.
    ToFile,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    // Task item 5: `streamboat <url>` (no subcommand) is sugar for
    // `streamboat open <url>` — rewritten here, before `clap` parses
    // anything, so a bare link doesn't need to match a subcommand name.
    let mut args: Vec<String> = std::env::args().collect();
    if let Some(first) = args.get(1) {
        if looks_like_content_link(first) {
            let url = args.remove(1);
            args.insert(1, "open".to_string());
            args.insert(2, url);
        }
    }
    let cli = Cli::parse_from(args);
    let Some(cmd) = cli.cmd else {
        return ui::run(None);
    };
    if let Cmd::Open { url } = cmd {
        // The GUI's own event loop is blocking and builds its own tokio
        // runtime (`ui::run`'s `player_rt`) — this must not also run inside
        // `rt.block_on` below, same as the no-subcommand path above.
        return ui::run(Some(url));
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(run(cmd))
}

/// Whether `arg` looks like a TIDAL/streamboat content link rather than an
/// ordinary CLI argument — the four schemes/hosts
/// `streamboat_core::api::images::parse_content_link` understands.
fn looks_like_content_link(arg: &str) -> bool {
    arg.starts_with("tidal://")
        || arg.starts_with("streamboat://")
        || arg.starts_with("https://tidal.com/")
        || arg.starts_with("https://listen.tidal.com/")
}

async fn run(cmd: Cmd) -> anyhow::Result<()> {
    match cmd {
        // Handled directly in `main()`, before this function is ever
        // called, since opening a link launches the GUI (`ui::run`), which
        // is blocking and builds its own tokio runtime — it cannot run
        // inside the `rt.block_on(run(cmd))` every other subcommand shares.
        Cmd::Open { .. } => unreachable!("Cmd::Open is intercepted in main() before run()"),
        Cmd::Paths => {
            let ctx = Context::load();
            let dirs = streamboat_core::config::AppDirs::resolve()?;
            println!("config:   {}", dirs.config.display());
            println!("data:     {}", dirs.data.display());
            println!("cache:    {}", dirs.cache.display());
            println!("runtime:  {}", dirs.runtime.display());
            println!("settings: {}", dirs.settings_path().display());
            println!("tokens:   {}", dirs.token_path().display());
            match ctx {
                Ok(ctx) => {
                    let c = ctx.api.credentials();
                    println!(
                        "client ids: device {} / pkce {} (from {:?})",
                        c.device.as_ref().map(|p| p.id.as_str()).unwrap_or("-"),
                        c.pkce.as_ref().map(|p| p.id.as_str()).unwrap_or("-"),
                        c.source
                    );
                    println!("master key: {}", ctx.store.key_location());
                }
                Err(e) => println!("credentials: {e}"),
            }
            Ok(())
        }
        Cmd::Login {
            pkce,
            capture,
            port,
            redirect_uri,
            no_open,
        } => {
            let ctx = Context::load()?;
            if ctx.api.is_logged_in().await {
                if let Ok(s) = ctx.api.session().await {
                    println!(
                        "already logged in (user {}, country {}); run `streamboat logout` first to switch",
                        s.user_id.unwrap_or(0),
                        s.country_code
                    );
                    return Ok(());
                }
            }
            let tokens = if pkce {
                let redirect = match (capture, redirect_uri.as_deref()) {
                    (_, Some(r)) => Some(r.to_string()),
                    (Capture::Loopback, None) => Some(format!("http://127.0.0.1:{port}/callback")),
                    (Capture::Paste, None) => ctx.settings.pkce_redirect_uri.clone(),
                };
                let session = PkceSession::start(
                    &ctx.api,
                    &ctx.device.client_unique_key,
                    redirect.as_deref(),
                )?;
                println!("Open this URL in a browser and log in to TIDAL:\n");
                println!("    {}\n", session.authorize_url);
                if !no_open {
                    open_browser(&session.authorize_url);
                }
                let code = match capture {
                    Capture::Paste => {
                        println!(
                            "After logging in the browser lands on a TIDAL page that says \"Oops\" at\n    {}\nCopy the full URL from the address bar and paste it here:",
                            session.redirect_uri
                        );
                        let line = read_line().await?;
                        code_from_redirect(&line)?
                    }
                    Capture::Loopback => {
                        println!(
                            "Waiting for the browser to come back to 127.0.0.1:{port}/callback ..."
                        );
                        capture_code_loopback(
                            ([127, 0, 0, 1], port).into(),
                            "/callback",
                            Duration::from_secs(600),
                        )
                        .await?
                    }
                };
                session.exchange(&ctx.api, &code).await?
            } else {
                let auth = start_device_flow(&ctx.api).await?;
                println!("Open this URL and enter the code to log in:\n");
                println!("    {}", auth.verification_url());
                println!("    code: {}\n", auth.user_code);
                if !no_open {
                    open_browser(&auth.verification_url());
                }
                println!(
                    "Waiting for you to finish in the browser (expires in {} s)...",
                    auth.expires_in
                );
                wait_for_device_token(&ctx.api, &auth, || true).await?
            };
            let s = ctx.api.session().await.ok();
            println!(
                "Logged in as user {} ({}) with the {} flow; tokens in {}, key in {}.",
                tokens
                    .user_id
                    .or(s.as_ref().and_then(|s| s.user_id))
                    .unwrap_or(0),
                s.map(|s| s.country_code)
                    .or(tokens.country_code)
                    .unwrap_or_default(),
                match tokens.flow {
                    streamboat_core::AuthFlow::Pkce => "PKCE",
                    streamboat_core::AuthFlow::DeviceCode => "device-code",
                },
                ctx.dirs.token_path().display(),
                ctx.store.key_location()
            );
            Ok(())
        }
        Cmd::Keyring { action } => {
            let ctx = Context::load()?;
            match action {
                KeyringAction::Status => {
                    println!("key location: {}", ctx.store.key_location());
                    println!(
                        "key_storage:  {:?} (settings.json)",
                        ctx.settings.key_storage
                    );
                }
                KeyringAction::Migrate => {
                    let loc = ctx.store.migrate_key_to_keyring()?;
                    println!("key now in {loc}");
                }
                KeyringAction::ToFile => {
                    let loc = ctx.store.migrate_key_to_file()?;
                    println!("key now in {loc}");
                }
            }
            Ok(())
        }
        Cmd::Logout => {
            let ctx = Context::load()?;
            ctx.api.logout().await?;
            println!("Tokens removed from {}.", ctx.dirs.token_path().display());
            Ok(())
        }
        Cmd::Whoami => {
            let ctx = Context::load()?;
            let s = ctx.api.session().await?;
            println!("user id:     {}", s.user_id.unwrap_or(0));
            println!("country:     {}", s.country_code);
            println!("session id:  {}", s.session_id.unwrap_or_default());
            if let Some(c) = s.client {
                println!("client:      {} (id {})", c.name, c.id.unwrap_or(0));
            }
            println!("user agent:  {}", ctx.api.user_agent());
            if let Some(t) = ctx.api.tokens().await {
                println!("login flow:  {:?} (client id {})", t.flow, t.client_id);
            }
            println!("master key:  {}", ctx.store.key_location());
            Ok(())
        }
        Cmd::Search { query, limit } => {
            let ctx = Context::load()?;
            let page = ctx.api.search_tracks(&query, limit).await?;
            for t in &page.items {
                println!(
                    "{:>12}  {} — {}  [{}]{}",
                    t.id,
                    t.artist_names(),
                    t.title,
                    t.album.as_ref().map(|a| a.title.as_str()).unwrap_or(""),
                    if t.has_hires_master() { "  hi-res" } else { "" }
                );
            }
            Ok(())
        }
        Cmd::Resolve { track_id, quality } => {
            let ctx = Context::load()?;
            let ceiling = quality.unwrap_or_else(|| ctx.settings.quality_ceiling());
            let sid = uuid::Uuid::new_v4().to_string();
            let r = ctx.api.resolve_stream(track_id, ceiling, &sid).await?;
            println!("track:        {}", r.track_id);
            println!("requested:    {}", r.requested);
            println!(
                "delivered:    {}",
                r.info
                    .quality
                    .map(|q| q.to_string())
                    .unwrap_or_else(|| "?".into())
            );
            println!("manifest:     {}", r.info.manifest_kind);
            println!(
                "codec:        {}",
                r.info.codec.clone().unwrap_or_else(|| "?".into())
            );
            println!(
                "format:       {} Hz / {}-bit",
                r.info
                    .sample_rate
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "?".into()),
                r.info
                    .bit_depth
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "?".into())
            );
            println!(
                "replaygain:   {:?} dB, peak {:?}",
                r.info.replay_gain_db, r.info.peak_amplitude
            );
            match &r.source {
                StreamSource::Url(u) => println!("source:       direct URL {}", redact_url(u)),
                StreamSource::DashMpd(x) => println!("source:       DASH MPD ({} bytes)", x.len()),
            }
            for w in &r.warnings {
                println!("warning:      {w}");
            }
            Ok(())
        }
        Cmd::Play {
            track_ids,
            search,
            quality,
            device,
            exclusive,
            volume,
        } => {
            let ctx = Context::load()?;
            let ids = if let Some(q) = search {
                let page = ctx.api.search_tracks(&q, 1).await?;
                let Some(t) = page.items.first() else {
                    bail!("no results for {q:?}")
                };
                println!("playing {} — {} ({})", t.artist_names(), t.title, t.id);
                vec![t.id]
            } else if track_ids.is_empty() {
                bail!("give at least one track id, or --search");
            } else {
                track_ids
            };
            let output = match (device, exclusive) {
                (Some(d), true) => OutputConfig::Exclusive { device: d },
                (d, false) => OutputConfig::Shared { device: d },
                (None, true) => unreachable!("clap requires --device with --exclusive"),
            };
            let (tx, rx) = mpsc::channel();
            let engine = GstEngine::new(tx, output.clone(), ctx.dirs.runtime.clone())
                .context("starting the audio engine")?;
            let cfg = PlayerConfig {
                quality_ceiling: quality.unwrap_or_else(|| ctx.settings.quality_ceiling()),
                output,
                volume,
            };
            let deps = PlayerDeps::for_context(&ctx)
                .context("wiring play reporting, scrobbling and streaming privileges")?;
            let handle = Player::spawn(ctx.api.clone(), Box::new(engine), rx, cfg, deps);
            let mut events = handle.subscribe();
            handle.send(Command::Play {
                items: ids
                    .into_iter()
                    .map(|track_id| PlayItem { track_id })
                    .collect(),
            });
            let ctrl_c = tokio::signal::ctrl_c();
            tokio::pin!(ctrl_c);
            let mut last_line_len = 0usize;
            loop {
                tokio::select! {
                    _ = &mut ctrl_c => {
                        eprintln!("\nstopping");
                        handle.send(Command::Stop);
                        handle.send(Command::Shutdown);
                        break;
                    }
                    ev = events.recv() => {
                        let Ok(ev) = ev else { break };
                        match ev {
                            Event::Position { position_ms, duration_ms } => {
                                let line = format!("  {} / {}", mmss(position_ms), duration_ms.map(mmss).unwrap_or_else(|| "--:--".into()));
                                eprint!("\r{:<width$}", line, width = last_line_len.max(line.len()));
                                last_line_len = line.len();
                                let _ = std::io::stderr().flush();
                            }
                            Event::TrackStarted { track, stream, index } => {
                                eprintln!(
                                    "\n▶ [{}] {} — {}  ({} {} {} Hz/{}-bit{})",
                                    index + 1,
                                    track.artists,
                                    track.title,
                                    stream.quality.map(|q| q.to_string()).unwrap_or_default(),
                                    stream.codec.clone().unwrap_or_default(),
                                    stream.sample_rate.map(|v| v.to_string()).unwrap_or_else(|| "?".into()),
                                    stream.bit_depth.map(|v| v.to_string()).unwrap_or_else(|| "?".into()),
                                    if stream.preview { ", PREVIEW" } else { "" }
                                );
                            }
                            Event::State { state } => {
                                if let Some(sp) = state.signal_path.filter(|_| state.status == streamboat_core::proto::PlaybackStatus::Playing) {
                                    tracing::info!(?sp, "signal path");
                                }
                            }
                            Event::Warning { message } => eprintln!("\nwarning: {message}"),
                            Event::Error { message, .. } => eprintln!("\nerror: {message}"),
                            Event::Buffering { percent } => { let _ = percent; }
                            Event::EndOfQueue | Event::Stopped => {
                                eprintln!("\ndone");
                                handle.send(Command::Shutdown);
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(())
        }
        Cmd::Devices => {
            gstreamer::init()?;
            let monitor = gstreamer::DeviceMonitor::new();
            monitor.add_filter(Some("Audio/Sink"), None);
            monitor
                .start()
                .map_err(|e| anyhow::anyhow!("device monitor: {e}"))?;
            for d in monitor.devices() {
                let props = d.properties().map(|p| p.to_string()).unwrap_or_default();
                println!(
                    "{}  [{}]\n    {}",
                    d.display_name(),
                    d.device_class(),
                    props
                );
            }
            monitor.stop();
            if let Ok(cards) = std::fs::read_to_string("/proc/asound/cards") {
                println!("\nALSA cards (use hw:<n>,0 with --device):");
                for line in cards
                    .lines()
                    .filter(|l| l.trim_start().starts_with(|c: char| c.is_ascii_digit()))
                {
                    println!("  {}", line.trim());
                }
            }
            Ok(())
        }
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = ("open", vec![url]);
    #[cfg(target_os = "windows")]
    let cmd = ("cmd", vec!["/C", "start", "", url]);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let cmd = ("xdg-open", vec![url]);
    match std::process::Command::new(cmd.0)
        .args(&cmd.1)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => {}
        Err(_) => eprintln!("(could not open a browser automatically; open the URL yourself)"),
    }
}

async fn read_line() -> anyhow::Result<String> {
    use tokio::io::AsyncBufReadExt;
    let mut line = String::new();
    tokio::io::BufReader::new(tokio::io::stdin())
        .read_line(&mut line)
        .await?;
    Ok(line)
}

fn mmss(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn redact_url(u: &str) -> String {
    match url_host_and_path(u) {
        Some((host, path)) => format!("https://{host}{path}?<signed>"),
        None => "<url>".into(),
    }
}

fn url_host_and_path(u: &str) -> Option<(String, String)> {
    let rest = u.split("://").nth(1)?;
    let (hostpath, _) = rest.split_once('?').unwrap_or((rest, ""));
    let (host, path) = hostpath.split_once('/').unwrap_or((hostpath, ""));
    let mut p = format!("/{path}");
    if p.len() > 40 {
        p.truncate(40);
        p.push('…');
    }
    Some((host.to_string(), p))
}
