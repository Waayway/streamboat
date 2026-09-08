---
name: tidal-oss-landscape
description: Architectural precedent from 20+ open-source TIDAL clients and SDKs (Sone, High Tide, Strawberry, tidal-hifi, TidaLuna, mopidy-tidal, tidalt, tidalrs, python-tidal, tidal-connect, the official tidal-sdk-web/android/ios, and more) for how streamboat should do auth, stream resolution, manifest/DASH/BTS parsing, bit-perfect/exclusive audio output, gapless playback, quality cascades, caching, credential handling, packaging, and the desktop+headless architecture split. Use this whenever writing or reviewing code that touches TIDAL auth/login, playbackinfo/manifest/stream-URL resolution, the audio pipeline (GStreamer, ALSA, bit-perfect, exclusive mode, gapless, normalization/ReplayGain), the core/daemon-vs-UI split, credential or token storage, packaging (Flatpak/Snap/AUR/deb/rpm/Nix/Windows installer), or update mechanisms — or whenever a task mentions TIDAL, python-tidal, tidalapi, Sone, High Tide, Strawberry, tidal-hifi, TidaLuna, mopidy-tidal, tidalt, tidalrs, playbackinfo, manifest, DASH, BTS, audioquality, ReplayGain, bit-perfect, WASAPI, ALSA, hog mode, gapless, subStatus, client ID/secret, or "open-source TIDAL client". Do not answer these from general music-app knowledge or general GStreamer/audio-API knowledge — TIDAL's actual unofficial API and the reference clients' hard-won engineering choices differ from generic assumptions in ways that cause silent bugs (see the pitfalls table below), and this skill is the one place that knowledge has already been fact-checked against the actual source of 20 projects.
---

# TIDAL open-source landscape for streamboat

Source of truth: `/home/user/streamboat/docs/research/oss-landscape.md` (the full research
report, corrected against **two** independent fact-check passes — read it for narrative depth and
the complete source list). This skill is the load-on-demand distillation: the facts, endpoints,
code pointers, and pitfalls an implementer needs while writing code, without re-reading a
2,600-line report every time. Note: the *first* fact-check pass itself introduced a couple of
errors while correcting the original draft (notably: it wrongly claimed Sone emits no
`track-advanced` event); the second pass caught these — see `references/verification-notes.md`
§1b if a fact here ever looks inconsistent with something you remember reading.

`ref:<project>/<path>` throughout this skill and the main report points at a shallow git clone of
a named open-source project kept in the research environment (e.g.
`ref:sone/src-tauri/src/audio.rs`) — read-only evidence, not part of the streamboat repo. Full
project→URL mapping, including projects cited by URL only with no local checkout, is in
`references/sources.md`.

## Owner decisions already made (do not re-litigate these)

- **Platforms now**: desktop Linux + Windows + macOS, and a headless/server/CLI mode, both now.
  Mobile (Android/iOS) is future scope — the architecture must not preclude it, but nothing
  mobile-specific ships now.
- **Stack**: "something simple but beautiful" — otherwise open; this skill documents the
  precedents, `docs/research/tech-stack.md` makes the recommendation.
- **TIDAL API approach**: do what High Tide and Sone do — the unofficial API used by
  `python-tidal`, which requires the user's own paid TIDAL subscription. streamboat is a player
  for subscribers, not a downloader/ripper. Document legal risk honestly; never design or
  document DRM circumvention or piracy tooling as a how-to (see Pitfall 1 below — this is not a
  hypothetical, TidaLuna does exactly this and it must never be ported).

## Facts that cause silent bugs if you get them wrong

| # | Trap | The fix |
| --- | --- | --- |
| 1 | TidaLuna contains a hardcoded AES key that decrypts `OLD_AES`-flagged streams. Copying it (or reimplementing the same idea) turns streamboat into DRM-circumvention tooling, which the project brief explicitly rules out. | **Never decrypt.** If a manifest's `encryptionType`/`securityType`/`encryptionKey` is non-empty, refuse the stream with Strawberry's message shape ("depends on the client ID in use, try changing it in settings"). `references/project-profiles.md` §4, §2. |
| 2 | A `subStatus` in `4000..=4999` on a 401 from `playbackinfo` looks like an auth error but **is not** — refreshing the token does nothing. Only `4005,4010,4030,4031,4032,4034,4035` are terminal (skip the track); `4033` is deliberately *not* in that list (subscription up-sell, user-fixable) and neither is `4006` (recovers on its own). | Copy the literal set, never a range. `references/api-auth-streaming.md` §6. |
| 3 | `HI_RES_LOSSLESS` (enum) vs `HIRES_LOSSLESS` (metadata tag) — no underscore before RES on the tag. String-equality checks across the two vocabularies silently fail. | Always use a lookup table between the `Quality` enum and `MediaMetadataTags`. `references/api-auth-streaming.md` §7. |
| 4 | ALSA `S24LE` (24-in-32, 4 bytes) = GStreamer `S24_32LE`; ALSA `S243LE` (packed, 3 bytes) = GStreamer `S24LE`. The names are inverted between the two libraries. Getting this backwards is a silent audio-corruption bug, not a crash. | `references/audio-engineering.md` §1. |
| 5 | High Tide's on-disk track cache is **not** an opt-in "music folder" feature — `MUSIC_DIR` is unconditional, there is no setting to disable it, and it's inside the project the owner named as a model. | Make offline caching a deliberate, documented streamboat decision (`SKILL.md` Open decisions #3), not an inherited default. `references/project-profiles.md` §1. |
| 6 | Sone's queue, shuffle, repeat, and history live in the **React webview**, not in Rust — the backend only persists a snapshot and mirrors frontend state out to MCP/overlay. This is *why* sone-windows is a stale fork rather than a portable core. | streamboat's core must own session/queue/transport; every UI is a thin subscriber — invert Sone's shape, don't copy it. `references/sone-deep-dive.md` §3. |
| 7 | `cpal` (the obvious pure-Rust audio crate) **cannot do WASAPI exclusive mode at all** — there is no flag or feature for it. | If going pure-Rust, budget a hand-written `wasapi`-crate Windows backend (same class of work as GStreamer's own missing macOS-exclusive backend). `references/audio-engineering.md` §5. |
| 8 | Sone's own Flathub manifest grants PulseAudio + read-only PipeWire but **no raw ALSA device access** — its flagship bit-perfect feature cannot run in the build Flathub actually ships. | Flathub = convenience tier only; exclusive/bit-perfect is deb/rpm/AUR/Nix/Snap-only. Plan this before advertising the feature on Flathub. `references/packaging-distribution.md` §2. |
| 9 | The unofficial stream endpoint (`playbackinfopostpaywall`) over-requesting quality returns **200 with a silently downgraded `audioQuality`, never an error**. Walking a quality cascade on a network error/rate-limit/terminal sub-status just multiplies request count 4× for nothing. | Stop the cascade immediately on those three conditions; only step down the ladder on an actual quality mismatch in the response. `references/api-auth-streaming.md` §7. |
| 10 | Enabling EBU R128 loudness normalization in a GStreamer pipeline forces the downstream chain to float caps (`F32LE`/`F64LE`) — this is **structurally incompatible** with bit-perfect integer output, not just "should be off by default." | Any normalization stage must be bypassable, not merely defaulted off. `references/audio-engineering.md` §4. |
| 11 | The entire `tidal.com` domain (not just `developer.tidal.com`/`support.tidal.com`) is blocked from this research environment — every quote from TIDAL's own policy text anywhere in this skill is second-hand. | Never cite these quotes as primary-sourced in user-facing legal text without a direct, unproxied re-read first. `references/verification-notes.md` §4. |
| 12 | mopidy-tidal's "login hack" QR code (the cleverest headless-auth idea in the set) sends the one-time TIDAL login URL to a third-party host (`api.qrserver.com`) to render it. | Generate QR codes locally if adapting this pattern — tidalt's `mdp/qrterminal` is the pattern to copy instead. `references/api-auth-streaming.md` §3. |
| 13 | A seek that detaches the armed gapless next-track slot (the "obvious" implementation) destroys a valid preroll and produces an audible gap — Sone tried this and reverted it. | Seek must leave the inactive prerolled `concat` branch alone; only the active branch gets flushed. `references/sone-deep-dive.md` §3a. |
| 14 | Gapless *arming* logic that gates on `isPlaying` will silently disable itself during ALSA device-busy retries, because `isPlaying` flickers false during those retries and on every pause. | Gate arming on `gapless && !exclusiveMode && !bitPerfect && !currentVideo && currentTrack` — never on `isPlaying`. `references/sone-deep-dive.md` §3b. |
| 15 | "Sone has 151 tests" reads as a test harness for the hard parts. It isn't one — `[dev-dependencies]` is one line (`tempfile`), and all 151 tests cover pure functions only; the HTTP layer, manifest parsers, and ALSA path have zero automated coverage. | Design a parser/transport split plus captured fixtures as new work — no project in this reference set demonstrates that pattern. `references/sone-deep-dive.md` §7. |
| 16 | Every project in this landscape except High Tide and mopidy-tidal — Strawberry included — is effectively bus-factor 1. "Very mature, very active" (Strawberry) is not the same signal as "safe to depend heavily on." | Read "maturity" as age + release cadence + CI, not community size, when deciding how much to lean on any one reference project. `references/sone-deep-dive.md` §10. |
| 17 | No project in the reference set signs its Windows/macOS builds or ships a signed in-app updater — including tidal-hifi, which has the best release CI in the set. | Budget code signing/notarization and decide the update mechanism per distribution channel as new work from day one; there is nothing to copy. `references/packaging-distribution.md` §3, §7. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| Official vs unofficial API surfaces, auth flows (device code, PKCE, three redirect-capture strategies, python-tidal's incomplete poll loop), stream-resolution endpoints and params, manifest formats (BTS/DASH/EMU/HLS, four DASH delivery strategies), the `subStatus` taxonomy (incl. the streaming-privileges/`4006`/Pushkin gap), quality-cascade stopping rules, credential-handling risk profiles, the Dolby Atmos/`SONY_360RA` gap | `references/api-auth-streaming.md` |
| Bit-perfect ALSA negotiation (format/rate probing, promotion table, S24 naming inversion, hw/sw params), three gapless designs compared, seeking's interaction with `concat` gapless, DASH caps handling, three normalization formulas (incl. why EBU R128 breaks bit-perfect), the pure-Rust-vs-GStreamer question (librespot, cpal, wasapi), macOS CoreAudio hog-mode precedents outside this reference set, tidalt's ALSA refinements (incl. release-on-pause) and combined device hot-plug/busy/lost policy, volume curves | `references/audio-engineering.md` |
| Sone's full stack, module map, IPC design and the state-ownership inversion to avoid, seeking, the gapless prefetch/arming policy (the frontend half), caching/crypto/settings, play-reporting wire format, music videos, packaging+CI (and why there is none, incl. what "151 tests" doesn't cover), code quality, borrow/avoid list, the sone-windows fork (incl. its Windows GStreamer/AAC-decoder gap), bus factor/contributor counts | `references/sone-deep-dive.md` |
| Per-project deep dives for everything else: High Tide, Strawberry, tidal-hifi, TidaLuna (incl. the official Redux action-namespace dump and Atmos/360RA handling), mopidy-tidal, Music Assistant, lms-plugin-tidal, tidal-connect, the three official SDKs, python-tidal, tidal-cli, tidalt (incl. its daemon/client D-Bus mechanics), tidalrs, TidalSwift, and the smaller/historical projects | `references/project-profiles.md` |
| The corrected overall/auth/audio comparison tables, incl. the bus-factor caveat on the "maturity" column | `references/comparison-tables.md` |
| Reusable packaging scripts and configs with paths, Sone's actual fetched Flathub manifest (and the Flatpak/bit-perfect conflict it confirms), the update-mechanism decision per distribution channel (generalized: no project in the set ships one), the GStreamer/`libav` bundling licensing decision (now with sone-windows' concrete plugin list and AAC-decoder gap), the i18n decision, code signing/notarization (no precedent to copy), a licensing-obligation table | `references/packaging-distribution.md` |
| Project→URL mapping (incl. projects cited by URL with no local checkout, incl. Music Assistant and lms-plugin-tidal) and the domains blocked from this research environment | `references/sources.md` |
| The fact-check audit trail across **both** passes: every refuted claim with its correction (including the first pass's own errors), every new finding with its source, and the recurring "read the code, not the comment" lesson | `references/verification-notes.md` |

For streaming wire-format depth beyond what's needed to confirm an endpoint shape (full manifest
byte layout, DRM specifics, image URL construction), go to `docs/research/tidal-api.md`. For
DSP/output engineering beyond the bit-perfect negotiation summarized here (resampling theory,
buffer-sizing rationale, more platform detail), go to `docs/research/audio-pipeline.md`. For the
headless/CLI control-protocol design question (MPD-compatible vs MPRIS-first vs bespoke), go to
`docs/research/headless-connect.md`. For TIDAL's own feature set (what screens/features to build,
independent of how each reference client's code is put together), go to
`docs/research/tidal-client-features.md` and its own `tidal-client-features` skill.

## The one architectural takeaway, if you read nothing else

**Every full-quality open-source TIDAL client uses the unofficial `api.tidal.com/v1`
`playbackinfopostpaywall` endpoint** — the official API cannot give a third party full-track
audio today (confirmed twice over: `tidal-music/tidal-sdk-web` issue #133 and
`tidal-music` discussion #179, both fetched directly). This is not a preference, it's the only
path that works. **Sone is the closest architectural match to streamboat's brief** (Tauri 2 +
Rust + React, GStreamer decode, a hand-written ALSA writer for bit-perfect output) but it has one
load-bearing flaw streamboat must invert, not copy: its playback queue lives in the frontend
webview, not in the Rust core (Pitfall 6 above). **tidalt is the closest *shape* match** — a
daemon holding the audio device plus a thin TUI client talking to it over D-Bus — but has no GUI
and is Linux-only. No project in the reference set combines a GUI desktop client and a headless
daemon from one codebase; that is the gap streamboat is actually filling.

**If you need a ranked priority order, not just a flat pitfall table**: the main report's §18-Q
gives a top-six ranking of everything in this skill, in order — (1) never decrypt, (2) core owns
session/queue/transport, every UI is a subscriber, (3) platform audio split behind a trait from
commit one, (4) unofficial v1 API as the only viable playback path, (5) client IDs/secrets as
user-replaceable settings, (6) exclusive/bit-perfect output as a non-Flathub tier. Everything else
in this skill ranks below those six.

## Open decisions (feed these into any decision tree or spec-writing task)

Only the owner (thijs) can resolve these — do not assume an answer when writing code or docs:

1. **Language/runtime for the core**: Rust (matches Sone/tidalrs, best for a shared daemon +
   desktop UI + CLI) vs Python (matches High Tide/mopidy-tidal, fastest to a working client, worst
   for bit-perfect audio and packaging) vs C++/Qt (matches Strawberry, most cross-platform-mature
   audio engine, slowest to write). See `references/audio-engineering.md` §5 for what a pure-Rust
   choice specifically costs on Windows/macOS output.
2. **GStreamer vs a pure-Rust audio stack** (Symphonia + hand-written `alsa`/`wasapi`/
   `coreaudio-rs` backends, librespot-shaped). Neither choice avoids hand-written macOS
   exclusive-output work. `references/audio-engineering.md` §5-§6.
3. **Project licence**: GPL-3.0 (aligns with Sone/High Tide/Strawberry, removes friction porting
   their ideas) vs MIT/Apache-2.0 (maximizes reuse, matches tidalrs — the likeliest Rust
   dependency). `references/packaging-distribution.md` §6.
4. **Whether to ship default TIDAL client credentials at all**, and if so, whether to state
   plainly they were extracted from an official client rather than obfuscating them (Strawberry's
   honest model vs Sone's/python-tidal's obfuscation). `references/api-auth-streaming.md` §8.
5. **Whether offline caching exists at all**, and if so: ephemeral-only, persistent-encrypted-
   expiring (the official SDKs' `Offliner`/validity-window shape), or plain unencrypted files on
   disk (High Tide's actual, corrected default). `references/project-profiles.md` §1, §7.
6. **Whether to report plays back to TIDAL** (Sone does, disableable) — good citizenship vs
   telemetry to a third party.
7. **Headless mode's control protocol**: bespoke, MPD-compatible, or MPRIS-first? See
   `docs/research/headless-connect.md`; tidalt's D-Bus-only shape in
   `references/project-profiles.md` §10 is Linux-only and not directly portable.
8. **Windows/macOS exclusive-mode ambition**: WASAPI exclusive is proven twice over
   (sone-windows, Strawberry). CoreAudio hog mode has no precedent *for TIDAL* but two usable
   external precedents exist (CamillaDSP, MPD) — this is now a scoping/effort decision, not a
   research gap. `references/audio-engineering.md` §6.
9. **Update mechanism, per distribution channel** — never an in-app updater on Flatpak/Snap/AUR/
   Nix; a real decision (signed updater vs check-and-notify) only for self-contained bundles.
   `references/packaging-distribution.md` §3.
10. **Whether `concat`-based gapless can be combined with an exclusive ALSA writer** — Sone gates
    them apart and nobody in the set has tried the combination. The most novel engineering claim
    streamboat could make; prototype before committing to it in a design doc.
    `references/audio-engineering.md` §2.
11. **i18n**: adopt a mechanism now (High Tide's gettext/Meson path is nearly free) or
    consciously ship English-only (Sone's actual, undocumented default) and say so.
    `references/packaging-distribution.md` §5.
12. **Dolby Atmos / 360 Reality Audio policy**: prefer/request stereo and refuse spatial-only
    manifests with an honest message, or invest in the (currently unverified anywhere in this set)
    decode path. Metadata/quality *handling* for spatial content does have a precedent to copy
    (TidaLuna), decode does not — these are separable decisions. `references/api-auth-streaming.md`
    §9.
13. **GStreamer/`libav` bundling**: include the FFmpeg-derived `libav` plugin in a bundled
    Windows/AppImage build (more codec coverage, more licensing obligation — sone-windows'
    Windows bundle excludes it and as a result cannot decode AAC, TIDAL's HIGH/LOW tiers) or ship
    only `base`/`good`/`bad` (FLAC + DASH demux, no FFmpeg, no lossy tiers on that platform).
    `references/packaging-distribution.md` §4.
14. **Music videos**: desktop-UI-only feature on a second, browser-based media path (Sone's
    answer — a headless/daemon core cannot render video) or explicitly out of scope for now.
    `references/sone-deep-dive.md` §5.
15. **Code signing / notarization budget and timeline**: no project in the set signs anything;
    this is new cost and CI-secrets work, not something to defer implicitly by omission.
    `references/packaging-distribution.md` §7.
16. **Whether and how to reverse-engineer TIDAL's real-time streaming-privileges channel** (the
    official client has one, `player/STREAMING_PRIVILEGES_REVOKED`; no OSS project does) — or
    accept `subStatus 4006` polling as the permanent answer. `references/api-auth-streaming.md` §6.

## Unverified — do not present these as settled fact in specs or code comments

- **TIDAL's own Developer Guidelines, Developer Terms, and consumer Content Guidelines text** —
  the whole `tidal.com` domain is blocked from this research environment; every quote anywhere in
  this skill is second-hand. Read them directly, from an unproxied network, before any of it goes
  into user-facing legal text.
- Whether TIDAL's consumer Terms of Use contain a clause specifically about third-party clients
  (as opposed to the general reverse-engineering prohibition in the Content Guidelines) — same
  domain-block caveat.
- Whether the 2026-03-21 unofficial-client-ID breakage (Tidal-Media-Downloader#1213) was
  permanent, which client IDs it affected beyond one gist, or how projects recovered — confirmed
  to be one reply-less issue, not a broad or confirmed-permanent trend.
- Whether `HI_RES` (MQA) still returns anything from a live `playbackinfopostpaywall` call —
  python-tidal has dropped the enum member, Sone/Strawberry still offer it in the UI, MQA content
  was reportedly withdrawn TIDAL-wide at end of July 2024 (corroborated independently by
  tidal-connect's README). Treat `[HI_RES_LOSSLESS, LOSSLESS, HIGH]` as the more likely-correct
  cascade pending a live test.
- Whether any client has ever obtained the `playback` scope with full-track entitlement from the
  official developer portal — no evidence found either way.
- Whether `concat`-based gapless can be combined with an exclusive ALSA writer — nobody has tried
  it (see Open decision 10).
- Whether the TIDAL Connect protocol is approachable at all for a receiver or a controller — no
  open-source implementation of either exists anywhere found; the only precedent wraps a
  proprietary certificate-authenticated binary.
- Whether `playbackinfopostpaywall` accepts an audio-mode/immersive parameter for Dolby Atmos, and
  what codec an Atmos track's BTS manifest actually reports — no reference project *decodes* this
  end to end (metadata/quality *handling* for spatial tracks is precedented, in TidaLuna — see
  `references/api-auth-streaming.md` §9 — decode is not).
- Whether TIDAL's real-time streaming-privileges channel (the mechanism behind
  `player/STREAMING_PRIVILEGES_REVOKED` in the official client) can be reverse-engineered at all —
  no open-source project in this set has attempted it. `references/api-auth-streaming.md` §6.

**Resolved by the second fact-check pass (2026-09-08) — no longer unverified**: contributor
counts per project (`references/sone-deep-dive.md` §10); whether Sone emits a `track-advanced`
event (it does; `references/sone-deep-dive.md` §3); the DASH-consumption-strategy count (four, not
two; `references/api-auth-streaming.md` §5).
