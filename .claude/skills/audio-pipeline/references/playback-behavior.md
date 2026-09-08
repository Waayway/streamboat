# Playback behavior: gapless, crossfade, loudness, buffering, caching, resilience

Full narrative: `docs/research/audio-pipeline.md` §4, plus fact-check gap-fill in its §10.2,
§10.9, §10.10.

## Table of contents

1. Gapless — three designs, plus mpv
2. Crossfade
3. Prefetch
4. Loudness normalization / ReplayGain — and its bit-perfect interaction
5. Buffering strategy
6. On-disk cache, and the legal framing for it
7. Network resilience: manifest/token lifecycle, privileges websocket, play-reporting
8. Seeking within DASH
9. Network-stall behaviour in exclusive mode
10. CDN URL expiry and mid-track resume
11. Sample-rate-change audible artifacts and DAC settling
12. Device-hold policy across pause/idle

---

## 1. Gapless — three designs, plus mpv

**(a) `playbin3` + `about-to-finish` — High Tide.** Create `playbin3`, connect `about-to-finish`,
`gapless_enabled = True`; falls back to `playbin`/`gapless_enabled = False` if `playbin3` can't be
created. On `about-to-finish`, `GLib.idle_add(self.play_next, True)` sets the next URI on the same
playbin. **High Tide disables gapless entirely when the sink is `pipewiresink`**
(`ref:high-tide/src/lib/player_object.py:96-98,211-215`) — an explicit, still-`[unverified]`
incompatibility worth investigating before copying the pattern. Note this design needs GStreamer
>= 1.26.10 for TIDAL's FLAC-in-DASH via the newer demux path — see `decoding-and-codecs.md` §1.

**(b) `concat` with two prerolled branches — Sone. The more robust design; steal these details**
(`ref:sone/src-tauri/src/audio.rs:255-480,1785-1960`):

```
uridecodebin(track A) -> queue(A) -+
                                    +- concat -> audioconvert -> audioresample -> norm_vol -> user_vol -> autoaudiosink
uridecodebin(track B) -> queue(B) -+
```

- Branch queue: `max-size-time=15s`, `max-size-buffers=0`, `max-size-bytes=0` — decouples the next
  decoder from `concat`'s gate on the inactive pad so it can pre-buffer while the current track
  plays.
- `uridecodebin buffer-duration` 15 s for DASH, 5 s for BTS, `use-buffering=true`.
- **All pad-slot operations on `concat` funnel through one serialized executor thread** (an
  `AttachJob::{Attach,Detach}` mpsc) — concurrent request/release of request pads races otherwise.
- **Detach order matters:** unlink and `release_request_pad` on `concat` *before* nulling the bin —
  `concat` hard-blocks the inactive sink pad, and null-first teardown deadlocks.
- Advance detected by `concat.connect_notify("active-pad")`, verified **by pad identity**.
- Under `concat` with `adjust-base=true`, `query_position(TIME)` is already re-based per track — no
  offset arithmetic needed.
- A flush seek does **not** disturb the prerolled branch: `FLUSH_START`/`FLUSH_STOP`/`SEGMENT` reach
  only `concat`'s active sink pad. Sone's comment records this as empirically verified on GStreamer
  1.24.2 specifically — treat as one project's empirical note on one version, not an upstream
  guarantee, and re-verify on whatever GStreamer version streamboat ships.
- `gapless_supported()` is just `ElementFactory::find("concat").is_some()` — `concat` ships in
  coreelements, effectively always true. **This contradicts Sone's own README**, which claims
  gapless "requires GStreamer 1.24+" — the code comment explicitly says "no GStreamer 1.24 or
  uridecodebin3 requirement." Treat the README as stale if copying this design.
- **Gapless is normal-mode only in Sone** — the DirectAlsa (exclusive/bit-perfect) backend never
  gets a second prerolled branch (the worker gates dispatch on backend type). Bit-perfect and
  gapless are mutually exclusive **in Sone**. **streamboat should not give up here** — gapless in
  exclusive mode is achievable by keeping the PCM device open across same-format tracks and only
  reopening on a format change (what mpv calls `--gapless-audio=weak`, and what tidalt does) — see
  §12 below for the device-hold policy this needs.

**(c) Dual media elements / dual Shaka instances — TIDAL's own web SDK.**
`#GAPLESS_CROSSFADE_MS = 250`, `#GAPLESS_START_BEFORE_END_S = 0.25`
(`ref:tidal-sdk-web/packages/player/src/player/shakaPlayer.ts:100-120`) — a 250 ms micro-crossfade
between two pre-created `<video>` elements, triggered a duration-derived timer 0.25 s before the end
(`timeupdate` at ~4 Hz is only a fallback, since it can skip the 250 ms window). **TIDAL's own
reference web implementation is not sample-accurate gapless** — a native client using `concat` or
`--gapless-audio=weak` can beat it.

**(d) mpv.** `--gapless-audio=<no|yes|weak>`, default `weak`: keeps the device open, closes and
reopens if the decoder's output format changes. `yes` keeps the *first* file's format and resamples
everything else to it — exactly wrong for a hi-res client. **For streamboat on libmpv the setting
must be `weak`, never `yes`.**

## 2. Crossfade

TIDAL's own player config: `crossfadeInMs: number` — 0 (default, gapless) to a positive
crossfade-overlap value, doc-commented "max 15000" (`ref:tidal-sdk-web/.../config.ts`). The type and
default are directly verified; **the 15000 ms ceiling is `[unverified]` as an enforced limit** — no
clamping code against it was located in the player source.

Strawberry implements crossfade with a `QTimeLine` fader across two pipelines and **explicitly
refuses to crossfade when any pipeline is in exclusive mode** —
`AnyExclusivePipelineActive()` gates it, and a new exclusive pipeline waits for existing ones to
finish (`ref:strawberry/src/engine/gstengine.cpp:233-290,1178-1200`). This is the correct
relationship: exclusive device access means one pipeline at a time, so **crossfade and
exclusive-device mode are mutually exclusive by construction.** No unofficial TIDAL client in the
reference set implements crossfade at all.

## 3. Prefetch

TIDAL supports manifest prefetch explicitly: the legacy playbackinfo call takes an
`x-tidal-prefetch: true` header, and `StreamInfo` carries a `prefetched` boolean end to end; there's
also a `preload-request` SDK event. Sone prerolls the whole next branch (decoder + 15 s queue)
during the current track. Shaka/browser preload a second element. The proprietary native player
component has `preload(url, streamFormat, encryptionKey?)` / `cancelPreload()`.

**Segment concurrency — corrected finding.** TidaLuna's `Semaphore(2)` bounds concurrent **track**
fetches, not segment fetches, and carries no claim about matching official-client behaviour
(`ref:TidaLuna/plugins/lib.native/src/request/fetchMediaItemStream.ts:20-23`, wrapping the whole
per-track fetch call). Within one track's stream, DASH segments are fetched strictly
**sequentially**: a plain `for` loop over URLs, no concurrency at all
(`ref:TidaLuna/plugins/lib.native/src/request/fetchStream.ts:39-40`). **Copy "sequential per
track", not an arbitrary concurrency number.**

**On libmpv:** use `--prefetch-playlist=yes` (default `no`) alongside `loadfile <uri> append` —
without it nothing is actually fetched ahead of time despite the queue looking populated. mpv's own
docs warn this "can occasionally make wrong prefetching decisions" if the queue is reordered —
disable it (or rebuild the playlist) around a user reorder.

## 4. Loudness normalization / ReplayGain — and its bit-perfect interaction

**Canonical formula**, `ref:tidal-sdk-android/.../volume/LoudnessNormalizer.kt`:

```kotlin
fun getReducedGain(replayGain: Float, peakAmplitude: Float, preAmp: Int): Float {
    val replayGainLinear = 10.0f.pow((replayGain + preAmp) / 20)
    if (peakAmplitude <= 0) return min(replayGainLinear, 1.0f)
    return min(replayGainLinear, 1 / peakAmplitude)
}
```

`preAmp` defaults 4 (0 on TV); modes `NONE|TRACK|ALBUM` default `ALBUM`
(`ref:tidal-sdk-android/.../VolumeHelper.kt`).

The **web SDK hardcodes `peak = 1`** — it ignores peak amplitude entirely
(`ref:tidal-sdk-web/.../normalize-volume.ts`):

```ts
export function normalizeVolume(replayGain: number): number {
  const peak = 1; const preAmp = 4;
  return Math.min(Math.pow(10, (preAmp + replayGain) / 20), 1 / peak);
}
```

applied by multiplying the user's volume, skipping the update during a seamless transition so
crossfade isn't disturbed.

**Sone adds an extra 0.8 factor** on top: `0.8 * min(10^((rg+4)/20), 1/peak)`
(`ref:sone/src-tauri/src/commands/playback.rs:10-21`). Album/track gain selection is context-aware
(`use_track_gain` picks track with album fallback, or vice versa), applied **before** `play_url` so
there's no volume spike at track start. **Do not copy the 0.8 factor unless the owner deliberately
wants quieter-than-TIDAL output — TIDAL's own SDKs don't have it.**

**High Tide** does it entirely inside GStreamer:
`taginject ! rgvolume pre-amp=4.0 fallback-gain=-10 headroom=6.0 ! rglimiter ! audioconvert` —
pre-amp comment: *"the pre-amp value is set to match tidal webs volume."* Values of exactly `1.0`
are skipped as a "missing" sentinel, with the note *"Rather quiet album than broken eardrums"*
(`ref:high-tide/src/lib/player_object.py:196-207,580-608`).

No evidence TIDAL normalizes server-side — all three of its own SDKs apply gain client-side from
`replayGain`/`peakAmplitude`, and `NONE` mode means unity.

**mpv:** `--volume-gain=<db>` — *"applied on top of other volume and gain settings"*, range set by
`--volume-gain-min`/`-max` (default -96..+12 dB). Set this directly in dB from the formula above,
rather than converting to a percentage and setting `volume`. mpv's own `--replaygain` (default `no`,
correctly left off) reads tags from the file — TIDAL's fMP4 carries none. `--replaygain-clip`
defaults `no`, i.e. mpv already avoids clipping — worth mirroring in a hand-rolled formula.

**Bit-perfect mode silently disables ReplayGain too, not only the volume slider** — this is the
interaction Implication tables elsewhere miss. Sone's bit-perfect pipeline branch builds
**neither** the user-volume nor the ReplayGain (`norm_vol`) GStreamer element
(`ref:sone/src-tauri/src/audio.rs:2936-2948`, contrast normal mode at `:1833-1841`, which builds
both). **The correct rule: bit-perfect implies no user volume, no ReplayGain, no dither, no
resample — full stop.** The transparency panel should read "ReplayGain: bypassed (bit-perfect)"
rather than show a gain factor that was never applied. See `output-backends.md` §7 for the
hardware-mixer-volume escape hatch this motivates.

**Volume taper:** Sone uses a cubic curve (`amplitude = slider^3`, ~50 dB range); High Tide offers
an optional quadratic mapping (`volume^2`, read back as `volume^(1/2)`).

## 5. Buffering strategy

| Implementation | Setting |
|---|---|
| Sone, GStreamer source | `uridecodebin buffer-duration` = 15 s (DASH) / 5 s (BTS), `use-buffering=true` |
| Sone, branch queue | `queue max-size-time = 15 s`, buffers/bytes unlimited |
| Sone, ALSA | `buffer_time ~500 ms`, `period_time ~50 ms`, `start_threshold` = full buffer, `avail_min` = period |
| Sone, appsink | `max-buffers = 20`, `sync = false` |
| Sone, writer channel | `crossbeam bounded(256)` |
| tidalt, ALSA | `period = 1024 frames`, `buffer = 4 x period` (~93 ms @ 44.1 kHz), ALSA-default `sw_params` |
| Shaka (TIDAL web) | `bufferingGoal = 40 s`, `bufferBehind = 40 s`, `defaultPresentationDelay = 0`, `disableText`, `disableThumbnails` |
| ExoPlayer (TIDAL Android) | `backBuffer = 20 s`, `minPlaybackBuffer = maxPlaybackBuffer = 2 min`, `bufferForPlayback = 2.5 s`, `bufferForPlaybackAfterRebuffer = 5 s`, `audioTrackBuffer = 1.5 s` |

The ExoPlayer numbers are the closest thing to an official TIDAL statement of sensible buffering:
**two minutes of media buffer, 1.5 s of device buffer.** Pick a number, make it configurable,
default generously for the Pi/headless case.

## 6. On-disk cache, and the legal framing for it

- **High Tide** caches whole tracks to `MUSIC_DIR/{track.id}_{quality}.m4a`, unencrypted, skipping
  caching entirely on a metered network. MPD tracks are remuxed with
  `ffmpeg -protocol_whitelist file,crypto,data,http,https,tcp,tls -i manifest.mpd -f mp4 -c copy`
  into a `.tmp` then atomically renamed; BTS tracks are downloaded in 8192-byte chunks. No size cap,
  no encryption. Note: the filename uses the *session's configured* quality, not necessarily the
  quality actually served.
- **mopidy-tidal** runs an HTTP relay proxy on localhost in front of TIDAL's CDN, backed by SQLite.
  Insertion is only *finalised* when the whole resource arrives (unfinalised data dropped at next
  startup); stores a `TidalID -> Path` mapping so a cached track resolves fully offline. `Range`
  requests supported for seeking — the model for §10's mid-track resume too.
- **Sone** caches only *metadata* (not audio), encrypted at rest (AES-GCM), with tiered TTLs, a 2 GB
  cap and LRU eviction to 1.8 GB:

  | Tier | Contents | TTL | SWR grace |
  |---|---|---|---|
  | `UserContent` | playlists, favourites | 15 min | 1 h |
  | `Dynamic` | artist bios, charts, home | 4 h | 24 h |
  | `StaticMeta` | album tracklists, credits | 7 d | 30 d |
  | `Image` | album art, avatars | 30 d | 90 d |

  Sone never persists a manifest — stream manifests are in-memory only per session.

**Legal framing:** caching for a logged-in subscriber during a session is a player concern; building
a persistent, quality-tagged, indefinitely-retained library of decrypted files is a ripper. High
Tide's `MUSIC_DIR` cache sits uncomfortably close to that line. If streamboat implements offline
caching, it should be capped, evicted, encrypted at rest, tied to the current session's credentials,
and purged on logout.

## 7. Network resilience: manifest/token lifecycle, privileges websocket, play-reporting

- **Manifest expiry: exactly 1 hour** (`MANIFEST_EXPIRATION_MS = 3600000`). CDN URLs inside a BTS
  manifest carry their own, likely shorter-lived, expiring token — see §10.
- **401 handling:** refresh + retry, **unless** the 401 carries a playbackinfo `subStatus`, in which
  case do not refresh — see `tidal-manifest-api.md` §7.
- **429 handling:** global cooldown gate — see `tidal-manifest-api.md` §7.
- **Streaming privileges (one active stream per account).** `POST {legacyApiUrl}/rt/connect` with a
  bearer token returns `{url}`; connect and handle
  `{type: 'PRIVILEGED_SESSION_NOTIFICATION', payload: {clientDisplayName, sessionId, endsAt,
  updatedAt}}` and `{type: 'RECONNECT'}`; the client claims the privilege by sending
  `{type: 'USER_ACTION', payload: {startedAt}}`
  (`ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts`). Public event:
  `streaming-privileges-revoked`. `subStatus 4006` is the HTTP-side manifestation of the same thing
  and is explicitly non-terminal.
- **Play reporting.** Sone records the *actually served* attributes (`actual_product_id`, `quality`,
  `audio_mode`, `presentation`, timestamp). TIDAL's own SDKs send a richer `streaming_metrics` event
  set: `playback_info_fetch`, `streaming_session_start`/`_end`, `playback_statistics`,
  `drm_license_fetch` — keyed on `streamingSessionId` (`tidal-manifest-api.md` §8). Whether
  streamboat sends any of this is an open owner decision (SKILL.md).

## 8. Seeking within DASH

Sone seeks with `pipeline.seek_simple(FLUSH | KEY_UNIT, position)` on the whole pipeline. In the
DirectAlsa (exclusive) backend it also bumps the generation counter, sends `WriterCommand::Flush`,
and sets `frames_written = position * current_sample_rate` so position readout stays correct —
because in exclusive mode position derives from frames written to ALSA, not a GStreamer query (see
`output-backends.md` §8 for why that itself needs a `snd_pcm_delay` correction). Seeking is also
where gapless preroll is easiest to break — see §1(b)'s flush-seek note.

Server-side, DASH seeking is just jumping to the right `$Number$` — the whole `SegmentTimeline` is
in the manifest, so the segment for any timestamp is computable offline. GStreamer 1.28 fixed
"seeking in dashdemux2 for streams with gaps."

## 9. Network-stall behaviour in exclusive mode

Sone's writer blocks on `rx.recv_timeout(period_duration)` and, on timeout, calls
`write_silence(&pcm, &silence_buf)` — feeding the DAC a period of silence rather than underrunning,
tearing down (`audio-error {kind:"device_disconnected"}`) only if the silence write itself fails
(`ref:sone/src-tauri/src/audio.rs:1024,1315-1335`). Two consequences to handle that Sone does not:

- `frames_written` keeps incrementing during silence-fill, so position (§8) drifts forward through
  a stall even before accounting for buffer delay.
- In the DirectAlsa path Sone only *logs* GStreamer's `Buffering` bus percentage
  (`ref:sone/src-tauri/src/audio.rs:1761-1765`) rather than surfacing a rebuffering UI state — a
  stall is indistinguishable from silence in the track.

**Recommendation:** count silence-fill periods, enter an explicit `Rebuffering` state and freeze the
position clock, and pick a threshold beyond which the device pauses instead of feeding silence
indefinitely.

## 10. CDN URL expiry and mid-track resume

CDN URLs inside a BTS manifest carry their own expiring token, shorter-lived than the 1-hour
manifest window (§7) — a segment 403 mid-track needs a defined recovery (re-fetch the manifest,
resume at the current position, without tearing down an exclusive-mode ALSA handle) that no
reference project documents in full. For **direct** (BTS/BaseURL) URLs, resume is an HTTP
`Range: bytes=<offset>-` re-request on the same URL — mopidy-tidal's cache proxy already implements
the full Range/Content-Range handling needed (`ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/types.py:12-40`,
`proxy.py:219`). For DASH, resume is re-fetching from the current `$Number$`. This is a design
streamboat needs to own; no reference client demonstrates it end-to-end.

## 11. Sample-rate-change audible artifacts and DAC settling

Closing and reopening the PCM on a rate change (`output-backends.md` §1/§5) is necessary but not
sufficient: many DACs mute-relay or resync on a rate change, swallowing the first fraction of a
second, and some AVRs discard the first 1-2 seconds outright. mpv names both halves of this problem:

- `--audio-stream-silence=<yes|no>`: *"Cash-grab consumer audio hardware (such as A/V receivers)
  often ignore initial audio sent over HDMI ... In order to compensate for this, you can enable this
  option to not to stop and restart audio on seeks, and fill the gaps with silence"* — with an
  explicit warning it "modifies certain subtle player behavior ... strongly discouraged" as a
  general default.
- `--audio-wait-open=<secs>`: *"the player will wait for the given amount of seconds after opening
  the audio device before sending actual audio data to it. Useful if your expensive hardware
  discards the first 1 or 2 seconds of audio data sent to it."*

For a Design-A (own-writer) engine, the equivalent is a configurable settle delay plus a short
silence pre-roll after `snd_pcm_prepare()` at a new rate. This also bears on mixed-rate albums:
gapless is impossible across a rate change by construction — the UI should say "rate change" rather
than appear to glitch.

## 12. Device-hold policy across pause/idle

Holding an exclusive `hw:` device across a pause means no other application can make a sound while
streamboat is merely paused; releasing it means a relay click and re-negotiation on every resume.
Both policies ship today:

- **Sone holds.** Its software pause writes 50 ms silence buffers to pace the thread and only tears
  the PCM down/reopens afterward; writer state "lives outside `PlaybackBackend` so it persists
  across track changes" (`ref:sone/src-tauri/src/audio.rs:105`, pause path ~1470+).
- **tidalt releases.** *"The daemon holds exclusive access to the audio device only while a track is
  actually playing — releasing it on pause so other applications can use it freely"*
  (`ref:tidalt/README.md:11`).

**Recommendation:** hold while playing, release after a configurable idle timeout on pause, and
release the `org.freedesktop.ReserveDevice1` name at the same moment — holding a reservation without
using the device helps nobody.
