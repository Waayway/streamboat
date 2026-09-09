//! Client credentials: the client ids (and optional secrets) streamboat
//! presents to TIDAL. Two pairs exist in the ecosystem: a device-code pair
//! (headless/CLI login) and a PKCE pair (desktop login, the only one served
//! HI_RES_LOSSLESS in the clear). Policy (D-023): nothing in the repository;
//! an optional build-time default; a user-supplied override in settings. Any
//! shipped credential is extractable and the docs say so.

use crate::config::Settings;
use crate::error::{Error, Result};

#[derive(Clone, PartialEq, Eq)]
pub struct ClientPair {
    pub id: String,
    pub secret: Option<String>,
}

impl std::fmt::Debug for ClientPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientPair")
            .field("id", &self.id)
            .field("secret", &self.secret.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl ClientPair {
    fn from_parts(id: Option<&str>, secret: Option<&str>) -> Option<Self> {
        let id = id.map(str::trim).filter(|s| !s.is_empty())?;
        Some(Self {
            id: id.to_string(),
            secret: secret
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        })
    }

    pub fn has_secret(&self) -> bool {
        self.secret.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientCredentials {
    /// Used by the device-code flow (RFC 8628).
    pub device: Option<ClientPair>,
    /// Used by the PKCE flow; hi-res tiers are served to this pair.
    pub pkce: Option<ClientPair>,
    pub source: CredentialSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    /// `client_id` / `pkce_client_id` in `settings.json`.
    Settings,
    /// `STREAMBOAT_CLIENT_ID` / `STREAMBOAT_PKCE_CLIENT_ID` (and `_SECRET`) at run time.
    Environment,
    /// Embedded when the binary was built with those variables set.
    BuildTime,
    /// Explicitly constructed (tests).
    Explicit,
}

impl ClientCredentials {
    /// A device-code pair only.
    pub fn new(client_id: impl Into<String>, client_secret: Option<String>) -> Self {
        Self {
            device: Some(ClientPair {
                id: client_id.into(),
                secret: client_secret.filter(|s| !s.is_empty()),
            }),
            pkce: None,
            source: CredentialSource::Explicit,
        }
    }

    pub fn with_pkce(
        mut self,
        client_id: impl Into<String>,
        client_secret: Option<String>,
    ) -> Self {
        self.pkce = Some(ClientPair {
            id: client_id.into(),
            secret: client_secret.filter(|s| !s.is_empty()),
        });
        self
    }

    /// Settings first, then the environment, then the build-time embed. A
    /// layer is used as a whole: the first layer that supplies at least one
    /// pair wins, so a settings override replaces the environment entirely.
    pub fn resolve(settings: &Settings) -> Result<Self> {
        let from_settings = Self {
            device: ClientPair::from_parts(
                settings.client_id.as_deref(),
                settings.client_secret.as_deref(),
            ),
            pkce: ClientPair::from_parts(
                settings.pkce_client_id.as_deref(),
                settings.pkce_client_secret.as_deref(),
            ),
            source: CredentialSource::Settings,
        };
        if from_settings.any() {
            return Ok(from_settings);
        }
        let env = |k: &str| std::env::var(k).ok();
        let from_env = Self {
            device: ClientPair::from_parts(
                env("STREAMBOAT_CLIENT_ID").as_deref(),
                env("STREAMBOAT_CLIENT_SECRET").as_deref(),
            ),
            pkce: ClientPair::from_parts(
                env("STREAMBOAT_PKCE_CLIENT_ID").as_deref(),
                env("STREAMBOAT_PKCE_CLIENT_SECRET").as_deref(),
            ),
            source: CredentialSource::Environment,
        };
        if from_env.any() {
            return Ok(from_env);
        }
        let from_build = Self {
            device: ClientPair::from_parts(
                option_env!("STREAMBOAT_CLIENT_ID"),
                option_env!("STREAMBOAT_CLIENT_SECRET"),
            ),
            pkce: ClientPair::from_parts(
                option_env!("STREAMBOAT_PKCE_CLIENT_ID"),
                option_env!("STREAMBOAT_PKCE_CLIENT_SECRET"),
            ),
            source: CredentialSource::BuildTime,
        };
        if from_build.any() {
            return Ok(from_build);
        }
        Err(Error::NoCredentials)
    }

    fn any(&self) -> bool {
        self.device.is_some() || self.pkce.is_some()
    }

    /// The pair to use for the device-code flow. Falls back to the PKCE pair
    /// (TIDAL answers with sub-status 1002 if it is not a device client).
    pub fn device_pair(&self) -> Option<&ClientPair> {
        self.device.as_ref().or(self.pkce.as_ref())
    }

    /// The pair to use for the PKCE flow. Falls back to the device pair.
    pub fn pkce_pair(&self) -> Option<&ClientPair> {
        self.pkce.as_ref().or(self.device.as_ref())
    }

    /// The pair that owns a client id, for refreshing tokens it minted.
    pub fn pair_for(&self, client_id: &str) -> Option<&ClientPair> {
        [self.device.as_ref(), self.pkce.as_ref()]
            .into_iter()
            .flatten()
            .find(|p| p.id == client_id)
    }

    /// Some client id, for headers sent before any login exists.
    pub fn primary_client_id(&self) -> &str {
        self.device_pair().map(|p| p.id.as_str()).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_layer_wins_whole() {
        let s = Settings {
            client_id: Some(" dev ".into()),
            client_secret: Some("".into()),
            pkce_client_id: Some("pk".into()),
            pkce_client_secret: Some("ps".into()),
            ..Default::default()
        };
        let c = ClientCredentials::resolve(&s).unwrap();
        assert_eq!(c.source, CredentialSource::Settings);
        assert_eq!(c.device.as_ref().unwrap().id, "dev");
        assert!(!c.device.as_ref().unwrap().has_secret());
        assert_eq!(c.pair_for("pk").unwrap().secret.as_deref(), Some("ps"));
        assert!(c.pair_for("nope").is_none());
    }

    #[test]
    fn pkce_only_settings_still_serve_device_flow() {
        let s = Settings {
            pkce_client_id: Some("pk".into()),
            ..Default::default()
        };
        let c = ClientCredentials::resolve(&s).unwrap();
        assert_eq!(c.device_pair().unwrap().id, "pk");
        assert_eq!(c.primary_client_id(), "pk");
    }

    #[test]
    fn debug_redacts_secrets() {
        let c = ClientCredentials::new("id", Some("s3cret".into()));
        assert!(!format!("{c:?}").contains("s3cret"));
    }
}
