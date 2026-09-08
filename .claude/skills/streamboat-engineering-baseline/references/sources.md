# Sources — project→URL mapping

`ref:<project>/<path>` throughout this skill and the main report
(`/home/user/streamboat/docs/research/engineering-baseline.md`) points at a shallow (`--depth 1`),
read-only git clone of the named GitHub project, held in the research environment's reference
checkouts directory (one subdirectory per project, named `<project>`) — **not** part of the
streamboat repo itself, and not guaranteed to still exist in a later research session. Clones were
taken 2026-09-07.

## Finding which checkout file backs a specific claim

This file maps projects to URLs and licenses; it does not index individual claims to individual
`ref:` citations — every claim in the nine other `references/*.md` files already carries its own
inline `ref:<project>/<path>` (and, where it matters, a line number) at the point the claim is
made, so there is no separate lookup table to keep in sync. To find the citation for a specific
claim: grep the `references/` directory for a distinctive phrase from the claim (e.g. `grep -rn
"AUMID" references/`), then read the `ref:` pointer next to it and, if the checkout still exists at
the path named in "Read-only shallow git clones" (project context), open that file directly. Facts
tables in `SKILL.md` cite the deep-dive reference file and section (`references/secrets-and-tokens.md`
§3, etc.); the reference file itself cites the checkout.

## Project → URL mapping

Commit column: the short SHA each checkout was actually pinned at when read (`git rev-parse
--short=7 HEAD` in each clone) — reproducibility for any `ref:<project>/<path>:<line>` citation
depends on this.

| `ref:<project>` | GitHub URL | Commit | License | What it is, for this skill's purposes |
| --- | --- | --- | --- | --- |
| `sone` | https://github.com/lullabyX/sone | `21494b9` | GPL-3.0-only | Tauri 2 + Rust + React 19 native Linux TIDAL client. Source of the secret-store envelope, cache tiering, log rotation, rate-gate, embedded-credential obfuscation, and the local MCP control-surface design this skill recommends copying. |
| `sone-windows` | https://github.com/lvllaby/sone-windows | `f2009f2` | GPL-3.0-only | A fork of Sone adding a Windows path. Not inspected in depth overall, but its `scripts/prepare-gstreamer.js` + generated NSIS/WiX fragments are the single most consequential Windows-specific finding in the reference set (`packaging-and-distribution.md` §4a) — bundling the GStreamer runtime into the installer, with an embedded developer-local-path bug worth avoiding. |
| `tidalswift` | https://github.com/melgu/TidalSwift | `cf0926b` | not recorded in this skill (macOS-only client) | Cited only for two narrow, code-level facts: it is the only checkout using Git LFS (for README screenshots, not test data — `testing-strategy.md` §11), and its macOS-only scope illustrates why a GPL-3.0-only app cannot ship on iOS (`licensing-and-legal.md` §2). |
| `high-tide` | https://github.com/Nokse22/high-tide | `49f2472` | GPL-3.0 | Python + GTK4/libadwaita native Linux TIDAL client. Source of the libsecret token-storage pattern, the accepted Flathub manifest, and the only reference project with real translations. |
| `strawberry` | https://github.com/strawberrymusicplayer/strawberry | `5d24706` | GPL-3.0 | C++17 + Qt6 + GStreamer general-purpose music player with TIDAL as one backend. Source of the 13-job build matrix, macOS notarization sequence, opt-in live-canary test pattern, and build-time credential encryption. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | `b1326db` | MIT | Electron wrapper around the TIDAL web player. Source of the CONTRIBUTING AI-usage-policy precedent, issue-template shape, and multi-format Linux/Windows packaging via electron-builder. |
| `python-tidal` (`tidalapi`) | https://github.com/EbbLabs/python-tidal (formerly `tamland/python-tidal`) | `9c41fbe` | LGPL-3.0-or-later | The de-facto unofficial-API client library. Source of the credential-store-chain test pattern (`EnvCredentials`→`CachedCredentials`→`KeyringCredentials`) and the "no test job in CI" cautionary example. |
| `mopidy-tidal` | https://github.com/EbbLabs/mopidy-tidal (formerly `tehkillerbee/mopidy-tidal`) | `18abb3b` | Apache-2.0 | A Mopidy backend for TIDAL. Source of the `pytest_httpserver`+`trustme` HTTP-mock pattern, the 100%-coverage claim, and the credential-free pexpect-driven device-code canary (the strongest precedent for §5's unauthenticated canary — see the note on its two separate workflows below). |
| `tidalt` | https://github.com/Benehiko/tidalt | `6cf18c9` | Apache-2.0 | A Go daemon + Bubble Tea TUI. Source of the daemon/client D-Bus single-instance model, the ALSA hw:/plughw: fallback tests, the pre-commit-hook pattern, the static-FFmpeg build flags, and the systemd/Docker/`.desktop` headless-packaging precedents. |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | `24f20a8` | MIT | A Node/TypeScript CLI using the official SDK. Source of the npm `--provenance` release pattern and the shipped-skill-as-artifact precedent. |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | `89244b4` | Apache-2.0 | TIDAL's official web SDK monorepo. Source of the daily OAS-drift-diff workflow, the credentialed Cypress/unit-test pattern, the manifest-parser fixture format, and CODEOWNERS/concurrency-group conventions. |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | `c93bff4` | Apache-2.0 | TIDAL's official Android SDK. Source of the `.agents/checks/` review-rule convention, Renovate config, and FOSSA scan-scoping pattern. |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | `787fe96` | Apache-2.0 | TIDAL's official iOS SDK. Source of the `.agents/skills/prepare-release/` convention, the changelog-sync CI check, the URLProtocol replay-stub test double, and the pinned pre-commit config. |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | `d8cd6bc` | MS-PL | An injector/plugin system for the official TIDAL Electron app. Cited here only for its `locale`/`countryCode`/`deviceType` query-construction pattern — **do not port its code**, it contains a hardcoded stream-decryption key (see `tidal-oss-landscape` skill). |
| `dotnet-tidal-usdk` | https://github.com/SacredSkull/dotnet-tidal-usdk | `fed9e61` | MIT + explicit anti-piracy clause | Dead (2020) C#/.NET client. Cited as the cautionary example of bolting a use-restriction onto MIT (non-OSI, unpackageable). |
| `tidalrs` | https://github.com/phayes/tidalrs | `8bb1de8` | MIT | A Rust API-client library. Cited only for its `Semaphore`-based concurrent-401-refresh-collapse pattern. |
| `libopentidal` | https://github.com/Fokka-Engineering/libopenTIDAL | `89a1eaa` | MIT | Dead (2021) ANSI C client library. Cited only in the license inventory. |

For the full 20+-project landscape (including projects with no local checkout, cited by URL only)
see the `tidal-oss-landscape` skill's own `references/sources.md` — this file lists only the
projects `docs/research/engineering-baseline.md` actually draws code-level evidence from.

**Correction to a claim in an earlier draft**: mopidy-tidal's CI is **two separate workflows**, not
one matrix — `.github/workflows/test.yml` (unit tests, Python 3.12/3.13/3.14 only, Codecov on the
3.13 leg) and `.github/workflows/integration.yml` (the Python × mopidy 3.3/3.4 matrix, no coverage,
runs on push/PR **and** a weekly `cron: "0 0 * * 0"`, executing the pexpect device-code suite). See
`testing-strategy.md` §5 and §10.

## External references (no local checkout — cited by URL, or read via a mirror/search index)

| Reference | URL | Cited for | How it was read |
| --- | --- | --- | --- |
| Flathub app-author requirements | https://docs.flathub.org/docs/for-app-authors/requirements | Console-software ban, dev-history requirement, app ID rules, AI policy | Direct fetch of the source repo `flathub-infra/documentation` (`docs.flathub.org` itself is egress-blocked) |
| Flathub metainfo/submission/maintenance/verification docs | `.../03-metainfo-guidelines/`, `.../05-submission.md`, `.../06-maintenance.md`, `.../10-verification.md` (same repo) | Concrete metainfo fields, submission mechanics, External Data Checker, verification | Direct fetch |
| Flathub AI-policy flip commit | https://github.com/flathub-infra/documentation/commit/992f57b30de98ddbd5e80959e9672998c83c8c97 | Claimed evidence the Generative AI policy briefly banned AI-assisted content in 2026-05 | **Not independently re-verified in this pass** — GitHub API access to this repo's commit history is not enabled here; do not repeat the specific commit/date as settled fact |
| flatpak-builder-lint `finish_args.py` + `exceptions.json` | https://github.com/flathub-infra/flatpak-builder-lint (checks/finish_args.py, staticfiles/exceptions.json) | The real own-name rule ids (`finish-args-unnecessary-appid-own-name`, `finish-args-unnecessary-appid-mpris-own-name`); confirms no rule named `finish-args-own-name-cpt` exists; **now also** the confirmed MPRIS talk-name rule (`finish-args-mpris-flatpak-id-talk-name`) and Secrets-casing rule (`finish-args-incorrect-secret-service-talk-name`), both with zero exceptions granted in the live file | Direct fetch of both files, read in full |
| Flathub linter rules (docs page) | https://docs.flathub.org/docs/for-app-authors/linter | `finish-args-x11-without-ipc`, `metainfo-missing-screenshots` | **Egress-blocked** — read via a search index only; the MPRIS/Secrets talk-name rule names are no longer sourced from here (see the linter source row above, now confirmed by direct fetch instead) |
| High Tide `data/meson.build` | ref:high-tide/data/meson.build:49-77 | `desktop-file-validate`/`appstream-util validate`/`glib-compile-schemas` wired into `meson test`; `i18n.merge_file` merging translated `.desktop.in`/`.appdata.xml.in` at build time | Direct read of the checkout |
| Sone `gen_credentials.py` | ref:sone/scripts/gen_credentials.py | Second credential-generation script that, like `gen_embedded.py`, emits only the A/B pair and omits the C/D PKCE constants | Direct read of the checkout |
| tidalt `docs/media-keys.md` | ref:tidalt/docs/media-keys.md | `playerctl`/MPRIS as the documented Wayland media-key mechanism | Direct read of the checkout |
| Flathub metainfo guidelines (branding/relations) | https://raw.githubusercontent.com/flathub-infra/documentation/master/docs/02-for-app-authors/03-metainfo-guidelines/index.md | `<branding>` color fields; `<requires>`/`<supports>` device-relations block; `<content_rating>` generation requirement | Direct fetch, read 2026-09-08 |
| Flatpak sandbox-permissions doc | https://github.com/flatpak/flatpak-docs/blob/master/docs/sandbox-permissions.rst | `--socket=pulseaudio` covers `/dev/snd`; device table; `--persist`; default MPRIS ownership | Direct fetch, verified verbatim |
| xdg-desktop-portal Secret portal spec | https://raw.githubusercontent.com/flatpak/xdg-desktop-portal/main/data/org.freedesktop.portal.Secret.xml | Per-app master secret over a pipe FD | Direct fetch |
| Homebrew Package-Acceptance-Policy | https://github.com/Homebrew/brew/blob/main/docs/Package-Acceptance-Policy.md | Disjunctive 30/30/75, 90/90/225 notability floor; 30-day repo-age rule | Direct fetch |
| Homebrew Acceptable-Casks / Security-and-Supply-Chain | same repo | Gatekeeper audit, quarantine, no SIP/Gatekeeper bypass, **and the "download cooldown on riskier ecosystems" clause** | Direct fetch |
| Apple Developer Program / Developer ID | https://developer.apple.com/programs/, https://developer.apple.com/support/developer-id/ | $99/year covers Developer ID cert; no separate notarization fee stated | Direct fetch (the "no fee" half is an absence-of-evidence inference) |
| Azure Trusted/Artifact Signing pricing | azure.microsoft.com/en-us/pricing/details/artifact-signing/, learn.microsoft.com/en-us/azure/artifact-signing/faq | $9.99/$99.99 monthly tiers, the org-vs-individual eligibility split (12-country org list vs US/Canada-only individuals), paid-subscription requirement | **Egress-blocked** — search index only; re-verify before a cost decision. **Correction**: an earlier pass inverted the eligibility reading (claimed individual eligibility was not narrower than organisation eligibility) — reverted; individual eligibility genuinely is narrower |
| winget submission policy | https://learn.microsoft.com/en-us/windows/package-manager/package/repository | `InstallerSha256`, automated validation, silent-install | **Egress-blocked** — corroborated via `microsoft/winget-pkgs` README/PRs instead; the "7-day PR timer" claim is unconfirmed |
| winget `doc/Validation.md` | https://raw.githubusercontent.com/microsoft/winget-pkgs/master/doc/Validation.md | `InstallerUrl` HTTPS/official-domain rule, PUA rejection, elevation/silent-install validation, `@wingetbot run` | Direct fetch |
| Windows `wincred.h` primary source | https://raw.githubusercontent.com/MicrosoftDocs/sdk-api/docs/sdk-api-src/content/wincred/ns-wincred-credentiala.md | `CRED_MAX_CREDENTIAL_BLOB_SIZE` = `5*512` = 2560 bytes, quoted verbatim | Direct fetch — this is the actual upstream source for the blocked `learn.microsoft.com` page, not a secondary source |
| mingw-w64 `wincred.h` | https://raw.githubusercontent.com/mingw-w64/mingw-w64/master/mingw-w64-headers/include/wincred.h | **Disagrees** with the Microsoft figure: defines the constant as a bare `512`, no version guard — a toolchain-specific trap, not a corroborating source (an earlier draft of this document mis-cited it as verification for the 2560-byte figure) | Direct fetch, line 114 read in context |
| jaraco/keyring#355 | https://github.com/jaraco/keyring/issues/355 | Observed failures past the Windows blob limit | Direct fetch |
| GStreamer licensing FAQ | https://raw.githubusercontent.com/GStreamer/gst-docs/master/markdown/frequently-asked-questions/licensing.md | LGPL core, `gst-plugins-ugly` for patent plugins | Direct fetch and grep — confirmed this page does **not** contain the "practical reasons under the GPL" or FFmpeg-build quotes some earlier drafts attributed to it |
| GStreamer/gst-libav README | https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-libav/README.md | **Both** the "for all practical reasons under the GPL itself" quote and the FFmpeg build-mode caveat, lines 15-20 — one file, not two, and **not** `gst-plugins-base`'s `LICENSE_readme` (that file does not exist at HEAD in the GStreamer monorepo — a second earlier-draft misattribution) | Direct fetch |
| GStreamer/gst-plugins-base COPYING | https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-base/COPYING | Confirms LGPL-2.1, and confirms no `LICENSE_readme` file exists in this subproject | Direct fetch |
| Fedora Licensing/FDK-AAC wiki | fedoraproject.org (blocked) | fdk-aac is GPL-incompatible, Debian non-free | Search index + tookmund.com, Hydrogenaudio corroboration |
| Qt open-source licensing FAQ | doc.qt.io, qt.io/faq (blocked) | LGPLv3 core, GPL-2.0-only vs GPL-3.0-only module conflict | Search index only — re-verify module-by-module if Qt is chosen |
| SemVer / Keep a Changelog / Conventional Commits | semver.org, keepachangelog.com, conventionalcommits.org | Versioning/changelog/commit conventions | Direct fetch |
| dirs-dev/directories-rs README | https://github.com/dirs-dev/directories-rs | Canonical per-OS config/cache/data/state/runtime path mapping | Direct fetch |
| tidal-api-reference OAS document | https://tidal-music.github.io/tidal-api-reference/tidal-api-oas.json | The OpenAPI document all three official SDKs diff daily | Referenced, not fetched in full |
| tauri-apps/tauri accessibility issues | github.com/tauri-apps/tauri#207, #4315 | Linux webview accessibility risk | Direct fetch |
| openssf.org / orcwg.org CRA pages | https://openssf.org/public-policy/eu-cyber-resilience-act/, https://orcwg.org/cra/ | CRA timeline, open-source-steward duties, Article 64(10) exemption | Direct fetch |
| TIDAL Developer Terms 2.0 | developer.tidal.com/documentation/guidelines-developer-terms-2_0 (blocked, as is all of tidal.com) | "Player module only" playback restriction | Independent web search returning matching quoted text — not a direct fetch; also see the developer-terms-vs-consumer-ToS scope note in `licensing-and-legal.md` §4 |

## Domains blocked from this research environment

`tidal.com` (all subdomains, including `developer.tidal.com`), `docs.flathub.org`,
`azure.microsoft.com`, `learn.microsoft.com`, `doc.qt.io`, `fedoraproject.org`, and
`gstreamer.freedesktop.org` all returned `EGRESS_BLOCKED` during this research pass. Where a claim
from one of these domains could be corroborated through a GitHub-hosted mirror (raw.githubusercontent.com)
or a direct repository fetch, that is noted above as "direct fetch" of the mirror. Where only a
search index or a secondary source was available, that is called out explicitly — treat those
claims as needing a from-scratch verification, not as settled, before they drive a legal, pricing,
or licensing decision.
