//! Test-only doubles shared by the control-API integration tests: the same
//! `FakeEngine` pattern `streamboat-player`'s own tests use
//! (`crates/streamboat-player/tests/player.rs`), plus a minimal in-process
//! TIDAL mock (wiremock) so a `Play` command has something to resolve.

use std::sync::Arc;
use std::sync::mpsc as std_mpsc;

use base64::Engine as _;
use serde_json::json;
use streamboat_core::proto::{OutputConfig, SignalPath};
use streamboat_core::token_store::{MemoryTokenStore, TokenSet, now_secs};
use streamboat_core::{ApiClient, AuthFlow, ClientCredentials, StreamSource};
use streamboat_player::engine::{EngineError, EngineResult};
use streamboat_player::{Engine, EngineEvent, LoadItem, Player, PlayerConfig, PlayerHandle};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub struct FakeEngine {
    events: std_mpsc::Sender<EngineEvent>,
    output: OutputConfig,
}

impl FakeEngine {
    pub fn new() -> (Self, std_mpsc::Receiver<EngineEvent>) {
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
            std::thread::sleep(std::time::Duration::from_millis(20));
            let _ = events.send(EngineEvent::Started { id: item.id });
        });
        Ok(())
    }
    // Gapless hand-over is not exercised by the control-API tests; nothing
    // here reads the successor back.
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
    let manifest =
        json!({"mimeType":"audio/flac","codecs":"flac","encryptionType":"NONE","urls":[url]});
    json!({
        "trackId": 1, "assetPresentation": "FULL", "audioMode": "STEREO", "audioQuality": "LOSSLESS",
        "manifestMimeType": "application/vnd.tidal.bts",
        "manifest": base64::engine::general_purpose::STANDARD.encode(manifest.to_string()),
        "bitDepth": 16, "sampleRate": 44100
    })
}

/// A single-track TIDAL mock: track id `1`, "One", resolving to a playable
/// (unencrypted) BTS manifest at `LOSSLESS`.
pub async fn tidal() -> MockServer {
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

pub fn client(server: &MockServer) -> ApiClient {
    ApiClient::builder(
        ClientCredentials::new("cid", None),
        Arc::new(MemoryTokenStore::with(tokens())),
    )
    .api_base(&format!("{}/", server.uri()))
    .auth_base(&format!("{}/", server.uri()))
    .build()
    .unwrap()
}

/// A running [`Player`] over the fake engine and the single-track mock
/// above, ready for a test to drive through a [`PlayerHandle`]. The
/// `MockServer` is returned too and must be kept alive for as long as the
/// handle is used — dropping it tears down the mock HTTP listener while the
/// player is still resolving streams against it.
pub async fn spawn_player() -> (PlayerHandle, MockServer) {
    let server = tidal().await;
    let (engine, rx) = FakeEngine::new();
    let handle = Player::spawn(
        client(&server),
        Box::new(engine),
        rx,
        PlayerConfig::default(),
    );
    (handle, server)
}
