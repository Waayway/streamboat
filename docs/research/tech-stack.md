# Technology stack and architecture evaluation for streamboat

Scope: pick the language, UI toolkit, audio engine, process architecture and packaging path
for streamboat — an unofficial TIDAL client that must run as a desktop application on Linux,
Windows and macOS **and** as a headless server/CLI, must eventually reach Android and iOS,
must deliver hi-res bit-perfect audio, must look good, and will be built by a solo owner
using AI coding agents heavily.

Research date: 2026-09-07. Reference checkouts live under
the reference checkouts (shallow clones of the cited GitHub projects; see references/sources.md)
and are cited as `ref:<project>/<path>`.

---

## Summary

- **The audio requirement, not the UI requirement, picks the stack.** Bit-perfect output means
  opening the sound device with a specific format and refusing every resample, dither and mixer
  stage. That is only reachable from a language with direct access to ALSA (`hw:`), WASAPI
  exclusive, and CoreAudio hog mode. Every candidate that cannot do that — plain Electron,
  Kotlin/JVM desktop, .NET without native interop — is disqualified for the audio path even
  where it wins on UI.
- **The webview must never carry audio.** SONE's own pitch is that "browsers and Electron apps
  downsample audio to 48kHz before it leaves the application"
  (`ref:sone/README.md`, "Why SONE?"). This does not disqualify a webview *UI*; it disqualifies a
  webview *player*. SONE is a Tauri app with a Rust/GStreamer audio path and reaches 24-bit/192 kHz.
- **SONE is the single closest precedent and it is Tauri 2 + Rust + React.** Verified from
  `ref:sone/src-tauri/Cargo.toml`: `tauri = "=2.11.2"`, `gstreamer = "0.23"`, `gstreamer-app = "0.23"`,
  `alsa = "0.10"`, `keyring = "3"`, `mpris-server = "0.9"`, `zbus = "5"`, `reqwest = "0.11"`,
  `tokio`, `aes-gcm = "0.10"`, `axum = "0.7"`, `rmcp = "1.7.0"`. Frontend from
  `ref:sone/package.json`: React 19.1, Tailwind 4.1, Jotai 2.17, `@tanstack/react-virtual`,
  `hls.js` 1.6.16, Vite 7, Vitest 4, TypeScript 5.8.
- **Size of the job, measured.** SONE at v0.21.0 is 26,721 lines of Rust
  (`tidal_api.rs` alone is 7,200 lines / 252 KB; `audio.rs` is 3,309 lines / 173 KB) plus 47,609
  lines of TypeScript/TSX across 199 files, 12 atom modules (13 files — one is a colocated test), and
  85 components. High Tide (Python/GTK4) is 6,821 lines of Python plus 22 Blueprint `.blp` files.
  tidalt (Go TUI+daemon) is 13,078 lines of Go plus 357 lines of cgo C/headers — `alsa.c`/`.h` (151
  lines) for ALSA output and `avcodec.c`/`.h` (206 lines) for FFmpeg decode; tidalt's decode path is
  FFmpeg via cgo, not just an ALSA writer. **Correction (fact-checked): Strawberry's `src/` is
  ~166,100 lines of C++/headers, not 14,708** — 491 `.cpp` files (121,981 lines) + 568 `.h` files
  (44,141 lines) across 40 subdirectories; `src/core` alone is 21,510 lines (recount against
  `ref:strawberry/src/`, 2026-09-07; the original count was wrong by ~11x, apparently from an `xargs
  wc -l` batching bug that silently truncated to the last batch's "total" line). Strawberry is by far
  the **largest** codebase in this survey — roughly 3.5x SONE's combined Rust+TS — which changes how
  its evaluation in §4.11 should be read: it is proof Qt/GStreamer can hit bit-perfect on all three
  desktop OSes, not a size comparable to a solo-owner MVP.
- **Cross-platform is not free even inside one stack.** SONE is Linux-only; Windows support exists
  only as a *separate fork*, `sone-windows`, whose README says "Ported with help from AI agents" and
  "may not be actively or correctly maintained". **Correction (fact-checked): the fork's delta is not
  "almost entirely the audio sink."** `sone-windows` forks an *older* SONE release — it is v0.16.0
  (15,753 lines of Rust + 28,518 lines of TS/TSX) against current upstream SONE v0.21.0 (26,721 Rust +
  47,609 TS/TSX) — roughly 59% of upstream's current size and five minor releases behind. Its
  Windows-specific audio code (`wasapi2sink` with the `exclusive` property,
  `ref:sone-windows/src-tauri/src/audio.rs:1236-1244`, plus `souvlaki = "0.8.3"` for SMTC) is real
  evidence that the approach compiles and runs, but do not read the fork as "a sink swap is all a
  Windows port costs" — it is missing five releases of upstream feature work. Plan for a per-OS output
  backend from day one, and budget cross-OS parity as a standing maintenance cost, not a one-time patch.
- **GStreamer is the proven demux/decode layer for TIDAL.** Four independent reference clients use
  it: SONE (Rust), High Tide (Python, `playbin3` with `about-to-finish` gapless,
  `ref:high-tide/src/lib/player_object.py:90-96`), Strawberry (C++/Qt6), mopidy-tidal (Python).
  It is the only engine in the survey with all TIDAL code paths proven: DASH/MPD, BTS direct URL,
  HLS video, and FLAC/AAC/EAC3 decode. **But the two reference projects that ship a GStreamer Windows
  sink disagree on which one to use.** SONE/`sone-windows` use `wasapi2sink exclusive=true`
  (`ref:sone-windows/src-tauri/src/audio.rs:1236-1244`); Strawberry's own startup code *demotes*
  `wasapi2sink` to `GST_RANK_SECONDARY` and ranks `directsoundsink` `GST_RANK_PRIMARY` on Windows, with
  the in-source comment "wasapisink does not support device switching and wasapi2sink has issues, see
  #1227" (`ref:strawberry/src/engine/gststartup.cpp:59-69`). Treat `wasapi2sink` exclusive mode as
  "compiles and works in one AI-assisted fork," not as a community-settled Windows answer — budget a
  hardware-verification pass before trusting it in production.
- **libmpv is the cheapest route to bit-perfect on all three desktop OSes.** mpv documents
  `--audio-exclusive=yes` (WASAPI), a separate `coreaudio_exclusive` AO described as "direct device
  access and exclusive mode (bypasses the sound server)", and `--alsa-resample` off by default.
  Supersonic (Go/Fyne) and Feishin (Electron/React) both delegate audio to mpv. Cost: `--gapless-audio=no`
  is required for strict bit-perfect, per the Feishin bit-perfect discussion.
- **Pure-Rust audio (cpal) cannot do bit-perfect on Windows today.** cpal 0.18.2 (2026-08-16) has no
  exclusive-mode API; its changelog records the opposite direction. **Correction (fact-checked) to the
  version attributions:** 0.17.2 (marked **[YANKED]** in the changelog) added "Enable as-necessary
  resampling in the WASAPI server process"; **0.18.2** (2026-08-16, not 0.18.0) added "Output streams
  no longer reject formats that the built-in resampler can convert"; separately, **0.18.0** (2026-06-06)
  added ALSA `device_by_id()` acceptance of PCM shorthand names like `hw:0,0`/`plughw:foo` — 0.17.0 had
  only introduced `device_by_id()` itself, for generic stable device IDs, not that ALSA-specific
  acceptance. Issue #106 (2016, **still open**) and #459 (2020) both asked for exclusive mode; #459 is
  closed without an implementation. Exclusive WASAPI needs the separate `wasapi` crate (0.24.0,
  2026-08-12, MIT).
- **Tauri 2 is mature on desktop and usable-but-thin on mobile.** `tauri` 2.11.5 released 2026-07-01;
  ~29.4M downloads. Mobile shipped with 2.0 (2024-10-02) but not every desktop plugin is ported.
  Audio on mobile is not a Tauri core capability: the community `tauri-plugin-native-audio` (1.0.5,
  2026-03-02, MIT/Apache-2.0) wraps Android Media3 ExoPlayer and iOS AVPlayer — but has only ~1,544
  total downloads, i.e. essentially unproven.
- **Flathub's AI policy changed in 2026 and is now a disclosure regime, not the blanket ban the
  press reported.** A commit ("Reword LLM policy to make it clear it's not allowed",
  `flathub-infra/documentation@992f57b`, dated "May 2026" by inference from press coverage — the
  commit's own date was not independently visible when checked) tightened it to "Applications
  containing AI-generated or AI-assisted code, documentation, or other content are not allowed", with
  "Exceptions may be granted for mature, well-maintained projects". The **live**
  `docs/02-for-app-authors/02-requirements.md` read on 2026-09-07 is softer: "Submitters must disclose
  any AI-generated code, documentation, packaging, or other material…", "Disclosed AI-generated
  material is evaluated at reviewer discretion", **"Disclosure does not create a presumption of
  acceptance,"** and a hard prohibition: "AI tools or agents must not open or automate Flathub
  submission pull requests, or generate their commit messages, descriptions, review comments,
  or replies." This is directly load-bearing for streamboat's stated build method.
- **Apple's App Store guideline 5.2.2 is a hard gate for an unofficial TIDAL client on iOS**:
  "If your app uses, accesses, monetizes access to, or displays content from a third-party service,
  ensure that you are specifically permitted to do so under the service's terms of use. Authorization
  must be provided upon request." An unofficial client using the undocumented API cannot produce that
  authorization. iOS distribution is therefore sideload/TestFlight/EU-alt-store territory, not the App Store.
- **GTK4 is a Linux-first choice.** GTK 4.22.4 (2026-04-30) has, in GTK's own words, "partial support"
  for macOS and Windows; libadwaita is explicitly a GNOME visual API that ignores `gtk-theme` by design.
  Choosing GTK means accepting a beautiful Linux app and a foreign-looking Windows/macOS app.
- **Every Rust-native UI toolkit is viable for audio and weak for agents.** iced 0.14.0 (2025-12-07,
  MIT), egui 0.36.1 (2026-08-07), Slint 1.17.1 (2026-07-07, tri-licensed GPL-3.0 / royalty-free /
  commercial), GPUI 0.2.2 (2025-10-22, Apache-2.0, pre-1.0 with frequent breaking changes),
  Relm4 0.11.0 (2026-04-08), Xilem still alpha. All have far less training-corpus mass than
  React+TypeScript, which is the dominant factor for AI-agent throughput.
- **The headless mode is a solved problem shape and tidalt is the model.** tidalt runs one process
  that owns the ALSA device and the D-Bus name `org.mpris.MediaPlayer2.tidalt`; every later invocation
  detects the taken name and becomes a thin client. `tidalt daemon` is the headless mode,
  `tidalt setup --daemon` writes `~/.config/systemd/user/tidalt.service`. Music Assistant is the
  server-first variant: a command-based API over WebSocket (port 8095) with a partial REST projection.
- **Recommended shape: one Rust workspace, one control protocol, two front ends.** A `core` crate
  (TIDAL API + auth + models), a `player` crate (engine trait + per-OS backends + queue state machine),
  a `daemon` binary (WebSocket + MPRIS + systemd), a `cli` client, and a Tauri desktop shell that
  speaks the *same* protocol against an in-process engine by default and can attach to a remote daemon
  instead. tidalt proves the "first instance is the server, second is a client" pattern works.
- **Recommended stack: Rust + Tauri 2 + React 19 + Tailwind 4 + GStreamer, with an `AudioEngine` trait
  boundary.** Score 90/100 on the weighted matrix below. Runners-up: Rust core + Slint (78, single
  language, no webview) and Rust core + Flutter via `flutter_rust_bridge` (82, best mobile path).
- **Estimated effort to MVP** (login, browse, search, play with quality selection, queue, media keys,
  settings, one packaged OS), solo with heavy agent use at ~20-25 h/week: 6-10 weeks for
  Tauri+React; 8-14 weeks for Rust+Slint; 10-16 weeks for Flutter+Rust core. Feature parity with the
  native client is a 9-18 month project regardless of stack — SONE took 21 minor releases to get there.
- **Biggest risks, ranked:** (1) macOS bit-perfect is unproven in every reference project surveyed;
  (2) WebKitGTK rendering divergence on Linux costs real workarounds — SONE ships one in CSS for an
  SVG `stroke-width` double-zoom bug (`ref:sone/src/App.css`); (3) shipping GStreamer on Windows/macOS
  is a heavy packaging problem; (4) Flathub's AI-disclosure regime may reject an agent-built app;
  (5) unofficial-client status blocks the iOS App Store outright.

---

## Findings

### 1. What is actually being decided

The brief contains four hard constraints and one soft one. Read as engineering requirements:

| Requirement | Consequence for the stack |
|---|---|
| Desktop Linux + Windows + macOS **now** | The UI toolkit must produce an acceptable app on all three without a per-OS rewrite. Rules out GTK4/libadwaita and SwiftUI as the primary shell. |
| Headless / server / CLI **now** | The player engine and API client must be usable with no GUI, no display server, and no webview process. Rules out any design where the player lives in the renderer. |
| Mobile **later**, must not be precluded | The core must be a library, not a monolith welded to a desktop toolkit. Favours Rust/C-ABI cores with binding generators (UniFFI, flutter_rust_bridge) or Tauri's own mobile targets. |
| Hi-res bit-perfect audio | Direct device access per OS: ALSA `hw:`, WASAPI exclusive, CoreAudio hog/physical-format. Rules out pure-managed runtimes and webview audio. |
| "Simple but beautiful" (soft) | Favours a styling system with a high design ceiling and cheap iteration. CSS is the highest-ceiling, lowest-friction option available; native toolkits trade ceiling for integration. |

One more constraint is implicit and, in practice, decisive: **the developer is one person driving AI
coding agents.** That makes *training-corpus mass and type-checkability of the stack* a first-class
selection criterion, not a footnote. Two of the reference projects are explicit about being built this
way: `ref:tidalt/README.md` states the project was written entirely with the help of LLM coding
assistants, and `ref:sone-windows/README.md` says "Ported with help from AI agents". Both are Rust or Go with strong
type systems; the Windows port in particular is a mechanical, type-guided port of a Rust codebase.

### 2. The audio pipeline decides the stack

#### 2.1 What "bit-perfect" concretely requires

From SONE's implementation and README, and tidalt's, bit-perfect means all of:

1. Decode to PCM at the source's native sample rate and bit depth — no resampling.
2. Open the hardware device in a mode that bypasses the OS mixer (ALSA `hw:` / WASAPI exclusive /
   CoreAudio hog mode + physical format change).
3. Negotiate a PCM format the DAC actually supports rather than letting a library convert.
4. Apply no volume scaling in software (SONE locks the volume slider at 100% in bit-perfect mode).
5. Apply no ReplayGain in bit-perfect mode (it is a multiply, therefore not bit-perfect).

SONE's format negotiation table is concrete and worth copying
(`ref:sone/src-tauri/src/audio.rs:190-211, 483-609`):

| GStreamer format | ALSA format (Rust `alsa` crate) | bytes/sample |
|---|---|---|
| `S16LE` | `Format::S16LE` | 2 |
| `S24LE` | `Format::S243LE` (24-bit packed, 3 bytes) | 3 |
| `S24_32LE` | `Format::S24LE` (24-in-32 container) | 4 |
| `S32LE` | `Format::S32LE` | 4 |
| `F32LE` | `Format::FloatLE` | 4 |

Note the naming trap that SONE annotates in-source: **ALSA `S24LE` is GStreamer `S24_32LE`, and ALSA
`S243LE` is GStreamer `S24LE`.** Getting this backwards produces silence or noise. SONE probes the
device with `snd_pcm_hw_params` for each candidate format and each rate before choosing
(`probe_supported_gst_formats`, `probe_supported_rates`, `ref:sone/src-tauri/src/audio.rs:483,545`),
then sets `start_threshold` and `avail_min` on the software params because
"`snd_pcm_hw_params()` resets start_threshold to 1" (`:692`).

tidalt does the same thing in C via cgo, with a documented preference order that differs by source
depth: for 16-bit sources `S32_LE > S16_LE > S24_3LE > S24_LE` (S32_LE first because of a specific
Hidizs USB DAC problem — CS43198-based devices per `ref:tidalt/docs/dac-compatibility.md`), for
24-bit sources `S24_3LE > S24_LE > S32_LE` (`ref:tidalt/internal/player/alsa.c:28-39`,
`ref:tidalt/CLAUDE.md:27-30`). **Correction (fact-checked): this order is not documented in
`ref:tidalt/README.md`** — that file covers only the `plughw:` fallback semantics, not the format
preference table. Worse, **tidalt's own `ref:tidalt/docs/architecture.md:43` states a different
16-bit order** (`S16_LE → S24_3LE → S24_LE → S32_LE`), directly contradicting the code. Cite `alsa.c`
(and `CLAUDE.md`) as authoritative, never `architecture.md` — and if streamboat copies a format table
like this, keep exactly one copy of it, physically next to the code, so it cannot drift the way
tidalt's own docs did. tidalt also takes a D-Bus PipeWire reservation
(`org.freedesktop.ReserveDevice1.Audio{N}`) before opening the device and releases it on stop, and
falls back to `plughw:` only when `snd_pcm_hw_params` refuses the format — distinguishing a format
refusal (retries through the plug layer, sets `bitPerfect=false`, UI badge shows `(converted)`) from a
busy device (keeps retrying `hw:` on `-EBUSY`, never downgrades).

#### 2.2 Candidate audio engines

| Engine | Bit-perfect Linux | Bit-perfect Windows | Bit-perfect macOS | DASH/HLS | Gapless | Deployment cost off-Linux |
|---|---|---|---|---|---|---|
| **GStreamer** (`gstreamer` 0.25.3, MIT/Apache) | Yes — `alsasink device=hw:X,Y`, or appsink + own ALSA writer (SONE) | Yes in one fork — `wasapi2sink` with `exclusive=true` (`ref:sone-windows/src-tauri/src/audio.rs:1236-1244`) — **but Strawberry's own startup code ranks `wasapi2sink` `SECONDARY` and `directsoundsink` `PRIMARY` on Windows, citing device-switching problems (`ref:strawberry/src/engine/gststartup.cpp:59-69`, issue #1227). Needs hardware verification, not settled.** | **Unproven.** `osxaudiosink` has no documented hog-mode/physical-format control | Yes — `dashdemux`, `hlsdemux`, `uridecodebin` | Yes — `concat` (SONE) or `playbin3` `about-to-finish` (High Tide); SONE notes gapless needs GStreamer ≥1.24 | High. Must ship the runtime: GStreamer documents packing the MSI and running it via `msiexec`, or Merge Modules, on Windows; a PackageMaker-style bundle on macOS (macOS page unverified here — domain blocked) |
| **libmpv** | Yes — `--audio-device=alsa/hw:1,0 --audio-exclusive=yes`, `--alsa-resample` off by default | Yes — `--ao=wasapi --audio-exclusive=yes` (buffer tuned via `--wasapi-exclusive-buffer`) | Yes (in mpv's own docs) — `--ao=coreaudio_exclusive --coreaudio-change-physical-format=yes` ("direct device access and exclusive mode (bypasses the sound server)"); **one field report describes it misrouting between DAC and HDMI** (see §2.3) | Yes (via FFmpeg) | Yes, but must be **disabled** for strict bit-perfect (`--gapless-audio=no`) | Medium. One shared library; Supersonic bundles it in the AppImage and requires `libmpv1`/`libmpv2` on Linux |
| **cpal 0.18.2 + symphonia 0.6.1** | Partial — `device_by_id()` accepts ALSA shorthand (`hw:0,0`, `plughw:foo`) since **0.18.0** (0.17.0 added `device_by_id()` itself, for generic stable IDs only) | **No.** No exclusive-mode API; 0.17.2 (**yanked**) added "as-necessary resampling in the WASAPI server process"; **0.18.2** (not 0.18.0) stopped rejecting formats the built-in resampler can convert | No | **No** — symphonia decodes containers (OGG/WAV/MKV/MP4/CAF/AIFF) and codecs (MP3, FLAC, Vorbis, AAC, ALAC, ADPCM, PCM) but does not fetch or demux DASH/HLS, and does not decode EAC3/AC4 | Manual | Zero — pure Rust, static |
| **Per-OS hand-rolled** (`alsa` 0.10 + `wasapi` 0.24.0 + `coreaudio-rs`) + symphonia for decode | Yes | Yes | Yes (in principle) | No — you write the MPD/HLS fetcher | Manual | Zero, but highest code volume |
| **FFmpeg/libav direct** (`ffmpeg-next`/`rsmpeg`) + per-OS sink | Yes | Yes (in principle — you wire exclusive mode yourself) | Yes (in principle) | Yes — `libavformat` demuxes DASH/HLS | Manual | Medium. One shared/static FFmpeg build (own licensing/build-config work), no plugin registry to ship — smaller than GStreamer off-Linux. This is what tidalt actually does via cgo: `libavformat`/`libavcodec`/`libswresample`, FLAC/AAC/ALAC decode, resample to S32LE (`ref:tidalt/internal/player/{avcodec.c,alsa.c}`, `ref:tidalt/docs/architecture.md`) |

Notes that matter:

- **symphonia is MPL-2.0**, not MIT/Apache. Fine for a GPL project; note it if the licence ever changes.
- **rodio 0.22.2** (2026-03-05, MIT/Apache) sits on cpal and inherits its ceiling. Its own description
  is a general-purpose audio playback/recording library, not specifically a "game-audio library" —
  either way, not a bit-perfect player.
- **A middle option exists between "ship all of GStreamer" and "pure Rust, no DASH/HLS/no Atmos":
  FFmpeg/libav directly.** tidalt already does this via cgo — `libavformat`/`libavcodec`/`libswresample`
  decode FLAC/AAC/ALAC and resample to S32LE, in 206 lines of C (`avcodec.c`/`.h`) alongside the
  111-line `alsa.c` writer — and libmpv is internally the same design. Rust equivalents:
  `ffmpeg-next`, `rsmpeg`. Cost: FFmpeg's own licensing/build-config surface and writing your own
  DASH/HLS fetch layer. Win: one dependency instead of a plugin registry, far smaller to ship on
  Windows/macOS than GStreamer. See the added table row above.
- **EAC3-JOC / Dolby Atmos**: python-tidal lists MP3, AAC/MP4A, FLAC, EAC3, AC4 as TIDAL codecs.
  symphonia does not decode EAC3/AC4; GStreamer and FFmpeg/mpv do (with `gst-libav` /
  `gstreamer1.0-libav`, which SONE's package deps include). If Atmos is ever in scope, pure-Rust
  decode is out.
- **Gapless conflicts with bit-perfect** in every engine: gapless requires holding the device open
  across tracks, which forces a single format. SONE resolves this by making gapless Normal-mode-only
  and panicking if the code path is reached in DirectAlsa
  (`"PlaybackBackend::concat() called on DirectAlsa — gapless is normal-only"`,
  `ref:sone/src-tauri/src/audio.rs:156`). Copy that decision explicitly rather than discovering it.

#### 2.3 Recommendation for the audio layer

Put an `AudioEngine` trait in `streamboat-player` with a small surface
(`load(uri, hints)`, `play`, `pause`, `seek`, `set_volume`, `position`, event stream) and ship
**GStreamer first**. Reasons:

1. It is the only engine with all four TIDAL code paths proven in the reference set (DASH manifest as
   a `data:application/dash+xml;base64,…` URI, BTS direct URL, HLS video, and FLAC/AAC/EAC3 decode).
2. `wasapi2sink exclusive=true` *compiles and ships* in a real TIDAL client (`sone-windows`) — but this
   is not a settled answer. **Strawberry's own GStreamer startup code demotes `wasapi2sink` and ranks
   `directsoundsink` primary on Windows**, citing device-switching problems
   (`ref:strawberry/src/engine/gststartup.cpp:59-69`, upstream issue #1227). Budget a Windows
   hardware-verification step (confirm the DAC actually receives exclusive-mode, unresampled PCM, not
   just that the pipeline builds) before shipping this as a claim, and keep `directsoundsink` as a
   documented fallback rank.
3. The Rust bindings are healthy: `gstreamer` 0.25.3 (2026-06-29), MIT/Apache, feature flags for
   GStreamer 1.16 → 1.30 (MSRV 1.92, edition 2024 — newer toolchain requirement than SONE's pinned
   0.23).

Keep libmpv behind the same trait as the second backend, specifically because it is the only
documented path to macOS bit-perfect (`coreaudio_exclusive` + `--coreaudio-change-physical-format=yes`)
and because it dramatically shrinks the macOS/Windows packaging problem. Treat macOS bit-perfect as an
**experiment with a hardware verification step**, not a shipped claim, until measured — the Feishin
discussion contains a user report of `coreaudio_exclusive` routing unexpectedly between DAC and HDMI,
and the original poster could not verify Apple behaviour for lack of a sample-rate indicator on the DAC.
**No reference project in this survey demonstrates verified bit-perfect output on macOS by any engine**
— GStreamer's `osxaudiosink` has no documented hog-mode control and mpv's `coreaudio_exclusive` is the
only documented path, unverified on hardware. Whichever engine streamboat ships first, macOS bit-perfect
is a research spike with its own line item, not a checkbox that falls out of the Linux/Windows work.

### 3. Architecture shapes

#### Shape 1 — single Rust workspace (core crate + desktop UI crate + daemon binary)

```
streamboat/
  crates/
    streamboat-core/      # HTTP client, OAuth (device code + PKCE), models, catalog, library, cache
    streamboat-player/    # AudioEngine trait, gstreamer backend, queue + state machine, ReplayGain
    streamboat-proto/     # command/event types, serde, shared by daemon + clients (single source of truth)
    streamboat-daemon/    # headless binary: WebSocket server, MPRIS, systemd unit, config
    streamboat-cli/       # TUI / one-shot client over the protocol
  desktop/
    src-tauri/            # thin Tauri shell: window, tray, deep links, global shortcuts
    src/                  # React frontend
```

- **State sync**: none needed in the desktop default path — the Tauri shell links `streamboat-player`
  directly and dispatches the same `Command` enum in-process. No IPC latency, no serialization of the
  hot path.
- **Latency**: Tauri `invoke` round-trips are sub-millisecond for small payloads; the only latency-
  sensitive thing in a music player is the play/pause key, which is fine either way.
- **Offline**: metadata cache in SQLite inside `streamboat-core`; both shells get it for free.
- **Complexity**: lowest of the three. One language, one build, `cargo test` covers the engine.
- **Weakness**: the UI is coupled to Rust. A future Flutter or Compose mobile UI needs Shape 2's
  binding layer added on top — but that is additive, not a rewrite, provided the core stays UI-agnostic.

#### Shape 2 — core as a library with FFI/UniFFI bindings to a non-Rust UI

UniFFI 0.32.0 (2026-06-30, MPL-2.0, ~11.9M downloads) generates Kotlin, Swift, Python and Ruby
bindings from a Rust crate; Mozilla uses it in Firefox mobile and desktop. Its own docs say
"We consider it ready for production use, but UniFFI is a long way from a 1.0 release", and Swift 6
support is described as partial.

- **When it pays**: the moment you want a SwiftUI iOS app and a Compose Android app sharing the
  streamboat core. Then the core is compiled to a `.xcframework` and an `.aar`, and each platform
  writes its own UI and its own audio output.
- **Cost**: the FFI boundary must be designed — no `&mut` graph handles, no Rust enums with payloads
  crossing casually, async needs care. Expect to write a facade crate whose only job is to be
  FFI-shaped.
- **Verdict**: adopt this *later*, additively, when mobile becomes real. Do not pay for it now.
  But **do** keep the core free of Tauri/GTK/toolkit types today so the facade is cheap to add.

#### Shape 3 — everything talks to a local daemon over WebSocket ("the GUI is just another remote")

Precedents: Music Assistant exposes a unified command-based API over WebSocket on port 8095 with a
partial JSON/REST projection; tidalt uses D-Bus instead of WebSocket but the same topology (one server
owns the device, everyone else is a client).

- **State sync**: daemon owns authoritative `PlayerState`; pushes typed events; clients send commands.
  Send position as a `(position, wall_clock, rate)` triple and let clients interpolate — do not stream
  a position tick at UI framerate.
- **Latency**: local loopback WebSocket is sub-millisecond; irrelevant in practice.
- **Complexity**: highest. Every UI action becomes a protocol message; every state read becomes a
  subscription. You need a reconnect story, a versioned protocol, an auth story for the socket
  (SONE gates its MCP server on a generated token — copy that).
- **Offline**: the desktop app is useless without the daemon running. That is a real UX regression
  for the 95% single-machine case.
- **Where it wins**: remote control from a phone or another machine, multi-client, and the headless
  server story. Which streamboat needs anyway.

#### Recommended hybrid

Define the protocol once in `streamboat-proto`. Give the desktop app **two transports for the same
protocol**: an in-process transport (default; the shell owns the engine) and a WebSocket transport
(when `--connect ws://host:port` is passed, or when a local daemon already holds the device).
Follow tidalt: on startup, try to claim the device/D-Bus name; if it is already claimed, become a
client. This gets Shape 1's simplicity for the common case and Shape 3's reach without a second
codebase.

Also expose the same command surface as:
- **MPRIS** on Linux (`org.mpris.MediaPlayer2.streamboat`) — works headless, drives `playerctl`,
  media keys and desktop widgets. tidalt is explicit that "A plain `tidalt` TUI session does not
  register a persistent MPRIS2 service", so register it from the engine, not the GUI.
- **SMTC** on Windows and **MPNowPlayingInfoCenter** on macOS via `souvlaki` 0.8.3 (MIT). Caveat from
  souvlaki's own README: **Windows requires an HWND** and macOS "requires an AppDelegate/winit event
  loop (an open window is not required)". A truly headless Windows daemon therefore gets no SMTC
  without extra work — the practical answer is: no SMTC in headless Windows mode, document that rather
  than trying to fake an HWND.

**Headless is a solved problem shape on Linux/systemd only — containers, macOS and Windows still need
answers.** "Headless/server/CLI now" means the first thing a server user does is run it in Docker on a
NAS or a Pi. tidalt's Docker recipe generalises directly: expose sound devices with `--device /dev/snd`
(which also makes `/proc/asound` readable, the mechanism tidalt uses for device discovery), join the
host audio group with `--group-add $(getent group audio | cut -d: -f3)` because `/dev/snd` nodes are
`root:audio`, persist the config dir (OAuth session) and data dir (device preference, volume, metadata
cache) as volumes, and optionally forward `-v /run/user/$(id -u)/bus:/run/user/1000/bus` so the
PipeWire/WirePlumber `org.freedesktop.ReserveDevice1` handoff still works; build for `linux/amd64` and
`linux/arm64` (`ref:tidalt/docs/docker.md`). No reference project answers macOS (launchd plist +
CoreAudio from a non-GUI process) or a Windows service (souvlaki's SMTC needs an HWND, so a Windows
service gets no now-playing integration) — treat both as open engineering work, not solved-elsewhere
problems.

**The audio engine is the hardest component and the one no reference project tests in-process.**
SONE's `audio.rs` (3,309 lines, the hardest code in the project) has zero `#[cfg(test)]` modules; its
only audio-specific test artefact is a 21-line `src-tauri/tests/gapless_probe.py`, and its only
dev-dependency is `tempfile = "3"` (`ref:sone/src-tauri/Cargo.toml`). tidalt is the partial exception,
with `internal/player/alsa_fallback_test.go` (68 lines) covering the `plughw:` fallback decision. CI
runners have no sound card, so design the `AudioEngine` trait to admit a headless backend from day
one — a GStreamer `fakesink`/`appsink` capturing PCM, or an ALSA `null` device / `snd-dummy` module —
so the queue state machine, format negotiation table and quality cascade are testable with
`cargo test`, and a golden-PCM comparison of a decoded FLAC is possible without hardware. (See
`/home/user/streamboat/docs/research/engineering-baseline.md:1405`, tagged **[STACK]** for exactly this
question.)

### 4. Candidate stacks, one by one

#### 4.1 Rust core + Tauri 2 + web frontend — **recommended**

**Status.** `tauri` 2.11.5, 2026-07-01, Apache-2.0 OR MIT, ~29.4M total downloads, MSRV 1.77.2.
Bundle targets: `deb`, `rpm`, `appimage`, `nsis`, `msi`, `app`, `dmg`, or `all`. macOS notarization is
built into the bundler via App Store Connect API key or Apple ID; iOS signing is documented separately.

**Webview per OS.** WebView2 (Chromium) on Windows, WKWebView on macOS, WebKitGTK on Linux
(`webkit2gtk = "=2.0.2"` is a Linux-only dependency in `ref:sone/src-tauri/Cargo.toml`; the deb
depends on `libwebkit2gtk-4.1-0`). This is the framework's central trade-off: you get CSS's design
ceiling and you pay for engine divergence on Linux.

**The divergence is real and costs code.** Documented WebKitGTK problems as of 2026: font weight
offset by 100 (tauri#14286), blurry rendering during CSS animations, `contenteditable` spans not
behaving as inputs, "shadow copy" artefacts on maximize/unmaximize (tauri#13157), and DMABUF renderer
crashes needing `WEBKIT_DISABLE_DMABUF_RENDERER=1` at the cost of the fast path. SONE ships a CSS
workaround for one of these in `ref:sone/src/App.css`:

> `/* WebKitGTK applies the CSS 'zoom' factor to the SVG stroke-width presentation attribute a second time, so icons render 'zoom'x too heavy (and too light when zoomed out). A CSS declaration overrides the attribute and scales once. */`

and its README carries an NVIDIA note: "If you see a blank window, rendering glitches, or a Wayland
protocol error on launch, start the app with `WEBKIT_DISABLE_COMPOSITING_MODE=1 sone`". Budget for a
handful of such workarounds, not for a rewrite. The practical WebKitGTK floor is whatever engine ships
on your build container's distro: SONE's deb depends on `libwebkit2gtk-4.1-0` and is built
`FROM ubuntu:22.04` (`ref:sone/build-scripts/build/Dockerfile.deb`), so Ubuntu 22.04's WebKitGTK 4.1 is
the practical baseline to design against — bundle fonts locally (no Google Fonts CDN; the app must
work offline and the CSP should not need to allow it), prefer plain CSS/Tailwind over engine-specific
effects, and treat `backdrop-filter`, heavy CSS animation and SVG presentation attributes as suspect.

**Capabilities, CSP and plugins — the first things an implementer hits.** Tauri 2's biggest change from
v1 is the capabilities ACL: every window/command permission must be explicitly listed in
`src-tauri/capabilities/*.json` or the call silently fails. SONE ships one capabilities file
(`ref:sone/src-tauri/capabilities/default.json`): identifier `default`, `windows: ["main", "miniplayer"]`,
and permissions `core:default`, a long list of `core:window:allow-*` (fullscreen, create,
always-on-top, close, minimize/unminimize, maximize/unmaximize, set-size, start-dragging,
start-resize-dragging, show, set-focus), `core:webview:allow-create-webview-window`, `opener:default`,
`global-shortcut:allow-register`, `deep-link:default`. SONE also sets `security.csp: null` in
`tauri.conf.json` — no content policy at all. Decide deliberately: a real CSP (`img-src` limited to the
TIDAL CDN hosts plus `asset:`/`tauri:`, no inline scripts) versus copying SONE's open posture knowingly
— a music client renders remote artwork, lyrics and user-authored playlist/track names inside a webview
that holds `invoke`, so this is a security decision, not a formality.

SONE's plugin set (`ref:sone/src-tauri/Cargo.toml`, all version-**pinned exactly** with `=`, a
convention worth copying to avoid surprise webview/plugin behaviour changes mid-project):
`tauri-plugin-single-instance =2.4.2` (with the `deep-link` feature), `tauri-plugin-deep-link =2.4.9`,
`tauri-plugin-opener =2.5.4`, `tauri-plugin-window-state =2.4.1`, `tauri-plugin-oauth =2.0.0`,
`tauri-plugin-global-shortcut =2.3.1`. `tauri.conf.json` registers the deep-link scheme `tidal` for
desktop. So the desktop OAuth flow runs **two mechanisms together**: a loopback HTTP listener
(`tauri-plugin-oauth`) for the redirect, and a `tidal://` custom-scheme handler routed through
single-instance for the case where the OS hands the redirect to a URL scheme instead. tidalt registers
the same `tidal://` scheme with XDG (`tidalt setup`, `ref:tidalt/docs/client-server.md:64`) — decide
which mechanism (or both) streamboat needs before building the login screen.

**Window chrome is a hidden cost of "beautiful."** SONE's window config is `decorations: false`,
`visible: false` until first paint (avoids a white flash), 1200×800, plus a second `miniplayer` window.
Turning decorations off moves the titlebar, drag regions, resize handles, and (per OS) macOS traffic
lights / Windows Snap Layouts into your own code — which is why its capability file needs the whole
`core:window:allow-start-dragging`/`allow-start-resize-dragging`/`allow-minimize`/etc. list above. Decide
early: native decorations (cheap, native feel, constrained design) versus custom chrome (SONE's route,
real per-OS polish work).

**Auto-update coverage is partial, and does not cover deb/rpm.** Tauri v2's updater plugin signs a
static JSON manifest (minisign keypair via `tauri signer generate`) keyed by platform
(`linux-x86_64`/`darwin-aarch64`/`windows-x86_64`) with per-platform `url` + `signature`, TLS enforced
in production. Bundle coverage is **Linux AppImage only** (`.AppImage`, `.AppImage.tar.gz`), macOS
`.app.tar.gz`, and Windows `.msi`/NSIS `.exe` — there is no updater for deb/rpm (those need a hosted
repo, as SONE does on Cloudsmith) or for Snap/Flatpak (store-managed). Plan the update channel per
package format, not as one in-app mechanism.

**Windows WebView2 install mode is a required, unsized config choice.** `bundle.windows.webviewInstallMode`
trades installer size against offline support: `downloadBootstrapper` (default, +0 MB, needs network,
weak on Win7 `.msi`), `embedBootstrapper` (+~1.8 MB), `offlineInstaller` (+~127 MB, installs with no
internet), `fixedVersion` (+~180 MB, pins an exact WebView2 build inside the app — the only lever that
freezes webview-divergence risk on Windows), `skip` (+0 MB, app will not run without the runtime
present). Windows 10 (April 2018 update or later) and Windows 11 ship the runtime with the OS, so
`downloadBootstrapper` is mainly a first-run risk on older or offline machines — pick deliberately
rather than defaulting silently.

**Cross-compilation does not exist; there is no reference build CI at all.** `tauri build` produces
artifacts only for the host platform (Museeks' README states this explicitly: "Tauri does not support
cross-platform binaries, so the command will only generate binaries for your current platform"). SONE's
own repo has **no build workflow** — `.github/workflows/` holds only `flathub-update.yml` — and releases
come from local per-distro Docker builds (`build-scripts/build/Dockerfile.deb` = `FROM ubuntu:22.04`,
chosen for the oldest glibc/WebKitGTK floor; `Dockerfile.rpm` = `FROM fedora:42`; plus
`Dockerfile.pacman`, `Dockerfile.rpm-opensuse`, a `PKGBUILD`, and a `build-scripts/test/` suite that
installs each package). Windows support in that ecosystem exists only as a separate fork. Budget three
release pipelines (Linux/Windows/macOS build hosts, three signing setups) rather than one CI matrix that
cross-compiles.

**Flathub packaging needs two offline-source generators, regenerated every release.** SONE's release
automation runs `flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json` and
`flatpak-node-generator pnpm --pnpm-store-version v11 pnpm-lock.yaml -o pnpm-sources.json`
(`ref:sone/.github/workflows/flathub-update.yml:95-139`) then commits the manifest plus both sources
files into the Flathub repo. This is itself an automated commit to a Flathub-adjacent repo — decide
explicitly what the release bot may commit versus what the human owner must author, given §9's AI
policy.

**macOS packaging has a documented hook for the audio backend.** Tauri's bundler supports
`bundle.macOS.frameworks`, which accepts system frameworks, custom `.framework` bundles, and `.dylib`
files — the config schema's own example lists `"CoreAudio"` alongside a custom dylib and a custom
framework, which is the hook for shipping `GStreamer.framework` or `libmpv`.
`bundle.macOS.minimumSystemVersion` defaults to macOS 10.13 and should be raised deliberately (12.0+ is
defensible); entitlements are applied at signing time via `bundle.macOS.entitlements`, which is where a
Hardened Runtime exception for the media backend would go.

**For a Tauri project to build for Android/iOS, the app crate must be a library from day one** — this
is the concrete shape "do not preclude mobile" implies, and it is cheap now and expensive to retrofit
after ~85 components exist: `[lib] crate-type = ["staticlib", "cdylib", "rlib"]`, all startup logic in
`src/lib.rs` behind `#[cfg_attr(mobile, tauri::mobile_entry_point)] pub fn run()`, with `src/main.rs`
reduced to calling `run()`. SONE already has this shape (its Cargo.toml comment notes the `_lib` name
suffix avoids a Windows name clash, rust-lang/cargo#8519). Tauri mobile targets Android 8/API 26 and
iOS 9 at the platform floor; the community `tauri-plugin-native-audio` needs Android 8.0+/iOS 14.0+ and
the iOS Background Modes → Audio capability.

**IPC has a large-library data-path trap.** Every `Serialize` return value from a Tauri command is
serialized to JSON; the docs warn this "can slow down your application if you try to return large data
such as a file or a download HTTP response," and provide `tauri::ipc::Response::new(bytes)` for raw
byte payloads. Design rules: page every list command, push state *deltas* not snapshots, never
round-trip album art as base64 JSON (cache covers Rust-side and serve through the asset/custom protocol
or `convertFileSrc`), and keep the playback position tick out of IPC entirely — send
`(position, wall_clock, rate)` and interpolate client-side, the same approach §3 already recommends for
the WebSocket transport.

**Audio.** Entirely outside the webview, in Rust. SONE proves the whole path: quality cascade,
manifest decode, GStreamer pipeline, exclusive ALSA writer thread, gapless via `concat`, signal-path
probing, ReplayGain. `ref:sone/src-tauri/src/audio.rs` is 3,309 lines — that is the honest price of
this feature set in this stack, and it is the smallest such implementation in the survey.

**Headless.** A Tauri workspace is a normal Cargo workspace. Add `crates/streamboat-daemon` with a
`[[bin]]` that does not depend on `tauri` at all. `cargo build -p streamboat-daemon` on a server with
no GTK/WebKit installed works as long as the daemon crate does not pull the desktop crate. Enforce
with a workspace dependency graph and a CI job that builds the daemon in a minimal container.

**Mobile.** Tauri 2 targets Android and iOS from the same project. Reality check: mobile support is
newer and less mature than desktop, not all desktop plugins are ported, and **audio is not covered by
a first-party plugin**. The third-party `tauri-plugin-native-audio` 1.0.5 (2026-03-02) wraps Android
Media3 ExoPlayer + `MediaSessionService` + notification controls and iOS AVPlayer +
`MPNowPlayingInfoCenter` + `MPRemoteCommandCenter`, requires Android 8.0+ / iOS 14.0+, and needs the
iOS Background Modes > Audio capability — but has ~1,544 total downloads. Treat it as a starting point
to fork, not a dependency to trust.

**AI-agent friendliness: the strongest of any candidate.** React + TypeScript + Tailwind is the single
best-represented UI stack in any model's training data; Rust's type system and `cargo clippy -D warnings`
catch a large fraction of agent mistakes before a human sees them. SONE's own `check` script is a good
template: `eslint src/ && prettier --check … && cargo clippy -- -D warnings && cargo fmt --check && knip`.
`sone-windows` is direct evidence that agents can port this exact codebase across an OS boundary.

**Licensing.** Tauri MIT/Apache-2.0; React MIT; Tailwind MIT; GStreamer LGPL-2.1 (plugins vary —
`gst-plugins-ugly` and `gst-libav` carry GPL/patent baggage; SONE depends on `gstreamer1.0-libav`).
Nothing here blocks a GPL-3.0 project.

#### 4.2 Rust + Slint — **recommended alternative (single language, no webview)**

**Status.** `slint` 1.17.1, 2026-07-07. Tri-licensed: `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0
OR LicenseRef-Slint-Software-3.0`. The royalty-free desktop/mobile/web licence is free "as long as you
disclose that you use Slint (for example with the AboutSlint widget or the Slint badge)"; without the
disclosure you need the commercial licence. Since streamboat is open source, the GPL-3.0 arm applies
cleanly and the disclosure question is moot. MSRV 1.92, Rust 2024 edition. Renderers: femtovg,
software, Skia, WGPU. ~1.63M downloads.

**Look.** Slint draws its own widgets. That means the ceiling is "whatever you design", the floor is
"nothing looks native anywhere", and the styling language is Slint's `.slint` DSL rather than CSS.
For a music player — which is a custom-looking app on every platform anyway — that is acceptable.

**Audio, headless, packaging**: identical to 4.1 (same Rust core).

**Why it loses to Tauri here**: the `.slint` DSL has a tiny corpus compared to CSS/React. Agents will
produce plausible-but-wrong `.slint`, and the compiler catches structure, not layout intent. Expect
noticeably slower UI iteration. Its mobile support exists but is far less exercised than Tauri's or
Flutter's.

#### 4.3 Rust + iced — not recommended

`iced` 0.14.0, 2025-12-07, MIT, MSRV 1.88, ~2.67M total downloads. 0.14 was a large release
(reactive rendering, time-travel debugging, headless testing, input methods, hot reloading, smart
scrollbars). Used by System76's COSMIC desktop, which is a real maturity signal.

Against: the Elm architecture forces every UI interaction through one `Message` enum and one `update`,
which is verbose for a media app with dozens of concurrent async loads; and iced's API has churned
hard across 0.9→0.10→0.12→0.13→0.14, so agent-generated code frequently targets a dead API. Psst
(Rust Spotify client) sits on druid and its author observed that "most of the complexity in the UI code
is due to deficiencies of the Druid reactive architecture" — the same family of problem.

#### 4.4 Rust + egui — not recommended for the primary UI

`egui` 0.36.1, 2026-08-07, MIT/Apache, ~23M downloads. Immediate-mode, extremely productive, extremely
hard to make *beautiful*: no retained layout, no CSS-grade styling, and a recognisable "tool UI" look.
Good candidate for an internal debug window (a signal-path inspector, a pipeline probe viewer) inside
whatever the real UI is.

#### 4.5 Rust + GPUI — not recommended (too early)

`gpui` 0.2.2, published 2025-10-22, Apache-2.0, ~147k recent downloads. Standalone use is possible
(`cargo add gpui`), and Zed itself is proof the framework can produce a genuinely beautiful, fast app.
But it is pre-1.0 with frequent breaking changes between versions, documentation is thin relative to
its ambition, and the standalone (non-Zed) story is young. For a solo developer with agents, the
combination of "sparse docs" and "breaking changes" is the worst possible pairing.

#### 4.6 Rust + GTK4/libadwaita (gtk4-rs / Relm4) — not recommended as the cross-platform shell

`relm4` 0.11.0, 2026-04-08. GTK 4.22.4 stable, 2026-04-30. On Linux with GNOME this produces the best
*integrated* result in the whole survey — High Tide and Amberol both look genuinely good, and Amberol
(GTK4 + Rust + GStreamer, latest 2026.1 on 2026-04-05) is the reference for "small beautiful GTK music
player".

Against, decisively: GTK's own documentation describes macOS and Windows as **partial support**, and
libadwaita is a GNOME visual API that "ignores the `gtk-theme` setting by design" and relies on
XDG portals for dark mode and accent colour — portals that do not exist on win32. The community
position is blunt: GNOME developers describe libadwaita as a library for the GNOME desktop, not for
Linux generally, let alone for Windows and macOS. Distributions have forked apps into GTK3 and GTK4
branches over exactly this (Manjaro's Pamac). Choosing GTK means shipping a first-class Linux app and
a visibly foreign Windows/macOS app.

#### 4.7 Python + GTK4/libadwaita (the High Tide stack) — not recommended

Verified stack: Python (ruff target `py311`), meson build, GTK4 + libadwaita, 22 Blueprint `.blp`
files compiled to `.ui`, GStreamer `playbin3` with `about-to-finish` gapless and a hot-swappable
`audio-sink` built by `Gst.parse_bin_from_description`, `libsecret` token storage, packaged as a
Flatpak on `org.gnome.Platform` 50.

Its Flatpak `finish-args` are instructive for the bit-perfect question:
`--socket=pulseaudio`, `--filesystem=xdg-run/pipewire-0:ro`. There is **no** `--device=all`, so raw
ALSA `hw:` access is not available inside that sandbox. High Tide offers `alsasink device=<dev>` in its
sink menu, but a Flatpak build cannot reach an exclusive `hw:` device without widening the sandbox.

Strengths: it is by far the smallest codebase in the survey (6,821 lines of Python) for a real,
Flathub-published TIDAL client, and Python is very agent-friendly. Weaknesses that kill it here:
Linux-only in practice, GIL and PyGObject threading make a rigorous audio state machine painful, no
mobile path, no static binary for a headless server deployment, and no type-checking backstop of the
kind Rust/TypeScript give an agent.

#### 4.8 Flutter/Dart + Rust core via flutter_rust_bridge — **recommended alternative if mobile is near-term**

Flutter draws every pixel with Skia/Impeller, so the design ceiling is high and identical on all six
targets. `flutter_rust_bridge` generates the Dart↔Rust binding; it is the mainstream way to run a
Rust core under a Flutter UI, and `simple_audio` is an existing Flutter audio package built on it.

For streamboat this means: `streamboat-core` and `streamboat-player` stay Rust; Flutter is the view
layer on desktop *and* mobile; audio output on mobile can use the Rust core with platform sinks, or
delegate to `just_audio`/ExoPlayer/AVPlayer where bit-perfect is not achievable anyway.

Against: three toolchains (Rust, Dart, plus per-platform native), Flutter's desktop plugin ecosystem
is thinner than its mobile one, and Flutter desktop apps rarely feel native. Dart is well represented
in training data but `flutter_rust_bridge` codegen conventions are not, so agents get the boundary
wrong more often than they get either side wrong.

#### 4.9 Kotlin Multiplatform + Compose Multiplatform — not recommended

Compose Multiplatform for iOS went **Stable** with 1.8.0 on 2025-05-06; 1.9.0 (2025-09) took Compose
for Web to Beta and improved iOS/desktop. Mobile-first, this is the strongest option in the list.

It fails on audio. Desktop Compose runs on the JVM, where `javax.sound.sampled` is the only built-in
path — no FLAC, no exclusive mode, no hw device selection. The KMP audio libraries that exist
(gadulka, AstroPlayer, kmp-audio-recorder-player, KorAU which explicitly uses `javax.sound.sampled` on
the JVM) are simple playback wrappers, not audiophile engines. Getting bit-perfect on desktop would
mean writing JNI to ALSA/WASAPI/CoreAudio — i.e. writing the hard part in C/Rust anyway, then paying
the JVM startup and memory cost on top. And a JVM headless daemon is a heavier server artefact than a
Rust binary.

#### 4.10 Electron + TypeScript — not recommended

tidal-hifi is the honest version of this stack, and its design is instructive precisely because it
*wraps* TIDAL's web player rather than implementing a client. Verified from `ref:tidal-hifi/package.json`:
`"electron": "github:castlabs/electron-releases#v43.0.0+wvcus"` — a castLabs Electron build with the
Widevine CDM, because TIDAL's web player is DRM-protected. Audio therefore goes through Chromium and
inherits Chromium's output path. tidal-hifi is 8,011 lines of TypeScript and MIT-licensed; it reads
playback state back out of the page through three fallback controllers (MediaSession API, Redux store,
DOM), which is exactly the fragility the wrapper approach buys.

If streamboat implements the TIDAL API itself (which the brief requires — the High Tide / SONE
approach), Widevine is not needed for the FLAC/BTS/DASH paths, so the castLabs build's one advantage
disappears. What remains is: a 120-200 MB bundle, a Chromium process tree, and the need to write a
native Node addon in C++/Rust for the audio path anyway. At that point Tauri does the same job with a
Rust core you were going to write regardless. Feishin — Electron + React 19 + Mantine + Zustand,
v1.15.1 on 2026-07-19 — solves this by shelling out to mpv, which is a reasonable design and is
available to any stack.

#### 4.11 Qt/QML — not recommended, but respectable

Strawberry is the proof: C++17 with Qt 6 (`QT_MIN_VERSION 6.8.0` on Windows/macOS, `6.4.0` elsewhere,
`ref:strawberry/CMakeLists.txt:228-232`), GStreamer with `autoaudiosink` / `osxaudiosink` /
`directsoundsink` / `wasapisink`, bit-perfect on Linux via ALSA exclusive, GPL-3.0. **Correction
(fact-checked): its `src/` is ~166,100 lines of C++/headers, not 14,708** — 491 `.cpp` (121,981 lines)
+ 568 `.h` (44,141 lines) across 40 subdirectories; `src/core` alone is 21,510 lines. Strawberry is the
**largest** codebase in this entire survey, roughly 3.5x SONE's Rust+TS combined — read its "works on
all three desktop OSes with a coherent look" achievement against that scale, not against a solo-owner
MVP budget. Its own Windows audio-sink ranking is also useful counter-evidence to the sone-windows path:
`gststartup.cpp` demotes `wasapi2sink` to `GST_RANK_SECONDARY` and ranks `directsoundsink`
`GST_RANK_PRIMARY`, citing device-switching problems (`ref:strawberry/src/engine/gststartup.cpp:59-69`,
issue #1227) — see §2.2/§2.3.

Against for streamboat: C++ plus QML plus CMake plus the Qt deployment story is the largest total
surface of any candidate; Qt's LGPL-3.0 licensing requires dynamic linking (or a commercial licence)
which complicates single-file distribution; and while `cxx-qt` 0.10.0 (2026-08-24, MIT/Apache, KDAB)
makes Rust↔Qt interop genuinely nice, it adds a fourth moving part. Agents write competent Qt Widgets
and mediocre QML.

#### 4.12 .NET + Avalonia — not recommended

Avalonia 12 exists as of 2026 with claimed 3× Android performance improvements (**web, unverified** —
same status this report already gives the Cider Tauri-migration rumour; avaloniaui.net was not directly
readable when fact-checked), a native dispatcher, page-based navigation and large rendering gains. Like
Slint and Flutter it draws every control itself,
so there is a real design ceiling and no native feel anywhere. The blocker is the same as KMP's:
no managed-runtime path to bit-perfect audio. NAudio is Windows-only; cross-platform .NET audio means
P/Invoke to the same three C APIs. Adding a .NET runtime dependency to a headless server binary is a
regression versus a static Rust binary.

#### 4.13 Swift/SwiftUI — disqualified by the platform requirement

Best-looking option on macOS and the only first-class iOS UI, and TidalSwift proves a Swift TIDAL
client works (AVPlayer/AVQueuePlayer, device OAuth, SwiftTagger metadata). But there is no Linux or
Windows story. Keep it in the back pocket as the *iOS* UI over a UniFFI-bound Rust core; never as the
primary shell.

#### 4.14 Go + Wails / Go + Fyne — not recommended

- **Wails v3** is still beta as of 2026 — **correction (fact-checked): the latest prerelease is
  `v3.0.0-beta.17` (2026-09-06), not `beta.9`** (the report was eight betas stale; re-check before
  relying on this section). Every beta since at least beta.8 carries the note "This is pre-release
  software. The API is stable, but you may still encounter issues before the final 3.0 release," so
  "still beta, desktop API described as stable" holds even though the version number does not. Same
  webview-per-OS model as Tauri, with a smaller ecosystem and the same WebKitGTK font-weight bug.
- **Fyne 2.8** (2026-07-13) is real and improving — 1,000+ commits, 39 contributors, GPU shapes,
  Wayland auto-selected on Linux, Go 1.22 minimum. But it is Material-Design-by-fiat with documented
  UX friction (underlined multiline text fields to indicate focus is a common complaint), so the
  "beautiful" bar is hard to clear.
- **Go's real strength here is the daemon**, and tidalt demonstrates it: 13,078 lines of Go, a clean
  server/client split over D-Bus, `docker/secrets-engine` keychain storage with an age-encrypted file
  fallback, FFmpeg via cgo for decode and a 111-line `alsa.c` for output. Go's audio ecosystem is
  cgo-to-C either way, which removes Go's main advantage (no C toolchain) exactly where it matters.

### 5. Scoring matrix

**Weights** (sum 100), justified against the brief:

| Criterion | Weight | Why this weight |
|---|---:|---|
| Audio pipeline & bit-perfect capability | 22 | The differentiating feature versus the official client and versus every webview wrapper. A stack that cannot do it is not a candidate. |
| Look-and-feel ceiling ("beautiful") | 18 | Owner's explicit ask, and the reason a user picks this over `mpd` + a TUI. |
| AI-coding-agent friendliness | 15 | Single largest determinant of actual throughput given the stated build method. |
| Effort to MVP and to maintain (solo) | 15 | Second-largest throughput determinant; also the main project-death risk. |
| Cross-platform desktop coverage (real parity) | 12 | Hard requirement, all three OSes now. |
| Headless/daemon fit and core sharing | 8 | Hard requirement now, but the shape is well understood in every candidate. |
| Mobile path | 5 | Explicitly future; must not be precluded, need not be cheap. |
| Packaging & distribution | 5 | Solvable everywhere; differs in effort, not in possibility. |

Scores are 1-5, then weighted (`score/5 × weight`).

| Stack | Audio 22 | Beauty 18 | Agents 15 | Effort 15 | X-plat 12 | Headless 8 | Mobile 5 | Pkg 5 | **Total** |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **Rust core + Tauri 2 + React/TS** | 5 (22.0) | 4.5 (16.2) | 5 (15.0) | 4 (12.0) | 4 (9.6) | 4 (6.4) | 4 (4.0) | 5 (5.0) | **90.2** |
| **Rust core + Flutter (flutter_rust_bridge)** | 4.5 (19.8) | 4.5 (16.2) | 4 (12.0) | 3 (9.0) | 4 (9.6) | 4.5 (7.2) | 5 (5.0) | 3.5 (3.5) | **82.3** |
| Go + Wails v3 (Rust-free) | 3.5 (15.4) | 4.5 (16.2) | 4.5 (13.5) | 4.5 (13.5) | 4 (9.6) | 5 (8.0) | 1 (1.0) | 3.5 (3.5) | **80.7** |
| **Rust core + Slint** | 5 (22.0) | 3.5 (12.6) | 2.5 (7.5) | 3.5 (10.5) | 4.5 (10.8) | 5 (8.0) | 3 (3.0) | 4 (4.0) | **78.4** |
| Rust core + egui | 5 (22.0) | 2 (7.2) | 3.5 (10.5) | 4.5 (13.5) | 4.5 (10.8) | 5 (8.0) | 1.5 (1.5) | 3.5 (3.5) | **77.0** |
| Electron + TS (+ mpv or native addon) | 2 (8.8) | 5 (18.0) | 5 (15.0) | 4.5 (13.5) | 5 (12.0) | 2.5 (4.0) | 1 (1.0) | 4.5 (4.5) | **76.8** |
| Qt 6 / QML (C++ or cxx-qt) | 5 (22.0) | 4 (14.4) | 3 (9.0) | 2.5 (7.5) | 4.5 (10.8) | 4 (6.4) | 3 (3.0) | 3 (3.0) | **76.1** |
| Go + Fyne | 3.5 (15.4) | 2 (7.2) | 4 (12.0) | 4.5 (13.5) | 4.5 (10.8) | 5 (8.0) | 3 (3.0) | 4 (4.0) | **73.9** |
| .NET + Avalonia | 2.5 (11.0) | 4 (14.4) | 3.5 (10.5) | 3.5 (10.5) | 4.5 (10.8) | 4 (6.4) | 4 (4.0) | 3.5 (3.5) | **71.1** |
| Rust core + iced | 5 (22.0) | 3 (10.8) | 2 (6.0) | 3 (9.0) | 4 (9.6) | 5 (8.0) | 1.5 (1.5) | 3.5 (3.5) | **70.4** |
| Rust + GTK4 / Relm4 | 5 (22.0) | 2.5 (9.0) | 3 (9.0) | 3.5 (10.5) | 2 (4.8) | 5 (8.0) | 1 (1.0) | 3.5 (3.5) | **67.8** |
| Kotlin MP + Compose MP | 2 (8.8) | 4.5 (16.2) | 4 (12.0) | 2.5 (7.5) | 3.5 (8.4) | 3.5 (5.6) | 5 (5.0) | 3 (3.0) | **66.5** |
| Rust + GPUI | 5 (22.0) | 4.5 (16.2) | 1.5 (4.5) | 1.5 (4.5) | 3 (7.2) | 5 (8.0) | 1 (1.0) | 3 (3.0) | **66.4** |
| Python + GTK4/libadwaita | 3.5 (15.4) | 2.5 (9.0) | 4 (12.0) | 5 (15.0) | 1.5 (3.6) | 3.5 (5.6) | 1 (1.0) | 2.5 (2.5) | **64.1** |
| Swift/SwiftUI | — | — | — | — | 1 | — | — | — | **disqualified** (no Linux/Windows) |

Scores are judgements, not measurements. The two that would most change the ranking if the owner
disagrees: "Beauty" for Slint/Avalonia/Flutter (custom-drawn UIs can be gorgeous — the 3.5-4.5 range
reflects iteration cost, not achievable quality), and "Agents" for Slint/iced (this penalises DSL
scarcity, which will improve over time).

### 6. What real music players in each stack look like

| Project | Stack | What it looks like / notable | Source |
|---|---|---|---|
| **SONE** | Tauri 2 + React 19 + Tailwind 4 + Jotai; Rust/GStreamer audio | Dark, Spotify-shaped: left sidebar, card grids, bottom player bar, now-playing drawer, full-screen player, floating resizable miniplayer, 15 theme presets + full colour picker, synced lyrics, signal-path panel with a "PRISTINE" verdict badge. Screenshots in `ref:sone/data/sone_homepage_readme.png`, `sone_drawer_readme.png`, `sone_theme_readme.png`; repo https://github.com/lullabyX/sone | verified from checkout |
| **sone-windows** | Same, plus `wasapi2sink` + `souvlaki` | Identical UI; separate fork, AI-assisted port | `ref:sone-windows/README.md` |
| **High Tide** | Python + GTK4 + libadwaita + Blueprint | GNOME HIG: `AdwNavigationView`-style pages, carousels, sidebar, `AdwToolbarView`. Screenshots at `ref:high-tide/data/resources/screenshot 1.png`, `screenshot 2.png`; https://flathub.org/apps/io.github.nokse22.high-tide | verified from checkout |
| **Amberol** | Rust + GTK4 + GStreamer | The reference for "small beautiful GTK music player": album-art-derived adaptive background, no library management by design. 2026.1 released 2026-04-05. https://apps.gnome.org/Amberol/ | web |
| **Decibels** | GJS + TypeScript + GTK4 + libadwaita | GNOME audio player with a waveform view | web |
| **Psst** | Rust + druid | Native-widget Spotify client, deliberately plain; author cites druid's reactive architecture as the source of most UI complexity, with Xilem as the intended successor. As of Feb 2026 requires the user's own Spotify Developer Client ID for Web API features | web |
| **Spot** | Rust + GTK4 | GNOME-styled Spotify client, same visual family as High Tide | web |
| **Supersonic** | Go + Fyne 2.8; **libmpv** for audio | Dark/light built-in themes, dense grid+list layout; ReplayGain, 15-band EQ, gapless, "optional audio exclusive mode". v0.22.0. AppImage bundles mpv; Linux needs `libmpv1`/`libmpv2`. Flatpak build "does not support CJK fonts as the sandboxing breaks font lookup". https://github.com/dweymouth/supersonic | web |
| **Feishin** | Electron + React 19 + Mantine + Zustand + electron-vite; **mpv** for audio | Full Spotify-alike for Navidrome/Jellyfin/Subsonic; synced lyrics, smart playlist editor. v1.15.1, 2026-07-19, ~9k stars. Bit-perfect requires `--audio-device=alsa/…`, `--gapless-audio=no`, exclusive mode; Flathub build uses PipeWire and won't do it | web |
| **tidal-hifi** | Electron (castLabs `v43.0.0+wvcus`) wrapping TIDAL's web player | Looks exactly like TIDAL web, because it *is* TIDAL web. Adds MPRIS, an Express HTTP API with Swagger, themes | `ref:tidal-hifi/package.json` |
| **tidalt** | Go + BubbleTea + Lipgloss TUI; FFmpeg + ALSA via cgo | Terminal UI with progress bar and text input; also `tidalt daemon` headless + MPRIS2. Screenshot at `ref:tidalt/docs/tui.png` | verified from checkout |
| **Strawberry** | C++17 + Qt 6.4/6.8 + GStreamer | Classic desktop three-pane collection manager; audiophile focus incl. DSD for local files | `ref:strawberry/` |
| **ncspot** | Rust TUI (cursive) | Terminal Spotify client; the "TUI is enough" data point | web |
| **Cider** | Electron + Vue.js, with Rust components | Apple Music client; visually the most "designed" of the Electron music clients | web (Tauri migration **not** verified) |
| **Nuclear** | Electron + React | Community music player; proof that Electron ships fast and looks fine | web |
| **Museeks** | **Correction (fact-checked): Tauri 2 + Rust + React, not Electron.** Museeks ported from Electron to Tauri (announced 2024; its README states "Back-end: Tauri v2 / Rust", "UI: React.js") | A **second** shipped Tauri 2 + React music player besides SONE — and the useful *negative* control for this report's argument: Museeks' `src-tauri/Cargo.toml` pins `tauri` 2.11.2 but has **no audio crate at all** (`lofty` 0.23.3 for tags, `m3u`, `axum` 0.8.9, `tokio` 1.52.3, `sqlx` 0.8.6) — playback stays in the webview, so it cannot be bit-perfect. That is exactly the split streamboat must not fall into (webview UI: yes; webview *player*: no). Its `sqlx`+`axum` choices are also independent evidence for the SQLite/local-HTTP sub-decisions in Recommendation 1. | https://github.com/martpie/museeks, `src-tauri/Cargo.toml` |

### 7. Packaging, per platform

| Target | Path | Notes / gotchas |
|---|---|---|
| **Flathub** | `flatpak-builder` manifest, `org.gnome.Platform` (High Tide uses 50) or `org.freedesktop.Platform` | Exclusive ALSA needs a wider sandbox than High Tide's (`--socket=pulseaudio`, `--filesystem=xdg-run/pipewire-0:ro`); plan for `--device=all`, which reviewers question. **AI policy applies — see §9.** Trademark rule: an app "cannot mention `Firefox` in its name or use any of the official icon, logo or artwork" — the same logic applies to TIDAL's marks. |
| **Snap** | `snapcraft.yaml`, `base: core24`, `confinement: strict` | SONE's plugs include `audio-playback` and `alsa`; `alsa` is a manual-connect interface, hence its README's `sudo snap connect sone:alsa`. SONE also has to override the gnome extension's WebKit bind path ("it targets gnome-46-2404, which ships no WebKit -> blank window") — a Tauri-on-Snap-specific trap. |
| **AUR** | PKGBUILD | SONE ships both `sone` (from source) and `sone-bin`. Cheap and expected by Arch users. |
| **deb / rpm** | `tauri build` emits both; SONE additionally builds them in Docker per-distro (`build-scripts/build/{deb,rpm,pacman}.sh`) and hosts an apt/dnf/zypper repo on Cloudsmith | SONE's deb `depends` list is the real dependency footprint of this design: `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1`, `libgstreamer1.0-0`, `gstreamer1.0-plugins-{base,good,bad}`, `gstreamer1.0-libav`, `gstreamer1.0-alsa`, `libsecret-1-0`, `libasound2 \| libasound2t64`, `librsvg2-common`, `pulseaudio-utils`. |
| **AppImage** | `tauri build` | SONE sets `appimage.bundleMediaFramework: true` to pull GStreamer in. |
| **Nix** | flake | Both SONE and High Tide ship `flake.nix`; `nix run github:owner/repo` is a zero-install demo path. |
| **Windows** | `msi` (WiX) and `nsis` from `tauri build`; winget manifest on top | If GStreamer is the engine, you must ship it: GStreamer documents packing its MSI and running it silently via `msiexec` with `INSTALLDIR`, or using Merge Modules. Its docs warn that because plugins load on demand, "if you don't know in advance what files you'll play, you don't know which DLLs you need to deploy" — so trimming is risky. MSIX/Store adds a further sandbox that makes WASAPI exclusive harder. |
| **macOS** | `app` + `dmg`; Developer ID Application cert; notarization via App Store Connect API key or Apple ID, both supported by the Tauri bundler | Hardened Runtime plus a microphone/audio entitlement review is not needed for playback-only, but CoreAudio hog mode inside an App Sandbox is a known problem — plan for Developer ID + notarized DMG, **not** Mac App Store. Homebrew cask is the practical install path. |
| **iOS** | Not App Store — see §9. TestFlight (90-day builds, still reviewed), ad-hoc/enterprise, or EU alternative marketplaces. | |
| **Android** | Sideload APK + F-Droid + (maybe) Play. Play's policy surface for unofficial clients is less absolute than Apple's but still cites third-party ToS. | |

### 8. AI-coding-agent friendliness, concretely

Ranked by expected agent-produced-correct-code rate, with the reasoning:

1. **TypeScript/React/Tailwind** — largest corpus by a wide margin; `tsc` + ESLint + Prettier catch a
   large class of errors mechanically; component boundaries are small enough to regenerate.
2. **Rust** — smaller corpus than TS but the compiler is the strongest available reviewer, and
   `cargo clippy -- -D warnings` plus `cargo fmt --check` gives an agent a hard, fast feedback loop.
   Two reference projects were built this way (tidalt, sone-windows).
3. **Python** — huge corpus, no type backstop by default. Fine for scripts, risky for a threaded
   GStreamer state machine.
4. **Go** — good corpus, simple language, strong tooling; the audio work still lands in cgo/C.
5. **Dart/Flutter** — good corpus for widgets, poor for `flutter_rust_bridge` boundary code.
6. **Kotlin/Compose** — good corpus for Android Compose, thinner for Compose Desktop.
7. **C++/Qt/QML** — good corpus for Qt Widgets, weaker for modern QML idioms; the build system is a
   frequent agent failure point.
8. **`.slint`, iced, GPUI DSLs/APIs** — thin corpora, fast API churn. Agents produce confident,
   compiling-but-wrong layouts.

Testing tooling per stack, since agents need a test to aim at:

- Rust: `cargo test`, `wiremock` for HTTP, `insta` for snapshot assertions on parsed manifests,
  `criterion` if needed. SONE keeps `tempfile` as its only dev-dependency, which is thinner than ideal.
- Frontend: Vitest + `@testing-library/react` + jsdom is exactly SONE's setup (`vitest` 4.1.7,
  `@testing-library/react` 16.3.2, `jsdom` 29.1.1), with `knip` for dead-code detection. SONE has
  test files colocated with components (`Header.test.tsx`, `PlayerBar.test.tsx`,
  `NowPlayingDrawer.escape.test.tsx`, `MediaCard.explicit.test.tsx`, `HomeSection.compactGrid.test.tsx`).
- E2E: `tauri-driver` works on **Windows and Linux only** — "macOS not having a WKWebView driver tool
  available". WebdriverIO's Tauri service covers macOS by running an embedded WebDriver server inside
  the app. Plan e2e on Linux CI and treat macOS as manual.

### 9. Licensing and policy constraints that bind the stack choice

- **Reference clients are copyleft.** SONE and sone-windows: `GPL-3.0-only`. High Tide: GPL-3.0.
  Strawberry: GPL-3.0. python-tidal: LGPL-3.0-or-later. mopidy-tidal: Apache-2.0. tidal-hifi: MIT.
  tidalt: Apache-2.0. tidalrs: MIT. libopenTIDAL: MIT. TidaLuna: MS-PL. TIDAL's own SDKs: Apache-2.0.
  If streamboat reuses *code* from SONE or High Tide it inherits GPL-3.0. If it only reads them for
  API knowledge, it does not — but the safe and community-consistent choice is GPL-3.0.
- **Toolkit licences**: Tauri MIT/Apache-2.0; React/Tailwind MIT; iced MIT; egui MIT/Apache;
  gpui Apache-2.0; relm4/gtk4-rs **Apache-2.0 OR MIT** (correction — crates.io's licence field for
  `relm4`, not plain MIT as an earlier draft said); cxx-qt MIT/Apache (but **Qt itself** is LGPL-3.0 or
  commercial —
  LGPL requires dynamic linking or object-file relinking); Slint tri-licensed
  (`GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`) — the
  royalty-free arm requires visible attribution, the GPL arm does not and suits an open-source project;
  symphonia MPL-2.0; cpal Apache-2.0; rodio MIT/Apache; gstreamer bindings MIT/Apache with the
  underlying GStreamer core LGPL-2.1 and some plugin sets GPL; souvlaki MIT; uniffi MPL-2.0;
  wasapi MIT.
- **Flathub AI policy — the most consequential 2026 change for this project.** Sequence of facts:
  - **Unverified (fact-checked as "uncertain"):** press reporting from late May 2026 said Flathub would
    "ban nearly all apps and submissions made with generative AI", effective 2026-05-29, covering
    application code, BaseApps, extensions, build scripts, manifests, metadata, documentation and PR
    text, non-retroactive, with exceptions possible for mature well-maintained projects. The source
    domains (gamingonlinux.com, linuxiac.com, opensourceforu.com) were not reachable in this
    environment either time this report was checked — the 2026-05-29 effective date and the
    non-retroactivity claim rest on secondary reporting, not a primary document read directly.
  - The primary artefact — directly read, high confidence — supports the tightening: commit
    `flathub-infra/documentation@992f57b` ("Reword LLM policy to make it clear it's not allowed") in
    `docs/02-for-app-authors/02-requirements.md` reads "Applications containing AI-generated or
    AI-assisted code, documentation, or other content are not allowed" with "Exceptions may be granted
    for mature, well-maintained projects." (Its exact commit date was not independently visible when
    fact-checked; "May 2026" is inferred from the press timeline above, not confirmed from the commit
    itself.)
  - **The live document, read 2026-09-07, is a disclosure regime rather than a ban.** Verbatim:
    "Submitters must disclose any AI-generated code, documentation, packaging, or other material they
    know or reasonably believe is included in the application or its Flathub packaging."
    "AI used only for research, discussion, or debugging does not need disclosure when no generated
    material is included in the application or its Flathub packaging."
    "Disclosed AI-generated material is evaluated at reviewer discretion."
    "**Disclosure does not create a presumption of acceptance.**" (the sentence immediately following
    the one above — an earlier draft of this report omitted it; it is the one that makes "Flathub
    remains reachable" a conclusion the evidence supports, rather than an optimistic reading of the
    reviewer-discretion sentence alone.)
    "Reviewers may reject a submission, including without further review, based on the extent or role
    of generated material or concerns about its review, quality, or maintainability."
    "AI tools or agents must not open or automate Flathub submission pull requests, or generate their
    commit messages, descriptions, review comments, or replies."
    "Submitters must not request AI-agent reviews."
    "Undisclosed or materially misrepresented AI-generated material… may result in rejection."
    "Repeated violations may result in a permanent ban from future submissions and activities."
  - **Operational consequence for streamboat**: Flathub remains reachable, but (a) the submission PR
    must be opened, described and discussed by the human owner, never by an agent; (b) AI involvement
    must be disclosed; (c) acceptance is discretionary. Do not make Flathub the *only* Linux
    distribution channel. AUR + deb/rpm repo + AppImage + Snap + Nix (all of which SONE ships) are
    unaffected by this policy.
- **Apple App Store 5.2.2** (verbatim): "If your app uses, accesses, monetizes access to, or displays
  content from a third-party service, ensure that you are specifically permitted to do so under the
  service's terms of use. Authorization must be provided upon request." Also 5.2.1: "Don't use
  protected third-party material such as trademarks, copyrighted works, or patented ideas in your app
  without permission…". An unofficial TIDAL client that uses the undocumented API cannot satisfy 5.2.2
  on request. **Treat the iOS App Store as closed**; this does not affect the technical stack choice
  but it does mean "mobile" should be scoped as Android-first with iOS via TestFlight/sideload.
- Note also 2.5.6 ("Apps that browse the web must use the appropriate WebKit framework"), which is why
  a Tauri iOS build is fine (WKWebView) but a bundled Chromium is not.

### 10. Mobile path, concretely

Ranked by how much of the desktop investment survives:

1. **Rust core + Tauri mobile.** The `streamboat-core` and `streamboat-proto` crates compile for
   `aarch64-linux-android` and `aarch64-apple-ios` unchanged. The React UI is reused with a responsive
   layout. What must be rewritten: the audio output (Tauri has no first-party audio plugin; you fork or
   write one wrapping Media3 ExoPlayer on Android and AVPlayer/AVAudioEngine on iOS), background
   playback service, lock-screen controls, and offline cache policy. Bit-perfect is not a mobile goal
   anyway (phones resample).
2. **Rust core + UniFFI + native UIs.** `streamboat-core` becomes an `.aar` and `.xcframework`;
   Android gets Compose, iOS gets SwiftUI, each with its own ExoPlayer/AVPlayer. Most work, best
   result, and the pattern the official TIDAL SDKs use (`tidal-sdk-android` player module is separate
   from `streaming-api`, which is separate from `auth`; `tidal-sdk-ios` likewise).
3. **Rust core + Flutter.** One UI codebase for desktop and mobile; audio delegated to a Flutter audio
   plugin on mobile and to the Rust engine on desktop.

Whichever is chosen, the invariant that protects all three is: **`streamboat-core` must not depend on
any UI toolkit, any windowing library, or any desktop-only OS API.** Enforce it with a CI job that
builds `streamboat-core` for `wasm32-unknown-unknown` or a bare `--no-default-features` profile.

### 11. What this report does not settle, and where the answer lives instead

Two brief items and several sibling-document cross-references were not fully worked through here:

- **Comparanda the brief named but this report did not reach**: Symphonium and named Flutter music
  apps (Harmony et al.) — no primary source was checked for either. The weakest evidence gap under this
  report's own mobile-path claim (§10) is that it cites production Tauri *desktop* apps (Spacedrive,
  AppFlowy, Clash Verge) but no example shipping on Android/iOS.
- **Accessibility per candidate** has no row anywhere above, though
  `engineering-baseline.md:1234,1253` tags both the screen-reader approach and the automated a11y gate
  **[STACK]** — deferred to this document, unanswered. For the recommended stack: ARIA plus the
  platform webview's accessibility tree (Tauri/Electron), versus AccessKit (egui/iced/Slint), AT-SPI
  (GTK), or UIA/NSAccessibility (Qt/Avalonia). Custom window chrome (see §4.1) means keyboard focus
  management and ARIA on the custom titlebar/player controls is real, uncalibrated work.
- **i18n mechanism** is likewise tagged **[STACK]** at `engineering-baseline.md:1403` and unanswered
  here: react-i18next/Fluent/ICU MessageFormat for the recommended stack vs. gettext for a GTK
  alternative.
- **Frontend framework within Recommendation 1** (React 19 vs. Svelte 5 vs. Solid) gets one open
  question (see below) and one scoring-matrix row; it deserves its own short evaluation of corpus size,
  bundle behaviour and state-management fit, since this report itself calls the choice irreversible
  after ~85 components.
- **The rest of `engineering-baseline.md`'s [STACK] checklist** (`:1390-1412`) that this document should
  close in one place: logging library (SONE: `flexi_logger` 0.31 + `log`; alternative: `tracing`), path
  resolution (`dirs` 5 vs. `directories`), crypto primitives (SONE: `aes-gcm` 0.10 + `zeroize` + `sha2`
  + `keyring` 3 with the `sync-secret-service` feature — that feature choice matters specifically for
  headless), fuzzing harness (`cargo-fuzz` for manifest/JSON parsers), monorepo tooling (cargo workspace
  + pnpm workspace — SONE has both, via `pnpm-workspace.yaml`), and redaction enforcement (whether a
  newtype can make a raw token unloggable, rather than relying on a reviewer to remember). Auto-update,
  Flatpak dependency generators, and the audio test harness are answered above (§3, §4.1); a11y and
  i18n are named just above as still open. When implementing, prefer this document's answer over
  `engineering-baseline.md`'s placeholder for any item both discuss, and update that document's table
  to point here rather than repeating the detail.

---

## Implications for streamboat

### Recommendation 1 (primary): Rust workspace + Tauri 2 desktop shell + headless daemon

**Score 90/100.** This is SONE's stack, chosen because SONE is the existence proof that it delivers
every hard requirement except macOS bit-perfect, and because it maximises agent throughput.

Sub-decisions this implies:

| Decision | Choice | Rationale |
|---|---|---|
| Language (core, player, daemon) | Rust 2021/2024, one Cargo workspace | Bit-perfect needs direct device access; the compiler is the agent's reviewer; a static daemon binary is the best server artefact |
| Desktop shell | Tauri 2 (`tauri` 2.11.x pinned exactly, as SONE does with `=2.11.2`) | Pinning avoids surprise webview behaviour changes mid-project |
| Frontend framework | React 19 + TypeScript 5.8 (strict) | Largest agent corpus. Svelte 5 is a defensible smaller-code alternative; Solid is not worth the corpus penalty |
| State | Jotai (SONE) or Zustand (Feishin) — pick one and never mix | Atom-per-concern maps well to a push-event model. SONE's `src/atoms/` split (auth, playback, playlists, favorites, navigation, theme, ui, video, updates, proxy, overlay, mcp) is a good template |
| Styling | Tailwind CSS 4 + CSS custom properties for themes (`--th-bg-base`, `--th-text-primary` as SONE does) | Themes become a token swap, not a rebuild. shadcn/ui + Radix is optional; SONE ships **no** component library, only Tailwind + `lucide-react`, and looks good |
| Icons | `lucide-react` | SONE's choice; consistent stroke weights (watch the WebKitGTK `stroke-width` bug) |
| Virtualisation | `@tanstack/react-virtual` | Mandatory: libraries have tens of thousands of rows |
| Video (music videos) | `hls.js` in the webview, separate from the audio path | SONE's split; keeps the lossless path clean |
| Audio engine | `gstreamer` 0.25 + `gstreamer-app`, behind an `AudioEngine` trait; libmpv as backend #2 | See §2.3 |
| Exclusive output | Linux: `alsa` 0.10 + appsink writer thread. Windows: `wasapi2sink exclusive=true`. macOS: unresolved — evaluate `coreaudio_exclusive` via libmpv | See §2 |
| Media controls | `mpris-server` 0.9 + `zbus` 5 on Linux (registered by the **engine**, so it works headless); `souvlaki` 0.8.3 on Windows/macOS | souvlaki needs an HWND on Windows and a run loop on macOS |
| Secrets | `keyring` 3 (Secret Service / Windows Credential Manager / macOS Keychain), with an age- or AES-GCM-encrypted file fallback | SONE uses `keyring` 3 + `aes-gcm` 0.10 + `zeroize`; tidalt uses `docker/secrets-engine` with an age fallback; High Tide uses libsecret directly |
| HTTP | `reqwest` (upgrade to 0.12+ rather than SONE's 0.11) with `rustls`, `socks` feature for proxy support | tidalrs uses `reqwest` 0.12 + rustls |
| Async | `tokio` | Everything else in the Rust audio/HTTP ecosystem assumes it |
| Local persistence | SQLite (`rusqlite` or `sqlx`) for catalog cache and queue state | mopidy-tidal's proxy cache uses SQLite; SONE has a 658-line `cache.rs` |
| Daemon protocol | JSON commands/events over WebSocket (`axum` 0.7, which SONE already uses for its MCP and OBS servers), token-gated | Music Assistant precedent; reuse the same `streamboat-proto` types on both sides |
| Packaging | Linux: deb + rpm + AppImage + AUR (`sone`/`sone-bin` pattern) + Snap + Nix flake + Flathub last; Windows: NSIS + MSI + winget; macOS: notarized DMG + Homebrew cask | See §7 |
| Testing | `cargo test` + `wiremock` + `insta` for manifest parsing; Vitest + Testing Library for components; `tauri-driver` e2e on Linux/Windows CI only | See §8 |
| CI | GitHub Actions matrix (ubuntu/windows/macos), plus a `--no-default-features` daemon-only build in a minimal container to prove headless independence | |

**Effort to MVP: 6-10 weeks** solo at ~20-25 h/week with heavy agent use, where MVP =
device-code login + token refresh + session/countryCode, browse home/album/artist/playlist,
search, play with quality cascade (HI_RES_LOSSLESS → HI_RES → LOSSLESS → HIGH), queue with
shuffle/repeat, MPRIS, settings, and a `.deb`. **To parity with the official client: 9-18 months.**
Calibration: SONE's `tidal_api.rs` alone is 7,200 lines and it reached v0.21.0 before claiming parity.

This estimate is calibrated unevenly and should be read that way, not as one uniform number: the
API-client and player crates are the sourced part (`streamboat-core`/`streamboat-player` a few
thousand lines each, against `ref:tidalrs/src/` at 3,545 lines for an API client alone and SONE's
`tidal_api.rs`+`audio.rs` at 10,509 lines combined); the **UI line count is not independently
calibrated** (SONE's 47,609 lines of TS/TSX across 199 files and 85 components is the only data point,
and it is a mature v0.21.0 app, not an MVP); and **the packaging tail is the item most often
underestimated** — it explicitly excludes design iteration, icon/branding, Apple Developer Program
enrollment and notarization setup, Windows code-signing setup, three separate release pipelines (no
cross-compilation — see §4.1), the Flathub submission process itself, accessibility work, i18n
infrastructure, and any macOS bit-perfect research spike. Re-derive this number per workstream before
committing to it as a schedule.

**Risks:**
- WebKitGTK divergence — mitigate by testing on Linux first and keeping a workaround log (SONE's CSS
  comment is the model).
- macOS bit-perfect unproven — mitigate by scoping macOS v1 to "system default output, correct sample
  rate passthrough" and treating exclusive mode as a later, hardware-verified feature.
- GStreamer on Windows/macOS packaging weight — mitigate by evaluating libmpv for those two platforms
  behind the same trait before committing to a Windows installer design.
- Two languages means two agent contexts. Mitigate with a strict, typed IPC surface: one
  `#[derive(Serialize, Deserialize)]` `Command`/`Event` pair, generated TS types (`ts-rs` or
  `specta`/`tauri-specta`), never hand-written duplicates.

### Recommendation 2 (alternative): Rust workspace + Slint desktop shell

**Score 78/100.** Choose this if the owner's priority is **one language, one toolchain, no webview,
no CSS**. Everything below `desktop/` changes; `crates/` is identical.

Sub-decisions: Slint 1.17+ under the GPL-3.0 arm (no attribution obligation, matches the project's
likely licence); Skia or femtovg renderer; `.slint` component library hand-built; theming via Slint
global properties; same audio, secrets, MPRIS, packaging, testing choices as Recommendation 1 minus
everything web.

**Effort to MVP: 8-14 weeks.** The delta over Recommendation 1 is almost entirely UI iteration speed.

**Risks:** agent output quality in `.slint` is the main one; design iteration is slower without CSS;
the mobile story is weaker than Tauri's or Flutter's.

### Recommendation 3 (alternative): Rust core + Flutter via flutter_rust_bridge

**Score 82/100.** Choose this if the owner decides mobile is a **2026-2027** target rather than a
someday target, and is willing to pay a slower desktop MVP for a much cheaper mobile one.

Sub-decisions: `flutter_rust_bridge` for the boundary; Riverpod or Bloc for state; Material 3 with a
heavily customised theme (Flutter's default Material look is the "beautiful" risk); `media_kit`
(libmpv-backed) or the Rust engine over FFI for desktop audio; `just_audio`/ExoPlayer/AVPlayer on
mobile; same Rust core, daemon and packaging story.

**Effort to MVP: 10-16 weeks** desktop; then roughly +4-6 weeks per mobile platform.

**Risks:** three toolchains; Flutter desktop plugins are thinner than mobile ones; agents are weakest
exactly at the FFI boundary; desktop Flutter apps are recognisable as such.

### Honest "why not" for everything else

- **Electron** — best beauty and agent scores, worst audio story. You would end up shelling out to mpv
  (Feishin) or writing a native addon, at which point you have Tauri's architecture with 20× the bundle
  and no Rust core to reuse on mobile. Only choose it if the owner reverses the "implement the API
  ourselves" decision and wraps TIDAL web, which then also requires castLabs Widevine.
- **GTK4 (Rust or Python)** — produces the nicest Linux app in the survey and a foreign-looking app
  everywhere else, against an explicit three-OS requirement. Revisit only if the owner narrows scope
  to Linux.
- **Qt/QML** — technically capable of everything, including bit-perfect on all three OSes, and
  Strawberry proves it. Rejected on total surface area (C++ + QML + CMake + Qt deployment + LGPL
  linking rules) for a solo developer, and on agent reliability in QML.
- **egui** — cannot clear the "beautiful" bar. Keep it for a developer-facing signal-path/debug window.
- **iced** — good engineering, wrong ergonomics for a many-async-loads media app, and an API that has
  churned enough that agent output frequently targets dead versions.
- **GPUI** — the most attractive future option in Rust and the least attractive present one: pre-1.0,
  breaking changes, thin standalone documentation.
- **Kotlin MP / Compose MP** — best mobile answer, no path to desktop bit-perfect without writing the
  hard part in native code anyway; heavy headless artefact.
- **.NET/Avalonia** — same audio objection as KMP, without KMP's mobile advantage over Tauri.
- **SwiftUI** — disqualified for the primary shell; the right choice for a future iOS UI over a
  UniFFI-bound core.
- **Go + Wails / Fyne** — Wails v3 is still beta; Fyne's Material look fights the "beautiful"
  requirement; Go's audio path is cgo either way. Go's genuine win — a clean daemon — is available in
  Rust too, and tidalt's own D-Bus server/client design is worth copying regardless of language.
- **Xilem** — alpha ("Lots of things need improvements," per its own README, not "plenty of missing
  features" as an earlier draft of this report paraphrased it), major breaking changes expected.
  Masonry's current release is 0.4.0 (2025-10-29), not an older 0.2.0. Not a candidate in 2026.

### Cross-cutting engineering decisions this research surfaces

1. **Put an `AudioEngine` trait in front of the engine on day one.** GStreamer vs libmpv vs
   per-OS-hand-rolled is the highest-variance decision in the project and the one most likely to be
   revisited on macOS.
2. **Register MPRIS from the engine, not from the GUI**, so `playerctl` and media keys work in daemon
   mode (tidalt calls this out explicitly).
3. **Make gapless and bit-perfect mutually exclusive by construction**, mirroring SONE's
   Normal-vs-DirectAlsa split, and say so in the UI.
4. **Do not persist stream manifests across restarts.** High Tide caches them in memory per session,
   resetting `self.manifest` per track with no TTL (`ref:high-tide/src/lib/player_object.py:140,457-491`).
   **Correction (fact-checked, "uncertain"): the specific "~1 hour manifest / ~24 hour segment URL"
   lifetimes in an earlier draft of this line are not supported by anything in the checkouts** — no
   expiry constant appears in `ref:python-tidal/tidalapi/media.py` or in High Tide's player object.
   State the rule without invented numbers: manifests and segment URLs are short-lived and must not be
   persisted across restarts; measure actual TIDAL expiry against a live account before hard-coding any
   duration.
5. **Build a signal-path panel early.** It is SONE's most distinctive feature (probing GStreamer,
   `pactl` and `/proc/asound`, with a PRISTINE verdict), it is cheap once the engine is trait-shaped,
   and it is the feature that makes the bit-perfect claim credible.
6. **Generate TypeScript types from Rust** rather than hand-maintaining them. SONE's `src/types.ts` is
   870 hand-written lines; that is 870 lines of drift risk.
7. **Ship a `--version`-stable JSON protocol** before the second client exists, not after.
8. **Never let an AI agent open the Flathub submission PR.** That specific act is prohibited by the
   live policy, independently of how the code was written.

---

## Open questions

Only the owner can decide these:

1. **Licence.** GPL-3.0 (matches SONE, High Tide, Strawberry; forces any fork open) vs Apache-2.0/MIT
   (matches tidalt, mopidy-tidal, TIDAL's own SDKs; permits proprietary forks). This also determines
   whether Slint's GPL arm or its royalty-free arm applies, and whether SONE/High Tide code can be
   adapted rather than merely read.
2. **Is macOS bit-perfect a v1 requirement or a v2 aspiration?** No reference project in the survey
   achieves it. If v1, budget for CoreAudio hog-mode work and a Mac with a DAC to verify on.
3. **How much does Flathub matter?** If it is a must-have, the AI-disclosure regime shapes the
   contribution workflow (human-authored PRs, disclosure text, discretionary review). If AUR + deb/rpm
   repo + AppImage + Snap + Nix is enough, the constraint largely disappears.
4. **Is mobile 2027-or-later, or 2026?** If 2026, Recommendation 3 (Flutter) or a UniFFI plan changes
   the desktop architecture now. If later, Recommendation 1 stays optimal.
5. **Frontend framework within Recommendation 1**: React 19 (max agent reliability, more boilerplate)
   vs Svelte 5 (less code, smaller corpus). This is reversible for ~2-3 weeks of work early and
   irreversible after ~85 components.
6. **GStreamer everywhere vs libmpv on Windows/macOS.** Trades a large packaging burden and a macOS
   unknown against a second engine implementation and mpv's licence/dependency footprint.
7. **Does streamboat want the SONE-style extras** (MCP server on a local port, OBS overlay, Discord
   presence, Last.fm/ListenBrainz scrobbling)? They are ~2,500 lines of Rust in SONE and they shape the
   daemon's HTTP surface.
8. **Offline caching for a logged-in subscriber**: in scope or out? It changes the storage design
   (encrypted-at-rest cache, eviction, manifest re-fetch) and the legal posture.
9. **Is v1 all three desktop OSes at once, or Linux-first with Windows/macOS best-effort?** The closest
   precedent (SONE) shipped Linux first and got a separate, self-described "may not be actively or
   correctly maintained" Windows fork rather than day-one parity; the stack cannot cross-compile
   (§4.1), so this is a staffing/sequencing decision, not a config flag.
10. **Does `streamboat-daemon` serve a browser UI / network remote, or is remote control MPRIS-bridge
    only?** This decides whether the React app may call `invoke` directly or must go through a
    `Transport` interface picked at bootstrap from day one — retrofitting that boundary later is
    expensive. Prior art on both sides: Music Assistant serves a web UI over its WebSocket API; SONE
    already runs `axum` 0.7 in-process for its MCP/OBS servers, so adding a browser UI is a small delta;
    tidalt deliberately has none and tells users to bridge MPRIS to a phone via KDE Connect/GSConnect
    instead, since "MPRIS2 is a local D-Bus protocol — it does not have a network transport."
11. **Native window decorations, or SONE's custom-chrome route?** See §4.1 — custom chrome is real,
    recurring per-OS work (drag regions, resize handles, traffic lights, Snap Layouts), not a one-time
    cost.
12. **Enforce a CSP, or copy SONE's `security.csp: null`?** See §4.1 — a music client renders remote
    artwork, lyrics and user playlist/track names in a webview holding `invoke`.
13. **Windows WebView2 install mode** — `downloadBootstrapper` (0 MB, needs network) through
    `fixedVersion` (+~180 MB, freezes the webview build). See §4.1.
14. **Update channel per platform**, given the Tauri updater plugin covers only AppImage/`.app.tar.gz`/
    MSI+NSIS — not deb/rpm (needs a hosted repo) and not Snap/Flatpak (store-managed). See §4.1.
15. **Minimum OS versions**: macOS floor (Tauri bundler default 10.13; SONE's practical Linux floor is
    whatever WebKitGTK its build container ships — Ubuntu 22.04 in SONE's case), Windows 10 April-2018-
    update-or-later for a preinstalled WebView2. These constrain CI runner and container choices
    immediately, not just release notes.
16. **Music-video scope.** The recommended design plays video with `hls.js` inside the webview, which
    cannot handle Widevine-protected streams — SONE ships no CDM of any kind (unlike tidal-hifi's
    castLabs Electron build). So video is either limited to non-DRM HLS or dropped from scope; decide
    which before designing the video player UI.
17. **Hardware and account prerequisites.** A Mac and at least one USB DAC are needed to verify the
    macOS and bit-perfect claims in this report, on top of the Apple Developer Program and Windows
    signing costs `engineering-baseline.md` already prices.

Unverified or low-confidence in this report — flagged so downstream agents do not treat them as fact:

- Tauri-vs-Electron RAM and binary-size numbers circulating for 2026 (75% less RAM, 20-50× smaller
  bundles, 3.7× faster startup) come from SEO-style aggregator blogs, not from a reproducible
  benchmark. Direct measurement of SONE vs tidal-hifi release artefacts was **not possible** in this
  environment (GitHub API and Flathub API are blocked). Treat the direction as right and the
  magnitudes as unverified.
- Cider's rumoured Electron→Tauri migration is **not confirmed**; the sources found describe Cider as
  Electron + Vue.js "utilizing localized Vue.js and Rust".
- The exact current Flathub AI wording may change again; the May 2026 commit and the September 2026
  live document disagree, and this report quotes both.
- macOS `coreaudio_exclusive` bit-perfect behaviour is documented by mpv but **not verified on
  hardware** by any source found, and one user report in the Feishin discussion describes it
  misrouting between DAC and HDMI.
- Whether cpal issue #459 was closed as "won't do", "duplicate", or "done elsewhere" could not be read
  from the fetched page; the changelog evidence (no exclusive mode, added resampling) is the stronger
  signal and is what this report relies on.
- Effort estimates are calibrated against reference-project line counts, not against the owner's
  actual velocity, and the packaging tail is the most commonly underestimated part — see the effort
  breakdown under Recommendation 1.
- **Kotlin Multiplatform desktop audio library survey (§4.9)** — the claim that KMP has no mature
  bit-perfect option and that gadulka/AstroPlayer/kmp-audio-recorder-player/KorAU are simple wrappers
  could not be re-verified (docs.korge.org, the gadulka blog, and klibs.io were not reachable). The
  underlying reasoning (JVM desktop audio goes through `javax.sound.sampled` unless you write native
  code) is sound; the specific library list is not independently confirmed.
- **shadcn/ui's Tailwind v4 / React 19 support and the `agmmnn/tauri-ui` scaffold (§Implications,
  styling sub-decision)** — `ui.shadcn.com` was not reachable when fact-checked. SONE's own choice
  *is* verified: it ships no component library at all, only Tailwind 4.1 + `lucide-react`.
- **Music Assistant's client-package split date ("October 2024")** — not supported by anything readable
  in the webserver README or the client repo page; drop the date or re-source it. The WebSocket-plus-
  partial-JSON/REST API description itself is independently confirmed.
- **GStreamer's macOS deployment page (PackageMaker-style bundle)** — `gstreamer.freedesktop.org` is
  blocked in this environment; only the Windows deployment page (MSI/`msiexec`/Merge Modules, the
  on-demand-plugin-loading warning) was directly read. Treat the macOS packaging claim as directionally
  right, not primary-sourced.
- **Tauri's "not all desktop plugins ported to mobile" and the macOS notarization mechanism (§4.1)** —
  `v2.tauri.app` is blocked in this environment; these rest on the docs' own maturity language and
  secondary summaries, not a page read directly in this pass. The updater plugin coverage table, the
  WebView2 install-mode table, the macOS `bundle.macOS.frameworks` hook, and the mobile `crate-type`
  requirement (all added to §4.1) **were** read directly from `raw.githubusercontent.com` mirrors of the
  same docs and are higher-confidence.

---

## Sources

### Reference checkouts (read directly)

- `ref:sone/src-tauri/Cargo.toml` — Tauri 2.11.2, gstreamer 0.23, gstreamer-app 0.23, alsa 0.10,
  keyring 3, mpris-server 0.9, zbus 5, aes-gcm 0.10, axum 0.7, rmcp 1.7.0, webkit2gtk 2.0.2 (Linux-only),
  ksni 0.3, x11rb/wayland-client; supports the "Tauri + Rust core + GStreamer" recommendation.
- `ref:sone/package.json` — React 19.1, Tailwind 4.1, Jotai 2.17, @tanstack/react-virtual 3.13,
  hls.js 1.6.16, lucide-react, Vite 7, Vitest 4.1.7, @testing-library/react 16.3.2, TypeScript 5.8,
  knip; supports the frontend sub-decisions and the testing stack.
- `ref:sone/src-tauri/tauri.conf.json` — bundle targets "all", deb/rpm dependency lists,
  `appimage.bundleMediaFramework: true`; supports the packaging section's dependency-footprint claim.
- `ref:sone/README.md` — bit-perfect/exclusive-ALSA feature claims, the "browsers and Electron apps
  downsample audio to 48kHz" statement, install matrix (Flathub/Snap/deb/rpm/AUR/Nix), the
  `WEBKIT_DISABLE_COMPOSITING_MODE=1` NVIDIA workaround, Tech Stack section, and the disclaimer
  ("streaming client only — it does not support offline downloads").
- `ref:sone/src-tauri/src/audio.rs` — `concat`-based gapless attach/detach, GStreamer↔ALSA format
  mapping (`:190-211`), `probe_supported_gst_formats`/`probe_supported_rates` (`:483,545`),
  `start_threshold` reset note (`:692`), the "gapless is normal-only" panic (`:156`); supports §2.1-2.3.
- `ref:sone/src/App.css` — the WebKitGTK SVG `stroke-width` double-zoom workaround; supports the
  webview-divergence risk.
- `ref:sone/src/components/`, `ref:sone/src/atoms/` — 85 components, 12 atom modules (13 files, one a
  colocated test: `ui.test.ts`); supports the effort calibration and the state-management sub-decision.
- `ref:sone/src-tauri/capabilities/default.json` — the capabilities ACL (`core:window:allow-*`,
  `core:webview:allow-create-webview-window`, `opener:default`, `global-shortcut:allow-register`,
  `deep-link:default`); supports the §4.1 capabilities/CSP note.
- `ref:sone/.github/workflows/` (only `flathub-update.yml` present) and
  `ref:sone/build-scripts/build/{Dockerfile.deb,Dockerfile.rpm,PKGBUILD}` — no cross-platform build CI,
  per-distro Docker builds instead (`Dockerfile.deb` = `FROM ubuntu:22.04`); supports the §4.1
  cross-compilation/CI note.
- `ref:sone/.github/workflows/flathub-update.yml:95-139`, `ref:sone/pnpm-workspace.yaml` —
  `flatpak-cargo-generator.py` + `flatpak-node-generator pnpm` regenerating `cargo-sources.json` and
  `pnpm-sources.json` per release; supports the §4.1 Flathub-packaging note.
- `ref:sone/snap/snapcraft.yaml` — `base: core24`, `confinement: strict`, plugs `audio-playback` and
  `alsa`, WebKit bind-path override; supports the Snap packaging notes.
- `ref:sone-windows/src-tauri/Cargo.toml:3` (version `0.16.0`) and
  `ref:sone-windows/src-tauri/src/audio.rs:1236-1244` — `souvlaki = "0.8.3"` on Windows, `wasapi2sink`
  with `exclusive` property; supports the Windows bit-perfect claim and (against
  `ref:sone/src-tauri/Cargo.toml:3` at `0.21.0`) the "fork is behind upstream, not a thin per-OS delta"
  correction.
- `ref:sone-windows/README.md` — "Ported with help from AI agents", "may not be actively or correctly
  maintained"; supports the cross-platform-fork risk and the agent-friendliness argument.
- `ref:strawberry/src/engine/gststartup.cpp:59-69` — demotes `wasapi2sink` to `GST_RANK_SECONDARY` and
  ranks `directsoundsink` `GST_RANK_PRIMARY` on Windows, citing issue #1227; supports the counter-
  evidence to the `wasapi2sink`-exclusive-mode recommendation in §2.2/§2.3/§4.1/§4.11.
- `ref:high-tide/src/lib/player_object.py:90-96,184-226` — `playbin3` with `about-to-finish` and a
  `playbin` fallback, sink map (`autoaudiosink`/`pulsesink`/`alsasink device=`/`jackaudiosink`/
  `pipewiresink`/`osssink`), `Gst.parse_bin_from_description`, `PIPEWIRE` forcing
  `gapless_enabled=False`; supports §2.2 and the GTK-stack description.
- `ref:high-tide/build-aux/io.github.nokse22.high-tide.json` — `org.gnome.Platform` 50,
  `finish-args` with `--socket=pulseaudio` and `--filesystem=xdg-run/pipewire-0:ro` (no `--device=all`);
  supports the Flatpak/bit-perfect sandbox point.
- `ref:high-tide/meson.build`, `ref:high-tide/pyproject.toml`, `ref:high-tide/data/ui/*.blp` —
  meson + ruff `py311` + 22 Blueprint files; supports the Python/GTK stack description.
- `ref:tidalt/README.md` and `ref:tidalt/docs/client-server.md` — three run modes (TUI/daemon/client;
  `tidalt play <url>` starts a TUI when no server is running rather than merely forwarding-and-exiting),
  D-Bus name `org.mpris.MediaPlayer2.tidalt`, "ALSA `hw:` devices cannot be shared between processes",
  `tidalt setup --daemon` writing `~/.config/systemd/user/tidalt.service`, and the README's statement
  that the project was written entirely with LLM coding assistants; supports §3 and the
  agent-friendliness argument. **Does not** document the ALSA format-preference order — see the
  `alsa.c`/`CLAUDE.md`/`architecture.md` citation below.
- `ref:tidalt/internal/player/alsa.c:28-39` and `ref:tidalt/CLAUDE.md:27-30` — the authoritative ALSA
  format-preference order (16-bit: `S32_LE > S16_LE > S24_3LE > S24_LE`; 24-bit:
  `S24_3LE > S24_LE > S32_LE`); `ref:tidalt/docs/architecture.md:43` gives a **different, stale** 16-bit
  order and must not be cited as the source of truth. `ref:tidalt/docs/dac-compatibility.md` names the
  Hidizs S9 Pro Plus (CS43198) as the reason for preferring S32_LE.
- `ref:tidalt/docs/docker.md` — the headless-container recipe (`--device /dev/snd`, `--group-add`
  audio, config/data volumes, PipeWire D-Bus socket forwarding); supports the §3 headless-container
  note.
- `ref:tidalt/go.mod`, `ref:tidalt/internal/player/{alsa.c,alsa.h,avcodec.c,avcodec.h}` —
  bubbletea 1.3.10, lipgloss 1.1.0, godbus/dbus v5, `docker/secrets-engine`, `filippo.io/age` v1.3.1
  (an **indirect** dependency, not direct); 357 lines of cgo C/headers total (`alsa.c`/`.h` 151 lines
  for ALSA output, `avcodec.c`/`.h` 206 lines for FFmpeg decode — not just a "111-line `alsa.c`");
  supports the Go evaluation.
- `ref:tidalt/internal/player/alsa_fallback_test.go` — the one reference-project audio test that exists
  (68 lines, covers the `plughw:` fallback); supports the §3 audio-test-harness note.
- `ref:tidal-hifi/package.json` — `"electron": "github:castlabs/electron-releases#v43.0.0+wvcus"`,
  MIT, electron-builder targets, mpris-service; supports the Electron evaluation.
- `ref:strawberry/CMakeLists.txt:228-232,242` — C++ with Qt, `QT_MIN_VERSION` 6.8.0 on
  `APPLE OR WIN32`, else 6.4.0 (corrects an earlier `:230-242` line citation).
- `ref:strawberry/src/` (recount, 2026-09-07) — ~166,122 lines of C++/headers (491 `.cpp` = 121,981
  lines; 568 `.h` = 44,141 lines; `src/core` alone 21,510 lines) across 40 subdirectories — corrects an
  earlier "14,708 lines" count, which came from an `xargs wc -l` invocation whose batching silently
  truncated to the last batch's "total" line; use `find ... -print0 | xargs -0 cat | wc -l` instead of
  piping a multi-batch `wc -l` through `tail -1` when recounting large trees.
- `ref:strawberry/src/engine/gststartup.cpp:59-69` — see above.
- `ref:python-tidal/tidalapi/` — 6,046 lines, LGPL-3.0-or-later, codec list including EAC3/AC4;
  supports the codec/licence notes.
- `ref:tidalrs/src/` — 3,545 lines of Rust, MIT, reqwest 0.12 + rustls + stream-download;
  supports the "a Rust TIDAL client core is a few thousand lines" calibration.
- `ref:high-tide/src/lib/player_object.py:140,457-491` — in-memory `self.manifest`, reset per track with
  no TTL; supports the corrected (numberless) manifest-lifetime note in the cross-cutting decisions.

### Web sources

- https://crates.io/api/v1/crates/tauri — tauri 2.11.5, 2026-07-01, Apache-2.0 OR MIT, MSRV 1.77.2,
  ~29.4M downloads.
- https://v2.tauri.app/blog/tauri-20/ (via search) — Tauri 2.0 stable 2024-10-02 with mobile support.
- https://en.wikipedia.org/wiki/Tauri_(software_framework) (via search) — 2.11 line, 2.11.5 on
  2026-07-01; production users Spacedrive, AppFlowy, Clash Verge.
- https://v2.tauri.app/develop/debug/linux-graphics/ , https://github.com/tauri-apps/tauri/issues/13157 ,
  https://github.com/tauri-apps/tauri/issues/14286 , https://github.com/tauri-apps/tauri/issues/7021 ,
  https://github.com/orgs/tauri-apps/discussions/9088 — WebKitGTK font-weight offset, glitchy rendering,
  DMABUF workaround; supports the webview-divergence risk.
- https://v2.tauri.app/reference/config/ (via search) — bundle targets deb/rpm/appimage/nsis/msi/app/dmg/all.
- https://v2.tauri.app/distribute/sign/macos/ , https://v2.tauri.app/distribute/sign/ios/ (via search) —
  notarization via App Store Connect API key or Apple ID.
- https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/develop/Tests/WebDriver/index.mdx ,
  https://github.com/tauri-apps/tauri/issues/7068 , https://webdriver.io/docs/desktop-testing/tauri/platform-support/ —
  tauri-driver is Windows+Linux only, no WKWebView driver on macOS.
- https://crates.io/crates/tauri-plugin-native-audio , https://github.com/uvarov-frontend/tauri-plugin-native-audio —
  1.0.5 (2026-03-02), MIT/Apache, Android Media3 ExoPlayer + iOS AVPlayer, Android 8.0+/iOS 14.0+,
  ~1,544 downloads; supports the "mobile audio is unproven in Tauri" claim.
- https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/plugin/updater.mdx —
  updater plugin platform coverage (AppImage-only on Linux, `.app.tar.gz` macOS, MSI/NSIS Windows; no
  deb/rpm/Snap/Flatpak); supports the §4.1 auto-update note.
- https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/distribute/windows-installer.mdx
  (lines ~187-323) — `webviewInstallMode` options and size deltas (`downloadBootstrapper` +0 MB,
  `embedBootstrapper` +~1.8 MB, `offlineInstaller` +~127 MB, `fixedVersion` +~180 MB, `skip` +0 MB);
  supports the §4.1 WebView2 install-mode note.
- https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/distribute/macos-application-bundle.mdx
  (lines ~120-180) — `bundle.macOS.frameworks` (system frameworks, custom `.framework`/`.dylib`, example
  lists `"CoreAudio"`), `bundle.macOS.minimumSystemVersion` default 10.13, `bundle.macOS.entitlements`;
  supports the §4.1 macOS packaging note.
- https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/develop/calling-rust.mdx
  (lines ~194-207, ~502-547) — IPC return values are JSON-serialized by default and this "can slow down
  your application" for large data; `tauri::ipc::Response::new(bytes)` for raw payloads; supports the
  §4.1 IPC data-path note.
- https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/start/migrate/from-tauri-1.mdx
  (lines ~21-44) — the `[lib] crate-type = ["staticlib","cdylib","rlib"]` + `tauri::mobile_entry_point`
  shape required for Android/iOS builds; supports the §4.1/§10 mobile-crate-shape note.
- https://github.com/martpie/museeks , https://raw.githubusercontent.com/martpie/museeks/master/src-tauri/Cargo.toml —
  **corrects an earlier draft's classification of Museeks as Electron.** Museeks is Tauri 2 + Rust +
  React (ported from Electron; author announcement at
  https://news.ycombinator.com/item?id=39486181); its `Cargo.toml` pins `tauri` 2.11.2 with no audio
  crate — playback stays in the webview. Also source for "Tauri does not support cross-platform
  binaries" in the README. Supports §6 and the §4.1 cross-compilation note.
- https://crates.io/api/v1/crates/cpal — cpal 0.18.2, 2026-08-16, Apache-2.0, MSRV 1.85.
- https://raw.githubusercontent.com/RustAudio/cpal/master/CHANGELOG.md — no exclusive mode anywhere;
  **0.17.2 (marked [YANKED])** "Enable as-necessary resampling in the WASAPI server process"
  (changelog line ~104); **0.18.2** (not 0.18.0) "Output streams no longer reject formats that the
  built-in resampler can convert" (line ~98, inside the `## [0.18.2] - 2026-08-16` section); **0.18.0**
  (2026-06-06, line ~137) `device_by_id()` accepts ALSA shorthand `hw:0,0`/`plughw:foo`; 0.17.0 only
  introduced `device_by_id()` itself, for generic stable device IDs. Supports the cpal disqualification
  for bit-perfect Windows, with the version attributions corrected against an earlier draft.
- https://github.com/RustAudio/cpal/issues/106 , https://github.com/RustAudio/cpal/issues/459 —
  exclusive-mode requests from 2016 and 2020; #459 closed without a documented implementation.
- https://crates.io/api/v1/crates/wasapi — wasapi 0.24.0, 2026-08-12, MIT; the crate you need for
  exclusive WASAPI in Rust.
- https://crates.io/api/v1/crates/symphonia — 0.6.1, 2026-08-13, MPL-2.0, codecs MP3/FLAC/Vorbis/AAC/
  ALAC/ADPCM/PCM, containers OGG/WAV/MKV/MP4/CAF/AIFF, MSRV 1.85.
- https://crates.io/api/v1/crates/rodio — 0.22.2, 2026-03-05, MIT/Apache.
- https://crates.io/api/v1/crates/gstreamer — 0.25.3, 2026-06-29, MIT/Apache, features for GStreamer
  1.16-1.30.
- https://raw.githubusercontent.com/mpv-player/mpv/master/DOCS/man/ao.rst — AO list incl.
  `coreaudio_exclusive` ("direct device access and exclusive mode (bypasses the sound server)"),
  `--wasapi-exclusive-buffer` applying "when using `--audio-exclusive=yes`", `--alsa-resample` disabled
  by default; supports the libmpv row of the audio table.
- https://github.com/jeffvli/feishin/discussions/88 — practical bit-perfect recipe with mpv:
  `--audio-device=alsa/iec958:CARD=…`, `--gapless-audio=no`, exclusive mode; Flathub build uses PipeWire
  and does not work; macOS `--ao=coreaudio_exclusive --coreaudio-change-physical-format=yes` reported as
  unreliable by one user; supports the macOS-unverified flag.
- https://gstreamer.freedesktop.org/documentation/deploying/windows.html and
  https://gstreamer.freedesktop.org/documentation/deploying/mac-osx.html — packing the GStreamer MSI and
  running it via `msiexec`, Merge Modules, and the warning that on-demand plugin loading makes trimming
  risky; supports the Windows/macOS packaging burden.
- https://crates.io/api/v1/crates/slint — 1.17.1, 2026-07-07, `GPL-3.0-only OR
  LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`, MSRV 1.92, femtovg/software/Skia/WGPU.
- https://github.com/slint-ui/slint/blob/master/LICENSE.md , https://slint.dev/pricing — royalty-free
  licence requires disclosure (AboutSlint widget or Slint badge) or a commercial licence.
- https://crates.io/api/v1/crates/iced , https://github.com/iced-rs/iced/releases/tag/0.14.0 —
  0.14.0, 2025-12-07, MIT, MSRV 1.88; reactive rendering, headless testing, hot reloading; COSMIC uses it.
- https://crates.io/api/v1/crates/egui — 0.36.1, 2026-08-07, MIT/Apache.
- https://crates.io/api/v1/crates/gpui , https://www.gpui.rs/ — 0.2.2, 2025-10-22, Apache-2.0, pre-1.0
  with frequent breaking changes.
- https://crates.io/crates/relm4 , https://relm4.org/ — 0.11.0, 2026-04-08 (0.10.1 2025-12-29,
  0.10.0 2025-09-01).
- https://github.com/linebender/xilem — Xilem/Masonry alpha as of 2026; README says "Lots of things
  need improvements"; major breaking changes expected.
- https://crates.io/api/v1/crates/masonry — masonry 0.4.0, 2025-10-29 (0.3.0 was 2025-05-10, 0.2.0 was
  2024-05-07 — over two years stale); corrects an earlier draft's "v0.2.0 is the recent release" claim.
- https://en.wikipedia.org/wiki/GTK , https://docs.gtk.org/gtk4/osx.html , https://docs.gtk.org/gtk4/windows.html —
  GTK 4.22.4 stable 2026-04-30; "partial support" for macOS and Windows; X11/Broadway backends deprecated.
- https://gtk-rs.org/gtk4-rs/stable/latest/book/libadwaita.html and the GNOME Discourse thread
  "Is libadwaita suitable for multiplatform apps?" (fetch blocked; read via search summary) —
  libadwaita is a GNOME visual API, relies on portals, ignores `gtk-theme` by design.
- https://crates.io/api/v1/crates/uniffi , https://mozilla.github.io/uniffi-rs/latest/ — 0.32.0,
  2026-06-30, MPL-2.0; Kotlin/Swift/Python production-quality, "a long way from a 1.0 release",
  partial Swift 6 support; Kotlin Multiplatform bindings exist.
- https://pub.dev/packages/flutter_rust_bridge , https://github.com/fzyzcjy/flutter_rust_bridge ,
  https://pub.dev/packages/simple_audio — Dart↔Rust binding generator; `simple_audio` is a
  flutter_rust_bridge-based audio package.
- https://blog.jetbrains.com/kotlin/2025/05/compose-multiplatform-1-8-0-released-compose-multiplatform-for-ios-is-stable-and-production-ready/ —
  Compose Multiplatform for iOS Stable, 1.8.0, 2025-05-06.
- https://blog.jetbrains.com/kotlin/2025/09/compose-multiplatform-1-9-0-compose-for-web-beta/ —
  1.9.0, Compose for Web Beta, iOS/desktop improvements.
- https://docs.korge.org/audio/ and https://iamkonstantin.eu/blog/meet-gadulka-a-minimalistic-player-library-for-kotlin-multiplatform/ —
  KorAU uses `javax.sound.sampled` on the JVM; gadulka is a minimalistic KMP player; supports the
  "no bit-perfect on JVM desktop" claim.
- https://avaloniaui.net/blog/avalonia-12 (fetch blocked; read via search) — Avalonia 12 in 2026,
  3× Android performance, native dispatcher, page-based navigation; draws every control itself.
- https://github.com/KDAB/cxx-qt , https://crates.io/api/v1/crates/cxx-qt — 0.10.0, 2026-08-24,
  MIT/Apache; safe Rust↔Qt interop.
- https://github.com/wailsapp/wails/releases — **corrects an earlier draft's `v3.0.0-beta.9` citation**:
  the latest prerelease as of 2026-09-07 is `v3.0.0-beta.17` (2026-09-06); every beta since at least
  beta.8 carries "the API is stable, but you may still encounter issues before the final 3.0 release";
  v2 remains the stable release. Re-check this URL rather than trusting a cached beta number.
- https://fyne.io/blog/2026/07/13/fyne-v2.8-released/ — Fyne 2.8, 2026-07-13, 1,000+ commits,
  Wayland auto-selected on Linux, Go 1.22 minimum.
- https://github.com/dweymouth/supersonic — Go + Fyne 2.8, mpv audio, v0.22.0, gapless + ReplayGain +
  15-band EQ + "optional audio exclusive mode", AppImage bundles mpv, Flatpak CJK font limitation.
- https://github.com/jeffvli/feishin/releases , https://deepwiki.com/jeffvli/feishin — Electron +
  React 19 + Mantine + Zustand + electron-vite, mpv backend, v1.15.1 on 2026-07-19.
- https://github.com/jpochyla/psst — Rust + druid; author's note on druid architecture and Xilem;
  as of Feb 2026 needs a user-supplied Spotify Developer Client ID.
- https://apps.gnome.org/Amberol/ — GTK4 + Rust + GStreamer, latest 2026.1 on 2026-04-05.
- https://github.com/flathub-infra/documentation/commit/992f57b30de98ddbd5e80959e9672998c83c8c97 —
  "Reword LLM policy to make it clear it's not allowed"; file
  `docs/02-for-app-authors/02-requirements.md`; "Applications containing AI-generated or AI-assisted
  code, documentation, or other content are not allowed"; "Exceptions may be granted for mature,
  well-maintained projects."
- https://raw.githubusercontent.com/flathub-infra/documentation/main/docs/02-for-app-authors/02-requirements.md
  (read 2026-09-07, section "### Generative AI policy" at line 241) — the current disclosure-based
  wording quoted verbatim in §9, **including "Disclosure does not create a presumption of acceptance,"
  omitted from an earlier draft of this report**, plus the trademark rule ("A Firefox fork cannot
  mention `Firefox` in its name or use any of the official icon, logo or artwork," lines ~727-728) and
  the redistribution/licence requirements.
- https://www.gamingonlinux.com/2026/05/flathub-moves-to-ban-nearly-all-apps-and-submissions-made-with-generative-ai/ ,
  https://linuxiac.com/flathub-now-rejects-ai-assisted-apps-and-submissions/ ,
  https://www.opensourceforu.com/2026/06/flathub-bans-ai-submissions-across-flatpak-repos/ — **fact-checked
  as "uncertain": these domains were not reachable in this environment either time this report was
  checked.** The 2026-05-29 effective date and non-retroactivity claim rest on secondary reporting
  (originally read via search summaries), not a primary document; supports the timeline in §9 at
  reduced confidence.
- https://developer.apple.com/app-store/review/guidelines/ — guideline 5.2.2 verbatim, 5.2.1, 4.2, 2.5.6;
  supports the iOS App Store gate.
- https://github.com/music-assistant/server , https://deepwiki.com/music-assistant/server/9-api-and-interfaces ,
  https://raw.githubusercontent.com/music-assistant/server/dev/music_assistant/controllers/webserver/README.md —
  command-based API over WebSocket on port 8095 (`/ws`, auth then routed commands) with a partial
  JSON-RPC-over-HTTP projection at `/api`; a separate `music-assistant/client` package exists, but **its
  "split out in October 2024" date is unverified — neither README states an origin date**; supports
  architecture Shape 3.
- https://raw.githubusercontent.com/Sinono3/souvlaki/master/README.md , https://crates.io/api/v1/crates/souvlaki —
  0.8.3, 2025-06-24, MIT; MPRIS on Linux/BSD, native frameworks on macOS/iOS, Windows media controls;
  **Windows requires an HWND**, macOS requires an AppDelegate/winit run loop.
- https://ui.shadcn.com/docs/tailwind-v4 , https://github.com/agmmnn/tauri-ui — shadcn/ui updated for
  Tailwind v4 and React 19, `@theme` directive, HSL→OKLCH; Tauri+shadcn scaffolds exist; supports the
  styling sub-decision.
