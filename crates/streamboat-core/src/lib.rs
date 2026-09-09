//! streamboat-core: the permissively licensed heart of streamboat.
//!
//! This crate knows how to talk to TIDAL's unofficial `api.tidal.com` API on
//! behalf of a logged-in subscriber, how to log in (device-code and PKCE
//! flows), how to keep tokens safe on disk, how to turn a `playbackinfo`
//! response into something a player can open, and it defines the
//! [`proto`] Command/Event types every streamboat front end speaks.
//!
//! It deliberately has no UI, no audio, no windowing and no desktop-OS
//! dependency (see `docs/DECISIONS.md` D-004 and D-045).
//!
//! Boundaries that are policy, not accident (D-022, D-035 and the
//! "player, not ripper" rule in `CLAUDE.md`): encrypted manifests are refused
//! loudly and never decrypted; `playbackmode=OFFLINE` is never requested;
//! nothing here helps produce a playable file that outlives a subscription.

pub mod api;
pub mod auth;
pub mod bootstrap;
pub mod config;
pub mod credentials;
pub mod error;
mod fsutil;
pub mod http;
pub mod instance_lock;
pub mod manifest;
pub mod models;
pub mod privileges;
pub mod proto;
pub mod reporting;
pub mod scrobble;
pub mod token_store;

pub use api::{ResolvedStream, StreamSource};
pub use credentials::{ClientCredentials, ClientPair};
pub use error::{Error, Result};
pub use http::ApiClient;
pub use models::AudioQuality;
pub use token_store::{
    AuthFlow, EncryptedFileStore, KeyLocation, KeyStorage, TokenSet, TokenStore,
};

/// Version string sent in the honest `User-Agent` and reported by `--version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Project URL included in the honest `User-Agent` (D-025).
pub const PROJECT_URL: &str = "https://github.com/Waayway/streamboat";
