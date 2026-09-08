# Sources — project→URL mapping

`ref:<project>/<path>` throughout this skill and the main report points at a shallow git clone of
the named GitHub project, held read-only in a scratch directory outside the streamboat repo
(`ref/<project>` under the research session's scratchpad). These clones are not part of the
streamboat repo itself. Below is every project referenced, its GitHub URL, what it is, and what
it's cited for in this skill set.

| `ref:<project>` | GitHub URL | What it is | Cited for |
| --- | --- | --- | --- |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | An Electron mod/plugin loader for the official TIDAL desktop app. Its `redux/types` directory is a **type-only dump of the official desktop client's Redux store and action list**, extracted from the shipping build (`v1.16.6-beta`, commit `d8cd6bc`, 2026-09-02). | The single strongest source: desktop action namespace, store shape, quality ladder, content models, context menus, official desktop API endpoints. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | An Electron wrapper around the TIDAL web player for Linux, with DOM scraping, a REST control API, and documentation of platform quirks. | Live DOM selectors, keyboard shortcuts (via a third-party aggregator), quality-badge shape, Chromium/PipeWire audio notes, Widevine bug notes, UI-redesign detection. |
| `python-tidal` (`tidalapi`) | https://github.com/tamland/python-tidal | A Python library for the unofficial v1 TIDAL API. Widely reused by other OSS clients. | Endpoint catalogue for pages, search, album/artist/track/playlist/mix, sort-order enums, doctest-verified category titles. |
| `high-tide` | https://github.com/Nokse22/high-tide | A GTK4/libadwaita native Linux TIDAL client using python-tidal. | Artist-page structure, synced lyrics, MPRIS (Python D-Bus), libsecret token storage, explore page, `tidal://` deep-link grammar. |
| `sone` | https://github.com/lullabyX/sone | A Tauri (Rust + web UI) native TIDAL client for Linux — the most feature-complete OSS client reviewed. | Play-reporting (`play_log`) pipeline, playlist-folder pagination, v1/v2 Home-feed parsing, MixType values, playlist `accessType` normalisation, `tidal://` grammar, MPRIS via `mpris-server`. |
| `sone-windows` | https://github.com/lvllaby/sone-windows | A third-party Windows fork of `lullabyX/sone`, not a port by Sone's own author — its README credits "the original creator **lullabyX** for their incredible work on **lullabyX/sone**" and separately notes it was "Ported with help from AI agents." | Windows SMTC integration via `souvlaki`; GStreamer WASAPI2 backend. |
| `mopidy-tidal` | https://github.com/tehkillerbee/mopidy-tidal | A Mopidy (headless music server) backend for TIDAL. | Proven headless browse-tree URI scheme (`tidal:home`, `tidal:explore`, etc.). |
| `strawberry` | https://github.com/strawberrymusicplayer/strawberry | A Qt desktop music player with a TIDAL backend among many services. | Stream-URL endpoints and the "refuse encrypted manifests" posture streamboat should copy; OAuth redirect URI `tidal://login/auth`. |
| `tidal-connect` | https://github.com/GioF71/tidal-connect | A Docker wrapper that runs the proprietary `tidal_connect_application` binary to act as a TIDAL Connect *target*. | Proof that building a Connect target requires a proprietary binary + vendor certificate; quality-cap history for that implementation. |
| `tidalt` | https://github.com/Benehiko/tidalt | An ALSA-direct headless reference client in Go. | Quality-ladder handling, `hw:` device negotiation, keychain-with-age-fallback token storage. |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | TIDAL's **official** web SDK monorepo (player, api client). Vendors the full v2 OpenAPI spec. | The official v2 manifest path (`/trackManifests/{id}`), `audioQualityToFormats` mapping, and — most importantly — `packages/api/bin/tidal-api-oas.json`, the complete official OpenAPI 1.10.104 spec (256 paths, 993 schemas). |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | TIDAL's **official** Android SDK. Also vendors a copy of the OpenAPI spec, plus generated Kotlin API clients. | Streaming-privileges module (proves server-side one-stream enforcement), generated `CollaborationInvites` client (proves collaborative playlists are a real code-generated surface). |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | TIDAL's official iOS SDK. | Referenced incidentally for the iOS playback shape (AVQueuePlayer, FairPlay). |
| `dotnet-tidal-usdk` | https://github.com/SacredSkull/dotnet-tidal-usdk | A .NET client library for the unofficial TIDAL API. | Referenced incidentally. |
| `tidalgo` | https://github.com/tcpj/tidalgo | A Go client library for the unofficial TIDAL API. | Referenced incidentally. |
| `tidalswift` | https://github.com/melgu/TidalSwift | A Swift client library for the unofficial TIDAL API. | Referenced for the video-playbackinfo endpoint. |
| `libopentidal` | https://github.com/Fokka-Engineering/libopenTIDAL | A C client library for the unofficial TIDAL API. | Referenced for auth-flow corroboration. |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | A command-line TIDAL client. | Referenced incidentally. |
| `tidalrs` | https://github.com/phayes/tidalrs | A Rust client library for the unofficial TIDAL API. | Referenced for auth-flow corroboration. |
| `tidal-api-docs` | https://github.com/gkasdorf/Tidal-API-Docs | A community-maintained doc dump of unofficial TIDAL API notes. | **Not yet consulted by this skill or the main report** — read it before treating any endpoint as fully catalogued; see the coverage-boundary note in the main report. |
| `tidal-fokka-engineering-` | https://github.com/Fokka-Engineering/TIDAL | Reverse-engineering notes on the TIDAL API. | **Not yet consulted** — same caveat as above. |

## Web sources (not reference checkouts)

The main report (`docs/research/tidal-client-features.md`, "Sources" section) carries the full list
of web citations with URLs, evidence tags (`[verified-web]` vs `[uncertain]`), and notes on which
TIDAL-owned domains are blocked from direct fetch in the research environment
(`support.tidal.com`, `tidal.com`, `developer.tidal.com` — cited via search-engine summaries, not
full reads, except where the underlying OpenAPI JSON is vendored directly as noted above). Re-check
that list before treating any `[verified-web]` or `[uncertain]` claim as final.
