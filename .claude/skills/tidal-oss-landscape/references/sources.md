# Sources — project→URL mapping

`ref:<project>/<path>` throughout this skill and the main report
(`/home/user/streamboat/docs/research/oss-landscape.md`) points at a shallow (`--depth 1`) git
clone of the named GitHub project, held read-only under
the reference checkouts (shallow clones of the cited GitHub projects; see references/sources.md)<project>`
in the research environment — not part of the streamboat repo itself. Clones were taken
2026-09-07. Because they are shallow, `git log` on any of them shows exactly one author — that is
an artifact of clone depth, not evidence about the project's real contributor count (see
`verification-notes.md` §J).

A few citations in the main report and this skill point at repositories with **no** local
checkout — cited by URL only, fetched live during the fact-check pass. Those are listed at the
bottom of this file, separately.

| `ref:<project>` | GitHub URL | License | What it is |
| --- | --- | --- | --- |
| `sone` | https://github.com/lullabyX/sone | GPL-3.0-only | Tauri 2 + Rust + React 19 native Linux TIDAL client. The owner's named reference #1; the closest architectural match to streamboat's brief. |
| `sone-windows` | https://github.com/lvllaby/sone-windows | GPL-3.0-only | A stale fork of Sone (v0.16.0 vs upstream 0.21.0) adding a Windows WASAPI2-exclusive audio path and SMTC media controls. Not indexed by the GitHub search API from the research session (exists per web search). |
| `high-tide` | https://github.com/Nokse22/high-tide | GPL-3.0 | Python + GTK4/libadwaita native Linux TIDAL client using `python-tidal`. The owner's named reference #2; highest-star native Linux client; Flathub-shipped. |
| `strawberry` | https://github.com/strawberrymusicplayer/strawberry | GPL-3.0 | C++17 + Qt6 + GStreamer general-purpose music player with TIDAL as one of several streaming backends. The most mature and actively maintained codebase in the set. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | MIT (GitHub reports "other"/NOASSERTION) | Electron wrapper around the TIDAL web player using castlabs' Widevine-enabled Electron fork. The highest-star project in the set; the only one that plays DRM-protected content legitimately. |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | MS-PL | An injector/plugin system that runs *inside* the official TIDAL Electron desktop app (successor to "Neptune"). Intelligence about the official client's internals, not a model to copy — it contains a hardcoded stream-decryption key. |
| `mopidy-tidal` | https://github.com/EbbLabs/mopidy-tidal (formerly `tehkillerbee/mopidy-tidal`) | Apache-2.0 | A Mopidy (headless music server) backend for TIDAL. The set's proof that a TIDAL backend works headlessly; its Range-capable caching HTTP proxy is unique in the set. |
| `python-tidal` (`tidalapi`) | https://github.com/EbbLabs/python-tidal (formerly `tamland/python-tidal`) | LGPL-3.0-or-later | The de-facto unofficial-API client library for Python; reused by High Tide and mopidy-tidal. The authoritative endpoint/manifest reference — read it, do not port it (LGPL). |
| `tidal-connect` | https://github.com/GioF71/tidal-connect | MIT (wrapper only) | A Docker wrapper around a proprietary, closed-source TIDAL Connect receiver binary shipped with an iFi Audio device certificate. The only "be a Connect target" precedent, and it is not open source underneath. |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | Apache-2.0 | TIDAL's **official** web SDK monorepo (auth, api, player, common, event-producer, true-time, template, player-web-components). Vendors the official OpenAPI v2 spec. |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | Apache-2.0 | TIDAL's **official** Android SDK. ExoPlayer playback, `EncryptedSharedPreferences` tokens, the canonical `ManifestMimeType` enum, and the offline validity-window model. |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | Apache-2.0 | TIDAL's **official** iOS SDK. The only complete, officially-sanctioned offline-download implementation (`Offliner` module) in the set. |
| `python-tidal` — see above | | | |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | MIT | A Node/TypeScript CLI using the **official** SDK (`@tidal-music/api`/`@tidal-music/auth`), PKCE, loopback redirect. Its own `previewReason` field is direct evidence the official playback path returns previews. |
| `tidalt` | https://github.com/Benehiko/tidalt | Apache-2.0 | A Go daemon + Bubble Tea TUI, unofficial API, hand-written bit-perfect ALSA output via cgo/FFmpeg. The only daemon/client-split precedent in the set — closest to streamboat's required desktop+headless shape (minus a GUI). README states it was written almost entirely with LLM coding assistants. |
| `tidalrs` | https://github.com/phayes/tidalrs | MIT | A Rust API-client library (manifest/URL resolution only — no decrypt, download or playback). The single most reusable dependency for a Rust streamboat; young (19★, one author). |
| `tidalswift` (TidalSwift) | https://github.com/melgu/TidalSwift | **None — no LICENSE file** | Swift/SwiftUI macOS+iOS client. Read for architecture only; **do not copy code**, it is effectively all-rights-reserved. |
| `libopentidal` | https://github.com/Fokka-Engineering/libopenTIDAL | MIT | Dead (2021) ANSI C + libcurl client library. Documents a fifth manifest-fetch variant (`playbackinfoprepaywall`) not seen elsewhere. |
| `tidalgo` | https://github.com/tcpj/tidalgo | None stated | Dead (2018) Go client. Historical evidence that stream encryption is client-ID-dependent (WiMP key vs standard TIDAL key). |
| `dotnet-tidal-usdk` | https://github.com/SacredSkull/dotnet-tidal-usdk | MIT + explicit anti-piracy clause | Dead (2020) C#/.NET client. The licence-with-a-piracy-clause is a precedent worth considering for streamboat's own licence text. |
| `tidal-api-docs` | https://github.com/gkasdorf/Tidal-API-Docs | None | Four markdown files of community PKCE-flow notes. No streaming documentation. |
| `tidal-fokka-engineering-` (TIDAL) | https://github.com/Fokka-Engineering/TIDAL | MIT | Two files of reverse-engineering notes; the author's explicit disclaimer against building "copyright-infringing apps" is worth quoting in streamboat's own legal section. |

## External references (no local checkout — cited by URL only)

Found and fetched during the fact-check pass; not part of the `ref/` scratch directory, and not
governed by any of the licence obligations above (only the licence stated on the linked repo
applies, and even that only if code is actually copied — none of it is recommended to be copied
verbatim here):

| Project | URL | Cited for |
| --- | --- | --- |
| librespot | https://github.com/librespot-org/librespot | Core+`Sink`-trait+many-clients architecture precedent for a pure-Rust audio stack (Spotify, not TIDAL). See `audio-engineering.md` §5. |
| CamillaDSP | https://github.com/HEnquist/camilladsp | Rust CoreAudio hog-mode exclusive output — the macOS precedent this TIDAL-specific landscape otherwise lacks. See `audio-engineering.md` §6. |
| MPD | https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/src/output/plugins/OSXOutputPlugin.cxx | C++ CoreAudio hog-mode reference implementation. See `audio-engineering.md` §6. |
| Sone's Flathub manifest | https://raw.githubusercontent.com/flathub/io.github.lullabyX.sone/master/io.github.lullabyX.sone.yml | The actual shipped Flatpak manifest (not in the `ref:sone` checkout — Flathub manifests live in a separate `flathub/<app-id>` repo). See `packaging-distribution.md` §2. |
| RustAudio/cpal issue #459 | https://github.com/RustAudio/cpal/issues/459 | Confirms `cpal` has no WASAPI exclusive-mode support. |
| wasapi-rs | https://github.com/HEnquist/wasapi-rs | Standalone Rust crate that does support WASAPI exclusive mode (what CamillaDSP uses). |

## Domains blocked from this research environment

The entire `tidal.com` domain — not only `developer.tidal.com` and `support.tidal.com` — returns
`EGRESS_BLOCKED` from this environment's proxy. `https://tidal.com/content-guidelines` is blocked
exactly like the developer subdomain. Every quote from TIDAL's own policy text anywhere in this
skill or the main report is therefore second-hand (a GitHub discussion quoting the guidelines
verbatim, or a search-result summary) — re-check directly, from an unproxied network, before
citing any of it in user-facing legal text. See `verification-notes.md` and the main report's
intro for the full caveat.
