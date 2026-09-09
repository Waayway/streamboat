//! `streamboatd`: the headless daemon. This spike speaks the Command/Event
//! protocol as JSON lines on stdin/stdout, which is the same contract the
//! HTTP+WebSocket control API (D-030) will carry next. It links no GUI
//! toolkit; CI builds it in a container without one to keep it that way.

use std::sync::mpsc;

use clap::Parser;
use streamboat_core::AudioQuality;
use streamboat_core::auth::device_code::{start_device_flow, wait_for_device_token};
use streamboat_core::bootstrap::Context;
use streamboat_core::proto::{Command, Event, OutputConfig};
use streamboat_player::{GstEngine, Player, PlayerConfig};
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
    let handle = Player::spawn(ctx.api.clone(), Box::new(engine), rx, cfg);
    let mut events = handle.subscribe();

    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    // Headless login: announce the device code as an event so a remote can
    // render its own QR, then confirm.
    if !ctx.api.is_logged_in().await {
        let api = ctx.api.clone();
        let out = out_tx.clone();
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
