# CI and repo governance

Full source: `docs/research/engineering-baseline.md` §7 (fact-checked). `ref:<project>/<path>`
points at a shallow clone — see `sources.md`.

## Contents

1. Job matrix
2. Caching
3. Gates, and what a hook can do instead
4. Security scanning and dependency policy
5. Reproducible builds
6. Repo governance mechanics

## 1. Job matrix

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
| package install smoke test | Docker, per distro | installs the built `.deb`/`.rpm`, asserts window/daemon start + MPRIS/control-socket name + no missing `.so` — release-candidate tags only (`packaging-and-distribution.md` §4) |

Plus scheduled jobs: daily API-spec diff (`testing-strategy.md` §4), weekly live canary run from a
**maintainer-local cron, not a GitHub-hosted self-hosted runner on the public repo**
(`testing-strategy.md` §5 — a self-hosted runner on a public repo is a code-execution risk; never
use `pull_request_target` either), weekly full-matrix rebuild (mopidy-tidal runs its integration
suite on `cron: "0 0 * * 0"` — this is a *separate* workflow from its unit tests, see `sources.md`).

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

## 2. Caching

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

## 3. Gates, and what a hook can do instead

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

## 4. Security scanning and dependency policy

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
  SonarCloud (`.sonarcloud.properties`). **No reference project runs a sanitizer, Valgrind, Miri or
  CodeQL at all** (grep across all 21 checkouts for `fsanitize|ASAN|UBSAN|valgrind|miri` hits only
  a false positive; zero CodeQL/Scorecard workflows) — run the §2.6 fuzz targets under ASan+UBSan
  nightly regardless; see `testing-strategy.md` §6 for the full recommendation.
- **Supply chain**: pin actions by SHA (above), enable branch protection with required checks,
  require signed commits or at least DCO, enable GitHub secret scanning + push protection, publish
  build provenance attestations, and publish `checksums.txt` with every release (see
  `packaging-and-distribution.md` §8 for the "no precedent, budget accordingly" caveat on
  provenance/SBOM).
- **License scanning**: see `licensing-and-legal.md` §3.

## 5. Reproducible builds

Full bit-for-bit reproducibility is a large project; the achievable subset:

- Set `SOURCE_DATE_EPOCH` from the tag's commit date in every packaging job.
- Commit and use lockfiles everywhere; build with `--frozen-lockfile` / `--locked`.
- Pin toolchain versions in-tree (`rust-toolchain.toml`, `.nvmrc`, `go.mod` `go` directive,
  `.python-version`) and have CI read them rather than hardcoding. **Rarer in the reference set
  than that phrasing implies** — only 2 of 21 checkouts pin a toolchain in-tree at all
  (`ref:tidal-hifi/.nvmrc`, `ref:tidal-sdk-web/.nvmrc`); no `rust-toolchain.toml` exists anywhere,
  sone's `Cargo.toml` has no `rust-version` MSRV field (ref:sone/src-tauri/Cargo.toml:7).
- **Pinning the build toolchain and declaring a minimum-supported toolchain (MSRV) are two
  different decisions — make both explicitly, in `docs/DECISIONS.md`.** The build pin is what
  reproducible-build tooling reads; the MSRV is what a Debian/Fedora packager checks before they
  can package streamboat at all — an MSRV newer than Debian stable's compiler means no Debian
  package, ever. Choose the MSRV against the oldest target distro's shipped toolchain (the same
  constraint that drives building the deb on Ubuntu 22.04 for the oldest glibc,
  `packaging-and-distribution.md` §4), and add a dedicated "oldest supported toolchain" CI leg.
- Build release artifacts inside a pinned container image (sone's Dockerfiles pin
  `ubuntu:22.04` and `pnpm@11.1.3`; tidalt pins `FFMPEG_VERSION=7.1.5`).
- Publish the exact build command and container digest in the release notes.
- Strip and normalise: `-ldflags="-s -w"` (Go), `strip = true` in the Rust release profile, and
  avoid embedding absolute build paths.
- Verify by rebuilding one release from the tag on a clean machine and diffing hashes; document
  the result even when it is "not yet reproducible, differs in X".

## 6. Repo governance mechanics

§4 says "enable branch protection with required checks"; `repo-layout-and-docs.md` §3 covers
CONTRIBUTING and templates. The mechanical pieces that make branch protection and code review
actually work are cheap to get right at repo creation and annoying to retrofit:

- **`CODEOWNERS`.** All three TIDAL SDKs ship `.github/CODEOWNERS`
  (ref:tidal-sdk-web/.github/CODEOWNERS, ref:tidal-sdk-ios/.github/CODEOWNERS). Add one from the
  start so `core` and the packaging directories have explicit reviewers as contributors arrive.
- **`concurrency:` groups with cancel-in-progress** on PR workflows, used by tidal-sdk-android
  (`pull-request.yml`, `post-merge.yml`, `publish-pages.yml`) and tidal-sdk-web (`cypress.yml`).
  Without one, a force-push queues a duplicate full matrix.
- **Name the required status checks explicitly.** State in `docs/DECISIONS.md` which of the §1
  jobs actually block merge — the credentialed and scheduled jobs (daily spec-drift, live canary)
  must **not** be in that list, per `testing-strategy.md` §9's own rule, or a fork PR will be
  unmergeable through no fault of the contributor.
- **State whether DCO sign-off is enforced by a bot** (e.g. the DCO GitHub App) — SKILL.md's Open
  decisions raise the CLA/DCO decision itself but not how it would be mechanically enforced once
  decided.
