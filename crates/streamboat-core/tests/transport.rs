//! Transport-level tests against an in-process HTTP server: the sequences an
//! unofficial-API client must get right without a live account
//! (`streamboat-engineering-baseline` testing §3).

use std::sync::Arc;

use base64::Engine;
use serde_json::json;
use streamboat_core::auth::device_code::{start_device_flow, wait_for_device_token};
use streamboat_core::token_store::{MemoryTokenStore, TokenSet, TokenStore, now_secs};
use streamboat_core::{ApiClient, AudioQuality, ClientCredentials, Error, StreamSource};
use wiremock::matchers::{body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn creds(secret: bool) -> ClientCredentials {
    ClientCredentials::new("cid", secret.then(|| "sec".to_string()))
}

fn tokens(expires_in: i64) -> TokenSet {
    TokenSet {
        access_token: "old-at".into(),
        refresh_token: Some("rt".into()),
        token_type: "Bearer".into(),
        expires_at: (now_secs() as i64 + expires_in) as u64,
        scope: "r_usr w_usr w_sub".into(),
        client_id: "cid".into(),
        user_id: Some(7),
        country_code: Some("NL".into()),
    }
}

fn client(server: &MockServer, store: Arc<MemoryTokenStore>, secret: bool) -> ApiClient {
    ApiClient::builder(creds(secret), store)
        .api_base(&format!("{}/", server.uri()))
        .auth_base(&format!("{}/", server.uri()))
        .build()
        .unwrap()
}

fn bts(url: &str) -> serde_json::Value {
    let manifest =
        json!({"mimeType":"audio/flac","codecs":"flac","encryptionType":"NONE","urls":[url]});
    json!({
        "trackId": 1, "assetPresentation": "FULL", "audioMode": "STEREO", "audioQuality": "LOSSLESS",
        "manifestMimeType": "application/vnd.tidal.bts",
        "manifest": base64::engine::general_purpose::STANDARD.encode(manifest.to_string()),
        "manifestHash": "h", "bitDepth": 16, "sampleRate": 44100,
        "trackReplayGain": -8.1, "trackPeakAmplitude": 0.98
    })
}

fn encrypted_bts() -> serde_json::Value {
    let manifest =
        json!({"codecs":"flac","encryptionType":"OLD_AES","keyId":"k","urls":["https://cdn/x"]});
    json!({
        "audioQuality": "HI_RES_LOSSLESS",
        "manifestMimeType": "application/vnd.tidal.bts",
        "manifest": base64::engine::general_purpose::STANDARD.encode(manifest.to_string()),
    })
}

/// Responds with a sequence of templates, then repeats the last one.
struct Sequence {
    responses: std::sync::Mutex<std::collections::VecDeque<ResponseTemplate>>,
    last: ResponseTemplate,
}

impl Sequence {
    fn new(v: Vec<ResponseTemplate>) -> Self {
        let last = v.last().cloned().expect("at least one response");
        Self {
            responses: std::sync::Mutex::new(v.into()),
            last,
        }
    }
}

impl Respond for Sequence {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let mut q = self.responses.lock().unwrap();
        if q.len() > 1 {
            q.pop_front().unwrap()
        } else {
            q.front().cloned().unwrap_or_else(|| self.last.clone())
        }
    }
}

#[tokio::test]
async fn device_code_flow_polls_until_authorized_and_persists() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth2/device_authorization"))
        .and(body_string_contains("client_id=cid"))
        .and(body_string_contains("scope=r_usr+w_usr+w_sub"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "deviceCode": "dev", "userCode": "ABCDE", "verificationUri": "link.tidal.com",
            "verificationUriComplete": "link.tidal.com/ABCDE", "expiresIn": 300, "interval": 0
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth2/token"))
        .and(body_string_contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"))
        .and(body_string_contains("device_code=dev"))
        .respond_with(Sequence::new(vec![
            ResponseTemplate::new(400).set_body_json(json!({"error":"authorization_pending","error_description":"waiting"})),
            ResponseTemplate::new(400).set_body_json(json!({"error":"slow_down"})),
            ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "AT", "refresh_token": "RT", "token_type": "Bearer", "expires_in": 3600,
                "user": {"userId": 99, "countryCode": "NL"}
            })),
        ]))
        .expect(3)
        .mount(&server)
        .await;

    let store = Arc::new(MemoryTokenStore::default());
    let c = client(&server, store.clone(), false);
    let auth = start_device_flow(&c).await.unwrap();
    assert_eq!(auth.verification_url(), "https://link.tidal.com/ABCDE");
    let t = wait_for_device_token(&c, &auth, || true).await.unwrap();
    assert_eq!(t.access_token, "AT");
    assert_eq!(t.user_id, Some(99));
    assert_eq!(t.country_code.as_deref(), Some("NL"));
    let stored = store.load().unwrap().unwrap();
    assert_eq!(stored.access_token, "AT");
    assert!(c.is_logged_in().await);
}

#[tokio::test]
async fn non_device_client_id_is_explained() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth2/device_authorization"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "status": 400, "subStatus": 1002, "userMessage": "Client not a Limited Input Device client"
        })))
        .mount(&server)
        .await;
    let c = client(&server, Arc::new(MemoryTokenStore::default()), false);
    let err = start_device_flow(&c).await.unwrap_err();
    assert!(
        matches!(err, Error::Auth(ref m) if m.contains("device-code")),
        "{err}"
    );
}

#[tokio::test]
async fn expired_token_refreshes_once_before_request_and_keeps_old_refresh_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth2/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=rt"))
        .and(body_string_contains("client_secret=sec"))
        // Refresh response omits refresh_token: the stored one must survive.
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "new-at", "token_type": "Bearer", "expires_in": 3600
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/sessions"))
        .and(header("authorization", "Bearer new-at"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"sessionId":"s","userId":7,"countryCode":"NL"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let store = Arc::new(MemoryTokenStore::with(tokens(10)));
    let c = client(&server, store.clone(), true);
    let s = c.session().await.unwrap();
    assert_eq!(s.country_code, "NL");
    let saved = store.load().unwrap().unwrap();
    assert_eq!(saved.access_token, "new-at");
    assert_eq!(saved.refresh_token.as_deref(), Some("rt"));
}

#[tokio::test]
async fn auth_401_refreshes_once_and_retries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/sessions"))
        .respond_with(Sequence::new(vec![
            ResponseTemplate::new(401).set_body_json(
                json!({"status":401,"subStatus":11003,"userMessage":"The token has expired."}),
            ),
            ResponseTemplate::new(200).set_body_json(json!({"sessionId":"s","countryCode":"DE"})),
        ]))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth2/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"access_token":"fresh","refresh_token":"rt2","expires_in":3600}),
            ),
        )
        .expect(1)
        .mount(&server)
        .await;
    let c = client(
        &server,
        Arc::new(MemoryTokenStore::with(tokens(3600))),
        false,
    );
    let s = c.session().await.unwrap();
    assert_eq!(s.country_code, "DE");
    assert_eq!(c.tokens().await.unwrap().access_token, "fresh");
}

#[tokio::test]
async fn concurrent_401s_collapse_into_one_refresh() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/sessions"))
        .respond_with(|req: &Request| {
            let auth = req
                .headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth == "Bearer fresh" {
                ResponseTemplate::new(200)
                    .set_body_json(json!({"sessionId":"s","countryCode":"DE"}))
            } else {
                ResponseTemplate::new(401)
                    .set_body_json(json!({"status":401,"subStatus":11003,"userMessage":"expired"}))
            }
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth2/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token":"fresh","expires_in":3600})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let c = client(
        &server,
        Arc::new(MemoryTokenStore::with(tokens(3600))),
        false,
    );
    let results = futures_join(vec![c.clone(), c.clone(), c.clone(), c.clone()]).await;
    assert!(results.iter().all(|r| r.is_ok()), "{results:?}");
}

async fn futures_join(
    clients: Vec<ApiClient>,
) -> Vec<Result<streamboat_core::models::Session, Error>> {
    let handles: Vec<_> = clients
        .into_iter()
        .map(|c| tokio::spawn(async move { c.session().await }))
        .collect();
    let mut out = Vec::new();
    for h in handles {
        out.push(h.await.unwrap());
    }
    out
}

#[tokio::test]
async fn playback_substatus_on_401_does_not_refresh() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/5/playbackinfopostpaywall"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"status":401,"subStatus":4006,"userMessage":"privileges"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth2/token"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let c = client(
        &server,
        Arc::new(MemoryTokenStore::with(tokens(3600))),
        true,
    );
    let err = c
        .playback_info(5, AudioQuality::Lossless, "sid")
        .await
        .unwrap_err();
    match err {
        Error::Api(e) => {
            assert_eq!(e.sub_status, Some(4006));
            assert!(!e.is_terminal_playback());
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn refresh_4xx_wipes_credentials_but_5xx_keeps_them() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth2/token"))
        .respond_with(Sequence::new(vec![
            ResponseTemplate::new(503),
            ResponseTemplate::new(400).set_body_json(json!({"error":"invalid_grant"})),
        ]))
        .mount(&server)
        .await;
    let store = Arc::new(MemoryTokenStore::with(tokens(10)));
    let c = client(&server, store.clone(), false);
    assert!(matches!(c.session().await, Err(Error::Api(_))));
    assert!(store.load().unwrap().is_some(), "5xx must keep credentials");
    assert!(matches!(c.session().await, Err(Error::AuthRequired)));
    assert!(
        store.load().unwrap().is_none(),
        "4xx must clear credentials"
    );
    assert!(!c.is_logged_in().await);
}

#[tokio::test]
async fn rate_limit_arms_cooldown_and_propagates() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/sessions"))
        .respond_with(Sequence::new(vec![
            ResponseTemplate::new(429).insert_header("Retry-After", "1"),
            ResponseTemplate::new(200).set_body_json(json!({"countryCode":"NL"})),
        ]))
        .expect(2)
        .mount(&server)
        .await;
    let c = client(
        &server,
        Arc::new(MemoryTokenStore::with(tokens(3600))),
        false,
    );
    let err = c.session().await.unwrap_err();
    assert!(
        matches!(
            err,
            Error::RateLimited {
                retry_after_secs: 1
            }
        ),
        "{err}"
    );
    let t0 = std::time::Instant::now();
    c.session().await.unwrap();
    assert!(
        t0.elapsed() >= std::time::Duration::from_millis(900),
        "second call must wait out the gate"
    );
}

#[tokio::test]
async fn cascade_stops_at_first_playable_tier_and_skips_encrypted_hires() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/1/playbackinfopostpaywall"))
        .and(query_param("audioquality", "HI_RES_LOSSLESS"))
        .and(query_param("playbackmode", "STREAM"))
        .and(query_param("assetpresentation", "FULL"))
        .and(header("x-tidal-token", "cid"))
        .and(header("x-tidal-streamingsessionid", "sid-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(encrypted_bts()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/1/playbackinfopostpaywall"))
        .and(query_param("audioquality", "LOSSLESS"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bts("https://cdn/l.flac")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/1/playbackinfopostpaywall"))
        .and(query_param("audioquality", "HIGH"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let c = client(
        &server,
        Arc::new(MemoryTokenStore::with(tokens(3600))),
        true,
    );
    let r = c
        .resolve_stream(1, AudioQuality::HiResLossless, "sid-1")
        .await
        .unwrap();
    assert_eq!(r.source, StreamSource::Url("https://cdn/l.flac".into()));
    assert_eq!(r.info.quality, Some(AudioQuality::Lossless));
    assert_eq!(r.info.codec.as_deref(), Some("flac"));
    assert!(
        r.warnings.iter().any(|w| w.contains("refused")),
        "{:?}",
        r.warnings
    );
}

#[tokio::test]
async fn cascade_skips_hires_without_secret() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/1/playbackinfopostpaywall"))
        .and(query_param("audioquality", "HI_RES_LOSSLESS"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bts("https://cdn/h")))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/1/playbackinfopostpaywall"))
        .and(query_param("audioquality", "LOSSLESS"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bts("https://cdn/l")))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(
        &server,
        Arc::new(MemoryTokenStore::with(tokens(3600))),
        false,
    );
    let r = c
        .resolve_stream(1, AudioQuality::HiResLossless, "sid")
        .await
        .unwrap();
    assert_eq!(r.requested, AudioQuality::Lossless);
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("hi-res tiers skipped"))
    );
}

#[tokio::test]
async fn cascade_stops_on_terminal_substatus_and_on_rate_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/2/playbackinfopostpaywall"))
        .and(query_param("audioquality", "LOSSLESS"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"status":401,"subStatus":4032,"userMessage":"region"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/2/playbackinfopostpaywall"))
        .and(query_param("audioquality", "HIGH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bts("https://cdn/no")))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/3/playbackinfopostpaywall"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(
        &server,
        Arc::new(MemoryTokenStore::with(tokens(3600))),
        false,
    );
    let err = c
        .resolve_stream(2, AudioQuality::Lossless, "sid")
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Api(ref e) if e.sub_status == Some(4032)),
        "{err}"
    );
    let err = c
        .resolve_stream(3, AudioQuality::Lossless, "sid")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::RateLimited { .. }), "{err}");
}

#[tokio::test]
async fn cascade_continues_past_non_terminal_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/4/playbackinfopostpaywall"))
        .and(query_param("audioquality", "LOSSLESS"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/4/playbackinfopostpaywall"))
        .and(query_param("audioquality", "HIGH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bts("https://cdn/h.m4a")))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(
        &server,
        Arc::new(MemoryTokenStore::with(tokens(3600))),
        false,
    );
    let r = c
        .resolve_stream(4, AudioQuality::Lossless, "sid")
        .await
        .unwrap();
    assert_eq!(r.requested, AudioQuality::High);
}

#[tokio::test]
async fn track_and_search_tolerate_missing_fields() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/10"))
        .and(query_param("countryCode", "NL"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 10, "title": "Song", "duration": null, "artists": [{"id": 1, "name": "A"}, {"id": 2, "name": "B"}],
            "album": {"id": 3, "title": "Al", "cover": "c-o-v"}, "audioQuality": "HI_RES", "unknownField": true,
            "mediaMetadata": {"tags": ["LOSSLESS", "HIRES_LOSSLESS"]}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/search/tracks"))
        .and(query_param("query", "song"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"items": [{"id": 10, "title": "Song"}], "totalNumberOfItems": 1}),
        ))
        .mount(&server)
        .await;
    let c = client(
        &server,
        Arc::new(MemoryTokenStore::with(tokens(3600))),
        false,
    );
    let t = c.track(10).await.unwrap();
    assert_eq!(t.artist_names(), "A, B");
    assert_eq!(t.duration, None);
    assert_eq!(t.audio_quality, Some(AudioQuality::HiResLegacy));
    assert!(t.has_hires_master());
    let page = c.search_tracks("song", 5).await.unwrap();
    assert_eq!(page.items.len(), 1);
}
