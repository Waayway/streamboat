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
| **Flathub** | `flatpak-builder` manifest, `org.gnome.Platform` (High Tide uses 50) or `org.freedesktop.Platform` | **Correction, previously stated backwards here**: `--socket=pulseaudio` (as in High Tide's and Sone's shipped manifests) already grants `/dev/snd` — Flatpak's own sandbox helper binds it whenever that socket is requested. Exclusive ALSA `hw:` does **not** need `--device=all`; do not plan for it or expect reviewer pushback on this specific point. Canonical source: `streamboat-engineering-baseline/references/packaging-and-distribution.md` §1. AI policy applies — see §3. Trademark rule in §5 applies directly to TIDAL's marks. |
| **Snap** | `snapcraft.yaml`, `base: core24`, `confinement: strict` | Sone's plugs include `audio-playback` and `alsa`; `alsa` is manual-connect, hence its README's `sudo snap connect sone:alsa`. Sone also overrides the gnome extension's WebKit bind path (it targets `gnome-46-2404`, which ships no WebKit → blank window) — a Tauri-on-Snap-specific trap. |
| **AUR** | PKGBUILD | Sone ships both `sone` (from source) and `sone-bin`. Cheap, expected by Arch users. |
| **deb / rpm** | `tauri build` emits both; Sone additionally builds them in Docker per-distro and hosts an apt/dnf/zypper repo on Cloudsmith | Sone's deb `depends`: `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1`, `libgstreamer1.0-0`, `gstreamer1.0-plugins-{base,good,bad}`, `gstreamer1.0-libav`, `gstreamer1.0-alsa`, `libsecret-1-0`, `libasound2\|libasound2t64`, `librsvg2-common`, `pulseaudio-utils`. **No updater covers deb/rpm** — see `references/tauri-engineering-facts.md` §4. |
| **AppImage** | `tauri build` | Sone sets `appimage.bundleMediaFramework: true` to pull GStreamer in. |
| **Nix** | flake | Both Sone and High Tide ship `flake.nix`; `nix run github:owner/repo` is a zero-install demo path. |
| **Windows** | `msi` (WiX) and `nsis` from `tauri build`; winget manifest on top | If GStreamer is the engine, you must ship it — GStreamer documents packing its MSI and running it silently via `msiexec` with `INSTALLDIR`, or Merge Modules; its docs warn that because plugins load on demand, "if you don't know in advance what files you'll play, you don't know which DLLs you need to deploy" — trimming is risky. MSIX/Store adds a further sandbox that makes WASAPI exclusive harder. See WebView2 install-mode table in `tauri-engineering-facts.md` §5. |
| **macOS** | `app` + `dmg`; Developer ID Application cert; notarization via App Store Connect API key or Apple ID | Hardened Runtime plus a microphone/audio entitlement review is not needed for playback-only, but CoreAudio hog mode inside an App Sandbox is a known problem — plan for Developer ID + notarized DMG, **not** Mac App Store. Homebrew cask is the practical install path. |
| **iOS** | Not App Store — see §4. TestFlight (90-day builds, still reviewed), ad-hoc/enterprise, or EU alternative marketplaces. | |
| **Android** | Sideload APK + F-Droid + (maybe) Play. Play's policy surface for unofficial clients is less absolute than Apple's but still cites third-party ToS. | |

**Cross-compilation does not exist and there is no reference build CI** — see
`references/tauri-engineering-facts.md` §6 for why this means three separate release pipelines, not
one CI matrix.

## §2 Toolkit and reference-project licences

Reference clients are copyleft: Sone and sone-windows GPL-3.0-only; High Tide GPL-3.0; Strawberry
GPL-3.0; python-tidal LGPL-3.0-or-later; mopidy-tidal Apache-2.0; tidal-hifi MIT; tidalt Apache-2.0;
tidalrs, libopenTIDAL, tidal-cli MIT; TidaLuna MS-PL; TIDAL's own SDKs Apache-2.0. If streamboat
reuses *code* from Sone or High Tide it inherits GPL-3.0; reading them for API knowledge does not,
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
  disclosure regime rather than a ban.** Full verbatim text is owned by
  `streamboat-engineering-baseline/references/packaging-and-distribution.md` §1 — cite it rather
  than re-quoting. **The sentence to never omit when summarizing this policy: "Disclosure does not
  create a presumption of acceptance."** — it immediately follows the reviewer-discretion sentence,
  and it's the one that makes "Flathub remains reachable" a conclusion the evidence actually
  supports, rather than an optimistic reading of reviewer-discretion alone.

**Operational consequence for streamboat**: Flathub remains reachable, but (a) the submission PR must
be opened, described and discussed by the human owner, never by an agent; (b) AI involvement must be
disclosed; (c) acceptance is discretionary, explicitly without a presumption in the submitter's
favour. Do not make Flathub the *only* Linux distribution channel — AUR + deb/rpm repo + AppImage +
Snap + Nix (all of which Sone ships) are unaffected by this policy. Also relevant: Flathub packaging
of a Tauri app needs two offline-source generators regenerated every release, and that regeneration
is itself automated in Sone's release pipeline — see `references/tauri-engineering-facts.md` §7 for
why that automation needs an explicit human/bot line drawn under this same policy.

**Sone — the closest precedent — is itself published on Flathub, and its release automation sits
close to the line this policy draws (gap identified during fact-checking).** `ref:sone/README.md:11-14`
carries a Flathub badge for `io.github.lullabyX.sone`; the repo's only GitHub Actions workflow,
`ref:sone/.github/workflows/flathub-update.yml`, is an automated bot that regenerates
`cargo-sources.json`/`pnpm-sources.json` and commits into the Flathub repo on every release. The live
policy prohibits AI tools/agents from "open[ing] or automat[ing] Flathub submission pull requests, or
generat[ing] their commit messages, descriptions, review comments, or replies" — it is **unresolved**
(and not addressed by Flathub's own text) whether that reaches an *automated, non-AI* release bot
updating an *already-published* app's lockfile sources, as opposed to opening a new submission.
Decide this explicitly before copying Sone's release-automation pattern: keep the human owner as the
one who opens and describes any Flathub-facing PR, and ask Flathub directly rather than assuming a
reading either way.

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
- **5.2.3** (verbatim, verified 2026-09-08 at `developer.apple.com/app-store/review/guidelines/`):
  "Apps should not facilitate illegal file sharing or include the ability to save, convert, or
  download media from third-party sources (e.g. Apple Music, YouTube, SoundCloud, Vimeo, etc.)
  without explicit authorization from those sources. Streaming of audio/video content may also
  violate Terms of Use, so be sure to check before your app accesses those services. Authorization
  must be provided upon request." This is the guideline most directly on point for a streaming
  client, separate from 5.2.2's general ToS-authorization requirement — it puts *both* streaming and
  any save/download capability in scope. It bears directly on Open decision #8 (offline caching for a
  logged-in subscriber): whatever streamboat's own on-disk cache design turns out to be, on iOS it
  reads as "download media from a third-party source" under this guideline, independent of whether
  5.2.2's authorization problem is ever solved. It also reinforces streamboat's own posture (see
  SKILL.md's owner-decisions section): a player for subscribers, not a downloader — nothing here
  should be read as a reason to build offline caching as a download/export feature.
- **Unresolved: whether GPL-3.0 itself is independently incompatible with App Store distribution,
  beyond 5.2.2's ToS-authorization problem** (gap identified during fact-checking, not researched in
  this pass). The FSF/VLC's long-standing position is that GPLv2/v3's terms conflict with the App
  Store's usage and installation-information rules, independently of 5.2.2. If that holds, the App
  Store is closed to a GPL-3.0 streamboat for two independent reasons, not one — relevant because Open
  decision #1 (licence) and "iOS is closed" are currently presented as independent conclusions and may
  not be, and because the same reasoning could bear on a future Play Store or alt-marketplace plan.
  Verify against a primary source (the FSF's GPL FAQ / App Store statements, and Apple's current
  terms) before asserting either way; the 5.2.2 conclusion above stands regardless of how this
  resolves.

## §5 Trademark rule

Flathub's requirements prohibit trademark-infringing naming and icons: "A Firefox fork cannot mention
`Firefox` in its name or use any of the official icon, logo or artwork" (lines ~727-728 of the same
requirements doc) — directly applicable to how streamboat may reference TIDAL's own marks, on
Flathub and, by the same logic, everywhere else (App Store 5.2.1 above says the same thing in Apple's
words).
