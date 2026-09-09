---
name: streamboat-overview
description: Read this first for any task in the streamboat repository — before writing code, docs, or research, before picking a stack, before touching TIDAL API/audio/packaging/legal questions, and before creating or editing anything under .claude/skills or docs/. This is the entry point that names the owner's actual decisions (so you stop re-litigating them), the glossary that keeps TIDAL/project vocabulary consistent across the codebase, and the index that tells you which of the seven topic skills to load next and why. Skipping this skill is how an agent re-derives a decision the owner already made, uses the wrong quality-tier name, or duplicates work that already lives in docs/research. Load it once per session/task, then load exactly the topic skill(s) the task actually touches.
---

# streamboat overview

streamboat is a new open-source TIDAL client. The goal, stated by the owner, is to eventually do
everything the native TIDAL client does — desktop and headless — for people who already pay for
TIDAL. It is not built yet: as of this writing the repository holds only a README, a `.claude/skills/`
research base, and `docs/research/` reports. No stack, no code, no architecture has been chosen.

This skill is the map. Read it before doing anything else in this repository. It does not contain
the deep facts themselves — those live in the seven topic skills indexed below and the research
reports they distill — it exists so you load the *right* one instead of guessing, and so you never
restate a decision the owner has already made.

## What streamboat is

- A TIDAL client — playback, browsing, library, search, queue — for **subscribers**, built against
  the same unofficial `api.tidal.com` API that the open-source clients High Tide (Nokse22/high-tide)
  and Sone (lullabyX/sone) use, and that `python-tidal` wraps. It requires the user's own paid TIDAL
  subscription; there is no free tier and no way around that.
- Not a downloader, ripper, or DRM-circumvention tool. It plays streams a paying account is entitled
  to play; it does not decrypt protected content, does not scrape TIDAL to build an offline library
  for redistribution, and does not exist to defeat TIDAL's access controls.
- Scoped, eventually, to match what TIDAL's own apps do across desktop, headless, and (later) mobile
  — but scoped *now* to a smaller MVP; see `tidal-client-features/references/feature-matrix.md` for
  the actual MVP/v1/later/out-of-scope tiering, and the Status section below for how the remaining
  stack/architecture decisions get made.

## Owner decisions already made — do not re-litigate these

These are fixed. Every topic skill repeats them; they are collected here once so this skill is the
single place to check.

- **Platforms, now**: desktop Linux, Windows, and macOS, **and** a headless/server/CLI mode — all of
  it now, not staged. **Mobile (Android/iOS) is future scope**: nothing mobile-specific ships today,
  but no architecture choice may preclude it later (this is why, e.g., the core must not depend on a
  UI toolkit, and why licence choice is weighed against future App Store distribution).
- **"Something simple but beautiful" — and, since 2026-09-08, a decided stack.** The owner worked
  through the decision tree: Rust workspace, **iced** desktop toolkit (no webview), **GStreamer on
  Linux and libmpv on Windows/macOS** behind one engine trait, two binaries (`streamboat`,
  `streamboatd`) over one core, GPL-3.0-only apps over an Apache-2.0 core, a full client with all
  four quality tiers at first release, exclusive/bit-perfect output on all three OSes. The complete
  list with rationale is `docs/DECISIONS.md` (`D-001` to `D-047`), distilled in the
  `streamboat-decisions` skill — load that skill right after this one. Where a topic skill's
  research recommendation differs (Tauri, Slint, GStreamer-everywhere, core-player MVP, no offline
  cache, play reporting off), the decision wins.
- **TIDAL API approach**: do what High Tide and Sone do — stream through the unofficial
  `api.tidal.com` API that `python-tidal` also uses, which requires the user's own paid subscription.
  This is a considered choice, not a shortcut: it's the only path that gives a third-party client
  full-track audio at all (the official Developer API's Player module is closed-source and can't run
  headless). Document the legal risk honestly, including how High Tide and Sone themselves position
  it (non-affiliation disclaimer, subscription-required, reverse-engineering risk under TIDAL's
  Content Guidelines) — see `tidal-oss-landscape/references/legal-posture.md` and
  `tidal-api/references/legal-and-landscape.md`.
- **Subscriber-only, never piracy tooling.** streamboat is a player, not a ripper. Never design,
  build, or document DRM circumvention, encrypted-manifest decryption (`OLD_AES`, Widevine, etc.),
  credential pooling, or an export/download path meant to leave a playable copy behind after the
  subscription ends. Refuse encrypted manifests loudly and explain why — never silently, never with a
  workaround. This line is absolute across every topic skill; see pitfall tables in `tidal-api`,
  `tidal-oss-landscape`, and `audio-pipeline` for the exact refusal rule
  (`encryptionType != "NONE"` or a non-empty `encryptionKey`/`securityType`+`securityToken`).

## Guiding principles

- **The owner's stated decisions are load-bearing; everything else is a researched recommendation,
  not yet ratified.** Several topic skills flag places where their own report's strong opinion (e.g.
  "start with libmpv," "bit-perfect is a core feature") has been mistaken for an owner decision
  before — it isn't one until the decision tree says so. Read a skill's "Research conclusions" or
  "strong recommendations, owner has not ratified" section as exactly that.
- **Model TIDAL's own vocabulary, not an assumption of what a name should mean.** TIDAL's own enum
  and UI names frequently do not match ("High" in the UI is enum `LOSSLESS`; enum `HIGH` is the lossy
  AAC tier) — see the Glossary below and the pitfall tables in `tidal-client-features` and `tidal-api`
  for the recurring cases. When in doubt, use TIDAL's own field/enum names, not an invented synonym.
- **Never decrypt, never impersonate past what's disclosed, never route around access control.**
  Applies to audio streams, to TIDAL Connect (a closed, certificate-gated protocol streamboat will
  never implement as a target), and to any endpoint that requires pretending to be a different,
  specific client to work (e.g. writing Recently Played requires impersonating TIDAL's Android app —
  that's a real policy decision, not a detail to route around quietly).
- **Read the source-of-truth report before trusting a skill's summary on a load-bearing fact.** Every
  topic skill is a distillation of a much longer, fact-checked report in `docs/research/`. The skills
  carry the pitfalls and quick answers; the reports carry the full evidence, citations, and "why."
- **Conflicts between skills are flagged, not hidden — resolve them with the owner, don't silently
  pick a side.** The one such conflict the research left open (`audio-pipeline` vs.
  `tech-stack-evaluation` on engine order) was resolved by the owner as D-016: GStreamer on Linux,
  libmpv on Windows and macOS, one trait. Where the two skills still carry their CRITICAL callouts,
  read them as history. Raise any new conflict to the owner rather than picking a side.
- **Cite evidence, keep evidence tiers visible.** The topic skills tag claims `[verified-source]`,
  `[verified-web]`, `[inferred]`, `[unverified]`/`[uncertain]` (exact tag vocabulary varies slightly
  per skill — each skill defines its own at the top). Don't flatten an `[unverified]` claim into
  settled fact when writing code, docs, or specs.

## Glossary

TIDAL vocabulary and project terms that recur across skills, with the pitfall attached where the
name is a trap. Full detail and sourcing is always in the topic skill named in parentheses.

**Quality tiers** — TIDAL's wire enum (`audioQuality`) and its UI label do **not** line up name-for-name:

| Wire enum | Meaning | UI label |
| --- | --- | --- |
| `LOW` | Lowest tier, HE-AAC v1 ~96 kbps | no current UI label |
| `HIGH` | Lossy AAC-LC ~320 kbps | (not "High" in the UI) |
| `LOSSLESS` | FLAC 16-bit/44.1 kHz | **"High"** in the UI |
| `HI_RES_LOSSLESS` | FLAC up to 24-bit/192 kHz | "Max"/Hi-Res Lossless |
| `HI_RES` | Legacy MQA-era tier | retired content (MQA withdrawn ~24 Jul 2024); live behaviour unresolved |

Never branch on an enum name looking like an English word (`tidal-client-features` pitfall #1,
`tidal-api` pitfall #15). Also watch the **metadata-tag** vocabulary, which is spelled differently
from the enum: tag `HIRES_LOSSLESS` has no underscore before "RES," unlike enum `HI_RES_LOSSLESS`
(`tidal-oss-landscape` pitfall #3). The account's real ceiling comes from
`GET /v1/users/{id}/subscription`'s `highestSoundQuality` — fetch and cache it before starting any
quality cascade (`tidal-api` pitfall #18).

**Manifest formats** — a `playbackinfopostpaywall` (v1) or `trackManifests` (v2) response returns a
base64-encoded manifest in one of: **BTS** (a JSON payload with a direct, pre-signed CDN URL — no
`Authorization` header, expires via its own query token), **DASH** (an MPD XML document with
`SegmentTemplate`/`SegmentTimeline`, FLAC/AAC/HE-AAC/E-AC-3 packaged in fragmented MP4), **HLS**
(m3u8, used for video and some audio paths), and **EMU** (a JSON variant). `encryptionType != "NONE"`
(or a non-empty `encryptionKey`/`securityType`+`securityToken`) means **refuse the stream, never
decrypt** — see Owner decisions above. (`audio-pipeline`, `tidal-api`)

**Mixes and radio** — TIDAL's algorithmic playlists (`MixType` enum: e.g. Daily Discovery, My Mix,
track/artist radio). `NEW_HISTORY_MIX` is a **Feed `activityType`, not a `MixType` value** — a
documented error in one source report that's worth remembering as the shape of this kind of trap.
(`tidal-client-features/references/browse-pages-screens.md`)

**Pages API** — TIDAL's module/section-based page-rendering system for Home, Explore, artist/album
pages, etc. Two generations coexist: legacy v1 (`pages/home`, `pages/my_collection_recently_played`,
and similar slugs) and current v2 (`home/feed/{slug}`, with tabs and cursor pagination) — **these are
not the same endpoint and `home/feed/static` is Home, not the social Feed.**
(`tidal-client-features/references/browse-pages-screens.md`)

**TIDAL Connect** — TIDAL's own multi-device-casting protocol. Verdict, settled by research: as a
**target** (streamboat pretending to be a Connect-capable speaker/receiver), **never** — the only
working implementation is a single closed, deliberately-obfuscated binary reusing one vendor's
device certificate, and reimplementing it means defeating that obfuscation and forging or reusing
someone else's device identity. As a **controller** (streamboat casting *to* a Connect device),
**out of scope, not categorically ruled out** — no open implementation or protocol capture exists
anywhere yet; revisit only if one surfaces. (`headless-and-tidal-connect/references/tidal-connect.md`)

**BTS / DASH manifests** — see "Manifest formats" above.

**Bit-perfect / exclusive mode** — playing audio with no OS-level resampling, dithering, or mixing:
Linux via a raw ALSA `hw:`/`plughw:` device (never through a `plug`/mixing layer), Windows via WASAPI
exclusive mode, macOS via CoreAudio hog mode. Undefined for lossy tiers (`LOW`/`HIGH`) — there is no
canonical bit-width to "preserve" from a lossy decode. Also disables ReplayGain/loudness
normalization, not only the volume slider — state that explicitly in any UI, don't imply gain still
applies. (`audio-pipeline`)

**MPRIS / SMTC** — OS media-integration surfaces: **MPRIS2** is the Linux D-Bus interface (four
sub-interfaces: Root, Player, `TrackList`, `Playlists` — not transport-only) that Wayland compositors
use to route hardware media keys, so it's mandatory wiring, not an optional nicety. **SMTC** (System
Media Transport Controls) is the Windows equivalent; **`MPNowPlayingInfoCenter`** is macOS's. The
crate `souvlaki` 0.8.3 actually covers all three OSes from one crate, though most reference clients
still split by platform. (`audio-pipeline/references/os-integration.md`,
`headless-and-tidal-connect`)

**Pushkin** — TIDAL's real-time streaming-privileges websocket (`rt/connect`,
`PRIVILEGED_SESSION_NOTIFICATION`). Enforces **one concurrent, privileged (playing) stream per
account** across every device and every process — not just per local sound device. A headless daemon
and a desktop GUI on the same account, on different machines, will revoke each other. Related:
`subStatus` **4006** on `playbackinfo` is this same condition surfacing as a recoverable 401 (never
evict the track on it), and **4033** (subscription up-sell) is also recoverable — the terminal
sub-status set is the fixed list `4005, 4010, 4030, 4031, 4032, 4034, 4035`, not a numeric range.
(`headless-and-tidal-connect/references/daemon-architecture.md`, `tidal-api/references/transport.md`)

**`client_unique_key`** — a value streamboat generates once at first login and must persist forever;
regenerating it burns through TIDAL's per-account device cap and the official SDK throws on refresh
if it changes mid-session. (`tidal-api/references/auth.md`)

**Device-code / PKCE** — the two OAuth flows against `auth.tidal.com`. Device-code has no browser
handoff and no reCAPTCHA gate, making it the reliable default for headless/CLI; PKCE requires a
browser and is reCAPTCHA v3-gated. Field casing and scope-delimiter conventions differ between the
two flows and must not be "fixed" into consistency. (`tidal-api/references/auth.md`)

**`streamboat-core` / daemon / server split** — the recommended architecture: a core library with no
UI and no server dependency (session, catalogue, queue, player engine, output backends, Pushkin
client), a `streamboat-server`/daemon binary that adds the control API/MPRIS/mDNS/MPD-subset, and a
GUI that talks to the same command/event surface the server exposes, even when running in-process.
This is what makes headless mode and a future mobile core cheap instead of a rewrite.
(`headless-and-tidal-connect`, `tech-stack-evaluation/references/architecture-shapes.md`)

## Index of the other skills

Load the topic skill(s) the task actually touches — not all of them, and not from memory. Each
skill is itself a distillation of a much longer fact-checked report in `docs/research/`; go one level
deeper (the skill's own reference files, then the report) only when the task needs that depth.

- **`streamboat-decisions`** — the owner-ratified stack, scope and rules, distilled from
  `docs/DECISIONS.md`. Load it for every task right after this skill and before any topic skill: it
  is what to build; the topic skills are how and why. It also lists where the owner departed from
  the research recommendations so you do not "correct" the code back toward them.
- **`tidal-client-features`** — product knowledge: what TIDAL's native apps (desktop/web/mobile/TV/
  Connect) actually do, what the unofficial API can reach, and the MVP/v1/later/out-of-scope tier for
  every feature. Load whenever you touch playback/quality/queue logic, browse/home-page rendering,
  playlists/library, search, artist/album/track pages, lyrics, remote playback, account/entitlements,
  or sharing/deep-links — or when scoping "should streamboat build X."
- **`tidal-api`** — wire-level knowledge of both TIDAL APIs streamboat talks to: the unofficial
  `api.tidal.com` v1/v2 surface (auth, playback-info/manifest, catalog, playlists, favorites, lyrics,
  play-reporting) and the official `openapi.tidal.com/v2` Developer API. Load whenever you write or
  review code touching either host, OAuth/token/keyring code, a playback-info or manifest parser,
  quality-tier logic, or catalogue/library mutation.
- **`tidal-oss-landscape`** — architectural precedent from 20+ open-source TIDAL clients and SDKs
  (Sone, High Tide, Strawberry, tidal-hifi, TidaLuna, mopidy-tidal, tidalt, python-tidal, the official
  SDKs, and more): what each one got right or wrong on auth, stream resolution, bit-perfect output,
  gapless playback, caching, credentials, and packaging. Load before designing any subsystem that has
  a working precedent worth copying (or explicitly not copying) — this skill's pitfall table is full
  of "X looks like the obvious design, here's why it's wrong."
- **`audio-pipeline`** — deep, code-level knowledge of the playback pipeline: manifest → decoder →
  bit-perfect/exclusive-mode output on Linux/Windows/macOS or a headless daemon, gapless, crossfade,
  ReplayGain, buffering/caching, Dolby Atmos feasibility, OS media integration, and the candidate
  engine stacks (GStreamer, libmpv, Symphonia, FFmpeg) with licensing. Load for any manifest fetch or
  parser, decoder/demuxer, output/device layer, gapless/crossfade logic, or normalization code.
- **`headless-and-tidal-connect`** — everything for the headless/server/CLI mode: the daemon-vs-GUI
  core split, streamboat's own local control protocol (HTTP+WebSocket JSON, MPRIS, an MPD-compatible
  subset, a Unix socket), Pushkin, multiroom output (Snapcast, Sendspin), Raspberry Pi deployment,
  headless login/pairing, and the full TIDAL Connect verdict (target: never; controller: out of
  scope). Load for any daemon/server/headless subcommand, control surface, discovery/pairing code, or
  multiroom/Connect question.
- **`tech-stack-evaluation`** — the researched answer to "what should streamboat be built in":
  language, UI toolkit, audio engine, process architecture, scored against 14 candidate stacks. The
  research recommended Tauri 2 + React; the owner decided on iced with a split engine (see
  `streamboat-decisions`). Load it for the underlying facts about toolkits, engines, packaging and
  the mobile path when writing `Cargo.toml`, CI or packaging scripts.
- **`streamboat-engineering-baseline`** — stack-agnostic engineering conventions: how to test an
  unofficial-API client without live credentials in CI, secret/token storage per OS, config/cache/log
  layout, licensing (GPL/LGPL/Apache split and compatibility), packaging and distribution per channel
  (Flathub, Snap, deb/rpm/AUR, AppImage, MSI/winget, DMG/notarization), CI design, repo layout/
  CONTRIBUTING, i18n/accessibility, and observability. Load for any CI workflow, secrets/keyring code,
  config/cache path resolution, packaging manifest, LICENSE/SPDX file, or test harness.

For streaming wire-format depth, start at `tidal-api`. For DSP/output engineering depth, start at
`audio-pipeline`. For "what should streamboat build," start at `tidal-client-features`. For "how do I
build it," start at `tech-stack-evaluation` and `streamboat-engineering-baseline`. For "what did
other TIDAL clients do here," start at `tidal-oss-landscape`. For "what did we decide," start at
`streamboat-decisions`.

## Status

**The tech stack, architecture and first-release scope are decided** (2026-09-08/09). The owner
worked through all twelve rounds of the decision tree in `docs/research/decision-tree.json`; every
answer, with alternatives, rationale and consequences, is in `docs/DECISIONS.md` (`D-001` to
`D-047`, plus "the decided stack at a glance"), and the `streamboat-decisions` skill is its
load-on-demand distillation.

**The first release's scope (D-001) is built**, on Linux end to end and on Windows/macOS as
compiled-but-unrun code: `cargo build --workspace` produces `streamboat` (the iced 0.14 desktop
shell with CLI subcommands: login, search, resolve, play, devices, pin/unpin/pins, open,
snapcast-plugin, snapcast-discover, debug-bundle, keyring, paths) and `streamboatd` (the daemon
hosting the HTTP + WebSocket control API, `--stdio` for the JSON-lines protocol). Both engines,
exclusive output, the ALSA writer, MPRIS/SMTC/NowPlaying, privileges, play reporting, scrobbling,
the offline cache, Snapcast, diagnostics and packaging exist. `docs/architecture.md` records what
exists, the implementation choices made along the way, and the honest "not yet built / not
verifiable here" list (no display, DAC, session bus, snapserver or Windows/macOS machine in the
build environment). Read it before touching code; load `iced-ui` before touching the shell.

Topic skills still carry their pre-decision "Open decisions" sections, each now headed by a note
pointing at the decision log; read them as the inputs that were considered, not as open questions.
Implementation-time calls that remain genuinely open (universal macOS builds, deb/rpm GStreamer
vendoring, the v1 `pages/home` fallback, sandboxed exclusive ALSA, GStreamer HLS for video) are
listed at the end of `streamboat-decisions`.

## Working conventions

- **Research lives in `docs/research/`** — long, fact-checked reports, one per topic, each the
  source of truth its matching skill distills from. See `docs/research/README.md` for the index.
- **Distilled, load-on-demand knowledge lives in `.claude/skills/`** — one skill per research topic,
  each a `SKILL.md` plus `references/*.md` files, kept in sync with its report. This skill
  (`streamboat-overview`) is the index and entry point for all of them.
- **Decisions live in `docs/DECISIONS.md`**, a dated, append-only log (`D-NNN` entries; a change
  is a new entry that supersedes the old one), distilled into the `streamboat-decisions` skill. A
  decision beats a research recommendation; only the owner changes one.
- **Keep skills updated when facts change.** If a research report is corrected, or a decision
  resolves something a skill lists as open, update the skill's pitfall table, "Owner decisions," or
  "Open decisions" section in the same change — don't let the skill drift out of sync with the report
  or the decision log. Skills cite their source report inline; when in doubt about which one is
  authoritative on a specific claim, the report is (skills are summaries, not a second source).
- **Cite sources as URLs or as `ref:<project>/<path>`** (a path into the read-only reference
  checkouts used during research) — never assert a TIDAL-specific fact without one of these, and
  never present an `[unverified]`/`[inferred]` claim from a topic skill as settled.
