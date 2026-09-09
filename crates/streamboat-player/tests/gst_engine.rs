//! The GStreamer backend against generated WAV files through `fakesink`:
//! proves load, gapless hand-over via `about-to-finish`, event ordering and
//! error reporting without a DAC.
#![cfg(feature = "gstreamer")]

use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use streamboat_core::StreamSource;
use streamboat_core::proto::OutputConfig;
use streamboat_player::{Engine, EngineEvent, GstEngine, LoadItem};

fn make_wav(path: &Path, buffers: u32) {
    gst::init().unwrap();
    let desc = format!(
        "audiotestsrc num-buffers={buffers} ! audio/x-raw,rate=44100,channels=2,format=S16LE ! wavenc ! filesink location={}",
        path.display()
    );
    let pipeline = gst::parse::launch(&desc).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let bus = pipeline.bus().unwrap();
    let msg = bus.timed_pop_filtered(
        gst::ClockTime::from_seconds(20),
        &[gst::MessageType::Eos, gst::MessageType::Error],
    );
    assert!(
        matches!(msg.map(|m| m.type_()), Some(gst::MessageType::Eos)),
        "wav generation failed"
    );
    pipeline.set_state(gst::State::Null).unwrap();
}

fn raw_item(id: u64, source: StreamSource) -> LoadItem {
    LoadItem {
        id,
        source,
        replay_gain_db: Some(-6.0),
        peak_amplitude: Some(0.9),
        codec: Some("pcm".into()),
        sample_rate: Some(44100),
        bit_depth: Some(16),
    }
}

fn item(id: u64, path: &Path) -> LoadItem {
    LoadItem {
        id,
        source: StreamSource::Url(format!("file://{}", path.canonicalize().unwrap().display())),
        replay_gain_db: Some(-6.0),
        peak_amplitude: Some(0.9),
        codec: Some("pcm".into()),
        sample_rate: Some(44100),
        bit_depth: Some(16),
    }
}

fn collect_until_eos(rx: &mpsc::Receiver<EngineEvent>, timeout: Duration) -> Vec<EngineEvent> {
    let start = Instant::now();
    let mut out = Vec::new();
    while start.elapsed() < timeout {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(ev) => {
                let done = matches!(ev, EngineEvent::EndOfStream | EngineEvent::Error { .. });
                out.push(ev);
                if done {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        }
    }
    out
}

fn ids(events: &[EngineEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::Started { id } => Some(format!("started:{id}")),
            EngineEvent::Finished { id } => Some(format!("finished:{id}")),
            EngineEvent::AboutToFinish { id } => Some(format!("about:{id}")),
            EngineEvent::EndOfStream => Some("eos".into()),
            EngineEvent::Error { .. } => Some("error".into()),
            _ => None,
        })
        .collect()
}

#[test]
fn plays_one_file_to_the_end() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.wav");
    make_wav(&a, 40);
    let (tx, rx) = mpsc::channel();
    let mut engine = GstEngine::with_sink_override(
        tx,
        OutputConfig::default(),
        dir.path().to_path_buf(),
        Some("fakesink".into()),
    )
    .unwrap();
    engine.load(item(1, &a)).unwrap();
    let events = collect_until_eos(&rx, Duration::from_secs(30));
    let seq = ids(&events);
    assert_eq!(
        seq,
        vec!["started:1", "about:1", "finished:1", "eos"],
        "{events:?}"
    );
    let sp = engine.signal_path().unwrap();
    assert!(sp.engine.starts_with("gstreamer 1."));
    assert!(
        sp.replaygain_applied,
        "ReplayGain must be applied in shared mode"
    );
    assert!(!sp.exclusive);
    assert_eq!(sp.bit_perfect, Some(false));
}

#[test]
fn gapless_handover_to_the_successor() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.wav");
    let b = dir.path().join("b.wav");
    make_wav(&a, 40);
    make_wav(&b, 40);
    let (tx, rx) = mpsc::channel();
    let mut engine = GstEngine::with_sink_override(
        tx,
        OutputConfig::default(),
        dir.path().to_path_buf(),
        Some("fakesink".into()),
    )
    .unwrap();
    engine.load(item(1, &a)).unwrap();
    engine.set_next(Some(item(2, &b)));
    let events = collect_until_eos(&rx, Duration::from_secs(30));
    let seq = ids(&events);
    assert_eq!(
        seq,
        vec![
            "started:1",
            "about:1",
            "finished:1",
            "started:2",
            "about:2",
            "finished:2",
            "eos"
        ],
        "{events:?}"
    );
}

#[test]
fn unreadable_source_reports_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, rx) = mpsc::channel();
    let mut engine = GstEngine::with_sink_override(
        tx,
        OutputConfig::default(),
        dir.path().to_path_buf(),
        Some("fakesink".into()),
    )
    .unwrap();
    let it = raw_item(
        9,
        StreamSource::Url(format!(
            "file://{}/does-not-exist.flac",
            dir.path().display()
        )),
    );
    let _ = engine.load(it);
    let events = collect_until_eos(&rx, Duration::from_secs(20));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::Error { id: Some(9), .. })),
        "{events:?}"
    );
}

#[test]
fn dash_manifest_is_fed_as_data_uri_or_temp_file() {
    // A minimal MPD pointing at a local WAV via BaseURL is "direct" per the
    // core parser, so here we only assert the engine accepts a DashMpd
    // source without panicking and reports an error for a bogus one.
    let dir = tempfile::tempdir().unwrap();
    let (tx, rx) = mpsc::channel();
    let mut engine = GstEngine::with_sink_override(
        tx,
        OutputConfig::default(),
        dir.path().to_path_buf(),
        Some("fakesink".into()),
    )
    .unwrap();
    let it = raw_item(3, StreamSource::DashMpd("<MPD><Period/></MPD>".into()));
    let _ = engine.load(it);
    let events = collect_until_eos(&rx, Duration::from_secs(20));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::Error { .. })),
        "{events:?}"
    );
}
