# Decoding and codec stacks

Full narrative: `docs/research/audio-pipeline.md` §2, plus the FLAC-in-DASH version-floor and
Symphonia-FLAC-in-fMP4 gap-fill in §1.4 and §9 (Design C).

## Table of contents

1. What must be decoded, and the GStreamer/Debian version floor for it
2. Decoder stacks compared, and their licences
3. AAC patent status in 2026
4. FFmpeg's E-AC-3/JOC decoder — what it does and doesn't do

---

## 1. What must be decoded, and the GStreamer/Debian version floor for it

| Tier / mode | Codec | Container | Notes |
|---|---|---|---|
| LOW | HE-AAC v1 (SBR) | fMP4 or raw via BTS | Needs SBR, not just AAC-LC |
| HIGH | AAC-LC | fMP4 / M4A | |
| LOSSLESS | FLAC 16/44.1 (ALAC possible, unobserved — see `tidal-manifest-api.md` §1) | fMP4 (`fLaC`+`dfLa`) via DASH, or `.flac` via BTS/urlpostpaywall | |
| HI_RES_LOSSLESS | FLAC up to 24/192 | same | |
| Atmos | E-AC-3 JOC | fMP4 | Decoder is licensed — see `atmos-and-immersive.md` |
| legacy | MP3, ALAC, AC-4 | — | Present in codec enums, not observed served in current tiers |

**FLAC-in-fMP4 container support:** FFmpeg maps `MKTAG('f','L','a','C')` → `AV_CODEC_ID_FLAC`
(`libavformat/isom_tags.c`) and parses the `dfLa` FlacSpecificBox (`mov_read_dfla` in `mov.c`).
GStreamer's `qtdemux` has `FOURCC_fLaC`/`FOURCC_dfLa` and emits `audio/x-flac` caps.

**The DASH-side version floor is a real, current packaging constraint — not a solved problem.**
GStreamer 1.26.10 (~24-26 December 2025) added "support for FLAC audio in DASH manifests" (plus
FLAC 6.1/7.1 layouts, 32-bit FLAC encode/decode). Before that, TIDAL FLAC-in-DASH worked only
through the **legacy** `dashdemux` (gst-plugins-bad), not through `adaptivedemux2`/`dashdemux2`.
Sone's own reason for staying on legacy `uridecodebin` (not `uridecodebin3`) is primarily that its
`concat`-based gapless makes `about-to-finish` unnecessary — getting the legacy demuxer (and
therefore pre-1.26.10 compatibility) is a secondary, supporting property of that choice, not the
stated motive (`ref:sone/src-tauri/src/audio.rs:1790-1795,3302-3305`).

**Debian 13 "trixie" (the base for current Raspberry Pi OS, and the brief's own headless/Pi target)
ships `gstreamer1.0-plugins-bad` 1.26.2-3 and `gstreamer1.0` 1.26.2-2** —
eight point releases below the 1.26.10 floor. **A GStreamer-based streamboat must use the legacy
`dashdemux`/`uridecodebin` path (never `uridecodebin3`/`playbin3` for DASH) until its minimum
supported GStreamer is >= 1.26.10**, which will not be true on Debian stable for the foreseeable
future. This is also a caveat on High Tide's `playbin3` gapless design, which depends on the newer
path being available (see `playback-behavior.md` §1).

## 2. Decoder stacks compared, and their licences

| Stack | Language | FLAC | AAC-LC | HE-AAC | E-AC-3 | DASH | HLS | Licence | Notes |
|---|---|---|---|---|---|---|---|---|---|
| **GStreamer** (1.26+/1.28) | C, bindings everywhere | yes | yes | yes | via `gst-libav` | yes (`dashdemux` legacy, `dashdemux2` >=1.26.10) | yes | Core LGPL-2.1+; some plugin sets pull in GPL/patent code | Everything TIDAL needs works out of the box on Linux; Windows/macOS need shipping the plugin set |
| **FFmpeg / libav\*** | C | yes | yes | yes | yes | yes | yes | LGPL-2.1+ by default; GPL if built `--enable-gpl` | tidalt links `libavformat/libavcodec/libswresample` directly, streams via an AVIO callback with no temp files |
| **libmpv** | C | yes (via FFmpeg) | yes | yes | yes | yes | yes | **mpv is GPLv2+ by default.** An LGPLv2.1+ build exists via the Meson switch **`-Dgpl=false`** — *not* `--enable-lgpl` (that was the old, now-nonexistent waf-build flag) — intended specifically for libmpv. mpv's own `Copyright` file says *"currently it's not recommended to build mpv CLI in LGPL mode at all"* — the intended use is libmpv, which is streamboat's use case. LGPL mode disables Linux X11 video output, OSS audio, vdpau, jack, DVD, CDDA, DVB, legacy direct3d. | Only cross-platform stack that also gives exclusive output on all three desktops (see `output-backends.md`) |
| **Symphonia** 0.6.1 | pure Rust | "excellent" | "great" | **no** ("in work or not started") | no | **no** upstream, but see §1 note below | no | MPL-2.0 | No network/streaming layer. Needs `dash-mpd` + a hand-rolled fetcher and an HE-AAC gap-filler — see the Design C detail below |
| **Shaka Player** | TS, browser | via MSE, browser-dependent | yes | yes | browser-dependent | yes | yes | Apache-2.0 | What TIDAL's own web SDK uses; inherits browser audio-path resampling concerns |
| **ExoPlayer / androidx.media3** 1.5.0 | Kotlin/Java | yes | yes | yes | yes | yes | yes | Apache-2.0 | What TIDAL's Android SDK uses |
| **AVPlayer/AVFoundation** | Swift | yes | yes | yes | yes | no (HLS only) | yes | Apple platform | What TIDAL's iOS SDK uses; drives its HLS-only manifest choice |

**Licensing consequences for streamboat**, assuming a GPL-3.0 project (like High Tide, Sone,
Strawberry):

- GStreamer core (LGPL-2.1+) and FFmpeg (LGPL-2.1+) are compatible with anything.
- `gst-libav` wraps FFmpeg; if that FFmpeg build uses `--enable-gpl`, the result is GPL. Distro
  builds of `gstreamer1.0-libav` are normally LGPL FFmpeg (depends on the distro — not universally
  verified).
- libmpv default build is GPLv2+, compatible with GPL-3.0 because mpv is GPLv2-**or-later**. The
  `-Dgpl=false` LGPL build is the escape hatch if a permissive licence is ever wanted, with the
  caveat above.
- Symphonia is MPL-2.0, file-level copyleft, compatible with everything.

**Symphonia's undocumented FLAC-in-fMP4 support and its traps (Design C detail).** Symphonia's own
README lists ISO/MP4 support as "Great" but does not list FLAC among its codecs there, and marks
ISO/MP4 gapless support "No". The source does parse it anyway:
`symphonia-format-isomp4/src/atoms/mod.rs` declares `AtomType::Flac`/`FlacAtom`, and
`atoms/stsd.rs`'s `read_audio_sample_entry` accepts `AtomType::Flac`, calling
`flac.fill_codec_params(codec_params)`. Three concrete traps for anyone building on this:

1. `stsd.rs` returns `unsupported_error("isomp4: more than 1 sample entry")` for a multi-entry
   `stsd` — verify TIDAL's init segments have exactly one before relying on this.
2. Gapless support is explicitly "No" for ISO/MP4 — gapless is entirely streamboat's code to write
   above the demuxer, not something Symphonia gives for free.
3. TIDAL's DASH init segments carry no `sidx` box, so `format.seek()` (which needs `sidx` for
   non-seekable sources) will not work. Seeking means jumping to the right `$Number$` segment from
   the manifest's `SegmentTimeline` and re-feeding the reader from there — the same approach
   python-tidal and tidal-cli use, not a demuxer-level seek.

`[uncertain]` — none of this is documented by Symphonia's own README; verified only against the
source files above, not exercised against a real TIDAL manifest in this research pass.

**`cpal` has no WASAPI exclusive mode and no CoreAudio hog mode on any platform.** The upstream
request (`RustAudio/cpal#459`, opened 2020-07-27) is closed unimplemented; the source
(`src/host/wasapi/device.rs`) hardcodes `AUDCLNT_SHAREMODE_SHARED` at every `Initialize` call site.
`rodio` and `kira` sit on `cpal` and inherit the limitation. **Any Rust bit-perfect client must go
below `cpal`** — direct `alsa`/`wasapi`/`coreaudio-rs`, or a different engine entirely.

## 3. AAC patent status in 2026

- AAC is covered by the Via Licensing Alliance ("Via LA") AAC pool, an active per-unit programme
  with rates roughly US$0.10–0.98 per unit and volume tiers (Standard Rate Structure: $0.98 for the
  first 500k units, stepping to $0.42 at 5-10M, per 2026 third-party reporting; via-la.com itself is
  blocked from this research environment).
- Individual AAC/HE-AAC patents expire country-by-country (one HE-AAC v2 patent expired 2023); there
  is no single global expiry date.
- **No source found in this research pass says the AAC pool has closed or AAC is royalty-free in
  2026.** Treat AAC as still encumbered.

Practical consequence: shipping an AAC decoder in a binary carries the same legal exposure every
media player has. Mitigations everyone uses: rely on system/distro codecs
(`gstreamer1.0-libav`, the OS decoder) rather than bundling one; or let the user choose
LOSSLESS-only, never requesting `LOW`/`HIGH`. FLAC is patent-free and BSD-licensed — an all-FLAC
streamboat has no codec-licensing question at all (see the "lossless-only?" open decision in
SKILL.md).

## 4. FFmpeg's E-AC-3/JOC decoder — what it does and doesn't do

FFmpeg's `eac3` decoder decodes the E-AC-3 core bed but **discards Dolby Atmos's JOC object
metadata** — the output is a 5.1 downmix, not object-based Atmos. Verified directly:
`libavcodec/ac3dec.c` and `libavcodec/eac3dec.c` contain zero occurrences of "joc", "object", or
"atmos". The only JOC-aware code anywhere in FFmpeg is in the **parser**,
`libavcodec/ac3_parser.c` (~lines 266-282), which reads the additional-bitstream-info byte into
`hdr->eac3_extension_type_a` with the comment that its LSB "can be used to detect Atmos presence" —
i.e. FFmpeg can *detect* that a stream carries Atmos, but has no object renderer, only the core-bed
decoder. A real Atmos renderer needs a licence from Dolby. See `atmos-and-immersive.md` for the full
feasibility discussion and recommendation (do not build it for v1).
