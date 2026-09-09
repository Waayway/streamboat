# Architecture (as built by the playable spike)

Companion to `docs/DECISIONS.md`: what exists in the workspace today and the
implementation choices made while building the first milestone (D-044).

## Crates

| Crate | Licence | Contents |
| --- | --- | --- |
| `streamboat-core` | Apache-2.0 | `http` (authenticated client, single-flight and cross-process refresh, 429 gate), `auth::device_code`, `auth::pkce`, `token_store` (AES-256-GCM file, `SBTK` header, keyring-held master key), `manifest` (BTS/EMU/DASH parsing, refusal rule), `api` (sessions, tracks, `playbackinfopostpaywall`, quality cascade, plus the catalogue/library surface below), `proto` (Command/Event), `config` (`AppDirs`, `Settings`, the control API's bearer-token file), `credentials` (device-code and PKCE pairs), `bootstrap`, `privileges` (the Pushkin streaming-privileges websocket, D-033), `reporting` (play reporting to `ec.tidal.com` and the server-anchored clock, D-027), `scrobble` (Last.fm/ListenBrainz, D-037), `diagnostics` (redacted logging, crash dumps, the debug-bundle archiver, D-029) |
| `streamboat-player` | GPL-3.0-only | `engine::Engine` trait, `gst::GstEngine` (playbin3, default feature), `mpv::MpvEngine` (libmpv2, `mpv` feature — D-016), `alsa_writer::ExclusiveSink` (exclusive-mode ALSA writer, Linux, `alsa-direct` feature, default on), `offline::OfflineCache` (pinned, encrypted offline cache, D-022), `player::Player` (queue, prefetch, Command→Event loop, `PlayerHandle::publish` for externally-sourced events, and the optional `PlayerDeps` wiring for the three modules above), `platform::default_engine`/`platform::enumerate_output_devices` (pick the compiled-in engine and list its output devices, see below), `media_controls::spawn` (picks the OS media-integration adapter below), `mpris` (Linux, feature `mpris`, default on: `org.mpris.MediaPlayer2.streamboat`), `smtc` (Windows, feature `smtc`, default on: SMTC via `MediaPlayer::SystemMediaTransportControls`), `nowplaying` (macOS, feature `nowplaying`, default on: `MPNowPlayingInfoCenter`/`MPRemoteCommandCenter`), `snapcast` (the fixed PCM format both engine backends target for `OutputConfig::Snapcast`, D-034) |
| `streamboat-server` | GPL-3.0-only | `streamboatd`: `api` (the HTTP + WebSocket control API, see below) hosted by default; `--stdio` keeps the original JSON-lines transport; headless login as `auth_required`/`auth_ok` events on the same broadcast every front end reads; constructs the privileges socket, play reporter and scrobblers from `Settings` |
| `streamboat-desktop` | GPL-3.0-only | `streamboat`: CLI subcommands (login [--pkce], logout, whoami, search, resolve, play, devices, keyring, paths, pin/unpin/pins, snapcast-plugin, snapcast-discover, debug-bundle) unchanged/extended as noted below; running with no subcommand now launches the iced shell (`src/ui/`) — see §"Desktop shell (iced)" below |

## Control API (D-030, D-031)

`streamboatd` hosts an `axum` HTTP + WebSocket server over the same
`PlayerHandle` the stdio transport and MPRIS use. Bound to `127.0.0.1:4747`
by default — streamboat's own pick (unclaimed by any control-API precedent
`headless-and-tidal-connect/references/daemon-architecture.md` §2 surveys;
not an owner decision, override with `--listen`). `--listen 0.0.0.0:<port>` or
`--lan` is the explicit LAN opt-in; either logs a warning naming the bind
address and the token file's path at startup. `--allow-host <name>` adds
extra `Host` header values beyond `localhost`/`127.0.0.1`/`::1` and (once
exposed) the literal bind address.

A random 32-byte token, hex-encoded, is generated on first start into a
0600 file at `<data dir>/control-token`
(`streamboat_core::config::load_or_create_control_token`) and printed at
startup. Every route but `GET /health` requires
`Authorization: Bearer <token>`; every route, `/health` included, is
rejected (403) unless its `Host` header matches the allowlist above — a
loopback bind and a token alone do not stop DNS rebinding
(`headless-and-tidal-connect/references/daemon-architecture.md` §3).

Routes:

- `GET /health` → `{version, protocol_version, capabilities: [...]}` —
  unauthenticated (still Host-checked), for capability negotiation.
- `GET /v1/state` → the current `PlayerState` snapshot.
- `POST /v1/commands` → a `Command` JSON body, enqueued and acknowledged
  with 202; the outcome arrives as an `Event`, not in the response.
- `GET /v1/events` → upgrades to a WebSocket. The first message is always
  a full `State` snapshot; every message after that is one `Event`. Every
  message is wrapped `{"revision": n, "event": {...}}`, `revision`
  monotonically increasing for the daemon's lifetime. **Reconnect rule**:
  a client that sees a gap in `revision` should request a new snapshot —
  either by reconnecting (a fresh connection always opens with one) or by
  sending a `{"type":"get_state"}` command on the same socket. The socket
  also accepts inbound `Command` JSON text messages, applied the same way
  `POST /v1/commands` does.

Tests: `crates/streamboat-server/tests/api.rs`, an in-process `axum` server
(ephemeral port) over the `FakeEngine` pattern
(`crates/streamboat-server/tests/common/mod.rs`) and a one-track wiremock
TIDAL, driven by real `reqwest` and `tokio-tungstenite` clients — health
without a token, 401 without/with the wrong token, Host-header rejection,
a POST command changing `/v1/state`, the WS snapshot-then-events envelope
with strictly increasing revisions, and an inbound WS command.

## Data flow

```
streamboat play 123 456
  └─ Context::load  → AppDirs, Settings, DeviceIdentity, ClientCredentials, EncryptedFileStore, ApiClient
  └─ GstEngine::new → playbin3, sink per OutputConfig, bus thread → EngineEvent channel
  └─ Player::spawn  → tokio task; commands in, Events broadcast out
       Play{items}  → api.track(id) per item → queue
       start(0)     → api.resolve_stream(id, ceiling, session_id)   [cascade, refusal]
                    → engine.load(LoadItem)                          [playbin uri, ReplayGain gain]
       Started(0)   → prefetch(1): resolve → engine.set_next(item)  [about-to-finish hand-over]
       StreamStart  → Finished(0), Started(1) → prefetch(2) ...
       Eos          → EndOfQueue
```

## Implementation choices recorded here

- **Seek flags**: `FLUSH | ACCURATE` (sample-accurate, Strawberry's choice)
  rather than `KEY_UNIT` (Sone/High Tide, segment-boundary snap on DASH).
- **DASH feeding**: `data:application/dash+xml;base64,…` when `dataurisrc`
  exists, else a per-session file under the runtime dir, deleted on drop.
  Manifests are never persisted beyond that.
- **`dashdemux2` demotion**: on GStreamer < 1.26.10 the modern demuxer is
  ranked `NONE` at init so the legacy `dashdemux` handles FLAC-in-DASH.
- **Exclusive mode (D-017, D-018)**: on Linux with the `alsa-direct` feature
  (default on), `OutputConfig::Exclusive` builds `audioconvert
  dithering=none noise-shaping=none ! appsink` (`crates/streamboat-player/
  src/alsa_writer.rs::ExclusiveSink`) instead of `alsasink`, fed by playbin
  through the same `native-audio` flag (no playbin-inserted conversion). A
  dedicated writer thread pulls samples from the appsink itself
  (`try-pull-sample`, not a signal callback) and owns the `snd_pcm_t`
  directly via the `alsa` 0.10 crate:
  - **Format**: probed once per device open (`S32LE`, ALSA `S24_LE`
    (= GStreamer `S24_32LE`), ALSA `S24_3LE` (= GStreamer `S24LE`), `FLOAT`,
    `S16LE`, in that priority). Each track's declared bit depth picks
    pass-through or the *narrowest lossless* promotion against that probed
    set (`pick_format`), pinned onto the appsink's `caps` property before
    the track's first buffer can reach it — no fallback ladder: a source
    with no lossless promotion available refuses to play rather than
    converting silently.
  - **Rate**: deliberately left unconstrained on the appsink caps. The
    writer opens `hw_params` with the rate the appsink actually receives,
    then reads `get_rate()` back and fails loudly ("DAC doesn't support N
    kHz; turn off bit-perfect mode for compatibility") if it differs —
    this read-back, not GStreamer negotiation, is what makes an
    unsupported rate an actionable message instead of an opaque pipeline
    error.
  - **Period before buffer** (1024 frames, then `4×` for the buffer),
    `sw_params.start_threshold`/`avail_min` restored after `hw_params`
    resets them, channel fallback to the device minimum with a
    stereo-to-N `mix-matrix` on the same `audioconvert`.
  - **Gapless (D-018)**: the PCM stays open across same-format tracks —
    reopen happens only when the writer observes an actual format/rate/
    channel change, with a 250 ms silence pre-roll after every reopen.
    Between tracks (queue gap, or paused) the writer feeds silence rather
    than draining, keeping the DAC clock alive.
  - **Recovery**: XRUN (`EPIPE`) → `prepare()` + a silence kick; suspend
    (`ESTRPIPE`) → bounded `resume()` retry, falling back to `prepare()`;
    `ENODEV` → a disconnected error, writer stops. Hardware pause
    (`snd_pcm_pause`) when the device supports it, else a software pause
    that keeps writing silence.
  - **Position**: corrected for the device's buffered-but-unplayed frames
    via `snd_pcm_delay()` (`frames_written − delay`), not
    `frames_written / rate` alone.
  - The `alsasink device=hw:X,Y` path (this section's previous shape)
    remains the fallback when the `alsa-direct` feature is off.
  - **Unverified until a DAC is available**: everything above is exercised
    by pure unit tests (format/promotion tables, rate-mismatch message,
    period/buffer/sw_params math, mix-matrix construction, XRUN/suspend/
    ENODEV classification) and by an integration test that plays a
    generated WAV through the real appsink → writer → ALSA `null` device
    path to `EndOfStream` (`crates/streamboat-player/tests/alsa_direct.rs`,
    skipped with a message if `null` can't be opened). `null` never XRUNs,
    suspends, or reports `ENODEV`, never reports a plausible non-2-channel
    minimum, and never actually reaches a non-44.1kHz rate mismatch, so the
    XRUN/suspend/disconnect/channel-fallback/rate-refusal paths, the real
    warm-up/settle behaviour of the 250 ms pre-roll, and whether hardware
    pause (`can_pause()`) is ever true in practice are all unverified
    against real hardware. Test on a DAC with:
    `RUST_LOG=debug streamboat play <id> --device hw:1,0 --exclusive`
    (unplug/replug the DAC mid-track for the `ENODEV` path; play an album
    that mixes bit depths or sample rates for the reopen/pre-roll path; a
    multi-channel USB interface for the channel-fallback/mix-matrix path).
- **ReplayGain**: a single `volume` audio-filter with TIDAL's formula
  `min(10^((gain+4)/20), 1/peak)`; bypassed in exclusive mode. Known boundary
  glitch between tracks with different gain; the per-branch element is a
  follow-up.
- **Quality ladder**: `HI_RES_LOSSLESS → LOSSLESS → HIGH → LOW`; the retired
  `HI_RES` tier is never requested. Hi-res tiers are skipped without a client
  secret; an encrypted manifest at a tier is a warning and the next tier is tried.
- **Token file format**: `"SBTK" || 0x01 || nonce(12) || AES-256-GCM(json)`;
  the key is 32 random bytes in a 0600 file. A bad header is a typed error.
- **Device identity**: `device.json` holds the `client_unique_key` (16 hex
  chars, Sone's shape) generated once at first run and sent identically on
  PKCE authorize, exchange and refresh; a fresh value per login would register
  a phantom device on the account.
- **PKCE redirect**: the ecosystem PKCE client id accepts only TIDAL's own
  `https://tidal.com/android/login/auth`, so what streamboat controls is the
  capture mechanism, not the redirect URI. Built: paste (default, works
  headless) and a loopback listener (`--capture loopback`, only with a client
  id whose registration allows it, unverified for the ecosystem id). A
  `streamboat://` handler is the desktop shell's mechanism; `tidal://` is
  parsed for content links but never claimed as an OS handler. The PKCE
  exchange sends `scope=r_usr+w_usr+w_sub` with literal `+` (form-encoded as
  `%2B`), as python-tidal and Sone do; the device flow sends spaces.
- **Master key resolution** (in order, no silent promotion): `STREAMBOAT_MASTER_KEY`;
  an existing keyring entry (`io.github.waayway.streamboat` / `master-key`);
  an existing key file; else generate, into the keyring when reachable and
  `key_storage` is not `file`, otherwise into the 0600 key file. Keyring calls
  run on their own thread so a backend that spins up an async runtime never
  nests inside ours. `key_storage = keyring` fails loudly when no keyring is
  reachable. Flatpak's Secret portal is not wired yet (no Flatpak build yet).
- **Cross-process refresh**: refresh runs under an advisory lock on
  `tokens.lock` and re-reads the store first; a process that finds newer
  tokens adopts them instead of refreshing again (TIDAL rotates refresh
  tokens, so a second refresh would log both processes out).
- **Protocol**: `proto::PROTOCOL_VERSION = 1`; JSON with `"type"` tags in
  snake_case; additive changes only.

## The libmpv backend (D-016)

`streamboat-player::mpv::MpvEngine`, behind the `mpv` cargo feature
(`libmpv2` 6, optional workspace dependency; `libmpv-dev`/`libmpv2` on the
system, no bundling yet). Windows and macOS in production; runs and is
tested on Linux too, since that is where CI exercises it (D-016 puts
GStreamer, not libmpv, on Linux in production).

- **Two mpv handles, like `gst.rs`'s bus thread**: a root `Mpv` (commands,
  properties) owned by `MpvEngine`, and a second client handle from
  `Mpv::create_client` moved into a dedicated event-pump thread that only
  ever calls `wait_event` against a stop flag — mirrors `gst.rs`'s bus
  thread exactly, just against libmpv's client API instead of a GStreamer
  bus.
- **Gapless**: `loadfile <uri> replace` starts a track; `set_next` appends
  the successor with `loadfile <uri> append` so mpv performs the hand-over
  itself (`--gapless-audio=weak`, D-018 — never `yes`). mpv has no
  `about-to-finish`-style early signal for a pre-queued entry, so
  `AboutToFinish` and `Finished` are both emitted off the same `EndFile`
  event, back to back, rather than genuinely early. Correlating mpv's
  `StartFile`/`PlaybackRestart`/`EndFile` events (which carry no id, only a
  reason) back to a `LoadItem` needs a `current_started` flag, not just
  "is a successor queued": `StartFile` fires once per playlist position,
  including the very first one for whatever `load()` just set as current,
  which otherwise races a `set_next` call landing on the successor slot
  before that first `StartFile` is even processed on the event thread —
  resolved by that flag, see its doc comment in `mpv.rs` for the detail.
- **Exclusive output (D-017)**: `audio-exclusive=yes` plus the platform
  AO — `wasapi` (Windows), mpv's dedicated `coreaudio_exclusive` AO
  (macOS — hog mode via direct device access, not `coreaudio` +
  `--audio-exclusive=yes`, which only redirects to it for compressed
  formats), `alsa` with a `hw:`/`plughw:` device string (Linux, where
  `--audio-exclusive` is a documented no-op on that AO — set anyway for
  consistency). Windows/macOS option values are cfg-gated and untested here
  — no Windows or macOS CI runner yet.
- **DASH feeding**: always a per-session temp file under the runtime dir,
  never a `data:` URI — unlike GStreamer, mpv/FFmpeg cannot re-fetch a
  `data:` URI mid-playback. The manifest's own `https://` segment URLs need
  `protocol_whitelist` widened on `demuxer-lavf-o`/`stream-lavf-o`, using
  mpv's `%N%` literal-length escape (a plain comma-separated value is
  misparsed as multiple invalid key/value entries by mpv's own option
  parser, since commas are that parser's own entry separator).
- **ReplayGain/volume**: TIDAL's stream carries no ReplayGain tags, so
  mpv's own `replaygain`/`replaygain-preamp` properties (which read tags
  out of the decoded file) cannot be fed TIDAL's per-track values. Computed
  with the same formula as `gst.rs` and folded into mpv's `volume` property
  together with the user volume, rather than an `af=volume`/`lavfi` filter
  chain of unverified availability. Applied once per `load()`, not
  re-applied at a gapless hand-over — the same boundary glitch `gst.rs`
  documents for its own shared `volume` element. Fixed at 100 and bypassed
  in exclusive mode, matching `gst.rs`.
- **Signal path**: `audio-params`/`audio-out-params` are `MPV_FORMAT_NODE`
  properties this crate's `PropertyData` cannot decode from a
  property-change event (it panics on `Node`) — read on demand instead via
  `get_property::<String>`, which returns mpv's own JSON rendering, parsed
  with `serde_json`. `decoder` comes from the `audio-codec` property (human
  text, e.g. "PCM signed 16-bit little-endian") — an extra `gst.rs` doesn't
  have an equivalent requirement for, but the property is free.
- **`msg-level` from `RUST_LOG`**: a deliberately simple translation (first
  directive's bare level only, not a full `EnvFilter` parser). mpv's own
  log messages are not forwarded as `EngineEvent::Warning` — `Event::LogMessage`
  needs `mpv_request_log_messages`, which `libmpv2` 6 does not wrap.

## Engine selection, device enumeration and media controls (D-016, D-030)

`streamboat-player::platform` and `streamboat-player::media_controls` are
what `streamboatd` and the desktop shell (`ui::engine_select`,
`ui::app::run_as_local_instance`) call instead of naming `GstEngine`/`MpvEngine` or an
individual OS adapter directly, so engine/adapter selection lives in one
place:

- **`platform::default_engine(events, output, runtime_dir) ->
  EngineResult<Box<dyn Engine>>`**: four non-overlapping `cfg` predicates
  over the `gstreamer`/`mpv` features (not `target_os` alone, since
  `cargo test -p streamboat-player --features mpv` builds both features at
  once on Linux) — Linux-with-`gstreamer` always wins there, matching
  D-016, even though `mpv` is also compiled in that job; `MpvEngine` stays
  reachable in that build only by constructing it directly, exactly as
  `mpv.rs`'s own tests already do. A build with neither feature compiled is
  a `compile_error!`, not a runtime `EngineError` — Cargo has no per-target
  default-feature selection, so `Cargo.toml`'s header comment documents the
  exact `--no-default-features --features mpv,smtc`/`mpv,nowplaying`
  invocation Windows/macOS builds need.
- **`platform::enumerate_output_devices() -> Vec<OutputDevice>`**
  (`OutputDevice { id, name, exclusive_capable }`): GStreamer's
  `DeviceMonitor` on Linux — the single home of that logic, which the
  `streamboat devices` CLI subcommand calls — reading `api.alsa.path` or
  `alsa.card`+`alsa.device` into an `hw:C,D` id
  (`os-integration.md` §1/§3's table); mpv's `audio-device-list` property
  on Windows/macOS, via a short-lived, otherwise-idle `Mpv` instance that
  opens no device, filtered to entries the running platform's own AO driver
  produces (`wasapi/`, `coreaudio`) plus the generic `auto` entry.
  `exclusive_capable` is `false` only for that generic entry, since
  exclusive mode (D-017) needs a concrete device. Absent either backend
  feature, this returns an empty list rather than failing to build:
  device enumeration is a nicety a missing backend degrades, unlike engine
  construction itself.
- **`media_controls::spawn(handle)`**: `cfg`-picks `mpris::spawn` (Linux),
  `smtc::spawn` (Windows, new), or `nowplaying::spawn` (macOS, new) — one
  adapter per OS, so no "prefer the platform pick over what else is
  compiled" logic is needed here the way `default_engine` needs it. Logs
  and returns on any other combination (an adapter feature deliberately
  dropped, or an unlisted OS).

**`smtc.rs`** (Windows, feature `smtc`, default on; `windows` 0.62.2 pinned,
features `Media`, `Media_Playback`, `Foundation`, `Storage_Streams`,
`Win32_System_WinRT`): `Windows::Media::Playback::MediaPlayer`'s own
`SystemMediaTransportControls` property, not
`SystemMediaTransportControlsInterop::GetForWindow` — the latter is the path
`os-integration.md` §5 documents as needing a real `HWND` ("a
`streamboat-server` Windows service or CLI daemon with no window gets no
SMTC integration at all"); a bare `MediaPlayer` instance creates its own
implicit message-only window, so this works from a plain background thread
with no window of streamboat's own. `RoInitialize(RO_INIT_MULTITHREADED)`
once per thread (every WinRT call needs an apartment first — the
`Win32_System_WinRT` feature, beyond the four core `windows` features above, is for exactly
this), `ButtonPressed` mapped to Play/Pause/Stop/Next/Previous `Command`s,
`DisplayUpdater` (`MusicProperties` title/artist/album, a thumbnail from the
album-cover URL via `RandomAccessStreamReference::CreateFromUri`),
`PlaybackStatus`, and `SystemMediaTransportControlsTimelineProperties` from
`Event::Position` (`TimeSpan::Duration` is 100 ns ticks). Volume: SMTC has
no volume surface of its own, so D-017's rule stays entirely at the
`Player`/engine layer — nothing here needs to force anything back.

**`nowplaying.rs`** (macOS, feature `nowplaying`, default on;
`objc2-media-player` 0.3.2 + `objc2-foundation` 0.3.2 + `objc2` 0.6.3 +
`block2` 0.6.2, all pinned and verified on crates.io, default features on
every one — `objc2-media-player`'s own default set already includes
`MPNowPlayingInfoCenter`/`MPRemoteCommand(Center, Event)`/`block2`):
`MPNowPlayingInfoCenter.nowPlayingInfo`/`.playbackState` rebuilt in full on
every update (title, artist, album, duration, elapsed, a fixed playback
rate of `1.0` — mirroring `mpris.rs`'s own "always resend the whole
`Metadata`" style) and `MPRemoteCommandCenter`'s play/pause/toggle/next/
previous/`changePlaybackPosition` commands, each `addTargetWithHandler`'d
with a `block2::RcBlock` that sends a `Command` through the plain `Send`
handle. **Still open**: `os-integration.md` §1 notes
`souvlaki`'s own README says macOS now-playing integration "requires an
AppDelegate/winit event loop" — a bare `streamboatd` has none. This is
written correctly against the documented Objective-C contract and registers
unconditionally regardless (same "log and continue" rule every adapter
here follows), but whether `MPRemoteCommandCenter`'s command *handlers*
actually fire with no run loop at all is unverified; the desktop shell,
once it has one, is where this is most likely to work fully.

**Untested beyond compiling — see "Cross-target type-checking" below**:
neither `smtc.rs` nor `nowplaying.rs` has ever run against a real SMTC
popup or Control Center; both are written from the `windows`/`objc2*`
crates' own published, generated bindings (checked line-by-line against
each crate's actual source, not from memory) rather than
from a live build, the same standing `mpv.rs`'s own Windows/macOS AO option
values already carry.

## Catalogue and library API surface (D-001, D-015, D-028, D-039)

`streamboat-core::api` is now a module directory: `mod.rs` keeps the spike's
sessions/track/search/`playbackinfopostpaywall`/cascade code unchanged, and
adds:

- `catalog` — album, artist, playlist, mix and video entity pages, plus
  track credits. No album-credits or video-playback method exists, by
  policy: neither has a dedicated unofficial-API endpoint (video playback is
  later scope; see the module's doc comments for the exact citation).
- `pages` — the v2 `home/feed` sections renderer input (`home_feed`,
  `home_tabs`, `expand_section`) and the v1 `pages/*` shape as an optional
  fallback (`page_v1`, `explore`). Both `FeedSection` and `PageModuleV1`
  (`models.rs`) keep the section's raw JSON alongside whatever typed fields
  this crate recognises, so an unrecognised `type` never fails the page.
- `search` — `search()` against the documented v1 response shape (the v2
  endpoint's request parameters are documented, its response shape is not).
- `library` — favourites (list/add/remove, all five collections plus
  mixes), `favorite_ids`, user playlists and
  `playlistsAndFavoritePlaylists` (with the `{playlist, created}` unwrap),
  collection folders, and `follow_artist`/`unfollow_artist` (D-039 — a
  documented alias over favouriting an artist; no distinct follow endpoint
  exists on the unofficial API).
- `playlists` — create/rename/describe/add/remove/reorder/delete and folder
  mutations, all through the ETag/`If-None-Match` write-precondition flow
  (`playlist_etag` fetches it; every mutation accepts an already-held etag
  or fetches its own).
- `lyrics` — `lyrics()` (404 → `Ok(None)`) plus `parse_synced_lyrics`, an
  LRC parser tolerant of both `.` and `:` fractional separators.
- `images` — pure `resources.tidal.com` URL builders per entity's documented
  valid sizes, and `parse_content_link` for `tidal://`/`listen.tidal.com`/
  `tidal.com` links (parses only; streamboat's own scheme is
  `streamboat://`, never registered here).
- `pagination` — `collect_all` (offset/limit) and `collect_all_cursor`
  (cursor-based), both capped.

Every response model in `models.rs` added for this surface stays tolerant of
missing/null/unknown fields, matching the spike's existing `Track`/`Page`
style; `crates/streamboat-core/tests/catalog.rs` covers pure parsing
(including a feed section with an unknown `type`, null fields, and a
missing/empty response) and the transport-level ETag flow.

## Streaming privileges, play reporting and scrobbling (D-027, D-033, D-037)

Three new `streamboat-core` modules, wired into `streamboat-player::Player`
through a `PlayerDeps` struct whose fields all default to `None` — a `Player`
with no dependencies configured behaves exactly as it did without them,
which is what every pre-existing player test relies on.

- **`privileges`** — `StreamingPrivileges`: `POST {api_base}v1/rt/connect`
  for a websocket URL (`tokio-tungstenite`, rustls/webpki-roots), then
  `USER_ACTION {startedAt}` on `claim()`, `PRIVILEGED_SESSION_NOTIFICATION`
  surfaced as `PrivilegesEvent::Revoked{client_display_name}`, `RECONNECT`
  handled by reconnecting, and capped exponential backoff with full jitter
  (`backoff_delay`, ceiling `MAX_DELAY = 60s`) on every other disconnect —
  never the reference browser SDK's unconditional immediate retry.
  `notify_token_refreshed()` forces a reconnect on the fresh token. `Player`
  calls `claim()` only from `Play`/`Next`/`Previous`/`Resume` command
  handlers, never from gapless hand-over or the buffering-pause/resume path,
  and reacts to a revoke by pausing and emitting the new
  `Event::PlaybackTakenOver { by }` (added additively to `proto.rs`)
  alongside a `Warning`.
- **`reporting`** — `PlayReporter`: builds the `ec.tidal.com/api/event-batch`
  SQS-`SendMessageBatch`-shaped form POST (up to 10 events/batch) carrying
  the documented `playback_session` JSON body and its nine-key `Headers`
  attribute, gated by the 30-second-played threshold and PREVIEW
  suppression, timestamped from `ServerClock` (`GET /v1/ping`'s `Date`
  header, cached and refreshed hourly), persisted to a JSON queue file
  under the data dir (atomic write) that survives a restart, retried on a
  network error or 5xx, and dropped permanently — never retried — on a
  `BatchResultErrorEntry` or another 4xx. The event's `client`/header
  identity follows the credential actually in use
  (`ApiClient::credentials().source`): the maintainer-embedded default pair
  (`CredentialSource::BuildTime`) gets Sone's pinned Android identity,
  because for that specific credential it is true; a user-supplied pair
  gets streamboat's own honest identity instead (`platform_name()`,
  `crate::VERSION`) — D-027's "the payload must follow the credential in
  use." `Player` calls `record()` from `EngineEvent::Finished` (still
  `self.index`-current at that point, before a gapless successor's
  `Started` is processed) and from every command that skips the current
  track before that (`Play`, `Next`, `Previous`, `Stop`, `ClearQueue`).
- **`scrobble`** — one `Scrobbler` trait (`now_playing`, `scrobble`),
  `LastfmScrobbler` (session-key auth, `md5(sorted params) + secret`
  signing, `track.updateNowPlaying`/`track.scrobble`) and
  `ListenBrainzScrobbler` (`Authorization: Token <user_token>`,
  `submit-listens` with `playing_now`/`single`), each with its own
  persistent, retrying queue for `scrobble()` (`now_playing` is an unqueued
  best-effort ping — by the time a retry would land it is stale anyway).
  `ScrobbleHub` fans out to every backend that is both `enabled` and has a
  full credential set, and is itself what `Player` holds as
  `Option<Arc<dyn Scrobbler>>`.
- **Settings**: `play_reporting: bool` (default `true`, D-027) and
  `scrobble: ScrobbleSettings` (`lastfm`/`listenbrainz`, each `enabled: bool`
  default `false` plus its credentials) on `config::Settings`.

`streamboatd` (`streamboat-server`), the `streamboat` CLI's `play` command, and the desktop shell
each construct all three (plus the offline cache, D-022) from `Context` through the shared
`PlayerDeps::for_context` helper and pass them to `Player::spawn` — the headless daemon is where
Pushkin matters from day one (no user watching a silent revocation), but every front end gets the
same behaviour.

Left uncertain by the reference material, not invented: the event-batch
endpoint's *response* shape (only its AWS-SQS-style *request* shape is
documented; `reporting::parse_batch_response` parses the standard
`SendMessageBatchResultEntry`/`BatchResultErrorEntry` XML shape that request
format implies, and trusts an HTTP 2xx when the body doesn't parse as that);
`os-version` in the reporting headers (left empty rather than guessed); and
whether `x-tidal-streamingsessionid` must actually equal the reported
`playbackSessionId` for a play to surface in Recently Played (Sone's plays
surface without ever sending that header at all — `reporting::PlayEvent`
reuses one id for both anyway, the cheapest safe move the reference names).

## Desktop shell (iced) (D-010, D-012, D-013, D-014, D-036)

`crates/streamboat-desktop/src/ui/` (loaded by `main.rs`'s `mod ui;`; `streamboat` with no
subcommand calls `ui::run()`, every existing CLI subcommand is unchanged). Pinned to
`iced = "=0.14.0"` exactly, features `tokio`, `image`, `svg`, `advanced`, `debug` — see the
`iced-ui` skill for the pinned-API facts the shell (including its multi-window code) is verified
against the actual 0.14 sources rather than memory (the API changed hard across 0.9-0.14, per D-013).

- **Architecture**: `ui::app::App` is the top-level state, now built with `iced::daemon(...)`
  instead of `iced::application(...)` (D-036) so it can own more than one window — see "Multi-window
  and the mini-player" below. `ui::app::Message` wraps each screen's own message enum (D-013's
  "split messages per screen/module") plus `PlayerEvent(Box<Event>)` (boxed: `Event::State` carries
  a full snapshot and dwarfs every other variant), `Nav`, `LoginCheck`, `ImageFetched`,
  `KeyShortcut`, `MiniPlayer`, `Tray`, `WindowCloseRequested`, `WindowClosed`, `ShowRequested` and
  `ReadyToExit`. `ui::nav::{Screen, Nav}` is the navigation stack (`go_to`/`back`/`forward`, back
  pushes history and clears forward). `ui::design::Tokens` is the
  design-token struct (background/surface/elevated/accent/text/muted/warning/danger/success/border
  colours, a radius/spacing/type scale) with `dark()` (default) and `light()` constructors;
  `Tokens::iced_theme` builds the `iced::Theme::custom` palette stock widgets style against, while
  custom containers/cards/badges read the tokens directly (captured `Copy` into style closures) —
  see the `iced-ui` skill §"Owner-context notes" for why the token struct, not `iced::Theme`, is the
  source of truth.
- **The engine seam (D-010)**: `ui::player_link::PlayerLink` is a trait (`send(Command) -> bool`,
  `events() -> BoxStream<Event>`); `InProcessLink` wraps a `PlayerHandle` from `Player::spawn` — the
  *only* two `streamboat-player` APIs the shell touches, never `Engine`/a backend type directly.
  `ui::remote_link::RemoteLink` is the other implementation, now built (not a stub): a control-API
  client over D-030's HTTP + WebSocket surface — `GET /v1/state` is not polled separately since the
  first WebSocket message on any connection is always a full snapshot; `send` fires a `POST
  /v1/commands` on an explicitly-held `tokio::runtime::Handle` (iced's own update loop is not
  async); `events` is a `futures::stream::unfold` state machine (`Disconnected`/`Connected`) that
  reconnects with capped exponential backoff (250ms, doubling, capped at 30s) on a dropped
  connection *and* on detecting its own revision gap (any jump greater than one — treated as a
  proactive resync, reconnecting immediately with no backoff since it is not a failure), always
  landing on a fresh snapshot either way per the reconnect rule. `Player::spawn` (when this process
  runs one) runs on a second, explicitly-built `tokio::runtime::Runtime` kept alive for the process's
  lifetime (a local variable threaded through `ui::app::run`'s helper functions that outlives the
  blocking `.run()` call) — separate from iced's own internal tokio runtime (its `tokio` feature),
  which drives `Task`/`Subscription` futures instead; the same runtime backs `RemoteLink`'s
  `send` when there is no local `Player`.
- **Single-instance lock and remote-client mode (D-010, D-045)**: `ui::instance::decide` is the one
  call `ui::app::run` makes before anything else — `streamboat_core::instance_lock::InstanceLock`
  (a portable `fd-lock` file at `AppDirs::instance_lock_path()`, `<runtime dir>/instance.lock`) is
  tried first; holding it means this process becomes the instance and spawns its own engine
  (`ui::app::run_as_local_instance`). Failing to take it checks
  `AppDirs::control_address_path()` (`<runtime dir>/control-address`, written by whichever holder
  also hosts the control API — always `streamboatd`, per D-031's "only the daemon binds a
  listener," never a GUI): present means become a `RemoteLink` client
  (`run_as_remote_client`); absent means the holder is another GUI instance, so this process
  touches `AppDirs::show_request_path()` (`<runtime dir>/show-request`) and exits without opening
  a window (`Decision::FocusedOther`). The holding GUI polls that file's mtime every 400ms
  (`ui::instance::show_request_events`, an `iced::Subscription::run_with` built only while
  `Decision::Local`) and answers a touch the same way the tray's "Show" does — unhide and focus the
  main window. Deliberately not built: the MPRIS-bus-name variant of D-010's lock (the decision
  names two mechanisms; the code always takes the portable-lock-file branch, which D-010 already
  permits on its own) and a `POST /v1/show` control-API route (rejected: it would
  require the GUI to bind a listener, contradicting D-031 — the filesystem touch is the
  `Command`-free alternative D-031 leaves room for).
- **Multi-window and the mini-player (D-036)**: `App::boot` opens the main window itself via
  `window::open`, storing the `window::Id` it returns synchronously; `App::view`/`App::title`
  dispatch on the `window::Id` iced asks for (`Some(window) == self.mini_window` renders
  `ui::mini_player::view` — art, title/artists, previous/play-pause/next, a thin seek bar, the
  quality badge and a restore button — everything else renders the main shell). The playback bar's
  "Mini player" toggle and Ctrl+M open/close it via `window::open`/`window::close`; both windows
  read the same `App::player_state`/`App::current_art()`, there is no separate copy of playback
  state for the mini window. The mini window is `window::Level::AlwaysOnTop`, fixed-size, not
  resizable.
- **Window lifecycle and tray (D-014)**: the main window's `window::Settings::exit_on_close_request`
  is `false`, so its native close button only delivers `window::Event::CloseRequested` (via
  `window::close_requests()`) instead of iced auto-closing it; `update` answers that by hiding it
  (`window::set_mode(id, window::Mode::Hidden)`) rather than closing it, keeping the lock, the
  engine and the audio device held exactly as before. `iced::daemon` never exits when its last
  window closes on its own (verified against `iced_winit`'s `is_daemon`-gated exit check), so this
  costs nothing extra. Quitting is explicit: the tray's "Quit" item or Ctrl+Q
  (`App::quit`) sends `Command::Shutdown` and waits up to 800ms for `Event::Stopped`/`EndOfQueue`
  (`ui::app::wait_for_shutdown`, over a fresh `PlayerLink::events()` subscription) before returning
  `iced::exit()` — never hangs quitting on a player that does not answer. `ui::tray` builds the tray
  icon: Linux via `ksni = "0.3"` (resolved 0.3.6, verified against its real crate source — a
  `StatusNotifierItem` over D-Bus, no GTK dependency, spawned on its own thread with a
  `current_thread` tokio runtime, the same shape `mpris.rs` already uses for its own D-Bus
  registration) with Show/Hide/Play-Pause/Next/Previous/Quit menu items and a "now playing" tooltip
  updated from `PlayerLink::events()`; Windows/macOS via `tray-icon = "0.24"` (resolved 0.24.1),
  cfg-gated behind `#[cfg(not(target_os = "linux"))]`, its tooltip updated the same way through a
  forwarding task, and **compile-checked only** (CI's `cross-check` job clippies both binaries in
  their Windows shape) — there is no
  Windows/macOS runner or display here, and `tray-icon`'s own documentation requires the icon to be
  created on the same thread as a running native event loop (a win32 message loop on Windows, the
  main thread's loop on macOS), which this implementation's dedicated background thread does not
  provide; see `ui::tray`'s doc comment for the full caveat. Both backends only ever produce
  `ui::tray::TrayEvent`s onto one channel; `ui::tray::tray_event_to_command` is the pure,
  D-Bus-free mapping from a tray click to a `Command` that `ui::app::update` calls — Show/Hide/Quit
  are handled directly against `window`/`exit` `Task`s instead, since they are not `Command`s.
- **Decoder probe (D-003)**: `streamboat_player::probe::DecoderSupport` — `low`/`high`/`lossless`/
  `hi_res_lossless` booleans, `probe()` dispatching to `probe_gstreamer()` (Linux: looks for the
  `flacdec` and `avdec_aac`/`faad` element factories) or `probe_libmpv()` (Windows/macOS: always
  "everything reachable," ffmpeg is bundled with mpv, D-016) by the same `cfg` `ui::engine_select`
  uses. `ui::app::run_as_local_instance` calls it once at startup, `DecoderSupport::cap(ceiling)`
  walks `AudioQuality::LADDER` for the highest still-reachable tier at or below the requested
  ceiling, and a mismatch publishes an `Event::Warning` explaining the cap through the same
  `PlayerHandle` every front end already reads. The result is stored on `App` (`decoder_support`)
  and handed to the Settings screen (`settings::State::with_decoder_support`), whose quality
  picker leaves unreachable tiers out and names them underneath with the reason — iced's
  `pick_list` has no per-item disabled state, so "greyed out" is "not offered, explained."
- **OS media controls from the shell**: `ui::app::run_as_local_instance` calls
  `streamboat_player::media_controls::spawn(handle.clone())`, the same call `streamboatd`'s
  `main.rs` makes — MPRIS on Linux, SMTC on Windows, NowPlaying on macOS.
- **Screens**: Login (`ui::screens::login` — device-code and PKCE-paste/PKCE-loopback,
  reusing `auth::device_code`/`auth::pkce` exactly as the CLI does; errors inline), Home
  (`ui::screens::home` — v2 `home/feed` sections, the tab bar from `header.vibes.items`, cursor
  paging, per-section "View all" via `expand_section`), Explore (`ui::screens::explore` — the v1
  `pages/explore` shape, same graceful-unknown-module rendering), Search (`ui::screens::search` —
  all types, a type filter, 300ms-debounced input), Now Playing (`ui::screens::now_playing` — large
  art, seek, quality badge, the queue list with move-up/move-down/remove buttons over
  `Command::MoveQueueItem`/`RemoveQueueItem`, a Lyrics button routing to the Lyrics screen), the
  persistent playback bar (`ui::playback_bar` — art/title/artist/transport/seek/volume/badges/queue,
  signal-path and mini-player toggles; the volume slider dims and grows a tooltip while `OutputConfig::is_exclusive()`,
  per D-017, rather than becoming inert — it still sends `SetVolume`, which the Player already
  refuses with a `Warning` event in exclusive mode), the signal-path panel (`ui::signal_path` —
  renders `PlayerState::signal_path` field-for-field, `None` stays "Unknown" rather than guessing,
  and prints "lossy source, bit-perfect not applicable" for AAC/lossy tiers per D-036), and Settings
  (`ui::screens::settings` — quality ceiling, output device + exclusive toggle, ReplayGain mode,
  play-reporting toggle with the D-027 disclosure text, credentials, key storage, theme, logout).
  The mini-player window and the tray are covered separately above (see "Multi-window and the
  mini-player" and "Window lifecycle and tray"); no second `iced` window is opened by anything
  below.
- **Entity, Collection and Lyrics screens (D-015)** replace the old
  `ui::screens::placeholder` routes (now deleted): Album (`ui::screens::album` — cover, title,
  artist links, year/track-count/duration, quality/explicit badges, a track list through the
  shared action row below, play-all/shuffle, a favourite toggle backed by `favorite_ids()`, a
  similar-albums row, and the review text when `album_review` returns one), Artist
  (`ui::screens::artist` — picture, follow toggle (D-039 — an alias over favouriting the artist),
  top tracks, an Albums/EPs & Singles/Compilations tab bar over the three `artist_albums` filter
  values loaded up front rather than per-tab, similar artists, bio with its source attribution, and
  a "Play artist mix" button that only appears once `artists/{id}/mix` resolves an id), Playlist
  (`ui::screens::playlist` — header, items paged 50 at a time with a "Load more" button
  (`api/pagination`'s page-size constant, not its `collect_all` helper — this screen wants explicit
  load-more, not eager collection), play-all, a favourite toggle, and — gated on
  `playlist.creator.id == user_id()`, fetched once per visit — rename/describe, remove item and
  move item up/down (both index-based through `api/playlists.rs`'s ETag-precondition helpers,
  passing `None` so each call fetches its own fresh etag rather than risking a stale cached one),
  and delete behind a two-step confirmation banner), Mix (`ui::screens::mix` — `mix_page` for the
  title (`ApiClient::mix_page`: `pages/mix?mixId=`, `tidal-client-features`
  browse-pages-screens.md §4's "note the required query param") and `mix_items` for the track list,
  items list, play-all), Track (`ui::screens::track` — fetches the track to learn its album id,
  then reuses `album::content_with_highlight` (the album view split out of its page/scrollable
  wrap so this screen can embed it) with that track's row picked out, plus a credits panel from
  `track_credits` grouped by role), and Video (`ui::screens::video` — metadata only, artist links,
  explicit/quality badges, and a fixed "Video playback is not supported yet." banner, never a play
  button, per D-038). My Collection (`ui::screens::collection`) is a persistent top-level `App` field
  like Home/Explore/Search, not a per-visit `Entity`-style screen: five tabs (Tracks/Albums/
  Artists/Playlists/Mixes & Radio), each loading lazily on first visit; Tracks/Albums/Artists/Mixes
  page 50 at a time with their documented `api/library.rs` sort orders (`ItemOrder`/`AlbumOrder`/
  `ArtistOrder`/`MixOrder`, each given a `Display` impl purely for the `pick_list` label —
  the wire value stays `as_str()`); Playlists uses `playlists_and_favorite_playlists` (client-side
  name/recency sort, since that endpoint takes no `order` param) plus a "Folders" section from
  `collection_folders_all` rendered as best-effort `name`/`title` labels, since no reference this
  crate cites enumerates that endpoint's item shape beyond `trn`; a "+ New playlist" dialog calls
  `create_playlist`. Lyrics (`ui::screens::lyrics`) is fetched by `ApiClient::lyrics` and rendered
  synced (LRC lines via `parse_synced_lyrics`, the current line picked out by a pure
  `highlighted_index(lines, position_ms)` helper and highlighted, with `iced::widget::operation::scroll_to`
  auto-scrolling to it — see the `iced-ui` skill for the exact API), plain-text (falls back when
  `subtitles` is absent or empty but `lyrics` isn't), or a "No lyrics for this track." note; a
  synced line is click-to-seek. `App` prefetches lyrics from `Event::TrackStarted` (so the Lyrics
  screen, reached from Now Playing's existing button, opens instantly) and re-requests on
  navigating there directly; `Event::Position` is also forwarded into the lyrics screen's own
  `update` on every tick to keep the highlighted line and scroll position correct even while some
  other screen is showing.
- **The shared per-track action row**: `ui::actions::track_action_row` — Play/Next/
  Queue/♥·♡/"+ List" buttons, plus Album/Artist buttons the caller can omit (the Album screen's own
  tracks omit "Album"; the Artist screen's top tracks omit "Artist") — used by every list above
  except Search's own separate inline buttons. Mix uses it too, but — unlike Album/Artist/Playlist,
  which each also carry their own container-level favourite toggle — needs no separate favourite
  state of its own, since the row's own per-track favourite is all a Mix needs.
  `TrackAction::PlayNext`/`AddLast` both resolve to the
  *existing* `Command::Enqueue{position: Next|Last}` rather than a dedicated `Command::PlayNext`,
  since `Enqueue{Next}` already inserts right after the playing
  index in `Player::handle_command`; a second command would just be a second name for the same
  behaviour. "Add to playlist" opens `ui::actions::PickerState`, a small modal `App` renders as a
  `stack!` overlay over whichever screen opened it: one `playlists_and_favorite_playlists`
  fetch, click a playlist, `playlist_add_tracks` with `None` for the etag. Every entity/Collection
  screen converts its own `Effect` enum into one shared `app::EntityEffect` via `From`, so `App` has
  exactly one `apply_entity_effect` instead of six near-identical copies of "send `Command::Play`,
  navigate, prefetch artwork, or open the picker."
- **The notification banner**: `ui::banner` replaces the previously-silent
  `Event::Warning`/`Event::Error`/`Event::PlaybackTakenOver` arms in `App::handle_player_event` —
  each pushes a dismissible entry that auto-expires after 8 seconds
  (`Task::perform(async { tokio::time::sleep(...).await }, ...)`, deliberately lazy — calling
  `tokio::time::sleep` eagerly, outside the async block, panics with no reactor when nothing has
  polled it yet, which is exactly the shape a plain `#[test]` exercises). A takeover banner
  ("Playback started on `<by>`.") carries a Resume button that sends `Command::Resume` — genuine
  user intent, never automatic, per D-033.
- **Deep links**: pasting a `tidal.com`/`listen.tidal.com`/`tidal://`/`streamboat://`
  link into the Search field (`api::images::parse_content_link`) opens the linked screen instead of
  running a search for it (`screens::search::Effect::OpenDeepLink`); `Screen::from_content_link`
  (added to `ui::nav`, which already owned every `Screen`/`EntityRef` variant) does the mapping,
  falling back to `Screen::Collection` for a folder link (no dedicated per-folder screen exists). A
  shared playlist link additionally fetches every track id (`playlist_items_all`) and starts
  playback (D-039). The same path is reachable from the command line: `streamboat open <url>`, or a
  bare link as `streamboat`'s only argument (rewritten to `open <url>` by `main` before `clap` ever
  parses it) — the only change deep-link handling makes to `main.rs`/`ui::app::run` beyond the
  screen-navigation logic itself, kept deliberately narrow to that one subcommand.
- **Image cache**: `ui::images::ImageCache`, a hand-rolled insertion-order-bounded map (not a true
  read-touches-recency LRU — `peek`, the only read `view` code calls, deliberately never reorders,
  since `view` only ever holds `&ImageCache`; eviction order is "oldest inserted," which is
  sufficient at this cache's actual access pattern). `ui::images::fetch` runs a plain
  (unauthenticated — TIDAL cover art is unauthenticated, `api/images.rs`) `reqwest::Client` GET
  inside `Task::perform`, decoding via the `image` crate on a `tokio::task::spawn_blocking` thread
  (CPU-bound decode off both the UI thread and the async executor's worker), producing an
  `iced::widget::image::Handle::from_rgba` the update thread only ever moves, never decodes.
- **Keyboard**: `iced::event::listen_with` matched against
  `keyboard::Event::KeyPressed` — space toggles play/pause, escape goes back one step in the nav
  stack, ctrl+f navigates to Search (it does not additionally force text-input focus — no
  `Task`-returning focus helper was found on `iced_widget::text_input` in 0.14.2's public API; see
  the `iced-ui` skill §9), ctrl+m toggles the mini-player window, ctrl+q quits (D-014, `App::quit`).
  Media keys are explicitly out of scope here (MPRIS, above).
- **Additive core/player support for the single-instance lock and decoder probe**: `streamboat_core::instance_lock` (new
  module: `InstanceLock`, `write_control_address`/`read_control_address`, `request_show`/
  `show_request_mtime`); three new `AppDirs` path methods (`instance_lock_path`,
  `control_address_path`, `show_request_path`); `streamboat_player::probe` (new module,
  `DecoderSupport`). All new items, no changed behaviour on anything that existed before.
- **Additive core/player support for Settings, theming and queue management**: `Context:
  Clone`; `config::{ThemePreference, ReplayGainMode}` plus two new `Settings` fields (`theme`,
  `replay_gain_mode`) and one (`play_reporting_enabled`, default `true` per D-027) — all three
  persisted by the Settings screen; `KeyStorage: Display`; `PkceSession: Debug` (hand-written,
  redacts the verifier — needed because enabling iced's `debug` feature makes `Message: Debug` a
  hard `Application::run`/`Daemon::run` requirement, transitively through every nested screen message);
  `proto::Command::{MoveQueueItem, RemoveQueueItem}` and their `Player::handle_command` arms
  (index-based queue reorder/removal, refusing to remove the currently-playing entry with a
  `Warning` event instead of the ordinary index bookkeeping). None of this changes any existing
  variant's behaviour — every change is a new field, a new trait impl, or a new enum variant with a
  new match arm.
- **Additive core support for the Mix screen and library sort-order labels**: `ApiClient::mix_page`
  (`api/pages.rs`, `pages/mix?mixId=`, for the Mix screen's title/subtitle — track listing still
  comes from the already-existing `mix_items`); `Display` impls on `api::library::{ItemOrder,
  AlbumOrder, ArtistOrder, MixOrder}` (a human `pick_list` label distinct from each enum's existing
  `as_str()` wire value, which is unchanged). Nothing here changes any existing method's behaviour
  or any enum's wire representation.

## Offline cache (D-022)

`streamboat-player::offline` (GPL-3.0-only; the file-format and crypto
helpers have no player dependency but stay in this crate rather than
`streamboat-core`, since D-022 scopes the offline cache to `streamboat-player`
and nothing in them needs to be Apache-licensed on its own). Guardrails and how each is
enforced:

- **Explicit, user-initiated pins only.** The only entry points are
  `OfflineCache::pin`/`unpin`, reached from `Command::Pin`/`Unpin` (additive
  to `proto.rs`, alongside `Command::ListPins` and
  `Event::{PinProgress,PinReady,PinFailed,PinsChanged}`) or the CLI's direct
  calls (`streamboat pin/unpin/pins`) — nothing calls them on its own.
  Artwork/metadata caching is out of scope here entirely.
- **A transparent cache of the ordinary stream.** `OfflineCache::pin`
  resolves through `ApiClient::resolve_stream` exactly as playback does
  (`playbackmode=STREAM`, `assetpresentation=FULL`); it never constructs a
  `playbackmode=OFFLINE`/`usage=DOWNLOAD` request. A `PREVIEW` asset is
  refused (offline pinning's own check — ordinary playback still plays a
  preview, it just isn't something to *pin*); an encrypted manifest never
  reaches this module because `resolve_stream` already refused it
  internally (`manifest::parse`'s existing rule, reused unchanged).
- **On-disk format.** `<offline_dir>/index.json` (`fsutil::atomic_write`,
  now `pub` in `streamboat-core` — an additive visibility change, no
  behaviour change) records, per pin, its kind/id/title, member track ids,
  and per-track metadata: quality actually stored, manifest hash,
  container mime, byte length, `stored_at`, `validated_at`.
  `<offline_dir>/chunks/` holds one AES-256-GCM ciphertext file per 1 MiB
  (`CHUNK_SIZE`) of plaintext, named by `hex(SHA-256(salt || chunk_index))`
  — a hash of the chunk's *position*, not its content, so no file list
  needs to be stored — with no extension. For a DASH source the init
  segment and every media segment are fetched in URL order (from a
  minimal, deliberately narrow `SegmentTemplate`/`SegmentTimeline`
  `$Number$` reader — no `$Time$`/`$Bandwidth$`/printf-width support,
  refused loudly rather than mis-parsed) and concatenated into one
  plaintext buffer before chunking; a BTS/EMU source is one direct GET.
- **Key derivation.** A 32-byte "install secret" lives in the OS keyring
  under its own entry (`bootstrap::OFFLINE_KEYRING_USER = "offline-key"`,
  resolved through the same `KeySlot` mechanism as the token store, via the
  new additive `bootstrap::key_slot_named` — `key_slot` itself is
  unchanged), or a 0600 key file fallback exactly as D-024 describes,
  deliberately separate from the token master key. The AES key actually
  used per chunk is `HKDF-SHA256(ikm = install secret, info =
  client_unique_key)` — device-bound: copying `offline/` to another
  install carries neither the keyring entry/key file nor (unless
  `device.json` is copied too, which nothing here relies on) the same
  `client_unique_key`, so the derived key differs and every chunk fails to
  decrypt. `OfflineCache::serve_track` proactively decrypts chunk 0 before
  handing out a URL, specifically to catch this case rather than fail
  mid-playback.
- **No export path.** No method anywhere in this module or the CLI decrypts
  to a file, opens the cache directory, or produces a shareable copy;
  `serve_track` only ever returns a `127.0.0.1` URL.
- **Playback without a written file.** `OfflineCache::open` starts one
  `axum` server bound to `127.0.0.1:0` for the process's life, serving
  `GET /<per-process-random-token>/<track_id>` — decrypts the requested
  chunks on the fly, supports `Range`, checks `ConnectInfo`'s peer is
  `is_loopback()`, and checks the path token, refusing otherwise (403).
  `Player::resolve_track` (new, wraps the two `resolve_stream` call sites in
  `start_from`/`prefetch`) tries `OfflineCache::serve_track` first when a
  cache is configured (`PlayerDeps::offline`) and only falls back to a live
  `resolve_stream` when it returns `None` (missing, stale-and-unrevalidatable,
  or a decrypt failure) — logged, never a crash or a stuck queue.
- **Revalidation and wipes.** `serve_track` checks
  `now - validated_at >= offline_validity_days` (default 30, `Settings`);
  if stale it calls `ApiClient::session()` once and bumps every pin's
  `validated_at` on success, or returns `None` (stream instead) on failure.
  `OfflineCache::wipe_all` (chunks + index, cache stays open) runs from
  `streamboat` on `Cmd::Logout` (via the cheaper `OfflineCache::wipe_dir`,
  which needs no key resolution at all) and from `Player::resolve_track`
  when a live resolve fails with a subscription-flavoured terminal
  sub-status (`offline::is_subscription_terminal`: 4030/4031/4032/4034/4035
  — deliberately narrower than `ApiError::is_terminal_playback`'s wider set,
  which also covers purely technical causes like a rotated client id that
  say nothing about the subscription). `OfflineCache::unpin` deletes one
  pin's chunks. `OfflineCache::open` garbage-collects any chunk file whose
  index entry is gone, on every startup.
- **Settings** (additive fields on `config::Settings`): `offline_dir`
  (override), `offline_validity_days` (default 30),
  `offline_max_bytes` (default 20 GiB) — `OfflineCache::pin` refuses a new
  pin that would push the cache over it, before writing anything, and
  cleans up any chunks the same pin attempt already wrote.
- **CLI** (`streamboat pin/unpin/pins`, `streamboat-desktop/src/main.rs`):
  call `OfflineCache`'s methods directly rather than round-tripping through
  `Command`/`Event` — listing or mutating pins needs no engine, so nothing
  here starts GStreamer. `streamboat play` builds an `OfflineCache` and
  passes it through `PlayerDeps` unconditionally, so a pinned track is used
  automatically; `streamboatd` does the same. `Command::Pin`/`Unpin` do
  exist on the wire (a `Player` driven remotely — the future control API —
  gets the same behaviour, running the download on its own `tokio::spawn`
  so it never blocks the command loop) and `Command::ListPins` exists for
  that same remote case, but it only ever emits `PinsChanged`: an in-process
  front end (the CLI) reads `OfflineCache::list_pins()` directly instead of
  waiting for a reply on the protocol.

Left untested: the DASH segment
planner has no test against a real multi-representation TIDAL manifest,
only the synthetic single-`SegmentTimeline` shape `manifest.rs`'s own tests
use; wiping the cache on a subscription-terminal sub-status
(`is_subscription_terminal`) is exercised only by its own pure unit test,
not through a live `Player`/`resolve_stream` failure; two pins that share a
member track store that track twice (no cross-pin deduplication); the
loopback server's memory use scales with the requested `Range` (a whole
big-file GET decrypts the whole file into memory before responding) rather
than being a bounded streaming pipeline; `streamboatd` builds an
`OfflineCache` but has no `logout` command of its own yet to hook a wipe
into.

## ReplayGain modes (D-019)

TIDAL's own formula (`min(10^((gain+4)/20), 1/peak)`) still lives only in
the engines (`gst.rs`'s `volume` audio-filter, `mpv.rs`'s `volume`
property) — ReplayGain mode selection changes only *which numbers* feed it, in
`streamboat-player::player`:

- `StreamInfo` gained `album_replay_gain_db`/`album_peak_amplitude`
  (additive; `replay_gain_db`/`peak_amplitude` are the pre-existing
  *track*-context pair), populated in `streamboat-core::api::resolve_stream`
  from `playbackinfopostpaywall`'s `albumReplayGain`/`albumPeakAmplitude`
  (`tidal-api/references/playback.md` §8) and propagated through the
  offline cache's `TrackEntry`/`ServedTrack` (D-022) the same way the track
  pair already was.
- `player::select_replay_gain(mode, &StreamInfo)` is the one place mode
  selection happens: `Off` → neither number; `Album` → the album pair, or
  the track pair when TIDAL reported no album value at all; `Track` →
  always the track pair, even when an album value exists. `load_item`
  calls it before building every `LoadItem`, so both engines keep receiving
  exactly the two numbers they always have — nothing about their own
  formula code changed.
- `Command::SetReplayGainMode { mode }` (additive) switches live: the
  currently-playing entry keeps whatever gain its own `load()` already
  applied (both engines document ReplayGain as applied once per `load()`,
  not re-applied at a gapless hand-over — that boundary is unchanged), but a successor already sitting in `engine.set_next()` is
  recomputed from its cached `ResolvedStream` and re-handed over
  immediately, with no extra network round trip.
- `SignalPath` gained `replaygain_mode: String` (additive) — the engines
  themselves only know the two numbers they were handed, not which mode
  chose them, so `Player::state()` stamps `self.replay_gain_mode.as_str()`
  onto the engine's own `signal_path()` report before it goes out.
- Bypass in exclusive/bit-perfect output is unchanged and automatic: both
  engines already skip the gain stage entirely whenever
  `OutputConfig::is_exclusive()` (D-017, D-019), independent of the mode
  setting above.

Tested end to end against the `FakeEngine` (`crates/streamboat-player/tests/player.rs`):
album mode preferring the album pair, album mode falling back to the track
pair when TIDAL omitted the album value, track mode ignoring an album value
that *is* present, off mode applying neither number, `SetReplayGainMode`
recomputing an already-prefetched successor, and the command's JSON shape.
Not tested here: an actual audible difference on real hardware (needs a DAC
and a human listening, same caveat as the exclusive-mode ALSA writer above).

## Multiroom: Snapcast output (D-034)

An explicit output mode, `OutputConfig::Snapcast { host, port }`
(`streamboat-core::proto`, additive) — mutually exclusive with bit-perfect
by construction (a distinct enum variant, not a flag on `Exclusive`).
`streamboat_player::snapcast` holds the one fixed format both backends
target: **48000 Hz, S16LE, stereo** (`SAMPLE_RATE`/`BIT_DEPTH`/`CHANNELS`
constants, unit-tested against the doc comment's own `sampleformat` string
so the two cannot drift apart silently). The matching snapserver config
line (a `tcp://` *server*-mode stream: snapserver listens, streamboat
dials in as the TCP client — see the Snapcast plugin section below for the
metadata/control half):

```
stream = tcp://0.0.0.0:4953?name=streamboat&mode=server&sampleformat=48000:16:2
```

(`4953` is not one of Snapcast's own fixed ports — those are 1704/1705/1780/1788,
already spoken for — it is only this example's port number; any free one
works as long as it matches what `OutputConfig::Snapcast.port` connects to.)

- **GStreamer** (`gst.rs::apply_output`): the sink becomes `audioconvert !
  audioresample ! audio/x-raw,rate=48000,format=S16LE,channels=2 !
  tcpclientsink host=.. port=..`, built the same way the existing
  `sink_override`/test-sink branch is (`gst::parse::bin_from_description`).
  ReplayGain and user volume still apply (this is not exclusive mode) —
  `signal_path()` reports the resampled format as a `converted` reason and
  `device` as `snapcast tcp://host:port`.
- **libmpv** (`mpv.rs::start_snapcast_pump`, Unix only — see below):
  `--ao=pcm` with `ao-pcm-file=<fresh FIFO under runtime_dir>` and
  `ao-pcm-waveheader=no` (headerless PCM), `audio-samplerate`/
  `audio-channels`/`audio-format` forced to the fixed format ahead of the
  AO (mpv's equivalent of `gst.rs`'s explicit `audioconvert`/
  `audioresample` chain), plus a detached thread (`run_snapcast_pump`) that
  opens the FIFO for read and copies its bytes onto a TCP connection to
  `host:port` — the libmpv-side counterpart of `tcpclientsink`, since mpv
  has no TCP-client sink of its own. **Windows/macOS are not implemented**:
  `start_snapcast_pump` returns a clear `EngineError::Output` there rather
  than silently doing nothing, because `mkfifo` (shelled out to, not linked
  as a dependency) and Unix-domain blocking-open semantics are what the
  unblock-on-drop logic relies on. The pump thread is deliberately not
  joined on drop (only signalled and given a best-effort unblock) since it
  may be blocked on mpv's own writer end, which this struct does not own —
  joining there could hang a `set_output`/shutdown on mpv's own timing.
- `streamboat snapcast-plugin` (`streamboat-desktop/src/snapcast_plugin.rs`):
  Snapcast's stream-plugin protocol — newline-delimited JSON-RPC 2.0 over
  stdin/stdout — bridging to a *running* `streamboatd`'s control API
  (D-030) over plain HTTP (`POST /v1/commands`, `GET /v1/state`) and the
  `/v1/events` WebSocket, so it works as a separate process the way
  snapserver actually launches a `controlscript`. Methods implemented are
  exactly what `headless-and-tidal-connect/references/mpd-and-multiroom.md`
  §2 names explicitly (itself citing `badaix/snapcast`
  `doc/json_rpc_api/stream_plugin.md`, not independently re-verified line
  by line here): snapserver → plugin `Plugin.Stream.Player.Control`
  (`play`/`pause`/`playPause`/`stop`/`next`/`previous`/`seek`/
  `setPosition`), `Plugin.Stream.Player.SetProperty` (only `volume` maps to
  anything streamboat can do; anything else is a typed JSON-RPC error, not
  a silent no-op), `Plugin.Stream.Player.GetProperties`; plugin → snapserver
  `Plugin.Stream.Player.Properties`, `Plugin.Stream.Log`, `Plugin.Stream.Ready`.
  **Marked uncertain in the module doc comment, not asserted as fact**:
  `Plugin.Stream.Log`'s exact parameter shape and whether `Plugin.Stream.Ready`
  carries params at all — the cited reference names the method but not
  those details. JSON-RPC framing itself (request/response/notification
  shape, `id` echoing, the reserved `-327xx` error codes) is the JSON-RPC
  2.0 specification, not a Snapcast invention.
- `streamboat snapcast-discover` (`streamboat-desktop/src/snapcast_discover.rs`):
  mDNS browse for `_snapcast._tcp` (what snapserver actually advertises,
  per the cited reference) and `_snapcast-tcp._tcp` (not in that reference
  at all — searched defensively anyway, in case a build or fork advertises
  under that name; finding nothing under it is expected, not a bug). `mdns-sd` 0.21.3, the version
  current as of this writing and confirmed directly against crates.io
  rather than trusted from memory.

Tested: the fixed-format-over-TCP path end to end on GStreamer
(`crates/streamboat-player/tests/gst_engine.rs`'s
`snapcast_output_streams_the_fixed_format_pcm_over_tcp` — a real
`TcpListener` standing in for snapserver, a generated WAV through the real
`apply_output` branch, byte count checked against the WAV's known duration
at the fixed format rather than decoding the stream); the JSON-RPC message
shapes in isolation (request/response/notification encoding, `id`
handling, the standard error codes, the `Control`/`SetProperty` → `Command`
mapping, and `PlayerState` → `Plugin.Stream.Player.Properties`); the mDNS
record-to-address parsing against a synthetic `ResolvedService` built
through `mdns_sd::ServiceInfo::new(..).as_resolved_service()` (the type is
`#[non_exhaustive]`, so this is the crate's own supported way to construct
one off-network). **Not verified here — no snapserver in this
environment**: an actual round trip against real `snapserver`/Snapweb (the
stream showing up with metadata and working transport controls), the
libmpv/FIFO pump path end to end (no test requires it — the TCP-format test
is scoped to the GStreamer path only), and real-network
mDNS discovery.

## Crash dumps, logging and the debug bundle (D-029)

`streamboat_core::diagnostics` (Apache-2.0 — generic enough to serve both
GPL binaries without depending on either): both `streamboat` and
`streamboatd` call `diagnostics::init(&dirs.data, app, version,
default_filter)` once, as early in `main` as possible, replacing their
previous bare `tracing_subscriber::fmt()...init()` call. `default_filter`
keeps each binary's own previous `RUST_LOG` fallback (`"warn"` for the CLI,
`"info"` for the daemon) — this is a redaction/rotation change, not a
verbosity change. Falls back to the old stderr-only setup if `AppDirs`
cannot resolve (no home directory), rather than starting with no logging
at all.

- **Redaction, by construction.** `diagnostics::redact` (regex-based,
  tested directly with synthetic lines) masks `Authorization: Bearer …`
  and bare `Bearer …`, `refreshToken`/`accessToken` fields, `sessionId`/
  `streamingSessionId`/`x-tidal-streamingsessionid`, `userId`, the control
  API's own bearer token by name, and — as a broad backstop — any bare
  64-hex-char run (also catches `manifest_hash`, which is not secret;
  over-redacting a hash costs nothing a debug session needs). The file log
  layer routes every event through a custom `FormatEvent` wrapper
  (`RedactingFormat`) that formats into a scratch buffer first and redacts
  the *whole* line, not one field at a time, so a secret split across
  fields still gets caught; the crash-report writer and the debug bundle
  both call `redact` on their own text too, independently, so a token that
  reaches a panic message or an already-written log file is still caught.
- **Logging**: `<data dir>/logs/streamboat.log`, rotated via the
  `file-rotate` crate (`ContentLimit::BytesSurpassed` at 5 MB,
  `AppendCount` keeping 9 rotated files — Sone's own numbers, ~50 MB
  ceiling) rather than `tracing-appender`'s `RollingFileAppender`, which
  only rotates on a time interval, not a byte size. A `RingBufferLayer`
  (200 lines, redacted before insertion) feeds the crash report below;
  stderr keeps logging exactly as before (unredacted — the redaction scope
  here is the file, matching D-029's own framing of "nothing leaves the
  machine unprompted," not "nothing appears on screen").
- **Crash dumps**: `diagnostics::crash::install_panic_hook` chains to
  whatever hook was already installed (the panic still prints to stderr
  normally) and then writes one JSON report per panic to
  `<data dir>/crashes/crash-<unix_ms>.json`: timestamp (human `httpdate`
  string plus the raw unix-ms), app/version/OS/arch, the panic message and
  `Location` (both redacted), a backtrace *only* when `RUST_BACKTRACE` is
  set (this hook honours that variable itself rather than always paying for
  a capture), and the ring buffer's last-200-lines snapshot. Never writes
  anywhere else.
- **`streamboat debug-bundle [--out path]`** (`streamboat-desktop/src/main.rs`,
  logic in `streamboat_core::diagnostics::bundle`): one zip (the `zip` crate,
  pinned, `deflate`-only feature set — "one archive format, one crate," not
  a `tar`+`flate2` pair) containing `environment.txt` (resolved
  config/data/cache/runtime dirs, OS/arch, and — since `streamboat-core`
  cannot depend on the player crate, D-004 — the compiled-in engine's version
  (`streamboat_player::engine_version()`: GStreamer's own version string on
  Linux, the libmpv client API version on Windows/macOS) and the
  decoder-probe result (`streamboat_player::probe::probe()`), both gathered
  by the `streamboat debug-bundle` command itself), `settings.redacted.json`
  (`Settings::to_redacted_json` — every credential field, including the
  client ids themselves, reduced to an `_set: bool`; everything else, quality
  ceiling, output config, ReplayGain mode, theme, play-reporting flag,
  offline caps, kept as-is), and every file directly under `<data
  dir>/logs/` and `<data dir>/crashes/`, each redacted again on the way in.
  **Built by construction to exclude the token file, the offline cache and
  the keyring**: the bundler only ever reads those two named subdirectories
  of the data dir, never the data dir itself — it cannot pick up
  `tokens.bin`, `control-token`, or `offline/` even by a future bug that
  adds a new file next to them, only one that adds it *inside*
  `logs/`/`crashes/`.

Tests: the redactor over synthetic lines (each known pattern masked, plain
text and an already-redacted line both left alone); the crash-report writer
(a real panic on a spawned thread, inspected for every field including a
leaked-token message coming back redacted; the two tests that install a
process-global panic hook are serialized against each other with a
`Mutex`, since a global hook installed by one could otherwise catch the
other's synthetic panic under `cargo test`'s default thread-parallel
runner); the ring-buffer logging layer (a real `tracing` event, redacted,
captured); the bundle's content list (logs/crashes/settings/environment
present; a realistic data dir carrying `tokens.bin`/`control-token`/
`offline/` alongside `logs/`/`crashes/` proves those three are absent from
the archive, not merely "not tested for," and that a token inside a log
line is redacted inside the archive too, not just excluded by name).

## Two small hooks (D-022, D-046)

- **`Command::Logout`** (additive, `streamboat-core::proto`): handled by
  `Player` — calls `ApiClient::logout`, wipes the offline cache via
  `OfflineCache::wipe_all` when one is configured, stops playback (clears
  the queue, resets position/stream), and emits `Event::Stopped` plus
  `Event::Warning("logged out")`. Exposed by `streamboatd`'s control API
  through the ordinary `POST /v1/commands` route — no new route needed,
  since that route already accepts any `Command` generically. The CLI's
  pre-existing `streamboat logout` (direct `ApiClient::logout` + cache wipe,
  no running `Player` to send a command to) is unchanged; this command is
  for a remote control surface (or a future desktop-shell logout button)
  talking to an already-running daemon/GUI.
- **The opt-in live canary** (D-046): `crates/streamboat-core/tests/live_canary.rs`,
  `#[ignore]`d tests gated additionally by `STREAMBOAT_LIVE_CANARY=1`,
  against whatever token store `Context::load()` finds (an already
  logged-in account — this suite mints no credentials of its own):
  login-state check (`GET /v1/sessions` shape), one search, and one
  `playbackinfopostpaywall` resolve through the ordinary `resolve_stream`
  cascade — every assertion is on shape (a field exists, an enum member,
  a rank comparison), never on specific catalogue content, and nothing is
  ever printed that could leak a token or a signed CDN URL. Documented in
  `CONTRIBUTING.md` as never wired into CI; no workflow in `.github/workflows/`
  passes `--ignored`, so a bare `cargo test --workspace` (what CI runs)
  reports these three as `ignored`, not run.

## Environment variables

| Variable | Effect |
| --- | --- |
| `STREAMBOAT_HOME` | Put config, data, cache and runtime dirs under one directory |
| `STREAMBOAT_CLIENT_ID`, `STREAMBOAT_CLIENT_SECRET` | Device-code client pair at run time (or at build time to embed) |
| `STREAMBOAT_PKCE_CLIENT_ID`, `STREAMBOAT_PKCE_CLIENT_SECRET` | PKCE client pair (hi-res), same precedence |
| `STREAMBOAT_MASTER_KEY` | 32-byte token-file key as hex or base64 (headless boxes) |
| `STREAMBOAT_GST_SINK` | Replace the audio sink with any element description (`fakesink` in CI) — takes priority over the alsa-direct exclusive-mode writer too, so CI never touches ALSA |
| `STREAMBOAT_GST_PLAYBIN` | `playbin` instead of `playbin3` |
| `STREAMBOAT_MPV_AO` | Override mpv's `ao` (`mpv` feature only; `null` in CI) |
| `RUST_LOG` | Log filter (`info` prints the signal path on track start); also mapped to mpv's `msg-level` when the `mpv` feature is built |
| `STREAMBOAT_LIVE_CANARY` | `1` enables the opt-in live-canary tests (D-046) against an already-logged-in account; unset (the CI default) leaves them `ignored` |

## Tests

- `streamboat-core`: unit tests for the token store (truncation at every byte
  offset, key resolution against a fake keyring, explicit migration both ways,
  the cross-process lock), PKCE (RFC 7636 test vector, code extraction, the
  loopback listener), manifest parsing and refusal, error-body shapes; wiremock
  transport tests for the device-code flow, the PKCE exchange and refresh,
  refresh semantics, adoption of another process's refresh, the 429 gate,
  playback sub-statuses and the cascade; `tests/catalog.rs` covers the
  catalogue/library surface above — pure-parsing tests against synthetic
  fixtures (feed sections with an unknown `type`, null fields, and a
  missing/empty response; the `Favorite<T>`/`PlaylistItem` tolerant shapes)
  plus wiremock tests for every entity/list/search endpoint and the
  playlist-mutation ETag flow (including a dedicated reorder-sends-the-
  fetched-etag case, and a case that supplies its own etag to skip the
  fetch); `tests/privileges.rs` runs a real local `tokio-tungstenite`
  websocket server (handshake, the `USER_ACTION` claim shape, a
  `PRIVILEGED_SESSION_NOTIFICATION` revoke, `RECONNECT`) plus pure backoff-
  ceiling tests; `tests/reporting.rs` covers the event-batch payload shape,
  the 30s threshold, PREVIEW suppression, the disabled flag, drop-on-
  sender-fault, retry-on-5xx and queue persistence across a restart, all
  via wiremock; `tests/scrobble.rs` covers both backends' request shapes
  (plus a known-vector test for the Last.fm signature, computed
  independently with `md5sum` in the test's own comment).
- `streamboat-player`: the GStreamer backend over generated WAV files through
  `fakesink` (single track, gapless hand-over, error paths); the libmpv
  backend (`mpv` feature) over hand-written PCM WAV files through `ao=null`
  (single track, gapless hand-over via `loadfile append`, an unreadable
  source, a bogus DASH manifest materialized as a real temp file) — tested
  on Linux only; the Windows/macOS-specific AO option values (§ above) are
  cfg-gated but unexercised here, no CI runner for either yet; the Player against
  a fake engine and an in-process TIDAL (queue, skip-with-event, exclusive
  volume policy, JSON round trip); `mpris` unit tests cover pure mapping only
  (`TrackSummary` → `Metadata`, `PlaybackStatus` mapping, the exclusive-mode
  volume rule) — CI has no D-Bus session bus, so registering a real `Player`
  is not exercised there; `alsa_writer`'s pure logic (format
  probing/promotion table, the ALSA/GStreamer format-name inversion, the
  rate read-back message, period/buffer/`sw_params` math, mix-matrix
  construction, XRUN/suspend/`ENODEV` classification, reopen-on-change);
  `tests/alsa_direct.rs` plays a generated WAV through the real appsink →
  writer → ALSA `null` device path to `EndOfStream` and checks a
  same-format gapless pair reopens the PCM exactly once, skipping with a
  message if `null` can't be opened (see the exclusive-mode bullet above
  for what this does *not* verify — everything hardware-dependent, tested
  only by hand against a real DAC);
  `tests/offline.rs` covers the offline
  cache end to end against a wiremock TIDAL and a real (loopback) HTTP
  round trip — a pin's chunk files hold no plaintext, playback through the
  loopback route round-trips the exact bytes (compared by hash) including
  a `Range` request and a wrong-token/non-loopback refusal, a PREVIEW asset
  and an encrypted manifest are both refused (and the mock's recorded
  requests never carry `playbackmode=OFFLINE`/`usage=DOWNLOAD`), unpin
  deletes chunks, `wipe_all`/`wipe_dir` both wipe everything, the size cap
  refuses a new pin and cleans up after itself, a 0-day validity window
  forces (and, once `/v1/sessions` answers, completes) revalidation, and a
  wrong device key (a `MemoryKeySlot`-backed second "install" over the same
  directory) is never served; `src/offline.rs`'s own unit tests cover the
  DASH `$Number$`/`SegmentTimeline` planner (including refusing an
  open-ended one), the chunk AES-GCM round trip, HKDF device-binding, and
  key resolution against a `MemoryKeySlot`;
  `platform` covers `enumerate_output_devices`
  not panicking and `default_engine` constructing the GStreamer backend
  (both Linux, fakesink) plus a pure `parse_mpv_device_list` JSON-mapping
  test that runs on Linux too under the `mpv` feature (deliberately not
  gated to "only when mpv is the preferred backend," unlike the function it
  tests, so `cargo test -p streamboat-player --features mpv` still exercises
  it even though `GstEngine` wins there); `smtc`/`nowplaying` unit tests
  cover pure mapping only (`PlaybackStatus`/`MediaPlaybackStatus`/
  `MPNowPlayingPlaybackState` conversion, the exclusive-mode volume-is-the-
  engine's-problem rule) and are compiled only on their own OS — neither
  runs anywhere here, see "Cross-target type-checking" below.
- `streamboat-server`: `tests/api.rs` (see "Control API" above) — health,
  auth, Host allowlisting, a command changing state, and both directions of
  the WebSocket, all over real sockets against an in-process daemon.
- `streamboat-desktop`: 111 tests (`cargo test -p streamboat-desktop`), all inline
  `#[cfg(test)]` (this crate is bin-only, no `lib.rs`, so there is no separate
  `tests/` integration-test target). iced 0.14's `iced_test` headless simulator
  (`simulator(view(...))`, `ui.find("text")`, confirmed to fall back to the
  `tiny-skia` CPU renderer with zero display server — see the `iced-ui` skill
  §7) covers: Login renders the device code and verification URL, and an inline
  error; Home renders a synthetic feed section plus the graceful unknown-section
  fallback, and a loading state; Explore does the same for the v1 module shape;
  the playback bar's volume control area renders the exclusive/shared badge
  correctly in both modes; Now Playing renders the current track, queue, and a
  "nothing playing" placeholder; the signal-path panel renders the AAC
  "bit-perfect not applicable" line and a "nothing playing" placeholder; Search
  renders the query/filter row and track results, and recognises a pasted
  content link instead of debouncing a search for it; Album renders its
  header/badges/track list and a loading state, and its highlighted-row view
  used by Track; Artist renders its header, top tracks, album tabs and the
  Following-vs-Follow label; Playlist shows/hides the rename/delete controls
  by ownership and renders its track list; Mix renders its title and items;
  Track renders a credits panel once loaded; Video renders its metadata and
  the fixed "not supported yet" notice, and a loading state; Collection renders
  its tab bar and switches tabs, and its create-playlist dialog opens/closes;
  Lyrics renders synced lines with the current one picked out, the plain-text
  fallback, and the "no lyrics" note; the notification banner renders a
  message (with a Resume button for a takeover) and clears on dismiss/resume;
  the shared per-track action row renders every button it's given. Plain
  `#[test]`s (no simulator) cover the pure view-model helpers —
  `mmss`/`quality_badge`/`bit_perfect_applicable` formatting,
  `feed_section_to_view`/`page_module_to_view`'s known-vs-unknown-type mapping,
  the `Nav` back/forward stack (five cases: push+clear-forward, round-trip,
  no-op on empty history, dropping the stale forward branch after a fresh
  `go_to`, no-op on navigating to the current screen) plus `Screen::from_content_link`'s
  mapping for every `ContentLink` variant, the `ImageCache`'s
  insert/evict/re-insert behaviour, `Tokens::dark()`/`light()` (distinct
  colours, shared scale), the `Settings`⇄`settings::State` round trip,
  `lyrics::highlighted_index`'s line-selection logic, `actions::shuffled`'s
  permutation property, each entity screen's `Effect` output for its own
  `update` (play-all's track-id order, an item removal/reorder updating the
  local list, a confirmed delete bubbling `Effect::Deleted`), and a track
  row's favourite toggle updating local state on a successful API result.
  The mini-player (`ui::mini_player`) renders "nothing
  playing," the current track/quality badge, and that its restore button emits
  `Message::Restore` on click. Plain `#[test]`s (no simulator) also cover
  `ui::tray::tray_event_to_command`'s pure mapping (every fixed menu entry
  present and distinct, PlayPause/Next/Previous map to their `Command`,
  Show/Hide/Quit correctly map to none), and `ui::instance::decide`'s three
  outcomes (`Local` when nothing else holds the lock, `Remote` when a
  `control-address` file is present, `FocusedOther` plus a `show-request`
  touch when neither) against real `fd-lock` files under a temp directory.
  `ui::remote_link::RemoteLink` is exercised against a real, separately
  runtime-hosted `streamboat_server::api::router` (the `FakeEngine` pattern,
  a local copy of `streamboat-server`'s own `tests/common` since that module
  is not importable across the crate boundary): a snapshot on connect, a
  `Play` command round-tripping back as a `State` event over the real HTTP +
  WebSocket wire, and a full reconnect story — the daemon's entire runtime is
  torn down mid-connection (`shutdown_background`, the only thing that
  reliably kills an already-accepted connection's task; aborting just the
  top-level `axum::serve` future does not, since each connection's handler is
  its own independently spawned task), the link surfaces a reconnect
  `Warning` rather than ending the stream, and once a fresh daemon binds the
  same address it reconnects and delivers a new snapshot.
  **Not verified without a display or a session/D-Bus bus** (this container
  has neither): the actual `iced::daemon(...).run()` event loop, real window
  creation/hide/show/focus and multi-window behaviour, real mouse/keyboard
  delivery through winit, whether `iced::widget::operation::scroll_to`
  actually scrolls the Lyrics screen's line list to the right offset (the
  pure `highlighted_index` helper it's driven by is tested; the scroll
  operation itself is not, the same way `Simulator` cannot drive a real click
  sequence through a `stack!` overlay to verify the add-to-playlist picker's
  dimmed backdrop blocks clicks to what's behind it), anything about visual
  layout beyond what `ui.find("...")` widget-tree assertions can see (no pixel/snapshot tests
  were taken here, though `Simulator::snapshot` exists for a future pass that
  adds them), the Linux tray's actual D-Bus/`ksni` registration (`mpris.rs`'s
  own D-Bus registration has the same gap), and the Windows/macOS tray-icon
  path, which is additionally unreachable to `cargo check` on this Linux
  sandbox at all (`#[cfg(not(target_os = "linux"))]`) — cross-checking it
  with `cargo check --target x86_64-pc-windows-gnu`/`--target
  aarch64-apple-darwin` was attempted and hit an unrelated, pre-existing
  blocker: `gstreamer`/`gstreamer-audio` are unconditional (not
  `target_os`-gated) dependencies of `streamboat-desktop`'s `Cargo.toml`, and
  `gstreamer-rs`'s `glib-sys` needs a real GLib pkg-config sysroot for the
  target platform, which this environment does not have — a pre-existing gap
  unrelated to the tray implementation, not something addressed here.
- `streamboat-core::diagnostics` (D-029): the redactor over synthetic lines
  covering every pattern plus plain text and idempotence; a real panic on a
  spawned thread producing a crash report with every field, including a
  redacted leaked-token panic message; the ring-buffer logging layer over a
  real `tracing` subscriber; the debug bundle's content list, including a
  realistic data dir that carries `tokens.bin`/`control-token`/`offline/`
  alongside `logs/`/`crashes/` to prove those three never reach the
  archive, and that a token inside a bundled log line is redacted in the
  archive, not merely excluded by filename. `Settings::to_redacted_json`
  (`streamboat-core::config`) is covered separately: every credential field
  reduced to a bool, non-secret fields untouched. The two crash-report
  tests share a `Mutex` since `install_panic_hook` replaces process-global
  state and `cargo test` runs this binary's tests on multiple threads by
  default.
- ReplayGain-mode selection (D-019, `crates/streamboat-player/tests/player.rs`):
  album mode preferring the album pair and falling back to the track pair
  when TIDAL reported none; track mode ignoring a present album value;
  off mode applying neither number; `Command::SetReplayGainMode` recomputing
  an already-prefetched successor live (polled with a timeout, since the
  successor's *first* prefetch genuinely awaits the wiremock HTTP round
  trip and can complete a moment after the `TrackStarted` event that
  triggered it); the command's JSON shape.
- Snapcast output (D-034): `crates/streamboat-player/tests/gst_engine.rs`'s
  `snapcast_output_streams_the_fixed_format_pcm_over_tcp` plays a generated
  WAV through the real `OutputConfig::Snapcast` branch into a local
  `TcpListener` standing in for snapserver, and checks the byte count
  against the WAV's own duration at the fixed format; `streamboat-desktop`'s
  `snapcast_plugin`/`snapcast_discover` modules cover the JSON-RPC message
  shapes (request/response/notification encoding, `id` handling, the
  standard error codes, the `Control`/`SetProperty` → `Command` mapping)
  and the mDNS record-to-address parsing against a synthetic
  `ResolvedService`, both with no real network. **Not verified here — no
  snapserver in this environment**: an actual round trip against real
  `snapserver`/Snapweb, the libmpv/FIFO pump path (no test requires it —
  see the Multiroom section above), and real-network mDNS discovery.
- `crates/streamboat-core/tests/live_canary.rs` (D-046): `#[ignore]`d,
  additionally gated by `STREAMBOAT_LIVE_CANARY=1`; never run by any CI
  workflow (see `CONTRIBUTING.md`).
- CI: fmt, clippy `-D warnings`, tests, release build; `cargo test -p
  streamboat-player --features mpv` on top of the default (GStreamer) build;
  a Debian container job builds `streamboatd` without GUI libraries and
  asserts none are linked — `axum`, `tokio-tungstenite` and
  `mpris-server`/`zbus` are all pure Rust and link no system D-Bus or GUI
  library, so this still passes. The `check` job also installs
  `libxkbcommon-dev`, `libxkbcommon-x11-dev`, `libwayland-dev`, `libx11-dev`,
  `libxrandr-dev`, `libxi-dev`, `libxcursor-dev` for iced/winit; the
  headless-daemon job is unaffected (it never builds `streamboat-desktop`).
  A `cross-check` job type-checks the Windows (`mpv`, `smtc`) feature
  combination — see below for exactly what that does and does not prove,
  and why macOS has no equivalent CI job.

## Cross-target type-checking (D-016)

Run by hand (`rustup target add x86_64-pc-windows-gnu
aarch64-apple-darwin`), and as the `cross-check` CI job for the half
that can run unattended:

- **`cargo check -p streamboat-player --target x86_64-pc-windows-gnu
  --no-default-features --features mpv,smtc` — passes, in CI now.** Needs
  `gcc-mingw-w64-x86-64` installed first: `streamboat-core`'s `reqwest`/
  `tokio-tungstenite` pull in `ring`, whose build script compiles C
  regardless of target, even at `cargo check` time — this is not specific
  to `libmpv2-sys` (which also builds cleanly here), it is just the first
  time this workspace has cross-checked a
  non-Linux target at all. With that toolchain present the whole
  dependency graph — `libmpv2-sys`/`libmpv2`, `windows` 0.62.2 with every
  feature `smtc.rs` uses, and `smtc.rs` itself — type-checks with zero
  errors and exactly one warning, pre-existing and unrelated to `smtc.rs`
  (`streamboat-core::fsutil`'s `unused import: File`, live only on
  a non-Linux target; not fixed here since `streamboat-core` is a separate
  crate outside `streamboat-player`'s scope — flagged for whoever next touches that module).
- **`cargo check -p streamboat-player --target aarch64-apple-darwin
  --no-default-features --features mpv,nowplaying` (or `nowplaying` alone)
  — fails before reaching any of `mpv.rs`/`nowplaying.rs`, in or out of CI.** Same
  `ring` build script, but this time it invokes the *host's* `cc` with
  macOS-only flags (`-arch arm64`, `-mmacosx-version-min=11.0`) that a
  plain Linux `cc` does not understand — cross-compiling `ring`'s C needs a
  real Apple SDK/`osxcross`-class toolchain, not just a Rust target added
  via `rustup`, and installing one is out of scope here (and
  arguably for a CI runner at all — Apple's SDK terms are the same reason
  no reference client in this project's research vendors one). This blocks
  before `objc2`/`nowplaying.rs` are reached at all, on either feature
  combination, so there is no CI job for it.
- **Supplementary check, not part of the CI job (`ring` never involved):**
  a standalone scratch crate depending on `objc2-foundation` 0.3.2,
  `objc2-media-player` 0.3.2 and `block2` 0.6.2, plus `objc2` 0.6.3 as
  declared (resolving to 0.6.4 here, same as the real crate's own Cargo.lock
  entry) — reproducing `nowplaying.rs`'s `wire_commands`/
  `update_now_playing_info` logic verbatim, checked clean against
  `aarch64-apple-darwin` with zero errors and (once two redundant nested
  `unsafe` blocks inside a closure already covered by an enclosing one were
  removed — a real finding from this check, now fixed in `nowplaying.rs`
  too) zero warnings. This is real evidence for the `objc2` API calls
  themselves (extern statics, `RcBlock::new`, `addTargetWithHandler`,
  `NSDictionary::from_slices`, `NonNull::cast`) being correct against the
  pinned versions on this target; it is not evidence that the full crate
  builds for macOS (blocked by `ring`, above) or that `MPNowPlayingInfoCenter`/
  `MPRemoteCommandCenter` behave as documented against a live Objective-C
  runtime — no macOS machine of any kind is available here.
- **Untested no differently than `mpv.rs`'s existing Windows/macOS AO
  option values**: neither `smtc.rs` nor `nowplaying.rs` has run against a
  real SMTC popup, Control Center, or `MPRemoteCommandCenter` callback.
  Both are written from the `windows`/`objc2*` crates' own published,
  generated bindings, checked line-by-line against each crate's actual
  source (not from memory) and, for `smtc.rs`, against a
  full, successful cross-target `cargo check` — a meaningfully higher bar
  than "compiles," but still short of "seen it work."

## Packaging (D-041)

`packaging/` holds every release artifact's source, and
`crates/streamboat-desktop/Cargo.toml`/`crates/streamboat-server/Cargo.toml`
carry `[package.metadata.deb]`/`[package.metadata.generate-rpm]` sections
for their respective binaries — see `packaging/README.md` for the full
per-artifact explanation, what was verified locally (both deb packages and
both rpm packages were actually built and inspected in this repository's
own dev environment; `dpkg-deb -c`/`rpm -qlp` confirm the file lists,
`systemd-analyze verify` confirms both systemd units), and what could not
be (no Windows or macOS machine exists in that environment; the WiX MSI,
the DMG script and the winget manifests are validated as well-formed
XML/YAML only).

`.github/workflows/release.yml` builds all of it on a `vX.Y.Z` tag: Linux
deb/rpm/AppImage/tarball on native x86_64 and aarch64 runners, a Windows
MSI, a macOS DMG (arm64 and x86_64), and the `streamboatd` Docker image
pushed to `ghcr.io`, all collected into one draft GitHub Release with a
`SHA256SUMS` file. No code signing anywhere (D-042); no in-app updater
(D-043); AUR/winget/Homebrew/Flathub submissions stay human-authored,
outside CI (D-040, D-041) — see CONTRIBUTING.md's release checklist.

The Windows and macOS build steps in `release.yml` run
`cargo build -p streamboat-desktop -p streamboat-server --no-default-features
--features streamboat-desktop/mpv,streamboat-server/mpv`: both binaries
forward engine selection to `streamboat-player` through their own
`gstreamer` (default) and `mpv` features and construct the engine through
`streamboat_player::default_engine`, so no GStreamer symbol is referenced
off Linux. The same invocation builds on Linux (against the distro libmpv)
and is what CI's `mpv-only-build` check runs; the Windows and macOS jobs
themselves have not run on a real runner yet.

deb/rpm depend on the distro's own GStreamer packages for this first
release rather than the `/opt/streamboat` vendored tree D-041 describes —
`packaging/linux/vendor-gstreamer.sh` is the real, complete script for
building that tree, just not wired into any job yet; `packaging/README.md`
explains why, and the "Still open" list in `streamboat-decisions` still
carries the question of whether that tree or the AppImage ends up the
recommended Debian-stable install path.

## Not yet built (in decision order)

the `streamboat://` handler registration per OS (D-024 — the OS-side
registration files exist in `packaging/linux/`, `packaging/windows/wix/` and
`packaging/macos/`, and `streamboat open <url>`/a bare-link argument parse and
navigate; what is missing is only the shell being launched by the OS with that
URL through the single-instance path); the Flatpak Secret portal (D-026); the
`org.freedesktop.ReserveDevice1` device-reservation handshake for the ALSA
writer (`output-backends.md` §2, explicitly optional — `EBUSY` on open is
handled with a bounded retry regardless); the
`/opt/streamboat` vendored GStreamer tree actually wired into a deb/rpm job
(the build script exists, see "Packaging" above); an AppUserModelID for
Windows SMTC and a universal macOS build (both still-open packaging
questions, see `streamboat-decisions`); video *playback* (D-038 — the Video
entity page itself is built, metadata-only, and says so); handing a
`streamboat://` link to an already-running instance (a second `streamboat
<url>` only asks the running window to come to the front today); real
verification of the Windows/macOS tray-icon path, the Linux tray's D-Bus
registration and the multi-window loop (no display, no session bus here);
the MPRIS-bus-name variant of D-010's single-instance lock (the portable
lock-file branch the decision also names is what runs); a Snapcast output toggle in the Settings screen (D-034 — the
`OutputConfig::Snapcast` variant and both engine backends exist, `Cmd`/CLI
plumbing for it does too, but the UI's own output picker still offers only
Shared/Exclusive, falling back to Shared when it round-trips a
`Snapcast` value it did not create); the libmpv/FIFO Snapcast pump on
Windows/macOS (D-034, Unix-only so far — `start_snapcast_pump` returns a
clear error there rather than a silent no-op); a verified round trip
against a real `snapserver`/Snapweb for both the Snapcast output and the
`snapcast-plugin`/`snapcast-discover` subcommands (no snapserver in this
environment).

`streamboat_player::default_engine`/`enumerate_output_devices`/
`media_controls::spawn` (D-016, D-030 — see "Engine selection, device
enumeration and media controls" above) are built and wired into
`streamboatd`, the `streamboat` CLI's `play` and `devices` commands and the
shell (`ui::engine_select`, `ui::app::run_as_local_instance`).
`smtc.rs`/`nowplaying.rs` are built (Windows/macOS, default-on features
`smtc`/`nowplaying`) but untested beyond cross-target type-checking — no
Windows or macOS CI runner exists yet; see "Cross-target type-checking"
below for exactly what did and did not get checked, and each module's own
doc comment for the specific open questions (SMTC: none beyond "no live
popup was ever seen"; NowPlaying: whether `MPRemoteCommandCenter`'s handlers
fire at all with no AppKit run loop in a bare daemon).
