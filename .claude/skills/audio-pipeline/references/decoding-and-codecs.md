# Decoding and codec stacks

Full narrative: `docs/research/audio-pipeline.md` §2, plus the FLAC-in-DASH version-floor and
Symphonia-FLAC-in-fMP4 gap-fill in §1.4 and §9 (Design C).

## Table of contents

1. What must be decoded, and the GStreamer/Debian version floor for it
2. Decoder stacks compared, and their licences
3. AAC patent status in 2026
4. FFmpeg's E-AC-3/JOC decoder — what it does and doesn't do
5. AAC-tier gapless is a different, harder, currently-unsolved problem

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
eight point releases below the 1.26.10 floor. **A GStreamer-based streamboat must route TIDAL's DASH
manifests through the legacy `dashdemux` (never `dashdemux2`) until its minimum supported GStreamer
is >= 1.26.10**, which will not be true on Debian stable for the foreseeable future. This is also a
caveat on High Tide's `playbin3` gapless design, which depends on the newer path being available (see
`playback-behavior.md` §1).

**Correction: choosing `uridecodebin` over `uridecodebin3` does not, by itself, select the legacy
demuxer.** Both `dashdemux` (legacy, `GST_RANK_PRIMARY`) and `dashdemux2`
(gst-plugins-good's `adaptivedemux2`, `GST_RANK_PRIMARY + 1`) register for `application/dash+xml`;
autoplugging picks the higher rank. On any distro that ships `adaptivedemux2` — the normal case —
plain `uridecodebin` therefore autoplugs `dashdemux2`, not `dashdemux`, regardless of source-element
choice. **The actual fix is a startup step**: `gst_plugin_feature_set_rank(dashdemux2_factory,
GST_RANK_NONE)` (or raise `dashdemux`'s rank above it), or a `decodebin::autoplug-select` hook — see
`output-backends.md` §12 for the full detail and the precedent this mirrors (Strawberry's Windows
sink-rank demotion).

**GStreamer version: the two floors below are the fact that matters, not the current point
release.** The 1.28 series (released 27 January 2026) is at **1.28.7 or later as of 2026-09** —
already past the 1.28.6 this skill previously called "the final 1.28 bug-fix release," which shows
how quickly a pinned point-release number goes stale here. Do not trust any specific 1.28.x number
written in this skill without re-checking; check instead against the two engineering floors that
actually gate code: >= 1.26.10 for FLAC-in-DASH (above), >= 1.28 for `wasapi2sink exclusive`
(`output-backends.md` §4) — both are unaffected by which 1.28.x point release is current. 1.28.6
added FFmpeg 9.0 support, matching the `ffmpeg-next` 9.0.0 crate recommendation in
`stacks-comparison.md`; the 1.28.3 `devicemonitor` fix referenced throughout this skill
(`os-integration.md` §3) is likewise unaffected by later point releases.
(https://linuxiac.com/gstreamer-1-28-7-released-with-security-and-playback-fixes/,
https://9to5linux.com/gstreamer-1-28-7-open-source-multimedia-framework-adds-support-for-opencv-5,
both dated 2026-09-08 — see `references/sources.md`.)

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

- AAC is covered by the Via Licensing Alliance ("Via LA") AAC pool, an active per-unit programme,
  tiered by volume: **$0.98 for units 1-500,000, stepping down to $0.78 / $0.68 / $0.45 at higher
  volume tiers**, 900+ licensees (per 2026 third-party reporting; via-la.com's own programme page is
  blocked from this research environment — see `references/sources.md`'s web-sources table).
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

**"Rely on system/distro codecs" is not a solved problem — it's a per-distro, per-platform gap.**
Sone's own install instructions expose the split: Debian/Ubuntu gets `gstreamer1.0-libav` from
`main`; **Fedora needs `gstreamer1-plugin-libav`, which lives in RPM Fusion (free), not Fedora's
default repos** — a stock Fedora install has no AAC decoder unless the user has RPM Fusion enabled;
Arch gets `gst-libav` (`ref:sone/README.md:283,306,345,401`). Sone's Snap build has to stage
`libfaad*` explicitly and recreate `blas`/`lapack` `update-alternatives` symlinks "so `libgstlibav`
(ffmpeg) can load them" — even the confined build needs deliberate, non-obvious work
(`ref:sone/snap/snapcraft.yaml:40,127-129,153`). **On Windows the gap is total**: the one documented
GStreamer-bundling recipe in the reference set (`output-backends.md` §14, Sone-windows) ships no AAC
decoder plugin at all — `LOW`/`HIGH` are unplayable on a shipping Windows build assembled that way
unless an AAC decoder is deliberately added to the bundle. **Add a startup capability probe**
(`gst::ElementFactory::find("avdec_aac")`, or a decodebin dry-run) that reports which tiers are
actually playable on the running installation, feed it into the same transparency panel as the
signal path, and grey out unreachable quality tiers in the UI instead of failing at play time.

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

## 5. AAC-tier gapless is a different, harder, currently-unsolved problem

`playback-behavior.md` §1's gapless designs all implicitly assume FLAC. FLAC has no encoder padding,
so concatenating decoded output is exact. AAC does: every AAC-LC frame set starts with priming
samples (2112 conventionally for AAC-LC, more for HE-AAC/SBR) and ends with trailing padding,
signalled in fMP4 by an `elst` edit-list box and/or an iTunes `gapless` atom. Concatenating two AAC
tracks without honouring that inserts tens of milliseconds of silence plus an audible click at every
boundary — on the `LOW`/`HIGH` tiers, gapless silently does not work even though the code path looks
identical to the FLAC case. **No reference client in the 21-project set handles AAC encoder-delay/
edit-list trimming** — Sone's `concat`, High Tide's `about-to-finish`, Strawberry's `SetNextUrl`, and
TIDAL's own 250 ms Shaka micro-crossfade all either delegate trimming to the demuxer or don't trim at
all. Working assumptions, **none verified end-to-end**: GStreamer's `qtdemux` + `aacparse` are
expected to apply `elst` trimming, so Sone's `concat` design is *probably* correct on AAC too but
must be tested on a real AAC album, not assumed; libmpv/FFmpeg's `mov` demuxer applies edit lists and
`--gapless-audio=weak` keeps the device open, so Design B is *likely* fine; Symphonia's ISO/MP4
gapless support is explicitly "No" (§2), so Design C cannot do AAC gapless at all — one more argument
for a lossless-only Design C variant. **Scope the gapless guarantee to the FLAC tiers explicitly;
treat AAC-tier gapless as best-effort, and verify it per-engine before claiming it in any UI or docs.**
