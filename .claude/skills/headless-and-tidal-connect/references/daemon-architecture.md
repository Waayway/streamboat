# Daemon architecture: core split, control protocol, security, and pairing

Table of contents:
1. Three core-split models, the recommendation, and process management per platform
2. Local IPC and control surfaces: MPRIS (corrected), a Unix domain socket, HTTP+WebSocket
3. Security model: tokens, binds, and the URL-path-vs-cookie decision
4. Control protocol design: event schema, state sync, and versioning/capability negotiation
5. Pairing a headless daemon with no screen; token/session identity between GUI and daemon
6. Implementing Pushkin (TIDAL's streaming-privileges websocket) correctly

---

## 1. Three core-split models, the recommendation, and process management per platform

Three models, all with precedents:

**Model 1 — one binary, mode flag (tidalt).** `streamboat` opens the GUI; `streamboat daemon` runs
headless; the GUI, if a daemon already owns the lock/bus name, becomes a *client*. One build, one
install, one config, no protocol version skew during upgrades. Cost: the GUI carries daemon code
and vice versa; the "client mode" GUI must handle a reduced feature set.
[`ref:tidalt` — see `headless-daemon-precedents.md` §2]

**Model 2 — GUI always a client of a daemon (MPD, Roon, Music Assistant).** The engine only ever
lives in one process; the GUI is a pure front-end over the same API the mobile app and web remote
use, which forces the API to be good. Cost: the desktop app must start, supervise and possibly
bundle a daemon; every desktop feature crosses an IPC boundary; a crashed daemon is a broken app; on
Windows/macOS "start a background service" is a chore users resent.

**Model 3 — GUI embeds the core in-process; the daemon is a separate, optional binary (librespot
family).** `streamboat-core` as a library; `streamboat` (GUI) links it; `streamboatd` links it too.
Cost: two binaries to ship; risk of the two drifting; state can't be shared between a running GUI
and a running daemon on the same host (which is exactly what the lock/bus-name check in Model 1
solves).

**Recommendation: Model 1 implemented on top of Model 3's library split.**

- `streamboat-core`: session/auth, catalogue, queue, player engine, output backends, Pushkin.
  No UI, no server. This is the artifact that makes a future mobile app cheap. Look at librespot's
  own crate boundaries when deciding where core ends (`headless-daemon-precedents.md` §9: `core`,
  `audio`, `playback`, `metadata`, `protocol`, `oauth`, `discovery`, `connect` as separate crates —
  note OAuth and zeroconf discovery are their own crates, and Connect-receiver logic lives in its
  own crate rather than inside the player). The core/UI split is unanimous outside Rust/Go too:
  Mopidy→{mopidy-mpd, mopidy-mpris, Iris}, tidalt→{TUI, daemon}, and **TidalSwift**→{`TidalSwiftLib`
  (session, catalogue, playback), a thin `TidalSwift` SwiftUI app} all follow the same shape — the
  only Swift-side precedent in this project's source set, and directly relevant to the future iOS
  build, where Swift/SwiftUI (not Rust/Tauri) is the realistic toolkit. [`ref:tidalswift` — see
  `TidalSwiftLib/` vs. `TidalSwift/` as separate targets in the checkout]. Before writing the TIDAL
  client from scratch, evaluate
  `ref:tidalrs` (crates.io `tidalrs`, phayes/tidalrs) — "a comprehensive Rust client library for the
  Tidal … API" with async/await on Tokio, device-code OAuth2, automatic token refresh, DASH/MPEG
  streaming and typed catalogue models, targeting the same unofficial v1 API this project uses (a
  companion `tidalv2` crate covers the JSON:API v2 surface). Its `Client` holds an
  `on_authz_refresh_callback: Option<AuthzCallback>`, "invoked whenever the client automatically
  refreshes" — exactly the hook needed to persist a refreshed token back to the keyring and notify a
  GUI sharing the same account (§5's token-identity problem). It is a third-party crate carrying the
  same unofficial-API risk as everything else here — evaluate it, don't dismiss it untested.
  [verified-source `ref:tidalrs/README.md`, `ref:tidalrs/src/lib.rs:271-281,321,401`]
- `streamboat-server`: the control API (HTTP+WS), MPRIS, mDNS, optional MPD listener. Depends on
  core.
- One shipped binary with subcommands (`streamboat`, `streamboat daemon`, `streamboat play`,
  `streamboat service install`), plus — if packagers want it — a `streamboatd` alias that is the
  same binary.
- The GUI **always** talks to the engine through the same command/event types the server exposes,
  even in-process. That keeps the API honest without paying an IPC tax for local use, and makes
  "GUI as a remote for another host's daemon" a configuration change rather than a rewrite.

**"One binary with subcommands" describes the command surface, not the build — the sibling
tech-stack research specifies the opposite at build time, and reconciling the two matters or a
headless install drags in a webview.** `docs/research/tech-stack.md:816-819` (quoting an earlier
draft; the crate is now named `streamboat-server`, not `streamboat-daemon` — see
`tech-stack-evaluation/references/architecture-shapes.md` §1, the reconciled and canonical naming):
"Add `crates/streamboat-daemon` with a `[[bin]]` that does not depend on `tauri` at all. `cargo
build -p streamboat-daemon` on a server with no GTK/WebKit installed works as long as the daemon
crate does not pull the desktop crate. Enforce with a workspace dependency graph and a CI job that
builds the daemon in a minimal container." `tech-stack.md:134` likewise plans a `daemon` binary, a
`cli` client, and a Tauri desktop shell as three separate artifacts (also pre-reconciliation
naming). tidalt (the one-binary precedent above) is a Go TUI with no webview, so its shape does not
transfer directly. **The reconciliation**: one user-facing command surface (`streamboat` /
`streamboat daemon` / `streamboat play`) compiled as **two Cargo build artifacts** from one
workspace — a GUI binary linking the webview, and a `streamboat-server` crate whose binary is
`streamboatd`, which must not, verified by a CI job that builds the `streamboat-server` crate alone
in a GTK/WebKit-free container. On a Pi, the installed package is `streamboatd` alone. Use
`streamboat-server`/`streamboatd` in any new text; the `streamboat-daemon` name above is quoted
verbatim from the pre-reconciliation report and should not be copied forward.
[verified-source `docs/research/tech-stack.md:134,816-819` — line numbers drift as the file is
edited; re-locate by the quoted phrases above if they no longer match]

**Does the desktop GUI itself expose the control API, so a phone can control the desktop app
directly and not only a headless daemon?** tidal-hifi — the precedent this skill's URL vocabulary is
modelled on (§3, `headless-daemon-precedents.md` §4) — is itself a *desktop GUI* whose entire API
exists so external things can control it; `ref:sone` (also a GUI) ships two local servers of its own
(`headless-daemon-precedents.md` §3). If the GUI hosts the same API a headless daemon does, it also
needs the token, loopback default, connection cap, firewall-prompt handling and macOS Local Network
grant below — decide this explicitly as one toggle in one place, rather than have a second, weaker
server appear in the GUI later because nobody decided it shouldn't.

**Process management, per platform:**

| Platform | Mechanism | Notes |
| --- | --- | --- |
| Linux (desktop) | `systemd --user` unit, generated by `streamboat service install` | Copy tidalt's template shape; `After=`/`PartOf=graphical-session.target` for a desktop daemon, but for a **headless server** use `WantedBy=default.target` and enable lingering (`loginctl enable-linger`) so it survives logout. Add `After=time-sync.target` (`Wants=`) — see `raspberry-pi-deployment.md` §3. |
| Linux (server/Pi) | system unit + a dedicated user, or `systemd --user` + linger | Needs `audio` group membership for `/dev/snd`; if PipeWire is in play the daemon must run in the user's session or use ALSA directly |
| Windows | `windows-service` crate (`define_windows_service!`) | Windows services run in **session 0**, which has no interactive audio endpoint by default. No first-party Microsoft statement of the exact rule was found in this research (general WASAPI docs only) — treat "can a service open WASAPI?" as unverified. The realistic shape of the answer: (a) a per-user Scheduled Task at logon, (b) a tray/background app that autostarts, or (c) a service that only ever drives a pipe/network output and never opens a local device directly. **This is a named, one-hour spike, not an open question to leave unresolved**: run a WASAPI render loop from a Windows service and from a scheduled task, and record which one produces sound. The answer decides whether `streamboat service install` exists on Windows at all. |
| macOS | `launchd` LaunchAgent (per-user, has audio) rather than a LaunchDaemon | A system LaunchDaemon cannot open CoreAudio the way a per-user LaunchAgent can — same audio-session caveat as Windows, but with a clearer answer. |
| Container | plain PID 1 + `restart: unless-stopped`; host networking if mDNS is used | `ref:tidal-connect/docker-compose.yaml` is a ready-made template for a different purpose (see `raspberry-pi-deployment.md` §2 for the build/distribution decisions this implies). |

**Windows headless is not as open a question as the table above implies — a precedent already
settles the practical half.** `snapclient` — the most widely deployed cross-platform audio playback
daemon after librespot — has **no Windows service mode at all**: `client/snapclient.cpp` contains no
`StartServiceCtrlDispatcher`/`SERVICE_WIN32` code, its daemonisation (`#ifdef HAS_DAEMON`,
`setpriority`, `/var/run/snapclient/pid`) is Unix-only, and on Windows it runs as a foreground
console app; the project's own Windows answer is a third-party tray app ("Snap.Net"). That is direct
support for option (b) above — a tray/background app that autostarts — as the pragmatic default:
**treat headless as a Linux/server story; ship Windows and macOS a GUI with a background/tray mode,
not a true service**, without waiting on the WASAPI-in-session-0 spike to decide the product
question (the narrow technical question stays unverified regardless).
[documented-web: https://raw.githubusercontent.com/badaix/snapcast/develop/client/snapclient.cpp,
`/develop/README.md` (Snap.Net tray client)]

**MPRIS and D-Bus device reservation both need a D-Bus *session* bus, and a true headless deployment
(system unit, no login session) does not have one by default — both silently fail, not error.**
`ref:tidalt/internal/mpris/server.go:145,207` and `ref:tidalt/internal/player/mpv.go:322` all call
`dbus.ConnectSessionBus()`. `systemd --user` + `loginctl enable-linger` (row above) genuinely does
get a session bus at `$XDG_RUNTIME_DIR/bus` even with nobody logged in — MPRIS is day-one there. For
a system-unit Pi deployment, either (a) own the MPRIS name on the **system** bus via a
`--dbus-type system`-style config (spotifyd's own answer, `headless-daemon-precedents.md` §9), which
needs a D-Bus policy file under `/usr/share/dbus-1/system.d/` granting the service user `own` on
`org.mpris.MediaPlayer2.streamboat`, or (b) make MPRIS registration optional and non-fatal — log and
continue. The same caveat applies to `org.freedesktop.ReserveDevice1` reservation (a
PipeWire/PulseAudio session convention, absent on a bare-ALSA system unit; make it non-fatal too).
The single-instance check recommended above via MPRIS bus-name ownership inherits this dependency
and needs a non-D-Bus fallback (a lock file, or an abstract Unix socket) on headless boxes.
[verified-source `ref:tidalt/internal/mpris/server.go:145,207`,
`ref:tidalt/internal/player/mpv.go:322`; documented-web
https://raw.githubusercontent.com/Spotifyd/spotifyd/master/docs/src/advanced/dbus.md]

**systemd unit hardening, runtime/state directories, and where the Unix control socket (§2) actually
lives are unspecified above — the tidalt template cited is a five-line desktop unit with no
directory story for a system service that has no `$HOME`.** For a proper system unit:
`RuntimeDirectory=streamboat` → `/run/streamboat` (`control.sock` here, `0600` in a `0700`
directory); `StateDirectory=streamboat` → `/var/lib/streamboat` (queue state, tokens);
`ConfigurationDirectory=streamboat` → `/etc/streamboat`; plus `User=streamboat`,
`SupplementaryGroups=audio`, `Restart=on-failure`, `After=network-online.target time-sync.target`
with matching `Wants=` for both. Add hardening (`NoNewPrivileges=yes`, `ProtectSystem=strict`,
`ProtectHome=yes`, `PrivateTmp=yes`) but note explicitly that `PrivateTmp=yes` breaks a classic
`/tmp/snapfifo` Snapcast pipe (`mpd-and-multiroom.md` §2) since the unit then sees its own private
`/tmp` — document the interaction rather than let a packager discover it as a silent Snapcast
failure. For `systemd --user` (desktop, or headless-with-linger), the socket instead belongs at
`$XDG_RUNTIME_DIR/streamboat/control.sock`. [verified-source `ref:tidalt/cmd/tidalt/daemon.go` (the
unit template, `graphical-session.target` only); systemd.exec(5)
`RuntimeDirectory`/`StateDirectory`/`ConfigurationDirectory` — general systemd documentation]

**Graceful shutdown and restart-during-playback are unspecified here, even though this section
recommends systemd, package updates and a state file — all three routinely stop the daemon while
music is playing.** `systemctl restart streamboat` and an unattended distro upgrade both happen
mid-track. Without an explicit answer the daemon either gets SIGKILLed at `TimeoutStopSec` with a
truncated ALSA buffer (an audible click, and a lost queue write), or hangs shutdown. Specify,
alongside the `RuntimeDirectory`/`StateDirectory` guidance above: on `SIGTERM`, stop feeding the PCM
and issue `snd_pcm_drop` — **not** `snd_pcm_drain`, which plays out up to a full buffer of stale
audio and delays exit — or a short fade; release the `ReserveDevice1` reservation; flush the
queue/state file synchronously; close the Pushkin socket cleanly so TIDAL is not left holding a stale
privileged session; then exit. Set `TimeoutStopSec=` above the worst-case state-flush time. Two
related, still-unstated decisions: `KillMode=` (the systemd default, `control-group`, will kill a
`process://`-spawned Snapcast helper or a `streamboat snapcast-plugin` child alongside the daemon —
is that wanted?); and whether `Restart=on-failure` should resume playback on the restart it triggers
(this must agree with the queue-restart decision in `raspberry-pi-deployment.md` §6, or a crash loop
silently becomes a music loop). `ref:tidalt`'s daemon has no shutdown handling to copy here — this is
a genuine specification gap, not a lookup. [absence in `ref:tidalt/cmd/tidalt/daemon.go` (a five-line
unit, no `TimeoutStopSec`/`KillMode`); systemd.kill(5)/systemd.service(5)
`KillMode`/`TimeoutStopSec` semantics — general systemd documentation]

**No effort or sequencing estimate is attached to the staged path in `SKILL.md`, though the design-
options comparison rates individual options XS–L, and stage 1.2 in particular bundles three items
this skill elsewhere rates M–L each.** The raw material is already scattered across this skill, just
never aggregated: the MPD subset is "a hand-written parser plus a state machine, on the order of a
few thousand lines" (`mpd-and-multiroom.md` §1) with no drop-in server library in Rust or Go — the
single largest item in the whole staged path, larger than the entire control API; the pipe output is
"tens of lines" (`mpd-and-multiroom.md` §2); MPRIS is "~a day of work" (`SKILL.md`'s staged-path
list). Split the discovery+pairing / MPD-subset / diagnostics work into three independently
shippable stages, and treat the MPD subset as the one stage that can be deferred indefinitely without
blocking anything else, since nothing else in this architecture depends on it.

**macOS 15+ and iOS's Local Network permission, and the first-run Windows firewall prompt, apply to
any streamboat code that does mDNS or LAN control.** macOS 15 (Sequoia) extended iOS's Local Network
privacy to the Mac: an app using Bonjour/mDNS or making LAN connections must ship
`NSLocalNetworkUsageDescription` in `Info.plist`; denial is silent at the API level (discovery just
returns nothing). Two traps: (a) if an older build of the same bundle ID without the key was ever
installed, the custom purpose string may not display; (b) LAN TCP connections can fail on Sequoia
when the app is not run from `/Applications` — bites during development and CI. Wire the key into
the Tauri macOS bundle's `Info.plist`; render an explicit "local network permission denied" UI state
rather than a silently empty device list; the future iOS app needs the key plus `NSBonjourServices`
listing `_streamboat._tcp`. On Windows, the first non-loopback bind or mDNS responder triggers a
Windows Defender Firewall prompt — decide whether the installer pre-creates the rule or the app asks
at first LAN-enable. [documented-web: https://developer.apple.com/forums/thread/759262]

**Sleep/idle inhibition is unaddressed, though the desktop GUI is expected to embed the core.** A
player that lets the machine suspend mid-track, or that keeps it awake merely because the process is
running (not because anything is playing), is an immediately visible bug. Acquire a sleep/idle
inhibitor only while actually playing, release on pause/stop: Linux —
`org.freedesktop.login1.Manager.Inhibit(what="sleep:idle", who, why, mode="block")` (login1 is on
the **system** bus, so — unlike MPRIS above — this genuinely works headless with no session bus);
macOS — an `IOPMAssertion`; Windows — `SetThreadExecutionState(ES_CONTINUOUS|ES_SYSTEM_REQUIRED)`.
Pair it with resume-from-suspend, also unaddressed in the lifecycle notes (`raspberry-pi-deployment.md`
§6): on wake, the ALSA device may be gone and the Pushkin socket dead (§6) — both need an explicit
reopen/reconnect path, not a silent stuck-paused state. [freedesktop login1 `Inhibit` interface —
general specification; `ref:tidalt/cmd/tidalt/daemon.go` ("No audio device is opened until playback
starts") as the device-side analogue]

**Config locations** — follow platform conventions rather than inventing:
`$XDG_CONFIG_HOME/streamboat/config.toml` (Linux), `~/Library/Application Support/streamboat/`
(macOS), `%APPDATA%\streamboat\` (Windows); cache under `$XDG_CACHE_HOME`; state/tokens under
`$XDG_STATE_HOME` or the keyring. A daemon must accept `--config <path>` because a system service
has no `$HOME` worth speaking of. Precedents: `~/.config/sone/settings.json` + `sone.key`
(`ref:sone`), `~/.config/tidalt/secrets` + `~/.local/share/tidalt/play.log` (`ref:tidalt`),
`/var/lib/mopidy/tidal/tidal-*.json` and `/etc/mopidy/mopidy.conf` (`ref:mopidy-tidal`).

**Logging**: log to stderr with a level flag and let the supervisor capture it (journald,
`journalctl --user -u streamboat -f`); offer `--log-file` for Windows/macOS where there is no
journal. tidalt writes a dedicated `play.log` for the deep-link path because that code runs with no
terminal — a good idea worth copying for any path invoked by the OS.

**Nothing here addresses token/credential redaction, and every precedent in this skill logs at
level 3+ by default.** journald entries are readable by any user in the `systemd-journal` group on a
shared box, and §3 recommends logging the reachable control-API URL. Bearer tokens, the pairing
token, the Pushkin WebSocket URL (token-bound per §6), signed CDN URLs, and the device-code
`user_code` are all things this daemon handles — any one of them printed once is a durable credential
leak on a shared machine. **Rule**: never log the control-API token, the pairing token,
`Authorization` headers, refresh/access tokens, the `rt/connect` WebSocket URL, or signed CDN URLs at
any level — log a stable prefix or hash instead when a correlation id is needed. Named traps: the
Sone-derived wildcard-bind-URL display (§3) must never carry the URL-path token form Sone itself
uses; tidalt's `play.log` is 0600 in a 0700 directory — copy the permissions, not just the file's
existence; `streamboat doctor`'s output and any debug bundle are the other place credentials escape —
redact there by construction, not by review. [`ref:tidalt/cmd/tidalt/play.go` (`play.log` at 0600 in
a 0700 dir); `ref:sone/src-tauri/src/overlay/server.rs` (wildcard-bind URL display)]

**Updates**: a daemon that auto-updates itself and restarts mid-track is hostile. Prefer OS
packaging (systemd + distro/Flatpak/AUR/Homebrew), and if an in-app updater exists, make it
"download now, apply on next idle".

---

## 2. Local IPC and control surfaces: MPRIS (corrected), a Unix domain socket, HTTP+WebSocket

**MPRIS is not "transport + metadata only" — an earlier internal draft of this research undersold
it.** The full spec defines four interfaces: `org.mpris.MediaPlayer2` (Root), `.Player`,
`.TrackList` and `.Playlists`. `mpris-server` 0.9 — already a dependency of `ref:sone`, and the
crate this project already recommends — implements all of them: `TrackListInterface`
(`GetTracksMetadata`, `AddTrack`, `RemoveTrack`, `GoTo`, `TrackListReplaced`) and
`PlaylistsInterface` (`GetPlaylists`, `ActivatePlaylist`). So MPRIS *can* express the queue and
stored-playlist activation for free, before any HTTP API exists — what it genuinely cannot express
is catalogue **search** or hierarchical **browsing**. `ref:tidalt`'s MPRIS-as-IPC wall
(`headless-daemon-precedents.md` §2) is a scoping choice (it implements only Root+Player), not a
ceiling MPRIS itself imposes.
[documented-web: https://raw.githubusercontent.com/SeaDve/mpris-server/main/README.md]

**A same-host, no-listener IPC surface is missing from the naive "MPRIS or HTTP" framing: a Unix
domain socket.** No port, no token, no TLS question — filesystem permissions are the auth model.
`ncspot` is the direct precedent: alongside embedding librespot as a library, it creates a
Unix-domain socket at the platform runtime directory **on Linux/macOS/\*BSD — Windows has no
equivalent in this precedent**, contrary to an earlier draft of this research; clients write plain
command words (`play`, `playpause`) and receive newline-delimited JSON after every state change,
e.g. `{"mode":{"Playing":{"secs_since_epoch":…}},"playable":{"type":"Track",
"id":…,"title":…,"duration":184132,"artists":[…],"cover_url":…}}`. `ncspot info` prints the socket
location. Documented uses: controlling a detached tmux session, status bars, startup scripts.
**Recommend this as streamboat's own v0 control surface** for the CLI and same-host GUI on
Linux/macOS, with the HTTP/WS API (below) re-exposing the same command/event vocabulary over the
network in v1. Rust's `interprocess` crate genuinely does cover both Unix sockets and Windows named
pipes behind one API — but **do not cite ncspot as precedent for the Windows half**: its own
`src/ipc.rs` imports only `tokio::net::{UnixListener, UnixStream}`, and its docs state the socket
exists "on UNIX platforms (Linux, macOS, \*BSD)" only, with no Windows named-pipe dependency in
`Cargo.toml`. The Windows named-pipe path has no reference implementation anywhere in this skill's
source set and should be tested explicitly, not assumed to behave the same way.
[refuted-and-corrected, documented-web:
https://raw.githubusercontent.com/hrkfdn/ncspot/main/doc/users.md lines 191-235,
https://raw.githubusercontent.com/hrkfdn/ncspot/main/src/ipc.rs]

**Rich IPC (HTTP + WebSocket JSON)** expresses everything, needs auth, needs a client — this is
where go-librespot, Mopidy, Music Assistant, tidal-hifi and Sone all landed
(`headless-daemon-precedents.md` §3-4, §9). streamboat needs **all three** surfaces: MPRIS (now
including queue/playlist) for OS integration, a Unix socket for same-host scripting and the CLI, and
a rich local API for real remotes.

**The command-transport *style* for that rich IPC is never actually decided above, and the default
here — REST + a separate WebSocket event stream, copied from tidal-hifi and go-librespot — is the
opposite choice from every *server-shaped* precedent already in this skill's own source set.**
JSON-RPC 2.0 as the command transport (not just a naming convention) already appears three times as
precedent: Mopidy exposes "HTTP/WebSocket JSON-RPC" (`headless-daemon-precedents.md` §1); snapserver's
control port 1705 is TCP JSON-RPC and its 1780 port is HTTP + WebSocket JSON-RPC
(`mpd-and-multiroom.md` §2); the Snapcast stream-plugin protocol is newline-delimited JSON-RPC 2.0
over stdin/stdout (`mpd-and-multiroom.md` §2). tidal-hifi's REST surface — the one this skill copies
its URL vocabulary from — is a GUI remote-control endpoint with **no event stream at all**, so it is
precedent for *URL naming* only, never for the transport architecture; go-librespot's REST+`/events`
split is the one real precedent for the two-transport shape recommended above, and the only
server-shaped precedent here that does not use JSON-RPC. Two transports means two framings, no
request/response correlation ids on the WebSocket half, and every client (including the future mobile
app) needing both an HTTP client and a WS client for full functionality. **Concrete recommendation**:
pick one framing for commands — JSON-RPC 2.0 request/response with ids, carried over the same
WebSocket that already carries events (§4) — plus a thin REST facade mapping tidal-hifi's URL
vocabulary onto the same method names, kept for curl/Stream Deck users who want a one-shot `curl -X
POST`. **gRPC is rejected**: it needs protobuf codegen in every client, has no usable browser story
without grpc-web, and buys nothing over JSON for a LAN daemon's command surface — this closes out the
brief's original "JSON-RPC/WebSocket/gRPC API" comparison request, which this skill otherwise never
answered. Also decide whether the Unix-socket surface above speaks the same JSON-RPC framing —
recommended: yes, since ncspot's NDJSON shape and JSON-RPC 2.0 are both newline-delimited JSON and
can share one codec. [this skill's own `headless-daemon-precedents.md` §1/§9 and `mpd-and-multiroom.md`
§2 text; `ref:tidal-hifi/src/features/api/swagger.json`]

**MPRIS bus-name convention**: `ref:tidalt` uses the short form `org.mpris.MediaPlayer2.tidalt`;
`ref:high-tide` uses the reverse-DNS form `org.mpris.MediaPlayer2.io.github.nokse22.high-tide`.
Either is valid under the spec — pick the short form (`org.mpris.MediaPlayer2.streamboat`) unless
packaging conventions (e.g. a Flatpak app-id) push toward reverse-DNS.

**The control API's own default port, bind address, and enable-state are unspecified above — and
tidal-hifi's actual defaults, cited as the naming precedent, are exactly the wrong posture to copy.**
`ref:tidal-hifi/src/scripts/settingsStore.ts:44-47` sets `api: true` (**enabled by default**) with
`apiSettings: { port: 47836, hostname: "127.0.0.1" }`, and `ref:tidal-hifi/src/features/api/index.ts`
(49 lines) has **no authentication of any kind** — grepping for `auth`/`token`/`bearer`/`password`
returns nothing. Copy the URL vocabulary; reject the posture — streamboat's API ships **disabled by
default, or token-required from first boot**, never open-and-unauthenticated. Other ports already in
this project's precedent set, for choosing a non-colliding default: Sone MCP 5577 / overlay 5578
(`headless-daemon-precedents.md` §3), go-librespot 3678, Music Assistant 8095, snapserver
1704/1705/1780/1788 (`mpd-and-multiroom.md` §2), mopidy-tidal login server 8989, MPD itself 6600
(Nuclear's fallback ladder 6601-6609, `mpd-and-multiroom.md` §1). **Recommendation, not yet an owner
decision — override freely**: default control-API port **4747** (collides with none of the above),
bound `127.0.0.1`, **disabled until a token is generated** (first-run `streamboat service install`
or `streamboat token create` mints one) rather than merely "token-required" with an open default —
matching trap #18's "never open-and-unauthenticated" rule at the enable-state level too. Whether to
instead bind port 0 and advertise the assigned port via mDNS (below), as a fixed-port alternative,
is still open. [verified-source `ref:tidal-hifi/src/scripts/settingsStore.ts:44-47`,
`ref:tidal-hifi/src/features/api/index.ts` (no auth)]

**`_streamboat._tcp` TXT record keys and instance identity are unspecified.** A controller (desktop
GUI, future mobile app) must decide from the browse result alone whether a discovered daemon is
compatible, already paired, and reachable over TLS. **Recommended set** (every input needed to pick
these already exists above; treat this as the default, not merely a floor): `v=` protocol version
(feeds the capability negotiation in §4); `id=` a stable instance UUID persisted in
`$XDG_STATE_HOME` (so a rename or DHCP change doesn't create a duplicate discovery entry — Sendspin
does this with a persistent Curve25519 static key as `client_id`/`server_id`, `mpd-and-multiroom.md`
§4); `name=` display name; `path=` API base path; `tls=0|1`; `auth=required|open`. Precedent for
splitting rather than overloading one record: snapserver advertises four separate service types for
its four ports; Sendspin uses two, one per connection direction (`mpd-and-multiroom.md` §2, §4).
Keep the friendly-name default single-word-safe — the Connect binary's own issue #216
(`tidal-connect.md` §2.4) is a directly relevant lesson.

**`streamboat play <url>` needs OS-level URL-scheme registration to be reachable from anything but a
terminal — deep-link *forwarding* code (§1's tidalt precedent) is not the same as *registration*.**
tidalt's own mechanics are the right recipe to copy, but **register `streamboat://`, not
`tidal://`** — see `tidal-api/references/auth.md` §13, the canonical owner of this decision:
`tidal://` is claimed by the official TIDAL desktop app itself plus Strawberry, Sone, and High
Tide, and OS handler registration is last-writer-wins, so claiming it steals it from whichever app
the user installed most recently. `ref:tidalt/cmd/tidalt/setup.go:16,38,45` shows the mechanics to
adapt (swap the scheme name): embeds `cmd/tidalt/tidalt.desktop:8`
(`MimeType=x-scheme-handler/tidal;` → `x-scheme-handler/streamboat;` for streamboat), writes it to
`~/.local/share/applications/` with the binary's absolute path in `Exec=`, then runs `xdg-mime
default streamboat.desktop x-scheme-handler/streamboat` and `update-desktop-database`
(`ref:tidalt/docs/browser-url-handler.md:92-113` gives the verification command and a KDE
`mimeapps.list` gotcha). Windows: `HKCU\Software\Classes\streamboat` with a `URL Protocol` value and
a `shell\open\command` key. macOS: `CFBundleURLTypes` in `Info.plist`. Separately, streamboat should
still **parse** (not register) `tidal://` content links so pasted links from other apps work — make
**claiming** `tidal://` an explicit opt-in setting, off by default, the same treatment as claiming
`https://tidal.com/...`, which also steals ordinary web links from the user's default browser and
should be opt-in, never a silent default.

---

## 3. Security model: tokens, binds, and the URL-path-vs-cookie decision

Adopt Sone's shape, not its exact carrier (`headless-daemon-precedents.md` §3): loopback by
default, opt-in LAN bind, a random token generated on first enable and persisted in settings, an
explicit connection cap on the event stream, and the reachable URL logged with `127.0.0.1` even
when bound to the wildcard.

**But Sone's specific choice of a URL-path token is for machine clients, not browsers.** A token in
the URL lands in browser history, a shared-phone address bar, any `Referer` a third-party asset
sends, and reverse-proxy logs. Use `Authorization: Bearer <token>` for API/script clients, and for
the browser-facing web remote, exchange the token once for an `HttpOnly`, `SameSite` cookie scoped
to the daemon's origin.

Add `allow_origin` and optional `cert_file`/`key_file` to the control-API config schema from day
one — go-librespot's `server.{address,port,allow_origin,cert_file,key_file}` is the model (**there
is no `server.tls` key**; TLS is configured by supplying `cert_file`+`key_file`). go-librespot's own
default (`server.address: localhost`) matches the loopback-by-default recommendation here.

**A plain `http://` listener on a LAN IP is not a secure browser context.** Only `https://` origins
and `localhost`/`127.0.0.1` are secure contexts (W3C Secure Contexts spec) — a LAN IP over `http://`
gets no service worker, no installable PWA, no Web Crypto. Design the embedded web remote around
this: no service-worker dependency, a payload small enough to need no offline cache, and treat PWA
installability as gated on the TLS/tunnel decision (§5) rather than assumed.
[documented-web: https://w3c.github.io/webappsec-secure-contexts/]

**DNS rebinding is the classic attack on a loopback daemon, and "bind loopback + require a token"
does not by itself stop it.** Any web page the user visits can resolve an attacker-controlled
hostname to `127.0.0.1` and talk to the daemon from the browser's own origin; CORS is not a defence
there (the browser thinks it's talking to the page's own declared origin), and a token living in a
cookie or a readable config file is then compromised. go-librespot's `server.allow_origin` above is
a **CORS** control, a narrower thing than a rebinding control. **Concrete mitigation**: Host-header
allowlisting — accept only `Host: localhost`, `127.0.0.1`, `[::1]`, or the explicit configured LAN
bind address, reject anything else with 421/403 before routing — plus `Origin` checking on the
WebSocket upgrade specifically (a WS upgrade is not subject to the same-origin policy the way
`fetch` is). "Bind loopback" alone is widely and wrongly assumed to be sufficient.

**The control API has exactly one all-or-nothing token throughout this skill, with no permission
tiers, no per-client scopes and no revocation story — and the MPD listener this skill puts on the
same box already ships a four-level access model (`mpd-and-multiroom.md` §1), so streamboat risks two
incompatible authorization designs on one daemon.** A household web remote left open on a shared
tablet, a Home Assistant integration, and the owner's own phone want three different trust levels.
MPD's own `default_permissions` is "a comma-separated list of permissions" drawn from `read, add,
player, control, admin`, and `local_permissions`/`host_permissions`/`password` each carry their own
permission set — i.e. the reference design in this niche is capability tiers, not one bearer token.
Decide and document explicitly: (a) whether streamboat tokens carry a scope (at minimum `read` vs.
`control`, and whether `admin` — settings, logout, LAN-enable, token management — is separable from
`control`); (b) how a token is revoked while a WebSocket using it is already open (close the socket
immediately, or let it run to natural disconnect); (c) whether the MPD listener's permission set
derives from the same policy object as the HTTP token, or is configured independently — if
independently, say so loudly, because "I revoked the phone's token but MALP still controls playback"
is the failure mode that results. Also decide whether MPD's own `commands`/`notcommands` on a
streamboat connection should reflect that connection's permission tier, matching MPD's own
per-connection-aware semantics. [documented-web:
https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/doc/user.rst
(`default_permissions`/`local_permissions`/`host_permissions`/`password`)]

**Remote control from outside the LAN is unaddressed, though every control surface here is LAN- or
loopback-scoped.** "Control my Pi from work" is the first feature request a headless product gets,
and it retrofits badly — it decides whether pairing tokens (§5) are bearer credentials that may
traverse the open internet, whether TLS becomes mandatory, and whether an ongoing relay-service
operating cost exists. Name the three positions and pick one: (a) **LAN-only** + a documented
WireGuard/Tailscale/SSH tunnel — zero code, zero liability, consistent with "every listener defaults
to loopback"; (b) a **NAT-traversing peer connection** — Music Assistant's own choice for Sendspin
remote access is WebRTC, per its Sendspin provider README; (c) a **hosted relay** — out of scope for
a project with no operating budget. Whichever is chosen, an internet-reachable token must be
short-lived and per-client revocable — an argument for the named-revocable-token option in §5.
[documented-web: https://github.com/music-assistant/server/blob/dev/music_assistant/providers/sendspin/README.md]

**Volume semantics when output is bit-perfect (no resampling, no software attenuation) and there is
no hardware mixer are unspecified, though MPRIS `Volume`, MPD `setvol`/`getvol`, and
`PUT /player/volume` all expose a slider.** Full detail and the MPD-side precedent in
`mpd-and-multiroom.md` §1 — summary: expose `volume: hardware | software | none` in `GET /health`
and make every surface derive from that one value rather than each guessing independently.

---

## 4. Control protocol design: event schema, state sync, and versioning/capability negotiation

**The control protocol's event schema and state-sync semantics are the most load-bearing artifact
in this whole architecture, and it is easy to leave unspecified.** "HTTP + WebSocket JSON … plus an
event stream" is not a design — it needs a message envelope, an event vocabulary, a
snapshot-vs-delta decision, sequence/revision numbers, a reconnect/resume rule, and a story for two
remotes editing the queue at once. Two precedents answer this directly:

- **go-librespot's WebSocket endpoint is `/events`**, with a closed vocabulary: `active`,
  `inactive`, `metadata`, `will_play`, `playing`, `not_playing`, `paused`, `stopped`,
  `seek{position}`, `volume{value,max}`, `shuffle_context`, `repeat_context`, `repeat_track`; its
  REST half is published as an OpenAPI spec (`api-spec.yml`) — worth doing the same for streamboat's
  own API from day one.
- **TIDAL's own client has a concurrency answer worth copying**:
  `ref:TidaLuna/plugins/lib/src/redux/types/store/PlayQueue.ts` defines
  `CloudQueue{queueId, etag, itemsEtag, currentItemId, headPosition, tailItemId, tailPosition,
  repeatMode, shuffled, historyMediaItemIds}` and
  `CloudQueueItem{id, media_id, properties{active, original_order, sourceType}}` — an ETag'd queue
  with stable per-item ids and explicit head/tail — plus
  `PlayQueueElement{context{type,id}, mediaItemId, priority, uid}`,
  `PlayQueueSourceType (album|artist|playlist|mix|search|MY_TRACKS|…)` and
  `RepeatMode{Off=0,All=1,One=2}`.

**Recommendation**: full snapshot on connect + typed deltas carrying a monotonic revision number, an
`itemsEtag` on the queue itself, stable per-item `uid`s (not array indices), and a documented
reconnect rule on a revision gap (resync with a fresh snapshot rather than replaying missed deltas).

**Protocol versioning and capability negotiation between the daemon, GUI, web remote and future
mobile app** is a second, separate gap: the GUI, the phone app and the daemon will be on different
versions the moment the project has real users. Two precedents solve it in opposite ways:

- **MPD's style (additive, tolerant)**: `commands`/`notcommands` advertise what the current
  connection may call; `protocol`/`protocol available`/`protocol enable {FEATURE}`/`protocol clear`
  negotiate optional behaviour per-connection (`mpd-and-multiroom.md` §1).
- **Sendspin's style (strict)**: "Core version 1, exact-match versioning" — any mismatch is a hard
  failure, not a negotiation (`mpd-and-multiroom.md` §4).
- **Recommendation**: the already-planned `GET /health` should return
  `{version, protocol_version, capabilities[]}`, and clients should degrade gracefully on a missing
  capability (MPD-style) rather than refuse to connect (Sendspin-style) — the daemon is a home
  appliance, not a paired security device, and a strict-versioning failure mode is a worse user
  experience than a silently-narrower feature set.

**The event vocabulary above (go-librespot's list) is transport-only — the states that actually need
surfacing on a headless box have no screen, and this design left them out.** Extend it with at
least: `auth_required` (device-code verification URL + user code, so a remote can render its own QR
— go-librespot's `GET /auth/code`, §5, is the REST half of the same idea); `auth_ok`;
`entitlement_changed` (subscription/quality-tier change); `quality_downgraded` (mopidy-tidal's
pre-flight `"HIRES_LOSSLESS" in track.media_metadata_tags` check, `headless-daemon-precedents.md`
§1, is the trigger to copy); `streaming_privileges_revoked` carrying `clientDisplayName` (§6);
`output_error` (device busy / xrun / rate unsupported); `track_unplayable`. Decide the accompanying
policy: on an unplayable track, skip-with-event or stop — and mirror the choice into MPD's `status`
`error` field (`mpd-and-multiroom.md` §1), which is otherwise listed but never wired to anything.
[documented-web https://raw.githubusercontent.com/devgianlu/go-librespot/master/API.md,
`/master/README.md` (`GET /auth/code`); verified-source
`ref:mopidy-tidal/mopidy_tidal/playback.py`;
`ref:tidal-sdk-web/packages/player/src/api/event/streaming-privileges-revoked.ts`]

---

## 5. Pairing a headless daemon with no screen; token/session identity between GUI and daemon

**Pairing a phone or GUI to a headless daemon that has no screen at all** is under-specified by "a
short pairing code exchanged for a long-lived token" alone — there is no mechanism, no expiry, no
revocation story, and the collision between "every listener defaults to loopback" and "the mobile
app controls the daemon" is unresolved. No single precedent solves this end to end; assemble it:

- go-librespot exposes its device-auth link and code over `GET /auth/code` "for as long as the
  daemon is waiting, so a frontend can show them instead of asking the user to read the logs", and
  supports TLS on the same listener via `server.cert_file`/`server.key_file` plus CORS via
  `server.allow_origin`.
- `ref:tidalt` renders a QR in the terminal with `qrterminal/v3` — works over SSH, no companion
  service needed.
- `ref:mopidy-tidal` serves a fixed-port login page — copy the pattern, not its unauthenticated
  wildcard bind (`headless-daemon-precedents.md` §1, §10).
- **Sendspin's pairing design is the most complete answer available** (`mpd-and-multiroom.md` §4):
  Noise `KKpsk2` with persistent Curve25519 identity keys, a long-term PSK from pairing, a Sentinel
  PSK for unpaired sessions, in-band rehandshake, and a fully specified QR pairing-token format —
  currently gated on an unresolved spec-licence question (`mpd-and-multiroom.md` §4).
- **A fifth shape, missing above until now: librespot's zeroconf pairing protocol — the closest
  working open-source answer to "how does an already-logged-in phone hand credentials to a screenless
  daemon with zero typing", and MIT-licensed and shipped for a decade, unlike Sendspin.** librespot's
  `discovery` crate (reused by spotifyd and go-librespot) advertises mDNS `_spotify-connect._tcp`
  with TXT records `VERSION=1.0` and `CPath=/`, on a port that **defaults to 0** — bind an ephemeral
  port and advertise the assigned value, the shipped answer to this skill's own still-open "port 0 vs.
  a fixed-port fallback ladder" question for `_streamboat._tcp` (§2). It serves two plain HTTP
  actions on that port: `GET /?action=getInfo` returns `status`, `statusString`, `spotifyError`,
  `version`, `deviceID`, `deviceType`, `remoteName`, `publicKey` (base64), `brandDisplayName`,
  `modelDisplayName`, `libraryVersion`, `groupStatus` (`GROUP|NONE`), `tokenType`, `clientID`,
  `scope`, `activeUser`, `aliases[]{name,id,isGroup}` — note `activeUser`, which lets a controller
  show "already claimed by X" in its picker before connecting, something this skill's own
  `_streamboat._tcp` TXT-key list (§2) has no equivalent for; and `POST /?action=addUser` accepts
  `userName`, `blob` (base64 encrypted credentials) and `clientKey` (base64 client public key). The
  crypto: Diffie-Hellman between the daemon's advertised `publicKey` and the controller's `clientKey`
  yields a shared secret; SHA1-HMAC derives an encryption key and a checksum key; the credential blob
  is a 16-byte IV + AES-128-CTR ciphertext + 20-byte HMAC-SHA1, MAC-checked before decryption.
  Zeroconf backends are selectable (`with-avahi`, `with-dns-sd`, `with-libmdns`), the same shape as
  go-librespot's `zeroconf_backend` key already cited in `headless-daemon-precedents.md` §9. Lessons
  to fold in: `CPath` is a working precedent for a `path=` TXT key and `VERSION` for a `v=` key (§2);
  port-0-and-advertise-the-assigned-port is a shipped answer, not a fallback ladder to invent; and the
  `getInfo`/`addUser` split is the shape of a pairing handshake that needs no shared secret
  pre-installed on the daemon — a genuine alternative or complement to Sendspin's Noise-based design.
  [documented-web: https://raw.githubusercontent.com/librespot-org/librespot/dev/discovery/src/server.rs,
  `/dev/discovery/src/lib.rs`]

**Decisions the owner still needs to make, not facts to look up**: (1) daemon-initiated pairing
(`streamboat pair` over SSH prints a short-lived code/QR) vs. client-initiated (a client asks, the
daemon logs a code the user confirms out-of-band); (2) one named, revocable token per client
(`streamboat tokens list|revoke`) vs. a single shared secret; (3) plain HTTP on the LAN (§3's
secure-context problem) vs. a self-signed certificate with a permanent browser warning vs.
loopback-only plus an SSH tunnel. A URL-path token (§3) is unsuitable for whichever of these ends up
rendered in a phone browser.

**Token and session identity when a GUI and a daemon share one host or one account is a related,
separate gap, and it is the first thing that breaks in real multi-process use.** Verified in code:
python-tidal's `Session.token_refresh()` posts `grant_type=refresh_token` with
`client_id`/`client_secret` and updates only `access_token`, `expiry_time` and `token_type` — it
never replaces `self.refresh_token`, so the refresh token is long-lived, not rotated on use, and two
processes holding the same refresh token do not invalidate each other
(`ref:python-tidal/tidalapi/session.py:717-750`). Still to decide and document:

- One shared token file with an advisory lock and a single writer, vs. separate logins per role
  (GUI vs. daemon).
- Whether the daemon uses a different `client_id` than the GUI — the Connect binary takes
  `--clientid` and python-tidal swaps `client_id`/`client_secret` between its OAuth and PKCE paths,
  so device identity is a per-client choice, and it is also the string TIDAL shows in
  `PRIVILEGED_SESSION_NOTIFICATION.clientDisplayName` (§6).
- Which process owns the Pushkin socket when both run on one host — two open Pushkin sockets on one
  account will fight each other exactly the way §6 warns about a daemon fighting the user's phone.

---

## 6. Implementing Pushkin (TIDAL's streaming-privileges websocket) correctly

TIDAL allows one concurrent stream per account and enforces it over a WebSocket, in both official
SDKs:

- `POST {legacyApiUrl}/rt/connect` with `Authorization: Bearer <token>` returns
  `{ "url": "<wss …>" }`. `ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts:
  fetchWebSocketURL`; `ref:tidal-sdk-android/player/streaming-privileges/src/main/kotlin/…/
  connection/StreamingPrivilegesService.kt` declares `@POST("rt/connect")` returning
  `StreamingPrivilegesWebSocketInfo(val url: String)`. The Android DI module registers
  `"${apiEndpoint}rt/connect"` as requiring `Credentials.Level.USER`.
- **Outgoing**: `{"type":"USER_ACTION","payload":{"startedAt":<epoch ms from a true-time source>}}`.
  The web SDK comments: "Call this method to tell Pushkin a user action happened, so it can make
  good qualified guesses if you're the session with allowed playback". Android names the same
  message `Acquire`.
- **Incoming**: `PRIVILEGED_SESSION_NOTIFICATION` (payload: `clientDisplayName`, `sessionId`,
  `endsAt{clientTime,serverTime}`, `updatedAt{…}`) → the web SDK immediately calls
  `playerState.activePlayer?.pause()` and dispatches a `streaming-privileges-revoked` event carrying
  the *other* device's display name; and `RECONNECT` → reconnect the socket.

**Implications for a headless streamboat daemon:**

1. Implement Pushkin, or a daemon left playing in another room will be silently killed by TIDAL's
   backend — or worse, will keep fighting the user's phone for the privilege.
2. Send `USER_ACTION` only on genuine user intent (a remote pressing play), never on autoplay or
   resume-after-network-hiccup, or the daemon will steal playback from the user's phone.
3. Surface "paused: playback started on `<clientDisplayName>`" through every control surface (MPRIS
   metadata, the WebSocket API, the MPD `idle`/status channel). Users will otherwise report it as a
   streamboat bug.
4. `clientDisplayName` is the string TIDAL shows for a device — register a sensible display name for
   the daemon (hostname-derived), which is also what a future streamboat-native remote picker
   should show.
5. The official auth stack this websocket's timestamp depends on is TrueTime-gated — see
   `raspberry-pi-deployment.md` §3 for why a Pi's missing RTC makes this concretely fragile at
   boot.

**The reference Pushkin client is a browser SDK's reconnect logic, and copying it verbatim into a
long-running daemon is actively unsafe.** Read directly in
`ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts` (`#connect`,
`#handleSocketClose`, `#handleSocketMessage`, `reconnect`, `userAction`, static `ensure`/`refresh`):

6. **No backoff, no jitter, no retry cap.** `#handleSocketClose()` is
   `this.#connecting = this.#connect()` — immediate, unconditional reconnect — and `#connect()`
   re-`POST`s `/rt/connect` for a fresh URL every time, so each reconnect costs one authenticated API
   call. Copied verbatim, a daemon offline for months will hot-loop `POST /rt/connect`. **Add
   exponential backoff with jitter and a ceiling** — this is exactly the kind of behaviour that gets
   an unofficial-API client's account or `client_id` flagged.
7. **The socket URL is token-bound, not fetched once.** `#connect()` calls `getAccessToken()` then
   `fetchWebSocketURL(accessToken)`; the SDK exposes `Pushkin.refresh()` "to re-setup pushkin with
   the new credentials". The daemon's token-refresh path (§5, `tidalrs`'s
   `on_authz_refresh_callback` above) must trigger a Pushkin reconnect explicitly, not just swap the
   HTTP client's bearer token.
8. **Network-recovery reconnect exists but the reference's version is a bug, not a pattern to copy.**
   `window.addEventListener('online', () => this.#handleSocketClose(), {once: true})` — note
   `{once: true}`: the browser SDK stops reacting to network-recovery events after the first one. A
   daemon's netlink/route-change watcher should not inherit that bug.
9. **Client-credentials sessions have no privileges socket at all**: `Pushkin.ensure()` is gated on
   `isAuthorizedWithUser()`. A daemon must authenticate as a real user (device-code or PKCE), never
   client-credentials, before Pushkin applies.
10. **On displacement, do not auto-retry.** On `PRIVILEGED_SESSION_NOTIFICATION` the SDK pauses
    immediately and does **not** attempt to re-acquire — a daemon must not automatically resend
    `USER_ACTION` after being displaced; this is a hard stop in the reference client, not a race to
    retry.

**One account = one stream is also the hard constraint that decides the entire multiroom
architecture — read this together with `mpd-and-multiroom.md` §2, not in isolation.** You cannot run
two streamboat daemons (two rooms) against one TIDAL account: Pushkin enforces exactly one
privileged session per account, so a second daemon acquiring its own Pushkin socket immediately
pauses the first. Multiroom must therefore be **one daemon holding the single stream and fanning
decoded PCM out to many endpoints** (Snapcast server-with-many-clients, or later a Sendspin
*source*) — never N independent daemons each resolving their own manifest. A multi-subscriber
household runs multiple independent daemon *instances* (one per account), not one daemon switching
accounts per connection — every config path in §1 assumes exactly one account per daemon; state that
explicitly rather than leaving it implicit.

**Play reporting ("Recently Played") from the headless daemon is a related, unaddressed gap.** If
the daemon never reports plays, Recently Played / My Mixes / artist play counts (and therefore
artist payouts) stay empty for streamboat listening while the official app populates them normally
— and for a subscriber-only player, not reporting plays also means artists aren't credited for those
streams. No headless precedent in this project's reference set does it: `mopidy-tidal` and
`python-tidal` were grepped for report/scrobble/play-log endpoints and neither has one. Cross-reference
the play-reporting endpoint in the `tidal-api` skill (`ec.tidal.com` play log) and the feature tiering
in the `tidal-client-features` skill. Sub-decisions specific to a daemon: what counts as a "play"
(a duration/percentage threshold), what happens to reports queued while offline, and whether
Last.fm/ListenBrainz scrobbling is a daemon feature (it should be — the daemon is what plays) or a
GUI feature. [negative-result grep over `ref:mopidy-tidal/mopidy_tidal/*.py` and
`ref:python-tidal/tidalapi/*.py`]
