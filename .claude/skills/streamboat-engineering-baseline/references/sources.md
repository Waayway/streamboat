# Sources — project→URL mapping

`ref:<project>/<path>` throughout this skill and the main report
(`/home/user/streamboat/docs/research/engineering-baseline.md`) points at a shallow (`--depth 1`)
git clone of the named GitHub project, held read-only under
the reference checkouts (shallow clones of the cited GitHub projects; see references/sources.md)<project>`
in the research environment — not part of the streamboat repo itself. Clones were taken 2026-09-07.

## Project → URL mapping

| `ref:<project>` | GitHub URL | License | What it is, for this skill's purposes |
| --- | --- | --- | --- |
| `sone` | https://github.com/lullabyX/sone | GPL-3.0-only | Tauri 2 + Rust + React 19 native Linux TIDAL client. Source of the secret-store envelope, cache tiering, log rotation, rate-gate, embedded-credential obfuscation, and the local MCP control-surface design this skill recommends copying. |
| `sone-windows` | https://github.com/lvllaby/sone-windows | GPL-3.0-only | A fork of Sone adding a Windows path; "not inspected in depth" — cited only in the license table. |
| `high-tide` | https://github.com/Nokse22/high-tide | GPL-3.0 | Python + GTK4/libadwaita native Linux TIDAL client. Source of the libsecret token-storage pattern, the accepted Flathub manifest, and the only reference project with real translations. |
| `strawberry` | https://github.com/strawberrymusicplayer/strawberry | GPL-3.0 | C++17 + Qt6 + GStreamer general-purpose music player with TIDAL as one backend. Source of the 13-job build matrix, macOS notarization sequence, opt-in live-canary test pattern, and build-time credential encryption. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | MIT | Electron wrapper around the TIDAL web player. Source of the CONTRIBUTING AI-usage-policy precedent, issue-template shape, and multi-format Linux/Windows packaging via electron-builder. |
| `python-tidal` (`tidalapi`) | https://github.com/EbbLabs/python-tidal (formerly `tamland/python-tidal`) | LGPL-3.0-or-later | The de-facto unofficial-API client library. Source of the credential-store-chain test pattern (`EnvCredentials`→`CachedCredentials`→`KeyringCredentials`) and the "no test job in CI" cautionary example. |
| `mopidy-tidal` | https://github.com/EbbLabs/mopidy-tidal (formerly `tehkillerbee/mopidy-tidal`) | Apache-2.0 | A Mopidy backend for TIDAL. Source of the `pytest_httpserver`+`trustme` HTTP-mock pattern, the 100%-coverage claim, and the credential-free pexpect-driven device-code canary (the strongest precedent for §5's unauthenticated canary — see the note on its two separate workflows below). |
| `tidalt` | https://github.com/Benehiko/tidalt | Apache-2.0 | A Go daemon + Bubble Tea TUI. Source of the daemon/client D-Bus single-instance model, the ALSA hw:/plughw: fallback tests, the pre-commit-hook pattern, the static-FFmpeg build flags, and the systemd/Docker/`.desktop` headless-packaging precedents. |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | MIT | A Node/TypeScript CLI using the official SDK. Source of the npm `--provenance` release pattern and the shipped-skill-as-artifact precedent. |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | Apache-2.0 | TIDAL's official web SDK monorepo. Source of the daily OAS-drift-diff workflow, the credentialed Cypress/unit-test pattern, the manifest-parser fixture format, and CODEOWNERS/concurrency-group conventions. |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | Apache-2.0 | TIDAL's official Android SDK. Source of the `.agents/checks/` review-rule convention, Renovate config, and FOSSA scan-scoping pattern. |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | Apache-2.0 | TIDAL's official iOS SDK. Source of the `.agents/skills/prepare-release/` convention, the changelog-sync CI check, the URLProtocol replay-stub test double, and the pinned pre-commit config. |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | MS-PL | An injector/plugin system for the official TIDAL Electron app. Cited here only for its `locale`/`countryCode`/`deviceType` query-construction pattern — **do not port its code**, it contains a hardcoded stream-decryption key (see `tidal-oss-landscape` skill). |
| `dotnet-tidal-usdk` | https://github.com/SacredSkull/dotnet-tidal-usdk | MIT + explicit anti-piracy clause | Dead (2020) C#/.NET client. Cited as the cautionary example of bolting a use-restriction onto MIT (non-OSI, unpackageable). |
| `tidalrs` | https://github.com/phayes/tidalrs | MIT | A Rust API-client library. Cited only for its `Semaphore`-based concurrent-401-refresh-collapse pattern. |
| `libopentidal` | https://github.com/Fokka-Engineering/libopenTIDAL | MIT | Dead (2021) ANSI C client library. Cited only in the license inventory. |

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
| Flathub AI-policy flip commit | https://github.com/flathub-infra/documentation/commit/992f57b30de98ddbd5e80959e9672998c83c8c97 | Evidence the Generative AI policy briefly banned AI-assisted content in 2026-05 | Direct fetch |
| Flathub linter rules | https://docs.flathub.org/docs/for-app-authors/linter | `finish-args-incorrect-secret-service-talk-name`, `finish-args-own-name-cpt`, `finish-args-x11-without-ipc`, `metainfo-missing-screenshots` | **Egress-blocked** — read via a search index only; the specific `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID` "never granted" rule name is unconfirmed |
| Flatpak sandbox-permissions doc | https://github.com/flatpak/flatpak-docs/blob/master/docs/sandbox-permissions.rst | `--socket=pulseaudio` covers `/dev/snd`; device table; `--persist`; default MPRIS ownership | Direct fetch, verified verbatim |
| xdg-desktop-portal Secret portal spec | https://raw.githubusercontent.com/flatpak/xdg-desktop-portal/main/data/org.freedesktop.portal.Secret.xml | Per-app master secret over a pipe FD | Direct fetch |
| Homebrew Package-Acceptance-Policy | https://github.com/Homebrew/brew/blob/main/docs/Package-Acceptance-Policy.md | Disjunctive 30/30/75, 90/90/225 notability floor; 30-day repo-age rule | Direct fetch |
| Homebrew Acceptable-Casks / Security-and-Supply-Chain | same repo | Gatekeeper audit, quarantine, no SIP/Gatekeeper bypass | Direct fetch |
| Apple Developer Program / Developer ID | https://developer.apple.com/programs/, https://developer.apple.com/support/developer-id/ | $99/year covers Developer ID cert; no separate notarization fee stated | Direct fetch (the "no fee" half is an absence-of-evidence inference) |
| Azure Trusted/Artifact Signing pricing | azure.microsoft.com, learn.microsoft.com | $9.99/$99.99 monthly tiers, eligibility wording | **Egress-blocked** — secondary sources only (devclass 2026-01-14, melatonin.dev, MS Community Hub); re-verify before a cost decision |
| winget submission policy | https://learn.microsoft.com/en-us/windows/package-manager/package/repository | `InstallerSha256`, automated validation, silent-install | **Egress-blocked** — corroborated via `microsoft/winget-pkgs` README/PRs instead; the "7-day PR timer" claim is unconfirmed |
| Windows `wincred.h` / keyring issue | learn.microsoft.com (blocked); AdysTech/CredentialManager#65; jaraco/keyring#355 | `CRED_MAX_CREDENTIAL_BLOB_SIZE` = 2560 bytes | mingw-w64 header copy + GitHub issues, not the primary MS page |
| GStreamer licensing FAQ | https://raw.githubusercontent.com/GStreamer/gst-docs/master/markdown/frequently-asked-questions/licensing.md | LGPL core, `gst-plugins-ugly` for patent plugins | Direct fetch and grep — confirmed this page does **not** contain the "practical reasons under the GPL" or FFmpeg-build quotes some earlier drafts attributed to it |
| gst-plugins-base LICENSE_readme / gst-libav README | GitHub (GStreamer org) | "For all practical reasons under the GPL itself"; FFmpeg build-mode caveat | Direct fetch |
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
