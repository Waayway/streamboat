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
    /// The control API's bearer token (D-030), under the data dir like
    /// `tokens.bin` rather than config, since it is a generated credential.
    pub fn control_token_path(&self) -> PathBuf {
        self.data.join("control-token")
    }
}

/// Load the control API's bearer token, generating one on first use: 32
/// random bytes, hex-encoded, written to a 0600 file. `streamboatd` requires
/// `Authorization: Bearer <token>` on every control-API route but
/// `GET /health` (D-030, D-031). The file is kept forever; delete it to force
/// a new token (any client holding the old one is then locked out).
pub fn load_or_create_control_token(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let token = s.trim().to_string();
            if token.is_empty() {
                return Err(Error::Config(format!(
                    "control token file {} is empty",
                    path.display()
                )));
            }
            Ok(token)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let bytes: [u8; 32] = rand::random();
            let token = hex::encode(bytes);
            fsutil::atomic_write(path, token.as_bytes(), 0o600)?;
            Ok(token)
        }
        Err(e) => Err(e.into()),
    }
}

/// User-editable settings. Secrets in here are the user's own choice
/// (a self-supplied client secret), which is why the file is written 0600.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_token_is_generated_once_and_persisted_0600() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control-token");
        let first = load_or_create_control_token(&path).unwrap();
        assert_eq!(first.len(), 64, "32 random bytes, hex-encoded");
        let second = load_or_create_control_token(&path).unwrap();
        assert_eq!(first, second, "the token must survive a restart");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn control_token_rejects_an_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control-token");
        std::fs::write(&path, b"").unwrap();
        assert!(load_or_create_control_token(&path).is_err());
    }
}
