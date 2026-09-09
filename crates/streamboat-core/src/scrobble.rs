//! Scrobbling to Last.fm and ListenBrainz behind one `Scrobbler` trait
//! (D-037), each backend independently enabled with its own credentials
//! and its own persistent retry queue for `scrobble()` calls.
//!
//! Unlike `reporting::PlayReporter`, these wire formats are each service's
//! own public, stable API — not part of the TIDAL research this project
//! otherwise cites `docs/research/` for:
//! - Last.fm: <https://www.last.fm/api> (`auth.getSession`'s signature
//!   scheme, `track.updateNowPlaying`, `track.scrobble`).
//! - ListenBrainz: <https://listenbrainz.readthedocs.io/en/latest/users/api/core.html>
//!   (`submit-listens`, `playing_now`/`single` listen types, `Authorization:
//!   Token <user_token>`).
//!
//! `now_playing` is a best-effort, unqueued ping for both backends — by the
//! time a retry would land the "now playing" status is stale anyway, which
//! is why neither service's own client guidance asks for one. `scrobble` is
//! queued and retried: unlike a now-playing ping, losing a real scrobble is
//! the kind of thing a user notices.

use std::collections::VecDeque;
use std::path::PathBuf;

use async_trait::async_trait;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;
use url::Url;

use crate::error::{Error, Result};
use crate::fsutil;

// ---------------------------------------------------------------------------
// Settings

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ScrobbleSettings {
    pub lastfm: LastfmSettings,
    pub listenbrainz: ListenBrainzSettings,
}

/// Last.fm session-key auth (the web-auth flow: `auth.getToken` then a
/// browser confirmation, then `auth.getSession` — or the mobile-session
/// flow, `auth.getMobileSession` with a username/password, for a headless
/// box with no browser). Either flow ends the same way: a session key that
/// never expires until revoked, stored here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LastfmSettings {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub session_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ListenBrainzSettings {
    pub enabled: bool,
    pub user_token: Option<String>,
}

// ---------------------------------------------------------------------------
// The trait

/// What a scrobble backend needs about one track: text only, no TIDAL id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrobbleTrack {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub duration_s: Option<u32>,
    pub track_number: Option<u32>,
    pub mbid: Option<String>,
}

/// One scrobbling destination. Both methods swallow their own errors — a
/// broken scrobbler must never interrupt playback — logging instead;
/// `scrobble` additionally persists to an on-disk queue before returning,
/// so a crash right after does not lose it.
#[async_trait]
pub trait Scrobbler: Send + Sync {
    async fn now_playing(&self, track: &ScrobbleTrack);
    async fn scrobble(&self, track: &ScrobbleTrack, started_at_unix_s: u64);
}

/// Dispatches to every backend that is configured and enabled — what
/// `streamboat-player` actually holds as `Option<Arc<dyn Scrobbler>>`, so
/// the player never needs to know how many backends are live.
pub struct ScrobbleHub {
    backends: Vec<std::sync::Arc<dyn Scrobbler>>,
}

impl ScrobbleHub {
    pub fn new(backends: Vec<std::sync::Arc<dyn Scrobbler>>) -> Self {
        Self { backends }
    }

    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }
}

#[async_trait]
impl Scrobbler for ScrobbleHub {
    async fn now_playing(&self, track: &ScrobbleTrack) {
        for b in &self.backends {
            b.now_playing(track).await;
        }
    }
    async fn scrobble(&self, track: &ScrobbleTrack, started_at_unix_s: u64) {
        for b in &self.backends {
            b.scrobble(track, started_at_unix_s).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Shared on-disk queue (both backends use the same shape)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingScrobble {
    track: ScrobbleTrack,
    started_at_unix_s: u64,
}

fn load_queue(path: &std::path::Path) -> Result<VecDeque<PendingScrobble>> {
    match std::fs::read(path) {
        Ok(bytes) if bytes.is_empty() => Ok(VecDeque::new()),
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(VecDeque::new()),
        Err(e) => Err(e.into()),
    }
}

fn persist_queue(path: &std::path::Path, queue: &VecDeque<PendingScrobble>) -> Result<()> {
    let items: Vec<&PendingScrobble> = queue.iter().collect();
    fsutil::atomic_write(path, &serde_json::to_vec(&items)?, 0o600)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Last.fm

pub const DEFAULT_LASTFM_BASE: &str = "https://ws.audioscrobbler.com/2.0/";

/// `md5(sorted "key" + "value" pairs, excluding "format"/"callback",
/// concatenated, then the shared secret)` — Last.fm's own signing scheme
/// for every authenticated call (<https://www.last.fm/api/authspec>).
fn lastfm_signature(params: &[(String, String)], secret: &str) -> String {
    let mut sorted: Vec<&(String, String)> = params
        .iter()
        .filter(|(k, _)| k != "format" && k != "callback")
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut s = String::new();
    for (k, v) in sorted {
        s.push_str(k);
        s.push_str(v);
    }
    s.push_str(secret);
    let digest = Md5::digest(s.as_bytes());
    hex::encode(digest)
}

pub struct LastfmScrobbler {
    http: reqwest::Client,
    base: Url,
    api_key: String,
    api_secret: String,
    session_key: String,
    queue_path: PathBuf,
    queue: Mutex<VecDeque<PendingScrobble>>,
}

impl LastfmScrobbler {
    /// `None` when disabled or missing a credential — callers fold that
    /// straight into "this backend does not exist" rather than an error.
    pub fn open(
        settings: &LastfmSettings,
        queue_path: PathBuf,
        user_agent: &str,
    ) -> Result<Option<Self>> {
        if !settings.enabled {
            return Ok(None);
        }
        let (Some(api_key), Some(api_secret), Some(session_key)) = (
            settings.api_key.clone(),
            settings.api_secret.clone(),
            settings.session_key.clone(),
        ) else {
            return Ok(None);
        };
        let http = reqwest::Client::builder().user_agent(user_agent).build()?;
        let queue = load_queue(&queue_path)?;
        Ok(Some(Self {
            http,
            base: Url::parse(DEFAULT_LASTFM_BASE).expect("valid default last.fm base"),
            api_key,
            api_secret,
            session_key,
            queue_path,
            queue: Mutex::new(queue),
        }))
    }

    /// Override the API host (tests only).
    pub fn base(mut self, url: &str) -> Result<Self> {
        self.base = Url::parse(url).map_err(|e| Error::Config(format!("bad last.fm base: {e}")))?;
        Ok(self)
    }

    pub async fn pending_len(&self) -> usize {
        self.queue.lock().await.len()
    }

    fn sign(&self, mut params: Vec<(String, String)>) -> Vec<(String, String)> {
        params.push(("api_key".into(), self.api_key.clone()));
        params.push(("sk".into(), self.session_key.clone()));
        let sig = lastfm_signature(&params, &self.api_secret);
        params.push(("api_sig".into(), sig));
        params.push(("format".into(), "json".into()));
        params
    }

    async fn call(&self, params: Vec<(String, String)>) -> Result<reqwest::StatusCode> {
        let signed = self.sign(params);
        let resp = self
            .http
            .post(self.base.clone())
            .form(&signed)
            .send()
            .await?;
        Ok(resp.status())
    }

    /// Send queued scrobbles one at a time, stopping at the first one that
    /// is not yet deliverable (network error or a 5xx) so it and everything
    /// behind it stay queued in order.
    pub async fn try_flush(&self) -> Result<usize> {
        let mut sent = 0;
        loop {
            let next = { self.queue.lock().await.front().cloned() };
            let Some(pending) = next else { return Ok(sent) };
            let mut params = vec![
                ("method".into(), "track.scrobble".into()),
                ("artist".into(), pending.track.artist.clone()),
                ("track".into(), pending.track.title.clone()),
                ("timestamp".into(), pending.started_at_unix_s.to_string()),
            ];
            if let Some(a) = &pending.track.album {
                params.push(("album".into(), a.clone()));
            }
            match self.call(params).await {
                Ok(status) if status.is_success() => {
                    let mut q = self.queue.lock().await;
                    q.pop_front();
                    persist_queue(&self.queue_path, &q)?;
                    sent += 1;
                }
                Ok(status) if status.is_client_error() => {
                    tracing::warn!(%status, "last.fm rejected a scrobble; dropping it");
                    let mut q = self.queue.lock().await;
                    q.pop_front();
                    persist_queue(&self.queue_path, &q)?;
                }
                Ok(status) => {
                    tracing::debug!(%status, "last.fm scrobble failed; will retry later");
                    return Ok(sent);
                }
                Err(e) => {
                    tracing::debug!(error = %e, "last.fm scrobble failed; will retry later");
                    return Ok(sent);
                }
            }
        }
    }
}

#[async_trait]
impl Scrobbler for LastfmScrobbler {
    async fn now_playing(&self, track: &ScrobbleTrack) {
        let mut params = vec![
            ("method".into(), "track.updateNowPlaying".into()),
            ("artist".into(), track.artist.clone()),
            ("track".into(), track.title.clone()),
        ];
        if let Some(a) = &track.album {
            params.push(("album".into(), a.clone()));
        }
        if let Some(d) = track.duration_s {
            params.push(("duration".into(), d.to_string()));
        }
        if let Err(e) = self.call(params).await {
            tracing::debug!(error = %e, "last.fm now-playing failed");
        }
    }

    async fn scrobble(&self, track: &ScrobbleTrack, started_at_unix_s: u64) {
        {
            let mut q = self.queue.lock().await;
            q.push_back(PendingScrobble {
                track: track.clone(),
                started_at_unix_s,
            });
            let _ = persist_queue(&self.queue_path, &q);
        }
        let _ = self.try_flush().await;
    }
}

// ---------------------------------------------------------------------------
// ListenBrainz

pub const DEFAULT_LISTENBRAINZ_BASE: &str = "https://api.listenbrainz.org/";

pub struct ListenBrainzScrobbler {
    http: reqwest::Client,
    base: Url,
    user_token: String,
    queue_path: PathBuf,
    queue: Mutex<VecDeque<PendingScrobble>>,
}

impl ListenBrainzScrobbler {
    pub fn open(
        settings: &ListenBrainzSettings,
        queue_path: PathBuf,
        user_agent: &str,
    ) -> Result<Option<Self>> {
        if !settings.enabled {
            return Ok(None);
        }
        let Some(user_token) = settings.user_token.clone() else {
            return Ok(None);
        };
        let http = reqwest::Client::builder().user_agent(user_agent).build()?;
        let queue = load_queue(&queue_path)?;
        Ok(Some(Self {
            http,
            base: Url::parse(DEFAULT_LISTENBRAINZ_BASE).expect("valid default listenbrainz base"),
            user_token,
            queue_path,
            queue: Mutex::new(queue),
        }))
    }

    /// Override the API host (tests only).
    pub fn base(mut self, url: &str) -> Result<Self> {
        self.base =
            Url::parse(url).map_err(|e| Error::Config(format!("bad listenbrainz base: {e}")))?;
        Ok(self)
    }

    pub async fn pending_len(&self) -> usize {
        self.queue.lock().await.len()
    }

    fn track_metadata(track: &ScrobbleTrack) -> serde_json::Value {
        let mut metadata = json!({
            "artist_name": track.artist,
            "track_name": track.title,
        });
        if let Some(a) = &track.album {
            metadata["release_name"] = json!(a);
        }
        let mut additional = serde_json::Map::new();
        if let Some(n) = track.track_number {
            additional.insert("tracknumber".into(), json!(n));
        }
        if let Some(d) = track.duration_s {
            additional.insert("duration".into(), json!(d));
        }
        if let Some(m) = &track.mbid {
            additional.insert("track_mbid".into(), json!(m));
        }
        if !additional.is_empty() {
            metadata["additional_info"] = serde_json::Value::Object(additional);
        }
        metadata
    }

    async fn submit(
        &self,
        listen_type: &str,
        listened_at: Option<u64>,
        track: &ScrobbleTrack,
    ) -> Result<reqwest::StatusCode> {
        let mut entry = json!({ "track_metadata": Self::track_metadata(track) });
        if let Some(ts) = listened_at {
            entry["listened_at"] = json!(ts);
        }
        let body = json!({ "listen_type": listen_type, "payload": [entry] });
        let url = self
            .base
            .join("1/submit-listens")
            .map_err(|e| Error::Config(format!("bad submit-listens url: {e}")))?;
        let resp = self
            .http
            .post(url)
            .header("Authorization", format!("Token {}", self.user_token))
            .json(&body)
            .send()
            .await?;
        Ok(resp.status())
    }

    pub async fn try_flush(&self) -> Result<usize> {
        let mut sent = 0;
        loop {
            let next = { self.queue.lock().await.front().cloned() };
            let Some(pending) = next else { return Ok(sent) };
            match self
                .submit("single", Some(pending.started_at_unix_s), &pending.track)
                .await
            {
                Ok(status) if status.is_success() => {
                    let mut q = self.queue.lock().await;
                    q.pop_front();
                    persist_queue(&self.queue_path, &q)?;
                    sent += 1;
                }
                Ok(status) if status.is_client_error() => {
                    tracing::warn!(%status, "listenbrainz rejected a listen; dropping it");
                    let mut q = self.queue.lock().await;
                    q.pop_front();
                    persist_queue(&self.queue_path, &q)?;
                }
                Ok(status) => {
                    tracing::debug!(%status, "listenbrainz submit failed; will retry later");
                    return Ok(sent);
                }
                Err(e) => {
                    tracing::debug!(error = %e, "listenbrainz submit failed; will retry later");
                    return Ok(sent);
                }
            }
        }
    }
}

#[async_trait]
impl Scrobbler for ListenBrainzScrobbler {
    async fn now_playing(&self, track: &ScrobbleTrack) {
        if let Err(e) = self.submit("playing_now", None, track).await {
            tracing::debug!(error = %e, "listenbrainz now-playing failed");
        }
    }

    async fn scrobble(&self, track: &ScrobbleTrack, started_at_unix_s: u64) {
        {
            let mut q = self.queue.lock().await;
            q.push_back(PendingScrobble {
                track: track.clone(),
                started_at_unix_s,
            });
            let _ = persist_queue(&self.queue_path, &q);
        }
        let _ = self.try_flush().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lastfm_signature_matches_a_known_vector() {
        // Independently verified: `printf '%s'
        // "api_keyakartistArtistmethodtrack.scrobbletimestamp1000000000trackSongsecret"
        // | md5sum` => 452a4654e43d55764e9096ec14efaa87
        let params = vec![
            ("method".to_string(), "track.scrobble".to_string()),
            ("artist".to_string(), "Artist".to_string()),
            ("track".to_string(), "Song".to_string()),
            ("timestamp".to_string(), "1000000000".to_string()),
            ("api_key".to_string(), "ak".to_string()),
        ];
        let sig = lastfm_signature(&params, "secret");
        assert_eq!(sig, "452a4654e43d55764e9096ec14efaa87");
    }

    #[test]
    fn lastfm_signature_excludes_format_and_callback() {
        let with_format = lastfm_signature(
            &[("a".into(), "1".into()), ("format".into(), "json".into())],
            "s",
        );
        let without = lastfm_signature(&[("a".into(), "1".into())], "s");
        assert_eq!(with_format, without);
    }
}
