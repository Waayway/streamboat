# streamboat decisions

Running, dated log of owner decisions. Each entry records what was decided, the alternatives
considered, why, and what follows from it. Research grounding lives in `docs/research/` and the
skills under `.claude/skills/`; question ids such as `q1.1` refer to
`docs/research/decision-tree.json`.

Status legend:

- **decided** — owner-ratified; do not re-litigate.
- **default** — research-derived; owner reviewed the list and accepted it.
- **reopened** — research default the owner wants revisited; pending until a later round.

## 2026-09-08 — Round 1: product scope and platform commitments

### D-001 First release scope: a full client, not a core-player MVP (q1.1) — decided

The first usable release renders Home/Explore, radio and mixes, and lyrics, on top of the core
player set: login with device-code and PKCE (token refresh and secure storage), search, My
Collection and favourites, transport controls, a queue with reorder and play-next versus add-last,
the quality cascade over DASH and BTS with loud refusal of encrypted manifests, gapless playback,
and MPRIS/SMTC/NowPlaying with media keys, delivered through both the desktop shell and the
headless daemon.

Alternatives: core player only (the research recommendation); core player plus bit-perfect output.

Why: the owner wants the first release to feel like a real TIDAL client rather than a spike.

Consequences: a longer first release; the pages-API renderer (q4.4) and lyrics are v1 work;
exclusive/bit-perfect scope is still decided separately (q5.3); milestone sequencing (q13.1)
happens inside this scope, not instead of it.

### D-002 Desktop platforms: Linux, Windows and macOS at parity from the first binary release (q1.2) — decided

CI builds and signs all three; a release-blocking bug on any of them blocks the release.

Alternatives: Linux first with best-effort others (Sone's path, which produced a stale Windows
fork); Linux and Windows first, macOS later.

Consequences: the code-signing budget (q12.3) starts at v1 for both Windows and macOS; release
cadence is bounded by the slowest platform; macOS exclusive output must at least be costed for v1
(q5.3); GStreamer packaging on Windows and macOS is v1 work if GStreamer is chosen (q5.2).

### D-003 Quality tiers: all four at launch — LOW, HIGH, LOSSLESS, HI_RES_LOSSLESS (q1.4) — decided

Alternatives: lossless now and lossy later (the research recommendation); lossless only,
permanently; a 16-bit/44.1 kHz ceiling for v1.

Consequences: AAC decoding ships on all three OSes from v1, through LGPL FFmpeg's `avdec_aac` or
`faad`, never `fdk-aac`; a startup decoder probe that greys out unreachable tiers is v1 work;
HI_RES_LOSSLESS requires the PKCE flow and a client id with a secret (q7.1, q7.2), so
"device code only" is off the table; the AAC patent question is part of the v1 packaging story.

### Research defaults accepted in round 1 — default

- No local Dolby Atmos decoding; at most a passthrough flag kept in the manifest-request layer.
- Encrypted manifests (`encryptionType != "NONE"`, non-empty `encryptionKey`, or
  `securityType`/`securityToken`) are refused loudly with an explanation; hi-res tiers are filtered
  out of the cascade pre-emptively when no client secret is configured.
- `streamboat://` is the URI scheme for both the OAuth redirect and content deep links; never
  claim `tidal://`.
- Every listener binds loopback by default; LAN exposure is a deliberate, logged, token-protected
  opt-in with Host-header allowlisting.
- No telemetry to the project's developers.
- 360 Reality Audio and MQA are not targets (removed by TIDAL in July 2024).
- `adaptive=false` on every manifest request.
- One TIDAL account is one concurrent stream (Pushkin); multiroom is one daemon fanning PCM out,
  never one daemon per room.
- `streamboat-core` carries no UI, windowing or desktop-OS dependency; the daemon is a separate
  artifact that must not link a webview, enforced by CI.
- The GUI reaches the engine only through the Command/Event types the control API exposes.
- Never link `fdk-aac`.
- Strings are externalised from the first commit; the user's real locale is sent to TIDAL.
- MPRIS registration lives in the engine so it works in daemon mode.
- Bit-perfect mode bypasses both the volume slider and ReplayGain, stated in the UI, and is never
  claimed for a lossy AAC source.
- TIDAL Connect *target*: never (proprietary obfuscated binary plus a vendor device certificate).
- Never redistribute a vendor's proprietary binary or device certificate.
- Play reporting, when enabled, is suppressed for PREVIEW assets.

### Reopened in round 1 — reopened

- **R-1 TIDAL Connect controller.** The research placed the device picker (sending playback to a
  Connect speaker) out of scope. The owner wants it considered. To be settled in the stream
  ownership round.
- **R-2 Crash reporting.** The research default is local crash dumps plus a debug-bundle command,
  nothing leaving the machine. The owner wants opt-in hosted crash reporting considered. To be
  settled in the account-data and privacy round.

## 2026-09-08 — Round 2: licence, identity and contribution

### D-004 Mobile: a someday target; the core stays UI-free (q1.3) — decided

No mobile work now. The only commitment is that `streamboat-core` carries no UI, windowing or
desktop-OS dependency, proven by a CI build with `--no-default-features`, so a binding layer can be
added later. iOS is treated as closed by App Store guideline 5.2.2 regardless of licence; Android
is the realistic first mobile target when the time comes.

Alternatives: plan Android for 2027 and budget for it in the crate layout now; pick a
mobile-capable stack now (Flutter over a Rust core, or UniFFI plus native apps).

### D-005 Licence: GPL-3.0-only apps over an Apache-2.0 core (q2.1) — decided

`streamboat-core` (API client, auth, manifest parsing, models) is Apache-2.0 so other clients can
reuse it; the desktop app and daemon are GPL-3.0-only so they may adapt Sone/High Tide/Strawberry
code and link GPL GStreamer plugins.

Alternatives: GPL apps over an LGPL core (python-tidal's shape); GPL-3.0-only everywhere (Sone,
High Tide, Strawberry); MIT/Apache-2.0 everywhere (tidalrs, tidalt, TIDAL's SDKs).

Consequences: relicensing later needs every contributor's consent, so the split is a one-way door;
the crate that holds the audio engine and per-OS output backends must be chosen deliberately
(q2.2), because ALSA/WASAPI negotiation and gapless concat are the parts most likely to be adapted
from GPL-3.0 Sone and must not land in the Apache-2.0 crate.

### D-006 Contribution agreement: none; provenance rests on GitHub commit metadata (q2.3) — decided

Alternatives: DCO sign-off checked in CI (the research recommendation); a CLA with relicensing
rights.

Consequences: lowest friction for contributors; relicensing later is impossible in practice, which
makes D-005 final. Conventional Commits on trunk with tags remains the assumed workflow.

### D-007 App ID and owner: `io.github.waayway.streamboat` under the existing personal account (q2.4) — decided

The string is baked into the Flatpak ID and metainfo `<id>`, the Flathub repo name, the D-Bus/MPRIS
bus name, the macOS bundle identifier, config and cache directory names and the Windows
AppUserModelID; moving later costs a Flathub end-of-life rebase plus a user-data migration.

Alternatives: a new GitHub organisation; a registered domain with a reverse-DNS ID.

Consequences: check that `streamboat` is free on crates.io, npm, PyPI, AUR, Flathub, the Snap
Store and winget before the first code commit; Flathub verification is by repo-owner
authentication.

## 2026-09-08 — Round 3: architecture shape and core language

### D-008 Core language: Rust, and no Tauri / webview shell (q3.1) — decided

One Cargo workspace produces the daemon, the CLI and the desktop shell, with hand-written
ALSA/WASAPI/CoreAudio writers where needed. The owner explicitly rules out Tauri: the desktop UI
is not a webview.

Alternatives: Go + Wails v3; Python via python-tidal; C++/Qt.

Consequences: the research's top-scoring UI shell (Tauri 2 + React) is off the table; the toolkit
choice (q4.2) is among Rust-native toolkits (Slint, iced, egui, GPUI, GTK4/libadwaita via gtk-rs
or relm4) or a non-Rust native UI over the Rust core (Flutter via flutter_rust_bridge); the
webview frontend question (q4.3) is moot; `tidalrs` (MIT) is a candidate starting point for the
API crate; Sone remains the closest precedent for the audio engine even though its UI shell is
not reused.

### D-009 Licence boundary: a GPL-3.0-only `streamboat-player` crate holds the engine (q2.2) — decided

`streamboat-core` stays narrow and Apache-2.0 (API client, auth, manifest parsing, models). The
audio engine, the per-OS output writers and the queue live in a GPL-3.0-only player crate that the
apps link, so adapting Sone's ALSA `hw_params` negotiation or High Tide's GStreamer graph is
licence-clean.

Alternatives: engine inside the Apache core under a clean-room commitment; the whole core under GPL.

### D-010 Process model: two artifacts over one core, single-instance lock, GUI falls back to remote client (q3.2) — decided

The desktop shell and the daemon are two Cargo artifacts over one library with one Command/Event
surface. On startup the GUI tries to take the single-instance lock: the MPRIS bus name where a
D-Bus session bus exists, a lock file or abstract Unix socket otherwise. If another instance holds
it, the GUI becomes a remote client speaking only the control API's Command/Event types. The lock,
not the audio device, is claimed; the device is opened when playback starts.

Alternatives: GUI always a client of a supervised daemon (MPD/Roon model); separate binaries with
no handoff (librespot/ncspot model).

### D-011 Headless platforms: a real daemon on Linux; Windows and macOS get always-on playback via tray mode (q3.3) — decided

Linux ships systemd user and system units (lingering for headless boxes) plus a Docker recipe.
Windows and macOS get background playback through the GUI's tray mode (q3.4), not a service. A
one-hour spike on WASAPI from session 0 is still worth running but does not block v1.

Alternatives: real services on all three; Linux plus a macOS LaunchAgent.

## 2026-09-08 — Round 4: UI toolkit and visual direction

### D-012 Look: one streamboat identity on all three platforms (q4.1) — decided

A single distinctive look, themeable from a small set of design tokens, rather than per-OS
conventions or a GNOME-first design. Accessibility (keyboard navigation, screen-reader exposure,
contrast) has to be built into the toolkit layer rather than inherited from a native toolkit.

Alternatives: GNOME citizen first (libadwaita, High Tide's look); native per-OS conventions.

### D-013 Toolkit: iced (q4.2) — decided

Pure-Rust, MIT-licensed, retained-mode toolkit used by System76's COSMIC desktop; 0.14 added
reactive rendering, hot reloading and headless testing. Chosen by the owner over the research's
non-webview recommendation (Slint) and over Flutter and GTK4.

Alternatives: Slint (research's non-webview pick, third on the matrix); Flutter over a Rust core
(second on the matrix, best mobile path); GTK4/libadwaita via relm4 (best Linux integration, weak
elsewhere); egui (kept as a candidate for an internal debug/signal-path window only); GPUI
(pre-1.0, thin docs).

Known costs to design around: the Elm architecture routes every interaction through one `Message`
enum, which gets verbose for a media app with many concurrent async loads, so split messages per
screen/module from the start; the API has churned hard across 0.9 to 0.14, so pin the version,
keep the iced docs for the pinned version in the repo's agent context, and review agent-written
iced code against the pinned API; custom widgets (virtualised lists for large collections, a
lyrics view, a seek bar) are the project's own work; theming from tokens maps naturally onto
iced's `Theme`/`Style` types.

### D-014 Window lifecycle: tray icon, closing the window keeps playing, quitting is explicit (q3.4) — decided

The desktop shell keeps the single-instance lock and the audio device while hidden. This is the
mechanism that gives Windows and macOS always-on playback without a background service (D-011).

Alternatives: close quits and releases the device; no tray at all with the daemon mandatory for
background playback.

### D-015 Browse screens: hybrid renderer (q4.4) — decided

Home and Explore render TIDAL's server-driven `home/feed` sections (enumerate the tab bar from the
header, page on the top-level cursor, expand sections via their `apiPath`) with a graceful fallback
for unknown section types; Collection and entity pages (album, artist, playlist, track, mix) are
hand-coded from typed endpoints. Whether to also parse the still-live v1 `pages/home` shape as a
fallback is a later implementation call; there is no automatic degradation between the two.

Alternatives: render page modules everywhere; hand-code every screen.

## 2026-09-08 — Round 5: audio engine and output

### D-016 Audio engine: GStreamer on Linux, libmpv on Windows and macOS, both behind one engine trait (q5.1) — decided

Linux uses GStreamer for demux and decode with the project's own ALSA writer for exclusive output
(Sone's shape). Windows and macOS use libmpv, which brings DASH, HLS, gapless, seek, buffering and
exclusive output (WASAPI exclusive, `coreaudio_exclusive`) in one dependency and removes the
GStreamer macOS packaging problem no reference client has solved.

Alternatives: GStreamer everywhere with libmpv as a secondary backend (the research
recommendation); libmpv everywhere; Symphonia with hand-written output (no HE-AAC, so
incompatible with D-003).

Consequences: two engine implementations and two test matrices from the first release; the engine
trait is the contract both must satisfy (load manifest, play/pause/seek, gapless preload, format
change notification, signal-path report, device enumeration and exclusive open); libmpv is
GPLv2+ unless built with `-Dgpl=false`, which is compatible with the GPL-3.0-only apps
(D-005) but rules out App Store distribution, already closed for other reasons; libmpv must be
bundled on Windows and macOS (no reference client does this, so the packaging is the project's
own work); signal-path introspection is weaker on the libmpv side and the UI must say so rather
than guess; HLS video (D-023 later) is proven in mpv and unproven in GStreamer, which favours the
split.

### D-017 Bit-perfect scope: exclusive output on all three OSes in the first release (q5.3) — decided

ALSA `hw:` on Linux, WASAPI exclusive on Windows, CoreAudio hog mode on macOS via libmpv's
`coreaudio_exclusive`. Off by default behind an advanced setting until proven on hardware per
platform. macOS remains the least verified path: mpv documents the behaviour, no source in the
research verified it on hardware, and one field report describes DAC/HDMI misrouting, so a Mac
plus a DAC is a v1 test requirement. Whether exclusive ALSA works under Flatpak is resolved with a
single test run before the toggle is greyed out under confinement.

Alternatives: Linux and Windows first (the research recommendation); shared mode only in v1.

### D-018 Format change at a track boundary: keep the device open across same-format tracks, reopen on change, accept the gap (q5.4) — decided

True gapless where it matters (albums), never resample. Budget a short silence pre-roll after each
reopen so the first note is not truncated. No reference client combines gapless with an exclusive
writer, so this is proven by a spike before it enters the engine design; the fallback if it fails
is the badged `plughw:` route.

Alternatives: fixed output rate, always gapless (not bit-perfect); `plughw:` fallback with a badge;
a per-device user setting.

### D-019 Loudness and crossfade: ReplayGain on by default (album mode, +4 dB pre-amp, TIDAL's formula), exposed as off / album / track, no crossfade (q6.1) — decided

Bypassed in bit-perfect mode along with the volume slider, stated in the UI. Crossfade is not
built: it is structurally incompatible with exclusive device access.

Alternatives: normalization off by default; crossfade in shared mode only.

## 2026-09-08 — Round 6: packaging the engine, DSP, caching, credentials

### D-020 GStreamer on Linux is bundled and pinned (q5.2) — decided

Flatpak (pinned runtime plus GStreamer module), AppImage and the daemon's Docker image all carry a
pinned GStreamer at or above 1.26.10 (FLAC-in-DASH floor) with a deliberately chosen plugin set
that includes an LGPL AAC decoder (`avdec_aac` or `faad`, never `fdk-aac`). Uniform behaviour and
both version floors met on Debian stable and Raspberry Pi OS, at the cost of roughly 20 MB and
owning security updates for a media stack. Windows and macOS bundle libmpv (D-016), so every
platform ships its own pinned engine.

Alternatives: system GStreamer with the `dashdemux2` rank-demotion workaround; a mixed
bundle-for-Flatpak, system-for-distros scheme; system only with a hard 1.26.10 requirement.

### D-021 No DSP, ever (q6.2) — decided

No EQ, crossfeed, upsampling or room correction. Stated as a product position in the README and
FAQ, with CamillaDSP named as the external route. Any DSP breaks the bit-perfect claim, and the
native TIDAL client has no equalizer either.

Alternatives: a shared-mode-only EQ; a full DSP chain with bit-perfect as one mode.

### D-022 On-disk audio: a pinned, encrypted offline cache for the logged-in subscriber (q6.3) — decided

Alternatives: no audio cache (Sone's shape, the research recommendation); a session-only opaque
buffer; deciding after v1.

Guardrails that make this a subscriber feature rather than a download feature, all mandatory:

- Explicit user pin per album/playlist/track, never automatic; artwork and metadata caching stay
  separate, capped and evicted.
- Encrypted at rest with a per-install key held in the OS keyring (encrypted-file fallback as in
  D-024), device-bound; no export, share or "open folder" path; files are opaque chunks, not
  playable media.
- Revalidated against the account on a validity window mirroring TIDAL's own offline model, and
  wiped on logout, on subscription lapse (`subStatus`), and when the pin is removed.
- Obtained as a transparent cache of the same cleartext stream the player would fetch anyway;
  never request `playbackmode=OFFLINE` or `usage=DOWNLOAD`, which are licensed, DRM-bound flows.
- Never cache PREVIEW assets or encrypted manifests; the encrypted-manifest refusal still applies.
- Per-track DASH manifests still go to a unique temp file per streaming session and are deleted on
  teardown.
- Documented plainly in the README's legal section next to the subscription requirement.

### D-023 Client credentials: embedded default with a user override (q7.1) — decided

No credential in the repository. An optional build-time input supplies a default client ID and
secret; settings expose a user-supplied pair. Docs state that any shipped credential is
extractable and that a secret-less ID filters the hi-res tiers out of the cascade. Packagers can
build with no credential at all.

Alternatives: user must supply their own; embedded and obfuscated (Sone, python-tidal).

## 2026-09-08 — Round 7: login, identity, tokens, play reporting

### D-024 Login: device code and PKCE both ship (q7.2) — decided

Device code (with a locally generated terminal QR) is the headless and CLI default; PKCE with a
loopback redirect on `streamboat://` is the desktop default. The token store records which client
pair minted each token because refresh must use the matching pair. PKCE is the only flow that
unlocks HI_RES_LOSSLESS; device code is the only flow that works on a headless box.

Alternatives: PKCE only (High Tide); device code only (caps quality below hi-res).

### D-025 Client identity: an honest User-Agent where it works (q7.3) — decided

Send a distinctive `streamboat/x.y` User-Agent by default and TIDAL's own device headers only on
the specific endpoints that demand them. No reference client has published whether an honest UA is
still served, so verify against a live account before the first release and keep a documented
fallback setting that switches to the reference clients' headers.

Alternatives: impersonate TIDAL's Android app on every request (the reference clients' habit);
send nothing distinctive.

### D-026 Token storage: OS keyring first, encrypted file fallback (q8.1) — decided

Keyring first; then an AES-256-GCM file whose key lives in the keyring; then a passphrase-derived
key. 0600 file in a 0700 directory. Under Flatpak use the Secret portal (no extra finish-arg).
Store only the refresh token in the Windows credential blob because of its size ceiling. On a
headless box the encrypted-file path is the normal case.

Alternatives: keyring only, fail loudly; an additional plaintext mode for servers.

### D-027 Play reporting: on by default, disableable, disclosed (q8.2) — decided

streamboat reports finished plays to TIDAL so Recently Played and Home personalisation behave as
the user expects out of the box (Sone's choice). Disclosed in the README, the metainfo and the
setting itself, which states exactly what is sent and that with the embedded credential the event
describes TIDAL's Android client.

Alternatives: off by default (the research recommendation); never implement it.

Operational rules, fixed regardless of the default: log a play only past 30 seconds; never for a
PREVIEW asset; drop permanently on a sender-fault batch error rather than retrying; derive
timestamps from a server-anchored clock (`GET /v1/ping`) rather than local system time, which
matters on a Pi without a real-time clock; a user-supplied client ID (D-023) changes what an honest
event looks like and the payload must follow the credential in use.

## 2026-09-08 — Round 8: account writes, crash reporting, control surface

### D-028 Account writes: library writes only (q8.3) — decided

Favourites, playlist create/edit/reorder (with ETag preconditions) and the queue; nothing else.
Profile, Block, Picks and AI-playlist writes stay out of reach.

Alternatives: read-only; add profile and taste writes.

### D-029 Crash reporting: local crash dumps plus a debug bundle; nothing leaves the machine unprompted (R-2, closed) — decided

The reopened item closes on the research default: structured logs with token redaction by
construction, local minidumps, and a "generate debug bundle" command the user attaches to an issue
by hand. No third-party or self-run crash service, no ingest key in the binary.

Alternatives: opt-in hosted reporting; an opt-in prefilled-GitHub-issue helper.

### D-030 Headless control surface: HTTP + WebSocket JSON on loopback (q9.1) — decided

One protocol for the GUI (in-process), the CLI, a web remote and a future phone app. Bound to
127.0.0.1 with a generated token and Host-header allowlisting; LAN exposure is a logged opt-in.
MPRIS and SMTC ship as day-one adapters over it, with MPRIS registration in the engine so it works
in daemon mode. Specify the track/album/playlist URI grammar before users store playlists
(mopidy-tidal's composite scheme is the precedent).

Alternatives: MPD protocol as primary; MPRIS/D-Bus only.

### D-031 Listener host and versioning: daemon only, additive and tolerant (q9.2) — decided

Only `streamboatd` binds a listener; the GUI speaks the same Command/Event types in-process without
opening a port. The protocol is versioned additively: a capabilities query, unknown fields ignored,
no field ever repurposed, so older clients keep working. The GUI can start hosting later without a
protocol change.

Alternatives: the GUI hosts a listener too; exact-match versions per release.
