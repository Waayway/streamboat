---
name: tidal-api
description: Wire-level knowledge of the TIDAL APIs streamboat talks to — the unofficial api.tidal.com v1/v2 surface every OSS client (High Tide, Sone, python-tidal) streams through, plus the official openapi.tidal.com/v2 Developer API — covering OAuth device-code/PKCE login, client-ID rotation risk, token refresh/storage/logout, headers and rate limiting, playbackinfopostpaywall and manifest/encryption/quality-cascade rules, catalog and pages endpoints, playlist/favorites CRUD, lyrics, images, play-reporting, and legal/ToS posture. Use whenever writing or reviewing code touching api.tidal.com, openapi.tidal.com, auth.tidal.com, login.tidal.com, OAuth/token-refresh/keyring code, a playback-info or manifest parser, quality-tier logic, encryption/DRM handling, playlist/favorites mutation, search, page rendering, or play-history; or when the task mentions TIDAL, python-tidal, tidalapi, High Tide, Sone, playbackinfopostpaywall, manifest, audioquality, HI_RES_LOSSLESS, client_id, device code, PKCE, or subStatus.
---

# TIDAL API for streamboat

Source of truth: `docs/research/tidal-api.md` (the full research report, fact-checked and corrected
— read it for narrative depth, full evidence, and the complete source list). This skill is the
load-on-demand distillation: the facts, endpoints, and pitfalls an implementer needs while writing
code, without re-reading a ~2000-line report every time.

`ref:<project>/<path>` throughout this skill and its references means a shallow, read-only git clone
of a named open-source project kept in the research environment (e.g.
`ref:python-tidal/tidalapi/session.py`) — not part of the streamboat repo. Full project→URL mapping,
commit SHAs, and clone date: `references/sources.md`.

## Owner decisions already made (do not re-litigate these)

- **Platforms now**: desktop Linux + Windows + macOS, and a headless/server/CLI mode. Mobile is
  future scope — the architecture must not preclude it, nothing mobile-specific ships now.
- **Stack**: open, "something simple but beautiful" — not decided by this skill.
- **API approach**: do what High Tide and Sone do — the **unofficial** `api.tidal.com` API used by
  python-tidal, which requires the user's own paid TIDAL subscription. streamboat is a player for
  subscribers, not a downloader/ripper. Never design or document DRM circumvention or piracy
  tooling as a how-to — see `references/legal-and-landscape.md`.
- **The two APIs are not mutually exclusive.** The same Bearer token minted by the unofficial
  device-code/PKCE flow is accepted by `openapi.tidal.com/v2` (the official Developer API). Sone
  creates/edits playlists through the official JSON:API while streaming through the unofficial one.
  streamboat can do the same — see `references/official-api.md`.
- **TIDAL Connect**: not an owner decision — see `headless-and-tidal-connect/SKILL.md` for the
  research verdict (target: never; controller: out of scope, not categorically ruled out) and cite
  that skill rather than restating a verdict here.

## The facts that cause silent bugs if you get them wrong

| # | Trap | The fix |
| --- | --- | --- |
| 1 | `[unverified]` Over-requesting `audioquality` is claimed to return **HTTP 200 with a silently downgraded `audioQuality`**, never an error — but this rests on a single Sone code comment with no second source, and is in tension with tidalt shipping a descending quality ladder (redundant if downgrades are always silent). | Always display the response's `audioQuality`, never the one you requested — that part is safe regardless. The stop-the-cascade-on-error rule that depends on the unverified claim should be confirmed with one live request per tier before being treated as settled. `references/playback.md` §1, §4. |
| 2 | `encryptionType != "NONE"` in a BTS manifest means **do not play it** — Strawberry refuses and says so; TidaLuna's `OLD_AES` decryptor runs *inside the licensed official client* and copying it makes streamboat undistributable. | Refuse, surface a clear message, fall back or skip. Never ship AES/Widevine handling. `references/playback.md` §3. |
| 3 | `subStatus 4034` (`NO_CONTENT_MATCHING_CLIENT`) is **terminal for this request** per Sone's shipped classification — copy the literal terminal set `4005, 4010, 4030, 4031, 4032, 4034, 4035` exactly (4006 and 4033 are deliberately excluded). | `[inferred]` A single narrow retry with a *different client id* is defensible before giving up on the track; do **not** retry at a lower quality tier — that's a different request (4031's case), not a different client. `references/transport.md` §6 (canonical; do not restate this table elsewhere). |
| 4 | The device-code flow's response is **camelCase** (`deviceCode`, `userCode`, `expiresIn`...), not the snake_case RFC 8628 spells; `verificationUriComplete` comes back **without a scheme**. | Prepend `https://` yourself; don't assume RFC-standard field names. `references/auth.md` §1. |
| 5 | The PKCE token exchange sends `scope=r_usr+w_usr+w_sub` with **literal `+` characters**; the device flow sends the same scopes with **spaces**. Both are correct for their own flow — this is not a bug to "fix" into consistency. | Match each flow's own convention exactly. `references/auth.md` §2. |
| 6 | `client_unique_key` must be **generated once and persisted**, not regenerated per process (python-tidal's mistake). TIDAL treats it as device identity and the official SDK **throws on refresh if it changes**. | Store it next to the refresh token at first login; never regenerate. `references/auth.md` §9. |
| 7 | The browser PKCE flow is **reCAPTCHA v3 gated**; the device-code flow is not. | Never script a headless POST to `login.tidal.com`. Device code is the reliable flow for headless/CLI for this reason, not only for convenience. `references/auth.md` §10. |
| 8 | `playbackmode=OFFLINE` / official `usage=DOWNLOAD` asks TIDAL for a **licensed, DRM-bound, expiring** asset — a materially different thing from caching an already-cleartext stream. | Build a transparent HTTP byte cache if offline is wanted at all; rule out `OFFLINE`/`DOWNLOAD` by policy, don't treat it as "offline, but more". `references/playback.md` §11. |
| 9 | The official `/trackManifests/{id}` endpoint returns a **DRM-protected** manifest (Widevine/FairPlay `drmData`) — this blocks a native player at the codec level, independent of the contractual "Player-module-only" restriction. | Two separate blockers, not one. A native GStreamer/Qt/GTK client cannot decode official manifests even if the contract were ignored. `references/official-api.md` §4-5. |
| 10 | Real, reproduced server-side pagination caps exist and are not "some endpoints max at 100": search caps at 300 total, `playlistsAndFavoritePlaylists` caps at 50, `favorites/videos` **rejects `limit=10000` outright**. | Default every collection page to 50; never send `limit` above 100 on v1 collection endpoints. `references/transport.md` §4. |
| 11 | `api.tidal.com` is very likely **not CORS-enabled** for browser origins (unlike the official API) — a webview UI cannot call it directly. | Put the API client in native/backend code with no browser-origin dependency, regardless of UI toolkit. `references/official-api.md` §6 (flagged partially unverified — re-check with a live preflight). |
| 12 | No dedicated REST endpoint for recently-played, charts, or collaborative playlists exists on the **unofficial** API — confirmed absent by grep across 21 checkouts, not merely unfound. Recently Played specifically exists only as the page slug `pages/my_collection_recently_played`; history is otherwise a side-effect of play reporting (trap 13). | Don't hunt for a dedicated endpoint; use the page slugs / mix types documented, or the official API where they *are* modeled. `references/catalog-and-library.md` §4, §8. |
| 13 | Writing plays back to TIDAL's Recently Played requires **impersonating TIDAL's Android client** (fixed `app-version`/`device-model`/`os-name` in the event payload) — there is no legitimate-looking way to do this. | This is a real policy decision, not an implementation detail — see Open decisions below. `references/play-logging-and-privileges.md` §3. |
| 14 | A token-refresh **failure** needs a real taxonomy, not "retry or give up": network error (status 0) ≠ fatal 4xx ≠ retryable 5xx, and a `clientUniqueKey`/scope mismatch is fatal *before* the network call. Get this backwards and streamboat either logs users out on a flaky network, or loops forever on a revoked token. | Copy the official SDK's classification wholesale. `references/auth.md` §12. |
| 15 | `bitDepth`/`sampleRate` are **not confirmed null only for LOW/HIGH** — an earlier draft got the tiers wrong (python-tidal's own comment says LOW and legacy `HI_RES`) and the web SDK gives no tier qualification at all. | Treat both fields as optional at every tier; prefer the DASH `Representation@id` triple when a manifest is present. `references/playback.md` §1. |
| 16 | `tidal://` as a custom URI scheme is claimed by **three different things** in this ecosystem: Strawberry's OAuth redirect, Sone/High Tide's content deep links, and the official TIDAL desktop app's own registration. Reusing it for streamboat's OAuth redirect risks a redirect landing in the wrong app. | Register a distinct scheme (e.g. `streamboat://`) for both OAuth redirect and content deep links; never claim `tidal://`. `references/auth.md` §13. |
| 17 | `EAC3_JOC` in the official `/trackManifests/{id}` `formats[]` ladder is attested **only on the Android SDK** — the web SDK and `tidal-cli` never request it, and Android additionally gates `FLAC_HIRES` on `HI_RES_LOSSLESS` specifically (not the legacy `HI_RES` tier). | Don't assume the web player requests immersive formats; follow the (more permissive) web SDK grouping if calling this endpoint directly. `references/playback.md` §10. |
| 18 | The account's **quality ceiling comes from `GET /v1/users/{id}/subscription`'s `highestSoundQuality`** — starting the cascade at `HI_RES_LOSSLESS` for every user burns 2-4 wasted requests per track for non-Max subscribers. | Fetch and cache this at login; clamp the cascade to `min(preference, highestSoundQuality)`. `references/auth.md` §6. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| Device-code + PKCE login, scopes, client-ID provenance/rotation, refresh, logout, secure storage, `client_unique_key`, reCAPTCHA, runtime client-id discovery | `references/auth.md` |
| Hosts, headers, pagination (+ real server caps), error shapes, the canonical sub-status table, rate limiting | `references/transport.md` |
| `playbackinfopostpaywall`, manifest types (BTS/DASH/EMU/HLS), encryption refusal rule, quality cascade, `urlpostpaywall`/`streamUrl` fallbacks, video, audio modes/codecs, replay gain, seeking/manifest lifetime, official `/trackManifests/{id}` contract, offline/DRM distinction | `references/playback.md` |
| Search (v1+v2), entity endpoints, track/media fields, Pages API (v1+v2 Home/Explore/artist/mix), favorites, playlists+folders (ETag, TRN scheme), lyrics (LRC format), image URL construction and valid sizes, locale/availability | `references/catalog-and-library.md` |
| Play-reporting wire format (`ec.tidal.com`), the impersonation tradeoff, server-anchored timestamps, streaming-privileges websocket (`rt/connect`) | `references/play-logging-and-privileges.md` |
| The official Developer API's real surface (256 paths — `/dynamicPages`, `/userCollections`, `/playQueues`, collaborative playlists, offline), its own auth flows, the `@tidal-music/player` "compliant-but-Chromium" architecture option, CORS | `references/official-api.md` |
| Project-by-project comparison table, what TIDAL's own ToS/Guidelines say (with confidence levels), enforcement history, the disclaimer wording to copy, packaging feasibility per channel, TIDAL Connect | `references/legal-and-landscape.md` |
| Project→GitHub URL mapping, commit SHAs, clone date, which blocked hosts are second-hand | `references/sources.md` |

## Quick orientation — the shape of the whole system

Two hosts matter most: `api.tidal.com/v1` and `/v2` (unofficial, undocumented, what actually streams
audio) and `openapi.tidal.com/v2` (official, documented, JSON:API, contractually the only sanctioned
way to play audio yourself — but its manifests are DRM-protected regardless). Auth lives at
`auth.tidal.com/v1/oauth2/*` and `login.tidal.com/authorize` for both. A Bearer token from either
flow works against either API. The core playback call is
`GET /v1/tracks/{id}/playbackinfopostpaywall`, which returns a base64 manifest (BTS JSON or DASH
XML) pointing at signed, expiring CDN URLs — never TIDAL's own API host. See
`references/playback.md` for everything past this paragraph.

## Minimal working path

The order matters — get this sequence right before writing anything else:

1. Generate a `client_unique_key` once and persist it; never regenerate it. `references/auth.md` §9.
2. Log in via device-code (headless/CLI default) or PKCE, against `auth.tidal.com/v1/oauth2/*` and
   `login.tidal.com/authorize`. `references/auth.md` §1-§2.
3. `GET /v1/sessions` → `countryCode`, `userId`. `references/auth.md` §6.
4. `GET /v1/users/{userId}/subscription` → `highestSoundQuality`; cache it. `references/auth.md` §6.
5. Browse or search to find something to play. `references/catalog-and-library.md` §1, §5.
6. `GET /v1/tracks/{id}/playbackinfopostpaywall` with `audioquality` clamped to
   `min(user preference, highestSoundQuality)`. `references/playback.md` §1, §4.
7. Decode the base64 `manifest`; refuse anything with `encryptionType != "NONE"`.
   `references/playback.md` §2-§3.
8. Play it: BTS `urls[0]` directly, or wrap the DASH base64 as a data URI for the pipeline.
   `references/playback.md` §2.
9. Optional: report the play to `ec.tidal.com` past 30 seconds of playback.
   `references/play-logging-and-privileges.md` §1-§3.

Step 4 must precede step 6 — starting the quality cascade at `HI_RES_LOSSLESS` for every user wastes
2-4 requests per track (trap 18). Step 1 must precede step 2's first `authorize`/
`device_authorization` call — regenerating the key later burns through TIDAL's device cap (trap 6).

## Open decisions (feed these into any decision tree or spec-writing task)

> **Resolved by the owner on 2026-09-08/09.** The items below were the inputs to the decision tree; the
> outcomes are recorded in `docs/DECISIONS.md` and distilled in the `streamboat-decisions` skill, which
> takes precedence over any recommendation here. Treat this list as history, not as open questions.

Only the owner (thijs) can resolve these — do not assume an answer when writing code or docs:

1. **Official API or unofficial API as the primary spine?** Unofficial is the only path to playing
   audio yourself; official is opportunistically useful for metadata/playlists/lyrics/editorial
   pages that the unofficial API also happens to cover, plus ISRC/UPC lookup which it covers alone.
   - **1a. "Compliant-but-Chromium" vs "native-but-unofficial."** `@tidal-music/player` is the one
     fully ToS-compliant playback path but needs a Widevine-CDM browser runtime and cannot run
     headless or with a native audio pipeline. Given the headless/server requirement,
     native-but-unofficial is close to forced — state it as a decision. `references/official-api.md` §5.
2. **Ship the ecosystem client ID, or require the user to supply one?** A hybrid (ship it, allow
   override, document what it is — Sone's pattern) is the default recommendation.
   `references/auth.md` §4, §11.
3. **Write plays back to TIDAL's Recently Played?** Requires impersonating TIDAL's Android client.
   Recommend opt-in, off by default, with the impersonation stated plainly in the setting's
   description. `references/play-logging-and-privileges.md` §3.
4. **Offline cache scope.** A transparent HTTP byte cache of an already-cleartext stream is a
   *different thing* from requesting `playbackmode=OFFLINE`/`usage=DOWNLOAD`, which is licensed and
   DRM-bound. Recommend building only the former, ruling out the latter by policy.
   `references/playback.md` §11.
5. **How much does Hi-Res matter?** Committing to `HI_RES_LOSSLESS` means committing to the PKCE
   flow (reCAPTCHA-gated browser handoff); a LOSSLESS-only v1 with device-code login is simpler.
6. **Video in or out for v1?** A whole second pipeline (HLS, video surface, separate quality
   setting). `references/playback.md` §6.
7. **Which control-surface contract for headless mode?** REST, MPD-compatible, MPRIS-only, MCP, or
   several — constrains the core API layer more than any UI decision.
8. **Write-scope policy.** How far does streamboat write to the user's real TIDAL account? A bug at
   the "writes" tier damages the subscriber's actual account, not just the local app.
9. **Social features scope (follow, activity, sharing, artist tools).** No reference client models
   follow/unfollow on the unofficial API at all; the official API has the full model (collaboration,
   sharing links, artist-profile editing including a presigned-upload cover-art flow). Pick a tier
   for v1 rather than discover the API boundary mid-implementation. `references/catalog-and-library.md`
   §7, `references/official-api.md` §8.
10. **Multi-account / profile switching.** No reference client supports more than one TIDAL account
    per install. Decide now whether the credential-storage schema needs to be account-keyed from day
    one — retrofitting it later is a migration, not a flag. `references/auth.md` §14.
11. **User-Agent / `deviceType` honesty.** Every reference client either impersonates TIDAL's own
    Android app or sends nothing distinctive; none has published whether an honest `streamboat/…` UA
    still works. Decide the default and test it against a live account before shipping.
    `references/transport.md` §3.

## Unverified — do not present these as settled fact in specs or code comments

- **Rate-limit numbers.** No published figures; two independent developer questions to TIDAL
  (`tidal-music` Discussions #269 and #285) are **confirmed unanswered** (re-checked directly).
  `references/transport.md` §7.
- **Whether a newly-registered third-party client gets full-track manifests or only 30-second
  previews from the official `/trackManifests/{id}`.** `tidal-music` Discussion #179 raises the
  doubt with no answer. `references/playback.md` §10.
- **The signed-URL TTL inside a BTS manifest's `urls[0]`** (distinct from the 1-hour client-side
  manifest cache, which *is* confirmed). `references/playback.md` §9.
- **Whether `x-tidal-streamingsessionid` must correlate with the play-log's `playbackSessionId`**
  for a play to surface in Recently Played. `references/play-logging-and-privileges.md` §3.
- **`api.tidal.com`'s CORS posture** — asserted from architecture alone; the community-doc citation
  once offered in support turned out on re-check to be about client-id retrieval, not CORS headers,
  so it's weaker-sourced than an earlier draft implied. No live preflight exists.
  `references/official-api.md` §6.
- **Whether `sessionId` can be safely omitted on every endpoint** (Sone omits it and works; untested
  against endpoints Sone doesn't exercise that python-tidal does — `pages/*`, `genres`,
  `urlpostpaywall`). `references/auth.md` §6.
- **The exact text of TIDAL's Developer Terms, Developer Guidelines, and consumer Terms** —
  `developer.tidal.com`, `support.tidal.com`, `tidal.com` were all blocked from direct fetch in the
  research environment; every quotation in `references/legal-and-landscape.md` is second-hand via
  search excerpts. Read the pages directly before any of that wording goes into a public-facing
  document.
- **Flathub's current policy stance** on apps embedding credentials extracted from proprietary
  clients — tolerated in practice (both High Tide and Sone are listed), no policy statement found.
- **Whether the shared ecosystem client-ID pair is still valid as of any given date** — TIDAL rotates
  these periodically (confirmed via python-tidal's changelog); no TIDAL account was available in this
  research pass to test it. **Recommend this as streamboat's literal first milestone**: run
  python-tidal's device-code login against the ecosystem pair with a live paid subscription before
  committing further engineering to the unofficial-API spine, and record the date and result.
  `references/legal-and-landscape.md` §2.
- **No example response bodies or captured fixtures exist in any of the 21 reference checkouts** —
  every field list in this skill was read from parsing code, not from a sample. Capture your own
  fixtures against a live account before writing strict deserializers. `references/transport.md` §8.
- **`x-tidal-client-version` semantics** — whether a stale/out-of-date value degrades or blocks
  anything is unknown. python-tidal pins `2025.7.16`, Sone pins `2025.11.3`; neither codebase
  explains why that specific value, or what happens if it drifts. `references/transport.md` §2.
