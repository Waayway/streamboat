# Architecture (as built by the playable spike)

Companion to `docs/DECISIONS.md`: what exists in the workspace today and the
implementation choices made while building the first milestone (D-044).

## Crates

| Crate | Licence | Contents |
| --- | --- | --- |
| `streamboat-core` | Apache-2.0 | `http` (authenticated client, single-flight and cross-process refresh, 429 gate), `auth::device_code`, `auth::pkce`, `token_store` (AES-256-GCM file, `SBTK` header, keyring-held master key), `manifest` (BTS/EMU/DASH parsing, refusal rule), `api` (sessions, tracks, `playbackinfopostpaywall`, quality cascade, plus the catalogue/library surface below), `proto` (Command/Event), `config`, `credentials` (device-code and PKCE pairs), `bootstrap`, `privileges` (the Pushkin streaming-privileges websocket, D-033), `reporting` (play reporting to `ec.tidal.com` and the server-anchored clock, D-027), `scrobble` (Last.fm/ListenBrainz, D-037) |
| `streamboat-player` | GPL-3.0-only | `engine::Engine` trait, `gst::GstEngine` (playbin3), `player::Player` (queue, prefetch, Command→Event loop, and the optional `PlayerDeps` wiring for the three modules above plus `offline`), `offline::OfflineCache` (the pinned, encrypted offline cache, D-022) |
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
  `fakesink` (single track, gapless hand-over, error paths); the Player against
  a fake engine and an in-process TIDAL (queue, skip-with-event, exclusive
  volume policy, JSON round trip); `tests/offline.rs` covers the offline
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
- CI: fmt, clippy `-D warnings`, tests, release build; a Debian container job
  builds `streamboatd` without GUI libraries and asserts none are linked.

## Not yet built (in decision order)

the `streamboat://` handler for the desktop shell (D-024); the Flatpak Secret
portal (D-026); the libmpv backend for Windows and macOS (D-016); the ALSA writer
thread with format read-back and the reopen-on-format-change policy (D-017,
D-018); the HTTP + WebSocket control API and MPRIS adapter (D-030);
streaming privileges, play reporting and scrobbling wired into the
`streamboat` CLI/desktop shell rather than only `streamboatd` (D-033, D-027,
D-037 — the modules and the daemon wiring exist; see the section above); the
iced shell (D-013); packaging (D-041). The offline cache (D-022) is built —
see the section above — except for `streamboatd` having no `logout` command
yet to hook its cache wipe into.
