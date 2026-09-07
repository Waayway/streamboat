# Engineering baseline for streamboat (stack-agnostic)

Scope: the conventions, testing strategy, secret handling, config/cache/log layout, licensing,
packaging, CI, repo layout, i18n/a11y and observability decisions that hold regardless of which
UI toolkit and language streamboat picks. Every item that only becomes concrete after the stack
decision is marked **[STACK]**.

Evidence is drawn first from the read-only reference checkouts of the projects named below (cited
as `ref:<project>/<path>`), then from primary upstream docs (cited as URLs). Where a claim
could not be verified from source or a primary doc, it says so.

Date of research: 2026-09-07.

---

## Summary

- **No reference TIDAL client records HTTP cassettes.** Grepping all 21 checkouts for
  `vcr|cassette|betamax|nock|wiremock` returns zero real hits. Every project either mocks at a
  seam above HTTP (mopidy-tidal, tidal-cli, tidal-sdk-web), stubs the transport in-process
  (tidalt, tidal-sdk-ios, strawberry), tests pure parse functions with inline JSON/base64 fixtures
  (sone, tidal-sdk-web), or requires a live account (python-tidal). streamboat should copy the
  in-process-transport + captured-fixture combination, not invent a cassette layer.
- **python-tidal — the library everything else depends on — has no test job in CI at all.** Its
  only workflow is `lint.yml`; the test suite needs a real subscription and passes tokens through
  `TIDAL_ACCESS_TOKEN` / `TIDAL_REFRESH_TOKEN` / `TIDAL_TOKEN_TYPE` (ref:python-tidal/tox.ini,
  ref:python-tidal/.github/workflows/lint.yml). Treat upstream behaviour as unverified by CI.
- **TIDAL's own SDKs run credentialed tests in CI** with a long-lived refresh token in GitHub
  secrets (`PLAYER_REFRESH_TOKEN`, `PLAYER_TEST_USER`), for both unit tests and Cypress
  (ref:tidal-sdk-web/.github/workflows/unit-test.yml, cypress.yml). streamboat cannot rely on
  that model for public PRs — forks do not get secrets — so the default gate must be
  credential-free.
- **The single most valuable CI idea to steal is TIDAL's own daily API-spec drift check**: a cron
  job diffs `https://tidal-music.github.io/tidal-api-reference/tidal-api-oas.json` against the
  vendored copy and opens a PR with the regenerated client
  (ref:tidal-sdk-ios/.github/workflows/check-tidalapi-spec.yml). An unofficial client needs the
  equivalent: a scheduled canary that detects endpoint drift before users do.
- **Live tests belong in a separate, opt-in target.** Strawberry ships
  `lyrics_live_tests` as `EXCLUDE_FROM_ALL`, explicitly documented as "a canary for detecting when
  a provider's endpoint, HTML structure, or response schema breaks"
  (ref:strawberry/tests/src/lyricsproviders_live_test.cpp, ref:strawberry/tests/CMakeLists.txt:126).
- **Encrypt tokens at rest with an OS-keyring-held key, and always keep a file fallback.** Sone
  uses AES-256-GCM with a 32-byte key from the OS keyring (service `sone`, entry `master-key`),
  falling back to `<config>/sone/sone.key` at mode 0600, with a 17-byte header
  `"SONE" + version + 12-byte nonce` and transparent passthrough of legacy plaintext
  (ref:sone/src-tauri/src/crypto.rs). High Tide instead stores the token JSON directly in
  libsecret under schema `io.github.nokse22.high-tide` (ref:high-tide/src/lib/secret_storage.py).
- **Under Flatpak you cannot unlock the login keyring.** High Tide guards its
  `Secret.Service`/`Collection.unlock_sync()` call with `Xdp.Portal.running_under_flatpak()`
  because "this is also only possible outside of a flatpak"
  (ref:high-tide/src/lib/secret_storage.py). Plan for keyring-unavailable at first launch.
- **Embedded client IDs are obfuscated, never secured, in every project that ships one.** Sone
  XORs them with a random key stored in the same file (ref:sone/scripts/gen_embedded.py,
  ref:sone/src-tauri/src/embedded_config.rs); Strawberry AES-256-CBC-encrypts them at CMake
  configure time with a key that CI generates as `openssl rand -hex 32` per build
  (ref:strawberry/cmake/ApiCredentials.cmake:37-78,
  ref:strawberry/.github/workflows/build.yaml:1216); python-tidal base64-encodes them. Document
  this honestly and prefer user-supplied credentials with an optional embedded default.
- **`--socket=pulseaudio` already grants raw ALSA.** The Flatpak sandbox docs state it "includes
  sound input (mic), sound output/playback, MIDI and ALSA sound devices in `/dev/snd`"
  (https://github.com/flatpak/flatpak-docs/blob/master/docs/sandbox-permissions.rst). A Flathub
  TIDAL client therefore does **not** need `--device=all` for exclusive ALSA output — High Tide
  ships only `--share=network --share=ipc --socket=fallback-x11 --socket=wayland --device=dri
  --socket=pulseaudio --filesystem=xdg-run/pipewire-0:ro`
  (ref:high-tide/build-aux/io.github.nokse22.high-tide.json).
- **Flathub will not take a console app**, requires "a meaningful history of development",
  requires a complete English localisation, requires the name and icon not to imply affiliation
  with a vendor (so: not "TIDAL <x>", not TIDAL's logo), and requires the license to be declared
  and match the source (https://docs.flathub.org/docs/for-app-authors/requirements — read from
  the source repo at flathub-infra/documentation, HEAD 2026-09-07). The headless mode ships via
  distro packages, not Flathub.
- **Flathub now has a Generative AI policy that requires disclosure.** Submitters "must disclose
  any AI-generated code, documentation, packaging, or other material", "AI tools or agents must
  not open or automate Flathub submission pull requests, or generate their commit messages,
  descriptions, review comments, or replies", and undisclosed material "may result in rejection"
  and repeat violations in "a permanent ban" (same doc). This is a hard constraint on how
  streamboat is developed and submitted, and the owner must decide the project's posture on it.
- **Every reference Linux client is GPL-3.0** (sone GPL-3.0-only, High Tide GPL-3.0, Strawberry
  GPL-3.0, tidal-hifi MIT is the exception because it is a web wrapper). Libraries are
  permissive/weak-copyleft: python-tidal LGPL-3.0-or-later, mopidy-tidal Apache-2.0, tidalrs MIT,
  libopenTIDAL MIT, tidalt Apache-2.0, all three TIDAL SDKs Apache-2.0.
- **GStreamer pushes you to GPL in practice.** Core and base plugins are LGPL-2.1, but
  gst-plugins-ugly and some bad plugins are GPL, and gst-libav depends on how FFmpeg was built;
  GStreamer's own guidance is that with GPL-linked plugins "GStreamer is for all practical
  reasons under the GPL itself"
  (https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/licensing.html).
  fdk-aac is GPL-incompatible and must not be linked into a GPL binary
  (https://fedoraproject.org/wiki/Licensing/FDK-AAC).
- **macOS distribution costs $99/year and cannot be skipped** for a usable download: Developer ID
  + notarization are covered by Apple Developer Program membership, Homebrew applies quarantine
  and audits casks against Gatekeeper, and Homebrew's notability floor for a self-submitted cask
  is 90 forks / 90 watchers / 225 stars, with repos under 30 days old normally ineligible
  (https://github.com/Homebrew/brew/blob/main/docs/Package-Acceptance-Policy.md,
  https://github.com/Homebrew/brew/blob/main/docs/Homebrew-Security-and-Supply-Chain.md).
- **Windows signing is now cheap if you qualify**: Azure Trusted Signing (renamed Azure Artifact
  Signing) is $9.99/month Basic, open to individuals in the USA and Canada and organisations in
  the EU/UK; an EV certificate is roughly $280–500/year instead.
- **A GStreamer app on macOS needs the `com.apple.security.cs.allow-jit` entitlement** because
  liborc JIT-compiles SIMD at runtime under Hardened Runtime
  (ref:strawberry/dist/macos/strawberry.entitlements).
- **Cache with tiers, TTL, stale-while-revalidate and a hard byte cap.** Sone caps the disk cache
  at 2 GiB with LRU eviction down to 90% (1.8 GB), and uses TTLs of 15 min / 4 h / 7 d / 30 d with
  SWR grace of 1 h / 24 h / 30 d / 90 d for user content / dynamic / static metadata / images
  (ref:sone/src-tauri/src/cache.rs:17-56,188-189,608-645).
- **Log rotation numbers to copy**: 5 MB per file, 9 rotated files kept (~50 MB ceiling), file
  logging behind a user toggle read from a plaintext sidecar before the app starts
  (ref:sone/src-tauri/src/logging.rs).
- **No telemetry by default is the norm and it is a marketing asset.** Sone: "No telemetry, no
  tracking — fully open source under GPL-3.0. SONE collects no telemetry and sends nothing to its
  developers" (ref:sone/README.md:110). Its one exception (play reporting back to TIDAL, so
  Recently Played works) is disclosed and switchable.
- **Nobody ships an in-app auto-updater on Linux.** Sone only polls
  `https://api.github.com/repos/lullabyX/sone/releases/latest` and compares semver, leaving the
  update to the package manager (ref:sone/src-tauri/src/commands/updates.rs). Strawberry enables
  Sparkle on macOS only (`-DENABLE_SPARKLE=ON`, ref:strawberry/.github/workflows/build.yaml).

---

## Findings

### 1. Baseline: what the reference projects actually do

| Project | License | Tests in CI | HTTP test double | Credentialed CI | Lint/format gate | Packaging targets |
|---|---|---|---|---|---|---|
| python-tidal | LGPL-3.0-or-later | none (lint only) | none — live account | n/a (local only) | isort + black + docformatter, mypy via tox | PyPI |
| mopidy-tidal | Apache-2.0 | yes, matrix 3.12/3.13/3.14 × mopidy 3.3/3.4 | `unittest.mock` over `tidalapi`; `pytest-httpserver` + `trustme` for the caching proxy | no | ruff check + ruff format | PyPI |
| sone | GPL-3.0-only | not wired to GitHub Actions (only flathub-update) | inline `serde_json::json!` fixtures for parsers; jsdom + Testing Library for UI | no | eslint, prettier, clippy `-D warnings`, cargo fmt, knip | Flathub, AUR (2 pkgs), Cloudsmith deb/rpm, Snap |
| sone-windows | GPL-3.0-only | not inspected in depth | — | — | same toolchain as sone | Windows |
| high-tide | GPL-3.0 | no tests | none | no | flake8/ruff config present; CI runs codespell + flatpak build | Flathub |
| tidal-hifi | MIT | build + lint only | none | no | oxlint + prettier | deb/rpm/pacman/AppImage/snap/tar.gz/freebsd, Windows MSI, macOS zip, AUR |
| strawberry | GPL-3.0 | gtest/gmock suite | `MockNetworkAccessManager` + `MockNetworkReply` | no (secrets only for build-time API keys) | clang-format config, `-DBUILD_WERROR=ON` | 13 build jobs incl. Ubuntu PPA, AppImage, FreeBSD, OpenBSD, macOS DMG (notarized), Windows MinGW + MSVC |
| tidalt | Apache-2.0 | `go build` + `go test` + golangci-lint v2.11.3 + clang-tidy | `httptest.Server` + custom `RoundTripper` | no | gofumpt, golangci-lint, pre-commit hook mirroring CI | deb, rpm, Arch, Docker, static binaries + `checksums.txt` |
| tidal-cli | MIT | vitest on Node 20 & 22 | `vi.mock('../auth')` + fake openapi client | no | — | npm (with `--provenance`), clawhub skill |
| tidal-sdk-web | Apache-2.0 | vitest unit + Cypress | `vi.mock` of storage/utils/fetch; inline base64 manifests | yes (`PLAYER_REFRESH_TOKEN`) | eslint, tsc typecheck, `arethetypeswrong` | npm via OIDC |
| tidal-sdk-ios | Apache-2.0 | SPM tests + xcodebuild | `JsonEncodedResponseURLProtocol` replay stub | not observed | SwiftFormat + SwiftLint via pre-commit | SwiftPM |
| tidal-sdk-android | Apache-2.0 | unit + instrumented | not inspected | not observed | detekt + ktfmt, FOSSA | Maven |
| TidaLuna | Ms-PL | build only | — | — | prettier, renovate | GitHub releases |

Read the table as a warning as much as a template: the two most-cited unofficial clients
(High Tide, tidal-hifi) ship **zero** automated tests of TIDAL behaviour, and the library they all
sit on (python-tidal) does not run its tests in CI. streamboat can be materially better here at
low cost, because the API surface is JSON in / typed structs out.

---

### 2. Testing a client of an unofficial API without live credentials in CI

#### 2.1 The architectural precondition

Every credential-free test in the reference set exists because somebody split "fetch bytes" from
"turn bytes into a model". Sone's parser tests are the clearest example: `parse_search_response`,
`parse_home_tabs`, `parse_playlist_items`, `parse_v1_module` and `DirectHitItem::from_typed_value`
are all pure functions over `&str`/`serde_json::Value`, so their tests are literal JSON literals
with no network, no auth and no async
(ref:sone/src-tauri/src/tidal_api.rs:6290-6400). Sone has roughly 145 inline `#[test]` functions
across its Rust sources, 51 of them in `tidal_api.rs` alone.

**Rule for streamboat:** every TIDAL response type gets a pure `parse_*` (or `TryFrom<Bytes>`)
function that takes bytes/JSON and returns a typed result or a typed error. HTTP, auth and retry
live outside it. This one rule is what makes the rest of this section cheap.

#### 2.2 Layer 1 — pure parser tests with captured fixtures (the bulk of the suite)

- Store real captured responses as files under `tests/fixtures/api/<endpoint>/<case>.json`, not as
  inline literals, once you have more than a handful. Inline literals (sone, tidal-sdk-web) are
  fine at small scale and much easier to read in the diff.
- Capture at least: a full success, an empty/missing-section success, a null-valued field, an
  unknown enum value, an error envelope for each documented sub-status, and a paginated page 2.
  Sone's tests encode exactly these cases: "A missing videos section yields an empty vec, never an
  error" and "no item should be dropped" on `duration: null`
  (ref:sone/src-tauri/src/tidal_api.rs:6303-6317, 6370-6383).
- **Redact before committing.** Captured playbackinfo/manifest payloads contain signed CDN URLs
  with `?token=` query parameters and, for DASH, Widevine/PlayReady `cenc:pssh` blobs — see the
  base64 manifests checked into
  ref:tidal-sdk-web/packages/player/src/internal/helpers/manifest-parser.test.ts. Those tokens are
  time-limited (the CDN token in that fixture encodes an epoch), so they are stale rather than
  dangerous, but a fixture must never contain an `access_token`, `refresh_token`, `sessionId`,
  `userId`, email, or a real subscriber's playlist contents. Write a `scripts/scrub-fixture` that
  runs over every capture and fails CI if a token-shaped string survives.
- Record the capture date and the endpoint+params at the top of each fixture (a sibling
  `.meta.json`), so a future failure can be attributed to drift rather than to a bug.

#### 2.3 Layer 2 — transport-level tests (in-process server or stubbed transport)

Two proven shapes, pick per stack:

- **In-process HTTP server.** mopidy-tidal's proxy tests run `pytest_httpserver.HTTPServer` with
  `trustme`-minted TLS certs and drive the real client code end to end, including `Range` requests
  and cache insertion (ref:mopidy-tidal/tests/test_proxy.py). This is the right shape for anything
  that touches HTTP semantics: ranges, redirects, retries, 429 handling, chunked transfer.
- **Stubbed transport.** tidalt injects a `roundTripFunc` that rewrites every request host to a
  local `httptest.Server`, and pins the OAuth token expiry an hour into the future so the refresh
  path never fires (ref:tidalt/internal/tidal/api_test.go:19-48). tidal-sdk-ios registers a
  `JsonEncodedResponseURLProtocol` that can `succeed(with:)`, `fail(with:)`, or `replay([...])` a
  sequence of responses, and records every `URLRequest` for assertions
  (ref:tidal-sdk-ios/Tests/PlayerTests/Mocks/JsonEncodedResponseURLProtocol.swift).

The `replay([Response])` shape is what a cassette gives you without a cassette format: a list of
canned responses consumed in order, so you can test "401 then refresh then 200", "429 with
Retry-After then success", and "5xx, 5xx, success" backoff sequences deterministically.

**Tests that must exist at this layer for streamboat:**

1. 401 on a content endpoint triggers exactly one token refresh and one retry, and concurrent 401s
   collapse into a single refresh (tidalrs uses a `Semaphore` for exactly this; tidal-sdk-android
   uses a `TokenMutex`).
2. 429 sets a global cooldown, clamped. Sone's `RateGate` clamps `Retry-After` to
   `[1, 120]` seconds with a 5-second default when the header is absent or in HTTP-date form,
   and stores an absolute deadline via `fetch_max` so concurrent 429s can only lengthen it
   (ref:sone/src-tauri/src/rate_gate.rs).
3. Terminal playback sub-statuses are not retried. The survey records sone treating 4005, 4010 and
   4030–4035 as terminal; verify the exact list against the streaming-topic research before
   encoding it.
4. The quality fallback cascade stops at the first success and does not cascade past a rate-limit
   or terminal error.
5. Token refresh persists the new tokens to storage exactly once and does not lose the refresh
   token when the response omits it.

#### 2.4 Layer 3 — contract tests against the published spec

TIDAL publishes an OpenAPI document at
`https://tidal-music.github.io/tidal-api-reference/tidal-api-oas.json`, and both official SDKs run
a **daily** cron that `cmp`s the vendored copy against the freshly downloaded one, prints a unified
diff into the job log, and triggers a client-regeneration workflow that opens or updates a PR
(ref:tidal-sdk-ios/.github/workflows/check-tidalapi-spec.yml, and the analogous
ref:tidal-sdk-android/.github/workflows/check-tidalapi-spec.yml).

For streamboat:

- Vendor the OAS document under `contracts/tidal-api-oas.json` and run the same daily diff. It
  covers the v2 (`openapi.tidal.com/v2`) surface only.
- The v1 surface streamboat actually needs for playback (`/v1/tracks/{id}/playbackinfopostpaywall`,
  `/v1/tracks/{id}/urlpostpaywall`, `/v1/sessions`, `/v1/pages/*`) is **not** in that spec. Cover
  it with hand-written JSON Schemas in `contracts/v1/*.schema.json`, validate every captured
  fixture against them in the offline suite, and validate live responses against them in the
  opt-in live suite. A schema violation in the live suite is the drift alarm.
- Version the schemas alongside a `contracts/CHANGELOG.md` so an endpoint change is a reviewable
  diff and not a mystery bug report.

#### 2.5 Layer 4 — opt-in live tests ("canary")

Design constraints, all evidenced:

- Never in the default target. `add_executable(lyrics_live_tests EXCLUDE_FROM_ALL ...)` and the
  comment "These tests hit the real network ... They are NOT part of the default `strawberry_tests`
  aggregate; build and invoke `lyrics_live_tests` explicitly"
  (ref:strawberry/tests/src/lyricsproviders_live_test.cpp:26, ref:strawberry/tests/CMakeLists.txt:126).
- Gate interactive login behind an explicit flag. python-tidal adds a `--interactive` pytest
  option and auto-skips anything marked `interactive` without it
  (ref:python-tidal/tests/conftest.py, `pytest_addoption` / `pytest_collection_modifyitems`).
- Take credentials from a credential store chain, not from a checked-in file. python-tidal tries
  `EnvCredentials` → `CachedCredentials` (`~/.local/cache/python-tidal/<key>.json`) →
  `KeyringCredentials`, and refuses to persist to the env store
  (ref:python-tidal/tests/conftest.py:100-131).
- Use a generous network timeout and a stable, well-known asset. Strawberry uses 30 s and a
  "canary track" with a fallback track when a provider's slug router collides.

Recommended shape for streamboat: `streamboat-test --live` (or `cargo test --features live-tests` /
`pytest --live` **[STACK]**) that:

1. Reads tokens from `STREAMBOAT_TEST_TOKENS` (a JSON blob) or the OS keyring entry
   `streamboat-test`.
2. Runs a fixed script: session info → search a known ISRC → fetch album → fetch track →
   `playbackinfopostpaywall` at each quality tier → validate every response against the vendored
   schemas → assert the manifest parses and the first segment URL returns 200 with a
   `Content-Type` in the expected set.
3. Does **not** play audio to a device, does not download whole tracks, and stops at the first
   segment's HTTP headers.
4. Emits a machine-readable report (`live-report.json`) with per-endpoint pass/fail plus the
   observed audioQuality/bitDepth/sampleRate, so the maintainer can diff two runs.
5. Runs from a maintainer's machine or a self-hosted runner on a schedule, never from a fork PR.

Add a second, credential-free canary that hits only the unauthenticated surfaces
(`/v1/oauth2/device_authorization` returns a device code without an account). mopidy-tidal proves
the value of testing the unauthenticated path end to end: it spawns real Mopidy with `pexpect`
against a config with no tokens and asserts that `mpc list artist` surfaces
`.* visit https://link.tidal.com/.*` (ref:mopidy-tidal/integration_tests/test_login_hack.py). That
single test would catch a device-flow endpoint change with no subscription at all.

#### 2.6 Fuzzing manifest and response parsing

No reference project fuzzes anything (grep for `fuzz` across the checkouts hits only tidalt UI
code unrelated to fuzzing, and no project depends on proptest/hypothesis/quickcheck). This is a
genuine gap streamboat can close cheaply, and it matters because the manifest parser consumes
attacker-influenceable input: base64 → XML (DASH MPD) or base64 → JSON (BTS), plus M3U8 for video.

Targets, in priority order:

1. `parse_manifest(mime_type, base64_body)` — malformed base64, truncated XML, XML with 10^6
   nested elements, billion-laughs entity expansion, `SegmentTemplate` with `$Number$` and an
   absurd `r=` repeat count, missing `BaseURL`, `codecs` attribute with control characters.
2. `parse_playbackinfo(json)` — wrong types for `bitDepth`/`sampleRate`, absent `manifest`,
   `manifestMimeType` unknown.
3. M3U8 / HLS playlist parsing for video.
4. The DASH segment-URL builder — a `$Number$` substitution that produces a URL pointing off-host
   must be rejected, not fetched. Pin the expected CDN host set and assert every derived URL is
   within it.

Corpus: seed from the committed fixtures (the tidal-sdk-web test file alone gives DASH-FLAC,
DASH-AAC, BTS-MP3 and EMU-HLS manifests to seed with). Run the fuzzer in CI for a bounded time
(60 s per target on PRs, 15 min nightly), commit crashers as regression fixtures.

Independent of the fuzzer: **set hard limits** — reject a manifest over N bytes (start at 1 MiB),
disable XML external entities and DTD processing outright, cap segment count, and cap total
decoded size.

#### 2.7 Audio pipeline tests

What the references do:

- **Short local audio fixtures.** tidal-sdk-ios ships `test_5sec.m4a` and `test_1min.m4a`
  (ref:tidal-sdk-ios/Tests/PlayerTests/Resources/AudioFiles/). Strawberry ships one track in 12
  container/codec combinations — aif, asf, flac, m4a, mp3, mp4, oga, ogg, opus, spx, wav, wv
  (ref:strawberry/tests/data/audio/).
- **A position tolerance, stated as a constant.** tidal-sdk-ios asserts playback positions with
  `acceptableAssetPositionTimeRange: TimeInterval = 0.5` seconds
  (ref:tidal-sdk-ios/Tests/PlayerTests/Playlog/PlayLogTestsHelper.swift).
- **A polling wait helper instead of fixed sleeps.** `optimizedWait(timeout: 10.0, step: 0.1,
  until:)` pumps the run loop and fails with a recorded issue rather than hanging
  (ref:tidal-sdk-ios/Tests/PlayerTests/Helpers/OptimizedWait.swift).
- **Pure tests for the device-negotiation state machine.** tidalt tests that only a format
  refusal (`errFormatRefused`) downgrades `hw:` to `plughw:` and that a busy device does not, and
  that the fallback is memoised per device — with no ALSA hardware involved
  (ref:tidalt/internal/player/alsa_fallback_test.go).

Recommended layers for streamboat:

1. **Pure logic, no device.** Sample-rate/bit-depth negotiation table, ReplayGain gain computation
   (sone's formula is `0.8 * min(10^((rg+4)/20), 1/peak)`, per the survey — verify against
   ref:sone/src-tauri/src/commands/playback.rs before encoding), quality-ladder mapping, the
   ALSA/WASAPI fallback state machine, queue/gapless scheduling decisions. These run everywhere,
   including CI, and should be the majority of audio tests.
2. **Golden decode.** Decode a committed 5-second FLAC and a 5-second AAC to PCM and compare
   against a committed golden PCM (or its SHA-256) — this catches decoder configuration
   regressions and, if you implement a bit-perfect path, proves no resampling or dithering
   happened. Keep the fixtures short (<200 KB) and generate them locally with a tone/sweep so no
   licensed music enters the repo.
3. **Null-sink pipeline test.** Run the real pipeline into a null/file sink in CI (Linux runners
   have no sound card; GStreamer `fakesink`/`appsink`, or ALSA `null` device). Assert: state
   reaches PLAYING, N seconds of PCM arrive with the expected caps, EOS fires once, position
   monotonically increases.
4. **Gapless timing.** Concatenate two 5-second fixtures, play them through the queue, and assert
   the sample count at the boundary matches within a stated tolerance and that no
   underrun/silence event was emitted. Express the assertion in samples, not wall-clock, when the
   sink is a file; use the 0.5 s tolerance only for real-device tests.
5. **Device tests are manual.** Exclusive ALSA/WASAPI, DAC hot-swap and real bit-perfect output
   cannot run in CI. Ship a documented manual test matrix (`docs/testing/audio-matrix.md`) and a
   `--signal-path` diagnostic command that prints the negotiated format at each element, so bug
   reports carry it. Sone's playback issue template is the model
   (ref:sone/.github/ISSUE_TEMPLATE/playback_issue.md) — it asks for version, install source,
   distro, session, GPU, output device, sample rate/bit depth, whether exclusive mode is on, and
   the log path.

#### 2.8 UI tests **[STACK]**

- Component-level with a DOM: sone runs vitest + jsdom + `@testing-library/react` with a
  dedicated `vitest.config.ts` separate from the app's Vite config, covering ~40 components
  including virtualization, pagination, keyboard shortcuts and settings tabs
  (ref:sone/vitest.config.ts, ref:sone/src/components/*.test.tsx).
- End-to-end with a browser: tidal-sdk-web runs Cypress against its own dev server, mapping
  `dev.tidal.com` to `127.0.0.1` in `/etc/hosts` and using `vite-plugin-mkcert` for TLS
  (ref:tidal-sdk-web/.github/workflows/cypress.yml).
- Native toolkits: Strawberry links `Qt::Test` and builds separate `test_gui_main` for
  widget-level tests (ref:strawberry/tests/CMakeLists.txt).
- tidalt smoke-tests its terminal UI views (`ref:tidalt/internal/ui/view_smoke_test.go`).

Whatever the stack, commit to: (a) a smoke test that constructs every top-level screen with a
fake data layer and asserts no crash, (b) a keyboard-navigation test per screen, (c) a snapshot or
golden-image test only for pure-presentational components, since screenshot tests on a media
player with album art are a maintenance sink.

#### 2.9 The non-negotiable CI rule

**The default test job must pass on a fork PR with no secrets, with the network disabled.** Enforce
it: run the offline suite with outbound network blocked (a proxy env var pointing at a dead port,
or a sandbox), so a test that accidentally reaches the internet fails loudly instead of flaking.
Everything credentialed goes in a separate workflow gated on
`github.event.pull_request.head.repo.full_name == github.repository`, the pattern Strawberry uses
for its signing/notarizing steps (ref:strawberry/.github/workflows/build.yaml:1178, 1253).

---

### 3. Secrets and tokens

#### 3.1 What lives where

| Data | Storage | Rationale |
|---|---|---|
| `refresh_token` | OS keyring; encrypted file fallback | Long-lived, the only real credential |
| `access_token`, `expiry` | Same store as refresh token, or memory + re-derive | Short-lived, but leaks the account if logged |
| `sessionId`, `userId`, `countryCode` | Encrypted config file | Not secret, but identifying |
| `client_id` / `client_secret` (embedded default) | Build-time constant, obfuscated at best | Extractable by anyone; see 3.4 |
| User-supplied `client_id`/`client_secret` | Same store as tokens | Treat as secret because the user chose it |
| Cached API responses | Encrypted or plain cache dir, user-configurable | Contains library/listening data |

#### 3.2 OS keyrings, per platform

- **Linux — Secret Service / libsecret.** High Tide defines a schema
  `io.github.nokse22.high-tide` with a single `version` string attribute, stores one JSON blob
  under key `high-tide-login` containing `token-type`, `access-token`, `refresh-token`,
  `expiry-time`, `is-pkce`, and wraps the whole load in a try/except that resets the store on
  corruption (ref:high-tide/src/lib/secret_storage.py). It also unlocks the default collection at
  startup — `Secret.Service.get_sync()` → `Secret.Collection.for_alias_sync(...,
  Secret.COLLECTION_DEFAULT, ...)` → `service.unlock_sync([collection])` — but only when
  `not Xdp.Portal.running_under_flatpak()`.
- **Flatpak.** `--talk-name=org.freedesktop.secrets` is the (lowercase — the Flathub linter has a
  never-granted rule for the wrong casing,
  https://docs.flathub.org/docs/for-app-authors/linter `finish-args-incorrect-secret-service-talk-name`)
  way to reach a host keyring. The sandbox-native alternative is the **Secret portal**,
  `org.freedesktop.portal.Secret` (xdg-desktop-portal ≥ 1.5.0), which hands the app a per-app
  master secret over a pipe FD; the secret is stable for the life of the installation and is
  itself stored in the user's keyring under the app ID
  (https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Secret.html).
  Prefer the portal: it works with no extra static permission and degrades to "generate a random
  key" when no keyring exists.
- **Windows — Credential Manager.** `CRED_MAX_CREDENTIAL_BLOB_SIZE` is `5*512` = 2560 bytes on
  Windows 7 and later (wincred.h;
  https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentiala). Keyring
  wrappers have historically produced obscure failures past that
  (https://github.com/jaraco/keyring/issues/355). A JWT access token plus a refresh token plus
  metadata can approach that ceiling, so **store only the refresh token (plus a short type/expiry
  marker) in the credential blob** and keep everything else in the encrypted config file. Verify
  the actual TIDAL token sizes before finalising.
- **macOS — Keychain.** Generic password items comfortably hold a token blob; TidalSwift and
  tidal-sdk-ios both use KeychainAccess-style generic items. Under a sandboxed/hardened build the
  keychain access group must be declared in entitlements **[STACK]**.
- **Headless/server.** There may be no keyring, no D-Bus session and no TTY. Support, in order:
  (1) an existing Secret Service if `DBUS_SESSION_BUS_ADDRESS` is set — python-tidal explicitly
  passes that through in tox for exactly this reason (ref:python-tidal/tox.ini); (2) an encrypted
  file whose key comes from `STREAMBOAT_MASTER_KEY` or a passphrase prompt; (3) a plain file at
  mode 0600 only when the user opts in with `--insecure-token-store`, logged loudly at startup.
  tidalt does the equivalent: `docker/secrets-engine` keychain first, `posixage` (age-encrypted,
  passphrase-callback) fallback at `~/.config/tidalt/secrets`, directory created 0700
  (ref:tidalt/internal/store/store.go:64-120).

#### 3.3 Encrypted file fallback: copy sone's format

`ref:sone/src-tauri/src/crypto.rs` is a good, small design worth reproducing in any language:

- AES-256-GCM, random 96-bit nonce per write.
- On-disk layout `MAGIC("SONE") || VERSION(1 byte) || NONCE(12) || CIPHERTEXT+TAG`, 17-byte header.
- `decrypt()` checks the magic; if absent it returns the input unchanged, giving free migration
  from an earlier plaintext version.
- Key resolution: OS keyring first (`keyring::Entry::new("sone", "master-key")`, `get_secret()`),
  then `<config>/sone.key` (exactly 32 bytes, `0o600`), else generate from `OsRng` — and **always
  write the file backup even when the keyring succeeds**, with the comment "keyring may be
  unreachable on next launch (e.g. AppImage with different D-Bus session)".
- The in-memory key buffer is zeroized after the cipher is constructed.

For streamboat, change two things: bump the version byte on any format change and refuse to
downgrade; and make the "write a key file even when the keyring works" behaviour a documented,
user-visible setting, because it means the encryption is only as strong as the file permissions on
that path.

#### 3.4 Client ID / client secret handling

State of the art in the references, all of which is obfuscation and none of which is security:

- **python-tidal** base64-decodes its client credentials at `tidalapi/session.py` lines ~155-162
  (per the survey) — reversible by anyone.
- **sone** XOR-encodes each value with a random pad and stores *the pad in the same file*
  (`STREAM_SALT_A` next to `CODEC_HINT_A`), regenerated by `scripts/gen_embedded.py`, with the
  variables deliberately misnamed ("stream salt", "codec hint") to avoid grepability
  (ref:sone/scripts/gen_embedded.py, ref:sone/src-tauri/src/embedded_config.rs). It ships four
  pairs (A/B = device-code id/secret, C/D = PKCE id/secret) and a `has_stream_keys()` predicate
  that treats a `PLACEHOLDER`-prefixed value as absent.
- **strawberry** encrypts build-time credentials with AES-256-CBC at CMake configure time, storing
  them as `ENC:<iv_hex>:<base64>` with the key being `SHA256(passphrase)`; CI supplies
  `-DAPI_CREDENTIALS_ENCRYPTION_KEY="$(openssl rand -hex 32)"`, i.e. a fresh key per build that is
  necessarily also present in the binary (ref:strawberry/cmake/ApiCredentials.cmake:37-78). The
  `TIDAL_CLIENT_ID` itself comes from a GitHub secret.
- **tidalt** hardcodes both values in the clear and documents them as public
  (ref:tidalt/internal/tidal/client.go, per the survey: `client_id=<client_id A>`,
  `client_secret=<redacted client_secret A; see ref:python-tidal/tidalapi/session.py>`).

**Recommendation:** design the app so a user can supply their own client id/secret in settings, and
make any embedded default an optional build input (`STREAMBOAT_CLIENT_ID` at build time, absent by
default). Never commit a credential to the repository. Do not invent a bespoke obfuscation scheme
and describe it as encryption in user-facing docs; say plainly that a shipped credential is
extractable. Where an embedded default exists, gate it behind a build flag so a distro packager can
build without it.

#### 3.5 What never gets committed, and how to enforce it

Blocklist: tokens of any kind, `client_secret` values, `.env`, session JSON, keyring dumps, HAR
files, unredacted fixture captures, `settings.json` from a real install, screenshots showing an
email address or a real library.

Enforcement:

- `.gitignore` covering `*.token`, `*.session.json`, `.env*`, `fixtures/**/raw/`, plus per-stack
  build dirs. Sone's `.gitignore` also excludes `nocommit/` as a scratch directory — a useful
  convention (ref:sone/.gitignore).
- A `pre-commit` hook that greps staged content for token-shaped strings (`ey[A-Za-z0-9_-]{20,}\.`
  for JWTs, `Bearer\s+[A-Za-z0-9._-]{20,}`, `refresh_token"\s*:` with a non-empty value). tidalt's
  hook is the model for "thin dispatcher that mirrors CI"
  (ref:tidalt/.githooks/pre-commit) — including the `git config core.hooksPath .githooks` opt-in
  and the documented `--no-verify` escape hatch.
- Secret scanning in CI (GitHub secret scanning + push protection on a public repo; gitleaks or
  trufflehog as a job for anything self-hosted).
- A documented rotation procedure: if a token leaks, the user revokes it by signing out of the
  device in TIDAL's account settings; if an embedded client id is abused, the fallback is
  user-supplied credentials.

---

### 4. Config, cache, data, logs, telemetry

#### 4.1 Paths

Use the platform convention, not XDG-everywhere. The canonical mapping (from
https://github.com/dirs-dev/directories-rs README, which encodes the same rules any language's
equivalent library follows):

| Purpose | Linux | Windows | macOS |
|---|---|---|---|
| config | `$XDG_CONFIG_HOME` or `~/.config` + `/streamboat` | `{FOLDERID_RoamingAppData}\streamboat\config` | `~/Library/Application Support/<bundle-id>` |
| cache | `$XDG_CACHE_HOME` or `~/.cache` + `/streamboat` | `{FOLDERID_LocalAppData}\streamboat\cache` | `~/Library/Caches/<bundle-id>` |
| data | `$XDG_DATA_HOME` or `~/.local/share` + `/streamboat` | `{FOLDERID_RoamingAppData}\streamboat\data` | `~/Library/Application Support/<bundle-id>` |
| local data | same as data on Linux | `{FOLDERID_LocalAppData}\streamboat\data` | same as data |
| state (logs) | `$XDG_STATE_HOME` or `~/.local/state` + `/streamboat` | no equivalent → LocalAppData | no equivalent → `~/Library/Logs/<bundle-id>` |
| runtime | `$XDG_RUNTIME_DIR/streamboat` | none | none |

Notes and traps:

- **Roaming vs local matters on Windows.** Config belongs in Roaming; caches and logs must go in
  Local, or they get synced across machines on a domain profile.
- **macOS has no XDG.** `~/Library/Application Support` for config *and* data, `~/Library/Caches`
  for cache, `~/Library/Logs` for logs. Use the bundle identifier as the directory name.
- **Do not put the cache inside the config directory.** Sone does — `<config>/sone/cache` and
  `<config>/sone/logs` (ref:sone/src-tauri/src/lib.rs:291-297, 523-531) — which means a 2 GB cache
  lands in a roaming/backed-up location. Learn from it rather than copying it.
- **Under Flatpak everything is redirected** to `~/.var/app/$FLATPAK_ID/{config,cache,data}`
  automatically for XDG dirs; a hardcoded `~/.foo` needs `--persist=.foo`, which bind-mounts to
  `~/.var/app/$FLATPAK_ID/.foo` (flatpak sandbox-permissions doc). Use the XDG APIs and this is
  free.
- **Under Snap**, `$SNAP_USER_COMMON` / `$SNAP_USER_DATA`; the `home` plug is what sone requests
  (ref:sone/snap/snapcraft.yaml).
- Support an override: `STREAMBOAT_CONFIG_DIR`, `STREAMBOAT_CACHE_DIR`, `STREAMBOAT_DATA_DIR`,
  `STREAMBOAT_LOG_DIR`, plus `--config-dir` etc. on the CLI. Headless/server deployments and the
  test suite both need it, and mopidy-tidal's integration tests do exactly this
  (`-o core/cache_dir=... -o core/data_dir=...`, ref:mopidy-tidal/integration_tests/util.py).

#### 4.2 Cache design and caps

Sone's tiering is a good starting point (ref:sone/src-tauri/src/cache.rs):

| Tier | Contents | TTL | Stale-while-revalidate grace |
|---|---|---|---|
| UserContent | playlists, favourites, liked tracks | 15 min | 1 h |
| Dynamic | artist bios, charts, home page | 4 h | 24 h |
| StaticMeta | album tracklists, credits | 7 d | 30 d |
| Image | album art, avatars | 30 d | 90 d |

Plus: `MAX_DISK_BYTES = 2 GiB`, `EVICT_TARGET = 90%` of that, LRU eviction by `last_access`, keys
hashed with SHA-256 into `<tier-subdir>/<hash>.dat` + `.meta`, entry metadata carrying
`schema_version`, `tags`, `tier`, `created_at`, `size`, and a `CacheResult::{Fresh,Stale,Miss}`
tri-state so a stale hit is served immediately while a refresh runs.

mopidy-tidal's separate *audio* cache defaults are also worth noting: `playback_cache = false`
(off by default), `playback_cache_max_entries = 1024`,
`playback_cache_buffer_bytes = 16777216` (16 MiB), backed by SQLite with LRU eviction by
`last_used` and a `manual` flag that pins entries
(ref:mopidy-tidal/mopidy_tidal/ext.conf, ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/cache.py:337-352).

For streamboat:

- Expose the cap in settings with a sane default (2 GiB metadata+images), show current usage
  (sone surfaces `CacheStats` with per-tier counts and MB), and ship a "clear cache" action.
- Keep an **audio buffer cache** strictly separate from the metadata cache, off by default, with
  its own smaller cap, and document plainly that it is a playback buffer for a logged-in
  subscriber and not a library of files. Never write full decrypted tracks to a
  user-browsable directory, never name the feature "download", and do not provide an export path.
  Sone states the boundary in its own metadata: "SONE is a streaming client only and does not
  support offline downloads" (ref:sone/data/io.github.lullabyX.sone.metainfo.xml).
- Cache entries must carry a `schema_version`; bump it and drop the tier on format change rather
  than trying to migrate.
- Never cache stream manifests across restarts — they expire (the tidal-sdk-web docs put manifest
  expiry at ~1 hour) and a stale manifest is a confusing failure. High Tide's rule is
  "Cache manifests per-session (in-memory) only".

#### 4.3 Logs and rotation

Copy sone's numbers (ref:sone/src-tauri/src/logging.rs):

- Rotate at 5 MB (`Criterion::Size(5_000_000)`), keep 9 rotated files
  (`Cleanup::KeepLogFiles(9)`) → ~50 MB ceiling, numbered naming, current file
  `streamboat_rCURRENT.log`.
- Default level spec `streamboat=debug,info` (own crate/module at debug, dependencies at info),
  overridable by `RUST_LOG`-equivalent env var **[STACK]**.
- Duplicate to stderr so `journalctl`/console users see the same stream.
- **Initialise the logger before anything else, and read the file-logging toggle from a tiny
  plaintext sidecar** (`<config>/streamboat/logging.toggle` containing literally `true`/`false`),
  because the real settings file is encrypted and not yet loadable at that point. Default to
  enabled on any read failure. Sone's implementation and its six unit tests for the toggle parser
  are a complete model.
- On every filesystem error, fall back to stderr-only rather than failing to start.

Log locations: `~/.local/state/streamboat/logs` (Linux), `%LOCALAPPDATA%\streamboat\logs`
(Windows), `~/Library/Logs/<bundle-id>` (macOS). Print the resolved path in `--version`/About so
bug reports can quote it — sone's issue template does exactly that.

#### 4.4 Telemetry and crash reporting

- **Ship with no telemetry, no analytics, no phone-home, and say so in the README, the metainfo
  and the privacy section.** This is both the norm among the references and a differentiator
  against the official client.
- The one legitimate outbound "extra" is play reporting back to TIDAL so Recently Played works.
  Sone has it on by default and disclosed under Settings → Scrobbling (ref:sone/README.md:110).
  Decide explicitly (see Open questions) and make it a visible toggle either way.
- **Update check** is a network request; make it opt-out, do it at most once per launch, and never
  send anything but the GitHub API request. Sone's implementation sends only a `User-Agent` and
  fetches the latest release tag (ref:sone/src-tauri/src/commands/updates.rs).
- **Crash reporting: do not run a crash-reporting service.** Options ranked:
  1. **Local crash dumps + a "generate debug bundle" button** (recommended). No server, no
     consent problem, no PII exfiltration risk.
  2. Self-hosted collector (Sentry self-hosted / GlitchTip) — only with explicit opt-in at first
     run, a documented retention period, and scrubbing of paths, usernames and URLs.
  3. Hosted Sentry — contradicts the no-telemetry stance; avoid.
  Note that a crash dump from an audio app can contain decoded PCM in memory. If you ship
  minidumps, exclude heap by default.

---

### 5. Licensing

#### 5.1 What the references chose

| Project | License | Notes |
|---|---|---|
| sone / sone-windows | GPL-3.0-only | Declared in `package.json`, `Cargo.toml`, PKGBUILD, snapcraft.yaml and metainfo `<project_license>` |
| High Tide | GPL-3.0 (COPYING); individual files LGPL-3.0-or-later headers | CONTRIBUTING: "Contributions should be licensed under the **GPL-3**" |
| Strawberry | GPL-3.0 | Qt + GStreamer app |
| tidal-hifi | MIT | wraps the web player; ships castlabs Electron with Widevine |
| python-tidal | LGPL-3.0-or-later | library, so weak copyleft |
| mopidy-tidal | Apache-2.0 | server plugin |
| tidalt | Apache-2.0 | statically links FFmpeg |
| tidalrs, libopenTIDAL, tidal-cli | MIT | libraries/CLI |
| tidal-sdk-{web,ios,android} | Apache-2.0 | official |
| TidaLuna | Ms-PL | mod for the official client |
| dotnet-tidal-usdk | MIT **with an added anti-piracy clause** | "The software cannot be used for piracy purposes; this includes using the software to create copies (or 'back-ups') of intelluctual property provided by TIDAL without the copyright holder's ... express permission" (ref:dotnet-tidal-usdk/LICENSE.md) |

The dotnet-tidal-usdk clause is a cautionary example: bolting a use restriction onto MIT makes the
license non-OSI, non-free, and unpackageable by Debian/Fedora/Flathub. Achieve the same effect with
a README/metainfo statement of scope and a design that has no ripping feature, exactly as sone does
in its Disclaimer (ref:sone/README.md:540-544).

#### 5.2 Choosing for streamboat

- **GPL-3.0-only** is the default recommendation for the app. It matches every comparable Linux
  TIDAL client, it is compatible with GStreamer's LGPL core *and* with the GPL plugins in
  `gst-plugins-ugly`/`-bad` (which an LGPL/permissive app cannot use), it is compatible with Qt's
  open-source terms, and it prevents a closed fork of a project whose whole value is being open.
- **Split the license by layer.** Make the reusable core library (`streamboat-core`: API client,
  auth, manifest parsing, models) **LGPL-3.0-or-later or Apache-2.0**, and the applications
  (desktop UI, headless daemon) **GPL-3.0-only**. Precedent: python-tidal is LGPL, the apps on top
  of it are GPL; the TIDAL SDKs are Apache-2.0. A permissive core maximises the chance other
  clients adopt it, which is the fastest route to shared maintenance of an unofficial API surface.
  Note the one-way door: relicensing later requires every contributor's agreement unless you
  collect a CLA/DCO with relicensing rights, which most contributors dislike.
- **AGPL-3.0 is a poor fit.** Its network clause only bites when users interact with the software
  over a network. streamboat's headless mode does expose a local control surface, so AGPL is not
  meaningless — but it would deter packagers and integrators (Music Assistant, Mopidy, HA-style
  projects) for a benefit that barely applies to a single-user player. If the owner cares about
  hosted forks, AGPL for the *server* component only is the narrower option.
- **MIT/Apache-2.0 for the whole app** is only right if the goal is maximum adoption including by
  closed products. It also forecloses GPL GStreamer plugins and would make the project the
  path of least resistance for a closed commercial TIDAL client.

#### 5.3 Dependency compatibility notes

- **GStreamer**: core, base, good are LGPL-2.1. "GStreamer demands plugins be licensed under the
  LGPL, even when they are using a GPL library"; plugins with patent issues live in
  `gst-plugins-ugly`; "When using GPL linked plugins, GStreamer is for all practical reasons under
  the GPL itself"
  (https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/licensing.html). A
  GPL-3.0 streamboat has no problem here. A permissive streamboat would have to restrict itself to
  LGPL plugins and an LGPL FFmpeg build.
- **FFmpeg**: LGPL-2.1+ by default; `--enable-gpl` and `--enable-nonfree` change that. tidalt
  builds FFmpeg 7.1.5 with `--disable-everything --disable-programs --disable-doc
  --disable-network --disable-autodetect --disable-shared --enable-static --enable-pic
  --disable-avdevice --disable-swscale --disable-avfilter --enable-protocol=file
  --enable-demuxer=flac,mov,aac,wav,ogg
  --enable-decoder=flac,aac,aac_latm,alac,pcm_s16le,pcm_s24le,pcm_s32le,vorbis
  --enable-parser=flac,aac,aac_latm,vorbis --enable-swresample` — no `--enable-gpl`, so the result
  is LGPL (ref:tidalt/packaging/build-static-ffmpeg.sh). **Static linking of LGPL code from an
  Apache-2.0 binary imposes the LGPL relinking obligation** (ship object files or the full source
  and build instructions). If streamboat statically links FFmpeg, publish the exact build script
  and the object archives, or dynamically link.
- **fdk-aac**: GPL-incompatible and "therefore nondistributable with GPL parts"; Debian ships it as
  non-free (https://fedoraproject.org/wiki/Licensing/FDK-AAC). streamboat needs AAC *decoding*
  only, which `avdec_aac` (LGPL FFmpeg) or `faad` covers; never link fdk-aac.
- **Qt 6** open source is mainly LGPLv3 with some modules GPL-only; some modules are GPL-2.0-only
  and others GPL-3.0-only, and mixing those two is itself a violation
  (https://doc.qt.io/qt-6/licensing.html, https://www.qt.io/faq/qt-open-source-licensing). A
  GPL-3.0-only app can use LGPLv3 and GPL-3.0-only Qt modules but must avoid GPL-2.0-only ones.
  Static linking against LGPLv3 Qt carries the relinking obligation.
- **GTK4/libadwaita** are LGPL-2.1 — no constraint on a GPL app.
- **Electron/Chromium** is largely BSD/MIT; the castlabs Widevine build that tidal-hifi uses
  bundles a proprietary CDM, which is why that route is architecturally different and why
  streamboat should not go there (it also implies DRM handling the project has said it will not
  design around).
- **Third-party license inventory:** run a license scanner in CI. TIDAL's own SDKs use FOSSA
  (ref:tidal-sdk-web/.github/workflows/fossa-scan.yml,
  ref:tidal-sdk-android/.fossa.yml — which is worth reading for the pattern of restricting the scan
  to *shipped* runtime classpaths so build tooling does not gate PRs). Free alternatives:
  `cargo-deny` (Rust), `pip-licenses`/`reuse` (Python), `license-checker` (npm), `go-licenses`
  (Go), plus REUSE-compliant `LICENSES/` + SPDX headers.
- **Flathub requires the license file of every module to be installed** to
  `$FLATPAK_DEST/share/licenses/$FLATPAK_ID` and the metainfo `<project_license>` to match the
  source (https://docs.flathub.org/docs/for-app-authors/requirements).

#### 5.4 Trademark and positioning (licensing-adjacent, affects packaging)

Flathub is explicit: "Official affiliation must not be implied by using a vendor's name in the
application name or icon unless the application is actually part of that vendor's project", with
the example "A WhatsApp client or wrapper cannot have `WhatsApp` in its name or use any of the
official icon, logo or artwork". "streamboat" is already safe. Keep TIDAL out of the name, the
icon, and the app ID; use it only in the description ("client for TIDAL", "requires an active
TIDAL subscription"), which every accepted client does. Sone's wording is the template:
"SONE is an independent, community-driven project. It is **not affiliated with, endorsed by, or
connected to TIDAL** in any way. All content is streamed directly from TIDAL's service and
requires a valid paid subscription. SONE is a streaming client only — it does not support offline
downloads, and does not redistribute or circumvent protection of any content. As with any
third-party client, please be aware of TIDAL's terms of use." (ref:sone/README.md:540-544)

One unverified but material data point: a search result attributes to TIDAL's Developer Terms 2.0
the statement that "The Player module in the SDK constitutes the only allowed way for third-party
applications to incorporate playback of TIDAL content". `developer.tidal.com` and `tidal.com` are
both unreachable from this environment, so **this is not verified from the primary source** and
must be checked by the legal/ToS research topic before any claim is published.

---

### 6. Distribution and packaging

#### 6.1 Flathub (Linux desktop — the primary channel)

Requirements that bite a TIDAL client, all from
https://docs.flathub.org/docs/for-app-authors/requirements (read at HEAD 2026-09-07 from the
flathub-infra/documentation repository):

- **Console software will not be accepted.** The headless/CLI mode cannot be a Flathub app. Ship
  it via deb/rpm/AUR/Docker; the Flatpak carries the GUI only (it may still contain the daemon
  binary for local use).
- **"Insufficient development history"**: submissions "must demonstrate a meaningful history of
  development or existence, evidence of real-world use and a clear commitment to ongoing
  maintenance ... a sustained commit history and standard release practices such as tagged
  versions. Applications that have only existed for a very short period of time will generally not
  be accepted." Plan Flathub submission for several months and several tagged releases in.
- **App ID**: reverse-DNS, ≥3 components, ≤5; GitHub-hosted projects must use `io.github.` and have
  ≥4 components; the domain portion must be lowercase with `-` → `_`; the ID must exactly match
  the metainfo `<id>`; the derived repository URL must be reachable. For a GitHub repo
  `github.com/<user>/streamboat` the ID is `io.github.<user>.streamboat`.
- **Metainfo is mandatory** and must pass validation; screenshots are required and the linter
  rule `metainfo-missing-screenshots` is "never granted".
- **Complete English localisation is mandatory** for UI, desktop file, metainfo and docs.
- **Permissions minimal, portals mandatory where they exist**: "When a suitable XDG portal exists,
  is supported across the ecosystem, and covers the application's use case ... using the portal
  becomes mandatory instead of static permissions."
- **No network access during build**; all dependencies must be declared as sources; **build
  entirely from source**; **stable releases only** (no nightlies); manifest at top level named
  after the app ID; `flathub.json` if not building on both x86_64 and aarch64.
- **Generative AI policy** (quoted in full because it is a project-level constraint): submitters
  "must disclose any AI-generated code, documentation, packaging, or other material they know or
  reasonably believe is included in the application or its Flathub packaging. The disclosure must
  identify the affected parts and approximate extent. AI used only for research, discussion, or
  debugging does not need disclosure when no generated material is included ... Reviewers may
  reject a submission, including without further review, based on the extent or role of generated
  material ... AI tools or agents must not open or automate Flathub submission pull requests, or
  generate their commit messages, descriptions, review comments, or replies. Submitters must not
  request AI-agent reviews. Undisclosed or materially misrepresented AI-generated material ... may
  result in rejection. Repeated violations may result in a permanent ban from future submissions."

**Permissions a TIDAL client actually needs.** High Tide's accepted manifest is the reference
(ref:high-tide/build-aux/io.github.nokse22.high-tide.json):

```
"--share=network",                          # required: all API + CDN traffic
"--share=ipc",                              # required alongside X11 (linter: finish-args-x11-without-ipc)
"--socket=wayland",
"--socket=fallback-x11",                    # never both x11 and fallback-x11
"--device=dri",                             # GPU rendering
"--socket=pulseaudio",                      # audio out AND /dev/snd (raw ALSA)
"--filesystem=xdg-run/pipewire-0:ro",       # direct PipeWire access
"--filesystem=xdg-run/discord-ipc-0"        # only if Discord RPC is a feature
```

Add for streamboat, if the features exist:

- `--talk-name=org.freedesktop.secrets` (lowercase!) for the host keyring, **or** rely on the
  Secret portal, which needs no permission.
- `--own-name=org.mpris.MediaPlayer2.<something>` — allowed by default only when it exactly
  matches the Flatpak ID, is an exact subname of it, or is an MPRIS subname
  (linter rule `finish-args-own-name-cpt`). Never use `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID`
  (never-granted rule).
- **Do not request `--device=all`.** `--socket=pulseaudio` already covers `/dev/snd`, which is what
  exclusive ALSA needs. Requesting `--device=all` invites reviewer pushback and is called out in
  Flatpak's own docs as excessive.
- Note `--filesystem=home` and friends are heavily linted; streamboat should need no home access
  at all.

**Release automation.** Sone's `flathub-update.yml` is a complete, copyable pipeline
(ref:sone/.github/workflows/flathub-update.yml): on a `v*` tag it validates the tag against
`^v[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$`, resolves the annotated tag to a commit with
`git ls-remote "$REPO" "$TAG^{}"`, clones `flathub/<app-id>`, rewrites `tag` and `commit` in the
manifest with `yq`, regenerates dependency manifests with a **pinned** clone of
`flatpak/flatpak-builder-tools` (commit `ac5a296ac6111aa2319daf532f609a067b88d8a9`) —
`flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json` and
`flatpak-node-generator pnpm --pnpm-store-version v11 pnpm-lock.yaml -o pnpm-sources.json` — and
opens a PR, skipping if one already exists for that tag. **[STACK]** the generator you need
depends on the language.

**CI-side build check**: High Tide builds the Flatpak on every push and PR for both x86_64 and
aarch64 using `flatpak/flatpak-github-actions/flatpak-builder@v6` inside
`ghcr.io/flathub-infra/flatpak-github-actions:gnome-48`
(ref:high-tide/.github/workflows/flatpak.yml). Do this from day one — it is the cheapest way to
keep the manifest honest.

#### 6.2 Snap

Sone's `snapcraft.yaml` (ref:sone/snap/snapcraft.yaml) shows the audio-specific parts:

- `confinement: strict`, `base: core24`, `grade: stable`.
- Plugs: `home`, `network`, `network-bind`, `network-status`, `audio-playback`, `alsa`,
  `screen-inhibit-control`.
- **`alsa` is not auto-connected**: the description tells users "For exclusive ALSA output, run:
  `sudo snap connect sone:alsa`". Budget for that support burden or treat exclusive mode as
  unsupported under Snap.
- MPRIS needs a **dotless** slot name: `slots: { mpris: { interface: mpris, name: sone } }`, and
  the app must own `org.mpris.MediaPlayer2.<that name>` — sone detects it via the `SNAP` env var.
- Staging GStreamer requires `layout:` binds for `gstreamer-1.0`, `alsa-lib` and `/usr/share/alsa`,
  plus `GST_PLUGIN_SYSTEM_PATH`.

#### 6.3 deb / rpm / AUR

Two working patterns:

- **Docker-per-distro (sone).** `build-scripts/build/Dockerfile.{deb,pacman,rpm,rpm-opensuse}` plus
  `deb.sh` etc.; the deb builds on Ubuntu 22.04 to get the oldest supported glibc, and the
  post-build step runs `dpkg-deb -I` and greps for `Package|Version|Depends|Section|Priority` to
  assert dependencies are declared (ref:sone/build-scripts/build/deb.sh). The Arch PKGBUILD simply
  extracts the built `.deb` into `$pkgdir` (ref:sone/build-scripts/build/PKGBUILD).
- **docker buildx bake (tidalt).** One `docker-bake.hcl` produces deb, rpm, Arch and container
  images for amd64 + arm64 in one release job, with `--set "*.output=type=local,dest=dist"`
  (ref:tidalt/.github/workflows/release.yml).

Both keep hand-written packaging metadata in-tree: `packaging/debian/{control,rules,copyright,
changelog,compat}` and `packaging/fedora/tidalt.spec` (ref:tidalt/packaging/). Note tidalt's
`Depends: ${shlibs:Depends}, ${misc:Depends}, libasound2t64, dbus` and
`Recommends: playerctl` — a media player's dependency list should name the audio stack explicitly.

**Repository hosting.** Sone publishes to **Cloudsmith** (free OSS plan, badge required in the
README) with a three-line layout: `.deb` → `<repo>/any-distro/any-version`, Fedora `.rpm` →
`<repo>/fedora/any-version`, openSUSE `.rpm` → `<repo>/opensuse/any-version`, "because Fedora and
openSUSE use different dependency names" (ref:sone/build-scripts/publish-cloudsmith.sh). Users add
it with `curl -1sLf 'https://dl.cloudsmith.io/public/<org>/<repo>/setup.deb.sh' | sudo -E bash`.
The alternative is openSUSE Build Service (OBS), which builds for many distros from a spec but is
more work to drive from GitHub Actions. Strawberry additionally maintains an **Ubuntu PPA**
(`upload-ubuntu-ppa` job).

**AUR.** Two packages is the convention: `<name>` (build from source) and `<name>-bin` (prebuilt),
as sone does. Automate with `KSXGitHub/github-actions-deploy-aur@v4.1.3` driven by an
`AUR_SSH_PRIVATE_KEY` secret, with the workflow skipping cleanly when the key is absent so forks
do not fail (ref:tidal-hifi/.github/workflows/release.yml, `publish_aur` job).

#### 6.4 Windows

- **Installer format**: tidal-hifi produces an MSI via electron-builder (`win: target: msi`);
  Strawberry ships an NSIS installer (`dist/windows/strawberry.nsi.in` with
  `Capabilities.nsh`, `FileAssociation.nsh`, `Registry.nsh`). MSIX is required only for the
  Microsoft Store and forces a signed package plus a store listing; skip it initially.
- **Signing**: unsigned installers hit SmartScreen. Options in 2026: Azure Trusted Signing /
  Azure Artifact Signing at $9.99/month Basic (5,000 signatures) — available to individuals in
  the USA and Canada and to organisations in the EU/UK — versus an EV code-signing certificate at
  roughly $280–500/year on a hardware token. Trusted Signing certificates cannot be exported, so
  losing eligibility means losing the ability to sign.
- **winget**: submit a manifest PR to `microsoft/winget-pkgs`. `InstallerSha256` is required and is
  verified against the downloaded installer; an automated validation pipeline installs and tests
  the package; a PR assigned to you has a 7-day timer before the bot closes it
  (https://learn.microsoft.com/en-us/windows/package-manager/package/repository). Generate the
  manifest with `wingetcreate`; automate updates from the release workflow once the release assets
  have stable names.
- No reference project auto-updates on Windows; electron-builder's `publish`/`autoUpdater` is not
  configured in tidal-hifi's build configs.

#### 6.5 macOS

- **DMG + Developer ID + notarization is the only usable path.** Strawberry's sequence:
  `apple-actions/import-codesign-certs@v7` with `APPLE_DEVELOPER_ID_CERTIFICATE` /
  `..._PASSWORD` → build with `-DAPPLE_DEVELOPER_ID=<team-id>` → `make deploy` → `codesign --deep -v
  strawberry.app` → `make dmg` → `xcrun notarytool store-credentials` with
  `APPLE_NOTARIZATION_APPLE_ID` / `..._PASSWORD` and the team id →
  `xcrun notarytool submit *.dmg --keychain-profile <profile> --wait` →
  `xcrun stapler staple *.dmg` (ref:strawberry/.github/workflows/build.yaml:1178-1265). Every
  signing step is guarded on `github.repository == '<upstream>'` and
  `github.event.pull_request.head.repo.fork == false`.
- **Cost**: Apple Developer Program $99/year covers the Developer ID certificate and unlimited
  notarization.
- **Entitlement**: a GStreamer app needs `com.apple.security.cs.allow-jit` under the Hardened
  Runtime because liborc JIT-compiles SIMD at runtime
  (ref:strawberry/dist/macos/strawberry.entitlements). Expect to discover one or two more
  (`allow-unsigned-executable-memory`, `disable-library-validation`) depending on how plugins are
  loaded **[STACK]**.
- **Both architectures**: Strawberry builds on `macos-15-intel` and `macos-15` runners and
  pins `MACOSX_DEPLOYMENT_TARGET` from the newest installed SDK.
- **Homebrew cask**: notability floor is 30 forks / 30 watchers / 75 stars, or 90 / 90 / 225 for a
  self-submission by the repo owner; a repository under 30 days old is normally ineligible; casks
  must pass Homebrew's Gatekeeper audit and "must not require System Integrity Protection or
  Gatekeeper to be disabled or bypassed", and casks failing those checks are deprecated, disabled
  and removed (https://github.com/Homebrew/brew/blob/main/docs/Package-Acceptance-Policy.md,
  .../Acceptable-Casks.md, .../Homebrew-Security-and-Supply-Chain.md). In short: **no
  notarization, no Homebrew.** Ship a personal tap in the meantime.
- **Auto-update**: Sparkle is the standard and Strawberry enables it (`-DENABLE_SPARKLE=ON`).
  Sparkle needs an EdDSA signing key and an appcast feed; keep it macOS-only.

#### 6.6 Auto-update strategy overall

Recommended: **no silent in-app updater on any platform initially.**

- Linux: package managers and Flathub/Snap update themselves.
- Windows: winget update; document it.
- macOS: Sparkle later, once notarization is in place.
- All platforms: a passive check against
  `https://api.github.com/repos/<owner>/streamboat/releases/latest`, semver-compared, surfacing a
  "version X is available" notice with a link — failures treated silently as "no update"
  (ref:sone/src-tauri/src/commands/updates.rs). Make it opt-out in settings and skip it entirely
  when the app detects it is running from a managed package (Flatpak/Snap/deb) — those users get
  updates elsewhere and the check is pure noise.

#### 6.7 Versioning, changelog, release cadence

- **SemVer 2.0.0** (https://semver.org/spec/v2.0.0.html), tags as `vMAJOR.MINOR.PATCH`. Stay on
  `0.x` until the core library API is stable; the desktop app's user-visible behaviour is what
  MINOR/PATCH should track after 1.0.
- **Keep a Changelog 1.1.0** (https://keepachangelog.com/en/1.1.0/) with an `## [Unreleased]`
  section and `### Added/Changed/Fixed/Removed/Deprecated/Security` groups — exactly the format
  tidal-sdk-ios uses, with a per-entry module scope suffix like `(Player)`, `(TidalAPI)`
  (ref:tidal-sdk-ios/CHANGELOG.md).
- **Enforce the changelog in CI.** tidal-sdk-ios runs `check-version-sync.sh` on every PR touching
  `version.txt`; it is four lines: `grep -q "$(cat ./version.txt)" ./CHANGELOG.md`
  (ref:tidal-sdk-ios/.agents/skills/prepare-release/scripts/check-version-sync.sh). tidal-sdk-web
  does the equivalent per-package with `bin/check-version-bump.sh` and a matrix over changed
  `packages/**/package.json` (ref:tidal-sdk-web/.github/workflows/check-changelog-files.yml).
- **Conventional Commits 1.0.0** (https://www.conventionalcommits.org/en/v1.0.0/). tidal-hifi asks
  contributors to "try to use conventional commit format when possible (e.g. `feat:`, `fix:`,
  `docs:`, `refactor:`)" (ref:tidal-hifi/CONTRIBUTING.md); Strawberry instead uses
  `ClassName: Short explanation` with no trailing period and a prose body for larger changes
  (ref:strawberry/CONTRIBUTING.md). Pick one and enforce it with a commit-msg hook +
  `commitlint`-equivalent in CI; conventional commits are the better choice because they let a
  script draft the changelog (tidal-sdk-ios's `suggest-changelog.sh` categorises by leading verb
  precisely because its history is not conventional).
- **Cadence**: sone shipped 0.7.0 → 0.21.0 between 2026-03-07 and now, i.e. roughly weekly minor
  releases with dated `<release>` entries in the metainfo
  (ref:sone/data/io.github.lullabyX.sone.metainfo.xml). That pace is realistic for a solo project
  and it keeps the Flathub `<releases>` block meaningful. Note Flathub forbids nightlies and
  "software requiring daily updates" in the stable repo.
- **Every release ships checksums.** tidalt generates `sha256sum * > checksums.txt` and pastes it
  into the release body with verification instructions (ref:tidalt/.github/workflows/release.yml).
  Add build provenance attestation (`actions/attest-build-provenance`, or npm
  `--provenance` as tidal-cli does) once the release pipeline is stable.

---

### 7. CI

#### 7.1 Job matrix

Minimum viable set, all triggered on push + PR:

| Job | Runner | Gate |
|---|---|---|
| lint + format | ubuntu-latest | formatter `--check`, linter with warnings-as-errors |
| typecheck | ubuntu-latest | **[STACK]** mypy / tsc / clippy / detekt |
| unit tests (offline) | ubuntu-latest, windows-latest, macos-latest | must pass with no secrets and no network |
| build | ubuntu, windows, macos | release profile |
| flatpak build | ubuntu-24.04 + ubuntu-24.04-arm | manifest validity |
| spellcheck | ubuntu-latest | codespell |
| license scan | ubuntu-latest | disallowed-license list |
| dependency audit | ubuntu-latest | known-vuln advisory database |

Plus scheduled jobs: daily API-spec diff (§2.4), weekly live canary on a self-hosted/maintainer
runner (§2.5), weekly full-matrix rebuild (mopidy-tidal runs its integration suite on
`cron: "0 0 * * 0"`).

Notes drawn from the references:

- Strawberry demonstrates the far end: 13 distinct build jobs across openSUSE (Tumbleweed + Leap),
  Fedora, OpenMandriva, Mageia, Debian, Ubuntu (+PPA upload), AppImage, FreeBSD, OpenBSD, macOS
  ×2 architectures, Windows MinGW and Windows MSVC. Do not start there; add distros when a bug
  report proves you need them.
- `fail-fast: false` on every matrix — mopidy-tidal comments "Run all the matrix jobs, even if one
  fails."
- Pin third-party actions to a commit SHA for anything that touches secrets or publishes
  (tidal-sdk-web pins `cypress-io/github-action@09090944...`, `tj-actions/changed-files@9426d409...`,
  `fossas/fossa-action@29693cc5...`; sone pins `actions/checkout@34e11487...`). Renovate/Dependabot
  keep the pins current.
- Set `permissions:` explicitly per workflow (`contents: read` by default, `contents: write` only
  on release) — tidalt and tidal-hifi both do.
- `timeout-minutes` on every job (tidal-sdk-web uses 15 for unit tests; tidalt uses 10 for the
  clang-tidy job "so a stalled apt-get fails fast").

#### 7.2 Caching

- Package-manager caches keyed on the lockfile: `actions/setup-node` `cache: npm|pnpm`,
  `astral-sh/setup-uv` with `enable-cache: true` and a `cache-suffix` per matrix leg
  (ref:mopidy-tidal/.github/workflows/test.yml), `actions/setup-go` with `go-version-file: go.mod`,
  `actions/setup-python` `cache: 'poetry'`.
- Docker BuildKit cache mounts for compiled dependencies: sone's deb Dockerfile mounts
  `/root/.cargo/registry`, `/root/.cargo/git` and `/app/src-tauri/target` as named caches
  (ref:sone/build-scripts/build/Dockerfile.deb).
- Cache heavy test binaries separately with restore/save (tidal-sdk-web caches `~/.cache/Cypress`
  keyed on the lockfile hash and saves with `if: always()`).
- Flatpak: `flatpak-builder` action takes a `cache-key`.

#### 7.3 Gates, and what a hook can do instead

tidalt's `.githooks/pre-commit` is the model: a "thin dispatcher [that] delegates to the same
tooling the CI workflows run ... so 'passes the hook' and 'passes CI' stay in lockstep. Slow gates
— the full test suite, vulnerability scanning — are deliberately left to CI so a commit never
blocks for minutes." It checks only staged files, reports format drift without mutating the tree,
and documents `git commit --no-verify` (ref:tidalt/.githooks/pre-commit). Activation is opt-in via
`make hooks` → `git config core.hooksPath .githooks`.

Alternative: `pre-commit` framework with pinned hook revisions, as tidal-sdk-ios does — SwiftFormat
0.54.6, SwiftLint 0.57.0 twice (once `--fix`, once `--strict`), plus
`check-jsonschema`'s `check-github-actions` and `check-github-workflows` to validate workflow YAML
(ref:tidal-sdk-ios/.pre-commit-config.yaml). The workflow-schema check is worth copying regardless
of stack.

#### 7.4 Security scanning and dependency policy

- **Dependency updates**: Renovate on a weekly schedule with minor+patch grouped into one PR is
  what TIDAL's Android SDK uses (`"schedule": ["on sunday"]`, `groupName: "all non-major
  dependencies"`, ref:tidal-sdk-android/renovate.json). Strawberry uses Dependabot for
  `github-actions` on a daily interval (ref:strawberry/.github/dependabot.yaml). Recommend:
  Renovate for language dependencies (weekly, grouped), Dependabot for GitHub Actions (weekly),
  and a documented policy that majors are handled manually with a changelog note.
- **Vulnerability scanning**: **[STACK]** `cargo audit`/`cargo deny` (Rust), `pip-audit` (Python),
  `npm audit --audit-level=high` / `osv-scanner` (JS), `govulncheck` (Go). `osv-scanner` covers all
  of them from one lockfile-aware tool and is the pragmatic default. Run it on PRs and nightly;
  fail on high/critical with a documented allowlist file for accepted risks.
- **Static analysis**: CodeQL on the default branch (free for public repos). tidal-hifi also wires
  SonarCloud (`.sonarcloud.properties`).
- **Supply chain**: pin actions by SHA (above), enable branch protection with required checks,
  require signed commits or at least DCO, enable GitHub secret scanning + push protection, publish
  build provenance attestations, and publish `checksums.txt` with every release.
- **License scanning**: see §5.3.

#### 7.5 Reproducible builds

Full bit-for-bit reproducibility is a large project; the achievable subset:

- Set `SOURCE_DATE_EPOCH` from the tag's commit date in every packaging job.
- Commit and use lockfiles everywhere; build with `--frozen-lockfile` / `--locked`.
- Pin toolchain versions in-tree (`rust-toolchain.toml`, `.nvmrc`, `go.mod` `go` directive,
  `.python-version`) and have CI read them rather than hardcoding.
- Build release artifacts inside a pinned container image (sone's Dockerfiles pin
  `ubuntu:22.04` and `pnpm@11.1.3`; tidalt pins `FFMPEG_VERSION=7.1.5`).
- Publish the exact build command and container digest in the release notes.
- Strip and normalise: `-ldflags="-s -w"` (Go), `strip = true` in the Rust release profile, and
  avoid embedding absolute build paths.
- Verify by rebuilding one release from the tag on a clean machine and diffing hashes; document
  the result even when it is "not yet reproducible, differs in X".

---

### 8. Repository layout, docs and contribution process

#### 8.1 Workspace layout

Target shape (names **[STACK]**-adjusted, structure is not):

```
/
├── README.md                  # what it is, install, disclaimer, no marketing
├── LICENSE / LICENSES/        # SPDX-named files; REUSE-compliant
├── CHANGELOG.md               # Keep a Changelog 1.1.0
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── SECURITY.md
├── crates|packages|src/
│   ├── core/                  # API client, auth, models, manifest parsing — permissive license
│   ├── audio/                 # pipeline, sinks, gapless, ReplayGain — platform-conditional
│   ├── daemon/                # headless: MPRIS/D-Bus + local control API
│   ├── cli/                   # thin client over daemon; also the debug/diagnostic tool
│   └── desktop/               # GUI
├── contracts/                 # vendored OpenAPI + hand-written v1 JSON Schemas
├── tests/
│   ├── fixtures/api/          # redacted captured responses + .meta.json
│   ├── fixtures/audio/        # short synthetic FLAC/AAC/PCM goldens
│   └── live/                  # opt-in canary suite
├── packaging/
│   ├── flatpak/               # manifest + generated sources (mirrored to flathub repo)
│   ├── debian/  fedora/  arch/
│   ├── snap/
│   ├── windows/  macos/
│   └── scripts/
├── data/                      # .desktop, metainfo.xml, icons, gschema
├── po/                        # translations
├── docs/
│   ├── research/              # this file and its siblings
│   ├── DECISIONS.md           # ADR log
│   ├── architecture.md
│   ├── testing/               # audio-matrix.md, fixtures.md, live-tests.md
│   ├── packaging.md
│   └── legal.md               # ToS posture, trademark, scope boundaries
├── .claude/skills/            # repo-specific agent skills (release, fixture capture, review)
├── .github/
│   ├── workflows/
│   ├── ISSUE_TEMPLATE/
│   ├── PULL_REQUEST_TEMPLATE.md
│   ├── dependabot.yml
│   └── renovate.json
└── .githooks/pre-commit
```

Precedents: tidal-sdk-web and TidaLuna use pnpm workspaces with a `packages/` dir plus a
`template/` package used to scaffold new modules; tidal-sdk-android uses Gradle modules
(`auth`, `player`, `common`, `eventproducer`, `tidalapi`, `bom`) with a `generate-module.sh`;
tidalt uses `cmd/` + `internal/{tidal,player,ui,store,spotify}` + `packaging/` + `docs/`;
TidalSwift separates `TidalSwiftLib/` from `TidalSwift/` and its own lesson is to "port this
pattern to separate streamboat-core library from platform-specific frontends".

**Mobile-readiness without building mobile now:** keep `core` free of any UI, filesystem-layout or
process-model assumptions — it takes a `Storage` trait/interface and an HTTP client, and returns
data. Sone's Tauri entry point already carries `#[cfg_attr(mobile, tauri::mobile_entry_point)]`
(ref:sone/src-tauri/src/lib.rs) as an example of leaving the door open. Do not add mobile build
targets, mobile CI, or mobile packaging yet.

#### 8.2 Docs and ADRs

- `docs/DECISIONS.md` as an append-only ADR log: one numbered entry per decision with Context /
  Decision / Consequences / Status(Proposed|Accepted|Superseded by ADR-N) / Date. Cross-link from
  the code where the decision is embodied.
- `docs/research/*.md` (this directory) stays as the evidence base; ADRs cite it.
- `docs/` should carry a per-topic file rather than one giant page — tidalt's `docs/` is a good
  size and shape: `architecture.md`, `installation.md`, `development.md`, `debugging.md`,
  `dac-compatibility.md`, `media-keys.md`, `mpris2.md`, `docker.md`, `client-server.md`,
  `ui.md` (ref:tidalt/docs/).
- `.claude/skills/` is a real, in-use convention in this ecosystem: tidal-sdk-ios ships
  `.agents/skills/prepare-release/SKILL.md` with `scripts/{bump-version,check-release-needed,
  check-version-bump,check-version-sync,extract-release-notes,suggest-changelog}.sh`, and **CI
  invokes the same scripts from the same paths** (`.agents/skills/prepare-release/scripts/
  check-version-sync.sh` in `changelog-check.yml`). tidal-sdk-android ships `.agents/checks/*.md`
  as one-file-per-review-rule with severity frontmatter, explicitly "additive" to existing
  linters and formatters. Adopt both patterns: skills that wrap real scripts CI also runs, and
  review rules that never re-litigate what the formatter owns.
- tidal-cli ships `skills/tidal-cli/SKILL.md` and publishes it as a distributable artifact in its
  release workflow — evidence that a skill can be a shipped deliverable, not just repo furniture.

#### 8.3 CONTRIBUTING, code of conduct, templates

Take from High Tide's CONTRIBUTING (ref:high-tide/CONTRIBUTING.md): licensing statement, where to
ask questions (Matrix), an explicit style section (naming conventions, indentation, max line
length 88, widget prefix convention), threading rules with a code example, error-handling rules
with a code example, memory/signal-disconnection rules, and an accessibility subsection ("When
using symbolic icons always add a tooltip", "Remember to mark all user facing strings as
translatable"). That last one is the model — put accessibility and i18n obligations in
CONTRIBUTING, not in a wiki nobody reads.

Take from tidal-hifi's CONTRIBUTING (ref:tidal-hifi/CONTRIBUTING.md): PRs target `develop` not
`master`; "Limit each pull request to one specific change"; "Keep refactoring, renaming, and
cleanup work in separate PRs"; "Don't update version files — the maintainers will handle this
during the release process"; and an explicit **AI usage policy**: "AI assistance is allowed for
development, but you are fully responsible for: the quality of code you submit, understanding how
your code works, responding to feedback and questions about your implementation, ensuring the code
follows project standards and patterns." streamboat needs its own AI policy in CONTRIBUTING, and
it must be consistent with Flathub's disclosure requirement (§6.1).

**Code of conduct**: adopt Contributor Covenant 2.1 verbatim with a real contact address.
python-tidal ships `CODE_OF_CONDUCT.md`; most of the others do not.

**Issue templates**: use GitHub's YAML forms with required fields, as tidal-hifi does — installation
method (dropdown), distro, version, pre-submission checklist with `required: true`
(ref:tidal-hifi/.github/ISSUE_TEMPLATE/bug_report.yml). Ship four: bug, playback/audio issue,
feature request, question. **The playback template is the important one** — copy sone's field list
verbatim (version, install source, distro+version, desktop/session, GPU+driver, output device,
sample rate/bit depth, exclusive-mode on/off, affected tracks, log path)
(ref:sone/.github/ISSUE_TEMPLATE/playback_issue.md).

**SECURITY.md**: state supported versions, a private reporting channel (GitHub private
vulnerability reporting), and an explicit scope note that token handling and the local control API
are in scope while "TIDAL's own service" is not.

**Branch strategy**: `main` protected, feature branches, squash merge, release tags on `main`. A
`develop` branch (tidal-hifi) only pays off with several contributors and a slow release train;
for a solo/small project, trunk plus tags is simpler and matches sone, tidalt and the SDKs.

---

### 9. Internationalisation and accessibility

#### 9.1 i18n

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
  (ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:76). python-tidal hardcodes
  `self.locale = "en_US"` with the comment `# TODO Get locale from system configuration`
  (ref:python-tidal/tidalapi/session.py:404, 671), and sone passes `("locale", "en_US")` on ~10
  call sites (ref:sone/src-tauri/src/tidal_api.rs). **streamboat should pass the user's actual
  locale** — that makes TIDAL's editorial page titles, mix names and descriptions come back
  localised for free, which is a bigger win than translating streamboat's own chrome. The exact
  set of locales TIDAL accepts is not documented in any reference checkout; determine it
  empirically (send a locale, compare the returned page titles) and record the result.
  `countryCode` comes from `GET /v1/sessions` and is a separate axis from `locale`.

#### 9.2 Accessibility

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

---

### 10. Observability for a media app

#### 10.1 Structured logging

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

#### 10.2 Network request logging with redaction

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

#### 10.3 Debug bundles

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

#### 10.4 Metrics

Do not export metrics by default. For the headless/daemon mode, an **opt-in** local Prometheus
endpoint (bound to loopback, off unless configured) is defensible: buffer underruns, HTTP error
rates by status, cache hit ratio by tier, token refresh count, current bitrate. It never leaves the
machine, which keeps it consistent with the no-telemetry stance.

---

## Implications for streamboat

### Decide and do now (stack-independent)

1. **Adopt the parser/transport split as a hard architectural rule.** It is the precondition for
   every credential-free test in §2 and the single highest-leverage decision in this document.
2. **Write the CI contract into `CONTRIBUTING.md`**: the default suite passes on a fork PR, with no
   secrets, with the network blocked. Credentialed and networked jobs are separate, guarded, and
   never required for merge.
3. **Stand up the fixture pipeline before the first API call is written**: `tests/fixtures/api/`,
   a `scripts/capture-fixture` that records a response with its request metadata, a
   `scripts/scrub-fixture` that redacts, and a CI check that fails on token-shaped strings.
4. **Vendor `contracts/tidal-api-oas.json` and add the daily drift-diff workflow** (copy
   ref:tidal-sdk-ios/.github/workflows/check-tidalapi-spec.yml). Hand-write JSON Schemas for the
   v1 playback endpoints that the OAS does not cover.
5. **Build the opt-in live canary and the credential-free unauthenticated canary** (§2.5) before
   the client has many endpoints, so the pattern is cheap to extend.
6. **Choose the license split now**: GPL-3.0-only for the apps, LGPL-3.0-or-later or Apache-2.0 for
   `core`. Add SPDX headers from the first file; retrofitting requires chasing contributors.
7. **Implement the secret store as an interface with three backends** (OS keyring, encrypted file,
   plaintext-with-loud-warning) and pick at runtime. Copy sone's AES-256-GCM envelope format
   including the magic-header migration path.
8. **Fix the on-disk layout now**: platform-native config/cache/data/state directories, env-var
   overrides, cache *outside* config, logs in state/Logs. Changing this later means writing
   migration code.
9. **Ship the tiered cache with a hard cap, TTL, SWR and LRU eviction** using sone's numbers as
   defaults, and expose usage + a clear button.
10. **Turn logging on with rotation at 5 MB × 10 files, a pre-start plaintext toggle, and a
    redaction type that makes leaking a token a compile error where the stack allows it.**
11. **Write `docs/legal.md` and the README disclaimer in sone's words** (adapted): independent,
    not affiliated, requires a paid subscription, streaming client only, no offline downloads, no
    circumvention. Keep TIDAL out of the name, icon and app ID.
12. **Decide and publish the project's AI-assistance policy** in CONTRIBUTING, aligned with
    Flathub's disclosure requirement — this is a gating question for Flathub distribution, not a
    style preference.
13. **Ship the four issue templates**, with the playback template copied field-for-field from sone.
14. **Set up Renovate (weekly, grouped) + Dependabot for actions + osv-scanner + codespell +
    workflow-schema validation** on day one; they are near-zero-cost and catch real problems.
15. **Adopt SemVer + Keep a Changelog + Conventional Commits, and enforce the changelog in CI**
    with a four-line version-sync script.
16. **Externalise every user-facing string from the first screen**, and pass the user's real
    locale to TIDAL's `locale` parameter rather than hardcoding `en_US` as every reference does.
17. **Put keyboard navigation, focus visibility and a shortcuts dialog in the definition of done**
    for every screen, with one keyboard test per screen in the default suite.
18. **Build `streamboat debug-bundle` early** — it pays for itself the first time a DAC bug is
    reported.

### Sequence the packaging work

1. GitHub Releases with `checksums.txt` and a tag-triggered build (week 1 of releasing).
2. AUR `-bin` + AUR source package; deb/rpm built in pinned containers, published to Cloudsmith.
3. Flatpak manifest built in CI on every PR from the start; **submit to Flathub only after several
   months of tagged releases and real users**, per the development-history requirement.
4. Snap once the ALSA plug support burden is understood.
5. Windows MSI/NSIS unsigned → winget once signing is sorted.
6. macOS DMG only after the $99 Apple Developer Program is in place; Homebrew cask only after
   notarization and after clearing the 90/90/225 notability bar.

---

## Items that must wait for the stack decision **[STACK]**

| Area | What is blocked |
|---|---|
| Test runner | pytest / vitest / cargo test / go test / XCTest / gtest; the `--live` gating mechanism (pytest option, cargo feature, build tag, CMake `EXCLUDE_FROM_ALL`) |
| HTTP test double | `pytest-httpserver` / `httptest.Server` / `wiremock` / `URLProtocol` / `MockNetworkAccessManager` / `msw` |
| Fuzzing harness | `cargo-fuzz`+libFuzzer / `go test -fuzz` / Atheris / libFuzzer+`-fsanitize=fuzzer` |
| Lint & format | ruff / eslint+prettier / clippy+rustfmt / golangci-lint+gofumpt / SwiftLint+SwiftFormat / clang-format+clang-tidy; and whether the gate is a hook, `pre-commit`, or CI-only |
| Typecheck | mypy / tsc / compiler |
| Keyring binding | `keyring` (Rust) / `keyring` (Python) / `keytar`/`tauri-plugin-stronghold` / `KeychainAccess` / Qt's own; plus which one degrades correctly headless |
| Crypto primitives | AES-256-GCM implementation and a zeroizing buffer type |
| Path resolution | `directories`/`dirs` (Rust) / `platformdirs` (Python) / `env-paths` (JS) / `os.UserConfigDir` (Go) / `QStandardPaths` (Qt) / `GLib.get_user_config_dir` (GTK) |
| Logging library | `flexi_logger`/`tracing` / `logging` + `RotatingFileHandler` / `winston`/`pino` / `log/slog` / `QLoggingCategory`; and the structured-event format |
| i18n mechanism | gettext / Fluent / ICU MessageFormat, plus the extraction command and translation platform (Weblate vs Crowdin vs plain PRs) |
| a11y approach | GTK4/Qt native semantics vs ARIA-in-webview; whether an automated a11y check exists for the toolkit |
| Audio test harness | how to run the pipeline into a null sink in CI; whether golden-PCM comparison is feasible |
| Flatpak dependency generator | `flatpak-cargo-generator.py` / `flatpak-node-generator` / `flatpak-pip-generator` / none |
| Packaging tooling | electron-builder / `cargo tauri build` / meson+flatpak / CPack+NSIS / `go build` + nfpm |
| macOS entitlements | which Hardened Runtime exceptions the chosen media backend requires beyond `allow-jit` |
| Auto-update | Sparkle (macOS) integration; whether the GUI framework has a built-in updater worth using |
| Monorepo tooling | cargo workspace / pnpm workspace / Gradle modules / meson subprojects / Go modules |
| Redaction enforcement | whether the type system can forbid logging a raw token, or it must be a lint |

---

## Open questions

Only the owner can settle these:

1. **AI-assistance posture.** Flathub requires disclosure of AI-generated code, documentation and
   packaging, forbids AI-opened submission PRs and AI-generated commit messages/PR descriptions on
   its side, and reserves the right to reject on the extent of generated material. Is Flathub a
   required distribution channel? If yes, what is the project's disclosure statement, and what
   process keeps commit messages and submission PRs human-authored?
2. **License split.** GPL-3.0-only everywhere, or permissive/LGPL `core` + GPL apps? The second is
   recommended but is effectively irreversible without contributor consent.
3. **Contributor agreement.** DCO sign-off, a CLA, or neither? Without one, relicensing later is
   impossible in practice.
4. **Play reporting to TIDAL.** On by default (so Recently Played works, as sone does) or off by
   default (strictest privacy reading)? Either is defensible; it must be disclosed and switchable.
5. **Update check on by default?** It is a network request to GitHub on launch. Recommend on for
   source/manual installs, off for managed packages.
6. **Embedded client credentials.** Ship an obfuscated default (better first-run UX, extractable,
   arguably higher ToS exposure) or require the user to supply their own (worse UX, cleaner
   posture)? A build flag can support both, but the default binary's behaviour is a decision.
7. **Audio buffer cache on disk.** Any on-disk audio buffering blurs the line the project has drawn
   against downloading. Off by default with a small cap, or not at all?
8. **Apple Developer Program ($99/year) and Windows signing ($120/year Trusted Signing, or
   ~$280–500/year EV).** Who pays, and are macOS/Windows first-class targets from v1 or
   best-effort?
9. **Headless control surface.** MPRIS/D-Bus only (Linux-native, no new attack surface) or an
   HTTP/JSON API (cross-platform, remote control, needs auth and hardening)? This changes the
   security model and the AGPL question.
10. **Release cadence.** Weekly-ish minors like sone, or slower and batched? It affects Flathub
    eligibility (no daily-update software in stable) and the changelog burden.
11. **Translation platform.** Self-hosted Weblate, Crowdin (Strawberry's choice), or plain PRs
    against `po/`?
12. **Minimum supported platform versions.** Oldest glibc/Ubuntu for the deb, oldest macOS SDK,
    oldest Windows. These constrain runner and container choices immediately.

Unverified items that must be checked before anything is published:

- **TIDAL's Terms of Service and Developer Terms could not be read** from this environment
  (`tidal.com` and `developer.tidal.com` are blocked by the egress proxy). A search result
  attributes to Developer Terms 2.0 the claim that the SDK's Player module "constitutes the only
  allowed way for third-party applications to incorporate playback of TIDAL content" — treat that
  as unconfirmed until read directly. The ToS was reported as effective 2026-06-29.
- **The exact `Retry-After` handling and terminal playback sub-status list** cited here come from
  the prior survey plus sone's `rate_gate.rs`; the sub-status numbers (4005, 4010, 4030–4035) were
  not re-verified against sone's source in this pass.
- **The ReplayGain formula** `0.8 * min(10^((rg+4)/20), 1/peak)` comes from the prior survey, not
  from a line read in this pass.
- **Windows Credential Manager blob limit**: `CRED_MAX_CREDENTIAL_BLOB_SIZE` is documented as
  `5*512` = 2560 bytes on Windows 7+, but reports differ on whether `CRED_TYPE_GENERIC` is further
  limited to 512 bytes. Measure with a real token before designing around it.
- **The set of locales TIDAL accepts** for the `locale` parameter is not documented anywhere in the
  reference checkouts; every project hardcodes `en_US`. Determine empirically.
- **Qt 6 module-by-module licensing** (which modules are GPL-2.0-only vs GPL-3.0-only) needs a
  per-module check against https://doc.qt.io/qt-6/licensing.html if Qt is chosen.
- **Flathub's Generative AI policy text quoted here is from the repository HEAD dated 2026-09-07**;
  press coverage in mid-2026 described it as a ban, and the documented policy is a disclosure
  regime. Re-read the live page before submitting.

---

## Sources

### Reference checkouts

- `ref:python-tidal/tests/conftest.py` — live-account-only test fixture; `EnvCredentials` →
  `CachedCredentials` → `KeyringCredentials` chain; `--interactive` pytest option and
  `pytest_collection_modifyitems` skip logic.
- `ref:python-tidal/tox.ini` — `passenv TIDAL_ACCESS_TOKEN TIDAL_REFRESH_TOKEN TIDAL_TOKEN_TYPE
  DBUS_SESSION_BUS_ADDRESS`; py39–py311 envlist; `[testenv:types]` mypy.
- `ref:python-tidal/.github/workflows/lint.yml` — the project's only workflow; lint only, no tests.
- `ref:python-tidal/Makefile`, `ref:python-tidal/pyproject.toml` — isort/black/docformatter/ruff
  toolchain; LGPL-3.0-or-later; dependency set including `mpegdash` and `pyaes`.
- `ref:python-tidal/tidalapi/session.py` — hardcoded `self.locale = "en_US"` with a TODO.
- `ref:mopidy-tidal/DEVELOPMENT.md` — "100% coverage" claim; uv-based workflow; ruff.
- `ref:mopidy-tidal/tests/conftest.py` — `unittest.mock` fakes of `tidalapi` models.
- `ref:mopidy-tidal/tests/test_proxy.py` — `pytest_httpserver` + `trustme` + `pytest-cases`.
- `ref:mopidy-tidal/integration_tests/{conftest.py,util.py,test_login_hack.py}` — pexpect-driven
  end-to-end test of the unauthenticated device-code path via `mpc`.
- `ref:mopidy-tidal/.github/workflows/{test.yml,integration.yml}` — 3.12/3.13/3.14 × mopidy 3.3/3.4
  matrix, `fail-fast: false`, uv cache keyed per leg, codecov, weekly cron.
- `ref:mopidy-tidal/mopidy_tidal/ext.conf` — `playback_cache=false`,
  `playback_cache_max_entries=1024`, `playback_cache_buffer_bytes=16777216`.
- `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/cache.py` — SQLite cache, LRU eviction by
  `last_used`, `manual` pin flag, `schema_version` in a metadata table.
- `ref:sone/src-tauri/src/crypto.rs` — AES-256-GCM envelope, `"SONE"|version|nonce` header, keyring
  (`sone`/`master-key`) then 0600 key file, zeroize, plaintext-passthrough migration.
- `ref:sone/src-tauri/src/cache.rs` — 4-tier TTL/SWR table, 2 GiB cap, 90% evict target, SHA-256
  keys, `CacheResult::{Fresh,Stale,Miss}`, `CacheStats`.
- `ref:sone/src-tauri/src/logging.rs` — flexi_logger 5 MB rotation, 9 kept files, stderr
  duplication, pre-start plaintext `logging.toggle` sidecar and its unit tests.
- `ref:sone/src-tauri/src/rate_gate.rs` — 429 cooldown, 5 s default, 120 s clamp, `fetch_max`
  absolute deadline, HTTP-date `Retry-After` rejected rather than mis-parsed.
- `ref:sone/src-tauri/src/commands/updates.rs` — GitHub-releases update check, semver comparison,
  failures treated as "no update".
- `ref:sone/src-tauri/src/tidal_api.rs` — pure `parse_*` functions with inline `json!` fixtures;
  `("locale", "en_US")` at ~10 call sites; one `<redacted: account endpoint>` log line.
- `ref:sone/src-tauri/src/lib.rs` — config dir resolution, cache under config, encrypted settings,
  migration handling.
- `ref:sone/scripts/gen_embedded.py`, `ref:sone/src-tauri/src/embedded_config.rs` — XOR
  obfuscation of embedded client credentials with the pad stored alongside.
- `ref:sone/build-scripts/build/{deb.sh,Dockerfile.deb,PKGBUILD}` — Ubuntu 22.04 container build,
  BuildKit cache mounts, `dpkg-deb -I` dependency assertion, PKGBUILD that unpacks the deb.
- `ref:sone/build-scripts/publish-cloudsmith.sh` — Cloudsmith repo layout for deb/Fedora/openSUSE.
- `ref:sone/.github/workflows/flathub-update.yml` — tag validation regex, annotated-tag
  dereference, `yq` manifest rewrite, pinned `flatpak-builder-tools` commit
  `ac5a296ac6111aa2319daf532f609a067b88d8a9`, cargo/pnpm source regeneration, PR creation.
- `ref:sone/snap/snapcraft.yaml` — plugs (`home`, `network`, `network-bind`, `network-status`,
  `audio-playback`, `alsa`, `screen-inhibit-control`), dotless MPRIS slot, GStreamer/ALSA layouts.
- `ref:sone/data/io.github.lullabyX.sone.metainfo.xml` — metainfo shape, `<releases>` cadence,
  "streaming client only ... does not support offline downloads".
- `ref:sone/README.md` — no-telemetry statement (line 110), disclaimer (lines 540-544), Cloudsmith
  install instructions.
- `ref:sone/.github/ISSUE_TEMPLATE/playback_issue.md` — the audio bug-report field list and log
  path.
- `ref:sone/vitest.config.ts`, `ref:sone/src/**/*.test.tsx` — jsdom + Testing Library component
  suite.
- `ref:high-tide/build-aux/io.github.nokse22.high-tide.json` — accepted Flathub finish-args for a
  TIDAL client; GNOME runtime 50.
- `ref:high-tide/src/lib/secret_storage.py` — libsecret schema, single JSON blob, keyring unlock
  guarded by `Xdp.Portal.running_under_flatpak()`.
- `ref:high-tide/CONTRIBUTING.md` — licensing statement, gettext workflow and `xgettext` command,
  style rules, threading/error-handling patterns, accessibility requirements.
- `ref:high-tide/.github/workflows/{flatpak.yml,spellcheck.yml}` — flatpak-builder action on
  x86_64 + aarch64; codespell configuration.
- `ref:high-tide/po/` — de, es, fr, it, nl, pl, pt_BR, zh_TW.
- `ref:strawberry/.github/workflows/build.yaml` — 13-job matrix; build-time credential injection
  including `TIDAL_CLIENT_ID` and `-DAPI_CREDENTIALS_ENCRYPTION_KEY="$(openssl rand -hex 32)"`;
  macOS `import-codesign-certs` → `codesign` → `notarytool submit --wait` → `stapler staple`;
  fork/upstream guards on every secret-using step.
- `ref:strawberry/cmake/ApiCredentials.cmake` — `ENC:<iv>:<b64>` credential format, AES-256-CBC via
  the openssl CLI at configure time, `CREATE_SOURCE_API_CREDENTIALS`.
- `ref:strawberry/dist/macos/strawberry.entitlements` — `com.apple.security.cs.allow-jit` for
  GStreamer's orc JIT.
- `ref:strawberry/tests/src/lyricsproviders_live_test.cpp`, `ref:strawberry/tests/CMakeLists.txt` —
  `EXCLUDE_FROM_ALL` live canary tests, 30 s timeout, canary+fallback fixtures.
- `ref:strawberry/tests/src/mock_networkaccessmanager.h` — `ExpectGet()` / `MockNetworkReply::Done()`
  HTTP mock.
- `ref:strawberry/tests/data/audio/` — 12 container/codec audio fixtures.
- `ref:strawberry/CONTRIBUTING.md` — commit-message convention (`Class: message`, no trailing
  period), rebase workflow.
- `ref:strawberry/.github/dependabot.yaml` — daily github-actions updates.
- `ref:tidalt/.githooks/pre-commit` — staged-file gate mirroring CI, non-mutating format check,
  documented `--no-verify`.
- `ref:tidalt/.github/workflows/{ci.yml,lint.yml,release.yml}` — golangci-lint v2.11.3, clang-tidy
  job with `timeout-minutes: 10`, arm64 runner, static-FFmpeg build, docker bake packaging,
  `sha256sum * > checksums.txt` in the release body.
- `ref:tidalt/packaging/build-static-ffmpeg.sh` — FFmpeg 7.1.5 configure flags, no `--enable-gpl`.
- `ref:tidalt/packaging/debian/{control,copyright}`, `ref:tidalt/packaging/fedora/tidalt.spec` —
  Apache-2.0 packaging metadata, audio dependency declarations.
- `ref:tidalt/internal/store/store.go` — keychain-then-age fallback, `~/.local/share/tidalt` 0700,
  `~/.config/tidalt/secrets`.
- `ref:tidalt/internal/tidal/api_test.go` — `httptest.Server` + `roundTripFunc` transport rewrite.
- `ref:tidalt/internal/tidal/quality_test.go` — every quality tier must render a non-empty label.
- `ref:tidalt/internal/player/alsa_fallback_test.go` — format-refusal vs busy-device sentinel,
  memoised `plughw:` fallback, all without hardware.
- `ref:tidal-hifi/CONTRIBUTING.md` — develop-branch policy, one-change PRs, no version bumps in
  PRs, explicit AI usage policy, conventional commits "when possible".
- `ref:tidal-hifi/.github/workflows/release.yml` — multi-OS build, snap publish via
  `canonical/action-publish`, AUR publish via `KSXGitHub/github-actions-deploy-aur@v4.1.3` skipped
  when the key is absent, `softprops/action-gh-release` with `generate_release_notes`.
- `ref:tidal-hifi/build/electron-builder*.yml` — deb/rpm/pacman/AppImage/snap/tar.gz/freebsd, win
  MSI, desktop-entry fields including `X-PulseAudio-Properties: media.role=music` and
  `MimeType: x-scheme-handler/tidal`.
- `ref:tidal-hifi/.github/ISSUE_TEMPLATE/bug_report.yml` — YAML issue form with required
  checkboxes and an installation-method dropdown.
- `ref:tidal-hifi/SECURITY.md` — minimal but present security policy.
- `ref:tidal-sdk-web/.github/workflows/unit-test.yml`, `cypress.yml` — `PLAYER_TEST_USER` /
  `PLAYER_REFRESH_TOKEN` secrets, `dev.tidal.com` → 127.0.0.1 hosts mapping, Cypress binary cache.
- `ref:tidal-sdk-web/.github/workflows/{lint.yml,pull-request.yml,check-changelog-files.yml,
  fossa-scan.yml,release-to-npm.yml}` — lint/typecheck/types-resolution jobs, changelog
  enforcement matrix, FOSSA, OIDC npm publish.
- `ref:tidal-sdk-web/packages/auth/src/auth/auth.test.ts` — `vi.mock` of storage/utils/fetch
  modules, `vi.resetModules()` for a pristine module.
- `ref:tidal-sdk-web/packages/player/src/internal/helpers/manifest-parser.test.ts` — real base64
  DASH/BTS/EMU manifests as inline fixtures, including Widevine/PlayReady `cenc:pssh` and signed
  CDN URLs.
- `ref:tidal-sdk-ios/.github/workflows/check-tidalapi-spec.yml` — daily OAS diff against
  `https://tidal-music.github.io/tidal-api-reference/tidal-api-oas.json` and auto-regeneration PR.
- `ref:tidal-sdk-ios/.github/workflows/changelog-check.yml`,
  `ref:tidal-sdk-ios/.agents/skills/prepare-release/` — CI invoking skill scripts from the skill
  directory; `check-version-sync.sh`; Keep a Changelog + SemVer process.
- `ref:tidal-sdk-ios/.pre-commit-config.yaml` — pinned SwiftFormat/SwiftLint plus GitHub Actions
  workflow-schema validation.
- `ref:tidal-sdk-ios/Tests/PlayerTests/Mocks/JsonEncodedResponseURLProtocol.swift` —
  `succeed/fail/replay/reset` transport stub that records requests.
- `ref:tidal-sdk-ios/Tests/PlayerTests/Resources/AudioFiles/` — `test_5sec.m4a`, `test_1min.m4a`.
- `ref:tidal-sdk-ios/Tests/PlayerTests/Playlog/PlayLogTestsHelper.swift` — 0.5 s position
  tolerance.
- `ref:tidal-sdk-ios/Tests/PlayerTests/Helpers/OptimizedWait.swift` — polling wait with recorded
  failure.
- `ref:tidal-sdk-android/.agents/README.md`, `.agents/checks/` — one-file-per-review-rule agent
  checks with severity calibration, explicitly additive to formatters and linters.
- `ref:tidal-sdk-android/renovate.json` — weekly grouped minor/patch updates.
- `ref:tidal-sdk-android/.fossa.yml` — scanning only shipped runtime classpaths.
- `ref:tidal-cli/src/__tests__/album.test.ts`, `ref:tidal-cli/skills/tidal-cli/SKILL.md` —
  `*Data()` / display-wrapper split; `vi.mock('../auth')`; tests mock the API client so no real
  API calls are made; a repo-shipped skill published as a release artifact.
- `ref:tidal-cli/.github/workflows/release.yml` — `npm publish --access public --provenance`.
- `ref:dotnet-tidal-usdk/LICENSE.md` — MIT plus a non-OSI anti-piracy clause.
- `ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts` — `countryCode`/`deviceType`/`locale`
  query construction.

### Upstream documentation

- https://docs.flathub.org/docs/for-app-authors/requirements — inclusion policy (console software
  rejected, minimal submissions rejected, insufficient development history), app ID rules,
  license and license-file requirements, permissions/portals policy, no network during build,
  build-from-source, stable-releases-only, localisation policy, trademark rules, and the
  Generative AI policy. Read from the source repository `flathub-infra/documentation`
  (`docs/02-for-app-authors/02-requirements.md`, HEAD dated 2026-09-07).
- https://docs.flathub.org/docs/for-app-authors/linter — finish-args rules including
  `finish-args-incorrect-secret-service-talk-name`, `finish-args-own-name-cpt`,
  `finish-args-mpris-flatpak-id-talk-name`, `finish-args-x11-without-ipc`,
  `metainfo-missing-screenshots`, `module-*-build-network-access`.
- https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines — metadata_license /
  project_license requirements.
- https://github.com/flatpak/flatpak-docs/blob/master/docs/sandbox-permissions.rst —
  `--socket=pulseaudio` includes `/dev/snd`; device table (`dri`, `kvm`, `shm`, `input`, `usb`,
  `all`); `--persist=DIR` semantics; default D-Bus policy allowing
  `org.mpris.MediaPlayer2.$FLATPAK_ID`.
- https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Secret.html —
  per-application master secret over a pipe FD, xdg-desktop-portal ≥ 1.5.0.
- https://github.com/Homebrew/brew/blob/main/docs/Package-Acceptance-Policy.md — notability
  thresholds 30/30/75 and 90/90/225, "a code repository less than 30 days old is normally not
  eligible".
- https://github.com/Homebrew/brew/blob/main/docs/Acceptable-Casks.md — Gatekeeper assessment
  requirement; no SIP/Gatekeeper bypass.
- https://github.com/Homebrew/brew/blob/main/docs/Homebrew-Security-and-Supply-Chain.md —
  quarantine application, Developer ID + notarization rationale, deprecation of casks failing
  Gatekeeper.
- https://developer.apple.com/support/developer-id — Developer ID certificate requires Apple
  Developer Program membership; notarization included.
- https://azure.microsoft.com/en-us/products/artifact-signing and
  https://azure.microsoft.com/en-in/pricing/details/trusted-signing/ — $9.99/month Basic,
  $99.99/month Premium, individual eligibility USA/Canada, organisations EU/UK.
- https://learn.microsoft.com/en-us/windows/package-manager/package/repository — winget manifest
  submission, `InstallerSha256` requirement, automated validation, 7-day PR timer.
- https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentiala —
  `CRED_MAX_CREDENTIAL_BLOB_SIZE`; https://github.com/jaraco/keyring/issues/355 — observed
  failures past the limit.
- https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/licensing.html —
  LGPL-2.1 core, GPL plugins in `-ugly`, "with GPL linked plugins GStreamer is for all practical
  reasons under the GPL itself", FFmpeg build-mode caveat for gst-libav.
- https://fedoraproject.org/wiki/Licensing/FDK-AAC — FDK-AAC license is GPL-incompatible.
- https://doc.qt.io/qt-6/licensing.html and https://www.qt.io/faq/qt-open-source-licensing — Qt 6
  mainly LGPLv3 with GPL-only modules; GPL-2.0-only and GPL-3.0-only modules must not be mixed.
- https://semver.org/spec/v2.0.0.html, https://keepachangelog.com/en/1.1.0/,
  https://www.conventionalcommits.org/en/v1.0.0/ — versioning, changelog and commit conventions.
- https://github.com/dirs-dev/directories-rs — the canonical per-OS mapping for config/cache/data/
  state/runtime directories used in §4.1.
- https://tidal-music.github.io/tidal-api-reference/tidal-api-oas.json — the OpenAPI document both
  official SDKs diff daily.
- https://github.com/tauri-apps/tauri/issues/207 (accessibility tracking) and
  https://github.com/tauri-apps/tauri/issues/4315 ("Could not determine the accessibility bus
  address") — the webview-stack accessibility risk noted in §9.2.

### Not reachable from this environment (flagged as unverified)

- `https://tidal.com/terms` and `https://developer.tidal.com/documentation/guidelines-developer-terms-2_0`
  — blocked by the egress proxy. All ToS/Developer-Terms claims in this document are second-hand
  and must be confirmed by the legal research topic.
- `https://docs.flathub.org` and `https://docs.flatpak.org` direct fetches were blocked; both were
  read from their upstream source repositories instead (`flathub-infra/documentation`,
  `flatpak/flatpak-docs`), which is equivalent or newer.
