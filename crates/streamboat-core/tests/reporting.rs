//! Transport-level tests for play reporting: the event-batch payload shape,
//! the 30-second threshold, PREVIEW suppression, and the persistent
//! outbox's retry/drop rules
//! (`tidal-api/references/play-logging-and-privileges.md` §1-3).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use streamboat_core::models::{AudioMode, AudioQuality};
use streamboat_core::reporting::{FlushOutcome, PlayEvent, PlayReporter, REPORT_THRESHOLD_MS};
use streamboat_core::token_store::{AuthFlow, MemoryTokenStore, TokenSet, now_secs};
use streamboat_core::{ApiClient, ClientCredentials};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

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
        user_id: Some(42),
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

fn event(played_ms: u64, presentation: &str) -> PlayEvent {
    PlayEvent {
        track_id: 5,
        streaming_session_id: "sess-1".into(),
        asset_presentation: presentation.into(),
        audio_quality: Some(AudioQuality::Lossless),
        audio_mode: Some(AudioMode::Stereo),
        start_timestamp_ms: 1_000_000,
        end_timestamp_ms: 1_000_000 + played_ms,
        start_position_s: 0.0,
        end_position_s: played_ms as f64 / 1000.0,
        source: None,
    }
}

fn form_field(body: &[u8], key: &str) -> Option<String> {
    url::form_urlencoded::parse(body)
        .into_owned()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

struct Capture(Arc<std::sync::Mutex<Option<Vec<u8>>>>);
impl Respond for Capture {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        *self.0.lock().unwrap() = Some(req.body.clone());
        ResponseTemplate::new(200)
    }
}

#[tokio::test]
async fn reports_the_documented_event_batch_payload_shape() {
    let server = MockServer::start().await;
    let captured = Arc::new(std::sync::Mutex::new(None));
    Mock::given(method("POST"))
        .and(path("/api/event-batch"))
        .respond_with(Capture(captured.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let reporter = PlayReporter::open(client(&server), dir.path().join("q.json"), true)
        .unwrap()
        .event_base(&format!("{}/", server.uri()))
        .unwrap();
    reporter
        .record(event(REPORT_THRESHOLD_MS + 5_000, "FULL"))
        .await;
    assert_eq!(reporter.pending_len().await, 0);

    let body = captured
        .lock()
        .unwrap()
        .take()
        .expect("event-batch was called");
    let msg_body = form_field(&body, "SendMessageBatchRequestEntry.1.MessageBody").unwrap();
    let headers = form_field(
        &body,
        "SendMessageBatchRequestEntry.1.MessageAttribute.2.Value.StringValue",
    )
    .unwrap();
    let attr1_value = form_field(
        &body,
        "SendMessageBatchRequestEntry.1.MessageAttribute.1.Value.StringValue",
    )
    .unwrap();
    assert_eq!(attr1_value, "playback_session");

    let parsed: serde_json::Value = serde_json::from_str(&msg_body).unwrap();
    assert_eq!(parsed["group"], "play_log");
    assert_eq!(parsed["version"], 2);
    assert_eq!(parsed["user"]["id"], 42);
    assert_eq!(parsed["user"]["clientId"], "cid");
    assert_eq!(parsed["user"]["sessionId"], "sess-1");
    assert_eq!(parsed["payload"]["playbackSessionId"], "sess-1");
    assert_eq!(parsed["payload"]["requestedProductId"], "5");
    assert_eq!(parsed["payload"]["actualProductId"], "5");
    assert_eq!(parsed["payload"]["actualAssetPresentation"], "FULL");
    assert_eq!(parsed["payload"]["actualQuality"], "LOSSLESS");
    assert_eq!(parsed["payload"]["actualAudioMode"], "STEREO");
    assert_eq!(parsed["payload"]["isPostPaywall"], true);
    // No source was supplied: the field must be absent, never null (§3).
    assert!(parsed["payload"].get("sourceType").is_none());

    let headers_json: serde_json::Value = serde_json::from_str(&headers).unwrap();
    assert_eq!(headers_json["authorization"], "at");
    assert_eq!(headers_json["client-id"], "cid");
    assert_eq!(headers_json["consent-category"], "NECESSARY");
    let mut keys: Vec<&str> = headers_json
        .as_object()
        .unwrap()
        .keys()
        .map(|s| s.as_str())
        .collect();
    keys.sort_unstable();
    let mut expected = [
        "client-id",
        "app-version",
        "os-name",
        "os-version",
        "device-model",
        "device-vendor",
        "consent-category",
        "requested-sent-timestamp",
        "authorization",
    ];
    expected.sort_unstable();
    assert_eq!(keys, expected);
}

#[tokio::test]
async fn under_threshold_play_is_never_reported() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/event-batch"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let reporter = PlayReporter::open(client(&server), dir.path().join("q.json"), true)
        .unwrap()
        .event_base(&format!("{}/", server.uri()))
        .unwrap();
    reporter
        .record(event(REPORT_THRESHOLD_MS - 1_000, "FULL"))
        .await;
    assert_eq!(reporter.pending_len().await, 0);
}

#[tokio::test]
async fn preview_play_is_never_reported_even_when_long() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/event-batch"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let reporter = PlayReporter::open(client(&server), dir.path().join("q.json"), true)
        .unwrap()
        .event_base(&format!("{}/", server.uri()))
        .unwrap();
    reporter.record(event(120_000, "PREVIEW")).await;
    assert_eq!(reporter.pending_len().await, 0);
}

#[tokio::test]
async fn disabled_reporter_never_calls_out() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/event-batch"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let reporter = PlayReporter::open(client(&server), dir.path().join("q.json"), false)
        .unwrap()
        .event_base(&format!("{}/", server.uri()))
        .unwrap();
    reporter.record(event(60_000, "FULL")).await;
    assert_eq!(reporter.pending_len().await, 0);
}

struct SenderFaultOnce;
impl Respond for SenderFaultOnce {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let id = form_field(&req.body, "SendMessageBatchRequestEntry.1.Id").unwrap_or_default();
        let xml = format!(
            "<SendMessageBatchResponse><SendMessageBatchResult>\
             <BatchResultErrorEntry><Id>{id}</Id><SenderFault>true</SenderFault>\
             <Code>MalformedInput</Code><Message>bad event</Message></BatchResultErrorEntry>\
             </SendMessageBatchResult></SendMessageBatchResponse>"
        );
        ResponseTemplate::new(200).set_body_string(xml)
    }
}

#[tokio::test]
async fn sender_fault_drops_permanently_and_is_never_retried() {
    let server = MockServer::start().await;
    // `.expect(1)`: a second flush call must not hit the network again.
    Mock::given(method("POST"))
        .and(path("/api/event-batch"))
        .respond_with(SenderFaultOnce)
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let reporter = PlayReporter::open(client(&server), dir.path().join("q.json"), true)
        .unwrap()
        .event_base(&format!("{}/", server.uri()))
        .unwrap();
    reporter.record(event(60_000, "FULL")).await;
    assert_eq!(
        reporter.pending_len().await,
        0,
        "sender-fault entry must be dropped"
    );
    // Nothing left to flush; must be a no-op, not another network call.
    let outcome = reporter.try_flush().await.unwrap();
    assert_eq!(outcome, FlushOutcome::default());
}

struct FlakyThenOk {
    calls: AtomicU32,
}
impl Respond for FlakyThenOk {
    fn respond(&self, _req: &Request) -> ResponseTemplate {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            ResponseTemplate::new(503)
        } else {
            ResponseTemplate::new(200)
        }
    }
}

#[tokio::test]
async fn retries_on_5xx_and_keeps_the_event_queued_until_then() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/event-batch"))
        .respond_with(FlakyThenOk {
            calls: AtomicU32::new(0),
        })
        .expect(2)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let reporter = PlayReporter::open(client(&server), dir.path().join("q.json"), true)
        .unwrap()
        .event_base(&format!("{}/", server.uri()))
        .unwrap();
    reporter.record(event(60_000, "FULL")).await; // first attempt: 503
    assert_eq!(
        reporter.pending_len().await,
        1,
        "a 5xx must keep the event queued"
    );
    let outcome = reporter.try_flush().await.unwrap(); // second attempt: 200
    assert_eq!(outcome.sent, 1);
    assert_eq!(reporter.pending_len().await, 0);
}

#[tokio::test]
async fn queue_persists_across_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let queue_path = dir.path().join("q.json");
    {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/event-batch"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;
        let reporter = PlayReporter::open(client(&server), queue_path.clone(), true)
            .unwrap()
            .event_base(&format!("{}/", server.uri()))
            .unwrap();
        reporter.record(event(60_000, "FULL")).await;
        assert_eq!(reporter.pending_len().await, 1);
        // `reporter` (and the first server) is dropped here: simulates a
        // process restart with the event still sitting on disk.
    }
    let server2 = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/event-batch"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server2)
        .await;
    let reporter2 = PlayReporter::open(client(&server2), queue_path, true)
        .unwrap()
        .event_base(&format!("{}/", server2.uri()))
        .unwrap();
    assert_eq!(
        reporter2.pending_len().await,
        1,
        "the leftover event must load from the on-disk queue"
    );
    let outcome = reporter2.try_flush().await.unwrap();
    assert_eq!(outcome.sent, 1);
    assert_eq!(reporter2.pending_len().await, 0);
}
