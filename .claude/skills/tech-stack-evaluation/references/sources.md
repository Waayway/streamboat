# Sources — project→URL mapping and stack summary

`ref:<project>/<path>` throughout this skill and `docs/research/tech-stack.md` points at a shallow
(`--depth 1`) git clone of the named GitHub project, held read-only under
the reference checkouts (shallow clones of the cited GitHub projects; see references/sources.md)<project>`
in the research environment — not part of the streamboat repo itself, and not guaranteed to still
exist in a later session (re-clone from the URL below if it is gone). Clones were taken
2026-09-07. Line counts and file citations throughout this skill were re-verified against these
checkouts on that date; a few in the original report were wrong (see `verification-notes.md`).

Some rows below have no local checkout — cited by URL only, fetched live during research or
fact-checking. Web-source URLs (crates.io, GitHub raw file mirrors, docs pages) are listed
separately, in "Web sources" below.

## Reference checkouts (stack-relevant projects)

| `ref:<project>` | GitHub URL | License | Stack | Why it matters for the stack decision |
| --- | --- | --- | --- | --- |
| `sone` | https://github.com/lullabyX/sone | GPL-3.0-only | Tauri 2 + Rust + React 19 + Tailwind 4 + Jotai; GStreamer/`alsa` audio | **The single closest architectural precedent.** Everything in `references/tauri-engineering-facts.md` and most of `references/audio-engine-comparison.md` is sourced here. v0.21.0: 26,721 lines Rust + 47,609 lines TS/TSX, 199 files, 85 components, 12 atom modules. |
| `sone-windows` | https://github.com/lvllaby/sone-windows | GPL-3.0-only | Same as SONE, plus `wasapi2sink`/`souvlaki` for Windows | A fork of an **older** SONE (v0.16.0: 15,753 Rust + 28,518 TS/TSX) against current upstream v0.21.0 — not a thin per-OS delta, ~59% of upstream's current size and five minor releases behind. README: "Ported with help from AI agents", "may not be actively or correctly maintained". |
| `high-tide` | https://github.com/Nokse22/high-tide | GPL-3.0 | Python + GTK4/libadwaita + Meson + Blueprint; GStreamer `playbin3` | The GTK4/Python alternative stack; smallest codebase in the whole survey (6,821 lines) for a real Flathub-shipped client; its Flatpak sandbox has no raw ALSA access. |
| `strawberry` | https://github.com/strawberrymusicplayer/strawberry | GPL-3.0 | C++17 + Qt 6 + CMake + GStreamer | The Qt/QML alternative and the only project proven on all three desktop OSes. **Its `src/` is ~166,100 lines, not the 14,708 an earlier draft counted** — by far the largest codebase surveyed (see `verification-notes.md`). Its GStreamer Windows sink ranking (`directsoundsink` primary, `wasapi2sink` demoted) is direct counter-evidence to the SONE-Windows recommendation. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | MIT | Electron (castLabs `v43.0.0+wvcus` fork with Widevine CDM) wrapping TIDAL's web player | The Electron alternative and the reason Electron is not recommended: 8,011 lines of TS, but audio goes through Chromium and needs a castLabs Widevine build specifically because it wraps the DRM'd web player — an objection that disappears (but so does the advantage) once you implement the API yourself. |
| `tidalt` | https://github.com/Benehiko/tidalt | Apache-2.0 | Go + BubbleTea/Lipgloss TUI; FFmpeg (cgo) + hand-written ALSA (cgo) | The daemon/headless precedent: single-instance-becomes-server pattern, D-Bus MPRIS, systemd unit generation, hand-rolled ALSA format negotiation. 13,078 lines of Go + 357 lines of cgo C/headers (151 ALSA, 206 FFmpeg decode — not just "a 111-line alsa.c"). README states it was written almost entirely with LLM coding assistants. |
| `python-tidal` (`tidalapi`) | https://github.com/tamland/python-tidal (also mirrored at EbbLabs/python-tidal) | LGPL-3.0-or-later | Python | The unofficial-API reference library used by High Tide and mopidy-tidal; source for the TIDAL codec enum (MP3/AAC/MP4A/FLAC/EAC3/AC4) used in the audio-engine comparison. |
| `tidalrs` | https://github.com/phayes/tidalrs | MIT | Rust; `reqwest` 0.12 + rustls + `stream-download` | Calibration data point: "a Rust TIDAL API-client core is a few thousand lines" (3,545 lines, manifest/URL resolution only, no playback). |
| `mopidy-tidal` | https://github.com/tehkillerbee/mopidy-tidal | Apache-2.0 | Python; Mopidy plugin | Headless-server precedent (a TIDAL backend running inside a general music server with no GUI at all). |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | MS-PL | JS/TS injector running inside the **official** TIDAL Electron app | Not a stack candidate — cited only for what the official client's internals reveal (not relevant to this skill; see the `tidal-client-features` skill). |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | Apache-2.0 | TypeScript | TIDAL's own official SDK; cited for the OpenAPI spec and to confirm codec/entitlement shape, not as a stack precedent. |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | Apache-2.0 | Kotlin | Cited in the mobile-path discussion: the official SDK's own module boundary (player / streaming-api / auth as separate libraries) is the pattern `references/mobile-path.md` recommends mirroring. |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | Apache-2.0 | Swift | Same as above, iOS side. |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | MIT | Node/TypeScript | Minor citation only (licensing table). |
| `libopentidal` | https://github.com/Fokka-Engineering/libopenTIDAL | MIT | ANSI C | Minor citation only (licensing table); dead project (2021). |
| `tidalgo` | https://github.com/tcpj/tidalgo | none stated | Go | Not used in this skill. |
| `tidalswift` (TidalSwift) | https://github.com/melgu/TidalSwift | none — no LICENSE file | Swift/SwiftUI, AVPlayer | Cited in §4.13 as proof a Swift TIDAL client works, and as the reason SwiftUI is kept "in the back pocket" for a future iOS UI over a UniFFI-bound Rust core, never as the primary shell (no Linux/Windows story). Effectively all-rights-reserved — read for architecture only, never copy code. |
| `dotnet-tidal-usdk` | https://github.com/SacredSkull/dotnet-tidal-usdk | MIT + explicit anti-piracy clause | C#/.NET | Not a stack candidate (see §4.12's KMP/.NET objections); its licence-with-anti-piracy-clause is worth considering for streamboat's own licence text. |
| `tidal-api-docs` | https://github.com/gkasdorf/Tidal-API-Docs | none | — | Not used in this skill. |
| `tidal-connect` | https://github.com/GioF71/tidal-connect | MIT (wrapper only) | Docker wrapper around a closed binary | Not a stack candidate for streamboat (see the `headless-and-tidal-connect` skill — TIDAL Connect is permanently out of scope). |
| `tidal-fokka-engineering-` (TIDAL) | https://github.com/Fokka-Engineering/TIDAL | MIT | C | Not used in this skill. |

## Web sources (no local checkout)

Registry APIs and raw-file mirrors, fetched live — generally reachable through this environment's
egress proxy:

- `https://crates.io/api/v1/crates/<name>` — the authoritative source for every Rust crate version/
  license/download-count claim in this skill (tauri, cpal, wasapi, symphonia, rodio, gstreamer,
  slint, iced, egui, gpui, relm4, uniffi, cxx-qt, masonry, souvlaki). Prefer this over a blog post
  for any version claim — it is what caught the Masonry-version and Wails-version staleness (see
  `verification-notes.md`).
- `https://raw.githubusercontent.com/<org>/<repo>/<ref>/<path>` — raw file mirrors; used for
  cpal's CHANGELOG.md, mpv's `DOCS/man/ao.rst`, souvlaki's README, the Flathub requirements doc
  (`flathub-infra/documentation/main/docs/02-for-app-authors/02-requirements.md`), and several
  `tauri-apps/tauri-docs` pages (`plugin/updater.mdx`, `distribute/windows-installer.mdx`,
  `distribute/macos-application-bundle.mdx`, `develop/calling-rust.mdx`,
  `start/migrate/from-tauri-1.mdx`) — `v2.tauri.app` itself is blocked by this environment's
  egress proxy, but its GitHub source repo is not.
- `https://github.com/<org>/<repo>/releases` — used to catch Wails' actual latest prerelease
  (`v3.0.0-beta.17`, not the `beta.9` an earlier draft cited).
- `https://github.com/<org>/<repo>/issues/<n>` and `.../commit/<sha>` — cpal #106/#459, Tauri
  #13157/#14286, Strawberry #1227, the Flathub policy commit `992f57b30de98ddbd5e80959e9672998c83c8c97`.
- `https://developer.apple.com/app-store/review/guidelines/` — Apple App Store guidelines 5.2.1,
  5.2.2, 2.5.6, quoted verbatim in `references/packaging-and-policy.md`.
- `https://github.com/martpie/museeks` and its `src-tauri/Cargo.toml` — the second shipped
  Tauri 2 + Rust + React music player, and the negative control for "webview UI, not webview
  player" (see `references/real-world-players.md`).

## Domains blocked from this research environment

`v2.tauri.app`, `gstreamer.freedesktop.org`, `ui.shadcn.com`, `apps.gnome.org`, `docs.gtk.org`,
`mozilla.github.io`, `webdriver.io`, `avaloniaui.net`, `fyne.io`, `slint.dev`, `gamingonlinux.com`,
`linuxiac.com`, `opensourceforu.com`, and the entire `tidal.com` domain all returned blocked/failed
fetches at some point during this research. Where a claim rests only on a blocked domain (read via
a search-result summary rather than the primary page), this skill and the report say so explicitly
— treat those as directionally right, not primary-sourced, and re-check before using in anything
the owner will rely on as settled fact.
