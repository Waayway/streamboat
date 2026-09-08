# Sources — project→URL mapping

`ref:<project>/<path>` throughout this skill and `docs/research/audio-pipeline.md` points at a
shallow git clone of the named GitHub project, held read-only in a scratch directory outside the
streamboat repo (`ref/<project>` under the research session's scratchpad — not part of this repo,
and not guaranteed to still be present in a later session; re-clone at the commit below to
re-verify a citation). Line numbers drift release to release — a citation off by a handful of lines
is almost always clone drift, not an error; prefer the named function/constant when both are given.

## Reference checkouts (`ref:<project>`)

| `ref:<project>` | GitHub URL | Commit read | What it is, for audio-pipeline purposes |
| --- | --- | --- | --- |
| `sone` | https://github.com/lullabyX/sone | `21494b9` | Tauri (Rust) native client. **The single richest audio-pipeline reference in the set** — full ALSA `hw_params`/`sw_params` negotiation, bit-perfect format promotion, `concat`-based gapless (which does not drain on end-of-track — its DirectAlsa writer already keeps the PCM open across tracks, `audio.rs:1169-1200`), signal-path transparency panel, encrypted metadata cache, rate-limit gate, MPRIS, additive idle-inhibit. `ref:sone/src-tauri/src/audio.rs` alone is ~3300 lines. `src/tidal_report/{event,mod}.rs` is the full play-reporting implementation (`ec.tidal.com/api/event-batch`, 30 s threshold, device-identity pinning). |
| `sone-windows` | https://github.com/lvllaby/sone-windows | `f2009f2` | Windows port of Sone. `wasapi2sink exclusive`/`low-latency`/`device`, live exclusive-mode toggle via Ready→Playing→re-seek, souvlaki 0.8.3. **Also the only documented GStreamer-on-Windows bundling recipe in the set** (`README.md:95-155` — NSIS/WiX hooks, exact DLL/plugin list, no AAC decoder bundled) and the only SMTC-needs-a-real-HWND finding (`src/media_controls.rs:14-46`); `src/idle_inhibit.rs` is Linux-only, confirming no reference project handles Windows/macOS idle inhibition. |
| `high-tide` | https://github.com/Nokse22/high-tide | `49f2472` | GTK4/libadwaita Linux client, GStreamer `playbin3`. Gapless via `about-to-finish` (disabled on `pipewiresink`, reason unknown), ReplayGain via `rgvolume`/`rglimiter`, whole-track caching **capped at 5 GB with atime-LRU eviction** (`src/window.py:223`, `src/lib/utils.py:76-78,828-843` — corrects an earlier "no size cap" claim), Flatpak manifest (no `/dev/snd`), local-file MPRIS artwork (`src/mpris.py:428-458`). Its cache MPD is written to one fixed shared path per session, unsafe under prefetch (`src/lib/player_object.py:494-513`). |
| `strawberry` | https://github.com/strawberrymusicplayer/strawberry | `5d24706` | Qt6 multi-service player, GStreamer. Generic `exclusive`-property handling across sinks, `hw:`/`plughw:` ⇒ exclusive inference, `directsoundsink` promoted over `wasapi(2)sink` on Windows, crossfade gated off in exclusive mode, real-time thread priority (`SCHED_RR`/`THREAD_PRIORITY_HIGHEST`), four v1 endpoint variants, encryption refusal. |
| `tidalt` | https://github.com/Benehiko/tidalt | `6cf18c9` | Go TUI/daemon, FFmpeg + raw ALSA via CGO. `org.freedesktop.ReserveDevice1` handshake (the canonical implementation to copy), split open/configure device-busy-vs-format-refused handling, single-owner-daemon client/server architecture, Docker headless recipe, hold-device-only-while-playing policy. |
| `mopidy-tidal` | https://github.com/tehkillerbee/mopidy-tidal | `18abb3b` | Headless Mopidy backend. Localhost HTTP relay cache proxy with SQLite chunked insertion and full `Range`/`Content-Range` support — the model for resuming a direct-URL stream after a mid-track failure. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | `b1326db` | Electron wrapper, castlabs Widevine-capable Electron fork. `--audio-output-sample-rate=192000` + `AudioServiceOutOfProcess`/`AudioServiceSandbox` disabling (opt-in, not default) — the negative example for why a browser-rendered-audio path cannot be bit-perfect. |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | `d8cd6bc` | Mod/plugin loader running *inside* the official desktop client. `encryptionType` value vocabulary (`NONE`/`OLD_AES` — the only file anywhere in the reference set that attests this vocabulary; cited as fact only, never to copy the decryptor); corrected finding: its `Semaphore(2)` bounds concurrent **track** fetches, not segment fetches, which are strictly sequential per track. |
| `python-tidal` | https://github.com/tamland/python-tidal | `9c41fbe` | The de-facto reference unofficial-API client. `Quality`/`AudioMode`/`MediaMetadataTags`/`ManifestMimeType` enums, `DashInfo` MPD field extraction (does *not* read `Representation@id`), MPD→HLS synthesis, `$Number$` segment numbering starting at 0 — with a second, independent bug beyond the start-index one: it under-counts `SegmentTimeline/S@r` by one segment per run (adds `r`, not `r+1`; `tidal-manifest-api.md` §4). No `HI_RES` value remains in its `Quality` enum (removed, MQA-related). |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | `24f20a8` | Plays via the **official** v2 API with a registered public client. v2 `/trackManifests/{id}` call shape, `<BaseURL>`-only MPD fallback parsing, `$Number$` segment numbering starting at 1, shells out to `mpv`/`afplay`. |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | `89244b4` | TIDAL's official web SDK. The single richest source for manifest-API parameters, sub-status→error mapping, Shaka player config/DRM, the 250 ms micro-crossfade "gapless", the proprietary native-player device-mode/error vocabulary, `streamingSessionId` generation (`generate-guid.ts`), `MANIFEST_EXPIRATION_MS`. |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | `c93bff4` | Official Android SDK, ExoPlayer/media3. Canonical loudness-normalization formula and defaults, `BufferConfiguration` defaults, v2 request-building, `EAC3_JOC` ⇒ `DOLBY_ATMOS` mapping. |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | `787fe96` | Official iOS SDK. Tier→codec mapping table, `AudioCodec.swift`'s ALAC/AC-4/360RA-unsupported comments, FairPlay. |
| `tidal-connect` | https://github.com/GioF71/tidal-connect | `05ea5d7` | Docker wrapper around a closed-source TIDAL Connect binary. Raspberry Pi HDMI/I2S-HAT ALSA configs, card-name-not-index device selection, ALSA softvol mixer-naming collision warning. **Correction: it supports up to 24/48 hi-res — only MQA self-unfolding (to 24/88-24/96) was lost in July 2024, it was never "LOSSLESS only."** |
| `libopentidal` | https://github.com/Fokka-Engineering/libopenTIDAL | `89a1eaa` | C library. Marketing bit-rate figures cross-reference only. |

Projects present in the research checkout set but not separately cited in this skill's audio-pipeline
references (they are covered in the sibling `tidal-api` skill instead): `tidalrs`, `tidalgo`,
`dotnet-tidal-usdk`, `tidalswift`, `tidal-api-docs`, `tidal-fokka-engineering-`.

Clone date for all rows above: 2026-09-07.

## Upstream source read directly (not a `ref:` checkout — raw file fetch)

| URL | Supports |
| --- | --- |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-bad/sys/wasapi2/gstwasapi2sink.cpp` | `wasapi2sink`'s real `exclusive` property, tagged `Since: 1.28` |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/sys/osxaudio/gstosxaudiosink.c` | `osxaudiosink` has no exclusive/hog property |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/sys/osxaudio/gstosxcoreaudiohal.c` | hog mode / physical-format code exists but is called only from `_open_spdif` |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-base/ext/alsa/gstalsasink.c` | `alsasink` has only `device`/`device-name`/`card-name` |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/gst/isomp4/qtdemux.c` + `fourcc.h` | `FOURCC_fLaC` handling, `audio/x-flac` caps |
| `https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavformat/isom_tags.c` + `.../mov.c` | `fLaC`→`AV_CODEC_ID_FLAC`, `dfLa` box parsing |
| `https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavcodec/ac3_parser.c`, `.../ac3dec.c`, `.../eac3dec.c` | E-AC-3 decoder discards JOC; parser can only *detect* Atmos presence |
| `https://raw.githubusercontent.com/mpv-player/mpv/master/DOCS/man/options.rst` | `--audio-exclusive` scope, `--gapless-audio`, `--audio-spdif`, `--prefetch-playlist`, `--volume-gain`/`--replaygain-clip`, `--demuxer-lavf-o` |
| `https://raw.githubusercontent.com/mpv-player/mpv/master/DOCS/man/ao.rst` | `coreaudio_exclusive` AO, `--coreaudio-change-physical-format`, `--wasapi-exclusive-buffer`, `--alsa-*` options |
| `https://raw.githubusercontent.com/mpv-player/mpv/master/DOCS/man/input.rst` | `ao-volume` vs `volume`, `audio-out-params`/`current-ao`/`audio-device-list` |
| `https://raw.githubusercontent.com/mpv-player/mpv/master/Copyright` | GPLv2+ default, `-Dgpl=false` LGPL build (not `--enable-lgpl`), LGPL-mode caveats |
| `https://raw.githubusercontent.com/pdeljanov/Symphonia/master/README.md` | codec/format support matrix |
| `https://raw.githubusercontent.com/pdeljanov/Symphonia/master/symphonia-format-isomp4/src/atoms/{mod,stsd}.rs` | undocumented FLAC-in-fMP4 (`AtomType::Flac`/`FlacAtom`) support and its limits |
| `https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/device.rs` | hardcodes `AUDCLNT_SHAREMODE_SHARED` at four call sites (lines 648, 726, 879, 982), no exclusive path |
| `gst-plugins-bad/ext/dash/gstdashdemux.c:446` (`GST_RANK_PRIMARY`) and `gst-plugins-good/ext/adaptivedemux2/dash/gstdashdemux.c:4210` (`GST_RANK_PRIMARY + 1`), both in the GStreamer monorepo `subprojects/` | `dashdemux2` outranks legacy `dashdemux` by default — plain `uridecodebin`/`playbin` autoplugs `dashdemux2`, not the legacy element, whenever `adaptivedemux2` is installed (`output-backends.md` §12) |
| `https://github.com/RustAudio/cpal/issues/459` | WASAPI exclusive-mode request, opened 2020, closed unimplemented |
| `https://github.com/HEnquist/camilladsp/blob/master/backend_coreaudio.md` | CoreAudio hog-mode flag, rate-change notification handling, S16/S24/S32/F32 physical formats |
| `https://github.com/Sinono3/souvlaki` (README + `Cargo.toml` @ 0.8.3) | one `MediaControls` API for MPRIS/SMTC/NowPlayingInfoCenter; macOS needs an AppDelegate/winit loop; `edition = "2024"` vs. crates.io MSRV 1.67 |
| `https://crates.io/api/v1/crates/{gstreamer,rodio,kira,symphonia,cpal,souvlaki,libmpv2,mpris-server,alsa,wasapi,coreaudio-rs,dash-mpd,stream-download,oboe,ffmpeg-next,rubato}` | crate versions/licences/MSRV quoted throughout, queried 2026-09-07, re-verified 2026-09-08: every version/licence/date matches exactly. Two MSRVs not previously stated: `wasapi` 0.24.0 declares `rust-version` 1.76; `stream-download` 0.24.4 declares 1.91.0 — the highest MSRV in the whole dependency set, above even `gstreamer` 0.25.3's 1.92 only by comparison to `souvlaki`'s effective 1.85 floor (`os-integration.md` §1). Effective per-design MSRV: Design A >= 1.92 (gstreamer), Design C >= 1.91 (stream-download) if used, Design B ~1.85 (libmpv2/souvlaki) — check each against the Flatpak SDK's and Debian trixie's shipped Rust before committing. |
| `https://packages.debian.org/trixie/gstreamer1.0-plugins-bad`, `.../source/trixie/gstreamer1.0` | Debian 13 ships GStreamer 1.26.2 — below the FLAC-in-DASH floor |

## Web sources (news/reporting, not a direct source-code read)

| URL | Supports |
| --- | --- |
| `https://linuxiac.com/gstreamer-1-26-10-brings-fixes-for-flac-opus-and-matroska-handling/`, `https://9to5linux.com/gstreamer-1-26-10-released-with-support-for-flac-audio-in-dash-manifests` | GStreamer 1.26.10 added FLAC-in-DASH support (exact commit unverified) |
| `https://www.phoronix.com/news/GStreamer-1.28`, `https://lists.freedesktop.org/archives/gstreamer-devel/2026-January/082207.html` | GStreamer 1.28.0 (27 Jan 2026) — wasapi2 IMMDevice port, dashdemux2 gap-seek fix |
| `https://linuxiac.com/gstreamer-1-28-3-released-with-security-and-playback-fixes`, `https://9to5linux.com/gstreamer-1-28-3-adds-nxp-i-mx-8m-plus-hardware-accelerated-h-265-encoding` | GStreamer 1.28.3: `devicemonitor` now waits for its start thread before listing devices |
| `https://linuxiac.com/gstreamer-1-28-5-released-with-security-and-playback-fixes/`, `https://linuxiac.com/gstreamer-1-28-6-adds-h-266-mp4-muxing-and-ffmpeg-9-0-support/`, `https://9to5linux.com/gstreamer-1-28-6-adds-h-266-muxing-support-to-the-rust-mp4-muxers`, `linuxcompatible.org` ("GStreamer 1.28.6 Drops: Final 1.28 Release") | **Corrects a prior "1.28.2/1.28.3 is current" statement in this skill.** The 1.28 series has shipped through 1.28.5 and then **1.28.6** (5 Aug 2026), the final 1.28 bug-fix release; 1.28.6 adds FFmpeg 9.0 support, matching the `ffmpeg-next` 9.0.0 crate recommendation |
| `https://www.digitaltrends.com/home-theater/tidal-killing-mqa-sony-360-reality-audio/`, `https://www.whathifi.com/news/tidal-scraps-mqa-and-spatial-audio-format-heres-what-that-means-for-subscribers`, `techradar.com/audio/audio-streaming/tidals-waving-goodbye-to-mqa-and-sony-360-reality-audio-heres-what-you-need-to-know` | MQA replaced with FLAC, all 360RA content removed, 24 July 2024; tidal-connect's own README (`ref:tidal-connect/README.md:59`) independently corroborates and clarifies that only MQA self-unfolding was lost, not hi-res itself |
| `https://ipfray.com/access-advance-via-la-position-multimedia-patent-pools-for-further-growth-with-price-stability-offer-regionalization-for-more-standards/`, `via-la.com/aac-license-fees-structures/`, `lumenci.com/blogs/aac-licensing-explained-patent-pools-royalty-obligations-and-sep-exposure/`, `scconline.com` (2026-02-13 Via Licensing Alliance analysis) | AAC/Via LA patent pool is an active, tiered per-unit programme in 2026 ($0.98/$0.78/$0.68/$0.45 by volume tier, 900+ licensees) — reached only via secondary reporting, `via-la.com`'s own programme page is blocked from this environment |
| `https://docs.pipewire.org/page_audio.html` | PipeWire passthrough-mode / `default.clock.*` — re-check exact config keys before publishing |
| `https://issues.chromium.org/issues/40944208` | **[uncertain]** — titled about WebAudio/AudioContext resampling specifically, not every Chromium audio path; issue body unreadable from this environment |
| `https://support.tidal.com/hc/en-us/articles/25876825185425-Audio-Format-Updates`, `.../360004255778-Dolby-Atmos` | TIDAL's own format/Atmos pages — **referenced but never read; `support.tidal.com` is blocked from the research environment** |

Blocked hosts whose content in this skill is second-hand only: `support.tidal.com`, `tidal.com`,
`gstreamer.freedesktop.org`, `issues.chromium.org`, `via-la.com` (programme page itself; rate figures
corroborated by the third-party URLs above), `docs.pipewire.org` (summarized, not directly quoted —
re-verify exact config-key names before shipping them in user docs).
