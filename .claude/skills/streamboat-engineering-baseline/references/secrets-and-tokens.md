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
7. Multi-process token, device and cache/database ownership
8. Logout, account switching, and complete data deletion

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
  key" when no keyring exists. **The portal secret is not, by itself, a usable AES key** — the spec
  says so explicitly: "the format is opaque to the application. In particular, the length of the
  secret might not be sufficient for the use with certain encryption algorithm. In that case, the
  application is supposed to expand it using a KDF algorithm."
  (https://raw.githubusercontent.com/flatpak/xdg-desktop-portal/main/data/org.freedesktop.portal.Secret.xml,
  read directly). Feed it through HKDF-SHA256 with a fixed `info` string before using it as an
  AES-256-GCM key — do not assume it is already 32 bytes of good entropy the way sone's
  keyring-held key is. `RetrieveSecret` also takes an opaque `token` option from the previous call;
  persist and pass it back rather than re-deriving from scratch each launch.
- **Windows — Credential Manager.** `CRED_MAX_CREDENTIAL_BLOB_SIZE` is `5*512` = 2560 bytes on
  Windows 7 and later, per Microsoft's own docs source: "This member cannot be larger than
  CRED_MAX_CREDENTIAL_BLOB_SIZE (5*512) bytes."
  (https://raw.githubusercontent.com/MicrosoftDocs/sdk-api/docs/sdk-api-src/content/wincred/ns-wincred-credentiala.md,
  read directly since `learn.microsoft.com` itself is blocked — this repo is the primary upstream
  source for that blocked page). Keyring wrappers have historically produced obscure failures past
  that limit (https://github.com/jaraco/keyring/issues/355); whether `CRED_TYPE_GENERIC` is
  *further* limited to 512 bytes remains disputed and unverified — measure it empirically.
  **A toolchain trap makes "measure it empirically" load-bearing, not optional**: mingw-w64's own
  header disagrees with Microsoft's. `mingw-w64-headers/include/wincred.h` at mingw-w64 `master`
  defines `#define CRED_MAX_CREDENTIAL_BLOB_SIZE 512` — a bare 512, **no** `WINVER` guard
  (https://raw.githubusercontent.com/mingw-w64/mingw-w64/master/mingw-w64-headers/include/wincred.h,
  line 114). Code that validates against the compile-time constant gets a 5x smaller limit under a
  MinGW-targeted build than under MSVC, for the identical Windows version, with no compiler
  warning — do not trust the compile-time constant on any toolchain; measure at runtime, and
  re-check on both MinGW and MSVC builds if both are shipped. (An earlier draft of this document
  mis-cited the mingw-w64 header as the *verification source* for the 2560-byte figure — wrong;
  mingw-w64's header actually disagrees with it. This is now corrected.)
  **Sidestep the whole question**: make the keyring hold a fixed-size *key*, not the token payload
  — exactly what sone already does on Linux (`keyring::Entry::new("sone", "master-key")` holds 32
  bytes; the tokens live in the AES-256-GCM file, ref:sone/src-tauri/src/crypto.rs). A 32-byte key
  can never approach 2560 bytes — or even 512 — on any OS or toolchain, which makes one design work
  uniformly regardless of compiler. Where no credential store is usable at all on Windows, DPAPI
  (`CryptProtectData`/`CryptUnprotectData` with `CRYPTPROTECT_UI_FORBIDDEN`) wraps the key file
  with the user's login credentials and has no size limit — name it alongside the age-encrypted
  fallback cited from tidalt below as the idiomatic Windows equivalent.
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

**Write it atomically, or a crash mid-write turns the magic-header passthrough into a silent
re-login.** sone's `decrypt()` treats a missing/bad magic header as "plaintext, pass through
unchanged" — right for an *intentional* legacy file, but a file *torn* by a crash/`SIGKILL`
mid-write is then silently misread as plaintext or fails opaquely, logging the user out with no
diagnostic. sone itself only writes one file class atomically — the *theme* file, not
settings/tokens: `write_theme_file` does temp-file → fsync → rename, mode `0644`, with the comment
"So a crash can never leave a torn file" (ref:sone/src-tauri/src/theme_config.rs:142-175), while
the settings/token path is a plain `fs::write(&self.settings_path, encrypted)` with no temp file
(ref:sone/src-tauri/src/lib.rs:474-477). sone's own scrobble/play-report queues *do* use the safer
`.bin.tmp` pattern (ref:sone/src-tauri/src/scrobble/queue.rs:69,
ref:sone/src-tauri/src/tidal_report/queue.rs:56). **Copy the theme-file pattern for every persisted
secret/settings/state file, not the settings-file pattern**: temp file in the same dir → write →
`fsync(file)` → rename → `fsync(dir)`; mode `0600` for secrets, `0644` otherwise. Once a format has
shipped v1, make a failed magic-header check a hard typed error, not silent passthrough, and add a
test that truncates the encrypted file at every byte offset and asserts a clean typed error — never
a silent re-login.

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

## 7. Multi-process token, device and cache/database ownership

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

**Extend the same decision to the disk cache and any local database — not just tokens.** The
`config-cache-logs-telemetry.md` §2 cache keeps a `total_disk_usage` counter and does LRU eviction
against it; the settings file and any future local library/queue database are the same shape of
shared, mutable, on-disk state. Two processes doing LRU eviction against the same cache directory
with no coordination will double-evict, race on the same `.meta` sidecar, and disagree about usage.
sone's cache is single-process by construction — no file locking at all, safe only because
`tauri_plugin_single_instance` guarantees a single process
(ref:sone/src-tauri/src/cache.rs:188-189,608-645; ref:sone/src-tauri/src/lib.rs:546-548).
mopidy-tidal's audio cache instead gets multi-process safety close to free via SQLite + WAL
(ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/cache.py:337-352). Resolve this in the same decision
as the token owner: either (a) the daemon owns tokens, cache and any local database and the GUI is
a thin client (tidalt's model, ref:tidalt/docs/client-server.md), or (b) every shared store is made
multi-writer-safe explicitly (SQLite+WAL, an advisory lock around cache eviction, atomic rename for
every write — §3's atomic-write pattern applies here too). Also decide account-switch behaviour
now: namespace the cache by user id, or purge it on logout — otherwise account A's cached library
can be served to account B (see §8).

## 8. Logout, account switching, and complete data deletion

§1-7 cover storing secrets in detail and say nothing about removing them. For a project positioned
on "no telemetry, sends nothing to its developers" and whose cache holds a subscriber's library and
listening data, "log out" and "delete everything about me" are correctness/privacy features, and
getting the ordering wrong produces concrete bugs: account A's cached playlists served to account
B, or the track playing at logout getting scrobbled to TIDAL after the user has signed out.

sone's `logout` command gives a copyable ordering, with its own comments explaining *why*
(ref:sone/src-tauri/src/commands/auth.rs:413-462):

1. Disconnect scrobbling/play-reporting **first** — "before stopping playback so the interrupted
   track is not scrobbled".
2. Stop playback, tear down the pipeline; clear MPRIS and Discord now-playing state.
3. Disconnect Discord RPC.
4. Shut down the local control server if one is running (see `i18n-a11y-observability.md` §7).
5. Release the idle inhibitor.
6. Clear in-memory tokens and reset `country_code`, while **deliberately preserving** the user's
   own client-id/secret (§4) for the next login.
7. Null out `auth_tokens`, `last_track_id` and scrobble credentials in settings and re-save (or
   delete the settings file outright if it fails to load).
8. Clear the entire disk cache.

What sone does **not** do, and streamboat should decide explicitly: it never deletes the
OS-keyring entry or the `0600` key file, so the AES master key outlives the logged-out session.
High Tide's equivalent is one call, `Secret.password_clear_sync(self.schema, {}, None)`
(ref:high-tide/src/lib/secret_storage.py:85-94, ref:high-tide/src/window.py:267-277).

For streamboat: implement `logout` in the sone ordering above, and separately ship a
`streamboat purge` / "Delete all local data" action that removes config, cache, state/logs, the
keyring entry, and the key file — strictly stronger than logout. Document what a distro package
uninstall does and does not remove (never the user's config/cache/data directories, in this
survey).
