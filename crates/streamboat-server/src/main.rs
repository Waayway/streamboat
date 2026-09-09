//! `streamboatd`: the headless daemon. This spike speaks the Command/Event
//! protocol as JSON lines on stdin/stdout, which is the same contract the
//! HTTP+WebSocket control API (D-030) will carry next. It links no GUI
//! toolkit; CI builds it in a container without one to keep it that way.

use std::sync::Arc;
use std::sync::mpsc;

use anyhow::Context as _;
use clap::Parser;
use streamboat_core::AudioQuality;
use streamboat_core::auth::device_code::{start_device_flow, wait_for_device_token};
use streamboat_core::bootstrap::Context;
use streamboat_core::privileges::{StreamingPrivileges, hostname_display_name};
use streamboat_core::proto::{Command, Event, OutputConfig};
use streamboat_core::reporting::PlayReporter;
use streamboat_core::scrobble::{LastfmScrobbler, ListenBrainzScrobbler, ScrobbleHub, Scrobbler};
use streamboat_player::{GstEngine, Player, PlayerConfig, PlayerDeps};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

#[derive(Parser)]
#[command(
    name = "streamboatd",
    version,
    about = "streamboat headless daemon (stdio protocol)"
)]
struct Cli {
    #[arg(long)]
    quality: Option<AudioQuality>,
    /// ALSA device, for example `hw:1,0`.
    #[arg(long)]
    device: Option<String>,
    #[arg(long, requires = "device")]
    exclusive: bool,
    #[arg(long, default_value_t = 1.0)]
    volume: f32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let ctx = Context::load()?;
    let output = match (cli.device, cli.exclusive) {
        (Some(d), true) => OutputConfig::Exclusive { device: d },
        (d, _) => OutputConfig::Shared { device: d },
    };
    let (tx, rx) = mpsc::channel();
    let engine = GstEngine::new(tx, output.clone(), ctx.dirs.runtime.clone())?;
    let cfg = PlayerConfig {
        quality_ceiling: cli
            .quality
            .unwrap_or_else(|| ctx.settings.quality_ceiling()),
        output,
        volume: cli.volume,
    };
    // Play reporting (D-027): on by default, from `Settings::play_reporting`.
    let reporter = Arc::new(
        PlayReporter::open(
            ctx.api.clone(),
            ctx.dirs.data.join("play_reports.json"),
            ctx.settings.play_reporting,
        )
        .context("opening the play-reporting outbox")?,
    );

    // Scrobbling (D-037): only the backends with both `enabled` and a full
    // credential set actually exist.
    let mut scrobble_backends: Vec<Arc<dyn Scrobbler>> = Vec::new();
    if let Some(lastfm) = LastfmScrobbler::open(
        &ctx.settings.scrobble.lastfm,
        ctx.dirs.data.join("scrobble_lastfm.json"),
        ctx.api.user_agent(),
    )
    .context("opening the last.fm scrobble queue")?
    {
        scrobble_backends.push(Arc::new(lastfm));
    }
    if let Some(listenbrainz) = ListenBrainzScrobbler::open(
        &ctx.settings.scrobble.listenbrainz,
        ctx.dirs.data.join("scrobble_listenbrainz.json"),
        ctx.api.user_agent(),
    )
    .context("opening the listenbrainz scrobble queue")?
    {
        scrobble_backends.push(Arc::new(listenbrainz));
    }
    let scrobbler: Option<Arc<dyn Scrobbler>> = if scrobble_backends.is_empty() {
        None
    } else {
        Some(Arc::new(ScrobbleHub::new(scrobble_backends)))
    };

    // The pinned, encrypted offline cache (D-022): unavailable (e.g. no
    // keyring reachable and `key_storage = keyring`) means stream-only,
    // logged and never fatal to starting the daemon.
    let offline = match streamboat_player::offline::OfflineCache::open(
        &ctx.dirs,
        &ctx.settings,
        &ctx.device.client_unique_key,
    )
    .await
    {
        Ok(cache) => Some(cache),
        Err(e) => {
            tracing::warn!(%e, "offline cache unavailable; streaming only");
            None
        }
    };

    // Streaming privileges ("Pushkin", D-033): a headless daemon needs this
    // from day one (`headless-and-tidal-connect` daemon-architecture.md
    // §6) — with no user watching, a silent revocation is unrecoverable.
    let (privileges, privileges_events) =
        StreamingPrivileges::spawn(ctx.api.clone(), hostname_display_name());
    let privileges = Arc::new(privileges);

    let handle = Player::spawn(
        ctx.api.clone(),
        Box::new(engine),
        rx,
        cfg,
        PlayerDeps {
            privileges: Some(privileges.clone()),
            privileges_events: Some(privileges_events),
            reporter: Some(reporter),
            scrobbler,
            offline,
        },
    );
    let mut events = handle.subscribe();

    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    // Headless login: announce the device code as an event so a remote can
    // render its own QR, then confirm.
    if !ctx.api.is_logged_in().await {
        let api = ctx.api.clone();
        let out = out_tx.clone();
        let privileges = privileges.clone();
        tokio::spawn(async move {
            match start_device_flow(&api).await {
                Ok(auth) => {
                    let _ = out.send(Event::AuthRequired {
                        verification_url: auth.verification_url(),
                        user_code: auth.user_code.clone(),
                        expires_in_secs: auth.expires_in,
                    });
                    match wait_for_device_token(&api, &auth, || true).await {
                        Ok(t) => {
                            let _ = out.send(Event::AuthOk {
                                user_id: t.user_id,
                                country_code: t.country_code,
                            });
                            // The privileges socket was already retrying
                            // against no token; kick it into an immediate
                            // reconnect now that one exists (§6 item 7).
                            privileges.notify_token_refreshed();
                        }
                        Err(e) => {
                            let _ = out.send(Event::Error {
                                message: format!("login failed: {e}"),
                                track_id: None,
                            });
                        }
                    }
                }
                Err(e) => {
                    let _ = out.send(Event::Error {
                        message: format!("login failed: {e}"),
                        track_id: None,
                    });
                }
            }
        });
    }

    let mut stdout = tokio::io::stdout();
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    handle.send(Command::GetState);
    loop {
        tokio::select! {
            line = lines.next_line() => match line? {
                None => { handle.send(Command::Shutdown); break; }
                Some(l) if l.trim().is_empty() => {}
                Some(l) => match serde_json::from_str::<Command>(&l) {
                    Ok(Command::Shutdown) => { handle.send(Command::Shutdown); break; }
                    Ok(cmd) => { handle.send(cmd); }
                    Err(e) => {
                        let ev = Event::Warning { message: format!("unparseable command: {e}") };
                        write_event(&mut stdout, &ev).await?;
                    }
                },
            },
            ev = events.recv() => match ev {
                Ok(ev) => write_event(&mut stdout, &ev).await?,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("dropped {n} events for a slow reader");
                }
                Err(_) => break,
            },
            Some(ev) = out_rx.recv() => write_event(&mut stdout, &ev).await?,
        }
    }
    Ok(())
}

async fn write_event(out: &mut tokio::io::Stdout, ev: &Event) -> anyhow::Result<()> {
    let mut line = serde_json::to_string(ev)?;
    line.push('\n');
    out.write_all(line.as_bytes()).await?;
    out.flush().await?;
    Ok(())
}
