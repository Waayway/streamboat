# Raspberry Pi, small-device deployment, and daemon lifecycle

Table of contents:
1. ALSA and small-device audio constraints
2. Build and distribution for small devices
3. Time sync: a Raspberry Pi has no real-time clock
4. mDNS coexistence and name collisions
5. Diagnostics and testability
6. Daemon lifecycle: queue persistence, prefetch/gapless, idle-device handling

---

## 1. ALSA and small-device audio constraints

**CPU is not the binding constraint for FLAC.** FLAC decoding at 24/192 is a low-cost integer
workload; the reference deployments (mopidy-tidal, upmpdcli's plugin, Music Assistant) all serve
24/192 from Pi-class hardware. The documented sizing advice for the *Connect binary specifically*
is "a Raspberry Pi 3/4 will work … for a usb dac and hi-res audio, consider at least a Pi 3b+ or,
even better, a Pi 4b" (`ref:tidal-connect/README.md:161`), and on an Asus Tinkerboard the author had
to keep the CPU governor above about 600 MHz to avoid crackling. **No measured decode-CPU benchmark
was found anywhere in the reference set; do not quote a percentage.**

**The real costs are elsewhere**: HTTPS + TLS for the CDN fetch, DASH manifest parsing, and any
resampling. Avoid resampling entirely in bit-perfect mode (`ref:sone` and `ref:tidalt` both do).

**USB DAC / I²S output — the most complete small-device output recipe in the reference set**:
`ref:tidalt/internal/player/alsa.c` opens `hw:` devices and negotiates formats with
`snd_pcm_hw_params`, preferring `S32_LE > S16_LE > S24_3LE > S24_LE` for 16-bit sources and
`S24_3LE > S24_LE > S32_LE` for 24-bit. The S32_LE-first ordering for 16-bit sources is because
"many USB DACs (e.g. CS43198-based devices) have a buggy or non-functional S16_LE USB endpoint but
work correctly via their native 32-bit endpoint" — **not** because of "a Hidizs USB issue" (a
mislabeling worth avoiding; the Hidizs S9 Pro Plus appears in a *different* comment about anomalous
period-size values, 87 frames). It falls back to `plughw:` **only** when format negotiation is
refused, retries on device-busy against `hw:`, and recovers xruns with `snd_pcm_recover`. It also
reserves the device over D-Bus (`org.freedesktop.ReserveDevice1.Audio<N>`, in
`ref:tidalt/internal/player/mpv.go:328-359`) so PipeWire yields it, releasing on stop.

**Sample-rate switching**: a bit-perfect daemon must reopen the PCM when the track's rate changes,
which produces an audible gap on most DACs. Decide whether streamboat prioritises bit-perfect (gap
at rate change) or gapless (resample to a fixed rate — the same trade-off the Snapcast pipe output
forces unconditionally, `mpd-and-multiroom.md` §2). `ref:tidal-connect`'s per-DAC presets include
fixed-44.1 and fixed-48 variants precisely because some outputs (RPi HDMI) can't switch.

**Device naming — resolve by name, and know exactly what the reference config opens.** Resolve the
card by **name** from `/proc/asound/cards` at every start, never by a stored index — indices change
simply because a USB DAC was or wasn't powered on at boot. `ref:tidal-connect/bin/common.sh`
generates an `/etc/asound.conf`, but it does **not** uniformly open `default`:
`write_audio_config()` emits `pcm.tidal-audio-device` + `pcm.tidal-softvol` and passes
`tidal-softvol` when softvol is enabled (the shipped default, `ENABLE_SOFTVOLUME=yes`); it passes
`$CREATED_ASOUND_CARD_NAME` when that variable is set and softvol is off; it passes `custom` when
the user supplies their own `asound.conf` via `userconfig/`; and it falls back to `default` only in
the remaining case.

**Volume**: check for an existing `Master` control with `amixer -c <idx> controls | grep 'Master'`;
create a softvol only if absent, else create `SoftMaster` and warn that the remote's slider will
move hardware volume shared with other players.

**Pre-flight test tone, and it is opt-out**: `aplay -D $PLAYBACK_DEVICE
/assets/audio/short-low-tone-48k.wav`, falling back to the 44.1 kHz file; the app starts if a tone
played *or* if `ENABLE_GENERATED_TONE=no` was set (`tone_skipped=1`), which skips the device check
entirely — a deliberate escape hatch for DACs that click on every open. Busy check:
`watch cat /proc/asound/<card>/pcm0p/sub0/hw_params` — anything other than `closed` means busy.

**Device compatibility database**: `ref:tidal-connect` ships **26** per-DAC `asound.conf` presets in
`userconfig/` and a tested-device table in `assets/known-devices.md` (Aune S6, Chord Qutest, FiiO
K11, Fosi DS1, HiFiBerry DAC+/Digi+ Pro, iFi ZEN DAC V2, IQaudIO DAC, Topping D10, SMSL A8, Yulong
D200, Apple USB dongle, RPi HDMI/headphone outputs, …), with per-device `CARD_NAME`/`CARD_FORMAT`/
softvol notes. **This table and these presets are directly reusable as streamboat's ALSA device
knowledge base.**

**Wi-Fi buffering**: no measured guidance found in the reference set for buffer *sizing*. The
relevant lever available is mopidy-tidal's `playback_cache_buffer_bytes = 16777216` (16 MiB) with a
Range-capable local proxy (`headless-daemon-precedents.md` §1 — but note that project's *caching
legal-posture caveat* before copying its storage design, not just its buffer size). Adopt a
comparable read-ahead buffer and make it configurable; sizing itself is [inferred, not measured].

**Two specific hardware failure modes are well documented in the Pi audio community and cheap to
check in `streamboat doctor` (§5) — an earlier draft of this reference said "no guidance found" for
both, which was too pessimistic:**

1. **Wi-Fi power management.** Pi Wi-Fi defaults to `power_save` on, a documented cause of audio
   stalls and dropped connections. Fix: `iw dev wlan0 set power_save off`, persisted via a systemd
   unit or, on NetworkManager systems, `wifi.powersave=2`. Standing community advice is Ethernet
   where possible, 5 GHz Wi-Fi otherwise. `streamboat doctor` can read the current state with `iw
   dev wlan0 get power_save` and warn if it is on. [documented-web:
   https://forums.raspberrypi.com/viewtopic.php?t=380009,
   https://thepihut.com/blogs/raspberry-pi-tutorials/disable-wifi-power-management]
2. **USB DAC dropouts.** A known kernel/USB-scheduling issue class on Raspberry Pi, tracked upstream
   as `raspberrypi/linux#2215` ("USB DAC dropouts/glitches") — worth naming explicitly so users don't
   report it as a streamboat bug, and worth an explicit larger-period-size fallback in the ALSA
   output backend when dropouts are detected. [documented-web:
   https://github.com/raspberrypi/linux/issues/2215]
3. **Onboard audio can go either way, and card resolution must be by name because of it.**
   `ref:tidal-connect/README.md` documents both outcomes on real boxes: one where "the operating
   system has just disabled the onboard audio and set the Hifiberry HAT as the default card" (line
   337), and one where "the operating system has not disabled the onboard audio" so a USB DAC is
   *not* selected automatically (line 373) — additional concrete evidence for the card-by-name rule
   above. Document `dtparam=audio=off` in `/boot/firmware/config.txt` as the fix for a dedicated
   audio box that keeps defaulting to the wrong device. [verified-source
   `ref:tidal-connect/README.md:337,373`]

---

## 2. Build and distribution for small devices

Every precedent in this document is consumed as a Docker image or a distro package on a Pi
(`ref:tidal-connect`, `upmpdcli-docker`, Music Assistant, Snapcast) — build/distribution planning is
not optional if streamboat targets a Pi at all.

- **aarch64** (Pi 3/4/5 on a 64-bit OS) is mandatory. **armv7** (Pi Zero 2 W, 32-bit Raspberry Pi
  OS — also the ARMv7 target of the ifi Connect binary itself) is a real, separate cost and needs an
  explicit yes/no from the owner, not an assumption.
- **glibc vs. musl-static matters concretely**: a static build removes exactly the failure class
  documented for the Connect binary on Pi 5 (`tidal_connect_application: error while loading shared
  libraries: libsystemd.so.0: ELF load command alignment not page-aligned`, worked around there with
  `kernel=kernel8.img` in `/boot/firmware/config.txt`). librespot documents `rustls` as the TLS
  option "for avoiding external OpenSSL dependencies, reproducible builds, or when targeting
  platforms where native TLS dependencies are unavailable or problematic (musl, embedded, static
  linking)" — the same lever applies to `streamboat-core`.
  [documented-web: https://raw.githubusercontent.com/librespot-org/librespot/dev/Cargo.toml]
- **If an official container is published**, it needs `network_mode: host` if mDNS is used,
  `/dev/snd` passthrough for ALSA, and a `/var/run/dbus` mount if MPRIS or Avahi are involved —
  `ref:tidal-connect/docker-compose.yaml` is a ready-made template for a different purpose, reusable
  for this one.

---

## 3. Time sync: a Raspberry Pi has no real-time clock

A Pi boots with a wrong clock until NTP converges. A wrong clock breaks TLS validation against
TIDAL's CDN/API, breaks token-expiry arithmetic, breaks the Pushkin `USER_ACTION` `startedAt`
timestamp (`daemon-architecture.md` §6, which the SDK says must come from "a true-time source"), and
breaks Snapcast's sub-millisecond sync (`mpd-and-multiroom.md` §2).

The official TIDAL auth stack is visibly time-sensitive, independent of anything streamboat writes:
`ref:tidal-cli/src/index.ts` opens by monkey-patching `console.warn` solely to "Suppress 'TrueTime
is not yet synchronized' warnings from `@tidal-music/auth`" — the official package ships a TrueTime
service and complains before it syncs.

**Concrete fixes to add to the deployment guidance in `daemon-architecture.md` §1**:
`After=time-sync.target` (with `Wants=`) in the daemon's systemd unit; retry-with-backoff on TLS
failure during early uptime instead of a hard auth error; a clock-sync line in the `/health` output
(`daemon-architecture.md` §4).

---

## 4. mDNS coexistence and name collisions

On a Linux desktop, Avahi already owns mDNS; on macOS, `mDNSResponder` does; and if two streamboat
boxes share a LAN, both default to being called "streamboat". `mdns-sd` (the Rust crate this
project's discovery design already assumes, `mpd-and-multiroom.md` §4) resolves both problems: it
is "tested with some existing common tools (e.g. Avahi on Linux, dns-sd on MacOS, and Bonjour
library on iOS) to verify the basic compatibility", supports macOS/Linux/Windows and IPv4/IPv6, and
implements Probing (RFC 6762 §8.1), Simultaneous Probe Tiebreaking (§8.2) and Conflict Resolution
(§9) — collisions are handled at the protocol level and surfaced to the application as a
`DnsNameChange` event. It does **not** implement unicast responses (§5.4) or multipacket
known-answer suppression on the responder side.
[documented-web: https://raw.githubusercontent.com/keepsimple1/mdns-sd/main/README.md]

**Concrete rules to write down, not leave implicit**: derive the default instance name from the
hostname; keep it single-word-safe (the Connect binary's own issue #216 — "multi-word friendly
names break Avahi for some users" — is a directly relevant lesson, `tidal-connect.md` §2.4); surface
`DnsNameChange` in both the UI and the logs rather than silently renaming; expose
`mdns_backend: builtin|avahi|off` mirroring go-librespot's `zeroconf_backend`.

Also requires `avahi-daemon` running (not installed by default on DietPi) and, for the Connect
binary at least, working IPv6 — both operational lessons worth carrying into streamboat's own
mDNS-dependent features even though streamboat itself is not implementing Connect.

---

## 5. Diagnostics and testability

A headless audio daemon is the hardest thing to test and the easiest thing to break silently on a
Pi. Cheap mechanisms, all with a precedent already documented elsewhere in this skill:

- **The pipe/stdout PCM output** (`mpd-and-multiroom.md` §2) doubles as a CI sink — add `null` and
  `wav:<path>` outputs and playback tests become byte comparisons.
- **MPD conformance** (`mpd-and-multiroom.md` §1) can be asserted in CI against real clients with
  `mpc` (scriptable, packaged everywhere) plus the spec's own `commands`/`notcommands` output as a
  golden file.
- **A `streamboat doctor` subcommand** that runs exactly the checks the Connect wrapper learned the
  hard way (§1 above, `tidal-connect.md` §2.4): ALSA card-name resolution, device-busy detection
  (`cat /proc/asound/<card>/pcm0p/sub0/hw_params` != `closed`), a pre-flight test tone, avahi/mDNS
  reachability, clock sync (§3), IPv6 availability.

---

## 6. Daemon lifecycle: queue persistence, prefetch/gapless, idle-device handling

An MPD client expects the queue to survive `systemctl restart` (MPD itself has a `state_file`);
prefetch of the next track is what makes gapless playback and Wi-Fi-jitter survival work. None of
this is optional once the daemon is a real product, and each piece has a precedent already in this
skill:

- `ref:tidalt` prints "No audio device is opened until playback starts" in daemon mode — a
  deliberate hold-only-while-playing policy — paired with its D-Bus
  `org.freedesktop.ReserveDevice1.Audio<N>` reservation (§1) so PipeWire yields the device and
  reclaims it on stop.
- `ref:mopidy-tidal`'s `playback_cache_buffer_bytes = 16777216` with a Range-capable proxy is the
  read-ahead lever (with the legal-posture caveat about *what* gets cached,
  `headless-daemon-precedents.md` §1).
- MPD's `status` exposes `playlist` as a monotonically increasing 31-bit queue version
  (`mpd-and-multiroom.md` §1) — the standard way a client detects a changed queue without diffing
  it; streamboat's own protocol (`daemon-architecture.md` §4) should carry an equivalent revision
  number.

**Decisions to state explicitly, not leave to accident**: where the queue state file lives
(`$XDG_STATE_HOME`); whether it is written per mutation or on shutdown plus a timer; whether a
restart resumes playing or paused; how many tracks ahead are prefetched.

**Sleep/idle inhibition and resume-from-suspend are a related, unaddressed pair on a desktop host**
(a laptop running the GUI-embedded core, not just a Pi): acquire a sleep/idle inhibitor only while
playing (`daemon-architecture.md` §1 has the per-platform APIs), and on wake, explicitly reopen the
ALSA device and reconnect the Pushkin socket (`daemon-architecture.md` §6) rather than leaving the
daemon stuck paused with no error.
