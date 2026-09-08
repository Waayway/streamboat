# Playback reporting and streaming privileges

Full narrative: `docs/research/tidal-api.md` §10. This is the one area where "do what High Tide and
Sone do" runs into an honest tradeoff — read the "impersonation" note below before implementing it.

## Table of contents

1. The event producer (`ec.tidal.com/api/event-batch`)
2. The `playback_session` payload
3. Sone's play-logging rules
4. Server-anchored timestamps (`@tidal-music/true-time`, `GET /v1/ping`)
5. Streaming privileges (`rt/connect`)

---

## 1. The event producer

TIDAL's clients log plays to `https://ec.tidal.com/api/event-batch`. Wire format is an **AWS SQS
`SendMessageBatch` form POST**, max 10 events per batch:
```
SendMessageBatchRequestEntry.N.Id                                   = <uuid>
SendMessageBatchRequestEntry.N.MessageBody                          = <event JSON>
SendMessageBatchRequestEntry.N.MessageAttribute.1.Name               = Name
SendMessageBatchRequestEntry.N.MessageAttribute.1.Value.StringValue  = playback_session
SendMessageBatchRequestEntry.N.MessageAttribute.1.Value.DataType     = String
SendMessageBatchRequestEntry.N.MessageAttribute.2.Name               = Headers
SendMessageBatchRequestEntry.N.MessageAttribute.2.Value.StringValue  = <headers JSON>
SendMessageBatchRequestEntry.N.MessageAttribute.2.Value.DataType     = String
```
Corroborated independently by the official web SDK's `sqsParamsConverter.ts`.

## 2. The `playback_session` payload

Sone's "mobile shape" (its tests assert this is what surfaces in Recently Played):
```json
{
  "group": "play_log",
  "version": 2,
  "ts": <endTimestampMs>,
  "uuid": "<uuid>",
  "user":   { "id": <uid>, "clientId": <cid>, "sessionId": "<sid>" },
  "client": { "token": "<cid as string>", "deviceType": "mobile",
              "version": "2.205.0", "platform": "android" },
  "payload": {
    "playbackSessionId": "<uuid>",
    "isPostPaywall": true,
    "productType": "TRACK",
    "requestedProductId": "<id>",
    "actualProductId": "<id>",
    "actualAssetPresentation": "FULL",
    "actualAudioMode": "STEREO",
    "actualQuality": "LOSSLESS",
    "startTimestamp": <ms>, "endTimestamp": <ms>,
    "startAssetPosition": 0.0, "endAssetPosition": <seconds>,
    "actions": [],
    "sourceType": "ALBUM|PLAYLIST|ARTIST|MIX",   // omitted entirely when unknown, not sent as null
    "sourceId": "<id>"
  }
}
```
Per-event `Headers` attribute — **nine keys exactly**: `client-id`, `app-version`, `os-name`,
`os-version`, `device-model`, `device-vendor`, `consent-category` (`NECESSARY`),
`requested-sent-timestamp`, `authorization` (bare token, no `Bearer` prefix — that lives on the HTTP
header only). `uid`/`cid`/`sid` come from decoding the JWT access token's middle segment **without
signature verification**.

The official web SDK sends the same conceptual event with one-for-one matching field names, via its
`event-producer` package — whose README says plainly: "This module is only intended for internal use
at TIDAL, but feel free to look at the code." It also sends `streaming_metrics` events:
`streaming_session_start`, `streaming_session_end`, `playback_info_fetch`, `drm_license_fetch`,
`playback_statistics`.

## 3. Sone's play-logging rules

All asserted in its own tests:
- Log a play only past **30 seconds** — "TIDAL's own rule: a play over 30 seconds counts as a
  stream."
- A **sourceless play is accepted but produces no Recently-Played row** — map every UI surface that
  has a real container id (`album`, `playlist`, `artist`, `mix`, plus `radio`→MIX and
  `artist-tracks`→ARTIST) and leave the rest unmapped.
- Outcome classification: 401/403 → refresh once then queue; 5xx → requeue; other 4xx or a
  `BatchResultErrorEntry` in the body → drop permanently ("SenderFault"), never retry.
- Events persist to an encrypted on-disk queue so they survive restarts.
- **It deliberately impersonates the TIDAL Android client**: `app-version` pinned to `2.205.0`,
  `os-name: Android`, `device-model: Pixel 7`, `device-vendor: Google` — with a test asserting the
  payload contains neither its own app name nor its own version string. Its own source comment:
  "Private, undocumented endpoint — the same posture as SONE's existing streaming calls;
  best-effort, may not surface." Events ride on that client's token, so they must describe that
  client, not the app sending them.

**This is a real decision for streamboat, not a detail** — the only known way to write to Recently
Played is to send events that claim to come from TIDAL's Android app. See "Open decisions" in
`SKILL.md`. Local scrobbling (Last.fm/ListenBrainz) is the uncontroversial alternative and every
reference project has it.

**Whether `x-tidal-streamingsessionid` (sent on the playbackinfo call, see `references/transport.md`
§2) needs to correlate with `playbackSessionId` here for a play to actually surface in Recently
Played is not confirmed** — Sone never sends that header at all and its plays still surface, via a
self-generated `playbackSessionId` alone. Treat the two ids as separate concerns unless proven
otherwise; cheapest safe move is to generate one UUID per playback and reuse it for both.

## 4. Server-anchored timestamps

If streamboat logs plays back to TIDAL, derive event timestamps from a server-anchored clock, not
`SystemTime::now()` — TIDAL ships a dedicated package for exactly this reason.
`@tidal-music/true-time`: "Small library to sync time between client and server, used to ensure
event timestamps are accurate." It works by fetching a URL and reading the HTTP `Date` response
header, re-syncing when the cached value is over an hour old; its own tests point it at
`https://api.tidal.com/v1/ping` — an unauthenticated liveness/time endpoint with no other documented
use. `GET /v1/ping` is also useful standalone as a connectivity/health probe for the headless mode,
before showing a login error.

A clock-skewed client produces event timestamps that get silently dropped or misordered
server-side — this is exactly the kind of bug that is very hard to attribute from client logs alone.

## 5. Streaming privileges (`rt/connect`)

```
POST https://api.tidal.com/v1/rt/connect
```
→ `{ url: "<websocket url>" }`. The client opens that websocket and uses it to *acquire* streaming
privileges (which "may cause other clients to lose their playback privileges") and to be *notified*
that "current streaming privileges for this client have been revoked." This is implemented in all
three official SDKs — Android, iOS (hardcoded URL), and web (`services/pushkin.ts`) — it is a normal
part of the official client contract on every platform, not an Android-only mechanism.

No OSS Linux client implements this. Consequence of skipping it: streamboat will not take over the
stream when the user starts playing on another device, and `subStatus 4006` (streaming privileges
lost) will occasionally appear instead of proactively yielding. That is graceful degradation, not a
blocker — reasonable to defer past v1. The web SDK's message types
(`USER_ACTION` out; `PRIVILEGED_SESSION_NOTIFICATION` in, carrying `clientDisplayName`/`endsAt`/
`sessionId`) are the cheapest starting point if streamboat ever implements it.
