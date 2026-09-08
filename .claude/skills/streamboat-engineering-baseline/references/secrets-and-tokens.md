# Secrets and tokens

Full source: `docs/research/engineering-baseline.md` §3 (fact-checked). `ref:<project>/<path>`
points at a shallow clone — see `sources.md`.

## Contents

1. What lives where
2. OS keyrings, per platform
3. Encrypted file fallback: copy sone's format
4. Client ID / client secret handling
5. What never gets committed, and how to enforce it
6. Testing the secret-store backends in CI
7. Multi-process token and device ownership

## 1. What lives where

| Data | Storage | Rationale |
|---|---|---|
| `refresh_token` | OS keyring; encrypted file fallback | Long-lived, the only real credential |
| `access_token`, `expiry` | Same store as refresh token, or memory + re-derive | Short-lived, but leaks the account if logged |
| `sessionId`, `userId`, `countryCode` | Encrypted config file | Not secret, but identifying |
| `client_id` / `client_secret` (embedded default) | Build-time constant, obfuscated at best | Extractable by anyone; see §4 |
| User-supplied `client_id`/`client_secret` | Same store as tokens | Treat as secret because the user chose it |
| Cached API responses | Encrypted or plain cache dir, user-configurable | Contains library/listening data |

## 2. OS keyrings, per platform

- **Linux — Secret Service / libsecret.** High Tide defines a schema
  `io.github.nokse22.high-tide` with a single `version` string attribute, stores one JSON blob
  under key `high-tide-login` containing `token-type`, `access-token`, `refresh-token`,
  `expiry-time`, `is-pkce`, and wraps the whole load in a try/except that resets the store on
  corruption (ref:high-tide/src/lib/secret_storage.py). It also unlocks the default collection at
  startup — `Secret.Service.get_sync()` → `Secret.Collection.for_alias_sync(...,
  Secret.COLLECTION_DEFAULT, ...)` → `service.unlock_sync([collection])` — but only when
  `not Xdp.Portal.running_under_flatpak()`.
- **Flatpak.** `--talk-name=org.freedesktop.secrets` is the (lowercase — the Flathub linter is
  reported to have a never-granted rule for the wrong casing, though `docs.flathub.org` could not
  be fetched directly to confirm the exact rule name — see `packaging-and-distribution.md` §1) way
  to reach a host keyring. The sandbox-native alternative is the **Secret portal**,
  `org.freedesktop.portal.Secret` (xdg-desktop-portal ≥ 1.5.0, version number corroborated by
  secondary sources), which hands the app a per-app master secret over a pipe FD; the secret is
  stable for the life of the installation and is itself stored in the user's keyring under the app
  ID (verified by direct fetch of
  `raw.githubusercontent.com/flatpak/xdg-desktop-portal/main/data/org.freedesktop.portal.Secret.xml`).
  Prefer the portal: it works with no extra static permission and degrades to "generate a random
  key" when no keyring exists.
- **Windows — Credential Manager.** `CRED_MAX_CREDENTIAL_BLOB_SIZE` is `5*512` = 2560 bytes on
  Windows 7 and later. `learn.microsoft.com` is blocked from this environment, so this is verified
  against a mingw-w64 copy of `wincred.h` and corroborated by AdysTech/CredentialManager issue #65
  ("The Windows 10 SDK's wincred.h sets CRED_MAX_CREDENTIAL_BLOB_SIZE to 5*512") rather than read
  from the primary Microsoft page. Keyring wrappers have historically produced obscure failures
  past that limit (https://github.com/jaraco/keyring/issues/355); whether `CRED_TYPE_GENERIC` is
  *further* limited to 512 bytes remains disputed and unverified — measure it empirically.
  **Sidestep the whole question**: make the keyring hold a fixed-size *key*, not the token payload
  — exactly what sone already does on Linux (`keyring::Entry::new("sone", "master-key")` holds 32
  bytes; the tokens live in the AES-256-GCM file, ref:sone/src-tauri/src/crypto.rs). A 32-byte key
  can never approach 2560 bytes on any OS, which makes one design work uniformly on
  Windows/macOS/Linux and removes the need to measure token sizes at all. Where no credential
  store is usable at all on Windows, DPAPI (`CryptProtectData`/`CryptUnprotectData` with
  `CRYPTPROTECT_UI_FORBIDDEN`) wraps the key file with the user's login credentials and has no
  size limit — name it alongside the age-encrypted fallback cited from tidalt below as the
  idiomatic Windows equivalent.
- **macOS — Keychain.** Generic password items comfortably hold a token blob; TidalSwift and
  tidal-sdk-ios both use KeychainAccess-style generic items. Under a sandboxed/hardened build the
  keychain access group must be declared in entitlements **[STACK]**.
- **Headless/server.** There may be no keyring, no D-Bus session and no TTY. Support, in order:
  (1) an existing Secret Service if `DBUS_SESSION_BUS_ADDRESS` is set — python-tidal explicitly
  passes that through in tox for exactly this reason (ref:python-tidal/tox.ini); (2) an encrypted
  file whose key comes from `STREAMBOAT_MASTER_KEY` or a passphrase prompt; (3) a plain file at
  mode 0600 only when the user opts in with `--insecure-token-store`, logged loudly at startup.
  tidalt does the equivalent: `docker/secrets-engine` keychain first, `posixage` (age-encrypted,
  passphrase-callback) fallback at `~/.config/tidalt/secrets`, directory created 0700
  (ref:tidalt/internal/store/store.go:64-120).

## 3. Encrypted file fallback: copy sone's format

`ref:sone/src-tauri/src/crypto.rs` is a good, small design worth reproducing in any language:

- AES-256-GCM, random 96-bit nonce per write.
- On-disk layout `MAGIC("SONE") || VERSION(1 byte) || NONCE(12) || CIPHERTEXT+TAG`, 17-byte header.
- `decrypt()` checks the magic; if absent it returns the input unchanged, giving free migration
  from an earlier plaintext version.
- Key resolution: OS keyring first (`keyring::Entry::new("sone", "master-key")`, `get_secret()`,
  lines 79-110 — **returns immediately on success, writing no file**), then `<config>/sone.key`
  (exactly 32 bytes, `0o600`, written only at first-run key generation, lines 113-148), else
  generate from `OsRng`. The comment "keyring may be unreachable on next launch (e.g. AppImage
  with different D-Bus session)" sits inside the *new-key-generation* branch only — it is not a
  claim that the file is rewritten on every launch. (An earlier draft of this document claimed
  sone "always writes the file backup even when the keyring succeeds" — refuted; corrected here.)
- The in-memory key buffer is zeroized (line 28) after the cipher is constructed.
- sone reuses this exact envelope for cache-entry encryption too, not only settings/tokens — see
  `config-cache-logs-telemetry.md` §2.

For streamboat, change two things: bump the version byte on any format change and refuse to
downgrade; and make the "write a key file even when the keyring works (at first-run only)"
behaviour a documented, user-visible setting, because it means the encryption is only as strong as
the file permissions on that path.

## 4. Client ID / client secret handling

State of the art in the references, all of which is obfuscation and none of which is security:

- **python-tidal** base64-decodes its client credentials at `tidalapi/session.py` lines ~155-162
  (per the prior survey) — reversible by anyone.
- **sone** XOR-encodes each value with a random pad and stores *the pad in the same file*
  (`STREAM_SALT_A` next to `CODEC_HINT_A`), regenerated by `scripts/gen_embedded.py`, with the
  variables deliberately misnamed ("stream salt", "codec hint") to avoid grepability
  (ref:sone/scripts/gen_embedded.py, ref:sone/src-tauri/src/embedded_config.rs). It ships four
  pairs (A/B = device-code id/secret, C/D = PKCE id/secret) and a `has_stream_keys()` predicate
  that treats a `PLACEHOLDER`-prefixed value as absent.
- **strawberry** encrypts build-time credentials with AES-256-CBC at CMake configure time, storing
  them as `ENC:<iv_hex>:<base64>` with the key being `SHA256(passphrase)`; CI supplies
  `-DAPI_CREDENTIALS_ENCRYPTION_KEY="$(openssl rand -hex 32)"`, i.e. a fresh key per build that is
  necessarily also present in the binary (ref:strawberry/cmake/ApiCredentials.cmake:37-78). The
  `TIDAL_CLIENT_ID` itself comes from a GitHub secret.
- **tidalt** hardcodes both values in the clear and documents them as public
  (ref:tidalt/internal/tidal/client.go, per the prior survey).

**Recommendation:** design the app so a user can supply their own client id/secret in settings, and
make any embedded default an optional build input (`STREAMBOAT_CLIENT_ID` at build time, absent by
default). Never commit a credential to the repository. Do not invent a bespoke obfuscation scheme
and describe it as encryption in user-facing docs; say plainly that a shipped credential is
extractable. Where an embedded default exists, gate it behind a build flag so a distro packager can
build without it.

## 5. What never gets committed, and how to enforce it

Blocklist: tokens of any kind, `client_secret` values, `.env`, session JSON, keyring dumps, HAR
files, unredacted fixture captures, `settings.json` from a real install, screenshots showing an
email address or a real library.

Enforcement:

- `.gitignore` covering `*.token`, `*.session.json`, `.env*`, `fixtures/**/raw/`, plus per-stack
  build dirs. Sone's `.gitignore` also excludes `nocommit/` as a scratch directory — a useful
  convention (ref:sone/.gitignore).
- A `pre-commit` hook that greps staged content for token-shaped strings (`ey[A-Za-z0-9_-]{20,}\.`
  for JWTs, `Bearer\s+[A-Za-z0-9._-]{20,}`, `refresh_token"\s*:` with a non-empty value). tidalt's
  hook is the model for "thin dispatcher that mirrors CI"
  (ref:tidalt/.githooks/pre-commit) — including the `git config core.hooksPath .githooks` opt-in
  and the documented `--no-verify` escape hatch.
- Secret scanning in CI (GitHub secret scanning + push protection on a public repo; gitleaks or
  trufflehog as a job for anything self-hosted).
- A documented rotation procedure: if a token leaks, the user revokes it by signing out of the
  device in TIDAL's account settings; if an embedded client id is abused, the fallback is
  user-supplied credentials.

## 6. Testing the secret-store backends in CI

The non-negotiable CI rule (`testing-strategy.md` §9) requires the default suite to pass on a fork
PR, but a headless GitHub runner has no Secret Service, no unlocked Keychain and no interactive
Credential Manager session. Make the store an interface and unit-test the encrypted-file and
plaintext backends everywhere (no OS dependency). For the keyring backend on Linux CI, run the job
under `dbus-run-session -- gnome-keyring-daemon --unlock` (feeding a dummy password on stdin) and
skip (not fail) the test when `DBUS_SESSION_BUS_ADDRESS` is unset — python-tidal already treats
that variable as the switch, passing it through in `tox.ini` precisely so its
`KeyringCredentials` path can run (ref:python-tidal/tox.ini). On macOS runners, create and unlock a
keychain explicitly (`security create-keychain` / `unlock-keychain`), the same step Strawberry's
signing job performs (ref:strawberry/.github/workflows/build.yaml). On Windows, the Credential
Manager API works headlessly for the current user, so that backend can be tested directly — and
it is the right place to empirically measure real TIDAL token sizes against the 2560-byte blob
limit in §2.

## 7. Multi-process token and device ownership

The owner has committed to shipping desktop **and** headless now. Both will read the same token
store, and both may want the same DAC. TIDAL rotates refresh tokens, so two processes refreshing
concurrently will invalidate each other and silently log the user out — a bug that only appears
after both binaries ship, and it constrains the storage and process model from the first commit.
`testing-strategy.md` §3 test 1 handles in-process refresh collapsing; it does not handle the
cross-process case.

tidalt solves both problems with a single-server model: the first process claims the D-Bus name
`org.mpris.MediaPlayer2.tidalt`; if the name is taken the process becomes a client and forwards
commands over D-Bus, and only the server process ever opens the ALSA `hw:` device — its own docs
state "ALSA `hw:` devices cannot be shared between processes. If two programs both try to open
`hw:1,0` the second one fails" (ref:tidalt/docs/client-server.md). `tidalt daemon` is the same
engine with no TUI (ref:tidalt/cmd/tidalt/daemon.go:44). sone takes the desktop-only version of
this with `tauri_plugin_single_instance` to focus the existing window instead of opening a second
one (ref:sone/src-tauri/src/lib.rs:546-548).

**Decide now**: either the daemon is always the token owner (with the GUI as a thin client), or
the store must be lock-protected for multi-writer access (an advisory file lock around
read-refresh-write, with a re-read-after-lock so a losing process adopts the winner's new token).
Add a test for this to `testing-strategy.md` §3's list.
