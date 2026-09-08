# Repository layout, docs and contribution process

Full source: `docs/research/engineering-baseline.md` §8 (fact-checked). `ref:<project>/<path>`
points at a shallow clone — see `sources.md`.

## Contents

1. Workspace layout
2. Docs and ADRs
3. CONTRIBUTING, code of conduct, templates
4. Developer environment

## 1. Workspace layout

Target shape (names **[STACK]**-adjusted, structure is not):

```
/
├── README.md                  # what it is, install, disclaimer, no marketing
├── LICENSE / LICENSES/        # SPDX-named files; REUSE-compliant
├── CHANGELOG.md               # Keep a Changelog 1.1.0
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── SECURITY.md
├── .gitattributes             # line-ending normalisation + LFS decision (testing-strategy.md §11)
├── crates|packages|src/
│   ├── core/                  # API client, auth, models, manifest parsing — permissive license
│   ├── audio/                 # pipeline, sinks, gapless, ReplayGain — platform-conditional
│   ├── daemon/                # headless: MPRIS/D-Bus + local control API
│   ├── cli/                   # thin client over daemon; also the debug/diagnostic tool
│   └── desktop/                # GUI
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
│   ├── FUNDING.yml            # 7 of 21 reference projects ship one — set up before packaging costs land
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

## 2. Docs and ADRs

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
  `.claude/` itself is not absent — tidal-sdk-ios, an *official* TIDAL SDK, ships
  `.claude/commands/create-release-pr.md` alongside its `.agents/`, and five of the 21 checkouts
  ship a root `CLAUDE.md` (strawberry, tidalt, tidalswift, tidal-cli, tidal-sdk-ios — two of them
  official SDKs); tidalswift also ships `AGENTS.md`. So the finding is narrower than "nobody uses
  `.claude/`": `.claude/skills/` specifically is unused, `.agents/skills/` is the pattern with
  working CI-invocation precedent, and a root `CLAUDE.md` is common enough (5/21, including
  official SDKs) not to read as unusual on its own. Adopt both real patterns regardless: skills
  that wrap real scripts CI also runs, and review rules that never re-litigate what the formatter
  owns. Whichever directory streamboat itself uses for its own agent-facing skills, a
  vendor-branded directory name is still the kind of detail a Flathub reviewer could read as an
  AI-tooling signal under the Generative AI policy (`packaging-and-distribution.md` §1) — a neutral
  `.agents/` name carries less of that risk if the project ever wants to minimize it.
- tidal-cli ships `skills/tidal-cli/SKILL.md` and publishes it as a distributable artifact in its
  release workflow — evidence that a skill can be a shipped deliverable, not just repo furniture.

## 3. CONTRIBUTING, code of conduct, templates

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
it must be consistent with Flathub's disclosure requirement (`packaging-and-distribution.md` §1).

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
vulnerability reporting), a target acknowledge/fix timeline (90 days is the common default — **this
is a separate clock from the EU CRA's 24h/72h/14-day incident-reporting timeline**, do not reuse
those numbers here, see `licensing-and-legal.md` §5), and an explicit scope note that token
handling and the local control API are in scope, "TIDAL's own service" is not, and reports about
circumventing TIDAL's DRM are out of scope and will not be accepted.

**Branch strategy**: `main` protected, feature branches, squash merge, release tags on `main`. A
`develop` branch (tidal-hifi) only pays off with several contributors and a slow release train;
for a solo/small project, trunk plus tags is simpler and matches sone, tidalt and the SDKs.

## 4. Developer environment

streamboat depends on GStreamer/ALSA/PipeWire/system audio libraries; "install these 30 packages"
is where contributors bounce, and CI reproducibility depends on the same answer — yet nothing in
§1–3 says how a contributor gets a working build. Five of the 21 checkouts ship a Nix flake with a
devShell: sone, high-tide, mopidy-tidal, python-tidal, TidaLuna. sone's is the instructive one —
its devShell adds nodejs/pnpm/cargo/cargo-tauri and its `shellHook` exports
`GST_PLUGIN_SYSTEM_PATH_1_0` composed from gstreamer plus plugins-base/good/bad/gst-libav, exactly
the class of setup that breaks on a bare distro (ref:sone/flake.nix). high-tide's flake pulls in
meson, ninja, blueprint-compiler, libadwaita, glib-networking, gst_all_1, libsecret, libportal and
alsa-utils (ref:high-tide/flake.nix). mopidy-tidal instead documents a two-command `uv` workflow
plus `make test`/`make format` (ref:mopidy-tidal/DEVELOPMENT.md). Ship `docs/development.md` plus
one reproducible dev shell (flake or container) plus a task runner. Two cheap, cited conventions
the repo layout in §1 omits: three references (python-tidal, tidal-sdk-ios, tidalt) use a Makefile
as the CI-mirroring entry point (ref:tidalt/Makefile), and three (tidal-hifi, tidal-sdk-android,
tidal-sdk-web) ship a `.editorconfig` (ref:tidal-hifi/.editorconfig).
