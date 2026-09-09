//! The libmpv backend against hand-written PCM WAV files through `ao=null`:
//! mirrors `gst_engine.rs` — load, gapless hand-over via `loadfile append`,
//! event ordering and error reporting without a sound device.
#![cfg(feature = "mpv")]

use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use streamboat_core::StreamSource;
use streamboat_core::proto::OutputConfig;
use streamboat_player::{Engine, EngineEvent, LoadItem, MpvEngine};

/// A minimal canonical-form PCM WAV file (44-byte header), written by hand
/// rather than through GStreamer's `audiotestsrc` so this suite builds and
/// runs with the `mpv` feature alone, independent of the `gstreamer` one.
fn write_wav(path: &Path, seconds: f32) {
    let rate: u32 = 44_100;
    let channels: u16 = 2;
    let bits: u16 = 16;
    let n_frames = (rate as f32 * seconds) as u32;
    let block_align = channels * (bits / 8);
    let byte_rate = rate * u32::from(block_align);
    let data_size = n_frames * u32::from(block_align);

    let mut buf = Vec::with_capacity(44 + data_size as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    for i in 0..n_frames {
        let t = i as f32 / rate as f32;
        let sample = (3000.0 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()) as i16;
        buf.extend_from_slice(&sample.to_le_bytes());
        buf.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, buf).unwrap();
}

fn raw_item(id: u64, source: StreamSource) -> LoadItem {
    LoadItem {
        id,
        source,
        replay_gain_db: Some(-6.0),
        peak_amplitude: Some(0.9),
        codec: Some("pcm".into()),
        sample_rate: Some(44_100),
        bit_depth: Some(16),
    }
}

fn item(id: u64, path: &Path) -> LoadItem {
    raw_item(
        id,
        StreamSource::Url(format!("file://{}", path.canonicalize().unwrap().display())),
    )
}

fn new_engine(dir: &Path) -> (MpvEngine, mpsc::Receiver<EngineEvent>) {
    let (tx, rx) = mpsc::channel();
    let engine = MpvEngine::with_audio_override(
        tx,
        OutputConfig::default(),
        dir.to_path_buf(),
        Some("null".into()),
    )
    .unwrap();
    (engine, rx)
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
    write_wav(&a, 0.3);
    let (mut engine, rx) = new_engine(dir.path());

    engine.load(item(1, &a)).unwrap();
    let events = collect_until_eos(&rx, Duration::from_secs(30));
    let seq = ids(&events);
    assert_eq!(
        seq,
        vec!["started:1", "about:1", "finished:1", "eos"],
        "{events:?}"
    );

    let sp = engine.signal_path().unwrap();
    assert!(sp.engine.starts_with("libmpv "), "{sp:?}");
    assert!(!sp.exclusive);
    assert!(
        sp.replaygain_applied,
        "ReplayGain must be applied in shared mode"
    );
    assert_eq!(sp.bit_perfect, Some(false));
}

#[test]
fn gapless_handover_to_the_successor() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.wav");
    let b = dir.path().join("b.wav");
    write_wav(&a, 0.3);
    write_wav(&b, 0.3);
    let (mut engine, rx) = new_engine(dir.path());

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
    let (mut engine, rx) = new_engine(dir.path());

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
fn dash_manifest_is_fed_as_a_temp_file_never_a_data_uri() {
    // A bogus MPD: never valid media, but this proves the engine writes it
    // to a real file under the runtime dir (mpv/ffmpeg cannot re-fetch a
    // `data:` URI mid-playback, tidal-manifest-api.md §3) and reports the
    // resulting decode failure as an Error rather than panicking.
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, rx) = new_engine(dir.path());

    let it = raw_item(3, StreamSource::DashMpd("<MPD><Period/></MPD>".into()));
    let _ = engine.load(it);

    let manifests: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("mpv-manifest-"))
        .collect();
    assert_eq!(
        manifests.len(),
        1,
        "expected exactly one temp manifest file"
    );

    let events = collect_until_eos(&rx, Duration::from_secs(20));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::Error { .. })),
        "{events:?}"
    );
}
