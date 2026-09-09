//! `playbackinfopostpaywall` responses and the manifests inside them.
//!
//! Four MIME types exist (`audio-pipeline/references/tidal-manifest-api.md` §3):
//! BTS and EMU are base64 JSON with direct CDN URLs, DASH is a base64 MPD,
//! HLS is video-only. The refusal rule (§5, D-022): any encryption marker
//! (`encryptionType != "NONE"`, a `keyId`, a `licenseSecurityToken`, or a
//! DASH `ContentProtection` element) is refused loudly and never decrypted.

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::models::{AudioMode, AudioQuality};

pub const MIME_BTS: &str = "application/vnd.tidal.bts";
pub const MIME_EMU: &str = "application/vnd.tidal.emu";
pub const MIME_DASH: &str = "application/dash+xml";
pub const MIME_HLS: &str = "application/vnd.apple.mpegurl";

/// `GET /v1/tracks/{id}/playbackinfopostpaywall`. Every field optional on
/// purpose: `bitDepth`/`sampleRate` are nullable at every tier, and
/// `subStatus`-style oddities show up here too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackInfo {
    pub track_id: Option<u64>,
    pub asset_presentation: Option<String>,
    pub preview_reason: Option<String>,
    pub audio_mode: Option<AudioMode>,
    /// What TIDAL actually delivered, which may be lower than requested.
    pub audio_quality: Option<AudioQuality>,
    pub manifest_mime_type: Option<String>,
    pub manifest: Option<String>,
    pub manifest_hash: Option<String>,
    pub bit_depth: Option<u32>,
    pub sample_rate: Option<u32>,
    pub album_replay_gain: Option<f64>,
    pub album_peak_amplitude: Option<f64>,
    pub track_replay_gain: Option<f64>,
    pub track_peak_amplitude: Option<f64>,
    pub license_security_token: Option<String>,
    pub streaming_session_id: Option<String>,
}

impl PlaybackInfo {
    pub fn is_preview(&self) -> bool {
        self.asset_presentation
            .as_deref()
            .map(|p| !p.eq_ignore_ascii_case("FULL"))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BtsManifest {
    mime_type: Option<String>,
    codecs: Option<String>,
    encryption_type: Option<String>,
    key_id: Option<String>,
    license_security_token: Option<String>,
    #[serde(default)]
    urls: Vec<String>,
}

/// What the engine opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamSource {
    /// A direct, signed, expiring CDN URL (BTS, EMU, or a bare-`BaseURL` MPD).
    Url(String),
    /// A DASH MPD document; the engine feeds it as a `data:` URI or a
    /// per-session temp file (never persisted, `tidal-api` playback §9).
    DashMpd(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedManifest {
    pub source: StreamSource,
    /// `bts`, `emu` or `dash`.
    pub kind: &'static str,
    pub codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
}

fn decode_base64(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    let engines = [
        base64::engine::general_purpose::STANDARD,
        base64::engine::general_purpose::STANDARD_NO_PAD,
        base64::engine::general_purpose::URL_SAFE,
        base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];
    for e in engines {
        if let Ok(v) = e.decode(s) {
            return Ok(v);
        }
    }
    Err(Error::Manifest("manifest is not valid base64".into()))
}

fn non_empty(s: &Option<String>) -> bool {
    s.as_deref().map(|v| !v.trim().is_empty()).unwrap_or(false)
}

/// Parse the manifest carried by a playback-info response, applying the
/// refusal rule before anything is handed to an engine.
pub fn parse(info: &PlaybackInfo) -> Result<ParsedManifest> {
    if non_empty(&info.license_security_token) {
        return Err(Error::ManifestRefused(
            "TIDAL served a DRM-licensed asset (licenseSecurityToken present) for this client id; \
             streamboat does not decrypt streams. Use a client id that receives cleartext \
             streams (see README, \"Client credentials\")"
                .into(),
        ));
    }
    let mime = info
        .manifest_mime_type
        .as_deref()
        .map(|m| m.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let raw = info
        .manifest
        .as_deref()
        .ok_or_else(|| Error::Manifest("playback info carries no manifest".into()))?;
    let bytes = decode_base64(raw)?;

    match mime.as_str() {
        MIME_BTS | MIME_EMU => parse_bts(&bytes, if mime == MIME_BTS { "bts" } else { "emu" }),
        MIME_DASH => parse_dash(&bytes),
        MIME_HLS => Err(Error::Manifest(
            "TIDAL served an HLS manifest; HLS is only used for video and is not supported on \
             the audio path"
                .into(),
        )),
        other => Err(Error::Manifest(format!("unknown manifest type {other:?}"))),
    }
}

fn parse_bts(bytes: &[u8], kind: &'static str) -> Result<ParsedManifest> {
    let m: BtsManifest = serde_json::from_slice(bytes)
        .map_err(|e| Error::Manifest(format!("{kind} manifest is not valid JSON: {e}")))?;
    let enc = m.encryption_type.as_deref().map(str::trim).unwrap_or("NONE");
    if !enc.eq_ignore_ascii_case("NONE") || non_empty(&m.key_id) || non_empty(&m.license_security_token) {
        return Err(Error::ManifestRefused(format!(
            "TIDAL served an encrypted stream (encryptionType {enc:?}) for this client id; streamboat \
             does not decrypt streams. Lower the quality ceiling or use a client id that receives \
             cleartext streams (see README, \"Client credentials\")"
        )));
    }
    let url = m
        .urls
        .into_iter()
        .find(|u| !u.trim().is_empty())
        .ok_or_else(|| Error::Manifest(format!("{kind} manifest carries no URLs")))?;
    let (sample_rate, bit_depth) = (None, None);
    Ok(ParsedManifest {
        source: StreamSource::Url(url),
        kind,
        codec: m.codecs.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()),
        sample_rate,
        bit_depth,
    })
}

fn parse_dash(bytes: &[u8]) -> Result<ParsedManifest> {
    let xml = std::str::from_utf8(bytes)
        .map_err(|_| Error::Manifest("DASH manifest is not UTF-8".into()))?
        .to_string();
    let doc = roxmltree::Document::parse(&xml)
        .map_err(|e| Error::Manifest(format!("DASH manifest is not well-formed XML: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "MPD" {
        return Err(Error::Manifest(format!(
            "DASH manifest root is <{}>, expected <MPD>",
            root.tag_name().name()
        )));
    }
    if root
        .descendants()
        .any(|n| n.is_element() && n.tag_name().name() == "ContentProtection")
    {
        return Err(Error::ManifestRefused(
            "TIDAL served a DRM-protected DASH manifest (ContentProtection present) for this client \
             id; streamboat does not decrypt streams. Lower the quality ceiling or use a client id \
             that receives cleartext streams (see README, \"Client credentials\")"
                .into(),
        ));
    }
    let representation = root
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "Representation");
    let (mut codec, mut sample_rate, mut bit_depth) = (None, None, None);
    if let Some(rep) = representation {
        codec = rep.attribute("codecs").map(|c| c.trim().to_string());
        sample_rate = rep.attribute("audioSamplingRate").and_then(|r| r.trim().parse().ok());
        // `Representation@id` carries "FLAC,44100,16" (codec, rate, depth) in
        // TIDAL's MPDs; the web SDK's parser is the only evidence, so treat it
        // as a hint that fills gaps rather than as the source of truth.
        if let Some(id) = rep.attribute("id") {
            let parts: Vec<&str> = id.split(',').map(str::trim).collect();
            if parts.len() >= 3 {
                if sample_rate.is_none() {
                    sample_rate = parts[1].parse().ok();
                }
                bit_depth = parts[2].parse().ok();
                if codec.is_none() && !parts[0].is_empty() {
                    codec = Some(parts[0].to_ascii_lowercase());
                }
            }
        }
    }
    let has_template = root
        .descendants()
        .any(|n| n.is_element() && matches!(n.tag_name().name(), "SegmentTemplate" | "SegmentList" | "SegmentBase"));
    let base_url = root
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "BaseURL")
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|s| s.starts_with("http"));
    let source = match (has_template, base_url) {
        // A bare <BaseURL> MPD with no segmenting is a direct file, handled
        // like BTS (tidal-cli's documented fallback).
        (false, Some(url)) => StreamSource::Url(url.to_string()),
        (true, _) => StreamSource::DashMpd(xml.clone()),
        (false, None) => {
            return Err(Error::Manifest(
                "DASH manifest has neither a SegmentTemplate nor a BaseURL".into(),
            ));
        }
    };
    Ok(ParsedManifest { source, kind: "dash", codec, sample_rate, bit_depth })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;

    fn info(mime: &str, body: &str) -> PlaybackInfo {
        PlaybackInfo {
            manifest_mime_type: Some(mime.into()),
            manifest: Some(STANDARD.encode(body)),
            ..Default::default()
        }
    }

    #[test]
    fn bts_cleartext() {
        let p = parse(&info(
            MIME_BTS,
            r#"{"mimeType":"audio/flac","codecs":"flac","encryptionType":"NONE","urls":["https://cdn.example/a.flac?token=x"]}"#,
        ))
        .unwrap();
        assert_eq!(p.kind, "bts");
        assert_eq!(p.codec.as_deref(), Some("flac"));
        assert_eq!(p.source, StreamSource::Url("https://cdn.example/a.flac?token=x".into()));
    }

    #[test]
    fn emu_is_a_subset_of_bts() {
        let p = parse(&info(MIME_EMU, r#"{"mimeType":"audio/mp4","urls":["https://cdn.example/e"]}"#))
            .unwrap();
        assert_eq!(p.kind, "emu");
        assert_eq!(p.codec, None);
    }

    #[test]
    fn bts_encrypted_is_refused() {
        let err = parse(&info(
            MIME_BTS,
            r#"{"codecs":"flac","encryptionType":"OLD_AES","keyId":"abc","urls":["https://x"]}"#,
        ))
        .unwrap_err();
        assert!(matches!(err, Error::ManifestRefused(_)), "{err}");
    }

    #[test]
    fn license_token_is_refused_before_parsing() {
        let mut i = info(MIME_BTS, r#"{"urls":["https://x"]}"#);
        i.license_security_token = Some("tok".into());
        assert!(matches!(parse(&i), Err(Error::ManifestRefused(_))));
    }

    const MPD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT2M26.47S">
  <Period id="0"><AdaptationSet id="0" contentType="audio" mimeType="audio/mp4">
    <Representation id="FLAC,44100,16" codecs="flac" bandwidth="1024000" audioSamplingRate="44100">
      <SegmentTemplate initialization="https://cdn.example/i.mp4?t=1" media="https://cdn.example/$Number$.m4s?t=1" startNumber="1" timescale="44100">
        <SegmentTimeline><S d="176128" r="35"/><S d="120832"/></SegmentTimeline>
      </SegmentTemplate>
    </Representation></AdaptationSet></Period></MPD>"#;

    #[test]
    fn dash_cleartext() {
        let p = parse(&info(MIME_DASH, MPD)).unwrap();
        assert_eq!(p.kind, "dash");
        assert_eq!(p.codec.as_deref(), Some("flac"));
        assert_eq!(p.sample_rate, Some(44100));
        assert_eq!(p.bit_depth, Some(16));
        assert!(matches!(p.source, StreamSource::DashMpd(_)));
    }

    #[test]
    fn dash_with_content_protection_is_refused() {
        let mpd = MPD.replace(
            "<Representation",
            r#"<ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011" value="cenc"/><Representation"#,
        );
        assert!(matches!(parse(&info(MIME_DASH, &mpd)), Err(Error::ManifestRefused(_))));
    }

    #[test]
    fn dash_bare_base_url_is_direct() {
        let mpd = r#"<MPD><Period><AdaptationSet><Representation id="FLAC,96000,24" codecs="flac"><BaseURL>https://cdn.example/whole.flac?t=2</BaseURL></Representation></AdaptationSet></Period></MPD>"#;
        let p = parse(&info(MIME_DASH, mpd)).unwrap();
        assert_eq!(p.source, StreamSource::Url("https://cdn.example/whole.flac?t=2".into()));
        assert_eq!(p.sample_rate, Some(96000));
        assert_eq!(p.bit_depth, Some(24));
    }

    #[test]
    fn hls_is_not_supported_on_audio_path() {
        let err = parse(&info(MIME_HLS, "#EXTM3U")).unwrap_err();
        assert!(matches!(err, Error::Manifest(_)));
    }

    #[test]
    fn unpadded_base64_is_accepted() {
        let mut i = info(MIME_BTS, r#"{"urls":["https://x/y"]}"#);
        i.manifest = Some(i.manifest.unwrap().trim_end_matches('=').to_string());
        assert!(parse(&i).is_ok());
    }
}
