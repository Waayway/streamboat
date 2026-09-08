# Dolby Atmos and 360 Reality Audio — feasibility on desktop

Full narrative: `docs/research/audio-pipeline.md` §5. Verdict for both: **do not build for v1.**

## 360 Reality Audio: dead

Removed from TIDAL 24 July 2024 alongside the MQA catalogue (replaced with FLAC); tracks are greyed
out and unstreamable (corroborated by multiple 2024 news reports — `digitaltrends.com`,
`whathifi.com`, `ecoustics.com` — the first-party `support.tidal.com` article itself is blocked from
the research environment). The iOS SDK returns `nil` for the mode with the comment *"Sony 360
Reality Audio has no codec the client needs, so it is unsupported here"*
(`ref:tidal-sdk-ios/Sources/Player/Common/Data/AudioCodec.swift`). There is no `SONY_360RA` token in
the v2 formats enum (`HEAACV1, AACLC, FLAC, FLAC_HIRES, EAC3_JOC`), and python-tidal's
`MediaMetadataTags` has no 360RA tag either. **Do not implement.**

## Dolby Atmos: possible in principle, hard in practice, TIDAL's own desktop apps reportedly don't do it

**What Atmos is on TIDAL:** E-AC-3 with Joint Object Coding. Format token `EAC3_JOC`; DASH signals
it as a plain `codecs="ec-3"` adaptation set, parsed as `audio/eac3`
(`ref:tidal-sdk-android/.../TidalTrackSelectionFactoryTest.kt`). Android maps
`AudioMode.DOLBY_ATMOS` -> codec string `eac3_joc`.

**What would be needed to actually build it:**

1. Request `EAC3_JOC` in the v2 `formats` array, or rely on the account's Atmos entitlement on v1.
2. Decode E-AC-3 with JOC, or pass the bitstream through untouched:
   - **Decoding to PCM does not give you Atmos.** FFmpeg's `eac3` decoder discards JOC object
     metadata entirely — verified directly against `libavcodec/ac3dec.c`/`eac3dec.c` (zero
     occurrences of "joc"/"object"/"atmos"); the only JOC-aware code is in the *parser*
     (`ac3_parser.c`, ~lines 266-282), which can only *detect* Atmos presence via the
     additional-bitstream-info byte, not render it. A real Atmos renderer requires a licence from
     Dolby. See `decoding-and-codecs.md` §4 for the full evidence.
   - **Passthrough to an AVR over HDMI is the only realistic path.** Needs IEC 61937
     bitstream-passthrough output: on Linux, `alsasink` on an `hdmi:` device with `audio/x-eac3`
     caps and ALSA IEC958 channel status set; on Windows, WASAPI exclusive with a
     `WAVE_FORMAT_DOLBY_AC3_SPDIF`-class format; on macOS, CoreAudio's SPDIF path — which, notably,
     is precisely the one path where GStreamer's macOS backend *does* take hog mode
     (`_open_spdif` — see `output-backends.md` §5).
3. GStreamer has no Atmos renderer. mpv's `--audio-spdif=<codecs>` supports `ac3, dts, dts-hd, eac3,
   truehd, dsd` passthrough — so the concrete Atmos-to-AVR path on libmpv is `--audio-spdif=eac3`.
   `dsd` passthrough needs an exclusive-access AO (currently `wasapi` only) — Windows-only, not
   relevant to TIDAL streaming specifically.

**What the reference clients actually do: nothing.** No unofficial TIDAL client in the set plays
Atmos. TIDAL's own Android SDK plays it via ExoPlayer on hardware with an E-AC-3 decoder; iOS maps
`DOLBY_ATMOS -> EAC3`; the **web SDK's own `_fetchTrackManifest` hardcodes
`audioMode: 'STEREO'`** with the comment *"Only stereo (and mono) supported for now, TODO: revise or
remove if multi-channel is added"* — TIDAL's own web player declining to do Atmos.
`[unverified]`: reporting says the TIDAL Windows/macOS desktop apps also lack Atmos support, limited
to mobile/TV/streamer hardware — this rests on second-hand reporting, `support.tidal.com` and
`tidal.com` are blocked from the research environment.

## Recommendation: do not implement Atmos or 360RA for v1

Instead:

- Surface `audioModes`/`mediaMetadataTags` in the UI (`tidal-manifest-api.md` §9) so users see an
  Atmos version exists.
- When a track is Atmos-only, request the stereo formats and play the stereo version — omitting
  `EAC3_JOC` from the requested `formats` naturally returns stereo.
- Keep an `immersive` flag in the manifest-request layer so E-AC-3 passthrough can be added later
  without an architectural change.
- If Atmos is ever added, it must be **bitstream passthrough to an HDMI/AVR sink only, never a
  bundled software renderer.**
