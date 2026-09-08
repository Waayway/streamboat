# Config, cache, data, logs, telemetry

Full source: `docs/research/engineering-baseline.md` §4 (fact-checked). `ref:<project>/<path>`
points at a shallow clone — see `sources.md`.

## Contents

1. Paths
2. Cache design and caps
3. Settings-file versioning and migration
4. Logs and rotation
5. Telemetry and crash reporting

## 1. Paths

Use the platform convention, not XDG-everywhere. The canonical mapping (verified by direct fetch
of `github.com/dirs-dev/directories-rs`'s README, which encodes the same rules any language's
equivalent library follows):

| Purpose | Linux | Windows | macOS |
|---|---|---|---|
| config | `$XDG_CONFIG_HOME` or `~/.config` + `/streamboat` | `{FOLDERID_RoamingAppData}\streamboat\config` | `~/Library/Application Support/<bundle-id>` |
| cache | `$XDG_CACHE_HOME` or `~/.cache` + `/streamboat` | `{FOLDERID_LocalAppData}\streamboat\cache` | `~/Library/Caches/<bundle-id>` |
| data | `$XDG_DATA_HOME` or `~/.local/share` + `/streamboat` | `{FOLDERID_RoamingAppData}\streamboat\data` | `~/Library/Application Support/<bundle-id>` |
| local data | same as data on Linux | `{FOLDERID_LocalAppData}\streamboat\data` | same as data |
| state (logs) | `$XDG_STATE_HOME` or `~/.local/state` + `/streamboat` | no equivalent → LocalAppData | no equivalent → `~/Library/Logs/<bundle-id>` |
| runtime | `$XDG_RUNTIME_DIR/streamboat` | none | none |

Notes and traps:

- **Roaming vs local matters on Windows.** Config belongs in Roaming; caches and logs must go in
  Local, or they get synced across machines on a domain profile.
- **macOS has no XDG.** `~/Library/Application Support` for config *and* data, `~/Library/Caches`
  for cache, `~/Library/Logs` for logs. Use the bundle identifier as the directory name.
- **Do not put the cache inside the config directory.** Sone does — `<config>/sone/cache` and
  `<config>/sone/logs` (ref:sone/src-tauri/src/lib.rs:291-297, 523-531) — which means a 2 GB cache
  lands in a roaming/backed-up location. Learn from it rather than copying it.
- **Under Flatpak everything is redirected** to `~/.var/app/$FLATPAK_ID/{config,cache,data}`
  automatically for XDG dirs (verified by direct fetch of the flatpak sandbox-permissions doc); a
  hardcoded `~/.foo` needs `--persist=.foo`, which bind-mounts to `~/.var/app/$FLATPAK_ID/.foo`.
  Use the XDG APIs and this is free.
- **Under Snap**, `$SNAP_USER_COMMON` / `$SNAP_USER_DATA`; the `home` plug is what sone requests
  (ref:sone/snap/snapcraft.yaml).
- Support an override: `STREAMBOAT_CONFIG_DIR`, `STREAMBOAT_CACHE_DIR`, `STREAMBOAT_DATA_DIR`,
  `STREAMBOAT_LOG_DIR`, plus `--config-dir` etc. on the CLI. Headless/server deployments and the
  test suite both need it, and mopidy-tidal's integration tests do exactly this
  (`-o core/cache_dir=... -o core/data_dir=...`, ref:mopidy-tidal/integration_tests/util.py).

## 2. Cache design and caps

Sone's tiering is a good starting point (ref:sone/src-tauri/src/cache.rs):

| Tier | Contents | TTL | Stale-while-revalidate grace |
|---|---|---|---|
| UserContent | playlists, favourites, liked tracks | 15 min | 1 h |
| Dynamic | artist bios, charts, home page | 4 h | 24 h |
| StaticMeta | album tracklists, credits | 7 d | 30 d |
| Image | album art, avatars | 30 d | 90 d |

Plus: `MAX_DISK_BYTES = 2 GiB`, `EVICT_TARGET = 90%` of that, LRU eviction by `last_access`, keys
hashed with SHA-256 into `<tier-subdir>/<hash>.dat` + `.meta`, entry metadata carrying
`schema_version`, `tags`, `tier`, `created_at`, `size`, and a `CacheResult::{Fresh,Stale,Miss}`
tri-state so a stale hit is served immediately while a refresh runs.

mopidy-tidal's separate *audio* cache defaults are also worth noting: `playback_cache = false`
(off by default), `playback_cache_max_entries = 1024`,
`playback_cache_buffer_bytes = 16777216` (16 MiB), backed by SQLite with LRU eviction by
`last_used` and a `manual` flag that pins entries
(ref:mopidy-tidal/mopidy_tidal/ext.conf, ref:mopidy-tidal/mopidy_tidal/gstreamer_proxy/cache.py:337-352).

For streamboat:

- Expose the cap in settings with a sane default (2 GiB metadata+images), show current usage
  (sone surfaces `CacheStats` with per-tier counts and MB), and ship a "clear cache" action.
- Keep an **audio buffer cache** strictly separate from the metadata cache, off by default, with
  its own smaller cap, and document plainly that it is a playback buffer for a logged-in
  subscriber and not a library of files. Never write full decrypted tracks to a
  user-browsable directory, never name the feature "download", and do not provide an export path.
  Sone states the boundary in its own metadata: "SONE is a streaming client only and does not
  support offline downloads" (ref:sone/data/io.github.lullabyX.sone.metainfo.xml).
- Cache entries must carry a `schema_version`; bump it and drop the tier on format change rather
  than trying to migrate. **Cheaper primary mechanism, used by sone**: version the whole cache
  directory (`cache_dir/v{CURRENT_SCHEMA_VERSION}/`, currently `v5`) and on startup delete any
  sibling `v*` directory that is not current — an O(1) migration that reclaims all disk from old
  formats in one step (ref:sone/src-tauri/src/cache.rs:190,200-215,267-271). Keep the per-entry
  `schema_version` field too, as a belt-and-braces guard, but make the directory scheme primary.
- Never cache stream manifests across restarts — they expire and a stale manifest is a confusing
  failure. The ~1-hour figure comes from source, not documentation: tidal-sdk-web's own client-side
  constant is `const MANIFEST_EXPIRATION_MS = 3600000; // 1 hour`
  (ref:tidal-sdk-web/packages/player/src/internal/helpers/playback-info-resolver.ts:79), used to
  stamp an `expires` timestamp and trigger "a complete reload since manifest has expired"
  (ref:tidal-sdk-web/packages/player/nativePlayer.ts:182). Treat this as the SDK's own client-side
  assumption, not a documented server guarantee — an earlier draft of this document attributed the
  figure to "tidal-sdk-web docs", which do not state it. High Tide's rule is "Cache manifests
  per-session (in-memory) only".
- **sone encrypts every cache entry, not just settings.** §1's storage table (in `SKILL.md`)
  hedges ("Encrypted or plain cache dir, user-configurable"), but the reference implementation is
  unconditional: `<hash>.dat` is written as
  `let encrypted = self.crypto.encrypt(data)?; fs::write(&dat_path, &encrypted)?`
  (ref:sone/src-tauri/src/cache.rs:429-433) — the same AES-256-GCM envelope as
  `secrets-and-tokens.md` §3, so the 2 GiB cap is 2 GiB of AEAD blobs (each with the 17-byte
  header) and the cache subsystem cannot start before the master key resolves. If streamboat
  copies the cap and tiering, copy or explicitly reject the encryption, and state the startup
  ordering (key resolution gates cache init) either way.

## 3. Settings-file versioning and migration

§2 requires a `schema_version` on cache entries; the settings file needs the equivalent, and it is
a harder problem because settings cannot simply be dropped and rebuilt like a cache tier. sone hit
this after shipping and had to retrofit a migration path — its own log lines document it:
`log::warn!("Failed to migrate settings to encrypted: {e}")` /
`log::info!("Migrated settings.json to encrypted format")` (ref:sone/src-tauri/src/lib.rs:346-348).
Put a `version` integer in the settings document from v0.1: make loading forward-only (bump on
change, migrate up, refuse to load a version newer than the running binary and say so rather than
silently overwriting), and add a unit test per migration step with a committed fixture of the old
shape. This is one of the cheapest things to do on day one and among the most expensive to
retrofit.

## 4. Logs and rotation

Copy sone's numbers (ref:sone/src-tauri/src/logging.rs):

- Rotate at 5 MB (`Criterion::Size(5_000_000)`), keep 9 rotated files
  (`Cleanup::KeepLogFiles(9)`) → ~50 MB ceiling, numbered naming, current file
  `streamboat_rCURRENT.log`.
- Default level spec `streamboat=debug,info` (own crate/module at debug, dependencies at info),
  overridable by `RUST_LOG`-equivalent env var **[STACK]**.
- Duplicate to stderr so `journalctl`/console users see the same stream.
- **Initialise the logger before anything else, and read the file-logging toggle from a tiny
  plaintext sidecar** (`<config>/streamboat/logging.toggle` containing literally `true`/`false`),
  because the real settings file is encrypted and not yet loadable at that point. Default to
  enabled on any read failure. Sone's implementation and its six unit tests for the toggle parser
  are a complete model.
- On every filesystem error, fall back to stderr-only rather than failing to start.

Log locations: `~/.local/state/streamboat/logs` (Linux), `%LOCALAPPDATA%\streamboat\logs`
(Windows), `~/Library/Logs/<bundle-id>` (macOS). Print the resolved path in `--version`/About so
bug reports can quote it — sone's issue template does exactly that.

## 5. Telemetry and crash reporting

- **Ship with no telemetry, no analytics, no phone-home, and say so in the README, the metainfo
  and the privacy section.** This is both the norm among the references and a differentiator
  against the official client.
- The one legitimate outbound "extra" is play reporting back to TIDAL so Recently Played works.
  Sone has it on by default and disclosed under Settings → Scrobbling (ref:sone/README.md:110).
  Decide explicitly (see SKILL.md's Open decisions) and make it a visible toggle either way.
- **Update check** is a network request; make it opt-out, do it at most once per launch, and never
  send anything but the GitHub API request. Sone's implementation sends only a `User-Agent` and
  fetches the latest release tag (ref:sone/src-tauri/src/commands/updates.rs).
- **Crash reporting: do not run a crash-reporting service.** Options ranked:
  1. **Local crash dumps + a "generate debug bundle" button** (recommended). No server, no
     consent problem, no PII exfiltration risk. See `i18n-a11y-observability.md` §3 for the
     debug-bundle shape.
  2. Self-hosted collector (Sentry self-hosted / GlitchTip) — only with explicit opt-in at first
     run, a documented retention period, and scrubbing of paths, usernames and URLs.
  3. Hosted Sentry — contradicts the no-telemetry stance; avoid.
  Note that a crash dump from an audio app can contain decoded PCM in memory. If you ship
  minidumps, exclude heap by default.
