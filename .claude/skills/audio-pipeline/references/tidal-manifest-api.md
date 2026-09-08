# TIDAL manifest APIs

Full narrative: `docs/research/audio-pipeline.md` §1, plus fact-check gap-fill in its §10.11,
§10.14, §10.15, and the "v2 reachability" open question in its Open questions section.

## Table of contents

1. Quality tiers and codec mapping
2. The two manifest APIs — v1 (legacy) and v2 (modern)
3. Manifest MIME types and parsing (BTS, EMU, DASH, HLS)
4. DASH manifest structure and segment assembly
5. Encryption — the refusal rule
6. Video
7. Error handling, sub-statuses, and the quality cascade
8. `streamingSessionId` — client-generated, not server-issued
9. `mediaMetadataTags` / `audioModes` vocabulary
10. Quality-tier transitions mid-stream (ABR)
11. Open question: is v2 reachable to an unofficial client at all?
12. HLS and EMU manifests need an explicit decision, not silent failure
13. PREVIEW / `previewReason` — a required behaviour, not an edge case
14. `countryCode` provenance and its diagnostic implication
15. Streaming-privileges WebSocket reconnect policy
16. Capture real TIDAL manifest fixtures — do this before writing a parser

---

## 1. Quality tiers and codec mapping

`ref:python-tidal/tidalapi/media.py:56-66`:

```
Quality.low_96k          = "LOW"              -> HE-AAC v1 (needs SBR, ~96 kbps)
Quality.low_320k         = "HIGH"             -> AAC-LC (~320 kbps)
Quality.high_lossless    = "LOSSLESS"         -> FLAC 16/44.1 (or ALAC — see below)
Quality.hi_res_lossless  = "HI_RES_LOSSLESS"  -> FLAC up to 24/192
Quality.default          = "HIGH"
```

A legacy fifth value `HI_RES` (MQA) is gone from python-tidal's `Quality` enum above but is still
requested by Sone's shipped quality cascade (`ref:sone/src-tauri/src/commands/playback.rs`) and
carried in the iOS SDK's codec map below — dead content since 24 July 2024 (§1.7 caveat below,
§7's open decision on whether to keep requesting it). The 96/320/1411/up-to-24-192 kbps figures are
TIDAL marketing numbers repeated by
third-party reviews — not confirmed against a first-party TIDAL page in this research pass
(`support.tidal.com` is blocked from the environment). **Correction: `bitDepth`/`sampleRate` are
not reliable on v1 either.** A prior version of this note called them "the authoritative per-track
values" on v1, "not present on v2." Both TIDAL's own web SDK and Sone treat the v1 fields as
optional/nullable: the SDK types them `bitDepth: number | null` / `sampleRate: number | null` with
the comment `// API sends null`, and Sone declares both `#[serde(default)] Option<u32>`
(`ref:tidal-sdk-web/.../playback-info-resolver.ts`, `ref:sone/src-tauri/src/tidal_api.rs:3676-3682`).
**Treat both endpoints the same way: parse `bitDepth`/`sampleRate` out of the DASH `Representation@id`/
`audioSamplingRate` or HLS `X-COM-TIDAL-SAMPLE-*` tags (§4), and confirm the final value by reading
it back from the opened device — never trust either endpoint's JSON fields as authoritative.**

Tier→codec table, from `ref:tidal-sdk-ios/Sources/Player/Common/Data/AudioCodec.swift`:

| Tier | Codec | Manifest codec strings |
|---|---|---|
| `LOW` | HE-AAC v1 | `mp4a.40.5`, `heaacv1` |
| `HIGH` | AAC-LC | `mp4a.40.2`, `aaclc`, `aac` |
| `LOSSLESS` | FLAC (ALAC possible — see below) | `flac` |
| `HI_RES` (legacy) | MQA | `mqa` |
| `HI_RES_LOSSLESS` | FLAC | `flac`, `flac_hires` |
| `DOLBY_ATMOS` (any tier) | E-AC-3 (JOC) | `eac3`, `ac4` |
| `SONY_360RA` | unsupported — iOS SDK returns `nil`, comment: *"Sony 360 Reality Audio has no codec the client needs, so it is unsupported here"* | `mha1`, `mhm1` |

**ALAC/AC-4 caveat.** The iOS SDK carries `.AC4` and `.ALAC` as first-class `AudioCodec` cases, and
its `LOSSLESS` mapping comment reads *"Could be `.ALAC`, but we need to update Player to get that"*
— i.e. TIDAL's own SDK anticipates serving ALAC for `LOSSLESS` on some path, even though no
reference client has ever observed it served. **A FLAC-only decode assumption for
LOSSLESS/HI_RES_LOSSLESS is not future-proof** — decode ALAC too if the decoder stack supports it
cheaply (FFmpeg and GStreamer both do).

The v2 API speaks in **formats**, not tiers (`ref:tidal-sdk-web/.../playback-info-resolver.ts`,
`audioQualityToFormats`):

```ts
'HI_RES' | 'HI_RES_LOSSLESS' -> ['HEAACV1','AACLC','FLAC','FLAC_HIRES']
'LOSSLESS'                   -> ['HEAACV1','AACLC','FLAC']
'HIGH'                       -> ['HEAACV1','AACLC']
'LOW'                        -> ['HEAACV1']
// audioFormatsToQuality: FLAC_HIRES->HI_RES_LOSSLESS; FLAC->LOSSLESS; AACLC->HIGH; else LOW
```

The full v2 format enum (`ref:tidal-sdk-android/tidalapi/.../TrackManifestsAttributes.kt`):
`HEAACV1, AACLC, FLAC, FLAC_HIRES, EAC3_JOC` — no 360RA token, no MQA token, consistent with both
being gone (§5 in `references/atmos-and-immersive.md`). The Android SDK builds this list
additively and appends `EAC3_JOC` only when immersive audio is requested
(`ref:tidal-sdk-android/.../PlaybackInfoRepositoryDefault.kt`, `getRequestedFormats`); presence of
`EAC3_JOC` in the response is how it decides `audioMode = DOLBY_ATMOS`.

## 2. The two manifest APIs — v1 (legacy) and v2 (modern)

**v1 — what every unofficial client uses:**

```
GET https://api.tidal.com/v1/tracks/{trackId}/playbackinfopostpaywall
  ?countryCode={cc}
  &audioquality={LOW|HIGH|LOSSLESS|HI_RES|HI_RES_LOSSLESS}
  &playbackmode=STREAM
  &assetpresentation=FULL
Authorization: Bearer {access_token}
```

Confirmed identical across `ref:sone/src-tauri/src/tidal_api.rs:3654-3670` (`get_stream_url`),
`ref:python-tidal/tidalapi/media.py` (`Track.get_stream`), and
`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:132-138`.

Response fields observed across implementations: `manifestMimeType`, `manifest` (base64),
`manifestHash`, `audioQuality`, `audioMode`, `assetPresentation`, `bitDepth`, `sampleRate`,
`trackReplayGain`, `trackPeakAmplitude`, `albumReplayGain`, `albumPeakAmplitude`, `trackId`,
`streamingSessionId` (see §8).

Prefetch headers on the web SDK's legacy path (`{legacyApiUrl}/{tracks|videos}/{id}/playbackinfo`,
no `postpaywall`): `x-tidal-token: {clientId}`, `x-tidal-streamingsessionid: {sessionId}`, and
`x-tidal-prefetch: true` when the fetch is a prefetch. `legacyApiUrl = https://api.tidal.com/v1`
(`ref:tidal-sdk-web/.../config.ts:27`).

Strawberry supports four v1 variants, user-selectable, defaulting to `playbackinfopostpaywall`
(`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:118-146`):
`tracks/{id}/streamUrl?soundQuality=`, `tracks/{id}/urlpostpaywall?…` (the simplest — returns
`{urls: [...]}`, a direct CDN URL, no manifest at all), `.../playbackinfopostpaywall`,
`.../playbackinfo`.

**`urlpostpaywall`'s parameter set is not one fixed shape — record both variants seen, don't invent
a third.** Strawberry's request carries **four** params: `audioquality`, `playbackmode=STREAM`,
`assetpresentation=FULL`, `urlusagemode=STREAM` (`tidalstreamurlrequest.cpp:126-131`). python-tidal's
own call sends a different set again — `urlusagemode=STREAM`, `audioquality`, `assetpresentation=FULL`,
with **no** `playbackmode` (`ref:python-tidal/tidalapi/media.py:422-429`) — and python-tidal refuses
this endpoint entirely under a PKCE session (`if self.session.is_pkce: raise URLNotAvailable`). If
streamboat ends up using PKCE auth (see `tidal-api` skill), `urlpostpaywall` may not be an option at
all regardless of which parameter shape is used.

**v2 — what TIDAL's own SDKs use:**

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

and `GET /v2/videoManifests/{id}?uriScheme=DATA&usage=PLAYBACK` (query is narrower: only
`uriScheme`/`usage`). `apiUrl = https://openapi.tidal.com/v2/`
(`ref:tidal-sdk-web/.../config.ts:22`). Confirmed in
`ref:tidal-sdk-web/.../playback-info-resolver.ts:362-377,447-458` (`_fetchTrackManifest`,
`_fetchVideoManifest`), `ref:tidal-cli/src/playback.ts` (`fetchTrackManifestData`), and
`ref:tidal-sdk-android/.../PlaybackInfoRepositoryDefault.kt:60-65` (source of the `DATA|HTTPS` /
`PLAYBACK|DOWNLOAD` enum values).

Response is JSON:API. Attributes seen: `uri` (a `data:<mime>;base64,<manifest>` URL when
`uriScheme=DATA`, else an https URL), `formats`, `hash`, `trackPresentation` (`FULL`/`PREVIEW`),
`previewReason`, `trackAudioNormalizationData.{replayGain,peakAmplitude}`,
`albumAudioNormalizationData.{replayGain,peakAmplitude}`, `drmData.licenseUrl`.

**v2 does not carry `bitDepth`/`sampleRate`.** The web SDK hardcodes both to `0` in its return value
(`playback-info-resolver.ts:411,419`) and recovers real values by regexing the manifest itself:
`dashFindBitDepth`/`dashFindSampleRate` (DASH `Representation@id`/`audioSamplingRate`) or
`hlsFindBitDepth`/`hlsFindSampleRate` (HLS `X-COM-TIDAL-SAMPLE-DEPTH`/`X-COM-TIDAL-SAMPLE-RATE`) in
`ref:tidal-sdk-web/.../manifest-parser.ts:111-121,191-210,254-256`. **Never trust `bitDepth`/`sampleRate`
from a raw v2 response — parse the manifest.**

**Manifest type is DRM-driven, not a free choice.** The web SDK asks for HLS when
`shaka.drm.FairPlay.isFairPlaySupported()` is true (Safari/Apple) and MPEG-DASH otherwise
(`playback-info-resolver.ts:358,373` — both branches confirmed). Android always requests
`MPEG_DASH` unconditionally (`PlaybackInfoRepositoryDefault.kt:58`, confirmed). **iOS requests
`.hls` unconditionally** — verified directly against the checkout:
`ref:tidal-sdk-ios/Sources/Player/Common/PlaybackInfo/PlaybackInfoFetcher.swift:87` passes
`manifestType: .hls` to `TrackManifestsAPITidal.trackManifestsIdGet`.

## 3. Manifest MIME types and parsing (BTS, EMU, DASH, HLS)

Four MIME types, from `ref:tidal-sdk-web/.../constants.ts`:

```ts
BTS:  'application/vnd.tidal.bts'
DASH: 'application/dash+xml'
EMU:  'application/vnd.tidal.emu'
HLS:  'application/vnd.apple.mpegurl'
```

python-tidal has `MPD`, `BTS`, a `VIDEO: "video/mp2t"` value, and commented-out `EMU`/`APPL`
(`ref:python-tidal/tidalapi/media.py:111-119`).

**BTS** base64-decodes to JSON:

```json
{ "mimeType": "...", "codecs": "flac", "encryptionType": "NONE", "keyId": "...",
  "licenseSecurityToken": "...", "urls": ["https://..."] }
```

Handling: base64 → JSON → take `urls[0]`, feed to the decoder as an ordinary HTTP URL. Confirmed in
`ref:sone/src-tauri/src/tidal_api.rs:3706-3725`, `ref:tidal-sdk-web/.../manifest-parser.ts`
(`parseJSONManifest`), `ref:high-tide/src/lib/player_object.py:515-522`, `ref:tidal-cli/src/playback.ts:110-113`
(`json.urls[0]`).

**EMU** (`EmuManifest` in the web SDK) is a genuine **subset** of BTS's shape — just
`{mimeType, urls}`, no `codecs`/`encryptionType`/`keyId`/`licenseSecurityToken` — parsed by the same
function. Do not assume EMU carries every BTS field.

**DASH** base64-decodes to an MPD XML document. **Four feeding strategies are in the reference
set** — do not describe this as "three," an earlier draft of this material undercounted by one:

1. Wrap back into a data URI, `data:application/dash+xml;base64,<b64>` — Sone
   (`ref:sone/src-tauri/src/commands/playback.rs:118-127`), Strawberry, TIDAL's own web SDK.
2. Write to a file and pass `file://` — mopidy-tidal always; High Tide **only on GStreamer >= 1.26**,
   falling back to the data URI below that (`ref:high-tide/src/lib/player_object.py:487-513`, reading
   `Gst.version()` directly). Whether this is because newer GStreamer stopped accepting the data-URI
   form is unconfirmed ([unverified] — see the SKILL.md Unverified list).
3. **Ephemeral local HTTP route**: base64-decode and serve the manifest from a local route kept
   alive for `track.duration + 300s`. Music Assistant's TIDAL provider
   (`music_assistant/providers/tidal/streaming.py`, fetched 2026-09-08), chosen explicitly because
   ffmpeg cannot re-fetch a `data:` URI mid-playback (`tidal-oss-landscape/references/project-profiles.md`
   §5a). The strategy to use if the media layer may re-open or range-request the manifest source
   (seeking, a restart, an external decoder like ffmpeg) and a filesystem write isn't wanted.
4. Parse the MPD yourself and fetch segments — python-tidal (`DashInfo`), tidal-cli.

**Selection rule**: if the media layer may re-open or range-request the manifest source (seeking, a
restart, an external decoder like ffmpeg), strategy 1 is unsafe — use 2 or 3. Strategy 4 is needed
only with no DASH-capable demuxer at all. **Default recommendation for streamboat: strategy 1, with
2 as the fallback.**

**HLS** base64-decodes to a master playlist whose variant lines are themselves base64 data URLs; the
web SDK decodes the first line again before scanning for `X-COM-TIDAL-SAMPLE-DEPTH`/`-RATE`.

## 4. DASH manifest structure and segment assembly

From `ref:python-tidal/tidalapi/media.py:744-860` (`DashInfo`), the MPD TIDAL emits with
`adaptive=false` (or via v1, which never sets `adaptive` at all) has exactly one Period → one
AdaptationSet → one Representation, with one SegmentTemplate and one SegmentTimeline. **This shape
holds only for the non-adaptive case** — `adaptive=true` yields multiple Representations and
mid-stream ABR switching (§10 below). python-tidal indexes
`periods[0].adaptation_sets[0].representations[0]` and does **not** read `Representation@id`.

Fields:

- `MPD@mediaPresentationDuration` (ISO-8601, e.g. `PT2M26.47S`)
- `AdaptationSet@contentType`, `@mimeType` (`audio/mp4`) — **[unverified]**: no reference client
  asserts this literal string, only reads the attribute; it is inferred from FLAC-in-fMP4 convention,
  not observed in a captured TIDAL manifest (see the fixture-capture note at the end of this section)
- `Representation@codecs` (`flac`, `mp4a.40.2`, `mp4a.40.5`)
- `Representation@audioSamplingRate`
- `Representation@id` — carries `"FLAC,44100,16"` (codec, rate, bit depth). **Evidenced only by the
  web SDK's parser** (`ref:tidal-sdk-web/.../manifest-parser.ts:107-121`), not by python-tidal — the
  web SDK takes the last integer as bit depth, returns `undefined` when there's no comma
  (`HEAACV1`, `AACLC`).
- `SegmentTemplate@initialization`, `@media` (a `$Number$` template), `@timescale`
- `SegmentTimeline/S@d` (duration in timescale units), `@r` (repeat count)

**Segment numbering disagreement — TWO separate bugs, not one:**

- python-tidal: `segments_count = 1 + 1 + Σ(S.r or 1)`, then
  `[media.replace("$Number$", str(i)) for i in range(segments_count)]` — starts at **0**, folds the
  init segment into the same numbering (`media.py:828-875`, accumulation loop at `:836-845`).
- tidal-cli: downloads `initialization` separately, numbers media segments from **1**
  (`playback.ts:120-170`, specifically `:124-133`).

Neither reads `SegmentTemplate@startNumber` (DASH default 1). That's bug #1 (start index).

**Bug #0, more serious than the numbering disagreement: python-tidal's `urls` list likely never
contains a usable init segment.** `DashInfo` parses `SegmentTemplate@initialization` into
`DashInfo.first_url` (`media.py:786-796`), but `get_urls()` never emits it — it substitutes
`$Number$` only into the **media** template, for `range(segments_count)` (`media.py:833-875`). A
stream assembled purely from `get_urls()`'s return value therefore has no `moov`/`dfLa` box and is
undecodable, unless TIDAL's media template at `$Number$=0` happens to be byte-identical to the
separately-parsed `initialization` URL — unproven either way, since no captured TIDAL fixture exists
(§16). Combined with the `1 + 1` prefix in `segments_count`, this also over-generates one URL past
the end of a single-run timeline (correct count for one `<S d="…" r="N"/>` run is `N+1`; the `1+1`
prefix makes it `N+2`) — a trailing 404 on a well-formed timeline. **Do not copy python-tidal's
segment-URL generation as reference behaviour for anything.** **[uncertain — read from source, not
exercised against a real manifest]**

**Bug #2, easy to miss: python-tidal also mis-accumulates `@r` itself.** Per the DASH spec,
`SegmentTimeline/S@r` is the number of *additional* repeats — one `<S>` element with a given `@r`
contributes **`r + 1`** segments to the timeline, and `@r` defaults to 0. python-tidal's loop does
`segments_count += s.r if s.r else 1` — it adds `r`, not `r + 1`, so it **under-counts by one segment
per `<S>` run that carries a nonzero `@r`**. tidal-cli gets it right:
`const repeat = m[2] ? parseInt(m[2]) + 1 : 1;` (`playback.ts:126-132`). python-tidal's
`1 + 1 + …` fudge only happens to land close to correct on a typical single-run MPD; it is not a safe
pattern to copy.

**The spec rule to implement, ignoring both references: total segments in one `<S d="…" r="N"/>` run
= `N + 1`; `@r` defaults to 0; `SegmentTemplate@startNumber` defaults to 1.**

**Neither reference client implements the full `SegmentTemplate` identifier set — implement it from
ISO/IEC 23009-1 §5.3.9.4.4, not from either client.** URL templates may use `$$` (literal `$`),
`$RepresentationID$`, `$Number$`, `$Bandwidth$`, and `$Time$` — each optionally with a printf-style
width tag (`$Number%05d$`, `$Time%011d$`). `$Time$` replaces `$Number$` when driven by a
`SegmentTimeline`: its value is the segment's `S@t` (start time in timescale units), a different
substitution rule, not a formatting variant. A grep for `RepresentationID|\$Bandwidth\$|\$Time\$|%0[0-9]d`
across the whole reference set finds zero hits — every client does a bare
`.replace('$Number$', str(i))`. Also unhandled anywhere: `SegmentTemplate` inherited from `Period`/
`AdaptationSet` level rather than declared on the `Representation` itself (python-tidal only ever
reads `representations[0].segment_templates[0]`, so it would silently produce nothing if TIDAL ever
puts the template one level up). **Use a real XML parser (`dash-mpd` for Rust) with full identifier,
format-tag and inheritance handling — never a regex or a single hardcoded substitution — and validate
it against the §16 fixture set before trusting it live.** **[verified for the reference-client
absence and the ISO identifier list; whether TIDAL's MPDs ever actually use `$Time$`/`$Bandwidth$` is
unknown]**

**Seek arithmetic, stated explicitly (the report only names this as a policy, not as maths):** for
segment index `k`, start time `t_k` = the previous segment's end (or
`SegmentTemplate@presentationTimeOffset`, default 0, for `k=0`); `$Number$` = `startNumber + k`;
segment `k` covers `[t_k, t_k + d)`. This gets a seek to a **segment boundary** — sample-accurate
seeking additionally requires decoding and discarding from the segment start, and the report does not
say this is required, so a naive Design-C seek would silently be coarse to a segment length (often
several seconds), not sample-accurate. Reference clients disagree on which to accept: Sone/High Tide
seek with `FLUSH | KEY_UNIT` (segment-boundary snap: `ref:sone/src-tauri/src/audio.rs:2330,2353`,
`ref:high-tide/src/lib/player_object.py:815-816`); Strawberry uses `FLUSH` alone (accurate:
`ref:strawberry/src/engine/gstenginepipeline.cpp:2370`). Pick one for streamboat and record the
reason. After any hand-rolled seek, **the init segment must be re-fed before the new media segment** —
state this as a hard requirement, not an aside. **Duration is ambiguous too**, and at least three
consumers need it to agree: the track object's integer `duration` (seconds), `MPD@mediaPresentationDuration`,
and the decoded sample count can disagree by up to a second; prefetch-trigger arithmetic, MPRIS
`mpris:length`, and the 30-second play-report threshold (`references/playback-behavior.md` §15) all
depend on one authoritative source. Use the track object's `duration`; treat MPD duration disagreement
as a signal the manifest is suspect, not as a tiebreaker.

python-tidal also synthesises an HLS playlist from the parsed MPD (`DashInfo.get_hls`), emitting
`#EXTINF` values of `S.d / timescale` — a useful trick for handing a TIDAL DASH track to any
HLS-capable player.

**Container.** `AdaptationSet@mimeType="audio/mp4"` + `codecs="flac"` = FLAC inside fragmented MP4
(the `fLaC` sample entry + `dfLa` FlacSpecificBox). FFmpeg maps `MKTAG('f','L','a','C')` →
`AV_CODEC_ID_FLAC` (`isom_tags.c`) and reads `dfLa` (`mov_read_dfla` in `mov.c`); GStreamer's
`qtdemux` has `FOURCC_fLaC` and emits `audio/x-flac` caps. See `references/decoding-and-codecs.md`
for the version floor this needs on the DASH side.

**A third MPD shape — bare `<BaseURL>`, no SegmentTemplate at all.** tidal-cli has a documented
fallback: after failing to find `initialization=`/`media=`/`<S d=…>`, it tries
`decoded.match(/<BaseURL>([^<]+)<\/BaseURL>/)` and, if present, returns
`{ type: 'direct', url: baseUrlMatch[1], codecs }` — handled identically to a BTS manifest
(`ref:tidal-cli/src/playback.ts:100-152`, union type `'direct' | 'dash'`). **Parse this as a
fallback MPD shape or a hand-rolled assembler will throw "unable to parse manifest" on whatever
fraction of the catalogue comes back this way.**

**Byte ranges.** No reference client uses DASH `SegmentBase`/`indexRange` — but that absence is
**weaker evidence than it looks**: neither reference parser is a real XML parser (tidal-cli matches
the MPD with plain regexes; python-tidal blindly indexes `[0]` at every level and raises
`ManifestDecodeError` otherwise), so an MPD that used `SegmentBase`, or even an ordinary
`<S t="0" d="…" r="…"/>` with a `t` attribute, would produce zero segments from tidal-cli and a hard
failure from python-tidal. **Their silence is evidence about their parsers' narrowness, not about
what TIDAL sends — do not read "no reference client uses it" as "TIDAL doesn't send it."** Only a
captured fixture (§16) actually settles this. HTTP `Range` on **direct** (BTS/BaseURL) URLs is real
regardless, and is how seeking works on those: mopidy-tidal's cache proxy implements full
`Range`/`Content-Range` handling for exactly this reason
(`ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/types.py:12-40`, `proxy.py:219`). Support `Range` on
direct URLs so a seek does not re-download from byte 0.

## 5. Encryption — the refusal rule

`encryptionType` in a BTS manifest takes values `NONE` and `OLD_AES`
(`ref:TidaLuna/plugins/lib.native/src/request/decrypt.ts:41-52`). python-tidal defaults
`encryption_type="NONE"` for MPD, reads the field for BTS (`media.py:660,672`). **Caveat: `OLD_AES`
is attested by exactly one file in the whole 21-project reference set** — it is not a proven-exhaustive
vocabulary. This is exactly why the refusal rule below is a denylist-of-one ("anything not `NONE`"),
not an allowlist of known-safe values — do not special-case `OLD_AES` as "the encrypted one" and treat
anything else as safe.

**Strawberry's rule — copy this exactly.** Refuse and say so when any of these hold
(`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:244-310`):
`encryptionType` non-empty and not `NONE` (:244-250); `encryptionKey` non-empty (:294-300);
`securityType` non-empty and not `NONE` with a `securityToken` present (:303-310). Message: the
stream is protected, and *"whether Tidal delivers encrypted streams depends on the client ID in
use. Try changing the Client ID in the Tidal settings."*

Sone reaches the same conclusion from the other side: it skips both hi-res tiers entirely when no
`client_secret` is configured — *"those credentials typically return encrypted DASH streams that
require Widevine"* (`ref:sone/src-tauri/src/commands/playback.rs:53-58`).

DRM endpoints (DASH/HLS path): Widevine at `https://api.tidal.com/v2/widevine`; FairPlay license at
`https://fp.fa.tidal.com/license`, certificate at `https://fp.fa.tidal.com/certificate`
(`ref:tidal-sdk-web/.../shakaPlayer.ts:645,661,667`). v2 manifest carries `drmData.licenseUrl`.

**streamboat must never implement `OLD_AES` decryption.** TidaLuna's decryptor with a hardcoded
master key runs *inside the licensed official client* — copying it into an independent player makes
streamboat undistributable and is exactly the DRM-circumvention posture the owner has ruled out.

## 6. Video

`GET /v1/videos/{id}/playbackinfopostpaywall` or `/v1/videos/{id}/urlpostpaywall` (M3U8), or v2
`/videoManifests/{id}`. Sone plays video through a completely separate path (hls.js in its webview)
and states: *"Video audio is streamed and does not use the bit-perfect lossless signal path that
music tracks use"* (`ref:sone/README.md:501`). Video is an open owner decision — see SKILL.md.

## 7. Error handling, sub-statuses, and the quality cascade

A playbackinfo failure is HTTP 401 with `status`/`subStatus`. The canonical terminal/recoverable
sub-status table is owned by `tidal-api/references/transport.md` §6 — do not restate it here;
`4006` (streaming privileges lost) and `4033` (subscription up-sell) recover, the rest of the
4xxx range is terminal for that request.

A 401 in the playbackinfo sub-status range must **not** trigger a token refresh + retry — the token
is fine, the content is not (`ref:sone/src-tauri/src/tidal_api.rs:1522-1531`).

TIDAL's web SDK's user-facing mapping (`playback-info-resolver.ts`, `getErrorId`):

| subStatus / status | Meaning |
|---|---|
| 4010 | `PEMonthlyStreamQuotaExceeded` |
| 4032, 4035 | `PEContentNotAvailableInLocation` |
| 4033 | `PEContentNotAvailableForSubscription` |
| HTTP 5xx or 429 | `PERetryable` |
| other 4xx | `PENotAllowed` |

**Quality cascade.** Sone tries tiers highest→lowest, stopping early on error classes a lower tier
cannot fix (`ref:sone/src-tauri/src/commands/playback.rs:48-90`): network errors propagate
immediately; rate-limited/terminal-unplayable propagate immediately; other errors are remembered and
the cascade continues. **The stated reason — "over-requesting quality returns 200 with a downgraded
`audioQuality`, never an error" — is `[unverified]`**: it's Sone's own code comment with no second
source, and it's in tension with tidalt's own descending ladder
(`HI_RES_LOSSLESS → LOSSLESS → HIGH → LOW`, `ref:tidalt/docs/architecture.md:17`), which only makes
sense if over-requesting sometimes *does* fail. Do not assume the downgrade-not-error behaviour
without observing it directly. TIDAL's own SDKs do **not** cascade — they send the whole `formats`
array once and read back what they got. Prefer that when using v2 (subject to the v2-reachability
question in §11).

**Open decision, not yet settled: should streamboat's ladder include the legacy `HI_RES` tier at
all?** Sone's shipped `ORDER` array still requests `[HI_RES_LOSSLESS, HI_RES, LOSSLESS, HIGH]`, even
though `HI_RES` (MQA) content was retired 24 July 2024 (§1, §atmos-and-immersive). No reference code
comment says what a live `audioquality=HI_RES` request returns today post-retirement — downgraded
FLAC, an error, or the same answer as `LOSSLESS`. Including it costs a wasted request per track if it
silently downgrades; dropping it costs nothing, since `HI_RES_LOSSLESS`/`LOSSLESS` already bracket it.
Settle with one live request per tier, the same fixture-capture task as §16.

**Rate limiting.** Sone's global cooldown gate: on 429, parse `Retry-After` (delta-seconds form
only — the HTTP-date form is rejected, not mis-parsed), clamp to `[1, 120]` s, default 5 s, store an
absolute deadline via `fetch_max` so concurrent 429s can only lengthen the cooldown
(`ref:sone/src-tauri/src/rate_gate.rs`). TIDAL's web SDK retries `500 * (4 - retriesRemaining)` ms,
up to 3 retries, only on network/5xx/429. Shaka's own config:
`{backoffFactor: 2, baseDelay: 1000, fuzzFactor: 0.5, maxAttempts: 5, timeout: 5000}`.

## 8. `streamingSessionId` — client-generated, not server-issued

§1.2/§4.8 of the report cite the `x-tidal-streamingsessionid` (v1) and `x-playback-session-id` (v2)
headers without saying who creates the value. **The client generates it.** TIDAL's web SDK builds a
v4 GUID from `crypto.getRandomValues`
(`ref:tidal-sdk-web/packages/player/src/internal/helpers/generate-guid.ts`), threaded as
`streamingSessionId` into both the v1 header (~line 218 of `playback-info-resolver.ts`) and the v2
header (~lines 365/450). **One id per media-product playback**, created before the manifest
request, reused for that same product's prefetch, echoed back in the v1 response body. TIDAL's SDKs
key their entire `streaming_metrics` event set (`playback_info_fetch`,
`streaming_session_start`/`_end`, `playback_statistics`, `drm_license_fetch`) on this id — if
streamboat sends any play-reporting at all, this is the join key, and reusing one id across tracks
or omitting it will produce broken reporting.

## 9. `mediaMetadataTags` / `audioModes` vocabulary

See `tidal-oss-landscape/references/api-auth-streaming.md` §7 for the canonical
`HI_RES_LOSSLESS`(enum)-vs-`HIRES_LOSSLESS`(tag) naming-mismatch warning and mopidy-tidal's
pre-flight check pattern — cite it rather than restating that warning. python-tidal has the
exhaustive vocabulary:

```python
class MediaMetadataTags(str, Enum):
    hi_res_lossless = "HIRES_LOSSLESS"
    lossless        = "LOSSLESS"
    dolby_atmos     = "DOLBY_ATMOS"
```

(`ref:python-tidal/tidalapi/media.py:87-95`). Values arrive on the **track** object, not the
manifest: `self.media_metadata_tags = json_obj.get("mediaMetadata", {}).get("tags", {})` (line 362),
consumed as `Track.is_hi_res_lossless`/`is_lossless` (from tags) and `Track.is_dolby_atmos` (from a
separate `audioModes` array, checking `AudioMode.dolby_atmos in self.audio_modes`) at lines 529-557.
No `SONY_360RA` tag, no MQA tag — consistent with both being gone. **Practical use:** clamp the
requested `formats` array to what the track actually offers before calling the manifest endpoint,
and render HIRES_LOSSLESS/DOLBY_ATMOS badges from these fields rather than a manifest round-trip.

## 10. Quality-tier transitions mid-stream (ABR)

With `adaptive=true` on v2, the MPD carries multiple Representations and the player's ABR switches
between them (TIDAL's web SDK: `abr: {enabled: true}`, reports every switch as an `adaptation`
event). Default config is `audioAdaptiveBitrateStreaming: true` with
`streamingWifiAudioQuality: 'LOW'`. With `adaptive=false` (what tidal-cli requests) there is exactly
one Representation, no mid-stream switching. **For a bit-perfect client, ABR is actively harmful** —
a representation change mid-track means a format/rate change means a device reopen means a gap.
**Request `adaptive=false`.** Changing the user's quality ceiling takes effect on the next track
only (it changes the `formats` array on the next manifest request); no reference client attempts a
mid-track tier change.

## 11. Open question: is v2 reachable to an unofficial client at all?

Every reference project using v2 (tidal-sdk-web, tidal-sdk-android, tidal-cli) authenticates as a
**registered TIDAL developer-portal application**. Every unofficial client (High Tide, Sone,
Strawberry, python-tidal, tidalt, mopidy-tidal, TidaLuna, tidal-hifi) uses v1 exclusively — none of
them touch `openapi.tidal.com/v2/trackManifests`. This split may be a coincidence of what each
project's author happened to build against, or it may be a hard access boundary TIDAL enforces by
client credential. **This is not verified either way**, and it directly determines whether "prefer
v2" (a major recommendation of the parent report) is buildable at all — if v2 needs a
developer-portal grant an unofficial player cannot get, the v1 quality cascade in §7 is not a
fallback, it is the only path. Confirm this against the live API with streamboat's actual client
credentials before committing architecture to v2.

## 12. HLS and EMU manifests need an explicit decision, not silent failure

§3 lists all four MIME types but a naive implementation only handles BTS and DASH — and
python-tidal, the most-copied unofficial client, has EMU and APPL (HLS) **commented out** of its
`ManifestMimeType` enum and raises `UnknownManifestFormat` on anything else
(`ref:python-tidal/tidalapi/media.py:112-120`). Since manifest type is DRM/client-driven (§2 — HLS
if FairPlay is supported, else DASH) and streamboat's client-ID choice is an open owner decision, a
credential that makes TIDAL answer with `application/vnd.apple.mpegurl` is otherwise an undesigned
total playback failure. **Decide it now:**

- **EMU**: parse with the exact same code path as BTS. TIDAL's own web SDK's manifest parser handles
  both with the same `parseJSONManifest` function (§3) — this is free, not separate work.
- **HLS**: either implement the web SDK's double-base64 variant decode (§3), or fail with a specific,
  actionable error naming the client ID as the likely cause — never a generic "unsupported manifest"
  message that gives the user nothing to act on.

## 13. PREVIEW / `previewReason` — a required behaviour, not an edge case

Both v2 SDKs surface `trackPresentation`/`assetPresentation` as first-class: the Android SDK maps
`TrackPresentation.PREVIEW` and `PreviewReason.{SUBSCRIPTION, PURCHASE, HIGHER_ACCESS_TIER}`
(`ref:tidal-sdk-android/.../PlaybackInfoRepositoryDefault.kt`), and the web SDK **defaults
`assetPresentation` to `'PREVIEW'` when the attribute is absent from a v2 response** — the opposite
default from v1, which defaults to `FULL`. A player that ignores this silently plays a 30-second clip
as though it were the full track and, if it sends play-reporting (§8, and see
`references/playback-behavior.md` §7), reports it to TIDAL as a full play — worse than an error.

**Required behaviour:**

- Treat `assetPresentation != FULL` as its own playback outcome with its own UI state — not a generic
  error and not silently played as if full.
- Suppress play-reporting for a preview: it must never cross the 30-second "counts as a stream"
  threshold as if it were the real track.
- Surface `previewReason` verbatim (`SUBSCRIPTION` / `PURCHASE` / `HIGHER_ACCESS_TIER`) — each implies
  a different corrective action (upgrade plan / buy the track / raise quality tier). Only the exact UI
  copy per reason remains genuinely open.

## 14. `countryCode` provenance and its diagnostic implication

§2's canonical v1 URL includes `countryCode`, and Sone passes it explicitly
(`ref:sone/src-tauri/src/tidal_api.rs:3665`) — but python-tidal's `get_stream` sends only
`playbackmode`/`audioquality`/`assetpresentation`; `countryCode` is injected by its request layer
from the **session**, not passed per call site. A client that omits it, sends a stale value, or
derives it from OS/browser locale instead of the account's actual country gets region-limited
results that surface as sub-status 4032/4035 (`PEContentNotAvailableInLocation`, §7) — which is
**terminal**, i.e. "skip this track, don't retry." **A misconfigured `countryCode` therefore looks
identical to "the whole catalogue is region-locked."** Specify `countryCode`'s provenance as the
session/user profile (never OS/browser locale), inject it once in the request layer rather than per
call site, and add a diagnostic that distinguishes "this specific track is genuinely region-locked"
from "our `countryCode` is wrong" before treating 4032/4035 as unconditionally terminal.

## 15. Streaming-privileges WebSocket reconnect policy

Getting this wrong is maximally user-visible: either streamboat silently loses the stream to
another device with no message, or it fights another device for the privilege in a loop. Full
protocol (message vocabulary, reconnect/backoff, token-rebinding, and the desktop-vs-headless
priority question) is owned by
`headless-and-tidal-connect/references/daemon-architecture.md` §6 — cite it rather than restating.

## 16. Capture real TIDAL manifest fixtures — do this before writing a parser

No captured TIDAL MPD or BTS response exists anywhere in the 21 reference checkouts — every DASH
structural claim in §4 (single-Period/AdaptationSet/Representation, the literal
`AdaptationSet@mimeType` value, the absence of `startNumber`, `Representation@id` shape) is
parser-derived, not observed. That is exactly why python-tidal and tidal-cli can disagree about
segment numbering with neither demonstrably wrong. **Make this the first engineering task, ahead of
any parser code:** on first successful login, dump one BTS and one DASH manifest per tier
(`LOW`/`HIGH`/`LOSSLESS`/`HI_RES_LOSSLESS`) plus one Dolby Atmos and one `PREVIEW`-asset response to
`tests/fixtures/`, redact the CDN tokens, and drive every parser test off them from day one. One step
converts §4's `startNumber`/`@r`/`mimeType` open questions from open to closed and gives CI something
concrete to fail on when TIDAL changes its manifest shape.
