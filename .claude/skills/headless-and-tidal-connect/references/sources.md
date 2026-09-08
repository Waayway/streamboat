# Sources — project→URL mapping

`ref:<project>/<path>` throughout this skill and the main report
(`/home/user/streamboat/docs/research/headless-connect.md`) points at a shallow (`--depth 1`) git
clone of the named GitHub project, held read-only under
the reference checkouts (shallow clones of the cited GitHub projects; see references/sources.md)<project>`
in the research environment — not part of the streamboat repo itself. Clones were taken 2026-09-07.

## Reference checkouts used by this skill

| `ref:<project>` | GitHub URL | License | What it is |
| --- | --- | --- | --- |
| `tidal-connect` | https://github.com/GioF71/tidal-connect | MIT (wrapper only) | Docker wrapper around a proprietary, closed-source TIDAL Connect receiver binary shipped with an iFi Audio device certificate. The only "be a Connect target" precedent, and it is not open source underneath. |
| `TidaLuna` | https://github.com/Inrixia/TidaLuna | MS-PL | An injector/plugin system running *inside* the official TIDAL Electron desktop app. Source of the `RemotePlayback`/`PlayQueue` Redux type definitions — intelligence about the official client's internals, not a model to copy. |
| `tidal-sdk-web` | https://github.com/tidal-music/tidal-sdk-web | Apache-2.0 | TIDAL's official web SDK monorepo. Source of the Pushkin streaming-privileges websocket implementation. |
| `tidal-sdk-android` | https://github.com/tidal-music/tidal-sdk-android | Apache-2.0 | TIDAL's official Android SDK. Corroborates the Pushkin `rt/connect` contract independently of the web SDK. |
| `tidal-sdk-ios` | https://github.com/tidal-music/tidal-sdk-ios | Apache-2.0 | TIDAL's official iOS SDK. Searched (negative result) for any Connect/device-picker code. |
| `mopidy-tidal` | https://github.com/EbbLabs/mopidy-tidal (formerly `tehkillerbee/mopidy-tidal`) | Apache-2.0 | A Mopidy backend for TIDAL — the strongest full-server headless precedent in the set: config schema, three headless-login shapes, stream resolution, and a caching proxy (with a legal-posture caveat, see `headless-daemon-precedents.md` §1). |
| `python-tidal` (`tidalapi`) | https://github.com/EbbLabs/python-tidal (formerly `tamland/python-tidal`) | LGPL-3.0-or-later | The de-facto unofficial-API client library. Source of the refresh-token-never-rotates fact used in the token/session-sharing discussion. |
| `tidalt` | https://github.com/Benehiko/tidalt | Apache-2.0 | A Go daemon + Bubble Tea TUI — the "one binary, two modes" precedent: MPRIS IPC, systemd unit generation, deep-link forwarding, terminal QR login, and the most complete small-device ALSA output recipe in the set. |
| `sone` | https://github.com/lullabyX/sone | GPL-3.0-only | Tauri 2 + Rust + React 19 native Linux TIDAL client. Source of the local MCP-server and OBS-overlay-server security model this skill recommends adapting. |
| `sone-windows` | https://github.com/lvllaby/sone-windows | GPL-3.0-only | A fork of Sone adding Windows WASAPI2-exclusive output and SMTC media controls via `souvlaki`. |
| `tidal-hifi` | https://github.com/Mastermindzh/tidal-hifi | MIT (GitHub reports "other"/NOASSERTION) | Electron wrapper around the TIDAL web player. Source of the documented HTTP player-control API vocabulary this skill recommends copying almost verbatim. |
| `high-tide` | https://github.com/Nokse22/high-tide | GPL-3.0 | Python + GTK4/libadwaita native Linux TIDAL client. Cited here only for its MPRIS bus-name convention. |
| `tidal-cli` | https://github.com/lucaperret/tidal-cli | MIT | A Node/TypeScript CLI using the official SDK. The CLI precedent this project should build from — see `headless-daemon-precedents.md` §8 for its full command surface and the anti-pattern (temp-file playback) not to copy. |

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
| Google Cast media docs | https://developers.google.com/cast/docs/media | Chromecast's FLAC quality ceiling |
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
| whathifi.com, ampvortex.com | https://www.whathifi.com/features/tidal-connect-everything-you-need-to-know, ampvortex.com (SEO aggregator, low reliability) | TIDAL Connect ecosystem scale, device counts — trade press, second-hand |

## Domains blocked from this research environment

`tidal.com` (all paths, including `/connect` and `/supported-devices`), `developer.tidal.com`,
`support.tidal.com`, `mpd.readthedocs.io`, `www.musicpd.org`, `docs.nuclearplayer.com`,
`www.music-assistant.io`, `www.lesbonscomptes.com`, `deepwiki.com`, `protodoc.io`,
`docs.spotifyd.rs`, `help.roonlabs.com`, `xakcop.com` all returned `EGRESS_BLOCKED` from this
environment's proxy. Where a reachable primary source exists it is cited above instead (spotifyd's
docs at their raw GitHub path; the Chromecast-receiver claim at `rgerganov/shanocast` directly;
Roon/RAAT via the music-assistant discussion thread; MPD's protocol spec at its raw GitHub path).
Anything else attributed to a blocked domain came from a search-result summary and is flagged
`[documented-web]`/`[unverified]` inline where it appears.
