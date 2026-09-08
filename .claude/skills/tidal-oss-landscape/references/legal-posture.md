# Legal / ToS posture — material for streamboat's README and docs/legal.md

Everything here is assembled from the reference projects' own public positioning plus TIDAL's own
policy text as quoted second-hand elsewhere in this skill (see the caveat at the end of this file
— re-read it before any of this goes into user-facing text). Use it as source material when
writing streamboat's README disclaimer or a `docs/legal.md`; do not invent new framing when this
much of it already exists, worded and shipped, in four other projects.

## What the reference projects themselves say

Every project that streams TIDAL content through the unofficial API states, unprompted, that it
is not TIDAL and that the user needs their own subscription. Reuse this framing rather than
drafting from scratch:

- **High Tide**: *"Not affiliated in any way with TIDAL, this is a third-party unofficial
  client"* (`ref:high-tide/README.md:21`).
- **Sone**: *"SONE is an independent, community-driven project. It is **not affiliated with,
  endorsed by, or connected to TIDAL** in any way. All content is streamed directly from TIDAL's
  service and requires a valid paid subscription. SONE is a streaming client only — it does not
  support offline downloads, and does not redistribute or circumvent protection of any content. As
  with any third-party client, please be aware of TIDAL's terms of use."* (`ref:sone/README.md:542`,
  restated more briefly at `ref:sone/README.md:21`: *"Requires an active TIDAL subscription. Not
  affiliated with TIDAL."*).
- **tidalrs**: *"This library is not officially affiliated with Tidal. Use at your own risk and
  ensure compliance with Tidal's Terms of Service."* (`ref:tidalrs/README.md:285`).
- **Fokka-Engineering/TIDAL**: *"I deeply discourage you from building and distributing
  copyright-infringing apps. Create something that adds up to TIDALs Service and improves it."*
  (`ref:tidal-fokka-engineering-/README.md:20`).

**The common shape**: an explicit non-affiliation statement, an explicit "you need your own paid
subscription" statement, and (High Tide, Sone) an explicit "this is a player, not a downloader"
framing. streamboat's own disclaimer should carry all three — it is the project brief in one
sentence: *a player for subscribers, not a downloader/ripper.*

## What none of them say — add these two things

1. **TIDAL's Content Guidelines prohibit reverse-engineering the service.** The load-bearing quote
   ("Player module is the only allowed playback path"), its corroborating evidence, and the
   second-hand-sourcing caveat are owned by `references/api-auth-streaming.md` §1 in this same
   skill — read it there rather than re-quoting it here. This is exactly what building against the
   unofficial API does — say so plainly rather than let a reader discover the tension themselves.
2. **Account suspension is a real, user-facing risk**, not a hypothetical. None of the four
   projects above states this explicitly, but it follows directly from (1): a subscriber running
   streamboat is using a client TIDAL's own guidelines describe as prohibited, on their own paid
   account. Say this in the README rather than only implying it through the non-affiliation
   disclaimer — a user deciding whether to try streamboat should be able to find the actual risk
   in one place, not infer it from a boilerplate legal notice.

## Framing to carry into streamboat's own text

- streamboat is a **player for subscribers**, not a downloader/ripper. This is the owner's own
  brief and matches Sone's explicit "streaming client only" framing above.
- Never decrypt (Pitfall 1) is the concrete engineering consequence of this framing — a decrypting
  client is a ripper regardless of what the README says.
- State non-affiliation, the subscription requirement, and the reverse-engineering/suspension risk
  together, in the README, not scattered across a wiki page nobody reads before installing.

## Caveat — carried forward from Pitfall 11

**The entire `tidal.com` domain (not just `developer.tidal.com`/`support.tidal.com`) is blocked
from this research environment** — every TIDAL-policy quote anywhere in this skill, including the
Content Guidelines quote above, is second-hand (a GitHub discussion quoting the guidelines
verbatim, or a search-result summary). **Never cite these quotes as primary-sourced in user-facing
legal text without a direct, unproxied re-read first** — read
`https://developer.tidal.com/documentation/guidelines/guidelines-developer-guidelines`,
`.../guidelines-developer-terms-2_0`, and `https://tidal.com/content-guidelines` directly, and
record the retrieval date, before any of this text ships in streamboat's own README or
`docs/legal.md`. Full detail: `references/verification-notes.md` §4.
