//! Constants shared by every Snapcast-output code path (D-034):
//! [`gst`](crate::gst)'s `tcpclientsink` branch, [`mpv`](crate::mpv)'s
//! FIFO-plus-pump-thread branch, and `streamboat snapcast-plugin`'s
//! `GetProperties` response (`streamboat-desktop`).
//!
//! `OutputConfig::Snapcast` is a fixed-format output: streamboat resamples
//! every track to one PCM format and connects out, as a TCP *client*, to a
//! `tcp://` stream source snapserver is listening on — the connection
//! direction snapcast calls `mode=server` in its own config (snapserver
//! holds the socket open; the audio source, streamboat here, dials in).
//! `snapserver.conf`'s matching stream line
//! (`headless-and-tidal-connect/references/mpd-and-multiroom.md` §2,
//! `badaix/snapcast` `doc/configuration.md`):
//!
//! ```text
//! stream = tcp://0.0.0.0:4953?name=streamboat&mode=server&sampleformat=48000:16:2
//! ```
//!
//! `4953` is not a Snapcast-assigned port (its own fixed ports are
//! 1704/1705/1780/1788, all already spoken for — see the mDNS service types
//! in `snapcast_discover`); it is only an example the stream line above
//! needs a number for. Any free port works as long as the `sampleformat`
//! query parameter matches [`SAMPLE_RATE`]/[`BIT_DEPTH`]/[`CHANNELS`]
//! exactly, since snapserver — not streamboat — enforces that format on the
//! stream once the TCP connection is up.

/// Snapcast's stream `sampleformat` is fixed per stream
/// (`<rate>:<bits>:<channels>`, here `48000:16:2`) — chosen because it is
/// CD-quality-adjacent, comfortably inside every Snapcast client's support,
/// and matches go-librespot's own documented pipe-output default rate, so a
/// snapserver config shared with another source needs no per-stream
/// override. This is a *resampling* output mode by construction (D-034): it
/// is never claimed to be bit-perfect, so picking 48 kHz over TIDAL's own
/// 44.1/48/96/192 kHz source rates costs nothing that bit-perfect mode
/// doesn't already give up by being a different, mutually exclusive output.
pub const SAMPLE_RATE: u32 = 48_000;
/// Signed 16-bit little-endian, matching the `16` in `sampleformat`.
pub const BIT_DEPTH: u32 = 16;
pub const CHANNELS: u32 = 2;

/// GStreamer's format name for [`BIT_DEPTH`]-bit signed PCM.
pub const GST_FORMAT: &str = "S16LE";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_format_matches_the_documented_snapserver_line() {
        // Guards the module doc's `sampleformat=48000:16:2` against a
        // constant changing without the doc comment being updated too.
        let sampleformat = format!("{SAMPLE_RATE}:{BIT_DEPTH}:{CHANNELS}");
        assert_eq!(sampleformat, "48000:16:2");
    }
}
