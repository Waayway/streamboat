# Reference-project pointers, candidate stacks, and recommended designs

Full narrative: `docs/research/audio-pipeline.md` §7-§9, plus fact-check gap-fill in §10.16-10.17.

## Table of contents

1. What each reference project actually does — file pointers
2. Candidate audio stacks compared (including stacks the first pass missed)
3. Recommended pipeline designs (A, B, C) and the recommendation

---

## 1. What each reference project actually does — file pointers

| Project | Language | Decode | Output | Gapless | Bit-perfect | Key files |
|---|---|---|---|---|---|---|
| **Sone** | Rust + Tauri 2 | GStreamer 0.23, `uridecodebin` | Normal: `autoaudiosink`. Exclusive: `appsink` -> own `libasound` writer thread | `concat` + prerolled branch, normal mode only | yes, full | `ref:sone/src-tauri/src/audio.rs` (~3300 lines), `signal_path.rs`, `pipeline_probe.rs`, `commands/playback.rs`, `cache.rs`, `mpris.rs`, `idle_inhibit/` |
| **sone-windows** | Rust + Tauri 2 | same | `wasapi2sink exclusive=… low-latency=true device=…` | inherited (concat path) | yes, WASAPI exclusive | `ref:sone-windows/src-tauri/src/audio.rs:1236-1246,1580-1602` |
| **High Tide** | Python + GTK4/libadwaita | GStreamer `playbin3` (fallback `playbin`) | Selectable: `autoaudiosink`, `pulsesink`, `alsasink device=…`, `jackaudiosink`, `osssink`, `pipewiresink` | `about-to-finish`, disabled on `pipewiresink` | partial — ALSA sink only, no rate/format control, no Flatpak `/dev/snd` | `ref:high-tide/src/lib/player_object.py`, `mpris.py`, Flatpak manifest |
| **Strawberry** | C++17 + Qt6 | GStreamer, four TIDAL endpoint variants | `alsasink`/`pulsesink`/`pipewiresink`/`osxaudiosink`/`directsoundsink` (Win default)/`wasapi(2)sink`/`asiosink` | `about-to-finish` + `SetNextUrl` | derived: `hw:`/`plughw:` -> exclusive; sets `exclusive` on any sink that has it | `ref:strawberry/src/engine/gstenginepipeline.cpp`, `gstengine.cpp`, `gststartup.cpp`, `tidal/tidalstreamurlrequest.cpp`, `engine/*devicefinder.cpp` |
| **tidalt** | Go + CGO | FFmpeg direct, AVIO callback streaming | direct `libasound` `hw:` with `plughw:` fallback | keeps device open across tracks | yes, plus PipeWire D-Bus reservation | `ref:tidalt/internal/player/alsa.c`, `mpv.go`, `avcodec.go`, `docs/architecture.md`, `docs/client-server.md` |
| **mopidy-tidal** | Python, headless | GStreamer via Mopidy | Mopidy's | Mopidy's | no | `ref:mopidy-tidal/mopidy_tidal/playback.py`, `gstreamer_proxy/{proxy,cache,types}.py` |
| **tidal-hifi** | TypeScript + Electron | Chromium + Widevine (castlabs fork) | Chromium -> Pulse/PipeWire/ALSA | Chromium's | no — Chromium reportedly resamples (see caveat below); mitigated with `--audio-output-sample-rate=192000` (opt-in) | `ref:tidal-hifi/src/constants/flags.ts`, `features/flags/flags.ts`, `package.json` |
| **tidal-sdk-web player** | TypeScript | Shaka Player (DASH), native HLS on Safari, or a proprietary native component | browser, or the native component's exclusive/shared modes | dual Shaka instances + 250 ms micro-crossfade | only via the proprietary native component | `player/shakaPlayer.ts`, `browserPlayer.ts`, `basePlayer.ts`, `nativeInterface.ts`, `internal/helpers/*` |
| **tidal-sdk-android player** | Kotlin | ExoPlayer/androidx.media3 1.5.0 | `DefaultAudioSink`, fixed buffer provider | ExoPlayer playlist | n/a | `player/di/{RendererModule,ExtendedExoPlayerModule}.kt`, `volume/{LoudnessNormalizer,VolumeHelper}.kt`, `model/BufferConfiguration.kt` |
| **tidal-sdk-ios player** | Swift | AVPlayer/AVQueuePlayer, HLS + FairPlay | AVAudioSession | AVQueuePlayer | n/a | `Common/Data/AudioCodec.swift`, `PlaybackInfo/PlaybackInfoFetcher.swift` |
| **python-tidal** | Python | none — library only | none | n/a | n/a | `tidalapi/media.py` (`Stream`, `StreamManifest`, `DashInfo`) |
| **tidal-cli** | TypeScript/Node | none — downloads segments, shells to `mpv`/`afplay` | external player | n/a | n/a | `src/playback.ts` |
| **TidaLuna** | TypeScript | mod inside the official Electron client | official client's | official client's | no | `plugins/lib.native/src/request/{decrypt,fetchMediaItemStream,fetchStream}.ts` |
| **tidal-connect** | Bash + Docker around a proprietary binary | proprietary | ALSA via PortAudio | proprietary | up to 24/48 hi-res; MQA self-unfolding lost July 2024 — **correction, was wrongly stated as "LOSSLESS only"** | `bin/{entrypoint.sh,common.sh}`, `userconfig/*.asound.conf`, `samples/*.env` |

## 2. Candidate audio stacks compared (including stacks the first pass missed)

Version/licence data from crates.io (queried 2026-09-07) and upstream source reads —
`references/sources.md`.

| Stack | Linux | Windows | macOS | Headless/Pi | Bit-perfect Linux | Bit-perfect Win | Bit-perfect macOS | 24/192 | Gapless | DASH | Licence | Maturity | Mobile path |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **gstreamer-rs 0.25.3** + own ALSA writer (Sone design) | yes | yes | yes | yes | yes (own writer) | yes (`wasapi2sink exclusive`, needs GStreamer >= 1.28) | **no** (needs own CoreAudio writer) | yes | yes (`concat`) | yes | bindings MIT/Apache-2.0; GStreamer LGPL-2.1+ | very high; two shipping TIDAL clients | GStreamer builds for Android/iOS but is heavy; would likely swap the sink |
| **libmpv (`libmpv2` 6.0.0)** | yes | yes | yes | yes | yes — via `--audio-device=alsa/hw:X,Y`, **not** `--audio-exclusive` (silently ignored on `alsa`) | yes (`--ao=wasapi --audio-exclusive=yes`) | **yes** (`coreaudio_exclusive`) | yes | yes (`--gapless-audio=weak`) | yes (FFmpeg) | crate LGPL-2.1; mpv GPLv2+ (LGPL via Meson `-Dgpl=false`) | very high | mpv builds for Android; iOS is awkward |
| **Symphonia 0.6.1 + cpal 0.18.2 / rodio 0.22.2** | yes | yes | yes | yes | **no** via cpal (needs direct `alsa` 0.12.1) | **no** (cpal has no exclusive mode, verified against its source) | **no** | yes (decode side) | needs hand-rolling | **no** — needs `dash-mpd` 0.20.4 + own fetcher | MPL-2.0/Apache-2.0/MIT | Symphonia high, assembled stack unproven | best story: pure Rust, Android via `oboe` 0.6.1, iOS via AVAudioEngine FFI |
| **Symphonia + direct backends** (`alsa`/`wasapi`/`coreaudio-rs`) | yes | yes | yes | yes | **yes** | **yes** | **yes** | yes | hand-rolled | **no** | all permissive | you write and maintain three backends | best |
| **FFmpeg (`ffmpeg-next` 9.0.0) + direct backends** (tidalt design) | yes | yes | yes | yes | yes | yes | yes | yes | hand-rolled (keep device open) | yes | crate WTFPL; FFmpeg LGPL-2.1+ | tidalt ships it on Linux; Win/macOS unproven | fine |
| **GStreamer from C++/Qt** (Strawberry design) | yes | yes | yes | yes | yes | yes | **no** | yes | yes | yes | LGPL/GPL | very high | poor |
| **GStreamer from Python** (High Tide design) | yes | awkward | awkward | yes | partial | untested | no | yes | yes | yes | LGPL/GPL | high on Linux only | poor |
| **Native core (Rust/C++) + webview UI** (Sone/sone-windows design — missing from the first pass) | yes | yes | yes | n/a (no webview needed headless) | yes | yes | no (same macOS gap as GStreamer) | yes | yes | yes | depends on core | very high — proves a beautiful UI does not cost bit-perfect audio (see caveat below) | webview doesn't port to mobile as-is, but the native core can |
| **Electron/MSE/Web Audio** (tidal-hifi design) | yes | yes | yes | no | **no** | **no** | **no** | resampled (uncertain, see caveat) | yes-ish | yes (Shaka) | Apache-2.0 + castlabs Electron | very high | good (it's a browser) |
| **miniaudio** (single-header C, public-domain/MIT-0) `[uncertain, see caveat]` | yes | yes (WASAPI exclusive claimed) | yes (CoreAudio) | yes | plausible | plausible | plausible | yes (device side) | hand-rolled | **no** | permissive | replaces the `alsa`/`wasapi`/`coreaudio-rs` trio only, not Symphonia | unproven |
| **python-mpv** (ctypes binding to libmpv) | yes | awkward | awkward | yes | same as libmpv | same as libmpv | same as libmpv | yes | yes | yes | same as libmpv + Python packaging | High Tide proves Python+GStreamer viable on Linux; same argument applies here | poor |
| **C++ + libmpv** | yes | yes | yes | yes | same as libmpv | same as libmpv | same as libmpv | yes | yes | yes | same as libmpv | same as libmpv, different host language | same as libmpv |

**Notes that decide this table:**

- **`cpal` has no exclusive mode on any platform.** `RustAudio/cpal#459` (opened 2020) is closed
  unimplemented; `src/host/wasapi/device.rs` hardcodes `AUDCLNT_SHAREMODE_SHARED` at every call
  site. rodio/kira sit on cpal and inherit this. Any Rust bit-perfect client must go below cpal.
- **Symphonia has no HE-AAC** (AAC-LC "great", HE-AAC "in work or not started") — a Symphonia-only
  client cannot play TIDAL's `LOW` tier.
- **Symphonia has no DASH/HTTP layer.** `dash-mpd` 0.20.4 parses MPDs, `stream-download` 0.24.4
  handles buffered HTTP. You assemble FLAC-in-fMP4 yourself from `SegmentTemplate`/`SegmentTimeline`
  and feed Symphonia's ISO/MP4 reader — doable (python-tidal/tidal-cli do the manifest half in
  ~150 lines) but see `decoding-and-codecs.md` §2 for the undocumented traps in Symphonia's own
  FLAC-in-fMP4 support before committing to this.
- **GStreamer cannot be bit-perfect on macOS** — see `output-backends.md` §5. Any GStreamer-based
  streamboat needs either an `appsink` -> own CoreAudio writer, or accepts shared-mode on macOS.
- **libmpv is the only off-the-shelf engine with exclusive output on all three desktops out of the
  box**, and brings DASH/HLS/gapless/seeking/buffering/every codec for free. Costs: a
  property-string API rather than a typed pipeline; GPLv2+ unless built LGPL (`-Dgpl=false`, and
  mpv's own docs discourage LGPL mode for anything but libmpv — which is exactly this use case);
  less direct control of the exact PCM path; harder to build the signal-path transparency panel from
  inside it (though `/proc/asound/*/hw_params` and libmpv's own `audio-out-params`/`current-ao`
  properties both still work — see `output-backends.md` §10).
- **A webview UI does not preclude bit-perfect audio — only rendering audio in the browser engine
  does.** Sone and sone-windows render their entire UI in a Tauri webview and are the strongest
  bit-perfect references in the set, because audio never touches the web layer — GStreamer decodes
  into an `appsink`, a Rust thread writes to `libasound`/WASAPI directly. The one place Sone *does*
  route audio through its webview is video, and it says so: *"Video audio is streamed and does not
  use the bit-perfect lossless signal path that music tracks use"* (`ref:sone/README.md:501`, via
  hls.js). **The rule:** bit-perfect is impossible only when the browser engine itself renders the
  audio (MSE/Web Audio — the tidal-hifi design); a webview used purely for UI, with a native core
  handling audio, is unaffected. Do not read the Electron row's limitations as ruling out a
  webview-based UI in general — a beautiful "simple but beautiful" UI (the owner's stated goal) does
  not have to cost bit-perfect audio.
- **The Electron/tidal-hifi "no bit-perfect" verdict itself carries a caveat.** The tidal-hifi flags
  (`--audio-output-sample-rate=192000` etc.) are confirmed exactly as written and are a user opt-in,
  not a default. The underlying claim that Chromium resamples *every* audio output path is
  `[uncertain]` — the only citation found (`issues.chromium.org/issues/40944208`) is titled about
  WebAudio/AudioContext specifically, and the issue body could not be read from this research
  environment.
- **miniaudio's row is `[uncertain]`** — capabilities described from general knowledge of the
  library, not independently verified against `https://miniaud.io/docs/` in this research pass.
  Re-check before relying on it. It would, if verified, replace only the device-output trio
  (`alsa`/`wasapi`/`coreaudio-rs`) in the pure-Rust design, not Symphonia's decode/DASH gap.
- **python-mpv / C+++libmpv** add no new capability over libmpv itself — listed only because the
  brief asked for a like-for-like comparison across host languages and the first pass omitted them.

## 3. Recommended pipeline designs (A, B, C) and the recommendation

### Design A — Rust core, GStreamer decode, per-platform output writer

The Sone design generalised to three platforms.

```
                       control thread (tokio): session, manifest fetch, quality cascade,
                       retry, replay-gain calc, queue, MPRIS/SMTC/NowPlaying
                                  |  AudioCommand (mpsc)      |  events (broadcast)
                       audio worker thread (owns the GStreamer pipeline)
                                  |                           |  AttachJob (mpsc)
                                  |                  attach executor thread
                                  |                  (serializes concat pad requests)
   SHARED MODE                    |        EXCLUSIVE / BIT-PERFECT MODE
   uridecodebin(A) -> queue -+   |        uridecodebin -> audioconvert
   uridecodebin(B) -> queue -+ concat       (dithering=none, noise-shaping=none)
     -> audioconvert -> audioresample         [-> capsfilter (BTS only)]
     -> volume(norm) -> volume(user)          -> appsink (max-buffers=20, sync=false)
     -> autoaudiosink | wasapi2sink | osxaudiosink        |  AudioChunk
                                                   writer thread: alsa | wasapi | coreaudio
```

- **Threads:** control (tokio multi-thread), audio worker (blocking, owns pipeline), attach executor
  (serializes `concat` pad ops), writer (one per open device, real-time-ish priority — see
  `output-backends.md` §9), plus GStreamer's own streaming threads.
- **Channels:** `mpsc<AudioCommand>` with reply channels; `crossbeam::bounded(256)` for
  `WriterCommand`; an `AtomicU32` holding `f32::to_bits(user_amp * norm_gain)` for lock-free volume;
  an `AtomicU64` generation counter to drop stale PCM; `AtomicU64 frames_written` for exclusive-mode
  position — **corrected via `snd_pcm_delay()`**, see `output-backends.md` §8.
- **Buffers:** see `playback-behavior.md` §5.
- **State machine:** `Idle -> Resolving -> Prerolling -> Playing <-> Paused -> Draining -> Idle`;
  device-level states orthogonal: `DeviceClosed -> Reserving -> Open(fmt) -> Reconfiguring(fmt') ->
  Open(fmt')`.
- **Gapless:** `concat` with two branches in shared mode. In exclusive mode, achievable when two
  tracks share format+rate — hold the device open, feed the next track's chunks straight through,
  close/reopen only on a format change (mpv's `--gapless-audio=weak`; what tidalt does). Sone gives
  up here; streamboat should not.
- **macOS:** write a CoreAudio writer mirroring the ALSA one — hog mode, `SupportsMixing=false`,
  `kAudioStreamPropertyPhysicalFormat` for rate/depth, **plus the
  `kAudioDevicePropertyNominalSampleRate` async-change listener** from `output-backends.md` §5 (the
  original design sketch omitted this).
- **Headless:** the control thread is the whole product; expose over a local socket/D-Bus
  (`org.mpris.MediaPlayer2` + a private interface, as tidalt does) and let the TUI/CLI be a client —
  see `os-integration.md` §4 for the full client/server architecture this implies.
- **Version floors this design must respect:** GStreamer >= 1.26.10 before switching off the legacy
  `dashdemux` for DASH (`decoding-and-codecs.md` §1) — **and note this needs an explicit
  `dashdemux2`-rank-demotion startup step even below that floor, since plain `uridecodebin` does not
  autoplug the legacy demuxer on its own** (`output-backends.md` §12); GStreamer >= 1.28 before
  relying on `wasapi2sink exclusive` (`output-backends.md` §4). Current GStreamer is **1.28.6**, not
  1.28.2/1.28.3 (`decoding-and-codecs.md` §1).
- **Mobile later:** replace the writer with `oboe`/`AAudio` (Android) or `AVAudioEngine` (iOS);
  either keep GStreamer (builds for both) or swap the decode half for the platform decoder.

**Cost:** three output backends to write and maintain (though see `output-backends.md` §13 — plain
`alsasink device=hw:` covers most of Linux and a custom writer is only strictly needed for a smaller
set of reasons than "bit-perfect at all"), plus GStreamer as a shipped runtime dependency on Windows
and macOS. **Windows has one fully documented bundling recipe** (sone-windows, ~20 MB, NSIS+WiX
hooks) **but it ships no AAC decoder** — `LOW`/`HIGH` need one added deliberately — **and macOS has
no reference recipe at all**, cost that as unproven, not solved (`output-backends.md` §14). Whether
to bundle GStreamer on Linux too, rather than link the system one, is itself an open decision with
real consequences for the FLAC-in-DASH version-floor problem — see `output-backends.md` §18.

### Design B — Rust core, libmpv engine

```
control thread (tokio) --property/command--> libmpv instance --> ao=alsa|wasapi|coreaudio
        ^                                            |
        +-------------- mpv event loop thread --------+
```

```
--ao=alsa               (Linux)   --audio-device=alsa/hw:1,0   --alsa-resample=no  (mpv's default; harmless to set explicitly)
--ao=wasapi             (Windows) --audio-exclusive=yes        --wasapi-exclusive-buffer=default
--ao=coreaudio          (macOS)   --audio-exclusive=yes        --coreaudio-change-physical-format=yes
--gapless-audio=weak    (never `yes` — locks the rate to the first track)
--audio-channels=auto-safe
--prefetch-playlist=yes (default no — without it, prefetch never actually happens)
--volume-gain=<db>      apply TIDAL's ReplayGain formula as dB, on top of user volume
--demuxer-lavf-o=protocol_whitelist=file,crypto,data,http,https,tcp,tls   (needed for a DASH MPD referencing http(s) segment URLs)
--cache=yes --demuxer-max-bytes=… --demuxer-readahead-secs=…
```

- **Queue/gapless:** `loadfile <uri> append` plus `--prefetch-playlist=yes` (§ above) — mpv keeps
  the device open across tracks when the format matches; disable prefetch (or rebuild the playlist)
  around a user reorder, per mpv's own caveat that prefetch "can occasionally make wrong prefetching
  decisions."
- **ReplayGain:** `--volume-gain=<db>` directly from the formula in `playback-behavior.md` §4, not a
  percentage-converted `volume` set. `--replaygain=no` (mpv's default, correctly left off — TIDAL's
  fMP4 carries no ReplayGain tags).
- **DASH loading:** the `--demuxer-lavf-o=protocol_whitelist=…` line above is very likely the actual
  fix for the open question of whether libmpv accepts a
  `data:application/dash+xml;base64,…` URI directly — FFmpeg's demuxer otherwise refuses to follow
  http(s) segment URLs out of a `file://`/`data:` manifest. Fallback: write the MPD to a temp file
  and pass `file://`, as mopidy-tidal and modern High Tide do.
- **Device reservation:** libmpv has **no** `ReserveDevice1` support — do the D-Bus handshake in the
  host process around libmpv (reserve -> hand the freed `hw:` device to mpv via `--audio-device` ->
  release on stop), exactly as tidalt drives mpv externally. See `output-backends.md` §2.
- **Hardware volume:** use libmpv's `ao-volume` (system/hardware mixer), not `volume` (software
  mixer), when bit-perfect mode needs *some* attenuation path — see `output-backends.md` §7.
- **Threads:** control (tokio) plus one thread pumping `mpv_wait_event`.
- **Pros:** one dependency covers decode, DASH, HLS, buffering, seek, gapless, exclusive output on
  all three desktops. Dramatically less code than A. Excellent headless fit. Fastest route to a
  working product.
- **Cons:** GPLv2+ unless built LGPL (`-Dgpl=false`, discouraged for anything but libmpv — which is
  this use case); less signal-path visibility (mitigated by `/proc/asound/*/hw_params` +
  `audio-out-params`/`current-ao`/`audio-device-list`, `output-backends.md` §10); property-string
  API rather than typed pipeline; no `ReserveDevice1` (handled above).

### Design C — pure Rust, no C media framework

**Fits:** Symphonia 0.6.1 + `dash-mpd` 0.20.4 + `stream-download` 0.24.4 +
`alsa`/`wasapi`/`coreaudio-rs` + `rubato` 5.0.0 for the non-bit-perfect path.

```
manifest -> MPD parse (dash-mpd) -> segment fetcher (sequential per track, reqwest — see
        playback-behavior.md §3 for why sequential, not an arbitrary concurrency number)
        -> fMP4 reassembly (init + $Number$ segments) -> Symphonia ISO/MP4 reader
        -> Symphonia FLAC/AAC-LC decoder -> ring buffer -> device writer
```

- **Pros:** no C build dependency on any platform, smallest binaries, best mobile story, full
  control of the signal path (transparency panel trivially truthful), all-permissive licences.
- **Cons:** **no HE-AAC** — cannot play `LOW`; you own the DASH assembler, fMP4 reassembly, seek
  logic, buffering policy, and three device backends; no video path at all.
- **FLAC-in-fMP4 in Symphonia is real but undocumented and partial** — see `decoding-and-codecs.md`
  §2 for the three concrete traps (single-entry `stsd`, no gapless support, no `sidx` in TIDAL init
  segments so seeking means jumping `$Number$` segments, not a demuxer-level seek). `[uncertain]`.
- **When it is right:** if streamboat decides to be lossless-only (LOSSLESS + HI_RES_LOSSLESS, no
  `LOW`/`HIGH`, no video), Design C becomes genuinely attractive and sidesteps the AAC patent
  question entirely.

### Recommendation

Start with **Design B (libmpv)** for a correct, gapless, bit-perfect player on all three desktops
and headless with a small amount of code. Structure the codebase so the engine sits behind a narrow
trait — `load(uri, hints)`, `preload(uri, hints)`, `play/pause/seek/stop`, `set_gain(linear)`,
`position()`, `signal_path()`, plus an event stream. If and when the signal path or macOS behaviour
proves inadequate, add **Design A** as a second engine implementation behind the same trait. Do not
build Design C unless the owner decides streamboat is lossless-only.
