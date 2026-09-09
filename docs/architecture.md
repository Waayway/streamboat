# Architecture (as built by the playable spike)

Companion to `docs/DECISIONS.md`: what exists in the workspace today and the
implementation choices made while building the first milestone (D-044).

## Crates

| Crate | Licence | Contents |
| --- | --- | --- |
| `streamboat-core` | Apache-2.0 | `http` (authenticated client, single-flight refresh, 429 gate), `auth::device_code`, `token_store` (AES-256-GCM file, `SBTK` header), `manifest` (BTS/EMU/DASH parsing, refusal rule), `api` (sessions, tracks, search, `playbackinfopostpaywall`, quality cascade), `proto` (Command/Event), `config`, `credentials`, `bootstrap` |
| `streamboat-player` | GPL-3.0-only | `engine::Engine` trait, `gst::GstEngine` (playbin3), `player::Player` (queue, prefetch, Command→Event loop) |
| `streamboat-server` | GPL-3.0-only | `streamboatd`: stdio JSON-lines transport for the protocol; headless login as `auth_required`/`auth_ok` events |
| `streamboat-desktop` | GPL-3.0-only | `streamboat`: CLI subcommands (login, logout, whoami, search, resolve, play, devices, paths); the iced shell is not built yet |

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
- **Device identity**: `device.json` holds the `clientUniqueKey` generated
  once at first run (not yet sent: it is used by the PKCE flow, which is next).
- **Protocol**: `proto::PROTOCOL_VERSION = 1`; JSON with `"type"` tags in
  snake_case; additive changes only.

## Environment variables

| Variable | Effect |
| --- | --- |
| `STREAMBOAT_HOME` | Put config, data, cache and runtime dirs under one directory |
| `STREAMBOAT_CLIENT_ID`, `STREAMBOAT_CLIENT_SECRET` | Client credentials at run time (or at build time to embed) |
| `STREAMBOAT_GST_SINK` | Replace the audio sink with any element description (`fakesink` in CI) |
| `STREAMBOAT_GST_PLAYBIN` | `playbin` instead of `playbin3` |
| `RUST_LOG` | Log filter (`info` prints the signal path on track start) |

## Tests

- `streamboat-core`: unit tests for the token store (including truncation at
  every byte offset), manifest parsing and refusal, error-body shapes; wiremock
  transport tests for the device-code flow, refresh semantics, the 429 gate,
  playback sub-statuses and the cascade.
- `streamboat-player`: the GStreamer backend over generated WAV files through
  `fakesink` (single track, gapless hand-over, error paths); the Player against
  a fake engine and an in-process TIDAL (queue, skip-with-event, exclusive
  volume policy, JSON round trip).
- CI: fmt, clippy `-D warnings`, tests, release build; a Debian container job
  builds `streamboatd` without GUI libraries and asserts none are linked.

## Not yet built (in decision order)

PKCE login and the `streamboat://` redirect (D-024); OS-keyring token storage
(D-026); the libmpv backend for Windows and macOS (D-016); the ALSA writer
thread with format read-back and the reopen-on-format-change policy (D-017,
D-018); the HTTP + WebSocket control API and MPRIS adapter (D-030); the
streaming-privileges websocket (D-033); the iced shell (D-013); play reporting
(D-027); the offline cache (D-022); packaging (D-041).
