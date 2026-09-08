# Audio engine comparison, for the stack decision

This file answers "which audio engine should the stack use" — the decision that picks the rest of
the stack (§2 of `docs/research/tech-stack.md`). For deep pipeline internals (manifest formats,
resampling theory, buffer sizing, per-platform mechanics beyond what's needed to compare engines),
see the `audio-pipeline` skill instead — this file intentionally stays at "which engine, and why,"
not "how the pipeline works."

## Contents

- §0 The `AudioEngine` trait surface, consolidated
- §1 What "bit-perfect" concretely requires
- §2 Sone's format negotiation table (the naming trap)
- §3 Candidate engines, compared
- §4 The wasapi2sink controversy
- §5 A middle option: FFmpeg/libav direct
- §6 Gapless conflicts with bit-perfect, in every engine
- §7 tidalt's format-preference order (and its own doc's contradiction)
- §8 macOS: no verified bit-perfect path exists anywhere in the survey — and the concrete mechanism
- §9 Device warm-up / PLL-lock delay belongs in the `AudioEngine` trait
- §10 libmpv's Rust binding is not yet chosen
- §11 ALSA buffer/period/XRUN numbers, and the format-change reopen owner decision
- §12 Two structurally different backend shapes: "raw device writer" vs. "delegated sink"
- §13 High Tide's sink map and PipeWire-forces-no-gapless precedent

---

## §0 The `AudioEngine` trait surface, consolidated

This trait is the single most load-bearing API decision in the project, and its surface previously
appeared piecemeal and inconsistently across this skill — the report's original sketch
(`docs/research/tech-stack.md` §2.3: `load(uri, hints)`, `play`, `pause`, `seek`, `set_volume`,
`position`, event stream), this file's own §9/§12 mentions (`load/play/pause/seek/set_volume/position/
events` — dropping `hints`), and a third short form previously duplicated in `architecture-shapes.md`
§1 (now removed there in favour of pointing here). Read this block as authoritative; every other
mention of the trait in this skill is a partial restatement of it, not a competing definition.

```rust
trait AudioEngine {
    // Core transport — docs/research/tech-stack.md §2.3. `hints` carries what the manifest/quality
    // cascade already knows (sample rate, bit depth, codec) so the backend can probe/negotiate before
    // the first sample, not after a format-mismatch underrun.
    fn load(&mut self, uri: &str, hints: FormatHints) -> Result<()>;
    fn play(&mut self) -> Result<()>;
    fn pause(&mut self) -> Result<()>;
    fn seek(&mut self, position: Duration) -> Result<()>;
    // No-op or rejected in bit-perfect mode — Sone locks the volume slider at 100% (§1).
    fn set_volume(&mut self, volume: f32) -> Result<()>;
    // (position, wall_clock, rate) triple — clients interpolate, never a per-frame IPC/WS tick.
    // architecture-shapes.md §3/§4b; tauri-engineering-facts.md §10.
    fn position(&self) -> (Duration, Instant, f64);
    fn events(&self) -> EventStream;

    // Per-device warm-up/settle parameter — §9. Missing from the report's original sketch; getting it
    // wrong reads to users as "the first second of every track is cut off."
    fn set_warmup_settle(&mut self, device: &DeviceId, duration: Duration);

    // Per-backend capability query, NOT one global "bit-perfect mode" flag — §6. `DirectAlsa`-shaped
    // backends return false; GStreamer-sink/libmpv-shaped backends return true-but-unverified.
    fn supports_gapless_with_exclusive(&self) -> bool;

    // Output target, not just a hardware device — architecture-shapes.md §1. Admits a pipe/fd sink
    // for Snapcast/multiroom sending alongside ALSA hw:/WASAPI exclusive/CoreAudio hog mode.
    fn set_output_target(&mut self, target: OutputTarget) -> Result<()>;
}

enum OutputTarget {
    Device(DeviceId),  // hw:X,Y / WASAPI device id / CoreAudio device id
    Pipe(RawFd),        // PCM to a file descriptor — architecture-shapes.md §1
}

enum Event {
    // ...
    FormatChanged { from: PcmFormat, to: PcmFormat },  // §11 — every sample-rate/bit-depth boundary
    ReopenFailed { reason: String },                   // §11
    BitDepthChanged { from: u8, to: u8 },               // §11 — Sone's `audio-bit-depth-changed`
    PlaybackRevoked { by_device_name: String },         // architecture-shapes.md §4a — Pushkin
}
```

An implementer who writes this trait from any single earlier mention in this skill (the report's
summary, this file's §9, or `architecture-shapes.md` §1 alone) will miss at least one of: `hints` on
`load()`, the warm-up parameter, `FormatChanged`/`ReopenFailed`/`BitDepthChanged`,
`supports_gapless_with_exclusive()`, or the pipe output target. This block is where all four live
together.

## §1 What "bit-perfect" concretely requires

From Sone's implementation/README and tidalt's, all of:

1. Decode to PCM at the source's native sample rate and bit depth — no resampling.
2. Open the hardware device in a mode that bypasses the OS mixer (ALSA `hw:` / WASAPI exclusive /
   CoreAudio hog mode + physical format change).
3. Negotiate a PCM format the DAC actually supports, rather than letting a library convert.
4. Apply no volume scaling in software (Sone locks the volume slider at 100% in bit-perfect mode).
5. Apply no ReplayGain in bit-perfect mode (it is a multiply, therefore not bit-perfect).

## §2 Sone's format negotiation table (the naming trap)

`ref:sone/src-tauri/src/audio.rs:190-211, 483-609`. **The ALSA↔GStreamer 24-bit naming inversion
table and unit-test recommendation are owned by `audio-pipeline/references/output-backends.md`
§1** — cite it rather than restating; ALSA `S24LE` is GStreamer `S24_32LE`, and ALSA `S243LE` is
GStreamer `S24LE`, backwards from what the names suggest. Getting this backwards produces silence
or noise, not a crash — the single most repeated pitfall across every skill that touches this
pipeline. Sone probes the device with `snd_pcm_hw_params` for each candidate format and rate before
choosing
(`probe_supported_gst_formats`/`probe_supported_rates`, `:483,545`), then sets `start_threshold` and
`avail_min` on the software params because "`snd_pcm_hw_params()` resets start_threshold to 1"
(`:692`).

## §3 Candidate engines, compared

| Engine | Bit-perfect Linux | Bit-perfect Windows | Bit-perfect macOS | DASH/HLS | Gapless | Deployment cost off-Linux |
| --- | --- | --- | --- | --- | --- | --- |
| **GStreamer** (`gstreamer` 0.25.3, MIT/Apache, MSRV 1.92, edition 2024) | Yes — `alsasink device=hw:X,Y`, or appsink + own ALSA writer (Sone) | Yes in **one fork** — `wasapi2sink exclusive=true` (`ref:sone-windows/src-tauri/src/audio.rs:1236-1244`) — **but see §4, the controversy** | **Unproven.** `osxaudiosink` has no documented hog-mode/physical-format control | Yes — `dashdemux`, `hlsdemux`, `uridecodebin` | Yes — `concat` (Sone) or `playbin3` `about-to-finish` (High Tide). **Correction (fact-checked): Sone's `concat` design does NOT need GStreamer ≥1.24** — that floor applies only to the `playbin3`/`about-to-finish` design (High Tide's). Sone's own `gapless_supported()` is `gst::ElementFactory::find("concat").is_some()`, and its code comments say the legacy `uridecodebin → queue → concat` path "works on GStreamer < 1.24" and has "no GStreamer 1.24 or uridecodebin3 requirement" (`ref:sone/src-tauri/src/audio.rs:1795,3303-3306`) — this directly contradicts Sone's own README, which claims a 1.24 floor for gapless generally. Cite the code, not the README. | High. Must ship the runtime: MSI packed and run via `msiexec`, or Merge Modules, on Windows; a PackageMaker-style bundle on macOS (macOS page unverified — domain blocked, and upstream's own page opens "FIXME: PackageMaker is dead we need a new solution" and pins GStreamer 1.8.1 — treat as upstream-acknowledged stale) |
| **libmpv** | Yes — `--audio-device=alsa/hw:1,0 --audio-exclusive=yes`, `--alsa-resample` off by default | Yes — `--ao=wasapi --audio-exclusive=yes` (`--wasapi-exclusive-buffer` tunes it) | Yes, in mpv's own docs — `--ao=coreaudio_exclusive --coreaudio-change-physical-format=yes` ("direct device access and exclusive mode (bypasses the sound server)") — **one field report describes it misrouting between DAC and HDMI** | Yes (via FFmpeg) | **Correction, previously stated wrong here: use `--gapless-audio=weak`, never `no` or `yes`.** mpv's own `DOCS/man/options.rst` defines `weak` as keep-the-device-open-but-reopen-on-any-format-change (bit-perfect safe) and `yes` as lock-to-the-first-file's-format (not bit-perfect safe on a format change); `no` closes the device between every track, forfeiting gapless for no bit-perfection `weak` doesn't already give. Canonical treatment: `audio-pipeline/references/stacks-comparison.md` §3. | Medium. One shared library; Supersonic bundles it in the AppImage, needs `libmpv1`/`libmpv2` on Linux |
| **cpal 0.18.2 + symphonia 0.6.1** | Partial — `device_by_id()` accepts ALSA shorthand (`hw:0,0`, `plughw:foo`) since **0.18.0** (0.17.0 added `device_by_id()` itself, generic IDs only) | **No.** No exclusive-mode API; 0.17.2 (**yanked**) added "as-necessary resampling in the WASAPI server process"; **0.18.2** (not 0.18.0) "output streams no longer reject formats the built-in resampler can convert" | No | **No** — symphonia decodes containers (OGG/WAV/MKV/MP4/CAF/AIFF) and codecs (MP3, FLAC, Vorbis, AAC, ALAC, ADPCM, PCM) but does not fetch or demux DASH/HLS, and does not decode EAC3/AC4 | Manual | Zero — pure Rust, static |
| **Per-OS hand-rolled** (`alsa` 0.10 + `wasapi` 0.24.0 + `coreaudio-rs`) + symphonia for decode | Yes | Yes | Yes (in principle) | No — write the MPD/HLS fetcher yourself | Manual | Zero, but highest code volume |
| **FFmpeg/libav direct** (`ffmpeg-next`/`rsmpeg`) + per-OS sink | Yes | Yes (in principle — wire exclusive mode yourself) | Yes (in principle) | Yes — `libavformat` demuxes DASH/HLS | Manual | Medium — one shared/static FFmpeg build (own licensing/build-config work), no plugin registry to ship. See §5. |

Notes:

- **The DASH manifest arrives base64-encoded and is handed to the pipeline as a data URI, not fetched
  by GStreamer itself.** Sone base64-encodes the MPD manifest string and formats
  `data:application/dash+xml;base64,{b64}` before calling `load()`
  (`ref:sone/src-tauri/src/commands/playback.rs:118-127`) — GStreamer's `dashdemux` then reads it like
  any other URI. BTS streams instead pass a plain direct URL (`stream_info.url.clone()`, same lines).
  Full manifest MIME-type/delivery-strategy detail (all four strategies, not just this one, and the
  full DASH structure) is owned by `audio-pipeline/references/tidal-manifest-api.md` §3-4. This one
  is one of the four proven TIDAL code paths that makes GStreamer the §2.3 pick in
  `docs/research/tech-stack.md`, and it is the concrete shape any alternative fetch layer (a hand-rolled
  `symphonia` front end, say) must replicate: the API returns the manifest inline in the
  `playbackinfopostpaywall` response, not as a URL to fetch separately.
- **symphonia is MPL-2.0**, not MIT/Apache — fine for a GPL project, note if the licence ever changes.
- **rodio 0.22.2** (2026-03-05, MIT/Apache) sits on cpal and inherits its ceiling. Its own description
  is a general-purpose audio playback/recording library, not specifically a "game-audio library" —
  either way, not a bit-perfect player.
- **EAC3-JOC/Dolby Atmos**: python-tidal lists MP3, AAC/MP4A, FLAC, EAC3, AC4 as TIDAL codecs.
  symphonia does not decode EAC3/AC4; GStreamer and FFmpeg/mpv do (`gst-libav`/
  `gstreamer1.0-libav`, which Sone's package deps include). If Atmos is ever in scope, pure-Rust
  decode is out.
- **Exclusive WASAPI in pure Rust needs the separate `wasapi` crate** (0.24.0, 2026-08-12, MIT,
  shared+exclusive+loopback capture) — `cpal` alone cannot do it; issue #106 (2016, still open) and
  #459 (2020, closed without an implementation) both asked for it.

## §4 The wasapi2sink controversy

The two reference projects that ship a GStreamer Windows sink **disagree**:

- Sone/`sone-windows` use `wasapi2sink exclusive=true` and it compiles and ships
  (`ref:sone-windows/src-tauri/src/audio.rs:1236-1244`).
- **Strawberry's own startup code demotes both `wasapisink` AND `wasapi2sink`** (correction,
  fact-checked: an earlier draft said `wasapi2sink` alone) **to `GST_RANK_SECONDARY` and ranks
  `directsoundsink` `GST_RANK_PRIMARY` on Windows**, with the in-source comment "wasapisink does not
  support device switching and wasapi2sink has issues, see #1227"
  (`ref:strawberry/src/engine/gststartup.cpp:58-75`). Read this as Strawberry distrusting the *whole*
  WASAPI sink family as its shared-mode default — it still routes its own exclusive-mode code path
  exclusively through those same sinks (see §9's `g_object_class_find_property` idiom), so
  `directsoundsink` is Strawberry's default, not its bit-perfect path.

Treat `wasapi2sink` exclusive mode as "compiles and works in one AI-assisted fork of an older Sone
release," not as a community-settled Windows answer. Budget a hardware-verification pass (confirm the
DAC actually receives exclusive-mode, unresampled PCM — not just that the pipeline builds) before
shipping this as a claim, and keep `directsoundsink` as a documented fallback rank in the
`AudioEngine` trait's GStreamer backend.

**Resolved (fact-checked — was previously "unresolved" in this file): the minimum GStreamer version
that exposes `wasapi2sink`'s `exclusive` property is 1.28.** `gstreamer.freedesktop.org` docs remain
blocked in this environment, but the plugin's own source is not:
`subprojects/gst-plugins-bad/sys/wasapi2/gstwasapi2sink.cpp` declares the property with the gtk-doc
annotation `/** GstWasapi2Sink:exclusive: * * Since: 1.28 */` immediately above
`g_object_class_install_property(gobject_class, PROP_EXCLUSIVE, g_param_spec_boolean("exclusive", …))`
(lines 195-204; `DEFAULT_EXCLUSIVE` is `FALSE`, line 64) —
https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-bad/sys/wasapi2/gstwasapi2sink.cpp.
**Treat this as a hard floor for the Windows installer's bundled GStreamer runtime**: pin the MSI/
Merge-Module version to ≥1.28, and remember that because the property lives in one boolean on one
`gst-plugins-bad` element, a runtime below that floor is a **silent** failure mode — the pipeline
builds and plays in shared mode with no error, which is the worst possible failure for a "bit-perfect"
claim. Copy Strawberry's defensive idiom regardless (see §9): gate on
`g_object_class_find_property(sink_class, "exclusive")` before setting it, and add a loud runtime
log/assert both when the property is absent *and* when the detected runtime version is below 1.28.

## §5 A middle option: FFmpeg/libav direct

Between "ship all of GStreamer" and "pure Rust, no DASH/HLS, no Atmos" sits FFmpeg/libav directly —
what tidalt already does via cgo (`libavformat`/`libavcodec`/`libswresample`, 206 lines of C across
`avcodec.c`/`.h`, alongside a 111-line `alsa.c` writer) and what libmpv is internally. Rust
equivalents: `ffmpeg-next`, `rsmpeg`. Cost: FFmpeg's own licensing/build-config surface and writing
your own DASH/HLS fetch layer. Win: one dependency instead of a plugin registry, far smaller to ship
on Windows/macOS than GStreamer.

## §6 Gapless conflicts with bit-perfect — per backend, NOT a universal engine law

**Correction (fact-checked): an earlier draft of this file stated "gapless conflicts with bit-perfect
in every engine" as a universal law. It is not — it is a property of Sone's specific Linux
`DirectAlsa` backend, and this report's own Windows evidence contradicts reading it as universal.**

On Sone's Linux `DirectAlsa` backend, gapless requires holding the device open across tracks, which
forces a single format for the whole run, so Sone makes gapless Normal-mode-only and panics if the
code path is reached in DirectAlsa: `"PlaybackBackend::concat() called on DirectAlsa — gapless is
normal-only"` (`ref:sone/src-tauri/src/audio.rs:150-158`) — the design Sone arrived at via a `concat`
element plus attach/detach of a second `uridecodebin → branch_queue` branch (`:44-86, 108-157,
247-303`).

But on Windows, `sone-windows`' `wasapi2sink exclusive=true` code
(`ref:sone-windows/src-tauri/src/audio.rs:1234-1247`) sits **inside** the file's own
`// ── Normal path (unchanged) ──` branch (`:1188`) — i.e. the same `concat`-based gapless GStreamer
pipeline Sone uses on Linux. Exclusive mode and gapless **coexist in one pipeline on Windows**; they
are two mutually exclusive backends on Linux. Whether the sink correctly re-enters exclusive mode
after `concat` renegotiates format at a track boundary is **unverified** — nothing in either checkout
tests it.

**Model this per backend in the `AudioEngine` trait, not as one global flag**: give it a
`supports_gapless_with_exclusive() -> bool` capability query. `DirectAlsa`-shaped ("raw device
writer," see §12) backends refuse/panic as Sone does; GStreamer-sink or libmpv-shaped ("delegated
sink") backends carry the interaction as unverified until measured on hardware, and the UI should say
so rather than assuming Sone's Linux rule travels to every platform.

**Correction carried over: this design is NOT gated behind GStreamer ≥1.24** — see the note in §3's
table; that floor applies only to the alternative `playbin3`/`about-to-finish` design (High Tide's),
not to the `concat` design (Sone's), which Sone's own code says works on GStreamer < 1.24.

## §7 tidalt's format-preference order (and its own doc's contradiction)

tidalt does the same negotiation in C via cgo, with a preference order that differs by source
depth. Full recipe (both format-preference orders, the D-Bus PipeWire reservation with
release-on-pause, and format-refusal-vs-EBUSY handling) is owned by
`audio-pipeline/references/output-backends.md` §1-2 — cite it rather than restating.

**The lesson unique to this file, worth keeping**: `ref:tidalt/docs/architecture.md:43` states a
different, stale 16-bit format order (`S16_LE → S24_3LE → S24_LE → S32_LE`) than
`ref:tidalt/internal/player/alsa.c:28-39` actually implements (`S32_LE > S16_LE > S24_3LE >
S24_LE`) — a real doc-vs-code contradiction in the reference project itself, and
`ref:tidalt/README.md` doesn't document this table at all (it only covers the `plughw:` fallback
semantics). **Lesson for streamboat: keep a format-preference table in exactly one place,
physically next to the code, so it cannot drift the way tidalt's own docs did.**

## §8 macOS: no verified bit-perfect path exists anywhere in the survey — and the concrete mechanism

GStreamer's `osxaudiosink` has no documented hog-mode/physical-format control. mpv's
`coreaudio_exclusive` + `--coreaudio-change-physical-format=yes` is the only *documented* path
("direct device access and exclusive mode (bypasses the sound server)"), but it is **not verified on
hardware** by any source found — a Feishin discussion (`jeffvli/feishin#88`) reports it misrouting
between DAC and HDMI, and the original poster could not verify Apple's behaviour for lack of a
sample-rate indicator on the DAC. No reference project in this entire survey (Sone, sone-windows,
tidalt, Strawberry, Supersonic, Feishin) demonstrates macOS bit-perfect output. **This is confirmed
directly against Strawberry's source, the survey's only genuinely tri-platform GStreamer client**:
`GstEngine::ExclusiveModeSupport()` returns `true` only for `wasapisink`/`wasapi2sink`
(`ref:strawberry/src/engine/gstengine.cpp:523-525`) — both Windows-only GStreamer elements, so this
function proves **Windows only**, not Linux. **Correction, previously overstated as "Linux and
Windows" here**: Strawberry's Linux `exclusive_mode_` flag comes from a different, weaker
mechanism — `ref:strawberry/src/engine/gstenginepipeline.cpp:632-637` sets it purely because the
configured ALSA device string starts with `hw:` **or `plughw:`**, and `plughw:` is by definition a
converting plug layer, not a bit-perfect path. `rg -i 'bit.perfect' strawberry/src/` returns
nothing — there is no dedicated bit-perfect code path on Linux at all, just this device-prefix
inference (`tidal-oss-landscape/references/comparison-tables.md` Table A and
`references/verification-notes.md` item 7 document this independently). Its `osxaudiosink` use and
all its CoreAudio `AudioHardware.h` calls are device *enumeration* only
(`ref:strawberry/src/engine/macosdevicefinder.cpp:27,73`) — there is no hog-mode or
physical-format-change code anywhere in `src/`. So Strawberry proves bit-perfect on **Windows
only** (via `wasapisink`/`wasapi2sink`); Linux "exclusive" is an unverified device-prefix
inference, and macOS is unproven — see `references/scoring-and-candidates.md` §4.11.

**The concrete mechanism, named for the first time in this pass (gap identified during
fact-checking):** macOS bit-perfect is **device hog mode** — `AudioObjectSetPropertyData` on
`kAudioDevicePropertyHogMode` (an exclusive process-ID claim on the device) plus setting
`kAudioStreamPropertyPhysicalFormat` on the output stream to match the source's rate/depth. That is
exactly what mpv's `--coreaudio-change-physical-format=yes` does under `--ao=coreaudio_exclusive`. In
Rust that is the `coreaudio-rs`/`coreaudio-sys` bindings, or a direct `objc2` FFI shim — **no crate in
this survey wraps it for bit-perfect use**, and no reference project implements it. Treat macOS
bit-perfect as a research spike with its own line item — "implement hog mode +
`kAudioStreamPropertyPhysicalFormat` via `coreaudio-rs`, verify on real hardware" — and its own
hardware-verification step (a Mac and a DAC), not a checkbox that falls out of the Linux/Windows work
— see the recommendation in `docs/research/tech-stack.md` §2.3.

## §9 Device warm-up / PLL-lock delay belongs in the `AudioEngine` trait

Two independent reference projects treat DAC warm-up as required, and this report's original
`AudioEngine` trait sketch (`load/play/pause/seek/set_volume/position/events`) has nowhere to express
it — a gap identified during fact-checking, not present in an earlier draft:

- **Strawberry** has a settings-backed `device_warmup_duration_ms` with a warmup-pending flag and a
  generation counter (`ref:strawberry/src/engine/gstenginepipeline.cpp:142-144`). It also sets its
  sink's `exclusive` property defensively — `if (g_object_class_find_property(sink_class,
  "exclusive"))` (`ref:strawberry/src/engine/gstenginepipeline.cpp:722-726`) — rather than assuming the
  property exists; ALSA `hw:`/`plughw:` device strings set `exclusive_mode_` implicitly
  (`:632-635`). Both idioms are worth copying regardless of engine choice.
- **tidalt** documents a specific DAC's PLL-lock delay as a debugging topic — the Hidizs S9 Pro Plus
  (`ref:tidalt/docs/dac-compatibility.md`, `ref:tidalt/docs/debugging.md:84`).

Getting this wrong reads to users as "the first second of every track is cut off," not as a config
bug — retrofitting it later is expensive because it touches the trait surface every backend
implements. Add a per-device warm-up/settle parameter to the `AudioEngine` trait and the device
settings model from day one.

## §10 libmpv's Rust binding is not yet chosen

`docs/research/tech-stack.md` recommends libmpv as backend #2 specifically for macOS (§8 above) and
for shrinking the Windows/macOS packaging burden, but **no crate was named, versioned, or
licence-checked** in the research pass — a gap identified during fact-checking. Candidates: `libmpv2`/
`libmpv-sys` on desktop, `media_kit` on the Flutter path (Recommendation 3 in
`references/scoring-and-candidates.md`). Each needs its own version/licence/maintenance check before
adoption. Note also: mpv's build configuration (LGPL vs GPL, depending on which optional components
are compiled in) determines whether a bundled libmpv forces GPL on the combined work — this interacts
with Open decision #1 (project licence) in `SKILL.md`.

## §11 ALSA buffer/period/XRUN numbers, and the format-change reopen owner decision

**Gap filled during fact-checking**: the report named `start_threshold`/`avail_min` without values.
The two reference implementations disagree by ~5x on buffer depth — a real decision, not a default to
copy blindly.

**Buffer/period sizing:**

| | Buffer | Period | Ordering | Source |
| --- | --- | --- | --- | --- |
| Sone | 500 ms (`set_buffer_time_near(500_000, Nearest)`) | 50 ms (`set_period_time_near(50_000, Nearest)`) | buffer first | `ref:sone/src-tauri/src/audio.rs:685-712` |
| tidalt | ~93 ms (`period_size * 4`) | ~23 ms (`period_size = 1024` frames @ 44.1 kHz) | **period first, deliberately** — in-source rationale: "Set period size first so the DAC gets a sane interrupt rate... Setting buffer first and then querying period_size_min can return absurdly small" values | `ref:tidalt/internal/player/alsa.c:67-95` |

Sone's `sw_params`: because "`snd_pcm_hw_params()` resets start_threshold to 1 (immediate start on
first writei), which causes underruns when the writer can't keep up from frame one," it sets
`start_threshold` to the largest period-aligned value ≤ buffer_size ("Match GStreamer alsasink:
start_threshold = buffer_size (full pre-fill)") and `avail_min = period_size`. tidalt leaves
`sw_params` at ALSA defaults.

**XRUN recovery**: Sone's `alsa_recover(pcm, errno)` helper logs `"[alsa-writer] XRUN, recovering"`
and calls `pcm.prepare()` on `EPIPE`; on suspend it loops `resume()` then falls back to `prepare()`
(`ref:sone/src-tauri/src/audio.rs:861-881`).

**Writer thread scheduling**: the writer is a plain named `std::thread::Builder::new().name(
"alsa-writer")` thread (`:832-834`). **Neither reference project requests real-time scheduling** — no
`SCHED_FIFO`, no `rtkit`, no `PTHREAD_PRIO` anywhere in either checkout. Treat RT priority as
unproven-but-unused precedent, not a requirement — relevant to how strongly the JVM/CLR-runtime-jitter
argument against KMP/Avalonia (see `references/scoring-and-candidates.md` §4.9/§4.12) should actually
weigh.

**Format-change reopen is unavoidable in bit-perfect mode, not a bug.** Sone's `reopen_alsa()` doc
comment: "Close and reopen ALSA device with new format. ... HW params [cannot be changed] in-place
after `snd_pcm_drop()` — need full close+reopen" (`ref:sone/src-tauri/src/audio.rs:990-993`), called from
four places in the writer loop, emitting an `audio-error{"kind":"format_change_failed"}` event on
failure and `audio-bit-depth-changed {from, to}` plus a signal-path `record_bit_depth_promotion` entry
when the negotiated depth differs from the source (`:990-1002,1071,1096,1118,1143,1210,1261`). A TIDAL
queue routinely mixes 44.1/16, 44.1/24 and 96/24 tracks — every format boundary in strict bit-perfect
mode reopens the device, which (a) makes gapless across a format boundary impossible in principle even
before any backend-specific limit applies, (b) re-triggers the §9 warm-up/PLL-lock delay on every such
boundary, not just at stream start, and (c) needs `FormatChanged`/`ReopenFailed` events on the
`AudioEngine` trait alongside the warm-up parameter.

**This is an owner decision, not only an engineering detail** — put it beside the volume-lock and
no-ReplayGain calls in §1, which are the same class of decision:

1. **Accept the audible gap** at every format boundary in strict bit-perfect mode — Sone's answer:
   reopen, pay the gap.
2. **Fix the device at one rate**, let a plug layer convert non-matching tracks — tidalt's documented
   `plughw:` fallback, which sets `bitPerfect=false` and shows a `(converted)` badge rather than
   pretending the whole queue is bit-perfect (`ref:tidalt/internal/player/alsa.c:28-39`, `README.md`
   plughw semantics).
3. **Group or reorder the queue by format** — no reference project does this.

## §12 Two structurally different backend shapes: "raw device writer" vs. "delegated sink"

**Gap identified during fact-checking**: the report's implicit framing throughout (Summary, §2.2,
§4.1) is that the per-OS output layer is one swappable sink behind the `AudioEngine` trait, with
`sone-windows` as evidence "the approach compiles and runs." The two *proven* implementations are not
two instances of one shape:

- **"Raw device writer"** (Linux bit-perfect in Sone, and any future CoreAudio-hog-mode backend): an
  `appsink` plus a hand-rolled writer thread doing its own format probing (§2), its own
  `hw_params`/`sw_params` negotiation (§11), its own XRUN recovery (§11), and its own device
  close+reopen on format change (§11). All of the negotiation logic lives in streamboat's own code.
- **"Delegated sink"** (Windows exclusive mode in `sone-windows`, and libmpv on any platform): three
  `set_property` calls on a stock element (`wasapi2sink`, `ref:sone-windows/src-tauri/src/
  audio.rs:1236-1247`) with all negotiation, buffering, and recovery delegated to the sink/library.
  streamboat has **no visibility** into what the sink does internally.

Format-probing, warm-up, and XRUN recovery exist only in the "raw device writer" kind in this survey.
Budget the Windows/macOS "delegated sink" backends as **"unverified, no negotiation visibility,"** not
as "done, one sink swap" — and design the `AudioEngine` trait's test/mock surface (§7 of
`architecture-shapes.md`) around that asymmetry: a fake "raw device writer" backend is easy to build
for CI; a fake "delegated sink" backend proves nothing about the real sink's behaviour.

## §13 High Tide's sink map and PipeWire-forces-no-gapless precedent

High Tide (Python + GTK4 + GStreamer `playbin3`) is the only non-Sone-family gapless/sink-selection
precedent in the survey, and it decides both questions differently from Sone:

**Sink selection is a static name map fed to `Gst.parse_bin_from_description`, not per-sink Rust
code:**

```python
sink_map = {
    AudioSink.AUTO: "autoaudiosink",
    AudioSink.PULSE: "pulsesink",
    AudioSink.ALSA: f"alsasink device={self.alsa_device}",
    AudioSink.JACK: "jackaudiosink",
    AudioSink.OSS: "osssink",
    AudioSink.PIPEWIRE: "pipewiresink",
}
```

(`ref:high-tide/src/lib/player_object.py:186-193`). `change_audio_sink()` hot-swaps at runtime by
setting pipeline state to `NULL`, rebuilding the bin from the description string, and restoring
playback position (`:228-247`) — a coarser but far simpler hot-swap mechanism than Sone's device
close+reopen (§11), worth considering for the non-bit-perfect sink choices even if bit-perfect mode
keeps Sone's own reopen path.

**PipeWire forces gapless off, unconditionally**: `if sink_type == AudioSink.PIPEWIRE:
self.gapless_enabled = False else: self.gapless_enabled = True` (`:211-214`). High Tide's own
`about-to-finish`-based gapless design (§3, §6 — the `playbin3` path that needs GStreamer ≥1.24) is
simply disabled rather than attempted through `pipewiresink`. No comment in the source explains why;
treat it as a precedent that "gapless + this specific sink" can be an explicit unsupported
combination, decided per sink at configuration time, not something the `AudioEngine` trait needs to
paper over — `supports_gapless_with_exclusive()` (§6, §0) is the Rust-side equivalent of this same
kind of per-backend refusal.
