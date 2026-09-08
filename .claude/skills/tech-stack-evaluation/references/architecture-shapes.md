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

## §1 Shape 1 — single Rust workspace (core crate + control-API crate + desktop UI crate)

**This is the reconciled layout — adopt this, not the earlier `streamboat-player`-as-its-own-crate
sketch below.** Inlined verbatim from `docs/research/headless-connect.md:1839-1841` §15
("Recommendation: Model 1 implemented on top of Model 3's library split"), which is the more recently
reconciled of the two crate layouts this research produced:

```
streamboat/
  crates/
    streamboat-core/      # session/auth (OAuth device-code + PKCE), catalogue, queue, player engine
                           # (the AudioEngine trait — §0 of audio-engine-comparison.md — and its
                           # backends), output backends incl. a pipe/fd target, Pushkin. No UI, no
                           # server.
    streamboat-server/    # control API (HTTP+WS), MPRIS, mDNS, optional MPD listener. Depends on core.
    streamboat-proto/     # command/event types, serde, shared by server + clients (single source of truth)
    streamboat-cli/       # TUI / one-shot client over the protocol
  desktop/
    src-tauri/            # thin Tauri shell: window, tray, deep links, global shortcuts
    src/                  # React frontend
```

One shipped binary carries the user-facing command surface (`streamboat`, `streamboat daemon`,
`streamboat play`, `streamboat service install`) via subcommands, built from two artifacts per
`docs/research/headless-connect.md` §15's own reconciliation with the daemon-build-isolation
requirement in `docs/research/tech-stack.md:816-817`: a GUI binary that links the webview, and a
`streamboatd`-shaped daemon binary that must not — enforce with a workspace dependency graph and a CI
job that builds the daemon crate in a container with no GTK/WebKit installed.

- **State sync**: none needed in the desktop default path — the Tauri shell links `streamboat-core`
  directly and dispatches the same `Command` enum in-process. No IPC latency, no hot-path
  serialization.
- **Latency**: Tauri `invoke` round-trips are sub-millisecond for small payloads; the only
  latency-sensitive thing in a music player is the play/pause key, fine either way.
- **Offline**: metadata cache in SQLite inside `streamboat-core`; both shells get it for free.
- **Complexity**: lowest of the three. One language, one build, `cargo test` covers the engine.
- **Weakness**: the UI is coupled to Rust. A future Flutter or Compose mobile UI needs Shape 2's
  binding layer added on top — additive, not a rewrite, provided the core stays UI-agnostic.

**Superseded (kept as a one-line note, not a competing option):** an earlier draft of this file split
the player engine into its own `streamboat-player` crate alongside `streamboat-core`,
`streamboat-proto`, a `streamboat-daemon` crate and `streamboat-cli`. The layout above replaces it.

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
  subscription. Needs a reconnect story, a versioned protocol, an auth story for the socket (Sone
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
and expensive to retrofit after ~100 component files exist — decide whether `streamboat-server`
(binary `streamboatd`) serves a browser UI or is MPRIS-bridge-only *before* writing the first
component (see Open decision #10 in `SKILL.md`).

## §4a TIDAL's account-wide streaming-privileges constraint (Pushkin) — not just device-local

**Gap identified during fact-checking; this was missing from the original research pass entirely.**
The device-ownership rule in §3/§4 above (first process to claim the sound device/D-Bus name wins,
later ones become clients) is a **local** rule. TIDAL enforces a separate, **account-wide** rule: only
one "privileged" (playing) session per account, enforced over a websocket TIDAL's own SDKs call
internally "Pushkin." A headless daemon on a Pi in the living room and a laptop GUI **on the same
account** will revoke each other's playback even though they hold different sound cards on different
machines — device ownership alone does not model this, and this report's original hybrid design does
not either.

**Protocol shape**: owned canonically by
`headless-and-tidal-connect/references/daemon-architecture.md` §6, including the reconnect/backoff
and token-rebinding analysis this file does not cover — cite it rather than restating the message
vocabulary. Headline facts worth keeping in mind here: `POST {legacyApiUrl}/rt/connect` returns the
websocket URL, the client sends `USER_ACTION` and receives `PRIVILEGED_SESSION_NOTIFICATION`/
`RECONNECT`, and on the REST side `playbackinfo` `subStatus` **4006** ("streaming privileges lost")
is the non-terminal signal a client discovers this from if it isn't watching the websocket — see
`tidal-api/references/transport.md` §6 for the canonical terminal/non-terminal sub-status table
(the literal set, not a range).

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

**Related, unresolved race — gap identified during fact-checking: multi-process OAuth token
refresh.** The recommended hybrid (§4) explicitly allows a daemon and a GUI on the same machine and
account to coexist ("try to claim the device/D-Bus name; if already claimed, become a client"), but
the OAuth token is shared state between those two processes: in TIDAL's common refresh flow,
whichever process refreshes first invalidates the other's refresh token — independently of the
Pushkin account-privilege problem above. `docs/research/engineering-baseline.md` already flags "multi-process token
races" as open. `docs/research/headless-connect.md` names a concrete reference implementation: `tidalrs` exposes an
`on_authz_refresh_callback` token-refresh hook, cited there as "a reference implementation for
`streamboat-core`'s refresh loop and the multi-process token-refresh notification problem"
(`ref:tidalrs/src/lib.rs:271-281,321,401`). The stack-level decision: either only the device-owning
process ever refreshes and pushes the new token to clients over the control protocol (making it a
`streamboat-proto` message, not just a keyring concern), or refresh is serialised behind a file lock.
Decide this alongside item 3 above — both are protocol design, not implementation detail.

**Unresolved**: whether the unofficial `api.tidal.com` surface used by Sone/High Tide/python-tidal
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

**Headless login/pairing UX is out of scope for this file by design — read `docs/research/headless-connect.md` §13
alongside it.** This report's own login content (`tauri-plugin-oauth` loopback listener + a `tidal://`
deep link, see `tauri-engineering-facts.md` §2) is desktop-shaped; "headless now" means the daemon
needs its own answer on day one. `docs/research/headless-connect.md` §13 documents terminal+QR device-code login
(`ref:tidalt/internal/tidal/loginprint.go`), a local web-page PKCE flow
(`ref:mopidy-tidal/mopidy_tidal/web_auth_server.py` on port 8989), pre-provisioned tokens via env
vars, and an HTTP-exposed device-auth code (`go-librespot`'s `GET /auth/code`) — plus the
keyring-with-encrypted-file-fallback pattern both `tidalt` and Sone use, which matters more on a
headless box with no unlocked Secret Service. Concrete consequence: `axum` belongs in the daemon
crate from day one for login, not only for the control API.

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

**ARM is treated as a daemon-only concern above; desktop ARM targets are never named — gap identified
during fact-checking.** Every `arm64`/`aarch64` mention in this file is inside the Pi-daemon
paragraph. Desktop has ARM questions too: `aarch64-pc-windows-msvc` (WebView2 ships on Windows on
ARM), arm64 Linux desktop (Asahi, Pi desktop builds, ARM laptops), and the macOS universal-binary
question (`tauri-engineering-facts.md`). Since this report establishes Tauri cannot cross-compile and
needs three separate release pipelines already, each additional architecture multiplies that number —
directly affecting the MVP effort estimate. Not researched further here; the decision to make
explicit is a v1 target-triple list across *all* platforms in one place, with the honest default
being x86_64 Linux + one of x86_64/arm64 Windows (not both) + macOS universal-or-Apple-Silicon-only,
and arm64 Linux limited to the headless daemon package.

## §7 The audio test harness problem

CI runners have no sound card, and **no reference project tests its audio engine in-process** except
partially:

- Sone's `audio.rs` (3,309 lines, the hardest code in the project) has **zero** `#[cfg(test)]`
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
