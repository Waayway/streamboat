//! The engine contract. An engine plays exactly one item at a time, may be
//! handed the next item ahead of time for gapless transitions, and reports
//! what happens through [`EngineEvent`]s on a channel it is given at
//! construction. It never talks to TIDAL: it receives resolved sources.

use std::sync::mpsc::Sender;

use streamboat_core::StreamSource;
use streamboat_core::proto::{OutputConfig, SignalPath};

/// One thing to play. `id` is the player's per-queue-entry id, not a track
/// id: the same track can sit in the queue twice.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadItem {
    pub id: u64,
    pub source: StreamSource,
    /// TIDAL's per-track ReplayGain (dB) and peak (linear), when known.
    pub replay_gain_db: Option<f64>,
    pub peak_amplitude: Option<f64>,
    /// What the manifest said the stream is, for the signal-path panel.
    pub codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    /// Audio for this item has started flowing (also fired at a gapless
    /// transition, for the successor).
    Started {
        id: u64,
    },
    /// The engine is about to run out of data for the current item and will
    /// continue with whatever was handed to `set_next`, if anything.
    AboutToFinish {
        id: u64,
    },
    /// This item played to its end (or was replaced).
    Finished {
        id: u64,
    },
    /// Nothing left to play.
    EndOfStream,
    Buffering {
        percent: u8,
    },
    /// The engine noticed what it is actually decoding/outputting.
    Format {
        id: u64,
        description: String,
    },
    Warning {
        message: String,
    },
    /// Playback of this item stopped on an error.
    Error {
        id: Option<u64>,
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("audio engine unavailable: {0}")]
    Unavailable(String),
    #[error("output error: {0}")]
    Output(String),
    #[error("cannot play this source: {0}")]
    Source(String),
    #[error("{0}")]
    Other(String),
}

pub type EngineResult<T> = std::result::Result<T, EngineError>;

pub trait Engine: Send {
    fn name(&self) -> &'static str;

    /// Replace whatever is playing with this item and start it.
    fn load(&mut self, item: LoadItem) -> EngineResult<()>;

    /// Hand over the successor for a gapless transition (or clear it).
    fn set_next(&mut self, item: Option<LoadItem>);

    fn play(&mut self) -> EngineResult<()>;
    fn pause(&mut self) -> EngineResult<()>;
    fn stop(&mut self) -> EngineResult<()>;
    fn seek(&mut self, position_ms: u64) -> EngineResult<()>;

    /// 0.0 ..= 1.0. Engines in exclusive mode ignore this and say so.
    fn set_volume(&mut self, volume: f32) -> EngineResult<()>;

    /// Takes effect on the next `load`.
    fn set_output(&mut self, output: &OutputConfig) -> EngineResult<()>;

    /// `(position_ms, duration_ms)` for the current item, if known.
    fn position(&self) -> Option<(u64, Option<u64>)>;

    fn signal_path(&self) -> Option<SignalPath>;
}

/// Constructor signature every backend offers.
pub type EventSender = Sender<EngineEvent>;
