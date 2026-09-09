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

/// D-034: `OutputConfig::Snapcast` resamples to the fixed
/// `SAMPLE_RATE`/`BIT_DEPTH`/`CHANNELS` format and connects out, as a TCP
/// client, to the address a snapserver `tcp://...&mode=server` stream
/// source would be listening on. Stands in for snapserver with a plain
/// `TcpListener` and checks the byte count against the WAV's own known
/// duration, at the fixed format, rather than trying to decode the stream.
#[test]
fn snapcast_output_streams_the_fixed_format_pcm_over_tcp() {
    use std::io::Read;
    use std::net::TcpListener;
    use streamboat_player::snapcast::{BIT_DEPTH, CHANNELS, SAMPLE_RATE};

    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.wav");
    // 44100 Hz source, 30 buffers of `audiotestsrc`'s default 1024 frames
    // each: audiotestsrc's own duration, not the output format under test.
    let buffers = 30u32;
    make_wav(&a, buffers);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let (tx, rx) = mpsc::channel();
    // `GstEngine::new` reads `STREAMBOAT_GST_SINK` itself and, when set
    // (CI, and this whole suite, set it to `fakesink` so audio tests never
    // touch real ALSA), it silently *replaces* the real `Snapcast` sink
    // branch with that override — exactly the branch this test exists to
    // exercise. `with_sink_override(.., None)` bypasses that inherited
    // environment override explicitly, regardless of how this test happens
    // to be invoked.
    let mut engine = GstEngine::with_sink_override(
        tx,
        OutputConfig::Snapcast {
            host: "127.0.0.1".into(),
            port,
        },
        dir.path().to_path_buf(),
        None,
    )
    .unwrap();
    engine.load(item(1, &a)).unwrap();

    // A plain blocking `accept()` has no timeout of its own: if the sink
    // ever fails to connect (a bug in this branch, or GStreamer element
    // contention under a parallel test run), this must fail loudly rather
    // than hang the whole test binary — `cargo test`'s default runner would
    // otherwise wait on it forever.
    listener.set_nonblocking(false).unwrap();
    let (accept_tx, accept_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = accept_tx.send(listener.accept());
    });
    let mut stream = accept_rx
        .recv_timeout(Duration::from_secs(15))
        .expect("timed out waiting for the snapcast tcpclientsink to connect")
        .expect("accept failed")
        .0;
    // The pipeline reaches EOS well within this; `tcpclientsink` does not
    // necessarily close the TCP connection at EOS, so this timeout (not a
    // read returning `Ok(0)`) is the loop's normal exit, not a failure mode.
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut received = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => received.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => panic!("reading the snapcast tcp stream: {e}"),
        }
    }
    let _ = collect_until_eos(&rx, Duration::from_secs(10));

    assert!(!received.is_empty(), "no PCM reached the tcp listener");
    let bytes_per_frame = (BIT_DEPTH / 8) * CHANNELS;
    assert_eq!(
        received.len() % bytes_per_frame as usize,
        0,
        "stream length must be a whole number of {BIT_DEPTH}-bit {CHANNELS}ch frames"
    );
    // The source is 30 * 1024 frames at 44100 Hz; resampled to SAMPLE_RATE
    // the frame count (and so the byte count) scales by that ratio. Allow a
    // generous tolerance for resampler edge effects rather than pin an
    // exact sample count.
    let source_frames = f64::from(buffers) * 1024.0;
    let expected_frames = source_frames * f64::from(SAMPLE_RATE) / 44100.0;
    let got_frames = received.len() as f64 / f64::from(bytes_per_frame);
    let ratio = got_frames / expected_frames;
    assert!(
        (0.5..=1.5).contains(&ratio),
        "expected roughly {expected_frames} frames at {SAMPLE_RATE} Hz, got {got_frames} \
         ({} bytes)",
        received.len()
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
