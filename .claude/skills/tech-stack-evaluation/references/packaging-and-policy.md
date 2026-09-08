# Packaging per platform, licensing, and policy constraints

This file covers packaging as a **stack-decision input** (bundle targets, sandbox constraints,
policy risk) — for reusable packaging scripts/configs with exact paths and the update-mechanism
decision per channel, see the `tidal-oss-landscape` skill's `packaging-distribution.md`, which
covers the same ground from a "how do I actually build this" angle. This file stays at "which
channels are viable, and what do they cost the stack decision."

## Contents

- §1 Packaging, per platform (table)
- §2 Toolkit and reference-project licences
- §3 Flathub's AI policy — verbatim, corrected
- §4 Apple App Store guidelines — verbatim
- §5 Trademark rule

---

## §1 Packaging, per platform

| Target | Path | Notes / gotchas |
| --- | --- | --- |
| **Flathub** | `flatpak-builder` manifest, `org.gnome.Platform` (High Tide uses 50) or `org.freedesktop.Platform` | Exclusive ALSA needs a wider sandbox than High Tide's (`--socket=pulseaudio`, `--filesystem=xdg-run/pipewire-0:ro`); plan for `--device=all`, which reviewers question. AI policy applies — see §3. Trademark rule in §5 applies directly to TIDAL's marks. |
| **Snap** | `snapcraft.yaml`, `base: core24`, `confinement: strict` | SONE's plugs include `audio-playback` and `alsa`; `alsa` is manual-connect, hence its README's `sudo snap connect sone:alsa`. SONE also overrides the gnome extension's WebKit bind path (it targets `gnome-46-2404`, which ships no WebKit → blank window) — a Tauri-on-Snap-specific trap. |
| **AUR** | PKGBUILD | SONE ships both `sone` (from source) and `sone-bin`. Cheap, expected by Arch users. |
| **deb / rpm** | `tauri build` emits both; SONE additionally builds them in Docker per-distro and hosts an apt/dnf/zypper repo on Cloudsmith | SONE's deb `depends`: `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1`, `libgstreamer1.0-0`, `gstreamer1.0-plugins-{base,good,bad}`, `gstreamer1.0-libav`, `gstreamer1.0-alsa`, `libsecret-1-0`, `libasound2\|libasound2t64`, `librsvg2-common`, `pulseaudio-utils`. **No updater covers deb/rpm** — see `references/tauri-engineering-facts.md` §4. |
| **AppImage** | `tauri build` | SONE sets `appimage.bundleMediaFramework: true` to pull GStreamer in. |
| **Nix** | flake | Both SONE and High Tide ship `flake.nix`; `nix run github:owner/repo` is a zero-install demo path. |
| **Windows** | `msi` (WiX) and `nsis` from `tauri build`; winget manifest on top | If GStreamer is the engine, you must ship it — GStreamer documents packing its MSI and running it silently via `msiexec` with `INSTALLDIR`, or Merge Modules; its docs warn that because plugins load on demand, "if you don't know in advance what files you'll play, you don't know which DLLs you need to deploy" — trimming is risky. MSIX/Store adds a further sandbox that makes WASAPI exclusive harder. See WebView2 install-mode table in `tauri-engineering-facts.md` §5. |
| **macOS** | `app` + `dmg`; Developer ID Application cert; notarization via App Store Connect API key or Apple ID | Hardened Runtime plus a microphone/audio entitlement review is not needed for playback-only, but CoreAudio hog mode inside an App Sandbox is a known problem — plan for Developer ID + notarized DMG, **not** Mac App Store. Homebrew cask is the practical install path. |
| **iOS** | Not App Store — see §4. TestFlight (90-day builds, still reviewed), ad-hoc/enterprise, or EU alternative marketplaces. | |
| **Android** | Sideload APK + F-Droid + (maybe) Play. Play's policy surface for unofficial clients is less absolute than Apple's but still cites third-party ToS. | |

**Cross-compilation does not exist and there is no reference build CI** — see
`references/tauri-engineering-facts.md` §6 for why this means three separate release pipelines, not
one CI matrix.

## §2 Toolkit and reference-project licences

Reference clients are copyleft: SONE and sone-windows GPL-3.0-only; High Tide GPL-3.0; Strawberry
GPL-3.0; python-tidal LGPL-3.0-or-later; mopidy-tidal Apache-2.0; tidal-hifi MIT; tidalt Apache-2.0;
tidalrs, libopenTIDAL, tidal-cli MIT; TidaLuna MS-PL; TIDAL's own SDKs Apache-2.0. If streamboat
reuses *code* from SONE or High Tide it inherits GPL-3.0; reading them for API knowledge does not,
but the safe and community-consistent choice is GPL-3.0.

Toolkit licences: Tauri MIT/Apache-2.0; React/Tailwind MIT; iced MIT; egui MIT/Apache; gpui
Apache-2.0; **`relm4`/gtk4-rs is Apache-2.0 OR MIT** (crates.io's actual licence field — not plain MIT
as an earlier draft said); cxx-qt MIT/Apache (but **Qt itself** is LGPL-3.0 or commercial — LGPL
requires dynamic linking or object-file relinking); Slint tri-licensed (`GPL-3.0-only OR
LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` — the royalty-free arm requires
visible attribution, the GPL arm does not and suits an open-source project); symphonia MPL-2.0; cpal
Apache-2.0; rodio MIT/Apache; gstreamer bindings MIT/Apache with the underlying GStreamer core
LGPL-2.1 and some plugin sets GPL; souvlaki MIT; uniffi MPL-2.0; wasapi MIT. Nothing here blocks a
GPL-3.0 project.

## §3 Flathub's AI policy — verbatim, corrected

**Most consequential 2026 policy change for this project.** Sequence of facts, with confidence
levels:

- **Unverified (fact-checked as "uncertain")**: press reporting from late May 2026 said Flathub
  would "ban nearly all apps and submissions made with generative AI", effective 2026-05-29, covering
  application code, BaseApps, extensions, build scripts, manifests, metadata, documentation and PR
  text, non-retroactive, with exceptions possible for mature well-maintained projects. The source
  domains (gamingonlinux.com, linuxiac.com, opensourceforu.com) were not reachable in this
  environment either time this report was checked.
- **Directly read, high confidence**: commit `flathub-infra/documentation@992f57b` ("Reword LLM
  policy to make it clear it's not allowed") in `docs/02-for-app-authors/02-requirements.md` reads
  "Applications containing AI-generated or AI-assisted code, documentation, or other content are not
  allowed" with "Exceptions may be granted for mature, well-maintained projects." (Its exact commit
  date was not independently visible when fact-checked — "May 2026" is inferred from the press
  timeline, not confirmed from the commit itself.)
- **The live document, read 2026-09-07, section "### Generative AI policy" at line 241, is a
  disclosure regime rather than a ban.** Verbatim (all sentences confirmed directly from
  `https://raw.githubusercontent.com/flathub-infra/documentation/main/docs/02-for-app-authors/02-requirements.md`):

  > "Submitters must disclose any AI-generated code, documentation, packaging, or other material they
  > know or reasonably believe is included in the application or its Flathub packaging."
  >
  > "AI used only for research, discussion, or debugging does not need disclosure when no generated
  > material is included in the application or its Flathub packaging."
  >
  > "Disclosed AI-generated material is evaluated at reviewer discretion."
  >
  > **"Disclosure does not create a presumption of acceptance."** *(the sentence immediately
  > following the one above — easy to omit, and the one that makes "Flathub remains reachable" a
  > conclusion the evidence actually supports, rather than an optimistic reading of the
  > reviewer-discretion sentence alone.)*
  >
  > "Reviewers may reject a submission, including without further review, based on the extent or
  > role of generated material or concerns about its review, quality, or maintainability."
  >
  > "AI tools or agents must not open or automate Flathub submission pull requests, or generate
  > their commit messages, descriptions, review comments, or replies."
  >
  > "Submitters must not request AI-agent reviews."
  >
  > "Undisclosed or materially misrepresented AI-generated material… may result in rejection."
  >
  > "Repeated violations may result in a permanent ban from future submissions and activities."

**Operational consequence for streamboat**: Flathub remains reachable, but (a) the submission PR must
be opened, described and discussed by the human owner, never by an agent; (b) AI involvement must be
disclosed; (c) acceptance is discretionary, explicitly without a presumption in the submitter's
favour. Do not make Flathub the *only* Linux distribution channel — AUR + deb/rpm repo + AppImage +
Snap + Nix (all of which SONE ships) are unaffected by this policy. Also relevant: Flathub packaging
of a Tauri app needs two offline-source generators regenerated every release, and that regeneration
is itself automated in SONE's release pipeline — see `references/tauri-engineering-facts.md` §7 for
why that automation needs an explicit human/bot line drawn under this same policy.

## §4 Apple App Store guidelines — verbatim

- **5.2.2** (verbatim): "If your app uses, accesses, monetizes access to, or displays content from a
  third-party service, ensure that you are specifically permitted to do so under the service's terms
  of use. Authorization must be provided upon request." An unofficial TIDAL client using the
  undocumented API cannot produce that authorization on request. Treat the iOS App Store as closed;
  this does not affect the technical stack choice but does mean "mobile" should be scoped as
  Android-first with iOS via TestFlight/sideload.
- **5.2.1**: "Don't use protected third-party material such as trademarks, copyrighted works, or
  patented ideas in your app without permission, and don't include misleading, false, or copycat
  representations, names, or metadata in your app bundle or developer name."
- **2.5.6**: "Apps that browse the web must use the appropriate WebKit framework and WebKit
  JavaScript. You may apply for an entitlement to use an alternative web browser engine in your
  app" — entitlement scoped explicitly to the EU and Japan. This is why a Tauri iOS build is fine
  (WKWebView) but a bundled Chromium is not.

## §5 Trademark rule

Flathub's requirements prohibit trademark-infringing naming and icons: "A Firefox fork cannot mention
`Firefox` in its name or use any of the official icon, logo or artwork" (lines ~727-728 of the same
requirements doc) — directly applicable to how streamboat may reference TIDAL's own marks, on
Flathub and, by the same logic, everywhere else (App Store 5.2.1 above says the same thing in Apple's
words).
