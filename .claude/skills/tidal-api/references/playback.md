# Playback

Full narrative: `docs/research/tidal-api.md` §8. This is the highest-stakes area of the whole API —
get the encryption-refusal rule and the quality-cascade stop conditions right before anything else.

## Table of contents

1. `playbackinfopostpaywall` — the load-bearing call
2. Manifest types (BTS, DASH, EMU, HLS)
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
(python-tidal, Sone, sone-windows, Strawberry). Two more (TidaLuna, the official web SDK's legacy
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
| `assetPresentation` | `FULL` \| `PREVIEW` |
| `previewReason` | only on previews |
| `audioMode` | `STEREO` \| `DOLBY_ATMOS` \| `SONY_360RA` (python-tidal's own enum only models the first two — a value it doesn't model can still arrive on the wire) |
| `audioQuality` | **what you actually got** — may be lower than requested |
| `manifestMimeType`, `manifest` (base64), `manifestHash` | see §2 |
| `bitDepth`, `sampleRate` | **null for LOW/HIGH tiers** — the web SDK annotates this explicitly; default to 16/44100 only for display purposes, don't invent real values |
| `albumReplayGain`, `albumPeakAmplitude`, `trackReplayGain`, `trackPeakAmplitude` | dB and linear peak — see §8 |
| `licenseSecurityToken` | present on DRM'd assets |
| `streamingSessionId` | echoed from `x-tidal-streamingsessionid` |

**Critical**: over-requesting quality returns **HTTP 200 with a downgraded `audioQuality`, not an
error**. The cascade in §4 exists to handle *errors*; `audioQuality` in the response is what you
display to the user, always, regardless of what was requested.

## 2. Manifest types

**`application/vnd.tidal.bts`** — base64 → JSON:
```json
{
  "mimeType": "audio/flac",
  "codecs": "flac",
  "encryptionType": "NONE",
  "keyId": "<base64>",            // only when encryptionType != NONE
  "urls": ["https://.../....flac?token=..."]
}
```
Take `urls[0]`. `codecs` values seen: `mp3`, `aac`, `aac+`, `flac`, `mp4a.40.2` (AAC-LC/HIGH),
`mp4a.40.5` (HE-AAC/LOW).

**`application/dash+xml`** — base64 → an MPEG-DASH MPD. **Delivery shortcut used by every reference
client that plays DASH**: don't parse the MPD, wrap the original base64 as a data URI —
`data:application/dash+xml;base64,<the original base64>` — and pass that straight to the pipeline.
GStreamer's `dashdemux` accepts it directly (High Tide, Sone, Strawberry all do exactly this).
mopidy-tidal instead writes the MPD to a cache file and passes a `file://` URI — an alternative if
your pipeline doesn't like data URIs. Only parse the MPD yourself if you need metadata the response
JSON doesn't already give you (codec/bit-depth/sample-rate can be mined from
`Representation@id`/`@codecs`/`@audioSamplingRate` by regex, as the web SDK does — see
`ref:tidal-sdk-web/.../manifest-parser.ts:105-205`).

**`application/vnd.tidal.emu`** — base64 → JSON with `urls[]`; used for video.

**`application/vnd.apple.mpegurl`** — HLS. Requested by official SDKs when FairPlay is available;
not something a Linux/Windows client normally asks for.

**Correction worth knowing**: python-tidal's own `ManifestMimeType` enum does not implement all four
types above — it declares only `MPD`, `BTS`, and a fifth value, `VIDEO = "video/mp2t"` (`EMU` and
`vnd.apple.mpegurl` are commented out in its source). Practical consequence: **python-tidal cannot
play videos through the manifest path** and falls back to `urlpostpaywall` for video instead. Source
the four-type list to `tidal-sdk-web`/`tidal-sdk-android`, not to python-tidal.

## 3. Encryption — the rule that keeps streamboat distributable

`encryptionType` in a BTS manifest is `NONE` or `OLD_AES`.

- **Strawberry refuses**: if `encryptionKey` is non-empty, or `encryptionType`/`securityType` is
  anything other than `NONE`, it fails with a user-facing message and does not play. **This is the
  posture streamboat must copy.**
- python-tidal records `encryption_type`/`encryption_key` but does not decrypt (hardcodes `"NONE"`
  for MPD manifests with a `TODO`).
- Sone avoids the situation pre-emptively: it drops the Hi-Res tiers from its cascade entirely when
  it has no `client_secret` configured, on the reasoning that those credentials typically return
  encrypted DASH streams requiring Widevine.
- tidal-hifi sidesteps it by running the actual official web player inside a Widevine-capable
  (castlabs) Electron build, so Chromium's CDM handles decryption — it never touches a manifest.
- TidaLuna (a mod running *inside* the official, licensed desktop client) decrypts `OLD_AES` with a
  hardcoded master key. **streamboat must not do this.** It is DRM circumvention, it is the
  behavior TIDAL has historically pursued legally (see `references/legal-and-landscape.md`), and it
  would make streamboat undistributable on Flathub and in distro repos.

**The rule**: treat any non-`NONE` `encryptionType` as "this tier is not available to us," surface a
clear message, and fall back or skip — Strawberry's behavior, with Sone's pre-emptive tier filtering
as an optimization that reduces how often users hit the wall at all. Do not ship any AES/Widevine
handling.

## 4. The quality cascade

Sone's `quality_tiers(ceiling, has_secret)` is the cleanest implementation:
```
ORDER = ["HI_RES_LOSSLESS", "HI_RES", "LOSSLESS", "HIGH"]
start at the user's ceiling; drop the two Hi-Res tiers when there is no client_secret;
the result always contains "HIGH", so it is never empty.
```
Loop rules:
- network error → propagate immediately, do not try lower tiers;
- rate-limited, or a terminal sub-status per `references/transport.md` §6 → propagate immediately (a
  lower tier will not help — **except** `4034`, which is client/tier-scoped: retry once at a lower
  tier or a different client id before giving up on it, do not treat it the way Sone's blanket
  terminal set does);
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
→ `{videoId, videoQuality, manifestMimeType, manifest}`, manifest decodes to `{urls: [...]}`, URL is
an HLS `.m3u8`. `GET videos/{id}/urlpostpaywall?urlusagemode=STREAM&videoquality=&assetpresentation=FULL`
is the shortcut. Sone plays video through hls.js in its webview, separate from the audio pipeline —
a reasonable pattern to copy if streamboat has a webview at all.

## 7. Audio modes and codecs

| Mode | Codec | Status |
|---|---|---|
| `STEREO` | `mp4a.40.5`/HE-AAC (LOW), `mp4a.40.2`/AAC-LC (HIGH), `flac` (LOSSLESS, HI_RES_LOSSLESS) | current |
| `DOLBY_ATMOS` | E-AC-3 JOC — `EAC3` unofficial, `EAC3_JOC` official. iOS SDK: "Dolby Atmos is delivered in the E-AC-3 (JOC) codec; the quality tier is irrelevant." | current |
| `SONY_360RA` | `mha1` (MPEG-H). iOS SDK: "has no codec the client needs, so it is unsupported here." | **removed from TIDAL 2024-07-24** |
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

TIDAL ships four numbers per track. Sone's normalization formula, which it calls "Tidal-correct":
```
norm_gain = 0.8 * min( 10^((replay_gain + 4) / 20), 1 / peak_amplitude )
```
`pre_amp = 4.0`, `peak` defaults to 1.0 when absent. Context: album context prefers
`albumReplayGain`/`albumPeakAmplitude`, mixed/shuffled queues prefer the track values, each falling
back to the other. High Tide instead configures GStreamer's `rgvolume` with
`pre-amp=4.0 fallback-gain=-10 headroom=6.0` and injects tags — same intent, different mechanism;
pick whichever fits your audio backend.

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
| HI_RES / HI_RES_LOSSLESS | `HEAACV1, AACLC, FLAC, FLAC_HIRES` |
| immersive audio (any tier) | append `EAC3_JOC` |

Whether a newly registered third-party client actually receives full-track manifests here, or only
30-second previews, is **unresolved** — `tidal-music` Discussion #179 raises exactly this doubt with
no answer; tidal-cli's code assumes full tracks. Untested against a live third-party registration.

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
