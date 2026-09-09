//! Wire models for the parts of the unofficial v1 API the spike touches.
//! Every optional-on-the-wire field is `Option` and every struct tolerates
//! unknown fields: TIDAL's JSON drifts, and a missing `duration` must never
//! drop an item (`tidal-api` transport §8).

use serde::{Deserialize, Serialize};

/// TIDAL's audio quality tiers, in the names the API uses.
///
/// `HiResLegacy` (`HI_RES`) is the retired MQA tier; it is modelled so a
/// response carrying it deserializes, but streamboat never requests it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AudioQuality {
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "HIGH")]
    High,
    #[serde(rename = "LOSSLESS")]
    Lossless,
    #[serde(rename = "HI_RES_LOSSLESS")]
    #[default]
    HiResLossless,
    #[serde(rename = "HI_RES")]
    HiResLegacy,
}

impl AudioQuality {
    /// The tiers streamboat requests, highest first, with the retired
    /// `HI_RES` tier excluded (dead content since July 2024).
    pub const LADDER: [AudioQuality; 4] = [
        AudioQuality::HiResLossless,
        AudioQuality::Lossless,
        AudioQuality::High,
        AudioQuality::Low,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AudioQuality::Low => "LOW",
            AudioQuality::High => "HIGH",
            AudioQuality::Lossless => "LOSSLESS",
            AudioQuality::HiResLossless => "HI_RES_LOSSLESS",
            AudioQuality::HiResLegacy => "HI_RES",
        }
    }

    /// Higher is better. `HI_RES` sits between LOSSLESS and HI_RES_LOSSLESS.
    pub fn rank(self) -> u8 {
        match self {
            AudioQuality::Low => 0,
            AudioQuality::High => 1,
            AudioQuality::Lossless => 2,
            AudioQuality::HiResLegacy => 3,
            AudioQuality::HiResLossless => 4,
        }
    }

    pub fn is_hi_res(self) -> bool {
        matches!(
            self,
            AudioQuality::HiResLossless | AudioQuality::HiResLegacy
        )
    }

    pub fn is_lossless(self) -> bool {
        self.rank() >= 2
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().replace('-', "_").as_str() {
            "LOW" => Some(Self::Low),
            "HIGH" => Some(Self::High),
            "LOSSLESS" => Some(Self::Lossless),
            "HI_RES_LOSSLESS" | "HIRES_LOSSLESS" | "HIRES" | "HI_RES_FLAC" | "MAX" => {
                Some(Self::HiResLossless)
            }
            "HI_RES" => Some(Self::HiResLegacy),
            _ => None,
        }
    }
}

impl std::fmt::Display for AudioQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AudioQuality {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        AudioQuality::parse(s).ok_or_else(|| {
            format!("unknown quality {s:?}; expected LOW, HIGH, LOSSLESS or HI_RES_LOSSLESS")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioMode {
    #[serde(rename = "STEREO")]
    #[default]
    Stereo,
    #[serde(rename = "DOLBY_ATMOS")]
    DolbyAtmos,
    #[serde(rename = "SONY_360RA")]
    Sony360Ra,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: Option<u64>,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AlbumRef {
    pub id: Option<u64>,
    #[serde(default)]
    pub title: String,
    /// Cover image id (`uuid` with dashes); see `tidal-api` catalog §Images.
    pub cover: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A track as returned by `GET /v1/tracks/{id}` and inside search results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    /// Seconds. The authoritative duration for prefetch and reporting maths.
    pub duration: Option<u32>,
    pub track_number: Option<u32>,
    pub volume_number: Option<u32>,
    pub replay_gain: Option<f64>,
    pub peak: Option<f64>,
    pub allow_streaming: Option<bool>,
    pub stream_ready: Option<bool>,
    pub explicit: Option<bool>,
    pub isrc: Option<String>,
    pub audio_quality: Option<AudioQuality>,
    #[serde(default)]
    pub audio_modes: Vec<AudioMode>,
    pub media_metadata: Option<MediaMetadata>,
    pub artist: Option<Artist>,
    #[serde(default)]
    pub artists: Vec<Artist>,
    pub album: Option<AlbumRef>,
    pub url: Option<String>,
}

impl Track {
    pub fn artist_names(&self) -> String {
        if !self.artists.is_empty() {
            self.artists
                .iter()
                .map(|a| a.name.as_str())
                .filter(|n| !n.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            self.artist
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_default()
        }
    }

    /// Whether the catalogue says this track has a HI_RES_LOSSLESS master
    /// (`mediaMetadata.tags` contains `HIRES_LOSSLESS`). Cheap pre-flight
    /// used to avoid a wasted hi-res request.
    pub fn has_hires_master(&self) -> bool {
        self.media_metadata
            .as_ref()
            .map(|m| m.tags.iter().any(|t| t == "HIRES_LOSSLESS"))
            .unwrap_or(false)
    }
}

/// Compact track description carried in events and queue snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TrackSummary {
    pub id: u64,
    pub title: String,
    pub artists: String,
    pub album: String,
    pub duration_ms: Option<u64>,
    pub cover: Option<String>,
}

impl From<&Track> for TrackSummary {
    fn from(t: &Track) -> Self {
        TrackSummary {
            id: t.id,
            title: t.title.clone(),
            artists: t.artist_names(),
            album: t
                .album
                .as_ref()
                .map(|a| a.title.clone())
                .unwrap_or_default(),
            duration_ms: t.duration.map(|d| u64::from(d) * 1000),
            cover: t.album.as_ref().and_then(|a| a.cover.clone()),
        }
    }
}

/// `GET /v1/sessions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: Option<String>,
    pub user_id: Option<u64>,
    #[serde(default)]
    pub country_code: String,
    pub channel_id: Option<u64>,
    pub partner_id: Option<u64>,
    pub client: Option<SessionClient>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionClient {
    pub id: Option<u64>,
    #[serde(default)]
    pub name: String,
    pub authorized_for_offline: Option<bool>,
}

/// The `user` object inside a token response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenUser {
    pub user_id: Option<u64>,
    pub email: Option<String>,
    pub country_code: Option<String>,
    pub full_name: Option<String>,
    pub username: Option<String>,
}

/// A page of search results (`GET /v1/search/tracks`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub total_number_of_items: Option<u64>,
}
