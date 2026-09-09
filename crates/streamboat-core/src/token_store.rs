//! Token persistence (D-026): tokens live in an AES-256-GCM file written
//! atomically; the 32-byte master key lives in the OS keyring when one is
//! reachable, else in a 0600 key file beside the tokens; a headless box can
//! also supply it as `STREAMBOAT_MASTER_KEY`. The key never moves between
//! locations on its own: a file-only install stays file-only until the user
//! runs `streamboat keyring migrate` (the engineering baseline flags silent
//! promotion as a security-posture change the user is never told about).
//!
//! On-disk token format (after Sone's `crypto.rs`, corrected per the
//! engineering baseline): `MAGIC "SBTK" || VERSION(1) || NONCE(12) ||
//! CIPHERTEXT+TAG`. A bad header is a typed error, never a silent passthrough.
//! Two processes (the CLI and the daemon) may share one store, so refresh
//! runs under an advisory file lock (see `http.rs`).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use base64::Engine as _;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{Error, Result};
use crate::fsutil;

const MAGIC: &[u8; 4] = b"SBTK";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 4 + 1 + 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthFlow {
    #[default]
    DeviceCode,
    Pkce,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    #[serde(default = "default_token_type")]
    pub token_type: String,
    /// Unix seconds.
    pub expires_at: u64,
    #[serde(default)]
    pub scope: String,
    /// The client id that minted these tokens; refresh must use the same one.
    pub client_id: String,
    /// Which flow minted them; PKCE refreshes carry `client_unique_key`.
    #[serde(default)]
    pub flow: AuthFlow,
    #[serde(default)]
    pub client_unique_key: Option<String>,
    pub user_id: Option<u64>,
    pub country_code: Option<String>,
}

fn default_token_type() -> String {
    "Bearer".into()
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_at", &self.expires_at)
            .field("client_id", &self.client_id)
            .field("flow", &self.flow)
            .field("user_id", &self.user_id)
            .field("country_code", &self.country_code)
            .finish()
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl TokenSet {
    pub fn expires_within(&self, secs: u64) -> bool {
        self.expires_at <= now_secs().saturating_add(secs)
    }
}

/// An owned cross-process lock on a store; hold the guard from `write()`
/// across a read-refresh-write sequence.
pub struct StoreLock {
    inner: fd_lock::RwLock<File>,
}

impl StoreLock {
    pub fn write(&mut self) -> Result<fd_lock::RwLockWriteGuard<'_, File>> {
        self.inner
            .write()
            .map_err(|e| Error::TokenStore(format!("cannot lock token store: {e}")))
    }
}

pub trait TokenStore: Send + Sync {
    fn load(&self) -> Result<Option<TokenSet>>;
    fn save(&self, tokens: &TokenSet) -> Result<()>;
    fn clear(&self) -> Result<()>;
    /// `None` when the store is private to this process.
    fn lock_exclusive(&self) -> Result<Option<StoreLock>> {
        Ok(None)
    }
}

/// In-memory store for tests and one-off tools.
#[derive(Default)]
pub struct MemoryTokenStore {
    inner: Mutex<Option<TokenSet>>,
}

impl MemoryTokenStore {
    pub fn with(tokens: TokenSet) -> Self {
        Self {
            inner: Mutex::new(Some(tokens)),
        }
    }
}

impl TokenStore for MemoryTokenStore {
    fn load(&self) -> Result<Option<TokenSet>> {
        Ok(self.inner.lock().unwrap().clone())
    }
    fn save(&self, tokens: &TokenSet) -> Result<()> {
        *self.inner.lock().unwrap() = Some(tokens.clone());
        Ok(())
    }
    fn clear(&self) -> Result<()> {
        *self.inner.lock().unwrap() = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Master key resolution

/// Where the master key may be kept besides the key file.
pub trait KeySlot: Send + Sync {
    /// `Ok(None)` when no key is stored (or the slot is disabled).
    fn get(&self) -> Result<Option<[u8; 32]>>;
    fn set(&self, key: &[u8; 32]) -> Result<()>;
    fn delete(&self) -> Result<()>;
    fn describe(&self) -> String;
}

/// A slot that never holds anything (feature off, or `key_storage = file`).
pub struct NoKeySlot;

impl KeySlot for NoKeySlot {
    fn get(&self) -> Result<Option<[u8; 32]>> {
        Ok(None)
    }
    fn set(&self, _key: &[u8; 32]) -> Result<()> {
        Err(Error::TokenStore(
            "no OS keyring backend in this build".into(),
        ))
    }
    fn delete(&self) -> Result<()> {
        Ok(())
    }
    fn describe(&self) -> String {
        "none".into()
    }
}

/// In-memory slot for tests.
#[derive(Default)]
pub struct MemoryKeySlot {
    key: Mutex<Option<[u8; 32]>>,
    pub fail: Mutex<bool>,
}

impl KeySlot for MemoryKeySlot {
    fn get(&self) -> Result<Option<[u8; 32]>> {
        if *self.fail.lock().unwrap() {
            return Err(Error::TokenStore("keyring unreachable".into()));
        }
        Ok(*self.key.lock().unwrap())
    }
    fn set(&self, key: &[u8; 32]) -> Result<()> {
        if *self.fail.lock().unwrap() {
            return Err(Error::TokenStore("keyring unreachable".into()));
        }
        *self.key.lock().unwrap() = Some(*key);
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        *self.key.lock().unwrap() = None;
        Ok(())
    }
    fn describe(&self) -> String {
        "memory".into()
    }
}

/// The OS keyring (Keychain, Credential Manager, Secret Service) holding the
/// 32-byte key under `service` / `user`. Every call runs on its own thread so
/// a backend that spins up an async runtime never nests inside ours.
#[cfg(feature = "keyring")]
pub struct OsKeyring {
    service: String,
    user: String,
}

#[cfg(feature = "keyring")]
fn describe_keyring_error(e: keyring::Error) -> String {
    match e {
        // `Entry::new` hides why the platform store failed to initialise
        // (no D-Bus session, no Secret Service, ...); `store_status` has it.
        keyring::Error::NoDefaultStore => match keyring::Entry::store_status() {
            Err(status) => format!("no OS keyring reachable ({status})"),
            Ok(()) => "no OS keyring store".to_string(),
        },
        other => other.to_string(),
    }
}

#[cfg(feature = "keyring")]
impl OsKeyring {
    pub fn new(service: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            user: user.into(),
        }
    }

    fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(keyring::Entry) -> keyring::Result<T> + Send + 'static,
    ) -> Result<T> {
        let service = self.service.clone();
        let user = self.user.clone();
        std::thread::Builder::new()
            .name("streamboat-keyring".into())
            .spawn(move || keyring::Entry::new(&service, &user).and_then(f))
            .map_err(|e| Error::TokenStore(format!("keyring thread: {e}")))?
            .join()
            .map_err(|_| Error::TokenStore("keyring call panicked".into()))?
            .map_err(|e| Error::TokenStore(format!("keyring: {}", describe_keyring_error(e))))
    }
}

#[cfg(feature = "keyring")]
impl KeySlot for OsKeyring {
    fn get(&self) -> Result<Option<[u8; 32]>> {
        let service = self.service.clone();
        let user = self.user.clone();
        let joined = std::thread::Builder::new()
            .name("streamboat-keyring".into())
            .spawn(move || keyring::Entry::new(&service, &user).and_then(|e| e.get_secret()))
            .map_err(|e| Error::TokenStore(format!("keyring thread: {e}")))?
            .join()
            .map_err(|_| Error::TokenStore("keyring call panicked".into()))?;
        match joined {
            Ok(bytes) if bytes.len() == 32 => {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                Ok(Some(key))
            }
            Ok(bytes) => Err(Error::TokenStore(format!(
                "keyring entry {}/{} holds {} bytes, expected 32; delete it to start over",
                self.service,
                self.user,
                bytes.len()
            ))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::TokenStore(format!(
                "keyring: {}",
                describe_keyring_error(e)
            ))),
        }
    }
    fn set(&self, key: &[u8; 32]) -> Result<()> {
        let key = *key;
        self.run(move |e| e.set_secret(&key))
    }
    fn delete(&self) -> Result<()> {
        match self.run(|e| e.delete_credential()) {
            Ok(()) => Ok(()),
            Err(Error::TokenStore(m))
                if m.contains("NoEntry") || m.contains("No matching entry") =>
            {
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
    fn describe(&self) -> String {
        format!("OS keyring ({}/{})", self.service, self.user)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeyStorage {
    #[default]
    Auto,
    Keyring,
    File,
}

impl std::fmt::Display for KeyStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            KeyStorage::Auto => "auto",
            KeyStorage::Keyring => "keyring",
            KeyStorage::File => "file",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyLocation {
    /// `STREAMBOAT_MASTER_KEY` in the environment.
    Environment,
    Keyring(String),
    File(PathBuf),
}

impl std::fmt::Display for KeyLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyLocation::Environment => write!(f, "STREAMBOAT_MASTER_KEY (environment)"),
            KeyLocation::Keyring(s) => write!(f, "{s}"),
            KeyLocation::File(p) => write!(f, "key file {}", p.display()),
        }
    }
}

fn parse_env_key(value: &str) -> Result<[u8; 32]> {
    let v = value.trim();
    let bytes = hex::decode(v)
        .ok()
        .or_else(|| base64::engine::general_purpose::STANDARD.decode(v).ok())
        .or_else(|| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(v)
                .ok()
        })
        .ok_or_else(|| {
            Error::TokenStore("STREAMBOAT_MASTER_KEY is neither hex nor base64".into())
        })?;
    if bytes.len() != 32 {
        return Err(Error::TokenStore(format!(
            "STREAMBOAT_MASTER_KEY decodes to {} bytes, expected 32",
            bytes.len()
        )));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

pub struct EncryptedFileStore {
    token_path: PathBuf,
    key_path: PathBuf,
    key: Zeroizing<[u8; 32]>,
    location: Mutex<KeyLocation>,
    slot: Arc<dyn KeySlot>,
}

impl std::fmt::Debug for EncryptedFileStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedFileStore")
            .field("token_path", &self.token_path)
            .field("key", &"<redacted>")
            .field("location", &self.key_location())
            .finish()
    }
}

impl EncryptedFileStore {
    /// A file-only store (no keyring); what tests and headless tools use.
    pub fn new(token_path: impl Into<PathBuf>, key_path: impl Into<PathBuf>) -> Result<Self> {
        Self::open(token_path, key_path, Arc::new(NoKeySlot), KeyStorage::File)
    }

    /// Resolve the master key now (keyring access happens here, once) and
    /// open the store.
    pub fn open(
        token_path: impl Into<PathBuf>,
        key_path: impl Into<PathBuf>,
        slot: Arc<dyn KeySlot>,
        storage: KeyStorage,
    ) -> Result<Self> {
        let env_key = std::env::var("STREAMBOAT_MASTER_KEY").ok();
        Self::open_with_env_key(token_path, key_path, slot, storage, env_key.as_deref())
    }

    pub fn open_with_env_key(
        token_path: impl Into<PathBuf>,
        key_path: impl Into<PathBuf>,
        slot: Arc<dyn KeySlot>,
        storage: KeyStorage,
        env_key: Option<&str>,
    ) -> Result<Self> {
        let token_path = token_path.into();
        let key_path = key_path.into();
        let (key, location) = resolve_key(&key_path, slot.as_ref(), storage, env_key)?;
        tracing::debug!(%location, "token master key resolved");
        Ok(Self {
            token_path,
            key_path,
            key,
            location: Mutex::new(location),
            slot,
        })
    }

    pub fn token_path(&self) -> &Path {
        &self.token_path
    }

    pub fn key_location(&self) -> KeyLocation {
        self.location.lock().unwrap().clone()
    }

    /// Move a file-held key into the keyring and delete the file. Explicit,
    /// user-initiated; never done silently.
    pub fn migrate_key_to_keyring(&self) -> Result<KeyLocation> {
        let mut loc = self.location.lock().unwrap();
        match &*loc {
            KeyLocation::Keyring(_) => return Ok(loc.clone()),
            KeyLocation::Environment => {
                return Err(Error::TokenStore(
                    "the key comes from STREAMBOAT_MASTER_KEY; unset it to use a keyring".into(),
                ));
            }
            KeyLocation::File(_) => {}
        }
        self.slot.set(&self.key)?;
        // Verify the round trip before deleting the file.
        match self.slot.get()? {
            Some(k) if k == *self.key => {}
            _ => {
                return Err(Error::TokenStore(
                    "keyring did not return the key it just stored".into(),
                ));
            }
        }
        if let Err(e) = std::fs::remove_file(&self.key_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.into());
            }
        }
        *loc = KeyLocation::Keyring(self.slot.describe());
        Ok(loc.clone())
    }

    /// Move a keyring-held key into the key file and delete the keyring entry.
    pub fn migrate_key_to_file(&self) -> Result<KeyLocation> {
        let mut loc = self.location.lock().unwrap();
        if let KeyLocation::Environment = &*loc {
            return Err(Error::TokenStore(
                "the key comes from STREAMBOAT_MASTER_KEY".into(),
            ));
        }
        fsutil::atomic_write(&self.key_path, self.key.as_ref(), 0o600)?;
        if let KeyLocation::Keyring(_) = &*loc {
            self.slot.delete()?;
        }
        *loc = KeyLocation::File(self.key_path.clone());
        Ok(loc.clone())
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(self.key.as_ref()))
    }
}

fn resolve_key(
    key_path: &Path,
    slot: &dyn KeySlot,
    storage: KeyStorage,
    env_key: Option<&str>,
) -> Result<(Zeroizing<[u8; 32]>, KeyLocation)> {
    if let Some(v) = env_key.filter(|v| !v.trim().is_empty()) {
        return Ok((Zeroizing::new(parse_env_key(v)?), KeyLocation::Environment));
    }
    // 1. An existing keyring entry.
    if storage != KeyStorage::File {
        match slot.get() {
            Ok(Some(k)) => return Ok((Zeroizing::new(k), KeyLocation::Keyring(slot.describe()))),
            Ok(None) => {}
            Err(e) if storage == KeyStorage::Keyring => return Err(e),
            Err(e) => tracing::warn!("keyring unavailable ({e}); using the key file"),
        }
    }
    // 2. An existing key file (never promoted to the keyring on its own).
    match std::fs::read(key_path) {
        Ok(bytes) => {
            if bytes.len() != 32 {
                return Err(Error::TokenStore(format!(
                    "{} is not a 32-byte key file; refusing to guess (delete it to log in again)",
                    key_path.display()
                )));
            }
            if storage == KeyStorage::Keyring {
                tracing::warn!(
                    "key_storage is 'keyring' but the key is in {}; run `streamboat keyring migrate`",
                    key_path.display()
                );
            }
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            return Ok((key, KeyLocation::File(key_path.to_path_buf())));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    // 3. First run: generate, prefer the keyring.
    let mut key = Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(key.as_mut());
    if storage != KeyStorage::File {
        match slot.set(&key) {
            Ok(()) => return Ok((key, KeyLocation::Keyring(slot.describe()))),
            Err(e) if storage == KeyStorage::Keyring => {
                return Err(Error::TokenStore(format!(
                    "key_storage is 'keyring' but no keyring is reachable: {e}"
                )));
            }
            Err(e) => tracing::warn!("keyring unavailable ({e}); storing the key in a file"),
        }
    }
    fsutil::atomic_write(key_path, key.as_ref(), 0o600)?;
    Ok((key, KeyLocation::File(key_path.to_path_buf())))
}

impl TokenStore for EncryptedFileStore {
    fn load(&self) -> Result<Option<TokenSet>> {
        let bytes = match std::fs::read(&self.token_path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if bytes.len() < HEADER_LEN || &bytes[..4] != MAGIC {
            return Err(Error::TokenStore(format!(
                "{} is not a streamboat token file or is corrupt (bad header); \
                 delete it and log in again",
                self.token_path.display()
            )));
        }
        if bytes[4] != VERSION {
            return Err(Error::TokenStore(format!(
                "{} was written by a newer streamboat (format v{}); refusing to downgrade",
                self.token_path.display(),
                bytes[4]
            )));
        }
        let nonce = Nonce::from_slice(&bytes[5..HEADER_LEN]);
        let plain = self
            .cipher()
            .decrypt(nonce, &bytes[HEADER_LEN..])
            .map_err(|_| {
                Error::TokenStore(format!(
                    "{} could not be decrypted with the key from {} (corrupt or wrong key); \
                 delete the token file and log in again",
                    self.token_path.display(),
                    self.key_location()
                ))
            })?;
        let plain = Zeroizing::new(plain);
        Ok(Some(serde_json::from_slice(&plain)?))
    }

    fn save(&self, tokens: &TokenSet) -> Result<()> {
        let plain = Zeroizing::new(serde_json::to_vec(tokens)?);
        let mut nonce_bytes = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher()
            .encrypt(nonce, plain.as_slice())
            .map_err(|_| Error::TokenStore("encryption failed".into()))?;
        let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        fsutil::atomic_write(&self.token_path, &out, 0o600)?;
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        match std::fs::remove_file(&self.token_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn lock_exclusive(&self) -> Result<Option<StoreLock>> {
        let lock_path = self.token_path.with_extension("lock");
        if let Some(dir) = lock_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        Ok(Some(StoreLock {
            inner: fd_lock::RwLock::new(file),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TokenSet {
        TokenSet {
            access_token: "at".into(),
            refresh_token: Some("rt".into()),
            token_type: "Bearer".into(),
            expires_at: 1_900_000_000,
            scope: "r_usr w_usr w_sub".into(),
            client_id: "cid".into(),
            flow: AuthFlow::Pkce,
            client_unique_key: Some("0123456789abcdef".into()),
            user_id: Some(42),
            country_code: Some("NL".into()),
        }
    }

    #[test]
    fn roundtrip_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            EncryptedFileStore::new(dir.path().join("tokens.bin"), dir.path().join("k")).unwrap();
        assert!(store.load().unwrap().is_none());
        store.save(&sample()).unwrap();
        assert_eq!(store.load().unwrap(), Some(sample()));
        let raw = std::fs::read(dir.path().join("tokens.bin")).unwrap();
        assert!(
            !raw.windows(2).any(|w| w == b"at"),
            "plaintext must not appear on disk"
        );
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
        assert!(matches!(store.key_location(), KeyLocation::File(_)));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("k"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn old_token_files_without_flow_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedFileStore::new(dir.path().join("t"), dir.path().join("k")).unwrap();
        let mut t = sample();
        t.flow = AuthFlow::DeviceCode;
        t.client_unique_key = None;
        store.save(&t).unwrap();
        let back = store.load().unwrap().unwrap();
        assert_eq!(back.flow, AuthFlow::DeviceCode);
        let v: serde_json::Value =
            serde_json::from_str(r#"{"access_token":"a","expires_at":1,"client_id":"c"}"#).unwrap();
        let legacy: TokenSet = serde_json::from_value(v).unwrap();
        assert_eq!(legacy.flow, AuthFlow::DeviceCode);
        assert_eq!(legacy.token_type, "Bearer");
    }

    #[test]
    fn truncation_at_every_offset_is_a_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tokens.bin");
        let store = EncryptedFileStore::new(&path, dir.path().join("k")).unwrap();
        store.save(&sample()).unwrap();
        let full = std::fs::read(&path).unwrap();
        for cut in 0..full.len() {
            std::fs::write(&path, &full[..cut]).unwrap();
            match store.load() {
                Err(Error::TokenStore(_)) => {}
                other => panic!("cut at {cut}: expected TokenStore error, got {other:?}"),
            }
        }
    }

    #[test]
    fn wrong_key_is_a_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tokens.bin");
        EncryptedFileStore::new(&path, dir.path().join("k1"))
            .unwrap()
            .save(&sample())
            .unwrap();
        let other = EncryptedFileStore::new(&path, dir.path().join("k2")).unwrap();
        assert!(matches!(other.load(), Err(Error::TokenStore(_))));
    }

    #[test]
    fn first_run_prefers_keyring_and_writes_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let slot = Arc::new(MemoryKeySlot::default());
        let store = EncryptedFileStore::open(
            dir.path().join("t"),
            dir.path().join("k"),
            slot.clone(),
            KeyStorage::Auto,
        )
        .unwrap();
        assert!(matches!(store.key_location(), KeyLocation::Keyring(_)));
        assert!(
            !dir.path().join("k").exists(),
            "no key file when the keyring works"
        );
        store.save(&sample()).unwrap();
        // A second open reads the same key back from the keyring.
        let again = EncryptedFileStore::open(
            dir.path().join("t"),
            dir.path().join("k"),
            slot,
            KeyStorage::Auto,
        )
        .unwrap();
        assert_eq!(again.load().unwrap(), Some(sample()));
    }

    #[test]
    fn unreachable_keyring_falls_back_to_file_in_auto_and_fails_in_keyring_mode() {
        let dir = tempfile::tempdir().unwrap();
        let slot = Arc::new(MemoryKeySlot::default());
        *slot.fail.lock().unwrap() = true;
        let store = EncryptedFileStore::open(
            dir.path().join("t"),
            dir.path().join("k"),
            slot.clone(),
            KeyStorage::Auto,
        )
        .unwrap();
        assert!(matches!(store.key_location(), KeyLocation::File(_)));
        let err = EncryptedFileStore::open(
            dir.path().join("t2"),
            dir.path().join("k2"),
            slot,
            KeyStorage::Keyring,
        )
        .unwrap_err();
        assert!(matches!(err, Error::TokenStore(_)), "{err}");
    }

    #[test]
    fn file_key_is_never_promoted_silently_but_migrates_on_request() {
        let dir = tempfile::tempdir().unwrap();
        let slot = Arc::new(MemoryKeySlot::default());
        // First run with the keyring down: key goes to a file.
        *slot.fail.lock().unwrap() = true;
        let s1 = EncryptedFileStore::open(
            dir.path().join("t"),
            dir.path().join("k"),
            slot.clone(),
            KeyStorage::Auto,
        )
        .unwrap();
        s1.save(&sample()).unwrap();
        drop(s1);
        // Keyring back: the file key must still be used, and not copied.
        *slot.fail.lock().unwrap() = false;
        let s2 = EncryptedFileStore::open(
            dir.path().join("t"),
            dir.path().join("k"),
            slot.clone(),
            KeyStorage::Auto,
        )
        .unwrap();
        assert!(matches!(s2.key_location(), KeyLocation::File(_)));
        assert!(slot.get().unwrap().is_none(), "no silent promotion");
        assert_eq!(s2.load().unwrap(), Some(sample()));
        // Explicit migration moves it and removes the file.
        let loc = s2.migrate_key_to_keyring().unwrap();
        assert!(matches!(loc, KeyLocation::Keyring(_)));
        assert!(!dir.path().join("k").exists());
        assert_eq!(s2.load().unwrap(), Some(sample()));
        let s3 = EncryptedFileStore::open(
            dir.path().join("t"),
            dir.path().join("k"),
            slot.clone(),
            KeyStorage::Auto,
        )
        .unwrap();
        assert_eq!(s3.load().unwrap(), Some(sample()));
        // And back to a file.
        let loc = s3.migrate_key_to_file().unwrap();
        assert!(matches!(loc, KeyLocation::File(_)));
        assert!(slot.get().unwrap().is_none());
        assert_eq!(s3.load().unwrap(), Some(sample()));
    }

    #[test]
    fn env_key_wins_and_must_be_32_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let slot = Arc::new(MemoryKeySlot::default());
        let hex_key = "00".repeat(32);
        let s = EncryptedFileStore::open_with_env_key(
            dir.path().join("t"),
            dir.path().join("k"),
            slot.clone(),
            KeyStorage::Auto,
            Some(&hex_key),
        )
        .unwrap();
        assert_eq!(s.key_location(), KeyLocation::Environment);
        assert!(slot.get().unwrap().is_none());
        assert!(!dir.path().join("k").exists());
        let err = EncryptedFileStore::open_with_env_key(
            dir.path().join("t"),
            dir.path().join("k"),
            slot,
            KeyStorage::Auto,
            Some("abcd"),
        )
        .unwrap_err();
        assert!(matches!(err, Error::TokenStore(_)));
    }

    #[test]
    fn exclusive_lock_serialises_two_stores_on_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = EncryptedFileStore::new(dir.path().join("t"), dir.path().join("k")).unwrap();
        let b = EncryptedFileStore::new(dir.path().join("t"), dir.path().join("k")).unwrap();
        let mut la = a.lock_exclusive().unwrap().unwrap();
        let ga = la.write().unwrap();
        let mut lb = b.lock_exclusive().unwrap().unwrap();
        assert!(
            lb.inner.try_write().is_err(),
            "second lock must not be granted while the first is held"
        );
        drop(ga);
        assert!(lb.inner.try_write().is_ok());
    }
}
