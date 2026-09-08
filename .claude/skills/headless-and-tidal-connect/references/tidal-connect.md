# TIDAL Connect: full dissection, feasibility, and legal posture

Table of contents:
1. What Connect is and why it doesn't apply to a headless daemon's own DSP
2. The only working Linux target, dissected in full
3. Discovery: mDNS, and the TXT-record experiment
4. The controller side: what the desktop client's own Redux store reveals
5. Feasibility and legal posture of all three options, plus two owner-facing positioning questions

---

## 1. What Connect is

TIDAL Connect is TIDAL's "cast to a device" feature, modelled on Spotify Connect: the TIDAL app
becomes a remote control and a network audio device becomes the player. The target fetches the
stream from TIDAL directly — the phone can sleep, and audio never traverses the phone.
[documented-web: https://www.whathifi.com/features/tidal-connect-everything-you-need-to-know]

Consequences for a client author:

- **Local DSP does not apply.** streamboat's replay-gain chain, resampler choice, exclusive-mode
  output and gapless implementation are all bypassed when the user hands off to a Connect target.
- **The controller role is catalogue + queue + control, not audio.** TIDAL also keeps a server-side
  queue (`cloudQueue/*`, §4) so handoff does not require the LAN.
- **The target role requires a device identity issued by TIDAL** (§2).

Ecosystem scale (trade press, not TIDAL's own site): SDK opened to hardware developers in 2021;
500+ compatible devices by 2023; 1M+ Connect devices claimed by 2025; partner brands include
Bluesound/BluOS, NAD, Naim, KEF, Cambridge Audio, iFi Audio, Marantz, McIntosh, Focal, WiiM, dCS,
DALI, Dynaudio, Electrocompaniet, Esoteric. [documented-web, low-to-medium reliability — an
aggregator page (ampvortex.com) reads as SEO content; the brand list is corroborated by
whathifi.com's launch coverage]

`tidal.com/connect`, `tidal.com/supported-devices` and `developer.tidal.com` are **blocked by this
environment's egress proxy** — every licensing claim here is second-hand. [unverified]

---

## 2. The only working Linux target, dissected in full

Every open-source "TIDAL Connect target" on Linux is the same closed binary. Lineage:

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

**The root of this chain has since disappeared from GitHub**: `github.com/ppy2/ifi-tidal-release`
now returns HTTP 404. `seniorgod/ifi-tidal-release` still describes itself as "tidal connect
application for ARM SBC based on https://github.com/ppy2/ifi-tidal-release", confirming the lineage
even though the origin repo itself is gone. Record this as "origin repo no longer reachable" — it is
not evidence either way on whether TIDAL has ever enforced against redistributors (§5's "no takedown
found" caveat stands on its own). [documented-web: https://github.com/ppy2/ifi-tidal-release (404)]

### 2.1 What the wrapper contributes vs. the binary

`ref:tidal-connect` is MIT-licensed and **contains no TIDAL binary**: "This repository does not
contain any tidal-connect binary" (`ref:tidal-connect/README.md:3-4`). It contributes ALSA device
resolution, an `/etc/asound.conf` generator, a test tone, a restart loop, and a compose file.

**The binary/certificate paths in §2.2 are fallback defaults, not the only option.**
`ref:tidal-connect/bin/entrypoint.sh:59-99` prefers, in order: a user-supplied
`/assets/custom/bin/tidal_connect` (+ matching `tidal_connect.dat`) over the shipped iFi binary; a
user-supplied `/assets/custom/certificate/tcon.crt` over the shipped iFi certificate; or an explicit
`$CERTIFICATE_PATH`. In practice no alternative binary or certificate has ever surfaced in the fork
ecosystem — but the design does not assume iFi's is the only one that will ever exist, so don't
describe "the iFi binary and certificate" as hardcoded when writing about this wrapper.

Container dependencies (`ref:tidal-connect/build/Dockerfile`, base `debian:bookworm-slim`):
`ca-certificates`, `alsa-utils`, `libportaudio-ocaml`, `libssl-dev`, `libavahi-client3`,
`libcurl4`, `libavformat-dev` (in that order). The upstream binary's own stated runtime deps are
older: `libssl1.0.0`, `libportaudio2`, `libflac++6v5`. [documented-web:
https://raw.githubusercontent.com/shawaj/ifi-tidal-release/master/README.md]

Compose service shape (`ref:tidal-connect/docker-compose.yaml`) — every line is a requirement:

```yaml
image: ${TIDAL_CONNECT_IMAGE:-edgecrush3r/tidal-connect:latest}
network_mode: host          # mandatory: mDNS is multicast, bridge networking breaks discovery
devices: [ /dev/snd ]       # direct ALSA access
volumes:
  - /var/run/dbus:/var/run/dbus   # Avahi client talks to the host avahi-daemon over D-Bus
dns: [ ${DNS_SERVER_LIST:-8.8.8.8} ]
restart: unless-stopped
```

### 2.2 The invocation — every flag, confirmed

`ref:tidal-connect/bin/entrypoint.sh` builds and `eval`s:

```
/app/ifi-tidal-release/bin/tidal_connect_application \
  --tc-certificate-path <cert>        # default /app/ifi-tidal-release/id_certificate/IfiAudio_ZenStream.dat
  --playback-device <alsa device>     # resolved at container start, see §2.4
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

**A twelfth flag exists upstream and is missing from GioF71's wrapper**: the systemd unit shipped by
`shawaj/ifi-tidal-release` invokes the binary with an additional `--netif-for-deviceid eth0` — the
Connect device id is derived partly from a named network interface (its MAC), *alongside* the
certificate. This is a second identity input; it is likely why the same reused certificate does not
visibly collide across many independent community installs. [confirmed, documented-web:
https://raw.githubusercontent.com/shawaj/ifi-tidal-release/master/README.md, the pasted `[Service]`
`ExecStart` block]

`--enable-websocket-log` and `--disable-web-security` both point at a WebSocket/HTTP control
surface inside the binary; `--clientid` shows the target carries a TIDAL OAuth client id like every
other TIDAL client.

An optional `speaker_controller_application` runs first, inside `tmux`, when present
(`DISABLE_CONTROL_APP=0`, `SLEEP_TIME_SEC=3` before launching the main app). Its role in the forks
is to bridge metadata and volume: TonyTromp's stack runs a "volume-bridge" service that exports
playback metadata and syncs the phone's volume changes to the ALSA mixer. [documented-web:
https://github.com/TonyTromp/tidal-connect-docker]

### 2.3 What the binary links against — the strongest protocol evidence available

`licenses/tidal_connect/` in the ifi release lists eight third-party licences, confirmed via
directory listing: [documented-web:
https://github.com/shawaj/ifi-tidal-release/tree/master/licenses/tidal_connect]

| Library | What it implies |
| --- | --- |
| `mdnsresponder` | Apple Bonjour — the target advertises itself over mDNS/DNS-SD |
| `websocketpp` | a WebSocket endpoint — almost certainly the control channel |
| `asio`, `boost` | the C++ async plumbing under both |
| `curl`, `openssl` | HTTPS to TIDAL's API/CDN, and TLS identity from the certificate |
| `clara` | the CLI flag parser seen above |
| `advobfuscator` | **compile-time obfuscation of strings and control flow** |

`advobfuscator` is the tell: someone deliberately made this binary unpleasant to reverse. Combined
with the per-device certificate, this is a clear signal that TIDAL treats the Connect target
protocol as a licensed, gated interface — not an undocumented-but-tolerated one like the streaming
API. [inferred, high confidence]

A runtime error string GioF71's README quotes, `[tisoc] [error] [avahiImpl.cpp:358]
avahi_client_new() FAILED: Daemon not running`, shows at least some builds use Avahi rather than
Bonjour — consistent with `libavahi-client3` in the Dockerfile and the `/var/run/dbus` mount. The
`mdnsresponder` licence is probably a different build target (the ZEN Stream firmware itself) or a
fallback backend. [verified-source + inferred]

### 2.4 Operational constraints the wrapper exists to solve

All from `ref:tidal-connect/README.md` and `ref:tidal-connect/bin/common.sh`:

- **IPv6 is mandatory.** "Tidal connect won't work if your system does not support ipv6" (issue
  #21). No workaround known.
- **avahi-daemon must be running.** Not installed by default on DietPi.
- **ALSA card indices move; the app does NOT uniformly open `default`.** `write_audio_config()`
  emits `pcm.tidal-audio-device` + `pcm.tidal-softvol` and passes `tidal-softvol` when softvol is
  enabled (`ENABLE_SOFTVOLUME=yes`, the shipped default); it passes `$CREATED_ASOUND_CARD_NAME` when
  that variable is set and softvol is off; it passes `custom` when the user supplies their own
  `asound.conf` via `userconfig/`; and it falls back to `default` only when none of the above apply.
  `entrypoint.sh` then invokes the binary with `--playback-device $(get_playback_device)`. Indices
  change simply because a USB DAC was or wasn't powered on at boot.
- **Software volume needs care.** `common.sh` runs `amixer -c <idx> controls | grep 'Master'`; if no
  `Master` exists it creates a softvol named `Master`; if one already exists it creates
  `SoftMaster` and logs a warning that the TIDAL slider will move the *hardware* volume and affect
  every other player on that card.
- **Exclusive device locking.** "Tidal Connect will access exclusively your audio device if you
  select it in your … Tidal App." Check with `watch cat /proc/asound/<card>/pcm0p/sub0/hw_params` —
  anything other than `closed` means busy.
- **Pre-flight test tone, and it is opt-out.** `aplay -D $PLAYBACK_DEVICE
  /assets/audio/short-low-tone-48k.wav`, falling back to the 44.1 kHz file; the app starts if a tone
  played *or* if `ENABLE_GENERATED_TONE=no` was set (`tone_skipped=1`), which skips the device check
  entirely.
- **Multi-word friendly names break Avahi for some users** (issue #216) — the default was changed to
  a single word.
- **Raspberry Pi 5**: `tidal_connect_application: error while loading shared libraries:
  libsystemd.so.0: ELF load command alignment not page-aligned`, fixed by adding
  `kernel=kernel8.img` to `/boot/firmware/config.txt`.
- **Hardware sizing**: "A Raspberry Pi 3/4 will work. If you plan to use a usb dac and hi-res audio,
  consider at least using a Pi 3b+ or, even better, a Pi 4b." On an Asus Tinkerboard the author had
  to pin the minimum CPU frequency around 600 MHz to stop crackling. No measured decode-CPU
  benchmark exists anywhere in the reference set — don't quote one.

The repo ships **26** per-DAC `asound.conf` presets in `userconfig/` (not "~40" — count them
yourself: `ls ref:tidal-connect/userconfig/*.asound.conf | wc -l`) and a tested-device table in
`assets/known-devices.md` (Aune S6, Chord Qutest, FiiO K11, Fosi DS1, HiFiBerry DAC+/Digi+ Pro, iFi
ZEN DAC V2, IQaudIO DAC, Topping D10, SMSL A8, Yulong D200, Apple USB dongle, RPi HDMI/headphone
outputs, …). **This table and these presets are directly reusable as streamboat's own ALSA device
compatibility database.**

### 2.5 Quality ceiling — the decisive product argument

After TIDAL removed all MQA content at the end of July 2024, this implementation "could play
hi-res files only up to 24/48 and MQA content" — with in-app unfolding to 24/88 or 24/96 — and is
now effectively limited to 16/44.1 redbook (`ref:tidal-connect/README.md:57-62`). The README lists
alternatives that *do* reach 24/192: mopidy-tidal, upmpdcli's TIDAL plugin (with a renderer
whitelist), Music Assistant, BubbleUPnP, Audirvana, Roon.

**A streamboat headless daemon on a Pi beats the only available Connect target on sound quality**,
because it can request `HI_RES_LOSSLESS` and hand a 24/192 FLAC to ALSA directly. Say so in the
README, next to the "we are not a Connect target" statement.

---

## 3. Discovery: mDNS, and the TXT-record experiment

- **Service type: `_tidalconnect._tcp`.** Verify with `avahi-browse -t _tidalconnect._tcp` or
  `avahi-browse -a | grep -i tidal`. Records carry a ~120 s TTL, which is why a rapidly restarting
  target can collide with its own stale advertisement. [confirmed, documented-web:
  https://raw.githubusercontent.com/TonyTromp/tidal-connect-docker/master/docs/TROUBLESHOOTING.md]
- The advertisement publishes at least the service (friendly) name and model name — the two values
  passed as `-f` and `--model-name`.
- **Port and TXT record keys are not documented in any source read for this project, but this is a
  15-minute experiment, not a research dead end.** Run `avahi-browse -r -t _tidalconnect._tcp` (the
  `-r` resolve flag — the troubleshooting page's own examples omit it) against any Connect-capable
  device already on a LAN (a Volumio/moOde box, a WiiM, a BluOS speaker), and it prints hostname,
  IPv4/IPv6 address, port and the full TXT record. On macOS: `dns-sd -B _tidalconnect._tcp` then
  `dns-sd -L <name> _tidalconnect._tcp`. This is worth running on the owner's own LAN — it costs
  minutes and settles both a possible diagnostics feature and the "do not collide with TIDAL's
  service type" rule.
- Host networking is required for the container because multicast does not cross Docker's bridge.

---

## 4. The controller side: what the desktop client's own Redux store reveals

`ref:TidaLuna` is a mod loader running *inside* the official Electron client, so its TypeScript
types are a faithful transcription of the official client's own Redux state — the best available
window onto the device picker.

`ref:TidaLuna/plugins/lib/src/redux/types/store/RemotePlayback.ts` — verbatim:

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
- **The Connect message vocabulary is Cast-shaped.** `mediaId` + `customData` + `metadata`, a queue
  object with `maxBeforeSize`/`maxAfterSize` and a `queueId`, `RemotePlayerStates` of
  `BUFFERING|IDLE|PAUSED|PLAYING`. Combined with `websocketpp` in the target binary, the most likely
  design is a JSON-over-WebSocket control channel with a Cast-like namespace/message model. **This
  is shape resemblance, not proof** — no wire capture was available. [inferred, medium confidence]
- **`cloudConnect` + `cloudQueue` mean handoff does not require the LAN — and the server-side queue
  behind this turns out to be officially documented, correcting what this reference previously
  said.** [refuted-and-corrected]
- **The official SDKs (web, Android, iOS) contain no `remotePlayback`/Connect/device-picker
  *transport* code at all.** Searching `ref:tidal-sdk-web`, `ref:tidal-sdk-android`,
  `ref:tidal-sdk-ios` for the literal strings `remotePlayback`, `cloudQueue`, `tidalConnect` returns
  nothing. **This half is confirmed**: the Connect wire protocol and the desktop client's
  `cloudConnect` device-discovery/handoff mechanism are not in the public SDK surface — confirmed as
  a negative result.
- **But all three SDKs bundle the official OpenAPI spec for `openapi.tidal.com/v2`, and it documents
  a full server-side play-queue resource — `/playQueues` — that this reference previously said did
  not exist anywhere.** [refuted-and-corrected, verified-source
  `ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json` (spec version 1.10.104); also present in
  `ref:tidal-sdk-android/tidalapi/bin/tidal-api.json` and
  `ref:tidal-sdk-ios/Sources/TidalAPI/Config/input/tidal-api-oas.json`, with a generated Swift client
  at `ref:tidal-sdk-ios/Sources/TidalAPI/Generated/OpenAPIClient/Classes/OpenAPIs/APIs/PlayQueuesAPI.swift`
  and a generated TS client in `ref:tidal-sdk-web/packages/api/src/allAPI.generated.ts`]
  - **Routes**: `GET/POST /playQueues`; `GET/PATCH/DELETE /playQueues/{id}`; `GET/PATCH
    /playQueues/{id}/relationships/current` (to-one, the now-playing item); `GET/PATCH/POST/DELETE
    /playQueues/{id}/relationships/future` (to-many, the upcoming tail); `GET
    /playQueues/{id}/relationships/past` and `/owners`.
  - **Attributes**: `createdAt`, `lastModifiedAt`, `repeat` (`NONE|ONE|BATCH`), `shuffle`
    (`OFF|BATCH|ALL`), `shuffled`. Item resource identifiers carry `meta {batchId (uuid), itemId,
    legacySource, replacement}`.
  - **Auth**: PKCE scopes `r_usr` (read) / `w_usr` (write) under `Authorization_Code_PKCE` — a
    **different auth stack** from the unofficial `api.tidal.com` v1 surface everything else in this
    skill set assumes. That is the real catch, not "undocumented": using this resource means running
    a second, official PKCE login alongside the unofficial-API session, or not using it at all.
  - **What this settles and what it doesn't**: `current`/`future`/`past` map closely onto the desktop
    client's `cloudQueue` shape above (`currentItemId`/tail/history), and `repeat`/`shuffled` map
    onto `RepeatMode`/`shuffled` — so a documented, officially supported, cross-device server-side
    queue does exist. streamboat could publish and consume the **user's own** cloud queue through the
    sanctioned API, handing off between streamboat's desktop app, its daemon and the official TIDAL
    app without touching the closed Connect transport at all — a real product option this reference
    previously foreclosed by mistake. What remains genuinely undocumented: the `cloudConnect`
    **transport** itself (how a device announces itself and receives a push when the queue changes),
    and the `etag`/`itemsEtag` concurrency fields the desktop client's Redux store uses — neither
    appears in the `/playQueues` OpenAPI spec. So "cloudConnect the wire protocol" is still closed;
    "a server-side queue resource" is not.

**Conclusion, revised**: the Connect **transport** (mDNS target discovery, device-to-device handoff,
`cloudConnect`) is as closed as the target side — streamboat can enumerate `_tidalconnect._tcp`
devices on the LAN (§3) but has no documented way to connect to one, and building one from scratch
means reverse-engineering an obfuscated binary's WebSocket protocol *and* solving controller-side
authentication. But **queue handoff between a user's own streamboat instances and their official
TIDAL app is a separate, documented, officially-sanctioned problem** (`/playQueues` above) that does
not require any of that reverse-engineering. Open sub-question for the owner: whether a queue written
through this official PKCE API is visible to the unofficial-API session the rest of streamboat uses,
and whether the two logins can be merged into one UX (see the Open decisions section of `SKILL.md`).
[inferred, high confidence on the transport half; confirmed on the queue-API half]

For the client-side Connect/device-picker data model as it applies to streamboat's own future
"remotes" UI (not the wire protocol), see the `tidal-client-features` skill's
`remote-playback-connect-controls.md` reference, which covers the same Redux types from the
GUI-feature angle plus `tidal://` deep links and OS media controls.

---

## 5. Feasibility and legal posture of the three Connect options

| Option | Technically possible? | Legal / distribution posture | Verdict |
| --- | --- | --- | --- |
| **(a) Bundle the proprietary `tidal_connect_application`** | Yes on ARM Linux — exactly what `edgecrush3r/tidal-connect` does | Redistributing an unlicensed binary extracted from device firmware, plus **iFi's device certificate**, is copyright infringement and identity misuse. GioF71 explicitly keeps the binary *out* of his repository and ships config only. The whole chain survives on obscurity, not permission. | **Reject.** Incompatible with an open-source project that wants Flathub/distro packaging. |
| **(b) Reimplement the Connect target (or controller) protocol** | No precedent exists anywhere. Requires defeating `advobfuscator`, recovering the WebSocket protocol, **and** obtaining or forging a device certificate. | Reverse-engineering a security/identity mechanism is the clearest possible fit for TIDAL's consumer-terms prohibition on "circumventing or modifying … any security technology" (see the `tidal-api` skill's legal reference). Unlike using the unofficial streaming API with a paid account, this is not a defensible gray area. | **Reject.** Highest legal risk in the whole project, for a feature capped in quality anyway. |
| **(c) Skip Connect; offer a streamboat-native remote** | Yes, entirely under streamboat's control | No TIDAL IP involved; the daemon is just another logged-in subscriber client. | **Adopt.** See `daemon-architecture.md` and `mpd-and-multiroom.md`. |

**What streamboat should still do about Connect** (cheap, honest, useful):

- Document plainly, in README and in-app, that streamboat is not a TIDAL Connect target or
  controller, and why. Point users at the official app for handing off to Connect hardware.
- Do **not** advertise `_tidalconnect._tcp`, and do not attempt to spoof a Connect device — that
  would be impersonation.
- Optionally *list* discovered `_tidalconnect._tcp` devices in a diagnostics view, clearly labelled
  "not controllable from streamboat". Low value; include only if free.
- Handle the consequence users will hit: if they start playback on a Connect speaker from the
  official app, streamboat's Pushkin socket will fire `PRIVILEGED_SESSION_NOTIFICATION` and
  streamboat must pause gracefully with a clear message (`daemon-architecture.md` §6).

**Two adjacent positioning questions the owner still needs to answer** (the technical "should we
build it" question above is already settled — these are different, and public-facing):

1. **Does streamboat ever approach TIDAL for legitimate Connect partner/SDK status?** This report
   establishes that target identity is a vendor-issued certificate obtainable only through a
   commercial partnership, and that `developer.tidal.com` was unreachable — so cost, terms, and
   whether an open-source project can even apply are *unknown*, not *impossible*. Decide whether to
   ask TIDAL directly (a documented answer either way is useful to the wider community of these
   projects) or to state publicly that streamboat will never be a Connect device — and put that
   reasoning in the README next to the existing "we are not a Connect target" statement.
2. **Does streamboat's documentation point users at the GioF71/TonyTromp Docker container as a
   companion for Connect on the same Pi?** That container runs a binary extracted from iFi firmware
   together with iFi's device certificate. Recommending it as a companion is a positioning choice
   with the same flavour as this report's own "the whole chain survives on obscurity, not
   permission" assessment. Decide it explicitly rather than leaving it implicit in whatever the
   README ends up saying.

**Unverified in this section**: TIDAL's actual partner terms (blocked domain); whether the Connect
control channel really is JSON-over-WebSocket (inferred, no wire capture); the `cloudConnect`
**transport** and the `etag`/`itemsEtag` concurrency fields (undocumented — but not the
`/playQueues` queue resource itself, which is documented, see §4); whether TIDAL has ever acted
against Connect binary redistributors (no takedown found, absence of evidence only — and the origin
repo's own 404 (§2) is not evidence either way).
