# Distribution and packaging

Full source: `docs/research/engineering-baseline.md` §6 (fact-checked). `ref:<project>/<path>`
points at a shallow clone — see `sources.md`.

## Contents

1. Flathub (Linux desktop — the primary channel)
2. Flathub: metainfo requirements, submission mechanics, maintenance and verification
3. Snap
4. deb / rpm / AUR (portable-binary step, Nix as a channel, package smoke tests, FUNDING.yml)
4a. Bundling the media runtime — Windows and macOS have no system GStreamer/FFmpeg
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
  **Treat this policy as unsettled and re-read it before submission, regardless of the history
  below.** The currently quoted text was re-read directly against `flathub-infra/documentation` and
  matches verbatim. A separate claim — that commit `992f57b` (2026-05-29) briefly replaced this
  text with an outright ban ("Applications containing AI-generated or AI-assisted code,
  documentation, or other content are not allowed ... Exceptions may be granted for mature,
  well-maintained projects"), widely reported as a ban, before reverting — **could not be
  independently re-verified in this pass**: GitHub API access to that repo's commit history is not
  enabled for this environment. Do not repeat the specific commit/date as settled fact without
  checking it directly; the safer framing is "this policy has changed before and may change again".
  Treat the quoted text as a dated snapshot (read 2026-09-07 at commit HEAD of
  `flathub-infra/documentation`), add a standing task to re-read it immediately before any Flathub
  submission, and note the adjacent linter rule: sandbox-escape exceptions (`home`, `host`,
  `flatpak-spawn`, arbitrary bus names) are reported as "not... granted if there are signs of LLM
  usage in the software or in the exception PR" — which interacts directly with the permission
  plan below.
- **Named "insecure design" rejection criterion — bears directly on two of this skill's own
  recommendations.** "Applications that include insecure or harmful design choices, such as
  disabling or bypassing security mechanisms, using insecure cryptographic practices, exposing or
  accessing sensitive information, or shipping overly permissive configurations will not be
  accepted." Concretely: never describe the XOR/AES-obfuscated embedded client id (see
  `secrets-and-tokens.md` §4) as "encryption" anywhere a reviewer reads — metainfo, README,
  manifest comments, not just user docs; compile the `--insecure-token-store` plaintext fallback
  (`secrets-and-tokens.md` §2) out of, or hard-disable it in, the Flatpak build; and prefer the
  Secret portal over `--talk-name=org.freedesktop.secrets` (needs no static permission). The same
  doc adds a "Trust and history" clause — submission decisions weigh the submitter's conduct
  across *other* apps too, not just this one.
- **Two carve-outs on rules stated above as absolutes**: the localisation requirement exempts
  inherently region-specific apps and allows an exception "if the author's native language is not
  English and they were unable to find contributors to help"; "building from source" allows a
  case-by-case exception "to well-known vendors ... where the necessary tooling to perform an
  offline source build is not available." Neither changes streamboat's plan (ship English + real
  translations, build from source) but don't overstate either as unconditional to a reviewer.
- **Runtime choice is a standing upgrade obligation, and it decides which codecs exist in the
  sandbox.** "The runtime(s) used in the manifest must be hosted on Flathub and must be the latest
  version at that time of submission" and "Submissions using an end-of-life runtime, extension or
  baseapp will not be accepted" — GNOME/KDE/freedesktop runtimes go EOL roughly yearly (see §2's
  "3+ runtime updates behind" EOL criterion), so this is a recurring release-calendar item, not a
  one-time pick. High Tide pins `org.gnome.Platform`/`org.gnome.Sdk` version 50 and supplies its
  extra pieces (`alsa-utils`, `libportal`, `python3-tidalapi`) as manifest *modules* built from
  source (no network access during build, above). **Zero of the 21 checkouts use
  `org.freedesktop.Platform.ffmpeg-full` or any `add-extensions` block** — no precedent for pulling
  codecs in as a Flatpak extension; if streamboat's decoder set exceeds the runtime's, build those
  decoders as manifest modules too. Decide and record: which runtime, which decoders come from it
  vs. are built in-manifest, and put "runtime version bump" on the release calendar.

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
  matches the Flatpak ID, is an exact subname of it, or is an MPRIS subname. **Correction**: there
  is no linter rule literally named `finish-args-own-name-cpt` — that string is not in
  `flatpak_builder_lint/checks/finish_args.py` (fetched from `flathub-infra/flatpak-builder-lint`
  HEAD, read directly; `cpt` there is an unrelated local variable used for a *different* rule id,
  `finish-args-portal-impl-{cpt}-talk-name`). The real rule ids are
  **`finish-args-unnecessary-appid-own-name`** and **`finish-args-unnecessary-appid-mpris-own-name`**
  (info text: "This is granted by default"); any other own-name gets a dynamic id
  `finish-args-own-name-<bus.name>`. The default sandbox policy already lets an app own
  `org.mpris.MediaPlayer2.$FLATPAK_ID` with no extra finish-arg at all (confirmed by direct fetch
  of `flatpak/flatpak-docs`' sandbox-permissions doc: an app may "own its own namespace named by
  $FLATPAK_ID, subnames of it and org.mpris.MediaPlayer2.$FLATPAK_ID"). **Now confirmed, previously
  flagged unverified**: the linter does carry a specific never-granted rule for the matching
  talk-name too. Direct fetch of `flatpak_builder_lint/checks/finish_args.py` (lines 515-524):
  `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID` emits rule id
  **`finish-args-mpris-flatpak-id-talk-name`** (info: "This is granted by default"), and the live
  `staticfiles/exceptions.json` in that repo has zero entries for it — direct evidence for "never
  granted", independent of the still-blocked `docs.flathub.org` page. Do not request that
  talk-name — the default policy already covers it. (The companion
  `finish-args-incorrect-secret-service-talk-name` rule for the miscased `org.freedesktop.Secrets`
  name is confirmed the same way — see `secrets-and-tokens.md` §2.)
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
- **Two fields the mandatory-fields list omits, both worth getting right day one**: (1)
  `<branding>` — apps "should set a brand color in both light and dark variants":
  `<branding><color type="primary" scheme_preference="light">#ff00ff</color><color type="primary"
  scheme_preference="dark">#993d3d</color></branding>`, used for Flathub/store banners. (2) A
  device-relations block — this is the metainfo half of the mobile-readiness decision
  (`repo-layout-and-docs.md` §1): desktop-only declares `<requires><control>keyboard</control>
  <control>pointing</control><display_length compare="ge">768</display_length></requires>`; adding
  mobile later means moving to `<supports>` with `<control>touch</control>` and
  `display_length compare="ge">360`. Ship the desktop-only `<requires>` form now. Also:
  `<content_rating type="oars-1.1" />` must be *generated* from hughsie.github.io/oars/generate.html,
  not hand-written.
- **Validate packaging metadata in CI, not only in the Flathub build.** High Tide wires three
  validators into its own `meson test`: `desktop-file-validate`, `appstream-util validate`, and
  `glib-compile-schemas --strict --dry-run` (ref:high-tide/data/meson.build:49-77, each
  `find_program(..., required: false)`). Add `appstreamcli validate --explain` (the modern
  successor), `desktop-file-validate` and `flatpak-builder-lint manifest|appstream|repo` as
  explicit `ci-and-repo-governance.md` §1 jobs. Manifest style is mechanically checkable too: JSON
  manifests need RFC-7159 validity, 4-tab indentation, LF endings, UTF-8, a trailing newline,
  double quotes, no trailing commas, comments only inside `"//":`/`"x-comment":` keys; YAML
  manifests need 2-space indentation, blank lines between modules, no vertical value alignment.
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
  one-month maintainer notice. **This is a native alternative to copying Sone's hand-rolled
  `yq`-rewrite-and-PR pipeline** (§1 above) — decide between them rather than assuming the latter.
- **Verification (the checkmark badge)**: an `io.github.*` app ID is verified for free and
  immediately by authenticating as the GitHub repo owner (or an org admin) — another reason to
  take the `io.github.<owner>.streamboat` ID already recommended, and the practical precondition
  for the automerge options above. Domain-based alternatives exist (HTTPS token at
  `https://<domain>/.well-known/org.flathub.VerifiedApps.txt`, DNS TXT record, GitLab variants) but
  are unnecessary here.
- **The app-ID / GitHub-owner choice is permanent — decide it as an explicit ADR before the first
  commit.** `io.github.<owner>.streamboat` is baked into: the Flatpak ID and metainfo `<id>`, the
  Flathub repo name, the D-Bus/MPRIS bus name, the macOS bundle identifier
  (`config-cache-logs-telemetry.md` §1 uses it as the directory name), and — once §5 introduces one
  — the Windows AppUserModelID. Settle two things now: (a) personal account or a new GitHub org —
  moving later costs a Flathub `end-of-life-rebase` plus a user-data migration; (b) check the name
  is free on crates.io/npm/PyPI/AUR/Flathub/Snap Store/winget and as a domain before locking it in.
  Precedent: Sone is `io.github.lullabyX.sone` (personal), High Tide is
  `io.github.nokse22.high-tide` (personal), Strawberry is `org.strawberrymusicplayer.strawberry`
  (owns its own domain) — an org gives a bus-factor-safe path to keeping the verification badge.

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
  media player and what reviewers required — Sone's own README implies the request was either not
  made or not granted, which is itself informative but not conclusive.
- MPRIS needs a **dotless** slot name: `slots: { mpris: { interface: mpris, name: sone } }`, and
  the app must own `org.mpris.MediaPlayer2.<that name>` — Sone detects it via the `SNAP` env var.
- Staging GStreamer requires `layout:` binds for `gstreamer-1.0`, `alsa-lib` and `/usr/share/alsa`,
  plus `GST_PLUGIN_SYSTEM_PATH`.

## 4. deb / rpm / AUR

Two working patterns:

- **Docker-per-distro (Sone).** `build-scripts/build/Dockerfile.{deb,pacman,rpm,rpm-opensuse}` plus
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
as Sone does. Automate with `KSXGitHub/github-actions-deploy-aur@v4.1.3` driven by an
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
references already hit: an AppImage has no stable app identity for the keyring — Sone's own
`crypto.rs` comments that the keyring "may be unreachable on next launch (e.g. AppImage with
different D-Bus session)" — which is exactly why the encrypted-file fallback in
`secrets-and-tokens.md` §3 is mandatory, not optional, for this channel.

**Nix is a fourth, zero-review, day-one channel for exactly the NixOS user above has nothing to
install — and it doubles as a from-source CI build gate.** Three checkouts expose *package*
outputs, not just a dev shell (contrast `repo-layout-and-docs.md` §4, which covers the dev-shell
use only): Sone's `flake.nix` exposes `packages.${system}.sone` / `apps.${system}.default`, and —
worth copying regardless of whether Nix ships as a channel — `checks.${system}.build =
self.packages.${system}.sone`, so `nix flake check` in CI is a full from-source build gate for free
(ref:sone/flake.nix:13-24,47). High Tide's flake exposes `packages.high-tide =
pkgs.python313Packages.buildPythonApplication {...}` (ref:high-tide/flake.nix:87-102). mopidy-tidal
exposes `packages.default` (ref:mopidy-tidal/flake.nix:66). Ship a flake with a package output from
the first release and add `nix flake check` to `ci-and-repo-governance.md` §1's matrix.

**Add a package-install smoke test to CI — a build-only job does not catch a missing runtime
dependency.** Sone's `build-scripts/test/{all,common,deb,rpm,pacman}.sh` install the built package
in Docker per-distro (Ubuntu 22.04/24.04, Debian 12 via `apt-get install -f` — that step validates
declared dependencies are correct and sufficient; archlinux:latest for pacman), start a D-Bus
session and Xvfb, launch the app, and assert: package registered installed, `ldd` reports no "not
found", a window appears within 15 s (`xdotool`), the MPRIS name appears on the bus, GStreamer
device enumeration succeeds, config dir created — plus an AppImage code path (`cd
/tmp/squashfs-root && ./AppRun`). Add this as a CI job on release-candidate tags; the headless-mode
equivalent check is "the control socket/D-Bus name appears", not a window.

**Ship `.github/FUNDING.yml` from day one.** Seven of the 21 checkouts ship one — **correction: an
earlier draft's count (seven) was right but its enumeration named only six** — Sone/Sone-windows
(`patreon: lullabyX`), High Tide (`github: Nokse22` + `ko_fi: nokse22`), TidaLuna
(`github: [inrixia]`), tidal-hifi (`github: [Mastermindzh]` + a PayPal.me custom link), tidalswift
(`github: [melgu]`), and the seventh, `tidal-fokka-engineering-`. Sone additionally declares the
donation link to Flathub itself via `<url type="donation">` alongside
`<url type="bugtracker">`/`<url type="vcs-browser">`
(ref:sone/data/io.github.lullabyX.sone.metainfo.xml).

## 4a. Bundling the media runtime — Windows and macOS have no system GStreamer/FFmpeg

§5-6 below cover installer format, signing and notarization on the assumption the binary is
self-contained. It is not, for any GStreamer/FFmpeg-based stack: Windows and macOS ship neither
runtime, so the installer must carry the entire media runtime plus a bundled-vs-system code path
for plugin/module discovery. This is the single most consequential Windows-specific fact in the
reference set, from the one checkout the main report otherwise declined to inspect in depth
(`sone-windows`).

- **Windows (Sone-windows).** `scripts/prepare-gstreamer.js` generates two packaging artifacts:
  (a) `src-tauri/gstreamer-hooks.nsi`, an NSIS `!macro NSIS_HOOK_POSTINSTALL` copying
  `gstreamer-runtime/*.dll`, `lib/gstreamer-1.0/*.dll` and `lib/gio/modules/*.dll` into `$INSTDIR`
  with a matching uninstall hook; (b) `src-tauri/gstreamer-fragment.wxs`, a WiX fragment with one
  `<Component>`/`<File>` pair per DLL (ref:sone-windows/src-tauri/gstreamer-hooks.nsi,
  ref:sone-windows/src-tauri/gstreamer-fragment.wxs,
  ref:sone-windows/scripts/prepare-gstreamer.js). **Both generated files embed the developer's own
  absolute local path** — a reproducible-build break (`ci-and-repo-governance.md` §5) and an
  incidental username leak; generate these files from a CI-relative path, not a developer machine.
- **macOS (Strawberry).** Deploys with a purpose-built tool: `cmake/Dmg.cmake` does
  `find_program(MACDEPLOYTOOL_EXECUTABLE NAMES ntool)` ("get it from
  https://github.com/jonaski/ntool"), and `src/engine/gststartup.cpp` sets `gst_plugin_scanner` and
  the GIO module search paths to bundle-relative directories at runtime — the app must detect "I am
  running from an app bundle" and repoint GStreamer's plugin/module discovery away from the (absent)
  system locations.
- **Three consequences to design for from the start**: (1) shipping LGPL-2.1 GStreamer DLLs/dylibs
  carries the same relinking obligation `licensing-and-legal.md` §3 already identifies for static
  FFmpeg — publish the exact bundled-runtime build/version list; (2) the app needs a
  bundled-vs-system code path for plugin/module discovery on both Windows and macOS; (3) generate
  packaging fragments (`.wxs`, `.nsi`) from CI-relative paths only, mark them `linguist-generated`
  in `.gitattributes` (`testing-strategy.md` §11).

## 5. Windows

- **Installer format**: tidal-hifi produces an MSI via electron-builder (`win: target: msi`);
  Strawberry ships an NSIS installer (`dist/windows/strawberry.nsi.in` with
  `Capabilities.nsh`, `FileAssociation.nsh`, `Registry.nsh`). MSIX is required only for the
  Microsoft Store and forces a signed package plus a store listing; skip it initially.
- **Signing**: unsigned installers hit SmartScreen. Options in 2026: Azure Trusted Signing /
  Azure Artifact Signing, reported at $9.99/month Basic (up to 5,000 signatures) and $99.99/month
  Premium (up to 100,000 signatures, 10 certificate profiles), versus an EV code-signing
  certificate at roughly $280–500/year on a hardware token. Trusted Signing certificates cannot be
  exported, so losing eligibility means losing the ability to sign. **Eligibility is genuinely
  narrower for an individual than for an organisation — a prior draft's "correction" inverting this
  has itself been reverted.** Per Microsoft's documented eligibility text: Public Trust
  certificates are available to **organisations** in the US, Canada, EU, UK, Australia, New
  Zealand, Japan, South Korea, Singapore, Switzerland, Norway and Israel; **individual developers
  must be located in the US or Canada.** Two more constraints: free/trial/sponsored Azure
  subscriptions are not supported (a paid subscription is required), and individual-developer
  onboarding is reportedly paused, with new organisation customers needing three years of
  verifiable history. **Re-verify before relying on this**: `azure.microsoft.com` and
  `learn.microsoft.com` are blocked from this environment, so all of the above comes from a search
  index over Microsoft's pricing/FAQ pages, not a direct fetch — but do not re-derive the
  "eligibility is not narrower for individuals" reading from those same secondary sources, that
  reading is wrong.
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
  release assets have stable names. **Two more requirements from the primary validation doc, both
  likely first-submission failures if ignored**: `InstallerUrl` must be HTTPS and its domain must
  be an approved official source for the publisher, discoverable by navigating from the publisher's
  own site (`doc/Validation.md`, "Manifest URLs" / step 06; include `PackageUrl` to help a moderator
  confirm this); and a package flagged as a Potentially Unwanted Application "cannot be accepted,
  regardless of the application's legitimacy" (step 07) — worth knowing given streamboat talks to
  an unofficial API and embeds a client credential, either of which a naive heuristic scanner could
  flag. Comment `@wingetbot run` to re-trigger validation after fixing a hash/URL issue.
  (https://raw.githubusercontent.com/microsoft/winget-pkgs/master/doc/Validation.md, read directly.)
- **AppUserModelID (AUMID) and install scope, together, are what make SMTC (Windows media transport
  controls) work — decide both alongside the app ID (§2), not as an afterthought.** The process
  must call the AUMID-setting API at startup (Win32
  `SetCurrentProcessExplicitAppUserModelID` or the stack's equivalent), and the installer must stamp
  the *identical* AUMID onto the Start Menu shortcut — a mismatch is why a media app's transport
  controls silently fail to appear in the Windows media flyout. No reference project sets an
  AppUserModelID at all (grep across all 21 checkouts: zero hits, even `sone-windows`), so this is a
  no-precedent item to specify. Tie it to the `Scope` decision above, since the two install modes
  place the Start Menu shortcut in different roots. Add "SMTC transport controls appear and respond"
  to `i18n-a11y-observability.md` §2's per-OS manual test matrix.
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
  notarization, no Homebrew.** Ship a personal tap in the meantime. **Gap: Homebrew also applies a
  download cooldown that delays release-to-availability latency** — "For ecosystems with a track
  record of fast-moving supply-chain attacks, Homebrew applies a download cooldown: a
  freshly-published upstream version is not adopted immediately"
  (Homebrew-Security-and-Supply-Chain.md). Check whether the cask formula's ecosystem is on that
  list before promising a same-day `brew install` on release day.
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
- **Cadence**: Sone shipped 0.7.0 → 0.21.0 between 2026-03-07 and now, i.e. roughly weekly minor
  releases with dated `<release>` entries in the metainfo
  (ref:sone/data/io.github.lullabyX.sone.metainfo.xml). That pace is realistic for a solo project
  and it keeps the Flathub `<releases>` block meaningful. Note Flathub forbids nightlies and
  "software requiring daily updates" in the stable repo.
- **Every release ships checksums.** tidalt generates `sha256sum * > checksums.txt` and pastes it
  into the release body with verification instructions (ref:tidalt/.github/workflows/release.yml).
  **No reference project verifies its own release artifacts beyond that.** A grep for
  `cosign|sbom|cyclonedx|attest-build-provenance` across all 21 checkouts returns no hits (drop
  `spdx` from that expression — it matches license-header comments, not provenance tooling, in 38
  files across the set). **Correction — provenance and OIDC trusted publishing are different
  mechanisms and only one has real precedent**: only tidal-cli actually attests provenance
  (`npm publish --access public --provenance`, ref:tidal-cli/.github/workflows/release.yml:39).
  tidal-sdk-web declares `id-token: write # Required for OIDC` and uses it for npm trusted
  publishing (short-lived credential exchange) but its publish step explicitly sets
  `NPM_CONFIG_PROVENANCE: "false"` (ref:tidal-sdk-web/.github/workflows/release-to-npm.yml:8,19) —
  i.e. it deliberately disables provenance while using OIDC for auth only. So: one project attests
  provenance, two use OIDC trusted publishing, and only for npm packages — nobody attests a Linux
  binary. Treat provenance and signing as a **no-precedent line item to budget for**, not a
  copyable pattern: recommend
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
- **Gap: pick one file as the single source of version truth, and add a CI check that the metainfo
  `<release>` tracks it — this is the field Flathub and Snap Store actually show users, and the
  one most likely to drift.** Sone derives every packaging version at build time from
  `package.json` (`VERSION=$(node -p "require('./package.json').version");
  craftctl set version="$VERSION"` in snapcraft, ref:sone/snap/snapcraft.yaml:140-141) but the
  metainfo `<releases>` block is hand-maintained — one `<release version="0.21.0">` entry per tag,
  added by hand (ref:sone/data/io.github.lullabyX.sone.metainfo.xml). Specify: (1) one version
  source, every other packaging file (snapcraft, PKGBUILD, debian/changelog, the `.spec`, the
  metainfo) derives from it at build time; (2) a CI job asserting the newest metainfo `<release
  version="...">` equals the git tag being built — same shape as tidal-sdk-ios's `grep -q
  "$(cat ./version.txt)" ./CHANGELOG.md` check above; (3) "add a metainfo `<release>` with
  user-visible notes" is a mandatory release-runbook step below, since a stale `<releases>` block
  makes a weekly cadence look abandoned on Flathub's app page.
- **Gap: write the release procedure as an ordered runbook, and gate `core`'s API with a
  stability check now that it exists as a permissively-licensed target for other clients
  (`licensing-and-legal.md` §2).** Base `docs/releasing.md` on the one working end-to-end
  precedent — tidal-sdk-ios's `.agents/skills/prepare-release/scripts/{bump-version,
  check-release-needed,check-version-sync,extract-release-notes,suggest-changelog}.sh`, invoked by
  CI from those same paths (ref:tidal-sdk-ios/.github/workflows/changelog-check.yml) — as an
  executable sequence: string freeze → bump the version source → derive packaging versions → add
  the metainfo `<release>` → move `[Unreleased]` to a dated section → tag → CI build → checksums +
  signing/attestation → package-install smoke test (§4) → Flathub PR → AUR → winget → announce →
  post-release verification. Separately, add a semver/API-diff gate on `core`
  (`cargo-semver-checks` / api-extractor / japicmp / apidiff — **[STACK]**) plus a written
  deprecation policy; no reference project needs this because the official SDKs sidestep API
  stability by regenerating from the OAS (`testing-strategy.md` §4) rather than hand-writing a
  stable surface — treat it as a no-precedent line item, same category as release signing above.

## 9. Headless/daemon packaging deliverables

§1–8 cover Flatpak, Snap, deb/rpm, AUR, MSI, DMG — all GUI. The headless mode, which the owner has
decided ships **now**, has no packaging deliverables specified beyond "ship it via
deb/rpm/AUR/Docker". Three concrete artifacts, all with working precedent:

1. **A systemd `--user` unit, hardened, plus a second system-level unit for true headless
   deployment.** tidalt's actual template has zero hardening:
   `[Unit] Description=... After=graphical-session.target PartOf=graphical-session.target` /
   `[Service] Type=simple ExecStart={{.Exec}} daemon Restart=on-failure RestartSec=5s` /
   `[Install] WantedBy=graphical-session.target`, installed by `tidalt setup --daemon`
   (ref:tidalt/cmd/tidalt/daemon.go:17-31). Copy the shape, add hardening given streamboat's daemon
   holds a live refresh token and may open a local control port
   (`i18n-a11y-observability.md` §7): `NoNewPrivileges=true`, `ProtectSystem=strict` with
   `ReadWritePaths=` limited to config/cache/state, `ProtectHome=read-only`, `PrivateTmp=true`,
   `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6`, `RestrictNamespaces=true`. Two conflicts to
   resolve explicitly: `MemoryDenyWriteExecute=` must stay **off** if the audio stack uses liborc's
   runtime JIT (the same JIT behind macOS's `allow-jit` entitlement, §6); and a `--user` unit bound
   to `graphical-session.target` never starts on a headless box at all — ship a *second*,
   system-level unit with a dedicated service user, `StateDirectory=`/`CacheDirectory=`/
   `ConfigurationDirectory=streamboat`, and `audio` group membership (same group the container
   guidance below needs). Decide `Type=notify` with a readiness signal vs `Type=simple`, since
   `Restart=on-failure` at a 5 s delay plus a token-refresh storm reproduces the
   `testing-strategy.md` §3 test-2 rate-limit scenario.
2. **A container image.** `--device /dev/snd` plus
   `--group-add $(getent group audio | cut -d: -f3)` is the documented minimum for ALSA in Docker,
   with `/proc/asound` readable for device discovery and config/state mounted as volumes
   (ref:tidalt/docs/docker.md).
3. **A `.desktop` file registering `streamboat://` (not `tidal://`), plus the Windows registry and
   macOS `CFBundleURLTypes` equivalents — this is not a Linux-only deliverable.** **Correction,
   previously said to register `tidal://` here**: `tidal://` is claimed by the official TIDAL
   desktop app itself plus Strawberry, Sone, and High Tide, and OS handler registration is
   last-writer-wins — claiming it would steal it from whichever app the user installed last. Canonical
   rule owned by `tidal-api/references/auth.md` §13: register `streamboat://` as streamboat's own
   scheme; **parse** (not register) `tidal://` content links so pasted links from other apps still
   work; make **claiming** `tidal://` an explicit opt-in setting, off by default. Adapt the reference
   projects' mechanics with the scheme name swapped: the Linux entry shape —
   `Exec=streamboat play %u`, `MimeType=x-scheme-handler/streamboat;`,
   `Categories=Audio;Music;Player;`, `Terminal=false` (pattern from
   ref:tidalt/cmd/tidalt/tidalt.desktop, which uses `tidal` since tidalt does not have this
   collision problem), plus `X-PulseAudio-Properties=media.role=music` (tidal-hifi's entry sets this
   too, ref:tidal-hifi/build/electron-builder.base.yml:40-63). **Cross-platform precedent for the
   registration mechanism** (scheme name still needs swapping to `streamboat`): Sone registers its
   scheme via `plugins.deep-link.desktop.schemes: [...]` in `tauri.conf.json` and
   `app.deep_link().register_all()` at startup (ref:sone/src-tauri/tauri.conf.json,
   ref:sone/src-tauri/src/lib.rs) — on Windows this writes `HKCU\Software\Classes` registry keys at
   runtime, just as reachable from an untrusted web page as the Linux handler. tidal-hifi declares
   one cross-platform block, `protocols: {name: "...", role: "Viewer", schemes: [...]}`, which
   electron-builder expands into both the macOS `CFBundleURLTypes` Info.plist entry and the Windows
   registry entries. The deliverable is three registrations (for `streamboat://`) plus
   second-instance argument forwarding (Sone pairs its deep-link handler with
   `tauri_plugin_single_instance`), and allowlist validation (`testing-strategy.md` §6 item 5) must
   run on the forwarded argument in every one of those entry paths, for both the registered
   `streamboat://` scheme and any accepted-but-unregistered `tidal://` link reaching the app via
   forwarding. **Two more desktop-entry/bundle fields**:
   `StartupWMClass` (must equal the app's real WM class or the taskbar/dock icon never associates
   with the running window) and `StartupNotify=true` (both in tidal-hifi's entry); on macOS,
   `LSApplicationCategoryType` (tidal-hifi uses `public.app-category.entertainment`), needed by the
   DMG/notarization path in §6.
