# Architecture (as built by the playable spike)

Companion to `docs/DECISIONS.md`: what exists in the workspace today and the
implementation choices made while building the first milestone (D-044).

## Crates

| Crate | Licence | Contents |
| --- | --- | --- |
| `streamboat-core` | Apache-2.0 | `http` (authenticated client, single-flight and cross-process refresh, 429 gate), `auth::device_code`, `auth::pkce`, `token_store` (AES-256-GCM file, `SBTK` header, keyring-held master key), `manifest` (BTS/EMU/DASH parsing, refusal rule), `api` (sessions, tracks, `playbackinfopostpaywall`, quality cascade, plus the catalogue/library surface below), `proto` (Command/Event), `config`, `credentials` (device-code and PKCE pairs), `bootstrap` |
| `streamboat-player` | GPL-3.0-only | `engine::Engine` trait, `gst::GstEngine` (playbin3), `player::Player` (queue, prefetch, Command→Event loop) |
| `streamboat-server` | GPL-3.0-only | `streamboatd`: stdio JSON-lines transport for the protocol; headless login as `auth_required`/`auth_ok` events |
| `streamboat-desktop` | GPL-3.0-only | `streamboat`: CLI subcommands (login [--pkce], logout, whoami, search, resolve, play, devices, keyring, paths) unchanged; running with no subcommand now launches the iced shell (`src/ui/`) — see §"Desktop shell (iced)" below |

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
  volume policy, JSON round trip).
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
- CI: fmt, clippy `-D warnings`, tests, release build; a Debian container job
  builds `streamboatd` without GUI libraries and asserts none are linked. The
  `check` job now also installs `libxkbcommon-dev`, `libxkbcommon-x11-dev`,
  `libwayland-dev`, `libx11-dev`, `libxrandr-dev`, `libxi-dev`, `libxcursor-dev`
  for iced/winit; the headless-daemon job is unaffected (it never builds
  `streamboat-desktop`).

## Not yet built (in decision order)

the `streamboat://` handler for the desktop shell (D-024); the Flatpak Secret
portal (D-026); the libmpv backend for Windows and macOS (D-016, gated for in
`ui::engine_select`); the ALSA writer thread with format read-back and the
reopen-on-format-change policy (D-017, D-018); the HTTP + WebSocket control
API and MPRIS adapter (D-030, `ui::player_link::RemoteLink`'s call site);
the streaming-privileges websocket (D-033); play reporting's actual reporting
client (D-027 — the Settings screen's toggle and disclosure text exist and
persist, nothing sends an event yet); the offline cache (D-022); packaging
(D-041); the mini-player window and tray icon (D-036); entity/Collection/
lyrics screens (D-015; `ui::screens::placeholder` covers routing only); the
control-API-backed remote-client `PlayerLink` (D-010, `RemoteLink`).
