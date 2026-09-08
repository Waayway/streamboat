# Project landscape, legal posture, packaging, and TIDAL Connect

Full narrative: `docs/research/tidal-api.md` §13, §14, §15. Read this before writing anything
legal-posture-adjacent into a README, About dialog, or CONTRIBUTING doc — it is the one area where
getting the wording wrong has consequences beyond a bug report.

## Table of contents

1. What each reference project does — comparison
2. What TIDAL's own documents say (with confidence levels)
3. Enforcement history
4. How the ecosystem frames itself — the wording to copy
5. Packaging implications per channel
6. TIDAL Connect — confirmed permanently out of reach

---

## 1. What each reference project does

| Project | API used | Auth | Stream path | Encrypted streams | Notable |
|---|---|---|---|---|---|
| python-tidal 0.8.11 | v1 + v2 + some openapi v2 | device code, PKCE | `playbackinfopostpaywall`, `urlpostpaywall` | records, does not decrypt | the de-facto reference; LGPL-3.0-or-later |
| High Tide | via python-tidal | **PKCE only** | via python-tidal | n/a | GTK4/libadwaita, Flathub, libsecret; GPL-3.0 |
| Sone | v1 + v2 + openapi v2 | device code **and** PKCE, user-supplied creds supported | `playbackinfopostpaywall` + DASH data-URI | skips Hi-Res without a secret | Tauri 2/Rust, play reporting to `ec.tidal.com`, rate gate; GPL-3.0-only |
| sone-windows | same | same | same | same | WASAPI backend, souvlaki media controls |
| Strawberry | v1 (`api.tidalhifi.com`) | PKCE, custom scheme `tidal://login/auth` | four selectable methods | **refuses, with a user-facing message** | Qt6, GPL-3.0 |
| mopidy-tidal | via python-tidal | device code or PKCE (local server on 8989) | MPD→`file://`, BTS→direct URL, optional caching proxy | n/a | headless/server model; Apache-2.0 |
| tidal-hifi | none (wraps the web player) | browser session | Chromium + Widevine (castlabs Electron 43) | handled by CDM | proves the "wrap the web player" path; MIT |
| TidaLuna | v1 via `desktop.tidal.com` + openapi v2 | steals credentials from the official client's Redux store | own fetch + AES for `OLD_AES` | **decrypts** | a mod of the official client; MS-PL — recorded as fact, not to copy |
| tidalt | v1 | device code (plaintext creds) | `urlpostpaywall` with a 4-tier ladder | n/a | Go TUI, FFmpeg + raw ALSA; Apache-2.0 |
| tidalrs | v1 | device code, caller supplies client id | `urlpostpaywall` / `playbackinfopostpaywall` | n/a | Rust library, arc-swap tokens, backoff; MIT |
| tidal-cli | **official** openapi v2 | PKCE with its own registered client `PYVtmSHMTGI9oBUs` | `/trackManifests/{id}` → DASH segments → local file | n/a | the only project here using the sanctioned API for playback; MIT |
| tidal-sdk-web/-android/-ios | official | PKCE, device, client credentials | `/trackManifests/{id}`, `/videoManifests/{id}` | Widevine / FairPlay | Apache-2.0; the Player module is the sanctioned playback path |
| tidalgo, dotnet-tidal-usdk | v1, `api.tidalhifi.com` | username/password + `x-tidal-token` | `streamUrl` / `urlpostpaywall` | n/a | dead (2018); useful only as history |
| libopenTIDAL, TidalSwift | v1 | device code | `playbackinfo*`, `streamUrl`, `/v1/logout` | n/a | proactive refresh patterns worth copying |

## 2. What TIDAL's own documents say

**Note on confidence**: `developer.tidal.com`, `support.tidal.com`, and `tidal.com` were blocked from
direct fetch in the research environment that produced this skill; the quotations below come from
search-result excerpts, not a direct read. Re-verify before using any of this wording in a
public-facing document.

- **Developer Guidelines**: "playbacks will only be available through our SDKs, namely, an official,
  unmodified version of the TIDAL Player module, and TIDAL will reject any quota extension requests
  for any Offering that attempts to circumvent this." Also: certain app categories (alarm/ringtone,
  games/quizzes, voice control, non-interactive webcasting, mixing TIDAL content with other
  services' streams) are prohibited without express written approval.
- **Developer Terms**: prohibits accessing the Developer Tools "beyond the scope of these Developer
  Terms or without an authorized TIDAL account," and prohibits text/data mining or scraping of the
  TIDAL Platform. TIDAL "may limit the number of service calls that applications may make, as TIDAL
  deems appropriate, in its sole discretion, without notice"; apps stay "in development" with quota
  limits until formally approved.
- **Consumer Content Guidelines/Terms**: users agree not to undertake "Circumventing or modifying,
  attempting to circumvent or modify, or encouraging or assisting any other person in circumventing
  or modifying any security technology or software that is part of the TIDAL Services" and not to
  "Reverse-engineer, decompile, disassemble, modify, or create derivative works of any material on
  the TIDAL Services, except where such restriction is expressly prohibited by applicable law."

**Honest reading**: using an unofficial client with your own paid subscription is not obviously
"circumventing security technology" as long as the client plays only what the service hands it in
the clear. Decrypting `OLD_AES` streams *is* squarely within that prohibition — see
`references/playback.md` §3. Using TIDAL's own app client IDs is a terms problem rather than a
copyright problem: unauthorized access relative to the Developer Terms, and impersonation of a
registered client. The realistic risk is not a lawsuit against streamboat; it is **TIDAL rotating
the shared client IDs** — which python-tidal's own changelog shows has happened at least twice
(v0.8.7, v0.8.8, both "OAuth Client ID, secret updated").

**Do not conflate this with the March 2026 "community keys broken" report** — that issue
(`yaronzz/Tidal-Media-Downloader` #1213) is specifically about the **legacy pre-OAuth
`x-tidal-token` keys**, not the OAuth device-code/PKCE client IDs the current ecosystem uses, and it
is a single unconfirmed reporter. Cite the changelog rotations as the stronger evidence.

## 3. Enforcement history

- 2016: TIDAL's counsel (Reed Smith LLP) filed a DMCA takedown against **TiDown**, a downloader,
  asserting "The code provided by the user can be used to circumvent access controls to copyright
  protected works." The developer disputed the framing. (Covered by TorrentFreak and Digital Music
  News — **`torrentfreak.com` is also blocked from direct fetch** in the research environment; this
  is second-hand, same confidence tier as the Developer Terms quotations above.)
- Downloaders (`Tidal-Media-Downloader`, `tidal-dl-ng`) continue to exist publicly with their own
  "private use only" disclaimers.
- **No evidence of any enforcement action against a player** — High Tide, Sone, Strawberry,
  mopidy-tidal, tidal-hifi are all publicly distributed, several through Flathub, with no takedown
  history. (Absence of evidence, stated as such — not proof of safety.)

## 4. How the ecosystem frames itself — the wording to copy

**Sone's disclaimer is the best-worded in the ecosystem — copy it nearly verbatim**: "SONE is an
independent, community-driven project. It is **not affiliated with, endorsed by, or connected to
TIDAL** in any way. All content is streamed directly from TIDAL's service and requires a valid paid
subscription. SONE is a streaming client only — it does not support offline downloads, and does not
redistribute or circumvent protection of any content. As with any third-party client, please be
aware of TIDAL's terms of use." Plus: "All trademarks belong to their respective owners." (Sone's
README badge, separately: "Requires an active TIDAL subscription. Not affiliated with TIDAL.")

High Tide's top-of-README callout is shorter but hits the same note: "Not affiliated in any way with
TIDAL, this is a third-party unofficial client."

Put streamboat's version of this in the README **and** in the app's About dialog.

## 5. Packaging implications

| Channel | Feasible? | Notes |
|---|---|---|
| **Flathub** | yes — both High Tide and Sone are listed | Requires a sandbox-aware secret story (High Tide branches on `Xdp.Portal.running_under_flatpak()`). Flathub does not object to embedded client IDs today. |
| **Snap** | yes — Sone is on the Snap Store | |
| **AUR** | yes, trivially | `python-tidalapi-git` already exists there. |
| **Distro repos (Debian/Fedora)** | plausible but harder | Shipping credentials extracted from a proprietary app is the kind of thing a Debian ftpmaster will ask about. A user-supplied-client-ID mode (see `references/auth.md` §4) makes this tractable. |
| **winget / Microsoft Store** | winget yes; Store is a signed-app review process and riskier | |
| **Apple App Store / Mac App Store** | effectively impossible | Review would reject an unauthorized third-party client for a subscription service. Distribute a notarized `.dmg`/Homebrew cask instead. |
| **Google Play (future mobile)** | very unlikely | Same reasoning, plus stricter impersonation rules. F-Droid is the realistic Android channel. |

Target Flathub + AUR + winget + a notarized macOS build. Do not plan on any app store.

## 6. TIDAL Connect — confirmed permanently out of reach

Given the owner's framing — streamboat "must eventually do everything the native TIDAL client
does" — Connect (casting to a Connect-capable DAC/streamer, or being a Connect target) reads like an
obvious future feature. **It is not reachable at all, in either direction, and this is a researched
conclusion, not an oversight.**

There is no open protocol and no reference implementation to build against. The `tidal-connect`
reference checkout is a docker-compose wrapper around a **closed-source ARM binary**
(`/app/ifi-tidal-release/bin/tidal_connect_application`) that TIDAL distributes only to hardware
partners under a device-certificate program; its own README states outright "This repository does
not contain any tidal-connect binary" and requires the user to separately obtain TIDAL's binaries,
certificate, and libraries. It is device-certificate-gated — no client SDK exists for embedding it,
and no protocol documentation exists for reimplementing it. (Incidentally, the same README
corroborates the MQA-removal findings in `references/playback.md` §7 from a different angle:
"content above 16/44 available as HI_RES quality ... is currently inexistent on Tidal" for the
Connect binary post-removal.)

**Recommendation**: state Connect as permanently out of scope in any public roadmap — so it reads as
a researched decision, not a gap — and offer the substitutes streamboat *can* build instead: MPRIS
(Linux desktop integration), UPnP/DLNA push, Chromecast, Snapcast, and plain ALSA/PipeWire/WASAPI
device selection.
