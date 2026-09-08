# What real music players in each stack look like

Corrected version of `docs/research/tech-stack.md` §6 — one significant reclassification
(Museeks) applied. Use this as a quick "has anyone shipped this stack for a music player, and did it
work" lookup when evaluating a candidate.

| Project | Stack | What it looks like / notable | Source |
| --- | --- | --- | --- |
| **SONE** | Tauri 2 + React 19 + Tailwind 4 + Jotai; Rust/GStreamer audio | Dark, Spotify-shaped: left sidebar, card grids, bottom player bar, now-playing drawer, full-screen player, floating resizable miniplayer, 15 theme presets + full colour picker, synced lyrics, signal-path panel with a "PRISTINE" verdict badge. Repo `github.com/lullabyX/sone` | verified from checkout |
| **sone-windows** | Same, plus `wasapi2sink` + `souvlaki` | Identical UI; separate fork, AI-assisted port, and **behind current upstream** (v0.16.0 vs. SONE's v0.21.0 — 15,753 Rust + 28,518 TS/TSX vs. 26,721 Rust + 47,609 TS/TSX) — not a thin per-OS delta | `ref:sone-windows/README.md` |
| **High Tide** | Python + GTK4 + libadwaita + Blueprint | GNOME HIG: `AdwNavigationView`-style pages, carousels, sidebar, `AdwToolbarView`. `flathub.org/apps/io.github.nokse22.high-tide` | verified from checkout |
| **Amberol** | Rust + GTK4 + GStreamer | The reference for "small beautiful GTK music player": album-art-derived adaptive background, no library management by design, no metadata editing, no network access. 2026.1 released 2026-04-05 by GTK core developer Emmanuele Bassi. `apps.gnome.org/Amberol` | web |
| **Decibels** | GJS + TypeScript + GTK4 + libadwaita | GNOME audio player with a waveform view | web |
| **Psst** | Rust + druid | Native-widget Spotify client, deliberately plain; author cites druid's reactive architecture as the source of most UI complexity, with Xilem as the intended successor. As of Feb 2026 requires the user's own Spotify Developer Client ID | web |
| **Spot** | Rust + GTK4 | GNOME-styled Spotify client, same visual family as High Tide | web |
| **Supersonic** | Go + Fyne 2.8; **libmpv** for audio | Dark/light built-in themes, dense grid+list layout; ReplayGain, 15-band EQ, gapless, "optional audio exclusive mode". **Correction (fact-checked): v0.22.1, not v0.22.0** — and its release timestamp reads 2024-08-09, which if accurate means ~2 years with no release; verify before citing it as an "actively maintained" comparator. AppImage bundles mpv; Linux needs `libmpv1`/`libmpv2`. Flatpak build "does not support CJK fonts as the sandboxing breaks font lookup". `github.com/dweymouth/supersonic` | web |
| **Feishin** | Electron + React 19 + Mantine + Zustand + electron-vite; **mpv** for audio | Full Spotify-alike for Navidrome/Jellyfin/Subsonic; synced lyrics, smart playlist editor. v1.15.1, 2026-07-19, ~9k stars. Bit-perfect requires `--audio-device=alsa/…`, `--gapless-audio=no`, exclusive mode; Flathub build uses PipeWire and won't do it | web |
| **tidal-hifi** | Electron (castLabs `v43.0.0+wvcus`) wrapping TIDAL's web player | Looks exactly like TIDAL web, because it *is* TIDAL web. Adds MPRIS, an Express HTTP API with Swagger, themes | `ref:tidal-hifi/package.json` |
| **tidalt** | Go + BubbleTea + Lipgloss TUI; FFmpeg + ALSA via cgo | Terminal UI with progress bar and text input; also `tidalt daemon` headless + MPRIS2 | verified from checkout |
| **Strawberry** | C++17 + Qt 6.4/6.8 + GStreamer | Classic desktop three-pane collection manager; audiophile focus incl. DSD for local files. **~166,100 lines of C++/headers — the largest codebase in this survey by a wide margin**, not the 14,708 an earlier count gave | `ref:strawberry/` |
| **ncspot** | Rust TUI (cursive) | Terminal Spotify client; the "TUI is enough" data point | web |
| **Cider** | Electron + Vue.js, with Rust components | Apple Music client; visually the most "designed" of the Electron music clients. The rumoured Electron→Tauri migration is **not confirmed** — sources describe it as Electron + Vue.js "utilizing localized Vue.js and Rust" | web (unverified) |
| **Nuclear** | Electron + React | Community music player; proof that Electron ships fast and looks fine | web |
| **Museeks** | **Corrected: Tauri 2 + Rust + React, not Electron.** Ported from Electron to Tauri (announced 2024; README: "Back-end: Tauri v2 / Rust", "UI: React.js") | **A second shipped Tauri 2 + React music player besides SONE — the useful negative control for this whole report's argument.** Its `src-tauri/Cargo.toml` pins `tauri` 2.11.2 but has **no audio crate at all** (`lofty` 0.23.3 for tags, `m3u`, `axum` 0.8.9, `tokio` 1.52.3, `sqlx` 0.8.6) — playback stays in the webview, so it cannot be bit-perfect. That is exactly the split streamboat must not fall into: webview UI, yes; webview *player*, no. Its `sqlx`+`axum` choices independently corroborate the SQLite/local-HTTP sub-decisions in Recommendation 1. Also the source for "Tauri does not support cross-platform binaries, so the command will only generate binaries for your current platform" — see `tauri-engineering-facts.md` §6. | `github.com/martpie/museeks`, its `src-tauri/Cargo.toml` |

**Avalonia 12's claimed 3× Android performance improvement is web, unverified** — same confidence
tier as the Cider migration rumour above (`avaloniaui.net` was blocked when fact-checked). It is not
a shipped music-player data point in this table but is cited elsewhere in
`references/scoring-and-candidates.md` §2 (4.12).
