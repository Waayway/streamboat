# Comparison tables (corrected)

These reproduce the main report's comparison tables with the fact-check pass's corrections
already applied — see `docs/research/oss-landscape.md` "Comparison tables" and §18 for the
uncorrected originals and the reasoning behind each fix. Per-project narrative is in
`project-profiles.md` and `sone-deep-dive.md`.

## Table of contents

1. Table A — overall
2. Table B — auth and secrets
3. Table C — audio output

## 1. Table A — overall

| Project | Stack | Platforms | Quality ceiling | Bit-perfect | Feature breadth | License | Stars | Maturity | Fit for streamboat |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **Sone** | Tauri 2 + Rust + React 19/TS | Linux desktop | HI_RES_LOSSLESS 24/192 | **Yes** (exclusive ALSA) | Very high (~28 areas) | GPL-3.0-only | 437 | Active, 1 maintainer, v0.21.0, **no CI at all** | **Closest architectural model — but invert its state-ownership shape, see `sone-deep-dive.md` §3** |
| **sone-windows** | same, forked | Windows (+Linux/mac via Tauri) | HI_RES_LOSSLESS | Yes (WASAPI2 exclusive) | High, minus 8 modules | GPL-3.0-only | n/a | Stale fork of v0.16.0, "may not be maintained" | Windows sink + DLL bundling only |
| **High Tide** | Python + GTK4/libadwaita + python-tidal | Linux (Flatpak) | HI_RES_LOSSLESS | No | High | GPL-3.0 | 671 | Active community, 90 open issues | UI/UX + Flatpak + PKCE model |
| **Strawberry** | C++17 + Qt 6 + GStreamer | Linux free; macOS/Windows builds **sponsor-only** | HI_RES_LOSSLESS | **No dedicated path** — WASAPI-exclusive is an explicit Windows setting; Linux "exclusive" is only inferred from an `hw:`/`plughw:` device prefix; macOS has none | TIDAL = one backend of many | GPL-3.0 | 3,948 | Very mature, very active | Encryption posture + settings model + Windows WASAPI-exclusive reference |
| **tidal-hifi** | Electron (castlabs Widevine) + TS | Linux, Windows, macOS | Whatever the web player serves (Chromium resamples to 48k by default) | No | Medium-high | MIT | 1,725 | Very active, real CI | Anti-model for audio; borrow the local API + controller-fallback pattern + packaging CI checklist |
| **TidaLuna** | TS mod inside official client | Win/mac (+Linux via tidal-hifi) | Official client's | n/a | Plugin platform | MS-PL | 591 | Active | Intelligence only; contains a DRM key — never port |
| **mopidy-tidal** | Python + Mopidy + GStreamer | Linux/macOS/Windows headless | HI_RES_LOSSLESS | No | Medium | Apache-2.0 | 123 | Active | **Headless mode model** |
| **tidal-connect** | Bash + Docker + proprietary binary | Linux ARM/x86 | LOSSLESS 16/44.1 (post-MQA) | Partly (direct ALSA) | Low (receiver only) | MIT (wrapper) | 167 | Maintained wrapper, dead binary | ALSA config lore only |
| **tidal-sdk-web** | TypeScript | Browsers (+ native bridge) | HI_RES_LOSSLESS *in theory* | No | SDK | Apache-2.0 | 204 | Active (TIDAL) | API abstraction shapes |
| **tidal-sdk-android** | Kotlin + ExoPlayer | Android 7+ | HI_RES_LOSSLESS + Atmos | n/a | SDK + offline | Apache-2.0 | 49 | Active (TIDAL) | Offline validity model |
| **tidal-sdk-ios** | Swift + AVPlayer | iOS/macOS/tvOS/watchOS | HI_RES_LOSSLESS + Atmos | n/a | SDK + offline | Apache-2.0 | 41 | Active (TIDAL) | Future mobile reference |
| **python-tidal** | Python library | any | HI_RES_LOSSLESS | n/a | API only | LGPL-3.0+ | 560 | Active | Endpoint reference |
| **tidal-cli** | TS + official SDK | CLI (Linux/mac/Win) + MCP | Official API (previews in practice) | No | Medium (CLI breadth) | MIT | 10 | Active, young | CLI/MCP structure |
| **tidalt** | Go + FFmpeg/ALSA cgo + Bubble Tea | Linux only | HI_RES_LOSSLESS | **Yes** | Medium | Apache-2.0 | 2 | Young, AI-authored | **Daemon/TUI split model — closest to streamboat's shape, minus a GUI** |
| **tidalrs** | Rust library | any | HI_RES_LOSSLESS | n/a | API only | **MIT** | 19 | Young, active, single author | **Best Rust dependency candidate — verify health before committing** |
| **TidalSwift** | Swift + SwiftUI + AVPlayer | macOS/iOS | HI_RES_LOSSLESS (claimed) | No (AVPlayer has no exclusive mode) | Medium + downloads | **none** | 97 | Low-activity | Read-only; do not copy |
| **libopenTIDAL** | ANSI C + libcurl | any | HI_RES | n/a | API only | MIT | — | Dead (2021) | Historical |
| **tidalgo** | Go | any | LOSSLESS | n/a | Minimal | none | 1 | Dead (2018) | Historical |
| **dotnet-tidal-usdk** | C#/.NET Core | any | HIGH | n/a | Minimal | MIT + anti-piracy clause | 0 | Dead (2020) | Licence-clause precedent |

**Read the "Maturity" column as age + release cadence + CI, not community size.** Contributor data
(`sone-deep-dive.md` §10) shows Strawberry's "very mature, very active" rating is not a better bus
factor than Sone's — `jonaski` has 5,678 commits vs. the next *human* contributor at 28; the #2
entry by commit count is Strawberry's own bot. Of every project examined, only High Tide (45
contributors) and mopidy-tidal (12, a genuine 3-person history) are not effectively
single-maintainer.

## 2. Table B — auth and secrets

| Project | Flow(s) | Client credentials | Token storage |
| --- | --- | --- | --- |
| Sone | device code, PKCE (window + browser), token import | Embedded, XOR-masked; user-overridable in settings | AES-256-GCM `settings.json`; key in OS keyring + 0600 file |
| High Tide | PKCE only | From `tidalapi` (double-base64) | libsecret, JSON blob, schema `io.github.nokse22.high-tide` |
| Strawberry | PKCE authorization code, redirect `tidal://login/auth` | Compile-time `TIDAL_CLIENT_ID` or user-supplied; **no secret** | QSettings via Strawberry's credential encryption |
| tidal-hifi | none (browser session) | none | Chromium cookies/localStorage |
| TidaLuna | none (scrapes host app) | scraped `getCredentials()` | host app's |
| mopidy-tidal | device code or PKCE | config file, user-supplied or from `tidalapi` | `tidal-oauth.json` / `tidal-pkce.json`, plaintext |
| tidalt | device code | plaintext, extracted from an official app | keyring, else age-encrypted file |
| tidalrs | device code | caller supplies | caller serializes `Authz` |
| tidal-cli | official PKCE, loopback :17893 | plaintext public `PYVtmSHMTGI9oBUs` | `~/.tidal-cli/session.json` (0600) |
| TidalSwift | device code | plaintext id **and secret** in `Config.swift` | app storage |
| official SDKs | PKCE, device, client-credentials | developer-portal issued | Keychain / EncryptedSharedPreferences / AES-GCM localStorage |

## 3. Table C — audio output (corrected)

| Project | Decode | Output | Gapless | Sample-rate switching | Normalization |
| --- | --- | --- | --- | --- | --- |
| Sone Normal | GStreamer `uridecodebin` | `autoaudiosink` | Yes (`concat` preroll) | delegated to the sound server | `volume` element, Sone's formula |
| Sone DirectAlsa | GStreamer → `appsink` | own ALSA writer thread | **No** (disabled) | Yes — reopens PCM per format hint | scalar in writer (off in bit-perfect) |
| sone-windows | GStreamer | `wasapi2sink exclusive=… low-latency=true` | Yes | via WASAPI exclusive | same |
| High Tide | `playbin3` | auto/pulse/alsa/jack/oss/pipewire | Yes via `about-to-finish` (off on pipewiresink) | No | `rgvolume`/`rglimiter` chain |
| Strawberry | GStreamer `playbin3` | per-platform; WASAPI-exclusive on Windows (explicit setting), inferred `hw:`/`plughw:` "exclusive" on Linux (not bit-perfect by itself), no exclusive path on macOS | Yes | No dedicated rate-matching — always two `audioresample` elements, unconstrained caps | ReplayGain **or** EBU R128 (R128 forces float caps, incompatible with bit-perfect) |
| tidal-hifi | Chromium | Chromium → PulseAudio/PipeWire | No | Only via a 192k Chromium flag + manual PW/Pulse reconfiguration | web player's |
| mopidy-tidal | GStreamer (Mopidy) | Mopidy's output | Mopidy's | No | No |
| tidalt | FFmpeg (cgo) | direct `snd_pcm` `hw:` with `plughw:` fallback | No | Yes, per-track negotiation | No |
| tidal-connect | proprietary binary | ALSA + optional softvol | binary's | No | softvol only |
