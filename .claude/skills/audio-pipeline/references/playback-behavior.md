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
13. Fade-out on stop/pause, and why it can't be a gain ramp in bit-perfect mode
14. CDN segment fetches carry no authentication
15. Play reporting to TIDAL, in full

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
  §12 below for the device-hold policy this needs. **Reframing: this is a smaller gap than it looks.**
  The DirectAlsa writer's `WriterCommand::EndOfTrack` handler does **not** call `snd_pcm_drain()` —
  it writes a silence period and enters an "idle silence loop — keep DAC clock alive between tracks"
  (`ref:sone/src-tauri/src/audio.rs:1169-1200`), i.e. it already keeps the PCM open across the track
  boundary, the mechanically hard part. What's actually missing is only a second, prerolled decode
  branch feeding that already-open writer. **A real bug this surfaces, worth fixing regardless of
  whether gapless is ever added to DirectAlsa:** because there's no drain, `track-finished` fires the
  moment the last decoded chunk is *handed to* the writer, not when the listener actually hears the
  end — up to one full ALSA buffer early (~500 ms at Sone's own settings, §5). That corrupts any
  scrobble/play-report timestamp (§7) or gapless-timing arithmetic derived from `track-finished` in
  the exclusive-mode path — the same class of bug as `output-backends.md` §8's write-vs-audible
  position error.

**Trap in the `concat` design: a single post-`concat` gain element cannot change gain at a
sample-accurate boundary.** §4's "apply ReplayGain before `play_url`" advice is correct only for a
cold start. On the gapless path, Sone applies the *next* track's gain from
`concat.connect_notify("active-pad")` onto the single shared `norm_vol` element shown in the pipeline
diagram above — which sits **downstream** of `concat` (`ref:sone/src-tauri/src/audio.rs:2656-2668`,
comment "concat is upstream of norm_vol, so the gain applies to the now-active branch"). `active-pad`
fires once the last buffer of track A has passed `concat`, not once it has been heard — whatever is
still queued in `audioconvert`/`audioresample`/the sink's own buffer at that instant is track A's
tail, and it gets track B's gain applied to it: an audible level jump on the wrong side of the
boundary whenever the two tracks' ReplayGain values differ. The same class of bug exists in High
Tide's `taginject`/`rgvolume`-swapped-on-`about-to-finish` design. **Fix: put a per-branch `volume`
element *inside* each decode branch, upstream of `concat`** — gain then travels with its own buffers
and switches exactly when `concat` switches — rather than one shared gain element downstream of the
mixing point. (A `GstControlBinding` timed to the boundary's running time is an alternative, but the
per-branch element matches the existing two-branch topology and is simpler.) **[uncertain — read
directly from source; not exercised against a real ReplayGain-differing track pair]**

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

**When to actually fire the prefetch — a concrete trigger point, previously missing entirely.**
"Preroll the next branch during the current track" doesn't say *when*. Fire too late and gapless
fails on a slow connection; too early and manifests (1 h TTL, §7) and `streamingSessionId`s
(`tidal-manifest-api.md` §8) go to waste. Strawberry's formula
(`ref:strawberry/src/engine/gstengine.cpp:88-89,595-620`): a 1 Hz timer computes
`remaining = length - position` and fires `AboutToFinish` when `remaining < gap + fudge`, where
`gap = buffer_duration_nanosec + (autocrossfade ? fadeout_duration : kPreloadGapNanosec)`,
`kPreloadGapNanosec = 8 s`, `fudge = timer_interval + 100 ms`. **Translated: preload lead =
compressed-network-buffer depth (§5) + 8 s, polled at ~1 Hz, latched so it fires once per track.**
Re-run resolution if the queue's next item changes after the latch — mpv's own
`--prefetch-playlist` docs warn about exactly this on reorder (above). **Discard the prefetched
`streamingSessionId` explicitly if the prefetch is abandoned by a skip/reorder** — do not let it
silently orphan.

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

**Edge cases beyond the formula — decide these explicitly, they are not settled by the formula
alone:**

- **Missing-value sentinel.** High Tide treats a gain of exactly `1.0` as "missing metadata" and
  skips applying it — a workaround for `python-tidal` issue #332
  (https://github.com/EbbLabs/python-tidal/issues/332); Sone instead returns unity gain when
  `replay_gain` is `None`. TIDAL's own web SDK is a third answer, and it agrees with Sone's
  *outcome*, not High Tide's: missing `trackReplayGain`/`peakAmplitude` default to `0` via `?? 0`
  (`ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts:395-403`),
  i.e. unity gain — strengthening "pick one sentinel, and unity-gain-on-missing has two of three
  reference implementations behind it" as the decision to make. These produce **different loudness
  for the same missing-metadata track** — pick one, streamboat currently has no stated answer.
- **`peak > 1` and gain/peak pairing.** `1/peak` correctly attenuates below unity even at zero gain,
  but this means two tracks on the same album can receive different *applied* gain under `ALBUM`
  mode depending on whether album gain pairs with album peak or track peak — the Android SDK pairs
  album gain with album peak; Sone's `use_track_gain` context-sensitively picks track-with-album-
  fallback or album-with-track-fallback. State which pairing streamboat uses.
- **Is `preAmp` user-exposed?** TIDAL fixes it at 4 (0 on TV); Sone silently multiplies by an extra
  0.8 with no user control. Whether the user gets a trim/preAmp control at all is currently
  undecided — settle it alongside the bit-perfect-default-state owner decision (SKILL.md).
- **Interaction with bit-perfect mode.** If bit-perfect disables ReplayGain entirely (above), the
  album-to-album loudness jumps normalization exists to remove come back specifically for the users
  most likely to notice them — an audiophile bit-perfect user. Weigh this trade in the same decision
  as bit-perfect's default state, not as a hidden side effect of the mode toggle.

## 5. Buffering strategy

**This is three separate pipeline stages, not one number — do not average them or read "2 minutes"
as a single buffer size.**

| Stage | Implementation | Setting |
|---|---|---|
| **Network/compressed** | Sone, `uridecodebin` source | `buffer-duration` = 15 s (DASH) / 5 s (BTS), `use-buffering=true` |
| **Network/compressed** | Shaka (TIDAL web) | `bufferingGoal = 40 s`, `bufferBehind = 40 s`, `defaultPresentationDelay = 0` |
| **Network/compressed** | ExoPlayer (TIDAL Android) | `DefaultLoadControl`'s "2 minutes" figure buffers *extracted/compressed* samples, not decoded PCM — easy to misread as the decoded-PCM number |
| **Decoded PCM reservoir** | Sone, branch queue | `queue max-size-time = 15 s` of *decoded* audio, buffers/bytes unlimited |
| **Decoded PCM reservoir** | Sone, appsink / writer channel | `max-buffers = 20`, `sync = false`; `crossbeam bounded(256)` |
| **Device buffer** | Sone, ALSA | `buffer_time ~500 ms`, `period_time ~50 ms`, `start_threshold` = full buffer, `avail_min` = period |
| **Device buffer** | tidalt, ALSA | `period = 1024 frames`, `buffer = 4 x period` (~93 ms @ 44.1 kHz), ALSA-default `sw_params` |
| **Device buffer** | ExoPlayer (TIDAL Android) | `audioTrackBuffer = 1.5 s`; separately, `backBuffer = 20 s`, `bufferForPlayback = 2.5 s`, `bufferForPlaybackAfterRebuffer = 5 s` |

**Default each stage independently** — network/compressed generous (network is the scarce resource
on a Pi; TIDAL's web SDK's 40 s is a reasonable ceiling), decoded-PCM conservative, device buffer per
`output-backends.md` §1/§9. **The memory arithmetic that makes this matter, especially headless/Pi:**
two minutes of *decoded* 24-bit/192kHz stereo PCM in an S32 container is
`192000 x 4 bytes x 2ch x 120s ~= 184 MB` (about 138 MB packed 24-bit); two prerolled gapless
branches (§1(b)) double that. Sone's actual 15 s decoded reservoir at 24/192 is
`192000 x 4 x 2 x 15 ~= 23 MB` per branch — safe on a 512 MB Pi. **That 23 MB figure is what
generalises to a decoded-PCM default; ExoPlayer's "2 minutes" does not** (it's compressed data, and
even the compressed-24/192-FLAC equivalent is only ~70-90 MB for 2 minutes).

## 6. On-disk cache, and the legal framing for it

- **High Tide** caches whole tracks to `MUSIC_DIR/{track.id}_{quality}.m4a`, unencrypted, skipping
  caching entirely on a metered network. MPD tracks are remuxed with
  `ffmpeg -protocol_whitelist file,crypto,data,http,https,tcp,tls -i manifest.mpd -f mp4 -c copy`
  into a `.tmp` then atomically renamed; BTS tracks are downloaded in 8192-byte chunks.
  **Correction: it is not uncapped.** `ref:high-tide/src/window.py:223` starts
  `threading.Thread(target=utils.evict_cache, args=(utils.MUSIC_DIR, 5))` at window construction, and
  `ref:high-tide/src/lib/utils.py:828-843` `evict_cache(cache_dir, max_gb)` sorts entries by
  `st_atime` and unlinks the oldest-accessed until total usage is <= `max_gb * 1024**3` — a **5 GB
  atime-LRU cache**, run once per window creation. No encryption is still correct. Two real
  weaknesses worth copying the fix for, not the "no cap" myth: eviction runs only at window-open, not
  during a long session, so usage can sit over 5 GB until the app is reopened; and `evict_cache` does
  `f.stat().st_size`/`f.unlink()` over a bare `cache_dir.iterdir()`, so any subdirectory ever created
  under `MUSIC_DIR` makes it raise. Note also: the filename uses the *session's configured* quality,
  not necessarily the quality actually served.
  **Also unsafe under prefetch/gapless — a separate bug**: every track's MPD is written to one fixed
  path, `Path(utils.CACHE_DIR, "manifest.mpd")`, opened and overwritten on every track
  (`ref:high-tide/src/lib/player_object.py:494-513`). Any prefetch of the next track (including High
  Tide's own `about-to-finish` gapless, §1(a)) can overwrite the current track's MPD while the
  demuxer may still need it. **streamboat must use a unique temp file per streaming session, delete
  it on track teardown, and place it in `XDG_RUNTIME_DIR`, not a shared cache dir** (relevant inside
  a Flatpak/Snap sandbox too).
- **mopidy-tidal** runs an HTTP relay proxy on localhost in front of TIDAL's CDN, backed by SQLite.
  Insertion is only *finalised* when the whole resource arrives (unfinalised data dropped at next
  startup); stores a `TidalID -> Path` mapping so a cached track resolves fully offline. `Range`
  requests supported for seeking — the model for §10's mid-track resume too.
- **Sone** caches only *metadata* (not audio), encrypted at rest (AES-GCM), tiered by TTL — table,
  cap, and eviction policy owned by
  `streamboat-engineering-baseline/references/config-cache-logs-telemetry.md` §2. Sone never
  persists a manifest — stream manifests are in-memory only per session.

**Legal framing:** caching for a logged-in subscriber during a session is a player concern; building
a persistent, quality-tagged, indefinitely-retained library of decrypted files is a ripper.
**Correction: High Tide's cache is not that case** — `MUSIC_DIR` is `Path(CACHE_DIR, "music")`
(`ref:high-tide/src/lib/utils.py:76-78`), inside the XDG cache dir, and it is capped/LRU-evicted
(above). It still lacks encryption at rest and a per-session/credential tie, which are the real gaps.
If streamboat implements offline caching, it should be capped, evicted (continuously, not only at
startup — High Tide's weakness), encrypted at rest, tied to the current session's credentials, and
purged on logout.

## 7. Network resilience: manifest/token lifecycle, privileges websocket, play-reporting

- **Manifest expiry: exactly 1 hour** (`MANIFEST_EXPIRATION_MS = 3600000`). CDN URLs inside a BTS
  manifest carry their own, likely shorter-lived, expiring token — see §10.
- **Resume-after-long-pause is a distinct path from §10's mid-track 403** — the manifest can go
  stale while nothing is being fetched at all. §12 makes long pauses (holding the device, doing
  nothing) an explicitly supported state, so a track paused for close to an hour is a live case, not
  a corner one. **Stamp each manifest with its fetch time; on resume, if `now - fetched_at` is within
  a margin of the 3600 s TTL (or already past it), re-fetch the manifest before writing another
  byte.** Do not resume on a manifest that is merely still-loaded-in-memory and wait for the CDN to
  return the 403 that a proactive re-fetch would have avoided (SKILL.md pitfall #29).
- **401 handling:** refresh + retry, **unless** the 401 carries a playbackinfo `subStatus`, in which
  case do not refresh — see `tidal-manifest-api.md` §7.
- **429 handling:** global cooldown gate — see `tidal-manifest-api.md` §7.
- **Streaming privileges (one active stream per account).** Wire format, reconnect policy, and the
  desktop-vs-headless priority question are owned by
  `headless-and-tidal-connect/references/daemon-architecture.md` §6 — cite it rather than
  restating. Audio-pipeline-specific consequence: `subStatus 4006` is the HTTP-side manifestation
  of a revocation and is explicitly non-terminal (`tidal-manifest-api.md` §7); a dropped
  privileges socket must never, by itself, stop playback — it degrades to "cannot confirm we still
  hold the privilege," not "stop."
- **Play reporting.** Wire format (`ec.tidal.com/api/event-batch`, SQS batching, headers, pinned
  device identity, the 30-second threshold, JWT attribution, offline outbox) is owned by
  `tidal-api/references/play-logging-and-privileges.md` §1-4 — cite it rather than restating.
  Audio-pipeline-specific consequence: suppress reporting for a `PREVIEW`-presentation play
  (`tidal-manifest-api.md` §13) — it must never cross the 30-second threshold as if it were the
  real track, since the report would misrepresent a preview as a real stream.

## 8. Seeking within DASH

Sone seeks with `pipeline.seek_simple(FLUSH | KEY_UNIT, position)` on the whole pipeline. In the
DirectAlsa (exclusive) backend it also bumps the generation counter, sends `WriterCommand::Flush`,
and sets `frames_written = position * current_sample_rate` so position readout stays correct —
because in exclusive mode position derives from frames written to ALSA, not a GStreamer query (see
`output-backends.md` §8 for why that itself needs a `snd_pcm_delay` correction). Seeking is also
where gapless preroll is easiest to break — see §1(b)'s flush-seek note.

Server-side, DASH seeking is just jumping to the right `$Number$` — the whole `SegmentTimeline` is
in the manifest, so the segment for any timestamp is computable offline. GStreamer release reporting
is cited elsewhere for "1.28 fixed seeking in dashdemux2 for streams with gaps," but this specific
claim could not be corroborated on re-check — its only two citations, plus
`gstreamer.freedesktop.org` itself, are all blocked from this research environment. **Treat as
unverified, not settled.**

**Seek precision and the seek-flag choice are policy decisions this skill leaves open, not solved
facts — see `tidal-manifest-api.md` §4 for the full segment-timestamp arithmetic** (the
`SegmentTimeline/S@t`/`@d`/`presentationTimeOffset` mapping, and why the DASH-spec mapping alone only
gets you to a segment boundary, not sample-accurate position). The reference clients disagree on
which to accept: Sone/High Tide use `FLUSH | KEY_UNIT` (accept a segment-boundary snap); Strawberry
uses `FLUSH` alone (accurate,
`ref:strawberry/src/engine/gstenginepipeline.cpp:2370`) — pick one and record why. After any
hand-rolled seek, the init segment must be re-fed before the new media segment. **Duration is also
ambiguous**: the track object's `duration`, the MPD's `mediaPresentationDuration`, and the decoded
sample count can disagree by up to a second, and the prefetch trigger (§3), MPRIS `mpris:length`
(`os-integration.md` §7), and §15's 30-second play threshold all need to agree on one authoritative
source — use the track object's `duration`.

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

**No reference client in the set actually implements a DAC warm-up/re-lock settle delay** — this is
a gap, not a documented pattern to copy. A search for sleep/delay calls around device open/reconfigure
in Sone's and tidalt's writer code finds only unrelated pacing (Sone's 10 ms XRUN-retry pause, its
50 ms software-pause silence cadence, a 10 ms poll, the 100 ms `DeviceMonitor` poll); the only thing
labelled "settle" anywhere is tidalt's `releaseSettleDelay = 200ms` (`output-backends.md` §2), which
is the D-Bus device-reservation hand-off waiting for a previous owner to close its handle — a
different problem from PLL lock on the DAC itself. Since exclusive-mode gapless (Implication #12,
`output-backends.md` §6) makes per-track rate switching a routine operation, not an edge case, this
gap is worth closing on day one: pre-roll a configurable silence period (order of 200-300 ms,
per-device override) after every device open/reconfigure, before writing real audio, and count it in
the time-to-first-audio budget (`output-backends.md` §17).

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

## 13. Fade-out on stop/pause, and why it can't be a gain ramp in bit-perfect mode

§2 covers crossfade; the more commonly-needed fade-out-on-stop/fade-on-pause (suppressing the
audible click of abruptly tearing down a stream) is missing from the original report entirely, even
though it's the defect an audiophile user notices first. Strawberry implements both, gated the same
way as crossfade: `fadeout_enabled_`/`fadeout_duration_` fade the outgoing pipeline on stop/track
change, `fadeout_pause_enabled_`/`fadeout_pause_duration_` fade on pause, both skipped when
`AnyExclusivePipelineActive()` (`ref:strawberry/src/engine/gstengine.cpp:233-249,336-339,362-389`).
Strawberry's own comment states the correctness detail worth copying verbatim: *"If we pause with
fadeout, deactivate fadeout and resume playback, the player would be muted if not faded in"* — the
fade-**in** on resume is mandatory, not cosmetic. **For streamboat's exclusive-mode path, the answer
cannot be a gain ramp** — any software gain quantises, exactly like the volume slider (§4). Real
options: (a) accept a click; (b) write a short silence ramp before `snd_pcm_drop()`/closing — not
bit-perfect content, but inaudible content, a defensible trade for a stop/pause transition
specifically; (c) `snd_pcm_drain()` then close, accepting the drain latency. Sone already writes
50 ms silence buffers around its software pause (§12) — mechanism (b) already exists in embryo;
extend it to stop as well as pause.

## 14. CDN segment fetches carry no authentication

Every design this skill and `output-backends.md`/`stacks-comparison.md` describe silently depends on
handing a manifest/segment URL to `souphttpsrc`, libmpv's `loadfile`, or a bare `fetch` with no
bearer token or header injection — stated nowhere until now. **Confirmed: TIDAL's CDN URLs are
pre-signed and take no `Authorization` header.** tidal-cli fetches both the DASH init segment and
every media segment with a bare `await fetch(url)` and no request-init at all
(`ref:tidal-cli/src/playback.ts:156-170`); mopidy-tidal's relay proxy forwards to the CDN with no
auth added; High Tide passes BTS URLs straight to `requests`/`ffmpeg` unmodified. **Corollaries:**
(a) the expiry lives in the URL's own query token, which is why §10's mid-track 403 recovery means
re-fetching the manifest, not refreshing the OAuth token; (b) any HTTP client works, so a
mopidy-tidal-style localhost relay proxy is a legitimate way to insert caching, `Range` handling and
retry *underneath* an engine that offers no hook — the cleanest answer to §10's open problem on
Design B, where you cannot reach inside libmpv's demuxer; (c) a working TLS stack in whatever
runtime/plugin set is shipped is therefore load-bearing on every platform — see
`output-backends.md` §14's `gioopenssl.dll` finding for Windows specifically.

## 15. Play reporting to TIDAL, in full

Full wire format (endpoint, threshold, attribution, device-impersonation constraint) is owned by
`tidal-api/references/play-logging-and-privileges.md` §1-4 — see §7 above for the
audio-pipeline-specific consequence. Play-reporting shares the `streamingSessionId` join key with
manifest prefetch (`tidal-manifest-api.md` §8).
