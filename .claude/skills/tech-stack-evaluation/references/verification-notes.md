# Verification notes — the fact-check audit trail

**Three rounds** of independent fact-check/gap-finding passes ran against
`docs/research/tech-stack.md`: round one on 2026-09-07 (two passes, overall quality 4/5 each), round
two on 2026-09-08 (two more passes, overall quality 4.5/5 and 4/5), round three on 2026-09-08 (two more
passes, overall quality 4.5/5 and 4/5). This file is the audit trail: every refuted claim with its
correction, and the recurring lessons. All corrections listed here are already applied to
`docs/research/tech-stack.md` itself and folded into this skill's other reference files — this file
exists so a future agent can see *why* a number changed, not just that it did.

## Contents

- §1 Round-one refuted claims and their corrections
- §1b Round-two refuted claims and their corrections
- §1c Round-two gap facts folded into this skill
- §1d Round-three refuted claims and their corrections
- §1e Round-three gap facts folded into this skill
- §2 Recurring lessons
- §3 Domains blocked from the research environment
- §4 Items still marked "uncertain" — do not upgrade to fact without re-checking

---

## §1 Round-one refuted claims and their corrections

| Claim (as originally written) | Correction | Where fixed |
| --- | --- | --- |
| Strawberry's `src/` is 14,708 lines of C++/headers | **~166,122 lines** (491 `.cpp` = 121,981 lines + 568 `.h` = 44,141 lines, 40 subdirectories; `src/core` alone 21,510 lines) — an ~11x undercount that changes Strawberry from "comparable to Sone" to "by far the largest codebase in the survey" | tech-stack.md Summary, §4.11; `scoring-and-candidates.md` §2 (4.11); `real-world-players.md` |
| sone-windows' delta versus Sone is "almost entirely the audio sink" | sone-windows forks an **older** Sone release (v0.16.0: 15,753 Rust + 28,518 TS/TSX) vs. current upstream (v0.21.0: 26,721 Rust + 47,609 TS/TSX) — ~59% of upstream's current size, five minor releases behind. Not a thin per-OS delta; a stale fork. | tech-stack.md Summary; `sources.md`; `real-world-players.md` |
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
| Sone's `src/components/` has 85 components | **102 files under `src/components/`** — 101 `.tsx` (83 top-level, 17 `settings/`, 1 `signal-path/`) plus `signal-path/types.ts`; 19 of the `.tsx` are `.test.tsx`. **This skill's own correction to round two's "102 component files" figure** (round two got the file *count* right, 102, but implied all 102 were `.tsx` — `signal-path/` actually holds one `.tsx` and one `types.ts`, not two `.tsx`): applied in `sources.md` and `scoring-and-candidates.md` §5. `docs/research/tech-stack.md` itself (Summary, §4.1, §11, Sources — outside this skill, not edited here) still carries round two's slightly imprecise "102 component files" phrasing. | `sources.md`; `scoring-and-candidates.md` §5 |
| Strawberry's `gststartup.cpp` "demotes `wasapi2sink`" to `GST_RANK_SECONDARY` | Demotes **both** `wasapisink` AND `wasapi2sink` — Strawberry distrusts the whole WASAPI sink family as its shared-mode default, while still routing its own exclusive-mode code through those same sinks | tech-stack.md Summary, §2.3, §4.11, Sources; `audio-engine-comparison.md` §4; `scoring-and-candidates.md` §2 (4.11) |
| Strawberry "proves Qt/GStreamer can hit bit-perfect on all three desktop OSes" (or on "Linux and Windows") | Strawberry's `GstEngine::ExclusiveModeSupport()` returns `true` only for `wasapisink`/`wasapi2sink` — **Windows only**, via Windows-only GStreamer elements. Its Linux `exclusive_mode_` flag is a separate, weaker mechanism (`gstenginepipeline.cpp:632-637`): it fires purely because the ALSA device string starts with `hw:` **or `plughw:`** (a converting plug layer), not a dedicated bit-perfect code path — `rg -i 'bit.perfect' strawberry/src/` returns nothing, confirmed independently in `tidal-oss-landscape/references/comparison-tables.md` Table A and `verification-notes.md` item 7. Its `osxaudiosink`/CoreAudio use is device enumeration only, no hog-mode or physical-format code anywhere in `src/`. This makes Strawberry *additional* evidence for "macOS bit-perfect is unproven everywhere," not a counterexample — and it is not verified-Linux evidence either, only Windows | tech-stack.md Summary, §4.11, "Honest why not", Sources; `audio-engine-comparison.md` §8; `scoring-and-candidates.md` §2 (4.11), §4 |
| Sone "notes gapless needs GStreamer ≥1.24" | Sone's own code contradicts its README: `gapless_supported()` is `gst::ElementFactory::find("concat").is_some()`, and code comments say the `uridecodebin → queue → concat` design "works on GStreamer < 1.24" / has "no GStreamer 1.24 or uridecodebin3 requirement." The 1.24 floor applies only to the alternative `playbin3`/`about-to-finish` design (High Tide's) | tech-stack.md §2.2 table, §6 "cross-cutting"; `audio-engine-comparison.md` §3, §6 |
| "Since streamboat is open source, the [Slint] GPL-3.0 arm applies cleanly and the disclosure question is moot" | Conditional on Open decision #1 (project licence), not automatic from "open source" — GPL-3.0-only is also incompatible with MIT/Apache-2.0 distribution and GPL-2.0-only downstreams; a permissive licence choice makes the royalty-free arm's attribution requirement a live product requirement | tech-stack.md §4.2; `scoring-and-candidates.md` §2 (4.2), §3 (Rec. 2); `packaging-and-policy.md` §2 |
| "a static daemon binary is the best server artefact" (Recommendation 1 sub-decision table) | True only for the `symphonia`+per-OS-sink engine (no DASH/HLS, no EAC3/AC4); the recommended GStreamer engine links `libgstreamer`/`libglib` and loads codec plugins from a runtime registry — nothing about that artefact is static | tech-stack.md "Implications" Rec. 1 table; `scoring-and-candidates.md` §3 |
| Enforce "core has no UI-toolkit dependency" with a `wasm32-unknown-unknown` CI gate | Wrong mechanism: `rusqlite`/`sqlx` (SQLite cache) don't build for `wasm32-unknown-unknown`, and `tokio` on that target lacks `net`/`fs`/multi-thread. Use `aarch64-linux-android`/`aarch64-apple-ios` builds plus a `cargo-deny`/`cargo tree -i` gate against `tauri`/`gtk`/`winit`/`wry` instead | tech-stack.md §10; `mobile-path.md` |
| Sone keeps `tempfile` as its only dev-dependency, "thinner than ideal" (implying thin testing overall) | Sone has **151 tests (145 `#[test]` + 6 `#[tokio::test]`)** across 22 `#[cfg(test)]` modules in 16 files, all std-only — so "thin testing overall" understates it. **Correction to this file's own earlier "everything except the audio engine is unit-tested" framing, which overstated it the other way**: with no HTTP-mocking library in `[dev-dependencies]`, what's actually covered is pure decision-functions (sub-status classification, rate-gate arithmetic, scrobble thresholds, LRC parsing) across most modules — the HTTP transport layer, the manifest parsers against real payloads, and `audio.rs` all have zero coverage. Matches `tidal-oss-landscape/references/sone-deep-dive.md` §7's independent reading, which is the more accurate of the two | tech-stack.md §8; `agent-friendliness-and-testing.md` |
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
| Workspace mechanics: `rust-toolchain.toml` pinning, GStreamer 0.25-vs-Sone's-0.23 corpus drift | `tauri-engineering-facts.md` §14 |
| The agent self-verification-loop (screenshot a running Vite dev server) and Rust compile-time arguments, missing from the original agent-friendliness ranking | `agent-friendliness-and-testing.md` |
| Dioxus and Leptos/Sycamore-under-Tauri named as the direct answer to "two languages means two agent contexts," with corpus data | `agent-friendliness-and-testing.md`; `mobile-path.md` |
| Sone's own Flathub publication + automated release bot sitting close to the AI-disclosure policy's line | `packaging-and-policy.md` §3 |
| Whether GPLv3 independently closes the App Store beyond guideline 5.2.2 | `packaging-and-policy.md` §4 |
| The minimum GStreamer version exposing `wasapi2sink`'s `exclusive` property — still unresolved, flagged rather than guessed | `audio-engine-comparison.md` §4 |
| Which Rust crate would drive libmpv (`libmpv2`/`libmpv-sys`/`media_kit`) — still unnamed, flagged | `audio-engine-comparison.md` §10 |
| Per-candidate headless-daemon story is missing for everything except Tauri/Slint/Flutter | `scoring-and-candidates.md` §2 intro |

## §1d Round-three refuted claims and their corrections

| Claim (as written after round two) | Correction | Where fixed |
| --- | --- | --- |
| Cross-cutting decision: "Make gapless and bit-perfect mutually exclusive by construction, mirroring Sone's Normal-vs-DirectAlsa split" (stated as a stack-wide, cross-platform invariant) | This is a **Linux/ALSA-specific design decision in Sone, not a cross-platform law** — `sone-windows`' `wasapi2sink exclusive=true` code sits inside the same `concat`-based gapless GStreamer pipeline Sone uses on Linux (`ref:sone-windows/src-tauri/src/audio.rs:1188,1234-1247`, the file's own `// ── Normal path (unchanged) ──` branch). Gapless and exclusive mode **coexist in one pipeline on Windows**; they are two mutually exclusive backends on Linux. Model this per backend (`supports_gapless_with_exclusive() -> bool`), not as one global flag | tech-stack.md §2.2, cross-cutting decision #3; `audio-engine-comparison.md` §6 |
| Implicit framing (Summary, §2.2, §4.1): the per-OS output layer is one swappable sink behind the `AudioEngine` trait, with sone-windows as evidence "the approach compiles and runs" | The two *proven* implementations are structurally different, not two instances of one shape: Linux bit-perfect (Sone) is a hand-rolled "raw device writer" doing its own format probing/negotiation/XRUN recovery/reopen; Windows exclusive mode (`sone-windows`) is a "delegated sink" — three `set_property` calls with all negotiation delegated to the sink. Format-probing, warm-up and XRUN recovery exist only in the first kind | tech-stack.md §3 (new note under Shape 1); `architecture-shapes.md` §1; `audio-engine-comparison.md` §12 |
| "The minimum GStreamer version that exposes `wasapi2sink`'s `exclusive` property is not established" | **Resolved: `Since: 1.28`**, read directly from `gst-plugins-bad/sys/wasapi2/gstwasapi2sink.cpp` on `raw.githubusercontent.com` (the `gstreamer.freedesktop.org` docs mirror remains blocked, but the plugin source is not) | tech-stack.md §2.3 item 2, Unverified section, Sources; `audio-engine-comparison.md` §4 |
| Supersonic's GitHub release timestamp "reads 2024-08-09, which if accurate means ~2 years with no release; verify before citing it as an 'actively maintained' comparator" (round two's own correction) | **This round-two correction was itself wrong.** The v0.22.1 release page shows a date with no year suffix — GitHub omits the year only for the current year — so it is **2026-08-09**, not 2024. Corroborated by Supersonic's main-branch `go.mod`, which requires `fyne.io/fyne/v2 v2.8.0` (released 2026-07-13) via a `replace` directive pinned to a 2026-08-09 fork commit; a 2024 release cannot depend on a mid-2026 package. Supersonic is actively maintained; the repo moved to `github.com/supersonic-app/supersonic` | tech-stack.md §6, Sources; `real-world-players.md` |
| §6 comparator table and §2.2 engine table present Supersonic and Feishin as peer music clients without noting neither talks to TIDAL | Supersonic supports Subsonic/OpenSubsonic + Jellyfin only; Feishin supports Navidrome/Jellyfin/OpenSubsonic only. Neither has ever spoken to TIDAL's API — label both explicitly as **stack-and-audio-engine comparators only**, not TIDAL-client precedents | tech-stack.md §6; `real-world-players.md` |
| Strawberry `CMakeLists.txt` `QT_MIN_VERSION` branch cited as `:230-242` in one place and `:228-232` in another | Both wrong; the correct range is **`:229-233`** (`find_package` itself is at line 242) | tech-stack.md §4.11, Sources |
| Psst author's druid-architecture quote cited as live/undated 2026 evidence against the Elm/druid family (weighed against iced 0.14) | The quote is from `jpochyla`, `github.com/jpochyla/psst` discussion #359, **January 2023** — it predates iced 0.14.0's reactive-rendering rewrite (2025-12-07) by nearly three years. Date it; do not let it carry weight against iced 0.14 specifically | tech-stack.md §4.3, §6; `real-world-players.md` |

## §1e Round-three gap facts folded into this skill

| Gap | Where it lives now |
| --- | --- |
| ALSA buffer/period sizing, `start_threshold`/`avail_min` values, XRUN recovery, and writer-thread scheduling (Sone: 500ms/50ms buffer/period; tidalt: period-first, ~23ms/~93ms; neither project uses RT scheduling) | `audio-engine-comparison.md` §11 |
| Format-change (sample-rate/bit-depth) reopen mechanism (`reopen_alsa()`, `FormatChanged`/`ReopenFailed` events) and the owner decision it implies (accept the gap / plughw fallback / group the queue) | `audio-engine-comparison.md` §11; tech-stack.md §2.1, Open Question 20 |
| The "raw device writer" vs. "delegated sink" backend-shape distinction for the `AudioEngine` trait | `audio-engine-comparison.md` §12; `architecture-shapes.md` §1 |
| Headless login/pairing UX is out of scope in this skill's own coverage — cross-reference to `docs/research/headless-connect.md` §13 (terminal+QR, local web-page PKCE, pre-provisioned tokens, HTTP-exposed device-auth code) | `architecture-shapes.md` §6 |
| Crate-decomposition conflict between this report's `streamboat-player`-as-its-own-crate and `docs/research/headless-connect.md`'s `streamboat-core`-includes-the-engine / `streamboat-server` naming, plus the missing pipe output backend | `architecture-shapes.md` §1 |
| macOS CPU-architecture strategy (universal binary vs. Apple-Silicon-only) — never mentioned before this round, and it constrains the audio-backend choice (a lipo'd libmpv dylib vs. a universal GStreamer plugin tree) | `tauri-engineering-facts.md` §15 |
| System tray / close-window-behaviour sub-decision (Sone's `ksni` on Linux, no headless or cross-platform equivalent) | `tauri-engineering-facts.md` §16 |
| Multi-process OAuth token refresh race between a daemon and a GUI on the same account/machine — `tidalrs`' `on_authz_refresh_callback` as the reference pattern | `architecture-shapes.md` §4a |
| ARM is treated as daemon-only; desktop ARM targets (Windows on ARM, arm64 Linux desktop, macOS universal) are never named as a v1 target-triple decision | `architecture-shapes.md` §6 |

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
  Sone/sone-windows checked out and *still* missed that Strawberry's own GStreamer sink-ranking code
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
- **A single-platform observation stated as a stack-wide law is a recurring failure mode, not a
  one-off** — "gapless conflicts with bit-perfect in every engine" generalized from Sone's Linux
  `DirectAlsa` backend alone, and the report already had the Windows counter-evidence
  (`sone-windows`) checked out and cited elsewhere. When a design rule is derived from one backend's
  behaviour, check every other backend in the same checkout set before stating it as universal.
- **A "verify before citing" caution can itself be the error, and correcting a correction needs the
  same rigor as the original claim.** Round two flagged Supersonic's release date as possibly stale
  (misreading GitHub's no-year-suffix current-year date format as 2024); round three had to
  cross-check against `go.mod`'s dependency versions to catch it. When a date looks suspicious,
  cross-check it against something that could not exist if the date were wrong (here: a dependency
  released after the suspicious date), not just against the same source read again.
- **"Blocked in this environment" is not permanent** — the `wasapi2sink` minimum-version gap was
  carried as unresolved across two rounds because `gstreamer.freedesktop.org`'s *docs* are blocked;
  the fix was reading the plugin's own *source* on `raw.githubusercontent.com` instead of retrying the
  same blocked docs domain. When a docs site is blocked, check whether the same fact is annotated
  in-source (gtk-doc `Since:` comments, doc-comments, changelogs) before marking it permanently
  unresolved.

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
  unreachable; Sone's own choice (no component library at all) *is* verified.
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
- ~~The minimum GStreamer version exposing `wasapi2sink`'s `exclusive` property~~ — **resolved in
  round three**: `Since: 1.28`, read from the plugin source directly. See §1d above and
  `audio-engine-comparison.md` §4. Kept here as a strikethrough so a future pass doesn't waste time
  re-deriving it.
- ~~Supersonic's release-date staleness~~ — **resolved in round three, and the round-two caution was
  itself the error**: it is 2026-08-09, confirmed against `go.mod`'s dependency versions; Supersonic
  is actively maintained. See §1d above.
- **(Round two)** Which Rust crate would drive libmpv as backend #2 (`libmpv2`/`libmpv-sys`/
  `media_kit`) — not named, versioned, or licence-checked.
- **(Round two)** Whether Flathub's "AI tools/agents must not…automate Flathub submission pull
  requests" reaches an automated *release* bot updating an already-published app's lockfile sources
  (Sone's own `flathub-update.yml`) — not addressed by Flathub's own policy text either way.
- **(Round two)** Whether GPLv3's own terms independently close the App Store, beyond guideline 5.2.2
  — the FSF/VLC position was not primary-sourced in this pass.
- **(Round two)** Every "Mobile" score in the scoring matrix rests on framework documentation, not a
  shipped precedent — no shipped Tauri 2 Android/iOS app of any kind was found, and no shipped
  Flutter-with-Rust-core music player.
- **(Round two)** Whether the unofficial `api.tidal.com` surface (Sone/High Tide/python-tidal) exposes
  the Pushkin streaming-privileges websocket in the same shape as the official SDKs — the protocol
  shape itself is read directly from the official SDKs and is high-confidence; the unofficial-API
  question needs a live-account check. See `tidal-api`/`tidal-oss-landscape` skills.
