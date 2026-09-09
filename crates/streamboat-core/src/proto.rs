//! The Command/Event protocol: the only way any front end (the iced shell,
//! the CLI, `streamboatd`'s control API, a future remote) drives the player
//! (D-010). Versioned additively: never repurpose a field, only add (D-031).

use serde::{Deserialize, Serialize};

use crate::models::{AudioMode, AudioQuality, TrackSummary};

/// Bumped only for incompatible changes, which the versioning rule forbids;
/// clients query capabilities instead of matching this exactly.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// Replace the queue with these items and start playing the first.
    Play {
        items: Vec<PlayItem>,
    },
    /// Append (or insert next) without interrupting playback.
    Enqueue {
        items: Vec<PlayItem>,
        position: QueuePosition,
    },
    Pause,
    Resume,
    TogglePlayPause,
    Stop,
    Next,
    Previous,
    Seek {
        position_ms: u64,
    },
    /// 0.0 ..= 1.0. Ignored (reported as such) while an exclusive output is active.
    SetVolume {
        volume: f32,
    },
    SetQualityCeiling {
        ceiling: AudioQuality,
    },
    SetOutput {
        output: OutputConfig,
    },
    ClearQueue,
    GetState,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuePosition {
    Next,
    Last,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayItem {
    pub track_id: u64,
}

/// Where audio goes. `Exclusive` opens the device directly (ALSA `hw:` on
/// Linux, WASAPI exclusive on Windows, CoreAudio hog mode on macOS) and
/// bypasses volume and ReplayGain, which the UI must say (D-017, D-019).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum OutputConfig {
    Shared {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device: Option<String>,
    },
    Exclusive {
        device: String,
    },
}

impl Default for OutputConfig {
    fn default() -> Self {
        OutputConfig::Shared { device: None }
    }
}

impl OutputConfig {
    pub fn is_exclusive(&self) -> bool {
        matches!(self, OutputConfig::Exclusive { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    #[default]
    Stopped,
    Buffering,
    Playing,
    Paused,
}

/// What was resolved for the current track: the truth about what is
/// playing, as opposed to what was requested.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StreamInfo {
    /// The quality TIDAL actually delivered (may be lower than requested).
    pub quality: Option<AudioQuality>,
    pub audio_mode: Option<AudioMode>,
    /// `bts`, `dash` or `emu`.
    pub manifest_kind: String,
    pub codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub replay_gain_db: Option<f64>,
    pub peak_amplitude: Option<f64>,
    /// `true` when TIDAL served a preview instead of the full asset.
    pub preview: bool,
}

/// The real decode and output path, for the signal-path panel (D-036).
/// Every field is what the engine can actually observe; unknown is `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SignalPath {
    pub engine: String,
    pub source_format: Option<String>,
    pub decoder: Option<String>,
    pub sink: Option<String>,
    pub device: Option<String>,
    pub device_format: Option<String>,
    pub exclusive: bool,
    /// `Some(reason)` when the engine had to resample or convert.
    pub converted: Option<String>,
    pub volume_applied: bool,
    pub replaygain_applied: bool,
    pub bit_perfect: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub current: Option<TrackSummary>,
    pub current_index: Option<usize>,
    pub queue_len: usize,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub volume: f32,
    pub quality_ceiling: AudioQuality,
    pub output: OutputConfig,
    pub stream: Option<StreamInfo>,
    pub signal_path: Option<SignalPath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
// `State` carries a full snapshot by design; it is sent on change, not in a hot loop.
#[allow(clippy::large_enum_variant)]
pub enum Event {
    /// Full snapshot; sent on connect, on `GetState`, and after any change.
    State {
        state: PlayerState,
    },
    TrackStarted {
        track: TrackSummary,
        stream: StreamInfo,
        index: usize,
    },
    TrackFinished {
        track_id: u64,
    },
    Position {
        position_ms: u64,
        duration_ms: Option<u64>,
    },
    QueueChanged {
        queue: Vec<TrackSummary>,
        current_index: Option<usize>,
    },
    Buffering {
        percent: u8,
    },
    /// Something the user should know that did not stop playback
    /// (hi-res refused for this client id, quality downgraded, ...).
    Warning {
        message: String,
    },
    /// Something that did stop this track or the queue.
    Error {
        message: String,
        track_id: Option<u64>,
    },
    /// The streaming-privileges websocket ("Pushkin") reported that another
    /// device claimed the account's one playback slot: playback has been
    /// paused (D-033). `by` is TIDAL's `clientDisplayName` for that device.
    /// Front ends should show "playback started on `<by>`" and must never
    /// resend a claim automatically after this.
    PlaybackTakenOver {
        by: String,
    },
    /// Login needed; a remote can render its own QR from this.
    AuthRequired {
        verification_url: String,
        user_code: String,
        expires_in_secs: u64,
    },
    AuthOk {
        user_id: Option<u64>,
        country_code: Option<String>,
    },
    EndOfQueue,
    Stopped,
}
