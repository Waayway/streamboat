# Architecture (as built by the playable spike)

Companion to `docs/DECISIONS.md`: what exists in the workspace today and the
implementation choices made while building the first milestone (D-044).

## Crates

| Crate | Licence | Contents |
| --- | --- | --- |
| `streamboat-core` | Apache-2.0 | `http` (authenticated client, single-flight and cross-process refresh, 429 gate), `auth::device_code`, `auth::pkce`, `token_store` (AES-256-GCM file, `SBTK` header, keyring-held master key), `manifest` (BTS/EMU/DASH parsing, refusal rule), `api` (sessions, tracks, `playbackinfopostpaywall`, quality cascade, plus the catalogue/library surface below), `proto` (Command/Event), `config` (`AppDirs`, `Settings`, the control API's bearer-token file), `credentials` (device-code and PKCE pairs), `bootstrap` |
| `streamboat-player` | GPL-3.0-only | `engine::Engine` trait, `gst::GstEngine` (playbin3), `player::Player` (queue, prefetch, Command→Event loop, `PlayerHandle::publish` for externally-sourced events), `mpris` (Linux, feature `mpris`, default on: `org.mpris.MediaPlayer2.streamboat`) |
| `streamboat-server` | GPL-3.0-only | `streamboatd`: `api` (the HTTP + WebSocket control API, see below) hosted by default; `--stdio` keeps the original JSON-lines transport; headless login as `auth_required`/`auth_ok` events on the same broadcast every front end reads |
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
- **Exclusive mode**: `alsasink device=hw:X,Y` with playbin's `native-audio`
  flag (no conversion/resample/soft-volume elements). An unsupported format
  fails negotiation loudly rather than being resampled. The hand-written
  ALSA writer with `hw_params` read-back (Sone's design) is the next step.
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

## Environment variables

| Variable | Effect |
| --- | --- |
| `STREAMBOAT_HOME` | Put config, data, cache and runtime dirs under one directory |
| `STREAMBOAT_CLIENT_ID`, `STREAMBOAT_CLIENT_SECRET` | Device-code client pair at run time (or at build time to embed) |
| `STREAMBOAT_PKCE_CLIENT_ID`, `STREAMBOAT_PKCE_CLIENT_SECRET` | PKCE client pair (hi-res), same precedence |
| `STREAMBOAT_MASTER_KEY` | 32-byte token-file key as hex or base64 (headless boxes) |
| `STREAMBOAT_GST_SINK` | Replace the audio sink with any element description (`fakesink` in CI) |
| `STREAMBOAT_GST_PLAYBIN` | `playbin` instead of `playbin3` |
| `RUST_LOG` | Log filter (`info` prints the signal path on track start) |

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
  fetch).
- `streamboat-player`: the GStreamer backend over generated WAV files through
  `fakesink` (single track, gapless hand-over, error paths); the Player against
  a fake engine and an in-process TIDAL (queue, skip-with-event, exclusive
  volume policy, JSON round trip); `mpris` unit tests cover pure mapping only
  (`TrackSummary` → `Metadata`, `PlaybackStatus` mapping, the exclusive-mode
  volume rule) — CI has no D-Bus session bus, so registering a real `Player`
  is not exercised there.
- `streamboat-server`: `tests/api.rs` (see "Control API" above) — health,
  auth, Host allowlisting, a command changing state, and both directions of
  the WebSocket, all over real sockets against an in-process daemon.
- CI: fmt, clippy `-D warnings`, tests, release build; a Debian container job
  builds `streamboatd` without GUI libraries and asserts none are linked —
  `axum`, `tokio-tungstenite` and `mpris-server`/`zbus` are all pure Rust and
  link no system D-Bus or GUI library, so this still passes.

## Not yet built (in decision order)

the `streamboat://` handler for the desktop shell (D-024); the Flatpak Secret
portal (D-026); the libmpv backend for Windows and macOS (D-016); the ALSA writer
thread with format read-back and the reopen-on-format-change policy (D-017,
D-018); SMTC/NowPlayingInfoCenter (MPRIS is done for Linux, D-030); the
streaming-privileges websocket (D-033); the iced shell (D-013); play reporting
(D-027); the offline cache (D-022); packaging (D-041).
