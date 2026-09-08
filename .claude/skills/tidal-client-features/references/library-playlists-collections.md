# Library, playlists, and content pages

Table of contents:
1. Playlists — capabilities, visibility, collaboration, folders
2. My Collection — six lists, sort orders, favorites shape, Save for Later
3. Artist page
4. Album page
5. Track / media item, lyrics, credits
6. History / Recently Played
7. Videos

## 1. Playlists — capabilities, visibility, collaboration, folders

Client capabilities (all `[verified-source]` from `actionTypes.ts` unless noted):

- Create (`folders/CREATE_PLAYLIST`), from album (`content/SHOW_CREATE_PLAYLIST_FROM_ALBUM_DIALOG`),
  from mix (`mix/SHOW_CREATE_PLAYLIST_FROM_MIX_DIALOG`), with AI
  (`folders/CREATE_AI_PLAYLIST`, `modal/SHOW_CREATE_AI_PLAYLIST` — endpoint/rollout unknown,
  **[uncertain]**, treat as out-of-scope for now).
- Edit metadata: title, description, cover (`modal/SHOW_EDIT_PLAYLIST_META`,
  `content/UPDATE_PLAYLIST`).
- Add/remove items (`content/ADD_MEDIA_ITEMS_TO_PLAYLIST`,
  `content/REMOVE_MEDIA_ITEMS_FROM_PLAYLIST`), add a whole album/mix.
- Reorder by drag (`content/MOVE_PLAYLIST_MEDIA_ITEMS`) with **ETag concurrency control**
  (`etag/SET_PLAYLIST_ETAG`) — respect this; a stale-ETag write can clobber a concurrent edit from
  the user's phone.
- Delete (`content/DELETE_PLAYLIST`).
- Suggested tracks (`content/LOAD_PLAYLIST_SUGGESTED_MEDIA_ITEMS`).
- Sorting, folders (see below).

**Visibility is three-state, not binary — get this right from the start.** The desktop client and
python-tidal both present it as a public/private boolean
(`userProfiles/TOGGLE_PUBLIC_PLAYLIST`; legacy v1 API `user-playlists/{id}/public`, `/set-public`,
`/set-private`). The underlying v2 model is `Playlists_Attributes.accessType: PUBLIC | UNLISTED |
PRIVATE`. The legacy v1 boolean cannot represent `UNLISTED` — Sone's own v1→v2 normaliser has to
fake it: `access_type: raw.public_playlist.map(|p| if p {"PUBLIC"} else {"UNLISTED"})`
(`ref:sone/src-tauri/src/tidal_api.rs:526-533`), which silently turns every legacy "private"
playlist into "unlisted." A client that models visibility as a checkbox will get this wrong.
`Playlists_Attributes` also carries `bounded`, `numberOfFollowers`,
`numberOfTrackItems`/`numberOfVideoItems`, `externalLinks` — not in the desktop Redux playlist
model below.

**Collaborative playlists exist as a documented, first-class TIDAL feature.** No `collaborat*`
action appears in the desktop Redux dump — consistent with a mobile/web-first rollout, not with
non-existence. The v2 spec defines the full flow, and — like `/comments` above — the method set is
per-path, not one flat set (verified against
`ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json`): **`/collaborationInvites` is GET
(list/lookup an invite) and POST (an owner mints one); `filter[code]` on the GET is `required: true`
in the spec — it is not an optional lookup filter, that's the only way to resolve an invite code to
a resource. `/collaborationInvites/{id}` is GET and DELETE** (an owner revokes one). Relationships
on an invite: `/collaborationInvites/{id}/relationships/subject` (the playlist it invites into) and
`/relationships/owners`. `/collaborationInviteRedemptions` is where a redeemer redeems a code, and
`playlists/{id}/relationships/{collaborators,collaboratorProfiles}` lists who has joined. The
generated Android SDK has a dedicated `CollaborationInvites` Kotlin interface confirming this is a
real, code-generated API surface
(`ref:tidal-sdk-android/tidalapi/src/main/kotlin/com/tidal/sdk/tidalapi/generated/apis/CollaborationInvites.kt`).

**Playlist folders**: create, rename, move items in/out, remove, add favourites to folder, sort
order (`folders/*`, several `modal/*_TO_FOLDER_MODAL` actions). python-tidal splits the API across
two files — creation/listing in `user.py`, rename (line 428) and move (line 519) in `playlist.py`
(don't cite `user.py` alone for the whole folder API). **Reading folders at scale needs
pagination**: `GET https://api.tidal.com/v2/my-collection/playlists/folders` takes `folderId`,
`offset`, `limit`, `order`, `orderDirection`, `countryCode`, `locale`, `deviceType`, optional
`includeOnly`, and a `cursor` (response carries a `cursor` for the next page). There is also
`my-collection/playlists/folders/flattened`, returning every playlist across all folders with the
same cursor pagination (Sone caps its own loop at 40 pages of 50,
`ref:sone/src-tauri/src/tidal_api.rs:3313-3400`). **The folder screen is a real client route**
(`route/LOADER_DATA__FOLDER`), not merely a sidebar disclosure triangle.

Playlist model (`ref:TidaLuna/plugins/lib/src/redux/types/store/content/Playlist.ts`):
`uuid, title, creator.id, description, duration, numberOfTracks, numberOfVideos, created,
lastUpdated, lastItemAddedAt, type:"USER", publicPlaylist, url, image, squareImage,
customImageUrl, promotedArtists[], popularity`. Editorial playlists have `type` values other than
`"USER"`.

**API**: `playlists/{uuid}`, `playlists/{uuid}/items`, `users/{id}/playlists`,
`users/{id}/playlistsAndFavoritePlaylists`, `my-collection/playlists/folders`,
`my-collection/playlists/folders/{create-folder,create-playlist,rename,move,remove,add-favorites}`
(`ref:python-tidal/tidalapi/{playlist,user}.py`).

## 2. My Collection — six lists, sort orders, favorites shape, Save for Later

Six lists, each with its own route loader and sort-order context menu: Tracks, Albums, Artists,
Playlists, Videos, Mixes & Radio. Sort orders (`ref:python-tidal/tidalapi/types.py`):

| List | Orders |
| --- | --- |
| Albums | `ARTIST`, `DATE` (added), `NAME`, `RELEASE_DATE` |
| Artists | `DATE`, `NAME` |
| Items (tracks/videos) | `ALBUM`, `ARTIST`, `DATE`, `INDEX`, `LENGTH`, `NAME` |
| Mixes | `DATE`, `MIX_TYPE`, `NAME` |
| Playlists | `DATE` (created), `NAME` |
| Videos | `ARTIST`, `DATE`, `NAME` |

Direction: `ASC` / `DESC`.

**Favorites shape**: a local *id cache*, not a list of item objects, and its element types are
mixed, not uniformly numeric — get both of those wrong and the model breaks
(`ref:TidaLuna/plugins/lib/src/redux/types/store/index.ts:102-110`):

```
Favorites {
  albums: number[], artists: number[], tracks: number[], videos: number[],   // numeric TIDAL ids
  mixes: string[], playlists: string[], users: string[]                      // string ids (UUIDs)
}
```

This is a flat object of bare arrays (not `{items: [...]}` wrapped) — that part is confirmed and
correct as-is, and is the *opposite* shape from `UserProfile`'s following lists, which use an
`{items: [...]}` wrapper (see `social-feed-creator.md` §2) — but treat `favorites` itself as "which
ids the account has favourited," not as "the My Collection list payload": the actual list contents
(title, cover, duration, etc.) come from `users/{id}/favorites/*` (below), keyed by these ids.
Note **users are favouritable too**. Mixes use a distinct write endpoint family:
`favorites/mixes/add`, `favorites/mixes/remove`.

**API**: `users/{id}/favorites/{albums,artists,tracks,videos,playlists}` with
`limit/offset/order/orderDirection`, plus cursor pagination on some lists
(`ref:python-tidal/tidalapi/user.py`).

**A seventh collection type the six-list inventory omits: Save for Later.** The v2 spec has
`/userCollectionSaveForLaters/{id}` (`{numberOfItems, lastModifiedAt}` + an `items` relationship),
and `/userCollectionFolders` generalises the folder concept past playlists. No OSS-client
implementation yet.

## 3. Artist page

Structure confirmed against the API by two independent clients — **the Videos row specifically
should be attributed to TidaLuna/python-tidal, not High Tide, which has no Videos carousel**:

- Header: artist picture, name, Play, Shuffle, Follow/Unfollow, Share, artist radio button.
- Top Tracks (`artists/{id}/toptracks`), Albums (`artists/{id}/albums`), EP & Singles
  (`filter=EPSANDSINGLES`), Appears On (`filter=COMPILATIONS`/"other"), Similar Artists
  (`artists/{id}/similar`).
- Bio (`artists/{id}/bio`) — with source attribution, e.g. "TiVo"
  (`ArtistBio.source`, `ref:TidaLuna/.../store/content/Artist.ts`); desktop shows it in
  `modal/SHOW_ARTIST_BIO`. **python-tidal's `get_bio()` returns only `json['text']` and discards the
  `source` field** — read the raw response if you want attribution.
- Videos (`artists/{id}/videos`, `ref:python-tidal/tidalapi/artist.py`) — cite this or TidaLuna, not
  High Tide's `artist_page.py`, which builds Top Tracks/Albums/EP & Singles/Appears On/Similar
  Artists/bio/radio/follow/share carousels but has no Videos carousel.
- Artist Mix / Artist Radio (`artists/{id}/mix`, `artists/{id}/radio`).

The client also models artist *roles* and *contributions*: `ArtistRoleCategory = "Artist" |
"Songwriter" | "Performer" | "Producer" | "Production team" | "Engineer" | "Misc"`, with
`content.artistContributions` and `content/LOAD_DYNAMIC_CONTRIBUTOR_PAGE` — a credits-driven
"contributor" page distinct from the artist page. Sharing a contributor is its own context-menu
type (`CONTRIBUTOR_SHARE`).

Artist favouriting/following: `content/TOGGLE_FAVORITE_ITEMS`, `artists/FAVORITE_ARTIST_ITEM`, plus
an artist-folder concept in My Collection (`artists/SET_ARTIST_FOLDER_STATE`).

## 4. Album page

Routes: `LOADER_DATA__ALBUM`, `LOADER_DATA__ALBUM_CREDITS`, `LOADER_DATA__ALBUM_TRACK_MIX`.

```
Album { id, title, cover, vibrantColor, videoCover,
        duration, streamStartDate, releaseDate, releaseYear,
        numberOfTracks, numberOfVideos, numberOfVolumes,
        copyright, type:"ALBUM", version, url, explicit, upc, popularity,
        audioQuality, audioModes[], mediaMetadata.tags[],
        upload, artist, artists[], genre, recordLabel,
        + StreamingFlags }
```
`AlbumCredit { type, contributors: [{name, id}] }` and `AlbumReview { text, source }` (e.g. source
"TiVo") — **both live in `ref:TidaLuna/.../store/content/index.ts:66-79`, not `Album.ts`.**

Album page shows: cover (and animated `videoCover`), a dominant `vibrantColor` used for theming,
multi-volume track listing, duration, release date, UPC, record label, genre, copyright, explicit
flag, quality badges from `mediaMetadata.tags`, credits grouped by role, an editorial review, and
similar albums. Actions: play, shuffle, favourite, add to playlist, create playlist from album,
share, album radio/track mix. Credits expansion is tracked (`eventTracking/EXPAND_CREDITS`).

**API**: `albums/{id}`, `albums/{id}/items`, `albums/{id}/tracks`, `albums/{id}/review`,
`albums/{id}/similar`, plus `pages/album?albumId=` and
`content/LOAD_ALL_ALBUM_MEDIA_ITEMS_WITH_CREDITS` for per-track credits
(`ref:python-tidal/tidalapi/album.py`, `ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts`).

## 5. Track / media item, lyrics, credits

Model (`ref:TidaLuna/.../store/content/{BaseMediaItem,Track}.ts`): `id, title, version, duration,
trackNumber, volumeNumber, streamStartDate, releaseDate, explicit, popularity, artist, artists[],
album, mixes, replayGain, peak, audioQuality, audioModes[], mediaMetadata.tags[], bpm, isrc,
copyright, upload:false, url` + `StreamingFlags`.

Per-track actions (`MEDIA_ITEM`/`MULTI_MEDIA_ITEM` context menus): Play now / Play next / Add to
queue, Add to playlist, Favourite/unfavourite, Go to album/artist, Track radio/mix, Credits, Lyrics,
Share, **Block** (`blocks/BLOCK`, `contextMenu/OPEN_BLOCK_ITEM`, `canBlock` flag — hides a
track/artist from recommendations and filters it out of the live queue, with an undo window and a
persistent blocked-items page `route/LOADER_DATA__BLOCKS` — this permanently reshapes account-wide
recommendations, treat it as a deliberate write, not an early/casual feature), Multi-select.

**Lyrics** (`ref:TidaLuna/.../store/Lyrics.ts`): `{trackId, lyricsProvider, providerCommontrackId,
providerLyricsId, lyrics, subtitles, isRightToLeft}`. `lyrics` is plain text; `subtitles` is the
timed (karaoke/synced) form; RTL is handled. Endpoint `tracks/{id}/lyrics`
(`ref:python-tidal/tidalapi`) — desktop uses
`https://desktop.tidal.com/v1/tracks/{id}/lyrics`. A synced-lyrics view is one of the highest
visible-value features per unit of effort; both High Tide and Sone ship it.

`ContentType = "track" | "video"` is the *media-item* union, not every content type in the app —
`Mix.ts` separately declares `contentType: "mix"`.

## 6. History / Recently Played

Client-side surface: `content/LOAD_RECENT_ACTIVITY`, `accumulatedPlaybackTime` state (per-product
accumulated listening time keyed by product id), and `cloudQueue/FILL_CLOUD_QUEUE_WITH_HISTORY`.

**Do not build Recently Played from a local play-history list — it will not match the real client.**
Recently Played on Home, and Home personalisation generally (Daily Discovery, New Arrivals), is fed
by **`play_log` event-batch telemetry**, not by anything the client reads back: Sone had to
implement `play_log` batches before Recently Played reflected its own playback
(`ref:sone/src-tauri/src/tidal_report/event.rs`, `ref:sone/README.md`). Full wire format is owned
by `tidal-api/references/play-logging-and-privileges.md` §1-4 — cite it rather than restating the
payload/headers here.

**Consequence, not just a missing feature**: skipping play reporting doesn't just leave Recently
Played empty in streamboat — it silently degrades the *user's own TIDAL account* (a dead Recently
Played and stale Daily Discovery/New Arrivals show up in the official app too, since they're all
fed by the same server-side signal). Tier **v1**; make it a settings toggle, defaulting on, exactly
as Sone does — see SKILL.md "Open decisions" #4 for the on-by-default-vs-opt-in policy call.

## 7. Videos

Videos are first-class media items alongside tracks, not a bolt-on: `ContentType = "track" |
"video"` (§5), a dedicated `Video` model, a `VIDEO_MIX` mix type (`browse-pages-screens.md` §4), a
`VIDEOS` search category (`browse-pages-screens.md` §5), a `pages/videos` explore page, and a My
Collection → Videos list (`route/LOADER_DATA__MY_COLLECTION_VIDEOS`).

`Video { id, title, quality: "MP4_1080P", imageId, vibrantColor, adsUrl, adsPrePaywallOnly, ... }`
(`ref:TidaLuna/.../store/content/Video.ts`).

**Stream URLs, and a gap in python-tidal**: the plain video URL comes from
`videos/{id}/urlpostpaywall` (`ref:python-tidal/tidalapi/media.py:976`, returns a bare m3u8 URL).
**python-tidal has no video equivalent of `playbackinfopostpaywall`** — no manifest/DRM-status
response for videos, only the direct-URL shortcut. For the playbackinfo-shaped variant, cite
`ref:sone/src-tauri/src/tidal_api.rs:3816` (`get_video_stream_url`, calls
`videos/{id}/playbackinfopostpaywall`) or `ref:tidalswift/TidalSwiftLib/Sources/TidalSwiftLib/Session/ContentUrls.swift:32`
(`videoUrl`, calls `videos/{id}/playbackinfo`, base64-decodes a manifest same-shaped as the audio
one). Budget for implementing this endpoint directly — no OSS client's high-level library wraps it.

**Separate pipeline, real cost**: Sone plays video through a native-HLS-first, `hls.js`-fallback
path (`ref:sone/src/components/VideoPlayer.tsx`) entirely separate from its bit-perfect audio signal
chain — its own README says only that video audio "does not use the bit-perfect lossless signal
path." Building video means a second playback pipeline end to end (manifest → HLS → decode →
render), not a variant of the audio path.

**[uncertain, possibly stale]**: the commonly repeated "650,000+ videos" catalogue-size figure and
"AAC ~320 kbps" video-audio-codec figure are sourced only to a whathifi review of unstated vintage
and were not re-verified for 2026 — do not present either as a solid 2026 fact.

Tier: **later** — a separate HLS pipeline for a scope the owner has not committed to (SKILL.md
"Open decisions" #2). This section gives the facts needed to cost that decision; the decision
itself is the owner's.
