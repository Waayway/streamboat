---
name: tidal-client-features
description: Product knowledge for streamboat's TIDAL feature set — what the native apps (desktop, web, mobile, TV, Connect) actually do, which features the unofficial API (python-tidal/High Tide/Sone) can reach, the priority tier (MVP/v1/later/out-of-scope), and pitfalls for each. Use this whenever touching playback/quality/queue logic, the browse or home-page renderer, playlists/library/collections, search, artist/album/track pages, lyrics, remote playback or TIDAL Connect, account/entitlements, or sharing/deep-links; also load it whenever a task mentions TIDAL, python-tidal, High Tide, Sone, tidal-hifi, TidaLuna, playbackinfo, manifest, audioQuality, ReplayGain, gapless, crossfade, cloud queue, Connect, MPRIS/SMTC, collaborative playlist, feature matrix, MVP/v1 scoping, roadmap, Recently Played, play_log, scrobbling, Last.fm, offline cache, Feed, profile, Uploads, video, credits, Explore, mixes and radio, or "feature parity" — TIDAL's actual model differs from generic assumptions in ways that cause silent bugs.
---

# TIDAL client features for streamboat

Source of truth: `docs/research/tidal-client-features.md` (the full research report, corrected —
read it for narrative depth, rationale and the full source list). This skill is the load-on-demand
distillation: the facts, tables and pitfalls an implementer needs while writing code, without
re-reading a 1800-line report every time.

`ref:<project>/<path>` in this skill and its references means a shallow git clone of a named
open-source project kept in the research environment, e.g. `ref:python-tidal/tidalapi/session.py`.
Full project→URL mapping is in `references/sources.md`. These clones are not part of the streamboat
repo; they are read-only evidence for the claims below.

**Evidence tags used inline** — `[verified-source]` = read directly out of a reference checkout,
the strongest tier; `[verified-web]` = a documentation/press source with a URL, fully fetched;
`[verified-web, unfetched]` = the same but the source could not be directly fetched in the research
environment (TIDAL's own support/developer domains are blocked to direct fetch there, so these are
cited via search-engine summaries) — weaker than a fetched `[verified-web]`, worth re-checking
before relying on it; `[inferred]` = a reasonable deduction from other evidence, not stated
anywhere; `[uncertain]` = could not confirm, flagged rather than silently dropped. Full definitions
and the network-constraint context: `docs/research/tidal-client-features.md`, "How to read the
evidence markers."

**Known errors in the source report — do not re-import these when reading it for narrative
depth**: it says 28 screens (corrected here and in `references/browse-pages-screens.md` §6 to 29
route loaders, 28 with success/fail variants); its Summary item 21 / §4.2 say python-tidal's
`Session.home()` "falls back" to the legacy Home endpoint (it doesn't — it's a plain caller-supplied
`if`/`else` on `use_legacy_endpoint` with no automatic v1 degradation; streamboat must implement its
own fallback — see `references/browse-pages-screens.md` §1); it treats `NEW_HISTORY_MIX` as a
`MixType` value (it's a `Feed` `activityType`, not a `mix_type` — see
`references/browse-pages-screens.md` §4); its §5.2 and Summary item 8 explain the desktop dump's
missing crossfade toggle as "most likely means the snapshot predates the desktop rollout" — this
reasoning is backwards, not merely unconfirmed: the dump is dated 2026-09-02, **three months after**
the Mar–Jun 2026 announcement window it's supposedly older than, so the absence is real evidence
against desktop, not a dating artifact to explain away — see `references/quality-playback-queue.md`
§7 for the corrected reasoning before citing the report's own framing.

**Version caveat that matters for every "the client has/doesn't have X" claim below:** the desktop
client's Redux action dump this report is built on is TidaLuna `v1.16.6-beta` (commit `d8cd6bc`,
2026-09-02) — 694 action types across 48 namespaces. Absence in that dump is evidence about one
build, not about TIDAL's current product — but read that evidence on its actual timeline: the dump
was captured **three months after** TIDAL's Mar–Jun 2026 iOS/Web crossfade announcement, so the
dump's crossfade absence is real evidence the Electron desktop build hadn't shipped it as of
2026-09-02, not a "the snapshot must predate the feature" excuse (see
`references/quality-playback-queue.md` §7 and §11 — only desktop status stays genuinely open). When
in doubt, treat an absence claim as dated, not final, re-verify against a live install (the
extracted Redux dump lives under `%localappdata%\TIDAL\app-x.xx.x\resources` on Windows,
`/Applications/TIDAL.app/Contents/Resources/app.asar` on macOS — there is no Linux desktop client;
`/opt/tidal-hifi/resources` is the nearest analogue there, via the third-party wrapper), and prefer
the OpenAPI spec (`ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json`, TIDAL API 1.10.104, 256
paths, 993 schemas) for anything you can check against it.

## Owner decisions already made (do not re-litigate these)

- **Platforms now**: desktop Linux + Windows + macOS, and a headless/server/CLI mode. Mobile
  (Android/iOS) is future scope — architecture must not preclude it, but nothing mobile-specific
  ships now.
- **Stack**: open, "something simple but beautiful" — not decided by this skill; see
  `docs/research/tech-stack.md`.
- **TIDAL API approach**: do what High Tide and Sone do — the unofficial API used by python-tidal,
  which requires the user's own paid TIDAL subscription. This is a player for subscribers, not a
  downloader/ripper. Document legal risk honestly; never design or document DRM circumvention or
  piracy tooling. See `docs/research/tidal-api.md` §14 for the full legal posture and how High
  Tide/Sone position themselves.
- **Scope framing**: "everything the native client does, eventually" — but read Implication 15 in
  the main report: of 48 Redux namespaces, roughly a dozen are user-visible features to match,
  another dozen are optional social/creator surfaces, the rest is TIDAL's own analytics/onboarding
  plumbing that is not streamboat's concern.

## The facts that cause silent bugs if you get them wrong

**The general rule behind several of these**: model streamboat's domain types on TIDAL's own names
and enum values (`ref:TidaLuna/plugins/lib/src/redux/types/store/`) — `AudioQuality`, `AudioMode`,
`MediaMetadataTag`, `RepeatMode`, `PlayQueueSourceType`, `StreamingFlags`, `MixType`,
`ArtistRoleCategory`. Cheap now; it makes every API response map 1:1 and defuses pitfalls 1, 2 and 6
below.

| # | Trap | The fix |
| --- | --- | --- |
| 1 | Wire enum `audioQuality: "HIGH"` is the **lossy AAC ~320 kbps** tier; UI label "High" is enum `LOSSLESS`. Enum `LOW` maps to the lowest rung ("Lowest"), which has no current UI label. | Never branch on the enum name looking like an English word. Use the ladder table in `references/quality-playback-queue.md` §1, sourced from `ref:TidaLuna/plugins/lib/src/classes/Quality.ts`. |
| 2 | Playlist visibility is **three-state** (`PUBLIC \| UNLISTED \| PRIVATE`) in the v2 model; the legacy v1 API only exposes a public/private boolean, which cannot represent `UNLISTED`. | Model visibility as an enum from day one, even if you only read the legacy boolean initially. See `references/library-playlists-collections.md` §1. |
| 3 | Playbackinfo terminal sub-statuses are a literal list, not a contiguous range — canonical table owned by `tidal-api/references/transport.md` §6. **4006** (streaming privileges lost) and **4033** (subscription up-sell) are deliberately excluded: both recover, and evicting the track on either is a bug. 4006 is almost certainly the same condition the desktop client surfaces as `STREAMING_PRIVILEGES_REVOKED` (**[inferred]** — not stated in either source; treat as transient/recoverable either way). | Copy the literal list, not a range, into your error classifier; never delete the track on 4006/4033. `references/quality-playback-queue.md` §3. |
| 4 | Force Volume **pins the app's own volume at 100%** so an external DAC/amp is the sole volume control — it is the *opposite* of "software volume when the device can't be controlled." | Get the UI copy/behaviour right; do not build a software-volume fallback under this name. `references/quality-playback-queue.md` §6. |
| 5 | Offline device cap is **5 devices offline, 1 online** — not 3. | Use 5 anywhere an entitlement/device-management screen is built. `references/entitlements-tiers-history.md` §3. |
| 6 | Shuffle is **seeded and reversible** (`shuffleSeed`); unshuffling restores original order. Repeat is `Off=0, All=1, One=2` client-side, but the *server* cloud-queue model uses a different vocabulary (`repeat: NONE\|ONE\|BATCH`, `shuffle: OFF\|BATCH\|ALL` + a separate `shuffled` flag). | Never shuffle destructively. Map explicitly between the client and cloud-queue enums if you touch cloud queue. `references/quality-playback-queue.md` §5. |
| 7 | `home/feed/static` is the **v2 Home page** endpoint, not the social Feed. Two Home schemas coexist (legacy `pages/home`, current `home/feed/{slug}` with tabs and cursor pagination). | Don't conflate Home and Feed when reaching for an endpoint. `references/browse-pages-screens.md` §1. |
| 8 | The favourites-list wrapper on `UserProfile` (`followers`, `followingUsers`, `followingArtists`, `publicPlaylists`) is `{items: [...]}`, not a bare array — and `publicPlaylists.items` holds a *different* element shape (`{itemId, itemType:"playlist"}`) than the other three (`Following[]`). My Collection's own `favorites` object is the opposite shape — bare arrays, not `{items}` — **and its own element types are mixed, not uniformly numeric**: `favorites: {albums: number[], artists: number[], tracks: number[], videos: number[], mixes: string[], playlists: string[], users: string[]}` — it's an id cache, not the list payload itself. | Check the wrapper before assuming array vs object; check numeric vs string id before assuming one type. `references/social-feed-creator.md` §2; `references/library-playlists-collections.md` §2. |
| 9 | Collaborative playlists **exist** (invite/redeem via `/collaborationInvites`) — the desktop client's silence on this is not evidence of absence, just of a mobile/web-first rollout. | Don't scope collaborative playlists as "maybe doesn't exist" — it's a real, documented v2 feature. `references/library-playlists-collections.md` §1. |
| 10 | Crossfade is a **shipping, officially announced 2026 feature** on iOS/Web (0–12s slider). Desktop-client status is the only open question. | Treat as parity work if you build DSP crossfade, not a "differentiator." `references/quality-playback-queue.md` §7. |
| 11 | `tidal://` deep-link grammar **is documented** (in reference clients, not TIDAL's own docs): `tidal://{track,album,artist}/{numeric id}`, `tidal://{playlist,mix}/{string id}`, plus `tidal://my-collection/tracks`. | Implement this, don't treat it as unknowable. `references/remote-playback-connect-controls.md` §5. |
| 12 | `souvlaki` 0.8.3 actually covers all three OSes in one crate, though the reference clients still split by platform anyway — canonical analysis owned by `audio-pipeline/references/os-integration.md` §1 and §5. | Don't budget three integrations by default — evaluate `souvlaki` per platform first (a dedicated crate like `mpris-server` may still win on Linux fidelity), and pick deliberately. `references/remote-playback-connect-controls.md` §4. |
| 13 | TIDAL's own desktop keyboard-shortcut list as reproduced by tidal-hifi is **one wrapper's copy of one unverifiable third-party aggregator** (defkey.com), not TIDAL documentation. | Treat it as a starting point, not gospel; verify against a live install's `modal/SHOW_SHORTCUTS` cheatsheet if precision matters. `references/remote-playback-connect-controls.md` §6. |
| 14 | The action count is **694 entries across 48 namespaces (679 namespaced + 15 bare action names, e.g. `noop`, `ADD_ARTISTS_TO_CATEGORY`)**, not "~600 across ~40" — and that count is dated to one build (see version caveat above). Reproduce the 694 as `grep -oE '"[^"]+"' actionTypes.ts \| wc -l` scoped to the `actionTypes` array only — grepping the whole file over-counts by one (`"@inrixia/helpers"` in the import line). Reproduce the 48 as: extract every quoted string in that array containing `/`, strip everything from `/` onward, exclude `__rtkq/*` (Redux Toolkit Query's own 4 framework actions — library-internal, not a TIDAL namespace), then count distinct prefixes — without excluding `__rtkq` you get 49, not 48. | Cite the number and the build together whenever you quote it; don't recompute the namespace count without excluding `__rtkq`. |
| 15 | The route loaders name **29 distinct screens, not 28** — 28 of them carry `--SUCCESS`/`--FAIL` variants; `LOGIN_AUTH` is the one that doesn't. | Say "29 route loaders (28 with success/fail variants)," not a flat "28 screens." `references/browse-pages-screens.md` §6. |
| 16 | Recently Played on Home (and Daily Discovery/New Arrivals quality) is fed by **`play_log` telemetry to `ec.tidal.com/api/event-batch`**, not by any client-readable history list. Skipping play reporting silently degrades the user's own TIDAL account, not just streamboat's UI. | Implement play reporting early, default-on, exactly as Sone does — don't build Recently Played from a local list. `references/library-playlists-collections.md` §6. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| Quality ladder, badge colours, manifest fetch (v1 + v2), quality cascade, encryption refusal, **transport/player-state** (playbackControls actions, PlaybackState/Player/ActivePlayerType enums, volume scale), gapless, queue semantics (priorities/shuffle/repeat/autoplay), normalization/ReplayGain, exclusive/bit-perfect output, Force Volume, crossfade, voice, desktop **settings surface** (SettingsState + adjacent surfaces), web-player/Linux-platform limits | `references/quality-playback-queue.md` |
| Home/Explore/Search page schemas (v1 + v2), module/section type vocabulary, category titles, **Mixes and radio** (MixType values, actions, routes, endpoints), the 29-route-loader table, editorial content, **Now Playing/fullscreen/mini-player** | `references/browse-pages-screens.md` |
| Playlists (CRUD, folders + pagination, visibility, collaboration, suggested tracks), My Collection (six lists + sort orders + favorites shape + Save for Later), artist/album/track page structure, lyrics, credits/contributor page, **history/Recently Played (play_log)**, **videos** | `references/library-playlists-collections.md` |
| Feed vs Home, public profiles, Picks/prompts, TIDAL Upload, onboarding-step checklist, the 2025–2026 social/creator API layer (comments, reactions, appreciations, artist claims, purchases) | `references/social-feed-creator.md` |
| TIDAL Connect (target vs controller), Chromecast, AirPlay, cloud queue, TIDAL Live/DJ status, OS media-controls per reference project, keyboard shortcuts caveat, deep links, share links, cover-art URLs, **headless-mode control surfaces** (tidal-hifi REST, Sone MCP/OBS, mopidy-tidal browse tree) | `references/remote-playback-connect-controls.md` |
| Subscription tiers, price history (2024–2026), DJ Extension add-on, offline/download data model, streaming-privileges enforcement, MQA/Sony 360RA/podcast removal timeline, **authentication** (device-code vs PKCE, session bootstrap, token storage per OS) | `references/entitlements-tiers-history.md` |
| The full **feature matrix**: every feature tiered MVP/v1/later/out-of-scope, with rationale and API availability/who-implements | `references/feature-matrix.md` |
| The full official v2 OpenAPI catalogue (paths/schemas this report draws on, beyond what's summarized elsewhere) | `references/openapi-v2-catalogue.md` |
| Project→URL mapping for every `ref:<project>` citation | `references/sources.md` |

For streaming wire-format depth beyond what's needed to confirm a feature exists (manifest byte
layout, DRM specifics, auth flows, image URL construction), go to
`docs/research/tidal-api.md` — this skill and the client-features report both point there rather
than duplicating it. For DSP/output engineering, `docs/research/audio-pipeline.md`. For the
headless/CLI control-surface design question, `docs/research/headless-connect.md`.

## Feature-matrix quick reference

Everything below is **MVP**: OAuth device-code + PKCE login, token refresh + secure storage,
`GET sessions`, manifest fetch + quality cascade (exact ladder not yet settled — see
`references/feature-matrix.md` §2), DASH/BTS
manifest parsing, encrypted-manifest detection and refusal, play/pause/seek/volume, seeded
reversible shuffle, repeat off/all/one, queue view (play-next vs add-to-queue as distinct
positions), gapless playback, streaming-quality selector, OS media-control integration
(MPRIS/SMTC/Now Playing) + hardware media keys, search (top hit + 5 core result types),
favouriting anything, My Collection tracks/albums/artists/playlists. Get these right before
anything else. (Streaming-privileges *handling* — one stream at a time, clean stop on revocation —
is tiered **v1** in `references/feature-matrix.md`, not MVP; the two lists agree, don't conflate a
different reading of either.)

**For "is X v1 or later? is Y out-of-scope? who implements Z?"** — every feature TIDAL's native
apps have, tiered MVP/v1/later/out-of-scope with a one-line rationale and its API availability, is
in `references/feature-matrix.md`. Read that instead of asking these questions from memory or
re-opening the 1800-line report.

## Open decisions (feed these into any decision tree / spec-writing task)

> **Resolved by the owner on 2026-09-08/09.** The items below were the inputs to the decision tree; the
> outcomes are recorded in `docs/DECISIONS.md` and distilled in the `streamboat-decisions` skill, which
> takes precedence over any recommendation here. Treat this list as history, not as open questions.

Only the owner (thijs) can resolve these — do not assume an answer when writing code or docs:

1. Social scope: Feed, profiles, followers, Picks, plus the newer comments/reactions/purchases
   layer — include, defer, or never, and at what depth?
2. Video scope: separate HLS pipeline, needed for music videos — include, defer, or never?
3. Offline caching posture for a logged-in subscriber: TIDAL's own `/offlineTasks` store/remove
   model is a candidate shape to mirror — ephemeral cache, pinned-encrypted cache, or nothing?
4. Play-reporting default: on by default (like Sone) or opt-in? (Reporting plays makes the user's
   own TIDAL account/recommendations work correctly, but is telemetry to a third party.)
5. Last.fm/ListenBrainz scrobbling in v1 or later?
6. Which "not in the native app" differentiators to adopt (mini-player, themes, signal-path
   transparency, Discord RPC, local control API/MCP, OBS overlay, proxy support) — each is cheap
   alone, expensive in aggregate.
7. Headless mode's control contract: REST (tidal-hifi-style), MPD-compatible, MPRIS-only, MCP, or
   several? Constrains the core API more than any UI decision.
8. Explicit-content filtering: client-side only in practice — ship in v1 or later?
9. Write-scope policy: how far does streamboat write to the user's real TIDAL account? Recommended
   tiers: read-only → + library writes (favourites/playlists/queue) → + profile/recommendation
   writes (Block, Picks, AI playlists) → + play_log reporting. A bug at the "writes" tier damages
   the subscriber's actual account, not just the local app — treat this as one policy decision.
10. Which TIDAL UI generation to target for parity (old, new, or streamboat's own design consuming
    the same data)? streamboat doesn't touch the DOM, so — unlike the DOM-scraping wrappers — it's
    genuinely free to choose. Make this an explicit call in light of "simple but beautiful."

## Unverified — do not present these as settled fact in specs or code comments

- Crossfade's existence is confirmed; **desktop-client status specifically** is not.
- TIDAL Live/DJ sessions: no trace in the 2026 desktop dump, no discontinuation announcement found.
- Dolby Atmos absence on desktop: ~0.85 confidence, not source-confirmed.
- TIDAL Connect: can the desktop app be a receiver/target now? Stronger evidence than before (a
  user-facing disconnect modal) but still unresolved.
- AI playlist creation (`folders/CREATE_AI_PLAYLIST`): exists in the client, no press/support
  confirmation, rollout state and endpoint unknown.
- Ads in 2026: vestigial plumbing exists post-free-tier-removal; live status unclear.
- Web player HiRes ceiling (24/192 vs 16/44.1 cap): browser/platform-dependent, needs a first-hand
  test.
- Custom playlist cover upload via the unofficial API: unverified.
- Sleep timer / car mode on mobile: not confirmed on TIDAL's own docs.
- Search result types `UPLOADS`/`USERPROFILES`: present in the client filter order, not implemented
  by python-tidal; exact `types=` values unverified. (Recent searches/suggestions/did-you-mean, by
  contrast, are now confirmed documented v2 endpoints — not in the same "unverified" bucket.)
- DJ Extension pricing (~$9/mo) and Serato-stems availability: possibly stale for 2026 — a trade
  article title suggests a price change and stems withdrawal/reinstatement during 2026 that could
  not be confirmed.
- Album normalization's "-14 LUFS, on by default on mobile": sourced only to 2019–2020 coverage, not
  re-verified for 2026.
- Whether a "Podcasts" category survives anywhere as a page-module link despite the 24 Jul 2024
  podcast removal (one client library's doctest fixture still shows it — likely stale).
- Mix cadence/count figures ("Daily Discovery: 10/day," "New Arrivals: 30/Friday"): unfetched
  support-article summaries only.
- The macOS mechanism for exclusive/bit-perfect output ("Core Audio hog mode" is a guess with no
  source); only the Windows WASAPI-exclusive half is attested.
