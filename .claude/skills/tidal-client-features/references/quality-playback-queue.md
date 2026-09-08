# Quality, playback and queue mechanics

Table of contents:
1. Quality ladder and the naming trap
2. Where quality is chosen / what's actually playing
3. Manifest fetch, the quality cascade, and terminal error codes
4. Transport and player state
5. Queue semantics
6. Audio output: exclusive mode, Force Volume, device enumeration
7. Crossfade, gapless, autoplay
8. Normalization / ReplayGain
9. Voice commands
10. Failure/notification model
11. Settings surface (desktop)
12. Platform notes: web player limits, no Linux desktop client

All facts below carry the same version caveat as the rest of this skill: sourced from TidaLuna
`v1.16.6-beta` (commit `d8cd6bc`, 2026-09-02) unless otherwise noted.

**Provenance note for every `ref:TidaLuna/...` citation below**: only
`plugins/lib/src/redux/types/*` is a verbatim `Object.keys(luna.core.buildActions)` export from the
shipping official TIDAL client — the strongest evidence tier this skill uses.
`plugins/lib/src/classes/*` is different: it is TidaLuna's **own** plugin/mod code, which imports
`@luna/core` and `@inrixia/helpers` and merely *consumes* those redux types. Treat anything sourced
from `classes/*` (concurrency limits, display names/labels, badge colours, request-retry logic) as
one modder's implementation choice, not attested official-client behaviour — it is directionally
useful (it had to work against the real API to ship) but not the same evidence tier as the redux
type dump.

## 1. Quality ladder and the naming trap

Source of truth: `ref:TidaLuna/plugins/lib/src/classes/Quality.ts`. Seven ordered rungs (`idx`
0–6), each with a name and badge colour, and **two separate lookup tables that disagree with each
other at the bottom two rungs** — this is the trap. Per the provenance note above, `Quality.ts` is
`classes/` code, so the rung **names** ("HiRes", "Sony630", "Low", "Lowest") and **badge colours**
below are TidaLuna's own labels, not confirmed TIDAL UI strings — treat the "UI …" annotations
(sourced separately from live DOM/UI observation) as the actual UI-facing claim. The two **lookup
tables** (`audioQuality` enum and `mediaMetadata.tags` values, both `redux/types` wire enums) are
the solid part, independently corroborated by `tidal-sdk-web`'s `audioQualityToFormats` below:

| `idx` | Name | Badge colour | `audioQuality` enum → this rung | `mediaMetadata.tags` → this rung | Codec/format | Ceiling |
| --- | --- | --- | --- | --- | --- | --- |
| 6 | HiRes (UI "Max") | `#ffd432` | `HI_RES_LOSSLESS` | `HIRES_LOSSLESS` | FLAC (`FLAC_HIRES`) | 24-bit / up to 192 kHz |
| 5 | MQA | `#F9BA7A` | `HI_RES` | `MQA` | MQA | Removed 24 Jul 2024 |
| 4 | Atmos | `#6ab5ff` | — | `DOLBY_ATMOS` | EAC3-JOC / AC-4 | audioMode `DOLBY_ATMOS` |
| 3 | Sony630 | `#6ab5ff` | — | `SONY_360RA` | Sony 360RA | Removed 24 Jul 2024 |
| 2 | High (UI "High") | `#33FFEE` | `LOSSLESS` | `LOSSLESS` | FLAC | 16-bit / 44.1 kHz |
| 1 | Low (UI "Low") | `#b9b9b9` | **`HIGH`** | — | AAC-LC (`AACLC`) | ~320 kbps |
| 0 | Lowest (no 2026 UI label) | `#b9b9b9` | **`LOW`** | — | HE-AAC v1 (`HEAACV1`) | ~96 kbps |

**Read the bottom two rows carefully**: wire enum `audioQuality: "HIGH"` maps to the rung named
"Low", and `audioQuality: "LOW"` maps to "Lowest", which has no current UI label. The 2026 UI string
"Low" is backed by enum `HIGH`, not `LOW`. Never map UI strings to enums by string-matching an
English word — encode this table once and use it everywhere.

`Quality.max`/`Quality.min` operate on the numeric `idx`. When an item carries several
`mediaMetadata.tags` (e.g. both `DOLBY_ATMOS` and `LOSSLESS`), the client takes the max `idx` as the
displayed quality — that's the precedence rule for badge rendering.

Format-array mapping (`audioQualityToFormats`,
`ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts:286-299`):

```
HI_RES / HI_RES_LOSSLESS → [HEAACV1, AACLC, FLAC, FLAC_HIRES]
LOSSLESS                 → [HEAACV1, AACLC, FLAC]
HIGH                      → [HEAACV1, AACLC]
LOW                       → [HEAACV1]
```
The inverse (`audioFormatsToQuality`) is at the same file, lines 263-284.

MQA and Sony 360 Reality Audio were both removed from TIDAL on **24 July 2024** (announced 17 June
2024); the legacy enum values still exist in the type system for old catalogue data. Dolby Atmos
(EAC3-JOC/AC-4) remains, mobile/TV/car only — not on desktop (confidence ~0.85; see the main
report's Open Questions).

## 2. Where quality is chosen / what's actually playing

Chosen: `settings.quality.streaming: AudioQuality`, action `settings/SET_STREAMING_QUALITY`, a
`SELECT_SOUND_QUALITY` context menu, telemetry `eventTracking/CHANGE_STREAM_QUALITY`. Mobile splits
further into Mobile-data streaming / Wi-Fi streaming / Download quality (three independent
settings).

Actually playing: `playbackControls.playbackContext` carries `actualAudioQuality`,
`actualAudioMode`, `actualAssetPresentation`, `actualStreamType`, `actualVideoQuality`, `bitDepth`,
`sampleRate`, `codec`, `playbackSessionId`, `actualDuration`, `assetPosition`
(`ref:TidaLuna/plugins/lib/src/redux/types/store/Playback.ts`). tidal-hifi surfaces exactly this as
its `/current/audio-quality` REST endpoint: `{quality, badgeText, bitDepth, sampleRate, codec}`
(`ref:tidal-hifi/src/models/audioQuality.ts`). The UI quality badge selector is
`*[data-test^="quality-badge-"]`.

## 3. Manifest fetch, the quality cascade, and terminal error codes

**v1 (unofficial API, what High Tide/Sone/python-tidal use):**

```
GET https://api.tidal.com/v1/tracks/{id}/playbackinfopostpaywall
    ?audioquality=HI_RES_LOSSLESS|LOSSLESS|HIGH|LOW
    &playbackmode=STREAM
    &assetpresentation=FULL
    &countryCode=XX
```
(`ref:python-tidal/tidalapi/media.py:507-516`; also `urlpostpaywall` with `urlusagemode=STREAM` for
a direct URL, used by `tidalt`/`tidalrs`.) TidaLuna's own client-side API helper calls
`https://desktop.tidal.com/v1/tracks/{id}/playbackinfo` with the same query shape and headers
`Authorization: Bearer` + `x-tidal-token` — evidence of the host the official client's requests
go to, not of the client's own internals — wrapped in a **self-imposed semaphore capping it to 2
concurrent requests** (`ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:51-52`, `// Lock to
two concurrent requests` / `new Semaphore(2)`). Per the provenance note above, that concurrency
cap is **TidaLuna's own rate-limiting choice**, not an attested TIDAL server-side or
official-client constraint — do not cite "2" as a TIDAL-imposed limit.

Response carries `manifestMimeType` (`application/dash+xml` or `application/vnd.tidal.bts`,
historically also `application/vnd.tidal.emu`), base64 `manifest`, `manifestHash`, `audioQuality`,
`audioMode`, `bitDepth`, `sampleRate`, plus four ReplayGain/peak fields (§8). BTS manifests
base64-decode to JSON `{mimeType, codecs, encryptionType, keyId, urls[]}`; DASH manifests decode to
MPD XML.

**v2 (official SDK path):**

```
GET /trackManifests/{id}
    ?formats=[HEAACV1,AACLC,FLAC,FLAC_HIRES]
    &manifestType=HLS|MPEG_DASH
    &uriScheme=DATA
    &usage=PLAYBACK
    &adaptive=
    &shareCode=
```
plus an `x-playback-session-id` header. `manifestType` is chosen as `HLS` when FairPlay is
supported, else `MPEG_DASH`. `shareCode` is optional — it carries the share-link context when
playback was reached via a shared link, present on every official-SDK call whether or not the
caller has one (`ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts:361-378`,
destructured from `mediaProduct.shareCode`). Manifests expire after **3,600,000 ms (1 hour)**
(`ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts:79,362-377`).

**Quality cascade**: request the highest tier, degrade on failure. **Terminal sub-statuses — the
literal list, not a range**: `4005, 4010, 4030, 4031, 4032, 4034, 4035`
(`ref:sone/src-tauri/src/tidal_api.rs:18`). Treat these as permanently unplayable and skip; retry
only transient errors.

**Playbackinfo sub-statuses occupy 4000–4999; auth failures use a separate namespace** (11002/11003
token, 6001 session, 1002 pending), so a 4xxx sub-status on a 401 response is never fixed by
refreshing the token (`ref:sone/src-tauri/src/tidal_api.rs:11-13`,
`PLAYBACKINFO_SUB_STATUS_RANGE = 4000..=4999`). Scope the classifier to the 4xxx range before
matching it against the terminal list below.

**Two sub-statuses in the same 4xxx range are deliberately excluded from that terminal list, and
must NOT evict the track** (`ref:sone/src-tauri/src/tidal_api.rs:15-18`, doc comment on
`TERMINAL_SUB_STATUSES`):
- **4006** — streaming privileges lost; recovers (`ref:sone/src-tauri/src/tidal_api.rs:15-18`).
  Almost certainly the same condition the desktop client surfaces as
  `player/STREAMING_PRIVILEGES_REVOKED` (§10) — a second device taking over the one-stream slot,
  with playback resuming once the user reclaims it — but that identification is **[inferred]**, not
  stated in either source. Treat as transient/recoverable either way, not a dead track.
- **4033** — subscription up-sell (the account needs a higher tier for this content). This is
  user-fixable (upgrade), not permanently unplayable — surface it as an account/entitlement message,
  not a "this track is broken" skip.

An error classifier that only encodes the seven-item terminal list and treats everything else in
the 4xxx range as "also terminal" will wrongly delete tracks on 4006/4033.

**Encryption**: manifests can be DRM-protected (FairPlay at `fp.fa.tidal.com/license`, Widevine at
`api.tidal.com/v2/widevine`) — that's the path the official web player and SDKs take. The
unofficial-API path reaches unencrypted FLAC/AAC manifests for the subscriber's own account.
**Strawberry explicitly refuses to play anything where `encryptionKey` is non-empty or
`encryptionType`/`securityType` != `NONE`**, showing a user-facing error instead
(`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:244-247,295-298,303-307`). **Whether TIDAL
delivers an encrypted or unencrypted stream at all is not a fixed property of a track — Strawberry's
own refusal message states it "depends on the client ID in use"**
(`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:246-248,297-299`). This changes how the
quality cascade and the refuse-cleanly path should be designed: encryption is a per-request
outcome of which client ID made the call, not a per-track fact you can cache once and reuse — see
`docs/research/tidal-api.md` for client-ID provenance. Copy Strawberry's posture regardless: detect
encryption and decline, never circumvent — this is both the correct legal stance and a clean
failure mode. Document it in the README so nobody files "add Widevine support."

**Region unavailability / media replacement**: the v2 spec exposes a `replacement` relationship on
`tracks`/`videos`/`albums` and a `replaceMedia=<relationship paths>` query param, with each
identifier carrying `meta.replacement: ORIGINAL|REPLACED|NOT_REPLACED` (flagged "BETA Internal
only" as of this reading). On v1, the equivalent signal is the per-item `StreamingFlags` (see the
`StreamingFlags` block at the end of this section)
plus `message/MEDIA_NOT_PLAYABLE` — the native app surfaces a message and moves on rather than
stalling on a dead track. `/usageRules` and `/tracks/{id}/relationships/usageRules` are the v2 home
for per-item entitlement rules.

**Every media item carries** `StreamingFlags { allowStreaming, streamReady, payToStream,
adSupportedStreamReady, djReady, stemReady, premiumStreamingOnly }`
(`ref:TidaLuna/plugins/lib/src/redux/types/store/content/StreamingFlags.ts`). `djReady`/`stemReady`
plausibly gate the DJ Extension add-on — this mapping is **[inferred]**, not stated in source.

## 4. Transport and player state

The state machine every implementer writes first — get this right before wiring the queue on top
of it. All facts below are `[verified-source]`
(`ref:TidaLuna/plugins/lib/src/redux/types/store/Playback.ts:10-11,45-46` for the state/enum shapes,
`ref:TidaLuna/plugins/lib/src/redux/types/actions/actionTypes.ts` for the action inventory).

**`playbackControls/*` actions**: `PLAY`, `PAUSE`, `TOGGLE_PLAYBACK`, `STOP`, `SKIP_NEXT`,
`SKIP_PREVIOUS`, `SEEK`, `SEEK_FORWARDS`, `SEEK_BACKWARDS`, `START_AT`, `TIME_UPDATE`,
`SET_DURATION`, `SET_VOLUME`, `INCREASE_VOLUME`, `DECREASE_VOLUME`, `SET_MUTE`, `TOGGLE_MUTE`,
`SET_VOLUME_UNMUTE`, `SET_PLAYBACK_STATE`, `SET_DESIRED_PAUSE_STATE`, `MEDIA_PRODUCT_TRANSITION`,
`PREFILL_MEDIA_PRODUCT_TRANSITION`, `UPDATE_PLAYBACK_CONTEXT`, `ENDED`.

**State shape**:

```
PlaybackState = "PLAYING" | "IDLE" | "PAUSED" | "NOT_PLAYING" | "STALLED"
Player = "BOOMBOX" | "GOOGLE_CAST" | "REMOTE_PLAYBACK"          // which engine renders audio
ActivePlayerType = "PLAYER_SDK" | "WEBPLAYER" | "EXTERNAL_PLAYER"  // ref:.../store/index.ts:64
volume: number       // 0-100, not 0-1
volumeUnmute: 100     // the level TOGGLE_MUTE restores to
```

**Five playback states, not a `playing: boolean`.** `STALLED` (buffering) and `NOT_PLAYING`
(nothing loaded) are distinct from `IDLE`/`PAUSED` — a naive boolean loses both: the buffering
state a loading spinner needs, and the cold-start "nothing loaded yet" state a fresh-launch UI
needs. `PlaybackControls` also carries `desiredPlaybackState` (what the user asked for) separately
from `playbackState` (what's actually happening) and `mediaProduct: {productId, productType,
referenceId, sourceId, sourceType}`. `PlaybackContext` (the "what's actually playing" struct) is
covered separately in §2.

Model streamboat's own playback state machine on this five-value enum from day one — retrofitting a
`STALLED` state into code built around a boolean is a rewrite, not a patch.

## 5. Queue semantics

State (`ref:TidaLuna/plugins/lib/src/redux/types/store/PlayQueue.ts`):

```
elements[]: { uid, mediaItemId, context, priority }
  priority ∈ { priority_history, priority_keep, priority_none }
currentIndex, repeatMode: {Off=0, All=1, One=2}
shuffleModeEnabled, lastShuffleSeed
originalSource[], backupElements[]/backupSource[]
sourceName, sourceTrackListName, sourceUrl, sourceEntityId, sourceEntityType
```

Operations: `ADD_NOW`, `ADD_NEXT`, `ADD_LAST`, `ADD_AT_INDEX`, `ADD_NOW_REST_OF_TRACK_LIST`,
`MOVE_TRACK {fromIndex,toIndex}`, `CLONE_TRACK`, `REMOVE_AT_INDEX`, `REMOVE_ELEMENT {uid}`,
`CLEAR_QUEUE`, `CLEAR_ACTIVE_ITEMS`, `SET_CURRENT_INDEX`, `MOVE_TO`, `MOVE_NEXT`, `MOVE_PREVIOUS`,
`SET_REPEAT_MODE`, `TOGGLE_REPEAT_MODE`, `TOGGLE_SHUFFLE`,
`ENABLE_SHUFFLE_MODE_AND_SHUFFLE_ITEMS {shuffleSeed}`, `DISABLE_SHUFFLE_MODE_AND_UNSHUFFLE_ITEMS`,
plus lazy list filling (`FETCH_FIRST_PAGE_AND_ADD_TO_QUEUE`,
`FETCH_REST_OF_THE_TRACKS_AND_ADD_TO_QUEUE` — playing a 10,000-track collection does not require
loading it first). Queue and player settings persist to local storage. Queue UI is a right-hand
aside (`view/TOGGLE_PLAY_QUEUE_VISIBILITY`).

**Shuffle is seeded (`shuffleSeed`) and reversible** — unshuffling restores the original order. Any
implementation that shuffles destructively will not match. **Play-next vs add-to-queue are distinct
insert positions** users test for immediately.

**Cloud queue (server-side, cross-device) uses a different vocabulary than the local queue —
map explicitly, don't assume they're the same enum.** Client-side: `cloudQueue/*` actions
(`CREATE_CLOUD_QUEUE`, `GET_CLOUD_QUEUE_ITEMS`, `ADD_ITEMS_TO_CLOUD_QUEUE`, `MOVE_TRACKS`,
`REMOVE_ELEMENT`, `SET_CURRENT_ITEM`, `SET_SHUFFLED`, `UPDATE_ITEMS_ETAG`,
`FILL_CLOUD_QUEUE_WITH_HISTORY`), `CloudQueue` state `{queueId, etag, itemsEtag, headPosition,
tailPosition, tailItemId, currentItemId, historyMediaItemIds, repeatMode, shuffled}`. Server-side
(v2 OpenAPI spec, `ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json`): `/playQueues` and
`/playQueues/{id}`, `PlayQueues_Attributes = {createdAt, lastModifiedAt, repeat:
NONE|ONE|BATCH, shuffle: OFF|BATCH|ALL, shuffled: boolean}`. No OSS client implements this yet, but
it is documented, not a black box.

## 6. Audio output: exclusive mode, Force Volume, device enumeration

State: `activeDeviceId`, `activeDeviceMode: "exclusive" | "shared"`, `availableDevices[]` of
`{id, name, nativeDeviceId, webDeviceId, type, controllableVolume}`, `desiredDeviceMode` (per
device), `forceVolume` (per device), `hasPreloadedNextProduct`. Actions
`player/SET_ACTIVE_DEVICE`, `player/SET_DEVICE_MODE`, `player/SET_FORCE_VOLUME`, context menu
`SELECT_SOUND_OUTPUT`.

**Exclusive Mode**: takes exclusive control of the output device, so volume must be changed in the
app rather than the OS mixer. It matches the source rate/depth (16/44.1, 24/96, 24/192) via
**WASAPI exclusive mode on Windows** (well attested). The **macOS mechanism is not attested** —
"Core Audio hog mode" is a plausible **[inferred]** guess repeated in some drafts; do not present it
as fact.

**Force Volume** (support.tidal.com, verbatim): "With Force Volume, Tidal keeps the app's volume at
the maximum level, allowing you to control the sound output through external devices like your DAC
or speakers." **This pins the in-app level at 100% so an external DAC/amp is the sole volume
control — it is the opposite of a software-volume fallback.** Exclusive Mode and Force Volume are
mutually exclusive in the UI (enabling one disables the other).

## 7. Crossfade, gapless, autoplay

**Crossfade**: confirmed shipping on **iOS and Web** — TIDAL Magazine, "What We're Working On. (And
Why.)" (June 2026): "Crossfade is once again available on iOS and Web… go to Settings and turn it
on," 0–12 second slider, corroborated by piunikaweb.com (23 Mar 2026, which puts the earliest
public sighting in March, ahead of TIDAL's own June Magazine post). Both sources put the
announcement in the **Mar–Jun 2026** window. Treat "iOS and Web" as the best-attested platform set
from these two sources, not necessarily an exhaustive one — neither source was cross-checked
against, say, Android release notes, so don't rule out a wider mobile rollout on the strength of
this citation alone. The TidaLuna desktop dump this skill is built on is
dated **2026-09-02 — three months after** that window, not before it. That makes the crossfade
toggle's absence from the desktop settings dump (§11) **real evidence the Electron desktop build
had not shipped it as of 2026-09-02**, not a "stale snapshot" artifact to explain away — a dump
this recent had every opportunity to carry a feature TIDAL had already announced for other
platforms months earlier. The only genuinely open question is desktop-client status: has it shipped
there since this dump was taken. Treat as parity work if built, not a differentiator, either way.

**Gapless**: implemented by preloading — `player/PRELOAD_ITEM`, `player/PRELOAD_NEXT_ITEM`,
`player/PRELOAD_SUCCESS`, `playbackControls.prefilled` (boolean), and
`playbackControls/PREFILL_MEDIA_PRODUCT_TRANSITION` (in the `playbackControls/` namespace, not
`playQueue/`). tidal-hifi 8.1.0 had to fix its position reading because "TIDAL's gapless playback
switches buffers" — direct evidence gapless is live in the web/desktop player.

**Autoplay**: `settings.autoPlay` + `settings/SET_AUTOPLAY`/`TOGGLE_AUTOPLAY`,
`content/LOAD_SUGGESTIONS`, a `suggestions` play-queue source type, `player/FORCE_AUTOMATIC_PROGRESSION`.
When the queue ends, TIDAL appends algorithmically suggested tracks. Not all TIDAL Connect devices
support Autoplay ([verified-web, unfetched]).

## 8. Normalization / ReplayGain

`settings.audioNormalization: "NONE" | "ALBUM" | "TRACK"`, action `settings/TOGGLE_NORMALIZATION`
(the *only* normalization action in the dump — no `SET_NORMALIZATION`, so the UI's cycling logic
isn't shown). ReplayGain data arrives with playbackinfo as `trackReplayGain`/`albumReplayGain` +
`trackPeakAmplitude`/`albumPeakAmplitude`. Sone's gain formula:
`0.8 * min(10^((rg+4)/20), 1/peak)` with album/track context switching (album context when playing
an album, track context otherwise).

**The commonly repeated "-14 LUFS, on by default on mobile" figure is [uncertain, possibly
stale]** — sourced only to 2019–2020 rollout coverage, not re-verified for 2026. Build against the
ReplayGain/peak fields (which are solid); don't hard-code the LUFS target as a real 2026 fact
without re-checking support.tidal.com.

Audio spectrum visualiser: `settings.audioSpectrumEnabled` / `settings/SET_AUDIO_SPECTRUM_ENABLED`
— a toggle in the desktop app, no other detail known.

## 9. Voice commands

`speech/{START_RECOGNITION, STOP_RECOGNITION, PARSE_VOICE_COMMAND}` with
`speech.recognitionSupported` — the desktop client has voice command support, not publicly
documented, likely the Web Speech API. Out of scope for streamboat; low value relative to cost.

## 10. Failure/notification model

Copy this as streamboat's own error/toast design rather than inventing one — third-party clients
are most visibly worse than the native app exactly at failure moments. The native client has:

- A connectivity namespace: `network/{CONNECT, CONNECTION_ESTABLISHED, CONNECTION_LOST,
  CONNECTION_REFRESH, FAILED_STARTUP}`.
- A typed notification model
  (`ref:TidaLuna/plugins/lib/src/redux/types/store/Notification.ts`):
  `Notification { id, category: MUTE|NETWORK|OTHER|PLAYBACK, severity: DEBUG|INFO|WARN|ERROR,
  message, data }`.
- Message actions: `message/{MESSAGE_INFO, MESSAGE_WARN, MESSAGE_ERROR, MEDIA_NOT_PLAYABLE,
  MESSAGE_BLOCK, MESSAGE_RELEASE, MESSAGE_DESKTOP_RELEASE, CLEAR_MESSAGE, CLEAR_MESSAGES}`.
- `player/STREAMING_PRIVILEGES_REVOKED` (server enforces one stream at a time; a second device
  kicks the first — must be *handled* even if you don't subscribe to the underlying websocket) and
  `remotePlayback/CONNECTION_LOST`.

This is enough to specify streamboat's whole error surface as an explicit parity target.

## 11. Settings surface (desktop)

The complete persisted settings state
(`ref:TidaLuna/plugins/lib/src/redux/types/store/index.ts`):

```
SettingsState {
  audioNormalization: "NONE" | "ALBUM" | "TRACK",
  audioSpectrumEnabled: boolean,
  autoPlay: boolean,
  desktop: { autoStartMode: 0 | 1, closeToTray: boolean },
  explicitContentEnabled: boolean,
  language: string,
  openLinksInDesktopApp: boolean,
  quality: { streaming: AudioQuality },
  updateAvailable: boolean,
  urls: { accessibility, betaParticipationAgreement, earlyAccess, privacy, support, terms }
}
```

`explicitContentEnabled` pairs with a confirmation modal (`modal/SHOW_EXPLICIT_TOGGLE_MODAL`) —
toggling it isn't a bare boolean flip in the UI, it's a guarded action. Filtering itself is
client-side only in practice (SKILL.md "Open decisions" #8).

**Adjacent user-facing settings surfaces that live outside the `settings` slice** — don't miss
these when building a Settings screen from the block above alone:
- Sound output device + exclusive/shared mode: `player.*`, context menu `SELECT_SOUND_OUTPUT` (§6).
- Force Volume per device: `player.forceVolume`, `player/SET_FORCE_VOLUME` — mutually exclusive
  with exclusive mode in the UI (§6).
- Last.fm connection: `lastFm/{LOGIN, DISCONNECT, REFRESH_SESSION, SET_CONNECTION_STATE}`, its own
  route `route/LOADER_DATA__LASTFM` — TIDAL treats this as headline enough to give it a dedicated
  screen, not just a settings toggle (see `entitlements-tiers-history.md` §7 for auth, and SKILL.md
  "Open decisions" #5 for the v1-vs-later call).
- Keyboard-shortcut cheatsheet: `modal/SHOW_SHORTCUTS` (`remote-playback-connect-controls.md` §6).
- Logout: `modal/SHOW_LOGOUT_MODAL`.
- Feature-flag user overrides: `featureFlags/TOGGLE_USER_OVERRIDE` — an internal/EAP surface, not a
  user-facing setting to replicate.
- The blocked-items page (`route/LOADER_DATA__BLOCKS`) — the Block action itself is in
  `library-playlists-collections.md` §5.
- Desktop lifecycle/update UX (minor, but the native app's parity target if streamboat ships an
  auto-updater): `modal/SHOW_DESKTOP_RELEASE_NOTES`, `session/TOGGLE_SHOW_DESKTOP_RELEASE_NOTES`,
  `message/MESSAGE_DESKTOP_RELEASE` (in-app release notes on update, alongside the
  `updateAvailable` flag above) and `modal/SHOW_UNSUPPORTED_OS_MODAL` /
  `session/ACKNOWLEDGE_OUTDATED_OS` (a warn-and-continue path for OS versions TIDAL no longer
  supports) — `ref:TidaLuna/plugins/lib/src/redux/types/actions/actionTypes.ts`.

**Absent from this settings dump, notably: no crossfade toggle, no equalizer, no gapless toggle, no
cache-size control, no download/offline settings.** Apply the standing version caveat here
specifically, but get the direction right: TIDAL announced a crossfade toggle for iOS/Web in
Mar–Jun 2026 (§7), and this dump is dated 2026-09-02 — **three months later, not earlier** — so "no
crossfade toggle" here is real evidence the Electron desktop build hadn't shipped it as of that
build, a genuine platform gap, not a dating artifact that explains the absence away. Only desktop
status stays open (has it shipped since 2026-09-02). The rest (equalizer,
cache-size control, download/offline settings) has no counter-evidence and can still be treated as
genuinely absent from the desktop client as of this reading.

## 12. Platform notes: web player limits, no Linux desktop client

**There is no official TIDAL desktop client for Linux.** Windows and macOS get an Electron desktop
app; Linux users run `listen.tidal.com` directly or a wrapper (tidal-hifi, Sone, High Tide) — this
is precisely why those wrappers, and streamboat itself, exist. tidal-hifi's own README states it
exists because "Linux support [was] lacking."

**Desktop and web are the same React+Redux app.** tidal-hifi's `ReduxController` reads
`playbackControls.playbackContext.actualAudioQuality`/`bitDepth`/`sampleRate`/`codec` from the *web*
player and gets identical fields to the desktop client — the two share a store shape, so nearly
everything in this skill sourced from the desktop Redux dump (quality ladder, queue semantics,
content models) applies to the web player too. The gap between them is narrow and specifically
audio/OS-integration: no exclusive/bit-perfect output, no tray/autostart/close-to-tray, no
`tidal://` protocol handler on the web.

**The web player's sample rate is not trustworthy by default**: Chromium resamples all audio output
to 48 kHz unless the browser is launched with `--audio-output-sample-rate=192000`
(`ref:tidal-hifi/docs/audio-quality.md`) — a HiRes 24/192 stream silently becomes 48 kHz audio
without that flag. This is one more reason the manifest-based unofficial-API path (native decode,
not an embedded browser) is the more robust design for streamboat's bit-perfect-output goal (§6),
not merely a legal-posture choice.

**Widevine DRM is broken on Windows for the castlabs Electron build (error S6007)**, with no
workaround as of this reading (`ref:tidal-hifi/docs/known-issues.md`, which also notes "Tidal is
working on removing/changing DRM"). Relevant if streamboat ever considers embedding a web view for
playback — one more argument for the manifest/decode path this skill already recommends over
touching DRM at all (§3).
