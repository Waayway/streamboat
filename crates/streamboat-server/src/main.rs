//! `streamboatd`: the headless daemon. Two front doors over the same
//! Command/Event protocol (`streamboat_core::proto`): by default, the
//! HTTP + WebSocket control API (D-030, D-031, `streamboat_server::api`);
//! with `--stdio`, the JSON-lines transport the playable spike shipped
//! first. Links no GUI toolkit; CI builds it in a container without one to
//! keep it that way.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, mpsc};

use anyhow::Context as _;
use clap::Parser;
use streamboat_core::AudioQuality;
use streamboat_core::auth::device_code::{start_device_flow, wait_for_device_token};
use streamboat_core::bootstrap::Context;
use streamboat_core::privileges::StreamingPrivileges;
use streamboat_core::proto::{Command, Event, OutputConfig};
use streamboat_player::{Player, PlayerConfig, PlayerDeps, PlayerHandle, default_engine};
use streamboat_server::api::{self, ApiState};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

/// Unclaimed by any control-API precedent this project surveyed
/// (`headless-and-tidal-connect/references/daemon-architecture.md` §2); not
/// an owner decision, just streamboat's own pick. Override with `--listen`.
const DEFAULT_PORT: u16 = 4747;

#[derive(Parser)]
#[command(
    name = "streamboatd",
    version,
    about = "streamboat headless daemon (control API, MPRIS, streaming privileges)"
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
    /// Speak the legacy JSON-lines protocol on stdin/stdout instead of
    /// hosting the HTTP + WebSocket control API.
    #[arg(long)]
    stdio: bool,
    /// Address for the control API. Defaults to `127.0.0.1:4747`. Anything
    /// other than a loopback address is a LAN opt-in (see `--lan`) and is
    /// logged loudly, with the token file's path, at startup.
    #[arg(long)]
    listen: Option<SocketAddr>,
    /// Acknowledge exposing the control API beyond localhost (D-030). If
    /// `--listen` is not also given, this switches the bind address to
    /// `0.0.0.0` on the default port.
    #[arg(long)]
    lan: bool,
    /// Extra `Host` header values the control API accepts, beyond
    /// `localhost`/`127.0.0.1`/`::1` and (once bound beyond localhost) the
    /// literal bind address — for a LAN hostname or reverse-proxy name.
    /// Repeatable.
    #[arg(long = "allow-host")]
    allow_host: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // D-029: redacted, rotating file logging plus a panic hook writing to
    // `<data dir>/crashes/` — installed before anything else can log or
    // panic. Same stderr verbosity default ("info") as before this existed;
    // falls back to the old stderr-only setup if `AppDirs` cannot resolve.
    let _diagnostics = match streamboat_core::config::AppDirs::resolve() {
        Ok(dirs) => Some(streamboat_core::diagnostics::init(
            &dirs.data,
            "streamboatd",
            env!("CARGO_PKG_VERSION"),
            "info",
        )),
        Err(_) => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .with_writer(std::io::stderr)
                .try_init();
            None
        }
    };
    let cli = Cli::parse();
    let ctx = Context::load()?;

    // Single-instance lock (D-010, D-045): `streamboatd` and `streamboat`
    // (the GUI) share one lock file so only one of them is ever "the
    // instance" driving an engine at a time. Refuse to start rather than
    // silently running two daemons against the same account/device.
    let lock_path = ctx.dirs.instance_lock_path();
    let _instance_lock = match streamboat_core::instance_lock::InstanceLock::try_acquire(&lock_path)
    {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            let holder = streamboat_core::instance_lock::read_control_address(
                &ctx.dirs.control_address_path(),
            );
            anyhow::bail!(
                "another streamboat instance already holds {}{}; stop it first (or, if it's a \
                 GUI, this is normal — a daemon and a GUI never run side by side against the \
                 same account/device)",
                lock_path.display(),
                holder
                    .map(|a| format!(" (control API at {a})"))
                    .unwrap_or_default()
            );
        }
        Err(e) => return Err(e.into()),
    };

    let output = match (cli.device.clone(), cli.exclusive) {
        (Some(d), true) => OutputConfig::Exclusive { device: d },
        (d, _) => OutputConfig::Shared { device: d },
    };
    let (tx, rx) = mpsc::channel();
    // Whichever backend this build actually compiled, preferring D-016's
    // platform pick (`streamboat_player::platform`'s module doc comment).
    let engine = default_engine(tx, output.clone(), ctx.dirs.runtime.clone())?;
    let cfg = PlayerConfig {
        quality_ceiling: cli
            .quality
            .unwrap_or_else(|| ctx.settings.quality_ceiling()),
        output,
        volume: cli.volume,
        replay_gain_mode: ctx.settings.replay_gain_mode,
    };
    // Play reporting, scrobbling and streaming privileges (D-027, D-037,
    // D-033) — the same wiring the desktop shell uses. A headless daemon
    // needs the privileges socket from day one
    // (`headless-and-tidal-connect` daemon-architecture.md §6): with no user
    // watching, a silent revocation is unrecoverable.
    let deps = PlayerDeps::for_context(&ctx)
        .await
        .context("wiring play reporting, scrobbling and streaming privileges")?;
    let privileges = deps
        .privileges
        .clone()
        .expect("PlayerDeps::for_context always creates the privileges client");

    // `default_engine` already returns `Box<dyn Engine>`.
    let handle = Player::spawn(ctx.api.clone(), engine, rx, cfg, deps);

    // OS media-key integration (MPRIS/SMTC/NowPlaying, D-030) lives in the
    // engine/player layer so it works the same way in daemon mode; whichever
    // adapter this build compiled for this target logs and does nothing
    // useful when there is no D-Bus session/WinRT runtime/run loop (a
    // headless box, this CI), never failing the daemon.
    streamboat_player::media_controls::spawn(handle.clone());

    if cli.stdio {
        // Subscribe before kicking off headless login: `PlayerHandle::publish`
        // (used for `AuthRequired`/`AuthOk` below) only reaches subscribers
        // that already exist at send time, so this order is load-bearing,
        // not cosmetic.
        let events = handle.subscribe();
        spawn_login(&ctx, &handle, privileges.clone());
        run_stdio(handle, events).await
    } else {
        let (state, addr) =
            control_api_state(&ctx, handle.clone(), cli.listen, cli.lan, cli.allow_host)?;
        spawn_login(&ctx, &handle, privileges.clone());
        let app = api::router(state);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        // Log the address actually bound, not the requested one — they
        // differ whenever `--listen` asks for port 0.
        let bound = listener.local_addr()?;
        tracing::info!(listen = %bound, "control API listening");
        // D-010's discovery half: a second process (typically the desktop
        // GUI, per `ui::instance` in `streamboat-desktop`) that fails to
        // take the instance lock reads this file to become a remote client
        // of this control API instead of doing nothing useful.
        let address_path = ctx.dirs.control_address_path();
        if let Err(e) = streamboat_core::instance_lock::write_control_address(&address_path, bound)
        {
            tracing::warn!(
                error = %e,
                path = %address_path.display(),
                "could not write the control-address file; a remote GUI client will not find this daemon"
            );
        }
        axum::serve(listener, app).await?;
        Ok(())
    }
}

/// Resolve the bind address, load or create the bearer token, build the
/// Host-header allowlist, and construct [`ApiState`] — which subscribes to
/// the player's event broadcast synchronously, before returning, for the
/// same reason `main` orders things the way it does above.
fn control_api_state(
    ctx: &Context,
    handle: PlayerHandle,
    listen: Option<SocketAddr>,
    lan: bool,
    extra_hosts: Vec<String>,
) -> anyhow::Result<(ApiState, SocketAddr)> {
    let mut addr = listen.unwrap_or_else(|| SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_PORT)));
    if lan && addr.ip().is_loopback() {
        addr.set_ip(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }

    let token_path = ctx.dirs.control_token_path();
    let token = streamboat_core::config::load_or_create_control_token(&token_path)?;
    tracing::info!(path = %token_path.display(), "control API bearer token");

    let mut allowed_hosts = api::default_allowed_hosts();
    if !addr.ip().is_loopback() {
        allowed_hosts.push(addr.ip().to_string());
        tracing::warn!(
            listen = %addr,
            token_path = %token_path.display(),
            "control API is bound beyond localhost: anyone on this network who obtains the \
             token at the path above can control playback. Pass --allow-host to also accept a \
             LAN hostname or reverse-proxy name."
        );
    }
    allowed_hosts.extend(extra_hosts);

    Ok((ApiState::new(handle, token, allowed_hosts), addr))
}

/// Headless login (unchanged from the stdio-only spike): announce the
/// device code as an `Event`, on the same broadcast every front end reads
/// from, so a remote can render its own QR.
fn spawn_login(ctx: &Context, handle: &PlayerHandle, privileges: Arc<StreamingPrivileges>) {
    let api = ctx.api.clone();
    let login_handle = handle.clone();
    tokio::spawn(async move {
        if api.is_logged_in().await {
            return;
        }
        match start_device_flow(&api).await {
            Ok(auth) => {
                login_handle.publish(Event::AuthRequired {
                    verification_url: auth.verification_url(),
                    user_code: auth.user_code.clone(),
                    expires_in_secs: auth.expires_in,
                });
                match wait_for_device_token(&api, &auth, || true).await {
                    Ok(t) => {
                        login_handle.publish(Event::AuthOk {
                            user_id: t.user_id,
                            country_code: t.country_code,
                        });
                        // The privileges socket was already retrying against
                        // no token; kick it into an immediate reconnect now
                        // that one exists (daemon-architecture.md §6 item 7).
                        privileges.notify_token_refreshed();
                    }
                    Err(e) => login_handle.publish(Event::Error {
                        message: format!("login failed: {e}"),
                        track_id: None,
                    }),
                }
            }
            Err(e) => login_handle.publish(Event::Error {
                message: format!("login failed: {e}"),
                track_id: None,
            }),
        }
    });
}

async fn run_stdio(
    handle: PlayerHandle,
    mut events: broadcast::Receiver<Event>,
) -> anyhow::Result<()> {
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
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("dropped {n} events for a slow reader");
                }
                Err(_) => break,
            },
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
