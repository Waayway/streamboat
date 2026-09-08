# Verification notes — the fact-check audit trail

**Two rounds** of independent fact-check/gap-finding passes ran against `docs/research/tech-stack.md`:
round one on 2026-09-07 (two passes, overall quality 4/5 each), round two on 2026-09-08 (two more
passes, overall quality 4.5/5 and 4/5). This file is the audit trail: every refuted claim with its
correction, and the recurring lessons. All corrections listed here are already applied to
`docs/research/tech-stack.md` itself and folded into this skill's other reference files — this file
exists so a future agent can see *why* a number changed, not just that it did.

## Contents

- §1 Round-one refuted claims and their corrections
- §1b Round-two refuted claims and their corrections
- §1c Round-two gap facts folded into this skill
- §2 Recurring lessons
- §3 Domains blocked from the research environment
- §4 Items still marked "uncertain" — do not upgrade to fact without re-checking

---

## §1 Round-one refuted claims and their corrections

| Claim (as originally written) | Correction | Where fixed |
| --- | --- | --- |
| Strawberry's `src/` is 14,708 lines of C++/headers | **~166,122 lines** (491 `.cpp` = 121,981 lines + 568 `.h` = 44,141 lines, 40 subdirectories; `src/core` alone 21,510 lines) — an ~11x undercount that changes Strawberry from "comparable to SONE" to "by far the largest codebase in the survey" | tech-stack.md Summary, §4.11; `scoring-and-candidates.md` §2 (4.11); `real-world-players.md` |
| sone-windows' delta versus SONE is "almost entirely the audio sink" | sone-windows forks an **older** SONE release (v0.16.0: 15,753 Rust + 28,518 TS/TSX) vs. current upstream (v0.21.0: 26,721 Rust + 47,609 TS/TSX) — ~59% of upstream's current size, five minor releases behind. Not a thin per-OS delta; a stale fork. | tech-stack.md Summary; `sources.md`; `real-world-players.md` |
| cpal 0.17.2 added resampling; **0.18.0** "output streams no longer reject formats the built-in resampler can convert"; 0.17.0 `device_by_id()` accepts `hw:0,0` | 0.17.2 (**yanked**) added the resampling change; **0.18.2** (not 0.18.0) added the format-rejection change; **0.18.0** (not 0.17.0) added ALSA shorthand (`hw:0,0`/`plughw:foo`) acceptance — 0.17.0 only introduced `device_by_id()` itself, generically | tech-stack.md Summary, §2.2 table, Sources; `audio-engine-comparison.md` §3 |
| Wails v3's latest prerelease is `v3.0.0-beta.9` | **`v3.0.0-beta.17`** (2026-09-06) — eight betas stale | tech-stack.md §4.14, Sources; `scoring-and-candidates.md` §2 (4.14) |
| Masonry's recent release is v0.2.0 | **v0.4.0** (2025-10-29); v0.2.0 was 2024-05-07, over two years stale | tech-stack.md §4/Xilem mention, Sources |
| Xilem's README says "plenty of missing features" | Verbatim wording is **"Lots of things need improvements"** | tech-stack.md, Sources |
| `relm4`/gtk4-rs licence is MIT | **Apache-2.0 OR MIT** (crates.io's actual field) | tech-stack.md §9; `packaging-and-policy.md` §2 |
| Nuclear/Museeks are both "Electron + React" | **Museeks is Tauri 2 + Rust + React**, ported from Electron in 2024; its `src-tauri/Cargo.toml` has no audio crate — playback stays in the webview, making it a *second* Tauri music-player precedent and a negative control (webview UI without a Rust audio path is not enough) | tech-stack.md §6; `real-world-players.md` |
| GStreamer Windows path is settled: `wasapi2sink exclusive=true` "verified working" | Strawberry's own `gststartup.cpp:59-69` demotes `wasapi2sink` to `GST_RANK_SECONDARY` and ranks `directsoundsink` `GST_RANK_PRIMARY`, citing device-switching issues (#1227) — direct counter-evidence the original report never surfaced despite having Strawberry checked out | tech-stack.md §2.2, §2.3, §4.11; `audio-engine-comparison.md` §4 |
| tidalt's ALSA format-preference order is documented in `README.md` | Not documented there at all — only in `alsa.c`/`CLAUDE.md`. Worse, tidalt's own `docs/architecture.md:43` states a **different, stale** 16-bit order, contradicting the code | tech-stack.md §2.1; `audio-engine-comparison.md` §7 |
| Cross-cutting decision: "TIDAL manifests expire (~1 hour) and segment URLs within ~24 hours" | **No source found for either number** anywhere in the checkouts — restated without invented figures | tech-stack.md, cross-cutting decisions |
| Flathub's live requirements doc quote omits "Disclosure does not create a presumption of acceptance" | Sentence added — it is the one that makes "Flathub remains reachable" an evidence-backed conclusion rather than an optimistic reading | tech-stack.md §9; `packaging-and-policy.md` §3 |

## §1b Round-two refuted claims and their corrections

| Claim (as written after round one) | Correction | Where fixed |
| --- | --- | --- |
| SONE's `src/components/` has 85 components | **102 component files** — 83 top-level `.tsx` + 17 in `settings/` + 2 in `signal-path/`; 19 of the 102 are `.test.tsx` | tech-stack.md Summary, §4.1, §11, Sources; `sources.md`; `scoring-and-candidates.md` §5; `tauri-engineering-facts.md` §9; `architecture-shapes.md` §4 |
| Strawberry's `gststartup.cpp` "demotes `wasapi2sink`" to `GST_RANK_SECONDARY` | Demotes **both** `wasapisink` AND `wasapi2sink` — Strawberry distrusts the whole WASAPI sink family as its shared-mode default, while still routing its own exclusive-mode code through those same sinks | tech-stack.md Summary, §2.3, §4.11, Sources; `audio-engine-comparison.md` §4; `scoring-and-candidates.md` §2 (4.11) |
| Strawberry "proves Qt/GStreamer can hit bit-perfect on all three desktop OSes" | Strawberry's `GstEngine::ExclusiveModeSupport()` returns `true` only for `wasapisink`/`wasapi2sink` — **Linux and Windows only.** Its `osxaudiosink`/CoreAudio use is device enumeration only, no hog-mode or physical-format code anywhere in `src/`. This makes Strawberry *additional* evidence for "macOS bit-perfect is unproven everywhere," not a counterexample | tech-stack.md Summary, §4.11, "Honest why not", Sources; `audio-engine-comparison.md` §8; `scoring-and-candidates.md` §2 (4.11), §4 |
| SONE "notes gapless needs GStreamer ≥1.24" | SONE's own code contradicts its README: `gapless_supported()` is `gst::ElementFactory::find("concat").is_some()`, and code comments say the `uridecodebin → queue → concat` design "works on GStreamer < 1.24" / has "no GStreamer 1.24 or uridecodebin3 requirement." The 1.24 floor applies only to the alternative `playbin3`/`about-to-finish` design (High Tide's) | tech-stack.md §2.2 table, §6 "cross-cutting"; `audio-engine-comparison.md` §3, §6 |
| "Since streamboat is open source, the [Slint] GPL-3.0 arm applies cleanly and the disclosure question is moot" | Conditional on Open decision #1 (project licence), not automatic from "open source" — GPL-3.0-only is also incompatible with MIT/Apache-2.0 distribution and GPL-2.0-only downstreams; a permissive licence choice makes the royalty-free arm's attribution requirement a live product requirement | tech-stack.md §4.2; `scoring-and-candidates.md` §2 (4.2), §3 (Rec. 2); `packaging-and-policy.md` §2 |
| "a static daemon binary is the best server artefact" (Recommendation 1 sub-decision table) | True only for the `symphonia`+per-OS-sink engine (no DASH/HLS, no EAC3/AC4); the recommended GStreamer engine links `libgstreamer`/`libglib` and loads codec plugins from a runtime registry — nothing about that artefact is static | tech-stack.md "Implications" Rec. 1 table; `scoring-and-candidates.md` §3 |
| Enforce "core has no UI-toolkit dependency" with a `wasm32-unknown-unknown` CI gate | Wrong mechanism: `rusqlite`/`sqlx` (SQLite cache) don't build for `wasm32-unknown-unknown`, and `tokio` on that target lacks `net`/`fs`/multi-thread. Use `aarch64-linux-android`/`aarch64-apple-ios` builds plus a `cargo-deny`/`cargo tree -i` gate against `tauri`/`gtk`/`winit`/`wry` instead | tech-stack.md §10; `mobile-path.md` |
| SONE keeps `tempfile` as its only dev-dependency, "thinner than ideal" (implying thin testing overall) | Literally true but misleading: SONE has 145 `#[test]` functions across 22 `#[cfg(test)]` modules in 16 files, all std-only. Accurate framing: everything **except** the audio engine is unit-tested; `audio.rs` specifically has zero tests | tech-stack.md §8; `agent-friendliness-and-testing.md` |
| Supersonic is v0.22.0 | **v0.22.1** — and the GitHub release timestamp reads 2024-08-09, which if accurate means ~2 years with no release; check before citing as "actively maintained" | tech-stack.md §6, Sources; `real-world-players.md` |
| Music Assistant exposes "a partial JSON/REST projection" | The HTTP surface is documented upstream as **JSON-RPC-over-HTTP**, not REST — "partial JSON/REST" overstates its RESTfulness | tech-stack.md Summary, §3; `architecture-shapes.md` §3 |

## §1c Round-two gap facts folded into this skill

Round two's gap-finding pass supplied several facts with primary sources that were missing from
round one entirely — these are now load-bearing content in this skill, not just corrections:

| Gap | Where it lives now |
| --- | --- |
| The concrete macOS bit-perfect mechanism (`AudioObjectSetPropertyData`/`kAudioDevicePropertyHogMode`, `kAudioStreamPropertyPhysicalFormat`, `coreaudio-rs`/`coreaudio-sys`) — named for the first time | `audio-engine-comparison.md` §8 |
| Strawberry's defensive `g_object_class_find_property(sink_class, "exclusive")` idiom and its `device_warmup_duration_ms` warmup-pending/generation state machine | `audio-engine-comparison.md` §4, §9 |
| TIDAL's account-wide "Pushkin" streaming-privileges websocket — an entirely new subsystem this skill did not cover before | `architecture-shapes.md` §4a |
| The remote-GUI-is-control-only invariant | `architecture-shapes.md` §4b |
| arm64/Raspberry Pi daemon build-pipeline gap | `architecture-shapes.md` §6 |
| Binary size/RAM/startup release-profile levers, and the criterion being dropped from the scoring matrix | `tauri-engineering-facts.md` §12; `scoring-and-candidates.md` §1 |
| Day-1 developer-environment prerequisites, incl. the Windows VBSCRIPT/MSI trap | `tauri-engineering-facts.md` §13 |
| Workspace mechanics: `rust-toolchain.toml` pinning, GStreamer 0.25-vs-SONE's-0.23 corpus drift | `tauri-engineering-facts.md` §14 |
| The agent self-verification-loop (screenshot a running Vite dev server) and Rust compile-time arguments, missing from the original agent-friendliness ranking | `agent-friendliness-and-testing.md` |
| Dioxus and Leptos/Sycamore-under-Tauri named as the direct answer to "two languages means two agent contexts," with corpus data | `agent-friendliness-and-testing.md`; `mobile-path.md` |
| SONE's own Flathub publication + automated release bot sitting close to the AI-disclosure policy's line | `packaging-and-policy.md` §3 |
| Whether GPLv3 independently closes the App Store beyond guideline 5.2.2 | `packaging-and-policy.md` §4 |
| The minimum GStreamer version exposing `wasapi2sink`'s `exclusive` property — still unresolved, flagged rather than guessed | `audio-engine-comparison.md` §4 |
| Which Rust crate would drive libmpv (`libmpv2`/`libmpv-sys`/`media_kit`) — still unnamed, flagged | `audio-engine-comparison.md` §10 |
| Per-candidate headless-daemon story is missing for everything except Tauri/Slint/Flutter | `scoring-and-candidates.md` §2 intro |

## §2 Recurring lessons

- **A project's own docs can be stale relative to its own code.** tidalt's `docs/architecture.md`
  disagrees with `internal/player/alsa.c` on the ALSA format order. When a project's README/docs and
  its source disagree, cite the source, and say so explicitly — don't silently pick one.
- **`xargs wc -l | tail -1` silently truncates on a large file list.** This is very likely how the
  original Strawberry count went wrong: when the argument list is large enough that `xargs` batches
  it into multiple `wc -l` invocations, each batch prints its own "total" line, and piping through
  `tail -1` shows only the *last* batch's total — not the grand total. Use
  `find ... -print0 | xargs -0 cat | wc -l` (or `find ... -exec cat {} +  | wc -l`) instead when
  recounting a large tree.
- **A registry API beats a blog post for version claims.** Both version-staleness refutations here
  (Wails, Masonry) were one `crates.io`/`github.com/.../releases` request away from being caught.
  Prefer `https://crates.io/api/v1/crates/<name>` and `https://github.com/<org>/<repo>/releases`
  over a cached blog citation for anything version/date/licence-shaped.
- **Being the reference project doesn't make a claim self-evidently true.** The original report had
  SONE/sone-windows checked out and *still* missed that Strawberry's own GStreamer sink-ranking code
  directly contradicts the `wasapi2sink`-exclusive recommendation. Cross-check a "verified" claim
  against every other checkout that touches the same subsystem, not just the one that motivated it.
- **A specific-sounding number is not automatically sourced.** The "~1 hour manifest / ~24 hour
  segment" figures read as precise engineering knowledge but had no citation anywhere in the
  checkouts. When a number can't be traced to a specific line, state the qualitative rule instead of
  inventing a plausible magnitude.
- **"Open source" and "GPL-3.0" are not the same fact, even when GPL-3.0 is the likely licence.**
  Round two's Slint "moot" error is the general form of a mistake worth watching for: don't let a
  probable future decision (Open decision #1) get silently baked into a present-tense claim.
- **A criterion the brief explicitly named (binary size/RAM/startup) can still get dropped from a
  scoring matrix without anyone noticing** if nothing forces a check against the original brief
  line-by-line. Re-read the brief's criteria list against the final matrix's column headers before
  treating a matrix as complete.
- **An architectural constraint can be missing entirely, not just miscounted** — TIDAL's account-wide
  streaming-privileges websocket (Pushkin) was not in round one's report at all, despite being
  directly relevant to the recommended device-ownership hybrid and present in every official SDK
  checked out. When a design claims "process A owns the resource, everyone else is a client," check
  whether the *external* system (here, TIDAL's own account-level session model) enforces a
  contradictory or complementary rule before treating the local design as complete.

## §3 Domains blocked from the research environment

`v2.tauri.app`, `gstreamer.freedesktop.org`, `ui.shadcn.com`, `apps.gnome.org`, `docs.gtk.org`,
`mozilla.github.io`, `webdriver.io`, `avaloniaui.net`, `fyne.io`, `slint.dev`, `gamingonlinux.com`,
`linuxiac.com`, `opensourceforu.com`, and the entire `tidal.com` domain all returned blocked or
failed fetches at some point during this research (both the original pass and the fact-check pass).
Where the primary page is blocked, this skill relies on GitHub raw-file mirrors of the same content
where they exist (this worked for every `tauri-apps/tauri-docs` page cited in
`tauri-engineering-facts.md`), or flags the claim as "web, unverified" when no mirror exists.

## §4 Items still marked "uncertain" — do not upgrade to fact without re-checking

- Press-reported Flathub AI-ban timeline (2026-05-29 effective date, non-retroactivity) — secondary
  reporting only, source domains unreachable twice.
- KMP desktop audio library survey (gadulka/AstroPlayer/kmp-audio-recorder-player/KorAU as "simple
  wrappers") — underlying reasoning is sound, specific library list unverified.
- shadcn/ui's Tailwind v4/React 19 support and the `agmmnn/tauri-ui` scaffold — `ui.shadcn.com`
  unreachable; SONE's own choice (no component library at all) *is* verified.
- Music Assistant client package's "split out in October 2024" — no date found in either README.
- GStreamer's macOS deployment page (PackageMaker-style bundle) — domain blocked, Windows deployment
  page was read directly but not the macOS one.
- Tauri's "not all desktop plugins ported to mobile" and the macOS notarization mechanism — rest on
  the docs' own maturity language and secondary summaries (`v2.tauri.app` blocked), not a page read
  directly, unlike the updater/WebView2/macOS-bundle/IPC/mobile-crate facts in
  `tauri-engineering-facts.md`, which were read from GitHub raw mirrors and are higher-confidence.
- Avalonia 12's claimed 3× Android performance improvement — same confidence tier as the (already
  flagged) Cider Tauri-migration rumour.
- **(Round two)** GTK 4.22.4's "partial support" wording for macOS/Windows, and the X11/Broadway
  deprecation-for-GTK-5 claim — version confirmed via secondary sources, exact wording not read from
  `docs.gtk.org` (blocked).
- **(Round two)** libadwaita's reliance on XDG portals and "ignores `gtk-theme` by design" — consistent
  with well-known libadwaita design, not read from a primary GNOME Discourse source in this pass.
- **(Round two)** Two of the four WebKitGTK divergence items ("blur during CSS animations,"
  `contenteditable` spans not behaving as inputs) — the other two (font-weight #14286, maximize glitch
  #13157) are read directly from the issues; these two rest on Tauri's linux-graphics doc, which is
  blocked in this environment.
- **(Round two)** The minimum GStreamer version exposing `wasapi2sink`'s `exclusive` property —
  unresolved; `gstreamer.freedesktop.org` is blocked and neither `sone-windows` nor Strawberry pins
  one. Copy Strawberry's defensive property-check pattern in the meantime.
- **(Round two)** Which Rust crate would drive libmpv as backend #2 (`libmpv2`/`libmpv-sys`/
  `media_kit`) — not named, versioned, or licence-checked.
- **(Round two)** Whether Flathub's "AI tools/agents must not…automate Flathub submission pull
  requests" reaches an automated *release* bot updating an already-published app's lockfile sources
  (SONE's own `flathub-update.yml`) — not addressed by Flathub's own policy text either way.
- **(Round two)** Whether GPLv3's own terms independently close the App Store, beyond guideline 5.2.2
  — the FSF/VLC position was not primary-sourced in this pass.
- **(Round two)** Every "Mobile" score in the scoring matrix rests on framework documentation, not a
  shipped precedent — no shipped Tauri 2 Android/iOS app of any kind was found, and no shipped
  Flutter-with-Rust-core music player.
- **(Round two)** Whether the unofficial `api.tidal.com` surface (SONE/High Tide/python-tidal) exposes
  the Pushkin streaming-privileges websocket in the same shape as the official SDKs — the protocol
  shape itself is read directly from the official SDKs and is high-confidence; the unofficial-API
  question needs a live-account check. See `tidal-api`/`tidal-oss-landscape` skills.
