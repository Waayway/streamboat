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
`DOLBY_ATMOS` (historically also `SONY_360RA`, `MQA`). Check it before requesting Hi-Res —
mopidy-tidal's pattern: if the requested tier is `HI_RES_LOSSLESS` but `HIRES_LOSSLESS` isn't in
`tags`, warn and fall back to `LOSSLESS` before even calling playbackinfo.

**Availability gating**: `allowStreaming` (aka `available`) and `streamReady` are the two booleans to
respect — detail fields are only populated when `available` is true. Album-level `streamReady` and
`adSupportedStreamReady` mirror this. Region failures can still surface as `subStatus 4032/4035` at
*playback* time even when browse-time metadata looked available — handle it there too, not only at
browse time.

## 4. What does NOT exist: charts, new releases, editorial

No dedicated `/charts` REST endpoint exists anywhere in the reference checkouts — stated explicitly,
not by silence: grepping all 23 checkouts for `chart|new_release|new-release` finds only a
`NEW_RELEASE_MIX` mix type and an unrelated cache comment. The surfaces that carry this content are:
the page slugs `pages/rising`, `pages/explore`, `pages/suggested_new_tracks_for_you`,
`pages/suggested_new_albums_for_you` (§5), the `NEW_RELEASE_MIX` mix type (§7's mix types), and on
the official API `/userNewReleaseMixes/{id}` and
`/userRecommendations/{id}/relationships/newArrivalMixes`. Album credits, similarly, are not a
dedicated endpoint the way track credits are — they arrive as a `credits` module inside `pages/album`.

## 5. Pages API (Home, Explore, artist/album/mix pages)

**v1** — `GET pages/{slug}?deviceType=BROWSER&locale=en_US&countryCode=XX`. Response:
`{title, rows: [{modules: [{type, title, pagedList:{items:[…]}, showMore:{apiPath,title}, viewAll}]}]}`.
Dispatch on `module.type`: `PAGE_LINKS_CLOUD`, `PAGE_LINKS`, `FEATURED_PROMOTIONS`,
`MULTIPLE_TOP_PROMOTIONS`, `ALBUM_LIST`, `ARTIST_LIST`, `TRACK_LIST`, `PLAYLIST_LIST`, `VIDEO_LIST`,
`MIX_LIST`, `TEXT_BLOCK`, `ITEM_LIST_WITH_ROLES`, `MIXED_TYPES_LIST`, and `ALBUM_ITEMS` (handled
alongside `ITEM_LIST_WITH_ROLES` — easy to miss, TidaLuna implements it too). **Ignore unknown
module types rather than failing the page.**

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

**Mixes**: a mix id is a string; `mixType` values include `TRACK_MIX`, `ARTIST_MIX`,
`HISTORY_ALLTIME_MIX`, `HISTORY_MONTHLY_MIX`, `HISTORY_YEARLY_MIX`, `NEW_RELEASE_MIX`. `pages/mix`
returns a two-category page: category 0 is the header (title, subTitle, images, `mixType`,
`contentBehavior`), category 1 is the items.

**Activity feed (v2)**: `GET .../v2/feed/activities?userId&countryCode&locale&deviceType&platform` →
`{activities:[{followableActivity, seen}], stats:{totalNotSeenActivities}}`. Mark seen with
`PUT .../v2/feed/activities/seen`.

## 6. User library: favorites

Base `users/{userId}/favorites`.

| Operation | Call |
|---|---|
| list | `GET .../favorites/{artists\|albums\|tracks\|videos}?limit=&offset=&order=&orderDirection=` |
| add album(s)/artist(s)/track(s) | `POST .../favorites/{albums\|artists\|tracks}` form `{albumId\|artistId\|trackId}=<id[,id...]>` |
| add video | `POST .../favorites/videos?limit=100` form `videoIds=<id>` — **plural**, unlike the others |
| remove | `DELETE .../favorites/{type}/{id}` — single id only |
| all ids at once | `GET .../favorites/ids` → `{"TRACK":[...], "ALBUM":[...], ...}` (string ids) — the cheap way to render "is favourited" state across a whole view; fetch once per session |

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
       -> {addedItemIds: [...]}                (FAIL is UNVERIFIED for adds — see note below)
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
- **The ETag must be re-fetched after every mutation.** Fetch etag → mutate → refetch, as one helper.

**`onDupes=FAIL` / `onArtifactNotFound=FAIL` on playlist item *adds* is unverified.** Reference
clients only ever send `ADD`/`SKIP` for `onDupes` and `SKIP` for `onArtifactNotFound` on this
endpoint. `FAIL` is verified only on the v2 favorites/mixes endpoints above. If streamboat builds a
playlist-import feature around "fail loudly on a duplicate," test `FAIL` against a live account
before documenting it as supported behavior.

**Other user endpoints**: `GET users/{id}/playlists` (created playlists, v1),
`GET users/{id}/playlistsAndFavoritePlaylists?limit=50` (**server-capped at 50**),
`GET https://api.tidal.com/v2/user-playlists/{id}/public?limit&offset`,
`GET https://api.tidal.com/v2/profiles/{id}`.

**Sone also creates/edits playlists through the official `openapi.tidal.com/v2` JSON:API** using the
same Bearer token minted by the unofficial device-code flow (`POST/PATCH .../v2/playlists`) —
proof the two APIs share a token and can be mixed freely. See `references/official-api.md`.

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

Genre images use a **different, older host**: `http://resources.wimpmusic.com/images/<path>/460x306.jpg`
— plain HTTP, legacy brand; don't build new features around it.

Images are unauthenticated — Sone fetches them with a client that bypasses the API auth path
entirely, reserved "for non-API hosts only (resources.tidal.com, scrobble providers)." Route image
fetches outside your authenticated API client.

## 11. Country, locale, availability

`countryCode` comes from `GET /v1/sessions` (or, for Strawberry, straight from the OAuth token
response — see `references/auth.md` §6) and must be sent on nearly every request. `locale` is
`en_US` in every unofficial-API client (BCP-47 hyphenated, `en-US`, on the official API — a real
format difference, not a typo). Track-level availability signals: `allowStreaming`, `streamReady`,
`premiumStreamingOnly`, `payToStream`, `adSupportedStreamReady`, `accessType`, `explicit`. The
official v2 API replaces these with `availability` (`STREAM`|`DJ`|`STEM`, now deprecated in favor of
`usageRules`) and `accessType` (`PUBLIC`|`UNLISTED`|`PRIVATE`).
