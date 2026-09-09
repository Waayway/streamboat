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

**Everything in the decided first release (D-001) is built, on Linux end to
end and on Windows/macOS as compiled-but-unrun code** — see
`docs/architecture.md` for the full inventory and the honest list of what
could not be verified without a display, a DAC, a session bus, a snapserver,
or a Windows/macOS machine.

- **Playback**: login (device code or PKCE), the quality cascade over all four
  tiers, gapless playback through GStreamer on Linux and libmpv on
  Windows/macOS behind one engine trait, exclusive/bit-perfect output (the
  project's own ALSA writer on Linux; WASAPI/CoreAudio through libmpv), a
  startup decoder probe that caps unreachable tiers, ReplayGain off/album/
  track, and a signal-path panel that reports only what the engine observes.
- **Desktop shell** (`streamboat` with no subcommand): an iced 0.14
  multi-window app — Login, Home and Explore rendered from TIDAL's own
  server-driven feed, Search, Album/Artist/Playlist/Mix/Track/Video pages,
  My Collection (favourites, playlists, folders), playlist editing with ETag
  preconditions, synced lyrics, Now Playing with a reorderable queue, a
  persistent playback bar, a per-track action row, a notification banner, a
  floating mini-player, a tray icon (close hides, Quit is explicit), Settings,
  and deep links (`tidal.com`/`listen.tidal.com`/`tidal://`/`streamboat://`
  pasted into Search, `streamboat open <url>`, or `streamboat <url>`). Only
  one instance runs the engine: a second `streamboat` becomes a remote client
  of a running `streamboatd`, or asks the running window to come forward.
- **Daemon** (`streamboatd`): the HTTP + WebSocket control API on
  `127.0.0.1:4747` with a generated token and Host allowlist (`--stdio` keeps
  the JSON-lines transport), MPRIS on Linux (SMTC/NowPlaying adapters on
  Windows/macOS), streaming privileges, play reporting and scrobbling
  (Last.fm, ListenBrainz), the pinned encrypted offline cache, Snapcast
  output with `streamboat snapcast-plugin`/`snapcast-discover`, systemd units
  and a Docker image.
- **Ops**: redacted rotating logs, local crash reports, `streamboat
  debug-bundle`, and packaging for every decided channel (`packaging/`).

Everything that was decided about the product and the stack is in
`docs/DECISIONS.md`.

## Install

Packaged builds exist for the first release (D-041); see `packaging/README.md`
for what each artifact is and what could and couldn't be verified without a
Windows or macOS machine. No channel here auto-updates (D-043) and none of
these binaries are code-signed (D-042) -- see the workaround note below.

- **Linux**
  - **AUR**: `streamboat` (builds from source against Arch's own system
    GStreamer) or `streamboat-bin` (prebuilt) -- `packaging/aur/`.
  - **deb/rpm**: built from `[package.metadata.deb]`/
    `[package.metadata.generate-rpm]` in `crates/streamboat-desktop/Cargo.toml`
    and `crates/streamboat-server/Cargo.toml`. This first release depends on
    the distro's own GStreamer packages rather than a vendored copy; see
    `packaging/README.md` for why and for the vendored-tree follow-up
    (`packaging/linux/vendor-gstreamer.sh`).
  - **AppImage**: a portable, distro-independent build (`packaging/appimage/`).
  - **Docker** (`streamboatd` only): `ghcr.io/waayway/streamboat`, tagged
    `latest` and per release. `packaging/docker/compose.yml` is a ready-made
    compose service.
  - Systemd units for both the daemon-as-a-service (`streamboatd.service`,
    system) and per-user (`streamboatd.service`, `systemd --user`) cases ship
    inside the deb/rpm packages and in `packaging/linux/systemd/`.
- **Windows**: an MSI (`packaging/windows/wix/`) plus a
  [winget](https://github.com/microsoft/winget-pkgs) manifest template
  (`packaging/windows/winget/`). SmartScreen will warn on first run because
  the installer is unsigned: click **More info**, then **Run anyway**.
- **macOS**: a DMG (`packaging/macos/`), arm64 and x86_64. Gatekeeper will
  refuse to open the unsigned, ad hoc-signed app: right-click (Control-click)
  it and choose **Open**, or run
  `xattr -d com.apple.quarantine /Applications/streamboat.app` once.
  No Homebrew cask until notarization exists (D-041).
- **GitHub Releases**: every tagged release publishes all of the above plus
  a `SHA256SUMS` file, as a **draft** -- a person reviews and publishes it,
  the release notes' human-authored parts included (D-040).
- **Not planned for v1: Flathub.** D-041; revisit once the repository has
  the tagged-release history Flathub's own submission requirements ask for.

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
streamboat open https://listen.tidal.com/album/12345   # opens that page directly
streamboat https://listen.tidal.com/playlist/<uuid>     # same thing; a bare link also works
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

## Offline pins

```
streamboat pin track 12345           # or: pin album 98765 / pin playlist <uuid>
streamboat pins                      # what's pinned, and whether it still needs revalidating
streamboat unpin track 12345
streamboat play 12345                # uses a pinned, valid copy automatically
```

A pin is always explicit — streamboat never caches anything you didn't ask
for, and never caches artwork or metadata this way either (D-022). Pinning
resolves and downloads the exact same cleartext stream ordinary playback
would (never TIDAL's licensed offline/download mode), then stores it as
AES-256-GCM-encrypted chunks under a key derived from this install's
keyring/file secret *and* its device identity — copy the `offline/`
directory to another machine and the chunks are unreadable there, on
purpose. Playback reads a pinned track through a loopback HTTP server on
127.0.0.1 that decrypts on the fly; no plaintext copy is ever written to
disk. A pin is revalidated against your account every `offline_validity_days`
(default 30) and the whole cache is wiped on logout, on an unpin, or if
TIDAL's subscription sub-status says the account itself can no longer
stream. `offline_dir` and `offline_max_bytes` in `settings.json` override
where it lives and how big it may grow. See
`crates/streamboat-player/src/offline.rs` for the exact format.

## Multiroom with Snapcast

`OutputConfig::Snapcast { host, port }` (D-034) is a fourth, explicit output
mode alongside Shared and Exclusive — mutually exclusive with bit-perfect
by construction, since it resamples every track to one fixed PCM format
(48000 Hz, 16-bit, stereo) and connects out, as a plain TCP client, to a
running `snapserver`'s stream source. Point snapserver's config at
whatever port streamboat is told to connect to:

```
stream = tcp://0.0.0.0:4953?name=streamboat&mode=server&sampleformat=48000:16:2
```

(`mode=server` is snapserver's own name for "I hold the socket open, the
source dials in" — the direction streamboat expects; `4953` is only this
example's port number, any free one works as long as both sides agree on
it.) Run `streamboat snapcast-discover` to find a snapserver on the LAN via
mDNS instead of hand-typing an address, and add
`controlscript=streamboat snapcast-plugin` to the same stream's config so
Snapweb and every Snapcast client show real title/artist/artwork and get
real play/pause/skip/seek controls instead of an anonymous PCM feed —
`snapcast-plugin` bridges Snapcast's own stream-plugin JSON-RPC protocol to
a running `streamboatd`'s control API. See `docs/architecture.md`'s
"Multiroom: Snapcast output" section for exactly what is and is not
implemented (the libmpv backend's Windows/macOS side is not, yet).

## Privacy

streamboat sends nothing about you anywhere except to TIDAL itself, to play
what your subscription entitles you to — no telemetry, no analytics, no
phone-home, no hosted crash-reporting service, ever (D-029). Play reporting
(above) is the one deliberate exception, disclosed and toggleable. Logs and
crash reports stay on this machine, in `<data dir>/logs/` and
`<data dir>/crashes/`, with tokens, session ids and similar values masked
before a line ever reaches disk (`streamboat paths` prints exactly where).

### Debug bundle

`streamboat debug-bundle [--out path]` writes one zip with everything a bug
report needs and nothing it must not have: the redacted logs and crash
reports above, your settings with every credential reduced to whether it is
set at all (never the value), and basic environment info (resolved
directories, OS/architecture, the linked GStreamer version). It never
includes the token file, the offline cache, or anything from the OS
keyring — attach it to an issue exactly as it comes out.

## Legal

streamboat uses an API TIDAL does not document for third parties, under your
own subscription. Read TIDAL's terms yourself before using it; the project's
posture, and how comparable clients position themselves, is recorded in
`docs/research/tidal-api.md` and `.claude/skills/tidal-api/references/legal-and-landscape.md`.
TIDAL Connect is out of scope in both directions, permanently (D-035).

Pinning a track for offline playback does not change any of that: it is an
encrypted cache of a stream your subscription already entitles you to, tied
to this specific install and this specific account, unreadable if copied
elsewhere, and deleted on logout or if the subscription itself stops
serving the account — never an export, a download, or a copy meant to
outlive the subscription (D-022).

## Licence

`streamboat-core` is Apache-2.0; the other crates are GPL-3.0-only. See
`LICENSES/`.
