# OS media integration and headless/Raspberry Pi architecture

Full narrative: `docs/research/audio-pipeline.md` §6, plus fact-check gap-fill in its §10.1 (the
big Raspberry Pi / headless section) and the souvlaki MSRV finding in §6's rewrite.

## Table of contents

1. Now-playing / transport controls (MPRIS, SMTC, NowPlayingInfoCenter, souvlaki)
2. Idle inhibit
3. Hot-plug and device enumeration
4. Headless / Raspberry Pi: the client/server architecture question, answered
5. SMTC needs a real HWND — no headless Windows now-playing integration
6. Idle inhibition on Windows and macOS has no reference implementation
7. Artwork for OS media integration must be local — never a remote URL
8. Headless/Pi CPU and memory feasibility is unmeasured

---

## 1. Now-playing / transport controls (MPRIS, SMTC, NowPlayingInfoCenter, souvlaki)

| Concern | Linux | Windows | macOS |
|---|---|---|---|
| Transport | MPRIS2 D-Bus (`org.mpris.MediaPlayer2`, `.Player`) | SMTC (`SystemMediaTransportControls`) | `MPNowPlayingInfoCenter` + `MPRemoteCommandCenter` |
| Media keys | MPRIS + `tauri-plugin-global-shortcut`, as Sone does | SMTC handles them natively | `MPRemoteCommandCenter` |
| Cross-platform crate | `souvlaki` 0.8.3 or `mpris-server` (Sone pins **0.9**; **0.10.0** is the current crates.io release as of this research — re-check before pinning) | `souvlaki` | `souvlaki` |
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

**Single-owner process, thin clients otherwise.** Full mechanics (D-Bus name-claim as mutex and
discovery, the four subcommand modes) are owned by
`headless-and-tidal-connect/references/headless-daemon-precedents.md` §2 and
`headless-and-tidal-connect/references/daemon-architecture.md` §1 — cite them rather than
restating. The physical reason, worth repeating here: *"ALSA `hw:` devices cannot be shared between
processes. If two programs both try to open `hw:1,0` the second one fails"* — which is why "desktop
and headless, both now" is fundamentally a client/server architecture question, not a UI
afterthought.

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

**This architecture is Linux-only as written — headless on Windows/macOS is a user-session process,
not a service, and that is an audio-layer fact, not only a media-integration one.** §5 below already
notes SMTC needs a real `HWND`. What is missing: `docs/research/headless-connect.md:1726-1727`
(marked unverified there, with a named one-hour spike to confirm) records that Windows services run
in session 0 with no interactive audio endpoint, and macOS needs a per-user LaunchAgent, not a
LaunchDaemon, for the same reason. The audio-layer consequence is that exclusive-mode WASAPI/CoreAudio
access from a non-interactive service context is itself the capability at risk, not only SMTC — a
Windows/macOS `streamboat-server` is architecturally a user-session background process on those two
platforms, never a true service, and the "control thread is the whole product" headless design above
is fully true only on Linux.

## 5. SMTC needs a real HWND — no headless Windows now-playing integration

§1 found the macOS event-loop constraint on souvlaki but not the Windows analogue, even though the
brief requires headless mode on every platform. souvlaki's `PlatformConfig` on Windows carries an
`hwnd: Option<*mut c_void>` that SMTC needs populated with a real window handle. Sone-windows spawns
a task that polls `app_handle.get_webview_window("main")` every 100 ms for up to 5 seconds before it
can build the config — *"We need to wait for the main window to be created to get HWND"*
(`ref:sone-windows/src-tauri/src/media_controls.rs:14-46`). **Consequences:** (1) a
`streamboat-server` Windows service or CLI daemon with no window gets **no SMTC integration at all**
— the Windows analogue of §4's "no session bus on headless" rule, and needs the same
do-not-depend-on-it treatment; (2) media-control init must be sequenced behind window creation on
Windows, never done at app start unconditionally; (3) Sone-windows's own handler wires only
Play/Pause/Toggle/Next/Previous/Stop, leaving `Seek`/`SetPosition`/`SetVolume` unimplemented — its
SMTC seek bar and volume are inert, a completeness bar streamboat should clear rather than copy.

## 6. Idle inhibition on Windows and macOS has no reference implementation

§2 gives Linux a complete four-layer answer; no reference client fills in Windows or macOS.
Sone-windows's `idle_inhibit.rs` contains only the Linux D-Bus/portal interfaces and is dead code on
the other two platforms; Strawberry has no `SetThreadExecutionState`/`IOPMAssertion` calls anywhere.
Write from platform APIs directly: **Windows** —
`SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)` while playing,
`SetThreadExecutionState(ES_CONTINUOUS)` on stop — deliberately **without** `ES_DISPLAY_REQUIRED`
(an audio player shouldn't keep the screen on); per-thread, so call from a thread alive for the whole
playback session. **macOS** — `IOPMAssertionCreateWithName(kIOPMAssertionTypePreventUserIdleSystemSleep,
kIOPMAssertionLevelOn, CFSTR("streamboat playback"), &assertionID)` (IOKit), `IOPMAssertionRelease`
on stop. For the headless/daemon Linux case, add `org.freedesktop.login1.Manager.Inhibit` with
`what="sleep"`, `mode="block"` (equivalent to `systemd-inhibit`) as a fifth layer — the only one of
§2's mechanisms that works with no session/display server at all.

## 7. Artwork for OS media integration must be local — never a remote URL

§1's table covers transport and enumeration but not artwork, often the first thing visibly broken —
most desktop shells will not fetch a remote `mpris:artUrl` over the network, leaving the now-playing
popup blank. High Tide caches the 320px cover to disk and hands MPRIS a local path:
`f"file://{utils.IMG_DIR}/{track.album.id}_320.jpg"` (`ref:high-tide/src/mpris.py:447-449`); Sone
carries `art_url` through its MPRIS command enum, only setting it when non-empty
(`ref:sone/src-tauri/src/mpris.rs:234,252-253`). **Consequence: the image cache is on the critical
path of OS media integration, not only the UI** — the cover file must exist on disk *before* the
metadata update is emitted, so cover fetch belongs in the track-transition sequence, prefetched
alongside the manifest for the next track. Windows/macOS need different shapes for the same
requirement: SMTC takes a thumbnail via `RandomAccessStreamReference` (a file or in-memory stream,
never a bare URL string); `MPNowPlayingInfoCenter` takes an `MPMediaItemArtwork` built from an
in-memory image. Rest of the MPRIS metadata contract while implementing this: `mpris:trackid` must be
a valid D-Bus object path (High Tide uses `/Track/{id}`); `mpris:length` is microseconds
(`track.duration * 1_000_000`).

## 8. Headless/Pi CPU and memory feasibility is unmeasured

§4 answers the *architecture* question well but never asks whether Pi-class hardware can actually
decode 24/192 FLAC plus run a second gapless decode branch — and **no reference project publishes
benchmarks**. This decides whether a GStreamer-based engine (two decode branches) or a pure-Rust
engine (smallest footprint) is the right headless build, and whether the ~23 MB-per-branch gapless
figure (`playback-behavior.md` §5) is affordable on a Pi Zero 2 W or Pi 3. **Treat this as a required
measurement, not an assumption:** decode 24/192 stereo FLAC to `/dev/null` with each engine candidate
(`gst-launch-1.0 filesrc ! flacparse ! flacdec ! fakesink`, `mpv --ao=null --untimed`, a Symphonia
decode loop) on a Pi 3B+, Pi 4, and Pi 5, reporting single-core utilization and peak RSS, with the
second decode branch running concurrently to model gapless. One bounding fact partly moots the
question for the most common headless setup: Pi HDMI output was limited to 16-bit/44.1kHz for
hi-res content to work at all (§4 above) — on a Pi's own HDMI output, decode feasibility is secondary
to the narrowing-conversion-with-dither path (`output-backends.md` §1) actually being exercised.
