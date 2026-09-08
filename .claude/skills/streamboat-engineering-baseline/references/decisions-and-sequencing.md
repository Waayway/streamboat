# Baseline table, decide-now checklist, packaging sequence, open questions

Full source: `docs/research/engineering-baseline.md` §1 and the "Implications for streamboat" /
"Open questions" sections (fact-checked). `ref:<project>/<path>` points at a shallow clone — see
`sources.md`. SKILL.md carries condensed versions of the Open decisions and Unverified sections;
this file is the full-depth version of both, plus the material SKILL.md has no room for (the
baseline table, the 31-item do-now checklist, the packaging sequence, and the stack-blocked table).

## Contents

1. Baseline: what the reference projects actually do (comparison table)
2. Decide and do now (stack-independent) — 31 items
3. Sequence the packaging work
4. Items that must wait for the stack decision
5. Open questions (owner decisions, full text)
6. Unverified items (full text)

## 1. Baseline: what the reference projects actually do

| Project | License | Tests in CI | HTTP test double | Credentialed CI | Lint/format gate | Packaging targets |
|---|---|---|---|---|---|---|
| python-tidal | LGPL-3.0-or-later | none (lint only) | none — live account | n/a (local only) | isort + black + docformatter, mypy via tox | PyPI |
| mopidy-tidal | Apache-2.0 | yes — unit tests on Python 3.12/3.13/3.14 (`test.yml`); separately, a weekly + push/PR integration matrix of Python 3.12/3.13/3.14 × mopidy 3.3/3.4 (`integration.yml`) | `unittest.mock` over `tidalapi`; `pytest-httpserver` + `trustme` for the caching proxy; `pexpect`-driven real-Mopidy integration tests | no | ruff check + ruff format | PyPI |
| Sone | GPL-3.0-only | not wired to GitHub Actions (only flathub-update) | inline `serde_json::json!` fixtures for parsers; jsdom + Testing Library for UI | no | eslint, prettier, clippy `-D warnings`, cargo fmt, knip | Flathub, AUR (2 pkgs), Cloudsmith deb/rpm, Snap |
| Sone-windows | GPL-3.0-only | not inspected in depth | — | — | same toolchain as Sone | Windows |
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

## 2. Decide and do now (stack-independent)

1. **Adopt the parser/transport split as a hard architectural rule.** It is the precondition for
   every credential-free test (`testing-strategy.md`) and the single highest-leverage decision in
   this document.
2. **Write the CI contract into `CONTRIBUTING.md`**: the default suite passes on a fork PR, with no
   secrets, with the network blocked. Credentialed and networked jobs are separate, guarded, and
   never required for merge.
3. **Stand up the fixture pipeline before the first API call is written**: `tests/fixtures/api/`,
   a `scripts/capture-fixture` that records a response with its request metadata, a
   `scripts/scrub-fixture` that redacts, and a CI check that fails on token-shaped strings.
4. **Vendor `contracts/tidal-api-oas.json` and add the daily drift-diff workflow** (copy
   ref:tidal-sdk-ios/.github/workflows/check-tidalapi-spec.yml). Hand-write JSON Schemas for the
   v1 playback endpoints that the OAS does not cover.
5. **Build the opt-in live canary and the credential-free unauthenticated canary**
   (`testing-strategy.md` §5) before the client has many endpoints, so the pattern is cheap to
   extend.
6. **Choose the license split now**: GPL-3.0-only for the apps, LGPL-3.0-or-later or Apache-2.0 for
   `core`. Add SPDX headers from the first file; retrofitting requires chasing contributors.
7. **Implement the secret store as an interface with three backends** (OS keyring, encrypted file,
   plaintext-with-loud-warning) and pick at runtime. Copy Sone's AES-256-GCM envelope format
   including the magic-header migration path.
8. **Fix the on-disk layout now**: platform-native config/cache/data/state directories, env-var
   overrides, cache *outside* config, logs in state/Logs. Changing this later means writing
   migration code.
9. **Ship the tiered cache with a hard cap, TTL, SWR and LRU eviction** using Sone's numbers as
   defaults, and expose usage + a clear button.
10. **Turn logging on with rotation at 5 MB × 10 files, a pre-start plaintext toggle, and a
    redaction type that makes leaking a token a compile error where the stack allows it.**
11. **Write `docs/legal.md` and the README disclaimer in Sone's words** (adapted): independent,
    not affiliated, requires a paid subscription, streaming client only, no offline downloads, no
    circumvention. Keep TIDAL out of the name, icon and app ID.
12. **Decide and publish the project's AI-assistance policy** in CONTRIBUTING, aligned with
    Flathub's disclosure requirement — this is a gating question for Flathub distribution, not a
    style preference.
13. **Ship the four issue templates**, with the playback template copied field-for-field from Sone.
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
19. **Decide the multi-process token/device-ownership model now** (`secrets-and-tokens.md` §7):
    either the daemon is always the token owner with the GUI as a client, or the store needs a
    cross-process lock. Add a test for it before the desktop and headless binaries can both be
    running at once.
20. **Fuzz the `tidal://` URI handler** (`testing-strategy.md` §6 item 5) alongside the
    manifest/playbackinfo targets — it is the one input surface reachable from an arbitrary web
    page before authentication.
21. **Define a coverage target for `core` and publish it** (`testing-strategy.md` §10); state a
    fixture/golden-file size policy and an LFS decision before the first large fixture lands
    (`testing-strategy.md` §11).
22. **Ship one reproducible dev environment on day one** (`repo-layout-and-docs.md` §4: a Nix
    flake or container, plus a Makefile that mirrors CI) — this is what determines whether a
    first-time contributor gets a working build in five minutes or gives up.
23. **Budget for release signing/SBOM and for a portable Linux binary as no-precedent, first-week
    line items**, not later add-ons: no reference project attests a Linux binary
    (`packaging-and-distribution.md` §8), and none ships an AppImage-equivalent for the
    *streamboat* stack yet either (`packaging-and-distribution.md` §4).
24. **Record the EU CRA scope determination in `docs/legal.md`** (`licensing-and-legal.md` §5) —
    a five-minute decision today, and a compliance-timeline problem if left implicit past
    2026-09-11.
25. **Write every persisted secret/settings/state file atomically** (temp file in the same
    directory → write → fsync → rename → fsync the directory) from the first commit
    (`secrets-and-tokens.md` §3) — Sone itself only does this for theme files, not
    settings/tokens, and retrofitting after real users have on-disk state is expensive.
26. **Implement `logout` in the Sone ordering, and ship a separate `streamboat purge`**
    (`secrets-and-tokens.md` §8) that also removes the keyring entry, key file and logs — decide
    this alongside item 19's multi-process token model, since both concern the same on-disk state.
27. **Decide the app ID / GitHub owner before the first commit** (`packaging-and-distribution.md`
    §2) — baked into the Flatpak ID, D-Bus name, macOS bundle identifier, and the Windows
    AppUserModelID; moving it later costs a Flathub `end-of-life-rebase` plus a data migration.
28. **Budget the Windows/macOS media-runtime bundling problem as a first-week line item**
    (`packaging-and-distribution.md` §4a): neither OS ships GStreamer/FFmpeg, so the installer must
    bundle the runtime and the app needs a bundled-vs-system plugin-path code path — the largest
    Windows-specific fact in the reference set (`sone-windows`), easy to discover only late.
29. **Add a package-install smoke test to CI on release-candidate tags**
    (`ci-and-repo-governance.md` §1, `packaging-and-distribution.md` §4): copy Sone's
    Docker-per-distro harness rather than relying on a build-only job to catch a missing runtime
    dependency.
30. **Design data export/import and a first-run "found other install data" probe before shipping
    AppImage-then-Flatpak** (`config-cache-logs-telemetry.md` §6) — the recommended packaging
    sequence otherwise guarantees the first upgrade silently loses all local data.
31. **Write `docs/releasing.md` as an executable runbook and pick one file as the single version
    source before cutting the first tagged release** (`packaging-and-distribution.md` §8) — every
    other packaging file (snapcraft, PKGBUILD, debian/changelog, `.spec`, metainfo `<release>`)
    should derive from it, with a CI check that the newest metainfo `<release>` matches the tag.

## 3. Sequence the packaging work

1. GitHub Releases with `checksums.txt` and a tag-triggered build (week 1 of releasing).
2. **AppImage (or a static tarball for the daemon/CLI)** — the only day-one option that reaches a
   user on a distro not covered by AUR or the Cloudsmith deb/rpm matrix; see
   `packaging-and-distribution.md` §4.
3. AUR `-bin` + AUR source package; deb/rpm built in pinned containers, published to Cloudsmith.
4. Flatpak manifest built in CI from the start; **submit to Flathub only after several
   months of tagged releases and real users**, per the development-history requirement, and
   re-read the Generative AI policy immediately before submitting
   (`packaging-and-distribution.md` §1).
5. Snap once the ALSA plug support burden is understood.
6. Windows MSI/NSIS unsigned → winget once signing is sorted, and once the silent-install switches
   and install `Scope` are decided (`packaging-and-distribution.md` §5).
7. macOS DMG only after the $99 Apple Developer Program is in place; Homebrew cask only after
   notarization and after clearing the disjunctive 30-or-30-or-75 (90-or-90-or-225 self-submission)
   notability bar.

## 4. Items that must wait for the stack decision

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

## 5. Open questions (owner decisions, full text)

Only the owner can settle these:

1. **AI-assistance posture.** Flathub requires disclosure of AI-generated code, documentation and
   packaging, forbids AI-opened submission PRs and AI-generated commit messages/PR descriptions on
   its side, and reserves the right to reject on the extent of generated material. Is Flathub a
   required distribution channel? If yes, what is the project's disclosure statement, and what
   process keeps commit messages and submission PRs human-authored?
2. **License split.** GPL-3.0-only everywhere, or Apache-2.0 `core` + GPL-3.0-only apps? The second
   is recommended and is effectively irreversible without contributor consent — **and it is now
   also the mobile decision**: GPL-3.0-only on the app is incompatible in practice with Apple App
   Store distribution, so an Apache-2.0 `core` is what keeps a future iOS app possible at all
   (`licensing-and-legal.md` §2).
3. **Contributor agreement.** DCO sign-off, a CLA, or neither? Without one, relicensing later is
   impossible in practice.
4. **Play reporting to TIDAL.** On by default (so Recently Played works, as Sone does) or off by
   default (strictest privacy reading)? Either is defensible; it must be disclosed and switchable.
5. **Update check on by default?** It is a network request to GitHub on launch. Recommend on for
   source/manual installs, off for managed packages.
6. **Embedded client credentials.** Ship an obfuscated default (better first-run UX, extractable,
   arguably higher ToS exposure) or require the user to supply their own (worse UX, cleaner
   posture)? A build flag can support both, but the default binary's behaviour is a decision.
7. **Audio buffer cache on disk.** Any on-disk audio buffering blurs the line the project has drawn
   against downloading. Off by default with a small cap, or not at all?
8. **Apple Developer Program ($99/year) and Windows signing (reported ~$120/year Trusted Signing
   Basic, or ~$280–500/year EV — see `packaging-and-distribution.md` §5 for the unverified-pricing
   caveat, and note individual Trusted Signing eligibility is restricted to the US/Canada while
   organisation eligibility spans a 12-country list — plan for who, geographically, can actually
   buy it).** Who pays, and are macOS/Windows first-class targets from v1 or best-effort?
9. **Headless control surface.** MPRIS/D-Bus only (Linux-native, no new attack surface) or an
   HTTP/JSON API (cross-platform, remote control, needs auth and hardening)? This changes the
   security model and the AGPL question. If an HTTP surface is chosen, Sone's loopback-bound,
   token-authenticated, field-sanitized MCP server (`i18n-a11y-observability.md` §7) is the
   baseline design to copy or deviate from explicitly, not a blank page.
10. **Release cadence.** Weekly-ish minors like Sone, or slower and batched? It affects Flathub
    eligibility (no daily-update software in stable) and the changelog burden.
11. **Translation platform.** Self-hosted Weblate, Crowdin (Strawberry's choice), or plain PRs
    against `po/`?
12. **Minimum supported platform versions.** Oldest glibc/Ubuntu for the deb, oldest macOS SDK,
    oldest Windows. These constrain runner and container choices immediately.
13. **Release automation tool and monorepo versioning.** No reference project uses a
    release-automation tool (release-please / changesets / semantic-release / cargo-dist /
    goreleaser+nfpm) — pick one appropriate to the chosen stack. Separately, decide whether `core`
    is versioned and released independently of the apps: that is what makes a permissively
    licensed `core` (item 2 above) actually adoptable by other clients, the stated reason for the
    license split in the first place (`packaging-and-distribution.md` §8).
14. **Forge choice.** Nearly every mechanism this skill recommends — CI, secret scanning, private
    vulnerability reporting, `io.github.*` Flathub verification, provenance attestation, the
    AUR/winget/Flathub automation, the GitHub-Environments secret scoping
    (`ci-and-repo-governance.md` §4) — assumes GitHub. Given the project's own stated posture
    (independence, no telemetry), whether to host on GitHub vs. Codeberg/self-hosted is a real
    decision with real tooling consequences. `packaging-and-distribution.md` §2 already makes the
    GitHub *owner* permanent through the app ID; settle the *platform* at the same time, as one
    ADR, not two.
15. **Fixture provenance.** Synthetic (structure-preserving, content-invented) fixtures, or real
    captured TIDAL API responses committed to a public test-fixtures directory? A committed capture
    carries TIDAL's own editorial copy and catalogue metadata, not just tokens — this is a
    legal/ToS posture decision, not only a redaction mechanic. The reference precedent splits:
    Sone's ~51 parser tests use hand-written `serde_json::json!` literals, not captures;
    tidal-sdk-web's manifest-parser fixtures are real captured base64 manifests — but that is
    TIDAL's own repo committing TIDAL's own payloads, not a third-party client doing it. Default
    recommendation: minimal synthetic fixtures for catalogue/editorial payloads, real captures only
    where the exact bytes are under test (manifests, error envelopes). Record the decision in
    `docs/legal.md` (`testing-strategy.md` §2).

## 6. Unverified items (full text)

Items that must be checked before anything is published:

- **TIDAL's Terms of Service and Developer Terms could not be read** from this environment
  (`tidal.com` and `developer.tidal.com` are blocked by the egress proxy). A search result
  attributes to Developer Terms 2.0 the claim that the SDK's Player module "constitutes the only
  allowed way for third-party applications to incorporate playback of TIDAL content" — treat that
  as unconfirmed until read directly. The ToS was reported as effective 2026-06-29. See
  `licensing-and-legal.md` §4 for the scope distinction (developer terms vs consumer ToS) that
  matters regardless of how this verification lands.
**Two items previously listed here are now resolved, not unverified** — see `testing-strategy.md`
§3 item 3 and §7 item 1 for the exact source: the terminal playback sub-status list is
`[4005, 4010, 4030, 4031, 4032, 4034, 4035]` (non-contiguous — `4033` and `4006` are deliberately
excluded as recoverable, ref:sone/src-tauri/src/tidal_api.rs:16-18), and the ReplayGain formula is
`0.8 * min(10^((replay_gain + 4) / 20), 1 / peak)` verbatim
(ref:sone/src-tauri/src/commands/playback.rs:9-20).
- **Windows Credential Manager blob limit**: `CRED_MAX_CREDENTIAL_BLOB_SIZE` is documented as
  `5*512` = 2560 bytes on Windows 7+ per Microsoft's own docs source, but reports differ on
  whether `CRED_TYPE_GENERIC` is further limited to 512 bytes, **and mingw-w64's own header
  disagrees outright** (bare `512`, no version guard) — a real toolchain-specific trap for a
  MinGW-targeted Windows build. Measure with a real token before designing around it (see
  `secrets-and-tokens.md` §2 for the "store a key, not the payload" design that sidesteps this,
  and for the corrected source attribution — an earlier draft of this document mis-cited the
  mingw-w64 header as the *verification* source for the 2560-byte figure, which is backwards).
- **The set of locales TIDAL accepts** for the `locale` parameter is not documented anywhere in the
  reference checkouts; every project hardcodes `en_US`. Determine empirically.
- **Qt 6 module-by-module licensing** (which modules are GPL-2.0-only vs GPL-3.0-only) needs a
  per-module check against https://doc.qt.io/qt-6/licensing.html if Qt is chosen — this
  environment could reach it only via a search index, not a direct fetch.
- **Flathub's Generative AI policy text is a dated snapshot** (repository HEAD 2026-09-07, re-read
  directly and matches verbatim). A separate claim that it already flipped once (commit `992f57b`,
  2026-05-29, briefly an outright ban) before reverting to the disclosure regime quoted in
  `packaging-and-distribution.md` §1 **could not be independently re-verified in this pass**
  (GitHub API access to that repo's commit history is not enabled here) — do not repeat the
  specific commit/date as settled fact without checking it directly. Re-read the live page
  immediately before submitting, not just once during research, regardless.
- **Azure Trusted Signing / Artifact Signing pricing and eligibility** ($9.99/$99.99 per month,
  signature caps, the paid-subscription requirement, and the eligibility split —
  organisations across a 12-country list (US, Canada, EU, UK, Australia, New Zealand, Japan, South
  Korea, Singapore, Switzerland, Norway, Israel), individual developers restricted to the US or
  Canada, individual onboarding reportedly paused — come from a search index over Microsoft's
  pricing/FAQ pages, not a direct fetch — both `azure.microsoft.com` and `learn.microsoft.com` are
  blocked from this environment. Re-verify before it drives a cost decision
  (`packaging-and-distribution.md` §5, §6); do not reintroduce the "individual eligibility is not
  narrower than organisation eligibility" reading — that reading was wrong and has been reverted.
- **winget's "7-day timer before the bot closes an assigned PR"** could not be confirmed — the
  primary `learn.microsoft.com` policy page is blocked from this environment, and the only
  auto-close mechanism found in reachable sources is tied to the `Needs-Author-Feedback` label.
  Drop or hedge this detail until read from the primary source (`packaging-and-distribution.md` §5).
- **Resolved, no longer unverified** — the Flathub linter's "never granted" exception rule for
  `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID`. A direct fetch of
  `flathub-infra/flatpak-builder-lint`'s `flatpak_builder_lint/checks/finish_args.py` (not
  `docs.flathub.org`, which remains blocked) confirms rule id
  `finish-args-mpris-flatpak-id-talk-name`, and the live `staticfiles/exceptions.json` contains
  zero entries for it (`packaging-and-distribution.md` §1). The companion
  `finish-args-incorrect-secret-service-talk-name` rule for the miscased `org.freedesktop.Secrets`
  name is confirmed the same way (`secrets-and-tokens.md` §2).
- **Whether Snap's `alsa` interface can be requested for auto-connection** for a media player, and
  what the store review process requires, was not checked against `snapcraft.io/forum`
  (`packaging-and-distribution.md` §3).
- **GStreamer, Qt and fdk-aac licensing claims** are correct in substance but several of the
  specific source URLs originally cited in an earlier draft of this document did not actually
  contain the quoted text on direct inspection; `licensing-and-legal.md` §3 now attributes each
  quote to its actual source file/page. Where the corrected source is itself unreachable from this
  environment (Qt, fdk-aac), the claim is read via a search index rather than a primary fetch —
  re-verify before publishing if Qt or a patent-encumbered codec is in scope.
