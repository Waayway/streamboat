# Sources — project→URL mapping

`ref:<project>/<path>` throughout this skill and `docs/research/tidal-api.md` points at a shallow
git clone of the named GitHub project, held read-only in a scratch directory outside the streamboat
repo (`ref/<project>` under the research session's scratchpad — not part of this repo, and not
guaranteed to still be present in a later session; re-clone at the commit below to re-verify a
citation). Line numbers drift release to release — a citation off by a handful of lines is almost
always clone drift, not an error; prefer the named function/constant when both are given.

| `ref:<project>` | GitHub URL | Commit read | What it is |
| --- | --- | --- | --- |
| `python-tidal` | https://github.com/EbbLabs/python-tidal (current home; the README still links `tamland/python-tidal`, the project's earlier home) | `9c41fbe` | The de-facto reference unofficial-API client (Python, LGPL-3.0-or-later, v0.8.11). Nearly every endpoint in this skill was first confirmed here. |
| `high-tide` | https://github.com/Nokse22/high-tide | `49f2472` | GTK4/libadwaita Linux client built on python-tidal. PKCE-only login, libsecret storage, Flatpak packaging. |
| `sone` | https://github.com/lullabyX/sone | `21494b9` | Tauri (Rust) native client — the most defensively-engineered TIDAL API layer reviewed (rate gate, sub-status classification, play reporting). Primary source for request-hardening patterns. |
| `sone-windows` | https://github.com/lvllaby/sone-windows | `f2009f2` | Windows port of Sone; same API layer, WASAPI/SMTC specifics. |
| `strawberry` | https://github.com/strawberrymusicplayer/strawberry | `5d24706` | Qt6 multi-service player. Source of the "refuse encrypted streams" posture and the legacy `api.tidalhifi.com` host. |
| `mopidy-tidal` | https://github.com/tehkillerbee/mopidy-tidal | `18abb3b` | Headless Mopidy backend. Source for the caching-proxy pattern and proof that CDN URLs support HTTP Range. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | `b1326db` | Electron wrapper around the official web player with a Widevine-capable (castlabs) Electron build — proves the "wrap the sanctioned Player module" architecture. |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | `d8cd6bc` | A mod/plugin loader running *inside* the official desktop client. Contains an `OLD_AES` decryptor — cited here only as a documented fact, never as something to copy. |
| `tidalt` | https://github.com/Benehiko/tidalt | `6cf18c9` | Go TUI client, FFmpeg + raw ALSA. Plaintext-with-comment client credentials. |
| `tidalrs` | https://github.com/phayes/tidalrs | `8bb1de8` | Rust library; caller supplies the client id (no embedded credential). Single-flight refresh via `Semaphore`. |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | `24f20a8` | The only reference project that plays audio through the **official** Developer API, with its own registered public client. |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | `89244b4` | TIDAL's official web SDK monorepo (auth, player, api, event-producer, true-time). Vendors the full official OpenAPI types (`allAPI.generated.ts`, 256 paths). The single richest source for official-API and sub-status detail. |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | `c93bff4` | Official Android SDK. `ApiError.kt` is the canonical name-per-sub-status source. Also the streaming-privileges module. |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | `787fe96` | Official iOS SDK. Codec-mapping comments (Atmos, 360RA, MQA), FairPlay, `OfflineEngine` states. |
| `tidalgo` | https://github.com/tcpj/tidalgo | `6e5564e` | Dead (2018-era) Go client using username/password + `x-tidal-token`. Retained as history for why client identity determines encryption. |
| `dotnet-tidal-usdk` | https://github.com/SacredSkull/dotnet-tidal-usdk | `fed9e61` | Dead .NET client, same legacy era as tidalgo. |
| `libopentidal` | https://github.com/Fokka-Engineering/libopenTIDAL | `89a1eaa` | C library. Source of the proactive-refresh-margin pattern and the `/v1/logout` endpoint documentation. |
| `tidalswift` | https://github.com/melgu/TidalSwift | `cf0926b` | Swift client. 5-minute proactive refresh margin, corroborates libopenTIDAL. |
| `tidal-api-docs` | https://github.com/gkasdorf/Tidal-API-Docs | `457e15d` | Community-maintained unofficial-API doc dump. Source for runtime client-id discovery and the "client_id may change any time" warning. |
| `tidal-connect` | https://github.com/GioF71/tidal-connect | `05ea5d7` | Docker wrapper around a closed-source ARM binary TIDAL ships only to hardware partners. Proof that TIDAL Connect cannot be implemented from the outside. |
| `tidal-fokka-engineering-` | https://github.com/Fokka-Engineering/TIDAL | `9b06cb1` | Reverse-engineering write-up (docTIDAL). Source for "the browser OAuth flow is reCAPTCHA v3 protected." |

Clone date for all rows: 2026-09-07.

## Web sources (not reference checkouts)

`docs/research/tidal-api.md`'s "Sources" section carries the full list with URLs. Notable blocked
hosts whose content is second-hand (search-result excerpts / third-party mirrors, not a direct
fetch): `developer.tidal.com`, `support.tidal.com`, `tidal.com`, `flathub.org`,
`tidal-music.github.io`, `forum.strawberrymusicplayer.org`, `torrentfreak.com`. Re-verify anything
quoted from those hosts before treating it as a direct quotation in a public-facing document.

Two gists carrying credential-shaped values are cited across this skill and are easy to confuse —
**they are not the same thing**:

- `gist.github.com/riad-uk/3003fa0183b464a0b0d2ca2e77afe477` — the **legacy pre-OAuth
  `x-tidal-token`** family (browser/android/ios/native/audirvana/amarra), from the
  `POST /v1/login/username` era. `references/auth.md` §4.
- `gist.github.com/yaronzz/48d01f5a24b4b7b37f19443977c22cd6` — **OAuth `clientId`/`clientSecret`**
  pairs per platform (Fire TV, Android TV, Android Auto, TV), linked from
  `github.com/yaronzz/Tidal-Media-Downloader/issues/1213` (the March 2026 "broken keys" report).
  `references/legal-and-landscape.md` §2.

Also directly readable (unlike the blocked hosts above): the 2016 TiDown DMCA takedown notice at
`github.com/github/dmca/blob/master/2016/2016-08-31-Tidal.md` — prefer this over TorrentFreak
coverage as the primary source.
