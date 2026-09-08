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
- GStreamer can do exclusive output on Linux (`alsasink device=hw:X,Y`, no property needed — plain
  device selection) and Windows (`wasapi2sink exclusive=true`, a real GObject property, but **only on
  GStreamer >= 1.28** — the property's gtk-doc block is tagged `Since: 1.28` and does not exist on
  1.26 or earlier), but **not** on macOS: `osxaudiosink` exposes only `device`, `unique-id`,
  `configure-session` and `volume`, and GStreamer's CoreAudio HAL sets hog mode only in the
  SPDIF/passthrough path (`_open_spdif`). **[verified]**
- libmpv does support exclusive output on all three desktops, but the mechanism differs by AO:
  `--audio-exclusive=yes` works for `wasapi`, `coreaudio`, `pipewire` and `audiounit` — **it does
  nothing on `alsa`**, which mpv's own docs say silently ignores the option. Exclusivity on the
  `alsa` AO comes only from opening a `hw:`/`plughw:` device directly
  (`--audio-device=alsa/hw:X,Y`). mpv also ships a dedicated `coreaudio_exclusive` AO for macOS.
  **[verified]**
- The reference Linux client with the strongest bit-perfect story (Sone) does not use a GStreamer
  audio sink at all in exclusive mode: it decodes with GStreamer into an `appsink` and writes PCM
  from its own thread straight to `libasound`, negotiating `snd_pcm_hw_params` itself.
  **[verified]** — `ref:sone/src-tauri/src/audio.rs`
- Handing over the ALSA device from PipeWire/WirePlumber needs the D-Bus
  `org.freedesktop.ReserveDevice1.Audio{N}` protocol (`RequestRelease`, then `RequestName` with
  `ReplaceExisting`). tidalt implements this; Sone does not and instead surfaces a "device busy"
  error. **[verified]**
- Chromium (and therefore Electron and any browser-based client) is reported to resample audio
  output; tidal-hifi works around it, as a user-facing opt-in setting (not a default), with
  `--audio-output-sample-rate=192000` plus
  `--disable-features=AudioServiceOutOfProcess,AudioServiceSandbox`. **[verified for the tidal-hifi
  flags and that both `--disable-features` values are merged into one switch, from
  `ref:tidal-hifi/src/constants/flags.ts` and `ref:tidal-hifi/src/features/flags/flags.ts`. The
  Chromium-side claim is `[uncertain]`: the only citation found, issues.chromium.org/issues/40944208,
  is titled "WebAudio always resampling to the output device sample rate" — a statement about the
  WebAudio/AudioContext API specifically, not a blanket claim about every Chromium audio output path
  (MSE, `<audio>`). issues.chromium.org is blocked from this environment so the issue body could not
  be read, and whether `--audio-output-sample-rate` still has any effect in 2026 is unconfirmed.]**
- Dolby Atmos on TIDAL is E-AC-3 with JOC (`EAC3_JOC` format token, `audioMode = DOLBY_ATMOS`). The
  official TIDAL desktop apps do not play Atmos. Sony 360 Reality Audio (`SONY_360RA`, codec `mha1`)
  was removed from TIDAL on 24 July 2024 and is unplayable. **[verified for the format tokens;
  the "desktop apps do not support Atmos" claim rests on TIDAL support-page reporting, not a
  first-party page read — see Open questions]**
- GStreamer 1.26.10 (December 2025) added "support for FLAC audio in DASH manifests"; before that,
  the newer `adaptivedemux2`/`dashdemux2` path did not handle TIDAL's FLAC-in-DASH and only the
  legacy `dashdemux` from gst-plugins-bad did. **[verified from release reporting; the exact code
  change is unverified]**
- GStreamer 1.28.0 was released 27 January 2026; the series has since shipped through **1.28.6**
  (5 August 2026), announced as the final 1.28 bug-fix release and adding FFmpeg 9.0 support (relevant
  because §8 recommends `ffmpeg-next` 9.0.0 for Design C/B's optional FFmpeg use). 1.28.3 fixed
  `devicemonitor` to wait for its start thread before listing devices, see §6 "Hot-plug" and §11.1.
  The Rust bindings crate `gstreamer` is at 0.25.3 (MIT OR Apache-2.0, MSRV 1.92). **[verified — see
  §11.1 for the correction; this line previously under-stated the series as "1.28.2/1.28.3 as of this
  check"]**
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
- **This report and `docs/research/tech-stack.md` currently recommend opposite primary audio
  engines** (libmpv-first here, GStreamer-first there) and disagree on mpv's own gapless flag. Neither
  document currently states this conflict or resolves it — see the new owner decision in Open
  questions and §12.5. Also newly surfaced: linking libmpv makes streamboat a GPL work, which is
  incompatible with iOS App Store distribution — a direct conflict with the "mobile must not be
  precluded" constraint that neither document's engine recommendation currently weighs. **[verified —
  quoted directly from both documents; see §12.4/§12.5]**

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
`tracks/{id}/streamUrl?soundQuality=`, `tracks/{id}/urlpostpaywall?…`,
`tracks/{id}/playbackinfopostpaywall`, `tracks/{id}/playbackinfo`.

`urlpostpaywall` is the simplest of the four — it returns `{urls: [...]}` with a direct CDN URL and
no manifest at all. **Its parameter set is not one fixed shape** — the two reference clients that
call it disagree, and both disagree with what was previously written here. Strawberry's request
carries **four** params: `audioquality`, `playbackmode=STREAM`, `assetpresentation=FULL`, and
`urlusagemode=STREAM` (`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:126-131`). python-tidal's
own `urlpostpaywall` call sends a different set again — `urlusagemode=STREAM`, `audioquality`,
`assetpresentation=FULL`, with **no** `playbackmode` param
(`ref:python-tidal/tidalapi/media.py:422-429`) — and python-tidal refuses to call this endpoint at
all under a PKCE session (`if self.session.is_pkce: raise URLNotAvailable`), a constraint the report
did not previously mention anywhere. Since streamboat will likely copy one of these two shapes,
record both rather than inventing a third that matches neither. **[verified — corrects a
previously-stated shape that matched neither reference client]**

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
`shaka.drm.FairPlay.isFairPlaySupported()` is true (Safari/Apple) and MPEG-DASH otherwise
(`ref:tidal-sdk-web/.../playback-info-resolver.ts:358,373`) — both halves independently confirmed.
Android always asks for `MPEG_DASH` unconditionally
(`ref:tidal-sdk-android/.../PlaybackInfoRepositoryDefault.kt:58`) — confirmed. iOS asks for `.hls`
(`ref:tidal-sdk-ios/Sources/Player/Common/PlaybackInfo/PlaybackInfoFetcher.swift:87`,
`manifestType: .hls`). **[verified — re-opened and confirmed during the third fact-check pass]**

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

From `ref:python-tidal/tidalapi/media.py:744-860` (`DashInfo`), the MPD TIDAL emits with
`adaptive=false` (or via the v1 API, which never sets `adaptive` at all) has exactly one
`Period` → one `AdaptationSet` → one `Representation`, and the `Representation` has one
`SegmentTemplate` with one `SegmentTimeline`. **This single-Representation shape holds only for the
non-adaptive case** — with `adaptive=true` on the v2 API the same MPD carries multiple
`Representation`s and the player's ABR switches between them mid-stream; see §4.9. Fields read
(python-tidal indexes `periods[0].adaptation_sets[0].representations[0]` and does not read
`Representation@id` at all — that field is evidenced separately below):

- `MPD@mediaPresentationDuration` (ISO-8601 duration, e.g. `PT2M26.47S`)
- `AdaptationSet@contentType`, `AdaptationSet@mimeType` (`audio/mp4`) — **caveat:** python-tidal reads
  this attribute but never asserts its literal value, and no reference project ships a captured TIDAL
  MPD, so `"audio/mp4"` is inferred from FLAC-in-fMP4 convention, not observed. **[unverified — see
  §11.20]**
- `Representation@codecs` (`flac`, `mp4a.40.2`, `mp4a.40.5`)
- `Representation@audioSamplingRate`
- `Representation@id` — carries `"FLAC,44100,16"`, i.e. codec, rate, bit depth. This is evidenced
  only by the web SDK's manifest parser, not by python-tidal: the web SDK takes the last integer as
  bit depth and returns `undefined` when the id has no comma (`HEAACV1`, `AACLC`)
  (`ref:tidal-sdk-web/.../manifest-parser.ts:107-121`). **[verified]**
- `SegmentTemplate@initialization`, `SegmentTemplate@media` (a `$Number$` template),
  `SegmentTemplate@timescale`
- `SegmentTimeline/S@d` (segment duration in timescale units) and `@r` (repeat count)

Segment count and URL generation differ between the two open implementations, and this is a real
ambiguity — with a second, independent bug in it that the original pass did not flag:

- python-tidal: `segments_count = 1 + 1 + Σ(S.r or 1)`, then
  `[media.replace("$Number$", str(i)) for i in range(segments_count)]` — starts at **0** and folds
  the init segment into the same numbering (`ref:python-tidal/tidalapi/media.py:828-875`, specifically
  `:836-845` for the accumulation loop).
- tidal-cli: downloads `initialization` separately, then numbers media segments from **1**
  (`ref:tidal-cli/src/playback.ts:120-170`, specifically `:124-133`).

Neither reads `SegmentTemplate@startNumber` (default **1** per the DASH spec). Any hand-rolled DASH
assembler for streamboat must read it rather than guessing. **[inferred]**

**A third, more serious issue: python-tidal's `urls` list likely never contains a usable init
segment.** `DashInfo` parses `SegmentTemplate@initialization` into `DashInfo.first_url`
(`ref:python-tidal/tidalapi/media.py:786-796`), but `get_urls()` never emits it — it substitutes
`$Number$` only into the **media** template, for `range(segments_count)`
(`ref:python-tidal/tidalapi/media.py:833-875`). So the URL list this method returns has no init
segment at all, unless TIDAL's media template evaluated at `$Number$=0` happens to be byte-identical
to the separately-parsed `initialization` URL — which no captured fixture in this reference set
proves either way. Combined with the `1 + 1` prefix in `segments_count = 1 + 1 + Σ(...)` (two segments
counted up front, for what is really "one init URL, handled separately, plus N media URLs"), the
practical effect is that a stream assembled purely from `get_urls()`'s output is missing its `moov`/
`dfLa` box and is undecodable unless segment 0 happens to serve as the init segment, and additionally
generates one URL past the end of a single-run `<S d="…" r="N"/>` timeline (correct media count for
one run is `N + 1`; the `1 + 1` prefix makes it `N + 2`) — i.e. a trailing 404 on a well-formed
timeline. This is a stronger reason than "the numbering disagrees with tidal-cli" for the rule already
stated below: **do not copy python-tidal's segment-URL generation as-is.** **[uncertain — read
directly from `media.py`'s source, but not exercised against a real TIDAL manifest in this pass; the
byte-identity possibility for segment 0 cannot be ruled out without a captured fixture, see §11.21]**

**Second bug: `@r` itself is mis-accumulated by python-tidal.** Per the DASH spec, `SegmentTimeline/S@r`
is the number of *additional* consecutive repeats of that `S` element — so one `S` element with a
given `@r` contributes `r + 1` segments to the timeline, and `@r` defaults to 0. python-tidal's loop
does `segments_count += s.r if s.r else 1`, i.e. it adds `r` (not `r + 1`) when `r` is truthy — this
under-counts by exactly one segment per `S` run that carries a nonzero `@r`. tidal-cli gets this right:
`const repeat = m[2] ? parseInt(m[2]) + 1 : 1;` (`ref:tidal-cli/src/playback.ts:126-132`). python-tidal's
`segments_count = 1 + 1 + …` fudge (adding 2 up front) happens to land close to correct on a typical
single-run TIDAL MPD only by accident, and is not a safe model to copy. **State the spec rule
explicitly for any streamboat DASH assembler: total segments in one `<S d="…" r="N"/>` run =
`N + 1`; `@r` defaults to 0; `SegmentTemplate@startNumber` defaults to 1.** Do not copy either
reference implementation as-is; implement the spec. **[verified against the DASH `SegmentTimeline`
semantics and both reference implementations' source]**

python-tidal also synthesises an HLS playlist from the parsed MPD (`DashInfo.get_hls`), emitting
`#EXTINF` values of `S.d / timescale` and a separate value for the last segment. That is a useful
trick for handing a TIDAL DASH track to any HLS-capable player. **[verified]**

**Container.** `AdaptationSet@mimeType = "audio/mp4"` with `codecs="flac"` means FLAC inside
fragmented MP4 (the `fLaC` sample entry plus a `dfLa` FlacSpecificBox). Both major demuxers handle
it: FFmpeg maps `MKTAG('f','L','a','C')` → `AV_CODEC_ID_FLAC` in `libavformat/isom_tags.c` and reads
`dfLa` in `mov_read_dfla` (`libavformat/mov.c`); GStreamer's `qtdemux` has `FOURCC_fLaC` and emits
`audio/x-flac` caps (`gst-plugins-good/gst/isomp4/qtdemux.c`). **[verified]**

**What was missing until recently.** GStreamer 1.26.10 (released around 24–26 December 2025) is
reported as adding "support for FLAC audio in DASH manifests" (plus FLAC 6.1/7.1 channel layouts and
32-bit FLAC encode/decode). Before that, TIDAL FLAC-in-DASH worked through the *legacy* `dashdemux`
from gst-plugins-bad but not through `adaptivedemux2` / `dashdemux2`. Sone's code comment
(`ref:sone/src-tauri/src/audio.rs:1790-1795`, `:3302-3305`) gives its reason for staying on legacy
`uridecodebin` (not `uridecodebin3`) as: *"`concat` does the gapless switching, so we no longer need
uridecodebin3 / about-to-finish. Legacy uridecodebin handles Tidal `data:application/dash+xml` URIs
and works on GStreamer < 1.24."* Read that precisely: the **primary** reason to drop `uridecodebin3`
is that `concat`-based gapless (§4.1(b)) makes `about-to-finish` unnecessary; getting the legacy
`dashdemux` (and therefore data-URI/pre-1.26.10 compatibility) is a secondary, supporting property of
that choice, not the stated motive. **[verified for the Sone comment and the release reporting; the
exact upstream commit behind 1.26.10's FLAC-in-DASH change is [unverified] — gstreamer.freedesktop.org
is blocked from this environment]**

**Packaging consequence.** This is not a temporary workaround to remove later — it is a live
constraint on current mainstream Linux distributions. Debian 13 "trixie" (the base for current
Raspberry Pi OS, and the target for the brief's headless/Pi platform) ships
`gstreamer1.0-plugins-bad` 1.26.2-3 and `gstreamer1.0` 1.26.2-2
(https://packages.debian.org/trixie/gstreamer1.0-plugins-bad,
https://packages.debian.org/source/trixie/gstreamer1.0) — eight point releases below the 1.26.10
that added FLAC-in-DASH. **A GStreamer-based streamboat must route TIDAL's DASH manifests through the
legacy `dashdemux` (never `dashdemux2`) until its minimum supported GStreamer is >= 1.26.10**, which
on Debian stable will not be true for the foreseeable future. This is also a caveat on High Tide's
`playbin3` gapless design (§4.1(a)), which depends on the newer path being available.

**Correction: choosing `uridecodebin` over `uridecodebin3` does not, by itself, select the legacy
demuxer.** The original wording implied it did; it does not. Both `dashdemux` (legacy,
gst-plugins-bad) and `dashdemux2` (gst-plugins-good's `adaptivedemux2`) register as handlers for
`application/dash+xml`, and autoplugging picks by rank: legacy `dashdemux` registers at
`GST_RANK_PRIMARY` while `dashdemux2` registers at `GST_RANK_PRIMARY + 1` — one rank higher. Whenever
gst-plugins-good's `adaptivedemux2` is installed (the normal case on any mainstream distro), plain
`uridecodebin` autoplugs `dashdemux2`, not the legacy element, regardless of whether the *source*
element is `uridecodebin` or `uridecodebin3`. Sone's own comment (cited above) is about `uridecodebin`
handling `data:` URIs as a *source* — a different autoplug decision from which demuxer gets picked.
**To actually force the legacy DASH path, demote `dashdemux2`'s rank at startup**
(`gst_plugin_feature_set_rank(dashdemux2_factory, GST_RANK_NONE)`, or raise `dashdemux` above it) —
the same mechanism Strawberry already uses for its Windows sink ranking
(`ref:strawberry/src/engine/gststartup.cpp:57-77`) — or hook `decodebin::autoplug-select`. Without
this, a Debian-13/Pi build on GStreamer 1.26.2 will autoplug `dashdemux2` and fail on FLAC-in-DASH no
matter which `uridecodebin` variant is used. Add the rank demotion as a concrete Design-A startup
step, not an implicit consequence of picking legacy `uridecodebin`. **[verified — see §11.2]**

#### 1.5 Encryption

`encryptionType` in a BTS manifest takes the values `NONE` and `OLD_AES`
(`ref:TidaLuna/plugins/lib.native/src/request/decrypt.ts:40-52`). python-tidal defaults
`encryption_type = "NONE"` for MPD streams and reads the field for BTS
(`ref:python-tidal/tidalapi/media.py:655-676`). **Caveat on the vocabulary:** `OLD_AES` is attested by
exactly one file in the whole reference set (a `grep OLD_AES` across all 21 checkouts returns one
match) — it is the set TidaLuna's decrypt path handles, not a proven-exhaustive list of every value
the server can send. This is exactly why the refusal rule below (Strawberry's "anything not `NONE`")
is the correct shape for streamboat, not an allowlist of known-safe values.

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

**Open question: should streamboat request the legacy `HI_RES` tier at all?** Sone's shipped cascade
still requests it as the second tier (below), even though this report's own Summary and §5/§1.7 record
that `HI_RES` (MQA) content was retired on 24 July 2024 and the tier now maps to plain FLAC or nothing.
Whether a live `audioquality=HI_RES` request today returns downgraded FLAC, an error, or the same
answer as `LOSSLESS` was not resolved in any fact-check pass — no reference client's code comment
addresses it, and it requires one live request per tier against the real API. Including a dead tier in
the ladder costs a wasted request per track if it silently downgrades; dropping it risks nothing, since
`HI_RES_LOSSLESS` and `LOSSLESS` already bracket it. **Decide this explicitly rather than copying
Sone's `ORDER` array unexamined** — it is the same fixture-capture task §11.21 already recommends, and
belongs on the Open questions list below. **[gap]**

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

Sone's comment is worth quoting because, if true, it saves three requests per track:

> *"A rate-limit or a terminal-unplayable answer will not change at a lower tier — over-requesting
> quality returns 200 with a downgraded audioQuality, never an error. Walking the rest of the
> cascade only multiplies the request count by 4."*

The early-exit **code** built on this comment is real
(`ref:sone/src-tauri/src/commands/playback.rs:64-78`: network errors, rate-limit and
terminal-unplayable errors all propagate immediately instead of falling through the cascade), but the
underlying **API-behaviour** claim — that over-requesting quality always returns HTTP 200 with a
downgraded `audioQuality` and never an error — is only Sone's own code comment, with no second
reference asserting it. **[unverified]** It is also in tension with tidalt's own descending ladder:
tidalt does the same walk, `HI_RES_LOSSLESS → LOSSLESS → HIGH → LOW`
(`ref:tidalt/docs/architecture.md:17`), and a descending retry ladder only makes sense if
over-requesting sometimes *does* fail rather than silently downgrading. Do not assume the
downgrade-not-error behaviour without observing it directly against the live API. TIDAL's own SDKs do
**not** cascade — they send the whole `formats` array in one request and let the server pick, then
read back the actual quality from `formats` in the response. That is strictly better and should be
preferred when using the v2 API. **[inferred]**

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
| legacy | MP3, ALAC, AC-4 | — | Present in enums, not observed in current tiers |

`ref:tidal-sdk-ios/Sources/Player/Common/Data/AudioCodec.swift` carries `.AC4` and `.ALAC` as
first-class cases, and its `LOSSLESS` mapping has the code comment *"Could be `.ALAC`, but we need to
update Player to get that"* — i.e. TIDAL's own iOS SDK anticipates serving ALAC for `LOSSLESS` on
some path. **A FLAC-only decode assumption for LOSSLESS/HI_RES_LOSSLESS is therefore not
future-proof**; decode both FLAC and ALAC for those tiers if the decoder stack supports it cheaply
(FFmpeg and GStreamer both do). **[verified for the enum/comment; not observed being served to any
reference client in this pass]**

#### 2.2 Decoder stacks and their licences

| Stack | Language | FLAC | AAC-LC | HE-AAC | E-AC-3 | DASH | HLS | Licence | Notes |
|---|---|---|---|---|---|---|---|---|---|
| **GStreamer** (1.26+/1.28) | C, bindings everywhere | yes (`flacdec`, `qtdemux` `fLaC`) | yes (`avdec_aac`/`faad`) | yes | via `gst-libav` | yes (`dashdemux` legacy, `dashdemux2`) | yes (`hlsdemux2`) | Core LGPL-2.1+; plugin sets vary — `gst-plugins-ugly`/`gst-libav` pull in GPL/patent-encumbered code | Everything TIDAL needs works out of the box on Linux; Windows/macOS need shipping the plugin set |
| **FFmpeg / libav\*** | C | yes | yes | yes | yes | yes (`dashdec`) | yes | LGPL-2.1+ by default; GPL if built `--enable-gpl` | tidalt links `libavformat/libavcodec/libswresample` directly and streams via an AVIO callback with no temp files (`ref:tidalt/internal/player/avcodec.go`) |
| **libmpv** | C | yes (via FFmpeg) | yes | yes | yes | yes | yes | mpv is GPLv2+ by default; an LGPLv2.1+ build exists via the Meson switch `-Dgpl=false` (not `--enable-lgpl` — that was the old waf-build flag and no longer exists), intended specifically for libmpv, and disables Linux X11 video output, OSS audio, vdpau, jack, DVD, CDDA, DVB and legacy direct3d | Only cross-platform stack that also gives exclusive output on all three desktops |
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
  (it is). If streamboat ever wants a permissive licence, the LGPL mpv build (`-Dgpl=false`) is the
  escape hatch — but note mpv's own `Copyright` file cautions *"currently it's not recommended to
  build mpv CLI in LGPL mode at all"*; the intended use is specifically libmpv, which is streamboat's
  use case anyway
  (`https://raw.githubusercontent.com/mpv-player/mpv/master/Copyright`).
- Symphonia is MPL-2.0, file-level copyleft, compatible with everything.

#### 2.3 AAC patent status in 2026

- AAC is covered by the Via Licensing Alliance ("Via LA") AAC pool, which still runs an active
  per-unit programme in 2026: the AAC Standard Rate Structure is tiered per-unit — $0.98 for units
  1–500,000, stepping down to $0.78 / $0.68 / $0.45 at higher volume tiers — with 900+ licensees and
  an optional region-based alternative rate table. **[verified from 2026 secondary reporting
  (via-la.com/aac-license-fees-structures/, lumenci.com's AAC-licensing explainer,
  scconline.com's 2026-02-13 Via Licensing Alliance analysis, ipfray.com's Access Advance/Via LA
  pricing piece) — via-la.com itself is blocked from this environment, so the primary programme page
  was reached only through search snippets and third-party summaries, not read directly; a prior pass
  of this report overstated this as read from "Via LA's own programme page"]**
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
- bit-perfect: `set_rate_resample(false)`. **Correction — this call is belt-and-braces, not "the one
  call that makes ALSA bit-perfect" as a previous pass of this report claimed.**
  `snd_pcm_hw_params_set_rate_resample` restricts the configuration space of a *plugin-backed* PCM
  (the `plug`/`rate` chain); a raw `hw:CARD,DEV` PCM has no rate plugin in its chain, so the call is a
  no-op there. It matters only if streamboat ever opens `plughw:`/`default` instead of `hw:` — there
  it turns a silent resample into an open failure. On `hw:`, bit-perfection instead comes from the
  device having no conversion stage at all, **plus the very next line**: the rate read-back.
- `set_rate(rate, Nearest)`, then read back `get_rate()` and **fail** if it differs, with the
  message "DAC doesn't support {n}kHz — turn off bit-perfect mode for compatibility". **This read-back
  is the real guard**, and it is the reason a custom writer is worth building at all — see §11.3:
  GStreamer's own `alsasink` calls `set_rate_near` but never reads the negotiated rate back or
  compares it, so `alsasink device=hw:X,Y` cannot make this check.
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
  declares `finish-args`: `--share=network`, `--share=ipc`, `--socket=fallback-x11`,
  `--device=dri`, `--socket=wayland`, `--socket=pulseaudio`,
  `--filesystem=xdg-run/pipewire-0:ro`, `--filesystem=xdg-run/discord-ipc-0` — eight entries.
  **Correction, previously stated backwards here**: `--socket=pulseaudio` alone already grants
  `/dev/snd` — Flatpak's own sandbox helper (`common/flatpak-run-pulseaudio.c`, `flatpak/flatpak`
  upstream) binds `/dev/snd` into the sandbox whenever `--socket=pulseaudio` is requested and the
  host has it, "since the practical permission of ALSA and PulseAudio are essentially the same...
  we reinterpret pulseaudio to also mean ALSA." Raw ALSA `hw:` **is** reachable inside this
  manifest as shipped; there is no `--device=all` requirement for it, and no manually widened
  permission set is needed. What genuinely is absent is `--device=all` itself, so non-audio device
  access beyond the explicit `--device=dri` grant is not available — but that was never what
  exclusive ALSA needed. **[verified — direct fetch of Flatpak's own sandbox source, not an
  inference from its documented permission model]**
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
  `PROP_DISPATCHER`, `PROP_CONTINUE_ON_ERROR`. **The property is new in GStreamer 1.28**: its
  gtk-doc block reads `GstWasapi2Sink:exclusive: ... Since: 1.28`. It does not exist on 1.26 or
  earlier, which matters because Strawberry's own default sink choice (below) predates 1.28 and
  demotes `wasapi2sink` for unrelated reasons — so a streamboat targeting `wasapi2sink exclusive` for
  Design A needs a minimum-GStreamer floor of 1.28 on Windows specifically, on top of the >= 1.26.10
  floor for FLAC-in-DASH from §1.4.
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
  TIDAL client uses ASIO. **Settled — previously an open question:** `asiosink` does ship in the
  mainstream Windows GStreamer runtime. sone-windows's build instructions bundle `gstasio.dll` from
  the official GStreamer MSVC x86_64 runtime installer's own `lib/gstreamer-1.0/` directory as one of
  its required plugins (`ref:sone-windows/README.md:114-146`) — it is not a separately-built or
  third-party sink. The only piece that remains genuinely open is the Steinberg ASIO SDK's
  redistribution terms for whatever ASIO host component streamboat would ship alongside `gstasio.dll`.
  **[verified for the bundling fact; the SDK licence terms are general knowledge and remain
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
  device ("also known as hog mode"), works internally in 32-bit float and lets CoreAudio convert
  unless an explicit physical `format` (S16/S24/S32/F32) is requested, and warns that hog mode breaks
  virtual devices like BlackHole. **Correction: it does not "close and reopen" on a device rate
  change.** A prior pass of this report said so in three places (here, §6 "Hot-plug", §10.7); all
  three were wrong the same way. CamillaDSP's own docs are explicit that a rate change **stops**
  playback outright and requires an external config reload to resume — it does not self-heal:
  *"If the capture device sample rate changes, then CamillaDSP will stop. … To continue from this
  state, the capture device needs to be closed and reopened. For CamillaDSP this means that the
  configuration must be reloaded."* There is no shipped precedent in this reference set for a player
  that detects a CoreAudio rate change and transparently reopens the device itself — §10.7's
  recommendation that streamboat's macOS writer should do exactly that is still sound, but it must be
  labelled **[inferred]**, not "matches CamillaDSP's design." **[verified against
  `github.com/HEnquist/camilladsp/blob/master/backend_coreaudio.md`, "Sample rate change
  notifications" section — corrects a claim repeated in §6 and §10.7 below]**
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

- Each branch queue: `max-size-time = 15 s`, `max-size-buffers = 0`, `max-size-bytes = 0`. Sone's own
  comment labels this the **decoded PCM reservoir** ahead of `concat`, distinct from the next point —
  see §11.9's stage-by-stage split of "buffering" before treating any single number as *the* buffer
  size. The queue decouples the next decoder from `concat`'s gate on the inactive pad so it can
  pre-buffer while the current track plays.
- `uridecodebin buffer-duration` is 15 s for DASH and 5 s for BTS, `use-buffering = true` — Sone's own
  comment labels this the **compressed network buffer**, a different stage from the queue above.
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
- Gapless is **normal-mode only** in Sone. The DirectAlsa backend never gets a second decode branch
  (the worker gates dispatch on backend type). So on Sone, bit-perfect and gapless are mutually
  exclusive. **[verified]** **Correction/reframing: this understates how much of the hard half Sone
  already has.** The DirectAlsa writer's `WriterCommand::EndOfTrack` handler does **not** call
  `snd_pcm_drain()` — instead it writes a silence period and enters an "idle silence loop — keep DAC
  clock alive between tracks" (`ref:sone/src-tauri/src/audio.rs:1169-1200`), i.e. it already keeps the
  PCM device open across the track boundary, which is the mechanically hard part of exclusive-mode
  gapless. What is actually missing is only a second, prerolled decode branch feeding that
  already-open writer — "add a second decode branch in front of the existing writer", not "invent a
  whole new device-hold mechanism". **A real defect this surfaces, worth fixing regardless of whether
  the second branch is ever added:** because there is no drain, Sone's `track-finished` event fires
  the moment the last decoded chunk is *handed to* the writer, not when the listener actually hears
  the end of the track — up to one full ALSA buffer early (~500 ms at Sone's own settings, §4.5). That
  is the same class of error as §10.8's write-position-vs-audible-position bug, and it corrupts any
  scrobble/play-report timestamp or gapless-timing arithmetic derived from `track-finished` in the
  exclusive-mode path. **[verified for the silence-loop/no-drain behaviour and its timing
  consequence]**

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
- **Segment concurrency.** Corrected from an earlier pass of this report, which mischaracterised
  TidaLuna's semaphore: it bounds concurrent **track fetches**, not segment fetches, and carries no
  claim about matching official-client behaviour. `ref:TidaLuna/plugins/lib.native/src/request/fetchMediaItemStream.ts:20-23`
  wraps the *entire* `fetchMediaItemStream` call (one call per track) in
  `new Semaphore(2)` with the comment "Lock to 2 concurrent streams" — i.e. at most 2 tracks'
  streams are being fetched at once (relevant to prefetch-while-playing, not to a single track's
  segment list). Within one track's stream, DASH segments are fetched strictly **sequentially**:
  `ref:TidaLuna/plugins/lib.native/src/request/fetchStream.ts:39-40` is a plain
  `for (const url of urls) { const res = await fetch(url, reqInit) … }` loop, segment concurrency 1.
  No comment anywhere in `lib.native/src` claims this matches the official client. **[verified —
  corrected]**

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

The type and default are directly verified from `config.ts`; the "max 15000" ceiling is documented
in the doc comment above but **[unverified]** as an enforced limit — no clamping/validation code
against 15000 was located in the player source during fact-check.

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
  (`ref:high-tide/src/lib/player_object.py:470-580`). **Correction: it is not uncapped.** A prior pass
  of this report stated "no size cap and no encryption" — the "no encryption" half is correct, the
  "no size cap" half is wrong. `ref:high-tide/src/window.py:223` starts
  `threading.Thread(target=utils.evict_cache, args=(utils.MUSIC_DIR, 5)).start()` on window
  construction, and `ref:high-tide/src/lib/utils.py:828-843` `evict_cache(cache_dir, max_gb)` computes
  `max_bytes = max_gb * 1024**3`, sorts cache entries by `st_atime`, and unlinks the oldest-accessed
  files until total usage is at or below the cap — a 5 GB atime-ordered LRU cache, run once per window
  creation. Two real weaknesses to note instead of the false "no cap" one: eviction runs only at
  window-open time, never during a long session, so a session that caches past 5 GB stays over budget
  until the app is reopened; and `evict_cache` calls `f.stat().st_size`/`f.unlink()` over a bare
  `cache_dir.iterdir()`, so any subdirectory ever created under `MUSIC_DIR` makes it raise.
  **[verified — corrects a previously-stated "no size cap" claim]**
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
files is a ripper. **Correction to the previous framing:** High Tide's cache is not the uncapped,
indefinitely-retained case it was previously described as — `MUSIC_DIR` is
`Path(CACHE_DIR, "music")` (`ref:high-tide/src/lib/utils.py:76-78`), i.e. inside the XDG cache
directory, and it is capped and LRU-evicted (above). It still lacks encryption at rest and a
per-session/credential tie, which are the two gaps actually worth closing. If streamboat implements
offline caching it should be capped, evicted (ideally continuously, not only at startup — the weakness
High Tide has), encrypted at rest, tied to the current session's credentials, and purged on logout.
**[inferred — this is a design position, not a legal opinion]**

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
1.28 release reporting is cited elsewhere for "seeking in dashdemux2 for streams with gaps", but this
specific sub-claim could not be corroborated on re-check: both of its citations
(`lists.freedesktop.org`, `phoronix.com`) and `gstreamer.freedesktop.org` itself are blocked from this
research environment, and targeted search found nothing else naming this fix. **[downgraded from
"verified" to unverified — treat as unconfirmed until read from an unblocked network]**

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
     Dolby. **[verified]** — `libavcodec/ac3dec.c` and `libavcodec/eac3dec.c` contain zero
     occurrences of "joc", "object" or "atmos"; the only JOC-aware code in FFmpeg is in the
     *parser*, `libavcodec/ac3_parser.c` (~lines 266-282), which reads the additional-bitstream-info
     byte into `hdr->eac3_extension_type_a` with the comment that its LSB "can be used to detect
     Atmos presence" — i.e. FFmpeg can detect that a stream carries Atmos but has no object
     renderer, only the E-AC-3 core-bed decoder.
   - **Passthrough to an AVR over HDMI:** this is the realistic path. It requires an IEC 61937
     bitstream-passthrough output. On Linux that is `alsasink` on an `hdmi:` device with
     `audio/x-eac3` caps and the ALSA IEC958 channel status set; on Windows it is WASAPI exclusive
     with `WAVE_FORMAT_DOLBY_AC3_SPDIF`-class formats; on macOS it is CoreAudio's SPDIF path —
     which, notably, is precisely the one path where GStreamer's macOS backend *does* take hog mode
     (`_open_spdif`).
3. GStreamer has no Atmos renderer. mpv's `--audio-spdif=<codecs>` option supports passthrough for
   `ac3`, `dts`, `dts-hd`, `eac3`, `truehd` and `dsd` (`mpv DOCS/man/options.rst`: "List of codecs
   for which compressed audio passthrough should be used. This works for both classic S/PDIF and
   HDMI."). **[verified]** So the Atmos-to-AVR path on libmpv is concretely `--audio-spdif=eac3`.
   Two details worth keeping: `dsd` passthrough "requires an audio output with exclusive device
   access (currently `wasapi`)" per the same doc, i.e. DSD/DoP passthrough in mpv is Windows-only —
   relevant only if streamboat ever grows a local-library or DSD path, not for TIDAL streaming.

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
`dbus-crossroads` backend (default, more stable) and a `zbus` backend (pure Rust). crates.io declares
MSRV 1.67, last published 2025-06-24, but its own repository `Cargo.toml` (github.com/Sinono3/souvlaki,
tag 0.8.3) declares `edition = "2024"`, which needs Rust >= 1.85 — a real contradiction of the
published MSRV that streamboat's CI should not trust blindly; pin a Rust toolchain >= 1.85 for any
build that includes souvlaki regardless of what crates.io reports. Its README also states a macOS
constraint the report would otherwise miss: souvlaki on macOS "requires an AppDelegate/winit event
loop" — a headless-only macOS build (no event loop) will not get working `MPNowPlayingInfoCenter`
integration from souvlaki. **[verified from crates.io and from souvlaki's own README/Cargo.toml]**

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
initially be empty"* (`ref:sone/src-tauri/src/audio.rs:3254-3266`). **This workaround is already
version-scoped and partly stale**: GStreamer 1.28.3 changed `devicemonitor` to wait for its start
thread to finish before listing devices, so `devices()` immediately after `start()` is no longer
empty on 1.28.3+ (linuxiac.com/gstreamer-1-28-3-released-with-security-and-playback-fixes,
9to5linux.com/gstreamer-1-28-3-adds-nxp-i-mx-8m-plus-hardware-accelerated-h-265-encoding: "devicemonitor
now waits for the start thread to finish when listing devices"). Sone's poll (and any streamboat
workaround copied from it) is only needed on GStreamer 1.28.0-1.28.2; scope it to that version window
rather than inheriting it as a permanent hack. **streamboat should subscribe to `DeviceMonitor` bus
messages rather than polling.** **[inferred]** (A prior pass of this report cited CamillaDSP's
CoreAudio backend here as precedent for "listens for rate-change notifications and reopens" — that is
wrong; CamillaDSP stops on a rate change rather than reopening. See §3.5's correction and §10.7.)

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
| **TidaLuna** | TypeScript | mod inside the official Electron client | official client's | official client's | no | `ref:TidaLuna/plugins/lib.native/src/request/decrypt.ts` (encryptionType values only), `fetchMediaItemStream.ts` (2-concurrent-track semaphore), `fetchStream.ts` (sequential per-track segment fetch) |
| **tidal-connect** | Bash + Docker around a proprietary binary | proprietary | ALSA via PortAudio | proprietary | up to 24/48 hi-res; MQA self-unfolding lost July 2024 (see §11 correction) | `ref:tidal-connect/bin/entrypoint.sh` |

---

### 8. Candidate audio stacks for streamboat

Version and licence data below is from the crates.io API on 2026-09-07 and from upstream source
reads. **[verified]**

| Stack | Linux | Windows | macOS | Headless/Pi | Bit-perfect Linux | Bit-perfect Win | Bit-perfect macOS | 24/192 | Gapless | DASH | Licence | Maturity | Mobile path |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **gstreamer-rs 0.25.3** + own ALSA writer (the Sone design) | yes | yes | yes | yes | yes (own writer) | yes (`wasapi2sink exclusive`) | **no** (would need an own CoreAudio writer) | yes | yes (`concat`) | yes | bindings MIT/Apache-2.0; GStreamer LGPL-2.1+ | very high; two shipping TIDAL clients | GStreamer runs on Android/iOS but is heavy; would likely swap the sink |
| **libmpv (`libmpv2` 6.0.0)** | yes | yes | yes | yes | yes (`--ao=alsa --audio-device=alsa/hw:X,Y` — device selection, **not** `--audio-exclusive`, which mpv silently ignores on the `alsa` AO) | yes (`--ao=wasapi --audio-exclusive=yes`) | **yes** (`coreaudio_exclusive`, reached via `--ao=coreaudio --audio-exclusive=yes`) | yes | yes (`--gapless-audio=weak`) | yes (FFmpeg) | crate LGPL-2.1; mpv GPLv2+ (LGPL build via Meson `-Dgpl=false`) | very high | mpv builds for Android; iOS is awkward |
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
--ao=alsa               (Linux)   --audio-device=alsa/hw:1,0   --alsa-resample=no  (mpv's default anyway; harmless to set explicitly)
--ao=wasapi             (Windows) --audio-exclusive=yes        --wasapi-exclusive-buffer=default
--ao=coreaudio          (macOS)   --audio-exclusive=yes        --coreaudio-change-physical-format=yes
--gapless-audio=weak    (never `yes` — `yes` locks the rate to the first track)
--audio-channels=auto-safe
--prefetch-playlist=yes (default no — without it `loadfile append` queues but never actually prefetches)
--volume-gain=<db>      apply TIDAL's ReplayGain as dB on top of user volume (see below)
--demuxer-lavf-o=protocol_whitelist=file,crypto,data,http,https,tcp,tls   (needed for a DASH MPD referencing http(s) segment URLs)
--cache=yes --demuxer-max-bytes=… --demuxer-readahead-secs=…
```

- **Queue/gapless:** use `loadfile <uri> append` to keep the next track queued, **plus
  `--prefetch-playlist=yes`** — without it mpv's own docs say prefetch defaults to `no` and nothing
  is fetched ahead of time; note mpv's own caveat that prefetch "can occasionally make wrong
  prefetching decisions" if the queue is reordered, so disable it (or rebuild the playlist) around a
  user reorder.
- **ReplayGain:** mpv exposes `--volume-gain=<db>` ("applied on top of other volume and gain
  settings", range set by `--volume-gain-min`/`-max`, default -96..+12 dB) — set this from TIDAL's
  gain formula directly in dB rather than converting to a percentage and setting `volume`. mpv's own
  `--replaygain` option (default `no`, correctly left off) reads tags from the file, which TIDAL's
  fMP4 does not carry; `--replaygain-clip=<yes|no>` defaults to `no`, i.e. mpv already avoids
  clipping on top of ReplayGain — worth mirroring in a hand-rolled formula.
- **DASH loading:** FFmpeg's demuxer refuses to follow http(s) segment URLs out of a `file://` or
  `data:` manifest unless the protocol whitelist is widened — the equivalent of High Tide's own
  remux invocation, `ffmpeg -protocol_whitelist file,crypto,data,http,https,tcp,tls -i manifest.mpd
  …` (`ref:high-tide/src/lib/player_object.py`). On libmpv this is `--demuxer-lavf-o=protocol_whitelist=…`
  (and/or `--stream-lavf-o=…`). This is very likely the actual answer to the open question below
  about whether libmpv accepts a `data:application/dash+xml;base64,…` URI directly.
- **Device reservation:** libmpv has **no** `org.freedesktop.ReserveDevice1` support of any kind — it
  is absent from both `DOCS/man/ao.rst` and `DOCS/man/options.rst`. Design B therefore needs the
  D-Bus reservation handshake done in the *host* process, exactly as tidalt drives it externally
  (`ref:tidalt/internal/player/mpv.go:314-392`: reserve → hand the now-free `hw:` device to mpv via
  `--audio-device` → release on stop), not inside libmpv. Skipping this means exclusive mode is a
  "device busy" error for every PipeWire user — the same failure the report criticises Sone for.
- **Threads:** control (tokio) plus one thread pumping `mpv_wait_event`.
- **Pros:** one dependency covers decode, DASH, HLS, buffering, seek, gapless and exclusive output on
  all three desktops. Dramatically less code than A. Excellent headless fit (mpv is already a
  headless player). Trivially the fastest route to a working product.
- **Cons:** GPLv2+ unless LGPL mode is used (and mpv's own docs discourage LGPL mode for anything but
  libmpv — which is exactly this use case); less visibility into the exact signal path, so the
  transparency panel becomes "what we asked for" rather than "what the kernel says" (though the
  `/proc/asound/*/hw_params` probe from Sone still works and gives ground truth; libmpv's own
  `audio-out-params`/`audio-params`/`current-ao`/`audio-device-list` properties are the read-back
  half of that panel); property-string API rather than typed pipeline; passing a
  `data:application/dash+xml;base64,…` URI to mpv is **[unverified]** — the protocol-whitelist point
  above is the most likely fix, and the fallback is to write the MPD to a temp file and pass
  `file://`, which is exactly what mopidy-tidal and modern High Tide do anyway; no ReserveDevice1
  support (see above).

#### Design C — pure Rust, no C media framework

**Fits:** Symphonia 0.6.1 + `dash-mpd` 0.20.4 + `stream-download` 0.24.4 + `alsa` 0.12.1 /
`wasapi` 0.24.0 / `coreaudio-rs` 0.14.2 + `rubato` 5.0.0 for the non-bit-perfect path.

```
manifest → MPD parse (dash-mpd) → segment fetcher (sequential per track, reqwest)
        → fMP4 reassembly (init + $Number$ segments) → Symphonia ISO/MP4 reader
        → Symphonia FLAC / AAC-LC decoder → ring buffer → device writer
```

(No reference client fetches DASH segments concurrently within one track — see the corrected §4.2
finding above — so "sequential" is the pattern to copy, not an arbitrary concurrency limit.)

- **Pros:** no C build dependency on any platform, smallest binaries, best mobile story, full control
  of the signal path (so the transparency panel is trivially truthful), all-permissive licences.
- **Cons:** **no HE-AAC** — you cannot play the `LOW` tier; you own the DASH assembler, the fMP4
  reassembly, the seek logic, the buffering policy and three device backends; and you get no video
  path at all. **FLAC-in-fMP4 support in Symphonia is real but undocumented and partial:** the
  README's format table lists ISO/MP4 as "Great" but does not list FLAC among its codecs and marks
  ISO/MP4 gapless support "No" — yet the source does parse it:
  `symphonia-format-isomp4/src/atoms/mod.rs` declares `AtomType::Flac`/`FlacAtom`, and
  `atoms/stsd.rs`'s `read_audio_sample_entry` accepts `AtomType::Flac` and calls
  `flac.fill_codec_params(codec_params)`. Three concrete traps for a hand-rolled Design C: (a)
  `stsd.rs` returns `unsupported_error("isomp4: more than 1 sample entry")` for a multi-entry
  `stsd` — check TIDAL's init segments have exactly one; (b) the demuxer's gapless support for
  ISO/MP4 is "No", so gapless is entirely streamboat's code to write above the demuxer, not
  something Symphonia gives you; (c) TIDAL's DASH init segments carry no `sidx` box, so
  `format.seek()` (which relies on `sidx` for non-seekable-source seeking) will not work — seeking
  means jumping to the right `$Number$` segment from the manifest's `SegmentTimeline` and re-feeding
  the reader from there, the same approach python-tidal/tidal-cli use, not a demuxer-level seek.
  **[uncertain — Symphonia's own README documents none of this; verified only against the source
  files above, not exercised against a real TIDAL manifest in this pass]**
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

### 10. Findings added by independent fact-check

This section folds in facts a second-pass review surfaced that the original findings above did not
cover. Each item cites its source the same way as the rest of this report.

#### 10.1 Headless / Raspberry Pi gets a real answer: tidalt's client/server model

The brief mandates headless/server/CLI mode now, including Pi-class hardware, but §3–§9 above are
desktop-centric. tidalt has a complete, documented design for exactly this, and it resolves the
"one process or two" architecture question (see Open questions) at the same time:

- **Single-owner process.** tidalt claims the D-Bus name `org.mpris.MediaPlayer2.tidalt` on the
  session bus at startup; if the name is already taken (`ErrAlreadyRunning`) the process becomes a
  thin client instead of exiting (`ref:tidalt/docs/client-server.md`). The reason is physical: "ALSA
  `hw:` devices cannot be shared between processes. If two programs both try to open `hw:1,0` the
  second one fails." Modes: `tidalt` (TUI, becomes server or client), `tidalt daemon` (headless
  engine, no terminal), `tidalt play tidal://track/<id>` (one D-Bus call, exits — used as a browser
  URL handler), `tidalt setup --daemon` (installs a systemd **user** service).
- **Consequence documented in the README:** *"A plain `tidalt` TUI session does not register a
  persistent MPRIS2 service, so media keys and `playerctl` will have no effect when the TUI is
  closed"* (`ref:tidalt/README.md:156`).
- **Container/headless recipe** (`ref:tidalt/docs/docker.md:89-125`):
  `docker run -d --device /dev/snd --group-add $(getent group audio | cut -d: -f3) …
  benehiko/tidalt:latest daemon`, and for MPRIS reachable from the host,
  `-v /run/user/$(id -u)/bus:/run/user/1000/bus -e DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus`.
- **Packaging:** `ref:tidalt/README.md:45-67` ships `arm64`/`aarch64` `.deb`/`.rpm` alongside amd64.
- **Pi audio hardware specifics**, from `ref:tidal-connect/userconfig/`: HDMI on Pi is ALSA card
  `vc4hdmi` (`vc4hdmi0`/`vc4hdmi1` on Pi 4, which has two HDMI outputs) and needs an `iec958` plug
  with `slave.format "IEC958_SUBFRAME_LE"` over a `type hw` slave. The same tree's README records
  *"I had to limit audio quality to 16bit/44.1kHz, otherwise this would fail when trying to stream
  hi-res content"* on Pi HDMI — a real, hardware-imposed narrowing case (see §10.12 dither). I2S HAT
  card names are literal ALSA card ids, e.g. `card snd_rpi_hifiberry_dacplus`
  (`ref:tidal-connect/userconfig/hifiberry-dac-plus.asound.conf`), `iqaudio-dac.asound.conf`.
- **No session D-Bus on a headless box, often.** A stripped-down Pi/server image frequently has no
  session bus at all, which silently disables both `ReserveDevice1` (tidalt's own reservation code
  skips itself when there is no session bus) and MPRIS. A headless build should not depend on either
  and should simply own `hw:` outright rather than degrading silently.

**Debian packaging note.** `ref:tidalt/README.md:45-67` targets current distributions; combine this
with the §1.4 finding that Debian 13 "trixie" (Raspberry Pi OS's current base) ships GStreamer
1.26.2 — eight releases short of the 1.26.10 FLAC-in-DASH floor — when picking the GStreamer-based
Design A for headless/Pi.

#### 10.2 Bit-perfect mode silently disables loudness normalization, not only the volume slider

Implication #9 (disable the volume slider in bit-perfect mode) and #10 (apply the ReplayGain
formula) look independent but are not: ReplayGain **is** a software gain, and applying it destroys
bit-perfection exactly as the slider would. Sone resolves this silently, and it belongs in the
report explicitly: `ref:sone/src-tauri/src/audio.rs:2936-2948` builds the bit-perfect pipeline branch
as `let (u_vol, n_vol, capsfilter_weak) = if bit_perfect { … (None, None, None) }` — in bit-perfect
mode **neither** the user-volume nor the ReplayGain (`norm_vol`) GStreamer element is created, versus
`:1833-1841` in normal mode, which builds both. Sone's README documents only the slider half of
this. **The correct statement for streamboat: bit-perfect implies no user volume, no ReplayGain, no
dither, no resample — full stop** — and the transparency panel (§3.2) should read "ReplayGain:
bypassed (bit-perfect)" rather than show a gain factor that was never applied. **[verified]**

#### 10.3 Hardware/device-mixer volume as the bit-perfect-compatible volume control

"Mute only, no attenuation" (Implication #9) is not the only bit-perfect-compatible answer, and for
users without an analogue preamp it is a poor one: many USB DACs and every Pi I2S HAT expose an ALSA
mixer control that attenuates **in the DAC**, downstream of the digital bitstream, so the stream
itself stays bit-perfect. Three concrete mechanisms:

1. **libmpv:** the `ao-volume` (RW) property is documented as *"System volume … on ALSA this usually
   changes system-wide audio volume on a linear curve"* — distinct from `volume` (*"the internal
   mixer (aka software volume)"*). The mixer element is selectable with `--alsa-mixer-device=<device>`
   (default `default`), `--alsa-mixer-name=<name>` (default `Master`, e.g. `PCM`), and
   `--alsa-mixer-index=<number>` (`mpv DOCS/man/input.rst`, `DOCS/man/ao.rst`). On Design B, `ao-volume`
   is the bit-perfect-compatible volume control, not `volume`.
2. **Direct ALSA:** `snd_mixer_*` on the selected card's playback element. No reference client does
   this — it is code streamboat would own.
3. **The negative example, worth avoiding:** tidal-connect layers an ALSA `type softvol` plugin over
   the `hw:` device and explicitly warns about the ambiguity this creates —
   `ref:tidal-connect/bin/common.sh:141-152` checks whether a real `Master` mixer control already
   exists and, if so, renames its own softvol control to `SoftMaster`, printing *"*WARNING* Tidal
   volume slider might act on the hardware volume control"*. **streamboat's UI must state explicitly
   which control the slider is bound to** — this exact ambiguity is what would make an audiophile
   client untrustworthy.

**[verified]** — mpv `DOCS/man/input.rst` (`ao-volume`/`ao-mute`), `DOCS/man/ao.rst` (`alsa` AO
mixer options), `ref:tidal-connect/bin/common.sh:136-211`,
`ref:tidal-connect/userconfig/xmos-dac-softvol-s16.asound.conf`.

#### 10.4 Real-time thread scheduling for the writer thread

Design A specifies a writer thread at "real-time-ish priority" (§9) with no detail. With a ~500 ms
ALSA buffer and 50 ms periods this survives on a desktop and glitches on a loaded Pi. Strawberry
already does the correct thing in a file this report cites for other reasons:
`ref:strawberry/src/engine/gstenginepipeline.cpp:1788-1803`, `GstEnginePipeline::TaskEnterCallback`:

```cpp
#ifdef Q_OS_UNIX
sched_param param{}; param.sched_priority = 40;
pthread_setschedparam(pthread_self(), SCHED_RR, &param);
#endif
#ifdef Q_OS_WIN32
SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
#endif
```

installed as GStreamer's task-enter hook, so every streaming thread gets it. **[verified]** Beyond
what Strawberry does, note for streamboat:

- On Linux an unprivileged process usually cannot call `sched_setscheduler(SCHED_RR)` without
  `CAP_SYS_NICE` or an `rtprio` limit in `/etc/security/limits.d`; the portable route is RealtimeKit
  (`org.freedesktop.RealtimeKit1.MakeThreadRealtime` on the system bus — the same mechanism PipeWire
  and JACK use), falling back to `nice()` when unavailable. **[inferred — standard RealtimeKit usage,
  not read from a reference client in this pass]**
- On Windows, an exclusive WASAPI writer thread should join MMCSS via
  `AvSetMmThreadCharacteristics("Pro Audio", …)`. On macOS, `thread_policy_set` with
  `THREAD_TIME_CONSTRAINT_POLICY`. **[inferred — standard platform practice, not read from a
  reference client]**
- The discipline that makes the priority worth having: no allocation, no locks, no logging inside
  the writer loop. Sone already follows this with a preallocated `silence_buf` and atomics — worth
  calling out explicitly as a rule, not just an implementation detail.

#### 10.5 Verifying bit-perfectness needs a test plan

"Bit-perfect" is a claim about bytes; without an automated way to check it, every refactor of the
format-promotion ladder or the ALSA reopen path risks silently breaking the one feature the client is
built around. Four concrete, headless-friendly mechanisms, none of them in the original report:

1. **ALSA loopback.** Load the `snd-aloop` kernel module, point streamboat at `hw:Loopback,0`, capture
   from `hw:Loopback,1` (`arecord -D hw:Loopback,1 -f S32_LE -r 96000`), and byte-compare the capture
   against the reference decode. Runs headless; the only true end-to-end proof.
2. **Decoder self-check.** FLAC's `STREAMINFO` block carries an MD5 of the *unencoded* audio — a
   decode-and-hash check proves the decoder half independent of the output path.
3. **Kernel ground truth as an assertion, not just a UI feature.** `/proc/asound/<card>/pcm<N>p/sub<M>/hw_params`
   reports the format/rate/channels/period_size/buffer_size actually negotiated, or the literal
   string `closed` (already found by this report for the transparency panel, `ref:sone/src-tauri/src/pipeline_probe.rs`)
   — assert on it in CI, not only render it.
4. **On libmpv,** `audio-out-params` is documented as *"Same as `audio-params`, but the format of the
   data written to the audio API"* — read it back and assert it equals the source format/rate;
   `current-ao` and `audio-device-list` complete the picture (`mpv DOCS/man/input.rst`).

Also add a unit test for the 24-bit ALSA/GStreamer naming inversion (Implication #6 asks for one, no
fixture existed): assert `S24_LE` -> 4 bytes/frame/channel and `S24_3LE` -> 3. **[verified for the
mechanisms cited above; the recommendation to wire them into CI is [inferred]]**

#### 10.6 Windows exclusive-mode failure modes Design A must handle

§3.4 says which GStreamer sink to use but nothing about WASAPI exclusive-mode failure states, all of
which are standard territory an implementer will hit immediately:

- Exclusive mode requires the per-endpoint *"Allow applications to take exclusive control of this
  device"* checkbox in Windows Sound settings to be on. There is no API to enable it; if it is off,
  `IAudioClient::Initialize` returns `AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED` and the only fix is a
  user instruction — surface this as a distinct, actionable error state, mirroring the
  `deviceexclusivemodenotallowed` event this report already found in TIDAL's own native-player
  vocabulary (`ref:tidal-sdk-web/packages/player/src/player/nativeInterface.ts`).
- Format support must be probed per (rate, bit depth, container) with
  `IAudioClient::IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, …)` — the Windows analogue of Sone's
  `probe_supported_gst_formats`/`probe_supported_rates` ladder (§3.1).
- `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED` must be handled by re-querying `GetBufferSize` and
  re-initialising with the aligned duration — the classic exclusive-mode trap.
- `AUDCLNT_E_DEVICE_IN_USE` is the Windows analogue of ALSA's `EBUSY` and needs the same
  retry/report treatment as §3.2.
- Exclusive mode should be event-driven (`AUDCLNT_STREAMFLAGS_EVENTCALLBACK`) with the feeding thread
  registered with MMCSS (§10.4).

**[inferred — standard WASAPI exclusive-mode documentation (Microsoft Learn, "Exclusive-Mode
Streams"), named here but not fetched in this pass; the error-vocabulary cross-reference to
`nativeInterface.ts` is verified]**

#### 10.7 macOS per-track sample-rate switching needs a nominal-rate change, not only a physical-format change

Design A's macOS bullet (§9) lists hog mode, `SupportsMixing=false` and
`kAudioStreamPropertyPhysicalFormat`, but a hi-res client's core operation — switching
44.1k -> 96k -> 192k between tracks — is a **nominal sample-rate change**
(`kAudioDevicePropertyNominalSampleRate`) that completes asynchronously on CoreAudio and must be
waited on via a property listener. **Correction: CamillaDSP is not a shipped precedent for
"listen and reopen."** Its CoreAudio backend does listen for these change notifications, but its
documented response is to **stop** playback and require a config reload, not to reopen the device
itself (`https://github.com/HEnquist/camilladsp/blob/master/backend_coreaudio.md`, "Sample rate
change notifications" — see the correction in §3.5). GStreamer's own CoreAudio HAL shows the same
class of asynchrony from the other side — a four-attempt physical-format confirm loop
(`gstosxcoreaudiohal.c:505-535`, already cited in §3.5), which *is* a real precedent for waiting on
the async completion, just not for the reopen-and-continue behaviour. **The recommendation itself
(add a property listener, wait for the async completion, then resume writing) is still right, but no
reference client in this set actually does the reopen half — mark it [inferred], not
[verified-by-precedent].** Without a listener at all, a macOS writer racing the driver on every album
that mixes sample rates is the concrete failure mode. **[verified for the CamillaDSP correction;
the design recommendation is [inferred]]**

#### 10.8 Position must be corrected for buffered-but-unplayed frames

Every position readout found in this report (`frames_written / rate` in Sone,
`ref:sone/src-tauri/src/audio.rs:2350,2377`) is derived from frames **written**, not frames
**played**. `rg 'snd_pcm_delay|get_delay'` across every reference checkout in this pass returns
nothing — no client corrects for the device buffer. With Sone's own ~500 ms ALSA buffer, this means
the progress bar, MPRIS `Position`, and any scrobble timestamp can run up to half a second ahead of
what the listener actually hears — and it is wrong at the exact moment a gapless transition is timed
off it. **The fix is `snd_pcm_delay()`** (exposed as `PCM::delay()` in the Rust `alsa` crate):
`played = frames_written - delay`. On libmpv this is already handled — `time-pos`/`playback-time`
are AO-delay corrected — a concrete advantage of Design B and a required correction if Design A ships.
**[verified as an absence across the reference set; the fix is standard ALSA practice]**

#### 10.9 Device-hold policy across pause/idle is an explicit choice with two opposite shipped answers

Holding an exclusive `hw:` device across a pause means no other application can make a sound while
streamboat is merely paused; releasing it means a relay click and re-negotiation on every resume.
Both policies ship today: **Sone holds** — its software pause writes 50 ms silence buffers to pace
the thread and only tears the PCM down and reopens afterwards; its writer state "lives outside
`PlaybackBackend` so it persists across track changes" (`ref:sone/src-tauri/src/audio.rs:105`, pause
path ~1470+). **tidalt releases** — *"The daemon holds exclusive access to the audio device only
while a track is actually playing — releasing it on pause so other applications can use it freely"*
(`ref:tidalt/README.md:11`). Recommendation: hold while playing, release after a configurable idle
timeout on pause, and release the `org.freedesktop.ReserveDevice1` name at the same moment — keeping
the reservation without using the device helps nobody. **[verified for both cited behaviours;
the recommendation is [inferred]]**

#### 10.10 Network-stall behaviour in exclusive mode is undocumented in the original report

§4.5 lists buffer sizes but never says what the writer does when the decoder starves — the most
common real-world interruption on a marginal connection. Sone's actual policy: the writer blocks on
`rx.recv_timeout(period_duration)` and on timeout calls `write_silence(&pcm, &silence_buf)` — it
feeds the DAC a period of silence rather than underrunning, and only tears down (`audio-error
{kind:"device_disconnected"}`) if the silence write itself fails
(`ref:sone/src-tauri/src/audio.rs:1024`, `:1315-1335`). Two consequences streamboat must handle that
Sone does not: `frames_written` keeps incrementing during the silence fill, so position (§10.8)
drifts forward through a stall even before accounting for buffer delay; and in the DirectAlsa path
Sone only *logs* GStreamer's `Buffering` bus percentage (`ref:sone/src-tauri/src/audio.rs:1761-1765`)
rather than surfacing a rebuffering UI state, so a stall is indistinguishable from silence in the
track. Recommendation: count silence-fill periods, enter an explicit `Rebuffering` state and freeze
the position clock, and pick a threshold beyond which the device pauses instead of feeding silence
indefinitely. **[verified for Sone's behaviour; the recommendation is [inferred]]**

Related and still open: CDN URLs inside a BTS manifest carry their own expiring token, shorter-lived
than the 1-hour manifest window (§4.8) — a segment 403 mid-track needs a defined recovery
(re-fetch the manifest, resume at the current position, without tearing down an exclusive-mode ALSA
handle) that no reference project documents and this report does not fully design. For **direct**
(BTS/BaseURL) URLs, resume is an HTTP `Range: bytes=<offset>-` re-request on the same URL —
mopidy-tidal's cache proxy already implements the full Range/Content-Range handling needed
(`ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/types.py:12-40`, `proxy.py:219`). For DASH, resume is
re-fetching from the current `$Number$`. **[inferred design, not found in any reference client]**

#### 10.11 A third MPD shape and byte-range seeking are unhandled

The brief asked for "init + media segments, FLAC in fMP4, segment templates, byte ranges"; §1.4
covers `SegmentTemplate`+`SegmentTimeline` and stops. tidal-cli has a documented fallback for a bare
`<BaseURL>` MPD with no `SegmentTemplate` at all: after failing to find
`initialization=`/`media=`/`<S d=…>`, it tries `decoded.match(/<BaseURL>([^<]+)<\/BaseURL>/)` and, if
present, returns `{ type: 'direct', url: baseUrlMatch[1], codecs }` — handled identically to a BTS
manifest (`ref:tidal-cli/src/playback.ts:100-152`, union type `type: 'direct' | 'dash'`). No
reference client uses DASH `SegmentBase`/`indexRange` — but **that absence is weaker evidence than it
looks, and a third-pass fact-check downgrades it accordingly.** Neither reference parser is a real XML
parser: tidal-cli matches the MPD with plain regexes (`initialization="([^"]+)"`, `media="([^"]+)"`,
`<S d="(\d+)"(?:\s+r="(\d+)")?\/>`) and python-tidal blindly indexes
`representations[0].segment_templates[0].segment_timelines[0]`, raising `ManifestDecodeError`
otherwise. An MPD using `SegmentBase`/`indexRange`, or even an ordinary `<S t="0" d="…" r="…"/>` with a
`t` attribute, would produce **zero segments** from tidal-cli's regex and a hard failure from
python-tidal — neither client would notice such a manifest, so their silence is evidence about their
parsers' narrowness, not about what TIDAL actually sends. **Do not read "probably absent" as licence
to skip the shape; the only thing that actually settles it is a captured fixture (§11.21).** HTTP
`Range` on **direct** (BTS/BaseURL) URLs is real and is how seeking works there regardless; see
mopidy-tidal's cache proxy above. streamboat needs an explicit position on both: parse `<BaseURL>` as
a fallback MPD shape, and support `Range` on direct URLs so a seek does not re-download from byte 0.
**[verified for tidal-cli's fallback and mopidy-tidal's Range handling; the "probably absent"
inference is downgraded to uncertain — the reference parsers cannot detect the shape either way]**

#### 10.12 Dither is required, not "never needed", when the DAC is narrower than the source

§3.6 concludes bit-perfect promotions are all widening, so "no dither is ever needed" — true only
inside that ladder. A 24/96 track on a 16-bit-only device is a **narrowing** conversion, exactly
where dither matters (undithered truncation produces audible correlated distortion on fades and
reverb tails), and this case is real on the brief's own target hardware: Pi HDMI needs "16bit/44.1kHz"
for hi-res content per `ref:tidal-connect/userconfig/README.md`, and
`ref:tidal-connect/userconfig/xmos-dac-softvol-s16.asound.conf` pins `format S16_LE`. Sone's ladder
(`pick_capsfilter_format`) only ever widens, then falls back to "the DAC's widest probed format"
without stating what happens when that is narrower than the source. **Rule for streamboat:**
bit-perfect mode fails loudly on a narrowing device (consistent with Implication #5); non-bit-perfect
mode must enable dither on any bit-depth reduction (in GStreamer, that means *not* setting
`audioconvert dithering=none` on that path — its default is TPDF, which is correct here), and the
transparency panel should report "dithered 24->16". The same reasoning applies to in-place integer
PCM volume scaling (`ref:sone/src-tauri/src/audio.rs:965-1010`): doing gain in f32 and dithering on
the way back to integer is the correct form, not scaling the integer PCM directly. **[verified for
the Pi-hardware narrowing case and Sone's ladder; the dither rule is standard DSP practice]**

#### 10.13 Device identity must be stable across reboots and hot-plug

ALSA card *indices* are assignment-order dependent: unplug and replug a USB DAC, or add a second
card, and `hw:1,0` now points at something else. tidal-connect already configures by card **name**:
`CARD_NAME=D10` (`ref:tidal-connect/samples/topping-d10.env`), and its asound.conf files resolve by
card id string (`card DAC`, `card snd_rpi_hifiberry_dacplus`, `card "vc4hdmi"`). Sone, by contrast,
enumerates through GStreamer's `DeviceMonitor` and falls back to composing `hw:C,D` from
`alsa.card`+`alsa.device` — the index form (`ref:sone/src-tauri/src/audio.rs:3285-3287`).
**Recommendation:** persist the ALSA card *id* (and, on Windows, the WASAPI endpoint id string; on
macOS, the device UID — both already stable), resolve to an index at open time, and when the saved
device is absent, refuse to silently fall back to `default` — offer the user the choice instead. A
silent fallback to the motherboard codec on a missing DAC is exactly the failure that would make an
audiophile client untrustworthy. **[verified for both cited behaviours; the recommendation is
[inferred]]**

#### 10.14 streamingSessionId is client-generated, not server-issued

§1.2/§4.8 cite the `x-tidal-streamingsessionid` (v1) and `x-playback-session-id` (v2) headers without
saying who creates the value. TIDAL's own web SDK generates it:
`ref:tidal-sdk-web/packages/player/src/internal/helpers/generate-guid.ts` builds a v4 GUID from
`crypto.getRandomValues`; it is threaded as `streamingSessionId` through
`playback-info-resolver.ts` into both the v1 header (`x-tidal-streamingsessionid`, ~line 218) and the
v2 header (`x-playback-session-id`, ~lines 365/450). **One id per media-product playback**, created
before the manifest request, reused for that same product's prefetch, and echoed back in the v1
response body. TIDAL's SDKs key their entire `streaming_metrics` event set (`playback_info_fetch`,
`streaming_session_start`/`_end`, `playback_statistics`, `drm_license_fetch`, already cited in §4.8)
on this id — if streamboat sends any play-reporting at all (Open questions, owner decision 5), this
is the join key, and a client that reuses one id across tracks or omits it will produce broken
reporting. **[verified]**

#### 10.15 mediaMetadataTags / audioModes: the vocabulary behind Implication #16

Implication #16 ("surface `audioModes`/`mediaMetadataTags` in the UI") never defined what those
fields contain. python-tidal has the exhaustive vocabulary:
`class MediaMetadataTags(str, Enum): hi_res_lossless = "HIRES_LOSSLESS"; lossless = "LOSSLESS";
dolby_atmos = "DOLBY_ATMOS"` (`ref:python-tidal/tidalapi/media.py:87-95`). The values arrive on the
**track** object, not the manifest: `self.media_metadata_tags = json_obj.get("mediaMetadata",
{}).get("tags", {})` (line 362), consumed as `Track.is_hi_res_lossless`/`is_lossless` (from tags)
and `Track.is_dolby_atmos` (from a separate `audioModes` array, checking
`AudioMode.dolby_atmos in self.audio_modes`) at lines 529-557. There is no `SONY_360RA` tag and no
MQA tag, consistent with §5/§1.7's findings that both are gone. **Practical use:** clamp the
requested `formats` array to what the track actually offers before calling the manifest endpoint,
and render the HIRES_LOSSLESS/DOLBY_ATMOS badges from these same fields rather than from a manifest
round-trip. **[verified]**

#### 10.16 A webview UI does not preclude bit-perfect audio — only rendering audio in the browser engine does

§8's Electron row and the Chromium-resampling finding (Summary, now hedged as uncertain) risk being
read as "any webview-based UI cannot be bit-perfect." Two of this report's own strongest references
disprove that broader reading: Sone and sone-windows render their entire UI in a Tauri webview and
are nonetheless the strongest bit-perfect references in the set, because **audio never touches the
web layer** — GStreamer decodes into an `appsink` and a Rust thread writes to `libasound`/WASAPI
directly (`ref:sone/src-tauri/src/audio.rs`). The one place Sone *does* route audio through the
webview is video, and it says so: *"Video audio is streamed and does not use the bit-perfect
lossless signal path that music tracks use"* (`ref:sone/README.md:501`), played via hls.js. **The
rule:** bit-perfect is impossible only when the browser engine itself renders the audio (MSE/Web
Audio — the tidal-hifi design); a webview used purely for UI, with audio handled by a native core, is
unaffected. This is a real fourth entry missing from §8's table: **native core + webview UI** (Tauri,
or an Electron/N-API native addon) — the stack two of this report's own reference clients actually
ship. **[verified]**

#### 10.17 Stacks named in the brief but missing from §8's comparison

The brief named candidate stacks to compare, including miniaudio explicitly; §8's table omits it,
omits python-mpv, and has no C++ libmpv row. One honest line each: **miniaudio** (single-header C,
public-domain/MIT-0) covers device output plus basic decode with WASAPI exclusive-mode and
CoreAudio/ALSA backends — a candidate replacement for the `alsa`/`wasapi`/`coreaudio-rs` trio in
Design C, but it has no DASH, no FLAC-in-fMP4 demuxing and no network layer, so it does not replace
Symphonia. **[uncertain — miniaudio's capability claims here should be re-checked against
https://miniaud.io/docs/ before being relied on; not independently verified in this pass]**
**python-mpv** is a ctypes binding to the same libmpv as Design B — inherits Design B's exclusive-
output story exactly, at the cost of Python packaging pain on Windows/macOS; worth a row because
High Tide already proves a Python+GStreamer client viable on Linux, so the comparison should be
like-for-like. **C++ + libmpv** is Design B with a different host language and no new capability.
None of these changes the §9 recommendation; they keep the comparison honest against what the brief
asked for.

---

### 11. Findings added by a second independent fact-check

This section folds in a second round of corrections and gaps that a follow-up review surfaced —
partly errors introduced or left in §1–§10 above, partly real gaps the brief needs an answer to that
neither this report's first pass nor §10 covered. Same citation convention as the rest of the report.

#### 11.1 GStreamer's 1.28 series has moved past 1.28.3 — it is now at 1.28.6, the final 1.28 release

The Summary and §6 previously stated "1.28.2 and 1.28.3 are out as of this check" and built the
hot-plug advice around 1.28.3 being current. As of 2026-09-08 the series has shipped 1.28.4, 1.28.5,
and **1.28.6** (5 August 2026), which multiple sources describe as the final bug-fix release of the
1.28 stable series. 1.28.6 also adds FFmpeg 9.0 support — directly relevant because §8 already
recommends `ffmpeg-next` 9.0.0 for any FFmpeg-touching part of the stack. The 1.28.3 `devicemonitor`
fix (§6, §10 "Hot-plug") is unaffected and still stands as the version that fixed the empty-`devices()`
race; only the "latest is 1.28.3" framing was stale. **[verified — see the linuxiac.com/9to5linux/
linuxcompatible.org rows in Sources]**

#### 11.2 Plain `uridecodebin`/`playbin` does not select the legacy DASH demuxer — `dashdemux2` outranks it

§1.4's "Packaging consequence" now states this correction directly; repeated here because it is the
single most load-bearing fix in this pass for the Design-A recommendation. Choosing `uridecodebin`
over `uridecodebin3` selects the *source* element, not the demuxer. Both `dashdemux` (legacy,
gst-plugins-bad, `GST_RANK_PRIMARY`) and `dashdemux2` (gst-plugins-good's `adaptivedemux2`,
`GST_RANK_PRIMARY + 1`) register for `application/dash+xml`; autoplugging always picks the
higher-ranked one. On any distro that ships `adaptivedemux2` — which is the normal case — plain
`uridecodebin` therefore autoplugs `dashdemux2`, not the legacy element, and will fail on
FLAC-in-DASH below GStreamer 1.26.10 regardless of source-element choice. **The fix is an explicit
startup step, not an implicit consequence of an element choice**: call
`gst_plugin_feature_set_rank(dashdemux2_factory, GST_RANK_NONE)` (or raise `dashdemux`'s rank above
it) — the same mechanism Strawberry already uses to prefer `directsoundsink` over `wasapisink`/
`wasapi2sink` on Windows (`ref:strawberry/src/engine/gststartup.cpp:57-77`) — or hook
`decodebin::autoplug-select` to reject `dashdemux2` explicitly. Add this to Design A's startup
sequence as a named, tested step. **[verified]**

#### 11.3 What a custom ALSA writer buys over plain `alsasink` — the delta, made explicit

§3.1 presents Sone's ~500-line ALSA writer as the way to be bit-perfect on Linux; the Summary and §7
simultaneously note that Strawberry gets exclusive/bit-perfect output from plain
`alsasink device=hw:X,Y` with no custom writer at all. The delta, read directly out of upstream
`gst-plugins-base/ext/alsa/gstalsasink.c`:

**Already handled by `alsasink`, so not a reason to write a custom sink:** format negotiation from
caps (`snd_pcm_hw_params_set_format`); period/buffer time negotiation; and the `sw_params` fix §10.4
and Implication #7 credit to Sone's writer — `alsasink` computes
`start_threshold = (buffer_size / avail_min) * avail_min` and calls `set_avail_min` itself. Implication
#7 is therefore already satisfied by `alsasink`; it is not a reason on its own to avoid it.

**Not handled by `alsasink`, and the real reasons to own a writer:** (1) no rate read-back —
`gstalsasink.c` calls `snd_pcm_hw_params_set_rate_near(handle, params, &rrate, NULL)` and never reads
`rrate` back or compares it against the request, so there is no hook to raise Sone's actionable "DAC
doesn't support 192kHz" error (§3.1, §11 correction there); (2) no per-format/per-rate probing ladder
and therefore no lossless-narrowest-promotion policy; (3) no `set_rate_resample(false)` call at all
(zero occurrences of `resample` in the file) — a `plughw:`/`default` fallback resamples silently
instead of failing explicitly; (4) no `snd_pcm_hw_params_get_sbits()` read-back for reporting the
DAC's true significant bit depth (tidalt does this, `ref:tidalt/internal/player/alsa.c`); (5) no place
to implement tidalt's period-before-buffer ordering for DACs with buggy `period_size_min` queries;
(6) no `snd_pcm_delay()` correction for playback position (§10.8); (7) no in-writer per-format PCM
volume for the exclusive-but-not-bit-perfect case.

**Recommendation:** start Design A's Linux output with `alsasink device=hw:` plus a `capsfilter` and
the `/proc/asound`-reading transparency panel, and treat the custom writer as a second-stage upgrade
justified specifically by items (1), (2) and (6) above — not as unconditional day-one work.
**[verified against upstream `gstalsasink.c` and the Sone/tidalt/Strawberry sources already cited in
this report]**

#### 11.4 Shipping GStreamer on Windows and macOS — the one documented recipe, and what it omits

§2.2 and §8's cost line treat "ship the GStreamer runtime on Windows/macOS" as a known, bounded cost.
One reference client has actually solved it, in detail, and its solution has a real gap the report
should surface. sone-windows's build instructions: install the official GStreamer MSVC x86_64 runtime
+ devel MSIs, copy a hand-picked subset into `src-tauri/gstreamer-runtime/`, and run
`node scripts/prepare-gstreamer.js`, which generates `gstreamer-hooks.nsi` (NSIS) and
`gstreamer-fragment.wxs` (WiX) so both Windows installer formats carry the runtime — roughly 20 MB
total (`ref:sone-windows/README.md:95-155`). The plugin set it bundles is: `gstadaptivedemux2`,
`gstasio`, `gstaudioconvert`, `gstaudioparsers`, `gstaudioresample`, `gstcoreelements`, `gstdash`,
`gstdecklink`, `gstflac`, `gstisomp4`, `gstplayback`, `gstsoup`, `gsttypefindfunctions`, `gstvolume`,
`gstwasapi2`, `gstwinks`, plus `lib/gio/modules/gioopenssl.dll`, documented as "essential for secure
HTTPS connection to TIDAL" — without it `souphttpsrc` cannot do TLS and every stream fails, which
makes that one DLL load-bearing for the whole pipeline. **Two consequences this report did not
previously draw out:** (a) that plugin list contains **no AAC decoder** — no `gst-libav`, no `faad` —
so a shipping Windows build assembled this way can play only the FLAC tiers; `LOW`/`HIGH` are
unplayable until an AAC decoder plugin is deliberately added to the bundle (see §11.20 for the same
problem on Linux distros); (b) both `gstdash.dll` (legacy) and `gstadaptivedemux2.dll` (which contains
`dashdemux2`) are bundled together, which is exactly the rank-collision situation in §11.2 — the
Windows build needs the same rank-demotion startup step, not just Linux.

**No equivalent recipe exists for macOS anywhere in the reference set** — no reference client ships
GStreamer on macOS at all. Design A's macOS packaging (a `GStreamer.framework` or a private dylib
tree, universal arm64+x86_64, notarization of ~30 unsigned dylibs, `GST_PLUGIN_SYSTEM_PATH` set inside
a `.app` bundle) should be costed and scheduled as **unproven, not merely "needs shipping the plugin
set"** as previously written. **[verified for the Windows recipe; the macOS gap is verified as an
absence across all 21 checkouts]**

#### 11.5 Whether bit-perfect output is achievable *through* PipeWire at all — unresolved, a second path alongside raw ALSA `hw:`

**Correction to Implication #17, which previously concluded Flatpak cannot do exclusive mode
because `/dev/snd` is invisible without `--device=all`: that premise is wrong.**
`--socket=pulseaudio` alone already grants `/dev/snd` inside the Flatpak sandbox (Flatpak's own
sandbox helper binds it whenever that socket is requested — see §3.3's corrected text), so raw
ALSA `hw:` is already reachable under Flatpak confinement; do not grey the toggle out on
confinement grounds alone. What this section is actually about is whether a PipeWire-native path
is *also* bit-perfect and therefore a viable alternative to grabbing the raw device directly, via
`--filesystem=xdg-run/pipewire-0`, which High Tide's own manifest already grants
(`ref:high-tide/build-aux/io.github.nokse22.high-tide.json`). **This is not resolvable from the
reference checkouts** — `docs.pipewire.org` is blocked from this research environment, so it could not
be checked in either fact-check pass, and web search returned only forum-level material
(`bbs.archlinux.org/viewtopic.php?id=290859`, "[SOLVED] Get bit-perfect audio with PipeWire"). Before
Implication #17 is written as final, resolve three concrete questions against
`docs.pipewire.org/page_man_pipewire-props_7.html` (device properties `audio.format`, `audio.rate`,
`audio.channels`, `api.alsa.disable-mixer`, `api.alsa.period-size`, `api.alsa.headroom`),
`docs.pipewire.org/page_man_pipewire_conf_5.html` (`default.clock.rate`,
`default.clock.allowed-rates`), and the WirePlumber 0.5 device-config docs: (a) does PipeWire's graph
convert everything to F32 before the sink, and if so does an S24 stream survive bit-exactly (F32's
24-bit mantissa preserves 24-bit integers but not true 32-bit integer) while true S32 does not?
(b) does a stream whose format and rate exactly match the sink node bypass the resampler and volume
stage entirely, and how does a client force that (`node.lock-quantum`, `node.dont-remix`, per-device
`audio.format`/`audio.rate` in WirePlumber)? (c) does `default.clock.allowed-rates` need to be
pre-populated with the full rate set for per-track rate-following to work — i.e. is this a
user/distro config step streamboat must document rather than something the app can request at
runtime? Two facts already in this report bear on the answer and are worth re-reading with this
question in mind: High Tide disables gapless entirely on `pipewiresink` for an unexplained reason
(§4.1(a) — possibly related to exactly this kind of format/rate negotiation); and mpv's `pipewire` AO
accepts `--audio-exclusive=yes` (§3.1/Summary), which — if it means what the flag name says — is a
documented PipeWire exclusive path that Design B may already get for free inside Flatpak. **[gap —
unresolved in both fact-check passes; the specific questions and doc pages to check are the
deliverable here]**

#### 11.6 Idle inhibition on Windows and macOS has no reference implementation

§6's table gives Linux a complete four-layer answer and leaves Windows and macOS blank. No reference
client fills them in: sone-windows's `idle_inhibit.rs` contains only the Linux D-Bus/portal interfaces
and is dead code on the other two platforms (`ref:sone-windows/src-tauri/src/idle_inhibit.rs`), and
Strawberry has no `SetThreadExecutionState`/`IOPMAssertion` calls anywhere in its tree. This must be
written from platform APIs directly: **Windows** —
`SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)` while playing, back to
`SetThreadExecutionState(ES_CONTINUOUS)` on stop — deliberately **without** `ES_DISPLAY_REQUIRED`,
since an audio player should not keep the screen on; the call is per-thread, so it must run from a
thread that lives for the whole playback session. **macOS** —
`IOPMAssertionCreateWithName(kIOPMAssertionTypePreventUserIdleSystemSleep, kIOPMAssertionLevelOn,
CFSTR("streamboat playback"), &assertionID)` (IOKit), released with `IOPMAssertionRelease` on stop.
For the headless/daemon case on Linux, add `org.freedesktop.login1.Manager.Inhibit` with
`what="sleep"`, `mode="block"` (equivalent to `systemd-inhibit`) as a fifth layer — the only one of
Sone's mechanisms that works with no session/display server at all. **[verified as an absence across
sone-windows and Strawberry; the platform API calls are standard practice, not read from a reference
implementation in this pass]**

#### 11.7 SMTC needs a real `HWND` — there is no headless Windows now-playing integration

The report already found the analogous macOS constraint (souvlaki needs an AppDelegate/winit event
loop, §6) but not the Windows one, even though the brief requires headless mode on every platform.
souvlaki's `PlatformConfig` on Windows carries an `hwnd: Option<*mut c_void>` that the SMTC backend
needs populated with a real window handle. sone-windows spawns a task that polls
`app_handle.get_webview_window("main")` every 100 ms for up to 5 seconds before it can build the
config, with the comment "We need to wait for the main window to be created to get HWND"
(`ref:sone-windows/src-tauri/src/media_controls.rs:14-46`). Consequences for streamboat: (1) a
`streamboat-server` Windows service or CLI daemon with no window gets **no SMTC integration at all**
— the Windows analogue of §10.1's "no session bus on headless" rule, and it needs the same "do not
depend on it" treatment; (2) media-control initialization must be sequenced behind window creation on
Windows, never done at app start unconditionally; (3) sone-windows's own handler only wires
Play/Pause/Toggle/Next/Previous/Stop and leaves `Seek`/`SetPosition`/`SetVolume` unimplemented, so its
SMTC seek bar and volume control are inert — a completeness bar streamboat should clear rather than
copy. **[verified]**

#### 11.8 OS media-integration artwork must be a local file or in-memory stream — never a remote URL

§6 covers transport controls and device enumeration but never mentions artwork, which is often the
first thing visibly broken — most desktop shells will not fetch a remote `mpris:artUrl` over the
network, leaving the now-playing popup blank. High Tide caches the 320px cover to disk and hands MPRIS
a local path: `f"file://{utils.IMG_DIR}/{track.album.id}_320.jpg"`
(`ref:high-tide/src/mpris.py:447-449`); Sone carries `art_url` through its MPRIS command enum and only
sets it when non-empty (`ref:sone/src-tauri/src/mpris.rs:234,252-253`). **Consequence for streamboat:
the image cache is on the critical path of OS media integration, not only the UI** — the cover file
must exist on disk *before* the metadata update is emitted, so cover fetch belongs in the
track-transition sequence, prefetched alongside the manifest for the next track. Windows and macOS
need different shapes for the same requirement: SMTC takes a thumbnail via `RandomAccessStreamReference`
(a file or in-memory stream, never a bare URL string) and `MPNowPlayingInfoCenter` takes an
`MPMediaItemArtwork` built from an in-memory image. Fill in the rest of the MPRIS metadata contract
while implementing this: `mpris:trackid` must be a valid D-Bus object path (High Tide uses
`/Track/{id}`), and `mpris:length` is microseconds (`track.duration * 1_000_000`). **[verified]**

#### 11.9 Buffering is three pipeline stages, not one number — with the memory arithmetic that decides Pi viability

Implication #20 previously merged three different pipeline stages under one recommendation ("pin
buffer numbers to TIDAL's own"), citing TIDAL's Android SDK, web SDK and Sone as if they measured the
same thing; they do not, and averaging them is unsafe to act on. Sone's own code comments already
distinguish the stages correctly (§4.5 now labels them): `uridecodebin buffer-duration` (15 s DASH /
5 s BTS) is the **compressed network buffer**; the branch `queue max-size-time` (15 s) is the
**decoded-PCM reservoir** ahead of `concat`; the ALSA `buffer_time`/`period_time` (~500 ms / ~50 ms) is
the **device buffer**. ExoPlayer's headline "2 minutes" figure (§4.5) buffers *extracted/compressed*
samples in `DefaultLoadControl`; its separate 1.5 s `audioTrackBuffer` figure is the PCM device buffer
— the two ExoPlayer numbers are already in different units, and it is a mistake to read "2 minutes" as
a decoded-PCM budget.

**Why this matters concretely for the Pi:** two minutes of *decoded* 24-bit/192kHz stereo PCM packed
into an S32 container is `192000 × 4 bytes × 2 ch × 120 s ≈ 184 MB` (about 138 MB if packed 24-bit);
with two prerolled gapless branches (§4.1(b)) that doubles. Two minutes of the equivalent *compressed*
24/192 FLAC is roughly 70-90 MB. **Neither is a safe default decoded-PCM budget on a 512 MB Pi.**
Sone's actual 15 s decoded reservoir at 24/192 is `192000 × 4 × 2 × 15 ≈ 23 MB` per branch — that is
the number that generalises to Design A's decoded-PCM stage, not ExoPlayer's "2 minutes." **Set each
stage's default independently: compressed-network generous (network is the scarce resource on a Pi,
and TIDAL's web SDK's 40 s figure is a reasonable ceiling there), decoded-PCM conservative (Sone's
15 s / ~23 MB-per-branch), device buffer per §3.1/§10.4 (~500 ms).** **[verified for the cited figures;
the memory arithmetic is computed from them in this pass]**

#### 11.10 Fade-out on stop/pause — and why it cannot be a gain ramp in bit-perfect mode

The report covers crossfade (§4.3) and correctly notes Strawberry gates it off in exclusive mode, but
never mentions the more commonly-needed fade-out-on-stop/fade-on-pause, which suppresses the audible
click/pop of abruptly tearing down a stream — exactly the defect an audiophile user notices first.
Strawberry implements both, gated the same way as crossfade: `fadeout_enabled_`/`fadeout_duration_`
fade the outgoing pipeline on stop/track-change, `fadeout_pause_enabled_`/`fadeout_pause_duration_`
fade on pause, both skipped when `AnyExclusivePipelineActive()`
(`ref:strawberry/src/engine/gstengine.cpp:233-249,336-339,362-389`). Strawberry's own inline comment
states the correctness detail worth copying verbatim: *"If we pause with fadeout, deactivate fadeout
and resume playback, the player would be muted if not faded in"* — the fade-**in** on resume is
mandatory, not cosmetic. **For streamboat's exclusive-mode path the answer cannot be a gain ramp** (any
software gain quantises, exactly like the volume slider, Implication #9): the real options are (a)
accept a click, (b) write a short silence ramp before `snd_pcm_drop()`/closing the device — not
bit-perfect audio content, but inaudible content, which is a defensible trade for a stop/pause
transition specifically, or (c) `snd_pcm_drain()` then close and accept the drain latency. Sone already
writes 50 ms silence buffers around its software pause (§3.1) — mechanism (b) already exists in
embryo; extend it to stop as well as pause. **[verified for Strawberry's implementation; the
bit-perfect-mode options are [inferred]]**

#### 11.11 When to start prefetching the next track — a concrete trigger point, not just "prefetch it"

§4.2 says Sone "prerolls the whole next branch during the current track" but gives no answer to the
first question an implementer needs: at what remaining-time does the next manifest fetch fire, and
what happens if the queue changes after that? Fire too late and gapless fails on a slow connection;
too early and manifests (§4.8, expire in 1 h) and `streamingSessionId`s (§10.14) are wasted.
Strawberry gives a concrete, tunable answer: a 1 Hz timer computes `remaining = length - position` and
emits `AboutToFinish` when `remaining < gap + fudge`, where
`gap = buffer_duration_nanosec_ + (autocrossfade ? fadeout_duration : kPreloadGapNanosec)`,
`kPreloadGapNanosec = 8 s`, and `fudge = kTimerIntervalNanosec + 100 ms`
(`ref:strawberry/src/engine/gstengine.cpp:88-89,595-620`). **Translated for streamboat: preload lead =
network/compressed buffer depth (§11.9) + 8 s, polled at 1 Hz, with a one-poll-interval fudge, latched
so it fires once per track.** Re-run resolution if the queue's next item changes after the latch fires
— a case Strawberry re-emits for and mpv's `--prefetch-playlist` docs explicitly warn "can occasionally
make wrong prefetching decisions" on reorder (already cited under Design B). One more interaction the
original report misses: prefetching a manifest allocates a client-generated `streamingSessionId`
(§10.14) that must be explicitly discarded, not silently orphaned, if the prefetch is abandoned by a
reorder or skip. **[verified for Strawberry's formula; the streamboat translation is [inferred]]**

#### 11.12 Gapless on the AAC tiers is a different, harder, and currently unsolved problem

§4.1's whole gapless discussion implicitly assumes FLAC. FLAC has no encoder padding, so concatenating
decoded output is exact. AAC does: every AAC-LC frame set starts with priming samples (conventionally
2112 for AAC-LC, more for HE-AAC/SBR) and ends with trailing padding, signalled in fMP4 by an `elst`
edit-list box and/or an iTunes `gapless` atom. Concatenating two AAC tracks without honouring that
inserts tens of milliseconds of silence plus an audible click at every boundary — gapless silently
does not work on the `LOW`/`HIGH` tiers even though the code path looks identical to the FLAC case.
**No reference client in the entire set handles this** — a search across all 21 checkouts for
edit-list/priming/encoder-delay handling finds nothing, and every gapless implementation found (Sone's
`concat`, High Tide's `about-to-finish`, Strawberry's `SetNextUrl`, TIDAL's own 250 ms Shaka
micro-crossfade) delegates any trimming to the demuxer or does not trim at all. Practical positions,
none of them verified end-to-end: on GStreamer, `qtdemux` + `aacparse` are expected to apply `elst`
trimming, so Sone's `concat` path is *probably* correct on AAC too, but this must be tested on a real
AAC album, not assumed; on libmpv, FFmpeg's `mov` demuxer applies edit lists and
`--gapless-audio=weak` keeps the device open, so Design B is *likely* fine; on Design C, Symphonia's
ISO/MP4 gapless support is documented as "No" (already cited in §2.2), so hand-rolled AAC gapless is
out of reach there — one more argument for a lossless-only Design C variant (Open questions #2).
**Scope streamboat's gapless guarantee to the FLAC tiers explicitly; describe AAC-tier gapless as
best-effort, and verify it per-engine before claiming it.** **[verified as an absence across the
reference set; the FFmpeg/qtdemux "probably correct" claims are [uncertain] and need a real test]**

#### 11.13 Play reporting to TIDAL, in full — the endpoint, the threshold, and a real constraint on client identity

§4.8 mentions play reporting only as an owner decision without the shape needed to actually decide it.
Sone implements it completely: `POST https://ec.tidal.com/api/event-batch`, batched at `MAX_BATCH = 10`
("Max events per SQS SendMessageBatch"), one `playback_session` event per qualifying play
(`ref:sone/src-tauri/src/tidal_report/event.rs:6-51`). **Threshold: a flat 30 seconds regardless of
track length** — the code comment states this is "TIDAL's own rule: a play over 30 seconds counts as
a stream," with unit tests asserting 30 s of a 200 s track counts and 25 s of a 25 s track does not.
**Attribution:** `SourceType::{Album, Playlist, Artist, Mix}` mapped from the container the play
started in; unmapped sources (favourites, search, home sections) are reported sourceless.
**Identity:** account identity comes from decoding the access token's JWT middle segment for
`uid`/`cid`/`sid`, with no signature verification. **The constraint that actually shapes the auth
layer:** "Events ride on that client's token, so they must describe that client — not streamboat" —
Sone pins `TIDAL_APP_VERSION = "2.205.0"`, `OS_NAME = "Android"`, `OS_VERSION = "35"`,
`DEVICE_MODEL = "Pixel 7"`, `DEVICE_VENDOR = "Google"` to match the client ID it authenticates with,
noting "TIDAL ships roughly weekly, so this pin drifts; bump it occasionally"
(`ref:sone/src-tauri/src/tidal_report/mod.rs:1-6,114-116,260-262,567-573`). It ships this on by
default with a settings toggle, describing the endpoint as "private, undocumented — best-effort, may
not surface." **This device-impersonation requirement is a real maintenance tax and an ethical/ToS
consideration the owner should weigh explicitly (Open questions #5), not a one-time implementation
detail.** **[verified]**

#### 11.14 CDN segment fetches carry no authentication — silently assumed everywhere, stated nowhere

Every design sketch in this report (§9 Designs A/B/C) silently depends on being able to hand a
manifest/segment URL to `souphttpsrc`, libmpv's `loadfile`, or a bare `fetch`, with no bearer token or
header injection — but the report never states this as a fact to rely on. Confirmed: TIDAL's CDN URLs
are pre-signed and take no `Authorization` header. tidal-cli fetches both the DASH init segment and
every media segment with a bare `await fetch(url)` and no request-init object at all
(`ref:tidal-cli/src/playback.ts:156-170`); mopidy-tidal's relay proxy forwards to the CDN with no auth
added; High Tide passes BTS URLs straight to `requests` and to `ffmpeg` unmodified. **Corollaries worth
stating explicitly:** (a) the expiry lives in the URL's own query token, which is why §10.10's
mid-track 403 recovery means re-fetching the manifest, not refreshing the OAuth token; (b) any HTTP
client works, so a mopidy-tidal-style localhost relay proxy is a legitimate way to insert caching,
Range handling and retry *underneath* an engine that offers no hook — the cleanest answer to §10.10's
open problem for Design B, where you cannot reach inside libmpv's demuxer; (c) a working TLS stack in
the plugin/runtime set is therefore load-bearing on every platform — see §11.4's `gioopenssl.dll`
finding for Windows specifically. **[verified]**

#### 11.15 Device hot-plug and removal on Windows and macOS while a device is held exclusively

§6's "Hot-plug" paragraph and §10.13 cover Linux (`DeviceMonitor`, `ENODEV`) and macOS nominal-rate
*changes*, but nothing about Windows, and nothing about device *removal* on either platform while
exclusive. Unplugging a USB DAC mid-track is routine and, in exclusive mode, is a hard failure that
must become a clean, recoverable state rather than a crash. **Not covered by any reference client**
(Sone's `ENODEV` teardown is the only handling anywhere in the set), so this must be written from
platform APIs: **Windows** — register an `IMMNotificationClient` on `IMMDeviceEnumerator` for
`OnDeviceStateChanged`, `OnDeviceRemoved`, `OnDeviceAdded`, `OnDefaultDeviceChanged`; a removed or
invalidated endpoint surfaces on the render client as `AUDCLNT_E_DEVICE_INVALIDATED` from
`GetBuffer`/`ReleaseBuffer` — not in §10.6's error list, and it needs a full stop-release-reacquire
cycle rather than a retry. **macOS** — add a property listener on `kAudioHardwarePropertyDevices`
(add/remove) and on `kAudioObjectPropertyDeviceIsAlive` / `kAudioHardwarePropertyDefaultOutputDevice`
for the held device; hog mode must be released explicitly on device loss or it can leak, and
CamillaDSP's warning that hog mode breaks virtual devices like BlackHole (§3.5) applies to anyone
routing through Loopback/Soundflower too. Tie this to §10.13: the reconnect path must resolve the
*persisted stable device id* (ALSA card id, WASAPI endpoint id, macOS device UID) and re-open only if
it is the same device — never silently fall back to `default`. **[verified as an absence across
sone-windows and Strawberry; the platform API shape is standard practice, not read from a reference
implementation]**

#### 11.16 Time-to-first-audio is a composed budget — and the exclusive-mode path can exceed 2 seconds if built naively

"Press play, hear music" is the most-felt performance characteristic of a player, and the exclusive/
bit-perfect path adds several genuinely serial steps this report specifies individually but never
composes. Summing the report's own figures: manifest resolution is 1 RTT on the v2 path but up to four
sequential requests on the v1 quality cascade (§1.8) — a strong practical argument for preferring v2
(Implication #2) beyond just request count; tidalt's `ReserveDevice1` handshake budget is
`releaseCallTimeout 500 ms + releaseSettleDelay 200 ms + openBusyRetryBudget 800 ms = 1.5 s` (§3.2);
then the ALSA/WASAPI/CoreAudio device open, which on macOS includes an asynchronous
`kAudioDevicePropertyNominalSampleRate` change that must be waited on (§10.7); then CDN connect plus
init segment plus at least one media segment; then `start_threshold` = the full ~500 ms ALSA buffer
before the device starts producing sound (§3.1). Serially, that composes to comfortably 2-3 seconds.
**The fix is architectural, not a single optimization:** device reservation and open depend only on
the *chosen device*, not on the track, so they can run **concurrently** with manifest resolution — even
started at app launch or on device selection, well before "play" is pressed. `start_threshold` can be
lowered for the very first buffer of a session and raised back to the steady-state value after. Show
`Resolving` and `Reserving`/`Open(fmt)` as genuinely parallel states in §9's device state machine — the
report already calls the device states "orthogonal" but never uses that to overlap latency in the
design. **Add an explicit, testable budget to the spec** (e.g. under 500 ms warm, under 1.5 s cold).
**[composed from figures already cited elsewhere in this report; the parallelization recommendation is
[inferred]]**

#### 11.17 Headless/Pi feasibility has never been measured — CPU, memory, and the HDMI hi-res ceiling

The brief names "headless (Raspberry Pi class)" as a first-class platform; §10.1 answers the
*architecture* question well but never asks whether the hardware can actually do the job, and no
reference project publishes benchmarks. This decides whether Design A (GStreamer + two decode
branches) or Design C (pure Rust, smallest footprint) is the right headless build, and whether the
gapless second branch (§11.9's ~23 MB-per-branch figure) is affordable at all on a Pi Zero 2 W or
Pi 3. **Name this as a required measurement, not an assumption:** decode 24/192 stereo FLAC to
`/dev/null` with each engine candidate (`gst-launch-1.0 filesrc ! flacparse ! flacdec ! fakesink`,
`mpv --ao=null --untimed`, a Symphonia decode loop) on a Pi 3B+, Pi 4, and Pi 5, reporting single-core
utilization and peak RSS, with the second decode branch running concurrently to model gapless. One
bounding fact from this report's own §10.1 makes the question partly moot for the most common headless
setup: Pi HDMI output (`vc4hdmi`) had to be limited to 16-bit/44.1kHz for hi-res content to work at
all (`ref:tidal-connect/userconfig/README.md`) — on a Pi's own HDMI output, the hi-res-decode question
is secondary to the narrowing-conversion-with-dither path (§10.12), which is the one actually
exercised in that configuration. **[gap — no benchmarks exist in the reference set; the measurement
shape and the HDMI bounding fact are the deliverable here]**

#### 11.18 High Tide's single shared MPD path is unsafe for the prefetch/gapless design this report recommends

§1.3 cites High Tide's `file://` MPD strategy approvingly as one of three ways to feed DASH to a
player, and Design B repeats it as a fallback, without noting that the implementation is not safe
under prefetch. High Tide writes every track's MPD to one fixed path,
`Path(utils.CACHE_DIR, "manifest.mpd")`, opened and overwritten on every track
(`ref:high-tide/src/lib/player_object.py:494-513`). With any prefetch of the next track — which High
Tide's own `about-to-finish` gapless performs — the next track's MPD can overwrite the current one
while the demuxer may still need it (a static MPD can be re-read on seek; `adaptivedemux2` keeps a
manifest URI for potential refresh). **streamboat must use a unique temp file per streaming session,
delete it on track teardown, and place it somewhere writable inside a Flatpak/Snap sandbox**
(`XDG_RUNTIME_DIR`, not the shared cache dir). This also sharpens the report's standing open question
about why High Tide moved from `data:` to `file://` at GStreamer 1.26 (§1.4): a testable hypothesis is
that `dashdemux2` resolves and re-fetches the manifest URI in a way a `data:` URI cannot satisfy —
checkable in one command, `gst-launch-1.0 uridecodebin uri="data:application/dash+xml;base64,…" !
fakesink` against 1.26 and 1.28, with and without `dashdemux2` demoted per §11.2, under
`GST_DEBUG=dashdemux*:5`. **[verified for the shared-path bug; the `dashdemux2` re-fetch hypothesis is
[inferred] and stated as a hypothesis, not a fact]**

#### 11.19 Bundle a pinned GStreamer everywhere, or link the system one on Linux — three options, laid out

Owner decision #12 above poses this; here are the three options and their consequences, all supported
by facts already in this report: **(1) system GStreamer on Linux, bundled elsewhere** — cheapest Linux
packaging, forces the §11.2 rank-demotion workaround, and gives Linux the *worst* FLAC-in-DASH story
of the three desktop platforms, since Windows/macOS bundling (§11.4) meets the 1.26.10/1.28 floors for
free. **(2) bundled GStreamer everywhere** (Flatpak with a pinned `org.freedesktop.Platform` +
GStreamer module, or an AppImage) — uniform behaviour, meets both version floors, at the cost of
~20 MB per platform (§11.4's figure) and of owning security updates for a media stack.
**(3) libmpv (Design B)**, where the whole question collapses: the FFmpeg-based DASH path has no
1.26.10-equivalent floor and `--audio-exclusive` needs no 1.28-equivalent floor — an argument for
Design B this report's own §9 recommendation section does not currently make. Flatpak makes option
(2) genuinely cheap on Linux and is the same channel that §11.5's PipeWire question says may keep
bit-perfect alive under confinement — the two decisions should be made together, not separately.
**[inferred from facts already verified elsewhere in this report]**

#### 11.20 AAC decoder availability is a per-distribution, per-platform problem, not solved by "rely on distro codecs"

§2.3's mitigation (a), "rely on system/distro codecs (`gstreamer1.0-libav`, the OS decoder)," reads as
though this is a solved problem. On the two most common Linux targets it is not, and on Windows (per
§11.4) it is currently absent entirely. Sone's own install instructions expose the Linux split:
Debian/Ubuntu gets `gstreamer1.0-libav` from `main`; **Fedora needs `gstreamer1-plugin-libav`, which
lives in RPM Fusion (free), not in Fedora's default repositories** — a stock Fedora install of
streamboat has no AAC decoder unless the user has RPM Fusion enabled; Arch gets `gst-libav`
(`ref:sone/README.md:283,306,345,401`). Sone's own Snap build has to stage
`usr/lib/x86_64-linux-gnu/libfaad*` explicitly and recreate `update-alternatives` symlinks for
`libblas`/`liblapack` "so `libgstlibav` (ffmpeg) can load them" — even the confined build needs
deliberate, non-obvious work (`ref:sone/snap/snapcraft.yaml:40,127-129,153`). **What streamboat needs
and does not currently have specified:** a startup capability probe
(`gst::ElementFactory::find("avdec_aac")`, or a decodebin dry-run) that reports which tiers are
actually playable on the running installation, surfaced in the same transparency panel as the signal
path (§3.2), with a quality-picker that greys out unreachable tiers instead of failing at play time —
the same pattern Implication #17 already prescribes for the exclusive-mode toggle under sandbox
confinement. **[verified]**

#### 11.21 Capture real TIDAL manifest fixtures — make it the first engineering task, not a later one

No captured TIDAL MPD or BTS response exists anywhere in the 21 reference checkouts, so every DASH
structural claim in this report — the single-Period/AdaptationSet/Representation shape, the literal
`AdaptationSet@mimeType="audio/mp4"` value (§1.4), the absence of `startNumber`, the absence of
`SegmentBase`/`indexRange`, `Representation@id="FLAC,44100,16"` — is parser-derived, not observed. A
`grep` across all 21 checkouts finds no `.mpd` fixture and no golden-manifest test anywhere, which is
exactly why python-tidal and tidal-cli can disagree about segment numbering (§1.4) with neither being
demonstrably wrong. **Make manifest-fixture capture the first engineering task, ahead of any parser
code:** on first successful login, dump one BTS and one DASH manifest per tier (`LOW`/`HIGH`/
`LOSSLESS`/`HI_RES_LOSSLESS`) plus one Dolby Atmos and one `PREVIEW`-asset response to
`tests/fixtures/`, redact the CDN tokens, and drive every parser test off them from day one. That one
step converts §1.4's `startNumber`/`@r`/`mimeType` open questions from open to closed and gives CI
something concrete to fail on when TIDAL changes its manifest shape. **[gap — no fixture exists
anywhere in the reference set; the fixture-capture plan is the deliverable]**

#### 11.22 HLS and EMU manifests need an explicit, decided behaviour — not silent failure

§1.3 establishes all four manifest MIME types, but every implementation sketch in §9 (Designs A/B/C)
only handles BTS and DASH. python-tidal has EMU and APPL (HLS) *commented out* of its
`ManifestMimeType` enum and raises `UnknownManifestFormat` otherwise
(`ref:python-tidal/tidalapi/media.py:112-120`) — i.e. the most-used unofficial client hard-fails on
HLS today. Since manifest type is DRM/client-driven (§1.2 — FairPlay support asks for HLS, else DASH)
and the client-ID choice is an open owner decision (#8), a credential that makes TIDAL answer with
`application/vnd.apple.mpegurl` would otherwise be a total, undesigned playback failure. **Decide it
explicitly:** EMU parses with the same code path as BTS — TIDAL's own web SDK's manifest parser
handles both with the same `parseJSONManifest` function (`ref:tidal-sdk-web/.../manifest-parser.ts`)
— so support it for free by construction, not as separate work. For HLS, either implement the web
SDK's double-base64 variant decode, or fail with a specific, actionable error naming the client ID in
use as the likely cause — never a generic "unsupported manifest" message. **[verified for the
python-tidal/web-SDK behaviour; the decision itself is this report's recommendation, not an observed
fact]**

#### 11.23 PREVIEW / previewReason needs a defined, required behaviour — not just a flagged unknown

A prior pass of this report only listed `assetpresentation=FULL` vs `PREVIEW` handling as an open
question. It should be a specified requirement instead: both v2 SDKs surface this as first-class.
`ref:tidal-sdk-android/.../PlaybackInfoRepositoryDefault.kt` maps `TrackPresentation.PREVIEW` and
`PreviewReason.{SUBSCRIPTION, PURCHASE, HIGHER_ACCESS_TIER}`, and the web SDK defaults
`assetPresentation` to `'PREVIEW'` when the attribute is absent from a v2 response — the **opposite**
default from v1, which defaults to `FULL`. A player that ignores this silently plays a 30-second clip
as though it were the full track and, per §4.8/§11.13, would report it to TIDAL as a full play — worse
than an error. **Required behaviour: treat `assetPresentation != FULL` as a distinct playback outcome**
with its own UI state (not a generic error) and its own play-reporting suppression (a preview must
never cross §11.13's 30-second-play threshold as if it were the real track), and **surface
`previewReason` verbatim** (`SUBSCRIPTION` / `PURCHASE` / `HIGHER_ACCESS_TIER`) since each implies a
different corrective action for the user (upgrade plan, buy the track, raise quality tier). What
remains genuinely open is only the exact UI copy per `previewReason` value. **[verified]**

#### 11.24 The streaming-privileges WebSocket needs an explicit reconnect policy, not just the message vocabulary

§4.8/Implication #15 say to subscribe to `POST /v1/rt/connect`, but never state what the client does
on socket close, on a `RECONNECT` message, or when `USER_ACTION` should be sent. Getting this wrong is
maximally user-visible: either streamboat silently loses the stream to another device with no
message, or it fights another device for the privilege in a loop. `pushkin.ts` has the reconnect and
`readyState` handling; it was previously cited only for the message vocabulary. **Read it in full
before implementing, and pin down three rules:** `USER_ACTION` is sent only on an explicit user-
initiated play, never on autoplay or a gapless track advance; on `RECONNECT`, re-fetch the WebSocket
URL from `POST /v1/rt/connect` rather than reusing the old one — the URL is issued per-connect and is
not guaranteed stable; and a dropped socket must never, by itself, stop playback — it degrades to "we
cannot confirm we still hold the privilege," not "stop." **[gap — the rules are recommendations
derived from the message vocabulary already cited plus standard WebSocket-client practice, not
independently re-read from `pushkin.ts`'s reconnect implementation in this pass]**

#### 11.25 `countryCode`'s provenance, and why it changes how you read a 4032/4035 error

§1.2's canonical v1 URL includes `countryCode`, and Sone passes it explicitly
(`ref:sone/src-tauri/src/tidal_api.rs:3665`), but python-tidal's `get_stream` sends only
`playbackmode`/`audioquality`/`assetpresentation` — `countryCode` is injected by its request layer
from the session, not passed per-call. A client that omits it, sends a stale value, or derives it from
OS locale instead of the account's actual country gets region-limited results that surface as
sub-status 4032/4035 (`PEContentNotAvailableInLocation`), which Implication #14 says to treat as
**terminal** and skip the track. A misconfigured `countryCode` would therefore present identically to
"this whole catalogue is region-locked" — a client bug masquerading as a licensing wall. **Specify
`countryCode`'s provenance as the session/user profile (never OS/browser locale), inject it once in
the request layer rather than per call site, and add a diagnostic that distinguishes "this specific
track is genuinely region-locked" from "our `countryCode` is wrong"** before treating 4032/4035 as
unconditionally terminal. **[inferred from the v1/python-tidal parameter-shape facts already verified
in §1.2]**

---

### 12. Findings added by a third independent fact-check

Same convention as §10/§11: corrections to what came before, plus real gaps neither prior pass closed.

#### 12.1 Gapless + ReplayGain: a single post-`concat` volume element cannot change gain at a sample-accurate boundary

§4.4 presents Sone's "gain applied before `play_url`" as the solved pattern to copy, but that is true
only for a cold start. On the gapless path (§4.1(b)), Sone applies the *next* track's gain from the
`concat.connect_notify("active-pad")` handler onto a single `volume` element (`norm_vol`) that sits
**downstream of `concat`** in the shared chain `concat → audioconvert → audioresample → norm_vol →
user_vol → sink` (`ref:sone/src-tauri/src/audio.rs:1833-1863`, gain application at `:2656-2668` with
the comment *"concat is upstream of norm_vol, so the gain applies to the now-active branch"*).
`active-pad` fires once the last buffer of track A has passed `concat` — not once it has been heard.
Whatever is still queued in `audioconvert`/`audioresample`/the sink's own ring buffer at that instant
is track A's tail, and it gets track B's gain applied to it: an audible level jump on the wrong side of
the track boundary, on every gapless transition where the two tracks' ReplayGain values differ. The
same class of bug exists in High Tide's design (`taginject`/`rgvolume` swapped on `about-to-finish`).
**Fix for streamboat:** put a per-branch `volume` element *inside* each decode branch, upstream of
`concat` — gain then travels with its own buffers and switches exactly when `concat` switches — rather
than one shared gain element downstream. (An alternative is a `GstControlBinding`/timed value keyed to
the boundary's running time, but the per-branch element is simpler and matches the two-branch
`concat` topology already in use.) This interacts with §4.1(b)'s already-documented `track-finished`
early-fire bug: both are consequences of doing per-track work on a shared element downstream of the
mixing point instead of inside the per-track branch. **[uncertain — read directly from Sone's source;
the defect is a straightforward consequence of the documented topology, but not exercised against a
real ReplayGain-differing track pair in this pass]**

#### 12.2 DASH `SegmentTemplate` substitution has more identifiers than any reference client implements

§1.4 tells streamboat to "implement the spec, not either reference implementation" for `$Number$`
numbering, but understates how much more of the spec surface a real assembler needs. Per ISO/IEC
23009-1 §5.3.9.4.4, `SegmentTemplate` URLs may use `$$` (a literal `$`), `$RepresentationID$`,
`$Number$`, `$Bandwidth$`, and `$Time$` — each optionally with a printf-style width tag, e.g.
`$Number%05d$` or `$Time%011d$`. `$Time$` is used *instead of* `$Number$` when driven by a
`SegmentTimeline`, and its value is the segment's `S@t` (start time in timescale units), not a
sequence index — a different substitution rule from `$Number$`, not a formatting variant of it. **No
reference client handles any of this**: a grep for `RepresentationID|\$Bandwidth\$|\$Time\$|%0[0-9]d`
across every checkout in the reference set returns zero hits in manifest code, and both reference
parsers do a single literal `.replace('$Number$', …)`
(`ref:python-tidal/tidalapi/media.py:833-845`, `ref:tidal-cli/src/playback.ts:165`). Two more gaps in
the same area: `SegmentTemplate` may be declared at `Period` or `AdaptationSet` level and inherited by
`Representation`s — python-tidal only ever reads
`representations[0].segment_templates[0]` and would silently produce nothing on an MPD that puts the
template one level up; and `SegmentTemplate@startNumber` exists in python-tidal's parsed fields but is
**deliberately commented out** at the call site (`media.py:797`) rather than merely unread — the
author saw it and chose to ignore it, which is a stronger signal than an oversight. **Recommendation:
use a real XML parser with full identifier, format-tag and inheritance handling (`dash-mpd` for Rust,
or a hand-rolled reader over `quick-xml`/`roxmltree`), never a regex, and validate it against the
§11.21 captured fixtures before trusting it on a live account.** **[verified for the reference-client
absence and the ISO spec identifier set; whether TIDAL's own MPDs ever use `$Time$`/`$Bandwidth$` is
unknown and is exactly what §11.21's fixture capture would settle]**

#### 12.3 Seek is specified as a policy, not as arithmetic — and duration has no single authoritative source

§4.7 says "the segment for any timestamp is computable offline" and stops there; an implementer needs
the actual mapping and the precision it delivers, since seek is one of the five operations any engine
trait must expose (§9). The mapping: for segment index `k`, start time
`t_k` = the previous segment's end (or `SegmentTemplate@presentationTimeOffset`, default 0, for `k=0`)
and `$Number$` = `startNumber + k` (default `startNumber` = 1, per §1.4/§12.2); segment `k` covers
`[t_k, t_k + d)` in timescale units. **None of `@t`, `@presentationTimeOffset`, or `@startNumber` is
read by any reference client** (same grep as §12.2). **Precision:** this mapping gets a seek to a
segment boundary, plus the fMP4 init segment — sample-accurate seeking additionally requires decoding
and discarding audio from the segment start to the requested position; the report does not currently
say this anywhere, so a hand-rolled Design C seek would silently be coarse to the segment length
(commonly several seconds) rather than sample-accurate. **The reference clients disagree on seek
accuracy and the report has not picked a side:** Sone and High Tide seek with
`FLUSH | KEY_UNIT` — i.e. accept snapping to a keyframe/segment boundary
(`ref:sone/src-tauri/src/audio.rs:2330,2353`; `ref:high-tide/src/lib/player_object.py:815-816`) —
while Strawberry uses `FLUSH` alone, i.e. accurate seeking
(`ref:strawberry/src/engine/gstenginepipeline.cpp:2370`). Pick one for streamboat and say why (FLAC
frames are short, so `KEY_UNIT` snapping is likely inaudible on the lossless tiers and much simpler to
implement — but state that as a decision, not an accident). **After any seek in a hand-rolled
assembler, the init segment must be re-fed before the new media segment** — §9 Design C mentions
re-feeding in passing; state it as a hard requirement, not an implementation detail. **Duration is also
ambiguous and consumed in at least three places that need it to agree:** the track object's integer
`duration` (seconds), the MPD's `MPD@mediaPresentationDuration` (e.g. `PT2M26.47S`), and the decoded
sample count can all disagree by up to a second; §11.11's prefetch trigger (`remaining = length -
position`), MPRIS `mpris:length` (§11.8), and §11.13's flat 30-second play threshold all depend on
picking one authoritative source and using it consistently. **Recommendation: use the track object's
`duration` as authoritative for UI/reporting (it is what TIDAL itself reports back against), and the
MPD's own duration only to detect a manifest that disagrees enough to be suspicious.** **[verified for
the seek-flag disagreement between Sone/High Tide and Strawberry; the rest is spec arithmetic and
inference, not independently re-observed against a live manifest]**

#### 12.4 libmpv's own distribution story was never examined — and the recommended engine turns out to foreclose the App Store

§9 recommends libmpv (Design B) as the primary engine, but never asks how `libmpv-2.dll`/`libmpv.dylib`
is actually obtained and shipped on Windows/macOS — the exact question §11.4 asks (and answers, at
length) for GStreamer. **No project in the entire reference set links libmpv at all**: a search for
`libmpv|mpv_` across every checkout, excluding vendored directories, returns nothing; the file named
`ref:tidalt/internal/player/mpv.go` is misleadingly named — its actual content is `#cgo LDFLAGS:
-lasound` plus tidalt's ALSA/D-Bus-reservation code, not any mpv usage. **So Design B's runtime is, in
this reference set, exactly as unproven as Design A's macOS packaging — the report should say so with
the same candour it applies to GStreamer.** Concretely unanswered: the libmpv version floor implied by
the options Design B already depends on (`--volume-gain`, `--prefetch-playlist`,
`coreaudio_exclusive`, `--wasapi-exclusive-buffer`) against what Debian 13/Raspberry Pi OS actually
ship as `libmpv2` — the same arithmetic §1.4 does for GStreamer 1.26.2 vs the 1.26.10 floor, not yet
done for mpv; how `libmpv-2.dll` is obtained on Windows (mpv publishes no official libmpv build; the
ecosystem in practice uses third-party build artefacts such as shinchiro's mpv-winbuild) and
`libmpv.dylib` on macOS (Homebrew's mpv, or an owned build); and that an LGPL libmpv (`-Dgpl=false`,
§2.2) also needs an **LGPL-built FFmpeg underneath it**, so a distro's default libmpv cannot be relied
on for the LGPL escape hatch — a distro build is very likely linked against a GPL FFmpeg.

**The licence consequence this report has not drawn out anywhere: linking libmpv (GPLv2+) makes
streamboat a GPL work, and GPL is incompatible with Apple's App Store terms.** The owner's constraint
is that mobile must not be architecturally precluded (project context); §9's recommended engine
forecloses iOS distribution specifically, and §8's "mobile path" column for Design B ("mpv builds for
Android; iOS is awkward") understates this — "awkward" reads as an engineering problem, but the actual
blocker is a licence/store-policy conflict that no amount of engineering effort removes. **[verified
for the absence of libmpv usage across the reference set (a directly run search, not an inference) and
for the LGPL-FFmpeg dependency stated in mpv's own `Copyright` file (§2.2); the GPL/App-Store
consequence is standard licensing knowledge, not read from a reference client]**

#### 12.5 Owner-facing conflict: this report and `tech-stack.md` recommend opposite primary audio engines

This report's §9 "Recommendation" says: *"Start with **Design B (libmpv)** … Then, if and when the
signal path or the macOS behaviour proves inadequate, add **Design A** as a second engine
implementation."* `docs/research/tech-stack.md` recommends the reverse: `| Audio engine | `gstreamer`
0.25 + `gstreamer-app`, behind an `AudioEngine` trait; libmpv as backend #2 |` (tech-stack.md:1206),
and separately: `| Exclusive output | Linux: `alsa` 0.10 + appsink writer thread. Windows:
`wasapi2sink exclusive=true`. macOS: unresolved — evaluate `coreaudio_exclusive` via libmpv |`
(tech-stack.md:1207). The two documents also disagree on mpv's own gapless flag: this report says
`--gapless-audio=weak`, never `yes` (§4.1(d), confirmed correct against mpv's documented semantics by
this fact-check); `tech-stack.md:225` states gapless "must be **disabled** for strict bit-perfect
(`--gapless-audio=no`)" — `no` is unnecessarily strict (it closes the device between every track,
forfeiting exclusive-mode gapless, Implication #12, for no bit-perfection benefit `weak` does not
already provide) and contradicts this report's own recommendation. **An owner reading both documents
gets two different day-one architectures with no signal that they disagree.** Engine choice drives
packaging, licensing (§12.4), macOS scope and headless-binary shape — it is the single most expensive
decision to reverse later. **Resolve this explicitly** (see the new owner decision in Open questions
below) using the facts already assembled across both documents: §11.19's packaging-collapse argument
for libmpv, §11.4's "GStreamer-on-macOS is unproven" finding, §12.4's "libmpv-on-anything is equally
unproven, and it forecloses iOS" finding, and note which document is authoritative on this question so
it is decided once, not twice. **[verified — the conflicting text is quoted directly from both
documents]**

#### 12.6 No DAC warm-up / re-lock settle delay anywhere in the reference set — the first audible defect of per-track rate switching

The report's exclusive-mode design closes and reopens the PCM device on every format/rate change
(§3.6) and Implication #12 proposes exclusive-mode gapless across same-format tracks — meaning a
hi-res album that mixes 44.1 kHz and 96 kHz tracks exercises a rate-change reopen routinely, not as an
edge case. Many USB/I2S DACs mute their analogue output for tens to a few hundred milliseconds while
the receiver's PLL re-locks to the new clock; writing real audio immediately after
`snd_pcm_prepare()`/`IAudioClient::Start()` at the new rate loses the first fraction of a second of the
track. **No reference client in this set implements any settle/warm-up delay around a device open or
reconfigure.** A search for sleep/delay calls around device open in
`ref:sone/src-tauri/src/audio.rs`, `ref:tidalt/internal/player/alsa.c`, and
`ref:tidalt/internal/player/mpv.go` finds only unrelated pacing: Sone's 10 ms XRUN-retry pause, its
50 ms software-pause silence cadence, a 10 ms poll loop, and the 100 ms `DeviceMonitor` poll (§6); the
only thing in the set labelled "settle" is tidalt's `releaseSettleDelay = 200 * time.Millisecond`
(§3.2), which is the D-Bus device-reservation hand-off waiting for a previous owner to close its
handle — a different problem from PLL lock. **Specify for streamboat:** after every device
open/format-change (`snd_pcm_prepare`, `IAudioClient::Start`, a CoreAudio physical/nominal-rate
change), pre-roll a configurable silence period (default on the order of 200-300 ms, overridable per
device) before writing the first real audio frame, and count it inside the time-to-first-audio budget
(§11.16). This also argues against attempting exclusive-mode gapless *across* a rate change
specifically — a rate change is, by this reasoning, inherently a small gap, so the queue should show
it as one rather than pretend it is seamless. **[verified as an absence across the cited files; the
concrete settle-delay recommendation is standard DAC-driver practice, not read from a reference
client]**

#### 12.7 "Bit-perfect" needs an explicit, stated rule for the lossy tiers — it is currently defined only for FLAC

§3 and its "bit-perfect" vocabulary (Implications #5, #9, §10.2) are written as though every stream
were FLAC; nothing in the report says what "bit-perfect" means, or what the exclusive-mode/transparency
panel should say, when the user plays a `LOW`/`HIGH` (AAC) track. **State the rule: bit-perfection is
only a meaningful claim for the lossless tiers**, because a lossy decode has no canonical output word
length in the first place — an AAC decoder emits float or a chosen integer depth by its own internal
convention, so there is no "original bits" on the wire to preserve end to end. Consequences to specify
now, not discover later: (a) for `LOW`/`HIGH`, streamboat must still pick a deliberate output format
(e.g. widen to the device's native format) and the transparency panel (§3.2) should say "lossy source —
bit-perfect not applicable," never claim bit-perfection it cannot define; (b) §3.1's
lossless-integer-widening promotion ladder (`S16→S24→S24_32→S32`) is a lossless-source ladder and needs
a distinct branch, or an explicit non-answer, for AAC decode output; (c) §10.2's rule ("bit-perfect
implies no user volume, no ReplayGain") should not apply globally to a mode toggle that also silently
covers AAC tracks — a user who enables bit-perfect gets no volume control on `LOW`/`HIGH` tracks for a
guarantee that was never actually available there, so the mode should be scoped per-track by source
type (lossless vs lossy), not as one global switch; (d) the *source* bit depth for the transparency
panel must come from the manifest (`Representation@id`, §1.4) or the FLAC `STREAMINFO` block, never
from decoder/GStreamer output caps — those describe the container word width after decode, not the
original source depth, and conflating the two is exactly the kind of inaccuracy that would make the
panel untrustworthy. **[inferred from §3.1's `pick_capsfilter_format` being lossless-only
(`ref:sone/src-tauri/src/audio.rs:516-544`) and §10.2's existing rule; no reference client states an
explicit lossy-tier bit-perfect policy]**

#### 12.8 `org.freedesktop.ReserveDevice1` can succeed vacuously — the handshake has a silent-failure mode Implication #4 does not name

Implication #4 makes the D-Bus reservation handshake (§3.2) mandatory, and §3.2 documents tidalt's
three explicit outcomes in detail. What is missing is the most common real-world failure: **the
protocol has no guaranteed server side.** `RequestName` can return `PrimaryOwner` simply because
nothing on the session bus implements `ReserveDevice1` for that device — no client, no `pactl` module,
no WirePlumber component — in which case the "reservation" is a name registration with nobody
listening, PipeWire/the ALSA-holding app never releases anything, and the subsequent `hw:` open still
fails with the exact `EBUSY` the whole handshake exists to prevent. **Fold this into the implementation
as an explicit rule: treat "reservation acquired" as advisory, not as a guarantee, and always still
handle `EBUSY` on open with tidalt's own retry budget** (`openBusyRetryBudget 800ms`, retried every
100 ms, already cited in §3.2) — the two mechanisms are complementary, not either/or, and the report's
current framing (reservation succeeds *or* fails cleanly) misses the vacuous-success case in between.
Also worth naming as a simpler, more commonly deployed alternative this report does not currently
mention: `pactl suspend-sink <sink> 1` (and its release, `... 0`) works through `pipewire-pulse` as
well as classic PulseAudio, and several audiophile players use it instead of, or alongside, the D-Bus
reservation dance. Finally, tie release ordering to §10.9's device-hold-policy recommendation: release
the PCM handle *before* the D-Bus reservation name, not after — releasing the name first lets a racing
client see a free name while the device handle is still held, reproducing the exact busy-device failure
the reservation exists to prevent. **[the vacuous-success mechanism and `pactl suspend-sink` need
verification against docs.pipewire.org/WirePlumber docs, both blocked from this environment in every
pass so far — flagged as a gap, not asserted as fact; the release-ordering point is [inferred] from
already-cited facts]**

#### 12.9 Concrete device-identity string forms per platform (fills in §10.13's recommendation)

§10.13 correctly recommends persisting a stable device identity instead of an ALSA card *index*, but
gives no concrete string shape — the difference between a recommendation and something implementable.
**Linux/ALSA:** persist the card *id* string (from `/proc/asound/cards`, the bracketed short name) and
open `hw:CARD=<id>,DEV=<n>` (e.g. `hw:CARD=D10,DEV=0`) — index-independent, survives replug/reboot
reordering; on libmpv the equivalent is `--audio-device=alsa/hw:CARD=D10,DEV=0`. tidal-connect already
does exactly this: `CARD_NAME=D10` (`ref:tidal-connect/samples/topping-d10.env`), and its asound.conf
files resolve by card id string (`card snd_rpi_hifiberry_dacplus`, `card "vc4hdmi"`). This is the
correction to Sone's own `hw:C,D` composition from `alsa.card`+`alsa.device` (`ref:sone/src-tauri/src/audio.rs:3285-3287`)
that §10.13 already flags as the index form to avoid. **Windows:** persist the WASAPI endpoint id
(`IMMDevice::GetId()`, the `{0.0.0.00000000}.{guid}`-shaped string) — already what sone-windows reads
as `device.id` from GStreamer's `DeviceMonitor` (`ref:sone-windows/src-tauri/src/audio.rs:2040-2055`).
**macOS:** persist `kAudioDevicePropertyDeviceUID`. In all three cases, apply the rule §10.13 already
states: never silently fall back to `default`/the system output when the persisted device is absent —
surface the choice to the user instead. **[verified for the tidal-connect/Sone/sone-windows citations
already in this report; assembled into concrete strings for the first time here]**

#### 12.10 Loudness-normalization edge cases beyond the formula

§4.4 gives the canonical formula and three shipped implementations but leaves several cases an
implementer hits within the first hundred tracks unanswered. **(a) Missing values.** High Tide treats
a gain of exactly `1.0` as a sentinel for "missing" and skips applying it, citing
`github.com/EbbLabs/python-tidal/issues/332`, with the comment *"Rather quiet album than broken
eardrums"* (`ref:high-tide/src/lib/player_object.py:580-608`, already cited in §4.4 for the mechanism
but not for the required behaviour it implies). Sone instead returns unity gain when `replay_gain` is
`None` (`ref:sone/src-tauri/src/commands/playback.rs:10-21`). **Pick one explicitly** — these produce
different loudness for the same missing-metadata track, and the report currently does not say which
streamboat should do. **(b) `peak > 1`.** `1/peak` correctly attenuates below unity even at zero gain,
but this means two tracks on the same album can receive different *applied* gain under `ALBUM` mode
depending on whether album peak or track peak pairs with the album gain — the Android SDK pairs album
gain with album peak; Sone's `use_track_gain` context-sensitively picks track-with-album-fallback or
album-with-track-fallback (`ref:sone/src-tauri/src/commands/playback.rs:130-150`, already cited).
State which pairing streamboat uses. **(c) Is `preAmp` user-exposed?** TIDAL fixes it at 4 (0 on TV,
§4.4); Sone silently multiplies by an extra 0.8 with no user control over it. Implication #10 already
says not to copy the 0.8 factor, but never states whether the user gets a `preAmp`/trim control at all
— decide this alongside owner decision #11 (bit-perfect default state) rather than leaving it implicit.
**(d) Interaction with §10.2/§12.7:** if bit-perfect mode disables ReplayGain entirely (as §10.2
already establishes it must), the album-to-album loudness jumps normalization exists to remove come
back specifically for the users most likely to be bothered by them — an audiophile bit-perfect user.
That trade belongs in the same owner-facing decision as bit-perfect's default state, not buried inside
§10.2's implementation note. **[verified for (a) and (b)'s citations, already present in this report
for other purposes; (c) and (d) are gaps in what the report currently decides]**

#### 12.11 Two smaller items worth a line each: headless audio-layer session context, and gapless test design

**Headless on Windows/macOS is a user-session process, not a service — and that is an audio-layer
fact, not only a media-integration one.** §10.1 answers headless architecture with tidalt's Linux
systemd-user/D-Bus/Docker model; §11.7 separately notes SMTC needs a real `HWND` (no headless Windows
now-playing). What neither states explicitly: `docs/research/headless-connect.md:1726-1727` already
records (marked unverified there, with a named one-hour spike to confirm) that Windows services run in
session 0 with no interactive audio endpoint, and that macOS needs a per-user LaunchAgent rather than a
LaunchDaemon for the same reason. The audio-layer consequence this report is the one place that can
state it: exclusive-mode WASAPI/CoreAudio access from a non-interactive service context is itself the
capability at risk, not only SMTC — so a Windows/macOS `streamboat-server` is architecturally a
user-session background process on those two platforms, never a service, and §9's "the control thread
is the whole product" headless sketch is fully true only on Linux as currently written. **[cross-
referenced from `headless-connect.md`, itself marked unverified there; restated here because it has an
audio-specific consequence that document does not draw out]**

**No test design exists anywhere for gapless correctness**, even though §10.5 gives a full bit-perfect
test plan and §11.12 flags AAC-tier gapless as unverified and possibly click-prone at every boundary.
A concrete shape: play two synthetic FLAC tracks whose concatenation is a continuous sine wave, capture
the output via `snd-aloop` (§10.5's loopback mechanism), and assert phase continuity / no inserted or
dropped samples at the boundary — the only way to actually detect the ReplayGain-boundary bug (§12.1),
the `track-finished`-fires-early bug (§4.1(b)), and the AAC priming-sample problem (§11.12) rather than
assuming any of them away. This needs fixture audio the project cannot take from TIDAL and does not yet
have: generate synthetic sine tracks at 16/44.1, 24/96 and 24/192 plus an AAC encode of the same, and
commit them alongside the §11.21 manifest fixtures. **[gap — no reference project in this set has any
audio-path test at all; the test shape is the deliverable]**

---

## Implications for streamboat

1. **Request `adaptive=false`.** ABR mid-track means a format change means a device reopen means a
   gap. Bit-perfect and ABR are incompatible.
2. **Prefer the v2 `/trackManifests/{id}` endpoint with a full `formats` array** over the v1
   quality-cascade loop: one request instead of up to four, and the server tells you what you got.
   Keep the v1 `playbackinfopostpaywall` path as a fallback, since every unofficial client uses it.
   **Correction: do not justify keeping v1 as "the one that returns `bitDepth`/`sampleRate`
   directly."** TIDAL's own web SDK types the v1 response's `bitDepth`/`sampleRate` as
   `number | null` with the comment `// API sends null` for both fields, and defensively casts them
   with `?? undefined` wherever they are read (`ref:tidal-sdk-web/.../playback-info-resolver.ts`,
   `.../manifest-parser.ts`); Sone likewise declares both `#[serde(default)] Option<u32>`
   (`ref:sone/src-tauri/src/tidal_api.rs:3676-3682`). Treat v1 the same way §1.2 already treats v2 on
   this point: parse `bitDepth`/`sampleRate` out of the DASH manifest (`Representation@id` /
   `@audioSamplingRate`) or HLS `X-COM-TIDAL-SAMPLE-*` tags on **both** paths, and always confirm the
   final negotiated value by reading it back from the opened device — never trust either endpoint's
   JSON fields as authoritative.
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
17. **Treat exclusive mode as a packaging feature, but not the way this item previously said.**
    **Correction**: Flatpak *can* reach `/dev/snd` — `--socket=pulseaudio` alone already grants it
    (§3.3) — so do not grey out the toggle on Flatpak confinement grounds. Snap genuinely does need
    the `alsa` plug connected manually (`snap connect streamboat:alsa`); detect and surface that
    connection state at runtime, and treat an unconfined native package (deb/rpm/AUR/Nix) as the
    option with no plug/permission caveats at all.
18. **Cap and expire any audio cache**, encrypt it at rest, and purge on logout — see §4.6.
19. **Use GStreamer's `DeviceMonitor` bus messages, not polling**, for the device list, and handle the
    async-provider behaviour of GStreamer 1.28+.
20. **"Pin buffer numbers to TIDAL's own" needs three separate numbers, not one — do not average
    them.** A previous pass of this Implication merged three different pipeline stages under one
    recommendation, which is unsafe to act on directly; see §11.9 for the full arithmetic. In short:
    (a) **network/compressed buffer** — Sone's `uridecodebin buffer-duration` = 15 s compressed;
    TIDAL's web SDK's Shaka `bufferingGoal`/`bufferBehind` = 40 s compressed; (b) **decoded-PCM
    queue** — Sone's branch `queue max-size-time` = 15 s of *decoded* audio, ExoPlayer's 2-minute
    `DefaultLoadControl` figure is *compressed*, not decoded, despite reading like the same kind of
    number; (c) **device buffer** — ExoPlayer's AudioTrack buffer is 1.5 s, Sone's ALSA buffer is
    ~500 ms. Default each stage independently and make each configurable; for the Pi/headless case,
    default the compressed-network stage generously (network is the scarce resource there) but keep
    the decoded-PCM stage conservative — two prerolled 24/192 stereo gapless branches at Sone's 15 s
    decoded-reservoir setting cost roughly 23 MB **each** (192000 × 4 bytes × 2 ch × 15 s), which is
    safe on a 512 MB Pi; the *naive* reading of ExoPlayer's "2 minutes" applied to decoded PCM instead
    of compressed data would be roughly 184 MB per branch and is not.
21. **Capture real TIDAL manifest fixtures as the first engineering task, before any DASH/BTS parser
    code.** One BTS and one DASH manifest per tier, plus one Atmos and one `PREVIEW` response, redacted
    and committed to `tests/fixtures/`. This closes §1.4's `startNumber`/`mimeType` open questions and
    gives CI something to fail on. See §11.21.
22. **Demote `dashdemux2`'s GStreamer element rank at startup** (or hook `autoplug-select`) if Design A
    needs the legacy `dashdemux` path below GStreamer 1.26.10 — plain `uridecodebin`/`playbin` alone
    does not select it, on Linux or on a bundled Windows runtime that ships both plugins. See §11.2,
    §11.4.
23. **Treat `assetPresentation != FULL` as a distinct, required playback state**, not an edge case:
    its own UI (not a generic error), its own play-report suppression, and `previewReason` surfaced
    verbatim. See §11.23.
24. **Decide HLS/EMU manifest handling explicitly.** Parse EMU with the same code path as BTS (same
    `{mimeType, urls}` shape). For HLS, implement the double-base64 decode or fail with a specific,
    actionable error naming the client ID — never a silent/generic failure. See §11.22.
25. **Fire the next-track prefetch at `network-buffer-depth + 8 s` remaining, polled at ~1 Hz**, and
    discard the prefetched `streamingSessionId` if the queue changes before it is used. See §11.11.
26. **Fade out on stop/pause with a short silence ramp, not a gain ramp, in bit-perfect mode** — any
    software gain quantises exactly like the volume slider (Implication #9). Fade back in on resume;
    skipping that mutes the resumed track. See §11.10.
27. **Cover art for OS media integration must be a local file/stream, fetched and written to disk
    before the metadata update is emitted** — remote `mpris:artUrl`s are not reliably fetched by
    desktop shells, and SMTC/`MPNowPlayingInfoCenter` need a local stream/in-memory image, not a URL
    string at all. See §11.8.
28. **Add a startup AAC-decoder capability probe and grey out unreachable quality tiers in the UI**
    rather than failing at play time — Fedora's default repos and a from-scratch Windows GStreamer
    bundle both ship with no AAC decoder present. See §11.4, §11.20.
29. **Scope the gapless guarantee to the FLAC tiers explicitly; treat AAC-tier (`LOW`/`HIGH`) gapless
    as best-effort and verify it per-engine** — no reference client anywhere handles AAC encoder-delay/
    edit-list trimming, and a naive `concat`/`about-to-finish` implementation may click at every
    AAC-tier track boundary even though the code path looks identical to the FLAC case. See §11.12.
30. **Apply per-track ReplayGain gain *inside* each gapless decode branch, upstream of the mixing
    element (`concat`), never on one shared gain element downstream of it.** A single post-`concat`
    `volume` element updated on `active-pad` change applies the wrong track's gain to whatever audio is
    still in flight downstream at the switch instant — an audible level jump at the exact moment
    gapless is supposed to be seamless. See §12.1.
31. **Implement the DASH `SegmentTemplate` identifier set from the spec, not from either reference
    client.** Handle `$Time$` (keyed off `SegmentTimeline/S@t`, not a sequence index), `$Bandwidth$`,
    `$RepresentationID$`, `$$`, and printf-style width tags (`$Number%05d$`); read
    `SegmentTemplate@startNumber` and `@presentationTimeOffset`; and resolve `SegmentTemplate`
    inheritance from `Period`/`AdaptationSet` level, not only `Representation` level. No reference
    client implements any of this. Use a real XML parser, never a regex. See §12.2.
32. **Specify seek precision and seek-flag choice explicitly, and re-feed the init segment after every
    hand-rolled seek.** Pick `FLUSH` (accurate) or `FLUSH | KEY_UNIT` (segment-boundary snap) and state
    why — the reference clients disagree. Pick one authoritative duration source (the track object's
    `duration`) for UI, MPRIS, prefetch-trigger and play-reporting arithmetic, rather than letting the
    MPD's `mediaPresentationDuration` and the decoded sample count silently disagree with it. See §12.3.
33. **Define "bit-perfect" per-track by source type, not as one global mode switch.** It is a
    meaningful claim only for the lossless tiers; for `LOW`/`HIGH` the transparency panel should say
    "lossy source — bit-perfect not applicable" rather than claim a guarantee that was never available,
    and a user should not lose volume control on AAC tracks just because bit-perfect mode is on. See
    §12.7.
34. **Treat `org.freedesktop.ReserveDevice1`'s "reservation acquired" as advisory, not a guarantee.**
    The protocol has no guaranteed server side; a `RequestName` success with nobody actually listening
    is a real failure mode distinct from the three outcomes §3.2 already documents. Always still handle
    `EBUSY` on open with tidalt's retry budget regardless of reservation outcome, and release the PCM
    handle before the D-Bus name on teardown, never after. See §12.8.
35. **Add a DAC warm-up/settle delay (on the order of 200-300 ms, configurable) after every device
    open or format/rate reconfigure, before writing real audio.** No reference client does this; many
    USB/I2S DACs mute output briefly while their PLL re-locks, and per-track rate switching (a headline
    feature per Implication #12) exercises this path routinely, not as an edge case. Count it in the
    time-to-first-audio budget (§11.16). See §12.6.

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
   code and less data leaving the machine. §11.13 now documents the concrete shape (Sone's
   `POST ec.tidal.com/api/event-batch`, a flat 30 s "counts as a stream" threshold, source attribution
   by container) and a real cost the owner should weigh explicitly: events must describe the client
   whose token they ride on, which means pinning a specific TIDAL app version/OS/device string that
   drifts as TIDAL updates — a genuine maintenance tax, not a one-time decision.
6. **Crossfade:** implement it (TIDAL offers 0–15000 ms) or ship gapless only? Note it is mutually
   exclusive with exclusive-device mode.
7. **Video:** in scope at all? If yes, it is a separate HLS path and never bit-perfect.
8. **Client credentials:** which client ID/secret pair, and therefore which tiers are reachable and
   whether streams come back encrypted. This is an auth-topic decision with a direct audio-pipeline
   consequence.
9. **One process or two?** Because a `hw:` device cannot be shared between processes, this is not a
   UI question but the top-level architecture decision for satisfying "desktop and headless, both
   now" (§10.1). tidalt's answer — one server process owns the DAC and the MPRIS name, every other
   invocation becomes a thin client (`ref:tidalt/docs/client-server.md`) — gets headless, CLI, media
   keys, browser-link handling and single-instance enforcement from one mechanism, at the cost of
   routing every UI feature through an IPC boundary from day one. Decide this before writing the
   engine trait in §9, because the trait's shape follows from it.
10. **Any DSP ever — EQ, crossfeed, upsampling, room correction?** Any of them is incompatible with
    bit-perfect by definition. CamillaDSP (already cited in §3.5/§10.7) is the natural "we don't
    build this, users route through it externally" answer; deciding not to build a filter chain is
    cheaper than building one later and is worth stating explicitly rather than leaving implicit.
11. **What is bit-perfect mode's default state?** Given §10.2 (bit-perfect silently disables
    ReplayGain and the volume slider), shipping it on by default produces a player with no working
    volume control for most users; shipping it off by default makes the headline feature invisible.
    Sone's answer is a toggle defaulting off with an explanatory panel; tidalt's is on, with a
    `plughw:` fallback and a `(converted)` badge (§3.1). Pick one and make the onboarding/UI copy
    follow from it.
12. **Bundle a pinned GStreamer on every platform, or link the system one on Linux?** Previously
    unposed even though the report establishes two hard version floors (>= 1.26.10 for FLAC-in-DASH,
    >= 1.28 for `wasapi2sink exclusive`) and separately establishes that current Debian/Raspberry Pi OS
    ships 1.26.2. That conclusion only forces the legacy-`dashdemux`-plus-rank-demotion workaround
    (§11.2) if streamboat links the *system* GStreamer; Windows and macOS must bundle anyway (§11.4),
    so the version floors are free there. See §11.19 for the three options and their consequences —
    system-on-Linux gives Linux the worst FLAC-in-DASH story of the three platforms, which is worth
    the owner seeing explicitly rather than falling out of a packaging default.
13. **Resolve the engine-choice conflict between this report and `tech-stack.md` before writing any
    engine code.** This report's §9 recommends starting with libmpv (Design B) and adding GStreamer
    (Design A) later; `tech-stack.md` recommends the reverse — GStreamer as the primary engine with
    libmpv as backend #2 (`tech-stack.md:1206-1207`). The two documents also disagree on mpv's own
    `--gapless-audio` setting (`weak` here vs `no` in `tech-stack.md:225`). Deciding facts now assembled
    across both documents: §11.4 finds GStreamer-on-macOS entirely unproven in the reference set; §12.4
    finds libmpv's own Windows/macOS distribution equally unproven, *and* that linking libmpv (GPLv2+)
    is a licence conflict with iOS App Store distribution — directly relevant to the owner's "mobile
    must not be precluded" constraint, which neither document currently weighs against its
    recommendation. State explicitly which document is authoritative on engine choice, or supersede
    both with one decision recorded in one place.
14. **Include the legacy `HI_RES` (MQA-era) quality tier in the request ladder, or drop it?** Sone's
    shipped cascade still requests it (§1.8); this report's own findings say the tier's only historical
    content (MQA) was retired 24 July 2024. What a live `audioquality=HI_RES` request returns today —
    downgraded FLAC, an error, or the same answer as `LOSSLESS` — is unresolved in every fact-check pass
    and needs one live request per tier to settle (§1.8, §11.21).
15. **Pick the ReplayGain missing-value and peak-pairing behaviour explicitly.** High Tide skips
    normalization when gain is exactly `1.0` (a missing-metadata sentinel); Sone returns unity gain for
    a `null` gain instead — these produce different loudness for the same track. Similarly, decide
    whether album-mode gain pairs with album peak or track peak. See §12.10.

**Unverified — check before relying on**

- Whether libmpv accepts a `data:application/dash+xml;base64,…` URI, or whether the MPD must be
  written to a file.
- Why High Tide switched from a data URI to `file://` for MPDs at GStreamer 1.26, and whether the
  data-URI form still works on 1.28.
- Why High Tide disables gapless when the sink is `pipewiresink`.
- The exact upstream commit behind "support for FLAC audio in DASH manifests" in GStreamer 1.26.10,
  and whether it changes behaviour for `dashdemux` (legacy) as well as `dashdemux2`.
- Whether GStreamer 1.28 actually fixed "seeking in dashdemux2 for streams with gaps" (§4.7) — cited
  from `phoronix.com`/`lists.freedesktop.org`, neither reachable from this environment, and a
  third-pass fact-check found no other corroborating source. Downgraded from verified to unverified.
- Whether the TIDAL Windows/macOS desktop apps really do not support Dolby Atmos —
  `support.tidal.com` and `tidal.com` are blocked from this environment, so this rests on
  second-hand reporting.
- The exact per-tier bit rates TIDAL publishes today (same blocked-domain problem).
- Whether `SegmentTemplate@startNumber` is present in TIDAL MPDs and what value it takes — the DASH
  spec's default (1) is now stated explicitly in §1.4 and should be assumed until a captured fixture
  proves otherwise (§11.21); neither python-tidal nor tidal-cli reads the attribute. Likewise the
  literal `AdaptationSet@mimeType` string (assumed `"audio/mp4"`) is inferred from convention, not
  observed in any reference checkout — both are exactly what a captured-fixture test suite (§11.21)
  would settle in one step.
- **No longer open — see §11.23:** whether `assetpresentation=FULL` vs `PREVIEW` and `previewReason`
  need special handling is now a required, specified behaviour, not an unknown. What remains genuinely
  open is only the exact UI copy for each `previewReason` value.
- Current PipeWire config keys for pinning rate/format (`default.clock.allowed-rates`,
  per-device `audio.format`, `api.alsa.*`) — `docs.pipewire.org` is blocked from this environment, so
  this could not be resolved in this pass either; §11.5 lists the exact pages and questions to check
  before Implication #17 (Flatpak/Snap exclusive-mode gating) is written as final.
- The Steinberg ASIO SDK's redistribution terms (the "is `asiosink` shipped at all" half of this
  question is now settled — see the ASIO note in §3.4).
- What Chromium's actual output resampling behaviour is in 2026 and whether
  `--audio-output-sample-rate` still works — relevant only if an Electron path is ever considered. Two
  further qualifiers from a second-pass check: the flags are an opt-in setting in tidal-hifi
  (`setManagedFlagsFromSettings`), not applied by default, and the only citation for "Chromium
  resamples" (issues.chromium.org/issues/40944208) is titled about the WebAudio/AudioContext API
  specifically, not proven for the MSE/`<audio>` paths a DASH-playing Electron app would actually use.
- Whether `souvlaki` 0.8.3 (last published June 2025) is still maintained, and whether its macOS
  backend works outside an app bundle (it needs an AppDelegate/winit event loop per its own README —
  see §6).
- Whether bit-perfect output is achievable *through* PipeWire at all without grabbing `hw:` directly —
  see §11.5. This decides whether the Flatpak build (which cannot reach `/dev/snd`, Implication #17)
  can ever offer the headline bit-perfect feature, or must always grey it out.
- Real-world CPU/memory/thermal numbers for FLAC 24/192 decode plus a second gapless decode branch on
  Pi-class hardware (Pi 3B+/4/5) — no reference project publishes benchmarks; see §11.17 for the
  measurement this report recommends running before finalizing the headless engine choice.
- Whether GStreamer.framework (or an equivalent bundled dylib tree) is a workable macOS packaging
  shape at all — no reference client in the set ships GStreamer on macOS, so Design A's macOS
  packaging story (§11.4) is entirely unproven, unlike the documented Windows recipe.
- **Whether the v2 `openapi.tidal.com/v2/trackManifests` endpoint is reachable at all with the
  client credentials an unofficial player like streamboat will actually use.** Every project in the
  reference set that uses v2 (tidal-sdk-web, tidal-sdk-android, tidal-cli) authenticates as a
  registered TIDAL developer-portal application; every unofficial client (High Tide, Sone,
  Strawberry, python-tidal, tidalt, mopidy-tidal, TidaLuna, tidal-hifi) uses v1 exclusively. This
  report's Implication #2 ("prefer v2 over the v1 cascade") is the single biggest architectural
  recommendation in the document, and it is unverified whether that access split is a coincidence or
  a hard boundary TIDAL will not grant to a third-party player. If v2 needs a developer-portal
  grant streamboat cannot get, Implication #2 does not apply and the v1 cascade in §1.8 is not a
  "fallback" — it is the only path.
- **Whether over-requesting `audioquality` on the v1 endpoint always returns HTTP 200 with a
  downgraded quality rather than an error** (§1.8) — this is Sone's own code comment, not
  independently confirmed, and is in tension with tidalt's own descending-ladder retry design.
- Whether `libmpv2` accepts a `data:application/dash+xml;base64,…` URI directly, or whether the
  fallback (write the MPD to a temp file and pass `file://`, or widen the protocol whitelist per
  §Design B) is required in practice.
- miniaudio's exact device-backend and exclusive-mode capabilities (§10.17) — cited from general
  knowledge of the library, not verified against https://miniaud.io/docs/ in this pass.
- Whether `org.freedesktop.ReserveDevice1` has any server-side implementation on a typical PipeWire/
  WirePlumber desktop (a WirePlumber device-reservation component, or PulseAudio's
  `module-reserve-wrapper`), and whether `pactl suspend-sink` is a viable simpler alternative or
  addition — `docs.pipewire.org` and WirePlumber's own docs are blocked from this environment in every
  pass so far. See §12.8.
- Where `libmpv-2.dll` (Windows) and `libmpv.dylib` (macOS) actually come from for a shipped streamboat
  build — mpv publishes no official libmpv binary for either platform, and no reference client in this
  set links libmpv at all, so unlike GStreamer's documented sone-windows recipe (§11.4), Design B's own
  distribution story has never been worked through. See §12.4.
- Whether TIDAL's actual live behaviour for `audioquality=HI_RES` (the retired MQA tier) is a
  downgrade, an error, or a no-op — see the new owner decision on the quality ladder, §1.8.

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
| `ref:strawberry/src/engine/gstenginepipeline.cpp` | generic `exclusive` property handling, `hw:`/`plughw:` ⇒ exclusive inference, audiobin element order; `TaskEnterCallback` (:1788-1803) raising the streaming thread to `SCHED_RR`/`THREAD_PRIORITY_HIGHEST` |
| `ref:strawberry/src/engine/gstengine.cpp` | crossfade gated off when any exclusive pipeline is active |
| `ref:strawberry/src/engine/gststartup.cpp` | `directsoundsink` promoted over `wasapisink`/`wasapi2sink` on Windows and why |
| `ref:strawberry/src/tidal/tidalstreamurlrequest.cpp` | four v1 endpoint variants; encryption rejection on `encryptionType`, `encryptionKey`, `securityType`/`securityToken`; DASH data-URI construction |
| `ref:strawberry/src/engine/*devicefinder.cpp` | per-platform device enumeration including `asiosink` |
| `ref:tidalt/internal/player/alsa.c` | split open/configure, per-bit-depth format preference orders and the DAC-quirk rationale, period-before-buffer ordering, `snd_pcm_hw_params_get_sbits` |
| `ref:tidalt/internal/player/mpv.go` | `org.freedesktop.ReserveDevice1` handshake with three distinct outcomes and its timing budget |
| `ref:tidalt/docs/architecture.md` | end-to-end bit-perfect flow, `plughw:` fallback semantics, quality ladder, MPRIS + private D-Bus interface, over-requesting-quality tension with Sone's 200-downgrade claim |
| `ref:tidalt/docs/client-server.md` | single-owner D-Bus name / thin-client model, `ErrAlreadyRunning` fallback, rationale that `hw:` cannot be shared between processes |
| `ref:tidalt/docs/docker.md` | headless/container invocation (`--device /dev/snd`, `--group-add audio`, `daemon` subcommand), MPRIS-over-mounted-bus recipe |
| `ref:tidalt/README.md` | CLI modes (`tidalt`, `daemon`, `play tidal://…`, `setup --daemon`), arm64/aarch64 packaging, MPRIS-not-registered-in-plain-TUI caveat, hold-only-while-playing device policy |
| `ref:mopidy-tidal/mopidy_tidal/playback.py` | MPD written to `manifest.mpd` and served as `file://`; BTS direct URL |
| `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/{proxy,cache}.py` | localhost relay proxy with SQLite chunked-insert cache, finalisation semantics, TidalID→Path offline mapping |
| `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/types.py` | HTTP `Range`/`Content-Range` handling used by the cache proxy — the model for resuming a direct-URL stream after a mid-track failure |
| `ref:tidal-connect/userconfig/` (`README.md`, `hdmi-rpi-44.asound.conf`, `hifiberry-dac-plus.asound.conf`, `iqaudio-dac.asound.conf`, `xmos-dac-softvol-s16.asound.conf`) | Raspberry Pi HDMI (`vc4hdmi`) and I2S HAT ALSA configs, the "limited to 16bit/44.1kHz on Pi HDMI" hi-res narrowing case, `iec958`/softvol plug patterns |
| `ref:tidal-connect/samples/topping-d10.env`, `ref:tidal-connect/bin/common.sh` | device selection and mixer-control naming by card **name** (not index); the `SoftMaster` rename and its "volume slider might act on the hardware volume control" warning |
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
| `ref:TidaLuna/plugins/lib.native/src/request/fetchMediaItemStream.ts`, `fetchStream.ts` | corrected §4.2 finding: `Semaphore(2)` bounds concurrent **track** fetches, not segment fetches; segments within one track are fetched in a strictly sequential `for` loop |
| `ref:tidal-sdk-web/packages/player/src/internal/helpers/generate-guid.ts` | client-side v4 GUID generation for `streamingSessionId` |
| `ref:tidal-connect/bin/entrypoint.sh` | ALSA device selection by card name, softvol, post-MQA hi-res ceiling (see §11 correction) |
| `ref:tidal-connect/README.md` | confirms MQA self-unfolding (to 24/88-24/96) was lost in July 2024 while the 24/48 hi-res ceiling is a separate, still-standing limit (§11 correction) |
| `ref:sone-windows/README.md:95-155` | the complete GStreamer-on-Windows bundling recipe: `prepare-gstreamer.js`, the exact DLL/plugin list, `gioopenssl.dll`'s role, no AAC decoder in the bundle (§11.4) |
| `ref:sone-windows/src-tauri/src/media_controls.rs` | SMTC needs a real `HWND`; polls for the main window up to 5 s before building `PlatformConfig`; only Play/Pause/Toggle/Next/Previous/Stop wired, no Seek/SetPosition/SetVolume (§11.7) |
| `ref:sone-windows/src-tauri/src/idle_inhibit.rs` | Linux-only D-Bus/portal idle inhibition; dead code on Windows — no reference implementation for the Windows/macOS idle-inhibit gap (§11.6) |
| `ref:high-tide/src/window.py:223`, `ref:high-tide/src/lib/utils.py:76-78,828-843` | `evict_cache(MUSIC_DIR, 5)` run at window construction; atime-ordered LRU eviction to a 5 GB cap — corrects the "no size cap" claim (§4.6) |
| `ref:high-tide/src/mpris.py:428-458` | `mpris:artUrl` set to a local `file://` path to a pre-cached 320px cover, never a remote URL; `mpris:trackid`/`mpris:length` shape (§11.8) |
| `ref:sone/src-tauri/src/mpris.rs:14,234,252-253` | `art_url` carried through the MPRIS command enum, set only when non-empty (§11.8) |
| `ref:strawberry/src/engine/gstengine.cpp:88-89,233-249,336-389,595-620` | fade-out on stop/pause gated the same way as crossfade on `AnyExclusivePipelineActive()`; the `AboutToFinish`/prefetch-lead formula (`buffer_duration + (autocrossfade ? fadeout : 8s)`, polled at 1 Hz) (§11.9, §11.10) |
| `ref:sone/src-tauri/src/tidal_report/{event,mod}.rs` | play-reporting to `ec.tidal.com/api/event-batch`, the flat 30 s threshold, source attribution, and the device-impersonation constraint on the client version pin (§11.13) |
| `ref:tidal-cli/src/playback.ts:156-170` | DASH init and media segment fetches sent with no `Authorization` header or any auth at all — CDN URLs are pre-signed (§11.14) |
| `ref:high-tide/src/lib/player_object.py:494-513` | every track's MPD is written to one fixed shared path (`CACHE_DIR/manifest.mpd`), overwritten per track — unsafe under prefetch/gapless (§11.18) |
| `ref:sone/README.md:283,306,345,401`, `ref:sone/snap/snapcraft.yaml:40,127-129,153` | per-distro AAC decoder packaging (`gstreamer1.0-libav` vs Fedora's RPM-Fusion-only `gstreamer1-plugin-libav` vs Arch's `gst-libav`); Snap's staged `libfaad`/blas-lapack symlink workaround (§11.20) |

### Upstream source read directly

| URL | Supports |
|---|---|
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-bad/sys/wasapi2/gstwasapi2sink.cpp` | `wasapi2sink` has a real `exclusive` property (`PROP_EXCLUSIVE`), plus `low-latency`, `device`, `mute`, `volume`, `continue-on-error`; its gtk-doc block is tagged `Since: 1.28` — absent on 1.26 and earlier |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/sys/osxaudio/gstosxaudiosink.c` | `osxaudiosink` exposes only `device`, `unique-id`, `configure-session`, `volume` — no exclusive/hog property |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/sys/osxaudio/gstosxcoreaudiohal.c` | hog mode (`kAudioDevicePropertyHogMode`), `SupportsMixing=false` and physical-format setting exist but are called only from `_open_spdif` |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-base/ext/alsa/gstalsasink.c` | `alsasink` has only `device`, `device-name`, `card-name` — no exclusive or resample control |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/gst/isomp4/qtdemux.c` and `.../fourcc.h` | `FOURCC_fLaC` handling; FLAC-in-MP4 produces `audio/x-flac` caps |
| `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-good/ext/adaptivedemux2/dash/gstmpdhelper.c` | DASH `audio/mp4` maps to `audio/x-m4a`; no FLAC-specific mapping in the mime helper |
| `https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavformat/isom_tags.c` and `.../mov.c` | `fLaC` → `AV_CODEC_ID_FLAC`; `dfLa` FlacSpecificBox parsing (`mov_read_dfla`) |
| `https://raw.githubusercontent.com/mpv-player/mpv/master/DOCS/man/options.rst` | `--audio-exclusive` works for wasapi/coreaudio/pipewire/audiounit **and silently no-ops on `alsa`**; `--gapless-audio=<no\|yes\|weak>` semantics; `--audio-samplerate`/`--audio-format`/`--audio-channels`; `--audio-spdif=ac3,dts,dts-hd,eac3,truehd,dsd`; `--prefetch-playlist`; `--volume-gain`/`--replaygain-clip`; `--demuxer-lavf-o`/`--stream-lavf-o` |
| `https://raw.githubusercontent.com/mpv-player/mpv/master/DOCS/man/ao.rst` | `coreaudio_exclusive` AO, `--coreaudio-change-physical-format`, `--wasapi-exclusive-buffer`, `--alsa-resample` (disabled by default), `--alsa-buffer-time`, `--alsa-periods`, `--alsa-mixer-device`/`-name`/`-index`, `--audio-channels=auto-safe` warning |
| `https://raw.githubusercontent.com/mpv-player/mpv/master/DOCS/man/input.rst` | `ao-volume` (system/hardware mixer) vs `volume` (software mixer); `audio-out-params`/`audio-params`/`current-ao`/`audio-device-list` read-back properties |
| `https://raw.githubusercontent.com/mpv-player/mpv/master/Copyright` | mpv is GPLv2+ by default; LGPLv2.1+ build via Meson `-Dgpl=false`, intended for libmpv, "not recommended to build mpv CLI in LGPL mode at all"; disabled features in LGPL mode |
| `https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavcodec/ac3_parser.c`, `.../ac3dec.c`, `.../eac3dec.c` | FFmpeg's E-AC-3 decoder discards JOC object metadata (5.1 downmix only); the parser can detect Atmos presence via the additional-bitstream-info byte but no renderer exists |
| `https://raw.githubusercontent.com/pdeljanov/Symphonia/master/symphonia-format-isomp4/src/atoms/{mod,stsd}.rs` | undocumented `AtomType::Flac`/`FlacAtom` support in the ISO/MP4 demuxer; single-sample-entry `stsd` limitation |
| `https://crates.io/api/v1/crates/{gstreamer,rodio,kira,symphonia,cpal,souvlaki,libmpv2,mpris-server,alsa,wasapi,coreaudio-rs,dash-mpd,stream-download,oboe,ffmpeg-next,rubato,symphonia-bundle-flac,symphonia-codec-aac}` | all crate versions, licences and last-update dates quoted in §8 (queried 2026-09-07) |
| `https://github.com/pdeljanov/Symphonia` | codec support matrix (FLAC excellent, AAC-LC great, HE-AAC not started), MPL-2.0, MSRV 1.85, gapless support notes (ISO/MP4 gapless: No) |
| `https://github.com/RustAudio/cpal/issues/459` | WASAPI exclusive mode requested 2020-07-27, issue closed with no implementation |
| `https://raw.githubusercontent.com/RustAudio/cpal/master/src/host/wasapi/device.rs` | hardcodes `AUDCLNT_SHAREMODE_SHARED` at four call sites (lines 648, 726, 879, 982 — corrected from a previous pass's "three"); no exclusive-mode path exists to enable |
| `https://github.com/HEnquist/camilladsp/blob/master/backend_coreaudio.md` | CoreAudio `exclusive` (hog) flag, rate-change notification handling, S16/S24/S32/F32 physical formats, BlackHole caveat |
| `https://github.com/Sinono3/souvlaki` (README, `Cargo.toml` tag 0.8.3) | one `MediaControls` API for MPRIS/SMTC/MPNowPlayingInfoCenter; macOS "requires an AppDelegate/winit event loop"; `edition = "2024"` (needs Rust >= 1.85) contradicts crates.io's declared MSRV 1.67 |
| `https://packages.debian.org/trixie/gstreamer1.0-plugins-bad` and `https://packages.debian.org/source/trixie/gstreamer1.0` | Debian 13 "trixie" ships GStreamer 1.26.2 — below the 1.26.10 FLAC-in-DASH floor |
| `https://linuxiac.com/gstreamer-1-28-3-released-with-security-and-playback-fixes` and `https://9to5linux.com/gstreamer-1-28-3-adds-nxp-i-mx-8m-plus-hardware-accelerated-h-265-encoding` | GStreamer 1.28.3: `devicemonitor` now waits for its start thread before listing devices |
| `gst-plugins-bad/ext/dash/gstdashdemux.c:446` (`GST_ELEMENT_REGISTER_DEFINE(dashdemux, "dashdemux", GST_RANK_PRIMARY, ...)`) and `gst-plugins-good/ext/adaptivedemux2/dash/gstdashdemux.c:4210` (registers at `GST_RANK_PRIMARY + 1`) — both in the GStreamer monorepo, `subprojects/` | legacy `dashdemux` and `dashdemux2` both autoplug for `application/dash+xml`; `dashdemux2` outranks `dashdemux` by one rank, so plain `uridecodebin`/`playbin` picks `dashdemux2` whenever `adaptivedemux2` is installed (§11.2) |

### Web sources

| URL | Supports |
|---|---|
| `https://www.via-la.com/licensing-programs/aac/` | AAC pool is an active per-unit licensing programme (blocked from this environment — reached only via the secondary sources below, see the §2.3 correction) |
| `https://ipfray.com/access-advance-via-la-position-multimedia-patent-pools-for-further-growth-with-price-stability-offer-regionalization-for-more-standards/`, `https://via-la.com/aac-license-fees-structures/`, `https://lumenci.com/blogs/aac-licensing-explained-patent-pools-royalty-obligations-and-sep-exposure/`, `https://scconline.com` (2026-02-13 Via Licensing Alliance analysis) | Via LA AAC pool rates and tiering, 2026 state: tiered per-unit ($0.98/$0.78/$0.68/$0.45 by volume), 900+ licensees |
| `https://www.digitaltrends.com/home-theater/tidal-killing-mqa-sony-360-reality-audio/` and `https://www.whathifi.com/news/tidal-scraps-mqa-and-spatial-audio-format-heres-what-that-means-for-subscribers` | MQA catalogue replaced with FLAC and all 360RA content removed on 24 July 2024; Dolby Atmos chosen as the surviving immersive format |
| `https://www.phoronix.com/news/GStreamer-1.28` and `https://lists.freedesktop.org/archives/gstreamer-devel/2026-January/082207.html` | GStreamer 1.28.0 released 27 January 2026 and ported wasapi2 to IMMDevice-based device selection — both re-confirmed via `discourse.gstreamer.org/t/gstreamer-1-28-0-new-major-feature-release/5700`. The "dashdemux2 seeking-with-gaps fix" sub-claim is **unverified**: both URLs in this row and `gstreamer.freedesktop.org` are blocked from this environment and no other source corroborates it (§4.7) |
| `https://linuxiac.com/gstreamer-1-28-5-released-with-security-and-playback-fixes/`, `https://linuxiac.com/gstreamer-1-28-6-adds-h-266-mp4-muxing-and-ffmpeg-9-0-support/`, `https://9to5linux.com/gstreamer-1-28-6-adds-h-266-muxing-support-to-the-rust-mp4-muxers`, `linuxcompatible.org` ("GStreamer 1.28.6 Drops: Final 1.28 Release") | corrects the report's own prior "1.28.2/1.28.3 is current" statement: the 1.28 series has shipped through 1.28.5 and then **1.28.6** (5 August 2026), announced as the final 1.28 bug-fix release; 1.28.6 adds FFmpeg 9.0 support (§11.1) |
| `https://9to5linux.com/gstreamer-1-26-10-released-with-support-for-flac-audio-in-dash-manifests` and `https://linuxiac.com/gstreamer-1-26-10-brings-fixes-for-flac-opus-and-matroska-handling/` | GStreamer 1.26.10 (December 2025) added FLAC-in-DASH-manifest support, FLAC 6.1/7.1 layouts and 32-bit FLAC, adaptivedemux2 stream-selection fixes |
| `https://issues.chromium.org/issues/40944208` and `https://strongrandom.com/post/high-bit-rate/` | **[uncertain]** cited for "Chromium/Web Audio always resamples to the output device rate", but the issue title is specifically about the WebAudio/AudioContext API, not every Chromium audio path, and the issue body could not be read (issues.chromium.org is blocked from this environment) |
| `https://lib.rs/crates/souvlaki` | souvlaki covers MPRIS, SMTC and MPNowPlayingInfoCenter behind one API; dbus-crossroads default and zbus alternative on Linux; MSRV 1.67 (see the upstream-source row above for the MSRV/edition contradiction this report found) |
| `https://docs.pipewire.org/page_audio.html` | audio adapter passthrough mode as the mechanism for exclusive access; `default.clock.rate` / `default.clock.allowed-rates` |
| `https://support.tidal.com/hc/en-us/articles/25876825185425-Audio-Format-Updates` and `https://support.tidal.com/hc/en-us/articles/360004255778-Dolby-Atmos` | TIDAL's own audio-format-update and Atmos pages — **referenced but not read; `support.tidal.com` is blocked from this environment** |
| `https://discourse.gstreamer.org/t/gstreamer-1-28-0-new-major-feature-release/5700` | Re-confirms GStreamer 1.28.0's release date (27 January 2026) and the WASAPI2 IMMDevice-based device-selection port from an unblocked source (§11.1); does **not** corroborate the dashdemux2 gap-seeking claim, which is downgraded to unverified in this pass (§4.7) |
| ISO/IEC 23009-1 (MPEG-DASH), §5.3.9.4.4 "Template-based Segment URL construction" and §5.3.9.6 "Segment timeline" | The full `SegmentTemplate` identifier set (`$Number$`, `$Time$`, `$Bandwidth$`, `$RepresentationID$`, `$$`, printf width tags) and the `SegmentTimeline/S@t`/`@d`/`@r` timing model — no reference client implements more than a `$Number$` subset of this; see §12.2/§12.3 |
| `docs/research/tech-stack.md:225,1206-1207,1390` (this repository) | Recommends GStreamer as the primary audio engine with libmpv as backend #2, and `--gapless-audio=no` for "strict bit-perfect" — directly contradicts this report's §9 (libmpv-first) and §4.1(d) (`--gapless-audio=weak`); flagged as an unresolved owner decision, §12.5 |
| `docs/research/headless-connect.md:1726-1727,2207` (this repository) | Windows services run in session 0 with no interactive audio endpoint; macOS needs a per-user LaunchAgent, not a LaunchDaemon — both marked unverified there; restated here for its audio-layer consequence (§12.11) |
