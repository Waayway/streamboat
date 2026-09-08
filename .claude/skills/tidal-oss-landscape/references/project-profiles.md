# Project profiles — everything except Sone

Sone and sone-windows have their own file (`sone-deep-dive.md`); auth/streaming/manifest detail
common to all projects is in `api-auth-streaming.md`; audio-engineering comparisons are in
`audio-engineering.md`. This file is the per-project narrative for everything else, corrected
against the fact-check pass. Full sourcing: `docs/research/oss-landscape.md` §4-§17.

## Table of contents

1. High Tide — the owner's named reference #2
2. Strawberry Music Player — TIDAL as one backend of many
3. tidal-hifi — the Electron/Widevine wrapper
4. TidaLuna — a mod of the official client (intelligence, not a model)
5. mopidy-tidal — the headless/server precedent
5a. Music Assistant — a second, actively-maintained headless precedent
5b. lms-plugin-tidal (Lyrion/LMS) — a third server-side precedent
6. tidal-connect (Docker) — the "be a Connect target" precedent
7. Official TIDAL SDKs (web, Android, iOS)
8. python-tidal (tidalapi) — the de-facto unofficial API library
9. tidal-cli — the official-API CLI + MCP precedent
10. tidalt — Go TUI + daemon, bit-perfect ALSA
11. tidalrs — Rust API client library
12. TidalSwift — Apple-platform precedent (read-only, do not copy)
13. Smaller / historical projects

## 1. High Tide — the owner's named reference #2

`ref:high-tide` · https://github.com/Nokse22/high-tide · GPL-3.0 · 671★ / 66 forks / 90 open
issues · created 2023-09-22 · last push 2026-08-21 · community project with a Matrix channel
(`#high-tide:matrix.org`).

**Stack**: Python 3 + PyGObject, GTK4 + libadwaita, Blueprint (`.blp` → `.ui`), Meson,
`python-tidal` for all API access, GStreamer for playback, `pypresence` for Discord, libsecret via
`gi.repository.Secret`, `libportal`/`Xdp` for Flatpak detection, gettext for i18n. 37 Python
files, 6,821 lines; 23 `.blp` files; **8 translations** (`fr de nl pt_BR es it zh_TW pl` per
`po/LINGUAS`).

**Auth: PKCE only, 91 lines, no device-code path.** `ref:high-tide/src/login.py` does exactly
three things: `session.pkce_login_url()` → user opens it → pastes the redirect URL back →
`session.pkce_get_auth_token(redirect_url)` → `process_auth_token(token_json, is_pkce_token=True)`
→ `check_login()`. Runs on a `threading.Thread`, marshals back with `GLib.idle_add`.

Token storage: a libsecret `Secret.Schema` named `io.github.nokse22.high-tide` with a single
`version` STRING attribute, storing a JSON blob under key `high-tide-login`
(`token-type`/`access-token`/`refresh-token`/`expiry-time`/`is-pkce`). On startup, **outside
Flatpak only**, it force-unlocks the default collection via `Secret.Service.get_sync()` →
`Collection.for_alias_sync()` → `unlock_sync()`, a workaround for a real bug (issue #97). Flatpak
detection: `Xdp.Portal.running_under_flatpak()`.

**GStreamer pipeline**: `Gst.Pipeline.new("dash-player")` containing a single `playbin3`
(falling back to plain `playbin`, gapless off, if `playbin3` is unavailable). Gapless =
`playbin3`'s `about-to-finish` signal. Audio sink is a bin built from a description string via
`Gst.parse_bin_from_description`: `queue ! audioconvert ! [normalization chain] ! audioresample !
<sink>` (full normalization-chain string in `audio-engineering.md` §4). Sink map: `AUTO→
autoaudiosink`, `PULSE→pulsesink`, `ALSA→alsasink device=<alsa_device>`, `JACK→jackaudiosink`,
`OSS→osssink`, `PIPEWIRE→pipewiresink`. **`pipewiresink` disables gapless.**
`change_audio_sink()` sets the pipeline to `NULL`, rebuilds the bin, restores `PLAYING`, and
re-seeks to the saved fractional position — sink switching without losing your place. Bus error
handling special-cases two strings: `"Internal data stream error"+"not-linked"` → restart the
pipeline on the same track; `"Error outputting to audio device"+"disconnected"` → toast "ALSA
Audio Device is not available" and pause. Volume: `playbin.volume = value**2` when "quadratic
volume" is on in settings, else linear. **There is no bit-perfect or exclusive mode** —
`alsasink device=` is as close as it gets; no `snd_pcm_hw_params` negotiation.

**Caching — corrected, this was significantly overstated as "opt-in" in an earlier pass.**
`utils.MUSIC_DIR` is **unconditionally** `$XDG_CACHE_HOME/high-tide/music` (or
`~/.cache/high-tide/music`), created at startup with **no GSettings key to disable it** (the full
16-key schema has none). `_get_cached_or_stream_url` first checks
`MUSIC_DIR/{track.id}_{session.audio_quality}.m4a`; otherwise, unless
`Gio.NetworkMonitor.get_network_metered()`, it spawns a background thread that runs `ffmpeg
-protocol_whitelist file,crypto,data,http,https,tcp,tls -i manifest.mpd -f mp4 -c copy -y <tmp>`
for MPD, or a raw `requests.get(stream_url, stream=True)` writing 8 KiB chunks for BTS. **MPD
caching only fires on the GStreamer ≥ 1.26 branch** (where a manifest file exists on disk) — on
older GStreamer, the `data:` URI branch never spawns this thread, so DASH tracks aren't cached
there; BTS caches unconditionally regardless of GStreamer version. It is bounded, not unbounded: a
startup thread (`window.py:223`) runs `utils.evict_cache(utils.MUSIC_DIR, 5)`, evicting by
`st_atime` down to 5 GB. **Net effect**: an always-on, unencrypted, non-consented, size-capped
(5 GB LRU) full-quality media cache with zero user visibility or control, for every user — not
opt-in, not unbounded. Still legally/reputationally the riskiest default in the set, and it's
inside the owner's own named reference — make this a deliberate, documented streamboat decision,
not an inherited default.

**Settings**: 16 GSettings keys (`window-width`, `window-height`, `quality`,
`last-playing-index`, `last-playing-thing-id`, `last-playing-thing-type`, `last-volume`,
`repeat`, `preferred-sink`, `run-background`, `normalize`, `quadratic-volume`,
`app-id-change-understood`, `video-covers`, `discord-rpc`, `alsa-device`).

**MPRIS** (`ref:high-tide/src/mpris.py`): hand-rolled `Gio.DBus` server, forked from Blanket
(which forked Lollypop) — the standard GNOME lineage. Introspection XML lives in the class
docstring.

**Threading model** (`CONTRIBUTING.md`): thread-target methods prefixed `th_`, must catch all
exceptions, must marshal to UI with `GLib.idle_add`. Custom widgets prefixed `HT`. 4-space
indent, 88-character lines.

**Flatpak**: runtime `org.gnome.Platform` **50**, meson build, `finish-args`:
`--share=network --share=ipc --socket=fallback-x11 --device=dri --socket=wayland
--socket=pulseaudio --filesystem=xdg-run/pipewire-0:ro --filesystem=xdg-run/discord-ipc-0`.
Modules: `libportal.json`, `blueprint-compiler.json`, `alsa-utils.json`, `python3-pypresence.json`,
`python3-six.json`, `python3-tidalapi.json` (pinned wheel list). Shipped on Flathub as
`io.github.nokse22.high-tide`. **No `--device=all`** — exclusive ALSA would not work under this
manifest as written; see `packaging-distribution.md` for the same conclusion independently
confirmed against Sone's own shipped Flathub manifest.

**Lacks**: bit-perfect/exclusive output; sample-rate switching; Windows/macOS; headless/CLI;
offline sync management UI; podcasts/audiobooks; TIDAL Connect; Atmos handling;
encrypted-at-rest cache; settings-level client-ID override.

**Borrow**: the 91-line PKCE dialog; the keyring unlock workaround for non-Flatpak Linux; the
swappable audio-sink-bin-from-a-description-string pattern and sink-change-with-position-restore;
the `IDisconnectable` signal-cleanup mixin; the CONTRIBUTING thread-safety rules; the Flatpak
manifest structure and vendored-wheels pattern; the PipeWire-breaks-gapless finding; the synced
lyrics widget (`audio-engineering.md`-adjacent, see `sone-deep-dive.md` §5 for the parsing detail
comparison). **Avoid**: silent on-disk caching with no toggle (corrected framing above); a single
`utils.py` at 843 lines carrying session state as module globals; `playbin`'s all-or-nothing
pipeline if you want signal-path control; assuming `playbin3` always exists.

## 2. Strawberry Music Player — TIDAL as one backend of many

`ref:strawberry` · https://github.com/strawberrymusicplayer/strawberry · GPL-3.0 · 3,948★ / 341
forks / 21 open issues · last push 2026-09-07 · C++17 + Qt 6.4+ · the most mature and actively
maintained codebase in the whole set, and the only general-purpose music player with TIDAL as one
of several streaming backends. TIDAL code: 2,709 lines across 6 `.cpp` files (3,429 across all 12
`.cpp`+`.h` files) in `ref:strawberry/src/tidal/`.

**Auth**: shared `OAuthenticator`, `Type::Authorization_Code`, `set_use_pkce(true)`,
`authorize_url = https://login.tidal.com/authorize`,
`access_token_url = https://login.tidal.com/oauth2/token`, `redirect_url = tidal://login/auth`
(custom scheme, `use_local_redirect_server(false)`), `scope = "r_usr w_usr"`. Client ID: optional
compile-time `TIDAL_CLIENT_ID` (via `Utilities::MaybeDecryptApiCredential`) or user-supplied in
settings — **no client secret is ever compiled in**. Full param detail in
`api-auth-streaming.md` §2.

**API base**: `https://api.tidalhifi.com/v1`. `countryCode` injected by `TidalBaseRequest`; HTTP
401 clears the session and forces re-login.

**Stream URL**: all four endpoint variants (`api-auth-streaming.md` §4), user-selectable, default
`PlaybackInfoPostPaywall`. Quality options `LOW/HIGH/LOSSLESS/HI_RES/HI_RES_LOSSLESS`, default
`LOSSLESS`.

**How it degrades — the posture streamboat should copy exactly.** Refuses encrypted streams
outright, three separate ways, one user-facing message: *"Received a %1 encrypted stream from
Tidal, which Strawberry does not support. Whether Tidal delivers encrypted streams depends on the
client ID in use. Try changing the Client ID in the Tidal settings"* — triggered by (a) a
manifest `encryptionType` non-empty and not `"NONE"`, (b) a non-empty top-level `encryptionKey`,
(c) a `securityType` non-empty and not `"NONE"` alongside a non-empty `securityToken`. DASH
manifests wrapped as `data:application/dash+xml;base64,…`. Filetype inferred from `mimeType` via
`QMimeDatabase`, falling back to `codecs`, falling back to URL extension.

**Collection**: three SQLite tables per source (`tidal_artists_songs`, `tidal_albums_songs`,
`tidal_songs`) through the shared `CollectionBackend` — a genuinely good pattern for a browsable
local index of a remote catalogue. Search limits default to 4 artists/10 albums/10 songs,
`searchdelay` 1500 ms.

**Audio — corrected, "general bit-perfect support" overstates it significantly.** See
`audio-engineering.md` §4 for the full EBU R128/bit-perfect incompatibility, and note here the
platform reality check: Strawberry's own README scopes bit-perfect to **Linux only** ("Bit-perfect
playback on Linux"), and its **macOS/Windows binary releases are sponsor-only**. So Strawberry is
real evidence for a Windows-WASAPI-exclusive path (same mechanism as sone-windows) but is not the
cross-platform bit-perfect precedent it might look like from its feature list alone.

**Borrow**: the honest refuse-and-explain behaviour on encrypted streams; the user-selectable
stream-URL method as a hedge against endpoint churn; the compile-time-or-user-supplied client ID
with no secret; the SQLite per-source collection schema; `MaybeDecryptApiCredential` as a
naming-honest alternative to Sone's XOR obfuscation; the WASAPI-exclusive-mode setting as a
Windows reference. **Avoid**: modelling streamboat on a general player's plugin shape if the goal
is a dedicated TIDAL client; citing Strawberry as proof the general engine gives bit-perfect on
macOS or gapless-safe Windows out of the box — it doesn't.

## 3. tidal-hifi — the Electron/Widevine wrapper

`ref:tidal-hifi` · https://github.com/Mastermindzh/tidal-hifi · MIT (GitHub reports
"other"/NOASSERTION) · 1,725★ / 100 forks / 22 open issues · created 2019-09-16 · last push
2026-08-31 · v8.1.3 · TypeScript, 8,011 lines in `src/`. **The highest-star project in the set.**

**The whole idea**: don't reimplement TIDAL — wrap `listen.tidal.com` in a Chromium that has a
Widevine CDM, via castlabs' Electron fork (`github:castlabs/electron-releases#v43.0.0+wvcus`,
`electronVersion: 43.0.0`). `components.whenReady()` awaited in `src/main.ts:413`. **The only
project in the set that plays DRM-protected TIDAL content without touching DRM itself** —
Chromium does it.

**The real objection is operational, not just about audio quality.** tidal-hifi's own README
credits castlabs specifically for "maintaining Electron with Widevine CDM installation,
**Verified Media Path (VMP)**, and **persistent licenses (StorageID)**." VMP means the binary must
be signed through castlabs' own signing service to be trusted by Widevine at all — adopting this
approach is a build/CI dependency on a third-party signing service, a much harder sell than "the
audio is worse." tidal-hifi's own packaging *is* the most complete CI in the set:
`.github/workflows/{build,release}.yml` plus electron-builder configs invoked by named npm
scripts (`build-deb`, `build-rpm`, `build-snap`, `build-arch`, `build-win`, `build-mac`) — worth
reading as a packaging-CI checklist even though the audio approach itself should not be copied.

**Controller abstraction** — a `TidalController` interface with four implementations selected by
`advanced.controllerType`: `MediaSessionController` (default, browser MediaSession API, falls
back to DOM for anything MediaSession doesn't expose), `DomTidalController` (DOM parsing, the
default fallback branch), `ReduxController` (walks the React fiber tree to find TIDAL's Redux
store — richest metadata: bit depth, sample rate, codec), `TidalApiController` (marked "In
Development / Not Ready / Minimal" in the docs — four controllers total, corrected from an
earlier "three"). This four-way strategy-with-graceful-fallback pattern is the right answer
whenever scraping a moving target.

**Additions on top of the web player**: MPRIS, Discord RPC, ListenBrainz, configurable hotkeys,
SCSS themes, idle inhibitor, custom titlebar, window transparency, a sharing service, and a local
**Express 5 API on port 47836** documented with swagger-jsdoc/swagger-ui-express, exposing
`GET /current` and `GET /current/audio-quality`.

**Audio quality caveat, from the project's own docs**: *"By default Chromium resamples all audio
to 48kHz."* A setting passes `--audio-output-sample-rate=192000` and disables the
out-of-process audio service, but the docs are explicit that PipeWire/PulseAudio must *also* be
reconfigured or it resamples back down regardless. No bit-perfect path, no exclusive mode, no
gapless.

**Borrow**: the controller-with-fallbacks pattern; the local HTTP API + OpenAPI generation as a
cheap, community-adopted integration surface; the electron-builder config set as a packaging
checklist; the honest audio-quality documentation. **Avoid**: the whole approach if streamboat
wants bit-perfect audio, gapless, or a native UI — a Chromium resampler sits between you and the
DAC and cannot be removed; the VMP signing dependency if DRM playback is ever considered.

## 4. TidaLuna — a mod of the official client (intelligence, not a model)

`ref:TidaLuna` · https://github.com/Inrixia/TidaLuna · MS-PL · 591★ / 56 forks / 15 open issues ·
created 2025-04-16 · last push 2026-09-01 · v1.16.6-beta · TypeScript/ESM, esbuild, successor to
"Neptune".

**What it is**: an injector plus plugin system that runs *inside* the official TIDAL Electron
desktop app (`native/injector.ts` intercepts HTTPS, strips CSP `<meta>` tags, injects a bundle
into the render process).

**What it reveals about the official client** (all confirmed by direct source read):
- Credentials obtainable by walking the app's bundled webpack module tree for a function literally
  named `getCredentials`, then invoking it — returns
  `{clientId, clientUniqueKey, expires, grantedScopes, requestedScopes, token, userId}`.
- Requests carry `Authorization: Bearer <token>` and `x-tidal-token: <clientId>`.
- The desktop client's own API host is **`https://desktop.tidal.com/v1`**, query args
  `countryCode=<from redux session>&deviceType=DESKTOP&locale=<settings.language>` — but this
  query-args string is **not** appended to the playback-info call specifically.
- Playback info: `GET https://desktop.tidal.com/v1/tracks/{id}/playbackinfo
  ?audioquality={q}&playbackmode=STREAM&assetpresentation=FULL` — **no `postpaywall` suffix and
  no `countryCode`** on this one call, unlike `track()`/`lyrics()`/`artist()` which do append the
  query-args string.
- Concurrency on playbackinfo is limited to **2** via an explicit `Semaphore(2)`; a 403/404 marks
  the track permanently unavailable in a `Set`; failed requests retry exactly once after 1 s.
- Client state is a Redux store (`playbackControls`, `playQueue`, `session`, `settings`,
  `content`, `user` slices).
- `AlbumPage`: `GET /v1/pages/album?albumId=…&countryCode=NZ&locale=en_US&deviceType=DESKTOP`,
  tracklist dug out of `rows[].modules[]` where `type === "ALBUM_ITEMS"` — TIDAL's "pages" API
  returns a CMS-style module tree, which is also what Sone's `commands/pages.rs` deals with (see
  the fuller catalogue/browse-UI treatment in the main report §18-I).
- **Quality/spatial-format handling for Atmos/360 Reality Audio is real here** — this is the thing
  to copy, correcting an earlier claim in this skill that Atmos handling was "undefined
  everywhere." `ref:TidaLuna/plugins/lib/src/classes/Quality.ts` defines a seven-level ladder
  including `Quality.Atmos` (tag `DOLBY_ATMOS`) and `Quality.Sony630` (tag `SONY_360RA`) alongside
  MQA, with tag↔`audioQuality` lookup tables. `MediaItem.ts:348-375` filters Atmos/Sony630 out of
  displayable tags, and — because a spatial track's metadata tags do not reveal its *real*
  delivered quality — issues a live `playbackInfo()` call for a spatial-only track and reads
  `cache.actualAudioQuality` back before labelling it. Add `SONY_360RA` to any tag set streamboat
  builds (python-tidal's three-value enum is incomplete); the operational rule is: for a
  spatial-only track, query `playbackinfo` and read `audioQuality` back rather than trusting tags
  alone. See `api-auth-streaming.md` §9 for the full Atmos policy discussion.
- **The Redux action-namespace dump is worth mining directly for streamboat's own transport and
  queue design.** `ref:TidaLuna/plugins/lib/src/redux/types/actions/actionTypes.ts` is a ~700-line
  sorted export of the official client's entire action set. Highlights: `playbackControls/*` is a
  full transport contract (`PLAY`, `PAUSE`, `SEEK`, `SEEK_FORWARDS`/`BACKWARDS`,
  `SET_DESIRED_PAUSE_STATE` modelled separately from actual playback state,
  `MEDIA_PRODUCT_TRANSITION`/`PREFILL_MEDIA_PRODUCT_TRANSITION`); `playQueue/*` shows real queue
  semantics (`ADD_NOW`/`ADD_NEXT`/`ADD_LAST`/`ADD_AT_INDEX`, `ENABLE_SHUFFLE_MODE` vs.
  `ENABLE_SHUFFLE_MODE_AND_SHUFFLE_ITEMS` as two distinct actions,
  `FETCH_FIRST_PAGE_AND_ADD_TO_QUEUE` + `FETCH_REST_OF_THE_TRACKS_AND_ADD_TO_QUEUE` for paginated
  queue fill); `player/*` has `PRELOAD_ITEM`/`PRELOAD_NEXT_ITEM`/`PRELOAD_SUCCESS` (the official
  client prefetches too, corroborating Sone's gapless-arming design, `sone-deep-dive.md` §3b) and
  `SET_ACTIVE_DEVICE`/`SET_AVAILABLE_DEVICES`/`SET_DEVICE_MODE` (Connect device switching is
  store-level state) plus a `chromeCast/*` namespace. Read for product design, never for code.

**The hard boundary**: `ref:TidaLuna/plugins/lib.native/src/request/decrypt.ts` ships a hardcoded
master key and uses it to decrypt streams whose manifest reports
`encryptionType: "OLD_AES"` (passes `"NONE"` through, throws on anything else). **The key and
procedure are deliberately not reproduced anywhere in streamboat's docs or code.** This is
circumvention of a technological protection measure — it is what separates "a player for
subscribers" from "a ripper," and it is the exact behaviour the streamboat project brief rules
out. Strawberry's refuse-and-explain (§2 above) is the correct alternative. **Read TidaLuna for
the endpoint intelligence above and for nothing else — never port or reference `decrypt.ts`.**

Also ships `plugins/linux/src/tidalHifi.ts`, bridging to tidal-hifi's main process for MPRIS,
Discord RPC, notifications, hotkeys, CSS injection — the two projects compose.

## 5. mopidy-tidal — the headless/server precedent

`ref:mopidy-tidal` · https://github.com/EbbLabs/mopidy-tidal (formerly
`tehkillerbee/mopidy-tidal`) · Apache-2.0 · 123★ / 35 forks / 39 open issues · last push
2026-06-12 · v0.3.13 · Python ≥3.12, `Mopidy>=3.0`, `tidalapi>=0.8.10`.

**Why it matters**: the only project that proves a TIDAL backend works headlessly, driven by MPD
clients/Iris/Snapcast, with no GUI at all — directly relevant to streamboat's required headless
mode.

**Config schema** (`ext.conf`): `quality = LOSSLESS` (LOW|HIGH|LOSSLESS|HI_RES_LOSSLESS),
`auth_method = OAUTH` (OAUTH|PKCE), `login_server_port = 8989`, `lazy = false`,
`login_method = AUTO` (BLOCK|AUTO|HACK), `playlist_cache_refresh_secs = 0`, `client_id=`,
`client_secret=`, `playback_cache = false`, `playback_cache_max_entries = 1024`,
`playback_cache_buffer_bytes = 16777216`.

**Login UX for a headless daemon — the interesting design problem, solved three ways**:
`BLOCK` (block startup until login completes), `AUTO` (start unauthenticated, log in lazily), and
the **"login hack"**: while logged out, the library/search providers return a dummy
Track/Album/Artist whose title is the login message and whose cover art is a QR code of the
login URL — so *any* MPD client displays the login prompt with zero protocol extension. **This is
the cleverest single idea in the reference set for headless auth — but generate the QR code
locally when streamboat adapts it.** mopidy-tidal's own implementation sends the login URL to
`https://api.qrserver.com/v1/create-qr-code/?...` (`login_hack.py:109-110`) — a real,
fixable data leak of a one-time login link to a third-party host. tidalt's `mdp/qrterminal` (§10)
renders locally and is the pattern to actually copy.

**Stream resolution**: MPD → write `manifest.mpd` into Mopidy's cache dir, return `file://`;
BTS → `manifest.get_urls()[0]`. Logs quality/bit-depth/sample-rate per track. Pre-flight check
when `quality == hi_res_lossless`: `"HIRES_LOSSLESS" in track.media_metadata_tags`, logging a
downgrade notice if absent (`playback.py:76-84`) — a cheap check streamboat should copy (see
`api-auth-streaming.md` §7).

**The caching proxy — the only Range-seek caching design in the set**:
`ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/` is a threaded local HTTP relay in front of
`https://lgf.audio.tidal.com/`, backed by a SQLite chunk cache (`cache.py`) with an insertion
context manager that only finalizes an entry on a *complete* download, and `Range` header support
in `proxy.py` so GStreamer can seek. `translate_uri` prefers the local proxy URL when a cache
entry exists; otherwise falls back to `track.get_url()`, then to `as_stream()` on
`URLNotAvailable` (the PKCE case). Directly reusable for streamboat's headless mode.

**Borrow**: the login-hack pattern (locally-rendered QR, see caveat above); the three login
methods; the config schema shape; the Range-capable caching proxy; the pre-flight quality check.
**Avoid**: coupling to Mopidy's provider API if streamboat wants its own daemon protocol; the
hardcoded `lgf.audio.tidal.com` host (CDN hostnames change); sending login URLs to a third-party
QR service.

## 5a. Music Assistant — a second, actively-maintained headless precedent

`music-assistant/server` — its TIDAL provider source (not its docs site, which is
egress-blocked like the rest of `tidal.com`) was fetched directly from GitHub. Two things it adds
beyond mopidy-tidal: **a fourth DASH-delivery strategy** — base64-decode the manifest and serve it
from an ephemeral local HTTP route kept alive for `track.duration + 300s`, explicitly because
ffmpeg cannot re-fetch a `data:` URI mid-playback (see `api-auth-streaming.md` §5); and **TIDAL
track-ID churn handling** — track lookups are cached for days, so a churned ID passes the cached
lookup and the 404 first appears on the `playbackinfo` call itself; the rule is that 404 must
trigger `resolve_live_track_id(item_id)` and exactly one retry with the healed ID, not a hard
failure. Any client that caches catalog IDs long-term (Sone's `StaticMeta` tier at 7d TTL/30d SWR
is exactly this shape) needs this retry path. It also has real provider-level tests with a mocked
API client — more automated coverage of the streaming path than any other project in this set.
Source: `music_assistant/providers/tidal/streaming.py`, `tests/providers/tidal/
test_streaming.py`, fetched 2026-09-08.

## 5b. lms-plugin-tidal (Lyrion/Logitech Media Server) — a third server-side precedent

`michaelherger/lms-plugin-tidal`, Perl. Fetched directly from GitHub (raw), 2026-09-08. Two
transferable points: (1) a **per-request cache-TTL flag** — its generic `_get()` helper takes
`$params->{_ttl}`, and the playback call opts out with `$params->{_nocache} = 1` — the same "never
cache manifests/stream URLs" rule this skill already gives (`api-auth-streaming.md` §4), expressed
as an explicit per-request parameter rather than a convention developers must remember; copy the
mechanism, not just the rule. (2) an **anti-pattern to avoid**: its error path collapses rate
limiting into an auth failure — `$error = 'NO_ACCESS_TOKEN' if $error !~ /429/` — treating a `429`
as "your credentials are bad" with no backoff. Pair with Sone's `subStatus` taxonomy (a 4xxx
sub-status must not trigger a token refresh, `api-auth-streaming.md` §6) as the opposite failure
mode done right: **classify TIDAL's failure modes (auth vs. rate-limit vs. content-unavailable)
before acting on them; never infer a category from a substring match on an error message.**

## 6. tidal-connect (Docker) — the "be a Connect target" precedent

`ref:tidal-connect` · https://github.com/GioF71/tidal-connect · MIT (wrapper only) · 167★ / 13
forks / 8 open issues · last push 2026-03-31 · Bash + Docker Compose.

Wraps a **proprietary, closed-source binary** (`/app/ifi-tidal-release/bin/
tidal_connect_application`) shipped with an iFi Audio device certificate. The wrapper contributes
ALSA device configuration, mDNS/Avahi setup, a test-tone pre-flight, and a restart loop.

**Quality ceiling**: after TIDAL removed all MQA content at the end of July 2024, this
implementation is limited to 16/44.1 LOSSLESS; it previously reached 24/48 plus MQA unfolding to
24/88 or 24/96 (this is an independent corroboration of the July-2024 MQA-removal date used
elsewhere in this skill — see `api-auth-streaming.md` §7). The README explicitly points users at
Mopidy-Tidal, Music Assistant, upmpdcli's TIDAL plugin, or BubbleUPnP for HI_RES_LOSSLESS.

**Operational lessons that generalize**: host networking is mandatory for mDNS discovery; ALSA
card indices shift across reboots, so prefer `CARD_NAME` over `CARD_INDEX` and generate a stable
`/etc/asound.conf` at startup; if a `Master` mixer control exists, create a `SoftMaster` softvol
rather than coupling to hardware volume; play a test tone before starting to catch a locked
device. The repo carries **26** per-DAC `asound.conf` presets in `userconfig/` (44 total if the
18 in `samples/` are also counted) and a tested-device table in `assets/known-devices.md`.

**Relevance to streamboat**: there is **no open-source implementation of the TIDAL Connect
receiver protocol** anywhere found. If streamboat ever wants to be a Connect target, that's a
reverse-engineering project with no precedent to borrow from, and certificate-based device
authentication suggests it is not casually reproducible. Treat "be a Connect target" as out of
scope for now; "control a Connect target" is equally undocumented.

## 7. Official TIDAL SDKs (web, Android, iOS)

All three are **Apache-2.0**, published by TIDAL (Block, Inc.).

**tidal-sdk-web** (204★ / 24 forks / 24 open issues, updated 2026-09-07): monorepo packages, each
independently Apache-2.0 — `api`, `auth`, `common`, `event-producer`, `player`,
`player-web-components`, `template`, `true-time`. Published to npm under `@tidal-music`. Auth:
OAuth2 PKCE against `login.tidal.com`/`auth.tidal.com/v1/oauth2/token`, plus device-authorization,
plus client-credentials; scopes as an array joined with spaces; storage via a pluggable
`StorageAdapter` with AES-GCM encryption by default. Playback has two paths: legacy
(`_fetchLegacyPlaybackInfo`, native player only) and modern (`_fetchTrackManifest`,
`GET /trackManifests/{id}` on `openapi.tidal.com/v2/` with `adaptive`, `formats`,
`manifestType: isFairPlaySupported ? 'HLS' : 'MPEG_DASH'`, `shareCode`, `uriScheme: 'DATA'`,
`usage: 'PLAYBACK'`, header `x-playback-session-id`). **Quality → formats mapping — worth copying
verbatim as an abstraction**:
```
HI_RES / HI_RES_LOSSLESS -> ['HEAACV1','AACLC','FLAC','FLAC_HIRES']
LOSSLESS                 -> ['HEAACV1','AACLC','FLAC']
HIGH                     -> ['HEAACV1','AACLC']
LOW / default            -> ['HEAACV1']
```
The client sends a *set of acceptable formats* and reads back what it got, rather than asking for
a tier and hoping — strictly better than the unofficial API's single `audioquality` string;
streamboat can emulate this pattern even against v1. DRM: Widevine license at
`https://api.tidal.com/v2/widevine`; FairPlay certificate at `https://fp.fa.tidal.com/certificate`
and license at `https://fp.fa.tidal.com/license` — both carry a `// TODO: update DRM URLs from
manifest response if changed` in the source, so **don't hardcode DRM licence URLs even if you
were building against the official API**; read them from the manifest. Three player backends:
`browser`, `shaka` (DASH/HLS + DRM), `native` (a proprietary `window.NativePlayerComponent` that
is *not* in this repo — **the tell that TIDAL's own desktop app uses a closed-source native
player component the open SDK only declares an interface for**; third parties get browser/Shaka).

**tidal-sdk-android** (49★ / 4 forks / 5 open issues, updated 2026-09-03): Kotlin,
Retrofit+OkHttp+Dagger+kotlinx.serialization, ExoPlayer (`androidx.media3`) for playback,
`EncryptedSharedPreferences` for tokens, a `TokenMutex` serializing refresh. Canonical
`ManifestMimeType` enum here: `EMU/BTS/DASH/HLS` (see `api-auth-streaming.md` §5). Notable design
ideas: separate `streamingWifiAudioQuality` vs `streamingCellularAudioQuality` policies;
`usage=playback` vs `usage=download` on the same manifest endpoint; offline entries carry
`offlineRevalidateAt`/`offlineValidUntil` — the officially-sanctioned offline-validity shape, if
streamboat ever does offline caching (contrast with High Tide's caching, §1 above, which has
none of this).

**tidal-sdk-ios** (41★ / 13 forks / 13 open issues, updated 2026-08-24, v0.12.6): Swift,
`Sources/{Auth,Common,EventProducer,Offliner,Player,Template,TidalAPI}`. Dependencies: GRDB.swift
≥6.27, SWXMLHash ≥7.0.2, KeychainAccess ≥4.2.2, **Kronos 4.2.2 (NTP time sync — because DRM
licence validity cannot trust the device clock)**, swift-log, AnyCodable. Playback is
`AVQueuePlayer`; DRM is FairPlay against `fp.fa.tidal.com`. The `Offliner` module is the only
*complete* offline-download implementation in the whole reference set, and it's the officially
sanctioned one.

**Can third parties actually use these SDKs?** Legally the *code* is Apache-2.0 — vendor, fork,
modify freely. But the *service* refuses: a developer-portal `clientId`, the `playback` scope,
and per the Developer Guidelines the Player module must be "official, unmodified" — third parties
get previews (`api-auth-streaming.md` §1). Forking the SDK to point at the unofficial v1 API would
be an odd hybrid: Apache-2.0 code calling endpoints the same licensor's own terms forbid calling
that way.

**Borrow**: the `formats[]+manifestType+uriScheme+usage` request abstraction; the
`ManifestMimeType` enum; the `CredentialsProvider` interface boundary (auth knows nothing about
playback; player depends on an interface, not a session singleton); the token-refresh
mutex/session-ID guard; NTP time sync for anything expiry-sensitive; the offline
validity-window model. **Avoid**: assuming the official endpoints will serve full tracks;
hardcoding DRM licence URLs.

## 8. python-tidal (tidalapi) — the de-facto unofficial API library

`ref:python-tidal` · https://github.com/EbbLabs/python-tidal (moved from `tamland/python-tidal`)
· LGPL-3.0-or-later · 560★ / 124 forks / 21 open issues · last push 2026-08-14 · v0.8.11 ·
maintainer `tehkillerbee`.

Runtime deps deliberately tiny: `requests`, `python-dateutil`, `typing-extensions`, `isodate`,
`mpegdash`, `pyaes`.

**Credentials**: four values (`client_id`, `client_secret`, `client_id_pkce`,
`client_secret_pkce`), each a **double-base64-encoded, split-in-two** byte literal reassembled at
runtime — same obfuscation category as Sone's XOR. Falls back to `client_id` if `client_secret`
is empty.

**Config**: `api_oauth2_token`, `api_pkce_auth`, `api_v1_location`, `api_v2_location`,
`openapi_v2_location`, `pkce_uri_redirect = https://tidal.com/android/login/auth`, `image_url`,
`video_url`, `listen_base_url`, `share_base_url` — full list in `api-auth-streaming.md`.
`item_limit` clamped to 10000 with a warning. An `alac` flag whose docstring warns that
`alac=false` turns video streams into audio-only and `num_videos` into `num_tracks` in playlists.

**Quality enum**: `LOW, HIGH, LOSSLESS, HI_RES_LOSSLESS`, default `HIGH`. **`HI_RES` (MQA) is gone
from the enum entirely.** Separately, `MediaMetadataTags` are `HIRES_LOSSLESS`, `LOSSLESS`,
`DOLBY_ATMOS`, and `AudioMode` is `STEREO`/`DOLBY_ATMOS`. **Enum-vs-tag naming mismatch**
(`HI_RES_LOSSLESS` vs `HIRES_LOSSLESS`) — always use lookup tables, never string equality (see
`api-auth-streaming.md` §7).

**`StreamManifest`** is the reference implementation of manifest handling: MPD parsed with
`mpegdash`, `codecs` mapped (`flac`→FLAC; `mp4a.40.5`=LOW 96k, `mp4a.40.2`=HIGH 320k, both→MP4A),
`audio_sampling_rate` read, and **`encryption_type` hardcoded to `"NONE"` with a `# TODO: Handle
encryption key`** comment — do not treat this as proof a DASH stream is actually unencrypted, it's
an unfinished code path, not a verified fact. BTS reads `urls`/`codecs`/`mimeType`/
`encryptionType`/`keyId`. `DashInfo` can also emit an HLS m3u8. Typed exceptions:
`ObjectNotFound`, `URLNotAvailable`, `StreamNotAvailable`, `UnknownManifestFormat`,
`TooManyRequests`, `MPDNotAvailableError`.

**PKCE and `urlpostpaywall` are mutually exclusive**: `Track.get_url()` raises `URLNotAvailable`
immediately when the session is PKCE.

**LGPL-3.0 matters for streamboat**: dynamic linking/separate-process use is fine under LGPL even
inside a differently-licensed application; statically vendoring or porting the code creates
obligations. If streamboat is not Python, treat this as a reference document, not a dependency.

## 9. tidal-cli — the official-API CLI + MCP precedent

`ref:tidal-cli` · https://github.com/lucaperret/tidal-cli · MIT · 10★ / 4 forks · created
2026-03-16 · last push 2026-08-23 · v1.2.5 · TypeScript, Node ≥20, Commander 14.

Uniquely in this set, uses the **official** SDK: `@tidal-music/api ^0.22.0` +
`@tidal-music/auth ^1.6.0`, hardcoded public client ID `PYVtmSHMTGI9oBUs`, PKCE, loopback redirect
`http://localhost:17893/callback`, scopes `collection.read/write`, `playlists.read/write`,
`playback`, `user.read`, `recommendations.read`, `entitlements.read`, `search.read/write`.

Playback: `GET /trackManifests/{id}` with `adaptive=false, formats=<by quality>,
manifestType=MPEG_DASH, uriScheme=DATA, usage=PLAYBACK`, base64-decodes the `data:` URI, handles
both BTS JSON and DASH XML, and for DASH extracts `initialization="…"`, `media="…$Number$…"` and
`<S d= r=>` repeat counts, downloads every segment sequentially, concatenates into a local
`.flac`/`.mp4`. **It carries `trackPresentation` and `previewReason`** and prints "Preview
reason:" — direct evidence the official path frequently returns previews rather than full tracks
(see `api-auth-streaming.md` §1).

Node-specific hacks: `@tidal-music/auth` expects browser globals, so `session.ts` installs a
`localStorage` polyfill backed by `~/.tidal-cli/session.json` (mode 0600) plus
`CustomEvent`/`EventTarget` polyfills.

**Borrow**: the CLI-command → `*Data()` function split so the same code backs the CLI and an MCP
server; the loopback-redirect PKCE flow; the cursor-pagination helper with bounded retries.
**Avoid**: taking its playback path as proof the official API works for a real player — the
`previewReason` field says otherwise.

## 10. tidalt — Go TUI + daemon, bit-perfect ALSA

`ref:tidalt` · https://github.com/Benehiko/tidalt · Apache-2.0 · 2★ / 4 forks / 0 open issues ·
created 2026-03-13 · last push 2026-09-06 · Go 1.26, ~13,078 lines Go in `internal/`+`cmd/` plus
263 lines of C (111 in `alsa.c`, 152 in `avcodec.c` — a prior pass misattributed the full total to
`alsa.c` alone).

README is candid: written almost entirely with LLM coding assistants, and *"Linux only. Requires
a Tidal HiFi or HiFi Plus subscription."*

**Auth/streaming is unofficial**, with one caveat. `BaseURL` is `api.tidal.com/v1`, the stream
endpoint is `GET /tracks/{id}/urlpostpaywall?urlusagemode=STREAM&audioquality=<q>
&assetpresentation=FULL&countryCode=<cc>`, client ID hardcoded in plaintext (extracted from an
official app, not issued to this project). **Caveat**: `client.go` also defines
`BaseURLV2 = https://openapi.tidal.com/v2`, and it's actually used — `/userRecommendations/
me/relationships/myMixes` and `/playlists/{id}/relationships/items` both hit the official v2
host. So: auth and streaming are entirely unofficial; mixes/playlist-item relationships hit the
official v2 host — a hybrid, not a clean split.

**Stack**: Bubble Tea 1.3.10 + Bubbles 1.0.0 + Lipgloss 1.1.0 (TUI); `godbus/dbus/v5` (MPRIS2 +
PipeWire device reservation); `golang.org/x/oauth2` (device flow); `go.etcd.io/bbolt` (metadata
cache); `docker/secrets-engine/store` for keyring with a `filippo.io/age`-encrypted file fallback;
`mdp/qrterminal/v3` for the device-code QR in the terminal (rendered locally — the pattern
mopidy-tidal's login-hack should have used, §5 above). FFmpeg (libavformat/libavcodec/
libswresample) via cgo for decode, `staticav` build tag for distro packages bundling a static
FFmpeg; ALSA via cgo (`-lasound`).

**Architecturally the most interesting thing in the whole set**: the daemon/client split. There is
no separate always-running daemon by default — **whichever process claims the D-Bus name
`org.mpris.MediaPlayer2.tidalt` first becomes the server**; a later invocation gets
`ErrAlreadyRunning` and switches to client mode. Name-claiming is simultaneously the mutex and the
discovery mechanism, because exactly one process may own an ALSA `hw:` device. Modes: `tidalt`
(TUI, server-or-client depending on who's first), `tidalt daemon` (headless server only),
`tidalt play <url>` (a one-shot client forwarding a `tidal://` URL over D-Bus in milliseconds and
exiting — what a registered browser URL handler actually invokes), `tidalt setup`/`tidalt setup
--daemon` (XDG handler / systemd `--user` service registration). Control surfaces: MPRIS2
(`org.mpris.MediaPlayer2.tidalt`, `SupportedUriSchemes: ["tidal"]`) plus a private `io.tidalt.App`
interface for everything MPRIS can't express. This is *exactly* the desktop-plus-headless shape
streamboat needs, generalized — and it's the only implementation of it in the set. **Portability
caveat**: the D-Bus name-claim trick is Linux-only; Windows/macOS need an equivalent single-
instance mutex plus a local transport (lock file + named pipe/Unix socket — similar to Sone's
`tauri-plugin-single-instance`). `ref:tidalt/docs/phone-control.md` also notes KDE Connect/
GSConnect already bridge MPRIS2 to an existing phone companion app with zero app-specific code —
a free partial answer to remote control while mobile is out of scope. Source:
`ref:tidalt/docs/client-server.md`, `ref:tidalt/docs/mpris2.md`, `ref:tidalt/docs/
phone-control.md`.

**Two more transferable ALSA findings — see `audio-engineering.md` §7 for the full detail**:
PipeWire device reservation (`org.freedesktop.ReserveDevice1.Audio<N>`) before opening `hw:`
exclusively, **released on pause, not only on stop** (`ref:tidalt/README.md:12` — "holds exclusive
access to the audio device only while a track is actually playing"; an earlier draft of this
skill said "release on stop," which is the less cooperative and incorrect version); and
distinguishing a format-negotiation refusal (fall back to `plughw:`, mark not-bit-perfect) from a
device-busy error (keep retrying `hw:`), memoized per device.

Packaging: `docker-bake.hcl` producing `.deb`/`.pkg.tar.zst`/`.rpm` for amd64+arm64 plus a Docker
image; a systemd user service; a `tidal://` URL handler.

## 11. tidalrs — Rust API client library

`ref:tidalrs` · https://github.com/phayes/tidalrs · MIT · 19★ / 7 forks / 2 open issues ·
created 2025-09-16 · last push 2026-09-02 · v0.5.0 · published on crates.io/docs.rs.

Deps: `reqwest 0.12` (rustls), `tokio 1.47`, `serde`, `thiserror 2`, `strum 0.27`, `arc-swap 1`,
`stream-download 0.22.4`, `base64 0.22`, `url 2.5.7`, `async-recursion`. Dev-deps include
`rodio 0.21` (only for `examples/audio_streaming.rs`) and `mockito`.

Three stream endpoints against `api.tidal.com/v1`: `/tracks/{id}/urlpostpaywall`,
`/tracks/{id}/playbackinfo`, `/tracks/{id}/playbackinfopostpaywall`. Device-flow auth with scope
`"r_usr w_usr w_sub"`; tokens held in an `Arc<Authz>` swapped via `arc-swap` (lock-free reads);
refresh serialized by a `Semaphore` to avoid a thundering herd; exponential backoff from 100 ms
(configurable ceiling, default 5 s); `Authz` is `Serialize` so the *application* owns persistence.

README: *"This library is not officially affiliated with Tidal. Use at your own risk and ensure
compliance with Tidal's Terms of Service."*

**The single most reusable dependency for a Rust streamboat**: MIT (no copyleft), async, typed,
actively released, deliberately stops at "return a manifest/URL" — no decrypt, no download, no
play. Main gaps: lyrics, mixes/radio, videos, DASH parsing (left to the consumer). **Risk**: 19★,
**4 contributors, `phayes` 60 of 67 commits — bus factor 1** (confirmed via the GitHub
contributors API, `sone-deep-dive.md` §10), v0.5.0 — treat as a fork candidate, not a load-bearing
dependency, and re-check its release health periodically.

## 12. TidalSwift — Apple-platform precedent (read-only, do not copy)

`ref:tidalswift` · https://github.com/melgu/TidalSwift · 97★ / 12 forks / 7 open issues · last
push 2026-07-08 · Swift + SwiftUI, macOS/iOS.

**No LICENSE file exists in the repository** (verified by `ls -a`) — effectively *all rights
reserved by default*. **Do not copy code from it.** Read for architecture only.

Architecture worth noting: a clean split between `TidalSwiftLib` (API + models + offline DB) and
the `TidalSwift` app (SwiftUI + `Player`). Device OAuth flow with hardcoded plaintext credentials
(both client ID **and secret**) in `Config.swift`. Streaming uses the oldest endpoint,
`GET /v1/tracks/{id}/streamUrl` with `soundQuality`. Playback is `AVPlayer` — no exclusive/hog
mode is possible through that API surface at all, which is why this project cannot demonstrate
macOS bit-perfect output even though it's a real macOS TIDAL client (see
`audio-engineering.md` §6 for what macOS exclusive output actually needs). Has a real
download-and-tag feature (SwiftTagger) and an `OfflineDB` with reference counting so a track
favourited from two playlists is stored once — the reference-counting idea is genuinely good even
though the feature itself is out of streamboat's scope. Latest commit (2026-07-08): "Fix token
refresh when app is open for a long time" — a reminder that long-lived-session refresh is a real
bug class worth testing deliberately.

## 13. Smaller / historical projects

| Project | License | Stars | Last activity | Status |
| --- | --- | --- | --- | --- |
| `libopentidal` (Fokka-Engineering/libopenTIDAL) | MIT | not resolvable via GitHub API this session | 2021-05-25 | Dead. ANSI C, libcurl-only. Documents `playbackinfopostpaywall` **and** `playbackinfoprepaywall` (a fifth manifest variant seen nowhere else, `Source/OTService/OTServiceStd.c:165-167`). Device flow with a 5-minute pre-expiry refresh buffer. Per-thread curl handles. |
| `tidalgo` (tcpj/tidalgo) | none stated | 1★ | last commit 2018-02-06 (GitHub `updated_at` separately reports 2019-05-09 — two different metrics; cite one and label it) | Dead. `api.tidalhifi.com/v1/`, username/password + `X-Tidal-SessionId`. Source comment records FLAC via the standard TIDAL key is **encrypted** while the "WiMP" key returns unencrypted FLAC — historical evidence the encryption behaviour is client-ID-dependent, matching Strawberry's user-facing message (§2 above). |
| `dotnet-tidal-usdk` (SacredSkull) | MIT with an explicit anti-piracy clause | 0★ | 2020-05-07 | Dead. `/v1/tracks/{id}/streamUrl` with `soundQuality`, Android token `kgsOOmYk3zShYrNP`, `clientUniqueKey vjknfvjbnjhbgjhbbg`, LOW/HIGH only. The licence-with-a-piracy-clause is a precedent worth considering for streamboat's own licence text. |
| `tidal-api-docs` (gkasdorf) | none | 0★ | content 2023-04-08, repo touched 2026-07-13 | 4 markdown files. Documents PKCE with client ID `CzET4vdadNUFQ5JU`, redirect `https://listen.tidal.com/login/auth`, `appMode=WEB`, token endpoint `https://login.tidal.com/oauth2/token`, scope `r_usr w_usr`, 24-hour access-token TTL, a manual "copy the token out of devtools" fallback. Explicitly warns the client ID changes without notice and that CORS blocks pure-browser flows. No streaming documentation at all. |
| `tidal-fokka-engineering-` (Fokka-Engineering/TIDAL) | MIT | not resolvable | 2022-02-10 | Two files. Historical value: *"I've decompiled various TIDAL App Versions and debundled the Browser JS App"* and *"I reversed engineered the TIDAL device authorization grant (RFC 8628) since the web flow (RFC 6749) is reCaptcha v3 secured."* Disclaimer: *"I deeply discourage you from building and distributing copyright-infringing apps. Create something that adds up to TIDALs Service and improves it."* |

Also encountered but not in the reference set, all *(unverified, not read)*: `pauljhdrake/
low-tide` (a terminal UI TIDAL client), `yaronzz/Tidal-Media-Downloader` (a downloader —
explicitly out of scope, cited only for its 2026-03-21 API-key breakage report, see
`verification-notes.md`), `GioF71/upmpdcli-docker` TIDAL plugin. `michaelherger/lms-plugin-tidal`
and Music Assistant's TIDAL provider were subsequently fetched and read — see §5a/§5b above.
