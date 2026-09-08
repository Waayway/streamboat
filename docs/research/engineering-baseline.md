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
  ref:python-tidal/.github/workflows/lint.yml). Treat upstream behaviour as unverified by CI. **A
  second, cheap data point makes this worse than "no CI test job"**: `ref:python-tidal/tox.ini`
  declares `envlist = py39,py310,py311`, while `lint.yml` — the only workflow that runs at all —
  pins `python-version: 3.13`. The tox matrix covers three Python versions nothing ever runs in CI,
  and the version the project actually lints under is not itself in that matrix. Treat any upstream
  compatibility statement for Python newer than 3.11 as unverified as well.
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
- **Encrypt tokens at rest with an OS-keyring-held key, and keep a file fallback.** Sone
  uses AES-256-GCM with a 32-byte key from the OS keyring (service `sone`, entry `master-key`),
  falling back to `<config>/sone/sone.key` at mode 0600, with a 17-byte header
  `"SONE" + version + 12-byte nonce` and transparent passthrough of legacy plaintext
  (ref:sone/src-tauri/src/crypto.rs:79-148). Precisely: the 0600 file is written **once, at
  first-run key generation**, even when the keyring store also succeeds — not on every launch.
  On a later launch where the keyring read succeeds, sone writes no file. High Tide instead
  stores the token JSON directly in libsecret under schema `io.github.nokse22.high-tide`
  (ref:high-tide/src/lib/secret_storage.py).
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
  TIDAL client therefore does **not** need `--device=all` for exclusive ALSA output — High Tide's
  eight finish-args are `--share=network --share=ipc --socket=fallback-x11 --socket=wayland
  --device=dri --socket=pulseaudio --filesystem=xdg-run/pipewire-0:ro
  --filesystem=xdg-run/discord-ipc-0`
  (ref:high-tide/build-aux/io.github.nokse22.high-tide.json).
- **Flathub will not take a console app**, requires "a meaningful history of development",
  requires a complete English localisation, requires the name and icon not to imply affiliation
  with a vendor (so: not "TIDAL <x>", not TIDAL's logo), and requires the license to be declared
  and match the source (https://docs.flathub.org/docs/for-app-authors/requirements — read from
  the source repo at flathub-infra/documentation, HEAD 2026-09-07). The headless mode ships via
  distro packages, not Flathub. **Two rules the requirements doc states with narrower carve-outs
  than a first read suggests** (both quoted directly, §6.1): localisation exempts inherently
  region-specific apps and allows a "could not find contributors to help" exception for a
  non-English-speaking author; "building from source" allows a case-by-case exception "to
  well-known vendors ... where the necessary tooling to perform an offline source build is not
  available." Neither carve-out changes streamboat's plan (ship English + real translations, build
  from source), but do not overstate either rule as an absolute when describing it to a reviewer.
  **Flathub also has a named "insecure design" rejection criterion that bears directly on two of
  this document's own recommendations**: "Applications that include insecure or harmful design
  choices, such as disabling or bypassing security mechanisms, using insecure cryptographic
  practices, exposing or accessing sensitive information, or shipping overly permissive
  configurations will not be accepted." Concretely: never describe the §3.4 XOR/AES-obfuscated
  embedded client id as "encryption" anywhere a reviewer reads (metainfo, README, manifest
  comments — not just user-facing docs); compile the §3.2/§10.5 `--insecure-token-store` plaintext
  fallback out of, or hard-disable it in, the Flatpak build, so the sandboxed build has no
  plaintext-credential code path at all; and prefer the Secret portal (§3.2) over
  `--talk-name=org.freedesktop.secrets`, since the portal needs no static permission. The same
  document adds a "Trust and history" clause — submission decisions weigh the submitter's prior
  conduct across other apps, not just this one.
- **Flathub now has a Generative AI policy that requires disclosure.** Submitters "must disclose
  any AI-generated code, documentation, packaging, or other material", "AI tools or agents must
  not open or automate Flathub submission pull requests, or generate their commit messages,
  descriptions, review comments, or replies", and undisclosed material "may result in rejection"
  and repeat violations in "a permanent ban" (same doc). This is a hard constraint on how
  streamboat is developed and submitted, and the owner must decide the project's posture on it.
- **Every reference GUI desktop client except tidal-hifi is GPL-3.0** (sone GPL-3.0-only, High Tide
  GPL-3.0, Strawberry GPL-3.0; tidal-hifi is MIT because it is a web wrapper). **Correction**:
  tidalt is a second, non-web-wrapper exception — a native Go TUI/daemon with cgo ALSA output and a
  `.desktop` entry (ref:tidalt/cmd/tidalt/tidalt.desktop), licensed Apache-2.0
  (ref:tidalt/LICENSE). Don't lean on "every native GUI client is GPL-3.0" as a premise; the correct
  framing is "every native GUI client *except* tidal-hifi and tidalt". Libraries are
  permissive/weak-copyleft: python-tidal LGPL-3.0-or-later, mopidy-tidal Apache-2.0, tidalrs MIT,
  libopenTIDAL MIT, tidalt Apache-2.0, all three TIDAL SDKs Apache-2.0.
- **GStreamer pushes you to GPL in practice.** Core and `gst-plugins-base` are LGPL-2.1, and
  patent-encumbered plugins live in `gst-plugins-ugly`, per GStreamer's own licensing FAQ
  (https://raw.githubusercontent.com/GStreamer/gst-docs/master/markdown/frequently-asked-questions/licensing.md).
  The stronger claim — "when using [plugins that link to] GPL libraries, GStreamer is for all
  practical reasons under the GPL itself" — is *not* on that FAQ page (verified by downloading and
  grepping it); it and the FFmpeg build-mode caveat ("if you are distributing an application which
  has a non-GPL compatible license … you have to make sure not to build FFmpeg with GPL code
  enabled") are both from the **same** file instead, `GStreamer/gst-libav`'s `README.md` (not
  `gst-plugins-base`'s `LICENSE_readme`, which does not exist at HEAD in the GStreamer monorepo —
  a second earlier-draft misattribution, now corrected; see §5.3). fdk-aac is GPL-incompatible and
  must not be linked into a GPL binary (Fedora Licensing/FDK-AAC wiki content, corroborated by
  tookmund.com "AAC and Debian").
- **macOS distribution costs $99/year and cannot be skipped** for a usable download: Developer ID
  + notarization are covered by Apple Developer Program membership, Homebrew applies quarantine
  and audits casks against Gatekeeper, and Homebrew's notability floor is **disjunctive, not
  cumulative**: 30 forks *or* 30 watchers *or* 75 stars normally, 90 forks *or* 90 watchers *or*
  225 stars for a self-submission by the repo owner (any one of the three clears the bar), with
  repos under 30 days old normally ineligible
  (https://github.com/Homebrew/brew/blob/main/docs/Package-Acceptance-Policy.md,
  https://github.com/Homebrew/brew/blob/main/docs/Homebrew-Security-and-Supply-Chain.md).
- **Windows signing is cheap if you qualify, but eligibility is narrower for an individual than
  for an organisation — re-verify the figures before relying on them.** Azure Trusted Signing
  (renamed Azure Artifact Signing) is reported at $9.99/month Basic (up to 5,000 signatures, one
  certificate profile) and $99.99/month Premium (up to 100,000 signatures, 10 profiles); an EV
  certificate is roughly $280–500/year instead. **Correction (reverts an earlier draft's inverted
  reading)**: Microsoft's own documented eligibility text is *not* symmetric. Public Trust
  certificates are available to **organisations** in the United States, Canada, the European
  Union, the United Kingdom, Australia, New Zealand, Japan, South Korea, Singapore, Switzerland,
  Norway and Israel; **individual developers must be located in the United States or Canada** —
  i.e. individual eligibility genuinely is geographically narrower than organisation eligibility.
  Two further constraints: Artifact Signing does not support free, trial or sponsored Azure
  subscriptions (a paid subscription is required), and reporting indicates individual-developer
  onboarding has been paused, with new organisation customers required to show at least three
  years of verifiable history. **Unverified**: `azure.microsoft.com` and `learn.microsoft.com` are
  blocked from this environment, so all of the above (pricing, signature caps, and the eligibility
  list) comes from a search index over Microsoft's pricing/FAQ pages, not a direct fetch. Re-read
  the Microsoft page directly before publishing a cost or eligibility figure, but do not repeat the
  "eligibility is not narrower for individuals" framing — that framing is itself wrong and was
  introduced by an earlier draft's misreading of the secondary sources.
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
| mopidy-tidal | Apache-2.0 | yes — unit tests on Python 3.12/3.13/3.14 (`test.yml`); separately, a weekly + push/PR integration matrix of Python 3.12/3.13/3.14 × mopidy 3.3/3.4 (`integration.yml`) | `unittest.mock` over `tidalapi`; `pytest-httpserver` + `trustme` for the caching proxy; `pexpect`-driven real-Mopidy integration tests | no | ruff check + ruff format | PyPI |
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
- **Capture mechanism**: build the recorder as a decorator on the same transport seam §2.3
  recommends for stubbing (tidalt's `roundTripFunc`, tidal-sdk-ios's
  `JsonEncodedResponseURLProtocol`), not a separate tool. A `STREAMBOAT_RECORD_FIXTURES=<dir>` mode
  wraps the real transport and writes the scrubbed request+response to `<dir>` as each call
  completes — scrub in the write path so an unredacted body never touches disk, even
  transiently. Do **not** use mitmproxy or a browser HAR export for this: both capture TLS traffic
  outside the scrubber and leave an unredacted file on disk that must be cleaned up after the fact,
  which is exactly the failure mode this pipeline exists to prevent.
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
- **Gap — fixture provenance (real captures vs. hand-written synthetic JSON) is a legal/ToS
  decision as much as a redaction problem, and the two reference approaches actually split rather
  than agreeing.** This section's default reads as "capture real responses, then scrub them", but
  committed fixture bodies also carry TIDAL's own editorial copy, catalogue metadata and cover-art
  URLs — not only tokens — and `docs/legal.md` (§5.4/§5.5) needs to say whether redistributing that
  in a public repository fits the project's declared posture. The precedent splits: sone's ~51
  parser tests in `tidal_api.rs` (cited above) use hand-written `serde_json::json!` literals shaped
  like real responses, not captures, while tidal-sdk-web's manifest-parser fixtures are real
  captured base64 manifests — but that is TIDAL's own repository committing TIDAL's own payloads,
  not a third-party client committing them. Recommended default for streamboat: minimal synthetic
  fixtures (structure-preserving, content-invented) for catalogue/editorial payloads, with real
  captures reserved for cases where the exact bytes are the thing under test — manifests, error
  envelopes, unusual encodings — trimmed to only the fields the parser reads. Record the decision in
  `docs/legal.md` alongside the ToS posture, and encode the rule in `scripts/scrub-fixture` (strip
  long free-text description/bio fields, not just token-shaped strings).
- Record the capture date and the endpoint+params at the top of each fixture (a sibling
  `.meta.json`), so a future failure can be attributed to drift rather than to a bug. Give the
  sidecar a fixed shape: capture date (UTC), HTTP method, path template and query params (values
  redacted), the streamboat version and client-id variant used to capture it, the response status,
  and the TIDAL response headers that matter for drift (`x-tidal-*`, `content-type`, any
  `Retry-After`) — the last of these is what lets a later failure be attributed to server drift
  rather than a streamboat bug.

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
3. Terminal playback sub-statuses are not retried. Verified against source: sone's
   `TERMINAL_SUB_STATUSES` is `&[4005, 4010, 4030, 4031, 4032, 4034, 4035]` — **not** the
   contiguous range `4030–4035`. It deliberately excludes `4006` ("streaming privileges lost —
   recovers") and `4033` ("subscription up-sell") as recoverable
   (ref:sone/src-tauri/src/tidal_api.rs:16-18, tests at 6746, 6755-6756).
4. The quality fallback cascade stops at the first success and does not cascade past a rate-limit
   or terminal error.
5. Token refresh persists the new tokens to storage exactly once and does not lose the refresh
   token when the response omits it.

#### 2.4 Layer 3 — contract tests against the published spec

TIDAL publishes an OpenAPI document at
`https://tidal-music.github.io/tidal-api-reference/tidal-api-oas.json`, and **all three official
SDKs (web, iOS, Android)** run a **daily** cron that `cmp`s the vendored copy against the freshly
downloaded one, prints a unified diff into the job log, and triggers a client-regeneration
workflow that opens or updates a PR (ref:tidal-sdk-ios/.github/workflows/check-tidalapi-spec.yml,
ref:tidal-sdk-android/.github/workflows/check-tidalapi-spec.yml, and
ref:tidal-sdk-web/.github/workflows/check-tidalapi-spec.yml — all three, not two, run the
identical mechanism).

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
5. Runs from a maintainer's machine on a schedule, never from a fork PR. **Do not use a
   GitHub-hosted "self-hosted runner" label on the public repo for this** — on a public repository
   a self-hosted runner is a known code-execution risk: a fork PR that reaches any workflow using
   that runner label executes attacker code on the machine holding the runner token, and here that
   machine also holds a live TIDAL refresh token for a real paying account. No reference project
   uses a self-hosted runner or `pull_request_target` (grep across all 21 checkouts: zero hits for
   either), so there is no precedent to copy and no reason to invent the risk. Prefer a maintainer-
   local cron that runs the canary and pushes only `live-report.json` to a private repo or gist — no
   inbound trust at all. If a self-hosted runner is used anyway, it must be ephemeral (one job per
   VM, destroyed after), registered to a *separate private* repository the public repo cannot
   trigger, and never carry a label any public workflow references. Also: never use
   `pull_request_target` in this repo — it runs workflow code from the base branch with secrets
   available while checking out fork code, which is the same class of risk from a different angle.

Add a second, credential-free canary that hits only the unauthenticated surfaces
(`/v1/oauth2/device_authorization` returns a device code without an account). mopidy-tidal proves
the value of testing the unauthenticated path end to end: it spawns real Mopidy with `pexpect`
against a config with no tokens and asserts that `mpc list artist` surfaces
`.* visit https://link.tidal.com/.*` (ref:mopidy-tidal/integration_tests/test_login_hack.py). That
single test would catch a device-flow endpoint change with no subscription at all.

**This is the strongest working precedent for the credential-free canary in the whole reference
set, and it already runs on a schedule.** `ref:mopidy-tidal/.github/workflows/integration.yml`
runs this exact pexpect suite on a 6-leg Python × mopidy matrix, `fail-fast: false`, on
`cron: "0 0 * * 0"` (weekly) **plus** on every push and PR — with no secrets, no live account, and
no coverage upload. Copy the shape directly: a scheduled, fork-safe, unauthenticated workflow that
asserts the device-flow surface still looks like a device-flow surface.

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
5. **The `streamboat://` (registered by default) and `tidal://` (parsed, opt-in-claim only —
   see `tidal-api.md` §13/§14) URI handlers** — this is the one input that arrives
   unsolicited from the open internet: registering a scheme means any web page the user visits can
   invoke `streamboat play <arbitrary string>` as a process argument, pre-authentication. **Correction:
   an earlier draft of this item told streamboat to register `tidal://` itself — do not, it is
   already claimed by the official TIDAL desktop app plus Strawberry/Sone/High Tide, and OS handler
   registration is last-writer-wins.** Fuzz the parser (it must handle both schemes) against an
   allowlist of shapes (`{scheme}://track/<digits>`, `album`, `playlist/<uuid>`, `artist`,
   `mix`) with hard rejection of everything else — path traversal, absurd lengths, embedded
   newlines/NULs, non-ASCII homoglyphs, anything that would be interpolated into a shell, a
   filesystem path, or an outbound URL. Never pass the raw argument to a subprocess or to the API
   host without re-composing the request from the parsed id. tidalt's
   `Exec=tidalt play %u` (ref:tidalt/cmd/tidalt/tidalt.desktop, using `tidal://` since tidalt does
   not have streamboat's scheme-collision problem) plus its D-Bus-forwarding
   `play.go` is the exact code shape to threat-model on Linux, with the scheme swapped to
   `streamboat`. **This is not a Linux-only surface —
   design and threat-model it for all three desktop OSes from the start.** sone's mechanism (adapt
   with `streamboat` as the scheme) is `plugins.deep-link.desktop.schemes: [...]` in `tauri.conf.json`
   and `app.deep_link().register_all()` at startup (ref:sone/src-tauri/tauri.conf.json,
   ref:sone/src-tauri/src/lib.rs) — on Windows this writes `HKCU\Software\Classes` registry keys at
   runtime, which are exactly as reachable from an untrusted web page as the Linux `.desktop`
   handler. tidal-hifi declares one cross-platform block,
   `protocols: {name: "...", role: "Viewer", schemes: [...]}`
   (ref:tidal-hifi/build/electron-builder.base.yml:60-63), which electron-builder expands into both
   the macOS `CFBundleURLTypes` Info.plist entry and the Windows registry entries. Whatever the
   stack, the deliverable is three registrations for `streamboat://` (Linux `.desktop`, Windows registry,
   macOS `CFBundleURLTypes`) plus second-instance argument forwarding (sone pairs its deep-link
   handler with `tauri_plugin_single_instance` for exactly this), and the allowlist validation
   above must run on the forwarded argument in every one of those entry paths, not only the Linux
   one, and for both the registered `streamboat://` scheme and any accepted-but-unregistered
   `tidal://` link reaching the app via forwarding.

Corpus: seed from the committed fixtures (the tidal-sdk-web test file alone gives DASH-FLAC,
DASH-AAC, BTS-MP3 and EMU-HLS manifests to seed with). Run the fuzzer in CI for a bounded time
(60 s per target on PRs, 15 min nightly), commit crashers as regression fixtures.

Independent of the fuzzer: **set hard limits** — reject a manifest over N bytes (start at 1 MiB),
disable XML external entities and DTD processing outright, cap segment count, and cap total
decoded size.

**Run the fuzz targets under a sanitizer, not just a plain build.** A fuzzer without a memory
sanitizer is only as good as its oracle: a heap overflow or use-after-free in a demuxer/decoder
path shows up as a pass, not a crash, unless ASan/UBSan are compiled in. No reference project runs
any sanitizer, Valgrind, Miri or CodeQL. **Correction**: grepping all 21 checkouts for
`fsanitize|ASAN|UBSAN|valgrind|miri` (excluding vendor) actually returns at least 10 files with
matches — `strawberry/src/core/mainwindow.cpp`, `strawberry/src/covermanager/coverproviders.{h,cpp}`,
`strawberry/src/dialogs/edittagdialog.cpp`, `strawberry/src/scrobbler/audioscrobbler.h`, several
`strawberry/src/translations/*.ts` files, and a tidal-sdk-android surface-holder test — every one of
them a case-insensitive substring false positive (`HasAnyProviders`, `unlockCanvasAndPost` match
`asan`), not one hit in one file as an earlier draft claimed. The **conclusion** is unaffected and
independently verified: zero real sanitizer/Valgrind/Miri usage, and a separate grep for
`codeql|scorecard` across all workflow YAML also returns zero files, so no reference project runs
CodeQL or OpenSSF Scorecard either. Treat sanitizer/fuzzer coverage as a no-precedent addition, not
a copyable pattern. Run the §2.6 fuzz targets under ASan+UBSan (add TSan for the gapless/queue
scheduling code, which is inherently multi-threaded) on the nightly job given the runtime cost. If
the stack is Rust, also run the pure-parser test suite under Miri and require review on every
`unsafe` block; make the committed fuzz-crasher regression corpus run under the sanitizer build
too, not only the plain one.

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
   — canonical formula is TIDAL's own, from the Android SDK
   (ref:tidal-sdk-android/player/playback-engine/src/main/kotlin/com/tidal/sdk/player/playbackengine/volume/LoudnessNormalizer.kt):
   `gain = min(10^((replay_gain + pre_amp) / 20), 1 / peak)`, `pre_amp = 4.0` dB, no extra
   attenuation factor. **Correction: sone's shipped formula is not this** —
   sone multiplies by an *additional* `0.8` headroom factor on top,
   `gain = 0.8 * min(10^((replay_gain + 4) / 20), 1 / peak)`
   (ref:sone/src-tauri/src/commands/playback.rs:9-20; `peak` defaulted to `1.0` when absent or
   non-positive, `gain = 1.0` when `replay_gain` is `None`) — a Sone-specific ~1.9 dB extra
   attenuation, not TIDAL's formula. Write the unit test against TIDAL's formula (no `0.8`) unless
   the owner deliberately wants Sone's quieter-than-TIDAL output. Also test the quality-ladder
   mapping, the ALSA/WASAPI fallback state machine, queue/gapless scheduling decisions. These run
   everywhere, including CI, and should be the majority of audio tests.
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
  dedicated `vitest.config.ts` separate from the app's Vite config
  ("Separate from vite.config.ts so the multi-input Tauri build config stays untouched"),
  covering virtualization, pagination, keyboard shortcuts and settings tabs
  (ref:sone/vitest.config.ts, ref:sone/src/components/*.test.tsx). Precisely: 46 test files total,
  of which only ~19 are component tests (16 in `src/components`, 3 in `src/components/settings`);
  the remaining ~27 cover hooks (9), lib helpers (14), atoms, contexts and utils — don't read this
  as "~40 components tested", the component coverage is roughly half that.
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

**The default test job must pass on a fork PR with no secrets, with the network disabled.**
An environment-level block (a proxy env var pointing at a dead port) is not by itself a reliable
mechanism: many HTTP clients ignore unset/dead proxy vars, and it does nothing on the Windows and
macOS legs of the §7.1 matrix. Enforce it in the code instead: the offline test build should
construct the client with a transport that panics/fails on any real connection attempt — the same
seam already recommended for stubbed transports in §2.3 (tidalt's `roundTripFunc`,
tidal-sdk-ios's `JsonEncodedResponseURLProtocol`), so an accidental network call is a deterministic
test failure on every OS. Layer an environment-level block on top only on Linux, where `unshare -n`
(or a network-less container) actually isolates the process, as a second net. Everything
credentialed goes in a separate workflow gated on
`github.event.pull_request.head.repo.full_name == github.repository`, the pattern Strawberry uses
for its signing/notarizing steps (ref:strawberry/.github/workflows/build.yaml:1178, 1253).

#### 2.9a Test determinism: clock, timezone and locale

§2.9 makes "no secrets, no network" the non-negotiable rule; three more sources of non-determinism
are specific to this app and untested by every reference project (grepping all 21 checkouts'
workflow YAML for `TZ:|LC_ALL|LANG:` returns nothing — an unclaimed, cheap win):

- **Wall clock.** Token expiry, cache TTL/SWR staleness (§4.2), and the `Fresh`/`Stale`/`Miss`
  tri-state all read the clock. Inject it as an interface everywhere expiry/TTL logic reads it, so
  §2.3's tests 1, 2 and 5 and §4.2's tri-state tests can *drive* the clock deterministically rather
  than merely avoiding it (the alternative — pinning a token's expiry an hour into the future so
  refresh never fires, which is what tidalt's test harness does,
  ref:tidalt/internal/tidal/api_test.go:19-48 — only avoids the problem for tests that don't care
  about refresh).
- **Timezone.** Release dates, "recently added", and any relative-time display are timezone-
  sensitive. Set `TZ=UTC` in the CI test environment, **and** add at least one matrix leg with a
  non-UTC, non-English locale (e.g. `TZ=Pacific/Chatham LC_ALL=de_DE.UTF-8`) so timezone- and
  locale-dependent formatting is exercised rather than accidentally passing because every runner
  happens to be UTC/en-US.
- **Random nonces.** Seed any RNG the tests exercise (AEAD nonces in §3.3, retry-backoff jitter)
  from an injectable source, so an encrypted-file round-trip test is byte-reproducible.

#### 2.10 Coverage strategy

No reference project names a coverage target except mopidy-tidal, which states plainly: "Mopidy-Tidal
has a test suite which currently has 100% coverage. Ideally contributions would come with tests to
keep this coverage up" (ref:mopidy-tidal/DEVELOPMENT.md), and its CI uploads to Codecov on the
Python 3.13 leg of `test.yml`. For streamboat: measure coverage on every PR, publish the number, and
gate on "no decrease" for the `core` parsing/auth crate only, where the parser/transport split of
§2.1 makes 90%+ trivially reachable — explicitly exclude UI and platform-audio code from the gate so
the number cannot be gamed by untestable surfaces.

#### 2.11 Fixture and golden-file size policy

Store real API captures under `tests/fixtures/api/`, golden audio under `tests/fixtures/audio/`,
and committed fuzz crashers as regression fixtures — but state a repo-size policy before the first
one lands, because it grows without bound otherwise: a byte cap per fixture (audio fixtures <200 KB
per §2.7; extend the cap to JSON captures, gzip large ones), generate audio goldens from a
tone/sweep at test time where determinism allows and commit only the SHA-256 rather than the PCM,
keep fuzz crashers minimised before committing, and decide explicitly for or against Git LFS now
because switching later rewrites history. Only one of the 21 checkouts uses LFS — tidalswift's
`.gitattributes` routes `*.jpg|*.jpeg|*.png|*.gif|*.heic|*.pdf|*.zip|*.tar|*.gz` through
`filter=lfs diff=lfs merge=lfs -text`, and only for README screenshots, not test data
(ref:tidalswift/.gitattributes, ref:tidalswift/README.assets/). Strawberry's 12-format audio corpus
(ref:strawberry/tests/data/audio/) stays in plain git with no `.gitattributes` at all. Plain git for
small test fixtures is still the right working default; the corrected precedent is "one of 21 uses
LFS, and only for marketing screenshots" rather than "nobody uses it".

**Commit a `.gitattributes` at repo creation regardless of the LFS decision** — line-ending
normalisation is the trap, not LFS. §2.7 recommends golden PCM/FLAC/AAC fixtures compared by
SHA-256 and §7.1 puts unit tests on `windows-latest`; without a `.gitattributes`, git's `autocrlf`
behaviour on a Windows checkout can rewrite any file it heuristically treats as text, and a JSON
fixture or golden file the parser tests byte-compare will then differ only on that leg — a failure
that looks like a mysterious platform bug. tidalswift's file is the only precedent in the set
(`.gitattributes` above, plus `Frameworks/* linguist-vendored`). Commit at minimum: `* text=auto
eol=lf`; `*.sh text eol=lf`; `*.bat text eol=crlf`; `tests/fixtures/** binary`; `*.flac`, `*.m4a`,
`*.pcm`, `*.wav binary`; and mark generated packaging files `linguist-generated` (`*.wxs`, `*.nsi`
once §6.4 introduces them). Pair it with the `.editorconfig` §8.4 already recommends.

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
  key" when no keyring exists. **The portal secret is not, by itself, a usable AES key — the spec
  says so explicitly**: "While the master secret can be used for encrypting any confidential data
  in the sandbox, the format is opaque to the application. In particular, the length of the secret
  might not be sufficient for the use with certain encryption algorithm. In that case, the
  application is supposed to expand it using a KDF algorithm."
  (https://raw.githubusercontent.com/flatpak/xdg-desktop-portal/main/data/org.freedesktop.portal.Secret.xml,
  read directly). Feed it through HKDF-SHA256 with a fixed `info` string before using it as the
  AES-256-GCM key, exactly as you would any other KDF-expanded key — do not assume it is already
  32 bytes of good entropy the way sone's keyring-held key is. `RetrieveSecret` also takes an
  opaque `token` option returned from the previous call; persist it and pass it back on the next
  call rather than re-deriving from scratch each launch.
- **Windows — Credential Manager.** `CRED_MAX_CREDENTIAL_BLOB_SIZE` is `5*512` = 2560 bytes on
  Windows 7 and later, per Microsoft's own docs source repo: "The size, in bytes, of the
  CredentialBlob member. This member cannot be larger than CRED_MAX_CREDENTIAL_BLOB_SIZE (5*512)
  bytes."
  (https://raw.githubusercontent.com/MicrosoftDocs/sdk-api/docs/sdk-api-src/content/wincred/ns-wincred-credentiala.md,
  read directly since `learn.microsoft.com` itself is blocked from this environment — this repo is
  the upstream source for that blocked page, and is a primary source, not a secondary one; an
  earlier draft of this document mis-cited a mingw-w64 header as the verification source instead,
  see the toolchain trap below). Keyring wrappers have historically produced obscure failures past
  that limit (https://github.com/jaraco/keyring/issues/355); whether `CRED_TYPE_GENERIC` is
  *further* limited to 512 bytes remains disputed and unverified — measure it empirically.
  **A toolchain trap makes "measure it empirically" load-bearing, not optional**: mingw-w64's own
  header disagrees with Microsoft's. `mingw-w64-headers/include/wincred.h` at mingw-w64 `master`
  defines `#define CRED_MAX_CREDENTIAL_BLOB_SIZE 512` — a bare 512, with **no** `WINVER` guard
  (https://raw.githubusercontent.com/mingw-w64/mingw-w64/master/mingw-w64-headers/include/wincred.h,
  line 114, read directly and confirmed against the surrounding lines). Code that validates against
  the compile-time constant gets a 5x smaller limit under a MinGW-targeted build than under MSVC,
  for the identical Windows version, with no compiler warning — a real portability trap for any
  build that can target Windows via MinGW. Do not trust the compile-time constant on any toolchain;
  measure the real runtime limit instead, and re-check it on both MinGW and MSVC builds if both are
  shipped.
  **Sidestep the whole question**: make the keyring hold a fixed-size *key*, not the token payload
  — exactly what sone already does on Linux (`keyring::Entry::new("sone", "master-key")` holds 32
  bytes; the tokens live in the AES-256-GCM file, ref:sone/src-tauri/src/crypto.rs). A 32-byte key
  can never approach 2560 bytes — or even 512 — on any OS or toolchain, which makes one design work
  uniformly on Windows/macOS/Linux regardless of compiler and removes the need to measure token
  sizes at all. Where no credential store is usable at all on Windows, DPAPI
  (`CryptProtectData`/`CryptUnprotectData` with `CRYPTPROTECT_UI_FORBIDDEN`) wraps the key file with
  the user's login credentials and has no size limit — name it alongside the age-encrypted fallback
  cited from tidalt below as the idiomatic Windows equivalent.
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
- Key resolution: OS keyring first (`keyring::Entry::new("sone", "master-key")`, `get_secret()`) —
  and if that succeeds, `load_or_generate_key()` returns immediately with **no file write on that
  path**. Only if the keyring read fails does it fall back to `<config>/sone.key` (exactly 32
  bytes, `0o600`); only if *that* is also absent does it generate a fresh key from `OsRng`, and
  **only in that key-generation branch** does it write the 0600 file backup — with the comment
  "Always write file backup — keyring may be unreachable on next launch (e.g. AppImage with
  different D-Bus session)". So the file backup happens once, at first-run key generation, not on
  every launch and not on a launch where the keyring already has the key
  (ref:sone/src-tauri/src/crypto.rs, `load_or_generate_key`).
- The in-memory key buffer is zeroized after the cipher is constructed.

For streamboat, change two things: bump the version byte on any format change and refuse to
downgrade; and make the "seed a key-file backup at first-run key generation" behaviour a
documented, user-visible setting, because it means the encryption is only as strong as the file
permissions on that path.

**Write it atomically, or a crash mid-write turns a bad magic-header check into a silent
re-login.** sone's own `decrypt()` treats a missing/bad magic header as "plaintext, pass through
unchanged" (above) — which is the right migration behaviour for an *intentional* legacy file, but
means a file *torn* by a crash or `SIGKILL` mid-write is silently misread as plaintext or fails
opaquely, either way logging the user out with no diagnostic. sone itself only writes one class of
file atomically — the *theme* file, not the settings/token file: `write_theme_file` documents
"Validate and atomically write the theme file (`tmp-<pid>` → fsync → rename), mode `0644`. So a
crash can never leave a torn file" (ref:sone/src-tauri/src/theme_config.rs:142-175), while the
settings/token path is a plain `fs::write(&self.settings_path, encrypted)` with no temp file at all
(ref:sone/src-tauri/src/lib.rs:474-477 and its migration paths). sone's own scrobble and play-report
queues do use the safer pattern (`self.path.with_extension("bin.tmp")`,
ref:sone/src-tauri/src/scrobble/queue.rs:69, ref:sone/src-tauri/src/tidal_report/queue.rs:56).
**streamboat should copy the theme-file pattern for every persisted secret/settings/state file, not
the settings-file pattern**: create a temp file in the same directory → write → `fsync(file)` →
rename → `fsync(dir)`; mode `0600` for anything containing a secret, `0644` otherwise. Once the
format has shipped a v1, make a failed magic-header check a hard, typed error rather than silent
plaintext passthrough, and add a test that truncates the encrypted file at every byte offset and
asserts a clean typed error — never a silent re-login with no explanation.

#### 3.4 Client ID / client secret handling

State of the art in the references, all of which is obfuscation and none of which is security:

- **python-tidal** base64-decodes its client credentials at `tidalapi/session.py` lines ~155-162
  (per the survey) — reversible by anyone.
- **sone** XOR-encodes each value with a random pad and stores *the pad in the same file*
  (`STREAM_SALT_A` next to `CODEC_HINT_A`), regenerated by `scripts/gen_embedded.py`, with the
  variables deliberately misnamed ("stream salt", "codec hint") to avoid grepability
  (ref:sone/scripts/gen_embedded.py, ref:sone/src-tauri/src/embedded_config.rs). It ships four
  pairs (A/B = device-code id/secret, C/D = PKCE id/secret) and a `has_stream_keys()` predicate
  that treats a `PLACEHOLDER`-prefixed value as absent. **Gap in the generator, worth knowing before
  copying the pattern**: both `ref:sone/scripts/gen_embedded.py` and `ref:sone/scripts/gen_credentials.py`
  emit only the A/B pair (`STREAM_SALT_A`/`CODEC_HINT_A`/`STREAM_SALT_B`/`CODEC_HINT_B` plus
  `stream_key_a()`/`stream_key_b()`/`has_stream_keys()`); the shipped
  `ref:sone/src-tauri/src/embedded_config.rs` additionally defines the C/D (PKCE) constants,
  `stream_key_c()`/`stream_key_d()` and `has_pkce_keys()`, none of which any committed script
  generates. Regenerating credentials with the shipped scripts would silently drop the PKCE half.
  Design streamboat's own generator to emit every credential slot the binary actually consumes, and
  add a CI check that the generator's output set matches the constants the code reads.
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

#### 3.6 Testing the secret-store backends in CI

§2.9 requires the default suite to pass on a fork PR, but a headless GitHub runner has no Secret
Service, no unlocked Keychain and no interactive Credential Manager session. Make the store an
interface and unit-test the encrypted-file and plaintext backends everywhere (no OS dependency).
For the keyring backend on Linux CI, run the job under
`dbus-run-session -- gnome-keyring-daemon --unlock` (feeding a dummy password on stdin) and skip
(not fail) the test when `DBUS_SESSION_BUS_ADDRESS` is unset — python-tidal already treats that
variable as the switch, passing it through in `tox.ini` precisely so its `KeyringCredentials` path
can run (ref:python-tidal/tox.ini). On macOS runners, create and unlock a keychain explicitly
(`security create-keychain` / `unlock-keychain`), the same step Strawberry's signing job performs
(ref:strawberry/.github/workflows/build.yaml). On Windows, the Credential Manager API works
headlessly for the current user, so that backend can be tested directly — and it is the right
place to empirically measure real TIDAL token sizes against the 2560-byte blob limit above.

#### 3.7 Multi-process token and device ownership

The owner has committed to shipping desktop **and** headless now (see Project context). Both will
read the same token store, and both may want the same DAC. TIDAL rotates refresh tokens, so two
processes refreshing concurrently will invalidate each other and silently log the user out — a bug
that only appears after both binaries ship, and it constrains the storage and process model from
the first commit. §2.3 test 1 handles in-process refresh collapsing; it does not handle the
cross-process case.

tidalt solves both problems with a single-server model: the first process claims the D-Bus name
`org.mpris.MediaPlayer2.tidalt`; if the name is taken the process becomes a client and forwards
commands over D-Bus, and only the server process ever opens the ALSA `hw:` device — its own docs
state "ALSA `hw:` devices cannot be shared between processes. If two programs both try to open
`hw:1,0` the second one fails" (ref:tidalt/docs/client-server.md). `tidalt daemon` is the same
engine with no TUI (ref:tidalt/cmd/tidalt/daemon.go:44). sone takes the desktop-only version of
this with `tauri_plugin_single_instance` to focus the existing window instead of opening a second
one (ref:sone/src-tauri/src/lib.rs:546-548).

**Decide now**: either the daemon is always the token owner (with the GUI as a thin client), or
the store must be lock-protected for multi-writer access (an advisory file lock around
read-refresh-write, with a re-read-after-lock so a losing process adopts the winner's new token).
Add a test for this to §2.3's list.

**Extend the same decision to the disk cache and any local database — not just tokens.** §4.2's
disk cache keeps a `total_disk_usage` counter and does LRU eviction against it; §4.2a's settings
file and any future local library/queue database are the same shape of shared, mutable, on-disk
state. Two processes each doing LRU eviction against the same cache directory with no
coordination will double-evict, race on writing the same `.meta` sidecar, and disagree about
usage. sone's cache is single-process by construction: `total_disk_usage` lives in memory with no
file locking at all, which is safe only because `tauri_plugin_single_instance` guarantees a single
process (ref:sone/src-tauri/src/cache.rs:188-189,608-645; ref:sone/src-tauri/src/lib.rs:546-548).
mopidy-tidal's audio cache instead gets multi-process safety close to free by using SQLite with LRU
eviction by `last_used` (ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/cache.py:337-352) — SQLite's
own locking/WAL mode does the coordination. Resolve this in the same decision as the token owner:
either (a) the daemon owns tokens, cache and any local database, and the GUI is a thin client over
it (tidalt's model, ref:tidalt/docs/client-server.md), or (b) every shared on-disk store is made
multi-writer-safe explicitly — SQLite with WAL for any index/database, an advisory lock around
cache eviction, and atomic rename for every write (§3.3's atomic-write pattern applies here too).
Also decide account-switch behaviour now: namespace the cache by user id, or purge it on logout —
otherwise account A's cached library can be served to account B on the same machine (see §3.8).

#### 3.8 Logout, account switching, and complete data deletion

The rest of §3 covers storing secrets in detail and says nothing about removing them. For a project
positioned on "no telemetry, sends nothing to its developers" (§4.4) and whose cache holds a
subscriber's library and listening data, "log out" and "delete everything about me" are correctness
and privacy features, not administrative afterthoughts — and getting the ordering wrong produces
concrete bugs: account A's cached playlists served to account B, or the track playing at the moment
of logout getting scrobbled to TIDAL after the user has already signed out.

sone's `logout` command gives a copyable ordering, with its own comments explaining *why* each step
is where it is (ref:sone/src-tauri/src/commands/auth.rs:413-462):

1. Disconnect scrobbling/play-reporting **first**, explicitly "before stopping playback so the
   interrupted track is not scrobbled".
2. Stop playback and tear down the pipeline; clear MPRIS and Discord now-playing state.
3. Disconnect Discord RPC.
4. Shut down the local control server (§10.5) if one is running.
5. Release the idle inhibitor.
6. Clear in-memory tokens and reset the resolved `country_code`, while **deliberately preserving**
   the user's own client-id/secret (§3.4) for the next login.
7. Null out `auth_tokens`, `last_track_id` and scrobble credentials in settings and re-save (or
   delete the settings file outright if it fails to load).
8. Clear the entire disk cache (§4.2).

What sone does **not** do, and streamboat should decide explicitly rather than by omission: it
never deletes the OS-keyring entry or the `0600` key file, so the AES master key outlives the
logged-out session. High Tide's equivalent is one call,
`Secret.password_clear_sync(self.schema, {}, None)`
(ref:high-tide/src/lib/secret_storage.py:85-94, ref:high-tide/src/window.py:267-277).

For streamboat: implement `logout` in the sone ordering above, and separately ship a
`streamboat purge` / "Delete all local data" action that removes config, cache, state/logs, the
keyring entry, and the key file — a strictly stronger operation than logout, for the user who wants
no trace left. Document what a distro package uninstall does and does not remove (it removes the
binary, never the user's config/cache/data directories on any packaging format in this survey).

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
`last_used` (ref:mopidy-tidal/mopidy_tidal/ext.conf,
ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/cache.py:283,337-352). **Correction — the schema's
`manual` column is not a working pin, do not copy it as one**: `evict()` is
`DELETE FROM head WHERE id NOT IN (SELECT id FROM head WHERE not manual ORDER BY last_used DESC, id
DESC LIMIT ?)` — the retained ("keep") set is explicitly `WHERE not manual`, so a row with
`manual = true` is *excluded* from the keep-set and gets deleted first, the opposite of pinning.
Moreover `manual` is never set to `TRUE` anywhere in `mopidy_tidal/` — it only appears as the
`CREATE TABLE` default `FALSE` (cache.py:279) and in that `WHERE` clause; no code path flips it.
Copy the SQLite-with-WAL LRU shape (it is sound and gives multi-process safety for free via
SQLite's own locking, see §3.7) but design streamboat's own pin flag as
`WHERE manual OR id IN (<keep-set>)` if a pin feature is wanted — the reference implementation's
column is vestigial, not a pattern to reproduce.

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
  than trying to migrate. **Cheaper primary mechanism, used by sone**: version the whole cache
  directory (`cache_dir/v{CURRENT_SCHEMA_VERSION}/`, currently `v5`) and on startup delete any
  sibling `v*` directory that is not current — an O(1) migration that reclaims all disk from old
  formats in one step (ref:sone/src-tauri/src/cache.rs:190,200-215,267-271). Keep the per-entry
  `schema_version` field too, as a belt-and-braces guard, but make the directory scheme primary.
- Never cache stream manifests across restarts — they expire and a stale manifest is a confusing
  failure. The ~1-hour figure comes from source, not documentation: tidal-sdk-web's own client-side
  constant is `const MANIFEST_EXPIRATION_MS = 3600000; // 1 hour`
  (ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts:79), used to
  stamp an `expires` timestamp and trigger "a complete reload since manifest has expired"
  (ref:tidal-sdk-web/packages/player/nativePlayer.ts:182). Treat this as the SDK's own client-side
  assumption, not a documented server guarantee. High Tide's rule is "Cache manifests per-session
  (in-memory) only".
- **sone encrypts every cache entry, not just settings.** §3.1's table hedges ("Encrypted or plain
  cache dir, user-configurable"), but the reference implementation is unconditional: `<hash>.dat` is
  written as `let encrypted = self.crypto.encrypt(data)?; fs::write(&dat_path, &encrypted)?`
  (ref:sone/src-tauri/src/cache.rs:429-433) — the same AES-256-GCM envelope as §3.3, so the 2 GiB
  cap is 2 GiB of AEAD blobs (each with the 17-byte header) and the cache subsystem cannot start
  before the master key resolves. If streamboat copies the cap and tiering, copy or explicitly
  reject the encryption, and state the startup ordering (key resolution gates cache init) either
  way.

#### 4.2a Settings-file versioning and migration

§4.2 requires a `schema_version` on cache entries; the settings file needs the equivalent, and it
is a harder problem because settings cannot simply be dropped and rebuilt like a cache tier. sone
hit this after shipping and had to retrofit a migration path — its own log lines document it:
`log::warn!("Failed to migrate settings to encrypted: {e}")` /
`log::info!("Migrated settings.json to encrypted format")` (ref:sone/src-tauri/src/lib.rs:346-348).
Put a `version` integer in the settings document from v0.1: make loading forward-only (bump on
change, migrate up, refuse to load a version newer than the running binary and say so rather than
silently overwriting), and add a unit test per migration step with a committed fixture of the old
shape. This is one of the cheapest things to do on day one and among the most expensive to retrofit.

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
- **Local logs are a listening-history record — resolve the tension with "no telemetry"
  explicitly, don't let it happen by omission.** §10.1 recommends logging `track_id` and playback
  lifecycle events at `info` with the §4.3 ~50 MB rotated ceiling; the result is a plaintext file
  on disk containing a timestamped record of everything the user played. Logs never leave the
  machine, so "no telemetry" (sent to *streamboat's developers*) stays true, but the file is
  personal data all the same, and a project whose positioning leans on privacy should say so on
  purpose. Do four things: (1) log the track id at `debug`, not `info`, so the default on-disk
  history is short-lived under normal rotation; (2) when the §10.3 debug bundle is generated, its
  manifest must call out that the included log files contain listening history, so "redacted" is
  not misread as "anonymised"; (3) `streamboat purge` (§3.8) deletes logs, not just cache and
  tokens; (4) keep sone's pre-start plaintext logging toggle (§4.3), but label it in settings as
  "file logging (records what you play)" rather than as a developer-only switch, so the user
  understands what enabling it means. No reference project addresses this tension explicitly.

#### 4.5 Data portability between install formats

**Gap: the recommended packaging sequence (§6, "Sequence the packaging work") guarantees a user
loses all local data on their first upgrade path, and nothing in §4.1 draws that consequence.**
AppImage ships first (day one, per the sequence above), Flathub arrives only "after several months
of tagged releases" (§6.1), and §4.1 already documents that Flatpak redirects every XDG directory to
`~/.var/app/$FLATPAK_ID/{config,cache,data}`. A user who follows exactly the upgrade path this
document recommends — install the AppImage at launch, move to the Flatpak once it lands — finds an
empty app with no tokens, no settings and no cache, with no error explaining why. No reference
project solves this; the closest acknowledgement in the set is sone's own comment that the keyring
"may be unreachable on next launch (e.g. AppImage with different D-Bus session)"
(ref:sone/src-tauri/src/crypto.rs) — the same class of problem, but for the key rather than the
data. Specify three things: (1) `streamboat export`/`import` of the settings document — secrets
re-entered on import, not exported, or exported only under an explicit passphrase — which doubles
as the answer for backup and for moving between machines; (2) a first-run probe that checks the
other well-known locations (`~/.config/streamboat`, `~/.var/app/<app-id>/config/streamboat`,
`$SNAP_USER_COMMON`) and offers an explicit import rather than migrating silently; (3) a documented
statement, in `docs/packaging.md`, of what each package format's uninstall does and does not remove
— which pairs with the `streamboat purge` action §3.8 already recommends. Sequence data import with
§4.2a's settings `version` field, since import must run the same forward-only migration ladder as a
normal upgrade.

---

### 5. Licensing

#### 5.1 What the references chose

| Project | License | Notes |
|---|---|---|
| sone / sone-windows | GPL-3.0-only | Declared in `package.json`, `Cargo.toml`, PKGBUILD, snapcraft.yaml and metainfo `<project_license>` |
| High Tide | GPL-3.0 (COPYING); one file (`secret_storage.py`) carries an LGPL-3.0-or-later SPDX header, the rest GPL-3.0-or-later | CONTRIBUTING: "Contributions should be licensed under the **GPL-3**" |
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
- **GPL-3.0-only on the app forecloses the future mobile target — this is a mobile-strategy
  decision, not just a copyleft preference, and the split above is what keeps the door open.**
  Project context states mobile (Android/iOS) is a future target that "the architecture must not
  preclude". GPL-3.0-only is incompatible in practice with Apple's App Store terms for a
  statically-linked iOS binary (the well-known VLC and GNU Go App Store removals over this exact
  conflict); a GPL-3.0-only app cannot be distributed through the only distribution channel iOS
  has. This makes the `core`/apps split above load-bearing for mobile, not optional: keep
  `streamboat-core` **Apache-2.0**, not LGPL-3.0 (LGPL's relinking requirement is itself contested
  for a statically-linked iOS binary under App Store terms), so a future iOS app has a clean
  permissive core to build on. If the owner wants a first-party iOS app later, either that app is a
  separate permissively-licensed codebase over the Apache-2.0 core, or the whole project goes
  Apache-2.0 now — there is no in-between once the desktop apps have shipped as GPL-3.0-only with
  outside contributions. No reference project in this survey faces this tension: TidalSwift is
  macOS-only, and the official TIDAL SDKs are Apache-2.0 precisely because they must be embeddable
  in App Store apps (ref:tidal-sdk-ios/LICENSE). Record this as its own line in the licensing ADR
  in `docs/DECISIONS.md`, and note it changes the answer to Open question 2 below.
- **AGPL-3.0 is a poor fit.** Its network clause only bites when users interact with the software
  over a network. streamboat's headless mode does expose a local control surface, so AGPL is not
  meaningless — but it would deter packagers and integrators (Music Assistant, Mopidy, HA-style
  projects) for a benefit that barely applies to a single-user player. If the owner cares about
  hosted forks, AGPL for the *server* component only is the narrower option.
- **MIT/Apache-2.0 for the whole app** is only right if the goal is maximum adoption including by
  closed products. It also forecloses GPL GStreamer plugins and would make the project the
  path of least resistance for a closed commercial TIDAL client.

#### 5.3 Dependency compatibility notes

- **GStreamer**: core and `gst-plugins-base` are LGPL-2.1 (verified by direct fetch of
  `subprojects/gstreamer/COPYING` and `subprojects/gst-plugins-base/COPYING` at
  `github.com/GStreamer/gstreamer`, both "GNU LESSER GENERAL PUBLIC LICENSE Version 2.1"). "We
  require that all code going into our core packages is LGPL", and plugins with patent issues
  "would need to go into our gst-plugins-ugly module" — both from GStreamer's own licensing FAQ
  (https://raw.githubusercontent.com/GStreamer/gst-docs/master/markdown/frequently-asked-questions/licensing.md,
  read directly and grepped: it contains neither the "practical reasons under the GPL" line nor any
  FFmpeg guidance, despite an earlier draft of this document attributing both to it). **Both of the
  remaining quotes come from one file, not two, and not `gst-plugins-base`'s `LICENSE_readme`
  (no such file exists at HEAD in the GStreamer monorepo — a second earlier-draft misattribution,
  now corrected)**: `gst-libav`'s own `README.md` states, in adjacent lines, "if you are
  distributing an application which has a non-GPL compatible license (like a closed-source
  application) with GStreamer, you have to make sure not to build FFmpeg with GPL code enabled."
  and "Overall, when using plugins that link to GPL libraries, GStreamer is for all practical
  reasons under the GPL itself."
  (https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-libav/README.md,
  lines 15-20, read directly — note the exact wording is "plugins that link to GPL libraries", not
  "GPL linked plugins"). A GPL-3.0 streamboat has no problem here. A permissive streamboat would
  have to restrict itself to LGPL plugins and an LGPL FFmpeg build (i.e. exactly avoid linking any
  plugin that itself links a GPL library, `gst-libav`/FFmpeg built with `--enable-gpl` included).
- **FFmpeg**: LGPL-2.1+ by default; `--enable-gpl` and `--enable-nonfree` change that. tidalt
  builds FFmpeg 7.1.5 with `--disable-everything --disable-programs --disable-doc
  --disable-network --disable-autodetect --disable-shared --enable-static --enable-pic
  --disable-avdevice --disable-swscale --disable-avfilter --enable-protocol=file
  --enable-demuxer=flac,mov,aac,wav,ogg
  --enable-decoder=flac,aac,aac_latm,alac,pcm_s16le,pcm_s24le,pcm_s32le,vorbis
  --enable-parser=flac,aac,aac_latm,vorbis --enable-swresample` — no `--enable-gpl`, so the result
  is LGPL (ref:tidalt/packaging/build-static-ffmpeg.sh:27,47-54 — version verified as
  `FFMPEG_VERSION="${FFMPEG_VERSION:-7.1.5}"`). **Static linking of LGPL code from an
  Apache-2.0 binary imposes the LGPL relinking obligation** (ship object files or the full source
  and build instructions). If streamboat statically links FFmpeg, publish the exact build script
  and the object archives, or dynamically link.
- **fdk-aac**: GPL-incompatible and "therefore nondistributable with GPL parts" per FFmpeg's own
  position; Debian ships it as non-free because the license forbids charging a fee for
  distribution (DFSG "No Discrimination Against Fields of Endeavor"). `fedoraproject.org` is
  blocked from this environment — this is read via a search index of the Fedora Licensing/FDK-AAC
  wiki, corroborated by tookmund.com "AAC and Debian" and the Hydrogenaudio knowledge base, not a
  primary fetch. One nuance worth carrying: Fedora's own posture has shifted over time — the
  license was reviewed as free but the package is no longer "allowed" in Fedora because of patent
  concerns, and Fraunhofer grants no patent license alongside the copyright license. streamboat
  needs AAC *decoding* only, which `avdec_aac` (LGPL FFmpeg) or `faad` covers; never link fdk-aac.
- **Qt 6** open source is mainly LGPLv3 with some modules GPL-only; some modules are GPL-2.0-only
  and others GPL-3.0-only, and mixing those two is itself a violation — Qt's own FAQ gives the
  concrete example that mixing GPL-3.0-only Spatial Audio with GPL-2.0-only TextToSpeech "violates
  GPL-3.0". `doc.qt.io` is blocked from this environment; this is read via a search index of the
  Qt open-source licensing FAQ, not a primary fetch — re-verify module-by-module before Qt is
  chosen (https://doc.qt.io/qt-6/licensing.html, https://www.qt.io/faq/qt-open-source-licensing).
  A GPL-3.0-only app can use LGPLv3 and GPL-3.0-only Qt modules but must avoid GPL-2.0-only ones.
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

**Copy this disclaimer paragraph, but do not copy reference-project *feature* copy verbatim —
some of it is stale.** TIDAL discontinued MQA and Sony 360 Reality Audio on 2024-07-24
("BREAKING: MQA, Sony 360 audio no longer supported July 24th 2024; removed references to these
formats", ref:python-tidal/HISTORY.rst:88, with a corresponding skipped test at
ref:python-tidal/tests/test_media.py:236, reason "MQA albums appears to fallback to LOSSLESS"), and
the current quality ladder is LOW/HIGH/LOSSLESS/HI_RES_LOSSLESS — yet sone's own README still
advertises "**Lossless FLAC and MQA streaming** up to Hi-Res (24-bit/192kHz)"
(ref:sone/README.md:58). If streamboat's own README or metainfo copy is adapted from a reference
project's feature list rather than the disclaimer paragraph above, check each format claim against
the current API rather than assuming the reference project is current.

One unverified but material data point: a search result attributes to TIDAL's Developer Terms 2.0
the statement that "The Player module in the SDK constitutes the only allowed way for third-party
applications to incorporate playback of TIDAL content", corroborated at second hand by an
independent search of `developer.tidal.com/documentation/guidelines-developer-terms-2_0` returning
both that sentence and "playbacks shall only be made available through TIDAL's SDKs, namely an
official, unmodified version of the TIDAL Player module". `developer.tidal.com` and `tidal.com` are
both unreachable from this environment, so **this is not verified from the primary source** and
must be checked by the legal/ToS research topic before any claim is published.

**Scope distinction to get right in `docs/legal.md`, regardless of how that verification lands**:
those Developer Terms govern the official developer-program API and SDK (the path High Tide, sone
and python-tidal explicitly do *not* take). streamboat's declared stance — per the owner's decision
— is the unofficial API that python-tidal/High Tide/sone use, which is governed by the consumer
Terms of Service instead. Conflating the two documents would produce the wrong legal analysis: a
"Player-module-only" restriction in the *developer* terms does not, by itself, establish that the
*unofficial* API route violates the *consumer* ToS — the two are separate questions and both need
their own primary-source read.

#### 5.5 EU Cyber Resilience Act — record the determination, do not skip it

The report's own date (2026-09-07) sits four days before a CRA compliance milestone: obligations
for manufacturers to report actively exploited vulnerabilities and severe incidents to ENISA and
national CSIRTs (24h early warning / 72h notification / 14 days after a patch) begin 2026-09-11;
conformity-assessment-body rules applied from 2026-06-11; full compliance is due 2027-12-11
(https://openssf.org/public-policy/eu-cyber-resilience-act/, https://orcwg.org/cra/). A free,
non-commercial FOSS project is out of scope, but that is a determination someone has to make and
write down, not assume. "Open-source stewards" — legal persons systematically supporting FOSS
intended for commercial activity — carry lighter Article 24 duties (cybersecurity policy,
cooperation on vulnerability handling, reporting) and are explicitly exempt from administrative
fines under Article 64(10); an individual maintainer distributing a free client is neither
manufacturer nor steward. Record the determination in `docs/legal.md`.

**Keep this separate from `SECURITY.md`'s coordinated-disclosure policy — they are two different
clocks, and copying the CRA's numbers into `SECURITY.md` promises the wrong timeline to the wrong
party.** The 24h/72h/14-day figures above are for *reporting actively exploited vulnerabilities* to
ENISA/national CSIRTs; they say nothing about how quickly a maintainer acknowledges or fixes a bug
a researcher reports privately. Write two things, in two places: (1) a coordinated-disclosure
policy in `SECURITY.md` — acknowledge within N business days, a target fix window (90 days is the
common default), a credit policy, and an explicit scope note (§8.3: token handling and the local
control API are in scope; TIDAL's own service is not; **reports about circumventing TIDAL's DRM are
out of scope and will not be accepted** — consistent with the project's declared stance of never
designing or documenting circumvention); (2) the CRA scope determination itself, and — only if the
determination ever changes (e.g. a paid/hosted build appears) — the ENISA/CSIRT reporting timeline,
in `docs/legal.md`, not `SECURITY.md`. Precedent is thin: only tidal-hifi ships a `SECURITY.md` in
the reference set, and it is minimal (ref:tidal-hifi/SECURITY.md). Enable GitHub private
vulnerability reporting as the intake channel — it needs no public email address.

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
  **Treat this policy as unsettled and re-read it before submission, regardless of the history
  below.** The currently quoted text was re-read directly against
  `flathub-infra/documentation` and matches verbatim. A separate claim — that commit `992f57b`
  (2026-05-29, "Reword LLM policy to make it clear it's not allowed") briefly replaced this text
  with an outright ban ("Applications containing AI-generated or AI-assisted code, documentation, or
  other content are not allowed ... Exceptions may be granted for mature, well-maintained
  projects"), widely reported as a ban, before reverting to the disclosure-plus-reviewer-discretion
  regime quoted above — **could not be independently re-verified in this pass**: GitHub API access
  to `flathub-infra/documentation`'s commit history was not enabled for this environment. Do not
  repeat the commit narrative as settled fact without checking the commit directly; the safer
  framing is "this policy has changed before and may change again", not the specific SHA and date.
  Treat the quoted text as a dated snapshot (read 2026-09-07 at commit HEAD of
  `flathub-infra/documentation`), add a standing task to re-read it immediately before any Flathub
  submission, and note the adjacent linter rule: sandbox-escape exceptions (`home`, `host`,
  `flatpak-spawn`, arbitrary bus names) are reported as "not... granted if there are signs of LLM
  usage in the software or in the exception PR" — which interacts directly with the permission
  plan below.

**Runtime choice is the first manifest decision, it is a standing upgrade obligation, and it
decides which audio codecs even exist inside the sandbox.** Neither the requirements doc nor §9's
audio-pipeline treatment previously named a runtime. Flathub's requirements state: "The runtime(s)
used in the manifest must be hosted on Flathub and must be the latest version at that time of
submission" and "Submissions using an end-of-life runtime, extension or baseapp will not be
accepted" — so the runtime version is not a one-time choice but a recurring release-calendar item
(GNOME/KDE/freedesktop runtimes go EOL roughly yearly; see §6.1a's EOL criterion "3+ runtime
updates behind"). High Tide pins `org.gnome.Platform`/`org.gnome.Sdk` version 50 and — the
load-bearing detail for a build with no network access — supplies its extra runtime pieces
(`alsa-utils`, `libportal`, `python3-tidalapi`) as manifest *modules*, built from source inside the
sandbox, not fetched at build time. **Zero of the 21 checkouts use
`org.freedesktop.Platform.ffmpeg-full` or any `add-extensions` block** (grep for
`ffmpeg-full|add-extensions|org.freedesktop.Platform.ffmpeg` across all of `ref/`: no hits), so
there is no reference precedent for pulling extra codecs in as a Flatpak extension — if
streamboat's decoder set exceeds what the chosen runtime ships, those decoders must be built as
manifest modules, the same way High Tide builds its Python dependencies. Decide and record: which
runtime, which decoders come from the runtime vs. are built in-manifest, and put "runtime version
bump" on the release calendar (https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/02-requirements.md
lines 133-137, 572-574, read 2026-09-08; ref:high-tide/build-aux/io.github.nokse22.high-tide.json).

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
  matches the Flatpak ID, is an exact subname of it, or is an MPRIS subname. **Correction**: the
  linter has no rule literally named `finish-args-own-name-cpt` — that string does not exist in
  `flatpak_builder_lint/checks/finish_args.py` (fetched from
  `flathub-infra/flatpak-builder-lint` at HEAD and read directly); `cpt` there is an unrelated local
  variable used to build `finish-args-portal-impl-{cpt}-talk-name`. The real rule ids are
  **`finish-args-unnecessary-appid-own-name`** and **`finish-args-unnecessary-appid-mpris-own-name`**
  — both carry the linter's info text "This is granted by default" — and for any *other* own-name
  the linter builds a per-name id dynamically, `finish-args-own-name-<bus.name>` (or
  `finish-args-own-name-wildcard-<name>` for a trailing `.*`). Don't cite a specific rule id you
  haven't read from the linter source; the default sandbox policy already lets an app own
  `org.mpris.MediaPlayer2.$FLATPAK_ID` with no extra finish-arg at all regardless (confirmed:
  https://github.com/flatpak/flatpak-docs/blob/master/docs/sandbox-permissions.rst states an app
  may "own its own namespace named by $FLATPAK_ID, subnames of it and
  org.mpris.MediaPlayer2.$FLATPAK_ID"). **Now confirmed (previously flagged unverified)**: the
  linter *does* carry a specific never-granted rule for the matching talk-name. Fetched directly
  from `flathub-infra/flatpak-builder-lint`'s `flatpak_builder_lint/checks/finish_args.py` (lines
  515-524): a `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID` request emits rule id
  **`finish-args-mpris-flatpak-id-talk-name`** with info text "This is granted by default". The
  live `staticfiles/exceptions.json` in that repository contains zero entries for that rule id,
  which is direct evidence for "never granted" independent of the still-blocked `docs.flathub.org`
  page. Do not request that talk-name — the default policy already covers it and the linter will
  flag the explicit grant as redundant.
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

**CI-side build check**: High Tide builds the Flatpak for both x86_64 and aarch64 using
`flatpak/flatpak-github-actions/flatpak-builder@v6` inside
`ghcr.io/flathub-infra/flatpak-github-actions:gnome-48`
(ref:high-tide/.github/workflows/flatpak.yml). The trigger is narrower than "every push and PR":
`push: branches: [master], paths-ignore: ['**/README.md']` and
`pull_request: branches: [master], types: [review_requested, ready_for_review]` — i.e. pushes to
`master` (excluding README-only changes) and PRs only once review is requested or the PR is marked
ready for review, not every PR event. Do this from day one regardless of the exact trigger — it is
the cheapest way to keep the manifest honest.

#### 6.1a Flathub: metainfo requirements, submission mechanics, maintenance and verification

The requirements doc above says only "metainfo is mandatory and must pass validation" — not enough
to write the file from. Concrete fields, all from
`flathub-infra/documentation` (read 2026-09-07):

- **Mandatory fields**: `<id>` exactly equal to the Flatpak app ID and the manifest; a
  `<metadata_license>` for the metainfo file itself (an AppStream-permissive value, e.g. `CC0-1.0`
  or `FSFAP` — **not** the same field as `<project_license>`); `<project_license>` as an SPDX
  identifier (e.g. `GPL-3.0-only`); `<name>` and `<summary>`; `<content_rating type="oars-1.1">`
  generated from the OARS website; at least one screenshot, hosted as a direct web resource; a
  `<developer id="..."><name>...</name></developer>` block with a reverse-DNS id (exactly one,
  untranslated); a `<releases>` tag ("Applications must supply a releases tag ... to pass
  validation"); `<url type="homepage">` at minimum, `url type="vcs-browser"` strongly recommended.
- **Two fields the "mandatory fields" list above omits, and both matter for streamboat
  specifically**: (1) `<branding>` — "applications should set a brand color in both light and dark
  variants": `<branding><color type="primary" scheme_preference="light">#ff00ff</color><color
  type="primary" scheme_preference="dark">#993d3d</color></branding>`, used by Flathub and native
  store clients for banners and app pages. (2) A device-relations block, `<requires>` or
  `<supports>`, which is the metainfo half of the mobile-readiness decision §8.1 already makes for
  the code: a desktop-only app declares
  `<requires><control>keyboard</control><control>pointing</control><display_length
  compare="ge">768</display_length></requires>`; declaring mobile support later means moving to
  `<supports>` with `<control>touch</control>` added and `display_length` lowered to `"ge">360`.
  Ship the desktop-only `<requires>` form now and note in the licensing/mobile ADR (§5.2) that it
  is the metainfo counterpart of that decision. Also: `<content_rating type="oars-1.1" />` must be
  *generated* from the OARS site (hughsie.github.io/oars/generate.html), not hand-written.
- **Packaging-metadata validation belongs in CI, not only in the Flathub build.** §7.1's matrix has
  a "flatpak build" job but nothing that validates the metainfo, the `.desktop` file or the
  manifest style, and these are the most common first-submission rejections. High Tide wires three
  validators into its own `meson test` run — `desktop-file-validate` on the merged desktop file,
  `appstream-util validate` on the merged appdata XML, and `glib-compile-schemas --strict --dry-run`
  on the gschema, each guarded with `find_program(..., required: false)`
  (ref:high-tide/data/meson.build:49-77). Add `appstreamcli validate --explain` (the modern
  successor to `appstream-util`), `desktop-file-validate` and `flatpak-builder-lint
  manifest|appstream|repo` as explicit §7.1 jobs. Flathub also imposes a mechanically-checkable
  manifest style: JSON manifests must be RFC-7159 valid with 4-tab indentation, LF line endings,
  UTF-8, a trailing newline, double quotes, no trailing commas, and comments only inside `"//":` or
  `"x-comment":` keys; YAML manifests must use 2-space indentation, blank lines between modules, and
  no vertical alignment of values.
- **Quality guidelines with numbers**: app name ideally ≤15 chars, must be <20; summary ideally
  10–25 chars, must not exceed 35, sentence case, no trailing period, must not repeat the app name,
  must not start with an article, must not mention the toolkit/language/"free and open source";
  description ~3–6 lines at 70 columns, feature lists ≤10 items; screenshots max 1000×700 (2000×1400
  HiDPI), 3–6 for a medium app, captured on Linux with default system settings and native window
  decoration, one-sentence caption each, at least one in English; icon SVG or PNG ≥256×256, square,
  no baked-in shadows.
- **Submission mechanics**: fork `github.com/flathub/flathub` with "Copy the master branch only"
  **unchecked**; clone the `new-pr` branch; branch off it; open the PR against base branch `new-pr`
  (not `master`), titled `Add io.github.<owner>.streamboat`. Build and lint locally with
  `org.flatpak.Builder` first. Comment `bot, build` to trigger a test build. On approval the
  submission is merged into a new repo under the `flathub` GitHub org, and you get a write-access
  invitation that must be accepted within **one week** (2FA required first). The official build
  publishes in ~1–2 hours.
- **Post-acceptance maintenance**: a global External Data Checker runs every two hours across all
  app repos; sources carrying an `x-checker-data` block get automatic update PRs. Two automation
  levels — GitHub automerge (maintainer approves once, CI gates future merges) or
  `automerge-flathubbot-prs` in `flathub.json` (checker merges on its next run) — both require
  Flathub admin approval and target verified apps or high-update-volume apps. Apps must use only
  `master` (→ stable) or `beta` branches. EOL is declared via `end-of-life` in `flathub.json`
  (`end-of-life-rebase` for a rename/migration). Flathub's own EOL criteria include 2+ years of
  upstream inactivity, an unmaintained Flatpak package, or being 3+ runtime updates behind, with a
  one-month maintainer notice. **This is a native alternative to copying sone's hand-rolled
  `yq`-rewrite-and-PR pipeline** (§6.1 below) — decide between them rather than assuming the latter.
- **Verification (the checkmark badge)**: an `io.github.*` app ID is verified for free and
  immediately by authenticating as the GitHub repo owner (or an org admin) — another reason to
  take the `io.github.<owner>.streamboat` ID already recommended, and the practical precondition
  for the automerge options above. Domain-based alternatives exist (HTTPS token at
  `https://<domain>/.well-known/org.flathub.VerifiedApps.txt`, DNS TXT record, GitLab variants) but
  are unnecessary here.
- **The app-ID / GitHub-owner choice is permanent and touches far more than Flathub — decide it as
  an explicit ADR before the first commit, not as a byproduct of "where did I push the repo".**
  `io.github.<owner>.streamboat` is baked into: the Flatpak ID and metainfo `<id>`, the Flathub
  repo name, the D-Bus/MPRIS bus name (§6.1, §10.5), the macOS bundle identifier (§4.1 uses it as
  the `Application Support`/`Caches`/`Logs` directory name), and — once §6.4 introduces one — the
  Windows AppUserModelID. Settle two things now: (a) personal account or a new GitHub org — publish
  under whichever one will still own the repo in three years, since moving later costs a Flathub
  `end-of-life-rebase` plus a config-directory migration on every user's machine; (b) check the name
  is free on crates.io/npm/PyPI/AUR/Flathub/Snap Store/winget and as a domain *before* locking it
  in. Precedent for the org-vs-personal split: sone is `io.github.lullabyX.sone` (personal), High
  Tide is `io.github.nokse22.high-tide` (personal), Strawberry is
  `org.strawberrymusicplayer.strawberry` (owns its own domain) — an org gives a bus-factor-safe path
  to keeping the verification badge if the owner is ever unavailable.

Sources: https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/03-metainfo-guidelines/index.md
and `.../01-quality-guidelines.md`;
https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/05-submission.md;
https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/06-maintenance.md;
https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/10-verification.md
(all read 2026-09-07).

#### 6.2 Snap

Sone's `snapcraft.yaml` (ref:sone/snap/snapcraft.yaml) shows the audio-specific parts:

- `confinement: strict`, `base: core24`, `grade: stable`.
- Plugs: `home`, `network`, `network-bind`, `network-status`, `audio-playback`, `alsa`,
  `screen-inhibit-control`.
- **`alsa` is not auto-connected**: the description tells users "For exclusive ALSA output, run:
  `sudo snap connect sone:alsa`". Budget for that support burden or treat exclusive mode as
  unsupported under Snap. **Not independently verified**: Snap publishers can request
  auto-connection for an interface through a store review request on snapforum
  (snapcraft.io/forum), which was not consulted here. Before committing to Snap as an exclusive-ALSA
  channel, check whether the `alsa` interface has ever been auto-connected for a media player and
  what reviewers required — sone's own README implies the request was either not made or not
  granted, which is itself informative but not conclusive.
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

**The packaging sequence has a gap: no portable Linux binary.** Flathub requires "a meaningful
history of development" (§6.1) and Homebrew requires a 30-day-old repo at minimum (§6.5) — so for
the first several months the only Linux channels this plan reaches are AUR and Cloudsmith deb/rpm.
A user on Fedora Silverblue, NixOS, Debian stable, or any distro outside the Cloudsmith matrix has
nothing to install. Both large reference GUI clients ship a portable artifact: Strawberry has a
dedicated AppImage job (ref:strawberry/.github/workflows/build.yaml) and tidal-hifi builds an
AppImage via electron-builder (ref:tidal-hifi/build/electron-builder.yml). **Add AppImage (or a
static tarball for the daemon/CLI) as the first packaging deliverable**, before AUR. One caveat the
references already hit: an AppImage has no stable app identity for the keyring — sone's own
`crypto.rs` comments that the keyring "may be unreachable on next launch (e.g. AppImage with
different D-Bus session)" — which is exactly why the encrypted-file fallback in §3.3 is mandatory,
not optional, for this channel.

**Nix is a fourth, zero-review, day-one channel for exactly the NixOS user the paragraph above
says has nothing to install — and it doubles as a from-source CI build gate.** Three checkouts
already expose *package* outputs, not just a dev shell (§8.4 covers the dev-shell use only — this
is the separate, additional use as a distribution target): sone's `flake.nix` exposes
`packages.${system}.sone` and `apps.${system}.default`, and — the part worth copying regardless of
whether Nix ships as a channel — `checks.${system}.build = self.packages.${system}.sone`, so
`nix flake check` in CI is a full from-source build gate for free
(ref:sone/flake.nix:13-24,47). High Tide's flake exposes
`packages.high-tide = pkgs.python313Packages.buildPythonApplication {...}` with
meson/ninja/blueprint-compiler/libadwaita/glib-networking/gst_all_1/libsecret/libportal/alsa-utils
in its inputs (ref:high-tide/flake.nix:87-102). mopidy-tidal exposes `packages.default`
(ref:mopidy-tidal/flake.nix:66). Ship a flake with a package output from the first release and add
`nix flake check` to the §7.1 matrix; treat an actual nixpkgs submission as a later, optional step.

**Add a package-install smoke test to CI — a build-only job does not catch a missing runtime
dependency, and sone ships a complete, copyable harness that does.** §7.1's minimum matrix has a
"flatpak build" job but nothing that installs a built `.deb`/`.rpm` and runs it; an undeclared
dependency, a missing GStreamer plugin, or an MPRIS name that never appears on the bus are exactly
the bugs a build-only job cannot catch and that ship straight to users. sone's
`build-scripts/test/{all,common,deb,rpm,pacman}.sh` run per-distro in Docker (Ubuntu 22.04/24.04,
Debian 12 for deb via `apt-get update && dpkg -i /pkg/*.deb || true && apt-get install -f -y` — the
`install -f` step is what actually validates declared dependencies are correct and sufficient;
archlinux:latest for pacman). Inside each container it starts a D-Bus session and Xvfb, launches the
app, and asserts six machine-readable checks: the package is registered installed, `ldd` reports no
"not found", a window appears within 15 s (polled via `xdotool`), the MPRIS name
(`org.mpris.MediaPlayer2.sone`) appears on the bus, GStreamer device enumeration succeeds, and the
config directory gets created — plus an AppImage code path (`cd /tmp/squashfs-root && ./AppRun`)
(ref:sone/build-scripts/test/{deb,common,pacman}.sh). Add this as a CI job on release-candidate
tags; for streamboat's headless mode the equivalent check is "the control socket/D-Bus name
appears", not a window.

**Ship `.github/FUNDING.yml` from day one, and add the matching `<url type="donation">` to the
metainfo (§6.1a) once one exists.** Seven of the 21 checkouts ship a `FUNDING.yml` — **correction:
an earlier draft's count was right but its enumeration named only six**; the seventh is
`tidal-fokka-engineering-`. Full list: sone/sone-windows (`patreon: lullabyX`), High Tide
(`github: Nokse22` + `ko_fi: nokse22`), TidaLuna (`github: [inrixia]`), tidal-hifi
(`github: [Mastermindzh]` + `custom: [https://www.paypal.me/mastermindzh]`), tidalswift
(`github: [melgu]`), and `tidal-fokka-engineering-`. sone additionally declares its donation link
to Flathub itself,
`<url type="donation">https://patreon.com/lullabyX</url>`, alongside `<url type="bugtracker">`,
`<url type="vcs-browser">` and `<url type="contribute">`
(ref:sone/data/io.github.lullabyX.sone.metainfo.xml). Set this up before the packaging costs land
(§6.4, §6.5), not after — it partially answers Open question 8 ("who pays"), by giving a mechanism
for users to help pay rather than settling who pays outright.

#### 6.3a Bundling the media runtime — Windows and macOS have no system GStreamer/FFmpeg

§6.4 and §6.5 below cover installer format, signing and notarization on the assumption the binary
is self-contained. It is not, for any GStreamer- or FFmpeg-based stack: Windows and macOS ship
neither runtime, so the installer must also carry the entire media runtime, plus a bundled-build
code path that points the plugin scanner at the bundle instead of the (nonexistent) system
install. This is the single most consequential Windows-specific fact in the reference set, and it
sits in the one checkout this report otherwise declined to inspect in depth (`sone-windows`, §1).

- **Windows (sone-windows).** `scripts/prepare-gstreamer.js` generates two packaging artifacts at
  build time: (a) `src-tauri/gstreamer-hooks.nsi`, an NSIS `!macro NSIS_HOOK_POSTINSTALL` that
  copies `gstreamer-runtime/*.dll`, `lib/gstreamer-1.0/*.dll` and `lib/gio/modules/*.dll` into
  `$INSTDIR`, with a matching `NSIS_HOOK_PREUNINSTALL` that deletes them on uninstall; and (b)
  `src-tauri/gstreamer-fragment.wxs`, a WiX fragment with one `<Component>`/`<File>` pair per DLL
  (`ffi-7.dll`, `FLAC-8.dll`, `gio-2.0-0.dll`, `glib-2.0-0.dll`, plus the gmodule/gobject/
  gstadaptivedemux/gstaudio/gstbase/gstisoff/gstnet/gstpbutils families)
  (ref:sone-windows/src-tauri/gstreamer-hooks.nsi, ref:sone-windows/src-tauri/gstreamer-fragment.wxs,
  ref:sone-windows/scripts/prepare-gstreamer.js). **Both generated files embed the developer's own
  absolute local path** (`C:\Users\lvllaby\Documents\sone\src-tauri\gstreamer-runtime\...`) — a
  reproducible-build break (§7.5) and an incidental username leak that §3.5's never-commit list does
  not currently cover; generate these files from a CI-relative path, not a developer machine.
- **macOS (Strawberry).** Strawberry deploys with a purpose-built tool, not a stock one:
  `cmake/Dmg.cmake` does `find_program(MACDEPLOYTOOL_EXECUTABLE NAMES ntool)` with the comment "get
  it from https://github.com/jonaski/ntool", and `src/engine/gststartup.cpp` sets
  `gst_plugin_scanner` and the GIO module search paths to bundle-relative directories at runtime —
  i.e. the app must detect "I am running from an app bundle" and repoint GStreamer's plugin/module
  discovery away from the (absent) system locations.
- **Three consequences to design for from the start, whichever stack streamboat uses**: (1) shipping
  LGPL-2.1 GStreamer DLLs/dylibs carries the same relinking obligation §5.3 already identifies for
  a statically linked FFmpeg — publish the exact bundled-runtime build/version list alongside the
  installer; (2) the app needs a bundled-vs-system code path for plugin/module discovery on both
  Windows and macOS, not just "install GStreamer and hope PATH resolves it"; (3) generate packaging
  fragments (`.wxs`, `.nsi`) from CI-relative paths only, and mark them `linguist-generated` in
  `.gitattributes` (§2.11).

#### 6.4 Windows

- **Installer format**: tidal-hifi produces an MSI via electron-builder (`win: target: msi`);
  Strawberry ships an NSIS installer (`dist/windows/strawberry.nsi.in` with
  `Capabilities.nsh`, `FileAssociation.nsh`, `Registry.nsh`). MSIX is required only for the
  Microsoft Store and forces a signed package plus a store listing; skip it initially.
- **Signing**: unsigned installers hit SmartScreen. Options in 2026: Azure Trusted Signing /
  Azure Artifact Signing, reported at $9.99/month Basic (up to 5,000 signatures) and $99.99/month
  Premium (up to 100,000 signatures, 10 certificate profiles), versus an EV code-signing
  certificate at roughly $280–500/year on a hardware token. Trusted Signing certificates cannot be
  exported, so losing eligibility means losing the ability to sign. **Re-verify before relying on
  this**: `azure.microsoft.com` and `learn.microsoft.com` are blocked from this environment, so
  these numbers come from secondary sources (devclass 2026-01-14, melatonin.dev, Microsoft
  Community Hub), and eligibility is reported there as "verified US, Canadian, EU and UK businesses
  and self-employed individuals" — broader than "individuals in the USA/Canada, organisations in
  the EU/UK" as an earlier draft of this document stated.
- **winget**: submit a manifest PR to `microsoft/winget-pkgs`. `InstallerSha256` is required per
  installer entry and WinGet blocks installation on a hash mismatch; an automated validation
  pipeline installs and tests the package, confirmed by the `microsoft/winget-pkgs` README and PR
  threads. **Unverified**: a "PR assigned to you has a 7-day timer before the bot closes it" could
  not be confirmed — `learn.microsoft.com` is blocked from this environment and the only auto-close
  behaviour found in reachable sources is tied to the `Needs-Author-Feedback` label, not to
  assignment; treat the 7-day figure as unconfirmed until read from the primary policy doc. **Two
  requirements the report previously omitted, and the ones most likely to fail a first submission**:
  the installer must support a silent/unattended install
  (`InstallerSwitches: Silent` / `SilentWithProgress`, e.g. `/S` for NSIS or `/qn` for msiexec) —
  the automated validation pipeline installs and uninstalls unattended in a sandbox, and an
  installer that shows UI fails outright; and the manifest must declare `Scope` (user vs machine) —
  a per-user install avoids UAC and is the better default for a music player, but it changes the
  install path and therefore the `%LOCALAPPDATA%` layout assumptions in §4.1. Decide both before
  cutting the first Windows installer. Generate the manifest with `wingetcreate`; automate updates
  from the release workflow once the release assets have stable names. **Two more requirements from
  the primary validation doc, both cheap to satisfy and both likely first-submission failures if
  ignored**: `InstallerUrl` must be HTTPS and its domain must be an approved official source for the
  publisher, discoverable by navigating from the publisher's own site — mirrors, aggregators and URL
  shorteners get flagged (`doc/Validation.md`, "Manifest URLs" / step 06; including `PackageUrl` in
  the manifest helps a moderator confirm this quickly); and a package flagged as a Potentially
  Unwanted Application "cannot be accepted, regardless of the application's legitimacy" (step 07) —
  worth knowing given streamboat talks to an unofficial API and embeds a client credential (§3.4),
  either of which a naive heuristic scanner could flag. Comment `@wingetbot run` on the PR to
  re-trigger validation after fixing a hash or URL issue.
  (https://raw.githubusercontent.com/microsoft/winget-pkgs/master/doc/Validation.md, read directly.)
- **AppUserModelID (AUMID) and install scope, together, are what make Windows media transport
  controls (SMTC) work — decide both at the same time as the app ID (§6.1a), not as an
  afterthought.** §9.2 names SMTC as the Windows now-playing integration but does not say what makes
  it appear: the process must call the AUMID-setting API at startup (Win32
  `SetCurrentProcessExplicitAppUserModelID` or the stack's equivalent), and the installer must stamp
  the *identical* AUMID onto the Start Menu shortcut it creates — a mismatch is why a media app's
  transport controls silently fail to appear in the Windows media flyout, for a feature this project
  is meant to headline. No reference project in the survey sets an AppUserModelID at all (grep for
  `AppUserModelID|SetCurrentProcessExplicitAppUserModelID` across all 21 checkouts: zero hits — even
  `sone-windows`, the only native Windows build here, relies on framework defaults), so this is a
  no-precedent item to specify, not a pattern to copy. Tie it to the per-user-vs-per-machine `Scope`
  decision above, since the two install modes place the Start Menu shortcut in different roots. Add
  "SMTC transport controls appear and respond" to the per-OS manual test matrix §9.2 already
  proposes.
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
- **Homebrew cask**: notability floor is **disjunctive** — 30 forks *or* 30 watchers *or* 75 stars
  normally, 90 forks *or* 90 watchers *or* 225 stars for a self-submission by the repo owner (any
  one of the three numbers clears the bar; it reads as a conjunction in slash notation but the
  policy text is "at least 30 forks, 30 watchers or 75 stars"). A repository under 30 days old is
  normally ineligible; casks
  must pass Homebrew's Gatekeeper audit and "must not require System Integrity Protection or
  Gatekeeper to be disabled or bypassed", and casks failing those checks are deprecated, disabled
  and removed (https://github.com/Homebrew/brew/blob/main/docs/Package-Acceptance-Policy.md,
  .../Acceptable-Casks.md, .../Homebrew-Security-and-Supply-Chain.md). In short: **no
  notarization, no Homebrew.** Ship a personal tap in the meantime. **Gap: Homebrew also applies a
  download cooldown that changes release-to-availability latency.**
  `Homebrew-Security-and-Supply-Chain.md` adds a "Cooldowns on riskier ecosystems" section: "For
  ecosystems with a track record of fast-moving supply-chain attacks, Homebrew applies a download
  cooldown: a freshly-published upstream version is not adopted immediately." Check whether the
  ecosystem streamboat's cask formula draws from is on that list before promising users a same-day
  `brew install` on release day.
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
- **Enforce the changelog in CI.** tidal-sdk-ios runs `check-version-sync.sh` on PRs touching
  `version.txt` (trigger: `pull_request: paths: ['version.txt']`); the script is nine lines (a
  shebang plus an if/else/fi block with success/failure echoes), and its single load-bearing line
  is `grep -q "$(cat ./version.txt)" ./CHANGELOG.md`
  (ref:tidal-sdk-ios/.agents/skills/prepare-release/scripts/check-version-sync.sh — corrected from
  an earlier "four lines" claim in this document). tidal-sdk-web
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
  **No reference project verifies its own release artifacts beyond that.** A grep for
  `cosign|sbom|cyclonedx|attest-build-provenance` across all 21 checkouts returns no hits (drop
  `spdx` from that expression — it matches license-header comments, not provenance tooling: e.g.
  `# SPDX-License-Identifier: GPL-3.0-or-later` throughout High Tide's source and 38 files total
  across the set). **Correction — provenance and OIDC are two different mechanisms and only one of
  them has precedent**: only tidal-cli publishes actual provenance
  (`npm publish --access public --provenance`, ref:tidal-cli/.github/workflows/release.yml:39).
  tidal-sdk-web declares `id-token: write # Required for OIDC` and uses that OIDC token for npm
  *trusted publishing* (short-lived credential exchange, no long-lived npm token in CI) — but its
  publish step explicitly sets `NPM_CONFIG_PROVENANCE: "false"`
  (ref:tidal-sdk-web/.github/workflows/release-to-npm.yml:8,19), i.e. it deliberately disables
  provenance attestation while using OIDC for authentication only. So the precedent is: one project
  (tidal-cli) attests provenance; two projects (tidal-cli, tidal-sdk-web) use OIDC trusted
  publishing; nobody attests a Linux binary. Treat provenance and
  signing as a **no-precedent line item to budget for**, not a copyable pattern: recommend
  GPG-/SSH-signed git tags, Sigstore/cosign keyless signing of every release asset (OIDC from
  GitHub Actions, no key to manage) alongside `checksums.txt`, `actions/attest-build-provenance`
  for provenance, and an SBOM (CycloneDX or SPDX) generated from the same lockfile the license scan
  in §5.3 already reads — nearly free once a license scanner is wired up, and it is what a
  downstream distro packager or a CRA-style enquiry (§5.5) will ask for.
- **Release automation tooling and monorepo versioning are both unresolved.** No reference project
  uses a release-automation tool — a grep for
  `release-please|changesets|semantic-release|cargo-dist|goreleaser|nfpm` across the checkouts
  hits only vendored `.goreleaser.yml` files inside tidalt's Go dependencies, not tidalt's own
  tooling. The closest working patterns are hand-rolled:
  `ref:tidal-sdk-ios/.agents/skills/prepare-release/scripts/{bump-version,check-release-needed,
  extract-release-notes,suggest-changelog}.sh` invoked by CI, and tidal-sdk-web's per-package
  `check-version-bump.sh` matrixed over changed `packages/**/package.json`
  (ref:tidal-sdk-web/.github/workflows/check-changelog-files.yml). Pick a tool appropriate to the
  chosen stack (release-please / changesets / cargo-release+cargo-dist / goreleaser+nfpm) **and**
  make an explicit owner decision the report otherwise skips: a single version for the whole repo,
  or `core` versioned and released independently of the apps — the latter is what makes a
  permissively licensed core (§5.2) actually adoptable by other clients, which is the stated reason
  for the license split in the first place.
- **Gap: pick one file as the single source of version truth, and add a CI check that the metainfo
  `<release>` entry tracks it — this is the field Flathub and Snap Store actually show users, and
  it is the one most likely to drift.** sone's working pattern derives every packaging version at
  build time from `package.json`: snapcraft does
  `VERSION=$(node -p "require('./package.json').version"); craftctl set version="$VERSION"` and
  the PKGBUILD does the same node read (ref:sone/snap/snapcraft.yaml:140-141). What sone does
  **not** derive is the metainfo `<releases>` block — it is hand-maintained, one `<release
  version="0.21.0">` entry per tag added by hand
  (ref:sone/data/io.github.lullabyX.sone.metainfo.xml). Specify three things: (1) one file is the
  version source, every other packaging file (snapcraft, PKGBUILD, debian/changelog, the `.spec`,
  the metainfo) derives from it at build time; (2) a CI job asserting the newest metainfo `<release
  version="...">` equals the git tag being built — the same shape as tidal-sdk-ios's
  `grep -q "$(cat ./version.txt)" ./CHANGELOG.md` check already cited above; (3) put "add a
  metainfo `<release>` with user-visible notes" in the release runbook below as a mandatory step,
  since Flathub validation requires the `<releases>` tag and a stale one is what makes a weekly
  release cadence look abandoned to a Flathub browser.
- **Gap: write the release procedure down as an ordered runbook, and add an API-stability gate for
  `core` now that it exists as a permissively-licensed target for other clients (§5.2).** Nothing in
  this document specifies the human sequence for cutting a release. Base `docs/releasing.md` on the
  one working end-to-end precedent in the set — tidal-sdk-ios keeps `bump-version.sh`,
  `check-release-needed.sh`, `check-version-sync.sh`, `extract-release-notes.sh` and
  `suggest-changelog.sh` under `.agents/skills/prepare-release/scripts/`, and CI invokes those exact
  scripts from those exact paths (ref:tidal-sdk-ios/.github/workflows/changelog-check.yml) — and
  make it executable, not prose: string freeze (§9.1) → bump the single version source above →
  derive packaging versions → add the metainfo `<release>` entry → move `[Unreleased]` to a dated
  changelog section → tag → CI build → checksums + signing/attestation (above) → package-install
  smoke test (§6.3) → Flathub PR → AUR → winget → announce → post-release verification. Separately,
  add a semver/API-diff gate on `core` (`cargo-semver-checks` / api-extractor / japicmp / apidiff —
  **[STACK]**) plus a written deprecation policy; no reference project needs this because the
  official SDKs sidestep API stability by regenerating from the OAS (§2.4) rather than hand-writing
  a stable surface — treat it as a no-precedent line item, the same category as release signing
  above.

#### 6.8 Headless/daemon packaging deliverables

§6.1–6.7 cover Flatpak, Snap, deb/rpm, AUR, MSI, DMG — all GUI. The headless mode, which the owner
has decided ships **now** (see Project context), has no packaging deliverables specified beyond
"ship it via deb/rpm/AUR/Docker". Three concrete artifacts, all with working precedent:

1. **A systemd `--user` unit, hardened, plus a second system-level unit for headless deployment.**
   tidalt's actual template is minimal — no hardening directives at all:
   `[Unit] Description=... After=graphical-session.target PartOf=graphical-session.target` /
   `[Service] Type=simple ExecStart={{.Exec}} daemon Restart=on-failure RestartSec=5s` /
   `[Install] WantedBy=graphical-session.target`, installed by `tidalt setup --daemon`
   (ref:tidalt/cmd/tidalt/daemon.go:17-31). Copy the shape but add hardening streamboat's own daemon
   needs given it holds a live refresh token (§2.5, §3.1) and may open a local control port (§10.5):
   `NoNewPrivileges=true`, `ProtectSystem=strict` with `ReadWritePaths=` limited to the config/cache/
   state directories, `ProtectHome=read-only`, `PrivateTmp=true`,
   `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6`, `RestrictNamespaces=true`. Two conflicts to
   resolve explicitly: `MemoryDenyWriteExecute=` must stay **off** if the audio stack uses liborc's
   runtime JIT (the same JIT that forces `com.apple.security.cs.allow-jit` on macOS, §6.5); and a
   `--user` unit bound to `graphical-session.target` never starts on a headless box at all — for the
   headless/server target the owner has already committed to (Project context), ship a *second*,
   system-level unit with a dedicated service user, `StateDirectory=`/`CacheDirectory=`/
   `ConfigurationDirectory=streamboat`, and membership of the `audio` group (the same group tidalt's
   Docker guidance requires below). Decide `Type=notify` with a readiness signal vs `Type=simple`,
   since `Restart=on-failure` with a 5 s delay combined with a token-refresh storm is exactly the
   §2.3-test-2 rate-limit scenario. (For the daemon's own control-surface and CLI conventions —
   exit codes, `--json` output, MPRIS/HTTP protocol shape — see the `headless-and-tidal-connect`
   skill; this document covers only the OS-packaging side.)
2. **A container image.** `--device /dev/snd` plus
   `--group-add $(getent group audio | cut -d: -f3)` is the documented minimum for ALSA in Docker,
   with `/proc/asound` readable for device discovery and config/state mounted as volumes
   (ref:tidalt/docs/docker.md).
3. **A `.desktop` file registering `streamboat://` (not `tidal://`), plus the Windows registry and
   macOS `CFBundleURLTypes` equivalents** — this covers the OAuth-redirect and content-deep-link
   surface on all three OSes, not just Linux (see §2.6 item 5 for the full cross-platform
   registration mechanism and its threat model). **Correction, previously said to register
   `tidal://` here**: `tidal://` is claimed by the official TIDAL desktop app itself plus
   Strawberry, Sone, and High Tide, and OS handler registration is last-writer-wins — claiming it
   would steal it from whichever app the user installed last (see `tidal-api.md` §13/§14 for the
   full reasoning). Register `streamboat://` as streamboat's own scheme; still **parse** (not
   register) `tidal://` content links so pasted links from other apps work, and make **claiming**
   `tidal://` an explicit opt-in setting, off by default. Adapt tidalt's mechanics with the scheme
   name swapped: `Exec=streamboat play %u`, `MimeType=x-scheme-handler/streamboat;`,
   `Categories=Audio;Music;Player;`, `Terminal=false` (pattern from
   ref:tidalt/cmd/tidalt/tidalt.desktop, which uses `tidal://` since tidalt does not have this
   collision problem). Also set `X-PulseAudio-Properties=media.role=music`,
   which tidal-hifi's desktop entry sets. **Two more desktop-entry/bundle fields worth copying,
   both fixes for visible bugs rather than nice-to-haves**: `StartupWMClass` — must equal the app's
   actual WM class or the taskbar/dock icon never associates with the running window — and
   `StartupNotify=true`, both present in tidal-hifi's entry
   (ref:tidal-hifi/build/electron-builder.base.yml:40-63); and, for the macOS bundle,
   `LSApplicationCategoryType` (tidal-hifi uses `public.app-category.entertainment`), which the DMG/
   notarization path in §6.5 needs. See §2.6 item 5 for the security implication of registering the
   scheme.

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
| package install smoke test | Docker, per distro | installs the built `.deb`/`.rpm`, asserts window/daemon start, MPRIS/control-socket name appears, no missing `.so` (§6.3, sone's `build-scripts/test/`) — run on release-candidate tags, not every PR |

Plus scheduled jobs: daily API-spec diff (§2.4), weekly live canary run from a maintainer-local
cron — not a GitHub-hosted self-hosted runner on the public repo (§2.5) — weekly full-matrix
rebuild (mopidy-tidal runs its integration suite on `cron: "0 0 * * 0"`).

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
- **Gap: inventory CI secrets explicitly, and use GitHub Environments to scope them — this document
  otherwise names secrets one at a time across §2.5/§6.3/§6.5/§7.4 with no single list and no
  protection mechanism named.** The reference set's actual secret surface, enumerated by grepping
  every `.github/workflows/*.yml` in all 21 checkouts for `secrets\.[A-Z_]+`:
  `APPLE_DEVELOPER_ID_CERTIFICATE`/`_PASSWORD`, `APPLE_NOTARIZATION_APPLE_ID`/`_PASSWORD`,
  `MACOS_KEYCHAIN_PASSWORD` (strawberry); `UBUNTU_PPA_GPG_PRIVATE_KEY`,
  `GPG_SIGNING_KEY_ID`/`_PASSWORD`/`_IN_MEMORY_KEY`; `SNAPCRAFT_STORE_CREDENTIALS`,
  `AUR_SSH_PRIVATE_KEY` (tidal-hifi); `FLATHUB_TOKEN`; `DOCKERHUB_TOKEN`; `CODECOV_TOKEN`;
  `TIDAL_CLIENT_ID`; `PLAYER_REFRESH_TOKEN`/`PLAYER_TEST_USER` (tidal-sdk-web). Note
  `CLOUDSMITH_API_KEY`/`CLOUDSMITH_REPO` exist only as local shell env in sone's publish script
  (`ref:sone/build-scripts/publish-cloudsmith.sh:6-7`) and never reach CI — a maintainer runs that
  publish step by hand. The mechanism this document had not previously named: four checkouts gate a
  privileged job behind a named GitHub `environment:`
  (ref:tidal-cli/.github/workflows/release.yml:27, ref:tidal-sdk-android/.github/workflows/publish-pages.yml:11,
  ref:tidal-sdk-ios/.github/workflows/docs.yml:28,
  ref:tidal-sdk-web/.github/workflows/{cypress.yml,docs.yml}:27) — this is how a secret gets scoped
  to one job and, optionally, gated behind a required human reviewer before a release runs. Add to
  this section: a secrets inventory table (secret → job → holder → expiry) in `docs/DECISIONS.md`
  or `docs/releasing.md`, a required-reviewer GitHub Environment on every publishing job, and a
  rotation note — Apple Developer ID certificates expire yearly, and the Trusted Signing profile and
  the AUR SSH key are both single points of failure for a solo maintainer with no documented
  succession plan.
- **Gap: lint the packaging shell scripts and workflow YAML themselves — this is where release
  credentials actually run, and no reference project checks it.** §7.1's lint job covers
  application code; add `shellcheck` over `packaging/` + `build-scripts/` + `.githooks/` and
  `actionlint` over `.github/workflows/` as explicit jobs. tidal-sdk-ios's pre-commit config runs
  `check-jsonschema`'s `check-github-actions`/`check-github-workflows`
  (ref:tidal-sdk-ios/.pre-commit-config.yaml, also cited in §7.3), which validates workflow
  *schema* but not workflow *semantics* — unquoted shell inside a `run:` block, an invalid `needs:`
  graph, an expression typo — which is exactly what `actionlint` covers and no checkout runs. Same
  for shellcheck: zero of the 21 checkouts run it despite sone alone shipping nine build/test shell
  scripts with no linting configured (ref:sone/build-scripts/build/*.sh,
  ref:sone/build-scripts/test/*.sh).

#### 7.5 Reproducible builds

Full bit-for-bit reproducibility is a large project; the achievable subset:

- Set `SOURCE_DATE_EPOCH` from the tag's commit date in every packaging job.
- Commit and use lockfiles everywhere; build with `--frozen-lockfile` / `--locked`.
- Pin toolchain versions in-tree (`rust-toolchain.toml`, `.nvmrc`, `go.mod` `go` directive,
  `.python-version`) and have CI read them rather than hardcoding. **This is rarer in the reference
  set than the phrasing above implies — only 2 of 21 checkouts pin a toolchain in-tree at all**
  (`ref:tidal-hifi/.nvmrc`, `ref:tidal-sdk-web/.nvmrc`); no `rust-toolchain.toml` exists anywhere in
  the set, sone's `Cargo.toml` declares `edition = "2021"` with no `rust-version` MSRV field
  (ref:sone/src-tauri/Cargo.toml:7), tidalrs uses `edition = "2024"` (ref:tidalrs/Cargo.toml:5),
  tidalt pins `go 1.26.4` in `go.mod`, and tidal-cli declares
  `"engines": {"node": ">=20"}` (ref:tidal-cli/package.json:43-45).
- **Pinning the *build* toolchain (above) and declaring a *minimum supported* toolchain (MSRV) are
  two different decisions — make both explicitly and write them into `docs/DECISIONS.md`.** The
  build pin is what reproducible-build tooling reads; the MSRV/minimum-runtime version is what a
  Debian/Fedora packager checks before they can package streamboat at all — an MSRV newer than
  Debian stable's compiler means no Debian package, ever. Choose the MSRV against the oldest target
  distro's shipped toolchain, the same constraint that already drives §6.3's "build the deb on
  Ubuntu 22.04 for the oldest glibc", and add a dedicated "oldest supported toolchain" CI leg that
  verifies it — this also answers Open question 12 for the compiler, not just glibc/macOS/Windows.
- Build release artifacts inside a pinned container image (sone's Dockerfiles pin
  `ubuntu:22.04` and `pnpm@11.1.3`; tidalt pins `FFMPEG_VERSION=7.1.5`).
- Publish the exact build command and container digest in the release notes.
- Strip and normalise: `-ldflags="-s -w"` (Go), `strip = true` in the Rust release profile, and
  avoid embedding absolute build paths — including the generated Windows packaging fragments §6.3a
  flags (sone-windows's `.wxs`/`.nsi` currently embed a developer's home directory path).
- Verify by rebuilding one release from the tag on a clean machine and diffing hashes; document
  the result even when it is "not yet reproducible, differs in X".

#### 7.6 Repo governance mechanics

§7.4 says "enable branch protection with required checks"; §8.3 covers CONTRIBUTING and templates.
The mechanical pieces that make branch protection and code review actually work are cheap to get
right at repo creation and annoying to retrofit:

- **`CODEOWNERS`.** All three TIDAL SDKs ship `.github/CODEOWNERS`
  (ref:tidal-sdk-web/.github/CODEOWNERS, ref:tidal-sdk-ios/.github/CODEOWNERS). Add one from the
  start so `core` and the packaging directories have explicit reviewers as contributors arrive.
- **`concurrency:` groups with cancel-in-progress** on PR workflows, used by tidal-sdk-android
  (`pull-request.yml`, `post-merge.yml`, `publish-pages.yml`) and tidal-sdk-web (`cypress.yml`).
  Without one, a force-push queues a duplicate full matrix.
- **Name the required status checks explicitly.** State in `docs/DECISIONS.md` which of the §7.1
  jobs actually block merge — the credentialed and scheduled jobs (§2.4, §2.5) must **not** be in
  that list, per §2.9's own rule, or a fork PR will be unmergeable through no fault of the
  contributor.
- **State whether DCO sign-off is enforced by a bot** (e.g. the DCO GitHub App) — open question 3
  raises the CLA/DCO decision itself but not how it would be mechanically enforced once decided.

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
- **The in-use convention in this ecosystem is a vendor-neutral `.agents/` directory, not
  `.claude/skills/`.** tidal-sdk-ios ships `.agents/skills/prepare-release/SKILL.md` with
  `scripts/{bump-version,check-release-needed, check-version-bump,check-version-sync,
  extract-release-notes,suggest-changelog}.sh`, and **CI invokes the same scripts from the same
  paths** (`.agents/skills/prepare-release/scripts/check-version-sync.sh` in
  `changelog-check.yml`). tidal-sdk-android ships `.agents/{README.md,checks,do.md}` — one file per
  review rule with severity frontmatter, explicitly "additive" to existing linters and formatters
  ("Coexist, don't replace ... Don't re-litigate formatting."). tidal-cli ships a plain
  `skills/tidal-cli/SKILL.md`. **Precisely**: no checkout uses `.claude/skills/` specifically, but
  `.claude/` itself is not absent from the ecosystem — tidal-sdk-ios, an *official* TIDAL SDK, ships
  `.claude/commands/create-release-pr.md` alongside its `.agents/` directory, and five of the 21
  checkouts ship a root `CLAUDE.md` (strawberry, tidalt, tidalswift, tidal-cli, tidal-sdk-ios —
  two of them official SDKs); tidalswift also ships an `AGENTS.md`. So "nobody in this ecosystem
  uses `.claude/`" is not quite the finding — the finding is narrower: `.claude/skills/`
  specifically is unused, `.agents/skills/` is the pattern with working CI-invocation precedent, and
  a root `CLAUDE.md` is common enough (5/21, including official SDKs) not to read as unusual on its
  own. Adopt both real patterns regardless: skills that wrap real scripts CI also runs, and review
  rules that never re-litigate what the formatter owns. Whichever directory streamboat itself uses
  for its own agent-facing skills, a vendor-branded directory name is still the kind of detail a
  Flathub reviewer could read as an AI-tooling signal under the Generative AI policy (§6.1) — a
  neutral `.agents/` name carries less of that risk if the project ever wants to minimize it,
  independent of which coding-agent tool actually produced the files.
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
vulnerability reporting), a target acknowledge/fix timeline (see §5.5 — this is a separate clock
from the CRA reporting timeline, do not reuse those numbers here), and an explicit scope note that
token handling and the local control API are in scope, "TIDAL's own service" is not, and reports
about circumventing TIDAL's DRM are out of scope and will not be accepted.

**Branch strategy**: `main` protected, feature branches, squash merge, release tags on `main`. A
`develop` branch (tidal-hifi) only pays off with several contributors and a slow release train;
for a solo/small project, trunk plus tags is simpler and matches sone, tidalt and the SDKs.

#### 8.4 Developer environment

streamboat depends on GStreamer/ALSA/PipeWire/system audio libraries; "install these 30 packages"
is where contributors bounce, and CI reproducibility depends on the same answer — yet nothing in
§8.1–8.3 says how a contributor gets a working build. Five of the 21 checkouts ship a Nix flake
with a devShell: sone, high-tide, mopidy-tidal, python-tidal, TidaLuna. sone's is the instructive
one — its devShell adds nodejs/pnpm/cargo/cargo-tauri and its `shellHook` exports
`GST_PLUGIN_SYSTEM_PATH_1_0` composed from gstreamer plus plugins-base/good/bad/gst-libav, exactly
the class of setup that breaks on a bare distro (ref:sone/flake.nix). high-tide's flake pulls in
meson, ninja, blueprint-compiler, libadwaita, glib-networking, gst_all_1, libsecret, libportal and
alsa-utils (ref:high-tide/flake.nix). mopidy-tidal instead documents a two-command `uv` workflow
plus `make test`/`make format` (ref:mopidy-tidal/DEVELOPMENT.md). Ship `docs/development.md` plus
one reproducible dev shell (flake or container) plus a task runner. Two cheap, cited conventions
the repo layout in §8.1 omits: three references (python-tidal, tidal-sdk-ios, tidalt) use a
Makefile as the CI-mirroring entry point (ref:tidalt/Makefile), and three (tidal-hifi,
tidal-sdk-android, tidal-sdk-web) ship a `.editorconfig` (ref:tidal-hifi/.editorconfig).

---

### 9. Internationalisation and accessibility

#### 9.1 i18n

- **The reference bar is low and easy to beat, and there are two working precedents to copy, not
  one — an earlier draft of this section stated "only High Tide ships translations", which
  contradicted its own next sentence about Strawberry's Crowdin setup; corrected here.** High Tide
  ships gettext: `po/{de,es,fr,it,nl,pl,pt_BR,zh_TW}.po`, a `high-tide.pot`, `po/LINGUAS`,
  `po/POTFILES` and a documented regeneration command
  `xgettext --files-from=po/POTFILES --output=po/high-tide.pot --from-code=UTF-8 --add-comments
  --keyword=_ --keyword=C_:1c,2` (ref:high-tide/CONTRIBUTING.md, ref:high-tide/po/). Strawberry
  ships 31 Qt `.ts` catalogues (`ca_ES` through `zh_*`) synced through Crowdin
  (ref:strawberry/src/translations/, ref:strawberry/crowdin.yml) — a second, larger-scale precedent
  with a translation-platform workflow already in the checkout set. sone, tidal-hifi and tidalt are
  English-only (no `.po`/`.ts`/`strings.xml`/`.lproj`/`.ftl` anywhere in those trees, or in
  sone-windows, TidaLuna, tidalswift or tidalt). Use the High Tide/gettext pair and the
  Strawberry/Crowdin pair together to frame the Weblate-vs-Crowdin-vs-plain-PRs decision in Open
  question 11 — whichever mechanism the chosen stack uses, a working example of *both* the
  extraction tooling and the platform-sync workflow already exists in this reference set.
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
- **i18n mechanics beyond picking a library**: three mechanical things break in practice that
  §9.1's library choice does not cover. (1) The `.desktop` file and the AppStream metainfo are
  user-facing and separately translated. **Correction — High Tide is not a gap here, it is a
  complete working template to copy, not an example of a pattern that needs extending**:
  `ref:high-tide/po/POTFILES` lines 21-23 explicitly list
  `data/io.github.nokse22.high-tide.desktop.in`, `data/io.github.nokse22.high-tide.appdata.xml.in`
  and `data/io.github.nokse22.high-tide.gschema.xml`, and `ref:high-tide/data/meson.build` merges
  the translations back into the shipped files at build time via `i18n.merge_file(input:
  '...desktop.in', type: 'desktop', po_dir: '../po')` and the equivalent for the appdata XML. Copy
  this shape directly: list the `.desktop.in`/`.appdata.xml.in` sources in the translation catalog
  and merge with `i18n.merge_file` (or the gettext/`msgfmt --desktop` equivalent for a non-Meson
  build) rather than treating desktop/metainfo strings as a separate, easy-to-forget translation
  surface. (2) Add a pseudo-localization locale (wrap every translated string in
  markers, pad it ~40%) and do a manual pass on it — the only cheap way to find strings that were
  never externalized and layouts that break on German. (3) Define a string freeze before each
  release so translators have a stable target, which matters given the roughly weekly cadence
  §6.7 recommends; Flathub forbids machine-generated or mixed translations (§6.1), so an empty
  locale is better than an auto-filled one.

#### 9.2 Accessibility

- **Keyboard navigation is the highest-value, stack-independent commitment**: every action
  reachable without a mouse, a visible focus ring, a shortcuts dialog (High Tide ships
  `data/shortcuts-dialog.blp`), Escape closing overlays (sone has a dedicated test,
  `NowPlayingDrawer.escape.test.tsx`), user-remappable shortcuts (sone lists "Customizable in-app
  keyboard shortcuts" as a feature and tests `useShortcuts`), and media keys via MPRIS / SMTC /
  MediaPlayerRemoteCommandCenter. **Gap — on Wayland this understates what "media keys" means: MPRIS
  is not one option among several, it is the mechanism.** The compositor owns the hardware key;
  pressing play/pause translates into a D-Bus call to whichever MPRIS2 service is registered, so
  there is no toolkit-level global-hotkey grab to fall back to. tidalt documents this directly:
  `playerctl` against `org.mpris.MediaPlayer2.tidalt` is "the recommended way to control tidalt from
  keyboard shortcuts on Wayland" (ref:tidalt/docs/media-keys.md). Anything *beyond* the standard
  media keys — a user-defined global hotkey — requires `org.freedesktop.portal.GlobalShortcuts`, not
  a toolkit-level grab, which is also what keeps a Flatpak build inside Flathub's mandatory-portal
  rule (§6.1). Add acceptance criteria to the manual matrix below: media keys work unfocused on
  GNOME/Wayland, KDE/Wayland and X11 (Linux) via MPRIS, on Windows via SMTC (which per §6.4 also
  needs the AppUserModelID matched on the Start Menu shortcut), and on macOS via
  MPNowPlayingInfoCenter/MPRemoteCommandCenter.
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
  - **Windows and macOS are first-class targets from v1 (see Project context) and need their own
    bar, not just a Linux one.** Windows exposes UI Automation (UIA) — test with NVDA, not only
    named in passing as the webview screen reader above. macOS uses NSAccessibility/AXAPI — test
    with VoiceOver, and a custom-drawn or webview UI needs explicit accessibility roles on *both*
    platforms, not just Linux's AT-SPI bridge. Add a per-OS row to the manual test matrix
    (`docs/testing/audio-matrix.md` already proposed below) and state the OS media-key/now-playing
    integration a screen-reader user relies on for each platform (SMTC on Windows,
    MPNowPlayingInfoCenter on macOS, MPRIS on Linux) with acceptance criteria, not just a mention.
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
machine, which keeps it consistent with the no-telemetry stance. Define the numbers once — §10.5
below proposes the same buffer-underrun and cache-hit-ratio metrics as *performance budgets*; do
not define them twice.

#### 10.5 Baseline security posture for the local control surface

Open question 9 defers the headless control-surface design (MPRIS vs HTTP/JSON) entirely to the
owner, but whichever way that lands, a local listening socket has an established safe default, and
the reference set already has a working precedent to copy rather than designing from the abstract.
sone ships an opt-in local HTTP control surface (an MCP server) whose design is the baseline:
disabled by default (`if !settings.mcp_enabled { return }`); bound explicitly to loopback
(`let addr: SocketAddr = ([127,0,0,1], port).into()`), never `0.0.0.0`; authenticated by a random
UUIDv4 bearer token generated on first enable and persisted in the (encrypted) settings, carried in
the URL path (`/{token}/mcp`); a mutex held across the whole check→bind→store sequence so two
callers cannot race the port; and a dedicated `sanitizer.rs` that projects the internal TIDAL
models onto reduced structs (`SanitizedTrack`/`Album`/`Artist`/`Playlist` — id, title, artist,
duration only) so the surface cannot leak account fields
(ref:sone/src-tauri/src/mcp/mod.rs:12-42, ref:sone/src-tauri/src/mcp/server.rs:40,72-85,
ref:sone/src-tauri/src/mcp/sanitizer.rs). **One thing to do differently**: a token in the URL path
lands in access logs by default — put it in a header instead, and add it to the §10.2 redaction
list regardless of which control-surface shape is chosen.

#### 10.6 Performance budgets

For a music player the user-visible quality bar is largely non-functional: how long until audio
starts, whether a 10,000-track library scrolls, how much RAM it holds overnight. No reference
project defines these, and this document specifies correctness testing in depth but nothing to
regress performance against. Define a small set of budgets in `docs/testing/` at v0.1, measured
with the same golden fixtures the audio tests (§2.7) use: cold start to first frame; time from
play-press to first audio sample (the number users compare against the official client, dominated
by the playbackinfo round-trip plus first-segment fetch — both already instrumented by the
structured logging in §10.1); seek latency; steady-state RSS after an hour of playback; frame time
scrolling a 10k-item list. Run them as a reporting (non-gating) CI job once the stack exists, since
runner variance makes gating unreliable.

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
    with a version-sync check whose load-bearing line is
    `grep -q "$(cat ./version.txt)" ./CHANGELOG.md` (tidal-sdk-ios's actual script is nine lines,
    not four).
16. **Externalise every user-facing string from the first screen**, and pass the user's real
    locale to TIDAL's `locale` parameter rather than hardcoding `en_US` as every reference does.
17. **Put keyboard navigation, focus visibility and a shortcuts dialog in the definition of done**
    for every screen, with one keyboard test per screen in the default suite.
18. **Build `streamboat debug-bundle` early** — it pays for itself the first time a DAC bug is
    reported.
19. **Decide the multi-process token/device-ownership model now** (§3.7): either the daemon is
    always the token owner with the GUI as a client, or the store needs a cross-process lock. Add
    a test for it before the desktop and headless binaries can both be running at once.
20. **Fuzz the `tidal://` URI handler** (§2.6 item 5) alongside the manifest/playbackinfo targets —
    it is the one input surface reachable from an arbitrary web page before authentication.
21. **Define a coverage target for `core` and publish it** (§2.10); state a fixture/golden-file
    size policy and an LFS decision before the first large fixture lands (§2.11).
22. **Ship one reproducible dev environment on day one** (§8.4: a Nix flake or container, plus a
    Makefile that mirrors CI) — this is what determines whether a first-time contributor gets a
    working build in five minutes or gives up.
23. **Budget for release signing/SBOM and for a portable Linux binary as no-precedent, first-week
    line items**, not later add-ons: no reference project attests a Linux binary (§6.7), and none
    ships an AppImage-equivalent for the *streamboat* stack yet either (§6.3).
24. **Record the EU CRA scope determination in `docs/legal.md`** (§5.5) — a five-minute decision
    today, and a compliance-timeline problem if left implicit past 2026-09-11.
25. **Write every persisted secret/settings/state file atomically** (temp file in the same
    directory → write → fsync → rename → fsync the directory) from the first commit (§3.3) — sone
    itself only does this for one file class (themes) and not for settings/tokens, and retrofitting
    it after real users have on-disk state is much more expensive than starting with it.
26. **Implement `logout` in the sone ordering** (stop reporting before stopping playback, then
    playback teardown, control-server shutdown, token clear, settings re-save, cache clear) **and
    ship a separate `streamboat purge`** that also removes the keyring entry, key file and logs
    (§3.8) — decide this alongside the multi-process token model in item 19, since both concern the
    same on-disk state.
27. **Decide the app ID / GitHub owner (personal vs org) before the first commit, not after** (§6.1a)
    — it is baked into the Flatpak ID, D-Bus name, macOS bundle identifier, and (once introduced)
    the Windows AppUserModelID, and moving it later costs a Flathub `end-of-life-rebase` plus a
    user-data migration.
28. **Budget the Windows/macOS media-runtime bundling problem as a first-week Windows/macOS line
    item, not a late add-on** (§6.3a): neither OS ships GStreamer/FFmpeg, so the installer must
    bundle the runtime and the app needs a bundled-vs-system plugin-path code path — this is the
    single largest Windows-specific fact the reference set contains (`sone-windows`) and it is easy
    to discover only after the first Windows build already "works" on the developer's own machine.
29. **Add a package-install smoke test to CI on release-candidate tags** (§7.1, §6.3): copy sone's
    Docker-per-distro harness (installs the built package, asserts window/daemon start, MPRIS/
    control-socket name on the bus, no missing shared libraries) rather than relying on a build-only
    job to catch a missing runtime dependency.

### Sequence the packaging work

1. GitHub Releases with `checksums.txt` and a tag-triggered build (week 1 of releasing).
2. **AppImage (or a static tarball for the daemon/CLI)** — the only day-one option that reaches a
   user on a distro not covered by AUR or the Cloudsmith deb/rpm matrix; see §6.3.
3. AUR `-bin` + AUR source package; deb/rpm built in pinned containers, published to Cloudsmith.
4. Flatpak manifest built in CI from the start; **submit to Flathub only after several
   months of tagged releases and real users**, per the development-history requirement, and
   re-read the Generative AI policy immediately before submitting (§6.1).
5. Snap once the ALSA plug support burden is understood.
6. Windows MSI/NSIS unsigned → winget once signing is sorted, and once the silent-install switches
   and install `Scope` are decided (§6.4).
7. macOS DMG only after the $99 Apple Developer Program is in place; Homebrew cask only after
   notarization and after clearing the disjunctive 30-or-30-or-75 (90-or-90-or-225 self-submission)
   notability bar.

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
2. **License split.** GPL-3.0-only everywhere, or Apache-2.0 `core` + GPL-3.0-only apps? The second
   is recommended and is effectively irreversible without contributor consent — **and it is now also
   the mobile decision**: GPL-3.0-only on the app is incompatible in practice with Apple App Store
   distribution, so an Apache-2.0 `core` is what keeps a future iOS app possible at all (§5.2).
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
   security model and the AGPL question. If an HTTP surface is chosen, sone's loopback-bound,
   token-authenticated, field-sanitized MCP server (§10.5) is the baseline design to copy or
   deviate from explicitly, not a blank page.
10. **Release cadence.** Weekly-ish minors like sone, or slower and batched? It affects Flathub
    eligibility (no daily-update software in stable) and the changelog burden.
11. **Translation platform.** Self-hosted Weblate, Crowdin (Strawberry's choice), or plain PRs
    against `po/`?
12. **Minimum supported platform versions.** Oldest glibc/Ubuntu for the deb, oldest macOS SDK,
    oldest Windows. These constrain runner and container choices immediately.
13. **Gap — forge choice.** Nearly every mechanism this document recommends — CI, secret scanning,
    private vulnerability reporting, `io.github.*` Flathub verification (§6.1a), provenance
    attestation, the AUR/winget/Flathub automation, the GitHub-Environments secret scoping in §7.4
    — assumes GitHub. Given the project's own stated posture (§5.4 independence, §4.4 no
    telemetry), whether to host on GitHub vs. Codeberg/self-hosted is a real decision with real
    tooling consequences, not a default to fall into. §6.1a already makes the GitHub *owner*
    permanent through the app ID; settle the *platform* at the same time, as one ADR, not two.

Unverified items that must be checked before anything is published:

- **TIDAL's Terms of Service and Developer Terms could not be read** from this environment
  (`tidal.com` and `developer.tidal.com` are blocked by the egress proxy). A search result
  attributes to Developer Terms 2.0 the claim that the SDK's Player module "constitutes the only
  allowed way for third-party applications to incorporate playback of TIDAL content" — treat that
  as unconfirmed until read directly. The ToS was reported as effective 2026-06-29.
**Two items previously listed here are now resolved, not unverified** — both are one grep away in
the checkout and are recorded with their exact source in §2.3 and §2.7 respectively: the terminal
playback sub-status list is `[4005, 4010, 4030, 4031, 4032, 4034, 4035]` (non-contiguous — 4033 and
4006 are deliberately excluded as recoverable, ref:sone/src-tauri/src/tidal_api.rs:16-18), and
**correction: the ReplayGain formula that was verbatim from source is Sone's own variant**,
`0.8 * min(10^((replay_gain + 4) / 20), 1 / peak)`
(ref:sone/src-tauri/src/commands/playback.rs:9-20) — TIDAL's own SDK formula has no `0.8` factor
(see §2.7's correction). Both are corrected in-place in this document; they no longer belong on
this unverified list.
- **Windows Credential Manager blob limit**: `CRED_MAX_CREDENTIAL_BLOB_SIZE` is documented as
  `5*512` = 2560 bytes on Windows 7+, but reports differ on whether `CRED_TYPE_GENERIC` is further
  limited to 512 bytes. Measure with a real token before designing around it.
- **The set of locales TIDAL accepts** for the `locale` parameter is not documented anywhere in the
  reference checkouts; every project hardcodes `en_US`. Determine empirically.
- **Qt 6 module-by-module licensing** (which modules are GPL-2.0-only vs GPL-3.0-only) needs a
  per-module check against https://doc.qt.io/qt-6/licensing.html if Qt is chosen.
- **Flathub's Generative AI policy text quoted here is from the repository HEAD dated 2026-09-07**;
  the policy already flipped once (commit `992f57b`, 2026-05-29, briefly an outright ban) before
  reverting to the disclosure regime quoted in §6.1. Re-read the live page immediately before
  submitting, not just once during research.
- **Azure Trusted Signing / Artifact Signing pricing and eligibility** ($9.99/$99.99 per month,
  signature caps, and the eligibility split — organisations across a 12-country list, individual
  developers restricted to the US/Canada, no free/trial/sponsored subscriptions, individual
  onboarding reportedly paused — come from a search index over Microsoft's pricing/FAQ pages, not a
  direct fetch, since both `azure.microsoft.com` and `learn.microsoft.com` are blocked from this
  environment. Re-verify before it drives a cost decision (§6.4, §6.5), but do not reintroduce the
  "individual eligibility is not narrower than organisation eligibility" framing — that framing was
  itself wrong and has been reverted (Summary, §6.4).
- **winget's "7-day timer before the bot closes an assigned PR"** could not be confirmed — the
  primary `learn.microsoft.com` policy page is blocked from this environment, and the only
  auto-close mechanism found in reachable sources is tied to the `Needs-Author-Feedback` label.
  Drop or hedge this detail until read from the primary source (§6.4).
- **Resolved, no longer unverified — the Flathub linter's "never granted" exception rule for
  `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID`.** A direct fetch of
  `flathub-infra/flatpak-builder-lint`'s `flatpak_builder_lint/checks/finish_args.py` (not
  `docs.flathub.org`, which remains blocked) confirms rule id
  `finish-args-mpris-flatpak-id-talk-name`, and the live `staticfiles/exceptions.json` in that repo
  contains zero entries for it (§6.1). The companion `finish-args-incorrect-secret-service-talk-name`
  rule for the miscased `org.freedesktop.Secrets` name is confirmed the same way.
- **Whether Snap's `alsa` interface can be requested for auto-connection** for a media player, and
  what the store review process requires, was not checked against `snapcraft.io/forum` (§6.2).
- **GStreamer, Qt and fdk-aac licensing claims in §5.3** are correct in substance but several of
  the specific source URLs originally cited in this document did not actually contain the quoted
  text on direct inspection; §5.3 now attributes each quote to its actual source file/page. Where
  the corrected source is itself unreachable from this environment (Qt, fdk-aac), the claim is
  read via a search index rather than a primary fetch — re-verify before publishing if Qt or a
  patent-encumbered codec is in scope.

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
- `ref:mopidy-tidal/.github/workflows/test.yml` — Python-only unit-test matrix (3.12/3.13/3.14),
  `fail-fast: false`, uv cache keyed per leg, codecov upload gated on the 3.13 leg.
- `ref:mopidy-tidal/.github/workflows/integration.yml` — the separate 3.12/3.13/3.14 × mopidy 3.3/3.4
  matrix (6 legs), `fail-fast: false`, no coverage upload, runs on push/PR **and**
  `cron: "0 0 * * 0"` (weekly), executing the pexpect integration suite.
- `ref:mopidy-tidal/mopidy_tidal/ext.conf` — `playback_cache=false`,
  `playback_cache_max_entries=1024`, `playback_cache_buffer_bytes=16777216`.
- `ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/cache.py` — SQLite cache, LRU eviction by
  `last_used`, `manual` pin flag, `schema_version` in a metadata table.
- `ref:sone/src-tauri/src/crypto.rs` — AES-256-GCM envelope, `"SONE"|version|nonce` header, keyring
  (`sone`/`master-key`, lines 79-110 return immediately on success — no file write on that path)
  then a 0600 key file written only at first-run key generation (lines 113-148), zeroize (line 28),
  plaintext-passthrough migration, cache-entry encryption reusing the same envelope (lines 429-433).
- `ref:sone/src-tauri/src/cache.rs` — 4-tier TTL/SWR table, 2 GiB cap, 90% evict target, SHA-256
  keys, `CacheResult::{Fresh,Stale,Miss}`, `CacheStats`.
- `ref:sone/src-tauri/src/logging.rs` — flexi_logger 5 MB rotation, 9 kept files, stderr
  duplication, pre-start plaintext `logging.toggle` sidecar and its unit tests.
- `ref:sone/src-tauri/src/rate_gate.rs` — 429 cooldown, 5 s default, 120 s clamp, `fetch_max`
  absolute deadline, HTTP-date `Retry-After` rejected rather than mis-parsed.
- `ref:sone/src-tauri/src/commands/updates.rs` — GitHub-releases update check, semver comparison,
  failures treated as "no update".
- `ref:sone/src-tauri/src/tidal_api.rs` — pure `parse_*` functions with inline `json!` fixtures
  (51 `#[test]` functions in this file, ~145 across all Rust sources); `("locale", "en_US")` at 21
  call sites (lines 1808, 2307, 3074, 3258, 3336, 3381, 3429, 3489, 3528, 3576, 3874, 3957, 4173,
  4240, 5048, 5185, 5211, 5235, 5273, 5458, 5858); one `<redacted: account endpoint>` log line at
  line 1473.
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
- `ref:tidal-sdk-ios/.github/workflows/check-tidalapi-spec.yml`,
  `ref:tidal-sdk-android/.github/workflows/check-tidalapi-spec.yml`,
  `ref:tidal-sdk-web/.github/workflows/check-tidalapi-spec.yml` — all three (not two) run the
  identical daily OAS diff against
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
  query construction (lines 76, 30).
- `ref:tidalt/docs/client-server.md`, `ref:tidalt/cmd/tidalt/daemon.go` (lines 17-31, 44) — single-
  server D-Bus-name-claim model, ALSA `hw:` single-owner rationale, systemd `--user` unit template.
- `ref:tidalt/docs/docker.md` — container ALSA minimum (`--device /dev/snd`, `audio` group add).
- `ref:tidalt/docs/media-keys.md` — `playerctl`/MPRIS as the documented Wayland media-key mechanism
  (§9.2).
- `ref:high-tide/data/meson.build:49-77` — `desktop-file-validate`, `appstream-util validate` and
  `glib-compile-schemas --strict --dry-run` wired into `meson test`; `i18n.merge_file` merging
  translated `.desktop.in`/`.appdata.xml.in` sources at build time (§6.1a, §9.1).
- `ref:sone/scripts/gen_credentials.py` — second credential-generation script that, like
  `gen_embedded.py`, emits only the A/B pair and omits the C/D PKCE constants (§3.4).
- `ref:tidalt/cmd/tidalt/tidalt.desktop` — `x-scheme-handler/tidal` registration,
  `Exec=tidalt play %u`.
- `ref:sone/src-tauri/src/lib.rs` (lines 546-548) — `tauri_plugin_single_instance` focuses the
  existing window instead of opening a second one; (lines 346-348) settings-encryption migration
  log lines.
- `ref:sone/src-tauri/src/mcp/{mod.rs,server.rs,sanitizer.rs}` — opt-in loopback-bound,
  UUIDv4-token-authenticated local MCP control surface with a field-sanitizing response layer.
- `ref:sone/flake.nix`, `ref:high-tide/flake.nix` — Nix devShells with GStreamer/GTK dependency
  sets exported via `shellHook`.
- `ref:mopidy-tidal/DEVELOPMENT.md` — two-command `uv` dev workflow, `make test`/`make format`.
- `ref:tidalt/Makefile` — CI-mirroring Makefile entry point.
- `ref:tidal-hifi/.editorconfig` — `.editorconfig` convention.
- `ref:tidal-sdk-web/.github/CODEOWNERS`, `ref:tidal-sdk-ios/.github/CODEOWNERS` — CODEOWNERS
  convention.
- `ref:tidal-sdk-android/.github/workflows/{pull-request.yml,post-merge.yml,publish-pages.yml}`,
  `ref:tidal-sdk-web/.github/workflows/cypress.yml` — `concurrency:` groups with
  cancel-in-progress.
- `ref:tidalswift/.gitattributes`, `ref:tidalswift/README.assets/` — the only Git LFS usage in the
  reference set, for README screenshots only (§2.11).
- `ref:sone-windows/src-tauri/gstreamer-hooks.nsi`, `ref:sone-windows/src-tauri/gstreamer-fragment.wxs`,
  `ref:sone-windows/scripts/prepare-gstreamer.js` — generated NSIS/WiX fragments that bundle the
  GStreamer runtime into the Windows installer, including an embedded developer-local path (§6.3a).
- `ref:strawberry/cmake/Dmg.cmake`, `ref:strawberry/src/engine/gststartup.cpp` — macOS `ntool`
  deploy tool and bundle-relative GStreamer plugin/GIO-module path rewriting (§6.3a).
- `ref:sone/build-scripts/test/{all,common,deb,rpm,pacman}.sh` — Docker-per-distro package install
  smoke tests: `dpkg -i ... && apt-get install -f`, window/MPRIS/GStreamer/config-dir assertions,
  and an AppImage code path (§6.3, §7.1).
- `ref:sone/.github/FUNDING.yml`, `ref:high-tide/.github/FUNDING.yml`,
  `ref:tidal-hifi/.github/FUNDING.yml`, `ref:TidaLuna/.github/FUNDING.yml`,
  `ref:tidalswift/.github/FUNDING.yml` — `FUNDING.yml` precedent (§6.3).
- `ref:sone/src-tauri/src/theme_config.rs:142-175` — the one atomic (temp-file + fsync + rename)
  write in sone, contrasted with the non-atomic settings-file write at
  `ref:sone/src-tauri/src/lib.rs:474-477` (§3.3).
- `ref:sone/src-tauri/src/scrobble/queue.rs:69`, `ref:sone/src-tauri/src/tidal_report/queue.rs:56`
  — sone's own `.bin.tmp` atomic-write pattern for queue files (§3.3).
- `ref:sone/src-tauri/src/commands/auth.rs:413-462` — the `logout` command's ordering: disconnect
  scrobbling before stopping playback, then playback teardown, control-server shutdown, token
  clearing (preserving user-supplied client credentials), settings re-save, cache clear (§3.8).
- `ref:high-tide/src/lib/secret_storage.py:85-94`, `ref:high-tide/src/window.py:267-277` —
  `Secret.password_clear_sync` as the keyring-deletion counterpart to logout (§3.8).
- `ref:python-tidal/HISTORY.rst:88`, `ref:python-tidal/tests/test_media.py:236` — MQA and Sony
  360 Reality Audio discontinued by TIDAL 2024-07-24; a skipped test documents the fallback (§5.4).
- `ref:sone/README.md:58` — still advertises MQA support, a stale claim not to copy verbatim (§5.4).
- `ref:tidal-hifi/.nvmrc`, `ref:tidal-sdk-web/.nvmrc`, `ref:sone/src-tauri/Cargo.toml:7`,
  `ref:tidalrs/Cargo.toml:5`, `ref:tidalt/go.mod:3`, `ref:tidal-cli/package.json:43-45` — the
  toolchain-pinning precedent (2 of 21) and MSRV/edition/engine declarations surveyed (§7.5).

### Upstream documentation

- https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/02-requirements.md
  — inclusion policy (console software rejected, minimal submissions rejected, insufficient
  development history), app ID rules, license and license-file requirements, permissions/portals
  policy, no network during build, build-from-source, stable-releases-only, localisation policy,
  trademark rules, and the Generative AI policy. Read directly from the source repository
  `flathub-infra/documentation` (HEAD dated 2026-09-07) after `docs.flathub.org` returned
  EGRESS_BLOCKED from this environment.
- https://docs.flathub.org/docs/for-app-authors/linter — finish-args rules including
  `finish-args-incorrect-secret-service-talk-name`, `finish-args-x11-without-ipc`,
  `metainfo-missing-screenshots`, `module-*-build-network-access`. **This page itself remains
  egress-blocked**; its content here comes from a search index of the page, not a direct fetch. The
  own-name rule ids (`finish-args-unnecessary-appid-own-name`,
  `finish-args-unnecessary-appid-mpris-own-name`), the MPRIS talk-name rule
  (`finish-args-mpris-flatpak-id-talk-name`, now confirmed rather than unverified — §6.1) and the
  Secrets-casing rule (`finish-args-incorrect-secret-service-talk-name`) were all instead confirmed
  by direct fetch of `flatpak_builder_lint/checks/finish_args.py` and `staticfiles/exceptions.json`
  from `flathub-infra/flatpak-builder-lint`, not from this page. Still unconfirmed via this route:
  sandbox-escape exceptions (home/host/flatpak-spawn/arbitrary bus names) reportedly not granted
  when there are "signs of LLM usage in the software or in the exception PR" — corroborated only by
  a search snippet.
- https://raw.githubusercontent.com/flathub-infra/flatpak-builder-lint/master/flatpak_builder_lint/checks/finish_args.py
  and `.../staticfiles/exceptions.json` — read directly; source for every Flathub linter rule id
  cited in §6.1, including the "insecure design" style checks and the confirmed zero-exceptions
  count for both the MPRIS and Secrets talk-name rules.
- https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/03-metainfo-guidelines/index.md
  and `.../01-quality-guidelines.md` — mandatory/recommended metainfo fields, name/summary/
  screenshot/icon quality numbers (§6.1a).
- https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/05-submission.md
  — fork/branch/PR mechanics, `bot, build`, write-access invitation window (§6.1a).
- https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/06-maintenance.md
  — External Data Checker, `x-checker-data`, automerge levels, master/beta branches, EOL rules
  (§6.1a).
- https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/10-verification.md
  — GitHub-owner verification for `io.github.*` IDs; domain/DNS/GitLab alternatives (§6.1a).
- https://github.com/flathub-infra/documentation/commit/992f57b30de98ddbd5e80959e9672998c83c8c97 —
  the 2026-05-29 commit that briefly replaced the Generative AI policy with an outright ban before
  it reverted to the disclosure regime; evidence the policy is not settled (§6.1).
- https://github.com/flatpak/flatpak-docs/blob/master/docs/sandbox-permissions.rst —
  `--socket=pulseaudio` includes `/dev/snd`; device table (`dri`, `kvm`, `shm`, `input`, `usb`,
  `all`); `--persist=DIR` semantics; default D-Bus policy allowing an app to own
  `org.mpris.MediaPlayer2.$FLATPAK_ID` with no extra finish-arg (verbatim, verified by direct
  fetch).
- https://raw.githubusercontent.com/flatpak/xdg-desktop-portal/main/data/org.freedesktop.portal.Secret.xml
  — per-application master secret over a pipe FD, stored in the user's keyring under the app ID.
  The `xdg-desktop-portal >= 1.5.0` version is corroborated by secondary sources (ArchWiki, Snap
  docs), not read from a primary changelog.
- https://github.com/Homebrew/brew/blob/main/docs/Package-Acceptance-Policy.md — notability
  thresholds are **disjunctive**: "at least 30 forks, 30 watchers or 75 stars" (90/90/225 for a
  self-submission by the repo owner); "a code repository less than 30 days old is normally not
  eligible". An earlier draft of this document read the slash notation as a conjunction; corrected
  throughout (Summary, §6.5, packaging sequence).
- https://github.com/Homebrew/brew/blob/main/docs/Acceptable-Casks.md — Gatekeeper assessment
  requirement; no SIP/Gatekeeper bypass.
- https://github.com/Homebrew/brew/blob/main/docs/Homebrew-Security-and-Supply-Chain.md —
  quarantine application, Developer ID + notarization rationale, deprecation of casks failing
  Gatekeeper, and the "download cooldown on riskier ecosystems" clause (§6.5).
- https://developer.apple.com/programs/ and https://developer.apple.com/support/developer-id —
  Developer ID certificate requires Apple Developer Program membership ($99/year); neither page
  lists a separate notarization fee (an absence-of-evidence inference, not an explicit quote).
- Azure Trusted Signing / Artifact Signing pricing and eligibility: **`azure.microsoft.com` and
  `learn.microsoft.com` returned EGRESS_BLOCKED from this environment.** The $9.99/$99.99-per-month
  figures, signature caps (5,000 / 100,000), the paid-subscription requirement, and the eligibility
  split — organisations in the US, Canada, EU, UK, Australia, New Zealand, Japan, South Korea,
  Singapore, Switzerland, Norway and Israel; individual developers restricted to the US or Canada,
  with individual onboarding reportedly paused — come from a search index over
  azure.microsoft.com/en-us/pricing/details/artifact-signing/ and
  learn.microsoft.com/en-us/azure/artifact-signing/faq, not a direct fetch. Re-verify against the
  primary Microsoft pages before this drives a cost decision (§6.4, §6.5); do not re-derive the
  earlier, now-reverted "not narrower for individuals" reading from these same secondary sources.
- https://learn.microsoft.com/en-us/windows/package-manager/package/repository — this page returned
  EGRESS_BLOCKED. `InstallerSha256`, the automated validation pipeline, and unattended-install
  requirements are corroborated instead by the `microsoft/winget-pkgs` README/`doc/README.md` and
  PR threads; the "7-day PR timer" claim is **not** corroborated by any reachable source and should
  be dropped or hedged until read from this primary page (§6.4).
- https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentiala — this page
  returned EGRESS_BLOCKED; `CRED_MAX_CREDENTIAL_BLOB_SIZE` is instead verified against
  `MicrosoftDocs/sdk-api`'s `ns-wincred-credentiala.md` (the primary source that page mirrors, read
  directly by raw fetch — a corrected attribution: an earlier draft of this document cited a
  mingw-w64 header as the *verification* source, which is wrong — mingw-w64's header actually
  *disagrees* with Microsoft's figure, see §3.2) and AdysTech/CredentialManager issue #65;
  https://github.com/jaraco/keyring/issues/355 — observed failures past the limit (§3.2).
- https://raw.githubusercontent.com/mingw-w64/mingw-w64/master/mingw-w64-headers/include/wincred.h
  — read directly: defines `CRED_MAX_CREDENTIAL_BLOB_SIZE` as a bare `512`, not `5*512`, with no
  `WINVER` guard — a toolchain-specific discrepancy from the Microsoft figure above, not a source
  that corroborates it (§3.2).
- https://raw.githubusercontent.com/GStreamer/gst-docs/master/markdown/frequently-asked-questions/licensing.md
  — LGPL-2.1 core ("We require that all code going into our core packages is LGPL"), patent plugins
  routed to `gst-plugins-ugly`. **Downloaded and grepped directly**: this file contains *neither*
  "practical reasons under the GPL" *nor* any FFmpeg/libav guidance, despite both being attributed
  to it in an earlier draft of this document. **Both remaining quotes are from one file, not two,
  and not `gst-plugins-base`'s `LICENSE_readme` (no such file exists at HEAD in the GStreamer
  monorepo — a second earlier-draft misattribution, now corrected):**
  https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-libav/README.md, lines
  15-20 — "if you are distributing an application which has a non-GPL compatible license … you have
  to make sure not to build FFmpeg with GPL code enabled" and "Overall, when using plugins that link
  to GPL libraries, GStreamer is for all practical reasons under the GPL itself" — both read via
  GitHub raw fetch, corroborated by direct fetch of `subprojects/gstreamer/COPYING` and
  `subprojects/gst-plugins-base/COPYING` (both LGPL-2.1) (§5.3).
- FDK-AAC license (GPL-incompatible, non-free in Debian): `fedoraproject.org` returned
  EGRESS_BLOCKED; corroborated via a search index of the Fedora Licensing/FDK-AAC wiki, plus
  tookmund.com "AAC and Debian" and the Hydrogenaudio knowledge base.
- Qt 6 module-by-module licensing (LGPLv3 core, GPL-2.0-only vs GPL-3.0-only modules must not be
  mixed, with the Spatial Audio/TextToSpeech example): `doc.qt.io` returned EGRESS_BLOCKED;
  corroborated via a search index of https://doc.qt.io/qt-6/licensing.html and
  https://www.qt.io/faq/qt-open-source-licensing, not a primary fetch. Re-verify module-by-module
  before Qt is chosen.
- https://semver.org/spec/v2.0.0.html, https://keepachangelog.com/en/1.1.0/,
  https://www.conventionalcommits.org/en/v1.0.0/ — versioning, changelog and commit conventions.
- https://github.com/dirs-dev/directories-rs — the canonical per-OS mapping for config/cache/data/
  state/runtime directories used in §4.1; verified by direct fetch of its README.
- https://tidal-music.github.io/tidal-api-reference/tidal-api-oas.json — the OpenAPI document all
  three official SDKs (not two) diff daily.
- https://github.com/tauri-apps/tauri/issues/207 (accessibility tracking) and
  https://github.com/tauri-apps/tauri/issues/4315 ("Could not determine the accessibility bus
  address") — the webview-stack accessibility risk noted in §9.2.
- https://openssf.org/public-policy/eu-cyber-resilience-act/ and https://orcwg.org/cra/ — CRA
  timeline (2026-06-11, 2026-09-11, 2027-12-11), open-source-steward duties under Article 24, and
  the Article 64(10) fine exemption, used in §5.5.
- `developer.tidal.com/documentation/guidelines-developer-terms-2_0` — both `developer.tidal.com`
  and `tidal.com` returned EGRESS_BLOCKED. The "Player module only" playback restriction (§5.4) is
  corroborated by an independent web search returning matching quoted text from that URL, not a
  direct fetch — still not a primary read, and the document governs the official developer-program
  SDK, not the unofficial-API route this project uses (§5.4 scope note).

### Not reachable from this environment (flagged as unverified)

- `https://tidal.com/terms` and `https://developer.tidal.com/documentation/guidelines-developer-terms-2_0`
  — blocked by the egress proxy. All ToS/Developer-Terms claims in this document are second-hand
  (independent web search corroborates the quoted text, §5.4) and must be confirmed by the
  legal/ToS research topic before publication.
- `https://docs.flathub.org` and `https://docs.flatpak.org` direct fetches were blocked; most
  content was read instead from their upstream source repositories
  (`flathub-infra/documentation`, `flatpak/flatpak-docs`), which is equivalent or newer. The
  linter page specifically (`docs.flathub.org/docs/for-app-authors/linter`) was read only via a
  search index, not a repository mirror — its `finish-args-mpris-flatpak-id-talk-name` rule name
  and the "LLM usage" sandbox-escape caveat are correspondingly less certain (§6.1).
- `https://azure.microsoft.com/*` and `https://learn.microsoft.com/*` — both blocked by the egress
  proxy for every page cited in this document (Trusted Signing pricing/eligibility, winget
  submission policy, `wincred.h`/`CREDENTIALA`). All claims from these domains are secondary-source
  or search-index reconstructions and must be re-verified from an unblocked network before they
  drive a cost or design decision (§3.2, §6.4, §6.5).
- `https://doc.qt.io/*`, `https://fedoraproject.org/wiki/*`, and
  `https://gstreamer.freedesktop.org/*` — all blocked. The Qt and fdk-aac licensing claims (§5.3)
  are read via search indexes; the GStreamer FAQ was instead read successfully via its
  `raw.githubusercontent.com` mirror, which is how the mis-citation in an earlier draft of §5.3
  was caught and corrected.
