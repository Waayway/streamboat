# AI-coding-agent friendliness, and testing per stack

The build method (solo owner, heavy agent use) is treated as a first-class selection criterion, not
a footnote — two reference projects are explicit about being built this way: `ref:tidalt/README.md`
states it was written entirely with LLM coding assistants; `ref:sone-windows/README.md` says "Ported
with help from AI agents." Both are Rust or Go with strong type systems — the Windows port in
particular is a mechanical, type-guided port of a Rust codebase.

## Ranked by expected agent-produced-correct-code rate

1. **TypeScript/React/Tailwind** — largest corpus by a wide margin; `tsc` + ESLint + Prettier catch a
   large class of errors mechanically; component boundaries are small enough to regenerate.
2. **Rust** — smaller corpus than TS but the compiler is the strongest available reviewer, and
   `cargo clippy -- -D warnings` plus `cargo fmt --check` gives an agent a hard, fast feedback loop.
   tidalt and sone-windows were both built this way.
3. **Python** — huge corpus, no type backstop by default. Fine for scripts, risky for a threaded
   GStreamer state machine.
4. **Go** — good corpus, simple language, strong tooling; the audio work still lands in cgo/C.
5. **Dart/Flutter** — good corpus for widgets, poor for `flutter_rust_bridge` boundary code.
6. **Kotlin/Compose** — good corpus for Android Compose, thinner for Compose Desktop.
7. **C++/Qt/QML** — good corpus for Qt Widgets, weaker for modern QML idioms; the build system is a
   frequent agent failure point.
8. **`.slint`, iced, GPUI DSLs/APIs** — thin corpora, fast API churn. Agents produce confident,
   compiling-but-wrong layouts.

**Two mechanical factors matter as much as corpus mass in practice, and the ranking above omits both
— gap identified during fact-checking.** First, a **self-verification loop**: a web frontend runs
against the Vite dev server in a plain browser, with no Rust build, no webview process, and no audio
hardware in the loop, so an agent can drive it with a headless browser and screenshot its own work
end-to-end. Slint, iced, egui, GTK, and Flutter-desktop give an agent no equivalent — screenshotting
them requires building and running the real app, and on CI that means a display server (Xvfb or
similar) at minimum. This is arguably the strongest concrete argument for Recommendation 1 and was
absent from the original report; read the "Agents" 5/5 score for Tauri+React in
`references/scoring-and-candidates.md` §1 as reflecting this loop, not corpus size alone. Second, the
counterweight: **Rust incremental compile time** is the dominant latency in an agent loop that runs
`cargo check`/`cargo clippy` after every edit — budget for compile-time tooling (a shared `sccache`
cache at minimum, and consider a faster linker: `mold`/`lld`) as part of "Effort to MVP," not as an
afterthought. A slow `cargo check` loop is a real tax on every Rust-native candidate, not just the
recommended one.

## Testing tooling per stack

- **Rust**: `cargo test`, `wiremock` for HTTP, `insta` for snapshot assertions on parsed manifests,
  `criterion` if needed. **Correction (fact-checked), fuller picture than "thin testing":** SONE's
  `[dev-dependencies]` is indeed just `tempfile = "3"`, but that is not the whole test story — SONE
  has 145 `#[test]` functions across 22 `#[cfg(test)]` modules in 16 files, all on std-only tooling
  (no mocking framework, no snapshot crate), covering `tidal_api.rs` (playback-info sub-status
  classification), `pipeline_probe.rs`, `rate_gate.rs`,
  `scrobble/{lastfm,listenbrainz,musicbrainz}.rs`, `mcp/sanitizer.rs`, `logging.rs`, `theme_config.rs`,
  `commands/{playback,updates}.rs`. The accurate framing: **everything except the audio engine is
  unit-tested**; `audio.rs` (3,309 lines, the hardest code in the project) specifically has zero
  `#[cfg(test)]` modules. That split — not a blanket "thin testing" verdict — is the template worth
  copying. See `references/architecture-shapes.md` §7 for the design consequence (a headless-testable
  `AudioEngine` backend).
- **Frontend**: Vitest + `@testing-library/react` + jsdom is exactly SONE's setup (`vitest` 4.1.7,
  `@testing-library/react` 16.3.2, `jsdom` 29.1.1), with `knip` for dead-code detection. SONE has
  test files colocated with components (`Header.test.tsx`, `PlayerBar.test.tsx`,
  `NowPlayingDrawer.escape.test.tsx`, `MediaCard.explicit.test.tsx`, `HomeSection.compactGrid.test.tsx`).
- **E2E**: `tauri-driver` works on **Windows and Linux only** — "macOS not having a WKWebView driver
  tool available." WebdriverIO's Tauri service covers macOS by running an embedded WebDriver server
  inside the app. Plan e2e on Linux CI and treat macOS as manual — including a manual three-webview
  visual pass per release (see `references/tauri-engineering-facts.md` §11).
- **Audio engine specifically**: tidalt's `internal/player/alsa_fallback_test.go` (68 lines,
  `plughw:` fallback) is the only real precedent for testing an audio decision in CI without
  hardware. Full detail in `references/architecture-shapes.md` §7.

## Brief items not covered by this research pass

- **Accessibility per candidate** has no row anywhere in the main report, though
  `engineering-baseline.md:1234,1253` tags both the screen-reader approach and the automated a11y
  gate **[STACK]** — deferred there, unanswered here too. For the recommended stack: ARIA plus the
  platform webview's accessibility tree (Tauri/Electron), versus AccessKit (egui/iced/Slint), AT-SPI
  (GTK), or UIA/NSAccessibility (Qt/Avalonia). Custom window chrome (see
  `references/tauri-engineering-facts.md` §3) means keyboard focus management and ARIA on the custom
  titlebar/player controls is real, uncalibrated work.
- **i18n mechanism** is likewise tagged **[STACK]** at `engineering-baseline.md:1403` and unanswered:
  react-i18next/Fluent/ICU MessageFormat for the recommended stack vs. gettext for a GTK alternative.
- **Frontend framework within Recommendation 1** (React 19 vs. Svelte 5 vs. Solid vs. Vue — Vue is
  named in the brief and never evaluated beyond describing Cider) gets one open question and one
  scoring-matrix row; it deserves its own evaluation of corpus size, bundle behaviour, and
  state-management fit, since the choice is irreversible after ~100 component files. Two Rust-only
  alternatives worth naming even though rejected, because they answer Recommendation 1's own "two
  languages means two agent contexts" risk: **Dioxus** (RSX, single Rust codebase on the same
  `wry`/`tao` webview stack Tauri uses, desktop + Android/iOS — `dioxus` 0.7.10 current stable, MIT OR
  Apache-2.0, ~2.57M total downloads, https://crates.io/api/v1/crates/dioxus fetched 2026-09-08) and
  Leptos/Sycamore under Tauri. Both sit in the same corpus-scarcity band as Slint/iced (compare
  `tauri`'s ~29.4M downloads) — the honest reason to reject them, not omission.
- Symphonium and named Flutter music apps (Harmony et al.), named in the brief as comparanda, were
  not reached by this research pass — no primary source checked for either. Every "Mobile" score in
  `references/scoring-and-candidates.md` §1 also rests on framework documentation, not a shipped
  precedent: no shipped Tauri 2 Android/iOS app of any kind was found, and no shipped
  Flutter-with-Rust-core music player — only production Tauri *desktop* apps (Spacedrive, AppFlowy,
  Clash Verge) and one third-party mobile audio plugin with ~1,544 downloads. Treat the Mobile column
  as documentation-based and unevidenced until one shipped example of each is found.
