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
hand-over to the next track and a selectable ALSA device. The desktop shell
(iced) and the libmpv backend for Windows and macOS come next. Everything that
was decided about the product and the stack is in `docs/DECISIONS.md`.

## Build (Linux)

```
sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  libgstreamer-plugins-bad1.0-dev gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
  gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-alsa libasound2-dev pkg-config
cargo build --release
```

Binaries: `target/release/streamboat` (CLI, later the desktop shell) and
`target/release/streamboatd` (daemon).

GStreamer 1.26.10 or newer is needed for TIDAL's FLAC-in-DASH streams through
the modern demuxer; on older versions streamboat demotes `dashdemux2` at
startup so the legacy element handles them. Release packages will bundle a
pinned GStreamer (D-020).

## Client credentials

No client id ships in this repository (D-023). streamboat presents the
client id you give it, in this order:

1. `client_id` / `client_secret` in `settings.json` (see `streamboat paths`);
2. `STREAMBOAT_CLIENT_ID` / `STREAMBOAT_CLIENT_SECRET` in the environment;
3. the same variables set when the binary was built (an embedded default).

The ids in use across the open-source TIDAL ecosystem were extracted from
TIDAL's own applications; any id you embed in a binary is extractable, and
TIDAL can revoke one at any time. Hi-res (`HI_RES_LOSSLESS`) is only served in
the clear to a client id that has a secret; without one, streamboat drops the
hi-res tiers from the cascade and says so.

## Use

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

The daemon speaks the same Command/Event protocol as JSON lines on stdin and
stdout for now (`streamboatd --device hw:1,0 --exclusive`, then
`{"type":"play","items":[{"track_id":12345}]}`); the HTTP + WebSocket control
API and MPRIS adapter follow.

Tokens are stored encrypted (AES-256-GCM) in the data directory with a
0600 key file beside them; OS-keyring storage comes with the desktop shell.

## Layout

```
crates/streamboat-core      Apache-2.0  API client, login, token store, manifests, protocol types
crates/streamboat-player    GPL-3.0     Engine trait, GStreamer backend, queue and playback
crates/streamboat-server    GPL-3.0     streamboatd
crates/streamboat-desktop   GPL-3.0     streamboat (CLI now, iced shell next)
docs/DECISIONS.md                       what was decided and why
docs/research/                          the fact-checked research the decisions rest on
.claude/skills/                         repo knowledge for coding agents
```

## Legal

streamboat uses an API TIDAL does not document for third parties, under your
own subscription. Read TIDAL's terms yourself before using it; the project's
posture, and how comparable clients position themselves, is recorded in
`docs/research/tidal-api.md` and `.claude/skills/tidal-api/references/legal-and-landscape.md`.
TIDAL Connect is out of scope in both directions, permanently (D-035).

## Licence

`streamboat-core` is Apache-2.0; the other crates are GPL-3.0-only. See
`LICENSES/`.
