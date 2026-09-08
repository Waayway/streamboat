# Packaging, distribution, updates, and the decisions they force

Full narrative: `docs/research/oss-landscape.md` §2.7, §4.5, §18-A/E/G, Implications 25-26 and
28-32. Sone's packaging scripts are the best single reusable artifact in the whole set — this
file collects the paths, the actual Flathub-manifest facts (fetched during the fact-check pass),
and the decisions those facts force that the original research pass had not surfaced.

## Table of contents

1. Reusable packaging scripts and configs (with paths)
2. Sone's actual Flathub manifest — the Flatpak/bit-perfect conflict is real, not hypothetical
3. The update-mechanism decision, per channel
4. The GStreamer-bundling licensing decision — now with a concrete Windows plugin list
5. The i18n decision
6. Licensing table (incl. the licence-vs-audio-module sequencing decision)
7. Code signing and notarization — a hard prerequisite with no precedent to copy
8. macOS: the full five-item work list

## 1. Reusable packaging scripts and configs (with paths)

- `ref:sone/build-scripts/build/{deb,rpm,pacman,all}.sh` + `Dockerfile.{deb,rpm,rpm-opensuse,
  pacman}` — four-format Linux packaging from one Tauri build.
- `ref:sone/build-scripts/build/PKGBUILD` — an Arch package that repacks the built `.deb` with
  `ar x sone.deb` then `tar xf data.tar.* -C "$pkgdir"` — one build artifact, four distro formats.
- `ref:sone/build-scripts/test/{all,common,deb,pacman,rpm}.sh` — per-format install smoke tests.
- `ref:sone/src-tauri/tauri.conf.json` — complete `.deb`/`.rpm` dependency lists for a
  GStreamer+WebKitGTK+libsecret+ALSA app (`webkit2gtk-4.1`, `gtk3`, ayatana appindicator,
  `gstreamer1.0` base/good/bad/libav/alsa on Debian, `alsa-lib` on the rpm list instead, plus
  `libsecret`, `libasound2`, `librsvg`, `pulseaudio-utils`), plus `appimage.bundleMediaFramework:
  true`. Window config: `decorations: false` (custom titlebar), `visible: false` at startup,
  `csp: null` (avoid this one — set a real CSP), deep-link scheme `tidal`.
- `ref:sone/flake.nix` + `ref:sone/nix/package.nix` — a Nix package + devShell wiring
  `GST_PLUGIN_SYSTEM_PATH_1_0` across gstreamer/base/good/bad/libav.
- `ref:sone/snap/snapcraft.yaml` — `core24`, strict confinement, with a documented
  `snap connect sone:alsa` step for exclusive output.
- `ref:sone/data/io.github.lullabyX.sone.{desktop,metainfo.xml}` — Freedesktop metadata.
- `ref:sone/.github/workflows/flathub-update.yml` — the *only* CI workflow in the repo; automates
  opening the Flathub PR on a `v*` tag (validates the tag regex, dereferences annotated tags to a
  commit SHA). **This is the entirety of Sone's CI** — see `sone-deep-dive.md` §6/§7 for why that
  matters for streamboat's own CI plan.
- `ref:sone/sync-version.mjs` — single-source version sync across `tauri.conf.json`, `Cargo.toml`,
  `PKGBUILD`, and the AppStream metainfo.
- `ref:sone-windows/scripts/prepare-gstreamer.js` — scans a GStreamer runtime directory for DLLs
  and generates both an NSIS macro (`NSIS_HOOK_POSTINSTALL`) and a WiX fragment. Directly reusable
  for any GStreamer-on-Windows app, independent of anything else borrowed from Sone.
- `ref:high-tide/build-aux/io.github.nokse22.high-tide.json` + `python3-tidalapi.json` — a
  working Flatpak manifest (GNOME 50) with vendored Python wheels.
- `ref:tidalt/docker-bake.hcl` + `ref:tidalt/packaging/` — multi-arch deb/rpm/pkg.tar.zst via
  Docker Bake.
- `ref:tidal-hifi/build/electron-builder.*.yml` — deb/rpm/snap/pacman/win/mac configs, invoked by
  named npm scripts (`build-deb`, `build-rpm`, `build-snap`, `build-arch`, `build-win`,
  `build-mac`) — a useful checklist of packaging targets and desktop-entry fields, and backed by
  real CI (`ref:tidal-hifi/.github/workflows/{build,release}.yml`) unlike Sone's.

## 2. Sone's actual Flathub manifest — `--socket=pulseaudio` already grants raw ALSA

The main report originally listed "Sone's actual Flathub manifest" as an open question, because
it lives in a separate `flathub/io.github.lullabyX.sone` repo, not in the `ref:sone` checkout.
The fact-check pass fetched it directly:
https://raw.githubusercontent.com/flathub/io.github.lullabyX.sone/master/
io.github.lullabyX.sone.yml (2026-09-07).

It builds against `org.gnome.Platform` **50** with the Rust and Node 24 SDK extensions, one
`sone` module (simple buildsystem) from the v0.21.0 tag. `finish-args` are exactly:
`--socket=wayland`, `--socket=fallback-x11`, `--socket=pulseaudio`, `--device=dri`,
`--share=ipc`, `--share=network`, `--talk-name=org.kde.StatusNotifierWatcher`,
`--env=WEBKIT_DISABLE_COMPOSITING_MODE=1`.

**Correction (this file previously claimed the opposite — verify against
`streamboat-engineering-baseline/references/packaging-and-distribution.md` §1, which owns this
fact): `--socket=pulseaudio` already grants `/dev/snd`, so raw ALSA `hw:` works in this manifest
as shipped.** Flatpak's own `common/flatpak-run-pulseaudio.c` (`flatpak/flatpak` upstream, `main`)
binds `/dev/snd` into the sandbox whenever `--socket=pulseaudio` is requested and `/dev/snd`
exists on the host — the function's own comment says "since the practical permission of ALSA and
PulseAudio are essentially the same... we reinterpret pulseaudio to also mean ALSA." There is no
separate `--device=all` requirement for exclusive/bit-perfect ALSA output, and Sone's manifest
does not need one. High Tide's manifest is the same shape (PulseAudio socket + read-only
PipeWire) and is equally able to open `hw:` devices.

What genuinely is absent from this manifest is `--device=all`, so *non-audio* device access
(e.g. `/dev/dri` beyond the explicit `--device=dri` grant, or arbitrary `/dev/*` nodes) is not
available — but that was never what bit-perfect ALSA needed. Don't reintroduce the "Flathub
can't do exclusive ALSA" framing; it does not match the sandbox source.

## 3. The update-mechanism decision, per channel

Sone ships **no auto-updater at all** (`sone-deep-dive.md` §5): `check_for_update` only compares
the latest GitHub release tag to the running version and links out to the release page — no
download, no install, no signature check. Combined with having no build/test/lint CI either
(§1 above), Sone's entire release process is manual.

**This generalizes to the whole reference set, not just Sone — treat "no in-app updater" as the
validated default, not an open question.** `tidal-hifi` — the project with the best release CI in
this set — also ships no updater: `package.json`'s only build script is `electron-builder
--publish=never`, there is no `electron-updater` dependency, and its release workflow only
uploads artifacts. High Tide and Sone's own Flathub build delegate updates to the package manager
entirely; tidalt ships distro packages plus a Docker image with no self-updater. **Zero of the
projects examined implement a signed in-app download-and-install updater.**

This forces an explicit, per-channel decision for streamboat, not a single yes/no:

- **Flatpak, Snap, AUR, Nix, and any Linux distro package**: the package manager owns updates.
  **Never** offer an in-app self-updater on these channels — it will conflict with (or silently
  duplicate) what the package manager already does, and on Flatpak/Snap it may not even have
  filesystem permission to replace itself.
- **A self-contained bundle** (AppImage, a Windows installer, a macOS `.app`/`.dmg` outside the
  Mac App Store): an in-app updater is the right call *if* streamboat wants one, but it needs a
  signing keypair, a hosted manifest (a `latest.json`-shaped file, e.g. via
  `tauri-plugin-updater` if the Tauri route is chosen), and CI that actually produces signed
  build artifacts — none of which exists yet anywhere in the reference set. Check-and-notify-only
  (Sone's current behaviour) is a legitimate, lower-effort default until that pipeline exists.

## 4. The GStreamer-bundling licensing decision

If streamboat uses GStreamer (`audio-engineering.md` §5 covers the technical case for and against
this choice), bundling it carries a licensing decision the original research pass had not priced.
`gstreamer1.0-libav` — which Sone's own `.deb`/`.rpm` dependency lists include, **dynamically**
(§1 above) — is FFmpeg-derived and covers TIDAL's AAC (HIGH/LOW tier) and Atmos-adjacent
(AC-4/E-AC-3) decode paths.

- A **dynamic distro dependency** (what Sone's Linux packages do) puts the LGPL/FFmpeg-licensing
  obligation on the distro, not on streamboat.
- A **bundled copy** — `ref:sone-windows/scripts/prepare-gstreamer.js` copying DLLs into an
  NSIS/WiX Windows installer payload, or `appimage.bundleMediaFramework: true` on Linux — puts
  LGPL relinking/notice obligations on streamboat directly.

**Partially resolved: Sone-windows already ships a real answer, with a concrete codec-coverage
cost.** `ref:sone-windows/src-tauri/tauri.conf.json` `bundle.windows.wix.componentRefs` lists 47
components, 16 of them GStreamer plugins: `gstadaptivedemux2`, `gstasio`, `gstaudioconvert`,
`gstaudioparsers`, `gstaudioresample`, `gstcoreelements`, `gstdash`, `gstdecklink`, `gstflac`,
`gstisomp4`, `gstplayback`, `gstsoup`, `gsttypefindfunctions`, `gstvolume`, `gstwasapi2`,
`gstwinks`. **No `gstlibav`, no AAC decoder of any kind.** So an LGPL-clean Windows bundle without
FFmpeg is achievable — this is the plugin list — but it can only play FLAC-in-fMP4/DASH; TIDAL's
`HIGH`/`LOW` tiers are AAC (`mp4a.40.2`/`mp4a.40.5`) and would fail to decode. There is no
LGPL-clean AAC decoder in GStreamer at all: `avdec_aac` is FFmpeg-derived, `faad`
(`gst-plugins-bad`) is GPL-encumbered, `fdkaacdec` carries the Fraunhofer FDK licence. **"Exclude
libav" in practice means "drop the lossy tiers on Windows, or ship a differently-licensed decoder
and price that separately."** Note also: the same file's `bundle.linux.deb.depends`/
`bundle.linux.rpm.depends` **do** include `gstreamer1.0-libav`, so Sone's Linux packages get AAC
today and its Windows bundle silently does not — decide this platform difference on purpose
rather than inheriting it.

## 5. The i18n decision

High Tide ships 8 gettext locales through Meson (`po/LINGUAS`: `fr de nl pt_BR es it zh_TW pl`)
— the standard GNOME/Flathub path that gets community translators for free with essentially no
extra engineering. Sone has **no i18n at all**: no locale directory, no i18n library in
`package.json`, no `useTranslation`/`i18n` usage anywhere across 199 TS/TSX files — it is
English-only.

i18n is cheap to bolt on before a component library exists and expensive to retrofit into ~90
components after the fact. "Everything the native TIDAL client does, eventually" plausibly
includes shipping in more than English. **Put this on the owner's decision list explicitly**
(it's in `SKILL.md`'s "Open decisions") rather than letting it default to Sone's English-only
shape by inertia.

## 6. Licensing table

If streamboat vendors or ports code from any reference project, the obligations differ sharply —
reading GPL code and reimplementing its *ideas* is fine; copying it into a non-GPL streamboat is
not:

| Licence | Projects | Obligation if code is copied |
| --- | --- | --- |
| GPL-3.0(-only) | Sone, Sone-windows, High Tide, Strawberry | Copyleft over the whole work |
| Apache-2.0 | mopidy-tidal, official SDKs (web/Android/iOS), tidalt | Permissive |
| MIT | tidalrs, tidal-hifi, tidal-cli, tidal-connect (wrapper only), libopenTIDAL, dotnet-tidal-usdk (+ an explicit anti-piracy clause) | Permissive |
| LGPL-3.0-or-later | python-tidal | Fine to use as a separate library/process; restrictive to statically port |
| MS-PL | TidaLuna | Permissive licence, but the code contains a DRM-circumvention key and is unusable regardless of licence terms |
| None at all | TidalSwift | **All rights reserved by default** — do not copy any code |

**If streamboat is GPL-3.0 itself, this mostly evaporates** for the GPL-licensed reference
projects — a reason to seriously consider GPL-3.0 as streamboat's own licence, independent of any
other factor.

**Sequence this decision before writing the audio module, not at release time — added by the
third fact-check pass.** Of this skill's own "borrow with pointers" lists, nearly everything with
real specialist value is GPL-3.0: Sone's ALSA negotiation (`audio-engineering.md` §1 — the single
largest chunk of specialist effort this skill recommends porting), its `subStatus` taxonomy,
quality cascade, `norm_gain` formula, cache tiers, crypto container, `concat` executor, the queue
model, ETag mutation pattern, rate-limit contract and scrobble threshold added in the third pass;
High Tide's PKCE dialog and keyring-unlock workaround; Strawberry's encryption refusal and
collection schema. Only tidalrs (MIT), tidal-hifi (MIT), tidal-cli (MIT), mopidy-tidal
(Apache-2.0), tidalt (Apache-2.0) and the official SDKs (Apache-2.0) are permissive — and **none
of those carries a bit-perfect output path**. Concretely: **GPL-3.0 streamboat can port Sone's
ALSA module directly**, saving real weeks; **MIT/Apache-2.0 streamboat must reimplement it from
this skill's own documented behaviour** (the format-naming inversion, the probe order, the
promotion table, the `sw_params` fix — all described in prose here, which is a clean-room spec,
not copied expression) — price that as real weeks of work, decided on purpose, not discovered
after the fact.

## 7. Code signing and notarization — a hard prerequisite with no precedent to copy

Never mentioned in the original research pass despite being a hard blocker for the owner's
Windows+macOS-now requirement. On Windows an unsigned installer triggers SmartScreen; on macOS an
unsigned/unnotarized `.app` is refused by Gatekeeper with no obvious user recovery path.

**No project in the reference set signs anything.** A grep for
`notariz|codesign|CSC_LINK|signtool|hardenedRuntime` across `tidal-hifi`'s build configs and
workflows and across `sone-windows/src-tauri/tauri.conf.json` returns nothing; tidal-hifi's
release workflow builds mac/win artifacts and just uploads them
(`actions/upload-artifact`), with `electron-builder --publish=never` as its only build script.
Strawberry sidesteps the problem entirely by making its macOS/Windows binaries **sponsor-only**
(`ref:strawberry/README.md:85`) rather than solving distribution for everyone.

There is nothing to copy here — budget it as new work: an Apple Developer Program membership plus
a `notarytool` CI step for the macOS DMG, and a Windows code-signing certificate (or Azure Trusted
Signing) for the MSI/NSIS installer, before the first Windows/macOS build ships. This is the same
decision surface as §3's updater question — Tauri's own updater needs its own signing keypair,
separate again from OS-level code signing.

## 8. macOS: the full five-item work list (added by the third fact-check pass — no prior pass
assembled this, and the comparison table previously implied Sone-windows covered macOS, which it
does not)

The owner requires macOS now. Assembled from the checkouts, the work splits five ways, and four
of the five have **zero precedent anywhere in this reference set**:

1. **Audio output** — hand-written CoreAudio backend (hog mode via
   `kAudioDevicePropertyHogMode`, format switching via `kAudioStreamPropertyPhysicalFormat`).
   GStreamer's `osxaudiosink` has no exclusive property and `cpal` gives shared mode only
   (`audio-engineering.md` §5-§6), so this is hand-written work under either audio stack.
   CamillaDSP and MPD's `OSXOutputPlugin.cxx` are the external reference implementations
   (`audio-engineering.md` §6).
2. **OS media integration** — MPRIS is Linux-only, SMTC is Windows-only; macOS needs
   `MPNowPlayingInfoCenter`/`MPRemoteCommandCenter`. `souvlaki 0.8.3` (used for SMTC in
   Sone-windows) *does* cover macOS upstream, but Sone-windows declares it only under
   `[target.'cfg(target_os = "windows")'.dependencies]` — verified,
   `grep -rn macos ref:sone-windows/src-tauri/Cargo.toml` returns nothing — so there is no macOS
   media-controls code anywhere in this set to copy, only a library that could be pointed at
   macOS if someone wrote the arm.
3. **Packaging/signing** — DMG plus Apple Developer Program membership and `notarytool` (§7
   above); no precedent in the set — nobody signs anything, on any platform.
4. **GStreamer bundling on macOS** — the framework must be bundled or Homebrew-depended.
   `ref:sone-windows/scripts/prepare-gstreamer.js` solves the equivalent Windows problem (§1
   above); there is no macOS analogue anywhere in the set.
5. **Keyring** — the one item with a direct precedent: the `keyring` crate covers Keychain, and
   `ref:tidal-sdk-ios/Package.swift:47-53` shows the official iOS SDK itself uses KeychainAccess.

**Budget accordingly**: "Tauri is cross-platform" is not evidence that a `#[cfg]`-split GStreamer
sink, an SMTC-only media-controls crate, or a Windows-only bundling script are — none of the four
from-scratch items above is a side effect of picking a cross-platform UI framework.
