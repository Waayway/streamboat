//! The libmpv backend (Windows and macOS, D-016), behind the `mpv` cargo
//! feature. Implements the same [`Engine`] contract as [`crate::gst`] so the
//! [`crate::player::Player`] and its tests stay backend-agnostic.
//!
//! Design points, from `audio-pipeline` (`references/stacks-comparison.md`
//! §3 Design B, `references/output-backends.md` §4-§6,
//! `references/playback-behavior.md` §1, `references/tidal-manifest-api.md`
//! §3) and `tech-stack-evaluation/references/audio-engine-comparison.md`:
//!
//! - `loadfile <uri> replace` starts a track; `loadfile <uri> append` queues
//!   the gapless successor, so the hand-over happens inside mpv itself
//!   rather than being driven from an `about-to-finish`-style signal (mpv has
//!   none for a pre-queued playlist entry). `--gapless-audio=weak` (D-018) —
//!   never `yes`, which locks every later track to the first one's format.
//! - Exclusive output (D-017): `audio-exclusive=yes` plus the platform AO —
//!   `wasapi` on Windows, mpv's dedicated `coreaudio_exclusive` AO on macOS
//!   (direct device access/hog mode, distinct from `coreaudio` +
//!   `--audio-exclusive=yes`, which only *redirects* to it for compressed
//!   formats), and `alsa` with a `hw:`/`plughw:` device string on Linux —
//!   where mpv's own docs say `--audio-exclusive` silently no-ops on the
//!   `alsa` AO (`output-backends.md` §6); it is still set there for
//!   consistency across the three platforms, it simply does nothing.
//! - DASH manifests are always written to a per-session temp file under the
//!   runtime dir, never a `data:` URI: unlike GStreamer's `dashdemux`, mpv
//!   (via FFmpeg) cannot re-fetch a `data:` URI mid-playback
//!   (`tidal-manifest-api.md` §3, strategy 2). The manifest's own `https://`
//!   segment URLs additionally need `protocol_whitelist` widened on both the
//!   demuxer and stream layers, since FFmpeg's demuxer otherwise refuses to
//!   follow them out of a `file://`-loaded document.
//! - ReplayGain/volume (D-019): TIDAL's fMP4/BTS streams carry no ReplayGain
//!   tags, so mpv's own `replaygain`/`replaygain-preamp`/`replaygain-clip`
//!   properties — which read tags out of the decoded file — cannot be fed
//!   TIDAL's per-track gain and peak externally. This backend computes
//!   TIDAL's formula itself (`min(10^((gain+4)/20), 1/peak)`, matching
//!   `gst.rs`) and folds it into mpv's own `volume` property together with
//!   the user volume, rather than an `af=volume`/`lavfi` filter chain, to
//!   avoid depending on a filter whose availability in this mpv build is
//!   unverified. Applied once per `load()`, not re-applied at a gapless
//!   hand-over — the same known boundary glitch `gst.rs` documents for its
//!   own single shared `volume` element. In exclusive mode, volume is fixed
//!   at 100 and ReplayGain is bypassed entirely, matching `gst.rs`.
//! - The mpv event loop runs on its own thread against a *second* client
//!   handle (`Mpv::create_client`), exactly mirroring `gst.rs`'s bus thread:
//!   the root handle (owned by [`MpvEngine`]) issues commands/properties,
//!   the client handle only ever calls `wait_event` until a stop flag is set.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use libmpv2::events::{Event as MpvEvent, PropertyData, mpv_event_id};
use libmpv2::{
    EndFileReason, Error as MpvError, Format as MpvFormat, Mpv, mpv_end_file_reason, mpv_error,
};
use streamboat_core::StreamSource;
use streamboat_core::proto::{OutputConfig, SignalPath};

use crate::engine::{Engine, EngineError, EngineEvent, EngineResult, LoadItem};

const OBS_PAUSE: u64 = 1;
const OBS_PAUSED_FOR_CACHE: u64 = 2;
const OBS_CACHE_BUFFERING: u64 = 3;

struct Shared {
    /// The successor handed to `set_next`, consumed by `StartFile` once mpv
    /// actually advances to it (mirrors `gst.rs`'s `next`, consumed by
    /// `about-to-finish` there instead).
    next: Mutex<Option<LoadItem>>,
    /// The item mpv's playlist is currently pointed at — set synchronously
    /// by `load()`, or by the `StartFile` handler when mpv advances to a
    /// queued successor on its own.
    current: Mutex<Option<LoadItem>>,
    /// Whether `current` has already had its own `PlaybackRestart` (i.e. it
    /// is not merely queued but has actually started). `StartFile` fires
    /// once per playlist position, including the very first one for
    /// whatever `load()` just set as `current` — this is what tells that
    /// apart from mpv genuinely advancing past `current` to the queued
    /// successor, which is the only time `next` should be consumed. Stays
    /// `true` across `current`'s own `EndFile` (which only clears
    /// `playing`, not `current`) so the *next* `StartFile` — mpv's real
    /// advance — still reads it correctly; only a fresh `load()` or the
    /// advance itself resets it.
    current_started: Mutex<bool>,
    /// The item audio is actually flowing for (mirrors `gst.rs`'s
    /// `playing`). Only this field drives `Finished`/`EndOfStream`, so an
    /// item interrupted by a fresh `load()` — which clears this immediately,
    /// before mpv's own asynchronous `EndFile(Stop)` for it arrives — never
    /// gets a spurious `Finished`, exactly as `gst.rs` behaves today.
    playing: Mutex<Option<LoadItem>>,
    /// The id `Format` was last emitted for: mpv fires `audio-reconfig`
    /// several times while a track's output settles, but `Format` should
    /// fire once per item, same cardinality as `gst.rs`'s single emission on
    /// `StreamStart`.
    format_emitted_for: Mutex<Option<u64>>,
    events: Sender<EngineEvent>,
    stop: AtomicBool,
}

impl Shared {
    fn emit(&self, e: EngineEvent) {
        let _ = self.events.send(e);
    }
}

pub struct MpvEngine {
    /// Root handle: commands and property access from `Engine` methods.
    mpv: Mpv,
    shared: Arc<Shared>,
    event_thread: Option<JoinHandle<()>>,
    output: OutputConfig,
    /// User volume, 0.0..=1.0.
    volume: f64,
    /// TIDAL's ReplayGain gain as a linear multiplier (1.0 = no change).
    replaygain_linear: f64,
    replaygain_applied: bool,
    /// The `ao` name last requested, for `signal_path()` when mpv hasn't
    /// reported `current-ao` yet (nothing loaded/playing so far).
    ao_name: String,
    temp_files: Vec<PathBuf>,
    runtime_dir: PathBuf,
    /// Test-only `ao` override (`ao=null`); when set, `apply_output` never
    /// touches `ao`/`audio-device` itself, only the volume/ReplayGain policy.
    audio_override: Option<String>,
    /// The FIFO-reading thread feeding a Snapcast TCP connection (D-034),
    /// while `output` is [`OutputConfig::Snapcast`] on a Unix target. `None`
    /// otherwise, including on a non-Unix target — see
    /// [`Self::start_snapcast_pump`]'s doc comment for why that side is not
    /// implemented yet.
    snapcast_pump: Option<SnapcastPump>,
}

/// Owns the pump thread's FIFO file for [`OutputConfig::Snapcast`] on Unix
/// (D-034): mpv's `pcm` AO (`ao=pcm`, `ao-pcm-file=<fifo>`,
/// `ao-pcm-waveheader=no`) writes headerless PCM into the FIFO; a detached
/// thread's only job is copying those bytes on to a TCP connection to
/// snapserver's `tcp://...&mode=server` stream source (the same destination
/// `gst.rs`'s `tcpclientsink` branch connects to). One-shot: it does not
/// reconnect if the TCP connection drops. Deliberately not joined on drop
/// (below) — it may be parked in a blocking read on mpv's own writer end,
/// which this struct does not own, so joining here could hang a
/// `set_output`/shutdown on mpv's timing instead of this struct's own. See
/// the module doc's Snapcast section for what is unverified here (no
/// snapserver in this environment).
struct SnapcastPump {
    fifo_path: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Drop for SnapcastPump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Best-effort unblock: opening the FIFO for write gives a pump
        // thread parked in the initial blocking open a peer to match, and
        // closes it again immediately (this `File` is not bound to a
        // variable) so a thread already blocked in `read` sees EOF too, as
        // long as mpv's own writer end is not still open — not guaranteed,
        // which is exactly why this is best-effort and not joined.
        let _ = std::fs::OpenOptions::new()
            .write(true)
            .open(&self.fifo_path);
        let _ = std::fs::remove_file(&self.fifo_path);
    }
}

impl MpvEngine {
    /// `runtime_dir` is where per-session DASH manifests are written (never
    /// a `data:` URI — see the module doc comment); deleted on drop. The
    /// environment variable `STREAMBOAT_MPV_AO` overrides `ao` (for CI:
    /// `null`).
    pub fn new(
        events: Sender<EngineEvent>,
        output: OutputConfig,
        runtime_dir: PathBuf,
    ) -> EngineResult<Self> {
        let audio_override = std::env::var("STREAMBOAT_MPV_AO")
            .ok()
            .filter(|s| !s.is_empty());
        Self::with_audio_override(events, output, runtime_dir, audio_override)
    }

    /// Like [`Self::new`] with an explicit `ao` override (tests: `ao=null`,
    /// which needs no real sound device and runs headless).
    pub fn with_audio_override(
        events: Sender<EngineEvent>,
        output: OutputConfig,
        runtime_dir: PathBuf,
        audio_override: Option<String>,
    ) -> EngineResult<Self> {
        let msg_level = msg_level_from_rust_log();
        let ao_opt = audio_override.clone();
        let mpv = Mpv::with_initializer(|init| {
            init.set_option("vid", "no")?;
            init.set_option("audio-display", "no")?;
            init.set_option("terminal", "no")?;
            init.set_option("idle", "yes")?;
            init.set_option("keep-open", "no")?;
            // D-018: never "yes" (locks the first file's format for the
            // whole run) — playback-behavior.md §1.
            init.set_option("gapless-audio", "weak")?;
            init.set_option("msg-level", msg_level.as_str())?;
            init.set_option("cache", "yes")?;
            // Sone's DASH buffer-duration hint (playback-behavior.md §1).
            init.set_option("demuxer-readahead-secs", "15")?;
            let whitelist = protocol_whitelist_opt();
            init.set_option("demuxer-lavf-o", whitelist.as_str())?;
            init.set_option("stream-lavf-o", whitelist.as_str())?;
            init.set_option("prefetch-playlist", "yes")?;
            // Headroom for `volume * replaygain_linear * 100`, up to 1000.
            init.set_option("volume-max", "1000")?;
            if let Some(ao) = &ao_opt {
                init.set_option("ao", ao.as_str())?;
            }
            Ok(())
        })
        .map_err(|e| EngineError::Unavailable(format!("cannot create mpv core: {e}")))?;

        let events_client = mpv.create_client(Some("streamboat-events")).map_err(|e| {
            EngineError::Unavailable(format!("cannot create mpv event client: {e}"))
        })?;
        // The root handle never calls `wait_event`; keep its own default
        // event queue empty rather than let it grow unread.
        let _ = mpv.disable_all_events();

        for ev in [
            mpv_event_id::StartFile,
            mpv_event_id::EndFile,
            mpv_event_id::FileLoaded,
            mpv_event_id::AudioReconfig,
            mpv_event_id::PlaybackRestart,
        ] {
            events_client
                .enable_event(ev)
                .map_err(|e| EngineError::Unavailable(format!("enable_event: {e}")))?;
        }
        // `pause` is observed but not translated into an EngineEvent: unlike
        // GStreamer, mpv has no bus-message analog either, and the Player
        // already owns pause state itself. Observed anyway so a future
        // externally-triggered pause is at least visible on the wire.
        let _ = events_client.observe_property("pause", MpvFormat::Flag, OBS_PAUSE);
        let _ = events_client.observe_property(
            "paused-for-cache",
            MpvFormat::Flag,
            OBS_PAUSED_FOR_CACHE,
        );
        let _ = events_client.observe_property(
            "cache-buffering-state",
            MpvFormat::Int64,
            OBS_CACHE_BUFFERING,
        );

        let shared = Arc::new(Shared {
            next: Mutex::new(None),
            current: Mutex::new(None),
            current_started: Mutex::new(false),
            playing: Mutex::new(None),
            format_emitted_for: Mutex::new(None),
            events,
            stop: AtomicBool::new(false),
        });

        let mut engine = Self {
            mpv,
            shared: shared.clone(),
            event_thread: None,
            output: output.clone(),
            volume: 1.0,
            replaygain_linear: 1.0,
            replaygain_applied: false,
            ao_name: audio_override.clone().unwrap_or_default(),
            temp_files: Vec::new(),
            runtime_dir,
            audio_override,
            snapcast_pump: None,
        };
        engine.apply_output(&output)?;
        engine.event_thread = Some(
            std::thread::Builder::new()
                .name("streamboat-mpv-events".into())
                .spawn(move || run_event_loop(events_client, shared))
                .expect("spawn mpv event thread"),
        );
        Ok(engine)
    }

    /// DASH goes to a per-session file under `runtime_dir` (tracked for
    /// deletion on drop); a direct URL passes through unchanged.
    fn uri_for(&mut self, source: &StreamSource) -> EngineResult<String> {
        match source {
            StreamSource::Url(u) => Ok(u.clone()),
            StreamSource::DashMpd(xml) => {
                std::fs::create_dir_all(&self.runtime_dir)
                    .map_err(|e| EngineError::Source(e.to_string()))?;
                let path = self.runtime_dir.join(format!(
                    "mpv-manifest-{}.mpd",
                    uuid::Uuid::new_v4().simple()
                ));
                std::fs::write(&path, xml).map_err(|e| EngineError::Source(e.to_string()))?;
                let abs =
                    std::fs::canonicalize(&path).map_err(|e| EngineError::Source(e.to_string()))?;
                self.temp_files.push(path);
                Ok(format!("file://{}", abs.display()))
            }
        }
    }

    /// TIDAL's own formula (matching `gst.rs`'s `set_replaygain`):
    /// `min(10^((gain + 4)/20), 1/peak)`, folded into mpv's `volume`
    /// property alongside the user volume — see the module doc comment for
    /// why not `af=volume`/`replaygain-preamp`. Bypassed entirely in
    /// exclusive mode (D-019). Applied once at `load()`, not re-applied at a
    /// gapless hand-over: the same known boundary glitch `gst.rs` documents.
    fn apply_replaygain(&mut self, item: &LoadItem) {
        if self.output.is_exclusive() {
            self.replaygain_linear = 1.0;
            self.replaygain_applied = false;
            return;
        }
        let linear = match item.replay_gain_db {
            Some(rg) => {
                let g = 10f64.powf((rg + 4.0) / 20.0);
                let cap = item
                    .peak_amplitude
                    .filter(|p| *p > 0.0)
                    .map(|p| 1.0 / p)
                    .unwrap_or(f64::INFINITY);
                g.min(cap)
            }
            None => 1.0,
        };
        self.replaygain_linear = linear.clamp(0.0, 10.0);
        self.replaygain_applied = (self.replaygain_linear - 1.0).abs() > 1e-6;
        self.push_volume();
    }

    fn push_volume(&mut self) {
        if self.output.is_exclusive() {
            return;
        }
        let percent = (self.volume * self.replaygain_linear * 100.0).clamp(0.0, 1000.0);
        let _ = self.mpv.set_property("volume", percent);
    }

    fn set_ao(&mut self, ao: &str) -> EngineResult<()> {
        self.ao_name = ao.to_string();
        self.mpv
            .set_property("ao", ao)
            .map_err(|e| EngineError::Output(format!("ao={ao}: {e}")))
    }

    fn set_str(&mut self, name: &str, value: &str) -> EngineResult<()> {
        self.mpv
            .set_property(name, value)
            .map_err(|e| EngineError::Output(format!("{name}={value}: {e}")))
    }

    fn set_flag(&mut self, name: &str, value: bool) -> EngineResult<()> {
        self.mpv
            .set_property(name, value)
            .map_err(|e| EngineError::Output(format!("{name}={value}: {e}")))
    }

    /// Sets `ao`/`audio-device`/`audio-exclusive` (and, on macOS,
    /// `coreaudio-change-physical-format`) for `output` (D-017). Skipped
    /// entirely when a test `audio_override` is active.
    fn apply_ao_for_output(&mut self, output: &OutputConfig) -> EngineResult<()> {
        #[cfg(target_os = "windows")]
        const EXCLUSIVE_AO: &str = "wasapi";
        #[cfg(target_os = "windows")]
        const SHARED_AO: &str = "wasapi";
        // D-017: hog mode via mpv's dedicated `coreaudio_exclusive` AO
        // (direct device access), not `coreaudio` + `--audio-exclusive=yes`
        // (which only redirects to it for compressed formats) — see the
        // module doc comment and `output-backends.md` §5-§6/§8. Untested on
        // real macOS hardware — no CI runner for it here.
        #[cfg(target_os = "macos")]
        const EXCLUSIVE_AO: &str = "coreaudio_exclusive";
        #[cfg(target_os = "macos")]
        const SHARED_AO: &str = "coreaudio";
        #[cfg(target_os = "linux")]
        const EXCLUSIVE_AO: &str = "alsa";
        #[cfg(target_os = "linux")]
        const SHARED_AO: &str = "alsa";
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        const EXCLUSIVE_AO: &str = "auto";
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        const SHARED_AO: &str = "auto";

        match output {
            OutputConfig::Exclusive { device } => {
                self.set_ao(EXCLUSIVE_AO)?;
                self.set_str("audio-device", &format!("{EXCLUSIVE_AO}/{device}"))?;
                // mpv's own docs: silently ignored on the `alsa` AO
                // (output-backends.md §6, pitfall #2) — set uniformly
                // anyway so the trait's three platforms read the same way;
                // exclusivity on Linux comes only from the `hw:`/`plughw:`
                // device string above.
                self.set_flag("audio-exclusive", true)?;
                #[cfg(target_os = "macos")]
                self.set_flag("coreaudio-change-physical-format", true)?;
            }
            OutputConfig::Shared { device } => {
                self.set_ao(SHARED_AO)?;
                if let Some(d) = device {
                    self.set_str("audio-device", &format!("{SHARED_AO}/{d}"))?;
                }
                self.set_flag("audio-exclusive", false)?;
            }
            OutputConfig::Snapcast { host, port } => {
                self.set_flag("audio-exclusive", false)?;
                self.snapcast_pump = Some(self.start_snapcast_pump(host, *port)?);
            }
        }
        Ok(())
    }

    /// D-034, Unix only (see the module doc comment's Snapcast section):
    /// `--ao=pcm` writes headerless, fixed-format PCM into a fresh FIFO
    /// under `runtime_dir`; a detached thread copies that FIFO's bytes on to
    /// a TCP connection to snapserver's `tcp://...&mode=server` stream
    /// source at `host:port` — the libmpv-backend equivalent of `gst.rs`'s
    /// `tcpclientsink` branch, since mpv (unlike GStreamer) has no
    /// TCP-client sink element to hand the resampled audio to directly.
    /// `audio-samplerate`/`audio-channels`/`audio-format` force mpv's own
    /// output to `crate::snapcast`'s fixed format ahead of the `pcm` AO, the
    /// same role `gst.rs`'s explicit `audioconvert ! audioresample !
    /// audio/x-raw,...` chain plays for GStreamer.
    #[cfg(unix)]
    fn start_snapcast_pump(&mut self, host: &str, port: u16) -> EngineResult<SnapcastPump> {
        use crate::snapcast::{BIT_DEPTH, CHANNELS, SAMPLE_RATE};
        std::fs::create_dir_all(&self.runtime_dir)
            .map_err(|e| EngineError::Output(format!("runtime dir: {e}")))?;
        let fifo_path = self
            .runtime_dir
            .join(format!("snapcast-{}.pcm", uuid::Uuid::new_v4().simple()));
        let _ = std::fs::remove_file(&fifo_path);
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo_path)
            .status()
            .map_err(|e| EngineError::Output(format!("mkfifo: {e}")))?;
        if !status.success() {
            return Err(EngineError::Output(format!(
                "mkfifo {} exited with {status}",
                fifo_path.display()
            )));
        }
        self.set_ao("pcm")?;
        self.set_str("ao-pcm-file", fifo_path.to_string_lossy().as_ref())?;
        self.set_flag("ao-pcm-waveheader", false)?;
        self.set_str("audio-samplerate", &SAMPLE_RATE.to_string())?;
        self.set_str("audio-channels", "stereo")?;
        self.set_str("audio-format", &format!("s{BIT_DEPTH}"))?;
        let _ = CHANNELS; // documents the `stereo` literal above matches the constant
        let stop = Arc::new(AtomicBool::new(false));
        let pump = SnapcastPump {
            fifo_path: fifo_path.clone(),
            stop: stop.clone(),
        };
        let host = host.to_string();
        std::thread::Builder::new()
            .name("streamboat-snapcast-pump".into())
            .spawn(move || run_snapcast_pump(fifo_path, host, port, stop))
            .map_err(|e| EngineError::Output(format!("spawn snapcast pump thread: {e}")))?;
        Ok(pump)
    }

    #[cfg(not(unix))]
    fn start_snapcast_pump(&mut self, _host: &str, _port: u16) -> EngineResult<SnapcastPump> {
        Err(EngineError::Output(
            "Snapcast output via the libmpv backend needs a FIFO, which is only implemented on \
             Unix targets so far (D-034); use OutputConfig::Shared/Exclusive on this OS, or the \
             GStreamer backend on Linux"
                .into(),
        ))
    }

    fn apply_output(&mut self, output: &OutputConfig) -> EngineResult<()> {
        // Any previous Snapcast pump belongs to the output being replaced —
        // drop it (stop + best-effort unblock, `SnapcastPump::drop`) before
        // whatever comes next might start a new one.
        self.snapcast_pump = None;
        if self.audio_override.is_none() {
            self.apply_ao_for_output(output)?;
        }
        self.output = output.clone();
        if output.is_exclusive() {
            self.replaygain_linear = 1.0;
            self.replaygain_applied = false;
            let _ = self.mpv.set_property("volume", 100.0_f64);
        } else {
            self.push_volume();
        }
        Ok(())
    }
}

/// The detached pump thread body (D-034): blocks opening `fifo_path` for
/// read (mpv's `pcm` AO is the writer once it starts outputting audio),
/// connects to `host:port` once, then copies bytes across in a small loop
/// so `stop` is checked between reads instead of blocking forever in one
/// `io::copy` call. Exits silently on any I/O error or `stop` — there is no
/// `EngineEvent` for this path to report through (it is not one of the
/// per-track events `Shared` carries), so a connection failure here is a
/// silent output failure until D-034's Snapcast support grows its own
/// error-reporting story; logged via `tracing` in the meantime.
#[cfg(unix)]
fn run_snapcast_pump(fifo_path: PathBuf, host: String, port: u16, stop: Arc<AtomicBool>) {
    use std::io::Read;
    let mut fifo = match std::fs::File::open(&fifo_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(%e, path = %fifo_path.display(), "snapcast: could not open the FIFO");
            return;
        }
    };
    let mut sock = match std::net::TcpStream::connect((host.as_str(), port)) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(%e, %host, port, "snapcast: could not connect to snapserver");
            return;
        }
    };
    let mut buf = [0u8; 8192];
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match fifo.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => {
                use std::io::Write;
                if sock.write_all(&buf[..n]).is_err() {
                    return;
                }
            }
            Err(e) => {
                tracing::warn!(%e, "snapcast: FIFO read failed");
                return;
            }
        }
    }
}

impl Engine for MpvEngine {
    fn name(&self) -> &'static str {
        "libmpv"
    }

    fn load(&mut self, item: LoadItem) -> EngineResult<()> {
        *self.shared.next.lock().unwrap() = None;
        *self.shared.playing.lock().unwrap() = None;
        *self.shared.format_emitted_for.lock().unwrap() = None;
        *self.shared.current_started.lock().unwrap() = false;
        let uri = self.uri_for(&item.source)?;
        self.apply_replaygain(&item);
        *self.shared.current.lock().unwrap() = Some(item);
        self.mpv
            .command("loadfile", &[uri.as_str(), "replace"])
            .map_err(|e| EngineError::Output(format!("loadfile: {e}")))?;
        Ok(())
    }

    fn set_next(&mut self, item: Option<LoadItem>) {
        // Drop whatever mpv has queued beyond the currently playing entry —
        // mpv, not us, owns the appended playlist entry, so invalidating a
        // stale successor from an earlier prefetch (Enqueue,
        // SetQualityCeiling) needs `playlist-clear`, not just clearing our
        // own bookkeeping the way `gst.rs`'s `next = None` can. Only do this
        // when there is actually something stale to drop: mpv's own notion
        // of "currently played file" (what `playlist-clear` keeps) is not
        // well-defined until that file has genuinely started, so calling it
        // immediately after `load()` — before mpv has caught up — can race
        // mpv's own bookkeeping and clear the just-loaded first item too.
        let had_stale_successor = self.shared.next.lock().unwrap().take().is_some();
        if had_stale_successor {
            let _ = self.mpv.command("playlist-clear", &[]);
        }
        let Some(item) = item else { return };
        let uri = match self.uri_for(&item.source) {
            Ok(uri) => uri,
            Err(e) => {
                self.shared.emit(EngineEvent::Warning {
                    message: format!("gapless successor skipped: {e}"),
                });
                return;
            }
        };
        if let Err(e) = self.mpv.command("loadfile", &[uri.as_str(), "append"]) {
            self.shared.emit(EngineEvent::Warning {
                message: format!("gapless successor skipped: {e}"),
            });
            return;
        }
        *self.shared.next.lock().unwrap() = Some(item);
    }

    fn play(&mut self) -> EngineResult<()> {
        self.mpv
            .set_property("pause", false)
            .map_err(|e| EngineError::Other(format!("play: {e}")))
    }

    fn pause(&mut self) -> EngineResult<()> {
        self.mpv
            .set_property("pause", true)
            .map_err(|e| EngineError::Other(format!("pause: {e}")))
    }

    fn stop(&mut self) -> EngineResult<()> {
        *self.shared.next.lock().unwrap() = None;
        // `stop`'s default flags clear mpv's whole playlist too, so nothing
        // queued via `set_next` survives it. mpv reports the interrupted
        // item's end asynchronously as `EndFile(Stop)` — handled by the
        // event thread, unlike `gst.rs`, whose `Null` transition never
        // generates a matching bus message and so pops/emits synchronously
        // here instead.
        self.mpv
            .command("stop", &[])
            .map_err(|e| EngineError::Other(format!("stop: {e}")))
    }

    fn seek(&mut self, position_ms: u64) -> EngineResult<()> {
        let secs = (position_ms as f64 / 1000.0).to_string();
        self.mpv
            .command("seek", &[secs.as_str(), "absolute"])
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
        self.push_volume();
        Ok(())
    }

    fn set_output(&mut self, output: &OutputConfig) -> EngineResult<()> {
        if *output == self.output {
            return Ok(());
        }
        let _ = self.mpv.command("stop", &[]);
        *self.shared.next.lock().unwrap() = None;
        self.apply_output(output)
    }

    fn position(&self) -> Option<(u64, Option<u64>)> {
        let pos: f64 = self.mpv.get_property("time-pos").ok()?;
        let dur: Option<f64> = self.mpv.get_property("duration").ok();
        Some((
            (pos.max(0.0) * 1000.0) as u64,
            dur.map(|d| (d.max(0.0) * 1000.0) as u64),
        ))
    }

    fn signal_path(&self) -> Option<SignalPath> {
        let version = self
            .mpv
            .get_property::<String>("mpv-version")
            .ok()
            .and_then(|s| s.split_whitespace().last().map(str::to_string))
            .unwrap_or_else(|| "?".into());
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
        let device_format = self
            .mpv
            .get_property::<String>("audio-out-params")
            .ok()
            .and_then(|s| describe_node_json(&s));
        let decoder = self.mpv.get_property::<String>("audio-codec").ok();
        let ao = self.mpv.get_property::<String>("current-ao").ok();
        let exclusive = self.output.is_exclusive();
        let device = match &self.output {
            OutputConfig::Exclusive { device }
            | OutputConfig::Shared {
                device: Some(device),
            } => Some(device.clone()),
            OutputConfig::Shared { device: None } => None,
            OutputConfig::Snapcast { host, port } => Some(format!("snapcast tcp://{host}:{port}")),
        };
        let converted = if exclusive {
            let source_params = self
                .mpv
                .get_property::<String>("audio-params")
                .ok()
                .and_then(|s| describe_node_json(&s));
            match (source_params, device_format.clone()) {
                (Some(s), Some(d)) if s != d => Some(format!("engine converted {s} to {d}")),
                _ => None,
            }
        } else if self.output.is_snapcast() {
            use crate::snapcast::{CHANNELS, SAMPLE_RATE};
            Some(format!(
                "resampled to the fixed Snapcast format s{} {SAMPLE_RATE} Hz {CHANNELS}ch (D-034)",
                crate::snapcast::BIT_DEPTH
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
            engine: format!("libmpv {version}"),
            source_format,
            decoder,
            sink: ao
                .or_else(|| Some(self.ao_name.clone()))
                .filter(|s| !s.is_empty()),
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

impl Drop for MpvEngine {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.event_thread.take() {
            let _ = t.join();
        }
        for p in self.temp_files.drain(..) {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn run_event_loop(client: Mpv, shared: Arc<Shared>) {
    while !shared.stop.load(Ordering::Relaxed) {
        match client.wait_event(0.2) {
            None => continue,
            Some(Ok(ev)) => handle_event(&client, &shared, ev),
            Some(Err(e)) => handle_wait_error(&shared, &e),
        }
    }
}

fn handle_event(client: &Mpv, shared: &Shared, ev: MpvEvent<'_>) {
    match ev {
        MpvEvent::StartFile => {
            // `StartFile` fires once per playlist position — including the
            // very first one, for whatever `load()` just set as `current`
            // synchronously before issuing the command. Only consume `next`
            // once `current` has itself already been confirmed playing (see
            // `current_started`): that is what tells this `StartFile` apart
            // as mpv genuinely advancing to the queued successor, rather
            // than the first `StartFile` for `current` itself racing a
            // `set_next` call that landed on `next` in the meantime.
            // `current_started` deliberately stays `true` across `current`'s
            // own `EndFile` (which only clears `playing`, never `current`),
            // so this still reads correctly for the real advance too.
            if *shared.current_started.lock().unwrap() {
                if let Some(item) = shared.next.lock().unwrap().take() {
                    *shared.current.lock().unwrap() = Some(item);
                    *shared.current_started.lock().unwrap() = false;
                }
            }
        }
        MpvEvent::PlaybackRestart => {
            let cur = shared.current.lock().unwrap().clone();
            let same_id = {
                let playing = shared.playing.lock().unwrap();
                playing.as_ref().map(|p| p.id) == cur.as_ref().map(|c| c.id)
            };
            // mpv also fires `playback-restart` after a `seek()` on the same
            // item; only a genuinely new item (per our own bookkeeping, not
            // mpv's) should re-emit `Started`.
            if !same_id {
                *shared.playing.lock().unwrap() = cur.clone();
                *shared.format_emitted_for.lock().unwrap() = None;
                *shared.current_started.lock().unwrap() = true;
                if let Some(c) = cur {
                    shared.emit(EngineEvent::Started { id: c.id });
                    maybe_emit_format(client, shared, c.id);
                }
            }
        }
        MpvEvent::AudioReconfig => {
            if let Some(id) = shared.playing.lock().unwrap().as_ref().map(|p| p.id) {
                maybe_emit_format(client, shared, id);
            }
        }
        MpvEvent::EndFile(reason) => handle_end_file(shared, reason),
        MpvEvent::PropertyChange { name, change, .. } => {
            handle_property_change(shared, name, change)
        }
        _ => {}
    }
}

/// `audio-out-params` (an `MPV_FORMAT_NODE`) settles a couple of
/// `audio-reconfig` events after the first, since this crate's own
/// `PropertyData` cannot decode a `Node`-formatted property-change payload —
/// so this is read on demand via `get_property::<String>` (mpv's own
/// string/JSON rendering of the node) rather than observed.
fn maybe_emit_format(client: &Mpv, shared: &Shared, id: u64) {
    let mut emitted = shared.format_emitted_for.lock().unwrap();
    if *emitted == Some(id) {
        return;
    }
    let Ok(raw) = client.get_property::<String>("audio-out-params") else {
        return;
    };
    if let Some(description) = describe_node_json(&raw) {
        shared.emit(EngineEvent::Format { id, description });
        *emitted = Some(id);
    }
}

fn handle_end_file(shared: &Shared, reason: EndFileReason) {
    if reason == mpv_end_file_reason::Quit || reason == mpv_end_file_reason::Redirect {
        return;
    }
    let Some(item) = shared.playing.lock().unwrap().take() else {
        return;
    };
    shared.emit(EngineEvent::AboutToFinish { id: item.id });
    shared.emit(EngineEvent::Finished { id: item.id });
    if reason == mpv_end_file_reason::Eof && shared.next.lock().unwrap().is_none() {
        shared.emit(EngineEvent::EndOfStream);
    }
}

fn handle_property_change(shared: &Shared, name: &str, change: PropertyData<'_>) {
    match (name, change) {
        ("cache-buffering-state", PropertyData::Int64(percent)) => {
            shared.emit(EngineEvent::Buffering {
                percent: percent.clamp(0, 100) as u8,
            });
        }
        // Safety net for `player.rs`'s buffering-pause-resume gate in case
        // `cache-buffering-state` itself didn't fire with exactly 100.
        ("paused-for-cache", PropertyData::Flag(false)) => {
            shared.emit(EngineEvent::Buffering { percent: 100 });
        }
        _ => {}
    }
}

/// `wait_event` surfaces a failed load/decode (a nonzero `EndFile.error`, in
/// this mpv/crate version) as an `Err` before ever constructing an
/// `Event::EndFile`, so this is the only place a genuine playback error is
/// observable — attributed to whichever item is currently tracked as
/// playing, or failing that, current.
fn handle_wait_error(shared: &Shared, e: &MpvError) {
    let id = shared
        .playing
        .lock()
        .unwrap()
        .take()
        .or_else(|| shared.current.lock().unwrap().take())
        .map(|i| i.id);
    shared.emit(EngineEvent::Error {
        id,
        message: describe_mpv_error(e),
    });
}

fn describe_mpv_error(e: &MpvError) -> String {
    if let MpvError::Raw(code) = e {
        if *code == mpv_error::LoadingFailed {
            return "source could not be opened".to_string();
        }
        if *code == mpv_error::AoInitFailed {
            return "audio output failed to initialize".to_string();
        }
    }
    e.to_string()
}

/// mpv's own literal-length escape (`%N%`) for a sub-option value: a plain
/// `protocol_whitelist=file,http,...` is parsed as five separate (invalid,
/// `=`-less) key/value entries by mpv's key-value-list option parser, since
/// commas are *also* that parser's entry separator.
fn protocol_whitelist_opt() -> String {
    let list = "file,crypto,data,http,https,tcp,tls";
    format!("protocol_whitelist=%{}%{list}", list.len())
}

/// A deliberately simple translation of `RUST_LOG` (tracing's `EnvFilter`
/// mini-language) into mpv's own `msg-level` option: only the first
/// directive's bare level (ignoring any `target=` prefix) is used. Good
/// enough to make `RUST_LOG=debug` also turn up mpv's own internal logging;
/// not a full `EnvFilter` parser, and mpv never surfaces its log messages as
/// `EngineEvent`s here (this crate's `Event::LogMessage` needs
/// `mpv_request_log_messages`, which the crate does not wrap).
fn msg_level_from_rust_log() -> String {
    let raw = std::env::var("RUST_LOG").unwrap_or_default();
    let first = raw.split(',').next().unwrap_or("").trim();
    let level = first
        .rsplit('=')
        .next()
        .unwrap_or(first)
        .trim()
        .to_ascii_lowercase();
    let mpv_level = match level.as_str() {
        "trace" => "trace",
        "debug" => "debug",
        "warn" | "warning" => "warn",
        "error" => "error",
        "off" => "no",
        _ => "info",
    };
    format!("all={mpv_level}")
}

/// Renders an `MPV_FORMAT_NODE` property's string/JSON form (`audio-params`,
/// `audio-out-params`) the same way `gst.rs`'s `describe_sink_caps` renders
/// GStreamer caps, so the two backends' signal-path descriptions read alike.
fn describe_node_json(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let format = value.get("format")?.as_str()?;
    let rate = value.get("samplerate")?.as_i64()?;
    let channels = value
        .get("channel-count")
        .and_then(|c| c.as_i64())
        .unwrap_or(0);
    Some(format!("{format} {rate} Hz {channels}ch"))
}
