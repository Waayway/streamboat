# streamboat

An open-source TIDAL client for people who already pay for TIDAL: a desktop app
for Linux, Windows and macOS, and a headless daemon for a Raspberry Pi or a
server, built in Rust. It talks to the same unofficial `api.tidal.com` API that
High Tide and Sone use, so it needs your own active subscription and one of the
client ids that ecosystem uses.

streamboat is not affiliated with or endorsed by TIDAL. It is a player, not a
downloader: it never decrypts streams, never uses TIDAL's licensed offline mode,
and never produces a playable file that outlives your subscription.

## Status

The first milestone is in: a **playable spike** from the command line. It logs
in with the device-code flow, resolves a track's manifest through the quality
cascade, and plays it to the end through GStreamer on Linux, with gapless
hand-over to the next track and a selectable ALSA device.

The desktop shell now exists too: `streamboat` with no subcommand opens an
iced 0.14 window — Login (device-code or PKCE), Home and Explore (TIDAL's own
server-driven feed sections, with a graceful card for a section type this
client doesn't recognise yet), Search, Now Playing with a reorderable queue,
a persistent playback bar, the signal-path panel, and Settings. Every CLI
subcommand keeps working exactly as before.

It is a multi-window app now (`iced::daemon`, D-036): the playback bar's
"Mini player" button or Ctrl+M opens a small always-on-top window (art,
title/artists, transport, a seek bar, the quality badge, a restore button)
over the same playback state as the main window. Closing the main window
hides it rather than quitting (D-014) — the lock and, once playing, the
audio device stay held; a tray icon (StatusNotifierItem via `ksni` on Linux,
`tray-icon` on Windows/macOS, the latter compile-checked only so far, no
runner for it yet) offers Show/Hide/Play-Pause/Next/Previous/Quit and a
"now playing" tooltip; Ctrl+Q or the tray's Quit item is the only way out,
and it waits briefly for playback to actually stop first. Only one instance
runs the engine at a time (D-010): a second `streamboat` either becomes a
plain remote client of a `streamboatd` that is already running, or, if
another `streamboat` window already holds the lock, just asks it to come to
the front and exits. A startup decoder probe (D-003) checks which quality
tiers this build can actually decode and caps the requested ceiling (with a
warning) rather than failing mid-playback; greying out an unreachable tier
in the Settings picker itself is still to come. Entity/album/artist/playlist
pages are the next wave; the libmpv backend for Windows and macOS is being
built alongside this. Everything that was decided about the product and the
stack is in `docs/DECISIONS.md`.

## Build (Linux)

```
sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  libgstreamer-plugins-bad1.0-dev gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
  gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-alsa libasound2-dev pkg-config \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev \
  libxcursor-dev
cargo build --release
```

The second line of packages is for iced/winit (the desktop shell); skip it for a
`streamboatd`-only build (`cargo build -p streamboat-server --release`, no GUI
libraries linked).

Binaries: `target/release/streamboat` (desktop shell + CLI subcommands) and
`target/release/streamboatd` (daemon).

GStreamer 1.26.10 or newer is needed for TIDAL's FLAC-in-DASH streams through
the modern demuxer; on older versions streamboat demotes `dashdemux2` at
startup so the legacy element handles them. Release packages will bundle a
pinned GStreamer (D-020).

## Client credentials

No client id ships in this repository (D-023). Two pairs exist in the
open-source ecosystem: a **device-code** pair (headless/CLI login) and a
**PKCE** pair (browser login; the only one TIDAL serves hi-res to in the
clear). streamboat takes whichever you give it, in this order:

1. `client_id` / `client_secret` and `pkce_client_id` / `pkce_client_secret`
   in `settings.json` (see `streamboat paths`);
2. `STREAMBOAT_CLIENT_ID` / `STREAMBOAT_CLIENT_SECRET` and
   `STREAMBOAT_PKCE_CLIENT_ID` / `STREAMBOAT_PKCE_CLIENT_SECRET` in the
   environment;
3. the same variables set when the binary was built (an embedded default).

The ids in use across the ecosystem were extracted from TIDAL's own
applications; any id you embed in a binary is extractable, and TIDAL can
revoke one at any time. Hi-res (`HI_RES_LOSSLESS`) is only served in the clear
to a client id that has a secret, on a PKCE session; otherwise streamboat drops
the hi-res tiers from the cascade and says so.

## Logging in

```
streamboat login                 # device-code flow: a link.tidal.com URL plus a code
streamboat login --pkce          # browser flow; paste the URL you land on afterwards
streamboat login --pkce --capture loopback --port 17893
```

`--pkce` opens `login.tidal.com`; after you log in, the browser lands on
`https://tidal.com/android/login/auth?code=...`, a TIDAL page that says
"Oops". Copy that URL from the address bar and paste it into the terminal.
The loopback capture (`--capture loopback`) instead asks TIDAL to redirect to
`http://127.0.0.1:<port>/callback`; whether TIDAL accepts that for the
ecosystem PKCE client id is unverified, so paste is the default.

## Use

```
streamboat                               # opens the desktop shell (Login if not signed in)
```

Or from the CLI:

```
export STREAMBOAT_CLIENT_ID=...          # and STREAMBOAT_CLIENT_SECRET=... for hi-res
streamboat login                         # opens link.tidal.com with a code
streamboat search "blue in green"
streamboat resolve 12345                 # what would play, at which quality
streamboat play 12345 67890              # shared output (PipeWire/PulseAudio)
streamboat devices                       # find hw:N,0
streamboat play 12345 --device hw:1,0 --exclusive   # bit-perfect: no mixing, no volume
```

`streamboat play --search "..."` plays the first search result. `RUST_LOG=info`
prints the signal path (decoder, sink, device format) when a track starts.

`streamboatd` hosts an HTTP + WebSocket control API by default, on
`127.0.0.1:4747`: `GET /health` (no token needed), `GET /v1/state`,
`POST /v1/commands` (a `Command` JSON body), and `GET /v1/events` (a
WebSocket: a full state snapshot on connect, then every `Event` wrapped as
`{"revision": n, "event": {...}}`, also accepting inbound `Command` JSON).
Every route but `/health` needs `Authorization: Bearer <token>` — the token
is generated on first start into a 0600 file under the data directory and
its path is printed at startup. `--listen 0.0.0.0:<port>` or `--lan` opts
into exposing it beyond localhost (logged loudly); `streamboatd --stdio`
keeps the original JSON-lines transport on stdin/stdout
(`{"type":"play","items":[{"track_id":12345}]}`). On Linux, `streamboatd`
(and, when it is the one running the engine, the `streamboat` desktop shell
itself) registers an MPRIS2 player (`org.mpris.MediaPlayer2.streamboat`) for
media keys and desktop "now playing" widgets, when a D-Bus session is
reachable.

Tokens are stored encrypted (AES-256-GCM) in the data directory. The 32-byte
master key goes to the OS keyring (Secret Service, Credential Manager,
Keychain) when one is reachable at first login, else to a 0600 key file in the
config directory; `STREAMBOAT_MASTER_KEY` (hex or base64, 32 bytes) supplies it
on a headless box. The key never moves on its own: `streamboat keyring status`
shows where it is, `streamboat keyring migrate` moves a file key into the
keyring, `streamboat keyring to-file` does the reverse. `key_storage` in
`settings.json` (`auto`, `keyring`, `file`) fixes the policy.

## Layout

```
crates/streamboat-core      Apache-2.0  API client, login, token store, manifests, protocol types
crates/streamboat-player    GPL-3.0     Engine trait, GStreamer and libmpv backends, ALSA writer, queue, playback, MPRIS
crates/streamboat-server    GPL-3.0     streamboatd: HTTP+WS control API, stdio fallback
crates/streamboat-desktop   GPL-3.0     streamboat (iced desktop shell + CLI subcommands)
docs/DECISIONS.md                       what was decided and why
docs/research/                          the fact-checked research the decisions rest on
.claude/skills/                         repo knowledge for coding agents
```

## Play reporting

By default, streamboat reports a track to TIDAL as "played" once you have
listened to more than 30 seconds of it, the same way TIDAL's own apps do, so
Recently Played and Home personalisation behave the way you'd expect (D-027).
This is disclosed, not hidden: it never fires for a preview clip, it is
dropped rather than retried if TIDAL's backend rejects it as malformed, and
its timestamps come from TIDAL's own clock (`GET /v1/ping`), not your
machine's. What it says about the *device* sending it follows the credential
your session is actually using: with your own client id it identifies itself
honestly as streamboat; only with the community's shared "ecosystem" client id
(D-023) does it describe that credential's real identity, TIDAL's Android
client, the way Sone's does. Set `play_reporting: false` in `settings.json` to
turn it off entirely. See `crates/streamboat-core/src/reporting.rs` for
exactly what is sent.

Streaming privileges (TIDAL allows one playing device per account at a time)
and scrobbling to Last.fm/ListenBrainz (D-037, both off until you supply
credentials in `settings.json`) are built the same way; neither is wired into
the CLI spike yet outside `streamboatd`.

## Legal

streamboat uses an API TIDAL does not document for third parties, under your
own subscription. Read TIDAL's terms yourself before using it; the project's
posture, and how comparable clients position themselves, is recorded in
`docs/research/tidal-api.md` and `.claude/skills/tidal-api/references/legal-and-landscape.md`.
TIDAL Connect is out of scope in both directions, permanently (D-035).

## Licence

`streamboat-core` is Apache-2.0; the other crates are GPL-3.0-only. See
`LICENSES/`.
