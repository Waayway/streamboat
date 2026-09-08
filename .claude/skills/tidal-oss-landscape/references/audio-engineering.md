# Audio engineering: bit-perfect output, gapless, normalization, the pure-Rust question

Full narrative: `/home/user/streamboat/docs/research/oss-landscape.md` §1.6, §2.3, §4.3, §5, §13,
§18-B/C/D. For the DSP/output engineering discussion beyond what's needed here (resampling
theory, buffer sizing rationale, more platform detail), see `docs/research/audio-pipeline.md` —
that report and this skill both point at each other rather than duplicating.

## Table of contents

1. Sone's bit-perfect ALSA negotiation (the most sophisticated artifact in the landscape)
2. Gapless playback: three designs compared
3. DASH-specific caps handling in the bit-perfect path
4. Normalization: three formulas, one incompatibility to know about
5. The pure-Rust audio-stack question: librespot, cpal, wasapi
6. macOS exclusive output: two precedents that exist outside this reference set
7. tidalt's two ALSA refinements Sone lacks
8. Volume curves
9. Mid-session stream-URL/manifest expiry — unsolved anywhere in the reference set
10. macOS: audio output is one of five from-scratch subsystems, not a standalone gap

## 1. Sone's bit-perfect ALSA negotiation

Source: `ref:sone/src-tauri/src/audio.rs`. This is explicitly recommended for wholesale porting.

**Two backends** (lines 105-122): `Normal` (`uridecodebin → [branch queue] → concat →
audioconvert → audioresample → norm_volume → user_volume → autoaudiosink`) and `DirectAlsa`
(exclusive/bit-perfect: `uridecodebin → audioconvert [→ audioresample] [→ capsfilter] → appsink`,
with a separate thread pulling PCM from the appsink and writing to a raw `alsa::PCM` device).
**Gapless is deliberately disabled on `DirectAlsa`** — this is the one thing streamboat should
try to do better than Sone (see §2 below).

**Format probing** (`audio.rs:483-506`): `HwParams::any(pcm)` then `test_format` over
`[S32LE, S24LE, S243LE, FloatLE, S16LE]`.

**The ALSA↔GStreamer 24-bit naming is inverted — getting this backwards is a silent corruption
bug.** Canonical treatment (with the unit-test recommendation) is owned by
`audio-pipeline/references/output-backends.md` §1 — cite it rather than restating the mapping;
Sone's own encode/decode of it is `gst_format_to_alsa`/`alsa_format_to_gst` (`audio.rs:189-215`).

**Rate probing** (`audio.rs:542-563`): `test_rate` over `44100, 48000, 88200, 96000, 176400,
192000, 352800, 384000, 705600, 768000`, with a safe fallback of `vec![44100, 48000]` if probing
fails; format probing falls back to `vec!["S32LE"]`.

**Capsfilter selection — pass through, then narrowest lossless promotion, then widest, with a
truthful message** (`audio.rs:508-540`): if the DAC supports the source format, pass through
unchanged. Else pick the *narrowest lossless promotion* the DAC supports — because
`audioconvert` with `dithering=none` does pure integer bit-shifts between these pairs, which is
lossless:
- `S16LE → [S24LE, S24_32LE, S32LE]`
- `S24LE → [S24_32LE, S32LE]`
- `S24_32LE → [S24LE, S32LE]`

Else fall back to the DAC's widest probed format, and **notify the user truthfully** with the
actual from→to conversion — not a generic "quality reduced" message.

**hw_params** (`audio.rs:569-751`): `Access::RWInterleaved`. In bit-perfect mode,
`set_rate_resample(false)` followed by a **post-hoc `get_rate()` equality check** that produces
*"DAC doesn't support {N}kHz — turn off bit-perfect mode for compatibility"* instead of an opaque
ALSA error. Channel negotiation falls back to `get_channels_min()` for pro interfaces
(Focusrite/Audient) that reject 2-channel outright. `set_buffer_time_near(500_000)` (500 ms),
`set_period_time_near(50_000)` (50 ms).

**sw_params — a real bug Sone works around**: `snd_pcm_hw_params()` resets `start_threshold` to
1, which causes underruns from frame one. Sone re-sets `start_threshold` to the largest
period-aligned value ≤ buffer_size (matching what `alsasink` does internally) and
`avail_min = period_size`.

**Fixed-channel DACs**: a `stereo_pad_mix_matrix(N)` is installed on `audioconvert` at *build*
time (`audio.rs:2874-2887`) so the first caps event resolves directly to the device's channel
count, avoiding a 2-channel transient that would thrash the ALSA writer.

## 2. Gapless playback: three designs compared

| Project | Mechanism | Trade-off |
| --- | --- | --- |
| Sone `Normal` | `concat` element at the pipeline *head*; a per-branch `queue` prerolls the next track's decoder while `concat`'s gate is closed; all pad-slot attach/detach ops on `concat` are funnelled through a **single serialized executor thread** consuming an `AttachJob::{Attach,Detach}` mpsc, because *"pad-slot operations on `concat` must never race"* (`ref:sone/src-tauri/src/audio.rs:43-88,255-482`). Gives you signal-path control (you can inspect/transform between tracks). | Disabled entirely on the `DirectAlsa` bit-perfect path — see below. Runtime availability check is just `gst::ElementFactory::find("concat").is_some()` (`audio.rs:3302-3309`) — the README's "requires GStreamer 1.24+" is a documentation claim only, not enforced in code. |
| High Tide | `playbin3`'s built-in `about-to-finish` signal sets the next URI (`ref:high-tide/src/lib/player_object.py:83-115,184-247`). | Simpler; no signal-path control; **`pipewiresink` disables gapless** (`self.gapless_enabled = False`) — an empirical finding worth carrying forward regardless of which gapless design streamboat picks. Falls back to plain `playbin` (gapless off) if `playbin3` is unavailable. |
| tidalt | None implemented — direct `snd_pcm` writes with `plughw:` fallback, no gapless. | Simplicity over feature completeness; consistent with its "young, AI-authored, Linux-only" scope. |

**The branch queue's buffer sizing**: both the branch decoder and the branch queue are configured
at **15 s** (`audio.rs:266-273`) — the same as the main DASH path (§3 below), *not* the "~3 s" a
stale in-source docstring at `audio.rs:249-250` claims. This is a general lesson, not just a
number to correct: **read the expression that sets a value, not a nearby comment**, before
porting a parameter — Sone's own source has at least two more instances of a stale comment
diverging from the code next to it (see `verification-notes.md`).

**The obvious unexplored combination — Sone gates these apart, nobody has tried the
combination**: a single long-lived ALSA writer thread fed by a `concat`-headed appsink chain,
i.e. gapless *and* bit-perfect together. This is streamboat's most novel engineering claim if
attempted — prototype it before it becomes a design commitment, because there is zero precedent
either way for how cleanly `concat` and a raw ALSA writer negotiate a format change across a
track boundary.

**Seeking is the transport operation most likely to break gapless — not covered above, and not
covered anywhere in the original version of this file.** On GStreamer 1.24.2, a
`seek_simple(FLUSH|KEY_UNIT)` forwards `FLUSH_START`/`FLUSH_STOP` and the new `SEGMENT` only to
`concat`'s **active** sink pad — the inactive prerolled next-track branch sees no flush events,
stays linked and `PLAYING`, and `concat` still switches to it correctly at the active branch's
EOS (`ref:sone/src-tauri/src/audio.rs:2307-2365`). **A seek must not detach the armed next-track
slot** — Sone tried that once and it destroyed a valid preroll on every seek. On the `DirectAlsa`
path a seek also bumps generation counters (dropping in-flight stale PCM chunks) and re-bases
`frames_written` from `position_secs * current_sample_rate` — position on the bit-perfect path is
derived from frames actually written to the PCM device, not a GStreamer position query. Full
detail and the frontend gapless-*arming* policy (a separate, entirely undocumented-elsewhere
problem — when to call into this machinery at all) are in `sone-deep-dive.md` §3a/§3b.

## 3. DASH-specific caps handling in the bit-perfect path

Source: `ref:sone/src-tauri/src/audio.rs:2844-3000` (appsink pipeline construction).

DASH gets `buffer-duration = 15 s` vs `5 s` for BTS, and `use-buffering = true` in both. The
appsink's caps are constrained to DAC-supported *formats* in both bit-perfect and non-bit-perfect
modes, but the *rate* is constrained only in **non**-bit-perfect mode — in bit-perfect mode there
is no resampler downstream, so pinning the rate on the appsink caps makes negotiation fail with a
generic "Internal data stream error" instead of the actionable message from §1's hw_params check.
Getting this ordering backwards silently degrades the actionable-error UX that is otherwise the
whole point of doing bit-perfect negotiation carefully.

## 4. Normalization: three implementations, one incompatibility to know about

Canonical TIDAL gain formula (`min(10^((replay_gain+pre_amp)/20), 1/peak)`, `pre_amp=4.0`, no
extra attenuation factor) is owned by `audio-pipeline/references/playback-behavior.md` §4 — this
table is about implementation *mechanism*, not the formula itself. Note Sone's own `norm_gain`
adds an extra `0.8` factor that is **not** part of TIDAL's formula (its "Tidal-correct" code
comment is misleading — TIDAL's own Android/web SDKs have no `0.8`); don't read this table as
attesting a `0.8 * ...` TIDAL formula.

| Project | Formula | Notes |
| --- | --- | --- |
| Sone | `norm_gain = 0.8 * min(10^((replay_gain + 4)/20), 1/peak_amplitude)` — the `0.8` is Sone's own addition, not TIDAL's | Applied as a separate `volume` GStreamer element in `Normal` mode, or as a scalar in the ALSA writer (absent entirely in bit-perfect mode). Album-vs-track gain selected by playback context (`use_track_gain`), each falling back to the other if its own value is missing. Source: `ref:sone/src-tauri/src/commands/playback.rs:9-21,127-146`. |
| High Tide | GStreamer-native chain: `taginject name=rgtags <tags> ! rgvolume pre-amp=4.0 fallback-gain=-10 headroom=6.0 ! rglimiter ! audioconvert`. Tags injected per-track from `stream.track_replay_gain`/`album_replay_gain`, **skipped entirely when the value is exactly `1.0`** (i.e. no ReplayGain data). Source: `ref:high-tide/src/lib/player_object.py:196-206,578-600`. |
| Strawberry | **Two independent stages, not one**: ReplayGain (`rgvolume`+`rglimiter`+converter) *or* **EBU R128 loudness normalization** as a separate `volume_ebur128_` stage. **Enabling R128 force-links the downstream chain through `audio/x-raw, format = {F32LE, F64LE}`** (`gstenginepipeline.cpp:953-961`) — this is the one incompatibility to carry forward: **a float-domain normalizer is mutually exclusive with bit-perfect integer output**, not merely "should be bypassed" in bit-perfect mode. Any normalization stage streamboat builds must be structurally bypassable, not just defaulted off, for exactly this reason. |

Both Sone and High Tide use a **+4 dB pre-amp** specifically "to match TIDAL web's volume" — worth
matching if streamboat wants its perceived loudness to feel consistent with the official web
player at the same ReplayGain-normalized setting.

## 5. The pure-Rust audio-stack question: librespot, cpal, wasapi

**A pure-Rust core+trait+many-clients architecture does have a precedent — for Spotify, not
TIDAL.** librespot (https://github.com/librespot-org/librespot, MIT, ~7.1k★) is architecturally
the closest thing to what streamboat needs anywhere, reference set or not: a core library, thin
downstream frontends (`spotifyd` daemon, `ncspot` TUI, `librespot-java`), and a `Sink` trait
(`start()`/`stop()`/`write(AudioPacket, &mut Converter)`) with a `sink_as_bytes!` macro handling
F32/S32/S24/S16 conversion, registered by name in `pub const BACKENDS: &[(&str, SinkBuilder)]`
(`playback/src/audio_backend/mod.rs`). Nine backends behind cargo features: `alsa`, `pulseaudio`,
`jack`, `portaudio`, **`rodio`+`cpal` (default)**, `rodiojack`, `sdl2`, `gstreamer`. Decoding is
**Symphonia 0.5**. Its own disclaimers are exactly streamboat's intended posture: *"librespot
only works with Spotify Premium. This will remain the case…"* and *"Using this code to connect to
Spotify's API is probably forbidden by them. Use at your own risk."*

**This refutes "pure Rust has zero precedent for this shape."** It does *not* solve TIDAL's
specific problem — DASH/BTS manifest handling and FLAC-in-MP4 demuxing are not things librespot
needs (Spotify's own manifest format differs) and remain real, unstarted work under a pure-Rust
choice.

**`cpal` cannot do WASAPI exclusive mode at all** — this is the concrete cost of the pure-Rust
option on Windows, not a hypothetical. It's a long-standing open request
(`RustAudio/cpal` issue #459), and cpal's own guidance points at ASIO as the low-latency
workaround, which is a different and heavier integration. The standalone `wasapi` crate
(`HEnquist/wasapi-rs`) *does* support both shared and exclusive modes — it's what CamillaDSP uses
(§6). **Conclusion**: a pure-Rust streamboat needs three hand-written output backends behind a
librespot-`Sink`-shaped trait regardless of decode library — `alsa` crate for Linux `hw:`,
`wasapi` crate for Windows exclusive, `coreaudio-rs` for macOS hog mode. The GStreamer route gets
Linux for free via the same `alsa` crate path Sone already wrote, and Windows for free via
`wasapi2sink exclusive=true`, but gets **nothing** on macOS (`osxaudiosink` has no exclusive
property at all — confirmed by reading `ref:strawberry/src/engine/gstengine.cpp:523-525`, which
lists only `wasapisink`/`wasapi2sink` in `ExclusiveModeSupport()`). **Price the macOS output
backend as hand-written work under either stack choice** — this is not a reason to prefer one
stack over the other on macOS specifically; §6 covers what to build it from.

## 6. macOS exclusive output: two precedents that exist outside this reference set

No project inside the `ref/` checkouts ships macOS bit-perfect/exclusive output for TIDAL or
anything else — TidalSwift is AVPlayer-based (no exclusive mode possible through that API at
all), and Strawberry's macOS path uses plain `osxaudiosink` with no exclusive property. That does
**not** mean the problem is unresearched in general — two usable external precedents exist:

1. **CamillaDSP** (`HEnquist`, Rust, https://github.com/HEnquist/camilladsp) supports ALSA,
   PulseAudio, Jack, WASAPI (shared *and* exclusive) and CoreAudio in one codebase. Its CoreAudio
   playback device has an `exclusive` setting explicitly documented as hog mode, implemented via
   an extended fork of `coreaudio-rs`, plus playback-driven rate control across ALSA/WASAPI/
   CoreAudio alike. See its `backend_coreaudio.md` and `backend_wasapi.md` docs.
2. **MPD**'s `src/output/plugins/OSXOutputPlugin.cxx` is the C++ reference implementation: a
   `hog_device` option that sets the device's hog PID via `kAudioDevicePropertyHogMode`, and
   `osx_output_set_device_format()` scoring and applying `kAudioStreamPropertyPhysicalFormat` to
   switch the device's sample rate/format, plus DoP (DSD-over-PCM) support.

`docs/research/audio-pipeline.md:677-681,1117,1501` already cites CamillaDSP's CoreAudio backend
for this — cross-reference it rather than treating macOS exclusive output as an unresearched gap
in any streamboat design doc. This is now a **scoping decision** (build it or defer it), not an
open research question.

## 7. tidalt's two ALSA refinements Sone lacks

Source: `ref:tidalt/internal/player/{mpv.go,alsa_fallback_test.go}`. Sone does not do either of
these; both are worth adding on top of Sone's negotiation logic (§1) if streamboat ports it. Full
recipe — the two format-preference orders, period-before-buffer ordering with the 87-frame
`period_size_min` anecdote, `org.freedesktop.ReserveDevice1` reservation with release-on-pause, and
format-refusal-vs-EBUSY with memoised `plughw:` fallback — is owned by
`audio-pipeline/references/output-backends.md` §1-2 (which also has the reservation timing budget
and the vacuous-success failure mode); cite it rather than restating. The headline shape: acquire
PipeWire device reservation over D-Bus and **release it on pause, not only on stop**
(`ref:tidalt/README.md:12` — an earlier draft of this file said "release on stop," the less
cooperative, incorrect version); and distinguish a format-negotiation refusal (fall back to
`plughw:`, honestly report as *not* bit-perfect) from a device-busy error (keep retrying `hw:`) —
conflating the two is why naive implementations silently and permanently drop to `plughw:` on a
transient busy error.

**Combine with device hot-plug/busy/lost handling from the other two GUI clients — no single
project has the complete policy.** Sone has a device-busy retry loop on the exclusive path (its
own comment: `isPlaying` "flickers false during device-busy retries", which is why gapless-arming
must not gate on it — `sone-deep-dive.md` §3b) and caches the device list in `AppState` with no
documented hot-plug invalidation. High Tide toasts "ALSA Audio Device is not available" and pauses
on a `"disconnected"` bus error, and restarts the pipeline on the same track for a `"not-linked"`
bus error. Combine Sone's retry-don't-fail-on-busy, High Tide's restart-on-not-linked, and
tidalt's release-on-pause into one policy before shipping exclusive mode.

## 8. Volume curves

- **Sone**: `slider_to_amplitude(v) = clamp(v, 0, 1)^3` — cubic taper, ~50 dB range
  (`audio.rs:217-222`). In bit-perfect mode the user-volume element is absent entirely and the
  slider is locked at 100% (matching the "no software attenuation in bit-perfect mode"
  philosophy).
- **High Tide**: `playbin.volume = value**2` when "quadratic volume" mode is on in settings, else
  linear (a per-user GSettings toggle, not automatic).

## 9. Mid-session stream-URL/manifest expiry — unsolved anywhere in the reference set (added by
the third fact-check pass)

Implication 19 (`oss-landscape.md`) says never cache manifests or stream URLs across *restarts*
(they expire in minutes to an hour). What it doesn't address, and what this section exists to flag
explicitly, is expiry **within** a session — and two of this skill's own recommendations create
the exposure: gapless arming resolves the next track's URI as soon as it's predicted (§2 above),
and a user can pause for an hour or close a laptop lid before that slot is consumed. On resume,
the armed URI may already be dead.

**Nobody in the reference set solves this.** A grep for expiry handling in the two most complete
playback paths returns nothing: `ref:sone/src-tauri/src/commands/playback.rs` has no expiry/TTL
logic at all, and `ref:high-tide/src/lib/player_object.py` has no `expire` handling either. The
closest mitigation anywhere is Music Assistant's ephemeral local DASH route, kept alive for
`track.duration + 300s` (`project-profiles.md` §5a) — which bounds the manifest *route's* own
lifetime, not the CDN URLs inside it.

**Design decisions streamboat must make explicitly, none of which this reference set answers**:
(a) does the armed next-track slot get re-resolved after a pause exceeding some threshold, or
unconditionally on resume; (b) is a mid-track 403/410 from the CDN transient (halt) or
"re-resolve and seek back to position" — note `sone-deep-dive.md` §3c's own `isUnplayableError`
classifier would treat a 403 as transient (halt) and a 410 as terminal (skip), and *neither* is
correct for an expired-but-otherwise-valid URL; (c) on the bit-perfect path, position is derived
from frames written to the PCM device (§1 above, seeking), so a re-resolve-and-seek recovery must
re-base `frames_written` exactly as the seek path already does. Prototype this explicitly; it is
new engineering, not a "copy from X" item.

## 10. macOS: audio output is one of five from-scratch subsystems, not a standalone gap (added by
the third fact-check pass)

§6 above already establishes there is no macOS bit-perfect/exclusive-output precedent in this
reference set — but that is only the audio slice of a larger picture. `sone-windows`
(`sone-deep-dive.md` §9) is the case in point: its `souvlaki` SMTC dependency is declared only
under `[target.'cfg(target_os = "windows")'.dependencies]`, and every audio sink/enumeration
branch is `#[cfg(target_os = "linux")]`/`#[cfg(target_os = "windows")]` only — **there is no macOS
arm anywhere in the fork**, so the entire reference set contains zero Tauri/Rust macOS TIDAL
precedent, for output or otherwise. `packaging-distribution.md` §8 assembles the full five-item
macOS work list (output, OS media integration, code signing/notarization, GStreamer bundling,
Keychain storage) — read it before scoping macOS as "Tauri handles that platform for us."
