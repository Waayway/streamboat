//! The Player against a fake engine and an in-process TIDAL: queue,
//! prefetch/gapless hand-over, skip-with-event, volume policy in exclusive
//! mode, and the Command/Event contract.

use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::json;
use streamboat_core::config::ReplayGainMode;
use streamboat_core::proto::{Command, Event, OutputConfig, PlayItem, PlaybackStatus, SignalPath};
use streamboat_core::token_store::{MemoryTokenStore, TokenSet, now_secs};
use streamboat_core::{ApiClient, AudioQuality, AuthFlow, ClientCredentials, StreamSource};
use streamboat_player::engine::{EngineError, EngineResult};
use streamboat_player::{Engine, EngineEvent, LoadItem, Player, PlayerConfig, PlayerDeps};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// `(replay_gain_db, peak_amplitude)`, in call order — what the
/// ReplayGain-mode tests below assert on.
type GainLog = Arc<Mutex<Vec<(Option<f64>, Option<f64>)>>>;

struct FakeEngine {
    events: std_mpsc::Sender<EngineEvent>,
    next: Arc<Mutex<Option<LoadItem>>>,
    output: OutputConfig,
    loaded: Arc<Mutex<Vec<String>>>,
    /// `(replay_gain_db, peak_amplitude)` as handed to every `load()` call,
    /// in order — what the ReplayGain-mode tests below assert on.
    gains: GainLog,
    /// The same pair, but from `set_next` (the prefetched successor) —
    /// separate so `Command::SetReplayGainMode`'s "recompute the prefetched
    /// successor" behaviour can be asserted on its own.
    next_gains: GainLog,
}

impl FakeEngine {
    fn new() -> (
        Self,
        std_mpsc::Receiver<EngineEvent>,
        Arc<Mutex<Vec<String>>>,
    ) {
        let (engine, rx, loaded, _gains, _next_gains) = Self::new_full();
        (engine, rx, loaded)
    }

    #[allow(clippy::type_complexity)]
    fn new_full() -> (
        Self,
        std_mpsc::Receiver<EngineEvent>,
        Arc<Mutex<Vec<String>>>,
        GainLog,
        GainLog,
    ) {
        let (tx, rx) = std_mpsc::channel();
        let loaded = Arc::new(Mutex::new(Vec::new()));
        let gains = Arc::new(Mutex::new(Vec::new()));
        let next_gains = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                events: tx,
                next: Arc::new(Mutex::new(None)),
                output: OutputConfig::default(),
                loaded: loaded.clone(),
                gains: gains.clone(),
                next_gains: next_gains.clone(),
            },
            rx,
            loaded,
            gains,
            next_gains,
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
        self.loaded.lock().unwrap().push(url);
        self.gains
            .lock()
            .unwrap()
            .push((item.replay_gain_db, item.peak_amplitude));
        let events = self.events.clone();
        let next = self.next.clone();
        std::thread::spawn(move || {
            let mut id = item.id;
            std::thread::sleep(Duration::from_millis(30));
            let _ = events.send(EngineEvent::Started { id });
            loop {
                std::thread::sleep(Duration::from_millis(60));
                let _ = events.send(EngineEvent::AboutToFinish { id });
                std::thread::sleep(Duration::from_millis(10));
                let successor = next.lock().unwrap().take();
                let _ = events.send(EngineEvent::Finished { id });
                match successor {
                    Some(n) => {
                        id = n.id;
                        let _ = events.send(EngineEvent::Started { id });
                    }
                    None => {
                        let _ = events.send(EngineEvent::EndOfStream);
                        break;
                    }
                }
            }
        });
        Ok(())
    }
    fn set_next(&mut self, item: Option<LoadItem>) {
        if let Some(i) = &item {
            self.next_gains
                .lock()
                .unwrap()
                .push((i.replay_gain_db, i.peak_amplitude));
        }
        *self.next.lock().unwrap() = item;
    }
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
        "bitDepth": 16, "sampleRate": 44100, "trackReplayGain": -7.0, "trackPeakAmplitude": 0.95
    })
}

/// Like [`bts`], but with explicit track/album ReplayGain and peak values
/// (`album_rg: None` omits `albumReplayGain`/`albumPeakAmplitude` from the
/// response entirely, exercising the "TIDAL did not report an album value"
/// fallback path) — for the ReplayGain-mode selection tests.
fn bts_gains(
    url: &str,
    track_rg: f64,
    track_peak: f64,
    album: Option<(f64, f64)>,
) -> serde_json::Value {
    let mut v = bts(url);
    let obj = v.as_object_mut().unwrap();
    obj.insert("trackReplayGain".into(), json!(track_rg));
    obj.insert("trackPeakAmplitude".into(), json!(track_peak));
    if let Some((rg, peak)) = album {
        obj.insert("albumReplayGain".into(), json!(rg));
        obj.insert("albumPeakAmplitude".into(), json!(peak));
    }
    v
}

async fn tidal() -> MockServer {
    let server = MockServer::start().await;
    for (id, title) in [
        (1, "One"),
        (2, "Two"),
        (3, "Three"),
        (4, "Four"),
        (5, "Five"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/v1/tracks/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": id, "title": title, "duration": 180, "artists": [{"id": 9, "name": "Artist"}],
                "album": {"id": 5, "title": "Album", "cover": "c"}
            })))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/v1/tracks/1/playbackinfopostpaywall"))
        .and(query_param("audioquality", "LOSSLESS"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bts("https://cdn/one.flac")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/2/playbackinfopostpaywall"))
        .and(query_param("audioquality", "LOSSLESS"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bts("https://cdn/two.flac")))
        .mount(&server)
        .await;
    // Track 3 is region-gated: terminal, so the player must skip it.
    Mock::given(method("GET"))
        .and(path("/v1/tracks/3/playbackinfopostpaywall"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            json!({"status":401,"subStatus":4032,"userMessage":"not in your region"}),
        ))
        .mount(&server)
        .await;
    // Track 4: both album and track ReplayGain reported (album mode must
    // prefer the album pair; track mode must ignore it).
    Mock::given(method("GET"))
        .and(path("/v1/tracks/4/playbackinfopostpaywall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bts_gains(
            "https://cdn/four.flac",
            -7.0,
            0.95,
            Some((-3.0, 0.8)),
        )))
        .mount(&server)
        .await;
    // Track 5: no album value at all (TIDAL omits the field) — album mode
    // must fall back to the track pair.
    Mock::given(method("GET"))
        .and(path("/v1/tracks/5/playbackinfopostpaywall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bts_gains(
            "https://cdn/five.flac",
            -5.0,
            0.9,
            None,
        )))
        .mount(&server)
        .await;
    server
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

/// Polls `gains` (a `FakeEngine` gain-recording vec) until it has at least
/// `len` entries, or panics after `timeout` — `prefetch`'s successor
/// resolution genuinely awaits the wiremock HTTP round trip on its first,
/// uncached call, so `engine.set_next()` can land a moment after the
/// `TrackStarted` event that triggered it, not necessarily before it.
async fn wait_for_gains(
    gains: &GainLog,
    len: usize,
    timeout: Duration,
) -> Vec<(Option<f64>, Option<f64>)> {
    let start = Instant::now();
    loop {
        {
            let g = gains.lock().unwrap();
            if g.len() >= len {
                return g.clone();
            }
        }
        if start.elapsed() > timeout {
            panic!(
                "timed out waiting for {len} gain entries; have {:?}",
                gains.lock().unwrap()
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn next_matching(
    rx: &mut tokio::sync::broadcast::Receiver<Event>,
    mut pred: impl FnMut(&Event) -> bool,
) -> Event {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let ev = rx.recv().await.expect("event stream open");
            if pred(&ev) {
                return ev;
            }
        }
    })
    .await
    .expect("timed out waiting for event")
}

#[tokio::test]
async fn plays_a_queue_gaplessly_and_ends() {
    let server = tidal().await;
    let (engine, rx, loaded) = FakeEngine::new();
    let handle = Player::spawn(
        client(&server),
        Box::new(engine),
        rx,
        PlayerConfig::default(),
        PlayerDeps::default(),
    );
    let mut events = handle.subscribe();
    assert!(handle.send(Command::Play {
        items: vec![PlayItem { track_id: 1 }, PlayItem { track_id: 2 }]
    }));

    let first = next_matching(&mut events, |e| matches!(e, Event::TrackStarted { .. })).await;
    match first {
        Event::TrackStarted {
            track,
            stream,
            index,
        } => {
            assert_eq!(track.id, 1);
            assert_eq!(index, 0);
            assert_eq!(stream.quality, Some(AudioQuality::Lossless));
            assert_eq!(stream.replay_gain_db, Some(-7.0));
        }
        other => panic!("{other:?}"),
    }
    let second = next_matching(&mut events, |e| matches!(e, Event::TrackStarted { .. })).await;
    assert!(matches!(second, Event::TrackStarted { track, index: 1, .. } if track.id == 2));
    next_matching(&mut events, |e| matches!(e, Event::EndOfQueue)).await;
    // Only the first track went through `load`; the second was handed over as the successor.
    assert_eq!(loaded.lock().unwrap().as_slice(), ["https://cdn/one.flac"]);
    handle.send(Command::GetState);
    let st = next_matching(&mut events, |e| matches!(e, Event::State { .. })).await;
    assert!(
        matches!(st, Event::State { state } if state.status == PlaybackStatus::Stopped && state.queue_len == 2)
    );
    handle.send(Command::Shutdown);
}

#[tokio::test]
async fn unplayable_track_is_skipped_with_an_error_event() {
    let server = tidal().await;
    let (engine, rx, _) = FakeEngine::new();
    let handle = Player::spawn(
        client(&server),
        Box::new(engine),
        rx,
        PlayerConfig {
            quality_ceiling: AudioQuality::Lossless,
            ..Default::default()
        },
        PlayerDeps::default(),
    );
    let mut events = handle.subscribe();
    handle.send(Command::Play {
        items: vec![PlayItem { track_id: 3 }, PlayItem { track_id: 1 }],
    });
    let err = next_matching(&mut events, |e| matches!(e, Event::Error { .. })).await;
    assert!(
        matches!(err, Event::Error { track_id: Some(3), ref message } if message.contains("4032")),
        "{err:?}"
    );
    let started = next_matching(&mut events, |e| matches!(e, Event::TrackStarted { .. })).await;
    assert!(matches!(started, Event::TrackStarted { track, index: 1, .. } if track.id == 1));
    handle.send(Command::Shutdown);
}

#[tokio::test]
async fn volume_is_refused_in_exclusive_mode() {
    let server = tidal().await;
    let (engine, rx, _) = FakeEngine::new();
    let cfg = PlayerConfig {
        output: OutputConfig::Exclusive {
            device: "hw:0,0".into(),
        },
        ..Default::default()
    };
    let handle = Player::spawn(
        client(&server),
        Box::new(engine),
        rx,
        cfg,
        PlayerDeps::default(),
    );
    let mut events = handle.subscribe();
    handle.send(Command::SetVolume { volume: 0.5 });
    let w = next_matching(&mut events, |e| matches!(e, Event::Warning { .. })).await;
    assert!(matches!(w, Event::Warning { message } if message.contains("exclusive")));
    let st = next_matching(&mut events, |e| matches!(e, Event::State { .. })).await;
    assert!(
        matches!(st, Event::State { state } if state.volume == 1.0 && state.output.is_exclusive())
    );
    handle.send(Command::Shutdown);
}

#[tokio::test]
async fn commands_and_events_round_trip_through_json() {
    let cmd = Command::Play {
        items: vec![PlayItem { track_id: 42 }],
    };
    let s = serde_json::to_string(&cmd).unwrap();
    assert_eq!(s, r#"{"type":"play","items":[{"track_id":42}]}"#);
    let back: Command = serde_json::from_str(&s).unwrap();
    assert_eq!(back, cmd);
    let ev = Event::AuthRequired {
        verification_url: "https://link.tidal.com/X".into(),
        user_code: "X".into(),
        expires_in_secs: 300,
    };
    let s = serde_json::to_string(&ev).unwrap();
    assert!(s.starts_with(r#"{"type":"auth_required""#));
}

#[tokio::test]
async fn replay_gain_set_replay_gain_mode_round_trips_through_json() {
    let cmd = Command::SetReplayGainMode {
        mode: ReplayGainMode::Track,
    };
    let s = serde_json::to_string(&cmd).unwrap();
    assert_eq!(s, r#"{"type":"set_replay_gain_mode","mode":"track"}"#);
    let back: Command = serde_json::from_str(&s).unwrap();
    assert_eq!(back, cmd);
}

/// D-019: album mode prefers TIDAL's album-context ReplayGain/peak (track 4
/// reports both) over the track-context pair.
#[tokio::test]
async fn replay_gain_album_mode_prefers_album_values() {
    let server = tidal().await;
    let (engine, rx, _loaded, gains, _next_gains) = FakeEngine::new_full();
    let handle = Player::spawn(
        client(&server),
        Box::new(engine),
        rx,
        PlayerConfig {
            replay_gain_mode: ReplayGainMode::Album,
            ..Default::default()
        },
        PlayerDeps::default(),
    );
    let mut events = handle.subscribe();
    handle.send(Command::Play {
        items: vec![PlayItem { track_id: 4 }],
    });
    next_matching(&mut events, |e| matches!(e, Event::TrackStarted { .. })).await;
    assert_eq!(gains.lock().unwrap().as_slice(), [(Some(-3.0), Some(0.8))]);
    handle.send(Command::Shutdown);
}

/// D-019: album mode falls back to the track pair when TIDAL reported no
/// album value at all (track 5 has no `albumReplayGain`).
#[tokio::test]
async fn replay_gain_album_mode_falls_back_to_track_values_when_album_is_absent() {
    let server = tidal().await;
    let (engine, rx, _loaded, gains, _next_gains) = FakeEngine::new_full();
    let handle = Player::spawn(
        client(&server),
        Box::new(engine),
        rx,
        PlayerConfig {
            replay_gain_mode: ReplayGainMode::Album,
            ..Default::default()
        },
        PlayerDeps::default(),
    );
    let mut events = handle.subscribe();
    handle.send(Command::Play {
        items: vec![PlayItem { track_id: 5 }],
    });
    next_matching(&mut events, |e| matches!(e, Event::TrackStarted { .. })).await;
    assert_eq!(gains.lock().unwrap().as_slice(), [(Some(-5.0), Some(0.9))]);
    handle.send(Command::Shutdown);
}

/// D-019: track mode always uses the track pair, ignoring the album pair
/// even when TIDAL reported one (track 4 has both).
#[tokio::test]
async fn replay_gain_track_mode_uses_track_values_even_when_album_is_present() {
    let server = tidal().await;
    let (engine, rx, _loaded, gains, _next_gains) = FakeEngine::new_full();
    let handle = Player::spawn(
        client(&server),
        Box::new(engine),
        rx,
        PlayerConfig {
            replay_gain_mode: ReplayGainMode::Track,
            ..Default::default()
        },
        PlayerDeps::default(),
    );
    let mut events = handle.subscribe();
    handle.send(Command::Play {
        items: vec![PlayItem { track_id: 4 }],
    });
    next_matching(&mut events, |e| matches!(e, Event::TrackStarted { .. })).await;
    assert_eq!(gains.lock().unwrap().as_slice(), [(Some(-7.0), Some(0.95))]);
    handle.send(Command::Shutdown);
}

/// D-019: off mode applies neither gain nor peak, regardless of what TIDAL
/// reported.
#[tokio::test]
async fn replay_gain_off_mode_applies_no_gain() {
    let server = tidal().await;
    let (engine, rx, _loaded, gains, _next_gains) = FakeEngine::new_full();
    let handle = Player::spawn(
        client(&server),
        Box::new(engine),
        rx,
        PlayerConfig {
            replay_gain_mode: ReplayGainMode::Off,
            ..Default::default()
        },
        PlayerDeps::default(),
    );
    let mut events = handle.subscribe();
    handle.send(Command::Play {
        items: vec![PlayItem { track_id: 4 }],
    });
    next_matching(&mut events, |e| matches!(e, Event::TrackStarted { .. })).await;
    assert_eq!(gains.lock().unwrap().as_slice(), [(None, None)]);
    handle.send(Command::Shutdown);
}

/// `Command::SetReplayGainMode` switches live: the currently-playing track
/// keeps whatever gain it already loaded with (never re-applied mid-track,
/// matching the engines' documented once-per-load boundary), but the
/// already-prefetched successor is recomputed and re-handed to
/// `set_next` under the new mode without a fresh network resolve.
#[tokio::test]
async fn set_replay_gain_mode_recomputes_the_prefetched_successor() {
    let server = tidal().await;
    let (engine, rx, _loaded, _gains, next_gains) = FakeEngine::new_full();
    let handle = Player::spawn(
        client(&server),
        Box::new(engine),
        rx,
        PlayerConfig {
            replay_gain_mode: ReplayGainMode::Track,
            ..Default::default()
        },
        PlayerDeps::default(),
    );
    let mut events = handle.subscribe();
    handle.send(Command::Play {
        items: vec![PlayItem { track_id: 1 }, PlayItem { track_id: 4 }],
    });
    next_matching(&mut events, |e| matches!(e, Event::TrackStarted { .. })).await;
    // The successor (track 4) was prefetched in track mode: (-7.0, 0.95).
    let seen = wait_for_gains(&next_gains, 1, Duration::from_secs(5)).await;
    assert_eq!(seen.last(), Some(&(Some(-7.0), Some(0.95))));
    handle.send(Command::SetReplayGainMode {
        mode: ReplayGainMode::Album,
    });
    handle.send(Command::GetState);
    next_matching(&mut events, |e| {
        matches!(e, Event::State { state } if state.signal_path.as_ref().map(|sp| sp.replaygain_mode.as_str()) == Some("album"))
    })
    .await;
    // Re-prefetched under album mode: track 4's album pair, (-3.0, 0.8).
    let seen = wait_for_gains(&next_gains, 2, Duration::from_secs(5)).await;
    assert_eq!(seen.last(), Some(&(Some(-3.0), Some(0.8))));
    handle.send(Command::Shutdown);
}
