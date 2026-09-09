//! Token persistence (D-026). This spike ships the encrypted-file tier:
//! AES-256-GCM with a random 32-byte key kept in a 0600 file, written
//! atomically. The OS-keyring tier that should hold that key comes with the
//! desktop shell. Format (after Sone's `crypto.rs`, corrected per the
//! engineering baseline): `MAGIC "SBTK" || VERSION(1) || NONCE(12) || CIPHERTEXT+TAG`.
//! A bad header is a typed error, never a silent passthrough.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{Error, Result};
use crate::fsutil;

const MAGIC: &[u8; 4] = b"SBTK";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 4 + 1 + 12;

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
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "<redacted>"))
            .field("expires_at", &self.expires_at)
            .field("client_id", &self.client_id)
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

pub trait TokenStore: Send + Sync {
    fn load(&self) -> Result<Option<TokenSet>>;
    fn save(&self, tokens: &TokenSet) -> Result<()>;
    fn clear(&self) -> Result<()>;
}

/// In-memory store for tests and one-off tools.
#[derive(Default)]
pub struct MemoryTokenStore {
    inner: Mutex<Option<TokenSet>>,
}

impl MemoryTokenStore {
    pub fn with(tokens: TokenSet) -> Self {
        Self { inner: Mutex::new(Some(tokens)) }
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

pub struct EncryptedFileStore {
    token_path: PathBuf,
    key_path: PathBuf,
}

impl EncryptedFileStore {
    pub fn new(token_path: impl Into<PathBuf>, key_path: impl Into<PathBuf>) -> Self {
        Self { token_path: token_path.into(), key_path: key_path.into() }
    }

    pub fn token_path(&self) -> &Path {
        &self.token_path
    }

    fn load_or_create_key(&self) -> Result<Zeroizing<[u8; 32]>> {
        match std::fs::read(&self.key_path) {
            Ok(bytes) => {
                if bytes.len() != 32 {
                    return Err(Error::TokenStore(format!(
                        "{} is not a 32-byte key file; refusing to guess (delete it to log in again)",
                        self.key_path.display()
                    )));
                }
                let mut key = Zeroizing::new([0u8; 32]);
                key.copy_from_slice(&bytes);
                Ok(key)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut key = Zeroizing::new([0u8; 32]);
                rand::rng().fill_bytes(key.as_mut());
                fsutil::atomic_write(&self.key_path, key.as_ref(), 0o600)?;
                Ok(key)
            }
            Err(e) => Err(e.into()),
        }
    }

    fn cipher(&self) -> Result<Aes256Gcm> {
        let key = self.load_or_create_key()?;
        Ok(Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_ref())))
    }
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
            .cipher()?
            .decrypt(nonce, &bytes[HEADER_LEN..])
            .map_err(|_| {
                Error::TokenStore(format!(
                    "{} could not be decrypted with {} (corrupt or wrong key); \
                     delete both and log in again",
                    self.token_path.display(),
                    self.key_path.display()
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
            .cipher()?
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
            user_id: Some(42),
            country_code: Some("NL".into()),
        }
    }

    #[test]
    fn roundtrip_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedFileStore::new(dir.path().join("tokens.bin"), dir.path().join("k"));
        assert!(store.load().unwrap().is_none());
        store.save(&sample()).unwrap();
        assert_eq!(store.load().unwrap(), Some(sample()));
        let raw = std::fs::read(dir.path().join("tokens.bin")).unwrap();
        assert!(!raw.windows(2).any(|w| w == b"at"), "plaintext must not appear on disk");
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("k")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn truncation_at_every_offset_is_a_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tokens.bin");
        let store = EncryptedFileStore::new(&path, dir.path().join("k"));
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
        EncryptedFileStore::new(&path, dir.path().join("k1")).save(&sample()).unwrap();
        let other = EncryptedFileStore::new(&path, dir.path().join("k2"));
        assert!(matches!(other.load(), Err(Error::TokenStore(_))));
    }
}
