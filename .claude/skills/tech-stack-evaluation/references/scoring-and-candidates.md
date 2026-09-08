# Scoring matrix and candidate stacks, in full

## Contents

- §1 Weights and the full scoring matrix
- §2 Candidate stacks one by one (4.1–4.14, corrected)
- §3 The three recommendations and their sub-decision tables
- §4 "Honest why not" for everything not recommended
- §5 Effort-to-MVP, broken down by workstream

Full narrative and citations: `docs/research/tech-stack.md` §4–5 and "Implications for streamboat".
This file is the condensed, corrected table form for quick lookup.

---

## §1 Weights and the full scoring matrix

Weights (sum 100), justified against the brief:

| Criterion | Weight | Why |
| --- | ---: | --- |
| Audio pipeline & bit-perfect capability | 22 | The differentiating feature vs. the official client and vs. every webview wrapper. A stack that cannot do it is not a candidate. |
| Look-and-feel ceiling ("beautiful") | 18 | Owner's explicit ask. |
| AI-coding-agent friendliness | 15 | Single largest determinant of throughput given the stated build method (solo owner, heavy agent use). |
| Effort to MVP and to maintain (solo) | 15 | Second-largest throughput determinant; the main project-death risk. |
| Cross-platform desktop coverage (real parity) | 12 | Hard requirement, all three OSes now. |
| Headless/daemon fit and core sharing | 8 | Hard requirement now; the shape is well understood in every candidate. |
| Mobile path | 5 | Explicitly future; must not be precluded, need not be cheap. |
| Packaging & distribution | 5 | Solvable everywhere; differs in effort, not possibility. |

Scores 1–5, weighted (`score/5 × weight`):

| Stack | Audio 22 | Beauty 18 | Agents 15 | Effort 15 | X-plat 12 | Headless 8 | Mobile 5 | Pkg 5 | **Total** |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **Rust core + Tauri 2 + React/TS** | 5 (22.0) | 4.5 (16.2) | 5 (15.0) | 4 (12.0) | 4 (9.6) | 4 (6.4) | 4 (4.0) | 5 (5.0) | **90.2** |
| **Rust core + Flutter (flutter_rust_bridge)** | 4.5 (19.8) | 4.5 (16.2) | 4 (12.0) | 3 (9.0) | 4 (9.6) | 4.5 (7.2) | 5 (5.0) | 3.5 (3.5) | **82.3** |
| Go + Wails v3 (Rust-free) | 3.5 (15.4) | 4.5 (16.2) | 4.5 (13.5) | 4.5 (13.5) | 4 (9.6) | 5 (8.0) | 1 (1.0) | 3.5 (3.5) | **80.7** |
| **Rust core + Slint** | 5 (22.0) | 3.5 (12.6) | 2.5 (7.5) | 3.5 (10.5) | 4.5 (10.8) | 5 (8.0) | 3 (3.0) | 4 (4.0) | **78.4** |
| Rust core + egui | 5 (22.0) | 2 (7.2) | 3.5 (10.5) | 4.5 (13.5) | 4.5 (10.8) | 5 (8.0) | 1.5 (1.5) | 3.5 (3.5) | **77.0** |
| Electron + TS (+ mpv or native addon) | 2 (8.8) | 5 (18.0) | 5 (15.0) | 4.5 (13.5) | 5 (12.0) | 2.5 (4.0) | 1 (1.0) | 4.5 (4.5) | **76.8** |
| Qt 6 / QML (C++ or cxx-qt) | 5 (22.0) | 4 (14.4) | 3 (9.0) | 2.5 (7.5) | 4.5 (10.8) | 4 (6.4) | 3 (3.0) | 3 (3.0) | **76.1** |
| Go + Fyne | 3.5 (15.4) | 2 (7.2) | 4 (12.0) | 4.5 (13.5) | 4.5 (10.8) | 5 (8.0) | 3 (3.0) | 4 (4.0) | **73.9** |
| .NET + Avalonia | 2.5 (11.0) | 4 (14.4) | 3.5 (10.5) | 3.5 (10.5) | 4.5 (10.8) | 4 (6.4) | 4 (4.0) | 3.5 (3.5) | **71.1** |
| Rust core + iced | 5 (22.0) | 3 (10.8) | 2 (6.0) | 3 (9.0) | 4 (9.6) | 5 (8.0) | 1.5 (1.5) | 3.5 (3.5) | **70.4** |
| Rust + GTK4 / Relm4 | 5 (22.0) | 2.5 (9.0) | 3 (9.0) | 3.5 (10.5) | 2 (4.8) | 5 (8.0) | 1 (1.0) | 3.5 (3.5) | **67.8** |
| Kotlin MP + Compose MP | 2 (8.8) | 4.5 (16.2) | 4 (12.0) | 2.5 (7.5) | 3.5 (8.4) | 3.5 (5.6) | 5 (5.0) | 3 (3.0) | **66.5** |
| Rust + GPUI | 5 (22.0) | 4.5 (16.2) | 1.5 (4.5) | 1.5 (4.5) | 3 (7.2) | 5 (8.0) | 1 (1.0) | 3 (3.0) | **66.4** |
| Python + GTK4/libadwaita | 3.5 (15.4) | 2.5 (9.0) | 4 (12.0) | 5 (15.0) | 1.5 (3.6) | 3.5 (5.6) | 1 (1.0) | 2.5 (2.5) | **64.1** |
| Swift/SwiftUI | — | — | — | — | 1 | — | — | — | **disqualified** (no Linux/Windows) |

Scores are judgements, not measurements. The two that would most change the ranking if the owner
disagrees: "Beauty" for Slint/Avalonia/Flutter (custom-drawn UIs can be gorgeous — the 3.5–4.5 range
reflects iteration cost, not achievable quality), and "Agents" for Slint/iced (this penalises DSL
scarcity, which will improve over time).

**Two structural gaps in this matrix, identified during fact-checking:**

1. **Binary size / RAM / startup — a criterion the brief named explicitly — is dropped entirely.** No
   row above scores it. See `references/tauri-engineering-facts.md` §12 for the release-profile levers
   to use in its place, and either add it as a scored criterion (even 3-5 points, redistributed from
   Effort or Cross-platform) or say plainly it was dropped and why.
2. **No sensitivity analysis exists, so the ranking's stability is unknown.** Three Open decisions
   (mobile timing, macOS bit-perfect as v1, Linux-first vs. three-OS v1) are effectively weight
   changes to this matrix, answered narratively rather than by re-scoring. The gap between 90.2, 82.3,
   and 78.4 is small enough that one weight change could reorder the ranking. Before treating the
   ranking as final, re-score under at least two alternative weightings — e.g. Mobile 5→20 (taken from
   Effort/Beauty) to test whether Flutter overtakes Tauri+React if mobile becomes a 2026 priority, and
   Beauty 18→10 / Audio 22→30 to test whether a Rust-native toolkit overtakes it if audio purity
   matters more than visual polish. Not computed here — mechanical next step, not new research.

---

## §2 Candidate stacks one by one (corrected)

**Gap identified during fact-checking: only 4.1 fully answers "how would the headless daemon be
built, and what does it share with the GUI" — a question "headless now" makes non-optional.** 4.2 is
identical to 4.1 by construction (same `crates/`, only `desktop/` changes); the rest below give at
most a sentence (4.9: "a JVM headless daemon is a heavier server artefact"; 4.12: "adding a .NET
runtime dependency…is a regression"). Read each subsection with that gap in mind — where the daemon
shape is not spelled out, treat it as unresearched, not as "same as Tauri's."

### 4.1 Rust core + Tauri 2 + web frontend — recommended

See `references/tauri-engineering-facts.md` for the full concrete detail (capabilities/CSP, plugins,
window chrome, updater, WebView2, cross-compilation, Flathub generators, macOS packaging, mobile
crate shape, IPC). Summary: Tauri `2.11.5` (2026-07-01, Apache-2.0 OR MIT, ~29.4M downloads, MSRV
1.77.2). Webview per OS: WebView2/WKWebView/WebKitGTK. WebKitGTK is the framework's real cost — font
weight offset, glitchy rendering on maximize, DMABUF crashes — all documented and all budgetable, not
a rewrite risk. Audio is entirely outside the webview in Rust; SONE's `audio.rs` (3,309 lines) is the
smallest complete implementation of this feature set in the survey. Licensing: Tauri MIT/Apache-2.0;
React/Tailwind MIT; nothing blocks a GPL-3.0 project.

### 4.2 Rust core + Slint — recommended alternative (single language, no webview)

`slint` 1.17.1 (2026-07-07). Tri-licensed `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR
LicenseRef-Slint-Software-3.0`; royalty-free arm is free as long as you disclose Slint use (e.g. the
`AboutSlint` widget or the Slint badge). **Correction (fact-checked): this is NOT automatically moot
for an open-source project** — it is conditional on Open decision #1 (project licence), not settled
by "open source" alone. The GPL-3.0 arm applies, and the disclosure obligation disappears, only if the
owner picks GPL-3.0(-compatible) for streamboat; if the owner instead picks a permissive licence
(Apache-2.0/MIT, matching tidalt/mopidy-tidal/TIDAL's own SDKs), the royalty-free arm applies and its
visible-attribution requirement becomes a live product requirement. Treat this recommendation as
depending on Open decision #1, not assuming it. MSRV 1.92, Rust 2024 edition, renderers
femtovg/software/Skia/WGPU, ~1.63M downloads. Draws its own widgets — high design ceiling, zero
native-look anywhere, on every platform equally (which is fine for a music player). **Daemon shape**
(gap identified during fact-checking): identical to 4.1 — `crates/streamboat-daemon` is the same
binary either way, since only `desktop/` (the Slint shell replacing Tauri+React) changes. Loses to
Tauri because `.slint`'s DSL has a tiny training corpus vs. CSS/React — agents produce
plausible-but-wrong layouts more often, and the compiler catches structure, not layout intent.

### 4.3 Rust + iced — not recommended

`iced` 0.14.0 (2025-12-07, MIT, MSRV 1.88, ~2.67M downloads). 0.14 was a large release (reactive
rendering, time-travel debugging, headless testing, hot reloading, smart scrollbars); System76's
COSMIC uses it, a real maturity signal. Against: the Elm architecture forces every interaction
through one `Message` enum, verbose for a media app with dozens of concurrent async loads; the API
has churned hard across 0.9→0.14, so agent-generated code frequently targets a dead API. Psst (Rust
Spotify client, druid-based) is the cautionary tale for this whole family — its author says "most of
the complexity in the UI code is due to deficiencies of the Druid reactive architecture."

### 4.4 Rust + egui — not recommended for the primary UI

`egui` 0.36.1 (2026-08-07, MIT/Apache, ~23M downloads). Immediate-mode, extremely productive,
extremely hard to make beautiful — no retained layout, no CSS-grade styling, recognisable "tool UI"
look. Good candidate for an internal debug window (a signal-path inspector) inside whatever the real
UI is.

### 4.5 Rust + GPUI — not recommended (too early)

`gpui` 0.2.2 (2025-10-22, Apache-2.0, ~147k recent downloads). Standalone use possible; Zed proves the
framework can be genuinely beautiful and fast. But pre-1.0 with frequent breaking changes, thin docs
relative to its ambition — the worst possible pairing for a solo developer using agents.

### 4.6 Rust + GTK4/libadwaita (gtk4-rs/Relm4) — not recommended as the cross-platform shell

`relm4` 0.11.0 (2026-04-08; **licence is Apache-2.0 OR MIT, correcting an earlier "MIT" claim**). GTK
4.22.4 stable (2026-04-30). Best *integrated* Linux result in the survey (High Tide, Amberol both
genuinely good). Against, decisively: GTK's own docs call macOS/Windows support "partial", and
libadwaita is a GNOME visual API that "ignores `gtk-theme` by design" and relies on portals absent on
win32. Choosing GTK means a first-class Linux app and a visibly foreign Windows/macOS app.

### 4.7 Python + GTK4/libadwaita (the High Tide stack) — not recommended

Python (ruff `py311`), Meson, GTK4+libadwaita, 22 Blueprint files, GStreamer `playbin3`, `libsecret`,
Flatpak on `org.gnome.Platform 50`. Its Flatpak `finish-args` (`--socket=pulseaudio`,
`--filesystem=xdg-run/pipewire-0:ro`, **no `--device=all`**) mean the Flatpak build cannot reach
exclusive ALSA `hw:` — instructive for streamboat's own Flatpak sandbox design regardless of stack.
Strengths: smallest codebase in the survey (6,821 lines), very agent-friendly language. Kills it here:
Linux-only in practice, GIL/PyGObject threading makes a rigorous audio state machine painful, no
mobile path, no static headless binary, no type-checking backstop.

### 4.8 Flutter/Dart + Rust core via flutter_rust_bridge — recommended alternative if mobile is near-term

Flutter draws every pixel with Skia/Impeller — identical design ceiling on all six targets.
`flutter_rust_bridge` is the mainstream Dart↔Rust binding generator; `simple_audio` is an existing
Flutter audio package built on it. `streamboat-core`/`streamboat-player` stay Rust; Flutter is the
view layer everywhere. **Daemon shape** (gap identified during fact-checking): `streamboat-daemon` is
the same headless Rust binary as Recommendation 1 — Flutter only replaces the desktop/mobile *view*
layer; the daemon links `streamboat-core`/`streamboat-player` directly with no Dart runtime in it at
all. Against: three toolchains, thinner desktop plugin ecosystem than mobile, Flutter desktop apps
rarely feel native, and `flutter_rust_bridge` codegen conventions are not well-represented in training
data — agents get the FFI boundary wrong more often than either side.

### 4.9 Kotlin Multiplatform + Compose Multiplatform — not recommended

Compose Multiplatform for iOS went Stable with 1.8.0 (2025-05-06); 1.9.0 (2025-09-22) took Compose
for Web to Beta. Strongest mobile-first option in the list. Fails on audio: desktop Compose runs on
the JVM, where `javax.sound.sampled` is the only built-in path — no FLAC, no exclusive mode, no hw
device selection. **The specific KMP audio-library survey (gadulka/AstroPlayer/
kmp-audio-recorder-player/KorAU) is unverified** (docs.korge.org, the gadulka blog, klibs.io were not
reachable when fact-checked) — the underlying reasoning (bit-perfect on JVM desktop needs JNI to
ALSA/WASAPI/CoreAudio, i.e. writing the hard part in native code anyway) is sound regardless.

### 4.10 Electron + TypeScript — not recommended

tidal-hifi is the honest version: `"electron": "github:castlabs/electron-releases#v43.0.0+wvcus"` — a
castLabs Widevine-enabled build, because it *wraps TIDAL's web player* rather than implementing a
client. 8,011 lines of TS, MIT. If streamboat implements the API itself (the brief's actual
requirement), Widevine is unneeded, so castLabs's one advantage disappears and you are left with a
120–200 MB bundle, a Chromium process tree, and still need a native addon for the audio path — at
which point you have Tauri's architecture with 20× the bundle and no Rust core for mobile. Feishin
(Electron + React 19 + Mantine + Zustand, v1.15.1, 2026-07-19) solves this by shelling out to mpv — a
reasonable design, available to any stack. **Museeks is the other Electron-family data point, and it
already left Electron**: it ported to Tauri 2 + Rust + React (`ref:museeks` no local checkout, GitHub
`martpie/museeks`), and its own `src-tauri/Cargo.toml` has no audio crate — it keeps playback in the
webview, making it a negative control (Tauri UI without a Rust audio path is not enough).

### 4.11 Qt/QML — not recommended, but respectable

Strawberry: C++17 + Qt 6 (`QT_MIN_VERSION` 6.8.0 on Windows/macOS, 6.4.0 elsewhere), GStreamer with
`autoaudiosink`/`osxaudiosink`/`directsoundsink`/`wasapisink`, bit-perfect on Linux via ALSA
exclusive, GPL-3.0. **Its `src/` is ~166,100 lines of C++/headers, not the 14,708 an earlier count
gave** (491 `.cpp` = 121,981 lines + 568 `.h` = 44,141 lines across 40 subdirectories; `src/core`
alone is 21,510 lines) — the **largest** codebase in this entire survey, ~3.5x SONE's Rust+TS
combined. **Correction (fact-checked): it is the strongest proof point for Linux+Windows only, NOT
three-OS bit-perfect.** `GstEngine::ExclusiveModeSupport()` returns `true` only for
`wasapisink`/`wasapi2sink` (`ref:strawberry/src/engine/gstengine.cpp:523-525`); its `osxaudiosink` use
and CoreAudio calls are device *enumeration* only
(`ref:strawberry/src/engine/macosdevicefinder.cpp:27,73`) — no hog-mode or physical-format code
anywhere in `src/`. This makes Strawberry *additional* evidence for "macOS bit-perfect is unproven
everywhere" (`references/audio-engine-comparison.md` §8), not a counterexample to it. Its own Windows
GStreamer sink ranking demotes **both** `wasapisink` and `wasapi2sink` (not `wasapi2sink` alone — a
correction against an earlier draft), a direct caution against the SONE-Windows `wasapi2sink`
recommendation — see `references/audio-engine-comparison.md` §4. Against for streamboat: C++ + QML +
CMake + Qt deployment is the largest total surface of any candidate; Qt's LGPL-3.0 requires dynamic
linking (or a commercial licence); `cxx-qt` 0.10.0 (2026-08-24, MIT/Apache, KDAB) makes Rust↔Qt
interop nice but adds a fourth moving part. Agents write competent Qt Widgets, mediocre QML.

### 4.12 .NET + Avalonia — not recommended

Avalonia 12 exists as of 2026 with claimed 3× Android performance improvements (**web, unverified —
avaloniaui.net blocked when fact-checked; same confidence tier as the Cider Tauri-migration rumour**).
Draws every control itself. Blocker is the same as KMP's: no managed-runtime path to bit-perfect
audio (NAudio is Windows-only; cross-platform .NET audio means P/Invoke to the same three C APIs).
Adding a .NET runtime to a headless server binary is a regression vs. a static Rust binary.

### 4.13 Swift/SwiftUI — disqualified by the platform requirement

Best-looking option on macOS, only first-class iOS UI; TidalSwift proves a Swift TIDAL client works.
No Linux or Windows story. Keep as the *iOS* UI over a UniFFI-bound Rust core later, never as the
primary shell.

### 4.14 Go + Wails / Go + Fyne — not recommended

- **Wails v3** is still beta as of 2026 — **the latest prerelease is `v3.0.0-beta.17` (2026-09-06),
  not `beta.9`** (an earlier draft was eight betas stale — re-check `github.com/wailsapp/wails/releases`
  before relying on this). Every beta since at least beta.8 carries "the API is stable, but you may
  still encounter issues before the final 3.0 release." Same webview-per-OS model as Tauri, smaller
  ecosystem, same WebKitGTK font-weight bug.
- **Fyne 2.8** (2026-07-13) is real and improving (1,000+ commits, 39 contributors, GPU shapes,
  Wayland auto-selected on Linux, Go 1.22 minimum) but is Material-Design-by-fiat with documented UX
  friction, so the "beautiful" bar is hard to clear.
- **Go's real strength is the daemon**, and tidalt demonstrates it: clean server/client split over
  D-Bus, `docker/secrets-engine` keychain storage with an age-encrypted fallback, FFmpeg via cgo for
  decode, a hand-written ALSA writer for output. Go's audio ecosystem is cgo-to-C either way, which
  removes Go's main advantage (no C toolchain) exactly where it matters.

---

## §3 The three recommendations and their sub-decision tables

### Recommendation 1 (primary): Rust workspace + Tauri 2 desktop shell + headless daemon — score 90/100

| Decision | Choice | Rationale |
| --- | --- | --- |
| Language | Rust 2021/2024, one Cargo workspace | Bit-perfect needs direct device access; compiler is the agent's reviewer. **Correction (fact-checked): "static daemon binary" overstates it** — true only for the `symphonia`+per-OS-sink engine (no DASH/HLS fetch, no EAC3/AC4); the recommended GStreamer engine links `libgstreamer`/`libglib` and loads codec plugins from a runtime registry (SONE's deb `depends`: `libgstreamer1.0-0`, `gstreamer1.0-plugins-{base,good,bad}`, `gstreamer1.0-libav`, `gstreamer1.0-alsa`) — nothing about that artefact is static. Decide this trade explicitly, see `references/audio-engine-comparison.md` |
| Desktop shell | Tauri 2, pinned exactly (`=2.11.2` style) | Avoids surprise webview behaviour changes mid-project |
| Frontend | React 19 + TypeScript 5.8 strict | Largest agent corpus; Svelte 5 is a defensible smaller-code alternative, Solid not worth the corpus penalty |
| State | Jotai (SONE) or Zustand (Feishin) — pick one, never mix | Atom-per-concern maps well to a push-event model; SONE's 12-module `src/atoms/` split is a good template |
| Styling | Tailwind CSS 4 + CSS custom properties for themes | Themes become a token swap; shadcn/ui+Radix optional (SONE ships none, and looks good) |
| Icons | `lucide-react` | Watch the WebKitGTK `stroke-width` bug |
| Virtualisation | `@tanstack/react-virtual` | Mandatory — libraries have tens of thousands of rows |
| Video | `hls.js` in the webview, separate from the audio path | Keeps the lossless path clean; cannot handle Widevine — non-DRM HLS only, or drop video from scope |
| Audio engine | `gstreamer` 0.25 + `gstreamer-app` behind an `AudioEngine` trait; libmpv as backend #2 | See `references/audio-engine-comparison.md` |
| Media controls | `mpris-server` 0.9 + `zbus` 5 on Linux (from the engine, not the GUI); `souvlaki` 0.8.3 on Windows/macOS | souvlaki needs an HWND on Windows, a run loop on macOS — no SMTC in headless Windows mode |
| Secrets | `keyring` 3 + AES-GCM/age-encrypted file fallback | See `engineering-baseline.md` for the redaction/crypto STACK items |
| HTTP/Async | `reqwest` (0.12+, rustls) / `tokio` | Ecosystem default |
| Persistence | SQLite (`rusqlite`/`sqlx`) | mopidy-tidal's proxy cache and SONE's `cache.rs` both use it |
| Daemon protocol | JSON commands/events over WebSocket (`axum`), token-gated | Music Assistant precedent |
| Packaging | Linux: deb+rpm+AppImage+AUR+Snap+Nix flake+Flathub last; Windows: NSIS+MSI+winget; macOS: notarized DMG+Homebrew | See `references/packaging-and-policy.md` |
| Testing | `cargo test`+`wiremock`+`insta`; Vitest+Testing Library; `tauri-driver` e2e Linux/Windows only | See `references/agent-friendliness-and-testing.md` |

Risks: WebKitGTK divergence (mitigate: test Linux first, keep a workaround log); macOS bit-perfect
unproven (mitigate: scope macOS v1 to correct-sample-rate passthrough, treat exclusive mode as a
later hardware-verified feature); GStreamer Windows/macOS packaging weight (mitigate: evaluate libmpv
behind the trait before committing to an installer design); two languages means two agent contexts
(mitigate: one typed `Command`/`Event` pair, generated TS types via `ts-rs`/`specta`, never
hand-written duplicates).

### Recommendation 2 (alternative): Rust workspace + Slint desktop shell — score 78/100

Choose if the priority is one language, one toolchain, no webview, no CSS. `crates/` identical to
Recommendation 1; everything below `desktop/` changes. Slint 1.17+ under the GPL-3.0 arm **if and only
if** Open decision #1 resolves to GPL-3.0 (see §2's correction above); Skia or femtovg renderer;
`.slint` component library hand-built; theming via Slint global properties. Effort to MVP:
8–14 weeks (delta over Recommendation 1 is almost entirely UI iteration speed). Risks: agent output
quality in `.slint`; slower design iteration without CSS; weaker mobile story than Tauri's or
Flutter's.

### Recommendation 3 (alternative): Rust core + Flutter via flutter_rust_bridge — score 82/100

Choose if mobile is a 2026–2027 target rather than a someday target, and the owner accepts a slower
desktop MVP for a much cheaper mobile one. `flutter_rust_bridge` for the boundary; Riverpod or Bloc
for state; Material 3 heavily customised (Flutter's default look is the "beautiful" risk);
`media_kit` (libmpv-backed) or the Rust engine over FFI for desktop audio; `just_audio`/
ExoPlayer/AVPlayer on mobile. Effort to MVP: 10–16 weeks desktop, +4–6 weeks per mobile platform.
Risks: three toolchains; thinner desktop plugins; agents weakest exactly at the FFI boundary;
desktop Flutter apps are recognisable as such.

---

## §4 "Honest why not" for everything else

- **Electron** — best beauty/agent scores, worst audio story. Ends at Tauri's architecture with 20×
  the bundle and no Rust core to reuse on mobile, unless the owner reverses "implement the API
  ourselves" and wraps TIDAL web (which then needs castLabs Widevine again).
- **GTK4 (Rust or Python)** — nicest Linux app in the survey, foreign everywhere else, against an
  explicit three-OS requirement. Revisit only if scope narrows to Linux.
- **Qt/QML** — technically capable of a great deal, and Strawberry proves bit-perfect on **Linux and
  Windows** (166k lines). **Correction (fact-checked): not macOS** — Strawberry's
  `ExclusiveModeSupport()` covers only `wasapisink`/`wasapi2sink`; its CoreAudio use is device
  enumeration only, no hog-mode code anywhere in `src/`. Rejected on total surface area for a solo
  developer, and agent reliability in QML.
- **egui** — cannot clear the "beautiful" bar; good for a debug window.
- **iced** — good engineering, wrong ergonomics for a many-async-loads media app, API churn targets
  dead versions in agent output.
- **GPUI** — most attractive future option in Rust, least attractive present one (pre-1.0, thin docs).
- **Kotlin MP/Compose MP** — best mobile answer, no desktop bit-perfect path without native code
  anyway, heavy headless artefact.
- **.NET/Avalonia** — same audio objection as KMP, without KMP's mobile advantage over Tauri.
- **SwiftUI** — disqualified for the primary shell; right choice for a future iOS UI over UniFFI.
- **Go+Wails/Fyne** — Wails v3 still beta; Fyne's Material look fights "beautiful"; Go's audio path is
  cgo either way. Go's genuine win (a clean daemon) is available in Rust too, and tidalt's D-Bus
  server/client design is worth copying regardless of language.
- **Xilem** — alpha ("Lots of things need improvements" per its own README — not "plenty of missing
  features" as an earlier draft paraphrased it), Masonry is at 0.4.0 (2025-10-29, not an older 0.2.0),
  major breaking changes expected. Not a candidate in 2026.

---

## §5 Effort-to-MVP, broken down by workstream

**Headline number: 6–10 weeks to MVP** (Recommendation 1), solo at ~20–25 h/week with heavy agent
use. MVP = device-code login + token refresh + session/countryCode, browse home/album/artist/
playlist, search, play with quality cascade (HI_RES_LOSSLESS → HI_RES → LOSSLESS → HIGH), queue with
shuffle/repeat, MPRIS, settings, and a `.deb`. **Parity with the official client: 9–18 months**
regardless of stack — SONE took 21 minor releases to get there.

This number is calibrated unevenly — treat each part separately, not as one uniform estimate:

| Workstream | Calibration data point | Confidence |
| --- | --- | --- |
| `streamboat-core` (API client) | `ref:tidalrs/src/` — 3,545 lines for manifest/URL resolution only | Sourced, few-thousand-line order of magnitude |
| `streamboat-player` (audio engine) | SONE's `audio.rs` — 3,309 lines for the full GStreamer/ALSA bit-perfect path | Sourced |
| Combined API+audio Rust core | SONE's `tidal_api.rs`+`audio.rs` — 10,509 lines at v0.21.0 (mature, not MVP) | Sourced but represents a *mature* app, not an MVP slice |
| UI (React/TS) | SONE — 47,609 lines, 199 files, **102 component files** (correction, fact-checked: not 85 — 83 top-level `.tsx` + 17 in `settings/` + 2 in `signal-path/`, 19 are `.test.tsx`) at v0.21.0 | **Not independently calibrated for an MVP slice** — the only data point is a mature app |
| Packaging tail | See below | **Most commonly underestimated** |

The estimate explicitly **excludes**: design iteration; icon/branding; Apple Developer Program
enrollment and notarization setup; Windows code-signing setup; three separate release pipelines (no
cross-compilation — see `references/tauri-engineering-facts.md` §"Cross-compilation and CI reality");
the Flathub submission process itself; accessibility work; i18n infrastructure; any macOS
bit-perfect research spike. Re-derive per workstream before committing to a schedule, and see
`docs/research/tech-stack.md` §11 for the brief items (accessibility, i18n, frontend-framework
depth) this document defers to `engineering-baseline.md`'s own `[STACK]` checklist.
