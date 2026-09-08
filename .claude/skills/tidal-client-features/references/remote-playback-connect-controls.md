# Remote playback, TIDAL Connect, OS media controls, sharing

Table of contents:
1. Three remote transports
2. TIDAL Connect: controller vs target
3. TIDAL Live / DJ sessions
4. OS media controls (MPRIS/SMTC/Now Playing) — `souvlaki` covers all three; reference clients split anyway
5. `tidal://` deep-link grammar
6. Keyboard shortcuts — evidence quality caveat
7. Share links and cover-art URLs
8. Control surfaces for headless mode

## 1. Three remote transports

`ref:TidaLuna/.../store/RemotePlayback.ts`:

```
RemotePlaybackDeviceType = "chromeCast" | "tidalConnect" | "cloudConnect"
RemotePlaybackDevice = { id, friendlyName, fullname /* e.g. x._googlecast._tcp.local */,
                         addresses[], port, type }
RemotePlayback { devices: {chromeCast[], cloudConnect[], tidalConnect[]},
                 connectedDevice, deviceToConnectTo, isConnected,
                 remotePlaybackReceiverState, session, sessionStatus }
```

Actions: `remotePlayback/{DISCOVER_DEVICES, REFRESH_DEVICES, DEVICES_RECEIVED, CONNECT_TO_DEVICE,
DEVICE_CONNECTED, DEVICE_DISCONNECTED, DISCONNECT_ALL_DEVICES, CONNECTION_LOST}`, plus
per-transport sub-namespaces (`remotePlayback/tidalConnect/*`,
`remotePlayback/remotePlaybackReceiver/*`). Discovery is mDNS (`_googlecast._tcp.local` visible in
the type). AirPlay is **not** a client-side transport on macOS — it's an OS-level output device
that shows up in the audio device list, so it comes largely free via CoreAudio device selection.

**The `remotePlaybackReceiver` namespace implies the client can also *be* a receiver — evidence is
stronger than a namespace name**: `modal/REMOTE_PLAYBACK_RECEIVER_DISCONNECT_MODAL` is a
user-facing dialog that only makes sense if the desktop app can be connected *to* as a target. Still
**[uncertain]** overall, but raise your confidence accordingly when scoping this.

**Cloud queue is documented, not a black box** — client-side `cloudConnect` + `cloudQueue/*`
namespace; server-side the v2 spec has `/playQueues`/`/playQueues/{id}` with
`PlayQueues_Attributes = {createdAt, lastModifiedAt, repeat: NONE|ONE|BATCH, shuffle:
OFF|BATCH|ALL, shuffled}`. **This vocabulary does not match the desktop client's own
`RepeatMode {Off,All,One}` + boolean+seed shuffle** — map between them explicitly if you touch this.
No OSS client implements it yet, but "call a documented endpoint" is a different research cost from
"reverse-engineer an undocumented one."

## 2. TIDAL Connect: controller vs target

A hardware streamer/DAC/speaker (or the TIDAL app on a TV) advertises itself over mDNS; the
phone/desktop app acts as *controller* and hands off; the target pulls the stream directly from
TIDAL's CDN. The controller's local DSP (e.g. crossfade) does not apply to the target. Not every
target supports Autoplay ([verified-web, unfetched]).

**Building a Connect target is not open-source-reproducible.** The only working Linux
implementation (`ref:tidal-connect`) wraps a **proprietary** `tidal_connect_application` binary
authenticated with a vendor device certificate (default: `IfiAudio_ZenStream.dat`), announced via
Avahi/mDNS, with ALSA output. It's a Docker wrapper around a closed binary, MIT-licensed itself but
useless without the binary. **Quality nuance**: `ref:tidal-connect/README.md:59` says this
implementation "could play hi-res files only up to 24/48 and MQA content" historically, with in-app
MQA unfolding to 24/88–24/96; after the 24 Jul 2024 MQA removal, HiRes FLAC is simply unavailable to
it. Say "historically 24/48 plus MQA; effectively LOSSLESS-only since MQA's removal" — not "capped
at LOSSLESS since July 2024" (a deliberate-sounding cap that isn't what happened).

**Do not build a Connect target.** A Connect *controller* is reproducible in principle (mDNS
discovery is documented, and cloud queue is a real spec'd resource per §1) but the device
discovery/control protocol itself is undocumented and unimplemented by any OSS client — treat as
research-later.

Chromecast: same controller/target split, Google's protocol; the client has a full `chromeCast/*`
namespace including `UPSTREAM_QUEUE_CHANGED`/`REPEAT_MODE_CHANGED`.

TV apps (Apple TV, Fire TV, Android TV, smart TVs, consoles) are Dolby Atmos targets and use
device-code login (`auth/DEVICE_AUTH_CODE_RESPONSE_RECEIVED`, `auth/DEVICE_AUTH_CODE_EXPIRED` exist
even in the desktop client).

## 3. TIDAL Live / DJ sessions

TIDAL "Live" (originally beta-named "DJ") launched April 2023: a subscriber broadcasts what they're
playing in real time to other subscribers. **No `live/` namespace anywhere in the 2026 desktop
Redux action list** — suggestive but not proof the feature is gone (see the standing version
caveat). No discontinuation announcement found either. **[uncertain]** — do not treat as active; do
not treat as confirmed removed.

Distinct: the **DJ Extension add-on** (~$9/mo, **[uncertain, possibly stale for 2026]** — see
`entitlements-tiers-history.md` §2), integrating TIDAL as a source in Serato DJ Pro (3.1.3+ for
stems), rekordbox, djay Pro, VirtualDJ, DJUCED. Item-level flags `djReady`/`stemReady` plausibly
gate this (**[inferred]**, not stated in source). This is third-party DJ-software integration, not
an in-app feature.

## 4. OS media controls (MPRIS/SMTC/Now Playing) — `souvlaki` covers all three; reference clients split anyway

**`souvlaki` 0.8.3 actually covers all three platforms in one crate.** Its resolved dependency tree
(`ref:sone-windows/src-tauri/Cargo.lock:5041-5055`) pulls in `dbus`+`dbus-crossroads` (Linux MPRIS
— crates.io also documents `use_dbus`/`use_zbus` feature-selectable Linux backends), `windows 0.44`
(Windows SMTC), and `cocoa`/`objc`/`dispatch` (macOS `MPNowPlayingInfoCenter`). Do not repeat "no
single crate unifies X" as fact here — it's contradicted by `souvlaki`'s own dependency graph. What
*is* true is only that the reference clients chose not to use it everywhere:

- sone-windows (a third-party fork of Sone by a different author, not Sone's own Windows port —
  `references/sources.md`) uses `souvlaki` (`ref:sone-windows/src-tauri/Cargo.toml:64`) for
  **Windows SMTC only** — its README (`ref:sone-windows/README.md:69`) describes only "Windows SMTC
  Integration," no MPRIS/Now Playing claim.
- Sone on Linux uses `mpris-server = "0.9"` instead (`ref:sone/src-tauri/Cargo.toml:66`) — a
  richer, async-native MPRIS surface than `souvlaki` exposes; plausibly why Sone picked it over
  `souvlaki` on Linux even though `souvlaki` can reach D-Bus there too.
- High Tide implements MPRIS itself via Python D-Bus (`ref:high-tide/src/mpris.py`).
- tidal-hifi (Node) has its own MPRIS service.

**Evaluate `souvlaki` for all three platforms before assuming a split is necessary** — compare its
per-platform surface against a dedicated library (does it expose everything `mpris-server` does on
Linux? what does its macOS backend actually cover?) rather than defaulting to three separate
integrations. Hardware media keys come largely free once whichever integration is chosen exists —
tidal-hifi binds exactly three OS-level key identifiers to its playback controls,
`MediaPlayPause`, `MediaNextTrack`, `MediaPreviousTrack` (`ref:tidal-hifi/src/constants/mediaKeys.ts`),
which is the standard Electron/OS media-key set; an MPRIS/SMTC/Now Playing integration typically
maps these for free as part of registering with the OS, so budget them as part of that
integration's cost, not a separate line item.

## 5. `tidal://` deep-link grammar

**Confirmed, correcting any "undocumented anywhere" claim.** Two independent reference clients
implement the identical grammar:

- `tidal://{track|album|artist}/{numeric id}`
- `tidal://{playlist|mix}/{string id}`
- At least one collection-link form: `tidal://my-collection/tracks`

Sources: `ref:high-tide/src/lib/utils.py` (`open_tidal_uri`, lines 486-518, a `match` over
artist/album/track/mix/playlist) and `ref:sone/src/lib/tidalUrl.ts` (`parseTidalUrl`, an identical
`switch`); the collection form is in `ref:sone/src/utils/itemHelpers.ts` and its `Home.tsx`.

The desktop app registers the protocol handler via `settings.openLinksInDesktopApp` /
`settings/SET_OPEN_LINKS_IN_DESKTOP_APP` and `launchHandler/LAUNCH`. Strawberry uses redirect URI
`tidal://login/auth` for OAuth (`ref:strawberry/src/tidal/tidalservice.cpp`) — that remains the only
attested *auth* use of the scheme, distinct from the entity-navigation grammar above.

## 6. Keyboard shortcuts — evidence quality caveat

**Do not present the commonly-copied shortcut table as TIDAL's own documentation.**
`ref:tidal-hifi/src/features/hotkeys/hotkeyConfig.ts` (`DEFAULT_HOTKEY_ACTIONS`) is a third-party
wrapper's own configurable global-hotkey defaults, using tidal-hifi-specific action ids
(`hardReload`, `sidebarMusic`, `openSettings1`/`openSettings2`). Its own header comment says it is
"Based on the hotkeys from https://defkey.com/tidal-desktop-shortcuts" — a crowd-sourced
third-party aggregator, unreachable to verify directly. Treat the table as **[verified-web, single
unverifiable aggregator]**, not [verified-source].

| Shortcut | Action (per tidal-hifi's defaults) |
| --- | --- |
| `Ctrl+A` | Toggle favourite on current track |
| `Ctrl+L` | Log out |
| `Ctrl+U` | Hard reload |
| `Ctrl+R` | Cycle repeat mode |
| `Ctrl+W` | Copy universal track link |
| `Ctrl+P` | Expand Now Playing |
| `Ctrl+↑` / `Ctrl+↓` | Volume ±10% |
| `Alt+←` / `Alt+→` | Back / forward |
| `Ctrl+=` or `Ctrl+0` | Settings |
| `Alt+M/E/F/U` | Music / Explore / Feed / Uploads |
| `Alt+S` | Toggle sidebar |
| `Alt+Shift+P/A/T/V/R/M` | Collection: Playlists / Albums / Tracks / Videos / Artists / Mixes & Radio |

Note: tidal-hifi's config describes `Ctrl+R` as cycling "off, one, all," whereas the client's own
`RepeatMode` enum is ordered `Off=0, All=1, One=2` — the cycle direction is not independently
confirmed either way. The desktop client's own in-app cheatsheet action is `modal/SHOW_SHORTCUTS`
— check that on a live install if precision matters. Shortcuts are window-local, not
global/system-wide. Desktop notifications on track change are a wrapper feature, not confirmed in
the official app.

## 7. Share links and cover-art URLs

- Web URLs: **only the track form is source-verified** — `getTrackURL` builds
  `${tidalUrl}/browse/track/${trackId}` (`ref:tidal-hifi/src/features/tidal/url.ts`). The
  album/artist/playlist/video/mix/user variants are **[inferred]** by pattern extrapolation.
- Content models separately carry canonical `url` fields (`http://www.tidal.com/track/{id}`,
  `/album/{id}`, `/artist/{id}`, `/playlist/{uuid}`) — a *different* scheme, both exist.
- **Universal links**: `getUniversalLink` appends `&u` when a query string already exists, else
  `?u`, turning a share link into a cross-platform link that resolves to the listener's own
  streaming service. **This is more than "a trivial string append" as a feature**: the v2 spec
  models `/dspSharingLinks` (`{spotify, appleMusic, amazonMusic, youTubeMusic}` link objects) and a
  `/shares`/`/savedShares` resource family as real server-side state — relevant if streamboat ever
  wants to *resolve* an incoming shared link or show which other services a track is on.
- Cover art: `getCoverURL` builds
  `https://resources.tidal.com/images/<uuid-with-dashes-replaced-by-slashes>/<size>x<size>.jpg`
  (sizes include 80, 1280), passing absolute URLs through untouched (needed for uploaded content,
  which doesn't use this convention).
- Share context menus per entity: `ALBUM_SHARE`, `ARTIST_SHARE`, `PLAYLIST_SHARE`, `MIX_SHARE`,
  `USER_SHARE`, `CONTRIBUTOR_SHARE`.
- TIDAL also publishes an official embed-widget product (developer.tidal.com embeds) for embedding
  a track/album/playlist player on a third-party page — a distinct sharing surface from the
  deep-link/universal-link grammar above. `[verified-web, unfetched]` — not read directly, since
  developer.tidal.com is blocked to direct fetch in the research environment; see
  `docs/research/tidal-client-features.md` §7.

## 8. Control surfaces for headless mode

Material for SKILL.md "Open decisions" #7 (headless mode's control contract) — three proven shapes,
none mutually exclusive:

**tidal-hifi's REST API** (`ref:tidal-hifi/src/features/api/swagger.json`) — a minimal but complete
playback-control contract, proven in production against the real client:
`/player/play`, `/player/pause`, `/player/playpause`, `/player/next`, `/player/previous`,
`/player/seek/absolute`, `/player/seek/relative`, `/player/volume`, `/player/shuffle/toggle`,
`/player/repeat/toggle`, `/player/favorite/toggle`, plus read endpoints `/current` and
`/current/audio-quality` (`{quality, badgeText, bitDepth, sampleRate, codec}` — see
`quality-playback-queue.md` §2). The same handlers are also exposed unprefixed (`/play`, `/pause`,
`/playpause`, `/next`, `/previous`, `/favorite/toggle`) as legacy aliases, plus `/image`,
`/current/image` and `/health` — a worked example of versioning a local control surface without
breaking existing clients (relevant to Open decision #7). Non-player settings endpoints exist too:
`/settings/skipped-artists`, `/settings/skipped-tracks` (+ `/current`, `/delete` on each).

**Sone's local services** (`ref:sone/src-tauri/src/lib.rs:49,51,212,215`, `ref:sone/README.md`) —
two independent, off-by-default local servers: an **MCP server on port 5577** that lets any MCP
client search the library, control playback, and manage playlists/favourites, with one-click token
generation in Settings; and an **OBS browser-source
overlay on port 5578** showing the currently-playing track (art, title, artist, quality badge,
progress bar) for streaming software, at a documented 400×120px size.

**mopidy-tidal's browse-URI tree** (`ref:mopidy-tidal/mopidy_tidal/library.py`) — the proven shape
for a headless *browse* hierarchy (distinct from playback control above), useful if streamboat
exposes an MPD-compatible or similar tree-browsing interface:
`tidal:home`, `tidal:for_you`, `tidal:explore`, `tidal:moods`, `tidal:genres`, `tidal:mixes`,
`tidal:hires`, plus the My Collection roots `tidal:my_artists`, `tidal:my_albums`,
`tidal:my_playlists`, `tidal:my_mixes`, `tidal:my_tracks`.

None of these three is "the" answer — they cover different layers (REST control, agent/streaming
integration, browse tree) and a headless streamboat plausibly wants more than one. Define the
contract before writing either the desktop UI or the CLI front end (Implication 7 in the main
report); see `docs/research/headless-connect.md` for the design write-up this skill points to
rather than duplicating.
