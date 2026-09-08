# Browse, pages, and the screen inventory

Table of contents:
1. Home — two schemas, tabs, pagination
2. Module/section type vocabulary and category titles
3. Explore
4. Mixes and radio — MixType (open-ended), actions, routes, endpoints
5. Search
6. The 29-route-loader table
7. Sidebar nav (top-level only — not the full inventory)
8. Now Playing, fullscreen, mini-player

## 1. Home — two schemas, tabs, pagination

Two backend schemas are both live in 2026; do not assume there is one flat module list.

**Legacy (v1)**: `GET pages/home` → `{title, rows: [{modules: [{type, title, pagedList, ...}]}]}`.
Also surfaced as `tidal:home` / `tidal:for_you` in `ref:mopidy-tidal/mopidy_tidal/library.py`.

**Current (v2)**: `GET https://api.tidal.com/v2/home/feed/{slug}` with
`deviceType=BROWSER&locale=<locale>&platform=WEB` →
`{header: {vibes: {items: [{name, type}]}}, items: [{type, title, items, viewAll, ...}], cursor}`.

- `header.vibes.items[]` **is the Home tab bar** — observed `type` values `STATIC`, `EDITORIAL`,
  `UPLOADS` — and `slug` is the lowercased `type`, so the real endpoints are `home/feed/static`,
  `home/feed/editorial`, `home/feed/uploads`.
- Sections page with a top-level `cursor`, passed back as `?cursor=`. Each section carries
  `hasMore` and an `apiPath` used for "View All" expansion (client-side:
  `homepage/LOAD_AND_ENQUEUE_ALL_VIEW_ALL_TRACKS`).
- `ref:python-tidal/tidalapi/session.py:1041-1060` (`Session.home(use_legacy_endpoint)`): this is a
  plain `if`/`else` on the caller-supplied flag, **not a fallback** —
  `use_legacy_endpoint=False` (default) calls `home/feed/static` with
  `deviceType=BROWSER&locale&platform=WEB`; `use_legacy_endpoint=True` calls `pages/home` instead.
  The caller chooses; there is no automatic v1 degradation, and python-tidal never requests the
  `editorial`/`uploads` slugs (§ above), only `static`. The generic page parser it calls
  (`ref:python-tidal/tidalapi/page.py:129-135`) branches on `json_obj.get("rows")` vs `items` to
  handle both shapes, but that branch is shared code, not home-specific fallback logic — streamboat
  must implement its own tab enumeration and any v2→v1 fallback itself.
  `ref:sone/src-tauri/src/tidal_api.rs:1240-1268, 4161-4210` parses both shapes independently.
- **Do not confuse `home/feed/static` with the social Feed** — it is the v2 Home page endpoint. The
  social Feed (`sidebar-feed`, `feed/LOAD_FEED`) is a separate surface — see
  `social-feed-creator.md` §1.
- **`pages/for_you` still exists separately** as a v1 endpoint
  (`ref:python-tidal/tidalapi/session.py`) — a different category set from the v2 Home feed.

The client loads Home with `route/LOADER_DATA__HOME` + `content/LOAD_DYNAMIC_PAGE`, tracks module
impressions (`homepage/TRACK_HOME_PAGE_VIEW`, `homepage/TRACK_ITEM`, `eventTracking/CLICK_MODULE`,
`eventTracking/DISPLAY_PAGE`, `eventTracking/SCROLL_PAGE`), supports inline play
(`homepage/PLAY_SINGLE_TRACK`).

## 2. Module/section type vocabulary and category titles

**V1 module types** observed in the wild: `ALBUM_LIST`, `ARTIST_LIST`, `PLAYLIST_LIST`,
`TRACK_LIST`, `MIX_LIST`, `ARTICLE_LIST`, `MIXED_TYPES_LIST`, `PAGE_LINKS`, `PAGE_LINKS_CLOUD`,
`FEATURED_PROMOTIONS`, `MULTIPLE_TOP_PROMOTIONS`, `HIGHLIGHT_MODULE`, `TEXT_BLOCK`, `ALBUM_HEADER`,
`ALBUM_ITEMS`, `MIX_HEADER`.

**V2 section types**: `SHORTCUT_LIST`, `HORIZONTAL_LIST`, `HORIZONTAL_LIST_WITH_CONTEXT`,
`TRACK_LIST`, `MIXED_LIST`, `GRID`. python-tidal types the v2 set as
`PageCategoriesV2 = TrackList | ShortcutList | HorizontalList | HorizontalListWithContext`.

**A module/section renderer needs a graceful unknown-type fallback** — TIDAL ships new types
without warning, and this dump is already known to be behind (crossfade proves it). Log unknown
shapes, never hard-fail a page because one module changed.

**Concrete category titles** (doctest-verified against a live account by python-tidal,
`ref:python-tidal/docs/pages.rst`): For You, Recently Played, Suggested New Tracks, Suggested New
Albums, Mixes For You, Radio Stations for You, Your History, Trending Playlists, Popular Playlists,
TIDAL Rising, The Charts, Popular Albums, Podcasts (see the note below — [uncertain] whether this
survives past the 24 Jul 2024 podcast removal, likely a stale fixture), Producers & Songwriters,
New Releases For You, Featured, Genres, **Moods, Activities & Events** (verbatim
comma-separated title, not "Moods/Activities"), Suggested Albums for You,
Suggested Artists for You, TIDAL Originals, New Music Videos, Album Experiences, New Video
Playlists, Classics Video Playlists, Movies, Hits Video Playlists.

Add, from unfetched support-article summaries only (**[uncertain]**): **My Daily Discovery** (10
tracks/day) and **My New Arrivals** (30 tracks, Fridays); Upload Spotlight promotions (since Nov
2025, [verified-web]).

## 3. Explore

`route/LOADER_DATA__VIEW` / dynamic pages. Backing endpoints: `pages/explore`, `pages/moods`,
`pages/genre_page`, `pages/genre_page_local`, `pages/hires`, `pages/videos`
(`ref:python-tidal/tidalapi/page.py`). mopidy-tidal exposes these as browse roots: `tidal:explore`,
`tidal:moods`, `tidal:genres`, `tidal:mixes`, `tidal:hires`.

Content: genres, Moods & Activities, **TIDAL Rising** (emerging/unsigned artists — has its own
dedicated `pages/rising` endpoint, `ref:sone/src-tauri/src/tidal_api.rs:4386-4406`, not merely "an
Explore module"), **The Charts** (a confirmed page category, reachable as a `PageLink` rather than
a dedicated endpoint), new releases, popular artists, editorial articles/interviews. The client
models editorial articles as a first-class content type (`content.articles`, `content.articleLists`,
`Article.ts`) — TIDAL Magazine content is inlined into the app. **Tier: later.** There is no
dedicated article endpoint — articles arrive as an `ARTICLE_LIST` v1 module type (§2) inside
whatever page renders them (typically Explore/Home), and python-tidal has no article model at all,
so this is client-side rendering of an existing module type, not new API surface. Low priority:
no playback value, safe to defer behind the module renderer already required for Explore/Home.

## 4. Mixes and radio — MixType (open-ended), actions, routes, endpoints

Desktop client's own `MixType` model — **a subset, not the complete set**:
`MixType = "VIDEO_MIX" | "TRACK_MIX" | "ALBUM_MIX" | "ARTIST_MIX"`
(`ref:TidaLuna/plugins/lib/src/redux/types/store/content/Mix.ts`).

The live API returns more: Sone's `TidalMix.mix_type` doc comment also lists
`HISTORY_ALLTIME_MIX`, `HISTORY_MONTHLY_MIX`, `HISTORY_YEARLY_MIX`
(`ref:sone/src-tauri/src/tidal_api.rs:841-847`) — this is what backs the "Your History" Home
category. **`NEW_HISTORY_MIX` is a separate thing** — a `Feed` `activityType`, not a `mix_type`
value (`ref:sone/src-tauri/src/tidal_api.rs:7076`, its Feed-activity-flattening test); don't file it
under `MixType`. The official v2 spec models the personalised families as separate resources
(`/userDailyMixes`, `/userDiscoveryMixes`, `/userNewReleaseMixes`, `/userOfflineMixes`) reachable
from `/userRecommendations/{id}/relationships/{myMixes,discoveryMixes,newArrivalMixes,offlineMixes}`,
and explicitly documents that new `MixType` values can appear at any time. Don't hard-code a closed
enum for this.

**Client actions** (`ref:TidaLuna/plugins/lib/src/redux/types/actions/actionTypes.ts`):
`mix/PLAY_MIX`, `mix/LOAD_MIXES_SUCCESS`, `mix/LOAD_TRACK_LIST_FOR_MIX_ID`,
`mix/LOAD_TRACK_MIX_ID` (+ `_SUCCESS`/`_FAIL`), `mix/LOAD_ALL_MIX_MEDIA_ITEMS_SUCCESS`,
`mix/ADD_MIX_TO_PLAYLIST`, `mix/SHOW_CREATE_PLAYLIST_FROM_MIX_DIALOG`,
`content/LOAD_DYNAMIC_MIX_PAGE`. Routes (§6): `LOADER_DATA__MIX`, `LOADER_DATA__ARTIST_MIX`,
`LOADER_DATA__ALBUM_TRACK_MIX`. Favouriting a mix uses its own endpoint family, distinct from
every other favourite type: `favorites/mixes/add`, `favorites/mixes/remove` (pitfall 8's
"favorites is bare arrays" shape still applies — only the write endpoint differs).

**API (v1, unofficial)**: `pages/mix?mixId=` (per-mix page, note the required query param — don't
drop it), `pages/my_collection_my_mixes` (the My Collection → Mixes & Radio list), plus the
per-entity radio/mix endpoints `tracks/{id}/mix`, `tracks/{id}/radio`, `artists/{id}/mix`,
`artists/{id}/radio` (`ref:python-tidal/tidalapi/{mix,artist,media}.py`) — the artist versions are
also covered from the artist-page angle in `library-playlists-collections.md` §3.

An implementer building the Mix page needs all three of: this section (MixType, actions, routes,
endpoints), `library-playlists-collections.md` §3 (artist mix/radio in context), and the
`feature-matrix.md` "Mixes: My Mix N, Daily Discovery, New Arrivals, Video Mix, history mixes" row
(tier **v1**) for the priority call.

## 5. Search

Client state shape (`ref:TidaLuna/plugins/lib/src/redux/types/store/index.ts`):

```
Search {
  searchPhrase, queryId, isLoading, topHit,
  recentSearches: RecentSearch[],   // kind: albums|artists|genres|playlists|search|tracks|userProfiles|videos
  searchResultFilterOrder: ["TRACKS","VIDEOS","ARTISTS","ALBUMS","PLAYLISTS","UPLOADS","USERPROFILES"],
  searchSession: { uuid, lastUpdated, triggerNewSession },
  suggestionUuid, didYouMeanByQueryMap, genres
}
```

Seven result categories including Uploads and User Profiles; "did you mean" spelling correction;
query suggestions; recent searches with clear; a search popover; a search-session UUID for
analytics; result-consumption telemetry. A separate track/album/artist picker search backs the
Picks feature.

**API (v1, unofficial)**: `GET search?query=&limit=&offset=&types=ARTISTS,ALBUMS,TRACKS,VIDEOS,PLAYLISTS`
(exact param dict at `ref:python-tidal/tidalapi/session.py:771-828`), returning per-type arrays plus
`topHit`. Limit ≤300 total results ("there aren't more than 300 items available in a search").
**Uploads and userProfiles result types are not implemented by python-tidal** — direct API work
needed.

**Recent searches and suggestions are server-side and cross-device, not local** — the v2 spec
documents `/searchResults` (`{query, trackingId, didYouMean}`, relationships `topHits`, `albums`,
`artists`, `playlists`, `tracks`, `videos` — `trackingId` is the server counterpart of the client's
own `searchSession.uuid`), `/searchSuggestions` (relationships `directHits`, `history`), and
`/searchHistoryEntries/{id}`. No OSS client has exercised these yet, but the work is "call a
documented endpoint," not "reverse-engineer an undocumented one."

## 6. The 29-route-loader table

The client's route loaders name **29 distinct screens, not 28** — verified by grepping
`actionTypes.ts` for `route/LOADER_DATA__*` and counting distinct base names (stripping any
`--SUCCESS`/`--FAIL` suffix). **28 of the 29 carry both a `--SUCCESS` and a `--FAIL` variant;
`LOGIN_AUTH` is the one that has neither** — only the bare `route/LOADER_DATA__LOGIN_AUTH` action
exists. Say "29 route loaders (28 with success/fail variants)," not a flat "28 screens":

```
ALBUM, ALBUM_CREDITS, ALBUM_TRACK_MIX, ARTIST, ARTIST_MIX, ARTIST_PICKER, BLOCKS,
FACEBOOK, FOLDER, HOME, LASTFM, LOGIN, LOGIN_AUTH, MIX,
MY_COLLECTION_ALBUMS, MY_COLLECTION_ARTISTS, MY_COLLECTION_MIXES, MY_COLLECTION_PLAYLISTS,
MY_COLLECTION_TRACKS, MY_COLLECTION_VIDEOS,
PLAYLIST, SEARCH, SETTINGS, SNAPCHAT, TIKTOK, TRACK, USER, VIDEO, VIEW
```

**Four are not reachable from the sidebar at all** and are easy to miss if you derive the IA from a
sidebar screenshot instead of this table:
- **TRACK** — a standalone track page.
- **VIDEO** — a standalone video page.
- **FOLDER** — the playlist-folder screen is a real route, not just a sidebar disclosure triangle.
- **USER** — another person's public profile page.

`VIEW` is the generic dynamic-page route serving Explore, genre, mood, and editorial pages.

Note: there is no `LOADER_DATA__FEED` or `LOADER_DATA__UPLOADS` in this dump even though both have
sidebar entries — a dating question about the TidaLuna snapshot, consistent with the general
version caveat (both features have their own action namespaces, `feed/*` and `creatorContent/*`,
just not a dedicated route-loader action in this build).

## 7. Sidebar nav (top-level only — not the full inventory)

From live DOM selectors (`ref:tidal-hifi/src/TidalControllers/DomController/constants.ts`):

| Nav item | Selector |
| --- | --- |
| Music (Home) | `sidebar-music` |
| Explore | `sidebar-explore` |
| Feed | `sidebar-feed` |
| Uploads | `sidebar-uploads` |
| Collection → Playlists | `sidebar-collection-playlists` |
| Collection → Albums | `sidebar-collection-albums` |
| Collection → Tracks | `sidebar-collection-tracks` |
| Collection → Videos | `sidebar-collection-videos` |
| Collection → Artists | `sidebar-collection-artists` |
| Collection → Mixes & Radio | `sidebar-collection-mixes-and-radio` |
| Collapse/expand | `sidebar-collapse` / `sidebar-expand` |

Sidebar extras: `SIDEBAR_ADD_NEW` context menu (create playlist / create folder),
`SIDEBAR_PLAYLISTS_SORT_ORDER` context menu, expandable playlist folders
(`selection/SET_OPEN_SIDEBAR_FOLDERS`), drag & drop (`selection/START_DRAG`/`END_DRAG`).

A UI redesign rolled out during 2025–2026: tidal-hifi 6.3.1 added runtime UI-version autodetection,
with `OLD_UI_OVERRIDES` vs `NEW_UI_OVERRIDES` selector tables (e.g. old `footer-track-title` vs new
`track-info`). streamboat doesn't touch the DOM, so this doesn't matter mechanically — but it does
mean streamboat is free to pick its own information architecture rather than copying either TIDAL
UI generation; that's an owner decision (see SKILL.md "Open decisions" #10), not an accident of
which screenshots got used as a reference.

## 8. Now Playing, fullscreen, mini-player

Actions (`ref:TidaLuna/plugins/lib/src/redux/types/actions/actionTypes.ts` `view/*`): `ENTER_NOWPLAYING`,
`ENTERED_NOWPLAYING`, `EXIT_NOWPLAYING`, `EXITED_NOWPLAYING`, `REQUEST_FULLSCREEN`,
`EXIT_FULLSCREEN`, `FULLSCREEN_ALLOWED`, `FULLSCREEN_DENIED`, `ENTER_NATIVE_FULLSCREEN`,
`LEAVE_NATIVE_FULLSCREEN`, `MINIMIZE`, `TOGGLE_VISIBILITY`, `LEAVE_PORTAL`. State:
`isNowPlaying`, `showNowPlaying`, `isFullscreen`, `isNativeFullscreen`, `isPortal`.

The Now Playing screen carries the lyrics and credits panels (see
`library-playlists-collections.md` §5) — toggling it has a dedicated control
(`player-details-toggle-now-playing` in the new UI; `footer-track-title`/expand affordance in the
old one). Official desktop shortcut: `Ctrl+P` (mirrored by tidal-hifi as `expandNowPlaying`, see
`remote-playback-connect-controls.md` §6). Tier: **v1** — high-visibility screen, cheap once the
underlying lyrics/credits data exists.

**There is no separate always-on-top mini-player window in the official desktop client's action
namespace** — `view/MINIMIZE` is plain window minimisation, not a floating mini-player. Sone and
sone-windows *add* a floating mini-player as a differentiator, which is itself evidence the
official app lacks one. Tier: **later (differentiator)** — cheap, popular with the OSS clients, no
schema risk, but not a parity gap to close.
