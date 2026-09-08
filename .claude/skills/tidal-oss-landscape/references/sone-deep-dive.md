# Sone deep dive — the owner's named reference #1

`ref:sone` · https://github.com/lullabyX/sone · GPL-3.0-only · 437★ / 24 forks / 64 open issues ·
created 2026-02-23 · last push 2026-09-05 · v0.21.0 · single primary author (`lullabyX`).

Full narrative and per-claim sourcing: `docs/research/oss-landscape.md` §2, §3, §18. Bit-perfect
ALSA internals and gapless machinery are covered in depth in `audio-engineering.md` — this file
does not repeat that, it covers everything else about Sone: stack, module map, IPC/state
ownership, caching/crypto/settings, packaging, code quality, and what to borrow/avoid.

## Table of contents

1. Stack
2. Module map
3. IPC design — and the state-ownership inversion streamboat must NOT copy (incl. the corrected
   anchor-poll-plus-interpolate position model and its settle-window guard)
3a. Seeking
3b. Gapless prefetch/arming policy (the frontend half of gapless)
3c. Track-availability pre-flight and playback error classification
3d. The queue data model (five collections, not one list)
3e. Autoplay and the explicit-content filter
3f. Navigation and scroll restoration
4. Caching, settings, secrets
4a. `countryCode` provenance and account-endpoint log redaction
4b. The playlist/favorites-mutation ETag precondition
4c. The rate-limit contract, with actual numbers
4d. Scrobble threshold and trigger points
5. Theming, lyrics, miniplayer, integration surfaces, music videos, play reporting (incl. the
   Android-client-identity impersonation in the play-reporting wire format)
6. Packaging and CI — reusable scripts, but no CI at all
7. Code quality: strong source, weak process (and what "151 tests" does not mean)
8. Borrow / avoid, consolidated
9. Sone-windows — the Windows fork, and why one codebase could serve both (corrected: Windows-only,
   no macOS arm)
10. Bus factor / contributor counts

## 1. Stack

**Rust backend**, mostly `=`-pinned: `tauri =2.11.2` (+`tauri-build =2.6.2`),
`tauri-plugin-single-instance =2.4.2`, `tauri-plugin-deep-link =2.4.9`,
`tauri-plugin-opener =2.5.4`, `tauri-plugin-window-state =2.4.1`, `tauri-plugin-oauth =2.0.0`,
`tauri-plugin-global-shortcut =2.3.1`, `gstreamer 0.23`, `gstreamer-app 0.23`,
`crossbeam-channel 0.5`, `reqwest 0.11` (json, socks), `tokio 1` (full), `base64 0.21`, `dirs 5`,
`aes-gcm 0.10`, `keyring 3` (sync-secret-service), `zeroize 1`, `sha2 0.10.9`, `rand 0.10.1`,
`md5 0.7`, `discord-rich-presence 1.1`, `zbus 5`, `rmcp 1.7.0` (server+streamable-http+macros),
`schemars 1`, `axum 0.7`, `uuid 1`, `flexi_logger 0.31`, `thiserror 2`, `semver 1.0.28`,
`walkdir 2`, `id3 1`, `image 0.25`. Linux-only: `webkit2gtk =2.0.2`, `gtk 0.18`, `gdk 0.18`,
`mpris-server 0.9`, **`alsa 0.10`**, `libc 0.2`, `ksni 0.3` (tray), `x11rb 0.13.2`
(screensaver/dpms), `wayland-client 0.31`, `wayland-backend 0.3`, `wayland-protocols 0.32`,
`gdkwayland-sys 0.18`.

**There is no `symphonia`, `cpal`, or `rodio`, and no `alsa-sys`-only usage.** Decode is
GStreamer; output is `autoaudiosink` or Sone's own ALSA writer thread (see
`audio-engineering.md` §1).

**Frontend**: React 19.1, `jotai 2.17` (atom state, no Redux), Tailwind 4 (`@tailwindcss/
postcss`), Vite 7.3, Vitest 4.1, TypeScript 5.8, `@tanstack/react-virtual 3.13`, `hls.js 1.6.16`
(music videos only), `lucide-react`, `qrcode.react` (device-code QR), `react-easy-crop` (avatar
upload). Tooling: pnpm 11.1.3, ESLint 9, Prettier 3, `knip`.

**Size, corrected**: 26,721 lines Rust across 61 files in `src/` alone (`tidal_api.rs` 7,200 lines;
`audio.rs` 3,309 lines); including `build.rs` the count is 62 files / 26,724 lines — cite one
pairing, not a mix of the two denominators. 47,609 lines TS/TSX across 199 files.

**Tests**: 151 `#[test]`/`#[tokio::test]` in Rust, 46 `*.test.ts(x)` files, plus one Python
integration probe `ref:sone/src-tauri/tests/gapless_probe.py` that nothing runs automatically.
**None of this is enforced by CI** — see §7.

## 2. Module map (`ref:sone/src-tauri/src/`)

```
main.rs                 thin bin; delegates to lib.rs
lib.rs                  AppState, Settings struct, config bootstrap, crypto init,
                        tauri::generate_handler![...] (177 commands, lib.rs:866-1070)
tidal_api.rs            the entire unofficial API client (auth, catalog, playback, library)
audio.rs                AudioPlayer: command loop, two backends, gapless, ALSA writer
pipeline_probe.rs       pad-caps probes (decoded vs output) for the signal-path panel
signal_path.rs          PRISTINE verdict; reads pactl + /proc/asound
cache.rs                4-tier encrypted on-disk cache with SWR + LRU + tag invalidation
crypto.rs               AES-256-GCM, "SONE" magic header, keyring + file key
theme_config.rs         external theme.json (15 presets + custom accent/background)
mpris.rs                mpris-server 0.9 bridge, command enum over an mpsc channel
tray.rs                 ksni tray
discord.rs              discord-rich-presence
idle_inhibit/{dbus,wayland,x11}.rs   three inhibit backends
scrobble/{lastfm,librefm,listenbrainz,musicbrainz,queue}.rs
tidal_report/{event,queue}.rs        play reporting back to TIDAL ("Recently Played")
mcp/{server,events,sanitizer,state_mirror,tools/*}    MCP server (rmcp + axum)
overlay/{server,state}.rs            OBS browser-source overlay (axum)
rate_gate.rs            client-side rate limiting
error.rs, http_util.rs, logging.rs, embedded_config.rs, embedded_*.rs
commands/               auth, library, pages, playback, metadata, search, feed, profile,
                        scrobble, overlay, mcp, updates, utility
```

Frontend mirrors it: `src/api/tidal.ts` (invoke wrapper + LRU), `src/atoms/*` (13 jotai atom
files — **this is where playback state actually lives, not in Rust; see §3**), `src/hooks/*` (35
hook files), `src/components/*` (85 components), `src/miniplayer-main.tsx` + `miniplayer.html` (a
second Tauri window).

## 3. IPC design — and the state-ownership inversion streamboat must NOT copy

**177** `#[tauri::command(rename_all = "camelCase")]` functions registered in one
`generate_handler!` block spanning `lib.rs:866-1070`, grouped by domain.

**The load-bearing architectural fact this skill exists partly to prevent streamboat from
copying**: Sone's playback session — queue, manual-queue, original-queue, history, repeat,
shuffle, playback source, current track, is-playing, consecutive-fail-count, user-paused — is
**client-side `jotai` atoms in the React webview**
(`ref:sone/src/atoms/playback.ts:54-109`), several persisted only to `localStorage`
(`atomWithStorage("sone.repeat.v1", ...)` etc.). The Rust side's only queue involvement is
persistence (`save_playback_queue`/`load_playback_queue`, `lib.rs:985-986`) — it does not own the
queue, does not decide what plays next, and does not implement shuffle/repeat logic. Its MCP,
overlay, and miniplayer surfaces work by **mirroring** frontend-pushed state into
`Arc<RwLock<McpState>>` (`ref:sone/src-tauri/src/mcp/state_mirror.rs`), not by being the source of
truth.

**This is why Sone-windows is a fork rather than a genuinely portable core** (§9), and it is the
single biggest thing streamboat's architecture must invert: **the core process must own session,
queue, and transport; every UI — desktop, CLI, MPRIS, HTTP — is a thin subscriber.** A queue that
lives in a webview cannot serve a CLI or a headless daemon as a first-class citizen; it can only
be mirrored to them after the fact, which is exactly Sone's shape.

**Corrected (this file's own earlier draft got this backwards — read the current oss-landscape.md
§2.4, not an older cached version of this claim): Sone *does* emit `track-advanced`.**
`app_handle.emit("track-advanced", json!({"trackId":…, "qid":…, "replayGain":…,
"peakAmplitude":…}))` fires from the audio thread on every gapless advance
(`ref:sone/src-tauri/src/audio.rs:2671-2682`) and Rust itself consumes it for scrobbling
(`ref:sone/src-tauri/src/lib.rs:705-712`). What remains true: **there is no periodic
position/state-tick event** — `track-advanced` is a discrete track-boundary event, not a position
stream. The full emitted-event set is `audio-error` (payload `kind` ∈ `{device_disconnected,
device_changed, format_change_failed}` — values of one event, not separate events),
`audio-resampled`, `audio-bit-depth-changed` (`audio.rs:852`, on a bit-depth promotion),
`signal-path-changed`, `track-finished`, `track-advanced`,
`pkce-login-success`/`-error`/`-cancelled`, `scrobble-auth-error`, `tray:{toggle-play,next-track,
prev-track}`, and `mpris:{play,pause,stop,seek,set-position,set-volume,set-shuffle,
set-loop-status,set-fullscreen,open-uri}` — the `tray:*`/`mpris:*` events flow **up** into the
webview for it to act on, not down from it (the same inversion as the queue).

**Corrected (third fact-check pass, 2026-09-08) — this file's own earlier draft of the position
model was itself wrong; "position is polled, not pushed, every consumer polls independently" is
removed.** Sone already implements the anchor-plus-interpolate pattern streamboat should copy:
`ref:sone/src/lib/playbackPosition.ts` is a module-level singleton that polls the backend **once**
every 2000 ms (`setInterval(fetchAndAnchor, 2000)`, one `invoke<number>("get_playback_position")`
call) and caches `{position, timestamp}`. Every consumer, including the 500 ms
`useProgressScrub.ts:38` timer, calls `getInterpolatedPosition()` — a **pure local computation**
(`anchor.position + (now - anchor.timestamp)/1000`), never a fresh IPC round-trip. The miniplayer
and overlay windows are **pushed to**, not polled from: `useMiniplayerEmitter.ts:167`
(`setInterval(scheduleEmit, 1000)`) and `useOverlayBridge.ts:100` each broadcast the interpolated
state outward once a second from the main webview. `useSignalPathRefresh.ts:30` polls a
*different* backend command (signal-path/format info), not position. **Net shape: one 2 s backend
anchor poll, local interpolation everywhere, 1 s push heartbeats to secondary windows** — not "N
independent pollers."

The genuinely transferable, non-obvious mechanism (previously undocumented anywhere in this skill)
is the **settle-window guard**: for `SETTLE_WINDOW_MS = 3000` after a track change, a freshly
polled position more than `SETTLE_AHEAD_TOLERANCE_SECS = 2` ahead of the interpolated value is
**discarded**, because at a `concat` gapless boundary GStreamer's `query_position` briefly reports
the *previous* track's cumulative pipeline runtime before the new per-track segment takes over —
without this guard the progress bar flashes forward to a stale cumulative value on every gapless
advance. A `loadingTrack` freeze stops interpolation during a user-initiated load (so the bar
doesn't visibly climb from 0 before the real position anchors), and a `trackGeneration` counter
discards any poll response that resolves after a newer track change has already superseded it.
Seeking anchors immediately via `notifySeek()` and deliberately never touches `trackResetTime`, so
a forward seek is never itself rejected by the settle-window guard. **Copy the anchor-plus-
interpolate-plus-settle-window shape for streamboat's own core↔UI protocol** — a naive per-consumer
poll wastes IPC, and a naive raw position push fights the same gapless-boundary glitch that
motivated the guard in the first place. Source: `ref:sone/src/lib/playbackPosition.ts` (whole
file); `ref:sone/src/hooks/{useProgressScrub.ts:38,useMiniplayerEmitter.ts:167,
useOverlayBridge.ts:100,useSignalPathRefresh.ts:30}`.

## 3a. Seeking

The transport operation most likely to break both `concat` gapless and the ALSA writer, and
undocumented anywhere else in this skill until now. `AudioCommand::Seek`
(`ref:sone/src-tauri/src/audio.rs:2307-2365`) carries an empirical comment from testing on
GStreamer 1.24.2: `seek_simple(FLUSH|KEY_UNIT)` on the pipeline forwards `FLUSH_START`/
`FLUSH_STOP` and the new `SEGMENT` only to `concat`'s **active** sink pad; the inactive prerolled
next-track branch sees no flush events, stays linked and `PLAYING`, and `concat` still switches to
it correctly at the active branch's EOS. **A seek must not detach the armed next-track slot** —
Sone tried that once and it destroyed a valid preroll on every seek, producing an audible gap at
the next track boundary. On the `DirectAlsa` (bit-perfect) path, a seek additionally bumps
`track_generation`/`writer_gen` (dropping in-flight stale PCM chunks), sends `WriterCommand::
Flush`, and re-bases `frames_written = position_secs * current_sample_rate` — **position on the
bit-perfect path is derived from frames actually written to the PCM device, not from a GStreamer
position query** — and paused state is saved/restored around the seek. Port this behaviour, not
just the ALSA negotiation in §1 of `audio-engineering.md`, if streamboat copies Sone's bit-perfect
path.

## 3b. Gapless prefetch/arming policy (the frontend half of gapless)

`audio-engineering.md` documents the Rust-side `concat` plumbing; this is the policy that decides
*when* to arm it, which is pure frontend logic an implementer must design from scratch — no
project other than Sone has one to copy. `ref:sone/src/hooks/useGaplessPrefetch.ts` (~180 lines):
(1) gapless capability is cached once via `invoke("get_gapless_supported")`; (2) arming requires
`gapless && !exclusiveMode && !bitPerfect && !currentVideo && currentTrack` — note **exclusive
mode**, not just the `DirectAlsa` backend, disables it, and a **video** as the current queue item
disables audio gapless too (see §5's music-videos note); (3) it deliberately does **not** gate on
`isPlaying`, because "isPlaying flickers false during device-busy retries and on every pause" —
i.e. Sone has an ALSA device-busy retry loop, and a paused track keeps its slot armed because
`concat` cannot switch an inactive pad while paused; (4) it dedups on `(trackId, qid)` before
calling into Rust, since the predicted next track is stable for a whole track and a naive debounce
would otherwise re-run the backend's full quality cascade repeatedly; (5) a negative cache
`failedRef {trackId, qid, until}` stops a track that failed to resolve from being retried on every
queue mutation; (6) in-flight refreshes are **coalesced, not dropped** (with shuffle on, the
prediction genuinely changes mid-flight); (7) a generation counter discards superseded responses.
Backend side: `commands/playback.rs:209` `set_next_track(trackId, qid, useTrackGain)` →
`audio_player.set_next_track(uri, gain, track_id, qid, rg, peak, is_dash)`, plus
`clear_next_track`. Prediction: `ref:sone/src/lib/gaplessPredict.ts::pickGaplessNext`; post-advance
bookkeeping: `ref:sone/src/hooks/usePlaybackActions.ts:494-540`.

## 3c. Track-availability pre-flight and playback error classification (added by the third
fact-check pass)

The first implementer question after "how do I get a stream URL" — and the only complete answer
to it in the reference set lives in one 79-line file, `ref:sone/src/lib/trackAvailability.ts`.

- **Metadata pre-flight** — `isTrackUnavailable(track)` is true if `streamReady === false`, or
  `allowStreaming === false`, or `streamStartDate` parses to a future timestamp. Videos are exempt
  (`itemType === "video"` always returns false — not audio-stream-gated). **Undefined fields count
  as available**, so a response that omits these flags does not grey out the whole catalogue. All
  three fields live on both the track and album shapes (`ref:sone/src/types.ts:113-116,183-185`).
- **Error classification** — `isUnplayableError(err)` is a narrow allowlist: HTTP 404, 410, 451,
  and 401 *only when* the body carries a terminal `subStatus` (§6 of `api-auth-streaming.md`). **A
  bare 401 stays transient** — that's ordinary auth expiry, not an unplayable track. Everything
  else (403, 429, 5xx, network, decode) is transient: halt playback, keep the track in the queue,
  do not auto-skip. `isRateLimitedError` splits 429 out on its own, reading a `retryAfterSecs`
  field back out of the error body.
- **Skip-loop bound**: `MAX_CONSECUTIVE_PLAY_FAILS = 3` (`ref:sone/src/hooks/
  usePlaybackActions.ts:80`); resets to 0 on any successful play (`:331,537,893`).
- A maintenance comment worth copying structurally: *"Mirrors TERMINAL_SUB_STATUSES in
  src-tauri/src/tidal_api.rs — keep in sync"* — Sone duplicates this table across the IPC boundary.
  Owning the queue in the core (§3 above) removes that duplication by construction.

## 3d. The queue data model (added by the third fact-check pass — Implication 28's highest-ranked
recommendation had no data model until now)

"streamboat's core must own session/queue/transport" (§3 above) needs a shape, not just an
ownership rule. Sone's — matching the official client's own shape — is **five separate
collections**, not one flat list (`ref:sone/src/atoms/playback.ts:56-109`):

| Atom | Holds |
| --- | --- |
| `queueAtom` | the live, possibly-shuffled context queue |
| `originalQueueAtom` | `Track[] \| null` — pre-shuffle order, so unshuffle restores rather than re-sorts |
| `manualQueueAtom` | user "play next" insertions — consumed **before** the context queue |
| `historyAtom` | played tracks |
| `playbackSourceAtom` / `contextSourceAtom` | what's actually playing vs. what the user is browsing — the pair behind a "Playing from …" chip |

`repeatAtom` is an int (0=off/1=all/2=one), `shuffleAtom` a boolean, both persisted. Two policy
atoms sit alongside: `useTrackGainAtom` — true = track ReplayGain (shuffle/mixed queue), false =
album ReplayGain (album playing in order) — the concrete rule behind `use_track_gain` in
`api-auth-streaming.md` §7's normalization discussion; and `userPausedAtom`, a global explicit-
pause-intent flag "so that gapless advanceToTrack can never resume audio the user paused" — the
same desired-vs-actual playback-state split the official client uses independently
(`playbackControls.playbackState` vs. `.desiredPlaybackState`, `ref:TidaLuna/plugins/lib/src/
classes/PlayState.ts:33-70`).

## 3e. Autoplay and the explicit-content filter (added by the third fact-check pass)

Two shipped features this skill previously never mentioned; both are queue-contract decisions,
not UI toggles, and cannot be retrofitted after the queue schema is fixed.

- **Autoplay** (`ref:sone/src/hooks/usePlaybackActions.ts:1090-1120`): when the queue drains and
  `autoplayAtom` is on, read `currentTrack.mixes?.TRACK_MIX` (bail if absent), call
  `getMixItems(trackMixId)`, filter out anything already in `historyIds` (seeded with the current
  track), anything explicit if the filter is on, and anything `isTrackUnavailable`; take the head,
  push the rest into the queue, force `useTrackGain = true` ("radio = mixed context"). Default:
  **off**.
- **Explicit filter** (`allowExplicitAtom`, persisted, default **on**): consulted at **ten**
  separate call sites in the same file (`:668,678,712,741,1038,1101,1467,1555,1599` — play,
  enqueue, play-album, play-playlist, autoplay, shuffle-all). That call-site count is the evidence
  it must be a **queue-layer invariant** in streamboat's core, enforced once at the mutation path,
  not a filter bolted onto each UI screen separately.

## 3f. Navigation and scroll restoration (added by the third fact-check pass — the UI-patterns
half of this skill previously had almost nothing)

Sone uses **no router library** (no react-router in `package.json`). Navigation is one jotai atom
holding a discriminated union: `currentViewAtom = atom<AppView>({type: "home"})`, where `AppView`
is a `ViewTarget` (`home | album | playlist | favorites | search | viewAll | artist | …`,
`ref:sone/src/types.ts:213-283`) plus `__navId`/`__navSession` bookkeeping. Destination-page atoms
carry optional `*Info` payloads (title/cover the navigating component already has), so the
destination renders a filled header instantly and fetches the rest — a cheap perceived-performance
trick worth copying regardless of framework. `useNavigation.navigate()`
(`ref:sone/src/hooks/useNavigation.ts:17-28`) dismisses the drawer/maximized player on every
navigation, stamps the view via `pushView` (scroll-memory + `__navId`), and wraps the write in
`startTransition` so React shows the destination skeleton without blocking on unmounting the
previous page. The `popstate` listener lives in `AppInitializer` — browser history is bridged to
the atom, not owned by a router.

**Scroll restoration** (`ref:sone/src/hooks/useScrollRestoration.ts`) is the hard part and is
fully worked out: `QUIET_MS = 3000` (list growth restarts the quiet period so a slow first page
keeps the restore alive), `MAX_RESTORE_MS = 15000` ceiling so an infinitely-growing feed is not
chased forever, abort on `wheel`/`pointerdown`/`keydown` (`pointerdown` specifically so a
scrollbar-thumb drag — which emits `scroll` with no `wheel` — wins against an in-flight restore),
`SMOOTH_RUNWAY = 1.5` viewport jump-then-glide so long restores don't read as a blur,
`SETTLE_ANIMATION_MS = 1200` during which recording is paused so intermediate positions don't
overwrite the offset being restored, and a `prefers-reduced-motion` check. Related hooks:
`useInfiniteScroll`, `useRestoreLoader`, `useViewTab`, `useEscapeDismiss`, `useContextMenu`,
`useShortcuts` (35 hook files total in `src/hooks/`).

## 4. Caching, settings, secrets

**Disk cache** (`cache.rs`): four tiers with TTL and stale-while-revalidate grace — table, the 2 GiB
cap, LRU eviction, the directory-versioning migration mechanism, and the encrypt-every-entry
consequence for startup ordering are owned by
`streamboat-engineering-baseline/references/config-cache-logs-telemetry.md` §2; cite it rather than
restating the tiers here.

Entries are AES-GCM-encrypted on disk, **keyed by a SHA-256 hex digest**
(`ref:sone/src-tauri/src/cache.rs:2,654-658` — a prior pass called this "FNV-style"; that hash is
a *different, frontend-only* cache, see immediately below), indexed by tag for invalidation,
evicted LRU, with `mark_in_flight`/`should_retry_refresh` guards against refresh storms.

**Frontend cache** (`src/api/tidal.ts`) is a *second*, separate, in-memory, size-based LRU capped
at 150 MB, with TTLs `SHORT = 2 min` (search/suggestions), `MEDIUM = 2 h` (lyrics, playlists,
favorites, mixes, page sections), `STATIC = 24 h` (albums, artists, credits), **FNV-1a** hashed
keys (`src/api/tidal.ts:55-62`, magic constants `0x811c9dc5`/`0x01000193`) and a tag index. Do not
conflate the two caches' hash functions — the disk cache is SHA-256, the frontend cache is
FNV-1a.

**Settings** (`lib.rs:121-215`): one `Settings` struct serialized to
`~/.config/sone/settings.json`, encrypted, with transparent migration from plaintext. Notable
fields: `auth_tokens`, `client_id`, `client_secret`, `auth_method`, `volume`, `last_track_id`,
`minimize_to_tray`, `decorations`, `titlebar_migration_v1`, `volume_normalization`,
`exclusive_mode`, `exclusive_device`, `bit_perfect`, `gapless` (default true), `max_quality`
(default `"HI_RES_LOSSLESS"`), `scrobble`, `proxy`, `discord_rpc`, `discord_status_text`,
`legacy_auth_notice_count` (the doc comment at `lib.rs:164-167` claims a cap of 5 and "never
resets" — **both halves are wrong**: the real cap is `const LEGACY_AUTH_NOTICE_LIMIT: u8 = 3`
gated `>= LEGACY_AUTH_NOTICE_LIMIT` at `commands/auth.rs:484,493`, and it *is* reset to 0 in two
places, `commands/auth.rs:395,561` — read the expression, not the comment),
`mcp_enabled`/`mcp_port` (5577)/`mcp_token`, `overlay_enabled`/`overlay_port`
(5578)/`overlay_host` (127.0.0.1 default — this is a user setting, not hard-coded;
`overlay/server.rs:31-34` explicitly handles a `0.0.0.0` bind, unlike the MCP server which is
loopback-only), `report_plays` (default true). **Settings-file location is Linux-only in this
description** — Sone hardcodes `~/.config/sone/`; streamboat's own settings/cache/log locations
must be resolved per platform (`~/.config/streamboat`, Windows `%APPDATA%\streamboat`, macOS
`~/Library/Application Support/streamboat`), not assumed to be a single `dirs`-crate default that
happens to match Linux.

**Proxy support** (`ProxySettings`, the `proxy` field above): `build_http_client` builds an HTTP or
SOCKS5 `reqwest::Proxy` from user-supplied host/port/credentials, but only after rejecting a host
string containing `@`, `/`, `?`, `#`, or whitespace — characters that could otherwise break the
`scheme://host:port` string it builds and inject into the proxy URL
(`ref:sone/src-tauri/src/tidal_api.rs:60-86`). A malformed host silently falls back to no proxy
rather than erroring. Worth copying as-is if streamboat exposes a user-configurable proxy: the
validation is small, cheap, and the injection risk it closes is real.

**Crypto** (`crypto.rs`): AES-256-GCM. On-disk layout `b"SONE" | version:u8 | nonce:12 |
ciphertext+tag`; `decrypt` returns non-magic input verbatim so unencrypted legacy files migrate
transparently. Key resolution, `load_or_generate_key` (`ref:sone/src-tauri/src/crypto.rs:78-110`),
is three branches, not one path that "always" writes a file: (1) OS keyring hit
(`keyring::Entry::new("sone","master-key")`, must be exactly 32 bytes) → return immediately,
writes nothing; (2) existing file `~/.config/sone/sone.key` → opportunistically push the key into
the keyring, still writes no file; (3) neither exists → generate a fresh key **and only here**
write the file backup, with the comment "keyring may be unreachable on next launch (e.g. AppImage
with a different D-Bus session)" attached to this branch specifically, not to the function as a
whole. **Correction, previously stated as "always written even when the keyring works" here** —
that copies Sone's comment without reading which branch it sits in; a keyring-first user on a
second run never gets a key file at all. File mode `0600` on Unix; the raw key is `zeroize`d after
constructing the cipher. Canonical, line-by-line source:
`streamboat-engineering-baseline/references/secrets-and-tokens.md` §3.

**Embedded credentials** (`embedded_config.rs`, generated by `scripts/gen_embedded.py`): four
credential strings as XOR-masked byte arrays — `stream_key_a/b` (device-code id/secret),
`stream_key_c/d` (PKCE id/secret), decoded by `decode(data, mask) = data[i] ^ mask[i]`.
`has_stream_keys()`/`has_pkce_keys()` check for a `PLACEHOLDER` prefix so public builds can ship
without them. **This is obfuscation, not security** — trivially reversible, and the variable
naming (`STREAM_SALT_*`, `CODEC_HINT_*`) is deliberately misleading about what it does. **Avoid
this exact pattern in streamboat** — if credentials ship at all, say so plainly.

## 4a. `countryCode` provenance and account-endpoint log redaction (added by the third fact-check
pass)

Nearly every unofficial v1 call takes `countryCode`; Sone bootstraps it from `GET
https://api.tidal.com/v1/sessions` (no query params), which returns `{userId, countryCode}` — the
same call is how the client learns its own user id for `/users/{id}/…` endpoints. **The default
before login is the literal `"US"`** (`ref:sone/src-tauri/src/tidal_api.rs:1290,1307`), so an
un-bootstrapped client silently serves the US catalogue rather than failing outright — decide that
trade-off (fail loudly vs. default) deliberately in streamboat rather than inheriting it by
accident. Also worth copying: the error logger suppresses response bodies for account endpoints —
`let is_account_endpoint = url.contains("/users/") || url.contains("/sessions"); …
body=<redacted: account endpoint>` (`:1470-1483`) — a cheap, concrete redaction rule for a client
whose logs users will paste into bug reports.

## 4b. The playlist/favorites-mutation ETag precondition (added by the third fact-check pass)

The report is otherwise entirely read-path; this is the write-path gap. Every mutation GETs the
playlist first, reads the `etag` response header (defaulting to `"*"` if absent), and sends it
back as `If-None-Match` on the write — `add_track_to_playlist`
(`ref:sone/src-tauri/src/tidal_api.rs:1973-1999`: GET `/playlists/{id}?countryCode=…` → etag →
`POST /playlists/{id}/items`, form body `trackIds`, `onDupes=FAIL`, `onArtifactNotFound=FAIL`),
the same pattern at `:2029-2044` (remove/reorder), `:2072-2084` (`DELETE /playlists/{id}`) and
`:3615-3632`. **python-tidal does the same independently**: it caches `self._etag` from every
playlist fetch (`ref:python-tidal/tidalapi/playlist.py:69,91,228,273,534`) and sends
`{"If-None-Match": self._etag}` on each mutation (`:593,626,717,758`). Two things to design in:
the etag here is a **write precondition**, not a cache validator (nobody in the set sends it on a
*read* to get a cheap 304 — see `api-auth-streaming.md` §10 for that unexploited affordance), and
`onDupes`/`onArtifactNotFound` (fail vs. skip on a duplicate or a dead track id) is a real API
surface worth exposing deliberately. Skip this and every playlist mutation returns an
inexplicable 412/428.

## 4c. The rate-limit contract, with actual numbers (added by the third fact-check pass —
`rate_gate.rs` was named four times elsewhere in this skill with no numbers attached)

Whole contract, 62 lines, `ref:sone/src-tauri/src/rate_gate.rs`: `DEFAULT_COOLDOWN_SECS = 5`
("long enough to break a debounced prefetch loop, short enough not to feel bricked") used when a
429 carries no usable `Retry-After`; `MAX_COOLDOWN_SECS = 120` clamp, because an unclamped server
value could brick the client for hours. State is a single `AtomicU64` absolute deadline —
lock-free, consultable while the client's own mutex is held; **sleeping is always the caller's
job, with the mutex released**. `trip_at` uses `fetch_max`, so concurrent 429s can only lengthen,
never shorten, the cooldown. `parse_retry_after_value` accepts **only the delta-seconds form** and
deliberately rejects the HTTP-date form rather than mis-parsing it — the documented failure mode
is a *shorter* cooldown than intended, never garbage. Known caveat left in-source: the absolute
wall-clock deadline means a backwards clock jump can outlast the intended duration, bounded only
by the 120 s clamp. `cooling_down_at(now)`/`trip_at(now, secs)` take `now` as a parameter purely
for testability — copy that shape for testable rate-limit code generally.

## 4d. Scrobble threshold and trigger points (added by the third fact-check pass)

`meets_threshold()` first guards on track length — **tracks 30 seconds or shorter never scrobble**
(`if self.track.duration_secs <= 30 { return false; }`, `ref:sone/src-tauri/src/scrobble/mod.rs:
146-149`, whose doc comment names it explicitly: "track is longer than 30 seconds") — then applies
`listened >= duration/2 || listened >= 240.0` seconds (`:143-152`), evaluated at four points —
track change (`:246`), a periodic peek at the current track (`:314-321`), explicit user stop
(`:336-342`), and shutdown (which also persists the queue, `:355-361`) — each guarded by a
`scrobbled` flag so a track is never double-counted. **Copy the 30-second guard, not just the
percentage/floor rule** — without it, interludes and skits scrobble on first play. **The official
client uses the identical percentage/floor rule** independently:
`PlayState.MIN_SCROBBLE_DURATION = 240000` ms, `MIN_SCROBBLE_PERCENTAGE = 0.5`
(`ref:TidaLuna/plugins/lib/src/classes/PlayState.ts:11-30`), tracking `cumulativePlaytime`
**separately from wall-clock position** because it "can be longer than track duration" — seeking
backwards and replaying a section accumulates playtime, so drive the threshold from accumulated
playback, not a point-in-time position check. Providers implemented: Last.fm, Libre.fm,
ListenBrainz, MusicBrainz, sharing one offline queue.

## 5. Theming, lyrics, miniplayer, integration surfaces, music videos, play reporting

- **Theming** (`theme_config.rs`): external `<config>/theme.json`, `{version:1, preset,
  custom:{accent, background}}`, 15 named presets kept in sync with `src/lib/theme.ts` (*Violet
  Night, Cyberpunk, Forest, Ocean, Midnight Cyan, Sakura, Rose, Ember, Copper, Noir, Daylight,
  Snowfall, Paper, Meadow, Blossom*). Colours derived from just two seed colours plus preset.
- **Lyrics**: one command, `get_track_lyrics` → `GET /v1/tracks/{id}/lyrics` returning
  `{lyrics, subtitles, isRightToLeft, lyricsProvider, providerLyricsId}`. The **synced** payload
  is `subtitles`, not `lyrics`. Parsing detail (worth copying, both under 100 lines):
  `ref:sone/src/lib/lrc.ts::parseLrc`'s timestamp regex is
  `/(\d{1,2}):(\d{2})(?:[.:]([\d]{1,3}))?/` — note it accepts a **colon or a dot** before the
  fractional part and 1-3 fractional digits, so a strict `[mm:ss.xx]`-only parser silently drops
  lines TIDAL actually sends. Active line: scan `lrcLines` in reverse for the first line whose
  timestamp is `<=` current position (`MaximizedPlayer.tsx:522,593-639`). High Tide's independent
  implementation (`ref:high-tide/src/widgets/lyrics_widget.py:95-175`) uses a stricter
  `\[(\d+):(\d+\.\d+)\](.*)` regex, switches a `Gtk.ListView` between `SingleSelection` (synced,
  clickable — seeks on click) and `NoSelection` (plain text), and does the same
  reverse/forward-scan-and-center approach.
- **Miniplayer**: a second Tauri window (`miniplayer.html` → `src/miniplayer-main.tsx`), driven by
  `useMiniplayerWindow`/`useMiniplayerBridge`/`useMiniplayerEmitter`.
- **MPRIS** (`mpris.rs`): `mpris-server 0.9`, driven by an `MprisCommand` enum over a tokio
  unbounded channel (`SetMetadata`, `SetPlaybackStatus`, `SetVolume`, `Seeked`, `SetShuffle`,
  `SetLoopStatus`, `SetFullscreen`, `Stop`) — keeps D-Bus off the audio thread. Worth copying
  as-is.
- **MCP server** (`mcp/`): `rmcp 1.7.0` streamable-HTTP over `axum`, **loopback-only**
  (`127.0.0.1:5577`), URL path contains a persistent UUID token
  (`http://127.0.0.1:{port}/{token}/mcp`), off by default. Tools cover catalog, playback,
  playlists, favorites, state.
- **OBS overlay** (`overlay/server.rs`): `axum`, self-contained HTML page, off by default. Default
  bind `127.0.0.1:5578`, but `overlay_host` is user-configurable and `0.0.0.0` is explicitly
  handled — do not assume this one is loopback-locked the way MCP is.
- **Play reporting** (`tidal_report/`): reports plays back to TIDAL so "Recently Played" works,
  capturing the *actually served* `audioQuality`/`audioMode`/`assetPresentation` from the
  playbackinfo response. User-disableable via `report_plays` (default on). Full wire format (the
  `ec.tidal.com/api/event-batch` endpoint, the AWS SQS `SendMessageBatch` form encoding, the nine
  `Headers` keys, the pinned Android device identity, the 30-second threshold, JWT attribution) is
  owned by `tidal-api/references/play-logging-and-privileges.md` §1-4 — cite it rather than
  restating. Sone-implementation specifics worth keeping here: **Mint `playbackSessionId` at
  stream-resolution time, not report time** — it is the same id the official SDK sends as
  `x-playback-session-id` on its manifest request, so one id must span resolve → play → report.
  The offline outbox (`tidal_report/queue.rs`) is encrypted with the same `Crypto` container as
  settings, capped at `MAX_ENTRIES = 500` /
  `MAX_ATTEMPTS = 10` / `MAX_AGE_SECS` = 14 days, and — worth copying regardless of whether
  streamboat impersonates anything — stores **only the frozen `MessageBody`**; the bearer-token-
  carrying `Headers` attribute is rebuilt fresh at send time, so a stolen queue file leaks no live
  token. Outcome is a four-way enum: `Accepted`; `AuthFailed` → refresh once, retry, else queue;
  `Retryable` → requeue; `SenderFault` → drop, never requeue. **Decision this forces**: making
  "Recently Played" work on the unofficial path means maintaining a client-identity pin that must
  be bumped roughly as often as TIDAL ships — there is no visible middle ground between doing that
  and not reporting plays at all.
- **Music videos are not in the Rust/GStreamer/ALSA engine at all.** `GET
  /videos/{id}/playbackinfopostpaywall` (`tidal_api.rs:3816`) and `GET /videos/{id}`
  (`tidal_api.rs:3871`) exist, but playback is `hls.js 1.6.16` inside the webview
  (`package.json`) — a second, entirely separate media path — and
  `ref:sone/src/hooks/useGaplessPrefetch.ts:65` disables audio gapless arming outright whenever
  the current queue item is a video. A headless/daemon core cannot render video: decide whether
  streamboat treats video as desktop-UI-only (Sone's answer) or out of scope, rather than
  discovering the gap when a video row shows up in a playlist.
- **Idle inhibit**: three implementations — D-Bus, Wayland (`idle-inhibit` protocol), X11
  (`x11rb` screensaver/dpms).
- **Sone does not auto-update.** `commands/updates.rs::check_for_update` `GET`s
  `api.github.com/repos/lullabyX/sone/releases/latest`, compares `tag_name` (semver) to
  `env!("CARGO_PKG_VERSION")`, and returns availability — no download, no install, no signature
  verification. Any failure is silently treated as "no update." See
  `packaging-distribution.md` for the per-channel update-mechanism decision this implies for
  streamboat.

## 6. Packaging and CI — reusable scripts, but no CI at all

Full packaging matrix and Flathub-manifest facts are in `packaging-distribution.md`. Summary:
`ref:sone/build-scripts/build/{deb,rpm,pacman,all}.sh` + matching Dockerfiles + a `PKGBUILD` that
repacks the built `.deb` (`ar x` + `tar xf data.tar.*`) — genuinely reusable, one build artifact
feeding four distro formats. `ref:sone/flake.nix`+`nix/package.nix`, `ref:sone/snap/
snapcraft.yaml` (core24 strict, documented `snap connect sone:alsa`), `ref:sone/data/
io.github.lullabyX.sone.{desktop,metainfo.xml}`, `ref:sone/sync-version.mjs` (one version source
across Cargo/Tauri/PKGBUILD/AppStream).

**But `ref:sone/.github/` contains exactly six files**: `FUNDING.yml`, four
`ISSUE_TEMPLATE/*.md`, and `workflows/flathub-update.yml` (which only opens a Flathub PR on a
tag). **There is no build, test, or lint CI at all** — nothing runs the 151 Rust tests or
`clippy` automatically; every packaged artifact is produced by a maintainer running the shell
scripts locally. If streamboat reuses these packaging scripts (recommended), it still needs to
build its own CI pipeline from scratch — there is nothing to copy for that half.

## 7. Code quality: strong source, weak process

Extensive design-rationale comments (the `2b-A1/A2/A3`, `C1/C3/C5` markers reference an internal
refactor plan), `cargo clippy -- -D warnings` + `cargo fmt --check` in a local `check` script,
`knip` for dead frontend code, 151 Rust tests + 46 frontend test files — **none of it enforced by
CI (§6)**. Weaknesses in the source: `tidal_api.rs` at 7,200 lines and `audio.rs` at 3,309 lines
are monoliths; single maintainer, 64 open issues; `image`/`id3`/`walkdir` pulled in for narrow
uses.

**What's actually reusable is narrower and more specific than "port the whole file"**: the parts
most worth porting are already pure functions with tests around them —
`quality_tiers(ceiling, has_secret)` (`commands/playback.rs:32-42`),
`RateGate::cooling_down_at(now)`/`trip_at(now, secs)` (deliberately take `now` as a parameter so
they're testable without a real clock, `rate_gate.rs`), and
`parse_release_tag`/`is_update_available` (`commands/updates.rs:18-28`). The hardware-dependent
ALSA probing (`audio-engineering.md` §1) cannot be unit-tested this way and is covered only by
the unrun `gapless_probe.py` — streamboat needs its own hardware test matrix for that part no
matter how much code is ported.

**"151 tests" does not mean there is a test harness for the hard part — read the numbers
correctly.** `ref:sone/src-tauri/Cargo.toml` `[dev-dependencies]` is exactly one line —
`tempfile = "3"`. There is no `mockito`, `wiremock`, `httpmock`, or recorded-response fixture
anywhere in the crate; all 151 tests are inline `#[cfg(test)]` modules covering **pure functions
only** — the HTTP layer, the manifest parsers against real payloads, and the whole ALSA path have
**zero** automated coverage. The frontend's Vitest tests use hand-written `invoke` stubs
(`src/hooks/useGaplessPrefetch.test.ts:36-39`), not a request-mocking library. Contrast:
`ref:tidalrs/Cargo.toml` ships `mockito` as a dev-dependency, and Music Assistant's TIDAL provider
has real mocked-API tests (`project-profiles.md` §5a). **The transferable pattern — parser/
transport split plus captured request/response fixtures replayed in CI — is not demonstrated by
any project in this reference set.** It is work streamboat must design, not code to port; see
`docs/research/engineering-baseline.md`.

## 8. Borrow / avoid, consolidated

**Borrow (with pointers):**
- The whole ALSA bit-perfect negotiation module — see `audio-engineering.md` §1 for the full
  breakdown and line numbers.
- The `subStatus` taxonomy: `tidal_api.rs:11-43` — see `api-auth-streaming.md` §6.
- The quality-cascade stopping rules: `commands/playback.rs:32-84` — see
  `api-auth-streaming.md` §7.
- The `norm_gain` formula and album-vs-track context selection — see `audio-engineering.md` §4.
- The cache tier/TTL/SWR table: `cache.rs:18-57` (§4 above).
- The crypto container format and keyring-plus-file key strategy: `crypto.rs` (§4 above).
- The serialized attach/detach executor pattern for gapless — see `audio-engineering.md` §2.
- The packaging scripts and the `sync-version.mjs` idea (§6 above; full detail in
  `packaging-distribution.md`).
- MPRIS-over-a-command-channel: `mpris.rs:8-46` (§5 above).
- **Signal-path transparency as a cheap differentiator**: `signal_path.rs` (PRISTINE verdict,
  reads `pactl` + `/proc/asound`) and `pipeline_probe.rs` (pad-caps probes, decoded vs. output) —
  see §2's module map. Once streamboat has the equivalent pipeline probes for its own audio path
  (needed anyway for bit-perfect verification, §1 above), showing the user the decoded format,
  every conversion, and what the device actually received costs little extra and is a real
  user-facing feature no other project in this set exposes this clearly.
- The pure decision-functions-with-tests pattern (§7) as a template for what to port with
  confidence vs what needs a hardware test matrix.
- The anchor-poll-plus-interpolate position model and its gapless-boundary settle-window guard
  (§3 above) — this is the corrected, positive lesson, not the "N pollers" anti-pattern an earlier
  draft of this skill described.
- The track-availability pre-flight + narrow error-classification allowlist (§3c) — the
  single most immediate thing to design before writing queue/auto-advance logic.
- The five-collection queue model and the desired-vs-actual pause-state split (§3d).
- The ETag write-precondition pattern for playlist/favorites mutations (§4b) — get this wrong and
  every mutation 412s.
- The rate-limit contract's actual numbers (§4c) and the scrobble threshold's cumulative-playtime
  rule (§4d) — both are small, concrete, and otherwise easy to get wrong by guessing.

**Design deliberately, do not simply avoid:**
- Play reporting's Android-client-identity impersonation (§5, play reporting) — decide whether
  streamboat reports plays at all before discovering that doing so on the unofficial path means
  maintaining a version pin that drifts roughly as often as TIDAL ships.

**Avoid (with reasons):**
- The queue-in-the-frontend architecture — see §3. This is the single most important thing to
  invert, not copy.
- `embedded_config.rs` XOR obfuscation — security theatre with dishonestly-named identifiers.
- 7,200-line `tidal_api.rs` — split by resource (auth, catalog, playback, library, playlists,
  pages, search, user) from day one, behind one shared request layer that injects
  `countryCode`/bearer/`x-tidal-client-version` and handles 401/refresh centrally — see
  `api-auth-streaming.md` §4a; splitting by resource alone does not give you that layer.
- `csp: null` in `tauri.conf.json` — set a real CSP.
- Reproducing zero CI — Sone's *scripts* are reusable, its *process* is not.
- `=`-pinning every Tauri crate — makes security updates a manual chore; pin the toolchain, use a
  lockfile for the rest.
- Shipping no update mechanism at all without a deliberate per-channel decision (§5, last bullet;
  full decision framing in `packaging-distribution.md`).

Binding MCP/overlay servers without an explicit opt-in is avoided *correctly* by Sone (both
default off) — keep that, do not "helpfully" enable either by default.

## 9. Sone-windows — the Windows fork, and why one codebase could serve both

`ref:sone-windows` · https://github.com/lvllaby/sone-windows · GPL-3.0-only · v0.16.0 · last
commit 2026-05-17.

**A fork, not a port layer.** Missing entirely vs upstream Sone: `mcp/`, `overlay/`,
`tidal_report/`, `signal_path.rs`, `pipeline_probe.rs`, `theme_config.rs`, `rate_gate.rs`,
`http_util.rs`, `logging.rs`, `commands/{feed,profile,updates,mcp,overlay}.rs`. Added:
`media_controls.rs` (souvlaki SMTC), a flat `idle_inhibit.rs` (vs Sone's `idle_inhibit/`
directory). Rust LOC: 15,753 vs Sone's 26,721. Dependency deltas: loose `"2"` version specs
instead of Sone's `=` pins; `env_logger 0.11` instead of `flexi_logger`; missing `rmcp`, `axum`,
`schemars`, `tokio-util`, `tokio-stream`, `futures-util`, `uuid`, `semver`, `x11rb`, `wayland-*`.
Adds `[target.'cfg(target_os = "windows")'.dependencies] souvlaki = "0.8.3"` for SMTC.

**The Windows audio change is small and localized** (`audio.rs` ~1230-1246): sink construction is
`#[cfg]`-split — Linux gets `autoaudiosink`; Windows gets **`wasapi2sink`** (not `wasapisink`)
with `exclusive` and `low-latency` properties and an optional `device`. Device enumeration is
also `#[cfg]`-split: Linux filters ALSA devices by `device.path`; Windows accepts the `wasapi2` or
`wasapi` API and reads `device.id`. The custom ALSA writer path (§1 in `audio-engineering.md`) is
Linux-only; on Windows, bit-perfect relies entirely on `wasapi2sink exclusive=true`.

**GStreamer runtime bundling**: `ref:sone-windows/scripts/prepare-gstreamer.js` scans
`src-tauri/gstreamer-runtime/**` for DLLs and generates both an NSIS macro
(`NSIS_HOOK_POSTINSTALL` copying core DLLs, `lib/gstreamer-1.0` plugins, `lib/gio/modules`) and a
WiX fragment — directly reusable for any GStreamer-on-Windows app, regardless of what else
streamboat borrows from Sone.

**The actual bundled plugin list answers the "does the Windows bundle need `gstreamer1.0-libav`"
licensing question — with a codec-coverage cost attached.** `tauri.conf.json`
`bundle.windows.wix.componentRefs` lists 47 components, 16 of them GStreamer plugins:
`gstadaptivedemux2`, `gstasio`, `gstaudioconvert`, `gstaudioparsers`, `gstaudioresample`,
`gstcoreelements`, `gstdash`, `gstdecklink`, `gstflac`, `gstisomp4`, `gstplayback`, `gstsoup`,
`gsttypefindfunctions`, `gstvolume`, `gstwasapi2`, `gstwinks`. **No `gstlibav`, no AAC decoder of
any kind.** So the Windows bundle *is* LGPL-clean without FFmpeg — but it can only play
FLAC-in-fMP4/DASH; TIDAL's `HIGH`/`LOW` tiers are AAC (`mp4a.40.2`/`mp4a.40.5`) and would fail to
decode. There is no LGPL-clean AAC decoder in GStreamer at all: `avdec_aac` is FFmpeg-derived,
`faad` (`gst-plugins-bad`) is GPL-encumbered, `fdkaacdec` carries the Fraunhofer FDK licence.
"Exclude libav" in practice means "drop the lossy tiers on Windows, or ship a differently-licensed
decoder and price that separately." Also: the same file's `bundle.linux.deb.depends`/
`bundle.linux.rpm.depends` **do** include `gstreamer1.0-libav`, so Sone's Linux packages get AAC
today and its Windows bundle silently does not — do not inherit that platform mismatch without
deciding it on purpose.

**Maintenance posture, verbatim from the README**: *"⚠️ Ported with help from AI agents"* and
*"This Windows port was created for personal use and **may not be actively or correctly
maintained** in the future."*

**Could one codebase serve both? Yes — nothing in the divergence is architectural.** It is one
sink-construction block, one device-enumeration block, one media-controls module, and a build
script. The fork exists because forking was easier than upstreaming a `#[cfg(target_os)]`
boundary, and it is now five minor versions behind and drifting. **Lesson for streamboat: put the
platform split behind a trait/`cfg` boundary in the audio module from commit one, and never fork
for a platform** — this is a stronger, more concrete version of the same lesson as §3's
state-ownership inversion: both are about drawing the seam in the right place before the second
platform/client exists, not after.

**Correction (third fact-check pass): Sone-windows is Windows-only, not "Windows (+Linux/mac via
Tauri)."** Every sink-construction and device-enumeration branch in `audio.rs` is
`#[cfg(target_os = "linux")]` or `#[cfg(target_os = "windows")]` only — there is **no macOS arm**,
so this fork does not build on macOS. `souvlaki` (which does support macOS upstream) is declared
only under `[target.'cfg(target_os = "windows")'.dependencies]`, confirming there is no macOS
media-controls code here either (`grep -rn macos ref:sone-windows/src-tauri/Cargo.toml` — no
hits). **This means the entire reference set contains zero Tauri/Rust macOS TIDAL precedent** —
see `audio-engineering.md` §6 (output) and `packaging-distribution.md` §8 (the full five-item
macOS work list: output, media integration, signing, GStreamer bundling, keyring) ("Tauri is
cross-platform" is not evidence that a `#[cfg]`-split GStreamer sink is; the macOS arm was simply
never written, upstream or in this fork).

## 10. Bus factor / contributor counts

Now obtainable via `GET /repos/{owner}/{repo}/contributors?per_page=100&anon=1` — the "not
obtainable" caveat in earlier drafts of this skill is stale. **Sone: 18 contributors,
`lullabyX` 1,046 of ~1,081 commits (~97%) — bus factor 1.** This is not unusual in this landscape:
of the projects examined, only High Tide (45 contributors, ~68% top author, a real long tail) and
mopidy-tidal (12 contributors, a genuine 3-person history: `2e0byo` 372 / `tehkillerbee` 191 /
`blacklight` 89) are not effectively single-maintainer. **Strawberry — read as "very mature, very
active" elsewhere in this skill — is also bus factor 1 in relative terms**: `jonaski` has 5,678
commits against the next *human* contributor at 28 (`LebedevRI`); the #2 entry by commit count is
Strawberry's own bot (`strawbsbot`, 553). Read "mature" as age + release cadence + CI, not as
community size, when weighing how much to lean on any single reference project — including Sone
itself.
