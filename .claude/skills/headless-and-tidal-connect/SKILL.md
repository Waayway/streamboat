---
name: headless-and-tidal-connect
description: Everything needed to build streamboat's headless/server/CLI mode, the daemon-vs-GUI core split, its own local control protocol (HTTP+WebSocket JSON, MPRIS, an MPD-compatible subset, a Unix-domain-socket surface), TIDAL's one-stream-at-a-time "Pushkin" streaming-privileges websocket, multiroom output (Snapcast, Sendspin), Raspberry Pi / small-device ALSA deployment, headless login and remote-pairing UX, and TIDAL Connect — what it actually is, why streamboat can never build a Connect target (and why a Connect controller is out of scope rather than categorically ruled out), and the exact technical and legal reasons (obfuscated binary, vendor device certificate, TIDAL's anti-circumvention terms). Covers every daemon/headless precedent this project has studied in depth — mopidy-tidal, tidalt, Sone, tidal-hifi, upmpdcli, Lyrion/LMS, Music Assistant, go-librespot, spotifyd, librespot, ncspot, psst, tidal-cli. Use this whenever writing or reviewing a daemon/server/headless subcommand or systemd/launchd/windows-service installer; the `streamboat-core`/`streamboat-server` boundary; any control surface (HTTP/REST, WebSocket, MPRIS/D-Bus, the MPD wire protocol, a Unix domain socket or named pipe); mDNS/zeroconf/Avahi discovery or pairing code; Snapcast/Sendspin/Chromecast/AirPlay/UPnP sender or receiver code; ALSA card-name/softvol/test-tone/device logic; a Pi or other small-device audio target; a device-code or headless login flow; token/session/pairing design; or the Pushkin streaming-privileges websocket (`rt/connect`). Also trigger whenever a task or file mentions TIDAL Connect, Connect target, Connect controller, remotePlayback, cloudQueue, MPRIS, MPD, Snapcast, Sendspin, Chromecast, AirPlay, avahi, mDNS, zeroconf, headless, daemon, systemd, Pushkin, streaming privileges, or Raspberry Pi. Do not answer a TIDAL Connect question from general "cast to a device" knowledge — the protocol is closed, obfuscated and gated by a vendor certificate, this has already been dissected in depth, and the target-side conclusion (never build it) is load-bearing for the rest of the architecture; the controller side is out of scope, not categorically ruled out — see the verdict in the skill body.
---

# Headless mode, remote control, and TIDAL Connect for streamboat

Source of truth: `docs/research/headless-connect.md` (the full research report, fact-checked and
corrected — read it for narrative depth, full evidence, and the complete source list). This skill
is the load-on-demand distillation: the facts, decisions, and pitfalls an implementer needs while
writing code, without re-reading a ~2,700-line, three-times-fact-checked report every time.

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
- **Never redistribute a vendor's proprietary binary or device certificate, and never decrypt or
  circumvent DRM/security tech** — consistent with the `tidal-api` skill's DRM-refusal rule. This
  covers the Connect binary/certificate specifically, but is the same policy line.

## Research conclusions — strong recommendations, owner has not ratified

The project context records no owner decision on TIDAL Connect, daemon architecture, or listener
defaults. These are this research's own conclusions, load-bearing for how the rest of this skill is
written, but the owner can still overrule any of them:

- **TIDAL Connect verdict, one line for both roles**: **target — never** (the only working Linux
  target is one closed, deliberately-obfuscated ARM binary reusing a single vendor's, iFi Audio's,
  device certificate; reimplementing it means defeating that obfuscation **and** forging or reusing
  someone else's device identity). **controller — out of scope, not "never"**: no open
  implementation of the controller-side wire protocol exists anywhere and nobody in the reference
  set has published one, but the controller side is not certificate-gated the way the target side
  is — its blocker is an undocumented protocol with zero precedent, not a forgeable device identity.
  Revisit the controller only if someone publishes a wire capture of the protocol; the target
  verdict does not change under any circumstance covered by this research. `references/tidal-connect.md`
  has the full dissection and the legal reasoning; two adjacent positioning questions are in Open
  decisions below. This verdict is also the one `tidal-api`, `tidal-client-features`, and
  `tidal-oss-landscape` should match — fix those skills to agree with this line, not the reverse.
- **Recommended architecture**: one shipped binary with subcommands, tidalt-style (Model 1), built
  on top of a `streamboat-core` library split (Model 3) — `streamboat-core` holds
  session/auth/catalogue/queue/player-engine/output-backends/Pushkin with no UI and no server;
  `streamboat-server` adds the control API, MPRIS, mDNS, the MPD listener. The GUI **always** talks
  to the engine through the same command/event types the server exposes, even in-process — that is
  what makes "GUI as a remote for another host's daemon" a config change instead of a rewrite.
  `references/daemon-architecture.md` §1.
- **Recommended default: every listener binds to loopback.** LAN exposure would be a deliberate,
  logged, token-protected choice, not the default. A URL-path token is for machine/script clients
  only; a browser-facing remote would get a cookie exchange instead. Loopback binding alone does not
  stop DNS rebinding — pair it with Host-header allowlisting if the owner adopts this.
  `references/daemon-architecture.md` §3.
- **One TIDAL account = one concurrent stream, enforced by Pushkin — this constrains the multiroom
  architecture, not just a login detail.** Multiroom cannot be "one streamboat daemon per room": it
  must be one daemon holding the single stream and fanning decoded PCM out to many endpoints
  (Snapcast today, a Sendspin source later). A multi-subscriber household would run multiple
  independent daemon *instances* (one per account), never one daemon switching accounts per
  connection. `references/daemon-architecture.md` §6; `references/mpd-and-multiroom.md` §2.
- **If "one binary with subcommands" is adopted, it should describe the command surface, not the
  build.** Compile it as two Cargo artifacts from one workspace: a GUI binary that links the
  webview, and a `streamboatd` that must not — enforced by a CI job building the daemon crate alone
  in a GTK/WebKit-free container. See `docs/research/tech-stack.md:134` ("a `daemon` binary … a
  `cli` client, and a Tauri desktop shell") — the `tech-stack-evaluation` skill owns crate/binary
  naming; this skill only flags that a headless implementer must not accidentally pull in
  Tauri/WebKitGTK. `references/daemon-architecture.md` §1.

## Traps — silently wrong behaviour if you get these wrong

Compressed index only — full context, code citations, and precedent for every row live in the
reference file its pointer names.

| # | Trap | The fix |
| --- | --- | --- |
| 1 | TIDAL enforces **one concurrent stream** over a WebSocket ("Pushkin"); ignore it and the daemon gets silently paused mid-track or fights the user's phone. | `POST {apiBase}rt/connect` → `wss://…` in `streamboat-core`. Send `USER_ACTION` only on genuine user-initiated play. On `PRIVILEGED_SESSION_NOTIFICATION`, pause immediately and surface `clientDisplayName` everywhere. `references/daemon-architecture.md` §6. |
| 2 | Connect device identity is **two inputs**: a vendor certificate **and** a MAC-derived interface id — the second is easy to miss. | Never obtain, ship, or reuse either; no documented way to get a certificate outside a commercial partnership. `references/tidal-connect.md` §1-2. |
| 3 | The only open Connect target historically supported 24/48 hi-res plus MQA (in-app unfolding to 24/88-24/96) — say **"effectively LOSSLESS-only [16-bit/44.1 kHz] since MQA's removal (July 2024)"**, not "capped at LOSSLESS/16-44.1 since July 2024" (a deliberate-sounding cap that isn't what happened: TIDAL simply stopped serving anything above 16/44 as HI_RES content, per the binary's own README) — streamboat's daemon can already do 24/192 either way. | State "no Connect" as a quality *upgrade*, not a compromise, in the README; use the precise historical-capability-vs-effective-ceiling phrasing, not a flat "capped at X" claim. `references/tidal-connect.md` §2. |
| 4 | MPD access control is **not** "one plaintext password" — `default_permissions`, `local_permissions`, `host_permissions`, `password "secret@permset"`. | Bind an MPD-subset listener to a Unix socket or loopback with `local_permissions`, skipping the shared password — only the network path is weak. `references/mpd-and-multiroom.md` §1. |
| 5 | MPRIS is **not** transport-only — four interfaces (Root, Player, `TrackList`, `Playlists`); `mpris-server` 0.9 (used by `ref:sone`) implements all of them. | Expose the queue and playlist activation over MPRIS for free; only catalogue search/browsing needs a richer surface. `references/daemon-architecture.md` §2. |
| 6 | A raw-PCM pipe to Snapcast **forces one fixed sample format** — every track gets resampled to it. | Design pipe/Snapcast output as a distinct mode, mutually exclusive with bit-perfect variable-rate output. `references/mpd-and-multiroom.md` §2. |
| 7 | mopidy-tidal caches **plain, playable, decrypted** audio; go-librespot caches only the **still-encrypted** file and re-fetches the key per play — different legal weight. | Copy go-librespot's posture — a directory of plain playable FLAC reads like a ripper, not a player. `references/headless-daemon-precedents.md` §1. |
| 8 | A control-API token in the **URL path** (Sone's pattern) suits a script client, not a browser — history, `Referer`, proxy logs. | `Authorization: Bearer` for API/script clients; exchange once for an `HttpOnly`, `SameSite` cookie for the web remote. `references/daemon-architecture.md` §3. |
| 9 | ALSA card **indices shift across reboots**; the reference wrapper doesn't just open `default` — `tidal-softvol`, `$CREATED_ASOUND_CARD_NAME`, `custom`, or `default` depending on config. | Resolve by card **name** from `/proc/asound/cards` at every start; check for an existing `Master` control, naming a new one `SoftMaster` if one exists. `references/raspberry-pi-deployment.md` §1. |
| 10 | A Raspberry Pi has **no real-time clock** — a wrong boot-time clock breaks TLS, token-expiry math, Pushkin's `startedAt`, and Snapcast sync. | `After=time-sync.target` on the daemon unit; retry-with-backoff on early-uptime TLS failure; surface clock-sync state in `/health`. `references/raspberry-pi-deployment.md` §3. |
| 11 | The `_tidalconnect._tcp` TXT keys and port are undocumented — a 15-minute experiment, not a permanent unknown. | `avahi-browse -r -t _tidalconnect._tcp` against a Connect-capable device, or `dns-sd -B`/`-L` on macOS. `references/tidal-connect.md` §3. |
| 12 | python-tidal's refresh token is **never rotated** — only `access_token`/`expiry_time` change. Two processes holding it don't invalidate each other. | Decide explicitly: GUI and daemon share one token file (advisory lock), or separate logins with distinct `client_id`s. `references/daemon-architecture.md` §5. |
| 13 | Google Cast's FLAC ceiling (≤96 kHz/24-bit; 176.4/192 kHz can hang a Chromecast Audio) settles "build a Cast sender" on quality grounds alone. | Don't reopen the Cast-sender question without this — it caps streamboat below its own headless daemon. `references/mpd-and-multiroom.md` §5. |
| 14 | mopidy-tidal's login QR is rendered by a **third-party remote service** (`api.qrserver.com`) — fails offline/firewalled, exactly the target deployment. | Generate any login QR locally, following `ref:tidalt`'s `qrterminal/v3` approach. `references/headless-daemon-precedents.md` §1. |
| 15 | TIDAL has an **officially documented server-side play queue** — `/playQueues` on `openapi.tidal.com/v2` — earlier research wrongly called this undocumented. | Don't build a custom cloud-queue mechanism without checking this; it's behind PKCE `r_usr`/`w_usr` scopes, a different auth stack. Only the `cloudConnect` wire *transport* is still undocumented. `references/tidal-connect.md` §4; `references/daemon-architecture.md` §4. |
| 16 | Pushkin's reference client reconnects with **no backoff, no jitter, no cap** — copying it verbatim hot-loops authenticated calls while offline. | Exponential backoff with jitter and a ceiling; reconnect explicitly on token refresh; never auto-retry `USER_ACTION` after a displacement. `references/daemon-architecture.md` §6. |
| 17 | MPRIS and `ReserveDevice1` both need a D-Bus **session** bus, which a true headless deployment (system unit, no login) lacks — both silently fail, not error. | `systemd --user` + `loginctl enable-linger` gets a session bus even headless; otherwise a system-bus policy file, or make registration non-fatal. `references/daemon-architecture.md` §1. |
| 18 | tidal-hifi — the naming precedent — ships its control API **enabled by default, `127.0.0.1:47836`, no authentication**. | streamboat's control API must be disabled by default, or token-required from first boot. `references/daemon-architecture.md` §3. |
| 19 | ncspot's same-host IPC socket is **Unix-only** — no Windows named-pipe variant exists in the reference implementation. | Cite ncspot for the Unix-socket half only; the Windows named-pipe half (via `interprocess`) needs its own testing. `references/daemon-architecture.md` §2. |
| 20 | An MPD-subset listener bound loopback-only can't deliver "phone clients work day one" — a phone needs LAN reach. | Pick one default deliberately: loopback/`local_permissions`, or LAN-bound `host_permissions` (unencrypted). Don't present both as default. `references/mpd-and-multiroom.md` §1. |
| 21 | macOS 15+ requires `NSLocalNetworkUsageDescription` for mDNS/LAN code, or discovery silently returns nothing. | Wire the key into the Tauri macOS `Info.plist`; render an explicit permission-denied UI state; iOS needs the key plus `NSBonjourServices`. `references/daemon-architecture.md` §1. |
| 22 | A Windows service (session 0) has no default audio session, and **no precedent in the reference set** shows one opening an audio device — even snapclient ships no Windows service mode. | Treat headless as a Linux/server story; ship Windows/macOS a GUI with a background/tray mode, not a true service, unless the WASAPI spike proves otherwise. `references/daemon-architecture.md` §1. |
| 23 | A `systemd --user` unit copied from a desktop precedent is `graphical-session.target`-bound with no `RuntimeDirectory`/`StateDirectory` — won't start headless, and `PrivateTmp=yes` breaks a classic `/tmp/snapfifo` pipe. | `RuntimeDirectory=streamboat`, `StateDirectory=streamboat`, `ConfigurationDirectory=streamboat`, `SupplementaryGroups=audio`, `After=network-online.target time-sync.target`; document the `PrivateTmp`/pipe interaction. `references/daemon-architecture.md` §1. |
| 24 | Bit-perfect output means no software attenuation — but MPRIS `Volume`, MPD `setvol`, and `PUT /player/volume` still expose a slider. **`volume: -1` is deprecated in the current MPD protocol spec.** | Omit `volume` from MPD `status` when no mixer exists; expose `volume: hardware\|software\|none` in `GET /health` and derive every surface from it. `references/daemon-architecture.md` §3; `references/mpd-and-multiroom.md` §1. |
| 25 | `streamboat play <url>` does nothing from a browser until the OS URL scheme is registered — separate from deep-link forwarding code. **Register `streamboat://` by default, not `tidal://`** — `tidal://` is claimed by the official TIDAL desktop app itself plus Strawberry, Sone, and High Tide; OS handler registration is last-writer-wins, so claiming it silently steals it from whichever app the user installed last. See `tidal-api/references/auth.md` §13 for the full rule (this skill's registration mechanics are still correct, only the scheme choice changes). | Linux: `.desktop` + `xdg-mime default`. Windows: `HKCU\Software\Classes\streamboat`. macOS: `CFBundleURLTypes`. **Accept** `tidal://` links by parsing them (grammar in `tidal-api/references/auth.md` §13), but make **claiming** `tidal://` an explicit opt-in setting, off by default — the same treatment as claiming `https://tidal.com/...`. `references/daemon-architecture.md` §2. |
| 26 | `tidal-cli`'s manifest-format list is not a fixed 4-element array — a quality-dependent cascade (`LOW`→1 value … `HI_RES`→4). | Model quality→format mapping as an explicit per-tier list, not a constant array. `references/headless-daemon-precedents.md` §8. |
| 27 | Pi Wi-Fi defaults to `power_save` on, a documented dropout cause; USB DAC dropouts are a known kernel/USB-scheduling issue class, not necessarily a streamboat bug. | `streamboat doctor` checks `iw dev wlan0 get power_save`; document `raspberrypi/linux#2215`; larger ALSA period size on repeated USB dropouts. `references/raspberry-pi-deployment.md` §1. |
| 28 | `/playQueues` (trap #15) caps every write at **20 items per call**, ordered by cursor not array index — a 500-track queue is 25 calls, not one. | Model writes against the real limit from day one; require `meta.mode` on every add. `references/tidal-connect.md` §4. |
| 30 | Sendspin isn't the only screen-free pairing precedent — librespot's **zeroconf** pairing is MIT-licensed and decade-shipped; Sendspin's spec currently has **no stated licence**. | Treat Sendspin's missing licence as a blocker on copying it wholesale; librespot's zeroconf is a safer fallback today. `references/daemon-architecture.md` §5; `references/mpd-and-multiroom.md` §4. |
| 31 | MPD advertises itself over mDNS by default (`_mpd._tcp`) — "MALP works day one" silently depends on that. | Advertise `_mpd._tcp` alongside `_streamboat._tcp` when the MPD listener is LAN-bound; not when loopback/Unix-socket-only. `references/mpd-and-multiroom.md` §1. |
| 33 | Catalogue **browsing** has no cache or rate-limit story — an MPD client renders one screen as dozens of upstream calls, hitting TIDAL's rate limiter. | A persistent, size-capped metadata `LruCache`, bounded catalogue-fan-out concurrency (4 workers in precedent), degrade-on-429 policy. `references/headless-daemon-precedents.md` §1. |
| 34 | Four ALSA lifecycle invariants are easy to miss, one a documented SIGSEGV: a reused PCM handle after pause dereferences NULL; `EBUSY` ≠ format refusal; `plughw:` fallback must be memoised per device; an unrecoverable output error must not signal track completion. | Copy `ref:tidalt`'s reopen condition (`formatChanged \|\| deviceClosed`), error-type distinction, fallback memoisation, `aborted` flag — plus bounded reopen-with-backoff for device removal. `references/raspberry-pi-deployment.md` §1. |
| 35 | `albumart`/`readpicture` are specified by command name only — reference MPD sources both from the **filesystem**, which a TIDAL stream has none of; the greeting version is a protocol promise, not cosmetic. | Serve `albumart`/`readpicture` from the metadata image cache (trap #33); announce only the MPD protocol version streamboat implements. `references/mpd-and-multiroom.md` §1. |
| 38 | `systemctl restart` and unattended updates stop the daemon mid-track — nothing specifies the `SIGTERM` handler. Default: SIGKILL with a truncated ALSA buffer, or a hung shutdown. | On `SIGTERM`: `snd_pcm_drop` (never `drain`), release `ReserveDevice1`, flush queue state, close Pushkin cleanly, exit. Set `KillMode=`/`TimeoutStopSec=`; agree `Restart=on-failure` with item 36. `references/daemon-architecture.md` §1. |
| 39 | Credential redaction is unaddressed, though every precedent logs at level 3+ to journald (readable by any `systemd-journal`-group user), and this project's own guidance recommends logging the control-API URL. | Never log the control-API token, pairing token, `Authorization` headers, refresh/access tokens, the Pushkin URL, or signed CDN URLs — log a stable hash instead. Redact `doctor` output and debug bundles by construction. `references/daemon-architecture.md` §1. |
| 42 | TIDAL's Connect target uses a ~120s mDNS TTL — that's the A/AAAA/SRV host-record figure under RFC 6762. PTR/TXT service-instance records (what `_streamboat._tcp` is) default to **75 minutes**; copying 120s understates staleness 30×. | Budget `_streamboat._tcp`'s TTL at 75 min (PTR/TXT default) and send an RFC 6762 goodbye packet (TTL=0) on clean shutdown. `references/tidal-connect.md` §3. |

## Unspecified — decide before writing code

Open design decisions this project has not made, not bugs — pick these deliberately or two
implementers will diverge.

| # | Gap | Decide |
| --- | --- | --- |
| 29 | The recommended transport (REST + a separate WS event stream) is the opposite of every server-shaped precedent here — Mopidy, Snapcast's control port, and the Snapcast stream-plugin protocol all use **JSON-RPC 2.0**; gRPC is never evaluated despite the brief asking for it. | Pick one framing — JSON-RPC 2.0 with ids over the same WebSocket that carries events — plus a thin REST facade for curl/Stream Deck users; state gRPC rejected. `references/daemon-architecture.md` §2. |
| 32 | No URI/identifier grammar is specified for tracks, albums, playlists or browse nodes — the most load-bearing naming decision in this daemon; two implementers will invent incompatible ones. | Adopt mopidy-tidal's composite scheme: `tidal:track:{artistId}:{albumId}:{trackId}`, single-id containers, a 12-entry browse root. Write it down before users have stored playlists. `references/mpd-and-multiroom.md` §1. |
| 36 | Queue persistence across restart collides with MPD song-id instability: ids "stay the same" while queued but are "not preserved across restarts" — unresolved, this breaks `plchanges` on every reboot. | Persist streamboat's own `uid`s and remap to fresh MPD `songid`s each start (bumping queue version), or persist MPD-facing ids and diverge — state which, and reset the revision number with a per-boot session id. `references/raspberry-pi-deployment.md` §6. |
| 37 | The control API has one all-or-nothing token — no scopes, no revocation — while the MPD listener on the same box has a four-level access model (trap #4). Two incompatible authz designs is the default outcome. | Token scopes (`read` vs. `control`, `admin` separable), how revocation affects an open WebSocket, whether MPD's permissions derive from the same policy object. `references/daemon-architecture.md` §3. |
| 40 | The Rust crates recommended by name have very different maintenance cadences, never stated: `souvlaki` 0.8.3 and `mpd_protocol` 1.0.3 are both over a year old as of this check; `mdns-sd`/`windows-service` are current. | Record a freshness check next to each crate recommendation; re-verify `souvlaki`/`mpd_protocol` specifically. `references/mpd-and-multiroom.md` §1. |
| 41 | The CLI is specified only as a same-host client — no `--host`/remote-daemon flag, no `streamboat daemons` discovery listing. | Add `streamboat --daemon <host\|unix:<path>\|auto>`, `streamboat daemons` (mDNS browse), `streamboat pair <name>`; extend `--json`/exit-code conventions to them. `references/headless-daemon-precedents.md` §8. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| TIDAL Connect protocol dissection (binary, flags, licence-folder evidence, discovery, quality ceiling), the controller-side Redux data model, feasibility/legal table for all three Connect options, and the two remaining owner-facing positioning questions | `references/tidal-connect.md` |
| Per-project deep dives: mopidy-tidal, tidalt, Sone, tidal-hifi, upmpdcli, Lyrion/LMS, Music Assistant, tidal-cli, and the librespot family (librespot/go-librespot/spotifyd/ncspot/psst) | `references/headless-daemon-precedents.md` |
| MPD protocol facts (confirmed against the primary spec), an honest MPD-subset design including catalogue browsing, Snapcast (ports, sources, mDNS, stream-plugin protocol), Sendspin (pairing, framing, versioning), Chromecast/AirPlay sender+receiver analysis, UPnP/DLNA, and the full design-options/comparison tables | `references/mpd-and-multiroom.md` |
| `streamboat-core`/`streamboat-server` split and models compared, per-platform process management (systemd/launchd/windows-service, config locations, logging, updates), the control-protocol event schema and versioning design, every local IPC surface (MPRIS, Unix socket, HTTP+WebSocket), the security/token model, headless login and remote pairing UX, token/session sharing between GUI and daemon | `references/daemon-architecture.md` |
| Raspberry Pi/ALSA deployment specifics, build/distribution for small devices, time sync, mDNS coexistence and naming, diagnostics/testability, daemon lifecycle (queue persistence, prefetch, idle-device handling) | `references/raspberry-pi-deployment.md` |
| Project→GitHub URL mapping, licenses, and which web sources are blocked from this research environment | `references/sources.md` |

## Quick orientation — the recommended staged path

Build the daemon protocol once, expose it through as many pre-existing client ecosystems as cheaply
as possible, and never touch TIDAL Connect. In order:

1. **Core split**: `streamboat-core` (auth, catalogue, queue, player engine, output backends
   including a **pipe** backend, Pushkin client). No UI, no server. Evaluate `ref:tidalrs`
   (a maintained Rust TIDAL client crate with device-code OAuth2 and an
   `on_authz_refresh_callback` hook) as a reference for the token-refresh loop before writing one
   from scratch. The core/UI split is unanimous across every precedent studied, including outside
   Rust/Go: TidalSwift splits into `TidalSwiftLib` (session, catalogue, playback) and a thin
   `TidalSwift` SwiftUI app — the only Swift-side precedent here, worth a look given Swift/SwiftUI
   is the realistic toolkit for the future iOS build. `references/daemon-architecture.md` §1. Compile
   as two Cargo build artifacts (GUI-linked, and a webview-free `streamboatd`), not one — see the
   build-artifact note in Owner decisions above.
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
12. **Never**: a TIDAL Connect target, a UPnP renderer, a Chromecast receiver. **Out of scope, not
    "never"**: a TIDAL Connect controller — no protocol and no precedent exist to build against
    today, but unlike the target it is not certificate-gated; revisit only if someone publishes a
    wire capture.

Full comparison table of every control surface (effort, auth story, expressiveness) is in
`references/mpd-and-multiroom.md` §6; the complete reusable-artifacts list (what to copy from which
file, with corrections applied) is in `references/headless-daemon-precedents.md` §11.

**No step above carries a size estimate, and step 9 (the MPD subset) is by far the largest — treat it
as the one stage that can be deferred indefinitely without blocking anything else.** It is "a
hand-written parser plus a state machine, on the order of a few thousand lines" with no drop-in
server library in Rust or Go (`references/mpd-and-multiroom.md` §1) — larger than the entire control
API in step 6. By contrast the pipe output (step 4) is "tens of lines" and MPRIS (step 3) is "~a day
of work". Split step 9 out of any critical-path plan explicitly; nothing else in this architecture
depends on it.

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
   framing, clock sync are all fully specified there) or invent one. **Re-check before committing**:
   Sendspin's spec currently states no licence at all, which is a blocker on copying its design
   "wholesale," not a caveat — if unresolved, librespot's MIT-licensed, already-shipped zeroconf
   pairing protocol (`references/daemon-architecture.md` §5) is the fallback.
   `references/mpd-and-multiroom.md` §4.
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
10. **Does the desktop GUI itself host the control API**, so a phone can control the desktop app
    directly (not only a headless daemon)? Falls out for free from "the GUI has no private path into
    the engine" if yes — but the desktop build then inherits the token, loopback-default,
    connection-cap and macOS Local Network requirements a pure daemon needs.
    `references/daemon-architecture.md` §1.
11. **Remote control from outside the LAN**: LAN-only + a documented tunnel (WireGuard/Tailscale/SSH),
    a NAT-traversing peer connection (WebRTC, Music Assistant's choice for Sendspin remote), or a
    hosted relay. Every pairing/token design in this skill currently assumes "never needed" by
    omission, not by decision. `references/daemon-architecture.md` §3.
12. **Does streamboat report plays back to TIDAL** ("Recently Played", My Mixes, artist payouts) from
    the headless daemon? No headless precedent in the reference set does this — silently empty
    listening history is the default outcome if this isn't decided. Cross-reference the `tidal-api`
    and `tidal-client-features` skills. `references/daemon-architecture.md` §6.
13. **Whether a `/playQueues` queue written through the official PKCE API is visible to the
    unofficial-API session the rest of streamboat uses**, and whether the two logins can be merged
    into one UX. `references/tidal-connect.md` §4.

## Unverified — do not present these as settled fact in specs or code comments

- **TIDAL's actual partner terms for Connect certification/licensing.** `developer.tidal.com` and
  `tidal.com/connect` are blocked from this research environment; every partner/device-count claim
  is second-hand trade press. `references/tidal-connect.md` §1.
- **Whether the Connect control channel really is JSON-over-WebSocket** resembling the Cast media
  namespace — inferred from `websocketpp` in the binary's licence folder plus Cast-shaped client
  types, with no wire capture. `references/tidal-connect.md` §4.
- **The `cloudConnect` wire *transport* and the desktop client's `etag`/`itemsEtag` concurrency
  fields.** Still genuinely undocumented. **Not unverified**: the underlying server-side queue
  *resource* — `/playQueues` — is officially documented (trap #15 above); don't re-flag that half as
  a gap. `references/tidal-connect.md` §4.
- **Google Cast's exact FLAC quality ceiling** (≤96 kHz/24-bit, cited elsewhere in this project's
  research). Sourced only via a search-index summary of `developers.google.com`, which is blocked
  from this research environment — re-verify before quoting a specific number; it does not change the
  recommendation to skip a Cast sender in v1. `references/mpd-and-multiroom.md` §5.
- **Whether a Windows service can open WASAPI output in session 0**, and whether `streamboat service
  install` should exist on Windows at all — assume not until the named spike (`references/daemon-
  architecture.md` §1) is actually run. That section also carries the concrete non-service fallback
  (snapclient's tray-app precedent) to use while this stays open.
- **The exact MPD command list Nuclear implements** — its docs site is blocked from this
  environment; port-fallback behaviour (6600→6601-6609) came from a search-index summary and needs
  re-verification. (The core MPD *protocol* facts are separately confirmed against the primary spec
  and do not need re-checking.) `references/mpd-and-multiroom.md` §1.
- **The Sendspin spec's licence**, and its adoption trajectory outside Music Assistant.
  `references/mpd-and-multiroom.md` §4.
- **Measured CPU cost of 24/192 FLAC decode on Pi 3/4/5.** No benchmark found anywhere in the
  reference set; don't quote a number. `references/raspberry-pi-deployment.md` §1.
- **Current hi-res status of `lms-plugin-tidal`.** Genuinely mixed (some issues closed, some open,
  one suggesting hi-res has since landed) — don't state it as simply broken or simply fixed.
  `references/headless-daemon-precedents.md` §6.
- **Whether TIDAL has ever taken action against Connect-binary redistributors.** No takedown found,
  but absence of evidence only. `references/tidal-connect.md` §5.
