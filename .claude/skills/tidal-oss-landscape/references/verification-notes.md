# Verification notes — three fact-check passes, 2026-09-07 and 2026-09-08 (×2)

This is the audit trail for every correction and every new finding folded into this skill and
into `docs/research/oss-landscape.md`. The research has been through **three** independent
fact-check passes: the first (2026-09-07, two reviewers) checked every numbered/quoted claim in
the original research pass; the second (2026-09-08) re-checked the *first pass's own corrections*
against source, the GitHub contributors API, and two more headless-precedent projects fetched
directly from GitHub — and found that the first pass had itself introduced several errors while
"correcting" the original draft (see item 2 in the table below, and §1b); the third (2026-09-08,
same day, a distinct pass) re-verified roughly 120 individual claims carried over from the first
two (118 held up; two did not — see §1c) and, more consequentially, went looking for facts already
sitting in the reference checkouts that neither prior pass had surfaced — 17 of them (§2c),
including the queue data model, playlist-mutation ETag preconditions, the rate-limit contract's
actual numbers, and the full identity Sone's play-reporting impersonates. Read this file when you
need to know *why* a number in another reference file differs from what you might remember
reading elsewhere, or when you want the primary evidence for a claim before repeating it in
streamboat's own docs.

## Table of contents

1. Refuted claims, corrected — first pass (2026-09-07)
1b. Refuted claims, corrected — second pass (2026-09-08), including the first pass's own errors
1c. Refuted claims, corrected — third pass (2026-09-08), including the second pass's own errors
2. New findings folded in — first pass
2b. New findings folded in — second pass
2c. New findings folded in — third pass (17 items)
3. The general lesson: comments in reference-project source are not proof of behaviour
4. What is still genuinely uncertain (see also SKILL.md's "Unverified" section)

## 1. Refuted claims, corrected

| # | Original claim | Correction | Where the full detail now lives |
| --- | --- | --- | --- |
| 1 | Sone registers "≈150" Tauri commands in one `generate_handler!` block | **177** commands, `lib.rs:866-1070` (both the block itself and a repo-wide `#[tauri::command` grep agree) | `sone-deep-dive.md` §3 |
| 2 | *(original draft)* Sone's Rust side emits a `track-advanced` event | *(first pass "corrected" this to "no such event exists" — that correction was itself wrong; see §1b item 2b below for the real answer.)* | `sone-deep-dive.md` §3 |
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

## 1b. Refuted claims, corrected — second pass (2026-09-08)

| # | Claim (some are the *first pass's own* errors) | Correction | Where the full detail now lives |
| --- | --- | --- | --- |
| 2b | First pass: "Sone emits no `track-advanced` event" | **Wrong — Sone does emit `track-advanced`**, `{trackId, qid, replayGain, peakAmplitude}`, from `audio.rs:2671-2682` on every gapless advance, consumed by Rust itself for scrobbling (`lib.rs:705-712`). What remains true: there is no periodic position/state-tick event — `track-advanced` is a discrete boundary event, not a position stream. Full event set also gained `audio-bit-depth-changed` (`audio.rs:852`), previously omitted | `sone-deep-dive.md` §3 |
| 15 | Strawberry's TIDAL code is "~2,709 lines across 12 files" | Mismatched denominators: 2,709 lines is the **6 `.cpp` files only**; all 12 `.cpp`+`.h` files total **3,429 lines** | `project-profiles.md` §2 |
| 16 | §1.4/DASH: "two consumption strategies observed" | The original text's own bullet list already had three; a fourth (Music Assistant's ephemeral local HTTP route) was found by the second pass. **Four strategies total**, with a selection rule (don't use a `data:` URI when the media layer may re-open the source) | `api-auth-streaming.md` §5 |
| 17 | tidalt: "release the PipeWire device reservation on stop" | tidalt's own README says it releases **on pause**, not only on stop (`ref:tidalt/README.md:12`) — the more cooperative and correct policy | `audio-engineering.md` §7, `project-profiles.md` §10 |
| 18 | "Atmos handling is undefined everywhere in the reference set" | Too broad — no project *decodes* Atmos/360RA end to end, but TidaLuna does implement real metadata/quality handling for spatial tracks (`Quality.Atmos`/`Quality.Sony630`, a live `playbackinfo` re-check for spatial-only tracks). Narrow the claim to "no decode precedent," add `SONY_360RA` to any tag set | `api-auth-streaming.md` §9, `project-profiles.md` §4 |
| 19 | "Sone's `legacy_auth_notice_count` never resets (comment says so); cap is unstated" | The doc comment claims cap 5 and "never resets" — **both wrong**: real cap is `LEGACY_AUTH_NOTICE_LIMIT: u8 = 3`, and the counter *is* reset to 0 at `auth.rs:395,561` | `sone-deep-dive.md` §4 |
| 20 | "Contributor counts are not obtainable from this environment" | **Now obtainable** — see §2b below and `sone-deep-dive.md` §10 for real numbers | §4 below |

## 1c. Refuted claims, corrected — third pass (2026-09-08)

| # | Claim (some are the *second pass's own* errors) | Correction | Where the full detail now lives |
| --- | --- | --- | --- |
| 21 | Second pass: "Sone's position is polled, not pushed — every consumer (main UI, miniplayer, overlay, signal-path panel) polls independently on its own `setInterval`" | **Wrong.** Sone already implements anchor-poll-plus-interpolate: `playbackPosition.ts` polls the backend **once** every 2 s; every consumer (incl. the 500 ms progress-bar timer) reads a **local interpolation** of that single anchor; the miniplayer/overlay are **pushed to** at 1 s from the main webview, not polled. The real, previously-undocumented mechanism is a **settle-window guard**: a polled position too far ahead of the interpolated value within 3 s of a track change is discarded, because a `concat` gapless boundary makes GStreamer briefly report the *previous* track's cumulative runtime | `sone-deep-dive.md` §3 |
| 22 | Comparison table A: "Sone-windows platforms: Windows (+Linux/mac via Tauri)" | **The macOS half is false.** Every sink-construction/device-enumeration branch in `sone-windows/src-tauri/src/audio.rs` is `#[cfg(target_os = "linux")]` or `#[cfg(target_os = "windows")]` only — there is **no macOS arm**, and `souvlaki` is declared only under the Windows target. Correct reading: Windows-only (Linux paths retained from upstream and still build there). This also means the entire reference set contains **zero** Tauri/Rust macOS TIDAL precedent | `sone-deep-dive.md` §9, `comparison-tables.md` §1, `packaging-distribution.md` §8 |

Two smaller wording corrections surfaced by the same pass, not worth a full row: tidalrs's "it
deliberately stops at return-a-manifest/URL, does not decrypt/download/play" downgrades to
"contains no such code" — there is no README or source statement declaring this a policy
(`project-profiles.md` §11); and mopidy-tidal's login-hack QR-leak note (already in this skill)
gets a second, previously-undocumented sibling — the same file also calls `api.voicerss.org` with
a hardcoded API key for TTS (`project-profiles.md` §5, `api-auth-streaming.md` §3).

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
| Contributor counts are not obtainable from this environment (shallow clones + proxy refusal) | *(superseded — see §2b: this became obtainable in the second pass)* | §4 below |

## 2b. New findings folded in — second pass (2026-09-08)

| Finding | Why it matters | Filed in |
| --- | --- | --- |
| Contributor counts are now obtainable (`GET /repos/{owner}/{repo}/contributors?per_page=100&anon=1`); real numbers: Sone 18 (97% one author), High Tide 45 (68% one author), mopidy-tidal 12 (3-person history), Strawberry ~5 significant, tidalrs 4 (89% one author) | Resolves Open question 18. Reveals every project but High Tide and mopidy-tidal is bus-factor 1 — including Strawberry, whose "very mature" rating this skill otherwise repeats uncritically | `sone-deep-dive.md` §10, `comparison-tables.md` §1 |
| Sone's seek command must not detach the armed gapless next-track slot; bit-perfect-path position is derived from frames written, not a GStreamer query | Seeking was entirely absent from the original document despite being the transport operation most likely to break both `concat` gapless and the ALSA writer | `sone-deep-dive.md` §3a, `audio-engineering.md` §2 |
| Sone's gapless *arming* policy (`useGaplessPrefetch.ts`) — does not gate on `isPlaying` because of a device-busy retry loop, dedups/coalesces/negative-caches prefetch requests | The frontend half of gapless an implementer must design from scratch; the original document only covered the Rust-side `concat` plumbing | `sone-deep-dive.md` §3b |
| Sone-windows' Windows GStreamer bundle has 16 plugins and **no AAC decoder at all** (`gstlibav` excluded) | Converts an abstract "does the bundle need libav" licensing question into a concrete codec-coverage cost: an LGPL-clean Windows bundle can't play TIDAL's HIGH/LOW (AAC) tiers | `sone-deep-dive.md` §9, `packaging-distribution.md` §4 |
| Music videos are not in Sone's GStreamer/ALSA engine at all — they're `hls.js` in the webview, a second media path that disables gapless | Video was untreated anywhere in the original document despite the brief's "everything the native client does" | `sone-deep-dive.md` §5 |
| Play-reporting wire format: `POST ec.tidal.com/api/event-batch`, `playbackSessionId` must be minted at stream-resolution time (same id as the official SDK's `x-playback-session-id`) | The original document named the *capability* with no endpoint/payload, so the owner's play-reporting decision (Open decision 6) had nothing to cost it against | `sone-deep-dive.md` §5 |
| Music Assistant (fourth DASH strategy: ephemeral local HTTP route; track-ID-churn recovery) and lms-plugin-tidal (per-request `_nocache` flag; a 429-as-auth-failure anti-pattern) — both listed "unverified, not read" in the original document — were fetched directly from GitHub and read | Two more headless-server precedents beyond mopidy-tidal, with transferable mechanisms neither the original document nor High Tide/Sone demonstrate | `project-profiles.md` §5a/§5b |
| tidalt's daemon/client split mechanics: D-Bus name-claiming as the mutex+discovery mechanism, `tidalt play <url>` one-shot client mode, lazy device acquisition | The original document cited `tidalt/docs/client-server.md` as an artifact but never actually explained the mechanism it recommends streamboat adopt (Implication 7) | `project-profiles.md` §10 |
| TIDAL's official Redux action namespace (via TidaLuna) reveals a complete transport/queue/preload/device-switching contract, including `STREAMING_PRIVILEGES_REVOKED`/`ENSURE_PLAYBACK_PRIVILEGES` — no project in the OSS set implements a real-time privileges channel | Free product intelligence the original document left almost entirely unmined; also flags that streamboat's only option today is polling `subStatus 4006`, not a push channel | `project-profiles.md` §4, `api-auth-streaming.md` §6 |
| No project in the reference set signs its Windows/macOS builds, and none ships a signed in-app updater (generalized from Sone alone to the whole set, incl. tidal-hifi which has the best release CI) | Two hard, unbudgeted prerequisites for the owner's Windows+macOS-now requirement | `packaging-distribution.md` §3, §7 |
| Sone's own test suite has zero HTTP/ALSA mocking (`[dev-dependencies]` is one line, `tempfile`); no project in the set demonstrates a fixture/mock testing strategy except tidalrs (has `mockito`) and Music Assistant (real provider tests) | "151 tests" reads as more test infrastructure than actually exists; streamboat must design the fixture-capture strategy, not port one | `sone-deep-dive.md` §7 |

## 2c. New findings folded in — third pass (2026-09-08, 17 items)

| Finding | Why it matters | Filed in |
| --- | --- | --- |
| Track-availability pre-flight (`streamReady`/`allowStreaming`/`streamStartDate`) and a narrow playback-error-classification allowlist, with a bounded auto-skip loop | The first implementer question after "how do I get a stream URL"; get it wrong and a network blip auto-skips the whole queue, or a region-blocked track hangs forever | `sone-deep-dive.md` §3c, `api-auth-streaming.md` §11 |
| Playlist/favorites mutation requires an ETag write precondition (`If-None-Match`, default `"*"`), confirmed in two independent implementations, plus `onDupes`/`onArtifactNotFound` semantics | Entirely missing write-path coverage; skip this and every mutation 412s/428s with no obvious explanation | `sone-deep-dive.md` §4b |
| The queue data model is five separate collections (context queue, pre-shuffle original order, manual "play next," history, two distinct source refs), plus a track-vs-album ReplayGain policy flag and a global user-pause-intent flag | Implication 28's highest-ranked recommendation ("core owns the queue") had zero data model until now | `sone-deep-dive.md` §3d |
| Autoplay (radio-mix continuation via `mixes.TRACK_MIX`) and the explicit-content filter, the latter enforced at ten separate call sites | Two shipped features never previously mentioned; both are queue-layer invariants, not UI toggles | `sone-deep-dive.md` §3e |
| `countryCode` bootstraps from `GET /v1/sessions` (default `"US"` before login) and an account-endpoint log-redaction rule | Nobody had documented where `countryCode` comes from, or the concrete redaction pattern for a client whose logs go into bug reports | `sone-deep-dive.md` §4a |
| The rate-limit contract's actual numbers: 5 s default cooldown, 120 s clamp, lock-free `AtomicU64` deadline, delta-seconds-only `Retry-After` parsing | `rate_gate.rs` was named four times elsewhere with no numbers attached | `sone-deep-dive.md` §4c |
| Navigation is one discriminated-union atom with no router library, plus a fully-worked-out scroll-restoration policy (quiet period, hard ceiling, cancel-on-user-input) | The UI-patterns half of this skill had almost nothing; this is the hardest part of a webview browse UI, solved in one file | `sone-deep-dive.md` §3f |
| Play reporting is actually AWS SQS `SendMessageBatch` form encoding that impersonates a specific TIDAL Android device identity (pinned app version/OS/device strings), with JWT claim decoding and a token-stripped-at-rest offline outbox | The real cost of the "should streamboat report plays" decision — maintaining a version pin that drifts roughly as often as TIDAL ships | `sone-deep-dive.md` §5, `oss-landscape.md` §2.6 |
| Scrobble threshold is `listened >= duration/2 \|\| listened >= 240s`, driven by cumulative playtime (not position), confirmed independently in the official client too | Determines whether the offline queue, shutdown path and seek path are correct | `sone-deep-dive.md` §4d |
| Mid-session stream-URL/manifest expiry (armed gapless slot dies during a long pause) is unsolved anywhere in the reference set | Collides directly with this skill's own gapless-arming recommendation; flagged as new engineering, not a "copy from X" item | `audio-engineering.md` §9 |
| The licence decision must be sequenced before the audio module — nearly everything in the top "borrow with pointers" list is GPL-3.0; only permissive projects exist with no bit-perfect path | Ranked lesson (3)/Open decision 3 previously didn't connect "port Sone's ALSA" to "Sone is GPL-3.0-only" as the same decision | `packaging-distribution.md` §6 |
| ETag is a write precondition today; no project uses it for cheap read-revalidation (304) on a GET | A small, free improvement over the `UserContent` 15-minute TTL tier, previously unexploited | `api-auth-streaming.md` §10 |
| No comparative UI/UX assessment exists; the real decision is toolkit-as-theming/accessibility trade-off, not just "beautiful" | The owner's only stated design constraint had nothing comparative to act on | `oss-landscape.md` §19-18 |
| macOS work splits five ways, four of five with zero precedent anywhere in the set (output, media integration, signing, GStreamer bundling; only Keychain has a direct precedent) | "Tauri is cross-platform" was being used as evidence macOS work is free; it isn't | `packaging-distribution.md` §8, `audio-engineering.md` §10 |
| upmpdcli (UPnP/OpenHome), and the librespot frontend ecosystem (spotifyd/ncspot/psst/go-librespot), are named by this project's own sources but were never fetched | Highest-value follow-up reads for the control-protocol and language/runtime open decisions | `project-profiles.md` §14 |
| A fixture inventory (which manifest/error/quality-tier responses to capture from a live account) was implied but never written out | Get the fixture set wrong once and a second live-capture session is needed against a credential-gated API whose interesting failures are transient | `oss-landscape.md` §19-16 |
| Module maps were missing for everything except Sone and High Tide | A reader chasing a recommended borrow (Strawberry's collection schema, tidalt's daemon split) had a filename but no orientation | `project-profiles.md` §15 |

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
- ~~Contributor counts per project are not obtainable from this environment~~ **— resolved by the
  second pass.** `GET /repos/{owner}/{repo}/contributors?per_page=100&anon=1` (after attaching
  each repo) works from a later session. Real numbers, and the bus-factor-1 finding they reveal
  (true of nearly every project, Strawberry included), are in `sone-deep-dive.md` §10 and
  `comparison-tables.md` §1 — do not describe contributor counts as unobtainable in any streamboat
  doc that cites this skill.
