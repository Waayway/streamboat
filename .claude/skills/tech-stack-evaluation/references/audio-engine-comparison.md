# Audio engine comparison, for the stack decision

This file answers "which audio engine should the stack use" — the decision that picks the rest of
the stack (§2 of `docs/research/tech-stack.md`). For deep pipeline internals (manifest formats,
resampling theory, buffer sizing, per-platform mechanics beyond what's needed to compare engines),
see the `audio-pipeline` skill instead — this file intentionally stays at "which engine, and why,"
not "how the pipeline works."

## Contents

- §1 What "bit-perfect" concretely requires
- §2 SONE's format negotiation table (the naming trap)
- §3 Candidate engines, compared
- §4 The wasapi2sink controversy
- §5 A middle option: FFmpeg/libav direct
- §6 Gapless conflicts with bit-perfect, in every engine
- §7 tidalt's format-preference order (and its own doc's contradiction)
- §8 macOS: no verified bit-perfect path exists anywhere in the survey

---

## §1 What "bit-perfect" concretely requires

From SONE's implementation/README and tidalt's, all of:

1. Decode to PCM at the source's native sample rate and bit depth — no resampling.
2. Open the hardware device in a mode that bypasses the OS mixer (ALSA `hw:` / WASAPI exclusive /
   CoreAudio hog mode + physical format change).
3. Negotiate a PCM format the DAC actually supports, rather than letting a library convert.
4. Apply no volume scaling in software (SONE locks the volume slider at 100% in bit-perfect mode).
5. Apply no ReplayGain in bit-perfect mode (it is a multiply, therefore not bit-perfect).

## §2 SONE's format negotiation table (the naming trap)

`ref:sone/src-tauri/src/audio.rs:190-211, 483-609`:

| GStreamer format | ALSA format (Rust `alsa` crate) | bytes/sample |
| --- | --- | --- |
| `S16LE` | `Format::S16LE` | 2 |
| `S24LE` | `Format::S243LE` (24-bit packed, 3 bytes) | 3 |
| `S24_32LE` | `Format::S24LE` (24-in-32 container) | 4 |
| `S32LE` | `Format::S32LE` | 4 |
| `F32LE` | `Format::FloatLE` | 4 |

**The naming trap SONE annotates in-source: ALSA `S24LE` is GStreamer `S24_32LE`, and ALSA `S243LE`
is GStreamer `S24LE`.** Getting this backwards produces silence or noise, not a crash — the single
most repeated pitfall across every skill that touches this pipeline. SONE probes the device with
`snd_pcm_hw_params` for each candidate format and rate before choosing
(`probe_supported_gst_formats`/`probe_supported_rates`, `:483,545`), then sets `start_threshold` and
`avail_min` on the software params because "`snd_pcm_hw_params()` resets start_threshold to 1"
(`:692`).

## §3 Candidate engines, compared

| Engine | Bit-perfect Linux | Bit-perfect Windows | Bit-perfect macOS | DASH/HLS | Gapless | Deployment cost off-Linux |
| --- | --- | --- | --- | --- | --- | --- |
| **GStreamer** (`gstreamer` 0.25.3, MIT/Apache, MSRV 1.92, edition 2024) | Yes — `alsasink device=hw:X,Y`, or appsink + own ALSA writer (SONE) | Yes in **one fork** — `wasapi2sink exclusive=true` (`ref:sone-windows/src-tauri/src/audio.rs:1236-1244`) — **but see §4, the controversy** | **Unproven.** `osxaudiosink` has no documented hog-mode/physical-format control | Yes — `dashdemux`, `hlsdemux`, `uridecodebin` | Yes — `concat` (SONE) or `playbin3` `about-to-finish` (High Tide); needs GStreamer ≥1.24 | High. Must ship the runtime: MSI packed and run via `msiexec`, or Merge Modules, on Windows; a PackageMaker-style bundle on macOS (macOS page unverified — domain blocked) |
| **libmpv** | Yes — `--audio-device=alsa/hw:1,0 --audio-exclusive=yes`, `--alsa-resample` off by default | Yes — `--ao=wasapi --audio-exclusive=yes` (`--wasapi-exclusive-buffer` tunes it) | Yes, in mpv's own docs — `--ao=coreaudio_exclusive --coreaudio-change-physical-format=yes` ("direct device access and exclusive mode (bypasses the sound server)") — **one field report describes it misrouting between DAC and HDMI** | Yes (via FFmpeg) | Yes, but must be **disabled** for strict bit-perfect (`--gapless-audio=no`) | Medium. One shared library; Supersonic bundles it in the AppImage, needs `libmpv1`/`libmpv2` on Linux |
| **cpal 0.18.2 + symphonia 0.6.1** | Partial — `device_by_id()` accepts ALSA shorthand (`hw:0,0`, `plughw:foo`) since **0.18.0** (0.17.0 added `device_by_id()` itself, generic IDs only) | **No.** No exclusive-mode API; 0.17.2 (**yanked**) added "as-necessary resampling in the WASAPI server process"; **0.18.2** (not 0.18.0) "output streams no longer reject formats the built-in resampler can convert" | No | **No** — symphonia decodes containers (OGG/WAV/MKV/MP4/CAF/AIFF) and codecs (MP3, FLAC, Vorbis, AAC, ALAC, ADPCM, PCM) but does not fetch or demux DASH/HLS, and does not decode EAC3/AC4 | Manual | Zero — pure Rust, static |
| **Per-OS hand-rolled** (`alsa` 0.10 + `wasapi` 0.24.0 + `coreaudio-rs`) + symphonia for decode | Yes | Yes | Yes (in principle) | No — write the MPD/HLS fetcher yourself | Manual | Zero, but highest code volume |
| **FFmpeg/libav direct** (`ffmpeg-next`/`rsmpeg`) + per-OS sink | Yes | Yes (in principle — wire exclusive mode yourself) | Yes (in principle) | Yes — `libavformat` demuxes DASH/HLS | Manual | Medium — one shared/static FFmpeg build (own licensing/build-config work), no plugin registry to ship. See §5. |

Notes:

- **symphonia is MPL-2.0**, not MIT/Apache — fine for a GPL project, note if the licence ever changes.
- **rodio 0.22.2** (2026-03-05, MIT/Apache) sits on cpal and inherits its ceiling. Its own description
  is a general-purpose audio playback/recording library, not specifically a "game-audio library" —
  either way, not a bit-perfect player.
- **EAC3-JOC/Dolby Atmos**: python-tidal lists MP3, AAC/MP4A, FLAC, EAC3, AC4 as TIDAL codecs.
  symphonia does not decode EAC3/AC4; GStreamer and FFmpeg/mpv do (`gst-libav`/
  `gstreamer1.0-libav`, which SONE's package deps include). If Atmos is ever in scope, pure-Rust
  decode is out.
- **Exclusive WASAPI in pure Rust needs the separate `wasapi` crate** (0.24.0, 2026-08-12, MIT,
  shared+exclusive+loopback capture) — `cpal` alone cannot do it; issue #106 (2016, still open) and
  #459 (2020, closed without an implementation) both asked for it.

## §4 The wasapi2sink controversy

The two reference projects that ship a GStreamer Windows sink **disagree**:

- SONE/`sone-windows` use `wasapi2sink exclusive=true` and it compiles and ships
  (`ref:sone-windows/src-tauri/src/audio.rs:1236-1244`).
- **Strawberry's own startup code demotes `wasapi2sink` to `GST_RANK_SECONDARY` and ranks
  `directsoundsink` `GST_RANK_PRIMARY` on Windows**, with the in-source comment "wasapisink does not
  support device switching and wasapi2sink has issues, see #1227"
  (`ref:strawberry/src/engine/gststartup.cpp:59-69`).

Treat `wasapi2sink` exclusive mode as "compiles and works in one AI-assisted fork of an older SONE
release," not as a community-settled Windows answer. Budget a hardware-verification pass (confirm the
DAC actually receives exclusive-mode, unresampled PCM — not just that the pipeline builds) before
shipping this as a claim, and keep `directsoundsink` as a documented fallback rank in the
`AudioEngine` trait's GStreamer backend.

## §5 A middle option: FFmpeg/libav direct

Between "ship all of GStreamer" and "pure Rust, no DASH/HLS, no Atmos" sits FFmpeg/libav directly —
what tidalt already does via cgo (`libavformat`/`libavcodec`/`libswresample`, 206 lines of C across
`avcodec.c`/`.h`, alongside a 111-line `alsa.c` writer) and what libmpv is internally. Rust
equivalents: `ffmpeg-next`, `rsmpeg`. Cost: FFmpeg's own licensing/build-config surface and writing
your own DASH/HLS fetch layer. Win: one dependency instead of a plugin registry, far smaller to ship
on Windows/macOS than GStreamer.

## §6 Gapless conflicts with bit-perfect, in every engine

Gapless requires holding the device open across tracks, which forces a single format for the whole
run. SONE resolves this by making gapless Normal-mode-only and panicking if the code path is reached
in DirectAlsa: `"PlaybackBackend::concat() called on DirectAlsa — gapless is normal-only"`
(`ref:sone/src-tauri/src/audio.rs:156`). **Copy that decision explicitly rather than discovering it**
— it is the design SONE arrived at via a `concat` element plus attach/detach of a second
`uridecodebin → branch_queue` branch (`:44-86, 108-157, 247-303`), gated behind GStreamer ≥1.24.

## §7 tidalt's format-preference order (and its own doc's contradiction)

tidalt does the same negotiation in C via cgo, with a preference order that differs by source depth:

- 16-bit: `S32_LE > S16_LE > S24_3LE > S24_LE` (S32_LE first — a specific Hidizs USB DAC problem,
  CS43198-based devices per `ref:tidalt/docs/dac-compatibility.md`)
- 24-bit: `S24_3LE > S24_LE > S32_LE`

Authoritative source: `ref:tidalt/internal/player/alsa.c:28-39` and `ref:tidalt/CLAUDE.md:27-30`.
**`ref:tidalt/docs/architecture.md:43` states a different, stale 16-bit order**
(`S16_LE → S24_3LE → S24_LE → S32_LE`), directly contradicting the code — and `ref:tidalt/README.md`
does not document this table at all (it only covers the `plughw:` fallback semantics). **Lesson for
streamboat: keep a format-preference table in exactly one place, physically next to the code, so it
cannot drift the way tidalt's own docs did.**

tidalt also takes a D-Bus PipeWire reservation (`org.freedesktop.ReserveDevice1.Audio{N}`) before
opening the device and releases it on stop, falling back to `plughw:` only when
`snd_pcm_hw_params` refuses the format — distinguishing a format refusal (retries through the plug
layer, sets `bitPerfect=false`, UI badge shows `(converted)`) from a busy device (keeps retrying
`hw:` on `-EBUSY`, never downgrades).

## §8 macOS: no verified bit-perfect path exists anywhere in the survey

GStreamer's `osxaudiosink` has no documented hog-mode/physical-format control. mpv's
`coreaudio_exclusive` + `--coreaudio-change-physical-format=yes` is the only *documented* path
("direct device access and exclusive mode (bypasses the sound server)"), but it is **not verified on
hardware** by any source found — a Feishin discussion (`jeffvli/feishin#88`) reports it misrouting
between DAC and HDMI, and the original poster could not verify Apple's behaviour for lack of a
sample-rate indicator on the DAC. No reference project in this entire survey (SONE, sone-windows,
tidalt, Strawberry, Supersonic, Feishin) demonstrates macOS bit-perfect output. Treat macOS
bit-perfect as a research spike with its own line item and its own hardware-verification step (a Mac
and a DAC), not a checkbox that falls out of the Linux/Windows work — see the recommendation in
`docs/research/tech-stack.md` §2.3.
