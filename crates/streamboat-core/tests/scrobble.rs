//! Wiremock tests for both scrobble backends' request shapes: Last.fm's
//! signed form POST (`track.updateNowPlaying` / `track.scrobble`) and
//! ListenBrainz's `submit-listens` JSON body and bearer-style token header.

use std::sync::Arc;

use streamboat_core::scrobble::{
    LastfmScrobbler, LastfmSettings, ListenBrainzScrobbler, ListenBrainzSettings, ScrobbleTrack,
    Scrobbler,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn track() -> ScrobbleTrack {
    ScrobbleTrack {
        artist: "Test Artist".into(),
        title: "Test Track".into(),
        album: Some("Test Album".into()),
        duration_s: Some(210),
        track_number: Some(3),
        mbid: None,
    }
}

struct Capture(Arc<std::sync::Mutex<Option<Vec<u8>>>>);
impl Respond for Capture {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        *self.0.lock().unwrap() = Some(req.body.clone());
        ResponseTemplate::new(200).set_body_string(r#"{"lfm":{"status":"ok"}}"#)
    }
}

fn form_field(body: &[u8], key: &str) -> Option<String> {
    url::form_urlencoded::parse(body)
        .into_owned()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

#[tokio::test]
async fn lastfm_now_playing_sends_a_signed_form_post() {
    let server = MockServer::start().await;
    let captured = Arc::new(std::sync::Mutex::new(None));
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(Capture(captured.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let settings = LastfmSettings {
        enabled: true,
        api_key: Some("ak".into()),
        api_secret: Some("secret".into()),
        session_key: Some("sk-value".into()),
    };
    let dir = tempfile::tempdir().unwrap();
    let scrobbler = LastfmScrobbler::open(&settings, dir.path().join("q.json"), "streamboat/test")
        .unwrap()
        .unwrap()
        .base(&format!("{}/", server.uri()))
        .unwrap();
    scrobbler.now_playing(&track()).await;

    let body = captured.lock().unwrap().take().unwrap();
    assert_eq!(
        form_field(&body, "method").unwrap(),
        "track.updateNowPlaying"
    );
    assert_eq!(form_field(&body, "artist").unwrap(), "Test Artist");
    assert_eq!(form_field(&body, "track").unwrap(), "Test Track");
    assert_eq!(form_field(&body, "album").unwrap(), "Test Album");
    assert_eq!(form_field(&body, "duration").unwrap(), "210");
    assert_eq!(form_field(&body, "api_key").unwrap(), "ak");
    assert_eq!(form_field(&body, "sk").unwrap(), "sk-value");
    assert_eq!(form_field(&body, "format").unwrap(), "json");
    assert!(form_field(&body, "api_sig").is_some());
}

#[tokio::test]
async fn lastfm_scrobble_is_queued_then_sent_as_track_scrobble() {
    let server = MockServer::start().await;
    let captured = Arc::new(std::sync::Mutex::new(None));
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(Capture(captured.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let settings = LastfmSettings {
        enabled: true,
        api_key: Some("ak".into()),
        api_secret: Some("secret".into()),
        session_key: Some("sk-value".into()),
    };
    let dir = tempfile::tempdir().unwrap();
    let scrobbler = LastfmScrobbler::open(&settings, dir.path().join("q.json"), "streamboat/test")
        .unwrap()
        .unwrap()
        .base(&format!("{}/", server.uri()))
        .unwrap();
    scrobbler.scrobble(&track(), 1_700_000_000).await;
    assert_eq!(scrobbler.pending_len().await, 0);

    let body = captured.lock().unwrap().take().unwrap();
    assert_eq!(form_field(&body, "method").unwrap(), "track.scrobble");
    assert_eq!(form_field(&body, "timestamp").unwrap(), "1700000000");
}

#[tokio::test]
async fn lastfm_disabled_or_missing_credentials_yields_no_backend() {
    let dir = tempfile::tempdir().unwrap();
    let disabled = LastfmSettings::default();
    assert!(
        LastfmScrobbler::open(&disabled, dir.path().join("q.json"), "ua")
            .unwrap()
            .is_none()
    );
    let missing_secret = LastfmSettings {
        enabled: true,
        api_key: Some("ak".into()),
        ..Default::default()
    };
    assert!(
        LastfmScrobbler::open(&missing_secret, dir.path().join("q2.json"), "ua")
            .unwrap()
            .is_none()
    );
}

struct CaptureJson(Arc<std::sync::Mutex<Option<Vec<u8>>>>);
impl Respond for CaptureJson {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        *self.0.lock().unwrap() = Some(req.body.clone());
        ResponseTemplate::new(200).set_body_string("{}")
    }
}

#[tokio::test]
async fn listenbrainz_now_playing_sends_playing_now_with_token_header() {
    let server = MockServer::start().await;
    let captured = Arc::new(std::sync::Mutex::new(None));
    Mock::given(method("POST"))
        .and(path("/1/submit-listens"))
        .and(header("authorization", "Token lb-token"))
        .respond_with(CaptureJson(captured.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let settings = ListenBrainzSettings {
        enabled: true,
        user_token: Some("lb-token".into()),
    };
    let dir = tempfile::tempdir().unwrap();
    let scrobbler =
        ListenBrainzScrobbler::open(&settings, dir.path().join("q.json"), "streamboat/test")
            .unwrap()
            .unwrap()
            .base(&format!("{}/", server.uri()))
            .unwrap();
    scrobbler.now_playing(&track()).await;

    let body = captured.lock().unwrap().take().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["listen_type"], "playing_now");
    assert_eq!(
        json["payload"][0]["track_metadata"]["artist_name"],
        "Test Artist"
    );
    assert_eq!(
        json["payload"][0]["track_metadata"]["track_name"],
        "Test Track"
    );
    assert_eq!(
        json["payload"][0]["track_metadata"]["release_name"],
        "Test Album"
    );
    assert!(json["payload"][0].get("listened_at").is_none());
}

#[tokio::test]
async fn listenbrainz_scrobble_sends_single_with_listened_at() {
    let server = MockServer::start().await;
    let captured = Arc::new(std::sync::Mutex::new(None));
    Mock::given(method("POST"))
        .and(path("/1/submit-listens"))
        .respond_with(CaptureJson(captured.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let settings = ListenBrainzSettings {
        enabled: true,
        user_token: Some("lb-token".into()),
    };
    let dir = tempfile::tempdir().unwrap();
    let scrobbler =
        ListenBrainzScrobbler::open(&settings, dir.path().join("q.json"), "streamboat/test")
            .unwrap()
            .unwrap()
            .base(&format!("{}/", server.uri()))
            .unwrap();
    scrobbler.scrobble(&track(), 1_700_000_000).await;
    assert_eq!(scrobbler.pending_len().await, 0);

    let body = captured.lock().unwrap().take().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["listen_type"], "single");
    assert_eq!(json["payload"][0]["listened_at"], 1_700_000_000);
    assert_eq!(
        json["payload"][0]["track_metadata"]["additional_info"]["tracknumber"],
        3
    );
}
