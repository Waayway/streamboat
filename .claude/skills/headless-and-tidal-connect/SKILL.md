---
name: headless-and-tidal-connect
description: Everything needed to build streamboat's headless/server/CLI mode, the daemon-vs-GUI core split, its own local control protocol (HTTP+WebSocket JSON, MPRIS, an MPD-compatible subset, a Unix-domain-socket surface), TIDAL's one-stream-at-a-time "Pushkin" streaming-privileges websocket, multiroom output (Snapcast, Sendspin), Raspberry Pi / small-device ALSA deployment, headless login and remote-pairing UX, and TIDAL Connect — what it actually is, why streamboat can never build a Connect target or controller, and the exact technical and legal reasons (obfuscated binary, vendor device certificate, TIDAL's anti-circumvention terms). Covers every daemon/headless precedent this project has studied in depth: mopidy-tidal, tidalt, sone, tidal-hifi, upmpdcli, Lyrion/LMS, Music Assistant, go-librespot, spotifyd, librespot, ncspot, psst, tidal-cli. Use this whenever writing or reviewing a daemon/server/headless subcommand or systemd/launchd/windows-service installer; the `streamboat-core`/`streamboat-server` boundary; any control surface (HTTP/REST, WebSocket, MPRIS/D-Bus, the MPD wire protocol, a Unix domain socket or named pipe); mDNS/zeroconf/Avahi discovery or pairing code; Snapcast/Sendspin/Chromecast/AirPlay/UPnP sender or receiver code; ALSA card-name/softvol/test-tone/device logic; a Pi or other small-device audio target; a device-code or headless login flow; token/session/pairing design; or the Pushkin streaming-privileges websocket (`rt/connect`). Also trigger whenever a task or file mentions TIDAL Connect, Connect target, Connect controller, remotePlayback, cloudQueue, MPRIS, MPD, Snapcast, Sendspin, Chromecast, AirPlay, avahi, mDNS, zeroconf, headless, daemon, systemd, Pushkin, streaming privileges, or Raspberry Pi. Do not answer a TIDAL Connect question from general "cast to a device" knowledge — the protocol is closed, obfuscated and gated by a vendor certificate, this has already been dissected in depth, and the conclusion (never build it, in either role) is already reached and load-bearing for the rest of the architecture.
---

# Headless mode, remote control, and TIDAL Connect for streamboat

Source of truth: `docs/research/headless-connect.md` (the full research report, fact-checked and
corrected — read it for narrative depth, full evidence, and the complete source list). This skill
is the load-on-demand distillation: the facts, decisions, and pitfalls an implementer needs while
writing code, without re-reading a ~1,800-line report every time.

`ref:<project>/<path>` throughout this skill and its references points at a shallow, read-only git
clone of a named open-source project kept in the research environment (e.g.
`ref:tidalt/internal/mpris/server.go`) — not part of the streamboat repo. Full project→URL mapping,
licenses, and clone context: `references/sources.md`.

## Owner decisions already made (do not re-litigate these)

- **Platforms now**: desktop Linux + Windows + macOS **and** a headless/server/CLI mode, both now.
  Mobile (Android/iOS) is future scope — the architecture must not preclude it, nothing
  mobile-specific ships now.
- **Stack**: "something simple but beautiful", otherwise open — not decided by this skill.
- **TIDAL API approach**: do what High Tide and Sone do — the unofficial `api.tidal.com` API used
  by `python-tidal`, requiring the user's own paid subscription. streamboat is a player for
  subscribers, not a downloader/ripper. See `tidal-api` skill for the API itself; this skill covers
  what that constraint means for caching (below) and for headless login.
- **TIDAL Connect is permanently out of scope, in both directions.** No open implementation of the
  protocol exists anywhere, as target or controller. The only working Linux target is one closed,
  deliberately-obfuscated ARM binary reusing a single vendor's (iFi Audio) device certificate.
  Reimplementing it means defeating that obfuscation **and** forging or reusing someone else's
  device identity — the same move streamboat has already ruled out. This is not a "later" feature;
  it is a closed door. `references/tidal-connect.md` has the full dissection and the legal
  reasoning; two adjacent positioning questions the owner still needs to answer (not "should we
  build it", which is settled) are in Open decisions below.
- **Recommended architecture**: one shipped binary with subcommands, tidalt-style (Model 1), built
  on top of a `streamboat-core` library split (Model 3) — `streamboat-core` holds
  session/auth/catalogue/queue/player-engine/output-backends/Pushkin with no UI and no server;
  `streamboat-server` adds the control API, MPRIS, mDNS, the MPD listener. The GUI **always** talks
  to the engine through the same command/event types the server exposes, even in-process — that is
  what makes "GUI as a remote for another host's daemon" a config change instead of a rewrite.
  `references/daemon-architecture.md` §1.
- **Every listener defaults to loopback.** LAN exposure is a deliberate, logged, token-protected
  choice, never the default. A URL-path token is for machine/script clients only; a browser-facing
  remote gets a cookie exchange instead. `references/daemon-architecture.md` §3.
- **Never redistribute a vendor's proprietary binary or device certificate, and never decrypt or
  circumvent DRM/security tech** — consistent with the `tidal-api` skill's DRM-refusal rule. This
  covers the Connect binary/certificate specifically, but is the same policy line.

## Facts that cause silent bugs or bad defaults if you get them wrong

| # | Trap | The fix |
| --- | --- | --- |
| 1 | TIDAL enforces **one concurrent stream** over a WebSocket ("Pushkin"). A daemon that ignores it will either get silently paused mid-track or will fight the user's phone for playback. | Implement `POST {apiBase}rt/connect` → `wss://…` in `streamboat-core`. Send `{"type":"USER_ACTION","payload":{"startedAt":…}}` **only** on genuine user-initiated play, never on autoplay/resume. On `PRIVILEGED_SESSION_NOTIFICATION`, pause immediately and surface `clientDisplayName` everywhere (MPRIS, the API, MPD `idle`). Handle `RECONNECT`. `references/daemon-architecture.md` §6. |
| 2 | Connect device identity is **two inputs, not one**: a vendor X.509-style certificate (`--tc-certificate-path`) **and** a network-interface-derived id (`--netif-for-deviceid <iface>`, MAC-based) — the second is easy to miss since GioF71's own wrapper drops it. | Never obtain, ship, or reuse either. There is no documented way to get a certificate outside a commercial partnership. `references/tidal-connect.md` §1-2. |
| 3 | The only open Connect target is capped at **16-bit/44.1 kHz** since TIDAL removed MQA (July 2024) — streamboat's own daemon can already do 24/192. | Don't treat "no Connect" as a compromise — it's a strict quality *upgrade* over the only alternative. State this in the README as a selling point. `references/tidal-connect.md` §2. |
| 4 | MPD's access control is **not** "one plaintext password, no encryption". It has `default_permissions`, `local_permissions` (Unix-socket clients), `host_permissions` (per-IP/CIDR), and any number of `password "secret@permset"` entries. | Bind an MPD-subset listener to a Unix socket or loopback with `local_permissions` and skip the shared-password requirement entirely — only the *network* password path is genuinely weak (no transport encryption, ever). `references/mpd-and-multiroom.md` §1. |
| 5 | MPRIS is **not** transport-only. The full spec has four interfaces — Root, Player, `TrackList`, `Playlists` — and `mpris-server` 0.9 (already used by `ref:sone`) implements all of them. | Expose the queue and stored-playlist activation over MPRIS for free, before any HTTP API exists. Only catalogue **search**/**browsing** genuinely needs a richer surface. `references/daemon-architecture.md` §2. |
| 6 | A raw-PCM pipe (the "feed Snapcast" recommendation) **forces one fixed sample format** — Snapcast pins a `sampleformat` per stream, so every track must be resampled to it. | Design the pipe/Snapcast output as a distinct output *mode*, mutually exclusive with bit-perfect variable-rate ALSA/WASAPI/CoreAudio output — not a free addition to the "24/192 beats Connect" story. `references/mpd-and-multiroom.md` §2. |
| 7 | Two opposite caching postures exist in the precedent set, with different legal weight. mopidy-tidal's proxy stores **plain, playable, decrypted audio chunks** in SQLite. go-librespot's cache stores only the **still-encrypted** file and re-fetches the key on every play. | Copy go-librespot's posture, not mopidy-tidal's, for any persistent cache — a directory of plain playable FLAC reads like a ripper, not a player, which is exactly the line the owner drew. `references/headless-daemon-precedents.md` §1. |
| 8 | A control-API token carried in the **URL path** (sone's pattern) is fine for a script/MCP client and unsuitable for a browser: it lands in history, the address bar, `Referer` headers, and proxy logs. | Use `Authorization: Bearer` for API/script clients; exchange the token once for an `HttpOnly`, `SameSite` cookie for the web remote. `references/daemon-architecture.md` §3. |
| 9 | ALSA card **indices shift across reboots**, and "resolve the name and always open `default`" is not literally what the reference wrapper does — it opens `tidal-softvol`, `$CREATED_ASOUND_CARD_NAME`, `custom`, or `default` depending on config. A `Master` mixer control may already exist. | Resolve by card **name** from `/proc/asound/cards` at every start; check for an existing `Master` control before creating a softvol, and name a new one `SoftMaster` (not `Master`) if one already exists, warning that the slider now controls shared hardware volume. `references/raspberry-pi-deployment.md` §1. |
| 10 | A Raspberry Pi has **no real-time clock**. A wrong boot-time clock breaks TLS to TIDAL's CDN/API, token-expiry math, Pushkin's `startedAt`, and Snapcast's sub-millisecond sync. TIDAL's own official auth SDK is visibly TrueTime-gated (it logs "TrueTime is not yet synchronized" until NTP converges). | Add `After=time-sync.target` (`Wants=`) to the daemon's unit; retry-with-backoff on early-uptime TLS failure instead of a hard auth error; surface clock-sync state in `/health`. `references/raspberry-pi-deployment.md` §3. |
| 11 | The `_tidalconnect._tcp` TXT record keys and port are undocumented in every source read for this project — but that is a 15-minute experiment, not a permanent unknown. | Run `avahi-browse -r -t _tidalconnect._tcp` (note the `-r` resolve flag) against any Connect-capable device already on a LAN, or `dns-sd -B`/`-L` on macOS. `references/tidal-connect.md` §3. |
| 12 | python-tidal's refresh token is **never rotated** on `token_refresh()` — only `access_token`/`expiry_time`/`token_type` change. Two processes (a GUI and a daemon) holding the same refresh token do not invalidate each other. | Decide explicitly whether the GUI and daemon share one token file (with an advisory lock) or log in separately with distinct `client_id`s — don't leave it to accident. `references/daemon-architecture.md` §5. |
| 13 | Google Cast's own FLAC ceiling (≤96 kHz/24-bit; 24-bit at 176.4/192 kHz can hang a Chromecast Audio) settles the "build a Cast sender" question on quality grounds alone, independent of the URL/DRM argument already in the report. | Don't reopen "should we build a Chromecast sender" without this fact — it caps streamboat *below* its own headless daemon, the same problem as Connect. `references/mpd-and-multiroom.md` §5. |
| 14 | mopidy-tidal's headless-login "login hack" QR code is rendered by a **third-party remote service** (`api.qrserver.com`), which fails offline/firewalled — exactly the deployment the feature exists for. | Generate any login QR locally (terminal or in-band), following `ref:tidalt`'s `qrterminal/v3` approach, not mopidy-tidal's hotlink. `references/headless-daemon-precedents.md` §1. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| TIDAL Connect protocol dissection (binary, flags, licence-folder evidence, discovery, quality ceiling), the controller-side Redux data model, feasibility/legal table for all three Connect options, and the two remaining owner-facing positioning questions | `references/tidal-connect.md` |
| Per-project deep dives: mopidy-tidal, tidalt, sone, tidal-hifi, upmpdcli, Lyrion/LMS, Music Assistant, tidal-cli, and the librespot family (librespot/go-librespot/spotifyd/ncspot/psst) | `references/headless-daemon-precedents.md` |
| MPD protocol facts (confirmed against the primary spec), an honest MPD-subset design including catalogue browsing, Snapcast (ports, sources, mDNS, stream-plugin protocol), Sendspin (pairing, framing, versioning), Chromecast/AirPlay sender+receiver analysis, UPnP/DLNA, and the full design-options/comparison tables | `references/mpd-and-multiroom.md` |
| `streamboat-core`/`streamboat-server` split and models compared, per-platform process management (systemd/launchd/windows-service, config locations, logging, updates), the control-protocol event schema and versioning design, every local IPC surface (MPRIS, Unix socket, HTTP+WebSocket), the security/token model, headless login and remote pairing UX, token/session sharing between GUI and daemon | `references/daemon-architecture.md` |
| Raspberry Pi/ALSA deployment specifics, build/distribution for small devices, time sync, mDNS coexistence and naming, diagnostics/testability, daemon lifecycle (queue persistence, prefetch, idle-device handling) | `references/raspberry-pi-deployment.md` |
| Project→GitHub URL mapping, licenses, and which web sources are blocked from this research environment | `references/sources.md` |

## Quick orientation — the recommended staged path

Build the daemon protocol once, expose it through as many pre-existing client ecosystems as cheaply
as possible, and never touch TIDAL Connect. In order:

1. **Core split**: `streamboat-core` (auth, catalogue, queue, player engine, output backends
   including a **pipe** backend, Pushkin client). No UI, no server.
2. **One binary, two modes**: `streamboat`, `streamboat daemon`, `streamboat play <url>`,
   `streamboat service install`; single-instance detection, second invocation becomes a client.
3. **OS media integration**: MPRIS2 on Linux (now including `TrackList`/`Playlists`), SMTC/Now
   Playing on Windows/macOS.
4. **Snapcast-ready output**: `--output pipe:<path>`/`stdout`, understood as a fixed-format mode.
5. **Headless login**: device-code flow, local QR generation, keyring-with-encrypted-file fallback.
6. **Control API**: HTTP + WebSocket JSON on `127.0.0.1` by default, Bearer token for API clients,
   a typed event schema with revision numbers (not ad hoc), `GET /health` carrying
   `{version, protocol_version, capabilities[]}`.
7. **Web remote**: a static page over the same listener — designed around the fact that plain
   `http://` on a LAN IP is not a secure browser context.
8. **Discovery + pairing**: `_streamboat._tcp` via `mdns-sd`, with collision/rename handling; a
   Sendspin-style pairing token, not a bare shared secret.
9. **MPD subset**: loopback/Unix-socket bound, covering transport + queue + `idle` **and** catalogue
   browsing via `lsinfo`/`search` over a virtual tree (mopidy-tidal's precedent — don't drop
   browsing to save scope).
10. **Diagnostics**: a `streamboat doctor` subcommand covering the ALSA/mDNS/clock/device checks
    every Connect-wrapper precedent already had to learn the hard way.
11. **Optional, demand-driven**: a Sendspin source, a Snapcast stream-plugin (metadata+control, not
    just a bare pipe), Cast/AirPlay senders only after the re-transmission question is settled, an
    opaque/encrypted seek-buffer cache.
12. **Never**: TIDAL Connect target or controller, a UPnP renderer, a Chromecast receiver.

Full comparison table of every control surface (effort, auth story, expressiveness) is in
`references/mpd-and-multiroom.md` §6; the complete reusable-artifacts list (what to copy from which
file, with corrections applied) is in `references/headless-daemon-precedents.md` §11.

## Open decisions (feed these into any decision tree or spec-writing task)

Only the owner (thijs) can resolve these — do not assume an answer when writing code or docs:

1. **Is the desktop GUI always a client of a daemon** (MPD/Roon/Music-Assistant model), or does it
   embed the core in-process and only optionally expose a daemon (the recommended tidalt-style
   model)? Everything about IPC, startup, packaging and error handling follows from this.
   `references/daemon-architecture.md` §1.
2. **Does streamboat ever re-transmit audio on the LAN** (a Snapcast pipe, an AirPlay/Cast sender,
   an HTTP stream endpoint)? All mean TIDAL audio leaving the machine that authenticated — legally
   untested against TIDAL's consumer terms, with the pipe/Snapcast case the most defensible and an
   open HTTP endpoint the least. `references/mpd-and-multiroom.md` §5.
3. **How far to go with MPD compatibility** — a transport+queue subset (weeks) or a fuller emulation
   including a browsable catalogue (months)? Note that browsing is now known to be cheap (mopidy-mpd
   precedent), which should shift this decision. `references/mpd-and-multiroom.md` §1.
4. **Bit-perfect vs. gapless when the sample rate changes** — pick a default rather than silently
   resampling. Directly linked to decision 2, since the Snapcast pipe forces a fixed format anyway.
5. **Is a Windows/macOS background service in scope at all**, or is "headless" a Linux/server story
   with GUI-only behaviour elsewhere? A one-hour spike (run a WASAPI render loop from a Windows
   service vs. a scheduled task) should answer the technical half before this is decided.
   `references/daemon-architecture.md` §1.
6. **Name and shape of the native remote/discovery protocol** — align with Sendspin (pairing design,
   framing, clock sync are all fully specified there) or invent one. `references/mpd-and-multiroom.md`
   §4.
7. **Does streamboat ever approach TIDAL for legitimate Connect partner/SDK status?** Cost, terms,
   and whether an open-source project can even apply are *unknown*, not *impossible* —
   `developer.tidal.com` was unreachable during this research. Decide whether to ask, or to state
   publicly that streamboat will never be a Connect device. `references/tidal-connect.md` §5.
8. **Does streamboat's documentation point users at the GioF71/TonyTromp Connect Docker container**
   as a companion on the same Pi? That container ships a binary reusing iFi's device certificate;
   recommending it is a positioning choice with the same flavour as streamboat's own "we do not
   ship vendor credentials" stance. `references/tidal-connect.md` §5.
9. **Additive-and-tolerant (MPD-style `commands`/`protocol enable`) vs. exact-match (Sendspin-style)
   versioning** for streamboat's own control API. `references/daemon-architecture.md` §4.

## Unverified — do not present these as settled fact in specs or code comments

- **TIDAL's actual partner terms for Connect certification/licensing.** `developer.tidal.com` and
  `tidal.com/connect` are blocked from this research environment; every partner/device-count claim
  is second-hand trade press. `references/tidal-connect.md` §1.
- **Whether the Connect control channel really is JSON-over-WebSocket** resembling the Cast media
  namespace — inferred from `websocketpp` in the binary's licence folder plus Cast-shaped client
  types, with no wire capture. `references/tidal-connect.md` §4.
- **`cloudQueue`/`cloudConnect` endpoints** — present in the desktop client's action set, absent
  from every official SDK and every open client. `references/tidal-connect.md` §4.
- **The exact MPD command list Nuclear implements** — its docs site is blocked from this
  environment; port-fallback behaviour (6600→6601-6609) came from a search-index summary and needs
  re-verification. (The core MPD *protocol* facts are separately confirmed against the primary spec
  and do not need re-checking.) `references/mpd-and-multiroom.md` §1.
- **The Sendspin spec's licence**, and its adoption trajectory outside Music Assistant.
  `references/mpd-and-multiroom.md` §4.
- **Whether a Windows service can open WASAPI output in session 0.** Assume not until the named
  spike is actually run. `references/daemon-architecture.md` §1.
- **Measured CPU cost of 24/192 FLAC decode on Pi 3/4/5.** No benchmark found anywhere in the
  reference set; don't quote a number. `references/raspberry-pi-deployment.md` §1.
- **Current hi-res status of `lms-plugin-tidal`.** Genuinely mixed (some issues closed, some open,
  one suggesting hi-res has since landed) — don't state it as simply broken or simply fixed.
  `references/headless-daemon-precedents.md` §6.
- **Whether TIDAL has ever taken action against Connect-binary redistributors.** No takedown found,
  but absence of evidence only. `references/tidal-connect.md` §5.
