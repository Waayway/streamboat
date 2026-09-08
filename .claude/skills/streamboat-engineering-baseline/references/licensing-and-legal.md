# Licensing and legal

Full source: `docs/research/engineering-baseline.md` §5 (fact-checked). `ref:<project>/<path>`
points at a shallow clone — see `sources.md`.

## Contents

1. What the references chose
2. Choosing for streamboat
3. Dependency compatibility notes
4. Trademark and positioning
5. EU Cyber Resilience Act

## 1. What the references chose

| Project | License | Notes |
|---|---|---|
| sone / sone-windows | GPL-3.0-only | Declared in `package.json`, `Cargo.toml`, PKGBUILD, snapcraft.yaml and metainfo `<project_license>` |
| High Tide | GPL-3.0 (COPYING); individual files LGPL-3.0-or-later headers | CONTRIBUTING: "Contributions should be licensed under the **GPL-3**" |
| Strawberry | GPL-3.0 | Qt + GStreamer app |
| tidal-hifi | MIT | wraps the web player; ships castlabs Electron with Widevine |
| python-tidal | LGPL-3.0-or-later | library, so weak copyleft |
| mopidy-tidal | Apache-2.0 | server plugin |
| tidalt | Apache-2.0 | statically links FFmpeg |
| tidalrs, libopenTIDAL, tidal-cli | MIT | libraries/CLI |
| tidal-sdk-{web,ios,android} | Apache-2.0 | official |
| TidaLuna | Ms-PL | mod for the official client |
| dotnet-tidal-usdk | MIT **with an added anti-piracy clause** | "The software cannot be used for piracy purposes; this includes using the software to create copies (or 'back-ups') of intelluctual property provided by TIDAL without the copyright holder's ... express permission" (ref:dotnet-tidal-usdk/LICENSE.md) |

The dotnet-tidal-usdk clause is a cautionary example: bolting a use restriction onto MIT makes the
license non-OSI, non-free, and unpackageable by Debian/Fedora/Flathub. Achieve the same effect with
a README/metainfo statement of scope and a design that has no ripping feature, exactly as sone does
in its Disclaimer (ref:sone/README.md:540-544).

## 2. Choosing for streamboat

- **GPL-3.0-only** is the default recommendation for the app. It matches every comparable Linux
  TIDAL client, it is compatible with GStreamer's LGPL core *and* with the GPL plugins in
  `gst-plugins-ugly`/`-bad` (which an LGPL/permissive app cannot use), it is compatible with Qt's
  open-source terms, and it prevents a closed fork of a project whose whole value is being open.
- **Split the license by layer.** Make the reusable core library (`streamboat-core`: API client,
  auth, manifest parsing, models) **LGPL-3.0-or-later or Apache-2.0**, and the applications
  (desktop UI, headless daemon) **GPL-3.0-only**. Precedent: python-tidal is LGPL, the apps on top
  of it are GPL; the TIDAL SDKs are Apache-2.0. A permissive core maximises the chance other
  clients adopt it, which is the fastest route to shared maintenance of an unofficial API surface.
  Note the one-way door: relicensing later requires every contributor's agreement unless you
  collect a CLA/DCO with relicensing rights, which most contributors dislike. Also decide whether
  `core` is versioned/released independently of the apps — see `ci-and-repo-governance.md`.
- **AGPL-3.0 is a poor fit.** Its network clause only bites when users interact with the software
  over a network. streamboat's headless mode does expose a local control surface, so AGPL is not
  meaningless — but it would deter packagers and integrators (Music Assistant, Mopidy, HA-style
  projects) for a benefit that barely applies to a single-user player. If the owner cares about
  hosted forks, AGPL for the *server* component only is the narrower option.
- **MIT/Apache-2.0 for the whole app** is only right if the goal is maximum adoption including by
  closed products. It also forecloses GPL GStreamer plugins and would make the project the
  path of least resistance for a closed commercial TIDAL client.

## 3. Dependency compatibility notes

- **GStreamer**: core, base, good are LGPL-2.1. "We require that all code going into our core
  packages is LGPL", and plugins with patent issues "would need to go into our gst-plugins-ugly
  module" — both from GStreamer's own licensing FAQ, verified by direct download and grep of
  `raw.githubusercontent.com/GStreamer/gst-docs/master/markdown/frequently-asked-questions/licensing.md`:
  it contains neither the "practical reasons under the GPL" line nor any FFmpeg guidance, despite
  those often being attributed to it. "When using GPL linked plugins, GStreamer is for all
  practical reasons under the GPL itself" is from `gst-plugins-base`'s `LICENSE_readme` instead;
  the FFmpeg build-mode caveat ("you have to make sure not to build FFmpeg with GPL code enabled")
  is from `GStreamer/gst-libav`'s `README.md`. A GPL-3.0 streamboat has no problem here. A
  permissive streamboat would have to restrict itself to LGPL plugins and an LGPL FFmpeg build.
- **FFmpeg**: LGPL-2.1+ by default; `--enable-gpl` and `--enable-nonfree` change that. tidalt
  builds FFmpeg 7.1.5 with `--disable-everything --disable-programs --disable-doc
  --disable-network --disable-autodetect --disable-shared --enable-static --enable-pic
  --disable-avdevice --disable-swscale --disable-avfilter --enable-protocol=file
  --enable-demuxer=flac,mov,aac,wav,ogg
  --enable-decoder=flac,aac,aac_latm,alac,pcm_s16le,pcm_s24le,pcm_s32le,vorbis
  --enable-parser=flac,aac,aac_latm,vorbis --enable-swresample` — no `--enable-gpl`, so the result
  is LGPL (ref:tidalt/packaging/build-static-ffmpeg.sh:27,47-54). **Static linking of LGPL code
  from an Apache-2.0 binary imposes the LGPL relinking obligation** (ship object files or the full
  source and build instructions). If streamboat statically links FFmpeg, publish the exact build
  script and the object archives, or dynamically link.
- **fdk-aac**: GPL-incompatible and "therefore nondistributable with GPL parts" per FFmpeg's own
  position; Debian ships it as non-free. `fedoraproject.org` is blocked from this environment —
  this is read via a search index of the Fedora Licensing/FDK-AAC wiki, corroborated by
  tookmund.com "AAC and Debian" and the Hydrogenaudio knowledge base, not a primary fetch.
  streamboat needs AAC *decoding* only, which `avdec_aac` (LGPL FFmpeg) or `faad` covers; never
  link fdk-aac.
- **Qt 6** open source is mainly LGPLv3 with some modules GPL-only; some modules are GPL-2.0-only
  and others GPL-3.0-only, and mixing those two is itself a violation — Qt's own FAQ gives the
  concrete example that mixing GPL-3.0-only Spatial Audio with GPL-2.0-only TextToSpeech "violates
  GPL-3.0". `doc.qt.io` is blocked from this environment; this is read via a search index of the
  Qt open-source licensing FAQ, not a primary fetch — **re-verify module-by-module before Qt is
  chosen** (https://doc.qt.io/qt-6/licensing.html, https://www.qt.io/faq/qt-open-source-licensing).
  A GPL-3.0-only app can use LGPLv3 and GPL-3.0-only Qt modules but must avoid GPL-2.0-only ones.
  Static linking against LGPLv3 Qt carries the relinking obligation.
- **GTK4/libadwaita** are LGPL-2.1 — no constraint on a GPL app.
- **Electron/Chromium** is largely BSD/MIT; the castlabs Widevine build that tidal-hifi uses
  bundles a proprietary CDM, which is why that route is architecturally different and why
  streamboat should not go there (it also implies DRM handling the project has said it will not
  design around).
- **Third-party license inventory:** run a license scanner in CI. TIDAL's own SDKs use FOSSA
  (ref:tidal-sdk-web/.github/workflows/fossa-scan.yml,
  ref:tidal-sdk-android/.fossa.yml — which is worth reading for the pattern of restricting the scan
  to *shipped* runtime classpaths so build tooling does not gate PRs). Free alternatives:
  `cargo-deny` (Rust), `pip-licenses`/`reuse` (Python), `license-checker` (npm), `go-licenses`
  (Go), plus REUSE-compliant `LICENSES/` + SPDX headers.
- **Flathub requires the license file of every module to be installed** to
  `$FLATPAK_DEST/share/licenses/$FLATPAK_ID` and the metainfo `<project_license>` to match the
  source (verified by direct fetch of `flathub-infra/documentation` — see
  `packaging-and-distribution.md` §1).

## 4. Trademark and positioning (licensing-adjacent, affects packaging)

Flathub is explicit: "Official affiliation must not be implied by using a vendor's name in the
application name or icon unless the application is actually part of that vendor's project", with
the example "A WhatsApp client or wrapper cannot have `WhatsApp` in its name or use any of the
official icon, logo or artwork". "streamboat" is already safe. Keep TIDAL out of the name, the
icon, and the app ID; use it only in the description ("client for TIDAL", "requires an active
TIDAL subscription"), which every accepted client does. Sone's wording is the template:
"SONE is an independent, community-driven project. It is **not affiliated with, endorsed by, or
connected to TIDAL** in any way. All content is streamed directly from TIDAL's service and
requires a valid paid subscription. SONE is a streaming client only — it does not support offline
downloads, and does not redistribute or circumvent protection of any content. As with any
third-party client, please be aware of TIDAL's terms of use." (ref:sone/README.md:540-544)

One unverified but material data point: a search result attributes to TIDAL's Developer Terms 2.0
the statement that "The Player module in the SDK constitutes the only allowed way for third-party
applications to incorporate playback of TIDAL content", corroborated at second hand by an
independent search of `developer.tidal.com/documentation/guidelines-developer-terms-2_0` returning
both that sentence and "playbacks shall only be made available through TIDAL's SDKs, namely an
official, unmodified version of the TIDAL Player module". `developer.tidal.com` and `tidal.com` are
both unreachable from this environment, so **this is not verified from the primary source** and
must be checked by the legal/ToS research topic before any claim is published.

**Scope distinction to get right in `docs/legal.md`, regardless of how that verification lands**:
those Developer Terms govern the official developer-program API and SDK (the path High Tide, sone
and python-tidal explicitly do *not* take). streamboat's declared stance — per the owner's
decision — is the unofficial API that python-tidal/High Tide/sone use, which is governed by the
consumer Terms of Service instead. Conflating the two documents would produce the wrong legal
analysis: a "Player-module-only" restriction in the *developer* terms does not, by itself,
establish that the *unofficial* API route violates the *consumer* ToS — the two are separate
questions and both need their own primary-source read.

## 5. EU Cyber Resilience Act — record the determination, do not skip it

2026-09-07 (research date) sits four days before a CRA compliance milestone: obligations for
manufacturers to report actively exploited vulnerabilities and severe incidents to ENISA and
national CSIRTs (24h early warning / 72h notification / 14 days after a patch) begin 2026-09-11;
conformity-assessment-body rules applied from 2026-06-11; full compliance is due 2027-12-11
(https://openssf.org/public-policy/eu-cyber-resilience-act/, https://orcwg.org/cra/). A free,
non-commercial FOSS project is out of scope, but that is a determination someone has to make and
write down, not assume. "Open-source stewards" — legal persons systematically supporting FOSS
intended for commercial activity — carry lighter Article 24 duties (cybersecurity policy,
cooperation on vulnerability handling, reporting) and are explicitly exempt from administrative
fines under Article 64(10); an individual maintainer distributing a free client is neither
manufacturer nor steward. Record the determination in `docs/legal.md`, and align `SECURITY.md`'s
disclosure process and timelines with the steward pattern anyway — it costs nothing and is the
answer if the project's status ever changes (e.g. a paid/hosted build appears).
