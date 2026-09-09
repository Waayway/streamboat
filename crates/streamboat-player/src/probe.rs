//! Startup decoder probe (D-003): which quality tiers this build can
//! actually decode, so the shell can grey out an unreachable tier in the
//! Settings quality picker instead of discovering the failure mid-playback,
//! and cap a requested ceiling that exceeds what is decodable.
//!
//! On the GStreamer backend (Linux, D-016) this looks for the concrete
//! element factories LOSSLESS/HI_RES_LOSSLESS (`flacdec`) and LOW/HIGH
//! (`avdec_aac` from LGPL FFmpeg, or `faad`) need — a stripped-down
//! GStreamer install can have one without the other. On the libmpv backend
//! (Windows/macOS, D-016) ffmpeg is bundled with mpv, so every tier is
//! assumed reachable; there is nothing to probe.

use streamboat_core::AudioQuality;

/// Which quality tiers this build's decoders can actually produce sound
/// for. Independent per tier: a GStreamer install can have FLAC support but
/// not AAC, or vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecoderSupport {
    pub low: bool,
    pub high: bool,
    pub lossless: bool,
    pub hi_res_lossless: bool,
}

impl DecoderSupport {
    /// Every tier reachable — the libmpv backend's constant answer
    /// (Windows/macOS, D-016): ffmpeg ships bundled with mpv, so this crate
    /// never needs to probe individual decoders the way the GStreamer
    /// backend does.
    pub const fn all() -> Self {
        Self {
            low: true,
            high: true,
            lossless: true,
            hi_res_lossless: true,
        }
    }

    /// Nothing reachable — used when GStreamer itself fails to initialise,
    /// which would also fail every other engine operation.
    const fn none() -> Self {
        Self {
            low: false,
            high: false,
            lossless: false,
            hi_res_lossless: false,
        }
    }

    /// Whether `q` is reachable at all. The retired MQA-era tier
    /// (`HiResLegacy`) is never requested by the quality ladder
    /// (`docs/architecture.md`'s "Quality ladder" note) and is always
    /// reported unreachable here rather than guessed at.
    pub fn is_reachable(&self, q: AudioQuality) -> bool {
        match q {
            AudioQuality::Low => self.low,
            AudioQuality::High => self.high,
            AudioQuality::Lossless => self.lossless,
            AudioQuality::HiResLossless => self.hi_res_lossless,
            AudioQuality::HiResLegacy => false,
        }
    }

    /// The highest reachable tier at or below `ceiling`, walking
    /// [`AudioQuality::LADDER`] (highest first). `None` only when every
    /// tier is unreachable — a GStreamer install with neither FLAC nor AAC
    /// support, which would fail to play anything at all regardless of this
    /// probe.
    pub fn cap(&self, ceiling: AudioQuality) -> Option<AudioQuality> {
        AudioQuality::LADDER
            .into_iter()
            .filter(|q| q.rank() <= ceiling.rank())
            .find(|q| self.is_reachable(*q))
    }
}

/// Probes whichever engine this build actually runs on this target — the
/// GStreamer backend on Linux, the constant "everything reachable" answer
/// for libmpv elsewhere (D-016) — mirroring `streamboat-desktop`'s own
/// `ui::engine_select` compile-time branch so a caller does not have to
/// duplicate that `cfg` logic just to probe.
pub fn probe() -> DecoderSupport {
    #[cfg(all(target_os = "linux", feature = "gstreamer"))]
    {
        probe_gstreamer()
    }
    #[cfg(not(all(target_os = "linux", feature = "gstreamer")))]
    {
        probe_libmpv()
    }
}

/// Probes the GStreamer backend's element factories directly. Calling
/// `gstreamer::init()` more than once (this crate's other GStreamer code
/// paths, `gst.rs`, also call it) is safe — GStreamer's own init guard is
/// idempotent and thread-safe.
#[cfg(feature = "gstreamer")]
pub fn probe_gstreamer() -> DecoderSupport {
    if let Err(e) = gstreamer::init() {
        tracing::warn!(
            error = %e,
            "decoder probe: gstreamer::init failed; assuming no quality tier is decodable"
        );
        return DecoderSupport::none();
    }
    let flac = gstreamer::ElementFactory::find("flacdec").is_some();
    let aac = gstreamer::ElementFactory::find("avdec_aac").is_some()
        || gstreamer::ElementFactory::find("faad").is_some();
    DecoderSupport {
        low: aac,
        high: aac,
        lossless: flac,
        hi_res_lossless: flac,
    }
}

/// The libmpv backend's constant answer (D-016): ffmpeg is bundled, so
/// every tier is always reachable.
pub const fn probe_libmpv() -> DecoderSupport {
    DecoderSupport::all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_reports_every_tier_reachable() {
        let s = DecoderSupport::all();
        for q in AudioQuality::LADDER {
            assert!(s.is_reachable(q), "{q:?} should be reachable");
        }
    }

    #[test]
    fn cap_keeps_the_ceiling_when_everything_up_to_it_is_reachable() {
        let s = DecoderSupport::all();
        assert_eq!(
            s.cap(AudioQuality::HiResLossless),
            Some(AudioQuality::HiResLossless)
        );
        assert_eq!(s.cap(AudioQuality::Low), Some(AudioQuality::Low));
    }

    #[test]
    fn cap_descends_past_an_unreachable_hi_res_tier() {
        let s = DecoderSupport {
            low: true,
            high: true,
            lossless: true,
            hi_res_lossless: false,
        };
        assert_eq!(
            s.cap(AudioQuality::HiResLossless),
            Some(AudioQuality::Lossless)
        );
    }

    #[test]
    fn cap_is_none_when_nothing_up_to_the_ceiling_is_reachable() {
        let s = DecoderSupport::none();
        assert_eq!(s.cap(AudioQuality::HiResLossless), None);
    }

    #[test]
    fn cap_never_considers_the_retired_hi_res_legacy_tier() {
        // HiResLegacy sits between Lossless and HiResLossless by rank but is
        // excluded from AudioQuality::LADDER, so it must never surface as a
        // capped answer even though `is_reachable` always returns false for
        // it (nothing to assert differently here beyond confirming LADDER
        // itself has no such entry, which `all_reports_every_tier_reachable`
        // already iterates).
        assert!(!AudioQuality::LADDER.contains(&AudioQuality::HiResLegacy));
    }

    // Real GStreamer is present in this sandbox (`STREAMBOAT_GST_SINK=fakesink`
    // is set for audio tests; `flacdec` is installed here per the task brief),
    // so this exercises the actual element-factory lookup, not a fake.
    #[cfg(feature = "gstreamer")]
    #[test]
    fn probe_gstreamer_finds_the_installed_flac_decoder() {
        let s = probe_gstreamer();
        assert!(
            s.lossless,
            "flacdec must be found in this environment (see the task brief)"
        );
        assert_eq!(s.lossless, s.hi_res_lossless);
    }
}
