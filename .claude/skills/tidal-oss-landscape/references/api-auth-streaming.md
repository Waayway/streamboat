# TIDAL API landscape, auth flows, and stream resolution

Full narrative and per-claim sourcing: `/home/user/streamboat/docs/research/oss-landscape.md`
§1, §17, §18. Wire-format depth beyond what's here (full manifest byte layout, DRM specifics,
image URL construction) lives in `docs/research/tidal-api.md` — this file gives you enough to
write auth and stream-resolution code without opening either.

## Table of contents

1. Two API surfaces and what each can actually do
2. Unofficial auth flows (device code, PKCE, refresh, legacy)
3. The OAuth-redirect-capture decision (three known solutions)
4. Stream resolution: the canonical endpoint and its three siblings
5. Manifest formats: BTS, DASH, EMU, HLS
6. The `subStatus` taxonomy (the single most valuable table in the landscape)
7. Quality cascade and stopping rules
8. Credential handling patterns and their risk profiles
9. Dolby Atmos — an unresolved gap, not a solved problem

## 1. Two API surfaces and what each can actually do

| | Official developer platform | Unofficial legacy API |
| --- | --- | --- |
| Hosts | `openapi.tidal.com/v2`, `auth.tidal.com/v1/oauth2`, `login.tidal.com` | `api.tidal.com/v1`, `api.tidal.com/v2`, `api.tidalhifi.com/v1`, `desktop.tidal.com/v1` |
| Documented | Yes (`developer.tidal.com`, `tidal-music.github.io/tidal-api-reference`) | No |
| Credentials | Registered `client_id` (+ optional secret) from the developer portal | Client IDs extracted from official apps |
| Playback call | `GET /trackManifests/{id}` | `GET /tracks/{id}/playbackinfopostpaywall` |
| Full tracks for 3rd parties | **Previews only, per Developer Guidelines** | Yes — works today |
| Used by | tidal-cli, official SDKs | Sone, High Tide, Strawberry, python-tidal, mopidy-tidal, tidalt, tidalrs, TidalSwift, libopenTIDAL, TidaLuna |

**The load-bearing constraint, confirmed by direct fetch of the GitHub discussion (the
`developer.tidal.com` page itself is egress-blocked from this environment — see
`sources.md`):** `github.com/orgs/tidal-music/discussions/179` quotes the guidelines verbatim —
*"The Player module in the SDK constitutes the only allowed way for third-party applications to
incorporate playback of TIDAL content. By using an official, unmodified version of the Player
module, third-party applications can include playback of TIDAL previews."* Plus, from a search
summary of the guidelines page itself: *"TIDAL will reject any quota extension requests for any
Offering that attempts to circumvent this."*

Corroborating, directly-verified evidence:
- `ref:tidal-cli/src/playback.ts:27-28,74-75,204-205` carries `trackPresentation` and
  `previewReason` fields and prints `"Preview reason:"` — the official manifest response
  routinely returns previews in practice, confirmed by reading the source.
- `tidal-music/tidal-sdk-web` issue #133 (opened 2024-05-28, confirmed by direct fetch): client
  credentials streamed only *"a 30-second, low resolution version of the track"*; PKCE returned
  `Token is missing required scope. Required scopes: r_usr playback`.
- `tidal-music` discussion #179 (confirmed by direct fetch): OP requested third-party app review
  on 2025-06-03 ("over 6 months" before that date already spent trying); as of an April 2026 reply,
  *"nothing has moved. I even tried reaching them via e-mail a few months ago, but got no
  reply."* Another participant confirms playback is preview-only.

**Conclusion, now doubly confirmed:** the owner's stance (do what High Tide and Sone do) is not a
convenience choice — building against the unofficial API is currently the *only* way to build a
full-quality third-party TIDAL player.

**Keep an official-API adapter behind the same internal interface anyway.** The official `/v2`
`trackManifests` shape (`formats[]`, `manifestType`, `uriScheme`, `usage`, `adaptive`) is a
strictly better abstraction than a single `audioquality` string — model streamboat's internal
playback-request interface on the official shape, implement it against v1, and streamboat is one
config switch away from the official path if TIDAL ever opens the `playback` scope.

## 2. Unofficial auth flows (all verified against source)

### Device authorization grant (RFC 8628)

Used by Sone, python-tidal, tidalt, TidalSwift, libopenTIDAL, mopidy-tidal (its default):

```
POST https://auth.tidal.com/v1/oauth2/device_authorization
  form: client_id, scope="r_usr w_usr w_sub" [, client_secret]
  -> deviceCode, userCode, verificationUri, verificationUriComplete, expiresIn, interval

POST https://auth.tidal.com/v1/oauth2/token
  form: client_id, device_code, grant_type=urn:ietf:params:oauth:grant-type:device_code,
        scope="r_usr w_usr w_sub" [, client_secret]
  400 + "authorization_pending" | "slow_down"  -> keep polling
  200 -> access_token, refresh_token, expires_in, token_type, user
```

Source: `ref:sone/src-tauri/src/tidal_api.rs:1550-1642` (the exact 400+`authorization_pending`||
`slow_down` → `Ok(None)` branch is at :1624-1628), `ref:python-tidal/tidalapi/session.py:616-618,
694-699`, `ref:tidalt/internal/tidal/client.go` (`AuthURL` const).

### PKCE authorization code (needed for Hi-Res in the reference clients)

Used by High Tide, Strawberry (different config, see below), Sone (alternative), python-tidal:

```
GET https://login.tidal.com/authorize
  ?response_type=code&redirect_uri=https://tidal.com/android/login/auth
  &client_id=<pkce client id>&lang=EN&appMode=android
  &client_unique_key=<random>&code_challenge=<S256(verifier)>
  &code_challenge_method=S256&restrict_signup=true
-> user completes in browser, is bounced to the redirect URI carrying ?code=...

POST https://auth.tidal.com/v1/oauth2/token
  form: code, client_id, grant_type=authorization_code, redirect_uri,
        scope="r_usr+w_usr+w_sub", code_verifier, client_unique_key
```

Source: `ref:python-tidal/tidalapi/session.py:482-500` (param set), `:513-530` (token exchange,
`scope_default = "r_usr+w_usr+w_sub"`), `ref:sone/src-tauri/src/tidal_api.rs:1644-1684`.

**Strawberry uses a materially different config** — its own authorize/token hosts, redirect
scheme, and scope, confirmed by direct source read:
`authorize_url = https://login.tidal.com/authorize`,
`access_token_url = https://login.tidal.com/oauth2/token` (note: `login.tidal.com`, not
`auth.tidal.com`), `redirect_url = tidal://login/auth` (custom scheme, no local HTTP server —
`use_local_redirect_server(false)`), `scope = "r_usr w_usr"` (no `w_sub`). Source:
`ref:strawberry/src/tidal/tidalservice.cpp:79-83,128-134`.

### Refresh

```
POST https://auth.tidal.com/v1/oauth2/token
  form: grant_type=refresh_token, refresh_token, client_id, scope="r_usr w_usr w_sub"
```
Source: `ref:sone/src-tauri/src/tidal_api.rs:1370-1390`.

### Legacy username/password (dead, historical only)

`POST /v1/login/username` with an `X-Tidal-Token` header, returning a `sessionId` used as
`X-Tidal-SessionId`. Seen in `ref:tidalgo/tidal.go:30-70` and
`ref:dotnet-tidal-usdk/TidalUSDK/TidalClient.cs`. Both unmaintained since 2018/2020 respectively.

## 3. The OAuth-redirect-capture decision (three known solutions — pick deliberately)

This is one design decision that appears three times, separately, in the reference set. Name it
as a single decision rather than discovering it three times in streamboat's own auth code:

| Approach | Who does it | Mechanism | Packaging consequence |
| --- | --- | --- | --- |
| Custom URI scheme | Strawberry (`tidal://login/auth`); Sone (`tidal` deep-link scheme) | OS registers the app as the handler for a scheme | Needs a `.desktop` MIME registration (Linux), an NSIS/Info.plist entry (Windows/macOS), and a single-instance guard so a second launch forwards the URL to the first (`tauri-plugin-single-instance`, which Sone carries alongside `tauri-plugin-deep-link`) |
| Loopback HTTP server | tidal-cli (`http://localhost:17893/callback`); Sone also carries `tauri-plugin-oauth`; mopidy-tidal serves its login page on port 8989 | Bind a local port, redirect there, read the query string | Needs a free port and a firewall-friendly bind; no OS registration needed |
| Manual paste | High Tide (91-line dialog, `ref:high-tide/src/login.py`) | User copies the redirect URL out of the browser bar and pastes it back | Zero infrastructure, ugliest UX |

**Recommendation for streamboat**: desktop = custom scheme with a loopback fallback; headless =
device code with a terminal QR (tidalt's `mdp/qrterminal`) plus a paste-form fallback for
machines with no browser at all; keep mopidy-tidal's "login hack" (a dummy library item whose
cover art is a QR code of the login URL, rendered through the normal client UI with no protocol
extension — see `project-profiles.md` §mopidy-tidal) as the zero-protocol fallback for any client
surface that can't render either. **Generate the QR code locally** — mopidy-tidal's own
implementation sends the one-time login URL to `api.qrserver.com`
(`ref:mopidy-tidal/mopidy_tidal/login_hack.py:109-110`), which leaks it to a third-party host;
tidalt's `mdp/qrterminal` renders locally and is the pattern to copy, not mopidy-tidal's.

## 4. Stream resolution: the canonical endpoint and its three siblings

Canonical unofficial request, used by every full-quality client in the set:

```
GET https://api.tidal.com/v1/tracks/{track_id}/playbackinfopostpaywall
  ?countryCode=<CC>&audioquality=<HI_RES_LOSSLESS|HI_RES|LOSSLESS|HIGH|LOW>
  &playbackmode=STREAM&assetpresentation=FULL
```

Verified in: `ref:sone/src-tauri/src/tidal_api.rs:3654-3670`,
`ref:python-tidal/tidalapi/media.py:508-517` (`countryCode` injected centrally at
`request.py:76`), `ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:134-140`,
`ref:tidalrs/src/track.rs:321-336`.

Response fields actually consumed: `manifestMimeType`, `manifest` (base64), `audioQuality`,
`bitDepth`, `sampleRate`, `albumReplayGain`, `albumPeakAmplitude`, `trackReplayGain`,
`trackPeakAmplitude`. Strawberry also reads `encryptionKey`, `securityType`, `securityToken`,
`codec(s)`, `urls`/`url`.

**Four endpoint variants exist**, all pointed at the same host — Strawberry exposes all four as a
user setting, a useful hedge against endpoint churn:

| Method | Path | Params |
| --- | --- | --- |
| `StreamUrl` | `tracks/{id}/streamUrl` | `soundQuality` |
| `UrlPostPaywall` | `tracks/{id}/urlpostpaywall` | `audioquality`, `playbackmode=STREAM`, `assetpresentation=FULL`, `urlusagemode=STREAM` |
| `PlaybackInfoPostPaywall` (default in Strawberry and Sone) | `tracks/{id}/playbackinfopostpaywall` | `audioquality`, `playbackmode=STREAM`, `assetpresentation=FULL` |
| `PlaybackInfo` | `tracks/{id}/playbackinfo` | same as above |

Source: `ref:strawberry/src/constants/tidalsettings.h:25-30,63` (enum + default),
`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:118-147`.

**A fifth variant exists**: libopenTIDAL documents `tracks/{id}/playbackinfoprepaywall`, used when
`isPreview` (`ref:libopentidal/Source/OTService/OTServiceStd.c:165-167`).

**PKCE and `urlpostpaywall` are mutually exclusive.** python-tidal's `Track.get_url()` (the
`urlpostpaywall` path) raises `URLNotAvailable` immediately when the session is PKCE
(`ref:python-tidal/tidalapi/media.py:410-421`) — if streamboat supports PKCE login, do not route
through `urlpostpaywall`; use `playbackinfopostpaywall`.

**Concurrency and rate limiting**: cap `playbackinfo` requests at **2 concurrent**
(`TidaLuna`'s `Semaphore(2)` matches the official client's own behaviour,
`ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:51`; retries exactly once after 1 s,
permanently marks a track unavailable on 403/404). Add client-side rate limiting on top
(Sone's `rate_gate.rs` pattern — pure functions taking `now` as a parameter so they're testable,
see `sone-deep-dive.md`).

**Never cache manifests or stream URLs across restarts** — they expire in minutes to an hour.
Cache *metadata* aggressively instead (see Sone's tier/TTL/SWR table in `sone-deep-dive.md`).

## 5. Manifest formats: BTS, DASH, EMU, HLS

- **BTS** (`application/vnd.tidal.bts`): base64 → JSON `{urls: [...], codecs, mimeType,
  encryptionType, keyId}`. Take `urls[0]`. Codec is `codecs.toUpperCase().split('.')[0]`.
  Source: `ref:python-tidal/tidalapi/media.py:662-676`, `ref:sone/src-tauri/src/tidal_api.rs:
  3706-3730` (note: Sone's own `BtsManifest` struct reads `urls`/`codecs`/`mimeType`/
  `encryptionType` but not `keyId` — only python-tidal reads all five fields).
- **DASH** (`application/dash+xml`): base64 → MPD XML. Two consumption strategies, both proven:
  - **Data URI**: wrap as `data:application/dash+xml;base64,<b64>` and hand to GStreamer's
    `dashdemux` directly. Works everywhere, no filesystem write. Used by Sone
    (`ref:sone/src-tauri/src/commands/playback.rs:117-125`), Strawberry
    (`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:224-233`), High Tide on GStreamer < 1.26.
  - **File URI**: write the MPD to a file and pass `file://`. Used by mopidy-tidal always
    (`ref:mopidy-tidal/mopidy_tidal/playback.py:38-46`), High Tide on GStreamer ≥ 1.26
    (`ref:high-tide/src/lib/player_object.py:487-513` — the exact `Gst.version() >= (1, 26)`
    branch). **Prefer the data-URI path as the default; keep the file-URI path as a fallback.**
  - **Manual segment reconstruction**: regex-extract `initialization="…"`, `media="…$Number$…"`
    and `<S d= r=>` repeat counts, then download and concatenate segments sequentially. Used by
    tidal-cli (`ref:tidal-cli/src/playback.ts:118-167,121-126`) — a real fallback if you have no
    DASH demuxer available at all, but heavier than either strategy above.
- **EMU** (`application/vnd.tidal.emu`) and **HLS** (`application/vnd.apple.mpegurl`) appear only
  in the official SDKs. Canonical enum:
  `ref:tidal-sdk-android/player/streaming-api/.../ManifestMimeType.kt` — `EMU =
  application/vnd.tidal.emu`, `BTS = application/vnd.tidal.bts`, `DASH = application/dash+xml`,
  `HLS = application/vnd.apple.mpegurl`. Every client must handle at least BTS and DASH.

python-tidal's `StreamManifest` **hardcodes `encryption_type = "NONE"` for MPD with a `# TODO:
Handle encryption key`** comment (`media.py:657-660`) — do not treat "python-tidal says NONE" as
proof a DASH stream is unencrypted; only the BTS branch actually reads `encryptionType`.

## 6. The `subStatus` taxonomy — the single most valuable table in the landscape

Source: `ref:sone/src-tauri/src/tidal_api.rs:10-43` (verified verbatim, including the code
comment naming the auth-namespace sub-statuses). This table appears nowhere else in the
reference set and is worth copying exactly, not approximating:

- `subStatus` in `4000..=4999` on a 401 is a **`playbackinfo` sub-status, not token expiry** —
  refreshing the token will not fix it. Do not trigger a refresh-and-retry on these.
- **Terminal** (the track will never play — skip it, do not retry): `4005, 4010, 4030, 4031,
  4032, 4034, 4035`. Note `4033` is deliberately **absent** from this list, not part of a
  contiguous "4030–4035" range — copy the literal set, not a range.
- **Deliberately not terminal**: `4006` (streaming privileges lost — recovers on its own) and
  `4033` (subscription up-sell — the user can fix it by upgrading).
- **Auth-namespace sub-statuses live elsewhere, do not confuse them with the 4xxx set above**:
  `11002`/`11003` (token), `6001` (session), `1002` (pending / *"not a Limited Input Device
  client"* — this is what a web-player client ID returns to the device-code endpoint,
  `ref:sone/src-tauri/src/tidal_api.rs:1576-1588,1575-1588`).

## 7. Quality cascade and stopping rules

Source: `ref:sone/src-tauri/src/commands/playback.rs:32-84`.

Cascade order: `HI_RES_LOSSLESS → HI_RES → LOSSLESS → HIGH`, truncated at the user's configured
ceiling. Both Hi-Res tiers are dropped when no `client_secret` is configured, because those
credentials return encrypted DASH requiring Widevine that streamboat (like Strawberry) should
refuse rather than decrypt.

**Stop the cascade immediately** on a network error, a rate limit, or a terminal `subStatus`
(§6) — walking the ladder on those only multiplies request count 4× for nothing, because
*"over-requesting quality returns 200 with a downgraded `audioQuality`, never an error."*

**Pre-flight check** (mopidy-tidal's pattern, cheap and worth copying): before requesting
`hi_res_lossless`, check `"HIRES_LOSSLESS" in track.media_metadata_tags`; log/skip the request if
absent (`ref:mopidy-tidal/mopidy_tidal/playback.py:76-84`). **Watch the naming mismatch**: the
`Quality` enum member is `HI_RES_LOSSLESS`, the metadata tag is `HIRES_LOSSLESS` (no underscore
before RES) — always use a lookup table, never string equality across the two vocabularies
(`ref:python-tidal/tidalapi/media.py:57-66,87-94`).

**python-tidal 0.8.11 has dropped `HI_RES` (MQA) from its `Quality` enum entirely** — only `LOW,
HIGH, LOSSLESS, HI_RES_LOSSLESS` remain (default `HIGH`). Sone and Strawberry still offer the
`HI_RES` tier in their UI. tidal-connect's own README independently corroborates that TIDAL
removed all MQA content at the end of July 2024. **Whether `HI_RES` still returns anything at all
from a live `playbackinfopostpaywall` call is unverified** — treat `[HI_RES_LOSSLESS, LOSSLESS,
HIGH]` as the more likely-correct cascade and budget a live test before committing to including
`HI_RES` as a fourth rung.

## 8. Credential handling patterns and their risk profiles

No pattern in the set is "safe" — pick the one whose failure mode you can live with and document
it honestly:

| Pattern | Examples | Risk |
| --- | --- | --- |
| Embedded, obfuscated | Sone (XOR-masked byte arrays with misleading names like `STREAM_SALT_*`; trivially reversible — this is obfuscation, not security), python-tidal (double-base64-encoded, split-in-two byte literals) | Dishonest about what it does; reversible in seconds; if you ship this, don't also claim it's secure |
| Embedded, plaintext | tidalt (`client.go:17`, comment admits the secret is "baked into the official Tidal app"), tidal-cli (public client ID, official SDK), TidalSwift (id **and** secret in `Config.swift`), Strawberry's optional compile-time `TIDAL_CLIENT_ID` (never a secret) | Honest at least; still a shared credential that can be revoked/rotated by TIDAL at any time |
| None at all | tidal-hifi, TidaLuna (both ride the official app's own session/credentials) | No credential-churn risk for the wrapper itself, but zero control over the upstream client's behaviour |

**Recommendation**: Strawberry's model is the honest one — compile-time-optional client ID, never
a secret compiled in, user-overridable in settings. If streamboat ships default credentials at
all, state plainly in the README that they were extracted from an official app and may stop
working; do not obfuscate and pretend otherwise (Sone's own `embedded_config.rs` is the example
of what not to do — see `sone-deep-dive.md` "Avoid").

**At-rest secret *storage* (as opposed to embedded client credentials) is a solved problem**: OS
keyring as primary (`keyring` crate / libsecret / Keychain / EncryptedSharedPreferences), an
encrypted file as fallback, and **always write the file backup even when the keyring works** —
Sone's own reasoning is that the keyring may be unreachable on next launch (e.g. an AppImage
running in a different D-Bus session than it was configured in). Full container-format detail in
`sone-deep-dive.md`.

## 9. Dolby Atmos — an unresolved gap, not a solved problem

python-tidal models `AudioMode = STEREO | DOLBY_ATMOS` and `MediaMetadataTags` includes
`DOLBY_ATMOS`; Sone models the same fields (`tidal_api.rs:582-584`, `:156,:287`) but its quality
cascade selects on `audioquality` only and never passes an audio-mode preference — whatever the
API returns is fed straight to GStreamer with no verification the decode actually succeeds. High
Tide has no Atmos handling at all. **Nothing in the reference set demonstrates correct end-to-end
handling of an Atmos-tagged track.** Two things are unverified and worth testing before writing
code: whether `playbackinfopostpaywall` accepts an audio-mode/immersive parameter, and what codec
an Atmos track's BTS manifest actually reports (likely AC-4 or E-AC-3, which plain GStreamer
`base`/`good`/`bad` will not decode — see `packaging-distribution.md` on the `libav` bundling
question). **Policy recommendation**: prefer/request stereo by default; if an Atmos-only manifest
arrives anyway, fail with a specific, honest user-facing message rather than GStreamer's generic
"Internal data stream error."
