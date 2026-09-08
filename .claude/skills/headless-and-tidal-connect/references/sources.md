# Sources — project→URL mapping

`ref:<project>/<path>` throughout this skill and the main report
(`/home/user/streamboat/docs/research/headless-connect.md`) points at a shallow (`--depth 1`) git
clone of the named GitHub project, held read-only under a `ref/<project>` directory in the research
environment's scratchpad — not part of the streamboat repo itself. Clones were taken 2026-09-07. A
project cited only "documented-web" (no `ref:` prefix) has **no local checkout** — cite it by its raw
GitHub URL, never as `ref:<project>` (ncspot is the one case this skill previously had to correct;
see `headless-daemon-precedents.md` §9 and §11).

## Reference checkouts used by this skill

Commit column: the short SHA each checkout was actually pinned at when read (`git rev-parse
--short=7 HEAD` in each clone) — reproducibility for any `ref:<project>/<path>:<line>` citation
depends on this.

| `ref:<project>` | GitHub URL | Commit | License | What it is |
| --- | --- | --- | --- | --- |
| `tidal-connect` | https://github.com/GioF71/tidal-connect | `05ea5d7` | MIT (wrapper only) | Docker wrapper around a proprietary, closed-source TIDAL Connect receiver binary shipped with an iFi Audio device certificate. The only "be a Connect target" precedent, and it is not open source underneath. |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | `d8cd6bc` | MS-PL | An injector/plugin system running *inside* the official TIDAL Electron desktop app. Source of the `RemotePlayback`/`PlayQueue` Redux type definitions — intelligence about the official client's internals, not a model to copy. |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | `89244b4` | Apache-2.0 | TIDAL's official web SDK monorepo. Source of the Pushkin streaming-privileges websocket implementation. |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | `c93bff4` | Apache-2.0 | TIDAL's official Android SDK. Corroborates the Pushkin `rt/connect` contract independently of the web SDK. |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | `787fe96` | Apache-2.0 | TIDAL's official iOS SDK. Searched (negative result) for any Connect/device-picker code. |
| `mopidy-tidal` | https://github.com/EbbLabs/mopidy-tidal (formerly `tehkillerbee/mopidy-tidal`) | `18abb3b` | Apache-2.0 | A Mopidy backend for TIDAL — the strongest full-server headless precedent in the set: config schema, three headless-login shapes, stream resolution, and a caching proxy (with a legal-posture caveat, see `headless-daemon-precedents.md` §1). |
| `python-tidal` (`tidalapi`) | https://github.com/EbbLabs/python-tidal (formerly `tamland/python-tidal`) | `9c41fbe` | LGPL-3.0-or-later | The de-facto unofficial-API client library. Source of the refresh-token-never-rotates fact used in the token/session-sharing discussion. |
| `tidalt` | https://github.com/Benehiko/tidalt | `6cf18c9` | Apache-2.0 | A Go daemon + Bubble Tea TUI — the "one binary, two modes" precedent: MPRIS IPC, systemd unit generation, deep-link forwarding, terminal QR login, and the most complete small-device ALSA output recipe in the set. |
| `sone` | https://github.com/lullabyX/sone | `21494b9` | GPL-3.0-only | Tauri 2 + Rust + React 19 native Linux TIDAL client. Source of the local MCP-server and OBS-overlay-server security model this skill recommends adapting. |
| `sone-windows` | https://github.com/lvllaby/sone-windows | `f2009f2` | GPL-3.0-only | A fork of Sone adding Windows WASAPI2-exclusive output and SMTC media controls via `souvlaki`. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | `b1326db` | MIT (GitHub reports "other"/NOASSERTION) | Electron wrapper around the TIDAL web player. Source of the documented HTTP player-control API vocabulary this skill recommends copying almost verbatim. |
| `high-tide` | https://github.com/Nokse22/high-tide | `49f2472` | GPL-3.0 | Python + GTK4/libadwaita native Linux TIDAL client. Cited here only for its MPRIS bus-name convention. |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | `24f20a8` | MIT | A Node/TypeScript CLI using the official SDK. The CLI precedent this project should build from — see `headless-daemon-precedents.md` §8 for its full command surface and the anti-pattern (temp-file playback) not to copy. |
| `tidalrs` | https://github.com/phayes/tidalrs | `8bb1de8` | MIT | A maintained Rust TIDAL client library (device-code OAuth2, auto token refresh, DASH/MPEG streaming) targeting the same unofficial v1 API this project uses. Evaluate as a stage-0 build-vs-adopt option and as a reference implementation for the token-refresh callback pattern — `daemon-architecture.md` §1, §5. |
| `tidalswift` | https://github.com/melgu/TidalSwift | `cf0926b` | no LICENSE file in the checkout — treat as all-rights-reserved, do not copy code | A native macOS TIDAL client, split into `TidalSwiftLib` (session/catalogue/playback, no UI) and a thin `TidalSwift` SwiftUI app. The only Swift-side core/UI-split precedent in this project's source set — relevant to the future iOS build. `daemon-architecture.md` §1. |

## External references (no local checkout — cited by URL only)

Found and fetched during research and the subsequent fact-check pass; not governed by any licence
obligation above (only the licence on the linked repo applies, and only if code is actually copied
— none of it is recommended to be copied verbatim here beyond the explicitly named artifacts).

| Project/page | URL | Cited for |
| --- | --- | --- |
| shawaj/ifi-tidal-release | https://raw.githubusercontent.com/shawaj/ifi-tidal-release/master/README.md | ARMv7 binaries, runtime deps, the `--netif-for-deviceid` flag, the full systemd `ExecStart` |
| shawaj/ifi-tidal-release licences | https://github.com/shawaj/ifi-tidal-release/tree/master/licenses/tidal_connect | The eight third-party libraries linked into the Connect binary |
| TonyTromp/tidal-connect-docker troubleshooting | https://raw.githubusercontent.com/TonyTromp/tidal-connect-docker/master/docs/TROUBLESHOOTING.md | `avahi-browse` commands, mDNS TTL |
| TonyTromp/tidal-connect-docker | https://github.com/TonyTromp/tidal-connect-docker | Binary provenance, volume-bridge metadata service |
| GioF71/upmpdcli-docker | https://raw.githubusercontent.com/GioF71/upmpdcli-docker/main/README.md | upmpdcli's TIDAL plugin config, `TIDAL_ENABLE_USER_AGENT_WHITELIST`, current plugin version table |
| GioF71/audio-tools | https://github.com/GioF71/audio-tools/blob/main/media-servers/tidal-hires/README.md | Hi-res renderer whitelist detail |
| michaelherger/lms-plugin-tidal | https://github.com/michaelherger/lms-plugin-tidal (issues #35, #88, #89, #98, #100, #113) | Lyrion/LMS TIDAL plugin hi-res status |
| music-assistant/server | https://raw.githubusercontent.com/music-assistant/server/dev/README.md, `/dev/music_assistant/providers/snapcast/constants.py`, `/dev/music_assistant/providers/sendspin/README.md` | Architecture, built-in Snapcast control port (1705, not 1780), Sendspin endpoint |
| mikebrady/shairport-sync | https://github.com/mikebrady/shairport-sync | AirPlay 2 receiver on Linux |
| philippe44/libraop, music-assistant/airplay-cli, akustikrausch/airplay2-sender-cpp, owntone/owntone-server | (respective GitHub repos) | AirPlay sender precedents |
| rgerganov/shanocast | https://github.com/rgerganov/shanocast | The one working open-source Chromecast receiver, and why it isn't a model to copy (reused AirReceiver signatures) |
| Google Cast media docs | https://developers.google.com/cast/docs/media | Chromecast's FLAC quality ceiling — **blocked from this environment (added to the blocked-domains list below by the second fact-check pass, having previously been mis-cited as directly confirmed); recovered via search-index summary only, re-verify before quoting a number** |
| tidal-sdk-web/android/ios API specs | `ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json`, `ref:tidal-sdk-android/tidalapi/bin/tidal-api.json`, `ref:tidal-sdk-ios/Sources/TidalAPI/Config/input/tidal-api-oas.json` | The official `openapi.tidal.com/v2` `/playQueues` server-side play-queue resource, bundled in all three SDKs — see `tidal-connect.md` §4 |
| badaix/snapcast client | https://raw.githubusercontent.com/badaix/snapcast/develop/client/snapclient.cpp | Confirms snapclient has no Windows service mode — `daemon-architecture.md` §1's Windows-headless precedent |
| Apple developer forums | https://developer.apple.com/forums/thread/759262 | macOS 15 `NSLocalNetworkUsageDescription`, the stale-purpose-string trap, `/Applications`-only LAN constraint — `daemon-architecture.md` §1 |
| raspberrypi/linux issue #2215 | https://github.com/raspberrypi/linux/issues/2215 | Documented USB DAC dropout/glitch issue class on Raspberry Pi — `raspberry-pi-deployment.md` §1 |
| Raspberry Pi Wi-Fi power-save guidance | https://forums.raspberrypi.com/viewtopic.php?t=380009, https://thepihut.com/blogs/raspberry-pi-tutorials/disable-wifi-power-management | Pi Wi-Fi `power_save` as a documented dropout cause, and the fix — `raspberry-pi-deployment.md` §1 |
| freedesktop login1 `Inhibit` interface | (general systemd/logind specification, not a single URL) | Sleep/idle inhibition design — works headless because login1 is on the system bus, unlike MPRIS — `daemon-architecture.md` §1 |
| systemd.exec(5) | (general systemd documentation, not a single URL) | `RuntimeDirectory=`/`StateDirectory=`/`ConfigurationDirectory=` for a headless system unit — `daemon-architecture.md` §1 |
| MusicPlayerDaemon/MPD | https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/doc/protocol.rst, `/doc/user.rst` | Primary MPD protocol spec (reachable even though `mpd.readthedocs.io`/`musicpd.org` are blocked) |
| mopidy/mopidy-mpd | https://raw.githubusercontent.com/mopidy/mopidy-mpd/main/src/mopidy_mpd/protocol/music_db.py, https://github.com/mopidy/mopidy-mpd | MPD-over-non-file-backend precedent; project maintenance status |
| docs.nuclearplayer.com | docs.nuclearplayer.com/nuclear/integrations/mpd-server | Nuclear's MPD-subset scope and port-fallback behaviour — page blocked from this environment, read via search-index summary |
| M0Rf30/rmpd | https://github.com/M0Rf30/rmpd | A from-scratch Rust MPD server |
| devgianlu/go-librespot | https://raw.githubusercontent.com/devgianlu/go-librespot/master/README.md, `/master/API.md`, `/master/api-spec.yml` | REST+WebSocket event vocabulary, config schema, cache design, `GET /auth/code` |
| Spotifyd/spotifyd docs | https://raw.githubusercontent.com/Spotifyd/spotifyd/master/docs/src/advanced/{dbus,mpris,hooks}.md | D-Bus/MPRIS split, `--dbus-type system`, `--onevent` (reachable at this raw path even though `docs.spotifyd.rs` itself is blocked) |
| librespot-org/librespot | https://raw.githubusercontent.com/librespot-org/librespot/dev/README.md, `/dev/Cargo.toml` | Crate workspace split, `rustls` TLS option |
| jpochyla/psst | https://github.com/jpochyla/psst/blob/main/README.md | `psst-core`/`psst-gui` split |
| hrkfdn/ncspot | https://raw.githubusercontent.com/hrkfdn/ncspot/main/doc/users.md | Unix-domain-socket/named-pipe NDJSON control surface |
| badaix/snapcast | https://raw.githubusercontent.com/badaix/snapcast/develop/README.md, `/develop/server/etc/snapserver.conf`, `/develop/doc/configuration.md`, `/develop/doc/json_rpc_api/stream_plugin.md` | Ports, sources, codecs, mDNS service types, stream-plugin protocol. Canonical repo is `badaix/snapcast` — `snapcast/snapcast` redirects. |
| Sendspin/spec | https://raw.githubusercontent.com/Sendspin/spec/main/README.md | Full protocol spec: framing, pairing, clock sync, versioning |
| music-assistant discussions #4200, #5133 | https://github.com/orgs/music-assistant/discussions/{4200,5133} | Sendspin origin; RAAT unavailability to open-source implementers |
| SeaDve/mpris-server | https://raw.githubusercontent.com/SeaDve/mpris-server/main/README.md | Confirms `TrackList`/`Playlists` interface support |
| docs.rs/souvlaki | https://docs.rs/souvlaki | Cross-platform OS media controls crate |
| specifications.freedesktop.org | https://specifications.freedesktop.org/mpris/latest/ | MPRIS D-Bus Interface Specification v2.2 |
| W3C Secure Contexts | https://w3c.github.io/webappsec-secure-contexts/ | Which origins count as a secure browser context |
| learn.microsoft.com | https://learn.microsoft.com/en-us/windows/win32/coreaudio/wasapi | WASAPI reference (no first-party session-0-service statement found) |
| help.roonlabs.com | help.roonlabs.com/portal/en/kb/articles/raat | Roon/RAAT model — **blocked from this environment**; corroborated instead via the music-assistant discussion above |
| crates.io | https://crates.io/crates/{cast-sender,rust_cast,mdns-sd,windows-service,mpd_protocol,mpd} | Rust crate descriptions for Cast senders, mDNS, Windows services, MPD clients |
| crates.io release-date API | https://crates.io/api/v1/crates/{mdns-sd,windows-service,souvlaki,mpd_protocol} | Maintenance-cadence check (2026-09-08): `mdns-sd` 0.21.3 (same-day), `windows-service` 0.8.1 (2026-05-08), `souvlaki` 0.8.3 (2025-06-24, over a year old), `mpd_protocol` 1.0.3 (2024-02-28, dormant) — see the trap table in `SKILL.md` |
| librespot-org/librespot discovery crate | https://raw.githubusercontent.com/librespot-org/librespot/dev/discovery/src/server.rs, `/dev/discovery/src/lib.rs` | librespot's zeroconf pairing protocol — `_spotify-connect._tcp`, `CPath`/`VERSION` TXT keys, port-0-and-advertise, `getInfo`/`addUser` handshake, DH+HMAC blob encryption — the fifth headless-login shape, `daemon-architecture.md` §5 |
| MusicPlayerDaemon/MPD zeroconf | https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/src/zeroconf/Glue.cxx | MPD's own `_mpd._tcp` mDNS service type and `"Music Player @ %h"` default name — `mpd-and-multiroom.md` §1 |
| RFC 6762 §10 | (IETF RFC, not a single fetchable URL in this environment) | mDNS TTL guidance: 120 s for A/AAAA/SRV host records, 75 minutes for PTR/TXT service-instance records — do not copy TIDAL's own ~120 s Connect-target TTL into `_streamboat._tcp`'s design, `tidal-connect.md` §3 |
| Sendspin/spec pairing-code sections | https://raw.githubusercontent.com/Sendspin/spec/main/README.md — Definitions (Pairing Code), Dynamic Pairing Code Flow, Pairing Token, PAKE | `static_pairing_code`/`dynamic_pairing_code` over CPace, the actual screen-free pairing answer (distinct from the QR pairing *token*) — `mpd-and-multiroom.md` §4 |
| badaix/snapcast stream-plugin doc | https://raw.githubusercontent.com/badaix/snapcast/develop/doc/json_rpc_api/stream_plugin.md | Cited again as the third JSON-RPC 2.0 precedent behind the control-API transport-choice decision, `daemon-architecture.md` §2 |
| whathifi.com, ampvortex.com | https://www.whathifi.com/features/tidal-connect-everything-you-need-to-know, ampvortex.com (SEO aggregator, low reliability) | TIDAL Connect ecosystem scale, device counts — trade press, second-hand |

## Domains blocked from this research environment

`tidal.com` (all paths, including `/connect` and `/supported-devices`), `developer.tidal.com`,
`support.tidal.com`, `mpd.readthedocs.io`, `www.musicpd.org`, `docs.nuclearplayer.com`,
`www.music-assistant.io`, `www.lesbonscomptes.com`, `deepwiki.com`, `protodoc.io`,
`docs.spotifyd.rs`, `help.roonlabs.com`, `xakcop.com`, `developers.google.com` all returned
`EGRESS_BLOCKED` from this environment's proxy. **`developers.google.com` was added to this list by
the second fact-check pass** — it had been cited as directly confirmed for the Google Cast quality
ceiling despite never having been reachable, the same citation-hygiene mistake the first fact-check
pass already caught for the domains before it. Where a reachable primary source exists it is cited
above instead (spotifyd's docs at their raw GitHub path; the Chromecast-receiver claim at
`rgerganov/shanocast` directly; Roon/RAAT via the music-assistant discussion thread; MPD's protocol
spec at its raw GitHub path). Anything else attributed to a blocked domain came from a search-result
summary and is flagged `[documented-web]`/`[unverified]` inline where it appears.

Music Assistant's own choice of **WebRTC for Sendspin remote access outside the LAN** (cited in
`daemon-architecture.md` §3 for streamboat's own "remote control outside the LAN" open decision) is
documented at https://github.com/music-assistant/server/blob/dev/music_assistant/providers/sendspin/README.md,
already listed above for its `ws://<server>:8927/sendspin` endpoint.
