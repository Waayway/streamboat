//! The GStreamer backend (Linux, D-016).
//!
//! Design, from `audio-pipeline` playback-behavior §1 and output-backends
//! §1/§12: `playbin3` with `about-to-finish` gapless; the audio sink is
//! `autoaudiosink` in shared mode or `alsasink device=hw:X,Y` in exclusive
//! mode with playbin's `native-audio` flag so no conversion or resampling
//! stage is inserted (a format the DAC refuses fails loudly instead of being
//! resampled behind the user's back); `dashdemux2` is demoted below the
//! legacy `dashdemux` on GStreamer < 1.26.10 because only the legacy element
//! demuxes TIDAL's FLAC-in-DASH there; DASH manifests are fed as `data:`
//! URIs, falling back to a per-session temp file that is deleted on drop.
//!
//! What this spike does not yet do (D-017 next steps): the hand-written ALSA
//! writer with `hw_params` read-back, dither policy and the per-branch
//! ReplayGain element. ReplayGain here is a single `volume` audio-filter,
//! which has the known boundary glitch when consecutive tracks differ.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use base64::Engine as _;
use gstreamer as gst;
use gstreamer::prelude::*;
use streamboat_core::StreamSource;
use streamboat_core::proto::{OutputConfig, SignalPath};

use crate::engine::{Engine, EngineError, EngineEvent, EngineResult, LoadItem};

/// FLAC-in-DASH via `dashdemux2` needs 1.26.10 (`decoding-and-codecs.md` §1).
const DASHDEMUX2_FLAC_FLOOR: (u32, u32, u32) = (1, 26, 10);

static INIT: OnceLock<Result<(), String>> = OnceLock::new();

/// `pub(crate)` so [`crate::platform::enumerate_output_devices`] can reuse
/// the same GStreamer init (and `dashdemux2` demotion) `GstEngine::new`
/// itself relies on, rather than calling `gst::init()` a second, redundant
/// way.
pub(crate) fn ensure_init() -> EngineResult<()> {
    INIT.get_or_init(|| {
        gst::init().map_err(|e| e.to_string())?;
        let (maj, min, mic, _) = gst::version();
        if (maj, min, mic) < DASHDEMUX2_FLAC_FLOOR {
            if let Some(f) = gst::ElementFactory::find("dashdemux2") {
                f.set_rank(gst::Rank::NONE);
                tracing::info!(
                    "GStreamer {maj}.{min}.{mic} < 1.26.10: demoted dashdemux2 so the legacy dashdemux \
                     handles FLAC-in-DASH"
                );
            }
        }
        Ok(())
    })
    .clone()
    .map_err(EngineError::Unavailable)
}

pub(crate) struct Shared {
    /// The successor handed to `set_next`, consumed by `about-to-finish`.
    pub(crate) next: Mutex<Option<LoadItem>>,
    /// The item whose URI is currently set on playbin (may be the successor
    /// after `about-to-finish` fired but before its stream started).
    pub(crate) current: Mutex<Option<LoadItem>>,
    /// The item audio is actually flowing for.
    pub(crate) playing: Mutex<Option<LoadItem>>,
    /// Items whose URI has been set on playbin, in order, whose
    /// `stream-start` the bus thread has not consumed yet. playbin posts one
    /// `stream-start` per URI in the order the URIs were set, so attributing
    /// each to the front of this queue is exact even when `about-to-finish`
    /// (GStreamer's streaming thread) has already moved `current` on to the
    /// successor before the bus thread got to the predecessor's start — which
    /// happens routinely with `fakesink` and short clips, and can happen
    /// under load with real sinks.
    pub(crate) pending_starts: Mutex<VecDeque<LoadItem>>,
    /// An `about-to-finish` that fired before the bus thread had emitted
    /// `Started` for that item: replayed right after the `Started`, so the
    /// Player always sees Started before AboutToFinish for the same id.
    pub(crate) deferred_about: Mutex<Option<u64>>,
    pub(crate) events: Sender<EngineEvent>,
    pub(crate) stop_bus: AtomicBool,
    /// The exclusive-mode ALSA sink, when one is active (D-017), so the
    /// `about-to-finish` handler can decide the gapless successor's
    /// bit-perfect format before its first buffer reaches the appsink.
    #[cfg(feature = "alsa-direct")]
    pub(crate) alsa_exclusive: Mutex<Option<Arc<crate::alsa_writer::ExclusiveSink>>>,
}

impl Shared {
    pub(crate) fn emit(&self, e: EngineEvent) {
        let _ = self.events.send(e);
    }
}

pub struct GstEngine {
    playbin: gst::Element,
    shared: Arc<Shared>,
    bus_thread: Option<JoinHandle<()>>,
    output: OutputConfig,
    volume: f64,
    sink_name: String,
    temp_files: Vec<PathBuf>,
    runtime_dir: PathBuf,
    rg_filter: Option<gst::Element>,
    replaygain_applied: bool,
    sink_override: Option<String>,
}

impl GstEngine {
    /// `runtime_dir` is where per-session DASH manifests go when `data:`
    /// URIs are unavailable; they are deleted on drop. The environment
    /// variable `STREAMBOAT_GST_SINK` replaces the audio sink with any
    /// GStreamer element description (for CI: `fakesink`).
    pub fn new(
        events: Sender<EngineEvent>,
        output: OutputConfig,
        runtime_dir: PathBuf,
    ) -> EngineResult<Self> {
        let sink_override = std::env::var("STREAMBOAT_GST_SINK")
            .ok()
            .filter(|s| !s.is_empty());
        Self::with_sink_override(events, output, runtime_dir, sink_override)
    }

    /// Like [`Self::new`] with an explicit sink override (tests).
    pub fn with_sink_override(
        events: Sender<EngineEvent>,
        output: OutputConfig,
        runtime_dir: PathBuf,
        sink_override: Option<String>,
    ) -> EngineResult<Self> {
        ensure_init()?;
        let playbin_name =
            std::env::var("STREAMBOAT_GST_PLAYBIN").unwrap_or_else(|_| "playbin3".into());
        let playbin = gst::ElementFactory::make(&playbin_name)
            .build()
            .or_else(|_| gst::ElementFactory::make("playbin").build())
            .map_err(|e| EngineError::Unavailable(format!("cannot create playbin: {e}")))?;

        let shared = Arc::new(Shared {
            next: Mutex::new(None),
            current: Mutex::new(None),
            playing: Mutex::new(None),
            pending_starts: Mutex::new(VecDeque::new()),
            deferred_about: Mutex::new(None),
            events,
            stop_bus: AtomicBool::new(false),
            #[cfg(feature = "alsa-direct")]
            alsa_exclusive: Mutex::new(None),
        });

        // Gapless: when playbin is about to run dry, hand it the successor.
        let s = shared.clone();
        playbin.connect("about-to-finish", false, move |args| {
            let pb = args[0].get::<gst::Element>().ok()?;
            let cur_id = s.current.lock().unwrap().as_ref().map(|i| i.id);
            if let Some(id) = cur_id {
                let started = s.playing.lock().unwrap().as_ref().map(|i| i.id) == Some(id);
                if started {
                    s.emit(EngineEvent::AboutToFinish { id });
                } else {
                    // The bus thread has not seen this item's stream-start
                    // yet; it replays the event once it has (see `Shared`).
                    *s.deferred_about.lock().unwrap() = Some(id);
                }
            }
            let next = s.next.lock().unwrap().take();
            if let Some(item) = next {
                match uri_for(&item.source, None) {
                    Ok((uri, _)) => {
                        // Decide the successor's bit-perfect format before its
                        // first buffer can reach the appsink (D-018): same
                        // format as the current track keeps the PCM open,
                        // gapless; a different one reopens with a silence
                        // pre-roll once the writer observes it.
                        #[cfg(feature = "alsa-direct")]
                        if let Some(sink) = s.alsa_exclusive.lock().unwrap().clone() {
                            if let Err(e) = sink.prepare_item(item.bit_depth) {
                                s.emit(EngineEvent::Error {
                                    id: Some(item.id),
                                    message: e.to_string(),
                                });
                            }
                        }
                        pb.set_property("uri", &uri);
                        s.pending_starts.lock().unwrap().push_back(item.clone());
                        *s.current.lock().unwrap() = Some(item);
                    }
                    Err(e) => s.emit(EngineEvent::Warning {
                        message: format!("gapless successor skipped: {e}"),
                    }),
                }
            }
            None
        });

        let mut engine = Self {
            playbin,
            shared,
            bus_thread: None,
            output: output.clone(),
            volume: 1.0,
            sink_name: String::new(),
            temp_files: Vec::new(),
            runtime_dir,
            rg_filter: None,
            replaygain_applied: false,
            sink_override,
        };
        engine.apply_output(&output)?;
        engine.start_bus_thread();
        Ok(engine)
    }

    fn start_bus_thread(&mut self) {
        let bus = self.playbin.bus().expect("playbin has a bus");
        let shared = self.shared.clone();
        let playbin_name = self.playbin.name().to_string();
        let sink_probe = self.playbin.clone();
        self.bus_thread = Some(
            std::thread::Builder::new()
                .name("streamboat-gst-bus".into())
                .spawn(move || {
                    use gst::MessageView;
                    while !shared.stop_bus.load(Ordering::Relaxed) {
                        let Some(msg) = bus.timed_pop(gst::ClockTime::from_mseconds(200)) else {
                            continue;
                        };
                        let from_playbin = msg
                            .src()
                            .map(|s| s.name() == playbin_name.as_str())
                            .unwrap_or(false);
                        match msg.view() {
                            MessageView::Eos(_) => {
                                let done = shared.playing.lock().unwrap().take();
                                if let Some(item) = done {
                                    shared.emit(EngineEvent::Finished { id: item.id });
                                }
                                shared.emit(EngineEvent::EndOfStream);
                            }
                            MessageView::Error(err) => {
                                let id = shared
                                    .playing
                                    .lock()
                                    .unwrap()
                                    .as_ref()
                                    .map(|i| i.id)
                                    .or_else(|| {
                                        shared.current.lock().unwrap().as_ref().map(|i| i.id)
                                    });
                                let message = format!(
                                    "{} ({})",
                                    err.error(),
                                    err.debug()
                                        .map(|d| d.to_string())
                                        .unwrap_or_default()
                                        .lines()
                                        .next()
                                        .unwrap_or("")
                                        .trim()
                                );
                                shared.emit(EngineEvent::Error { id, message });
                            }
                            MessageView::Warning(w) => {
                                shared.emit(EngineEvent::Warning {
                                    message: w.error().to_string(),
                                });
                            }
                            MessageView::Buffering(b) => {
                                let pct = b.percent().clamp(0, 100) as u8;
                                shared.emit(EngineEvent::Buffering { percent: pct });
                            }
                            MessageView::StreamStart(_) if from_playbin => {
                                // A new stream is flowing: the oldest item whose
                                // URI was set and whose start we have not seen.
                                let queued = shared.pending_starts.lock().unwrap().pop_front();
                                let cur = queued.or_else(|| shared.current.lock().unwrap().clone());
                                let prev = shared.playing.lock().unwrap().replace(
                                    cur.clone().unwrap_or_else(|| LoadItem {
                                        id: 0,
                                        source: StreamSource::Url(String::new()),
                                        replay_gain_db: None,
                                        peak_amplitude: None,
                                        codec: None,
                                        sample_rate: None,
                                        bit_depth: None,
                                    }),
                                );
                                if let Some(p) = prev {
                                    if cur.as_ref().map(|c| c.id) != Some(p.id) {
                                        shared.emit(EngineEvent::Finished { id: p.id });
                                    }
                                }
                                if let Some(c) = cur {
                                    shared.emit(EngineEvent::Started { id: c.id });
                                    let deferred = shared.deferred_about.lock().unwrap().take();
                                    match deferred {
                                        Some(id) if id == c.id => {
                                            shared.emit(EngineEvent::AboutToFinish { id });
                                        }
                                        other => *shared.deferred_about.lock().unwrap() = other,
                                    }
                                    // The alsa-direct writer emits its own
                                    // Format event from the real hw_params
                                    // read-back once it opens/confirms the
                                    // PCM; this caps-guess would otherwise
                                    // report the *source* format flowing
                                    // into the writer's bin, not the device
                                    // format (D-036: report only what is
                                    // observed).
                                    let alsa_direct_active = {
                                        #[cfg(feature = "alsa-direct")]
                                        {
                                            shared.alsa_exclusive.lock().unwrap().is_some()
                                        }
                                        #[cfg(not(feature = "alsa-direct"))]
                                        {
                                            false
                                        }
                                    };
                                    if !alsa_direct_active {
                                        if let Some(desc) = describe_sink_caps(&sink_probe) {
                                            shared.emit(EngineEvent::Format {
                                                id: c.id,
                                                description: desc,
                                            });
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                })
                .expect("spawn bus thread"),
        );
    }

    fn apply_output(&mut self, output: &OutputConfig) -> EngineResult<()> {
        // Any previous exclusive-mode ALSA writer belongs to the output
        // being replaced; stop it (and its thread) before building whatever
        // comes next, mirroring the D-Bus device-reservation release-order
        // rule (release the old resource before acquiring the new one).
        #[cfg(feature = "alsa-direct")]
        if let Some(old) = self.shared.alsa_exclusive.lock().unwrap().take() {
            old.stop();
        }

        let test_sink = self.sink_override.clone();
        let (sink, flags, name): (gst::Element, &str, String) = match (&test_sink, output) {
            (Some(desc), _) => {
                let sink = gst::parse::bin_from_description(desc, true)
                    .map(|b| b.upcast::<gst::Element>())
                    .or_else(|_| gst::ElementFactory::make(desc).build())
                    .map_err(|e| EngineError::Output(format!("test sink {desc:?}: {e}")))?;
                (sink, "audio+soft-volume", format!("test:{desc}"))
            }
            #[cfg(feature = "alsa-direct")]
            (None, OutputConfig::Exclusive { device }) => {
                let exclusive =
                    crate::alsa_writer::ExclusiveSink::new(device, self.shared.clone())?;
                let sink = exclusive.bin.clone();
                *self.shared.alsa_exclusive.lock().unwrap() = Some(Arc::new(exclusive));
                // native-audio: no audioconvert/audioresample/volume from
                // playbin's own side; our bin's own audioconvert only ever
                // does a lossless container widening or a channel mix.
                (
                    sink,
                    "audio+native-audio",
                    format!("alsa-direct device={device}"),
                )
            }
            #[cfg(not(feature = "alsa-direct"))]
            (None, OutputConfig::Exclusive { device }) => {
                let sink = gst::ElementFactory::make("alsasink")
                    .property("device", device)
                    .build()
                    .map_err(|e| EngineError::Output(format!("alsasink: {e}")))?;
                // native-audio: no audioconvert/audioresample/volume in the
                // path; the DAC gets the decoded samples or negotiation fails.
                (
                    sink,
                    "audio+native-audio",
                    format!("alsasink device={device}"),
                )
            }
            (
                None,
                OutputConfig::Shared {
                    device: Some(device),
                },
            ) => {
                let sink = gst::ElementFactory::make("alsasink")
                    .property("device", device)
                    .build()
                    .map_err(|e| EngineError::Output(format!("alsasink: {e}")))?;
                (
                    sink,
                    "audio+soft-volume",
                    format!("alsasink device={device}"),
                )
            }
            (None, OutputConfig::Shared { device: None }) => {
                let sink = gst::ElementFactory::make("autoaudiosink")
                    .build()
                    .map_err(|e| EngineError::Output(format!("autoaudiosink: {e}")))?;
                (sink, "audio+soft-volume", "autoaudiosink".into())
            }
            // D-034: an explicit, fixed-format output feeding a running
            // snapserver's `tcp://...&mode=server` stream source —
            // streamboat connects out as the TCP *client* (`crate::snapcast`'s
            // module doc has the matching `snapserver.conf` line). Resamples
            // in-bin rather than relying on playbin's own audio-sink
            // negotiation, so the caps downstream of `tcpclientsink` are
            // always exactly `crate::snapcast`'s fixed format regardless of
            // the source's own rate/depth — mutually exclusive with
            // bit-perfect by construction (a different enum variant, not a
            // flag combinable with `Exclusive`).
            (None, OutputConfig::Snapcast { host, port }) => {
                use crate::snapcast::{CHANNELS, GST_FORMAT, SAMPLE_RATE};
                let desc = format!(
                    "audioconvert ! audioresample ! audio/x-raw,rate={SAMPLE_RATE},format={GST_FORMAT},channels={CHANNELS} ! tcpclientsink host={host} port={port}"
                );
                let sink = gst::parse::bin_from_description(&desc, true)
                    .map(|b| b.upcast::<gst::Element>())
                    .map_err(|e| EngineError::Output(format!("snapcast sink: {e}")))?;
                (
                    sink,
                    "audio+soft-volume",
                    format!("snapcast tcp://{host}:{port}"),
                )
            }
        };
        self.playbin.set_property("audio-sink", &sink);
        self.playbin.set_property_from_str("flags", flags);
        // ReplayGain gain stage only outside exclusive mode (D-019).
        if output.is_exclusive() {
            self.rg_filter = None;
            self.playbin
                .set_property("audio-filter", None::<gst::Element>);
        } else {
            let vol = gst::ElementFactory::make("volume")
                .build()
                .map_err(|e| EngineError::Output(format!("volume: {e}")))?;
            self.playbin.set_property("audio-filter", &vol);
            self.rg_filter = Some(vol);
        }
        self.output = output.clone();
        self.sink_name = name;
        Ok(())
    }

    fn set_replaygain(&mut self, item: &LoadItem) {
        self.replaygain_applied = false;
        let Some(filter) = &self.rg_filter else {
            return;
        };
        // TIDAL's own formula: min(10^((rg + preamp)/20), 1/peak), preamp 4 dB.
        let gain = match (item.replay_gain_db, item.peak_amplitude) {
            (Some(rg), peak) => {
                let g = 10f64.powf((rg + 4.0) / 20.0);
                let cap = peak
                    .filter(|p| *p > 0.0)
                    .map(|p| 1.0 / p)
                    .unwrap_or(f64::INFINITY);
                g.min(cap)
            }
            _ => 1.0,
        };
        filter.set_property("volume", gain.clamp(0.0, 10.0));
        self.replaygain_applied = (gain - 1.0).abs() > 1e-6;
    }
}

/// Turn a source into a playbin URI. DASH goes as a `data:` URI when the
/// `dataurisrc` element exists, else into `runtime_dir` (caller tracks the
/// file for deletion). Returns the URI and the temp path if one was written.
fn uri_for(
    source: &StreamSource,
    runtime_dir: Option<&PathBuf>,
) -> EngineResult<(String, Option<PathBuf>)> {
    match source {
        StreamSource::Url(u) => Ok((u.clone(), None)),
        StreamSource::DashMpd(xml) => match runtime_dir {
            Some(dir) if gst::ElementFactory::find("dataurisrc").is_none() => {
                std::fs::create_dir_all(dir).map_err(|e| EngineError::Source(e.to_string()))?;
                let path = dir.join(format!("manifest-{}.mpd", uuid::Uuid::new_v4().simple()));
                std::fs::write(&path, xml).map_err(|e| EngineError::Source(e.to_string()))?;
                let uri = url_from_path(&path)?;
                Ok((uri, Some(path)))
            }
            _ => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(xml.as_bytes());
                Ok((format!("data:application/dash+xml;base64,{b64}"), None))
            }
        },
    }
}

fn url_from_path(path: &std::path::Path) -> EngineResult<String> {
    let abs = std::fs::canonicalize(path).map_err(|e| EngineError::Source(e.to_string()))?;
    Ok(format!("file://{}", abs.display()))
}

fn describe_sink_caps(playbin: &gst::Element) -> Option<String> {
    let sink: gst::Element = playbin.property("audio-sink");
    let caps = find_sink_caps(&sink)?;
    let s = caps.structure(0)?;
    let format = s.get::<&str>("format").ok().unwrap_or("?");
    let rate = s.get::<i32>("rate").ok().unwrap_or(0);
    let channels = s.get::<i32>("channels").ok().unwrap_or(0);
    Some(format!("{format} {rate} Hz {channels}ch"))
}

fn find_sink_caps(sink: &gst::Element) -> Option<gst::Caps> {
    if let Some(pad) = sink.static_pad("sink") {
        if let Some(c) = pad.current_caps() {
            return Some(c);
        }
    }
    // A bin (autoaudiosink or a test bin): look at its ghost pad's target.
    let pad = sink.static_pad("sink")?;
    pad.peer().and_then(|p| p.current_caps())
}

fn decoder_name(playbin: &gst::Element) -> Option<String> {
    let bin = playbin.downcast_ref::<gst::Bin>()?;
    let mut it = bin.iterate_recurse();
    while let Ok(Some(el)) = it.next() {
        if let Some(f) = el.factory() {
            let klass = f.klass();
            if klass.contains("Decoder") && klass.contains("Audio") {
                return Some(f.name().to_string());
            }
        }
    }
    None
}

impl Engine for GstEngine {
    fn name(&self) -> &'static str {
        "gstreamer"
    }

    fn load(&mut self, item: LoadItem) -> EngineResult<()> {
        self.playbin
            .set_state(gst::State::Null)
            .map_err(|e| EngineError::Other(format!("stop before load: {e}")))?;
        *self.shared.playing.lock().unwrap() = None;
        *self.shared.next.lock().unwrap() = None;
        self.shared.pending_starts.lock().unwrap().clear();
        *self.shared.deferred_about.lock().unwrap() = None;
        let (uri, tmp) = uri_for(&item.source, Some(&self.runtime_dir))?;
        if let Some(p) = tmp {
            self.temp_files.push(p);
        }
        self.set_replaygain(&item);
        #[cfg(feature = "alsa-direct")]
        if let Some(sink) = self.shared.alsa_exclusive.lock().unwrap().clone() {
            sink.prepare_item(item.bit_depth)?;
        }
        self.playbin.set_property("uri", &uri);
        if !self.output.is_exclusive() {
            self.playbin.set_property("volume", self.volume);
        }
        self.shared
            .pending_starts
            .lock()
            .unwrap()
            .push_back(item.clone());
        *self.shared.current.lock().unwrap() = Some(item);
        self.playbin.set_state(gst::State::Playing).map_err(|_| {
            EngineError::Output(format!("cannot start playback on {}", self.sink_name))
        })?;
        Ok(())
    }

    fn set_next(&mut self, item: Option<LoadItem>) {
        *self.shared.next.lock().unwrap() = item;
    }

    fn play(&mut self) -> EngineResult<()> {
        self.playbin
            .set_state(gst::State::Playing)
            .map(|_| ())
            .map_err(|e| EngineError::Other(format!("play: {e}")))?;
        #[cfg(feature = "alsa-direct")]
        if let Some(sink) = self.shared.alsa_exclusive.lock().unwrap().clone() {
            sink.set_pause(false);
        }
        Ok(())
    }

    fn pause(&mut self) -> EngineResult<()> {
        self.playbin
            .set_state(gst::State::Paused)
            .map(|_| ())
            .map_err(|e| EngineError::Other(format!("pause: {e}")))?;
        // Hardware pause when the device supports it, else the writer
        // falls back to a software pause (silence-paced), per D-018/§12:
        // hold the device across a pause rather than releasing it.
        #[cfg(feature = "alsa-direct")]
        if let Some(sink) = self.shared.alsa_exclusive.lock().unwrap().clone() {
            sink.set_pause(true);
        }
        Ok(())
    }

    fn stop(&mut self) -> EngineResult<()> {
        *self.shared.next.lock().unwrap() = None;
        self.playbin
            .set_state(gst::State::Null)
            .map(|_| ())
            .map_err(|e| EngineError::Other(format!("stop: {e}")))?;
        let done = self.shared.playing.lock().unwrap().take();
        if let Some(item) = done {
            self.shared.emit(EngineEvent::Finished { id: item.id });
        }
        self.shared.pending_starts.lock().unwrap().clear();
        *self.shared.deferred_about.lock().unwrap() = None;
        *self.shared.current.lock().unwrap() = None;
        Ok(())
    }

    fn seek(&mut self, position_ms: u64) -> EngineResult<()> {
        // FLUSH | ACCURATE: sample-accurate, at the cost of decoding from the
        // preceding keyframe/segment (Strawberry's choice; Sone/High Tide use
        // KEY_UNIT and snap to a segment boundary on DASH).
        self.playbin
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::ClockTime::from_mseconds(position_ms),
            )
            .map_err(|e| EngineError::Other(format!("seek: {e}")))
    }

    fn set_volume(&mut self, volume: f32) -> EngineResult<()> {
        let v = f64::from(volume.clamp(0.0, 1.0));
        self.volume = v;
        if self.output.is_exclusive() {
            return Err(EngineError::Output(
                "volume is fixed at 100% while an exclusive output is active".into(),
            ));
        }
        self.playbin.set_property("volume", v);
        Ok(())
    }

    fn set_output(&mut self, output: &OutputConfig) -> EngineResult<()> {
        if *output == self.output {
            return Ok(());
        }
        // Changing the sink on a live pipeline is not supported by playbin;
        // it applies at the next load, which tears the pipeline down anyway.
        self.playbin
            .set_state(gst::State::Null)
            .map_err(|e| EngineError::Other(format!("stop for output change: {e}")))?;
        self.apply_output(output)
    }

    fn position(&self) -> Option<(u64, Option<u64>)> {
        let dur = self
            .playbin
            .query_duration::<gst::ClockTime>()
            .map(|d| d.mseconds());
        // The write-vs-audible correction (`snd_pcm_delay`, §8) is only
        // meaningful for the exclusive-mode writer; a normal sink answers
        // playbin's own position query correctly already.
        #[cfg(feature = "alsa-direct")]
        if let Some(sink) = self.shared.alsa_exclusive.lock().unwrap().clone() {
            if let Some(pos) = sink.position_ms() {
                return Some((pos, dur));
            }
        }
        let pos = self.playbin.query_position::<gst::ClockTime>()?;
        Some((pos.mseconds(), dur))
    }

    fn signal_path(&self) -> Option<SignalPath> {
        let (maj, min, mic, _) = gst::version();

        // The alsa-direct writer reports the *actually negotiated* device
        // format from its own hw_params read-back, rather than the
        // best-effort caps-guess `find_sink_caps` below has to make for an
        // opaque `alsasink`/`autoaudiosink` (D-036: report only what is
        // observed).
        #[cfg(feature = "alsa-direct")]
        if let Some(sink) = self.shared.alsa_exclusive.lock().unwrap().clone() {
            let playing = self.shared.playing.lock().unwrap().clone();
            let source_format = playing.as_ref().map(|i| {
                format!(
                    "{} {} Hz {}-bit",
                    i.codec.clone().unwrap_or_else(|| "?".into()),
                    i.sample_rate
                        .map(|r| r.to_string())
                        .unwrap_or_else(|| "?".into()),
                    i.bit_depth
                        .map(|b| b.to_string())
                        .unwrap_or_else(|| "?".into())
                )
            });
            return Some(SignalPath {
                engine: format!("gstreamer {maj}.{min}.{mic} + alsa-direct writer"),
                source_format,
                decoder: decoder_name(&self.playbin),
                sink: Some(self.sink_name.clone()),
                device: Some(sink.device().to_string()),
                device_format: sink.device_format_description(),
                exclusive: true,
                converted: sink.converted_description(),
                volume_applied: false,
                replaygain_applied: false,
                replaygain_mode: String::new(),
                bit_perfect: sink.bit_perfect(),
            });
        }

        let sink: gst::Element = self.playbin.property("audio-sink");
        let device_format = find_sink_caps(&sink).and_then(|c| {
            let s = c.structure(0)?;
            Some(format!(
                "{} {} Hz {}ch",
                s.get::<&str>("format").ok().unwrap_or("?"),
                s.get::<i32>("rate").ok().unwrap_or(0),
                s.get::<i32>("channels").ok().unwrap_or(0)
            ))
        });
        let playing = self.shared.playing.lock().unwrap().clone();
        let source_format = playing.as_ref().map(|i| {
            format!(
                "{} {} Hz {}-bit",
                i.codec.clone().unwrap_or_else(|| "?".into()),
                i.sample_rate
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "?".into()),
                i.bit_depth
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| "?".into())
            )
        });
        let exclusive = self.output.is_exclusive();
        let device = match &self.output {
            OutputConfig::Exclusive { device }
            | OutputConfig::Shared {
                device: Some(device),
            } => Some(device.clone()),
            OutputConfig::Snapcast { host, port } => Some(format!("snapcast tcp://{host}:{port}")),
            _ => None,
        };
        let converted = if exclusive {
            None
        } else if self.output.is_snapcast() {
            use crate::snapcast::{CHANNELS, GST_FORMAT, SAMPLE_RATE};
            Some(format!(
                "resampled to the fixed Snapcast format {GST_FORMAT} {SAMPLE_RATE} Hz {CHANNELS}ch (D-034)"
            ))
        } else {
            Some("shared mode: the system mixer may resample and mix".to_string())
        };
        let bit_perfect = if exclusive {
            match (
                playing.as_ref().and_then(|i| i.sample_rate),
                device_format.as_deref(),
            ) {
                (Some(src_rate), Some(df)) => Some(df.contains(&format!(" {src_rate} Hz"))),
                _ => None,
            }
        } else {
            Some(false)
        };
        Some(SignalPath {
            engine: format!("gstreamer {maj}.{min}.{mic}"),
            source_format,
            decoder: decoder_name(&self.playbin),
            sink: Some(self.sink_name.clone()),
            device,
            device_format,
            exclusive,
            converted,
            volume_applied: !exclusive && (self.volume - 1.0).abs() > 1e-6,
            replaygain_applied: !exclusive && self.replaygain_applied,
            replaygain_mode: String::new(),
            bit_perfect,
        })
    }
}

impl Drop for GstEngine {
    fn drop(&mut self) {
        // Stop the writer thread deterministically rather than relying on
        // the playbin's own signal-handler closure to drop its `Shared`
        // clone (and this `Arc<ExclusiveSink>` with it) at some later,
        // GObject-refcount-determined point.
        #[cfg(feature = "alsa-direct")]
        if let Some(sink) = self.shared.alsa_exclusive.lock().unwrap().take() {
            sink.stop();
        }
        self.shared.stop_bus.store(true, Ordering::Relaxed);
        let _ = self.playbin.set_state(gst::State::Null);
        if let Some(t) = self.bus_thread.take() {
            let _ = t.join();
        }
        for p in self.temp_files.drain(..) {
            let _ = std::fs::remove_file(p);
        }
    }
}
