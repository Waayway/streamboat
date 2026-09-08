# Transport — hosts, headers, pagination, errors, sub-status codes, rate limiting

Full narrative: `docs/research/tidal-api.md` §2 and §4.

## Table of contents

1. Hosts and what lives on each
2. Headers to send
3. Common query parameters
4. Pagination — cursor vs offset, and the real server-side caps
5. Error body shapes
6. The canonical sub-status table
7. Rate limiting
8. Deserialization pitfalls, consolidated — no fixtures exist, and the date format is a trap

---

## 1. Hosts and what lives on each

| Host | Purpose |
|---|---|
| `auth.tidal.com/v1/oauth2/` | `device_authorization`, `token` (all grant types), `logout` |
| `login.tidal.com/` | `authorize` (browser login page). Strawberry additionally posts its token exchange to `login.tidal.com/oauth2/token` rather than `auth.tidal.com/v1/oauth2/token` (`ref:strawberry/src/tidal/tidalservice.cpp:80-81`) — both hosts work, don't assume the token endpoint is always on `auth.tidal.com`. |
| `api.tidal.com/v1/` | tracks, albums, artists, playlists, favorites, pages, playbackinfo, lyrics, credits, `rt/connect` |
| `api.tidal.com/v2/` | `home/feed/*`, `search`, `suggestions/`, `my-collection/playlists/folders`, `favorites/mixes`, `artist/{id}`, `feed/activities`, `profiles/{id}`, `user-playlists/{id}/public` |
| `openapi.tidal.com/v2/` | official Developer API (JSON:API) — see `references/official-api.md` |
| `desktop.tidal.com/v1/` | an alias of the v1 API used by the official desktop app |
| `api.tidalhifi.com/v1` | legacy alias, still used by Strawberry |
| `resources.tidal.com` | images and animated covers — see `references/catalog-and-library.md` §Images |
| `listen.tidal.com`, `tidal.com/browse` | shareable/canonical URLs only — no requests are made against these |
| `ec.tidal.com/api/event-batch` | play logging / streaming metrics — see `references/play-logging-and-privileges.md` |
| `fp.fa.tidal.com`, `api.tidal.com/v2/widevine` | FairPlay / Widevine DRM license servers (official player only) |
| `lgf.audio.tidal.com` | an audio CDN host; mopidy-tidal proxies/caches through it |
| CDN hosts inside manifests | not fixed — signed, expiring URLs; treat as opaque |

Streams are **not** fetched from `api.tidal.com`. The playbackinfo response hands you signed CDN
URLs (BTS) or an MPD whose segment template points at a CDN (DASH). Manifests have a **1-hour
client-side cache lifetime** in the official SDK (`MANIFEST_EXPIRATION_MS = 3600000`,
`ref:tidal-sdk-web/.../playback-info-resolver.ts:80`) — re-resolve after that, and on any CDN 403,
rather than surfacing an error. See `references/playback.md` §Seeking for Range-request and
manifest-expiry handling.

## 2. Headers to send

```
Authorization: Bearer <token>
x-tidal-client-version: <a version string, e.g. 2025.11.3>
User-Agent: <not distinctively yours — see note>
```
The version string is a moving target — it pins a web/app client build and reference projects bump
it periodically (python-tidal: `2025.7.16`; Sone: `2025.11.3`, sent only on `/v2/` URLs). No
reference project sets a distinctive User-Agent identifying itself as a third-party app.

On the legacy playbackinfo call specifically, the official player additionally sends:
`x-tidal-token: <clientId>` (this header is now a **client-id** header, not the legacy API key it
once was), `x-tidal-streamingsessionid: <uuid>`, `x-tidal-playlistuuid: <uuid>` (current queue
context — easy to miss), and `x-tidal-prefetch: true` when prefetching.

**Whether `x-tidal-streamingsessionid` is required for a play to be counted in Recently Played is
not fully answerable from the reference checkouts.** The web SDK generates one and gets it echoed
back as `streamingSessionId`; Sone never sends this header at all yet its plays still surface in
Recently Played via a self-generated `playbackSessionId` in the report event (see
`references/play-logging-and-privileges.md`). The two ids appear to be separate concerns. Cheap,
low-risk recommendation: generate one UUID per playback, send it as `x-tidal-streamingsessionid`
(matches the official client, costs nothing), and reuse it as `playbackSessionId` in reporting.

## 3. Common query parameters

`countryCode` (required), `locale` (`en_US` — note: the **official** API uses BCP-47 hyphenated
locale, `en-US`, a different format), `deviceType` — **only `BROWSER` and `DESKTOP` are attested in
any unofficial-API checkout** (pages default to `BROWSER`, `PageLink.get()` uses `DESKTOP`); the
official v2 schema types it `BROWSER|CAR|DESKTOP|PHONE|TABLET|TV` and the unofficial API plausibly
accepts the same set, but `PHONE` specifically has no attestation anywhere in these checkouts — don't
assume it works — `platform` (`WEB`, only on v2 page endpoints), `limit`, `offset`, `order`,
`orderDirection`.

**This baseline UA/deviceType choice is itself an impersonation posture and deserves a deliberate
decision** (see `references/legal-and-landscape.md` for the disclaimer wording it sits next to).
python-tidal's UA (an Android WebView string) is applied to every request unless overridden; Sone
sends no distinctive UA at all. Recommendation: send an honest `streamboat/<version> (+<url>)` UA by
default, keep the Android string behind a single settings toggle labelled as a compatibility
workaround, default `deviceType=BROWSER`, and test the honest-UA path against a live account before
shipping — nobody in this ecosystem has published whether TIDAL's servers care.

## 4. Pagination — cursor vs offset, and the real server-side caps

Offset/limit with `totalNumberOfItems` in the response body for v1 collections. Get a count cheaply
with `limit=1` and read `totalNumberOfItems`.

The v2 `my-collection/playlists/folders` endpoint is **cursor-based and ignores `offset`** — thread
a `cursor` field instead. The v2 home feed is also cursor-based (`json.page.cursor`).

**Concrete server-side caps** (do not guess these — they were each found the hard way by a reference
project):

- Search returns **at most 300 items total**, regardless of `limit`/`offset`.
- `GET users/{id}/playlistsAndFavoritePlaylists` is **server-capped at 50**.
- `GET users/{id}/favorites/videos` **rejects `limit=10000` outright** — Sone's fix comment: "The
  favorites/videos content endpoint caps page size (a single limit=10000 request is rejected — which
  left every video heart empty on startup). Page through it at the known-good size," paging at
  `PAGE = 50`.
- python-tidal's own docstring: "The maximum item_limit is 10000, and some endpoints have a maximum
  of 100 items ... In cases where the maximum is 100 items, you will have to use offsets."

**Do not copy python-tidal's `Config.item_limit` gotcha**: it injects `limit=1000` (capped 10000)
into *every* request, including ones where that is wrong. Default every collection page to 50, never
send `limit` above 100 on v1 collection endpoints, and treat `totalNumberOfItems` in the response
body as the only pagination authority — not a client-side constant.

## 5. Error body shapes

Two shapes, handle both:

- v1 / legacy v2: `{"status": <http>, "subStatus": <int>, "userMessage": "..."}`.
- newer v2 / openapi v2: `{"errors": [{"detail": "..."}]}`.

## 6. The canonical sub-status table

**This table is the single owner of the `playbackinfo` sub-status taxonomy across every
streamboat skill.** `tidal-oss-landscape`, `tidal-client-features`, `audio-pipeline`,
`streamboat-engineering-baseline` and `tech-stack-evaluation` all reference or test against this
classification — fix it here only, and point cross-references at this section rather than
re-copying the table.

An earlier internal draft of this guide (and several OSS clients) reconstruct this table by
inference. **TIDAL's own Android SDK names every code in the 4xxx range** —
`ref:tidal-sdk-android/player/common/.../ApiError.kt` — use it as the source of truth for the
*names* over any client's guessed classification, but note the SDK assigns no
retryable/terminal classification of its own; the terminal-vs-recoverable column below is Sone's
shipped classification (`ref:sone/src-tauri/src/tidal_api.rs:18`, attested by running code) plus
one narrow inference for 4034 called out below.

| subStatus | Official name | Recommended handling |
|---|---|---|
| 1002 | (not in ApiError.kt — device-flow specific) | client is not a Limited Input Device; tell the user |
| 4005 | `GENERIC_PLAYBACK_ERROR` | terminal for this attempt; retry once, then give up |
| 4006 | `NO_STREAMING_PRIVILEGES` | **recovers** — do not blacklist the track |
| 4007 | `USER_CLIENT_NOT_AUTHORIZED_FOR_OFFLINE` | this client id isn't entitled to offline (see `references/playback.md` §Offline) |
| 4010 | `USER_MONTHLY_STREAM_QUOTA_EXCEEDED` | show a quota message |
| 4020 | `SESSION_NOT_FOUND` | re-login |
| 4021 | `USER_NOT_FOUND` | re-login |
| 4022 | `CLIENT_NOT_FOUND` | client id is invalid/revoked — surface prominently; this is "TIDAL rotated the key" |
| 4023 | `PRODUCT_NOT_FOUND` | skip the track; do not blacklist (may be transient) |
| 4030 | `NO_CONTENT_AVAILABLE_IN_PRODUCT` | terminal for this subscription tier |
| 4031 | `NO_CONTENT_MATCHING_REQUEST` | terminal for the requested parameters |
| 4032 | `NO_CONTENT_MATCHING_SUBSCRIPTION_LOCATION` | region-gated; terminal for this account's region |
| 4033 | `NO_CONTENT_MATCHING_SUBSCRIPTION_CONFIGURATION` | **user-fixable** (upsell) — do not blacklist |
| **4034** | `NO_CONTENT_MATCHING_CLIENT` | **terminal for this request, per Sone's shipped classification** (`ref:sone/src-tauri/src/tidal_api.rs:18`, `TERMINAL_SUB_STATUSES`) — quote this code in the terminal set below, identically. `[inferred]` One narrow retry is defensible before giving up: a *different client id* (the code name is client-scoped), not a lower quality tier — a tier change alters the request parameters, which is what 4031 (`NO_CONTENT_MATCHING_REQUEST`, also terminal) already covers, so retrying 4034 at a lower tier would contradict how 4031 is read in this same table. This client-id retry is this guide's own inference, not attested by any source; TIDAL's Android SDK (`ApiError.kt`) names the code but assigns it no retryable/terminal classification at all |
| 4035 | `NO_CONTENT_MATCHING_PRE_PAYWALL_LOCATION` | terminal for this account's region |
| 6001 | session error | — |
| 11001, 11002, 11003, 11101 | token/auth errors | — |

A 4xxx `subStatus` on a 401 is never fixed by refreshing the token — it's a playback error, not an
auth error. Watch for float-encoded sub-statuses (`4005.0`) in some responses.

## 7. Rate limiting

No published numbers. Two independent developer questions to TIDAL are **confirmed unanswered**
(re-checked directly): `github.com/orgs/tidal-music/discussions/269` (re-verified 2026-09-08: 0
comments, still marked Unanswered) and `.../discussions/285` ("Limitations on requests that can be
made consecutively" — a developer there says "I throttle for 500ms between every request" as their
own practice; this is the only concrete community-sourced number available, not a documented limit).

Observed handling to copy:

- Surface `Retry-After` from a 429 to the caller.
- Run a **global cooldown gate** shared across all requests (Sone's `RateGate`): on a 429, read
  `Retry-After` (delta-seconds form only — the HTTP-date form should be rejected rather than
  mis-parsed), clamp to `1..=120` s, store an absolute deadline with a max-merge so concurrent 429s
  only lengthen it, never shorten it. While cooling down, synthesize a 429 for every caller without
  touching the network. Default cooldown when no header is present: 5 s.
- **Do not walk a quality cascade after a 429 or a terminal sub-status** — a lower tier will not fix
  a rate limit, and doing so only multiplies your request count.
- If you retry at all, retry only GET/HEAD/OPTIONS, with exponential backoff + jitter. The official
  web SDK's formula: `D = min(B * 2^n * j, M)`, `j ∈ [0.8, 1)`, with separate policies per failure
  class (network: base 1s/max 16s/10 retries; status: 500ms/16s/3; timeout: 8s/32s/3) and a 10s
  per-attempt read timeout. Never retry writes.

## 8. Deserialization pitfalls, consolidated

**No example response bodies or captured fixtures exist in any of the 21 reference checkouts.**
python-tidal's own tests hit the *live* API with real credentials; there are no `.json` fixture files
anywhere in the tree. streamboat cannot borrow fixtures from this ecosystem — capture your own
against a live account, before writing parsers, as a concrete first-week task.

**python-tidal's `Config(alac=...)` flag is vestigial — do not model it.** Its docstring makes strong
claims ("`alac=false` will mean that video streams turn into audio-only streams … `num_videos` will
turn into `num_tracks` in playlists"), but the flag is only assigned
(`ref:python-tidal/tidalapi/session.py:140,144`) and never read anywhere else in the module —
confirmed by grep, not just by inspection. It is a leftover from the legacy `x-tidal-token` era; a
port of python-tidal's `Config` should drop this field rather than implement the behavior the
docstring describes.

**Date/timestamp format is a parser trap.** TIDAL emits `2022-09-23T04:52:14.568+0000` — ISO-8601
*basic*-format offset (no colon), not RFC 3339 extended (`+00:00`). Python's `dateutil.isoparse`
accepts both; Rust's `chrono::DateTime::parse_from_rfc3339` and Go's `time.RFC3339` **reject** the
no-colon form. In Rust, use `chrono`'s `%Y-%m-%dT%H:%M:%S%.3f%z` format string or the `iso8601` crate.
Some date fields arrive as bare `YYYY-MM-DD`. Treat every date field as nullable regardless.

**Consolidated nullability/absence traps**, gathered from across every reference file so they aren't
missed one at a time: `bitDepth`/`sampleRate` are optional at every tier (`references/playback.md`
§1). `subStatus` can arrive float-encoded (`4005.0`), not just as an int (§6 above). A play-log
event's `sourceType` is *omitted entirely*, not nulled, when the play has no known container
(`references/play-logging-and-privileges.md`). A token-refresh response may omit `refresh_token`
(`references/auth.md` §5). `keyId` in a BTS manifest is present only when
`encryptionType != NONE` (`references/playback.md` §2). `GET .../favorites/ids` returns every id as
a **string**, even for integer-id entity types (`references/catalog-and-library.md` §6).
`audioMode` can carry values python-tidal's own enum doesn't model — don't fail closed on an
unrecognised value. `playlistsAndFavoritePlaylists` items are `{playlist, created}` wrappers, not
bare playlists (`references/catalog-and-library.md` §7). `PlaybackMode`/`AssetPresentation` are not
modelled as enums anywhere in python-tidal — it only sends the literal strings `"STREAM"`/`"FULL"`;
source `OFFLINE`/`PREVIEW` from the Android SDK instead. A BTS manifest's `codecs` field is not the
raw string to compare against — python-tidal normalizes it with `codecs.upper().split(".")[0]`
(`mp4a.40.2` → `MP4A`) before comparing; normalize the same way, or compare full dotted strings
consistently, but don't mix the two (`references/playback.md` §2).
