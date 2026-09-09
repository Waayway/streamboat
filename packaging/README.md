# Packaging

Everything under this directory produces a distributable form of streamboat
(D-041). This file explains what each piece is, what was actually verified
in this repository's own development environment (no Windows or macOS
machine exists there), and the two open items that matter most before any
of this ships for real.

Read `docs/DECISIONS.md` D-040 through D-047 first if you haven't --
channels, code-signing posture and update policy are owner decisions, not
choices made while writing this directory.

## Read this first: a real, pre-existing gap in the Windows/macOS build

D-016 puts GStreamer on Linux and libmpv on Windows and macOS, selected
through one `Engine` trait. The trait and both backends exist
(`crates/streamboat-player/src/gst.rs`, `crates/streamboat-player/src/mpv.rs`),
but **`crates/streamboat-desktop/src/main.rs` and
`crates/streamboat-server/src/main.rs` both construct `GstEngine`
unconditionally** -- there is no `target_os`- or feature-gated path that
picks `MpvEngine` on Windows/macOS yet. `docs/architecture.md`'s "Not yet
built" list already names this ("wiring `MpvEngine` into
`streamboat`/`streamboatd`'s engine selection for Windows and macOS").

Concretely: `GstEngine` is only exported from `streamboat-player` when its
`gstreamer` feature is enabled (`crates/streamboat-player/src/lib.rs`), and
that feature is on by default. Building either binary with
`--no-default-features --features mpv` -- the invocation D-016 implies for
a Windows/macOS release build, and what
`.github/workflows/release.yml`'s Windows and macOS jobs actually run --
fails today with an unresolved `use streamboat_player::GstEngine` import,
before it gets anywhere near needing GStreamer's own C libraries on a
platform that doesn't have them. This is pre-existing, not something this
packaging pass introduced, and it is Rust source, not `Cargo.toml`
metadata -- out of scope for this change to fix. `release.yml`'s Windows
and macOS build steps are marked `continue-on-error: true` with a comment
pointing back here, specifically so a real tag push produces a clearly
failed, clearly explained job instead of a silently broken artifact.

**Fixing this needs**: a small `#[cfg(...)]`-gated engine constructor in
both `main.rs` files (or a shared helper in `streamboat-player`) that picks
`GstEngine` on Linux and `MpvEngine` everywhere else, matching the feature
flags each platform actually builds with below. Surface this to the owner
if it needs to happen before a real release, per `streamboat-decisions`'
"only the owner changes a decision, stop and surface the conflict" rule --
D-016 itself is not in question, only the fact that its last wiring step
isn't done.

## `linux/`

- **`io.github.waayway.streamboat.desktop`** -- the desktop entry (D-007).
  Registers `x-scheme-handler/streamboat` for the `streamboat://` scheme
  (PKCE login callbacks, shareable deep links) and deliberately does
  **not** register `tidal://`: that scheme already belongs to TIDAL's own
  desktop app and to High Tide/Sone, and desktop-file/registry/Info.plist
  handler registration is last-writer-wins everywhere. streamboat still
  parses a pasted `tidal://...` link; it just never claims the scheme.
  Validated locally with `desktop-file-validate` (apt package
  `desktop-file-utils`) -- passes clean.
- **`io.github.waayway.streamboat.metainfo.xml`** -- AppStream metadata
  (D-007). Carries the plain "requires a paid TIDAL subscription; not
  affiliated with TIDAL" summary/description the task asked for, the same
  play-reporting disclosure sentence README.md's "Play reporting" section
  uses, GPL-3.0-only as `<project_license>`, and an explicit placeholder
  `<screenshots>` entry with a comment explaining why: the iced desktop
  shell (D-013) doesn't exist yet, so there is nothing to screenshot.
  **Replace that placeholder with a real, hosted screenshot before any
  store submission** -- Flathub's `metainfo-missing-screenshots` lint rule
  is never waived, even though Flathub itself isn't planned for v1
  (D-041). Validated locally with `appstreamcli validate --no-net` (apt
  package `appstream`) -- passes with two informational (not error) notes
  about `<control>` inside `<requires>`, which is the AppStream-recommended
  form for a desktop-only app (see the device-relations note in
  `streamboat-engineering-baseline/references/packaging-and-distribution.md`).
- **`io.github.waayway.streamboat.svg`** -- the app icon. Original
  artwork: a paper-boat hull and sail with three sound-wave arcs off the
  sail (a boat that streams). Deliberately not TIDAL's own wave mark --
  different silhouette, different motif, no shared colour story beyond
  "blue," which isn't distinctive enough to be a trademark concern on its
  own. Square, scalable, no baked-in shadow, per the AppStream/Flathub icon
  guidelines this metainfo file already follows.
- **`systemd/streamboatd.service`** -- the system unit, for a true
  headless box (D-011): `DynamicUser=yes` plus `SupplementaryGroups=audio`
  for `/dev/snd`, `RuntimeDirectory`/`StateDirectory`/`CacheDirectory`/
  `ConfigurationDirectory=streamboat` (so a `DynamicUser` account still has
  somewhere persistent to keep tokens and queue state), a graceful
  `SIGTERM` shutdown budget, and hardening (`ProtectSystem=strict`,
  `NoNewPrivileges=yes`, a closed `DevicePolicy` that explicitly allows
  only `/dev/snd`, and so on) with two deliberate exceptions called out in
  its own comments: `MemoryDenyWriteExecute=no` (liborc's SIMD JIT needs
  it, the same reason macOS needs `com.apple.security.cs.allow-jit`) and
  no outbound `IPAddressDeny` (the control API and TIDAL/Pushkin traffic
  both need normal networking). Validated locally with
  `systemd-analyze verify` -- passes (with a stub `/usr/bin/streamboatd`
  present, since `verify` also checks that `ExecStart` resolves to a real
  executable).
- **`systemd/user/streamboatd.service`** -- the `systemd --user` unit, for
  a logged-in desktop session or, combined with `loginctl enable-linger`,
  a genuinely headless box that still wants a real D-Bus session bus for
  MPRIS (the system unit's `DynamicUser` account has no session bus at
  all, so MPRIS registration there is best-effort/non-fatal). Also passes
  `systemd-analyze verify --user`.
- **`vendor-gstreamer.sh`** -- see "The vendored GStreamer plan" below.

## `docker/`

- **`Dockerfile`** -- multi-stage: builds on `rust:bookworm` with the
  GStreamer *development* packages, runs on `debian:bookworm-slim` with
  only the GStreamer *runtime* plugins `streamboatd` actually needs (the
  same package list the deb/rpm metadata below uses), as a dedicated
  non-root user, with `/var/lib/streamboat` as a named volume. Listens on
  `127.0.0.1:4747` by default (matching `streamboatd`'s own default); the
  documented override is `--lan` (or `--listen 0.0.0.0:4747`) passed as an
  extra container argument **together with** changing the port mapping --
  both steps are deliberate, the same "logs a warning naming the bind
  address" caution `streamboatd` itself already applies.
- **`compose.yml`** -- the same image as a compose service, with the
  `--device /dev/snd` + `group_add: [audio]` pairing the
  headless-and-tidal-connect daemon-architecture reference documents as
  the minimum for ALSA-in-Docker.

Built locally in this environment; a full `docker build` was not run here
(no Docker daemon in this sandbox -- see the top-level task constraints).
The Dockerfile's own package list was cross-checked against the same
GStreamer plugin set the deb/rpm metadata and CI's `check` job use.

## `aur/`

- **`PKGBUILD`** -- builds the whole Cargo workspace from a tagged source
  tarball, linking Arch's own system GStreamer (`gstreamer`,
  `gst-plugins-{base,good,bad}`, `gst-libav`, `alsa-lib`) -- Arch's
  GStreamer is always at or above D-020's 1.26.10 floor, so this is the one
  Linux channel that needs no vendoring plan at all (D-041). Packages both
  binaries, the desktop entry/metainfo/icon, and both systemd units.
- **`PKGBUILD-bin`** -- the prebuilt counterpart: downloads the per-arch
  release tarball `.github/workflows/release.yml` produces
  (`streamboat-vX.Y.Z-linux-<arch>.tar.gz`) instead of building from
  source. Same runtime `depends` as `PKGBUILD`.

Both pass `bash -n` and a clean `shellcheck -s bash` (with
`SC2034`/`SC2154` disabled at the top of each file -- both warnings fire on
every real-world PKGBUILD, since `pkgver`/`pkgdesc`/`pkgdir`/etc. are read
and written by `makepkg` itself, not by the script). Neither was run
through `makepkg` here (no Arch base and no real tagged release to point
at yet); `sha256sums`/`sha256sums_x86_64`/`sha256sums_aarch64` are
placeholders (`SKIP`) that must be replaced with real hashes before either
file is ever submitted to the AUR -- see CONTRIBUTING.md's release
checklist.

## `windows/`

- **`wix/Package.wxs`** -- a WiX v4 (`http://wixtoolset.org/schemas/v4/wxs`)
  MSI: both binaries, the vendored `libmpv-2.dll`, a Start Menu shortcut,
  and `streamboat://` protocol registration under `HKCU\Software\Classes`
  (per-user install, no UAC prompt) -- never `tidal://`, same rule as the
  Linux desktop entry. No code signing (D-042): the file's own header
  comment says so and points at this README's workaround section.
  Validated locally as well-formed XML (`python3 -c "import xml.dom.minidom"`);
  **never run through the real `wix build`** -- no WiX toolchain and no
  Windows in this environment. The file names the exact build invocation
  and where its `-d` file paths come from in its own header comment, and
  `.github/workflows/release.yml` runs that invocation for real (subject
  to the engine-selection gap above).
- **`winget/`** -- three manifest templates (version, installer, default
  locale) for `Waayway.streamboat`, matching the two requirements most
  likely to fail a first winget submission: `InstallModes`/
  `InstallerSwitches` for a silent/unattended install (`msiexec /quiet` and
  `/passive`, since `InstallerType: msi` already implies msiexec's own
  switch vocabulary), and an explicit `Scope: user` matching the MSI's own
  per-user install. `InstallerSha256` is a placeholder -- fill it in from
  the release's `SHA256SUMS` before ever opening a real
  `microsoft/winget-pkgs` PR, and see this repository's own note in
  `tech-stack-evaluation`/`streamboat-engineering-baseline` packaging
  references about winget's "Potentially Unwanted Application" check: an
  unofficial-API client with an embedded shared client credential is
  exactly the shape a naive heuristic scanner flags, so budget for that
  submission needing a human explanation, not just a clean hash. All three
  files validated locally as parseable YAML.

Two items this directory does **not** resolve, both named in
`streamboat-decisions`' "Still open" list and in `Package.wxs`'s own
header comment: the AppUserModelID that SMTC transport controls need (no
reference implementation anywhere to copy, and the iced shell doesn't
exist yet either), and universal vs. per-architecture install scope.

## `macos/`

- **`Info.plist`** -- a template (`__VERSION__` substituted at build time)
  declaring `CFBundleURLSchemes: [streamboat]` (never `tidal://`, same
  rule again), `LSMinimumSystemVersion: 13.0`, `LSApplicationCategoryType:
  public.app-category.music`, and `NSLocalNetworkUsageDescription` --
  streamboat has no LAN-discovery code yet (Snapcast multiroom output is
  still future work per `docs/DECISIONS.md`), but macOS 15's Local Network
  privacy silently empties any future Bonjour/mDNS discovery without this
  key present, so it ships now rather than needing a second bundle-identity
  review later. Validated locally as well-formed XML.
- **`build-dmg.sh`** -- assembles `streamboat.app` (both binaries in
  `Contents/MacOS/`, the vendored libmpv dylib in `Contents/Frameworks/`,
  `install_name_tool`-relinked to `@executable_path/../Frameworks/...`),
  applies an ad hoc `codesign` (satisfies arm64's "must carry some
  signature to run at all" requirement -- **not** a Developer ID signature
  and does nothing for Gatekeeper, D-042), and wraps it in a DMG with
  `hdiutil` -- no `create-dmg`/Homebrew dependency, only tools macOS itself
  ships. Passes `bash -n` and a clean `shellcheck`; its argument-parsing
  path was smoke-tested on Linux (fails immediately and clearly with
  "required tool not found: hdiutil," since this sandbox has no macOS
  tools at all) -- the assembly/relink/DMG logic itself has never run for
  real.

## `appimage/`

- **`AppImageBuilder.yml`** -- an `appimage-builder` recipe (not
  `linuxdeploy`, since `appimage-builder`'s own `apt` bundling step pulls
  GStreamer's plugin packages and their real dependency closure the same
  way the Docker image's runtime stage does, rather than needing a
  separate GStreamer-specific plugin binary). `${VERSION}` is a plain
  placeholder substituted with `sed` before the real tool ever reads the
  file, the same substitute-before-use approach `Info.plist` and the WiX
  `-d` variables use, kept deliberately free of any custom YAML tag so
  `python3 -c "import yaml"` (this task's own validation bar) can parse it
  as-is. Validated locally as parseable YAML; never run through the real
  `appimage-builder` (needs a mounted apt sandbox and FUSE, and a
  meaningful test needs real GStreamer plugin packages resolved against a
  live Ubuntu mirror -- more than this sandbox's `bash -n`/`python3 -c
  "import yaml"` bar covers). `.github/workflows/release.yml` runs it for
  real on the x86_64 leg; the aarch64 leg logs a warning and skips it for
  now rather than guessing at an arm64 apt-sources config that has never
  been tried -- a genuine follow-up, not an oversight.

## deb / rpm: verified locally, and why they depend on distro GStreamer for now

`[package.metadata.deb]` and `[package.metadata.generate-rpm]` sections
live in `crates/streamboat-desktop/Cargo.toml` and
`crates/streamboat-server/Cargo.toml` (the two binary crates, D-045) --
not a separate packaging manifest, so the metadata can't drift from the
crate it describes. Both were built and inspected for real in this
environment:

```
cargo install --locked cargo-deb cargo-generate-rpm
cargo build --release -p streamboat-desktop -p streamboat-server
cargo deb -p streamboat-server --no-build --no-strip
cargo deb -p streamboat-desktop --no-build --no-strip
cargo generate-rpm -p crates/streamboat-server --target-dir "$PWD/../../target" -o /tmp/rpm-out/
cargo generate-rpm -p crates/streamboat-desktop --target-dir "$PWD/../../target" -o /tmp/rpm-out/
```

Confirmed, with `dpkg-deb -c`/`-I` and (after `apt-get install rpm`)
`rpm -qlp`/`-qRp`/`--scripts`:

- `streamboatd_0.0.1-1_amd64.deb` installs `usr/bin/streamboatd`, both
  systemd units (`usr/lib/systemd/system/streamboatd.service`,
  `usr/lib/systemd/user/streamboatd.service`), and generates real
  `postinst`/`prerm`/`postrm` maintainer scripts that enable and start
  *only* the system unit on install and stop/disable it on removal --
  `cargo-deb`'s systemd integration needs `maintainer-scripts` pointed at
  a real directory to generate those scripts at all (see the comment next
  to that field in `streamboat-server/Cargo.toml`; without it, the unit
  gets packaged but never actually enabled, which was the first thing this
  pass got wrong and then fixed).
- `streamboat_0.0.1-1_amd64.deb` installs `usr/bin/streamboat`, the
  desktop entry, metainfo and icon.
- Both `Depends` lines combine `cargo-deb`'s automatic
  `dpkg-shlibdeps`-based detection (`libgstreamer1.0-0`, `libasound2t64`,
  `libglib2.0-0t64`, `libc6`, ...) with an explicit, hand-written list of
  the GStreamer *plugin* packages (`gstreamer1.0-plugins-{base,good,bad}`,
  `gstreamer1.0-libav`, `gstreamer1.0-alsa`) -- those are loaded by
  GStreamer's own registry scan at runtime (`dlopen`, not a direct link),
  so `dpkg-shlibdeps` cannot see them; they have to be named by hand,
  mirroring exactly what `.github/workflows/ci.yml`'s `check` job installs
  and has verified actually runs the workspace's own test suite.
- The generated `.rpm`s carry the equivalent explicit `requires` (Fedora's
  package names: `gstreamer1`, `gstreamer1-plugins-{base,good}`,
  `gstreamer1-plugins-bad-free`, `gstreamer1-libav`, `alsa-lib`) alongside
  `cargo-generate-rpm`'s own automatic `ldd`-based detection, plus
  `post_install_script`/`pre_uninstall_script`/`post_uninstall_script`
  scriptlets that call `systemctl` directly -- `cargo-generate-rpm` builds
  the package with the `rpm` crate rather than driving `rpmbuild`, so the
  usual `%systemd_post`/`%systemd_preun` spec-file macros were never going
  to be available to expand here.

### Why deb/rpm depend on the distro's GStreamer for now, not the vendored `/opt/streamboat` tree

D-041 says deb/rpm packages vendor a private, pinned GStreamer tree under
`/opt/streamboat`, Chrome-style, rather than linking the distro's own. This
first cut's `depends`/`requires` lines point at the distro's GStreamer
packages instead. `packaging/linux/vendor-gstreamer.sh` is the real,
complete script for building that vendored tree -- it is just not wired
into any packaging job yet. Its own header comment gives the full
reasoning; briefly:

1. D-020's floor is GStreamer >= 1.26.10. Every currently-supported
   Debian/Ubuntu/Fedora release the deb/rpm metadata here targets already
   clears that (the one exception -- this environment's own Ubuntu 24.04
   apt mirror still resolves `gstreamer1.0-plugins-*` at 1.24.2 -- is the
   same below-the-floor case `docs/architecture.md`'s `dashdemux2` demotion
   already handles at runtime, not a new problem this packaging pass
   introduces). The correctness problem the vendored tree exists to solve
   does not actually bite the distros this first cut targets.
2. A vendored tree makes streamboat, not the distro, responsible for
   GStreamer's own security updates -- a real, ongoing cost that should be
   taken on deliberately, with the vendoring script actually wired into a
   release job, not shipped silently ahead of that decision.
3. `streamboat-decisions`' own "Still open" list already flags "whether
   deb/rpm vendoring of GStreamer lives under `/opt/streamboat` or the
   AppImage is the recommended install path on Debian stable" as
   undecided -- building the vendored tree for real is the input that
   decision needs, not something to ship ahead of it.

When a maintainer does need it (a target distro's GStreamer genuinely
falls below 1.26.10, or that "Still open" question resolves toward
vendoring), `vendor-gstreamer.sh --version 1.26.10 --dest dist/vendor-gstreamer/`
fetches, verifies against published checksums, and builds
`gstreamer`/`gst-plugins-{base,good,bad}`/`gst-libav` with Meson into that
prefix, deliberately with `-Dexamples=disabled -Dtests=disabled
-Ddoc=disabled` and the narrow plugin set D-020 names -- not "build
everything." It was not run for real here (it fetches real upstream
tarballs; running it just to prove the script parses would mean a large,
pointless network fetch in this sandbox) -- it passes `bash -n` and a
clean `shellcheck`.

## Unsigned-binary workarounds (D-042)

No code signing anywhere in this repository's packaging or release
automation. Concretely, on first launch:

- **Windows**: SmartScreen blocks the installer with "Windows protected
  your PC." Click **More info**, then **Run anyway**.
- **macOS**: Gatekeeper refuses to open the unsigned, ad hoc-signed
  `streamboat.app`. Either right-click (or Control-click) the app and
  choose **Open** from the menu, confirming the dialog that follows, or
  remove the quarantine attribute from a terminal:

  ```
  xattr -d com.apple.quarantine /Applications/streamboat.app
  ```

Both are one-time per install, not per launch. Revisit before a 1.0
release (D-042); Azure Trusted Signing eligibility and the Apple Developer
Program membership are the two costs to budget then.

## No Flathub in v1

D-041: not planned for the first release. `io.github.waayway.streamboat.metainfo.xml`
is written to Flathub's own metainfo schema anyway (it costs nothing extra
and keeps the door open per D-040's disclosure posture), but nothing here
opens a Flathub submission, automates one, or assumes the repository has
the "meaningful development history" Flathub's own requirements ask for
yet.
