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
