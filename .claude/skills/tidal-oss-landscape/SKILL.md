---
name: tidal-oss-landscape
description: Architectural precedent from 20+ open-source TIDAL clients and SDKs — Sone, High Tide, Strawberry, tidal-hifi, TidaLuna, mopidy-tidal, tidalt, tidalrs, python-tidal, tidal-connect, and the official tidal-sdk-web/android/ios — covering auth, stream resolution, manifest/DASH/BTS parsing, bit-perfect/exclusive audio output, gapless playback, quality cascades, caching, credential handling, packaging, and the desktop+headless architecture split. Use whenever writing or reviewing code touching TIDAL auth/login, playbackinfo/manifest/stream-URL resolution, the audio pipeline (GStreamer, ALSA, bit-perfect, exclusive mode, gapless, ReplayGain), the core/daemon-vs-UI split, credential/token storage, packaging (Flatpak/Snap/AUR/deb/rpm/Nix/Windows), or update mechanisms — or when a task mentions playbackinfo, manifest, DASH, BTS, audioquality, WASAPI, hog mode, subStatus, or client ID/secret. Do not answer from general music-app or GStreamer/audio-API knowledge — TIDAL's unofficial API and these clients' hard-won choices differ from generic assumptions in ways that cause silent bugs (see the pitfalls table below).
---

# TIDAL open-source landscape for streamboat

Source of truth: `docs/research/oss-landscape.md` (the full research
report, corrected against **three** independent fact-check passes — read it for narrative depth
and the complete source list). This skill is the load-on-demand distillation: the facts,
endpoints, code pointers, and pitfalls an implementer needs while writing code, without
re-reading a 3,300-line report every time. If a fact here ever looks inconsistent with something
you remember reading, the audit trail of what each fact-check pass corrected (including two
passes' own errors) is in `references/verification-notes.md` §1b/§1c.

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
  hypothetical, TidaLuna does exactly this and it must never be ported). **Writing the actual
  legal/README text**: every reference project that streams TIDAL this way carries its own
  non-affiliation + subscription-required disclaimer (High Tide, Sone, tidalrs, Fokka-Engineering
  — quoted verbatim with sources in `references/legal-posture.md`); reuse that framing, and add
  the two things none of them state explicitly — the Content Guidelines reverse-engineering
  prohibition, and that account suspension is a real user-facing risk.

## Facts that cause silent bugs if you get them wrong

| # | Trap | The fix |
| --- | --- | --- |
| 1 | TidaLuna contains a hardcoded AES key that decrypts `OLD_AES`-flagged streams. Copying it (or reimplementing the same idea) turns streamboat into DRM-circumvention tooling, which the project brief explicitly rules out. | **Never decrypt.** Refuse the stream when the manifest's `encryptionType` is non-empty and not `"NONE"`, when a top-level `encryptionKey` is non-empty, or when `securityType` is non-empty, not `"NONE"`, and paired with a non-empty `securityToken` (ref:strawberry/src/tidal/tidalstreamurlrequest.cpp:244-247,298,303-307). **`encryptionType: "NONE"` is the normal case for an ordinary BTS manifest — do not refuse it.** Message shape: Strawberry's ("depends on the client ID in use, try changing it in settings"). `references/project-profiles.md` §4, §2. |
| 2 | A `subStatus` in `4000..=4999` on a 401 from `playbackinfo` looks like an auth error but **is not** — refreshing the token does nothing. Only `4005,4010,4030,4031,4032,4034,4035` are terminal (skip the track); `4033` is deliberately *not* in that list (subscription up-sell, user-fixable) and neither is `4006` (recovers on its own). | Copy the literal set, never a range. `references/api-auth-streaming.md` §6. |
| 3 | `HI_RES_LOSSLESS` (enum) vs `HIRES_LOSSLESS` (metadata tag) — no underscore before RES on the tag. String-equality checks across the two vocabularies silently fail. | Always use a lookup table between the `Quality` enum and `MediaMetadataTags`. `references/api-auth-streaming.md` §7. |
| 4 | ALSA `S24LE` (24-in-32, 4 bytes) = GStreamer `S24_32LE`; ALSA `S243LE` (packed, 3 bytes) = GStreamer `S24LE`. The names are inverted between the two libraries. Getting this backwards is a silent audio-corruption bug, not a crash. Canonical table: `audio-pipeline/references/output-backends.md` §1. | `references/audio-engineering.md` §1. |
| 5 | High Tide's on-disk track cache is **not** an opt-in "music folder" feature — `MUSIC_DIR` is unconditional, there is no setting to disable it, and it's inside the project the owner named as a model. | Make offline caching a deliberate, documented streamboat decision (`SKILL.md` Open decisions #5), not an inherited default. `references/project-profiles.md` §1. |
| 6 | Sone's queue, shuffle, repeat, and history live in the **React webview**, not in Rust — the backend only persists a snapshot and mirrors frontend state out to MCP/overlay. This is *why* Sone-windows is a stale fork rather than a portable core. | streamboat's core must own session/queue/transport; every UI is a thin subscriber — invert Sone's shape, don't copy it. `references/sone-deep-dive.md` §3. |
| 7 | `cpal` (the obvious pure-Rust audio crate) **cannot do WASAPI exclusive mode at all** — there is no flag or feature for it. | If going pure-Rust, budget a hand-written `wasapi`-crate Windows backend (same class of work as GStreamer's own missing macOS-exclusive backend). `references/audio-engineering.md` §5. |
| 8 | **Correction, previously stated backwards**: Sone's own Flathub manifest grants `--socket=pulseaudio`, and that socket grant alone already covers `/dev/snd` (Flatpak's sandbox helper binds it whenever the PulseAudio socket is requested) — its exclusive/bit-perfect ALSA writer *can* run in the build Flathub ships. There is no `--device=all` need for raw ALSA. | Do not gate bit-perfect output on the packaging channel. The Snap channel is the one that needs an explicit step (`snap connect sone:alsa`). Owned by `streamboat-engineering-baseline/references/packaging-and-distribution.md` §1; see `references/packaging-distribution.md` §2 here. |
| 9 | `[unverified]` The unofficial stream endpoint (`playbackinfopostpaywall`) over-requesting quality is claimed to return **200 with a silently downgraded `audioQuality`, never an error** — but this rests on a single Sone code comment with no second source and no captured live response, and it's in tension with tidalt shipping a descending quality ladder (redundant if downgrades are always silent). | Do not assume the downgrade-not-error behaviour without observing it directly — one request per quality tier against a live account settles it. Until then, still always display the response's `audioQuality` rather than the requested one; only the stop-the-cascade rule is what's uncertain. `references/api-auth-streaming.md` §7. |
| 10 | Enabling EBU R128 loudness normalization in a GStreamer pipeline forces the downstream chain to float caps (`F32LE`/`F64LE`) — this is **structurally incompatible** with bit-perfect integer output, not just "should be off by default." | Any normalization stage must be bypassable, not merely defaulted off. `references/audio-engineering.md` §4. |
| 11 | The entire `tidal.com` domain (not just `developer.tidal.com`/`support.tidal.com`) is blocked from this research environment — every quote from TIDAL's own policy text anywhere in this skill is second-hand. | Never cite these quotes as primary-sourced in user-facing legal text without a direct, unproxied re-read first. `references/verification-notes.md` §4. |
| 12 | mopidy-tidal's "login hack" QR code (the cleverest headless-auth idea in the set) sends the one-time TIDAL login URL to a third-party host (`api.qrserver.com`) to render it. | Generate QR codes locally if adapting this pattern — tidalt's `mdp/qrterminal` is the pattern to copy instead. `references/api-auth-streaming.md` §3. |
| 13 | A seek that detaches the armed gapless next-track slot (the "obvious" implementation) destroys a valid preroll and produces an audible gap — Sone tried this and reverted it. | Seek must leave the inactive prerolled `concat` branch alone; only the active branch gets flushed. `references/sone-deep-dive.md` §3a. |
| 14 | Gapless *arming* logic that gates on `isPlaying` will silently disable itself during ALSA device-busy retries, because `isPlaying` flickers false during those retries and on every pause. | Gate arming on `gapless && !exclusiveMode && !bitPerfect && !currentVideo && currentTrack` — never on `isPlaying`. `references/sone-deep-dive.md` §3b. |
| 15 | "Sone has 151 tests" reads as a test harness for the hard parts. It isn't one — `[dev-dependencies]` is one line (`tempfile`), and all 151 tests cover pure functions only; the HTTP layer, manifest parsers, and ALSA path have zero automated coverage. | Design a parser/transport split plus captured fixtures as new work — no project in this reference set demonstrates that pattern. `references/sone-deep-dive.md` §7. |
| 16 | Every project in this landscape except High Tide and mopidy-tidal — Strawberry included — is effectively bus-factor 1. "Very mature, very active" (Strawberry) is not the same signal as "safe to depend heavily on." | Read "maturity" as age + release cadence + CI, not community size, when deciding how much to lean on any one reference project. `references/sone-deep-dive.md` §10. |
| 17 | No project in the reference set signs its Windows/macOS builds or ships a signed in-app updater — including tidal-hifi, which has the best release CI in the set. | Budget code signing/notarization and decide the update mechanism per distribution channel as new work from day one; there is nothing to copy. `references/packaging-distribution.md` §3, §7. |
| 18 | Playlist/favorites mutations on the unofficial API silently require an ETag captured from a *prior GET of the same resource*, sent back as `If-None-Match` — skip this and every write returns an inexplicable 412/428. | GET before every mutation, cache the `etag` (default `"*"` if absent), send it back on the write. Confirmed independently in two implementations. `references/sone-deep-dive.md` §4b. |
| 19 | A queue modeled as one flat list makes shuffle-unshuffle, "play next," "playing from" attribution, and album-vs-track ReplayGain unimplementable later without a rewrite. | Model it as five collections from the first schema: context queue, pre-shuffle original order, a separate manual "play next" list, history, and two distinct source refs (playing vs. browsing). `references/sone-deep-dive.md` §3d. |
| 20 | The explicit-content filter is not a screen-level checkbox — it has to be checked at every single enqueue/play/autoplay/shuffle call site (ten of them in the one reference implementation), or content leaks through whichever path forgot the check. | Enforce it once, in the core's queue-mutation path, not per UI action handler. `references/sone-deep-dive.md` §3e. |
| 21 | Sone's playback position is a single 2 s backend anchor poll, interpolated locally by every consumer — not N independent per-consumer pollers. Copying an N-pollers design wastes IPC and still needs the settle-window guard below, which only a single-anchor design can express cleanly. | Copy the real pattern: one backend anchor poll, local interpolation everywhere, 1 s push heartbeats to secondary windows, and a settle-window guard around gapless track-change boundaries (a freshly polled position that jumps implausibly far ahead of the interpolated value in the few seconds after a track change is discarded, not trusted). `references/sone-deep-dive.md` §3 (position model). |
| 22 | Sone-windows is described in some places as "Windows (+Linux/mac via Tauri)" — the macOS half is false; there is no macOS `#[cfg]` arm anywhere in that fork, and `souvlaki` (SMTC) is declared Windows-only. | Do not budget macOS output, media integration, or GStreamer bundling as free side effects of a cross-platform UI framework — four of five macOS subsystems are from-scratch. `references/packaging-distribution.md` §8. |
| 23 | Play-reporting to TIDAL ("Recently Played") is not a plain JSON POST — it's an AWS SQS `SendMessageBatch` form-encoded call whose per-event `Headers` attribute pins a *specific Android device identity* (app version, OS, device model), because events must describe the client whose token they ride on. | Budget play reporting as maintaining a device-identity pin that drifts roughly as often as TIDAL ships a release, or decide not to report plays at all — there is no visible middle ground. `references/sone-deep-dive.md` §5 (play reporting). |
| 24 | A "half the track or 4 minutes" scrobble rule sounds simple until it's implemented against wall-clock position instead of *cumulative playtime* — a user who seeks backward and replays a section will never reach the threshold under a naive position check. It also silently scrobbles interludes and skits if you drop the length guard that comes first. | Tracks of 30 s or less never scrobble; otherwise scrobble when `cumulative_playtime >= duration/2 \|\| cumulative_playtime >= 240s`. Drive the threshold from accumulated playback time, confirmed identical in two independent implementations (a reference client and the official one). `references/sone-deep-dive.md` §4d. |
| 25 | Gapless arming resolves the next track's stream URL as soon as it's predicted — but stream URLs and manifests expire in minutes to an hour, and a user can pause for longer than that before the armed slot is consumed. No project in this reference set handles the resulting mid-session expiry. | Design an explicit re-resolve-on-resume policy and a mid-track-403/410 recovery path before shipping gapless + long-pause support; do not assume this is solved because gapless itself is. `references/audio-engineering.md` §9. |

**Design deliberately, do not simply avoid — mopidy-tidal's login-hack pattern (the cleverest
headless-auth idea in the set) makes not one but *two* undisclosed third-party calls: a QR-code
render against `api.qrserver.com` (Pitfall 12) and, added by the third fact-check pass, a
text-to-speech call against `api.voicerss.org` with a hardcoded API key. Drop both if borrowing
the pattern. `references/project-profiles.md` §5.**

## Where to go for depth

| Question | Reference file |
| --- | --- |
| Official vs unofficial API surfaces, auth flows (device code, PKCE, three redirect-capture strategies, python-tidal's incomplete poll loop), stream-resolution endpoints and params, manifest formats (BTS/DASH/EMU/HLS, four DASH delivery strategies), the `subStatus` taxonomy (incl. the streaming-privileges/`4006`/Pushkin gap), quality-cascade stopping rules, credential-handling risk profiles, the Dolby Atmos/`SONY_360RA` gap, the sharpened `playback`-scope-vs-`r_usr` evidence, ETag read-revalidation, and a pointer to the track-availability/error-classification policy | `references/api-auth-streaming.md` |
| Bit-perfect ALSA negotiation (format/rate probing, promotion table, S24 naming inversion, hw/sw params), three gapless designs compared, seeking's interaction with `concat` gapless, DASH caps handling, three normalization formulas (incl. why EBU R128 breaks bit-perfect), the pure-Rust-vs-GStreamer question (librespot, cpal, wasapi), macOS CoreAudio hog-mode precedents outside this reference set (plus the full five-item macOS work list), tidalt's ALSA refinements (incl. release-on-pause) and combined device hot-plug/busy/lost policy, volume curves, mid-session stream-URL/manifest expiry (unsolved) | `references/audio-engineering.md` |
| Sone's full stack, module map, IPC design and the state-ownership inversion to avoid (incl. the corrected anchor-poll-plus-interpolate position model and its settle-window guard), seeking, the gapless prefetch/arming policy, track-availability pre-flight + error classification, the five-collection queue data model, autoplay + explicit-content filter, navigation + scroll restoration, caching/crypto/settings, `countryCode` bootstrap + log redaction, the playlist/favorites-mutation ETag precondition, the rate-limit contract's actual numbers, scrobble threshold, play-reporting wire format (incl. the Android-identity impersonation), music videos, packaging+CI, code quality, borrow/avoid list, the Sone-windows fork (corrected: Windows-only, no macOS arm), bus factor/contributor counts | `references/sone-deep-dive.md` |
| Per-project deep dives for everything else: High Tide, Strawberry, tidal-hifi, TidaLuna (incl. the official Redux action-namespace dump and Atmos/360RA handling), mopidy-tidal (incl. its two undisclosed third-party calls), Music Assistant, lms-plugin-tidal, tidal-connect, the three official SDKs, python-tidal, tidal-cli, tidalt (incl. its daemon/client D-Bus mechanics), tidalrs (corrected), TidalSwift, the smaller/historical projects, headless precedents named but never fetched (upmpdcli, spotifyd, ncspot, psst, go-librespot), and module maps for every project that lacked one | `references/project-profiles.md` |
| The corrected overall/auth/audio comparison tables (incl. the Sone-windows macOS correction), and the bus-factor caveat on the "maturity" column | `references/comparison-tables.md` |
| Reusable packaging scripts and configs with paths, Sone's actual fetched Flathub manifest (and the Flatpak/bit-perfect conflict it confirms), the update-mechanism decision per distribution channel (generalized: no project in the set ships one), the GStreamer/`libav` bundling licensing decision (now with Sone-windows' concrete plugin list and AAC-decoder gap), the i18n decision, code signing/notarization (no precedent to copy), a licensing-obligation table (incl. the licence-vs-audio-module sequencing decision), and the full five-item macOS work list | `references/packaging-distribution.md` |
| Project→URL mapping (incl. projects cited by URL with no local checkout, incl. Music Assistant, lms-plugin-tidal, upmpdcli, and the librespot frontend ecosystem) and the domains blocked from this research environment | `references/sources.md` |
| The fact-check audit trail across **all three** passes: every refuted claim with its correction (including each pass's own errors), every new finding with its source, and the recurring "read the code, not the comment" lesson | `references/verification-notes.md` |
| Material for streamboat's README/legal-doc positioning: the four reference projects' own non-affiliation/subscription-required disclaimer text, the Content Guidelines reverse-engineering prohibition, the account-suspension risk none of them states explicitly, and the second-hand-sourcing caveat that must travel with all of it | `references/legal-posture.md` |

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

**Three more recommendations worth acting on early, not just avoiding pitfalls on:**
- **Separate auth/credentials, catalogue/API, and the playback engine as three modules from
  commit one**, the way the official SDKs do (`CredentialsProvider`-shaped auth; the auth module
  must not know about playback; the player depends on an interface, not a session singleton).
  This is what makes headless mode and the future mobile target cheap instead of a rewrite.
  `references/project-profiles.md` §7.
- **Ship a local HTTP/JSON integration surface early**: token-gated, loopback-bound, default-off.
  This is what the community actually built on top of (tidal-hifi's local Express API, Sone's MCP
  server and OBS overlay) — cheap to add now, expensive to retrofit once external tools depend on
  scraping the UI instead. `references/project-profiles.md` §3, `references/sone-deep-dive.md` §5.
- **Signal-path transparency is a cheap differentiator once the pipeline probes exist**: show the
  user the decoded format, every conversion, and what the device actually received, the way
  Sone's `signal_path.rs`/`pipeline_probe.rs` do. `references/sone-deep-dive.md` §2, §8.

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
3. **Project licence — decide this before writing the audio module, not at release time.**
   GPL-3.0 (aligns with Sone/High Tide/Strawberry, permits porting Sone's ALSA negotiation
   wholesale — the single largest specialist-effort item in this skill) vs MIT/Apache-2.0
   (maximizes reuse, matches tidalrs — the likeliest Rust dependency — but forces a clean-room
   reimplementation of the bit-perfect audio path from this skill's own documented behaviour;
   budget that as real weeks, not a footnote). `references/packaging-distribution.md` §6.
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
   (Sone-windows, Strawberry). CoreAudio hog mode has no precedent *for TIDAL* but two usable
   external precedents exist (CamillaDSP, MPD) — this is now a scoping/effort decision, not a
   research gap. `references/audio-engineering.md` §6. **Budget macOS as a whole, not just
   output**: four of five macOS subsystems (output, media integration, code signing, GStreamer
   bundling) have zero precedent anywhere in this reference set — including inside Sone-windows,
   which has no macOS `#[cfg]` arm at all. `references/packaging-distribution.md` §8.
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
    Windows/AppImage build (more codec coverage, more licensing obligation — Sone-windows'
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
17. **UI toolkit — platform-native (GTK4/Qt) vs a webview (Tauri/React) — added by the third
    fact-check pass.** This is a theming-and-accessibility trade-off as much as a "simple but
    beautiful" visual call: platform-native buys HIG consistency and AT-SPI/UIA accessibility for
    free with no runtime theming (High Tide's answer); a webview buys full visual control,
    including cheap theming from two seed colours (Sone's `theme.json`), at the cost of owning
    every pixel of the accessibility and i18n work a native toolkit would otherwise provide. No
    comparative screenshot-level UX assessment of the reference clients exists — this is a design
    call, not something further source-reading resolves. `references/project-profiles.md` §15 is
    the closest available comparative material (screen/component inventories per project).
18. **Mid-session stream-URL/manifest expiry policy — added by the third fact-check pass, and
    genuinely unsolved anywhere in the reference set.** Decide whether an armed gapless-next-track
    slot gets re-resolved after a pause exceeding some threshold, or unconditionally on resume;
    and whether a mid-track 403/410 means halt-and-fail or re-resolve-and-reseek (which the
    bit-perfect frame-counted position path must support if chosen). `references/audio-engineering.md`
    §9.

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
  official developer portal — no evidence found either way, but **sharper evidence, added by the
  third fact-check pass**: `tidal-cli` requests `playback` among ten official scopes and still
  surfaces a non-empty `previewReason` at runtime; SDK issue #133 names `r_usr playback` together
  as "required scopes," and `r_usr` is specifically the scope the *unofficial* flows request —
  raising the question of whether `r_usr` itself, not `playback`, is the real gate. Needs a live
  test against a registered developer client. `references/api-auth-streaming.md` §1.
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
- **Mid-session stream-URL/manifest expiry recovery — added by the third fact-check pass; not a
  documentation gap, a genuine open engineering question.** No project in the reference set
  re-resolves an armed gapless-next-track slot after a long pause, or defines what a mid-track
  403/410 should mean once a stream URL may simply have expired rather than gone permanently
  unplayable. `references/audio-engineering.md` §9.
- **Not yet fetched, only named — added by the third fact-check pass**: `medoc92/upmpdcli`'s
  TIDAL renderer plugin, and the librespot frontend ecosystem (`spotifyd`, `ncspot`, `psst`,
  `go-librespot`). Flagged as the highest-value follow-up reads for the control-protocol and
  language/runtime open decisions, not evaluated here. `references/project-profiles.md` §14.

**Resolved by the second fact-check pass (2026-09-08) — no longer unverified**: contributor
counts per project (`references/sone-deep-dive.md` §10); whether Sone emits a `track-advanced`
event (it does; `references/sone-deep-dive.md` §3); the DASH-consumption-strategy count (four, not
two; `references/api-auth-streaming.md` §5).

**Resolved by the third fact-check pass (2026-09-08) — no longer unverified**: Sone's actual
position model (anchor-poll-plus-interpolate with a settle-window guard, not "N independent
pollers" — `references/sone-deep-dive.md` §3); whether Sone-windows covers macOS (it does not, no
`#[cfg]` arm exists — `references/sone-deep-dive.md` §9); the playlist/favorites-mutation
precondition (an ETag `If-None-Match`, confirmed in two implementations —
`references/sone-deep-dive.md` §4b); the queue data model (five collections, not one list —
`references/sone-deep-dive.md` §3d); the rate-limit contract's actual numbers
(`references/sone-deep-dive.md` §4c); the scrobble threshold rule (`references/sone-deep-dive.md`
§4d); the play-reporting wire format's true transport and identity (AWS SQS `SendMessageBatch`,
impersonating a pinned Android device — `references/sone-deep-dive.md` §5).
