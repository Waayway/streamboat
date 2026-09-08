# Headless/daemon precedents, project by project

Table of contents:
1. mopidy-tidal — the full server precedent (and its caching-proxy legal-posture caveat)
2. tidalt — the "one binary, two modes" precedent
3. Sone — local HTTP control done carefully (and its token-security caveat)
4. tidal-hifi — a documented HTTP control API
5. upmpdcli — TIDAL over UPnP, and the renderer-whitelist mechanism
6. Lyrion (LMS) + Squeezelite
7. Music Assistant — the "server + many player providers" maximum
8. tidal-cli — the CLI precedent this project should actually build from
9. The librespot family — crate splits, control surfaces, and what NOT to copy
10. Headless authentication UX — four shapes, and where secrets live
11. Reusable artifacts — the complete copy-or-adapt list

---

## 1. mopidy-tidal — the full server precedent

`ref:mopidy-tidal` · https://github.com/EbbLabs/mopidy-tidal (formerly `tehkillerbee/mopidy-tidal`) ·
Apache-2.0 · v0.3.13 · `requires-python = ">=3.12"` · `Mopidy>=3.0`, `tidalapi>=0.8.10`.

**Architecture.** mopidy-tidal is a *backend* only: it provides library, playlist, search and
playback providers to Mopidy. Everything user-facing comes from other Mopidy extensions. The
project's own nix example lists the realistic set:

```nix
extensionPackages = [ mopidy-local mopidy-iris mopidy-mpd mopidy-mpris ] ++ [ mopidy-tidal ];
settings = { tidal.quality = "LOSSLESS"; tidal.playback_cache = true; };
```

So one backend yields, for free: an MPD server (mopidy-mpd), a web UI (Iris), MPRIS/D-Bus
(mopidy-mpris), and Mopidy's own HTTP/WebSocket JSON-RPC API. **That leverage — one backend, four
control surfaces — is the single most important lesson in this document.**

**Config schema** (`ref:mopidy-tidal/mopidy_tidal/ext.conf`), verbatim:

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

**Headless login, three ways** — see §10 for the shape comparison across all projects. This
project's own three: `BLOCK` (block startup until login), `AUTO` (start unauthenticated, log in
lazily), `HACK` (the login hack, below).

**The login hack, in brief.** While logged out, every library/search provider returns a dummy
Track/Album/Artist whose *title* is the login instruction and whose *cover art URL* is a QR of the
login URL. Any MPD client renders it — no protocol extension, no companion app. Full mechanics and
both undisclosed third-party calls this pattern makes (`api.qrserver.com` for the QR, plus a
`api.voicerss.org` TTS call with a hardcoded key) are owned by
`tidal-oss-landscape/references/project-profiles.md` §5 — cite it rather than restating. Copy the
pattern, not the implementation: `ref:tidalt`'s `qrterminal/v3` approach (§2, §10) generates the
QR locally and has neither problem; streamboat should render its own QR locally, in-band or in a
terminal, rather than reuse this hotlink pattern.

A tiny HTTP server on `login_server_port` (default 8989) serves a page with the login link and, for
PKCE, a form: "Paste the response URL here". It binds `("", port)` — **all interfaces, no
authentication** (`ref:mopidy-tidal/mopidy_tidal/web_auth_server.py`). Copy the idea, not the bind
address. **Undocumented detail worth knowing when picking streamboat's own default**: the config
schema constrains `login_server_port` to `config.Integer(optional=True, choices=range(8000, 9000))`
— any value outside 8000-8999 is rejected at config-load time, not just a convention
(`ref:mopidy-tidal/mopidy_tidal/__init__.py:38-40`). streamboat's own login-server port (if this
shape is copied at all) is not bound by that range, but a value inside it collides with an existing
mopidy-tidal install on the same host.

**Stream resolution** (`ref:mopidy-tidal/mopidy_tidal/playback.py`):

- MPD-manifest tracks: write the manifest to `<cache_dir>/manifest.mpd` and return
  `file://<path>` for GStreamer to consume.
- BTS tracks: return `manifest.get_urls()[0]`.
- Pre-flight quality check: when configured for `hi_res_lossless`, test
  `"HIRES_LOSSLESS" in track.media_metadata_tags` and log a downgrade notice if absent.
- Logs `quality`, `bit_depth`, `sample_rate` (and `codec` for BTS) per track — exactly the telemetry
  a headless user needs.

**The caching proxy, and a legal-posture caveat that matters for streamboat specifically.**
`ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/` is a threaded local HTTP relay in front of
`https://lgf.audio.tidal.com/` (`__init__.py:17-19`) with a SQLite chunk cache (`cache.py`, entries
finalized only on complete download) and `Range` support in `proxy.py` so GStreamer can seek.
`translate_uri` prefers a cached local URL, else `track.get_url()`, else falls back to `as_stream()`
when `URLNotAvailable` is raised (the PKCE case). Default buffer 16 MiB, 1024 entries.

This proxy stores **plain, playable HTTP chunks** of the CDN response in SQLite — a cache directory
full of decrypted FLAC, finalized once a download completes. That is a materially different design
from the one defensible precedent in this space, go-librespot's cache (§9): "only the raw, still-
encrypted files are stored: a cached file is useless without a valid Spotify account, since the
audio key is retrieved again on every playback" (`cache.{enabled,dir,size_limit}`, off by default,
1 GB LRU when on). streamboat is a player for subscribers, not a ripper, and a cache of plain
playable audio reads closer to the latter than the former.
**Recommendation: adopt go-librespot's posture, not mopidy-tidal's** — cache for jitter/seek in
memory or a short-lived buffer, and if a persistent cache is ever added, keep it opaque (encrypted
or otherwise useless without a live, authenticated session).

**A second, separate cache layer exists in mopidy-tidal for *catalogue metadata* — this skill
otherwise only ever discusses the audio cache, leaving browsing and rate-limiting unaddressed even
though the recommended MPD subset (`mpd-and-multiroom.md` §1) explicitly adds browsing.** An MPD
client or web remote renders one screen by requesting dozens of items at once; without a metadata
cache, `lsinfo`/`listplaylistinfo`/`albumart` on a streaming backend become one upstream API call per
item, and an ordinary browsing session hits TIDAL's rate limiter. mopidy-tidal's answer:
`mopidy_tidal/lru_cache.py` — an `LruCache(OrderedDict)`, `max_size=1024` in memory, `persist=True`
to disk, sharded as `<cache_dir>/<obj_type>/<id[:2]>/<key>.cache` (pickled) — instantiated per entity
type as `_artist_cache`/`_album_cache`/`_track_cache`, plus a `PlaylistMetadataCache` and a separate
`LruCache(directory="image")` for cover art (also the answer to the `albumart`-sourcing trap in
`mpd-and-multiroom.md` §1). Rate limiting is handled explicitly, not left to fail loudly: `from
tidalapi.exceptions import ObjectNotFound, TooManyRequests`, caught at `library.py:133` (an image
fetch — log and return empty rather than fail the whole browse) and `library.py:508`
(`logger.warning("TooManyRequests when fetching album tracks: %s", album_id)`) — i.e. degrade to a
partial result, never propagate a hard error. Concurrency is explicitly bounded:
`ThreadPoolExecutor(4, thread_name_prefix="mopidy-tidal-images-")`, and the same cap for search
(`search.py:161`) — four workers, not unbounded fan-out. `playlist_cache_refresh_secs = 0`
(`ext.conf`) is the freshness knob. streamboat needs the equivalent three decisions stated
explicitly: a persistent metadata cache with a size cap and disk layout; a bounded concurrency limit
on catalogue fan-out; and a documented degrade-on-429 policy (partial result plus a
`quality_downgraded`-shaped event from `daemon-architecture.md` §4's vocabulary, never a bare
`ACK`/error). [verified-source `ref:mopidy-tidal/mopidy_tidal/lru_cache.py:18-60`,
`ref:mopidy-tidal/mopidy_tidal/library.py:11,133,147-150,366,508-509`,
`ref:mopidy-tidal/mopidy_tidal/search.py:161`, `ref:mopidy-tidal/mopidy_tidal/ext.conf`]

**Pros**: enormous ecosystem leverage; battle-tested; Apache-2.0 so the ideas are freely copyable.
**Cons**: Python/GStreamer/Mopidy stack streamboat is unlikely to adopt wholesale; coupling to
Mopidy's provider API; a hardcoded CDN hostname that will rot; the login-server bind is insecure;
the caching proxy's legal posture (above).

---

## 2. tidalt — the "one binary, two modes" precedent

`ref:tidalt` · Go 1.26 · Apache-2.0 · BubbleTea TUI · FFmpeg decode + direct ALSA output.

Subcommand surface (`ref:tidalt/cmd/tidalt/main.go`, `daemon.go`, `play.go`):

| Command | Behaviour |
| --- | --- |
| `tidalt` | TUI. **If another instance already owns the MPRIS bus name, it opens in client mode** and forwards commands to the running daemon instead of starting a second player. |
| `tidalt daemon` | Headless: full playback engine + MPRIS2 server, BubbleTea run with `tea.WithoutRenderer()` and `tea.WithInput(nil)`. Prints "No audio device is opened until playback starts." |
| `tidalt play <url>` | Forwards a `tidal://`/`https://tidal.com/…` deep link over D-Bus to the running instance; if none, spawns a terminal emulator running the TUI with the URL queued. Terminal lookup order: `$TERMINAL`, then kitty, ghostty, alacritty, foot, wezterm, konsole, xfce4-terminal, gnome-terminal, xterm. |
| `tidalt setup --daemon` | Writes `~/.config/systemd/user/tidalt.service`, then `systemctl --user daemon-reload / enable / start`, prints status/logs/stop/disable commands. |
| `tidalt logout` | Revokes the token. |

The systemd unit template, verbatim (`ref:tidalt/cmd/tidalt/daemon.go`):

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
`org.mpris.MediaPlayer2.Player.PlayPause`/`.Next`/`.Previous`, and a custom `SendURL` method carries
deep links. Logs go to `~/.local/share/tidalt/play.log` (0600, dir 0700).

**Headless login**: device flow with a QR code rendered in the terminal via
`github.com/mdp/qrterminal/v3`; the verification URL is built as
`"https://" + verificationUri + "?user_code=" + urlencode(userCode)`
(`ref:tidalt/internal/tidal/client.go:87-94`, `loginprint.go:395`). Session is stored in the system
keychain via `docker/secrets-engine`, falling back to an age-encrypted file.

**Limits**: MPRIS is a *scoping choice*, not a ceiling MPRIS itself imposes (see §9 for the
correction to this) — tidalt's client mode is a TUI that re-authenticates and does its own API
calls, using MPRIS only for transport control, because it implements only Root+Player. That is a
real boundary in tidalt specifically, but not proof MPRIS "cannot express" more.

**ALSA output detail** (`ref:tidalt/internal/player/alsa.c`): opens `hw:` devices and negotiates
formats with `snd_pcm_hw_params`. Full recipe (format-preference orders, period-before-buffer
ordering, `ReserveDevice1` reservation with release-on-pause, format-refusal-vs-EBUSY handling) is
owned by `audio-pipeline/references/output-backends.md` §1-2 — cite it rather than restating (this
is the same recipe pointed at from `raspberry-pi-deployment.md` §1 of this skill). One thing worth
keeping here specifically: device reservation is paired with the daemon's "no device opened until
playback starts" policy.

---

## 3. Sone — local HTTP control done carefully

`ref:sone` (Tauri 2 + Rust) is a GUI app, but it ships two local servers worth copying:

- **MCP server**: `axum` + `rmcp 1.7.0` (`StreamableHttpService`, stateful mode off, JSON
  responses). Binds **`127.0.0.1` only** — `let addr: SocketAddr = ([127, 0, 0, 1], port).into();`
  — default port 5577, and the entire API lives under a path segment that is a random token:
  `format!("/{}/mcp", token)`. The token is a UUID v4 generated on first enable and persisted to
  settings (`ref:sone/src-tauri/src/mcp/mod.rs:30-37`, `mcp/server.rs:70-85`). Disabled by default
  (`mcp_enabled: false`).
- **OBS overlay server**: routes `GET /overlay`, `/overlay/state`, `/overlay/theme.css`,
  `/overlay/events` (SSE), default port 5578, host configurable including `0.0.0.0`, with a
  `Semaphore(MAX_SSE_CONNECTIONS)` limiting concurrent SSE clients and code that displays
  `127.0.0.1` when bound to the wildcard (`ref:sone/src-tauri/src/overlay/server.rs`).

**This is most of the security model streamboat should adopt for its control API — with one
qualification.** Loopback by default, opt-in LAN bind, a generated token, an explicit connection
cap: adopt all of that. But Sone's specific choice of carrying the token *in the URL path* is sound
for a machine-to-machine client (an MCP client, a script) and weak for a human-facing surface — a
URL-path token opened in a phone browser lands in browser history, a shared-phone address bar, any
`Referer` a third-party asset sends, and reverse-proxy logs. **Recommendation**: use
`Authorization: Bearer <token>` for API clients, and for the web remote issue a one-time pairing URL
that immediately exchanges the token for an `HttpOnly`, `SameSite` cookie scoped to the daemon's
origin — never a token that persists in the address bar. Also add `allow_origin` and optional
`cert_file`/`key_file` to the control-API config schema from day one (go-librespot's
`server.{address,port,allow_origin,cert_file,key_file}` is the model — there is no `server.tls` key;
TLS is configured by supplying `cert_file`+`key_file`, §9).

MPRIS in Sone is `mpris-server = "0.9"` over `zbus = "5"`
(`ref:sone/src-tauri/Cargo.toml:53,66`); Sone-windows adds `souvlaki = "0.8.3"` alongside
`mpris-server` for Windows SMTC (`ref:sone-windows/src-tauri/Cargo.toml:58,64`) — Linux MPRIS in
Sone itself is `mpris-server`+`zbus`, not `souvlaki`.

---

## 4. tidal-hifi — a documented HTTP control API

`ref:tidal-hifi/src/features/api/swagger.json` (OpenAPI 3.1.0, `"version": "8.1.3"`) documents a
complete small player API. Current (non-deprecated) surface:

```
POST /player/play            POST /player/pause          POST /player/playpause
POST /player/next            POST /player/previous
POST /player/shuffle/toggle  POST /player/repeat/toggle
POST /player/favorite/toggle
PUT  /player/volume          # 0.0–1.0, minimum 0, maximum 1, multipleOf 0.01
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
existing user scripts and Stream Deck integrations port over.

**Copy the naming; explicitly reject the security posture.** `ref:tidal-hifi/src/scripts/
settingsStore.ts:44-47` sets `api: true` — this API is **enabled by default** — with
`apiSettings: { port: 47836, hostname: "127.0.0.1" }`, and `ref:tidal-hifi/src/features/api/index.ts`
(49 lines) has **no authentication of any kind** (grep for `auth`/`token`/`bearer`/`password`: no
matches). streamboat's own control API (`daemon-architecture.md` §2-3) must ship disabled by
default, or token-required from first boot — never open-and-unauthenticated the way tidal-hifi
ships. [verified-source `ref:tidal-hifi/src/scripts/settingsStore.ts:44-47`,
`ref:tidal-hifi/src/features/api/index.ts`]

---

## 5. upmpdcli — TIDAL over UPnP, and the renderer-whitelist mechanism

upmpdcli implements a UPnP/OpenHome Media Renderer on top of MPD, and separately a Media Server with
Python plugins that gateway external services — including TIDAL, built on python-tidal. GioF71
contributed a rewritten TIDAL plugin. **Treat any specific plugin version as a dated snapshot, not a
fact to build against** — the version table read `release|tidal|0.8.12` and
`master|tidal|0.8.16`/`edge|tidal|0.8.16` as of 2026-09, already superseding an earlier
`0.8.3`/2025-04-07 pairing found during research. Configuration is by environment variables
including `TIDAL_AUDIO_QUALITY` and pre-generated tokens (`TIDAL_TOKEN_TYPE`, `TIDAL_ACCESS_TOKEN`,
`TIDAL_REFRESH_TOKEN`, `TIDAL_EXPIRY_TIME`). **The quality enum is `LOW|HIGH|LOSSLESS|HI_RES_LOSSLESS`**
(default `LOSSLESS`) — not `LOW|HIGH|HI_RES|HI_RES_LOSSLESS`, a mistake worth flagging explicitly
since streamboat's own quality-tier names are modelled on ones exactly like these and `HI_RES` is a
real but *different* TIDAL tier name elsewhere. [verified-source:
https://raw.githubusercontent.com/GioF71/upmpdcli-docker/main/README.md:282]

The interesting failure mode: in `HI_RES_LOSSLESS` mode the plugin serves DASH manifests, and **most
UPnP renderers cannot play a manifest**. GioF71's answer is a *whitelist* — MPD+upmpdcli,
gmrender-resurrect, and some WiiM Pro/Pro Plus devices (on `master`/`edge` images) get hi-res;
everything else is served 16/44.1 for compatibility. **The gate is keyed on User-Agent, and it has a
documented off switch**: the environment variable is `TIDAL_ENABLE_USER_AGENT_WHITELIST`; setting it
to `no` serves hi-res to any renderer, which the maintainer explicitly recommends as a way to
discover additional compliant devices worth whitelisting. That is the difference between a
hardcoded device list and a testable policy — copy the *policy shape*, not just the list of known-
good renderers.

**Lesson for streamboat**: if streamboat ever exposes a UPnP renderer or serves streams to
third-party devices, the DASH-manifest-vs-plain-URL split becomes a device compatibility matrix.
Prefer to decode locally and emit PCM (the Snapcast model, `mpd-and-multiroom.md` §2) over shipping
manifests to foreign renderers.

---

## 6. Lyrion (LMS) + Squeezelite

`michaelherger/lms-plugin-tidal` integrates TIDAL with Lyrion Music Server (ex-Logitech Media
Server), playing to Squeezebox hardware and Squeezelite software players. `ref:tidal-connect/README.md`
(2024) states it supports LOSSLESS but not TIDAL HiRes. **Hi-res FLAC status is genuinely mixed, not
simply "an ongoing problem"**: issue #35 ("Hi-Res: use of FLAC/MQA") is **closed**, #89 ("can't play
24 bit files") is **open**; #88 "Hires FLAC" (closed), #98 "only cd quallity no HiRes" (closed),
#100 "Attempt at Hires Lossless (Max) and Dolby Atmos support" (closed — suggesting hi-res work has
since landed), #113 "forced atomos stream for some tracks" (open). Keep this flagged as genuinely
uncertain rather than stating either "broken" or "fixed". [unverified beyond the issue tracker,
2026 state]

Relevance: mostly as a warning that hi-res + a legacy streaming ecosystem is where these projects
break, and as evidence that a *server with many thin players* is a durable architecture in this
space.

---

## 7. Music Assistant — the "server + many player providers" maximum

Python server, official distribution via a Home Assistant add-on or Docker, dev server at
`http://localhost:8095`, bundling ffmpeg 6.1+ (not published to PyPI as a Python package). Music
providers include TIDAL; player providers include Sonos, Chromecast, AirPlay, Squeezelite, DLNA,
Snapcast and, since 2.8 (March 2026), Sendspin including "Sendspin Bridges" that wrap existing
AirPlay/Cast devices so they can join a synchronized Sendspin group (`mpd-and-multiroom.md` §4).
**The built-in snapserver's control port is 1705** (JSON-RPC, `127.0.0.1` by default —
`DEFAULT_SNAPSERVER_IP`/`DEFAULT_SNAPSERVER_PORT` in
`music_assistant/providers/snapcast/constants.py`), not 1780 — 1780 is Snapcast's own HTTP/Snapweb
port (1788 for SSL), a different service entirely (`mpd-and-multiroom.md` §2).

This is the maximal version of the architecture streamboat is choosing among. It is also a warning:
Music Assistant's value is breadth of *player* support, which is a large, permanent maintenance
surface. streamboat should not compete there; it should be a good citizen of it (feedable *by*
Snapcast/Sendspin, not a reimplementation of them).

---

## 8. tidal-cli — the CLI precedent this project should actually build from

`ref:tidal-cli` (`@lucaperret/tidal-cli` v1.2.5, MIT, Node ≥20, built on `commander`) sat unopened
in the reference set through an entire research pass despite the owner's brief specifically asking
for a CLI mode "now" — don't repeat that miss.

**Full command surface**: `auth` | `search {artist,album,track,video,playlist,suggest,editorial,
history,history-delete,history-clear}` | `artist {info,tracks,albums,similar,radio}` |
`album {info,barcode}` | `track {info,similar,isrc,radio}` | `playlist {list,create,rename,delete,
add-track,remove-track,add-album,move-track,set-description}` | `library {add,remove,
favorite-playlists,add-playlist,remove-playlist}` | `recommend --type {daily,discovery,new-release,
offline}` | `mix items` | `history {tracks,albums,artists}` | `saved {list,add,remove}` |
`share {track,album}` | `user profile` | `playback {info,url,play}`.

**Copyable conventions**: one global `--json` flag placed before the subcommand
(`tidal-cli --json search track "…"`), human-readable output by default, `exit(2)` for an invalid
argument value vs. `exit(1)` for a runtime error.

**What NOT to copy**: `playbackPlay` downloads the *entire* track (or every DASH segment) to
`os.tmpdir()` and shells out to `mpv --no-video`/`afplay`/`start`, deleting the file on SIGINT — no
streaming, no gapless, no seek. This is the anti-pattern streamboat's own `streamboat play` must
avoid; stream through `streamboat-core`'s player engine instead.

**A fact missed elsewhere in this project's research**: this CLI takes playback data from the
*official* `developer.tidal.com` API v2 — `GET /trackManifests/{id}` with `{adaptive:false,
formats:[…], manifestType:'MPEG_DASH', uriScheme:'DATA', usage:'PLAYBACK'}` returning a base64
data-URI manifest plus `trackAudioNormalizationData {replayGain,peakAmplitude}` and
`albumAudioNormalizationData` in one call — a second, officially sanctioned manifest path worth
comparing against the unofficial-API approach documented in the `tidal-api` skill. **`formats` is a
quality-dependent cascade, not the fixed 4-element array `[HEAACV1,AACLC,FLAC,FLAC_HIRES]`** — that
array is only the `HI_RES` branch. `ref:tidal-cli/src/playback.ts:44-58` builds
`formats: qualityToFormats[quality] ?? qualityToFormats.HIGH` from `src/playback.ts:12-15`:
`LOW: ['HEAACV1']`, `HIGH: ['HEAACV1','AACLC']`, `LOSSLESS: ['HEAACV1','AACLC','FLAC']`,
`HI_RES: ['HEAACV1','AACLC','FLAC','FLAC_HIRES']` — one to four values depending on requested
quality. Model streamboat's own quality→format mapping as an explicit per-tier list, not a single
constant array, if this endpoint is ever used as a reference. [refuted-and-corrected, verified-source
`ref:tidal-cli/src/playback.ts:12-15,44-58`]

**The CLI is specified only as a same-host client throughout this skill — there is no
`--host`/remote-daemon story, and no answer for scripting a daemon on a different box.** The recommended
staged path (`SKILL.md`) sells "the desktop GUI grows a play-on… picker listing local daemons", but
the CLI — the thing an SSH user actually has on a Pi's *other* machine — is only ever described as
"try the running local instance, else spawn one" (tidalt's model, §2). An operator managing three Pis
has no documented way to point the CLI at one of them. Fold in a concrete surface: `streamboat
--daemon <host[:port]|unix:<path>|auto>` (default `auto` = local Unix socket, then local loopback
HTTP, then fail with the discovery list); `streamboat daemons` to print the mDNS browse result (name,
address, port, `v=`, `auth=`, paired-or-not, from the TXT-key design in `daemon-architecture.md` §2);
and `streamboat pair <name>` to run the pairing exchange from the terminal. Precedent for the pieces:
ncspot's `ncspot info` prints its socket location so scripts can find it (§9, cited there for the
socket itself, not for this); go-librespot's `GET /auth/code` (§10) is the pattern for surfacing an
in-progress auth state to any frontend, including a remote CLI; and the `--json`/exit-code
conventions already adopted below should extend to these new subcommands too. Also decide whether a
remote daemon's token is stored per-host in the CLI's own config — the first thing a multi-Pi user
hits.

**A time-sync tell worth knowing about independent of the CLI itself**: `ref:tidal-cli/src/index.ts`
opens by monkey-patching `console.warn` solely to "Suppress 'TrueTime is not yet synchronized'
warnings from `@tidal-music/auth`" — the *official* auth package ships a TrueTime service and
complains until it syncs. This is corroborating evidence for the time-sync requirement in
`raspberry-pi-deployment.md` §3.

---

## 9. The librespot family — crate splits, control surfaces, and what NOT to copy

| Project | Language | Control surfaces | Discovery / handoff | Notes |
| --- | --- | --- | --- | --- |
| **librespot** | Rust | Library **and** binary; the binary registers as a Spotify Connect receiver | Zeroconf (Spotify Connect) | Reused as a linked **crate** by ncspot and spotifyd; consumed as a **subprocess** by Snapcast ("launches librespot and reads audio from stdout" — a spawned process reading a pipe, the same integration shape this project recommends for streamboat at stage 0, not the library-linking pattern). |
| **spotifyd** | Rust (on librespot) | `rs.spotifyd.Controls` D-Bus interface on the `rs.spotifyd.instance$PID` bus name — **always present**, exposing `TransferPlayback`/`VolumeUp`/`VolumeDown` even when not the active device; MPRIS (`org.mpris.MediaPlayer2.spotifyd.instance$PID`) — **only once spotifyd becomes the active playback device**; `--onevent` hook; `--dbus-type system` for headless boxes with no session bus; optional Secret Service keyring | Spotify Connect | Docs reachable at `raw.githubusercontent.com/Spotifyd/spotifyd/master/docs/src/advanced/{dbus,mpris,hooks}.md` even though `docs.spotifyd.rs` itself is blocked. |
| **go-librespot** | Go | REST API + WebSocket **`/events`** with a closed vocabulary: `active`, `inactive`, `metadata`, `will_play`, `playing`, `not_playing`, `paused`, `stopped`, `seek{position}`, `volume{value,max}`, `shuffle_context`, `repeat_context`, `repeat_track`; REST half published as `api-spec.yml`; MPRIS; `GET /auth/code` during device-auth, served "for as long as the daemon is waiting, so a frontend can show them instead of asking the user to read the logs" | `zeroconf_backend` = `builtin` or `avahi`, `zeroconf_enabled`, `zeroconf_port` | `audio_backend` (alsa, pipe, pulseaudio, audio-toolbox, wasapi), `bitrate`, `volume_steps`, normalisation, `crossfade_duration`, `server.{address,port,allow_origin,cert_file,key_file}` (**no `tls` key** — TLS is `cert_file`+`key_file`), `cache.{enabled,dir,size_limit}` (only the still-encrypted file is cached, §1). librespot-java development retired in its favour. |
| **ncspot** | Rust | TUI; embeds librespot as a library; **also** a Unix-domain socket at the platform runtime directory — **Linux/macOS/\*BSD only, no Windows equivalent** (correction below) — plain command words in (`play`, `playpause`), newline-delimited JSON status out after every state change, e.g. `{"mode":{"Playing":{...}},"playable":{"type":"Track","id":…,"title":…,"duration":184132,"artists":[…],"cover_url":…}}`. `ncspot info` prints the socket location. | — | Proof a TUI/GUI and a daemon share one engine crate, **and** proof a same-host control surface doesn't need HTTP at all — documented uses include controlling a detached tmux session, status bars, and startup scripts. **Recommend this as streamboat's own v0 control surface** for the CLI and same-host GUI on Linux/macOS. **Correction (second fact-check pass)**: an earlier draft said "Windows: named pipe" — false. ncspot's own docs state the socket exists "on UNIX platforms (Linux, macOS, \*BSD)" only, and `src/ipc.rs` imports only `tokio::net::{UnixListener, UnixStream}` with no Windows named-pipe dependency in `Cargo.toml`. Rust's `interprocess` crate genuinely covers both Unix sockets and Windows named pipes, but there is **no reference implementation for the Windows half anywhere in this project's source set** — test it explicitly, don't assume parity. |
| **psst** | Rust | `psst-core` ("Spotify TCP session, audio file retrieval, decoding, audio output, playback queue") + `psst-gui`, a design *inspired by* librespot rather than depending on it | — | A second real-world answer to "how do you split the core", alongside ncspot's "just link the crate". |
| **Roon** | proprietary | Core/Remote/Bridge three-tier; endpoints implement RAAT ("Roon Advanced Audio Transport"), up to 32-bit/768 kHz PCM and DSD512; DAC vendors must implement Roon's Endpoint Code + RAAT to be "Roon Ready" | proprietary | The commercial version of the same idea; strictly closed — RAAT is unavailable to open-source implementers (confirmed via a Music Assistant maintainer discussion; `help.roonlabs.com` itself is blocked from this environment). |

**librespot's own internal shape is worth copying, not just its existence.** librespot 0.8.0 (Rust,
edition 2024, MIT) is a **workspace of separate crates**: `core`, `audio`, `playback`, `metadata`,
`protocol`, `oauth`, `discovery`, `connect`, plus the top-level binary. OAuth and zeroconf discovery
are their own crates; the Connect-receiver logic lives in its own `connect` crate rather than inside
the player — a real boundary answer for streamboat's own "split `streamboat-core` out first"
decision (`daemon-architecture.md` §1). Default features are `native-tls`, `rodio-backend`,
`with-libmdns`, with a documented `rustls` alternative "for avoiding external OpenSSL dependencies,
reproducible builds, or when targeting platforms where native TLS dependencies are unavailable or
problematic (musl, embedded, static linking)" — directly relevant to a static Pi binary
(`raspberry-pi-deployment.md` §2).

**The control-surface pattern splits into three, not two.** *Transport-only IPC* (MPRIS/D-Bus) is
**not** limited to "transport + metadata only" — see `daemon-architecture.md` §2 for the
`TrackList`/`Playlists` correction. *Same-host, no-listener IPC* (Unix domain socket/named pipe) —
ncspot's precedent above — is trivial, needs no port/token/TLS decision, and is the missing v0
surface for the CLI and same-host GUI. *Rich IPC* (HTTP+WebSocket JSON) expresses everything and
needs auth. streamboat needs all three.

---

## 10. Headless authentication UX — four shapes, and where secrets live

The device-code (RFC 8628) flow is the natural headless login and every precedent uses it:

1. **Terminal + QR** — `ref:tidalt`: build `https://<verificationUri>?user_code=<userCode>`, render
   with `qrterminal/v3` at error-correction level M, print the URL and code too. Generated locally.
2. **Local web page** — `ref:mopidy-tidal/mopidy_tidal/web_auth_server.py`: "TIDAL Web Auth", "KEEP
   THIS TAB OPEN", and for PKCE a form to paste the full redirect URL. Port 8989 by default; **bind
   loopback, unlike the original** (its `("", port)` bind is the anti-pattern to avoid).
3. **In-band prompt** — mopidy-tidal's login hack (§1): show the login instruction *as content*
   inside whatever client the user already has open. Applies directly to an MPD-compatible mode —
   but generate the QR locally (§1's caveat), don't hotlink a third-party service.
4. **Pre-provisioned tokens** — upmpdcli's docker images accept `TIDAL_TOKEN_TYPE`,
   `TIDAL_ACCESS_TOKEN`, `TIDAL_REFRESH_TOKEN`, `TIDAL_EXPIRY_TIME` as environment variables so a
   token generated on a desktop can be dropped into a headless box.

**Secret storage on a headless box is the hard part.** A Linux server has no unlocked keyring.
Per-OS mechanism table and the keyring-with-encrypted-file-fallback recommendation are owned by
`streamboat-engineering-baseline/references/secrets-and-tokens.md` §2-3 — cite it rather than
restating; the headless-specific consequence is that a headless box always falls through to the
encrypted-file path (no keyring available at all), so that fallback is not optional there, it is
the primary mechanism.

**Pairing a phone/GUI to a headless daemon with no screen at all** is a related but separate problem
— see `daemon-architecture.md` §5 for the full assembly (go-librespot's `GET /auth/code`, Sendspin's
pairing-token/pairing-code design, **librespot's own zeroconf pairing protocol** — a fifth,
MIT-licensed and already-shipped shape this table's librespot row above does not otherwise mention —
and the owner decisions this still needs).

---

## 11. Reusable artifacts — the complete copy-or-adapt list

| Artifact | Source | Use |
| --- | --- | --- |
| ALSA card-name → index resolution + generated `asound.conf` | `ref:tidal-connect/bin/common.sh` | daemon startup on Pi/servers |
| Softvol creation with `Master`/`SoftMaster` detection | `ref:tidal-connect/bin/common.sh` | volume on hardware without a mixer |
| Pre-flight test tone before opening the device (opt-out via `ENABLE_GENERATED_TONE=no`) | `ref:tidal-connect/bin/entrypoint.sh` | catch locked/misconfigured devices without forcing a click on every open |
| 26 per-DAC `asound.conf` presets + tested-device table | `ref:tidal-connect/userconfig/`, `assets/known-devices.md` | a device compatibility database |
| `systemd --user` unit template + installer, plus `After=time-sync.target` | `ref:tidalt/cmd/tidalt/daemon.go` | Starting point only for a **desktop** `systemd --user` install — the template is `graphical-session.target`-bound with no `RuntimeDirectory`/`StateDirectory`/hardening; for a true headless system unit, build from `daemon-architecture.md` §1's specification instead |
| Token-refresh callback hook (`on_authz_refresh_callback`) in a maintained Rust TIDAL client crate | `ref:tidalrs/src/lib.rs:271-281,321,401` | evaluate as a reference implementation for `streamboat-core`'s refresh loop and the multi-process token-refresh problem (`daemon-architecture.md` §5) before writing one from scratch |
| Single-instance + client-mode fallback via D-Bus name ownership | `ref:tidalt/internal/mpris/server.go`, `cmd/tidalt/main.go` | one binary, two roles |
| Deep-link forwarding with terminal fallback | `ref:tidalt/cmd/tidalt/play.go` | `streamboat play <url>` |
| Unix-domain-socket NDJSON control surface | `hrkfdn/ncspot` `doc/users.md` (no local checkout — see `sources.md`) | `streamboat status`/CLI/same-host GUI, v0 |
| Terminal QR device-code login (generated locally) | `ref:tidalt/internal/tidal/loginprint.go` | headless login |
| Local login page + PKCE paste form | `ref:mopidy-tidal/mopidy_tidal/web_auth_server.py` | headless login (bind loopback, unlike the original) |
| "Login hack": login prompt rendered as catalogue content | `ref:mopidy-tidal/mopidy_tidal/login_hack.py` | login inside an MPD client (render the QR locally, not via `api.qrserver.com`) |
| Encrypted/opaque audio cache (still-encrypted file, key re-fetched on play) | go-librespot `cache.{enabled,dir,size_limit}` | seeking + jitter smoothing without a plain-audio cache directory |
| Pre-flight `HIRES_LOSSLESS in media_metadata_tags` check | `ref:mopidy-tidal/mopidy_tidal/playback.py` | honest quality reporting |
| Loopback + token + SSE-cap local server (Bearer for API, cookie for browser) | `ref:sone/src-tauri/src/mcp/server.rs`, `overlay/server.rs` | the control API's security shape |
| Player HTTP vocabulary | `ref:tidal-hifi/src/features/api/swagger.json` | API naming |
| Streaming-privileges WebSocket handling | `ref:tidal-sdk-web/packages/player/src/internal/services/pushkin.ts` | core (`daemon-architecture.md` §6) |
| CLI noun-verb surface, `--json` flag, exit-code convention (not its temp-file playback) | `ref:tidal-cli/README.md`, `src/index.ts` | `streamboat`'s CLI subcommands |
| Snapcast stream-plugin (JSON-RPC over stdin/stdout) | `badaix/snapcast` `doc/json_rpc_api/stream_plugin.md` | `streamboat snapcast-plugin` (`mpd-and-multiroom.md` §2) |
| Sendspin pairing design (Noise `KKpsk2`, QR pairing token, Sentinel PSK) | `Sendspin/spec` README | streamboat's own pairing flow (`mpd-and-multiroom.md` §4, `daemon-architecture.md` §5) |
