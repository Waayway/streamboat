//! Where streamboat keeps things, and the small settings file.
//!
//! Directory layout follows each OS's conventions through `directories`
//! (XDG on Linux, `Application Support` on macOS, `AppData` on Windows) under
//! the permanent app identity `io.github.waayway.streamboat` (D-007).
//! `STREAMBOAT_HOME=<dir>` overrides everything at once, which is what tests
//! and headless boxes use.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil;
use crate::models::AudioQuality;
use crate::proto::OutputConfig;

#[derive(Debug, Clone)]
pub struct AppDirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    pub runtime: PathBuf,
}

impl AppDirs {
    pub fn resolve() -> Result<Self> {
        if let Some(home) = std::env::var_os("STREAMBOAT_HOME") {
            let base = PathBuf::from(home);
            return Ok(Self {
                config: base.join("config"),
                data: base.join("data"),
                cache: base.join("cache"),
                runtime: base.join("run"),
            });
        }
        let dirs = directories::ProjectDirs::from("io.github", "waayway", "streamboat")
            .ok_or_else(|| Error::Config("cannot determine the user's home directory".into()))?;
        let runtime = dirs
            .runtime_dir()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| std::env::temp_dir().join("streamboat"));
        Ok(Self {
            config: dirs.config_dir().to_path_buf(),
            data: dirs.data_dir().to_path_buf(),
            cache: dirs.cache_dir().to_path_buf(),
            runtime,
        })
    }

    /// Create the directories; config and data are private (0700).
    pub fn ensure(&self) -> Result<()> {
        fsutil::ensure_private_dir(&self.config)?;
        fsutil::ensure_private_dir(&self.data)?;
        std::fs::create_dir_all(&self.cache)?;
        fsutil::ensure_private_dir(&self.runtime)?;
        Ok(())
    }

    pub fn settings_path(&self) -> PathBuf {
        self.config.join("settings.json")
    }
    pub fn device_path(&self) -> PathBuf {
        self.config.join("device.json")
    }
    pub fn key_path(&self) -> PathBuf {
        self.config.join("streamboat.key")
    }
    pub fn token_path(&self) -> PathBuf {
        self.data.join("tokens.bin")
    }
    /// The pinned, encrypted offline cache's directory (D-022), before any
    /// `Settings::offline_dir` override.
    pub fn offline_dir(&self) -> PathBuf {
        self.data.join("offline")
    }
}

/// User-editable settings. Secrets in here are the user's own choice
/// (a self-supplied client secret), which is why the file is written 0600.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Device-code client pair; overrides the environment / build-time value (D-023).
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    /// PKCE client pair (desktop login; hi-res is served to this pair).
    pub pkce_client_id: Option<String>,
    pub pkce_client_secret: Option<String>,
    /// Redirect URI for the PKCE flow. The ecosystem PKCE client id only
    /// accepts TIDAL's own `https://tidal.com/android/login/auth`; set this
    /// only to try another capture mechanism with a client id that allows it.
    pub pkce_redirect_uri: Option<String>,
    /// Where the token-file master key lives: `auto` (keyring if reachable,
    /// else a 0600 key file), `keyring` (fail if unreachable), `file`.
    pub key_storage: crate::token_store::KeyStorage,
    /// Highest tier to request; the cascade descends from here.
    pub quality_ceiling: Option<AudioQuality>,
    pub output: Option<OutputConfig>,
    /// `None` sends the honest `streamboat/<version>` User-Agent (D-025).
    /// Set only as a documented compatibility workaround.
    pub user_agent_override: Option<String>,
    /// ISO country code override; normally taken from the session.
    pub country_code: Option<String>,
    /// Report finished plays to TIDAL (D-027). On by default, disclosed in
    /// README.md and the setting itself; see `reporting::PlayReporter` for
    /// exactly what is sent and when it is suppressed.
    pub play_reporting: bool,
    /// Scrobbling to Last.fm and ListenBrainz (D-037), each backend off
    /// until its credentials are supplied.
    pub scrobble: crate::scrobble::ScrobbleSettings,
    /// Overrides `AppDirs::offline_dir()` for the pinned, encrypted offline
    /// cache (D-022). `None` uses `<data_dir>/offline`.
    pub offline_dir: Option<PathBuf>,
    /// Days a pin stays valid before the account must be revalidated online
    /// (a successful `ApiClient::session()` call) before it is served for
    /// playback again (D-022). Mirrors TIDAL's own offline validity window.
    pub offline_validity_days: u32,
    /// Refuse a new pin that would push the cache over this many bytes.
    /// `None` disables the cap, which is not recommended for an
    /// automatically-growing cache — `Some` is the default.
    pub offline_max_bytes: Option<u64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            client_id: None,
            client_secret: None,
            pkce_client_id: None,
            pkce_client_secret: None,
            pkce_redirect_uri: None,
            key_storage: crate::token_store::KeyStorage::default(),
            quality_ceiling: None,
            output: None,
            user_agent_override: None,
            country_code: None,
            // D-027: on by default, unlike every other opt-in field here.
            play_reporting: true,
            scrobble: crate::scrobble::ScrobbleSettings::default(),
            offline_dir: None,
            offline_validity_days: 30,
            // 20 GiB: generous enough for a handful of hi-res albums,
            // small enough that "refuse over the cap" is reachable in
            // practice rather than a number nobody hits (D-022).
            offline_max_bytes: Some(20 * 1024 * 1024 * 1024),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_vec_pretty(self)?;
        fsutil::atomic_write(path, &json, 0o600)?;
        Ok(())
    }

    pub fn quality_ceiling(&self) -> AudioQuality {
        self.quality_ceiling.unwrap_or(AudioQuality::HiResLossless)
    }
}

/// The device identity TIDAL keys authorized devices on (`clientUniqueKey`).
/// Generated once, kept forever, sent identically on every auth call; a fresh
/// value per login would register a phantom device each time
/// (`tidal-api` auth §9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub client_unique_key: String,
}

impl DeviceIdentity {
    pub fn load_or_create(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // 16 hex chars, Sone's shape (python-tidal pads to 1-16).
                let id = Self {
                    client_unique_key: format!("{:016x}", rand::random::<u64>()),
                };
                fsutil::atomic_write(path, &serde_json::to_vec_pretty(&id)?, 0o600)?;
                Ok(id)
            }
            Err(e) => Err(e.into()),
        }
    }
}
