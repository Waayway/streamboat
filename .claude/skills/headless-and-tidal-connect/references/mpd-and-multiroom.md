# MPD compatibility, multiroom, and cast-to-device options

Table of contents:
1. MPD protocol facts, access control, and an honest subset design (including browsing)
2. Snapcast: ports, sources, mDNS, stream-plugin protocol, and the pipe format constraint
3. UPnP/DLNA renderers
4. Sendspin: pairing, framing, clock sync, versioning
5. Chromecast and AirPlay: sender feasibility and quality ceilings; receivers out of scope
6. Design options and the full comparison table

---

## 1. MPD protocol facts, access control, and an honest subset design

**Protocol facts, confirmed against the primary spec source** —
`raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/doc/{protocol.rst,user.rst}` is reachable
even though `mpd.readthedocs.io`/`musicpd.org` are blocked from this research environment. Cite the
spec source directly, not search summaries.

- Default TCP port **6600** ("If no port is specified, the default port is 6600" — `doc/user.rst`);
  Unix-socket listening is also supported.
- Line-oriented text protocol; server greets with `OK MPD <version>`; a command "returns `OK` on
  completion or `ACK <error>` on failure", full form `ACK [error@command_listNum] {current_command}
  message_text`, e.g. `ACK [2@1] {play} Bad song index`.
- Command lists: `command_list_begin`/`command_list_ok_begin` … `command_list_end`. **`idle` and
  `noidle` are explicitly not allowed inside a command list.**
- **`idle [SUBSYSTEMS…]`** blocks until something changes and replies `changed: <subsystem>` — full
  subsystem list: `database`, `update`, `stored_playlist`, `playlist`, `player`, `mixer`, `output`,
  `options`, `partition`, `sticker`, `subscription`, `message`, `neighbor`, `mount`.
- **`status` fields** an MPD client actually renders: `partition`, `volume`, `repeat`, `random`,
  `single` (`0|1|oneshot`), `consume` (`0|1|oneshot`), `playlist` (a monotonically increasing 31-bit
  **queue version** — the standard way a client detects a changed queue without diffing),
  `playlistlength`, `state`, `song`/`songid`, `nextsong`/`nextsongid`, `elapsed`, `duration`,
  `bitrate`, `xfade`, `mixrampdb`, `mixrampdelay`, `audio` (`samplerate:bits:channels` — **this is
  how an MPD client displays 24-bit/192 kHz today**, the honest-quality-reporting hook for MPD
  mode), `updating_db`, `error`, `lastloadedplaylist`.
- **Capability advertisement and version negotiation, relevant to streamboat's own protocol
  versioning decision** (`daemon-architecture.md` §4): `commands`/`notcommands` list what the
  current connection may call; `protocol` / `protocol available` / `protocol enable {FEATURE}` /
  `protocol clear` negotiate optional behaviour additively. This is the "tolerant" alternative to
  Sendspin's exact-match versioning (§4).
- **Security is not "a single plaintext password and no encryption".** MPD has four access-control
  layers: `default_permissions` (baseline for unauthenticated clients); `local_permissions`
  (permissions for clients on a local Unix socket); `host_permissions` (per-IP or per-CIDR, e.g.
  `host_permissions "192.168.1.0/24 read,control"`); and any number of
  `password "secret@permissionset"` entries, each carrying its own permission set. **Only the
  transport-encryption half of the original claim holds**: "the password option is not secure:
  passwords are sent in clear-text over the connection, and the client cannot verify the server's
  identity." **This changes the risk picture for an MPD-compatible listener**: bind it to a Unix
  socket or loopback with `local_permissions`, and it needs no shared plaintext password at all —
  the weak link is specifically a *network* password, not MPD's access control in general.
- Binary responses exist for `albumart`/`readpicture`: a `binary: <n>` header line, then exactly
  `<n>` bytes, then the completion line; a client-settable `binarylimit SIZE` (since MPD 0.22.4)
  caps chunk size.
- **Partitions**: one MPD process can present multiple frontends with separate queue/player/outputs.

**Clients streamboat would inherit for free**: ncmpcpp, mpc, ncmpc (terminal); Cantata (Qt desktop);
MALP/M.A.L.P. (Android); numerous iOS clients; mpDris2 (bridges MPD → MPRIS).

**Two precedents for implementing a *subset* — and they prove more than "transport control only":**

- **Mopidy-MPD** implements the MPD protocol over a *non-file* backend, and its music-database
  command set (`count`, `find`, `findadd`, `list`, `listall`, `listallinfo`, `listfiles`, `lsinfo`,
  `search`, `searchadd`, `searchaddpl`, `update`, `rescan`) is the proof that catalogue browsing
  maps cleanly onto a streaming service. `lsinfo` walks `context.browse(uri, recursive=False,
  lookup=True)` and emits `("directory", path)` for internal nodes plus translated tracks for
  leaves, with stored playlists surfaced at the root — a service catalogue mapped straight into
  MPD's directory model. This is exactly why MALP and ncmpcpp can browse a TIDAL library through
  mopidy-tidal today. **Do not scope catalogue browsing out of streamboat's own MPD subset on the
  theory that "MPD fits a streaming catalogue awkwardly" — it doesn't, once you copy this mapping.**
  The repository's own maintenance status is worth carrying alongside the precedent: it currently
  states it is "kept on life support by the Mopidy core developers" with a "Maintainer wanted"
  notice — sound design, uncertain long-term maintenance.
- **Nuclear** — a desktop GUI player with a built-in MPD-compatible server that works with `mpc`,
  `ncmpcpp` and `mpDris2`, implementing "the subset of the protocol needed for playback control,
  queue management, and real-time notifications", with library browsing and stored playlists
  explicitly not supported (the opposite scoping choice from Mopidy-MPD — name this fork explicitly
  as a decision, don't let it happen by default). **Port behaviour** (recovered via search index
  since `docs.nuclearplayer.com` is blocked — re-verify the exact command list once the page is
  reachable): it binds `127.0.0.1:6600` only, and if 6600 is already taken — a real collision on a
  box already running `mpd`, exactly the audiophile Linux target streamboat cares about — it tries
  6601, 6602, … up to 6609. It supports `command_list_begin`/`command_list_ok_begin`/
  `command_list_end` and emits change notifications for playback state, current track, queue
  contents, volume, and repeat/random/single mode. **Copy this port-fallback behaviour** for
  streamboat's own MPD listener.

**Rust tooling**: `mpd_protocol` and `mpd` on crates.io are **client-side**; `M0Rf30/rmpd` is a
from-scratch MPD *server* in Rust ("Modern, high-performance MPD server written in pure Rust with
DSD support, multi-room audio, and extensible plugin architecture", 244 commits, 15 stars — young,
not a drop-in dependency). Go has `fhs/gompd` (client). There is no drop-in "embed an MPD server in
your app" library for either language — implementing the subset is a hand-written parser plus a
state machine, on the order of a few thousand lines.

**streamboat's recommended MPD-subset design, pulling the above together**: loopback/Unix-socket
bound by default (so `local_permissions` covers auth with no shared password); port 6600 falling
back through 6601-6609 (Nuclear); `status`, `currentsong`, transport commands, queue commands,
`setvol`, `idle` with the full subsystem list, `albumart`/`readpicture`; **and** `lsinfo`/`search`/
`find` over a virtual browse tree (Home / My Collection / Playlists / Mixes) plus
`listplaylists`/`listplaylistinfo` for TIDAL playlists, following Mopidy-MPD's `context.browse()`
mapping. Document explicitly which commands are supported and which are not.

---

## 2. Snapcast: ports, sources, mDNS, stream-plugin protocol, and the pipe format constraint

Snapcast (canonical repo **`badaix/snapcast`** — `snapcast/snapcast` redirects) is a client/server
synchronized multiroom player: the server reads PCM from a source, timestamps and encodes it, and
clients play in sync — "typically the deviation is below 0.2ms".

- **Ports**: 1704 (TCP, binary audio + time sync to clients), 1705 (TCP JSON-RPC control), 1780
  (HTTP + WebSocket JSON-RPC + the Snapweb UI, 1788 for SSL). Bind addresses configurable via
  `--stream-bind-address`/`--control-bind-address`/`--http-bind-address` or the `[tcp-streaming]`,
  `[tcp-control]`, `[http]` config sections.
- **mDNS**: snapserver advertises itself with four service types — `_snapcast._tcp` (streaming,
  1704), `_snapcast-ctrl._tcp` (control, 1705), `_snapcast-http._tcp` (1780), `_snapcast-https._tcp`
  (1788). Directly relevant to streamboat's own discovery design (`daemon-architecture.md` §3) — a
  streamboat daemon emitting a Snapcast pipe could *discover* a snapserver via these types rather
  than requiring the user to hand-configure a FIFO path.
- **Sources**: named pipe (classically `/tmp/snapfifo`), ALSA capture, TCP, process stdout, plus
  purpose-built `librespot`/`airplay` source types in some builds. librespot itself is consumed here
  as a **subprocess** reading stdout, not a linked crate (`headless-daemon-precedents.md` §9).
- **Codecs**: PCM, FLAC (default), Vorbis, Opus.
- **Clients**: Linux, macOS, FreeBSD, Android, Windows, Raspberry Pi, and an ESP32 port.
- Requires accurate clocks (NTP/chrony) — see `raspberry-pi-deployment.md` §3.

**The integration cost is one output backend, with one real trade-off.** An option like
`--output pipe:/tmp/snapfifo` (or `--output stdout`) emitting interleaved PCM at a fixed format —
librespot's `pipe` backend and go-librespot's "raw named pipe for custom routing" are the exact
precedent, and it buys synchronized multiroom, ESP32 endpoints, and Home Assistant integration for
almost nothing. **But a FIFO carries no format metadata, and Snapcast's stream sources pin a fixed
`sampleformat` per stream** (e.g. `48000:16:2`); go-librespot's own docs warn the pipe output must
match ("go-librespot uses a sample rate of 44100 for the pipe output, you can either configure this
globally for snapcast or specify it only for the go-librespot source"). **So this output is mutually
exclusive with bit-perfect variable-rate output** — the daemon must resample every track to one
fixed format to feed it. Design it as an explicit output *mode*, not a free addition to the
"24/192 bit-perfect beats Connect" argument. On Linux, a FIFO in `/tmp` on a recent kernel may need
`fs.protected_fifos=0`; Snapcast's pipe source also takes a `mode=create|read` option worth
exposing.

**Don't stop at a bare PCM pipe — Snapcast defines a stream-plugin protocol for metadata and
control, and it's cheap.** A bare FIFO produces sound with no title, artist, cover art or transport
control anywhere in Snapweb or any Snapcast client. snapserver launches an executable per stream and
speaks newline-delimited JSON-RPC 2.0 over its stdin/stdout, configured as
`source = pipe:///tmp/snapfifo?name=streamboat&controlscript=meta_streamboat.py` (relative script
paths resolve under `/usr/share/snapserver/plug-ins`; snapserver always passes `--stream=<id>` and,
when HTTP is enabled, `--snapcast-host`/`--snapcast-port`). The plugin implements
`Plugin.Stream.Player.Control` (`play`, `pause`, `playPause`, `stop`, `next`, `previous`,
`seek{offset}`, `setPosition{position}`), `Plugin.Stream.Player.SetProperty` and
`Plugin.Stream.Player.GetProperties`, and may emit `Plugin.Stream.Player.Properties`,
`Plugin.Stream.Log` and `Plugin.Stream.Ready`; capabilities are booleans (`canGoNext`,
`canGoPrevious`, `canPlay`, `canPause`, `canSeek`, `canControl`). Shipped examples: `meta_mpd.py`,
`meta_mopidy.py`, `meta_go-librespot.py` — a `streamboat snapcast-plugin` subcommand following the
same shape is a few hundred lines and turns Snapweb into a complete remote, not just a speaker.
Separately, snapserver can *launch* streamboat itself via a `process://` source (with
`params`/`idle_threshold`/`wd_timeout`), which can replace a separate systemd unit on a box whose
only job is feeding Snapcast.

---

## 3. UPnP/DLNA renderers

- `upmpdcli` turns MPD into a UPnP AV/OpenHome renderer, and separately gateways TIDAL as a Media
  Server (`headless-daemon-precedents.md` §5). `gmrender-resurrect` is the other renderer that
  handles TIDAL hi-res via that plugin.
- BubbleUPnP (Android) and mConnect (iOS/Android) are *control points* with built-in TIDAL support;
  BubbleUPnP added hi-res FLAC in late November 2023. `ref:tidal-connect/README.md` recommends this
  path to hi-res users.
- **Does any UPnP renderer expose TIDAL natively? No** — the pattern is always *control point or
  media server* holds the TIDAL credentials, and the renderer just plays a URL. The renderer never
  knows about TIDAL.

**For streamboat**: implementing a UPnP renderer is low value (upmpdcli exists, MPD exists, and
renderers can't parse DASH manifests without a whitelist). Implementing a UPnP *media server* that
gateways TIDAL would duplicate upmpdcli's plugin. **Skip both.**

---

## 4. Sendspin: pairing, framing, clock sync, versioning

Sendspin (formerly "Resonate"), an Open Home Foundation protocol, is the 2026 development to watch.
Spec: https://raw.githubusercontent.com/Sendspin/spec/main/README.md

- Core version 1, **exact-match versioning**.
- Transport: WebSocket, explicitly plain `ws://` — "Confidentiality and integrity are provided end
  to end by the Noise layer inside the WebSocket payloads."
- Discovery: `_sendspin._tcp.local.` for server-initiated (client listens, recommended port **8928**)
  and `_sendspin-server._tcp.local.` for client-initiated (server listens, recommended port
  **8927**).
- **Framing**: binary frames after the Noise handshake; the first byte is the message type — the
  confirmed table is 0 = JSON, 1 = fragmentation, **2 = Pairing, 3 = Reserved**, 4–7 player, 8–11
  artwork, 12–15 source, 16–23 visualizer, 24–191 reserved for future roles, 192–255 custom roles.
- Codecs are configuration, not mandate — `opus`, `flac`, `pcm` are named as stream-config values.
- Clock sync: clients "MUST use the time-filter algorithm" (a two-dimensional Kalman filter,
  https://github.com/Sendspin-Protocol/time-filter) to translate microsecond server timestamps.
- Spec licence: **not stated in the README**. [unverified]
- Shipped in Music Assistant 2.8 (2026-03-25) including bridges that wrap AirPlay/Cast devices.
  Music Assistant's own endpoint is `ws://<server>:8927/sendspin`.

**Sendspin's pairing and identity design is a fully specified answer to streamboat's own headless-
pairing problem** (`daemon-architecture.md` §5), not just an analogy:

- Noise pattern `KKpsk2` with Curve25519 static keys serving as `client_id`/`server_id` (base64url,
  43 characters, persistent across reboots).
- A 32-byte long-term PSK established during pairing, mixed into every subsequent handshake.
- A **"Sentinel PSK"** fallback for unpaired sessions, so an unpaired client can still connect at a
  reduced trust level rather than being refused outright.
- **In-band rehandshake**: a session already in transport mode can be promoted to paired without
  dropping the WebSocket.
- **QR-code pairing tokens**: a version-1 token, a 24-byte code, 39 body characters — a fully
  specified, screen-free pairing flow for exactly the headless-box scenario.
- The **server** is always the Noise initiator, regardless of which side opened the connection —
  worth copying since it removes an asymmetry question from streamboat's own design.

Sendspin is architecturally what streamboat's own remote protocol would look like (mDNS + WebSocket
+ typed binary frames + clock sync + a fully specified pairing scheme). If it gains adoption,
implementing a Sendspin *source* would let streamboat feed a whole ecosystem of endpoints; even if
it doesn't, its pairing design is worth copying wholesale rather than reinventing streamboat's own.
Track it; do not bet v1 on it. [unverified: licence, and adoption trajectory outside Music
Assistant]

---

## 5. Chromecast and AirPlay: sender feasibility and quality ceilings; receivers out of scope

Both are "send audio somewhere else on the LAN", and both put streamboat in the position of either
handing over a URL or re-transmitting decoded audio.

**Chromecast (sender).** Rust support exists: `cast-sender` ("a fully asynchronous implementation of
the Google Cast CASTV2 protocol"), `rust_cast` (protobufs from the Chromium Open Screen mirror), and
the multi-target `fcast-sender-sdk`. Discovery is mDNS `_googlecast._tcp` — the same service type
that appears in TIDAL's own `RemotePlaybackDevice.fullname` example (`tidal-connect.md` §4). Default
Media Receiver app id is `CC1AD845`.

The blocker is not just the protocol, it is the media URL — and even solving that only buys a lower
ceiling than streamboat's own daemon. A Cast receiver fetches the content itself, so streamboat
would have to give it either (i) the signed TIDAL CDN URL — time-limited, may carry DASH manifests
the Default Media Receiver cannot parse, and hands a third-party device a credentialed URL; or (ii)
a URL served by streamboat itself, i.e. streamboat becomes an HTTP re-server of TIDAL audio on the
LAN (what Music Assistant does for its players). **And even a working Cast sender caps streamboat
below its own headless daemon**: Google Cast supports FLAC only up to 96 kHz/24-bit, and 24-bit at
176.4/192 kHz does not play on Chromecast Audio and can hang the device — the same class of quality
ceiling the report rejects the Connect binary for (`tidal-connect.md` §2.5).
[documented-web: https://developers.google.com/cast/docs/media]

**AirPlay (sender).** Precedents: `philippe44/libraop` (RAOP/AirPlay v2 player + library, Windows/
macOS/Linux x86 and ARM), `music-assistant/airplay-cli` ("unified command-line binary for streaming
to AirPlay 1 (RAOP) and AirPlay 2 devices"), `akustikrausch/airplay2-sender-cpp` (encrypted
RAOP/RTSP, ALAC, Apache-2.0, C++20), and OwnTone's `src/outputs/airplay.c`. AirPlay is a *push*
protocol: streamboat would decode to PCM/ALAC and transmit. No credentialed URL leaves the machine
(cleaner than Cast on that axis), but it is still re-transmission of decoded audio.

**macOS gets AirPlay for free.** Selecting an AirPlay device as the system output device routes
CoreAudio output there with no code in streamboat. Same for Windows with some devices.
Cross-platform parity is the only reason to implement a sender at all.

**Receiving is out of scope, but not because it is impossible.** shairport-sync already covers
AirPlay 2 receiving on Linux/FreeBSD, and UxPlay covers mirroring. A Chromecast receiver is **not**
technically blocked — `rgerganov/shanocast` is a working open-source one, built on Chromium's
Openscreen — but it passes Chrome's receiver authentication only by reusing **precomputed
signatures taken from AirReceiver**, another vendor's device credentials. That is the same "reuse
someone else's identity" move streamboat has already rejected for iFi's Connect certificate
(`tidal-connect.md` §5), which is the stronger and more honest reason to skip a Chromecast receiver
— not that the protocol can't be implemented. (`xakcop.com`, the original shanocast write-up, is
blocked from this environment; cite the `rgerganov/shanocast` repo directly instead.)

**Recommendation**: do not build Cast or AirPlay senders in v1. Build the Snapcast pipe/stream-
plugin (§2) instead — it already solves multiroom, including AirPlay/Cast endpoints via other
software, at a fraction of the effort. Revisit Cast/AirPlay senders only if users ask, and resolve
the re-transmission open question first (§6, open decisions).

---

## 6. Design options and the full comparison table

| # | Option | Effort | Ecosystem leverage | Security posture | "Simple but beautiful" fit | Mobile-future fit |
| --- | --- | --- | --- | --- | --- | --- |
| **a** | **CLI player** (`streamboat play <url>`, `search`, `now-playing`) | XS | none directly, but a working precedent (`headless-daemon-precedents.md` §8) already answers most open questions | trivial (no listener) | Excellent — a beautiful CLI is a real deliverable | none directly |
| **b** | **Daemon + JSON HTTP/WebSocket API + MPRIS + small web remote** | M | moderate: curl/scripts/Stream Deck/Home Assistant, and streamboat's own GUI | needs a real answer: loopback default, token, opt-in LAN | Good — one protocol, one document, one web page | **Best** — the mobile app speaks the same protocol |
| **c** | **MPD-protocol-compatible server, including browsing** | M–L (parser + state machine + `idle`) | **Highest** — MALP, ncmpcpp, Cantata, mpc, mpDris2 work on day one | `local_permissions`/`host_permissions` mean a Unix-socket- or loopback-bound listener needs no shared password at all (§1) | Good — Mopidy-MPD proves a streaming catalogue maps cleanly onto MPD's directory model; a *subset* is honest, a full byte-for-byte reimplementation is the wrong scope | Indirect but real — phone MPD clients exist today and, with a browse mapping, can navigate a TIDAL library, as mopidy-tidal already demonstrates |
| **d** | **streamboat-native remote protocol** (mDNS + auth + state sync; GUI and future mobile app as controllers) | L | none externally; total internally | designed in from the start — Sendspin's pairing design (§4) is a ready-made answer | Good if it *is* option (b) with discovery bolted on — bad if it is a second protocol | **Essential eventually** |
| **e** | **UPnP/DLNA renderer** | M | some (BubbleUPnP, mConnect, upplay) | UPnP has no auth model worth the name | Poor — SOAP/XML, and hi-res needs a renderer whitelist | none |

**The key structural insight**: (b) and (d) are the same thing. A JSON-over-WebSocket control API
plus mDNS advertisement plus a pairing-based token *is* a Connect-like protocol. Building (b) with
(d) in mind costs almost nothing extra; building (b) and then (d) separately costs double.

**(c) is a genuine multiplier but must be scoped as a subset**, following Nuclear's transport+queue
scope *plus* Mopidy-MPD's browsing mapping (§1) — and not pretending to be a byte-for-byte file
database. Ship it once (b) is stable, and document exactly which commands are supported.

**(a) falls out of (b) for free** if the CLI is a thin client of the daemon (`streamboat play` → one
WebSocket call, or one Unix-socket write — `daemon-architecture.md` §2). tidalt's `play.go` is the
model: try the running instance first, fall back to starting one.

**(e) should be dropped.**

### Full control-surface comparison table

| Surface | Clients gained | Effort | Auth story | Expressiveness | Recommend |
| --- | --- | --- | --- | --- | --- |
| **MPRIS2/D-Bus** (`org.mpris.MediaPlayer2.streamboat`) | playerctl, GNOME/KDE media widgets, hardware media keys, mpDris2-style bridges | XS (`mpris-server` 0.9 + `zbus` 5, as in `ref:sone`) | session bus = local user | transport + metadata, **and** queue (`TrackList`) + stored-playlist activation (`Playlists`) — both optional interfaces `mpris-server` 0.9 already implements; catalogue search/browsing is still out of reach | **v1, Linux** |
| **Unix domain socket / named pipe** (NDJSON, ncspot-style) | the CLI, same-host GUI, tmux/status-bar scripts | XS | filesystem permissions; no port, no token | transport + status; no browsing | **v0** |
| **Windows SMTC / macOS Now Playing** | OS media overlays and keys | XS (`souvlaki` 0.8.3 as in `ref:sone-windows`) | OS-scoped | transport + metadata | **v1, GUI only** |
| **HTTP + WebSocket JSON API** | scripts, Home Assistant, Stream Deck, streamboat's own web remote and future mobile app | M | loopback default + generated token (`Authorization: Bearer` for API clients, a cookie exchange for the browser remote) + opt-in LAN bind | everything | **v1** |
| **Embedded web remote** (served by the daemon) | any phone browser on the LAN | S once the API exists | same token; served over the same listener; not a secure browser context over plain HTTP | everything the API has | **v1.1** |
| **MPD subset on 6600** | MALP, ncmpcpp, Cantata, mpc | M–L | `local_permissions`/Unix-socket needs no password at all; a network password stays unencrypted regardless | queue + transport + catalogue browsing via `lsinfo`/`search` over a virtual tree | **v1.2, explicitly a subset — including browsing** |
| **mDNS `_streamboat._tcp`** | streamboat GUI on another host; future mobile app | S | Sendspin-style pairing token → long-lived, revocable token | discovery only | **v1.2** |
| **Raw PCM pipe/stdout output** | Snapcast, and anything that eats PCM | XS | n/a (local pipe) | n/a — forces a fixed sample format (§2) | **v1** |
| **Sendspin source** | Music Assistant + emerging ESP32/Pi endpoints | L | Noise handshake per spec | full | **watch** |
| **Chromecast / AirPlay sender** | LAN speakers | L each | n/a | full, but quality-capped below streamboat's own daemon (§5) | **defer; resolve re-transmission question first** |
| **UPnP renderer / media server** | BubbleUPnP, upplay | M | none meaningful | full | **skip** |
| **TIDAL Connect target or controller** | TIDAL's own apps | XL + blocked | vendor certificate | full | **never** (`tidal-connect.md`) |
