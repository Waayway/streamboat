---
name: streamboat-engineering-baseline
description: Stack-agnostic engineering conventions for streamboat, researched and fact-checked against 13+ reference TIDAL-adjacent projects — how to test an unofficial-API client without live credentials in CI (parser/transport split, fixture capture, fuzzing, opt-in live canaries), secret/token storage (OS keyring formats per platform, encrypted-file fallback, embedded client-ID handling, multi-process token races), config/cache/log/telemetry layout and caps, licensing (GPL/LGPL/Apache split, GStreamer/FFmpeg/Qt compatibility, trademark posture), packaging and distribution (Flathub requirements and submission mechanics, Snap, deb/rpm/AUR, AppImage, Windows MSI/winget/signing, macOS DMG/notarization/Homebrew), CI job design, repo layout/CONTRIBUTING/issue-templates/dev-environment, i18n/accessibility, and observability (structured logging, redaction, debug bundles, performance budgets). Use this whenever writing or reviewing a CI workflow (`.github/workflows/*`), a secret/keyring/token-storage module, config/cache/settings-path resolution code, a Flatpak/Snap/deb/rpm/AUR/AppImage/MSI/DMG/winget packaging manifest or script, LICENSE/SPDX/REUSE files, CONTRIBUTING.md/SECURITY.md/CODE_OF_CONDUCT.md/issue templates, docs/legal.md or docs/DECISIONS.md, i18n/gettext/Fluent setup, an accessibility pass, or logging/redaction/debug-bundle/metrics code — or whenever a task mentions Flathub, Flatpak, Snap, AUR, Homebrew, winget, notarization, Trusted Signing, keyring/Secret Service/Credential Manager/Keychain, GPL/LGPL/Apache/AGPL, CRA (Cyber Resilience Act), fixture capture, live canary, fuzzing, coverage target, changelog/SemVer, or "how should streamboat's CI/packaging/licensing/contributing process work." Do not answer these from generic open-source-project conventions — this skill has already fact-checked dozens of specific, easy-to-get-wrong claims (Homebrew's disjunctive-not-conjunctive star threshold, which of the three official TIDAL SDKs run the daily spec-drift check, sone's cache/settings encryption specifics, Flathub's volatile AI-disclosure policy) and the corrections matter more than the surface-level claim.
---

# Engineering baseline for streamboat

Source of truth: `docs/research/engineering-baseline.md` (the full research report — fact-checked
and corrected against two independent passes, ~2,250 lines). This skill is the load-on-demand
distillation for coding agents: the facts, numbers, and pitfalls needed while touching this area,
without re-reading the whole report every time. Read the report itself for full narrative depth
and the complete source list.

`ref:<project>/<path>` throughout this skill and its references means a shallow, read-only git
clone of a named open-source project held in the research environment (e.g.
`ref:sone/src-tauri/src/crypto.rs`) — not part of the streamboat repo, and not guaranteed to still
exist in a later session. Project→URL mapping and how each external source was actually read
(direct fetch, mirror, or search index): `references/sources.md`.

## Owner decisions already made (do not re-litigate these)

- **Platforms now**: desktop Linux + Windows + macOS **and** a headless/server/CLI mode, both now.
  Mobile (Android/iOS) is future scope — the architecture must not preclude it, but nothing
  mobile-specific ships now.
- **Stack**: "something simple but beautiful" — otherwise open; see `docs/research/tech-stack.md`
  and its own skill for the recommendation. Items marked **[STACK]** below are blocked on it.
- **TIDAL API approach**: the unofficial API used by python-tidal/High Tide/sone, requiring the
  user's own paid subscription. streamboat is a player for subscribers, not a downloader/ripper —
  document legal risk honestly; never design or document DRM circumvention or piracy tooling as a
  how-to.
- **Never run `git commit`/`push`/`checkout`/`stash`/`reset`/`clean`** — this applies to any agent
  working in this repo, not just this skill's topic.

## Facts and pitfalls that cause real mistakes

| # | Trap | The fix |
| --- | --- | --- |
| 1 | No reference project uses HTTP cassettes/VCR, and CI cannot use TIDAL's own credentialed-test model (forks get no secrets). | Split every response type into a pure `parse_*`/`TryFrom<Bytes>` function; test it with captured, redacted fixtures; stub the transport layer for retry/refresh/rate-limit behaviour. `references/testing-strategy.md` §1-3. |
| 2 | A headless CI runner has no Secret Service, no unlocked Keychain, no interactive Credential Manager — "test the secret store" is not obvious. | `dbus-run-session -- gnome-keyring-daemon --unlock` on Linux, `security create-keychain`/`unlock-keychain` on macOS, direct Credential Manager API calls on Windows (all headless-safe). `references/secrets-and-tokens.md` §6. |
| 3 | Windows Credential Manager's `CRED_MAX_CREDENTIAL_BLOB_SIZE` (2560 bytes) makes storing a full token blob risky, and whether `CRED_TYPE_GENERIC` is further limited to 512 bytes is disputed. | Don't store the token in the keyring at all — store a fixed 32-byte **key** there (sone's pattern) and keep the encrypted token payload in a file. A 32-byte key never approaches any platform's blob limit. `references/secrets-and-tokens.md` §2. |
| 4 | Two streamboat processes (desktop GUI + headless daemon) refreshing the same TIDAL token concurrently will invalidate each other and silently log the user out. This is now a real scenario, not hypothetical, because both ship at once. | Decide now: the daemon is always the token owner (GUI is a thin client), or the store needs an advisory cross-process lock with re-read-after-lock. `references/secrets-and-tokens.md` §7. |
| 5 | Registering `x-scheme-handler/tidal` (needed for "open in desktop app" from tidal.com) means any web page can invoke `streamboat play <arbitrary string>` pre-authentication — an attacker-reachable surface easy to miss when scoping the fuzzing/security plan. | Fuzz the `tidal://` URI parser against an allowlist of shapes; never pass the raw argument to a subprocess or API call without re-composing it from the parsed id. `references/testing-strategy.md` §6 item 5. |
| 6 | sone does **not** "always write a keyring-backup file even when the keyring succeeds" (an easy claim to copy wrong) — the plaintext-permission 0600 file is written once, at first-run key generation, only. | Get this right if copying sone's encrypted-file-fallback design; the security posture differs materially between "always" and "first run only." `references/secrets-and-tokens.md` §3. |
| 7 | sone encrypts **every cache entry**, not just settings/tokens — the 2 GiB cache cap is 2 GiB of AES-256-GCM blobs, and the cache subsystem cannot start before the master key resolves. | Decide explicitly whether streamboat's cache is encrypted, and state the startup ordering (key resolution gates cache init) either way. `references/config-cache-logs-telemetry.md` §2. |
| 8 | "All three official TIDAL SDKs run a daily API-spec-drift cron" — an earlier draft of this material said "both," which undercounts by one (web, iOS, **and** Android all run it). | Copy the mechanism from any of the three; cite all three, not two. `references/testing-strategy.md` §4. |
| 9 | Homebrew's notability floor ("30 forks / 30 watchers / 75 stars, 90/90/225 for self-submission") reads like a conjunction in slash notation but is **disjunctive** — any one number alone clears the bar. | 225 stars alone is enough for a self-submission; don't over-scope this as a launch blocker. `references/packaging-and-distribution.md` §6. |
| 10 | Flathub's Generative-AI-disclosure policy already flipped once in 2026 (briefly an outright ban, then reverted to disclosure). The quoted policy text in this skill is a dated snapshot. | Add a standing task to re-read `flathub-infra/documentation`'s current policy immediately before any Flathub submission — do not treat the quoted text as permanently settled. `references/packaging-and-distribution.md` §1. |
| 11 | The packaging sequence has no portable Linux binary for the first several months (Flathub needs a dev-history track record; Homebrew needs a 30-day-old repo) — a Silverblue/NixOS/Debian-stable user has nothing to install. | Ship an AppImage (or static tarball for the daemon) as the *first* packaging deliverable, before AUR. `references/packaging-and-distribution.md` §4, `references/decisions-and-sequencing.md` §3. |
| 12 | Azure Trusted Signing pricing/eligibility and winget's "7-day PR auto-close timer" could not be verified from primary Microsoft sources in this research pass (both domains are blocked from the research environment). | Re-verify both from an unblocked network before they drive a cost or submission-process decision. `references/packaging-and-distribution.md` §5. |
| 13 | TIDAL's v1 endpoints take a `locale` parameter separate from `countryCode`; every reference project hardcodes `en_US` (sone does this at 21 call sites, not the ~10 an earlier draft estimated). | Pass the user's real locale — TIDAL's own editorial titles/mixes come back localised for free. `references/i18n-a11y-observability.md` §1. |
| 14 | The GStreamer "for all practical reasons under the GPL itself" quote and the FFmpeg-build-mode guidance are **not** on GStreamer's own licensing FAQ page (verified by direct download+grep) — they're from `gst-plugins-base`'s `LICENSE_readme` and `gst-libav`'s `README.md` respectively. | Cite the correct source file when this claim appears in `docs/legal.md` or a licensing writeup. `references/licensing-and-legal.md` §3. |
| 15 | "`.claude/skills/` is a real convention in this ecosystem" is false — the reference projects use a vendor-neutral `.agents/` directory (tidal-sdk-ios, tidal-sdk-android) or plain `skills/` (tidal-cli); none uses `.claude/`. | A vendor-branded directory name is exactly the kind of detail a Flathub reviewer reads as an AI-tooling signal — be aware of this if minimizing that signal matters. `references/repo-layout-and-docs.md` §2. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| Testing an unofficial-API client without live CI credentials: parser/transport split, fixture capture and redaction, contract tests against the OAS, opt-in live canaries, fuzzing (incl. the `tidal://` handler), audio-pipeline tests, UI tests, the "no secrets, no network" CI rule, coverage strategy, fixture-size policy | `references/testing-strategy.md` |
| Secrets and tokens: what lives where, OS keyrings per platform (Linux/Flatpak/Windows/macOS/headless), sone's encrypted-file envelope format, embedded client-ID handling, what never gets committed, testing secret backends in CI, multi-process token/device ownership | `references/secrets-and-tokens.md` |
| Config/cache/data/log paths per OS, Flatpak/Snap path redirection, cache tiering/TTL/SWR/eviction, settings-file schema versioning and migration, log rotation, telemetry/crash-reporting posture | `references/config-cache-logs-telemetry.md` |
| Licensing: what the references chose, choosing GPL vs LGPL/Apache split for streamboat, GStreamer/FFmpeg/fdk-aac/Qt compatibility notes (with corrected citations), trademark/positioning wording, the EU Cyber Resilience Act determination | `references/licensing-and-legal.md` |
| Packaging and distribution: Flathub (requirements, exact finish-args, metainfo fields, submission mechanics, maintenance/verification), Snap, deb/rpm/AUR (and the missing-AppImage gap), Windows (installer/signing/winget), macOS (DMG/notarization/Homebrew), auto-update strategy, versioning/changelog/release-signing, headless/daemon packaging (systemd unit, container, `.desktop`) | `references/packaging-and-distribution.md` |
| CI job matrix, caching, pre-commit/hook gates, security scanning and dependency policy, reproducible builds, repo governance mechanics (CODEOWNERS, concurrency groups, required-checks list) | `references/ci-and-repo-governance.md` |
| Repository workspace layout, docs/ADR conventions, CONTRIBUTING/code-of-conduct/issue-templates/SECURITY.md, developer-environment setup (Nix flakes, Makefiles, `.editorconfig`) | `references/repo-layout-and-docs.md` |
| i18n (including TIDAL's own `locale` parameter and the mechanics gettext alone doesn't cover), accessibility (keyboard nav, screen readers per platform, contrast/motion), structured logging, network-log redaction, debug bundles, metrics, the local-control-surface security baseline, performance budgets | `references/i18n-a11y-observability.md` |
| The full reference-project comparison table, the 24-item "decide and do now" checklist, the packaging rollout sequence, the stack-blocked items table, the full text of every open decision and unverified item | `references/decisions-and-sequencing.md` |
| Project→URL mapping, license per project, and — critically — **how each external claim was verified** (direct fetch / GitHub mirror / search index / unverified) | `references/sources.md` |

## The one architectural takeaway, if you read nothing else

**Split "fetch bytes" from "turn bytes into a model," and design the CI/security/packaging story
around forks getting no secrets and no live TIDAL account.** Every credential-free test, every
fork-safe CI job, and most of the day-one architecture decisions in this skill exist because of
that one rule. The two most-cited unofficial TIDAL clients (High Tide, tidal-hifi) ship **zero**
automated tests of TIDAL behaviour, and the library everything else depends on (python-tidal) does
not run its test suite in CI at all — streamboat can be materially better here at low cost, because
the API surface is JSON in / typed structs out. See `references/decisions-and-sequencing.md` §1 for
the full comparison table.

## Open decisions

These feed the project's decision tree — only the owner can settle them (full text with sources:
`references/decisions-and-sequencing.md` §5):

1. **AI-assistance posture** for Flathub's disclosure regime — required channel? disclosure
   statement? process keeping commits/PRs human-authored?
2. **License split**: GPL-3.0-only everywhere, or permissive/LGPL `core` + GPL-3.0-only apps
   (recommended, but a one-way door without contributor consent for relicensing).
3. **Contributor agreement**: DCO, CLA, or neither.
4. **Play reporting to TIDAL**: on by default (Recently Played works) or off (strictest privacy)?
5. **Update check on by default** for source/manual installs; off for managed packages — confirm.
6. **Embedded client credentials**: ship an obfuscated default, or require user-supplied only?
7. **Audio buffer cache on disk**: off entirely, or off-by-default with a small cap?
8. **Who pays for Apple Developer Program ($99/yr) and Windows signing**, and are macOS/Windows
   first-class from v1 or best-effort?
9. **Headless control surface**: MPRIS/D-Bus only, or an HTTP/JSON API? If HTTP, sone's
   loopback-bound, token-authenticated, field-sanitized MCP server is the baseline design
   (`references/i18n-a11y-observability.md` §7).
10. **Release cadence**: weekly-ish (sone's pace) or slower/batched?
11. **Translation platform**: self-hosted Weblate, Crowdin, or plain PRs against `po/`?
12. **Minimum supported platform versions** (oldest glibc/Ubuntu, macOS SDK, Windows version).
13. **Release automation tool and monorepo versioning**: pick one appropriate to the chosen stack
    (release-please / changesets / cargo-release+cargo-dist / goreleaser+nfpm), and decide whether
    `core` is versioned/released independently of the apps.

## Unverified

Facts in this skill that could not be confirmed from a primary source in this research pass — do
not present these as settled in specs, code comments, or `docs/legal.md` (full text and sources:
`references/decisions-and-sequencing.md` §6):

- **TIDAL's Terms of Service and Developer Terms** — the entire `tidal.com` domain is blocked from
  the research environment; every quote is second-hand. The "Player module is the only allowed
  playback path" claim applies to the *developer-program* SDK, not necessarily the *unofficial*
  API route streamboat uses — these are separate legal questions (`references/licensing-and-legal.md` §4).
- Terminal playback sub-status list (4005, 4010, 4030–4035) and the ReplayGain formula
  `0.8 * min(10^((rg+4)/20), 1/peak)` — carried from a prior survey, not re-verified against source
  in this pass.
- Whether Windows' `CRED_TYPE_GENERIC` is further limited to 512 bytes (vs the 2560-byte
  `CRED_MAX_CREDENTIAL_BLOB_SIZE`) — measure empirically; sidestepped by storing a key, not a
  payload (`references/secrets-and-tokens.md` §2).
- The exact set of locales TIDAL's `locale` parameter accepts — undocumented anywhere in the
  reference set; determine empirically.
- Qt 6 module-by-module licensing (which modules are GPL-2.0-only vs GPL-3.0-only) — `doc.qt.io`
  is blocked; re-verify per module if Qt is chosen.
- Azure Trusted/Artifact Signing pricing and eligibility, and winget's "7-day PR timer" — both
  Microsoft domains are blocked; secondary-sourced only.
- The Flathub linter's exact wording for a `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID`
  never-granted rule — `docs.flathub.org` is blocked; the *default* MPRIS-ownership policy is
  confirmed from a primary source, only the specific linter-rule text is not.
- Whether Snap's `alsa` interface can ever be requested for auto-connection for a media player —
  not checked against `snapcraft.io/forum`.
