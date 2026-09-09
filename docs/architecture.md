# Architecture (as built by the playable spike)

Companion to `docs/DECISIONS.md`: what exists in the workspace today and the
implementation choices made while building the first milestone (D-044).

## Crates

| Crate | Licence | Contents |
| --- | --- | --- |
| `streamboat-core` | Apache-2.0 | `http` (authenticated client, single-flight and cross-process refresh, 429 gate), `auth::device_code`, `auth::pkce`, `token_store` (AES-256-GCM file, `SBTK` header, keyring-held master key), `manifest` (BTS/EMU/DASH parsing, refusal rule), `api` (sessions, tracks, `playbackinfopostpaywall`, quality cascade, plus the catalogue/library surface below), `proto` (Command/Event), `config` (`AppDirs`, `Settings`, the control API's bearer-token file, `Settings::to_redacted_json`), `credentials` (device-code and PKCE pairs), `bootstrap`, `privileges` (the Pushkin streaming-privileges websocket, D-033), `reporting` (play reporting to `ec.tidal.com` and the server-anchored clock, D-027), `scrobble` (Last.fm/ListenBrainz, D-037), `diagnostics` (redacted logging, crash dumps, the debug-bundle archiver, D-029) |
| `streamboat-player` | GPL-3.0-only | `engine::Engine` trait, `gst::GstEngine` (playbin3, default feature), `mpv::MpvEngine` (libmpv2, `mpv` feature — D-016), `alsa_writer::ExclusiveSink` (exclusive-mode ALSA writer, Linux, `alsa-direct` feature, default on), `offline::OfflineCache` (pinned, encrypted offline cache, D-022), `player::Player` (queue, prefetch, Command→Event loop, ReplayGain-mode selection D-019, `PlayerHandle::publish` for externally-sourced events, and the optional `PlayerDeps` wiring for the three modules above), `mpris` (Linux, feature `mpris`, default on: `org.mpris.MediaPlayer2.streamboat`), `snapcast` (the fixed PCM format both engine backends target for `OutputConfig::Snapcast`, D-034) |
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

## Desktop shell (iced) (D-010, D-012, D-013)

`crates/streamboat-desktop/src/ui/` (loaded by `main.rs`'s `mod ui;`; `streamboat` with no
subcommand calls `ui::run()`, every existing CLI subcommand is unchanged). Pinned to
`iced = "=0.14.0"` exactly, features `tokio`, `image`, `svg`, `advanced`, `debug` — see the
`iced-ui` skill for the pinned-API facts this wave verified against the actual 0.14 sources rather
than memory (the API changed hard across 0.9-0.14, per D-013).

- **Architecture**: `ui::app::App` is the top-level `iced::application` state; `ui::app::Message`
  wraps each screen's own message enum (D-013's "split messages per screen/module") plus
  `PlayerEvent(Box<Event>)` (boxed: `Event::State` carries a full snapshot and dwarfs every other
  variant), `Nav`, `LoginCheck`, `ImageFetched` and `KeyShortcut`. `ui::nav::{Screen, Nav}` is the
  navigation stack (`go_to`/`back`/`forward`, back pushes history and clears forward, matching the
  task brief). `ui::design::Tokens` is the design-token struct (background/surface/elevated/
  accent/text/muted/warning/danger/success/border colours, a radius/spacing/type scale) with
  `dark()` (default) and `light()` constructors; `Tokens::iced_theme` builds the `iced::Theme::custom`
  palette stock widgets style against, while custom containers/cards/badges read the tokens
  directly (captured `Copy` into style closures) — see the `iced-ui` skill §"Owner-context notes"
  for why the token struct, not `iced::Theme`, is the source of truth.
- **The engine seam (D-010)**: `ui::player_link::PlayerLink` is a trait (`send(Command) -> bool`,
  `events() -> BoxStream<Event>`); `InProcessLink` wraps a `PlayerHandle` from `Player::spawn` — the
  *only* two `streamboat-player` APIs the shell touches, never `Engine`/a backend type directly.
  `RemoteLink` is a documented, deliberately-unimplemented stub for the future control-API client
  (D-030) — `PlayerLink::connect` always errors today; no HTTP/WebSocket code exists yet because
  `streamboatd`'s control API doesn't either. `Player::spawn` runs on a second, explicitly-built
  `tokio::runtime::Runtime` kept alive for the process's lifetime (a local variable in `ui::app::run`
  that outlives the blocking `.run()` call) — separate from iced's own internal tokio runtime (its
  `tokio` feature), which drives `Task`/`Subscription` futures instead.
- **Startup (task item 2)**: `ui::app::run()` loads `Context` (now `#[derive(Clone)]`, an additive
  change — every field it holds was already `Clone`, needed so the `Fn`-bound `boot` closure can
  `ctx.clone()` on each call instead of moving out of a capture), builds the platform engine via
  `ui::engine_select::build` (`#[cfg(target_os = "linux")]` → `GstEngine`; a `compile_error!` fires
  on any other target unless this crate's own `mpv` feature is on, which gates a call site for the
  libmpv backend another agent is adding to `streamboat-player` — see that module's doc comment),
  spawns `Player`, and opens the window. The app starts on `Screen::Login` and only flips to `Home`
  once an async `ApiClient::is_logged_in()` check resolves `true` (never optimistically shows a
  protected screen first).
- **Screens this wave**: Login (`ui::screens::login` — device-code and PKCE-paste/PKCE-loopback,
  reusing `auth::device_code`/`auth::pkce` exactly as the CLI does; errors inline), Home
  (`ui::screens::home` — v2 `home/feed` sections, the tab bar from `header.vibes.items`, cursor
  paging, per-section "View all" via `expand_section`), Explore (`ui::screens::explore` — the v1
  `pages/explore` shape, same graceful-unknown-module rendering), Search (`ui::screens::search` —
  all types, a type filter, 300ms-debounced input), Now Playing (`ui::screens::now_playing` — large
  art, seek, quality badge, the queue list with move-up/move-down/remove buttons over
  `Command::MoveQueueItem`/`RemoveQueueItem`, a Lyrics button routing to the placeholder), the
  persistent playback bar (`ui::playback_bar` — art/title/artist/transport/seek/volume/badges/queue
  and signal-path toggles; the volume slider dims and grows a tooltip while `OutputConfig::is_exclusive()`,
  per D-017, rather than becoming inert — it still sends `SetVolume`, which the Player already
  refuses with a `Warning` event in exclusive mode), the signal-path panel (`ui::signal_path` —
  renders `PlayerState::signal_path` field-for-field, `None` stays "Unknown" rather than guessing,
  and prints "lossy source, bit-perfect not applicable" for AAC/lossy tiers per D-036), and Settings
  (`ui::screens::settings` — quality ceiling, output device + exclusive toggle, ReplayGain mode,
  play-reporting toggle with the D-027 disclosure text, credentials, key storage, theme, logout).
  Entity/Collection/Lyrics screens (`ui::screens::placeholder`) are the NEXT wave: routing is
  complete (cards already navigate to `Screen::Entity(EntityRef::Album(id))` etc. with the real id),
  the screens themselves are a "coming soon" note. The mini-player window and the tray are not
  started at all yet (no second `iced` window is opened this wave).
- **Image cache**: `ui::images::ImageCache`, a hand-rolled insertion-order-bounded map (not a true
  read-touches-recency LRU — `peek`, the only read `view` code calls, deliberately never reorders,
  since `view` only ever holds `&ImageCache`; eviction order is "oldest inserted," which is
  sufficient at this cache's actual access pattern). `ui::images::fetch` runs a plain
  (unauthenticated — TIDAL cover art is unauthenticated, `api/images.rs`) `reqwest::Client` GET
  inside `Task::perform`, decoding via the `image` crate on a `tokio::task::spawn_blocking` thread
  (CPU-bound decode off both the UI thread and the async executor's worker), producing an
  `iced::widget::image::Handle::from_rgba` the update thread only ever moves, never decodes.
- **Keyboard (task item 4)**: `iced::event::listen_with` matched against
  `keyboard::Event::KeyPressed` — space toggles play/pause, escape goes back one step in the nav
  stack, ctrl+f navigates to Search (it does not additionally force text-input focus — no
  `Task`-returning focus helper was found on `iced_widget::text_input` in 0.14.2's public API; see
  the `iced-ui` skill §8). Media keys are explicitly out of scope here (MPRIS, later).
- **Additive core/player changes this wave required**: `Context: Clone` (above);
  `config::{ThemePreference, ReplayGainMode}` plus two new `Settings` fields
  (`theme`, `replay_gain_mode`) and one (`play_reporting_enabled`, default `true` per D-027) — all
  three persisted by the Settings screen; `KeyStorage: Display`; `PkceSession: Debug` (hand-written,
  redacts the verifier — needed because enabling iced's `debug` feature makes `Message: Debug` a
  hard `Application::run` requirement, transitively through every nested screen message);
  `proto::Command::{MoveQueueItem, RemoveQueueItem}` and their `Player::handle_command` arms
  (index-based queue reorder/removal, refusing to remove the currently-playing entry with a
  `Warning` event instead of the ordinary index bookkeeping). None of this changes any existing
  variant's behaviour — every change is a new field, a new trait impl, or a new enum variant with a
  new match arm.

## Offline cache (D-022)

`streamboat-player::offline` (GPL-3.0-only; the file-format and crypto
helpers have no player dependency but stayed in this crate rather than
`streamboat-core`, since the task's own scope named this crate and nothing
in them needs to be Apache-licensed on its own). Guardrails and how each is
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

Left untested (see the task report for the full list): the DASH segment
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
property) — this wave changed only *which numbers* feed it, in
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
  not re-applied at a gapless hand-over — this wave did not change that
  boundary), but a successor already sitting in `engine.set_next()` is
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
  at all — searched defensively anyway per this task's own brief; finding
  nothing under it is expected, not a bug). `mdns-sd` 0.21.3, the version
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
libmpv/FIFO pump path end to end (no test requires it — the task's own test
list scopes the TCP-format test to the GStreamer path), and real-network
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
  config/data/cache/runtime dirs, OS/arch, `gst::version_string()` — gathered
  by the CLI itself, since `streamboat-core` cannot depend on GStreamer,
  D-004; libmpv's version is not included yet, since the desktop crate's own
  `mpv` feature is still a stub, see "Not yet built"), `settings.redacted.json`
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
  `logs/`/`crashes/`. `streamboat_player::probe` (another agent's work) is
  deliberately not depended on; if/when it lands, wiring its decoder-probe
  output into `environment.txt` is a follow-up, not done here.

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
  key resolution against a `MemoryKeySlot`.
- `streamboat-server`: `tests/api.rs` (see "Control API" above) — health,
  auth, Host allowlisting, a command changing state, and both directions of
  the WebSocket, all over real sockets against an in-process daemon.
- `streamboat-desktop`: 37 tests (`cargo test -p streamboat-desktop`), all inline
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
  renders the query/filter row and track results; the Entity placeholder shows
  the right kind/id. Plain `#[test]`s (no simulator) cover the pure view-model
  helpers — `mmss`/`quality_badge`/`bit_perfect_applicable` formatting,
  `feed_section_to_view`/`page_module_to_view`'s known-vs-unknown-type mapping,
  the `Nav` back/forward stack (five cases: push+clear-forward, round-trip,
  no-op on empty history, dropping the stale forward branch after a fresh
  `go_to`, no-op on navigating to the current screen), the `ImageCache`'s
  insert/evict/re-insert behaviour, `Tokens::dark()`/`light()` (distinct
  colours, shared scale), and the `Settings`⇄`settings::State` round trip.
  **Not verified without a display** (this container has none): the actual
  `iced::application(...).run()` event loop, window creation, real mouse/keyboard
  delivery through winit, and anything about visual layout beyond what
  `ui.find("...")` widget-tree assertions can see (no pixel/snapshot tests were
  taken here, though `Simulator::snapshot` exists for a future pass that adds
  them).
- CI: fmt, clippy `-D warnings`, tests, release build; `cargo test -p
  streamboat-player --features mpv` on top of the default (GStreamer) build;
  a Debian container job builds `streamboatd` without GUI libraries and
  asserts none are linked — `axum`, `tokio-tungstenite` and
  `mpris-server`/`zbus` are all pure Rust and link no system D-Bus or GUI
  library, so this still passes. The `check` job also installs
  `libxkbcommon-dev`, `libxkbcommon-x11-dev`, `libwayland-dev`, `libx11-dev`,
  `libxrandr-dev`, `libxi-dev`, `libxcursor-dev` for iced/winit; the
  headless-daemon job is unaffected (it never builds `streamboat-desktop`).
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

## Not yet built (in decision order)

the `streamboat://` handler for the desktop shell (D-024); the Flatpak Secret
portal (D-026); wiring `MpvEngine` into `streamboat`/`streamboatd`'s engine
selection for Windows and macOS (D-016 — the backend itself is built and
tested on Linux, see above; `ui::engine_select` gates the desktop side); the
`org.freedesktop.ReserveDevice1` device-reservation handshake for the ALSA
writer (`output-backends.md` §2, explicitly optional — `EBUSY` on open is
handled with a bounded retry regardless); SMTC/NowPlayingInfoCenter (MPRIS is
done for Linux, D-030); packaging (D-041); the
mini-player window and tray icon (D-036, D-014); entity/Collection/lyrics
screens (D-015; `ui::screens::placeholder` covers routing only); the
control-API-backed remote-client `PlayerLink` and the single-instance lock
(D-010, `RemoteLink`); the `streamboat://` handler registration per OS
(D-024); a Snapcast output toggle in the Settings screen (D-034 — the
`OutputConfig::Snapcast` variant and both engine backends exist, `Cmd`/CLI
plumbing for it does too, but the UI's own output picker still offers only
Shared/Exclusive, falling back to Shared when it round-trips a
`Snapcast` value it did not create); the libmpv/FIFO Snapcast pump on
Windows/macOS (D-034, Unix-only so far — `start_snapcast_pump` returns a
clear error there rather than a silent no-op); a verified round trip
against a real `snapserver`/Snapweb for both the Snapcast output and the
`snapcast-plugin`/`snapcast-discover` subcommands (no snapserver in this
environment); libmpv's own version string in `streamboat debug-bundle`'s
`environment.txt` (D-029 — gated on the desktop crate's `mpv` feature
actually linking `libmpv2`, which it does not yet, see above); wiring
`streamboat_player::probe`'s decoder-probe output into the same bundle,
once that module lands from the other agent building it.
