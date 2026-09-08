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
3. IPC design — and the state-ownership inversion streamboat must NOT copy
3a. Seeking
3b. Gapless prefetch/arming policy (the frontend half of gapless)
4. Caching, settings, secrets
5. Theming, lyrics, miniplayer, integration surfaces, music videos, play reporting
6. Packaging and CI — reusable scripts, but no CI at all
7. Code quality: strong source, weak process (and what "151 tests" does not mean)
8. Borrow / avoid, consolidated
9. sone-windows — the Windows fork, and why one codebase could serve both
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

**This is why sone-windows is a fork rather than a genuinely portable core** (§9), and it is the
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
webview for it to act on, not down from it (the same inversion as the queue). **Position is
polled, not pushed**: the frontend runs `setInterval(syncPosition, 500)`
(`ref:sone/src/hooks/useProgressScrub.ts:38`); the miniplayer, overlay, and signal-path panel each
poll independently on their own separate intervals. **Make an explicit push-vs-poll decision for
streamboat's own core↔UI protocol** rather than reproducing N independent ad hoc pollers.

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

## 4. Caching, settings, secrets

**Disk cache** (`cache.rs`): four tiers with TTL and stale-while-revalidate grace:

| Tier | Contents | TTL | SWR grace | Subdir |
| --- | --- | --- | --- | --- |
| `UserContent` | playlists, likes, favorites | 15 min | 1 h | `user` |
| `Dynamic` | artist bios, charts, home page | 4 h | 24 h | `dynamic` |
| `StaticMeta` | album tracklists, credits | 7 d | 30 d | `static` |
| `Image` | album art, avatars | 30 d | 90 d | `images` |

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
loopback-only), `report_plays` (default true).

**Crypto** (`crypto.rs`): AES-256-GCM. On-disk layout `b"SONE" | version:u8 | nonce:12 |
ciphertext+tag`; `decrypt` returns non-magic input verbatim so unencrypted legacy files migrate
transparently. Key resolution: OS keyring (`keyring::Entry::new("sone","master-key")`, must be
exactly 32 bytes) → file `~/.config/sone/sone.key` → generate. **A file backup is always written
even when the keyring works**, because "keyring may be unreachable on next launch (e.g. AppImage
with a different D-Bus session)." File mode `0600` on Unix; the raw key is `zeroize`d after
constructing the cipher.

**Embedded credentials** (`embedded_config.rs`, generated by `scripts/gen_embedded.py`): four
credential strings as XOR-masked byte arrays — `stream_key_a/b` (device-code id/secret),
`stream_key_c/d` (PKCE id/secret), decoded by `decode(data, mask) = data[i] ^ mask[i]`.
`has_stream_keys()`/`has_pkce_keys()` check for a `PLACEHOLDER` prefix so public builds can ship
without them. **This is obfuscation, not security** — trivially reversible, and the variable
naming (`STREAM_SALT_*`, `CODEC_HINT_*`) is deliberately misleading about what it does. **Avoid
this exact pattern in streamboat** — if credentials ship at all, say so plainly.

## 5. Theming, lyrics, miniplayer, integration surfaces

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
  playbackinfo response. User-disableable via `report_plays` (default on). **Wire format**: POSTs
  to `https://ec.tidal.com/api/event-batch` (`tidal_report/event.rs:6`). `SessionEvent` carries
  `session_id`, `requested_product_id`, `actual_product_id`, `quality`, `audio_mode`,
  `presentation`, `source`, `start_ts_ms`, `end_ts_ms`, `end_asset_pos` (`event.rs:89-100`),
  serialized as a mobile-shaped JSON body with `playbackSessionId`, `isPostPaywall: true`,
  `productType: "TRACK"` (`event.rs:103-115`) — note requested vs. actual product id/quality are
  both reported. There is a retry/offline queue (`tidal_report/queue.rs`). **Mint
  `playbackSessionId` at stream-resolution time, not report time** — it is the same id the
  official SDK sends as `x-playback-session-id` on its manifest request, so one id must span
  resolve → play → report.
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
has real mocked-API tests (`project-profiles.md` §8a). **The transferable pattern — parser/
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
- The pure decision-functions-with-tests pattern (§7) as a template for what to port with
  confidence vs what needs a hardware test matrix.

**Avoid (with reasons):**
- The queue-in-the-frontend architecture — see §3. This is the single most important thing to
  invert, not copy.
- `embedded_config.rs` XOR obfuscation — security theatre with dishonestly-named identifiers.
- 7,200-line `tidal_api.rs` — split by resource (auth, catalog, playback, library, playlists,
  pages, search, user) from day one.
- `csp: null` in `tauri.conf.json` — set a real CSP.
- Reproducing zero CI — Sone's *scripts* are reusable, its *process* is not.
- `=`-pinning every Tauri crate — makes security updates a manual chore; pin the toolchain, use a
  lockfile for the rest.
- Shipping no update mechanism at all without a deliberate per-channel decision (§5, last bullet;
  full decision framing in `packaging-distribution.md`).

Binding MCP/overlay servers without an explicit opt-in is avoided *correctly* by Sone (both
default off) — keep that, do not "helpfully" enable either by default.

## 9. sone-windows — the Windows fork, and why one codebase could serve both

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
