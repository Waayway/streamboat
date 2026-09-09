# Architecture (as built by the playable spike)

Companion to `docs/DECISIONS.md`: what exists in the workspace today and the
implementation choices made while building the first milestone (D-044).

## Crates

| Crate | Licence | Contents |
| --- | --- | --- |
| `streamboat-core` | Apache-2.0 | `http` (authenticated client, single-flight and cross-process refresh, 429 gate), `auth::device_code`, `auth::pkce`, `token_store` (AES-256-GCM file, `SBTK` header, keyring-held master key), `manifest` (BTS/EMU/DASH parsing, refusal rule), `api` (sessions, tracks, `playbackinfopostpaywall`, quality cascade, plus the catalogue/library surface below), `proto` (Command/Event), `config`, `credentials` (device-code and PKCE pairs), `bootstrap` |
| `streamboat-player` | GPL-3.0-only | `engine::Engine` trait, `gst::GstEngine` (playbin3, default feature), `mpv::MpvEngine` (libmpv2, `mpv` feature — D-016), `player::Player` (queue, prefetch, Command→Event loop) |
| `streamboat-server` | GPL-3.0-only | `streamboatd`: stdio JSON-lines transport for the protocol; headless login as `auth_required`/`auth_ok` events |
| `streamboat-desktop` | GPL-3.0-only | `streamboat`: CLI subcommands (login [--pkce], logout, whoami, search, resolve, play, devices, keyring, paths); the iced shell is not built yet |

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

## Environment variables

| Variable | Effect |
| --- | --- |
| `STREAMBOAT_HOME` | Put config, data, cache and runtime dirs under one directory |
| `STREAMBOAT_CLIENT_ID`, `STREAMBOAT_CLIENT_SECRET` | Device-code client pair at run time (or at build time to embed) |
| `STREAMBOAT_PKCE_CLIENT_ID`, `STREAMBOAT_PKCE_CLIENT_SECRET` | PKCE client pair (hi-res), same precedence |
| `STREAMBOAT_MASTER_KEY` | 32-byte token-file key as hex or base64 (headless boxes) |
| `STREAMBOAT_GST_SINK` | Replace the audio sink with any element description (`fakesink` in CI) |
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
  fetch).
- `streamboat-player`: the GStreamer backend over generated WAV files through
  `fakesink` (single track, gapless hand-over, error paths); the libmpv
  backend (`mpv` feature) over hand-written PCM WAV files through `ao=null`
  (single track, gapless hand-over via `loadfile append`, an unreadable
  source, a bogus DASH manifest materialized as a real temp file) — tested
  on Linux only; the Windows/macOS-specific AO option values (§ above) are
  cfg-gated but unexercised here, no CI runner for either yet; the Player
  against a fake engine and an in-process TIDAL (queue, skip-with-event,
  exclusive volume policy, JSON round trip).
- CI: fmt, clippy `-D warnings`, tests, release build; `cargo test -p
  streamboat-player --features mpv` on top of the default (GStreamer) build;
  a Debian container job builds `streamboatd` without GUI libraries and
  asserts none are linked.

## Not yet built (in decision order)

the `streamboat://` handler for the desktop shell (D-024); the Flatpak Secret
portal (D-026); wiring `MpvEngine` into `streamboat`/`streamboatd`'s engine
selection for Windows and macOS (D-016 — the backend itself is built and
tested on Linux, see above); the ALSA writer thread with format read-back
and the reopen-on-format-change policy (D-017, D-018); the HTTP + WebSocket
control API and MPRIS adapter (D-030); the
streaming-privileges websocket (D-033); the iced shell (D-013); play reporting
(D-027); the offline cache (D-022); packaging (D-041).
