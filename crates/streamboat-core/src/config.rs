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
}

/// User-editable settings. Secrets in here are the user's own choice
/// (a self-supplied client secret), which is why the file is written 0600.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    /// Overrides the environment / build-time client id (D-023).
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    /// Highest tier to request; the cascade descends from here.
    pub quality_ceiling: Option<AudioQuality>,
    pub output: Option<OutputConfig>,
    /// `None` sends the honest `streamboat/<version>` User-Agent (D-025).
    /// Set only as a documented compatibility workaround.
    pub user_agent_override: Option<String>,
    /// ISO country code override; normally taken from the session.
    pub country_code: Option<String>,
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
                let id = Self {
                    client_unique_key: uuid::Uuid::new_v4().simple().to_string(),
                };
                fsutil::atomic_write(path, &serde_json::to_vec_pretty(&id)?, 0o600)?;
                Ok(id)
            }
            Err(e) => Err(e.into()),
        }
    }
}
