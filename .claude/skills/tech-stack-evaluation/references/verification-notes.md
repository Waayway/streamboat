# Verification notes — the fact-check audit trail

Two independent fact-check/gap-finding passes ran against `docs/research/tech-stack.md` on
2026-09-07 (an overall quality score of 4/5 each). This file is the audit trail: every refuted claim
with its correction, and the recurring lessons. All corrections listed here are already applied to
`docs/research/tech-stack.md` itself and folded into this skill's other reference files — this file
exists so a future agent can see *why* a number changed, not just that it did.

## Contents

- §1 Refuted claims and their corrections
- §2 Recurring lessons
- §3 Domains blocked from the research environment
- §4 Items still marked "uncertain" — do not upgrade to fact without re-checking

---

## §1 Refuted claims and their corrections

| Claim (as originally written) | Correction | Where fixed |
| --- | --- | --- |
| Strawberry's `src/` is 14,708 lines of C++/headers | **~166,122 lines** (491 `.cpp` = 121,981 lines + 568 `.h` = 44,141 lines, 40 subdirectories; `src/core` alone 21,510 lines) — an ~11x undercount that changes Strawberry from "comparable to SONE" to "by far the largest codebase in the survey" | tech-stack.md Summary, §4.11; `scoring-and-candidates.md` §2 (4.11); `real-world-players.md` |
| sone-windows' delta versus SONE is "almost entirely the audio sink" | sone-windows forks an **older** SONE release (v0.16.0: 15,753 Rust + 28,518 TS/TSX) vs. current upstream (v0.21.0: 26,721 Rust + 47,609 TS/TSX) — ~59% of upstream's current size, five minor releases behind. Not a thin per-OS delta; a stale fork. | tech-stack.md Summary; `sources.md`; `real-world-players.md` |
| cpal 0.17.2 added resampling; **0.18.0** "output streams no longer reject formats the built-in resampler can convert"; 0.17.0 `device_by_id()` accepts `hw:0,0` | 0.17.2 (**yanked**) added the resampling change; **0.18.2** (not 0.18.0) added the format-rejection change; **0.18.0** (not 0.17.0) added ALSA shorthand (`hw:0,0`/`plughw:foo`) acceptance — 0.17.0 only introduced `device_by_id()` itself, generically | tech-stack.md Summary, §2.2 table, Sources; `audio-engine-comparison.md` §3 |
| Wails v3's latest prerelease is `v3.0.0-beta.9` | **`v3.0.0-beta.17`** (2026-09-06) — eight betas stale | tech-stack.md §4.14, Sources; `scoring-and-candidates.md` §2 (4.14) |
| Masonry's recent release is v0.2.0 | **v0.4.0** (2025-10-29); v0.2.0 was 2024-05-07, over two years stale | tech-stack.md §4/Xilem mention, Sources |
| Xilem's README says "plenty of missing features" | Verbatim wording is **"Lots of things need improvements"** | tech-stack.md, Sources |
| `relm4`/gtk4-rs licence is MIT | **Apache-2.0 OR MIT** (crates.io's actual field) | tech-stack.md §9; `packaging-and-policy.md` §2 |
| Nuclear/Museeks are both "Electron + React" | **Museeks is Tauri 2 + Rust + React**, ported from Electron in 2024; its `src-tauri/Cargo.toml` has no audio crate — playback stays in the webview, making it a *second* Tauri music-player precedent and a negative control (webview UI without a Rust audio path is not enough) | tech-stack.md §6; `real-world-players.md` |
| GStreamer Windows path is settled: `wasapi2sink exclusive=true` "verified working" | Strawberry's own `gststartup.cpp:59-69` demotes `wasapi2sink` to `GST_RANK_SECONDARY` and ranks `directsoundsink` `GST_RANK_PRIMARY`, citing device-switching issues (#1227) — direct counter-evidence the original report never surfaced despite having Strawberry checked out | tech-stack.md §2.2, §2.3, §4.11; `audio-engine-comparison.md` §4 |
| tidalt's ALSA format-preference order is documented in `README.md` | Not documented there at all — only in `alsa.c`/`CLAUDE.md`. Worse, tidalt's own `docs/architecture.md:43` states a **different, stale** 16-bit order, contradicting the code | tech-stack.md §2.1; `audio-engine-comparison.md` §7 |
| Cross-cutting decision: "TIDAL manifests expire (~1 hour) and segment URLs within ~24 hours" | **No source found for either number** anywhere in the checkouts — restated without invented figures | tech-stack.md, cross-cutting decisions |
| Flathub's live requirements doc quote omits "Disclosure does not create a presumption of acceptance" | Sentence added — it is the one that makes "Flathub remains reachable" an evidence-backed conclusion rather than an optimistic reading | tech-stack.md §9; `packaging-and-policy.md` §3 |

## §2 Recurring lessons

- **A project's own docs can be stale relative to its own code.** tidalt's `docs/architecture.md`
  disagrees with `internal/player/alsa.c` on the ALSA format order. When a project's README/docs and
  its source disagree, cite the source, and say so explicitly — don't silently pick one.
- **`xargs wc -l | tail -1` silently truncates on a large file list.** This is very likely how the
  original Strawberry count went wrong: when the argument list is large enough that `xargs` batches
  it into multiple `wc -l` invocations, each batch prints its own "total" line, and piping through
  `tail -1` shows only the *last* batch's total — not the grand total. Use
  `find ... -print0 | xargs -0 cat | wc -l` (or `find ... -exec cat {} +  | wc -l`) instead when
  recounting a large tree.
- **A registry API beats a blog post for version claims.** Both version-staleness refutations here
  (Wails, Masonry) were one `crates.io`/`github.com/.../releases` request away from being caught.
  Prefer `https://crates.io/api/v1/crates/<name>` and `https://github.com/<org>/<repo>/releases`
  over a cached blog citation for anything version/date/licence-shaped.
- **Being the reference project doesn't make a claim self-evidently true.** The original report had
  SONE/sone-windows checked out and *still* missed that Strawberry's own GStreamer sink-ranking code
  directly contradicts the `wasapi2sink`-exclusive recommendation. Cross-check a "verified" claim
  against every other checkout that touches the same subsystem, not just the one that motivated it.
- **A specific-sounding number is not automatically sourced.** The "~1 hour manifest / ~24 hour
  segment" figures read as precise engineering knowledge but had no citation anywhere in the
  checkouts. When a number can't be traced to a specific line, state the qualitative rule instead of
  inventing a plausible magnitude.

## §3 Domains blocked from the research environment

`v2.tauri.app`, `gstreamer.freedesktop.org`, `ui.shadcn.com`, `apps.gnome.org`, `docs.gtk.org`,
`mozilla.github.io`, `webdriver.io`, `avaloniaui.net`, `fyne.io`, `slint.dev`, `gamingonlinux.com`,
`linuxiac.com`, `opensourceforu.com`, and the entire `tidal.com` domain all returned blocked or
failed fetches at some point during this research (both the original pass and the fact-check pass).
Where the primary page is blocked, this skill relies on GitHub raw-file mirrors of the same content
where they exist (this worked for every `tauri-apps/tauri-docs` page cited in
`tauri-engineering-facts.md`), or flags the claim as "web, unverified" when no mirror exists.

## §4 Items still marked "uncertain" — do not upgrade to fact without re-checking

- Press-reported Flathub AI-ban timeline (2026-05-29 effective date, non-retroactivity) — secondary
  reporting only, source domains unreachable twice.
- KMP desktop audio library survey (gadulka/AstroPlayer/kmp-audio-recorder-player/KorAU as "simple
  wrappers") — underlying reasoning is sound, specific library list unverified.
- shadcn/ui's Tailwind v4/React 19 support and the `agmmnn/tauri-ui` scaffold — `ui.shadcn.com`
  unreachable; SONE's own choice (no component library at all) *is* verified.
- Music Assistant client package's "split out in October 2024" — no date found in either README.
- GStreamer's macOS deployment page (PackageMaker-style bundle) — domain blocked, Windows deployment
  page was read directly but not the macOS one.
- Tauri's "not all desktop plugins ported to mobile" and the macOS notarization mechanism — rest on
  the docs' own maturity language and secondary summaries (`v2.tauri.app` blocked), not a page read
  directly, unlike the updater/WebView2/macOS-bundle/IPC/mobile-crate facts in
  `tauri-engineering-facts.md`, which were read from GitHub raw mirrors and are higher-confidence.
- Avalonia 12's claimed 3× Android performance improvement — same confidence tier as the (already
  flagged) Cider Tauri-migration rumour.
