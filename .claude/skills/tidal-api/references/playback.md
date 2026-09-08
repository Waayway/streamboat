# Playback

Full narrative: `docs/research/tidal-api.md` §8. This is the highest-stakes area of the whole API —
get the encryption-refusal rule and the quality-cascade stop conditions right before anything else.

## Table of contents

1. `playbackinfopostpaywall` — the load-bearing call
2. Manifest types (BTS, DASH, EMU, HLS)
   - 2a. If you must parse the MPD yourself
3. Encryption — the rule that keeps streamboat distributable
4. The quality cascade
5. Simpler fallback endpoints (`urlpostpaywall`, `streamUrl`)
6. Video
7. Audio modes and codecs
8. Replay gain
9. Seeking, buffering, and manifest/URL lifetime
10. The official API's playback contract — `/trackManifests/{id}`
11. Offline is a licensed, DRM-bound concept — do not build toward it

---

## 1. `playbackinfopostpaywall` — the load-bearing call

```
GET https://api.tidal.com/v1/tracks/{trackId}/playbackinfopostpaywall
    ?audioquality=HI_RES_LOSSLESS|LOSSLESS|HIGH|LOW
    &playbackmode=STREAM
    &assetpresentation=FULL
    &countryCode=XX
Authorization: Bearer <token>
```
Four independent implementations agree on exactly this endpoint name and parameter set
(python-tidal, Sone, Sone-windows, Strawberry). Two more (TidaLuna, the official web SDK's legacy
path) use the same parameters against the shorter `/playbackinfo` form (no `postpaywall`) — treat
`playbackinfopostpaywall` as primary and `/playbackinfo` as an equivalent short form some clients
use, not a different call.

Parameter domains:
- `audioquality`: `LOW`, `HIGH`, `LOSSLESS`, `HI_RES_LOSSLESS`. `HI_RES` also exists as a legacy tier
  (the iOS SDK maps it to MQA, which is dead content — see §7).
- `playbackmode`: `STREAM` | `OFFLINE`. Only `STREAM` appears in any OSS client — see §11 before ever
  sending `OFFLINE`.
- `assetpresentation`: `FULL` | `PREVIEW`. `PREVIEW` is the 30-second clip; the response then carries
  `previewReason` (`FULL_REQUIRES_SUBSCRIPTION` | `FULL_REQUIRES_PURCHASE` |
  `FULL_REQUIRES_HIGHER_ACCESS_TIER`).
- `videoquality` (for `/videos/{id}/…`): `HIGH` | `MEDIUM` | `LOW` | `AUDIO_ONLY`.
- prefetch is the **header** `x-tidal-prefetch: true`, not a query parameter.

Response fields worth knowing:

| Field | Notes |
|---|---|
| `trackId` (or `videoId`) | may not match the request — track substitution is real, Strawberry logs the mismatch |
| `assetPresentation` | `FULL` \| `PREVIEW` — web SDK / TidaLuna types; python-tidal's `Stream.parse` does not read this |
| `previewReason` | only on previews — same web-SDK-only caveat |
| `audioMode` | `STEREO` \| `DOLBY_ATMOS` \| `SONY_360RA` (python-tidal's own enum only models the first two — a value it doesn't model can still arrive on the wire) |
| `audioQuality` | **what you actually got** — may be lower than requested |
| `manifestMimeType`, `manifest` (base64), `manifestHash` | see §2 |
| `bitDepth`, `sampleRate` | **correction: not established which tiers actually return null.** python-tidal's comment says LOW and legacy HI_RES (not LOW/HIGH), defaulting both to 16/44100; the web SDK types both `number \| null` with *no* tier qualification at all. Treat as optional at every tier; prefer the DASH `Representation@id` triple (e.g. `id="FLAC,44100,16"`) when a manifest is present, over trusting these two fields blindly. |
| `albumReplayGain`, `albumPeakAmplitude`, `trackReplayGain`, `trackPeakAmplitude` | dB and linear peak — see §8 |
| `licenseSecurityToken` | present on DRM'd assets — web SDK only, not read by python-tidal |
| `streamingSessionId` | echoed from `x-tidal-streamingsessionid` — web SDK only; see `references/transport.md` §2 for whether it's required for Recently Played |

**Critical**: over-requesting quality returns **HTTP 200 with a downgraded `audioQuality`, not an
error**. The cascade in §4 exists to handle *errors*; `audioQuality` in the response is what you
display to the user, always, regardless of what was requested.

## 2. Manifest types

Manifest MIME types (BTS/DASH/EMU/HLS), the BTS JSON shape, the DASH delivery-strategy options
(data-URI shortcut, `file://`, parse-and-fetch), and the full DASH `SegmentTemplate`/
`SegmentTimeline` structure are owned by `audio-pipeline/references/tidal-manifest-api.md` §3-4 —
cite it rather than restating; it also has the four-delivery-strategy breakdown and python-tidal's
`DashInfo` segment-numbering bugs. Two tidal-api-specific traps worth keeping here:

**python-tidal's own `ManifestMimeType` enum does not implement all four types** — it declares only
`MPD`, `BTS`, and a fifth value, `VIDEO = "video/mp2t"` (`EMU` and `vnd.apple.mpegurl` are
commented out in its source). Practical consequence: **python-tidal cannot play videos through the
manifest path** and falls back to `urlpostpaywall` for video instead. Source the four-type list to
`tidal-sdk-web`/`tidal-sdk-android`, not to python-tidal.

**codec normalization trap**: python-tidal upper-cases and truncates the BTS `codecs` string before
comparing it —`codecs.upper().split(".")[0]`, so `mp4a.40.2` becomes `MP4A` (both the leading `mp4a`
segment and the case change happen). A naive equality check against the raw wire value (`mp4a.40.2`)
will not match python-tidal's normalized form; normalize the same way before comparing codec strings,
or compare the full dotted string consistently and don't mix the two.
(`ref:python-tidal/tidalapi/media.py:670`.)

## 3. Encryption — the rule that keeps streamboat distributable

Strawberry's three refusal conditions, the `OLD_AES`-attested-by-one-file caveat (denylist-of-one,
not an allowlist), and the never-decrypt policy are owned by
`audio-pipeline/references/tidal-manifest-api.md` §5 — cite it rather than restating. One
tidal-api-specific data point worth keeping: **tidal-hifi sidesteps the whole problem** by running
the actual official web player inside a Widevine-capable (castlabs) Electron build, so Chromium's
CDM handles decryption — it never touches a manifest at all, which is a materially different
architecture from every native client in this set.

## 4. The quality cascade

Sone's `quality_tiers(ceiling, has_secret)` is the cleanest implementation:
```
ORDER = ["HI_RES_LOSSLESS", "HI_RES", "LOSSLESS", "HIGH"]
start at the user's ceiling; drop the two Hi-Res tiers when there is no client_secret;
the result always contains "HIGH", so it is never empty.
```
Loop rules:
- network error → propagate immediately, do not try lower tiers;
- rate-limited, or a terminal sub-status per `references/transport.md` §6 (including `4034`, which
  is terminal for this request — a lower tier will not help) → propagate immediately; `[inferred]`
  a different client id is the one retry worth trying on `4034` specifically, not a different tier;
- anything else → remember it, try the next tier.

tidalt has the same ladder over `urlpostpaywall`; Strawberry instead offers a *method* fallback chain
(`playbackinfopostpaywall` → `urlpostpaywall` → `streamUrl` → `playbackinfo`) selectable in settings.

## 5. Simpler fallback endpoints

```
GET tracks/{id}/urlpostpaywall?urlusagemode=STREAM&audioquality=…&assetpresentation=FULL&countryCode=
```
→ `{urls: ["https://..."]}` directly, no manifest, no base64. Used by python-tidal's `Track.get_url()`,
tidalt, tidalrs. **Blocked under PKCE sessions** in python-tidal, and gives you no replay-gain or
bit-depth metadata. Good as a fallback and for a minimum-viable path.

```
GET tracks/{id}/streamUrl?soundQuality=…&countryCode=
```
The oldest form, still offered by Strawberry, returning `{url, trackId, soundQuality, encryptionKey,
codec}`. Likely dead or degraded — status not independently confirmed.

## 6. Video

```
GET https://api.tidal.com/v1/videos/{id}/playbackinfopostpaywall
    ?videoquality=HIGH&playbackmode=STREAM&assetpresentation=FULL&countryCode=XX
```
→ `{videoId, videoQuality, manifestMimeType, manifest}`, manifest decodes to a `{urls: [...]}` EMU
JSON body (Sone takes `urls[0]` without checking its extension). **That `urls[0]` is specifically an
HLS `.m3u8` is an inference, not confirmed for this EMU path** — the only direct evidence for `.m3u8`
is python-tidal's docstring on the *separate* `urlpostpaywall` shortcut, not on this manifest path.
Assume `.m3u8` but verify against a live response before hardcoding a parser that requires it.
`GET videos/{id}/urlpostpaywall?urlusagemode=STREAM&videoquality=&assetpresentation=FULL` is the
shortcut. Sone plays video through hls.js in its webview, separate from the audio pipeline — a
reasonable pattern to copy if streamboat has a webview at all.

**Concrete per-tier codec/resolution/bitrate data** (libopenTIDAL's manual, dated 2021 — indicative,
not a contract; actual values may vary by client id):

| `videoquality` | Codecs | Resolution / framerate / bitrate |
|---|---|---|
| `AUDIO_ONLY` | HE-AAC (`mp4a.40.5`) | no video — **reusable by the existing audio pipeline, no video surface needed** |
| `LOW` | AAC-LC `mp4a.40.2` + H.264 `avc1.42001e` | 320x180 @ 25 fps |
| `MEDIUM` | AAC-LC `mp4a.40.2` + H.264 `avc1.4d001f` | 640x360 @ 25 fps |
| `HIGH` | HLS, multiple renditions | adaptive ladder, 1920x1080 @ 25 fps (~10173 kbps) down to 320x180 @ 12.5 fps (~318 kbps) — **needs a real ABR-capable player, not a single URL** |

`AUDIO_ONLY` is a cheap partial answer to "video in or out for v1" — no new pipeline needed — while
`HIGH` is genuinely a second pipeline. `ref:libopentidal/Docs/OTQuality.7`.

## 7. Audio modes and codecs

| Mode | Codec | Status |
|---|---|---|
| `STEREO` | `mp4a.40.5`/HE-AAC (LOW), `mp4a.40.2`/AAC-LC (HIGH), `flac` (LOSSLESS, HI_RES_LOSSLESS) | current |
| `DOLBY_ATMOS` | E-AC-3 JOC — `EAC3` unofficial, `EAC3_JOC` official. iOS SDK: "Dolby Atmos is delivered in the E-AC-3 (JOC) codec; the quality tier is irrelevant." | current |
| `SONY_360RA` | `mha1` (MPEG-H) is a plausible but **unconfirmed** codec — the iOS SDK declares `mha1`/`mhm1` constants but its `init?` returns `nil` for `SONY_360RA` rather than selecting a codec, with the comment: "has no codec the client needs, so it is unsupported here." | **removed from TIDAL 2024-07-24** |
| MQA (`HI_RES` tier) | `mqa` | **removed from TIDAL 2024-07-24** |

MQA and Sony 360 Reality Audio were removed 2024-07-24, replaced by FLAC and Dolby Atmos (TIDAL's own
support article, corroborated by multiple 2024 outlets, and independently by `tidal-connect`'s
README noting no content above 16/44 is currently offered as `HI_RES` post-removal).

**Practical consequence**: Atmos is the only immersive format left, and playing it means decoding
E-AC-3 JOC — GStreamer can decode E-AC-3, but JOC object rendering to stereo/binaural is not
something the open stack does well. Treating Atmos and 360RA as out of scope for v1 (still parse
`audioModes` so the UI can label them, and so you never request a tier that unexpectedly returns
one) is defensible.

## 8. Replay gain

TIDAL ships four numbers per track (`trackReplayGain`/`albumReplayGain` +
`trackPeakAmplitude`/`albumPeakAmplitude`). Canonical formula, pre-amp value, album-vs-track
context selection, and the Sone/High Tide implementation variants are owned by
`audio-pipeline/references/playback-behavior.md` §4 — do not restate them here. Headline point:
TIDAL's own SDKs use `min(10^((replay_gain+pre_amp)/20), 1/peak)` with `pre_amp=4.0` and no extra
attenuation factor; Sone's shipped code multiplies that by an additional `0.8`, which is Sone's
own choice, not TIDAL's formula.

## 9. Seeking, buffering, and manifest/URL lifetime

- **Seeking works — the CDN honors HTTP Range.** mopidy-tidal's caching proxy in front of
  `lgf.audio.tidal.com` parses `Range:` request headers and answers `206 Partial Content` with
  `Content-Range` — a proxy could not do that if the origin didn't support byte ranges. Seeking in a
  BTS FLAC/AAC file is a plain HTTP range request; `souphttpsrc` (or your stack's equivalent) handles
  it without special-casing TIDAL.
- **Manifests are not permanent.** The 1-hour client-side cache lifetime (see
  `references/transport.md` §1) is the number to copy. On a CDN 403 mid-track, re-issue
  `playbackinfopostpaywall`/`urlpostpaywall` and resume at the current position — do not surface an
  error to the user for what is normal manifest expiry.
- **Still unverified**: the actual signed-URL TTL inside `urls[0]` itself (distinct from the 1-hour
  manifest cache). No checkout states this number — handle expiry-by-403 defensively rather than
  trying to predict it.
- **Do not persist manifests to disk.** Hold a resolved manifest in memory for the session only — it
  carries signed, expiring CDN URLs. This is a different thing from the byte-cache carve-out in §11:
  caching the decoded audio bytes for a logged-in subscriber is defensible, caching the manifest that
  points at TIDAL's signed CDN URLs is not (it just goes stale and adds no value once the CDN URLs it
  contains expire).

## 10. The official API's playback contract — `/trackManifests/{id}`

```
GET https://openapi.tidal.com/v2/trackManifests/{id}
    ?manifestType=HLS|MPEG_DASH
    &formats[]=HEAACV1|AACLC|FLAC|FLAC_HIRES|EAC3_JOC     (a SET, not a single tier)
    &uriScheme=HTTPS|DATA
    &usage=PLAYBACK|DOWNLOAD
    &adaptive=true|false
    &shareCode=<optional — grants access to UNLISTED resources>
```
Response (`TrackManifests_Attributes`): `uri` (a `data:` URL when `uriScheme=DATA`), `formats[]`,
`hash`, `drmData` (`{certificateUrl, drmSystem: FAIRPLAY|WIDEVINE, initData[], licenseUrl}` — "absence
implies no DRM"), `trackPresentation: FULL|PREVIEW`, `previewReason`, `trackAudioNormalizationData`,
`albumAudioNormalizationData`.

Quality→`formats[]` ladder the official player sends (there is no cascade to walk — the server picks
within the offered set):

| Requested quality | `formats[]` |
|---|---|
| LOW | `HEAACV1` |
| HIGH | `HEAACV1, AACLC` |
| LOSSLESS | `HEAACV1, AACLC, FLAC` |
| HI_RES / HI_RES_LOSSLESS (web SDK grouping); **HI_RES_LOSSLESS only** (Android SDK, equality check — `HI_RES` gets no `FLAC_HIRES` there) | `HEAACV1, AACLC, FLAC, FLAC_HIRES` |
| immersive audio (any tier) | append `EAC3_JOC` — **Android SDK only**; `rg EAC3_JOC` over `tidal-sdk-web/packages/player` and `tidal-cli` returns nothing |

If streamboat calls `/trackManifests/{id}` directly, follow the web SDK's more-permissive grouping
(don't special-case `HI_RES`) and don't assume `EAC3_JOC` is sent by the web player.

Whether a newly registered third-party client actually receives full-track manifests here, or only
30-second previews, is **unresolved** — `tidal-music` Discussion #179 raises exactly this doubt with
no answer; tidal-cli's code assumes full tracks. Untested against a live third-party registration.

**Leaning signal, not proof**: one developer on that thread reports first-hand, after
Authorization-Code (PKCE) login, that their users "cannot play but the 30s low quality preview of
tracks" (`ildella`, comment dated 2025-09-06). No TIDAL staff reply confirms or denies it anywhere on
the thread, and the OP's last update (2026-04-16) is still about the review pipeline being stalled,
not about playback behavior. Treat this as evidence leaning toward preview-only for newly-registered
clients — still unconfirmed by TIDAL itself.

**Note this endpoint returns a DRM-protected manifest** (see `drmData` above) — decoding it yourself
without a licensed CDM does not work regardless of the contractual question. See
`docs/research/tidal-api.md` §1 and §14 for the two-part (contractual + technical) reason official
playback is closed to third parties, and `references/legal-and-landscape.md` for the "compliant vs
native" architecture choice this implies.

## 11. Offline is a licensed, DRM-bound concept — do not build toward it

`playbackmode=OFFLINE` (unofficial) and `usage=DOWNLOAD` (official `/trackManifests/{id}`) are the
same concept. There is a dedicated sub-status, `4007 USER_CLIENT_NOT_AUTHORIZED_FOR_OFFLINE` — the
server separately checks whether *your client id* is entitled to offline at all, so a shared
ecosystem client id may simply be refused. TIDAL's own offline assets are DRM-licensed with expiry:
the iOS SDK's `OfflineEngine` models `NOT_OFFLINED | OFFLINED_AND_VALID | OFFLINED_BUT_NOT_VALID |
OFFLINED_BUT_NO_LICENSE | OFFLINED_BUT_EXPIRED`. The official API models the whole workflow:
`/offlineTasks`, `/downloads`, `/trackFiles/{id}`, `/installations/{id}/relationships/offlineInventory`,
`/userOfflineMixes`.

**A transparent HTTP byte cache of an already-cleartext stream (what mopidy-tidal's caching proxy
does) is a materially different thing from requesting `playbackmode=OFFLINE`**, which asks TIDAL for
a licensed, DRM-bound downloadable asset and would drag DRM into streamboat. If offline caching is
wanted at all: build the former, and rule out the latter by policy — it is not a "more ambitious"
version of the same feature, it is a different and disqualifying one. See the "Open decisions"
section of `SKILL.md`.
