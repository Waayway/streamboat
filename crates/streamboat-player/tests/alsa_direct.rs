//! Exclusive-mode ALSA writer through the full appsink path, against the
//! ALSA `null` device (`ref:` alsa-lib's built-in `pcm.null` type in
//! `/usr/share/alsa/alsa.conf` — openable without a sound card). Skips with
//! a clear message if `null` cannot be opened in this environment (CI has
//! no sound card and no `/dev/snd`, so even `null` may be unavailable on
//! some runners).
#![cfg(all(feature = "gstreamer", feature = "alsa-direct"))]

use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use streamboat_core::StreamSource;
use streamboat_core::proto::OutputConfig;
use streamboat_player::{Engine, EngineEvent, GstEngine, LoadItem};

fn null_device_available() -> bool {
    alsa::pcm::PCM::new("null", alsa::Direction::Playback, false).is_ok()
}

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

#[test]
fn plays_generated_wav_through_the_alsa_null_device() {
    if !null_device_available() {
        eprintln!(
            "skipping plays_generated_wav_through_the_alsa_null_device: \
             ALSA 'null' device unavailable in this environment"
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.wav");
    make_wav(&path, 60);

    let (tx, rx) = mpsc::channel();
    let mut engine = GstEngine::with_sink_override(
        tx,
        OutputConfig::Exclusive {
            device: "null".into(),
        },
        dir.path().to_path_buf(),
        None, // no test-sink override: exercise the real alsa-direct path
    )
    .expect("engine construction with the alsa-direct exclusive writer");

    engine
        .load(LoadItem {
            id: 1,
            source: StreamSource::Url(format!("file://{}", path.canonicalize().unwrap().display())),
            replay_gain_db: None,
            peak_amplitude: None,
            codec: Some("pcm".into()),
            sample_rate: Some(44_100),
            bit_depth: Some(16),
        })
        .expect("load through the appsink/alsa-direct pipeline");

    let mut saw_format = false;
    let mut saw_started = false;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(EngineEvent::Format { .. }) => saw_format = true,
            Ok(EngineEvent::Started { .. }) => saw_started = true,
            Ok(EngineEvent::EndOfStream) => break,
            Ok(EngineEvent::Error { message, .. }) => {
                panic!("engine reported an error: {message}")
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for EndOfStream through the alsa-direct writer"
        );
    }

    assert!(saw_started, "expected a Started event");
    assert!(
        saw_format,
        "expected a Format event reporting the negotiated ALSA device format"
    );

    let sp = engine.signal_path().expect("signal path");
    assert!(sp.exclusive, "signal path should report exclusive=true");
    assert_eq!(
        sp.bit_perfect,
        Some(true),
        "the alsa-direct path never falls back to a lossy conversion"
    );
    assert!(
        sp.device_format.is_some(),
        "device_format should be the actually negotiated ALSA hw_params, not a guess"
    );
}

#[test]
fn gapless_successor_reuses_the_open_pcm_when_the_format_matches() {
    if !null_device_available() {
        eprintln!(
            "skipping gapless_successor_reuses_the_open_pcm_when_the_format_matches: \
             ALSA 'null' device unavailable in this environment"
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.wav");
    let b = dir.path().join("b.wav");
    make_wav(&a, 60);
    make_wav(&b, 60);

    let (tx, rx) = mpsc::channel();
    let mut engine = GstEngine::with_sink_override(
        tx,
        OutputConfig::Exclusive {
            device: "null".into(),
        },
        dir.path().to_path_buf(),
        None,
    )
    .expect("engine construction with the alsa-direct exclusive writer");

    let item = |id: u64, path: &Path| LoadItem {
        id,
        source: StreamSource::Url(format!("file://{}", path.canonicalize().unwrap().display())),
        replay_gain_db: None,
        peak_amplitude: None,
        codec: Some("pcm".into()),
        sample_rate: Some(44_100),
        bit_depth: Some(16),
    };

    engine.load(item(1, &a)).expect("load first track");
    engine.set_next(Some(item(2, &b)));

    let mut format_events = 0u32;
    let mut finished_ids = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(EngineEvent::Format { .. }) => format_events += 1,
            Ok(EngineEvent::Finished { id }) => finished_ids.push(id),
            Ok(EngineEvent::EndOfStream) => break,
            Ok(EngineEvent::Error { message, .. }) => {
                panic!("engine reported an error: {message}")
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for EndOfStream"
        );
    }

    assert_eq!(
        finished_ids,
        vec![1, 2],
        "both tracks should play to completion in order"
    );
    // Same-format gapless hand-over: the PCM opens once and stays open, so
    // exactly one Format event is expected (D-018), not one per track.
    assert_eq!(
        format_events, 1,
        "same-format tracks should keep the ALSA device open, not reopen"
    );
}
