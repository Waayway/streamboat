---
name: tech-stack-evaluation
description: The researched, fact-checked answer to "what should streamboat be built in" — language, UI toolkit, audio engine, and process architecture. Tauri 2 + Rust + React was the research recommendation, scored against 14 candidate stacks on a weighted matrix; the owner then decided on iced (no webview) with GStreamer on Linux and libmpv on Windows/macOS — see the streamboat-decisions skill, which wins over this one wherever they differ. Use this whenever choosing or reconsidering the stack; writing or reviewing `Cargo.toml`, `package.json`, `tauri.conf.json`, `src-tauri/capabilities/*.json`, a CI workflow, or a packaging script; adding a Tauri plugin, deciding a CSP, or wiring the OAuth redirect; picking an audio engine (GStreamer, libmpv, cpal, symphonia, FFmpeg); or checking whether a dependency blocks headless/server/CLI, cross-platform parity, or the future mobile path — or whenever a file mentions Tauri, WebView2, WebKitGTK, bit-perfect, exclusive mode, a UI-toolkit name, scoring matrix, MVP estimate, or "which stack"/"what language" for streamboat. Do not answer from general framework knowledge — fact-checked against 20+ reference projects to prevent plausible-but-wrong claims.
---

# Tech stack evaluation for streamboat

> **Owner decision recorded (2026-09-08, `docs/DECISIONS.md` D-008, D-013, D-016, D-045):** the stack is
> Rust with **iced** as the desktop toolkit (no webview shell), **GStreamer on Linux and libmpv on
> Windows/macOS** behind one engine trait, and two binaries (`streamboat`, `streamboatd`) over one
> core. This differs from the research recommendation below (Tauri 2 + React, GStreamer everywhere).
> Use this skill for the underlying facts (toolkit and engine properties, packaging, mobile path,
> scoring); use `streamboat-decisions` for what to build.

Source of truth: `docs/research/tech-stack.md` (the full research report — fact-checked and
corrected against **three rounds** of independent passes, ~2,000 lines). This skill is the
load-on-demand distillation for coding agents: the facts, tables, and pitfalls needed while making or
acting on the stack decision, without re-reading the whole report every time. Read the report itself
for full narrative depth, every citation, and the complete source list.

`ref:<project>/<path>` throughout this skill and its references means a shallow, read-only git clone
of a named open-source project held in the research environment (e.g.
`ref:sone/src-tauri/src/audio.rs`) — not part of the streamboat repo, and not guaranteed to still
exist in a later session. Project→URL mapping, licences, and the web-source list:
`references/sources.md`.

## Owner decisions already made (do not re-litigate these)

- **Platforms now**: desktop Linux + Windows + macOS **and** a headless/server/CLI mode, both now.
  Mobile (Android/iOS) is future scope — the architecture must not preclude it, nothing
  mobile-specific ships now.
- **Tech stance**: "something simple but beautiful" — otherwise open. This skill is the research and
  recommendation; the owner has not yet ratified a final choice.
- **TIDAL API approach**: do what High Tide and Sone do — the unofficial `api.tidal.com` API used by
  `python-tidal`, requiring the user's own paid TIDAL subscription. streamboat is a player for
  subscribers, not a downloader/ripper. Never design or document DRM circumvention or piracy tooling
  as a how-to. See the `tidal-api` skill for the API itself.
- **This skill's own recommendation** (it is what the referenced report concludes, scored 90/100 on
  the weighted matrix in `references/scoring-and-candidates.md`): **one Rust workspace, Tauri 2
  desktop shell, React 19 + TypeScript + Tailwind 4 frontend, GStreamer behind an `AudioEngine` trait
  (libmpv as backend #2), a headless daemon binary sharing the same core.** Treat this as the
  load-bearing default when writing code or docs unless the owner has explicitly chosen otherwise —
  but several sub-decisions *within* this recommendation are still open; see below.

## CRITICAL — unresolved conflict: this skill and `audio-pipeline` disagree on the primary engine

**Do not write engine-selection or `Cargo.toml` audio-dependency code from either skill alone.**
This skill (and the report it distills) recommends **GStreamer first, libmpv as backend #2**
(`references/scoring-and-candidates.md` §3, `references/architecture-shapes.md` §4); the
`audio-pipeline` skill (and its own report) recommends the opposite — **libmpv first**, GStreamer
as backend #2. The two documents previously also disagreed on mpv's own gapless flag; that part is
now fixed (`--gapless-audio=weak`, never `no` — see `references/audio-engine-comparison.md` §3),
but the engine-choice disagreement itself is real and unresolved. Also weigh: linking libmpv's
default GPLv2+ build makes streamboat a GPL work; whether that additionally conflicts with iOS App
Store distribution beyond 5.2.2's unconditional closure is `[unverified]`
(`tech-stack-evaluation/references/packaging-and-policy.md` §4) — still worth weighing, but not as
settled fact. This is directly relevant to the owner's "mobile must not be precluded" constraint,
and neither engine's off-Linux distribution is
proven in the reference set (no project links libmpv at all; no project ships GStreamer on macOS).
This is a genuine open owner decision, not a fact either skill can resolve on its own — see
`audio-pipeline/references/stacks-comparison.md` §3 for the full facts on both sides. **If you are
about to pick or scaffold the audio engine, stop and surface this to the owner (thijs) first.**

## Facts that would produce a wrong stack decision if you get them wrong

| # | Trap | The fix |
| --- | --- | --- |
| 1 | Strawberry's `src/` "is 14,708 lines" — a number from a earlier miscount (an `xargs wc -l \| tail -1` batching bug). | It is **~166,100 lines** — the largest codebase in the whole survey by a wide margin, not comparable in scope to a solo MVP. Never recount a large tree with `wc -l \| tail -1`; use `find ... -print0 \| xargs -0 cat \| wc -l`. `references/verification-notes.md` §1-2. |
| 2 | `sone-windows`'s Windows port looks like proof that "a sink swap is all a Windows port costs." | It forks an **older** Sone release (v0.16.0 vs. current v0.21.0, ~59% of upstream's size, five minor releases behind) — a stale fork, not a thin per-OS delta. Budget cross-OS parity as a standing maintenance cost. `references/sources.md`, `real-world-players.md`. |
| 3 | "`wasapi2sink exclusive=true` is verified working" reads as a settled Windows audio answer. | Strawberry's own GStreamer startup code **demotes both `wasapisink` AND `wasapi2sink`** (not `wasapi2sink` alone) and ranks `directsoundsink` primary on Windows, citing device-switching problems (issue #1227) — direct counter-evidence in a checkout this report already had open. Budget a hardware-verification pass before shipping the claim. `references/audio-engine-comparison.md` §4. |
| 4 | cpal's changelog: "0.17.2 added resampling, 0.18.0 stopped rejecting resampler-convertible formats, 0.17.0 added `hw:0,0` acceptance." | Wrong on two of three: 0.17.2 (**yanked**) added the resampling change; **0.18.2** (not 0.18.0) stopped rejecting formats; **0.18.0** (not 0.17.0) added ALSA-shorthand device-ID acceptance. `references/audio-engine-comparison.md` §3. |
| 5 | tidalt's ALSA format-preference order — which file is authoritative? | `internal/player/alsa.c`/`CLAUDE.md`, **not** `README.md` (doesn't cover it) and **not** `docs/architecture.md` (states a different, stale order). A project's own docs can contradict its own code — cite the source. `references/audio-engine-comparison.md` §7. |
| 6 | Tauri capability/command calls "just don't do anything" with no error. | Every window/command permission must be listed in `src-tauri/capabilities/*.json` or the call **silently fails**. `references/tauri-engineering-facts.md` §1. |
| 7 | "The Tauri updater plugin handles auto-update" — assumed to cover every package format. | It covers **AppImage only** on Linux, `.app.tar.gz` on macOS, MSI/NSIS on Windows — **no deb/rpm, no Snap/Flatpak**. Plan the update channel per package format. `references/tauri-engineering-facts.md` §4. |
| 8 | "`tauri build` + a CI matrix" sounds like cross-platform CI is standard practice. | `tauri build` produces artifacts **only for the host platform** — there is no cross-compilation, and Sone's own repo has **no cross-platform build workflow at all**. Budget three release pipelines. `references/tauri-engineering-facts.md` §6. |
| 9 | "Nuclear/Museeks are both Electron" music players. | **Museeks ported to Tauri 2 + Rust + React in 2024** — a second shipped Tauri music player, and a *negative* control: its `Cargo.toml` has no audio crate, so playback stays in the (non-bit-perfect) webview. `references/real-world-players.md`. |
| 10 | Flathub's AI policy is quoted as a disclosure regime with no downside for a disclosed submission. | The live doc's next sentence after "evaluated at reviewer discretion" is **"Disclosure does not create a presumption of acceptance."** Don't drop it when summarizing. `references/packaging-and-policy.md` §3. |
| 11 | "No reference project has cracked macOS bit-perfect" reads like a research gap to close before v1. | It's a structural gap in *every* engine surveyed (GStreamer's `osxaudiosink` has no hog-mode control; mpv's `coreaudio_exclusive` is undocumented-on-hardware and one user reports DAC/HDMI misrouting) — scope macOS v1 without exclusive mode and treat it as its own research spike, not a checkbox. `references/audio-engine-comparison.md` §8. |
| 12 | A version/date/licence claim sourced from a blog post or cached number. | Both version-staleness errors caught here (Wails `beta.9`→`beta.17`, Masonry `0.2.0`→`0.4.0`) were one `crates.io`/`github.com/.../releases` request away from being right. Prefer the registry API. `references/verification-notes.md` §2. |
| 13 | "Strawberry proves Qt/GStreamer can hit bit-perfect on all three desktop OSes" (or even "on Linux and Windows") — a plausible reading of "the only tri-platform GStreamer client in the survey." | Strawberry's `ExclusiveModeSupport()` covers only `wasapisink`/`wasapi2sink` — **Windows only**; its Linux "exclusive" flag is an unverified `hw:`/`plughw:` device-prefix inference (`plughw:` converts — `rg -i 'bit.perfect' strawberry/src/` finds no dedicated path), and its CoreAudio use is device enumeration, no hog-mode code anywhere. It is *additional* evidence macOS bit-perfect is unproven everywhere, not a counterexample; and it is not evidence for a verified Linux path either. `references/audio-engine-comparison.md` §8. |
| 14 | "Sone notes gapless needs GStreamer ≥1.24" — Sone's own README says so. | Sone's **code** contradicts its README: `gapless_supported()` checks for `concat` (in `coreelements` since forever) and its own comments say the design "works on GStreamer < 1.24." The 1.24 floor applies only to the alternative `playbin3`/`about-to-finish` design (High Tide's). Cite the code, not the README. `references/audio-engine-comparison.md` §3, §6. |
| 15 | **TIDAL allows only one privileged (playing) session per account — enforced over a websocket, not just by who holds the local sound device.** Easy to miss entirely, since the local device-ownership rule (§3/§4 of `architecture-shapes.md`) looks sufficient on its own. | It is not: a headless daemon and a desktop GUI **on the same account** revoke each other even on different machines with different sound cards. TIDAL's own SDKs implement this as the "Pushkin" websocket (`rt/connect`, `PRIVILEGED_SESSION_NOTIFICATION`); Sone classifies `playbackinfo` `subStatus` 4006 as this exact condition. `streamboat-proto` needs a `PlaybackRevoked` event or this fails silently. `references/architecture-shapes.md` §4a. |
| 16 | "A `wasm32-unknown-unknown` CI build proves `streamboat-core` has no UI-toolkit dependency." | It doesn't build at all — `rusqlite`/`sqlx` need a real filesystem and `tokio` on that target lacks `net`/`fs`. Use `aarch64-linux-android`/`aarch64-apple-ios` builds plus a `cargo-deny` dependency-graph gate instead. `references/mobile-path.md`. |
| 17 | "Gapless conflicts with bit-perfect in every engine" / "make gapless and bit-perfect mutually exclusive by construction" — stated as a stack-wide law. | It's a **Linux/ALSA-specific fact about Sone's `DirectAlsa` backend**, refuted by this report's own Windows evidence: `sone-windows`' `wasapi2sink exclusive=true` runs inside the *same* `concat`-based gapless pipeline Sone uses on Linux — the two coexist on Windows. Give the `AudioEngine` trait a per-backend `supports_gapless_with_exclusive()` query, not one global flag. `references/audio-engine-comparison.md` §6. |
| 18 | "sone-windows proves the `AudioEngine` trait just needs one swappable sink per OS." | The two proven backends are structurally different kinds: Linux is a hand-rolled **"raw device writer"** (own format probing, own XRUN recovery, own reopen-on-format-change); Windows exclusive mode is a **"delegated sink"** (three `set_property` calls, zero negotiation visibility). Budget the delegated-sink backends as unverified, not "done." `references/audio-engine-comparison.md` §12. |
| 19 | "The minimum GStreamer version for `wasapi2sink` exclusive mode is unknown" / Supersonic's release date "looks stale, verify before citing as maintained." | Both **resolved**: `wasapi2sink`'s `exclusive` property is `Since: 1.28` (read from plugin source, not the blocked docs site); Supersonic's "2024-08-09" reading was GitHub's no-year-suffix date format misread — the real date is 2026-08-09, confirmed against its `go.mod`. `references/audio-engine-comparison.md` §4; `references/real-world-players.md`. |
| 20 | Supersonic and Feishin cited in §6/§2.2 alongside Sone/High Tide/tidalt as if they were TIDAL-client precedents. | Neither talks to TIDAL — Supersonic is Subsonic/OpenSubsonic/Jellyfin, Feishin is Navidrome/Jellyfin/OpenSubsonic. Use both **only** as libmpv-audio-engine and Go/Electron-stack comparators, never as evidence for manifest/quality-cascade/`subStatus` behaviour. `references/real-world-players.md`. |
| 21 | A track's format/sample-rate change mid-queue is treated as an edge case to handle later. | It forces a **full ALSA device close+reopen** in bit-perfect mode (`reopen_alsa()`) — unavoidable by construction, not a bug, and it makes gapless across a format boundary impossible even before any backend-specific limit applies. Decide up front whether to accept the gap, fall back through `plughw:`, or group the queue by format. `references/audio-engine-comparison.md` §11. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| Full weighted scoring matrix, every candidate stack (4.1–4.14) evaluated with corrections, the two computed sensitivity re-scores (does Flutter overtake at Mobile weight 20? does a Rust-native toolkit overtake at Audio weight 30? — no, in both cases), the three ranked recommendations and their sub-decision tables (incl. logging/path-resolution/crypto/fuzzing/monorepo/redaction), "honest why not" for everything not chosen, the MVP-effort breakdown by workstream | `references/scoring-and-candidates.md` |
| Concrete Tauri 2 engineering facts an implementer hits immediately: capabilities ACL/CSP, plugins + the two-mechanism OAuth redirect, window-chrome cost, auto-update coverage, Windows WebView2 install-mode sizes, cross-compilation/CI reality, Flathub's two offline-source generators, macOS `bundle.macOS.frameworks` packaging hook, the mobile `crate-type` shape, the IPC large-payload trap, the full WebKitGTK divergence catalog, binary-size/RAM release-profile levers (Museeks ships them, verified), day-1 developer-environment prerequisites, workspace/toolchain-pinning mechanics | `references/tauri-engineering-facts.md` |
| **The consolidated `AudioEngine` trait definition (§0) — read this before writing the trait.** Bit-perfect requirements, Sone's ALSA/GStreamer format-naming trap, the full candidate-engine comparison table (GStreamer/libmpv/cpal/hand-rolled/FFmpeg), the DASH-manifest-as-base64-data-URI fact, the `wasapi2sink` controversy, tidalt's format-order table and its own doc's contradiction, why gapless and bit-perfect conflict in every engine, the concrete macOS CoreAudio hog-mode mechanism, device warm-up/PLL-lock delay, why macOS has no verified path anywhere, High Tide's sink-map/PipeWire-disables-gapless precedent | `references/audio-engine-comparison.md` |
**The reconciled crate layout (§1 — `streamboat-core`, `streamboat-server`, `streamboat-proto`, `streamboat-cli`, `desktop/`) — read this before creating `Cargo.toml`.** The three architecture shapes (single workspace / FFI-bound core / daemon-over-WebSocket) and the recommended hybrid, **TIDAL's account-wide streaming-privileges constraint (Pushkin) and why device ownership alone is not enough**, the remote-GUI-is-control-only invariant, MPRIS/SMTC registration rules, the headless-container Docker recipe and the arm64/Raspberry Pi build-pipeline gap, the audio-test-harness-in-CI problem | `references/architecture-shapes.md` |
| The three mobile paths ranked, the Tauri-mobile/UniFFI/Flutter tradeoffs, the core-must-not-depend-on-UI invariant, comparanda this research didn't reach | `references/mobile-path.md` |
| Packaging per platform (deb/rpm/AppImage/Snap/Flathub/AUR/Nix/MSI/NSIS/DMG/TestFlight/APK), toolkit and reference-project licences, Flathub's AI-disclosure policy quoted verbatim and corrected, Apple App Store guidelines quoted verbatim, the trademark rule | `references/packaging-and-policy.md` |
| What real music players in each candidate stack actually look like, corrected (Museeks reclassified) | `references/real-world-players.md` |
| AI-agent friendliness ranked per language/framework, testing tooling per stack, the brief items this research didn't reach (accessibility, i18n, frontend-framework depth, Symphonium/Flutter comparanda) | `references/agent-friendliness-and-testing.md` |
| Project→URL mapping, licences, web-source list, blocked domains | `references/sources.md` |
| The fact-check audit trail: every refuted claim with its correction, the recurring lessons (stale project docs, the `wc -l` batching bug, prefer registry APIs), items still marked uncertain | `references/verification-notes.md` |

## Related skills — do not duplicate their depth here

- **`audio-pipeline`** — deep pipeline internals (manifest formats, resampling theory, buffer
  sizing, per-platform mechanics beyond what's needed to *compare* engines). This skill answers
  "which engine, and why"; that one answers "how the pipeline works."
- **`headless-and-tidal-connect`** — the headless control-protocol design question in depth
  (MPD-compatible vs. bespoke vs. MPRIS-first), multiroom output, headless login/pairing UX, TIDAL
  Connect (permanently out of scope). This skill covers only the *topology* (where the engine lives)
  needed for the stack decision. The crate layout (`streamboat-core`+engine, `streamboat-server`) is
  reconciled and inlined in `references/architecture-shapes.md` §1 — read it there, not as a
  cross-skill deferral.
- **`tidal-oss-landscape`** — per-project architectural precedent across auth/streaming/audio/
  packaging for 20+ projects, and reusable packaging scripts with exact paths. This skill's packaging
  file covers the same channels from a "which channels are viable, what do they cost the stack
  decision" angle, not "how do I actually build this."
- **`tidal-client-features`** — product/feature scope (what screens and features to build),
  independent of which stack builds them.
- **`tidal-api`** — the TIDAL API wire format itself.

## The one takeaway, if you read nothing else

**The audio requirement picks the stack, not the UI requirement.** Bit-perfect output needs direct
device access (ALSA `hw:`, WASAPI exclusive, CoreAudio hog mode) that no webview, no JVM, and no
plain Electron path can reach — that disqualifies everything except a native/systems-language audio
core, whatever the UI ends up being. Sone (Tauri 2 + Rust + React + GStreamer) is the closest
existing proof this works, but it is Linux-only in practice — its Windows story is a stale AI-ported
fork, and its own GStreamer Windows sink choice is directly contradicted by Strawberry's engineering,
and **no reference project anywhere has verified macOS bit-perfect** (not even Strawberry, the only
genuinely tri-platform reference — its own code proves Windows only, via
`wasapisink`/`wasapi2sink`; its Linux "exclusive" flag is an unverified device-prefix inference,
not a dedicated bit-perfect path). Plan a per-OS `AudioEngine`
backend, plan macOS as a research spike with its own hardware-verification step (the mechanism is
CoreAudio hog mode + `kAudioStreamPropertyPhysicalFormat`, see `references/audio-engine-comparison.md`
§8), and keep `streamboat-core` (which includes the player engine — `references/architecture-shapes.md`
§1) free of any UI-toolkit dependency from day one so the mobile path stays open without a rewrite.

**Device ownership is not the only "who's allowed to play" rule — TIDAL enforces its own, at the
account level.** Only one privileged session per account is allowed, enforced over a websocket
("Pushkin" in TIDAL's own SDKs); it revokes playback across *machines*, not just across processes on
one machine. This is easy to miss because the local device-ownership design (first process claims
the sound device/D-Bus name) looks self-sufficient — it isn't. See
`references/architecture-shapes.md` §4a before building the daemon/GUI handoff.

## Cross-cutting decisions (not owner-open — do these)

Three of the report's eleven cross-cutting engineering decisions are easy to miss because they carry
no dedicated section anywhere in this skill:

- **Do not persist stream manifests or segment URLs across restarts.** High Tide caches the manifest
  in memory per session with no TTL, resetting it per track (`ref:high-tide/src/lib/
  player_object.py:140,457-491`) — no reference project stores one on disk. No invented expiry number
  is safe to hard-code; measure against a live account before assuming a duration. (Covered in depth by
  the `audio-pipeline` skill; carried here only as the one-line rule.)
- **Build the signal-path panel early.** It is Sone's most distinctive feature (probes GStreamer,
  `pactl`, and `/proc/asound`, ending in a PRISTINE verdict badge — see the Sone row in
  `references/real-world-players.md`), it is cheap once the engine is trait-shaped behind `AudioEngine`
  (`references/audio-engine-comparison.md` §0), and it is the feature that makes the bit-perfect claim
  credible to a user rather than an assertion in a settings screen.
- **Ship a `--version`-stable JSON protocol before the second client exists**, not after. Define it
  once in `streamboat-proto` (`references/architecture-shapes.md` §4) — retrofitting versioning after
  a CLI or a web remote already speaks the unversioned shape is expensive in exactly the way ~100
  component files makes the frontend `Transport`-interface decision expensive (§4).

## Open decisions

> **Resolved by the owner on 2026-09-08/09.** The items below were the inputs to the decision tree; the
> outcomes are recorded in `docs/DECISIONS.md` and distilled in the `streamboat-decisions` skill, which
> takes precedence over any recommendation here. Treat this list as history, not as open questions.

Only the owner can decide these — do not assume an answer when writing code or docs:

1. **Licence.** GPL-3.0 (matches Sone, High Tide, Strawberry; forces forks open) vs. Apache-2.0/MIT
   (matches tidalt, mopidy-tidal, TIDAL's own SDKs; permits proprietary forks) — also determines
   which Slint licence arm applies, and whether Sone/High Tide code can be adapted vs. merely read.
2. **Is macOS bit-perfect a v1 requirement or a v2 aspiration?** No reference project achieves it. If
   v1, budget CoreAudio hog-mode research and a Mac with a DAC to verify on.
3. **How much does Flathub matter?** Shapes the contribution workflow (human-authored PRs, AI
   disclosure, discretionary review) — see `references/packaging-and-policy.md` §3.
4. **Is mobile 2027-or-later, or 2026?** If near-term, Recommendation 3 (Flutter) or a UniFFI plan
   changes the desktop architecture now.
5. **Frontend framework**: React 19 (max agent reliability, more boilerplate) vs. Svelte 5 (less
   code, smaller corpus) vs. Solid vs. Vue (named in the brief, never evaluated) — reversible for
   ~2-3 weeks early, irreversible after ~100 component files. Dioxus and Leptos/Sycamore-under-Tauri
   are the direct answer to Recommendation 1's own "two languages means two agent contexts" risk;
   named, not silently rejected. `references/agent-friendliness-and-testing.md`.
6. **GStreamer everywhere vs. libmpv on Windows/macOS** — trades a large packaging burden and a
   macOS unknown against a second engine implementation and mpv's licence/dependency footprint.
7. **Does streamboat want the Sone-style extras** (MCP server, OBS overlay, Discord presence,
   Last.fm/ListenBrainz scrobbling)? ~2,500 lines of Rust in Sone; shapes the daemon's HTTP surface.
8. **Offline caching for a logged-in subscriber**: in scope or out? Changes storage design and legal
   posture, and on iOS specifically reads as a download/save feature under App Store guideline 5.2.3
   regardless of how 5.2.2's authorization question resolves. `references/packaging-and-policy.md` §4.
9. **Is v1 all three desktop OSes at once, or Linux-first with Windows/macOS best-effort?** The
   closest precedent shipped Linux first and got a stale Windows fork rather than day-one parity;
   the stack cannot cross-compile, so this is staffing/sequencing, not a config flag.
10. **Does `streamboat-server` (binary `streamboatd`) serve a browser UI / network remote, or is
    remote control MPRIS-bridge only?** Decides whether the React app may call `invoke` directly,
    or must go through a `Transport` interface picked at bootstrap from day one.
    `references/architecture-shapes.md` §4.
11. **Native window decorations, or Sone's custom-chrome route?** Custom chrome is real, recurring
    per-OS work, not a one-time cost. `references/tauri-engineering-facts.md` §3.
12. **Enforce a CSP, or copy Sone's `security.csp: null`?** A music client renders remote artwork,
    lyrics, and user text in a webview holding `invoke`. `references/tauri-engineering-facts.md` §1.
13. **Windows WebView2 install mode** — 0 MB (`downloadBootstrapper`) through +~180 MB
    (`fixedVersion`, freezes the webview build). `references/tauri-engineering-facts.md` §5.
14. **Update channel per platform**, given the Tauri updater covers only AppImage/`.app.tar.gz`/
    MSI+NSIS. `references/tauri-engineering-facts.md` §4.
15. **Minimum OS versions**: macOS floor (Tauri default 10.13), Windows 10 April-2018-update-or-later
    for a preinstalled WebView2, and the Linux glibc/WebKitGTK floor implied by the build container.
16. **Music-video scope.** `hls.js` in the webview cannot handle Widevine — non-DRM HLS only, or
    drop video from scope.
17. **Hardware/account prerequisites**: a Mac + USB DAC to verify macOS/bit-perfect claims, on top of
    the Apple Developer Program and Windows signing costs `docs/research/engineering-baseline.md` prices.
18. **Accessibility and i18n mechanism** — both tagged `[STACK]` in `docs/research/engineering-baseline.md` and
    unanswered by any research pass so far. `references/agent-friendliness-and-testing.md`.
19. **What should streamboat look like?** "Beautiful" carries 18/100 in the scoring matrix but no
    question asks what it means: native-per-OS (pushes toward Qt/Avalonia/native chrome), one
    distinctive look everywhere (CSS/Slint/Flutter — what Sone/Feishin/Supersonic actually ship), or
    best-in-class GNOME citizen first (the only branch where GTK4/libadwaita's weakness stops
    mattering)? Answer before, not after, the toolkit decision — answering late risks reopening
    Recommendation 1. `references/real-world-players.md`'s screenshot inventory makes this a concrete
    choice rather than an abstract one; `docs/research/tech-stack.md` §6 for the full narrative.
20. **Is v1 headless scope Linux/systemd + Docker only, or all three OSes?** No reference project
    answers a macOS launchd plist driving CoreAudio from a non-GUI process, or a Windows service
    (souvlaki's SMTC needs an HWND, so a Windows service gets no now-playing integration). The owner
    said "headless…now" — decide explicitly what that means given these costs.
    `references/architecture-shapes.md` §6.
21. **Format-boundary gaps in strict bit-perfect mode**: accept the audible gap at every sample-
    rate/bit-depth change (Sone's answer — reopen the device, pay the gap), fall back through a plug
    layer for non-matching tracks (tidalt's `plughw:`+`(converted)`-badge answer), or group/reorder
    the queue by format (no reference project does this)? The same class of call as the volume-lock
    and no-ReplayGain decisions already settled in §1. `references/audio-engine-comparison.md` §11.
22. **Native tray, or none — and does closing the window quit the app or keep it playing in the
    background?** The tray is the one desktop feature with no headless counterpart, and the
    close-window answer decides whether the desktop shell can ever hold the audio device while
    invisible. `references/tauri-engineering-facts.md` §16.
23. **macOS: universal binary (Intel + Apple Silicon) or Apple-Silicon-only?** A universal build
    roughly doubles build time and artifact size and requires every bundled native library
    (GStreamer or libmpv) to itself be universal — a concrete argument for libmpv on macOS beyond the
    packaging-weight case already made. `references/tauri-engineering-facts.md` §15.

## Unverified

Flagged so downstream agents do not treat these as settled fact:

- Tauri-vs-Electron RAM/binary-size numbers circulating for 2026 (75% less RAM, 20-50× smaller
  bundles, 3.7× faster startup) — SEO-blog sourced, not a reproducible benchmark; direct measurement
  was blocked (GitHub/Flathub release-asset APIs unreachable). Treat direction as right, magnitude as
  unverified.
- Cider's rumoured Electron→Tauri migration — not confirmed; sources describe it as Electron+Vue.js.
- Press-reported Flathub AI-ban timeline (2026-05-29 effective date) — secondary reporting only,
  source domains unreachable twice. `references/verification-notes.md` §4.
- macOS `coreaudio_exclusive` bit-perfect behaviour — documented by mpv, **not verified on
  hardware** by any source found; one field report describes DAC/HDMI misrouting.
- Whether cpal issue #459 was closed as "won't do", "duplicate", or "done elsewhere" — the changelog
  evidence (no exclusive mode, added resampling) is the stronger signal this skill relies on.
- KMP desktop audio library survey, shadcn/ui's Tailwind v4 support, Music Assistant's client-split
  date, GStreamer's macOS deployment page, Tauri's "not all plugins ported to mobile" claim, and
  Avalonia 12's 3× Android performance figure — all listed with full detail in
  `references/verification-notes.md` §4.
- Effort estimates are calibrated against reference-project line counts, not the owner's actual
  velocity — see the workstream breakdown in `references/scoring-and-candidates.md` §5.
- GTK 4.22.4's "partial support" wording for macOS/Windows and the X11/Broadway-deprecated-for-GTK-5
  claim; libadwaita's XDG-portal reliance — version confirmed, exact wording not read from a primary
  source (`docs.gtk.org`/GNOME Discourse blocked). `references/verification-notes.md` §4.
- ~~The minimum GStreamer version exposing `wasapi2sink`'s `exclusive` property~~ — **resolved**:
  `Since: 1.28`, read from the plugin source directly. `references/audio-engine-comparison.md` §4.
- Which Rust crate would drive libmpv as backend #2 — still unresolved, not guessed at.
  `references/audio-engine-comparison.md` §10.
- Whether a GStreamer-sink or libmpv backend correctly re-enters exclusive mode after `concat`
  renegotiates format at a gapless track boundary — no reference project tests the combination.
  `references/audio-engine-comparison.md` §6.
- Whether Flathub's automation ban reaches an already-published app's automated release bot (Sone's
  own pattern), and whether GPLv3 independently closes the App Store beyond guideline 5.2.2 — both
  unresolved. `references/packaging-and-policy.md` §3, §4.
- Every "Mobile" score in the scoring matrix rests on framework documentation, not a shipped
  precedent — no shipped Tauri 2 Android/iOS app or Flutter-with-Rust-core music player was found.
  `references/mobile-path.md`.
- Whether the unofficial `api.tidal.com` surface exposes the Pushkin streaming-privileges websocket
  in the same shape as the official SDKs — the protocol itself is high-confidence (read from the
  official SDKs directly); the unofficial-API question needs a live-account check.
  `references/architecture-shapes.md` §4a.
