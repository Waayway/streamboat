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

    /// The pinned, encrypted offline cache's directory (D-022), before any
    /// `Settings::offline_dir` override.
    pub fn offline_dir(&self) -> PathBuf {
        self.data.join("offline")
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
    /// Desktop shell theme choice (D-012, D-013): a small set of design
    /// tokens, not a native-per-OS look. Dark is the default.
    pub theme: ThemePreference,
    /// ReplayGain mode (D-019): off / album / track. Persisted here for the
    /// Settings screen; the player/engine side of switching modes live is
    /// not built yet (`docs/architecture.md` "not yet built") — today the
    /// engine always applies the single per-track gain the manifest reports,
    /// equivalent to album mode, and bypasses it in exclusive/bit-perfect
    /// output regardless of this setting.
    pub replay_gain_mode: ReplayGainMode,
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
            theme: ThemePreference::default(),
            replay_gain_mode: ReplayGainMode::default(),
            offline_dir: None,
            offline_validity_days: 30,
            // 20 GiB: generous enough for a handful of hi-res albums,
            // small enough that "refuse over the cap" is reachable in
            // practice rather than a number nobody hits (D-022).
            offline_max_bytes: Some(20 * 1024 * 1024 * 1024),
        }
    }
}

/// Desktop shell theme choice (D-012). A small, closed set on purpose: the
/// owner's brief is one distinctive look with dark/light variants driven by
/// design tokens, not a system-native theme per OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    Dark,
    Light,
}

/// ReplayGain mode (D-019): off, album (default), or track. TIDAL's
/// per-track gain/peak is the only value the API reports either way; "album"
/// vs "track" is normally a normalization-target distinction upstream
/// players make when they have the whole album's loudness data, which this
/// client does not compute independently — see the field doc on
/// [`Settings::replay_gain_mode`] for what is and is not wired up yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReplayGainMode {
    Off,
    #[default]
    Album,
    Track,
}

impl ReplayGainMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ReplayGainMode::Off => "off",
            ReplayGainMode::Album => "album",
            ReplayGainMode::Track => "track",
        }
    }

    pub const ALL: [ReplayGainMode; 3] = [
        ReplayGainMode::Off,
        ReplayGainMode::Album,
        ReplayGainMode::Track,
    ];
}

impl std::fmt::Display for ReplayGainMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ThemePreference {
    pub const ALL: [ThemePreference; 2] = [ThemePreference::Dark, ThemePreference::Light];

    pub fn as_str(self) -> &'static str {
        match self {
            ThemePreference::Dark => "dark",
            ThemePreference::Light => "light",
        }
    }
}

impl std::fmt::Display for ThemePreference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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

    /// `Settings` with every secret/credential field replaced by whether it
    /// is set at all — for `streamboat debug-bundle` (D-029): "redacted
    /// settings (secrets and credentials removed — keep only whether each
    /// is set)". Everything else (quality ceiling, output, ReplayGain mode,
    /// theme, play-reporting flag, offline caps, which scrobble backends
    /// are enabled) is not a secret and is kept as-is.
    pub fn to_redacted_json(&self) -> serde_json::Value {
        serde_json::json!({
            "client_id_set": self.client_id.is_some(),
            "client_secret_set": self.client_secret.is_some(),
            "pkce_client_id_set": self.pkce_client_id.is_some(),
            "pkce_client_secret_set": self.pkce_client_secret.is_some(),
            "pkce_redirect_uri_set": self.pkce_redirect_uri.is_some(),
            "key_storage": self.key_storage,
            "quality_ceiling": self.quality_ceiling,
            "output": self.output,
            "user_agent_override": self.user_agent_override,
            "country_code": self.country_code,
            "play_reporting": self.play_reporting,
            "scrobble": {
                "lastfm": {
                    "enabled": self.scrobble.lastfm.enabled,
                    "api_key_set": self.scrobble.lastfm.api_key.is_some(),
                    "api_secret_set": self.scrobble.lastfm.api_secret.is_some(),
                    "session_key_set": self.scrobble.lastfm.session_key.is_some(),
                },
                "listenbrainz": {
                    "enabled": self.scrobble.listenbrainz.enabled,
                    "user_token_set": self.scrobble.listenbrainz.user_token.is_some(),
                },
            },
            "theme": self.theme,
            "replay_gain_mode": self.replay_gain_mode,
            "offline_dir": self.offline_dir,
            "offline_validity_days": self.offline_validity_days,
            "offline_max_bytes": self.offline_max_bytes,
        })
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

    #[test]
    fn redacted_settings_json_never_contains_a_secret_value() {
        let settings = Settings {
            client_id: Some("plain-client-id".into()),
            client_secret: Some("super-secret-client-secret".into()),
            pkce_client_id: Some("pkce-id".into()),
            pkce_client_secret: Some("pkce-secret".into()),
            ..Settings::default()
        };
        let json = settings.to_redacted_json();
        let text = json.to_string();
        assert!(!text.contains("super-secret-client-secret"));
        assert!(!text.contains("pkce-secret"));
        // The client id itself is also a credential, so it is reduced to a
        // bool too, not just the secret.
        assert!(!text.contains("plain-client-id"));
        assert!(!text.contains("pkce-id"));
        assert_eq!(json["client_id_set"], true);
        assert_eq!(json["client_secret_set"], true);
        assert_eq!(json["pkce_client_id_set"], true);
        assert_eq!(json["pkce_client_secret_set"], true);
        // Non-secret fields survive untouched.
        assert_eq!(json["play_reporting"], true);
    }
}
