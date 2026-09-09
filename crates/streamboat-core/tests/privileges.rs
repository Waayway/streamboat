//! Local-websocket-server tests for the streaming-privileges ("Pushkin")
//! client: handshake, the `USER_ACTION` claim message shape, a
//! `PRIVILEGED_SESSION_NOTIFICATION` revoke, `RECONNECT` handling, and the
//! backoff ceiling
//! (`headless-and-tidal-connect/references/daemon-architecture.md` §6).

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use streamboat_core::privileges::{MAX_DELAY, PrivilegesEvent, StreamingPrivileges, backoff_delay};
use streamboat_core::token_store::{AuthFlow, MemoryTokenStore, TokenSet, now_secs};
use streamboat_core::{ApiClient, ClientCredentials};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

fn client(server: &MockServer) -> ApiClient {
    ApiClient::builder(
        ClientCredentials::new("cid", None),
        Arc::new(MemoryTokenStore::with(tokens())),
    )
    .api_base(&format!("{}/", server.uri()))
    .auth_base(&format!("{}/", server.uri()))
    .build()
    .unwrap()
}

type WsStream = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

/// A tiny local websocket server: accepts connections on an ephemeral port
/// and hands each upgraded socket back over a channel, so a test can drive
/// the "other end" of the Pushkin conversation directly.
struct WsTestServer {
    ws_url: String,
    accepted: mpsc::UnboundedReceiver<WsStream>,
}

impl WsTestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                    continue;
                };
                if tx.send(ws).is_err() {
                    break;
                }
            }
        });
        Self {
            ws_url: format!("ws://{addr}/"),
            accepted: rx,
        }
    }

    async fn next_connection(&mut self) -> WsStream {
        tokio::time::timeout(Duration::from_secs(5), self.accepted.recv())
            .await
            .expect("a client connected in time")
            .expect("server accept channel still open")
    }
}

async fn mount_connect(server: &MockServer, ws_url: &str, times: u64) {
    Mock::given(method("POST"))
        .and(path("/v1/rt/connect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "url": ws_url })))
        .expect(times)
        .mount(server)
        .await;
}

async fn next_event(events: &mut mpsc::UnboundedReceiver<PrivilegesEvent>) -> PrivilegesEvent {
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("an event arrived in time")
        .expect("event channel still open")
}

#[tokio::test]
async fn connects_and_claims_with_the_documented_message_shape() {
    let mut ws_server = WsTestServer::start().await;
    let http_server = MockServer::start().await;
    mount_connect(&http_server, &ws_server.ws_url, 1).await;

    let (handle, mut events) = StreamingPrivileges::spawn(client(&http_server), "test".into());
    assert_eq!(next_event(&mut events).await, PrivilegesEvent::Connected);
    let mut server_socket = ws_server.next_connection().await;

    handle.claim();
    let msg = tokio::time::timeout(Duration::from_secs(5), server_socket.next())
        .await
        .expect("claim message arrived in time")
        .expect("socket stayed open")
        .expect("a valid websocket message");
    let Message::Text(txt) = msg else {
        panic!("expected a text frame, got {msg:?}");
    };
    let parsed: serde_json::Value = serde_json::from_str(&txt).unwrap();
    assert_eq!(parsed["type"], "USER_ACTION");
    assert!(
        parsed["payload"]["startedAt"].as_u64().is_some(),
        "startedAt must be an epoch-ms number: {parsed}"
    );
}

#[tokio::test]
async fn revoked_session_surfaces_the_other_devices_display_name() {
    let mut ws_server = WsTestServer::start().await;
    let http_server = MockServer::start().await;
    mount_connect(&http_server, &ws_server.ws_url, 1).await;

    let (_handle, mut events) = StreamingPrivileges::spawn(client(&http_server), "test".into());
    assert_eq!(next_event(&mut events).await, PrivilegesEvent::Connected);
    let mut server_socket = ws_server.next_connection().await;

    server_socket
        .send(Message::Text(
            json!({
                "type": "PRIVILEGED_SESSION_NOTIFICATION",
                "payload": { "clientDisplayName": "Bob's Phone" }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let ev = next_event(&mut events).await;
    assert_eq!(
        ev,
        PrivilegesEvent::Revoked {
            client_display_name: "Bob's Phone".into()
        }
    );
}

#[tokio::test]
async fn reconnect_message_triggers_a_fresh_connect_call() {
    let mut ws_server = WsTestServer::start().await;
    let http_server = MockServer::start().await;
    // Once for the initial connect, once again after the server asks for a
    // reconnect.
    mount_connect(&http_server, &ws_server.ws_url, 2).await;

    let (_handle, mut events) = StreamingPrivileges::spawn(client(&http_server), "test".into());
    assert_eq!(next_event(&mut events).await, PrivilegesEvent::Connected);
    let mut server_socket = ws_server.next_connection().await;

    server_socket
        .send(Message::Text(
            json!({ "type": "RECONNECT" }).to_string().into(),
        ))
        .await
        .unwrap();

    assert_eq!(next_event(&mut events).await, PrivilegesEvent::Reconnect);
    assert_eq!(next_event(&mut events).await, PrivilegesEvent::Connected);
    // A second, real connection landed on the test server too.
    let _second_socket = ws_server.next_connection().await;
}

#[test]
fn backoff_never_exceeds_the_configured_ceiling() {
    let cap = MAX_DELAY;
    for attempt in 0..64 {
        let d = backoff_delay(attempt, Duration::from_secs(1), cap);
        assert!(d <= cap, "attempt {attempt} produced {d:?} > {cap:?}");
    }
}

#[test]
fn backoff_reaches_close_to_the_ceiling_for_large_attempts() {
    // Past a handful of doublings the exponential term is pinned at the
    // cap; across enough samples the jitter (uniform on [0, cap]) should
    // get close to it.
    let cap = Duration::from_millis(200);
    let base = Duration::from_millis(10);
    let max_seen = (0..500)
        .map(|_| backoff_delay(10, base, cap))
        .max()
        .unwrap();
    assert!(
        max_seen > cap * 8 / 10,
        "expected jitter to approach the cap {cap:?}, saw {max_seen:?}"
    );
}

#[test]
fn backoff_is_never_the_unbounded_immediate_retry() {
    // The reference browser SDK reconnects with zero delay; this client
    // must not, even on the very first attempt (its jitter can still be 0,
    // but the ceiling itself must be a real, bounded value).
    assert!(MAX_DELAY >= Duration::from_secs(1));
    assert!(MAX_DELAY <= Duration::from_secs(600));
}
