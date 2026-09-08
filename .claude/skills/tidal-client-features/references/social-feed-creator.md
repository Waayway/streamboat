# Feed, profiles, Uploads, and the 2025–2026 social/creator layer

Table of contents:
1. Feed vs Home — do not conflate
2. Public profiles
3. Picks / track prompts
4. Onboarding steps (TIDAL's own headline-feature checklist)
5. TIDAL Upload
6. The 2025–2026 social/creator API layer (comments, reactions, appreciations, claims, purchases)

## 1. Feed vs Home — do not conflate

These are different endpoints with different content. `feed/LOAD_FEED`, `feed/CHECK_FEED_UPDATES`,
`feed.hasNewFeedItems`, `view/SHOW_FEED_SIDEBAR`/`HIDE_FEED_SIDEBAR`, and a `FEED` play-queue source
type — the Feed is both a page and a right-hand sidebar you can play from.

`home/feed/static` is the **v2 Home page** endpoint (see `browse-pages-screens.md` §1), **not** the
social Feed, despite the name similarity. For the real Feed API, use Sone's implementation: it
fetches the Feed separately and parses `activities[]` / `totalNotSeenActivities` rows
(`flatten_feed_activity`, `parse_feed_body`, `ref:sone/src-tauri/src/tidal_api.rs:680-770,
4161-4210`).

## 2. Public profiles

```
UserProfile { userId, name, picture, color: [c1,c2,c3], numberOfFollowers, numberOfFollows,
              followers, followingUsers, followingArtists, publicPlaylists }
```

**Each of `followers`/`followingUsers`/`followingArtists`/`publicPlaylists` is an object wrapping
an `items` array** (e.g. `followers: {items: Following[]}`), **not a bare array** — this is the
opposite shape from My Collection's own `favorites` object, which *is* bare arrays (see
`library-playlists-collections.md` §2). `Following` carries `followType: "USER" | "ARTIST"`,
`blocked`, `imFollowing`, and a `trn:user:{id}` resource name.

Actions: `user/UPDATE_PROFILE_NAME`, `user/UPDATE_PROFILE_PICTURE`,
`user/UPDATE_PROFILE_SOCIAL_HANDLES`, `user/DELETE_PROFILE_PICTURE_BUTTON_CLICKED`, picture sources
from Facebook/Snapchat/TikTok (`user/{FACEBOOK,SNAPCHAT,TIKTOK}_PICTURE_BUTTON_CLICKED`, routes
`LOADER_DATA__{FACEBOOK,SNAPCHAT,TIKTOK}`).

## 3. Picks / track prompts

`trackPrompts/*` namespace: `SET_TRACK_FOR_PROMPT`, `REMOVE_TRACK_FOR_PROMPT`, `TOGGLE_PROMPTS`,
`GENERATE_SHARE_IMAGES`, `STORE_GENERATED_SHARE_IMAGES`, `PLAY_MY_PICKS_ITEM`, with
`ALBUM_PROMPT`/`ARTIST_PROMPT`/`TRACK_PROMPT` context menus. This is the "My Picks" profile feature
(pinned favourites answering prompts, shareable as generated images). The "public and on by default,
with an off switch in Settings" characterisation rests only on an unfetched support-article
summary — **[verified-web, unfetched]**, not source-verified.

## 4. Onboarding steps (TIDAL's own headline-feature checklist)

`ref:TidaLuna/.../store/User.ts` — this list doubles as TIDAL's own view of what's headline enough
to onboard new users on: `ADD_TO_FAVORITES`, `ADD_TO_OFFLINE`, `ADD_TO_PLAYLIST`, `ALBUM_INFO`,
`ARTIST_CREDITS`, `ARTIST_PICKER`, `BLOCK`, `BOTTOM_NAVIGATION_INTRO`, `CAST`, `CAST2`,
`CONTRIBUTOR`, `DOLBY_ATMOS`, `LYRICS`, `MENU_MY_MUSIC`, `MENU_OFFLINE_CONTENT`, `OPTIONS_MENU`,
`PLAY_QUEUE`, `PLAY_QUEUE_BUTTON`, `PLAY_QUEUE_SUGGESTIONS`, `RELOCATION_SETTINGS`, `SETTINGS`,
`SPRINT_REDESIGN_UPDATE`, `USER_PROFILE_ONBOARDED`, `WEB_3.0.0_UPDATE`.

Also: `artistPicker/*` + `route/LOADER_DATA__ARTIST_PICKER` — the taste-onboarding flow for new
accounts.

## 5. TIDAL Upload

Launched **November 2025**. Users upload their own audio; client namespace `creatorContent/*`:
`START_TRACK_UPLOAD`, `SET_TRACK_UPLOAD_PROGRESS`, `SET_TRACK_UPLOAD_ERROR`,
`FINISH_TRACK_UPLOAD`, `SET_TRACK_ITEM_ID`, `START_ARTWORK_UPLOAD`, `FINISH_ARTWORK_UPLOAD`, plus
context menus `OPEN_CREATOR_CONTENT_OWNED_ITEM`/`OPEN_CREATOR_CONTENT_RECEIVED_ITEM`. Uploaded
items are marked by an `upload: boolean` flag on Album (and `upload: false` on catalogue tracks);
`UPLOADS` is a search result category. Limits: ≤5 GB/track, ≤200 tracks, beta, no royalties,
available in US/UK/EEA/Switzerland/Canada, 18+. TIDAL ran an "Upload Headliners" $100k contest and
a weekly editorial Spotlight.

Cover-art note: `getCoverURL` in tidal-hifi returns the uploaded URL verbatim when it's already
absolute, because uploaded content doesn't use the
`resources.tidal.com/images/<uuid-with-slashes>/<size>x<size>.jpg` convention
(`ref:tidal-hifi/src/features/tidal/url.ts`).

## 6. The 2025–2026 social/creator API layer

This layer is entirely absent from the desktop Redux dump and easy to miss if you only read that
dump. It's all in the v2 OpenAPI spec (`ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json`;
example timestamps in the schemas are 2025-09/2025-11 — i.e. it shipped around the TIDAL Upload
launch above). Treat every item below as needing an explicit owner scoping decision before
building, not a default inclusion — see SKILL.md "Open decisions" #1.

- **Comments**: `/comments` (GET/POST/PATCH/DELETE), attributes `{message (1..2000 chars),
  createdAt, lastModifiedAt, likeCount, replyCount, moderationStatus:
  NOT_MODERATED|FLAGGED|TAKEN_DOWN|OK|ERROR, startTime, endTime}`. `startTime`/`endTime` are
  ISO-8601 durations (e.g. `PT1M30S`) — **SoundCloud-style time-anchored comments on a track**.
  Subjects restricted to `albums` and `tracks`; replies hang off `parentComment`; relationship
  `ownerProfiles`.
- **Reactions and appreciations**: `/reactions` (with `CurrentUserReaction`) and `/appreciations`
  (POST-only, `appreciatedItem` type enum = `artists`) — separate gesture types from comments.
- **Creator/monetisation**: `/artistClaims`, `/manualArtistClaims`, `/artistClaimStatuses`,
  `/contentClaims`, `/trackStatistics/{id}`, `/albumStatistics/{id}`, `/purchases`,
  `/priceConfigurations`, `/stripeConnections`, `/stripeDashboardLinks`, `/squareConnections`,
  `/trackSourceFiles`.
- **Reporting**: `/userReports` for reporting content.

Each carries real moderation/privacy implications if streamboat ever renders it — that's why these
need an explicit out-of-scope row with a reason if left out, not silent omission. None has an
OSS-client implementation to point to yet.
