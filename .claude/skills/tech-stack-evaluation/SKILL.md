---
name: tech-stack-evaluation
description: The researched, fact-checked answer to "what should streamboat be built in" — language, UI toolkit, audio engine, process architecture, and packaging path — covering Tauri 2 + Rust + React (the recommendation), Slint, Flutter/flutter_rust_bridge, GStreamer/libmpv/cpal/FFmpeg audio engines, GTK4, Qt/QML, Electron, Kotlin Multiplatform, Avalonia, Wails, Fyne, and SwiftUI, plus the weighted scoring matrix, effort-to-MVP estimates, Tauri-specific engineering facts (capabilities ACL, CSP, plugins, OAuth redirect, window chrome, auto-update coverage, WebView2 install mode, cross-compilation reality, Flathub packaging generators, mobile crate shape, IPC data-path limits), and licensing/policy constraints (Flathub's AI-disclosure policy, Apple App Store 5.2.2/5.2.1/2.5.6). Use this whenever choosing or reconsidering any part of the stack; writing or reviewing `Cargo.toml`, `package.json`, `tauri.conf.json`, `src-tauri/capabilities/*.json`, a CI workflow, or a packaging script (deb/rpm/AppImage/Snap/Flatpak/AUR/Nix/MSI/NSIS/DMG); adding a Tauri plugin, deciding a CSP, or wiring the OAuth redirect; picking or comparing an audio engine (GStreamer, libmpv, cpal, symphonia, FFmpeg/ffmpeg-next, wasapi, alsa, coreaudio-rs); estimating effort or writing a project plan/roadmap; evaluating whether a dependency or approach blocks headless/server/CLI, cross-platform desktop parity, or the future mobile path; or whenever the task or a file mentions Tauri, WebView2, WebKitGTK, WKWebView, bit-perfect, exclusive mode, wasapi2sink, GStreamer, libmpv, Slint, egui, iced, GPUI, Relm4/GTK4, Qt/QML, cxx-qt, Flutter, UniFFI, Kotlin Multiplatform/Compose, Avalonia, Wails, Fyne, SwiftUI, scoring matrix, MVP estimate, Flathub AI policy, App Store review guidelines, or "which stack"/"what language"/"what framework" for any part of streamboat. Do not answer a stack question from general "pick a cross-platform framework" knowledge — this decision has already been researched and fact-checked against 20+ reference projects' actual source, and several plausible-but-wrong claims (Strawberry's real size, cpal's changelog attributions, which project actually verified wasapi2sink, tidalt's own docs contradicting its own code) are traps this skill exists to prevent.
---

# Tech stack evaluation for streamboat

Source of truth: `docs/research/tech-stack.md` (the full research report — fact-checked and
corrected against two independent passes, ~1,430 lines). This skill is the load-on-demand
distillation for coding agents: the facts, tables, and pitfalls needed while making or acting on the
stack decision, without re-reading the whole report every time. Read the report itself for full
narrative depth, every citation, and the complete source list.

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

## Facts that would produce a wrong stack decision if you get them wrong

| # | Trap | The fix |
| --- | --- | --- |
| 1 | Strawberry's `src/` "is 14,708 lines" — a number from a earlier miscount (an `xargs wc -l \| tail -1` batching bug). | It is **~166,100 lines** — the largest codebase in the whole survey by a wide margin, not comparable in scope to a solo MVP. Never recount a large tree with `wc -l \| tail -1`; use `find ... -print0 \| xargs -0 cat \| wc -l`. `references/verification-notes.md` §1-2. |
| 2 | `sone-windows`'s Windows port looks like proof that "a sink swap is all a Windows port costs." | It forks an **older** SONE release (v0.16.0 vs. current v0.21.0, ~59% of upstream's size, five minor releases behind) — a stale fork, not a thin per-OS delta. Budget cross-OS parity as a standing maintenance cost. `references/sources.md`, `real-world-players.md`. |
| 3 | "`wasapi2sink exclusive=true` is verified working" reads as a settled Windows audio answer. | Strawberry's own GStreamer startup code **demotes `wasapi2sink`** and ranks `directsoundsink` primary on Windows, citing device-switching problems (issue #1227) — direct counter-evidence in a checkout this report already had open. Budget a hardware-verification pass before shipping the claim. `references/audio-engine-comparison.md` §4. |
| 4 | cpal's changelog: "0.17.2 added resampling, 0.18.0 stopped rejecting resampler-convertible formats, 0.17.0 added `hw:0,0` acceptance." | Wrong on two of three: 0.17.2 (**yanked**) added the resampling change; **0.18.2** (not 0.18.0) stopped rejecting formats; **0.18.0** (not 0.17.0) added ALSA-shorthand device-ID acceptance. `references/audio-engine-comparison.md` §3. |
| 5 | tidalt's ALSA format-preference order — which file is authoritative? | `internal/player/alsa.c`/`CLAUDE.md`, **not** `README.md` (doesn't cover it) and **not** `docs/architecture.md` (states a different, stale order). A project's own docs can contradict its own code — cite the source. `references/audio-engine-comparison.md` §7. |
| 6 | Tauri capability/command calls "just don't do anything" with no error. | Every window/command permission must be listed in `src-tauri/capabilities/*.json` or the call **silently fails**. `references/tauri-engineering-facts.md` §1. |
| 7 | "The Tauri updater plugin handles auto-update" — assumed to cover every package format. | It covers **AppImage only** on Linux, `.app.tar.gz` on macOS, MSI/NSIS on Windows — **no deb/rpm, no Snap/Flatpak**. Plan the update channel per package format. `references/tauri-engineering-facts.md` §4. |
| 8 | "`tauri build` + a CI matrix" sounds like cross-platform CI is standard practice. | `tauri build` produces artifacts **only for the host platform** — there is no cross-compilation, and SONE's own repo has **no cross-platform build workflow at all**. Budget three release pipelines. `references/tauri-engineering-facts.md` §6. |
| 9 | "Nuclear/Museeks are both Electron" music players. | **Museeks ported to Tauri 2 + Rust + React in 2024** — a second shipped Tauri music player, and a *negative* control: its `Cargo.toml` has no audio crate, so playback stays in the (non-bit-perfect) webview. `references/real-world-players.md`. |
| 10 | Flathub's AI policy is quoted as a disclosure regime with no downside for a disclosed submission. | The live doc's next sentence after "evaluated at reviewer discretion" is **"Disclosure does not create a presumption of acceptance."** Don't drop it when summarizing. `references/packaging-and-policy.md` §3. |
| 11 | "No reference project has cracked macOS bit-perfect" reads like a research gap to close before v1. | It's a structural gap in *every* engine surveyed (GStreamer's `osxaudiosink` has no hog-mode control; mpv's `coreaudio_exclusive` is undocumented-on-hardware and one user reports DAC/HDMI misrouting) — scope macOS v1 without exclusive mode and treat it as its own research spike, not a checkbox. `references/audio-engine-comparison.md` §8. |
| 12 | A version/date/licence claim sourced from a blog post or cached number. | Both version-staleness errors caught here (Wails `beta.9`→`beta.17`, Masonry `0.2.0`→`0.4.0`) were one `crates.io`/`github.com/.../releases` request away from being right. Prefer the registry API. `references/verification-notes.md` §2. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| Full weighted scoring matrix, every candidate stack (4.1–4.14) evaluated with corrections, the three ranked recommendations and their sub-decision tables, "honest why not" for everything not chosen, the MVP-effort breakdown by workstream | `references/scoring-and-candidates.md` |
| Concrete Tauri 2 engineering facts an implementer hits immediately: capabilities ACL/CSP, plugins + the two-mechanism OAuth redirect, window-chrome cost, auto-update coverage, Windows WebView2 install-mode sizes, cross-compilation/CI reality, Flathub's two offline-source generators, macOS `bundle.macOS.frameworks` packaging hook, the mobile `crate-type` shape, the IPC large-payload trap, the full WebKitGTK divergence catalog | `references/tauri-engineering-facts.md` |
| Bit-perfect requirements, SONE's ALSA/GStreamer format-naming trap, the full candidate-engine comparison table (GStreamer/libmpv/cpal/hand-rolled/FFmpeg), the `wasapi2sink` controversy, tidalt's format-order table and its own doc's contradiction, why gapless and bit-perfect conflict in every engine, why macOS has no verified path anywhere | `references/audio-engine-comparison.md` |
| The three architecture shapes (single workspace / FFI-bound core / daemon-over-WebSocket) and the recommended hybrid, MPRIS/SMTC registration rules, the headless-container Docker recipe, the audio-test-harness-in-CI problem | `references/architecture-shapes.md` |
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
  (MPD-compatible vs. bespoke vs. MPRIS-first), multiroom output, TIDAL Connect (permanently out of
  scope). This skill covers only the *topology* (where the engine lives) needed for the stack
  decision.
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
core, whatever the UI ends up being. SONE (Tauri 2 + Rust + React + GStreamer) is the closest
existing proof this works, but it is Linux-only in practice — its Windows story is a stale AI-ported
fork, and its own GStreamer Windows sink choice is directly contradicted by Strawberry's engineering,
and **no reference project anywhere has verified macOS bit-perfect**. Plan a per-OS `AudioEngine`
backend, plan macOS as a research spike with its own hardware-verification step, and keep the core
(`streamboat-core`/`streamboat-player`) free of any UI-toolkit dependency from day one so the mobile
path stays open without a rewrite.

## Open decisions

Only the owner can decide these — do not assume an answer when writing code or docs:

1. **Licence.** GPL-3.0 (matches SONE, High Tide, Strawberry; forces forks open) vs. Apache-2.0/MIT
   (matches tidalt, mopidy-tidal, TIDAL's own SDKs; permits proprietary forks) — also determines
   which Slint licence arm applies, and whether SONE/High Tide code can be adapted vs. merely read.
2. **Is macOS bit-perfect a v1 requirement or a v2 aspiration?** No reference project achieves it. If
   v1, budget CoreAudio hog-mode research and a Mac with a DAC to verify on.
3. **How much does Flathub matter?** Shapes the contribution workflow (human-authored PRs, AI
   disclosure, discretionary review) — see `references/packaging-and-policy.md` §3.
4. **Is mobile 2027-or-later, or 2026?** If near-term, Recommendation 3 (Flutter) or a UniFFI plan
   changes the desktop architecture now.
5. **Frontend framework**: React 19 (max agent reliability, more boilerplate) vs. Svelte 5 (less
   code, smaller corpus) — reversible for ~2-3 weeks early, irreversible after ~85 components.
6. **GStreamer everywhere vs. libmpv on Windows/macOS** — trades a large packaging burden and a
   macOS unknown against a second engine implementation and mpv's licence/dependency footprint.
7. **Does streamboat want the SONE-style extras** (MCP server, OBS overlay, Discord presence,
   Last.fm/ListenBrainz scrobbling)? ~2,500 lines of Rust in SONE; shapes the daemon's HTTP surface.
8. **Offline caching for a logged-in subscriber**: in scope or out? Changes storage design and legal
   posture.
9. **Is v1 all three desktop OSes at once, or Linux-first with Windows/macOS best-effort?** The
   closest precedent shipped Linux first and got a stale Windows fork rather than day-one parity;
   the stack cannot cross-compile, so this is staffing/sequencing, not a config flag.
10. **Does `streamboat-daemon` serve a browser UI / network remote, or is remote control MPRIS-bridge
    only?** Decides whether the React app may call `invoke` directly, or must go through a
    `Transport` interface picked at bootstrap from day one. `references/architecture-shapes.md` §4.
11. **Native window decorations, or SONE's custom-chrome route?** Custom chrome is real, recurring
    per-OS work, not a one-time cost. `references/tauri-engineering-facts.md` §3.
12. **Enforce a CSP, or copy SONE's `security.csp: null`?** A music client renders remote artwork,
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
    the Apple Developer Program and Windows signing costs `engineering-baseline.md` prices.
18. **Accessibility and i18n mechanism** — both tagged `[STACK]` in `engineering-baseline.md` and
    unanswered by any research pass so far. `references/agent-friendliness-and-testing.md`.

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
