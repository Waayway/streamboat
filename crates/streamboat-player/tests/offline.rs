//! Integration tests for the pinned, encrypted offline cache (D-022),
//! through the public API only — mirrors `tests/player.rs`'s style (a real
//! wiremock TIDAL, a real `ApiClient`), with a `MemoryKeySlot` standing in
//! for the OS keyring per the task's request.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use serde_json::json;
use sha2::Digest as _;
use streamboat_core::config::{AppDirs, Settings};
use streamboat_core::proto::PinKind;
use streamboat_core::token_store::{
    KeyStorage, MemoryKeySlot, MemoryTokenStore, TokenSet, now_secs,
};
use streamboat_core::{ApiClient, AudioQuality, AuthFlow, ClientCredentials};
use streamboat_player::offline::{OfflineCache, OfflineError};
use wiremock::matchers::{method, path as wpath};
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

fn dirs(tmp: &Path) -> AppDirs {
    AppDirs {
        config: tmp.join("config"),
        data: tmp.join("data"),
        cache: tmp.join("cache"),
        runtime: tmp.join("run"),
    }
}

fn settings(offline_dir: PathBuf, max_bytes: Option<u64>, validity_days: u32) -> Settings {
    Settings {
        offline_dir: Some(offline_dir),
        offline_max_bytes: max_bytes,
        offline_validity_days: validity_days,
        key_storage: KeyStorage::Auto,
        ..Settings::default()
    }
}

/// Opens a cache backed by a fresh `MemoryKeySlot` (never a real OS
/// keyring) — the task's requested test double for the offline cache key.
async fn open_cache(tmp: &Path, max_bytes: Option<u64>, validity_days: u32) -> Arc<OfflineCache> {
    let d = dirs(tmp);
    let s = settings(tmp.join("offline"), max_bytes, validity_days);
    OfflineCache::open_with_key_slot(
        &d,
        &s,
        "device-under-test",
        Arc::new(MemoryKeySlot::default()),
    )
    .await
    .unwrap()
}

fn bts_manifest_body(url: &str) -> serde_json::Value {
    let manifest =
        json!({"mimeType":"audio/flac","codecs":"flac","encryptionType":"NONE","urls":[url]});
    json!({
        "trackId": 1, "assetPresentation": "FULL", "audioMode": "STEREO",
        "audioQuality": "LOSSLESS", "manifestMimeType": "application/vnd.tidal.bts",
        "manifest": base64::engine::general_purpose::STANDARD.encode(manifest.to_string()),
        "bitDepth": 16, "sampleRate": 44100, "trackReplayGain": -2.0, "trackPeakAmplitude": 0.9
    })
}

async fn mount_track(server: &MockServer, id: u64, title: &str) {
    Mock::given(method("GET"))
        .and(wpath(format!("/v1/tracks/{id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": id, "title": title, "duration": 200,
            "artists": [{"id": 9, "name": "Artist"}],
            "album": {"id": 5, "title": "Album", "cover": "c"}
        })))
        .mount(server)
        .await;
}

/// No request the mock saw ever asked for a licensed/DRM-bound download —
/// only ever `playbackmode=STREAM`, never `playbackmode=OFFLINE` or
/// `usage=DOWNLOAD` (D-022).
async fn assert_never_requested_offline_download(server: &MockServer) {
    for req in server.received_requests().await.unwrap() {
        let query = req.url.query().unwrap_or("");
        assert!(!query.contains("playbackmode=OFFLINE"), "{query}");
        assert!(!query.contains("usage=DOWNLOAD"), "{query}");
    }
}

/// End to end: pin a track, its chunk files hold no plaintext, playback
/// through the loopback route round-trips the exact bytes (by hash),
/// `Range` requests work, and unpinning deletes the chunks.
#[tokio::test]
async fn pin_playback_range_and_unpin_round_trip() {
    let server = MockServer::start().await;
    mount_track(&server, 1, "One").await;
    let plaintext = b"THIS-IS-NOT-ENCRYPTED-YET-flac-audio-bytes-0123456789";
    Mock::given(method("GET"))
        .and(wpath("/audio/one.flac"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(plaintext.to_vec())
                .insert_header("content-type", "audio/flac"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wpath("/v1/tracks/1/playbackinfopostpaywall"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(bts_manifest_body(&format!(
                "{}/audio/one.flac",
                server.uri()
            ))),
        )
        .mount(&server)
        .await;
    let api = client(&server);
    let dir = tempfile::tempdir().unwrap();
    let cache = open_cache(dir.path(), Some(1_000_000_000), 30).await;

    let mut progress = Vec::new();
    cache
        .pin(
            &api,
            PinKind::Track,
            "1".into(),
            AudioQuality::HiResLossless,
            |done, total| progress.push((done, total)),
        )
        .await
        .unwrap();
    assert_eq!(progress, vec![(1, 1)]);

    let pins = cache.list_pins();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].kind, PinKind::Track);
    assert!(pins[0].valid);
    assert_eq!(pins[0].bytes, plaintext.len() as u64);

    // No chunk file on disk holds the plaintext, or a substring of it.
    let chunks_dir = dir.path().join("offline").join("chunks");
    let mut saw_a_chunk = false;
    for entry in std::fs::read_dir(&chunks_dir).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        for needle in [&plaintext[..], b"NOT-ENCRYPTED", b"flac-audio"] {
            assert!(
                !bytes.windows(needle.len().max(1)).any(|w| w == needle),
                "a chunk file leaks the plaintext or a substring of it"
            );
        }
        saw_a_chunk = true;
    }
    assert!(saw_a_chunk, "pinning wrote no chunk files");

    // Playback via the loopback route round-trips the exact bytes.
    let served = cache.serve_track(&api, 1).await.expect("cache hit");
    assert!(served.url.starts_with("http://127.0.0.1:"));
    let got = reqwest::get(&served.url).await.unwrap();
    assert!(got.status().is_success());
    let got_bytes = got.bytes().await.unwrap();
    assert_eq!(got_bytes.as_ref(), plaintext);
    assert_eq!(
        sha2::Sha256::digest(&got_bytes).as_slice(),
        sha2::Sha256::digest(plaintext).as_slice(),
        "playback bytes must hash identically to the original stream"
    );

    // Range requests work.
    let http = reqwest::Client::new();
    let ranged = http
        .get(&served.url)
        .header("Range", "bytes=5-14")
        .send()
        .await
        .unwrap();
    assert_eq!(ranged.status().as_u16(), 206);
    let content_range = ranged
        .headers()
        .get("content-range")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(content_range.starts_with("bytes 5-14/"), "{content_range}");
    let ranged_bytes = ranged.bytes().await.unwrap();
    assert_eq!(ranged_bytes.as_ref(), &plaintext[5..15]);

    // A wrong token or a non-loopback-looking request path is refused.
    let bad_token = format!("http://127.0.0.1:{}/not-the-token/1", cache.loopback_port());
    let resp = reqwest::get(&bad_token).await.unwrap();
    assert_eq!(resp.status().as_u16(), 403);

    // Unpin deletes the chunks.
    assert!(cache.unpin(PinKind::Track, "1").unwrap());
    assert!(cache.list_pins().is_empty());
    let remaining: Vec<_> = std::fs::read_dir(&chunks_dir).unwrap().collect();
    assert!(remaining.is_empty(), "unpin left chunk files behind");

    assert_never_requested_offline_download(&server).await;
}

/// A wrong device key (a copy on another install) cannot decrypt an
/// existing pin's chunks and is never served for playback.
#[tokio::test]
async fn wrong_device_key_fails_to_decrypt() {
    let server = MockServer::start().await;
    mount_track(&server, 1, "One").await;
    Mock::given(method("GET"))
        .and(wpath("/audio/one.flac"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello world".to_vec()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wpath("/v1/tracks/1/playbackinfopostpaywall"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(bts_manifest_body(&format!(
                "{}/audio/one.flac",
                server.uri()
            ))),
        )
        .mount(&server)
        .await;
    let api = client(&server);
    let dir = tempfile::tempdir().unwrap();
    let cache_a = open_cache(dir.path(), Some(1_000_000_000), 30).await;
    cache_a
        .pin(
            &api,
            PinKind::Track,
            "1".into(),
            AudioQuality::HiResLossless,
            |_, _| {},
        )
        .await
        .unwrap();

    // A second "install" pointed at the same directory, but its own fresh
    // `MemoryKeySlot` — a different device-bound file key, same as copying
    // `offline/` to another machine.
    let d = dirs(dir.path());
    let s = settings(dir.path().join("offline"), Some(1_000_000_000), 30);
    let cache_b = OfflineCache::open_with_key_slot(
        &d,
        &s,
        "a-different-device",
        Arc::new(MemoryKeySlot::default()),
    )
    .await
    .unwrap();
    assert!(
        cache_b.serve_track(&api, 1).await.is_none(),
        "a wrong device key must not be served for playback"
    );
}

/// A PREVIEW asset is refused before anything is written to disk.
#[tokio::test]
async fn preview_asset_is_refused() {
    let server = MockServer::start().await;
    mount_track(&server, 1, "One").await;
    let mut body = bts_manifest_body(&format!("{}/audio/one.flac", server.uri()));
    body["assetPresentation"] = json!("PREVIEW");
    body["previewReason"] = json!("NOT_ON_DEMAND");
    Mock::given(method("GET"))
        .and(wpath("/v1/tracks/1/playbackinfopostpaywall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let api = client(&server);
    let dir = tempfile::tempdir().unwrap();
    let cache = open_cache(dir.path(), None, 30).await;
    let err = cache
        .pin(
            &api,
            PinKind::Track,
            "1".into(),
            AudioQuality::HiResLossless,
            |_, _| {},
        )
        .await
        .unwrap_err();
    assert!(matches!(err, OfflineError::Refused(_)), "{err}");
    assert!(cache.list_pins().is_empty());
}

/// An encrypted manifest at every tier is refused — core's existing
/// refusal rule, reused unchanged inside `resolve_stream` — and the pin
/// never touches the (should-not-be-fetched) CDN URL inside it. No request
/// the mock saw ever asked for `playbackmode=OFFLINE`/`usage=DOWNLOAD`.
#[tokio::test]
async fn encrypted_manifest_is_refused_and_never_requests_offline_download() {
    let server = MockServer::start().await;
    mount_track(&server, 1, "One").await;
    let manifest = json!({
        "codecs": "flac", "encryptionType": "OLD_AES", "keyId": "abc",
        "urls": ["https://cdn.example/should-not-be-fetched"]
    });
    let body = json!({
        "trackId": 1, "assetPresentation": "FULL",
        "manifestMimeType": "application/vnd.tidal.bts",
        "manifest": base64::engine::general_purpose::STANDARD.encode(manifest.to_string()),
    });
    Mock::given(method("GET"))
        .and(wpath("/v1/tracks/1/playbackinfopostpaywall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let api = client(&server);
    let dir = tempfile::tempdir().unwrap();
    let cache = open_cache(dir.path(), None, 30).await;
    let err = cache
        .pin(
            &api,
            PinKind::Track,
            "1".into(),
            AudioQuality::HiResLossless,
            |_, _| {},
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("does not decrypt"), "{err}");
    assert!(cache.list_pins().is_empty());
    assert_never_requested_offline_download(&server).await;
}

/// The size cap refuses a new pin and leaves no chunks behind.
#[tokio::test]
async fn size_cap_refuses_and_cleans_up() {
    let server = MockServer::start().await;
    mount_track(&server, 1, "One").await;
    Mock::given(method("GET"))
        .and(wpath("/audio/one.flac"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 2048]))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wpath("/v1/tracks/1/playbackinfopostpaywall"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(bts_manifest_body(&format!(
                "{}/audio/one.flac",
                server.uri()
            ))),
        )
        .mount(&server)
        .await;
    let api = client(&server);
    let dir = tempfile::tempdir().unwrap();
    let cache = open_cache(dir.path(), Some(100), 30).await;
    let err = cache
        .pin(
            &api,
            PinKind::Track,
            "1".into(),
            AudioQuality::HiResLossless,
            |_, _| {},
        )
        .await
        .unwrap_err();
    assert!(matches!(err, OfflineError::Cache(_)), "{err}");
    assert!(err.to_string().contains("cap"));
    assert!(cache.list_pins().is_empty());
    let chunks_dir = dir.path().join("offline").join("chunks");
    let chunks: Vec<_> = std::fs::read_dir(&chunks_dir).unwrap().collect();
    assert!(
        chunks.is_empty(),
        "a refused pin must not leave chunks behind"
    );
}

/// Logout-style wipes remove every pin and its chunks: both `wipe_all` on
/// an open cache, and `wipe_dir` for the CLI's "no cache instance open"
/// logout hook.
#[tokio::test]
async fn logout_wipes_the_cache() {
    let server = MockServer::start().await;
    mount_track(&server, 1, "One").await;
    Mock::given(method("GET"))
        .and(wpath("/audio/one.flac"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"abc".to_vec()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wpath("/v1/tracks/1/playbackinfopostpaywall"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(bts_manifest_body(&format!(
                "{}/audio/one.flac",
                server.uri()
            ))),
        )
        .mount(&server)
        .await;
    let api = client(&server);
    let dir = tempfile::tempdir().unwrap();
    let cache = open_cache(dir.path(), Some(1_000_000_000), 30).await;
    cache
        .pin(
            &api,
            PinKind::Track,
            "1".into(),
            AudioQuality::HiResLossless,
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(cache.list_pins().len(), 1);

    // The in-process wipe (Player's logout hook when a cache is open).
    cache.wipe_all().unwrap();
    assert!(cache.list_pins().is_empty());
    let chunks_dir = dir.path().join("offline").join("chunks");
    assert!(std::fs::read_dir(&chunks_dir).unwrap().next().is_none());

    // The CLI's logout hook when no `OfflineCache` is open at all.
    OfflineCache::wipe_dir(&dir.path().join("offline")).unwrap();
    assert!(!dir.path().join("offline").exists());
}

/// Expired validity forces revalidation before serving a pin, and falls
/// back to streaming when the account cannot be revalidated online.
#[tokio::test]
async fn expired_validity_forces_revalidation() {
    let server = MockServer::start().await;
    mount_track(&server, 1, "One").await;
    Mock::given(method("GET"))
        .and(wpath("/audio/one.flac"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"abcdef".to_vec()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wpath("/v1/tracks/1/playbackinfopostpaywall"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(bts_manifest_body(&format!(
                "{}/audio/one.flac",
                server.uri()
            ))),
        )
        .mount(&server)
        .await;
    // No /v1/sessions mock mounted yet: revalidation fails while stale.
    let api = client(&server);
    let dir = tempfile::tempdir().unwrap();
    // A 0-day validity window: every pin is stale the instant it is
    // stored (`validated_at` is `now`, and `now - validated_at (0) >= 0`
    // is always true) — deterministic, unlike waiting for a real clock
    // second to pass.
    let cache = open_cache(dir.path(), Some(1_000_000), 0).await;
    cache
        .pin(
            &api,
            PinKind::Track,
            "1".into(),
            AudioQuality::HiResLossless,
            |_, _| {},
        )
        .await
        .unwrap();
    assert!(
        !cache.list_pins()[0].valid,
        "a 0-day validity window must be stale immediately"
    );
    assert!(
        cache.serve_track(&api, 1).await.is_none(),
        "a stale pin with no way to revalidate must fall back to streaming, not crash"
    );

    // Now the account can be revalidated online: the same pin is served.
    let before = cache.list_pins()[0].validated_at;
    Mock::given(method("GET"))
        .and(wpath("/v1/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sessionId": "s1", "userId": 1, "countryCode": "NL"
        })))
        .mount(&server)
        .await;
    let served = cache.serve_track(&api, 1).await;
    assert!(
        served.is_some(),
        "revalidation should re-enable the cache hit"
    );
    assert!(
        cache.list_pins()[0].validated_at >= before,
        "a successful revalidation must advance validated_at"
    );
}
