//! Exclusive-mode ALSA writer (D-017, D-018): a dedicated `snd_pcm_t` fed by
//! a GStreamer `appsink`, replacing `alsasink` when [`OutputConfig::Exclusive`]
//! is active and this `alsa-direct` feature is on. Linux only.
//!
//! Design lifted from Sone's ALSA writer (`ref:sone/src-tauri/src/audio.rs`)
//! and corrected against `tidalt` (`ref:tidalt/internal/player/alsa.c`) per
//! `audio-pipeline` `references/output-backends.md` §1/§8/§9/§12/§13 and
//! `references/playback-behavior.md` §1/§11-§13 — no code copied verbatim,
//! only the design and the corrected ordering:
//!
//! - Format probing priority: `S32LE`, ALSA `S24_LE` (= GStreamer
//!   `S24_32LE`, 4 bytes/sample), ALSA `S24_3LE` (= GStreamer `S24LE`, 3
//!   bytes/sample — **the naming is inverted between ALSA and GStreamer**),
//!   `FLOAT`, `S16LE`.
//! - Bit-perfect format choice: pass through the source format if the DAC
//!   accepts it; otherwise the *narrowest* lossless integer promotion the
//!   DAC accepts; otherwise refuse — bit-perfect has no fallback ladder.
//! - Set the ALSA period *before* the buffer (some USB DACs report
//!   nonsensical `period_size_min` once the buffer is already set), then
//!   `set_rate` and **read the negotiated rate back**, failing loudly if it
//!   differs — this read-back, not GStreamer caps negotiation, is what
//!   turns an unsupported rate into an actionable message instead of an
//!   opaque pipeline error (the appsink's caps leave rate unconstrained in
//!   bit-perfect mode for exactly this reason).
//! - `snd_pcm_hw_params()` resets `sw_params.start_threshold` to 1; restore
//!   it (and `avail_min`) afterward.
//! - Channel fallback to the device minimum, with a stereo-to-N mix matrix
//!   installed on the GStreamer side at pipeline-build time.
//! - The writer thread pulls samples from the appsink itself (the "dedicated
//!   writer thread owning the `snd_pcm_t`"), keeps the PCM open across
//!   same-format tracks and feeds silence between them (D-018), reopens
//!   with a short silence pre-roll on a format change, and recovers from
//!   XRUN (`EPIPE`), suspend (`ESTRPIPE`) and `ENODEV` explicitly.
//! - Position is corrected for the device's buffered-but-unplayed frames
//!   via `snd_pcm_delay()`, not `frames_written / rate` alone.
#![cfg(feature = "alsa-direct")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use alsa::pcm::{Access, Format as AlsaFormat, Frames, HwParams, PCM, State};
use alsa::{Direction, ValueOr};
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::engine::{EngineError, EngineEvent, EngineResult};
use crate::gst::Shared;

// ---------------------------------------------------------------------------
// Pure logic: format tables, promotion, rate/period/buffer decisions.
// Kept free of any ALSA/GStreamer handle so it is unit-testable without
// hardware (`output-backends.md` §10's "write a unit test for the naming
// inversion" instruction).
// ---------------------------------------------------------------------------

/// Bit-perfect format probing/promotion priority, Sone's order
/// (`output-backends.md` §1's `probe_supported_gst_formats`).
pub const FORMAT_PRIORITY: [AlsaFormat; 5] = [
    AlsaFormat::S32LE,
    AlsaFormat::S24LE,
    AlsaFormat::S243LE,
    AlsaFormat::FloatLE,
    AlsaFormat::S16LE,
];

/// Rates to probe, ascending (`output-backends.md` §1's `probe_supported_rates`).
pub const RATE_LADDER: [u32; 10] = [
    44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000, 705_600, 768_000,
];

/// Default ALSA period, in frames. Set *before* the buffer
/// (`output-backends.md` §1/SKILL.md pitfall #5): some USB DACs report
/// absurd `period_size_min` values once the buffer is already sized.
pub const DEFAULT_PERIOD_FRAMES: Frames = 1024;

/// Silence pre-roll written after every device open/reopen
/// (`playback-behavior.md` §11): DACs mute-relay/resync on a rate change and
/// can swallow the first fraction of a second.
pub const SILENCE_PREROLL_MS: u32 = 250;

/// ALSA <-> GStreamer raw-PCM format name mapping. **The naming is inverted**
/// between the two: ALSA `S24_LE` is 24-in-32 (4 bytes/sample) = GStreamer
/// `S24_32LE`; ALSA `S24_3LE` is packed 24-bit (3 bytes/sample) = GStreamer
/// `S24LE` (SKILL.md pitfall #1). Get this backwards and it's a silent
/// 8-bit shift.
pub fn gst_format_name(f: AlsaFormat) -> Option<&'static str> {
    Some(match f {
        AlsaFormat::S32LE => "S32LE",
        AlsaFormat::S24LE => "S24_32LE",
        AlsaFormat::S243LE => "S24LE",
        AlsaFormat::FloatLE => "F32LE",
        AlsaFormat::S16LE => "S16LE",
        _ => return None,
    })
}

/// Inverse of [`gst_format_name`].
pub fn alsa_format_from_gst(name: &str) -> Option<AlsaFormat> {
    Some(match name {
        "S32LE" => AlsaFormat::S32LE,
        "S24_32LE" => AlsaFormat::S24LE,
        "S24LE" => AlsaFormat::S243LE,
        "F32LE" => AlsaFormat::FloatLE,
        "S16LE" => AlsaFormat::S16LE,
        _ => return None,
    })
}

/// Bytes per sample (one channel, one frame) for the five formats this
/// writer deals in.
pub fn bytes_per_sample(f: AlsaFormat) -> Option<u32> {
    match f {
        AlsaFormat::S32LE | AlsaFormat::S24LE | AlsaFormat::FloatLE => Some(4),
        AlsaFormat::S243LE => Some(3),
        AlsaFormat::S16LE => Some(2),
        _ => None,
    }
}

/// Lossless integer-widening promotion candidates for a source format the
/// DAC does not accept directly, narrowest first
/// (`output-backends.md` §1's `pick_capsfilter_format`, translated from its
/// GStreamer-name table into this module's ALSA-domain names). Every entry
/// is a pure container widening (`audioconvert dithering=none
/// noise-shaping=none`) — never a narrowing, never lossy.
fn lossless_promotions(source: AlsaFormat) -> &'static [AlsaFormat] {
    use AlsaFormat::*;
    match source {
        S16LE => &[S243LE, S24LE, S32LE],
        S243LE => &[S24LE, S32LE],
        S24LE => &[S243LE, S32LE],
        _ => &[],
    }
}

/// The bit-perfect format decision for a source format against what the
/// device actually probed as supported. Bit-perfect has no fallback
/// ladder: [`FormatDecision::Unsupported`] must refuse to play, never fall
/// back to a lossy/converted path silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatDecision {
    PassThrough(AlsaFormat),
    Promoted {
        source: AlsaFormat,
        chosen: AlsaFormat,
    },
    Unsupported,
}

impl FormatDecision {
    /// The format to actually open the PCM with, if playable at all.
    pub fn target(&self) -> Option<AlsaFormat> {
        match self {
            FormatDecision::PassThrough(f) => Some(*f),
            FormatDecision::Promoted { chosen, .. } => Some(*chosen),
            FormatDecision::Unsupported => None,
        }
    }
}

/// `output-backends.md` §1's `pick_capsfilter_format`: pass through if the
/// DAC accepts the source format; otherwise the narrowest lossless
/// promotion the DAC accepts; otherwise refuse (rule 3, "widest probed
/// format as a lossy fallback," is deliberately not implemented — bit-perfect
/// mode never falls back to a lossy path silently; the caller decides
/// whether to refuse outright or drop out of bit-perfect mode).
pub fn pick_format(source: AlsaFormat, supported: &[AlsaFormat]) -> FormatDecision {
    if supported.contains(&source) {
        return FormatDecision::PassThrough(source);
    }
    for &candidate in lossless_promotions(source) {
        if supported.contains(&candidate) {
            return FormatDecision::Promoted {
                source,
                chosen: candidate,
            };
        }
    }
    FormatDecision::Unsupported
}

/// A human `kHz` label matching common rates without spurious trailing
/// zeros (`44100` -> `"44.1"`, `48000` -> `"48"`, `352800` -> `"352.8"`).
fn khz_label(rate: u32) -> String {
    let khz = rate as f64 / 1000.0;
    if (khz - khz.round()).abs() < 1e-6 {
        format!("{}", khz.round() as u64)
    } else {
        format!("{khz:.1}")
    }
}

/// The rate read-back check (`output-backends.md` §1's real bit-perfect
/// guard, the actual justification for a custom writer over `alsasink`
/// per §13): fail loudly, with an actionable message, rather than silently
/// resampling or letting GStreamer report an opaque negotiation error.
pub fn check_negotiated_rate(requested: u32, negotiated: u32) -> Result<(), String> {
    if requested == negotiated {
        Ok(())
    } else {
        Err(format!(
            "DAC doesn't support {} kHz; turn off bit-perfect mode for compatibility",
            khz_label(requested)
        ))
    }
}

/// Buffer size for a given period: `4 * period`
/// (`output-backends.md` §1, tidalt's ordering).
pub fn buffer_frames_for(period: Frames) -> Frames {
    period * 4
}

/// `snd_pcm_hw_params()` resets `sw_params.start_threshold` to 1, which
/// underruns from the first write (SKILL.md pitfall #4). Restore
/// `start_threshold = floor(buffer/period)*period` and `avail_min = period`.
pub fn restore_sw_params(buffer: Frames, period: Frames) -> (Frames, Frames) {
    if period <= 0 {
        return (buffer, period);
    }
    ((buffer / period) * period, period)
}

/// Channel fallback: the requested count if the device supports it,
/// otherwise the device's minimum (`output-backends.md` §1: "pro USB
/// interfaces expose a fixed channel count and reject stereo").
pub fn choose_channel_count(requested: u32, min: u32, max: u32) -> u32 {
    if requested >= min && requested <= max {
        requested
    } else {
        min
    }
}

/// Stereo source onto `out_channels` output channels: unity gain on
/// channels 0/1, digital silence elsewhere (`output-backends.md` §1's
/// `stereo_pad_mix_matrix`), installed on the pipeline at build time, not
/// reactively on a transient 2-channel period. `matrix[out][in]`.
pub fn stereo_to_n_mix_matrix(out_channels: u32) -> Vec<Vec<f32>> {
    (0..out_channels)
        .map(|ch| {
            let mut row = vec![0.0f32; 2];
            if ch < 2 {
                row[ch as usize] = 1.0;
            }
            row
        })
        .collect()
}

/// How a failed `snd_pcm_writei` should be handled
/// (`output-backends.md` §1's writer loop: XRUN/suspend/disconnect).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteRecovery {
    /// Transient (`EINTR`/`EAGAIN`): just retry the write.
    Retry,
    /// Buffer underrun (`EPIPE`): `snd_pcm_prepare()` then a silence kick.
    Xrun,
    /// Device suspended (`ESTRPIPE`): retry `snd_pcm_resume()`.
    Suspended,
    /// Device gone (`ENODEV`): tear down, tell the user.
    Disconnected,
    /// Anything else: not recoverable here.
    Fatal,
}

pub fn classify_write_error(errno: i32) -> WriteRecovery {
    match errno {
        libc::EINTR | libc::EAGAIN => WriteRecovery::Retry,
        libc::EPIPE => WriteRecovery::Xrun,
        libc::ESTRPIPE => WriteRecovery::Suspended,
        libc::ENODEV => WriteRecovery::Disconnected,
        _ => WriteRecovery::Fatal,
    }
}

/// The format actually negotiated for a track, as observed at the appsink
/// ("FormatHint" in `output-backends.md` §1's comment on why bit-perfect
/// mode leaves the rate unconstrained). `format`/`channels` are decided
/// proactively by [`ExclusiveSink::prepare_item`] from the track's declared
/// bit depth (pinned onto the appsink's `caps` property); `rate` is read
/// from the sample actually delivered, since nothing constrains it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatHint {
    pub format: AlsaFormat,
    pub rate: u32,
    pub channels: u32,
}

/// What the ALSA writer currently has the PCM opened as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenParams {
    pub format: AlsaFormat,
    pub rate: u32,
    pub channels: u32,
    pub period_frames: Frames,
}

/// D-018: reopen only when the format actually changes; same-format tracks
/// keep the device open.
pub fn needs_reopen(current: Option<OpenParams>, hint: FormatHint) -> bool {
    match current {
        None => true,
        Some(p) => p.format != hint.format || p.rate != hint.rate || p.channels != hint.channels,
    }
}

pub fn frames_for_ms(ms: u32, rate: u32) -> Frames {
    (u64::from(rate) * u64::from(ms) / 1000) as Frames
}

/// All-zero bytes are silence regardless of format (signed integer PCM and
/// IEEE float both represent zero as an all-zero bit pattern).
fn silence_bytes(frames: Frames, bytes_per_sample: u32, channels: u32) -> Vec<u8> {
    let len = (frames.max(0) as u64) * u64::from(bytes_per_sample) * u64::from(channels);
    vec![0u8; len as usize]
}

/// The source PCM word width assumed from the manifest's declared bit
/// depth. This is a prediction, not an observation: the actual decoder's
/// native raw-audio output is confirmed only once GStreamer negotiates
/// against the pinned caps this decision produces. Unknown depth assumes
/// 16-bit, the narrowest and thus always-promotable case.
fn assumed_source_format(bit_depth: Option<u32>) -> AlsaFormat {
    match bit_depth {
        Some(d) if d <= 16 => AlsaFormat::S16LE,
        Some(24) => AlsaFormat::S24LE,
        Some(d) if d >= 32 => AlsaFormat::S32LE,
        _ => AlsaFormat::S16LE,
    }
}

fn oerr(context: impl std::fmt::Display, e: impl std::fmt::Display) -> EngineError {
    EngineError::Output(format!("{context}: {e}"))
}

// ---------------------------------------------------------------------------
// Device probing and PCM configuration (real ALSA calls; exercised by the
// "null" device integration test, not by the pure unit tests above).
// ---------------------------------------------------------------------------

/// What a device reported for the formats/rates/channels this writer cares
/// about, probed once per device open (`output-backends.md` §1's
/// `probe_supported_gst_formats`/`probe_supported_rates`).
#[derive(Debug, Clone)]
pub struct ProbedDevice {
    pub formats: Vec<AlsaFormat>,
    pub rates: Vec<u32>,
    pub channels_min: u32,
    pub channels_max: u32,
}

/// §2's advisory: always handle `EBUSY` on open with a bounded retry,
/// regardless of whether a (not implemented here — §2 is explicitly
/// optional) `org.freedesktop.ReserveDevice1` handshake ran first.
fn open_with_ebusy_retry(device: &str) -> EngineResult<PCM> {
    let deadline = Instant::now() + Duration::from_millis(800);
    loop {
        match PCM::new(device, Direction::Playback, false) {
            Ok(pcm) => return Ok(pcm),
            Err(e) if e.errno() == libc::EBUSY && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(oerr(format!("open {device}"), e)),
        }
    }
}

fn probe_device(device: &str) -> EngineResult<ProbedDevice> {
    let pcm = open_with_ebusy_retry(device)?;
    let hwp = HwParams::any(&pcm).map_err(|e| oerr("hw_params_any", e))?;
    let formats: Vec<AlsaFormat> = FORMAT_PRIORITY
        .iter()
        .copied()
        .filter(|f| hwp.test_format(*f).is_ok())
        .collect();
    let rates: Vec<u32> = RATE_LADDER
        .iter()
        .copied()
        .filter(|r| hwp.test_rate(*r).is_ok())
        .collect();
    let channels_min = hwp
        .get_channels_min()
        .map_err(|e| oerr("get_channels_min", e))?;
    let channels_max = hwp
        .get_channels_max()
        .map_err(|e| oerr("get_channels_max", e))?;
    // `pcm` (and `hwp`, which borrows it) drop here, closing the probe
    // handle before the writer thread reopens the device for real playback
    // (tidalt's split `open_hw_device`/`configure_hw_pcm`, §1).
    Ok(ProbedDevice {
        formats,
        rates,
        channels_min,
        channels_max,
    })
}

fn configure_sw_params(pcm: &PCM) -> EngineResult<()> {
    let hwp = pcm
        .hw_params_current()
        .map_err(|e| oerr("hw_params_current", e))?;
    let buffer = hwp
        .get_buffer_size()
        .map_err(|e| oerr("get_buffer_size", e))?;
    let period = hwp
        .get_period_size()
        .map_err(|e| oerr("get_period_size", e))?;
    let (start_threshold, avail_min) = restore_sw_params(buffer, period);
    let swp = pcm
        .sw_params_current()
        .map_err(|e| oerr("sw_params_current", e))?;
    swp.set_start_threshold(start_threshold)
        .map_err(|e| oerr("set_start_threshold", e))?;
    swp.set_avail_min(avail_min)
        .map_err(|e| oerr("set_avail_min", e))?;
    pcm.sw_params(&swp).map_err(|e| oerr("sw_params", e))?;
    Ok(())
}

/// Open (or reopen) the PCM for real playback at an already-decided
/// format/rate/channels — no fallback ladder: `set_format` must succeed as
/// requested, and the rate is read back and must match exactly.
fn open_pcm(device: &str, params: OpenParams) -> EngineResult<PCM> {
    let pcm = open_with_ebusy_retry(device)?;
    let hwp = HwParams::any(&pcm).map_err(|e| oerr("hw_params_any", e))?;
    hwp.set_access(Access::RWInterleaved)
        .map_err(|e| oerr("set_access", e))?;
    hwp.set_format(params.format)
        .map_err(|e| oerr("set_format", e))?;
    hwp.set_channels(params.channels)
        .map_err(|e| oerr("set_channels", e))?;
    // Period before buffer (SKILL.md pitfall #5).
    hwp.set_period_size(params.period_frames, ValueOr::Nearest)
        .map_err(|e| oerr("set_period_size", e))?;
    hwp.set_buffer_size(buffer_frames_for(params.period_frames))
        .map_err(|e| oerr("set_buffer_size", e))?;
    hwp.set_rate(params.rate, ValueOr::Nearest)
        .map_err(|e| oerr("set_rate", e))?;
    pcm.hw_params(&hwp).map_err(|e| oerr("hw_params", e))?;
    drop(hwp); // release the borrow on `pcm` before returning it below

    let negotiated_rate = pcm
        .hw_params_current()
        .map_err(|e| oerr("hw_params_current", e))?
        .get_rate()
        .map_err(|e| oerr("get_rate", e))?;
    check_negotiated_rate(params.rate, negotiated_rate).map_err(EngineError::Output)?;

    configure_sw_params(&pcm)?;
    Ok(pcm)
}

/// Write as much of `bytes` as possible; returns the byte offset actually
/// written and, if a write failed, the ALSA error for the caller's
/// recovery logic to act on.
fn write_all(pcm: &PCM, bytes: &[u8]) -> (usize, Option<alsa::Error>) {
    let mut offset = 0usize;
    while offset < bytes.len() {
        let io = pcm.io_bytes();
        match io.writei(&bytes[offset..]) {
            Ok(0) => break,
            Ok(frames) => {
                let written =
                    (pcm.frames_to_bytes(frames as Frames) as usize).min(bytes.len() - offset);
                offset += written;
            }
            Err(e) => return (offset, Some(e)),
        }
    }
    (offset, None)
}

// ---------------------------------------------------------------------------
// The writer thread.
// ---------------------------------------------------------------------------

enum Control {
    Pause(bool),
    Stop,
}

#[derive(Debug, Clone, Copy, Default)]
struct PositionSnapshot {
    frames_written: u64,
    delay_frames: i64,
    rate: u32,
}

/// The format/channels an upcoming (or the current) track is pinned to,
/// decided by [`ExclusiveSink::prepare_item`] and read by the writer thread
/// once per pulled sample.
#[derive(Debug, Clone, Copy)]
struct TargetFormat {
    format: AlsaFormat,
    channels: u32,
}

fn extract_rate(sample: &gst::Sample) -> Option<u32> {
    let caps = sample.caps()?;
    let s = caps.structure(0)?;
    s.get::<i32>("rate").ok().map(|r| r as u32)
}

enum Pulled {
    Sample(gst::Sample),
    Empty,
    Eos,
}

/// Pull from the appsink directly (the "dedicated writer thread" pulling
/// its own data, rather than a signal callback on a GStreamer-owned
/// thread), bounded so the loop can still poll the control channel and
/// keep the DAC clock fed with silence between tracks.
fn try_pull_sample(appsink: &gst::Element, timeout_ns: u64, ever_received: bool) -> Pulled {
    match appsink.emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&timeout_ns]) {
        Some(sample) => Pulled::Sample(sample),
        None => {
            // `eos` also reads true before the pipeline has started, so it
            // only means "really done" once we've actually seen a sample.
            let eos: bool = appsink.property("eos");
            if ever_received && eos {
                Pulled::Eos
            } else {
                Pulled::Empty
            }
        }
    }
}

struct WriterState {
    device: String,
    appsink: gst::Element,
    target: Arc<Mutex<TargetFormat>>,
    position: Arc<Mutex<PositionSnapshot>>,
    generation: Arc<AtomicU64>,
    /// Published copy of `open`, for `ExclusiveSink::device_format_description`
    /// (read from a different thread than the one that owns `open` itself).
    last_open: Arc<Mutex<Option<OpenParams>>>,
    shared: Arc<Shared>,

    pcm: Option<PCM>,
    open: Option<OpenParams>,
    can_hw_pause: bool,
    paused: bool,
    disconnected: bool,
    ever_received: bool,
}

impl WriterState {
    fn current_id(&self) -> Option<u64> {
        self.shared.current.lock().unwrap().as_ref().map(|i| i.id)
    }

    fn reopen(&mut self, hint: FormatHint) -> EngineResult<()> {
        self.pcm = None; // full close before reopen (some USB DACs can't reconfigure in place)
        let params = OpenParams {
            format: hint.format,
            rate: hint.rate,
            channels: hint.channels,
            period_frames: DEFAULT_PERIOD_FRAMES,
        };
        let pcm = open_pcm(&self.device, params)?;
        let can_hw_pause = pcm
            .hw_params_current()
            .map(|h| h.can_pause())
            .unwrap_or(false);

        let preroll_frames = frames_for_ms(SILENCE_PREROLL_MS, hint.rate);
        let bps = bytes_per_sample(hint.format).unwrap_or(2);
        let preroll = silence_bytes(preroll_frames, bps, hint.channels);
        if let (_, Some(e)) = write_all(&pcm, &preroll) {
            return Err(oerr("silence pre-roll", e));
        }

        self.generation.fetch_add(1, Ordering::SeqCst);
        self.can_hw_pause = can_hw_pause;
        self.pcm = Some(pcm);
        self.open = Some(params);
        *self.last_open.lock().unwrap() = Some(params);

        let desc = format!(
            "{} {} Hz {}ch (hw: {})",
            gst_format_name(hint.format).unwrap_or("?"),
            hint.rate,
            hint.channels,
            self.device
        );
        self.shared.emit(EngineEvent::Format {
            id: self.current_id().unwrap_or(0),
            description: desc,
        });
        Ok(())
    }

    fn recover_from(&mut self, e: alsa::Error) -> EngineResult<()> {
        match classify_write_error(e.errno()) {
            WriteRecovery::Retry => Ok(()),
            WriteRecovery::Xrun => {
                let Some(pcm) = self.pcm.as_ref() else {
                    return Ok(());
                };
                pcm.prepare().map_err(|e| oerr("xrun prepare", e))?;
                if let Some(open) = self.open {
                    let bps = bytes_per_sample(open.format).unwrap_or(2);
                    let kick = silence_bytes(open.period_frames, bps, open.channels);
                    let _ = write_all(pcm, &kick);
                }
                self.shared.emit(EngineEvent::Warning {
                    message: "ALSA buffer underrun (XRUN); recovered".into(),
                });
                Ok(())
            }
            WriteRecovery::Suspended => {
                let Some(pcm) = self.pcm.as_ref() else {
                    return Ok(());
                };
                let mut tries = 0;
                loop {
                    match pcm.resume() {
                        Ok(()) => break,
                        Err(re) if re.errno() == libc::EAGAIN && tries < 50 => {
                            tries += 1;
                            thread::sleep(Duration::from_millis(100));
                        }
                        Err(_) => {
                            pcm.prepare().map_err(|e| oerr("suspend prepare", e))?;
                            break;
                        }
                    }
                }
                self.shared.emit(EngineEvent::Warning {
                    message: "audio device suspended; resumed".into(),
                });
                Ok(())
            }
            WriteRecovery::Disconnected => {
                self.pcm = None;
                self.disconnected = true;
                Err(EngineError::Output(format!(
                    "audio device disconnected: {}",
                    self.device
                )))
            }
            WriteRecovery::Fatal => Err(oerr("write", e)),
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let frames_for_position = self
            .open
            .and_then(|o| bytes_per_sample(o.format).map(|bps| (o, bps)))
            .map(|(o, bps)| {
                u64::from(bytes.len() as u32) / (u64::from(bps) * u64::from(o.channels))
            })
            .unwrap_or(0);
        let mut remaining = bytes;
        loop {
            if self.disconnected {
                return;
            }
            let Some(pcm) = self.pcm.as_ref() else {
                return;
            };
            let (written, err) = write_all(pcm, remaining);
            remaining = &remaining[written..];
            match err {
                None => break,
                Some(e) => match self.recover_from(e) {
                    Ok(()) => continue,
                    Err(fatal) => {
                        self.shared.emit(EngineEvent::Error {
                            id: self.current_id(),
                            message: fatal.to_string(),
                        });
                        return;
                    }
                },
            }
        }
        self.publish_position(frames_for_position);
    }

    fn publish_position(&self, frames_written: u64) {
        let mut snap = self.position.lock().unwrap();
        snap.frames_written += frames_written;
        if let Some(pcm) = &self.pcm {
            if let Ok(d) = pcm.delay() {
                snap.delay_frames = d;
            }
        }
        snap.rate = self.open.map(|o| o.rate).unwrap_or(0);
    }

    fn handle_sample(&mut self, sample: gst::Sample) {
        self.ever_received = true;
        let target = *self.target.lock().unwrap();
        let rate =
            extract_rate(&sample).unwrap_or_else(|| self.open.map(|o| o.rate).unwrap_or(44_100));
        let hint = FormatHint {
            format: target.format,
            rate,
            channels: target.channels,
        };
        if needs_reopen(self.open, hint) {
            if let Err(e) = self.reopen(hint) {
                self.shared.emit(EngineEvent::Error {
                    id: self.current_id(),
                    message: e.to_string(),
                });
                self.disconnected = true;
                return;
            }
        }
        let Some(buffer) = sample.buffer() else {
            return;
        };
        let Ok(map) = buffer.map_readable() else {
            return;
        };
        self.write_bytes(map.as_slice());
    }

    /// Feed exactly one period of silence to keep the DAC clock alive
    /// between tracks (D-018's "keeps the PCM open and feeds silence
    /// between tracks") — used both for the idle-between-tracks gap and
    /// for a software pause.
    fn idle_silence(&mut self) {
        let Some(open) = self.open else {
            return;
        };
        let bps = bytes_per_sample(open.format).unwrap_or(2);
        let silence = silence_bytes(open.period_frames, bps, open.channels);
        self.write_bytes(&silence);
    }
}

/// One poll iteration's timeout, sized to roughly one period at the
/// currently-open rate so idle silence keeps pace with real playback
/// without a nested loop; a fixed fallback before anything is open yet.
fn poll_timeout_ns(open: Option<OpenParams>) -> u64 {
    match open {
        Some(o) if o.rate > 0 => (u64::from(o.period_frames.max(1) as u32) * 1_000_000_000
            / u64::from(o.rate))
        .max(5_000_000),
        _ => 50_000_000, // 50ms
    }
}

fn run_writer_loop(mut state: WriterState, control_rx: Receiver<Control>) {
    loop {
        match control_rx.try_recv() {
            Ok(Control::Stop) => break,
            Ok(Control::Pause(p)) => state.paused = p,
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => break,
        }

        if state.paused {
            if state.can_hw_pause {
                if let Some(pcm) = &state.pcm {
                    if pcm.state() != State::Paused {
                        let _ = pcm.pause(true);
                    }
                }
                thread::sleep(Duration::from_millis(50));
            } else {
                // Software pause: keep writing silence to pace the thread
                // and keep the clock alive (Sone's approach, §12).
                state.idle_silence();
            }
            continue;
        }
        if state.can_hw_pause {
            if let Some(pcm) = &state.pcm {
                if pcm.state() == State::Paused {
                    let _ = pcm.pause(false);
                }
            }
        }

        if state.disconnected {
            thread::sleep(Duration::from_millis(200));
            continue;
        }

        let timeout = poll_timeout_ns(state.open);
        match try_pull_sample(&state.appsink, timeout, state.ever_received) {
            Pulled::Sample(sample) => state.handle_sample(sample),
            Pulled::Eos => break,
            Pulled::Empty => {
                if state.open.is_some() {
                    state.idle_silence();
                }
            }
        }
    }
}

/// A handle to the writer thread: pause/resume, position, and a clean stop.
struct AlsaWriter {
    control: Sender<Control>,
    join: Mutex<Option<JoinHandle<()>>>,
    position: Arc<Mutex<PositionSnapshot>>,
    last_open: Arc<Mutex<Option<OpenParams>>>,
}

impl AlsaWriter {
    fn spawn(
        device: String,
        appsink: gst::Element,
        target: Arc<Mutex<TargetFormat>>,
        generation: Arc<AtomicU64>,
        shared: Arc<Shared>,
    ) -> Self {
        let position = Arc::new(Mutex::new(PositionSnapshot::default()));
        let last_open = Arc::new(Mutex::new(None));
        let (control, control_rx) = mpsc::channel();
        let state = WriterState {
            device,
            appsink,
            target,
            position: position.clone(),
            generation,
            last_open: last_open.clone(),
            shared,
            pcm: None,
            open: None,
            can_hw_pause: false,
            paused: false,
            disconnected: false,
            ever_received: false,
        };
        let join = thread::Builder::new()
            .name("streamboat-alsa-writer".into())
            .spawn(move || run_writer_loop(state, control_rx))
            .expect("spawn streamboat-alsa-writer thread");
        Self {
            control,
            join: Mutex::new(Some(join)),
            position,
            last_open,
        }
    }

    fn set_pause(&self, paused: bool) {
        let _ = self.control.send(Control::Pause(paused));
    }

    fn stop(&self) {
        let _ = self.control.send(Control::Stop);
        if let Some(j) = self.join.lock().unwrap().take() {
            let _ = j.join();
        }
    }

    fn position_ms(&self) -> Option<u64> {
        let snap = *self.position.lock().unwrap();
        if snap.rate == 0 {
            return None;
        }
        let played = snap
            .frames_written
            .saturating_sub(snap.delay_frames.max(0) as u64);
        Some(played * 1000 / u64::from(snap.rate))
    }

    fn last_open(&self) -> Option<OpenParams> {
        *self.last_open.lock().unwrap()
    }
}

impl Drop for AlsaWriter {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// GStreamer glue: the sink bin (`audioconvert ! appsink`) and the per-item
// format decision, wired into `GstEngine`.
// ---------------------------------------------------------------------------

/// The exclusive-mode ALSA output: an `audioconvert ! appsink` bin (the
/// `audioconvert` only ever does a lossless container widening or a
/// stereo-to-N mix, per [`FormatDecision`]/[`stereo_to_n_mix_matrix`]) plus
/// the writer thread that pulls from the appsink and owns the `snd_pcm_t`.
pub struct ExclusiveSink {
    pub bin: gst::Element,
    appsink: gst::Element,
    probed: ProbedDevice,
    channels: u32,
    device: String,
    target: Arc<Mutex<TargetFormat>>,
    writer: AlsaWriter,
    generation: Arc<AtomicU64>,
    shared: Arc<Shared>,
    last_decision: Mutex<Option<FormatDecision>>,
}

fn apply_mix_matrix(audioconvert: &gst::Element, channels: u32) {
    let rows = stereo_to_n_mix_matrix(channels);
    let matrix = gst::Array::new(rows.into_iter().map(|row| {
        let inner: gst::Array = gst::Array::new(row.into_iter().map(f64::from));
        inner
    }));
    audioconvert.set_property("mix-matrix", matrix);
}

impl ExclusiveSink {
    /// `Shared` is `pub(crate)`, so this constructor is crate-internal;
    /// `GstEngine::apply_output` is the only caller.
    pub(crate) fn new(device: &str, shared: Arc<Shared>) -> EngineResult<Self> {
        let probed = probe_device(device)?;
        if probed.formats.is_empty() {
            return Err(EngineError::Output(format!(
                "{device}: none of the bit-perfect PCM formats probed successfully"
            )));
        }
        if probed.rates.is_empty() {
            shared.emit(EngineEvent::Warning {
                message: format!(
                    "{device}: rate probe against the common rate ladder returned nothing; \
                     bit-perfect mode still relies on the per-open rate read-back"
                ),
            });
        }
        let min = probed.channels_min.max(1);
        let max = probed.channels_max.max(min);
        let channels = choose_channel_count(2, min, max);

        let audioconvert = gst::ElementFactory::make("audioconvert")
            .build()
            .map_err(|e| oerr("audioconvert", e))?;
        audioconvert.set_property_from_str("dithering", "none");
        audioconvert.set_property_from_str("noise-shaping", "none");
        if channels != 2 {
            apply_mix_matrix(&audioconvert, channels);
        }

        let appsink = gst::ElementFactory::make("appsink")
            .build()
            .map_err(|e| oerr("appsink", e))?;
        appsink.set_property("sync", false);
        appsink.set_property("emit-signals", false);
        appsink.set_property("drop", false);
        appsink.set_property("max-buffers", 8u32);

        let bin = gst::Bin::new();
        bin.add_many([&audioconvert, &appsink])
            .map_err(|e| oerr("bin add", e))?;
        gst::Element::link_many([&audioconvert, &appsink]).map_err(|e| oerr("bin link", e))?;
        let sink_pad = audioconvert
            .static_pad("sink")
            .ok_or_else(|| EngineError::Output("audioconvert has no sink pad".into()))?;
        let ghost = gst::GhostPad::with_target(&sink_pad).map_err(|e| oerr("ghost pad", e))?;
        bin.add_pad(&ghost).map_err(|e| oerr("bin add_pad", e))?;

        let target = Arc::new(Mutex::new(TargetFormat {
            format: probed.formats[0],
            channels,
        }));
        let generation = Arc::new(AtomicU64::new(0));

        let writer = AlsaWriter::spawn(
            device.to_string(),
            appsink.clone(),
            target.clone(),
            generation.clone(),
            shared.clone(),
        );

        Ok(Self {
            bin: bin.upcast(),
            appsink,
            probed,
            channels,
            device: device.to_string(),
            target,
            writer,
            generation,
            shared,
            last_decision: Mutex::new(None),
        })
    }

    /// Decide and pin the ALSA-domain format for an upcoming (or the
    /// current) item from its declared bit depth, updating the appsink's
    /// `caps` property so GStreamer negotiates into exactly that format
    /// (D-018's proactive half of the `FormatHint`; the writer confirms the
    /// rate reactively per sample). Called from `load()` for the first
    /// track and from the `about-to-finish` handler for the gapless
    /// successor.
    pub fn prepare_item(&self, bit_depth: Option<u32>) -> EngineResult<FormatDecision> {
        let source = assumed_source_format(bit_depth);
        let decision = pick_format(source, &self.probed.formats);
        let Some(chosen) = decision.target() else {
            return Err(EngineError::Output(format!(
                "{}: DAC supports none of {source:?}'s lossless promotions; \
                 bit-perfect mode has no fallback ladder — turn it off for this track",
                self.device
            )));
        };
        *self.target.lock().unwrap() = TargetFormat {
            format: chosen,
            channels: self.channels,
        };
        let caps = gst::Caps::builder("audio/x-raw")
            .field("format", gst_format_name(chosen).unwrap_or("S16LE"))
            .field("channels", self.channels as i32)
            .field("layout", "interleaved")
            .build();
        self.appsink.set_property("caps", &caps);
        if let FormatDecision::Promoted { source, chosen } = decision {
            self.shared.emit(EngineEvent::Warning {
                message: format!(
                    "promoted {source:?} source to {chosen:?} for bit-perfect output on {} (lossless)",
                    self.device
                ),
            });
        }
        *self.last_decision.lock().unwrap() = Some(decision);
        Ok(decision)
    }

    pub fn set_pause(&self, paused: bool) {
        self.writer.set_pause(paused);
    }

    pub fn position_ms(&self) -> Option<u64> {
        self.writer.position_ms()
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    /// The device format actually negotiated (read back from `hw_params`),
    /// not a guess from GStreamer caps — `None` until the writer has opened
    /// the PCM for the first real sample.
    pub fn device_format_description(&self) -> Option<String> {
        let open = self.writer.last_open()?;
        Some(format!(
            "{} {} Hz {}ch",
            gst_format_name(open.format).unwrap_or("?"),
            open.rate,
            open.channels
        ))
    }

    /// `Some(reason)` when the last format decision needed a lossless
    /// promotion, mirroring `SignalPath::converted`'s contract.
    pub fn converted_description(&self) -> Option<String> {
        match *self.last_decision.lock().unwrap() {
            Some(FormatDecision::Promoted { source, chosen }) => Some(format!(
                "promoted {source:?} source to {chosen:?} (lossless container widening)"
            )),
            _ => None,
        }
    }

    /// Bit-perfect mode has no lossy fallback: once a format decision has
    /// actually opened the device, the path is bit-perfect by construction
    /// (pass-through or a lossless promotion). `None` before the first
    /// track has opened the PCM.
    pub fn bit_perfect(&self) -> Option<bool> {
        self.writer.last_open().map(|_| true)
    }

    pub fn stop(&self) {
        self.writer.stop();
    }
}

impl Drop for ExclusiveSink {
    fn drop(&mut self) {
        self.writer.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alsa_gst_format_naming_is_inverted() {
        // SKILL.md pitfall #1: ALSA S24_LE (4 bytes) = GStreamer S24_32LE;
        // ALSA S24_3LE (3 bytes) = GStreamer S24LE.
        assert_eq!(gst_format_name(AlsaFormat::S24LE), Some("S24_32LE"));
        assert_eq!(gst_format_name(AlsaFormat::S243LE), Some("S24LE"));
        assert_eq!(bytes_per_sample(AlsaFormat::S24LE), Some(4));
        assert_eq!(bytes_per_sample(AlsaFormat::S243LE), Some(3));
    }

    #[test]
    fn format_name_mapping_round_trips() {
        for f in FORMAT_PRIORITY {
            let name = gst_format_name(f).expect("named");
            assert_eq!(alsa_format_from_gst(name), Some(f));
        }
    }

    #[test]
    fn pick_format_passes_through_when_supported() {
        let supported = [AlsaFormat::S16LE, AlsaFormat::S32LE];
        assert_eq!(
            pick_format(AlsaFormat::S16LE, &supported),
            FormatDecision::PassThrough(AlsaFormat::S16LE)
        );
    }

    #[test]
    fn pick_format_promotes_to_the_narrowest_lossless_option() {
        // DAC only does 24-in-32 and 32-bit: a 16-bit source should promote
        // to the narrower S24LE (=ALSA S24_32) container, not straight to S32.
        let supported = [AlsaFormat::S24LE, AlsaFormat::S32LE];
        assert_eq!(
            pick_format(AlsaFormat::S16LE, &supported),
            FormatDecision::Promoted {
                source: AlsaFormat::S16LE,
                chosen: AlsaFormat::S24LE,
            }
        );
    }

    #[test]
    fn pick_format_promotes_packed_24_to_32_in_32_before_32() {
        let supported = [AlsaFormat::S24LE, AlsaFormat::S32LE];
        assert_eq!(
            pick_format(AlsaFormat::S243LE, &supported),
            FormatDecision::Promoted {
                source: AlsaFormat::S243LE,
                chosen: AlsaFormat::S24LE,
            }
        );
    }

    #[test]
    fn pick_format_refuses_when_no_lossless_promotion_exists() {
        // A float-only DAC cannot losslessly host an integer source in this
        // ladder; bit-perfect has no fallback, so it must refuse.
        let supported = [AlsaFormat::FloatLE];
        assert_eq!(
            pick_format(AlsaFormat::S16LE, &supported),
            FormatDecision::Unsupported
        );
    }

    #[test]
    fn rate_readback_matches_is_ok() {
        assert!(check_negotiated_rate(96_000, 96_000).is_ok());
    }

    #[test]
    fn rate_readback_mismatch_produces_the_actionable_message() {
        let err = check_negotiated_rate(192_000, 96_000).unwrap_err();
        assert_eq!(
            err,
            "DAC doesn't support 192 kHz; turn off bit-perfect mode for compatibility"
        );
    }

    #[test]
    fn khz_label_formats_common_rates() {
        assert_eq!(khz_label(44_100), "44.1");
        assert_eq!(khz_label(48_000), "48");
        assert_eq!(khz_label(96_000), "96");
        assert_eq!(khz_label(192_000), "192");
        assert_eq!(khz_label(352_800), "352.8");
    }

    #[test]
    fn buffer_is_four_periods() {
        assert_eq!(buffer_frames_for(1024), 4096);
    }

    #[test]
    fn sw_params_restore_floors_to_a_whole_number_of_periods() {
        assert_eq!(restore_sw_params(4096, 1024), (4096, 1024));
        // A buffer that isn't an exact multiple of the period still floors
        // start_threshold to a whole number of periods.
        assert_eq!(restore_sw_params(4100, 1024), (4096, 1024));
    }

    #[test]
    fn channel_fallback_prefers_requested_within_range() {
        assert_eq!(choose_channel_count(2, 1, 8), 2);
    }

    #[test]
    fn channel_fallback_drops_to_device_minimum_outside_range() {
        // A pro interface that only offers 4-8 channels rejects stereo.
        assert_eq!(choose_channel_count(2, 4, 8), 4);
    }

    #[test]
    fn mix_matrix_is_identity_for_stereo() {
        let m = stereo_to_n_mix_matrix(2);
        assert_eq!(m, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
    }

    #[test]
    fn mix_matrix_puts_stereo_on_the_first_two_channels_with_silence_elsewhere() {
        let m = stereo_to_n_mix_matrix(6);
        assert_eq!(m.len(), 6);
        assert_eq!(m[0], vec![1.0, 0.0]);
        assert_eq!(m[1], vec![0.0, 1.0]);
        for row in &m[2..] {
            assert_eq!(row, &vec![0.0, 0.0]);
        }
    }

    #[test]
    fn write_error_classification_matches_alsa_recovery_paths() {
        assert_eq!(classify_write_error(libc::EPIPE), WriteRecovery::Xrun);
        assert_eq!(
            classify_write_error(libc::ESTRPIPE),
            WriteRecovery::Suspended
        );
        assert_eq!(
            classify_write_error(libc::ENODEV),
            WriteRecovery::Disconnected
        );
        assert_eq!(classify_write_error(libc::EAGAIN), WriteRecovery::Retry);
        assert_eq!(classify_write_error(libc::EINVAL), WriteRecovery::Fatal);
    }

    #[test]
    fn reopen_is_needed_only_on_an_actual_format_change() {
        let a = OpenParams {
            format: AlsaFormat::S16LE,
            rate: 44_100,
            channels: 2,
            period_frames: 1024,
        };
        assert!(needs_reopen(None, hint_of(a)));
        assert!(!needs_reopen(
            Some(a),
            FormatHint {
                format: a.format,
                rate: a.rate,
                channels: a.channels,
            }
        ));
        assert!(needs_reopen(
            Some(a),
            FormatHint {
                format: a.format,
                rate: 96_000,
                channels: a.channels,
            }
        ));
    }

    fn hint_of(p: OpenParams) -> FormatHint {
        FormatHint {
            format: p.format,
            rate: p.rate,
            channels: p.channels,
        }
    }

    #[test]
    fn silence_is_all_zero_bytes_of_the_right_length() {
        let s = silence_bytes(4, 4, 2);
        assert_eq!(s.len(), 4 * 4 * 2);
        assert!(s.iter().all(|&b| b == 0));
    }

    #[test]
    fn frames_for_ms_matches_the_expected_rate_math() {
        assert_eq!(frames_for_ms(250, 44_100), 11_025);
        assert_eq!(frames_for_ms(1000, 48_000), 48_000);
    }

    #[test]
    fn unknown_bit_depth_assumes_the_safest_narrowest_source() {
        assert_eq!(assumed_source_format(None), AlsaFormat::S16LE);
        assert_eq!(assumed_source_format(Some(16)), AlsaFormat::S16LE);
        assert_eq!(assumed_source_format(Some(24)), AlsaFormat::S24LE);
        assert_eq!(assumed_source_format(Some(32)), AlsaFormat::S32LE);
    }

    #[test]
    fn extract_rate_reads_the_negotiated_caps() {
        gst::init().unwrap();
        let caps = gst::Caps::builder("audio/x-raw")
            .field("format", "S16LE")
            .field("rate", 96_000i32)
            .field("channels", 2i32)
            .field("layout", "interleaved")
            .build();
        let buffer = gst::Buffer::new();
        let sample = gst::Sample::builder().buffer(&buffer).caps(&caps).build();
        assert_eq!(extract_rate(&sample), Some(96_000));
    }
}
