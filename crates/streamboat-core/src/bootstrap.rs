//! One way to build a configured client from disk, shared by every binary.

use std::sync::Arc;

use crate::config::{AppDirs, DeviceIdentity, Settings};
use crate::credentials::ClientCredentials;
use crate::error::Result;
use crate::http::ApiClient;
use crate::token_store::{EncryptedFileStore, KeySlot, KeyStorage, NoKeySlot};

/// The keyring service name: the permanent app identity (D-007).
pub const KEYRING_SERVICE: &str = "io.github.waayway.streamboat";
pub const KEYRING_USER: &str = "master-key";
/// The offline cache's own keyring entry (D-022): deliberately separate
/// from `KEYRING_USER` so the token-store master key and the offline-cache
/// key can be rotated, migrated or wiped independently.
pub const OFFLINE_KEYRING_USER: &str = "offline-key";

#[derive(Clone)]
pub struct Context {
    pub dirs: AppDirs,
    pub settings: Settings,
    pub device: DeviceIdentity,
    pub api: ApiClient,
    pub store: Arc<EncryptedFileStore>,
}

/// The OS keyring when compiled in and not disabled by settings, holding
/// the entry named `user` under the permanent app identity. Additive
/// generalisation of [`key_slot`] (same mechanism, a different keyring
/// entry) so other secrets — the offline cache's file key (D-022) — get
/// their own entry instead of sharing `master-key`.
pub fn key_slot_named(storage: KeyStorage, user: &str) -> Arc<dyn KeySlot> {
    if storage == KeyStorage::File {
        return Arc::new(NoKeySlot);
    }
    #[cfg(feature = "keyring")]
    {
        Arc::new(crate::token_store::OsKeyring::new(KEYRING_SERVICE, user))
    }
    #[cfg(not(feature = "keyring"))]
    {
        Arc::new(NoKeySlot)
    }
}

/// The OS keyring when compiled in and not disabled by settings.
pub fn key_slot(storage: KeyStorage) -> Arc<dyn KeySlot> {
    key_slot_named(storage, KEYRING_USER)
}

/// Everything on disk that does not need TIDAL client credentials:
/// directories, settings, the device identity and the token store. The
/// commands that only inspect or maintain local state (`keyring status`,
/// `pins`, `debug-bundle`) load this, so a box with no credential
/// configured yet can still be diagnosed and cleaned up.
#[derive(Clone)]
pub struct LocalContext {
    pub dirs: AppDirs,
    pub settings: Settings,
    pub device: DeviceIdentity,
    pub store: Arc<EncryptedFileStore>,
}

impl LocalContext {
    /// Resolve directories, load settings and the device identity, and open
    /// the token store — no credentials needed.
    pub fn load() -> Result<Self> {
        let dirs = AppDirs::resolve()?;
        dirs.ensure()?;
        let settings = Settings::load(&dirs.settings_path())?;
        let device = DeviceIdentity::load_or_create(&dirs.device_path())?;
        let store = Arc::new(EncryptedFileStore::open(
            dirs.token_path(),
            dirs.key_path(),
            key_slot(settings.key_storage),
            settings.key_storage,
        )?);
        Ok(Self {
            dirs,
            settings,
            device,
            store,
        })
    }
}

impl Context {
    /// Resolve directories, load settings and the device identity, resolve
    /// credentials and open the token store.
    pub fn load() -> Result<Self> {
        let LocalContext {
            dirs,
            settings,
            device,
            store,
        } = LocalContext::load()?;
        let creds = ClientCredentials::resolve(&settings)?;
        let api = ApiClient::builder(creds, store.clone())
            .user_agent_override(settings.user_agent_override.clone())
            .country_code(settings.country_code.clone())
            .build()?;
        Ok(Self {
            dirs,
            settings,
            device,
            api,
            store,
        })
    }
}
