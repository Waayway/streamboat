# Open-source TIDAL client landscape: architectural precedents for streamboat

Research date: 2026-09-07. All "verified" claims below were read from the read-only reference
checkouts under `ref:<project>/<path>` (shallow clones taken 2026-09-07) or from cited URLs.
Claims marked *(unverified)* could not be confirmed from source or reachable documentation.

This report has been through one independent fact-check pass (two reviewers, 2026-09-07): every
numbered/quoted claim was checked against the reference checkouts, the GitHub API, or a direct
fetch, and each was marked confirmed, refuted, or uncertain. All refuted claims below have been
corrected in place (the wrong figure is not left standing anywhere); corrections are also
collected in **§18 Verification-pass corrections and new findings** for a single audit trail.
Facts the fact-check turned up that the original pass missed ("gaps") have been folded into the
relevant sections and are cross-referenced from §18 too. Items that remain genuinely unresolved
are marked **uncertain** where they occur and are collected in *Open questions*.

**The entire `tidal.com` domain is blocked by this environment's egress proxy — not only
`developer.tidal.com` and `support.tidal.com`.** `https://tidal.com/content-guidelines` returns
`EGRESS_BLOCKED` exactly like the developer subdomain. Every official-policy quote below is
therefore second-hand (a GitHub discussion quoting the guidelines verbatim, or a search-result
summary); treat them as high-confidence-but-secondary and re-check them directly, from an
unproxied network, before they go into user-facing legal/README text.

---

## Summary

1. **Two disjoint TIDAL APIs exist.** The *official* developer platform (`openapi.tidal.com/v2`,
   OAuth2 PKCE, documented) and the *unofficial* legacy API (`api.tidal.com/v1`,
   `api.tidalhifi.com/v1`, `desktop.tidal.com/v1`) that every working third-party player uses.
   They are not interchangeable.
2. **The official API will not give a third party full-track audio today.** TIDAL's Developer
   Guidelines state that playback is only permitted through "an official, unmodified version of
   the TIDAL Player module", and that third-party apps using it "can include playback of TIDAL
   previews". Developers on `tidal-music` discussion #179 report the app-review process was
   still non-functional as of April 2026. This is the single most important constraint on
   streamboat's architecture.
3. **Every full-quality open-source client uses the unofficial v1 endpoint
   `GET /tracks/{id}/playbackinfopostpaywall`** with `audioquality`, `playbackmode=STREAM`,
   `assetpresentation=FULL` (+ `countryCode` where required). Verified in Sone, Strawberry,
   python-tidal, tidalrs, libopenTIDAL and TidaLuna (the latter against `desktop.tidal.com/v1`
   without the `postpaywall` suffix).
4. **Two manifest formats.** `application/vnd.tidal.bts` (base64 → JSON with `urls[]`, `codecs`,
   `mimeType`, `encryptionType`, `keyId`) and `application/dash+xml` (base64 → MPD XML).
   Official v2 adds `application/vnd.tidal.emu` and HLS. Every client must handle at least BTS
   and DASH.
5. **Quality ladder is `HI_RES_LOSSLESS` → `HI_RES` → `LOSSLESS` → `HIGH` → `LOW`.** Sone's
   cascade drops both Hi-Res tiers when no `client_secret` is present, because those credentials
   return encrypted DASH requiring Widevine (`ref:sone/src-tauri/src/commands/playback.rs:32-42`).
   python-tidal 0.8.11 has dropped `HI_RES` (MQA) from its `Quality` enum entirely.
6. **Encryption is the fault line.** Strawberry *refuses* any stream whose `encryptionType`,
   `securityType` or `encryptionKey` is non-empty and shows a user-facing message. TidaLuna
   *decrypts* with a hardcoded AES-256-CBC master key. streamboat must follow Strawberry, not
   TidaLuna.
7. **Sone (GPL-3.0-only, 437★, v0.21.0, last push 2026-09-05)** is the closest architectural
   match to streamboat's brief: Tauri 2 + Rust backend + React 19/TS frontend, GStreamer 0.23
   decode, and a hand-written ALSA writer thread for exclusive/bit-perfect output. ~26.7k lines
   Rust, ~47.6k lines TS, 151 Rust `#[test]`s, 46 frontend test files.
8. **Sone uses no Rust audio crate stack** (no symphonia, no cpal, no rodio). It is
   `gstreamer`/`gstreamer-app` 0.23 for decode + the `alsa` 0.10 crate driving `snd_pcm` directly
   in a dedicated writer thread. Two backends: `Normal` (autoaudiosink + `concat` gapless) and
   `DirectAlsa` (appsink → PCM → ALSA writer, gapless disabled).
9. **Sone's bit-perfect logic is the most sophisticated artifact in the landscape** — DAC format
   and rate probing via `HwParams::test_format`/`test_rate`, a lossless-promotion table, an
   ALSA↔GStreamer 24-bit naming inversion (ALSA `S24LE` = GStreamer `S24_32LE`), explicit
   `sw_params` `start_threshold` pre-fill, and actionable error messages instead of "internal
   data stream error".
10. **High Tide (GPL-3.0, 671★, last push 2026-08-21)** is the highest-star native Linux client:
    Python + GTK4/libadwaita + Blueprint, `python-tidal` for API, GStreamer `playbin3` with
    `about-to-finish` gapless and a swappable `parse_bin_from_description` audio bin. PKCE-only
    login. Tokens in libsecret. Flathub-shipped. 8 translations (the prior survey's "18" is wrong).
11. **High Tide silently and unconditionally caches every played track to disk** — this is not a
    user-chosen "music directory" opt-in (the original pass's framing); `utils.MUSIC_DIR` is
    unconditionally `~/.cache/high-tide/music/` and there is no setting to turn caching off. It
    writes `{track_id}_{quality}.m4a` via `ffmpeg -c copy` on the MPD manifest or a raw
    `requests.get` on the BTS URL (`ref:high-tide/src/lib/player_object.py:471,529-577`), skipped
    only on a metered network connection. It is bounded, not unbounded: a startup thread evicts by
    access time down to 5 GB (`ref:high-tide/src/window.py:223`, `ref:high-tide/src/lib/utils.py:
    828-843`). Net effect: an always-on, unencrypted, non-consented, size-capped full-quality media
    cache with no user visibility. streamboat must treat this as a deliberate, documented policy
    decision, not inherit it as a default.
12. **sone-windows is a stale fork, not a port layer.** It is Sone v0.16.0 (upstream is 0.21.0),
    missing MCP, OBS overlay, signal-path, theme file, TIDAL play reporting, profile, feed and
    updater modules. Its Windows sink is GStreamer `wasapi2sink` with `exclusive` and
    `low-latency` properties (the prior survey's "wasapisink" is wrong). One codebase *could*
    serve both — the divergence is `#[cfg(target_os)]`-shaped, not architectural.
13. **tidal-hifi (MIT, 1725★) is the highest-star project** and the only one that plays
    DRM-protected TIDAL content legitimately: it wraps the web player in castlabs'
    Widevine-enabled Electron (`github:castlabs/electron-releases#v43.0.0+wvcus`). It adds
    MPRIS, Discord RPC, ListenBrainz, hotkeys, themes and a local Express API on port 47836.
14. **TidaLuna (MS-PL, 591★) is a mod of the official TIDAL desktop client**, not a client. It
    scrapes credentials out of the running app's webpack module tree, calls
    `desktop.tidal.com/v1`, and contains a hardcoded `OLD_AES` master key. Useful only as
    intelligence about the official client's internals; nothing in it is safe to copy.
15. **mopidy-tidal (Apache-2.0, 123★) is the headless precedent**: it proves a server-mode TIDAL
    backend works on GStreamer, and its HTTP caching proxy in front of `lgf.audio.tidal.com`
    with a SQLite chunk cache is the only Range-seek caching design in the set.
16. **Official SDKs are Apache-2.0 and technically reusable, but policy-locked.** tidal-sdk-web
    (204★), -android (49★), -ios (41★). Their *code* is permissively licensed; their *service*
    is not. Their `formats`/`manifestType`/`uriScheme`/`usage` query shape is worth copying as an
    abstraction even if the endpoint is not usable.
17. **Credential handling splits three ways**: embedded-and-obfuscated (Sone XOR masks,
    python-tidal double-base64), embedded-in-plaintext (tidalt, tidal-cli, TidalSwift,
    Strawberry's compile-time `TIDAL_CLIENT_ID`), or none-at-all (tidal-hifi, TidaLuna). All
    three carry different risk profiles; none is "safe".
18. **At-rest secret storage is a solved problem with two proven patterns**: OS keyring
    (`keyring` crate / libsecret / Keychain / EncryptedSharedPreferences) as primary, and an
    encrypted file as fallback. Sone does both (AES-256-GCM, `SONE` magic header, key in keyring
    with `config_dir/sone.key` 0600 fallback).
19. **The unofficial API breaks.** A breakage of shared client IDs was reported against
    Tidal-Media-Downloader on 2026-03-21 (one reply-less issue about one public gist of keys —
    corrected from an earlier "widely-reported" characterization; treat it as one data point, not
    a trend, see §18-K), and Sone's code carries a "legacy sign-in" notice pushing users off the
    device-code flow after repeated failures. Any streamboat design must still assume auth flows
    and client IDs will churn and must make them user-replaceable — that caution does not depend
    on how widely this one incident was reported.
20. **Nobody has built what the owner asked for, precisely.** No project in the set ships a GUI
    desktop app (Linux+Windows+macOS) *and* a headless daemon from one codebase. The nearest split
    is Sone (Linux desktop) + mopidy-tidal (headless) + tidal-cli (CLI, official API,
    preview-only); tidalt comes closer architecturally (one Go module, a daemon plus a TUI client)
    but has no GUI at all and is Linux-only.
21. **Sone's own architecture has a load-bearing flaw streamboat must not copy: the queue lives in
    the frontend, not the core.** All playback-session state — queue, shuffle, repeat, history,
    playback source — is client-side `jotai` atoms in the React webview
    (`ref:sone/src/atoms/playback.ts:54-109`); the Rust side only persists a snapshot
    (`save_playback_queue`/`load_playback_queue`) and receives *mirrors* of frontend state for its
    MCP/overlay/miniplayer surfaces (`ref:sone/src-tauri/src/mcp/state_mirror.rs`). Position is
    polled by the frontend on a timer, not pushed by an event — there is no `track-advanced` event
    in Sone's Rust source (an error in the original pass); the real event set is `audio-error`,
    `audio-resampled`, `signal-path-changed`, `track-finished`, plus `mpris:*`/`tray:*` input
    events forwarded *up* to the webview. This inversion is why sone-windows is a fork rather than
    a headless-capable port: a webview-resident queue cannot serve a CLI or MPRIS client as a
    first-class citizen. **streamboat's core must own session, queue and transport; every UI
    (desktop, CLI, MPRIS, HTTP) is a thin subscriber** — the opposite of Sone's shape.
22. **A pure-Rust audio stack does have a precedent — just not for TIDAL.** librespot
    (github.com/librespot-org/librespot, MIT, ~7.1k★) is the same problem shape for
    Spotify-Premium-only playback: a core library plus thin frontend clients (spotifyd, ncspot),
    a `Sink` trait (`start`/`stop`/`write`) with nine backends behind cargo features including
    `rodio`+`cpal` (default) and `gstreamer`, and decode via Symphonia. It refutes "pure Rust has
    zero precedent" (Open question 2, corrected), but it does not solve TIDAL's DASH/BTS manifest
    handling, and `cpal` itself cannot do WASAPI *exclusive* mode (RustAudio/cpal#459) — a
    pure-Rust streamboat still needs hand-written `wasapi`-crate and `coreaudio-rs` backends for
    Windows/macOS exclusive output, exactly as GStreamer needs a hand-written macOS backend (see
    §18 and Implication 9).

---

## Findings

### 1. The TIDAL API landscape (verified against source)

#### 1.1 Two API surfaces

| | Official developer platform | Unofficial legacy API |
|---|---|---|
| Hosts | `openapi.tidal.com/v2`, `auth.tidal.com/v1/oauth2`, `login.tidal.com` | `api.tidal.com/v1`, `api.tidal.com/v2`, `api.tidalhifi.com/v1`, `desktop.tidal.com/v1` |
| Documented | Yes (developer.tidal.com, `tidal-music.github.io/tidal-api-reference`) | No |
| Credentials | Registered `client_id` (+ optional secret) from developer portal | Client IDs extracted from official apps |
| Playback | `GET /trackManifests/{id}` | `GET /tracks/{id}/playbackinfopostpaywall` |
| Full tracks for 3rd parties | **Previews only per Developer Guidelines** | Yes (works today) |
| Used by | tidal-cli, official SDKs | Sone, High Tide, Strawberry, python-tidal, mopidy-tidal, tidalt, tidalrs, TidalSwift, libopenTIDAL, TidaLuna |

**The official-path constraint, quoted** (via web search of developer.tidal.com, not directly
fetchable here): *"Playbacks shall only be made available through TIDAL's SDKs, namely an
official, unmodified version of the TIDAL Player module. The Player module in the SDK constitutes
the only allowed way for third-party applications to incorporate playback of TIDAL content"* and
*"by using an official, unmodified version of the Player module, third-party applications can
include playback of TIDAL previews"*, plus *"TIDAL will reject any quota extension requests for
any Offering that attempts to circumvent this"*.

Corroborating evidence from source and issues:

- `ref:tidal-cli/src/playback.ts:27-28,74-75,204-205` carries `trackPresentation` and
  `previewReason` fields and prints "Preview reason:" — i.e. the official manifest response
  routinely returns previews.
- `tidal-music/tidal-sdk-web` issue #133 (opened 2024-05-28): a developer using client
  credentials could only stream "a 30-second, low resolution version of the track", and PKCE
  auth returned `Token is missing required scope. Required scopes: r_usr playback`.
- `tidal-music` discussion #179: original poster requested third-party app review >6 months
  before 2025-06-03; as of April 2026 reports "nothing has moved" and no replies.

**Conclusion:** the owner's stated stance (do what High Tide and Sone do) is not merely a
convenience choice — it is currently the *only* way to build a full-quality TIDAL player as an
independent third party.

#### 1.2 Unofficial auth flows (all verified)

**Device authorization grant (RFC 8628)** — used by Sone, python-tidal, tidalt, TidalSwift,
libopenTIDAL, mopidy-tidal (default):

```
POST https://auth.tidal.com/v1/oauth2/device_authorization
  form: client_id, scope="r_usr w_usr w_sub" [, client_secret]
  -> deviceCode, userCode, verificationUri, verificationUriComplete, expiresIn, interval

POST https://auth.tidal.com/v1/oauth2/token
  form: client_id, device_code, grant_type=urn:ietf:params:oauth:grant-type:device_code,
        scope="r_usr w_usr w_sub" [, client_secret]
  400 + "authorization_pending" | "slow_down"  -> keep polling
  200 -> access_token, refresh_token, expires_in, token_type, user
```
Source: `ref:sone/src-tauri/src/tidal_api.rs:1550-1642`,
`ref:python-tidal/tidalapi/session.py:609-716`, `ref:tidalt/internal/tidal/client.go`.

**PKCE authorization code (needed for Hi-Res)** — used by High Tide, Strawberry, Sone
(alternative), python-tidal:

```
GET https://login.tidal.com/authorize
  ?response_type=code&redirect_uri=https://tidal.com/android/login/auth
  &client_id=<pkce client id>&lang=EN&appMode=android
  &client_unique_key=<random>&code_challenge=<S256(verifier)>
  &code_challenge_method=S256&restrict_signup=true
-> user completes in browser, is bounced to the redirect URI carrying ?code=...

POST https://auth.tidal.com/v1/oauth2/token
  form: code, client_id, grant_type=authorization_code, redirect_uri,
        scope="r_usr+w_usr+w_sub", code_verifier, client_unique_key
```
Source: `ref:python-tidal/tidalapi/session.py:482-545`,
`ref:sone/src-tauri/src/tidal_api.rs:1644-1684`.

Strawberry differs: it uses `https://login.tidal.com/authorize` +
`https://login.tidal.com/oauth2/token` with redirect `tidal://login/auth` (a custom scheme, no
local HTTP server) and scope `r_usr w_usr`
(`ref:strawberry/src/tidal/tidalservice.cpp:79-83,133-136`).

**Refresh:** `POST https://auth.tidal.com/v1/oauth2/token` with
`grant_type=refresh_token, refresh_token, client_id, scope="r_usr w_usr w_sub"`
(`ref:sone/src-tauri/src/tidal_api.rs:1370-1390`).

**Legacy username/password** (dead, historical only): `POST /v1/login/username` with an
`X-Tidal-Token` header, returning a `sessionId` used as `X-Tidal-SessionId`. Seen in
`ref:tidalgo/tidal.go:30-70` and `ref:dotnet-tidal-usdk/TidalUSDK/TidalClient.cs`. Both are
unmaintained (last commits 2018 and 2020).

#### 1.3 Stream resolution (verified)

Canonical unofficial request:

```
GET https://api.tidal.com/v1/tracks/{track_id}/playbackinfopostpaywall
  ?countryCode=<CC>&audioquality=<HI_RES_LOSSLESS|HI_RES|LOSSLESS|HIGH|LOW>
  &playbackmode=STREAM&assetpresentation=FULL
```

Response fields actually consumed by clients (`ref:sone/src-tauri/src/tidal_api.rs:3672-3690`):
`manifestMimeType`, `manifest` (base64), `audioQuality`, `bitDepth`, `sampleRate`,
`albumReplayGain`, `albumPeakAmplitude`, `trackReplayGain`, `trackPeakAmplitude`. Strawberry also
reads `encryptionKey`, `securityType`, `securityToken`, `codec(s)`, `urls`/`url`.

Alternative endpoints exist and Strawberry exposes all four as a user setting
(`ref:strawberry/src/constants/tidalsettings.h:25-30`,
`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:118-147`):

| Method | Path | Params |
|---|---|---|
| `StreamUrl` | `tracks/{id}/streamUrl` | `soundQuality` |
| `UrlPostPaywall` | `tracks/{id}/urlpostpaywall` | `audioquality`, `playbackmode=STREAM`, `assetpresentation=FULL`, `urlusagemode=STREAM` |
| `PlaybackInfoPostPaywall` (default) | `tracks/{id}/playbackinfopostpaywall` | `audioquality`, `playbackmode=STREAM`, `assetpresentation=FULL` |
| `PlaybackInfo` | `tracks/{id}/playbackinfo` | same as above |

python-tidal's `Track.get_url()` (the `urlpostpaywall` path) **raises `URLNotAvailable` when the
session is PKCE** (`ref:python-tidal/tidalapi/media.py:410-421`) — i.e. `urlpostpaywall` and PKCE
are mutually exclusive. libopenTIDAL additionally knows a `playbackinfoprepaywall` variant
(`ref:libopentidal/Source/OTService/OTServiceStd.c:165-167`).

#### 1.4 Manifest formats (verified)

- **BTS** (`application/vnd.tidal.bts`): base64 → JSON `{ urls: [...], codecs, mimeType,
  encryptionType, keyId }`. Take `urls[0]`, feed straight to the player. Codec is
  `codecs.toUpperCase().split('.')[0]` (`ref:python-tidal/tidalapi/media.py:662-675`,
  `ref:sone/src-tauri/src/tidal_api.rs:3706-3730`).
- **DASH** (`application/dash+xml`): base64 → MPD XML. Two consumption strategies observed:
  - Wrap as `data:application/dash+xml;base64,<b64>` and hand to GStreamer's `dashdemux`
    (Sone `ref:sone/src-tauri/src/commands/playback.rs:117-125`; Strawberry
    `ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:228-232`; High Tide on GStreamer < 1.26).
  - Write the MPD to a file and pass `file://` (mopidy-tidal
    `ref:mopidy-tidal/mopidy_tidal/playback.py:40-49`; High Tide on GStreamer ≥ 1.26
    `ref:high-tide/src/lib/player_object.py:494-506`).
  - Parse segments manually and concatenate: `initialization="…"`, `media="…$Number$…"`,
    `<S d= r=>` repeat counts (`ref:tidal-cli/src/playback.ts:118-167`).
- **EMU** (`application/vnd.tidal.emu`) and **HLS** (`application/vnd.apple.mpegurl`) appear only
  in the official SDKs (`ref:tidal-sdk-android/player/streaming-api/.../ManifestMimeType.kt`).

#### 1.5 Error sub-statuses (verified, Sone only)

`ref:sone/src-tauri/src/tidal_api.rs:11-43`:

- `subStatus` in `4000..=4999` on a 401 is a *playbackinfo* sub-status, **not** token expiry —
  refreshing the token will not help, so Sone suppresses the refresh-and-retry.
- Terminal (track will never play, skip it): `4005, 4010, 4030, 4031, 4032, 4034, 4035`.
- Deliberately **not** terminal: `4006` (streaming privileges lost — recovers) and `4033`
  (subscription up-sell — the user can fix it).
- Auth-namespace sub-statuses live elsewhere: `11002`/`11003` (token), `6001` (session),
  `1002` (pending / "not a Limited Input Device client").

This table is the single most valuable piece of operational knowledge in the whole landscape and
appears nowhere else. `1002` in particular is what a web-player client ID returns to the
device-code endpoint (`ref:sone/src-tauri/src/tidal_api.rs:1576-1588`).

#### 1.6 Loudness normalization (verified, two different formulas)

- **Sone**: `norm_gain = 0.8 * min(10^((replay_gain + 4)/20), 1/peak_amplitude)`, applied as a
  separate GStreamer `volume` element in Normal mode or as a scalar in the ALSA writer.
  Album-vs-track gain selected by playback context (`use_track_gain`), each falling back to the
  other. `ref:sone/src-tauri/src/commands/playback.rs:9-21,127-146`.
- **High Tide**: GStreamer-native — `taginject name=rgtags <tags> ! rgvolume pre-amp=4.0
  fallback-gain=-10 headroom=6.0 ! rglimiter ! audioconvert`, with tags injected per track from
  `stream.track_replay_gain`/`album_replay_gain` and skipped when the value is exactly `1.0`.
  `ref:high-tide/src/lib/player_object.py:196-206,578-600`.

Both use a +4 dB pre-amp "to match Tidal web's volume".

---

### 2. Sone — deep dive (the owner's named reference #1)

`ref:sone` · https://github.com/lullabyX/sone · GPL-3.0-only · 437★ / 24 forks / 64 open issues ·
created 2026-02-23 · last push 2026-09-05 · v0.21.0 · single primary author (lullabyX).

#### 2.1 Stack (verified from `ref:sone/src-tauri/Cargo.toml`, `ref:sone/package.json`)

**Rust backend** — pinned hard, mostly with `=` version pins:
`tauri =2.11.2` (+ `tauri-build =2.6.2`), `tauri-plugin-single-instance =2.4.2`,
`tauri-plugin-deep-link =2.4.9`, `tauri-plugin-opener =2.5.4`,
`tauri-plugin-window-state =2.4.1`, `tauri-plugin-oauth =2.0.0`,
`tauri-plugin-global-shortcut =2.3.1`, `gstreamer 0.23`, `gstreamer-app 0.23`,
`crossbeam-channel 0.5`, `reqwest 0.11` (`json`, `socks`), `tokio 1` (full), `base64 0.21`,
`dirs 5`, `aes-gcm 0.10`, `keyring 3` (`sync-secret-service`), `zeroize 1`, `sha2 0.10.9`,
`rand 0.10.1`, `md5 0.7`, `discord-rich-presence 1.1`, `zbus 5`, `rmcp 1.7.0`
(server + streamable-http + macros), `schemars 1`, `axum 0.7`, `uuid 1`, `flexi_logger 0.31`,
`thiserror 2`, `semver 1.0.28`, `walkdir 2`, `id3 1`, `image 0.25`.

Linux-only: `webkit2gtk =2.0.2`, `gtk 0.18`, `gdk 0.18`, `mpris-server 0.9`, **`alsa 0.10`**,
`libc 0.2`, `ksni 0.3` (tray), `x11rb 0.13.2` (`screensaver`, `dpms`), `wayland-client 0.31`,
`wayland-backend 0.3`, `wayland-protocols 0.32`, `gdkwayland-sys 0.18`.

**There is no `symphonia`, no `cpal`, no `rodio`, and no `alsa-sys`-only usage.** Decode is
GStreamer; output is either GStreamer's `autoaudiosink` or Sone's own ALSA writer thread.

**Frontend**: React 19.1, `jotai 2.17` (atom state, no Redux), Tailwind 4 (via
`@tailwindcss/postcss`), Vite 7.3, Vitest 4.1, TypeScript 5.8, `@tanstack/react-virtual 3.13`,
`hls.js 1.6.16` (music videos only), `lucide-react`, `qrcode.react` (device-code QR),
`react-easy-crop` (avatar upload). Tooling: pnpm 11.1.3, ESLint 9, Prettier 3, `knip`.

**Size**: 26,721 lines Rust across 61 files in `src/` alone (`tidal_api.rs` 7,200; `audio.rs`
3,309); including `build.rs` the count is 62 files / 26,724 lines (a prior pass conflated the two
denominators — cite one pairing, not a mix). 47,609 lines TS/TSX across 199 files.
**Tests**: 151 `#[test]`/`#[tokio::test]` in Rust, 46 `*.test.ts(x)` files, plus one Python
integration probe `ref:sone/src-tauri/tests/gapless_probe.py`. **None of this is enforced by CI**
— see the corrected §2.8 below.

#### 2.2 Module map (`ref:sone/src-tauri/src/`)

```
main.rs                 thin bin; delegates to lib.rs
lib.rs                  AppState, Settings struct, config bootstrap, crypto init,
                        tauri::generate_handler![...] (177 commands, corrected — see §2.4)
tidal_api.rs            the entire unofficial API client (auth, catalog, playback, library)
audio.rs                AudioPlayer: command loop, two backends, gapless, ALSA writer
pipeline_probe.rs       pad-caps probes (decoded vs output) for the signal-path panel
signal_path.rs          PRISTINE verdict; reads pactl + /proc/asound
cache.rs                4-tier encrypted on-disk cache with SWR + LRU + tag invalidation
crypto.rs               AES-256-GCM, "SONE" magic header, keyring + file key
theme_config.rs         external theme.json (15 presets + custom accent/background)
mpris.rs                mpris-server 0.9 bridge, command enum over an mpsc channel
tray.rs                 ksni tray
discord.rs              discord-rich-presence
idle_inhibit/{dbus,wayland,x11}.rs   three inhibit backends
scrobble/{lastfm,librefm,listenbrainz,musicbrainz,queue}.rs
tidal_report/{event,queue}.rs        play reporting back to TIDAL ("Recently Played")
mcp/{server,events,sanitizer,state_mirror,tools/*}    MCP server (rmcp + axum)
overlay/{server,state}.rs            OBS browser-source overlay (axum)
rate_gate.rs            client-side rate limiting
error.rs, http_util.rs, logging.rs, embedded_config.rs, embedded_*.rs
commands/               auth, library, pages, playback, metadata, search, feed, profile,
                        scrobble, overlay, mcp, updates, utility
```

Frontend mirrors it: `src/api/tidal.ts` (invoke wrapper + LRU), `src/atoms/*` (13 jotai atom
files — **this is where playback state actually lives, not in Rust; see §18**), `src/hooks/*`
(35 hooks, corrected from a prior "40"), `src/components/*` (85 components, close to the prior
"~90"), `src/miniplayer-main.tsx` + `miniplayer.html` (a second Tauri window).

#### 2.3 Audio: how the Rust side actually plays

`ref:sone/src-tauri/src/audio.rs`. Two `PlaybackBackend` variants (lines 106-122):

**`Normal`** — `uridecodebin → [per-branch queue] → concat → audioconvert → audioresample →
norm_volume → user_volume → autoaudiosink`. `concat` sits at the *head* so a second decoder for
the next track can be attached to `sink_1` and preroll while track one plays.

**`DirectAlsa`** (exclusive and/or bit-perfect) — `uridecodebin → audioconvert [→ audioresample]
[→ capsfilter] → appsink`, with a separate thread pulling PCM out of the appsink and writing it
to a raw `alsa::PCM` device. Gapless is deliberately disabled on this path.

**Gapless machinery (`concat`)** — `ref:sone/src-tauri/src/audio.rs:43-88,255-482`:
- `NextBinState` holds the prerolled `uridecodebin`, its per-branch `queue`, the track id, the
  queue id, and the next track's `norm_gain`/`replay_gain`/`peak_amplitude`.
- All pad-slot operations on `concat` are funnelled through a **single serialized executor
  thread** consuming an `AttachJob::{Attach,Detach}` mpsc, because "pad-slot operations on
  `concat` must never race". The worker dispatches and returns immediately.
- The branch `queue` decouples the next decoder from `concat`'s closed gate so it pre-buffers.
  **Corrected**: the branch decoder's `buffer-duration` and the branch queue's `max-size-time` are
  both **15 s** — the same as the main DASH path, not the "~3 s" a stale in-source docstring
  claims (`audio.rs:249-250` is a leftover comment; the actual values are set at `audio.rs:
  266-273`). Read the code, not the comment, before porting a numeric parameter.
- Availability check is just `gst::ElementFactory::find("concat").is_some()`
  (`audio.rs:3307`). The README's "requires GStreamer 1.24+" is a documentation claim, not a
  runtime check.

**Bit-perfect / exclusive ALSA — the valuable part.**

1. **Format probing** (`audio.rs:483-506`): `HwParams::any(pcm)` then `test_format` over
   `[S32LE, S24LE→"S24_32LE", S243LE→"S24LE", FloatLE, S16LE]`.
   *The ALSA↔GStreamer 24-bit naming is inverted* — ALSA `S24LE` is 24-in-32 (= GStreamer
   `S24_32LE`, 4 bytes); ALSA `S243LE` is packed 24-bit (= GStreamer `S24LE`, 3 bytes)
   (`audio.rs:190-215`). Getting this backwards is a silent corruption bug.
2. **Rate probing** (`audio.rs:545-568`): `test_rate` over
   `44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000, 705600, 768000`.
3. **Capsfilter selection** (`audio.rs:516-544`): pass through if the DAC supports the source
   format; else the *narrowest lossless promotion* the DAC supports
   (`S16LE→[S24LE,S24_32LE,S32LE]`, `S24LE→[S24_32LE,S32LE]`, `S24_32LE→[S24LE,S32LE]`) because
   `audioconvert` with `dithering=none` does pure integer shifts between these; else the DAC's
   widest probed format, with a truthful "from→to" toast.
4. **hw_params** (`audio.rs:569-751`): `Access::RWInterleaved`; in bit-perfect mode
   `set_rate_resample(false)` and a post-hoc `get_rate()` equality check that produces
   *"DAC doesn't support {N}kHz — turn off bit-perfect mode for compatibility"* rather than an
   opaque error. Channel negotiation falls back to `get_channels_min()` for pro interfaces
   (Focusrite/Audient) that reject 2-channel. `set_buffer_time_near(500_000)`,
   `set_period_time_near(50_000)`.
5. **sw_params**: `snd_pcm_hw_params()` resets `start_threshold` to 1, causing underruns from
   frame one; Sone re-sets `start_threshold` to the largest period-aligned value ≤ buffer_size
   (matching `alsasink`) and `avail_min = period_size`.
6. **Fixed-channel DACs**: a `stereo_pad_mix_matrix(N)` is installed on `audioconvert` at
   *build* time so the first caps event resolves to the device channel count directly, avoiding a
   2-channel transient that would thrash the writer (`audio.rs:2874-2887`).
7. **DASH-specific handling** (`audio.rs:2905-2945`): the appsink caps are constrained to
   DAC-supported formats for both modes, but the *rate* is constrained only in non-bit-perfect
   mode — in bit-perfect mode there is no resampler, so pinning the rate makes negotiation fail
   with "Internal data stream error" instead of the actionable message. DASH also gets
   `buffer-duration = 15 s` vs 5 s for BTS, and `use-buffering = true`.
8. **Volume curve**: `slider_to_amplitude(v) = clamp(v,0,1)^3` (cubic, ~50 dB range)
   (`audio.rs:219-222`). In bit-perfect mode the user volume element is absent entirely and the
   slider is locked at 100%.

**`WriterCommand`** enum (`audio.rs:90-104`): `Data(AudioChunk)`, `EndOfTrack{emit_finished,
generation}`, `FormatHint(PcmFormat)`, `Resampling{from,to}`, `PendingPromotion{from,generation}`,
`Flush`, `Shutdown`. Generation counters guard against stale chunks after a track change.

#### 2.4 API client and IPC

- `TIDAL_AUTH_URL = https://auth.tidal.com/v1/oauth2`, `TIDAL_API_URL = https://api.tidal.com/v1`,
  `TIDAL_API_V2_URL = https://api.tidal.com/v2`, `TIDAL_OPENAPI_URL = https://openapi.tidal.com/v2`,
  `TIDAL_CLIENT_VERSION = "2025.11.3"` sent as an `x-tidal-client-version` header on `/v2/` URLs
  (`tidal_api.rs:87-91,1541-1543`).
- **Quality cascade** (`commands/playback.rs:32-42`): `["HI_RES_LOSSLESS","HI_RES","LOSSLESS",
  "HIGH"]`, truncated at the user's ceiling, with both Hi-Res tiers dropped when
  `client_secret` is empty. The cascade **stops immediately** on a network error, a rate limit,
  or a terminal sub-status — because "over-requesting quality returns 200 with a downgraded
  audioQuality, never an error", so walking the ladder on those only multiplies request count 4×.
- **Proxy**: HTTP or SOCKS5, with host-character validation to prevent URL injection
  (`tidal_api.rs:60-86`).
- **IPC design**: **177** `#[tauri::command(rename_all = "camelCase")]` functions (corrected from
  an original-pass "≈150"; verified by both the `generate_handler!` block and a repo-wide
  `#[tauri::command` grep) registered in one `generate_handler!` block spanning `lib.rs:866-1070`,
  grouped by domain. **There is no `track-advanced` event and no position/state-tick event at
  all** — that was the original pass's guess, not a real emitted event. The actual emitted-event
  set is `audio-error`, `audio-resampled`, `signal-path-changed`, `track-finished`,
  `pkce-login-success`/`-error`/`-cancelled`, `scrobble-auth-error`, `tray:*` (toggle-play/
  next-track/prev-track) and `mpris:*` (play/pause/stop/seek/set-position/set-volume/set-shuffle/
  set-loop-status/set-fullscreen/open-uri) — the `tray:*`/`mpris:*` events flow *up* into the
  webview for it to act on, not down from it. **Position is polled, not pushed**: the frontend
  runs `setInterval(syncPosition, 500)` (`ref:sone/src/hooks/useProgressScrub.ts:38`); the
  miniplayer, overlay and signal-path panel each poll independently on their own intervals. Any
  IPC protocol streamboat designs should make an explicit push-vs-poll choice for position rather
  than reproducing this ad hoc per-consumer polling. Bridges for the miniplayer window, MCP and
  the overlay: `useMiniplayerBridge`, `useMcpBridge`, `useOverlayBridge`.

#### 2.5 Caching, settings, secrets

**Disk cache** (`cache.rs`): four tiers with TTL and stale-while-revalidate grace —

| Tier | Contents | TTL | SWR grace | Subdir |
|---|---|---|---|---|
| `UserContent` | playlists, likes, favorites | 15 min | 1 h | `user` |
| `Dynamic` | artist bios, charts, home page | 4 h | 24 h | `dynamic` |
| `StaticMeta` | album tracklists, credits | 7 d | 30 d | `static` |
| `Image` | album art, avatars | 30 d | 90 d | `images` |

Entries are AES-GCM-encrypted on disk, **keyed by a SHA-256 hex digest** (corrected — the original
pass called this "FNV-style"; `ref:sone/src-tauri/src/cache.rs:2,654-658` imports `sha2` and hashes
with it. FNV-1a is a different, *frontend-only* hash, see immediately below — the two caches use
different hash functions and the original pass conflated them), indexed by tag for invalidation,
evicted LRU, with `mark_in_flight` / `should_retry_refresh` guards against refresh storms.

**Frontend cache** (`src/api/tidal.ts`): a second, in-memory, size-based LRU capped at
`150 MB` with TTLs `SHORT = 2 min` (search/suggestions), `MEDIUM = 2 h` (lyrics, playlists,
favorites, mixes, page sections), `STATIC = 24 h` (albums, artists, credits), **FNV-1a** hashed
keys (`ref:sone/src/api/tidal.ts:55-62`, magic constants `0x811c9dc5`/`0x01000193`) and a tag
index.

**Settings** (`lib.rs:121-215`): a single `Settings` struct serialized to
`~/.config/sone/settings.json`, encrypted, with transparent migration from plaintext. Notable
fields: `auth_tokens`, `client_id`, `client_secret`, `auth_method`, `volume`, `last_track_id`,
`minimize_to_tray`, `decorations`, `titlebar_migration_v1`, `volume_normalization`,
`exclusive_mode`, `exclusive_device`, `bit_perfect`, `gapless` (default true),
`max_quality` (default `"HI_RES_LOSSLESS"`), `scrobble`, `proxy`, `discord_rpc`,
`discord_status_text`, `legacy_auth_notice_count`, `mcp_enabled`/`mcp_port` (5577)/`mcp_token`,
`overlay_enabled`/`overlay_port` (5578)/`overlay_host` (127.0.0.1), `report_plays` (default true).

**Crypto** (`crypto.rs`): AES-256-GCM. On-disk layout is `b"SONE" | version:u8 | nonce:12 |
ciphertext+tag`; `decrypt` returns non-magic input verbatim so unencrypted legacy files migrate
transparently. Key resolution: OS keyring (`keyring::Entry::new("sone","master-key")`, secret
must be exactly 32 bytes) → file `~/.config/sone/sone.key` → generate. A **file backup is always
written** even when the keyring works, because "keyring may be unreachable on next launch (e.g.
AppImage with different D-Bus session)". File mode `0600` on Unix; the raw key is `zeroize`d
after constructing the cipher.

**Embedded credentials** (`embedded_config.rs`, generated by `scripts/gen_embedded.py` from
`scripts/gen_credentials.py`): four credential strings stored as XOR-masked byte arrays —
`stream_key_a/b` (device-code id/secret) and `stream_key_c/d` (PKCE id/secret), decoded by
`decode(data, mask) = data[i] ^ mask[i]`. `has_stream_keys()` / `has_pkce_keys()` check for a
`PLACEHOLDER` prefix so public builds can ship without them. **This is obfuscation, not
security** — trivially reversible. The variable naming (`STREAM_SALT_*`, `CODEC_HINT_*`) is
deliberately misleading.

#### 2.6 Theming, lyrics, miniplayer, extras

- **Theming** (`theme_config.rs`): an external `<config>/theme.json`, `{version:1, preset,
  custom:{accent, background}}`, with 15 named presets that must stay in sync with
  `src/lib/theme.ts` — *Violet Night, Cyberpunk, Forest, Ocean, Midnight Cyan, Sakura, Rose,
  Ember, Copper, Noir, Daylight, Snowfall, Paper, Meadow, Blossom*. Colours are derived in the
  frontend from just two seed colours plus the preset.
- **Lyrics**: backend is one command, `get_track_lyrics` →
  `GET /v1/tracks/{id}/lyrics` returning `{lyrics, subtitles, isRightToLeft, lyricsProvider,
  providerLyricsId}` (`tidal_api.rs:3881-3890`, `commands/metadata.rs:44-50`). The **synced**
  payload is `subtitles`, not `lyrics`. Sync/display logic lives entirely in React
  (`MaximizedPlayer.tsx`, `NowPlayingDrawer.tsx`) — gap filled in by the fact-check pass, since the
  parsing detail matters if streamboat wants a compatible parser: `ref:sone/src/lib/lrc.ts::
  parseLrc`'s timestamp regex is `/(\d{1,2}):(\d{2})(?:[.:]([\d]{1,3}))?/` — note it accepts a
  **colon or a dot** before the fractional part, and 1–3 fractional digits, so a strict
  `[mm:ss.xx]`-only parser will silently drop lines TIDAL actually sends. The active line is
  chosen by scanning `lrcLines` in reverse for the first line whose timestamp is `<=` the current
  position (`ref:sone/src/components/MaximizedPlayer.tsx:522,593-639`). High Tide's independent
  implementation (`ref:high-tide/src/widgets/lyrics_widget.py:95-175`) uses a stricter
  `\[(\d+):(\d+\.\d+)\](.*)` regex, switches a `Gtk.ListView` between `SingleSelection` (synced,
  clickable — clicking a line seeks, via `on_seek_from_lyrics`) and `NoSelection` (plain text)
  depending on whether sync markers are found, and does the same reverse/forward linear scan to
  find and center the active line. Both are under 100 lines; read whichever matches streamboat's
  UI toolkit rather than designing this from scratch.
- **Miniplayer**: a second Tauri window with its own HTML entry (`miniplayer.html` →
  `src/miniplayer-main.tsx`), driven by `useMiniplayerWindow` / `useMiniplayerBridge` /
  `useMiniplayerEmitter`.
- **MPRIS** (`mpris.rs`): `mpris-server 0.9`, driven by an `MprisCommand` enum over a tokio
  unbounded channel (`SetMetadata`, `SetPlaybackStatus`, `SetVolume`, `Seeked`, `SetShuffle`,
  `SetLoopStatus`, `SetFullscreen`, `Stop`) — a clean way to keep D-Bus off the audio thread.
- **MCP server** (`mcp/`): `rmcp 1.7.0` streamable-HTTP over `axum`, bound to `127.0.0.1:5577`,
  URL path contains a persistent UUID token (`http://127.0.0.1:{port}/{token}/mcp`), off by
  default. Tools cover catalog, playback, playlists, favorites, state.
- **OBS overlay** (`overlay/server.rs`): `axum`, serves a self-contained HTML page, off by
  default. `127.0.0.1:5578` is only the *default* bind — `overlay_host` is a user setting and
  `overlay/server.rs:31-34` explicitly handles a `0.0.0.0` bind, so this is not hard-coded to
  loopback the way the MCP server is; document that distinction if streamboat copies the pattern.
- **Play reporting** (`tidal_report/`): reports plays back to TIDAL so "Recently Played" works,
  capturing the *actually served* `audioQuality`/`audioMode`/`assetPresentation` from the
  playbackinfo response. User-disableable via `report_plays`.
- **Idle inhibit**: three implementations — D-Bus, Wayland (`idle-inhibit` protocol via
  `wayland-protocols`), X11 (`x11rb` screensaver/dpms).

#### 2.7 Packaging and CI (reusable artifacts)

- `ref:sone/build-scripts/build/` — `deb.sh`, `rpm.sh`, `pacman.sh`, `all.sh`, plus
  `Dockerfile.deb`, `Dockerfile.rpm`, `Dockerfile.rpm-opensuse`, `Dockerfile.pacman`, and a
  `PKGBUILD` that repacks the Tauri-built `.deb` with `ar x` + `tar xf data.tar.*` (a neat trick:
  one artifact, four distro formats).
- `ref:sone/build-scripts/test/` — matching smoke-test scripts per format.
- `ref:sone/build-scripts/publish-cloudsmith.sh` — repo publishing.
- `ref:sone/src-tauri/tauri.conf.json` — the complete dependency lists for `.deb` and `.rpm`
  (webkit2gtk-4.1, gtk3, ayatana appindicator, gstreamer1.0 base/good/bad/libav/alsa, libsecret,
  libasound2, librsvg, pulseaudio-utils) and `appimage.bundleMediaFramework: true`. Window is
  `decorations: false` (custom React titlebar), `visible: false` at startup, `csp: null`.
  Deep-link scheme `tidal`.
- `ref:sone/flake.nix` + `ref:sone/nix/package.nix` — a Nix package plus a devShell that sets
  `GST_PLUGIN_SYSTEM_PATH_1_0` across gstreamer/base/good/bad/libav.
- `ref:sone/snap/snapcraft.yaml` — `core24`, strict confinement, with a documented
  `snap connect sone:alsa` step for exclusive output.
- `ref:sone/data/io.github.lullabyX.sone.{desktop,metainfo.xml}` — Freedesktop metadata.
- `ref:sone/.github/workflows/flathub-update.yml` — automates opening the Flathub PR on a `v*`
  tag (validates the tag regex, dereferences annotated tags to a commit SHA). **The Flathub
  manifest itself lives in the separate Flathub repo, not here.**
- `ref:sone/sync-version.mjs` — single-source version sync across `tauri.conf.json`,
  `Cargo.toml`, `PKGBUILD` and the AppStream metainfo.

**Sone does not auto-update — gap filled in by the fact-check pass.** `commands/updates.rs` is a
single `check_for_update` command: it `GET`s `https://api.github.com/repos/lullabyX/sone/
releases/latest` (10 s timeout, `User-Agent: SONE-update-checker`), parses `tag_name` with
`semver` after stripping a leading `v`, compares to `env!("CARGO_PKG_VERSION")`, and returns
`{available, current, latest, url}`; the frontend treats any failure as "no update" (silent).
There is **no `tauri-plugin-updater` in `Cargo.toml`, no signature verification, no download or
install path** — the user is sent to the GitHub release page. Combined with the absence of any
release CI (above), Sone's release process is entirely manual. **streamboat must decide this
explicitly, per distribution channel**: a signed in-app updater needs a keypair, a hosted
`latest.json`, and CI to produce signed artifacts, and it must never be offered to Flatpak/Snap/
AUR users (whose package manager owns updates) — check-and-notify-only is the safe default for
those channels, an in-app updater is only sensible for a self-contained bundle (AppImage,
Windows/macOS installer).

#### 2.8 Code quality

Strong as *source*, weak as *process* — the two need to be reported separately. Extensive
design-rationale comments (the `2b-A1/A2/A3`, `C1/C3/C5` markers reference an internal refactor
plan), `cargo clippy -- -D warnings` and `cargo fmt --check` in the local `check` script, `knip`
for dead frontend code, 151 Rust tests plus 46 frontend test files.

**But none of it runs anywhere.** `ref:sone/.github/` contains exactly six files: `FUNDING.yml`,
four `ISSUE_TEMPLATE/*.md`, and `workflows/flathub-update.yml` (which only opens a Flathub PR on a
tag). **There is no build, test, or lint CI at all** — nothing runs the 151 tests or `clippy` on a
PR or a push; every `.deb`/`.rpm`/`.pacman`/AppImage artifact is produced by a maintainer running
`build-scripts/build/*.sh` locally. Contrast `ref:tidal-hifi/.github/workflows/{build,release}.yml`,
which does have real CI. If streamboat reuses Sone's packaging *scripts* (recommended, §2.7), it
must still build its own CI pipeline from scratch — there is nothing to copy for that part.

Weaknesses in the source itself: `tidal_api.rs` at 7,200 lines and `audio.rs` at 3,309 lines are
monoliths; there is a single maintainer and 64 open issues; `image`, `id3` and `walkdir` are
pulled in for narrow uses. On the positive side for reuse, the parts most worth porting are
already pure functions with tests around them: `quality_tiers(ceiling, has_secret)`
(`commands/playback.rs:32-42`), `RateGate::cooling_down_at(now)`/`trip_at(now, secs)` (which take
`now` as a parameter specifically so they're testable without a real clock,
`rate_gate.rs`), and `parse_release_tag`/`is_update_available` (`commands/updates.rs:18-28`). The
hardware-dependent ALSA probing cannot be unit-tested and is covered only by
`ref:sone/src-tauri/tests/gapless_probe.py`, which nothing runs automatically — streamboat will
need to build its own hardware test matrix for that part regardless of how much code is ported.

#### 2.9 What to borrow / what to avoid

**Borrow (with pointers):**
- The whole ALSA bit-perfect negotiation module: `audio.rs:190-215` (format naming inversion),
  `483-568` (probing), `516-544` (promotion table), `569-751` (hw/sw params).
- The `subStatus` taxonomy: `tidal_api.rs:11-43`.
- The quality-cascade stopping rules: `commands/playback.rs:32-84`.
- The `norm_gain` formula and album-vs-track context selection: `commands/playback.rs:9-21,127-146`.
- The cache tier/TTL/SWR table: `cache.rs:18-57`.
- The crypto container format and keyring-plus-file key strategy: `crypto.rs`.
- The serialized attach/detach executor pattern for gapless: `audio.rs:63-88,427-482`.
- The packaging scripts and the `sync-version.mjs` idea: `build-scripts/`, `sync-version.mjs`.
- MPRIS-over-a-command-channel: `mpris.rs:8-46`.

**Avoid (with reasons):**
- `embedded_config.rs` XOR obfuscation — it is security theatre, and the misleading identifier
  names make the code dishonest about what it is doing. If streamboat ships credentials at all,
  say so plainly in the README.
- 7,200-line `tidal_api.rs` — split by resource from day one.
- `csp: null` in `tauri.conf.json` — set a real CSP.
- Binding MCP/overlay servers without an explicit opt-in is avoided *correctly* here (both
  default off); keep that, do not "helpfully" enable them.
- `=`-pinning every Tauri crate makes security updates a manual chore; pin the toolchain, use
  a lockfile for the rest.

---

### 3. sone-windows — the Windows fork

`ref:sone-windows` · https://github.com/lvllaby/sone-windows · GPL-3.0-only · not indexed by the
GitHub search API from this session (repo exists per web search) · v0.16.0 · last commit
2026-05-17 ("feat: implement automated GStreamer runtime packaging for Windows installer").

**It is a fork, not a port layer.** Verified by `diff -rq` against `ref:sone`:

Missing entirely from sone-windows: `mcp/`, `overlay/`, `tidal_report/`, `signal_path.rs`,
`pipeline_probe.rs`, `theme_config.rs`, `rate_gate.rs`, `http_util.rs`, `logging.rs`,
`commands/{feed,profile,updates,mcp,overlay}.rs`. Added: `media_controls.rs` (souvlaki SMTC),
`idle_inhibit.rs` (flat file instead of Sone's `idle_inhibit/` directory).
Rust LOC: 15,753 vs Sone's 26,721.

**Dependency deltas** (`ref:sone-windows/src-tauri/Cargo.toml`): loose `"2"` version specs
instead of Sone's `=` pins; `env_logger 0.11` instead of `flexi_logger`; no `rmcp`, `axum`,
`schemars`, `tokio-util`, `tokio-stream`, `futures-util`, `uuid`, `semver`, `x11rb`, `wayland-*`.
Adds `[target.'cfg(target_os = "windows")'.dependencies] souvlaki = "0.8.3"`.

**The Windows audio change is small and localized.** In `ref:sone-windows/src-tauri/src/audio.rs`
around lines 1230-1246 the sink construction is `#[cfg]`-split:

```rust
#[cfg(target_os = "linux")]  let sink = ElementFactory::make("autoaudiosink").build()?;
#[cfg(target_os = "windows")] let sink = {
    let s = ElementFactory::make("wasapi2sink").name("audio_sink").build()?;
    s.set_property("exclusive", exclusive);
    s.set_property("low-latency", true);
    if let Some(ref d) = device { s.set_property("device", d); }
    s
};
```

Note it is **`wasapi2sink`**, not `wasapisink` (the prior survey is wrong). Device enumeration is
also `#[cfg]`-split: Linux filters ALSA devices by `device.path`, Windows accepts the `wasapi2`
or `wasapi` API and reads `device.id` (`audio.rs:2040-2060`). The custom ALSA writer path is
Linux-only; on Windows bit-perfect relies on `wasapi2sink exclusive=true`.

**GStreamer runtime bundling**: `ref:sone-windows/scripts/prepare-gstreamer.js` scans
`src-tauri/gstreamer-runtime/**` for DLLs and generates both an NSIS macro
(`NSIS_HOOK_POSTINSTALL` copying core DLLs, `lib/gstreamer-1.0` plugins and `lib/gio/modules`)
and a WiX fragment. This is directly reusable for any GStreamer-on-Windows app.

**Maintenance posture** (`ref:sone-windows/README.md:5,20`): *"⚠️ Ported with help from AI
agents"* and *"This Windows port was created for personal use and **may not be actively or
correctly maintained** in the future."*

**Could one codebase serve both?** Yes. Nothing in the divergence is architectural — it is one
sink-construction block, one device-enumeration block, one media-controls module, and a build
script. The fork exists because it was easier than upstreaming, and it is now five minor versions
behind. **Lesson for streamboat: put the platform split behind a trait/`cfg` boundary in the
audio module from commit one, and never fork for a platform.**

---

### 4. High Tide — deep dive (the owner's named reference #2)

`ref:high-tide` · https://github.com/Nokse22/high-tide · GPL-3.0 · 671★ / 66 forks / 90 open
issues · created 2023-09-22 · last push 2026-08-21 · community project with a Matrix channel
(`#high-tide:matrix.org`).

#### 4.1 Stack and layout

Python 3 + PyGObject, GTK4 + libadwaita, Blueprint (`.blp` → `.ui`), Meson build, `python-tidal`
(`tidalapi`) for all API access, GStreamer for playback, `pypresence` for Discord, libsecret via
`gi.repository.Secret`, `libportal`/`Xdp` for Flatpak detection, gettext for i18n.

37 Python files, 6,821 lines; 23 `.blp` files; **8 translations** (`fr de nl pt_BR es it zh_TW
pl` per `ref:high-tide/po/LINGUAS`) — the prior survey's "18 languages" is wrong.

```
src/main.py                      Adw.Application entry
src/window.py                    main window, feature orchestration
src/login.py                     PKCE login dialog (91 lines)
src/mpris.py                     MPRIS via Gio.DBus, forked from Blanket/Lollypop
src/disconnectable_iface.py      IDisconnectable mixin (signal cleanup)
src/new_playlist.py
src/lib/player_object.py         GStreamer engine, queue, shuffle/repeat, cache
src/lib/utils.py                 API helpers, favourites, search, session
src/lib/secret_storage.py        libsecret token store
src/lib/cache.py                 in-memory object cache
src/lib/discord_rpc.py
src/pages/*.py                   album, artist, playlist, mix, search, explore, collection,
                                 track_list, generic, from_function, not_logged_in
src/widgets/*.py                 HT-prefixed widgets: tracks_list, queue, lyrics, card,
                                 carousel, top_hit, auto_load, link_label, shortcuts
data/ui/**/*.blp                 Blueprint markup
data/io.github.nokse22.high-tide.gschema.xml    GSettings schema
build-aux/*.json                 Flatpak manifest + vendored Python wheels
```

#### 4.2 Auth: PKCE only

`ref:high-tide/src/login.py` is 91 lines and does exactly three things:
`session.pkce_login_url()` → user opens it → user pastes the redirect URL →
`session.pkce_get_auth_token(redirect_url)` → `session.process_auth_token(token_json,
is_pkce_token=True)` → `session.check_login()`. The exchange runs on a `threading.Thread` and
marshals back with `GLib.idle_add`. There is **no device-code path in the UI**.

Token storage (`ref:high-tide/src/lib/secret_storage.py`): a libsecret `Secret.Schema` named
`io.github.nokse22.high-tide` with a single `version` STRING attribute, storing a JSON blob under
key `high-tide-login` containing `token-type`, `access-token`, `refresh-token`, `expiry-time` and
an `is-pkce` flag. On startup, **outside Flatpak only**, it force-unlocks the default collection
via `Secret.Service.get_sync()` → `Secret.Collection.for_alias_sync()` → `unlock_sync()`, because
of a real bug (issue #97). Flatpak detection is `Xdp.Portal.running_under_flatpak()`.

#### 4.3 GStreamer pipeline

`ref:high-tide/src/lib/player_object.py:83-115,184-247`:

- `Gst.Pipeline.new("dash-player")` containing a single `playbin3` (falling back to `playbin`,
  with a logged error, if `playbin3` is unavailable).
- Gapless = `playbin3`'s `about-to-finish` signal → `play_next_gapless` sets the next URI.
- The audio sink is a bin built from a string with `Gst.parse_bin_from_description`:
  ```
  queue ! audioconvert ! [taginject name=rgtags <tags> !
                          rgvolume name=rgvol pre-amp=4.0 fallback-gain=-10 headroom=6.0 !
                          rglimiter ! audioconvert !] audioresample ! <sink>
  ```
- Sink map: `AUTO→autoaudiosink`, `PULSE→pulsesink`, `ALSA→alsasink device=<alsa_device>`,
  `JACK→jackaudiosink`, `OSS→osssink`, `PIPEWIRE→pipewiresink`.
- **`pipewiresink` disables gapless** (`self.gapless_enabled = False`) — an empirical finding
  worth carrying forward.
- `change_audio_sink()` sets the pipeline to `NULL`, rebuilds the bin, restores `PLAYING`, and
  re-seeks to the saved fractional position — sink switching without losing your place.
- Bus error handling special-cases two strings: `"Internal data stream error" + "not-linked"` →
  restart the pipeline on the same track; `"Error outputting to audio device" + "disconnected"` →
  toast "ALSA Audio Device is not available" and pause.
- Volume: `playbin.volume = value**2` when quadratic mode is on, else linear.

**There is no bit-perfect or exclusive mode.** `alsasink device=` is as close as it gets — no
`snd_pcm_hw_params` negotiation, no rate matching, no resampler bypass.

#### 4.4 Stream resolution and on-disk caching

`ref:high-tide/src/lib/player_object.py:455-527`:

```python
stream   = track.get_stream()
manifest = stream.get_stream_manifest()
if stream.manifest_mime_type == ManifestMimeType.MPD:
    data = stream.get_manifest_data()
    if Gst.version() >= (1, 26):
        write data to CACHE_DIR/manifest.mpd ; return "file://…/manifest.mpd"
    else:
        return "data:application/dash+xml;base64," + b64(data)
elif stream.manifest_mime_type == ManifestMimeType.BTS:
    return manifest.get_urls()[0]
```

**The caching behaviour deserves attention — corrected by the fact-check pass, the original
"opt-in" framing was wrong.** `utils.MUSIC_DIR` is not a user-chosen library folder: it is set
*unconditionally* to `$XDG_CACHE_HOME/high-tide/music` (or `~/.cache/high-tide/music` — see
`ref:high-tide/src/lib/utils.py:63-78`), created at startup with no GSettings key to disable it
(the full 16-key schema has no such toggle; grep confirms it). `_get_cached_or_stream_url`
(`player_object.py:469-480`) first looks for `MUSIC_DIR/{track.id}_{session.audio_quality}.m4a`
and plays it from `file://` if present. Otherwise, unless
`Gio.NetworkMonitor.get_network_metered()`, it spawns a background thread that:
- for MPD, runs `ffmpeg -protocol_whitelist file,crypto,data,http,https,tcp,tls -i manifest.mpd
  -f mp4 -c copy -y <tmp>` and renames on success — but **only on the GStreamer ≥ 1.26 branch**,
  where the manifest exists as a file on disk; on GStreamer < 1.26 the `data:` URI branch never
  spawns this thread, so DASH tracks are not cached there (a nuance the original pass omitted);
- for BTS, `requests.get(stream_url, stream=True)` and writes 8 KiB chunks — unconditionally,
  regardless of GStreamer version.

It is bounded, not unbounded: `ref:high-tide/src/window.py:223` runs `utils.evict_cache(utils.
MUSIC_DIR, 5)` in a startup thread, and `ref:high-tide/src/lib/utils.py:828-843` evicts by
`st_atime` until the directory is under 5 GB. So the accurate statement is narrower than "a
downloader with a player attached": it is an **always-on, unencrypted, non-consented,
size-capped (5 GB LRU) full-quality media cache with zero user visibility or control** — for
every user, not opt-in, and not unbounded. That is still legally and reputationally the riskiest
default in the reference set, and it is inside the project the owner named as a model.
**streamboat must make a deliberate, documented decision here rather than inheriting it.**

#### 4.5 Settings, quality, MPRIS, packaging

- **GSettings** (`ref:high-tide/data/io.github.nokse22.high-tide.gschema.xml`) — 16 keys:
  `window-width`, `window-height`, `quality`, `last-playing-index`, `last-playing-thing-id`,
  `last-playing-thing-type`, `last-volume`, `repeat`, `preferred-sink`, `run-background`,
  `normalize`, `quadratic-volume`, `app-id-change-understood`, `video-covers`, `discord-rpc`,
  `alsa-device`.
- **Quality selection**: the `quality` GSetting is pushed into `tidalapi`'s session config. Since
  the login is PKCE, `HI_RES_LOSSLESS` is reachable. python-tidal 0.8.11's `Quality` enum offers
  `LOW / HIGH / LOSSLESS / HI_RES_LOSSLESS` only.
- **MPRIS** (`ref:high-tide/src/mpris.py`): hand-rolled `Gio.DBus` server, forked from Blanket
  which forked Lollypop — i.e. the standard GNOME lineage. Introspection XML lives in the class
  docstring.
- **Threading model** (`ref:high-tide/CONTRIBUTING.md`): thread-target methods are prefixed
  `th_`, must catch all exceptions, and must marshal to the UI with `GLib.idle_add`. Custom
  widgets are prefixed `HT`. 4-space indent, 88-character lines.
- **Flatpak** (`ref:high-tide/build-aux/io.github.nokse22.high-tide.json`): runtime
  `org.gnome.Platform` **50**, command `high-tide`, meson build, `finish-args`:
  `--share=network --share=ipc --socket=fallback-x11 --device=dri --socket=wayland
  --socket=pulseaudio --filesystem=xdg-run/pipewire-0:ro --filesystem=xdg-run/discord-ipc-0`.
  Modules: `libportal.json`, `blueprint-compiler.json`, `alsa-utils.json`,
  `python3-pypresence.json`, `python3-six.json`, `python3-tidalapi.json` (a pinned wheel list
  including certifi, charset_normalizer, idna, isodate, mpegdash, pyaes, python_dateutil,
  requests[socks], tidalapi). Shipped on Flathub as `io.github.nokse22.high-tide`.

Note the Flatpak sandbox has **`--socket=pulseaudio` and a read-only PipeWire socket, but no
`--device=all`** — exclusive ALSA would not work under this manifest as written.

#### 4.6 What High Tide lacks

Bit-perfect / exclusive output; sample-rate switching; Windows/macOS; headless/CLI; any offline
sync management UI (the caching above is invisible); podcasts/audiobooks; TIDAL Connect; Atmos
handling; encrypted-at-rest cache; a settings-level client-ID override.

#### 4.7 Borrow / avoid

**Borrow:** the PKCE dialog flow (`src/login.py` — 91 lines is the whole thing); the keyring
unlock workaround for non-Flatpak Linux (`src/lib/secret_storage.py:50-60`); the swappable
audio-sink bin built from a description string and the sink-change-with-position-restore
(`player_object.py:184-247`); the `IDisconnectable` signal-cleanup pattern; the CONTRIBUTING
thread-safety rules; the Flatpak manifest structure and the vendored-wheels module pattern; the
PipeWire-breaks-gapless finding.

**Avoid:** silent on-disk track caching without an explicit user-facing toggle and a clear label;
a single `utils.py` at 843 lines carrying session state as module globals; `playbin`'s
all-or-nothing pipeline when you want signal-path control; assuming `playbin3` exists.

---

### 5. Strawberry Music Player — TIDAL integration

`ref:strawberry` · https://github.com/strawberrymusicplayer/strawberry · GPL-3.0 · **3,947★** /
340 forks / 21 open issues · last push 2026-09-07 · C++17 + Qt 6.4+, GStreamer engine · the most
mature and most actively maintained codebase in the set, and the only one that is a general music
player with TIDAL as one of several streaming backends.

TIDAL code is ~2,709 lines across 12 files in `ref:strawberry/src/tidal/`:
`tidalservice`, `tidalbaserequest`, `tidalrequest`, `tidalstreamurlrequest`,
`tidalfavoriterequest`, `tidalurlhandler`.

**Auth** (`ref:strawberry/src/tidal/tidalservice.cpp:70-136`): a shared `OAuthenticator` set to
`Type::Authorization_Code` with `set_use_pkce(true)`, `authorize_url =
https://login.tidal.com/authorize`, `access_token_url = https://login.tidal.com/oauth2/token`,
`redirect_url = tidal://login/auth`, `scope = "r_usr w_usr"`,
`use_local_redirect_server = false`. The client ID can be compiled in via a `TIDAL_CLIENT_ID`
define (run through `Utilities::MaybeDecryptApiCredential`) or supplied by the user in settings —
**no client secret is ever compiled in**.

**API base**: `https://api.tidalhifi.com/v1` (`ref:strawberry/src/tidal/tidalservice.cpp:59`).
`countryCode` is injected by `TidalBaseRequest`; HTTP 401 clears the session and forces re-login.

**Stream URL**: all four methods above, user-selectable, default
`PlaybackInfoPostPaywall` (`ref:strawberry/src/constants/tidalsettings.h:63`). Quality options in
the UI are `LOW / HIGH / LOSSLESS / HI_RES / HI_RES_LOSSLESS`, default `LOSSLESS`
(`ref:strawberry/src/settings/tidalsettingspage.cpp:68-72`).

**How it degrades — the important bit** (`ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:
244-249,295-310`): it refuses encrypted streams outright, three separate ways, each with the same
user-facing message:

> *"Received a %1 encrypted stream from Tidal, which Strawberry does not support. Whether Tidal
> delivers encrypted streams depends on the client ID in use. Try changing the Client ID in the
> Tidal settings"*

triggered by (a) a manifest `encryptionType` that is non-empty and not `"NONE"`, (b) a non-empty
top-level `encryptionKey`, (c) a `securityType` that is non-empty and not `"NONE"` alongside a
non-empty `securityToken`. DASH manifests are wrapped as `data:application/dash+xml;base64,…`.
Filetype is inferred from `mimeType` via `QMimeDatabase`, falling back to `codecs`, falling back
to the URL extension.

**Collection**: three SQLite tables per source (`tidal_artists_songs`, `tidal_albums_songs`,
`tidal_songs`) through the shared `CollectionBackend` — a genuinely good pattern for a client
that wants a browsable local index of a remote catalogue. Concurrent requests are capped
(search limits default to 4 artists / 10 albums / 10 songs, `searchdelay` 1500 ms).

**Audio — corrected, "general bit-perfect support" overstates it.** The shared Strawberry
GStreamer engine (`src/engine/gstengine.cpp`, `gstenginepipeline.cpp`) uses `playbin3` and
per-platform sinks, but there is **no dedicated bit-perfect code path at all** — `rg -i
'bit.perfect'` over `src/` returns nothing. What exists is narrower and mechanism-specific:
- **Windows/WASAPI only** has an explicit, user-facing "exclusive mode" setting
  (`ref:strawberry/src/constants/backendsettings.h:36,64`, default off); `ExclusiveModeSupport()`
  (`gstengine.cpp:523-525`) returns true only for `wasapisink`/`wasapi2sink`.
- **On Linux the "exclusive" flag is merely inferred**, not user-set: it's set true when the
  output sink is `alsasink` *and* the device string starts with `hw:` **or `plughw:`**
  (`gstenginepipeline.cpp:634-635`) — and `plughw:` is by definition a converting ALSA plugin, so
  this inference does not imply bit-perfect. Its only real effect is passing `exclusive` to the
  sink if it has that property, and suppressing crossfade.
- **`osxaudiosink` has no exclusive path at all.**
- The audio bin **always** contains two `audioresample` elements
  (`gstenginepipeline.cpp:748,758`), linked to the sink with unconstrained `audio/x-raw` caps —
  there is no `snd_pcm` format/rate probing, no promotion table, and no user feedback on a rate
  mismatch, unlike Sone.
- Strawberry also implements **EBU R128 loudness normalization** as a separate stage from
  ReplayGain, and enabling it force-links the downstream chain through `audio/x-raw, format =
  {F32LE, F64LE}` (`gstenginepipeline.cpp:953-961`) — i.e. **a float-domain normalizer is
  mutually exclusive with bit-perfect integer output**, not merely "bypassed in bit-perfect
  mode." Any normalization stage streamboat builds must be bypassable for exactly this reason.
- **Platform reality check**: Strawberry's own README scopes bit-perfect to *Linux only*
  ("Bit-perfect playback on Linux", `ref:strawberry/README.md:61`), and its macOS/Windows binary
  releases are **sponsor-only** (`README.md:85`). So Strawberry is real evidence for a
  Windows-WASAPI-exclusive path (same mechanism as sone-windows) but it is not the cross-platform
  bit-perfect precedent the comparison table below originally implied.

**Borrow:** the honest refuse-and-explain behaviour on encrypted streams (this is exactly the
posture streamboat wants); the user-selectable stream-URL method as a hedge against endpoint
churn; the compile-time-or-user-supplied client ID with no secret; the SQLite per-source
collection schema; `MaybeDecryptApiCredential` as a naming-honest alternative to Sone's XOR; the
WASAPI-exclusive-mode setting as a Windows reference. **Avoid:** modelling streamboat on a general
player's plugin shape if the goal is a dedicated TIDAL client — the abstraction tax is visible;
citing Strawberry as proof that "the general engine" gives you bit-perfect on macOS or gapless
Windows out of the box — it does not.

---

### 6. tidal-hifi — the Electron/Widevine wrapper

`ref:tidal-hifi` · https://github.com/Mastermindzh/tidal-hifi · MIT (GitHub reports license
"other"/NOASSERTION) · **1,725★** / 100 forks / 22 open issues · created 2019-09-16 · last push
2026-08-31 · v8.1.3 · TypeScript, 8,011 lines in `src/`.

**The whole idea**: don't reimplement TIDAL, wrap `listen.tidal.com` in a Chromium that has a
Widevine CDM. `ref:tidal-hifi/package.json:67` and
`ref:tidal-hifi/build/electron-builder.base.yml:1-6`:

```
"electron": "github:castlabs/electron-releases#v43.0.0+wvcus"
electronVersion: 43.0.0
electronDownload: { version: v43.0.0+wvcus,
                    mirror: https://github.com/castlabs/electron-releases/releases/download/v }
```

`components.whenReady()` is awaited in `ref:tidal-hifi/src/main.ts:413` (castlabs' Widevine
component bootstrap). **This is the only project in the set that plays DRM-protected TIDAL
content without touching DRM itself** — Chromium does it.

**The real objection to this approach is operational, not just about audio quality — gap filled
in by the fact-check pass.** tidal-hifi's own README credits castlabs specifically "for
maintaining Electron with Widevine CDM installation, **Verified Media Path (VMP)**, and
**persistent licenses (StorageID)**" (`README.md:175-176`). VMP means the Electron binary must be
signed through castlabs' own signing service to be trusted by Widevine at all — i.e. adopting this
approach means taking a build/CI dependency on a third-party signing service, not just accepting
Chromium's resampler. That is a much harder sell than "the audio is worse"; state it plainly if
the option is discussed at all. On the positive side, tidal-hifi's *packaging* is the most
complete CI in the whole set: `ref:tidal-hifi/.github/workflows/{build,release}.yml` plus
electron-builder configs invoked by named npm scripts (`build-deb`, `build-rpm`, `build-snap`,
`build-arch`, `build-win`, `build-mac`) — worth reading as a packaging-CI checklist even though
the audio approach itself should not be copied.

**Controller abstraction** (`ref:tidal-hifi/src/TidalControllers/`, doc
`ref:tidal-hifi/docs/tidal-controllers.md`): a `TidalController` interface with four
implementations selected by `advanced.controllerType`
(`ref:tidal-hifi/src/preload.ts:39-70`):
- `MediaSessionController` (default) — browser MediaSession API, falls back to the DOM controller
  for buttons and anything MediaSession does not expose;
- `DomTidalController` — parses the page DOM; the default fallback branch;
- `ReduxController` — walks the React fiber tree to find TIDAL's Redux store; gives the richest
  audio metadata (bit depth, sample rate, codec);
- `TidalApiController` — marked *"In Development / Not Ready / Minimal"* in the docs table.

This four-way strategy pattern with graceful fallback is the right answer whenever you are
scraping a moving target.

**Additions on top of the web player**: MPRIS (`mpris-service ^2.1.2`), Discord RPC
(`@xhayper/discord-rpc 1.3.4`), ListenBrainz, configurable hotkeys (`hotkeys-js`), SCSS themes,
an idle inhibitor, a custom titlebar, window transparency, a sharing service, and a local
**Express 5 API on port 47836** (`ref:tidal-hifi/src/scripts/settingsStore.ts:45`) documented with
`swagger-jsdoc` + `swagger-ui-express`, exposing `GET /current` and `GET /current/audio-quality`.

**Audio quality caveat** (`ref:tidal-hifi/docs/audio-quality.md`): *"By default Chromium resamples
all audio to 48kHz"*. The app offers a setting that passes
`--audio-output-sample-rate=192000` and disables the out-of-process audio service, but the doc is
explicit that PipeWire/PulseAudio must also be reconfigured or it resamples back down. There is
no bit-perfect path, no exclusive mode, and no gapless.

**Packaging**: `electron-builder` configs for deb, rpm, snap (`core22` with a
`gnome-42-2204` content-snap override), pacman, unpacked, Windows and macOS; Linux
`executableArgs` include `--ozone-platform-hint=auto`, `--enable-features=WaylandWindowDecorations`,
`--enable-wayland-ime`; `x-scheme-handler/tidal` MIME registration.

**Borrow:** the controller-with-fallbacks pattern; the local HTTP API + OpenAPI generation as the
integration surface (cheap, and the community actually used it); the electron-builder config set
as a packaging checklist; the honest audio-quality documentation. **Avoid:** the whole approach,
if streamboat wants bit-perfect audio, gapless, or a native UI — a Chromium resampler sits
between you and the DAC and you cannot remove it.

---

### 7. TidaLuna — a mod of the official client (intelligence, not a model)

`ref:TidaLuna` · https://github.com/Inrixia/TidaLuna · **MS-PL** · 591★ / 56 forks / 15 open
issues · created 2025-04-16 · last push 2026-09-01 · v1.16.6-beta · TypeScript/ESM, esbuild,
successor to "Neptune".

**What it is**: an injector plus a plugin system that runs *inside* the official TIDAL Electron
desktop app. `ref:TidaLuna/native/injector.ts` intercepts HTTPS, strips CSP `<meta>` tags, and
injects a bundle into the render process.

**What it reveals about the official client:**
- Credentials are obtainable by walking the app's bundled webpack module tree for a function
  literally named `getCredentials`, then invoking it
  (`ref:TidaLuna/plugins/lib/src/helpers/getCredentials.ts:14-18`). It returns `{clientId, token,
  userId, expires, grantedScopes}`.
- Requests then carry `Authorization: Bearer <token>` and `x-tidal-token: <clientId>`
  (`ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:21-26`).
- The desktop client's own API host is **`https://desktop.tidal.com/v1`**, with query args
  `countryCode=<from redux session>&deviceType=DESKTOP&locale=<settings.language>`
  (`.../TidalApi/index.ts:26-31`).
- Playback info is `GET https://desktop.tidal.com/v1/tracks/{id}/playbackinfo
  ?audioquality={q}&playbackmode=STREAM&assetpresentation=FULL` — note: **no `postpaywall`
  suffix and no `countryCode`** on this call (`.../TidalApi/index.ts:53-61`).
- Concurrency on playbackinfo is limited to **2** by an explicit `Semaphore(2)`, and a 403/404
  marks the track permanently unavailable in a `Set`. Failed requests retry exactly once after
  1 s.
- The client state is a Redux store, reachable via `redux.store.getState()` with
  `playbackControls`, `playQueue`, `session`, `settings`, `content`, `user` slices.
- An `AlbumPage` is fetched via `GET /v1/pages/album?albumId=…&countryCode=NZ&locale=en_US
  &deviceType=DESKTOP` and the tracklist is dug out of `rows[].modules[]` where
  `type === "ALBUM_ITEMS"` — i.e. TIDAL's "pages" API returns a CMS-style module tree, which is
  also what Sone's `commands/pages.rs` deals with.

**The hard boundary.** `ref:TidaLuna/plugins/lib.native/src/request/decrypt.ts` ships a hardcoded
master key and uses it to decrypt streams whose manifest reports `encryptionType: "OLD_AES"`
(it passes `"NONE"` through and throws on anything else). The key value and the procedure are
deliberately not reproduced in this repository.

**streamboat must not do this.** It is circumvention of a technological protection measure, it
is what separates "a player for subscribers" from "a ripper", and it is the exact behaviour the
project brief rules out. Strawberry's refuse-and-explain is the correct alternative. TidaLuna is
worth reading for the endpoint intelligence above and for nothing else.

TidaLuna also ships `plugins/linux/src/tidalHifi.ts`, which bridges the mod to tidal-hifi's main
process for MPRIS, Discord RPC, notifications, hotkeys and CSS injection — i.e. the two projects
compose.

---

### 8. mopidy-tidal — the headless/server precedent

`ref:mopidy-tidal` · https://github.com/EbbLabs/mopidy-tidal (formerly tehkillerbee/mopidy-tidal)
· Apache-2.0 · 123★ / 35 forks / 39 open issues · last push 2026-06-12 · v0.3.13 · Python ≥3.12,
`Mopidy>=3.0`, `tidalapi>=0.8.10`.

**Why it matters**: it is the only project that proves a TIDAL backend works headlessly, driven
by MPD clients / Iris / Snapcast, with no GUI at all. That is directly relevant to streamboat's
required headless mode.

**Config schema** (`ref:mopidy-tidal/mopidy_tidal/ext.conf`):
```
enabled = true
quality = LOSSLESS            # LOW | HIGH | LOSSLESS | HI_RES_LOSSLESS
auth_method = OAUTH           # OAUTH | PKCE
login_server_port = 8989
lazy = false
login_method = AUTO           # BLOCK | AUTO | HACK
playlist_cache_refresh_secs = 0
client_id =
client_secret =
playback_cache = false
playback_cache_max_entries = 1024
playback_cache_buffer_bytes = 16777216   # 16 MiB
```

**Login UX for a headless daemon** is the interesting design problem, solved three ways
(`ref:mopidy-tidal/mopidy_tidal/backend.py`, `web_auth_server.py`, `login_hack.py`):
- `BLOCK` — block startup until login completes;
- `AUTO` — start unauthenticated and log in lazily;
- the **"login hack"** — while logged out, the library/search providers return a dummy
  Track/Album/Artist whose title is the login message and whose cover art is a QR code of the
  login URL, so *any* MPD client displays the login prompt with no protocol extension. This is
  the cleverest single idea in the reference set for headless auth.
- A small HTTP server on port 8989 renders the link (OAuth) or a form to paste the PKCE redirect
  URL.

**Stream resolution** (`ref:mopidy-tidal/mopidy_tidal/playback.py:27-64`): for MPD, write
`manifest.mpd` into Mopidy's cache dir and return a `file://` URI; for BTS, return
`manifest.get_urls()[0]`. It also logs quality/bit-depth/sample-rate per track. When
`session.config.quality == hi_res_lossless` it checks `"HIRES_LOSSLESS" in
track.media_metadata_tags` and logs a downgrade notice if absent — a cheap pre-flight check
streamboat should copy.

**The caching proxy** (`ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/`): a threaded local HTTP
relay in front of **`https://lgf.audio.tidal.com/`** (`__init__.py:17-19`) backed by a SQLite
chunk cache (`cache.py`) with an insertion context manager that only finalizes an entry on a
complete download, and `Range` header support in `proxy.py` so GStreamer can seek. When a proxy
entry exists, `translate_uri` returns the local URL instead; otherwise it falls back to
`track.get_url()` and, if that raises `URLNotAvailable` (the PKCE case), to `as_stream()`.

This is the only design in the set that gives you seeking over a cached remote stream without
writing a whole file first, and it is directly reusable for streamboat's headless mode.

**Borrow:** the login-hack pattern; the three login methods; the config schema shape; the Range-
capable caching proxy; the pre-flight `media_metadata_tags` quality check. **Avoid:** coupling to
Mopidy's provider API if streamboat wants its own daemon protocol; the hardcoded
`lgf.audio.tidal.com` host (CDN hostnames change).

---

### 9. tidal-connect (Docker) — the "be a Connect target" precedent

`ref:tidal-connect` · https://github.com/GioF71/tidal-connect · MIT (wrapper only) · 167★ / 13
forks / 8 open issues · last push 2026-03-31 · Bash + Docker Compose.

It wraps a **proprietary, closed-source binary** (`/app/ifi-tidal-release/bin/
tidal_connect_application`) shipped with an iFi Audio device certificate
(`IfiAudio_ZenStream.dat`). The wrapper contributes ALSA device configuration, mDNS/Avahi setup,
a test-tone pre-flight, and a restart loop.

Invocation (`ref:tidal-connect/bin/entrypoint.sh:158-176`):
```
tidal_connect_application \
  --tc-certificate-path <cert> --playback-device <alsa dev> \
  -f "<friendly name>" --model-name "<model>" \
  --codec-mpegh true --codec-mqa <bool> \
  --disable-app-security <bool> --disable-web-security <bool> \
  --enable-mqa-passthrough <bool> --log-level <n> \
  --enable-websocket-log "0" [--clientid "<id>"]
```

**Quality ceiling**: `ref:tidal-connect/README.md:57-62` — after TIDAL removed all MQA content at
the end of July 2024, this implementation is effectively limited to 16/44.1 LOSSLESS; it could
previously reach 24/48 plus MQA unfolding to 24/88 or 24/96. The README explicitly points users
at Mopidy-Tidal, Music Assistant, upmpdcli's TIDAL plugin, or BubbleUPnP for HI_RES_LOSSLESS.

**Operational lessons that generalize:** host networking is mandatory for mDNS discovery; ALSA
card indices shift across reboots so `CARD_NAME` must be preferred over `CARD_INDEX` and a stable
`/etc/asound.conf` generated at startup; if a `Master` mixer control exists, create a `SoftMaster`
softvol rather than coupling to hardware volume; play a test tone before starting the app to
catch a locked device. The repo carries **26** per-DAC `asound.conf` presets in `userconfig/`
(corrected from an original "40+" — a separate `samples/` directory holds 18 more, so 44 is the
right figure only if both directories are counted together) and a tested-device table in
`assets/known-devices.md`.

**Relevance to streamboat**: there is **no open-source implementation of the TIDAL Connect
receiver protocol**. If streamboat ever wants to be a Connect target, that is a reverse-
engineering project with no precedent to borrow from, and the certificate-based device
authentication suggests it is not casually reproducible. Treat "be a Connect target" as out of
scope; "control a Connect target" is equally undocumented.

---

### 10. Official TIDAL SDKs

All three are **Apache-2.0** and published by TIDAL (Block, Inc.).

#### 10.1 tidal-sdk-web (204★ / 24 forks / 24 open issues, last push 2026-08-31)

Monorepo packages, each Apache-2.0: `api`, `auth`, `common`, `event-producer`, `player`,
`player-web-components`, `template`, `true-time`. Published to npm under `@tidal-music`.
Tooling: pnpm, Vite, Vitest, ESLint, TypeDoc, Renovate. Recent commits include a TypeScript 7
upgrade (2026-08-31).

**Auth** (`ref:tidal-sdk-web/packages/auth/src/auth/auth.ts`): OAuth2 PKCE against
`login.tidal.com` / `auth.tidal.com/v1/oauth2/token`, plus a device-authorization flow, plus
client-credentials. Scopes are passed as an array and joined with spaces; there is explicit
handling for scope changes and `clientUniqueKey` mismatches, and a session-ID guard so a logout
during an in-flight token request cannot resurrect credentials. Storage goes through a pluggable
`StorageAdapter` with AES-GCM encryption by default.

**Playback** (`ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts`):
two paths.
- *Legacy* (`_fetchLegacyPlaybackInfo`, native player only, line 168+):
  `GET {legacyApiUrl}/{productType}s/{productId}/playbackinfo` with `audioquality` (or
  `videoquality=HIGH`), `playbackmode=STREAM`, `assetpresentation=FULL`, and headers
  `authorization`, `x-tidal-playlistuuid`, `x-tidal-prefetch`.
- *Modern* (`_fetchTrackManifest`, line 341+): `GET /trackManifests/{id}` on
  `https://openapi.tidal.com/v2/` with query `adaptive`, `formats`,
  `manifestType: isFairPlaySupported ? 'HLS' : 'MPEG_DASH'`, `shareCode`, `uriScheme: 'DATA'`,
  `usage: 'PLAYBACK'`, and header `x-playback-session-id`. The response's
  `data.attributes.uri` is a `data:` URL that is split back into `{manifest, manifestMimeType}`.
  Normalization data arrives as `trackAudioNormalizationData` / `albumAudioNormalizationData`
  with `replayGain` and `peakAmplitude`.

**Quality → formats mapping** (lines 286-301) — worth copying verbatim as an abstraction:
```
HI_RES / HI_RES_LOSSLESS -> ['HEAACV1','AACLC','FLAC','FLAC_HIRES']
LOSSLESS                 -> ['HEAACV1','AACLC','FLAC']
HIGH                     -> ['HEAACV1','AACLC']
LOW / default            -> ['HEAACV1']
```
The client sends a *set of acceptable formats* and reads back what it got, rather than asking for
a tier and hoping. This is strictly better than the unofficial API's single `audioquality` string,
and streamboat can emulate the pattern (request the ladder, trust the response) even against v1.

**DRM** (`ref:tidal-sdk-web/packages/player/src/player/shakaPlayer.ts:632-667`,
`fairplay-drm.ts:6`): Widevine license at `https://api.tidal.com/v2/widevine`; FairPlay
certificate at `https://fp.fa.tidal.com/certificate` and license at
`https://fp.fa.tidal.com/license`. Three player backends — `browser` (HTMLMediaElement/Web
Audio), `shaka` (DASH/HLS + DRM), `native` (a proprietary C++ `window.NativePlayerComponent`
that is *not* in this repo).

**The `native` player is the tell**: TIDAL's own desktop app uses a closed-source native player
component that the open SDK only declares an interface for. Third parties get browser/Shaka.

#### 10.2 tidal-sdk-android (49★ / 4 forks / 5 open issues, last push 2026-09-03)

Kotlin, Apache-2.0, modules `auth`, `common`, `eventproducer`, `player`, `tidalapi`, `bom`,
`template`. Retrofit + OkHttp + Dagger + kotlinx.serialization; ExoPlayer (`androidx.media3`)
for playback; `EncryptedSharedPreferences` for tokens; a `TokenMutex` serializing refresh.

`ManifestMimeType` (`ref:tidal-sdk-android/player/streaming-api/src/main/kotlin/com/tidal/sdk/
player/streamingapi/playbackinfo/model/ManifestMimeType.kt`) is the canonical enumeration:
`EMU = application/vnd.tidal.emu`, `BTS = application/vnd.tidal.bts`,
`DASH = application/dash+xml`, `HLS = application/vnd.apple.mpegurl`.

Notable design ideas: separate `streamingWifiAudioQuality` vs `streamingCellularAudioQuality`
policies; `usage=playback` vs `usage=download` on the same manifest endpoint; offline entries
carry `offlineRevalidateAt`/`offlineValidUntil` so licences must be re-validated — the official,
compliant shape of an offline feature.

#### 10.3 tidal-sdk-ios (41★ / 13 forks / 13 open issues, last push 2026-08-24, v0.12.6)

Swift, Apache-2.0, `Sources/{Auth,Common,EventProducer,Offliner,Player,Template,TidalAPI}`.
Dependencies (`ref:tidal-sdk-ios/Package.swift:47-53`): GRDB.swift ≥6.27 (local DB),
SWXMLHash ≥7.0.2 (HLS/manifest parsing), KeychainAccess ≥4.2.2, **Kronos 4.2.2 (NTP time sync —
because DRM licence validity cannot trust the device clock)**, swift-log, AnyCodable.
Playback is `AVQueuePlayer`; DRM is FairPlay against `fp.fa.tidal.com`.

The `Offliner` module is the only complete offline-download implementation in the whole
reference set, and it is the officially sanctioned one.

#### 10.4 Can third parties use them?

Legally the *code* is Apache-2.0 — you may vendor, fork and modify it. But the *service* refuses:
you need a developer-portal `clientId`, the `playback` scope, and per the Developer Guidelines the
Player module must be "official, unmodified" and third parties get previews. Forking the SDK to
point at the unofficial v1 API would be an odd hybrid: Apache-2.0 code calling endpoints the
Apache-2.0 licensor's own terms forbid you to call that way.

**Borrow:** the `formats[] + manifestType + uriScheme + usage` request abstraction; the
`ManifestMimeType` enum; the `CredentialsProvider` interface boundary (auth module knows nothing
about playback, player depends on an interface not a singleton); the token-refresh mutex /
session-ID guard; NTP time sync for anything expiry-sensitive; the offline validity-window model
if streamboat ever does offline. **Avoid:** assuming the official endpoints will serve you full
tracks; hardcoding DRM licence URLs (the SDK's own TODO says to read them from the manifest).

---

### 11. python-tidal (tidalapi) — the de-facto unofficial API library

`ref:python-tidal` · https://github.com/EbbLabs/python-tidal · **LGPL-3.0-or-later** · 560★ / 124
forks / 21 open issues · last push 2026-08-14 · v0.8.11 · maintainer `tehkillerbee`, formerly
`tamland` and `morguldir`. **The repository has moved to the EbbLabs org** — the prior survey's
`tamland/python-tidal` URL is stale (`ref:python-tidal/pyproject.toml:10`).

Runtime deps are deliberately tiny: `requests`, `python-dateutil`, `typing-extensions`,
`isodate`, `mpegdash`, `pyaes`.

**Credentials** (`ref:python-tidal/tidalapi/session.py:155-185`): four values —
`client_id`, `client_secret`, `client_id_pkce`, `client_secret_pkce` — each stored as a
**double-base64-encoded, split-in-two** byte literal and reassembled at runtime. Same category of
obfuscation as Sone's XOR. If `client_secret` is empty it falls back to `client_id`.

**Config** (`session.py:100-135`): `api_oauth2_token = https://auth.tidal.com/v1/oauth2/token`,
`api_pkce_auth = https://login.tidal.com/authorize`,
`api_v1_location = https://api.tidal.com/v1/`, `api_v2_location = https://api.tidal.com/v2/`,
`openapi_v2_location = https://openapi.tidal.com/v2/`,
`pkce_uri_redirect = https://tidal.com/android/login/auth`,
`image_url = https://resources.tidal.com/images/%s/%ix%i.jpg`,
`video_url = https://resources.tidal.com/videos/%s/%ix%i.mp4`,
`listen_base_url = https://listen.tidal.com`, `share_base_url = https://tidal.com/browse`.
`item_limit` is clamped to 10000 with a warning. There is an `alac` flag whose docstring warns
that `alac=false` turns video streams into audio-only streams and `num_videos` into `num_tracks`
in playlists.

**Quality enum** (`ref:python-tidal/tidalapi/media.py:57-62`): `LOW`, `HIGH`, `LOSSLESS`,
`HI_RES_LOSSLESS`, default `HIGH`. **`HI_RES` (MQA) is gone from the enum** — corrects the prior
survey. Separately `MediaMetadataTags` are `HIRES_LOSSLESS`, `LOSSLESS`, `DOLBY_ATMOS`, and
`AudioMode` is `STEREO` / `DOLBY_ATMOS`. Note the enum-vs-tag naming mismatch
(`HI_RES_LOSSLESS` vs `HIRES_LOSSLESS`) — always use lookup tables, never string equality.

**`StreamManifest`** (`media.py:624-741`) is the reference implementation of manifest handling:
for MPD it parses with `mpegdash`, extracts `urls`, maps `codecs` (`flac` → FLAC;
`mp4a.40.5` = LOW 96k and `mp4a.40.2` = HIGH 320k both → MP4A), reads `audio_sampling_rate`, and
**hardcodes `encryption_type = "NONE"` with a `TODO: Handle encryption key`**; for BTS it reads
`urls`, `codecs`, `mimeType`, `encryptionType` and `keyId`. `DashInfo` can also emit an HLS m3u8.

Exceptions are typed: `ObjectNotFound`, `URLNotAvailable`, `StreamNotAvailable`,
`UnknownManifestFormat`, `TooManyRequests`, `MPDNotAvailableError`.

**LGPL-3.0 matters for streamboat**: dynamic linking / separate-process use is fine under LGPL
even in a differently-licensed application, but statically vendoring or porting the code creates
obligations. If streamboat is not Python, this is a reference document rather than a dependency.

---

### 12. tidal-cli — the official-API CLI + MCP precedent

`ref:tidal-cli` · https://github.com/lucaperret/tidal-cli · MIT · 10★ / 4 forks · created
2026-03-16 · last push 2026-08-23 · v1.2.5 · TypeScript, Node ≥20, Commander 14.

Uniquely in this set it uses the **official** SDK: `@tidal-music/api ^0.22.0` and
`@tidal-music/auth ^1.6.0`, with a hardcoded public client ID `PYVtmSHMTGI9oBUs` and PKCE, a
loopback redirect on `http://localhost:17893/callback`, and scopes
`collection.read/write`, `playlists.read/write`, `playback`, `user.read`,
`recommendations.read`, `entitlements.read`, `search.read/write`
(`ref:tidal-cli/src/auth.ts:17-36`).

Playback (`ref:tidal-cli/src/playback.ts`): `GET /trackManifests/{id}` with
`adaptive=false, formats=<by quality>, manifestType=MPEG_DASH, uriScheme=DATA, usage=PLAYBACK`,
then base64-decodes the `data:` URI, handles both BTS JSON and DASH XML, and for DASH extracts
`initialization="…"`, `media="…$Number$…"` and `<S d= r=>` repeat counts, downloads the init
segment plus every media segment sequentially, and concatenates them into a local `.flac`/`.mp4`.

**It carries `trackPresentation` and `previewReason`** and prints "Preview reason:" — direct
evidence that this official path frequently returns previews rather than full tracks.

Node-specific hacks worth knowing: `@tidal-music/auth` expects browser globals, so
`ref:tidal-cli/src/session.ts` installs a `localStorage` polyfill backed by
`~/.tidal-cli/session.json` (mode 0600) plus `CustomEvent`/`EventTarget` polyfills.

**Borrow:** the CLI-command → `*Data()` function split so the same code backs the CLI and the MCP
server; the loopback-redirect PKCE flow with a temporary local HTTP server; the cursor-pagination
helper with bounded retries (`src/pagination.ts`). **Avoid:** taking its playback path as proof
that the official API works for a real player — the `previewReason` field says otherwise.

---

### 13. tidalt — Go TUI + daemon, bit-perfect ALSA

`ref:tidalt` · https://github.com/Benehiko/tidalt · Apache-2.0 · 2★ / 4 forks / 0 open issues ·
created 2026-03-13 · last push 2026-09-06 · Go 1.26, ~13,078 lines Go in `internal/`+`cmd/` plus
263 lines of C.

README is candid: it states the project was written almost entirely with LLM coding assistants,
and *"Linux only. Requires a Tidal HiFi or HiFi Plus subscription."*

**Correction to the prior survey**: its auth and streaming path is **not** the official API.
`BaseURL` is the unofficial `api.tidal.com/v1`, the stream endpoint is
`GET /tracks/{id}/urlpostpaywall?urlusagemode=STREAM&audioquality=<q>&assetpresentation=FULL
&countryCode=<cc>` (`ref:tidalt/internal/tidal/api.go:304-311`), and the client ID
`<client_id A>` is hardcoded in plaintext at `ref:tidalt/internal/tidal/client.go:17` — a
client ID extracted from an official app, not one issued to this project. **One caveat the
fact-check pass added**: `client.go:25` also defines `BaseURLV2 = https://openapi.tidal.com/v2`,
and it is actually called — `api.go:595` (`/userRecommendations/me/relationships/myMixes`) and
`:650` (`/playlists/{id}/relationships/items`) both hit the openapi.tidal.com/v2 host. So the
precise statement is: tidalt's *auth and streaming* path is entirely unofficial, but it does use
the official v2 host for mixes/playlist-item relationships — a hybrid, not a clean split.

**Stack**: Bubble Tea 1.3.10 + Bubbles 1.0.0 + Lipgloss 1.1.0 (TUI); `godbus/dbus/v5` (MPRIS2 and
PipeWire device reservation); `golang.org/x/oauth2` (device flow); `go.etcd.io/bbolt` (metadata
cache at `~/.local/share/tidalt/tidal-cache.db`); `docker/secrets-engine/store` for the keyring
with a `filippo.io/age`-encrypted file fallback at `~/.config/tidalt/secrets`;
`mdp/qrterminal/v3` for the device-code QR in the terminal. FFmpeg (libavformat/libavcodec/
libswresample) via cgo for decode, with a `staticav` build tag for distro packages that bundle a
static FFmpeg; ALSA via cgo (`-lasound`) in `internal/player/alsa.c`.

**Architecturally the most interesting thing** is the daemon/client split: `tidalt daemon` holds
the exclusive device lock and registers both `org.mpris.MediaPlayer2` and a private
`io.tidalt.App` D-Bus interface; `tidalt` in client mode is a TUI that forwards commands over
D-Bus. That is *exactly* the desktop-plus-headless shape streamboat needs, and it is the only
implementation of it in the set (`ref:tidalt/docs/client-server.md`).

Two more transferable audio findings:
- **PipeWire device reservation**: acquire `org.freedesktop.ReserveDevice1.Audio<N>` over D-Bus
  before opening `hw:` exclusively, and release on stop. Sone does not do this; tidalt does.
- **Distinguish two ALSA failure modes**: a format-negotiation refusal (an `errFormatRefused`
  sentinel → fall back to `plughw:` and mark the session as *not* bit-perfect) versus a
  device-busy error (keep retrying `hw:`). Memoize the verdict per device. Conflating them is why
  naive implementations silently drop to `plughw:`.
- Format preference is source-dependent: for 16-bit sources `S32_LE > S16_LE > S24_3LE > S24_LE`
  (S32 first because of a specific Hidizs USB issue); for 24-bit sources
  `S24_3LE > S24_LE > S32_LE`.

Packaging: `docker-bake.hcl` producing `.deb`, `.pkg.tar.zst` and `.rpm` for amd64 and arm64 plus
a Docker image; a systemd user service; a `tidal://` URL handler.

---

### 14. tidalrs — Rust API client library

`ref:tidalrs` · https://github.com/phayes/tidalrs · **MIT** · 19★ / 7 forks / 2 open issues ·
created 2025-09-16 · last push 2026-09-02 · v0.5.0 · published on crates.io/docs.rs.

Deps: `reqwest 0.12` (rustls), `tokio 1.47`, `serde`, `thiserror 2`, `strum 0.27`,
`arc-swap 1`, `stream-download 0.22.4`, `base64 0.22`, `url 2.5.7`, `async-recursion`.
Dev-deps include `rodio 0.21` (only for `examples/audio_streaming.rs`) and `mockito`.

Three stream endpoints are implemented against `api.tidal.com/v1`
(`ref:tidalrs/src/track.rs`): `/tracks/{id}/urlpostpaywall` (line 142),
`/tracks/{id}/playbackinfo` (line 279), `/tracks/{id}/playbackinfopostpaywall` (line 321).
Device-flow auth with `scope "r_usr w_usr w_sub"`; tokens held in an `Arc<Authz>` swapped via
`arc-swap` so the read path is lock-free; refresh serialized by a `Semaphore` to avoid a
thundering herd; exponential backoff from 100 ms with a configurable ceiling (default 5 s);
`Authz` is `Serialize` so the *application* owns persistence.

README disclaimer: *"This library is not officially affiliated with Tidal. Use at your own risk
and ensure compliance with Tidal's Terms of Service."*

**This is the single most reusable dependency for a Rust streamboat**: MIT (no copyleft),
async, typed, actively released, and it deliberately stops at "return a manifest/URL" — it does
not decrypt, does not download, does not play. Its main gaps are lyrics, mixes/radio, videos and
DASH parsing (left to the consumer). Risk: 19★, one author, v0.5.0 — treat it as a fork
candidate, not a load-bearing dependency.

---

### 15. TidalSwift — Apple-platform precedent

`ref:tidalswift` · https://github.com/melgu/TidalSwift · 97★ / 12 forks / 7 open issues · last
push 2026-07-08 · Swift + SwiftUI, macOS/iOS.

**No LICENSE file exists in the repository** (verified by `ls -a`) — the prior survey is right
that it is effectively unlicensed, which means *all rights reserved by default*. **Do not copy
code from it.** Read it for architecture only.

Architecture worth noting: a clean split between `TidalSwiftLib` (API + models + offline DB) and
the `TidalSwift` app (SwiftUI + `Player`). Device OAuth flow with hardcoded plaintext credentials
at `ref:tidalswift/TidalSwiftLib/Sources/TidalSwiftLib/Config.swift:12-13`
(`OAuthClientID = "<client_id C>"`, `OAuthClientSecret = "<redacted client_secret C; see the cited source file>"`).
Streaming uses the oldest endpoint, `GET /v1/tracks/{id}/streamUrl` with `soundQuality`
(`.../Session/ContentUrls.swift:14`). Playback is `AVPlayer`. It has a real
download-and-tag feature (SwiftTagger) and an `OfflineDB` with reference counting so a track
favourited from two playlists is stored once — the reference-counting idea is genuinely good even
if the feature is out of scope. Latest commit (2026-07-08) is "Fix token refresh when app is open
for a long time", which is a useful reminder that long-lived-session refresh is a real bug class.

---

### 16. Smaller / historical projects

| Project | License | Stars | Last activity | Status |
|---|---|---|---|---|
| `ref:libopentidal` (Fokka-Engineering/libopenTIDAL) | MIT | not resolvable via GitHub API from this session | 2021-05-25 | Dead. ANSI C, libcurl-only. Documents `playbackinfopostpaywall` **and** `playbackinfoprepaywall` (`Source/OTService/OTServiceStd.c:165-167`). Device flow with a 5-minute pre-expiry refresh buffer. Per-thread curl handles. |
| `ref:tidalgo` (tcpj/tidalgo) | none stated | 1★ | last commit 2018-02-06 (GitHub `updated_at` reports 2019-05-09 — these are two different metrics, see §18; cite one and label it) | Dead. `api.tidalhifi.com/v1/`, username/password + `X-Tidal-SessionId`. Its source comment records that FLAC via the standard TIDAL key is **encrypted** while the "WiMP" key returns unencrypted FLAC — historical evidence that the encryption behaviour is client-ID-dependent, which matches Strawberry's user-facing message. |
| `ref:dotnet-tidal-usdk` (SacredSkull) | MIT with an explicit anti-piracy clause | 0★ | 2020-05-07 | Dead. `/v1/tracks/{id}/streamUrl` with `soundQuality`, Android token `kgsOOmYk3zShYrNP`, `clientUniqueKey vjknfvjbnjhbgjhbbg`, LOW/HIGH only. Its licence-with-a-piracy-clause is a precedent worth considering. |
| `ref:tidal-api-docs` (gkasdorf) | none | 0★ | 2023-04-08 content, repo touched 2026-07-13 | 4 markdown files. Documents PKCE with client ID `CzET4vdadNUFQ5JU`, redirect `https://listen.tidal.com/login/auth`, `appMode=WEB`, token endpoint `https://login.tidal.com/oauth2/token`, scope `r_usr w_usr`, 24-hour access-token TTL, and a manual "copy the token out of devtools" fallback. Explicitly warns the client ID changes without notice and that CORS blocks pure-browser flows. No streaming documentation at all. |
| `ref:tidal-fokka-engineering-` (Fokka-Engineering/TIDAL) | MIT | not resolvable | 2022-02-10 | Two files. Historical value: *"I've decompiled various TIDAL App Versions and debundled the Browser JS App"* and *"I reversed engineered the TIDAL device authorization grant (RFC 8628) since the web flow (RFC 6749) is reCaptcha v3 secured."* Disclaimer: *"I deeply discourage you from building and distributing copyright-infringing apps. Create something that adds up to TIDALs Service and improves it."* The linked wiki (openTIDAL/docTIDAL) is not resolvable from this session. |

Also encountered but not in the reference set: `pauljhdrake/low-tide` (a terminal UI TIDAL
client), `yaronzz/Tidal-Media-Downloader` (a downloader — explicitly out of scope, cited here only
for its 2026-03-21 API-key breakage report), `michaelherger/lms-plugin-tidal`,
`GioF71/upmpdcli-docker` TIDAL plugin, and Music Assistant's TIDAL provider — all *(unverified,
not read)*.

---

### 17. Corrections to the prior survey

Verified against source; the preliminary survey pass is wrong on these points:

1. **python-tidal repository** is `EbbLabs/python-tidal`, not `tamland/python-tidal`
   (`ref:python-tidal/pyproject.toml:10`). Version is 0.8.11, not 0.8.10.
2. **python-tidal `Quality` has no `HI_RES`** — only `LOW`, `HIGH`, `LOSSLESS`,
   `HI_RES_LOSSLESS` (`ref:python-tidal/tidalapi/media.py:57-62`).
3. **mopidy-tidal repository** is `EbbLabs/mopidy-tidal`; version 0.3.13; Python ≥3.12.
4. **High Tide has 8 translations, not 18** (`ref:high-tide/po/LINGUAS`).
5. **High Tide is not "v1.6.0, August 2026"** — no such version is verifiable from the checkout;
   the last commit is 2026-08-21.
6. **High Tide's login is PKCE-only in the UI** — there is no device-code path in `src/login.py`.
7. **Sone is v0.21.0**, Tauri `=2.11.2`; it uses `gstreamer 0.23` + the `alsa 0.10` crate.
   No symphonia/cpal/rodio anywhere.
8. **Sone's gapless uses `concat` with a serialized attach/detach executor**, not
   "attach/detach" done inline; the runtime check is `ElementFactory::find("concat")`, not a
   GStreamer version check.
9. **sone-windows uses `wasapi2sink`**, not `wasapisink`, and is at v0.16.0 against upstream
   0.21.0 — it is missing MCP, overlay, signal-path, theming, play-reporting, profile, feed and
   updater modules entirely.
10. **tidalt does not use the official TIDAL API.** It uses `api.tidal.com/v1` with a hardcoded
    client ID extracted from an official app (`ref:tidalt/internal/tidal/client.go:17`,
    `ref:tidalt/internal/tidal/api.go:304`). The survey's "legal_posture: uses official Tidal API"
    is wrong.
11. **tidal-hifi has four controllers**, including a `TidalApiController` (beta), not three
    (`ref:tidal-hifi/src/preload.ts:39-70`).
12. **tidal-hifi is v8.1.3** on castlabs Electron `v43.0.0+wvcus`, and its local API port is
    47836 (default, configurable).
13. **Strawberry's API host is `api.tidalhifi.com/v1`** and it offers five quality options
    including `HI_RES`; default quality `LOSSLESS`, default stream method
    `PlaybackInfoPostPaywall`.
14. **TidaLuna hits `desktop.tidal.com/v1/tracks/{id}/playbackinfo`** — no `postpaywall` suffix,
    no `countryCode` on that call.
15. **TidalSwift has no LICENSE file** — treat as all-rights-reserved.
16. The survey's claim that the official SDKs let third parties stream full tracks is
    contradicted by TIDAL's own Developer Guidelines and by `previewReason` in tidal-cli.

---

### 18. Verification-pass corrections and new findings (2026-09-07)

All of the following were produced by an independent fact-check pass and are additional to the
corrections already made inline above (§2.1, §2.2, §2.3, §2.4, §2.5, §2.8, §4.4, §5, §9, §13, §16).
Sources are cited per item; nothing here duplicates a citation already given inline.

**A. Sone's Flathub manifest is now known — Open question 14 answered.**
`flathub/io.github.lullabyX.sone` (fetched 2026-09-07) builds against `org.gnome.Platform` **50**
with the Rust and Node 24 SDK extensions, one `sone` module from the v0.21.0 tag. `finish-args` are
exactly `--socket=wayland`, `--socket=fallback-x11`, `--socket=pulseaudio`, `--device=dri`,
`--share=ipc`, `--share=network`, `--talk-name=org.kde.StatusNotifierWatcher`,
`--env=WEBKIT_DISABLE_COMPOSITING_MODE=1`. **There is no `--device=all` and no raw ALSA
filesystem grant** — Sone's own exclusive/bit-perfect ALSA writer, the single most sophisticated
artifact in this landscape (§1.9, §2.3), cannot function in the build Flathub actually ships.
High Tide's manifest is the same shape (PulseAudio socket + read-only PipeWire, no raw device
access). The concrete conclusion for streamboat: **Flathub is a convenience tier (shared audio
server only); exclusive/bit-perfect output is a deb/rpm/AUR/Nix/Snap-only feature**, and the Snap
route needs the same explicit `snap connect streamboat:alsa` step Sone documents. Source:
https://raw.githubusercontent.com/flathub/io.github.lullabyX.sone/master/io.github.lullabyX.sone.yml
(fetched 2026-09-07); `ref:high-tide/build-aux/io.github.nokse22.high-tide.json`;
`ref:sone/snap/snapcraft.yaml`.

**B. A pure-Rust audio stack precedent exists — for Spotify, not TIDAL.** librespot
(https://github.com/librespot-org/librespot, MIT, ~7.1k★) is an unofficial Spotify-Premium-only
client library that is architecturally close to what streamboat needs: a core library, thin
downstream frontends (`spotifyd` daemon, `ncspot` TUI, `librespot-java`), and a `Sink` trait
(`start`/`stop`/`write(AudioPacket, &mut Converter)`) with a `sink_as_bytes!` macro handling
F32/S32/S24/S16 conversion, registered by name in `pub const BACKENDS: &[(&str, SinkBuilder)]`
(`playback/src/audio_backend/mod.rs`). Nine backends behind cargo features: alsa, pulseaudio,
jack, portaudio, **rodio+cpal (default)**, rodiojack, sdl2, gstreamer. Decoding is **Symphonia
0.5**. Its own disclaimers are exactly streamboat's posture: *"librespot only works with Spotify
Premium. This will remain the case…"* and *"Using this code to connect to Spotify's API is
probably forbidden by them. Use at your own risk."* This refutes the framing of the original
Open question 2 ("pure Rust has zero precedent") — the precedent exists, just not for TIDAL. What
remains genuinely TIDAL-specific and unsolved in pure Rust is DASH/BTS manifest handling and
FLAC-in-MP4 demuxing; price that as real work either way. Source:
https://github.com/librespot-org/librespot (README, licence, stars);
https://raw.githubusercontent.com/librespot-org/librespot/dev/playback/Cargo.toml;
https://raw.githubusercontent.com/librespot-org/librespot/dev/playback/src/audio_backend/mod.rs
(all fetched 2026-09-07).

**C. `cpal` cannot do WASAPI exclusive mode — the concrete cost of the pure-Rust option on
Windows.** `cpal` operates in WASAPI *shared* mode only; exclusive-mode support is a long-standing
open request (RustAudio/cpal#459), and cpal's own guidance points at ASIO as the low-latency
workaround. The standalone `wasapi` crate (HEnquist/wasapi-rs) supports both shared and exclusive
modes — it's what CamillaDSP uses (see next item). So a pure-Rust streamboat needs three
hand-written output backends behind a librespot-`Sink`-shaped trait — `alsa` crate for Linux
`hw:`, `wasapi` crate for Windows exclusive, `coreaudio-rs` for macOS hog mode — regardless of
whether decode is Symphonia or GStreamer. The GStreamer route gets Linux via the same `alsa`
crate path Sone already wrote and Windows via `wasapi2sink exclusive=true`, but gets **nothing**
on macOS (`osxaudiosink` has no exclusive property, confirmed in §5). **Price the macOS output
backend as hand-written work under either stack choice.** Source:
https://github.com/RustAudio/cpal/issues/459; https://docs.rs/wasapi;
https://github.com/HEnquist/wasapi-rs.

**D. macOS exclusive/bit-perfect output has two usable precedents outside this reference set —
Open question 8/16 was "unresearched", not actually unanswerable.** (1) **CamillaDSP**
(HEnquist, Rust) supports ALSA, PulseAudio, Jack, WASAPI (shared *and* exclusive) and CoreAudio in
one codebase; its CoreAudio playback device has an `exclusive` setting explicitly documented as
hog mode, implemented via an extended fork of `coreaudio-rs`, plus playback-driven rate control on
ALSA/WASAPI/CoreAudio alike. (2) **MPD**'s `src/output/plugins/OSXOutputPlugin.cxx` is the C++
reference: a `hog_device` option that sets the device's hog PID via
`kAudioDevicePropertyHogMode`, and `osx_output_set_device_format()` scoring and applying
`kAudioStreamPropertyPhysicalFormat` to switch the device's sample rate/format, plus DoP support.
`/home/user/streamboat/docs/research/audio-pipeline.md:677-681,1117,1501` already cites
CamillaDSP's CoreAudio backend for this — cross-reference it from here rather than calling the
area unresearched. Source: https://github.com/HEnquist/camilladsp/blob/master/backend_coreaudio.md
and https://www.camilladsp.com/docs/camilladsp/4.0.x/backend_wasapi/;
https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/src/output/plugins/
OSXOutputPlugin.cxx.

**E. GStreamer bundling carries licensing obligations the report had not priced.**
Implications 9 and 25 recommend GStreamer and bundling it into Windows/AppImage installers, but
`gstreamer1.0-libav` (which Sone's own `.deb`/`.rpm` dependency lists include, dynamically) is
FFmpeg-derived and covers TIDAL's AAC (HIGH/LOW tier) and Atmos-adjacent (AC-4/E-AC-3) decode
paths. A dynamic distro dependency (Sone's Linux packages) puts the obligation on the distro; a
*bundled* copy (`ref:sone-windows/scripts/prepare-gstreamer.js` copying DLLs into the NSIS/WiX
payload, or `appimage.bundleMediaFramework: true`) puts LGPL relinking/notice obligations on
streamboat directly. **This is undecided and flagged as work to do, not resolved here**: does the
Windows/AppImage bundle include the `libav` plugin, or only `base`/`good`/`bad` (FLAC + the
ISOBMFF/DASH demuxers + the AAC decoder that lives in `gst-plugins-bad`, no FFmpeg)? Excluding
`libav` shrinks both the licence surface and the installer size. Source: `ref:sone/src-tauri/
tauri.conf.json` (deb/rpm depends lists, `appimage.bundleMediaFramework`);
`ref:sone-windows/scripts/prepare-gstreamer.js`.

**F. Dolby Atmos handling is undefined everywhere in the reference set — a real user-facing
failure mode with no plan.** python-tidal models `AudioMode = STEREO | DOLBY_ATMOS` and
`MediaMetadataTags` includes `DOLBY_ATMOS`; Sone models the same fields
(`audio_mode: Option<String>`, `tidal_api.rs:582-584`; `audio_modes: Option<Vec<String>>` on
album/track, `:156,:287`) but its quality cascade (`commands/playback.rs:32-84`) selects on
`audioquality` only and never passes an audio-mode preference — whatever the API returns is fed
straight to GStreamer, decode result unverified. High Tide has no Atmos handling at all (`rg -i
atmos` over its `src/` returns nothing). **Undetermined and flagged as work**: whether
`playbackinfopostpaywall` accepts an audio-mode/immersive parameter, and what codec an Atmos
track's BTS manifest actually reports. streamboat needs an explicit policy — prefer stereo by
default; if an Atmos-only manifest arrives, refuse with a specific user-facing message rather than
an opaque "Internal data stream error." Source: `ref:sone/src-tauri/src/tidal_api.rs:156,287,
582-584`; `ref:sone/src-tauri/src/commands/playback.rs:32-84`; `ref:python-tidal/tidalapi/
media.py:87-94`.

**G. Internationalization is a day-one framework decision the report had not surfaced.** High
Tide ships 8 gettext locales through Meson (`po/LINGUAS`), the standard GNOME/Flathub path that
gets community translators for free. Sone has **no i18n at all** — no locale directory, no i18n
library in `package.json`, no `useTranslation`/`i18n` usage anywhere across 199 TS/TSX files; it
is English-only. i18n is cheap to add before ~90 components exist and expensive to retrofit
after. "Everything the native client does" includes shipping in more than English — put this on
the owner's decision list. Source: `ref:high-tide/po/LINGUAS`; `rg` over `ref:sone/src` and
`ref:sone/package.json` (no hits).

**H. The OAuth-redirect capture problem has three known solutions in this set, not one — worth
naming as a single decision.** (a) **Custom URI scheme**: Strawberry's `tidal://login/auth` with
`use_local_redirect_server = false` (`ref:strawberry/src/tidal/tidalservice.cpp:79-83`); Sone
registers a `tidal` deep-link scheme via `tauri-plugin-deep-link` plus `tauri-plugin-single-
instance` (needed so a second app launch forwards the URL to the first). (b) **Loopback HTTP
server**: tidal-cli binds `http://localhost:17893/callback`
(`ref:tidal-cli/src/auth.ts:17-36`); Sone also carries `tauri-plugin-oauth`; mopidy-tidal serves
its login page on port 8989. (c) **Manual paste**: High Tide's 91-line dialog asks the user to
paste the redirect URL back (`ref:high-tide/src/login.py`). Recommendation: desktop = custom
scheme with a loopback fallback; headless = device code with a terminal QR (tidalt's
`mdp/qrterminal`) plus a paste-form fallback; keep mopidy-tidal's login-hack (§8) as the
zero-protocol fallback for clients that can't render either.

**I. The catalogue/browse surface — roughly 70% of what a TIDAL client actually is — is nearly
absent from this report, which is auth- and playback-centric.** This is a scope gap, not a
correction: an implementer following only this document can log in and play a track with no
guidance on home/explore, mixes, artist/album pages, favourites, playlist CRUD, pagination, or
list virtualization. The endpoint catalogue itself belongs in `docs/research/tidal-api.md`, but
"how each project turns TIDAL's page/module tree into UI" is this report's job and is only
covered by one sentence about TidaLuna. What's confirmed from the checkouts: Sone hits
`/pages/album`, `/pages/mix`, `pages/artist?…` and `/search`, split across `commands/{pages,feed,
library}.rs`, with the frontend using `@tanstack/react-virtual` for long lists and the 150 MB
tag-indexed LRU (§2.5) for the responses. TidaLuna confirms the shape: `GET /v1/pages/album?
albumId=…&countryCode=…&locale=…&deviceType=DESKTOP` returns `rows[].modules[]`, and the
tracklist is the module with `type === "ALBUM_ITEMS"`. High Tide's answer is a page-class-per-view
directory (`src/pages/{album,artist,playlist,mix,search,explore,collection,...}.py`) with an
`auto_load_widget` for infinite scroll. See `docs/research/tidal-client-features.md` for the
feature-level treatment this report does not attempt to duplicate. Source: `rg` over
`ref:sone/src-tauri/src/tidal_api.rs` (`/pages/album`, `/pages/mix`, `pages/artist?`, `/search`);
`ref:sone/src-tauri/src/commands/{pages,feed,library}.rs`; `ref:TidaLuna/plugins/lib/src/classes/
TidalApi/index.ts`; `ref:high-tide/src/pages/`, `ref:high-tide/src/widgets/auto_load_widget.py`.

**J. Contributor counts (explicitly asked for, left as Open question 18) are not obtainable from
this environment** — the reference checkouts are `--depth 1` shallow clones (`git log --format=
%an | sort -u` returns exactly one author per repo, which is an artifact of the clone depth, not
a fact about the project), and unauthenticated GitHub API contributor endpoints are refused by
this session's proxy. The method to get them, for whoever runs this next: `GET /repos/{owner}/
{repo}/contributors?per_page=100&anon=1` after attaching each repo, or read each repo's Insights →
Contributors page directly. Until then, treat the qualitative signals already gathered as the
best available proxy: Sone — single primary author (`lullabyX`), no CI (§2.8), 64 open issues;
High Tide — community project with a public Matrix channel and 8 community-supplied locales;
Strawberry — 341 forks, 21 open issues, near-daily commit activity.

**K. Minor numeric corrections not already folded inline**: Strawberry's star/fork/issue count
drifted by one between the original pass and this one (3,947→3,948 stars, 340→341 forks — cite
whichever count is current at read time, the drift itself is not significant); "TIDAL-Media-
Downloader #1213" (§17-adjacent, cited for the 2026-03-21 unofficial-client-ID breakage) is a
**single reply-less GitHub issue about one public gist of keys**, not a "widely-reported"
breakage as an earlier draft phrased it — cite it as exactly that, one data point, not a trend.
Separately, Sone's own source comment claims `legacy_auth_notice_count` "never resets"
(`lib.rs:164-167`) but the code itself resets it to `0` in two places
(`ref:sone/src-tauri/src/commands/auth.rs:395,561`) — another instance of the pattern in finding
"C" of the original pass (§2.3's corrected buffer-duration): **a comment in Sone's source is not
proof of Sone's behaviour; read the expression, not the docstring, before porting a number or a
claim.**

---

## Comparison tables

### A. Overall

| Project | Stack | Platforms | Quality ceiling | Bit-perfect | Feature breadth | License | Stars | Maturity | Fit for streamboat |
|---|---|---|---|---|---|---|---|---|---|
| **Sone** | Tauri 2 + Rust + React 19/TS | Linux desktop | HI_RES_LOSSLESS 24/192 | **Yes** (exclusive ALSA) | Very high (~28 areas) | GPL-3.0-only | 437 | Active, 1 maintainer, v0.21.0 | **Closest architectural model** |
| **sone-windows** | same, forked | Windows (+Linux/mac via Tauri) | HI_RES_LOSSLESS | Yes (WASAPI exclusive) | High, minus 8 modules | GPL-3.0-only | n/a | Stale fork of v0.16.0, "may not be maintained" | Windows sink + DLL bundling only |
| **High Tide** | Python + GTK4/libadwaita + python-tidal | Linux (Flatpak) | HI_RES_LOSSLESS | No | High | GPL-3.0 | 671 | Active community, 90 open issues | UI/UX + Flatpak + PKCE model |
| **Strawberry** | C++17 + Qt 6 + GStreamer | Linux, BSD free; macOS/Windows builds sponsor-only | HI_RES_LOSSLESS | **Corrected: no dedicated bit-perfect path.** WASAPI-exclusive is an explicit Windows-only setting; on Linux "exclusive" is only inferred from an `hw:`/`plughw:` device prefix (affects crossfade suppression, not conversion); `osxaudiosink` has none | TIDAL = one backend of many | GPL-3.0 | 3,948 | Very mature, very active | Encryption posture + settings model + Windows WASAPI-exclusive reference |
| **tidal-hifi** | Electron (castlabs Widevine) + TS | Linux, Windows, macOS | Whatever the web player serves (Chromium resamples to 48k by default) | No | Medium-high | MIT | 1,725 | Very active | Anti-model; borrow the local API + controller pattern |
| **TidaLuna** | TS mod inside official client | Win/mac (+Linux via tidal-hifi) | Official client's | n/a | Plugin platform | MS-PL | 591 | Active | Intelligence only; contains DRM key |
| **mopidy-tidal** | Python + Mopidy + GStreamer | Linux/macOS/Windows headless | HI_RES_LOSSLESS | No | Medium | Apache-2.0 | 123 | Active | **Headless mode model** |
| **tidal-connect** | Bash + Docker + proprietary binary | Linux ARM/x86 | LOSSLESS 16/44.1 (post-MQA) | Partly (direct ALSA) | Low (receiver only) | MIT (wrapper) | 167 | Maintained wrapper, dead binary | ALSA config lore only |
| **tidal-sdk-web** | TypeScript | Browsers (+ native bridge) | HI_RES_LOSSLESS *in theory* | No | SDK | Apache-2.0 | 204 | Active (TIDAL) | API abstraction shapes |
| **tidal-sdk-android** | Kotlin + ExoPlayer | Android 7+ | HI_RES_LOSSLESS + Atmos | n/a | SDK + offline | Apache-2.0 | 49 | Active (TIDAL) | Offline validity model |
| **tidal-sdk-ios** | Swift + AVPlayer | iOS/macOS/tvOS/watchOS | HI_RES_LOSSLESS + Atmos | n/a | SDK + offline | Apache-2.0 | 41 | Active (TIDAL) | Future mobile reference |
| **python-tidal** | Python library | any | HI_RES_LOSSLESS | n/a | API only | LGPL-3.0+ | 560 | Active | Endpoint reference |
| **tidal-cli** | TS + official SDK | CLI (Linux/mac/Win) + MCP | Official API (previews in practice) | No | Medium (CLI breadth) | MIT | 10 | Active, young | CLI/MCP structure |
| **tidalt** | Go + FFmpeg/ALSA cgo + Bubble Tea | Linux only | HI_RES_LOSSLESS | **Yes** | Medium | Apache-2.0 | 2 | Young, AI-authored | **Daemon/TUI split model** |
| **tidalrs** | Rust library | any | HI_RES_LOSSLESS | n/a | API only | **MIT** | 19 | Young, active | **Best Rust dependency candidate** |
| **TidalSwift** | Swift + SwiftUI + AVPlayer | macOS/iOS | HI_RES_LOSSLESS (claimed) | No | Medium + downloads | **none** | 97 | Low-activity | Read-only; do not copy |
| **libopenTIDAL** | ANSI C + libcurl | any | HI_RES | n/a | API only | MIT | — | Dead (2021) | Historical |
| **tidalgo** | Go | any | LOSSLESS | n/a | Minimal | none | 1 | Dead (2018) | Historical |
| **dotnet-tidal-usdk** | C#/.NET Core | any | HIGH | n/a | Minimal | MIT + anti-piracy clause | 0 | Dead (2020) | Licence-clause precedent |

### B. Auth and secrets

| Project | Flow(s) | Client credentials | Token storage |
|---|---|---|---|
| Sone | device code, PKCE (window + browser), token import | Embedded, XOR-masked; user-overridable in settings | AES-256-GCM `settings.json`; key in OS keyring + 0600 file |
| High Tide | PKCE only | From `tidalapi` (double-base64) | libsecret, JSON blob, schema `io.github.nokse22.high-tide` |
| Strawberry | PKCE authorization code, redirect `tidal://login/auth` | Compile-time `TIDAL_CLIENT_ID` or user-supplied; **no secret** | QSettings via Strawberry's credential encryption |
| tidal-hifi | none (browser session) | none | Chromium cookies/localStorage |
| TidaLuna | none (scrapes host app) | scraped `getCredentials()` | host app's |
| mopidy-tidal | device code or PKCE | config file, user-supplied or from `tidalapi` | `tidal-oauth.json` / `tidal-pkce.json`, plaintext |
| tidalt | device code | plaintext `<client_id A>` | keyring, else age-encrypted file |
| tidalrs | device code | caller supplies | caller serializes `Authz` |
| tidal-cli | official PKCE, loopback :17893 | plaintext public `PYVtmSHMTGI9oBUs` | `~/.tidal-cli/session.json` (0600) |
| TidalSwift | device code | plaintext id **and secret** in `Config.swift` | app storage |
| official SDKs | PKCE, device, client-credentials | developer-portal issued | Keychain / EncryptedSharedPreferences / AES-GCM localStorage |

### C. Audio output

| Project | Decode | Output | Gapless | Sample-rate switching | Normalization |
|---|---|---|---|---|---|
| Sone Normal | GStreamer `uridecodebin` | `autoaudiosink` | Yes (`concat` preroll) | delegated to the sound server | `volume` element, Tidal formula |
| Sone DirectAlsa | GStreamer → `appsink` | own ALSA writer thread | **No** (disabled) | Yes — reopens PCM per format hint | scalar in writer (off in bit-perfect) |
| sone-windows | GStreamer | `wasapi2sink exclusive=… low-latency=true` | Yes | via WASAPI exclusive | same |
| High Tide | `playbin3` | auto/pulse/alsa/jack/oss/pipewire | Yes via `about-to-finish` (off on pipewiresink) | No | `rgvolume`/`rglimiter` chain |
| Strawberry | GStreamer `playbin3` | per-platform; WASAPI-exclusive on Windows (explicit setting), inferred `hw:`/`plughw:` "exclusive" on Linux (not bit-perfect by itself), no exclusive path on macOS | Yes | No dedicated rate-matching — always two `audioresample` elements in the chain, unconstrained caps | ReplayGain **or** EBU R128 (R128 forces float caps, incompatible with bit-perfect) |
| tidal-hifi | Chromium | Chromium → PulseAudio/PipeWire | No | Only via a 192k Chromium flag + manual PW config | web player's |
| mopidy-tidal | GStreamer (Mopidy) | Mopidy's output | Mopidy's | No | No |
| tidalt | FFmpeg (cgo) | direct `snd_pcm` `hw:` with `plughw:` fallback | No | Yes, per-track negotiation | No |
| tidal-connect | proprietary binary | ALSA + optional softvol | binary's | No | softvol only |

---

## Implications for streamboat

### API and legal posture

1. **Design for the unofficial v1 API as the playback path**, because the official API cannot
   deliver full tracks to an unapproved third party today. Document this explicitly and honestly
   in the repository: the reason is TIDAL policy, not technical preference.
2. **Keep an official-API adapter behind the same internal interface.** The official `/v2`
   `trackManifests` shape (`formats[]`, `manifestType`, `uriScheme`, `usage`, `adaptive`) is a
   strictly better abstraction than a single `audioquality` string, and if TIDAL ever opens the
   `playback` scope to third parties streamboat should be one config switch away from it.
   Model the internal interface on the official one, implement it against v1.
3. **Never decrypt.** If the manifest carries `encryptionType != "NONE"`, a non-empty
   `encryptionKey`, or a `securityType`/`securityToken` pair, refuse the stream and show
   Strawberry's message shape: explain that encryption depends on the client ID in use and point
   at the client-ID setting. Do not vendor, port, or reference TidaLuna's `decrypt.ts`.
4. **Make the client ID and secret user-supplied and first-class settings.** Strawberry's model
   (compile-time optional ID, no secret, user override in the UI) is the honest one. If
   streamboat ships defaults, state plainly in the README that they are extracted from an
   official app and may stop working; do not obfuscate them and pretend otherwise.
5. **Write the legal section from the projects' own positioning**, which is consistent across
   High Tide (*"Not affiliated in any way with TIDAL, this is a third-party unofficial client"*),
   Sone (*"an independent, community-driven project… not affiliated with, endorsed by, or
   connected to TIDAL"* + requires an active paid subscription + streaming-only), tidalrs (*"Use
   at your own risk and ensure compliance with Tidal's Terms of Service"*) and Fokka-Engineering
   (*"I deeply discourage you from building and distributing copyright-infringing apps"*). Add
   what they omit: that TIDAL's Content Guidelines prohibit reverse-engineering the service, and
   that account suspension is a real user-facing risk.
6. **Offline caching is a policy decision, not a feature decision.** High Tide already writes
   permanent unencrypted `.m4a` files by default when a music directory is set. If streamboat
   does anything here, follow the official SDKs' shape — an explicit user action, encrypted at
   rest, with an `offlineValidUntil`/`offlineRevalidateAt` equivalent that expires — and default
   it off. A short ephemeral read-ahead buffer for the current track is a different thing and is
   uncontroversial.

### Architecture

7. **Adopt the daemon/client split from day one** (tidalt's model, generalized). One `streamboat`
   core process owns the session, the queue and the audio device; the desktop UI and the CLI are
   both clients of it over a local IPC. This is the only shape that satisfies "desktop and
   headless, both now" without two codebases, and it makes the future mobile target a matter of
   writing another client. tidalt uses D-Bus; that is Linux-only, so streamboat needs a portable
   transport (a local socket with a documented JSON-RPC-ish protocol, plus MPRIS on Linux and
   SMTC/MPNowPlaying on Windows/macOS as *adapters over* it, not as the protocol).
8. **Put the audio backend behind a trait with per-platform implementations from commit one.**
   sone-windows exists because Sone did not. The seam is small: sink construction, device
   enumeration, and the exclusive-mode mechanism (ALSA `snd_pcm` / WASAPI exclusive / macOS
   CoreAudio hog mode). Everything above it is shared.
9. **Use GStreamer for decode+demux.** Every project that plays TIDAL well on Linux uses it
   (Sone, High Tide, Strawberry, mopidy-tidal); it handles DASH via `dashdemux`, FLAC, AAC and
   MP4 without writing a demuxer; it works on all three desktop platforms. The cost is a heavy
   runtime dependency and Windows DLL bundling (for which `ref:sone-windows/scripts/
   prepare-gstreamer.js` is a working solution) plus an FFmpeg-licensing decision if `libav` is
   bundled rather than left as a distro dependency (§18-E). The alternative — symphonia + cpal in
   pure Rust — is **not precedent-free for the architecture** (librespot proves core+`Sink`-trait
   +many-clients works, §18-B) **but is precedent-free for TIDAL specifically** and needs a
   hand-written DASH/BTS layer either way. Whichever decode stack is chosen, **macOS
   exclusive/bit-perfect output is hand-written work regardless**: GStreamer's `osxaudiosink` has
   no exclusive property (§5), and `cpal` cannot do WASAPI exclusive either (§18-C) — so the
   pure-Rust path additionally needs a hand-written `wasapi`-crate Windows backend, while
   GStreamer gets Windows for free via `wasapi2sink exclusive=true`. Two usable macOS references
   exist outside this reference set for whichever stack is chosen: CamillaDSP's CoreAudio hog-mode
   backend and MPD's `OSXOutputPlugin.cxx` (§18-D). *(This is a decision for the owner; see
   decision inputs.)*
10. **Separate three concerns explicitly**, as the official SDKs do: auth/credentials
    (`CredentialsProvider`-shaped), catalogue/API, playback engine. The auth module must not know
    about playback; the player must depend on an interface, not a session singleton. This is what
    makes the headless mode and the future mobile target cheap.
11. **Do not build one 7,000-line API client.** Split by resource (auth, catalog, playback,
    library, playlists, pages, search, user) behind a thin request layer that injects
    `countryCode`, the bearer token, `x-tidal-client-version` and handles 401/refresh centrally.

### Playback specifics to implement

12. **Quality cascade** with Sone's stopping rules: try `HI_RES_LOSSLESS → HI_RES → LOSSLESS →
    HIGH` truncated at the user's ceiling; skip Hi-Res tiers when no client secret is configured;
    stop the cascade immediately on network errors, rate limits and terminal sub-statuses.
    Pre-flight with `media_metadata_tags` containing `HIRES_LOSSLESS` (mopidy-tidal's check) to
    avoid a wasted request.
13. **Implement the `subStatus` taxonomy** from `ref:sone/src-tauri/src/tidal_api.rs:11-43`
    verbatim. A 401 carrying a 4xxx sub-status must not trigger a token refresh; `4005, 4010,
    4030-4032, 4034, 4035` mean skip the track; `4006` and `4033` must not.
14. **Handle both manifest types and both DASH delivery styles.** Prefer the
    `data:application/dash+xml;base64,…` URI (works everywhere); keep the write-to-file `file://`
    fallback, which High Tide adopted for GStreamer ≥ 1.26 and mopidy-tidal always uses.
15. **Normalization**: implement Sone's `0.8 * min(10^((rg+4)/20), 1/peak)` with album-vs-track
    context selection and mutual fallback, applied as a dedicated gain stage that is bypassed
    entirely in bit-perfect mode.
16. **Bit-perfect**: port Sone's ALSA negotiation wholesale (format probing, the promotion table,
    the S24 naming inversion, `set_rate_resample(false)` plus a post-hoc rate check,
    `sw_params` start-threshold pre-fill, channel-minimum fallback). Add tidalt's two refinements:
    PipeWire device reservation over `org.freedesktop.ReserveDevice1.Audio<N>` before opening
    `hw:`, and distinguishing a format refusal (fall back to `plughw:`, report "not bit-perfect")
    from a device-busy error (retry `hw:`).
17. **Gapless**: use `concat` at the pipeline head with a per-branch queue and a *serialized*
    attach/detach executor (Sone), not `playbin3`'s `about-to-finish` (High Tide) if you want
    signal-path control. Accept that gapless and exclusive-ALSA are mutually exclusive in Sone's
    design, and decide whether streamboat will do better (a single long-lived ALSA writer fed by
    a `concat`-headed appsink chain is the obvious unexplored option).
18. **Cap playbackinfo concurrency at 2** (TidaLuna's `Semaphore(2)` matches the official
    client's behaviour) and add client-side rate limiting (Sone's `rate_gate.rs`).
19. **Never cache manifests or stream URLs across restarts** — they expire in minutes to an hour.
    Cache *metadata* aggressively instead, on Sone's tier/TTL/SWR table.

### Storage, settings, UX

20. **Secrets**: OS keyring as primary, encrypted file as fallback, always write the file backup
    too (Sone's AppImage/D-Bus lesson). AES-256-GCM with a magic-header container so the format
    can version and so plaintext migrates transparently. Zeroize key material.
21. **Settings**: one versioned, serialized struct with per-field defaults, encrypted on disk,
    with explicit migration flags for behaviour changes (Sone's `titlebar_migration_v1` and
    `legacy_auth_notice_count` are good patterns). Platform-standard locations
    (`~/.config/streamboat`, `%APPDATA%`, `~/Library/Application Support`).
22. **Headless login** must work without a browser on the same machine: device code with a
    terminal QR (tidalt's `qrterminal`), plus a small local HTTP page for the PKCE redirect paste
    (mopidy-tidal), plus mopidy-tidal's login-hack idea adapted to whatever streamboat's clients
    are — a "not logged in" pseudo-item that any client renders.
23. **Ship an integration surface early**: a local HTTP/JSON API is cheap and is the thing the
    community actually built on in tidal-hifi (i3blocks, scrobblers) and Sone (OBS overlay, MCP).
    Default it off, bind to loopback, and gate it with a generated token (Sone's URL-path UUID).
24. **Signal-path transparency is a differentiator** and it is cheap to do once you already have
    the pipeline probes: show the user the decoded format, every conversion, and what the device
    finally received (`ref:sone/src-tauri/src/signal_path.rs`, `pipeline_probe.rs`).

### Packaging

25. Target the same distribution set as Sone: Flathub (manifest lives in the Flathub repo; keep a
    tag-triggered PR workflow in-tree), AUR, `.deb`, `.rpm`, Snap, Nix flake, AppImage with
    `bundleMediaFramework`, and a Windows installer with bundled GStreamer DLLs. Reuse
    `ref:sone/build-scripts/build/*.sh` + Dockerfiles and the PKGBUILD-repacks-the-deb trick, and
    `ref:sone-windows/scripts/prepare-gstreamer.js` for the NSIS/WiX generation. Keep one version
    source (`ref:sone/sync-version.mjs`).
26. **Flatpak sandbox reality check — now confirmed against Sone's actual shipped manifest, not
    just High Tide's** (§18-A): both High Tide's and **Sone's own** Flathub manifest grant
    PulseAudio and read-only PipeWire but explicitly no `--device=all` and no raw ALSA filesystem
    access — Sone's flagship exclusive-ALSA feature cannot run in the build Flathub actually
    ships. Treat Flathub as a convenience tier (shared audio server only) and deb/rpm/AUR/Nix/Snap
    as the audiophile tier from the start; Sone's Snap documents an explicit
    `snap connect sone:alsa` step for exactly this reason. Plan the confinement story alongside
    the bit-perfect feature, not after it.

### Licensing

27. If streamboat vendors or ports code from Sone, High Tide, Strawberry or mopidy-tidal's
    GPL/Apache sources, the obligations differ: **GPL-3.0** (Sone, High Tide, Strawberry) is
    copyleft over the whole work; **Apache-2.0** (mopidy-tidal, official SDKs, tidalt) and **MIT**
    (tidalrs, tidal-hifi, tidal-cli) are permissive; **LGPL-3.0** (python-tidal) is fine to use
    as a separate library, restrictive to port; **MS-PL** (TidaLuna) is permissive but the code
    is unusable for other reasons; **TidalSwift has no licence at all** — all rights reserved.
    Reading GPL code and reimplementing its ideas is fine; copying it into a non-GPL streamboat
    is not. If streamboat is GPL-3.0 itself this mostly evaporates — which is a reason to
    consider GPL-3.0.

### Additional implications (from the fact-check pass, §18)

28. **Invert Sone's own state-ownership shape.** Sone's queue, shuffle, repeat and history live in
    the React webview, not in Rust (§18-B parent finding; `ref:sone/src/atoms/playback.ts:54-109`);
    the backend only persists a snapshot and mirrors frontend state out to MCP/overlay/miniplayer
    consumers. That is why sone-windows is a fork, not a headless-capable port. streamboat's core
    must own session/queue/transport; every UI (desktop, CLI, MPRIS, HTTP) is a thin subscriber —
    do the opposite of what Sone actually does, not what its module names suggest it does.
29. **Decide push vs poll for playback position explicitly.** Sone has no position-tick event at
    all; every consumer (main UI, miniplayer, overlay, signal-path panel) polls on its own
    `setInterval`. Pick one behaviour for streamboat's core↔UI protocol — a single push channel at
    a fixed tick rate is simpler to reason about and cheaper than N independent pollers.
30. **Decide the update mechanism per distribution channel, not once.** Sone ships no
    auto-updater and no build/test/lint CI (§18 note under §2.7/§2.8) — its release process is
    entirely manual. A signed in-app updater needs a keypair, a hosted manifest and CI to produce
    signed builds, and must never be offered to Flatpak/Snap/AUR users whose package manager owns
    updates for them; check-and-notify-only is the right default there, an in-app updater only
    makes sense for a self-contained bundle (AppImage, Windows/macOS installer).
31. **Make an explicit i18n decision before the component count grows.** High Tide gets 8
    community locales for free via gettext/Meson; Sone has none across 199 TS/TSX files. Pick a
    mechanism (or consciously ship English-only and say so) while the UI is still small.
32. **Price the GStreamer-bundling licence surface before choosing what to bundle.** Bundling
    `gstreamer1.0-libav` (FFmpeg-derived) into a Windows installer or AppImage puts LGPL
    relinking/notice obligations on streamboat directly, unlike a dynamic distro dependency.
    Decide whether the bundle needs `libav` at all, or whether `base`/`good`/`bad` (FLAC + DASH
    demux + AAC decode, no FFmpeg) is sufficient for TIDAL's actual codec set.
33. **Define an explicit Dolby Atmos policy rather than letting it fall out of the quality
    cascade by accident.** No reference project verifiably handles an Atmos-tagged track
    correctly end to end. Default to requesting/preferring stereo; if an Atmos-only manifest
    arrives, fail with a specific, honest message instead of GStreamer's generic
    "Internal data stream error."

---

## Reusable artifacts (with paths)

**Packaging / build**
- `ref:sone/build-scripts/build/{deb,rpm,pacman,all}.sh` + `Dockerfile.{deb,rpm,rpm-opensuse,pacman}` — four-format Linux packaging from one Tauri build.
- `ref:sone/build-scripts/build/PKGBUILD` — Arch package that repacks the built `.deb`.
- `ref:sone/build-scripts/test/{all,common,deb,pacman,rpm}.sh` — per-format install smoke tests.
- `ref:sone/src-tauri/tauri.conf.json` — complete `.deb`/`.rpm` dependency lists for a GStreamer + WebKitGTK + libsecret + ALSA app.
- `ref:sone/flake.nix`, `ref:sone/nix/package.nix` — Nix package + devShell with `GST_PLUGIN_SYSTEM_PATH_1_0` wiring.
- `ref:sone/snap/snapcraft.yaml` — `core24` strict snap with the ALSA interface story.
- `ref:sone/.github/workflows/flathub-update.yml` — tag-triggered Flathub PR automation.
- `ref:sone/sync-version.mjs` — one version source across Cargo/Tauri/PKGBUILD/AppStream.
- `ref:sone-windows/scripts/prepare-gstreamer.js` — generates NSIS hooks and a WiX fragment from a GStreamer runtime directory.
- `ref:high-tide/build-aux/io.github.nokse22.high-tide.json` + `python3-tidalapi.json` — a working Flatpak manifest (GNOME 50) with vendored Python wheels.
- `ref:tidalt/docker-bake.hcl` and `ref:tidalt/packaging/` — multi-arch deb/rpm/pkg.tar.zst via Docker Bake.
- `ref:tidal-hifi/build/electron-builder.*.yml` — deb/rpm/snap/pacman/win/mac configs (as a checklist of targets and desktop-entry fields).

**API client code**
- `ref:tidalrs/src/{lib,track,album,artist,playlist,search}.rs` — MIT-licensed, async Rust, the best starting point for a Rust streamboat.
- `ref:python-tidal/tidalapi/{session,media,request}.py` — the authoritative endpoint/manifest reference (read, do not port; LGPL).
- `ref:sone/src-tauri/src/tidal_api.rs` — the richest single implementation (GPL; read for the sub-status table, header set, cascade).
- `ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts` — the official request shape and quality→formats mapping (Apache-2.0).
- `ref:tidal-sdk-android/player/streaming-api/.../ManifestMimeType.kt` — canonical manifest MIME enum (Apache-2.0).

**Audio**
- `ref:sone/src-tauri/src/audio.rs:190-215,483-568,569-751` — ALSA format/rate negotiation and the S24 naming inversion.
- `ref:sone/src-tauri/src/audio.rs:43-88,255-482` — `concat` gapless with a serialized executor.
- `ref:sone/src-tauri/src/audio.rs:2844-3000` — appsink pipeline construction incl. DASH caps handling.
- `ref:sone/src-tauri/tests/gapless_probe.py` — an integration probe for gapless behaviour.
- `ref:high-tide/src/lib/player_object.py:184-247` — swappable sink bin + sink change with position restore.
- `ref:tidalt/internal/player/alsa.c` (111 lines) + `ref:tidalt/internal/player/avcodec.c` (152 lines) — minimal C ALSA open/configure/write with `plughw:` fallback, plus the cgo FFmpeg decode glue (263 lines combined — a prior pass misattributed the full total to `alsa.c` alone).
- `ref:sone/src-tauri/src/signal_path.rs`, `pipeline_probe.rs` — signal-path transparency.

**Security / storage**
- `ref:sone/src-tauri/src/crypto.rs` — AES-256-GCM container with magic header, keyring + file key, transparent migration.
- `ref:sone/src-tauri/src/cache.rs` — 4-tier encrypted disk cache with SWR, tags and LRU.
- `ref:high-tide/src/lib/secret_storage.py` — libsecret store + the non-Flatpak keyring unlock workaround.

**UI / UX patterns**
- `ref:sone/src/api/tidal.ts` — invoke wrapper with a 150 MB in-memory LRU, TTL tiers and tag invalidation.
- `ref:sone/src/atoms/*`, `ref:sone/src/hooks/*` — jotai atom + hook decomposition for a player UI.
- `ref:sone/src/components/MiniPlayer.tsx` + `ref:sone/miniplayer.html` + `useMiniplayerWindow` — second-window miniplayer.
- `ref:sone/src-tauri/src/theme_config.rs` + `src/lib/theme.ts` — two-seed-colour theming with named presets in an external file.
- `ref:high-tide/data/ui/**/*.blp` — Blueprint markup for a GNOME-native player.
- `ref:high-tide/src/disconnectable_iface.py` — signal-cleanup mixin preventing leaks in long-lived widgets.
- `ref:tidal-hifi/src/TidalControllers/` + `ref:tidal-hifi/docs/tidal-controllers.md` — strategy-with-fallbacks pattern.
- `ref:mopidy-tidal/mopidy_tidal/login_hack.py` — headless login prompt rendered through the normal library UI.
- `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/{proxy,cache}.py` — Range-capable caching HTTP proxy over a SQLite chunk store.
- `ref:tidalt/docs/{architecture,client-server}.md` — daemon/client D-Bus split documentation.

**External references (not in the `ref/` checkouts — cited by URL only, from the fact-check
pass)**
- https://github.com/librespot-org/librespot — `playback/src/audio_backend/mod.rs` (the `Sink`
  trait + `BACKENDS` table), `playback/Cargo.toml` (nine backend features, Symphonia decode) — the
  core+`Sink`-trait+many-clients precedent for a pure-Rust streamboat (§18-B).
- https://github.com/HEnquist/camilladsp/blob/master/backend_coreaudio.md — CoreAudio hog-mode
  exclusive output in Rust, the macOS precedent this landscape otherwise lacks (§18-D).
- https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/src/output/plugins/
  OSXOutputPlugin.cxx — the C++ reference for `kAudioDevicePropertyHogMode` +
  `kAudioStreamPropertyPhysicalFormat` device-format switching (§18-D).
- https://raw.githubusercontent.com/flathub/io.github.lullabyX.sone/master/
  io.github.lullabyX.sone.yml — Sone's actual shipped Flathub manifest: no raw ALSA access (§18-A).

**Integration surfaces**
- `ref:sone/src-tauri/src/mcp/` — MCP server over `rmcp` + `axum`, token-gated, off by default.
- `ref:sone/src-tauri/src/overlay/server.rs` — self-contained OBS overlay page.
- `ref:sone/src-tauri/src/mpris.rs` — MPRIS behind a command channel.
- `ref:sone/src-tauri/src/scrobble/` — Last.fm / Libre.fm / ListenBrainz / MusicBrainz with an offline queue.
- `ref:tidal-hifi/src/features/api/` — Express + swagger-jsdoc local API.
- `ref:tidal-connect/userconfig/` (26 presets) + `ref:tidal-connect/samples/` (18 more) + `ref:tidal-connect/assets/known-devices.md` — 44 per-DAC `asound.conf` presets total and a tested-hardware table.

---

## Open questions

**Only the owner can decide these:**

1. **Language and runtime for the core.** Rust (matches Sone/tidalrs, best for a shared
   daemon + Tauri desktop + CLI) vs Python (matches High Tide/mopidy-tidal, fastest to a working
   client via python-tidal, worst for bit-perfect audio and packaging) vs C++/Qt (matches
   Strawberry, best cross-platform audio maturity, slowest to write). "Simple but beautiful"
   pushes toward Rust core + a web-tech UI, which is Sone's answer.
2. **GStreamer vs a pure-Rust audio stack.** GStreamer is the only precedent that works *for
   TIDAL specifically*; pure Rust (Symphonia + cpal, or Symphonia + `alsa`/`wasapi`/`coreaudio-rs`
   behind a librespot-shaped `Sink` trait — the architecture pattern does have a precedent,
   librespot, §18-B) would be lighter and easier to package but still needs a hand-written
   DASH/BTS layer, and `cpal` specifically cannot do WASAPI exclusive mode at all (§18-C) — a
   pure-Rust choice still needs a hand-written `wasapi`-crate Windows backend, same as GStreamer
   needs a hand-written macOS backend either way (next item).
3. **Project licence.** GPL-3.0 aligns with Sone/High Tide/Strawberry and removes friction if
   ideas or code flow from them; MIT/Apache-2.0 maximizes reuse (and matches tidalrs, which is
   the most likely Rust dependency).
4. **Whether to ship default TIDAL client credentials at all**, and if so whether to state
   plainly that they were extracted from an official client rather than obfuscating them.
5. **Whether offline caching exists at all**, and if so whether it is ephemeral-only,
   or persistent-encrypted-expiring, or (as High Tide does) plain files on disk.
6. **Whether to report plays back to TIDAL** (Sone does by default, disableable), which is
   arguably good citizenship and arguably telemetry.
7. **Whether the headless mode's control protocol is bespoke, MPD-compatible, or MPRIS-first.**
   MPD compatibility buys an entire ecosystem of clients for free; bespoke buys clean semantics.
8. **Windows/macOS exclusive-mode ambition.** WASAPI exclusive is proven twice over in this set
   (sone-windows' `wasapi2sink`, Strawberry's WASAPI-exclusive setting). CoreAudio hog mode has
   **no precedent inside this reference set**, but it is not actually unresearchable: CamillaDSP
   (Rust) and MPD's `OSXOutputPlugin.cxx` (C++) both implement it and are documented in §18-D —
   this is now a scoping/effort decision for the owner (build it or not), not an open research
   question.

**Unverified / needs direct checking before it is relied on:**

9. **The exact current text of TIDAL's Developer Guidelines, Developer Terms, and the consumer
   Content Guidelines — still fully unverified.** The fact-check pass confirmed the block is
   *domain-wide*: not just `developer.tidal.com`/`support.tidal.com` but `tidal.com` itself
   (`https://tidal.com/content-guidelines` also returns `EGRESS_BLOCKED`). Every quote in this
   report is second-hand via a GitHub discussion or a search summary. Someone must read, from an
   unproxied network: `https://developer.tidal.com/documentation/guidelines/
   guidelines-developer-guidelines`, `.../guidelines-developer-terms-2_0`, and
   `https://tidal.com/content-guidelines` directly, and record retrieval dates.
10. Whether TIDAL's consumer Terms of Use contain a clause specifically about third-party clients
    (as opposed to the general reverse-engineering prohibition in the Content Guidelines) — still
    unverified for the same reason as #9.
11. **The 2026-03-21 unofficial-client-ID breakage report is thinner evidence than the original
    pass implied** — corrected by the fact-check pass. Tidal-Media-Downloader#1213 is confirmed to
    exist and to be dated correctly, but it is **one reply-less GitHub issue about one public gist
    of keys**, not a "widely-reported" breakage. Whether it was permanent, which client IDs it
    affected beyond that gist, and how each project recovered remains genuinely unknown — keep
    the underlying caution (assume client IDs churn) but do not cite this issue as evidence of a
    broad or confirmed-permanent breakage.
12. Whether `HI_RES` (MQA) still returns anything from the API at all, given MQA content was
    removed in July 2024 — Strawberry and Sone still offer the tier, python-tidal removed it,
    and tidal-connect's own README independently corroborates the July 2024 MQA removal date
    from the receiver side. Still not directly tested against a live `playbackinfopostpaywall`
    call with `audioquality=HI_RES`.
13. Whether any client has successfully obtained the `playback` scope with full-track entitlement
    from the official developer portal. No evidence found either way.
14. ~~Sone's actual Flathub manifest~~ **— answered by the fact-check pass, see §18-A.** It grants
    PulseAudio + read-only PipeWire, explicitly no `--device=all` and no raw ALSA access.
15. Whether `concat`-based gapless can be combined with an exclusive ALSA writer (Sone gates them
    apart; nobody in the set has tried the combination) — still open, and still the single most
    novel engineering claim streamboat would be making if it attempts this. Prototype before
    committing to it in a design doc.
16. **macOS, corrected — the framing was wrong on both halves.** Strawberry *does* ship a macOS
    TIDAL client that streams `LOSSLESS`/`HI_RES_LOSSLESS` (confirmed, §5) — the "no lossless
    macOS TIDAL client exists" half of the original claim is false. What is genuinely true and
    still unresolved: no project in the set ships **bit-perfect/exclusive** macOS output for
    TIDAL, and Strawberry's own macOS *binaries* are sponsor-only, so its lossless macOS support
    is not casually redistributable either. Two external, non-TIDAL precedents for the exclusive
    half now exist (CamillaDSP, MPD's `OSXOutputPlugin.cxx`, §18-D) — this is a scoping decision,
    not a research gap, going forward. Separately: no project in the set ships **both a GUI
    desktop client and a headless daemon from one codebase** (the framing the original pass
    intended) — tidalt comes closest (a daemon plus a TUI client, no GUI, Linux-only).
17. TIDAL Connect: no open-source receiver or controller implementation exists. Whether the
    protocol is even approachable is unknown.
18. **Current contributor counts per project — still not obtainable from this environment**,
    confirmed by the fact-check pass: the checkouts are shallow (`--depth 1`) clones, so
    `git log` gives exactly one author per repo regardless of the real number, and this session's
    proxy refuses unauthenticated GitHub contributor-endpoint calls. Method for whoever runs this
    next: `GET /repos/{owner}/{repo}/contributors?per_page=100&anon=1` after attaching each repo
    via the GitHub connector, or each repo's Insights → Contributors page. Until then, use the
    qualitative proxy in §18-J (Sone: 1 author, no CI, 64 open issues; High Tide: community +
    Matrix channel + 8 translator locales; Strawberry: 341 forks, daily commits).

---

## Sources

**Reference checkouts** (read directly; paths are `ref:<project>/<path>`)

- `ref:sone/src-tauri/Cargo.toml`, `package.json` — Sone's exact dependency set and versions (Tauri =2.11.2, gstreamer 0.23, alsa 0.10, keyring 3, aes-gcm 0.10, rmcp 1.7.0, React 19, jotai).
- `ref:sone/src-tauri/src/audio.rs` — two playback backends, `concat` gapless with a serialized executor, ALSA format/rate probing, promotion table, hw/sw params, DASH caps handling, volume curve.
- `ref:sone/src-tauri/src/tidal_api.rs` — API hosts, `TIDAL_CLIENT_VERSION 2025.11.3`, sub-status taxonomy, device-code/PKCE/refresh flows, `playbackinfopostpaywall` request and BTS/DASH/JSON manifest parsing, proxy builder.
- `ref:sone/src-tauri/src/commands/playback.rs` — quality cascade + stopping rules, `compute_norm_gain`, DASH data-URI construction, album-vs-track gain selection.
- `ref:sone/src-tauri/src/crypto.rs` — AES-256-GCM container, keyring + 0600 file key, transparent migration.
- `ref:sone/src-tauri/src/cache.rs` — 4-tier TTL/SWR cache table.
- `ref:sone/src-tauri/src/lib.rs` — `Settings` struct, defaults (mcp 5577, overlay 5578, max_quality HI_RES_LOSSLESS), config dir, 177-command `generate_handler!` (lib.rs:866-1070).
- `ref:sone/src-tauri/src/embedded_config.rs` — XOR-masked embedded credentials.
- `ref:sone/src-tauri/src/theme_config.rs` — `theme.json` schema and the 15 preset names.
- `ref:sone/src-tauri/src/mpris.rs`, `mcp/server.rs`, `overlay/server.rs` — integration surfaces.
- `ref:sone/src/api/tidal.ts` — frontend 150 MB LRU with TTL tiers.
- `ref:sone/src-tauri/tauri.conf.json`, `flake.nix`, `snap/snapcraft.yaml`, `build-scripts/**`, `.github/workflows/flathub-update.yml`, `sync-version.mjs` — packaging.
- `ref:sone-windows/src-tauri/Cargo.toml`, `src/audio.rs`, `README.md`, `scripts/prepare-gstreamer.js` — the Windows fork: `wasapi2sink`, souvlaki, v0.16.0, DLL bundling, maintenance disclaimer.
- `ref:high-tide/src/login.py` — PKCE-only login dialog.
- `ref:high-tide/src/lib/secret_storage.py` — libsecret schema and keyring unlock workaround.
- `ref:high-tide/src/lib/player_object.py` — `playbin3` + `about-to-finish`, sink bin string, ReplayGain chain, MPD/BTS resolution, disk caching via ffmpeg/requests, bus error handling.
- `ref:high-tide/data/io.github.nokse22.high-tide.gschema.xml` — 16 settings keys.
- `ref:high-tide/build-aux/*.json` — Flatpak manifest (GNOME 50) and vendored wheels.
- `ref:high-tide/CONTRIBUTING.md`, `po/LINGUAS` — coding rules; 8 translations.
- `ref:strawberry/src/tidal/tidalservice.cpp` — OAuth PKCE config, `api.tidalhifi.com/v1`.
- `ref:strawberry/src/tidal/tidalstreamurlrequest.cpp` — four stream methods and the three encryption refusals.
- `ref:strawberry/src/constants/tidalsettings.h`, `src/settings/tidalsettingspage.cpp` — settings keys, defaults, quality list.
- `ref:tidal-hifi/package.json`, `build/electron-builder.base.yml` — castlabs Electron `v43.0.0+wvcus`, v8.1.3, MIT.
- `ref:tidal-hifi/src/preload.ts`, `docs/tidal-controllers.md`, `docs/audio-quality.md` — four controllers; Chromium 48 kHz resampling.
- `ref:tidal-hifi/src/scripts/settingsStore.ts` — API port 47836.
- `ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts` — `desktop.tidal.com/v1`, auth headers, `Semaphore(2)`, pages API.
- `ref:TidaLuna/plugins/lib/src/helpers/getCredentials.ts` — module-tree credential extraction.
- `ref:TidaLuna/plugins/lib.native/src/request/decrypt.ts` — the hardcoded `OLD_AES` master key (cited as the boundary streamboat must not cross).
- `ref:mopidy-tidal/mopidy_tidal/ext.conf`, `playback.py`, `gstreamer_proxy/__init__.py`, `pyproject.toml` — config schema, MPD/BTS handling, `lgf.audio.tidal.com` proxy, Apache-2.0, v0.3.13.
- `ref:tidal-connect/bin/entrypoint.sh`, `README.md` — proprietary binary invocation flags; post-MQA quality ceiling; alternatives list.
- `ref:tidal-sdk-web/packages/*/package.json` (all Apache-2.0), `packages/player/src/internal/helpers/playback-info-resolver.ts`, `packages/player/src/player/shakaPlayer.ts`, `packages/player/src/config.ts` — `/trackManifests/{id}` query shape, quality→formats map, DRM URLs, `openapi.tidal.com/v2`.
- `ref:tidal-sdk-android/player/streaming-api/.../ManifestMimeType.kt` — EMU/BTS/DASH/HLS.
- `ref:tidal-sdk-ios/Package.swift`, `LICENSE`, `Sources/` — Apache-2.0, GRDB/SWXMLHash/KeychainAccess/Kronos, Offliner module.
- `ref:python-tidal/pyproject.toml` — v0.8.11, LGPL-3.0-or-later, EbbLabs repo, dependency list.
- `ref:python-tidal/tidalapi/session.py` — Config endpoints, obfuscated credentials, PKCE URL params, device authorization.
- `ref:python-tidal/tidalapi/media.py` — Quality enum without HI_RES, `playbackinfopostpaywall`/`urlpostpaywall` params, `StreamManifest` BTS/MPD parsing, PKCE-blocks-`get_url`.
- `ref:tidal-cli/src/auth.ts` — official SDK, public client `PYVtmSHMTGI9oBUs`, scopes incl. `playback`, loopback :17893.
- `ref:tidal-cli/src/playback.ts` — `/trackManifests/{id}` params, DASH segment reconstruction, `trackPresentation`/`previewReason`.
- `ref:tidalt/internal/tidal/client.go`, `api.go` — hardcoded client ID, device flow, `urlpostpaywall` on `api.tidal.com/v1`.
- `ref:tidalt/go.mod`, `README.md`, `LICENSE` — Bubble Tea/D-Bus/bbolt/age stack, Apache-2.0, "vibe coded" note.
- `ref:tidalrs/Cargo.toml`, `src/track.rs` — MIT, v0.5.0, three stream endpoints, stream-download.
- `ref:tidalswift/TidalSwiftLib/Sources/TidalSwiftLib/Config.swift`, `Session/ContentUrls.swift`, repo root listing — plaintext id+secret, `streamUrl` endpoint, no LICENSE file.
- `ref:libopentidal/Source/OTService/OTServiceStd.c` — `playbackinfopostpaywall` and `playbackinfoprepaywall`.
- `ref:tidalgo/tidal.go` — `api.tidalhifi.com/v1/`, username/password session auth, the WiMP-key-vs-TIDAL-key encryption note.
- `ref:dotnet-tidal-usdk/TidalUSDK/**` — legacy `streamUrl`, Android token constants.
- `ref:tidal-api-docs/**` — PKCE flow with `CzET4vdadNUFQ5JU`, 24 h token TTL, CORS warning, no streaming docs.
- `ref:tidal-fokka-engineering-/README.md` — decompilation provenance and the anti-piracy disclaimer.

**Web sources**

- https://github.com/lullabyX/sone — stars 437, forks 24, open issues 64, pushed 2026-09-05, GPL-3.0; README disclaimer, feature list, install channels, MCP/overlay ports.
- https://github.com/Nokse22/high-tide — stars 671, forks 66, open issues 90, pushed 2026-08-21, GPL-3.0.
- https://github.com/Mastermindzh/tidal-hifi — stars 1725, forks 100, open issues 22, pushed 2026-08-31.
- https://github.com/Inrixia/TidaLuna — stars 591, forks 56, open issues 15, pushed 2026-09-01, MS-PL.
- https://github.com/EbbLabs/python-tidal — stars 560, forks 124, open issues 21, pushed 2026-08-14, LGPL-3.0.
- https://github.com/EbbLabs/mopidy-tidal — stars 123, forks 35, open issues 39, pushed 2026-06-12, Apache-2.0.
- https://github.com/GioF71/tidal-connect — stars 167, forks 13, open issues 8, pushed 2026-03-31, MIT.
- https://github.com/strawberrymusicplayer/strawberry — stars 3947, forks 340, open issues 21, updated 2026-09-07.
- https://github.com/tidal-music/tidal-sdk-web — stars 204, open issues 24, updated 2026-09-07.
- https://github.com/tidal-music/tidal-sdk-android — stars 49, open issues 5, updated 2026-09-07.
- https://github.com/tidal-music/tidal-sdk-ios — stars 41, open issues 13, updated 2026-08-24.
- https://github.com/melgu/TidalSwift — stars 97, open issues 7, updated 2026-07-08.
- https://github.com/lucaperret/tidal-cli — stars 10, open issues 4, updated 2026-08-23, MIT.
- https://github.com/Benehiko/tidalt — stars 2, open issues 0, updated 2026-09-06, Apache-2.0.
- https://github.com/phayes/tidalrs — stars 19, open issues 2, updated 2026-09-02, MIT.
- https://github.com/tcpj/tidalgo — 1 star, last updated 2019-05-09 (dead).
- https://github.com/SacredSkull/dotnet-tidal-usdk — 0 stars, last updated 2020-05-07 (dead).
- https://github.com/gkasdorf/Tidal-API-Docs — 0 stars, repo updated 2026-07-13, content from 2023.
- https://github.com/lvllaby/sone-windows — exists per web search; not returned by the GitHub search API in this session.
- https://developer.tidal.com/documentation/guidelines/guidelines-developer-guidelines — source of *"Playbacks shall only be made available through TIDAL's SDKs, namely an official, unmodified version of the TIDAL Player module"* and *"third-party applications can include playback of TIDAL previews"*. **Not directly fetchable from this environment (egress blocked); quoted via search-result summary.**
- https://developer.tidal.com/documentation/guidelines-developer-terms-2_0 — Developer Terms (reverse-engineering prohibition). Same caveat.
- https://tidal.com/content-guidelines — consumer Content Guidelines prohibiting reverse-engineering/decompiling/modifying TIDAL Services. Same caveat.
- https://github.com/orgs/tidal-music/discussions/179 — third-party app review still non-functional as of April 2026; preview-only playback confirmed by participants.
- https://github.com/tidal-music/tidal-sdk-web/issues/133 — 30-second low-resolution previews under client credentials; `Required scopes: r_usr playback` under PKCE.
- https://github.com/api-evangelist/tidal — independent profile listing the ten public TIDAL APIs, the scope names, and the audio-bytes/Player-SDK policy statement.
- https://github.com/yaronzz/Tidal-Media-Downloader/issues/1213 — unofficial client-ID breakage reported 2026-03-21, no resolution in-thread.
- https://tidal-music.github.io/tidal-api-reference/ — official OpenAPI reference *(not fetched; listed for follow-up)*.
- https://tidalapi.netlify.app/ — python-tidal documentation *(not fetched; listed for follow-up)*.
- https://www.music-assistant.io/music-providers/tidal/ — Music Assistant's TIDAL provider *(not fetched; a further headless precedent worth reviewing)*.

**Fact-check-pass additions (2026-09-07, external — not `ref:` checkouts, see §18)**

- https://github.com/librespot-org/librespot — MIT, ~7.1k★; `Sink` trait + `BACKENDS` table +
  Symphonia decode; the core+trait+many-clients precedent for pure-Rust audio (§18-B).
- https://raw.githubusercontent.com/librespot-org/librespot/dev/playback/Cargo.toml and
  `.../playback/src/audio_backend/mod.rs` — the nine backend features and the trait definition.
- https://github.com/RustAudio/cpal/issues/459 — confirms `cpal` has no WASAPI exclusive mode.
- https://docs.rs/wasapi and https://github.com/HEnquist/wasapi-rs — the standalone crate that
  does support WASAPI exclusive (used by CamillaDSP).
- https://github.com/HEnquist/camilladsp/blob/master/backend_coreaudio.md and
  https://www.camilladsp.com/docs/camilladsp/4.0.x/backend_wasapi/ — CoreAudio hog-mode exclusive
  output in Rust (§18-D).
- https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/src/output/plugins/
  OSXOutputPlugin.cxx — the C++ CoreAudio hog-mode reference (§18-D).
- https://raw.githubusercontent.com/flathub/io.github.lullabyX.sone/master/
  io.github.lullabyX.sone.yml — Sone's actual shipped Flathub manifest, fetched 2026-09-07: GNOME
  50 runtime, no `--device=all`, no raw ALSA (§18-A, answers Open question 14).
