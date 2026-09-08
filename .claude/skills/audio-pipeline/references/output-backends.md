# Output backends and bit-perfect / exclusive modes

Full narrative: `docs/research/audio-pipeline.md` §3, plus fact-check gap-fill in its §10.3-10.9,
§10.12-10.13.

## Table of contents

1. Linux — ALSA `hw:` direct
2. Linux — PipeWire/PulseAudio and device reservation
3. Sandbox implications (Flatpak, Snap)
4. Windows — WASAPI and ASIO
5. macOS — CoreAudio
6. mpv/libmpv exclusive-mode scope across platforms
7. Bit-perfect as a feature: hardware volume, dither, and where DSP fits
8. Position: correcting for buffered-but-unplayed frames
9. Real-time thread scheduling for the writer thread
10. Verifying bit-perfectness — a test plan
11. Device identity and persistence across reboots/hot-plug
12. `dashdemux2` outranks `dashdemux` by default — the rank-collision correction
13. What a custom ALSA writer buys over plain `alsasink`
14. Shipping GStreamer on Windows and macOS
15. Bit-perfect through PipeWire — unresolved, and it decides Flatpak's fate
16. Device hot-plug and removal on Windows and macOS while held exclusively
17. Time-to-first-audio is a composed budget
18. Bundle vs. system GStreamer — three options

---

## 1. Linux — ALSA `hw:` direct

Opening `hw:CARD,DEV` bypasses `dmix`, `plug`, and (with §2's D-Bus handshake) the sound server.

**Sone** (`ref:sone/src-tauri/src/audio.rs`) — GStreamer decodes into an `appsink`; a dedicated
`alsa-writer` thread owns the `snd_pcm_t`, bypassing GStreamer's own audio sinks entirely (build via
`gstreamer`/`gstreamer-app` 0.23, ALSA I/O via the `alsa` 0.10 crate).

Format probing (`probe_supported_gst_formats`, lines 483-507), priority order:
`S32LE`, `S24LE` (= GStreamer `S24_32LE`), `S243LE` (= GStreamer `S24LE`), `FloatLE`, `S16LE`.
**The naming is inverted between ALSA and GStreamer** — get this backwards and it's a silent 8-bit
shift:

```
ALSA S24_LE  = 24-in-32 container = GStreamer S24_32LE (4 bytes/sample)
ALSA S24_3LE = packed 24-bit      = GStreamer S24LE    (3 bytes/sample)
```

Rate probing (`probe_supported_rates`, lines 545-567):
`44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000, 705600, 768000`.

Bit-perfect format selection (`pick_capsfilter_format`, lines 516-544):

1. pass through if the DAC accepts the source format;
2. otherwise the narrowest **lossless** promotion the DAC accepts — `S16LE -> [S24LE, S24_32LE,
   S32LE]`, `S24LE -> [S24_32LE, S32LE]`, `S24_32LE -> [S24LE, S32LE]` — pure integer container
   changes, valid because `audioconvert` is `dithering=none noise-shaping=none`;
3. otherwise the DAC's widest probed format, reported honestly to the UI as a lossy fallback.

`configure_alsa_hwparams` (lines 569-751) — copy this **except** the buffer/period ordering flagged
below, where tidalt's order is the one to follow:

- `set_access(RWInterleaved)`.
- Bit-perfect: `set_format(requested)` must succeed, no fallback ladder.
- Bit-perfect: `set_rate_resample(false)`. **Correction: this is belt-and-braces, not "the one call
  that makes ALSA bit-perfect."** `snd_pcm_hw_params_set_rate_resample` restricts a *plugin-backed*
  PCM's config space (the `plug`/`rate` chain); a raw `hw:CARD,DEV` PCM has none, so the call is a
  no-op there. It matters only if streamboat ever opens `plughw:`/`default` instead of `hw:`. On
  `hw:`, bit-perfection comes from having no conversion stage at all — **plus the next line**.
- `set_rate(rate, Nearest)`, then read back `get_rate()` and **fail** if it differs — message:
  "DAC doesn't support {n}kHz — turn off bit-perfect mode for compatibility". **This read-back is the
  real guard**, and it's the actual justification for a custom writer (§13): upstream `alsasink`
  calls `set_rate_near` but never reads the negotiated rate back, so `alsasink device=hw:X,Y` cannot
  make this check on its own.
- **What produces that friendly message instead of GStreamer's opaque "Internal data stream error"
  is a deliberate appsink-caps choice, not the ALSA read-back alone.** Sone constrains the DASH
  appsink to the DAC's probed *formats* in both modes, but constrains **rate** only in
  non-bit-perfect mode — there an `audioresample` element bridges any source rate to a
  DAC-supported one, so pinning the rate caps is always satisfiable. In **bit-perfect mode the rate
  caps are deliberately left unconstrained**: there is no resampler to bridge a mismatch, so pinning
  the rate would make GStreamer fail caps negotiation outright (the opaque bus error); leaving it
  unconstrained lets the source rate reach the appsink, whose `FormatHint` drives the ALSA reopen
  above — so it's `configure_alsa_hwparams`'s read-back, not GStreamer negotiation, that fails, and
  that is what produces the actionable toast
  (`ref:sone/src-tauri/src/audio.rs:2896-2917`, comment block above the caps-builder call).
  **Rule:** bit-perfect — leave appsink rate caps unconstrained; non-bit-perfect exclusive — pin
  appsink rate caps to the probed rate list, insert `audioresample`, and emit a
  `Resampling { from, to }` event for the transparency panel (§2) whenever the two differ.
- Channel negotiation falls back to `get_channels_min()` — pro USB interfaces (Focusrite, Audient)
  expose a fixed channel count and reject stereo. When the negotiated device channel count is > 2,
  Sone installs a `mix-matrix` on the `audioconvert` element mapping the stereo source onto output
  channels 0/1 at unity gain, with digital silence on every other channel
  (`stereo_pad_mix_matrix`, `ref:sone/src-tauri/src/audio.rs:2835-2841`, applied at
  `:2885`). It does this at **pipeline-build time**, before the first caps negotiation, specifically
  to avoid a transient 2-channel period that would otherwise reach the ALSA writer and thrash it —
  do not set the matrix reactively on `pad-added`.
- `set_buffer_time_near(500_000)` (500 ms), `set_period_time_near(50_000)` (50 ms). **Sone sets
  buffer time before period time — do not copy that ordering.** tidalt's period-before-buffer
  ordering below exists precisely because some USB DACs report nonsensical
  `period_size_min` values when queried after the buffer is already set; set the period first, then
  size the buffer as a multiple of it (SKILL.md pitfall #5).
- **`sw_params`: `snd_pcm_hw_params()` resets `start_threshold` to 1**, which underruns from the
  first write. Restore `start_threshold = floor(buffer/period)*period` and `avail_min = period`
  afterward.

Writer loop (lines 830+): generation counters drop stale PCM from a torn-down pipeline; hardware
pause via `snd_pcm_pause` when supported, else a software pause writing 50 ms silence buffers to
pace the thread then `drop()`+`prepare()` to flush; XRUN recovery (`EPIPE` -> `prepare()` + silence
kick), suspend recovery (`ESTRPIPE` -> `resume()` retry on `EAGAIN`), `ENODEV` -> "device
disconnected"; full close-and-reopen of the PCM on format change (*"Some hardware (e.g. XMOS USB
controllers) can't reconfigure HW params in-place after `snd_pcm_drop()`"*).

**tidalt** (`ref:tidalt/internal/player/alsa.c`, `mpv.go`) — CGO, FFmpeg decode + libasound out.
Different, equally instructive choices:

- **16-bit format preference:** `S32_LE -> S16_LE -> S24_3LE -> S24_LE`, because *"many USB DACs
  (e.g. CS43198-based devices) have a buggy or non-functional S16_LE USB endpoint but work correctly
  via their native 32-bit endpoint."* **24-bit preference:** `S24_3LE -> S24_LE -> S32_LE`.
- **Period size set first** (1024 frames), buffer afterward at `4x period`. Reason: querying
  `period_size_min` after setting the buffer returns absurd values on some DACs ("87 frames on the
  Hidizs S9 Pro Plus"), producing ~1000 interrupts/s and audible distortion.
- `snd_pcm_hw_params_get_sbits()` reports the hardware's *significant* bit depth (e.g. 24 inside an
  `S32_LE` container).
- `open_hw_device` and `configure_hw_pcm` are **split** so "device busy" (retry on `hw:`) is
  distinguishable from "format refused" (fall back to `plughw:`, mark the path no-longer-bit-perfect,
  show a `(converted)` badge). Memoised per device.

**Dither for narrowing conversions — the case §1's promotion ladder does not cover.** The ladder
above only ever widens or does a lossless container change, so "no dither is ever needed" *inside
that ladder* — but a 24/96 track on a **16-bit-only device** is a narrowing conversion, and
undithered truncation there produces audible correlated distortion on fades and reverb tails. This
case is real on target hardware: Raspberry Pi HDMI needs "16bit/44.1kHz" for hi-res content per
`ref:tidal-connect/userconfig/README.md`, and
`ref:tidal-connect/userconfig/xmos-dac-softvol-s16.asound.conf` pins `format S16_LE`. **Rule:**
bit-perfect mode fails loudly on a narrowing device (consistent with §7 below); non-bit-perfect mode
must enable dither on any bit-depth reduction — in GStreamer, that means *not* setting
`audioconvert dithering=none` on that path (its default is TPDF, which is correct); the transparency
panel should report "dithered 24->16". The same reasoning applies to in-place integer PCM volume
scaling (`ref:sone/src-tauri/src/audio.rs:965-1010`): do gain in f32 and dither on the way back to
integer, don't scale the integer PCM directly.

## 2. Linux — PipeWire/PulseAudio and device reservation

`autoaudiosink` picks `pipewiresink`/`pulsesink`/`alsasink` at runtime; the child is added
asynchronously so it cannot be inspected synchronously (`ref:sone/src-tauri/src/audio.rs:1977-1979`).

**Device reservation — copy tidalt's implementation exactly.** To take `hw:` while PipeWire holds
it, speak `org.freedesktop.ReserveDevice1`. tidalt's `reserveALSADevice`
(`ref:tidalt/internal/player/mpv.go:314-395`):

1. Connect to the **session** bus; if none exists, skip reservation and try the device directly.
2. Call `RequestRelease(int32 MaxInt32)` on `org.freedesktop.ReserveDevice1.Audio{N}` at
   `/org/freedesktop/ReserveDevice1/Audio{N}`, 500 ms deadline.
3. Distinguish three outcomes and keep them distinct:
   - reply `released == false` -> explicit refusal, fail;
   - deadline exceeded -> an owner exists but is slow; **back off, do not steal**
     (`ReplaceExisting` stealing is exactly what the protocol exists to prevent);
   - any other call error -> nobody owns the name, proceed.
4. Wait 200 ms for the previous owner to actually close its handle.
5. `RequestName(name, NameFlagReplaceExisting | NameFlagAllowReplacement)`, require
   `RequestNameReplyPrimaryOwner`.
6. **Correction, previously stated wrong here: release on pause, not only on stop, and reacquire
   on resume** (`ref:tidalt/internal/player/mpv.go:567,890-900`; `ref:tidalt/README.md:12`: "holds
   exclusive access to the audio device only while a track is actually playing — releasing it on
   pause so other applications can use it freely"). Releasing only on stop leaves the device locked
   for the entire time the user has streamboat merely paused, which is exactly the uncooperative
   behavior device reservation exists to avoid on a general-purpose desktop.

Timing budget so the whole handshake fits inside a 3 s shutdown window:
`releaseCallTimeout 500ms + releaseSettleDelay 200ms + openBusyRetryBudget 800ms = 1.5s`, `EBUSY`
retried every 100 ms.

**Trap: a successful `ReserveDevice1` handshake is not a guarantee — treat it as advisory.** The
protocol has no guaranteed server side. `RequestName` can return `PrimaryOwner` simply because
nothing on the session bus implements `ReserveDevice1` for that device at all — no WirePlumber
device-reservation component, no PulseAudio `module-reserve-wrapper`, nothing — in which case the
"reservation" is a name registration with nobody listening, the real holder of the ALSA device never
releases anything, and the subsequent `hw:` open still fails with the exact `EBUSY` the handshake
exists to prevent. **Always still handle `EBUSY` on open with tidalt's retry budget above, regardless
of what the reservation handshake reported** — the two mechanisms are complementary, not either/or.
On release, order matters: release the PCM handle **before** the D-Bus name, never after — releasing
the name first lets a racing client see a free name while the device handle is still held, which
reproduces the exact busy-device failure the reservation exists to prevent. A simpler mechanism worth
naming alongside this: `pactl suspend-sink <sink> 1` (and `... 0` to release) works through
`pipewire-pulse` as well as classic PulseAudio, and some audiophile players use it instead of, or
alongside, the D-Bus dance. **[the vacuous-success mechanism and `pactl suspend-sink` as an
alternative need verification against `docs.pipewire.org`/WirePlumber docs — blocked from every
research pass so far, flagged as a gap, not asserted as fact; the release-ordering rule is inferred
from already-verified facts]**

**`libmpv` has no `ReserveDevice1` support of any kind** — absent from both `DOCS/man/ao.rst` and
`DOCS/man/options.rst`. A libmpv-based engine (Design B in `stacks-comparison.md`) needs this
handshake done in the **host process** around libmpv — reserve, hand the now-free `hw:` device to
mpv via `--audio-device`, release on stop — not inside libmpv. Skipping this makes exclusive mode a
permanent "device busy" error for every PipeWire user, which is exactly the gap Sone has: it opens
the PCM eagerly and maps `EBUSY` to a `device_busy` error string, telling the user in its FAQ to
close whatever else holds the device (`ref:sone/src-tauri/src/audio.rs:770-779`).

**Reading OS mixer state as ground truth.** Sone's `pipeline_probe.rs` shells out to `pactl info` /
`pactl list sinks` (`LC_ALL=C` forced) for server name, default sink, sample spec, volume in dB
(converted to a linear multiplier as `10^(dB/20)`, not trusted from the cubic-mapped percentage),
mute state; maps the sink to `/proc/asound/<alsa.id>` and parses
`/proc/asound/<card>/pcm{N}p/sub{M}/hw_params` for kernel ground truth (format, rate, channels,
period_size, buffer_size, or the literal `closed`). This feeds the "signal path transparency" panel
(`ref:sone/src-tauri/src/signal_path.rs`) — copy this idea; it is the single best user-facing
feature in the reference set.

**The panel's field list, collected in one place** (fields this skill adds elsewhere as individual
requirements — cross-referenced below rather than restated):

1. Backend/engine in use (GStreamer/libmpv/etc.) and, where relevant, the specific sink/AO.
2. Decoded format: codec, bit depth, sample rate, channel count, as read from the manifest/decoder,
   never assumed.
3. Negotiated device format: bit depth, sample rate, channel count actually opened on the DAC.
4. Kernel `hw_params` ground truth (`/proc/asound/<card>/pcm{N}p/sub{M}/hw_params`, above) — the
   final word over whatever the app layer believes it negotiated.
5. OS mixer state — volume (linear, not the cubic-mapped percentage) and mute, read from `pactl`/
   the platform mixer API, not from the app's own volume slider state.
6. Every alteration applied between decode and device, named explicitly rather than left implicit:
   resample (`Resampling { from, to }`, above), bit-depth promotion/narrowing (dither status —
   "dithered 24->16"), format fallback to a non-bit-perfect path, software volume/ReplayGain factor
   (or "ReplayGain: bypassed (bit-perfect)", `playback-behavior.md` §4), and lossy-source
   bit-perfect-not-applicable labeling (`output-backends.md` §7). AAC-decoder-probe tier
   availability (`decoding-and-codecs.md` §3) belongs in the same surface, not a separate one.

**PipeWire's own capabilities** (from PipeWire docs, summarized — re-check exact config keys before
publishing): the audio adapter supports a passthrough mode with no conversions, the mechanism behind
exclusive access; `default.clock.rate`/`default.clock.allowed-rates` control which rates the graph
switches to; per-device `audio.format` can be pinned in WirePlumber. **This is unresolved, not merely
unverified** — see §15 for the specific questions to check. It does not gate *whether* the Flatpak
build can offer bit-perfect output at all (§3: raw ALSA `hw:` is already reachable there via
`--socket=pulseaudio`) — it decides whether a PipeWire-native path can be an equally good
*alternative* to grabbing the raw device.

## 3. Sandbox implications (Flatpak, Snap)

- **Flatpak.** High Tide's manifest (`ref:high-tide/build-aux/io.github.nokse22.high-tide.json`)
  declares `finish-args`: `--share=network`, `--share=ipc`, `--socket=fallback-x11`,
  `--device=dri`, `--socket=wayland`, `--socket=pulseaudio`, `--filesystem=xdg-run/pipewire-0:ro`,
  `--filesystem=xdg-run/discord-ipc-0`. **Correction, previously stated backwards here**:
  `--socket=pulseaudio` already grants `/dev/snd` — Flatpak's own sandbox helper
  (`common/flatpak-run-pulseaudio.c` in `flatpak/flatpak` upstream) binds `/dev/snd` into the
  sandbox whenever `--socket=pulseaudio` is requested and `/dev/snd` exists on the host, "since the
  practical permission of ALSA and PulseAudio are essentially the same." Raw ALSA `hw:` **is**
  reachable inside this manifest as shipped — there is no `--device=all` requirement for it. Owned
  canonically by `streamboat-engineering-baseline/references/packaging-and-distribution.md` §1.
  What genuinely is absent is `--device=all` itself, so non-audio device access is not granted —
  but that was never what exclusive ALSA needed.
- **Snap.** Sone ships `plugs: [audio-playback, alsa]` and instructs
  `sudo snap connect sone:alsa` for exclusive output, because the `alsa` interface is not
  auto-connected (`ref:sone/snap/snapcraft.yaml`) — this manual-connect step is real and is the one
  packaging-channel caveat that survives the Flatpak correction above.
- **Consequence:** don't grey out the exclusive-mode toggle on Flatpak on confinement grounds alone
  — `hw:` is reachable there. Do detect and surface the Snap `alsa` plug's connection state (it can
  be disconnected by policy or by the user), and still ship an unconfined native package
  (deb/rpm/AUR/Nix) as the option with no plug/permission caveats at all.

## 4. Windows — WASAPI and ASIO

- `wasapi2sink` (gst-plugins-bad) has a real `exclusive` boolean property alongside `device`,
  `low-latency`, `mute`, `volume`, `dispatcher`, `continue-on-error`. **The property is new in
  GStreamer 1.28** — its gtk-doc block is tagged `Since: 1.28` and does not exist on 1.26 or
  earlier. Gate any code that sets it on a runtime GStreamer-version check.
- Sone-windows: `wasapi2sink` with `exclusive` from settings, `low-latency=true`, `device` from the
  picker (`ref:sone-windows/src-tauri/src/audio.rs:1236-1246`). Toggling exclusivity mid-playback:
  drop to `Ready`, set the properties, back to `Playing`, re-seek to the saved position — the device
  must actually be released and re-acquired (`:1580-1602`).
- Strawberry generalises: any sink with an `exclusive` property gets it set
  (`ref:strawberry/src/engine/gstenginepipeline.cpp:721-727`), and *derives* exclusivity on Linux
  from the device string starting `hw:`/`plughw:` (`:632-637`).
- **Strawberry's default Windows sink is `directsoundsink`, not WASAPI** — `directsoundsink` is
  raised to `GST_RANK_PRIMARY`, `wasapisink`/`wasapi2sink` demoted to `GST_RANK_SECONDARY`:
  *"wasapisink does not support device switching and wasapi2sink has issues, see #1227"*
  (`ref:strawberry/src/engine/gststartup.cpp:59-74`). A caution flag for GStreamer on Windows.
- Device enumeration: Strawberry has `MMDeviceFinder` (wasapisink/wasapi2sink),
  `UWPDeviceFinder` (`wasapi2sink_`), `DirectSoundDeviceFinder`. Sone-windows enumerates via
  GStreamer's `DeviceMonitor` filtered on `device.api ∈ {wasapi, wasapi2}`, reading `device.id`.
- **ASIO — settled, previously an open question.** Strawberry has an `AsioDeviceFinder` targeting
  `asiosink` — a GStreamer ASIO sink exists, but no reference TIDAL client uses it.
  `asiosink` **does** ship in the mainstream Windows GStreamer runtime: Sone-windows's build
  instructions bundle `gstasio.dll` straight from the official GStreamer MSVC x86_64 runtime
  installer's own `lib/gstreamer-1.0/` directory (`ref:sone-windows/README.md:114-146`, and §14
  below) — it is not a separately-built or third-party sink. Only the Steinberg ASIO SDK's
  redistribution terms for whatever ASIO host component streamboat ships alongside `gstasio.dll`
  remain genuinely open.
- Rust crate: `wasapi` 0.24.0 (MIT) exposes exclusive-mode format probing directly for a
  non-GStreamer path.

**WASAPI exclusive-mode failure states an implementer will hit on day two** (standard WASAPI
territory, not project-specific):

- Exclusive mode requires the per-endpoint *"Allow applications to take exclusive control of this
  device"* checkbox enabled in Windows Sound settings. No API enables it; if off,
  `IAudioClient::Initialize` returns `AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED` — surface a distinct,
  actionable error, mirroring TIDAL's own `deviceexclusivemodenotallowed` native-player event
  (`ref:tidal-sdk-web/packages/player/src/player/nativeInterface.ts`).
- Probe format support per (rate, bit depth, container) with
  `IAudioClient::IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, ...)` — the Windows analogue of §1's
  ALSA format/rate probing ladder.
- `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED` -> re-query `GetBufferSize`, re-initialise with the aligned
  duration (the classic exclusive-mode trap).
- `AUDCLNT_E_DEVICE_IN_USE` is the Windows analogue of ALSA's `EBUSY` — same retry/report treatment
  as §2.
- Run event-driven (`AUDCLNT_STREAMFLAGS_EVENTCALLBACK`) with the feeding thread registered with
  MMCSS (§9).

## 5. macOS — CoreAudio

The weakest platform in every reference implementation.

- `osxaudiosink` properties (`gst-plugins-good/sys/osxaudio/gstosxaudiosink.c`): `device`,
  `unique-id`, `configure-session`, `volume`. **No `exclusive` property, no hog-mode property.**
- GStreamer's CoreAudio HAL *does* implement hog mode — `_audio_device_get_hog`/`_set_hog` around
  `kAudioDevicePropertyHogMode`, plus `_audio_device_set_mixing(device, FALSE)` and physical-format
  setting via `kAudioStreamPropertyPhysicalFormat` with a four-attempt confirm loop — but the only
  call site is `_open_spdif()` (`gstosxcoreaudiohal.c:682-740`), reached only for passthrough.
  **GStreamer cannot do hog-mode bit-perfect PCM on macOS.** Any GStreamer-based streamboat needs
  either (a) an `appsink` -> own CoreAudio writer, mirroring the ALSA one, or (b) accept shared-mode
  output on macOS and say so.
- libmpv can: the `coreaudio` AO honours `--audio-exclusive=yes` and auto-redirects to the dedicated
  `coreaudio_exclusive` AO for compressed formats. `--coreaudio-change-physical-format=yes` changes
  the device's physical format system-wide (equivalent to Audio MIDI Setup's Format field). There is
  also an `avfoundation` AO.
- CamillaDSP (non-TIDAL Rust reference) exposes an `exclusive` (hog) flag on its CoreAudio playback
  device, works internally in 32-bit float, lets CoreAudio convert unless an explicit physical
  `format` (S16/S24/S32/F32) is requested, and warns hog mode breaks virtual devices like BlackHole.
- `coreaudio-rs` 0.14.2 (MIT/Apache-2.0) is the Rust binding for a native CoreAudio backend.

**Per-track sample-rate switching needs a nominal-rate change, not only a physical-format change.**
A hi-res client's core operation — switching 44.1k -> 96k -> 192k between tracks — is a
**`kAudioDevicePropertyNominalSampleRate`** change, which completes *asynchronously* on CoreAudio and
must be waited on via a property listener. **Correction: CamillaDSP is not a "listen and reopen"
precedent — it stops.** A prior draft of this skill (and of the underlying report) said CamillaDSP's
CoreAudio backend "listens for these notifications and reopens." Its own docs say the opposite: on a
rate change, CamillaDSP **stops playback outright** and requires an external config reload to resume
— *"If the capture device sample rate changes, then CamillaDSP will stop. … To continue from this
state, the capture device needs to be closed and reopened. For CamillaDSP this means that the
configuration must be reloaded"*
(`https://github.com/HEnquist/camilladsp/blob/master/backend_coreaudio.md`, "Sample rate change
notifications"). **There is no shipped precedent in this reference set for a player that detects a
CoreAudio rate change and transparently reopens the device itself.** GStreamer's own CoreAudio HAL
shows the same class of asynchrony from the other side, with its four-attempt physical-format confirm
loop (`gstosxcoreaudiohal.c:505-535`) — that *is* real precedent for waiting on the async completion,
just not for reopen-and-continue. The recommendation itself (add a property listener, wait for the
completion, resume writing rather than stopping) is still sound — mark it **[inferred]**, not
"matches CamillaDSP." Without a listener at all, a macOS writer racing the driver on every album that
mixes sample rates is the concrete failure mode.

The TIDAL web SDK's proprietary native-player component (not in any repo) has
`selectDevice(device, mode)` where `mode: 'exclusive' | 'shared'`, and emits
`deviceexclusivemodenotallowed`, `deviceformatnotsupported`, `devicelocked`, `devicenotfound`,
`devicevolumenotsupported`, `devicedisconnected`, plus `active-device-pass-through-changed`
(`ref:tidal-sdk-web/packages/player/src/player/nativeInterface.ts`). This is the error-state
checklist TIDAL itself considers necessary — reuse it.

## 6. mpv/libmpv exclusive-mode scope across platforms

**`--audio-exclusive=yes` does not mean the same thing on every AO.** mpv's own docs: *"This only
works for some audio outputs, such as `wasapi`, `coreaudio`, `pipewire` and `audiounit`. Other audio
outputs silently ignore this option."* **The `alsa` AO is not in that list** — passing
`--audio-exclusive` with `--ao=alsa` is a silent no-op. On Linux/libmpv, exclusivity comes only from
device selection: `--audio-device=alsa/hw:X,Y` (plus `--alsa-resample=no`, which is mpv's own default
anyway). On Windows: `--ao=wasapi --audio-exclusive=yes --wasapi-exclusive-buffer=default`. On
macOS: `--ao=coreaudio --audio-exclusive=yes --coreaudio-change-physical-format=yes`.

`--audio-spdif=<codecs>` supports passthrough for `ac3, dts, dts-hd, eac3, truehd, dsd` — the eac3
value is the concrete Atmos-to-AVR path (see `atmos-and-immersive.md`). `dsd` passthrough
"requires an audio output with exclusive device access (currently `wasapi`)" — DSD/DoP passthrough
in mpv is Windows-only.

## 7. Bit-perfect as a feature: hardware volume, dither, and where DSP fits

**"Mute only, no attenuation" is not the only bit-perfect-compatible volume answer**, and for users
without an analogue preamp it's a poor one. Many USB DACs and every Pi I2S HAT expose an ALSA mixer
control that attenuates *in the DAC*, downstream of the digital bitstream — the stream itself stays
bit-perfect:

1. **libmpv:** `ao-volume` (RW) is *"System volume ... on ALSA this usually changes system-wide
   audio volume on a linear curve"* — distinct from `volume` (*"the internal mixer (aka software
   volume)"*). Mixer element selectable with `--alsa-mixer-device`, `--alsa-mixer-name` (default
   `Master`), `--alsa-mixer-index`. **On a libmpv engine, `ao-volume` is the bit-perfect-compatible
   control, not `volume`.** `ao-mute` is its mute-only companion property — bind the UI's mute
   button to it, not to a `volume=0` hack, so mute stays bit-perfect-compatible too.
2. **Direct ALSA:** `snd_mixer_*` on the card's playback element — no reference client does this, it
   is code streamboat would own.
3. **The negative example.** tidal-connect layers an ALSA `type softvol` plugin over `hw:` and
   explicitly warns about the resulting ambiguity: `ref:tidal-connect/bin/common.sh:141-152` checks
   whether a real `Master` mixer control already exists and, if so, renames its own softvol control
   to `SoftMaster`, printing *"*WARNING* Tidal volume slider might act on the hardware volume
   control."* **streamboat's UI must state explicitly which control the slider is bound to.**

Dither for narrowing conversions is covered in §1 above (the ALSA-specific case); the rule is the
same on every platform: bit-perfect fails loudly rather than truncating silently, and any
non-bit-perfect bit-depth reduction gets proper dither.

**Bit-perfect mode also disables loudness normalization, not only the volume slider** — see
`playback-behavior.md` §4 for the full ReplayGain interaction; the summary is that Sone's
bit-perfect pipeline branch builds neither the user-volume nor the ReplayGain GStreamer element, and
the transparency panel should say "ReplayGain: bypassed (bit-perfect)" rather than show an unapplied
gain factor.

**"Bit-perfect" is undefined for the lossy tiers — state the rule explicitly, do not leave it
implied.** Every bit-perfect discussion in this skill is written as if every stream were FLAC.
Bit-perfection is only a meaningful claim for the **lossless** tiers, because a lossy decode has no
canonical output word length in the first place — an AAC decoder emits float or a chosen integer
depth by its own convention, so there is no "original bits" to preserve end to end. Consequences: (a)
for `LOW`/`HIGH`, the transparency panel should say "lossy source — bit-perfect not applicable,"
never claim a guarantee it cannot define; (b) the lossless-integer-widening promotion ladder in §1
does not apply to AAC decode output and needs its own branch, or an explicit non-answer; (c) do not
apply "bit-perfect implies no volume control" as one global mode switch across both source types — a
user who enables bit-perfect should not lose volume control on their `LOW`/`HIGH` tracks for a
guarantee that source never had; scope the mode per-track by source type. (d) the *source* bit depth
shown in the transparency panel must come from the manifest (`Representation@id`,
`tidal-manifest-api.md` §4) or the FLAC `STREAMINFO` block, never from decoder/GStreamer output caps —
those describe the post-decode container word width, not the original source depth, and conflating
the two is exactly the kind of inaccuracy that would make the panel untrustworthy.

**Is there any DSP ever — EQ, crossfeed, upsampling, room correction?** Any of it is incompatible
with bit-perfect by definition. CamillaDSP (cited throughout this file for its CoreAudio backend) is
the natural "we don't build this, users route through it externally" answer — deciding not to build
a filter chain is cheaper than building one later, and worth stating explicitly as policy rather
than leaving implicit. This, and whether bit-perfect ships on or off by default, are open owner
decisions — see SKILL.md.

## 8. Position: correcting for buffered-but-unplayed frames

Every position readout in the reference set is derived from frames **written**
(`frames_written / rate` in Sone, `ref:sone/src-tauri/src/audio.rs:2350,2377`), not frames
**played**. `rg 'snd_pcm_delay|get_delay'` across every reference checkout returns nothing — no
client corrects for the device buffer. With Sone's own ~500 ms ALSA buffer, this means the progress
bar, MPRIS `Position`, and any scrobble timestamp can run up to half a second ahead of what is
actually audible — and it is wrong at the exact moment a gapless transition is timed off it. **The
fix is `snd_pcm_delay()`** (`PCM::delay()` in the Rust `alsa` crate): `played = frames_written -
delay`. On libmpv this is already handled — `time-pos`/`playback-time` are AO-delay corrected — a
concrete advantage of a libmpv-based engine and a required correction if a GStreamer/own-writer
design ships.

## 9. Real-time thread scheduling for the writer thread

A ~500 ms ALSA buffer with 50 ms periods survives on a desktop and glitches on a loaded Pi without
real-time-ish scheduling. Strawberry does this correctly:
`ref:strawberry/src/engine/gstenginepipeline.cpp:1788-1803`, `GstEnginePipeline::TaskEnterCallback`:

```cpp
#ifdef Q_OS_UNIX
sched_param param{}; param.sched_priority = 40;
pthread_setschedparam(pthread_self(), SCHED_RR, &param);
#endif
#ifdef Q_OS_WIN32
SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
#endif
```

installed as GStreamer's task-enter hook, so every streaming thread gets it. Beyond what Strawberry
does:

- On Linux, an unprivileged process usually cannot call `sched_setscheduler(SCHED_RR)` without
  `CAP_SYS_NICE` or an `rtprio` limit in `/etc/security/limits.d`; the portable route is RealtimeKit
  (`org.freedesktop.RealtimeKit1.MakeThreadRealtime` on the system bus — the same mechanism PipeWire
  and JACK use), falling back to `nice()`.
- On Windows, join MMCSS: `AvSetMmThreadCharacteristics("Pro Audio", ...)`.
- On macOS: `thread_policy_set` with `THREAD_TIME_CONSTRAINT_POLICY`.
- The discipline that makes the priority worth having: no allocation, no locks, no logging inside
  the writer loop. Sone follows this with a preallocated `silence_buf` and atomics.

## 10. Verifying bit-perfectness — a test plan

"Bit-perfect" is a claim about bytes; without an automated check, every refactor of the
format-promotion ladder or the ALSA reopen path risks silently breaking it. Four headless-friendly
mechanisms:

1. **ALSA loopback:** load `snd-aloop`, play into `hw:Loopback,0`, capture from `hw:Loopback,1`
   (`arecord -D hw:Loopback,1 -f S32_LE -r 96000`), byte-compare against the reference decode. The
   only true end-to-end proof, and it runs headless.
2. **Decoder self-check:** FLAC's `STREAMINFO` block carries an MD5 of the *unencoded* audio —
   decode-and-hash proves the decoder half independent of the output path.
3. **Kernel ground truth as an assertion, not just UI:** `/proc/asound/<card>/pcm<N>p/sub<M>/hw_params`
   (§2) — assert on it in CI.
4. **On libmpv:** `audio-out-params` is *"Same as `audio-params`, but the format of the data written
   to the audio API"* — read back and assert it equals the source format/rate;
   `current-ao`/`audio-device-list` complete the picture.

Also write a unit test for the §1 24-bit ALSA/GStreamer naming inversion: `S24_LE` -> 4
bytes/frame/channel, `S24_3LE` -> 3.

**No test design exists anywhere in the reference set for gapless correctness**, even though this
skill flags AAC-tier gapless as unverified (`decoding-and-codecs.md` §5) and a real ReplayGain
boundary bug exists on the FLAC path too (`playback-behavior.md` §1). Shape: play two synthetic FLAC
tracks whose concatenation is a continuous sine wave, capture via `snd-aloop` (mechanism 1 above), and
assert phase continuity / no inserted or dropped samples at the boundary. Needs fixture audio the
project does not have and cannot take from TIDAL: generate synthetic sine tracks at 16/44.1, 24/96,
24/192, plus an AAC encode of the same, and commit them alongside the manifest fixtures
(`tidal-manifest-api.md` §16).

## 11. Device identity and persistence across reboots/hot-plug

ALSA card *indices* are assignment-order dependent: unplug and replug a USB DAC, or add a second
card, and `hw:1,0` now points at something else. tidal-connect already configures by card **name**:
`CARD_NAME=D10` (`ref:tidal-connect/samples/topping-d10.env`), and its asound.conf files resolve by
card id string (`card DAC`, `card snd_rpi_hifiberry_dacplus`, `card "vc4hdmi"`). Sone, by contrast,
enumerates through GStreamer's `DeviceMonitor` and falls back to composing `hw:C,D` from
`alsa.card`+`alsa.device` — the index form (`ref:sone/src-tauri/src/audio.rs:3285-3287`).
**Recommendation, with the exact string forms:** on Linux, persist the ALSA card *id* string (read
from `/proc/asound/cards`) and open `hw:CARD=<id>,DEV=<n>` (e.g. `hw:CARD=D10,DEV=0`) —
index-independent, survives replug/reboot reordering; on libmpv the equivalent is
`--audio-device=alsa/hw:CARD=D10,DEV=0`. On Windows, persist the WASAPI endpoint id
(`IMMDevice::GetId()`, the `{0.0.0.00000000}.{guid}`-shaped string) — already what Sone-windows reads
as `device.id` from GStreamer's `DeviceMonitor` (`ref:sone-windows/src-tauri/src/audio.rs:2040-2055`).
On macOS, persist `kAudioDevicePropertyDeviceUID`. In every case, resolve to an index only at open
time, and when the saved device is absent, refuse to silently fall back to `default` — offer the user
the choice instead. A silent fallback to the motherboard codec on a missing DAC is exactly the failure
that would make an audiophile client untrustworthy.

No reference project implements true device hot-plug re-binding. Sone's ALSA writer detects
`ENODEV` from `snd_pcm_writei` and emits `audio-error {kind: "device_disconnected"}`, tearing down
the pipeline. GStreamer's `DeviceMonitor` emits added/removed bus messages and is the right source
for a live device list, but Sone only polls it once with a 2 s timeout because *"GStreamer 1.28+
starts providers async, so devices() may initially be empty"*
(`ref:sone/src-tauri/src/audio.rs:3254-3266`). **This workaround is version-scoped and partly
stale:** GStreamer 1.28.3 changed `devicemonitor` to wait for its start thread before listing
devices, so this poll is only needed on 1.28.0-1.28.2 — scope any copied workaround to that window,
and subscribe to `DeviceMonitor` bus messages rather than polling as the long-term design.
(GStreamer's 1.28 series is well past 1.28.2/1.28.3 by now — at 1.28.7 or later as of 2026-09 — see
`decoding-and-codecs.md` §1; re-check the current point release rather than trusting a number here.)

## 12. `dashdemux2` outranks `dashdemux` by default — the rank-collision correction

Choosing `uridecodebin` over `uridecodebin3` (for the "legacy dashdemux, works with `data:` URIs and
pre-1.26.10 GStreamer" reasons in `decoding-and-codecs.md` §1) does **not** by itself select the
legacy DASH demuxer. Both elements autoplug for `application/dash+xml`; `dashdemux` (legacy,
gst-plugins-bad) registers at `GST_RANK_PRIMARY`, while `dashdemux2`
(gst-plugins-good's `adaptivedemux2`) registers at `GST_RANK_PRIMARY + 1` — one rank higher.
Whenever `adaptivedemux2` is installed (the normal case on any mainstream distro, and true of
Sone-windows's own bundled plugin set — see §14), plain `uridecodebin` autoplugs `dashdemux2`, not
the legacy element, regardless of source-element choice. **Add an explicit startup step**:
`gst_plugin_feature_set_rank(dashdemux2_factory, GST_RANK_NONE)` (or raise `dashdemux` above it) —
the same mechanism Strawberry already uses to prefer `directsoundsink` over `wasapisink`/
`wasapi2sink` on Windows (§4, `ref:strawberry/src/engine/gststartup.cpp:57-77`) — or hook
`decodebin::autoplug-select` to reject `dashdemux2` explicitly. Without this, a Debian-13/Pi build on
GStreamer 1.26.2 (or a naive Windows bundle carrying both plugins) autoplugs `dashdemux2` and fails
on FLAC-in-DASH no matter which `uridecodebin` variant is used.

**Verify the rank demotion actually took effect** with `GST_DEBUG=dashdemux*:5` — the wildcard
matches both the legacy `dashdemux` and `dashdemux2` GStreamer debug categories, so the autoplug
trace shows directly which element got picked for a given manifest rather than inferring it
indirectly from playback success or failure.

## 13. What a custom ALSA writer buys over plain `alsasink`

**Correction, previously overstated the case for skipping a custom writer**: this section used to
claim Strawberry proves plain `alsasink device=hw:X,Y` gets exclusive/bit-perfect output with no
custom writer at all. That is not what Strawberry's code shows.
`ref:strawberry/src/engine/gstenginepipeline.cpp:632-637` sets Strawberry's Linux
`exclusive_mode_` flag purely because the configured device string starts with `hw:` **or
`plughw:`** — a device-prefix inference, not a verified bit-perfect code path — and
`plughw:` is by definition a converting plug layer, so the same flag fires for a non-bit-perfect
device too. `rg -i 'bit.perfect' strawberry/src/` returns nothing: there is no dedicated
bit-perfect logic anywhere in Strawberry's source (`tech-stack-evaluation/references/audio-engine-comparison.md`
§8 and `tidal-oss-landscape/references/comparison-tables.md` Table A confirm this independently).
Strawberry is not a counterexample to needing a custom writer — it simply never built the
verification a custom writer would give it. §1 presents Sone's ~500-line ALSA writer as the way to
be bit-perfect on Linux; the delta plain `alsasink` leaves on the table, read from upstream
`gst-plugins-base/ext/alsa/gstalsasink.c`, is exactly what argues for owning a writer rather than
trusting `alsasink` alone:

**Already handled by `alsasink` — not a reason to avoid it:** format negotiation from caps; period/
buffer time negotiation; and the `sw_params` fix §1/§9 credit to a custom writer —
`alsasink` computes `start_threshold = (buffer_size / avail_min) * avail_min` and calls
`set_avail_min` itself.

**Not handled by `alsasink` — the real reasons to own a writer:** (1) no rate read-back (§1's
correction above) — `alsasink` calls `set_rate_near` but never compares the negotiated rate to the
request, so there's no hook for "DAC doesn't support 192kHz"; (2) no per-format/per-rate probing
ladder, so no lossless-narrowest-promotion policy; (3) no `set_rate_resample(false)` call anywhere in
the file — a `plughw:`/`default` fallback resamples silently instead of failing explicitly;
(4) no `snd_pcm_hw_params_get_sbits()` read-back for reporting true significant bit depth (tidalt
does this, §1); (5) no hook for tidalt's period-before-buffer ordering for DACs with buggy
`period_size_min`; (6) no `snd_pcm_delay()` position correction (§8); (7) no in-writer per-format PCM
volume for exclusive-but-not-bit-perfect mode.

**Recommendation, unchanged in substance but no longer resting on a Strawberry "proof"**: start
Linux output with `alsasink device=hw:` + a `capsfilter` and the `/proc/asound`-reading
transparency panel (§2) as an MVP-speed path, and treat the custom writer as a justified
second-stage upgrade for items (1), (2), (6) specifically. Be explicit in any streamboat-facing
doc that this MVP path is **not verified bit-perfect** until the custom writer (or an equivalent
rate/format read-back) lands — don't cite Strawberry's shipped behavior as evidence that plain
`alsasink` alone is proven sufficient, because it isn't.

## 14. Shipping GStreamer on Windows and macOS

**Windows has a complete, documented recipe** — the only one in the reference set. Sone-windows:
install the official GStreamer MSVC x86_64 runtime + devel MSIs, copy a hand-picked subset into
`src-tauri/gstreamer-runtime/`, run `node scripts/prepare-gstreamer.js`, which generates
`gstreamer-hooks.nsi` (NSIS) and `gstreamer-fragment.wxs` (WiX) so both installer formats carry it —
roughly 20 MB total (`ref:sone-windows/README.md:95-155`). Bundled plugins: `gstadaptivedemux2`,
`gstasio`, `gstaudioconvert`, `gstaudioparsers`, `gstaudioresample`, `gstcoreelements`, `gstdash`,
`gstdecklink`, `gstflac`, `gstisomp4`, `gstplayback`, `gstsoup`, `gsttypefindfunctions`, `gstvolume`,
`gstwasapi2`, `gstwinks`, plus `lib/gio/modules/gioopenssl.dll` — documented as *"essential for
secure HTTPS connection to TIDAL"*; without it `souphttpsrc` cannot do TLS and every stream fails,
making that one DLL load-bearing for the whole pipeline.

**Two consequences worth acting on:** (a) **no AAC decoder is in that plugin list** — no `gst-libav`,
no `faad` — so a Windows build assembled this way plays only the FLAC tiers; `LOW`/`HIGH` need an AAC
decoder deliberately added (see `decoding-and-codecs.md` §2's AAC-availability finding, which also
covers Fedora's parallel gap); (b) **both `gstdash.dll` (legacy) and `gstadaptivedemux2.dll`
(containing `dashdemux2`) are bundled together** — exactly the §12 rank-collision situation, so the
Windows build needs the same rank-demotion startup step, not only Linux.

**No equivalent recipe exists for macOS anywhere in the reference set** — no reference client ships
GStreamer on macOS at all. Cost and schedule Design A's macOS packaging (a `GStreamer.framework` or
private dylib tree, universal arm64+x86_64, notarization of ~30 unsigned dylibs,
`GST_PLUGIN_SYSTEM_PATH` set inside the `.app` bundle) as **unproven**, not as "needs shipping the
plugin set" — that phrasing understates the risk relative to the documented Windows path.

## 15. Bit-perfect through PipeWire — unresolved, a second path alongside raw ALSA `hw:`

§3 established that raw ALSA `hw:` is already reachable inside the Flatpak sandbox via
`--socket=pulseaudio` — so this section is not about whether Flatpak *can* do bit-perfect output at
all, it already can. What's still open is whether a PipeWire-native path is *also* bit-perfect and
therefore a viable alternative to grabbing the raw device directly, via
`--filesystem=xdg-run/pipewire-0`, already granted in High Tide's manifest. **This could not be
resolved in this research pass** — `docs.pipewire.org` is
blocked from the environment, and web search returned only forum-level material
(`bbs.archlinux.org/viewtopic.php?id=290859`, "[SOLVED] Get bit-perfect audio with PipeWire"). Before
treating the Flatpak grey-out as final, check
`docs.pipewire.org/page_man_pipewire-props_7.html` (`audio.format`, `audio.rate`, `audio.channels`,
`api.alsa.disable-mixer`, `api.alsa.period-size`, `api.alsa.headroom`),
`docs.pipewire.org/page_man_pipewire_conf_5.html` (`default.clock.rate`,
`default.clock.allowed-rates`), and the WirePlumber 0.5 device-config docs, and answer three
questions: (a) does the graph convert everything to F32 before the sink, and does an S24 stream
survive bit-exactly (F32's 24-bit mantissa preserves it; true S32 does not)? (b) does a stream whose
format/rate exactly match the sink node bypass the resampler and volume stage, and how does a client
force that (`node.lock-quantum`, `node.dont-remix`, per-device `audio.format`/`audio.rate` in
WirePlumber)? (c) does `default.clock.allowed-rates` need pre-populating with the full rate set for
per-track rate-following — i.e. is this a user/distro config step streamboat must document, not
something the app can request at runtime? Two facts elsewhere in this skill bear on the answer: High
Tide disables gapless entirely on `pipewiresink` for an unexplained reason
(`playback-behavior.md` §1); mpv's `pipewire` AO accepts `--audio-exclusive=yes` (§6) — which, if it
means what it says, is a documented PipeWire exclusive path Design B may get for free inside Flatpak.

## 16. Device hot-plug and removal on Windows and macOS while held exclusively

§11 covers Linux device identity/hot-plug in detail; Windows and macOS are uncovered, and **device
removal while a device is held exclusively** is uncovered on every platform. Unplugging a USB DAC
mid-track is routine and, in exclusive mode, is a hard failure that must become a clean, recoverable
state, not a crash. **No reference client handles this on Windows or macOS** (Sone's `ENODEV`
teardown, §11, is the only handling anywhere in the set) — write it from platform APIs directly:

- **Windows:** register an `IMMNotificationClient` on `IMMDeviceEnumerator` for
  `OnDeviceStateChanged`, `OnDeviceRemoved`, `OnDeviceAdded`, `OnDefaultDeviceChanged`. A removed or
  invalidated endpoint surfaces on the render client as `AUDCLNT_E_DEVICE_INVALIDATED` from
  `GetBuffer`/`ReleaseBuffer` — not in §4's failure-state list — and needs a full
  stop-release-reacquire cycle, not a retry.
- **macOS:** add a property listener on `kAudioHardwarePropertyDevices` (add/remove) and on
  `kAudioObjectPropertyDeviceIsAlive`/`kAudioHardwarePropertyDefaultOutputDevice` for the held
  device. Release hog mode explicitly on device loss or it can leak; CamillaDSP's BlackHole warning
  (§5) applies to anyone routing through Loopback/Soundflower too.
- Tie both to §11: the reconnect path resolves the *persisted stable device id*, and re-opens only if
  it is the same device — never silently fall back to `default`.

## 17. Time-to-first-audio is a composed budget

"Press play, hear music" is the most-felt performance characteristic, and the exclusive/bit-perfect
path has several genuinely serial steps this skill documents individually but never sums. Composed:
manifest resolution (1 RTT on v2, up to four sequential requests on the v1 cascade — see
`tidal-manifest-api.md` §7); the `ReserveDevice1` handshake budget, `500 + 200 + 800 ms = 1.5 s`
(§2); the ALSA/WASAPI/CoreAudio device open, including macOS's asynchronous
`kAudioDevicePropertyNominalSampleRate` wait (§5); CDN connect + init segment + first media segment;
`start_threshold` = the full ~500 ms ALSA buffer before sound starts (§1); and, if implemented, the
DAC warm-up settle delay (`playback-behavior.md` §11) after device open. Serially, that's comfortably
2-3 seconds. **The fix
is architectural:** device reservation and open depend only on the *chosen device*, not the track —
run them **concurrently** with manifest resolution, even starting at app launch or on device
selection. Lower `start_threshold` for the very first buffer of a session and raise it back to
steady-state after. Design the device state machine with `Resolving` and `Reserving`/`Open(fmt)` as
genuinely parallel states, not sequential ones. Add an explicit, testable budget to the spec (e.g.
under 500 ms warm, under 1.5 s cold).

## 18. Bundle vs. system GStreamer — three options

Two hard version floors exist: >= 1.26.10 for FLAC-in-DASH, >= 1.28 for `wasapi2sink exclusive`
(`decoding-and-codecs.md` §1, §4 above). Debian/Raspberry Pi OS ships 1.26.2 — below both. That only
forces the §12 rank-demotion workaround if streamboat links the **system** GStreamer; Windows/macOS
must bundle regardless (§14), so the floors are free there. Three options: **(1) system GStreamer on
Linux, bundled elsewhere** — cheapest Linux packaging, forces §12's workaround, gives Linux the worst
FLAC-in-DASH story of the three desktop platforms. **(2) bundled GStreamer everywhere** (Flatpak with
a pinned `org.freedesktop.Platform` + GStreamer module, or an AppImage) — uniform behaviour, meets
both floors, costs ~20 MB per platform (§14) plus owning security updates for a media stack.
**(3) libmpv (Design B)** — the whole question collapses: FFmpeg's DASH path has no 1.26.10-equivalent
floor and `--audio-exclusive` needs no 1.28-equivalent floor, an argument for Design B worth weighing
against Design A's version-floor cost. Flatpak makes option (2) cheap on Linux and is the same
channel §15's PipeWire question may keep bit-perfect alive under — decide the two together.
