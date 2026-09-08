# Sources — project→URL mapping and stack summary

`ref:<project>/<path>` throughout this skill and `docs/research/tech-stack.md` points at a shallow
(`--depth 1`) git clone of the named GitHub project, held read-only under `ref:<project>` in the
research environment — not part of the streamboat repo itself, and not guaranteed to still exist in a
later session (re-clone from the URL below if it is gone). Clones were taken 2026-09-07. Line counts
and file citations throughout this skill were re-verified against these checkouts on that date and
again during two later fact-check rounds; a few in the original report were wrong (see
`verification-notes.md`).

Some rows below have no local checkout — cited by URL only, fetched live during research or
fact-checking. Web-source URLs (crates.io, GitHub raw file mirrors, docs pages) are listed
separately, in "Web sources" below.

## Reference checkouts (stack-relevant projects)

| `ref:<project>` | GitHub URL | Commit | License | Stack | Why it matters for the stack decision |
| --- | --- | --- | --- | --- | --- |
| `sone` | https://github.com/lullabyX/sone | `21494b9` | GPL-3.0-only | Tauri 2 + Rust + React 19 + Tailwind 4 + Jotai; GStreamer/`alsa` audio | **The single closest architectural precedent.** Everything in `references/tauri-engineering-facts.md` and most of `references/audio-engine-comparison.md` is sourced here. v0.21.0: 26,721 lines Rust + 47,609 lines TS/TSX, 199 files, **102 files under `src/components/`** — 101 `.tsx` (83 top-level, 17 `settings/`, 1 `signal-path/`) plus `signal-path/types.ts` (recount, fact-checked: not 85, and not 102 `.tsx` — `signal-path/` holds one `.tsx` and one `types.ts`, not two `.tsx`), 19 of the `.tsx` are `.test.tsx`, 12 atom modules. Also the source of the TIDAL `subStatus` 4006 streaming-privileges-lost classification (`src-tauri/src/tidal_api.rs:15-18,6755`) and the Flathub release-automation bot (`.github/workflows/flathub-update.yml`). |
| `sone-windows` | https://github.com/lvllaby/sone-windows | `f2009f2` | GPL-3.0-only | Same as Sone, plus `wasapi2sink`/`souvlaki` for Windows | A fork of an **older** Sone (v0.16.0: 15,753 Rust + 28,518 TS/TSX) against current upstream v0.21.0 — not a thin per-OS delta, ~59% of upstream's current size and five minor releases behind. README: "Ported with help from AI agents", "may not be actively or correctly maintained". |
| `high-tide` | https://github.com/Nokse22/high-tide | `49f2472` | GPL-3.0 | Python + GTK4/libadwaita + Meson + Blueprint; GStreamer `playbin3` | The GTK4/Python alternative stack; smallest codebase in the whole survey (6,821 lines) for a real Flathub-shipped client; its Flatpak sandbox has no raw ALSA access. |
| `strawberry` | https://github.com/strawberrymusicplayer/strawberry | `5d24706` | GPL-3.0 | C++17 + Qt 6 + CMake + GStreamer | The Qt/QML alternative — **proven bit-perfect on Windows only, not Linux and not macOS** (correction, fact-checked twice over: an earlier draft said "all three desktop OSes", a second draft said "Linux and Windows"; `GstEngine::ExclusiveModeSupport()` in `src/engine/gstengine.cpp:523-525` covers only `wasapisink`/`wasapi2sink` — both Windows-only elements; its Linux `exclusive_mode_` flag, `src/engine/gstenginepipeline.cpp:632-637`, is only a `hw:`/`plughw:` device-prefix inference, not a dedicated bit-perfect path, and `plughw:` is itself a converting layer; `rg -i 'bit.perfect' src/` finds nothing; `src/engine/macosdevicefinder.cpp:27,73`'s CoreAudio use is device enumeration only). **Its `src/` is ~166,100 lines, not the 14,708 an earlier draft counted** — by far the largest codebase surveyed (see `verification-notes.md`). Its GStreamer Windows sink ranking demotes **both** `wasapisink` and `wasapi2sink` (`src/engine/gststartup.cpp:58-75`, not `wasapi2sink` alone) to `GST_RANK_SECONDARY`, ranking `directsoundsink` `GST_RANK_PRIMARY` — direct counter-evidence to the Sone-Windows recommendation. Also the source of the defensive `g_object_class_find_property(…, "exclusive")` idiom and the `device_warmup_duration_ms` warmup state machine (`src/engine/gstenginepipeline.cpp:142-144,632-635,722-726`). |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | `b1326db` | MIT | Electron (castLabs `v43.0.0+wvcus` fork with Widevine CDM) wrapping TIDAL's web player | The Electron alternative and the reason Electron is not recommended: 8,011 lines of TS, but audio goes through Chromium and needs a castLabs Widevine build specifically because it wraps the DRM'd web player — an objection that disappears (but so does the advantage) once you implement the API yourself. |
| `tidalt` | https://github.com/Benehiko/tidalt | `6cf18c9` | Apache-2.0 | Go + BubbleTea/Lipgloss TUI; FFmpeg (cgo) + hand-written ALSA (cgo) | The daemon/headless precedent: single-instance-becomes-server pattern, D-Bus MPRIS, systemd unit generation, hand-rolled ALSA format negotiation. 13,078 lines of Go + 357 lines of cgo C/headers (151 ALSA, 206 FFmpeg decode — not just "a 111-line alsa.c"). README states it was written almost entirely with LLM coding assistants. |
| `python-tidal` (`tidalapi`) | https://github.com/EbbLabs/python-tidal (formerly `tamland/python-tidal` — a move, not a mirror: the old URL still resolves only via GitHub's redirect) | `9c41fbe` | LGPL-3.0-or-later | Python | The unofficial-API reference library used by High Tide and mopidy-tidal; source for the TIDAL codec enum (MP3/AAC/MP4A/FLAC/EAC3/AC4) used in the audio-engine comparison. |
| `tidalrs` | https://github.com/phayes/tidalrs | `8bb1de8` | MIT | Rust; `reqwest` 0.12 + rustls + `stream-download` | Calibration data point: "a Rust TIDAL API-client core is a few thousand lines" (3,545 lines, manifest/URL resolution only, no playback). Also cited (`src/lib.rs:271-281,321,401`) for its `on_authz_refresh_callback` token-refresh hook — the reference implementation for the multi-process token-refresh race in `architecture-shapes.md` §4a. |
| `mopidy-tidal` | https://github.com/EbbLabs/mopidy-tidal (formerly `tehkillerbee/mopidy-tidal`) | `18abb3b` | Apache-2.0 | Python; Mopidy plugin | Headless-server precedent (a TIDAL backend running inside a general music server with no GUI at all). |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | `d8cd6bc` | MS-PL | JS/TS injector running inside the **official** TIDAL Electron app | Not a stack candidate — cited only for what the official client's internals reveal (not relevant to this skill; see the `tidal-client-features` skill). |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | `89244b4` | Apache-2.0 | TypeScript | TIDAL's own official SDK; cited for the OpenAPI spec and to confirm codec/entitlement shape, not as a stack precedent. **Also the primary source for the Pushkin streaming-privileges websocket** (`packages/player/src/internal/services/pushkin.ts`, `packages/player/src/api/event/streaming-privileges-revoked.ts`) — see `references/architecture-shapes.md` §4a. |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | `c93bff4` | Apache-2.0 | Kotlin | Cited in the mobile-path discussion: the official SDK's own module boundary (player / streaming-api / auth as separate libraries) is the pattern `references/mobile-path.md` recommends mirroring. Also carries the same Pushkin streaming-privileges module (`player/streaming-privileges/`). |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | `787fe96` | Apache-2.0 | Swift | Same as above, iOS side. Also carries `StreamingPrivilegesHandler.swift`. |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | `24f20a8` | MIT | Node/TypeScript | Minor citation only (licensing table). |
| `libopentidal` | https://github.com/Fokka-Engineering/libopenTIDAL | `89a1eaa` | MIT | ANSI C | Minor citation only (licensing table); dead project (2021). |
| `tidalgo` | https://github.com/tcpj/tidalgo | `6e5564e` | none stated | Go | Not used in this skill. |
| `tidalswift` (TidalSwift) | https://github.com/melgu/TidalSwift | `cf0926b` | none — no LICENSE file | Swift/SwiftUI, AVPlayer | Cited in §4.13 as proof a Swift TIDAL client works, and as the reason SwiftUI is kept "in the back pocket" for a future iOS UI over a UniFFI-bound Rust core, never as the primary shell (no Linux/Windows story). Effectively all-rights-reserved — read for architecture only, never copy code. |
| `dotnet-tidal-usdk` | https://github.com/SacredSkull/dotnet-tidal-usdk | `fed9e61` | MIT + explicit anti-piracy clause | C#/.NET | Not a stack candidate (see §4.12's KMP/.NET objections); its licence-with-anti-piracy-clause is worth considering for streamboat's own licence text. |
| `tidal-api-docs` | https://github.com/gkasdorf/Tidal-API-Docs | `457e15d` | none | — | Not used in this skill. |
| `tidal-connect` | https://github.com/GioF71/tidal-connect | `05ea5d7` | MIT (wrapper only) | Docker wrapper around a closed binary | Not a stack candidate for streamboat (see the `headless-and-tidal-connect` skill — TIDAL Connect is permanently out of scope). |
| `tidal-fokka-engineering-` (TIDAL) | https://github.com/Fokka-Engineering/TIDAL | `9b06cb1` | MIT | C | Not used in this skill. |

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
- `https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/concept/size.mdx`
  (fetched 2026-09-08) — the `[profile.release]` levers in `tauri-engineering-facts.md` §12.
- `https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/start/prerequisites.mdx`
  (fetched 2026-09-08) — the per-OS dev-environment prerequisites and the Windows VBSCRIPT/MSI trap in
  `tauri-engineering-facts.md` §13.
- `https://crates.io/api/v1/crates/dioxus` (fetched 2026-09-08) — dioxus 0.7.10, MIT OR Apache-2.0,
  ~2.57M downloads; supports the Dioxus mention in `agent-friendliness-and-testing.md` and
  `mobile-path.md`.
- `https://github.com/supersonic-app/supersonic/releases/tag/v0.22.1` (repo moved from
  `dweymouth/supersonic`) and
  `https://raw.githubusercontent.com/dweymouth/supersonic/main/go.mod` — confirms Supersonic's
  version (v0.22.1) and release date (**2026-08-09**, not the "2024-08-09" a round-two misreading of
  GitHub's no-year-suffix date format produced), cross-checked against `go.mod`'s `fyne.io/fyne/v2
  v2.8.0` requirement (released 2026-07-13); see `real-world-players.md` and `verification-notes.md`
  §1d.
- `https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-bad/sys/wasapi2/gstwasapi2sink.cpp`
  (fetched during round-three fact-checking) — the `exclusive` property on `GstWasapi2Sink` is
  annotated `Since: 1.28` (lines 195-204), `DEFAULT_EXCLUSIVE` `FALSE` (line 64); resolves the
  previously-unestablished minimum-GStreamer-version gap. See `audio-engine-comparison.md` §4.
- `https://github.com/orgs/tauri-apps/discussions/9419` and
  `https://dev.to/hiyoyok/building-a-universal-binary-with-tauri-v2-its-easier-than-you-think-1b53`
  (via search, 2026-09-08; `v2.tauri.app` itself blocked here) — `tauri build --target
  universal-apple-darwin`, cost ~2x build time/size, every bundled native library must itself be
  universal. See `tauri-engineering-facts.md` §15.
- `https://github.com/jpochyla/psst/discussions/359` (dated January 2023) — the druid-architecture
  quote used in `real-world-players.md`; date it when citing, since it predates iced 0.14's
  reactive-rendering rewrite.

## Domains blocked from this research environment

`v2.tauri.app`, `gstreamer.freedesktop.org`, `ui.shadcn.com`, `apps.gnome.org`, `docs.gtk.org`,
`mozilla.github.io`, `webdriver.io`, `avaloniaui.net`, `fyne.io`, `slint.dev`, `gamingonlinux.com`,
`linuxiac.com`, `opensourceforu.com`, and the entire `tidal.com` domain all returned blocked/failed
fetches at some point during this research. Where a claim rests only on a blocked domain (read via
a search-result summary rather than the primary page), this skill and the report say so explicitly
— treat those as directionally right, not primary-sourced, and re-check before using in anything
the owner will rely on as settled fact.
