//! Play reporting: tells TIDAL a track was actually listened to, so
//! Recently Played and Home personalisation behave the way a subscriber
//! expects (D-027). On by default, disableable via `Settings::play_reporting`,
//! disclosed in README.md.
//!
//! Wire format in full: `tidal-api/references/play-logging-and-privileges.md`
//! §1-4, restated here only where a field or rule is actually implemented;
//! audio-pipeline-specific consequences (PREVIEW suppression, the
//! `streamingSessionId` join key) are `audio-pipeline/references/
//! playback-behavior.md` §7 and §15. Every field below is one the reference
//! documents by name; where it does not document a field's *value* for a
//! case streamboat can hit (see `event_identity` below), that is called out
//! rather than invented.
//!
//! Operational rules (D-027), all fixed regardless of the enabled default:
//! log a play only past the 30-second threshold; never for a PREVIEW asset;
//! drop permanently on a sender-fault batch error, never retry it; derive
//! timestamps from a server-anchored clock (`GET /v1/ping`), not the local
//! clock; persist the outbox so a play recorded just before a crash or
//! restart is not lost.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;

use crate::credentials::CredentialSource;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::http::ApiClient;
use crate::models::{AudioMode, AudioQuality};
use crate::token_store::TokenSet;

/// `https://ec.tidal.com/api/event-batch` — the event producer TIDAL's own
/// clients log plays to (§1). A different host from `api.tidal.com`.
pub const DEFAULT_EVENT_BASE: &str = "https://ec.tidal.com/";
/// "Log a play only past 30 seconds" (§3, Sone's own test comment: "a play
/// over 30 seconds counts as a stream") — strictly past, not at, 30s.
pub const REPORT_THRESHOLD_MS: u64 = 30_000;
/// AWS SQS `SendMessageBatch`'s own limit, which this wire format inherits (§1).
pub const EVENT_BATCH_MAX: usize = 10;

// ---------------------------------------------------------------------------
// Server-anchored clock (§4)

/// A clock anchored to TIDAL's server time via `GET /v1/ping`'s `Date`
/// header, refreshed at most hourly, exactly like `@tidal-music/true-time`
/// (§4). Shared by [`crate::privileges::StreamingPrivileges`] for its
/// `USER_ACTION.startedAt` and by [`PlayReporter`] for event timestamps —
/// both need "epoch ms from a true-time source," not `SystemTime::now()`.
pub struct ServerClock {
    state: Mutex<Option<ClockState>>,
}

struct ClockState {
    /// `server_ms - local_ms` at the moment of the last successful ping.
    offset_ms: i64,
    fetched_at: std::time::Instant,
}

const REFRESH_INTERVAL: Duration = Duration::from_secs(3600);

impl Default for ServerClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerClock {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(None),
        }
    }

    /// The current time, in epoch milliseconds, anchored to TIDAL's server
    /// clock when reachable. Never blocks the caller on a slow network
    /// longer than one HTTP request: on failure this falls back to the last
    /// known offset, or to the local clock if no offset was ever captured —
    /// this must never itself stop a claim or a play report.
    pub async fn now_ms(&self, api: &ApiClient) -> u64 {
        let mut guard = self.state.lock().await;
        let stale = guard
            .as_ref()
            .map(|s| s.fetched_at.elapsed() > REFRESH_INTERVAL)
            .unwrap_or(true);
        if stale {
            match Self::fetch_offset(api).await {
                Ok(offset_ms) => {
                    *guard = Some(ClockState {
                        offset_ms,
                        fetched_at: std::time::Instant::now(),
                    })
                }
                Err(e) => {
                    tracing::debug!(error = %e, "v1/ping failed; using cached or local clock");
                }
            }
        }
        let local = local_now_ms();
        match guard.as_ref() {
            Some(s) => (local as i64 + s.offset_ms).max(0) as u64,
            None => local,
        }
    }

    async fn fetch_offset(api: &ApiClient) -> Result<i64> {
        let before = local_now_ms() as i64;
        let date_header = api.ping_server_date().await?;
        let server_time = httpdate::parse_http_date(&date_header)
            .map_err(|e| Error::Manifest(format!("v1/ping Date header {date_header:?}: {e}")))?;
        let server_ms = server_time
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Ok(server_ms - before)
    }
}

fn local_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// The event itself

/// The album/playlist/artist/mix a play happened from, when the caller
/// knows one. TIDAL's own field is omitted entirely (not sent as null) when
/// unknown (§3: "a sourceless play is accepted but produces no
/// Recently-Played row").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaySource {
    pub source_type: &'static str,
    pub source_id: String,
}

/// One finished or skipped play, as the player observed it. Built by the
/// caller (`streamboat-player`) from the resolved stream and the queue
/// entry; [`PlayReporter::record`] applies the threshold and PREVIEW policy
/// and takes it from there.
#[derive(Debug, Clone)]
pub struct PlayEvent {
    pub track_id: u64,
    /// The same id sent as `x-tidal-streamingsessionid` on the
    /// `playbackinfopostpaywall` call for this play — the reference leaves
    /// open whether TIDAL's backend actually correlates the two, and notes
    /// reusing one uuid for both is the cheapest safe move (§3, last
    /// paragraph); this field is that shared id.
    pub streaming_session_id: String,
    /// TIDAL's own `assetPresentation` for the resolved manifest. Anything
    /// other than `"FULL"` (i.e. a preview) is never reported, regardless
    /// of duration (`playback-behavior.md` §7/§15).
    pub asset_presentation: String,
    pub audio_quality: Option<AudioQuality>,
    pub audio_mode: Option<AudioMode>,
    pub start_timestamp_ms: u64,
    pub end_timestamp_ms: u64,
    pub start_position_s: f64,
    pub end_position_s: f64,
    pub source: Option<PlaySource>,
}

impl PlayEvent {
    pub fn played_ms(&self) -> u64 {
        self.end_timestamp_ms
            .saturating_sub(self.start_timestamp_ms)
    }

    fn is_preview(&self) -> bool {
        !self.asset_presentation.eq_ignore_ascii_case("FULL")
    }
}

fn audio_mode_str(m: AudioMode) -> &'static str {
    match m {
        AudioMode::Stereo => "STEREO",
        AudioMode::DolbyAtmos => "DOLBY_ATMOS",
        AudioMode::Sony360Ra => "SONY_360RA",
        AudioMode::Unknown => "STEREO",
    }
}

/// Whether this session's client id is the maintainer-embedded default pair
/// (D-023: "an optional build-time default") — the community-known
/// device-code/PKCE pair that `python-tidal`, Sone and High Tide also
/// embed, which really is one of TIDAL's own app credentials, making Sone's
/// pinned Android identity (§2) an *honest* description of that specific
/// credential. A `Settings`/`Environment`/`Explicit`-sourced client id is
/// the user's own, and this project has no basis to claim it is TIDAL's
/// Android app — D-027: "the payload must follow the credential in use."
fn event_identity(api: &ApiClient) -> EventIdentity {
    if api.credentials().source == CredentialSource::BuildTime {
        // Sone's pinned shape (§2), accurate only because that credential
        // really is the Android client it names.
        EventIdentity {
            device_type: "mobile",
            platform: "android",
            app_version: "2.205.0",
            device_model: "Pixel 7",
            device_vendor: "Google",
        }
    } else {
        EventIdentity {
            device_type: "desktop",
            platform: platform_name(),
            app_version: crate::VERSION,
            device_model: "streamboat",
            device_vendor: "streamboat",
        }
    }
}

struct EventIdentity {
    device_type: &'static str,
    platform: &'static str,
    app_version: &'static str,
    device_model: &'static str,
    device_vendor: &'static str,
}

fn platform_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        "linux" => "linux",
        other => other,
    }
}

/// One SQS `SendMessageBatchRequestEntry` worth of data, persisted to the
/// on-disk outbox exactly as it will be sent (§1): the whole point of the
/// queue file is that a crash between building the entry and sending it
/// never needs the original [`PlayEvent`] again.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueuedEvent {
    id: String,
    body: serde_json::Value,
    headers: serde_json::Value,
}

fn build_event(
    event: &PlayEvent,
    tokens: &Option<TokenSet>,
    identity: &EventIdentity,
    access_token: &str,
    now_ms: u64,
) -> QueuedEvent {
    let id = Uuid::new_v4().to_string();
    let client_id = tokens
        .as_ref()
        .map(|t| t.client_id.clone())
        .unwrap_or_default();
    let user_id = tokens.as_ref().and_then(|t| t.user_id);

    let mut payload = json!({
        "playbackSessionId": event.streaming_session_id,
        "isPostPaywall": true,
        "productType": "TRACK",
        "requestedProductId": event.track_id.to_string(),
        "actualProductId": event.track_id.to_string(),
        "actualAssetPresentation": event.asset_presentation,
        "startTimestamp": event.start_timestamp_ms,
        "endTimestamp": event.end_timestamp_ms,
        "startAssetPosition": event.start_position_s,
        "endAssetPosition": event.end_position_s,
        "actions": [],
    });
    if let Some(q) = event.audio_quality {
        payload["actualQuality"] = json!(q.as_str());
    }
    if let Some(m) = event.audio_mode {
        payload["actualAudioMode"] = json!(audio_mode_str(m));
    }
    // Omitted entirely when unknown, never sent as null (§3).
    if let Some(src) = &event.source {
        payload["sourceType"] = json!(src.source_type);
        payload["sourceId"] = json!(src.source_id);
    }

    let body = json!({
        "group": "play_log",
        "version": 2,
        "ts": event.end_timestamp_ms,
        "uuid": id,
        "user": {
            "id": user_id,
            "clientId": client_id,
            "sessionId": event.streaming_session_id,
        },
        "client": {
            "token": client_id,
            "deviceType": identity.device_type,
            "version": identity.app_version,
            "platform": identity.platform,
        },
        "payload": payload,
    });

    // The nine keys the reference documents exactly (§2). `os-version` has
    // no honest cross-platform value available without a new dependency
    // this crate does not otherwise need; left empty rather than guessed.
    let headers = json!({
        "client-id": client_id,
        "app-version": identity.app_version,
        "os-name": identity.platform,
        "os-version": "",
        "device-model": identity.device_model,
        "device-vendor": identity.device_vendor,
        "consent-category": "NECESSARY",
        "requested-sent-timestamp": now_ms.to_string(),
        "authorization": access_token,
    });

    QueuedEvent { id, body, headers }
}

fn build_form(batch: &[QueuedEvent]) -> Vec<(String, String)> {
    let mut form = Vec::with_capacity(batch.len() * 8);
    for (i, e) in batch.iter().enumerate() {
        let n = i + 1;
        form.push((format!("SendMessageBatchRequestEntry.{n}.Id"), e.id.clone()));
        form.push((
            format!("SendMessageBatchRequestEntry.{n}.MessageBody"),
            e.body.to_string(),
        ));
        form.push((
            format!("SendMessageBatchRequestEntry.{n}.MessageAttribute.1.Name"),
            "Name".to_string(),
        ));
        form.push((
            format!("SendMessageBatchRequestEntry.{n}.MessageAttribute.1.Value.StringValue"),
            "playback_session".to_string(),
        ));
        form.push((
            format!("SendMessageBatchRequestEntry.{n}.MessageAttribute.1.Value.DataType"),
            "String".to_string(),
        ));
        form.push((
            format!("SendMessageBatchRequestEntry.{n}.MessageAttribute.2.Name"),
            "Headers".to_string(),
        ));
        form.push((
            format!("SendMessageBatchRequestEntry.{n}.MessageAttribute.2.Value.StringValue"),
            e.headers.to_string(),
        ));
        form.push((
            format!("SendMessageBatchRequestEntry.{n}.MessageAttribute.2.Value.DataType"),
            "String".to_string(),
        ));
    }
    form
}

/// A batch's outcome (§3's outcome classification, folded down to what the
/// queue needs to do next).
enum BatchOutcome {
    /// A network error, a 5xx, or a 401/403 that was refreshed: keep every
    /// entry queued and try again later.
    Retry,
    /// `succeeded` were accepted (or the response gave no way to tell, in
    /// which case a 2xx is trusted); `dropped` hit a `BatchResultErrorEntry`
    /// or another 4xx and must never be retried (§3: "drop permanently,
    /// never retry").
    Result {
        succeeded: Vec<String>,
        dropped: Vec<String>,
    },
}

/// Parses the AWS SQS `SendMessageBatch` XML response shape this endpoint's
/// request format implies (`<SendMessageBatchResultEntry><Id>…`,
/// `<BatchResultErrorEntry><Id>…`). The reference documents the *request*
/// shape in full but not this response shape; when the body does not parse
/// as this XML (including an empty body), every id in the batch is treated
/// as accepted, since the only documented failure signal above HTTP status
/// is a `BatchResultErrorEntry` — there is nothing else to check for.
fn parse_batch_response(text: &str, batch: &[QueuedEvent]) -> (Vec<String>, Vec<String>) {
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return (batch.iter().map(|e| e.id.clone()).collect(), Vec::new());
    };
    let mut succeeded = Vec::new();
    let mut dropped = Vec::new();
    for node in doc.descendants() {
        let child_text = |name: &str| {
            node.children()
                .find(|c| c.has_tag_name(name))
                .and_then(|c| c.text())
                .map(str::to_string)
        };
        match node.tag_name().name() {
            "SendMessageBatchResultEntry" => {
                if let Some(id) = child_text("Id") {
                    succeeded.push(id);
                }
            }
            "BatchResultErrorEntry" => {
                if let Some(id) = child_text("Id") {
                    dropped.push(id);
                }
            }
            _ => {}
        }
    }
    if succeeded.is_empty() && dropped.is_empty() {
        // Well-formed XML that just isn't this shape: fall back to trusting
        // the HTTP status, same as an unparseable body.
        return (batch.iter().map(|e| e.id.clone()).collect(), Vec::new());
    }
    (succeeded, dropped)
}

/// What one [`PlayReporter::try_flush`] call did, for tests and logging.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FlushOutcome {
    pub sent: usize,
    pub dropped: usize,
    pub retry_pending: bool,
}

/// Builds and sends the documented event-batch payload for finished or
/// skipped plays, with a persistent, retrying outbox.
pub struct PlayReporter {
    api: ApiClient,
    http: reqwest::Client,
    event_base: Url,
    clock: ServerClock,
    enabled: AtomicBool,
    queue_path: PathBuf,
    queue: Mutex<VecDeque<QueuedEvent>>,
}

impl PlayReporter {
    /// Opens (or creates) the persistent outbox at `queue_path`, loading
    /// any events left over from a previous run so they are retried rather
    /// than lost.
    pub fn open(api: ApiClient, queue_path: PathBuf, enabled: bool) -> Result<Self> {
        let queue = load_queue(&queue_path)?;
        let http = reqwest::Client::builder()
            .user_agent(api.user_agent())
            .build()?;
        Ok(Self {
            api,
            http,
            event_base: Url::parse(DEFAULT_EVENT_BASE).expect("valid default event base"),
            clock: ServerClock::new(),
            enabled: AtomicBool::new(enabled),
            queue_path,
            queue: Mutex::new(queue),
        })
    }

    /// Override the event-batch host (tests only; production always uses
    /// [`DEFAULT_EVENT_BASE`]).
    pub fn event_base(mut self, url: &str) -> Result<Self> {
        self.event_base =
            Url::parse(url).map_err(|e| Error::Config(format!("bad event base: {e}")))?;
        Ok(self)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub async fn pending_len(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// The server-anchored clock this reporter uses, for a caller
    /// (`streamboat-player`) that needs to stamp a play's start/end time
    /// with the same clock the eventual report will use.
    pub async fn now_ms(&self) -> u64 {
        self.clock.now_ms(&self.api).await
    }

    /// Apply the reporting policy to one play and, if it survives, enqueue
    /// and attempt an immediate best-effort flush. A play that is disabled,
    /// a PREVIEW, or shorter than [`REPORT_THRESHOLD_MS`] is silently
    /// dropped — this is policy, not a bug to fix by retrying it.
    pub async fn record(&self, event: PlayEvent) {
        if !self.is_enabled() {
            return;
        }
        if event.is_preview() {
            tracing::debug!(track_id = event.track_id, "PREVIEW play: not reported");
            return;
        }
        let played = event.played_ms();
        if played <= REPORT_THRESHOLD_MS {
            tracing::debug!(
                track_id = event.track_id,
                played_ms = played,
                "play at or under the 30s threshold: not reported"
            );
            return;
        }
        let now_ms = self.clock.now_ms(&self.api).await;
        let tokens = self.api.tokens().await;
        let identity = event_identity(&self.api);
        let access_token = self.api.access_token().await.unwrap_or_default();
        let queued = build_event(&event, &tokens, &identity, &access_token, now_ms);
        {
            let mut q = self.queue.lock().await;
            q.push_back(queued);
            let _ = persist(&self.queue_path, &q);
        }
        let _ = self.try_flush().await;
    }

    /// Send up to [`EVENT_BATCH_MAX`] queued events in one batch. Returns
    /// without error when the queue is empty. A network error or a 5xx
    /// leaves the queue untouched for a later call; a
    /// `BatchResultErrorEntry` or another 4xx drops just that entry,
    /// permanently (§3).
    pub async fn try_flush(&self) -> Result<FlushOutcome> {
        let batch: Vec<QueuedEvent> = {
            let q = self.queue.lock().await;
            q.iter().take(EVENT_BATCH_MAX).cloned().collect()
        };
        if batch.is_empty() {
            return Ok(FlushOutcome::default());
        }
        match self.send_batch(&batch).await {
            Ok(BatchOutcome::Retry) => Ok(FlushOutcome {
                retry_pending: true,
                ..Default::default()
            }),
            Ok(BatchOutcome::Result { succeeded, dropped }) => {
                let mut q = self.queue.lock().await;
                q.retain(|e| !succeeded.contains(&e.id) && !dropped.contains(&e.id));
                persist(&self.queue_path, &q)?;
                Ok(FlushOutcome {
                    sent: succeeded.len(),
                    dropped: dropped.len(),
                    retry_pending: false,
                })
            }
            Err(_network) => Ok(FlushOutcome {
                retry_pending: true,
                ..Default::default()
            }),
        }
    }

    async fn send_batch(&self, batch: &[QueuedEvent]) -> Result<BatchOutcome> {
        let url = self
            .event_base
            .join("api/event-batch")
            .map_err(|e| Error::Config(format!("bad event-batch url: {e}")))?;
        let form = build_form(batch);
        let resp = self.http.post(url).form(&form).send().await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if status.is_success() {
            let (succeeded, dropped) = parse_batch_response(&text, batch);
            return Ok(BatchOutcome::Result { succeeded, dropped });
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            // §3: "401/403 -> refresh once then queue." Best-effort: the
            // refreshed token is picked up on the next flush attempt.
            let _ = self.api.refresh_tokens().await;
            return Ok(BatchOutcome::Retry);
        }
        if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Ok(BatchOutcome::Retry);
        }
        // "other 4xx ... -> drop permanently" (§3).
        Ok(BatchOutcome::Result {
            succeeded: Vec::new(),
            dropped: batch.iter().map(|e| e.id.clone()).collect(),
        })
    }
}

fn load_queue(path: &std::path::Path) -> Result<VecDeque<QueuedEvent>> {
    match std::fs::read(path) {
        Ok(bytes) if bytes.is_empty() => Ok(VecDeque::new()),
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(VecDeque::new()),
        Err(e) => Err(e.into()),
    }
}

fn persist(path: &std::path::Path, queue: &VecDeque<QueuedEvent>) -> Result<()> {
    let items: Vec<&QueuedEvent> = queue.iter().collect();
    let json = serde_json::to_vec(&items)?;
    fsutil::atomic_write(path, &json, 0o600)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_form_shape_matches_the_documented_sqs_fields() {
        let batch = vec![QueuedEvent {
            id: "abc".into(),
            body: json!({"k": "v"}),
            headers: json!({"h": "v"}),
        }];
        let form = build_form(&batch);
        let keys: Vec<&str> = form.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "SendMessageBatchRequestEntry.1.Id",
                "SendMessageBatchRequestEntry.1.MessageBody",
                "SendMessageBatchRequestEntry.1.MessageAttribute.1.Name",
                "SendMessageBatchRequestEntry.1.MessageAttribute.1.Value.StringValue",
                "SendMessageBatchRequestEntry.1.MessageAttribute.1.Value.DataType",
                "SendMessageBatchRequestEntry.1.MessageAttribute.2.Name",
                "SendMessageBatchRequestEntry.1.MessageAttribute.2.Value.StringValue",
                "SendMessageBatchRequestEntry.1.MessageAttribute.2.Value.DataType",
            ]
        );
    }

    #[test]
    fn headers_json_has_exactly_the_nine_documented_keys() {
        let identity = EventIdentity {
            device_type: "desktop",
            platform: "linux",
            app_version: "0.0.1",
            device_model: "streamboat",
            device_vendor: "streamboat",
        };
        let event = sample_event(31_000);
        let queued = build_event(&event, &None, &identity, "tok", 42);
        let mut keys: Vec<&str> = queued
            .headers
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();
        keys.sort_unstable();
        let mut expected = vec![
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
        assert_eq!(queued.headers["authorization"], "tok");
        assert_eq!(queued.headers["consent-category"], "NECESSARY");
    }

    fn sample_event(played_ms: u64) -> PlayEvent {
        PlayEvent {
            track_id: 7,
            streaming_session_id: "sid-1".into(),
            asset_presentation: "FULL".into(),
            audio_quality: Some(AudioQuality::Lossless),
            audio_mode: Some(AudioMode::Stereo),
            start_timestamp_ms: 1_000,
            end_timestamp_ms: 1_000 + played_ms,
            start_position_s: 0.0,
            end_position_s: played_ms as f64 / 1000.0,
            source: None,
        }
    }

    #[test]
    fn parse_batch_response_reads_sender_fault_entries() {
        let batch = vec![
            QueuedEvent {
                id: "ok-1".into(),
                body: json!({}),
                headers: json!({}),
            },
            QueuedEvent {
                id: "bad-1".into(),
                body: json!({}),
                headers: json!({}),
            },
        ];
        let xml = r#"<SendMessageBatchResponse>
            <SendMessageBatchResult>
                <SendMessageBatchResultEntry><Id>ok-1</Id><MessageId>m1</MessageId></SendMessageBatchResultEntry>
                <BatchResultErrorEntry><Id>bad-1</Id><SenderFault>true</SenderFault><Code>MalformedInput</Code></BatchResultErrorEntry>
            </SendMessageBatchResult>
        </SendMessageBatchResponse>"#;
        let (succeeded, dropped) = parse_batch_response(xml, &batch);
        assert_eq!(succeeded, vec!["ok-1".to_string()]);
        assert_eq!(dropped, vec!["bad-1".to_string()]);
    }

    #[test]
    fn parse_batch_response_trusts_2xx_when_unparseable() {
        let batch = vec![QueuedEvent {
            id: "a".into(),
            body: json!({}),
            headers: json!({}),
        }];
        let (succeeded, dropped) = parse_batch_response("", &batch);
        assert_eq!(succeeded, vec!["a".to_string()]);
        assert!(dropped.is_empty());
    }

    #[test]
    fn played_ms_and_preview_detection() {
        let mut e = sample_event(45_000);
        assert_eq!(e.played_ms(), 45_000);
        assert!(!e.is_preview());
        e.asset_presentation = "PREVIEW".into();
        assert!(e.is_preview());
    }

    #[tokio::test]
    async fn queue_survives_a_process_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("play_reports.json");
        let mut q = VecDeque::new();
        q.push_back(QueuedEvent {
            id: "leftover".into(),
            body: json!({"x": 1}),
            headers: json!({}),
        });
        persist(&path, &q).unwrap();
        let loaded = load_queue(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "leftover");
    }
}
