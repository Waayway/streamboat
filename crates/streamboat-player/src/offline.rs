//! The pinned, encrypted offline cache (D-022) — the sharpest guardrail in
//! the project. Every rule below is mandatory, not a default to loosen:
//!
//! - **Explicit, user-initiated pins only.** Nothing in this module is ever
//!   called except from [`Command::Pin`]/[`Command::Unpin`] (or the CLI's
//!   direct equivalent); streamboat never pins on its own. Artwork and
//!   metadata caching are out of scope here entirely.
//! - **A transparent cache of the ordinary stream.** [`OfflineCache::pin`]
//!   resolves through [`streamboat_core::ApiClient::resolve_stream`] exactly
//!   as playback does (`playbackmode=STREAM`, `assetpresentation=FULL`) —
//!   nothing here ever requests `playbackmode=OFFLINE` or `usage=DOWNLOAD`,
//!   which are licensed, DRM-bound flows, not caching.
//! - **Never a preview, never an encrypted manifest.** A `PREVIEW` asset is
//!   refused before a byte is written (core does not refuse these for
//!   ordinary playback, since a preview is still something to *play*; it is
//!   not something to *pin*). An encrypted manifest never reaches this
//!   module at all: [`streamboat_core::manifest::parse`], reused unchanged
//!   inside `resolve_stream`, already refuses those (`Error::ManifestRefused`)
//!   before returning — see `streamboat-core/src/manifest.rs`.
//! - **Opaque, encrypted chunks; no export path.** Every stored byte is
//!   AES-256-GCM ciphertext under a device-bound key (see "Key derivation"
//!   below); there is no "open folder," "export," "share," or
//!   decrypt-to-file method anywhere in this module or in the CLI. Copying
//!   the `offline/` directory to another install yields unreadable chunks:
//!   the file key is derived from *this install's* keyring/file secret
//!   folded together with *this install's* `client_unique_key`, and a fresh
//!   install has neither.
//! - **Revalidated, and wiped on logout/lapse/unpin.** See
//!   [`OfflineCache::serve_track`] (revalidation) and [`OfflineCache::wipe_dir`]
//!   / [`OfflineCache::wipe_all`] / [`OfflineCache::unpin`] (wipes).
//!
//! # On-disk format
//!
//! `<offline_dir>/index.json` (atomic writes via
//! [`streamboat_core::fsutil::atomic_write`]) is a [`Index`] of pins, each
//! recording its kind/id, member track ids, and per-track metadata (quality
//! actually stored, manifest hash, container mime, byte length, `stored_at`,
//! `validated_at`). `<offline_dir>/chunks/` holds one file per 1 MiB
//! ciphertext chunk, named by hash and carrying no extension — see
//! [`chunk_name`]. A chunk file's byte layout is exactly
//! `AES-256-GCM(plaintext) = ciphertext || 16-byte tag`, with no header: the
//! chunk's index and its track's random salt (stored in the index) are
//! enough to recompute the nonce and to know where to look for it.
//!
//! # Key derivation
//!
//! A 32-byte "install secret" lives in the OS keyring under its own entry
//! (`streamboat_core::bootstrap::OFFLINE_KEYRING_USER`, resolved through the
//! same [`KeySlot`] mechanism the token store uses, with an encrypted...
//! actually a plain 0600 key-file fallback exactly as D-024 describes for
//! the token store, deliberately a *separate* file and keyring entry from
//! the token master key). The key actually used to encrypt/decrypt chunks
//! is **not** that secret directly: it is
//! `HKDF-SHA256(ikm = install secret, info = client_unique_key)`, which
//! ties every chunk to this specific device identity as well as this
//! specific install's secret. Delete `device.json` (or restore an install
//! on new hardware without it) and the offline cache becomes permanently
//! unreadable — by design, matching "device-bound."
//!
//! # Playback without a playable file on disk
//!
//! [`OfflineCache::open`] starts one loopback HTTP server (`axum`) bound to
//! `127.0.0.1:0` (an OS-assigned ephemeral port) for the life of the
//! process. It serves `GET /<token>/<track_id>`, decrypting the requested
//! chunks on the fly and supporting `Range` requests; `<token>` is a random
//! per-process value, and any request from a non-loopback peer or with the
//! wrong token is refused. `Player` hands the engine
//! `StreamSource::Url("http://127.0.0.1:<port>/<token>/<track_id>")` for a
//! pinned, valid track instead of ever writing a decrypted file to disk.
//!
//! # Known limitations (see the task report / architecture doc for the full list)
//!
//! - DASH segment planning only understands a `SegmentTemplate` with a
//!   `$Number$` media identifier and a closed (non-open-ended)
//!   `SegmentTimeline` — the same limitation `audio-pipeline`'s research
//!   documents against every reference DASH parser (no `$Time$`,
//!   `$Bandwidth$`, `$RepresentationID$`, or printf-width identifiers).
//!   Refused loudly with a clear error rather than mis-parsed.
//! - A track pinned twice (once alone, once as part of an album/playlist
//!   also containing it) is stored twice; there is no cross-pin
//!   deduplication.
//! - The whole resolved stream is buffered in memory before being split
//!   into chunks, and a full requested `Range` is decrypted into memory
//!   before being returned — fine at the track sizes this format deals in,
//!   not a true bounded-memory streaming pipeline.

use std::collections::{BTreeMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use axum::extract::{ConnectInfo, Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;

use streamboat_core::api::pagination;
use streamboat_core::config::{AppDirs, Settings};
use streamboat_core::proto::{PinKind, StreamInfo};
use streamboat_core::token_store::{KeySlot, KeyStorage};
use streamboat_core::{ApiClient, AudioQuality, ResolvedStream, StreamSource, fsutil};

/// One AES-256-GCM chunk's plaintext size, except the last chunk of a
/// track, which may be shorter.
pub const CHUNK_SIZE: u32 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum OfflineError {
    #[error("TIDAL: {0}")]
    Api(#[from] streamboat_core::Error),
    #[error("refused to pin: {0}")]
    Refused(String),
    #[error("offline cache: {0}")]
    Cache(String),
    #[error("offline cache crypto: {0}")]
    Crypto(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type OfflineResult<T> = std::result::Result<T, OfflineError>;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn pin_key(kind: PinKind, id: &str) -> String {
    format!("{}:{}", kind.as_str(), id)
}

// ---------------------------------------------------------------------------
// On-disk index

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    pins: BTreeMap<String, PinEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PinEntry {
    kind: PinKind,
    id: String,
    #[serde(default)]
    title: String,
    track_ids: Vec<u64>,
    tracks: BTreeMap<u64, TrackEntry>,
    stored_at: u64,
    validated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrackEntry {
    quality: AudioQuality,
    manifest_hash: Option<String>,
    /// The stored bytes' container/mime, for the loopback route's
    /// `Content-Type` (e.g. the CDN's own `Content-Type` for a BTS/EMU
    /// asset, or `audio/mp4` for a concatenated DASH init+media stream).
    mime: String,
    codec: Option<String>,
    sample_rate: Option<u32>,
    bit_depth: Option<u32>,
    byte_length: u64,
    chunk_size: u32,
    /// Hex-encoded 8-byte per-track random salt; folded into every chunk's
    /// AES-GCM nonce alongside the chunk index (`chunk_nonce`).
    salt_hex: String,
    replay_gain_db: Option<f64>,
    peak_amplitude: Option<f64>,
    /// Album ReplayGain/peak, when TIDAL reported one (D-019); `None` on a
    /// pin written before this field existed, which `Player`'s album-mode
    /// selection already falls back on to the track values above.
    #[serde(default)]
    album_replay_gain_db: Option<f64>,
    #[serde(default)]
    album_peak_amplitude: Option<f64>,
}

impl TrackEntry {
    fn chunk_count(&self) -> u32 {
        self.byte_length.div_ceil(u64::from(self.chunk_size)).max(1) as u32
    }
}

fn load_index(path: &Path) -> OfflineResult<Index> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Index::default()),
        Err(e) => Err(e.into()),
    }
}

// ---------------------------------------------------------------------------
// A resolved, playable, cached track handed back to the Player.

/// What [`OfflineCache::serve_track`] hands back: enough for `Player` to
/// build a `LoadItem` exactly as it would from a live [`ResolvedStream`].
pub struct ServedTrack {
    pub url: String,
    pub quality: AudioQuality,
    pub manifest_hash: Option<String>,
    pub codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub replay_gain_db: Option<f64>,
    pub peak_amplitude: Option<f64>,
    pub album_replay_gain_db: Option<f64>,
    pub album_peak_amplitude: Option<f64>,
}

impl ServedTrack {
    /// Wraps this cached track as a [`ResolvedStream`] so `Player` can treat
    /// a cache hit identically to a fresh `resolve_stream` call.
    pub fn into_resolved_stream(self, track_id: u64) -> ResolvedStream {
        ResolvedStream {
            track_id,
            requested: self.quality,
            source: StreamSource::Url(self.url),
            info: StreamInfo {
                quality: Some(self.quality),
                audio_mode: None,
                manifest_kind: "offline-cache".to_string(),
                codec: self.codec,
                sample_rate: self.sample_rate,
                bit_depth: self.bit_depth,
                replay_gain_db: self.replay_gain_db,
                peak_amplitude: self.peak_amplitude,
                album_replay_gain_db: self.album_replay_gain_db,
                album_peak_amplitude: self.album_peak_amplitude,
                preview: false,
            },
            manifest_hash: self.manifest_hash,
            warnings: Vec::new(),
        }
    }
}

/// A pin as shown to a user (`streamboat pins`, a future control-API list).
#[derive(Debug, Clone)]
pub struct PinInfo {
    pub kind: PinKind,
    pub id: String,
    pub title: String,
    pub track_count: usize,
    pub bytes: u64,
    pub stored_at: u64,
    pub validated_at: u64,
    pub valid: bool,
}

// ---------------------------------------------------------------------------
// Crypto

fn chunk_nonce(salt: &[u8; 8], idx: u32) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[..8].copy_from_slice(salt);
    n[8..].copy_from_slice(&idx.to_be_bytes());
    n
}

/// The chunk's filename: a hash of its (salt, index) pair, not of its
/// content — deterministic without needing to store a file list, and
/// carries no information about the plaintext.
fn chunk_name(salt: &[u8; 8], idx: u32) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(idx.to_be_bytes());
    hex::encode(hasher.finalize())
}

fn decode_salt(hex_str: &str) -> OfflineResult<[u8; 8]> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| OfflineError::Cache(format!("corrupt salt in offline index: {e}")))?;
    if bytes.len() != 8 {
        return Err(OfflineError::Cache(
            "corrupt salt in offline index: wrong length".into(),
        ));
    }
    let mut salt = [0u8; 8];
    salt.copy_from_slice(&bytes);
    Ok(salt)
}

fn encrypt_chunk(
    key: &[u8; 32],
    salt: &[u8; 8],
    idx: u32,
    plaintext: &[u8],
) -> OfflineResult<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = chunk_nonce(salt, idx);
    cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|_| OfflineError::Crypto("chunk encryption failed".into()))
}

fn decrypt_chunk(
    key: &[u8; 32],
    salt: &[u8; 8],
    idx: u32,
    ciphertext: &[u8],
) -> OfflineResult<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = chunk_nonce(salt, idx);
    cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext)
        .map_err(|_| {
            OfflineError::Crypto(
                "chunk failed to decrypt (wrong device key, or the chunk is corrupt)".into(),
            )
        })
}

fn derive_file_key(install_secret: &[u8; 32], client_unique_key: &str) -> [u8; 32] {
    let hk = hkdf::Hkdf::<Sha256>::new(None, install_secret);
    let mut okm = [0u8; 32];
    hk.expand(client_unique_key.as_bytes(), &mut okm)
        .expect("32 is a valid HKDF-SHA256 output length");
    okm
}

/// Resolves the offline cache's own 32-byte install secret: an existing
/// keyring entry, else an existing 0600 key file, else generate and store
/// (keyring preferred) — the same shape as the token store's master-key
/// resolution (D-024/D-026), deliberately duplicated rather than reusing
/// `token_store`'s private helper, and deliberately a separate keyring
/// entry/file from the token master key (D-022).
fn resolve_install_secret(
    key_path: &Path,
    slot: &dyn KeySlot,
    storage: KeyStorage,
) -> OfflineResult<[u8; 32]> {
    if storage != KeyStorage::File {
        match slot.get() {
            Ok(Some(k)) => return Ok(k),
            Ok(None) => {}
            Err(e) if storage == KeyStorage::Keyring => {
                return Err(OfflineError::Cache(format!(
                    "offline cache key_storage is 'keyring' but no keyring is reachable: {e}"
                )));
            }
            Err(e) => {
                tracing::warn!(
                    "keyring unavailable for the offline cache key ({e}); using a key file"
                );
            }
        }
    }
    match std::fs::read(key_path) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
        Ok(bytes) => {
            return Err(OfflineError::Cache(format!(
                "{} is not a 32-byte key file ({} bytes); delete it to reset the offline cache \
                 (every existing pin becomes unreadable)",
                key_path.display(),
                bytes.len()
            )));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let mut key = [0u8; 32];
    rand::rng().fill_bytes(&mut key);
    if storage != KeyStorage::File {
        match slot.set(&key) {
            Ok(()) => return Ok(key),
            Err(e) if storage == KeyStorage::Keyring => {
                return Err(OfflineError::Cache(format!(
                    "offline cache key_storage is 'keyring' but no keyring is reachable: {e}"
                )));
            }
            Err(e) => {
                tracing::warn!(
                    "keyring unavailable for the offline cache key ({e}); storing it in a file"
                );
            }
        }
    }
    fsutil::atomic_write(key_path, &key, 0o600)?;
    Ok(key)
}

// ---------------------------------------------------------------------------
// DASH: init segment + every media segment, in order, concatenated.

struct DashPlan {
    init_url: String,
    media_urls: Vec<String>,
}

/// Turns a DASH MPD into an ordered list of byte-range-free segment URLs:
/// the init segment followed by every media segment. Deliberately narrow —
/// see the module doc's "Known limitations" — and refuses loudly rather
/// than guessing when the manifest uses a shape this does not understand.
fn plan_dash_segments(xml: &str) -> OfflineResult<DashPlan> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| OfflineError::Cache(format!("DASH manifest is not well-formed XML: {e}")))?;
    let root = doc.root_element();
    let representation = root
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "Representation")
        .ok_or_else(|| OfflineError::Cache("DASH manifest has no <Representation>".into()))?;
    let template = representation
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "SegmentTemplate")
        .ok_or_else(|| {
            OfflineError::Cache(
                "offline pinning only supports a DASH <SegmentTemplate> Representation".into(),
            )
        })?;
    let init_url = template
        .attribute("initialization")
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| OfflineError::Cache("<SegmentTemplate> has no initialization URL".into()))?
        .to_string();
    let media_template = template
        .attribute("media")
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| OfflineError::Cache("<SegmentTemplate> has no media URL template".into()))?;
    if !media_template.contains("$Number$") || media_template.contains("$Time$") {
        return Err(OfflineError::Cache(
            "offline pinning only supports a plain $Number$ media template (no $Time$, \
             $Bandwidth$, $RepresentationID$ or printf-width identifiers)"
                .into(),
        ));
    }
    let start_number: u64 = template
        .attribute("startNumber")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let timeline = template
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "SegmentTimeline")
        .ok_or_else(|| {
            OfflineError::Cache(
                "offline pinning requires a <SegmentTimeline> (a duration-only \
                 <SegmentTemplate> is not supported)"
                    .into(),
            )
        })?;
    let mut count: u64 = 0;
    for s in timeline
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "S")
    {
        let r: i64 = s.attribute("r").and_then(|v| v.parse().ok()).unwrap_or(0);
        if r < 0 {
            return Err(OfflineError::Cache(
                "offline pinning does not support an open-ended <SegmentTimeline> (r=\"-1\")"
                    .into(),
            ));
        }
        count += 1 + r as u64;
    }
    if count == 0 {
        return Err(OfflineError::Cache(
            "DASH <SegmentTimeline> describes zero segments".into(),
        ));
    }
    let media_urls = (start_number..start_number + count)
        .map(|n| media_template.replacen("$Number$", &n.to_string(), 1))
        .collect();
    Ok(DashPlan {
        init_url,
        media_urls,
    })
}

async fn fetch_track_bytes(
    http: &reqwest::Client,
    source: &StreamSource,
) -> OfflineResult<(Vec<u8>, String)> {
    match source {
        StreamSource::Url(url) => {
            let resp = http
                .get(url)
                .send()
                .await
                .map_err(|e| OfflineError::Cache(format!("fetching the stream: {e}")))?;
            if !resp.status().is_success() {
                return Err(OfflineError::Cache(format!(
                    "the CDN returned HTTP {} for the stream",
                    resp.status()
                )));
            }
            let mime = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| OfflineError::Cache(format!("reading the stream body: {e}")))?;
            Ok((bytes.to_vec(), mime))
        }
        StreamSource::DashMpd(xml) => {
            let plan = plan_dash_segments(xml)?;
            let mut out = Vec::new();
            for (i, url) in std::iter::once(&plan.init_url)
                .chain(plan.media_urls.iter())
                .enumerate()
            {
                let resp =
                    http.get(url).send().await.map_err(|e| {
                        OfflineError::Cache(format!("fetching DASH segment {i}: {e}"))
                    })?;
                if !resp.status().is_success() {
                    return Err(OfflineError::Cache(format!(
                        "the CDN returned HTTP {} for DASH segment {i}",
                        resp.status()
                    )));
                }
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| OfflineError::Cache(format!("reading DASH segment {i}: {e}")))?;
                out.extend_from_slice(&bytes);
            }
            Ok((out, "audio/mp4".to_string()))
        }
    }
}

// ---------------------------------------------------------------------------
// Expanding a pin into its member track ids.

async fn expand_pin(api: &ApiClient, kind: PinKind, id: &str) -> OfflineResult<(String, Vec<u64>)> {
    match kind {
        PinKind::Track => {
            let track_id: u64 = id
                .parse()
                .map_err(|_| OfflineError::Cache(format!("{id:?} is not a numeric track id")))?;
            Ok((format!("track {track_id}"), vec![track_id]))
        }
        PinKind::Album => {
            let album_id: u64 = id
                .parse()
                .map_err(|_| OfflineError::Cache(format!("{id:?} is not a numeric album id")))?;
            let album = api.album(album_id).await?;
            let tracks =
                pagination::collect_all(pagination::DEFAULT_PAGE_SIZE, 10_000, |offset, limit| {
                    api.album_tracks(album_id, limit, offset)
                })
                .await?;
            let title = if album.title.is_empty() {
                format!("album {album_id}")
            } else {
                album.title
            };
            Ok((title, tracks.into_iter().map(|t| t.id).collect()))
        }
        PinKind::Playlist => {
            let playlist = api.playlist(id).await?;
            let tracks =
                pagination::collect_all(pagination::DEFAULT_PAGE_SIZE, 10_000, |offset, limit| {
                    api.playlist_tracks(id, limit, offset)
                })
                .await?;
            let title = if playlist.title.is_empty() {
                format!("playlist {id}")
            } else {
                playlist.title
            };
            Ok((title, tracks.into_iter().map(|t| t.id).collect()))
        }
    }
}

// ---------------------------------------------------------------------------
// OfflineCache

pub struct OfflineCache {
    #[allow(dead_code)] // kept for `Debug`/diagnostics symmetry with dir layout
    dir: PathBuf,
    chunks_dir: PathBuf,
    index_path: PathBuf,
    file_key: [u8; 32],
    max_bytes: Option<u64>,
    validity: Duration,
    index: Mutex<Index>,
    token: String,
    port: u16,
    http: reqwest::Client,
}

impl OfflineCache {
    /// Resolves the cache directory, the device-bound file key (via the OS
    /// keyring/file fallback, D-022/D-024), garbage collects chunks whose
    /// index entry is gone, and starts the loopback playback server. Call
    /// once per process; hand the returned `Arc` around
    /// (`PlayerDeps::offline`, the CLI's pin/unpin/pins commands).
    pub async fn open(
        dirs: &AppDirs,
        settings: &Settings,
        client_unique_key: &str,
    ) -> OfflineResult<Arc<Self>> {
        let slot = streamboat_core::bootstrap::key_slot_named(
            settings.key_storage,
            streamboat_core::bootstrap::OFFLINE_KEYRING_USER,
        );
        Self::open_with_key_slot(dirs, settings, client_unique_key, slot).await
    }

    /// Like [`Self::open`] with an explicit [`KeySlot`] instead of the real
    /// OS keyring — how a test supplies a `MemoryKeySlot` instead of
    /// touching a real keyring, and otherwise identical to `open`.
    pub async fn open_with_key_slot(
        dirs: &AppDirs,
        settings: &Settings,
        client_unique_key: &str,
        slot: Arc<dyn KeySlot>,
    ) -> OfflineResult<Arc<Self>> {
        let dir = settings
            .offline_dir
            .clone()
            .unwrap_or_else(|| dirs.offline_dir());
        let chunks_dir = dir.join("chunks");
        fsutil::ensure_private_dir(&dir)?;
        fsutil::ensure_private_dir(&chunks_dir)?;
        let index_path = dir.join("index.json");
        let index = load_index(&index_path)?;
        gc_orphans(&chunks_dir, &index)?;

        let key_path = dir.join("offline.key");
        let install_secret =
            resolve_install_secret(&key_path, slot.as_ref(), settings.key_storage)?;
        let file_key = derive_file_key(&install_secret, client_unique_key);

        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let mut token_bytes = [0u8; 16];
        rand::rng().fill_bytes(&mut token_bytes);
        let token = hex::encode(token_bytes);

        let cache = Arc::new(Self {
            dir,
            chunks_dir,
            index_path,
            file_key,
            max_bytes: settings.offline_max_bytes,
            validity: Duration::from_secs(u64::from(settings.offline_validity_days) * 86_400),
            index: Mutex::new(index),
            token,
            port,
            http: reqwest::Client::new(),
        });
        spawn_loopback(listener, cache.clone());
        Ok(cache)
    }

    pub fn loopback_port(&self) -> u16 {
        self.port
    }

    fn persist(&self, index: &Index) -> OfflineResult<()> {
        let json = serde_json::to_vec_pretty(index)?;
        fsutil::atomic_write(&self.index_path, &json, 0o600)?;
        Ok(())
    }

    fn total_bytes_locked(index: &Index) -> u64 {
        index
            .pins
            .values()
            .flat_map(|p| p.tracks.values())
            .map(|t| t.byte_length)
            .sum()
    }

    fn cleanup_chunks(&self, tracks: &BTreeMap<u64, TrackEntry>) {
        for entry in tracks.values() {
            let Ok(salt) = decode_salt(&entry.salt_hex) else {
                continue;
            };
            for idx in 0..entry.chunk_count() {
                let _ = std::fs::remove_file(self.chunks_dir.join(chunk_name(&salt, idx)));
            }
        }
    }

    /// Pin `kind`/`id`: expand to member tracks, resolve and download each
    /// exactly as playback would (up to `ceiling`), refuse a preview or an
    /// encrypted manifest, encrypt and store the result, and record it in
    /// the index. Nothing partial is left behind on failure: chunks written
    /// for tracks already completed in this attempt are deleted before the
    /// error is returned. `on_progress(completed, total)` is called after
    /// each member track finishes.
    pub async fn pin(
        &self,
        api: &ApiClient,
        kind: PinKind,
        id: String,
        ceiling: AudioQuality,
        mut on_progress: impl FnMut(u32, u32),
    ) -> OfflineResult<()> {
        let (title, track_ids) = expand_pin(api, kind, &id).await?;
        if track_ids.is_empty() {
            return Err(OfflineError::Cache(
                "nothing to pin: no tracks found".into(),
            ));
        }
        let total = track_ids.len() as u32;
        let mut tracks = BTreeMap::new();
        let mut running_total = 0u64;
        for (i, track_id) in track_ids.iter().enumerate() {
            match self
                .download_track(api, *track_id, ceiling, &mut running_total)
                .await
            {
                Ok(entry) => {
                    tracks.insert(*track_id, entry);
                    on_progress(i as u32 + 1, total);
                }
                Err(e) => {
                    self.cleanup_chunks(&tracks);
                    return Err(e);
                }
            }
        }
        let now = now_secs();
        let key = pin_key(kind, &id);
        let mut idx = self.index.lock().unwrap();
        idx.pins.insert(
            key,
            PinEntry {
                kind,
                id,
                title,
                track_ids,
                tracks,
                stored_at: now,
                validated_at: now,
            },
        );
        self.persist(&idx)
    }

    async fn download_track(
        &self,
        api: &ApiClient,
        track_id: u64,
        ceiling: AudioQuality,
        running_total: &mut u64,
    ) -> OfflineResult<TrackEntry> {
        let session_id = uuid::Uuid::new_v4().to_string();
        // Exactly as playback resolves: STREAM/FULL, the quality cascade,
        // and the manifest refusal rule (encrypted manifests never reach
        // here — `resolve_stream` already refused them internally).
        let resolved = api.resolve_stream(track_id, ceiling, &session_id).await?;
        if resolved.info.preview {
            return Err(OfflineError::Refused(format!(
                "TIDAL served track {track_id} as a PREVIEW; offline pinning refuses to cache \
                 previews (D-022)"
            )));
        }
        let (bytes, mime) = fetch_track_bytes(&self.http, &resolved.source).await?;
        let len = bytes.len() as u64;
        if let Some(max) = self.max_bytes {
            let current = {
                let idx = self.index.lock().unwrap();
                Self::total_bytes_locked(&idx)
            };
            let projected = current + *running_total + len;
            if projected > max {
                return Err(OfflineError::Cache(format!(
                    "pinning track {track_id} would bring the offline cache to {projected} bytes, \
                     over the {max} byte cap (offline_max_bytes); unpin something first or raise the cap"
                )));
            }
        }
        *running_total += len;

        let mut salt = [0u8; 8];
        rand::rng().fill_bytes(&mut salt);
        for (idx, start) in (0..bytes.len()).step_by(CHUNK_SIZE as usize).enumerate() {
            let idx = idx as u32;
            let end = (start + CHUNK_SIZE as usize).min(bytes.len());
            let ciphertext = encrypt_chunk(&self.file_key, &salt, idx, &bytes[start..end])?;
            let path = self.chunks_dir.join(chunk_name(&salt, idx));
            fsutil::atomic_write(&path, &ciphertext, 0o600)?;
        }

        Ok(TrackEntry {
            quality: resolved.info.quality.unwrap_or(resolved.requested),
            manifest_hash: resolved.manifest_hash,
            mime,
            codec: resolved.info.codec,
            sample_rate: resolved.info.sample_rate,
            bit_depth: resolved.info.bit_depth,
            byte_length: len,
            chunk_size: CHUNK_SIZE,
            salt_hex: hex::encode(salt),
            replay_gain_db: resolved.info.replay_gain_db,
            peak_amplitude: resolved.info.peak_amplitude,
            album_replay_gain_db: resolved.info.album_replay_gain_db,
            album_peak_amplitude: resolved.info.album_peak_amplitude,
        })
    }

    /// Remove a pin and delete its chunks. `Ok(false)` when it was not
    /// pinned (not an error: unpinning something already gone is a no-op).
    pub fn unpin(&self, kind: PinKind, id: &str) -> OfflineResult<bool> {
        let key = pin_key(kind, id);
        let mut idx = self.index.lock().unwrap();
        match idx.pins.remove(&key) {
            Some(entry) => {
                self.cleanup_chunks(&entry.tracks);
                self.persist(&idx)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Every pin, for display (`streamboat pins`).
    pub fn list_pins(&self) -> Vec<PinInfo> {
        let idx = self.index.lock().unwrap();
        let now = now_secs();
        idx.pins
            .values()
            .map(|p| PinInfo {
                kind: p.kind,
                id: p.id.clone(),
                title: p.title.clone(),
                track_count: p.track_ids.len(),
                bytes: p.tracks.values().map(|t| t.byte_length).sum(),
                stored_at: p.stored_at,
                validated_at: p.validated_at,
                // The exact logical negation of `find_track`'s staleness
                // check (`>=`), so the two never disagree at the boundary.
                valid: now.saturating_sub(p.validated_at) < self.validity.as_secs(),
            })
            .collect()
    }

    /// Delete every pin and its chunks (logout, a terminal subscription
    /// sub-status). The cache directory and its loopback server stay open —
    /// only the content is wiped — so a process that is already running
    /// does not need to be restarted for the wipe to take effect.
    pub fn wipe_all(&self) -> OfflineResult<()> {
        let mut idx = self.index.lock().unwrap();
        for pin in idx.pins.values() {
            self.cleanup_chunks(&pin.tracks);
        }
        idx.pins = BTreeMap::new();
        self.persist(&idx)
    }

    /// Delete the entire offline cache directory outright. Used where no
    /// `OfflineCache` needs to be (or already is) open — the CLI's
    /// `logout` command, which has no reason to resolve the cache key or
    /// start a loopback server just to wipe.
    pub fn wipe_dir(offline_dir: &Path) -> OfflineResult<()> {
        match std::fs::remove_dir_all(offline_dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn find_track(&self, track_id: u64) -> Option<(String, TrackEntry, bool)> {
        let idx = self.index.lock().unwrap();
        let now = now_secs();
        for (key, pin) in &idx.pins {
            if let Some(entry) = pin.tracks.get(&track_id) {
                // `>=`, not `>`: a zero-length validity window (or a pin
                // validated in the same wall-clock second as this check)
                // must count as stale immediately, not only once a full
                // second has elapsed.
                let stale = now.saturating_sub(pin.validated_at) >= self.validity.as_secs();
                return Some((key.clone(), entry.clone(), stale));
            }
        }
        None
    }

    fn mark_all_revalidated(&self) {
        let now = now_secs();
        let mut idx = self.index.lock().unwrap();
        for pin in idx.pins.values_mut() {
            pin.validated_at = now;
        }
        if let Err(e) = self.persist(&idx) {
            tracing::warn!(%e, "could not persist offline cache revalidation");
        }
    }

    /// Try a full round trip on chunk 0 to make sure the current file key
    /// can actually decrypt this track — the cheap way to detect "this
    /// cache was copied from another install" (D-022) before handing a
    /// dead URL to the engine.
    fn sanity_decrypt(&self, entry: &TrackEntry) -> OfflineResult<()> {
        let salt = decode_salt(&entry.salt_hex)?;
        let path = self.chunks_dir.join(chunk_name(&salt, 0));
        let ciphertext = std::fs::read(&path)?;
        decrypt_chunk(&self.file_key, &salt, 0, &ciphertext)?;
        Ok(())
    }

    /// Serve `track_id` from the cache for playback, if it is pinned,
    /// still (or freshly re-) validated, and its key still decrypts. On any
    /// of those failing this logs why and returns `None`, which is
    /// `Player`'s cue to fall back to streaming (D-022: never crash, never
    /// block playback on a stale or broken cache entry).
    pub async fn serve_track(&self, api: &ApiClient, track_id: u64) -> Option<ServedTrack> {
        let (_, entry, stale) = self.find_track(track_id)?;
        if stale {
            match api.session().await {
                Ok(_) => self.mark_all_revalidated(),
                Err(e) => {
                    tracing::info!(
                        track_id,
                        error = %e,
                        "offline cache: pin is past its validity window and could not be \
                         revalidated online; falling back to streaming"
                    );
                    return None;
                }
            }
        }
        if let Err(e) = self.sanity_decrypt(&entry) {
            tracing::warn!(
                track_id,
                error = %e,
                "offline cache entry failed to decrypt; falling back to streaming"
            );
            return None;
        }
        Some(ServedTrack {
            url: format!("http://127.0.0.1:{}/{}/{}", self.port, self.token, track_id),
            quality: entry.quality,
            manifest_hash: entry.manifest_hash,
            codec: entry.codec,
            sample_rate: entry.sample_rate,
            bit_depth: entry.bit_depth,
            replay_gain_db: entry.replay_gain_db,
            peak_amplitude: entry.peak_amplitude,
            album_replay_gain_db: entry.album_replay_gain_db,
            album_peak_amplitude: entry.album_peak_amplitude,
        })
    }
}

/// Whether an API error indicates the account's *subscription itself* is
/// why playback failed, not a temporary or purely technical block — the
/// offline cache is wiped on these (D-022). Deliberately narrower than
/// `ApiError::is_terminal_playback`'s wider terminal set, which also
/// includes purely technical causes (a rotated client id, an invalid
/// session) that say nothing about the subscription and would make wiping
/// on them destroy a perfectly good cache for the wrong reason.
pub fn is_subscription_terminal(err: &streamboat_core::Error) -> bool {
    matches!(
        err,
        streamboat_core::Error::Api(a) if matches!(a.sub_status, Some(4030 | 4031 | 4032 | 4034 | 4035))
    )
}

fn gc_orphans(chunks_dir: &Path, index: &Index) -> OfflineResult<()> {
    if !chunks_dir.exists() {
        return Ok(());
    }
    let mut expected = HashSet::new();
    for pin in index.pins.values() {
        for entry in pin.tracks.values() {
            let Ok(salt) = decode_salt(&entry.salt_hex) else {
                continue;
            };
            for i in 0..entry.chunk_count() {
                expected.insert(chunk_name(&salt, i));
            }
        }
    }
    for dir_entry in std::fs::read_dir(chunks_dir)? {
        let dir_entry = dir_entry?;
        let name = dir_entry.file_name().to_string_lossy().to_string();
        if !expected.contains(&name) {
            let _ = std::fs::remove_file(dir_entry.path());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The loopback playback route.

fn spawn_loopback(listener: TcpListener, cache: Arc<OfflineCache>) {
    let app = axum::Router::new()
        .route("/{token}/{track_id}", get(serve_chunk))
        .with_state(cache);
    tokio::spawn(async move {
        if let Err(e) = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        {
            tracing::warn!(%e, "offline cache loopback server stopped");
        }
    });
}

/// Parses `Range: bytes=start-end` / `bytes=start-`. Anything else (multiple
/// ranges, `bytes=-N` suffix ranges) is treated as "no range" — a whole-body
/// response is always a safe fallback.
fn parse_range(value: &str, total: u64) -> Option<(u64, u64)> {
    let s = value.trim().strip_prefix("bytes=")?;
    let (start_s, end_s) = s.split_once('-')?;
    if start_s.is_empty() {
        return None;
    }
    let start: u64 = start_s.parse().ok()?;
    let end: u64 = if end_s.is_empty() {
        total.saturating_sub(1)
    } else {
        end_s.parse().ok()?
    };
    if start > end {
        return None;
    }
    Some((start, end.min(total.saturating_sub(1))))
}

async fn serve_chunk(
    AxumPath((token, track_id)): AxumPath<(String, u64)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(cache): State<Arc<OfflineCache>>,
) -> Response {
    if !addr.ip().is_loopback() {
        return (StatusCode::FORBIDDEN, "loopback only").into_response();
    }
    if token != cache.token {
        return (StatusCode::FORBIDDEN, "bad token").into_response();
    }
    let Some((_, entry, stale)) = cache.find_track(track_id) else {
        return (StatusCode::NOT_FOUND, "not pinned").into_response();
    };
    if stale {
        // The loopback route itself never re-validates online (no
        // `ApiClient` in scope here) — `Player`/the CLI already ran
        // `serve_track` before handing out this URL, so reaching here with
        // a still-stale entry means playback started, then the validity
        // window ticked over mid-track; serving the rest of the same track
        // is harmless, refusing a *new* stale request is the safe default.
        return (StatusCode::FORBIDDEN, "pin needs revalidation").into_response();
    }
    let total = entry.byte_length;
    let salt = match decode_salt(&entry.salt_hex) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(%e, "offline cache: corrupt index entry");
            return (StatusCode::INTERNAL_SERVER_ERROR, "corrupt cache entry").into_response();
        }
    };
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| parse_range(s, total));
    let (start, end) = range.unwrap_or((0, total.saturating_sub(1)));
    if total == 0 || start >= total || end >= total {
        return (
            StatusCode::RANGE_NOT_SATISFIABLE,
            [(header::CONTENT_RANGE, format!("bytes */{total}"))],
        )
            .into_response();
    }
    let chunk_size = u64::from(entry.chunk_size);
    let first_chunk = (start / chunk_size) as u32;
    let last_chunk = (end / chunk_size) as u32;
    let mut body = Vec::with_capacity((end - start + 1) as usize);
    for idx in first_chunk..=last_chunk {
        let name = chunk_name(&salt, idx);
        let path = cache.chunks_dir.join(&name);
        let ciphertext = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(%e, chunk = %name, "offline cache: chunk missing");
                return (StatusCode::INTERNAL_SERVER_ERROR, "chunk missing").into_response();
            }
        };
        let plain = match decrypt_chunk(&cache.file_key, &salt, idx, &ciphertext) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(%e, "offline cache: chunk failed to decrypt on the loopback route");
                return (StatusCode::INTERNAL_SERVER_ERROR, "decrypt failed").into_response();
            }
        };
        let chunk_start = u64::from(idx) * chunk_size;
        let chunk_end = chunk_start + plain.len() as u64; // exclusive
        let lo = start.max(chunk_start) - chunk_start;
        let hi = end.min(chunk_end.saturating_sub(1)) - chunk_start;
        if lo as usize <= hi as usize && (hi as usize) < plain.len() {
            body.extend_from_slice(&plain[lo as usize..=hi as usize]);
        }
    }
    let is_range = range.is_some();
    let mut builder = Response::builder()
        .status(if is_range {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, entry.mime.clone())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, body.len().to_string());
    if is_range {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{total}"),
        );
    }
    match builder.body(axum::body::Body::from(body)) {
        Ok(r) => r.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "response build failed").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use streamboat_core::token_store::MemoryKeySlot;

    #[test]
    fn dash_number_template_is_planned_in_order() {
        // The exact shape `streamboat-core`'s own manifest test fixture
        // uses (`crates/streamboat-core/src/manifest.rs`): one `S` with
        // `r="35"` (36 segments) plus one implicit-`r=0` `S` (1 segment).
        let mpd = r#"<?xml version="1.0"?>
<MPD><Period><AdaptationSet><Representation id="FLAC,44100,16" codecs="flac">
  <SegmentTemplate initialization="https://cdn.example/i.mp4?t=1" media="https://cdn.example/$Number$.m4s?t=1" startNumber="1" timescale="44100">
    <SegmentTimeline><S d="176128" r="35"/><S d="120832"/></SegmentTimeline>
  </SegmentTemplate>
</Representation></AdaptationSet></Period></MPD>"#;
        let plan = plan_dash_segments(mpd).unwrap();
        assert_eq!(plan.init_url, "https://cdn.example/i.mp4?t=1");
        assert_eq!(plan.media_urls.len(), 37);
        assert_eq!(plan.media_urls[0], "https://cdn.example/1.m4s?t=1");
        assert_eq!(plan.media_urls[36], "https://cdn.example/37.m4s?t=1");
    }

    #[test]
    fn dash_open_ended_timeline_is_refused_not_guessed() {
        let mpd = r#"<MPD><Period><AdaptationSet><Representation codecs="flac">
  <SegmentTemplate initialization="https://cdn.example/i.mp4" media="https://cdn.example/$Number$.m4s" startNumber="1">
    <SegmentTimeline><S d="1" r="-1"/></SegmentTimeline>
  </SegmentTemplate>
</Representation></AdaptationSet></Period></MPD>"#;
        assert!(plan_dash_segments(mpd).is_err());
    }

    #[test]
    fn chunk_encrypt_decrypt_round_trips_and_rejects_wrong_key() {
        let key_a = [9u8; 32];
        let key_b = [10u8; 32];
        let salt = [1, 2, 3, 4, 5, 6, 7, 8];
        let plaintext = b"some track bytes, chunk zero";
        let ciphertext = encrypt_chunk(&key_a, &salt, 0, plaintext).unwrap();
        assert_ne!(ciphertext, plaintext);
        let back = decrypt_chunk(&key_a, &salt, 0, &ciphertext).unwrap();
        assert_eq!(back, plaintext);
        assert!(decrypt_chunk(&key_b, &salt, 0, &ciphertext).is_err());
    }

    #[test]
    fn hkdf_derivation_is_device_bound() {
        let secret = [42u8; 32];
        let key_device_a = derive_file_key(&secret, "device-a");
        let key_device_b = derive_file_key(&secret, "device-b");
        assert_ne!(key_device_a, key_device_b);
        // Deterministic for the same (secret, device) pair.
        assert_eq!(key_device_a, derive_file_key(&secret, "device-a"));
    }

    #[test]
    fn memory_key_slot_is_a_valid_keyslot_for_offline_resolution() {
        // Exercises the same resolution path `OfflineCache::open` uses,
        // against a `MemoryKeySlot` (a test-only keyring stand-in) instead
        // of a real OS keyring.
        let dir = tempfile::tempdir().unwrap();
        let slot = MemoryKeySlot::default();
        let key_path = dir.path().join("offline.key");
        let k1 = resolve_install_secret(&key_path, &slot, KeyStorage::Auto).unwrap();
        let k2 = resolve_install_secret(&key_path, &slot, KeyStorage::Auto).unwrap();
        assert_eq!(k1, k2, "the same slot must resolve to the same secret");
        assert!(
            !key_path.exists(),
            "no key file should be written when the keyring slot works"
        );
    }
}
