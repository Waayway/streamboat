//! Integration tests for the control API (D-030, D-031): an in-process
//! `axum` server over a real [`Player`](streamboat_player::Player) driving
//! the `FakeEngine` test double, exercised through a real HTTP client
//! (`reqwest`) and a real WebSocket client (`tokio-tungstenite`) — nothing
//! here talks to the router in-process without going over a socket, so it
//! also proves the Host/token middleware actually runs on the wire.

mod common;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use streamboat_server::api::{ApiState, Envelope, default_allowed_hosts, router};
use tokio_tungstenite::tungstenite::http::Uri;
use tokio_tungstenite::tungstenite::{ClientRequestBuilder, Message as WsMessage};

const TEST_TOKEN: &str = "test-only-token-not-a-secret";

/// Start the control API on an ephemeral loopback port over a fresh
/// `Player` (single-track TIDAL mock, `FakeEngine`). Returns the base HTTP
/// URL and the `MockServer`, which must stay alive for the caller's whole
/// test.
async fn spawn_api() -> (String, wiremock::MockServer) {
    let (handle, mock) = common::spawn_player().await;
    let state = ApiState::new(handle, TEST_TOKEN, default_allowed_hosts());
    let app = router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), mock)
}

#[tokio::test]
async fn health_needs_no_token() {
    let (base, _mock) = spawn_api().await;
    let client = reqwest::Client::new();
    let resp = client.get(format!("{base}/health")).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("state"))
    );
    assert!(body["protocol_version"].is_number());
}

#[tokio::test]
async fn state_is_rejected_without_a_token_and_with_the_wrong_one() {
    let (base, _mock) = spawn_api().await;
    let client = reqwest::Client::new();

    let no_auth = client.get(format!("{base}/v1/state")).send().await.unwrap();
    assert_eq!(no_auth.status(), reqwest::StatusCode::UNAUTHORIZED);

    let wrong = client
        .get(format!("{base}/v1/state"))
        .bearer_auth("not-the-token")
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);

    let right = client
        .get(format!("{base}/v1/state"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(right.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn a_host_header_outside_the_allowlist_is_rejected() {
    let (base, _mock) = spawn_api().await;
    let client = reqwest::Client::new();

    // A same-origin request (the mock's real address) but with a spoofed
    // `Host` header — exactly the DNS-rebinding shape the guard exists for.
    let rebound = client
        .get(format!("{base}/health"))
        .header(reqwest::header::HOST, "evil.example.com")
        .send()
        .await
        .unwrap();
    assert_eq!(rebound.status(), reqwest::StatusCode::FORBIDDEN);

    // The real Host header (whatever `base` resolves to) still works.
    let ok = client.get(format!("{base}/health")).send().await.unwrap();
    assert_eq!(ok.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn posting_a_command_changes_the_reported_state() {
    let (base, _mock) = spawn_api().await;
    let client = reqwest::Client::new();

    let accepted = client
        .post(format!("{base}/v1/commands"))
        .bearer_auth(TEST_TOKEN)
        .json(&json!({"type": "play", "items": [{"track_id": 1}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), reqwest::StatusCode::ACCEPTED);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let state: Value = client
            .get(format!("{base}/v1/state"))
            .bearer_auth(TEST_TOKEN)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if state["current"].is_object() {
            assert_eq!(state["current"]["id"], 1);
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("state never reflected the Play command: {state:?}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn ws_request(base: &str, token: &str) -> ClientRequestBuilder {
    let uri: Uri = format!("{}/v1/events", base.replacen("http://", "ws://", 1))
        .parse()
        .unwrap();
    ClientRequestBuilder::new(uri).with_header("Authorization", format!("Bearer {token}"))
}

#[tokio::test]
async fn ws_sends_a_snapshot_then_events_with_increasing_revision() {
    let (base, _mock) = spawn_api().await;
    let (mut ws, _resp) = tokio_tungstenite::connect_async(ws_request(&base, TEST_TOKEN))
        .await
        .unwrap();

    let first = next_envelope(&mut ws).await;
    assert_eq!(
        first.event["type"], "state",
        "first message must be a snapshot: {first:?}"
    );
    let first_revision = first.revision;

    // Cause more events by posting a command over plain HTTP.
    reqwest::Client::new()
        .post(format!("{base}/v1/commands"))
        .bearer_auth(TEST_TOKEN)
        .json(&json!({"type": "play", "items": [{"track_id": 1}]}))
        .send()
        .await
        .unwrap();

    let second = next_envelope(&mut ws).await;
    assert!(
        second.revision > first_revision,
        "revision must strictly increase: {first_revision} then {}",
        second.revision
    );
    let third = next_envelope(&mut ws).await;
    assert!(third.revision > second.revision);
}

#[tokio::test]
async fn ws_accepts_an_inbound_command() {
    let (base, _mock) = spawn_api().await;
    let (mut ws, _resp) = tokio_tungstenite::connect_async(ws_request(&base, TEST_TOKEN))
        .await
        .unwrap();

    let snapshot = next_envelope(&mut ws).await;
    assert_eq!(snapshot.event["type"], "state");

    ws.send(WsMessage::text(r#"{"type":"get_state"}"#))
        .await
        .unwrap();

    // `get_state` always causes the player to (re-)emit a `State` event;
    // seeing one with a higher revision than the snapshot proves the
    // inbound command on the socket reached the player.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let env = next_envelope(&mut ws).await;
        if env.event["type"] == "state" && env.revision > snapshot.revision {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("never saw a State event caused by the inbound get_state command");
        }
    }
}

/// One parsed `{"revision": n, "event": {...}}` message.
struct Parsed {
    revision: u64,
    event: Value,
}

impl std::fmt::Debug for Parsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parsed {{ revision: {}, event: {} }}",
            self.revision, self.event
        )
    }
}

async fn next_envelope(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Parsed {
    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("timed out waiting for a WS message")
        .expect("WS stream ended")
        .expect("WS error");
    match msg {
        WsMessage::Text(text) => {
            let env: Envelope = serde_json::from_str(&text).expect("valid envelope JSON");
            let event = serde_json::to_value(&env.event).unwrap();
            Parsed {
                revision: env.revision,
                event,
            }
        }
        other => panic!("expected a text message, got {other:?}"),
    }
}
