# Feature matrix — every feature, tiered

The full tiering the report's "Feature matrix" section builds, condensed to what an implementer
needs to act (full per-row native-app-location column and reference-client abbreviations are in the
report, `docs/research/tidal-client-features.md`, "Feature matrix" section, if you need those too).

Priority tiers: **MVP** = required for a first usable release; **v1** = required before calling it
a TIDAL client; **later** = post-1.0; **out-of-scope** = will not build (with reason).

Reference-client abbreviations used in the "who implements" column: HT = High Tide, SO =
Sone/Sone-windows, TH = tidal-hifi, TL = TidaLuna, MO = mopidy-tidal, ST = Strawberry, TT = tidalt,
CL = tidal-cli, PT = python-tidal (library-level support, not a standalone client). A reference
client implementing a feature is evidence the API is reachable, not proof streamboat's own
architecture should copy that client's implementation.

Table of contents:
1. Authentication and account
2. Playback
3. Browse and content
4. Library management
5. Remote playback and integrations
6. UI, settings and platform

## 1. Authentication and account

| Feature | Tier | API availability / who implements | Rationale |
| --- | --- | --- | --- |
| OAuth device-code login | **MVP** | Fully available; PT, SO, TT, MO, tidalrs, libopenTIDAL — ref:python-tidal/tidalapi/session.py | Nothing plays without a session; only flow that works headless/CLI |
| OAuth PKCE login (browser redirect) — required for `HI_RES_LOSSLESS` | **MVP** | Available; PT, HT, MO, ST, tidal-cli, SDKs — ref:python-tidal/tidalapi/session.py, ref:high-tide | Unlocks the top quality tier; desktop/GUI needs it from day one |
| Token refresh + secure token storage | **MVP** | HT: libsecret; SO: OS keyring + AES-256-GCM file; TT: keychain + age fallback — ref:high-tide/src/lib/secret_storage.py, ref:sone/README.md, ref:tidalt | Without it every restart is a re-login; plaintext storage is a credential-leak risk |
| `GET sessions` → sessionId, userId, countryCode (mandatory on later calls) | **MVP** | Available; all clients — ref:python-tidal/tidalapi/session.py | `countryCode` gates catalogue availability on every subsequent call |
| Subscription/entitlement read (`users/{id}/subscription`), gate UI on `highestSoundQuality` | **v1** | Available; PT — ref:python-tidal/tidalapi/user.py | Show the right quality ceiling; don't request tiers the account can't get |
| Streaming-privileges enforcement (one stream at a time; handle revocation) | **v1** | Server-enforced regardless of streamboat; must *handle* the error even without subscribing to the socket — ref:.../actionTypes.ts `player/STREAMING_PRIVILEGES_REVOKED`; ref:tidal-sdk-android/player/streaming-privileges | Unhandled revocation looks like a crash/hang, not a clean stop |
| Multiple accounts / account switching | out-of-scope | — (absence in Redux namespace) | Not a parity gap — native app doesn't do it either |
| Sign-up, payment, plan management | out-of-scope | Web-only — ref:.../store/index.ts `settings.urls` | TIDAL itself punts this to a web page |
| Facebook / Snapchat / TikTok linking | out-of-scope | ref:.../actionTypes.ts `user/*` | Social-graph plumbing with no listening value |

## 2. Playback

| Feature | Tier | API availability / who implements | Rationale |
| --- | --- | --- | --- |
| Play / pause / next / previous / stop | **MVP** | Local — `playbackControls/*` | The whole point of the app |
| Seek (absolute, relative ±) | **MVP** | Local — `SEEK`, `SEEK_FORWARDS`, `SEEK_BACKWARDS` | Table stakes |
| Volume, mute, unmute-to-previous | **MVP** | Local — `SET_VOLUME`, `TOGGLE_MUTE` | Table stakes |
| Manifest fetch + quality cascade | **MVP** | `playbackinfopostpaywall`; SO, HT, ST, TT, tidalrs — ref:python-tidal/tidalapi/media.py | Nothing plays without it; retrofitting terminal-vs-transient handling later is painful. **Correction, previously misattributed**: the `HI_RES_LOSSLESS→LOSSLESS→HIGH→LOW` order shown here does not come from Sone — Sone's shipped `ORDER` is `[HI_RES_LOSSLESS, HI_RES, LOSSLESS, HIGH]` (`ref:sone/src-tauri/src/commands/playback.rs:33`), no `LOW`; the `→LOW` form matches tidalt's docs instead (`ref:tidalt/docs/architecture.md:17`, itself flagged stale elsewhere against tidalt's own code). Whether to keep the retired `HI_RES` rung and whether `LOW`/`HIGH` are in scope at all are both still open — see `audio-pipeline` Open decisions #3/#13 and `tidal-oss-landscape` Unverified — don't treat either ladder as settled MVP scope. |
| DASH (MPD) and BTS manifest handling | **MVP** | PT, HT, SO, MO, ST — ref:python-tidal/tidalapi/media.py | Both occur in normal use; missing one silently fails some tracks |
| Detect encrypted manifests and refuse cleanly | **MVP** | ST does this — ref:strawberry/src/tidal/tidalstreamurlrequest.cpp | Legal posture, not just a feature |
| Shuffle (seeded, reversible) | **MVP** | Local — `lastShuffleSeed` | Users notice immediately if shuffle is destructive/unseeded |
| Repeat off / all / one | **MVP** | Local — `RepeatMode {Off=0,All=1,One=2}` | Table stakes |
| Queue view: reorder, remove, clear, play-next vs add-to-queue | **MVP** | Local — `ADD_NEXT` vs `ADD_LAST`, `MOVE_TRACK`, `REMOVE_AT_INDEX`, `CLEAR_QUEUE` | Play-next vs add-to-queue as distinct insert positions is tested immediately |
| Gapless playback (preload next) | **MVP** | GStreamer `playbin3` about-to-finish, needs GStreamer ≥1.24 (HT) or `concat` (SO) | A native client's core reason to exist — the web player can't always guarantee this. **Correction, previously misattributed the 1.24 floor**: it applies only to the `playbin3`/`about-to-finish` design (High Tide's); Sone's `concat` design has no such floor — `gapless_supported()` is just `gst::ElementFactory::find("concat").is_some()`, and `concat` has shipped since long before 1.24 (`ref:sone/src-tauri/src/audio.rs:3302-3309`; Sone's own README claims a 1.24 floor, contradicted by its own code — cite the code). |
| Streaming-quality selector | **MVP** | `settings/SET_STREAMING_QUALITY` | Choosing quality is core to why users pay for TIDAL |
| Queue source attribution ("Playing from …") | **v1** | Local — `sourceName`, `sourceUrl` | Cheap and expected once the queue exists |
| Lazy queue filling for huge lists | **v1** | Local — `FETCH_REST_OF_THE_TRACKS_AND_ADD_TO_QUEUE` | Without it, a 10,000-track collection stalls on load |
| Queue persistence across restarts | **v1** | Local; SO advertises it — ref:sone/README.md | Expected baseline once a queue exists |
| Autoplay / continuation when queue ends | **v1** | `content/LOAD_SUGGESTIONS`; SO implements — ref:sone/README.md | Visible UX parity; depends on recommendation endpoints |
| Loudness normalization NONE/ALBUM/TRACK | **v1** | Gain + peak in playbackinfo; SO, HT implement — ref:sone/README.md | A checkbox implementation is the wrong shape (three states, not two) |
| Bit-perfect / exclusive output (WASAPI exclusive, ALSA hw:; macOS mechanism **[inferred]**) | **v1** | Local; SO (ALSA+WASAPI), TT (ALSA hw:) — ref:sone, ref:tidalt | The reason a native client exists at all — the web player can't do this |
| Audio device enumeration + selection | **v1** | Local — `availableDevices`, `SELECT_SOUND_OUTPUT` | Required before exclusive output means anything |
| Play reporting (`play_log` → `ec.tidal.com/api/event-batch`) so Recently Played works | **v1** | SO implements; no other OSS client does — ref:sone/src-tauri/src/tidal_report/event.rs | Without it the user's own account degrades (dead Recently Played, stale Daily Discovery) — bigger than any missing screen |
| Force Volume per device — **pins the app's own volume at 100% so an external DAC/amp is the sole volume control** (opposite of a software-volume fallback) | later | Local — `player.forceVolume`, `SET_FORCE_VOLUME` | Cheap once exclusive-mode plumbing exists; not needed for a usable v1 |
| Signal-path transparency (show every conversion) | later (differentiator) | Local; SO implements — ref:sone/README.md | Not in native app; nice audiophile extra, no schema risk |
| Crossfade — **confirmed shipping on iOS/Web in 2026; parity work, not a differentiator; re-tier once desktop status is confirmed** | later (pending desktop confirmation) | Purely local DSP if built | [verified-web] — TIDAL Magazine, June 2026 |
| Audio spectrum visualiser | later | Local — `settings.audioSpectrumEnabled` | Cosmetic, no schema risk |
| Video playback (music videos, HLS) | later | `videos/{id}/urlpostpaywall`; SO implements with hls.js — see `library-playlists-collections.md` §7 | Separate HLS pipeline for a scope the owner hasn't committed to (Open decision #2) |
| Equalizer | out-of-scope | — (absence in Redux namespace) | Not a parity gap — native app has none either |
| Dolby Atmos playback | out-of-scope | — | Native desktop doesn't do it; EAC3-JOC decode + renderer is a large lift for no parity gain |

## 3. Browse and content

| Feature | Tier | API availability / who implements | Rationale |
| --- | --- | --- | --- |
| Search: top hit + tracks/videos/artists/albums/playlists | **MVP** | `GET search?types=…` (≤300 results); PT, HT, SO, MO, CL — ref:python-tidal/tidalapi/session.py | Users expect to find things immediately |
| My Collection: Tracks/Albums/Artists/Playlists (with sort + direction) | **MVP** | `users/{id}/favorites/*`; PT, HT, SO, MO — ref:python-tidal/tidalapi/{user,types}.py | A user's own library is core to "using TIDAL" |
| Favourite / unfavourite anything (incl. users) | **MVP** | `users/{id}/favorites/*` add/remove; `favorites/mixes/{add,remove}` — ref:python-tidal/tidalapi/user.py | The single most-used library-mutation action in any music app |
| Home page (dynamic modules, tabs, cursor pagination) | **v1** | `pages/home` and/or `home/feed/{slug}`; HT, SO, MO — ref:python-tidal/tidalapi/page.py | First screen the user sees; a defensive module renderer pays off everywhere else |
| For You page | **v1** | `pages/for_you`; MO — ref:mopidy-tidal/mopidy_tidal/library.py | Distinct endpoint from Home; expected on parity |
| Explore: genres, moods, charts, TIDAL Rising, new releases | **v1** | `pages/explore`, `pages/moods`, `pages/genre_page`, `pages/hires`, `pages/videos`; HT, MO — ref:high-tide/src/pages/explore_page.py | Second most-used nav destination after Home |
| Artist page: top tracks, albums, EP&singles, appears-on, similar, bio, videos, radio, follow, share | **v1** | All endpoints available; HT implements all but Videos — ref:high-tide/src/pages/artist_page.py, ref:python-tidal/tidalapi/artist.py | One of the most-visited page types |
| Album page: multi-volume tracks, credits, review, similar, UPC/label/date, quality badges | **v1** | `albums/{id}/items`, `/review`, `/similar`, `pages/album`; HT, SO — ref:.../store/content/Album.ts | Second most-visited page type after artist/home |
| Track credits panel | **v1** | `content/LOAD_ITEM_CONTRIBUTORS`; not in PT high-level API — ref:.../actionTypes.ts | Visible, cheap once the album page exists |
| Lyrics — plain and time-synced, RTL aware | **v1** | `tracks/{id}/lyrics`; HT, SO implement synced lyrics — ref:.../store/Lyrics.ts | High visible value per unit of effort |
| Track radio / artist radio / album mix | **v1** | `tracks/{id}/radio`, `artists/{id}/radio`, `*/mix`; HT, SO — ref:python-tidal/tidalapi/{media,artist}.py | Expected context-menu action, cheap once the page exists |
| Mixes: My Mix N, Daily Discovery, New Arrivals, Video Mix, history mixes | **v1** | `pages/mix`, `pages/my_collection_my_mixes`; HT, SO, MO — ref:python-tidal/tidalapi/mix.py | User-visible personalisation the owner has not asked to cut |
| My Collection: Videos/Mixes (sort + direction) | **v1** | `users/{id}/favorites/*`; PT, HT, SO, MO | Depends on video/mix scope decisions but the endpoint exists |
| Recently played | **v1** | `content/LOAD_RECENT_ACTIVITY`; depends on play reporting — see `library-playlists-collections.md` §6 | Depends on play reporting (row above) being implemented first |
| Editorial articles inline (`content.articles`, `content.articleLists`) | later | Client-side content type only; python-tidal has no article model — ref:.../store/content/Article.ts | No playback value; safe to defer |
| Search: uploads and user-profile result types | later | **Not in python-tidal** — needs direct API work — ref:.../store/index.ts `searchResultFilterOrder` | Small result-type gap, low value until Uploads/social scope is decided |
| Recent searches, suggestions, "did you mean" | later | `/searchResults`, `/searchSuggestions`, `/searchHistoryEntries/{id}` documented in the v2 spec; no OSS client implements them yet — ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json | Documented endpoints, not reverse-engineering — "later" only because untested by any OSS client |
| Contributor/credits page (songwriter, producer, engineer …) | later | `content/LOAD_DYNAMIC_CONTRIBUTOR_PAGE`; not in any OSS client — ref:.../store/content/Artist.ts | Real value for credits nerds but low traffic |
| Charts | later | Reachable as an Explore `PageLink`, no dedicated endpoint — ref:python-tidal/docs/pages.rst | Confirmed page category, reachable once Explore renders modules |
| Podcasts | out-of-scope | Removed 24 Jul 2024 — ecoustics.com/news/tidal-drops-mqa-360ra-podcasts/ | TIDAL's own removal is dispositive; residual doctest category is a staleness question, not a design one |

## 4. Library management

| Feature | Tier | API availability / who implements | Rationale |
| --- | --- | --- | --- |
| Create / rename / delete playlist | **v1** | `playlists`, `my-collection/playlists/folders/create-playlist`; PT, HT, SO — ref:python-tidal/tidalapi/playlist.py | Core library-management action |
| Add / remove playlist items | **v1** | `playlists/{uuid}/items`; PT — ref:python-tidal/tidalapi/playlist.py | Core library-management action |
| Reorder playlist items (with ETag concurrency) | **v1** | `playlists/{uuid}/items/{index}` move; PT — ref:python-tidal/tidalapi/playlist.py | Skipping ETag guards risks clobbering a concurrent edit from another device |
| Edit playlist title/description/cover | **v1** | PT supports metadata edit; custom cover upload **[uncertain]** — ref:python-tidal/tidalapi/playlist.py | Expected once create/delete exists |
| Visibility: **three-state PUBLIC/UNLISTED/PRIVATE**, not a binary toggle | **v1** | Legacy binary: `/set-public`, `/set-private` (PT); v2 three-state: `Playlists_Attributes.accessType` — ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json | Modelling this as a checkbox silently turns UNLISTED into PUBLIC or PRIVATE |
| Playlist folders (create/rename/move/remove; read at scale via cursor pagination) | **v1** | `my-collection/playlists/folders/*`; PT, SO — ref:python-tidal/tidalapi/{user,playlist}.py | Common for users with many playlists; the folder screen is a real route |
| Multi-select operations on track lists | **v1** | Local — `selection/*`, `MULTI_MEDIA_ITEM` menu | Expected once any track-list UI exists |
| Collaborative playlists (invite/redeem) | later | `/collaborationInvites`, `/collaborationInviteRedemptions`, `playlists/{id}/relationships/collaborators`; no OSS client — ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json | Real feature, but no OSS client has implemented it and the desktop UI may not expose it yet |
| Suggested tracks for a playlist | later | `content/LOAD_PLAYLIST_SUGGESTED_MEDIA_ITEMS`; not in PT — ref:.../actionTypes.ts | Recommendation-quality feature, not core library management |
| Block track / artist (+ blocked-items page) | later | `blocks/*`; no OSS client implements — ref:.../actionTypes.ts | Permanently reshapes account-wide recommendations — worth building deliberately |
| Play uploaded content that others shared with you | later | `upload` flag on items; absolute cover URLs — ref:tidal-hifi/src/features/tidal/url.ts | Playback-only consumption of a feature streamboat won't produce content for |
| Save for Later (7th collection type) | later | `/userCollectionSaveForLaters/{id}`; no OSS client — ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json | Real v2 collection type; low priority until the core six lists are solid |
| AI playlist creation | out-of-scope (for now) | `folders/CREATE_AI_PLAYLIST`; no OSS client; endpoint unknown — ref:.../actionTypes.ts | Endpoint and rollout state both unconfirmed; revisit once documented |
| TIDAL Upload (upload your own tracks) | out-of-scope | Would need undocumented upload endpoints — ref:.../actionTypes.ts `creatorContent/*` | Creator tooling, not a player feature |

## 5. Remote playback and integrations

| Feature | Tier | API availability / who implements | Rationale |
| --- | --- | --- | --- |
| Be controlled by MPRIS (Linux) / SMTC (Windows) / Now Playing (macOS) | **MVP** | `souvlaki` 0.8.3 covers all three in one crate — Linux via `dbus`/`dbus-crossroads`, Windows SMTC, macOS `MPNowPlayingInfoCenter` (ref:sone-windows/src-tauri/Cargo.lock:5040-5056) — but the reference clients still split per platform: Sone-windows uses it for Windows SMTC only (ref:sone-windows/src-tauri/Cargo.toml:64); Sone/Linux uses `mpris-server` instead (ref:sone/src-tauri/Cargo.toml:66); High Tide hand-rolls Python D-Bus MPRIS (ref:high-tide/src/mpris.py); tidal-hifi has its own MPRIS service | Expected baseline OS integration on every desktop platform; evaluate `souvlaki` per platform before assuming a split is needed |
| Hardware media keys | **MVP** | Via the media-controls integration above — ref:tidal-hifi/src/constants/mediaKeys.ts | Comes largely free once OS media-control integration exists |
| Last.fm scrobbling | **v1** | Native has `lastFm/*`; SO adds Last.fm + Libre.fm + ListenBrainz — ref:.../actionTypes.ts, ref:sone/README.md | TIDAL itself treats this as headline (own onboarding step, own route — `LOADER_DATA__LASTFM`) |
| System tray + minimise/close to tray + autostart | **v1** | `settings.desktop.closeToTray`, `autoStartMode`; SO, TH — ref:.../store/index.ts | Expected desktop-app behaviour on Windows/macOS/Linux |
| Local HTTP control API / MCP / OBS overlay | later (differentiator, but natural for headless mode) | TH exposes `/player/*` REST; SO exposes MCP on 5577 and OBS overlay on 5578 — ref:tidal-hifi/src/features/api/swagger.json, ref:sone/README.md — see `remote-playback-connect-controls.md` §8 | Directly serves the owner's headless/CLI requirement; define the contract before writing either front end |
| Cloud queue / continue listening across devices | later | `cloudQueue/*` client-side; `/playQueues`/`/playQueues/{id}` server-side (v2 spec); no OSS client implements it yet — ref:.../store/PlayQueue.ts, ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json | **Documented, not a black box** — needs the local queue model solid first, and its repeat/shuffle enum differs from the client's own |
| TIDAL Connect **controller** (discover + hand off to speakers) | later | mDNS discovery documented; cloud-queue mechanism confirmed documented; device *control* protocol undocumented, unimplemented by anyone — ref:.../store/RemotePlayback.ts | Real feature owners of Connect hardware expect, but real research cost |
| Chromecast | later | `chromeCast/*` in native; no OSS client — ref:.../actionTypes.ts | Real feature, moderate research cost, no OSS precedent |
| AirPlay (macOS) | later | Comes free via CoreAudio device selection — **[inferred]** | Low cost, why not |
| Discord Rich Presence | later (differentiator) | SO, TH, HT | Cheap, popular with OSS clients, no schema risk |
| Proxy support (HTTP/HTTPS/SOCKS5) | later | SO | Niche but cheap; no schema risk |
| Desktop notifications on track change | later | Wrapper feature — ref:tidal-hifi/README.md | Nice-to-have, no schema risk |
| TIDAL Connect **target** (be a Connect endpoint) | out-of-scope | Requires a proprietary binary + vendor certificate — ref:tidal-connect/bin/entrypoint.sh | Not reproducible without the binary |

## 6. UI, settings and platform

| Feature | Tier | API availability / who implements | Rationale |
| --- | --- | --- | --- |
| Streaming-quality selector | **MVP** | `settings/SET_STREAMING_QUALITY` — ref:.../store/index.ts | Users choosing quality is core to why they pay for TIDAL |
| Sidebar nav: Home, Explore, Collection groups | **v1** | Local — ref:tidal-hifi/src/TidalControllers/DomController/constants.ts | Primary navigation loop |
| Now Playing full screen with art, lyrics, credits | **v1** | Local — `view/ENTER_NOWPLAYING` — see `browse-pages-screens.md` §8 | High-visibility screen, cheap once lyrics/credits data exists |
| Keyboard shortcuts + in-app cheatsheet | **v1** | Local; match TIDAL's *reported* bindings loosely, not as gospel — ref:tidal-hifi/src/features/hotkeys/hotkeyConfig.ts | Expected desktop-app affordance |
| Explicit-content filter | **v1** | `explicit` flag on every item; filter client-side — ref:.../store/index.ts | Client-side only in practice (Open decision #8), cheap once the flag is read |
| Deep links: `https://tidal.com/browse/...` and `tidal://` | **v1** (open the URL scheme), later (register OS handler) | `tidal://{track,album,artist}/{numeric id}` and `tidal://{playlist,mix}/{string id}` confirmed in two clients — ref:high-tide/src/lib/utils.py, ref:sone/src/lib/tidalUrl.ts | Grammar is confirmed — no excuse to defer |
| Universal share links (`?u`) — producing | **v1** | Trivial string append — ref:tidal-hifi/src/features/tidal/url.ts | Cheap once share links exist |
| Sidebar nav: Feed, Uploads | later | Depends on social/creator scope decisions | Not part of the primary navigation loop until scoped |
| Native fullscreen | later | Local — `view/ENTER_NATIVE_FULLSCREEN` | Nice-to-have window-management feature |
| Mini player (floating) | later (differentiator) | **Not in native app**; SO, Sone-windows add one — ref:sone/README.md | Cheap, popular with OSS clients, no schema risk |
| Language / localisation | later | Client ships i18n bundles (`locale.bundles`) — ref:.../store/index.ts | Nice-to-have, large surface area, no functional blocker |
| Universal share-link resolution via `/dspSharingLinks` | later | `/dspSharingLinks`, `/shares`, `/savedShares` (v2 spec, no OSS client) — ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json | Relevant only if streamboat wants to *resolve* an incoming shared link |
| Theming / custom colours | later (differentiator) | **Not in native app**; SO (15 presets + picker), TH (themes) — ref:sone/README.md | Cheap, visible, no schema risk |
| Offline downloads / logged-in subscriber cache | out-of-scope for desktop-parity; separate owner-scoped design question | TIDAL's own `/offlineTasks` STORE/REMOVE + `/installations/.../offlineInventory` is a candidate shape if the owner wants a cache — ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json | No native desktop equivalent; mirroring TIDAL's shape beats inventing one |
| Ads / ad-supported playback | out-of-scope | Vestigial plumbing only — `playQueue/ADD_PROMO_ITEM_TO_QUEUE`, `Video.adsUrl`, `Video.adsPrePaywallOnly` (ref:.../actionTypes.ts, ref:.../store/content/Video.ts), plus `StreamingFlags.adSupportedStreamReady` (`quality-playback-queue.md` §3) — **[uncertain]** whether any of it is live post-free-tier-removal | No live ad-supported tier exists to build against |
| Voice commands | out-of-scope | Undocumented, likely Web Speech API — ref:.../actionTypes.ts | Low value relative to cost |
| Live / DJ sessions | out-of-scope | — **[uncertain]** | Cannot safely build against a feature whose current existence is unconfirmed |
| Feature-flag / experiment platform | out-of-scope | `experimentationPlatform/*`, `featureFlags/*` — ref:.../actionTypes.ts | TIDAL-internal tooling, no user-facing equivalent |
| Analytics/event tracking (`eventTracking/*`) | out-of-scope **except** play_log | Only `play_log` has user-visible consequences — ref:.../actionTypes.ts | The rest is TIDAL's own product telemetry |
| Comments, reactions, appreciations, artist claims, purchases | out-of-scope (needs an explicit owner decision, not a default) | `/comments`, `/reactions`, `/appreciations`, `/artistClaims`, `/purchases`; no OSS client — ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json | Real moderation/privacy implications if rendered; owner hasn't scoped social this far (Open decision #1) |
