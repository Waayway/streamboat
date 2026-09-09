# Architecture (as built by the playable spike)

Companion to `docs/DECISIONS.md`: what exists in the workspace today and the
implementation choices made while building the first milestone (D-044).

## Crates

| Crate | Licence | Contents |
| --- | --- | --- |
| `streamboat-core` | Apache-2.0 | `http` (authenticated client, single-flight and cross-process refresh, 429 gate), `auth::device_code`, `auth::pkce`, `token_store` (AES-256-GCM file, `SBTK` header, keyring-held master key), `manifest` (BTS/EMU/DASH parsing, refusal rule), `api` (sessions, tracks, `playbackinfopostpaywall`, quality cascade, plus the catalogue/library surface below), `proto` (Command/Event), `config` (`AppDirs`, `Settings`, the control API's bearer-token file), `credentials` (device-code and PKCE pairs), `bootstrap`, `privileges` (the Pushkin streaming-privileges websocket, D-033), `reporting` (play reporting to `ec.tidal.com` and the server-anchored clock, D-027), `scrobble` (Last.fm/ListenBrainz, D-037) |
| `streamboat-player` | GPL-3.0-only | `engine::Engine` trait, `gst::GstEngine` (playbin3, default feature), `mpv::MpvEngine` (libmpv2, `mpv` feature — D-016), `alsa_writer::ExclusiveSink` (exclusive-mode ALSA writer, Linux, `alsa-direct` feature, default on), `player::Player` (queue, prefetch, Command→Event loop, `PlayerHandle::publish` for externally-sourced events, and the optional `PlayerDeps` wiring for the three modules above), `platform::default_engine`/`platform::enumerate_output_devices` (pick the compiled-in engine and list its output devices, see below), `media_controls::spawn` (picks the OS media-integration adapter below), `mpris` (Linux, feature `mpris`, default on: `org.mpris.MediaPlayer2.streamboat`), `smtc` (Windows, feature `smtc`, default on: SMTC via `MediaPlayer::SystemMediaTransportControls`), `nowplaying` (macOS, feature `nowplaying`, default on: `MPNowPlayingInfoCenter`/`MPRemoteCommandCenter`) |
| `streamboat-server` | GPL-3.0-only | `streamboatd`: `api` (the HTTP + WebSocket control API, see below) hosted by default; `--stdio` keeps the original JSON-lines transport; headless login as `auth_required`/`auth_ok` events on the same broadcast every front end reads; constructs the privileges socket, play reporter and scrobblers from `Settings` |
| `streamboat-desktop` | GPL-3.0-only | `streamboat`: CLI subcommands (login [--pkce], logout, whoami, search, resolve, play, devices, keyring, paths); the iced shell is not built yet |

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
  hit and fixed during this work, see the field's doc comment in `mpv.rs`.
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
what `streamboatd` (and, once it exists, the desktop shell — see the note in
"Not yet built" below) call instead of naming `GstEngine`/`MpvEngine` or an
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
  `DeviceMonitor` on Linux — moved here from `streamboat-desktop`'s
  `devices` CLI subcommand, which still has its own copy of the same logic
  inline (that CLI is another agent's in-flight work in this repository;
  the desktop shell's `devices` command should call this function instead
  once that lands, per the note below) — reading `api.alsa.path` or
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
`Win32_System_WinRT` feature beyond the four the task named is for exactly
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
handle. **Open, not resolved by this change**: `os-integration.md` §1 notes
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
each crate's actual source for this change, not from memory) rather than
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
with no dependencies configured behaves exactly as it did before this work,
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

`streamboatd` (`streamboat-server`) constructs all three from `Context` and
passes them to `Player::spawn` — the headless daemon is where Pushkin
matters from day one (no user watching a silent revocation). The
`streamboat` CLI spike still passes `PlayerDeps::default()`; wiring the
desktop shell up the same way is follow-up work, tracked below.

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
  only by hand against a real DAC); `platform` covers `enumerate_output_devices`
  not panicking and `default_engine` constructing the GStreamer backend
  (both Linux, fakesink) plus a pure `parse_mpv_device_list` JSON-mapping
  test that runs on Linux too under the `mpv` feature (deliberately not
  gated to "only when mpv is the preferred backend," unlike the function it
  tests, so `cargo test -p streamboat-player --features mpv` still exercises
  it even though `GstEngine` wins there); `smtc`/`nowplaying` unit tests
  cover pure mapping only (`PlaybackStatus`/`MediaPlaybackStatus`/
  `MPNowPlayingPlaybackState` conversion, the exclusive-mode volume-is-the-
  engine's-problem rule) and are compiled only on their own OS — neither
  runs anywhere in this change, see "Cross-target type-checking" below.
- `streamboat-server`: `tests/api.rs` (see "Control API" above) — health,
  auth, Host allowlisting, a command changing state, and both directions of
  the WebSocket, all over real sockets against an in-process daemon.
- CI: fmt, clippy `-D warnings`, tests, release build; `cargo test -p
  streamboat-player --features mpv` on top of the default (GStreamer) build;
  a Debian container job
  builds `streamboatd` without GUI libraries and asserts none are linked —
  `axum`, `tokio-tungstenite` and `mpris-server`/`zbus` are all pure Rust and
  link no system D-Bus or GUI library, so this still passes; a `cross-check`
  job type-checks the Windows (`mpv`, `smtc`) feature combination — see
  below for exactly what that does and does not prove, and why macOS has no
  equivalent CI job.

## Cross-target type-checking (D-016, this task)

Run by hand for this change (`rustup target add x86_64-pc-windows-gnu
aarch64-apple-darwin`), and as the new `cross-check` CI job for the half
that can run unattended:

- **`cargo check -p streamboat-player --target x86_64-pc-windows-gnu
  --no-default-features --features mpv,smtc` — passes, in CI now.** Needs
  `gcc-mingw-w64-x86-64` installed first: `streamboat-core`'s `reqwest`/
  `tokio-tungstenite` pull in `ring`, whose build script compiles C
  regardless of target, even at `cargo check` time — this is not specific
  to `libmpv2-sys` (which also builds cleanly here) or to anything new in
  this change, it is just the first time this workspace has cross-checked a
  non-Linux target at all. With that toolchain present the whole
  dependency graph — `libmpv2-sys`/`libmpv2`, `windows` 0.62.2 with every
  feature this task uses, and `smtc.rs` itself — type-checks with zero
  errors and exactly one warning, pre-existing and unrelated to this
  change (`streamboat-core::fsutil`'s `unused import: File`, live only on
  a non-Linux target; not touched here since `streamboat-core` is outside
  this task's scope — flagged for whoever next touches that module).
- **`cargo check -p streamboat-player --target aarch64-apple-darwin
  --no-default-features --features mpv,nowplaying` (or `nowplaying` alone)
  — fails before reaching any of this task's code, in or out of CI.** Same
  `ring` build script, but this time it invokes the *host's* `cc` with
  macOS-only flags (`-arch arm64`, `-mmacosx-version-min=11.0`) that a
  plain Linux `cc` does not understand — cross-compiling `ring`'s C needs a
  real Apple SDK/`osxcross`-class toolchain, not just a Rust target added
  via `rustup`, and installing one is out of scope for this change (and
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
  runtime — no macOS machine of any kind was available to this change.
- **Untested no differently than `mpv.rs`'s existing Windows/macOS AO
  option values**: neither `smtc.rs` nor `nowplaying.rs` has run against a
  real SMTC popup, Control Center, or `MPRemoteCommandCenter` callback.
  Both are written from the `windows`/`objc2*` crates' own published,
  generated bindings, checked line-by-line against each crate's actual
  source for this change (not from memory) and, for `smtc.rs`, against a
  full, successful cross-target `cargo check` — a meaningfully higher bar
  than "compiles," but still short of "seen it work."

## Not yet built (in decision order)

the `streamboat://` handler for the desktop shell (D-024); the Flatpak Secret
portal (D-026); the `org.freedesktop.ReserveDevice1` device-reservation
handshake for the ALSA writer (`output-backends.md` §2, explicitly optional
— `EBUSY` on open is handled with a bounded retry regardless); streaming
privileges, play reporting and scrobbling wired into the `streamboat` CLI/desktop shell
rather than only `streamboatd` (D-033, D-027, D-037 — the modules and the
daemon wiring exist; see the section above); the iced shell (D-013); the
offline cache (D-022); packaging (D-041).

`streamboat_player::default_engine`/`enumerate_output_devices`/
`media_controls::spawn` (D-016, D-030 — see "Engine selection, device
enumeration and media controls" above) are built and wired into
`streamboatd`; **the desktop shell's `main.rs` should call the same three
helpers once its own rewrite lands** — `Cmd::Play` in place of its direct
`GstEngine::new(...)`, `Cmd::Devices` in place of its own inline
`DeviceMonitor` copy (`enumerate_output_devices` is the single
source of that logic now), and once it holds a `PlayerHandle` for real,
`media_controls::spawn(handle)` in place of nothing today — this was not
done here because another agent owns `crates/streamboat-desktop/src/main.rs`
in this worktree. `smtc.rs`/`nowplaying.rs` are built (Windows/macOS,
default-on features `smtc`/`nowplaying`) but genuinely untested beyond
`cargo check`/cross-target type-checking — no Windows or macOS CI runner
exists yet; see "Cross-target type-checking" below for exactly what did and
did not get checked, and each module's own doc comment for the specific
open questions (SMTC: none beyond "no live popup was ever seen"; NowPlaying:
whether `MPRemoteCommandCenter`'s handlers fire at all with no AppKit run
loop in a bare daemon).
