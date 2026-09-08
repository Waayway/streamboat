# Architecture shapes: how the pieces are wired together

This file is about **topology** — where the audio engine lives relative to the UI and the daemon,
and how control commands and state flow between them. For the headless control-protocol *design*
question in depth (MPD-compatible vs. bespoke vs. MPRIS-first) and TIDAL Connect, see the
`headless-and-tidal-connect` skill; this file stays at the level tech-stack.md needs: which shape to
pick and why.

## Contents

- §1 Shape 1 — single Rust workspace (recommended)
- §2 Shape 2 — core as a library with FFI/UniFFI bindings
- §3 Shape 3 — everything talks to a local daemon over WebSocket
- §4 The recommended hybrid
- §4a TIDAL's account-wide streaming-privileges constraint (Pushkin) — not just device-local
- §4b A remote GUI is control-only; it does not carry audio
- §5 MPRIS/SMTC: register from the engine, not the GUI
- §6 Headless beyond Linux/systemd: containers, macOS, Windows, and arm64/Raspberry Pi
- §7 The audio test harness problem

---

## §1 Shape 1 — single Rust workspace (core crate + desktop UI crate + daemon binary)

```
streamboat/
  crates/
    streamboat-core/      # HTTP client, OAuth (device code + PKCE), models, catalog, library, cache
    streamboat-player/    # AudioEngine trait, gstreamer backend, queue + state machine, ReplayGain
    streamboat-proto/     # command/event types, serde, shared by daemon + clients (single source of truth)
    streamboat-daemon/    # headless binary: WebSocket server, MPRIS, systemd unit, config
    streamboat-cli/       # TUI / one-shot client over the protocol
  desktop/
    src-tauri/            # thin Tauri shell: window, tray, deep links, global shortcuts
    src/                  # React frontend
```

- **State sync**: none needed in the desktop default path — the Tauri shell links `streamboat-player`
  directly and dispatches the same `Command` enum in-process. No IPC latency, no hot-path
  serialization.
- **Latency**: Tauri `invoke` round-trips are sub-millisecond for small payloads; the only
  latency-sensitive thing in a music player is the play/pause key, fine either way.
- **Offline**: metadata cache in SQLite inside `streamboat-core`; both shells get it for free.
- **Complexity**: lowest of the three. One language, one build, `cargo test` covers the engine.
- **Weakness**: the UI is coupled to Rust. A future Flutter or Compose mobile UI needs Shape 2's
  binding layer added on top — additive, not a rewrite, provided the core stays UI-agnostic.

## §2 Shape 2 — core as a library with FFI/UniFFI bindings to a non-Rust UI

UniFFI 0.32.0 (2026-06-30, MPL-2.0, ~11.9M downloads) generates Kotlin, Swift, Python and Ruby
bindings from a Rust crate; Mozilla uses it in production but its own docs say "We consider it ready
for production use, but UniFFI is a long way from a 1.0 release" — treat Swift 6 support as partial,
unconfirmed in this pass.

- **When it pays**: the moment you want a SwiftUI iOS app and a Compose Android app sharing the
  streamboat core. The core compiles to a `.xcframework` and an `.aar`; each platform writes its own
  UI and its own audio output.
- **Cost**: the FFI boundary must be designed — no `&mut` graph handles, no Rust enums with payloads
  crossing casually, async needs care. Expect a facade crate whose only job is to be FFI-shaped.
- **Verdict**: adopt this *later*, additively, when mobile becomes real. Do not pay for it now. But
  **do** keep the core free of Tauri/GTK/toolkit types today so the facade is cheap to add.

## §3 Shape 3 — everything talks to a local daemon over WebSocket ("the GUI is just another remote")

Precedents: Music Assistant exposes a unified command-based API over WebSocket on port 8095
(`/ws`, auth then routed commands) with a partial JSON-RPC-over-HTTP projection at `/api` — its
client package's "split out in October 2024" origin date is unverified, neither README states one.
tidalt uses D-Bus instead of WebSocket but the same topology (one process owns the device, everyone
else is a client).

- **State sync**: daemon owns authoritative `PlayerState`, pushes typed events, clients send
  commands. Send position as a `(position, wall_clock, rate)` triple and let clients interpolate — do
  not stream a position tick at UI framerate.
- **Latency**: local loopback WebSocket is sub-millisecond, irrelevant in practice.
- **Complexity**: highest. Every UI action becomes a protocol message; every state read becomes a
  subscription. Needs a reconnect story, a versioned protocol, an auth story for the socket (SONE
  gates its MCP server on a generated token — copy that).
- **Offline**: the desktop app is useless without the daemon running — a real UX regression for the
  95% single-machine case.
- **Where it wins**: remote control from a phone or another machine, multi-client, and the headless
  server story — which streamboat needs anyway.

## §4 The recommended hybrid

Define the protocol once in `streamboat-proto`. Give the desktop app **two transports for the same
protocol**: an in-process transport (default; the shell owns the engine) and a WebSocket transport
(when `--connect ws://host:port` is passed, or a local daemon already holds the device). Follow
tidalt: on startup, try to claim the device/D-Bus name; if already claimed, become a client. This
gets Shape 1's simplicity for the common case and Shape 3's reach without a second codebase.

**Frontend constraint this implies but is easy to miss**: if the React bundle must also run in a
plain browser (served by the daemon as a web remote — the cheapest possible answer to "mobile
later", years before a Tauri/Flutter mobile app), then no component may call `invoke` directly. Every
call must go through a `Transport` interface picked at bootstrap. This is cheap to enforce on day one
and expensive to retrofit after ~100 component files exist — decide whether `streamboat-daemon` serves
a browser UI or is MPRIS-bridge-only *before* writing the first component (see Open decision #10 in
`SKILL.md`).

## §4a TIDAL's account-wide streaming-privileges constraint (Pushkin) — not just device-local

**Gap identified during fact-checking; this was missing from the original research pass entirely.**
The device-ownership rule in §3/§4 above (first process to claim the sound device/D-Bus name wins,
later ones become clients) is a **local** rule. TIDAL enforces a separate, **account-wide** rule: only
one "privileged" (playing) session per account, enforced over a websocket TIDAL's own SDKs call
internally "Pushkin." A headless daemon on a Pi in the living room and a laptop GUI **on the same
account** will revoke each other's playback even though they hold different sound cards on different
machines — device ownership alone does not model this, and this report's original hybrid design does
not either.

**Protocol shape**, read directly from the official TIDAL SDKs:

- `POST {legacyApiUrl}/rt/connect` with `Authorization: Bearer <accessToken>` returns the websocket
  URL (`ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts`).
- The client sends `USER_ACTION` with a `{startedAt}` payload when the user starts playback.
- The client receives `PRIVILEGED_SESSION_NOTIFICATION` (payload: `clientDisplayName`, `sessionId`,
  `endsAt {clientTime, serverTime}`, `updatedAt`) and `RECONNECT`.
- The web SDK surfaces this as a `streaming-privileges-revoked` CustomEvent whose detail names the
  other device (`ref:tidal-sdk-web/packages/player/src/api/event/streaming-privileges-revoked.ts`).
  The same module exists in the Android SDK
  (`ref:tidal-sdk-android/player/streaming-privileges/`) and iOS SDK
  (`ref:tidal-sdk-ios/Sources/Player/PlaybackEngine/Internal/StreamingPrivileges/StreamingPrivilegesHandler.swift`).
- On the REST side, SONE classifies `playbackinfo` `subStatus` **4006** as "streaming privileges
  lost" and deliberately excludes it from its terminal-error list (terminal: 4005, 4010,
  4030-4032, 4034, 4035 — `ref:sone/src-tauri/src/tidal_api.rs:15-18,6755`): it recovers and must not
  delete the track, and a `4xxx` sub-status on a 401 is never fixed by a token refresh.

**Consequences for streamboat's design:**

1. The device-ownership rule needs an account-level counterpart, not only a device-level one.
2. `streamboat-proto` needs a `PlaybackRevoked { by_device_name }` event and the UI needs a "playing
   on another device" state — otherwise a revocation looks like an unexplained silent stop
   (`subStatus` 4006 with nothing surfaced).
3. **Decide whether streamboat implements the Pushkin websocket at all** — declining it means silent
   4006s instead of a clean cross-device takeover UX; implementing it is a new networked subsystem
   with its own reconnect story, layered on top of the daemon/GUI protocol above.
4. If the GUI defaults to an in-process engine (Recommendation 1's default path), taking over
   playback from an already-running daemon **on the same account** is a user-visible handoff, not a
   transparent one — the daemon's session gets revoked, audibly, the moment the GUI starts playing.

**Unresolved**: whether the unofficial `api.tidal.com` surface used by SONE/High Tide/python-tidal
exposes this websocket in the same shape as the official SDKs, or something different — confirm
against a live account before implementing. See `tidal-api`/`tidal-oss-landscape` skills for the
unofficial-API side of this.

## §4b A remote GUI is control-only; it does not carry audio

**Gap identified during fact-checking; not stated explicitly in the original hybrid design.** When
the desktop shell attaches to a remote daemon over the WebSocket transport, playback happens on the
**daemon's** machine, through the **daemon's** DAC — the GUI is a remote control with no local sound,
exactly like a phone controlling a living-room Pi. State this invariant explicitly: the device
picker, the quality/bit-perfect badge, and the signal-path panel must all be scoped to whichever
machine is actually producing sound, never to the machine running the GUI. Scope
multiroom/streamed-back audio (a daemon streaming its output back to the GUI's machine,
Snapcast-style) as a distinct, later feature — it is not implied by "attach to a remote daemon" and
was not evaluated in this research pass.

## §5 MPRIS/SMTC: register from the engine, not the GUI

- **MPRIS** on Linux (`org.mpris.MediaPlayer2.streamboat`) — works headless, drives `playerctl`,
  media keys, desktop widgets. tidalt is explicit: "A plain `tidalt` TUI session does not register a
  persistent MPRIS2 service" — so register it from the engine/daemon, not the GUI process, or media
  keys silently stop working the moment the window closes.
- **SMTC** on Windows, **MPNowPlayingInfoCenter** on macOS, via `souvlaki` 0.8.3 (MIT). Caveat from
  souvlaki's own README: **Windows requires an HWND**; macOS "requires an AppDelegate/winit event
  loop (an open window is not required)". **A truly headless Windows daemon therefore gets no SMTC**
  — document that as the answer rather than trying to fake an HWND.

## §6 Headless beyond Linux/systemd: containers, macOS, Windows

"Headless/server/CLI now" means the first thing a server user does is run it in Docker on a NAS or a
Pi. tidalt's Docker recipe generalises directly (`ref:tidalt/docs/docker.md`):

- Expose sound devices with `--device /dev/snd` — this also makes `/proc/asound` readable, which
  tidalt uses for device discovery.
- Join the host audio group with `--group-add $(getent group audio | cut -d: -f3)`, because
  `/dev/snd` nodes are `root:audio`.
- Persist the config dir (OAuth session) and data dir (device preference, volume, metadata cache) as
  volumes.
- Optionally forward `-v /run/user/$(id -u)/bus:/run/user/1000/bus` so the PipeWire/WirePlumber
  `org.freedesktop.ReserveDevice1` handoff still works inside the container.
- Build for `linux/amd64` and `linux/arm64`.

**No reference project answers macOS or Windows headless.** macOS needs a launchd plist + CoreAudio
access from a non-GUI process — unsolved anywhere in this survey. Windows needs a service, and per
§5, gets no SMTC — the honest scope statement is "no now-playing integration in headless Windows
mode," not a promise to eventually fake an HWND.

**The daemon's own arm64/Raspberry Pi release pipeline is unresolved beyond that one Docker note —
gap identified during fact-checking.** tidalt's `linux/arm64` image is the only arm64 precedent in
the survey, and it sidesteps cross-compilation entirely by building inside per-arch container images
(`ref:tidalt/docs/docker.md`) — it says nothing about target triples, glibc floor, or whether a
non-container Pi OS package (DietPi, Armbian, Raspberry Pi OS) is in scope. This is compounded by the
GStreamer engine choice: cross-compiling a project that links `libgstreamer`/`libglib` against an
arm64 sysroot (via `cross`, `docker buildx`+qemu, or a native arm64 runner) is materially harder than
cross-compiling a pure-Rust binary — one more reason to keep `symphonia`/libmpv live behind the
`AudioEngine` trait specifically for the arm64 daemon target, not only for macOS/Windows. State the
target-triple list (`aarch64-unknown-linux-gnu` at minimum, `armv7-unknown-linux-gnueabihf` for
32-bit Pi OS) and the build mechanism explicitly before the first Pi release.

## §7 The audio test harness problem

CI runners have no sound card, and **no reference project tests its audio engine in-process** except
partially:

- SONE's `audio.rs` (3,309 lines, the hardest code in the project) has **zero** `#[cfg(test)]`
  modules; its only audio-specific test artefact is a 21-line `src-tauri/tests/gapless_probe.py`; its
  only dev-dependency is `tempfile = "3"` (`ref:sone/src-tauri/Cargo.toml`).
- tidalt is the partial exception: `internal/player/alsa_fallback_test.go` (68 lines) covers the
  `plughw:` fallback decision specifically.

Design consequence: shape the `AudioEngine` trait to admit a headless backend from day one — a
GStreamer `fakesink`/`appsink` capturing PCM, or an ALSA `null` device/`snd-dummy` module — so the
queue state machine, format negotiation table, and quality cascade are testable with `cargo test`,
and a golden-PCM comparison of a decoded FLAC is possible without hardware. This is tagged
**[STACK]** in `docs/research/engineering-baseline.md:1405` ("how to run the pipeline into a null
sink in CI; whether golden-PCM comparison is feasible") — this file is where that question is
answered.
