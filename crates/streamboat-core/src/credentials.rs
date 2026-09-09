//! Client credentials: the client id (and optional secret) streamboat presents
//! to TIDAL. Policy (D-023): nothing in the repository; an optional build-time
//! default; a user-supplied override in settings. Any shipped credential is
//! extractable and the docs say so.

use crate::config::Settings;
use crate::error::{Error, Result};

#[derive(Clone, PartialEq, Eq)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub source: CredentialSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    /// `client_id` in `settings.json`.
    Settings,
    /// `STREAMBOAT_CLIENT_ID` / `STREAMBOAT_CLIENT_SECRET` at run time.
    Environment,
    /// Embedded when the binary was built with those variables set.
    BuildTime,
    /// Explicitly constructed (tests).
    Explicit,
}

impl std::fmt::Debug for ClientCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientCredentials")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .field("source", &self.source)
            .finish()
    }
}

impl ClientCredentials {
    pub fn new(client_id: impl Into<String>, client_secret: Option<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.filter(|s| !s.is_empty()),
            source: CredentialSource::Explicit,
        }
    }

    /// Settings first, then the environment, then the build-time embed.
    pub fn resolve(settings: &Settings) -> Result<Self> {
        if let Some(id) = settings
            .client_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            return Ok(Self {
                client_id: id.trim().to_string(),
                client_secret: settings
                    .client_secret
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                source: CredentialSource::Settings,
            });
        }
        if let Some(id) = std::env::var("STREAMBOAT_CLIENT_ID")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            return Ok(Self {
                client_id: id.trim().to_string(),
                client_secret: std::env::var("STREAMBOAT_CLIENT_SECRET")
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                source: CredentialSource::Environment,
            });
        }
        if let Some(id) = option_env!("STREAMBOAT_CLIENT_ID").filter(|s| !s.trim().is_empty()) {
            return Ok(Self {
                client_id: id.trim().to_string(),
                client_secret: option_env!("STREAMBOAT_CLIENT_SECRET")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                source: CredentialSource::BuildTime,
            });
        }
        Err(Error::NoCredentials)
    }

    /// Hi-res tiers are only served to a client id that has a secret
    /// (`tidal-api` auth §4); without one they are filtered out pre-emptively.
    pub fn has_secret(&self) -> bool {
        self.client_secret.is_some()
    }
}
