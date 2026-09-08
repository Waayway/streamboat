---
name: streamboat-engineering-baseline
description: Stack-agnostic engineering conventions for streamboat, researched and fact-checked against 13+ reference TIDAL-adjacent projects — how to test an unofficial-API client without live credentials in CI (parser/transport split, fixture capture, fuzzing, opt-in live canaries), secret/token storage (OS keyring formats per platform, encrypted-file fallback, embedded client-ID handling, multi-process token races), config/cache/log/telemetry layout and caps, licensing (GPL/LGPL/Apache split, GStreamer/FFmpeg/Qt compatibility, trademark posture), packaging and distribution (Flathub requirements and submission mechanics, Snap, deb/rpm/AUR, AppImage, Windows MSI/winget/signing, macOS DMG/notarization/Homebrew), CI job design, repo layout/CONTRIBUTING/issue-templates/dev-environment, i18n/accessibility, and observability (structured logging, redaction, debug bundles, performance budgets). Use this whenever writing or reviewing a CI workflow (`.github/workflows/*`), a secret/keyring/token-storage module, config/cache/settings-path resolution code, a Flatpak/Snap/deb/rpm/AUR/AppImage/MSI/DMG/winget packaging manifest or script, LICENSE/SPDX/REUSE files, CONTRIBUTING.md/SECURITY.md/CODE_OF_CONDUCT.md/issue templates, docs/legal.md or docs/DECISIONS.md, i18n/gettext/Fluent setup, an accessibility pass, or logging/redaction/debug-bundle/metrics code — or whenever a task mentions Flathub, Flatpak, Snap, AUR, Homebrew, winget, notarization, Trusted Signing, keyring/Secret Service/Credential Manager/Keychain, GPL/LGPL/Apache/AGPL, CRA (Cyber Resilience Act), fixture capture, live canary, fuzzing, coverage target, changelog/SemVer, or "how should streamboat's CI/packaging/licensing/contributing process work." Do not answer these from generic open-source-project conventions — this skill has already fact-checked dozens of specific, easy-to-get-wrong claims (Homebrew's disjunctive-not-conjunctive star threshold, which of the three official TIDAL SDKs run the daily spec-drift check, sone's cache/settings encryption specifics, Flathub's volatile AI-disclosure policy) and the corrections matter more than the surface-level claim.
---

# Engineering baseline for streamboat

Source of truth: `docs/research/engineering-baseline.md` (the full research report — fact-checked
and corrected against multiple independent passes, ~2,700 lines). This skill is the load-on-demand
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
| 10 | Flathub's Generative-AI-disclosure policy is a dated snapshot; a claim that it briefly flipped to an outright ban in 2026 (commit `992f57b`) could **not** be independently re-verified in this pass (no GitHub API access to that repo's history here). | Don't repeat the specific commit/date as settled fact. Add a standing task to re-read `flathub-infra/documentation`'s current policy immediately before any Flathub submission regardless. `references/packaging-and-distribution.md` §1. |
| 11 | The packaging sequence has no portable Linux binary for the first several months (Flathub needs a dev-history track record; Homebrew needs a 30-day-old repo) — a Silverblue/NixOS/Debian-stable user has nothing to install. | Ship an AppImage (or static tarball for the daemon) as the *first* packaging deliverable, before AUR — Nix (`flake.nix` with a package output, not just a devShell) is a second, zero-review day-one channel. `references/packaging-and-distribution.md` §4, `references/decisions-and-sequencing.md` §3. |
| 12 | Azure Trusted Signing pricing/eligibility and winget's "7-day PR auto-close timer" could not be verified from primary Microsoft sources in this research pass (both domains are blocked from the research environment). | Re-verify both from an unblocked network before they drive a cost or submission-process decision. `references/packaging-and-distribution.md` §5. |
| 13 | TIDAL's v1 endpoints take a `locale` parameter separate from `countryCode`; every reference project hardcodes `en_US` (sone does this at 21 call sites, not the ~10 an earlier draft estimated). | Pass the user's real locale — TIDAL's own editorial titles/mixes come back localised for free. `references/i18n-a11y-observability.md` §1. |
| 14 | The GStreamer "for all practical reasons under the GPL itself" quote and the FFmpeg-build-mode guidance are **not** on GStreamer's own licensing FAQ page (verified by direct download+grep) — and they're **both from one file**, `gst-libav`'s `README.md`, **not** from `gst-plugins-base`'s `LICENSE_readme` (which does not exist at HEAD in the GStreamer monorepo — a second earlier-draft misattribution). | Cite `gst-libav/README.md` lines 15-20 for both quotes when this claim appears in `docs/legal.md` or a licensing writeup. `references/licensing-and-legal.md` §3. |
| 15 | "`.claude/skills/` is a real convention in this ecosystem" overgeneralises — no checkout uses `.claude/skills/` specifically, but `.claude/` itself is not absent (tidal-sdk-ios ships `.claude/commands/`) and 5 of 21 checkouts (2 of them official SDKs) ship a root `CLAUDE.md`. | The narrower finding still holds — `.agents/skills/` has working CI-invocation precedent, `.claude/skills/` does not — but don't claim "nobody uses `.claude/`". `references/repo-layout-and-docs.md` §2. |
| 16 | The Windows Credential Manager blob-limit claim was previously verified against the **wrong** source — mingw-w64's `wincred.h` actually *disagrees* with the 2560-byte figure (it defines a bare `512`, no version guard), it does not corroborate it. | Cite `MicrosoftDocs/sdk-api`'s `ns-wincred-credentiala.md` for the 2560-byte figure; treat the mingw-w64/MSVC discrepancy itself as a real toolchain trap — measure at runtime on both toolchains if both are shipped. `references/secrets-and-tokens.md` §2. |
| 17 | The xdg-desktop-portal Secret portal's per-app secret is **not** already a usable AES key — the spec says its format is opaque and may be too short, and tells the caller to expand it with a KDF. | Feed the portal secret through HKDF-SHA256 with a fixed `info` string before using it as an AES-256-GCM key; don't assume it behaves like sone's 32-byte keyring value. `references/secrets-and-tokens.md` §2. |
| 18 | Terminal playback sub-statuses are **not** the contiguous range `4030–4035` — `4033` (subscription up-sell) and `4006` (recoverable) are deliberately excluded. Verified list: `[4005, 4010, 4030, 4031, 4032, 4034, 4035]`. | Write the non-contiguous list, not a range, in any retry/terminal-error logic. `references/testing-strategy.md` §3. |
| 19 | GPL-3.0-only on the app is not just a copyleft preference — it forecloses the future iOS/App Store target the project context says the architecture must not preclude. | Keep `streamboat-core` **Apache-2.0** (not LGPL — its relinking requirement is itself contested on iOS), not just "permissive" — this is what keeps a future App Store app possible at all. `references/licensing-and-legal.md` §2. |
| 20 | Neither Windows nor macOS ships GStreamer/FFmpeg — the installer must bundle the entire media runtime plus a bundled-vs-system plugin-discovery code path. This is easy to miss because a dev build "just works" on the developer's own machine (which has the runtime installed separately). | Budget this as a first-week Windows/macOS line item, not a late add-on; see sone-windows's generated NSIS/WiX fragments and Strawberry's `ntool`/`gststartup.cpp` bundle-relative plugin-path rewriting for the concrete shape. `references/packaging-and-distribution.md` §4a. |
| 21 | A GitHub-hosted self-hosted runner for the live-canary/live-test workflow is a real code-execution risk on a public repo (a fork PR can run attacker code on the machine holding a live TIDAL refresh token) — no reference project uses one, and none uses `pull_request_target` either. | Run the live canary from a maintainer-local cron instead; never use `pull_request_target` in this repo. `references/testing-strategy.md` §5. |

## Facts and pitfalls, continued (gaps this skill closes that the original survey missed)

| # | Trap | The fix |
| --- | --- | --- |
| 22 | sone writes its settings/token file non-atomically (`fs::write`, no temp file) even though it uses a proper atomic temp-file→fsync→rename pattern for its *theme* file — an easy one to copy wrong if you generalise from "sone does X" without checking which file. | Write every persisted secret/settings/state file with the atomic pattern from commit one; make a failed magic-header check on the encrypted file a hard typed error once v1 ships, not silent plaintext passthrough. `references/secrets-and-tokens.md` §3. |
| 23 | Nothing in the original report covers logout, account switching, or "delete all my data" — yet sone's cache/settings design makes getting the ordering wrong a real bug (a scrobble sent after logout; account A's cache served to account B). | Implement `logout` in sone's exact ordering (stop reporting before stopping playback, ...) and ship a separate, stronger `streamboat purge` that also clears the keyring entry, key file and logs. `references/secrets-and-tokens.md` §8. |
| 24 | The multi-process token-ownership decision (`secrets-and-tokens.md` §7) doesn't by itself cover the disk cache or any local database — two processes doing LRU eviction against the same cache directory with no coordination will double-evict and corrupt `.meta` sidecars. | Extend the single-decision-not-two-decisions framing to every shared on-disk store, not just tokens. `references/secrets-and-tokens.md` §7. |
| 25 | winget's silent-install and `Scope` requirements are necessary but not sufficient — `InstallerUrl` must also be HTTPS from an approved official domain, and a package flagged as a Potentially Unwanted Application is rejected "regardless of the application's legitimacy," which a naive scanner could do to an unofficial-API client. | Read `doc/Validation.md`'s "Manifest URLs" section before the first winget submission; comment `@wingetbot run` to re-trigger after a fix. `references/packaging-and-distribution.md` §5. |
| 26 | No reference project sets a Windows AppUserModelID — so "SMTC doesn't show transport controls" is a silent, no-precedent failure mode, not a copyable bug to avoid. | Set the AUMID at startup and stamp the identical one on the installer's Start Menu shortcut; decide this alongside the app-ID/`Scope` decisions, not after. `references/packaging-and-distribution.md` §5. |
| 27 | Pinning a build toolchain (`rust-toolchain.toml`, `.nvmrc`) and declaring an MSRV/minimum-runtime version are two different decisions that are easy to conflate — only 2 of 21 checkouts pin a toolchain at all, and none declares an MSRV. | Make both explicitly in `docs/DECISIONS.md`; an MSRV newer than the oldest target distro's compiler means that distro's package is permanently impossible. `references/ci-and-repo-governance.md` §5. |
| 28 | Copying a reference project's README feature-list wording (as opposed to its disclaimer paragraph) risks copying a stale claim — sone's own README still advertises MQA support, discontinued by TIDAL on 2024-07-24. | Copy sone's disclaimer paragraph verbatim; verify every format/feature claim against the current API before adapting feature-list copy. `references/licensing-and-legal.md` §4. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| Testing an unofficial-API client without live CI credentials: parser/transport split, fixture capture mechanism and redaction, contract tests against the OAS, opt-in live canaries (incl. the self-hosted-runner risk), fuzzing under sanitizers (incl. the `tidal://` handler), audio-pipeline tests, UI tests, the "no secrets, no network" CI rule, test determinism (clock/TZ/locale), coverage strategy, fixture-size policy and `.gitattributes` | `references/testing-strategy.md` |
| Secrets and tokens: what lives where, OS keyrings per platform (Linux/Flatpak incl. Secret-portal KDF/Windows incl. the mingw-w64 toolchain trap/macOS/headless), sone's encrypted-file envelope format and atomic-write pattern, embedded client-ID handling, what never gets committed, testing secret backends in CI, multi-process token/cache/database ownership, logout/purge/account-switching | `references/secrets-and-tokens.md` |
| Config/cache/data/log paths per OS, Flatpak/Snap path redirection, cache tiering/TTL/SWR/eviction, settings-file schema versioning and migration, log rotation, telemetry/crash-reporting posture, logs-as-listening-history | `references/config-cache-logs-telemetry.md` |
| Licensing: what the references chose, choosing GPL vs Apache split for streamboat (incl. the App-Store/mobile constraint), GStreamer/FFmpeg/fdk-aac/Qt compatibility notes (with corrected citations), trademark/positioning wording (incl. the stale-MQA-copy trap), the EU Cyber Resilience Act determination vs SECURITY.md's separate disclosure clock | `references/licensing-and-legal.md` |
| Packaging and distribution: Flathub (requirements, exact finish-args with corrected linter-rule ids, metainfo fields, submission mechanics, app-ID permanence, maintenance/verification), Snap, deb/rpm/AUR/Nix-as-a-channel/package-install-smoke-tests/FUNDING.yml, bundling the media runtime on Windows/macOS, Windows (installer/signing/winget incl. PUA/domain rules, AppUserModelID/SMTC), macOS (DMG/notarization/Homebrew), auto-update strategy, versioning/changelog/release-signing, headless/daemon packaging (systemd unit, container, `.desktop`) | `references/packaging-and-distribution.md` |
| CI job matrix (incl. package-install smoke tests), caching, pre-commit/hook gates, security scanning and dependency policy (incl. sanitizers), reproducible builds (incl. MSRV vs toolchain-pin), repo governance mechanics (CODEOWNERS, concurrency groups, required-checks list) | `references/ci-and-repo-governance.md` |
| Repository workspace layout (incl. `.gitattributes`, `FUNDING.yml`), docs/ADR conventions, CONTRIBUTING/code-of-conduct/issue-templates/SECURITY.md (with the corrected disclosure-timeline scope note), developer-environment setup (Nix flakes, Makefiles, `.editorconfig`) | `references/repo-layout-and-docs.md` |
| i18n (including TIDAL's own `locale` parameter and the mechanics gettext alone doesn't cover), accessibility (keyboard nav, screen readers per platform incl. SMTC/AUMID, contrast/motion), structured logging (incl. listening-history sensitivity), network-log redaction, debug bundles, metrics, the local-control-surface security baseline, performance budgets | `references/i18n-a11y-observability.md` |
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
2. **License split**: GPL-3.0-only everywhere, or Apache-2.0 `core` + GPL-3.0-only apps
   (recommended, but a one-way door without contributor consent for relicensing — **and now also
   the mobile decision**: GPL-3.0-only on the app is incompatible in practice with Apple App Store
   distribution, so this choice determines whether a future iOS app is possible at all).
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
- ~~Terminal playback sub-status list and the ReplayGain formula~~ — **now resolved, not
  unverified**: the list is `[4005, 4010, 4030, 4031, 4032, 4034, 4035]` (non-contiguous — `4033`
  and `4006` deliberately excluded) and the formula is
  `0.8 * min(10^((replay_gain + 4) / 20), 1 / peak)`, both verified at source
  (`references/testing-strategy.md` §3, §7).
- Whether Windows' `CRED_TYPE_GENERIC` is further limited to 512 bytes (vs the 2560-byte
  `CRED_MAX_CREDENTIAL_BLOB_SIZE`) — measure empirically; sidestepped by storing a key, not a
  payload. Separately confirmed: mingw-w64's own header disagrees with the 2560-byte figure (bare
  `512`), a real MinGW-vs-MSVC toolchain trap (`references/secrets-and-tokens.md` §2).
- The exact set of locales TIDAL's `locale` parameter accepts — undocumented anywhere in the
  reference set; determine empirically.
- Qt 6 module-by-module licensing (which modules are GPL-2.0-only vs GPL-3.0-only) — `doc.qt.io`
  is blocked; re-verify per module if Qt is chosen.
- Azure Trusted/Artifact Signing pricing and eligibility, and winget's "7-day PR timer" — both
  Microsoft domains are blocked; secondary-sourced only.
- The Flathub linter's exact wording for a `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID`
  never-granted rule — `docs.flathub.org` is blocked; the *default* MPRIS-ownership policy is
  confirmed from a primary source, only the specific linter-rule text is not. (The own-name rule
  ids for the appid/MPRIS case, by contrast, *are* now confirmed by direct fetch — see fact #15's
  correction of `finish-args-own-name-cpt`.)
- Whether Snap's `alsa` interface can ever be requested for auto-connection for a media player —
  not checked against `snapcraft.io/forum`.
- Whether Flathub's Generative AI policy actually flipped to an outright ban in commit `992f57b`
  (2026-05-29) before reverting — could not be independently re-verified in this pass (no GitHub
  API access to that repo's history here); the current policy text itself is confirmed current.
