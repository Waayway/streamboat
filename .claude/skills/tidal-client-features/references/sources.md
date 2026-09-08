# Sources — project→URL mapping

`ref:<project>/<path>` throughout this skill and the main report points at a shallow git clone of
the named GitHub project, held read-only in a scratch directory outside the streamboat repo
(`ref/<project>` under the research session's scratchpad). These clones are not part of the
streamboat repo itself. Below is every project referenced, its GitHub URL, the exact commit each
checkout is pinned at, what it is, and what it's cited for in this skill set.

**Every `ref:<project>/<path>:<line>` citation in this skill set is only reproducible against the
commit in the "Clone (commit, date)" column below** — line numbers drift as upstream repos move.
If you re-clone a project (e.g. `git clone` without pinning a ref) and a cited line range looks
wrong, `git checkout <commit>` first before concluding the citation is broken; only file it as
drift if the line is also wrong at the pinned commit.

| `ref:<project>` | GitHub URL | Clone (commit, date) | What it is | Cited for |
| --- | --- | --- | --- | --- |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | `d8cd6bc2`, 2026-09-02 | An Electron mod/plugin loader for the official TIDAL desktop app. Its `redux/types` directory is a **type-only dump of the official desktop client's Redux store and action list**, extracted from the shipping build (`v1.16.6-beta`, same commit). | The single strongest source: desktop action namespace, store shape, quality ladder, content models, context menus, official desktop API endpoints. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | `b1326db2`, 2026-08-18 | An Electron wrapper around the TIDAL web player for Linux, with DOM scraping, a REST control API, and documentation of platform quirks. | Live DOM selectors, keyboard shortcuts (via a third-party aggregator), quality-badge shape, Chromium/PipeWire audio notes, Widevine bug notes, UI-redesign detection. |
| `python-tidal` (`tidalapi`) | https://github.com/EbbLabs/python-tidal (formerly `tamland/python-tidal`) | `9c41fbe6`, 2026-08-14 | A Python library for the unofficial v1 TIDAL API. Widely reused by other OSS clients. | Endpoint catalogue for pages, search, album/artist/track/playlist/mix, sort-order enums, doctest-verified category titles. |
| `high-tide` | https://github.com/Nokse22/high-tide | `49f24724`, 2026-08-21 | A GTK4/libadwaita native Linux TIDAL client using python-tidal. | Artist-page structure, synced lyrics, MPRIS (Python D-Bus), libsecret token storage, explore page, `tidal://` deep-link grammar. |
| `sone` | https://github.com/lullabyX/sone | `21494b97`, 2026-09-06 | A Tauri (Rust + web UI) native TIDAL client for Linux — the most feature-complete OSS client reviewed. | Play-reporting (`play_log`) pipeline, playlist-folder pagination, v1/v2 Home-feed parsing, MixType values, playlist `accessType` normalisation, `tidal://` grammar, MPRIS via `mpris-server`. |
| `sone-windows` | https://github.com/lvllaby/sone-windows | `f2009f20`, 2026-05-17 | A third-party Windows fork of `lullabyX/sone`, not a port by Sone's own author — its README credits "the original creator **lullabyX** for their incredible work on **lullabyX/Sone**" and separately notes it was "Ported with help from AI agents." | Windows SMTC integration via `souvlaki`; GStreamer WASAPI2 backend. |
| `mopidy-tidal` | https://github.com/EbbLabs/mopidy-tidal (formerly `tehkillerbee/mopidy-tidal`) | `18abb3b8`, 2026-03-16 | A Mopidy (headless music server) backend for TIDAL. | Proven headless browse-tree URI scheme (`tidal:home`, `tidal:explore`, etc.). |
| `strawberry` | https://github.com/strawberrymusicplayer/strawberry | `5d247064`, 2026-09-06 | A Qt desktop music player with a TIDAL backend among many services. | Stream-URL endpoints and the "refuse encrypted manifests" posture streamboat should copy; OAuth redirect URI `tidal://login/auth`. |
| `tidal-connect` | https://github.com/GioF71/tidal-connect | `05ea5d78`, 2026-03-31 | A Docker wrapper that runs the proprietary `tidal_connect_application` binary to act as a TIDAL Connect *target*. | Proof that building a Connect target requires a proprietary binary + vendor certificate; quality-cap history for that implementation. |
| `tidalt` | https://github.com/Benehiko/tidalt | `6cf18c99`, 2026-09-06 | An ALSA-direct headless reference client in Go. | Quality-ladder handling, `hw:` device negotiation, keychain-with-age-fallback token storage. |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | `89244b43`, 2026-08-31 | TIDAL's **official** web SDK monorepo (player, api client). Vendors the full v2 OpenAPI spec. | The official v2 manifest path (`/trackManifests/{id}`), `audioQualityToFormats` mapping, and — most importantly — `packages/api/bin/tidal-api-oas.json`, the complete official OpenAPI 1.10.104 spec (256 paths, 993 schemas). |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | `c93bff40`, 2026-09-03 | TIDAL's **official** Android SDK. Also vendors a copy of the OpenAPI spec, plus generated Kotlin API clients. | Streaming-privileges module (proves server-side one-stream enforcement), generated `CollaborationInvites` client (proves collaborative playlists are a real code-generated surface), ExoPlayer/media3 playback engine, the five-codec `TrackManifestsAttributes` format enum. |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | `787fe964`, 2026-08-24 | TIDAL's official iOS SDK. | iOS playback shape: `AVQueuePlayerWrapper`, `FairPlayLicenseFetcher` (`fp.fa.tidal.com/license`). |
| `dotnet-tidal-usdk` | https://github.com/SacredSkull/dotnet-tidal-usdk | `fed9e61d`, 2020-05-07 | A .NET client library for the unofficial TIDAL API. | Referenced incidentally. |
| `tidalgo` | https://github.com/tcpj/tidalgo | `6e5564ec`, 2018-02-06 | A Go client library for the unofficial TIDAL API. | Referenced incidentally. |
| `tidalswift` | https://github.com/melgu/TidalSwift | `cf0926bb`, 2026-07-08 | A Swift client library for the unofficial TIDAL API. | Referenced for the video-playbackinfo endpoint. |
| `libopentidal` | https://github.com/Fokka-Engineering/libopenTIDAL | `89a1eaa4`, 2021-05-25 | A C client library for the unofficial TIDAL API. | Referenced for auth-flow corroboration. |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | `24f20a85`, 2026-07-01 | A command-line TIDAL client. | Referenced incidentally. |
| `tidalrs` | https://github.com/phayes/tidalrs | `8bb1de80`, 2026-08-31 | A Rust client library for the unofficial TIDAL API. | Referenced for auth-flow corroboration. |
| `tidal-api-docs` | https://github.com/gkasdorf/Tidal-API-Docs | `457e15d2`, 2023-04-08 | A community-maintained doc dump of unofficial TIDAL API notes. | **Not yet consulted by this skill or the main report** — read it before treating any endpoint as fully catalogued; see the coverage-boundary note in the main report. |
| `tidal-fokka-engineering-` | https://github.com/Fokka-Engineering/TIDAL | `9b06cb15`, 2022-02-10 | Reverse-engineering notes on the TIDAL API. | **Not yet consulted** — same caveat as above. |

Dates are the cited commit's own commit date (`git log -1 --format=%ci`), not the day the shallow
clone was made — for an actively developed project (Sone, strawberry, tidalt, TidaLuna itself) that
date is close to the clone date; for the long-dormant libraries (`tidalgo`, `dotnet-tidal-usdk`,
`libopentidal`, `tidal-api-docs`, `tidal-fokka-engineering-`) it reflects the project's last real
commit, sometimes years before this research pass — read as "this is the newest code that exists,"
not "this was checked yesterday."

## Web sources (not reference checkouts)

The main report (`docs/research/tidal-client-features.md`, "Sources" section) carries the full list
of web citations with URLs, evidence tags (`[verified-web]` vs `[uncertain]`), and notes on which
TIDAL-owned domains are blocked from direct fetch in the research environment
(`support.tidal.com`, `tidal.com`, `developer.tidal.com` — cited via search-engine summaries, not
full reads, except where the underlying OpenAPI JSON is vendored directly as noted above). Re-check
that list before treating any `[verified-web]` or `[uncertain]` claim as final.
