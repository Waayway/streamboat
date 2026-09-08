# The TIDAL API as used by open-source clients

Research report for streamboat. Written 2026-09-07.

Scope: the API surface a full-featured TIDAL client needs — authentication, catalog, library,
playback, reporting, images, and the legal posture around all of it — documented from the source of
the reference implementations first, and from public documentation second.

Two conventions are used throughout:

- `ref:<project>/<path>` points at the read-only reference checkouts under
  `scratchpad/ref/`. Every such claim was read out of that file.
- **[verified]** = read directly in reference source. **[documented]** = stated by a first- or
  third-party document I could fetch. **[inferred]** = my conclusion from the above; not stated
  anywhere verbatim.

Two TIDAL hosts I could not fetch: `developer.tidal.com` and `support.tidal.com` are blocked by this
session's network egress policy, as are `tidal.com`, `flathub.org`, `tidal-music.github.io`,
`forum.strawberrymusicplayer.org` and **`torrentfreak.com`**. Facts that would normally come from
those pages are sourced from search-result excerpts and third-party mirrors and are marked with
lower confidence. Anyone continuing this work should re-check them directly.

This report went through an independent fact-check pass (two reviewers, full source read-back
against the checkouts). Every claim below reflects that pass: refuted claims were corrected in
place, confirmed claims kept their citations (with line numbers corrected where the reviewers found
drift — checkouts are shallow clones and lines move release to release), and gaps the reviewers
found are folded in as new material, each tagged `[gap-filled]`. Reference checkouts were read at
the following commits (record these when re-verifying a citation; a line number a few lines off is
most likely clone drift, not an error):

| Project | Commit |
|---|---|
| python-tidal | `9c41fbe6b2f2cd9fa00dca11e83574fd929ec020` |
| high-tide | `49f24724c38b937f3ff6c73fc58def994c940a9c` |
| sone | `21494b97e8e2cdc75217202402622e58e6c77444` |
| sone-windows | `f2009f207bd686a33901939ecb710ee72a740560` |
| strawberry | `5d247064cbb7b548fbb4fcdfbabfc6635a9dacb3` |
| mopidy-tidal | `18abb3b8709d1fac899a9fd3b402d23837350c95` |
| tidal-hifi | `b1326db2f257543d350e02ec20c8a00daf48261c` |
| TidaLuna | `d8cd6bc29b5edc30dd8ec28861b88d3519560126` |
| tidalt | `6cf18c99f8078c506a49a6cc3f48561e97548dc2` |
| tidalrs | `8bb1de8077f2f052da2e852d030012996700fa16` |
| tidal-cli | `24f20a852978dfd28ec8963fee9c753ac8d4e04d` |
| tidal-sdk-web | `89244b4309443c8e64f8aa830baa0d566d4179eb` |
| tidal-sdk-android | `c93bff404c88206c8d7d4392972bdcfa67ed77e0` |
| tidal-sdk-ios | `787fe9640096b44135ff206bf4769fa9a5da56f0` |
| tidalgo | `6e5564ecb7420901bf80f4b05f0a0df30f706c8f` |
| dotnet-tidal-usdk | `fed9e61d6990e12f84f0074606bfe4dbeddf8c7b` |
| libopentidal | `89a1eaa4f842471fe0e0ae194b5976a0c243a9f0` |
| tidalswift | `cf0926bbd43f37db23e53e53a7fe6b4e955f7065` |
| tidal-api-docs | `457e15d2e94ada68e18997f978e0e89b4bb024b8` |
| tidal-connect | `05ea5d780df4496f1f071278cf967cff44a4cc9e` |
| tidal-fokka-engineering- | `9b06cb15d13c0796d13418041ab0c5e6d929583d` |

Clone date for all of the above: 2026-09-07 (this session).

---

## Summary

1. There are **two entirely different TIDAL APIs**. The *official Developer API* lives at
   `https://openapi.tidal.com/v2/` (JSON:API), is documented, and is what `tidal-cli` and the
   official SDKs target. The *unofficial/internal API* lives at `https://api.tidal.com/v1/` and
   `https://api.tidal.com/v2/`, is what the TIDAL apps themselves use, and is what High Tide, Sone,
   Strawberry, mopidy-tidal, python-tidal and every other Linux client uses for playback. [verified]
2. **Official-API playback is closed to third parties for two independent reasons, not one.**
   Contractually: TIDAL's developer guidelines state that "playbacks will only be available through
   our SDKs, namely, an official, unmodified version of the TIDAL Player module, and TIDAL will
   reject any quota extension requests for any Offering that attempts to circumvent this."
   [documented, medium confidence — sourced via search excerpt, not fetched directly; confirmed
   verbatim by an independent fact-check pass]. Technically: the manifest `/trackManifests/{id}`
   returns is DRM-protected — its `drmData` carries a Widevine or FairPlay license URL and init
   data (`ref:tidal-sdk-web/packages/api/src/allAPI.generated.ts` `DrmData`, `TrackManifests_Attributes`)
   — so even a client willing to ignore the contract cannot decode it without a licensed CDM. A native
   GStreamer/Qt/GTK client is blocked at the codec level, not only at the terms level. **[gap-filled]**
3. **The official third-party app review pipeline appears stalled.** In `tidal-music` GitHub
   Discussion #179, a developer reports waiting since ~2024 for app review, with a follow-up in
   April 2026 saying "nothing has moved" and email to TIDAL unanswered. No TIDAL staff reply on the
   thread. [documented]
4. The unofficial API's auth is OAuth2 against `https://auth.tidal.com/v1/oauth2/…` with the
   authorize page at `https://login.tidal.com/authorize`. Two flows are in use: **device code**
   (RFC 8628) and **authorization code + PKCE**. Both are used with **client IDs extracted from
   TIDAL's own mobile/desktop apps**, not with IDs the OSS project registered. [verified]
5. The two client ID/secret pairs that essentially the whole ecosystem shares, shipped in
   python-tidal (double-base64'd) and in Sone (XOR-masked), are:
   `<client_id A>` / `<redacted client_secret A; see ref:python-tidal/tidalapi/session.py>` (device-code pair) and
   `<client_id B>` / `<redacted client_secret B; see ref:python-tidal/tidalapi/session.py>` (PKCE pair, the one that
   unlocks `HI_RES_LOSSLESS`). tidalt hardcodes the first pair in plaintext with a comment. All
   three ship the same values. [verified]
6. **The client ID determines what you get.** python-tidal's own docstring: PKCE "is the only way how
   to get access to HiRes (Up to 24-bit, 192 kHz) FLAC files". Strawberry's user-facing error text:
   "Whether Tidal delivers encrypted streams depends on the client ID in use. Try changing the Client
   ID in the Tidal settings". [verified]
7. The playback call on the unofficial API is
   `GET https://api.tidal.com/v1/tracks/{id}/playbackinfopostpaywall` with
   `audioquality`, `playbackmode=STREAM`, `assetpresentation=FULL`, `countryCode`. It returns
   `manifestMimeType` + base64 `manifest`, plus `audioQuality`, `audioMode`, `bitDepth`,
   `sampleRate`, `albumReplayGain`, `albumPeakAmplitude`, `trackReplayGain`, `trackPeakAmplitude`,
   `manifestHash`. [verified in five independent implementations]
8. Three manifest MIME types exist: `application/vnd.tidal.bts` (JSON, `urls[]` + `codecs` +
   `encryptionType` + optional `keyId`), `application/dash+xml` (MPEG-DASH MPD, segment-templated
   FLAC), and `application/vnd.tidal.emu` (JSON, `urls[]`, used for video). A fourth,
   `application/vnd.apple.mpegurl` (HLS), appears in the official SDKs. [verified]
9. `encryptionType` is either `NONE` or `OLD_AES` in the BTS manifest. Strawberry **refuses to play**
   anything not `NONE` and says so to the user. TidaLuna, which runs inside the official client,
   contains an AES implementation for `OLD_AES`. streamboat should follow Strawberry, not TidaLuna.
   [verified]
10. Quality tiers are `LOW` (HE-AAC ~96 kbps), `HIGH` (AAC-LC ~320 kbps), `LOSSLESS` (FLAC 16/44.1),
    `HI_RES_LOSSLESS` (FLAC up to 24/192). A legacy `HI_RES` tier maps to MQA and is dead content.
    Every serious client implements a **descending cascade** and accepts what it gets, because the
    server silently downgrades rather than erroring. [verified]
11. **MQA and Sony 360 Reality Audio were removed from TIDAL on 2024-07-24.** Dolby Atmos survives,
    delivered as E-AC-3 JOC (`EAC3_JOC` in the official API, `EAC3`/`AC4` codec strings in the
    unofficial one). Sony 360RA used the `mha1` codec; the iOS SDK explicitly says it "has no codec
    the client needs, so it is unsupported here". [documented + verified]
12. **`subStatus` codes in the 4xxx range are playback-specific and are not auth failures.** Sone
    treats `4005, 4010, 4030, 4031, 4032, 4034, 4035` as terminal ("this track will never play") and
    deliberately excludes `4006` (privileges lost, recovers) and `4033` (subscription up-sell). The
    web SDK maps `4010`→quota exceeded, `4032`/`4035`→not available in location, `4033`→not available
    for subscription. Refreshing the token on a 4xxx 401 is wasted work. [verified] TIDAL's own
    Android SDK names every code in the range (`ApiError.kt`) — the canonical table is in §4; the one
    correction worth calling out here is that **`4034` (`NO_CONTENT_MATCHING_CLIENT`) is scoped to
    the client id/tier, not truly terminal** — the same track may play under a different client id or
    a lower requested quality, so do not permanently blacklist a track on `4034` the way Sone's
    blanket terminal set does. **[gap-filled]**
13. **429 handling matters.** python-tidal surfaces `Retry-After`; Sone runs a global cooldown gate
    (default 5 s, clamped to 120 s) shared across all requests, and returns a synthesized 429 to the
    UI while cooling down. No public rate-limit numbers exist — a developer asked TIDAL directly in
    Discussion #269 and got no answer; a second, independent thread (Discussion #285, "Limitations on
    requests that can be made consecutively") is also unanswered, and its author's own practice —
    throttling to one request per 500 ms — is the only concrete community-sourced number available.
    [verified + documented; #269's "unanswered" status itself is **unverified** in this pass, see
    Unverified §]
14. Playlist mutation on `api.tidal.com/v1` is **ETag-guarded**: `GET /playlists/{uuid}` (or
    `/tracks`, `/items`) returns an `etag` header that must be echoed as `If-None-Match` on the
    subsequent POST/DELETE. python-tidal and Sone both do this. [verified]
15. Home/Explore/Artist/Album/Mix pages come from a **"pages" API** — v1 `GET /pages/{slug}` with
    `deviceType`/`locale`/`countryCode`, returning `rows[].modules[]`, and a newer v2
    `GET /v2/home/feed/{slug}` returning `items[]` with a cursor. Both shapes are in production and
    clients handle both. [verified]
16. TIDAL's own play reporting goes to **`https://ec.tidal.com/api/event-batch`**, an AWS-SQS-shaped
    `SendMessageBatch` endpoint carrying a `playback_session` event. Sone reimplements this to make
    plays show up in the user's TIDAL "Recently Played"; the threshold it uses is 30 seconds.
    [verified]
17. There is a **streaming-privileges websocket**: `POST https://api.tidal.com/v1/rt/connect` returns
    a websocket URL; connecting to it is how TIDAL enforces one-stream-at-a-time and how a client
    learns its privileges were revoked by another device. [verified in the Android SDK, and — more
    strongly than originally stated — also in the iOS SDK (hardcoded URL,
    `StreamingPrivilegesHandler.swift`) and the web SDK (`services/pushkin.ts`), i.e. it is a normal
    part of the official client contract on every platform, not an Android peculiarity. **[gap-filled]**]
18. Artwork is `https://resources.tidal.com/images/<uuid-with-slashes>/<W>x<H>.jpg`, with
    **per-entity-type valid sizes** (albums 80/160/320/640/1280 + `origin`; artists
    160/320/480/750; playlists square 160…1080 and wide 160x107/480x320/750x500/1080x720; videos
    160x107/480x320/750x500/1080x720; users 100/210/600). Wrong sizes 403. [verified]
19. **Everything except the official API is undocumented and can break without notice.** In
    March 2026 a report surfaced of community-shared API keys broken en masse
    (`yaronzz/Tidal-Media-Downloader` issue #1213) — but that issue is specifically about the
    **legacy pre-OAuth `x-tidal-token` keys** (the riad-uk gist family), not the OAuth client IDs
    (device-code/PKCE pairs) that High Tide, Sone and python-tidal actually use, and it is a single
    unconfirmed reporter. The stronger evidence for "these credentials get rotated" is python-tidal's
    own changelog: two separate "OAuth Client ID, secret updated" entries (v0.8.7, v0.8.8). The
    correct architectural response is unchanged: isolate the API layer and make the client ID
    user-replaceable. [documented + inferred; scope-corrected — **[gap-filled]**]
20. Legal posture of the ecosystem is consistent: "not affiliated with TIDAL", "requires an active
    paid subscription", player-only, no downloader. Both High Tide and Sone are on Flathub under
    that framing. TIDAL has historically DMCA'd *downloaders* (TiDown, 2016) but there is no
    evidence of takedowns against *players*. [documented]
21. **TIDAL Connect is permanently out of reach and is not a "later sprint."** The `tidal-connect`
    checkout is a wrapper around a closed-source, device-certificate-gated ARM binary that TIDAL
    ships only to hardware partners; its own README says "This repository does not contain any
    tidal-connect binary." There is no open protocol and no reference implementation to build
    against, in either direction (casting to a Connect receiver, or being a Connect target). Plan
    MPRIS, UPnP/DLNA, Chromecast, Snapcast and plain device selection as the substitutes instead.
    **[gap-filled]**
22. **`api.tidal.com` is very likely not CORS-enabled for arbitrary browser origins**, unlike the
    official `openapi.tidal.com/v2` (which the browser-based `tidal-sdk-web` calls directly). Every
    unofficial-API OSS client that has a webview frontend (Sone, sone-windows) routes 100% of its
    TIDAL calls through native/backend code and only lets the webview touch `resources.tidal.com`
    directly. A community docs repo names CORS explicitly as the reason browser-based PKCE handoffs
    don't work. This is a hard constraint on streamboat's UI stack, not a preference: whatever the
    toolkit, the API client must be a native-side module with no browser-origin dependency. **This
    specific claim was not independently re-verified with a live CORS preflight in this pass — do
    that before relying on it.** [inferred from architecture + documented from a community source;
    **[gap-filled]**, partially **unverified**]

---

## Findings

### 1. The two APIs, and which one streamboat is actually choosing between

| | Official Developer API | Unofficial / internal API |
|---|---|---|
| Base | `https://openapi.tidal.com/v2/` | `https://api.tidal.com/v1/`, `https://api.tidal.com/v2/` |
| Shape | JSON:API (`data`/`attributes`/`relationships`/`included`), `application/vnd.api+json` | Ad-hoc JSON (`items[]`, `totalNumberOfItems`, camelCase) |
| Docs | developer.tidal.com, OpenAPI spec, generated SDKs | none; reverse-engineered |
| Auth | OAuth2 PKCE (user) or client_credentials (app), **your own registered client ID** | OAuth2 device-code or PKCE with **TIDAL's own app client IDs** |
| Registration | required; app review reportedly stalled | none |
| Audio bytes | manifest issued, but playing it yourself is prohibited by the Developer Guidelines, **and the manifest is DRM-protected (Widevine/FairPlay) so an unlicensed client cannot decode it regardless** | works, in the clear (mostly — see §8.3) |
| Used by | `tidal-cli`, `tidal-sdk-web`, `tidal-sdk-android`, `tidal-sdk-ios` | High Tide, Sone, Strawberry, mopidy-tidal, tidalt, tidalrs, python-tidal, TidaLuna |

The owner's stated stance — "do what High Tide and Sone do" — selects the right-hand column for
playback. But the two are not mutually exclusive and the reference projects mix them freely: **the
same Bearer token minted by the unofficial device-code flow is accepted by `openapi.tidal.com/v2`.**
Sone creates and edits playlists through the official v2 JSON:API while streaming through v1:

```
POST   https://openapi.tidal.com/v2/playlists            (create; body is JSON:API)
PATCH  https://openapi.tidal.com/v2/playlists/{id}       (rename/describe/accessType)
```
— `ref:sone/src-tauri/src/tidal_api.rs` lines ~1840–1960. python-tidal does the same for ISRC and
UPC lookups (`filter[isrc]`, `filter[barcodeId]`) — `ref:python-tidal/tidalapi/session.py`
`get_tracks_by_isrc` / `get_albums_by_barcode`. TidaLuna also hits
`https://openapi.tidal.com/v2/tracks?filter[isrc]=…` —
`ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:97`. [verified]

**The official API's surface is far larger than "ISRC lookup, JSON:API playlists, lyrics."** It is
256 top-level path entries (`ref:tidal-sdk-web/packages/api/src/allAPI.generated.ts`, mechanically
counted — not "~230"), including the sanctioned editorial-pages endpoint `/dynamicPages` +
`/dynamicModules`, full library CRUD under `/userCollections/{id}/relationships/*`, `/searchResults`
+ `/searchSuggestions`, `/userRecommendations`/`/userDailyMixes`/`/userDiscoveryMixes`, cross-device
`/playQueues`, `/credits/{id}`, `/genres`, `/shares`, and DSP cross-links via `/dspSharingLinks`. See
`.claude/skills/tidal-api/references/official-api.md` for the enumerated list and the `/trackManifests/{id}` parameter
contract. Note the locale format differs from the unofficial API: official is BCP-47 hyphenated
(`en-US`), unofficial is underscored (`en_US`). **[gap-filled]**

**A third architecture exists and the report should name it, not silently reject it:**
`@tidal-music/player` (the package behind `tidal-sdk-web`'s player) is published on npm,
Apache-2.0, and is the *only fully ToS-compliant way to play full-quality TIDAL audio* as a third
party — because it **is** "an official, unmodified version of the TIDAL Player module." It is a
Shaka-Player/EME-based browser player, so it needs a Widevine CDM — the same castlabs-Electron trick
`tidal-hifi` already uses — and it **cannot run in a headless/Node CLI or with a native GStreamer/Qt
audio pipeline**. So the real choice is: (a) "compliant-but-Chromium" — embed the sanctioned Player
module, accept an Electron/CEF-with-Widevine runtime, give up the headless mode; or (b)
"native-but-unofficial" — build a native audio pipeline against the unofficial API, as every Linux
client the owner named does. Given the owner's explicit headless/server requirement, (b) is close to
forced, but it is a decision the owner should see stated, not one made silently by the choice of
reference projects. **[gap-filled]**

**Implication:** streamboat can use the official v2 API opportunistically for the things it is
genuinely better at (ISRC/UPC lookup, JSON:API playlists, `lyrics` with `lrcText`, editorial pages via
`/dynamicPages`) while using v1 for playback and the page feeds. It does not need a second login. The
official API's own auth module supports the same three flows as a bonus: device code, PKCE, and
`client_credentials` for a no-user, metadata-only path (`ref:tidal-sdk-web/packages/auth/src/auth/auth.ts:218,334,451,480,525`).

### 2. Hosts and what lives on each

| Host | Purpose | Evidence |
|---|---|---|
| `auth.tidal.com/v1/oauth2/` | `device_authorization`, `token` (all grant types) | `ref:python-tidal/tidalapi/session.py:110` (`api_oauth2_token`), `ref:sone/src-tauri/src/tidal_api.rs:87`, `ref:tidal-sdk-web/packages/auth/src/auth/auth.ts:67`, `ref:tidal-sdk-android/auth/.../AuthConfig.kt:34` |
| `login.tidal.com/` | `authorize` (the browser login page); Strawberry additionally uses `login.tidal.com/oauth2/token` | `ref:python-tidal/tidalapi/session.py:111` (`api_pkce_auth`), `ref:strawberry/src/tidal/tidalservice.cpp:80-81`, `ref:tidal-sdk-web/packages/auth/src/auth/auth.ts:66` |
| `api.tidal.com/v1/` | the internal API: tracks, albums, artists, playlists, favorites, pages, playbackinfo, lyrics, credits, `rt/connect` | everywhere |
| `api.tidal.com/v2/` | newer internal endpoints: `home/feed/*`, `search`, `suggestions/`, `my-collection/playlists/folders`, `favorites/mixes`, `artist/{id}`, `feed/activities`, `profiles/{id}`, `user-playlists/{id}/public` | `ref:sone/src-tauri/src/tidal_api.rs:89`, `ref:python-tidal/tidalapi/session.py:112` |
| `openapi.tidal.com/v2/` | official Developer API (JSON:API) | `ref:tidal-sdk-web/packages/api/src/api.ts:18`, `ref:tidal-sdk-android/.../ApiClient.kt:513` |
| `desktop.tidal.com/v1/` | an alias of the v1 API used by the official desktop app; TidaLuna calls it | `ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:48,57,65,68,72,76,88,92` |
| `api.tidalhifi.com/v1` | *legacy* alias still used by Strawberry and by the 2018-era tidalgo/dotnet clients | `ref:strawberry/src/tidal/tidalservice.cpp:59`, `ref:tidalgo/tidal.go` |
| `resources.tidal.com` | images (`/images/…jpg`) and animated covers (`/videos/…mp4`) | `ref:python-tidal/tidalapi/session.py:116-121`, `ref:sone/src/types.ts` |
| `listen.tidal.com` | web player; used only to build shareable/`listen` URLs | `ref:python-tidal/tidalapi/session.py:128` |
| `tidal.com/browse` | canonical share URLs | `ref:python-tidal/tidalapi/session.py:129` |
| `ec.tidal.com/api/event-batch` | TIDAL's event consumer (play logging, streaming metrics) | `ref:sone/src-tauri/src/tidal_report/event.rs:7`, `ref:tidal-sdk-android/eventproducer/.../EventProducer.kt:56` |
| `fp.fa.tidal.com/certificate`, `/license` | FairPlay DRM (Apple) | `ref:tidal-sdk-ios/Sources/Player/Common/DRM/FairPlayLicenseFetcher.swift:23-24`, `ref:tidal-sdk-web/packages/player/src/player/shakaPlayer.ts:661,667` |
| `api.tidal.com/v2/widevine` | Widevine license server | `ref:tidal-sdk-web/packages/player/src/player/shakaPlayer.ts:645` |
| `lgf.audio.tidal.com` | an audio CDN host; mopidy-tidal proxies/caches through it | `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/__init__.py:19` |
| CDN hosts in manifests | not fixed; the BTS `urls[]` and DASH `BaseURL`/`SegmentTemplate` carry signed, expiring URLs. Treat as opaque. | [inferred from manifest handling in all clients] |

Note that streams are *not* fetched from `api.tidal.com`. The playbackinfo response hands you signed
CDN URLs (BTS) or an MPD whose segment template points at a CDN (DASH). The web SDK gives manifests a
1-hour lifetime (`MANIFEST_EXPIRATION_MS = 3600000`) —
`ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts:80`. [verified]

**Logout / token revocation — missing from every section above and needed for a real sign-out
feature.** `POST https://auth.tidal.com/v1/logout` with the Bearer access token and an empty body
invalidates the tokens server-side — libopenTIDAL's man page: "This call requests a logout from
TIDALs session layer. If successful the tokens issued by the authorization server get invalidated.
Further interaction with the API is not possible without reauthenticating."
(`ref:libopentidal/Source/OTService/OTServiceAuth.c:125-143`, `ref:libopentidal/Docs/OTServiceLogout.3`).
It is fire-and-forget (libopenTIDAL marks it `isDummy = 1`, discarding the response body). The
official Android SDK's `logout()`, by contrast, only erases local tokens and does not call the
server (`ref:tidal-sdk-android/auth/src/main/kotlin/com/tidal/sdk/auth/login/LoginRepository.kt:112-115`).
streamboat should do both: call the endpoint, then erase local tokens regardless of the result.
**[gap-filled]**

### 3. Authentication — unofficial API

#### 3.1 Device-code flow (RFC 8628)

Step 1, request a code:

```
POST https://auth.tidal.com/v1/oauth2/device_authorization
Content-Type: application/x-www-form-urlencoded

client_id=<client_id A>&scope=r_usr%20w_usr%20w_sub
```
Sone additionally sends `client_secret` when it has one
(`ref:sone/src-tauri/src/tidal_api.rs` `start_device_auth`); python-tidal sends only `client_id`
(`ref:python-tidal/tidalapi/session.py` `get_link_login`).

Response fields consumed by python-tidal's `LinkLogin`
(`ref:python-tidal/tidalapi/session.py:71-97`): `deviceCode`, `userCode`, `verificationUri`,
`verificationUriComplete`, `expiresIn`, `interval`. Note the response is **camelCase**, not the
snake_case RFC 8628 spells — a real gotcha. `verificationUri` is `link.tidal.com`; python-tidal
prints `https://{verification_uri_complete}` (i.e. the value comes back without a scheme). [verified]

Step 2, poll:

```
POST https://auth.tidal.com/v1/oauth2/token
client_id=<client_id A>
&client_secret=<redacted client_secret A; see ref:python-tidal/tidalapi/session.py>
&device_code=<deviceCode>
&grant_type=urn:ietf:params:oauth:grant-type:device_code
&scope=r_usr w_usr w_sub
```
python-tidal loops at `interval` seconds until `expiresIn` elapses or the body reports
`error == "expired_token"` (`ref:python-tidal/tidalapi/session.py` `_check_link_login`). Sone treats
HTTP 400 with `authorization_pending` or `slow_down` in the body as "keep waiting"
(`ref:sone/src-tauri/src/tidal_api.rs` `poll_device_token`). [verified]

Failure mode worth handling: a client ID that is not registered as a Limited Input Device returns an
error containing `not a Limited Input Device client` or `sub_status":1002`. Sone detects this string
and tells the user their client ID is probably a web-player ID
(`ref:sone/src-tauri/src/tidal_api.rs` ~line 1576). [verified]

Success response: `{ access_token, refresh_token, token_type: "Bearer", expires_in, user: {...} }`.
Sone's `AuthTokens` also reads an optional `user_id`. [verified]

#### 3.2 PKCE / authorization-code flow

Authorize URL (python-tidal `pkce_login_url`, `ref:python-tidal/tidalapi/session.py`):

```
https://login.tidal.com/authorize
  ?response_type=code
  &redirect_uri=https://tidal.com/android/login/auth
  &client_id=<client_id B>
  &lang=EN
  &appMode=android
  &client_unique_key=<hex of a random 64-bit int>
  &code_challenge=<base64url(sha256(verifier)), unpadded>
  &code_challenge_method=S256
  &restrict_signup=true
```
Sone builds the **same parameter set in the same order** —
`ref:sone/src-tauri/src/commands/auth.rs:345-350` — but not a byte-identical URL: see the
`client_unique_key` correction just below. python-tidal also builds the query with a library
urlencoder while Sone hardcodes the percent-encoded `redirect_uri` literal; functionally equivalent,
not byte-identical. [verified, corrected]

`code_verifier` in python-tidal is `base64.urlsafe_b64encode(os.urandom(32))[:-1]` — i.e. 32 random
bytes, base64url, one trailing char stripped to drop the `=`. `client_unique_key` is
`format(random.getrandbits(64), "02x")` — **`"02x"` is a minimum-width-2 format, so the value is
1–16 hex characters (leading zeros stripped), not a fixed 16**, as an earlier draft of this report
said. Sone instead uses `format!("{:016x}", …)`, always exactly 16 chars — a real, if minor,
difference between the two implementations. [verified, corrected]

**`client_unique_key` must be persisted, not regenerated per process — TIDAL treats it as the
device identity.** python-tidal generates a fresh one in every `Config()` and never writes it to the
session file (`save_session_to_file` persists `token_type`/`session_id`/`access_token`/
`refresh_token`/`is_pkce` only). The official SDK documents `clientUniqueKey` as "The unique key of
the application," sends it on both `authorize` and the token exchange, and **throws on refresh/token
upgrade if the `clientUniqueKey` is not the same as the one that minted the token**
(`ref:tidal-sdk-web/packages/auth/src/auth/auth.ts:102,125,178-180,276-278,553`). Copying
python-tidal's per-process regeneration means streamboat registers a new "device" with TIDAL on every
login, burning through TIDAL's authorized-device cap and leaving stale phantom devices in the user's
account. **streamboat must generate this key once at first run and store it alongside the refresh
token**, sending the identical value on every authorize/exchange/refresh. Also persist
`expiry_time` and `client_id`, which python-tidal drops (see §3.7). **[gap-filled]**

**The browser authorization-code flow (RFC 6749) is reCAPTCHA v3 protected; the device flow (RFC
8628) is not.** docTIDAL, the write-up behind libopenTIDAL: "I reversed engineered the TIDAL device
authorization grant (RFC 8628) since the web flow (RFC 6749) is reCaptcha v3 secured"
(`ref:tidal-fokka-engineering-/README.md`). Consequences: never attempt a scripted/headless POST of
credentials to `login.tidal.com`; an embedded webview needs a realistic browser fingerprint or the
challenge may score it as a bot; device code is the reliable flow for headless/CLI and for
constrained webviews, for a reason beyond "no browser needed" — it is simply not gated by the
challenge. **[gap-filled]**

The redirect target `https://tidal.com/android/login/auth` is a page that does not exist ("Oops"),
so the client must capture the URL. Three strategies in the wild:

- **Paste the URL.** python-tidal's `login_pkce` prompts for it; High Tide shows an `AdwEntryRow`
  and calls `session.pkce_get_auth_token(redirect_url)` — `ref:high-tide/src/login.py`. Ugly but
  needs no browser integration and works headless.
- **Embedded webview** that watches for navigation to the redirect URI. Sone opens a Tauri
  `WebviewWindow` — `ref:sone/src-tauri/src/commands/auth.rs` around `finish_embedded_pkce`.
- **Custom URI scheme.** Strawberry registers `tidal://login/auth` and does not run a local server
  (`set_use_local_redirect_server(false)`) — `ref:strawberry/src/tidal/tidalservice.cpp:82,133`.
- **Local HTTP server.** mopidy-tidal runs one on port 8989 and serves a form for the paste —
  `ref:mopidy-tidal/mopidy_tidal/web_auth_server.py`. tidal-cli (official API) uses
  `http://localhost:17893/callback` — `ref:tidal-cli/src/auth.ts:23-24`.

Token exchange:

```
POST https://auth.tidal.com/v1/oauth2/token
code=<code>
&client_id=<client_id B>
&grant_type=authorization_code
&redirect_uri=https://tidal.com/android/login/auth
&scope=r_usr+w_usr+w_sub
&code_verifier=<verifier>
&client_unique_key=<same key as authorize>
```
Both python-tidal and Sone literally send `scope=r_usr+w_usr+w_sub` here (with `+` characters, not
spaces) while the device flow sends `r_usr w_usr w_sub`. That inconsistency is in both codebases and
apparently accepted by the server. `ref:python-tidal/tidalapi/session.py` `pkce_get_auth_token`,
`ref:sone/src-tauri/src/tidal_api.rs` `exchange_pkce_code`. [verified]

#### 3.3 Scopes

The unofficial API uses three coarse scopes: `r_usr` (read user data), `w_usr` (write user data),
`w_sub` (write subscription). python-tidal documents them inline: "w_usr=WRITE_USR,
r_usr=READ_USR_DATA, w_sub=WRITE_SUBSCRIPTION" — `ref:python-tidal/tidalapi/session.py`
`pkce_get_auth_token`. Strawberry requests only `r_usr w_usr`
(`ref:strawberry/src/tidal/tidalservice.cpp:83`); tidalrs requests `r_usr w_usr` for one call and
`r_usr w_usr w_sub` for others (`ref:tidalrs/src/lib.rs:767,1060,1114`). [verified]

The *official* API uses fine-grained scopes. tidal-cli's list is the fullest I found in source:
`collection.read`, `collection.write`, `playlists.read`, `playlists.write`, `playback`, `user.read`,
`recommendations.read`, `entitlements.read`, `search.read`, `search.write` —
`ref:tidal-cli/src/auth.ts:26-36`. [verified]

#### 3.4 Client IDs — provenance and what each is tied to

The decoded values from `ref:python-tidal/tidalapi/session.py` `Config.__init__` (they are stored
double-base64-encoded and split in two, which is obfuscation, not security — **the file carries no
comment justifying the obfuscation; do not attribute a rationale to it.** The only nearby comments
are `# OAuth Client Authorization` and `# PKCE Client Authorization. We will keep the former
client_id as a fallback...`, ref:python-tidal/tidalapi/session.py:154,169-170. tidalt's comment on
its own copy of the same pair is the honest one to quote, and it is already quoted correctly two
paragraphs below. **[corrected — this quotation was fabricated in an earlier draft]**):

| Field | Value |
|---|---|
| `client_id` | `<client_id A>` |
| `client_secret` | `<redacted client_secret A; see ref:python-tidal/tidalapi/session.py>` |
| `client_id_pkce` | `<client_id B>` |
| `client_secret_pkce` | `<redacted client_secret B; see ref:python-tidal/tidalapi/session.py>` |

Sone stores the same four values XOR-masked in `ref:sone/src-tauri/src/embedded_config.rs`
(`stream_key_a`…`stream_key_d`); decoding `STREAM_SALT_A ^ CODEC_HINT_A` yields
`<client_id A>` and `STREAM_SALT_C ^ CODEC_HINT_C` yields `<client_id B>`. tidalt hardcodes
the first pair in plaintext with an honest comment: "ClientSecret is the public client secret baked
into the official Tidal app; it is not a user credential" —
`ref:tidalt/internal/tidal/client.go:15-24`. [verified]

Behavioural differences established from the code:

- The PKCE pair is what unlocks Hi-Res. python-tidal `login_pkce` docstring: "This is the only way
  how to get access to HiRes (Up to 24-bit, 192 kHz) FLAC files." mopidy-tidal's README/config
  distinguishes OAuth (default) from "PKCE (HI_RES only)". [verified]
- Track `get_url()` (the `urlpostpaywall` shortcut) is **disabled** under a PKCE session in
  python-tidal: `if self.session.is_pkce: raise URLNotAvailable(...)` —
  `ref:python-tidal/tidalapi/media.py:410-417`. So the PKCE identity gets DASH manifests, not plain
  URLs. [verified]
- Sone's cascade skips `HI_RES_LOSSLESS`/`HI_RES` when no `client_secret` is configured, with the
  comment: "Without client_secret, skip Hi-Res (those credentials typically return encrypted DASH
  streams that require Widevine). With a secret, the confidential PKCE credentials may return
  unencrypted Hi-Res BTS streams." — `ref:sone/src-tauri/src/commands/playback.rs` `resolve_play_uri`.
  [verified]
- Strawberry's error string, shown to users verbatim: "Received a %1 encrypted stream from Tidal,
  which Strawberry does not support. Whether Tidal delivers encrypted streams depends on the client
  ID in use. Try changing the Client ID in the Tidal settings." —
  `ref:strawberry/src/tidal/tidalstreamurlrequest.cpp`. [verified]

A separate, older family of credentials still circulates: the pre-OAuth `x-tidal-token` values used
with username/password login. A widely-mirrored gist documents them with behavioural notes —
`browser` `wdgaB1CilGA-S_s2` ("many Lossless Streams are encrypted"), `android` `kgsOOmYk3zShYrNP`
("All Streams are HTTP Streams … best Token to use"), `ios` `_DSTon1kC8pABnTw` ("Same as Android
Token, but uses ALAC instead of FLAC"), `native` `4zx46pyr9o8qZNRw` ("FLAC streams are encrypted"),
`audirvana` `BI218mwp9ERZ3PFI`, `amarra` `wc8j_yBJd20zOmx0`. These correspond to the era of
`POST /v1/login/username` (still visible in `ref:tidalgo/tidal.go` and
`ref:dotnet-tidal-usdk/TidalUSDK/`), which is dead — but the *pattern* they document (identity
determines encryption and codec) is exactly what Strawberry's error text still describes.
[documented; the token values are from a third-party gist, not source]

How projects ship credentials — this matters for streamboat's own posture:

| Project | Ships an ID? | User-replaceable? |
|---|---|---|
| python-tidal | yes, double-base64'd in `Config` | only by subclassing/patching `Config` |
| High Tide | inherits python-tidal's (it pip-installs the `tidalapi` wheel into the Flatpak — `ref:high-tide/build-aux/python3-tidalapi.json` pins `tidalapi-0.8.8`) | no UI for it |
| Sone | yes, XOR-masked, both pairs | **yes** — Settings takes a custom client ID/secret; `resolve_credentials` prefers user values and falls back to embedded — `ref:sone/src-tauri/src/commands/auth.rs:31-45` |
| Strawberry | optional compile-time `TIDAL_CLIENT_ID` (encrypted at build); official releases may ship none | **yes** — "use custom client ID" checkbox — `ref:strawberry/src/settings/tidalsettingspage.cpp:101-179` |
| tidalt | yes, plaintext, with a justifying comment | no |
| tidalrs | no — the caller passes one to `TidalClient::new(client_id)` | n/a (library) |
| tidal-cli | yes, its **own registered** public Developer-API client `PYVtmSHMTGI9oBUs`, with a comment explaining it is a public OAuth client, not a secret | no |

[verified for all rows]

**A second mitigation against key revocation, beyond "make it user-replaceable": runtime discovery.**
The current web-player client id can be read at runtime by visiting TIDAL's own web player and
pulling it from the login redirect URL; `tidal-api-docs` records the web id as `CzET4vdadNUFQ5JU` and
advises "Ideally, your implementation will dynamically retrieve the client_id from Tidal before
making any requests" (`ref:tidal-api-docs/Authorization/Retrieve-Current-Client-Id.md`,
`ref:tidal-api-docs/README.md` — "Tidal is liable to change their client_id at any time"). **Caveat:
a web-player id is not registered as a Limited Input Device**, so it cannot do the device-code flow —
this is exactly the failure Sone detects (`sub_status":1002` / `not a Limited Input Device client`,
§3.1). Runtime discovery is therefore only a fallback for the PKCE path. Recommendation: ship the
ecosystem id as default, allow a user override (Sone/Strawberry pattern), and consider runtime
discovery as an explicitly opt-in recovery mode. **[gap-filled]**

#### 3.5 Token refresh

```
POST https://auth.tidal.com/v1/oauth2/token
grant_type=refresh_token
&refresh_token=<rt>
&client_id=<same id family that minted it>
&client_secret=<if you have one>
&scope=r_usr w_usr w_sub        # Sone sends this; python-tidal does not
```
python-tidal switches between the PKCE and non-PKCE pair based on `session.is_pkce` —
`ref:python-tidal/tidalapi/session.py` `token_refresh`. **The refresh response may omit
`refresh_token`**; Sone falls back to the existing one
(`ref:sone/src-tauri/src/tidal_api.rs` `RefreshResponse`), and so should streamboat. [verified]

Refresh triggers in the wild:

- python-tidal is *reactive and string-matched*: on any non-OK response it checks whether the JSON
  body's `userMessage` starts with `"The token has expired."` and only then refreshes —
  `ref:python-tidal/tidalapi/request.py:107-121`. Fragile.
- Sone refreshes on any 401 **except** one carrying a 4xxx `subStatus` —
  `ref:sone/src-tauri/src/tidal_api.rs` `authenticated_get`. Better.
- libopenTIDAL refreshes proactively 300 s before `expiresIn` —
  `ref:libopentidal/Source/OTSessionRefresh.c`. tidalswift uses a 5-minute margin. Best.
- The official web SDK has a single-flight guard (`pending`, `pendingPromises`, `sessionId`) so
  concurrent 401s produce one refresh — `ref:tidal-sdk-web/packages/auth/src/auth/auth.ts:40-64`.
  tidalrs guards concurrent refreshes with an `authz_update_semaphore: Semaphore`
  (`ref:tidalrs/src/lib.rs:317,751`) — **its source comment reads "Try to become the single
  refresher," not "to avoid thundering herd"; that phrase does not appear in the tidalrs checkout
  and should not be quoted as if it does** [corrected]. streamboat needs this: a page load fires a
  dozen parallel requests.

The official SDK also has `grant_type=update_client` ("token upgrade"), used when a client secret is
added to a previously public client — `ref:tidal-sdk-web/packages/auth/src/auth/auth.ts:469-480`.
Not needed for the unofficial flow. [verified]

#### 3.6 Session bootstrap: `GET /v1/sessions`

**Not every client calls this** — see correction below — but python-tidal, Sone, sone-windows and
tidalt do, immediately after obtaining a token:

```
GET https://api.tidal.com/v1/sessions
Authorization: Bearer <access_token>
```
Response supplies `sessionId`, `countryCode`, `userId`. python-tidal stores all three and sets
`locale = "en_US"` with a `TODO` to read it from the system —
`ref:python-tidal/tidalapi/session.py` `process_auth_token`. Sone reads `userId` and `countryCode`
only — `ref:sone/src-tauri/src/tidal_api.rs` `get_session_info`. [verified]

**Correction: Strawberry never calls `/v1/sessions`.** It takes `countryCode` straight from the OAuth
token response instead (`ref:strawberry/src/tidal/tidalservice.cpp:205-207`,
`return oauth_->country_code()`, injected via `ref:strawberry/src/tidal/tidalbaserequest.cpp:74`).
So the endpoint is common but not universal — write "python-tidal, Sone, sone-windows and tidalt call
it," not "every client." **[corrected]**

`countryCode` is mandatory on essentially every subsequent call; python-tidal injects it (and
`sessionId`, and `limit`) into **every** request in `basic_request` —
`ref:python-tidal/tidalapi/request.py:74-78`. `sessionId` is legacy in the sense that Sone never
sends it and works fine, so Bearer alone is likely sufficient in 2026 — **but this is untested
against the specific endpoints python-tidal exercises that Sone does not** (`pages/*`, `genres`,
`urlpostpaywall`). Treat "drop sessionId" as a reasonable default with a compatibility flag to send
it if an endpoint ever misbehaves without it, not as a settled fact. [verified + inferred; confidence
lowered — **[gap-filled]**]

Login validity check: python-tidal's `check_login()` calls
`GET /v1/users/{userId}/subscription` and returns whether it was OK —
`ref:python-tidal/tidalapi/session.py`. Cheap and it also tells you the tier. [verified]

#### 3.7 Secure token storage per OS

| Project | Mechanism |
|---|---|
| High Tide | freedesktop Secret Service via libsecret, schema `io.github.nokse22.high-tide`, item key `high-tide-login`, value is a JSON blob with `token-type`, `access-token`, `refresh-token`, `expiry-time`, `is-pkce`. On startup, when **not** under Flatpak (`Xdp.Portal.running_under_flatpak()`), it explicitly unlocks the default collection — `ref:high-tide/src/lib/secret_storage.py`. |
| Sone | settings JSON at `~/.config/sone/settings.json`, encrypted at rest; master key in the OS keyring (service `sone`, entry `master-key`) with a file fallback at `~/.config/sone/sone.key` — per its README/`crypto.rs`. |
| Strawberry | QSettings with Strawberry's own `CryptUtils` obfuscation. |
| python-tidal / mopidy-tidal | plain JSON file (`tidal-oauth.json` / `tidal-pkce.json`); `save_session_to_file` writes `token_type`, `session_id`, `access_token`, `refresh_token`, `is_pkce` — note `expiry_time` is **commented out**, so a reloaded session has no expiry and relies on reactive refresh — `ref:python-tidal/tidalapi/session.py`. |
| tidalt | `docker/secrets-engine` → system keychain, with an age-encrypted file fallback at `~/.config/tidalt/secrets`. |
| tidal-cli | file-backed `localStorage` polyfill at `~/.tidal-cli/session.json`, mode 600 — `ref:tidal-cli/src/session.ts`. |
| tidal-sdk-web | `StorageAdapter`, default `localStorage` with AES-GCM. |
| tidal-sdk-android / ios | `EncryptedSharedPreferences` / Keychain. |

[verified]

The Flatpak caveat in High Tide's code is worth carrying forward: inside a sandbox you cannot unlock
the keyring yourself, you go through the Secret portal. A headless/server mode has no keyring at
all, so streamboat needs a documented file fallback with restrictive permissions. [verified +
inferred]

### 4. Request conventions on the unofficial API

**Headers.** python-tidal sends, on every request
(`ref:python-tidal/tidalapi/request.py:57-101`):

```
Authorization: Bearer <token>
x-tidal-client-version: 2025.7.16
User-Agent: Mozilla/5.0 (Linux; Android 12; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/91.0.4472.114 Safari/537.36
```
Sone sends `x-tidal-client-version: 2025.11.3` **only on `/v2/` URLs** —
`ref:sone/src-tauri/src/tidal_api.rs:1512-1514, :91`. That version string is a moving target: it
pins the web client build and both projects bump it periodically. Neither sets a distinctive
User-Agent identifying the app. [verified]

The official player's legacy playbackinfo call adds:
`x-tidal-token: <clientId>`, `x-tidal-streamingsessionid: <uuid>`, `x-tidal-playlistuuid: <uuid>`
(the current queue context — easy to miss, not mentioned in an earlier draft), and
`x-tidal-prefetch: true` when prefetching —
`ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts:196-218`.
TidaLuna also sends `x-tidal-token: <clientId>` alongside the Bearer —
`ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:21-27`. So `x-tidal-token` is now a
*client-id* header, not the legacy API key it once was. [verified]

**Whether `x-tidal-streamingsessionid` is required for a play to be counted in Recently Played is
not fully answerable from these checkouts, and the report should say so plainly rather than imply an
answer.** The web SDK generates a streaming session id, sends it in this header on the legacy
playbackinfo call, and gets it echoed back as `streamingSessionId` in the response; its streaming-
metrics events are keyed on it. **Sone never sends this header at all** (grep of
`sone/src-tauri/src` for `streamingsession`/`streaming_session` finds nothing), yet its play-log
event carries a self-generated `playbackSessionId` and its own tests assert that shape is what
surfaces in Recently Played. So the two ids look like separate concerns, and correlating them is
apparently not required — but this is inference, not something read out of TIDAL's own logic.
Recommendation, cheap and low-risk: generate one UUID per playback, send it as
`x-tidal-streamingsessionid` anyway (matches the official client, costs nothing) and reuse it as
`playbackSessionId` in reporting (§10). **[gap-filled]**

**Common query parameters.** `countryCode` (required), `locale` (`en_US`), `deviceType`
(`BROWSER` | `DESKTOP` | `PHONE`; python-tidal defaults pages to `BROWSER`, `PageLink.get()` uses
`DESKTOP`), `platform` (`WEB`, only on v2 page endpoints), `limit`, `offset`, `order`,
`orderDirection`. [verified]

**Pagination.** Offset/limit with `totalNumberOfItems` in the body for v1 collections. python-tidal
gets a count cheaply with `limit=1` and reads `totalNumberOfItems`
(`get_tracks_count` etc., `ref:python-tidal/tidalapi/user.py`). The v2
`my-collection/playlists/folders` endpoint is **cursor-based** and *ignores* `offset` — python-tidal
documents this explicitly and threads a `cursor` field
(`ref:python-tidal/tidalapi/user.py:669-745`). The v2 home feed is also cursor-based
(`json.page.cursor`) — `ref:sone/src-tauri/src/tidal_api.rs` `fetch_v2_home_feed`. [verified]

**A python-tidal gotcha to not copy:** `Config.item_limit` (default 1000, capped at 10000) is
injected as `limit` into *every* request, including ones where you did not want it. Search is
separately documented as returning at most 300 items regardless. [verified]

**Concrete server-side pagination caps, beyond "some endpoints max at 100":**

- python-tidal's own docstring: "The maximum item_limit is 10000, and some endpoints have a maximum
  of 100 items ... In cases where the maximum is 100 items, you will have to use offsets"
  (`ref:python-tidal/tidalapi/session.py:103-104,139,146-147`).
- **A specific, reproduced production bug**: `GET users/{id}/favorites/videos` rejects
  `limit=10000` outright. Sone's fix comment: "The favorites/videos content endpoint caps page size
  (a single limit=10000 request is rejected — which left every video heart empty on startup). Page
  through it at the known-good size," paging at `const PAGE: u32 = 50`
  (`ref:sone/src-tauri/src/tidal_api.rs:2675-2690`).
- `GET users/{id}/playlistsAndFavoritePlaylists` is server-capped at 50 (already noted in §7).
- Search returns at most 300 items total regardless of `limit`/`offset` (already noted above).

Recommendation: default every collection page to 50, never send `limit` above 100 on v1 collection
endpoints, and treat `totalNumberOfItems` in the response body as the only pagination authority — not
a client-side constant. **[gap-filled]**

**Errors.** The error body shape is `{"status": <http>, "subStatus": <int>, "userMessage": "..."}`.
Strawberry detects exactly that triple and formats `"%1 (%2) (%3)"` —
`ref:strawberry/src/tidal/tidalbaserequest.cpp`. Newer v2/openapi endpoints instead return
`{"errors": [{"detail": "..."}]}` — python-tidal reads `resp["errors"][0]["detail"]`
(`ref:python-tidal/tidalapi/request.py:174-184`). Handle both. [verified]

**Sub-status codes.** An earlier draft consolidated this table from Sone's guesses plus the web SDK
and was missing five codes. **TIDAL's own Android SDK names every code in the 4xxx range** —
`ref:tidal-sdk-android/player/common/src/main/kotlin/com/tidal/sdk/player/common/model/ApiError.kt`
— and that file, not Sone, is the canonical source. It also confirms the error envelope is exactly
`{status, subStatus, userMessage}`.

| subStatus | Meaning (official name) | Recommended handling |
|---|---|---|
| 1002 | client is not a Limited Input Device (device flow refused) | tell the user their client id can't do device-code login |
| 4005 | `GENERIC_PLAYBACK_ERROR` | terminal for this attempt; retry once, then give up |
| 4006 | `NO_STREAMING_PRIVILEGES` | **recovers** — do not blacklist the track |
| 4007 | `USER_CLIENT_NOT_AUTHORIZED_FOR_OFFLINE` | this client id isn't entitled to offline; see §8.9 |
| 4010 | `USER_MONTHLY_STREAM_QUOTA_EXCEEDED` | show the user a quota message |
| 4020 | `SESSION_NOT_FOUND` | re-login |
| 4021 | `USER_NOT_FOUND` | re-login |
| 4022 | `CLIENT_NOT_FOUND` | client id is invalid/revoked — surface prominently, this is the "TIDAL rotated the key" case |
| 4023 | `PRODUCT_NOT_FOUND` | skip the track, do not blacklist (may be a transient catalog issue) |
| 4030 | `NO_CONTENT_AVAILABLE_IN_PRODUCT` | terminal for this subscription tier |
| 4031 | `NO_CONTENT_MATCHING_REQUEST` | terminal for the requested parameters |
| 4032 | `NO_CONTENT_MATCHING_SUBSCRIPTION_LOCATION` | region-gated; terminal for this account's region |
| 4033 | `NO_CONTENT_MATCHING_SUBSCRIPTION_CONFIGURATION` | **user-fixable** (upsell) — do not blacklist |
| 4034 | `NO_CONTENT_MATCHING_CLIENT` | **client/tier-scoped, not truly terminal** — retry once at a lower tier or with a different client id before giving up; do not permanently blacklist the track (correction to an earlier draft, which called this flatly "terminal") |
| 4035 | `NO_CONTENT_MATCHING_PRE_PAYWALL_LOCATION` | terminal for this account's region |
| 6001 | session error | `ref:tidal-sdk-web/packages/auth/src/auth/auth.ts:68`, `ref:sone/src-tauri/src/tidal_api.rs:11-12` |
| 11001, 11002, 11003, 11101 | token/auth errors | `ref:tidal-sdk-web/packages/auth/src/auth/auth.ts:68` |

Sone's `PLAYBACKINFO_SUB_STATUS_RANGE = 4000..=4999` and its `TERMINAL_SUB_STATUSES = [4005, 4010,
4030, 4031, 4032, 4034, 4035]` remain a reasonable *approximation* of "don't bother refreshing the
token," and it correctly handles float-encoded sub-statuses (`4005.0`) — but **do not copy its
blanket treatment of `4034` as terminal**; give it the tier/client-scoped retry described above
instead. [verified; corrected — **[gap-filled]**]

**Rate limiting.** No published numbers. A developer asked TIDAL directly in
`github.com/orgs/tidal-music/discussions/269`; **that thread's "unanswered" status was not
independently re-confirmed in this pass and should be treated as unverified**, though nothing in any
of the 22 checkouts states a numeric limit, which is at least consistent with it. A second,
independently-found thread strengthens the finding: `github.com/orgs/tidal-music/discussions/285`
("Limitations on requests that can be made consecutively") — fetched and confirmed unanswered with 0
comments — has a developer writing "I notice there is limitation on the amount of requests that can
be made using the Rest API. Currently, I throttle for 500ms between every request, but are there
guidelines for this?" That is the only concrete community-sourced number available (one request per
500 ms, self-imposed, not a documented limit). Cite both threads together. **[gap-filled, partially
unverified]** Observed handling:

- python-tidal raises `TooManyRequests` on 429 and attaches `Retry-After` if present —
  `ref:python-tidal/tidalapi/exceptions.py:75-80`.
- Sone runs a lock-free global `RateGate`: on a 429 it reads `Retry-After` (delta-seconds form only;
  the HTTP-date form is rejected rather than mis-parsed), clamps to `1..=120` s, and stores an
  absolute deadline with `fetch_max` so concurrent 429s only lengthen it. While cooling down, every
  request returns a synthesized 429 body with `retryAfterSecs` and a `userMessage`, without touching
  the network. Default cooldown when no header: 5 s. — `ref:sone/src-tauri/src/rate_gate.rs`,
  `ref:sone/src-tauri/src/tidal_api.rs:1332-1348`.
- Sone also refuses to walk the quality cascade after a 429: "A rate-limit or a terminal-unplayable
  answer will not change at a lower tier … Walking the rest of the cascade only multiplies the
  request count by 4." — `ref:sone/src-tauri/src/commands/playback.rs`.
- The official web SDK retries GET/HEAD/OPTIONS only, with exponential backoff + jitter
  (`D = min(B * 2^n * j, M)`, `j ∈ [0.8, 1)`), separate policies for network (base 1 s, max 16 s,
  10 retries), status (500 ms / 16 s / 3) and timeout (8 s / 32 s / 3), and a 10 s per-attempt read
  timeout — `ref:tidal-sdk-web/packages/api/src/retry.ts`. Writes are never retried.

[verified]

### 5. Catalog

All paths below are relative to `https://api.tidal.com/v1/` unless noted, and take `countryCode`.

**Search (v1)** — `GET search?query=&limit=&offset=&types=`. python-tidal's default `types` string is
`artists,albums,tracks,videos,playlists,mixs` (lowercase, and note the ungrammatical `mixs`, which
comes from mechanically pluralising its type identifiers) — `ref:python-tidal/tidalapi/session.py`
`search`. Response has `artists`, `albums`, `tracks`, `videos`, `playlists` each `{items:[...]}` plus
`topHit: {type, value}`. Docstring: "there aren't more than 300 items available in a search".
[verified]

**Search (v2)** — `GET https://api.tidal.com/v2/search` with `query`, `countryCode`, `limit`,
`types=ARTISTS,ALBUMS,TRACKS,PLAYLISTS,VIDEOS` (uppercase here), plus
`includeContributors=true`, `includeUserPlaylists=true`, `includeDidYouMean=true`,
`supportsUserData=true`, `locale`, `deviceType`. Sone tries v2 first and falls back to v1, with the
comment "web app uses this, returns playlists properly" — `ref:sone/src-tauri/src/tidal_api.rs`
`search`/`search_v2`. [verified]

**Search suggestions** — `GET https://api.tidal.com/v2/suggestions/` (trailing slash) with `query`,
`countryCode`, `explicit=true`, `hybrid=true`. Response carries a `history` array (source
`"history"`) and text suggestions plus "direct hit" entities — this is what the web player's
mini-search dropdown uses — `ref:sone/src-tauri/src/tidal_api.rs:4078-4155`. [verified]

**Entities**

| Path | Notes |
|---|---|
| `tracks/{id}` | full track |
| `tracks/{id}/lyrics` | see §9 |
| `tracks/{id}/credits` | array of `{type, contributors:[{name,id}]}` — `ref:sone/src-tauri/src/tidal_api.rs:3910-3925` |
| `tracks/{id}/radio?limit=` | list of similar tracks |
| `tracks/{id}/mix` | returns `{id: "<mixId>"}` for the track radio as a Mix |
| `tracks/{id}/playbackinfopostpaywall` | §7 |
| `tracks/{id}/urlpostpaywall` | §7 |
| `albums/{id}` | |
| `albums/{id}/tracks?limit=&offset=` | |
| `albums/{id}/items?limit=&offset=` | tracks **and** videos |
| `albums/{id}/similar` | 404 → "no similar albums exist" |
| `albums/{id}/review` | `{text: "...", ...}` — editorial review, contains wimp:// links |
| `artists/{id}` | |
| `artists/{id}/albums?filter=EPSANDSINGLES` / `?filter=COMPILATIONS` / no filter | three discography buckets |
| `artists/{id}/toptracks?limit=&offset=` | |
| `artists/{id}/videos` | |
| `artists/{id}/bio` | `{text: "..."}` |
| `artists/{id}/similar` | |
| `artists/{id}/radio?limit=` | |
| `artists/{id}/mix` | `{id: "<mixId>"}` |
| `videos/{id}` | |
| `videos/{id}/urlpostpaywall` | `{urls:[m3u8]}` |
| `videos/{id}/playbackinfopostpaywall` | EMU manifest |
| `mixes/{id}/items` | legacy fallback for mix contents |
| `genres` | `[{name, path, hasPlaylists, hasArtists, hasAlbums, hasTracks, hasVideos, image}]` |
| `genres/{path}/{tracks\|albums\|artists\|playlists\|videos}` | |
| `playlists/{uuid}`, `/tracks`, `/items`, `/recommendations/items` | §6 |
| `users/{id}`, `users/{id}/subscription` | |
| `rt/connect` (POST) | streaming-privileges websocket handshake, §8 |

[verified — every row read from python-tidal `album.py`/`artist.py`/`media.py`/`genre.py` or from
`ref:sone/src-tauri/src/tidal_api.rs`]

**Track/media fields** worth knowing, from `ref:python-tidal/tidalapi/media.py` `Media.parse` and
`Track.parse_track`:

`id, title, duration (seconds), explicit, popularity, allowStreaming, streamReady, stemReady,
djReady, adSupportedStreamReady, streamStartDate, dateAdded, trackNumber, volumeNumber, artists[],
artist, album, artistRoles, type, payToStream, premiumStreamingOnly, editable, upload, spotlighted,
url, audioQuality, audioModes[], accessType, mediaMetadata.tags[], index, itemUuid, isrc,
description, version, copyright, bpm, key, keyScale, peak, replayGain, mixes{}`.

`mediaMetadata.tags` is the availability advertisement: `LOSSLESS`, `HIRES_LOSSLESS`, `DOLBY_ATMOS`
(historically also `SONY_360RA`, `MQA`) — `ref:TidaLuna/plugins/lib/src/redux/types/store/content/Track.ts:5`,
`ref:sone/src/types.ts:71`. python-tidal exposes `is_hi_res_lossless`, `is_lossless`,
`is_dolby_atmos` properties off it. mopidy-tidal uses it to warn before requesting Hi-Res:
"No HIRES_LOSSLESS available for this track; Using playback quality LOSSLESS" —
`ref:mopidy-tidal/mopidy_tidal/playback.py`. [verified]

**Availability gating:** `allowStreaming` (aka `available`) and `streamReady` are the two booleans to
respect; `Track.parse_track` only populates the detail fields when `available` is true. Album-level
`streamReady` and `adSupportedStreamReady` mirror this. [verified]

**Charts, new releases and editorial: no dedicated endpoint exists, stated explicitly rather than by
silence.** Grepping all 23 checkouts for `chart|new_release|newrelease|new-release` finds only
`mix.py:43 new_release = "NEW_RELEASE_MIX"` (a `mixType`) and an unrelated Sone cache comment. There
is no `/charts` REST endpoint on the unofficial API. The surfaces that actually carry this content
are the v1 page slugs already in §6's table (`pages/rising`, `pages/explore`,
`pages/suggested_new_tracks_for_you`, `pages/suggested_new_albums_for_you`), the `NEW_RELEASE_MIX`
mix type, and on the official API `/userNewReleaseMixes/{id}` and
`/userRecommendations/{id}/relationships/newArrivalMixes`. Album credits, similarly, are not a
dedicated endpoint the way track credits are (`tracks/{id}/credits`) — they arrive as a `credits`
module inside `pages/album` (`ref:sone/src-tauri/src/tidal_api.rs:5325`). **[gap-filled]**

### 6. Pages API (Home, Explore, artist/album pages)

**v1 shape** — `GET pages/{slug}?deviceType=BROWSER&locale=en_US&countryCode=XX`. Response is
`{title, rows: [{modules: [ {type, title, pagedList:{items:[…]}, showMore:{apiPath,title}, viewAll} ]}]}`.
python-tidal parses `row["modules"][0]` per row and dispatches on `module.type`:
`PAGE_LINKS_CLOUD`, `PAGE_LINKS`, `FEATURED_PROMOTIONS`, `MULTIPLE_TOP_PROMOTIONS`, `ALBUM_LIST`,
`ARTIST_LIST`, `TRACK_LIST`, `PLAYLIST_LIST`, `VIDEO_LIST`, `MIX_LIST`, `TEXT_BLOCK`,
`ITEM_LIST_WITH_ROLES`, `MIXED_TYPES_LIST` — `ref:python-tidal/tidalapi/page.py:176-244, 389-470`.
`showMore.apiPath` / `viewAll` give you the "see all" page path to `GET` next. [verified]

Slugs actually in use:

| Slug | What |
|---|---|
| `pages/home` | legacy home |
| `pages/explore` | Explore |
| `pages/for_you` | For You |
| `pages/hires` | Hi-Res |
| `pages/videos` | Videos |
| `pages/genre_page`, `pages/genre_page_local` | genres |
| `pages/moods` | moods |
| `pages/my_collection_my_mixes` | My Mixes |
| `pages/my_collection_recently_played` | Recently Played (this is the *only* history surface I found) |
| `pages/rising` | Rising |
| `pages/suggested_new_tracks_for_you`, `pages/suggested_new_albums_for_you` | |
| `pages/show/essential_album` | |
| `pages/album?albumId=` | album page (similar albums, credits blocks) |
| `pages/artist?artistId=` | artist page |
| `pages/mix?mixId=&deviceType=BROWSER` | mix page — this is how you fetch a mix's tracks |

`ref:python-tidal/tidalapi/session.py` (`home`, `explore`, `hires_page`, `for_you`, `videos`,
`genres`, `local_genres`, `moods`, `mixes`), `ref:python-tidal/tidalapi/album.py:311`,
`ref:python-tidal/tidalapi/mix.py:97,219`, `ref:sone/src-tauri/src/commands/pages.rs:941-948`.
[verified]

**v2 shape** — `GET https://api.tidal.com/v2/home/feed/{slug}?countryCode&locale&deviceType=BROWSER&platform=WEB[&cursor=]`.
python-tidal uses `home/feed/static` with `deviceType=BROWSER, locale, platform=WEB`;
Sone parameterises the slug and pages with `json.page.cursor`. Response is
`{items: [ {type, moduleId, title, subtitle, ...} ]}` where item `type` is one of `SHORTCUT_LIST`,
`HORIZONTAL_LIST`, `HORIZONTAL_LIST_WITH_CONTEXT`, `TRACK_LIST`, and inner item types are
`PLAYLIST`, `VIDEO`, `TRACK`, `ARTIST`, `ALBUM`, `MIX` —
`ref:python-tidal/tidalapi/page.py:245-380`, `ref:sone/src-tauri/src/tidal_api.rs:4161-4225`.
[verified]

Both shapes are live simultaneously. python-tidal's `session.home(use_legacy_endpoint=False)`
defaults to v2 and keeps v1 "for backwards compatibility" (HISTORY v0.8.6). Sone fetches the v2 feed
and, if that yields nothing, falls back to concatenating several v1 `pages/*` endpoints —
`ref:sone/src-tauri/src/tidal_api.rs:4332-4410`. streamboat should plan for both. [verified]

There is also a **v2 artist page**: `GET https://api.tidal.com/v2/artist/{id}` with
`countryCode, locale, deviceType=BROWSER, platform=WEB`, falling back to v1 `pages/artist?artistId=`.
Its "view all" paths are v2 paths such as `artist/ARTIST_TOP_TRACKS/view-all?artistId=…&limit=&offset=`
— `ref:sone/src-tauri/src/tidal_api.rs:5181-5290`. [verified]

**Mixes.** A mix id is a string like `0016d…`; `mixType` values seen include `TRACK_MIX`,
`ARTIST_MIX`, `HISTORY_ALLTIME_MIX`, `HISTORY_MONTHLY_MIX`, `HISTORY_YEARLY_MIX` —
`ref:sone/src-tauri/src/tidal_api.rs:844`. `pages/mix` returns a two-category page: category 0 is
the mix header (title, subTitle, images SMALL/MEDIUM/LARGE, `mixType`, `contentBehavior`), category 1
is the items — `ref:python-tidal/tidalapi/mix.py:84-115`. [verified]

**Activity feed (v2).** `GET https://api.tidal.com/v2/feed/activities?userId&countryCode&locale&deviceType&platform`
returns `{activities: [{followableActivity: {...}, seen}], stats: {totalNotSeenActivities}}`, with
payload discriminants like `historyMix`, `album`, and `activityType` values like `NEW_HISTORY_MIX`.
Mark seen with `PUT https://api.tidal.com/v2/feed/activities/seen` —
`ref:sone/src-tauri/src/tidal_api.rs:5848-5900`. [verified]

### 7. User library

**Favorites (v1).** Base `users/{userId}/favorites`.

| Operation | Call |
|---|---|
| list | `GET .../favorites/{artists\|albums\|tracks\|videos}?limit=&offset=&order=&orderDirection=` |
| add album(s) | `POST .../favorites/albums` form `albumId=<id[,id...]>` |
| add artist(s) | `POST .../favorites/artists` form `artistId=<ids>` |
| add track(s) | `POST .../favorites/tracks` form `trackId=<ids>` |
| add video | `POST .../favorites/videos?limit=100` form `videoIds=<id>` (note: **`videoIds`**, plural, unlike the others) |
| remove | `DELETE .../favorites/{artists\|albums\|tracks\|videos}/{id}` — single id only |
| all ids at once | `GET .../favorites/ids` → `{"TRACK":[...], "ALBUM":[...], ...}` (string ids) |

`ref:python-tidal/tidalapi/user.py:289-485`, `ref:sone/src-tauri/src/tidal_api.rs:3066-3100`.
Sone adds `onArtifactNotFound=FAIL` on video adds. [verified]

Sort values (`order`): albums `ARTIST|DATE|NAME|RELEASE_DATE`; artists `DATE|NAME`; items
`ALBUM|ARTIST|DATE|INDEX|LENGTH|NAME`; mixes `DATE|MIX_TYPE|NAME`; playlists `DATE|NAME`; videos
`ARTIST|DATE|NAME`. `orderDirection` is `ASC|DESC`. — `ref:python-tidal/tidalapi/types.py`.
[verified]

**Favorite mixes (v2).** `PUT https://api.tidal.com/v2/favorites/mixes/add?mixIds=a,b&onArtifactNotFound=FAIL`
and `.../remove`, response `{addedItems:[...]}` / `{deletedItems:[...]}`. List with
`GET https://api.tidal.com/v2/favorites/mixes?limit&offset&order&orderDirection&countryCode&locale&deviceType`.
`ref:python-tidal/tidalapi/user.py:407-435, 487-516, 930-960`,
`ref:sone/src-tauri/src/tidal_api.rs:3171-3260`. [verified]

**Playlists and folders (v2, `my-collection`).** This whole subsystem is v2 and cursor-paginated.

| Operation | Call |
|---|---|
| list playlists at root | `GET https://api.tidal.com/v2/my-collection/playlists/folders?folderId=root&limit=50&includeOnly=PLAYLIST&order=DATE&orderDirection=DESC[&cursor=]` |
| list folders | same with `includeOnly=FOLDER` |
| create playlist | `PUT https://api.tidal.com/v2/my-collection/playlists/folders/create-playlist?name=&description=&folderId=root` |
| create folder | `PUT .../folders/create-folder?name=&folderId=root` |
| rename folder | `PUT .../folders/rename?trn=trn:folder:<id>&name=` |
| remove folder/playlist | `PUT .../folders/remove?trns=trn:folder:<id>[,trn:playlist:<uuid>]` |
| move into folder | `PUT .../folders/move?folderId=<id>&trns=trn:playlist:<uuid>,…` |
| favourite a playlist | `PUT .../folders/add-favorites?folderId=root&uuids=<uuid,…>` → `{addedItems:[{trn:"trn:playlist:<uuid>"}]}` |

`ref:python-tidal/tidalapi/user.py:213-258, 315-360, 518-547, 669-807`,
`ref:python-tidal/tidalapi/playlist.py:341-527`. Note the **TRN scheme**: `trn:playlist:<uuid>`,
`trn:folder:<id>`. `includeOnly=""` returns both types and is how python-tidal counts. [verified]

**Playlist contents and mutation (v1, ETag-guarded).**

```
GET    playlists/{uuid}                       -> body + `etag` response header
GET    playlists/{uuid}/tracks?limit&offset&order&orderDirection   -> also returns etag
GET    playlists/{uuid}/items?limit&offset    -> tracks AND videos, each wrapped {item, type}
POST   playlists/{uuid}/items                 If-None-Match: <etag>
       form: trackIds=1,2,3 & toIndex=<n> & onDupes=ADD|SKIP|FAIL & onArtifactNotFound=SKIP|FAIL
       -> {addedItemIds: [...]}                (FAIL is unverified for adds — see note below)
POST   playlists/{uuid}/items                 If-None-Match: <etag>
       form: fromPlaylistUuid=<uuid> & onDupes & onArtifactNotFound     (merge another playlist)
POST   playlists/{uuid}/items/{i,j,k}         If-None-Match; form toIndex=<n>   (reorder)
DELETE playlists/{uuid}/items/{i,j,k}         If-None-Match                     (remove by index)
POST   playlists/{uuid}                       form: title=&description=          (edit metadata)
DELETE playlists/{uuid}
PUT    https://api.tidal.com/v2/playlists/{uuid}/set-public
PUT    https://api.tidal.com/v2/playlists/{uuid}/set-private
GET    playlists/{uuid}/recommendations/items?limit&offset            (suggested additions)
```

`ref:python-tidal/tidalapi/playlist.py:83-92, 201-276, 530-813`,
`ref:sone/src-tauri/src/tidal_api.rs:1963-2100, 2290-2320`. Two things to internalise:

- **Reorder and delete address items by index, not by id.** python-tidal implements `move_by_id` and
  `remove_by_id` by fetching all tracks and computing the index. That is racy and expensive on a
  large playlist. Sone does the same. [verified]
- **The ETag must be re-fetched after every mutation.** python-tidal calls `_reparse()` (a fresh
  `GET playlists/{uuid}`) after each mutating call. Sone re-fetches immediately before each
  mutation and defaults to `*` if absent. [verified]

**`onDupes=FAIL` / `onArtifactNotFound=FAIL` on playlist item adds is unverified — do not assume it
works.** python-tidal only ever sends `ADD`/`SKIP` for `onDupes` and `SKIP` for `onArtifactNotFound`
on this endpoint (`ref:python-tidal/tidalapi/playlist.py:585-599`). `FAIL` is verified only on the
v2 favorites/mixes endpoints (`ref:python-tidal/tidalapi/user.py:418`). If streamboat builds a
playlist-import feature around "fail loudly on a duplicate," test `FAIL` against a live account
before shipping it as documented behavior. **[gap-filled]**

**Collaborative playlists are not reachable from the unofficial API used by any reference client.**
Grepping all checkouts for `collaborat` returns only one unrelated Sone UI string. It *is* modelled
on the official API: `/playlists/{id}/relationships/collaborators`,
`/playlists/{id}/relationships/collaboratorProfiles`, `/collaborationInvites`,
`/collaborationInviteRedemptions`. The unofficial equivalent of "shared" is only the binary
`set-public`/`set-private` toggle above; the official `accessType` is three-valued
(`PUBLIC`|`UNLISTED`|`PRIVATE`) and the unofficial pair cannot express `UNLISTED`. Also worth noting
for the playlist model: official `Playlists_Attributes` additionally carries `playlistType`
(`EDITORIAL`|`USER`|`MIX`|`ARTIST`), `bounded`, `numberOfItems`/`numberOfTrackItems`/
`numberOfVideoItems`, ISO-8601 `duration`, and `numberOfFollowers` —
`ref:tidal-sdk-web/packages/api/src/allAPI.generated.ts:23766-23820`. **[gap-filled]**

**Other user endpoints.** `GET users/{id}/playlists` (created playlists, v1),
`GET users/{id}/playlistsAndFavoritePlaylists?limit=50` (v1; limited to 50 by the server),
`GET https://api.tidal.com/v2/user-playlists/{id}/public?limit&offset`,
`GET https://api.tidal.com/v2/profiles/{id}`. [verified]

**History.** There is no clean "recently played" REST endpoint in any reference implementation. The
only surfaces are the `pages/my_collection_recently_played` page and the `HISTORY_*` mix types.
Writing history is a side-effect of *play reporting* (§8), not a separate call. [verified — absence
confirmed by grepping all checkouts]

### 8. Playback

#### 8.1 `playbackinfopostpaywall` — the load-bearing call

```
GET https://api.tidal.com/v1/tracks/{trackId}/playbackinfopostpaywall
    ?audioquality=HI_RES_LOSSLESS|LOSSLESS|HIGH|LOW
    &playbackmode=STREAM
    &assetpresentation=FULL
    &countryCode=XX
Authorization: Bearer <token>
```
Four independent implementations agree on exactly these parameters and this exact endpoint name:
`ref:python-tidal/tidalapi/media.py:505-520` (`get_stream`),
`ref:sone/src-tauri/src/tidal_api.rs:3653-3672`,
`ref:sone-windows/src-tauri/src/tidal_api.rs`,
`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:132-138`. A fifth and sixth implementation use
the same *parameters* against the shorter `/playbackinfo` form (no `postpaywall`) instead:
`ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:53-58` calls it on `desktop.tidal.com` with
no `countryCode`, and the official web SDK's legacy path
(`ref:tidal-sdk-web/.../playback-info-resolver.ts:178-180`) does the same. Treat `playbackinfopostpaywall`
as the primary form and `/playbackinfo` as an equivalent short form some clients use. [verified,
corrected from "five implementations of playbackinfopostpaywall"]

Parameter domains:

- `audioquality`: `LOW`, `HIGH`, `LOSSLESS`, `HI_RES_LOSSLESS`. `HI_RES` also exists as a legacy tier
  (Sone's cascade includes it; the iOS SDK maps `HI_RES → MQA`). — `ref:python-tidal/tidalapi/media.py:56-63`,
  `ref:sone/src-tauri/src/commands/playback.rs`, `ref:tidal-sdk-ios/Sources/Player/Common/Data/AudioCodec.swift`.
- `playbackmode`: `STREAM` | `OFFLINE`. Only `STREAM` appears in any OSS client.
- `assetpresentation`: `FULL` | `PREVIEW`. `PREVIEW` is the 30-second clip; the response carries
  `previewReason` (`FULL_REQUIRES_SUBSCRIPTION` | `FULL_REQUIRES_PURCHASE` |
  `FULL_REQUIRES_HIGHER_ACCESS_TIER`) — `ref:tidal-sdk-web/packages/api/src/allAPI.generated.ts:25355-25365`.
- `videoquality` (for `/videos/{id}/…`): `HIGH` | `MEDIUM` | `LOW` | `AUDIO_ONLY` —
  `ref:python-tidal/tidalapi/media.py:66-73`.
- prefetch is a *header*, `x-tidal-prefetch: true`, not a query parameter —
  `ref:tidal-sdk-web/.../playback-info-resolver.ts:207-209`.

[verified]

Response fields (union of what the implementations parse):

| Field | Notes |
|---|---|
| `trackId` (or `videoId`) | Strawberry logs a mismatch against the requested id — track substitution is real |
| `assetPresentation` | `FULL` \| `PREVIEW` |
| `previewReason` | only on previews |
| `audioMode` | `STEREO` \| `DOLBY_ATMOS` \| `SONY_360RA` (python-tidal's own `AudioMode` enum only declares `STEREO`/`DOLBY_ATMOS` — a value it doesn't model can still arrive on the wire) |
| `audioQuality` | **what you actually got**, which may be lower than requested |
| `manifestMimeType` | see below |
| `manifest` | base64 |
| `manifestHash` | integrity/dedup key |
| `bitDepth`, `sampleRate` | **null for LOW/HIGH tiers**; python-tidal defaults to 16/44100, the web SDK explicitly annotates "API sends null" |
| `albumReplayGain`, `albumPeakAmplitude`, `trackReplayGain`, `trackPeakAmplitude` | dB and linear peak |
| `licenseSecurityToken` | present on DRM'd assets — `ref:tidal-sdk-web/.../playback-info-resolver.ts:25` |
| `streamingSessionId` | echoed from the `x-tidal-streamingsessionid` header |

`ref:python-tidal/tidalapi/media.py:546-575` (`Stream.parse`),
`ref:sone/src-tauri/src/tidal_api.rs:575-607`,
`ref:TidaLuna/plugins/lib/src/classes/TidalApi/types/PlaybackInfo.ts`. [verified]

Critical: **over-requesting quality returns HTTP 200 with a downgraded `audioQuality`, not an error.**
Sone states this in a comment as the reason not to walk the cascade after a rate-limit or terminal
failure. So the cascade exists to handle *errors*, and the `audioQuality` field is what you display.
[verified]

#### 8.2 Manifest types

**`application/vnd.tidal.bts`** — base64 → JSON:

```json
{
  "mimeType": "audio/flac",
  "codecs": "flac",
  "encryptionType": "NONE",
  "keyId": "<base64>",            // only when encryptionType != NONE
  "urls": ["https://.../....flac?token=..."]
}
```
Take `urls[0]` and hand it to the player. `codecs` values seen: `mp3`, `aac`, `aac+`, `flac`,
`mp4a.40.2` (AAC-LC / HIGH), `mp4a.40.5` (HE-AAC / LOW) —
`ref:tidal-sdk-web/packages/player/src/internal/helpers/manifest-parser.ts:27-45`. python-tidal
normalises with `codecs.upper().split(".")[0]`, so `mp4a.40.2` becomes `MP4A`. [verified]

**Correction: python-tidal's own `ManifestMimeType` enum does not enumerate all four manifest
types.** It declares only `MPD`, `BTS`, and a fifth value the report's four-type list omits, `VIDEO =
"video/mp2t"` — `EMU` and `application/vnd.apple.mpegurl` are commented out in
`ref:python-tidal/tidalapi/media.py:109-118`. Practical consequence: **python-tidal cannot play
videos through the manifest path** (it falls back to `urlpostpaywall` for video, §8.6). Source the
four-type list (BTS/DASH/EMU/HLS) to `tidal-sdk-web/.../constants.ts` and
`tidal-sdk-android/.../ManifestMimeType.kt` only, not to python-tidal. **[gap-filled]**

**`application/dash+xml`** — base64 → an MPEG-DASH MPD. Structure that clients rely on
(`ref:python-tidal/tidalapi/media.py:742-828`, `DashInfo`):
`MPD@mediaPresentationDuration`, `Period[0].AdaptationSet[0]@contentType/@mimeType`,
`.Representation[0]@codecs/@audioSamplingRate/@id`, and inside it
`SegmentTemplate@initialization/@media/@timescale` plus a `SegmentTimeline` of `S@d`/`S@r`.
Segment URLs are built by substituting `$Number$` from `0` to
`1 + 1 + Σ(S.r or 1)`. python-tidal can also synthesise an HLS `.m3u8` from this
(`DashInfo.get_hls()`), which is a neat trick for players that speak HLS but not DASH.

The web SDK additionally mines the MPD by regex: bit depth from the last integer of
`Representation@id` (e.g. `id="FLAC,44100,16"` → 16), codec from `codecs="…"`, sample rate from
`audioSamplingRate="…"`, duration from `mediaPresentationDuration` —
`ref:tidal-sdk-web/packages/player/src/internal/helpers/manifest-parser.ts:105-205`. Sone does the
same for codec with a plain `find("codecs=\"")`. [verified]

Delivery to the player: rather than parse the MPD, **wrap it as a data URI**:
`data:application/dash+xml;base64,<the original base64>` and pass that as the pipeline URI. High
Tide, Sone and Strawberry all do exactly this; GStreamer's `dashdemux` accepts it —
`ref:sone/src-tauri/src/commands/playback.rs`, `ref:strawberry/src/tidal/tidalstreamurlrequest.cpp`.
mopidy-tidal instead writes the MPD to `cache/manifest.mpd` and passes a `file://` URI —
`ref:mopidy-tidal/mopidy_tidal/playback.py`. [verified]

**`application/vnd.tidal.emu`** — base64 → JSON with `urls[]`; used for video
(`ref:sone/src-tauri/src/tidal_api.rs` `get_video_stream_url` calls its struct `EmuManifest`). The
official SDKs treat EMU and BTS with the same JSON parser —
`ref:tidal-sdk-web/.../manifest-parser.ts:290-300`. [verified]

**`application/vnd.apple.mpegurl`** — HLS. Requested by the official SDKs when FairPlay is
available (`manifestType: isFairPlaySupported ? 'HLS' : 'MPEG_DASH'`) and by the iOS SDK always.
Not something a Linux/Windows client will normally ask for. [verified]

#### 8.3 Encryption: what the projects actually do

`encryptionType` in the BTS manifest is `NONE` or `OLD_AES` —
`ref:TidaLuna/plugins/lib.native/src/request/decrypt.ts:40-52` is the only place in these checkouts
that enumerates both.

- **Strawberry refuses.** If `encryptionKey` is non-empty, or `encryptionType`/`securityType` is
  anything other than `NONE`, it emits `StreamURLFailure` with a user-facing message and does not
  play. — `ref:strawberry/src/tidal/tidalstreamurlrequest.cpp`.
- **python-tidal records but does not decrypt.** `StreamManifest` stores `encryption_type` and
  `encryption_key` and exposes `is_encrypted`; for MPD manifests it hardcodes
  `self.encryption_type = "NONE"` with a `TODO: Handle encryption key`. The package depends on
  `pyaes`, so the capability is nominally present, but no code path in the checkout uses it for
  stream decryption. — `ref:python-tidal/tidalapi/media.py:624-680`.
- **Sone avoids the situation.** It drops the Hi-Res tiers entirely when it has no `client_secret`,
  reasoning that those credentials "typically return encrypted DASH streams that require Widevine".
  — `ref:sone/src-tauri/src/commands/playback.rs`.
- **tidal-hifi sidesteps it** by running the actual TIDAL web player inside a castlabs Electron
  build (`"electron": "github:castlabs/electron-releases#v43.0.0+wvcus"` —
  `ref:tidal-hifi/package.json:67`) so Chromium's Widevine CDM handles decryption. tidal-hifi never
  touches a manifest.
- **TidaLuna decrypts `OLD_AES`** using a hardcoded master key. It runs as a mod *inside* the
  official, licensed TIDAL desktop client. I am recording its existence because the brief asks for a
  factual survey; **streamboat must not do this.** It is DRM circumvention, it is the behaviour TIDAL
  has historically pursued legally, and it would make streamboat undistributable on Flathub and in
  distro repos.

[verified]

**The design rule that falls out of this:** treat a non-`NONE` `encryptionType` as "this tier is not
available to us", surface a clear message, and fall back or skip — i.e. Strawberry's behaviour, with
Sone's pre-emptive tier filtering as an optimisation. [inferred]

#### 8.4 The quality cascade

Sone's `quality_tiers(ceiling, has_secret)` is the cleanest implementation
(`ref:sone/src-tauri/src/commands/playback.rs`):

```
ORDER = ["HI_RES_LOSSLESS", "HI_RES", "LOSSLESS", "HIGH"]
start at the user's ceiling; drop the two Hi-Res tiers when there is no client_secret;
the result always contains "HIGH", so it is never empty.
```
and the loop rules:

- network error → propagate immediately, do not try lower tiers;
- rate-limited or terminal-unplayable → propagate immediately (a lower tier will not help);
- anything else → remember it and try the next tier.

tidalt has the same ladder over `urlpostpaywall`
(`ref:tidalt/internal/tidal/api.go:252-300`); Strawberry instead offers a *method* fallback chain
(`playbackinfopostpaywall` → `urlpostpaywall` → `streamUrl` → `playbackinfo`) selectable in settings
(`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:118-146`). [verified]

#### 8.5 The simpler alternatives

`GET tracks/{id}/urlpostpaywall?urlusagemode=STREAM&audioquality=…&assetpresentation=FULL&countryCode=`
returns `{urls: ["https://..."]}` directly — no manifest, no base64. python-tidal's
`Track.get_url()`, tidalt and tidalrs use it. It is **blocked under PKCE sessions** in python-tidal
and does not give you replay-gain or bit-depth metadata. Useful as a fallback and for a
minimum-viable path. `ref:python-tidal/tidalapi/media.py:409-440`, `ref:tidalt/internal/tidal/api.go:304-310`,
`ref:tidalrs/src/track.rs`. [verified]

`GET tracks/{id}/streamUrl?soundQuality=…&countryCode=` is the oldest form, still offered by
Strawberry, returning `{url, trackId, soundQuality, encryptionKey, codec}`. Likely dead or
degraded. [verified in Strawberry; status unverified]

#### 8.6 Video

```
GET https://api.tidal.com/v1/videos/{id}/playbackinfopostpaywall
    ?videoquality=HIGH&playbackmode=STREAM&assetpresentation=FULL&countryCode=XX
```
→ `{videoId, videoQuality, manifestMimeType, manifest}` where the manifest decodes to
`{urls: [...]}`; the URL is an HLS `.m3u8` — `ref:sone/src-tauri/src/tidal_api.rs:3808-3870`.
`GET videos/{id}/urlpostpaywall?urlusagemode=STREAM&videoquality=&assetpresentation=FULL` is the
shortcut form — `ref:python-tidal/tidalapi/media.py:967-985`. Sone plays video through hls.js in the
webview, separate from the audio pipeline. [verified]

#### 8.7 Audio modes and codecs

| Mode | Codec | Status |
|---|---|---|
| `STEREO` | `mp4a.40.5`/HE-AAC (LOW), `mp4a.40.2`/AAC-LC (HIGH), `flac` (LOSSLESS, HI_RES_LOSSLESS) | current |
| `DOLBY_ATMOS` | E-AC-3 JOC — `EAC3` in the unofficial API, `EAC3_JOC` in the official one. The iOS SDK: "Dolby Atmos is delivered in the E-AC-3 (JOC) codec; the quality tier is irrelevant." `AC4` also appears in codec enums. | current |
| `SONY_360RA` | `mha1` (MPEG-H). iOS SDK: "Sony 360 Reality Audio has no codec the client needs, so it is unsupported here." | **removed from TIDAL 2024-07-24** |
| MQA (`HI_RES` tier) | `mqa` | **removed from TIDAL 2024-07-24** |

`ref:tidal-sdk-ios/Sources/Player/Common/Data/AudioCodec.swift:11-120`,
`ref:python-tidal/tidalapi/media.py:118-130`,
`ref:tidal-sdk-web/packages/player/src/internal/types.ts:7`. The removal date and scope come from
press coverage of TIDAL's own announcement (What Hi-Fi, Pocket-lint, Strata-gee, PhoneArena, all
citing 2024-07-24 for MQA **and** 360 Reality Audio, replaced by FLAC and Dolby Atmos). [verified +
documented]

Practical consequence for a desktop client: **Atmos is the only immersive format left, and playing
it means decoding E-AC-3 JOC.** GStreamer can decode E-AC-3 (`gstreamer1.0-plugins-bad` /
`gst-libav`), but JOC object rendering to stereo/binaural is not something the open stack does well.
Every OSS client I read either ignores Atmos or plays the core E-AC-3 bed. Sone's README markets
"Lossless FLAC and MQA streaming" — the MQA mention is stale copy. Treating Atmos as out of scope
for v1 and requesting stereo tiers is defensible. [inferred]

#### 8.8 Replay gain

TIDAL ships four numbers per track. Sone's normalisation formula, which it calls "Tidal-correct":

```
norm_gain = 0.8 * min( 10^((replay_gain + 4) / 20), 1 / peak_amplitude )
```
with `pre_amp = 4.0` and `peak` defaulting to 1.0 when absent —
`ref:sone/src-tauri/src/commands/playback.rs:10-21`. Context selection: album context prefers
`albumReplayGain`/`albumPeakAmplitude`, mixed queues prefer the track values, each falling back to
the other. High Tide instead configures GStreamer's `rgvolume` with
`pre-amp=4.0 fallback-gain=-10 headroom=6.0` and injects tags. Same intent, different mechanism.
[verified]

#### 8.9 Seeking, buffering, and manifest/URL lifetime — the first questions after "I have a URL"

An earlier draft stopped at "take `urls[0]` and hand it to the player." Three follow-on questions
every implementer hits in week one, answered from the checkouts: **[gap-filled]**

- **Seeking works — the CDN honours HTTP Range.** mopidy-tidal's caching proxy in front of
  `https://lgf.audio.tidal.com/` parses a `Range:` request header, expands it against
  `content_length`, and answers `HTTP/1.1 206 Partial Content` with a `Content-Range` header
  (`ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/proxy.py:209-295`) — a caching proxy could not do
  that if the origin did not support byte ranges. Seeking in a BTS FLAC/AAC file is therefore a plain
  HTTP range request; GStreamer's `souphttpsrc` (and equivalents in other stacks) handle this without
  special-casing TIDAL.
- **Manifests (and the URLs inside them) are not permanent.** The official web SDK caches a resolved
  playback-info for exactly one hour (`MANIFEST_EXPIRATION_MS = 3600000`, §2) and re-resolves after
  that. On a 403 from the CDN mid-track, the correct response is to re-issue
  `playbackinfopostpaywall`/`urlpostpaywall` and resume at the current position, not to surface an
  error to the user.
- **Prefetch** is the `x-tidal-prefetch: true` header (already documented in §4), meant to be paired
  with the 1-hour cache to pre-resolve the next queue item before it's needed.
- **Still unverified**: the actual signed-URL TTL inside `urls[0]` itself (as opposed to the 1-hour
  client-side manifest cache). No checkout states this number; treat it as opaque and handle
  expiry-by-403 defensively rather than trying to predict it.

#### 8.10 The official API's playback contract — `/trackManifests/{id}`

The report cites this endpoint in the comparison table and in Open Questions but, in an earlier
draft, never gave its parameters — the first thing an implementer needs if streamboat ever calls it.
**[gap-filled]**

```
GET https://openapi.tidal.com/v2/trackManifests/{id}
    ?manifestType=HLS|MPEG_DASH
    &formats[]=HEAACV1|AACLC|FLAC|FLAC_HIRES|EAC3_JOC     (a set, not a single tier — see below)
    &uriScheme=HTTPS|DATA
    &usage=PLAYBACK|DOWNLOAD
    &adaptive=true|false
    &shareCode=<optional — grants access to UNLISTED resources>
```
Response (`TrackManifests_Attributes`): `uri` (a `data:` URL when `uriScheme=DATA`, which the SDK
splits with `/data:([^;]+);base64,(.+)/` back into the familiar mime-type + base64 pair), `formats[]`,
`hash`, `drmData` (`{certificateUrl, drmSystem: FAIRPLAY|WIDEVINE, initData[], licenseUrl}` —
"Absence implies no DRM"), `trackPresentation: FULL|PREVIEW`, `previewReason`,
`trackAudioNormalizationData`, `albumAudioNormalizationData`.
(`ref:tidal-sdk-web/packages/api/src/allAPI.generated.ts:12886-12938,25348-25368,22567-22573`;
`ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts:286-301,307-320,372-376`.)

Quality→`formats[]` ladder the official player sends (note: **a set of acceptable formats, not a
single requested tier** — there is no quality cascade to walk on this API, because the server picks
within the set you offer):

| Requested quality | `formats[]` sent |
|---|---|
| LOW | `HEAACV1` |
| HIGH | `HEAACV1, AACLC` |
| LOSSLESS | `HEAACV1, AACLC, FLAC` |
| HI_RES / HI_RES_LOSSLESS | `HEAACV1, AACLC, FLAC, FLAC_HIRES` |
| immersive audio (any tier) | append `EAC3_JOC` |

Documented error responses include 403, 404 and 429. This is the endpoint Open Question 8 (below)
asks whether a newly registered third party actually receives full manifests from, or only previews
— unresolved either way.

#### 8.11 Offline is a licensed, DRM-bound concept in TIDAL's own model — sharpens Open Question 4

`playbackmode=OFFLINE` on the unofficial API and `usage=DOWNLOAD` on the official
`/trackManifests/{id}` are the same concept; the iOS SDK switches on it
(`usage: playbackMode == .offline ? .download : .playback`). There is a dedicated sub-status for it,
`4007 USER_CLIENT_NOT_AUTHORIZED_FOR_OFFLINE` (§4) — the server separately checks whether *your
client id* is entitled to offline at all, so a shared/ecosystem client id may simply be refused.
TIDAL's own offline assets are DRM-licensed with expiry: the iOS SDK's `OfflineEngine` models
`NOT_OFFLINED | OFFLINED_AND_VALID | OFFLINED_BUT_NOT_VALID | OFFLINED_BUT_NO_LICENSE |
OFFLINED_BUT_EXPIRED`. The official API models the whole workflow: `/offlineTasks`, `/downloads`,
`/trackFiles/{id}`, `/installations/{id}/relationships/offlineInventory`, `/userOfflineMixes`.
(`ref:tidal-sdk-android/player/common/.../ApiError.kt`;
`ref:tidal-sdk-ios/Sources/Player/OfflineEngine/Data/InternalOfflineState.swift`,
`OfflineState.swift`; `ref:tidal-sdk-web/packages/api/src/allAPI.generated.ts`.)

This sharpens Open Question 4: **a transparent HTTP byte cache of an already-cleartext stream** (what
mopidy-tidal's caching proxy does) **is a materially different thing from requesting
`playbackmode=OFFLINE`**, which asks TIDAL for a licensed, DRM-bound downloadable asset and would
drag DRM into streamboat. Recommendation: build the former if offline caching is wanted at all, and
rule out the latter by policy — it is not a "later, more ambitious" version of the same feature, it
is a different and disqualifying one. **[gap-filled]**

### 9. Lyrics

**Unofficial:** `GET tracks/{id}/lyrics?countryCode=XX` →
`{trackId, lyricsProvider, providerCommontrackId, providerLyricsId, lyrics, subtitles, isRightToLeft}`.
`lyrics` is plain text; **`subtitles` is the time-synced version** — python-tidal's comment on the
field is simply "Contains timestamps as well". A 404 means no lyrics
(python-tidal raises `MetadataNotAvailable`). `ref:python-tidal/tidalapi/media.py:869-895`,
`ref:sone/src-tauri/src/tidal_api.rs:538-553, 3883-3893`. [verified]

I did **not** find, in any checkout, a parser that states the `subtitles` format explicitly. High
Tide's lyrics widget takes a string and splits on lines, with a comment that it "may or may not
contain timestamps" — `ref:high-tide/src/widgets/lyrics_widget.py:95-104`. The
official v2 API models the same data as `lrcText` (LRC) alongside `text`, `direction`
(`LEFT_TO_RIGHT`|`RIGHT_TO_LEFT`) and `technicalStatus` (`PENDING`|`PROCESSING`|`ERROR`|`OK`) —
`ref:tidal-sdk-web/packages/api/src/allAPI.generated.ts:23132-23140`. That strongly suggests
`subtitles` is LRC (`[mm:ss.xx]line`), but **I could not confirm it from a sample**. Flagged as an
open question.

**Official:** `GET https://openapi.tidal.com/v2/lyrics` / `/lyrics/{id}` with a
`tracks/{id}/relationships/lyrics` link. [verified from the OpenAPI types]

### 10. Playback reporting and streaming privileges

#### 10.1 The event producer

TIDAL's clients log plays to `https://ec.tidal.com/api/event-batch`
(`ref:sone/src-tauri/src/tidal_report/event.rs:7`;
`ref:tidal-sdk-android/eventproducer/.../EventProducer.kt:56` names `https://ec.tidal.com` as
`TIDAL_PRODUCTION_TL_CONSUMER_URI`). The wire format is an **AWS SQS `SendMessageBatch` form POST**,
max 10 events per batch:

```
SendMessageBatchRequestEntry.N.Id                              = <uuid>
SendMessageBatchRequestEntry.N.MessageBody                     = <event JSON>
SendMessageBatchRequestEntry.N.MessageAttribute.1.Name         = Name
SendMessageBatchRequestEntry.N.MessageAttribute.1.Value.StringValue = playback_session
SendMessageBatchRequestEntry.N.MessageAttribute.1.Value.DataType    = String
SendMessageBatchRequestEntry.N.MessageAttribute.2.Name         = Headers
SendMessageBatchRequestEntry.N.MessageAttribute.2.Value.StringValue = <headers JSON>
SendMessageBatchRequestEntry.N.MessageAttribute.2.Value.DataType    = String
```
`ref:sone/src-tauri/src/tidal_report/event.rs` `sqs_form`; corroborated by
`ref:tidal-sdk-web/packages/event-producer/src/utils/sqsParamsConverter.ts:8-9`. [verified]

Event body (Sone's "mobile shape", which its tests assert is the one that surfaces in Recently
Played):

```json
{
  "group": "play_log",
  "version": 2,
  "ts": <endTimestampMs>,
  "uuid": "<uuid>",
  "user":   { "id": <uid>, "clientId": <cid>, "sessionId": "<sid>" },
  "client": { "token": "<cid as string>", "deviceType": "mobile",
              "version": "2.205.0", "platform": "android" },
  "payload": {
    "playbackSessionId": "<uuid>",
    "isPostPaywall": true,
    "productType": "TRACK",
    "requestedProductId": "<id>",
    "actualProductId": "<id>",
    "actualAssetPresentation": "FULL",
    "actualAudioMode": "STEREO",
    "actualQuality": "LOSSLESS",
    "startTimestamp": <ms>, "endTimestamp": <ms>,
    "startAssetPosition": 0.0, "endAssetPosition": <seconds>,
    "actions": [],
    "sourceType": "ALBUM|PLAYLIST|ARTIST|MIX",   // omitted entirely when unknown
    "sourceId": "<id>"
  }
}
```
Per-event `Headers` attribute — nine keys exactly, mirroring TIDAL's `HeadersUtils.kt`:
`client-id`, `app-version`, `os-name`, `os-version`, `device-model`, `device-vendor`,
`consent-category` (`NECESSARY`), `requested-sent-timestamp`, `authorization` (bare token, the
`Bearer` prefix lives on the HTTP header). `ref:sone/src-tauri/src/tidal_report/event.rs`
`build_body` / `build_headers`. The `uid`/`cid`/`sid` values come from decoding the JWT access
token's middle segment without signature verification (`parse_claims`). [verified]

Sone's rules, all asserted in its tests:

- report a play only past **30 seconds** ("TIDAL's own rule: a play over 30 seconds counts as a
  stream") — `ref:sone/src-tauri/src/tidal_report/mod.rs`;
- a **sourceless play is accepted but produces no Recently-Played row**, so map every UI surface that
  has a real container id (`album`, `playlist`, `artist`, `mix`, plus `radio`→MIX and
  `artist-tracks`→ARTIST) and leave the rest unmapped;
- outcome classification: 401/403 → refresh once then queue; 5xx → requeue;
  other 4xx or `<BatchResultErrorEntry>` in the body → drop permanently ("SenderFault"), never retry;
- events are persisted to an encrypted on-disk queue (`tidal_report_queue.bin`) so they survive
  restarts;
- **and it deliberately impersonates the TIDAL Android client**: `app-version` pinned to `2.205.0`,
  `os-name: Android`, `device-model: Pixel 7`, `device-vendor: Google`, with a test named
  `event_carries_no_sone_fingerprint` asserting the payload contains neither "SONE" nor the app's
  version. The file's own header calls this a "Private, undocumented endpoint — the same posture as
  SONE's existing streaming calls; best-effort, may not surface."

[verified]

That last point is a real decision for streamboat and I flag it in Open Questions: writing to
Recently Played is a genuinely nice feature (the user's TIDAL history stays coherent across devices),
but the only known way to do it is to send events that claim to come from TIDAL's Android app.

The official web SDK sends the same conceptual event (`playback_session`, with a `payload` whose
field names match one-for-one: `actions`, `actualAssetPresentation`, `actualAudioMode`,
`actualProductId`, `actualQuality`, `endAssetPosition`, `endTimestamp`, `isPostPaywall`,
`playbackSessionId`, `productType`, `requestedProductId`, `sourceId`, `sourceType`,
`startAssetPosition`, `startTimestamp`) via the `event-producer` package, whose README says "This
module is only intended for internal use at TIDAL, but feel free to look at the code."
`ref:tidal-sdk-web/packages/player/src/internal/event-tracking/play-log/playback-session.ts`,
`ref:tidal-sdk-web/packages/event-producer/README.md`. It also sends `streaming_metrics` events:
`streaming_session_start`, `streaming_session_end`, `playback_info_fetch`, `drm_license_fetch`,
`playback_statistics` — `ref:tidal-sdk-web/packages/player/src/internal/event-tracking/streaming-metrics/`.
[verified]

**Event timestamps should be server-anchored, not `SystemTime::now()` — TIDAL ships a dedicated
package for exactly this.** `@tidal-music/true-time`: "Small library to sync time between client and
server, used to ensure event timestamps are accurate." It works by fetching a URL and reading the
HTTP `Date` response header (`this.#serverTime = new Date(response.headers.get('date')).getTime()`),
re-syncing when the cached value is over an hour old; its own tests point it at
`https://api.tidal.com/v1/ping` — an unauthenticated liveness/time endpoint with no other documented
use. The event-producer calls `trueTime.now()` for `sentTimestamp` on every batch.
(`ref:tidal-sdk-web/packages/true-time/README.md`, `src/index.ts:43-56`, `src/index.test.ts:20`;
`ref:tidal-sdk-web/packages/event-producer/src/init.ts:17`, `src/send/send.ts:43`,
`src/monitor/index.ts:77`.) If streamboat implements play reporting, do the same: a clock-skewed
client produces event timestamps that are silently dropped or misordered server-side, in a way that
is hard to attribute from the client side alone. `GET /v1/ping` is also useful on its own as a
connectivity/health probe for the headless mode, before showing a login error. **[gap-filled]**

#### 10.2 Streaming privileges

`POST https://api.tidal.com/v1/rt/connect` → `{ url: "<websocket url>" }`; the client opens that
websocket and uses it to *acquire* streaming privileges (which "may cause other clients to lose
their playback privileges") and to be *notified* that "current streaming privileges for this client
have been revoked". `ref:tidal-sdk-android/player/streaming-privileges/src/main/kotlin/com/tidal/sdk/player/streamingprivileges/`
(`StreamingPrivilegesService.kt`, `StreamingPrivileges.kt`, `StreamingPrivilegesListener.kt`,
`connection/ConnectRunnable.kt`), with the base URL bound to `Common.TIDAL_API_ENDPOINT_V1`
(`ref:tidal-sdk-android/player/src/main/kotlin/com/tidal/sdk/player/Player.kt:69`). [verified]

No OSS Linux client implements this. The consequence of not implementing it: streamboat will not
take over the stream when the user starts playing on another device, and `subStatus 4006` ("streaming
privileges lost") will occasionally appear instead. That is a graceful degradation, not a blocker.
[inferred]

### 11. Images and animated covers

```
https://resources.tidal.com/images/<uuid with '-' replaced by '/'>/<W>x<H>.jpg
https://resources.tidal.com/images/<uuid with slashes>/origin.jpg
https://resources.tidal.com/videos/<uuid with slashes>/<W>x<H>.mp4
https://resources.tidal.com/videos/<uuid with slashes>/origin.mp4
```
`ref:python-tidal/tidalapi/session.py:116-121`. A cover `1e01cdb6-f15d-4d8b-8440-a047976c1cac`
becomes `.../images/1e01cdb6/f15d/4d8b/8440/a047976c1cac/320x320.jpg`. [verified]

Valid sizes are **per entity type** and wrong ones 403 (Sone's code comments say so explicitly):

| Entity | Source field | Valid sizes |
|---|---|---|
| Album cover | `album.cover` | 80, 160, 320, 640, 1280 (square) + `origin` |
| Album animated cover | `album.videoCover` | 80, 160, 320, 640, 1280 (`.mp4`) + `origin`; Sone snaps to 640/1280 |
| Artist picture | `artist.picture` | 160, 320, 480, 750 — "there is no 640 or 1280, which 403" |
| Playlist square | `playlist.squareImage` | 160, 320, 480, 640, 750, 1080 |
| Playlist wide | `playlist.image` | 160x107, 480x320, 750x500, 1080x720 |
| Video thumbnail | `video.imageId` | 160x107, 480x320, 750x500, 1080x720 |
| User picture | `user.picture` | 100, 210, 600 |
| Promo banner (`MULTIPLE_TOP_PROMOTIONS`) | `imageId` | **550x400 only** — "the square sizes 403" |
| Mix | `mix.images.{SMALL,MEDIUM,LARGE}.url` | full URLs are returned in the payload; nominal 320/640/1500 |

`ref:python-tidal/tidalapi/album.py:244-300`, `ref:python-tidal/tidalapi/artist.py:280-300`,
`ref:python-tidal/tidalapi/playlist.py:278-320`, `ref:python-tidal/tidalapi/user.py:121-132`,
`ref:python-tidal/tidalapi/media.py:986-996`, `ref:python-tidal/tidalapi/mix.py:150-168`,
`ref:sone/src/types.ts:1-70`. Strawberry exposes a user-selectable cover size of
160/320/640/750/1280 — `ref:strawberry/src/settings/tidalsettingspage.cpp:74-78` — note that its list
mixes the album set and the artist set. [verified]

Placeholder ids used when an entity has no artwork: album
`0dfd3368-3aa1-49a3-935f-10ffb39803c0`, artist `1e01cdb6-f15d-4d8b-8440-a047976c1cac` —
`ref:python-tidal/tidalapi/album.py:35`, `ref:python-tidal/tidalapi/artist.py:39`. [verified]

Genre images use a different, older host: `http://resources.wimpmusic.com/images/<path>/460x306.jpg`
— `ref:python-tidal/tidalapi/genre.py:52`. Plain HTTP, legacy brand. Probably worth not relying on.
[verified]

Images are unauthenticated: Sone fetches them with a "raw" HTTP client that bypasses the API auth
path, "for non-API hosts only (resources.tidal.com, scrobble providers)" —
`ref:sone/src-tauri/src/tidal_api.rs:1356-1361`. [verified]

### 12. Country, locale, availability

- `countryCode` comes from `GET /v1/sessions` and must be sent on nearly every request. Sone
  defaults to a hardcoded value if the session call fails and logs that it will "use default
  country_code". [verified]
- `locale` is `en_US` in every OSS client; python-tidal has a `TODO Get locale from system
  configuration` in two places. Localised editorial copy on pages presumably follows it.
- Availability signals on a track: `allowStreaming`, `streamReady`, `premiumStreamingOnly`,
  `payToStream`, `adSupportedStreamReady`, `accessType`, `explicit`. Album-level: `streamReady`,
  `adSupportedStreamReady`, `explicit`. The official v2 API replaces these with `availability`
  (`STREAM`|`DJ`|`STEM`, now deprecated in favour of `usageRules`) and `accessType`
  (`PUBLIC`|`UNLISTED`|`PRIVATE`) — `ref:tidal-sdk-web/packages/api/src/allAPI.generated.ts:25648-25705`.
- Region failures surface as `subStatus` 4032/4035 at playback time even when the metadata looked
  available. Handle it there, not only at browse time. [verified]

### 13. What each reference project does — comparison

| Project | API used | Auth | Stream path | Encrypted streams | Notable |
|---|---|---|---|---|---|
| python-tidal 0.8.11 | v1 + v2 + some openapi v2 | device code, PKCE | `playbackinfopostpaywall`, `urlpostpaywall` | records, does not decrypt | the de-facto reference; LGPL-3.0-or-later |
| High Tide | via python-tidal | **PKCE only** | via python-tidal | n/a | GTK4/libadwaita, Flathub, libsecret; GPL-3.0 |
| Sone | v1 + v2 + openapi v2 | device code **and** PKCE, user-supplied creds supported | `playbackinfopostpaywall` + DASH data-URI | skips Hi-Res without a secret | Tauri 2/Rust, play reporting to `ec.tidal.com`, rate gate; GPL-3.0-only |
| sone-windows | same | same | same | same | WASAPI backend, souvlaki media controls |
| Strawberry | v1 (`api.tidalhifi.com`) | PKCE, custom scheme `tidal://login/auth` | four selectable methods | **refuses, with a user-facing message** | Qt6, GPL-3.0 |
| mopidy-tidal | via python-tidal | device code or PKCE (local server on 8989) | MPD→`file://`, BTS→direct URL, optional caching proxy to `lgf.audio.tidal.com` | n/a | headless/server model; Apache-2.0 |
| tidal-hifi | none (wraps the web player) | browser session | Chromium + Widevine (castlabs Electron 43) | handled by CDM | proves the "wrap the web player" path; MIT |
| TidaLuna | v1 via `desktop.tidal.com` + openapi v2 | steals credentials from the official client's Redux store | own fetch + AES for `OLD_AES` | **decrypts** | a mod of the official client; MS-PL |
| tidalt | v1 | device code (plaintext creds) | `urlpostpaywall` with a 4-tier ladder | n/a | Go TUI, FFmpeg + raw ALSA; Apache-2.0 |
| tidalrs | v1 | device code, caller supplies client id | `urlpostpaywall` / `playbackinfopostpaywall` | n/a | Rust library, arc-swap tokens, backoff; MIT |
| tidal-cli | **official** openapi v2 | PKCE with its own registered client `PYVtmSHMTGI9oBUs` | `/trackManifests/{id}` → DASH segments → local file | n/a | the only project here using the sanctioned API for playback; MIT |
| tidal-sdk-web/-android/-ios | official | PKCE, device, client credentials | `/trackManifests/{id}`, `/videoManifests/{id}` | Widevine / FairPlay | Apache-2.0; the Player module is the sanctioned playback path |
| tidalgo, dotnet-tidal-usdk | v1, `api.tidalhifi.com` | **username/password + `x-tidal-token`** | `streamUrl` / `urlpostpaywall` | n/a | dead (2018); useful only as history |
| libopenTIDAL, TidalSwift | v1 | device code | `playbackinfo*`, `streamUrl` | n/a | proactive refresh patterns worth copying |

[verified for every cell except licence strings, which I took from the project survey and spot-checked
against `LICENSE`/`COPYING` where present]

### 14. Legal and terms-of-service posture

#### What TIDAL's own documents say

- **Developer Guidelines (official API):** "playbacks will only be available through our SDKs, namely,
  an official, unmodified version of the TIDAL Player module, and TIDAL will reject any quota
  extension requests for any Offering that attempts to circumvent this." Also: creating certain app
  categories (alarm/ringtone, games/quizzes, voice control, non-interactive webcasting, mixing TIDAL
  content with other services' streams) is prohibited without express written approval.
  [documented, medium confidence — from search excerpts of developer.tidal.com, which I could not
  fetch]
- **Developer Terms:** prohibits accessing the Developer Tools "beyond the scope of these Developer
  Terms or without an authorized TIDAL account", and prohibits text/data mining or scraping of the
  TIDAL Platform. TIDAL "may limit the number of service calls that applications may make, as TIDAL
  deems appropriate, in its sole discretion, without notice"; apps stay "in development" with quota
  limits until formally approved. [documented, medium confidence]
- **Consumer Content Guidelines / Terms:** users agree not to undertake "Circumventing or modifying,
  attempting to circumvent or modify, or encouraging or assisting any other person in circumventing
  or modifying any security technology or software that is part of the TIDAL Services" and not to
  "Reverse-engineer, decompile, disassemble, modify, or create derivative works of any material on
  the TIDAL Services, except where such restriction is expressly prohibited by applicable law".
  [documented, medium confidence]

**Honest reading.** Using an unofficial client with your own paid subscription is not obviously
"circumventing security technology" as long as the client plays only what the service hands it in the
clear. Decrypting `OLD_AES` streams *is* squarely within that prohibition. Using TIDAL's own app
client IDs is a terms problem rather than a copyright problem: it is unauthorised access relative to
the Developer Terms, and it is impersonating a registered client. The realistic risk is not a lawsuit
against streamboat; it is **TIDAL invalidating the shared client IDs**, which has demonstrably
happened (community keys reported broken en masse on 2026-03-21). [inferred, with the March 2026
event documented]

#### How the ecosystem frames itself

- **High Tide** — a top-of-README callout: "Not affiliated in any way with TIDAL, this is a
  third-party unofficial client". `ref:high-tide/README.md`. On Flathub as a verified app. Its
  README does not mention the subscription requirement or the API's unofficial status beyond that
  line; the Flathub listing does state a subscription is needed. [verified + documented]
- **Sone** — README badge: "Requires an active TIDAL subscription. Not affiliated with TIDAL." Full
  Disclaimer section: "SONE is an independent, community-driven project. It is **not affiliated with,
  endorsed by, or connected to TIDAL** in any way. All content is streamed directly from TIDAL's
  service and requires a valid paid subscription. SONE is a streaming client only — it does not
  support offline downloads, and does not redistribute or circumvent protection of any content. As
  with any third-party client, please be aware of TIDAL's terms of use." Plus "All trademarks belong
  to their respective owners." FAQ: "Yes. SONE is a client for TIDAL and requires an active paid
  TIDAL subscription." On Flathub and the Snap Store. `ref:sone/README.md:21, 480, 542`.
  [verified] — **this is the best-worded disclaimer in the ecosystem and the model to copy.**
- **Strawberry** — one line in the feature list: "Unofficial Tidal, Spotify, and Qobuz integration",
  plus in-app text that steers users to supply their own client ID. No legal essay.
  `ref:strawberry/README.md:79`. [verified]
- **tidal-hifi** — describes itself as "The web version of TIDAL running in electron with Hi-Fi
  (High & Max) support thanks to widevine". It ships no credentials and touches no manifest; its
  exposure is limited to shipping a Widevine-enabled Chromium. `ref:tidal-hifi/README.md`. [verified]
- **mopidy-tidal** — Apache-2.0, no disclaimer beyond pointing at python-tidal for API issues.
  [verified]
- **python-tidal** — "Unofficial Python API for TIDAL music streaming service." Nothing more.
  `ref:python-tidal/README.rst`. [verified]
- **dotnet-tidal-usdk** — MIT with an added piracy clause explicitly forbidding DRM circumvention,
  backup copies and offline ripping. An interesting precedent, though bolting conditions onto MIT
  makes the licence non-standard and I would not copy the mechanism. [per project survey]

#### Enforcement history

- 2016: TIDAL's counsel (Reed Smith LLP) filed a DMCA takedown against **TiDown**, a downloader,
  asserting "The code provided by the user can be used to circumvent access controls to copyright
  protected works". The developer disputed the framing. Covered by TorrentFreak and Digital Music
  News. [documented]
- Downloaders such as `Tidal-Media-Downloader` and `tidal-dl-ng` continue to exist publicly, carrying
  their own "Private use only … may be illegal in your country" notices. [documented]
- I found **no evidence of any enforcement action against a player** — High Tide, Sone, Strawberry,
  mopidy-tidal and tidal-hifi are all publicly distributed, several through Flathub, with no
  takedown history. [documented — absence of evidence, stated as such]
- 2026-03-21: community-shared TIDAL API keys reported broken en masse
  (`yaronzz/Tidal-Media-Downloader` issue #1213). No workaround documented in that thread. This is
  the operational risk, and it is recurring: python-tidal's changelog shows credential updates in
  v0.8.7 ("OAuth Client ID, secret updated") and again in v0.8.8 ("Bugfix: OAuth Client ID, secret
  updated") — `ref:python-tidal/HISTORY.rst`. [documented + verified]

#### Packaging implications

| Channel | Feasible? | Notes |
|---|---|---|
| **Flathub** | yes — both High Tide (`io.github.nokse22.high-tide`) and Sone (`io.github.lullabyX.sone`) are listed | Requires an appstream metainfo file, a stable app-id, and — important — a *sandbox-aware* secret story. High Tide's code branches on `Xdp.Portal.running_under_flatpak()`. Flathub does not object to embedded client IDs today. |
| **Snap** | yes — Sone is on the Snap Store | |
| **AUR** | yes, trivially | The AUR already carries `python-tidalapi-git`. |
| **Distro repos (Debian/Fedora)** | plausible but harder | Shipping credentials extracted from a proprietary app is the kind of thing a Debian ftpmaster will ask about. A user-supplied-client-ID mode makes this tractable. |
| **winget / Microsoft Store** | winget yes (it is a manifest pointing at your installer); Store is a signed-app review process and riskier | |
| **Apple App Store / Mac App Store** | effectively impossible | Review would reject an unauthorised third-party client for a subscription service, and sandbox entitlements plus the ToS problem compound. Distribute a notarised `.dmg`/Homebrew cask instead. |
| **Google Play (future mobile)** | very unlikely | Same reasoning, plus Play's stricter impersonation rules. F-Droid is the realistic Android channel — but F-Droid's policy on binaries/credentials extracted from proprietary apps would need checking. |

[documented for Flathub/Snap/AUR; inferred for the rest]

### 15. TIDAL Connect — confirmed permanently out of reach **[gap-filled]**

An earlier draft of this report never mentioned TIDAL Connect. Given the owner's framing —
streamboat "must eventually do everything the native TIDAL client does" — and that Connect (casting
to a Connect-capable DAC/streamer, or being a Connect target) is a headline feature of the native
apps, its absence read as an oversight rather than a researched conclusion. It is the latter:

There is no open protocol and no reference implementation to build against, in either direction. The
`tidal-connect` reference checkout is a docker-compose wrapper around a **closed-source ARM binary**
(`/app/ifi-tidal-release/bin/tidal_connect_application`) that TIDAL distributes only to hardware
partners under a device-certificate program; the repo's own README states outright "This repository
does not contain any tidal-connect binary" and requires the user to separately obtain TIDAL's
binaries, certificate and libraries themselves. It is device-certificate-gated, so streamboat cannot
implement Connect in either direction — no client SDK exists for embedding it, and no protocol
documentation exists for reimplementing it. (Incidentally, the same README corroborates the MQA/HiRes
findings in §8.7 from a different angle: "content above 16/44 available as HI_RES quality (so 24/44
or 24/48) is currently inexistent on Tidal" for the Connect binary post-MQA-removal.)

`ref:tidal-connect/README.md`, `ref:tidal-connect/assets/known-devices.md`,
`ref:tidal-connect/docker-compose.yaml`.

**Recommendation:** state Connect as permanently out of scope in any public roadmap, and offer the
substitutes streamboat *can* build instead: MPRIS (Linux desktop integration), UPnP/DLNA push,
Chromecast, Snapcast, and plain ALSA/PipeWire/WASAPI device selection.

---

## Implications for streamboat

**Architecture**

1. **Isolate the API in one crate/module with no UI dependencies.** Every reference project that
   survived a TIDAL change did so because the API layer was separable (`tidalapi/`,
   `src-tauri/src/tidal_api.rs`, `TidalSwiftLib`). streamboat needs this doubly, because the same
   layer must serve the desktop app and the headless/CLI mode.
2. **Model the transport explicitly**: base URL (v1 / v2 / openapi v2), auth header, the
   auto-injected `countryCode`/`locale`/`deviceType`, `x-tidal-client-version`, retry policy, rate
   gate, refresh single-flight. Copy Sone's `send()` + `RateGate` + `authenticated_get()` shape; it
   is the most defensively written of the lot. **[gap-filled]** Make this a native-side module with
   no browser-origin dependency regardless of UI toolkit — `api.tidal.com` is very likely not
   CORS-enabled for arbitrary origins (§ Summary point 22), so a webview-based UI cannot call it
   directly; this is why Sone puts 100% of its TIDAL code in Rust behind Tauri commands and only lets
   the webview touch `resources.tidal.com` directly.
3. **Make the client ID a first-class, user-replaceable setting**, defaulting to the ecosystem pair,
   exactly as Sone and Strawberry do. This is the single highest-leverage decision: it converts a
   "TIDAL revoked the key, the app is dead" event into a "paste a different key" support answer, and
   it materially improves the distro-packaging story. Surface `subStatus 4022 CLIENT_NOT_FOUND`
   prominently in the UI as the specific signal that this has happened (§4). **[gap-filled]**
4. **Support both auth flows.** Device code is the right default for headless/server/CLI (no browser
   integration needed, user types a code at link.tidal.com, and — unlike the browser flow — it is not
   reCAPTCHA-v3-gated, §3.2). PKCE is required for `HI_RES_LOSSLESS` and is the right default for
   desktop. Store which flow minted the token — refresh must use the matching client ID pair.
5. **Token storage**: OS keyring first (libsecret / Windows Credential Manager / macOS Keychain via
   a crate like `keyring`), encrypted-file fallback for headless and for Flatpak edge cases, mode
   0600. Persist `token_type`, `access_token`, `refresh_token`, `expiry_time`, `is_pkce`,
   `client_id`, **and `client_unique_key`** — generate the latter once at first run and never
   regenerate it; do **not** repeat python-tidal's mistake of dropping `expiry_time` or regenerating
   `client_unique_key` per process (§3.2). **[gap-filled]**
6. **Refresh proactively** at ~5 minutes before expiry (libopenTIDAL/TidalSwift pattern) and
   reactively on 401 *only when the body has no 4xxx `subStatus`* (Sone pattern), behind a
   single-flight lock.
6a. **Implement sign-out as a real server-side call, not just a local token wipe.** `POST
    https://auth.tidal.com/v1/logout` (§2), then erase local tokens regardless of the response.
    **[gap-filled]**

**Playback**

7. Implement `playbackinfopostpaywall` with the descending cascade and Sone's stop conditions
   (network / rate-limited / terminal → stop; anything else → next tier). Display the returned
   `audioQuality`, not the requested one. **Do not copy Sone's blanket "4034 is terminal"** — retry
   once at a lower tier or with a different client id before giving up on a `4034` (§4).
   **[gap-filled]**
8. Handle three manifest types: BTS (JSON, take `urls[0]`), DASH (re-wrap the original base64 as
   `data:application/dash+xml;base64,…` — do not re-encode, do not parse unless you need metadata),
   EMU (video, JSON `urls[]`).
9. **Refuse anything with `encryptionType != "NONE"`** and say why, in Strawberry's words. Do not
   ship AES/Widevine handling. Filter Hi-Res tiers out of the cascade when no client secret is
   configured, so users hit the wall less often.
10. Cache manifests in memory for the session only, with the 1-hour expiry the official SDK uses;
    do not persist them.
11. Apply replay gain with Sone's formula, album context by default, track context for shuffled
    queues, and make it switchable.
12. Treat Dolby Atmos and Sony 360RA as out of scope for v1: 360RA is gone from the service, and
    Atmos needs E-AC-3 JOC handling the open stack does not do well. Still parse `audioModes` so the
    UI can label them and so you never request a tier that returns one unexpectedly.
13. Support `urlpostpaywall` as a fallback path — it is the simplest thing that works and it is what
    keeps tidalt and tidalrs alive.

**Catalog and library**

14. Build for **both page shapes** (v1 `rows[].modules[]` and v2 `items[]` + cursor) from day one.
    Model a page as a list of typed sections and dispatch on `type`, ignoring unknown types rather
    than failing the page — Sone drops unparseable sections and logs the count; python-tidal warns.
15. Playlist mutation needs an ETag round-trip and index-based addressing. Wrap it in one place with
    a "fetch etag → mutate → refetch" helper, and be honest in the UI that reorder on a huge playlist
    is a heavy operation.
16. Use the v2 `my-collection/playlists/folders` cursor pagination correctly — `offset` is ignored
    there.
17. `GET users/{id}/favorites/ids` is the cheap way to render "is favourited" state across a whole
    view. Fetch it once per session and maintain it locally.

**Reporting**

18. Play reporting to `ec.tidal.com` is optional, off-by-default-able, and requires impersonating
    TIDAL's Android client. Decide deliberately (see Open Questions). Local scrobbling
    (Last.fm/ListenBrainz) is the uncontroversial alternative and every reference project has it.
19. Skip the `rt/connect` streaming-privileges websocket for v1 and simply handle `subStatus 4006`
    gracefully.

**Posture and packaging**

20. Adopt Sone's disclaimer wording nearly verbatim: independent project, not affiliated with or
    endorsed by TIDAL, requires an active paid subscription, streaming client only, no offline
    downloads, no circumvention, trademarks belong to their owners, be aware of TIDAL's terms.
    Put it in the README **and** in the app's About dialog.
21. Features to avoid, permanently and by policy stated in `CONTRIBUTING`: track export/download to
    file, DRM decryption of any kind, credential-sharing/multi-account pooling, anything that plays
    without a logged-in subscriber, and — spelled out explicitly rather than left implicit —
    requesting `playbackmode=OFFLINE`/`usage=DOWNLOAD` from TIDAL (§8.11). A **cache for the
    logged-in subscriber** (a transparent HTTP byte cache of an already-cleartext stream: bounded,
    encrypted or at least opaque, invalidated on logout, not exposed as files the user can copy out)
    is a different thing from a downloader and mopidy-tidal already does it; it is defensible but
    should be off by default and clearly scoped. Do not conflate the two — see the correction to
    Open Question 4. **[gap-filled]**
22. Target Flathub + AUR + winget + a notarised macOS build. Do not plan on any app store.
23. **TIDAL Connect is out of scope, permanently, not provisionally** — no open protocol exists to
    build against (§ Summary point 21). Note it explicitly in any public roadmap so it doesn't read
    as an oversight, and point at the substitutes (MPRIS/UPnP/Chromecast/Snapcast) instead.
    **[gap-filled]**

---

## Open questions

Things only the owner can decide:

1. **Official API or unofficial API as the primary spine?** The unofficial one is the only path to
   playing audio yourself. But streamboat could use the official API for metadata and the unofficial
   one only for `playbackinfopostpaywall`. That reduces breakage surface at the cost of a second
   registered client and possible review friction. My recommendation: unofficial for everything now,
   with the transport layer abstract enough to move metadata later.
   - **1a. [gap-filled] "Compliant-but-Chromium" vs "native-but-unofficial" — this is really two
     sub-questions and an earlier draft only implicitly answered the second.** `@tidal-music/player`
     is a published, Apache-2.0 npm package that **is** "an official, unmodified version of the
     TIDAL Player module" — the only fully ToS-compliant way for a third party to play full-quality
     audio. It needs a Widevine-CDM browser runtime (the castlabs-Electron trick `tidal-hifi` uses)
     and cannot run headless or with a native GStreamer/Qt pipeline. Given the owner's explicit
     headless/server requirement, "native-but-unofficial" (what High Tide/Sone/Strawberry do) is
     close to forced — but say so as a decision, not a default. See §1.
2. **Ship the ecosystem client ID, or require the user to supply one?** Shipping it is what every
   Linux client does and is the only way to get a working out-of-box experience. Requiring one is
   the cleanest legally and the cleanest for Debian/Fedora. A hybrid (ship it, allow override,
   document what it is) is what Sone does. Runtime discovery of a web-player client id (§3.4) is a
   further fallback worth considering for the PKCE path specifically.
3. **Write plays back to TIDAL's Recently Played?** It needs client impersonation of TIDAL's Android
   app. Options: don't do it; do it opt-in and off by default; do it on by default. My inclination is
   opt-in, off by default, with the impersonation stated plainly in the setting's description. If
   built, generate one UUID per playback and reuse it for both `x-tidal-streamingsessionid` and
   `playbackSessionId` (§4, §10.1) — [gap-filled].
4. **Offline cache scope.** None / in-memory only / bounded on-disk cache for the current session /
   full "make available offline" for a logged-in subscriber. Each step increases both usefulness and
   the distance from "player" toward "downloader". **[gap-filled, §8.11]: these are not points on one
   continuum.** A transparent HTTP byte cache of an already-cleartext stream is one thing; requesting
   `playbackmode=OFFLINE`/`usage=DOWNLOAD` asks TIDAL for a licensed, DRM-bound asset (with its own
   sub-status, `4007`, and its own expiring-license states) and is a different, disqualifying thing.
   Recommend ruling the latter out by policy rather than treating it as "offline, but more so."
5. **How much does hi-res matter?** Committing to `HI_RES_LOSSLESS` means committing to the PKCE
   flow with the paste-the-URL or embedded-webview login, which is the ugliest part of the whole UX
   — and, per §3.2, is reCAPTCHA-v3-gated in a way device code is not, which is a second reason it's
   the harder path, not just an uglier one. A LOSSLESS-only v1 with device-code login is dramatically
   simpler.
6. **Video.** Music videos are a whole second pipeline (HLS, a video surface, a separate quality
   setting). In or out for v1?
7. **[gap-filled] TIDAL Connect.** Confirmed permanently out of reach (§ Summary point 21) — not an
   open question about feasibility, but the owner should decide which substitutes to build and when:
   MPRIS, UPnP/DLNA push, Chromecast, Snapcast, plain device selection.

Things I could not verify and that someone should check against a live account:

8. **The exact word-level timing of `tracks/{id}/lyrics` → `subtitles`.** **[gap-filled, no longer
   fully open]** This is confirmed to be LRC-shaped: Sone parses it with `[mm:ss.xx]line` regex
   (`ref:sone/src/lib/lrc.ts`, `parseLrc`, accepting `.` or `:` before 1-3 fractional digits) and
   High Tide matches `[m:ss.xx]text` (`ref:high-tide/src/widgets/lyrics_widget.py:105`); the official
   v2 API models the same data as `lrcText`. What remains open is only whether any track ever carries
   word-level (as opposed to line-level) timing — neither reference parser looks for it.
9. **Whether the official `/trackManifests/{id}` endpoint actually returns full-track manifests to a
   newly registered third-party client, or only 30-second previews.** Developers in
   `tidal-music` Discussion #179 raise exactly this doubt and get no answer. tidal-cli's code assumes
   full tracks. Untested here. Moot in practice if streamboat rules out the "compliant-but-Chromium"
   path (1a) — but relevant if it doesn't.
10. **Current rate limits.** Nobody has published numbers; Discussion #269 asked and got no reply
    (that "no reply" status is itself **unverified** in this pass — see Unverified section below). A
    second thread, Discussion #285, is confirmed unanswered and its author's own practice (throttling
    to 1 request/500ms) is the only concrete community number available. Sone's 5 s / 120 s cooldowns
    are guesses that work, not measured limits. **[gap-filled]**
11. **Whether the shared `<client_id A>` / `<client_id B>` pair is still valid today.** The
    March 2026 breakage report is about **legacy pre-OAuth keys, not these OAuth client IDs** — see
    the correction to Summary point 19. python-tidal's changelog (two credential-rotation entries) is
    the better evidence that these do get rotated periodically. python-tidal 0.8.11's values are what
    the current checkout ships; whether they work in September 2026 is untested. **[corrected scope]**
12. **`x-tidal-client-version` semantics** — whether an out-of-date value degrades or blocks
    anything. python-tidal pins `2025.7.16`, Sone `2025.11.3`; neither explains why.
13. **What `Config(alac=...)` in python-tidal still does.** The docstring makes strong claims ("ALAC
    =false will mean that video streams turn into audio-only streams … num_videos will turn into
    num_tracks in playlists") but the flag is assigned (`session.py:140,144`) and never read anywhere
    else in the module — confirmed by grep, not just by inspection. Vestigial from the
    `x-tidal-token` era; do not model it.
14. **The exact text of TIDAL's Developer Terms, Developer Guidelines and consumer Terms.**
    `developer.tidal.com`, `support.tidal.com` and `tidal.com` are all blocked from this session, so
    every quotation in §14 comes from search-result excerpts. **`torrentfreak.com` is also blocked**
    and was not flagged as such in an earlier source list — its content (the 2016 TiDown DMCA
    coverage) is likewise second-hand. These need to be read directly before any of that wording goes
    into a public-facing document. **[corrected]**
15. **Flathub's current stance** on apps embedding credentials extracted from proprietary clients.
    Both High Tide and Sone are listed today, so it is at minimum tolerated, but I could not fetch
    flathub.org to check for any policy statement.
16. **`api.tidal.com`'s CORS posture** — asserted in Summary point 22 from architecture and a
    community doc, not from a live preflight request. Re-verify with
    `curl -I -H 'Origin: https://example.com' https://api.tidal.com/v1/sessions` before treating it
    as settled. **[gap-filled, partially unverified]**

---

## Sources

### Reference checkouts (read directly)

- `ref:python-tidal/tidalapi/session.py` — `Config` base URLs and the four embedded client
  credentials; device-code and PKCE flows; `token_refresh`; `GET /v1/sessions`; search; all page
  helper methods; ISRC/UPC lookups against openapi v2.
- `ref:python-tidal/tidalapi/request.py` — injected `sessionId`/`countryCode`/`limit`;
  `x-tidal-client-version: 2025.7.16`; Android User-Agent; the `"The token has expired."`
  string-matched refresh; error-body shapes.
- `ref:python-tidal/tidalapi/media.py` — quality/video-quality/audio-mode/codec/mime enums;
  `Track.get_stream()` parameters; `Stream` response fields; `StreamManifest` BTS/MPD handling;
  `DashInfo` MPD parsing and HLS synthesis; `Lyrics`; `Video` and its image sizes.
- `ref:python-tidal/tidalapi/user.py` — favorites CRUD, order enums, counts via `limit=1`,
  cursor-paginated v2 playlist folders, `create-playlist`/`create-folder`.
- `ref:python-tidal/tidalapi/playlist.py` — ETag lifecycle, `/items` vs `/tracks`, add/move/remove by
  index, `set-public`/`set-private`, Folder TRNs, playlist image sizes.
- `ref:python-tidal/tidalapi/album.py`, `artist.py`, `mix.py`, `genre.py`, `types.py`,
  `exceptions.py` — entity endpoints, image size tables, `pages/mix`, 404/429 mapping.
- `ref:python-tidal/HISTORY.rst`, `pyproject.toml` — version 0.8.11, LGPL-3.0-or-later, the repeated
  "OAuth Client ID, secret updated" entries.
- `ref:high-tide/src/login.py`, `src/lib/secret_storage.py`, `src/window.py`,
  `build-aux/python3-tidalapi.json`, `README.md` — PKCE-only login, libsecret storage and the Flatpak
  branch, quality selection, the pinned `tidalapi-0.8.8` wheel, the "not affiliated" callout.
- `ref:sone/src-tauri/src/tidal_api.rs` — all four base URLs, `x-tidal-client-version: 2025.11.3`,
  sub-status ranges and terminal set, `authenticated_get` refresh policy, `send()` rate gate,
  device/PKCE token calls, `get_stream_url`, `get_video_stream_url`, v2 search/suggestions/home
  feed/artist page/feed, favorites and playlist endpoints, ETag flow, openapi-v2 playlist create and
  patch.
- `ref:sone/src-tauri/src/commands/playback.rs` — `quality_tiers`, cascade stop conditions,
  `compute_norm_gain`, DASH data-URI construction.
- `ref:sone/src-tauri/src/rate_gate.rs` — cooldown semantics, `Retry-After` parsing, 120 s clamp.
- `ref:sone/src-tauri/src/tidal_report/event.rs`, `mod.rs` — `ec.tidal.com/api/event-batch`, the SQS
  form encoding, the `playback_session` body and nine-key headers, JWT claim extraction, the 30 s
  threshold, source mapping, outcome classification, the anti-fingerprint test.
- `ref:sone/src-tauri/src/embedded_config.rs` — XOR-masked client credentials.
- `ref:sone/src-tauri/src/commands/auth.rs` — `resolve_credentials`, PKCE authorize URL, redirect URI.
- `ref:sone/src-tauri/src/commands/pages.rs` — the list of v1 page slugs.
- `ref:sone/src/types.ts` — per-entity image size rules and the 550x400 promo note.
- `ref:sone/README.md` — the disclaimer wording, Flathub/Snap listing, subscription requirement.
- `ref:strawberry/src/tidal/tidalservice.cpp`, `tidalbaserequest.cpp`, `tidalstreamurlrequest.cpp`,
  `tidalrequest.cpp`, `src/settings/tidalsettingspage.cpp` — `api.tidalhifi.com/v1`,
  `login.tidal.com` OAuth with `tidal://login/auth`, the four stream-URL methods, the encrypted-stream
  refusal and its message, cover sizes, custom-client-ID setting.
- `ref:mopidy-tidal/mopidy_tidal/playback.py`, `web_auth_server.py`,
  `gstreamer_proxy/__init__.py` — MPD-to-`file://`, BTS direct URL, the PKCE paste form, the
  `lgf.audio.tidal.com` caching proxy.
- `ref:tidal-hifi/package.json`, `README.md`, `docs/audio-quality.md` — castlabs Electron
  `v43.0.0+wvcus`, the wrap-the-web-player model, the 192 kHz flag.
- `ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts`, `types/PlaybackInfo.ts`,
  `plugins/lib.native/src/request/decrypt.ts`, `plugins/lib/src/redux/types/store/content/Track.ts` —
  `desktop.tidal.com/v1`, `x-tidal-token` as client id, the playbackinfo response type, the
  `NONE`/`OLD_AES` enumeration, the `MediaMetadataTag` union including `MQA` and `SONY_360RA`.
- `ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts` — legacy
  playbackinfo parameters and headers, `x-tidal-prefetch`, sub-status → error mapping, the 1-hour
  manifest expiry, `audioQualityToFormats`/`audioFormatsToQuality`, `/trackManifests/{id}` and
  `/videoManifests/{id}` calls.
- `ref:tidal-sdk-web/packages/player/src/internal/helpers/manifest-parser.ts`, `internal/constants.ts`
  — the four manifest MIME types, BTS/EMU JSON parsing, DASH regex mining, codec strings.
- `ref:tidal-sdk-web/packages/player/src/config.ts` — `apiUrl` `openapi.tidal.com/v2/`, `legacyApiUrl`
  `api.tidal.com/v1`.
- `ref:tidal-sdk-web/packages/player/src/player/shakaPlayer.ts` — Widevine at
  `api.tidal.com/v2/widevine`, FairPlay at `fp.fa.tidal.com`.
- `ref:tidal-sdk-web/packages/auth/src/auth/auth.ts` — `login.tidal.com/` and `auth.tidal.com/v1/`,
  PKCE/device/client-credentials/refresh/`update_client` bodies, known auth sub-statuses, the
  single-flight refresh guard.
- `ref:tidal-sdk-web/packages/api/src/api.ts`, `retry.ts`, `allAPI.generated.ts` — the official v2
  base URL and JSON:API content type, the retry policy numbers, the 256-path surface (see
  `.claude/skills/tidal-api/references/official-api.md` for the enumeration),
  `TrackManifests_Attributes`, `DrmData`, `AudioNormalizationData`, `Lyrics_Attributes`,
  `Tracks_Attributes`.
- `ref:tidal-sdk-web/packages/event-producer/README.md`, `src/utils/headerUtils.ts`,
  `src/utils/sqsParamsConverter.ts` — "only intended for internal use at TIDAL", the header key set,
  the SQS batch encoding.
- `ref:tidal-sdk-web/packages/player/src/internal/event-tracking/play-log/playback-session.ts` — the
  canonical `playback_session` payload field list.
- `ref:tidal-sdk-android/auth/src/main/kotlin/com/tidal/sdk/auth/model/AuthConfig.kt`,
  `player/common/src/main/kotlin/com/tidal/sdk/player/common/Common.kt`,
  `player/streaming-api/.../PlaybackInfoRepositoryDefault.kt`, `.../PlaybackInfoService.kt`,
  `.../ManifestMimeType.kt`, `player/streaming-privileges/**`, `eventproducer/.../EventProducer.kt` —
  the auth/login base URLs, the two API endpoints, the `trackManifestsIdGet` parameters and
  quality→formats ladder, the four manifest MIME types, `broadcasts/{djSessionId}/playbackinfo`,
  `POST rt/connect`, `https://ec.tidal.com`.
- `ref:tidal-sdk-ios/Sources/Player/Common/Data/AudioCodec.swift`, `AudioMode.swift`,
  `Common/DRM/FairPlayLicenseFetcher.swift` — codec manifest names including `mha1`/`mhm1`/`ac4`,
  the Atmos→E-AC-3 rule, the "Sony 360RA … unsupported here" comment, the FairPlay URLs.
- `ref:tidal-cli/src/auth.ts`, `src/playback.ts`, `README.md` — a registered public Developer-API
  client (`PYVtmSHMTGI9oBUs`), the full official scope list, `localhost:17893/callback`, the
  `/trackManifests/{id}` call and quality→formats map, DASH segment assembly.
- `ref:tidalt/internal/tidal/client.go`, `internal/tidal/api.go` — plaintext ecosystem credentials
  with a justifying comment, the quality ladder, `urlpostpaywall` parameters.
- `ref:tidalrs/src/lib.rs`, `src/track.rs`, `README.md` — caller-supplied client id, device flow,
  scope strings, backoff, `urlpostpaywall` / `playbackinfopostpaywall`.
- `ref:tidalgo/tidal.go`, `ref:dotnet-tidal-usdk/**` — the dead `POST /v1/login/username` +
  `X-Tidal-SessionId` era, retained only as context for why the client identity determines encryption.
- `ref:libopentidal/Source/OTSessionRefresh.c` — the 300-second proactive refresh margin.

### Web sources

- <https://github.com/api-evangelist/tidal> — third-party profile of the official API; source of the
  "audio bytes flow exclusively through the official TIDAL Player SDK; the Playback API only issues
  signed manifests" formulation and the scope/flow summary. *Third-party paraphrase of TIDAL's docs,
  not TIDAL's own words.*
- <https://github.com/orgs/tidal-music/discussions/179> — third-party app review stalled since 2024;
  April 2026 "nothing has moved"; developer uncertainty about full tracks vs 30-second previews.
- <https://github.com/orgs/tidal-music/discussions/269> — rate limits asked, unanswered.
- <https://developer.tidal.com/documentation/guidelines/guidelines-developer-guidelines> and
  `.../guidelines-developer-terms-1_0`, `.../guidelines-developer-terms-2_0` — the Player-module-only
  requirement, the prohibited-application categories, the scraping and authorised-account clauses,
  the quota language. **Not fetched — blocked by egress policy; content via search excerpts only.**
- <https://tidal.com/content-guidelines>, <https://tidal.com/terms> — consumer prohibitions on
  circumvention and reverse engineering. **Not fetched — blocked; via search excerpts.**
- <https://www.whathifi.com/news/tidal-scraps-mqa-and-spatial-audio-format-heres-what-that-means-for-subscribers>,
  <https://www.strata-gee.com/tidal-says-all-mqa-titles-will-be-deleted-from-the-service-by-july-24th/>,
  <https://www.pocket-lint.com/tidal-is-moving-away-from-mqa-in-favor-of-hires-flac/>,
  <https://www.phonearena.com/news/tidal-removes-mqa-360-reality-audio-formats_id159679> — MQA and
  Sony 360 Reality Audio removed 2024-07-24, replaced by FLAC and Dolby Atmos.
- <https://github.com/yaronzz/Tidal-Media-Downloader/issues/1213> — community API keys reported
  broken 2026-03-21, no workaround.
- <https://torrentfreak.com/tidal-shuts-tidal-downloader-tool-160902/>,
  <https://www.digitalmusicnews.com/2016/09/05/tidown-downloader-taken-down/> — the 2016 TiDown DMCA
  takedown and the exact wording of the claim.
- <https://gist.github.com/riad-uk/3003fa0183b464a0b0d2ca2e77afe477> — the legacy `x-tidal-token`
  values and their documented per-token behaviour (encryption, ALAC vs FLAC, `numberOfVideos`).
- <https://flathub.org/en/apps/io.github.nokse22.high-tide>,
  <https://www.omgubuntu.co.uk/2025/06/high-tide-linux-tidal-streaming-music-client-flathub> — High
  Tide on Flathub as a verified app; subscription required. **Flathub not fetched — blocked.**
- <https://github.com/Nokse22/high-tide> — repository active, not archived, "Not affiliated in any
  way with TIDAL".
- <https://github.com/EbbLabs/python-tidal> — python-tidal's current home (the checkout's
  `pyproject.toml` still points here while the README links `tamland/python-tidal`).
