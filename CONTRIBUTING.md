# Contributing to streamboat

Thanks for helping. A few things are fixed and worth knowing before you start.

## Read the decisions first

`docs/DECISIONS.md` is the log of what the owner decided and why; the
`.claude/skills/streamboat-decisions` skill is its short form. A decision beats a
research recommendation in `docs/research/`. If a change needs to depart from a
decision, open an issue that names the `D-` entry rather than sending the change.

## The line we do not cross

streamboat is a player for TIDAL subscribers. It never decrypts streams, never
requests TIDAL's licensed offline mode, never pools credentials, and never
produces a playable file that outlives a subscription. Encrypted manifests are
refused loudly. Pull requests that move any of this are closed, not reviewed.

## AI-assisted development (disclosure)

Parts of streamboat are developed with AI coding assistants: the research
reports under `docs/research/`, the skills under `.claude/skills/`, and code
throughout the workspace, all reviewed by a human before merge. Every commit
message and every store or package-repository submission is written by a person.
Contributors may use AI tools too; say so in the pull request, keep the commit
message your own, and review what you submit as if you had typed it.

## Workflow

- Trunk-based on `main` with tags; Conventional Commits (`feat(core): ...`).
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace` must pass. CI needs no secrets and no TIDAL account;
  keep it that way (fixtures are synthetic, D-047).
- `crates/streamboat-core/tests/live_canary.rs` (D-046) is the one exception:
  `#[ignore]`d tests against the developer's own TIDAL account, run by hand
  with `STREAMBOAT_LIVE_CANARY=1 cargo test -p streamboat-core --test
  live_canary -- --ignored` against a box that has already run `streamboat
  login`. **Never wire this into CI or any workflow** — it needs a live
  subscription and asserts shape only, on purpose, so it stays safe to run
  against a real account by hand.
- Never commit a client id, client secret, token, or a real subscriber's data.
- Code adapted from Sone, High Tide or Strawberry (GPL-3.0) goes in the GPL
  crates only (`streamboat-player`, `streamboat-server`, `streamboat-desktop`),
  never in the Apache-2.0 `streamboat-core`.
- No contributor agreement is required (D-006).

## Release checklist

`.github/workflows/release.yml` builds and drafts a GitHub Release on every
`vX.Y.Z` tag push (D-041): Linux (deb, rpm, AppImage, AUR-ready tarball),
Windows (MSI), macOS (DMG, arm64 and x86_64), and the `streamboatd` Docker
image (`ghcr.io/waayway/streamboat`). See `packaging/README.md` for what
each artifact is and its known gaps -- most importantly, the Windows and
macOS jobs (libmpv engine, `--no-default-features --features
streamboat-desktop/mpv,streamboat-server/mpv`) have not yet run on a real
Windows or macOS runner; the same invocation is proven on Linux by CI.

The workflow only produces artifacts and a `SHA256SUMS` file. Everything
below it is a human, per D-040 -- **every store or package-repository
submission PR is written and opened by a person, never by CI or an agent**:

1. Confirm `cargo fmt --all -- --check`, `cargo clippy --workspace
   --all-targets -- -D warnings`, `STREAMBOAT_GST_SINK=fakesink cargo test
   --workspace` and `cargo test -p streamboat-player --features mpv` are
   green on the commit being tagged (CI's `check` job runs exactly these).
2. Bump `version` in the root `Cargo.toml` (`[workspace.package]`) and add a
   dated `<release version="...">` entry to
   `packaging/linux/io.github.waayway.streamboat.metainfo.xml` describing
   what changed, in the same commit.
3. Tag `vX.Y.Z` and push the tag; watch the release workflow's jobs.
4. Once the draft release has every expected artifact, write the
   human-authored parts of the release notes yourself (the workflow's own
   note about unsigned binaries can stay) and publish it.
5. **AUR**: update `pkgver`/`sha256sums` in `packaging/aur/PKGBUILD` and
   `PKGBUILD-bin` from the tag and the release's `SHA256SUMS`, and push
   both to their AUR git repos yourself.
6. **winget**: fill in the real `InstallerSha256` in
   `packaging/windows/winget/Waayway.streamboat.installer.yaml` from
   `SHA256SUMS`, and open the `microsoft/winget-pkgs` PR yourself.
7. Flathub, a Homebrew cask: not planned for this release (D-041); revisit
   per `packaging/README.md`.

None of steps 5-7 run in CI, on purpose -- an agent must not open, describe,
or comment on any of those PRs even if asked to "finish the release."
