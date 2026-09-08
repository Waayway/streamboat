# Headless mode, remote control, and TIDAL Connect

Research for streamboat, 2026-09-07. Scope: what TIDAL Connect is and whether streamboat can
participate in it; what the open-source headless/server precedents actually do; what protocol
streamboat's own headless daemon should speak; how a GUI and a daemon share one core; and what
small-device (Raspberry Pi) deployment demands.

Evidence tags used throughout:
- **[verified-source]** — read directly in a reference checkout under
  `/tmp/.../scratchpad/ref/`, cited as `ref:<project>/<path>`.
- **[documented-web]** — read on the web (URL cited). Reliability varies; noted where weak.
- **[inferred]** — my reasoning from the above, not stated anywhere.
- **[unverified]** — could not confirm; stated as a gap, not a fact.

**Fact-check pass (2026-09-07)**: this document has been independently fact-checked against the
reference checkouts and reachable primary sources. Claims that turned out wrong are marked
**refuted**/**corrected** in place, with the correction and its source; claims that turned out right
often carry a **confirmed** tag plus a nuance the original draft missed; new facts the fact-check
supplied are marked **[gap]**. Genuinely open items keep **[unverified]**. Treat any claim without one
of these markers as carried over unchanged from the original research pass.

---

## Summary

1. **TIDAL Connect is a controller/target split**: the phone/desktop/web TIDAL app is the
   *controller*; a speaker, streamer, DAC or TV is the *target*; the target pulls audio from TIDAL's
   CDN itself. The controller's local audio pipeline (DSP, replay gain, exclusive output) does not
   apply to the target. [documented-web + verified-source]
2. **Targets are discovered over mDNS with the service type `_tidalconnect._tcp`**
   (`avahi-browse -t _tidalconnect._tcp`; add `-r` to resolve hostname/address/port/TXT record —
   the troubleshooting doc's own examples omit `-r`, but `avahi-browse(1)` supports it). Records
   carry a ~120 s TTL, which is why a rapidly restarting target can collide with its own stale
   advertisement. [confirmed, documented-web:
   https://github.com/TonyTromp/tidal-connect-docker/blob/master/docs/TROUBLESHOOTING.md]
3. **There is no open implementation of the Connect protocol — neither target nor controller.**
   Every working Linux target is the same closed ARM binary (`tidal_connect_application`) leaked from
   an iFi Audio device image, re-packaged by TonyTromp → GioF71 → HiFiBerry/Volumio/moOde users.
   [verified-source `ref:tidal-connect/bin/entrypoint.sh`; documented-web]
4. **The Connect binary is deliberately hardened against reverse engineering.** Its third-party
   licence folder lists `advobfuscator` (a compile-time string/control-flow obfuscator) alongside
   `asio`, `boost`, `clara`, `curl`, `mdnsresponder`, `openssl`, `websocketpp`. [documented-web:
   https://github.com/shawaj/ifi-tidal-release/tree/master/licenses/tidal_connect]
5. **That dependency list is the best available description of the protocol**: mDNS advertisement +
   a WebSocket control channel + HTTPS/TLS to TIDAL, with a per-device X.509-style certificate
   (`--tc-certificate-path`, default `IfiAudio_ZenStream.dat`) as the device identity. [inferred from
   the licence list + CLI flags]
6. **Device identity is the hard gate, and it has two inputs, not one.** A Connect target
   authenticates with a vendor-issued certificate (`--tc-certificate-path`) **and** the upstream
   systemd unit also passes `--netif-for-deviceid eth0` — i.e. the device id is derived partly from
   a named network interface (its MAC), which is likely why the same certificate can be reused
   across many community installs without an observed collision. GioF71's Docker wrapper drops this
   flag entirely (`ref:tidal-connect/bin/entrypoint.sh` has no `--netif-for-deviceid`), which may be
   an unnoticed gap in its own uniqueness story. There is no documented way to obtain a certificate
   outside a commercial partnership; the community route is to reuse iFi's. streamboat must not ship
   either. [verified-source + confirmed, documented-web:
   https://raw.githubusercontent.com/shawaj/ifi-tidal-release/master/README.md]
7. **The Connect binary is capped at 16-bit/44.1 kHz LOSSLESS since TIDAL removed MQA at the end of
   July 2024** — previously 24/48 plus MQA unfolding to 24/88 or 24/96.
   `ref:tidal-connect/README.md` explicitly steers hi-res users to mopidy-tidal, upmpdcli's TIDAL
   plugin, Music Assistant or BubbleUPnP instead. [verified-source]
8. **The desktop client's device picker is visible in the official Redux action set**: a
   `remotePlayback/*` namespace with three device types — `chromeCast`, `tidalConnect`,
   `cloudConnect` — plus a `cloudQueue/*` namespace for server-side queues.
   `ref:TidaLuna/plugins/lib/src/redux/types/store/RemotePlayback.ts` gives the device shape
   (`addresses[]`, `friendlyName`, `fullname` e.g. `${string}._googlecast._tcp.local`, `id`, `port`,
   `type`). It reveals the *client-side data model*, not the wire protocol. [verified-source]
9. **TIDAL enforces one concurrent stream through a WebSocket ("Pushkin")**: `POST {apiBase}rt/connect`
   returns `{ "url": … }`; the client opens that socket, sends `{"type":"USER_ACTION","payload":
   {"startedAt":…}}` on user-initiated play, and must pause on incoming
   `PRIVILEGED_SESSION_NOTIFICATION` (also handle `RECONNECT`). A headless daemon that ignores this
   will fight the user's phone. [verified-source `ref:tidal-sdk-web/packages/player/src/internal/
   services/pushkin.ts`, `ref:tidal-sdk-android/player/streaming-privileges/…`]
10. **The strongest headless precedent inside the reference set is mopidy-tidal** (Apache-2.0,
    v0.3.13, Python ≥3.12, `Mopidy>=3.0`, `tidalapi>=0.8.10`): a TIDAL backend for the Mopidy server,
    driven by MPD clients, the Iris web UI, or MPRIS, with a headless login story and a Range-capable
    SQLite caching proxy in front of `https://lgf.audio.tidal.com/`. [verified-source]
11. **The second strongest is `tidalt`**: one Go binary with `tidalt daemon` (playback engine +
    MPRIS2, no TUI), plain `tidalt` (TUI; if a daemon already owns the D-Bus name it opens in
    *client mode* and forwards commands), `tidalt play <url>` (forwards a deep link over D-Bus or
    spawns a terminal), and `tidalt setup --daemon` (writes a `systemd --user` unit).
    `ref:tidalt/cmd/tidalt/daemon.go`. This is exactly the "same binary, two modes" architecture
    streamboat needs. [verified-source]
12. **MPD compatibility is the cheapest way to inherit a client ecosystem**: port 6600, a line
    protocol, `idle` for push notifications, and existing phone/desktop clients (MALP, ncmpcpp,
    Cantata, mpc). Mopidy-MPD and Nuclear both implement *subsets* successfully — and, contrary to an
    earlier draft of this document, a subset does not have to drop catalogue browsing: Mopidy-MPD
    maps `lsinfo`/`search`/`find` onto an arbitrary backend via `context.browse()`, which is exactly
    how MALP and ncmpcpp browse TIDAL today through mopidy-tidal. MPD's access control is **not**
    just a shared plaintext password: it also supports `local_permissions` (Unix-socket clients) and
    `host_permissions` (per-IP/CIDR ranges), so a loopback- or Unix-socket-bound listener needs no
    password at all — only the transport stays unencrypted. Mopidy-MPD itself is now "kept on life
    support by the Mopidy core developers" with a "Maintainer wanted" notice, which tempers how much
    weight to put on it as a *maintained* precedent even though its design is sound. [confirmed,
    documented-web: https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/doc/user.rst;
    https://github.com/mopidy/mopidy-mpd]
13. **A local HTTP/WebSocket control API is the norm for modern daemons.** go-librespot ships
    "REST API + WebSocket events" plus MPRIS; spotifyd ships MPRIS plus a custom
    `rs.spotifyd.Controls` D-Bus interface (`TransferPlayback`, `VolumeUp`); tidal-hifi ships a
    documented HTTP API (`POST /player/play`, `PUT /player/volume`, `GET /current`, …); sone ships an
    MCP server on `127.0.0.1:5577` behind a random UUID token in the URL path. [documented-web +
    verified-source]
14. **Multiroom has a cheap answer and a new one.** Cheap: a raw-PCM pipe sink feeding
    `snapserver` (ports 1704 stream / 1705 TCP JSON-RPC / 1780 HTTP+WS+Snapweb). New: **Sendspin**
    (formerly Resonate), an Open Home Foundation protocol shipped in Music Assistant 2.8 —
    WebSocket transport with Noise encryption, mDNS `_sendspin._tcp.local.` (client, port 8928) and
    `_sendspin-server._tcp.local.` (server, port 8927), binary frames typed by first byte, Kalman
    time-filter clock sync. [documented-web]
15. **Chromecast sending is technically reachable** (`cast-sender` / `rust_cast` crates,
    `_googlecast._tcp` discovery, Default Media Receiver `CC1AD845`) but requires handing the
    receiver a URL it can fetch — which for TIDAL means either a signed CDN URL that expires or a
    local re-serving proxy — **and it caps quality below what streamboat's own daemon already
    reaches**: Google Cast supports FLAC only up to 96 kHz/24-bit, and 24-bit at 176.4/192 kHz does
    not play on Chromecast Audio and can hang the device. **AirPlay sending** has precedents
    (`libraop`, Music Assistant's `airplay-cli`, `akustikrausch/airplay2-sender-cpp`) but means
    decoding and re-transmitting audio. Both raise the same re-transmission question. [documented-web:
    https://developers.google.com/cast/docs/media + inferred]
16. **Being an AirPlay/Chromecast *receiver* is not streamboat's job — but not because it is
    impossible.** shairport-sync (AirPlay 2) and UxPlay already exist for the AirPlay side. A
    Chromecast receiver is *not* technically blocked: `rgerganov/shanocast` is a working open-source
    one, built on Chromium's Openscreen. Its receiver authentication passes only because it reuses
    **precomputed signatures taken from AirReceiver** — another vendor's device credentials — which
    is exactly the "reuse someone else's identity" move streamboat has already rejected for iFi's
    Connect certificate (§6). Framing the Chromecast receiver as a technical dead end understates the
    real, and stronger, reason to skip it: doing it right requires the same kind of credential
    borrowing streamboat refuses to do elsewhere. [refuted-and-corrected, documented-web:
    https://github.com/rgerganov/shanocast]
17. **Headless login is a solved UX problem with three known shapes**: device-code + QR in the
    terminal (`ref:tidalt/internal/tidal/loginprint.go` uses `qrterminal/v3`); a tiny local HTTP page
    that shows the link and accepts the pasted PKCE redirect URL (mopidy-tidal, port 8989); and
    mopidy-tidal's **"login hack"** — while logged out, return a dummy Track/Album/Artist whose title
    is the login message and whose cover art is a QR of the login URL, so *any* MPD client displays
    the prompt with no protocol extension. [verified-source]
18. **Raspberry Pi is a proven target for this workload**, but the constraints are ALSA-shaped, not
    CPU-shaped: card indices shift across reboots (prefer card *name*), a `Master` mixer control may
    already exist (create `SoftMaster` softvol instead of coupling to hardware volume), the device
    can be locked by another process, and Pi 5 needed `kernel=kernel8.img` to load `libsystemd.so.0`
    for the Connect binary. `ref:tidal-connect/README.md`, `ref:tidal-connect/bin/common.sh`.
    [verified-source]
19. **Recommended path**: build `streamboat-core` as a library; ship one binary with a `daemon`
    subcommand; expose MPRIS on Linux from day one; add a token-authenticated local HTTP+WebSocket
    control API and a tiny web remote; add an MPD-compatible listener to inherit phone clients; add
    a raw-PCM pipe output for Snapcast; advertise `_streamboat._tcp` over mDNS so the desktop GUI —
    and later the mobile app — can drive a remote daemon. Do **not** attempt TIDAL Connect in either
    role.
20. **The single biggest open decision** is whether the desktop GUI is always a client of a local
    daemon (Roon/MPD model) or embeds the core in-process and only *optionally* runs a daemon
    (tidalt model). Everything else — IPC shape, state sync, config layout — follows from it.

---

## Findings

### 1. What TIDAL Connect actually is

TIDAL Connect is TIDAL's own "cast to a device" feature, modelled on Spotify Connect: the TIDAL app
becomes a remote control and a network audio device becomes the player. The target fetches the
stream from TIDAL directly, so the phone can sleep, and audio never traverses the phone. [documented-
web: https://www.whathifi.com/features/tidal-connect-everything-you-need-to-know]

Consequences that matter for a client author:

- **Local DSP does not apply.** streamboat's replay-gain chain, resampler choice, exclusive-mode
  output and gapless implementation are all bypassed when the user hands off to a Connect target.
  The existing feature research already records this asymmetry
  (`/home/user/streamboat/docs/research/tidal-client-features.md`, §10). [verified-source]
- **The controller role is a *catalogue + queue + control* role**, not an audio role. That is why
  TIDAL also keeps a server-side queue (`cloudQueue/*`, see §4).
- **The target role requires a device identity issued by TIDAL** (see §2).

Ecosystem scale, from trade press rather than TIDAL: the SDK opened to hardware developers in 2021;
500+ compatible devices by 2023; 1M+ Connect devices claimed by 2025; partner brands include
Bluesound/BluOS, NAD, Naim, KEF, Cambridge Audio, iFi Audio, Marantz, McIntosh, Focal, WiiM, dCS,
DALI, Dynaudio, Electrocompaniet, Esoteric. [documented-web, low-to-medium reliability — the
aggregator pages (ampvortex.com) read as SEO content; the brand list is corroborated by
whathifi.com's launch coverage]

`tidal.com/connect`, `tidal.com/supported-devices` and `developer.tidal.com` are **blocked by this
environment's egress proxy**, so TIDAL's own wording on partner licensing could not be read. Treat
every licensing claim below as second-hand. [unverified]

---

### 2. The only working Linux Connect target, dissected

Everything the open-source world runs is one closed binary. Lineage, as far as the record shows:

```
iFi Audio ZEN Stream firmware image
  → ppy2/ifi-tidal-release          (binaries + scripts extracted, ARMv7)
  → shawaj/ifi-tidal-release        (fork; carries licences/ folder)
  → seniorgod/ifi-tidal-release     (fork for ARM SBCs)
  → TonyTromp/tidal-connect-docker  (Docker image edgecrush3r/tidal-connect)
  → GioF71/tidal-connect            (ref:tidal-connect — compose + config wrapper, MIT)
  → ce-designs / lovehifi / chiefy / kevinjpickard forks
  → pulpier/tidal-connect-hifiberry (HiFiBerry OS NG, PipeWire)
```
[documented-web, from repository descriptions and READMEs]

#### 2.1 What the wrapper contributes

`ref:tidal-connect` is MIT-licensed and **contains no TIDAL binary**: "This repository does not
contain any tidal-connect binary." (`ref:tidal-connect/README.md:3-4`). It contributes ALSA device
resolution, an `/etc/asound.conf` generator, a test tone, a restart loop, and a compose file.
[verified-source]

Container dependencies (`ref:tidal-connect/build/Dockerfile`, base `debian:bookworm-slim`):

```
ca-certificates          libportaudio-ocaml       libavahi-client3
alsa-utils               libssl-dev               libcurl4
libavformat-dev
```
[verified-source]

The upstream binary's own stated runtime deps are older: `libssl1.0.0`, `libportaudio2`,
`libflac++6v5`. [documented-web: https://github.com/shawaj/ifi-tidal-release/blob/master/README.md]

Compose service shape (`ref:tidal-connect/docker-compose.yaml`) — every line is a requirement, not a
convenience: [verified-source]

```yaml
image: ${TIDAL_CONNECT_IMAGE:-edgecrush3r/tidal-connect:latest}
network_mode: host          # mandatory: mDNS is multicast, bridge networking breaks discovery
devices: [ /dev/snd ]       # direct ALSA access
volumes:
  - /var/run/dbus:/var/run/dbus   # Avahi client talks to the host avahi-daemon over D-Bus
dns: [ ${DNS_SERVER_LIST:-8.8.8.8} ]
restart: unless-stopped
```

#### 2.2 The invocation

`ref:tidal-connect/bin/entrypoint.sh` builds and `eval`s: [verified-source]

```
/app/ifi-tidal-release/bin/tidal_connect_application \
  --tc-certificate-path <cert>        # default /app/ifi-tidal-release/id_certificate/IfiAudio_ZenStream.dat
  --playback-device <alsa device>     # resolved from CARD_NAME/CARD_INDEX at container start
  -f "<friendly name>"                # what the TIDAL app shows; default TidalConnect
  --model-name "<model>"              # default "Audio Streamer"
  --codec-mpegh true
  --codec-mqa <true|false>            # default false
  --disable-app-security <bool>       # default false
  --disable-web-security <bool>       # default true
  --enable-mqa-passthrough <bool>     # default false
  --log-level <n>                     # default 3
  --enable-websocket-log "0"
  [--clientid "<id>"]                 # optional, overrides the built-in client id
```

Notable: `--enable-websocket-log` and `--disable-web-security` both point at a WebSocket/HTTP control
surface inside the binary; `--clientid` shows the target carries a TIDAL OAuth client id like every
other TIDAL client. [verified-source + inferred]

**A twelfth flag exists upstream that GioF71's wrapper drops.** The systemd unit shipped by
`shawaj/ifi-tidal-release` invokes the same binary with an additional `--netif-for-deviceid eth0` —
the Connect device id is derived partly from a named network interface (i.e. its MAC address),
alongside the certificate. That is a second identity input the wrapper's own flag list — and this
report's flag list until this fact-check — omits. [confirmed, documented-web:
https://raw.githubusercontent.com/shawaj/ifi-tidal-release/master/README.md, the pasted `[Service]`
`ExecStart` block]

An optional `speaker_controller_application` runs first, inside `tmux`, when present
(`DISABLE_CONTROL_APP=0`, `SLEEP_TIME_SEC=3` before launching the main app). Its role in the forks is
to bridge metadata and volume: TonyTromp's stack runs a "volume-bridge" service that exports playback
metadata and syncs the phone's volume changes to the ALSA mixer. [verified-source +
documented-web: https://github.com/TonyTromp/tidal-connect-docker]

#### 2.3 What the binary links against — the strongest protocol evidence available

`licenses/tidal_connect/` in the ifi release contains eight third-party licences: [documented-web:
https://github.com/shawaj/ifi-tidal-release/tree/master/licenses/tidal_connect]

| Library | What it implies |
| --- | --- |
| `mdnsresponder` | Apple Bonjour — the target advertises itself over mDNS/DNS-SD |
| `websocketpp` | a WebSocket endpoint — almost certainly the control channel |
| `asio`, `boost` | the C++ async plumbing under both |
| `curl`, `openssl` | HTTPS to TIDAL's API/CDN, and TLS identity from the certificate |
| `clara` | the CLI flag parser seen above |
| `advobfuscator` | **compile-time obfuscation of strings and control flow** |

`advobfuscator` is the tell. Someone deliberately made this binary unpleasant to reverse. Combined
with the per-device certificate, that is a clear signal that TIDAL treats the Connect target
protocol as a licensed, gated interface — not an undocumented-but-tolerated one like the streaming
API. [inferred, high confidence]

A second data point: GioF71's README quotes a runtime error from the binary,
`[tisoc] [error] [avahiImpl.cpp:358] avahi_client_new() FAILED: Daemon not running`
(`ref:tidal-connect/README.md`). So at least some builds use an **Avahi** implementation rather than
Bonjour — consistent with `libavahi-client3` in the Dockerfile and the `/var/run/dbus` mount. The
`mdnsresponder` licence is probably a different build target (the ZEN Stream firmware) or a fallback
backend. [verified-source + inferred]

#### 2.4 Operational constraints the wrapper exists to solve

All from `ref:tidal-connect/README.md` and `ref:tidal-connect/bin/common.sh`: [verified-source]

- **IPv6 is mandatory.** "Tidal connect won't work if your system does not support ipv6"
  (issue #21). No workaround known.
- **avahi-daemon must be running.** Not installed by default on DietPi.
- **ALSA card indices move.** Prefer `CARD_NAME` over `CARD_INDEX`; the index is resolved at
  container start and a fresh `/etc/asound.conf` written. **Correction to an earlier draft of this
  document**: the app does not uniformly open `default`. `common.sh`'s `write_audio_config()` emits
  `pcm.tidal-audio-device` + `pcm.tidal-softvol` and passes `tidal-softvol` when softvol is enabled
  (the shipped default, `ENABLE_SOFTVOLUME=yes`); it passes `$CREATED_ASOUND_CARD_NAME` when that
  variable is set and softvol is off; it passes `custom` when the user supplies their own
  `asound.conf` via `userconfig/`; and it falls back to `default` only in the remaining case (no
  softvol, no `CREATED_ASOUND_CARD_NAME`). `entrypoint.sh` then invokes the binary with
  `--playback-device $(get_playback_device)`. Indices change simply because a USB DAC was or wasn't
  powered on at boot. [confirmed, verified-source `ref:tidal-connect/bin/common.sh`]
- **Software volume needs care.** `common.sh` runs `amixer -c <idx> controls | grep 'Master'`; if no
  `Master` exists it creates a softvol named `Master`; if one exists it creates `SoftMaster` and logs
  a warning that the TIDAL slider will move the *hardware* volume and affect every other player on
  that card.
- **Exclusive device locking.** "Tidal Connect will access exclusively your audio device if you
  select it in your … Tidal App." Check with `watch cat /proc/asound/<card>/pcm0p/sub0/hw_params` —
  anything other than `closed` means busy.
- **Pre-flight test tone, and it is opt-out.** `aplay -D $PLAYBACK_DEVICE
  /assets/audio/short-low-tone-48k.wav` (absolute path inside the container), falling back to the
  44.1 kHz file; the app starts if a tone played *or* if `ENABLE_GENERATED_TONE=no` was set
  (`tone_skipped=1`), which skips the device check entirely — a deliberate escape hatch for DACs
  that click on every open. This catches a locked or misconfigured device before a silent failure,
  but only when the gate is left on. [confirmed, verified-source `ref:tidal-connect/bin/entrypoint.sh`]
- **Multi-word friendly names break Avahi for some users** (issue #216) — the default was changed to
  a single word.
- **Raspberry Pi 5**: `tidal_connect_application: error while loading shared libraries:
  libsystemd.so.0: ELF load command alignment not page-aligned`, fixed by adding
  `kernel=kernel8.img` to `/boot/firmware/config.txt`.
- **Hardware sizing**: "A Raspberry Pi 3/4 will work. If you plan to use a usb dac and hi-res audio,
  consider at least using a Pi 3b+ or, even better, a Pi 4b." On an Asus Tinkerboard the author had
  to pin the minimum CPU frequency around 600 MHz to stop crackling.

The repo ships **26** per-DAC `asound.conf` presets in `userconfig/` (not "~40" as an earlier draft
of this document said — `ls ref:tidal-connect/userconfig/*.asound.conf | wc -l` = 26; the directory
has 27 entries, the 27th being `README.md`) and a tested-device table in
`assets/known-devices.md` (Aune S6, Chord Qutest, FiiO K11, Fosi DS1, HiFiBerry DAC+/Digi+ Pro, iFi
ZEN DAC V2, IQaudIO DAC, Topping D10, SMSL A8, Yulong D200, Apple USB dongle, RPi HDMI/headphone
outputs, …), with per-device `CARD_NAME` / `CARD_FORMAT` / softvol notes. **This table and these
presets are directly reusable as streamboat's ALSA device knowledge base.** [verified-source]

#### 2.5 Quality ceiling

`ref:tidal-connect/README.md:57-62`: after TIDAL removed all MQA content at the end of July 2024,
this implementation "could play hi-res files only up to 24/48 and MQA content" — with in-app
unfolding to 24/88 or 24/96 — and is now effectively limited to 16/44.1 redbook. The README then
lists alternatives that *do* reach 24/192: mopidy-tidal, upmpdcli's TIDAL plugin (with a renderer
whitelist), Music Assistant, BubbleUPnP, Audirvana, Roon. [verified-source]

That is a decisive product argument: **a streamboat headless daemon on a Pi beats the only available
Connect target on sound quality**, because it can request `HI_RES_LOSSLESS` and hand a 24/192 FLAC to
ALSA directly.

---

### 3. Discovery: what the TIDAL app looks for

- **Service type: `_tidalconnect._tcp`.** Verify with `avahi-browse -t _tidalconnect._tcp` or
  `avahi-browse -a | grep -i tidal`. mDNS records carry ~120 s TTL, which is why a rapidly
  restarting target can collide with its own stale advertisement. [documented-web:
  https://github.com/TonyTromp/tidal-connect-docker/blob/master/docs/TROUBLESHOOTING.md]
- The advertisement publishes at least the service (friendly) name and model name — the two values
  passed as `-f` and `--model-name`. [verified-source, inferred mapping]
- **Port and TXT record keys are not documented in any of the sources read for this report — but
  this is resolvable in minutes, not a research dead end.** Run `avahi-browse -r -t
  _tidalconnect._tcp` (the `-r` resolve flag; the troubleshooting page cited above omits it) against
  any Connect-capable device already on the LAN — a Volumio/moOde box, a WiiM, a BluOS speaker — and
  it prints hostname, IPv4/IPv6 address, port and the full TXT record. On macOS: `dns-sd -B
  _tidalconnect._tcp` then `dns-sd -L <name> _tidalconnect._tcp`. Treat this as a task for the owner
  to run on their own network (it costs minutes) rather than as an open question, since it settles
  both a diagnostics feature (§6) and the "do not collide with TIDAL's service type" rule at the same
  time. [documented-web: https://github.com/TonyTromp/tidal-connect-docker/blob/master/docs/TROUBLESHOOTING.md
  (unresolved form only); `avahi-browse(1)`, `dns-sd(1)` — **[unverified until the owner runs it]**]
- Host networking is required for the container because multicast does not cross Docker's bridge.
  [verified-source]

The controller side sees devices in the shape recorded in the desktop client's Redux store (§4),
including a `fullname` field whose example is an mDNS fullname.

---

### 4. The controller side: what TidaLuna and the SDKs reveal

`ref:TidaLuna` is a mod loader running *inside* the official Electron client, so its TypeScript
types are a faithful transcription of the official client's own Redux state. This is the best
available window onto the device picker.

`ref:TidaLuna/plugins/lib/src/redux/types/store/RemotePlayback.ts` — verbatim: [verified-source]

```ts
export type RemotePlaybackDeviceType = "chromeCast" | "tidalConnect" | "cloudConnect";
export type RemotePlaybackDevice = {
  /** @example 127.0.0.1 */
  addresses: string[];
  friendlyName: string;
  /** @example `${string}._googlecast._tcp.local` */
  fullname: string;
  id: string;
  port: number;
  type: RemotePlaybackDeviceType;
};
export type RemotePlayerStates = "BUFFERING" | "IDLE" | "PAUSED" | "PLAYING";
export interface TidalConnectQueueInfo {
  maxAfterSize: number; maxBeforeSize: number;
  queueId: string; repeatMode: unknown; shuffled: boolean;
}
export interface TidalConnectMediaInfo {
  customData: { audioMode: AudioMode; audioQuality: AudioQuality };
  itemId: string; mediaId: string;
  metadata: { albumName: string; artists: ItemId[]; duration: number;
              images: unknown; title: string };
}
export interface RemotePlayback {
  connectedDevice: unknown; deviceToConnectTo: unknown;
  devices: { chromeCast: RemotePlaybackDevice[]; cloudConnect: RemotePlaybackDevice[];
             tidalConnect: RemotePlaybackDevice[] };
  isConnected: boolean; remotePlaybackReceiverState: number;
  session: unknown; sessionStatus: "nosession";
}
```

Matching actions in `ref:TidaLuna/plugins/lib/src/redux/types/actions/actionTypes.ts`:
[verified-source]

```
remotePlayback/DISCOVER_DEVICES          remotePlayback/REFRESH_DEVICES
remotePlayback/DEVICES_RECEIVED          remotePlayback/CONNECT_TO_DEVICE
remotePlayback/CONNECT_TO_DEVICE_FAILED  remotePlayback/DEVICE_CONNECTED
remotePlayback/DEVICE_DISCONNECTED       remotePlayback/DISCONNECT_ALL_DEVICES
remotePlayback/CONNECTION_LOST
remotePlayback/tidalConnect/{UPDATE_PLAYER_STATE, MEDIA_CHANGED, QUEUE_CHANGED,
                            QUEUE_ITEMS_CHANGED, HANDLE_ERROR, DISCONNECT}
remotePlayback/chromeCast/{UPDATE_PLAYER_STATE, UPSTREAM_QUEUE_CHANGED, DISCONNECT}
remotePlayback/cloudConnect/{UPSTREAM_QUEUE_CHANGED, DISCONNECT}
remotePlayback/remotePlaybackReceiver/{STATE_CHANGED, MEDIA_CHANGED, DISCONNECT}
chromeCast/{API_AVAILABLE, CAST_STATE_CHANGED, CONNECTED, DISCONNECTED,
            REMOTE_PLAYER_CHANGED, REPEAT_MODE_CHANGED, REQUEST_START_CASTING,
            SESSION_STATE_CHANGED, UPSTREAM_QUEUE_CHANGED}
cloudQueue/{CREATE_CLOUD_QUEUE, ADD_ITEMS_TO_CLOUD_QUEUE, GET_CLOUD_QUEUE_ITEMS,
            FILL_CLOUD_QUEUE_WITH_HISTORY, MOVE_TRACKS, REMOVE_ELEMENT,
            SET_CURRENT_ITEM, SET_SHUFFLED, UPDATE_ITEMS_ETAG, …}
```

Readings:

- **The three transports share one abstraction.** `remotePlaybackReceiver/*` looks like the common
  receiver interface, with `tidalConnect`, `chromeCast` and `cloudConnect` as concrete backends.
  [inferred]
- **The Connect message vocabulary is Cast-shaped.** `mediaId` + `customData` + `metadata`, a queue
  object with `maxBeforeSize`/`maxAfterSize` and a `queueId`, `RemotePlayerStates` of
  `BUFFERING|IDLE|PAUSED|PLAYING`. Combined with `websocketpp` in the target binary, the most likely
  design is a JSON-over-WebSocket control channel with a Cast-like namespace/message model. This is
  **shape resemblance, not proof**; no wire capture was available. [inferred, medium confidence]
- **`cloudConnect` + `cloudQueue` mean handoff does not require the LAN.** A server-side queue with
  an ETag (`UPDATE_ITEMS_ETAG`) lets any signed-in device pick up the queue. Endpoints for
  `cloudQueue` are **not documented anywhere I could find** and do not appear in the official SDKs.
  [unverified]
- **The official SDKs (web, Android, iOS) contain no Connect/device-picker code at all.** Searching
  `ref:tidal-sdk-web`, `ref:tidal-sdk-android`, `ref:tidal-sdk-ios` for `remotePlayback`,
  `cloudQueue`, `tidalConnect` returns nothing. The SDKs cover auth, catalogue, manifests and local
  playback only. **Connect is not in the public SDK surface.** [verified-source — negative result]

**Conclusion for streamboat: the controller side is as closed as the target side.** streamboat can
enumerate `_tidalconnect._tcp` devices on the LAN, but has no documented way to connect to one, and
building one from scratch means reverse-engineering an obfuscated binary's WebSocket protocol *and*
solving controller-side authentication. [inferred, high confidence]

---

### 5. Streaming privileges — the constraint every daemon hits

TIDAL allows one concurrent stream per account and enforces it over a WebSocket, in both official
SDKs:

- `POST {legacyApiUrl}/rt/connect` with `Authorization: Bearer <token>` returns `{ "url": "<wss …>" }`.
  `ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts:fetchWebSocketURL`;
  `ref:tidal-sdk-android/player/streaming-privileges/src/main/kotlin/…/connection/
  StreamingPrivilegesService.kt` declares `@POST("rt/connect")` returning
  `StreamingPrivilegesWebSocketInfo(val url: String)`. The Android DI module registers
  `"${apiEndpoint}rt/connect"` as requiring `Credentials.Level.USER`. [verified-source]
- **Outgoing**: `{"type":"USER_ACTION","payload":{"startedAt":<epoch ms from a true-time source>}}`.
  The web SDK comments: "Call this method to tell Pushkin a user action happened, so it can make good
  qualified guesses if you're the session with allowed playback". Android names the same message
  `Acquire`. [verified-source]
- **Incoming**: `PRIVILEGED_SESSION_NOTIFICATION` (payload: `clientDisplayName`, `sessionId`,
  `endsAt{clientTime,serverTime}`, `updatedAt{…}`) → the web SDK immediately calls
  `playerState.activePlayer?.pause()` and dispatches a `streaming-privileges-revoked` event carrying
  the *other* device's display name; and `RECONNECT` → reconnect the socket. [verified-source]

**Implications for a headless streamboat daemon:** [inferred]

1. Implement Pushkin, or a daemon left playing in another room will be silently killed by TIDAL's
   backend — or worse, will keep fighting the user's phone for the privilege.
2. Send `USER_ACTION` only on genuine user intent (a remote pressing play), never on autoplay or
   resume-after-network-hiccup, or the daemon will steal playback from the user's phone.
3. Surface "paused: playback started on <clientDisplayName>" through every control surface (MPRIS
   metadata, the WebSocket API, the MPD `idle`/status channel). Users will otherwise report it as a
   streamboat bug.
4. `clientDisplayName` is the string TIDAL shows for a device — streamboat should register a
   sensible display name for the daemon (hostname-derived), which is also what a future
   streamboat-native remote picker should show.

---

### 6. Feasibility and legal posture of the three Connect options

| Option | Technically possible? | Legal / distribution posture | Verdict |
| --- | --- | --- | --- |
| **(a) Bundle the proprietary `tidal_connect_application`** | Yes on ARM Linux; that is exactly what `edgecrush3r/tidal-connect` does | Redistributing an unlicensed binary extracted from device firmware, plus **iFi's device certificate**, is copyright infringement and identity misuse. GioF71 explicitly keeps the binary *out* of his repository (`ref:tidal-connect/README.md:3-4`) and only ships config. The whole chain survives on obscurity, not permission. | **Reject.** Incompatible with an open-source project that wants Flathub/distro packaging. |
| **(b) Reimplement the Connect target (or controller) protocol** | No precedent exists. Requires defeating `advobfuscator`, recovering the WebSocket protocol, **and** obtaining or forging a device certificate. | Reverse-engineering a security/identity mechanism is the clearest possible fit for TIDAL's consumer-terms prohibition on "circumventing or modifying … any security technology" (see `/home/user/streamboat/docs/research/tidal-api.md` §14). Unlike using the unofficial streaming API with a paid account, this is not a defensible gray area. | **Reject.** Highest legal risk in the whole project, for a feature that would be capped in quality anyway. |
| **(c) Skip Connect; offer a streamboat-native remote** | Yes, entirely under streamboat's control | No TIDAL IP involved; the daemon is just another logged-in subscriber client. | **Adopt.** See §12–§14. |

Supporting facts for (c): the Connect target route is capped at 16/44.1 since July 2024
[verified-source], and a streamboat daemon can play 24/192 [verified-source, via mopidy-tidal and
upmpdcli precedents]. Skipping Connect is not only safer, it produces a better product on the
dimension the owner cares about.

**What streamboat should still do about Connect** (cheap, honest, useful): [inferred]

- Document plainly, in README and in-app, that streamboat is not a TIDAL Connect target or
  controller, and why. Point users at the official app for handing off to Connect hardware.
- Do **not** advertise `_tidalconnect._tcp`, and do not attempt to spoof a Connect device — that
  would be impersonation.
- Optionally *list* discovered `_tidalconnect._tcp` devices in a diagnostics view, clearly labelled
  as "not controllable from streamboat". Low value; include only if free.
- Handle the consequence users will hit: if they start playback on a Connect speaker from the
  official app, streamboat's Pushkin socket will fire `PRIVILEGED_SESSION_NOTIFICATION` and
  streamboat must pause gracefully with a clear message.

---

### 7. Chromecast and AirPlay from the desktop app

Both are "send audio somewhere else on the LAN", and both put streamboat in the position of either
handing over a URL or re-transmitting decoded audio.

**Chromecast (sender).** Rust support exists: `cast-sender` ("a fully asynchronous implementation of
the Google Cast CASTV2 protocol"), `rust_cast` (protobufs taken from the Chromium Open Screen
mirror), and the multi-target `fcast-sender-sdk`. Discovery is mDNS `_googlecast._tcp` — the same
service type that appears in TIDAL's own `RemotePlaybackDevice.fullname` example. The Default Media
Receiver app id is `CC1AD845`. [documented-web: crates.io/lib.rs listings; verified-source for the
`_googlecast._tcp` example string]

The blocker is not just the protocol, it is the media URL — and even solving that only buys a lower
ceiling than streamboat's own daemon. A Cast receiver fetches the content itself, so streamboat would
have to give it either (i) the signed TIDAL CDN URL — which is time-limited, may carry DASH manifests
the Default Media Receiver cannot parse, and hands a third-party device a credentialed URL; or (ii) a
URL served by streamboat itself, i.e. streamboat becomes an HTTP re-server of TIDAL audio on the LAN.
Option (ii) is technically easy and is what Music Assistant does for its players, but it is a
re-transmission design and deserves an explicit decision. And even a working Cast sender caps
streamboat below its own headless daemon: Google Cast supports FLAC only up to 96 kHz/24-bit, and
24-bit at 176.4/192 kHz does not play on Chromecast Audio and can hang the device — the exact same
class of quality ceiling the report rejects the Connect binary for (§2.5). [confirmed, documented-web:
https://developers.google.com/cast/docs/media; the "TIDAL serves signed, expiring URLs" fact is
established in `/home/user/streamboat/docs/research/tidal-api.md`]

**AirPlay (sender).** Precedents: `philippe44/libraop` (RAOP/AirPlay v2 player + library, Windows /
macOS / Linux x86 and ARM), `music-assistant/airplay-cli` ("unified command-line binary for streaming
to AirPlay 1 (RAOP) and AirPlay 2 devices"), `akustikrausch/airplay2-sender-cpp` (encrypted
RAOP/RTSP, ALAC), and OwnTone's `src/outputs/airplay.c`. AirPlay is a *push* protocol: streamboat
would decode to PCM/ALAC and transmit. No credentialed URL leaves the machine, which makes it
cleaner than Cast on that axis, but it is still re-transmission of decoded audio. [documented-web]

**macOS gets AirPlay for free.** On macOS, selecting an AirPlay device as the system output device
routes CoreAudio output there with no code in streamboat. Same for Windows with some devices.
Cross-platform parity is the only reason to implement a sender at all. [inferred]

**Receiving** (streamboat as an AirPlay/Cast target) is out of scope: shairport-sync already covers
AirPlay 2 receiving on Linux/FreeBSD, and UxPlay covers mirroring. **A Chromecast receiver is not
technically impossible** — `rgerganov/shanocast` is a working open-source one built on Chromium's
Openscreen — but it passes Chrome's receiver authentication only by reusing **precomputed signatures
taken from AirReceiver**, i.e. another vendor's device credentials. That is the same "reuse someone
else's identity" move streamboat has already rejected for iFi's Connect certificate (§6), which is
the stronger and more honest reason to skip a Chromecast receiver — not that the protocol can't be
implemented. [refuted-and-corrected, documented-web: https://github.com/mikebrady/shairport-sync,
https://github.com/rgerganov/shanocast (`xakcop.com`, the original write-up, is blocked by this
environment's egress proxy — see §17/Sources)]

**Recommendation**: do not build Cast or AirPlay senders in v1. Build the *raw PCM pipe output*
(§11) instead — it feeds Snapcast, which already solves multiroom including AirPlay/Cast endpoints
via other software, at a fraction of the effort. Revisit Cast/AirPlay senders only if users ask, and
resolve the re-transmission question first (§16, open questions).

---

### 8. Headless precedents in detail

#### 8.1 mopidy-tidal — the full server precedent

`ref:mopidy-tidal` · https://github.com/EbbLabs/mopidy-tidal (formerly tehkillerbee) · Apache-2.0 ·
v0.3.13 · `requires-python = ">=3.12"` · `Mopidy>=3.0`, `tidalapi>=0.8.10`. [verified-source
`ref:mopidy-tidal/pyproject.toml`]

**Architecture.** mopidy-tidal is a *backend* only: it provides library, playlist, search and
playback providers to Mopidy. Everything user-facing comes from other Mopidy extensions. The
project's own nix example lists the realistic set: [verified-source `ref:mopidy-tidal/README.md`]

```nix
extensionPackages = [ mopidy-local mopidy-iris mopidy-mpd mopidy-mpris ] ++ [ mopidy-tidal ];
settings = { tidal.quality = "LOSSLESS"; tidal.playback_cache = true; };
```

So one backend yields, for free: an MPD server (mopidy-mpd), a web UI (Iris), MPRIS/D-Bus
(mopidy-mpris), and Mopidy's own HTTP/WebSocket JSON-RPC API. **That leverage — one backend, four
control surfaces — is the single most important lesson in this document.** [verified-source +
inferred]

**Config schema** (`ref:mopidy-tidal/mopidy_tidal/ext.conf`), verbatim: [verified-source]

```ini
[tidal]
enabled = true
quality = LOSSLESS            ; LOW | HIGH | LOSSLESS | HI_RES_LOSSLESS
auth_method = OAUTH           ; OAUTH | PKCE
login_server_port = 8989
lazy = false
login_method = AUTO           ; BLOCK | AUTO | HACK
playlist_cache_refresh_secs = 0
client_id =
client_secret =
playback_cache = false
playback_cache_max_entries = 1024
playback_cache_buffer_bytes = 16777216
```

**Headless login, three ways** — the hardest UX problem in headless mode, solved:
[verified-source `ref:mopidy-tidal/mopidy_tidal/backend.py`, `web_auth_server.py`, `login_hack.py`]

- `BLOCK` — block startup until login completes.
- `AUTO` — start unauthenticated, log in lazily.
- `HACK` — the **login hack**: while logged out, every library/search provider returns a dummy
  Track/Album/Artist whose *title* is the login instruction and whose *cover art URL* is a generated
  QR code of the login URL. Any MPD client renders it. No protocol extension, no companion app.
  `login_hack.py` builds these objects by introspecting the provider's return type annotations
  (`ObjectBuilder`, `width = height = 150`). **One detail worth copying carefully, not verbatim**:
  the QR is not generated locally. `_image_url()` builds
  `"https://api.qrserver.com/v1/create-qr-code/?" + urlencode(...)` — the cover art is a hotlinked
  image from a third-party remote service, which fails on an offline or firewalled box, i.e. exactly
  the headless deployment this feature exists for. `ref:tidalt`'s `qrterminal/v3` approach generates
  the QR locally and has neither problem; streamboat should render its own QR locally (in-band or in
  a terminal) rather than reuse this hotlink pattern. [confirmed + corrected, verified-source
  `ref:mopidy-tidal/mopidy_tidal/login_hack.py:109-115`]
- A tiny HTTP server on `login_server_port` (default 8989) serves a page with the login link and, for
  PKCE, a form: "Paste the response URL here". It binds `("", port)` — **all interfaces, no
  authentication** (`ref:mopidy-tidal/mopidy_tidal/web_auth_server.py`). Copy the idea, not the bind
  address.

**Stream resolution** (`ref:mopidy-tidal/mopidy_tidal/playback.py`): [verified-source]

- MPD-manifest tracks: write the manifest to `<cache_dir>/manifest.mpd` and return
  `file://<path>` for GStreamer to consume.
- BTS tracks: return `manifest.get_urls()[0]`.
- Pre-flight quality check: when configured for `hi_res_lossless`, test
  `"HIRES_LOSSLESS" in track.media_metadata_tags` and log a downgrade notice if absent.
- Logs `quality`, `bit_depth`, `sample_rate` (and `codec` for BTS) per track — exactly the telemetry
  a headless user needs.

**The caching proxy** (`ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/`): a threaded local HTTP relay
in front of `https://lgf.audio.tidal.com/` (`__init__.py:17-19`) with a SQLite chunk cache
(`cache.py`, entries finalized only on complete download) and `Range` support in `proxy.py` so
GStreamer can seek. `translate_uri` prefers a cached local URL, else `track.get_url()`, else falls
back to `as_stream()` when `URLNotAvailable` is raised (the PKCE case). Default buffer 16 MiB, 1024
entries. [verified-source]

**Pros**: enormous ecosystem leverage; battle-tested; Apache-2.0 so the ideas are freely copyable.
**Cons**: Python/GStreamer/Mopidy stack that streamboat is unlikely to adopt wholesale; coupling to
Mopidy's provider API; a hardcoded CDN hostname that will rot; the login-server bind is insecure.

**A legal-posture caveat the "copy this" recommendation must carry**: this caching proxy stores
**plain, playable HTTP chunks** of the CDN response in SQLite — a cache directory full of decrypted
FLAC, finalized once a download completes. That is a materially different design from the one
defensible precedent in this space, go-librespot's cache: "only the raw, still-encrypted files are
stored: a cached file is useless without a valid Spotify account, since the audio key is retrieved
again on every playback" (`cache.{enabled,dir,size_limit}`, off by default, 1 GB LRU when on). The
owner's stance is that streamboat is a player for subscribers, not a ripper, and a cache of plain
playable audio reads closer to the latter than the former. **Recommendation, revising the "reusable
artifacts" table below**: adopt go-librespot's posture — cache for jitter/seek in memory or a
short-lived buffer, and if a persistent cache is ever added, keep it opaque (encrypted or otherwise
useless without a live, authenticated session) rather than copying mopidy-tidal's SQLite proxy as-is.
[confirmed + gap, documented-web: https://raw.githubusercontent.com/devgianlu/go-librespot/master/README.md
(Audio cache section); verified-source `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/__init__.py`,
`cache.py`]

#### 8.2 tidalt — the "one binary, two modes" precedent

`ref:tidalt` · Go 1.26 · Apache-2.0 · BubbleTea TUI · FFmpeg decode + direct ALSA output.

Subcommand surface (`ref:tidalt/cmd/tidalt/main.go`, `daemon.go`, `play.go`): [verified-source]

| Command | Behaviour |
| --- | --- |
| `tidalt` | TUI. **If another instance already owns the MPRIS bus name, it opens in client mode** and forwards commands to the running daemon instead of starting a second player. |
| `tidalt daemon` | Headless: full playback engine + MPRIS2 server, BubbleTea run with `tea.WithoutRenderer()` and `tea.WithInput(nil)`. Prints "No audio device is opened until playback starts." |
| `tidalt play <url>` | Forwards a `tidal://` / `https://tidal.com/…` deep link over D-Bus to the running instance; if none, spawns a terminal emulator running the TUI with the URL queued. Terminal lookup order: `$TERMINAL`, then kitty, ghostty, alacritty, foot, wezterm, konsole, xfce4-terminal, gnome-terminal, xterm. |
| `tidalt setup --daemon` | Writes `~/.config/systemd/user/tidalt.service`, then `systemctl --user daemon-reload / enable / start`, and prints the status/logs/stop/disable commands. |
| `tidalt logout` | Revokes the token. |

The systemd unit template, verbatim (`ref:tidalt/cmd/tidalt/daemon.go`): [verified-source]

```ini
[Unit]
Description=tidalt — Tidal HiFi music player daemon
Documentation=https://github.com/Benehiko/tidalt
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart={{.Exec}} daemon
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=graphical-session.target
```

**IPC is MPRIS itself.** `ref:tidalt/internal/mpris/server.go` owns
`org.mpris.MediaPlayer2.tidalt` at `/org/mpris/MediaPlayer2`, implementing
`org.mpris.MediaPlayer2` and `org.mpris.MediaPlayer2.Player`, with full introspection XML. Single
instance is enforced by `conn.RequestName(busName, dbus.NameFlagDoNotQueue)` and checking for
`RequestNameReplyPrimaryOwner`; otherwise it returns `ErrAlreadyRunning`. A `Client` wrapper calls
`org.mpris.MediaPlayer2.Player.PlayPause` / `.Next` / `.Previous`, and a custom `SendURL` method
carries deep links. Logs go to `~/.local/share/tidalt/play.log` (0600, dir 0700). [verified-source]

**Headless login**: device flow with a QR code rendered in the terminal via
`github.com/mdp/qrterminal/v3`; the verification URL is built as
`"https://" + verificationUri + "?user_code=" + urlencode(userCode)`
(`ref:tidalt/internal/tidal/client.go:87-94`, `loginprint.go:395`). Session is stored in the system
keychain via `docker/secrets-engine`, falling back to an age-encrypted file. [verified-source]

**Limits**: MPRIS is a poor API for *browsing* (no search, no library) — tidalt's client mode is a
TUI that re-authenticates and does its own API calls, using MPRIS only for transport control. That
is the honest boundary of the MPRIS-as-IPC idea. [inferred from `main.go:149-163`]

#### 8.3 sone — local HTTP control done carefully

`ref:sone` (Tauri 2 + Rust) is a GUI app, but it ships two local servers worth copying:
[verified-source]

- **MCP server**: `axum` + `rmcp 1.7.0` (`StreamableHttpService`, stateful mode off, JSON responses).
  Binds **`127.0.0.1` only** — `let addr: SocketAddr = ([127, 0, 0, 1], port).into();` — default port
  5577, and the entire API lives under a path segment that is a random token:
  `format!("/{}/mcp", token)`. The token is a UUID v4 generated on first enable and persisted to
  settings (`ref:sone/src-tauri/src/mcp/mod.rs:30-37`, `mcp/server.rs:70-85`). Disabled by default
  (`mcp_enabled: false`).
- **OBS overlay server**: routes `GET /overlay`, `/overlay/state`, `/overlay/theme.css`,
  `/overlay/events` (SSE), default port 5578, host configurable including `0.0.0.0`, with a
  `Semaphore(MAX_SSE_CONNECTIONS)` limiting concurrent SSE clients and code that displays
  `127.0.0.1` when bound to the wildcard (`ref:sone/src-tauri/src/overlay/server.rs`).

**This is the security model streamboat should adopt for its control API, with one qualification**:
loopback by default, opt-in LAN bind, a generated token, an explicit connection cap. But sone's
specific choice of carrying the token *in the URL path* is sound for a machine-to-machine client (an
MCP client, a script) and weak for a human-facing surface — a URL-path token opened in a phone
browser lands in browser history, a shared-phone address bar, any `Referer` a third-party asset
sends, and reverse-proxy logs. **Recommendation**: use `Authorization: Bearer <token>` for API
clients, and for the web remote (§14/§17) issue a one-time pairing URL that immediately exchanges the
token for an `HttpOnly`, `SameSite` cookie scoped to the daemon's origin — never a token that persists
in the address bar. Also add `allow_origin` and optional `cert_file`/`key_file` to the control-API
config schema from day one (go-librespot's `server.{address,port,allow_origin,cert_file,key_file}` is
the model — note there is no `server.tls` key, TLS is configured by supplying `cert_file`+`key_file`).
[uncertain — design recommendation, not a fact-check target itself; documented-web:
https://raw.githubusercontent.com/devgianlu/go-librespot/master/README.md]

MPRIS in sone is `mpris-server = "0.9"` over `zbus = "5"` (`ref:sone/src-tauri/Cargo.toml:53,66`);
sone-windows adds `souvlaki = "0.8.3"` alongside `mpris-server` for Windows SMTC
(`ref:sone-windows/src-tauri/Cargo.toml:58,64`). *(Correction to the prior project survey, which
attributed Linux MPRIS in sone to souvlaki.)* [verified-source]

#### 8.4 tidal-hifi — a documented HTTP control API

`ref:tidal-hifi/src/features/api/swagger.json` (OpenAPI 3.1.0, `"version": "8.1.3"`) documents a
complete small player API. Current (non-deprecated) surface: [verified-source]

```
POST /player/play            POST /player/pause          POST /player/playpause
POST /player/next            POST /player/previous
POST /player/shuffle/toggle  POST /player/repeat/toggle
POST /player/favorite/toggle
PUT  /player/volume          # 0.0–1.0
PUT  /player/seek/absolute   PUT /player/seek/relative
GET  /current                GET /current/image          GET /current/audio-quality
GET  /health
GET/POST /settings/skipped-artists          POST /settings/skipped-artists/delete
POST/DELETE /settings/skipped-artists/current
GET/POST /settings/skipped-tracks           POST /settings/skipped-tracks/delete
POST/DELETE /settings/skipped-tracks/current
```

Deprecated legacy verbs (`GET /play`, `/pause`, `/next`, `/previous`, `/playpause`,
`/favorite/toggle`, `/image`) remain for compatibility. Note the shape: verbs are `POST`, state
setters are `PUT`, reads are `GET`, and there is a `/health` endpoint. **Copy this naming almost
verbatim** — it is the closest thing to a de-facto convention in this niche, and matching it means
existing user scripts and Stream Deck integrations port over. [verified-source + inferred]

#### 8.5 upmpdcli — TIDAL over UPnP, and the renderer whitelist problem

upmpdcli implements a UPnP/OpenHome Media Renderer on top of MPD, and separately a Media Server with
Python plugins that gateway external services — including TIDAL, built on python-tidal. GioF71
contributed a rewritten TIDAL plugin. **Version correction**: an earlier draft of this document dated
the plugin at 0.8.3 (2025-04-07); the current upmpdcli-docker version table reads `release|tidal|0.8.12`
and `master|tidal|0.8.16`/`edge|tidal|0.8.16` as of this check (2026-09) — a version number this stale
in a design doc reads as current when it isn't, so treat any specific plugin version as a snapshot,
not a fact to build against. Configuration is by environment variables including
`TIDAL_AUDIO_QUALITY` (`LOW|HIGH|HI_RES|HI_RES_LOSSLESS`) and pre-generated tokens
(`TIDAL_TOKEN_TYPE`, `TIDAL_ACCESS_TOKEN`, `TIDAL_REFRESH_TOKEN`, `TIDAL_EXPIRY_TIME`).
[confirmed + corrected, documented-web: https://raw.githubusercontent.com/GioF71/upmpdcli-docker/main/README.md,
discussion #281, https://github.com/GioF71/audio-tools/blob/main/media-servers/tidal-hires/README.md]

The interesting failure mode: in `HI_RES_LOSSLESS` mode the plugin serves DASH manifests, and **most
UPnP renderers cannot play a manifest**. GioF71's answer is a *whitelist* — MPD+upmpdcli,
gmrender-resurrect, and some WiiM Pro/Pro Plus devices (on the `master`/`edge` images) get hi-res;
everything else is served 16/44.1 for compatibility. **The gate is keyed on User-Agent, and it has a
documented off switch**: the environment variable is `TIDAL_ENABLE_USER_AGENT_WHITELIST`; setting it
to `no` serves hi-res to any renderer, which the maintainer explicitly recommends as a way to
discover additional compliant devices worth whitelisting. That is the difference between a hardcoded
device list and a testable policy — worth copying the *policy shape*, not just the list of known-good
renderers. [confirmed + gap, documented-web:
https://raw.githubusercontent.com/GioF71/upmpdcli-docker/main/README.md; `ref:tidal-connect/README.md`
corroborates the renderer list]

**Lesson for streamboat**: if streamboat ever exposes a UPnP renderer or serves streams to third-party
devices, the DASH-manifest-vs-plain-URL split becomes a device compatibility matrix. Prefer to decode
locally and emit PCM (Snapcast model) over shipping manifests to foreign renderers. [inferred]

#### 8.6 Lyrion (LMS) + Squeezelite

`michaelherger/lms-plugin-tidal` integrates TIDAL with Lyrion Music Server (ex-Logitech Media
Server), playing to Squeezebox hardware and Squeezelite software players. `ref:tidal-connect/README.md`
(2024) states it supports LOSSLESS but not TIDAL HiRes. Hi-res FLAC friction is real but the picture
is more mixed than "an ongoing problem": issue #35 ("Hi-Res: use of FLAC/MQA") is **closed**, #89
("can't play 24 bit files") is **open**, and there are at least four more data points — #88 "Hires
FLAC" (closed), #98 "only cd quallity no HiRes" (closed), #100 "Attempt at Hires Lossless (Max) and
Dolby Atmos support" (closed, suggesting hi-res work has since landed), #113 "forced atomos stream
for some tracks" (open). **Keep this flagged as genuinely uncertain rather than stating either
"broken" or "fixed"** — the issue tracker shows active, incremental progress, not a settled state.
[uncertain, confirmed via GitHub issue search on `michaelherger/lms-plugin-tidal`; status as of 2026
**[unverified]** beyond what the issue tracker shows]

Relevance: mostly as a warning that hi-res + a legacy streaming ecosystem is where these projects
break, and as evidence that a *server with many thin players* is a durable architecture in this space.

#### 8.7 Music Assistant — the "server + many player providers" maximum

Python server, official distribution via a Home Assistant add-on or Docker, dev server at
`http://localhost:8095`, bundling ffmpeg 6.1+ (not published to PyPI as a Python package). Music
providers include TIDAL; player providers include Sonos, Chromecast, AirPlay, Squeezelite, DLNA,
Snapcast and, since 2.8 (March 2026), **Sendspin** including "Sendspin Bridges" that wrap existing
AirPlay/Cast devices so they can join a synchronized Sendspin group. **Port correction**: the
built-in snapserver's control port is **1705** (JSON-RPC, bound to `127.0.0.1` by default —
`DEFAULT_SNAPSERVER_IP = "127.0.0.1"`, `DEFAULT_SNAPSERVER_PORT = 1705` in
`music_assistant/providers/snapcast/constants.py`), not 1780 as an earlier draft of this document
said; 1780 is Snapcast's own HTTP/Snapweb port (1788 for SSL), a different service entirely.
[confirmed + corrected, documented-web:
https://raw.githubusercontent.com/music-assistant/server/dev/README.md; music-assistant GitHub
discussions #4200, #5354; blog post 2026-03-25 — note music-assistant.io itself was blocked by the
egress proxy, so the site's own pages are second-hand here]

This is the maximal version of the architecture streamboat is choosing among. It is also a warning:
Music Assistant's value is breadth of *player* support, which is a large, permanent maintenance
surface. streamboat should not compete there; it should be a good citizen of it (be feedable *by*
Snapcast/Sendspin, not a reimplementation of them). [inferred]

---

### 9. The MPD ecosystem

**Protocol facts, re-verified against the primary spec** (`mpd.readthedocs.io` and `musicpd.org` are
blocked by this environment's egress proxy, but the spec's own source is not:
`raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/doc/protocol.rst` and `doc/user.rst` — cite
these, not search summaries, and drop the earlier "medium confidence, re-verify" hedge on the facts
below since they are now confirmed against the spec text itself):

- Default TCP port **6600** ("If no port is specified, the default port is 6600" — `doc/user.rst`);
  Unix-socket listening is also supported.
- Line-oriented text protocol; server greets with `OK MPD <version>`; a command "returns `OK` on
  completion or `ACK <error>` on failure", full form `ACK [error@command_listNum] {current_command}
  message_text`, e.g. `ACK [2@1] {play} Bad song index`.
- Command lists: `command_list_begin` / `command_list_ok_begin` … `command_list_end`. **`idle` and
  `noidle` are explicitly not allowed inside a command list.**
- **`idle [SUBSYSTEMS…]`** blocks until something changes and replies `changed: <subsystem>` — the
  full, confirmed subsystem list is `database`, `update`, `stored_playlist`, `playlist`, `player`,
  `mixer`, `output`, `options`, `partition`, `sticker`, `subscription`, `message`, `neighbor`, `mount`.
- **`status` fields** (the fields an MPD client actually renders, confirmed against the spec):
  `partition`, `volume`, `repeat`, `random`, `single` (`0|1|oneshot`), `consume` (`0|1|oneshot`),
  `playlist` (a monotonically increasing 31-bit **queue version** — the standard way a client detects
  a changed queue without diffing), `playlistlength`, `state`, `song`/`songid`, `nextsong`/`nextsongid`,
  `elapsed`, `duration`, `bitrate`, `xfade`, `mixrampdb`, `mixrampdelay`, `audio`
  (`samplerate:bits:channels` — **this is how an MPD client displays 24-bit/192 kHz today; it is the
  honest-quality-reporting hook for MPD mode**), `updating_db`, `error`, `lastloadedplaylist`.
- **Capability advertisement and version negotiation exist and matter for streamboat's own protocol
  design** (see §15/§17): `commands`/`notcommands` list what the current connection may call;
  `protocol` / `protocol available` / `protocol enable {FEATURE}` / `protocol clear` negotiate
  optional behaviour additively. This is the "tolerant" alternative to Sendspin's exact-match
  versioning (§11) and is a real design choice to surface, not silently pick.
- **Security is not "a single plaintext password and no encryption", as an earlier draft of this
  document said.** MPD has four access-control layers: `default_permissions` (the baseline for
  unauthenticated clients); `local_permissions` (permissions granted to clients connecting over a
  local Unix socket); `host_permissions` (per-IP or per-CIDR, e.g. `host_permissions "192.168.1.0/24
  read,control"`); and any number of `password "secret@permissionset"` entries, each carrying its own
  permission set. **Only the transport-encryption half of the original claim holds**: "the password
  option is not secure: passwords are sent in clear-text over the connection, and the client cannot
  verify the server's identity." **This changes the risk assessment for an MPD-compatible listener**:
  bind it to a Unix socket or loopback with `local_permissions`, and it needs no shared plaintext
  password at all — the weak link is specifically a *network* password, not MPD's access control in
  general.
- Binary responses exist for `albumart` / `readpicture`: a `binary: <n>` header line, then exactly
  `<n>` bytes, then the completion line; a client-settable `binarylimit SIZE` (since MPD 0.22.4) caps
  chunk size.
- **Partitions**: one MPD process can present multiple frontends with separate queue/player/outputs.

**Clients streamboat would inherit for free**: ncmpcpp, mpc, ncmpc (terminal); Cantata (Qt desktop);
MALP / M.A.L.P. (Android); numerous iOS clients; mpDris2 (bridges MPD → MPRIS). [documented-web]

**Two precedents for implementing a *subset* — and they prove more than "transport control only":**

- **Mopidy-MPD** implements the MPD protocol over a *non-file* backend, and its music-database
  command set (`count`, `find`, `findadd`, `list`, `listall`, `listallinfo`, `listfiles`, `lsinfo`,
  `search`, `searchadd`, `searchaddpl`, `update`, `rescan`) is the proof that catalogue browsing maps
  cleanly onto a streaming service. `lsinfo` walks `context.browse(uri, recursive=False, lookup=True)`
  and emits `("directory", path)` for internal nodes plus translated tracks for leaves, with stored
  playlists surfaced at the root — a service catalogue mapped straight into MPD's directory model.
  This is exactly why MALP and ncmpcpp can browse a TIDAL library through mopidy-tidal today, and it
  directly contradicts treating "MPD fits a streaming catalogue awkwardly" as a reason to drop
  browsing from streamboat's own MPD subset (§14/§17 revise this). **The repository's own maintenance
  status is worth carrying alongside the precedent**: it currently states it is "kept on life support
  by the Mopidy core developers" with a "Maintainer wanted" notice — sound design, uncertain
  long-term maintenance. [confirmed + corrected, documented-web:
  https://raw.githubusercontent.com/mopidy/mopidy-mpd/main/src/mopidy_mpd/protocol/music_db.py;
  https://github.com/mopidy/mopidy-mpd]
- **Nuclear** — a desktop GUI player with a built-in MPD-compatible server that works with `mpc`,
  `ncmpcpp` and `mpDris2`, implementing "the subset of the protocol needed for playback control, queue
  management, and real-time notifications", with library browsing and stored playlists explicitly not
  supported (the opposite scoping choice from Mopidy-MPD — a genuine design fork worth naming
  explicitly in streamboat's own decision). **Port behaviour, recovered via search index since
  `docs.nuclearplayer.com` is blocked**: it binds `127.0.0.1:6600` only, and if 6600 is already taken
  (a real collision on a box already running `mpd` — exactly the audiophile Linux target streamboat
  cares about) it tries 6601, 6602, … up to 6609. It supports `command_list_begin` /
  `command_list_ok_begin` / `command_list_end` and emits change notifications for playback state,
  current track, queue contents, volume, and repeat/random/single mode. **This port-fallback behaviour
  is the missing piece of streamboat's own MPD-listener design** — bind loopback, and fall back through
  a small port range rather than failing outright when 6600 is occupied. [documented-web:
  docs.nuclearplayer.com/nuclear/integrations/mpd-server — page itself blocked, content recovered via
  search index; **re-verify the exact command list once the page is reachable**]

**Rust tooling**: `mpd_protocol` and `mpd` on crates.io are **client-side**; `M0Rf30/rmpd` is a
from-scratch MPD *server* in Rust ("Modern, high-performance MPD server written in pure Rust with DSD
support, multi-room audio, and extensible plugin architecture", 244 commits, 15 stars — young, not a
drop-in dependency) aiming at protocol compatibility with a plugin architecture. Go has `fhs/gompd`
(client). There is no drop-in "embed an MPD server in your app" library for either language —
implementing the subset is a hand-written parser plus a state machine, on the order of a few thousand
lines. [confirmed, documented-web + inferred]

---

### 10. UPnP/DLNA renderers

- `upmpdcli` turns MPD into a UPnP AV / OpenHome renderer, and separately gateways TIDAL as a Media
  Server (§8.5). `gmrender-resurrect` is the other renderer that handles TIDAL hi-res via that
  plugin. [documented-web]
- BubbleUPnP (Android) and mConnect (iOS/Android) are *control points* with built-in TIDAL support;
  BubbleUPnP added hi-res FLAC in late November 2023. `ref:tidal-connect/README.md` recommends this
  path to hi-res users. [verified-source, citing a Reddit post]
- **Does any UPnP renderer expose TIDAL natively?** No — the pattern is always *control point or
  media server* holds the TIDAL credentials, and the renderer just plays a URL. The renderer never
  knows about TIDAL. [inferred, consistent with everything above]

**For streamboat**: implementing a UPnP renderer is low value (upmpdcli exists, MPD exists, and
renderers can't parse DASH manifests without a whitelist). Implementing a UPnP *media server* that
gateways TIDAL would duplicate upmpdcli's plugin. Skip both. [inferred]

---

### 11. Multiroom: Snapcast today, Sendspin tomorrow

**Snapcast** (https://github.com/snapcast/snapcast) is a client/server synchronized multiroom
player: the server reads PCM from a source, timestamps and encodes it, and clients play in sync —
"typically the deviation is below 0.2ms". [documented-web]

- **Ports**: 1704 (TCP, binary audio + time sync to clients), 1705 (TCP JSON-RPC control), 1780
  (HTTP + WebSocket JSON-RPC + the Snapweb UI). Bind addresses configurable via
  `--stream-bind-address` / `--control-bind-address` / `--http-bind-address` or the `[tcp-streaming]`,
  `[tcp-control]`, `[http]` config sections. [documented-web]
- **Sources**: named pipe (classically `/tmp/snapfifo`), ALSA capture, TCP, process stdout, plus
  purpose-built `librespot` / `airplay` source types in some builds.
- **Codecs**: PCM, FLAC (default), Vorbis, Opus.
- **Clients**: Linux, macOS, FreeBSD, Android, Windows, Raspberry Pi, and an ESP32 port.
- Requires accurate clocks (NTP/chrony) — see the Pi time-sync note in §16.
- **snapserver advertises itself over mDNS with four service types** — directly relevant to
  streamboat's own discovery design (§17): `_snapcast._tcp` (streaming, 1704), `_snapcast-ctrl._tcp`
  (control, 1705), `_snapcast-http._tcp` (1780), `_snapcast-https._tcp` (1788). [confirmed,
  documented-web: https://raw.githubusercontent.com/badaix/snapcast/develop/server/etc/snapserver.conf]
  The canonical repository is `badaix/snapcast` — `snapcast/snapcast` is a redirect.

**The integration cost for streamboat is one output backend, with one real trade-off**: an option like
`--output pipe:/tmp/snapfifo` (or `--output stdout`) emitting interleaved PCM at a fixed format.
librespot's `pipe` backend and go-librespot's "raw named pipe for custom routing" are the exact
precedent. This buys synchronized multiroom, ESP32 endpoints, and Home Assistant integration for
almost nothing — but a FIFO carries no format metadata, and Snapcast's stream sources pin a *fixed*
sample format per stream (`sampleformat`, e.g. `48000:16:2`); go-librespot's own docs warn the pipe
output must match: "go-librespot uses a sample rate of 44100 for the pipe output, you can either
configure this globally for snapcast or specify it only for the go-librespot source." **So the pipe
output is mutually exclusive with bit-perfect variable-rate output** — the daemon must resample every
track to one fixed format to feed it, which is a real cost against the "24/192 bit-perfect beats the
Connect target" product argument (§2.5) and belongs in the design as an explicit output *mode*, not a
free addition. On Linux, a FIFO in `/tmp` on a recent kernel may also need `fs.protected_fifos=0`, and
Snapcast's pipe source takes a `mode=create|read` option worth exposing. Still **very high
leverage-per-line-of-code**, just not free. [uncertain→resolved: pipe output forces a fixed sample
rate/bit depth, documented-web: https://raw.githubusercontent.com/badaix/snapcast/develop/doc/configuration.md]

**Snapcast integration should not stop at a bare PCM pipe** — a bare FIFO produces sound with no
title, artist, cover art or transport control anywhere in Snapweb or any Snapcast client, which is a
poor fit for "simple but beautiful". Snapcast defines a **stream plugin** protocol for exactly this:
snapserver launches an executable per stream and speaks newline-delimited JSON-RPC 2.0 over its
stdin/stdout, configured as `source = pipe:///tmp/snapfifo?name=streamboat&controlscript=meta_streamboat.py`
(relative script paths resolve under `/usr/share/snapserver/plug-ins`; snapserver always passes
`--stream=<id>` and, when HTTP is enabled, `--snapcast-host`/`--snapcast-port`). The plugin implements
`Plugin.Stream.Player.Control` (`play`, `pause`, `playPause`, `stop`, `next`, `previous`,
`seek{offset}`, `setPosition{position}`), `Plugin.Stream.Player.SetProperty` and
`Plugin.Stream.Player.GetProperties`, and may emit `Plugin.Stream.Player.Properties`,
`Plugin.Stream.Log` and `Plugin.Stream.Ready`; capabilities are booleans (`canGoNext`,
`canGoPrevious`, `canPlay`, `canPause`, `canSeek`, `canControl`). Shipped examples include
`meta_mpd.py`, `meta_mopidy.py` and `meta_go-librespot.py` — a `streamboat snapcast-plugin`
subcommand following the same shape is a few hundred lines and turns Snapweb into a complete remote,
not just a speaker. Separately, snapserver can also *launch* streamboat itself via a `process://`
source (with `params`/`idle_threshold`/`wd_timeout`), which can replace a separate systemd unit on a
box whose only job is feeding Snapcast. [confirmed + gap, documented-web:
https://raw.githubusercontent.com/badaix/snapcast/develop/doc/json_rpc_api/stream_plugin.md;
https://raw.githubusercontent.com/badaix/snapcast/develop/doc/configuration.md]

**Sendspin** (formerly "Resonate"), an Open Home Foundation protocol, is the 2026 development to
watch: [documented-web, from the spec repo README at
https://raw.githubusercontent.com/Sendspin/spec/main/README.md]

- Core version 1, exact-match versioning.
- Transport: WebSocket, and explicitly plain `ws://` — "Confidentiality and integrity are provided
  end to end by the Noise layer inside the WebSocket payloads."
- Discovery: `_sendspin._tcp.local.` for server-initiated (client listens, recommended port **8928**)
  and `_sendspin-server._tcp.local.` for client-initiated (server listens, recommended port **8927**).
- Framing: binary frames after the Noise handshake; the first byte is the message type. **Correction
  to an earlier draft of this document, which skipped two IDs**: the confirmed table is 0 = JSON,
  1 = fragmentation, **2 = Pairing, 3 = Reserved**, 4–7 player, 8–11 artwork, 12–15 source, 16–23
  visualizer, 24–191 reserved for future roles, 192–255 custom roles.
- Codecs are configuration, not mandate — `opus`, `flac`, `pcm` are named as stream-config values.
- Clock sync: clients "MUST use the time-filter algorithm" (a two-dimensional Kalman filter,
  https://github.com/Sendspin-Protocol/time-filter) to translate microsecond server timestamps.
- Spec licence: **not stated in the spec README**. [unverified]
- Shipped in Music Assistant 2.8 (2026-03-25) including bridges that wrap AirPlay/Cast devices.
  Music Assistant's own endpoint is `ws://<server>:8927/sendspin`. [documented-web]

**Sendspin's pairing and identity design directly answers open question 6 (§18) and stage 1.2's
"a short pairing code exchanged for a long-lived token", which this report otherwise leaves
unspecified**: [confirmed + gap, documented-web:
https://raw.githubusercontent.com/Sendspin/spec/main/README.md — Definitions, Encryption, Cipher
Suites, Rehandshake, Pairing Token sections]

- Noise pattern `KKpsk2` with Curve25519 static keys serving as `client_id`/`server_id` (base64url,
  43 characters, persistent across reboots).
- A 32-byte long-term PSK established during pairing, mixed into every subsequent handshake.
- A **"Sentinel PSK"** fallback for unpaired sessions, so an unpaired client can still connect at a
  reduced trust level rather than being refused outright.
- **In-band rehandshake**: a session already in transport mode can be promoted to paired without
  dropping the WebSocket — no reconnect-and-reauthenticate round trip.
- **QR-code pairing tokens**: a version-1 token, a 24-byte code, 39 body characters — i.e. a fully
  specified, screen-free pairing flow for exactly the headless-box scenario streamboat's own stage
  1.2 needs.
- The **server** is always the Noise initiator, regardless of which side opened the TCP/WebSocket
  connection — worth carrying into streamboat's own protocol since it removes an asymmetry question.

Sendspin is *architecturally what streamboat's own remote protocol would look like* (mDNS + WebSocket
+ typed binary frames + clock sync + a fully specified pairing scheme). If it gains adoption,
implementing a Sendspin *source* would let streamboat feed a whole ecosystem of endpoints; even if it
does not, its pairing design is worth copying wholesale rather than reinventing streamboat's own.
Track it; do not bet v1 on it. [inferred]

---

### 12. Daemon analogues from other services

| Project | Language | Control surfaces | Discovery / handoff | Notes |
| --- | --- | --- | --- | --- |
| **librespot** | Rust | Library **and** binary; the binary registers as a Spotify Connect receiver | Zeroconf (Spotify Connect) | **Correction to an earlier draft**: librespot is reused as a linked *crate* by ncspot and spotifyd, but **not** by Snapcast — Snapcast's own docs describe its librespot source as "launches librespot and reads audio from stdout", i.e. it spawns the binary as a subprocess and reads a pipe, the same integration shape this report recommends for streamboat at stage 0, not the library-linking pattern. [confirmed + corrected, documented-web: https://raw.githubusercontent.com/librespot-org/librespot/dev/README.md; https://raw.githubusercontent.com/badaix/snapcast/develop/README.md] |
| **spotifyd** | Rust (on librespot) | MPRIS (`org.mpris.MediaPlayer2.spotifyd.instance$PID`) — **exposed only once spotifyd becomes the active playback device**; a separate, always-present **`rs.spotifyd.Controls` D-Bus interface on the `rs.spotifyd.instance$PID` bus name** exposes `TransferPlayback`/`VolumeUp`/`VolumeDown` even when *not* the active device; `--onevent` hook to run an external command; `--dbus-type system` for headless boxes with no session bus; optional Secret Service keyring | Spotify Connect | [confirmed, documented-web: https://raw.githubusercontent.com/Spotifyd/spotifyd/master/docs/src/advanced/{dbus,mpris,hooks}.md — reachable at this raw path even though `docs.spotifyd.rs` itself is blocked] |
| **go-librespot** | Go | **REST API + WebSocket events**; MPRIS; `GET /auth/code` during device-auth, "served for as long as the daemon is waiting, so a frontend can show them instead of asking the user to read the logs" | `zeroconf_backend` = `builtin` or `avahi`, `zeroconf_enabled`, `zeroconf_port` | Config covers `audio_backend` (alsa, pipe, pulseaudio, audio-toolbox, wasapi), `bitrate`, `volume_steps`, `external_volume`, normalisation (`normalisation_pregain`, `normalisation_use_album_gain`), `crossfade_duration`, `server.{enabled,address,port,allow_origin,cert_file,key_file}` — **there is no `server.tls` key**, as an earlier draft of this document implied; TLS is configured by supplying `cert_file`+`key_file` — and `cache.{enabled,dir,size_limit}` (only the still-encrypted file is cached; the audio key is re-fetched on every playback, see §8.1's caching caveat). librespot-java development was retired in its favour. [confirmed + corrected, documented-web: https://raw.githubusercontent.com/devgianlu/go-librespot/master/README.md] |
| **ncspot** | Rust | TUI; embeds librespot as a library; **also** a Unix-domain socket (Windows: named pipe) at the platform runtime directory, newline-delimited: plain command words in (`play`, `playpause`), newline-delimited JSON status out after every state change | — | Proof that a TUI/GUI and a daemon can share one engine crate — **and a second, cheaper proof that a same-host control surface doesn't need HTTP at all**: `ncspot info` prints the socket location; documented uses include controlling a detached session in tmux, status bars, and startup scripts. [confirmed + gap, documented-web: https://raw.githubusercontent.com/hrkfdn/ncspot/main/doc/users.md lines 191-235] |
| **Roon** | proprietary | Core / Remote / Bridge three-tier; endpoints implement RAAT ("Roon Advanced Audio Transport"), up to 32-bit/768 kHz PCM and DSD512; DAC vendors must implement Roon's Endpoint Code + RAAT to be "Roon Ready" | proprietary | The commercial version of the same idea; strictly closed. Music Assistant users have asked for RAAT support and cannot have it. [documented-web] |
| **tidalt** | Go | MPRIS + a client-mode TUI over D-Bus | none (single host) | §8.2 [verified-source] |
| **mopidy(-tidal)** | Python | MPD, HTTP/WS JSON-RPC, MPRIS, Iris web UI — all via separate extensions | none | §8.1 [verified-source] |

**The code-sharing pattern is unanimous, and librespot's own internal shape is worth copying, not
just its existence**: a *library* holds session/auth/catalogue/playback, and thin front-ends (daemon,
TUI, GUI) depend on it — but librespot itself (0.8.0, Rust edition 2024, MIT) is not one monolithic
core, it is a **workspace of separate crates**: `core`, `audio`, `playback`, `metadata`, `protocol`,
`oauth`, `discovery`, `connect`, plus the top-level binary. Note in particular that OAuth and zeroconf
*discovery* are their own crates, and the Connect-receiver logic lives in its own `connect` crate
rather than inside the player — the exact boundary question streamboat's "split `streamboat-core` out
first" decision (§18) leaves unspecified. Default features are `native-tls`, `rodio-backend`,
`with-libmdns`, with a documented `rustls` alternative "for avoiding external OpenSSL dependencies,
reproducible builds, or when targeting platforms where native TLS dependencies are unavailable or
problematic (musl, embedded, static linking)" — directly relevant to shipping a static Pi binary
(§16). A second precedent this report previously omitted: **Psst** ships `psst-core` ("Spotify TCP
session, audio file retrieval, decoding, audio output, playback queue") plus `psst-gui`, a design
*inspired by* librespot rather than depending on it — a second real-world answer to "how do you split
the core" alongside ncspot's "just link the crate". [confirmed + gap, documented-web:
https://raw.githubusercontent.com/librespot-org/librespot/dev/Cargo.toml;
https://github.com/jpochyla/psst/blob/main/README.md]

Mopidy→{mopidy-mpd, mopidy-mpris, Iris}, tidalt→{TUI, daemon}, TidalSwift→{TidalSwiftLib, app} follow
the same shape at a coarser grain. streamboat should define `streamboat-core` as the first artifact,
before deciding on any UI — and should look at librespot's crate boundaries, not just its slogan, when
deciding where `streamboat-core` ends and `streamboat-server`/`streamboat-connect`(-analogue) begins.
[verified-source + documented-web]

**The control-surface pattern splits into (at least) three, not two — an earlier draft of this
document undersold MPRIS**:
- *Transport-only IPC* (MPRIS / D-Bus): trivial, standard, gets media keys and playerctl for free.
  **Correction**: MPRIS is not limited to "transport + metadata only". The full spec defines four
  interfaces — `org.mpris.MediaPlayer2` (Root), `.Player`, `.TrackList` and `.Playlists` — and
  `mpris-server` 0.9, the crate this report already recommends and that `ref:sone` already depends
  on, implements all of them: `TrackListInterface` (`GetTracksMetadata`, `AddTrack`, `RemoveTrack`,
  `GoTo`, `TrackListReplaced`) and `PlaylistsInterface` (`GetPlaylists`, `ActivatePlaylist`). So MPRIS
  *can* express the queue and stored-playlist activation for free, before any HTTP API exists — what
  it genuinely cannot express is catalogue **search** or hierarchical **browsing**. tidalt's
  MPRIS-as-IPC wall (§8.2) is a scoping choice (it implements only Root+Player), not a ceiling MPRIS
  itself imposes. [confirmed + corrected, documented-web:
  https://raw.githubusercontent.com/SeaDve/mpris-server/main/README.md]
- *Same-host, no-listener IPC* (Unix domain socket / named pipe): trivial, no port, no token, no TLS
  question — filesystem permissions are the auth model. ncspot's socket (above) is the direct
  precedent for exactly the `streamboat play <url>` / `streamboat status` / same-host-GUI traffic this
  report otherwise routes through the HTTP API by default. Recommend this as the v0 control surface
  for the CLI and same-host GUI, with the HTTP/WS API re-exposing the same command/event vocabulary
  over the network in v1 (Rust's `interprocess` crate covers both Unix sockets and Windows named
  pipes). [gap, documented-web: https://raw.githubusercontent.com/hrkfdn/ncspot/main/doc/users.md]
- *Rich IPC* (HTTP + WebSocket JSON): expresses everything, needs auth, needs a client. go-librespot,
  Mopidy, Music Assistant, tidal-hifi, sone all landed here.

streamboat needs **all three**: MPRIS (now including queue/playlist) for OS integration, a Unix
socket for same-host scripting and the CLI, and a rich local API for real remotes. [inferred]

---

### 13. Headless authentication UX

The device-code (RFC 8628) flow is the natural headless login and every headless precedent uses it.
Concretely, from the reference set: [verified-source unless noted]

1. **Terminal + QR** — `ref:tidalt`: build `https://<verificationUri>?user_code=<userCode>`, render it
   with `qrterminal/v3` at error-correction level M, and also print the URL and code.
2. **Local web page** — `ref:mopidy-tidal/mopidy_tidal/web_auth_server.py`: an HTTP page titled
   "TIDAL Web Auth" with "KEEP THIS TAB OPEN", a link to the TIDAL login page, and — for PKCE — a
   form to paste the full redirect URL ("Probably a 'Oops / Not found' page, nevertheless we need the
   whole URL as is"). Port 8989 by default.
3. **In-band prompt** — mopidy-tidal's login hack (§8.1): show the login instruction *as content*
   inside whatever client the user already has open. Applies directly to an MPD-compatible mode.
4. **Pre-provisioned tokens** — upmpdcli's docker images accept `TIDAL_TOKEN_TYPE`,
   `TIDAL_ACCESS_TOKEN`, `TIDAL_REFRESH_TOKEN`, `TIDAL_EXPIRY_TIME` as environment variables so a
   token generated on a desktop can be dropped into a headless box. [documented-web]

**Secret storage on a headless box is the hard part.** A Linux server has no unlocked keyring.
tidalt's answer: system keychain via `docker/secrets-engine` with an **age-encrypted file fallback**
at `~/.config/tidalt/secrets`. sone's answer: settings JSON encrypted with a master key in the OS
keyring, file fallback at `~/.config/sone/sone.key`. mopidy-tidal's answer: a plain JSON token file
at `/var/lib/mopidy/tidal/tidal-<session_type>.json`. streamboat needs the keyring-with-file-fallback
design, and the file must be 0600 in a 0700 directory. [verified-source]

**Gap this report previously left as a one-liner: pairing a *phone or GUI* to a headless daemon that
has no screen at all.** Stage 1.2 (§17) says only "a short pairing code exchanged for a long-lived
token", with no mechanism, no expiry, no revocation story, and no resolution of the collision between
"every listener defaults to loopback" and "the mobile app controls the daemon". No single precedent in
the reference set solves this end to end; assemble it from the pieces that do exist: [gap,
documented-web: https://raw.githubusercontent.com/devgianlu/go-librespot/master/README.md
(`GET /auth/code`, `server.{allow_origin,cert_file,key_file}`); verified-source
`ref:tidalt/internal/tidal/loginprint.go`; verified-source
`ref:mopidy-tidal/mopidy_tidal/web_auth_server.py`]

- go-librespot exposes its device-auth link and code over `GET /auth/code` "for as long as the daemon
  is waiting, so a frontend can show them instead of asking the user to read the logs", and supports
  TLS on the same listener via `server.cert_file`/`server.key_file` plus CORS via
  `server.allow_origin`.
- `ref:tidalt` renders a QR in the terminal with `qrterminal/v3` — works over SSH, no companion
  service needed.
- `ref:mopidy-tidal` serves a fixed-port login page — the pattern to copy, not its unauthenticated
  wildcard bind (§8.1).
- **Decisions the owner still needs to make, not facts to look up**: (1) daemon-initiated pairing
  (`streamboat pair` over SSH prints a short-lived code/QR) vs. client-initiated (a client asks, the
  daemon logs a code the user confirms out-of-band); (2) one named, revocable token per client
  (`streamboat tokens list|revoke`) vs. a single shared secret; (3) plain HTTP on the LAN — which
  makes the web remote a **non-secure context** in the browser sense (no service worker, no
  installable PWA, no Web Crypto — see the secure-contexts gap in §17) — vs. a self-signed certificate
  with a permanent browser warning vs. loopback-only plus an SSH tunnel. A URL-path token (§8.3) is
  unsuitable for whichever of these ends up rendered in a phone browser.

**Token and session identity when a GUI and a daemon share one host or one account is also
unspecified, and it is the first thing that breaks in real multi-process use.** Partial answer,
verified in the code: python-tidal's `Session.token_refresh()` posts `grant_type=refresh_token` with
`client_id`/`client_secret` and updates only `access_token`, `expiry_time` and `token_type` — it never
replaces `self.refresh_token`, so the refresh token is long-lived, not rotated on use, and two
processes holding the same refresh token do not invalidate each other
(`ref:python-tidal/tidalapi/session.py:717-750`). Still to decide and document: (a) one shared token
file with an advisory lock and a single writer, vs. separate logins per role (GUI vs. daemon); (b)
whether the daemon uses a different `client_id` than the GUI — the Connect binary takes `--clientid`
and python-tidal swaps `client_id`/`client_secret` between its OAuth and PKCE paths, so device
identity is a per-client choice, and it is also the string TIDAL shows in
`PRIVILEGED_SESSION_NOTIFICATION.clientDisplayName` (§5); (c) which process owns the Pushkin socket
when both run on one host — two open Pushkin sockets on one account will fight each other exactly the
way the report already warns about a daemon fighting the user's phone (§5). [gap, verified-source
`ref:python-tidal/tidalapi/session.py:717-750`; `ref:tidal-connect/bin/entrypoint.sh` (`--clientid`);
`ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts`]

---

### 14. Design options for streamboat headless

| # | Option | Effort | Ecosystem leverage | Security posture | "Simple but beautiful" fit | Mobile-future fit |
| --- | --- | --- | --- | --- | --- | --- |
| **a** | **CLI player** (`streamboat play <url>`, `search`, `now-playing`) | XS | none, but zero-friction for scripting — and a working, current precedent already sits unread-until-this-fact-check in the reference set (see below) | trivial (no listener) | Excellent — a beautiful CLI is a real deliverable | none directly |
| **b** | **Daemon + JSON HTTP/WebSocket API + MPRIS + small web remote** | M | moderate: curl/scripts/Stream Deck/Home Assistant, and streamboat's own GUI | needs a real answer: loopback default, token, opt-in LAN | Good — one protocol, one document, one web page | **Best** — the mobile app speaks the same protocol |
| **c** | **MPD-protocol-compatible server** | M–L (parser + state machine + `idle`) | **Highest** — MALP, ncmpcpp, Cantata, mpc, mpDris2 work on day one | **not as weak as an earlier draft said**: MPD's `local_permissions`/`host_permissions` mean a Unix-socket- or loopback-bound listener needs no shared password at all; a single plaintext network password remains the only *weak* mode, and it has no transport encryption regardless (§9) | **Better than "mixed"**: Mopidy-MPD proves a streaming catalogue maps cleanly onto MPD's directory model via `lsinfo`/`search` over `context.browse()` (§9) — a *subset* should include browsing, not drop it; a full byte-for-byte MPD reimplementation is still the wrong scope | Indirect — phone MPD clients exist today, and with a browse mapping they *can* navigate a TIDAL library, as mopidy-tidal already demonstrates |
| **d** | **streamboat-native remote protocol** (mDNS + auth + state sync; GUI and future mobile app as controllers) | L | none externally; total internally | designed in from the start (pairing code → token; TLS optional on LAN) | Good if it *is* option (b) with discovery bolted on — bad if it is a second protocol | **Essential eventually** |
| **e** | **UPnP/DLNA renderer** | M | some (BubbleUPnP, mConnect, upplay) | UPnP has no auth model worth the name | Poor — SOAP/XML, and hi-res needs a renderer whitelist | none |

**The key structural insight**: (b) and (d) are the same thing. A JSON-over-WebSocket control API
plus mDNS advertisement plus a pairing-based token *is* a Connect-like protocol. Building (b) with
(d) in mind costs almost nothing extra; building (b) and then (d) separately costs double. [inferred]

**(c) is a genuine multiplier but must be scoped as a subset**, following Nuclear: playback control,
queue management, `idle` notifications, `status`/`currentsong`; and *not* pretending to be a file
database. Ship it once (b) is stable, and document exactly which commands are supported.

**(a) falls out of (b) for free** if the CLI is a thin client of the daemon (`streamboat play` →
one WebSocket call). tidalt's `play.go` is the model: try the running instance first, fall back to
starting one. [verified-source]

**A working, current CLI already sits in the reference set and answers most of option (a)'s open
questions** — the brief specifically asked for a CLI mode "now", and this report's original draft
rated (a) as "XS effort, none" without ever opening `ref:tidal-cli` or `ref:tidalgo`. [gap,
verified-source `ref:tidal-cli/README.md`, `ref:tidal-cli/src/index.ts`, `ref:tidal-cli/src/playback.ts`]

- `ref:tidal-cli` (`@lucaperret/tidal-cli` v1.2.5, MIT, Node ≥20, built on `commander`) is a full
  noun-verb CLI: `auth` | `search {artist,album,track,video,playlist,suggest,editorial,history,
  history-delete,history-clear}` | `artist {info,tracks,albums,similar,radio}` |
  `album {info,barcode}` | `track {info,similar,isrc,radio}` | `playlist {list,create,rename,delete,
  add-track,remove-track,add-album,move-track,set-description}` | `library {add,remove,
  favorite-playlists,add-playlist,remove-playlist}` | `recommend --type {daily,discovery,new-release,
  offline}` | `mix items` | `history {tracks,albums,artists}` | `saved {list,add,remove}` |
  `share {track,album}` | `user profile` | `playback {info,url,play}`.
- **Copyable conventions**: one global `--json` flag placed before the subcommand
  (`tidal-cli --json search track "…"`), human-readable output by default, `exit(2)` for an invalid
  argument value vs. `exit(1)` for a runtime error.
- **What NOT to copy**: `playbackPlay` downloads the *entire* track (or every DASH segment) to
  `os.tmpdir()` and shells out to `mpv --no-video` / `afplay` / `start`, deleting the file on SIGINT —
  no streaming, no gapless, no seek. This is the anti-pattern streamboat's own `streamboat play` must
  avoid; it should stream through `streamboat-core`'s player engine instead.
- **A fact this report missed entirely elsewhere**: this CLI takes playback data from the *official*
  `developer.tidal.com` API v2 — `GET /trackManifests/{id}` with `{adaptive:false,
  formats:[HEAACV1,AACLC,FLAC,FLAC_HIRES], manifestType:'MPEG_DASH', uriScheme:'DATA',
  usage:'PLAYBACK'}` returning a base64 data-URI manifest plus `trackAudioNormalizationData
  {replayGain,peakAmplitude}` and `albumAudioNormalizationData` in one call — a second, officially
  sanctioned manifest path worth comparing against the unofficial-API approach documented in
  `/home/user/streamboat/docs/research/tidal-api.md`.

**(e) should be dropped.** [inferred]

---

### 15. Sharing the core between GUI and headless

Three models, all with precedents:

**Model 1 — one binary, mode flag (tidalt).** `streamboat` opens the GUI; `streamboat daemon` runs
headless; the GUI, if a daemon already owns the lock/bus name, becomes a *client*. One build, one
install, one config, no protocol version skew during upgrades. Cost: the GUI carries daemon code and
vice versa; the "client mode" GUI must handle a reduced feature set. [verified-source `ref:tidalt`]

**Model 2 — GUI always a client of a daemon (MPD, Roon, Music Assistant).** The engine only ever
lives in one process; the GUI is a pure front-end over the same API the mobile app and web remote
use, which forces the API to be good. Cost: the desktop app must start, supervise and possibly
bundle a daemon; every desktop feature crosses an IPC boundary; a crashed daemon is a broken app; on
Windows/macOS "start a background service" is a chore users resent. [documented-web]

**Model 3 — GUI embeds the core in-process; the daemon is a separate, optional binary (librespot
family).** `streamboat-core` as a library; `streamboat` (GUI) links it; `streamboatd` links it too.
Cost: two binaries to ship; risk of the two drifting; state can't be shared between a running GUI and
a running daemon on the same host (which is exactly what the lock/bus-name check solves).

**Recommendation: Model 1 implemented on top of Model 3's library split.** [inferred]

- `streamboat-core`: session/auth, catalogue, queue, player engine, output backends, Pushkin.
  No UI, no server. This is the artifact that makes a future mobile app cheap.
- `streamboat-server`: the control API (HTTP+WS), MPRIS, mDNS, optional MPD listener. Depends on core.
- One shipped binary with subcommands (`streamboat`, `streamboat daemon`, `streamboat play`,
  `streamboat service install`), plus — if packagers want it — a `streamboatd` alias that is the same
  binary.
- The GUI **always** talks to the engine through the same command/event types the server exposes,
  even in-process. That keeps the API honest without paying an IPC tax for local use, and makes
  "GUI as a remote for another host's daemon" a configuration change rather than a rewrite.

**The control protocol's event schema and state-sync semantics are the most load-bearing artifact in
this whole document, and this report previously specified none of it.** Stage 1 says only "HTTP +
WebSocket JSON … plus an event stream", and stage 1.2 says the future mobile app speaks it — with no
message envelope, no event vocabulary, no snapshot-vs-delta decision, no sequence/revision numbers, no
reconnect/resume rule, and no story for two remotes editing the queue at once. Two precedents in the
reference set answer this directly and were not previously cited for it: [gap, documented-web
https://raw.githubusercontent.com/devgianlu/go-librespot/master/API.md and
`/master/api-spec.yml`; verified-source `ref:TidaLuna/plugins/lib/src/redux/types/store/PlayQueue.ts`]

- **go-librespot's WebSocket endpoint is `/events`**, with a closed vocabulary: `active`, `inactive`,
  `metadata`, `will_play`, `playing`, `not_playing`, `paused`, `stopped`, `seek{position}`,
  `volume{value,max}`, `shuffle_context`, `repeat_context`, `repeat_track`; its REST half is published
  as an OpenAPI spec (`api-spec.yml`) — worth doing the same for streamboat's own API from day one.
- **TIDAL's own client already has a concurrency answer worth copying**, in a file this report never
  cited before this fact-check: `PlayQueue.ts` defines `CloudQueue{queueId, etag, itemsEtag,
  currentItemId, headPosition, tailItemId, tailPosition, repeatMode, shuffled,
  historyMediaItemIds}` and `CloudQueueItem{id, media_id, properties{active, original_order,
  sourceType}}` — an ETag'd queue with stable per-item ids and explicit head/tail — plus
  `PlayQueueElement{context{type,id}, mediaItemId, priority, uid}`, `PlayQueueSourceType`
  (`album|artist|playlist|mix|search|MY_TRACKS|…`) and `RepeatMode{Off=0,All=1,One=2}`.
- **Recommendation**: full snapshot on connect + typed deltas carrying a monotonic revision number, an
  `itemsEtag` on the queue itself, stable per-item `uid`s (not array indices), and a documented
  reconnect rule on a revision gap (resync with a fresh snapshot rather than replaying missed deltas).

**Protocol versioning and capability negotiation between the daemon, the GUI, the web remote and the
future mobile app is a second gap in the same artifact.** The GUI, the phone app and the daemon will
be on different versions the moment the project has real users — an un-updated Pi, a store-lagged
phone release. Two precedents already cited elsewhere in this document solve it in opposite ways, and
the choice between them should be made explicitly rather than left implicit: [gap, documented-web
https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/doc/protocol.rst;
https://raw.githubusercontent.com/Sendspin/spec/main/README.md]

- **MPD's style (additive, tolerant)**: `commands`/`notcommands` advertise what the current connection
  may call, and `protocol` / `protocol available` / `protocol enable {FEATURE}` / `protocol clear`
  negotiate optional behaviour per-connection (§9).
- **Sendspin's style (strict)**: "Core version 1, exact-match versioning" — any mismatch is a hard
  failure, not a negotiation (§11).
- **Recommendation**: the already-planned `GET /health` should return `{version, protocol_version,
  capabilities[]}`, and clients should be required to degrade gracefully on a missing capability
  (MPD-style) rather than refuse to connect (Sendspin-style) — the daemon is a home appliance, not a
  paired security device, and a strict-versioning failure mode there is a worse user experience than a
  silently-narrower feature set.

**Process management, per platform** [documented-web + inferred; the systemd template is verified]:

| Platform | Mechanism | Notes |
| --- | --- | --- |
| Linux (desktop) | `systemd --user` unit, generated by `streamboat service install` | Copy tidalt's template shape; `After=`/`PartOf=graphical-session.target` for a desktop daemon, but for a **headless server** use `WantedBy=default.target` and enable lingering (`loginctl enable-linger`) so it survives logout |
| Linux (server/Pi) | system unit + a dedicated user, or `systemd --user` + linger | Needs `audio` group membership for `/dev/snd`; if PipeWire is in play the daemon must run in the user's session or use ALSA directly |
| Windows | `windows-service` crate (`define_windows_service!`) | Windows services run in **session 0**, which has no interactive audio endpoint by default. First-party Microsoft documentation stating the exact rule could not be reached from this environment (general WASAPI docs only), so **treat "can a service open WASAPI?" as still unverified** — but the realistic shape of the answer is: (a) a per-user Scheduled Task at logon, (b) a tray/background app that autostarts, or (c) a service that only ever drives a pipe/network output and never opens a local device directly. **This is a named, one-hour spike, not an open question to leave unresolved**: run a WASAPI render loop from a Windows service and from a scheduled task, and record which one produces sound. The answer decides whether `streamboat service install` exists on Windows at all. [gap — needs first-party verification: https://learn.microsoft.com/en-us/windows/win32/coreaudio/wasapi plus Windows session-0-isolation guidance] |
| macOS | `launchd` LaunchAgent (per-user, has audio) rather than a LaunchDaemon | Same audio-session caveat as Windows |
| Container | plain PID 1 + `restart: unless-stopped`; host networking if mDNS is used | `ref:tidal-connect/docker-compose.yaml` is the template |

**Config locations** — follow platform conventions rather than inventing:
`$XDG_CONFIG_HOME/streamboat/config.toml` (Linux), `~/Library/Application Support/streamboat/`
(macOS), `%APPDATA%\streamboat\` (Windows); cache under `$XDG_CACHE_HOME`; state/tokens under
`$XDG_STATE_HOME` or the keyring. A daemon must accept `--config <path>` because a system service has
no `$HOME` worth speaking of. Precedents in the set: `~/.config/sone/settings.json` + `sone.key`
(`ref:sone`), `~/.config/tidalt/secrets` + `~/.local/share/tidalt/play.log` (`ref:tidalt`),
`/var/lib/mopidy/tidal/tidal-*.json` and `/etc/mopidy/mopidy.conf` (`ref:mopidy-tidal`).
[verified-source]

**Logging**: log to stderr with a level flag and let the supervisor capture it (journald,
`journalctl --user -u streamboat -f`); offer `--log-file` for Windows/macOS where there is no
journal. tidalt writes a dedicated `play.log` for the deep-link path because that code runs with no
terminal — a good idea worth copying for any path invoked by the OS. [verified-source]

**Updates**: a daemon that auto-updates itself and restarts mid-track is hostile. Prefer OS
packaging (systemd + distro/Flatpak/AUR/Homebrew), and if an in-app updater exists, make it
"download now, apply on next idle". [inferred]

**Build and distribution for small devices were entirely unaddressed in the original draft, despite
every precedent in this document being consumed as a Docker image or a distro package on a Pi**
(`ref:tidal-connect`, `upmpdcli-docker`, Music Assistant, Snapcast). Decisions to surface explicitly,
not defer: [gap, documented-web https://raw.githubusercontent.com/librespot-org/librespot/dev/Cargo.toml
(TLS feature guidance); verified-source `ref:tidal-connect/docker-compose.yaml`;
`ref:tidal-connect/README.md` (Pi 5 loader issue, §2.4)]

- **aarch64** (Pi 3/4/5 on a 64-bit OS) is mandatory; **armv7** (Pi Zero 2 W, 32-bit Raspberry Pi OS —
  also the ARMv7 target of the ifi Connect binary itself) is a real, separate cost and needs an
  explicit yes/no from the owner, not an assumption.
- **glibc vs. musl-static matters concretely**: a static build removes exactly the failure class
  documented for the Connect binary on Pi 5 (`libsystemd.so.0: ELF load command alignment not
  page-aligned`, worked around there with `kernel=kernel8.img`, §2.4). librespot documents `rustls` as
  the TLS option "for avoiding external OpenSSL dependencies, reproducible builds, or when targeting
  platforms where native TLS dependencies are unavailable or problematic (musl, embedded, static
  linking)" — the same lever applies to `streamboat-core`.
- **If an official container is published**, it needs `network_mode: host` if mDNS is used, `/dev/snd`
  passthrough for ALSA, and a `/var/run/dbus` mount if MPRIS or Avahi are involved —
  `ref:tidal-connect/docker-compose.yaml` is a ready-made template for a different purpose, reusable
  for this one.

**Daemon lifecycle — queue persistence, prefetch/gapless, and idle-device handling — has no answer in
the staged path below, and each is a one-line design decision with a precedent already in this
document.** An MPD client expects the queue to survive `systemctl restart` (MPD itself has a
`state_file`); prefetch of the next track is what makes gapless playback and Wi-Fi-jitter survival
work, but §16 only ever discusses buffering as a byte count. [gap, verified-source
`ref:tidalt/cmd/tidalt/daemon.go`; `ref:tidalt/internal/player/alsa.c`;
`ref:mopidy-tidal/mopidy_tidal/ext.conf`; documented-web
https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/doc/protocol.rst (the `playlist`
queue-version field, §9)]

- `ref:tidalt` prints "No audio device is opened until playback starts" in daemon mode — a
  deliberate hold-only-while-playing policy — paired with its D-Bus
  `org.freedesktop.ReserveDevice1.Audio<N>` reservation so PipeWire yields the device and reclaims it
  on stop (§16).
- `ref:mopidy-tidal`'s `playback_cache_buffer_bytes = 16777216` with a Range-capable proxy is the
  read-ahead lever (with the legal-posture caveat from §8.1 about *what* gets cached).
- MPD's `status` exposes `playlist` as a monotonically increasing 31-bit queue version (§9) — the
  standard way a client detects a changed queue without diffing it; streamboat's own protocol (above)
  should carry an equivalent revision number.
- **Decisions to state explicitly**: where the queue state file lives (`$XDG_STATE_HOME`); whether it
  is written per mutation or on shutdown plus a timer; whether a restart resumes playing or paused;
  how many tracks ahead are prefetched.

---

### 16. Raspberry Pi and small-device constraints

- **CPU is not the binding constraint for FLAC.** FLAC decoding at 24/192 is a low-cost integer
  workload; the reference deployments (mopidy-tidal, upmpdcli's plugin, Music Assistant) all serve
  24/192 from Pi-class hardware. The documented sizing advice for the *Connect binary* is "a
  Raspberry Pi 3/4 will work … for a usb dac and hi-res audio, consider at least a Pi 3b+ or, even
  better, a Pi 4b", and on an Asus Tinkerboard the author had to keep the CPU governor above about
  600 MHz to avoid crackling. [verified-source `ref:tidal-connect/README.md`] **No measured
  decode-CPU benchmark was found; do not quote a percentage.** [unverified]
- **The real costs are elsewhere**: HTTPS + TLS for the CDN fetch, DASH manifest parsing, and any
  resampling. Avoid resampling entirely in bit-perfect mode (`ref:sone` and `ref:tidalt` both do).
- **USB DAC / I²S output**: `ref:tidalt/internal/player/alsa.c` opens `hw:` devices and negotiates
  formats with `snd_pcm_hw_params`, preferring `S32_LE > S16_LE > S24_3LE > S24_LE` for 16-bit
  sources and `S24_3LE > S24_LE > S32_LE` for 24-bit. **Correction to an earlier draft, which
  attributed the S32_LE-first ordering to "a Hidizs USB issue"**: the code comment actually says
  "Prefer S32_LE for 16-bit sources: many USB DACs (e.g. CS43198-based devices) have a buggy or
  non-functional S16_LE USB endpoint but work correctly via their native 32-bit endpoint" — the
  Hidizs device (an S9 Pro Plus) appears in a *different* comment about anomalous period-size values
  (87 frames), not about format ordering. [confirmed + corrected, verified-source
  `ref:tidalt/internal/player/alsa.c:28-31,70`]
  it falls back to `plughw:` **only** when format negotiation is refused, retries on device-busy
  against `hw:`, and recovers xruns with `snd_pcm_recover`. It also reserves the device over D-Bus
  (`org.freedesktop.ReserveDevice1.Audio<N>`) so PipeWire yields it, releasing on stop. This is the
  most complete small-device output recipe in the reference set. [verified-source, per the project
  survey; `alsa.c` referenced by path]
- **Sample-rate switching**: a bit-perfect daemon must reopen the PCM when the track's rate changes,
  which produces an audible gap on most DACs. Decide whether streamboat prioritises bit-perfect (gap
  at rate change) or gapless (resample to a fixed rate). `ref:tidal-connect`'s per-DAC presets
  include fixed-44.1 and fixed-48 variants precisely because some outputs (RPi HDMI) can't switch.
  [verified-source + inferred]
- **Device naming**: resolve the card by **name** from `/proc/asound/cards` at every start, never by
  a stored index. `ref:tidal-connect/bin/common.sh` is a working implementation, generating an
  `/etc/asound.conf`. **Correction**: the application does not uniformly open `default` — see §2.4
  for the exact `tidal-softvol` / `$CREATED_ASOUND_CARD_NAME` / `custom` / `default` branching the
  generated config actually implements. [verified-source]
- **Volume**: check for an existing `Master` control with `amixer -c <idx> controls`; create a
  softvol only if absent, else create `SoftMaster` and warn that the remote's slider will move
  hardware volume shared with other players. [verified-source]
- **Wi-Fi buffering**: no measured guidance found in the reference set. The relevant lever in the
  set is mopidy-tidal's `playback_cache_buffer_bytes = 16777216` (16 MiB) with a Range-capable local
  proxy, which both smooths jitter and enables seeking. Adopt a comparable read-ahead buffer and make
  it configurable. [verified-source for the constant; sizing advice **[inferred]**]
- **mDNS on a Pi**: requires `avahi-daemon` (not installed by default on DietPi), host networking in
  containers, and — for the Connect binary at least — working IPv6. A streamboat-native mDNS
  responder using `mdns-sd` (Rust, "supports both the client (querier) and the server (responder)
  uses") avoids the Avahi dependency, matching go-librespot's `zeroconf_backend: builtin | avahi`
  choice. [verified-source + documented-web]

- **mDNS coexistence and name collisions were an open worry, and `mdns-sd` resolves both.** On a
  Linux desktop Avahi already owns mDNS; on macOS `mDNSResponder` does; and if two streamboat boxes
  share a LAN, both default to being called "streamboat". `mdns-sd` is "tested with some existing
  common tools (e.g. Avahi on Linux, dns-sd on MacOS, and Bonjour library on iOS) to verify the basic
  compatibility", supports macOS/Linux/Windows and IPv4/IPv6, and implements Probing (RFC 6762 §8.1),
  Simultaneous Probe Tiebreaking (§8.2) and Conflict Resolution (§9) — collisions are handled at the
  protocol level and surfaced to the application as a `DnsNameChange` event; it does **not** implement
  unicast responses (§5.4) or multipacket known-answer suppression on the responder side. **Concrete
  rules to write down, not leave implicit**: derive the default instance name from the hostname, keep
  it single-word-safe (the Connect binary's own issue #216 — "multi-word friendly names break Avahi
  for some users" — is a directly relevant lesson, §2.4), surface `DnsNameChange` in both the UI and
  the logs rather than silently renaming, and expose `mdns_backend: builtin|avahi|off` mirroring
  go-librespot. [gap, documented-web: https://raw.githubusercontent.com/keepsimple1/mdns-sd/main/README.md]

- **A Raspberry Pi has no real-time clock, and this report previously said nothing about the
  consequence.** A Pi boots with a wrong clock until NTP converges; a wrong clock breaks TLS
  validation against TIDAL's CDN/API, breaks token-expiry arithmetic, breaks the Pushkin `USER_ACTION`
  `startedAt` timestamp (§5, which the SDK says must come from "a true-time source"), and breaks
  Snapcast's sub-millisecond sync (§11). The official TIDAL auth stack is visibly time-sensitive:
  `ref:tidal-cli/src/index.ts` opens by monkey-patching `console.warn` solely to "Suppress 'TrueTime
  is not yet synchronized' warnings from `@tidal-music/auth`" — the official package ships a TrueTime
  service and complains before it syncs. **Concrete fixes worth adding to the deployment/systemd
  guidance in §15**: `After=time-sync.target` (with `Wants=`) in the daemon's unit, retry-with-backoff
  on TLS failure during early uptime instead of a hard auth error, and a clock-sync line in the
  `/health` output. [gap, verified-source `ref:tidal-cli/src/index.ts:3-8`; documented-web
  https://raw.githubusercontent.com/badaix/snapcast/develop/README.md (NTP/chrony requirement)]

- **No testability or diagnostics story exists for the headless daemon, and this report's own
  "reusable artifacts" table (§18) already contains the ingredients without naming the command.** A
  headless audio daemon is the hardest thing to test and the easiest thing to break silently on a Pi.
  Cheap mechanisms, all with a precedent already cited in this document: (a) the pipe/stdout PCM
  output (§11) doubles as a CI sink — add `null` and `wav:<path>` outputs and playback tests become
  byte comparisons; (b) MPD conformance (§9/§14) can be asserted in CI against real clients with `mpc`
  (scriptable, packaged everywhere) plus the spec's own `commands`/`notcommands` output as a golden
  file; (c) ship a `streamboat doctor` subcommand that runs exactly the checks the Connect wrapper
  learned the hard way (§2.4): ALSA card-name resolution, device-busy detection
  (`cat /proc/asound/<card>/pcm0p/sub0/hw_params` != `closed`), a pre-flight test tone, avahi/mDNS
  reachability, clock sync (above), IPv6 availability. [gap, verified-source
  `ref:tidal-connect/bin/entrypoint.sh`, `ref:tidal-connect/bin/common.sh`]

---

### 17. Recommended staged path

Rationale in one line: **build the daemon protocol once, expose it through as many pre-existing
client ecosystems as cheaply as possible, and never touch TIDAL Connect.**

| Stage | Deliverable | Why now | Precedent |
| --- | --- | --- | --- |
| **0 — Core split** | `streamboat-core`: session/auth (device-code + PKCE), catalogue, queue, player engine, output backends (ALSA/WASAPI/CoreAudio + **pipe**), Pushkin streaming-privileges client. No UI, no server. | Every later stage is a shell around it; a future mobile app is otherwise unaffordable. | librespot as a library; `ref:mopidy-tidal` as a Mopidy backend; TidalSwiftLib |
| **0 — One binary, two modes** | `streamboat` (GUI/TUI), `streamboat daemon`, `streamboat play <url>`, `streamboat service install`. Single-instance detection; second invocation becomes a client. | Users install one thing; the daemon is not a separate product. | `ref:tidalt/cmd/tidalt/{main,daemon,play}.go` |
| **0 — OS media integration** | MPRIS2 on Linux (`org.mpris.MediaPlayer2.streamboat`), SMTC/Now Playing on Windows/macOS in the GUI. | ~a day of work; delivers media keys, desktop widgets, `playerctl`, and a usable minimal IPC. | `ref:sone` (`mpris-server` 0.9 + `zbus` 5), `ref:sone-windows` (`souvlaki` 0.8.3), `ref:tidalt` |
| **0 — Snapcast-ready output** | `--output pipe:<path>` / `stdout` emitting interleaved PCM. | Tens of lines; unlocks synchronized multiroom, ESP32 endpoints, Home Assistant. | librespot/go-librespot `pipe` backend; Snapcast `/tmp/snapfifo` |
| **0 — Headless login** | Device-code flow with a terminal QR + printed URL/code; token in the OS keyring with a 0600 encrypted-file fallback; `--config` path override. | Without it the daemon is unusable on a Pi. | `ref:tidalt/internal/tidal/loginprint.go`; `ref:mopidy-tidal/mopidy_tidal/web_auth_server.py` |
| **1 — Control API** | HTTP + WebSocket JSON on `127.0.0.1` by default, generated UUID token, opt-in `--bind`; vocabulary modelled on tidal-hifi (`POST /player/*`, `PUT /player/volume`, `GET /current`, `GET /health`) plus catalogue/search/queue and an event stream. | This *is* the protocol the GUI, the web remote and the future mobile app all use. Build it once. | `ref:tidal-hifi/.../swagger.json`; `ref:sone/src-tauri/src/mcp/server.rs`; go-librespot |
| **1.1 — Web remote** | A single static page served by the daemon over the same listener and token. Built with no service-worker dependency and a small enough payload to need no offline cache, because a plain-`http://` LAN IP is **not a secure browser context** — no service worker, no installable PWA, no Web Crypto — until the TLS/tunnel decision (§13) is made. | A phone remote for zero extra protocol work; the "beautiful" part of headless — as long as it is designed around the insecure-context constraint instead of discovering it later. | Snapweb; Mopidy-Iris; W3C Secure Contexts (https://w3c.github.io/webappsec-secure-contexts/) |
| **1.2 — Discovery + pairing** | Advertise `_streamboat._tcp` via `mdns-sd`, with the collision/rename handling from §16 (`DnsNameChange`, single-word default name); a pairing flow following Sendspin's shape (§11) — a QR/short-code pairing token exchanged in-band for a long-lived, named, revocable token — rather than a shared secret; the desktop GUI grows a "play on…" picker listing local daemons. | Turns the daemon into a Connect-like target *for streamboat's own clients*, which is the honest substitute for TIDAL Connect. | Sendspin's discovery + pairing design (§11); go-librespot `zeroconf_backend` |
| **1.2 — MPD subset** | An MPD-protocol listener on 6600 (falling back through 6601-6609 if occupied, Nuclear-style) covering `status`, `currentsong`, `play/pause/next/previous/seek`, queue commands, `setvol`, `idle` with the relevant subsystems, `albumart`/`readpicture`, **and** `lsinfo`/`search`/`find` over a virtual browse tree (Home / My Collection / Playlists / Mixes) plus `listplaylists`/`listplaylistinfo` for TIDAL playlists — browsing is cheap to include, not a reason to scope it out (§9/§14). Documented explicitly as a subset, bound loopback/Unix-socket by default so `local_permissions` covers auth without a shared password (§9). | MALP, ncmpcpp, Cantata and mpc work the day it ships — the largest client-ecosystem gain available. | Nuclear's MPD server (port fallback); Mopidy-MPD (`lsinfo` over `context.browse()`) |
| **1.2 — Diagnostics** | `streamboat doctor`: ALSA card-name resolution, device-busy check, pre-flight test tone (opt-out), avahi/mDNS reachability, clock sync, IPv6 availability — plus `null`/`wav:<path>` CI output sinks and MPD-conformance testing against `mpc`. | Makes the MPD subset and the Pi deployment actually verifiable instead of "should work". | `ref:tidal-connect/bin/{entrypoint,common}.sh` (§16) |
| **2 — Optional** | Sendspin source if the protocol gains traction; a Snapcast stream-plugin (`streamboat snapcast-plugin`, JSON-RPC over stdin/stdout, §11) so Snapweb becomes a full remote, not just a speaker; Cast/AirPlay senders only after the re-transmission question is settled; a Range-capable local caching proxy for seek + Wi-Fi jitter, built opaque/encrypted per the caching-legal-posture note in §8.1, not a plain-chunk cache. | Demand-driven. | Music Assistant 2.8; Snapcast stream-plugin API; go-librespot's encrypted-cache posture |
| **Never** | TIDAL Connect target, TIDAL Connect controller, UPnP renderer, Chromecast receiver. | §6, §7, §10. | — |

Two rules that keep the stages coherent:

1. **The GUI must not have a private path into the engine.** Even in-process, it issues the same
   commands and consumes the same events the control API exposes. That is what makes stage 1.2's
   "GUI drives a remote daemon" a config change rather than a rewrite.
2. **Every listener defaults to loopback.** LAN exposure is a deliberate, logged, token-protected
   choice — not the default. mopidy-tidal's `("", 8989)` login server is the anti-pattern;
   go-librespot's own default (`server.address: localhost`) matches the recommendation here.
3. **A URL-path token is for machine clients, not browsers.** The web remote should exchange a
   pairing token for an `HttpOnly` cookie on first use rather than carry the token in the address bar
   for the life of the session (§8.3).

---

## Comparison table: control surfaces streamboat could expose

| Surface | Clients gained | Effort | Auth story | Expressiveness | Recommend |
| --- | --- | --- | --- | --- | --- |
| **MPRIS2 / D-Bus** (`org.mpris.MediaPlayer2.streamboat`) | playerctl, GNOME/KDE media widgets, hardware media keys, mpDris2-style bridges | XS (`mpris-server` 0.9 + `zbus` 5, as in `ref:sone`) | session bus = local user | transport + metadata, **and** queue (`TrackList`) + stored-playlist activation (`Playlists`) — both optional interfaces `mpris-server` 0.9 already implements; catalogue search/browsing is still out of reach | **v1, Linux** |
| **Unix domain socket / named pipe** (NDJSON, ncspot-style) | the CLI, same-host GUI, tmux/status-bar scripts | XS | filesystem permissions; no port, no token | transport + status; no browsing | **v0** |
| **Windows SMTC / macOS Now Playing** | OS media overlays and keys | XS (`souvlaki` 0.8.3 as in `ref:sone-windows`) | OS-scoped | transport + metadata | **v1, GUI only** |
| **HTTP + WebSocket JSON API** | scripts, Home Assistant, Stream Deck, streamboat's own web remote and future mobile app | M | loopback default + generated token (`Authorization: Bearer` for API clients, a cookie exchange for the browser remote — §8.3) + opt-in LAN bind (`ref:sone/src-tauri/src/mcp/server.rs`) | everything | **v1** |
| **Embedded web remote** (served by the daemon) | any phone browser on the LAN | S once the API exists | same token; served over the same listener; not a secure browser context over plain HTTP (§17) | everything the API has | **v1.1** |
| **MPD subset on 6600** | MALP, ncmpcpp, Cantata, mpc | M–L | `local_permissions`/Unix-socket needs no password at all; a network password stays unencrypted regardless (§9) | queue + transport + catalogue browsing via `lsinfo`/`search` over a virtual tree, following Mopidy-MPD (§9/§14) | **v1.2, explicitly a subset — including browsing** |
| **mDNS `_streamboat._tcp`** | streamboat GUI on another host; future mobile app | S | Sendspin-style pairing token → long-lived, revocable token (§11/§13) | discovery only | **v1.2** |
| **Raw PCM pipe / stdout output** | Snapcast, and anything that eats PCM | XS | n/a (local pipe) | n/a | **v1** |
| **Sendspin source** | Music Assistant + emerging ESP32/Pi endpoints | L | Noise handshake per spec | full | **watch** |
| **Chromecast / AirPlay sender** | LAN speakers | L each | n/a | full | **defer; resolve re-transmission question first** |
| **UPnP renderer / media server** | BubbleUPnP, upplay | M | none meaningful | full | **skip** |
| **TIDAL Connect target or controller** | TIDAL's own apps | XL + blocked | vendor certificate | full | **never** |

---

## Implications for streamboat

**Architecture decisions this topic forces:**

1. **Split `streamboat-core` out first.** Session, auth, catalogue, queue, player engine, output
   backends, Pushkin client. No UI. No HTTP. Everything else in this document is a thin shell around
   it. Unanimous across librespot, Mopidy, tidalt, TidalSwift.
2. **Ship one binary with subcommands**, tidalt-style: default = GUI (or TUI where no GUI is built),
   `daemon` = headless, `play <url>` = deep-link forwarder, `service install` = platform service
   installer. Single-instance detection via the MPRIS bus name on Linux and a lock file elsewhere;
   if an instance exists, act as a client instead of failing.
3. **Design one control protocol and use it everywhere** — GUI↔engine (in-process, same types),
   GUI↔remote daemon, web remote↔daemon, future mobile app↔daemon. JSON over WebSocket for events,
   HTTP for one-shot commands, with tidal-hifi's URL vocabulary as the starting point
   (`POST /player/play`, `PUT /player/volume`, `GET /current`, `GET /health`).
4. **Bind loopback by default; require a token; make LAN exposure explicit.** Copy sone's shape —
   random UUID token generated on first enable, persisted in settings — but not its exact carrier: use
   `Authorization: Bearer` for API/script clients, and for the browser-facing web remote exchange the
   token once for an `HttpOnly`, `SameSite` cookie rather than keeping it in the URL (§8.3). Add a
   connection cap on the event stream; log the reachable URL with `127.0.0.1` when bound to the
   wildcard.
5. **Implement Pushkin (`POST rt/connect` → WebSocket) in core, not in the UI.** A daemon that
   ignores streaming privileges will misbehave in exactly the multi-device scenario headless users
   live in.
6. **Add a raw-PCM output backend on day one.** It is a few dozen lines and it delivers Snapcast
   multiroom, ESP32 endpoints and arbitrary downstream processing.
7. **Do not build TIDAL Connect in either direction, and say so in the README** with the reasons
   (proprietary binary, vendor certificate, obfuscated protocol, 16/44.1 ceiling). Convert it into a
   selling point: streamboat's own daemon plays 24/192 where the Connect target cannot.

**Reusable artifacts identified (copy or adapt):**

| Artifact | Source | Use |
| --- | --- | --- |
| ALSA card-name → index resolution + generated `asound.conf` | `ref:tidal-connect/bin/common.sh` | daemon startup on Pi/servers |
| Softvol creation with `Master`/`SoftMaster` detection | `ref:tidal-connect/bin/common.sh` | volume on hardware without a mixer |
| Pre-flight test tone before opening the device (opt-out via `ENABLE_GENERATED_TONE=no`) | `ref:tidal-connect/bin/entrypoint.sh` | catch locked/misconfigured devices, without forcing a click on every open |
| 26 per-DAC `asound.conf` presets + tested-device table | `ref:tidal-connect/userconfig/`, `assets/known-devices.md` | a device compatibility database |
| `systemd --user` unit template + installer, plus `After=time-sync.target` | `ref:tidalt/cmd/tidalt/daemon.go` | `streamboat service install`; avoid the clock-skew failure mode in §16 |
| Single-instance + client-mode fallback via D-Bus name ownership | `ref:tidalt/internal/mpris/server.go`, `cmd/tidalt/main.go` | one binary, two roles |
| Deep-link forwarding with terminal fallback | `ref:tidalt/cmd/tidalt/play.go` | `streamboat play <url>` |
| Unix-domain-socket NDJSON control surface | `ref:hrkfdn/ncspot` (`doc/users.md`) | `streamboat status`/CLI/same-host GUI, v0 (§12) |
| Terminal QR device-code login (generated locally) | `ref:tidalt/internal/tidal/loginprint.go` | headless login |
| Local login page + PKCE paste form | `ref:mopidy-tidal/mopidy_tidal/web_auth_server.py` | headless login (bind loopback, unlike the original) |
| "Login hack": login prompt rendered as catalogue content | `ref:mopidy-tidal/mopidy_tidal/login_hack.py` | login inside an MPD client (render the QR locally, not via `api.qrserver.com` — §8.1) |
| Encrypted/opaque audio cache (still-encrypted file, key re-fetched on play) | go-librespot `cache.{enabled,dir,size_limit}` | seeking + jitter smoothing without a plain-audio cache directory (§8.1) |
| Pre-flight `HIRES_LOSSLESS in media_metadata_tags` check | `ref:mopidy-tidal/mopidy_tidal/playback.py` | honest quality reporting |
| Loopback + token + SSE-cap local server (Bearer for API, cookie for browser — §8.3) | `ref:sone/src-tauri/src/mcp/server.rs`, `overlay/server.rs` | the control API's security shape |
| Player HTTP vocabulary | `ref:tidal-hifi/src/features/api/swagger.json` | API naming |
| Streaming-privileges WebSocket handling | `ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts` | core |
| CLI noun-verb surface, `--json` flag, exit-code convention (not its temp-file playback) | `ref:tidal-cli/README.md`, `src/index.ts` | `streamboat`'s CLI subcommands (§14) |
| Snapcast stream-plugin (JSON-RPC over stdin/stdout) | `badaix/snapcast` `doc/json_rpc_api/stream_plugin.md` | `streamboat snapcast-plugin` (§11) |
| Sendspin pairing design (Noise `KKpsk2`, QR pairing token, Sentinel PSK) | `Sendspin/spec` README | streamboat's own pairing flow (§11/§13) |

---

## Open questions

**Only the owner can decide:**

1. **Is the desktop GUI a client of a daemon, always?** (Model 2) Or does it embed the core and only
   optionally expose one? (Model 1+3, recommended.) Everything about IPC, startup, packaging and
   error handling follows.
2. **Does streamboat ever re-transmit audio on the LAN?** A Snapcast pipe, an AirPlay sender, a Cast
   sender, or an HTTP stream endpoint all mean TIDAL audio leaving the machine that authenticated.
   Technically identical to any local output; legally an untested question against TIDAL's consumer
   terms. The pipe/Snapcast case is the most defensible (same-user, same-LAN, no service
   impersonation); an open HTTP stream endpoint is the least. Decide the line and document it.
3. **How far to go with MPD compatibility?** A transport+queue subset (Nuclear's scope, ~weeks) or a
   fuller emulation including a browsable catalogue mapped into MPD's database model (months, and
   fundamentally lossy)?
4. **Bit-perfect vs gapless when the sample rate changes.** Pick a default and expose the trade-off,
   rather than silently resampling.
5. **Is a Windows/macOS background service in scope at all**, or is "headless" a Linux/server story
   with GUI-only behaviour elsewhere?
6. **Name and shape of the native remote protocol** — and whether to align it with Sendspin rather
   than inventing one. Sendspin's pairing design (§11) and event/versioning shape (§15) are strong
   defaults if the answer is "align".
7. **Does streamboat ever approach TIDAL for legitimate Connect partner/SDK status?** This report
   establishes that target identity is a vendor-issued certificate obtainable only through a
   commercial partnership, and that `developer.tidal.com` was unreachable from this environment — so
   the cost, terms, and whether an open-source project can even apply are *unknown*, not
   *impossible*. The owner should decide whether to ask TIDAL directly (a documented answer either way
   is useful to the wider community of these projects) or to state publicly that streamboat will never
   be a Connect device, and put that reasoning in the README next to the existing "we are not a
   Connect target" statement.
8. **Does streamboat's own documentation point users at the GioF71/TonyTromp Docker container as a
   companion for Connect on the same Pi?** That container runs a binary extracted from iFi firmware
   together with iFi's device certificate (§2); linking to it as a recommended companion is a
   positioning choice with the same flavour as this report's own "the whole chain survives on
   obscurity, not permission" assessment (§2.3). Decide it explicitly rather than leaving it implicit
   in whatever the README ends up saying.
9. **Additive-and-tolerant (MPD-style) vs. exact-match (Sendspin-style) protocol versioning** for
   streamboat's own control API (§15) — pick one rather than discovering the answer at the first
   version skew between daemon and mobile app.

**Unverified / needs follow-up before anything is built on it:**

- **TIDAL's actual partner terms for Connect.** `tidal.com`, `support.tidal.com` and
  `developer.tidal.com` are blocked by this environment's egress proxy. Every claim about SDK
  licensing, certification and partner counts here is second-hand trade press.
- **The `_tidalconnect._tcp` TXT record keys and port.** Not documented in any source read for this
  report, but resolvable in minutes on the owner's own LAN with `avahi-browse -r -t
  _tidalconnect._tcp` (§3) — treat as a pending experiment, not a permanent unknown.
- **Whether the Connect control channel really is JSON-over-WebSocket**, and whether it resembles the
  Cast media namespace. Inferred from `websocketpp` in the licence folder plus the Cast-shaped types
  in the desktop client's store. No wire capture.
- **`cloudQueue` / `cloudConnect` endpoints.** Present in the desktop client's action set, absent
  from every SDK and every open client. Undocumented.
- **The exact MPD command list Nuclear implements.** Its docs site (`docs.nuclearplayer.com`) remains
  blocked by this environment's egress proxy; the port-fallback behaviour (6600→6601-6609) and its
  general command scope were recovered via a search-index summary and should be re-verified once the
  page is reachable. The core MPD *protocol* facts in §9, by contrast, are now confirmed against the
  primary spec source (`raw.githubusercontent.com/MusicPlayerDaemon/MPD`) and no longer need
  re-verification.
- **The Sendspin spec's licence** and its adoption trajectory outside Music Assistant.
- **Whether a Windows service can open WASAPI output** in session 0. Assume not until the one-hour
  spike named in §15 is actually run.
- **Measured CPU cost of 24/192 FLAC decode on Pi 3/4/5.** No benchmark found; do not quote numbers.
- **Current hi-res status of `lms-plugin-tidal`.** More nuanced than "ongoing problems": issue #35 is
  closed, #89 is open, and #100 suggests hi-res work has since landed (§8.6) — genuinely mixed, not
  simply broken or simply fixed.
- **Whether TIDAL has ever acted against the Connect binary redistributors.** No takedown found, but
  absence of evidence only.
- **Whether shanocast's AirReceiver-signature approach still works against current Chrome/Chromecast
  firmware.** Cited here only to establish that a Chromecast receiver is not a technical dead end
  (§7/§16), not as something streamboat should build on.

---

## Sources

### Reference checkouts (read directly)

| Path | Supports |
| --- | --- |
| `ref:tidal-connect/README.md` | Connect target ops: `_tidalconnect._tcp` context, avahi requirement, IPv6 requirement, Pi 5 `kernel8.img` fix, MQA removal → 16/44.1 ceiling, hardware sizing, softvol behaviour, exclusive device locking, alternatives list, "this repository does not contain any tidal-connect binary" |
| `ref:tidal-connect/bin/entrypoint.sh` | The exact `tidal_connect_application` command line and every flag; `speaker_controller_application` under tmux; test-tone pre-flight; custom binary/cert injection paths |
| `ref:tidal-connect/bin/common.sh` | Card-name→index resolution, `asound.conf` generation, `Master`/`SoftMaster` softvol logic |
| `ref:tidal-connect/build/Dockerfile` | Runtime deps: `libportaudio-ocaml`, `libavahi-client3`, `alsa-utils`, `libssl-dev`, `libcurl4`, `libavformat-dev` |
| `ref:tidal-connect/docker-compose.yaml` | `network_mode: host`, `/dev/snd`, `/var/run/dbus` mount, DNS override, full env-var surface |
| `ref:tidal-connect/assets/known-devices.md`, `ref:tidal-connect/userconfig/` | Per-DAC card names, formats and 26 `asound.conf` presets |
| `ref:TidaLuna/plugins/lib/src/redux/types/store/RemotePlayback.ts` | `RemotePlaybackDeviceType`, `RemotePlaybackDevice` shape, `TidalConnectQueueInfo`, `TidalConnectMediaInfo`, `RemotePlayer States` |
| `ref:TidaLuna/plugins/lib/src/redux/types/actions/actionTypes.ts` | `remotePlayback/*`, `chromeCast/*`, `cloudQueue/*` action names |
| `ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts` | `POST {legacyApiUrl}/rt/connect`, `USER_ACTION`, `PRIVILEGED_SESSION_NOTIFICATION`, `RECONNECT`, auto-pause behaviour |
| `ref:tidal-sdk-web/packages/player/src/api/event/streaming-privileges-revoked.ts` | The event surfaced to the app |
| `ref:tidal-sdk-android/player/streaming-privileges/…/connection/StreamingPrivilegesService.kt` | `@POST("rt/connect")` → `{url}` |
| `ref:tidal-sdk-android/player/streaming-privileges/…/messages/WebSocketMessage.kt` | Message type strings, `USER_ACTION` = `Acquire` |
| `ref:tidal-sdk-android/player/src/main/kotlin/…/di/NetworkModule.kt` | `"${apiEndpoint}rt/connect"` requires `Credentials.Level.USER` |
| `ref:mopidy-tidal/pyproject.toml` | v0.3.13, Python ≥3.12, `Mopidy>=3.0`, `tidalapi>=0.8.10`, Apache-2.0, the `complete` extras list (iris/mpd/local) |
| `ref:mopidy-tidal/mopidy_tidal/ext.conf` | Full config schema incl. `login_server_port = 8989`, `playback_cache_buffer_bytes = 16777216` |
| `ref:mopidy-tidal/mopidy_tidal/playback.py` | MPD-manifest → `file://`, BTS → first URL, `HIRES_LOSSLESS` pre-flight, cache-proxy fallback chain |
| `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/__init__.py` | `https://lgf.audio.tidal.com/` upstream, SQLite cache, buffer bytes |
| `ref:mopidy-tidal/mopidy_tidal/web_auth_server.py` | The headless login page, PKCE paste form, `("", port)` bind |
| `ref:mopidy-tidal/mopidy_tidal/login_hack.py` | The login-as-content hack and its QR cover art |
| `ref:mopidy-tidal/README.md` | The `mopidy-local mopidy-iris mopidy-mpd mopidy-mpris` extension set; gstreamer bad-plugins requirement; token file location |
| `ref:tidalt/cmd/tidalt/daemon.go` | `tidalt daemon`, the systemd `--user` unit template, `ErrAlreadyRunning` |
| `ref:tidalt/cmd/tidalt/main.go` | Subcommand dispatch, client-mode fallback |
| `ref:tidalt/cmd/tidalt/play.go` | D-Bus deep-link forwarding, terminal-emulator fallback list, `play.log` |
| `ref:tidalt/internal/mpris/server.go` | Bus name `org.mpris.MediaPlayer2.tidalt`, object path, interfaces, `RequestName` single-instance |
| `ref:tidalt/internal/tidal/client.go`, `internal/tidal/loginprint.go` | Device-code verification URL construction, `qrterminal/v3` QR login |
| `ref:tidalt/go.mod` | `godbus/dbus/v5`, `mdp/qrterminal/v3`, `docker/secrets-engine/store` |
| `ref:sone/src-tauri/src/mcp/server.rs`, `mcp/mod.rs` | `127.0.0.1` bind, port 5577, `/{token}/mcp` path, UUID token generation/persistence |
| `ref:sone/src-tauri/src/overlay/server.rs` | Overlay routes, SSE semaphore, wildcard-bind display handling |
| `ref:sone/src-tauri/Cargo.toml` | `zbus = "5"`, `mpris-server = "0.9"`, `rmcp = "1.7.0"`, `axum = "0.7"` |
| `ref:sone-windows/src-tauri/Cargo.toml` | `mpris-server = "0.9"`, `souvlaki = "0.8.3"` |
| `ref:tidal-hifi/src/features/api/swagger.json` | The complete player HTTP API (OpenAPI 3.1.0, version 8.1.3) |
| `ref:high-tide/src/mpris.py` | MPRIS bus name `org.mpris.MediaPlayer2.io.github.nokse22.high-tide`, path, interfaces |
| `ref:tidalt/internal/player/alsa.c` | ALSA format-negotiation order, CS43198 S16_LE-endpoint workaround, `snd_pcm_recover`, `plughw:` fallback |
| `ref:tidalt/internal/player/mpv.go` | `org.freedesktop.ReserveDevice1.Audio%d` acquire/release around playback |
| `ref:python-tidal/tidalapi/session.py` | `token_refresh()`: refresh token is never rotated on use (lines 717-750) |
| `ref:tidal-cli/README.md`, `ref:tidal-cli/src/index.ts`, `ref:tidal-cli/src/playback.ts` | Full CLI command surface, `--json` flag, TrueTime-warning suppression, temp-file DASH playback (anti-pattern), official `GET /trackManifests/{id}` v2 API call |
| `/home/user/streamboat/docs/research/tidal-api.md` | TIDAL terms-of-service analysis, enforcement history, signed/expiring CDN URLs |
| `/home/user/streamboat/docs/research/tidal-client-features.md` | Controller/target split in the native app; remote-playback feature matrix |
| `/home/user/streamboat/docs/research/oss-landscape.md` | Prior architectural survey of the same reference set |

### Web sources

| URL | Supports |
| --- | --- |
| https://raw.githubusercontent.com/TonyTromp/tidal-connect-docker/master/docs/TROUBLESHOOTING.md | `avahi-browse -t _tidalconnect._tcp`, `avahi-browse -a \| grep -i tidal`, ~120 s mDNS TTL, `ifi-pa-devs-get` |
| https://raw.githubusercontent.com/shawaj/ifi-tidal-release/master/README.md | `--netif-for-deviceid`, ARMv7 binaries, `libssl1.0.0`/`libportaudio2`/`libflac++6v5`, full systemd `ExecStart` |
| https://github.com/TonyTromp/tidal-connect-docker | Binary provenance, Avahi/ALSA/dbus/host-network deps, volume-bridge metadata service, "relies on proprietary Tidal Connect binaries" |
| https://github.com/shawaj/ifi-tidal-release/tree/master/licenses/tidal_connect | The binary's third-party libraries: advobfuscator, asio, boost, clara, curl, mdnsresponder, openssl, websocketpp |
| https://github.com/GioF71/tidal-connect | Wrapper repo identity, MIT, fork lineage |
| https://github.com/seniorgod/ifi-tidal-release, https://github.com/ce-designs/tidal-connect-docker, https://github.com/lovehifi/tidal-connect-docker, https://github.com/chiefy/tidal-connect-docker, https://github.com/pulpier/tidal-connect-hifiberry | The fork ecosystem; evidence that all Linux Connect targets are the same binary |
| https://www.whathifi.com/features/tidal-connect-everything-you-need-to-know | What Connect is, from a first-party-adjacent source |
| https://www.ampvortex.com/tidal-connect-the-hi-fi-casting-protocol-that-defined-mqa-spatial-audio-streaming-2020-2026/ | SDK opened 2021, device counts, brand list — **low reliability, SEO-style aggregator** |
| https://github.com/EbbLabs/mopidy-tidal | Current home of mopidy-tidal |
| https://raw.githubusercontent.com/mopidy/mopidy-mpd/main/src/mopidy_mpd/protocol/music_db.py; https://github.com/mopidy/mopidy-mpd | MPD-protocol emulation over a non-MPD backend, incl. `lsinfo`/`search`/`find` over `context.browse()`; "kept on life support … Maintainer wanted" status |
| docs.nuclearplayer.com/nuclear/integrations/mpd-server | A GUI player exposing an MPD subset (playback control, queue, notifications; no library browsing or stored playlists), `127.0.0.1:6600` with 6601-6609 fallback — **page blocked by this environment's egress proxy; read via search-index summary, re-verify the exact command list before implementing** |
| https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/doc/protocol.rst, https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/doc/user.rst | **Primary MPD spec source, reachable even though `mpd.readthedocs.io`/`musicpd.org` are blocked.** Command/response framing, `idle` subsystems, `status` fields, `commands`/`notcommands`, `protocol` negotiation, binary framing, `local_permissions`/`host_permissions`/`password` access control |
| https://wiki.archlinux.org/title/Music_Player_Daemon | MPD port 6600, password config, client list |
| https://github.com/M0Rf30/rmpd | A Rust MPD-protocol *server* implementation (young: 244 commits, 15 stars) |
| https://crates.io/crates/mpd_protocol, https://crates.io/crates/mpd | Rust MPD *client* libraries (not server) |
| https://raw.githubusercontent.com/devgianlu/go-librespot/master/README.md, `/master/API.md`, `/master/api-spec.yml` | REST API + WebSocket `/events` vocabulary, MPRIS, `zeroconf_backend` builtin/avahi, audio backends, `GET /auth/code`, `server.{address,port,allow_origin,cert_file,key_file}` (no `tls` key), encrypted-file cache design |
| https://raw.githubusercontent.com/Spotifyd/spotifyd/master/docs/src/advanced/dbus.md, `/advanced/mpris.md`, `/advanced/hooks.md` | `rs.spotifyd.Controls` on `rs.spotifyd.instance$PID` (always present) vs. `org.mpris.MediaPlayer2.spotifyd.instance$PID` (only once active), `--dbus-type system`, `--onevent` — **reachable at this raw path; `docs.spotifyd.rs` itself is blocked by this environment's egress proxy** |
| https://raw.githubusercontent.com/librespot-org/librespot/dev/README.md, `/dev/Cargo.toml` | librespot as a Connect-receiver binary; workspace crate split (`core`/`audio`/`playback`/`metadata`/`protocol`/`oauth`/`discovery`/`connect`); `rustls` option for musl/static builds |
| https://github.com/jpochyla/psst/blob/main/README.md | `psst-core` + `psst-gui`: a second core/GUI split, inspired by librespot rather than depending on it |
| https://raw.githubusercontent.com/hrkfdn/ncspot/main/doc/users.md | ncspot's Unix-domain-socket/named-pipe NDJSON control surface (`ncspot info`), alongside embedding librespot as a library |
| https://raw.githubusercontent.com/badaix/snapcast/develop/README.md, `/develop/server/etc/snapserver.conf`, `/develop/doc/configuration.md`, `/develop/doc/json_rpc_api/stream_plugin.md` | **Canonical repo is `badaix/snapcast`** (`snapcast/snapcast` redirects). Architecture, sources, codecs, sync accuracy, ports 1704/1705/1780/1788, four mDNS service types, fixed per-stream `sampleformat`, stream-plugin JSON-RPC protocol, `process://` source; librespot integration is "launches librespot and reads audio from stdout" (subprocess, not a linked crate) |
| https://raw.githubusercontent.com/music-assistant/server/dev/music_assistant/providers/snapcast/constants.py | Built-in snapserver control port is 1705 at `127.0.0.1`, not 1780 |
| https://raw.githubusercontent.com/Sendspin/spec/main/README.md | Sendspin core v1: `ws://` + Noise `KKpsk2`, `_sendspin._tcp.local.` (8928) / `_sendspin-server._tcp.local.` (8927), binary frame type bytes incl. 2=Pairing/3=Reserved, time-filter clock sync, QR pairing tokens, Sentinel PSK, in-band rehandshake |
| https://github.com/orgs/music-assistant/discussions/4200 | Sendspin (formerly Resonate) origin, Open Home Foundation |
| https://raw.githubusercontent.com/music-assistant/server/dev/README.md | Music Assistant server architecture, `localhost:8095`, ffmpeg 6.1+, Python 3.14+ |
| https://github.com/music-assistant/server/blob/dev/music_assistant/providers/sendspin/README.md | `ws://<ma-server-ip>:8927/sendspin`, WebRTC for remote, spec at github.com/Sendspin/spec |
| https://raw.githubusercontent.com/GioF71/upmpdcli-docker/main/README.md, discussion #281 | upmpdcli's TIDAL plugin: python-tidal based, `TIDAL_AUDIO_QUALITY`, pre-provisioned token env vars, `TIDAL_ENABLE_USER_AGENT_WHITELIST`; plugin version table (0.8.12 release / 0.8.16 master+edge as of 2026-09 — treat any cited version as a dated snapshot) |
| https://github.com/GioF71/audio-tools/blob/main/media-servers/tidal-hires/README.md | Hi-res renderer whitelist (MPD+upmpdcli, gmrender-resurrect, WiiM Pro/Pro Plus) |
| https://github.com/michaelherger/lms-plugin-tidal (issues #35, #88, #89, #98, #100, #113) | Lyrion/LMS TIDAL plugin and its hi-res difficulties — mixed state, not simply broken |
| https://github.com/mikebrady/shairport-sync | AirPlay 2 *receiver* on Linux — why streamboat needn't build one |
| https://github.com/philippe44/libraop, https://github.com/music-assistant/airplay-cli, https://github.com/akustikrausch/airplay2-sender-cpp, https://github.com/owntone/owntone-server | AirPlay *sender* precedents |
| https://github.com/rgerganov/shanocast | A working open-source Chromecast **receiver** — passes Chrome's auth only by reusing AirReceiver's precomputed device signatures. (`xakcop.com/post/shanocast/`, the original write-up, is blocked by this environment's egress proxy; cite this repo instead) |
| https://developers.google.com/cast/docs/media | Google Cast media quality ceiling: FLAC up to 96 kHz/24-bit only |
| https://crates.io/crates/cast-sender, https://crates.io/crates/rust_cast | Rust CASTV2 sender crates |
| https://raw.githubusercontent.com/keepsimple1/mdns-sd/main/README.md, https://crates.io/crates/mdns-sd | Rust mDNS responder+querier; RFC 6762 Probing/Tiebreaking/Conflict Resolution, `DnsNameChange` |
| https://crates.io/crates/windows-service | Rust Windows service scaffolding |
| https://learn.microsoft.com/en-us/windows/win32/coreaudio/wasapi | WASAPI reference — no first-party statement found on session-0 service audio; treat as unverified pending the spike in §15 |
| https://specifications.freedesktop.org/mpris/latest/ | MPRIS D-Bus Interface Specification v2.2 |
| https://raw.githubusercontent.com/SeaDve/mpris-server/main/README.md | `mpris-server` 0.9 implements `TrackList` and `Playlists`, not just Root+Player |
| https://docs.rs/souvlaki | Cross-platform OS media controls (MPRIS / SMTC / MPNowPlayingInfoCenter) |
| help.roonlabs.com/portal/en/kb/articles/raat | Roon Core/Remote/endpoint model and RAAT as a closed vendor-implemented transport — **blocked by this environment's egress proxy; corroborated via the Music Assistant discussion below instead** |
| https://github.com/orgs/music-assistant/discussions/5133 | RAAT is unavailable to open-source implementers |
| https://w3c.github.io/webappsec-secure-contexts/ | Only `https://` and `localhost`/`127.0.0.1` are secure browser contexts — bears on the plain-HTTP LAN web remote (§17) |

### Sources wanted but unreachable

`tidal.com/connect`, `tidal.com/supported-devices`, `developer.tidal.com/documentation/open-source`,
`www.music-assistant.io/*`, `www.lesbonscomptes.com/upmpdcli/*`, `mpd.readthedocs.io`,
`www.musicpd.org`, `docs.nuclearplayer.com`, `deepwiki.com`, `protodoc.io`, `docs.spotifyd.rs`,
`help.roonlabs.com`, `xakcop.com` — all blocked by this environment's egress proxy. **The last three
were missing from this list in an earlier draft despite being cited as read** — a citation-hygiene
gap a fact-check caught. Where a reachable primary exists, this document now cites that instead and
notes the substitution inline: spotifyd's docs live in-repo at
`raw.githubusercontent.com/Spotifyd/spotifyd/master/docs/src/advanced/{dbus,mpris,hooks}.md`; the
Chromecast-receiver claim is cited to `github.com/rgerganov/shanocast` directly; Roon/RAAT is
corroborated via the Music Assistant discussion thread instead. Anything else attributed to a blocked
domain above came from search-result summaries or mirrors and is flagged accordingly.
