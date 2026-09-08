# Testing strategy for an unofficial-API client

Full source: `docs/research/engineering-baseline.md` §2 (fact-checked). This file is that section
verbatim, corrected. `ref:<project>/<path>` points at a shallow clone — see `sources.md`.

## Contents

1. The architectural precondition (parser/transport split)
2. Layer 1 — pure parser tests with captured fixtures
3. Layer 2 — transport-level tests (in-process server or stubbed transport)
4. Layer 3 — contract tests against the published spec
5. Layer 4 — opt-in live tests ("canary")
6. Fuzzing manifest and response parsing
7. Audio pipeline tests
8. UI tests **[STACK]**
9. The non-negotiable CI rule
10. Coverage strategy
11. Fixture and golden-file size policy

## 1. The architectural precondition

Every credential-free test in the reference set exists because somebody split "fetch bytes" from
"turn bytes into a model". Sone's parser tests are the clearest example: `parse_search_response`,
`parse_home_tabs`, `parse_playlist_items`, `parse_v1_module` and `DirectHitItem::from_typed_value`
are all pure functions over `&str`/`serde_json::Value`, so their tests are literal JSON literals
with no network, no auth and no async (ref:sone/src-tauri/src/tidal_api.rs:6290-6400). Sone has
roughly 145 inline `#[test]` functions across its Rust sources, 51 of them in `tidal_api.rs` alone.

**Rule for streamboat:** every TIDAL response type gets a pure `parse_*` (or `TryFrom<Bytes>`)
function that takes bytes/JSON and returns a typed result or a typed error. HTTP, auth and retry
live outside it. This one rule is what makes the rest of this section cheap.

## 2. Layer 1 — pure parser tests with captured fixtures (the bulk of the suite)

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

## 3. Layer 2 — transport-level tests (in-process server or stubbed transport)

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
2. 429 sets a global cooldown, clamped. Sone's `RateGate` clamps `Retry-After` to `[1, 120]`
   seconds with a 5-second default when the header is absent or in HTTP-date form, and stores an
   absolute deadline via `fetch_max` so concurrent 429s can only lengthen it
   (ref:sone/src-tauri/src/rate_gate.rs).
3. Terminal playback sub-statuses are not retried. The survey records sone treating 4005, 4010 and
   4030–4035 as terminal; **unverified in this pass** — verify the exact list against the
   streaming-topic research before encoding it.
4. The quality fallback cascade stops at the first success and does not cascade past a rate-limit
   or terminal error.
5. Token refresh persists the new tokens to storage exactly once and does not lose the refresh
   token when the response omits it.
6. **Multi-process**: a second process refreshing the same stored token does not invalidate the
   first process's session. See `secrets-and-tokens.md` §7 (Multi-process token and device
   ownership) — this is required once desktop and headless can both be running, which is now.

## 4. Layer 3 — contract tests against the published spec

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

## 5. Layer 4 — opt-in live tests ("canary")

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

**This is the strongest working precedent for the credential-free canary in the whole reference
set, and it already runs on a schedule.** `ref:mopidy-tidal/.github/workflows/integration.yml`
runs this exact pexpect suite on a 6-leg Python × mopidy matrix, `fail-fast: false`, on
`cron: "0 0 * * 0"` (weekly) **plus** on every push and PR — with no secrets, no live account, and
no coverage upload. Copy the shape directly: a scheduled, fork-safe, unauthenticated workflow that
asserts the device-flow surface still looks like a device-flow surface. (This is a *different*
workflow from mopidy-tidal's unit-test matrix — see `sources.md` for the corrected split.)

## 6. Fuzzing manifest and response parsing

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
5. **The `tidal://` URI handler** — this is the one input that arrives unsolicited from the open
   internet: registering `x-scheme-handler/tidal` means any web page the user visits can invoke
   `streamboat play <arbitrary string>` as a process argument, pre-authentication. Fuzz the parser
   against an allowlist of shapes (`tidal://track/<digits>`, `album`, `playlist/<uuid>`, `artist`,
   `mix`) with hard rejection of everything else — path traversal, absurd lengths, embedded
   newlines/NULs, non-ASCII homoglyphs, anything that would be interpolated into a shell, a
   filesystem path, or an outbound URL. Never pass the raw argument to a subprocess or to the API
   host without re-composing the request from the parsed id. tidalt's `Exec=tidalt play %u`
   (ref:tidalt/cmd/tidalt/tidalt.desktop) plus its D-Bus-forwarding `play.go` is the exact code
   shape to threat-model.

Corpus: seed from the committed fixtures (the tidal-sdk-web test file alone gives DASH-FLAC,
DASH-AAC, BTS-MP3 and EMU-HLS manifests to seed with). Run the fuzzer in CI for a bounded time
(60 s per target on PRs, 15 min nightly), commit crashers as regression fixtures.

Independent of the fuzzer: **set hard limits** — reject a manifest over N bytes (start at 1 MiB),
disable XML external entities and DTD processing outright, cap segment count, and cap total
decoded size.

## 7. Audio pipeline tests

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
   (sone's formula is `0.8 * min(10^((rg+4)/20), 1/peak)`, per the prior survey — **unverified in
   this pass**, confirm against ref:sone/src-tauri/src/commands/playback.rs before encoding),
   quality-ladder mapping, the ALSA/WASAPI fallback state machine, queue/gapless scheduling
   decisions. These run everywhere, including CI, and should be the majority of audio tests.
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
   the log path. Add per-OS rows: Windows (NVDA/UIA + SMTC), macOS (VoiceOver/AXAPI +
   MPNowPlayingInfoCenter) — see `i18n-a11y-observability.md`.

## 8. UI tests **[STACK]**

- Component-level with a DOM: sone runs vitest + jsdom + `@testing-library/react` with a
  dedicated `vitest.config.ts` separate from the app's Vite config, covering 46 test files across
  components, hooks and contexts (~19 of them directly under `src/components/`)
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

## 9. The non-negotiable CI rule

**The default test job must pass on a fork PR with no secrets, with the network disabled.**
An environment-level block (a proxy env var pointing at a dead port) is not by itself a reliable
mechanism: many HTTP clients ignore unset/dead proxy vars, and it does nothing on the Windows and
macOS legs of the CI matrix (see `ci-and-repo-governance.md` §1). Enforce it in the code instead:
the offline test build should construct the client with a transport that panics/fails on any real
connection attempt — the same seam already recommended for stubbed transports in §3 above
(tidalt's `roundTripFunc`, tidal-sdk-ios's `JsonEncodedResponseURLProtocol`), so an accidental
network call is a deterministic test failure on every OS. Layer an environment-level block on top
only on Linux, where `unshare -n` (or a network-less container) actually isolates the process, as
a second net. Everything credentialed goes in a separate workflow gated on
`github.event.pull_request.head.repo.full_name == github.repository`, the pattern Strawberry uses
for its signing/notarizing steps (ref:strawberry/.github/workflows/build.yaml:1178, 1253).

## 10. Coverage strategy

No reference project names a coverage target except mopidy-tidal, which states plainly:
"Mopidy-Tidal has a test suite which currently has 100% coverage. Ideally contributions would come
with tests to keep this coverage up" (ref:mopidy-tidal/DEVELOPMENT.md), and its CI uploads to
Codecov on the Python 3.13 leg of `test.yml` (not the integration matrix — see `sources.md`).
For streamboat: measure coverage on every PR, publish the number, and gate on "no decrease" for
the `core` parsing/auth crate only, where the parser/transport split of §1 makes 90%+ trivially
reachable — explicitly exclude UI and platform-audio code from the gate so the number cannot be
gamed by untestable surfaces.

## 11. Fixture and golden-file size policy

Store real API captures under `tests/fixtures/api/`, golden audio under `tests/fixtures/audio/`,
and committed fuzz crashers as regression fixtures — but state a repo-size policy before the first
one lands, because it grows without bound otherwise: a byte cap per fixture (audio fixtures
<200 KB per §7; extend the cap to JSON captures, gzip large ones), generate audio goldens from a
tone/sweep at test time where determinism allows and commit only the SHA-256 rather than the PCM,
keep fuzz crashers minimised before committing, and decide explicitly for or against Git LFS now
because switching later rewrites history. No reference project uses LFS, and Strawberry's
12-format audio corpus (ref:strawberry/tests/data/audio/) is small enough to live in plain git —
treat that as the working default, but record it as a decision.
