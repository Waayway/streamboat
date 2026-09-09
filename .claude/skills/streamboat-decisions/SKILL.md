---
name: streamboat-decisions
description: The owner-ratified decisions for streamboat — the actual stack (Rust workspace, iced UI, GStreamer on Linux and libmpv on Windows/macOS behind one engine trait, two binaries over one core), product scope, quality tiers, licence split, offline-cache guardrails, login flows, control API, multiroom, distribution and testing posture — distilled from docs/DECISIONS.md. Load this for ANY task that touches streamboat code, architecture, packaging, product scope or docs, right after streamboat-overview and before any topic skill, because several owner decisions deliberately depart from the topic skills' research recommendations (iced instead of Tauri or Slint, a split audio engine, all four quality tiers at launch, a pinned offline cache, play reporting on by default, no code signing or updater in v1) and an agent that follows the research recommendation instead of the decision will build the wrong thing. Also load it whenever a task mentions "which toolkit", "which engine", "what did we decide", crate layout, first milestone, MVP scope, licence, App ID, or asks to record, change or revisit a decision.
---

# streamboat decisions

Source of truth: `docs/DECISIONS.md` — the dated log with alternatives, rationale and
consequences for every entry, numbered `D-001` onward. This skill is the load-on-demand
distillation. When the two disagree, the log wins; fix this skill in the same change.

Two rules for using it:

1. **A decision beats a recommendation.** The seven topic skills carry research
   recommendations and "Open decisions" sections written before the owner decided. Where this
   skill states a decision, the topic skill's recommendation is historical context, not
   guidance. Do not "correct" code or docs back toward the research recommendation.
2. **Only the owner changes a decision.** If a task seems to require departing from one, stop
   and surface the conflict with the `D-` number rather than silently deviating. Record any
   change the owner makes as a new dated entry in `docs/DECISIONS.md` that supersedes the old
   one (never edit history), then update this skill.

## The stack, in one table

| Area | Decision | Ref |
| --- | --- | --- |
| Language | Rust workspace; no webview shell of any kind | D-008 |
| UI toolkit | iced (pinned version), one streamboat look on all three OSes, token-based theming | D-012, D-013 |
| Audio engine | One `Engine` trait; GStreamer backend on Linux (bundled, pinned ≥ 1.26.10), libmpv backend on Windows and macOS (bundled) | D-016, D-020 |
| Output | Exclusive/bit-perfect on all three OSes in v1 (ALSA `hw:`, WASAPI exclusive, CoreAudio hog via libmpv), off by default; device stays open across same-format tracks, reopens on format change with a silence pre-roll | D-017, D-018 |
| Signal path | ReplayGain on by default (album mode, +4 dB pre-amp, off/album/track), bypassed in bit-perfect mode; no crossfade; no DSP ever (point at CamillaDSP) | D-019, D-021 |
| Process model | Two artifacts over one core (`streamboat` desktop shell with CLI subcommands, `streamboatd` daemon); single-instance lock (MPRIS bus name or lock file); GUI becomes a remote client if a daemon holds the lock; GUI reaches the engine only through the control API's Command/Event types | D-010, D-045 |
| Headless | Real daemon on Linux (systemd user/system units, Docker); Windows and macOS get always-on playback through the tray, not a service | D-011, D-014 |
| Control API | HTTP + WebSocket JSON on 127.0.0.1, generated token, Host allowlist; hosted by `streamboatd` only; additive, tolerant versioning with a capabilities query; MPRIS/SMTC are adapters over it, MPRIS registered in the engine; MPD subset later | D-030, D-031, D-032 |
| Licence | `streamboat-core` Apache-2.0; `streamboat-player`, `streamboat-server`, `streamboat-desktop` GPL-3.0-only; no CLA, no DCO | D-005, D-006, D-009 |
| Identity | GitHub `Waayway/streamboat`; app ID `io.github.waayway.streamboat`; URI scheme `streamboat://` (never `tidal://`) | D-007 |
| Mobile | Someday; `streamboat-core` has no UI/windowing/desktop-OS dependency, proven by a CI build with `--no-default-features`; iOS treated as closed | D-004 |

## Crate layout

```
crates/
  streamboat-core/     Apache-2.0  API client, OAuth (device code + PKCE), manifest parsing, models, token store
  streamboat-player/   GPL-3.0     Engine trait, gstreamer + libmpv backends, per-OS exclusive writers, queue, ReplayGain, offline cache
  streamboat-server/   GPL-3.0     streamboatd: control API, MPRIS/SMTC adapters, streaming-privileges socket, Snapcast plugin, systemd/Docker
  streamboat-desktop/  GPL-3.0     streamboat: iced shell, tray, mini-player, signal-path panel, CLI subcommands
```

Anything adapted from Sone, High Tide or Strawberry (all GPL-3.0) may land only in the GPL
crates. `streamboat-core` must stay reusable by non-GPL clients and must never link a GUI
toolkit; CI builds `streamboatd` in a container with no GUI libraries to prove it.

## Product scope for the first release (D-001, D-003, D-015, D-036 to D-039)

- Full client, not a core-player MVP: login, search, My Collection and favourites, playlists
  (create/edit/reorder with ETag preconditions), a real queue (reorder, play-next vs add-last),
  Home and Explore rendered from TIDAL's server-driven `home/feed` sections with an
  unknown-section fallback, hand-coded entity and collection pages, mixes and radio, synced
  lyrics, gapless playback, MPRIS/SMTC/NowPlaying with media keys — through both the desktop
  shell and the daemon.
- All four quality tiers at launch: LOW, HIGH (AAC via LGPL FFmpeg `avdec_aac` or `faad`, never
  `fdk-aac`), LOSSLESS, HI_RES_LOSSLESS. A startup decoder probe greys out unreachable tiers.
- No Dolby Atmos decoding (at most a passthrough flag kept in the manifest-request layer); MQA
  and 360RA are dead content, not targets.
- Extras in v1: the signal-path panel (must report only what the active engine can actually
  observe, and say "lossy source, bit-perfect not applicable" on AAC) and a floating
  mini-player (a second iced window over shared state). Theming presets, Discord, OBS and MCP
  are not v1.
- Scrobbling to Last.fm and ListenBrainz behind one abstraction.
- Video: later, desktop only, as a separate HLS path sharing the queue; show video entities as
  unplayable until then.
- Social: follow artists only. No Feed, profiles or Picks. Shared playlist links open and play.
- Account writes: library writes only (favourites, playlists, queue). Never profile, Block,
  Picks or AI-playlist writes.

## TIDAL access rules (D-022 to D-027, D-033 to D-035)

- **Credentials**: no credential in the repository; optional build-time default; settings expose
  a user-supplied client ID and secret; docs say any shipped credential is extractable and a
  secret-less ID drops hi-res from the cascade.
- **Login**: device code (terminal QR) is the headless/CLI default; PKCE with a loopback
  `streamboat://` redirect is the desktop default. The token store records which client pair
  minted each token; refresh uses the matching pair.
- **Identity**: honest `streamboat/x.y` User-Agent by default, TIDAL's device headers only on
  endpoints that demand them; verify against a live account before release; keep a documented
  fallback setting.
- **Tokens**: OS keyring first, then an AES-256-GCM file keyed from the keyring, then a
  passphrase-derived key; 0600 in 0700; Secret portal under Flatpak; only the refresh token in the
  Windows credential blob.
- **Play reporting**: on by default, disableable, disclosed in README, metainfo and the setting.
  Log only past 30 s, never for PREVIEW assets, drop on a sender-fault error without retry, use a
  server-anchored clock (`GET /v1/ping`), and shape the event after the credential in use.
- **Offline cache** (the sharpest line in the project): explicit user pin only; encrypted at rest
  with a keyring-held per-install key; device-bound; opaque chunks, no export path; revalidated on
  a validity window; wiped on logout, subscription lapse or unpin; obtained as a transparent cache
  of the ordinary cleartext stream — never `playbackmode=OFFLINE` or `usage=DOWNLOAD`; never for
  PREVIEW or encrypted manifests; documented next to the subscription requirement.
- **Encrypted manifests** (`encryptionType != "NONE"`, non-empty `encryptionKey`,
  `securityType`/`securityToken`) are refused loudly with an explanation; hi-res tiers are
  filtered out pre-emptively when no secret is configured.
- **Streaming privileges ("Pushkin") websocket**: connect; register a hostname-derived display
  name; claim only on genuine user intent (never autoplay or resume-after-hiccup); on takeover,
  pause and show "playback started on <device>" on every control surface; never auto-retry;
  capped backoff with jitter; reconnect on token refresh.
- **Multiroom / LAN audio**: Snapcast plugin output only (fixed-format PCM, mutually exclusive
  with bit-perfect), with a `streamboat snapcast-plugin` subcommand and mDNS discovery of
  snapserver. No Cast/AirPlay senders, no HTTP stream endpoint.
- **TIDAL Connect**: never a target, never a controller, stated publicly in README and in-app;
  do not advertise `_tidalconnect._tcp`; hand-off between the user's own instances and the
  official app goes through TIDAL's official play-queue API.
- **Privacy**: no telemetry; local crash dumps plus a "generate debug bundle" command with
  redaction by construction; no hosted crash service.

## Distribution and delivery (D-040 to D-047)

- AI-assisted development is disclosed in CONTRIBUTING; every commit message and any store
  submission PR is human-authored.
- Channels for v1: GitHub Releases with checksums; AUR source and `-bin`; hosted apt/dnf repo
  (deb/rpm vendor a pinned GStreamer tree under `/opt/streamboat`; AUR links Arch's system
  GStreamer); AppImage and a Docker image for the daemon; Windows MSI plus winget; macOS DMG.
  No Flathub for v1; Homebrew only after notarization exists.
- No code signing in v1: README documents the SmartScreen and Gatekeeper workarounds; revisit
  before 1.0.
- No in-app update check or updater; releases are announced on GitHub Releases.
- First milestone: a playable CLI spike (login, manifest, one track to the end through the
  Command/Event types, engine behind its trait on all three OSes, gapless-plus-exclusive
  prototyped) before any browse UI.
- Testing from the first commit: parser/transport split with synthetic fixtures and a mock HTTP
  server; clippy and rustfmt gates; the GUI-free daemon container build; iced UI tests through
  the toolkit's headless harness for key screens; an opt-in live canary outside CI. Fixtures are
  synthetic (real shapes, invented content); nothing of TIDAL's catalogue is republished.

## Where the owner departed from the research

Keep these in mind so you do not "fix" them back:

| Topic | Research recommendation | Owner decision |
| --- | --- | --- |
| UI shell | Tauri 2 + React (or Slint without a webview) | iced, no webview |
| Engine | GStreamer everywhere with libmpv as secondary | GStreamer on Linux, libmpv on Windows/macOS |
| First release | Core player only | Full client with pages, mixes, lyrics |
| Quality tiers | Lossless now, lossy later | All four at launch |
| Bit-perfect scope | Linux and Windows first | All three OSes in v1 |
| Offline audio | No audio cache | Pinned, encrypted cache with guardrails |
| Play reporting | Off by default | On by default, disclosed |
| Contribution | DCO sign-off | No agreement |
| Code signing | Both platforms budgeted | None for v1 |
| Updates | Check and notify | GitHub Releases only |
| Testing | Unit and fixture tests | Unit, fixture and UI tests |

## Status of delivery

Every item of the decided first release (D-001, D-003, D-010 to D-047) has an implementation:
the playable spike (D-044); both login flows (D-024) and keyring-backed token storage (D-026);
GStreamer on Linux and libmpv on Windows/macOS behind one `Engine` trait with `default_engine`
picking the compiled-in backend (D-016); exclusive output through the ALSA writer and libmpv's
WASAPI/CoreAudio AOs (D-017, D-018); ReplayGain modes (D-019); the decoder probe (D-003); the
control API (D-030, D-031); MPRIS, SMTC and NowPlaying adapters (D-030); streaming privileges
(D-033), play reporting (D-027) and scrobbling (D-037); the pinned encrypted offline cache
(D-022); Snapcast output, plugin and discovery (D-034); local crash reports and the debug bundle
(D-029); the iced shell with every decided screen, the mini-player, the tray and the
single-instance/remote-client model (D-010, D-013 to D-015, D-036, D-038, D-039); packaging and
the release workflow for every decided channel (D-040 to D-043); the test posture (D-046, D-047).
`docs/architecture.md` is the inventory, with the implementation choices fixed along the way and
the "not yet built" list — which now names only what this build environment could not verify
(Windows/macOS runtime behaviour, a real DAC, a session bus, a snapserver, the display) plus a
handful of small follow-ups (vendored GStreamer wired into deb/rpm, the libmpv/FIFO Snapcast pump
on Windows/macOS). The Settings screen's output-mode picker (Shared/Exclusive/Snapcast, D-034) and
handing a deep link to an already-running instance (D-010, D-024) are both built now.

## Still open (implementation-time calls, not owner decisions)

- Universal versus Apple-Silicon-only macOS builds (decide with the first macOS CI job; the
  bundled libmpv tree must match).
- Whether deb/rpm vendoring of GStreamer lives under `/opt/streamboat` or the AppImage is the
  recommended install on Debian stable (confirm with the first packaging job).
- Whether to also parse the still-live v1 `pages/home` shape as a fallback for the v2 feed.
- Whether exclusive ALSA works under a sandbox (one test run decides whether the toggle is greyed
  out there).
- GStreamer's HLS path for TIDAL video on Linux (needs a spike before video is scheduled).

## How to record a new decision

Append a dated `### D-NNN Title (qX.Y or free-form) — decided` entry to `docs/DECISIONS.md`
with the decision, the alternatives, why, and consequences; if it supersedes an earlier entry,
say so in both places. Then update the table above and, if a topic skill's guidance changes as
a result, that skill and its report in the same change.
