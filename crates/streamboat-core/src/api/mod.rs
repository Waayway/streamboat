//! The v1 endpoints the spike needs, plus the quality cascade that turns a
//! track id into something an engine can open. The catalogue/library
//! surface added on top of that spike (D-001, D-015, D-028, D-039) lives in
//! the sibling modules below, each documented at its own endpoint's exact
//! reference citation.

pub mod catalog;
pub mod images;
pub mod library;
pub mod lyrics;
pub mod pages;
pub mod pagination;
pub mod playlists;
pub mod search;

use crate::error::{Error, Result};
use crate::http::ApiClient;
use crate::manifest::{self, PlaybackInfo};
use crate::models::{AudioMode, AudioQuality, Page, Session, Track};
use crate::proto::StreamInfo;
use crate::token_store::AuthFlow;

pub use crate::manifest::StreamSource;

/// A resolved, playable stream for one track.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedStream {
    pub track_id: u64,
    pub requested: AudioQuality,
    pub source: StreamSource,
    pub info: StreamInfo,
    pub manifest_hash: Option<String>,
    /// Things the user should hear about that did not prevent playback.
    pub warnings: Vec<String>,
}

impl ApiClient {
    pub async fn session(&self) -> Result<Session> {
        let s: Session = self.get_json("v1/sessions", &[], &[]).await?;
        if !s.country_code.is_empty() {
            self.set_country_code(s.country_code.clone());
        }
        Ok(s)
    }

    /// The account's country code: from the token set, else a settings hint,
    /// else one session call, cached afterwards.
    pub async fn country_code(&self) -> Result<String> {
        if let Some(cc) = self.country_code_hint() {
            return Ok(cc);
        }
        if let Some(cc) = self.tokens().await.and_then(|t| t.country_code) {
            self.set_country_code(cc.clone());
            return Ok(cc);
        }
        let s = self.session().await?;
        if s.country_code.is_empty() {
            return Err(Error::Config(
                "TIDAL did not report a country code for this session".into(),
            ));
        }
        Ok(s.country_code)
    }

    pub async fn track(&self, id: u64) -> Result<Track> {
        let cc = self.country_code().await?;
        self.get_json(&format!("v1/tracks/{id}"), &[("countryCode", cc)], &[])
            .await
    }

    pub async fn search_tracks(&self, query: &str, limit: u32) -> Result<Page<Track>> {
        let cc = self.country_code().await?;
        self.get_json(
            "v1/search/tracks",
            &[
                ("query", query.to_string()),
                ("limit", limit.to_string()),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }

    /// One `playbackinfopostpaywall` call at one tier. `streaming_session_id`
    /// is client-generated, one per playback (`tidal-manifest-api` §8).
    pub async fn playback_info(
        &self,
        track_id: u64,
        quality: AudioQuality,
        streaming_session_id: &str,
    ) -> Result<PlaybackInfo> {
        let cc = self.country_code().await?;
        self.get_json(
            &format!("v1/tracks/{track_id}/playbackinfopostpaywall"),
            &[
                ("audioquality", quality.as_str().to_string()),
                ("playbackmode", "STREAM".to_string()),
                ("assetpresentation", "FULL".to_string()),
                ("countryCode", cc),
            ],
            &[
                ("x-tidal-token", self.session_client_id().await),
                (
                    "x-tidal-streamingsessionid",
                    streaming_session_id.to_string(),
                ),
            ],
        )
        .await
    }

    /// The tiers to try for a ceiling, highest first. Hi-res tiers are
    /// dropped pre-emptively when the client id that minted the session has
    /// no secret (D-023, Sone's rule); a device-code session gets a warning
    /// because python-tidal documents PKCE as the only route to hi-res.
    pub async fn quality_ladder(&self, ceiling: AudioQuality) -> (Vec<AudioQuality>, Vec<String>) {
        let mut warnings = Vec::new();
        let tokens = self.tokens().await;
        let client_id = tokens
            .as_ref()
            .map(|t| t.client_id.clone())
            .unwrap_or_else(|| self.credentials().primary_client_id().to_string());
        let has_secret = self
            .credentials()
            .pair_for(&client_id)
            .map(|p| p.has_secret())
            .unwrap_or(false);
        let flow = tokens.as_ref().map(|t| t.flow).unwrap_or_default();
        let wants_hires = ceiling.is_hi_res();
        if wants_hires && !has_secret {
            warnings.push(
                "hi-res tiers skipped: the client id this session uses has no client secret, and \
                 TIDAL only serves cleartext HI_RES_LOSSLESS to a client id with one"
                    .to_string(),
            );
        } else if wants_hires && flow == AuthFlow::DeviceCode {
            warnings.push(
                "hi-res requested on a device-code session; TIDAL normally serves HI_RES_LOSSLESS \
                 only to PKCE sessions (`streamboat login --pkce`)"
                    .to_string(),
            );
        }
        let tiers: Vec<AudioQuality> = AudioQuality::LADDER
            .iter()
            .copied()
            .filter(|q| q.rank() <= ceiling.rank())
            .filter(|q| !q.is_hi_res() || has_secret)
            .collect();
        (tiers, warnings)
    }

    /// The quality cascade (`tidal-api` playback §4): try each tier from the
    /// ceiling down; stop at the first playable manifest; stop immediately on
    /// network errors, rate limits and terminal sub-statuses; an encrypted
    /// manifest at one tier is a warning and the next tier is tried.
    pub async fn resolve_stream(
        &self,
        track_id: u64,
        ceiling: AudioQuality,
        streaming_session_id: &str,
    ) -> Result<ResolvedStream> {
        let (tiers, mut warnings) = self.quality_ladder(ceiling).await;
        let mut last_err: Option<Error> = None;
        for tier in tiers {
            let info = match self
                .playback_info(track_id, tier, streaming_session_id)
                .await
            {
                Ok(i) => i,
                Err(e) if e.stops_cascade() => return Err(e),
                Err(e) => {
                    tracing::debug!(%tier, error = %e, "tier failed; trying next");
                    warnings.push(format!("{tier}: {e}"));
                    last_err = Some(e);
                    continue;
                }
            };
            match manifest::parse(&info) {
                Ok(parsed) => {
                    let got = info.audio_quality;
                    if let Some(g) = got {
                        if g.rank() < tier.rank() {
                            warnings.push(format!("requested {tier}, TIDAL delivered {g}"));
                        }
                    }
                    if info.is_preview() {
                        warnings.push(format!(
                            "TIDAL served a preview instead of the full track ({})",
                            info.preview_reason
                                .clone()
                                .unwrap_or_else(|| "no reason given".into())
                        ));
                    }
                    let stream_info = StreamInfo {
                        quality: got.or(Some(tier)),
                        audio_mode: info.audio_mode.or(Some(AudioMode::Stereo)),
                        manifest_kind: parsed.kind.to_string(),
                        codec: parsed.codec.clone(),
                        sample_rate: parsed.sample_rate.or(info.sample_rate),
                        bit_depth: parsed.bit_depth.or(info.bit_depth),
                        replay_gain_db: info.track_replay_gain,
                        peak_amplitude: info.track_peak_amplitude,
                        preview: info.is_preview(),
                    };
                    return Ok(ResolvedStream {
                        track_id: info.track_id.unwrap_or(track_id),
                        requested: tier,
                        source: parsed.source,
                        info: stream_info,
                        manifest_hash: info.manifest_hash,
                        warnings,
                    });
                }
                Err(e @ Error::ManifestRefused(_)) => {
                    tracing::warn!(%tier, "{e}");
                    warnings.push(format!("{tier} refused: {e}"));
                    last_err = Some(e);
                }
                Err(e) => {
                    warnings.push(format!("{tier}: {e}"));
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| {
            Error::Manifest(format!("no playable quality tier at or below {ceiling}"))
        }))
    }
}
