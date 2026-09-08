---
name: audio-pipeline
description: Deep, code-level knowledge of streamboat's audio playback pipeline — how bytes get from TIDAL's manifest APIs (v1 playbackinfopostpaywall and v2 trackManifests), through a BTS/DASH/HLS manifest and a FLAC-in-fMP4 or AAC/HE-AAC/E-AC-3 decoder, out to a bit-perfect/exclusive-mode DAC on Linux (ALSA hw:/PipeWire), Windows (WASAPI), and macOS (CoreAudio hog mode) or a headless/Pi daemon — plus gapless playback, crossfade, ReplayGain/loudness normalization, buffering and on-disk caching, Dolby Atmos/360RA feasibility, OS media integration (MPRIS/SMTC/NowPlaying), and the candidate engine stacks (GStreamer, libmpv, Symphonia, FFmpeg) with their licensing. Use this whenever writing or reviewing code that touches any of the following — a manifest fetch or parser (playbackinfopostpaywall, trackManifests, BTS/DASH/EMU/HLS, SegmentTemplate/SegmentTimeline); a decoder or demuxer (GStreamer/gstreamer-rs, libmpv/libmpv2, Symphonia, FFmpeg/ffmpeg-next, qtdemux, dashdemux); an output/device layer (ALSA/alsa crate, hw:/plughw:, WASAPI/wasapi crate, CoreAudio/coreaudio-rs, cpal, rodio, kira, snd_pcm_hw_params, AudioClient, exclusive mode, hog mode, bit-perfect); gapless/crossfade/concat/playbin3/about-to-finish logic; ReplayGain, loudness normalization, or volume-slider/taper code; device reservation (org.freedesktop.ReserveDevice1), PipeWire/WirePlumber, or Flatpak/Snap sandbox audio permissions; MPRIS/souvlaki/SMTC/MPNowPlayingInfoCenter integration; buffering, caching, or offline-storage of streamed audio; Dolby Atmos/E-AC-3-JOC or 360 Reality Audio handling; or headless/daemon/CLI playback architecture. Also trigger on these terms — bit-perfect, exclusive mode, gapless, ReplayGain, DASH manifest, FLAC-in-fMP4, HI_RES_LOSSLESS, wasapi2sink, osxaudiosink, alsasink, snd_pcm, hw_params, ao=alsa/wasapi/coreaudio, audio-exclusive, ReserveDevice1, concat element, playbackinfopostpaywall, trackManifests, encryptionType, or streamingSessionId. Do not rely on general "how streaming audio players work" knowledge here — TIDAL's manifest shapes, the ALSA/GStreamer 24-bit naming inversion, per-platform exclusive-mode mechanics, and several plausible-but-wrong claims (mpv's `--audio-exclusive` on the `alsa` AO, the old `--enable-lgpl` mpv flag, wasapi2sink's `exclusive` property on pre-1.28 GStreamer) are all traps this skill exists to prevent.
---

# Audio pipeline for streamboat

Source of truth: `docs/research/audio-pipeline.md` (the full research report — through two rounds of
fact-checking and correction, ~2700 lines, including a "§11 Findings added by a second independent
fact-check" section this skill is fully synchronized with). This skill is the load-on-demand
distillation for coding agents: the facts, parameters, and pitfalls needed while writing code,
without re-reading the whole report every time. Read the report itself for full narrative depth and
the complete source list.

`ref:<project>/<path>` throughout this skill and its references means a shallow, read-only git clone
of a named open-source project held in the research environment
(e.g. `ref:sone/src-tauri/src/audio.rs`) — not part of the streamboat repo, and not guaranteed to
still exist in a later session. Project→URL mapping and commit SHAs: `references/sources.md`.

## Owner decisions already made (do not re-litigate these)

- **Platforms now**: desktop Linux + Windows + macOS, and a headless/server/CLI mode — all four, now.
  Mobile (Android/iOS) is future scope: the architecture must not preclude it, nothing mobile-specific
  ships yet.
- **Stack**: open, "something simple but beautiful" — not decided by this skill. See
  `references/stacks-comparison.md` for the researched options and a recommendation (libmpv first).
- **API/legal posture**: streamboat plays TIDAL for a paying subscriber via the unofficial API, like
  High Tide and Sone — not a downloader/ripper. Never design or document DRM circumvention as a
  how-to; refuse encrypted manifests and say why (`references/tidal-manifest-api.md` §5). Detailed
  API/auth/legal ground is `tidal-api` skill's territory, not this one.
- **Bit-perfect / exclusive-mode output matters** — the owner wants a hi-res-capable client, so ALSA
  `hw:` / WASAPI exclusive / CoreAudio hog mode are core features, not a stretch goal.

## The facts that cause silent bugs if you get them wrong

| # | Trap | The fix |
| --- | --- | --- |
| 1 | ALSA `S24_LE` is **24-in-32** (4 bytes/sample) = GStreamer `S24_32LE`; ALSA `S24_3LE` is **packed** (3 bytes/sample) = GStreamer `S24LE`. The naming is inverted between the two. | Get this backwards and you get a silent 8-bit shift. Write a unit test. `references/output-backends.md` §1. |
| 2 | mpv's `--audio-exclusive=yes` does **nothing** on the `alsa` AO — mpv's own docs say it silently ignores the option there. It only works for `wasapi`, `coreaudio`, `pipewire`, `audiounit`. | On Linux/libmpv, exclusivity comes from opening a `hw:`/`plughw:` device (`--audio-device=alsa/hw:X,Y`), never from `--audio-exclusive`. `references/output-backends.md` §6. |
| 3 | `wasapi2sink`'s `exclusive` GObject property is **new in GStreamer 1.28** (`Since: 1.28` in its gtk-doc) — absent on 1.26 and earlier. Debian 13 "trixie" (current Raspberry Pi OS base) ships GStreamer 1.26.2. Current GStreamer is **1.28.6** (final 1.28 bug-fix release) — not 1.28.2/1.28.3, a version this skill previously understated. | Gate any `wasapi2sink exclusive=true` Design-A code on a GStreamer >= 1.28 runtime check; do not assume it exists. `references/output-backends.md` §4; `references/decoding-and-codecs.md` §1 for the version correction. |
| 4 | `snd_pcm_hw_params()` resets `start_threshold` to 1, which underruns from the first write. | Restore `sw_params` afterward: `start_threshold = floor(buffer/period)*period`, `avail_min = period`. `references/output-backends.md` §1. |
| 5 | Set the ALSA **period before the buffer**. Some USB DACs report absurd `period_size_min` values (documented: 87 frames) when queried after the buffer is set, causing ~1000 interrupts/s. | Set `period_size` first (e.g. 1024 frames), then `buffer_size = period * 4`. `references/output-backends.md` §1. |
| 6 | Bit-perfect mode silently disables **ReplayGain too**, not only the volume slider — Sone's own bit-perfect pipeline branch builds neither the user-volume nor the normalization `volume` element. | State this explicitly in the transparency panel ("ReplayGain: bypassed (bit-perfect)"); do not imply gain is still applied. `references/playback-behavior.md` §4. |
| 7 | libmpv has **no** `org.freedesktop.ReserveDevice1` support of any kind. | The D-Bus reservation handshake must run in the host process around libmpv, exactly as tidalt drives mpv externally — not inside libmpv. Skipping it means exclusive mode is a permanent "device busy" for every PipeWire user. `references/output-backends.md` §2. |
| 8 | Position derived from frames **written** (`frames_written / rate`) runs ahead of what is actually audible by the device buffer's depth — no reference client (Sone included) corrects for this. | Use `snd_pcm_delay()` (`PCM::delay()` in the `alsa` crate): `played = frames_written - delay`. libmpv already does this correctly via `time-pos`. `references/output-backends.md` §8. |
| 9 | Requesting `adaptive=true` on the v2 manifest API returns **multiple DASH Representations** and the player ABR-switches between them mid-stream — a representation change mid-track means a format/rate change means a device reopen means a gap. | Always request `adaptive=false` for a bit-perfect client. `references/tidal-manifest-api.md` §10. |
| 10 | The v2 `trackManifests` response does **not** carry `bitDepth`/`sampleRate` (hardcoded to 0 by TIDAL's own web SDK) — real values must be regexed out of the DASH `Representation@id`/`audioSamplingRate` or HLS `X-COM-TIDAL-SAMPLE-*` tags. | Never trust `bitDepth`/`sampleRate` from a v2 response directly; parse the manifest. `references/tidal-manifest-api.md` §2. |
| 11 | `encryptionType != "NONE"` (or a non-empty `encryptionKey`/`securityType`) in a manifest means **do not play it** — never implement `OLD_AES` decryption (TidaLuna's exists only inside the licensed official client). | Refuse and tell the user the stream is protected; whether TIDAL sends protected streams depends on the client credentials in use. `references/tidal-manifest-api.md` §5. |
| 12 | A 401 carrying a playbackinfo `subStatus` (4xxx) is **not** an auth error — refreshing the token wastes a round trip and never fixes it. Only `4006`/`4033` recover; `4005/4010/4030/4031/4032/4034/4035` are terminal for that request. | Branch on `subStatus` before ever touching the refresh-token path. `references/tidal-manifest-api.md` §7. |
| 13 | FFmpeg's `eac3` decoder **discards JOC object metadata** — decoding Dolby Atmos gets you a 5.1 downmix, not Atmos; a real renderer needs a Dolby licence. | Do not build a software Atmos renderer. If Atmos is ever supported, it's bitstream passthrough to an AVR only. `references/atmos-and-immersive.md`. |
| 14 | mpv's LGPL build flag is the Meson switch **`-Dgpl=false`**, not `--enable-lgpl` (that was the old, now-nonexistent waf-build flag) — and mpv's own docs say LGPL mode is not recommended for anything but libmpv. | Don't put a dead flag in build docs; know LGPL mode disables X11 video, OSS, vdpau, jack, DVD/CDDA/DVB, legacy direct3d. `references/decoding-and-codecs.md` §2. |
| 15 | A `hw:` ALSA device cannot be opened by two processes at once — this is an **architecture** decision (one owning daemon + thin clients, tidalt's model), not a UI detail, and it is how streamboat satisfies "desktop and headless, both now" from one mechanism. | Decide client/server topology before writing the engine trait. `references/os-integration.md` §4. |
| 16 | Plain `uridecodebin`/`playbin` does **not** select the legacy `dashdemux` on its own — `dashdemux2` (gst-plugins-good) outranks it by one `GST_RANK`, so a normal distro/bundle autoplugs `dashdemux2` regardless of source-element choice. | Explicitly demote `dashdemux2`'s rank at startup (`GST_RANK_NONE`) or hook `autoplug-select` — this is required on Windows too, since sone-windows bundles both plugins. `references/output-backends.md` §12. |
| 17 | `set_rate_resample(false)` is **not** "the one call that makes ALSA bit-perfect" — it's a no-op on a raw `hw:` PCM (it only restricts a `plug`/`rate` plugin chain, which `hw:` has none of). | The real guard is reading back `get_rate()` after `set_rate()` and failing if it differs — `alsasink` itself never does this, which is the actual justification for a custom writer. `references/output-backends.md` §1, §13. |
| 18 | The v1 `playbackinfopostpaywall` response is **not** a reliable source of `bitDepth`/`sampleRate` either — TIDAL's own web SDK types both as nullable with the comment "API sends null," and Sone declares them `Option<u32>`. | Parse `bitDepth`/`sampleRate` from the DASH/HLS manifest on **both** the v1 and v2 paths, never from either endpoint's JSON fields directly. `references/tidal-manifest-api.md` §1. |
| 19 | High Tide's whole-track cache is **not** uncapped — a prior draft of this skill said "no size cap"; it is a 5 GB atime-LRU cache evicted once per window creation. AAC decoder availability is **not** solved by "rely on distro codecs" — Fedora's default repos and a from-scratch Windows GStreamer bundle both ship with none. | Don't repeat the "uncapped cache" myth; do add a startup AAC-decoder capability probe and grey out unreachable quality tiers rather than failing at play time. `references/playback-behavior.md` §6, `references/decoding-and-codecs.md` §2. |
| 20 | `assetPresentation != FULL` (a `PREVIEW`) is a required, distinct playback state, not an edge case — v2's web SDK **defaults to `PREVIEW`** when the field is absent (the opposite of v1's `FULL` default). Playing it as if full and reporting it as a play is worse than an error. | Give preview playback its own UI state, suppress play-reporting for it, and surface `previewReason` verbatim. `references/tidal-manifest-api.md` §13. |
| 21 | CDN segment/BTS URLs carry **no** `Authorization` header — they are pre-signed and expire via their own query token, separate from the 1-hour manifest TTL and from OAuth token expiry. | A mid-stream 403 means re-fetch the manifest, not refresh the OAuth token; any bare HTTP client (or a caching relay proxy) can fetch them. `references/playback-behavior.md` §14. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| The two manifest APIs (v1 `playbackinfopostpaywall`, v2 `trackManifests`), MIME types (BTS/DASH/EMU/HLS), DASH `SegmentTemplate`/`SegmentTimeline` + `<BaseURL>` fallback + byte-range seeking (including the `@r`/`startNumber` spec rule), encryption refusal rule, 401 sub-status handling, quality cascade, rate limiting, `streamingSessionId` generation, `mediaMetadataTags`/`audioModes` vocabulary, HLS/EMU handling decision, `PREVIEW`/`previewReason` required behaviour, `countryCode` provenance, streaming-privileges reconnect rules, manifest-fixture capture plan | `references/tidal-manifest-api.md` |
| What must be decoded (codec/tier/container matrix), decoder stack comparison and licensing (GStreamer/FFmpeg/libmpv/Symphonia/ExoPlayer/AVFoundation), AAC patent status, ALAC/AC-4 caveat, Symphonia's undocumented FLAC-in-fMP4 support and its traps, AAC-tier gapless (unsolved), per-distro/platform AAC decoder availability | `references/decoding-and-codecs.md` |
| Bit-perfect/exclusive output on Linux (ALSA `hw_params`/`sw_params`, format & rate probing, promotion ladder, dither rules), PipeWire device reservation and the unresolved PipeWire-native-bit-perfect question, Windows (WASAPI exclusive + failure modes + GStreamer bundling recipe), macOS (CoreAudio hog mode + nominal-rate-change notifications + unproven GStreamer packaging), sandbox implications (Flatpak/Snap), hardware/mixer volume, real-time thread scheduling, verifying bit-perfectness, device identity/persistence, `dashdemux2` rank-collision fix, custom-ALSA-writer-vs-`alsasink` delta, device hot-plug on Windows/macOS, time-to-first-audio budget, bundle-vs-system-GStreamer decision | `references/output-backends.md` |
| Gapless (3 designs + mpv, including the Sone track-finished timing bug), prefetch (with the concrete trigger-point formula), crossfade, fade-out on stop/pause, ReplayGain/loudness-normalization formula, buffering numbers (as three separate pipeline stages, with Pi memory arithmetic), on-disk caching + legal framing (including the High Tide cache-cap correction and its unsafe shared-MPD-path bug), seeking within DASH, network resilience/manifest expiry/streaming-privileges websocket, play-reporting in full (endpoint/threshold/device-impersonation), CDN-no-authentication finding, mid-track network stalls & CDN URL expiry, sample-rate-change audible artifacts, device-hold policy across pause | `references/playback-behavior.md` |
| Dolby Atmos and 360 Reality Audio feasibility on desktop (verdict: don't build either for v1) | `references/atmos-and-immersive.md` |
| OS media integration (MPRIS/souvlaki/SMTC/NowPlaying), the required-local-file artwork rule, idle inhibit (Linux complete; Windows/macOS platform-API recipes with no reference implementation), hot-plug/DeviceMonitor, SMTC's real-HWND requirement (no headless Windows now-playing), headless/Raspberry-Pi architecture (client/server model, Docker recipe, Pi audio hardware specifics, unmeasured CPU/memory feasibility) | `references/os-integration.md` |
| Per-project pointer table (what Sone/High Tide/Strawberry/tidalt/etc. each actually do and where — including the tidal-connect hi-res-ceiling correction), candidate stack comparison table (§8 of the report) including gaps the report's first pass missed (miniaudio, python-mpv, C++ libmpv), the three recommended pipeline designs (A: GStreamer+own writers, B: libmpv, C: pure Rust) with full config and per-design effective MSRV | `references/stacks-comparison.md` |
| Project→GitHub URL mapping, commit SHAs, which upstream/web sources were read directly vs. second-hand | `references/sources.md` |

## Quick orientation — the shape of the whole pipeline

```
TIDAL manifest API (v1 playbackinfopostpaywall | v2 trackManifests)
  -> base64 manifest: BTS (JSON, direct CDN URL) | DASH (MPD XML) | HLS (m3u8) | EMU (JSON)
  -> DASH: SegmentTemplate + SegmentTimeline, FLAC/AAC-LC/HE-AAC/E-AC-3 in fragmented MP4
  -> decode: GStreamer | libmpv (FFmpeg) | Symphonia (no HE-AAC, no DASH — needs hand-rolling)
  -> output: normal (autoaudiosink/pulsesink/pipewiresink) or bit-perfect/exclusive
       Linux:   ALSA hw:/plughw: direct, set_rate_resample(false), sw_params restored
       Windows: WASAPI exclusive (wasapi2sink >= GStreamer 1.28, or libmpv --ao=wasapi)
       macOS:   CoreAudio hog mode + physical-format set (GStreamer sink CANNOT do this —
                only osxaudiosink's SPDIF/passthrough path takes hog mode; libmpv can)
  -> gapless (concat w/ prerolled branch, or mpv --gapless-audio=weak)
  -> loudness normalization: min(10^((replayGain+preAmp)/20), 1/peakAmplitude), preAmp=4, mode=ALBUM
     (bypassed entirely in bit-perfect mode — see pitfall #6)
  -> OS media integration: MPRIS2 (Linux) / SMTC (Windows) / MPNowPlayingInfoCenter (macOS)
```

Manifests expire in exactly 1 hour (`MANIFEST_EXPIRATION_MS = 3600000`); CDN URLs inside a BTS
manifest carry their own, separately-expiring token and take no `Authorization` header at all
(pitfall #21). A `hw:` ALSA device, once reserved, cannot be opened by a second process — this
shapes the headless/desktop architecture question (pitfall #15). GStreamer's own FLAC-in-DASH
support needs >= 1.26.10, which current Debian stable does not ship — route DASH through the legacy
`dashdemux`, not `dashdemux2`, until that changes, **and demote `dashdemux2`'s rank explicitly at
startup** (pitfall #16) — picking a `uridecodebin` variant alone does not select the demuxer.

## Open decisions (feed these into any decision tree or spec-writing task)

Only the owner (thijs) can resolve these — do not assume an answer when writing code or docs:

1. **Which engine?** libmpv (fast, GPLv2+ unless built `-Dgpl=false`, exclusive output on all three
   desktops out of the box, less introspection) vs. GStreamer + own per-platform writers (more code,
   full signal-path introspection, no macOS exclusive without writing a CoreAudio writer) vs. pure
   Rust/Symphonia (no HE-AAC so no `LOW` tier, most work, best mobile story).
   `references/stacks-comparison.md`.
2. **One process or two?** A `hw:`/WASAPI-exclusive/CoreAudio-hog device cannot be shared between
   processes — decide the client/server topology (tidalt's single-owner-daemon model is a concrete,
   working answer) before writing the engine trait. `references/os-integration.md` §4.
3. **Lossless-only?** Dropping `LOW`/`HIGH` removes the AAC patent-licensing question entirely and
   makes the pure-Rust/Symphonia design viable — at the cost of making some content unplayable for
   cheaper-plan subscribers. `references/decoding-and-codecs.md` §3.
4. **Is bit-perfect a headline feature or a power-user toggle, and what is its default state?**
   Determines whether the macOS CoreAudio writer is v1 or v2 work, and whether shipping bit-perfect
   on by default (no working volume for most users — pitfall #6) or off by default (the headline
   feature stays invisible) is the right call. `references/output-backends.md` §7,
   `references/playback-behavior.md` §4.
5. **Offline caching:** none, session-only, or a capped encrypted cache? The sharpest legal/ethical
   line in the project. `references/playback-behavior.md` §6.
6. **Send TIDAL's play-reporting/streaming-metrics events?** Well-behaved-client / royalty-relevant
   vs. less code and less data leaving the machine. `references/playback-behavior.md` §7.
7. **Crossfade:** implement it (TIDAL offers 0–15000 ms) or gapless-only? Mutually exclusive with
   exclusive-device mode by construction. `references/playback-behavior.md` §2.
8. **Video in scope at all?** A separate HLS path, never bit-perfect. `references/tidal-manifest-api.md` §6.
9. **Any DSP ever** — EQ, crossfeed, upsampling, room correction? Any of it breaks bit-perfect by
   definition; CamillaDSP (external) is the "we don't build this" answer. `references/output-backends.md` §7.
10. **Client credentials** (shared feed with the `tidal-api` skill): which client ID/secret, and
    therefore which tiers are reachable and whether streams come back encrypted.
11. **Bundle a pinned GStreamer everywhere, or link the system one on Linux (Design A only)?** Two
    hard version floors (>= 1.26.10 FLAC-in-DASH, >= 1.28 `wasapi2sink exclusive`) exist against a
    Debian/Pi baseline of 1.26.2; Windows/macOS must bundle regardless, so the floors are free there
    — meaning "link system GStreamer on Linux" gives Linux the *worst* FLAC-in-DASH story of the
    three desktop platforms unless deliberately chosen with that tradeoff understood.
    `references/output-backends.md` §18.

## Unverified — do not present these as settled fact in specs or code comments

- **Whether `openapi.tidal.com/v2/trackManifests` is reachable at all with the client credentials an
  unofficial player will actually use** — every v2-using reference project is a registered
  developer-portal app; every unofficial client uses v1 only. This bears directly on whether "prefer
  v2" is buildable. `references/tidal-manifest-api.md` §11.
- **Whether over-requesting `audioquality` on v1 always returns HTTP 200 with a silent downgrade** —
  one project's code comment, in tension with another project's descending retry ladder.
  `references/tidal-manifest-api.md` §7.
- Whether libmpv accepts a `data:application/dash+xml;base64,…` URI directly, or needs a widened
  protocol whitelist / a temp-file `file://` fallback. `references/stacks-comparison.md` §3 (Design B).
- Why one GStreamer-based client disables gapless specifically on `pipewiresink`.
  `references/playback-behavior.md` §1.
- The exact upstream commit behind GStreamer 1.26.10's FLAC-in-DASH support, and whether it changes
  `dashdemux` (legacy) as well as `dashdemux2`. `references/decoding-and-codecs.md` §1.
- Whether TIDAL's Windows/macOS desktop apps really lack Dolby Atmos support — rests on second-hand
  reporting; `support.tidal.com`/`tidal.com` are blocked from the research environment.
  `references/atmos-and-immersive.md`.
- The exact per-tier bit rates TIDAL publishes today (same blocked-domain problem).
- Whether `SegmentTemplate@startNumber` and the literal `AdaptationSet@mimeType="audio/mp4"` string
  appear in real TIDAL MPDs — the DASH-spec defaults/conventions are now stated explicitly
  (`references/tidal-manifest-api.md` §4) but nothing in the reference set observes a captured TIDAL
  manifest; see the fixture-capture recommendation, §16 of that file.
- **Whether bit-perfect output is achievable *through* PipeWire at all, without grabbing `hw:`
  directly** — unresolved in two research passes now (`docs.pipewire.org` is blocked from this
  environment). This decides whether the Flatpak build can ever offer bit-perfect output at all, or
  must always grey the toggle out. `references/output-backends.md` §15 has the exact questions and
  doc pages to check.
- Whether Chromium really resamples every audio output path (not just WebAudio/AudioContext) and
  whether `--audio-output-sample-rate` still has effect in 2026 — relevant only if an Electron path
  is ever considered. Also unresolved: the tidal-hifi flags are a user opt-in, not applied by
  default. `references/stacks-comparison.md` §2.
- Whether `souvlaki` 0.8.3 is still actively maintained, and its real MSRV (crates.io says 1.67; its
  own `Cargo.toml` declares `edition = "2024"`, needing Rust >= 1.85). `references/os-integration.md` §1.
- miniaudio's exact device-backend and exclusive-mode capabilities — cited from general knowledge,
  not verified against its docs in this pass. `references/stacks-comparison.md` §2.
- Real-world CPU/memory/thermal numbers for FLAC 24/192 decode plus a second gapless decode branch on
  Pi-class hardware — no reference project publishes benchmarks. `references/os-integration.md` §8
  has the measurement this skill recommends running before finalizing the headless engine choice.
- Whether a `GStreamer.framework`/bundled-dylib macOS packaging shape is workable at all — no
  reference client ships GStreamer on macOS, unlike the fully-documented Windows recipe.
  `references/output-backends.md` §14.
- The AAC/qtdemux+aacparse-handles-`elst`-trimming assumption behind "Sone's `concat` gapless is
  probably fine on AAC too" — asserted, not tested against a real AAC album.
  `references/decoding-and-codecs.md` §5.
- **Settled, previously listed here — no longer unverified:** whether `asiosink` ships in mainstream
  Windows GStreamer builds (it does, bundled by sone-windows; only the ASIO SDK's own redistribution
  terms remain open — `references/output-backends.md` §4).
