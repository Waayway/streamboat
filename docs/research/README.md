# Research reports index

Date: 2026-09-07. All seven reports below were researched and fact-checked (most through two or
three independent passes) on this date, against read-only shallow clones of 20+ TIDAL-adjacent
open-source projects and whatever web sources were reachable from the research environment (several
TIDAL-owned domains — `tidal.com`, `developer.tidal.com`, `support.tidal.com` — were blocked
throughout; claims sourced from them are flagged `[unverified]`/second-hand inside the reports
themselves). Treat 2026-09-07 as recent but not evergreen: version numbers, pricing, and any
"current as of" claim inside a report should be re-checked before being relied on much later.

Each report has a matching distilled skill under `.claude/skills/<topic>/`. A skill is what an
agent should load while writing code or docs day to day — it carries the pitfall tables, the quick
answers, and pointers back into the report's own `references/*.md` files for depth. **A report is
the source of truth; its skill is a summary.** When a skill and its report disagree, or a skill's
"Known errors in the source report" section flags a correction, the correction (usually recorded in
the skill) wins — but re-read the report section in question rather than assuming the skill's
one-line fix tells the whole story.

If you edit a report, update its matching skill in the same change, and vice versa — see
`CLAUDE.md`'s "Do not" section. If a report's own conclusion conflicts with another report's (the
two flagged conflicts below), do not resolve it unilaterally in code; surface it to the owner, per
`.claude/skills/streamboat-overview/SKILL.md`.

## Reports

### `tidal-client-features.md`
Product specification target for streamboat: an inventory of what TIDAL's native apps (desktop,
web, mobile, TV, Connect) actually do — quality/playback/queue behaviour, browse and page
structure, library/playlists/collections, social/feed/creator features, remote playback and
Connect, entitlements and account tiers — each feature marked with whether the unofficial API
(python-tidal/High Tide/Sone) can reach it and tiered MVP/v1/later/out-of-scope with a rationale.
Built primarily from TidaLuna's extracted Redux action-namespace dump (one build, dated 2026-09-02)
and the official OpenAPI spec vendored in the TIDAL SDK checkouts. Matching skill:
`tidal-client-features`.

### `tidal-api.md`
Wire-level documentation of both TIDAL API surfaces streamboat can talk to: the unofficial
`api.tidal.com` v1/v2 API every full-featured OSS client streams through (OAuth device-code/PKCE,
token refresh/storage, `playbackinfopostpaywall`, manifest/encryption/quality-cascade rules,
catalog/pages/playlist/favorites/lyrics/image endpoints, play-reporting, rate limits and error
shapes), and the official `openapi.tidal.com/v2` Developer API (256 documented paths, usable
opportunistically with the same Bearer token for metadata/playlists/lyrics, but DRM-gated for
playback). Closes with the legal/ToS posture the unofficial-API approach carries. Matching skill:
`tidal-api`.

### `oss-landscape.md`
Architectural precedent from 20+ open-source TIDAL clients and SDKs — Sone, High Tide, Strawberry,
tidal-hifi, TidaLuna, mopidy-tidal, tidalt, tidalrs, python-tidal, tidal-connect, and the official
`tidal-sdk-web`/`-android`/`-ios` — covering auth, stream resolution, manifest/DASH/BTS parsing,
bit-perfect/exclusive audio output, gapless playback, quality cascades, caching, credential
handling, packaging, and the desktop-vs-headless architecture split each project chose. The single
largest report (three independent fact-check passes); its own "one architectural takeaway" is that
no existing project combines a GUI desktop client and a headless daemon from one codebase — the gap
streamboat is meant to fill. Matching skill: `tidal-oss-landscape`.

### `audio-pipeline.md`
How to get audio bytes from a TIDAL manifest to a DAC, source-accurate, on Linux (ALSA
`hw:`/PipeWire), Windows (WASAPI), macOS (CoreAudio hog mode), or a headless/Pi daemon — covering
manifest parsing (BTS/DASH/HLS/EMU), decoder/demuxer choice and licensing (GStreamer, libmpv,
Symphonia, FFmpeg), bit-perfect/exclusive-mode negotiation per platform, gapless playback and
crossfade, ReplayGain/loudness normalization, buffering and on-disk caching, Dolby Atmos/360RA
feasibility (verdict: don't build either for v1), and OS media integration (MPRIS/SMTC/NowPlaying).
Recommends starting engine work with libmpv, which conflicts with `tech-stack.md`'s
GStreamer-first recommendation — both reports flag this explicitly; unresolved. Matching skill:
`audio-pipeline`.

### `headless-connect.md`
Design for streamboat's headless/server/CLI mode: the daemon-vs-GUI core split
(`streamboat-core`/`streamboat-server`), a local control protocol (HTTP+WebSocket JSON, MPRIS, an
MPD-compatible subset, a Unix-socket surface), TIDAL's one-stream-at-a-time "Pushkin"
streaming-privileges websocket, multiroom output (Snapcast, Sendspin), Raspberry Pi/small-device
ALSA deployment, and headless login/pairing UX. Also carries the full TIDAL Connect dissection and
verdict: a Connect **target** is never buildable (closed, obfuscated binary reusing a single
vendor's device certificate); a Connect **controller** is out of scope today (no protocol capture
exists) but not categorically ruled out. Matching skill: `headless-and-tidal-connect`.

### `tech-stack.md`
The researched, fact-checked answer to "what should streamboat be built in" — language, UI toolkit,
audio engine, and process architecture — scored against 14 candidate stacks on a weighted matrix.
Current recommendation: one Rust workspace, a Tauri 2 desktop shell, React 19 + TypeScript +
Tailwind 4 frontend, GStreamer behind an `AudioEngine` trait (libmpv as backend #2), and a headless
daemon binary sharing the same core. Not yet ratified by the owner — see
`docs/research/decision-tree.json` and `streamboat-overview`'s Status section. Its engine-order
recommendation conflicts with `audio-pipeline.md`'s; both reports flag this explicitly. Matching
skill: `tech-stack-evaluation`.

### `engineering-baseline.md`
Stack-agnostic engineering conventions, fact-checked against 13+ reference projects: how to test an
unofficial-API client without live credentials in CI (parser/transport split, fixture capture,
opt-in live canaries), secret/token storage per OS (keyring formats, encrypted-file fallback,
multi-process token races), config/cache/log/telemetry layout, licensing (GPL/LGPL/Apache split and
compatibility with GStreamer/FFmpeg/Qt), packaging and distribution per channel (Flathub, Snap,
deb/rpm/AUR, AppImage, Windows MSI/winget/signing, macOS DMG/notarization/Homebrew), CI job design,
repo layout/CONTRIBUTING/issue templates, i18n/accessibility, and observability. Matching skill:
`streamboat-engineering-baseline`.

### `decision-tree.json`
Not a narrative report — a structured, 13-round decision tree (product scope and platform
commitments, through licensing, architecture, audio engine, packaging, and release process) the
owner works through to turn the six reports above into ratified decisions. Each round's questions
carry researched options, a recommendation, and grounding citations back into the reports and
skills. Its `already_decided` array restates the fixed owner decisions that `streamboat-overview`
and `CLAUDE.md` also carry; its `notes` field records known cross-round dependencies and the two
open conflicts between reports (engine order; whether exclusive ALSA works inside a Flatpak).

## How these relate to the skills

Every report above has exactly one matching skill under `.claude/skills/`, named in its entry. Load
the skill for day-to-day work — it's the load-on-demand distillation meant for writing code and
docs without re-reading a 2,000-3,300 line report each time. Come back to the report itself for:

- full narrative reasoning and every citation behind a claim the skill only summarizes;
- the complete "Unverified" list, not just the subset a skill's pitfall table calls out;
- fact-check audit trails (what a second or third pass corrected, and why);
- anything the skill's own "Where to go for depth" table points at.

`.claude/skills/streamboat-overview/SKILL.md` is the entry point that indexes all seven topic
skills with one line each on when to load them — read it before this file if you haven't already.
