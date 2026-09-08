# OS media integration and headless/Raspberry Pi architecture

Full narrative: `docs/research/audio-pipeline.md` §6, plus fact-check gap-fill in its §10.1 (the
big Raspberry Pi / headless section) and the souvlaki MSRV finding in §6's rewrite.

## Table of contents

1. Now-playing / transport controls (MPRIS, SMTC, NowPlayingInfoCenter, souvlaki)
2. Idle inhibit
3. Hot-plug and device enumeration
4. Headless / Raspberry Pi: the client/server architecture question, answered

---

## 1. Now-playing / transport controls (MPRIS, SMTC, NowPlayingInfoCenter, souvlaki)

| Concern | Linux | Windows | macOS |
|---|---|---|---|
| Transport | MPRIS2 D-Bus (`org.mpris.MediaPlayer2`, `.Player`) | SMTC (`SystemMediaTransportControls`) | `MPNowPlayingInfoCenter` + `MPRemoteCommandCenter` |
| Cross-platform crate | `souvlaki` 0.8.3 or `mpris-server` 0.10.0 | `souvlaki` | `souvlaki` |
| Device enumeration | GStreamer `DeviceMonitor`, `device.api=="alsa"`, `api.alsa.path` or `alsa.card`+`alsa.device` -> `hw:C,D` | same monitor, `device.api ∈ {wasapi, wasapi2}`, `device.id` | `gstosxaudiodeviceprovider` |

**MPRIS.** Sone runs `mpris-server` 0.9 on a dedicated thread with a current-thread tokio runtime
and a `LocalSet`, driven by an `MprisCommand` enum covering metadata, playback status, volume,
seeked, shuffle, loop status, fullscreen (`ref:sone/src-tauri/src/mpris.rs`). High Tide hand-writes
the D-Bus interface XML, implementing `Raise, Quit, Next, Previous, PlayPause, Play, Pause, Stop,
Seek, SetPosition, Get, GetAll, Set, PropertiesChanged, Introspect` (`ref:high-tide/src/mpris.py`).
tidalt runs an MPRIS2 server *plus* a private `io.tidalt.App` interface for client<->daemon
communication (`ref:tidalt/docs/architecture.md`) — the pattern to copy for headless mode with a
separate CLI/TUI (see §4).

Under Snap, the MPRIS bus name must be dotless — Sone declares an `mpris` slot and uses the name
`sone` under snap (`ref:sone/snap/snapcraft.yaml:71-76`).

**`souvlaki` 0.8.3** covers Linux/Windows/macOS behind one `MediaControls` API — Linux offers both a
`dbus-crossroads` backend (default, more stable) and a `zbus` backend (pure Rust). Two caveats the
crates.io listing hides:

- **MSRV contradiction.** crates.io declares `rust_version = 1.67`, but souvlaki's own
  `Cargo.toml` (github.com/Sinono3/souvlaki, tag 0.8.3) declares `edition = "2024"`, which needs
  Rust >= 1.85. **Pin a Rust toolchain >= 1.85 for any build including souvlaki, regardless of what
  crates.io reports.**
- **macOS needs an event loop.** souvlaki's own README states it "requires an AppDelegate/winit
  event loop" on macOS — a headless-only macOS build (no event loop) will not get working
  `MPNowPlayingInfoCenter` integration from souvlaki.

`[unverified]`: whether souvlaki (last published 2025-06-24) is still actively maintained.

## 2. Idle inhibit

Sone's `idle_inhibit` module is the most complete implementation in the reference set: it detects
the display server from `WAYLAND_DISPLAY`/`DISPLAY` (Wayland wins under Xwayland) and runs **every
applicable layer additively** — Wayland `zwp_idle_inhibit`, X11 screensaver + DPMS, D-Bus
(`org.freedesktop.ScreenSaver`, GNOME, `login1`), and the XDG portal as a last resort
(`ref:sone/src-tauri/src/idle_inhibit/mod.rs`). Copy the "run every layer" structure — do not pick
just one and hope it covers every desktop environment.

## 3. Hot-plug and device enumeration

No reference project implements true device hot-plug re-binding. Sone's ALSA writer detects
`ENODEV` and emits `audio-error {kind: "device_disconnected"}`, tearing the pipeline down.
GStreamer's `DeviceMonitor` emits added/removed bus messages and is the right long-term source for a
live device list — **subscribe to those, don't poll.** The full detail on why Sone currently polls
once with a 2 s timeout (a GStreamer 1.28.0-1.28.2 quirk, fixed in 1.28.3), and on persisting device
identity by name/UID rather than index, is in `output-backends.md` §11.

## 4. Headless / Raspberry Pi: the client/server architecture question, answered

The brief mandates headless/server/CLI mode **now**, on Pi-class hardware. This is not a UI
afterthought: because a `hw:`/WASAPI-exclusive/CoreAudio-hog device cannot be opened by two
processes at once, "desktop and headless, both now" is fundamentally a client/server architecture
question, and tidalt has a complete, working answer to copy.

**Single-owner process, thin clients otherwise.** tidalt claims the D-Bus name
`org.mpris.MediaPlayer2.tidalt` on the session bus at startup; if already taken
(`ErrAlreadyRunning`), the process becomes a thin client instead of exiting
(`ref:tidalt/docs/client-server.md`). The stated reason is physical: *"ALSA `hw:` devices cannot be
shared between processes. If two programs both try to open `hw:1,0` the second one fails."* Modes:

- `tidalt` — TUI, becomes server or client depending on whether the name is free.
- `tidalt daemon` — headless engine, no terminal.
- `tidalt play tidal://track/<id>` — one D-Bus call to the running server, then exits; this is what
  a browser URL handler invokes.
- `tidalt setup --daemon` — installs a systemd **user** service.

**Consequence documented in tidalt's own README:** *"A plain `tidalt` TUI session does not register
a persistent MPRIS2 service, so media keys and `playerctl` will have no effect when the TUI is
closed"* (`ref:tidalt/README.md:156`). A design copying this pattern needs to decide the same
tradeoff explicitly, not stumble into it.

**Container/headless invocation** (`ref:tidalt/docs/docker.md:89-125`):

```
docker run -d --device /dev/snd --group-add $(getent group audio | cut -d: -f3) \
  benehiko/tidalt:latest daemon
```

For MPRIS reachable from the host: mount and point at the host's session bus —

```
-v /run/user/$(id -u)/bus:/run/user/1000/bus \
-e DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
```

**Packaging.** `ref:tidalt/README.md:45-67` ships `arm64`/`aarch64` `.deb`/`.rpm` alongside amd64 —
plan the same for streamboat's headless/Pi artefacts.

**Raspberry Pi audio hardware specifics**, from `ref:tidal-connect/userconfig/`:

- HDMI on Pi is ALSA card `vc4hdmi` (`vc4hdmi0`/`vc4hdmi1` on Pi 4, which has two HDMI outputs),
  needing an `iec958` plug with `slave.format "IEC958_SUBFRAME_LE"` over a `type hw` slave.
- The same tree's README records: *"I had to limit audio quality to 16bit/44.1kHz, otherwise this
  would fail when trying to stream hi-res content"* on Pi HDMI — a real hardware narrowing case;
  see `output-backends.md` §1 for the dither rule this triggers.
- I2S HAT card names are literal ALSA card ids, e.g. `card snd_rpi_hifiberry_dacplus`
  (`ref:tidal-connect/userconfig/hifiberry-dac-plus.asound.conf`), `iqaudio-dac.asound.conf`.

**Debian packaging interacts with the GStreamer version floor.** Debian 13 "trixie" (current
Raspberry Pi OS's base) ships GStreamer 1.26.2 — below the 1.26.10 FLAC-in-DASH floor from
`decoding-and-codecs.md` §1. Combine both constraints when picking a GStreamer-based engine for
headless/Pi.

**No session D-Bus on a headless box, often.** A stripped-down Pi/server image frequently has no
session bus at all, which silently disables both `ReserveDevice1` (tidalt's own reservation code
skips itself when there is no session bus — `output-backends.md` §2) and MPRIS. **A headless build
should not depend on either and should simply own `hw:` outright**, not degrade silently into a
half-working state.
