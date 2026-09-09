//! Error taxonomy. The distinctions here are load-bearing for the quality
//! cascade and for token refresh (see `tidal-api` skill, transport §6 and
//! auth §12): a 4xxx `subStatus` on a 401 is a playback error, never an auth
//! error, and a network failure must never wipe credentials.

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Transport-level failure (DNS, TLS, connection reset, timeout).
    /// Never clear credentials or cascade quality on this.
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    /// TIDAL answered with an error body.
    #[error("{0}")]
    Api(ApiError),

    /// HTTP 429. The client-wide cooldown has been armed; retry after it.
    #[error("rate limited by TIDAL; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    /// No usable token: the user must log in.
    #[error("not logged in: run `streamboat login`")]
    AuthRequired,

    /// The login flow itself failed (device-code expired, wrong client id, ...).
    #[error("authentication failed: {0}")]
    Auth(String),

    /// A manifest that streamboat will not play, by policy (D-022): encrypted,
    /// DRM-tokened, or an HLS/video manifest on the audio path.
    #[error("refused to play: {0}")]
    ManifestRefused(String),

    /// A manifest we could not understand.
    #[error("manifest error: {0}")]
    Manifest(String),

    #[error(
        "no TIDAL client credentials configured: set STREAMBOAT_CLIENT_ID (and optionally \
         STREAMBOAT_CLIENT_SECRET) in the environment, or put client_id/client_secret in settings.json; \
         see README.md \"Client credentials\""
    )]
    NoCredentials,

    #[error("token store: {0}")]
    TokenStore(String),

    #[error("configuration: {0}")]
    Config(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// An error body from TIDAL. Two wire shapes exist and both are folded in here
/// (`{status, subStatus, userMessage}` and `{errors: [{detail}]}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub sub_status: Option<u32>,
    pub message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TIDAL API error HTTP {}", self.status)?;
        if let Some(s) = self.sub_status {
            write!(f, " subStatus {s}")?;
        }
        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }
        Ok(())
    }
}

impl ApiError {
    /// Sub-statuses that mean "this request cannot succeed at any quality
    /// tier": the cascade must stop, not try a lower tier. Canonical table:
    /// `tidal-api/references/transport.md` §6. `4006` (streaming privileges
    /// lost) and `4033` (subscription up-sell) are deliberately *not* here:
    /// they recover, and must not blacklist the track.
    pub fn is_terminal_playback(&self) -> bool {
        matches!(
            self.sub_status,
            Some(4005 | 4007 | 4010 | 4020 | 4021 | 4022 | 4023 | 4030 | 4031 | 4032 | 4034 | 4035)
        )
    }

    /// A 4xxx sub-status is a playback error even when it rides on HTTP 401;
    /// refreshing the token will not fix it.
    pub fn is_playback_sub_status(&self) -> bool {
        matches!(self.sub_status, Some(4000..=4999))
    }

    /// Token/session errors that call for a refresh (or re-login).
    pub fn is_auth_sub_status(&self) -> bool {
        matches!(self.sub_status, Some(6001 | 11001 | 11002 | 11003 | 11101))
    }

    /// Human-readable hint for the most actionable codes.
    pub fn hint(&self) -> Option<&'static str> {
        Some(match self.sub_status? {
            1002 => {
                "this client id is not registered as a Limited Input Device: it is probably a \
                     web-player id, not a device-flow id"
            }
            4006 => "another device is playing on this account (TIDAL allows one stream at a time)",
            4010 => "monthly stream quota exceeded",
            4020 | 4021 => "session no longer valid: log in again",
            4022 => {
                "TIDAL no longer accepts this client id (rotated or revoked): supply another one"
            }
            4030 => "not available on this subscription tier",
            4032 | 4035 => "not available in this account's region",
            4033 => "requires a higher subscription tier",
            4034 => "not available for this client id",
            _ => return None,
        })
    }
}

impl From<ApiError> for Error {
    fn from(e: ApiError) -> Self {
        Error::Api(e)
    }
}

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Config(msg.into())
    }

    /// Whether the quality cascade should give up immediately on this error
    /// rather than trying the next tier down.
    pub fn stops_cascade(&self) -> bool {
        match self {
            Error::Network(_) | Error::RateLimited { .. } | Error::AuthRequired => true,
            Error::Api(e) => e.is_terminal_playback(),
            _ => false,
        }
    }
}
