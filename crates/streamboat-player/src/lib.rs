//! streamboat-player (GPL-3.0-only): everything between a resolved TIDAL
//! stream and the DAC. The [`engine::Engine`] trait is the contract both
//! backends satisfy (D-016): GStreamer on Linux, libmpv on Windows and macOS.
//! [`player::Player`] owns the queue and turns protocol Commands into Events.
//!
//! This crate is GPL so that engineering adapted from Sone, High Tide and
//! Strawberry (all GPL-3.0) is licence-clean here and never lands in the
//! Apache-2.0 core (D-009).

#[cfg(feature = "alsa-direct")]
pub mod alsa_writer;
pub mod engine;
#[cfg(feature = "gstreamer")]
pub mod gst;
#[cfg(all(feature = "mpris", target_os = "linux"))]
pub mod mpris;
#[cfg(feature = "mpv")]
pub mod mpv;
pub mod player;
pub mod probe;

pub use engine::{Engine, EngineEvent, LoadItem};
pub use player::{Player, PlayerConfig, PlayerDeps, PlayerHandle};
pub use probe::DecoderSupport;

#[cfg(feature = "gstreamer")]
pub use gst::GstEngine;
#[cfg(feature = "mpv")]
pub use mpv::MpvEngine;
