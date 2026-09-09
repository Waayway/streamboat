//! TIDAL's streaming-privileges websocket ("Pushkin"): TIDAL allows exactly
//! one concurrent, privileged (playing) stream per account, enforced over a
//! websocket every official client connects to (D-033).
//!
//! Wire format: `tidal-api/references/play-logging-and-privileges.md` §5 and,
//! in full, `headless-and-tidal-connect/references/daemon-architecture.md`
//! §6, whose ten numbered lessons this implementation follows point for
//! point. The two that matter most because they are exactly what the
//! reference SDK gets wrong for a long-running process:
//!
//! - §6 item 6: the browser SDK reconnects immediately and unconditionally
//!   on every socket close, re-`POST`ing `/rt/connect` every time — copied
//!   verbatim, a daemon offline for months would hot-loop that call. This
//!   client backs off exponentially, with jitter, capped at [`MAX_DELAY`].
//! - §6 item 10: on `PRIVILEGED_SESSION_NOTIFICATION` the reference client
//!   pauses and does **not** try to re-acquire. [`StreamingPrivileges::claim`]
//!   is therefore never called by this module itself — only the player, and
//!   only on genuine user intent (D-033), calls it.
//!
//! Also followed: §6 item 9 (this socket only exists for a real user session,
//! never a client-credentials one — callers must not spawn this before
//! login) and §6 item 7 (the socket URL is token-bound; a bearer-token swap
//! alone does not reach an already-open connection, so a refresh must call
//! [`StreamingPrivileges::notify_token_refreshed`] explicitly).

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::error::{Error, Result};
use crate::http::ApiClient;
use crate::reporting::ServerClock;

/// What the player/UI needs to know about this socket's state. Carried on a
/// plain tokio `mpsc` channel (`streamboat_core::proto::Event` is the wire
/// protocol; a front end maps these onto it — see
/// `proto::Event::PlaybackTakenOver`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegesEvent {
    /// The socket is connected and ready to carry `claim()` calls.
    Connected,
    /// Another device claimed the account's one privileged stream.
    /// `client_display_name` is TIDAL's name for that device
    /// (`PRIVILEGED_SESSION_NOTIFICATION.payload.clientDisplayName`).
    /// Playback must be paused by the caller; this module never re-claims
    /// automatically (§6 item 10).
    Revoked { client_display_name: String },
    /// The server asked for a fresh connection (`{"type":"RECONNECT"}`).
    /// A reconnect is already under way by the time this is sent.
    Reconnect,
    /// The socket dropped (error or close) and a backed-off reconnect has
    /// been scheduled.
    Disconnected,
}

#[derive(Debug, Deserialize)]
struct ConnectResponse {
    url: String,
}

#[derive(Debug, Serialize)]
struct UserActionPayload {
    #[serde(rename = "startedAt")]
    started_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum Outgoing {
    #[serde(rename = "USER_ACTION")]
    UserAction { payload: UserActionPayload },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Incoming {
    #[serde(rename = "PRIVILEGED_SESSION_NOTIFICATION")]
    PrivilegedSessionNotification { payload: PrivilegedSessionPayload },
    #[serde(rename = "RECONNECT")]
    Reconnect,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, Default)]
struct PrivilegedSessionPayload {
    #[serde(rename = "clientDisplayName", default)]
    client_display_name: String,
}

enum Internal {
    Claim,
    TokenRefreshed,
    Shutdown,
}

/// Base retry delay before the first backoff step.
const BASE_DELAY: Duration = Duration::from_secs(1);
/// The backoff ceiling (D-033: "capped exponential backoff with jitter and a
/// ceiling" — never the reference SDK's unbounded immediate retry).
pub const MAX_DELAY: Duration = Duration::from_secs(60);

/// Exponential backoff with full jitter, capped at `cap`: `delay = uniform(0,
/// min(base * 2^attempt, cap))`. A pure function so the ceiling behaviour is
/// testable (see `tests/privileges.rs`) without waiting real time out.
pub fn backoff_delay(attempt: u32, base: Duration, cap: Duration) -> Duration {
    let shift = attempt.min(20); // 2^20 * 1s already dwarfs any sane cap
    let exp = base.checked_mul(1u32 << shift).unwrap_or(cap).min(cap);
    let millis = exp.as_millis().max(1) as u64;
    Duration::from_millis(rand::rng().random_range(0..=millis))
}

/// The client that keeps the Pushkin socket open, claims the privileged
/// stream on demand, and reconnects (with backoff) when it drops.
pub struct StreamingPrivileges {
    tx: mpsc::UnboundedSender<Internal>,
}

impl StreamingPrivileges {
    /// Spawn the background connection task and return the handle plus the
    /// event stream. `display_name` is what other devices see if this
    /// client ever holds the privileged stream — see
    /// [`hostname_display_name`] for the hostname-derived convention
    /// (D-033, `daemon-architecture.md` §6 item 4). Callers must only spawn
    /// this after a real user login (§6 item 9): there is no privileges
    /// socket for a client-credentials session.
    pub fn spawn(
        api: ApiClient,
        display_name: String,
    ) -> (Self, mpsc::UnboundedReceiver<PrivilegesEvent>) {
        let (itx, irx) = mpsc::unbounded_channel();
        let (etx, erx) = mpsc::unbounded_channel();
        tokio::spawn(run(api, display_name, irx, etx));
        (Self { tx: itx }, erx)
    }

    /// Send `USER_ACTION` on the open socket — "tell Pushkin a user action
    /// happened, so it can make good qualified guesses if you're the
    /// session with allowed playback" (the web SDK's own comment). Call
    /// this **only** when the player is starting playback because of
    /// genuine user intent (D-033): a `Play`, `Next`, `Previous` or
    /// `Resume` command the user issued. Never on autoplay hand-over, never
    /// on resume-after-buffering, and never automatically in response to a
    /// [`PrivilegesEvent::Revoked`] — doing any of those would silently
    /// steal playback from the user's other device.
    pub fn claim(&self) {
        let _ = self.tx.send(Internal::Claim);
    }

    /// The session's token was refreshed: reconnect using it. The socket
    /// URL from `rt/connect` is token-bound (§6 item 7); swapping the
    /// bearer token used elsewhere does not by itself reach an already-open
    /// connection.
    pub fn notify_token_refreshed(&self) {
        let _ = self.tx.send(Internal::TokenRefreshed);
    }
}

impl Drop for StreamingPrivileges {
    fn drop(&mut self) {
        let _ = self.tx.send(Internal::Shutdown);
    }
}

async fn connect_once(
    api: &ApiClient,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let (resp, _headers) = api
        .request_json::<ConnectResponse>(reqwest::Method::POST, "v1/rt/connect", &[], None, &[])
        .await?;
    let (socket, _response) = tokio_tungstenite::connect_async(resp.url)
        .await
        .map_err(|e| Error::Config(format!("rt/connect websocket: {e}")))?;
    Ok(socket)
}

async fn run(
    api: ApiClient,
    display_name: String,
    mut cmds: mpsc::UnboundedReceiver<Internal>,
    events: mpsc::UnboundedSender<PrivilegesEvent>,
) {
    // Kept for parity with `daemon-architecture.md` §6 item 4 (register a
    // sensible display name) — no separate outgoing "register" message is
    // documented; TIDAL surfaces this session's own name to *other*
    // sessions only once it holds the privileged stream, so there is
    // nothing to send proactively here.
    let _ = &display_name;
    let clock = ServerClock::new();
    let mut attempt: u32 = 0;
    'outer: loop {
        let mut socket = match connect_once(&api).await {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(error = %e, attempt, "rt/connect failed; backing off");
                let delay = backoff_delay(attempt, BASE_DELAY, MAX_DELAY);
                attempt = attempt.saturating_add(1);
                tokio::select! {
                    _ = tokio::time::sleep(delay) => continue 'outer,
                    cmd = cmds.recv() => match cmd {
                        None | Some(Internal::Shutdown) => return,
                        _ => continue 'outer,
                    },
                }
            }
        };
        attempt = 0;
        let _ = events.send(PrivilegesEvent::Connected);
        loop {
            tokio::select! {
                msg = socket.next() => match msg {
                    Some(Ok(Message::Text(txt))) => {
                        match serde_json::from_str::<Incoming>(&txt) {
                            Ok(Incoming::PrivilegedSessionNotification { payload }) => {
                                let _ = events.send(PrivilegesEvent::Revoked {
                                    client_display_name: payload.client_display_name,
                                });
                                // Hard stop (§6 item 10): no auto re-claim.
                            }
                            Ok(Incoming::Reconnect) => {
                                let _ = events.send(PrivilegesEvent::Reconnect);
                                continue 'outer;
                            }
                            Ok(Incoming::Unknown) => {}
                            Err(e) => tracing::debug!(error = %e, "unrecognised pushkin message"),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        let _ = events.send(PrivilegesEvent::Disconnected);
                        continue 'outer;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::debug!(error = %e, "pushkin socket error");
                        let _ = events.send(PrivilegesEvent::Disconnected);
                        continue 'outer;
                    }
                },
                cmd = cmds.recv() => match cmd {
                    None | Some(Internal::Shutdown) => return,
                    Some(Internal::Claim) => {
                        let started_at = clock.now_ms(&api).await;
                        let msg = Outgoing::UserAction {
                            payload: UserActionPayload { started_at },
                        };
                        if let Ok(txt) = serde_json::to_string(&msg) {
                            if socket.send(Message::Text(txt.into())).await.is_err() {
                                let _ = events.send(PrivilegesEvent::Disconnected);
                                continue 'outer;
                            }
                        }
                    }
                    Some(Internal::TokenRefreshed) => continue 'outer,
                },
            }
        }
    }
}

/// A per-install display name derived from the machine's hostname (D-033):
/// what other devices will show for this client if it ever holds the
/// privileged stream. Best-effort — there is no dependency-free, portable
/// hostname API in `std`, so this tries the platform's own `hostname`
/// command (present on Linux, macOS and Windows) before giving up.
pub fn hostname_display_name() -> String {
    format!("streamboat ({})", hostname_best_effort())
}

fn hostname_best_effort() -> String {
    if let Ok(h) = std::env::var("COMPUTERNAME") {
        // Windows sets this reliably without spawning a process.
        if !h.trim().is_empty() {
            return h;
        }
    }
    if let Ok(out) = std::process::Command::new("hostname").output() {
        if out.status.success() {
            let h = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !h.is_empty() {
                return h;
            }
        }
    }
    "this device".to_string()
}

// Backoff-ceiling tests live in `tests/privileges.rs` alongside the
// websocket-server tests, since `backoff_delay` is `pub` for exactly that.
