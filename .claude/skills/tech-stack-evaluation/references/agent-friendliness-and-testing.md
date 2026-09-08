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

## Testing tooling per stack

- **Rust**: `cargo test`, `wiremock` for HTTP, `insta` for snapshot assertions on parsed manifests,
  `criterion` if needed. SONE keeps `tempfile` as its **only** dev-dependency — thinner than ideal,
  and its `audio.rs` (3,309 lines, the hardest code in the project) has zero `#[cfg(test)]` modules.
  See `references/architecture-shapes.md` §7 for the design consequence (a headless-testable
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
- **Frontend framework within Recommendation 1** (React 19 vs. Svelte 5 vs. Solid) gets one open
  question and one scoring-matrix row; it deserves its own evaluation of corpus size, bundle
  behaviour and state-management fit, since the choice is irreversible after ~85 components.
- Symphonium and named Flutter music apps (Harmony et al.), named in the brief as comparanda, were
  not reached by this research pass — no primary source checked for either.
