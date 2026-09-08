# What the native TIDAL clients do — feature inventory and product spec for streamboat

Research date: 2026-09-07. This document is the product specification target for streamboat: an
exhaustive inventory of what TIDAL's own clients do, plus a proposed priority tier for each feature
and a note on whether the feature is reachable through the unofficial API that High Tide, Sone and
python-tidal use.

**How to read the evidence markers.** Every claim below is tagged:

- **[verified-source]** — read directly out of a reference checkout in
  `/tmp/.../ref/<project>` and cited as `ref:<project>/<path>`. The strongest evidence, because
  TidaLuna's type definitions are extracted from the running official TIDAL desktop client, and
  tidal-hifi's DOM selectors are extracted from the running official web player.
- **[verified-web]** — from a documentation/press source with a URL.
- **[inferred]** — a reasonable deduction from the above, not directly stated anywhere.
- **[uncertain]** — could not confirm; the report says what is missing and why.

Two network constraints shaped the method: `support.tidal.com`, `tidal.com` and
`developer.tidal.com` are blocked to direct fetch from this environment, so official TIDAL support
pages are cited via search-engine summaries of those pages rather than full-page reads. Where a
claim rests only on a low-quality SEO aggregator it is marked **[uncertain]** explicitly.

**Correction to the above (post fact-check): the official API spec is not actually unreachable.**
`developer.tidal.com` itself is blocked, but the full OpenAPI document it publishes is vendored
verbatim in two reference checkouts and was under-used in the first pass of this report:
`ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json` (identical copy at
`ref:tidal-sdk-android/tidalapi/bin/tidal-api.json`) — **TIDAL API 1.10.104**, server
`https://openapi.tidal.com/v2`, 256 paths, 993 schemas. This is a first-party, machine-readable
spec, not a web summary, and it is now cited directly throughout this report (particularly §4.8,
§4.13, §5.4, §9) and in `references/openapi-v2-catalogue.md`. Treat it as **[verified-source]**
evidence, one tier above `[verified-web]`.

**Standing caveat on every absence-based claim.** A claim of the shape "the client dump has no
action/field for X, therefore TIDAL does not do X" is only as current as the one snapshot it was
read from: `ref:TidaLuna` at commit `d8cd6bc29b5edc30dd8ec28861b88d3519560126`
(`package.json` version `1.16.6-beta`), cloned 2026-09-02. This is not a hypothetical risk — it is
the proven cause of this report's single worst error: the original draft concluded crossfade does
not exist because no `crossfade` action appears in that dump, when TIDAL had in fact already
re-announced crossfade for iOS and Web (see §5.2 and the Notable changes table). Read every
"no trace of X in the dump" sentence below as "no trace of X in TidaLuna 1.16.6-beta
(2026-09-02)", not as "X does not exist in TIDAL."

**Coverage boundary.** Of the 21 checkouts under `ref/`, this report draws on TidaLuna,
tidal-hifi, python-tidal, tidal-sdk-web, tidal-sdk-android, sone, sone-windows, high-tide,
mopidy-tidal, strawberry, tidal-connect and tidalt. `tidal-api-docs` and
`tidal-fokka-engineering-` were not consulted in either research pass; `tidalgo`, `tidalswift`,
`libopentidal`, `tidalrs`, `tidal-cli`, `dotnet-tidal-usdk` and `tidal-sdk-ios` are referenced only
incidentally. Re-verifiers should start with the two unread checkouts before re-opening web
searches.

**Related documents.** This file is the feature/product inventory. For depth on adjacent topics,
go to the sibling report rather than duplicating it here: wire-format detail for auth, playback
manifests, encryption, the quality cascade and image URLs lives in `tidal-api.md`; DSP/output
engineering lives in `audio-pipeline.md`; TIDAL Connect and the headless control-surface design
live in `headless-connect.md`; the stack decision lives in `tech-stack.md`; other OSS clients'
architecture choices live in `oss-landscape.md`. §6 of this report keeps only the feature-level
summary and points to `tidal-api.md` for the wire format.

---

## Summary

1. **There is no official TIDAL desktop app for Linux.** TIDAL ships Electron desktop apps for
   Windows and macOS only; Linux users run the web player or a wrapper. TidaLuna's install
   instructions name `%localappdata%\TIDAL\app-x.xx.x\resources` (Windows),
   `/Applications/TIDAL.app/Contents/Resources` (macOS) and, for Linux,
   `/opt/tidal-hifi/resources` — i.e. the third-party tidal-hifi wrapper, not a TIDAL binary
   (ref:TidaLuna/README.md). [verified-source]
2. **The desktop app is a React + Redux Electron app** whose entire action namespace is enumerable.
   `ref:TidaLuna/plugins/lib/src/redux/types/actions/actionTypes.ts` is a literal
   `Object.keys(luna.core.buildActions).sort()` dump from the shipping client — **694 action types
   across 48 distinct namespaces** (verified by `grep -oP '^\t"[^"]+"' | wc -l` = 694 and
   `grep -o '"[a-zA-Z0-9]*/' | sort -u | wc -l` = 48), taken from TidaLuna `v1.16.6-beta`
   (commit `d8cd6bc`, cloned 2026-09-02). This is the single most complete feature inventory
   available, but every absence claim built on it is dated to that one build — see the version
   caveat above. [verified-source]
3. **Quality tiers as of 2026 are named Low / High / Max** in the UI, mapping to internal
   `LOW` (AAC ≤320 kbps), `HIGH`/`LOSSLESS` (FLAC 16-bit/44.1 kHz) and `HI_RES_LOSSLESS`
   (HiRes FLAC up to 24-bit/192 kHz). The legacy names Normal/High/HiFi/Master are gone.
   [verified-web + verified-source]
4. **The subscription is a single tier** (Individual / Family / Student pricing variants) since the
   March–April 2024 consolidation; HiFi vs HiFi Plus no longer exist, and every paying plan gets
   HiRes FLAC. The Free ad-supported tier was discontinued in April 2024. [verified-web]
5. **MQA and Sony 360 Reality Audio were removed from TIDAL on 24 July 2024**, announced 17 June
   2024. Dolby Atmos (EAC3-JOC / AC-4) remains. The client's type system still carries `MQA` and
   `SONY_360RA` as legacy `mediaMetadata.tags` values
   (ref:TidaLuna/plugins/lib/src/redux/types/store/content/Track.ts). [verified-web +
   verified-source]
6. **The desktop app has exclusive/bit-perfect output.** Redux carries
   `player.activeDeviceMode: "exclusive" | "shared"`, `player.availableDevices[]`,
   `player.forceVolume` and the actions `player/SET_DEVICE_MODE`, `player/SET_ACTIVE_DEVICE`,
   `player/SET_FORCE_VOLUME`, plus a `SELECT_SOUND_OUTPUT` context menu
   (ref:TidaLuna/plugins/lib/src/redux/types/store/index.ts,
   ref:TidaLuna/plugins/lib/src/redux/types/actions/ContextMenu.ts). It matches the source
   rate/depth via WASAPI exclusive mode on Windows; the macOS mechanism ("Core Audio hog mode") is
   a plausible **[inferred]** guess, not attested anywhere reachable — do not repeat it as fact.
   Force Volume is the opposite of a software-volume fallback: it pins the app's own volume at
   maximum so an external DAC/amp is the sole volume control (see §5.3). [verified-source +
   verified-web]
7. **Loudness normalization is three-state, not a boolean**:
   `settings.audioNormalization: "NONE" | "ALBUM" | "TRACK"`
   (ref:TidaLuna/plugins/lib/src/redux/types/store/index.ts). The only normalization *action* in
   the dump is `settings/TOGGLE_NORMALIZATION` — there is no `SET_NORMALIZATION`, so how the UI
   cycles the three states is not shown. The commonly repeated "-14 LUFS, on by default on mobile"
   figure is sourced only to 2019–2020 rollout articles and is marked **[uncertain, possibly
   stale]** below; streamboat only needs the ReplayGain/peak fields from playbackinfo, which are
   independently verified. [verified-source; -14 LUFS claim **[uncertain]**]
8. **Crossfade is a shipping, officially announced 2026 TIDAL feature — the opposite of what the
   first draft of this report concluded.** TIDAL Magazine's "What We're Working On. (And Why.)"
   (June 2026) states: "Crossfade is once again available on iOS and Web… go to Settings and turn
   it on," with a 0–12 second slider, corroborated independently by piunikaweb.com (23 Mar 2026).
   It is *not* present in the TidaLuna 1.16.6-beta desktop action dump, which most likely means the
   snapshot predates the desktop rollout (see the version caveat above) rather than that desktop
   lacks it. **Do not scope crossfade as an out-of-native-app differentiator — it is parity work.**
   [verified-web; desktop-client status **[uncertain]**]
9. **Offline downloads do not exist on desktop.** No `offline/` or `download/` Redux namespace
   exists in the desktop action list; `user.clients[].numberOfOfflineAlbums` and the onboarding
   steps `ADD_TO_OFFLINE` / `MENU_OFFLINE_CONTENT` are mobile-device concepts surfaced in the shared
   account model (ref:TidaLuna/plugins/lib/src/redux/types/store/User.ts). Offline is mobile-only.
   The device cap is **5 simultaneous offline devices (1 online)**, not 3 — TIDAL's own "How Many
   Devices Can I Use Simultaneously?" article states exactly this
   (support.tidal.com/hc/en-us/articles/201623252). The v2 OpenAPI spec also documents a concrete
   offline data model — `/offlineTasks` (`action: STORE|REMOVE`, `state:
   PENDING|IN_PROGRESS|FAILED|COMPLETED`) and `/installations/{id}/relationships/offlineInventory`
   — which is a candidate shape for the owner's "logged-in subscriber cache" question (§Open
   questions). [verified-source + verified-web]
10. **Collaborative playlists exist as a documented, first-class TIDAL feature — not confirmed
    absent, as the first draft claimed.** The desktop Redux dump has no `collaborat*` action
    (consistent with this being mobile/web-first), but the official v2 spec defines a full
    invite/redeem flow: `/collaborationInvites` (mint an invite `code`), `/collaborationInviteRedemptions`
    (redeem it), and `playlists/{id}/relationships/{collaborators,collaboratorProfiles}`.
    Playlist visibility is also **three-state, not binary**: `Playlists_Attributes.accessType` is
    `PUBLIC | UNLISTED | PRIVATE` in the v2 model; the legacy v1 boolean `publicPlaylist` cannot
    represent `UNLISTED`, and Sone's own v1→v2 normaliser has to fake it
    (`ref:sone/src-tauri/src/tidal_api.rs:526-533`). [verified-source]
11. **Remote playback has three transports**: `chromeCast`, `tidalConnect`, `cloudConnect`, plus a
    `remotePlaybackReceiver` role, all in
    `ref:TidaLuna/plugins/lib/src/redux/types/store/RemotePlayback.ts`. The desktop app is
    primarily a *controller*; whether it is now also a Connect *target* is **[uncertain]** — but
    the evidence is stronger than a namespace name: a user-facing
    `modal/REMOTE_PLAYBACK_RECEIVER_DISCONNECT_MODAL` only makes sense if the app can be connected
    *to*.
12. **A "cloud queue" exists and is documented, not a black box.** `cloudQueue/*` actions and
    `CloudQueue` state (`queueId`, `etag`, `headPosition`, history) are the desktop client's view
    of it; the v2 spec exposes the same mechanism as `/playQueues` /`/playQueues/{id}` with
    `PlayQueues_Attributes = {createdAt, lastModifiedAt, repeat: NONE|ONE|BATCH, shuffle:
    OFF|BATCH|ALL, shuffled}`. **The server's repeat/shuffle vocabulary does not match the desktop
    client's** (`RepeatMode {Off,All,One}` + a boolean+seed shuffle) — an implementer must map
    between the two, not assume they are the same enum. [verified-source]
13. **TIDAL Upload launched November 2025**: users upload their own tracks (≤5 GB per track, ≤200
    tracks, no royalties, beta, US/UK/EEA/CH/CA). The client has a `creatorContent/*` namespace and
    a `sidebar-uploads` nav item (ref:tidal-hifi/src/TidalControllers/DomController/constants.ts).
    Alongside it a broader 2025–2026 creator/social layer exists in the v2 API — comments (incl.
    time-anchored track comments), reactions, appreciations, artist claims, purchases — that this
    report's first draft never surfaced; see §4.13. [verified-web + verified-source]
14. **The desktop sidebar as of 2026 is: Music, Explore, Feed, Uploads**, plus a Collection group
    (Playlists, Albums, Tracks, Videos, Artists, Mixes & Radio) — read off the live DOM selectors
    `sidebar-music`, `sidebar-explore`, `sidebar-feed`, `sidebar-uploads`,
    `sidebar-collection-*` (ref:tidal-hifi/src/TidalControllers/DomController/constants.ts) and
    corroborated by the route loaders `LOADER_DATA__MY_COLLECTION_{ALBUMS,ARTISTS,MIXES,PLAYLISTS,TRACKS,VIDEOS}`.
    The client's route table actually names **28 distinct screens**, four of which (a standalone
    TRACK page, a standalone VIDEO page, the playlist FOLDER page, and another user's USER profile
    page) are not reachable from the sidebar at all — see §4.1 and `references/browse-pages-screens.md`.
    [verified-source]
15. **Playlists support folders, collaboration, AI generation and reordering**:
    `folders/CREATE_FOLDER`, `folders/RENAME_FOLDER`, `folders/MOVE_ITEMS_TO_FOLDER`,
    `folders/CREATE_AI_PLAYLIST`, `content/MOVE_PLAYLIST_MEDIA_ITEMS`,
    `userProfiles/TOGGLE_PUBLIC_PLAYLIST`. python-tidal exposes the folder-mutation API split across
    two files: creation/listing in `user.py`, rename and move in `playlist.py`
    (ref:python-tidal/tidalapi/{user,playlist}.py). Collaboration itself is confirmed in the v2 spec
    (see item 10 above), not merely inferred from an absence. [verified-source]
16. **Last.fm scrobbling is built into the official client** (`lastFm/LOGIN`,
    `lastFm/DISCONNECT`, `lastFm/REFRESH_SESSION`, and a `LOADER_DATA__LASTFM` route).
    [verified-source]
17. **Play reporting is a separate telemetry pipeline**: Sone reproduces it by POSTing `play_log`
    group events to `https://ec.tidal.com/api/event-batch`, which is what makes "Recently Played"
    update (ref:sone/src-tauri/src/tidal_report/event.rs). A client that does not do this will have
    a dead Recently Played and no recommendation feedback. [verified-source]
18. **Streaming privileges are enforced server-side**: `player/STREAMING_PRIVILEGES_REVOKED` in the
    desktop client, and a whole `streaming-privileges` module in the official Android SDK. One
    stream at a time per account; a second device kicks the first. [verified-source]
19. **Web player ceiling**: lossless in Chrome/Edge/Firefox via EME; HiRes availability is
    inconsistent and browser-dependent; no exclusive mode, no offline. tidal-hifi documents that
    Chromium resamples everything to 48 kHz unless launched with
    `--audio-output-sample-rate=192000` (ref:tidal-hifi/docs/audio-quality.md). [verified-source +
    verified-web]
20. **Dolby Atmos is not available on desktop** (iOS/Android/TV/soundbar/car only), even though the
    client carries `modal/SHOW_DOLBY_ATMOS` and a `DOLBY_ATMOS` onboarding step — most likely an
    upsell/explainer modal. Confidence on the desktop-absence half raised to ~0.85: both the
    support-article summary and the single-action client evidence agree. [verified-web,
    desktop-absence **[uncertain]**]
21. **The whole browsable surface is dynamic, server-driven pages — but there are two incompatible
    schemas, tabs, and cursor pagination, not one flat module list.** Legacy: `pages/home`,
    `pages/for_you`, `pages/explore`, `pages/moods`, `pages/genre_page`, `pages/hires`,
    `pages/videos`, `pages/album`, `pages/artist`, `pages/mix`, `pages/my_collection_my_mixes`
    (ref:python-tidal/tidalapi/page.py, ref:mopidy-tidal/mopidy_tidal/library.py). Current: `GET
    https://api.tidal.com/v2/home/feed/{slug}` where `slug` is a lowercased Home tab name
    (`static`/`editorial`/`uploads`, from `header.vibes.items[]`), paginated with an opaque
    top-level `cursor`. Both schemas are live in 2026; python-tidal's `Session.home()` hits the v2
    endpoint and falls back to the legacy one. Module/section type vocabulary (`ALBUM_LIST`,
    `SHORTCUT_LIST`, `HORIZONTAL_LIST_WITH_CONTEXT`, etc.) and concrete category titles are
    catalogued in `references/browse-pages-screens.md`. Any client that wants to look like TIDAL
    renders these module lists — defensively, with an unknown-module fallback — rather than
    hand-coding a home screen. [verified-source]

---

## Findings

### 1. Platform matrix — where the native client exists

| Platform | Official client | Technology | Notes |
| --- | --- | --- | --- |
| Windows | Yes — TIDAL desktop | Electron (React + Redux) | Also a Windows Store build; TidaLuna explicitly does not support the Store version (ref:TidaLuna/README.md) |
| macOS | Yes — TIDAL desktop | Electron | `/Applications/TIDAL.app/Contents/Resources/app.asar`; code-signing required after modification (ref:TidaLuna/README.md) |
| Linux desktop | **No** | — | Users run listen.tidal.com or wrappers (tidal-hifi, Sone, High Tide). tidal-hifi exists precisely because "Linux support [was] lacking" (ref:tidal-hifi/README.md) |
| Web | Yes — listen.tidal.com | React + Redux + EME/Widevine/FairPlay | Same Redux store shape as desktop; tidal-hifi drives it via `data-test` selectors |
| iOS / iPadOS | Yes | Native (AVPlayer, FairPlay HLS) | ref:tidal-sdk-ios shows the shape: `AVQueuePlayer`, `fp.fa.tidal.com/license` |
| Android | Yes | Native (ExoPlayer/media3, Widevine) | ref:tidal-sdk-android; formats HEAACV1/AACLC/FLAC/FLAC_HIRES/EAC3_JOC |
| Android Automotive / Auto, CarPlay | Yes | Car projections | Offline playback of downloaded My Collection content (5-device offline cap applies, not 3 — see §2); Automotive has no download/offline. [verified-web, unfetched support articles] |
| Smart TVs, Apple TV, Fire TV, consoles | Yes | Various | Dolby Atmos target platforms |
| TIDAL Connect devices | Third-party hardware | Vendor SDK | Streamer/DAC/speaker acts as playback *target*; phone/desktop is *controller* |

**[verified-source]** for the desktop/web/Linux rows; **[verified-web]** for the rest.

The desktop app auto-updates and has release notes in-app: `modal/SHOW_DESKTOP_RELEASE_NOTES`,
`session/TOGGLE_SHOW_DESKTOP_RELEASE_NOTES`, `message/MESSAGE_DESKTOP_RELEASE`, plus
`modal/SHOW_UNSUPPORTED_OS_MODAL` and `session/ACKNOWLEDGE_OUTDATED_OS`. [verified-source]

A **UI redesign was rolling out during 2025–2026**: tidal-hifi 6.3.1 added "App now autodetects
which UI Tidal is using and change its dom detection based on it", and its selector table carries
`OLD_UI_OVERRIDES` vs `NEW_UI_OVERRIDES` (e.g. old `footer-track-title` vs new `track-info`;
old `[aria-label^="toggle now playing screen"]` vs new `player-details-toggle-now-playing`)
(ref:tidal-hifi/CHANGELOG.md, ref:tidal-hifi/src/TidalControllers/DomController/constants.ts).
[verified-source]

### 2. Account tiers and entitlements as of 2026

**Tier structure.** One product, three billing shapes. TIDAL announced the HiFi/HiFi Plus
consolidation on 6 March 2024, **effective 10 April 2024** (the same date the Free tier and
Student/Military/First-Responder discounts noted below ended) — the report's first draft jumped
straight from that announcement to the Aug 2026 price rise without this date:

| Plan | US price (Aug 2026) | Notes |
| --- | --- | --- |
| Individual | $11.99/mo (was $10.99) | Price rose on the first billing date on/after 3 Aug 2026 |
| Family | $19.99/mo (was $16.99) | Up to 6 accounts |
| Student | $6.99/mo (was $5.49) | Went to $4.99 at the 10 Apr 2024 consolidation, then rose again by Aug 2026; verification required |
| DJ Extension add-on | ~$9/mo **[uncertain, possibly stale]** | Requires a base plan; unlocks DJ software integration + stem separation. A 2026 trade article title ("Big Price Increase For DJs Using Tidal, But Stems Coming Back", digitaldjtips.com) suggests this changed during 2026 and stems were pulled then reinstated — unconfirmed here, re-check before using this figure. |
| Free (ad-supported) | **Discontinued 10 April 2024** | Was 160 kbps, shuffle-only — cadence/bitrate detail is aggregator-only, **[uncertain]** |

Military/First-Responder discounts also ended 10 June 2024. **[verified-web]** — prices are from
aggregator/trade sites and should be treated as indicative, not authoritative; streamboat does not
need them.

**What the tier gates.** After the 2024 consolidation, quality is *not* tier-gated: every paying
plan gets lossless and HiRes FLAC. What remains gated:

- Playback at all (`premiumStreamingOnly`, `payToStream`, `allowStreaming`, `streamReady` per
  item — ref:TidaLuna/plugins/lib/src/redux/types/store/content/StreamingFlags.ts). [verified-source]
- DJ software integration and stems (`djReady`, `stemReady` per item) behind the DJ Extension
  add-on — the `djReady`/`stemReady` → "gates the DJ Extension" mapping itself is a reasonable
  **[inferred]** reading of the flag names, not stated anywhere in source. [verified-source +
  verified-web]
- Offline downloads: subscriber-only, **5 devices offline / 1 device online simultaneously** (not
  3 — support.tidal.com/hc/en-us/articles/201623252, "How Many Devices Can I Use
  Simultaneously?"). [verified-web]
- Simultaneous streams: one at a time (`player/STREAMING_PRIVILEGES_REVOKED`). [verified-source]

**The account model the client actually reads**
(ref:TidaLuna/plugins/lib/src/redux/types/store/User.ts): [verified-source]

```
UserSubscription {
  startDate, validUntil, status,
  subscription: { type, offlineGracePeriod },
  highestSoundQuality: AudioQuality,
  premiumAccess: boolean,
  canGetTrial: boolean,
  paymentType, paymentOverdue: boolean
}
UserClient {
  id, name, application{name,type,service}, uniqueKey,
  authorizedForOffline, authorizedForOfflineDate, lastLogin, created,
  numberOfOfflineAlbums, numberOfOfflinePlaylists
}
UserMeta {
  id, username, profileName, firstName, lastName, email, emailVerified,
  countryCode, created, newsletter, acceptedEULA, dateOfBirth,
  facebookUid, appleUid, parentId, partner, tidalId,
  earlyAccessProgram, yearOfBirth, nostrPublicKey, artistId
}
```

Notable: `earlyAccessProgram` (EAP, with `user/UPDATE_EAP_STATUS`), `artistId` (artist accounts),
`nostrPublicKey` (Block/Square lineage), `parentId` (family sub-accounts), `offlineGracePeriod`.

**API availability:** `users/{id}/subscription` and `sessions` are both reachable through the
unofficial API (ref:python-tidal/tidalapi/user.py, ref:python-tidal/tidalapi/session.py).
[verified-source]

### 3. Audio quality model

**Corrected table — the first draft of this table contradicted its own prose warning below it and
must not be copied as it originally stood.** The source of truth is the ladder in
`ref:TidaLuna/plugins/lib/src/classes/Quality.ts`: seven ordered rungs (`idx` 0–6), each with a
name and a badge colour, and *two separate* lookup tables (`lookups.audioQuality` and
`lookups.metadataTags`) that map the wire enums onto that ladder — **the two lookups disagree with
each other for the bottom two rungs, which is the trap**:

| Ladder rung (`idx`) | Badge colour | `audioQuality` enum → rung | `mediaMetadata.tags` → rung | Codec / format | Ceiling |
| --- | --- | --- | --- | --- | --- |
| 6 HiRes (UI "Max") | `#ffd432` | `HI_RES_LOSSLESS` | `HIRES_LOSSLESS` | FLAC (`FLAC_HIRES`) | 24-bit / up to 192 kHz |
| 5 MQA | `#F9BA7A` | `HI_RES` | `MQA` | MQA | removed 24 Jul 2024 |
| 4 Atmos | `#6ab5ff` | — | `DOLBY_ATMOS` | EAC3-JOC / AC-4 | audioMode `DOLBY_ATMOS` |
| 3 Sony630 | `#6ab5ff` | — | `SONY_360RA` | Sony 360RA | removed 24 Jul 2024 |
| 2 High (UI "High") | `#33FFEE` | `LOSSLESS` | `LOSSLESS` | FLAC | 16-bit / 44.1 kHz |
| 1 Low (UI "Low") | `#b9b9b9` | **`HIGH`** | — | AAC-LC (`AACLC`) | ~320 kbps |
| 0 Lowest (no 2026 UI label) | `#b9b9b9` | **`LOW`** | — | HE-AAC v1 (`HEAACV1`) | ~96 kbps |

Read the bottom two rows carefully: the wire enum value `audioQuality: "HIGH"` maps to the rung
named **"Low"**, and `audioQuality: "LOW"` maps to the rung named **"Lowest"**, which has no
current UI label at all. The 2026 UI string "Low" is therefore backed by enum `HIGH`, not enum
`LOW`. `Quality.max`/`Quality.min` operate on the numeric `idx`, so when an item carries several
`mediaMetadata.tags` (e.g. both `DOLBY_ATMOS` and `LOSSLESS`), the client takes the max `idx` as
the item's displayed quality — that is the precedence rule for badge display.

Sources: enum list in ref:TidaLuna/plugins/lib/src/redux/types/store/content/Track.ts; ladder,
colours and both lookup tables in ref:TidaLuna/plugins/lib/src/classes/Quality.ts; format mapping
in ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts
(`audioQualityToFormats`: `HI_RES`/`HI_RES_LOSSLESS` → `['HEAACV1','AACLC','FLAC','FLAC_HIRES']`;
`LOSSLESS` → `['HEAACV1','AACLC','FLAC']`; `HIGH` → `['HEAACV1','AACLC']`; `LOW` → `['HEAACV1']`).

Note the awkward historical naming that streamboat must not get wrong: the enum value `HIGH` is the
*lossy* 320 kbps tier, while the UI label "High" is the *lossless* tier (`LOSSLESS`). Anything that
maps UI strings to enums directly will produce silent quality regressions — encode the ladder table
above once, in one place, and never branch on the enum name looking like an English word.
[verified-source]

**Where quality is chosen.** `settings.quality.streaming: AudioQuality` with action
`settings/SET_STREAMING_QUALITY`, plus a `SELECT_SOUND_QUALITY` context menu on the player, plus
telemetry `eventTracking/CHANGE_STREAM_QUALITY`. Mobile splits it further into Mobile-data
streaming / Wi-Fi streaming / Download quality. [verified-source + verified-web]

**What is actually playing.** `playbackControls.playbackContext` carries
`actualAudioQuality`, `actualAudioMode`, `actualAssetPresentation`, `actualStreamType`,
`actualVideoQuality`, `bitDepth`, `sampleRate`, `codec`, `playbackSessionId`, `actualDuration`,
`assetPosition` (ref:TidaLuna/plugins/lib/src/redux/types/store/Playback.ts). tidal-hifi surfaces
exactly this as its `/current/audio-quality` API:
`{quality, badgeText, bitDepth, sampleRate, codec}` (ref:tidal-hifi/src/models/audioQuality.ts).
The UI shows a quality badge (`*[data-test^="quality-badge-"]`). [verified-source]

**Normalization.** `settings.audioNormalization: "NONE" | "ALBUM" | "TRACK"` with
`settings/TOGGLE_NORMALIZATION` (the only normalization action in the dump — there is no
`SET_NORMALIZATION`, so the UI's state-cycling logic is not shown). **The "-14 LUFS, on by default
on mobile" figure is [uncertain, source predates 2021]**: it comes only from 2019–2020 rollout
coverage (productionadvice.co.uk, audioxpress.com) of the *original* album-normalization launch,
was not re-fetched for 2026, and support.tidal.com is blocked from this environment — do not treat
it as current without re-checking. What *is* solid: ReplayGain data arrives with the playback info
as `trackReplayGain`/`albumReplayGain` + `trackPeakAmplitude`/`albumPeakAmplitude`, independently of
the LUFS target, and that is all streamboat actually needs to implement the three-state toggle.
Sone's implementation of the gain formula is `0.8 * min(10^((rg+4)/20), 1/peak)` with album/track
context switching — **note the `0.8` is Sone's own addition, not TIDAL's formula**: TIDAL's SDKs
compute `min(10^((rg+pre_amp)/20), 1/peak)` with no extra factor. [verified-source; LUFS target
**[uncertain]**]

**Audio spectrum visualiser.** `settings.audioSpectrumEnabled` /
`settings/SET_AUDIO_SPECTRUM_ENABLED` — a visualiser toggle in the desktop app. [verified-source]

### 4. Desktop app — navigation and screens

#### 4.1 Sidebar / top-level nav

From live DOM selectors (ref:tidal-hifi/src/TidalControllers/DomController/constants.ts,
[verified-source]) and tidal-hifi's *own* default hotkey config
(ref:tidal-hifi/src/features/hotkeys/hotkeyConfig.ts), whose header comment says it is
"Based on the hotkeys from https://defkey.com/tidal-desktop-shortcuts" — a third-party wrapper's
default config, derived from a third-party crowd-sourced shortcut list that was unreachable to
verify directly. **Downgrade the shortcut column from "TIDAL's own bindings" to "tidal-hifi's
default config, sourced from one unverifiable aggregator page" — [verified-web, single
unverifiable aggregator]**, not [verified-source]:

| Nav item | Selector | tidal-hifi's default hotkey (attributed to TIDAL by defkey.com) |
| --- | --- | --- |
| Music (Home) | `sidebar-music` | `alt+m` |
| Explore | `sidebar-explore` | `alt+e` |
| Feed | `sidebar-feed` | `alt+f` |
| Uploads | `sidebar-uploads` | `alt+u` |
| Collection → Playlists | `sidebar-collection-playlists` | `alt+shift+p` |
| Collection → Albums | `sidebar-collection-albums` | `alt+shift+a` |
| Collection → Tracks | `sidebar-collection-tracks` | `alt+shift+t` |
| Collection → Videos | `sidebar-collection-videos` | `alt+shift+v` |
| Collection → Artists | `sidebar-collection-artists` | `alt+shift+r` |
| Collection → Mixes & Radio | `sidebar-collection-mixes-and-radio` | `alt+shift+m` |
| Collapse/expand sidebar | `sidebar-collapse` / `sidebar-expand` | `alt+s` |

Sidebar extras: `SIDEBAR_ADD_NEW` context menu (create playlist / create folder),
`SIDEBAR_PLAYLISTS_SORT_ORDER` context menu, `selection/SET_OPEN_SIDEBAR_FOLDERS` and
`selection/SET_LOADED_SIDEBAR_FOLDERS` (expandable playlist folders in the sidebar), and drag &
drop (`selection/START_DRAG`, `selection/END_DRAG`, `view.isDragging`). [verified-source]

**The sidebar is not the full screen inventory.** The client's route loaders name **28 distinct
`route/LOADER_DATA__*` screens** (each with `--SUCCESS`/`--FAIL` variants), four of which are not
reachable from the sidebar at all: a standalone **TRACK** page, a standalone **VIDEO** page, a
**FOLDER** page (playlist folders are a navigable route, not just a sidebar disclosure triangle),
and **USER** (another person's public profile). The full list, and the generic `VIEW` route that
serves Explore/genre/mood/editorial pages, is in `references/browse-pages-screens.md`.
[verified-source]

#### 4.2 Home / For You

Home is a server-driven page of modules, not a hand-built screen. The client loads it with
`route/LOADER_DATA__HOME` and `content/LOAD_DYNAMIC_PAGE`, tracks module impressions
(`homepage/TRACK_HOME_PAGE_VIEW`, `homepage/TRACK_ITEM`, `homepage/STORE_TRACKED_ITEM`,
`homepage/HYDRATE_TRACKED_ITEMS`, `eventTracking/CLICK_MODULE`, `eventTracking/DISPLAY_PAGE`,
`eventTracking/SCROLL_PAGE`) and supports inline play (`homepage/PLAY_SINGLE_TRACK`) and
"view all" expansion (`homepage/LOAD_AND_ENQUEUE_ALL_VIEW_ALL_TRACKS`). [verified-source]

**Two incompatible backend schemas are both live, and Home has tabs plus pagination that the first
draft omitted entirely.** Full detail, including the complete module/section type vocabulary and
every observed category title, is in `references/browse-pages-screens.md`; the essentials:

- **Legacy (v1)**: `GET pages/home` → `{title, rows: [{modules: [{type, title, pagedList, ...}]}]}`.
  Also surfaced as `tidal:home` / `tidal:for_you` in `ref:mopidy-tidal/mopidy_tidal/library.py`.
- **Current (v2)**: `GET https://api.tidal.com/v2/home/feed/{slug}` with
  `deviceType=BROWSER&locale=<locale>&platform=WEB` → `{header: {vibes: {items: [{name, type}]}},
  items: [{type, title, items, viewAll, ...}], cursor}`. `header.vibes.items[]` **is the Home tab
  bar** — observed `type` values `STATIC`, `EDITORIAL`, `UPLOADS` — and `slug` is the lowercased
  `type`, so the endpoint is really `home/feed/static`, `home/feed/editorial`, `home/feed/uploads`.
  Sections page with a top-level `cursor` (pass back as `?cursor=`) and each item carries `hasMore`
  plus an `apiPath` used for "View All" expansion (the same thing
  `homepage/LOAD_AND_ENQUEUE_ALL_VIEW_ALL_TRACKS` drives client-side).
  `ref:python-tidal/tidalapi/session.py:1041-1060` implements exactly this v2 call and falls back to
  the v1 one; `ref:sone/src-tauri/src/tidal_api.rs:1240-1268, 4161-4210` parses both shapes.
- **`pages/for_you` still exists separately** as a v1 endpoint
  (ref:python-tidal/tidalapi/session.py). Do not conflate it with the v2 Home feed — they return
  different category sets.

Known module/category titles, doctest-verified against a live account by python-tidal
(`ref:python-tidal/docs/pages.rst`): For You, Recently Played, Suggested New Tracks, Suggested New
Albums, Mixes For You, Radio Stations for You, Your History, Trending Playlists, Popular
Playlists, TIDAL Rising, The Charts, Popular Albums, **Podcasts** (see the note on podcasts in
§4.12 — this category surviving in a page-module fixture is exactly the kind of thing that needs a
fresh check against a live account), Producers & Songwriters, New Releases For You, Featured,
Genres, Moods/Activities & Events, Suggested Albums for You, Suggested Artists for You. Add, from
support-article summaries only (**[uncertain]**, not re-verified for 2026): **My Daily Discovery**
(10 tracks/day) and **My New Arrivals** (30 tracks, Fridays); Upload Spotlight promotions (since
Nov 2025, [verified-web]).

Personalised mix types in the desktop client's own data model — **treat as a subset, not the
complete set**: `MixType = "VIDEO_MIX" | "TRACK_MIX" | "ALBUM_MIX" | "ARTIST_MIX"`
(ref:TidaLuna/plugins/lib/src/redux/types/store/content/Mix.ts). The live API returns more:
Sone's `TidalMix.mix_type` also documents `HISTORY_ALLTIME_MIX`, `HISTORY_MONTHLY_MIX`,
`HISTORY_YEARLY_MIX` and handles `NEW_HISTORY_MIX` (`ref:sone/src-tauri/src/tidal_api.rs:841-847`)
— this is what backs the "Your History" Home category — and the official v2 spec models the
personalised families as separate resources (`/userDailyMixes`, `/userDiscoveryMixes`,
`/userNewReleaseMixes`, `/userOfflineMixes`) reachable from
`/userRecommendations/{id}/relationships/{myMixes,discoveryMixes,newArrivalMixes,offlineMixes}`,
and explicitly documents that new `MixType` values can appear at any time. [verified-source]

#### 4.3 Explore

`route/LOADER_DATA__VIEW` / dynamic pages. Backing endpoints: `pages/explore`, `pages/moods`,
`pages/genre_page`, `pages/genre_page_local`, `pages/hires`, `pages/videos`
(ref:python-tidal/tidalapi/page.py). mopidy-tidal exposes exactly these as browse roots:
`tidal:explore`, `tidal:moods`, `tidal:genres`, `tidal:mixes`, `tidal:hires`
(ref:mopidy-tidal/mopidy_tidal/library.py). [verified-source]

Content on Explore, upgraded from [verified-web] to [verified-source] for the two items that were
previously guessed: genres, Moods & Activities, **TIDAL Rising** (emerging/unsigned artists — has
its own dedicated `pages/rising` endpoint, `ref:sone/src-tauri/src/tidal_api.rs:4386-4406`, not
merely "an Explore module" as first drafted), **The Charts** (a page category confirmed by
python-tidal's doctest output, `ref:python-tidal/docs/pages.rst`, reachable as a `PageLink` rather
than a dedicated endpoint — this part of the original conclusion holds), new releases, popular
artists, editorial articles/interviews. The client models editorial articles as a first-class
content type (`content.articles`, `content.articleLists`, `Article.ts`) — TIDAL Magazine content is
inlined into the app. [verified-source]

#### 4.4 Search

State shape (ref:TidaLuna/plugins/lib/src/redux/types/store/index.ts): [verified-source]

```
Search {
  searchPhrase, queryId, isLoading, topHit,
  recentSearches: RecentSearch[],   // kind: albums|artists|genres|playlists|search|tracks|userProfiles|videos
  searchResultFilterOrder: ["TRACKS","VIDEOS","ARTISTS","ALBUMS","PLAYLISTS","UPLOADS","USERPROFILES"],
  searchSession: { uuid, lastUpdated, triggerNewSession },
  suggestionUuid, didYouMeanByQueryMap, genres
}
```

So the shipping search has: a Top Hit, seven result categories **including Uploads and User
Profiles**, "did you mean" spelling correction, query suggestions
(`search/QUERY_SUGGESTIONS_DISPLAYED`, `search/QUERY_SUGGESTION_CLICKED`), recent searches with
clear (`search/ADD_TO_RECENT_SEARCHES`, `search/CLEAR_RECENT_SEARCHES`,
`search/RECENT_SEARCHES_RESTORED`), a search popover (`view/SHOW_SEARCH_POPOVER`), search focus
hotkey (`view/SEARCH_FOCUS`), a search session UUID for analytics, and result-consumption
telemetry (`search/SEARCH_RESULT_CONSUMED`). There is also a dedicated track/album/artist picker
search used by the Picks feature (`search/SEARCH_TRACK_FOR_TRACK_PICKER`, etc.). [verified-source]

**API availability:** `GET search?query=&limit=&offset=&types=ARTISTS,ALBUMS,TRACKS,VIDEOS,PLAYLISTS`
(exact param dict verified at ref:python-tidal/tidalapi/session.py:771-828) returning per-type
arrays plus `topHit`. Limit ≤300 total results (the docstring says "there aren't more than 300
items available in a search"). **Uploads and userProfiles result types are not implemented by
python-tidal** — reaching them would need direct API work. [verified-source]

**Recent searches and suggestions are server-side and cross-device, not local — the first draft
tiered them as "later" on the mistaken premise that they were unreachable.** The v2 spec documents
`/searchResults` (attributes `{query, trackingId, didYouMean}` — `trackingId` is the server-side
counterpart of the client's own `searchSession.uuid`; relationships `topHits`, `albums`, `artists`,
`playlists`, `tracks`, `videos`), `/searchSuggestions` (relationships `directHits`, `history`), and
`/searchHistoryEntries/{id}` for individual entries. This means a client that wants "did you mean"
and synced recent searches has a real endpoint family to target, not just the local `Search` Redux
state. [verified-source]

#### 4.5 Artist page

Structure confirmed against the API by two independent clients — **the videos row specifically was
misattributed to High Tide in the first draft; correct it to TidaLuna/python-tidal**:
[verified-source]

- Header: artist picture, name, Play, Shuffle, Follow/Unfollow, Share, artist radio button
- Top Tracks (`artists/{id}/toptracks`)
- Albums (`artists/{id}/albums`)
- EP & Singles (`filter=EPSANDSINGLES`)
- Appears On (`filter=COMPILATIONS` / "other")
- Similar Artists (`artists/{id}/similar`)
- Bio (`artists/{id}/bio`) — with source attribution, e.g. TiVo (ref:TidaLuna/.../store/content/Artist.ts
  `ArtistBio.source`); the desktop shows it in a modal (`modal/SHOW_ARTIST_BIO`). Note
  python-tidal's `get_bio()` returns only `json['text']` and discards the `source` field — a
  streamboat implementation that wants source attribution must read the raw response itself.
- Videos (`artists/{id}/videos`) — ref:python-tidal/tidalapi/artist.py; **High Tide's own artist
  page** (ref:high-tide/src/pages/artist_page.py) builds Top Tracks / Albums / EP & Singles /
  Appears On / Similar Artists / bio / radio / follow / share carousels but has **no Videos
  carousel** — do not cite High Tide for that row.
- Artist Mix / Artist Radio (`artists/{id}/mix`, `artists/{id}/radio`)

The client also models artist *roles* and *contributions*: `ArtistRoleCategory = "Artist" |
"Songwriter" | "Performer" | "Producer" | "Production team" | "Engineer" | "Misc"`, with
`content.artistContributions` and `content/LOAD_DYNAMIC_CONTRIBUTOR_PAGE` — i.e. a credits-driven
"contributor" page distinct from the artist page
(ref:TidaLuna/plugins/lib/src/redux/types/store/content/Artist.ts). Sharing a contributor is its own
context menu type (`CONTRIBUTOR_SHARE`). [verified-source]

Artist favouriting/following: `content/TOGGLE_FAVORITE_ITEMS`, `artists/FAVORITE_ARTIST_ITEM`,
plus an artist-folder concept in My Collection (`artists/SET_ARTIST_FOLDER_STATE`,
`artists/TOGGLE_FAVORITES_IN_ARTIST`). [verified-source]

#### 4.6 Album page

Routes: `LOADER_DATA__ALBUM`, `LOADER_DATA__ALBUM_CREDITS`, `LOADER_DATA__ALBUM_TRACK_MIX`.
Content model (ref:TidaLuna/plugins/lib/src/redux/types/store/content/Album.ts): [verified-source]

```
Album { id, title, cover, vibrantColor, videoCover,
        duration, streamStartDate, releaseDate, releaseYear,
        numberOfTracks, numberOfVideos, numberOfVolumes,
        copyright, type:"ALBUM", version, url, explicit, upc, popularity,
        audioQuality, audioModes[], mediaMetadata.tags[],
        upload, artist, artists[], genre, recordLabel,
        + StreamingFlags }
AlbumCredit { type, contributors: [{name, id}] }        // ref:.../store/content/index.ts:66-79, not Album.ts
AlbumReview { text, source }   // e.g. source "TiVo"    // ref:.../store/content/index.ts:66-79, not Album.ts
```

So the album page shows: cover (and animated `videoCover`), a dominant `vibrantColor` used for
theming, multi-volume track listing, duration, release date, UPC, record label, genre, copyright,
explicit flag, quality badges from `mediaMetadata.tags`, credits grouped by role, an editorial
review, and similar albums (`albums/{id}/similar`). Actions: play, shuffle, favourite,
add to playlist, create playlist from album (`content/SHOW_CREATE_PLAYLIST_FROM_ALBUM_DIALOG`),
share (`ALBUM_SHARE` context menu), album radio/track mix. Credits expansion is even tracked
(`eventTracking/EXPAND_CREDITS`). [verified-source]

**API:** `albums/{id}`, `albums/{id}/items`, `albums/{id}/tracks`, `albums/{id}/review`,
`albums/{id}/similar`, plus `pages/album?albumId=` for the module-structured page and
`content/LOAD_ALL_ALBUM_MEDIA_ITEMS_WITH_CREDITS` for per-track credits
(ref:python-tidal/tidalapi/album.py, ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts).
[verified-source]

#### 4.7 Track / media item

Model (ref:TidaLuna/plugins/lib/src/redux/types/store/content/{BaseMediaItem,Track}.ts):
`id, title, version, duration, trackNumber, volumeNumber, streamStartDate, releaseDate, explicit,
popularity, artist, artists[], album, mixes, replayGain, peak, audioQuality, audioModes[],
mediaMetadata.tags[], bpm, isrc, copyright, upload:false, url` + `StreamingFlags`.
[verified-source]

Per-track actions, from the `MEDIA_ITEM` / `MULTI_MEDIA_ITEM` context menus and the surrounding
action namespace [verified-source]:

- Play now / Play next (`playQueue/ADD_NEXT`) / Add to queue (`playQueue/ADD_LAST`)
- Add to playlist (`modal/ADD_TO_PLAYLIST_MODAL`, `content/ADD_MEDIA_ITEMS_TO_PLAYLIST`)
- Favourite / unfavourite (`content/TOGGLE_FAVORITE_ITEMS`)
- Go to album / go to artist (multi-artist → `modal/SHOW_ARTISTS_MODAL`)
- Track radio / track mix (`mix/LOAD_TRACK_MIX_ID`, `tracks/{id}/mix`, `tracks/{id}/radio`)
- Credits (`content/LOAD_ITEM_CONTRIBUTORS`; context-menu location `trackCredits`)
- Lyrics (`content/LOAD_ITEM_LYRICS`)
- Share (universal link; see §7)
- **Block** (`blocks/BLOCK`, `contextMenu/OPEN_BLOCK_ITEM`, `canBlock` flag) — hide a track or
  artist from recommendations and filter it out of the live queue
  (`playQueue/FILTER_QUEUE_ON_ADD_ITEM_TO_BLOCK`), with an undo window
  (`blocks/CLEAR_ITEM_TO_BLOCK_ON_UNDO`) and a persistent blocked list page
  (`route/LOADER_DATA__BLOCKS`)
- Multi-select operations across a track list (`selection/ADD_ITEMS`, `MULTI_MEDIA_ITEM` menu)

**Lyrics** (ref:TidaLuna/plugins/lib/src/redux/types/store/Lyrics.ts):
`{ trackId, lyricsProvider, providerCommontrackId, providerLyricsId, lyrics, subtitles,
isRightToLeft }`. `lyrics` is the plain text; `subtitles` is the timed (karaoke/synced) form; RTL
is handled. Endpoint `tracks/{id}/lyrics` (ref:python-tidal/tidalapi, and
`https://desktop.tidal.com/v1/tracks/{id}/lyrics` in ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts).
[verified-source]

#### 4.8 Playlists

Client capabilities [verified-source]:

- Create (`folders/CREATE_PLAYLIST`, `modal/SHOW_CREATE_PLAYLIST`), create from album
  (`content/SHOW_CREATE_PLAYLIST_FROM_ALBUM_DIALOG`), create from mix
  (`mix/SHOW_CREATE_PLAYLIST_FROM_MIX_DIALOG`), **create with AI**
  (`folders/CREATE_AI_PLAYLIST`, `modal/SHOW_CREATE_AI_PLAYLIST`)
- Edit metadata: title, description, cover (`modal/SHOW_EDIT_PLAYLIST_META`,
  `content/UPDATE_PLAYLIST`); info modal (`modal/SHOW_PLAYLIST_INFO_MODAL`)
- Add / remove items (`content/ADD_MEDIA_ITEMS_TO_PLAYLIST`,
  `content/REMOVE_MEDIA_ITEMS_FROM_PLAYLIST`), add a whole album
  (`content/ADD_ALBUM_TO_PLAYLIST`), add a whole mix (`mix/ADD_MIX_TO_PLAYLIST`)
- Reorder by drag (`content/MOVE_PLAYLIST_MEDIA_ITEMS`), with ETag concurrency control
  (`etag/SET_PLAYLIST_ETAG`)
- Delete (`content/DELETE_PLAYLIST`)
- Visibility toggle (`userProfiles/TOGGLE_PUBLIC_PLAYLIST` on the desktop client; legacy v1 API
  `user-playlists/{id}/public`, `/set-public` / `/set-private` — ref:python-tidal/tidalapi/playlist.py).
  **This is presented as a binary in the desktop client and in python-tidal, but the underlying
  model is three-state**: the v2 spec's `Playlists_Attributes.accessType` is
  `PUBLIC | UNLISTED | PRIVATE`. The legacy v1 boolean cannot represent `UNLISTED` — Sone's own
  v1→v2 normaliser has to fake the mapping (`access_type: raw.public_playlist.map(|p| if p
  {"PUBLIC"} else {"UNLISTED"})`, ref:sone/src-tauri/src/tidal_api.rs:526-533), which silently
  turns every legacy "private" playlist into "unlisted". A client that models visibility as a
  checkbox will get this wrong. `Playlists_Attributes` also carries `bounded`, `numberOfFollowers`,
  `numberOfTrackItems`/`numberOfVideoItems` and `externalLinks`, none of which are in the desktop
  Redux playlist model below. [verified-source]
- **Suggested tracks** appended to a playlist page
  (`content/LOAD_PLAYLIST_SUGGESTED_MEDIA_ITEMS`, `content/ADD_SUGGESTED_ITEM_TO_PLAYLIST`,
  `eventTracking/CLICK_SUGGESTED_TRACKS_PLAYNOW`)
- Sorting (`content/TOGGLE_TRACK_LIST_SORTING`, `content/HYDRATE_SORT_ORDERS`;
  `MY_PLAYLISTS_SORT_ORDER` context menu)
- **Folders**: create, rename, move items in/out, remove, add favourites to folder, sort order
  (`folders/*`, `modal/ADD_TO_FOLDER_MODAL`, `modal/ADD_MULTIPLE_ITEMS_TO_FOLDER_MODAL`,
  `MOVE_SINGLE_MODAL_ITEM_TO_FOLDER`, `MOVE_MULTIPLE_MODAL_ITEMS_TO_FOLDER`,
  `FOLDER_SORT_ORDER`)

Playlist model: `uuid, title, creator.id, description, duration, numberOfTracks, numberOfVideos,
created, lastUpdated, lastItemAddedAt, type:"USER", publicPlaylist, url, image, squareImage,
customImageUrl, promotedArtists[], popularity`
(ref:TidaLuna/plugins/lib/src/redux/types/store/content/Playlist.ts). Editorial playlists have
`type` values other than `"USER"`. [verified-source]

**Collaborative playlists: confirmed to exist, correcting the first draft's "[uncertain] — maybe
does not exist" conclusion.** No `collaborat*` action or field appears anywhere in the desktop
Redux namespace, but the official v2 spec defines the full flow: `/collaborationInvites` (GET by
`filter[code]`, POST create, DELETE — an owner mints an invite code),
`/collaborationInviteRedemptions` (a redeemer redeems the code), and on the playlist itself
`playlists/{id}/relationships/{collaborators,collaboratorProfiles}`. The generated Android SDK has
a dedicated `CollaborationInvites` Kotlin interface
(ref:tidal-sdk-android/tidalapi/src/main/kotlin/com/tidal/sdk/tidalapi/generated/apis/CollaborationInvites.kt).
The desktop dump's silence most likely means the desktop UI does not (yet) expose it, not that the
feature is absent from TIDAL — consistent with this being a mobile/web-first feature the way
collaborative playlists usually roll out. [verified-source]

**API:** `playlists/{uuid}`, `playlists/{uuid}/items`, `users/{id}/playlists`,
`users/{id}/playlistsAndFavoritePlaylists`, `my-collection/playlists/folders`,
`my-collection/playlists/folders/{create-folder,create-playlist,rename,move,remove,add-favorites}`
(ref:python-tidal/tidalapi/{playlist,user}.py). All implemented by python-tidal and therefore by
High Tide, Sone and mopidy-tidal. [verified-source]

**Reading folders at scale needs pagination, and the folder screen is a real route, not just a
sidebar tree.** `GET https://api.tidal.com/v2/my-collection/playlists/folders` takes `folderId`,
`offset`, `limit`, `order`, `orderDirection`, `countryCode`, `locale`, `deviceType`, optional
`includeOnly`, and a `cursor` — the response carries a `cursor` for the next page. There is also
`my-collection/playlists/folders/flattened`, returning every playlist across all folders with the
same cursor pagination (Sone caps its own loop at 40 pages of 50,
ref:sone/src-tauri/src/tidal_api.rs:3313-3400). The folder screen has its own client route,
`route/LOADER_DATA__FOLDER` — treat it as a screen in its own right (see
`references/browse-pages-screens.md`), not merely a disclosure triangle in the sidebar tree.
[verified-source]

#### 4.9 Mixes and radio

Types: `TRACK_MIX`, `ALBUM_MIX`, `ARTIST_MIX`, `VIDEO_MIX` in the desktop client, plus
editorial/personal "My Mix N", "My Daily Discovery", "My New Arrivals" — **treat `MixType` as
open-ended, not a closed four-value enum; see §4.2 for the history-mix values (`HISTORY_ALLTIME_MIX`
etc.) the desktop client doesn't carry.** Actions: `mix/PLAY_MIX`, `mix/LOAD_MIXES_SUCCESS`,
`mix/LOAD_TRACK_LIST_FOR_MIX_ID`, `mix/LOAD_TRACK_MIX_ID`, `content/LOAD_DYNAMIC_MIX_PAGE`,
routes `LOADER_DATA__MIX`, `LOADER_DATA__ARTIST_MIX`, `LOADER_DATA__ALBUM_TRACK_MIX`. Favouriting
mixes uses a distinct endpoint family: `favorites/mixes/add`, `favorites/mixes/remove`.
[verified-source]

**API:** `pages/mix?mixId=`, `pages/my_collection_my_mixes`, `tracks/{id}/mix`,
`tracks/{id}/radio`, `artists/{id}/mix`, `artists/{id}/radio`
(ref:python-tidal/tidalapi/{mix,artist,media}.py). [verified-source]

#### 4.10 My Collection

Six lists, each with its own route loader and sort-order context menu: Tracks, Albums, Artists,
Playlists, Videos, Mixes & Radio. Sort orders exposed by the API
(ref:python-tidal/tidalapi/types.py): [verified-source]

| List | Orders |
| --- | --- |
| Albums | `ARTIST`, `DATE` (added), `NAME`, `RELEASE_DATE` |
| Artists | `DATE`, `NAME` |
| Items (tracks/videos) | `ALBUM`, `ARTIST`, `DATE`, `INDEX`, `LENGTH`, `NAME` |
| Mixes | `DATE`, `MIX_TYPE`, `NAME` |
| Playlists | `DATE` (created), `NAME` |
| Videos | `ARTIST`, `DATE`, `NAME` |

Direction: `ASC` / `DESC`. Client state mirrors this as `List.currentOrder` /
`List.currentDirection` and `content/TOGGLE_TRACK_LIST_SORTING`
(ref:TidaLuna/plugins/lib/src/redux/types/store/content/Lists.ts). Favourite IDs are held in a flat
`favorites: { albums[], artists[], mixes[], playlists[], tracks[], users[], videos[] }` — note
**users** are favouritable too. [verified-source]

**API:** `users/{id}/favorites/{albums,artists,tracks,videos,playlists}` with
`limit/offset/order/orderDirection`, plus cursor pagination on some lists
(ref:python-tidal/tidalapi/user.py). [verified-source]

**A seventh collection type the six-list inventory above omits: Save for Later.** The v2 spec has
`/userCollectionSaveForLaters/{id}` (`{numberOfItems, lastModifiedAt}` + an `items` relationship),
and `/userCollectionFolders` generalises the folder concept past playlists specifically. Neither
has an OSS-client implementation to point to yet. [verified-source]

#### 4.11 History / recently played

`content/LOAD_RECENT_ACTIVITY`, `accumulatedPlaybackTime` state (per-product accumulated listening
time keyed by product id), and `cloudQueue/FILL_CLOUD_QUEUE_WITH_HISTORY`. Recently Played on Home
is fed by the **play_log telemetry**, not by the client's own list: Sone had to implement
`play_log` event batches to `https://ec.tidal.com/api/event-batch` to make Recently Played reflect
its playback (ref:sone/src-tauri/src/tidal_report/event.rs, ref:sone/README.md). Event payload
fields include `playbackSessionId`, `isPostPaywall`, `productType`, `requestedProductId`,
`actualProductId`, `actualAssetPresentation`, `actualAudioMode`, `actualQuality`,
`startTimestamp`/`endTimestamp`, `startAssetPosition`/`endAssetPosition`, `actions[]`,
`sourceType`/`sourceId`. [verified-source]

#### 4.12 Videos

Videos are first-class media items alongside tracks: `ContentType = "track" | "video"`, a
`Video` model with `quality: "MP4_1080P"`, `imageId`, `vibrantColor`, ad fields (`adsUrl`,
`adsPrePaywallOnly`), a My Collection → Videos list, a `VIDEO_MIX` mix type, a `VIDEOS` search
category and a `pages/videos` explore page (ref:TidaLuna/.../store/content/Video.ts,
ref:TidaLuna/.../store/content/Mix.ts, ref:TidaLuna/.../store/index.ts). Stream URLs come from
`videos/{id}/urlpostpaywall` (ref:python-tidal/tidalapi/media.py:976) — **the
`playbackinfopostpaywall` variant for videos is not in python-tidal; cite
ref:sone/src-tauri/src/tidal_api.rs:3816 and ref:tidalswift/.../ContentUrls.swift:32 instead**.
Sone plays videos via a separate HLS decoder path; its own README says only that video audio "does
not use the bit-perfect lossless signal path" — **the commonly repeated "650,000+ videos" and
"AAC ~320 kbps" figures are [uncertain, possibly stale]**, sourced only to a whathifi review of
unstated vintage, and were not re-verified for 2026. [verified-source; catalogue size and audio
codec **[uncertain]**]

#### 4.13 Feed, profiles and social

- **Feed — the first draft conflated this with the Home page; they are different endpoints with
  different content.** `feed/LOAD_FEED`, `feed/CHECK_FEED_UPDATES`, `feed.hasNewFeedItems`,
  `view/SHOW_FEED_SIDEBAR` / `HIDE_FEED_SIDEBAR`, and a `FEED` play-queue source type — the Feed is
  both a page and a right-hand sidebar you can play from. `home/feed/static` (cited as the Feed API
  in the first draft) is actually the **v2 Home page** endpoint — see §4.2 — not the social Feed.
  Sone fetches the real social Feed separately and parses `activities[]` /
  `totalNotSeenActivities` rows (`flatten_feed_activity`, `parse_feed_body`,
  ref:sone/src-tauri/src/tidal_api.rs:680-770, 4161-4210). Use that as the Feed API reference
  instead. [verified-source]
- **Public profile**: `UserProfile { userId, name, picture, color: [c1,c2,c3], numberOfFollowers,
  numberOfFollows, followers, followingUsers, followingArtists, publicPlaylists }`. **Correction:
  these last four are not bare arrays** — each is an object wrapping an `items` array (e.g.
  `followers: { items: Following[] }`). `Following` itself carries `followType: "USER" | "ARTIST"`,
  `blocked`, an `imFollowing` flag the first draft omitted, and a `trn:user:{id}` resource name.
  Actions `user/UPDATE_PROFILE_NAME`, `user/UPDATE_PROFILE_PICTURE`,
  `user/UPDATE_PROFILE_SOCIAL_HANDLES`, `user/DELETE_PROFILE_PICTURE_BUTTON_CLICKED`, and picture
  sources from Facebook / Snapchat / TikTok
  (`user/{FACEBOOK,SNAPCHAT,TIKTOK}_PICTURE_BUTTON_CLICKED`, routes `LOADER_DATA__{FACEBOOK,
  SNAPCHAT,TIKTOK}`). [verified-source]
- **Picks / track prompts**: a `trackPrompts/*` namespace — `SET_TRACK_FOR_PROMPT`,
  `REMOVE_TRACK_FOR_PROMPT`, `TOGGLE_PROMPTS`, `GENERATE_SHARE_IMAGES`,
  `STORE_GENERATED_SHARE_IMAGES`, `PLAY_MY_PICKS_ITEM`, with `ALBUM_PROMPT`/`ARTIST_PROMPT`/
  `TRACK_PROMPT` context menus. This is the "My Picks" profile feature (pinned favourites answering
  prompts, shareable as generated images). [verified-source] The "public and on by default, with an
  off switch in Settings" characterisation rests only on a search-summary of a blocked
  support.tidal.com article — keep it **[verified-web, unfetched]**, not source-verified.
- **Artist picker** onboarding (`artistPicker/*`, `route/LOADER_DATA__ARTIST_PICKER`) — the
  taste-onboarding flow for new accounts. [verified-source]
- **Onboarding steps** the client tracks (ref:.../store/User.ts): `ADD_TO_FAVORITES`,
  `ADD_TO_OFFLINE`, `ADD_TO_PLAYLIST`, `ALBUM_INFO`, `ARTIST_CREDITS`, `ARTIST_PICKER`, `BLOCK`,
  `BOTTOM_NAVIGATION_INTRO`, `CAST`, `CAST2`, `CONTRIBUTOR`, `DOLBY_ATMOS`, `LYRICS`,
  `MENU_MY_MUSIC`, `MENU_OFFLINE_CONTENT`, `OPTIONS_MENU`, `PLAY_QUEUE`, `PLAY_QUEUE_BUTTON`,
  `PLAY_QUEUE_SUGGESTIONS`, `RELOCATION_SETTINGS`, `SETTINGS`, `SPRINT_REDESIGN_UPDATE`,
  `USER_PROFILE_ONBOARDED`, `WEB_3.0.0_UPDATE`. This list doubles as a checklist of features TIDAL
  itself considers headline. [verified-source]

**A 2025–2026 social/creator layer that the first draft missed entirely** (all from the v2 spec,
`ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json`; example timestamps in the schemas are
2025-09/2025-11, i.e. this shipped around the TIDAL Upload launch already covered in §4.14):

- **Comments** — `/comments` (GET/POST/PATCH/DELETE), attributes `{message (1..2000 chars),
  createdAt, lastModifiedAt, likeCount, replyCount, moderationStatus: NOT_MODERATED|FLAGGED|
  TAKEN_DOWN|OK|ERROR, startTime, endTime}`. `startTime`/`endTime` are ISO-8601 durations (e.g.
  `PT1M30S`) — **SoundCloud-style time-anchored comments on a track**. Subjects are restricted to
  `albums` and `tracks`; replies hang off `parentComment`.
- **Reactions and appreciations** — `/reactions` (with a `CurrentUserReaction` schema) and
  `/appreciations` (POST-only, `appreciatedItem` type enum = `artists`) are separate gesture types
  from comments.
- **Creator/monetisation** — `/artistClaims`, `/manualArtistClaims`, `/artistClaimStatuses`,
  `/contentClaims`, `/trackStatistics/{id}`, `/albumStatistics/{id}`, `/purchases`,
  `/priceConfigurations`, `/stripeConnections`, `/stripeDashboardLinks`, `/squareConnections`,
  `/trackSourceFiles`.
- **Reporting** — `/userReports` for reporting content.

Each of these needs at minimum an explicit out-of-scope row in the feature matrix with a reason —
they carry real moderation and privacy implications if streamboat ever renders them (see the
"owner decisions" list at the end of this report). None has an OSS-client implementation to point
to yet. [verified-source]

#### 4.14 Uploads (TIDAL Upload)

Launched November 2025. Users upload their own audio; the client namespace is `creatorContent/*`:
`START_TRACK_UPLOAD`, `SET_TRACK_UPLOAD_PROGRESS`, `SET_TRACK_UPLOAD_ERROR`, `FINISH_TRACK_UPLOAD`,
`SET_TRACK_ITEM_ID`, `START_ARTWORK_UPLOAD`, `FINISH_ARTWORK_UPLOAD`, plus context menus
`OPEN_CREATOR_CONTENT_OWNED_ITEM` / `OPEN_CREATOR_CONTENT_RECEIVED_ITEM`. Uploaded items are marked
by an `upload: boolean` flag on Album (and `upload: false` on catalogue tracks), and `UPLOADS` is a
search result category. Limits: ≤5 GB per track, ≤200 tracks, beta, no royalties, available in
US/UK/EEA/Switzerland/Canada, 18+. TIDAL ran an "Upload Headliners" $100k contest and a weekly
editorial Spotlight. [verified-source + verified-web]

Note for cover-art handling: `getCoverURL` in tidal-hifi returns the uploaded URL verbatim when it
is already an absolute URL, because uploaded content does not use the
`resources.tidal.com/images/<uuid-with-slashes>/<size>x<size>.jpg` convention
(ref:tidal-hifi/src/features/tidal/url.ts). [verified-source]

#### 4.15 Live / DJ

TIDAL "Live" (originally beta-named "DJ") launched April 2023: a subscriber broadcasts what they
are playing in real time to other subscribers. **There is no `live/` namespace in the 2026 desktop
Redux action list**, which strongly suggests the feature is not present in the current desktop
client. No announcement of its discontinuation was found. Status: **[uncertain]** — do not treat
Live as an active feature; do not treat it as confirmed removed.

Distinct from Live is the **DJ Extension add-on** (~$9/mo **[uncertain, possibly stale — see §2]**),
which integrates TIDAL as a source in Serato DJ Pro (3.1.3+), rekordbox and similar, including
**stem separation**. The item-level flags `djReady` and `stemReady` gate this — plausible but
**[inferred]**, not stated anywhere in source. This is an integration with third-party DJ software,
not an in-app feature. [verified-source + verified-web]

#### 4.16 Settings (desktop)

The complete settings state the desktop client persists
(ref:TidaLuna/plugins/lib/src/redux/types/store/index.ts): [verified-source]

```
SettingsState {
  audioNormalization: "NONE" | "ALBUM" | "TRACK",
  audioSpectrumEnabled: boolean,
  autoPlay: boolean,
  desktop: { autoStartMode: 0 | 1, closeToTray: boolean },
  explicitContentEnabled: boolean,
  language: string,
  openLinksInDesktopApp: boolean,
  quality: { streaming: AudioQuality },
  updateAvailable: boolean,
  urls: { accessibility, betaParticipationAgreement, earlyAccess, privacy, support, terms }
}
```

Plus, not in `settings` but user-facing settings surfaces:
sound output device and exclusive/shared mode (`player.*`, `SELECT_SOUND_OUTPUT`),
force-volume per device (`player.forceVolume`, `player/SET_FORCE_VOLUME` — mutually exclusive with
exclusive mode in the UI), Last.fm connection (`lastFm/*`), explicit-content confirmation modal
(`modal/SHOW_EXPLICIT_TOGGLE_MODAL`), keyboard-shortcut cheatsheet (`modal/SHOW_SHORTCUTS`),
release notes, logout (`modal/SHOW_LOGOUT_MODAL`), feature-flag user overrides
(`featureFlags/TOGGLE_USER_OVERRIDE` — an internal/EAP surface), and the blocked-items page.
[verified-source]

Absent from *this* settings dump, notably: no crossfade, no equalizer, no gapless toggle, no
cache-size control, no download/offline settings. Per the version caveat at the top of this report,
read "no crossfade" here as "not in TidaLuna 1.16.6-beta (2026-09-02)" — TIDAL has since announced
a crossfade toggle for iOS/Web (§Summary item 8) which is not reflected in this dump. The rest
(equalizer, cache-size, download settings) has no counter-evidence and can still be treated as
genuinely absent from the desktop client. [verified-source, absence-based → **[inferred]**]

### 5. Player and playback

#### 5.1 Transport and player state

Actions (ref:.../actions/actionTypes.ts): `playbackControls/{PLAY, PAUSE, TOGGLE_PLAYBACK, STOP,
SKIP_NEXT, SKIP_PREVIOUS, SEEK, SEEK_FORWARDS, SEEK_BACKWARDS, START_AT, TIME_UPDATE, SET_DURATION,
SET_VOLUME, INCREASE_VOLUME, DECREASE_VOLUME, SET_MUTE, TOGGLE_MUTE, SET_VOLUME_UNMUTE,
SET_PLAYBACK_STATE, SET_DESIRED_PAUSE_STATE, MEDIA_PRODUCT_TRANSITION,
PREFILL_MEDIA_PRODUCT_TRANSITION, UPDATE_PLAYBACK_CONTEXT, ENDED}`. Volume is 0–100 in state.
Playback states: `PLAYING | IDLE | PAUSED | NOT_PLAYING | STALLED`. Player engines:
`BOOMBOX | GOOGLE_CAST | REMOTE_PLAYBACK`; active-player kinds `PLAYER_SDK | WEBPLAYER |
EXTERNAL_PLAYER`. [verified-source]

#### 5.2 Queue

State (ref:.../store/PlayQueue.ts): `elements[]` of `{uid, mediaItemId, context, priority}` where
`priority ∈ {priority_history, priority_keep, priority_none}`, plus `currentIndex`, `repeatMode`
(`Off=0, All=1, One=2`), `shuffleModeEnabled`, `lastShuffleSeed`, `originalSource[]`,
`backupElements[]`/`backupSource[]`, and source description (`sourceName`, `sourceTrackListName`,
`sourceUrl`, `sourceEntityId`, `sourceEntityType`). [verified-source]

Queue operations: `ADD_NOW`, `ADD_NEXT`, `ADD_LAST`, `ADD_AT_INDEX`, `ADD_NOW_REST_OF_TRACK_LIST`,
`MOVE_TRACK {fromIndex,toIndex}`, `CLONE_TRACK`, `REMOVE_AT_INDEX`, `REMOVE_ELEMENT {uid}`,
`CLEAR_QUEUE`, `CLEAR_ACTIVE_ITEMS`, `SET_CURRENT_INDEX`, `MOVE_TO`, `MOVE_NEXT`, `MOVE_PREVIOUS`,
`SET_REPEAT_MODE`, `TOGGLE_REPEAT_MODE`, `TOGGLE_SHUFFLE`,
`ENABLE_SHUFFLE_MODE_AND_SHUFFLE_ITEMS {shuffleSeed}`,
`DISABLE_SHUFFLE_MODE_AND_UNSHUFFLE_ITEMS`, plus lazy list filling
(`FETCH_FIRST_PAGE_AND_ADD_TO_QUEUE`, `FETCH_REST_OF_THE_TRACKS_AND_ADD_TO_QUEUE` — so playing a
10,000-track collection does not require loading it first). Queue and player settings persist to
local storage (`LOAD_PLAY_QUEUE_FROM_LOCAL_STORAGE_SUCCESS`,
`LOAD_PLAYER_SETTINGS_FROM_LOCAL_STORAGE_SUCCESS`). Queue is a right-hand aside
(`view/TOGGLE_PLAY_QUEUE_VISIBILITY`, `HIDE_PLAY_QUEUE_ASIDE`, `#playQueueSidebar`).
[verified-source]

Shuffle is **seeded** (`shuffleSeed`) and reversible — unshuffling restores the original order.
Any streamboat queue that shuffles destructively will not match. [verified-source]

**Autoplay / "up next" continuation**: `settings.autoPlay` with `settings/SET_AUTOPLAY` and
`settings/TOGGLE_AUTOPLAY`, `content/LOAD_SUGGESTIONS` and a `suggestions` play-queue source type,
`player/FORCE_AUTOMATIC_PROGRESSION`. When the queue ends, TIDAL appends algorithmically suggested
tracks. TIDAL notes that not all TIDAL Connect devices support Autoplay. [verified-source +
verified-web]

**Gapless**: implemented by preloading — `player/PRELOAD_ITEM`, `player/PRELOAD_NEXT_ITEM`,
`player/PRELOAD_SUCCESS`, `playbackControls.prefilled` (ref:.../store/Playback.ts:42
`prefilled: boolean`), and `PREFILL_MEDIA_PRODUCT_TRANSITION` — **this action lives in the
`playbackControls/` namespace, not `playQueue/`** as an earlier draft implied. tidal-hifi 8.1.0 had
to fix its position reading because "player state … is now read from the active audio buffer …
after TIDAL's gapless playback switches buffers" (ref:tidal-hifi/CHANGELOG.md 8.1.0) — direct
evidence gapless is live in the web/desktop player. [verified-source]

**Crossfade**: not found in *this* dump of the desktop client, but confirmed as a live, officially
announced 2026 feature on iOS and Web (0–12 second slider, Settings toggle) — see §Summary item 8.
Whether the Electron desktop client has shipped it yet is the genuinely open question; the dump's
silence is far more likely to mean it predates the rollout than that desktop lacks it. **[uncertain
— desktop status only; the feature's existence is [verified-web], not uncertain]**.

#### 5.3 Audio output on desktop

`Player` state: `activeDeviceId`, `activeDeviceMode: "exclusive" | "shared"`, `availableDevices[]`
of `{id, name, nativeDeviceId, webDeviceId, type, controllableVolume}`, `desiredDeviceMode` (per
device), `forceVolume` (per device), `hasPreloadedNextProduct`. Actions `player/SET_ACTIVE_DEVICE`,
`player/SET_DEVICE_MODE`, `player/SET_FORCE_VOLUME` and the `SELECT_SOUND_OUTPUT` context menu.
[verified-source]

Behavioural facts [verified-web, from a search-summary of
support.tidal.com/hc/en-us/articles/28548110049681-Exclusive-Mode]: Exclusive Mode takes exclusive
control of the output device, so volume must be changed in TIDAL rather than the OS mixer;
Exclusive Mode and Force Volume are mutually exclusive in the UI (turning on Exclusive greys out
Force Volume — "cannot select both exclusive mode and forced volume simultaneously," corroborated
independently by a forums.whathifi.com thread). It matches the source rate/depth (16/44.1, 24/96,
24/192) via **WASAPI exclusive mode on Windows** — this half is well attested. The macOS mechanism
is *not*: "Core Audio hog mode" is a plausible but **[inferred]** guess repeated in earlier drafts
of this report; nothing reachable states how TIDAL implements exclusive output on macOS, so do not
present it as fact.

**Force Volume, precisely** (support.tidal.com wording, verbatim): "With Force Volume, Tidal keeps
the app's volume at the maximum level, allowing you to control the sound output through external
devices like your DAC or speakers." This is the *opposite* of a software-volume fallback — it pins
the in-app level at 100% so an external DAC/amp is the sole volume control. The feature matrix in
§Feature matrix below states this correctly; earlier drafts had a contradictory gloss in one row
that has been corrected.

#### 5.4 Remote playback: Connect, Chromecast, AirPlay

Three device transports discovered and connected uniformly
(ref:.../store/RemotePlayback.ts): [verified-source]

```
RemotePlaybackDeviceType = "chromeCast" | "tidalConnect" | "cloudConnect"
RemotePlaybackDevice = { id, friendlyName, fullname /* e.g. x._googlecast._tcp.local */,
                         addresses[], port, type }
RemotePlayback { devices: {chromeCast[], cloudConnect[], tidalConnect[]},
                 connectedDevice, deviceToConnectTo, isConnected,
                 remotePlaybackReceiverState, session, sessionStatus }
```

Actions: `remotePlayback/{DISCOVER_DEVICES, REFRESH_DEVICES, DEVICES_RECEIVED, CONNECT_TO_DEVICE,
DEVICE_CONNECTED, DEVICE_DISCONNECTED, DISCONNECT_ALL_DEVICES, CONNECTION_LOST}`, plus per-transport
sub-namespaces including `remotePlayback/tidalConnect/{MEDIA_CHANGED, QUEUE_CHANGED,
QUEUE_ITEMS_CHANGED, UPDATE_PLAYER_STATE, HANDLE_ERROR, DISCONNECT}` and
`remotePlayback/remotePlaybackReceiver/{MEDIA_CHANGED, STATE_CHANGED, DISCONNECT}` — the
`remotePlaybackReceiver` namespace implies the client can also *be* a receiver. The evidence for
this is stronger than a namespace name: `modal/REMOTE_PLAYBACK_RECEIVER_DISCONNECT_MODAL` is a
**user-facing dialog** that only makes sense if the desktop app can be connected *to* as a target —
this raises the stakes on Implication 11 below (do not build a Connect target): being a target may
be technically reachable on the desktop client, it is just not open-source-reproducible via the
vendor SDK path (§5.4 continues below). Discovery is mDNS (`_googlecast._tcp.local` visible in the
type). AirPlay is not a client-side transport: on macOS it is an OS-level output device that shows
up in the audio device list. [verified-source; receiver role **[uncertain, stronger evidence than
before]**]

`cloudConnect` + the `cloudQueue/*` namespace (`CREATE_CLOUD_QUEUE`, `GET_CLOUD_QUEUE_ITEMS`,
`ADD_ITEMS_TO_CLOUD_QUEUE`, `MOVE_TRACKS`, `REMOVE_ELEMENT`, `SET_CURRENT_ITEM`, `SET_SHUFFLED`,
`UPDATE_ITEMS_ETAG`, `FILL_CLOUD_QUEUE_WITH_HISTORY`) is server-side queue state with ETags — this
is how the queue follows you between devices. [verified-source]

**API availability:** none of the reference open-source clients *implement* TIDAL Connect
controlling, cloud queue or Chromecast — but "no OSS client implements it" is not the same claim as
"undocumented", which the first draft conflated for cloud queue specifically. `/playQueues` and
`/playQueues/{id}` are documented in the v2 spec (see §Summary item 12 for the exact attributes and
the repeat/shuffle enum mismatch an implementer must handle). TIDAL Connect device
control itself remains genuinely undocumented outside the vendor SDK. tidal-connect
(ref:tidal-connect) runs a **proprietary closed-source binary** (`tidal_connect_application`) with
a vendor certificate to act as a Connect *target* — that path is not open-source-reproducible. This
remains the single largest capability gap for any third-party client on the *target* side; the
*controller* side (cloud queue, discovery) has more documented surface than previously stated.
[verified-source]

#### 5.5 Now Playing, mini player, full screen

`view/{ENTER_NOWPLAYING, ENTERED_NOWPLAYING, EXIT_NOWPLAYING, EXITED_NOWPLAYING, REQUEST_FULLSCREEN,
EXIT_FULLSCREEN, FULLSCREEN_ALLOWED, FULLSCREEN_DENIED, ENTER_NATIVE_FULLSCREEN,
LEAVE_NATIVE_FULLSCREEN, MINIMIZE, TOGGLE_VISIBILITY, LEAVE_PORTAL}`, with state
`isNowPlaying`, `showNowPlaying`, `isFullscreen`, `isNativeFullscreen`, `isPortal`. The Now Playing
screen carries the lyrics and credits panels; toggling it has a dedicated control
(`player-details-toggle-now-playing` in the new UI). The official desktop shortcut for it is
`Ctrl+P` (mirrored by tidal-hifi as `expandNowPlaying`). [verified-source]

There is no separate always-on-top mini-player window in the official desktop client's action
namespace — `view/MINIMIZE` is window minimisation. Sone and sone-windows *add* a floating
miniplayer as a differentiator, which is evidence the official app lacks one.
[verified-source → **[inferred]**]

#### 5.6 Keyboard shortcuts, media keys, notifications

**Relabelled: this is tidal-hifi's default hotkey config, not an attested TIDAL document.**
`ref:tidal-hifi/src/features/hotkeys/hotkeyConfig.ts` (`DEFAULT_HOTKEY_ACTIONS`) is a third-party
wrapper's own configurable global-hotkey defaults, using tidal-hifi-specific action ids
(`hardReload`, `sidebarMusic`, `openSettings1`/`openSettings2`), whose header comment says it is
"Based on the hotkeys from https://defkey.com/tidal-desktop-shortcuts" — a crowd-sourced
third-party aggregator, unreachable to verify directly. Treat the whole table as
**[verified-web, single unverifiable aggregator]**, not [verified-source]; the desktop client's own
in-app cheatsheet (`modal/SHOW_SHORTCUTS`, [verified-source]) is the thing to check on a live
install if this matters:

| Shortcut | Action (per tidal-hifi's defaults) |
| --- | --- |
| `Ctrl+A` | Toggle favourite on current track |
| `Ctrl+L` | Log out |
| `Ctrl+U` | Hard reload |
| `Ctrl+R` | Cycle repeat mode |
| `Ctrl+W` | Copy universal track link |
| `Ctrl+P` | Expand Now Playing |
| `Ctrl+↑` / `Ctrl+↓` | Volume ±10% |
| `Alt+←` / `Alt+→` | Back / forward |
| `Ctrl+=` or `Ctrl+0` | Settings |
| `Alt+M/E/F/U` | Music / Explore / Feed / Uploads |
| `Alt+S` | Toggle sidebar |
| `Alt+Shift+P/A/T/V/R/M` | Collection: Playlists / Albums / Tracks / Videos / Artists / Mixes & Radio |

Note also a small discrepancy worth resolving before copying this: tidal-hifi's config describes
`Ctrl+R` as cycling "off, one, all", whereas the client's own `RepeatMode` enum is ordered
`Off=0, All=1, One=2` — the cycle direction/order is not independently confirmed either way.

In-app cheatsheet: `modal/SHOW_SHORTCUTS`. Shortcuts are window-local, not global/system-wide.
Hardware media keys are handled by the OS via the standard media-key path (`MediaPlayPause`,
`MediaNextTrack`, `MediaPreviousTrack`); on Linux, MPRIS integration is supplied by the wrappers,
not by TIDAL. Desktop notifications on track change are a wrapper feature, not confirmed in the
official app. [verified-source; official-app notification behaviour **[uncertain]**]

#### 5.7 Explicit content, blocking, ads

- `settings.explicitContentEnabled` / `settings/SET_EXPLICIT_CONTENT_TOGGLE` with a confirm modal
  (`modal/SHOW_EXPLICIT_TOGGLE_MODAL`); every media item carries `explicit: boolean`.
  [verified-source]
- Blocking: see §4.7. A block list page exists (`route/LOADER_DATA__BLOCKS`) and blocks persist
  (`blocks/LOAD_PERSISTENT_BLOCKED_LIST`). [verified-source]
- Ads: `playQueue/ADD_PROMO_ITEM_TO_QUEUE`, `Video.adsUrl`, `Video.adsPrePaywallOnly`,
  `StreamingFlags.adSupportedStreamReady`. tidal-hifi ships uBlock filters that disable "audio &
  visual ads … and unlimited skips" (ref:tidal-hifi/README.md). Since the free tier was withdrawn in
  April 2024, the presence of live ad plumbing in 2026 is **[uncertain]** — possibly vestigial,
  possibly promo/pre-paywall content, possibly a market-specific ad-supported mode.

#### 5.8 Voice

`speech/{START_RECOGNITION, STOP_RECOGNITION, PARSE_VOICE_COMMAND}` with
`speech.recognitionSupported` — the desktop client has voice command support. Not documented
publicly; likely uses the Web Speech API. [verified-source, purpose **[inferred]**]

### 6. Streaming and manifests — what a client must implement

**This section is a feature-level summary; the wire-format depth lives in `tidal-api.md`
(§8 playbackinfo, §8.2 manifest types, §8.3 encryption, §8.4 the quality cascade, §11 images) —
go there for anything beyond what's needed to see that the feature exists.** The first draft of
this report re-derived that material here at length; keep it in one place going forward.

The playback path every unofficial client uses (all **[verified-source]**):

1. `GET https://api.tidal.com/v1/tracks/{id}/playbackinfopostpaywall`
   `?audioquality=HI_RES_LOSSLESS|LOSSLESS|HIGH|LOW&playbackmode=STREAM&assetpresentation=FULL&countryCode=XX`
   (ref:python-tidal/tidalapi/media.py; also `urlpostpaywall` with `urlusagemode=STREAM` for a
   direct URL — used by tidalt and tidalrs).
   The official desktop uses `https://desktop.tidal.com/v1/tracks/{id}/playbackinfo`
   (ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts) with a **semaphore of 2 concurrent
   requests**.
2. Response carries `manifestMimeType` (`application/dash+xml` or `application/vnd.tidal.bts`,
   historically also `application/vnd.tidal.emu`), base64 `manifest`, `manifestHash`,
   `audioQuality`, `audioMode`, `bitDepth`, `sampleRate`, and the four ReplayGain/peak fields.
3. BTS manifests base64-decode to JSON `{mimeType, codecs, encryptionType, keyId, urls[]}`;
   DASH manifests decode to MPD XML.
4. Quality fallback cascade — request the highest tier, degrade on failure; distinguish terminal
   sub-statuses from transient errors. **Correction: the terminal set is not a contiguous range.**
   Sone's actual constant is `TERMINAL_SUB_STATUSES = [4005, 4010, 4030, 4031, 4032, 4034, 4035]`
   (ref:sone/src-tauri/src/tidal_api.rs:18) — **4033 is deliberately absent**. Do not write this as
   "4030–4035".
5. The modern v2 path is `GET /trackManifests/{id}` with
   `formats=[HEAACV1,AACLC,FLAC,FLAC_HIRES]&manifestType=HLS|MPEG_DASH&uriScheme=DATA&usage=PLAYBACK&adaptive=`
   plus an `x-playback-session-id` header; manifests expire after ~1 hour
   (ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts).

Manifests can be DRM-protected (FairPlay at `fp.fa.tidal.com/license`, Widevine at
`api.tidal.com/v2/widevine`) — that is the path the official web player and the official SDKs take.
The unofficial-API path used by High Tide/Sone/python-tidal reaches unencrypted FLAC/AAC manifests
for the subscriber's own account; Strawberry explicitly *refuses* to play anything where
`encryptionKey` is non-empty or `encryptionType`/`securityType` != NONE and shows a user-facing
error instead (ref:strawberry/src/tidal/tidalstreamurlrequest.cpp). That is the correct posture for
streamboat: **detect encryption and decline, never circumvent.** [verified-source]

**Failure behaviour has a native-app model this report did not previously mine, and streamboat
needs a parity target for it, not an invented one.** The desktop client has a whole
`network/{CONNECT, CONNECTION_ESTABLISHED, CONNECTION_LOST, CONNECTION_REFRESH, FAILED_STARTUP}`
namespace for connectivity state, and a typed notification system:
`message/{MESSAGE_INFO, MESSAGE_WARN, MESSAGE_ERROR, MEDIA_NOT_PLAYABLE, MESSAGE_BLOCK,
MESSAGE_RELEASE, MESSAGE_DESKTOP_RELEASE, CLEAR_MESSAGE, CLEAR_MESSAGES}`, backed by a
`Notification { id, category: MUTE|NETWORK|OTHER|PLAYBACK, severity: DEBUG|INFO|WARN|ERROR,
message, data }` model (ref:TidaLuna/.../store/Notification.ts) — i.e. a typed toast/notification
system with dedicated `PLAYBACK` and `NETWORK` categories. Add
`player/STREAMING_PRIVILEGES_REVOKED` (§Summary item 18) and `remotePlayback/CONNECTION_LOST`. This
is enough to specify streamboat's whole error-surface as an explicit parity target instead of
inventing one from scratch. [verified-source]

**Region unavailability and TIDAL's own replacement mechanism are also worth building for from day
one.** Every real library has tracks unplayable in the user's country, and TIDAL actively
substitutes alternatives rather than showing dead rows: the v2 spec exposes a `replacement`
relationship on `tracks`/`videos`/`albums` and a `replaceMedia=<relationship paths>` query
parameter, with each identifier then carrying `meta.replacement: ORIGINAL|REPLACED|NOT_REPLACED`
(flagged "BETA Internal only" in the spec as of this reading). On the v1 side the equivalent
signals are the per-item `StreamingFlags` already in §4.7 (`streamReady`, `allowStreaming`,
`premiumStreamingOnly`, `payToStream`) plus the user-facing `message/MEDIA_NOT_PLAYABLE` action —
i.e. the native app surfaces a message and moves on rather than stalling on a dead track.
`/usageRules` and `/tracks/{id}/relationships/usageRules` are the v2 home for per-item entitlement
rules. [verified-source]

### 7. Sharing and deep links

- Web URLs: **only the track form is source-verified** — tidal-hifi builds
  `${tidalUrl}/browse/track/${trackId}` (`getTrackURL`, ref:tidal-hifi/src/features/tidal/url.ts).
  The album/artist/playlist/video/mix/user variants
  (`https://tidal.com/browse/{album|artist|playlist|video|mix|user}/{id}`) are **[inferred]** by
  pattern extrapolation, not individually attested. [verified-source for track; rest **[inferred]**]
- Content models carry canonical `url` fields: `http://www.tidal.com/track/{id}`,
  `/album/{id}`, `/artist/{id}`, `/playlist/{uuid}` (ref:.../store/content/*.ts) — this is a
  *different* scheme from the `/browse/` one above; both exist. [verified-source]
- **Universal links**: appending `?u` (or `&u`) to a TIDAL share link turns it into a
  cross-platform link that resolves to the listener's own streaming service. `getUniversalLink`
  appends `&u` when a query string already exists, else `?u`
  (ref:tidal-hifi/src/features/tidal/url.ts; TIDAL support: "Sharing music links across streaming
  platforms"). TIDAL's own share menu produces these. The v2 spec also models this server-side, not
  just as a URL trick: `/dspSharingLinks` and `/dspSharingLinks/{id}/relationships/subject` return
  `{spotify, appleMusic, amazonMusic, youTubeMusic}` link objects, and a separate `/shares` +
  `/savedShares` resource family (GET/POST) models sharing as server-side state. Content resources
  additionally carry `externalLinks` in their v2 attributes. Treat "universal share link" as more
  than a trivial string append if streamboat ever wants to *resolve* a shared link or show which
  other services a track is on. [verified-source + verified-web]
- Cover art: `getCoverURL` builds
  `https://resources.tidal.com/images/<cover-uuid-with-dashes-replaced-by-slashes>/<size>x<size>.jpg`,
  sizes including 80 and 1280, and passes through absolute URLs untouched — needed for uploaded
  content, which does not use this convention (see §4.14). (ref:tidal-hifi/src/features/tidal/url.ts).
  [verified-source]
- **`tidal://` custom scheme — grammar now confirmed, correcting the first draft's "undocumented
  anywhere" conclusion.** The grammar was in two reference checkouts this report already cited but
  had not read for this purpose: `tidal://{track|album|artist}/{numeric id}` and
  `tidal://{playlist|mix}/{string id}`, implemented identically in
  `ref:high-tide/src/lib/utils.py` (`open_tidal_uri`, a `match` over
  artist/album/track/mix/playlist) and `ref:sone/src/lib/tidalUrl.ts` (`parseTidalUrl`, an
  identical `switch`). There is at least one collection-link form too:
  `tidal://my-collection/tracks`, handled in `ref:sone/src/utils/itemHelpers.ts` and its `Home.tsx`.
  The desktop app registers the protocol handler via `settings.openLinksInDesktopApp` /
  `settings/SET_OPEN_LINKS_IN_DESKTOP_APP` and `launchHandler/LAUNCH`; Strawberry uses redirect URI
  `tidal://login/auth` for OAuth (ref:strawberry/src/tidal/tidalservice.cpp) — that remains the only
  attested *auth* use of the scheme, distinct from the entity-navigation grammar above.
  [verified-source]
- Share context menus per entity: `ALBUM_SHARE`, `ARTIST_SHARE`, `PLAYLIST_SHARE`, `MIX_SHARE`,
  `USER_SHARE`, `CONTRIBUTOR_SHARE`. [verified-source]
- Embeds: TIDAL publishes an official embed widget product (developer.tidal.com embeds). Not
  read directly. [verified-web]

### 8. Web player (listen.tidal.com) — differences and limits

| Aspect | Desktop app | Web player |
| --- | --- | --- |
| Max quality | Max (HiRes FLAC 24/192) with exclusive output | Lossless reliably; HiRes availability browser-dependent **[uncertain]** |
| DRM | Widevine via Electron (castlabs build for wrappers) | EME: Widevine (Chrome/Edge/Firefox) or FairPlay (Safari) |
| Exclusive/bit-perfect output | Yes | **No** — browser audio only |
| Sample rate | Matches source | Chromium resamples everything to 48 kHz unless launched with `--audio-output-sample-rate=192000` (ref:tidal-hifi/docs/audio-quality.md) |
| Offline | No (neither has it) | No |
| Tray, autostart, close-to-tray | Yes (`settings.desktop`) | No |
| Protocol handler | Yes (`openLinksInDesktopApp`) | No |
| Everything else (browse, search, playlists, queue, lyrics, Connect controller) | Same Redux app | Same Redux app |

Practical note for streamboat: because the web player and the desktop app share the Redux store
shape (tidal-hifi's `ReduxController` reads `playbackControls.playbackContext.actualAudioQuality`,
`bitDepth`, `sampleRate`, `codec` from the *web* player and gets identical fields to the desktop
client's), feature parity between the two is near-total apart from the OS-integration column.
[verified-source]

tidal-hifi also documents that **Widevine DRM is broken on Windows for the castlabs Electron build
(error S6007)** with no workaround, and that "Tidal is working on removing/changing DRM"
(ref:tidal-hifi/docs/known-issues.md) — one more reason the manifest-based unofficial path is more
robust for a native client than embedding the web player. [verified-source]

### 9. What the mobile apps add (FUTURE scope — concise)

[verified-web unless noted]

- **Offline downloads**: albums, playlists, tracks; **5 devices offline simultaneously (1 online)**
  — not 3, corrected throughout this report from support.tidal.com/hc/en-us/articles/201623252;
  separate Download quality setting (Low/High/Max); `offlineGracePeriod` on the subscription;
  per-device counters `numberOfOfflineAlbums`/`numberOfOfflinePlaylists` [verified-source]; deleted
  MQA downloads had to be manually re-downloaded as FLAC after 24 Jul 2024.
- **Separate Mobile-data vs Wi-Fi streaming quality** settings.
- **Dolby Atmos playback** (not available on desktop).
- **CarPlay**: full app projection; car volume controls; offline mode plays downloaded My
  Collection content only. **[verified-web, unfetched support article]**
- **Android Auto**: same, plus voice search ("Play X on Tidal"), shuffle/repeat/track-radio on Now
  Playing. **[verified-web, unfetched support article]**
- **Android Automotive**: no downloads, no offline mode, no quality badge on Now Playing, but
  streaming quality is settable. **[verified-web, unfetched support article]**
- **Casting**: Chromecast, AirPlay, and TIDAL Connect device picker.
- **Apple Watch / Wear**: playback control and (Watch) offline listening.
- **Sleep timer / car mode**: could not confirm on official TIDAL documentation. **[uncertain]**
- **Widgets, Siri/Google Assistant, share-to-social with generated images** (`trackPrompts/
  GENERATE_SHARE_IMAGES` exists in the desktop client too). [verified-source]

### 10. TV and Connect apps — controller vs target roles

- **TIDAL Connect target**: a hardware streamer/DAC/speaker (or the TIDAL app on a TV) advertises
  itself over mDNS; the user's phone/desktop app acts as the *controller* and hands off; the target
  pulls the stream directly from TIDAL's CDN. This means the controller's local DSP (e.g. crossfade)
  does not apply to the target. Not every Connect target supports Autoplay.
- **Chromecast**: same controller/target split, Google's protocol; the client has a full
  `chromeCast/*` namespace including `UPSTREAM_QUEUE_CHANGED` and `REPEAT_MODE_CHANGED`.
  [verified-source]
- **Building a Connect target is not open-source-reproducible.** The only working Linux
  implementation (ref:tidal-connect) wraps a **proprietary** `tidal_connect_application` binary
  authenticated with a vendor device certificate (default: `IfiAudio_ZenStream.dat`), announced via
  Avahi/mDNS, with ALSA output. It is a Docker wrapper around a closed binary, MIT-licensed itself
  but useless without the binary. **Quality nuance, corrected**: this is not simply "capped at
  LOSSLESS since July 2024" — `ref:tidal-connect/README.md:59` says the implementation "could play
  hi-res files only up to 24/48 and MQA content" historically, with in-app MQA unfolding to
  24/88–24/96; after the 24 July 2024 MQA removal (§Notable changes below), HiRes FLAC is simply
  unavailable to it, so the accurate framing is "historically 24/48 plus MQA; effectively
  LOSSLESS-only since MQA's removal", not a deliberate LOSSLESS cap. [verified-source]
- **TV apps** (Apple TV, Fire TV, Android TV, smart TVs, consoles) are Dolby Atmos targets and use
  device-code login (`auth/DEVICE_AUTH_CODE_RESPONSE_RECEIVED`, `auth/DEVICE_AUTH_CODE_EXPIRED`
  exist even in the desktop client). [verified-source + verified-web]

### 11. Notable feature additions and removals, 2024–2026

| When | Change | Evidence |
| --- | --- | --- |
| 6 Mar 2024 (announced) → 10 Apr 2024 (effective) | Tier consolidation: HiFi and HiFi Plus merged into one subscription; HiRes FLAC for all payers; Student dropped to $4.99; **Free ad-supported tier discontinued**; Military/First-Responder discounts ended (10 Jun 2024) | [verified-web] |
| 17 Jun 2024 (announced) → 24 Jul 2024 (effective) | **MQA removed**; **Sony 360 Reality Audio removed**; **all podcasts removed** (same change); catalogue re-served as FLAC; offline MQA downloads had to be re-downloaded | [verified-web] (ecoustics.com/news/tidal-drops-mqa-360ra-podcasts/ for the podcast removal date); legacy enums still present [verified-source] |
| 2024→2026 | HiRes FLAC (24/192) becomes the "Max" tier name; Low/High/Max replaces Normal/High/HiFi/Master | [verified-web + verified-source] |
| ~2024–2025 | Public profiles, Picks/prompts, Feed, blocking, user-profile search results mature | [verified-source] |
| 2025 (some dates 2025-09/2025-11 in schema examples) | A social/creator API layer ships: comments (incl. time-anchored track comments), reactions, appreciations, artist claims, purchases, collaborative-playlist invites — see §4.13, §4.8 | [verified-source, from the v2 OpenAPI spec] |
| 2025 | AI playlist creation (`folders/CREATE_AI_PLAYLIST`) present in the desktop client; no press confirmation found | [verified-source], product framing **[uncertain]** |
| Nov 2025 | **TIDAL Upload** launches (creator uploads, Spotlight, $100k Upload Headliners contest); `Uploads` sidebar item and `UPLOADS` search category appear | [verified-web + verified-source] |
| 2025–2026 | Desktop/web **UI redesign** rolls out; two UIs coexist long enough that wrappers must detect which is live | [verified-source] |
| Uncertain, possibly during 2026 | DJ Extension pricing reportedly changed and stems were withdrawn/reinstated (title of digitaldjtips.com/big-price-increase-for-djs-using-tidal/) | **[uncertain]** — could not fetch; re-check before publishing a DJ Extension price |
| Aug 2026 | Price increase: Individual $10.99→$11.99, Family $16.99→$19.99, Student $5.49→$6.99 (US), effective first billing date on/after 3 Aug 2026 | [verified-web] |
| Mar–Jun 2026 | **Crossfade reintroduced** on iOS and Web (0–12 s slider, Settings toggle) — officially announced by TIDAL itself, not just third-party coverage; see §Summary item 8. Desktop-client status still open. | [verified-web]; desktop status **[uncertain]** |
| Ongoing | Dolby Atmos remains mobile/TV/car only; **not on desktop** | [verified-web] |
| Unclear | TIDAL **Live** (DJ sessions, launched Apr 2023) has no trace in the 2026 desktop client | **[uncertain]** |

---

## Feature matrix

Priority tiers: **MVP** = required for a first usable release; **v1** = required before calling it a
TIDAL client; **later** = post-1.0; **out-of-scope** = will not build (with reason).

Reference-client abbreviations: HT = High Tide, SO = Sone/sone-windows, TH = tidal-hifi,
TL = TidaLuna, MO = mopidy-tidal, ST = Strawberry, TT = tidalt, CL = tidal-cli, PT = python-tidal
(library-level support). "TH/TL implement it" is weak evidence for API reachability — they ride
inside the official client — so the API column flags that.

### Authentication and account

| Feature | Where in native app | Tier | Rationale | API availability / who implements | Source |
| --- | --- | --- | --- | --- | --- |
| OAuth device-code login (enter code at link.tidal.com) | All clients; TV/CLI-friendly | **MVP** | Nothing plays without a session; the only flow that works for headless/CLI | Fully available; PT, SO, TT, MO, tidalrs, libopenTIDAL | ref:python-tidal/tidalapi/session.py |
| OAuth PKCE login (browser redirect) — required for HI_RES_LOSSLESS | Desktop/mobile | **MVP** | Required to unlock the top quality tier; desktop/GUI mode needs it from day one | Available; PT, HT, MO, ST, tidal-cli, SDKs | ref:python-tidal/tidalapi/session.py; ref:high-tide |
| Token refresh + secure token storage (keyring/libsecret/Keychain/DPAPI) | All | **MVP** | Without it every restart is a re-login; plaintext token storage is a real credential-leak risk | HT uses libsecret; SO uses OS keyring + AES-256-GCM file; TT uses keychain with age fallback | ref:high-tide/src/lib/secret_storage.py; ref:sone/README.md; ref:tidalt |
| `GET sessions` → sessionId, userId, countryCode (mandatory on later calls) | All | **MVP** | `countryCode` gates catalogue availability on every subsequent call | Available; all clients | ref:python-tidal/tidalapi/session.py |
| Subscription/entitlement read (`users/{id}/subscription`) — gate UI on `highestSoundQuality` | Settings/account | **v1** | Needed to show the right quality ceiling and avoid requesting tiers the account can't get | Available; PT | ref:python-tidal/tidalapi/user.py |
| Multiple accounts / account switching | Not in native app | out-of-scope | Not a parity gap — the native app doesn't do it either | — | absence in Redux namespace |
| Sign-up, payment, plan management | Native app links out to web | out-of-scope | TIDAL itself punts this to a web page; no reason to build it natively | Web-only | ref:.../store/index.ts `settings.urls` |
| Facebook / Snapchat / TikTok linking | Profile settings | out-of-scope | Social-graph plumbing with no listening value; adds three OAuth integrations for a profile-picture picker | Social-graph plumbing with no listening value | ref:.../actionTypes.ts `user/*` |
| Streaming-privileges enforcement (one stream at a time; handle revocation) | All | **v1** | Server enforces this regardless of what streamboat does — an unhandled revocation looks like a crash/hang, not a clean stop | Server-enforced; must *handle* the error even if not subscribing to the socket. No OSS client implements the socket | ref:.../actionTypes.ts `player/STREAMING_PRIVILEGES_REVOKED`; ref:tidal-sdk-android/player/streaming-privileges |

### Playback

| Feature | Where in native app | Tier | Rationale | API availability / who implements | Source |
| --- | --- | --- | --- | --- | --- |
| Play / pause / next / previous / stop | Player bar | **MVP** | The whole point of the app | Local | ref:.../actionTypes.ts `playbackControls/*` |
| Seek (absolute, relative ±) | Progress bar | **MVP** | Table stakes for any player | Local | `playbackControls/SEEK`, `SEEK_FORWARDS`, `SEEK_BACKWARDS` |
| Volume, mute, unmute-to-previous | Player bar | **MVP** | Table stakes | Local | `playbackControls/SET_VOLUME`, `TOGGLE_MUTE`, `volumeUnmute` |
| Manifest fetch + quality cascade | Internal | **MVP** | Nothing plays without this; retrofitting terminal-vs-transient error handling later is painful. **Correction: the exact ladder shown in an earlier draft here, `HI_RES_LOSSLESS→LOSSLESS→HIGH→LOW`, is not Sone's** — Sone's shipped `ORDER` is `[HI_RES_LOSSLESS, HI_RES, LOSSLESS, HIGH]` (`ref:sone/src-tauri/src/commands/playback.rs:33`), no `LOW`; the `→LOW` form matches tidalt's docs instead. Whether to keep the retired `HI_RES` rung and whether `LOW`/`HIGH` are in scope at all are open questions — don't print a single ladder as settled MVP scope. | `playbackinfopostpaywall`; SO, HT, ST, TT, tidalrs | ref:sone/src-tauri/src/commands/playback.rs; ref:python-tidal/tidalapi/media.py |
| DASH (MPD) and BTS manifest handling | Internal | **MVP** | Both manifest types occur in normal use; missing one means some tracks silently fail | PT, HT, SO, MO, ST | ref:python-tidal/tidalapi/media.py |
| Detect encrypted manifests and refuse cleanly | Internal | **MVP** | Legal posture, not just a feature — document it so nobody files "add Widevine support" | ST does exactly this | ref:strawberry/src/tidal/tidalstreamurlrequest.cpp |
| Shuffle (seeded, reversible) | Player bar | **MVP** | Users notice immediately when shuffle is destructive or unseeded | Local | ref:.../store/PlayQueue.ts `lastShuffleSeed` |
| Repeat off / all / one | Player bar | **MVP** | Table stakes | Local | `RepeatMode {Off=0,All=1,One=2}` |
| Queue view: reorder, remove, clear, play-next vs add-to-queue | Right sidebar | **MVP** | Play-next vs add-to-queue as distinct insert positions is one of the first things users test | Local | `playQueue/ADD_NEXT` vs `ADD_LAST`, `MOVE_TRACK`, `REMOVE_AT_INDEX`, `CLEAR_QUEUE` |
| Queue source attribution ("Playing from …") | Player bar | **v1** | Small, cheap, and expected once the queue exists | Local | `playQueue.sourceName`, `sourceUrl` |
| Lazy queue filling for huge lists | Internal | **v1** | Without it, playing a 10,000-track collection stalls on load | Local | `playQueue/FETCH_REST_OF_THE_TRACKS_AND_ADD_TO_QUEUE` |
| Queue persistence across restarts | Implicit | **v1** | Expected baseline behaviour once a queue exists | Local; SO advertises it | ref:sone/README.md |
| Gapless playback (preload next) | Internal | **MVP** | A native client's core reason to exist — the web player structurally cannot always guarantee this | GStreamer `playbin3` about-to-finish, needs GStreamer ≥1.24 (HT) or `concat`, no version floor (SO) — **correction: the 1.24 floor applies only to the `playbin3`/`about-to-finish` design; Sone's own `gapless_supported()` is just `gst::ElementFactory::find("concat").is_some()`, and `concat` has shipped since long before 1.24 — Sone's README claims a 1.24 floor, contradicted by its own code** | ref:high-tide; ref:sone/src-tauri/src/audio.rs:3302-3309 |
| Autoplay / continuation when queue ends | Setting + queue | **v1** | Visible UX parity item; depends on catalogue recommendation endpoints, so not MVP | `content/LOAD_SUGGESTIONS`; SO implements | ref:sone/README.md |
| Loudness normalization NONE/ALBUM/TRACK (ReplayGain from manifest) | Settings | **v1** | A checkbox implementation is a known-wrong shape (three states, not two) | Gain + peak in playbackinfo; SO, HT implement | ref:.../store/index.ts; ref:sone/README.md |
| Bit-perfect / exclusive output (WASAPI exclusive, ALSA hw:; macOS mechanism **[inferred]**) | Sound output menu | **v1** | The reason a native client exists at all — the web player cannot do this | Local; SO (ALSA + WASAPI), TT (ALSA hw:) implement | ref:.../store/index.ts `PlayerDeviceMode`; ref:sone; ref:tidalt |
| Audio device enumeration + selection | Sound output menu | **v1** | Required before exclusive output means anything | Local | `player.availableDevices`, `SELECT_SOUND_OUTPUT` |
| Force Volume per device — **pins the app's own volume at 100% so an external DAC/amp is the sole volume control** (corrected: this is the opposite of a software-volume fallback) | Sound output menu | later | Cheap once exclusive-mode plumbing exists; not needed for a usable v1 | Local | `player.forceVolume`, `SET_FORCE_VOLUME` |
| Signal-path transparency (show every conversion) | **Not in native app** | later (differentiator) | Nice audiophile-facing extra, no schema risk, doesn't block anything else | Local; SO implements | ref:sone/README.md |
| Crossfade | **Confirmed shipping on iOS/Web in 2026 — this is parity work, not a differentiator; re-tier once desktop status is confirmed** | later (pending desktop confirmation) | Purely local DSP if built; low risk either way, but should not be dismissed as "not a TIDAL feature" | Purely local DSP if built | [verified-web] — see §Summary item 8 |
| Equalizer | Not in native app | out-of-scope | Not a parity gap — the native app has none either | Native app has none | absence in Redux namespace |
| Audio spectrum visualiser | Settings toggle | later | Cosmetic, no schema risk | Local | `settings.audioSpectrumEnabled` |
| Video playback (music videos, HLS) | Video pages | later | Separate HLS pipeline for a scope the owner hasn't committed to (§Open questions) | `videos/{id}/urlpostpaywall`; SO implements with hls.js | ref:sone/README.md; ref:python-tidal/tidalapi/media.py |
| Dolby Atmos playback | Mobile/TV only | out-of-scope | Native desktop does not do it; EAC3-JOC decode + renderer is a large lift for no parity gain | Native desktop does not do it | [verified-web] |
| Play reporting (`play_log` → `ec.tidal.com/api/event-batch`) so Recently Played works | Invisible | **v1** | Without it the user's own TIDAL account degrades (dead Recently Played, stale Daily Discovery) from using streamboat — bigger than any missing screen | SO implements; no other OSS client does | ref:sone/src-tauri/src/tidal_report/event.rs |

### Browse and content

| Feature | Where in native app | Tier | Rationale | API availability / who implements | Source |
| --- | --- | --- | --- | --- | --- |
| Home page (dynamic modules, incl. tabs and cursor pagination — §4.2) | Music tab | **v1** | The first screen the user sees; a defensive module renderer here pays off across every other page | `pages/home` and/or `home/feed/{slug}`; HT, SO, MO | ref:python-tidal/tidalapi/page.py |
| For You page | Music tab | **v1** | Distinct endpoint from Home; expected on parity but not blocking | `pages/for_you`; MO | ref:mopidy-tidal/mopidy_tidal/library.py |
| Explore: genres, moods, charts, TIDAL Rising, new releases | Explore tab | **v1** | Second most-used nav destination after Home | `pages/explore`, `pages/moods`, `pages/genre_page`, `pages/hires`, `pages/videos`; HT, MO | ref:high-tide/src/pages/explore_page.py |
| Editorial articles inline | Explore | later | Content type with no playback value; safe to defer | `content.articles` exists in client; python-tidal has no article model | ref:.../store/content/Article.ts |
| Search: top hit + tracks/videos/artists/albums/playlists | Search field | **MVP** | Users expect to find things immediately; the app is unusable without it | `GET search?types=…` (≤300 results); PT, HT, SO, MO, CL | ref:python-tidal/tidalapi/session.py |
| Search: uploads and user-profile result types | Search filters | later | Small result-type gap, low value until Uploads/social scope is decided | **Not in python-tidal** — needs direct API work | ref:.../store/index.ts `searchResultFilterOrder` |
| Recent searches, suggestions, "did you mean" | Search popover | later | **Re-tiered: these are server-side/cross-device endpoints (`/searchSuggestions`, `/searchHistoryEntries`), not unreachable as first drafted** — still "later" because no OSS client has exercised them yet, but the work is "call a documented endpoint," not "reverse-engineer one" | `/searchResults`, `/searchSuggestions`, `/searchHistoryEntries/{id}` documented in the v2 spec; no OSS client implements them yet | ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json |
| Artist page: top tracks, albums, EP & singles, appears on, similar, bio, videos, radio, follow, share | Artist route | **v1** | One of the most-visited page types; all endpoints already available | All endpoints available; HT implements everything except the Videos row (cite TidaLuna/python-tidal for that one, §4.5) | ref:high-tide/src/pages/artist_page.py; ref:python-tidal/tidalapi/artist.py |
| Contributor/credits page (songwriter, producer, engineer …) | Contributor route | later | Deep-cut feature with real value for credits nerds but low traffic | `content/LOAD_DYNAMIC_CONTRIBUTOR_PAGE`; not in any OSS client | ref:.../store/content/Artist.ts |
| Album page: multi-volume tracks, credits, review, similar, UPC/label/date, quality badges | Album route | **v1** | Second most-visited page type after artist/home | `albums/{id}/items`, `/review`, `/similar`, `pages/album`; HT, SO | ref:.../store/content/Album.ts |
| Track credits panel | Now Playing / album | **v1** | Visible, cheap once the album page exists | `content/LOAD_ITEM_CONTRIBUTORS`; not in PT high-level API | ref:.../actionTypes.ts |
| Lyrics — plain and time-synced (`subtitles`), RTL aware | Now Playing | **v1** | High visible value per unit of effort; both HT and SO ship synced lyrics | `tracks/{id}/lyrics`; HT, SO implement synced lyrics | ref:.../store/Lyrics.ts; ref:high-tide/src/widgets/lyrics_widget.py |
| Track radio / artist radio / album mix | Context menus | **v1** | Expected context-menu action, cheap once the underlying page exists | `tracks/{id}/radio`, `artists/{id}/radio`, `*/mix`; HT, SO | ref:python-tidal/tidalapi/{media,artist}.py |
| Mixes: My Mix N, Daily Discovery, New Arrivals, Video Mix, history mixes | Home + Collection | **v1** | User-visible personalisation the owner has not asked to cut | `pages/mix`, `pages/my_collection_my_mixes`; HT, SO, MO | ref:python-tidal/tidalapi/mix.py |
| My Collection: Tracks/Albums/Artists/Playlists/Videos/Mixes with sort + direction | Collection group | **MVP** (tracks/albums/artists/playlists), **v1** (videos/mixes) | A user's own library is core to "using TIDAL"; video/mix collections depend on scope decisions | `users/{id}/favorites/*`; PT, HT, SO, MO | ref:python-tidal/tidalapi/{user,types}.py |
| Favourite / unfavourite anything (incl. favouriting users) | Everywhere | **MVP** | The single most-used library-mutation action in any music app | `users/{id}/favorites/*` add/remove; `favorites/mixes/{add,remove}` for mixes | ref:python-tidal/tidalapi/user.py |
| Recently played | Home | **v1** | Depends on play reporting (Playback table) being implemented first | `content/LOAD_RECENT_ACTIVITY`; depends on play reporting | ref:sone/README.md |
| Charts | Explore | later | A page category (confirmed, not inferred — see §4.3), reachable once Explore renders modules | Reachable as an Explore `PageLink`, no dedicated endpoint | ref:python-tidal/docs/pages.rst |
| Podcasts | **Discontinued 24 Jul 2024 — [uncertain] whether any trace survives as an editorial page link** | out-of-scope | TIDAL's own removal announcement is dispositive for building podcast playback; the residual doctest category is a documentation-staleness question, not a design question | Removed 24 Jul 2024 (ecoustics.com/news/tidal-drops-mqa-360ra-podcasts/); a "Podcasts…" category still appears in python-tidal's page-module doctest output (ref:python-tidal/docs/pages.rst) — likely a stale fixture, re-check `pages/explore` on a live account before relying on either claim | [verified-web] removal; **[uncertain]** residual category |

### Library management

| Feature | Where in native app | Tier | Rationale | API availability / who implements | Source |
| --- | --- | --- | --- | --- | --- |
| Create / rename / delete playlist | Sidebar, context menus | **v1** | Core library-management action | `playlists`, `my-collection/playlists/folders/create-playlist`; PT, HT, SO | ref:python-tidal/tidalapi/playlist.py |
| Add / remove playlist items | Context menus | **v1** | Core library-management action | `playlists/{uuid}/items`; PT | ref:python-tidal/tidalapi/playlist.py |
| Reorder playlist items (with ETag concurrency) | Playlist page drag | **v1** | Skipping ETag guards risks silently clobbering a concurrent edit from another device | `playlists/{uuid}/items/{index}` move; PT | ref:python-tidal/tidalapi/playlist.py |
| Edit playlist title/description/cover | Edit modal | **v1** | Expected once create/delete exists | PT supports metadata edit; custom cover upload **[uncertain]** | ref:python-tidal/tidalapi/playlist.py |
| Visibility: **three-state PUBLIC / UNLISTED / PRIVATE**, not a binary toggle (§4.8) | Playlist menu | **v1** | Modelling this as a checkbox silently turns UNLISTED playlists into PUBLIC or PRIVATE — get the enum right from the start | Legacy binary: `/set-public`, `/set-private`, `user-playlists/{id}/public` (PT); v2 three-state: `Playlists_Attributes.accessType` | ref:python-tidal/tidalapi/playlist.py; ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json |
| Playlist folders (create, rename, move, remove; read at scale via cursor pagination — §4.8) | Sidebar + its own `FOLDER` route | **v1** | Common for users with many playlists; the folder screen is a real route, not just a sidebar tree | `my-collection/playlists/folders/*` (create/list in `user.py`, rename/move in `playlist.py`); PT, SO | ref:python-tidal/tidalapi/{user,playlist}.py |
| Collaborative playlists (invite/redeem) | Not in desktop client; confirmed in the v2 API (§4.8, §Summary item 10) | later | Real feature, but no OSS client has implemented it and the desktop UI itself may not expose it yet | `/collaborationInvites`, `/collaborationInviteRedemptions`, `playlists/{id}/relationships/collaborators`; no OSS client | ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json |
| Suggested tracks for a playlist | Playlist page footer | later | Recommendation-quality feature, not core library management | `content/LOAD_PLAYLIST_SUGGESTED_MEDIA_ITEMS`; not in PT | ref:.../actionTypes.ts |
| AI playlist creation | Create menu | out-of-scope (for now) | Endpoint and rollout state are both unconfirmed (§Open questions); revisit once documented | `folders/CREATE_AI_PLAYLIST`; no OSS client; endpoint unknown | ref:.../actionTypes.ts |
| Block track / artist (+ blocked-items page) | Context menu | later | Permanently reshapes account-wide recommendations — a write with real consequences, worth building deliberately rather than early | `blocks/*`; no OSS client implements | ref:.../actionTypes.ts |
| Multi-select operations on track lists | Track lists | **v1** | Expected once any track-list UI exists | Local | `selection/*`, `MULTI_MEDIA_ITEM` menu |
| TIDAL Upload (upload your own tracks) | Uploads tab | out-of-scope | Creator tooling, not a player feature | Would need undocumented upload endpoints | ref:.../actionTypes.ts `creatorContent/*` |
| Play uploaded content that others shared with you | Uploads tab | later | Playback-only consumption of a feature streamboat won't produce content for | `upload` flag on items; cover URLs are absolute | ref:tidal-hifi/src/features/tidal/url.ts |
| Save for Later (a 7th collection type, distinct from the six in §4.10) | Not implemented by any OSS client | later | Real collection type in the v2 model; low priority until the core six lists are solid | `/userCollectionSaveForLaters/{id}`; no OSS client | ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json |

### Remote playback and integrations

| Feature | Where in native app | Tier | Rationale | API availability / who implements | Source |
| --- | --- | --- | --- | --- | --- |
| Be controlled by MPRIS (Linux) / SMTC (Windows) / Now Playing (macOS) | Not TIDAL's — OS layer | **MVP** | Expected baseline OS integration on every desktop platform | **Corrected: no single crate unifies all three.** sone-windows uses the `souvlaki` crate (`ref:sone-windows/src-tauri/Cargo.toml:64`, not its README, which never mentions it) for **Windows SMTC only**; Sone on Linux uses `mpris-server` instead (`ref:sone/src-tauri/Cargo.toml:66`); High Tide implements MPRIS itself via Python D-Bus (`ref:high-tide/src/mpris.py`); tidal-hifi (Node) has its own MPRIS service. Budget for three separate integrations, or `souvlaki` for Windows+macOS plus a Linux MPRIS library, not one crate for everything. | ref:sone-windows/src-tauri/Cargo.toml; ref:sone/src-tauri/Cargo.toml; ref:high-tide/src/mpris.py |
| Hardware media keys | OS | **MVP** | Comes largely free once OS media-control integration exists | Via the media-controls integration above | ref:tidal-hifi/src/constants/mediaKeys.ts |
| Desktop notifications on track change | Wrapper feature | later | Nice-to-have, no schema risk | Local | ref:tidal-hifi/README.md |
| System tray + minimise/close to tray + autostart | Desktop settings | **v1** | Expected desktop-app behaviour on Windows/macOS/Linux | `settings.desktop.closeToTray`, `autoStartMode`; SO, TH | ref:.../store/index.ts |
| TIDAL Connect **controller** (discover + hand off to speakers) | Device picker | later | Real feature owners of TIDAL Connect hardware expect, but the control protocol is undocumented and unimplemented anywhere — real research cost | mDNS discovery documented; cloud-queue mechanism now confirmed documented (`/playQueues`, see below); the device *control* protocol itself remains undocumented and **no OSS client implements it** | ref:.../store/RemotePlayback.ts |
| TIDAL Connect **target** (be a Connect endpoint) | Third-party hardware | out-of-scope | Requires a proprietary binary + vendor certificate — not reproducible without it | Requires a proprietary binary + vendor certificate | ref:tidal-connect/bin/entrypoint.sh |
| Chromecast | Device picker | later | Real feature, moderate research cost, no OSS precedent | `chromeCast/*` in native; no OSS client | ref:.../actionTypes.ts |
| AirPlay (macOS) | OS audio device | later | Comes free via CoreAudio device selection — low cost, why not | Comes free via CoreAudio device selection | **[inferred]** |
| Cloud queue / continue listening across devices | Invisible | later | **Re-tiered: the mechanism is documented (`/playQueues`, `PlayQueues_Attributes`), just not implemented by any OSS client — this is "call a documented endpoint," not "reverse-engineer an undocumented one."** Still later because it needs the local queue model solid first, and because its repeat/shuffle enum differs from the desktop client's (§Summary item 12) | `cloudQueue/*` client-side; `/playQueues`/`/playQueues/{id}` server-side (v2 spec); no OSS client implements it yet | ref:.../store/PlayQueue.ts; ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json |
| Last.fm scrobbling | Built into TIDAL | **v1** | TIDAL itself treats this as a headline feature (own onboarding step, own route) | Native has `lastFm/*`; SO adds Last.fm + Libre.fm + ListenBrainz | ref:.../actionTypes.ts; ref:sone/README.md |
| Discord Rich Presence | Not in native app | later (differentiator) | Cheap, popular with the OSS clients, no schema risk | SO, TH, HT | ref:sone/README.md |
| Local HTTP control API / MCP / OBS overlay | Not in native app | later (differentiator, but natural for headless mode) | Directly serves the owner's headless/CLI requirement; define the contract before writing either front end | TH exposes `/player/*` REST; SO exposes MCP on 5577 and OBS overlay on 5578 | ref:tidal-hifi/src/features/api/swagger.json; ref:sone/README.md |
| Proxy support (HTTP/HTTPS/SOCKS5) | Not in native app | later | Niche but cheap; no schema risk | SO | ref:sone/README.md |

### UI, settings and platform

| Feature | Where in native app | Tier | Rationale | API availability / who implements | Source |
| --- | --- | --- | --- | --- | --- |
| Sidebar nav: Home, Explore, Feed, Uploads, Collection groups (28 screens total — §4.1) | Sidebar | **v1** (Home/Explore/Collection), later (Feed/Uploads) | Music/Explore/Collection are the primary navigation loop; Feed/Uploads depend on the owner's social/creator scope decisions | Local | ref:tidal-hifi/src/TidalControllers/DomController/constants.ts |
| Now Playing full screen with art, lyrics, credits | Player | **v1** | High-visibility screen, cheap once the underlying data (lyrics, credits) exists | Local | `view/ENTER_NOWPLAYING` |
| Native fullscreen | Player | later | Nice-to-have window-management feature | Local | `view/ENTER_NATIVE_FULLSCREEN` |
| Mini player (floating) | **Not in native app** | later (differentiator) | Cheap, popular with OSS clients, no schema risk | SO, sone-windows | ref:sone/README.md |
| Keyboard shortcuts + in-app cheatsheet | Whole app | **v1** | Expected desktop-app affordance; match TIDAL's *reported* bindings loosely, not as gospel (§5.6 — the published list is one wrapper's copy of one crowd-sourced aggregator, not TIDAL's own documentation) | Local; match TIDAL's reported bindings where sensible | ref:tidal-hifi/src/features/hotkeys/hotkeyConfig.ts |
| Streaming-quality selector | Settings + player menu | **MVP** | Users choosing quality is core to why they pay for TIDAL | `settings/SET_STREAMING_QUALITY` | ref:.../store/index.ts |
| Explicit-content filter | Settings | **v1** | Client-side only in practice (owner decision — §Open questions), but cheap once the flag is read | `explicit` flag on every item; filter client-side | ref:.../store/index.ts |
| Language / localisation | Settings | later | Nice-to-have, large surface area, no functional blocker | Client ships i18n bundles (`locale.bundles`) | ref:.../store/index.ts |
| Deep links: `https://tidal.com/browse/...` and `tidal://` | Protocol handler | **v1** (open the URL scheme), later (register the OS-level handler) | **The `tidal://` grammar is now confirmed (§7), removing the excuse to defer this** — `tidal://{track,album,artist}/{numeric id}` and `tidal://{playlist,mix}/{string id}` are implemented identically in two reference clients | HT, SO implement `tidal://`; grammar confirmed in both | ref:high-tide/src/lib/utils.py; ref:sone/src/lib/tidalUrl.ts |
| Universal share links (`?u`) and cross-DSP resolution | Share menus | **v1** (produce the link), later (resolve incoming cross-service links via `/dspSharingLinks`) | **Correction: producing a `?u` link is a trivial string append, but the feature is not** — `/dspSharingLinks` (`{spotify, appleMusic, amazonMusic, youTubeMusic}`) and `/shares`/`/savedShares` model sharing as real server-side state, relevant if streamboat ever wants to *resolve* an incoming shared link | Producing: trivial. Resolving: `/dspSharingLinks`, `/shares`, `/savedShares` (v2 spec, no OSS client) | ref:tidal-hifi/src/features/tidal/url.ts; ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json |
| Theming / custom colours | **Not in native app** | later (differentiator) | Cheap, visible, no schema risk | SO (15 presets + picker), TH (themes) | ref:sone/README.md |
| Offline downloads / logged-in subscriber cache | Mobile only (native desktop has none) | out-of-scope for desktop-parity; a *logged-in subscriber cache* is a separate, owner-scoped design question (§Open questions) | TIDAL's own offline model (`/offlineTasks` STORE/REMOVE + `/installations/.../offlineInventory`) is a candidate shape if the owner wants a cache — mirroring it beats inventing one | No native desktop equivalent; v2 spec models `/offlineTasks`, `/downloads`, `/installations` | ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json |
| Ads / ad-supported playback | Vestigial plumbing after the free tier's 2024 removal | out-of-scope | No live ad-supported tier exists to build against | — | **[uncertain]** |
| Voice commands | Desktop `speech/*` | out-of-scope | Undocumented, likely Web Speech API, low value relative to cost | Undocumented | ref:.../actionTypes.ts |
| Live / DJ sessions | Status unclear | out-of-scope | Cannot safely build against a feature whose current existence is unconfirmed | — | **[uncertain]** |
| Feature-flag / experiment platform | Internal | out-of-scope | TIDAL-internal tooling with no user-facing equivalent to build | `experimentationPlatform/*`, `featureFlags/*` | ref:.../actionTypes.ts |
| Analytics/event tracking (`eventTracking/*`) | Invisible | out-of-scope **except** play_log | Only `play_log` has user-visible consequences (Recently Played, recommendations); the rest is TIDAL's own product telemetry, not streamboat's concern | Only `play_log` has user-visible consequences | ref:.../actionTypes.ts |
| Comments, reactions, appreciations, artist claims, purchases (§4.13) | Not in desktop client; documented in v2 API | out-of-scope (needs an explicit owner decision, not a default) | Real 2025–2026 features with moderation/privacy implications if rendered; the owner has not scoped social this far (§Open questions) | `/comments`, `/reactions`, `/appreciations`, `/artistClaims`, `/purchases`; no OSS client | ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json |

---

## Implications for streamboat

1. **Model the domain on TIDAL's own model, not on a generic music-player model.** Copy the type
   names and enum values from `ref:TidaLuna/plugins/lib/src/redux/types/store/` — `AudioQuality`,
   `AudioMode`, `MediaMetadataTag`, `RepeatMode`, `PlayQueueSourceType`, `StreamingFlags`,
   `MixType`, `ArtistRoleCategory`. Cheap now, and it makes every API response map 1:1.
2. **Guard against the `HIGH` vs "High" trap.** UI label "High" is enum `LOSSLESS`; enum `HIGH` is
   lossy AAC 320. Encode the ladder once, in one table, with the format arrays from
   `audioQualityToFormats`.
3. **Build the playback core as a quality cascade with terminal-error classification** from day one
   (Sone's actual model: try HI_RES_LOSSLESS → HI_RES → LOSSLESS → HIGH, no `LOW` rung — correction,
   an earlier draft here printed `HI_RES_LOSSLESS → LOSSLESS → HIGH → LOW`, which is not Sone's
   shipped `ORDER`; treat playbackinfo sub-statuses
   **4005, 4010, 4030, 4031, 4032, 4034, 4035** — not a contiguous "4030–4035" range, 4033 is
   deliberately absent — as permanently unplayable and skip, retry only transient errors).
   Retrofitting this is painful.
4. **Refuse encrypted manifests explicitly and visibly**, as Strawberry does. This is both the legal
   posture and a clean failure mode. Document it in the README so nobody files "add Widevine
   support" issues.
5. **Implement play_log reporting early** (`ec.tidal.com/api/event-batch`, group `play_log`) and
   make it a settings toggle, defaulting on, exactly as Sone does. Without it, Recently Played, Home
   personalisation, Daily Discovery and New Arrivals all degrade — the user's *TIDAL account* gets
   worse from using streamboat, which is a much bigger deal than any missing screen.
6. **Design the browse layer as a renderer of server-driven page modules** (`pages/home`,
   `pages/for_you`, `pages/explore`, `pages/moods`, `pages/genre_page`, `pages/mix`, `pages/album`,
   `pages/artist`), not as hand-coded screens. TIDAL ships new module types without warning; a
   module renderer with a graceful unknown-module fallback ages far better. This also gives the
   headless/CLI mode a natural browse tree — mopidy-tidal's `tidal:home`, `tidal:explore`,
   `tidal:moods`, `tidal:genres`, `tidal:mixes`, `tidal:hires` is a proven mapping.
7. **The headless/CLI mode and the desktop UI must share a core with an explicit control surface.**
   tidal-hifi's REST shape (`/player/play|pause|playpause|next|previous|seek/absolute|seek/relative|
   volume|shuffle/toggle|repeat/toggle|favorite/toggle`, `/current`, `/current/audio-quality`) is a
   good minimal contract, and Sone's MCP server shows where that leads. Define this contract before
   writing either front end.
8. **Queue semantics are the highest-risk area for "feels wrong".** Seeded reversible shuffle,
   `priority_history`/`priority_keep`/`priority_none` element priorities, play-next vs add-to-queue
   as distinct insert positions, and lazy filling of huge sources are all things users notice
   immediately when they are missing.
9. **Exclusive/bit-perfect output is the reason a native client exists at all.** Both Sone
   (ALSA hw: + WASAPI exclusive) and the official desktop have it; the web player structurally
   cannot. Treat it as v1, with per-device mode memory (`desiredDeviceMode` keyed by device) and the
   documented mutual exclusion with force-volume.
10. **Normalization must be three-state (NONE/ALBUM/TRACK), not a checkbox**, using
    `albumReplayGain`/`trackReplayGain` and the peak values from the manifest response, with album
    context when playing an album and track context otherwise.
11. **Do not build a Connect target.** It is not reproducible without a proprietary binary and a
    vendor certificate — and the evidence that the desktop app itself may be able to act as a
    receiver is now stronger (a user-facing disconnect modal, §5.4), which raises the stakes on
    getting this decision right rather than changing it. A Connect *controller* has more documented
    surface than first thought (the cloud-queue mechanism is a real, spec'd resource, §Summary item
    12) but the device discovery/control protocol itself remains undocumented and unimplemented by
    anyone; treat it as research-later.
12. **Mobile is future scope but two decisions bind now**: the core must be usable from a non-desktop
    runtime (so: no GTK/Qt assumptions in the core, no desktop-only crypto for token storage), and
    the offline story must be designed as a *cache for a logged-in subscriber* — bounded, encrypted
    or at least non-portable, invalidated on logout — never as an exportable library.
13. **Lyrics need both forms.** `lyrics` (plain) and `subtitles` (timed) plus `isRightToLeft` — a
    synced-lyrics view is one of the highest visible-value features per unit of effort, and both
    High Tide and Sone ship it.
14. **Expect two TIDAL UIs and API drift.** tidal-hifi had to add runtime UI-version detection.
    streamboat does not touch the DOM, but the same instability applies to page-module shapes: parse
    defensively, log unknown shapes, never hard-fail a page because one module changed.
15. **Feature-parity scope reality check.** Of the 48 Redux namespaces in the official desktop
    client (TidaLuna 1.16.6-beta, 2026-09-02 — see the version caveat), roughly a dozen are
    user-visible features streamboat should match, another dozen or so are social/creator surfaces
    that are optional, and the rest are analytics, experiments, onboarding, session plumbing and
    modals. "Everything the native client does" is a smaller target than it first appears — the
    hard parts are audio output quality, queue fidelity, and page-module rendering.
16. **Use the vendored OpenAPI spec as a first-class source, not a curiosity.**
    `ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json` (TIDAL API 1.10.104, 256 paths, 993
    schemas) is a first-party, machine-readable document sitting on disk, not a web summary — it
    resolves several of this report's own open questions (collaborative playlists, playlist
    visibility, cloud queue, offline model, search history) and documents a whole 2025–2026 layer
    (comments, reactions, purchases, artist claims) this report's first draft missed entirely. Any
    future research pass on this codebase should start there before falling back to web search.
17. **Specify the error/failure surface as a parity target, not an afterthought.** The native
    client's own `Notification { category: MUTE|NETWORK|OTHER|PLAYBACK, severity }` model plus
    `message/MEDIA_NOT_PLAYABLE` and the `network/*` connectivity namespace (§6) is a ready-made
    spec for streamboat's toasts/error states. Third-party clients are most visibly worse than the
    native app exactly at failure moments (dead track, lost connection, revoked session) — copy the
    native app's categorisation rather than inventing streamboat's own.
18. **The route table, not the sidebar, is the actual screen inventory.** 28 `route/LOADER_DATA__*`
    screens exist, four of them (standalone Track, standalone Video, Folder, another user's public
    Profile) unreachable from the sidebar (§4.1, `references/browse-pages-screens.md`). Use the route
    table when scoping IA/routing work, not a DOM screenshot of the sidebar.
19. **Pick a UI-generation target deliberately, not by accident.** Two TIDAL UIs coexist through
    2025–2026 (tidal-hifi's `OLD_UI_OVERRIDES` vs `NEW_UI_OVERRIDES`, §1). streamboat renders its own
    UI and consumes the same underlying data either way, which means — unlike a DOM-scraping
    wrapper — it is genuinely free to choose its own information architecture. That freedom should
    be an explicit product decision ("simple but beautiful," per the owner's brief), not an accident
    of whichever TIDAL screenshots the implementer happened to have open. See §Open questions.
20. **Decide the write-scope policy as one tiered decision, not feature-by-feature.** Favouriting,
    playlist mutation, blocking, Picks, profile edits, play_log reporting and (if ever adopted)
    comments all *write* to the subscriber's real TIDAL account through an unofficial API — a bug in
    streamboat then damages the account, not just the local app. Recommended tiers: read-only →
    read + library writes (favourites/playlists/queue) → read + library + profile/recommendation
    writes (Block, Picks, AI playlists) → + reporting (play_log). Use an `Idempotency-Key` on every
    v2 mutation (the spec documents replay-safety within 1h) and respect playlist ETags — a stale
    write can clobber a concurrent edit from the user's phone.

---

## Open questions

**Only the owner can decide:**

1. **Social scope.** Feed, public profiles, followers, Picks/prompts, favouriting other users — a
   coherent slice of the native app, but large and unrelated to playing music well. The scope is
   now bigger than the first draft suggested: a 2025–2026 layer of comments (incl. time-anchored
   track comments), reactions, appreciations and artist claims exists too (§4.13). Include, defer,
   or never — and if included, at what depth?
2. **Video scope.** TIDAL has music videos (catalogue size and audio codec are **[uncertain]**, not
   confirmed at "650k+ / AAC ~320 kbps" — see §4.12), a Videos collection list, a Video Mix type and
   video search results. Videos need an entirely separate HLS pipeline. Include, defer, or never?
3. **Offline caching posture.** Native desktop has none. Owner has explicitly allowed *discussing*
   caching for a logged-in subscriber. TIDAL's own offline model (`/offlineTasks` STORE/REMOVE +
   `/installations/.../offlineInventory`, §6/§9) is now a concrete candidate shape to mirror. What is
   the intended shape — ephemeral read-through cache, pinned-for-offline with encryption-at-rest
   modelled on TIDAL's own store/remove tasks, or nothing at all?
4. **Play reporting default.** Reporting plays to TIDAL makes the user's account work properly but is
   telemetry to a third party. Sone defaults it on with a toggle. Same default for streamboat?
5. **Last.fm / ListenBrainz scrobbling** — the native client has Last.fm built in; the OSS clients
   add ListenBrainz and Libre.fm. In scope for v1?
6. **Which "not in the native app" differentiators to adopt**: floating mini-player, themes, signal
   path transparency, Discord RPC, local control API / MCP, OBS overlay, proxy support. Each is
   cheap individually and expensive collectively.
7. **Headless mode's control contract**: REST like tidal-hifi, MPD-compatible like mopidy, MPRIS-only,
   MCP, or several? This choice constrains the core API more than any UI decision.
8. **Explicit-content filtering** is client-side only in practice. Ship it in v1 or later?
9. **Write-scope policy** (Implication 20): how far does streamboat write to the user's real TIDAL
   account — read-only, + library writes, + profile/recommendation writes (Block, Picks, AI
   playlists), + play_log reporting? This is one cross-cutting decision, not eight independent
   feature calls, because a bug at the "writes" tier damages the subscriber's actual account.
10. **Which TIDAL UI generation streamboat targets for parity** (Implication 19): the new UI, the
    old UI, or streamboat's own design that merely consumes the same data (in keeping with "simple
    but beautiful")? streamboat doesn't touch the DOM, so unlike the wrapper clients it is free to
    choose — but every screen-layout decision in the feature matrix silently assumes an answer.

**Unverified / needs confirmation before it goes into a spec:**

11. **Crossfade on the desktop client specifically.** The feature's *existence* is now confirmed
    ([verified-web], iOS/Web, 2026) — this is no longer an open question. What remains open is
    whether the Electron desktop client has shipped it; the TidaLuna 1.16.6-beta dump (2026-09-02)
    has no trace of it, which is more likely a stale snapshot than evidence of absence.
12. **TIDAL Live / DJ sessions.** Launched April 2023; no trace in the 2026 desktop Redux namespace;
    no discontinuation announcement found. Present-but-mobile-only, or removed?
13. **Dolby Atmos on desktop.** Documentation says desktop is unsupported, and the client evidence is
    now stronger (only one action, `modal/SHOW_DOLBY_ATMOS`, plus an onboarding step, most likely an
    upsell modal) — confidence raised to ~0.85, but still not a source-confirmed absence.
14. **TIDAL Connect: can the desktop app be a target now?** TIDAL support said "coming soon"; the
    client evidence for a receiver role is now stronger than a namespace name (a dedicated
    `modal/REMOTE_PLAYBACK_RECEIVER_DISCONNECT_MODAL`), but this remains unresolved.
15. **AI playlist creation.** `folders/CREATE_AI_PLAYLIST` and `modal/SHOW_CREATE_AI_PLAYLIST` exist
    in the shipping client, but no press coverage or support article was found. Rollout state,
    market availability and endpoint are all unknown.
16. **Ads in 2026.** `adSupportedStreamReady`, `Video.adsUrl`, `playQueue/ADD_PROMO_ITEM_TO_QUEUE`
    and tidal-hifi's ad-blocking filters all exist after the free tier was withdrawn. Vestigial,
    pre-paywall preview content, or a market-specific ad-supported mode?
17. **Web player HiRes ceiling.** Sources conflict on whether listen.tidal.com serves 24/192 or caps
    at 16/44.1. Browser- and platform-dependent; needs a first-hand test.
18. **Custom playlist cover upload** via the unofficial API — python-tidal exposes metadata edits;
    whether image upload is reachable was not verified.
19. **Sleep timer and car mode** on mobile — commonly listed by third parties, not confirmed on
    TIDAL's own documentation.
20. **Search result types `UPLOADS` and `USERPROFILES`** — present in the client's filter order, not
    implemented by python-tidal; the `types=` parameter values are unverified. (Recent
    searches/suggestions/did-you-mean, by contrast, are now confirmed as documented v2 endpoints —
    §4.4 — and are no longer open in the same way.)
21. **DJ Extension pricing and stems availability in 2026.** The ~$9/mo figure and Serato-stems note
    may be stale — a 2026 trade-article title suggests a price change and a stems
    withdrawal/reinstatement during 2026 that could not be fetched here
    (digitaldjtips.com/big-price-increase-for-djs-using-tidal/). Re-check before publishing a price.
22. **Album normalization's -14 LUFS target and "on by default on mobile."** Sourced only to
    2019–2020 rollout coverage, not re-verified for 2026 (§3). streamboat only needs the
    ReplayGain/peak fields, which are solid independent of this number.
23. **Whether "Podcasts" survives anywhere as a page-module category.** python-tidal's own doctest
    fixture lists a "Podcasts…" Home/Explore category (`ref:python-tidal/docs/pages.rst`) despite
    TIDAL's 24 Jul 2024 podcast removal (§4.12, §11). Likely a stale fixture; worth one live-account
    check on `pages/explore` before treating either claim as settled.
24. **Mix cadence/count figures** ("My Daily Discovery: 10 tracks/day," "My New Arrivals: 30
    tracks/Friday") rest only on unfetched support-article summaries (§4.2).
25. **The macOS mechanism for exclusive/bit-perfect output.** "Core Audio hog mode," repeated in
    earlier drafts of this report, is an **[inferred]** guess with no source backing it (§5.3) —
    only the Windows WASAPI-exclusive half is attested.

---

## Sources

### Reference checkouts (strongest evidence)

- `ref:TidaLuna/plugins/lib/src/redux/types/actions/actionTypes.ts` — the complete sorted list of
  **694 Redux action types across 48 namespaces** dumped from the shipping official TIDAL desktop
  client, TidaLuna `v1.16.6-beta` (commit `d8cd6bc`, cloned 2026-09-02) — record this version/date
  next to any re-verification. Primary source for every "the native app does/does not have X"
  claim: sidebar structure, queue semantics, settings, remote playback, uploads, blocking, Picks,
  Last.fm, voice, AI playlists — and every *absence* claim (crossfade, offline, Live) is only as
  current as this one build (proven by the crossfade case, §Summary item 8).
- `ref:TidaLuna/plugins/lib/src/redux/types/actions/index.ts` — action payload shapes; source for
  queue operation parameters, settings payload types, remote-playback payloads.
- `ref:TidaLuna/plugins/lib/src/redux/types/store/index.ts` — the whole store shape. Source for
  `SettingsState` (audioNormalization, autoPlay, explicitContentEnabled, quality.streaming,
  desktop.{autoStartMode,closeToTray}, openLinksInDesktopApp, audioSpectrumEnabled), `Player`
  (activeDeviceMode exclusive/shared, availableDevices, forceVolume), `Search`
  (searchResultFilterOrder incl. UPLOADS/USERPROFILES, recentSearches, sessions), `View`, `Session`.
- `ref:TidaLuna/plugins/lib/src/redux/types/store/content/{Track,Album,Playlist,Mix,Video,Artist,BaseMediaItem,StreamingFlags,Lists,index}.ts`
  — the content model: quality enums, audio modes, metadata tags, album UPC/label/review/credits,
  playlist fields, mix types, artist roles, streaming flags (`djReady`, `stemReady`,
  `adSupportedStreamReady`), sort state.
- `ref:TidaLuna/plugins/lib/src/redux/types/store/PlayQueue.ts` — queue element priorities, repeat
  mode enum, shuffle seed, cloud queue shape.
- `ref:TidaLuna/plugins/lib/src/redux/types/store/Playback.ts` — `PlaybackContext` fields
  (actualAudioQuality, actualAudioMode, bitDepth, sampleRate, codec, playbackSessionId).
- `ref:TidaLuna/plugins/lib/src/redux/types/store/RemotePlayback.ts` — the three remote transports,
  mDNS device shape, TIDAL Connect media/queue info.
- `ref:TidaLuna/plugins/lib/src/redux/types/store/{User,UserProfile,Lyrics,Notification}.ts` —
  subscription/entitlement model, per-client offline authorisation, onboarding step list, profile
  and following model, lyrics shape (`lyrics` + `subtitles` + `isRightToLeft`).
- `ref:TidaLuna/plugins/lib/src/redux/types/actions/ContextMenu.ts` — every context-menu type in the
  official client, including `SELECT_SOUND_OUTPUT`, `SELECT_SOUND_QUALITY`, the six sort-order menus
  and the six share menus.
- `ref:TidaLuna/plugins/lib/src/classes/Quality.ts` — quality ladder, `metadataTags` ↔ `audioQuality`
  lookups, badge colours.
- `ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts` — the official desktop's own endpoints:
  `desktop.tidal.com/v1/tracks/{id}`, `/playbackinfo`, `/lyrics`, `/artists/{id}`, `/albums/{id}`,
  `pages/album`, `/playlists/{uuid}/items`, `openapi.tidal.com/v2/tracks?filter[isrc]=`; auth headers
  (`Authorization: Bearer`, `x-tidal-token`); query args `countryCode`, `deviceType=DESKTOP`,
  `locale`; the 2-concurrent playbackinfo semaphore.
- `ref:TidaLuna/README.md` — install paths proving Windows/macOS official apps and Linux-via-tidal-hifi.
- `ref:tidal-hifi/src/TidalControllers/DomController/constants.ts` — live web-player DOM selectors:
  sidebar items, collection items, quality badge, footer favourite, progress bar, shuffle/repeat,
  and the old-UI vs new-UI split.
- `ref:tidal-hifi/src/features/hotkeys/hotkeyConfig.ts` — TIDAL's own desktop keyboard shortcuts.
- `ref:tidal-hifi/src/features/tidal/url.ts` — `tidal.com/browse/track/{id}`, cover-art URL scheme,
  universal-link `?u` suffix, absolute-URL handling for uploaded content.
- `ref:tidal-hifi/src/models/audioQuality.ts`, `ref:tidal-hifi/src/TidalControllers/ReduxController/{ReduxStoreType,ReduxStoreActions}.ts`
  — the quality fields readable from the live player and the actions used to control it.
- `ref:tidal-hifi/src/features/api/swagger.json` — a working minimal remote-control contract.
- `ref:tidal-hifi/docs/audio-quality.md` — Chromium's 48 kHz resampling and the
  `--audio-output-sample-rate=192000` workaround; PipeWire `default.clock.allowed-rates` setup.
- `ref:tidal-hifi/docs/known-issues.md` — Widevine S6007 on Windows; "Tidal is working on
  removing/changing DRM"; volume-reset-from-normalization.
- `ref:tidal-hifi/CHANGELOG.md` — the 2025–2026 UI redesign (auto-detected old/new UI), gapless
  buffer switching, `/current/audio-quality` endpoint shape.
- `ref:tidal-hifi/README.md` — feature list, ad-blocking claims, why no Linux client exists.
- `ref:python-tidal/tidalapi/page.py` — the page endpoints: `pages/home`, `pages/for_you`,
  `pages/explore`, `pages/moods`, `pages/genre_page`, `pages/genre_page_local`, `pages/hires`,
  `pages/videos`, `pages/album`, `pages/artist`, `pages/mix`, `pages/my_collection_my_mixes`.
- `ref:python-tidal/tidalapi/session.py` — device-code and PKCE flows, `GET sessions`,
  `GET search?query&limit&offset&types` with `topHit`, 300-result cap.
- `ref:python-tidal/tidalapi/user.py` — `users/{id}/favorites`, `users/{id}/playlists`,
  `users/{id}/playlistsAndFavoritePlaylists`, `users/{id}/subscription`, and the folder-creation
  half of the folder API (`create-playlist` 224, `create-folder` 247, `add-favorites` 332,
  `remove` 539, folder listing 737/782/799).
- `ref:python-tidal/tidalapi/playlist.py` — playlist CRUD, `/items`, `/set-public`, `/set-private`,
  `user-playlists/{id}/public`, and **the rename (428) and move (519) halves of the folder API** —
  not `user.py`, which the report's summary claim originally implied.
- `ref:python-tidal/tidalapi/{album,artist,media,mix}.py` — `albums/{id}/{items,tracks,review,similar}`,
  `artists/{id}/{albums,bio,similar,toptracks,videos,mix,radio}`,
  `tracks/{id}/{lyrics,mix,radio,playbackinfopostpaywall,urlpostpaywall}`,
  `videos/{id}/urlpostpaywall`, `favorites/mixes/{add,remove}`, `home/feed/static`.
- `ref:python-tidal/tidalapi/types.py` — the exact sort-order enums for every collection list.
- `ref:high-tide/src/pages/artist_page.py` — the artist page section list as reproduced from the API.
- `ref:high-tide/src/pages/explore_page.py`, `ref:high-tide/src/widgets/lyrics_widget.py`,
  `ref:high-tide/src/mpris.py`, `ref:high-tide/src/lib/secret_storage.py` — explore page, synced
  lyrics, MPRIS, libsecret token storage.
- `ref:sone/README.md` and `ref:sone-windows/README.md` — the most complete third-party feature set:
  bit-perfect ALSA/WASAPI exclusive, DAC format matching, signal-path transparency, ReplayGain with
  album/track context, autoplay, gapless (README claims GStreamer ≥1.24; the code contradicts this
  — Sone's `concat`-based `gapless_supported()` has no such floor, see the quality-cascade and
  gapless-playback feature-matrix rows above for the correction), video playback, miniplayer, full-screen
  player, queue persistence, themes, scrobbling, play reporting, proxy, MCP server, OBS overlay,
  `tidal://` deep links, playlist folders, profile editing. **Correction: neither README mentions
  MPRIS/SMTC implementation detail** — that lives in the Cargo manifests, not the READMEs (see
  next entry); do not cite these READMEs for the media-controls crate claim.
- `ref:sone/src-tauri/Cargo.toml:66` (`mpris-server = "0.9"`) and
  `ref:sone-windows/src-tauri/Cargo.toml:64` (`souvlaki = "0.8.3"`) — the actual media-controls
  dependencies: Sone (Linux) uses `mpris-server` directly; sone-windows uses `souvlaki` for Windows
  SMTC only (`ref:sone-windows/README.md:69` describes only "Windows SMTC Integration", no MPRIS/
  Now Playing claim). Neither crate "unifies all three" platforms as an earlier draft of this
  report stated.
- `ref:sone/src-tauri/src/tidal_report/{event.rs,mod.rs,queue.rs}` — the play-reporting pipeline:
  `https://ec.tidal.com/api/event-batch`, group `play_log`, payload fields
  (`playbackSessionId`, `isPostPaywall`, `actualQuality`, `startAssetPosition`/`endAssetPosition`,
  `sourceType`/`sourceId`), headers (`client-id`, `app-version`, `consent-category: NECESSARY`).
- `ref:sone/src-tauri/src/tidal_api.rs` — also: `TERMINAL_SUB_STATUSES` (line 18, the exact
  non-contiguous list), playlist `accessType` normalisation (526-533), TIDAL Connect quality
  history reference, v1/v2 Home-feed parsing (1240-1268, 4161-4210), playlist-folder pagination and
  the `/flattened` endpoint (3313-3400), Feed-vs-Home-page distinction (680-770, 4161-4210),
  `pages/rising` (4386-4406), `TidalMix.mix_type` including history-mix values (841-847).
- `ref:sone/src/lib/tidalUrl.ts` (`parseTidalUrl`) and `ref:high-tide/src/lib/utils.py`
  (`open_tidal_uri`, lines 486-518) — two independent, matching implementations of the `tidal://`
  entity-path grammar: `{track,album,artist}/{numeric id}`, `{playlist,mix}/{string id}`. Also
  `ref:sone/src/utils/itemHelpers.ts` for the `tidal://my-collection/tracks` collection-link form.
- `ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts` — the official
  v2 manifest path: `GET /trackManifests/{id}` with `formats`, `manifestType`, `uriScheme=DATA`,
  `usage=PLAYBACK`, `adaptive`, `x-playback-session-id`; `audioQualityToFormats` /
  `audioFormatsToQuality`; 1-hour manifest expiry.
- **`ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json`** (identical copy at
  `ref:tidal-sdk-android/tidalapi/bin/tidal-api.json`) — **the full official v2 OpenAPI spec, TIDAL
  API 1.10.104, server `https://openapi.tidal.com/v2`, 256 paths, 993 schemas.** Under-used in the
  first research pass; now the primary source for collaborative playlists
  (`/collaborationInvites`, `/collaborationInviteRedemptions`), playlist visibility
  (`Playlists_Attributes.accessType: PUBLIC|UNLISTED|PRIVATE`), cloud queue
  (`/playQueues`, `PlayQueues_Attributes`), offline (`/offlineTasks`, `/downloads`,
  `/installations`), search history (`/searchResults`, `/searchSuggestions`,
  `/searchHistoryEntries`), the social/creator layer (`/comments`, `/reactions`, `/appreciations`,
  `/artistClaims`, `/purchases`), and media replacement/region unavailability
  (`.../relationships/replacement`, `replaceMedia=`, `/usageRules`). See
  `references/openapi-v2-catalogue.md` for the fuller path/schema catalogue this report draws on.
- `ref:tidal-sdk-android/tidalapi/src/main/kotlin/com/tidal/sdk/tidalapi/generated/apis/CollaborationInvites.kt`
  — the generated Kotlin client confirming the collaboration-invite flow is a real, code-generated
  API surface, not merely a spec fragment.
- `ref:TidaLuna/plugins/lib/src/redux/types/store/Notification.ts` — the `Notification {category:
  MUTE|NETWORK|OTHER|PLAYBACK, severity: DEBUG|INFO|WARN|ERROR}` model and the `message/*` /
  `network/*` action namespaces — the native app's own failure/error-surface design (§6).
- `ref:tidal-sdk-android/player/streaming-privileges/` — a whole module dedicated to
  one-stream-at-a-time enforcement, confirming it is server-driven and must be handled.
- `ref:strawberry/src/tidal/tidalstreamurlrequest.cpp` — the four stream-URL endpoints and the
  refuse-on-encryption behaviour worth copying.
- `ref:mopidy-tidal/mopidy_tidal/library.py` — a proven headless browse tree: `tidal:home`,
  `tidal:for_you`, `tidal:explore`, `tidal:moods`, `tidal:genres`, `tidal:mixes`, `tidal:hires`,
  `tidal:my_{artists,albums,playlists,mixes,tracks}`.
- `ref:tidal-connect/bin/entrypoint.sh`, `ref:tidal-connect/build/Dockerfile` and
  `ref:tidal-connect/README.md:59` — proof that a TIDAL Connect *target* requires the proprietary
  `tidal_connect_application` binary plus a vendor device certificate; README.md:59 gives the
  quality nuance ("could play hi-res files only up to 24/48 and MQA content") that the July 2024
  MQA removal then reduced to effectively LOSSLESS-only.
- `ref:tidalt/internal/tidal/api.go`, `ref:tidalt/internal/player/alsa.c` — an ALSA-direct headless
  reference: `tracks/{id}/urlpostpaywall` with quality ladder, `hw:` device with format negotiation.

### Web sources

- https://support.tidal.com/hc/en-us/articles/360003650917-Change-sound-quality-level — Low / High /
  Max naming, codecs and rates; Mobile-data / Wi-Fi / Download quality split. (Accessed via search
  summary; direct fetch blocked.)
- https://support.tidal.com/hc/en-us/articles/17412130162961-HiRes-FLAC-audio — HiRes FLAC up to
  24-bit/192 kHz, platform support.
- https://support.tidal.com/hc/en-us/articles/25876825185425-Audio-Format-Updates — MQA and Sony
  360RA removal.
- https://www.whathifi.com/news/tidal-scraps-mqa-and-spatial-audio-format-heres-what-that-means-for-subscribers,
  https://www.headphonesty.com/2024/06/tidal-officially-dumps-mqa/,
  https://www.strata-gee.com (MQA/360RA removal coverage) and
  https://www.techradar.com (MQA/360RA removal coverage) — announced 17 Jun 2024, effective
  24 Jul 2024; FLAC replacement; offline re-download requirement; 360RA tracks greyed out.
- https://www.ecoustics.com/news/tidal-drops-mqa-360ra-podcasts/ — **the same 24 Jul 2024 change also
  removed all podcasts**; use this dated primary fact in place of the absence-based
  `ContentType = track|video` argument for why podcasts are out of scope (§4.12, §11).
- https://www.digitalmusicnews.com/2024/03/06/tidal-simplifies-subscription-options/,
  https://musicbusinessworldwide.com/tidal-streamlines-offerings-merging-hifi-and-hifi-plus-into-single-10-99-a-month-tier/
  and https://musically.com/2025/11/14/... (2024/03/06 archive) — announced 6 Mar 2024, effective
  10 Apr 2024: HiFi/HiFi Plus consolidation, Student to $4.99, Free tier discontinued, Military/
  First-Responder discounts ended 10 Jun 2024.
- https://www.ecoustics.com/news/good-news-audiophiles-tidal-hifi/ — HiFi Plus price drop to
  $10.99 as part of the consolidation.
- https://www.ecoustics.com/news/tidal-price-increase-2026/ and
  https://www.neowin.net/news/tidal-is-getting-a-price-hike/ — **replace the earlier pricing
  aggregators (subscriptionscompare.com, jaideepass.com, freeyourmusic.com) with these**: Aug 2026
  price increase to $11.99 / $19.99 / $6.99, effective the first billing date on/after 3 Aug 2026.
  Confidence raised from the first draft's 0.6 to solid [verified-web] — two independent trade
  sources agree on the exact figures.
- https://support.tidal.com/hc/en-us/articles/28548110049681-Exclusive-Mode and
  https://forums.whathifi.com (thread 127860) — exclusive control of the audio device; volume must
  be changed in-app; mutual exclusion with Force Volume, corroborated independently. Exact quote:
  "With Force Volume, Tidal keeps the app's volume at the maximum level, allowing you to control the
  sound output through external devices like your DAC or speakers" — use this wording, not "software
  volume when device volume is uncontrollable" (an earlier draft's incorrect gloss, now corrected
  throughout §5.3 and the feature matrix).
- https://support.tidal.com/hc/en-us/articles/360004565898-Tidal-Connect — how Connect works,
  same-network requirement, iOS 15 / Android 7 minimums, Autoplay not universal on targets, and the
  "can't control your desktop app from mobile … coming soon" note.
- https://support.tidal.com/hc/en-us/articles/201623252-How-Many-Devices-Can-I-Use-Simultaneously —
  **the offline-device-count source**: 1 device online, **5 devices offline** simultaneously — not
  3, which was wrong in three places in the first draft (summary, §2, §9) and is corrected
  throughout this version.
- https://tidal.com/connect and https://tidal.com/supported-devices?filter=tidal-connect — Connect
  device partners.
- https://support.tidal.com/hc/en-us/articles/360004255778-Dolby-Atmos — Atmos platform support
  (mobile/TV/car); desktop not listed.
- https://support.tidal.com/hc/en-us/articles/23351150845329-Daily-Discovery — Daily Discovery: 10
  tracks, daily, in Home under Mixes For You.
- https://support.tidal.com/hc/en-us/articles/29710779228817-My-New-Arrivals — My New Arrivals: 30
  tracks, weekly on Fridays.
- https://support.tidal.com/hc/en-us/articles/360000702697-My-Mix and
  https://support.tidal.com/hc/en-us/articles/360003671658-My-Video-Mix — My Mix and My Video Mix.
- https://support.tidal.com/hc/en-us/articles/19255247157905-Picks and
  https://support.tidal.com/hc/en-us/articles/10177158714769-Manage-your-TIDAL-Profile — Picks,
  public profiles, playlist sharing.
- https://support.tidal.com/hc/en-us/articles/26542012438673-Tidal-Upload — TIDAL Upload: capability,
  beta status.
- https://musically.com/2025/11/14/tidal-adds-a-soundcloud-esque-upload-feature-for-diy-artists/ and
  https://celebrityaccess.com/2025/11/17/tidal-introduces-upload-spotlight-and-a-100k-contest-for-indie-artists/
  and https://www.musicradar.com/music-tech/software-apps/... — Upload launch Nov 2025, 5 GB/track,
  200-track cap, no royalties, US/UK/EEA/CH/CA, Spotlight and Upload Headliners contest.
- https://support.tidal.com/hc/en-us/articles/27563493690129-DJ-Extension-Add-On and
  https://tidal.com/djs — DJ Extension add-on (~$9/mo), stems, Serato/rekordbox/djay Pro/VirtualDJ/
  DJUCED integration; base-plan requirement (not confirmed as "Individual or Student only"
  specifically).
- https://support.serato.com/hc/en-us/articles/360000588435-Serato-DJ-Pro-TIDAL-Music-Frequently-Asked-Questions
  — Serato DJ Pro 3.1.3+ required for TIDAL stems.
- https://www.digitaldjtips.com/big-price-increase-for-djs-using-tidal/ — **title alone indicates a
  2026 DJ Extension price change and a stems withdrawal/reinstatement; could not be fetched
  (EGRESS_BLOCKED) or corroborated. Re-check this before publishing the ~$9/mo figure** —
  §Open questions item 21.
- https://techcrunch.com/2023/04/04/tidals-new-live-feature-will-let-you-host-a-live-djing-session/
  and https://djmag.com/news/tidal-testing-new-dj-sessions-feature — TIDAL Live / DJ sessions, April
  2023 launch. No discontinuation source found.
- **https://tidal.com/magazine/article/what-were-working-on-and-why/1-98678** (TIDAL Magazine, June
  2026) and **https://piunikaweb.com/2026/03/23/tidal-is-bringing-back-the-much-requested-crossfade-feature/**
  (23 Mar 2026) — **crossfade is officially confirmed reintroduced on iOS and Web** with a 0–12
  second slider, toggled in Settings. This replaces the first draft's dismissal of crossfade
  reports as "SEO content farms" — that was the report's single most damaging error (§Summary item
  8). Re-verify current desktop-client status before finalising any feature matrix built on this.
- https://productionadvice.co.uk/tidal-normalization-upgrade/ and
  https://audioxpress.com/news/tidal-implements-album-loudness-normalization-and-activates-it-by-default-for-mobile-players
  — album normalization to −14 LUFS, on by default on mobile. **Both predate 2021; not re-verified
  for 2026 — keep as [uncertain] (§3, §Open questions item 22).**
- https://support.tidal.com/hc/en-us/articles/23553629074193-Sharing-music-links-across-streaming-platforms
  — universal share links across services.
- https://support.tidal.com/hc/en-us/articles/18906946496017-CarPlay,
  https://support.tidal.com/hc/en-us/articles/18907040043921-Android-Auto,
  https://support.tidal.com/hc/en-us/articles/18907133150993-Android-Automotive — car integrations
  and their offline limits.
- https://support.tidal.com/hc/en-us/sections/115001636665-Web-Player — web player section.
- https://defkey.com/tidal-desktop-shortcuts — a third-party, crowd-sourced shortcut aggregator
  (direct fetch blocked); `ref:tidal-hifi/src/features/hotkeys/hotkeyConfig.ts` cites this page as
  *its own* source for its default hotkey config. **This is one wrapper's copy of one unverifiable
  aggregator page, not TIDAL documentation — downgrade any claim built on it from
  [verified-source] to [verified-web, single unverifiable aggregator]** (§5.6).
- https://www.whathifi.com/tidal/review and https://www.digitaltrends.com/home-theater/what-is-tidal/
  — 650k+ music videos, catalogue size, video audio quality. **Vintage unstated; treat as
  [uncertain, possibly stale]** (§4.12, §Open questions item 2) — Sone's own README only says video
  audio "does not use the bit-perfect lossless signal path," which is weaker than "AAC ~320 kbps."
- https://www.stereofox.com/articles/explore-the-best-tidal-music-editorial-playlists/ — Explore page
  composition: genres, Moods/Activities, TIDAL Rising, editorial content.
- https://developer.tidal.com/documentation and https://tidal-music.github.io/tidal-api-reference/ —
  official developer portal and OpenAPI reference (direct fetch blocked from this environment; cited
  for completeness, not relied on for any claim above).

### Sources that were wanted but unreachable

`support.tidal.com`, `tidal.com` and `developer.tidal.com` are blocked by the network egress proxy in
this environment, so every TIDAL-owned page above is cited through a search-engine summary rather
than a full read — **except that `developer.tidal.com`'s own OpenAPI document is not actually
unreachable**: it is vendored on disk (see the Reference-checkouts section above,
`tidal-sdk-web/packages/api/bin/tidal-api-oas.json`) and should be the first stop for anything this
report marks undocumented. `defkey.com`, `digitaldjtips.com` and `reddit.com` are likewise
unreachable from this environment. Anyone re-verifying this report from a normal network should:
(1) re-read the TIDAL support articles directly, particularly for the Open Questions in
§Open questions items 11–25; (2) fetch digitaldjtips.com's DJ-pricing article (item 21); and
(3) read `ref/tidal-api-docs` and `ref/tidal-fokka-engineering-`, the two reference checkouts this
report never consulted (see the coverage-boundary note at the top of this document).
