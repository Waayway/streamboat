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
| `sone` | https://github.com/lullabyX/sone | `21494b9` | Tauri (Rust) native client. **The single richest audio-pipeline reference in the set** — full ALSA `hw_params`/`sw_params` negotiation, bit-perfect format promotion, `concat`-based gapless, signal-path transparency panel, encrypted metadata cache, rate-limit gate, MPRIS, additive idle-inhibit. `ref:sone/src-tauri/src/audio.rs` alone is ~3300 lines. |
| `sone-windows` | https://github.com/lvllaby/sone-windows | `f2009f2` | Windows port of Sone. `wasapi2sink exclusive`/`low-latency`/`device`, live exclusive-mode toggle via Ready→Playing→re-seek, souvlaki 0.8.3. |
| `high-tide` | https://github.com/Nokse22/high-tide | `49f2472` | GTK4/libadwaita Linux client, GStreamer `playbin3`. Gapless via `about-to-finish` (disabled on `pipewiresink`, reason unknown), ReplayGain via `rgvolume`/`rglimiter`, whole-track caching, Flatpak manifest (no `/dev/snd`). |
| `strawberry` | https://github.com/strawberrymusicplayer/strawberry | `5d24706` | Qt6 multi-service player, GStreamer. Generic `exclusive`-property handling across sinks, `hw:`/`plughw:` ⇒ exclusive inference, `directsoundsink` promoted over `wasapi(2)sink` on Windows, crossfade gated off in exclusive mode, real-time thread priority (`SCHED_RR`/`THREAD_PRIORITY_HIGHEST`), four v1 endpoint variants, encryption refusal. |
| `tidalt` | https://github.com/Benehiko/tidalt | `6cf18c9` | Go TUI/daemon, FFmpeg + raw ALSA via CGO. `org.freedesktop.ReserveDevice1` handshake (the canonical implementation to copy), split open/configure device-busy-vs-format-refused handling, single-owner-daemon client/server architecture, Docker headless recipe, hold-device-only-while-playing policy. |
| `mopidy-tidal` | https://github.com/tehkillerbee/mopidy-tidal | `18abb3b` | Headless Mopidy backend. Localhost HTTP relay cache proxy with SQLite chunked insertion and full `Range`/`Content-Range` support — the model for resuming a direct-URL stream after a mid-track failure. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | `b1326db` | Electron wrapper, castlabs Widevine-capable Electron fork. `--audio-output-sample-rate=192000` + `AudioServiceOutOfProcess`/`AudioServiceSandbox` disabling (opt-in, not default) — the negative example for why a browser-rendered-audio path cannot be bit-perfect. |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | `d8cd6bc` | Mod/plugin loader running *inside* the official desktop client. `encryptionType` value vocabulary (`NONE`/`OLD_AES` — cited as fact only, never to copy the decryptor); corrected finding: its `Semaphore(2)` bounds concurrent **track** fetches, not segment fetches, which are strictly sequential per track. |
| `python-tidal` | https://github.com/tamland/python-tidal | `9c41fbe` | The de-facto reference unofficial-API client. `Quality`/`AudioMode`/`MediaMetadataTags`/`ManifestMimeType` enums, `DashInfo` MPD field extraction (does *not* read `Representation@id`), MPD→HLS synthesis, `$Number$` segment numbering starting at 0. |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | `24f20a8` | Plays via the **official** v2 API with a registered public client. v2 `/trackManifests/{id}` call shape, `<BaseURL>`-only MPD fallback parsing, `$Number$` segment numbering starting at 1, shells out to `mpv`/`afplay`. |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | `89244b4` | TIDAL's official web SDK. The single richest source for manifest-API parameters, sub-status→error mapping, Shaka player config/DRM, the 250 ms micro-crossfade "gapless", the proprietary native-player device-mode/error vocabulary, `streamingSessionId` generation (`generate-guid.ts`), `MANIFEST_EXPIRATION_MS`. |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | `c93bff4` | Official Android SDK, ExoPlayer/media3. Canonical loudness-normalization formula and defaults, `BufferConfiguration` defaults, v2 request-building, `EAC3_JOC` ⇒ `DOLBY_ATMOS` mapping. |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | `787fe96` | Official iOS SDK. Tier→codec mapping table, `AudioCodec.swift`'s ALAC/AC-4/360RA-unsupported comments, FairPlay. |
| `tidal-connect` | https://github.com/GioF71/tidal-connect | `05ea5d7` | Docker wrapper around a closed-source TIDAL Connect binary. Raspberry Pi HDMI/I2S-HAT ALSA configs, card-name-not-index device selection, ALSA softvol mixer-naming collision warning, LOSSLESS-only-since-July-2024. |
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
| `https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/device.rs` | hardcodes `AUDCLNT_SHAREMODE_SHARED`, no exclusive path |
| `https://github.com/RustAudio/cpal/issues/459` | WASAPI exclusive-mode request, opened 2020, closed unimplemented |
| `https://github.com/HEnquist/camilladsp/blob/master/backend_coreaudio.md` | CoreAudio hog-mode flag, rate-change notification handling, S16/S24/S32/F32 physical formats |
| `https://github.com/Sinono3/souvlaki` (README + `Cargo.toml` @ 0.8.3) | one `MediaControls` API for MPRIS/SMTC/NowPlayingInfoCenter; macOS needs an AppDelegate/winit loop; `edition = "2024"` vs. crates.io MSRV 1.67 |
| `https://crates.io/api/v1/crates/{gstreamer,rodio,kira,symphonia,cpal,souvlaki,libmpv2,mpris-server,alsa,wasapi,coreaudio-rs,dash-mpd,stream-download,oboe,ffmpeg-next,rubato}` | crate versions/licences/MSRV quoted throughout, queried 2026-09-07 |
| `https://packages.debian.org/trixie/gstreamer1.0-plugins-bad`, `.../source/trixie/gstreamer1.0` | Debian 13 ships GStreamer 1.26.2 — below the FLAC-in-DASH floor |

## Web sources (news/reporting, not a direct source-code read)

| URL | Supports |
| --- | --- |
| `https://linuxiac.com/gstreamer-1-26-10-brings-fixes-for-flac-opus-and-matroska-handling/`, `https://9to5linux.com/gstreamer-1-26-10-released-with-support-for-flac-audio-in-dash-manifests` | GStreamer 1.26.10 added FLAC-in-DASH support (exact commit unverified) |
| `https://www.phoronix.com/news/GStreamer-1.28`, `https://lists.freedesktop.org/archives/gstreamer-devel/2026-January/082207.html` | GStreamer 1.28.0 (27 Jan 2026) — wasapi2 IMMDevice port, dashdemux2 gap-seek fix. **Stale**: 1.28.2/1.28.3 have since shipped |
| `https://linuxiac.com/gstreamer-1-28-3-released-with-security-and-playback-fixes`, `https://9to5linux.com/gstreamer-1-28-3-adds-nxp-i-mx-8m-plus-hardware-accelerated-h-265-encoding` | GStreamer 1.28.3: `devicemonitor` now waits for its start thread before listing devices |
| `https://www.digitaltrends.com/home-theater/tidal-killing-mqa-sony-360-reality-audio/`, `https://www.whathifi.com/news/tidal-scraps-mqa-and-spatial-audio-format-heres-what-that-means-for-subscribers` | MQA replaced with FLAC, all 360RA content removed, 24 July 2024 |
| `https://www.via-la.com/licensing-programs/aac/`, `https://ipfray.com/access-advance-via-la-position-multimedia-patent-pools-for-further-growth-with-price-stability-offer-regionalization-for-more-standards/` | AAC/Via LA patent pool is an active per-unit programme in 2026 |
| `https://docs.pipewire.org/page_audio.html` | PipeWire passthrough-mode / `default.clock.*` — re-check exact config keys before publishing |
| `https://issues.chromium.org/issues/40944208` | **[uncertain]** — titled about WebAudio/AudioContext resampling specifically, not every Chromium audio path; issue body unreadable from this environment |
| `https://support.tidal.com/hc/en-us/articles/25876825185425-Audio-Format-Updates`, `.../360004255778-Dolby-Atmos` | TIDAL's own format/Atmos pages — **referenced but never read; `support.tidal.com` is blocked from the research environment** |

Blocked hosts whose content in this skill is second-hand only: `support.tidal.com`, `tidal.com`,
`gstreamer.freedesktop.org`, `issues.chromium.org`, `via-la.com` (programme page itself; rate figures
corroborated by the third-party URLs above), `docs.pipewire.org` (summarized, not directly quoted —
re-verify exact config-key names before shipping them in user docs).
