# Entitlements, tiers, pricing history, and offline model

Table of contents:
1. Tier structure and pricing history
2. DJ Extension add-on
3. Offline / download model
4. Streaming-privileges enforcement
5. Account model the client reads
6. Notable feature additions/removals, 2024–2026
7. Authentication: device-code vs PKCE, session bootstrap, token storage

## 1. Tier structure and pricing history

One product, three billing shapes since the March–April 2024 consolidation (announced 6 Mar 2024,
**effective 10 April 2024**): HiFi and HiFi Plus merged into a single subscription; every paying
plan gets lossless and HiRes FLAC; the Free ad-supported tier was discontinued the same day;
Student dropped to $4.99; Military/First-Responder discounts ended 10 June 2024.

| Plan | US price (Aug 2026) | Notes |
| --- | --- | --- |
| Individual | $11.99/mo (was $10.99) | Rose on the first billing date on/after 3 Aug 2026 |
| Family | $19.99/mo (was $16.99) | Up to 6 accounts |
| Student | $6.99/mo (was $5.49) | Went to $4.99 at the Apr 2024 consolidation, then rose again by Aug 2026 |
| Free (ad-supported) | Discontinued 10 April 2024 | Was ~160 kbps, shuffle-only — cadence/bitrate detail is aggregator-only, **[uncertain]** |

Sources for the Aug 2026 figures: ecoustics.com/news/tidal-price-increase-2026/ and
neowin.net/news/tidal-is-getting-a-price-hike/ — two independent trade sources agree on the exact
numbers; treat as solid [verified-web], not merely indicative.

**What the tier gates** (quality itself is *not* gated post-2024-consolidation):
- Playback at all (`premiumStreamingOnly`, `payToStream`, `allowStreaming`, `streamReady` per item).
- DJ software integration and stems (`djReady`, `stemReady` per item) behind the DJ Extension.
- Offline downloads: subscriber-only, **5 devices offline / 1 device online simultaneously** (see
  §3 — not 3, a figure that appears wrong in several older summaries of this material).
- Simultaneous streams: one at a time (see §4).

## 2. DJ Extension add-on

~$9/mo, requires a base plan (not confirmed as "Individual or Student only" specifically),
integrates TIDAL as a source in Serato DJ Pro, rekordbox, djay Pro, VirtualDJ, DJUCED, with stem
separation (Serato DJ Pro 3.1.3+ required for stems,
support.serato.com/hc/en-us/articles/360000588435).

**[uncertain, possibly stale for 2026]**: a 2026 trade-article title —
"Big Price Increase For DJs Using Tidal, But Stems Coming Back"
(digitaldjtips.com/big-price-increase-for-djs-using-tidal/) — indicates the pricing and stems
availability both changed during 2026. Could not be fetched (EGRESS_BLOCKED) or corroborated here.
**Re-check this article and tidal.com/djs before publishing a DJ Extension price anywhere.**

## 3. Offline / download model

Native desktop has **no** offline/download capability — no `offline/` or `download/` Redux
namespace exists in the desktop action list. Offline is mobile/tablet/watch only.

**Device cap: 5 devices offline, 1 device online, simultaneously** — this is the figure to use
everywhere; the "3 devices" figure that circulates elsewhere is wrong. Source:
support.tidal.com/hc/en-us/articles/201623252, "How Many Devices Can I Use Simultaneously?" —
**[verified-web, unfetched]** like its neighbours in this section (support.tidal.com is blocked to
direct fetch in the research environment, so this is a search-engine-summary citation, not a full
read); re-confirmed against the support-article summary during this skill's 2026-09 review pass, so
treat it as solid despite the tag, not as a claim still needing a first look.

`UserClient` carries `authorizedForOffline`/`authorizedForOfflineDate` per client — the mechanism,
not the count.

**A concrete server-side model exists and is a candidate shape for the owner's "logged-in
subscriber cache" question** (see SKILL.md "Open decisions" #3):
- `/offlineTasks` and `/offlineTasks/{id}`: `OfflineTasks_Attributes = {action: STORE|REMOVE, state:
  PENDING|IN_PROGRESS|FAILED|COMPLETED, position, volume}`, relationships `item`, `collection`,
  `owners`.
- `/installations` and `/installations/{id}/relationships/offlineInventory` — per-device offline
  inventory, the server-side counterpart of `UserClient.numberOfOfflineAlbums`.
- `/downloads`, `/downloads/{id}`: `Downloads_Attributes = {downloadLinks: [{href, meta}]}`,
  `/tracks/{id}/relationships/download`.
- `/userOfflineMixes/{id}` reachable from `/userRecommendations/{id}/relationships/offlineMixes`.
- `UserSubscription.offlineGracePeriod` is the expiry clock for all of this.

Mirroring TIDAL's own store/remove task model and per-installation inventory — with the grace
period as the invalidation rule — is a defensible design for a bounded, logged-out-invalidated
subscriber cache. It is explicitly not the basis for an export/rip feature; streamboat is a player
for subscribers, not a downloader.

MQA-specific note: deleted MQA downloads had to be manually re-downloaded as FLAC after the 24 Jul
2024 MQA removal (§6).

**Mobile-only additions beyond the offline cap — these bound the future-scope architecture, not
just a features list** (report §9, `[verified-web]` unless noted): a separate **Mobile-data vs
Wi-Fi streaming quality** split (a third, independent quality setting alongside streaming/download
quality); **Dolby Atmos playback** (mobile/TV/car only, never desktop); **CarPlay** (full app
projection, car volume controls, offline mode limited to downloaded My Collection content only —
`[verified-web, unfetched support article]`); **Android Auto** (same, plus voice search and
shuffle/repeat/track-radio on Now Playing — `[verified-web, unfetched support article]`);
**Android Automotive** (no downloads, no offline mode, no quality badge on Now Playing, but
streaming quality is still settable — `[verified-web, unfetched support article]`); **Apple
Watch/Wear** (playback control and, on Watch, its own offline listening). None of this is buildable
now (mobile is future scope, per SKILL.md "Owner decisions already made"), but each is a reason the
core (quality selection, offline/cache model, playback control surface) must not hard-code
desktop-only assumptions — Implication 12 in the main report names the two concrete constraints
this binds today, not just a future aspiration: **(1) no GTK/Qt assumptions in the core** — a
desktop-only UI toolkit dependency in the shared core would have to be ripped out to ever reach
mobile, so keep the core UI-toolkit-agnostic even while only building desktop+headless now; **(2)
no desktop-only crypto for token storage** — e.g. a Windows-DPAPI-only or macOS-Keychain-API-only
token encryption call baked into the core (rather than behind a per-OS storage interface, see
§7 below) would be a rewrite, not a port, once a mobile keychain/keystore is added.

## 4. Streaming-privileges enforcement

One stream at a time — per subscription slot. The Family plan carries 6 simultaneous online
streams (one per member), same source as §3, so revocation may not reproduce on a Family account
during testing; still handle it, since Individual/Student accounts hit it constantly. A second
device on the same slot kicks the first. `player/STREAMING_PRIVILEGES_REVOKED`
in the desktop client, plus a whole `streaming-privileges` module in the official Android SDK
(`ref:tidal-sdk-android/player/streaming-privileges/` — acquire/connect websocket protocol,
`AcquireRunnable`, `IncomingWebSocketMessageParser`, `SocketConnectionState`). This is
server-enforced regardless of what streamboat does; the revocation must be *handled* (shown as a
clean stop, not a crash/hang) even if streamboat doesn't implement the websocket itself.

## 5. Account model the client reads

`ref:TidaLuna/plugins/lib/src/redux/types/store/User.ts`:

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

Notable: `earlyAccessProgram` (EAP), `artistId` (artist accounts), `nostrPublicKey` (Block/Square
lineage), `parentId` (family sub-accounts).

**API**: `users/{id}/subscription` and `sessions` both reachable through the unofficial API
(`ref:python-tidal/tidalapi/{user,session}.py`).

## 6. Notable feature additions/removals, 2024–2026

| When | Change |
| --- | --- |
| 6 Mar 2024 (announced) → 10 Apr 2024 (effective) | Tier consolidation; Student to $4.99; Free tier discontinued; Military/First-Responder discounts ended 10 Jun 2024 |
| 17 Jun 2024 (announced) → 24 Jul 2024 (effective) | **MQA removed**; **Sony 360 Reality Audio removed**; **all podcasts removed** (same change); catalogue re-served as FLAC; offline MQA downloads had to be re-downloaded; 360RA tracks greyed out |
| 2024→2026 | HiRes FLAC (24/192) becomes the "Max" UI tier name; Low/High/Max replaces Normal/High/HiFi/Master |
| ~2024–2025 | Public profiles, Picks/prompts, Feed, blocking, user-profile search results mature |
| 2025 (schema examples dated 2025-09/2025-11) | A social/creator API layer ships: comments, reactions, appreciations, artist claims, purchases, collaborative-playlist invites — see `social-feed-creator.md` §6 |
| Nov 2025 | **TIDAL Upload** launches (creator uploads, Spotlight, $100k Upload Headliners contest) |
| 2025–2026 | Desktop/web UI redesign rolls out; two UIs coexist |
| Possibly during 2026 | DJ Extension pricing reportedly changed, stems withdrawn/reinstated — **[uncertain]**, unconfirmed |
| Aug 2026 | Price increase (§1) |
| Mar–Jun 2026 | **Crossfade reintroduced** on iOS/Web — officially announced by TIDAL, 0–12s slider |
| Ongoing | Dolby Atmos remains mobile/TV/car only, not on desktop |
| Unclear | TIDAL Live (DJ sessions, Apr 2023) has no trace in the 2026 desktop client — **[uncertain]** |

Podcasts specifically: removed 24 Jul 2024 in the same change as MQA/360RA
(ecoustics.com/news/tidal-drops-mqa-360ra-podcasts/) — use this dated fact rather than the weaker
absence-based argument ("no podcast `ContentType` exists"). One OSS library's doctest fixture
(`ref:python-tidal/docs/pages.rst`) still lists a "Podcasts…" page-module category despite the
removal — likely a stale fixture; worth one live-account check before relying on either claim.

## 7. Authentication: device-code vs PKCE, session bootstrap, token storage

Full wire-format detail (request/response bodies, retry/poll timing, error codes) is in
`docs/research/tidal-api.md` §3 — this section is the feature-level summary an implementer needs
to scope MVP auth work without opening that file.

**Two flows, and the choice isn't cosmetic**: OAuth **device-code** (RFC 8628 — user visits
link.tidal.com and enters a code; the only flow that works headless/CLI/TV) and OAuth
**authorization-code + PKCE** (browser redirect). **PKCE is required to unlock
`HI_RES_LOSSLESS`** — python-tidal's own `login_pkce` docstring calls it "the only way" to get
Hi-Res — but PKCE *costs* you the direct-URL shortcut: `Track.get_url()` (the `urlpostpaywall`
shortcut) raises `URLNotAvailable` whenever `session.is_pkce` is true, not the other way around
(`ref:python-tidal/tidalapi/media.py:410-421`). A PKCE session must always go through
`playbackinfopostpaywall` + manifest parsing; it can never take the cheap `urlpostpaywall` path.
Device-code sessions *can* use `urlpostpaywall`, but are capped below Max quality. Practical
consequence: a CLI/headless mode built device-code-only is capped below the top quality tier by
construction, not by any server-side headless restriction — ship both flows in MVP if Max-tier
quality is an MVP requirement (SKILL.md "Feature-matrix quick reference" already lists both as
MVP). This also strengthens the case for building the `playbackinfopostpaywall` + manifest-parsing
path first: every PKCE session needs it regardless of whether the device-code-only
`urlpostpaywall` shortcut is ever implemented.

**`GET sessions` (or the OAuth token response, for Strawberry) supplies `countryCode`**, and
`countryCode` is mandatory on essentially every subsequent catalogue call — python-tidal injects it
into every request. Fetch and cache this immediately after auth, before making any other API call.

**Token storage is a genuinely per-OS problem, not a single library call.** Per-OS mechanism table
and the keyring-plus-encrypted-file-fallback recommendation are owned by
`streamboat-engineering-baseline/references/secrets-and-tokens.md` §2-3 — cite it rather than
restating; the same "no single crate covers every platform" shape recurs for OS media controls
(`remote-playback-connect-controls.md` §4, owned by `audio-pipeline/references/os-integration.md`
§1 and §5).
