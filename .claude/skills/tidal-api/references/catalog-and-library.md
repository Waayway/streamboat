# Catalog, pages, library, lyrics, images

Full narrative: `docs/research/tidal-api.md` §5, §6, §7, §9, §11, §12.

## Table of contents

1. Search (v1 and v2)
2. Entity endpoints
3. Track/media fields and availability gating
4. What does NOT exist: charts, new releases, editorial as dedicated endpoints
5. Pages API (Home/Explore/artist/album/mix)
6. User library: favorites
7. Playlists and folders
8. What does NOT exist: collaborative playlists on the unofficial API
9. Lyrics
10. Images and animated covers
11. Country, locale, availability

---

## 1. Search

**v1** — `GET search?query=&limit=&offset=&types=`. Default `types` string (python-tidal):
`artists,albums,tracks,videos,playlists,mixs` (lowercase; note the ungrammatical `mixs`, from
mechanically pluralizing type identifiers). Response: `artists`, `albums`, `tracks`, `videos`,
`playlists` each `{items:[...]}` plus `topHit: {type, value}`. Returns **at most 300 items total**
regardless of `limit`/`offset`.

**v2** — `GET https://api.tidal.com/v2/search` with `query`, `countryCode`, `limit`,
`types=ARTISTS,ALBUMS,TRACKS,PLAYLISTS,VIDEOS` (**uppercase** here), plus `includeContributors=true`,
`includeUserPlaylists=true`, `includeDidYouMean=true`, `supportsUserData=true`, `locale`,
`deviceType`. Sone tries v2 first and falls back to v1 — "web app uses this, returns playlists
properly." Prefer v2 for search.

**Suggestions** — `GET https://api.tidal.com/v2/suggestions/` (trailing slash) with `query`,
`countryCode`, `explicit=true`, `hybrid=true`. Response carries a `history` array plus text
suggestions and "direct hit" entities — this is what the web player's mini-search dropdown uses.

## 2. Entity endpoints

All relative to `https://api.tidal.com/v1/` unless noted; all take `countryCode`.

| Path | Notes |
|---|---|
| `tracks/{id}` | full track |
| `tracks/{id}/lyrics` | see §9 |
| `tracks/{id}/credits` | array of `{type, contributors:[{name,id}]}` |
| `tracks/{id}/radio?limit=` | similar tracks |
| `tracks/{id}/mix` | `{id: "<mixId>"}` — track radio as a Mix |
| `tracks/{id}/playbackinfopostpaywall`, `/urlpostpaywall` | see `references/playback.md` |
| `albums/{id}`, `/tracks`, `/items` (tracks+videos), `/similar` (404 = none), `/review` (`{text}`) | |
| `artists/{id}`, `/albums?filter=EPSANDSINGLES\|COMPILATIONS` (else no filter = main discography), `/toptracks`, `/videos`, `/bio` (`{text}`), `/similar`, `/radio`, `/mix` (`{id}`) | |
| `videos/{id}`, `/urlpostpaywall` (`{urls:[m3u8]}`), `/playbackinfopostpaywall` (EMU manifest) | |
| `mixes/{id}/items` | legacy fallback for mix contents |
| `genres` | `[{name, path, hasPlaylists, hasArtists, hasAlbums, hasTracks, hasVideos, image}]` |
| `genres/{path}/{tracks\|albums\|artists\|playlists\|videos}` | genre images use the legacy `resources.wimpmusic.com` host — see §10 |
| `playlists/{uuid}`, `/tracks`, `/items`, `/recommendations/items` | §7 |
| `users/{id}`, `/subscription` | |
| `rt/connect` (POST) | streaming-privileges websocket — see `references/play-logging-and-privileges.md` |

## 3. Track/media fields and availability gating

`Media.parse`/`Track.parse_track` fields: `id, title, duration (seconds), explicit, popularity,
allowStreaming, streamReady, stemReady, djReady, adSupportedStreamReady, streamStartDate, dateAdded,
trackNumber, volumeNumber, artists[], artist, album, artistRoles, type, payToStream,
premiumStreamingOnly, editable, upload, spotlighted, url, audioQuality, audioModes[], accessType,
mediaMetadata.tags[], index, itemUuid, isrc, description, version, copyright, bpm, key, keyScale,
peak, replayGain, mixes{}`.

`mediaMetadata.tags` is the **availability advertisement**: `LOSSLESS`, `HIRES_LOSSLESS`,
`DOLBY_ATMOS` (historically also `SONY_360RA`, `MQA`). Check it before requesting Hi-Res — the
`HI_RES_LOSSLESS` (enum) vs `HIRES_LOSSLESS` (tag) naming mismatch and mopidy-tidal's pre-flight
check pattern are owned by `tidal-oss-landscape/references/api-auth-streaming.md` §7; cite it
rather than restating.

**Availability gating**: `allowStreaming` (aka `available`) and `streamReady` are the two booleans to
respect — detail fields are only populated when `available` is true. Album-level `streamReady` and
`adSupportedStreamReady` mirror this. Region failures can still surface as `subStatus 4032/4035` at
*playback* time even when browse-time metadata looked available — handle it there too, not only at
browse time.

## 4. What does NOT exist: charts, new releases, editorial

No dedicated `/charts` REST endpoint exists anywhere in the reference checkouts — stated explicitly,
not by silence: grepping all 21 checkouts for `chart|new_release|new-release` finds only a
`NEW_RELEASE_MIX` mix type and an unrelated cache comment. The surfaces that carry this content are:
the page slugs `pages/rising`, `pages/explore`, `pages/suggested_new_tracks_for_you`,
`pages/suggested_new_albums_for_you` (§5), the `NEW_RELEASE_MIX` mix type (§7's mix types), and on
the official API `/userNewReleaseMixes/{id}` and
`/userRecommendations/{id}/relationships/newArrivalMixes`. Album credits, similarly, are not a
dedicated endpoint the way track credits are on the unofficial API — they arrive as a `credits`
module inside `pages/album`. The official API does have a dedicated path, `/credits/{id}` (part of
the 256-entry surface, `references/official-api.md`) — the alternative if streamboat needs album
credits outside the page module.

## 5. Pages API (Home, Explore, artist/album/mix pages)

**v1** — `GET pages/{slug}?deviceType=BROWSER&locale=en_US&countryCode=XX`. Response:
`{title, rows: [{modules: [{type, title, pagedList:{items:[…]}, showMore:{apiPath,title}, viewAll}]}]}`.
Dispatch on `module.type`: `PAGE_LINKS_CLOUD`, `PAGE_LINKS`, `FEATURED_PROMOTIONS`,
`MULTIPLE_TOP_PROMOTIONS`, `ALBUM_LIST`, `ARTIST_LIST`, `TRACK_LIST`, `PLAYLIST_LIST`, `VIDEO_LIST`,
`MIX_LIST`, `TEXT_BLOCK`, `ITEM_LIST_WITH_ROLES`, `MIXED_TYPES_LIST`, `ALBUM_ITEMS`, and `ARTICLE_LIST`
(the last two easy to miss). **Ignore unknown module types rather than failing the page** — new ones
will appear. `showMore.apiPath`/`viewAll` is a **relative v1 path**, fetched with `deviceType` added
(`DESKTOP` for `PageLink.get()`, `BROWSER` by default).

Slugs in use: `pages/home` (legacy), `pages/explore`, `pages/for_you`, `pages/hires`, `pages/videos`,
`pages/genre_page`(`_local`), `pages/moods`, `pages/my_collection_my_mixes`,
`pages/my_collection_recently_played` (**the only history surface**), `pages/rising`,
`pages/suggested_new_tracks_for_you`, `pages/suggested_new_albums_for_you`,
`pages/show/essential_album`, `pages/album?albumId=`, `pages/artist?artistId=`,
`pages/mix?mixId=&deviceType=BROWSER`.

**v2** — `GET https://api.tidal.com/v2/home/feed/{slug}?countryCode&locale&deviceType=BROWSER&platform=WEB[&cursor=]`.
Response: `{items: [{type, moduleId, title, subtitle, ...}]}`, item `type` one of `SHORTCUT_LIST`,
`HORIZONTAL_LIST`, `HORIZONTAL_LIST_WITH_CONTEXT`, `TRACK_LIST`; inner item types `PLAYLIST`,
`VIDEO`, `TRACK`, `ARTIST`, `ALBUM`, `MIX`. **Both shapes are live simultaneously** — build for both
from day one; python-tidal defaults to v2 and keeps v1 "for backwards compatibility," Sone fetches v2
and falls back to concatenating v1 pages if v2 yields nothing.

**v2 artist page**: `GET https://api.tidal.com/v2/artist/{id}` with
`countryCode,locale,deviceType=BROWSER,platform=WEB`, falling back to v1 `pages/artist?artistId=`.
"View all" paths are v2, e.g. `artist/ARTIST_TOP_TRACKS/view-all?artistId=…&limit=&offset=`.

**Pitfall: python-tidal's v2 "view all" is dead code upstream — do not copy it.**
`PageCategoryV2.view_all()` calls `self.session.view_all(api_path)`, but `Session.view_all` does not
exist anywhere in the package (confirmed by grep — no definition site). Following it produces an
`AttributeError`. The only *working* v2 view-all example in any checkout is the artist-page pattern
just above — build from that for any v2 feed row's `viewAll` field, not from python-tidal.

**Mixes**: a mix id is a string; `mixType` values include `TRACK_MIX`, `ARTIST_MIX`,
`HISTORY_ALLTIME_MIX`, `HISTORY_MONTHLY_MIX`, `HISTORY_YEARLY_MIX`, `NEW_RELEASE_MIX`. `pages/mix`
returns a two-category page: category 0 is the header (title, subTitle, images, `mixType`,
`contentBehavior`), category 1 is the items.

**Activity feed (v2)**: `GET .../v2/feed/activities?userId&countryCode&locale&deviceType&platform` →
`{activities:[{followableActivity, seen}], stats:{totalNotSeenActivities}}`. Mark seen with
`PUT .../v2/feed/activities/seen`.

## 6. User library: favorites

Base `users/{userId}/favorites`. **A fifth collection, `playlists`, is easy to miss** and its
parameter names are the ones most likely to break a generated/templated client:

| Operation | Call |
|---|---|
| list | `GET .../favorites/{artists\|albums\|tracks\|videos}?limit=&offset=&order=&orderDirection=` |
| list playlists | `GET .../favorites/playlists?countryCode&limit&offset` — separate from the four above |
| add album(s)/artist(s)/track(s) | `POST .../favorites/{albums\|artists\|tracks}` form `{albumId\|artistId\|trackId}=<id[,id...]>` |
| add video | `POST .../favorites/videos?limit=100` form `videoIds=<id>` — **plural**, unlike the others |
| add playlist | `POST .../favorites/playlists` form `uuid=<uuid>` — **singular `uuid`, a UUID, not `playlistId`** |
| remove | `DELETE .../favorites/{type}/{id}` — single id only |
| remove playlist | `DELETE .../favorites/playlists/{uuid}` |
| all ids at once | `GET .../favorites/ids?countryCode&locale&deviceType` → `{"TRACK":[...], "ALBUM":[...], "ARTIST":[...], "PLAYLIST":[...], ...}` (**all string ids, even for integer-id entity types**) — the cheap way to render "is favourited" state across a whole view; fetch once per session |

**Do not generate the four add/remove calls from one `{type}Id` template** — albums/artists/tracks
use `<type>Id`, videos use `videoIds` (plural), playlists use `uuid` (singular, a UUID not an
integer). A templated client will silently send the wrong field name for videos and playlists.

Sort values: albums `ARTIST|DATE|NAME|RELEASE_DATE`; artists `DATE|NAME`; items
`ALBUM|ARTIST|DATE|INDEX|LENGTH|NAME`; mixes `DATE|MIX_TYPE|NAME`; playlists `DATE|NAME`; videos
`ARTIST|DATE|NAME`. `orderDirection` is `ASC|DESC`.

**Favorite mixes (v2)**: `PUT https://api.tidal.com/v2/favorites/mixes/add?mixIds=a,b&onArtifactNotFound=FAIL`
and `.../remove`; list with `GET .../v2/favorites/mixes?limit&offset&order&orderDirection&countryCode&locale&deviceType`.

## 7. Playlists and folders

**Folders (v2, `my-collection`)** — cursor-paginated:

| Operation | Call |
|---|---|
| list playlists at root | `GET .../v2/my-collection/playlists/folders?folderId=root&limit=50&includeOnly=PLAYLIST&order=DATE&orderDirection=DESC[&cursor=]` |
| list folders | same with `includeOnly=FOLDER` |
| create playlist | `PUT .../folders/create-playlist?name=&description=&folderId=root` |
| create folder | `PUT .../folders/create-folder?name=&folderId=root` |
| rename folder | `PUT .../folders/rename?trn=trn:folder:<id>&name=` |
| remove | `PUT .../folders/remove?trns=trn:folder:<id>[,trn:playlist:<uuid>]` |
| move into folder | `PUT .../folders/move?folderId=<id>&trns=trn:playlist:<uuid>,…` |
| favourite a playlist | `PUT .../folders/add-favorites?folderId=root&uuids=<uuid,…>` |

**TRN scheme**: `trn:playlist:<uuid>`, `trn:folder:<id>`. `includeOnly=""` returns both types.
**`offset` is ignored on this endpoint — use `cursor`.**

**Playlist contents and mutation (v1, ETag-guarded)**:
```
GET    playlists/{uuid}                       -> body + `etag` response header
GET    playlists/{uuid}/tracks?limit&offset&order&orderDirection   -> also returns etag
GET    playlists/{uuid}/items?limit&offset    -> tracks AND videos, each wrapped {item, type}
POST   playlists/{uuid}/items                 If-None-Match: <etag>
       form: trackIds=1,2,3 & toIndex=<n> & onDupes=ADD|SKIP|FAIL & onArtifactNotFound=SKIP|FAIL
       -> {addedItemIds: [...]}                (FAIL attested in Sone's shipped code, not observed live — see note below)
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

Two things to internalize:
- **Reorder and delete address items by index, not by id.** Reference clients implement
  "move/remove by id" by fetching all tracks and computing the index — racy and expensive on a large
  playlist. Wrap it in one place.
- **The ETag must be re-fetched after every mutation.** Fetch etag → mutate → refetch, as one
  helper. Full ETag write-precondition mechanics (default `"*"`, the unexploited read-revalidation
  follow-on) are owned by `tidal-oss-landscape/references/sone-deep-dive.md` §4b — cite it rather
  than re-deriving the pattern.

**`onDupes=FAIL` on playlist item adds: attested in Sone's shipped code, not observed against a
live response.** python-tidal only ever sends `ADD`/`SKIP` for `onDupes` on this endpoint, at both
call sites (`add()` and `merge()`) — but Sone ships `onDupes=FAIL` in production
(`ref:sone/src-tauri/src/tidal_api.rs:1996`, alongside `onArtifactNotFound=FAIL`; a second Sone
call site at `:3636` uses `"SKIP"`), which is a stronger evidence tier than "unverified" — this was
previously understated here. **`onArtifactNotFound=FAIL` *is* independently verified** — python-tidal's
`merge()` (which POSTs to this same endpoint to merge another playlist in) sends `"FAIL"` when
`allow_missing` is false, and the v2 favorites/mixes endpoints use it too. Full write-path/ETag
detail for this endpoint (deepest treatment) is in
`tidal-oss-landscape/references/sone-deep-dive.md` §4b. Still worth one live-account test before
depending on `onDupes=FAIL` in production error handling, since it's shipped-code evidence, not an
observed response.

**Other user endpoints**: `GET users/{id}/playlists` (created playlists, v1),
`GET users/{id}/playlistsAndFavoritePlaylists?limit=50` (**server-capped at 50 — and its items are
NOT bare playlists**: they are `{playlist: {...}, created: "..."}` wrappers; python-tidal rewrites
`item["playlist"]["dateAdded"] = item["created"]` before parsing each one — reproduce that unwrap or
every item silently loses its `dateAdded`),
`GET https://api.tidal.com/v2/user-playlists/{id}/public?limit&offset`,
`GET https://api.tidal.com/v2/profiles/{id}`.

**Sone also creates/edits playlists through the official `openapi.tidal.com/v2` JSON:API** using the
same Bearer token minted by the unofficial device-code flow (`POST/PATCH .../v2/playlists`) —
proof the two APIs share a token and can be mixed freely. **Sone sends plain `Content-Type:
application/json` for this, not the `application/vnd.api+json` the official web SDK sets — it still
works, so treat "requires vnd.api+json" as the SDK's own behavior, not a demonstrated server
requirement.** See `references/official-api.md`.

**Sone's same token also drives official-API *artist* writes** — follower listing, bio/profile-link
editing, and a four-step presigned-S3 cover-art upload (`POST /artworks` → `PUT` the presigned URL,
no auth header, `content-md5` required → poll `GET /artworks/{id}` → `PATCH
/artists/{id}/relationships/profileArt`). This is the only worked binary-upload example in any
checkout and a real feature-scope question ("claim your artist page" is a native-app feature) — see
`references/official-api.md` §8 for the full four-step flow.

**Social features (follow, activity, sharing) are one coherent decision area, not scattered
endpoints.** Follow/unfollow **does not exist on the unofficial API** in any checkout — the only
follower surface anywhere is the official-API relationship `GET /artists/{id}/relationships/followers`
(same artist-tools code above). The activity feed (§5) is unofficial-v2. Collaboration/sharing
(`/shares`, `/dspSharingLinks`, `/collaborationInvites`, §8) is official-API only. Recommend the owner
pick a tier for v1 rather than discover the API boundary mid-implementation: (a) read-only
profile/public-playlist viewing — cheap, unofficial v2; (b) activity feed with seen-marking —
unofficial v2; (c) following/collaboration/sharing — official API only, inherits its review/quota
story (`references/official-api.md`).

## 8. What does NOT exist: collaborative playlists on the unofficial API

Not reachable from the unofficial API used by any reference client. It **is** modeled on the
official API: `/playlists/{id}/relationships/collaborators`,
`/playlists/{id}/relationships/collaboratorProfiles`, `/collaborationInvites`,
`/collaborationInviteRedemptions`. The unofficial equivalent of "shared" is only the binary
`set-public`/`set-private` toggle above; the official `accessType` is three-valued
(`PUBLIC`|`UNLISTED`|`PRIVATE`) and the unofficial pair cannot express `UNLISTED`. The official
`Playlists_Attributes` also carries `playlistType` (`EDITORIAL`|`USER`|`MIX`|`ARTIST`), `bounded`,
`numberOfItems`/`numberOfTrackItems`/`numberOfVideoItems`, ISO-8601 `duration`, `numberOfFollowers`.

## 9. Lyrics

`GET tracks/{id}/lyrics?countryCode=XX` →
`{trackId, lyricsProvider, providerCommontrackId, providerLyricsId, lyrics, subtitles, isRightToLeft}`.
`lyrics` is plain text. 404 means no lyrics (raise/catch as a "not available" case, not an error).

**`subtitles` is LRC** (`[mm:ss.xx]line`, possibly several tags per line) — confirmed by two
reference parsers, not merely inferred from the official API's `lrcText` naming: Sone's
`parseLrc` (accepting `.` or `:` before 1–3 fractional digits) and High Tide's
`[m:ss.xx]text` regex. What remains genuinely open is only whether any track carries **word-level**
(as opposed to line-level) timing — neither parser looks for it.

**Official**: `GET https://openapi.tidal.com/v2/lyrics` / `/lyrics/{id}` with a
`tracks/{id}/relationships/lyrics` link; models the data as `lrcText` alongside `text`, `direction`
(`LEFT_TO_RIGHT`|`RIGHT_TO_LEFT`), `technicalStatus` (`PENDING`|`PROCESSING`|`ERROR`|`OK`).

## 10. Images and animated covers

```
https://resources.tidal.com/images/<uuid with '-' replaced by '/'>/<W>x<H>.jpg
https://resources.tidal.com/images/<uuid with slashes>/origin.jpg
https://resources.tidal.com/videos/<uuid with slashes>/<W>x<H>.mp4   (animated cover)
```
A cover `1e01cdb6-f15d-4d8b-8440-a047976c1cac` becomes
`.../images/1e01cdb6/f15d/4d8b/8440/a047976c1cac/320x320.jpg`.

**Valid sizes are per entity type; a wrong size 403s** — this is not a "reasonable guess," reference
projects hit real 403s and documented the exact valid set:

| Entity | Valid sizes |
|---|---|
| Album cover | 80, 160, 320, 640, 1280 (square) + `origin` |
| Album animated cover | 80, 160, 320, 640, 1280 (`.mp4`) + `origin` |
| Artist picture | 160, 320, 480, 750 — no 640/1280, which 403 |
| Playlist square | 160, 320, 480, 640, 750, 1080 |
| Playlist wide | 160x107, 480x320, 750x500, 1080x720 |
| Video thumbnail | 160x107, 480x320, 750x500, 1080x720 |
| User picture | 100, 210, 600 |
| Promo banner (`MULTIPLE_TOP_PROMOTIONS`) | **550x400 only** — the square sizes 403 |
| Mix | full URLs are returned in the payload (`images.{SMALL,MEDIUM,LARGE}.url`); don't construct these |

Placeholder ids (use when an entity has no artwork): album `0dfd3368-3aa1-49a3-935f-10ffb39803c0`,
artist `1e01cdb6-f15d-4d8b-8440-a047976c1cac`.

**Trap: don't reuse one size list across entity types.** Strawberry's user-facing cover-size setting
offers exactly `160x160`, `320x320`, `640x640`, `750x750`, `1280x1280`
(`ref:strawberry/src/settings/tidalsettingspage.cpp:74-78`) — a union of the album-cover set (80,
160, 320, 640, 1280) and the artist-picture set (160, 320, 480, 750). If that single setting, or any
client-wide "cover size" constant built the same way, is applied to an artist-picture URL instead of
an album-cover URL, `640` and `1280` will 403 (the artist-picture table above tops out at 750). Keep
the per-entity valid-size table above as the source of truth; never a single shared size constant.

Genre images use a **different, older host**: `http://resources.wimpmusic.com/images/<path>/460x306.jpg`
— plain HTTP, legacy brand; don't build new features around it.

Images are unauthenticated — Sone fetches them with a client that bypasses the API auth path
entirely, reserved "for non-API hosts only (resources.tidal.com, scrobble providers)." Route image
fetches outside your authenticated API client.

## 11. Country, locale, availability

`countryCode` comes from `GET /v1/sessions` (or, for Strawberry, straight from the OAuth token
response — see `references/auth.md` §6) and must be sent on nearly every request. Have a defined
fallback for when that call hasn't run yet or fails: Sone's `TidalClient` struct initializes
`country_code` to a hardcoded `"US"` before `get_session_info()` populates it
(`ref:sone/src-tauri/src/tidal_api.rs:1307`) — a reasonable pattern (never send an empty
`countryCode`), but pick the fallback deliberately rather than let an uninitialized field silently
default to one region. `locale` is
`en_US` in every unofficial-API client (BCP-47 hyphenated, `en-US`, on the official API — a real
format difference, not a typo). Track-level availability signals: `allowStreaming`, `streamReady`,
`premiumStreamingOnly`, `payToStream`, `adSupportedStreamReady`, `accessType`, `explicit`. The
official v2 API replaces these with `availability` (`STREAM`|`DJ`|`STEM`, now deprecated in favor of
`usageRules`) and `accessType` (`PUBLIC`|`UNLISTED`|`PRIVATE`).
