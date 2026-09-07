# Audio playback pipeline for streamboat

Research report. Scope: how to get bytes from TIDAL into a DAC at the source's native rate and bit
depth, on Linux desktop, Windows, macOS and headless Linux (Raspberry Pi class), with a path to
mobile later.

Everything marked **[verified]** was read directly from a reference checkout or from a primary
document (upstream source file, upstream release notes, crates.io API). Everything marked
**[inferred]** is a conclusion drawn from those facts. Everything marked **[unverified]** could not
be confirmed in this pass and must be checked before it is relied on.

---

## Summary

- TIDAL delivers four audio quality tiers: `LOW` (HE-AAC v1, ~96 kbps), `HIGH` (AAC-LC, ~320 kbps),
  `LOSSLESS` (FLAC 16-bit/44.1 kHz), `HI_RES_LOSSLESS` (FLAC up to 24-bit/192 kHz). A fifth legacy
  value `HI_RES` maps to MQA and is dead content since July 2024. **[verified]**
- Two different manifest APIs are in the wild. The legacy v1 endpoint is
  `GET https://api.tidal.com/v1/tracks/{id}/playbackinfopostpaywall?countryCode&audioquality&playbackmode=STREAM&assetpresentation=FULL`;
  the modern v2 endpoint is `GET https://openapi.tidal.com/v2/trackManifests/{id}?formats=[…]&manifestType=MPEG_DASH|HLS&uriScheme=DATA&usage=PLAYBACK&adaptive=true|false`.
  Both return a base64 manifest. **[verified]**
- A manifest comes in one of four MIME types: `application/vnd.tidal.bts` (JSON with direct CDN
  URLs), `application/dash+xml` (MPEG-DASH MPD), `application/vnd.apple.mpegurl` (HLS, used when
  FairPlay is supported), `application/vnd.tidal.emu` (JSON, same shape as BTS). **[verified]**
- Lossless and hi-res arrive as FLAC in fragmented MP4 delivered over DASH with a
  `SegmentTemplate` + `SegmentTimeline`; the codec string in the MPD is `flac` and the
  `Representation` `id` carries `"FLAC,44100,16"`-style metadata. **[verified]**
- Manifests expire. TIDAL's own web SDK treats a manifest as valid for exactly one hour
  (`MANIFEST_EXPIRATION_MS = 3600000`). **[verified]**
- Loudness normalization is a client-side gain, not baked into the stream. The canonical formula in
  TIDAL's own SDKs is `min(10^((replayGain + preAmp)/20), 1/peakAmplitude)` with `preAmp = 4`
  (0 on TV), applied from `albumReplayGain`/`albumPeakAmplitude` (mode `ALBUM`, the default) or the
  track equivalents (mode `TRACK`). **[verified]**
- TIDAL's own web player offers a crossfade of 0–15000 ms, defaulting to 0. At 0 it does a 250 ms
  micro-crossfade between two Shaka instances, i.e. its "gapless" is not sample-accurate.
  **[verified]**
- Bit-perfect output requires bypassing the OS mixer. Linux: open the ALSA `hw:` device directly and
  disable ALSA's soft-resample. Windows: WASAPI exclusive mode. macOS: CoreAudio hog mode
  (`kAudioDevicePropertyHogMode`) plus a physical-format change. **[verified]**
- GStreamer can do exclusive output on Linux (`alsasink device=hw:X,Y`) and Windows
  (`wasapi2sink exclusive=true`, a real GObject property), but **not** on macOS: `osxaudiosink`
  exposes only `device`, `unique-id`, `configure-session` and `volume`, and GStreamer's CoreAudio HAL
  sets hog mode only in the SPDIF/passthrough path (`_open_spdif`). **[verified]**
- libmpv does support exclusive output on all three desktops: `--audio-exclusive=yes` works for the
  `wasapi`, `coreaudio`, `pipewire` and `audiounit` AOs, and mpv ships a dedicated
  `coreaudio_exclusive` AO for macOS. **[verified]**
- The reference Linux client with the strongest bit-perfect story (Sone) does not use a GStreamer
  audio sink at all in exclusive mode: it decodes with GStreamer into an `appsink` and writes PCM
  from its own thread straight to `libasound`, negotiating `snd_pcm_hw_params` itself.
  **[verified]** — `ref:sone/src-tauri/src/audio.rs`
- Handing over the ALSA device from PipeWire/WirePlumber needs the D-Bus
  `org.freedesktop.ReserveDevice1.Audio{N}` protocol (`RequestRelease`, then `RequestName` with
  `ReplaceExisting`). tidalt implements this; Sone does not and instead surfaces a "device busy"
  error. **[verified]**
- Chromium (and therefore Electron and any browser-based client) resamples audio output; TIDAL's own
  browser path cannot be bit-perfect. tidal-hifi works around it with
  `--audio-output-sample-rate=192000` plus `--disable-features=AudioServiceOutOfProcess,AudioServiceSandbox`.
  **[verified for the flags; the resampling claim is verified from Chromium issue trackers, not from
  a Chromium source read]**
- Dolby Atmos on TIDAL is E-AC-3 with JOC (`EAC3_JOC` format token, `audioMode = DOLBY_ATMOS`). The
  official TIDAL desktop apps do not play Atmos. Sony 360 Reality Audio (`SONY_360RA`, codec `mha1`)
  was removed from TIDAL on 24 July 2024 and is unplayable. **[verified for the format tokens;
  the "desktop apps do not support Atmos" claim rests on TIDAL support-page reporting, not a
  first-party page read — see Open questions]**
- GStreamer 1.26.10 (December 2025) added "support for FLAC audio in DASH manifests"; before that,
  the newer `adaptivedemux2`/`dashdemux2` path did not handle TIDAL's FLAC-in-DASH and only the
  legacy `dashdemux` from gst-plugins-bad did. **[verified from release reporting; the exact code
  change is unverified]**
- GStreamer 1.28.0 was released 27 January 2026. The Rust bindings crate `gstreamer` is at 0.25.3
  (MIT OR Apache-2.0, MSRV 1.92). **[verified]**
- Symphonia 0.6.1 (MPL-2.0) decodes FLAC "excellent" and AAC-LC "great", but has no HE-AAC (needed
  for TIDAL `LOW`), no DASH, and no network layer. **[verified]**
- `cpal` 0.18.2 has no WASAPI exclusive mode and no CoreAudio hog mode; anything built on it (rodio
  0.22.2, kira 0.12.4) inherits that limitation. **[verified for cpal's missing exclusive mode from
  the upstream issue and docs; that rodio/kira use cpal is verified from their descriptions]**
- The one-device-at-a-time streaming privilege is pushed over a WebSocket whose URL comes from
  `POST https://api.tidal.com/v1/rt/connect`; the message type is `PRIVILEGED_SESSION_NOTIFICATION`.
  **[verified]**
- Sub-statuses in the 4xxx range on a playbackinfo 401 are content errors, not auth errors: 4005,
  4010, 4030, 4031, 4032, 4034, 4035 are terminal (do not retry, do not refresh the token);
  4006 (streaming privileges lost) and 4033 (subscription up-sell) recover. **[verified]**

---

## Findings

### 1. What TIDAL actually delivers

#### 1.1 Quality tiers and their codec mapping

`ref:python-tidal/tidalapi/media.py` lines 57–66 define the tier vocabulary:

```
Quality.low_96k          = "LOW"
Quality.low_320k         = "HIGH"
Quality.high_lossless    = "LOSSLESS"
Quality.hi_res_lossless  = "HI_RES_LOSSLESS"
Quality.default          = "HIGH"
```

The iOS SDK is the clearest primary statement of tier → codec, in
`ref:tidal-sdk-ios/Sources/Player/Common/Data/AudioCodec.swift`:

| Tier | Codec | Manifest codec strings recognised |
|---|---|---|
| `LOW` | HE-AAC v1 | `mp4a.40.5`, `heaacv1` |
| `HIGH` | AAC-LC | `mp4a.40.2`, `aaclc`, `aac` |
| `LOSSLESS` | FLAC (`ALAC` possible but the SDK does not emit it) | `flac` |
| `HI_RES` | MQA | `mqa` |
| `HI_RES_LOSSLESS` | FLAC | `flac`, `flac_hires` |
| mode `DOLBY_ATMOS` (any tier) | E-AC-3 (JOC) | `eac3`, `ac4` |
| mode `SONY_360RA` | — SDK explicitly returns `nil`: *"Sony 360 Reality Audio has no codec the client needs, so it is unsupported here."* | `mha1`, `mhm1` |

The v2 API speaks in **formats**, not tiers. `ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts`
maps them both ways:

```ts
audioQualityToFormats('HI_RES' | 'HI_RES_LOSSLESS') -> ['HEAACV1','AACLC','FLAC','FLAC_HIRES']
audioQualityToFormats('LOSSLESS')                   -> ['HEAACV1','AACLC','FLAC']
audioQualityToFormats('HIGH')                       -> ['HEAACV1','AACLC']
audioQualityToFormats('LOW')                        -> ['HEAACV1']

audioFormatsToQuality: FLAC_HIRES -> HI_RES_LOSSLESS; FLAC -> LOSSLESS; AACLC -> HIGH; else LOW
```

The Android SDK builds the same list additively and appends `EAC3_JOC` when immersive audio is
requested (`ref:tidal-sdk-android/player/streaming-api/src/main/kotlin/com/tidal/sdk/player/streamingapi/playbackinfo/repository/PlaybackInfoRepositoryDefault.kt`,
`getRequestedFormats`). Presence of `EAC3_JOC` in the response's `formats` is how the Android SDK
decides `audioMode = DOLBY_ATMOS` (`getAudioModeFromFormats`).

The full format enum (`ref:tidal-sdk-android/tidalapi/.../TrackManifestsAttributes.kt`) is
`HEAACV1, AACLC, FLAC, FLAC_HIRES, EAC3_JOC`. There is no 360RA format token — consistent with
360RA having been removed.

**Bit rates and depths.** TIDAL's marketing numbers (LOW ≈ 96 kbps HE-AAC, HIGH ≈ 320 kbps AAC,
LOSSLESS = 1411 kbps 16/44.1, HI_RES_LOSSLESS up to 24/192) appear in `libopenTIDAL`'s docs
(`ref:libopentidal/README.md` region) and in third-party reviews. The authoritative per-track values
come back in the playbackinfo response itself as `bitDepth` and `sampleRate`. **[verified that the
fields exist; the marketing bit-rate figures are [unverified] against a first-party TIDAL page,
because support.tidal.com and tidal.com are blocked from this environment]**

#### 1.2 The two manifest APIs

**Legacy v1 (what every unofficial client uses).**

```
GET https://api.tidal.com/v1/tracks/{trackId}/playbackinfopostpaywall
  ?countryCode={cc}
  &audioquality={LOW|HIGH|LOSSLESS|HI_RES|HI_RES_LOSSLESS}
  &playbackmode=STREAM
  &assetpresentation=FULL
Authorization: Bearer {access_token}
```

Verified in `ref:sone/src-tauri/src/tidal_api.rs:3654-3680` (`get_stream_url`), in
`ref:python-tidal/tidalapi/media.py` (`Track.get_stream`), and in
`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:110-150`.

Response fields observed across implementations:
`manifestMimeType`, `manifest` (base64), `manifestHash`, `audioQuality`, `audioMode`,
`assetPresentation`, `bitDepth`, `sampleRate`, `trackReplayGain`, `trackPeakAmplitude`,
`albumReplayGain`, `albumPeakAmplitude`, `trackId`, `streamingSessionId`.

The web SDK's `_fetchLegacyPlaybackInfo` (used only for the native player) hits
`{legacyApiUrl}/{tracks|videos}/{id}/playbackinfo` (no `postpaywall`) and sets extra headers:
`x-tidal-token: {clientId}`, `x-tidal-streamingsessionid: {sessionId}`, and `x-tidal-prefetch: true`
when the fetch is a prefetch. **[verified]**

Strawberry supports four v1 variants, user-selectable, defaulting to `playbackinfopostpaywall`
(`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:118-146`):
`tracks/{id}/streamUrl?soundQuality=`, `tracks/{id}/urlpostpaywall?audioquality=&urlusagemode=STREAM`,
`tracks/{id}/playbackinfopostpaywall`, `tracks/{id}/playbackinfo`.

`urlpostpaywall` is the simplest of the four — it returns `{urls: [...]}` with a direct CDN URL and
no manifest at all. tidalt uses only that:
`GET https://api.tidal.com/v1/tracks/{id}/urlpostpaywall?urlusagemode=STREAM&audioquality=…&assetpresentation=FULL&countryCode=…`
(`ref:tidalt/internal/tidal/api.go`, per the project survey; the file confirms the same call shape).

**Modern v2 (what TIDAL's own SDKs use).**

```
GET https://openapi.tidal.com/v2/trackManifests/{id}
  ?formats=HEAACV1,AACLC,FLAC,FLAC_HIRES[,EAC3_JOC]
  &manifestType=MPEG_DASH|HLS
  &uriScheme=DATA|HTTPS
  &usage=PLAYBACK|DOWNLOAD
  &adaptive=true|false
  [&shareCode=…]
x-playback-session-id: {streamingSessionId}
```

and `GET /v2/videoManifests/{id}?uriScheme=DATA&usage=PLAYBACK`.

Verified in `ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts`
(`_fetchTrackManifest`, `_fetchVideoManifest`), `ref:tidal-cli/src/playback.ts`
(`fetchTrackManifestData`), and `ref:tidal-sdk-android/.../PlaybackInfoRepositoryDefault.kt`.

The v2 response is JSON:API. Attributes seen:
`uri` (a `data:<mime>;base64,<manifest>` URL when `uriScheme=DATA`, otherwise an https URL),
`formats`, `hash`, `trackPresentation` (`FULL`/`PREVIEW`), `previewReason`,
`trackAudioNormalizationData.{replayGain,peakAmplitude}`,
`albumAudioNormalizationData.{replayGain,peakAmplitude}`, `drmData.licenseUrl`.

Note the v2 response does **not** carry `bitDepth`/`sampleRate` — the web SDK hardcodes
`bitDepth: 0, sampleRate: 0` and recovers real values by regexing the DASH/HLS manifest
(`dashFindBitDepth`, `dashFindSampleRate`, `hlsFindBitDepth`, `hlsFindSampleRate` in
`ref:tidal-sdk-web/packages/player/src/internal/helpers/manifest-parser.ts`). **[verified]**

The manifest-type choice is DRM-driven: the web SDK asks for HLS when
`shaka.drm.FairPlay.isFairPlaySupported()` is true (Safari/Apple) and MPEG-DASH otherwise. Android
always asks for `MPEG_DASH`. iOS asks for `.hls`. **[verified]**

#### 1.3 Manifest MIME types

`ref:tidal-sdk-web/packages/player/src/internal/constants.ts`:

```ts
export const mimeTypes = {
  BTS:  'application/vnd.tidal.bts',
  DASH: 'application/dash+xml',
  EMU:  'application/vnd.tidal.emu',
  HLS:  'application/vnd.apple.mpegurl',
} as const;
```

The Android SDK adds the same four (`ManifestMimeType`), and python-tidal has `MPD`, `BTS`, plus a
`VIDEO: "video/mp2t"` value and commented-out `EMU`/`APPL`
(`ref:python-tidal/tidalapi/media.py:112-120`).

**BTS** (`application/vnd.tidal.bts`) base64-decodes to JSON:

```json
{ "mimeType": "...", "codecs": "flac", "encryptionType": "NONE", "keyId": "...",
  "licenseSecurityToken": "...", "urls": ["https://..."] }
```

Handling: base64 → JSON → take `urls[0]` and feed it to the decoder as an ordinary HTTP URL.
Verified in `ref:sone/src-tauri/src/tidal_api.rs:3706-3725`,
`ref:tidal-sdk-web/.../manifest-parser.ts` (`parseJSONManifest`), and
`ref:high-tide/src/lib/player_object.py:515-522`.

**EMU** has the same shape (`{mimeType, urls}`) and the web SDK parses it with the same function.

**DASH** (`application/dash+xml`) base64-decodes to an MPD XML document. Three different strategies
are in use for feeding it to a player:

1. Wrap it back into a data URI and hand it to the demuxer:
   `data:application/dash+xml;base64,<b64>` — Sone
   (`ref:sone/src-tauri/src/commands/playback.rs:118-127`), Strawberry
   (`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:227-233`), TIDAL's own web SDK
   (`ref:tidal-sdk-web/.../manifest-parser.ts`).
2. Write the MPD to a file and pass `file://` — mopidy-tidal always
   (`ref:mopidy-tidal/mopidy_tidal/playback.py`, `as_stream`), and High Tide **only on
   GStreamer ≥ 1.26**, falling back to the data URI below that
   (`ref:high-tide/src/lib/player_object.py:492-512`). This is a strong hint that newer GStreamer
   stopped accepting the data-URI form for DASH. **[verified that the version check exists; the
   reason is [inferred]]**
3. Parse the MPD yourself and fetch segments — python-tidal (`DashInfo`) and tidal-cli.

**HLS** (`application/vnd.apple.mpegurl`) base64-decodes to a master playlist whose variant lines
are themselves base64 data URLs; the web SDK's HLS branch decodes the first line again before
scanning for `X-COM-TIDAL-SAMPLE-DEPTH` / `X-COM-TIDAL-SAMPLE-RATE`
(`ref:tidal-sdk-web/.../manifest-parser.ts`, HLS branch). **[verified]**

#### 1.4 DASH manifest structure and segment assembly

From `ref:python-tidal/tidalapi/media.py:744-860` (`DashInfo`), the MPD TIDAL emits has exactly one
`Period` → one `AdaptationSet` → one `Representation`, and the `Representation` has one
`SegmentTemplate` with one `SegmentTimeline`. Fields read:

- `MPD@mediaPresentationDuration` (ISO-8601 duration, e.g. `PT2M26.47S`)
- `AdaptationSet@contentType`, `AdaptationSet@mimeType` (`audio/mp4`)
- `Representation@codecs` (`flac`, `mp4a.40.2`, `mp4a.40.5`)
- `Representation@audioSamplingRate`
- `Representation@id` — carries `"FLAC,44100,16"`, i.e. codec, rate, bit depth. The web SDK takes
  the last integer as bit depth and returns `undefined` when the id has no comma (`HEAACV1`,
  `AACLC`). **[verified]**
- `SegmentTemplate@initialization`, `SegmentTemplate@media` (a `$Number$` template),
  `SegmentTemplate@timescale`
- `SegmentTimeline/S@d` (segment duration in timescale units) and `@r` (repeat count)

Segment count and URL generation differ between the two open implementations, and this is a real
ambiguity:

- python-tidal: `segments_count = 1 + 1 + Σ(S.r or 1)`, then
  `[media.replace("$Number$", str(i)) for i in range(segments_count)]` — starts at **0** and folds
  the init segment into the same numbering (`ref:python-tidal/tidalapi/media.py:828-875`).
- tidal-cli: downloads `initialization` separately, then numbers media segments from **1**
  (`ref:tidal-cli/src/playback.ts:120-170`).

Neither reads `SegmentTemplate@startNumber`. Any hand-rolled DASH assembler for streamboat must read
`startNumber` (default 1 per the DASH spec) rather than guessing. **[inferred]**

python-tidal also synthesises an HLS playlist from the parsed MPD (`DashInfo.get_hls`), emitting
`#EXTINF` values of `S.d / timescale` and a separate value for the last segment. That is a useful
trick for handing a TIDAL DASH track to any HLS-capable player. **[verified]**

**Container.** `AdaptationSet@mimeType = "audio/mp4"` with `codecs="flac"` means FLAC inside
fragmented MP4 (the `fLaC` sample entry plus a `dfLa` FlacSpecificBox). Both major demuxers handle
it: FFmpeg maps `MKTAG('f','L','a','C')` → `AV_CODEC_ID_FLAC` in `libavformat/isom_tags.c` and reads
`dfLa` in `mov_read_dfla` (`libavformat/mov.c`); GStreamer's `qtdemux` has `FOURCC_fLaC` and emits
`audio/x-flac` caps (`gst-plugins-good/gst/isomp4/qtdemux.c`). **[verified]**

**What was missing until recently.** GStreamer 1.26.10 (released around 24–26 December 2025) is
reported as adding "support for FLAC audio in DASH manifests". Before that, TIDAL FLAC-in-DASH
worked through the *legacy* `dashdemux` from gst-plugins-bad but not through `adaptivedemux2` /
`dashdemux2`. Sone's code comments say exactly this: it deliberately uses legacy `uridecodebin`
(not `uridecodebin3`) so it gets the legacy dash demuxer
(`ref:sone/src-tauri/src/audio.rs:255-262`, `ref:sone/src-tauri/src/audio.rs:3300-3308`).
**[verified for the Sone comments and the release reporting; the exact upstream commit is
[unverified]]**

#### 1.5 Encryption

`encryptionType` in a BTS manifest takes the values `NONE` and `OLD_AES`
(`ref:TidaLuna/plugins/lib.native/src/request/decrypt.ts:40-52`). python-tidal defaults
`encryption_type = "NONE"` for MPD streams and reads the field for BTS
(`ref:python-tidal/tidalapi/media.py:655-676`).

Strawberry refuses to play anything encrypted and says so in the UI, with a note that whether TIDAL
returns encrypted streams depends on the client ID in use
(`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:244-310`). It checks three independent
signals — the manifest's `encryptionType`, the response's `encryptionKey`, and the pair
`securityType`/`securityToken`.

Sone reaches the same conclusion from the other direction: it skips the two hi-res tiers entirely
when no `client_secret` is configured, with the comment *"those credentials typically return
encrypted DASH streams that require Widevine"*
(`ref:sone/src-tauri/src/commands/playback.rs:53-58`).

For DASH/HLS, TIDAL uses standard DRM: Widevine at `https://api.tidal.com/v2/widevine` and FairPlay
at `https://fp.fa.tidal.com/license` with the certificate at `https://fp.fa.tidal.com/certificate`
(`ref:tidal-sdk-web/packages/player/src/player/shakaPlayer.ts:625-670`,
`ref:tidal-sdk-ios/Sources/Player/Common/DRM/FairPlayLicenseFetcher.swift`). The v2 manifest carries
`drmData.licenseUrl` (`ref:tidal-sdk-android/.../PlaybackInfoRepositoryDefault.kt`).

TidaLuna implements `OLD_AES` decryption with a hardcoded master key. **streamboat must not do
this.** The correct behaviour is Strawberry's: detect a non-`NONE` `encryptionType`, a non-empty
`encryptionKey`, or a non-`NONE` `securityType`, refuse the stream, and tell the user their client
credentials produce protected streams. The one legitimate DRM path — a licensed Widevine/FairPlay
CDM, as tidal-hifi gets via castlabs Electron — is discussed in §5 and §7.

#### 1.6 Video

Videos use `GET /v1/videos/{id}/playbackinfopostpaywall` (`ref:sone/src-tauri/src/tidal_api.rs:3808`)
or `/v1/videos/{id}/urlpostpaywall` returning an M3U8 (`ref:python-tidal/tidalapi/media.py`), or the
v2 `/videoManifests/{id}`. Sone plays video through a completely separate path (hls.js in the
webview) and states in its README that *"Video audio is streamed and does not use the bit-perfect
lossless signal path that music tracks use"* (`ref:sone/README.md:501`).

#### 1.7 Immersive audio

- **Dolby Atmos** = E-AC-3 with Joint Object Coding. Format token `EAC3_JOC`; DASH signals it as a
  plain `codecs="ec-3"` adaptation set, which parses as `audio/eac3`
  (`ref:tidal-sdk-android/player/playback-engine/src/test/.../TidalTrackSelectionFactoryTest.kt:25`).
  Android maps `AudioMode.DOLBY_ATMOS` → codec string `eac3_joc`
  (`ref:tidal-sdk-android/player/playback-engine/src/main/kotlin/.../FormatHelper.kt`).
- **AC-4** appears in python-tidal's and iOS's codec enums but no reference client requests it.
- **Sony 360RA** = MPEG-H, codec `mha1`/`mhm1`. The iOS SDK returns `nil` for it. TIDAL removed all
  360RA content on 24 July 2024 (tracks became greyed out and unstreamable) at the same time as it
  replaced the MQA catalogue with FLAC. **[verified from multiple 2024 news reports; the TIDAL
  support article itself is blocked from this environment]**

#### 1.8 Error handling, sub-statuses, and the quality cascade

A playbackinfo failure comes back as HTTP 401 with a JSON body carrying `status` and `subStatus`.
Sone classifies them (`ref:sone/src-tauri/src/tidal_api.rs:14-45`):

```rust
const TERMINAL_SUB_STATUSES: &[u64] = &[4005, 4010, 4030, 4031, 4032, 4034, 4035];
// PLAYBACKINFO_SUB_STATUS_RANGE covers the whole 4xxx band.
// 4006 (streaming privileges lost) and 4033 (subscription up-sell) recover.
```

Critically, a 401 whose sub-status is in the playbackinfo range must **not** trigger a token refresh
and retry — the token is fine, the content is not
(`ref:sone/src-tauri/src/tidal_api.rs:1520-1530`).

TIDAL's own web SDK maps a subset to user-facing errors
(`ref:tidal-sdk-web/.../playback-info-resolver.ts`, `getErrorId`):

| subStatus | Meaning |
|---|---|
| 4010 | `PEMonthlyStreamQuotaExceeded` |
| 4032, 4035 | `PEContentNotAvailableInLocation` |
| 4033 | `PEContentNotAvailableForSubscription` |
| — HTTP 5xx or 429 | `PERetryable` |
| — HTTP 4xx other | `PENotAllowed` |

**Quality cascade.** Sone tries tiers highest→lowest and stops early on classes of error that a
lower tier cannot fix (`ref:sone/src-tauri/src/commands/playback.rs:48-90`):

```
ORDER = [HI_RES_LOSSLESS, HI_RES, LOSSLESS, HIGH]   // clipped to the user's ceiling
// drop the two hi-res tiers when no client_secret is present
for tier in tiers:
    ok            -> use it
    network error -> propagate immediately
    rate-limited or terminal-unplayable -> propagate immediately
    other         -> remember and continue
```

Sone's comment is worth quoting because it saves three requests per track:

> *"A rate-limit or a terminal-unplayable answer will not change at a lower tier — over-requesting
> quality returns 200 with a downgraded audioQuality, never an error. Walking the rest of the
> cascade only multiplies the request count by 4."*

tidalt does the same descending walk (`HI_RES_LOSSLESS → LOSSLESS → HIGH → LOW`,
`ref:tidalt/docs/architecture.md`). TIDAL's own SDKs do **not** cascade — they send the whole
`formats` array in one request and let the server pick, then read back the actual quality from
`formats` in the response. That is strictly better and should be preferred when using the v2 API.
**[inferred]**

**Rate limiting.** Sone implements a global cooldown gate: on a 429 it parses `Retry-After`
(delta-seconds form only; the HTTP-date form is rejected rather than mis-parsed), clamps to
[1, 120] s, defaults to 5 s, and stores an absolute deadline with `fetch_max` so concurrent 429s
lengthen but never shorten the cooldown (`ref:sone/src-tauri/src/rate_gate.rs`).

TIDAL's web SDK retries with a linear-ish backoff of `500 * (4 - retriesRemaining)` ms, up to 3
retries, only on network failure, HTTP 5xx, or 429
(`ref:tidal-sdk-web/.../playback-info-resolver.ts`, `fetchWithRetries`). Shaka is configured with
`{backoffFactor: 2, baseDelay: 1000, fuzzFactor: 0.5, maxAttempts: 5, timeout: 5000}` for manifest,
segment and DRM requests (`ref:tidal-sdk-web/packages/player/src/player/shakaPlayer.ts:720-760`).

---

### 2. Decoding

#### 2.1 What must be decoded

| Tier / mode | Codec | Container | Notes |
|---|---|---|---|
| LOW | HE-AAC v1 (SBR) | fMP4 or raw via BTS | Needs SBR, not just AAC-LC |
| HIGH | AAC-LC | fMP4 / M4A | |
| LOSSLESS | FLAC 16/44.1 | fMP4 (`fLaC`+`dfLa`) via DASH, or `.flac` via BTS/urlpostpaywall | |
| HI_RES_LOSSLESS | FLAC up to 24/192 | same | |
| Atmos | E-AC-3 JOC | fMP4 | Decoder is licensed; see §5 |
| legacy | MP3, ALAC | — | Present in enums, not observed in current tiers |

#### 2.2 Decoder stacks and their licences

| Stack | Language | FLAC | AAC-LC | HE-AAC | E-AC-3 | DASH | HLS | Licence | Notes |
|---|---|---|---|---|---|---|---|---|---|
| **GStreamer** (1.26+/1.28) | C, bindings everywhere | yes (`flacdec`, `qtdemux` `fLaC`) | yes (`avdec_aac`/`faad`) | yes | via `gst-libav` | yes (`dashdemux` legacy, `dashdemux2`) | yes (`hlsdemux2`) | Core LGPL-2.1+; plugin sets vary — `gst-plugins-ugly`/`gst-libav` pull in GPL/patent-encumbered code | Everything TIDAL needs works out of the box on Linux; Windows/macOS need shipping the plugin set |
| **FFmpeg / libav\*** | C | yes | yes | yes | yes | yes (`dashdec`) | yes | LGPL-2.1+ by default; GPL if built `--enable-gpl` | tidalt links `libavformat/libavcodec/libswresample` directly and streams via an AVIO callback with no temp files (`ref:tidalt/internal/player/avcodec.go`) |
| **libmpv** | C | yes (via FFmpeg) | yes | yes | yes | yes | yes | mpv is GPLv2+; an LGPLv2.1+ build exists via `--enable-lgpl`, intended specifically for libmpv, and disables some features | Only cross-platform stack that also gives exclusive output on all three desktops |
| **Symphonia** 0.6.1 | pure Rust | "excellent" | AAC-LC "great" | **no** ("in work or not started") | no | **no** | no | MPL-2.0 | No network/streaming layer, no DASH. Would need `dash-mpd` + a hand-rolled fetcher and an HE-AAC gap-filler |
| **Shaka Player** | TS, browser | via MSE, browser-dependent | yes | yes | browser-dependent | yes | yes | Apache-2.0 | What TIDAL's own web SDK uses. Requires a browser/Electron and therefore inherits Chromium resampling |
| **ExoPlayer / androidx.media3** 1.5.0 | Kotlin/Java | yes | yes | yes | yes | yes | yes | Apache-2.0 | What TIDAL's Android SDK uses |
| **AVPlayer / AVFoundation** | Swift | yes | yes | yes | yes | no (HLS only) | yes | Apple platform | What TIDAL's iOS SDK uses; drives the HLS manifest choice on Apple |

Licensing consequences for streamboat, assuming streamboat itself is GPL-3.0 (which is what High
Tide, Sone and Strawberry are):

- GStreamer core (LGPL-2.1+) and FFmpeg (LGPL-2.1+) are compatible with anything.
- `gst-libav` wraps FFmpeg; if FFmpeg is built with `--enable-gpl`, the result is GPL. Distro builds
  of `gstreamer1.0-libav` are normally LGPL FFmpeg. **[inferred — depends on the distro]**
- libmpv default build is GPLv2+, which is compatible with GPL-3.0 only if mpv is GPLv2-**or-later**
  (it is). If streamboat ever wants a permissive licence, the LGPL mpv build is the escape hatch.
- Symphonia is MPL-2.0, file-level copyleft, compatible with everything.

#### 2.3 AAC patent status in 2026

- AAC is covered by the Via Licensing Alliance ("Via LA") AAC pool, which still runs an active
  per-unit programme with rates roughly US$0.10–0.98 per unit and volume tiers.
  **[verified from Via LA's own programme page and 2026 reporting]**
- Individual AAC/HE-AAC patents have been expiring on a country-by-country basis (e.g. one HE-AAC v2
  patent expired in 2023), and there is no single global expiry date. **[verified from patent-pool
  reporting]**
- **No source found in this pass says the AAC pool has closed or that AAC is royalty-free in 2026.**
  Treat AAC as still encumbered. **[verified as an absence, not a presence]**

Practical consequence: shipping an AAC decoder in a binary is the same legal exposure every media
player has. The mitigations everyone uses are (a) rely on system/distro codecs
(`gstreamer1.0-libav`, the OS decoder), (b) do not bundle a decoder in the release artefact, (c) let
the user choose LOSSLESS-only and never request `LOW`/`HIGH`. FLAC is patent-free and BSD-licensed,
so an all-FLAC streamboat has no codec licensing question at all. **[inferred]**

---

### 3. Output backends and bit-perfect / exclusive modes

#### 3.1 Linux — ALSA `hw:` direct

Opening `hw:CARD,DEV` bypasses `dmix`, `plug`, and (with the D-Bus handshake in §3.2) the sound
server. Two reference implementations do it properly.

**Sone** (`ref:sone/src-tauri/src/audio.rs`) — GStreamer decodes into an `appsink`; a dedicated
`alsa-writer` thread owns the `snd_pcm_t`.

Format probing (`probe_supported_gst_formats`, lines 483–507) tests, in this priority order:
`S32LE`, `S24LE` (= GStreamer `S24_32LE`), `S243LE` (= GStreamer `S24LE`), `FloatLE`, `S16LE`.
Note the naming inversion, documented in `alsa_format_to_gst` (lines 202–217):

```
ALSA S24LE   = 24-in-32 container = GStreamer S24_32LE (4 bytes/sample)
ALSA S243LE  = packed 24-bit      = GStreamer S24LE    (3 bytes/sample)
```

Getting this backwards is a silent 8-bit shift. Rate probing (`probe_supported_rates`, lines
545–567) tests `44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000, 705600, 768000`.

Bit-perfect format selection (`pick_capsfilter_format`, lines 516–544):

1. pass through if the DAC accepts the source format;
2. otherwise the **narrowest lossless promotion** the DAC accepts —
   `S16LE → [S24LE, S24_32LE, S32LE]`, `S24LE → [S24_32LE, S32LE]`,
   `S24_32LE → [S24LE, S32LE]`. These are pure integer container changes, valid because
   `audioconvert` is configured `dithering=none noise-shaping=none`;
3. otherwise the DAC's widest probed format, and report the conversion honestly to the UI.

`configure_alsa_hwparams` (lines 569–751) is the part worth copying wholesale:

- `set_access(RWInterleaved)`
- bit-perfect: `set_format(requested)` must succeed, no fallback ladder
- bit-perfect: `set_rate_resample(false)` — **this is the one call that makes ALSA bit-perfect**
- `set_rate(rate, Nearest)`, then read back `get_rate()` and **fail** if it differs, with the
  message "DAC doesn't support {n}kHz — turn off bit-perfect mode for compatibility"
- channel negotiation with a fallback to `get_channels_min()`, because pro USB interfaces
  (Focusrite, Audient) expose a fixed channel count and reject stereo
- `set_buffer_time_near(500_000)` (500 ms), `set_period_time_near(50_000)` (50 ms)
- **`sw_params`**: `snd_pcm_hw_params()` resets `start_threshold` to 1, which underruns from the
  first write. Sone restores `start_threshold = floor(buffer/period) * period` and
  `avail_min = period`, matching what `alsasink` does.

The writer loop (lines 830+) handles: generation counters so stale PCM from a torn-down pipeline is
dropped; hardware pause via `snd_pcm_pause` when `hw_params.can_pause()`, otherwise a software pause
that writes 50 ms silence buffers to pace the thread and then `drop()`+`prepare()` to flush the
silence; XRUN recovery (`EPIPE` → `prepare()` then a silence kick), suspend recovery (`ESTRPIPE` →
`resume()` retry loop on `EAGAIN`), `ENODEV` → surface "device disconnected"; and a full
close-and-reopen of the PCM on format change, with the comment that *"Some hardware (e.g. XMOS USB
controllers) can't reconfigure HW params in-place after `snd_pcm_drop()`"*.

**tidalt** (`ref:tidalt/internal/player/alsa.c`, `ref:tidalt/internal/player/mpv.go`) — CGO,
FFmpeg decode + libasound out. Different and equally instructive choices:

- The format preference for **16-bit** sources is `S32_LE → S16_LE → S24_3LE → S24_LE`, because
  *"many USB DACs (e.g. CS43198-based devices) have a buggy or non-functional S16_LE USB endpoint but
  work correctly via their native 32-bit endpoint"*. For **24-bit** sources it is
  `S24_3LE → S24_LE → S32_LE`.
- Period size is set **first** (1024 frames), buffer afterwards at `4 × period`. The comment explains
  why: setting buffer first and querying `period_size_min` returns absurd values on some DACs
  ("87 frames on the Hidizs S9 Pro Plus"), producing ~1000 interrupts/s and audible distortion.
- `snd_pcm_hw_params_get_sbits()` is read back to report the hardware's *significant* bit depth
  (e.g. 24 in an S32_LE container).
- `open_hw_device` and `configure_hw_pcm` are split so that "device busy" (retry on `hw:`) is
  distinguishable from "format refused" (fall back to `plughw:`, mark the path as no longer
  bit-perfect, and show the user a `(converted)` badge). The result is memoised per device.

#### 3.2 PipeWire / PulseAudio, and getting the device released

`autoaudiosink` in GStreamer picks `pipewiresink`/`pulsesink`/`alsasink` at runtime, and the child
is added asynchronously so you cannot inspect it synchronously
(`ref:sone/src-tauri/src/audio.rs:1977-1979`).

**Device reservation.** To take `hw:` while PipeWire holds it you must speak
`org.freedesktop.ReserveDevice1`. tidalt's `reserveALSADevice`
(`ref:tidalt/internal/player/mpv.go:314-395`) is a complete, correct implementation:

1. connect to the **session** bus; if there is none, skip reservation and try the device directly;
2. call `org.freedesktop.ReserveDevice1.RequestRelease(int32 MaxInt32)` on
   `org.freedesktop.ReserveDevice1.Audio{N}` at `/org/freedesktop/ReserveDevice1/Audio{N}`, with a
   500 ms deadline;
3. distinguish three outcomes and keep them distinct:
   - reply `released == false` → explicit refusal, fail;
   - deadline exceeded → an owner exists but is slow; **back off, do not steal** (stealing with
     `ReplaceExisting` is exactly what the protocol exists to prevent);
   - any other call error → nobody owns the name, the device is free, proceed;
4. wait 200 ms for the previous owner to actually close its handle;
5. `RequestName(name, NameFlagReplaceExisting | NameFlagAllowReplacement)` and require
   `RequestNameReplyPrimaryOwner`;
6. hold the name for the whole playback and release on stop.

tidalt also budgets the whole handshake so it fits inside its 3 s shutdown window:
`releaseCallTimeout 500ms + releaseSettleDelay 200ms + openBusyRetryBudget 800ms = 1.5s`, with
`EBUSY` retried every 100 ms.

Sone does **not** do this: it opens the PCM eagerly and maps `EBUSY` to a `device_busy` error string,
telling the user in the FAQ to stop whatever else holds the device
(`ref:sone/src-tauri/src/audio.rs:770-779`, `ref:sone/README.md:506-513`). That is a worse user
experience and streamboat should implement the reservation. **[inferred]**

**Reading back what the mixer is doing.** Sone's `pipeline_probe.rs` shells out to `pactl info` and
`pactl list sinks` (with `LC_ALL=C` forced on the child) to read the server name, default sink,
sample spec, volume in dB (converting to a linear multiplier as `10^(dB/20)` rather than trusting the
cubic-mapped percentage) and mute state; it maps the sink to `/proc/asound/<alsa.id>` and parses
`/proc/asound/<card>/pcm{N}p/sub{M}/hw_params` for the kernel's ground truth (format, rate, channels,
period_size, buffer_size, or the literal word `closed`). This feeds a "signal path transparency"
panel (`ref:sone/src-tauri/src/signal_path.rs`) that reports resampling, bit-depth promotion, format
fallback, software volume and ReplayGain factor. It is the single best user-facing idea in any of
these projects and streamboat should copy it. **[verified; the recommendation is [inferred]]**

**PipeWire's own capabilities.** PipeWire's audio adapter supports a passthrough mode in which it
performs no conversions, which is the mechanism behind exclusive access; `default.clock.rate` and
`default.clock.allowed-rates` control which rates the graph will switch to, and per-device
`audio.format` can be pinned in WirePlumber. **[verified from PipeWire docs summaries; the exact
config keys should be re-checked against docs.pipewire.org before being written into streamboat's
docs — see Open questions]** mpv exposes `--audio-exclusive=yes` for its `pipewire` AO. **[verified]**

#### 3.3 Sandbox implications

- **Flatpak.** High Tide's manifest (`ref:high-tide/build-aux/io.github.nokse22.high-tide.json`)
  grants `--socket=pulseaudio` and `--filesystem=xdg-run/pipewire-0:ro` and nothing else. There is
  no `--device=all`, so `/dev/snd` is not visible and raw ALSA `hw:` is impossible inside that
  sandbox — even though the manifest bundles `alsa-utils` and `libasound`. High Tide's ALSA sink
  option therefore only works outside Flatpak, or with a manually widened permission set.
  **[verified for the manifest contents; the /dev/snd conclusion is [inferred] from Flatpak's
  documented permission model]**
- High Tide detects confinement with `Xdp.Portal.running_under_flatpak()` and skips the keyring
  auto-unlock when confined (per the project survey; the pattern is the right one for any
  sandbox-conditional behaviour).
- **Snap.** Sone ships `plugs: [audio-playback, alsa]` and instructs
  `sudo snap connect sone:alsa` for exclusive output, because the `alsa` interface is not
  auto-connected (`ref:sone/snap/snapcraft.yaml`, `ref:sone/README.md:224-228`).
- **Consequence for streamboat:** exclusive/bit-perfect mode is a *packaging* feature, not just a
  code feature. Plan for at least three Linux artefacts — an unconfined native package (deb/rpm/AUR/
  Nix) that can do `hw:`, a Flatpak that cannot and must say so in the UI, and a Snap with the
  `alsa` plug. **[inferred]**

#### 3.4 Windows — WASAPI and ASIO

- `wasapi2sink` (gst-plugins-bad) has a real `exclusive` boolean property. Verified by reading
  `subprojects/gst-plugins-bad/sys/wasapi2/gstwasapi2sink.cpp`: the property enum contains
  `PROP_EXCLUSIVE` alongside `PROP_DEVICE`, `PROP_LOW_LATENCY`, `PROP_MUTE`, `PROP_VOLUME`,
  `PROP_DISPATCHER`, `PROP_CONTINUE_ON_ERROR`.
- sone-windows uses exactly that: `wasapi2sink` with `exclusive` from the setting, `low-latency=true`
  and `device` from the picker (`ref:sone-windows/src-tauri/src/audio.rs:1236-1246`). Toggling
  exclusivity mid-playback is done by dropping the pipeline to `Ready`, setting the properties, going
  back to `Playing` and re-seeking to the saved position — because the device must actually be
  released and re-acquired (`ref:sone-windows/src-tauri/src/audio.rs:1580-1602`).
- Strawberry generalises this: any sink with an `exclusive` property gets it set
  (`ref:strawberry/src/engine/gstenginepipeline.cpp:722-727`), and it *derives* exclusivity on Linux
  from the device string starting with `hw:` or `plughw:`
  (`ref:strawberry/src/engine/gstenginepipeline.cpp:632-637`).
- Strawberry's default Windows sink is **`directsoundsink`**, not WASAPI:
  `gststartup.cpp` raises `directsoundsink` to `GST_RANK_PRIMARY` and demotes `wasapisink` and
  `wasapi2sink` to `GST_RANK_SECONDARY`, with the comment *"wasapisink does not support device
  switching and wasapi2sink has issues, see #1227"*
  (`ref:strawberry/src/engine/gststartup.cpp:59-74`). That is a caution flag for GStreamer on
  Windows. **[verified]**
- mpv's `wasapi` AO supports `--audio-exclusive=yes` plus
  `--wasapi-exclusive-buffer=<default|min|1-2000000>` (`mpv DOCS/man/ao.rst`).
- **ASIO.** Strawberry has an `AsioDeviceFinder` targeting an `asiosink`
  (`ref:strawberry/src/engine/asiodevicefinder.cpp`), so a GStreamer ASIO sink exists. No reference
  TIDAL client uses ASIO. ASIO SDK redistribution has its own Steinberg licence terms.
  **[verified that Strawberry references `asiosink`; the licensing note is general knowledge and
  [unverified] here]**
- Device enumeration on Windows: Strawberry has `MMDeviceFinder` (for `wasapisink`/`wasapi2sink`),
  `UWPDeviceFinder` (`wasapi2sink_`) and `DirectSoundDeviceFinder`. sone-windows enumerates via
  GStreamer's `DeviceMonitor` filtered on `device.api ∈ {wasapi, wasapi2}` reading `device.id`
  (`ref:sone-windows/src-tauri/src/audio.rs:2040-2055`).
- Rust crate option: `wasapi` 0.24.0 (MIT) exposes exclusive-mode format probing directly, for a
  non-GStreamer path. **[verified from crates.io]**

#### 3.5 macOS — CoreAudio

This is the weakest platform in every reference implementation.

- `osxaudiosink` properties, read from `gst-plugins-good/sys/osxaudio/gstosxaudiosink.c`:
  `device` (int device ID), `unique-id` (string), `configure-session`, `volume`. **There is no
  `exclusive` property and no hog-mode property.**
- GStreamer's CoreAudio HAL *does* implement hog mode — `_audio_device_get_hog` /
  `_audio_device_set_hog` around `kAudioDevicePropertyHogMode`, plus
  `_audio_device_set_mixing(device, FALSE)` and physical-format setting via
  `kAudioStreamPropertyPhysicalFormat` with a four-attempt confirm loop. But the only call site is
  `_open_spdif()` (`gst-plugins-good/sys/osxaudio/gstosxcoreaudiohal.c:682-740`), reached only when
  `core_audio->is_passthrough` is set. **So GStreamer cannot do hog-mode bit-perfect PCM on macOS.**
  **[verified]**
- mpv can: the `coreaudio` AO honours `--audio-exclusive=yes` and auto-redirects to the dedicated
  `coreaudio_exclusive` AO ("native macOS audio output driver using direct device access and
  exclusive mode (bypasses the sound server)") for compressed formats. `--coreaudio-change-physical-format=yes`
  changes the device's physical format — equivalent to changing Format in Audio MIDI Setup, and it
  changes the setting system-wide. There is also an `avfoundation` AO. **[verified from mpv's
  `DOCS/man/ao.rst` and `DOCS/man/options.rst`]**
- CamillaDSP (Rust, a good non-TIDAL reference) exposes an `exclusive` flag on its CoreAudio playback
  device, works internally in 32-bit float and lets CoreAudio convert unless an explicit physical
  `format` (S16/S24/S32/F32) is requested, monitors CoreAudio notifications for device rate changes
  and closes/reopens on change, and warns that hog mode breaks virtual devices like BlackHole.
  **[verified from `camilladsp/backend_coreaudio.md`]**
- `coreaudio-rs` 0.14.2 (MIT/Apache-2.0) is the Rust binding if a native CoreAudio backend is
  written. **[verified from crates.io]**
- The TIDAL web SDK's *native* player component (a proprietary C++ blob, not in the repo) has
  `selectDevice(device, mode)` where mode is `'exclusive' | 'shared'`, and emits
  `deviceexclusivemodenotallowed`, `deviceformatnotsupported`, `devicelocked`, `devicenotfound`,
  `devicevolumenotsupported`, `devicedisconnected`. There is also an
  `active-device-pass-through-changed` event. This is the shape of the API TIDAL itself considers
  necessary, and it is a good checklist of error states to handle
  (`ref:tidal-sdk-web/packages/player/src/player/nativeInterface.ts`,
  `ref:tidal-sdk-web/packages/player/src/api/event/active-device-pass-through-changed.ts`).
  **[verified]**

#### 3.6 Sample-rate switching, resampling, dither, volume, channel mapping

**Automatic per-track rate switching.** In Sone's exclusive path, the pipeline never resamples in
bit-perfect mode (there is deliberately no `audioresample` element); the ALSA writer sees a
`PcmFormat` change on the incoming chunk, closes the PCM, calls `configure_alsa_hwparams` again at
the new rate, and reopens. The user-visible failure when the DAC does not support the new rate is a
friendly message rather than GStreamer's opaque "Internal data stream error" — Sone achieves this by
deliberately **not** constraining the appsink's rate caps in bit-perfect mode, letting the source rate
reach the writer so the ALSA open is the thing that fails
(`ref:sone/src-tauri/src/audio.rs:2905-2930`, and lines 645–700 for the reopen).

In non-bit-perfect exclusive mode Sone does the opposite: it pins the appsink caps to the DAC's
probed rate list and inserts `audioresample`, then emits a `Resampling { from, to }` message so the
transparency panel can say so honestly.

**Dither.** `audioconvert dithering=none noise-shaping=none` in bit-perfect mode. Sone sets both
(`ref:sone/src-tauri/src/audio.rs:2938-2940`). Dither only matters when reducing bit depth; the
promotions Sone allows are all widening or lossless container changes, so no dither is ever needed.
**[verified]**

**Volume.**

- Normal mode: two `volume` elements in series — `norm_vol` (ReplayGain) and `user_vol` (slider) —
  before the sink (`ref:sone/src-tauri/src/audio.rs:1833-1841`).
- Exclusive non-bit-perfect: no GStreamer `volume` elements; the ALSA writer scales PCM in place with
  a `combined_vol` atomic (`amp * norm_gain`), with per-format clamping including correct 24-bit
  packed handling (`ref:sone/src-tauri/src/audio.rs:965-1010`).
- Bit-perfect: **volume control is disabled entirely**; Sone's README states *"The volume slider is
  locked at 100% and disabled"* (`ref:sone/README.md:521`).
- Taper: Sone uses a cubic curve, `amplitude = slider³`, described as roughly a 50 dB range
  (`slider_to_amplitude`, `ref:sone/src-tauri/src/audio.rs:219-226`). High Tide offers an optional
  quadratic mapping (`volume²`, with `volume^(1/2)` on read-back)
  (`ref:high-tide/src/lib/player_object.py:746-770`).
- **Recommendation for streamboat:** in bit-perfect mode, offer mute-only (write zeros or stop the
  stream) and no attenuation; anything else quantises. **[inferred]**

**Channel mapping.** Sone builds an explicit `mix-matrix` on `audioconvert` at pipeline-build time
when the negotiated device channel count exceeds 2, mapping stereo into channel 0/1 and silence
elsewhere (`stereo_pad_mix_matrix`, `ref:sone/src-tauri/src/audio.rs:2835-2841`). Doing it at build
time rather than on `pad-added` avoids a 2-channel transient that would thrash the ALSA writer.
mpv's ALSA AO defaults to `--audio-channels=auto-safe`, which *rejects* multichannel on a hardware
device because there is no reliable way to detect support. **[verified]**

---

### 4. Playback behaviour

#### 4.1 Gapless

Three distinct designs are in the references.

**(a) `playbin3` + `about-to-finish` — High Tide.**
`ref:high-tide/src/lib/player_object.py:86-100`: create `playbin3`, connect `about-to-finish`, set
`gapless_enabled = True`; if `playbin3` cannot be created, fall back to `playbin` and set
`gapless_enabled = False`. On `about-to-finish`, `GLib.idle_add(self.play_next, True)` sets the next
URI on the same playbin (`play_next_gapless`, lines 631–642). A `use_about_to_finish` flag suppresses
it during sink swaps. **High Tide disables gapless entirely when the sink is `pipewiresink`**
(lines 96–98 and 211–215) — an explicit, unexplained incompatibility worth investigating.
**[verified that the code does this; the reason is [unverified]]**

**(b) `concat` with two prerolled branches — Sone.**
This is the more robust design. The pipeline head is:

```
uridecodebin(track A) → queue(A) ─┐
                                  ├─ concat → audioconvert → audioresample → norm_vol → user_vol → autoaudiosink
uridecodebin(track B) → queue(B) ─┘
```

Details worth stealing (`ref:sone/src-tauri/src/audio.rs:255-480`, 1785–1960):

- Each branch queue: `max-size-time = 15 s`, `max-size-buffers = 0`, `max-size-bytes = 0`. The queue
  decouples the next decoder from `concat`'s gate on the inactive pad so it can pre-buffer while the
  current track plays.
- `uridecodebin buffer-duration` is 15 s for DASH and 5 s for BTS, `use-buffering = true`.
- **All pad-slot operations on `concat` are funnelled through one serialized executor thread** (an
  `AttachJob::{Attach,Detach}` mpsc), because concurrent request/release of request pads races.
- Detach order matters: unlink and `release_request_pad` on `concat` **before** nulling the bin,
  because `concat` hard-blocks the inactive sink pad and a null-first teardown deadlocks.
- The advance is detected by `concat.connect_notify("active-pad")` and verified **by pad identity**,
  not by assuming.
- Under `concat` with `adjust-base=true`, `query_position(TIME)` is already re-based per track, so no
  offset arithmetic is needed.
- A flush seek does **not** disturb the prerolled branch: the `FLUSH_START`/`FLUSH_STOP`/`SEGMENT`
  only reach `concat`'s active sink pad. Sone's comment records that this was empirically verified on
  GStreamer 1.24.2, and that detaching on seek was a bug that threw away a valid preroll on every
  seek.
- `gapless_supported()` is just `ElementFactory::find("concat").is_some()` — `concat` ships in
  coreelements, so effectively always true. **This contradicts Sone's own README, which says gapless
  "requires GStreamer 1.24+"** (`ref:sone/README.md:66` vs `ref:sone/src-tauri/src/audio.rs:3300-3308`).
  The code comment explicitly says "no GStreamer 1.24 or uridecodebin3 requirement". Treat the README
  as stale.
- Gapless is **normal-mode only** in Sone. The DirectAlsa backend never gets a second branch (the
  worker gates dispatch on backend type). So on Sone, bit-perfect and gapless are mutually exclusive.
  **[verified]**

**(c) Dual media elements / dual Shaka instances — TIDAL's own web SDK.**
`ref:tidal-sdk-web/packages/player/src/player/shakaPlayer.ts:100-120`:

```ts
static readonly #GAPLESS_CROSSFADE_MS = 250;
static readonly #GAPLESS_START_BEFORE_END_S = 0.25;
```

with the comment that the ramp is *"long enough to mask buffer boundaries but short enough that
listeners perceive the transition as truly gapless"*, executed as ~16 `setTimeout` ticks over 250 ms.
The primary trigger is a duration-derived timer; `timeupdate` (≈4 Hz) is only a fallback because it
can skip the 250 ms window. Two `<video>` elements are pre-created and mounted into a hidden
`#tidal-player-root` div (`ref:tidal-sdk-web/packages/player/src/player/audio-context-store.ts`).
`ref:tidal-sdk-web/packages/player/src/player/browserPlayer.ts` does the same with a
`#preloadPlayer` HTMLVideoElement whose `src` is swapped into the current player on transition.

**Conclusion:** TIDAL's own reference web implementation is not sample-accurate gapless. A native
client using `concat` (GStreamer) or mpv's `--gapless-audio` can beat it. **[inferred]**

**(d) mpv.** `--gapless-audio=<no|yes|weak>`, default `weak`: keep the device open, but close and
reopen if the decoder's output format changes; `yes` keeps the first file's format and resamples
everything else to it — which is exactly wrong for a hi-res client. For streamboat on libmpv the
setting must be `weak`, never `yes`. **[verified from mpv `DOCS/man/options.rst`]**

#### 4.2 Prefetch

- **Manifest prefetch.** TIDAL supports it explicitly: the legacy playbackinfo call takes a
  `x-tidal-prefetch: true` header, and `StreamInfo` carries a `prefetched` boolean end to end
  (`ref:tidal-sdk-web/.../playback-info-resolver.ts`, `ref:tidal-sdk-web/.../manifest-parser.ts`).
  There is also a `preload-request` event in the SDK's public API.
- **Media prefetch.** Sone prerolls the whole next branch (decoder + 15 s queue) during the current
  track. Shaka/browser preload a second element. The native player component has `preload(url,
  streamFormat, encryptionKey?)` and `cancelPreload()`.
- **Segment concurrency.** TidaLuna limits segment fetches to **2 concurrent** with a semaphore,
  noting it matches official client behaviour (per the project survey of
  `ref:TidaLuna/plugins/lib.native/src/request/fetchMediaItemStream.ts`).

#### 4.3 Crossfade

TIDAL's own player config has it (`ref:tidal-sdk-web/packages/player/src/config.ts`):

```ts
/**
 * Controls track transition behavior:
 *  - Zero: gapless (default)
 *  - Positive: crossfade overlap in ms (max 15000)
 */
crossfadeInMs: number;   // default 0
```

Strawberry implements crossfade with a `QTimeLine` fader across two pipelines, and explicitly
**refuses to crossfade when a pipeline is in exclusive mode** — `AnyExclusivePipelineActive()` gates
it, and a new exclusive pipeline waits for the old one to finish
(`ref:strawberry/src/engine/gstengine.cpp:233-290`, 1178–1200). That is the correct relationship:
exclusive device access means one pipeline at a time, so crossfade and exclusive mode are mutually
exclusive by construction. **[verified]**

No unofficial TIDAL client in the reference set implements crossfade.

#### 4.4 Loudness normalization

**The canonical formula.** `ref:tidal-sdk-android/player/playback-engine/src/main/kotlin/com/tidal/sdk/player/playbackengine/volume/LoudnessNormalizer.kt`:

```kotlin
fun getReducedGain(replayGain: Float, peakAmplitude: Float, preAmp: Int): Float {
    val replayGainLinear = 10.0f.pow((replayGain + preAmp) / 20)
    if (peakAmplitude <= 0) return min(replayGainLinear, 1.0f)
    return min(replayGainLinear, 1 / peakAmplitude)
}
```

with `LOUDNESS_NORMALIZATION_PRE_AMP_DEFAULT = 4`, `LOUDNESS_NORMALIZATION_PRE_AMP_TV = 0`, and modes
`NONE | TRACK | ALBUM` defaulting to `ALBUM`
(`ref:tidal-sdk-android/.../volume/VolumeHelper.kt`).

The web SDK is the same with `peak` hardcoded to 1 — i.e. it ignores the peak amplitude entirely
(`ref:tidal-sdk-web/packages/player/src/internal/helpers/normalize-volume.ts`):

```ts
export function normalizeVolume(replayGain: number): number {
  const peak = 1; const preAmp = 4;
  return Math.min(Math.pow(10, (preAmp + replayGain) / 20), 1 / peak);
}
```

and it applies the result by multiplying the user's volume
(`ref:tidal-sdk-web/packages/player/src/player/basePlayer.ts:254-264`), skipping the update during a
seamless transition so the crossfade is not disturbed.

**Sone** adds an extra 0.8 factor (`ref:sone/src-tauri/src/commands/playback.rs:10-21`):

```rust
pub fn compute_norm_gain(replay_gain: Option<f64>, peak_amplitude: Option<f64>) -> f64 {
    match replay_gain {
        Some(rg) => { let pre_amp = 4.0;
                      let linear = 10.0_f64.powf((rg + pre_amp) / 20.0);
                      let peak = peak_amplitude.filter(|&p| p > 0.0).unwrap_or(1.0);
                      0.8 * linear.min(1.0 / peak) }
        None => 1.0,
    }
}
```

Album/track selection is context-aware: `use_track_gain` picks track values with album as fallback,
otherwise album with track as fallback (`ref:sone/src-tauri/src/commands/playback.rs:130-150`). The
gain is applied **before** `play_url` so the pipeline builds with the right value and there is no
volume spike at track start.

**High Tide** does it entirely inside GStreamer with the standard ReplayGain elements
(`ref:high-tide/src/lib/player_object.py:196-207`):

```
taginject name=rgtags {tags} ! rgvolume name=rgvol pre-amp=4.0 fallback-gain=-10 headroom=6.0 ! rglimiter ! audioconvert !
```

with the pre-amp comment *"the pre-amp value is set to match tidal webs volume"*. Tags are injected
per track as `replaygain-track-gain=…,replaygain-track-peak=…` or the album equivalents, and values
of exactly `1.0` are skipped as a sentinel for "missing", referencing
`https://github.com/EbbLabs/python-tidal/issues/332` with the note *"Rather quiet album than broken
eardrums"* (`ref:high-tide/src/lib/player_object.py:580-608`).

**Does TIDAL normalize server-side?** No evidence of it in any reference. All three of TIDAL's own
SDKs apply gain client-side from `replayGain`/`peakAmplitude` returned by the manifest endpoint, and
`NONE` mode simply means unity. **[verified as an absence]**

#### 4.5 Buffering strategy

| Implementation | Setting |
|---|---|
| Sone, GStreamer source | `uridecodebin buffer-duration` = 15 s (DASH) / 5 s (BTS), `use-buffering=true` |
| Sone, branch queue | `queue max-size-time = 15 s`, buffers/bytes unlimited |
| Sone, ALSA | `buffer_time ≈ 500 ms`, `period_time ≈ 50 ms`, `start_threshold` = full buffer, `avail_min` = period |
| Sone, appsink | `max-buffers = 20`, `sync = false` |
| Sone, writer channel | `crossbeam bounded(256)` |
| tidalt, ALSA | `period = 1024 frames`, `buffer = 4 × period` (≈93 ms at 44.1 kHz), ALSA-default `sw_params` |
| Shaka (TIDAL web) | `bufferingGoal = 40 s`, `bufferBehind = 40 s`, `defaultPresentationDelay = 0`, `disableText`, `disableThumbnails` |
| ExoPlayer (TIDAL Android) | `backBuffer = 20 s`, `minPlaybackBuffer = maxPlaybackBuffer = 2 min`, `bufferForPlayback = 2.5 s`, `bufferForPlaybackAfterRebuffer = 5 s`, `audioTrackBuffer = 1.5 s` (`ref:tidal-sdk-android/player/playback-engine/src/main/kotlin/.../model/BufferConfiguration.kt`) |

The ExoPlayer numbers are the closest thing to an official statement of what TIDAL considers a
sensible buffer: **two minutes of media buffer** and a 1.5 s device buffer.

#### 4.6 On-disk cache

Three quite different approaches:

- **High Tide** caches whole tracks to `MUSIC_DIR/{track.id}_{quality}.m4a`, unencrypted, and skips
  caching entirely on a metered network (`Gio.NetworkMonitor.get_default().get_network_metered()`).
  MPD tracks are remuxed with `ffmpeg -protocol_whitelist file,crypto,data,http,https,tcp,tls -i
  manifest.mpd -f mp4 -c copy` into a `.tmp` then atomically renamed; BTS tracks are downloaded with
  `requests` in 8192-byte chunks. Playback checks the cache first
  (`ref:high-tide/src/lib/player_object.py:470-580`). There is no size cap and no encryption.
- **mopidy-tidal** runs an HTTP relay proxy on localhost in front of TIDAL's CDN, backed by SQLite.
  Requests are looked up by path; a miss is relayed upstream while the bytes are simultaneously
  streamed to GStreamer and inserted into the cache in chunks; the insertion is only *finalised*
  when the whole resource arrives, and un-finalised data is dropped at next startup. It also stores a
  `TidalID → Path` mapping so a cached track can be resolved fully offline without calling the API.
  Range requests are supported for seeking
  (`ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/proxy.py`,
  `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/cache.py`,
  `ref:mopidy-tidal/mopidy_tidal/playback.py`).
- **Sone** caches only *metadata*, not audio, and it caches it **encrypted** (`crypto.rs` /
  `aes-gcm`) with tiered TTLs and stale-while-revalidate grace, a 2 GB cap and LRU eviction to
  1.8 GB (`ref:sone/src-tauri/src/cache.rs`):

  | Tier | Contents | TTL | SWR grace |
  |---|---|---|---|
  | `UserContent` | playlists, favourites | 15 min | 1 h |
  | `Dynamic` | artist bios, charts, home | 4 h | 24 h |
  | `StaticMeta` | album tracklists, credits | 7 d | 30 d |
  | `Image` | album art, avatars | 30 d | 90 d |

  Sone never persists a manifest: High Tide's survey note and Sone's structure agree that stream
  manifests are in-memory only per session.

**Legal framing for streamboat:** caching for a logged-in subscriber during a session is a player
concern; building a persistent, quality-tagged, indefinitely-retained library of decrypted audio
files is a ripper. High Tide's `MUSIC_DIR` cache sits uncomfortably close to the line. If streamboat
implements offline caching it should be capped, evicted, encrypted at rest, tied to the current
session's credentials, and purged on logout. **[inferred — this is a design position, not a legal
opinion]**

#### 4.7 Seeking within DASH

Sone seeks with `pipeline.seek_simple(SeekFlags::FLUSH | SeekFlags::KEY_UNIT, position)` on the
whole pipeline in both backends. In the DirectAlsa backend it additionally bumps the generation
counter, sends `WriterCommand::Flush`, and sets `frames_written` to `position * current_sample_rate`
so the position readout stays correct — because in exclusive mode position is derived from frames
written to ALSA, not from a GStreamer query
(`ref:sone/src-tauri/src/audio.rs:2307-2390`).

Seeking is also where the gapless preroll is easiest to break; see §4.1(b).

Server-side, DASH seeking is just a matter of jumping to the right `$Number$` — but the whole
`SegmentTimeline` is in the manifest, so the segment for any timestamp is computable offline. GStreamer
1.28 fixed "seeking in dashdemux2 for streams with gaps". **[verified from 1.28 release reporting]**

#### 4.8 Network resilience, token refresh, and privileges

- **Manifest expiry: 1 hour** (`MANIFEST_EXPIRATION_MS = 3600000`,
  `ref:tidal-sdk-web/.../playback-info-resolver.ts`). CDN URLs inside a BTS manifest carry their own
  expiring token; treat them as shorter-lived still. A track that sits paused for over an hour needs
  its manifest re-fetched before resume. **[verified for the constant; the resume implication is
  [inferred]]**
- **401 handling.** Refresh the token and retry — **unless** the 401 carries a playbackinfo
  sub-status, in which case do not refresh (`ref:sone/src-tauri/src/tidal_api.rs:1520-1540`).
- **429 handling.** Global cooldown gate, see §1.8.
- **Streaming privileges.** TIDAL enforces one active stream per account and pushes a revocation over
  a WebSocket. `POST {legacyApiUrl}/rt/connect` with a bearer token returns `{ url }`; connect to
  that URL and handle `{type: 'PRIVILEGED_SESSION_NOTIFICATION', payload: {clientDisplayName,
  sessionId, endsAt, updatedAt}}` and `{type: 'RECONNECT'}`; the client sends
  `{type: 'USER_ACTION', payload: {startedAt}}` to claim the privilege
  (`ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts`). The public event is
  `streaming-privileges-revoked` carrying the other device's name
  (`ref:tidal-sdk-web/packages/player/src/api/event/streaming-privileges-revoked.ts`). Sub-status
  4006 is the HTTP-side manifestation of the same thing and is explicitly non-terminal in Sone.
- **Play reporting.** Sone records the *actually served* attributes (`actual_product_id`, `quality`,
  `audio_mode`, `presentation`, timestamp) for TIDAL play-reporting
  (`ref:sone/src-tauri/src/commands/playback.rs:96-118`, `ref:sone/src-tauri/src/tidal_report/`).
  TIDAL's own SDKs send a rich `streaming_metrics` event set: `playback_info_fetch`,
  `streaming_session_start`, `streaming_session_end`, `playback_statistics`, `drm_license_fetch`
  (`ref:tidal-sdk-web/packages/player/src/internal/event-tracking/streaming-metrics/`). Whether
  streamboat should send any of this is an owner decision (see Open questions).

#### 4.9 Quality-tier transitions mid-stream

- With `adaptive=true` on the v2 API, the MPD carries multiple representations and the player's ABR
  switches between them mid-stream. TIDAL's web SDK enables Shaka's ABR (`abr: { enabled: true }`)
  and reports every switch as an `adaptation` event with `mimeType`, `codecs`, `bandwidth` and asset
  position (`ref:tidal-sdk-web/packages/player/src/player/adaptations.ts`). The default config is
  `audioAdaptiveBitrateStreaming: true` with `streamingWifiAudioQuality: 'LOW'`
  (`ref:tidal-sdk-web/packages/player/src/config.ts`).
- With `adaptive=false` (what tidal-cli requests) there is exactly one representation and no
  mid-stream switching.
- For a bit-perfect client, ABR is actively harmful: a mid-track representation change means a
  format/rate change means an ALSA reopen means a gap. **streamboat should request
  `adaptive=false`.** **[inferred]**
- Changing the *user's* quality ceiling takes effect on the next track, because it changes the
  `formats` array sent with the next manifest request. No reference client attempts a mid-track
  tier change.

---

### 5. Dolby Atmos and 360 Reality Audio on desktop — feasibility

**360 Reality Audio: dead.** Removed from TIDAL 24 July 2024; tracks are greyed out and unstreamable.
The iOS SDK returns `nil` for the mode. There is no `SONY_360RA` token in the v2 formats enum. **Do
not implement.** **[verified]**

**Dolby Atmos: possible in principle, hard in practice, and TIDAL's own desktop apps do not do it.**

What would be needed:

1. Request `EAC3_JOC` in the `formats` array (v2) or rely on the account's Atmos entitlement (v1).
2. Decode E-AC-3 with JOC, or pass the bitstream through untouched.
   - **Decoding to PCM:** FFmpeg's `eac3` decoder decodes E-AC-3 but **discards the JOC object
     metadata** — you get a 5.1 downmix, not Atmos. A real Atmos renderer requires a licence from
     Dolby. **[inferred from how JOC works; not verified against an FFmpeg source read in this pass]**
   - **Passthrough to an AVR over HDMI:** this is the realistic path. It requires an IEC 61937
     bitstream-passthrough output. On Linux that is `alsasink` on an `hdmi:` device with
     `audio/x-eac3` caps and the ALSA IEC958 channel status set; on Windows it is WASAPI exclusive
     with `WAVE_FORMAT_DOLBY_AC3_SPDIF`-class formats; on macOS it is CoreAudio's SPDIF path —
     which, notably, is precisely the one path where GStreamer's macOS backend *does* take hog mode
     (`_open_spdif`).
3. GStreamer has no Atmos renderer. mpv has `--audio-spdif=ac3,eac3,dts-hd,truehd` for passthrough.
   **[verified that mpv has SPDIF passthrough options via `--coreaudio-spdif-hack` references in
   `ao.rst`; the exact `--audio-spdif` value list is [unverified] in this pass]**

What the references do: **nothing.** No unofficial TIDAL client in the set plays Atmos. TIDAL's own
Android SDK plays it via ExoPlayer on hardware with an E-AC-3 decoder; the iOS SDK maps
`DOLBY_ATMOS → EAC3`; the web SDK's `AudioMode` type includes it but `_fetchTrackManifest` hardcodes
`audioMode: 'STEREO'` with the comment *"Only stereo (and mono) supported for now, TODO: revise or
remove if multi-channel is added"* (`ref:tidal-sdk-web/.../playback-info-resolver.ts`). That is
TIDAL's own web player declining to do Atmos.

Reporting (not first-party-verified here) says the TIDAL Windows and macOS desktop apps do not
support Dolby Atmos either, and that it is limited to mobile, TV and streamer hardware.

**Recommendation: do not implement Atmos or 360RA for v1.**
Instead:
- surface `audioModes`/`mediaMetadataTags` in the UI so users see that an Atmos version exists;
- when a track is Atmos-only, request the stereo formats and play the stereo version (which is what
  the format-array mechanism does naturally — omit `EAC3_JOC` and the server returns stereo);
- keep an `immersive` flag in the manifest-request layer so E-AC-3 passthrough can be added later
  without an architectural change;
- if Atmos is ever added, do it as **bitstream passthrough to an HDMI/AVR sink only**, never as a
  bundled renderer. **[inferred]**

---

### 6. OS media integration

| Concern | Linux | Windows | macOS |
|---|---|---|---|
| Now-playing / transport | MPRIS2 D-Bus (`org.mpris.MediaPlayer2`, `.Player`) | SMTC (`SystemMediaTransportControls`) | `MPNowPlayingInfoCenter` + `MPRemoteCommandCenter` |
| Cross-platform Rust crate | `souvlaki` 0.8.3 (MIT) or `mpris-server` 0.10.0 (MPL-2.0) | `souvlaki` | `souvlaki` |
| Media keys | MPRIS + `tauri-plugin-global-shortcut` in Sone | SMTC handles them | `MPRemoteCommandCenter` |
| Device enumeration | GStreamer `DeviceMonitor` filtered on `device.api == "alsa"`, reading `api.alsa.path` or `alsa.card`+`alsa.device` → `hw:C,D` | same monitor filtered on `device.api ∈ {wasapi, wasapi2}` reading `device.id` | GStreamer `gstosxaudiodeviceprovider` |
| Idle inhibit | Wayland `zwp_idle_inhibit`, X11 screensaver+DPMS, D-Bus ScreenSaver/GNOME/login1, XDG portal — **all additively** | — | — |

**MPRIS.** Sone uses `mpris-server` 0.9 on a dedicated thread with a current-thread tokio runtime and
a `LocalSet`, driven by an `MprisCommand` enum covering metadata, playback status, volume, seeked,
shuffle, loop status and fullscreen (`ref:sone/src-tauri/src/mpris.rs`). High Tide hand-writes the
D-Bus interface XML and implements `Raise, Quit, Next, Previous, PlayPause, Play, Pause, Stop, Seek,
SetPosition, Get, GetAll, Set, PropertiesChanged, Introspect`
(`ref:high-tide/src/mpris.py`). tidalt runs an MPRIS2 server *plus* a private `io.tidalt.App`
interface for client↔daemon communication (`ref:tidalt/docs/architecture.md`) — a good pattern for a
headless mode with a separate CLI/TUI.

Under Snap, the MPRIS name must be dotless; Sone declares a `mpris` slot and uses the name `sone`
under snap (`ref:sone/snap/snapcraft.yaml:71-76`).

**Windows/macOS.** sone-windows uses `souvlaki` 0.8.3 (`ref:sone-windows/src-tauri/Cargo.toml:64`).
souvlaki covers all three platforms behind one `MediaControls` API; on Linux it offers both a
`dbus-crossroads` backend (default, more stable) and a `zbus` backend (pure Rust). MSRV 1.67. Last
published 2025-06-24. **[verified from crates.io and docs summaries]**

**Idle inhibit.** Sone's `idle_inhibit` module is the most complete implementation in the set: it
detects the display server from `WAYLAND_DISPLAY`/`DISPLAY` (Wayland wins under Xwayland) and then
runs **every applicable layer additively** — Wayland `zwp_idle_inhibit`, X11 screensaver + DPMS,
D-Bus (`org.freedesktop.ScreenSaver`, GNOME, `login1`), and the XDG portal as a last resort
(`ref:sone/src-tauri/src/idle_inhibit/mod.rs`).

**Hot-plug.** No reference implements true device hot-plug re-binding. Sone's ALSA writer detects
`ENODEV` from `snd_pcm_writei` and emits `audio-error {kind: "device_disconnected"}`, tearing down
the pipeline (`ref:sone/src-tauri/src/audio.rs:900-925`). GStreamer's `DeviceMonitor` emits
added/removed bus messages and would be the right source for a live device list, but Sone only polls
it once with a 2 s timeout because *"GStreamer 1.28+ starts providers async, so devices() may
initially be empty"* (`ref:sone/src-tauri/src/audio.rs:3255-3268`). CamillaDSP's CoreAudio backend
listens for device rate-change notifications and reopens. **streamboat should subscribe to
`DeviceMonitor` bus messages rather than polling.** **[inferred]**

**Notifications.** Not implemented in the audio layer in any reference; MPRIS metadata is what
desktop shells surface.

---

### 7. What each reference actually does — file pointers

| Project | Language | Decode | Output | Gapless | Bit-perfect | Key files |
|---|---|---|---|---|---|---|
| **Sone** | Rust + Tauri 2 | GStreamer 0.23 bindings, `uridecodebin` | Normal: `autoaudiosink`. Exclusive: `appsink` → own `libasound` writer thread (`alsa` 0.10) | `concat` + prerolled second branch, **normal mode only** | yes, full | `ref:sone/src-tauri/src/audio.rs` (3309 lines), `ref:sone/src-tauri/src/signal_path.rs`, `ref:sone/src-tauri/src/pipeline_probe.rs`, `ref:sone/src-tauri/src/commands/playback.rs`, `ref:sone/src-tauri/src/cache.rs`, `ref:sone/src-tauri/src/mpris.rs`, `ref:sone/src-tauri/src/idle_inhibit/` |
| **sone-windows** | Rust + Tauri 2 | same | `wasapi2sink exclusive=… low-latency=true device=…` | inherited (concat path) | yes, via WASAPI exclusive | `ref:sone-windows/src-tauri/src/audio.rs:1236-1246` (sink), `:1580-1602` (live exclusive toggle), `ref:sone-windows/src-tauri/Cargo.toml` |
| **High Tide** | Python + GTK4/libadwaita | GStreamer `playbin3` (fallback `playbin`) | Selectable: `autoaudiosink`, `pulsesink`, `alsasink device=…`, `jackaudiosink`, `osssink`, `pipewiresink` | `about-to-finish`, **disabled on `pipewiresink`** | partial — ALSA sink only, no rate/format control, no Flatpak `/dev/snd` | `ref:high-tide/src/lib/player_object.py`, `ref:high-tide/src/mpris.py`, `ref:high-tide/build-aux/io.github.nokse22.high-tide.json` |
| **Strawberry** | C++17 + Qt6 | GStreamer, four TIDAL endpoint variants | `alsasink`/`pulsesink`/`pipewiresink`/`osxaudiosink`/`directsoundsink`(default on Win)/`wasapi(2)sink`/`asiosink` | `about-to-finish` + `SetNextUrl` | derived: `device` starting `hw:`/`plughw:` ⇒ `exclusive_mode_`; sets `exclusive` on any sink that has it | `ref:strawberry/src/engine/gstenginepipeline.cpp`, `ref:strawberry/src/engine/gstengine.cpp`, `ref:strawberry/src/engine/gststartup.cpp`, `ref:strawberry/src/tidal/tidalstreamurlrequest.cpp`, `ref:strawberry/src/engine/*devicefinder.cpp` |
| **tidalt** | Go + CGO | FFmpeg `libavformat/libavcodec/libswresample`, streaming AVIO callback | direct `libasound` `hw:` with `plughw:` fallback | keeps device open across tracks | yes, plus **PipeWire D-Bus reservation** | `ref:tidalt/internal/player/alsa.c`, `ref:tidalt/internal/player/mpv.go`, `ref:tidalt/internal/player/avcodec.go`, `ref:tidalt/docs/architecture.md` |
| **mopidy-tidal** | Python, headless | GStreamer via Mopidy | Mopidy's | Mopidy's | no | `ref:mopidy-tidal/mopidy_tidal/playback.py`, `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/{proxy,cache}.py` |
| **tidal-hifi** | TypeScript + Electron | Chromium + Widevine (castlabs `electron-releases#v43.0.0+wvcus`) | Chromium → Pulse/PipeWire/ALSA | Chromium's | **no** — Chromium resamples; mitigated with `--audio-output-sample-rate=192000` | `ref:tidal-hifi/src/constants/flags.ts`, `ref:tidal-hifi/src/features/flags/flags.ts`, `ref:tidal-hifi/package.json` |
| **tidal-sdk-web player** | TypeScript | Shaka Player (DASH), native HLS on Safari, or a proprietary native component | browser, or the native component's exclusive/shared device modes | dual Shaka instances + 250 ms micro-crossfade | only via the proprietary native component | `ref:tidal-sdk-web/packages/player/src/player/shakaPlayer.ts`, `browserPlayer.ts`, `basePlayer.ts`, `nativeInterface.ts`, `audio-context-store.ts`, `internal/helpers/{manifest-parser,playback-info-resolver,normalize-volume}.ts`, `config.ts` |
| **tidal-sdk-android player** | Kotlin | ExoPlayer / androidx.media3 1.5.0 | `DefaultAudioSink` with a fixed `DefaultAudioTrackBufferSizeProvider` | ExoPlayer playlist | n/a | `ref:tidal-sdk-android/player/playback-engine/src/main/kotlin/.../player/di/{RendererModule,ExtendedExoPlayerModule}.kt`, `.../volume/{LoudnessNormalizer,VolumeHelper}.kt`, `.../dash/DashManifestFactory.kt`, `.../model/BufferConfiguration.kt` |
| **tidal-sdk-ios player** | Swift | AVPlayer/AVQueuePlayer, HLS + FairPlay | AVAudioSession | AVQueuePlayer | n/a | `ref:tidal-sdk-ios/Sources/Player/Common/Data/AudioCodec.swift`, `.../PlaybackInfo/PlaybackInfoFetcher.swift`, `.../DRM/FairPlayLicenseFetcher.swift` |
| **python-tidal** | Python | none — library only | none | n/a | n/a | `ref:python-tidal/tidalapi/media.py` (`Stream`, `StreamManifest`, `DashInfo`) |
| **tidal-cli** | TypeScript/Node | none — downloads segments, shells out to `mpv`/`afplay` | external player | n/a | n/a | `ref:tidal-cli/src/playback.ts` |
| **TidaLuna** | TypeScript | mod inside the official Electron client | official client's | official client's | no | `ref:TidaLuna/plugins/lib.native/src/request/decrypt.ts` (encryptionType values only) |
| **tidal-connect** | Bash + Docker around a proprietary binary | proprietary | ALSA via PortAudio | proprietary | LOSSLESS only since July 2024 | `ref:tidal-connect/bin/entrypoint.sh` |

---

### 8. Candidate audio stacks for streamboat

Version and licence data below is from the crates.io API on 2026-09-07 and from upstream source
reads. **[verified]**

| Stack | Linux | Windows | macOS | Headless/Pi | Bit-perfect Linux | Bit-perfect Win | Bit-perfect macOS | 24/192 | Gapless | DASH | Licence | Maturity | Mobile path |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **gstreamer-rs 0.25.3** + own ALSA writer (the Sone design) | yes | yes | yes | yes | yes (own writer) | yes (`wasapi2sink exclusive`) | **no** (would need an own CoreAudio writer) | yes | yes (`concat`) | yes | bindings MIT/Apache-2.0; GStreamer LGPL-2.1+ | very high; two shipping TIDAL clients | GStreamer runs on Android/iOS but is heavy; would likely swap the sink |
| **libmpv (`libmpv2` 6.0.0)** | yes | yes | yes | yes | yes (`--ao=alsa --audio-exclusive` / device `hw:`) | yes (`--ao=wasapi --audio-exclusive`) | **yes** (`coreaudio_exclusive`) | yes | yes (`--gapless-audio=weak`) | yes (FFmpeg) | crate LGPL-2.1; mpv GPLv2+ (LGPLv2.1+ build available) | very high | mpv builds for Android; iOS is awkward |
| **Symphonia 0.6.1 + cpal 0.18.2 / rodio 0.22.2** | yes | yes | yes | yes | **no** via cpal (would need direct `alsa` 0.12.1) | **no** (cpal has no exclusive mode) | **no** | yes (decode side) | needs hand-rolling | **no** — needs `dash-mpd` 0.20.4 + own fetcher | MPL-2.0 / Apache-2.0 / MIT | Symphonia high, the assembled stack unproven | best story: pure Rust, Android via `oboe` 0.6.1, iOS via AVAudioEngine FFI |
| **Symphonia + direct backends** (`alsa` 0.12.1 / `wasapi` 0.24.0 / `coreaudio-rs` 0.14.2) | yes | yes | yes | yes | **yes** | **yes** | **yes** | yes | hand-rolled | **no** | all permissive | you write and maintain three backends | best |
| **FFmpeg (`ffmpeg-next` 9.0.0) + direct backends** (the tidalt design) | yes | yes | yes | yes | yes | yes | yes | yes | hand-rolled (keep device open) | yes | crate WTFPL; FFmpeg LGPL-2.1+ | tidalt ships it on Linux; Win/macOS unproven | fine |
| **GStreamer from C++ / Qt** (the Strawberry design) | yes | yes | yes | yes | yes | yes | **no** | yes | yes | yes | LGPL/GPL | very high | poor |
| **GStreamer from Python** (the High Tide design) | yes | awkward | awkward | yes | partial | untested | no | yes | yes | yes | LGPL/GPL | high on Linux only | poor |
| **Electron / MSE / Web Audio** (the tidal-hifi design) | yes | yes | yes | no | **no** | **no** | **no** | resampled | yes-ish | yes (Shaka) | Apache-2.0 + castlabs Electron | very high | good (it is a browser) |

Notes that decide this table:

- **cpal has no exclusive mode on any platform.** The upstream request (RustAudio/cpal#459, opened
  2020) is closed with no implementation, and the docs describe WASAPI shared-mode format
  enumeration as the design assumption. rodio and kira sit on cpal and inherit that. Any Rust
  bit-perfect client must go below cpal. **[verified]**
- **Symphonia has no HE-AAC.** The upstream matrix marks AAC-LC "great" and HE-AAC variants "in work
  or not started". TIDAL `LOW` is HE-AAC v1. A Symphonia-only client cannot play the `LOW` tier.
  **[verified]**
- **Symphonia has no DASH and no HTTP.** `dash-mpd` 0.20.4 (MIT) parses MPDs; `stream-download`
  0.24.4 handles buffered HTTP streaming (tidalrs already uses it,
  `ref:tidalrs/Cargo.toml` per the project survey). You would assemble the FLAC-in-fMP4 stream
  yourself from `SegmentTemplate` + `SegmentTimeline` and feed it to Symphonia's ISO/MP4 reader.
  That is very doable — python-tidal and tidal-cli both do exactly this in ~150 lines — but it is
  code you own. **[verified for the crates; the effort estimate is [inferred]]**
- **GStreamer cannot be bit-perfect on macOS.** See §3.5. Any GStreamer-based streamboat needs either
  (a) an appsink → own CoreAudio writer on macOS, exactly mirroring what Sone does for ALSA, or
  (b) accept shared-mode output on macOS and say so.
- **libmpv is the only off-the-shelf engine with exclusive output on all three desktops**, and it
  brings DASH, HLS, gapless, seeking, buffering and every codec for free. Its costs are: a C API with
  a property-string interface rather than a typed pipeline; GPLv2+ unless you build LGPL mode; less
  control over the exact PCM path (you get `--audio-format`/`--audio-samplerate`/`--alsa-resample=no`
  rather than direct `snd_pcm_hw_params`); and you cannot easily build the "signal path transparency"
  panel from inside it.

---

### 9. Recommended pipeline designs

Three sketches, in descending order of how well they fit the brief ("simple but beautiful", desktop +
headless now, mobile not precluded).

#### Design A — Rust core, GStreamer decode, per-platform output writer

**Fits:** gstreamer-rs. This is the Sone design generalised to three platforms.

```
                       ┌──────────────── control thread (tokio) ────────────────┐
                       │ session · manifest fetch · quality cascade · retry     │
                       │ replay-gain calc · queue · MPRIS/SMTC/NowPlaying       │
                       └──────────┬──────────────────────────┬──────────────────┘
                                  │ AudioCommand (mpsc)      │ events (broadcast)
                       ┌──────────▼──────────────────────────┴──────────────────┐
                       │  audio worker thread (owns the GStreamer pipeline)     │
                       └──────────┬──────────────────────────┬──────────────────┘
                                  │                          │ AttachJob (mpsc)
                                  │                 ┌────────▼─────────┐
                                  │                 │ attach executor  │  serializes concat
                                  │                 │ thread           │  pad requests
                                  │                 └──────────────────┘
   SHARED MODE                    │        EXCLUSIVE / BIT-PERFECT MODE
   uridecodebin(A) → queue ─┐     │        uridecodebin → audioconvert
   uridecodebin(B) → queue ─┴ concat        (dithering=none, noise-shaping=none)
     → audioconvert → audioresample           [→ capsfilter (BTS only)]
     → volume(norm) → volume(user)            → appsink (max-buffers=20, sync=false)
     → autoaudiosink | wasapi2sink | osxaudiosink                │ AudioChunk
                                                        ┌────────▼────────┐
                                                        │ writer thread   │
                                                        │ alsa | wasapi   │
                                                        │ | coreaudio     │
                                                        └─────────────────┘
```

- **Threads:** control (tokio multi-thread), audio worker (blocking, owns the pipeline), attach
  executor (serialises `concat` pad ops), writer (one per open device, real-time-ish priority),
  plus GStreamer's own streaming threads.
- **Channels:** `mpsc<AudioCommand>` with a reply `Sender<T>` per command for request/response;
  `crossbeam::bounded(256)` for `WriterCommand`; an `AtomicU32` holding `f32::to_bits(user_amp *
  norm_gain)` so the writer reads volume lock-free; an `AtomicU64` generation counter so stale PCM
  from a torn-down pipeline is dropped; `AtomicU64 frames_written` for position in exclusive mode.
- **Buffers:** `uridecodebin buffer-duration` 15 s (DASH) / 5 s (BTS); branch `queue max-size-time`
  15 s; device buffer ~500 ms with ~50 ms periods; `start_threshold` = full buffer.
- **State machine:** `Idle → Resolving → Prerolling → Playing ⇄ Paused → Draining → Idle`, with
  `Resolving` covering the quality cascade and `Prerolling` covering both the initial preroll and the
  gapless attach. Device-level states are orthogonal: `DeviceClosed → Reserving → Open(fmt) →
  Reconfiguring(fmt') → Open(fmt')`.
- **Gapless:** `concat` with two branches in shared mode. In exclusive mode, gapless is achievable if
  the two tracks share format+rate — hold the PCM device open, feed the next track's chunks straight
  through, and only close/reopen on a format change (this is what mpv calls `--gapless-audio=weak`,
  and it is what tidalt does). Sone gives up here; streamboat should not.
- **macOS:** write a CoreAudio writer mirroring the ALSA one — `kAudioDevicePropertyHogMode` to hog,
  `kAudioDevicePropertySupportsMixing = false`, `kAudioStreamPropertyPhysicalFormat` to set the rate
  and depth (with the confirm-loop GStreamer already demonstrates in
  `gstosxcoreaudiohal.c`), and release both on stop.
- **Headless:** the control thread is the whole product; expose it over a local socket / D-Bus
  (`org.mpris.MediaPlayer2` plus a private interface, as tidalt does) and let the TUI/CLI be a client.
- **Mobile later:** replace the writer with `oboe`/`AAudio` (Android) or `AVAudioEngine` (iOS), and
  either keep GStreamer (it builds for both) or swap the decode half for the platform decoder.

**Cost:** three output backends to write and maintain, plus GStreamer as a runtime dependency that
must be shipped on Windows and macOS.

#### Design B — Rust core, libmpv engine

**Fits:** `libmpv2` 6.0.0.

```
control thread (tokio)  ──property/command──▶  libmpv instance  ──▶  ao=alsa|wasapi|coreaudio
        ▲                                            │
        └──────────── mpv event loop thread ─────────┘
```

Configuration that makes it bit-perfect and gapless:

```
--ao=alsa               (Linux)   --audio-device=alsa/hw:1,0   --alsa-resample=no
--ao=wasapi             (Windows) --audio-exclusive=yes        --wasapi-exclusive-buffer=default
--ao=coreaudio          (macOS)   --audio-exclusive=yes        --coreaudio-change-physical-format=yes
--gapless-audio=weak    (never `yes` — `yes` locks the rate to the first track)
--audio-channels=auto-safe
--volume-max=100        + `replaygain=no`, apply gain yourself via `--volume` or `af=volume`
--cache=yes --demuxer-max-bytes=… --demuxer-readahead-secs=…
```

- **Queue/gapless:** use `loadfile <uri> append` to keep the next track queued; mpv keeps the device
  open across tracks when the format matches.
- **ReplayGain:** compute the linear gain yourself from the manifest and set mpv's `volume` property
  (mpv's own `replaygain` option reads tags from the file, which TIDAL's fMP4 does not carry).
- **Threads:** control (tokio) plus one thread pumping `mpv_wait_event`.
- **Pros:** one dependency covers decode, DASH, HLS, buffering, seek, gapless and exclusive output on
  all three desktops. Dramatically less code than A. Excellent headless fit (mpv is already a
  headless player). Trivially the fastest route to a working product.
- **Cons:** GPLv2+ unless LGPL mode is used; less visibility into the exact signal path, so the
  transparency panel becomes "what we asked for" rather than "what the kernel says" (though the
  `/proc/asound/*/hw_params` probe from Sone still works and gives ground truth); property-string API
  rather than typed pipeline; passing a `data:application/dash+xml;base64,…` URI to mpv is
  **[unverified]** — the fallback is to write the MPD to a temp file and pass `file://`, which is
  exactly what mopidy-tidal and modern High Tide do anyway.

#### Design C — pure Rust, no C media framework

**Fits:** Symphonia 0.6.1 + `dash-mpd` 0.20.4 + `stream-download` 0.24.4 + `alsa` 0.12.1 /
`wasapi` 0.24.0 / `coreaudio-rs` 0.14.2 + `rubato` 5.0.0 for the non-bit-perfect path.

```
manifest → MPD parse (dash-mpd) → segment fetcher (2 concurrent, reqwest)
        → fMP4 reassembly (init + $Number$ segments) → Symphonia ISO/MP4 reader
        → Symphonia FLAC / AAC-LC decoder → ring buffer → device writer
```

- **Pros:** no C build dependency on any platform, smallest binaries, best mobile story, full control
  of the signal path (so the transparency panel is trivially truthful), all-permissive licences.
- **Cons:** **no HE-AAC** — you cannot play the `LOW` tier; you own the DASH assembler, the fMP4
  reassembly, the seek logic, the buffering policy and three device backends; and you get no video
  path at all.
- **When it is right:** if streamboat decides to be a lossless-only client (LOSSLESS +
  HI_RES_LOSSLESS, no `LOW`/`HIGH`, no video), Design C becomes genuinely attractive and sidesteps
  the AAC patent question entirely.

#### Recommendation

Start with **Design B (libmpv)** to get a correct, gapless, bit-perfect player on all three desktops
and headless within a small amount of code, and structure the codebase so the engine is behind a
narrow trait — `load(uri, hints)`, `preload(uri, hints)`, `play/pause/seek/stop`,
`set_gain(linear)`, `position()`, `signal_path()`, plus an event stream. Then, if and when the signal
path or the macOS behaviour proves inadequate, add **Design A** as a second engine implementation
behind the same trait. Do not build Design C unless the owner decides streamboat is lossless-only.
**[inferred — this is a recommendation, not a finding]**

---

## Implications for streamboat

1. **Request `adaptive=false`.** ABR mid-track means a format change means a device reopen means a
   gap. Bit-perfect and ABR are incompatible.
2. **Prefer the v2 `/trackManifests/{id}` endpoint with a full `formats` array** over the v1
   quality-cascade loop: one request instead of up to four, and the server tells you what you got.
   Keep the v1 `playbackinfopostpaywall` path as a fallback, since every unofficial client uses it and
   it is the one that returns `bitDepth`/`sampleRate` directly.
3. **Refuse encrypted streams. Never implement `OLD_AES` decryption.** Detect
   `encryptionType != "NONE"`, a non-empty `encryptionKey`, or `securityType != "NONE"`, and show
   Strawberry's message: the stream is protected, and whether TIDAL sends protected streams depends on
   the client credentials in use.
4. **Implement `org.freedesktop.ReserveDevice1` on Linux.** Copy tidalt's three-outcome logic exactly,
   including the refusal to steal the name on a timeout. Without it, exclusive mode is a "device
   busy" error for every PipeWire user.
5. **Do not resample in bit-perfect mode; fail loudly and actionably instead.** Sone's message —
   "DAC doesn't support 192kHz — turn off bit-perfect mode for compatibility" — is the right shape.
   Achieve it by *not* constraining rate caps upstream, so the device open is what fails.
6. **Get the 24-bit format naming right.** ALSA `S24_LE` is 24-in-32 (4 bytes); ALSA `S24_3LE` is
   packed (3 bytes); GStreamer's `S24_32LE` and `S24LE` are the other way round. Write a unit test.
7. **Restore ALSA `sw_params` after `snd_pcm_hw_params()`.** It resets `start_threshold` to 1 and you
   will underrun from the first write.
8. **Set the ALSA period before the buffer.** Some USB DACs report absurd `period_size_min` when the
   buffer is set first.
9. **In bit-perfect mode, disable the volume slider and offer mute-only.** Any software attenuation
   quantises. Say so in the UI, as Sone does.
10. **Loudness normalization:** `min(10^((rg + 4)/20), 1/peak)`, modes `NONE|TRACK|ALBUM` defaulting
    to `ALBUM`, applied before playback starts so there is no volume step at track start. Do not add
    Sone's extra 0.8 factor unless the owner wants deliberately quieter-than-TIDAL output; TIDAL's own
    SDKs do not have it.
11. **Ship the signal-path transparency panel.** Backend, decoded format/rate/channels, negotiated
    device format/rate/channels, kernel `hw_params`, OS mixer state, and every alteration (resample,
    bit-depth promotion, format fallback, software volume, ReplayGain factor). Read it from
    `/proc/asound/<card>/pcm*p/sub*/hw_params` and `pactl` on Linux; the equivalent device-format
    query on Windows/macOS. This is the feature that makes an audiophile client trustworthy.
12. **Gapless in exclusive mode is achievable** by keeping the device open across same-format tracks.
    Sone gives up on this; streamboat should not, because it is the whole point of a hi-res client
    playing an album.
13. **Refetch the manifest before resuming a track that has been paused for close to an hour.**
    Manifests expire at 3600 s.
14. **Handle the 4xxx sub-statuses correctly:** 4005/4010/4030/4031/4032/4034/4035 are terminal — skip
    the track, do not retry, do not refresh the token. 4006 and 4033 recover.
15. **Subscribe to the privileges WebSocket** (`POST /v1/rt/connect`) so a second device taking the
    stream produces a clean "playback moved to <device>" message instead of a mysterious 401.
16. **Do not build Atmos or 360RA.** Surface the badge, play the stereo version, keep an `immersive`
    flag in the request layer for later.
17. **Treat exclusive mode as a packaging feature.** Flatpak cannot reach `/dev/snd` without
    `--device=all`; Snap needs the `alsa` plug connected manually. Detect confinement at runtime and
    grey out the exclusive-mode toggle with an explanation rather than failing at play time.
18. **Cap and expire any audio cache**, encrypt it at rest, and purge on logout — see §4.6.
19. **Use GStreamer's `DeviceMonitor` bus messages, not polling**, for the device list, and handle the
    async-provider behaviour of GStreamer 1.28+.
20. **Pin buffer numbers to TIDAL's own:** two minutes of media buffer and ~1.5 s of device buffer is
    what TIDAL's Android SDK ships; 40 s is what its web SDK ships; 15 s is what Sone ships. Pick a
    number, make it configurable, and default generously for the Pi/headless case.

---

## Open questions

**Owner decisions**

1. **Which engine?** libmpv (fast, GPLv2+, exclusive everywhere, less introspection) vs GStreamer +
   own writers (more code, better introspection, no macOS exclusive without writing a CoreAudio
   writer) vs pure Rust (no HE-AAC, most work, best mobile story).
2. **Lossless-only?** Dropping `LOW`/`HIGH` removes the AAC patent question entirely and makes the
   pure-Rust design viable. It also means some content is unplayable for users on cheaper plans.
3. **Is bit-perfect a headline feature or a power-user toggle?** The answer determines whether the
   macOS CoreAudio writer is v1 work or v2 work.
4. **Offline caching:** none, session-only, or a capped encrypted cache? This is the sharpest
   legal/ethical line in the whole project.
5. **Do we send TIDAL's play-reporting / streaming-metrics events?** Sending them is what a
   well-behaved client does and probably what artists' royalties depend on; not sending them is less
   code and less data leaving the machine.
6. **Crossfade:** implement it (TIDAL offers 0–15000 ms) or ship gapless only? Note it is mutually
   exclusive with exclusive-device mode.
7. **Video:** in scope at all? If yes, it is a separate HLS path and never bit-perfect.
8. **Client credentials:** which client ID/secret pair, and therefore which tiers are reachable and
   whether streams come back encrypted. This is an auth-topic decision with a direct audio-pipeline
   consequence.

**Unverified — check before relying on**

- Whether libmpv accepts a `data:application/dash+xml;base64,…` URI, or whether the MPD must be
  written to a file.
- Why High Tide switched from a data URI to `file://` for MPDs at GStreamer 1.26, and whether the
  data-URI form still works on 1.28.
- Why High Tide disables gapless when the sink is `pipewiresink`.
- The exact upstream commit behind "support for FLAC audio in DASH manifests" in GStreamer 1.26.10,
  and whether it changes behaviour for `dashdemux` (legacy) as well as `dashdemux2`.
- Whether FFmpeg's `eac3` decoder discards JOC metadata (asserted here as [inferred]).
- Whether the TIDAL Windows/macOS desktop apps really do not support Dolby Atmos —
  `support.tidal.com` and `tidal.com` are blocked from this environment, so this rests on
  second-hand reporting.
- The exact per-tier bit rates TIDAL publishes today (same blocked-domain problem).
- Whether `SegmentTemplate@startNumber` is present in TIDAL MPDs and what value it takes; python-tidal
  and tidal-cli disagree about where segment numbering starts.
- Whether `assetpresentation=FULL` vs `PREVIEW` and `previewReason` need special handling for
  free-tier or region-limited accounts.
- Current PipeWire config keys for pinning rate/format (`default.clock.allowed-rates`,
  per-device `audio.format`) — read from docs.pipewire.org before writing them into user docs.
- Whether GStreamer's `asiosink` is shipped in any mainstream Windows GStreamer build, and what the
  Steinberg ASIO SDK licence permits for redistribution.
- What Chromium's actual output resampling behaviour is in 2026 and whether
  `--audio-output-sample-rate` still works — relevant only if an Electron path is ever considered.
- Whether `souvlaki` 0.8.3 (last published June 2025) is still maintained, and whether its macOS
  backend works outside an app bundle.

---

## Sources

### Reference checkouts

| Path | Supports |
|---|---|
| `ref:sone/src-tauri/src/audio.rs` | ALSA `hw_params`/`sw_params` negotiation, format probing and the 24-bit naming inversion, rate probing list, bit-perfect format promotion ladder, XRUN/suspend/ENODEV recovery, hardware vs software pause, per-format PCM volume scaling, `concat`-based gapless with a serialised attach executor, buffer/queue numbers, appsink caps policy, seek behaviour, `DeviceMonitor` enumeration, `gapless_supported()` |
| `ref:sone/src-tauri/src/commands/playback.rs` | `compute_norm_gain` (0.8 × TIDAL formula), quality-tier cascade and its early-exit rules, DASH data-URI construction, album/track gain selection |
| `ref:sone/src-tauri/src/tidal_api.rs` | `playbackinfopostpaywall` call shape and parameters, BTS/DASH/JSON manifest branches, terminal vs recoverable sub-status lists, 401-with-sub-status handling |
| `ref:sone/src-tauri/src/rate_gate.rs` | 429 cooldown gate, `Retry-After` parsing rules and clamps |
| `ref:sone/src-tauri/src/signal_path.rs`, `ref:sone/src-tauri/src/pipeline_probe.rs` | signal-path transparency model; `/proc/asound/*/hw_params` and `pactl` probing |
| `ref:sone/src-tauri/src/cache.rs` | encrypted tiered metadata cache, TTLs, SWR grace, 2 GB cap / 1.8 GB evict target |
| `ref:sone/src-tauri/src/mpris.rs`, `ref:sone/src-tauri/src/idle_inhibit/` | MPRIS command model; additive idle-inhibition across Wayland/X11/D-Bus/portal |
| `ref:sone/README.md`, `ref:sone/snap/snapcraft.yaml` | exclusive vs bit-perfect definitions, GStreamer plugin dependencies, Snap `alsa` plug requirement, the stale "GStreamer 1.24+" gapless claim |
| `ref:sone-windows/src-tauri/src/audio.rs`, `ref:sone-windows/src-tauri/Cargo.toml` | `wasapi2sink exclusive`/`low-latency`/`device`, live exclusive toggle via Ready→Playing→seek, WASAPI device enumeration, souvlaki 0.8.3 |
| `ref:high-tide/src/lib/player_object.py` | `playbin3`+`about-to-finish` gapless and its `pipewiresink` exception, sink map, `taginject`/`rgvolume`/`rglimiter` chain and pre-amp, MPD file:// vs data: split at GStreamer 1.26, whole-track caching with metered-network check, quadratic volume |
| `ref:high-tide/src/mpris.py` | hand-written MPRIS interface surface |
| `ref:high-tide/build-aux/io.github.nokse22.high-tide.json` | Flatpak `finish-args` — pulseaudio socket + pipewire ro, no `/dev/snd` |
| `ref:strawberry/src/engine/gstenginepipeline.cpp` | generic `exclusive` property handling, `hw:`/`plughw:` ⇒ exclusive inference, audiobin element order |
| `ref:strawberry/src/engine/gstengine.cpp` | crossfade gated off when any exclusive pipeline is active |
| `ref:strawberry/src/engine/gststartup.cpp` | `directsoundsink` promoted over `wasapisink`/`wasapi2sink` on Windows and why |
| `ref:strawberry/src/tidal/tidalstreamurlrequest.cpp` | four v1 endpoint variants; encryption rejection on `encryptionType`, `encryptionKey`, `securityType`/`securityToken`; DASH data-URI construction |
| `ref:strawberry/src/engine/*devicefinder.cpp` | per-platform device enumeration including `asiosink` |
| `ref:tidalt/internal/player/alsa.c` | split open/configure, per-bit-depth format preference orders and the DAC-quirk rationale, period-before-buffer ordering, `snd_pcm_hw_params_get_sbits` |
| `ref:tidalt/internal/player/mpv.go` | `org.freedesktop.ReserveDevice1` handshake with three distinct outcomes and its timing budget |
| `ref:tidalt/docs/architecture.md` | end-to-end bit-perfect flow, `plughw:` fallback semantics, quality ladder, MPRIS + private D-Bus interface |
| `ref:mopidy-tidal/mopidy_tidal/playback.py` | MPD written to `manifest.mpd` and served as `file://`; BTS direct URL |
| `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/{proxy,cache}.py` | localhost relay proxy with SQLite chunked-insert cache, finalisation semantics, TidalID→Path offline mapping |
| `ref:tidal-hifi/src/constants/flags.ts`, `ref:tidal-hifi/src/features/flags/flags.ts`, `ref:tidal-hifi/package.json` | `audio-output-sample-rate=192000` + `AudioServiceOutOfProcess`/`AudioServiceSandbox` disabling; castlabs `electron-releases#v43.0.0+wvcus` |
| `ref:tidal-sdk-web/packages/player/src/internal/constants.ts` | the four manifest MIME types |
| `ref:tidal-sdk-web/packages/player/src/internal/helpers/manifest-parser.ts` | BTS/EMU/DASH/HLS parsing, codec extraction, bit depth from `Representation@id`, HLS `X-COM-TIDAL-*` tags |
| `ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts` | v1 and v2 endpoints and every parameter, `MANIFEST_EXPIRATION_MS = 3600000`, retry policy, sub-status→error mapping, `audioQualityToFormats`/`audioFormatsToQuality`, `audioMode: 'STEREO'` hardcode |
| `ref:tidal-sdk-web/packages/player/src/internal/helpers/normalize-volume.ts`, `.../player/basePlayer.ts` | TIDAL's normalization formula and how it is applied |
| `ref:tidal-sdk-web/packages/player/src/config.ts` | `crossfadeInMs` 0–15000 default 0, `loudnessNormalizationMode` default ALBUM, `audioAdaptiveBitrateStreaming` default true, api URLs |
| `ref:tidal-sdk-web/packages/player/src/player/shakaPlayer.ts` | 250 ms micro-crossfade "gapless", dual Shaka instances, Widevine/FairPlay license URLs, Shaka buffering and retry config |
| `ref:tidal-sdk-web/packages/player/src/player/browserPlayer.ts`, `audio-context-store.ts` | dual `<video>` element preload/transition |
| `ref:tidal-sdk-web/packages/player/src/player/nativeInterface.ts` | native player component API: exclusive/shared device modes and the device error event vocabulary |
| `ref:tidal-sdk-web/packages/player/src/player/adaptations.ts` | ABR adaptation reporting |
| `ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts`, `.../api/event/streaming-privileges-revoked.ts` | `POST /v1/rt/connect` → WebSocket URL, `PRIVILEGED_SESSION_NOTIFICATION`, `USER_ACTION`, `RECONNECT` |
| `ref:tidal-sdk-android/player/streaming-api/.../PlaybackInfoRepositoryDefault.kt` | v2 `trackManifestsIdGet` parameters, additive format list, `EAC3_JOC` ⇒ `DOLBY_ATMOS` |
| `ref:tidal-sdk-android/player/playback-engine/.../volume/{LoudnessNormalizer,VolumeHelper}.kt` | canonical normalization formula, preAmp 4 / TV 0, `NONE|TRACK|ALBUM` default ALBUM |
| `ref:tidal-sdk-android/player/playback-engine/.../model/BufferConfiguration.kt` | 20 s back buffer, 2 min min/max playback buffer, 2.5 s / 5 s start thresholds, 1.5 s AudioTrack buffer |
| `ref:tidal-sdk-android/player/playback-engine/.../player/di/{RendererModule,ExtendedExoPlayerModule}.kt` | `DefaultAudioSink` with fixed PCM buffer duration, empty AudioProcessor array, `DefaultLoadControl` |
| `ref:tidal-sdk-android/tidalapi/.../TrackManifestsAttributes.kt` | format enum `HEAACV1, AACLC, FLAC, FLAC_HIRES, EAC3_JOC` |
| `ref:tidal-sdk-android/gradle/libs.versions.toml` | androidx-media3 1.5.0 |
| `ref:tidal-sdk-ios/Sources/Player/Common/Data/AudioCodec.swift` | manifest codec strings, tier→codec map, 360RA declared unsupported |
| `ref:python-tidal/tidalapi/media.py` | `Quality`/`AudioMode`/`MediaMetadataTags`/`ManifestMimeType`/`Codec`/`MimeType` enums, `DashInfo` MPD field extraction and segment-URL generation, MPD→HLS synthesis, `encryption_type` defaulting |
| `ref:tidal-cli/src/playback.ts` | v2 `/trackManifests/{id}` with `adaptive:false`, quality→formats map, MPD regex parsing, init+`$Number$` segment download starting at 1 |
| `ref:TidaLuna/plugins/lib.native/src/request/decrypt.ts` | `encryptionType` values `NONE` and `OLD_AES` (cited for the value vocabulary only) |
| `ref:tidal-connect/bin/entrypoint.sh` | ALSA device selection by card name, softvol, LOSSLESS-only post-July-2024 |

### Upstream source read directly

| URL | Supports |
|---|---|
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-bad/sys/wasapi2/gstwasapi2sink.cpp` | `wasapi2sink` has a real `exclusive` property (`PROP_EXCLUSIVE`), plus `low-latency`, `device`, `mute`, `volume`, `continue-on-error` |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/sys/osxaudio/gstosxaudiosink.c` | `osxaudiosink` exposes only `device`, `unique-id`, `configure-session`, `volume` — no exclusive/hog property |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/sys/osxaudio/gstosxcoreaudiohal.c` | hog mode (`kAudioDevicePropertyHogMode`), `SupportsMixing=false` and physical-format setting exist but are called only from `_open_spdif` |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-base/ext/alsa/gstalsasink.c` | `alsasink` has only `device`, `device-name`, `card-name` — no exclusive or resample control |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/gst/isomp4/qtdemux.c` and `.../fourcc.h` | `FOURCC_fLaC` handling; FLAC-in-MP4 produces `audio/x-flac` caps |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/ext/adaptivedemux2/dash/gstmpdhelper.c` | DASH `audio/mp4` maps to `audio/x-m4a`; no FLAC-specific mapping in the mime helper |
| `https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavformat/isom_tags.c` and `.../mov.c` | `fLaC` → `AV_CODEC_ID_FLAC`; `dfLa` FlacSpecificBox parsing (`mov_read_dfla`) |
| `https://raw.githubusercontent.com/mpv-player/mpv/master/DOCS/man/options.rst` | `--audio-exclusive` works for wasapi/coreaudio/pipewire/audiounit; `--gapless-audio=<no\|yes\|weak>` semantics; `--audio-samplerate`/`--audio-format`/`--audio-channels` |
| `https://raw.githubusercontent.com/mpv-player/mpv/master/DOCS/man/ao.rst` | `coreaudio_exclusive` AO, `--coreaudio-change-physical-format`, `--wasapi-exclusive-buffer`, `--alsa-resample`, `--alsa-buffer-time`, `--alsa-periods`, `--audio-channels=auto-safe` warning |
| `https://crates.io/api/v1/crates/{gstreamer,rodio,kira,symphonia,cpal,souvlaki,libmpv2,mpris-server,alsa,wasapi,coreaudio-rs,dash-mpd,stream-download,oboe,ffmpeg-next,rubato,symphonia-bundle-flac,symphonia-codec-aac}` | all crate versions, licences and last-update dates quoted in §8 (queried 2026-09-07) |
| `https://github.com/pdeljanov/Symphonia` | codec support matrix (FLAC excellent, AAC-LC great, HE-AAC not started), MPL-2.0, MSRV 1.85, gapless support notes |
| `https://github.com/RustAudio/cpal/issues/459` | WASAPI exclusive mode requested 2020-07-27, issue closed with no implementation |
| `https://github.com/HEnquist/camilladsp/blob/master/backend_coreaudio.md` | CoreAudio `exclusive` (hog) flag, rate-change notification handling, S16/S24/S32/F32 physical formats, BlackHole caveat |

### Web sources

| URL | Supports |
|---|---|
| `https://www.via-la.com/licensing-programs/aac/` | AAC pool is an active per-unit licensing programme |
| `https://ipfray.com/access-advance-via-la-position-multimedia-patent-pools-for-further-growth-with-price-stability-offer-regionalization-for-more-standards/` | Via LA AAC pool rates and tiering, 2026 state |
| `https://www.digitaltrends.com/home-theater/tidal-killing-mqa-sony-360-reality-audio/` and `https://www.whathifi.com/news/tidal-scraps-mqa-and-spatial-audio-format-heres-what-that-means-for-subscribers` | MQA catalogue replaced with FLAC and all 360RA content removed on 24 July 2024; Dolby Atmos chosen as the surviving immersive format |
| `https://www.phoronix.com/news/GStreamer-1.28` and `https://lists.freedesktop.org/archives/gstreamer-devel/2026-January/082207.html` | GStreamer 1.28.0 released 27 January 2026; wasapi2 ported to IMMDevice-based device selection; dashdemux2 seeking-with-gaps fix |
| `https://9to5linux.com/gstreamer-1-26-10-released-with-support-for-flac-audio-in-dash-manifests` and `https://linuxiac.com/gstreamer-1-26-10-brings-fixes-for-flac-opus-and-matroska-handling/` | GStreamer 1.26.10 (December 2025) added FLAC-in-DASH-manifest support, FLAC 6.1/7.1 layouts and 32-bit FLAC, adaptivedemux2 stream-selection fixes |
| `https://issues.chromium.org/issues/40944208` and `https://strongrandom.com/post/high-bit-rate/` | Chromium/Web Audio always resamples to the output device rate; bit-perfect is not achievable in a browser context |
| `https://lib.rs/crates/souvlaki` | souvlaki covers MPRIS, SMTC and MPNowPlayingInfoCenter behind one API; dbus-crossroads default and zbus alternative on Linux; MSRV 1.67 |
| `https://docs.pipewire.org/page_audio.html` | audio adapter passthrough mode as the mechanism for exclusive access; `default.clock.rate` / `default.clock.allowed-rates` |
| `https://support.tidal.com/hc/en-us/articles/25876825185425-Audio-Format-Updates` and `https://support.tidal.com/hc/en-us/articles/360004255778-Dolby-Atmos` | TIDAL's own audio-format-update and Atmos pages — **referenced but not read; `support.tidal.com` is blocked from this environment** |
