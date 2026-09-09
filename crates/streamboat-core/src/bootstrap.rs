//! One way to build a configured client from disk, shared by every binary.

use std::sync::Arc;

use crate::config::{AppDirs, DeviceIdentity, Settings};
use crate::credentials::ClientCredentials;
use crate::error::Result;
use crate::http::ApiClient;
use crate::token_store::EncryptedFileStore;

pub struct Context {
    pub dirs: AppDirs,
    pub settings: Settings,
    pub device: DeviceIdentity,
    pub api: ApiClient,
}

impl Context {
    /// Resolve directories, load settings and the device identity, resolve
    /// credentials and open the token store.
    pub fn load() -> Result<Self> {
        let dirs = AppDirs::resolve()?;
        dirs.ensure()?;
        let settings = Settings::load(&dirs.settings_path())?;
        let device = DeviceIdentity::load_or_create(&dirs.device_path())?;
        let creds = ClientCredentials::resolve(&settings)?;
        let store = EncryptedFileStore::new(dirs.token_path(), dirs.key_path());
        let api = ApiClient::builder(creds, Arc::new(store))
            .user_agent_override(settings.user_agent_override.clone())
            .country_code(settings.country_code.clone())
            .build()?;
        Ok(Self {
            dirs,
            settings,
            device,
            api,
        })
    }
}
