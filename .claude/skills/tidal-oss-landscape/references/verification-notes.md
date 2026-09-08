# Verification notes — fact-check pass, 2026-09-07

This is the audit trail for every correction and every new finding folded into this skill and
into `docs/research/oss-landscape.md`. Two independent reviewers checked every numbered/quoted
claim in the original research pass against the reference checkouts, the GitHub API, or a direct
fetch. Read this file when you need to know *why* a number in another reference file differs from
what you might remember reading elsewhere, or when you want the primary evidence for a claim
before repeating it in streamboat's own docs.

## Table of contents

1. Refuted claims, corrected (quick index — full detail lives in the file that covers the topic)
2. New findings folded in (quick index — same)
3. The general lesson: comments in reference-project source are not proof of behaviour
4. What is still genuinely uncertain (see also SKILL.md's "Unverified" section)

## 1. Refuted claims, corrected

| # | Original claim | Correction | Where the full detail now lives |
| --- | --- | --- | --- |
| 1 | Sone registers "≈150" Tauri commands in one `generate_handler!` block | **177** commands, `lib.rs:866-1070` (both the block itself and a repo-wide `#[tauri::command` grep agree) | `sone-deep-dive.md` §3 |
| 2 | Sone's Rust side emits a `track-advanced` event and position/state-tick events | **No such event exists.** Real events: `audio-error`, `audio-resampled`, `signal-path-changed`, `track-finished`, `pkce-login-*`, `scrobble-auth-error`, `tray:*`, `mpris:*`. Position is polled by the frontend (`setInterval(syncPosition, 500)`), not pushed | `sone-deep-dive.md` §3 |
| 3 | Sone is "26,721 lines Rust across 62 files" | 26,721 lines / 61 files in `src/` alone; 62 files / 26,724 lines *including* `build.rs` — a prior pass mixed the two denominators | `sone-deep-dive.md` §1 |
| 4 | Sone's disk cache is keyed by "an FNV-style hash" | The **disk** cache is keyed by **SHA-256** (`cache.rs:2,654-658`); FNV-1a is a *different*, **frontend-only** in-memory cache's hash (`src/api/tidal.ts:55-62`) — two caches, two different hash functions, previously conflated | `sone-deep-dive.md` §4 |
| 5 | Sone's gapless branch queue has a deliberately small (~3 s) buffer so it prerolls without fully downloading | Both the branch decoder and branch queue are set to **15 s** — the same as the main DASH path. The "~3 s" figure came from a stale in-source docstring (`audio.rs:249-250`) that no longer matches the code next to it (`audio.rs:266-273`) | `audio-engineering.md` §2 |
| 6 | Sone's frontend has "40 hooks" in `src/hooks/` | **35** hook files (`~90` components is close enough to the actual 85 to leave as an approximation) | `sone-deep-dive.md` §1-§2 |
| 7 | Strawberry: "Platforms: Linux, macOS, Windows, BSD" with "Bit-perfect: Yes (general engine)" | Strawberry's own README scopes bit-perfect to **Linux only**; macOS/Windows **binary releases are sponsor-only**. There is no dedicated bit-perfect code path at all (`rg -i 'bit.perfect'` over `src/` returns nothing) — what exists is an inferred Linux `hw:`/`plughw:` exclusive flag (affects only crossfade suppression) and an explicit Windows-only WASAPI-exclusive setting; `osxaudiosink` has no exclusive path at all | `project-profiles.md` §2, `audio-engineering.md` §4, `comparison-tables.md` |
| 8 | tidal-connect ships "40+" per-DAC `asound.conf` presets | **26** confirmed in `userconfig/`; 44 total only if the 18 in a separate `samples/` directory are also counted | `project-profiles.md` §6 |
| 9 | `ref:tidalt/internal/player/alsa.c` is "263 lines" of minimal ALSA C | 263 lines is the *combined* total of `alsa.c` (111 lines) **+** `avcodec.c` (152 lines) — a prior pass attributed the whole figure to `alsa.c` alone | `project-profiles.md` §10 |
| 10 | tidalgo's "last activity" is a single date | Two different metrics disagree: last commit is 2018-02-06; GitHub API `updated_at` is 2019-05-09. Cite one and label which metric it is | `project-profiles.md` §13 |
| 11 | High Tide's on-disk track cache is "opt-in" (triggered by a user-chosen music directory) and is "functionally a downloader with a player attached" | `utils.MUSIC_DIR` is **unconditional** (`~/.cache/high-tide/music/`), with **no GSettings key to disable it** — not opt-in for anyone. It **is** bounded: a startup thread LRU-evicts by access time down to 5 GB. Net accurate description: an always-on, unencrypted, non-consented, **size-capped** full-quality media cache with zero user visibility — narrower than "downloader with a player attached," but still the riskiest default in the set | `project-profiles.md` §1 |
| 12 | Sone has "general bit-perfect support" comparable across the whole app | See #7 — this was really about Strawberry, restated here because both the main report's summary and its comparison table repeated the same overstatement independently | `comparison-tables.md` |
| 13 | The 2026-03-21 unofficial-client-ID breakage was "widely-reported" | Tidal-Media-Downloader#1213 is confirmed to exist and be dated correctly, but is **one reply-less GitHub issue about one public gist of keys** — treat as one data point, not a trend | §2 below, `api-auth-streaming.md` §1 |
| 14 | Sone's own comment says `legacy_auth_notice_count` "never resets" | The code resets it to `0` in two places (`commands/auth.rs:395,561`) — the comment is stale, not the behaviour | `sone-deep-dive.md` §4, §3 below |

## 2. New findings folded in

These did not correct an existing claim; they filled gaps the original research pass left open,
or answered open questions the original pass had marked unresolved. Each is filed in the
reference file that owns its topic — this is just the index and the one-line "why it matters":

| Finding | Why it matters | Filed in |
| --- | --- | --- |
| Sone's own Flathub manifest has no `--device=all`/raw ALSA grant | Answers what was Open question 14 — confirms the Flatpak/bit-perfect conflict against the owner's own named reference, not just against High Tide | `packaging-distribution.md` §2 |
| librespot: a core+`Sink`-trait+many-clients precedent exists for pure-Rust audio (Spotify, not TIDAL) | Refutes "pure Rust has zero precedent" as a blanket claim; narrows the real gap to TIDAL-specific DASH/BTS handling | `audio-engineering.md` §5 |
| `cpal` cannot do WASAPI exclusive mode at all | Concrete cost of the pure-Rust option on Windows — a hand-written `wasapi`-crate backend is needed regardless of decode library choice | `audio-engineering.md` §5 |
| CamillaDSP and MPD's `OSXOutputPlugin.cxx` both implement CoreAudio hog mode | macOS exclusive output is a scoping decision, not an unresearched gap — two usable external precedents exist | `audio-engineering.md` §6 |
| Sone's queue/shuffle/repeat/history live in the React webview, not in Rust | The single biggest architecture lesson: streamboat must invert this so its core owns session/queue/transport | `sone-deep-dive.md` §3 |
| Sone has no auto-updater and no build/test/lint CI at all | Forces an explicit per-channel update-mechanism decision and a from-scratch CI plan, even if Sone's packaging *scripts* are reused | `sone-deep-dive.md` §5-§7, `packaging-distribution.md` §3 |
| `gstreamer1.0-libav` bundling carries LGPL/FFmpeg obligations if bundled rather than left as a distro dependency | An unpriced licensing decision in the original GStreamer recommendation | `packaging-distribution.md` §4 |
| No reference project correctly handles a Dolby Atmos-tagged track end to end | A real user-facing failure mode with no defined streamboat policy yet | `api-auth-streaming.md` §9 |
| Sone has zero i18n; High Tide gets 8 locales for free via gettext/Meson | A day-one framework decision the original pass never surfaced | `packaging-distribution.md` §5 |
| Three known OAuth-redirect-capture solutions exist (custom scheme, loopback server, manual paste), not one | Worth naming as a single deliberate decision instead of rediscovering it three times | `api-auth-streaming.md` §3 |
| The catalogue/browse surface (~70% of what a TIDAL client is) is nearly absent from the audio/auth-centric original report | A scope gap in the original document, now flagged and partially filled with what the checkouts confirm | see `docs/research/oss-landscape.md` §18-I and `docs/research/tidal-client-features.md` for the full feature-level treatment |
| Sone's synced-lyrics timestamp regex accepts a colon *or* a dot before the fractional part | A strict `[mm:ss.xx]`-only parser silently drops real TIDAL lyric lines | `sone-deep-dive.md` §5 |
| Contributor counts are not obtainable from this environment (shallow clones + proxy refusal) | Explains why "bus factor" claims throughout this skill are qualitative, not a hard number | §4 below |

## 3. The general lesson: comments in reference-project source are not proof of behaviour

Three separate, independently-discovered instances of the same failure mode, all in Sone's own
source: the "~3 s" gapless buffer-duration docstring (item 5 above) doesn't match the 15 s the
code actually sets; the "never resets" comment on `legacy_auth_notice_count` (item 14 above)
doesn't match the two call sites that reset it to 0; and the README's "requires GStreamer 1.24+"
claim for gapless support isn't enforced anywhere — the actual runtime check is just
`gst::ElementFactory::find("concat").is_some()` (see `audio-engineering.md` §2).

**Practical rule for anyone (human or agent) reading these checkouts further**: when a numeric
parameter, a version requirement, or a behavioural claim comes from a comment or a docstring,
find and read the expression that actually implements it before repeating the number elsewhere.
This is not a one-off Sone quirk to shrug off — it is the single most common failure mode this
fact-check pass found, and it will recur in whatever streamboat's own codebase becomes if the
same discipline isn't applied there too.

## 4. What is still genuinely uncertain

See `SKILL.md`'s "Unverified" section for the concise list to keep in mind while writing specs
or code. The two structurally-important ones, expanded:

- **The entire `tidal.com` domain is blocked from this research environment** — not only
  `developer.tidal.com`/`support.tidal.com`. `https://tidal.com/content-guidelines` returns
  `EGRESS_BLOCKED` exactly like the developer subdomain. Every quote from TIDAL's own policy text
  anywhere in this skill or the main report is second-hand (a GitHub discussion quoting the
  guidelines verbatim, or a search-result summary). Before any of it goes into a user-facing
  legal/README section, someone must read, from an unproxied network:
  `https://developer.tidal.com/documentation/guidelines/guidelines-developer-guidelines`,
  `.../guidelines-developer-terms-2_0`, and `https://tidal.com/content-guidelines` directly, and
  record retrieval dates.
- **Contributor counts per project are not obtainable from this environment.** The reference
  checkouts are `--depth 1` shallow clones, so `git log --format=%an | sort -u` returns exactly
  one author per repo regardless of the real number — this is an artifact of clone depth, not a
  fact about any project. This session's proxy also refuses unauthenticated GitHub
  contributor-endpoint calls. Method for whoever runs this next: `GET /repos/{owner}/
  {repo}/contributors?per_page=100&anon=1` after attaching each repo via the GitHub connector, or
  read each repo's Insights → Contributors page directly. Until then, the qualitative signals
  already gathered are the best available proxy (Sone: single primary author, no CI, 64 open
  issues; High Tide: community project with a public Matrix channel and 8 translator-supplied
  locales; Strawberry: 341 forks, 21 open issues, near-daily commit activity) — treat "single
  author" claims throughout this skill as this same qualitative signal, not a verified headcount.
