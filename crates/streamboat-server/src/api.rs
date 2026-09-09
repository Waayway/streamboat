//! The headless control API (D-030, D-031): HTTP + WebSocket JSON on
//! `127.0.0.1`, a generated bearer token, and Host-header allowlisting
//! against DNS rebinding (`headless-and-tidal-connect/references/daemon-architecture.md`
//! §3). Only `streamboatd` hosts this; the desktop shell talks to the same
//! [`streamboat_player::PlayerHandle`] in-process (D-010).
//!
//! Routes:
//!
//! - `GET /health` — unauthenticated, still Host-checked. Returns
//!   `{version, protocol_version, capabilities: [...]}` so a client can
//!   degrade gracefully on a missing capability instead of refusing to
//!   connect (§4's MPD-style recommendation over Sendspin's strict one).
//! - `GET /v1/state` — the current [`PlayerState`] snapshot.
//! - `POST /v1/commands` — a [`Command`] JSON body; enqueued on the player
//!   and acknowledged with 202, not awaited to completion (the player
//!   itself reports the outcome as an `Event`).
//! - `GET /v1/events` — upgrades to a WebSocket. The first message is
//!   always a full `State` snapshot; every message after that is one
//!   `Event`. Every message on the wire is wrapped the same way:
//!   `{"revision": n, "event": {...}}`, `revision` monotonically
//!   increasing for the lifetime of the daemon (§4's recommendation). If a
//!   client ever sees a gap in `revision` (a dropped connection, a slow
//!   reader that lagged past the server's replay buffer), the rule is to
//!   request a new snapshot rather than trying to patch the gap — either by
//!   reconnecting (a fresh connection always opens with one) or by sending
//!   a `{"type":"get_state"}` command on the same socket and waiting for
//!   the `State` event it causes. The socket also accepts inbound `Command`
//!   JSON text messages, applied the same way `POST /v1/commands` does.
//!
//! Every route but `/health` requires `Authorization: Bearer <token>`.
//! Every route, `/health` included, requires a `Host` header matching one
//! of the configured allowlist entries — a plain loopback bind plus a
//! generated token does not by itself stop DNS rebinding (a page in the
//! user's browser resolving an attacker hostname to `127.0.0.1`), so the
//! Host check runs first and unconditionally.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use streamboat_core::proto::{Command, Event, PROTOCOL_VERSION, PlayerState};
use streamboat_player::PlayerHandle;
use tokio::sync::{broadcast, watch};

/// The control API's own feature list, returned by `GET /health` for
/// capability negotiation (D-031: additive and tolerant, never exact-match).
pub const CAPABILITIES: &[&str] = &["state", "commands", "events"];

/// One message on the `/v1/events` WebSocket: an [`Event`] tagged with a
/// revision number monotonically increasing for the daemon's lifetime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub revision: u64,
    pub event: Event,
}

/// Shared state behind every route. Cheap to clone (every field is an
/// `Arc`, a channel handle, or a watch receiver) — axum clones it once per
/// request.
#[derive(Clone)]
pub struct ApiState {
    handle: PlayerHandle,
    token: Arc<str>,
    allowed_hosts: Arc<[String]>,
    revision: Arc<AtomicU64>,
    state_rx: watch::Receiver<PlayerState>,
    envelopes: broadcast::Sender<Envelope>,
}

impl ApiState {
    /// Build the shared state and start the background task that turns the
    /// player's own event broadcast into the revision-numbered stream every
    /// route reads from. `allowed_hosts` are compared case-insensitively
    /// against the request's `Host` header with any port stripped (so
    /// `127.0.0.1` matches both `127.0.0.1` and `127.0.0.1:4747`).
    pub fn new(
        handle: PlayerHandle,
        token: impl Into<Arc<str>>,
        allowed_hosts: Vec<String>,
    ) -> Self {
        let revision = Arc::new(AtomicU64::new(0));
        let (envelopes_tx, _) = broadcast::channel(512);
        let (state_tx, state_rx) = watch::channel(PlayerState::default());

        // Subscribe (and request a snapshot) synchronously, before this
        // function returns: `PlayerHandle::publish` only reaches
        // subscribers that already exist at send time, so a caller that
        // kicks off headless login right after this call must not be able
        // to race the background task below into existence.
        let mut events = handle.subscribe();
        handle.send(Command::GetState);

        let task_revision = revision.clone();
        let task_envelopes = envelopes_tx.clone();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if let Event::State { state } = &event {
                            let _ = state_tx.send(state.clone());
                        }
                        let rev = task_revision.fetch_add(1, Ordering::SeqCst) + 1;
                        let _ = task_envelopes.send(Envelope {
                            revision: rev,
                            event,
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            missed = n,
                            "control API's revision task lagged behind the player's own event broadcast"
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        Self {
            handle,
            token: token.into(),
            allowed_hosts: allowed_hosts.into(),
            revision,
            state_rx,
            envelopes: envelopes_tx,
        }
    }
}

/// Build the router. Host and bearer-token enforcement (§ above) wrap every
/// route as one middleware layer so there is exactly one place that decides
/// what a request is allowed to reach.
pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/state", get(get_state))
        .route("/v1/commands", post(post_command))
        .route("/v1/events", get(ws_events))
        .route_layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

async fn guard(State(state): State<ApiState>, req: Request, next: Next) -> Response {
    let host_header = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok());
    let host_ok = host_header.is_some_and(|h| {
        let host = strip_port(h);
        state
            .allowed_hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
    });
    if !host_ok {
        tracing::warn!(
            host = host_header.unwrap_or("<missing>"),
            "control API rejected a request: Host header not on the allowlist (DNS-rebinding guard)"
        );
        return (StatusCode::FORBIDDEN, "Host header not allowed").into_response();
    }

    if req.uri().path() == "/health" {
        return next.run(req).await;
    }

    let auth_ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| check_bearer(h, &state.token));
    if !auth_ok {
        return (
            StatusCode::UNAUTHORIZED,
            "missing or invalid Authorization: Bearer <token>",
        )
            .into_response();
    }

    next.run(req).await
}

/// Strip a trailing `:<port>` from a `Host` header value, including the
/// bracketed IPv6 form (`[::1]:4747`).
fn strip_port(host: &str) -> &str {
    if let Some(rest) = host.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            return &rest[..end];
        }
    }
    match host.rsplit_once(':') {
        Some((h, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => host,
    }
}

fn check_bearer(header_value: &str, token: &str) -> bool {
    match header_value.strip_prefix("Bearer ") {
        Some(v) => constant_time_eq(v.trim(), token),
        None => false,
    }
}

/// Not cryptographically hardened (a local daemon's threat model does not
/// need it), but avoids the cheapest short-circuit-on-first-byte-mismatch
/// timing tell a plain `==` gives a network attacker.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[derive(Serialize)]
struct Health {
    version: &'static str,
    protocol_version: u32,
    capabilities: &'static [&'static str],
}

async fn health() -> Json<Health> {
    Json(Health {
        version: env!("CARGO_PKG_VERSION"),
        protocol_version: PROTOCOL_VERSION,
        capabilities: CAPABILITIES,
    })
}

async fn get_state(State(state): State<ApiState>) -> Json<PlayerState> {
    Json(state.state_rx.borrow().clone())
}

async fn post_command(
    State(state): State<ApiState>,
    Json(cmd): Json<Command>,
) -> impl IntoResponse {
    if state.handle.send(cmd) {
        (StatusCode::ACCEPTED, Json(json!({"status": "accepted"})))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "player_shut_down"})),
        )
    }
}

async fn ws_events(State(state): State<ApiState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: ApiState) {
    let mut sub = state.envelopes.subscribe();

    // The initial snapshot always carries the revision as of right now;
    // anything the background task already published at or before this
    // revision is skipped below so the client never sees the same change
    // twice.
    let mut last_sent = state.revision.load(Ordering::SeqCst);
    let snapshot = state.state_rx.borrow().clone();
    let snapshot_env = Envelope {
        revision: last_sent,
        event: Event::State { state: snapshot },
    };
    if send_envelope(&mut socket, &snapshot_env).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<Command>(&text) {
                            Ok(cmd) => { state.handle.send(cmd); }
                            Err(e) => tracing::debug!(
                                error = %e,
                                "control API WS: ignoring an unparseable inbound command"
                            ),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            received = sub.recv() => {
                match received {
                    Ok(env) => {
                        if env.revision > last_sent {
                            last_sent = env.revision;
                            if send_envelope(&mut socket, &env).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // The reconnect rule (see the module doc) applies to
                        // the server's own replay buffer too: rather than
                        // leave the client with a gap, resync it in place
                        // with a fresh snapshot.
                        tracing::warn!(
                            missed = n,
                            "control API WS client lagged; resyncing with a fresh snapshot"
                        );
                        let rev = state.revision.load(Ordering::SeqCst);
                        let snap = state.state_rx.borrow().clone();
                        last_sent = rev;
                        let env = Envelope { revision: rev, event: Event::State { state: snap } };
                        if send_envelope(&mut socket, &env).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn send_envelope(socket: &mut WebSocket, env: &Envelope) -> Result<(), axum::Error> {
    let text = serde_json::to_string(env).unwrap_or_else(|_| "{}".to_string());
    socket.send(Message::Text(text.into())).await
}

/// The always-allowed loopback aliases, before any `--lan`/`--allow-host`
/// addition (D-030).
pub fn default_allowed_hosts() -> Vec<String> {
    vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_port_handles_ipv4_ipv6_and_bare_host() {
        assert_eq!(strip_port("127.0.0.1:4747"), "127.0.0.1");
        assert_eq!(strip_port("127.0.0.1"), "127.0.0.1");
        assert_eq!(strip_port("[::1]:4747"), "::1");
        assert_eq!(strip_port("[::1]"), "::1");
        assert_eq!(strip_port("localhost:4747"), "localhost");
    }

    #[test]
    fn bearer_check_requires_the_prefix_and_exact_token() {
        assert!(check_bearer("Bearer abc", "abc"));
        assert!(!check_bearer("Bearer abcd", "abc"));
        assert!(!check_bearer("abc", "abc"));
        assert!(!check_bearer("Bearer wrong", "abc"));
    }
}
