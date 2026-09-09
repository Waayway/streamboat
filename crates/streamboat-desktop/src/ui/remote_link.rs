//! [`RemoteLink`]: the control-API client half of D-010's "GUI becomes a
//! remote client" path, over the HTTP + WebSocket control API D-030/D-031
//! define (`streamboat_server::api`, documented in `docs/architecture.md`
//! "Control API"). `ui::instance` builds one whenever this process fails to
//! take the single-instance lock but finds a `control-address` file —
//! meaning another process (always `streamboatd` today, per D-031's "only
//! the daemon binds a listener") already hosts the surface this talks to.
//!
//! Every screen keeps working unchanged: `RemoteLink` implements the same
//! [`PlayerLink`] trait [`crate::ui::player_link::InProcessLink`] does, so
//! `ui::app::App` never knows which one it holds.
//!
//! - `GET /v1/state` is not polled here: the very first WebSocket message on
//!   any connection is always a full snapshot (`docs/architecture.md`), so
//!   [`events()`](PlayerLink::events) alone gives `ui::app::App` the same
//!   "state on connect" behaviour `InProcessLink` gets for free from
//!   `Player::spawn`'s own broadcast.
//! - `POST /v1/commands` backs [`send()`](PlayerLink::send).
//! - The `Host` header the control API's Host-allowlist middleware checks
//!   is set automatically by `reqwest`/`tokio-tungstenite` from the request
//!   URL's authority — connecting straight to `http://<bind-addr>/...`
//!   already sends exactly the bind address as `Host`, which is what the
//!   allowlist check needs; no override is needed.
//! - Reconnects with a fresh snapshot on any revision gap or disconnect,
//!   with capped exponential backoff (`docs/architecture.md`'s reconnect
//!   rule, `headless-and-tidal-connect/references/daemon-architecture.md`
//!   §4 for the client-reconnect rule this generalises).

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::StreamExt as _;
use streamboat_core::proto::{Command, Event};
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::tungstenite::{ClientRequestBuilder, Message as WsMessage, http::Uri};

use crate::ui::player_link::PlayerLink;
use crate::ui::stream_ext::BoxStream;

type WsStream = tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

fn next_backoff(current: Duration) -> Duration {
    if current.is_zero() {
        INITIAL_BACKOFF
    } else {
        (current * 2).min(MAX_BACKOFF)
    }
}

/// One `{"revision": n, "event": {...}}` message on `/v1/events`. Mirrors
/// `streamboat_server::api::Envelope`'s wire shape (part of the documented
/// protocol, D-031) — this crate only depends on `streamboat-server` as a
/// dev-dependency for tests, so production code re-declares the shape
/// rather than reaching for a non-dev dependency just for one struct.
#[derive(Debug, Clone, serde::Deserialize)]
struct Envelope {
    revision: u64,
    event: Event,
}

enum ConnState {
    Disconnected {
        backoff: Duration,
    },
    Connected {
        // Boxed: `WsStream` itself is over a kilobyte (mostly TLS/socket
        // buffer state carried by `MaybeTlsStream`), which would otherwise
        // make every `ConnState::Disconnected` pay that size too.
        ws: Box<WsStream>,
        last_revision: Option<u64>,
    },
}

/// A control-API client. Cheap to clone: `reqwest::Client` and
/// `tokio::runtime::Handle` already are, and connection info is plain data.
#[derive(Clone)]
pub struct RemoteLink {
    http: reqwest::Client,
    http_base: String,
    ws_base: String,
    token: String,
    /// `send()` fires an async POST from what may be iced's plain (non-async)
    /// update loop — this is the runtime it gets spawned onto, kept alive by
    /// `ui::app::run` for the process's lifetime. `events()` does not need
    /// it: its stream is polled by whichever runtime iced's own
    /// `Subscription` machinery drives (the `tokio` feature's runtime), the
    /// same way `InProcessLink::events()` never needs one either.
    runtime: tokio::runtime::Handle,
}

impl RemoteLink {
    pub fn new(
        addr: SocketAddr,
        token: impl Into<String>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent(format!(
                    "streamboat/{} (+{}; remote client)",
                    streamboat_core::VERSION,
                    streamboat_core::PROJECT_URL
                ))
                .build()
                .unwrap_or_default(),
            http_base: format!("http://{addr}"),
            ws_base: format!("ws://{addr}"),
            token: token.into(),
            runtime,
        }
    }

    fn events_url(&self) -> String {
        format!("{}/v1/events", self.ws_base)
    }
}

impl PlayerLink for RemoteLink {
    fn send(&self, cmd: Command) -> bool {
        let http = self.http.clone();
        let url = format!("{}/v1/commands", self.http_base);
        let token = self.token.clone();
        self.runtime.spawn(async move {
            match http.post(&url).bearer_auth(&token).json(&cmd).send().await {
                Ok(resp) if !resp.status().is_success() => {
                    tracing::warn!(
                        status = %resp.status(),
                        "remote link: the control API rejected a command"
                    );
                }
                Err(e) => tracing::warn!(error = %e, "remote link: POST /v1/commands failed"),
                Ok(_) => {}
            }
        });
        // The control API acknowledges with 202 and reports the outcome as
        // an `Event`, not in the response, so there is nothing synchronous
        // to fail on here — matching `InProcessLink::send`'s "false only if
        // the player has already shut down" contract as closely as an
        // inherently asynchronous transport can: a remote link cannot know
        // that synchronously either way, so it always returns `true`,
        // exactly as no existing call site checks this return value today.
        true
    }

    fn events(&self) -> BoxStream<Event> {
        let ws_url = self.events_url();
        let token = self.token.clone();
        Box::pin(futures::stream::unfold(
            ConnState::Disconnected {
                backoff: Duration::ZERO,
            },
            move |state| {
                let ws_url = ws_url.clone();
                let token = token.clone();
                async move {
                    let (event, next) = step(&ws_url, &token, state).await;
                    Some((event, next))
                }
            },
        ))
    }
}

/// Advances the reconnect/receive state machine by exactly one emitted
/// [`Event`]. Never returns `None`: this stream retries forever (a dropped
/// `Subscription`, i.e. the app exiting, is what actually stops it — the
/// same shape `InProcessLink::events()` has for the in-process case, where
/// only the player's broadcast closing ends the stream).
async fn step(ws_url: &str, token: &str, mut state: ConnState) -> (Event, ConnState) {
    loop {
        state = match state {
            ConnState::Disconnected { backoff } => {
                if !backoff.is_zero() {
                    tokio::time::sleep(backoff).await;
                }
                match connect(ws_url, token).await {
                    Ok(ws) => ConnState::Connected {
                        ws: Box::new(ws),
                        last_revision: None,
                    },
                    Err(e) => {
                        return (
                            Event::Warning {
                                message: format!("remote link: connecting to the control API: {e}"),
                            },
                            ConnState::Disconnected {
                                backoff: next_backoff(backoff),
                            },
                        );
                    }
                }
            }
            ConnState::Connected {
                mut ws,
                last_revision,
            } => match recv_envelope(&mut ws).await {
                Some(env) => {
                    // A gap: something between the last message we saw and
                    // this one was missed. This event is still valid data,
                    // so forward it, but drop the connection so the *next*
                    // poll reconnects and gets a fresh snapshot immediately
                    // (no backoff — this is a proactive resync, not a
                    // failure) per the reconnect rule.
                    let gapped = last_revision.is_some_and(|last| env.revision > last + 1);
                    let next_state = if gapped {
                        ConnState::Disconnected {
                            backoff: Duration::ZERO,
                        }
                    } else {
                        ConnState::Connected {
                            ws,
                            last_revision: Some(env.revision),
                        }
                    };
                    return (env.event, next_state);
                }
                None => {
                    return (
                        Event::Warning {
                            message:
                                "remote link: lost the connection to the control API; reconnecting"
                                    .to_string(),
                        },
                        ConnState::Disconnected {
                            backoff: Duration::ZERO,
                        },
                    );
                }
            },
        };
    }
}

async fn connect(ws_url: &str, token: &str) -> anyhow::Result<WsStream> {
    let uri: Uri = ws_url.parse()?;
    let request =
        ClientRequestBuilder::new(uri).with_header("Authorization", format!("Bearer {token}"));
    let (ws, _response) = tokio_tungstenite::connect_async(request).await?;
    Ok(ws)
}

/// Reads the next envelope off the socket, skipping non-text frames
/// (ping/pong/binary) and tolerating one unparseable text message (logged,
/// not fatal — the same "don't drop the whole connection over one bad
/// frame" posture `streamboat_server::api`'s own inbound-command handling
/// takes). `None` means the socket closed or errored.
async fn recv_envelope(ws: &mut WsStream) -> Option<Envelope> {
    loop {
        match ws.next().await {
            Some(Ok(WsMessage::Text(text))) => match serde_json::from_str::<Envelope>(&text) {
                Ok(env) => return Some(env),
                Err(e) => {
                    tracing::debug!(error = %e, "remote link: ignoring an unparseable envelope");
                    continue;
                }
            },
            Some(Ok(WsMessage::Close(_))) | None => return None,
            Some(Ok(_)) => continue,
            Some(Err(_)) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc as std_mpsc;

    use base64::Engine as _;
    use serde_json::json;
    use streamboat_core::proto::{Command, OutputConfig, PlayItem, SignalPath};
    use streamboat_core::token_store::{MemoryTokenStore, TokenSet, now_secs};
    use streamboat_core::{ApiClient, AuthFlow, ClientCredentials, StreamSource};
    use streamboat_player::engine::{EngineError, EngineResult};
    use streamboat_player::{Engine, EngineEvent, LoadItem, Player, PlayerConfig, PlayerDeps};
    use streamboat_server::api::{ApiState, default_allowed_hosts, router};

    use super::*;

    /// A minimal `Engine` double, mirroring `streamboat-server`'s own
    /// `tests/common::FakeEngine` (not reusable across the crate boundary —
    /// it lives under that crate's `tests/`, not its library — so this is a
    /// deliberately small local copy of the same pattern, not a divergent
    /// one).
    struct FakeEngine {
        events: std_mpsc::Sender<EngineEvent>,
        output: OutputConfig,
    }

    impl FakeEngine {
        fn new() -> (Self, std_mpsc::Receiver<EngineEvent>) {
            let (tx, rx) = std_mpsc::channel();
            (
                Self {
                    events: tx,
                    output: OutputConfig::default(),
                },
                rx,
            )
        }
    }

    impl Engine for FakeEngine {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn load(&mut self, item: LoadItem) -> EngineResult<()> {
            let url = match &item.source {
                StreamSource::Url(u) => u.clone(),
                StreamSource::DashMpd(_) => "dash".into(),
            };
            if url.contains("unplayable") {
                return Err(EngineError::Source("fake engine refuses this url".into()));
            }
            let events = self.events.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                let _ = events.send(EngineEvent::Started { id: item.id });
            });
            Ok(())
        }
        fn set_next(&mut self, _item: Option<LoadItem>) {}
        fn play(&mut self) -> EngineResult<()> {
            Ok(())
        }
        fn pause(&mut self) -> EngineResult<()> {
            Ok(())
        }
        fn stop(&mut self) -> EngineResult<()> {
            Ok(())
        }
        fn seek(&mut self, _position_ms: u64) -> EngineResult<()> {
            Ok(())
        }
        fn set_volume(&mut self, _volume: f32) -> EngineResult<()> {
            Ok(())
        }
        fn set_output(&mut self, output: &OutputConfig) -> EngineResult<()> {
            self.output = output.clone();
            Ok(())
        }
        fn position(&self) -> Option<(u64, Option<u64>)> {
            Some((1000, Some(180_000)))
        }
        fn signal_path(&self) -> Option<SignalPath> {
            Some(SignalPath {
                engine: "fake".into(),
                exclusive: self.output.is_exclusive(),
                ..Default::default()
            })
        }
    }

    fn tokens() -> TokenSet {
        TokenSet {
            access_token: "at".into(),
            refresh_token: Some("rt".into()),
            token_type: "Bearer".into(),
            expires_at: now_secs() + 3600,
            scope: "r_usr w_usr w_sub".into(),
            client_id: "cid".into(),
            flow: AuthFlow::DeviceCode,
            client_unique_key: None,
            user_id: Some(1),
            country_code: Some("NL".into()),
        }
    }

    fn bts(url: &str) -> serde_json::Value {
        let manifest = json!({
            "mimeType": "audio/flac", "codecs": "flac", "encryptionType": "NONE", "urls": [url]
        });
        json!({
            "trackId": 1, "assetPresentation": "FULL", "audioMode": "STEREO",
            "audioQuality": "LOSSLESS", "manifestMimeType": "application/vnd.tidal.bts",
            "manifest": base64::engine::general_purpose::STANDARD.encode(manifest.to_string()),
            "bitDepth": 16, "sampleRate": 44100
        })
    }

    async fn tidal_mock() -> wiremock::MockServer {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/tracks/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 1, "title": "One", "duration": 180, "artists": [{"id": 9, "name": "Artist"}],
                "album": {"id": 5, "title": "Album", "cover": "c"}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/tracks/1/playbackinfopostpaywall"))
            .respond_with(ResponseTemplate::new(200).set_body_json(bts("https://cdn/one.flac")))
            .mount(&server)
            .await;
        server
    }

    const TEST_TOKEN: &str = "remote-link-test-token";

    /// Owns a control-API "daemon" running on its *own* dedicated
    /// multi-thread [`tokio::runtime::Runtime`] — deliberately separate
    /// from the `#[tokio::test]` runtime the test body and `RemoteLink`
    /// itself run on, mirroring the real client/daemon process boundary
    /// this type exists for. [`ServerRuntime::kill`] (and, if never called,
    /// `Drop`) tears the whole thing down via `shutdown_background`, which
    /// terminates every task on it *immediately* — including already-
    /// established connections' handler tasks — the way an OS actually
    /// kills every socket a crashed or stopped process held open. Aborting
    /// only the top-level `axum::serve` task (tried first while writing
    /// this test) does not: each accepted connection's handler is its own
    /// independently spawned task that keeps running unaffected, so the
    /// client-visible symptom of "abort the serve task" is silence, not a
    /// disconnect.
    struct ServerRuntime(Option<tokio::runtime::Runtime>);

    impl ServerRuntime {
        fn kill(&mut self) {
            if let Some(rt) = self.0.take() {
                rt.shutdown_background();
            }
        }
    }

    impl Drop for ServerRuntime {
        fn drop(&mut self) {
            self.kill();
        }
    }

    /// Starts the real control API (`streamboat_server::api::router`) on
    /// `addr` (bind `127.0.0.1:0` for an ephemeral port), backed by a real
    /// `Player` over `FakeEngine` and the single-track TIDAL mock above —
    /// the same harness shape `streamboat-server`'s own `tests/api.rs`
    /// uses, built locally since that file lives under a `tests/` directory
    /// this crate cannot import. Everything server-side runs on its own
    /// runtime (see [`ServerRuntime`]) built inside `spawn_blocking` —
    /// `Runtime::block_on` panics if called while already inside another
    /// runtime's worker thread, which every caller here is (a
    /// `#[tokio::test]`), so the nested runtime has to be driven from a
    /// plain OS thread instead. Returns the bound address, the `MockServer`
    /// (which must outlive the caller), and the [`ServerRuntime`].
    async fn spawn_control_api(addr: &str) -> (SocketAddr, wiremock::MockServer, ServerRuntime) {
        let addr = addr.to_string();
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let (bound, tidal) = rt.block_on(async move {
                let tidal = tidal_mock().await;
                let api = ApiClient::builder(
                    ClientCredentials::new("cid", None),
                    Arc::new(MemoryTokenStore::with(tokens())),
                )
                .api_base(&format!("{}/", tidal.uri()))
                .auth_base(&format!("{}/", tidal.uri()))
                .build()
                .unwrap();
                let (engine, rx) = FakeEngine::new();
                let handle = Player::spawn(
                    api,
                    Box::new(engine),
                    rx,
                    PlayerConfig::default(),
                    PlayerDeps::default(),
                );
                let state = ApiState::new(handle, TEST_TOKEN, default_allowed_hosts());
                let app = router(state);
                let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
                let bound = listener.local_addr().unwrap();
                tokio::spawn(async move {
                    let _ = axum::serve(listener, app).await;
                });
                (bound, tidal)
            });
            (bound, tidal, ServerRuntime(Some(rt)))
        })
        .await
        .unwrap()
    }

    /// A raw envelope `{"revision": n, "event": {"type": "state", ...}}`
    /// with a default `PlayerState` — the exact content never matters for
    /// this test, only `revision`.
    fn envelope_json(revision: u64) -> String {
        json!({
            "revision": revision,
            "event": {"type": "state", "state": streamboat_core::proto::PlayerState::default()}
        })
        .to_string()
    }

    /// Exercises the gap-detection branch of `step()` end to end, against a
    /// raw `tokio-tungstenite` server this test writes itself rather than
    /// the real control API — the real server's own revisions only ever
    /// increase by exactly one, so a deliberately-crafted gap is the only
    /// way to trigger this deterministically rather than racing a broadcast-
    /// channel overflow. The server plays two connections in order: the
    /// first sends revision 1 then jumps to revision 5 (a gap); the second
    /// (which only a client that actually reconnected will ever reach)
    /// sends revision 100. Seeing all three revisions in order proves the
    /// gap was both forwarded (a detected gap does not discard the event
    /// that revealed it — it is still valid data) and caused an immediate
    /// reconnect that landed on a fresh snapshot.
    #[tokio::test]
    async fn reconnects_immediately_on_a_detected_revision_gap_and_gets_a_fresh_snapshot() {
        use futures_util::SinkExt as _;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                if let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await {
                    let _ = ws.send(WsMessage::text(envelope_json(1))).await;
                    let _ = ws.send(WsMessage::text(envelope_json(5))).await;
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
            if let Ok((stream, _)) = listener.accept().await {
                if let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await {
                    let _ = ws.send(WsMessage::text(envelope_json(100))).await;
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        });

        let link = RemoteLink::new(addr, "any-token", tokio::runtime::Handle::current());
        let mut events = link.events();

        for _ in 0..3 {
            let ev = tokio::time::timeout(Duration::from_secs(5), events.next())
                .await
                .expect("no timeout")
                .expect("stream stays open");
            assert!(
                matches!(ev, Event::State { .. }),
                "expected a State event: {ev:?}"
            );
        }
    }

    #[tokio::test]
    async fn events_yields_a_snapshot_first() {
        let (addr, _tidal, _server) = spawn_control_api("127.0.0.1:0").await;
        let link = RemoteLink::new(addr, TEST_TOKEN, tokio::runtime::Handle::current());
        let mut events = link.events();
        let first = events.next().await.expect("a first event");
        assert!(
            matches!(first, Event::State { .. }),
            "the first event over a fresh connection must be a full snapshot: {first:?}"
        );
    }

    #[tokio::test]
    async fn send_reaches_the_player_through_the_control_api() {
        let (addr, _tidal, _server) = spawn_control_api("127.0.0.1:0").await;
        let link = RemoteLink::new(addr, TEST_TOKEN, tokio::runtime::Handle::current());
        let mut events = link.events();
        let snapshot = events.next().await.expect("snapshot");
        assert!(matches!(snapshot, Event::State { .. }));

        assert!(link.send(Command::Play {
            items: vec![PlayItem { track_id: 1 }],
        }));

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let ev = tokio::time::timeout(Duration::from_secs(5), events.next())
                .await
                .expect("an event before the timeout")
                .expect("the stream stayed open");
            if let Event::State { state } = &ev {
                if state.current.as_ref().is_some_and(|t| t.id == 1) {
                    break;
                }
            }
            if tokio::time::Instant::now() > deadline {
                panic!("never saw the Play command reflected back as an Event");
            }
        }
    }

    /// The full reconnect story: the daemon vanishes mid-connection, the
    /// link notices and starts retrying instead of ending the stream, and
    /// once a daemon is reachable again at the same address it reconnects
    /// and delivers a fresh snapshot — proving both halves of "reconnect
    /// with a fresh snapshot on ... disconnect (capped backoff)" rather
    /// than just one.
    #[tokio::test]
    async fn reconnects_and_resnapshots_after_the_daemon_disappears_and_comes_back() {
        let (addr, _tidal_1, mut server_1) = spawn_control_api("127.0.0.1:0").await;
        let link = RemoteLink::new(addr, TEST_TOKEN, tokio::runtime::Handle::current());
        let mut events = link.events();

        let snapshot = events.next().await.expect("initial snapshot");
        assert!(matches!(snapshot, Event::State { .. }));

        // Kill the daemon: tearing down its whole runtime drops the
        // listener *and* every already-accepted connection's task, which
        // is what a crashed or stopped `streamboatd` looks like from a
        // client's side (see `ServerRuntime`'s doc comment for why merely
        // aborting the top-level serve task, tried first, does not).
        server_1.kill();

        // The link must notice and start retrying, not end the stream.
        let after_disconnect = tokio::time::timeout(Duration::from_secs(5), events.next())
            .await
            .expect("the stream must keep producing events, not end, after a disconnect")
            .expect("the stream must stay open (Some), never terminate (None)");
        assert!(
            matches!(after_disconnect, Event::Warning { .. }),
            "expected a reconnect warning while the daemon is down: {after_disconnect:?}"
        );

        // Bring a daemon back up on the exact same address (a fresh Player,
        // fresh TIDAL mock — a real restart, not the same process).
        let (_addr_2, _tidal_2, _serve_2) = spawn_control_api(&addr.to_string()).await;

        // With capped exponential backoff starting at 250ms this can take a
        // few retries; a generous timeout covers that without the test
        // itself sleeping through the backoff on a fixed schedule.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            let ev = tokio::time::timeout(Duration::from_secs(15), events.next())
                .await
                .expect("no timeout")
                .expect("stream stays open");
            if matches!(ev, Event::State { .. }) {
                break;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("never reconnected and re-snapshotted after the daemon came back");
            }
        }
    }
}
