---
name: streamboat-engineering-baseline
description: Stack-agnostic engineering conventions for streamboat, researched and fact-checked against 13+ reference TIDAL-adjacent projects — how to test an unofficial-API client without live credentials in CI (parser/transport split, fixture capture, fuzzing, opt-in live canaries), secret/token storage (OS keyring formats per platform, encrypted-file fallback, embedded client-ID handling, multi-process token races), config/cache/log/telemetry layout and caps, licensing (GPL/LGPL/Apache split, GStreamer/FFmpeg/Qt compatibility, trademark posture), packaging and distribution (Flathub requirements and submission mechanics, Snap, deb/rpm/AUR, AppImage, Windows MSI/winget/signing, macOS DMG/notarization/Homebrew), CI job design, repo layout/CONTRIBUTING/issue-templates/dev-environment, i18n/accessibility, and observability (structured logging, redaction, debug bundles, performance budgets). Use this whenever writing or reviewing a CI workflow (`.github/workflows/*`), a secret/keyring/token-storage module, config/cache/settings-path resolution code, a Flatpak/Snap/deb/rpm/AUR/AppImage/MSI/DMG/winget packaging manifest or script, LICENSE/SPDX/REUSE files, CONTRIBUTING.md/SECURITY.md/CODE_OF_CONDUCT.md/issue templates, docs/legal.md or docs/DECISIONS.md, i18n/gettext/Fluent setup, an accessibility pass, logging/redaction/debug-bundle/metrics code, a test file or test harness (`tests/**`, `conftest.py`, `*_test.go`, `*.test.ts`, `#[test]` parser tests), a fixture directory or fixture-capture/scrub script, `.gitattributes`/`.gitignore`, or a systemd unit or Dockerfile for the daemon — or whenever a task mentions Flathub, Flatpak, Snap, AUR, Homebrew, winget, notarization, Trusted Signing, keyring/Secret Service/Credential Manager/Keychain, GPL/LGPL/Apache/AGPL, CRA (Cyber Resilience Act), fixture capture, live canary, fuzzing, coverage target, changelog/SemVer, or "how should streamboat's CI/packaging/licensing/contributing process work." Do not answer these from generic open-source-project conventions — this skill has already fact-checked dozens of specific, easy-to-get-wrong claims (Homebrew's disjunctive-not-conjunctive star threshold, which of the three official TIDAL SDKs run the daily spec-drift check, Sone's cache/settings encryption specifics, Flathub's volatile AI-disclosure policy) and the corrections matter more than the surface-level claim.
---

# Engineering baseline for streamboat

Source of truth: `docs/research/engineering-baseline.md` (the full research report — fact-checked
and corrected against multiple independent passes, ~3,000 lines). This skill is the load-on-demand
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
- **TIDAL API approach**: the unofficial API used by python-tidal/High Tide/Sone, requiring the
  user's own paid subscription. streamboat is a player for subscribers, not a downloader/ripper —
  document legal risk honestly; never design or document DRM circumvention or piracy tooling as a
  how-to.
- **Never run `git commit`/`push`/`checkout`/`stash`/`reset`/`clean`** — this applies to any agent
  working in this repo, not just this skill's topic.

## Facts and pitfalls that cause real mistakes

42 facts from three research passes, merged into one table grouped by topic. Numbers are a stable
ID — every `fact #N` cross-reference elsewhere in this skill still resolves.

### Testing and CI

| # | Trap | The fix |
| --- | --- | --- |
| 1 | No reference project uses HTTP cassettes/VCR, and CI cannot use TIDAL's own credentialed-test model (forks get no secrets). | Split every response type into a pure `parse_*`/`TryFrom<Bytes>` function; test it with captured, redacted fixtures; stub the transport layer for retry/refresh/rate-limit behaviour. `references/testing-strategy.md` §1-3. |
| 5 | Registering a URL scheme (needed for "open in desktop app") means any web page can invoke `streamboat play <arbitrary string>` pre-authentication — an attacker-reachable surface easy to miss when scoping the fuzzing/security plan. **Correction: streamboat should register `x-scheme-handler/streamboat`, not `x-scheme-handler/tidal`** — `tidal://` is already claimed by the official TIDAL desktop app plus Strawberry/Sone/High Tide, and OS registration is last-writer-wins; canonical rule at `tidal-api/references/auth.md` §13. | Fuzz the URI parser (it must handle both `streamboat://` and parsed-but-unregistered `tidal://` links) against an allowlist of shapes; never pass the raw argument to a subprocess or API call without re-composing it from the parsed id. `references/testing-strategy.md` §6 item 5. |
| 8 | "All three official TIDAL SDKs run a daily API-spec-drift cron" — an earlier draft of this material said "both," which undercounts by one (web, iOS, **and** Android all run it). | Copy the mechanism from any of the three; cite all three, not two. `references/testing-strategy.md` §4. |
| 18 | Terminal playback sub-statuses are **not** the contiguous range `4030–4035` — `4033` (subscription up-sell) and `4006` (recoverable) are deliberately excluded. Canonical list owned by `tidal-api/references/transport.md` §6, do not restate it here. | Write the non-contiguous list, not a range, in any retry/terminal-error logic. `references/testing-strategy.md` §3. |
| 21 | A GitHub-hosted self-hosted runner for the live-canary/live-test workflow is a real code-execution risk on a public repo (a fork PR can run attacker code on the machine holding a live TIDAL refresh token) — no reference project uses one, and none uses `pull_request_target` either. | Run the live canary from a maintainer-local cron instead; never use `pull_request_target` in this repo. `references/testing-strategy.md` §5. |
| 32 | Registering `streamboat://` (and the fuzzing/threat-modelling that goes with it, fact #5 above) is not a Linux-only surface. Sone's deep-link registration mechanism (adapt with `streamboat` as the scheme, not Sone's `tidal`) writes `HKCU\Software\Classes` on Windows at runtime through Tauri's deep-link plugin; tidal-hifi's single cross-platform `protocols:` block expands into both the Windows registry and the macOS `CFBundleURLTypes` entry via electron-builder. | Design and threat-model all three registrations (Linux `.desktop`, Windows registry, macOS `CFBundleURLTypes`) for `streamboat://` plus second-instance argument forwarding from day one, not just the Linux `.desktop` file — and don't register `tidal://` itself, only parse it (fact #5). `references/testing-strategy.md` §6 item 5, `references/packaging-and-distribution.md` §9. |
| 41 | Committing real captured TIDAL API responses as test fixtures is a legal/ToS posture decision (they carry TIDAL's editorial copy and catalogue metadata, not just tokens), not only a redaction problem — and the reference precedent actually splits between synthetic (Sone) and real-captured (tidal-sdk-web, TIDAL's own repo) fixtures. | Default to minimal synthetic fixtures for catalogue/editorial payloads; reserve real captures for cases where the exact bytes are under test (manifests, error envelopes); record the decision in `docs/legal.md`. `references/testing-strategy.md` §2. |

### Secrets and tokens

| # | Trap | The fix |
| --- | --- | --- |
| 2 | A headless CI runner has no Secret Service, no unlocked Keychain, no interactive Credential Manager — "test the secret store" is not obvious. | `dbus-run-session -- gnome-keyring-daemon --unlock` on Linux, `security create-keychain`/`unlock-keychain` on macOS, direct Credential Manager API calls on Windows (all headless-safe). `references/secrets-and-tokens.md` §6. |
| 3 | Windows Credential Manager's `CRED_MAX_CREDENTIAL_BLOB_SIZE` (2560 bytes) makes storing a full token blob risky, and whether `CRED_TYPE_GENERIC` is further limited to 512 bytes is disputed. | Don't store the token in the keyring at all — store a fixed 32-byte **key** there (Sone's pattern) and keep the encrypted token payload in a file. A 32-byte key never approaches any platform's blob limit. `references/secrets-and-tokens.md` §2. |
| 4 | Two streamboat processes (desktop GUI + headless daemon) refreshing the same TIDAL token concurrently will invalidate each other and silently log the user out. This is now a real scenario, not hypothetical, because both ship at once. | Decide now: the daemon is always the token owner (GUI is a thin client), or the store needs an advisory cross-process lock with re-read-after-lock. `references/secrets-and-tokens.md` §7. |
| 6 | Sone does **not** "always write a keyring-backup file even when the keyring succeeds" (an easy claim to copy wrong) — the plaintext-permission 0600 file is written once, at first-run key generation, only. | Get this right if copying Sone's encrypted-file-fallback design; the security posture differs materially between "always" and "first run only." `references/secrets-and-tokens.md` §3. |
| 16 | The Windows Credential Manager blob-limit claim was previously verified against the **wrong** source — mingw-w64's `wincred.h` actually *disagrees* with the 2560-byte figure (it defines a bare `512`, no version guard), it does not corroborate it. | Cite `MicrosoftDocs/sdk-api`'s `ns-wincred-credentiala.md` for the 2560-byte figure; treat the mingw-w64/MSVC discrepancy itself as a real toolchain trap — measure at runtime on both toolchains if both are shipped. `references/secrets-and-tokens.md` §2. |
| 17 | The xdg-desktop-portal Secret portal's per-app secret is **not** already a usable AES key — the spec says its format is opaque and may be too short, and tells the caller to expand it with a KDF. | Feed the portal secret through HKDF-SHA256 with a fixed `info` string before using it as an AES-256-GCM key; don't assume it behaves like Sone's 32-byte keyring value. `references/secrets-and-tokens.md` §2. |
| 22 | Sone writes its settings/token file non-atomically (`fs::write`, no temp file) even though it uses a proper atomic temp-file→fsync→rename pattern for its *theme* file — an easy one to copy wrong if you generalise from "Sone does X" without checking which file. | Write every persisted secret/settings/state file with the atomic pattern from commit one; make a failed magic-header check on the encrypted file a hard typed error once v1 ships, not silent plaintext passthrough. `references/secrets-and-tokens.md` §3. |
| 23 | Nothing in the original report covers logout, account switching, or "delete all my data" — yet Sone's cache/settings design makes getting the ordering wrong a real bug (a scrobble sent after logout; account A's cache served to account B). | Implement `logout` in Sone's exact ordering (stop reporting before stopping playback, ...) and ship a separate, stronger `streamboat purge` that also clears the keyring entry, key file and logs. `references/secrets-and-tokens.md` §8. |
| 24 | The multi-process token-ownership decision (`secrets-and-tokens.md` §7) doesn't by itself cover the disk cache or any local database — two processes doing LRU eviction against the same cache directory with no coordination will double-evict and corrupt `.meta` sidecars. | Extend the single-decision-not-two-decisions framing to every shared on-disk store, not just tokens. `references/secrets-and-tokens.md` §7. |
| 33 | Sone's embedded-credential generator scripts (`gen_embedded.py`, `gen_credentials.py`) only emit the A/B (device-code) credential pair — neither emits the C/D (PKCE) pair that the shipped `embedded_config.rs` also defines and reads. Regenerating credentials with the shipped scripts silently drops PKCE support. | If copying this pattern, design streamboat's generator to emit every credential slot the binary consumes, and add a CI check that the generator's output matches the constants the code reads. `references/secrets-and-tokens.md` §4. |

### Config, cache, logs and telemetry

| # | Trap | The fix |
| --- | --- | --- |
| 7 | Sone encrypts **every cache entry**, not just settings/tokens — the 2 GiB cache cap is 2 GiB of AES-256-GCM blobs, and the cache subsystem cannot start before the master key resolves. | Decide explicitly whether streamboat's cache is encrypted, and state the startup ordering (key resolution gates cache init) either way. `references/config-cache-logs-telemetry.md` §2. |
| 29 | mopidy-tidal's audio-cache `manual` column does **not** pin entries — an easy claim to copy backwards. `evict()`'s keep-set is `WHERE not manual`, so a `manual=true` row is *excluded* from the keep-set and deleted first; `manual` is also never set `TRUE` anywhere in the codebase. | Copy the SQLite+WAL LRU shape (it's sound and gives multi-process safety for free), but design a real pin as `WHERE manual OR id IN (<keep-set>)` if you want one — the reference column is vestigial. `references/config-cache-logs-telemetry.md` §2. |
| 40 | The recommended packaging sequence (AppImage first, Flathub months later) guarantees a user's first upgrade path silently loses all local data, because Flatpak redirects every XDG directory — nothing in the original material drew this consequence. | Design `streamboat export`/`import` and a first-run "other install data found" probe before shipping the second package format. `references/config-cache-logs-telemetry.md` §6. |

### Licensing and legal

| # | Trap | The fix |
| --- | --- | --- |
| 14 | The GStreamer "for all practical reasons under the GPL itself" quote and the FFmpeg-build-mode guidance are **not** on GStreamer's own licensing FAQ page (verified by direct download+grep) — and they're **both from one file**, `gst-libav`'s `README.md`, **not** from `gst-plugins-base`'s `LICENSE_readme` (which does not exist at HEAD in the GStreamer monorepo — a second earlier-draft misattribution). | Cite `gst-libav/README.md` lines 15-20 for both quotes when this claim appears in `docs/legal.md` or a licensing writeup. `references/licensing-and-legal.md` §3. |
| 19 | The load-bearing reason the App Store is closed to streamboat is App Store Guideline **5.2.2** (third-party-service authorization) — an unofficial TIDAL client cannot produce the authorization it requires, full stop, independent of licence. **Whether GPL-3.0-only *additionally and independently* forecloses it is unresolved** — canonical, flagged-uncertain treatment owned by `tech-stack-evaluation/references/packaging-and-policy.md` §4; do not assert the GPL claim as settled fact (this fact previously did). | Keep `streamboat-core` **Apache-2.0** (not LGPL — its relinking requirement is itself contested on iOS) regardless of how the GPL question resolves — 5.2.2 alone already closes the App Store, so a permissive core costs nothing and keeps every other option open. `references/licensing-and-legal.md` §2. |
| 28 | Copying a reference project's README feature-list wording (as opposed to its disclaimer paragraph) risks copying a stale claim — Sone's own README still advertises MQA support, discontinued by TIDAL on 2024-07-24. | Copy Sone's disclaimer paragraph verbatim; verify every format/feature claim against the current API before adapting feature-list copy. `references/licensing-and-legal.md` §4. |
| 37 | "Every native GUI client is GPL-3.0" overstates the pattern by one — tidalt (a native Go TUI/daemon with a `.desktop` entry, not a web wrapper) is Apache-2.0, a second exception alongside tidal-hifi. | Frame it as "every native GUI client except tidal-hifi and tidalt is GPL-3.0" in any licensing writeup. `references/licensing-and-legal.md` §1. |

### Packaging and distribution

| # | Trap | The fix |
| --- | --- | --- |
| 9 | Homebrew's notability floor ("30 forks / 30 watchers / 75 stars, 90/90/225 for self-submission") reads like a conjunction in slash notation but is **disjunctive** — any one number alone clears the bar. Homebrew *also* applies an ecosystem-specific download cooldown that delays a freshly-published version's adoption. | 225 stars alone is enough for a self-submission; don't over-scope this as a launch blocker. Check the cooldown list before promising same-day `brew install` availability on release day. `references/packaging-and-distribution.md` §6. |
| 10 | Flathub's Generative-AI-disclosure policy is a dated snapshot; a claim that it briefly flipped to an outright ban in 2026 (commit `992f57b`) could **not** be independently re-verified in this pass (no GitHub API access to that repo's history here). | Don't repeat the specific commit/date as settled fact. Add a standing task to re-read `flathub-infra/documentation`'s current policy immediately before any Flathub submission regardless. `references/packaging-and-distribution.md` §1. |
| 11 | The packaging sequence has no portable Linux binary for the first several months (Flathub needs a dev-history track record; Homebrew needs a 30-day-old repo) — a Silverblue/NixOS/Debian-stable user has nothing to install. | Ship an AppImage (or static tarball for the daemon) as the *first* packaging deliverable, before AUR — Nix (`flake.nix` with a package output, not just a devShell) is a second, zero-review day-one channel. `references/packaging-and-distribution.md` §4, `references/decisions-and-sequencing.md` §3. |
| 12 | Azure Trusted Signing eligibility is genuinely **narrower for an individual than for an organisation** — organisations are eligible across a 12-country list (US, Canada, EU, UK, Australia, NZ, Japan, South Korea, Singapore, Switzerland, Norway, Israel); individual developers are restricted to the US or Canada only, free/trial/sponsored Azure subscriptions don't qualify, and individual onboarding is reportedly paused. An earlier pass inverted this reading — do not repeat "not narrower for individuals." winget's "7-day PR auto-close timer" also remains unconfirmed. | A non-US/Canada individual maintainer cannot buy this at all — budget accordingly, and re-verify both facts from an unblocked network before they drive a cost or submission-process decision. `references/packaging-and-distribution.md` §5. |
| 20 | Neither Windows nor macOS ships GStreamer/FFmpeg — the installer must bundle the entire media runtime plus a bundled-vs-system plugin-discovery code path. This is easy to miss because a dev build "just works" on the developer's own machine (which has the runtime installed separately). | Budget this as a first-week Windows/macOS line item, not a late add-on; see Sone-windows's generated NSIS/WiX fragments and Strawberry's `ntool`/`gststartup.cpp` bundle-relative plugin-path rewriting for the concrete shape. `references/packaging-and-distribution.md` §4a. |
| 25 | winget's silent-install and `Scope` requirements are necessary but not sufficient — `InstallerUrl` must also be HTTPS from an approved official domain, and a package flagged as a Potentially Unwanted Application is rejected "regardless of the application's legitimacy," which a naive scanner could do to an unofficial-API client. | Read `doc/Validation.md`'s "Manifest URLs" section before the first winget submission; comment `@wingetbot run` to re-trigger after a fix. `references/packaging-and-distribution.md` §5. |
| 26 | No reference project sets a Windows AppUserModelID — so "SMTC doesn't show transport controls" is a silent, no-precedent failure mode, not a copyable bug to avoid. | Set the AUMID at startup and stamp the identical one on the installer's Start Menu shortcut; decide this alongside the app-ID/`Scope` decisions, not after. `references/packaging-and-distribution.md` §5. |
| 30 | The Flathub linter's "never granted" rule for `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID` was previously unverified — it is now **confirmed** by direct fetch (`finish-args-mpris-flatpak-id-talk-name`, zero exceptions in the live `exceptions.json`), alongside the companion Secrets-casing rule. | Don't request that talk-name; the default sandbox policy already covers MPRIS ownership. `references/packaging-and-distribution.md` §1. |
| 34 | Flatpak runtime choice is a **standing, roughly-yearly upgrade obligation** (Flathub rejects EOL runtimes), not a one-time pick — and zero of the 21 checkouts use `org.freedesktop.Platform.ffmpeg-full` or any `add-extensions` block, so there is no precedent for sourcing extra codecs as a Flatpak extension. | Decide which runtime, which decoders it ships, and which decoders streamboat must build as manifest modules (High Tide's pattern for its own extra deps); put "runtime version bump" on the release calendar. `references/packaging-and-distribution.md` §1. |
| 35 | Flathub has a named "insecure design" rejection criterion ("disabling or bypassing security mechanisms ... shipping overly permissive configurations") that bears directly on two of this skill's own recommendations: the obfuscated embedded client id (fact #33-adjacent, §4) and the `--insecure-token-store` plaintext fallback (`references/secrets-and-tokens.md` §2, Headless/server). | Never call the embedded-credential obfuscation "encryption" anywhere a reviewer reads (metainfo/README/manifest comments, not just user docs); compile the plaintext-token-store path out of the Flatpak build entirely. `references/packaging-and-distribution.md` §1, `references/secrets-and-tokens.md` §2. |
| 36 | Provenance and OIDC trusted publishing are different mechanisms with different precedent counts — an earlier pass conflated them as "two projects use provenance." Only tidal-cli actually attests provenance (`npm publish --provenance`); tidal-sdk-web uses OIDC for npm trusted publishing but explicitly sets `NPM_CONFIG_PROVENANCE: "false"`. | Cite tidal-cli alone as the provenance precedent; treat release-artifact signing/provenance/SBOM as a genuinely no-precedent line item to budget for. `references/packaging-and-distribution.md` §8. |
| 39 | tidalt's systemd unit template (copied verbatim by an earlier pass) has **zero** hardening directives, is `--user`-scoped, and therefore never starts on a true headless/server box at all — a real gap given the owner's headless-now mandate. | Add `NoNewPrivileges`/`ProtectSystem=strict`/`ProtectHome`/`PrivateTmp`/`RestrictAddressFamilies`/`RestrictNamespaces` (keep `MemoryDenyWriteExecute` off if using liborc JIT); ship a *second*, system-level unit with a dedicated service user for headless deployment. `references/packaging-and-distribution.md` §9. |

### i18n, accessibility and observability

| # | Trap | The fix |
| --- | --- | --- |
| 13 | TIDAL's v1 endpoints take a `locale` parameter separate from `countryCode`; every reference project hardcodes `en_US` (Sone does this at 21 call sites, not the ~10 an earlier draft estimated). | Pass the user's real locale — TIDAL's own editorial titles/mixes come back localised for free. `references/i18n-a11y-observability.md` §1. |
| 31 | "Only High Tide ships translations" is wrong and self-contradicting — Strawberry ships 31 Qt `.ts` catalogues synced through Crowdin, a second working i18n precedent at larger scale. Separately, High Tide's `po/POTFILES` **already** lists the `.desktop.in`/`.appdata.xml.in` sources and merges them via `i18n.merge_file` — it is a complete template, not a gap needing "add these files explicitly." | Use the High Tide/gettext pair and the Strawberry/Crowdin pair together when deciding the translation platform; copy High Tide's `i18n.merge_file` pattern directly for desktop/metainfo strings. `references/i18n-a11y-observability.md` §1. |
| 42 | On Wayland, "media keys via MPRIS" undersells it — MPRIS is not one option among several, it is *the* mechanism, since the compositor routes hardware media keys to whichever MPRIS2 service is registered; a user-defined global hotkey needs the separate `org.freedesktop.portal.GlobalShortcuts` portal. | Document MPRIS as mandatory (not optional) media-key wiring on Linux, and use the GlobalShortcuts portal — not a toolkit-level grab — for any custom hotkey inside a Flatpak. `references/i18n-a11y-observability.md` §2. |

### Repo layout and CI governance

| # | Trap | The fix |
| --- | --- | --- |
| 15 | "`.claude/skills/` is a real convention in this ecosystem" overgeneralises — no checkout uses `.claude/skills/` specifically, but `.claude/` itself is not absent (tidal-sdk-ios ships `.claude/commands/`) and 5 of 21 checkouts (2 of them official SDKs) ship a root `CLAUDE.md`. | The narrower finding still holds — `.agents/skills/` has working CI-invocation precedent, `.claude/skills/` does not — but don't claim "nobody uses `.claude/`". `references/repo-layout-and-docs.md` §2. |
| 27 | Pinning a build toolchain (`rust-toolchain.toml`, `.nvmrc`) and declaring an MSRV/minimum-runtime version are two different decisions that are easy to conflate — only 2 of 21 checkouts pin a toolchain at all, and none declares an MSRV. | Make both explicitly in `docs/DECISIONS.md`; an MSRV newer than the oldest target distro's compiler means that distro's package is permanently impossible. `references/ci-and-repo-governance.md` §5. |
| 38 | CI secrets are named piecemeal across this skill with no single inventory and no protection mechanism identified. Four reference SDKs actually gate privileged jobs behind a named GitHub `environment:`, which is how a secret gets scoped to one job and optionally requires human approval. | Build a secrets-inventory table (secret → job → holder → expiry) and put a required-reviewer Environment on every publishing job; note the Apple cert / Trusted Signing profile / AUR key are single points of failure with no documented succession. `references/ci-and-repo-governance.md` §4. |

## Where to go for depth

| Question | Reference file |
| --- | --- |
| Testing an unofficial-API client without live CI credentials: parser/transport split, fixture capture mechanism and redaction, contract tests against the OAS, opt-in live canaries (incl. the self-hosted-runner risk), fuzzing under sanitizers (incl. the `tidal://` handler), audio-pipeline tests, UI tests, the "no secrets, no network" CI rule, test determinism (clock/TZ/locale), coverage strategy, fixture-size policy and `.gitattributes` | `references/testing-strategy.md` |
| Secrets and tokens: what lives where, OS keyrings per platform (Linux/Flatpak incl. Secret-portal KDF/Windows incl. the mingw-w64 toolchain trap/macOS/headless), Sone's encrypted-file envelope format and atomic-write pattern, embedded client-ID handling, what never gets committed, testing secret backends in CI, multi-process token/cache/database ownership, logout/purge/account-switching | `references/secrets-and-tokens.md` |
| Config/cache/data/log paths per OS, Flatpak/Snap path redirection, cache tiering/TTL/SWR/eviction, settings-file schema versioning and migration, log rotation, telemetry/crash-reporting posture, logs-as-listening-history | `references/config-cache-logs-telemetry.md` |
| Licensing: what the references chose, choosing GPL vs Apache split for streamboat (incl. the App-Store/mobile constraint), GStreamer/FFmpeg/fdk-aac/Qt compatibility notes (with corrected citations), trademark/positioning wording (incl. the stale-MQA-copy trap), the EU Cyber Resilience Act determination vs SECURITY.md's separate disclosure clock | `references/licensing-and-legal.md` |
| Packaging and distribution: Flathub (requirements, exact finish-args with corrected linter-rule ids, metainfo fields, submission mechanics, app-ID permanence, maintenance/verification), Snap, deb/rpm/AUR/Nix-as-a-channel/package-install-smoke-tests/FUNDING.yml, bundling the media runtime on Windows/macOS, Windows (installer/signing/winget incl. PUA/domain rules, AppUserModelID/SMTC), macOS (DMG/notarization/Homebrew), auto-update strategy, versioning/changelog/release-signing, headless/daemon packaging (systemd unit, container, `.desktop`) | `references/packaging-and-distribution.md` |
| CI job matrix (incl. package-install smoke tests), caching, pre-commit/hook gates, security scanning and dependency policy (incl. sanitizers), reproducible builds (incl. MSRV vs toolchain-pin), repo governance mechanics (CODEOWNERS, concurrency groups, required-checks list) | `references/ci-and-repo-governance.md` |
| Repository workspace layout (incl. `.gitattributes`, `FUNDING.yml`), docs/ADR conventions, CONTRIBUTING/code-of-conduct/issue-templates/SECURITY.md (with the corrected disclosure-timeline scope note), developer-environment setup (Nix flakes, Makefiles, `.editorconfig`) | `references/repo-layout-and-docs.md` |
| i18n (including TIDAL's own `locale` parameter and the mechanics gettext alone doesn't cover), accessibility (keyboard nav, screen readers per platform incl. SMTC/AUMID, contrast/motion), structured logging (incl. listening-history sensitivity), network-log redaction, debug bundles, metrics, the local-control-surface security baseline, performance budgets | `references/i18n-a11y-observability.md` |
| The full reference-project comparison table, the 31-item "decide and do now" checklist, the packaging rollout sequence, the stack-blocked items table, the full text of every open decision and unverified item | `references/decisions-and-sequencing.md` |
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

> **Resolved by the owner on 2026-09-08/09.** The items below were the inputs to the decision tree; the
> outcomes are recorded in `docs/DECISIONS.md` and distilled in the `streamboat-decisions` skill, which
> takes precedence over any recommendation here. Treat this list as history, not as open questions.

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
9. **Headless control surface**: MPRIS/D-Bus only, or an HTTP/JSON API? If HTTP, Sone's
   loopback-bound, token-authenticated, field-sanitized MCP server is the baseline design
   (`references/i18n-a11y-observability.md` §7).
10. **Release cadence**: weekly-ish (Sone's pace) or slower/batched?
11. **Translation platform**: self-hosted Weblate, Crowdin, or plain PRs against `po/`?
12. **Minimum supported platform versions** (oldest glibc/Ubuntu, macOS SDK, Windows version).
13. **Release automation tool and monorepo versioning**: pick one appropriate to the chosen stack
    (release-please / changesets / cargo-release+cargo-dist / goreleaser+nfpm), and decide whether
    `core` is versioned/released independently of the apps.
14. **Forge choice**: GitHub, or Codeberg/self-hosted? Nearly every mechanism in this skill (CI,
    secret scanning, private vulnerability reporting, `io.github.*` Flathub verification, the
    GitHub-Environments secret scoping) assumes GitHub — decide this alongside the app-ID/owner ADR
    (`references/packaging-and-distribution.md` §2), not as a byproduct of "where did I push it."
15. **Fixture provenance**: synthetic (structure-preserving, content-invented) vs. real captured
    TIDAL API responses committed to a public test-fixtures directory — a legal/ToS posture
    decision, not only a redaction mechanic (`references/testing-strategy.md` §2, fact #41 above).

## Unverified

Facts in this skill that could not be confirmed from a primary source in this research pass — do
not present these as settled in specs, code comments, or `docs/legal.md` (full text and sources:
`references/decisions-and-sequencing.md` §6):

- **TIDAL's Terms of Service and Developer Terms** — the entire `tidal.com` domain is blocked from
  the research environment; every quote is second-hand. The "Player module is the only allowed
  playback path" claim applies to the *developer-program* SDK, not necessarily the *unofficial*
  API route streamboat uses — these are separate legal questions (`references/licensing-and-legal.md` §4).
- **Terminal playback sub-status list**: resolved, not unverified — the list is
  `[4005, 4010, 4030, 4031, 4032, 4034, 4035]` (non-contiguous — `4033` and `4006` deliberately
  excluded), verified at source and owned canonically by `tidal-api/references/transport.md` §6.
  **The ReplayGain formula is a separate question and is NOT `0.8 * min(...)`** — that was this
  skill's own error, copying Sone's misleading code comment rather than TIDAL's SDKs. TIDAL's own
  Android/web SDKs compute `min(10^((replay_gain + 4) / 20), 1 / peak)`, no `0.8` factor; Sone adds
  the `0.8` as its own choice. Canonical formula owned by
  `audio-pipeline/references/playback-behavior.md` §4 (`references/testing-strategy.md` §7).
- Whether Windows' `CRED_TYPE_GENERIC` is further limited to 512 bytes (vs the 2560-byte
  `CRED_MAX_CREDENTIAL_BLOB_SIZE`) — measure empirically; sidestepped by storing a key, not a
  payload. Separately confirmed: mingw-w64's own header disagrees with the 2560-byte figure (bare
  `512`), a real MinGW-vs-MSVC toolchain trap (`references/secrets-and-tokens.md` §2).
- The exact set of locales TIDAL's `locale` parameter accepts — undocumented anywhere in the
  reference set; determine empirically.
- Qt 6 module-by-module licensing (which modules are GPL-2.0-only vs GPL-3.0-only) — `doc.qt.io`
  is blocked; re-verify per module if Qt is chosen.
- Azure Trusted/Artifact Signing exact pricing, and winget's "7-day PR timer" — both Microsoft
  domains are blocked; secondary-sourced only. **The eligibility split itself is not in question**
  (organisations: 12-country list; individuals: US/Canada only) — that part is corrected and
  believed accurate; only the dollar figures and the winget timer remain unverified.
- **Resolved, no longer unverified**: the Flathub linter's `--talk-name=org.mpris.MediaPlayer2.$FLATPAK_ID`
  never-granted rule. Now confirmed by direct fetch of the linter source as
  `finish-args-mpris-flatpak-id-talk-name`, zero exceptions in the live `exceptions.json` — see
  fact #30 above.
- Whether Snap's `alsa` interface can ever be requested for auto-connection for a media player —
  not checked against `snapcraft.io/forum`.
- Whether Flathub's Generative AI policy actually flipped to an outright ban in commit `992f57b`
  (2026-05-29) before reverting — could not be independently re-verified in this pass (no GitHub
  API access to that repo's history here); the current policy text itself is confirmed current.
