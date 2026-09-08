# Tauri 2 engineering facts for the recommended stack

## Contents

- §1 Capabilities ACL and CSP
- §2 Plugins, and the two-mechanism OAuth redirect
- §3 Window chrome is a hidden cost of "beautiful"
- §4 Auto-update coverage is partial
- §5 Windows WebView2 install mode
- §6 Cross-compilation does not exist; there is no reference build CI
- §7 Flathub packaging needs two offline-source generators
- §8 macOS packaging hook for the audio backend
- §9 Mobile: the app crate must be a library from day one
- §10 IPC has a large-library data-path trap
- §11 WebKitGTK divergence catalog
- §12 Binary size / RAM / startup: the levers, since the brief named this criterion
- §13 Day-1 developer-environment cost, and one Windows trip hazard
- §14 Workspace mechanics: `rust-toolchain.toml`, the pnpm+cargo coexistence, GStreamer version drift
- §15 macOS CPU-architecture strategy: universal binary vs. Apple-Silicon-only
- §16 System tray / close-window behaviour has no sub-decision row

These are the facts an implementer hits in roughly the first week of a Tauri 2 + Rust + React
project, gathered because `docs/research/tech-stack.md`'s original pass under-covered them (they
came out of the fact-check gap-finding pass, not the original research). Primary sources are GitHub
raw-file mirrors of `tauri-apps/tauri-docs` (readable through this environment's proxy even though
`v2.tauri.app` itself is blocked) plus Sone's actual checked-out config.

---

## §1 Capabilities ACL and CSP

Tauri 2's biggest change from v1: every window/command permission must be explicitly listed in
`src-tauri/capabilities/*.json` or the call **silently fails** — no exception, no obvious error in
the console. Design and test against this from day one, not after the first "why doesn't this
button work" debugging session.

Sone ships one capabilities file (`ref:sone/src-tauri/capabilities/default.json`):

- identifier `default`, `windows: ["main", "miniplayer"]`
- `core:default`
- `core:window:allow-{set-fullscreen, create, set-always-on-top, close, minimize, unminimize,
  maximize, unmaximize, set-size, start-dragging, start-resize-dragging, show, set-focus}`
- `core:webview:allow-create-webview-window`
- `opener:default`
- `global-shortcut:allow-register`
- `deep-link:default`

Sone also sets `security.csp: null` in `tauri.conf.json` — **no content policy at all**. Decide
deliberately: a real CSP (`img-src` limited to the TIDAL CDN hosts plus `asset:`/`tauri:`, no inline
scripts) versus copying Sone's open posture knowingly. This matters specifically for a music client:
it renders remote artwork, lyrics, and user-authored playlist/track names inside a webview that
holds `invoke` — that is a security decision, not a formality.

## §2 Plugins, and the two-mechanism OAuth redirect

Sone's plugin set (`ref:sone/src-tauri/Cargo.toml`), all version-**pinned exactly** with `=` — a
convention worth copying to avoid surprise webview/plugin behaviour changes mid-project:

| Plugin | Pinned version |
| --- | --- |
| `tauri-plugin-single-instance` (with `deep-link` feature) | `=2.4.2` |
| `tauri-plugin-deep-link` | `=2.4.9` |
| `tauri-plugin-opener` | `=2.5.4` |
| `tauri-plugin-window-state` | `=2.4.1` |
| `tauri-plugin-oauth` | `=2.0.0` |
| `tauri-plugin-global-shortcut` | `=2.3.1` |

`tauri.conf.json` registers the deep-link scheme `tidal` for desktop. **The desktop OAuth flow runs
two mechanisms together**: a loopback HTTP listener (`tauri-plugin-oauth`) for the redirect, and a
`tidal://` custom-scheme handler routed through single-instance for the case where the OS hands the
redirect to a URL scheme instead. tidalt independently registers the same `tidal://` scheme with XDG
(`tidalt setup`, `ref:tidalt/docs/client-server.md:64`). Decide which mechanism (or both) streamboat
needs before building the login screen — it is cheaper to decide once than to support both by
accident.

## §3 Window chrome is a hidden cost of "beautiful"

Sone's window config: `decorations: false`, `visible: false` until first paint (avoids a white
flash), 1200×800, plus a second `miniplayer` window. Turning decorations off moves the titlebar, drag
regions, resize handles, and (per OS) macOS traffic lights / Windows Snap Layouts into your own code —
this is *why* its capability file needs the whole `core:window:allow-start-dragging`/
`allow-start-resize-dragging`/`allow-minimize`/etc. list in §1. Decide early: native decorations
(cheap, native feel, constrained design) versus custom chrome (Sone's route, real recurring per-OS
polish work — not a one-time cost).

## §4 Auto-update coverage is partial

Tauri v2's updater plugin signs a static JSON manifest (minisign keypair via `tauri signer generate`,
private key via env var — `.env` files do not work) keyed by platform
(`linux-x86_64`/`darwin-aarch64`/`windows-x86_64`) with per-platform `url` + `signature`; TLS is
enforced in production.

**Bundle coverage, exactly**: Linux **AppImage only** (`.AppImage`, `.AppImage.tar.gz`), macOS
`.app.tar.gz`, Windows `.msi` and NSIS `.exe`. There is **no updater for deb/rpm** (those need a
hosted repo — Sone hosts apt/dnf/zypper on Cloudsmith) and **none for Snap/Flatpak** (store-managed
updates instead). Plan the update channel per package format, not as one in-app mechanism — and note
this is consistent with `docs/research/engineering-baseline.md`'s own recommendation of no silent in-app updater
initially, now backed by the actual coverage table.

Source: `https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/plugin/updater.mdx`

## §5 Windows WebView2 install mode

`bundle.windows.webviewInstallMode` is a required config choice trading installer size against
offline support:

| Mode | Size delta | Notes |
| --- | --- | --- |
| `downloadBootstrapper` (default) | +0 MB | Needs network; weak Win7 `.msi` support |
| `embedBootstrapper` | +~1.8 MB | Better Win7 `.msi` support |
| `offlineInstaller` | +~127 MB | Installs with no internet |
| `fixedVersion` | +~180 MB | Pins a specific WebView2 build inside the app — the **only** lever that freezes webview-divergence risk on Windows |
| `skip` | +0 MB | App will not run without the runtime already present |

Windows 10 (April 2018 update or later) and Windows 11 ship the WebView2 runtime with the OS, so
`downloadBootstrapper` is mainly a first-run risk on older or offline machines. Pick deliberately —
this is one of the Open decisions listed in the main report and in `SKILL.md`.

Source: `https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/distribute/windows-installer.mdx` (lines ~187–323)

## §6 Cross-compilation does not exist; there is no reference build CI

`tauri build` produces artifacts **only for the host platform**. Museeks' own README states this
plainly: "Tauri does not support cross-platform binaries, so the command will only generate binaries
for your current platform." Sone's repo has **no cross-platform build workflow at all** —
`.github/workflows/` holds only `flathub-update.yml` — and releases come from local per-distro Docker
builds:

- `build-scripts/build/Dockerfile.deb` = `FROM ubuntu:22.04` (chosen for the oldest glibc/WebKitGTK
  floor)
- `Dockerfile.rpm` = `FROM fedora:42`
- plus `Dockerfile.pacman`, `Dockerfile.rpm-opensuse`, a `PKGBUILD`, and a `build-scripts/test/` suite
  that installs each package to verify it

Windows support in that ecosystem exists only as a separate fork (`sone-windows`). **Budget three
release pipelines** (Linux/Windows/macOS build hosts, three signing setups) rather than one CI matrix
that cross-compiles — this changes the "one GitHub Actions matrix" assumption a casual reading of the
report might leave you with.

## §7 Flathub packaging needs two offline-source generators, regenerated every release

Sone's release automation (`ref:sone/.github/workflows/flathub-update.yml:95-139`) runs:

```
python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py sone/src-tauri/Cargo.lock -o flathub/cargo-sources.json
flatpak-node-generator pnpm --pnpm-store-version v11 sone/pnpm-lock.yaml -o flathub/pnpm-sources.json
```

then commits the manifest plus both sources files into the separate Flathub repo. This depends on
pnpm specifically (`ref:sone/pnpm-workspace.yaml`) — swap generator if streamboat uses npm/yarn. This
workflow is itself an **automated commit to a Flathub-adjacent repo** — decide explicitly what a
release bot may commit versus what the human owner must author, given the Flathub AI-disclosure
policy in `references/packaging-and-policy.md`.

## §8 macOS packaging hook for the audio backend

Tauri's bundler supports `bundle.macOS.frameworks`, accepting system frameworks, custom `.framework`
bundles, and `.dylib` files — the config schema's own example lists `"CoreAudio"` alongside a custom
dylib and a custom framework. This is the hook for shipping `GStreamer.framework` or `libmpv`.
`bundle.macOS.minimumSystemVersion` defaults to macOS 10.13 and should be raised deliberately (12.0+
is defensible). Entitlements are applied at signing time via `bundle.macOS.entitlements` — this is
where a Hardened Runtime exception for the media backend would go.

Source: `https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/distribute/macos-application-bundle.mdx` (lines ~120–180)

## §9 Mobile: the app crate must be a library from day one

The concrete shape "do not preclude mobile" implies, and it is cheap now, expensive to retrofit after
~100 component files exist:

```toml
[lib]
name = "streamboat_lib"   # the _lib suffix avoids a Windows name clash, rust-lang/cargo#8519
crate-type = ["staticlib", "cdylib", "rlib"]
```

All startup logic lives in `src/lib.rs` behind
`#[cfg_attr(mobile, tauri::mobile_entry_point)] pub fn run()`, with `src/main.rs` reduced to calling
`run()` (plus `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`). Sone already has
exactly this shape. Tauri mobile targets Android 8/API 26 and iOS 9 at the platform floor; the
community `tauri-plugin-native-audio` needs Android 8.0+/iOS 14.0+ and the iOS Background Modes →
Audio capability. See `references/mobile-path.md` for the full mobile comparison.

Source: `https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/start/migrate/from-tauri-1.mdx` (lines ~21–44)

## §10 IPC has a large-library data-path trap

Every `Serialize` return value from a Tauri command is JSON-serialized. The docs warn this "can slow
down your application if you try to return large data such as a file or a download HTTP response,"
and provide `tauri::ipc::Response::new(bytes)` for raw byte payloads (and `tauri::ipc::Request` for
raw request bodies).

Design rules for a TIDAL-sized library (tens of thousands of rows, thousands of covers):

- Page every list command — never return a full library snapshot.
- Push state **deltas**, not snapshots, for playback/queue state.
- Never round-trip album art as base64 JSON — cache covers Rust-side and serve through the
  asset/custom protocol or `convertFileSrc`.
- Keep the playback position tick out of IPC entirely: send `(position, wall_clock, rate)` and
  interpolate client-side — the same approach the WebSocket daemon transport already needs (see
  `references/architecture-shapes.md`).

Source: `https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/develop/calling-rust.mdx` (lines ~194–207, ~502–547)

## §11 WebKitGTK divergence catalog

Documented as of 2026 (Tauri issue tracker, verified directly):

| Symptom | Issue/workaround |
| --- | --- |
| Font weight offset by 100 | `tauri-apps/tauri#14286` — Epiphany (also WebKitGTK) renders it correctly, so this is a Tauri-side default, not an engine-wide bug |
| "Shadow copy" / glitchy rendering on maximize/unmaximize | `tauri-apps/tauri#13157`, WebKitGTK 2.48.0 |
| Blurry rendering during CSS animations; `contenteditable` spans not behaving as inputs | Documented in Tauri's linux-graphics doc (page itself blocked in this environment — directionally right, not re-verified this pass) |
| Blank window / rendering glitches / Wayland protocol error on NVIDIA | `WEBKIT_DISABLE_COMPOSITING_MODE=1` — Sone's README carries this exact workaround |
| DMABUF renderer crashes | `WEBKIT_DISABLE_DMABUF_RENDERER=1`, at the cost of the fast path |
| SVG `stroke-width` presentation attribute double-scaled by CSS `zoom` | Sone ships a CSS override in `ref:sone/src/App.css` — copy the pattern, not necessarily the exact rule |

The practical floor is whatever WebKitGTK version ships on your build container's distro: Sone's deb
depends on `libwebkit2gtk-4.1-0` and is built `FROM ubuntu:22.04`
(`ref:sone/build-scripts/build/Dockerfile.deb`) — so Ubuntu 22.04's WebKitGTK 4.1 is the practical
design baseline. Concrete rules: bundle fonts locally (no Google Fonts CDN — the app must work
offline and the CSP should not need to allow it); prefer plain CSS/Tailwind over engine-specific
effects; treat `backdrop-filter`, heavy CSS animation, and SVG presentation attributes as suspect;
add a manual three-webview visual pass to the release checklist, since `tauri-driver` cannot drive
WKWebView on macOS (Windows+Linux only — no WebDriver tool exists for WKWebView; WebdriverIO's Tauri
service covers macOS by running an embedded WebDriver server inside the app instead).

## §12 Binary size / RAM / startup: the levers, since the brief named this criterion

**Gap identified during fact-checking: the main report's scoring matrix drops this criterion
entirely** — no candidate is scored on it beyond a disclaimer that circulating Tauri-vs-Electron
numbers (75% less RAM, 20-50× smaller bundles, 3.7× faster startup) are SEO-blog sourced, not a
reproducible benchmark. Direct artefact measurement was not possible in this research environment
(`api.github.com` is blocked for repos outside this session). What can be stated without a benchmark:

- **Sone ships no `[profile.release]` section and no `rust-toolchain.toml` at all**
  (`ref:sone/src-tauri/Cargo.toml`, checked 2026-09-07) — its own release artefacts are unoptimised
  defaults, not a size datum for a tuned build.
- Tauri documents the actual `[profile.release]` levers:
  `codegen-units = 1`, `lto = true`, `opt-level = "s"` (or `"3"` for a speed preference),
  `panic = "abort"`, `strip = true`, `incremental = true`; on nightly additionally
  `trim-paths = "all"` and `rustflags = ["-Cdebuginfo=0", "-Zthreads=8"]`
  (`https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/concept/size.mdx`,
  fetched 2026-09-08). **`panic = "abort"` needs a deliberate decision for a long-running daemon** that
  may want to catch a panicking decoder thread rather than take the whole process down — do not adopt
  it mechanically.
- The size levers this skill already documents but never totals: WebView2 `fixedVersion` (+~180 MB)
  vs. `downloadBootstrapper` (+0 MB, §5 above), `appimage.bundleMediaFramework: true` pulling all of
  GStreamer into the AppImage, and shipping the GStreamer runtime as an MSI on Windows. In practice
  **the GStreamer runtime, not the UI toolkit, likely dominates installed size on Windows/macOS.**

If the owner wants real numbers, measure Sone/Museeks/tidal-hifi release assets on a machine with
GitHub access, and add this as a scored criterion (even 3-5 points, redistributed from Effort or
Cross-platform) rather than leaving it dropped.

**Museeks ships exactly this `[profile.release]` block in production** (verified live,
`raw.githubusercontent.com/martpie/museeks/master/src-tauri/Cargo.toml`, fetched 2026-09-08):
`codegen-units = 1`, `lto = true`, `opt-level = "s"`, `panic = "abort"`, `strip = true` — a second
shipped Tauri 2 + Rust + React music player (see `references/real-world-players.md`) actually running
these levers, not only Tauri's own docs recommending them. It is also a shipped precedent for
generating TypeScript types from Rust rather than hand-maintaining them (cross-cutting decision #6 in
`docs/research/tech-stack.md`, and Recommendation 1's own risk mitigation in
`references/scoring-and-candidates.md` §3): Museeks pins `ts-rs = "12.0.1"` in the same `Cargo.toml`,
independently corroborating the `ts-rs`/`specta` choice against Sone's 870-hand-written-line
`src/types.ts` counterexample.

## §13 Day-1 developer-environment cost, and one Windows trip hazard

**Gap identified during fact-checking: "Effort to MVP" never prices getting the toolchain running on
three OSes.** From Tauri's own prerequisites page
(`https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/start/prerequisites.mdx`,
fetched 2026-09-08):

- **Linux**: `libwebkit2gtk-4.1-dev` plus `build-essential`, `curl`, `wget`, `file`, `libxdo-dev`,
  `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev` (Arch: `webkit2gtk-4.1`,
  `appmenu-gtk-module`, `libappindicator-gtk3`, `librsvg`, `xdotool`; Fedora: `webkit2gtk4.1-devel`,
  `libappindicator-gtk3-devel`, `libxdo-devel`, plus the C-development group). This confirms Tauri 2
  on Linux hosts a **GTK3** window (`libayatana-appindicator3`, `libgtk-3-0` also in Sone's deb
  `depends`) with **WebKitGTK 4.1**, not GTK4 — worth remembering wherever HiDPI/fractional scaling or
  Linux look-and-feel comes up.
- **Windows**: Microsoft C++ Build Tools with "Desktop development with C++", WebView2 (preinstalled
  on Windows 10 1803+), **and the VBSCRIPT optional Windows feature, which is "only required for MSI
  installer packages"** — an easy silent-failure point on a hardened image or a CI runner building the
  `msi` target.
- **macOS**: Xcode Command Line Tools for desktop-only work, but a **full Xcode install for iOS
  development**.

Add the GStreamer devel packages on top of all three (see `references/packaging-and-policy.md` §1).

## §14 Workspace mechanics: `rust-toolchain.toml`, the pnpm+cargo coexistence, GStreamer version drift

**Gap identified during fact-checking: `references/architecture-shapes.md` §1's layout stops at
directory names.** Before ~100 component files exist, pin the mechanics:

- Decide explicitly whether `desktop/src-tauri` is a member of the root Cargo workspace or a nested
  one.
- Sone has a root `pnpm-workspace.yaml` sitting next to `src-tauri/` with **no `rust-toolchain.toml`
  at all** — a reproducibility gap worth avoiding, not a pattern to copy. It matters concretely here
  because the recommended `gstreamer` 0.25.3 needs MSRV 1.92/edition 2024 while `tauri` 2.11.x
  declares MSRV 1.77.2: **pin the toolchain explicitly in `rust-toolchain.toml` from day one.**
- Corpus consequence of picking `gstreamer` 0.25 over Sone's pinned 0.23: Sone is the codebase an
  agent will most often be asked to imitate, so expect agent-suggested GStreamer code to target the
  slightly older 0.23 API even once the project is on 0.25 — see
  `references/audio-engine-comparison.md` §3, item 3.

## §15 macOS CPU-architecture strategy: universal binary vs. Apple-Silicon-only

**Gap identified during fact-checking: never mentioned elsewhere in this research** — zero
occurrences of "universal", `universal-apple-darwin`, `aarch64-apple-darwin` or
`x86_64-apple-darwin` before this pass, despite §8's macOS packaging coverage. It is a required
`tauri build` flag decision, and it propagates into the audio-backend choice.

Tauri supports `tauri build --target universal-apple-darwin`, which builds both Rust targets and
`lipo`s them into one `.app` running natively on Intel and Apple Silicon
(https://github.com/orgs/tauri-apps/discussions/9419,
https://dev.to/hiyoyok/building-a-universal-binary-with-tauri-v2-its-easier-than-you-think-1b53 —
both via search 2026-09-08, `v2.tauri.app` itself blocked here; verify against the official
distribute/macos page before publishing). Costs: roughly double the build time (two full compiles)
and roughly double the artifact size; and **every bundled sidecar or native library must itself be
universal** (community guidance names `ffmpeg` as the go-to example of this trap).

Consequences:

1. **Owner decision**: drop Intel and ship Apple-Silicon-only, or pay 2x build+size for a universal
   binary.
2. **Interacts with the engine choice** (`audio-engine-comparison.md`): a lipo'd `libmpv.dylib` is one
   fat dylib, while a universal GStreamer plugin tree is a much bigger packaging job — an argument for
   libmpv on macOS this report otherwise makes only on packaging-weight grounds.
3. **Interacts with §8's `minimumSystemVersion`** — raising the floor to 12.0+ (recommended there) is
   itself close to where Intel support becomes moot.

## §16 System tray / close-window behaviour has no sub-decision row

**Gap identified during fact-checking**: "what happens when I close the window" is a first-week
product decision for a music player, and the main report's sub-decision table covers window chrome,
icons, virtualisation, state, and styling but not the tray — "tray" appears exactly once in the whole
report, inside a parenthetical.

Sone does **not** use Tauri's built-in tray plugin on Linux — it depends on `ksni = "0.3"`
(StatusNotifierItem over D-Bus) under `[target.'cfg(target_os = "linux")'.dependencies]`
(`ref:sone/src-tauri/Cargo.toml:69`), while its packages **still** declare
`libayatana-appindicator3-1` (deb, `ref:sone/src-tauri/tauri.conf.json:43`) and
`libappindicator-gtk3` (rpm, `:62`) as runtime dependencies — because Tauri's own Linux prerequisites
list `libayatana-appindicator3-dev` regardless (§13 above), that dependency arrives via Tauri whether
or not the tray plugin is used.

The tray is the one desktop feature with no headless counterpart, and no macOS/Windows equivalent to
`ksni` (both use their native tray APIs directly). It is linked to a real product decision: does
closing the window quit the app, or keep it playing in the tray/background? That decision determines
whether the desktop shell can ever hold the audio device while invisible — which interacts with the
"first process claims the device, later ones become clients" rule in `architecture-shapes.md` §4. Add
both as an explicit sub-decision row and an Open Question rather than leaving them implicit.
