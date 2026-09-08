# Distribution and packaging

Full source: `docs/research/engineering-baseline.md` §6 (fact-checked). `ref:<project>/<path>`
points at a shallow clone — see `sources.md`.

## Contents

1. Flathub (Linux desktop — the primary channel)
2. Flathub: metainfo requirements, submission mechanics, maintenance and verification
3. Snap
4. deb / rpm / AUR (and the missing portable-binary step)
5. Windows
6. macOS
7. Auto-update strategy overall
8. Versioning, changelog, release cadence
9. Headless/daemon packaging deliverables

## 1. Flathub (Linux desktop — the primary channel)

Requirements that bite a TIDAL client, all from
https://docs.flathub.org/docs/for-app-authors/requirements (read at HEAD 2026-09-07, via the
`flathub-infra/documentation` repository directly — `docs.flathub.org` itself is egress-blocked
from this environment):

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
  rule `metainfo-missing-screenshots` is "never granted". See §2 for the concrete fields.
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
  **This policy has already flipped once in 2026 and must be re-read before submission, not
  treated as settled**: commit `992f57b` (2026-05-29, "Reword LLM policy to make it clear it's not
  allowed") briefly replaced this text with an outright ban — "Applications containing
  AI-generated or AI-assisted code, documentation, or other content are not allowed ... Exceptions
  may be granted for mature, well-maintained projects" — which was widely reported as a ban before
  the text reverted to the disclosure-plus-reviewer-discretion regime quoted above. Treat the
  quoted text as a dated snapshot (read 2026-09-07 at commit HEAD of
  `flathub-infra/documentation`), add a standing task to re-read it immediately before any Flathub
  submission, and note the adjacent linter rule: sandbox-escape exceptions (`home`, `host`,
  `flatpak-spawn`, arbitrary bus names) are reported as "not... granted if there are signs of LLM
  usage in the software or in the exception PR" — which interacts directly with the permission
  plan below.

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
  Secret portal, which needs no permission (see `secrets-and-tokens.md` §2).
- `--own-name=org.mpris.MediaPlayer2.<something>` — allowed by default only when it exactly
  matches the Flatpak ID, is an exact subname of it, or is an MPRIS subname
  (linter rule `finish-args-own-name-cpt`). The default sandbox policy already lets an app own
  `org.mpris.MediaPlayer2.$FLATPAK_ID` with no extra finish-arg at all (confirmed by direct fetch
  of `flatpak/flatpak-docs`' sandbox-permissions doc: an app may "own its own namespace named by
  $FLATPAK_ID, subnames of it and org.mpris.MediaPlayer2.$FLATPAK_ID"). **Unverified**: whether the
  linter has a specific never-granted exception rule for
  `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID` could not be confirmed — `docs.flathub.org` is
  blocked in this environment and the rule name is not quoted in any reachable mirror. Do not
  request that talk-name (it should not be needed given the default policy above); re-verify the
  specific linter-rule wording from `docs.flathub.org/docs/for-app-authors/linter` or
  `flathub-infra/flatpak-builder-lint`'s `exceptions.json` before relying on the "never granted"
  framing.
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
depends on the language. **See §2 below for Flathub's own native alternative** (the External Data
Checker) before assuming this hand-rolled pipeline is the way to go.

**CI-side build check**: High Tide builds the Flatpak for both x86_64 and aarch64 using
`flatpak/flatpak-github-actions/flatpak-builder@v6` inside
`ghcr.io/flathub-infra/flatpak-github-actions:gnome-48`
(ref:high-tide/.github/workflows/flatpak.yml). The trigger is narrower than "every push and PR":
`push: branches: [master], paths-ignore: ['**/README.md']` and
`pull_request: branches: [master], types: [review_requested, ready_for_review]` — i.e. pushes to
`master` (excluding README-only changes) and PRs only once review is requested or the PR is marked
ready for review, not every PR event. Do this from day one regardless of the exact trigger — it is
the cheapest way to keep the manifest honest.

## 2. Flathub: metainfo requirements, submission mechanics, maintenance and verification

The requirements doc above says only "metainfo is mandatory and must pass validation" — not enough
to write the file from. Concrete fields, all from `flathub-infra/documentation` (read 2026-09-07):

- **Mandatory fields**: `<id>` exactly equal to the Flatpak app ID and the manifest; a
  `<metadata_license>` for the metainfo file itself (an AppStream-permissive value, e.g. `CC0-1.0`
  or `FSFAP` — **not** the same field as `<project_license>`); `<project_license>` as an SPDX
  identifier (e.g. `GPL-3.0-only`); `<name>` and `<summary>`; `<content_rating type="oars-1.1">`
  generated from the OARS website; at least one screenshot, hosted as a direct web resource; a
  `<developer id="..."><name>...</name></developer>` block with a reverse-DNS id (exactly one,
  untranslated); a `<releases>` tag ("Applications must supply a releases tag ... to pass
  validation"); `<url type="homepage">` at minimum, `url type="vcs-browser"` strongly recommended.
- **Quality guidelines with numbers**: app name ideally ≤15 chars, must be <20; summary ideally
  10–25 chars, must not exceed 35, sentence case, no trailing period, must not repeat the app name,
  must not start with an article, must not mention the toolkit/language/"free and open source";
  description ~3–6 lines at 70 columns, feature lists ≤10 items; screenshots max 1000×700
  (2000×1400 HiDPI), 3–6 for a medium app, captured on Linux with default system settings and
  native window decoration, one-sentence caption each, at least one in English; icon SVG or PNG
  ≥256×256, square, no baked-in shadows.
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
  `yq`-rewrite-and-PR pipeline** (§1 above) — decide between them rather than assuming the latter.
- **Verification (the checkmark badge)**: an `io.github.*` app ID is verified for free and
  immediately by authenticating as the GitHub repo owner (or an org admin) — another reason to
  take the `io.github.<owner>.streamboat` ID already recommended, and the practical precondition
  for the automerge options above. Domain-based alternatives exist (HTTPS token at
  `https://<domain>/.well-known/org.flathub.VerifiedApps.txt`, DNS TXT record, GitLab variants) but
  are unnecessary here.

Sources:
https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/03-metainfo-guidelines/index.md
and `.../01-quality-guidelines.md`;
https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/05-submission.md;
https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/06-maintenance.md;
https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/10-verification.md
(all read 2026-09-07).

## 3. Snap

Sone's `snapcraft.yaml` (ref:sone/snap/snapcraft.yaml) shows the audio-specific parts:

- `confinement: strict`, `base: core24`, `grade: stable`.
- Plugs: `home`, `network`, `network-bind`, `network-status`, `audio-playback`, `alsa`,
  `screen-inhibit-control`.
- **`alsa` is not auto-connected**: the description tells users "For exclusive ALSA output, run:
  `sudo snap connect sone:alsa`". Budget for that support burden or treat exclusive mode as
  unsupported under Snap. **Not independently verified**: Snap publishers can request
  auto-connection for an interface through a store review request on snapforum
  (snapcraft.io/forum), which was not consulted here. Before committing to Snap as an
  exclusive-ALSA channel, check whether the `alsa` interface has ever been auto-connected for a
  media player and what reviewers required — sone's own README implies the request was either not
  made or not granted, which is itself informative but not conclusive.
- MPRIS needs a **dotless** slot name: `slots: { mpris: { interface: mpris, name: sone } }`, and
  the app must own `org.mpris.MediaPlayer2.<that name>` — sone detects it via the `SNAP` env var.
- Staging GStreamer requires `layout:` binds for `gstreamer-1.0`, `alsa-lib` and `/usr/share/alsa`,
  plus `GST_PLUGIN_SYSTEM_PATH`.

## 4. deb / rpm / AUR

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
history of development" (§1) and Homebrew requires a 30-day-old repo at minimum (§6) — so for the
first several months the only Linux channels this plan reaches are AUR and Cloudsmith deb/rpm. A
user on Fedora Silverblue, NixOS, Debian stable, or any distro outside the Cloudsmith matrix has
nothing to install. Both large reference GUI clients ship a portable artifact: Strawberry has a
dedicated AppImage job (ref:strawberry/.github/workflows/build.yaml) and tidal-hifi builds an
AppImage via electron-builder (ref:tidal-hifi/build/electron-builder.yml). **Add AppImage (or a
static tarball for the daemon/CLI) as the first packaging deliverable**, before AUR. One caveat the
references already hit: an AppImage has no stable app identity for the keyring — sone's own
`crypto.rs` comments that the keyring "may be unreachable on next launch (e.g. AppImage with
different D-Bus session)" — which is exactly why the encrypted-file fallback in
`secrets-and-tokens.md` §3 is mandatory, not optional, for this channel.

## 5. Windows

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
  requirements most likely to fail a first submission**: the installer must support a
  silent/unattended install (`InstallerSwitches: Silent` / `SilentWithProgress`, e.g. `/S` for
  NSIS or `/qn` for msiexec) — the automated validation pipeline installs and uninstalls unattended
  in a sandbox, and an installer that shows UI fails outright; and the manifest must declare
  `Scope` (user vs machine) — a per-user install avoids UAC and is the better default for a music
  player, but it changes the install path and therefore the `%LOCALAPPDATA%` layout assumptions in
  `config-cache-logs-telemetry.md` §1. Decide both before cutting the first Windows installer.
  Generate the manifest with `wingetcreate`; automate updates from the release workflow once the
  release assets have stable names.
- No reference project auto-updates on Windows; electron-builder's `publish`/`autoUpdater` is not
  configured in tidal-hifi's build configs.

## 6. macOS

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
  notarization (neither `developer.apple.com/programs/` nor `.../support/developer-id/` lists a
  separate notarization fee — an absence-of-evidence inference, not an explicit quote).
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
  normally ineligible; casks must pass Homebrew's Gatekeeper audit and "must not require System
  Integrity Protection or Gatekeeper to be disabled or bypassed", and casks failing those checks
  are deprecated, disabled and removed
  (https://github.com/Homebrew/brew/blob/main/docs/Package-Acceptance-Policy.md,
  .../Acceptable-Casks.md, .../Homebrew-Security-and-Supply-Chain.md). In short: **no
  notarization, no Homebrew.** Ship a personal tap in the meantime.
- **Auto-update**: Sparkle is the standard and Strawberry enables it (`-DENABLE_SPARKLE=ON`).
  Sparkle needs an EdDSA signing key and an appcast feed; keep it macOS-only.

## 7. Auto-update strategy overall

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

## 8. Versioning, changelog, release cadence

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
  (ref:tidal-sdk-ios/.agents/skills/prepare-release/scripts/check-version-sync.sh — the script is
  nine lines, not four, correcting an earlier draft of this document). tidal-sdk-web does the
  equivalent per-package with `bin/check-version-bump.sh` and a matrix over changed
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
  `cosign|sbom|cyclonedx|spdx` across all 21 checkouts returns no hits, and only tidal-cli
  (`npm publish --access public --provenance`) and tidal-sdk-web (OIDC) use any provenance
  mechanism at all, and only for npm packages — nobody attests a Linux binary. Treat provenance and
  signing as a **no-precedent line item to budget for**, not a copyable pattern: recommend
  GPG-/SSH-signed git tags, Sigstore/cosign keyless signing of every release asset (OIDC from
  GitHub Actions, no key to manage) alongside `checksums.txt`, `actions/attest-build-provenance`
  for provenance, and an SBOM (CycloneDX or SPDX) generated from the same lockfile the license scan
  (`licensing-and-legal.md` §3) already reads — nearly free once a license scanner is wired up, and
  it is what a downstream distro packager or a CRA-style enquiry (`licensing-and-legal.md` §5)
  will ask for.
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
  make an explicit owner decision: a single version for the whole repo, or `core` versioned and
  released independently of the apps — the latter is what makes a permissively licensed core
  (`licensing-and-legal.md` §2) actually adoptable by other clients, which is the stated reason
  for the license split in the first place.

## 9. Headless/daemon packaging deliverables

§1–8 cover Flatpak, Snap, deb/rpm, AUR, MSI, DMG — all GUI. The headless mode, which the owner has
decided ships **now**, has no packaging deliverables specified beyond "ship it via
deb/rpm/AUR/Docker". Three concrete artifacts, all with working precedent:

1. **A systemd `--user` unit.** tidalt templates
   `[Unit] Description=... After=graphical-session.target PartOf=graphical-session.target` /
   `[Service] Type=simple ExecStart={{.Exec}} daemon Restart=on-failure RestartSec=5s` /
   `[Install] WantedBy=graphical-session.target`, installed by `tidalt setup --daemon`
   (ref:tidalt/cmd/tidalt/daemon.go:17-31).
2. **A container image.** `--device /dev/snd` plus
   `--group-add $(getent group audio | cut -d: -f3)` is the documented minimum for ALSA in Docker,
   with `/proc/asound` readable for device discovery and config/state mounted as volumes
   (ref:tidalt/docs/docker.md).
3. **A `.desktop` file registering the `tidal://` URL scheme** — this is what makes "Open in
   desktop app" on tidal.com work: `Exec=tidalt play %u`, `MimeType=x-scheme-handler/tidal;`,
   `Categories=Audio;Music;Player;`, `Terminal=false` (ref:tidalt/cmd/tidalt/tidalt.desktop). Also
   set `X-PulseAudio-Properties=media.role=music`, which tidal-hifi's desktop entry sets
   (ref:tidal-hifi/build/electron-builder.yml). See `testing-strategy.md` §6 item 5 for the
   security implication of registering this scheme.
