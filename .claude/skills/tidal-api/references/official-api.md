# The official Developer API (`openapi.tidal.com/v2`)

Full narrative: `docs/research/tidal-api.md` §1 and §8.10.

## Table of contents

1. What it is and why it is bigger than "ISRC lookup and JSON:API playlists" (includes the ISRC/UPC
   lookup calls)
2. Auth on the official API
3. The enumerated surface
4. `/trackManifests/{id}` and `/videoManifests/{id}` — the playback contracts
5. The "compliant-but-Chromium" architecture option
6. CORS
7. Format differences from the unofficial API
8. Sone's official-API artist-tools surface — the only binary-upload example in any checkout

---

## 1. What it is

Base `https://openapi.tidal.com/v2/`, JSON:API shape (`data`/`attributes`/`relationships`/
`included`, `application/vnd.api+json`). Documented (developer.tidal.com, generated SDKs).
Registration required; third-party app review appears stalled — `tidal-music` GitHub Discussion #179
(opened 2025-06-03, first contact roughly six months earlier per the OP — not "since ~2024") got a
2026-04-16 update: "As of today, nothing has moved. I even tried reaching them via e-mail a few
months ago, but got no reply." No TIDAL staff reply on the thread.

**It is far larger than "ISRC/UPC lookup, JSON:API playlists, lyrics"** — that undersells it. The
`paths` interface in `tidal-sdk-web/packages/api/src/allAPI.generated.ts` declares **256** top-level
path entries (mechanically counted — not "~230"), a 28,093-line generated file. It includes real
product surfaces streamboat might one day want:

- `/dynamicPages` + `/dynamicModules` — the sanctioned editorial-pages API (see §3).
- Full library CRUD: `/userCollections/{id}/relationships/{albums,artists,playlists,tracks,videos}`,
  `/userCollectionAlbums|Artists|Playlists|Tracks|Videos|Folders|SaveForLaters`.
- `/searchResults` (relationships `albums,artists,playlists,topHits,tracks,videos`) and
  `/searchSuggestions` (relationships `directHits,history`).
- `/userRecommendations/{id}/relationships/{discoveryMixes,myMixes,newArrivalMixes,offlineMixes}`,
  plus `/userDailyMixes`, `/userDiscoveryMixes`, `/userNewReleaseMixes`.
- `/playQueues` — a cross-device queue, `repeat: NONE|ONE|BATCH`, `shuffle: OFF|BATCH|ALL`.
- `/tracks/{id}/relationships/{radio,similarTracks,credits,lyrics,usageRules}`,
  `/artists/{id}/relationships/{radio,similarArtists,biography,followers,roles}`,
  `/albums/{id}/relationships/{similarAlbums,items,usageRules,coverArt}`.
- `/artworks`, `/artistBiographies`, `/credits/{id}`, `/genres`, `/shares`, `/dspSharingLinks`
  (Spotify/Apple/Amazon/YouTube cross-links).
- Collaborative playlists: `/playlists/{id}/relationships/{collaborators,collaboratorProfiles}`,
  `/collaborationInvites`, `/collaborationInviteRedemptions` — see
  `references/catalog-and-library.md` §8.
- Offline: `/offlineTasks`, `/downloads`, `/trackFiles/{id}`,
  `/installations/{id}/relationships/offlineInventory`, `/userOfflineMixes` — see
  `references/playback.md` §11 before building toward any of these.
- **ISRC/UPC lookup** — the one thing `SKILL.md`'s open decision 1 cites as covered "alone" by the
  official API:
  ```
  GET https://openapi.tidal.com/v2/tracks?filter[isrc]=<ISRC>&countryCode=
  GET https://openapi.tidal.com/v2/albums?filter[barcodeId]=<UPC/EAN>&countryCode=
  ```
  Not reachable on the unofficial API at all. `ref:python-tidal/tidalapi/session.py`
  `get_tracks_by_isrc`/`get_albums_by_barcode` (lines 890-950); TidaLuna hits the same ISRC filter
  directly (`ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:97`).

Sone already uses this API opportunistically for artist bios and artworks
(`{}/artistBiographies/{}`, `{}/artworks`) and for playlist create/patch, **using the same Bearer
token minted by the unofficial device-code flow** — the two APIs are not mutually exclusive and do
not require a second login. See §8 for the full artist-tools write surface, including a presigned-S3
upload flow — it's larger than "bios and artworks" suggests.

## 2. Auth on the official API

Same host, `auth.tidal.com/v1/oauth2/*`, but **three** grant types are supported, not just PKCE:
device code (`oauth2/device_authorization` + `grant_type:
urn:ietf:params:oauth:grant-type:device_code`), PKCE, and `client_credentials` (app-only, no user —
useful for a metadata-only path, e.g. a logged-out `search` subcommand or a share-link resolver).
Scopes are fine-grained: `collection.read`, `collection.write`, `playlists.read`, `playlists.write`,
`playback`, `user.read`, `recommendations.read`, `entitlements.read`, `search.read`, `search.write`
(tidal-cli's list, the fullest found in source). `grant_type=update_client` exists for adding a
secret to a previously public client. See `references/auth.md` for the unofficial-API flows this
sits alongside.

## 3. The enumerated surface — `/dynamicPages`

The sanctioned editorial-pages endpoint: `GET /dynamicPages?deviceType=BROWSER|CAR|DESKTOP|PHONE|TABLET|TV
&filter[pageType]=…&countryCode=&locale=`, with `pageType`:
`HOME_STATIC|HOME_UPLOADS|HOME_EDITORIAL|HOME_FREE|ARTIST|ALBUM|PLAYLIST|TRACK|VIDEO`. Modules carry
`previewLayout`/`viewAllLayout`: `GRID|LIST|COMPACT|UNKNOWN`, with an explicit forward-compatibility
instruction in the schema docs: "`UNKNOWN` is the forward-compatible default; clients should skip the
module." Copy that rule for any official-API page renderer, the same way §5 of
`references/catalog-and-library.md` says to skip unknown module types on the unofficial pages API.

## 4. `/trackManifests/{id}` and `/videoManifests/{id}` — the playback contracts

Fully documented in `references/playback.md` §10 (parameters, response shape, quality→formats
ladder, DRM fields). Summary: it is DRM-protected (`drmData`), and whether a newly-registered
third-party client gets full tracks or previews from it is unresolved (`tidal-music` Discussion
#179) — one developer on that thread reports first-hand, after PKCE login, that their users "cannot
play but the 30s low quality preview of tracks" (comment dated 2025-09-06); no TIDAL staff reply
confirms or denies it. Treat it as a leaning signal, not a settled answer — see
`references/playback.md` §10 for the full quote and dates.

**`/videoManifests/{id}`** is this endpoint's video sibling — named only in passing elsewhere in this
skill until now:
```
GET https://openapi.tidal.com/v2/videoManifests/{id}
    ?uriScheme=HTTPS|DATA
    &usage=PLAYBACK|DOWNLOAD
```
A simpler parameter set than `/trackManifests/{id}` — no `formats[]`, `manifestType`, or `adaptive`;
video quality is presumably server-selected. Response (`VideoManifests_Attributes`): `link` (a
`Link_Object` — a `data:` URL when `uriScheme=DATA`), `drmData` (same shape as `/trackManifests/{id}`
above), `videoPresentation: FULL|PREVIEW`, `previewReason`. Same DRM caveat applies: decoding it
without a licensed CDM does not work regardless of the contractual question.
(`ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts:429-470`
`_fetchVideoManifest`, `ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json` path
`/videoManifests/{id}`.)

## 5. The "compliant-but-Chromium" architecture option

`@tidal-music/player` — the package behind `tidal-sdk-web`'s player — lives in the
`tidal-music/tidal-sdk-web` monorepo under **Apache-2.0** (public npm publication not independently
confirmed), and is the *only fully ToS-compliant way for a third party to play full-quality
TIDAL audio*, because it **is** "an official, unmodified version of the TIDAL Player module" (the
exact phrase the Developer Guidelines require, `docs/research/tidal-api.md` §14). It is a
Shaka-Player/EME-based browser player, so it needs a Widevine CDM — the same castlabs-Electron trick
`tidal-hifi` already uses (`"electron": "github:castlabs/electron-releases#v43.0.0+wvcus"`) — and it
**cannot run in a headless/Node CLI or with a native GStreamer/Qt/GTK audio pipeline.**

This means the real architecture choice is two options, not one path silently chosen by which
reference projects got read:
- **(a) "compliant-but-Chromium"** — embed the sanctioned Player module, accept an
  Electron/CEF-with-Widevine runtime, give up the headless mode.
- **(b) "native-but-unofficial"** — build a native audio pipeline against the unofficial API, as
  every Linux client the owner named (High Tide, Sone, Strawberry) does.

Given the owner's explicit headless/server requirement, (b) is close to forced — but it is a
decision to state, not one to make silently. See "Open decisions" in `SKILL.md`, item 1a.

## 6. CORS

`api.tidal.com` (the **unofficial** API) is very likely **not** CORS-enabled for arbitrary browser
origins — unlike `openapi.tidal.com/v2` (the official API), which the browser-based `tidal-sdk-web`
calls directly. Every unofficial-API OSS client with a webview frontend (Sone, Sone-windows) routes
100% of its TIDAL calls through native/backend code and only lets the webview touch
`resources.tidal.com` directly. **Genuinely unverified, and weaker-sourced than it may look**: the
`tidal-api-docs` citation once offered in support (`README.md:17`,
`Authorization/Retrieve-From-Authentication-Flow.md:8`) is actually about retrieving the `client_id`
from the web player from within a browser, not about `api.tidal.com`'s CORS response headers — it
does not directly evidence CORS posture. No checkout runs a live preflight. Run `curl -i -H 'Origin:
https://example.com' -X OPTIONS https://api.tidal.com/v1/sessions` and record the result before
treating this as settled — but treat it as the working assumption meanwhile: whatever UI toolkit
streamboat picks, the API client must be a native-side module with no browser-origin dependency, not
a "nice to have."

## 7. Format differences from the unofficial API

- Locale: official is BCP-47 hyphenated (`en-US`, `nb-NO`); unofficial is underscored (`en_US`).
- Errors: official/JSON:API is `{"errors": [{"detail": "..."}]}`; unofficial is
  `{"status", "subStatus", "userMessage"}` — see `references/transport.md` §5.
- Content-Type: the official web SDK sets `Content-Type: application/vnd.api+json` on
  `POST`/`PATCH`/`DELETE`. **This is the SDK's own behavior, not a demonstrated server
  requirement** — Sone POSTs/PATCHes the same `openapi.tidal.com/v2/playlists` endpoints with plain
  `application/json` (reqwest's `.json(&body)`) and it works. Don't block on matching the header
  exactly.
- Pagination: JSON:API `page[cursor]` idioms, with a query serializer that allows reserved
  characters through (`allowReserved: true`) for include-lists and cursor values.

## 8. Sone's official-API artist-tools surface — the only binary-upload example in any checkout

Sone's unofficial-API token also drives official-API *artist* writes, not just playlist writes — the
`§1` bullet list undersells this too. All on `https://openapi.tidal.com/v2` with the same Bearer
token, `Content-Type: application/vnd.api+json`, `x-tidal-client-version`, `?countryCode=`:

- `GET /artists/{id}/relationships/followers` — follower list.
- `PATCH /artists/{id}` — attributes and external links.
- `PATCH /artistBiographies/{id}` — bio text.
- **Cover-art upload, a four-step presigned-S3 flow**:
  1. `POST /artworks` with `{"data":{"type":"artworks","attributes":{"mediaType":"IMAGE",
     "sourceFile":{"md5Hash":"<hex md5>","size":<bytes>}}}}` → response carries `data.id` and
     `data.attributes.sourceFile.uploadLink.href`.
  2. `PUT <uploadLink.href>` — a presigned URL with **no Authorization header**, but
     `content-md5: <base64 of the same md5 digest>` and `Content-Type: image/jpeg` required. The
     digest is sent twice, in two different encodings (hex in the JSON body, base64 in the header) —
     an easy transcription bug.
  3. Poll `GET /artworks/{id}` until it reports ready.
  4. `PATCH /artists/{id}/relationships/profileArt` with
     `{"data":[{"type":"artworks","id":"<artworkId>"}]}`.

This is a genuine feature-scope question, not an implementation detail — "claim your artist page" /
edit bio / edit links / upload art are things the native app does. See
`references/catalog-and-library.md` §7 for where this connects to the social-features scope decision.
