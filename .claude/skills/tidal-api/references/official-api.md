# The official Developer API (`openapi.tidal.com/v2`)

Full narrative: `docs/research/tidal-api.md` §1 and §8.10.

## Table of contents

1. What it is and why it is bigger than "ISRC lookup and JSON:API playlists"
2. Auth on the official API
3. The enumerated surface
4. `/trackManifests/{id}` — the playback contract
5. The "compliant-but-Chromium" architecture option
6. CORS
7. Format differences from the unofficial API

---

## 1. What it is

Base `https://openapi.tidal.com/v2/`, JSON:API shape (`data`/`attributes`/`relationships`/
`included`, `application/vnd.api+json`). Documented (developer.tidal.com, generated SDKs).
Registration required; third-party app review has reportedly stalled since ~2024 with no TIDAL
staff reply as of an April 2026 follow-up (`tidal-music` GitHub Discussion #179).

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

Sone already uses this API opportunistically for artist bios and artworks
(`{}/artistBiographies/{}`, `{}/artworks`) and for playlist create/patch, **using the same Bearer
token minted by the unofficial device-code flow** — the two APIs are not mutually exclusive and do
not require a second login.

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

## 4. `/trackManifests/{id}` — the playback contract

Fully documented in `references/playback.md` §10 (parameters, response shape, quality→formats
ladder, DRM fields). Summary: it is DRM-protected (`drmData`), and whether a newly-registered
third-party client gets full tracks or previews from it is unresolved (`tidal-music` Discussion
#179).

## 5. The "compliant-but-Chromium" architecture option

`@tidal-music/player` — the package behind `tidal-sdk-web`'s player — is published on **npm**,
**Apache-2.0**, and is the *only fully ToS-compliant way for a third party to play full-quality
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
calls directly. Every unofficial-API OSS client with a webview frontend (Sone, sone-windows) routes
100% of its TIDAL calls through native/backend code and only lets the webview touch
`resources.tidal.com` directly; a community docs repo (`tidal-api-docs`) names CORS explicitly as
the reason browser-based PKCE handoffs don't work in-browser. **This was not independently
re-verified with a live CORS preflight in this pass** — re-check with
`curl -I -H 'Origin: https://example.com' https://api.tidal.com/v1/sessions` before treating it as
settled, but treat it as the working assumption: whatever UI toolkit streamboat picks, the API
client must be a native-side module with no browser-origin dependency, not a "nice to have."

## 7. Format differences from the unofficial API

- Locale: official is BCP-47 hyphenated (`en-US`, `nb-NO`); unofficial is underscored (`en_US`).
- Errors: official/JSON:API is `{"errors": [{"detail": "..."}]}`; unofficial is
  `{"status", "subStatus", "userMessage"}` — see `references/transport.md` §5.
- Content-Type: `POST`/`PATCH`/`DELETE` on the official API require
  `Content-Type: application/vnd.api+json`.
- Pagination: JSON:API `page[cursor]` idioms, with a query serializer that allows reserved
  characters through (`allowReserved: true`) for include-lists and cursor values.
