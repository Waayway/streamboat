# Internationalisation, accessibility and observability

Full source: `docs/research/engineering-baseline.md` §9-§10 (fact-checked). `ref:<project>/<path>`
points at a shallow clone — see `sources.md`.

## Contents

1. i18n
2. Accessibility
3. Structured logging
4. Network request logging with redaction
5. Debug bundles
6. Metrics
7. Baseline security posture for the local control surface
8. Performance budgets

## 1. i18n

- **The reference bar is low and easy to beat.** Only High Tide ships translations: gettext with
  `po/{de,es,fr,it,nl,pl,pt_BR,zh_TW}.po`, a `high-tide.pot`, `po/LINGUAS`, `po/POTFILES` and a
  documented regeneration command
  `xgettext --files-from=po/POTFILES --output=po/high-tide.pot --from-code=UTF-8 --add-comments
  --keyword=_ --keyword=C_:1c,2` (ref:high-tide/CONTRIBUTING.md, ref:high-tide/po/). sone,
  tidal-hifi and tidalt are English-only. Strawberry uses Crowdin (`crowdin.yml`).
- **Flathub requires a complete English localisation** for UI, desktop file, metainfo and docs, and
  forbids "low quality, automated or machine-generated or mixed translations". So: English first,
  human translations only, and a note in the metainfo if the app is ever non-English-primary.
- **Mechanism [STACK]**: gettext for GTK/Qt/C/Python (the ecosystem default, best tooling, weakest
  plural/gender handling); Fluent for Rust/JS (best at plurals, gender and per-locale term
  variation, native to Firefox); ICU MessageFormat for JVM/JS/Android. Pick the one native to the
  chosen stack rather than a cross-stack lowest common denominator.
- **Externalise strings from commit one, whatever the mechanism.** Retrofitting i18n into a media
  player's dozens of screens is the expensive path.
- **Non-string localisation matters at least as much here**: track durations, dates in "recently
  added", large numbers (play counts), and RTL layout mirroring. Use the platform's
  locale-aware formatters, never manual `mm:ss` string building beyond duration.
- **TIDAL's own locale surface**: the v1 endpoints take a `locale` parameter alongside
  `countryCode` and `deviceType` — e.g. `?countryCode=NZ&locale=en_US&deviceType=DESKTOP`
  (ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:76,30). python-tidal hardcodes
  `self.locale = "en_US"` with the comment `# TODO Get locale from system configuration`
  (ref:python-tidal/tidalapi/session.py:404, 671), and sone passes `("locale", "en_US")` at **21**
  call sites in `tidal_api.rs` (lines 1808, 2307, 3074, 3258, 3336, 3381, 3429, 3489, 3528, 3576,
  3874, 3957, 4173, 4240, 5048, 5185, 5211, 5235, 5273, 5458, 5858 — corrected from an earlier
  "~10" estimate in this document). **streamboat should pass the user's actual locale** — that
  makes TIDAL's editorial page titles, mix names and descriptions come back localised for free,
  which is a bigger win than translating streamboat's own chrome. The exact set of locales TIDAL
  accepts is not documented in any reference checkout; determine it empirically (send a locale,
  compare the returned page titles) and record the result. `countryCode` comes from
  `GET /v1/sessions` and is a separate axis from `locale`.
- **i18n mechanics beyond picking a library**: three mechanical things break in practice that the
  library choice above does not cover. (1) The `.desktop` file and the AppStream metainfo are
  user-facing and separately translated — `msgfmt --desktop` merges translations into the desktop
  entry, and metainfo needs its own translated `<name>`/`<summary>`/`<description>`; High Tide's
  `po/POTFILES` + `xgettext` pattern (above) does not itself cover these files, so add them to
  `POTFILES` explicitly. (2) Add a pseudo-localization locale (wrap every translated string in
  markers, pad it ~40%) and do a manual pass on it — the only cheap way to find strings that were
  never externalized and layouts that break on German. (3) Define a string freeze before each
  release so translators have a stable target, which matters given the roughly weekly cadence
  `packaging-and-distribution.md` §8 recommends; Flathub forbids machine-generated or mixed
  translations (`packaging-and-distribution.md` §1), so an empty locale is better than an
  auto-filled one.

## 2. Accessibility

- **Keyboard navigation is the highest-value, stack-independent commitment**: every action
  reachable without a mouse, a visible focus ring, a shortcuts dialog (High Tide ships
  `data/shortcuts-dialog.blp`), Escape closing overlays (sone has a dedicated test,
  `NowPlayingDrawer.escape.test.tsx`), user-remappable shortcuts (sone lists "Customizable in-app
  keyboard shortcuts" as a feature and tests `useShortcuts`), and media keys via MPRIS / SMTC /
  MediaPlayerRemoteCommandCenter.
- **Screen readers [STACK]**, with the concrete state of each option:
  - GTK4 dropped ATK and talks to AT-SPI directly; widgets are accessible by default and the
    remaining work is labelling. High Tide's CONTRIBUTING already requires tooltips on symbolic
    icons.
  - Qt exposes `QAccessible` out of the box; on Linux the bridge activates with
    `QT_ACCESSIBILITY=1`.
  - Electron needs `ACCESSIBILITY_ENABLED=1` to activate its AT-SPI bridge on Linux.
  - Tauri/WebKitGTK inherits WebKit's AT-SPI support, but Tauri has a long-running accessibility
    tracking issue (tauri-apps/tauri#207) and users hit "Could not determine the accessibility bus
    address" (tauri-apps/tauri#4315). **Treat Linux screen-reader support as a risk item if the
    stack decision lands on a webview.**
  - Any webview-based UI must use semantic HTML plus ARIA and be tested with a real screen reader
    (Orca on Linux, NVDA on Windows, VoiceOver on macOS), not just an automated audit.
  - **Windows and macOS are first-class targets from v1 and need their own bar, not just a Linux
    one.** Windows exposes UI Automation (UIA) — test with NVDA, not only named in passing as the
    webview screen reader above. macOS uses NSAccessibility/AXAPI — test with VoiceOver, and a
    custom-drawn or webview UI needs explicit accessibility roles on *both* platforms, not just
    Linux's AT-SPI bridge. Add a per-OS row to the manual test matrix (`docs/testing/audio-matrix.md`,
    see `testing-strategy.md` §7) and state the OS media-key/now-playing integration a
    screen-reader user relies on for each platform (SMTC on Windows, MPNowPlayingInfoCenter on
    macOS, MPRIS on Linux) with acceptance criteria, not just a mention.
- **Contrast and themes**: support the system light/dark preference and a high-contrast mode;
  never encode state in colour alone (quality badges need a text label as well as a colour —
  tidalt tests exactly this, asserting every tier on the quality ladder produces a non-empty
  label, ref:tidalt/internal/tidal/quality_test.go).
- **Motion and audio**: honour the system reduced-motion preference for the visualiser/lyrics
  scrolling; never autoplay audio on launch without an explicit prior state.
- **Automated a11y gate [STACK]**: axe-core in the component test run (webview stacks), or the
  toolkit's own accessibility checker. Add one keyboard-navigation test per screen to the default
  suite so regressions are caught by CI rather than by users.
- **Target**: WCAG 2.2 AA for the UI surfaces where it applies. Write it down in CONTRIBUTING so
  contributors know the bar.

## 3. Structured logging

- Structured records (key/value or JSON) with a stable event name, not free-form prose:
  `event=playback.start track_id=… quality=HI_RES_LOSSLESS codec=flac sample_rate=96000
  bit_depth=24 sink=alsa device=hw:2,0`.
- Log the *decisions*, not just the outcomes: which quality tier was requested, which the server
  returned, why a fallback happened, which sink was chosen, whether resampling was inserted, what
  the negotiated PCM format was. Sone ships this as a user-facing feature ("Signal Path
  Transparency — inspect the live pipeline and confirm a pristine, bit-clean path to your DAC")
  backed by `signal_path.rs` and `pipeline_probe.rs`; make the same information available in the
  log.
- Levels: `error` for user-visible failures, `warn` for degraded-but-working (fallback to
  `plughw:`, quality downgrade, cache miss storm), `info` for lifecycle (login, playback start/stop,
  device change), `debug` for API request/response metadata, `trace` for bodies — and put bodies
  behind an explicit flag, never on by default.
- Keep a bounded in-memory ring buffer of the last N structured events regardless of the file-log
  toggle, so the debug bundle is useful even when file logging is off.

## 4. Network request logging with redaction

This is the highest-risk logging surface. Rules:

- **Redact by construction, not by regex over the finished line.** Build a `RedactedUrl` /
  `RedactedHeaders` type that the logger accepts, and make the raw types not implement the log
  formatting trait at all **[STACK]**. If a reviewer has to remember to redact, tokens will leak.
- Always redact: `Authorization` header, `X-Tidal-Token`, `X-Tidal-SessionId`, `sessionId`,
  `access_token`, `refresh_token`, `code`, `code_verifier`, `device_code`, `user_code`,
  `client_secret`, any `?token=` CDN query parameter, `userId`, and the email in the user profile
  response. Sone's precedent is narrow — one call site logs
  `body=<redacted: account endpoint>` (ref:sone/src-tauri/src/tidal_api.rs:1473) — so this is an
  area where streamboat should exceed the references.
- Log a stable fingerprint instead of the value where correlation matters: first 6 chars of the
  SHA-256 of the token, so "did the token change?" is answerable from a log.
- Log request *shape*: method, host, path template (`/v1/tracks/{id}/playbackinfopostpaywall`, not
  the concrete id if the id is sensitive — track ids are not), status, duration, retry count,
  `x-playback-session-id` if present. Do not log full response bodies at `info`.
- Add a `--log-http=headers|bodies` opt-in for debugging, printed with a loud warning that the
  output will contain sensitive data and must be scrubbed before sharing.

## 5. Debug bundles

Ship a first-class `streamboat debug-bundle` (and a button in the About/Settings screen) that
produces a single zip containing:

1. Version, build id, git SHA, package format (flatpak/snap/deb/aur/source), OS + version, desktop
   session, and — for a media app — the audio stack (PipeWire/PulseAudio/ALSA versions), the output
   device list, and the DAC's advertised formats and rates.
2. The effective settings, with secrets stripped and the fact of their presence noted
   (`refresh_token: <present>`).
3. The last N log files, passed through the same redactor.
4. The signal-path snapshot: every pipeline element and its negotiated caps.
5. The last M structured events from the ring buffer.
6. A manifest listing exactly what is included, so the user can audit before sharing.

Print the output path and state plainly that the bundle has been redacted but should still be
reviewed before posting publicly. This turns the sone playback-issue template's manual checklist
into one command.

## 6. Metrics

Do not export metrics by default. For the headless/daemon mode, an **opt-in** local Prometheus
endpoint (bound to loopback, off unless configured) is defensible: buffer underruns, HTTP error
rates by status, cache hit ratio by tier, token refresh count, current bitrate. It never leaves the
machine, which keeps it consistent with the no-telemetry stance. Define the numbers once — §8
below proposes the same buffer-underrun and cache-hit-ratio metrics as *performance budgets*; do
not define them twice.

## 7. Baseline security posture for the local control surface

The headless control-surface design (MPRIS vs HTTP/JSON) is left to the owner (see SKILL.md's Open
decisions), but whichever way that lands, a local listening socket has an established safe
default, and the reference set already has a working precedent to copy rather than designing from
the abstract. sone ships an opt-in local HTTP control surface (an MCP server) whose design is the
baseline: disabled by default (`if !settings.mcp_enabled { return }`); bound explicitly to loopback
(`let addr: SocketAddr = ([127,0,0,1], port).into()`), never `0.0.0.0`; authenticated by a random
UUIDv4 bearer token generated on first enable and persisted in the (encrypted) settings, carried in
the URL path (`/{token}/mcp`); a mutex held across the whole check→bind→store sequence so two
callers cannot race the port; and a dedicated `sanitizer.rs` that projects the internal TIDAL
models onto reduced structs (`SanitizedTrack`/`Album`/`Artist`/`Playlist` — id, title, artist,
duration only) so the surface cannot leak account fields
(ref:sone/src-tauri/src/mcp/mod.rs:12-42, ref:sone/src-tauri/src/mcp/server.rs:40,72-85,
ref:sone/src-tauri/src/mcp/sanitizer.rs). **One thing to do differently**: a token in the URL path
lands in access logs by default — put it in a header instead, and add it to §4's redaction list
regardless of which control-surface shape is chosen.

## 8. Performance budgets

For a music player the user-visible quality bar is largely non-functional: how long until audio
starts, whether a 10,000-track library scrolls, how much RAM it holds overnight. No reference
project defines these, and correctness testing (`testing-strategy.md`) is specified in depth but
nothing regresses performance. Define a small set of budgets in `docs/testing/` at v0.1, measured
with the same golden fixtures the audio tests (`testing-strategy.md` §7) use: cold start to first
frame; time from play-press to first audio sample (the number users compare against the official
client, dominated by the playbackinfo round-trip plus first-segment fetch — both already
instrumented by the structured logging in §3); seek latency; steady-state RSS after an hour of
playback; frame time scrolling a 10k-item list. Run them as a reporting (non-gating) CI job once
the stack exists, since runner variance makes gating unreliable.
