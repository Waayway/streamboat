//! The authenticated HTTP client: bearer tokens, proactive and reactive
//! refresh behind a single-flight lock, the global 429 cooldown gate, and the
//! two error-body shapes. Base URLs are injectable so tests run against an
//! in-process server.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tokio::sync::RwLock;
use url::Url;

use crate::credentials::ClientCredentials;
use crate::error::{ApiError, Error, Result};
use crate::token_store::{now_secs, TokenSet, TokenStore};
use crate::{PROJECT_URL, VERSION};

pub const DEFAULT_API_BASE: &str = "https://api.tidal.com/";
pub const DEFAULT_AUTH_BASE: &str = "https://auth.tidal.com/";
/// TIDAL's own SDK refreshes 60 s before expiry against server time.
const REFRESH_MARGIN_SECS: u64 = 60;

pub struct ApiClientBuilder {
    creds: ClientCredentials,
    store: Arc<dyn TokenStore>,
    api_base: Url,
    auth_base: Url,
    user_agent: Option<String>,
    country_code: Option<String>,
    timeout: Duration,
}

impl ApiClientBuilder {
    pub fn api_base(mut self, url: &str) -> Self {
        self.api_base = Url::parse(url).expect("valid api base url");
        self
    }
    pub fn auth_base(mut self, url: &str) -> Self {
        self.auth_base = Url::parse(url).expect("valid auth base url");
        self
    }
    /// A documented compatibility override; `None` keeps the honest UA (D-025).
    pub fn user_agent_override(mut self, ua: Option<String>) -> Self {
        self.user_agent = ua.filter(|s| !s.trim().is_empty());
        self
    }
    pub fn country_code(mut self, cc: Option<String>) -> Self {
        self.country_code = cc.filter(|s| !s.trim().is_empty());
        self
    }
    pub fn timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    pub fn build(self) -> Result<ApiClient> {
        let ua = self
            .user_agent
            .unwrap_or_else(|| format!("streamboat/{VERSION} (+{PROJECT_URL})"));
        let http = reqwest::Client::builder()
            .user_agent(&ua)
            .timeout(self.timeout)
            .build()?;
        let tokens = self.store.load()?;
        Ok(ApiClient {
            inner: Arc::new(Inner {
                http,
                api_base: self.api_base,
                auth_base: self.auth_base,
                creds: self.creds,
                store: self.store,
                tokens: RwLock::new(tokens),
                refresh_lock: tokio::sync::Mutex::new(()),
                rate_gate: Mutex::new(None),
                country_code: Mutex::new(self.country_code),
                user_agent: ua,
            }),
        })
    }
}

struct Inner {
    http: reqwest::Client,
    api_base: Url,
    auth_base: Url,
    creds: ClientCredentials,
    store: Arc<dyn TokenStore>,
    tokens: RwLock<Option<TokenSet>>,
    refresh_lock: tokio::sync::Mutex<()>,
    rate_gate: Mutex<Option<Instant>>,
    country_code: Mutex<Option<String>>,
    user_agent: String,
}

#[derive(Clone)]
pub struct ApiClient {
    inner: Arc<Inner>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: Option<String>,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
    pub user_id: Option<u64>,
    pub user: Option<crate::models::TokenUser>,
}

impl TokenResponse {
    pub(crate) fn into_token_set(self, client_id: &str, previous: Option<&TokenSet>) -> TokenSet {
        TokenSet {
            access_token: self.access_token,
            refresh_token: self
                .refresh_token
                .or_else(|| previous.and_then(|p| p.refresh_token.clone())),
            token_type: self.token_type.unwrap_or_else(|| "Bearer".into()),
            expires_at: now_secs().saturating_add(self.expires_in.unwrap_or(3600)),
            scope: self
                .scope
                .or_else(|| previous.map(|p| p.scope.clone()))
                .unwrap_or_default(),
            client_id: client_id.to_string(),
            user_id: self
                .user_id
                .or(self.user.as_ref().and_then(|u| u.user_id))
                .or(previous.and_then(|p| p.user_id)),
            country_code: self
                .user
                .as_ref()
                .and_then(|u| u.country_code.clone())
                .or_else(|| previous.and_then(|p| p.country_code.clone())),
        }
    }
}

/// Parse either error-body shape; falls back to the raw text.
pub(crate) fn parse_error_body(status: u16, text: &str) -> ApiError {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct V1 {
        sub_status: Option<serde_json::Value>,
        user_message: Option<String>,
        error: Option<String>,
        #[serde(rename = "error_description")]
        error_description: Option<String>,
    }
    #[derive(Deserialize)]
    struct Detail {
        detail: Option<String>,
        code: Option<String>,
    }
    #[derive(Deserialize)]
    struct V2 {
        errors: Vec<Detail>,
    }
    if let Ok(v1) = serde_json::from_str::<V1>(text) {
        let sub_status = v1.sub_status.and_then(|v| match v {
            serde_json::Value::Number(n) => n.as_f64().map(|f| f as u32),
            serde_json::Value::String(s) => s.parse::<f64>().ok().map(|f| f as u32),
            _ => None,
        });
        let message = v1
            .user_message
            .or_else(|| match (v1.error, v1.error_description) {
                (Some(e), Some(d)) => Some(format!("{e}: {d}")),
                (Some(e), None) => Some(e),
                (None, Some(d)) => Some(d),
                (None, None) => None,
            })
            .unwrap_or_default();
        if sub_status.is_some() || !message.is_empty() {
            return ApiError { status, sub_status, message };
        }
    }
    if let Ok(v2) = serde_json::from_str::<V2>(text) {
        let message = v2
            .errors
            .into_iter()
            .map(|d| match (d.code, d.detail) {
                (Some(c), Some(t)) => format!("{c}: {t}"),
                (_, Some(t)) => t,
                (Some(c), None) => c,
                _ => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
        return ApiError { status, sub_status: None, message };
    }
    let mut message = text.trim().to_string();
    if message.len() > 300 {
        message.truncate(300);
        message.push('…');
    }
    ApiError { status, sub_status: None, message }
}

fn retry_after_secs(headers: &reqwest::header::HeaderMap) -> u64 {
    // Delta-seconds only; the HTTP-date form is rejected, not mis-parsed.
    // Clamped to [1, 120], default 5 (Sone's RateGate).
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|s| s.clamp(1, 120))
        .unwrap_or(5)
}

impl ApiClient {
    pub fn builder(creds: ClientCredentials, store: Arc<dyn TokenStore>) -> ApiClientBuilder {
        ApiClientBuilder {
            creds,
            store,
            api_base: Url::parse(DEFAULT_API_BASE).unwrap(),
            auth_base: Url::parse(DEFAULT_AUTH_BASE).unwrap(),
            user_agent: None,
            country_code: None,
            timeout: Duration::from_secs(30),
        }
    }

    pub fn credentials(&self) -> &ClientCredentials {
        &self.inner.creds
    }

    pub fn user_agent(&self) -> &str {
        &self.inner.user_agent
    }

    pub fn api_base(&self) -> &Url {
        &self.inner.api_base
    }

    pub fn auth_base(&self) -> &Url {
        &self.inner.auth_base
    }

    pub async fn tokens(&self) -> Option<TokenSet> {
        self.inner.tokens.read().await.clone()
    }

    pub async fn is_logged_in(&self) -> bool {
        self.inner.tokens.read().await.is_some()
    }

    /// Install freshly minted tokens and persist them.
    pub async fn set_tokens(&self, tokens: TokenSet) -> Result<()> {
        self.inner.store.save(&tokens)?;
        if let Some(cc) = tokens.country_code.clone() {
            self.set_country_code(cc);
        }
        *self.inner.tokens.write().await = Some(tokens);
        Ok(())
    }

    /// Forget tokens locally. (Server-side revocation is best-effort and
    /// separate; a headless box may be offline.)
    pub async fn logout(&self) -> Result<()> {
        self.inner.store.clear()?;
        *self.inner.tokens.write().await = None;
        Ok(())
    }

    pub fn country_code_hint(&self) -> Option<String> {
        self.inner.country_code.lock().unwrap().clone()
    }

    pub fn set_country_code(&self, cc: String) {
        *self.inner.country_code.lock().unwrap() = Some(cc);
    }

    /// Wait out an armed 429 cooldown.
    async fn wait_rate_gate(&self) {
        let deadline = *self.inner.rate_gate.lock().unwrap();
        if let Some(d) = deadline {
            let now = Instant::now();
            if d > now {
                tokio::time::sleep(d - now).await;
            }
        }
    }

    fn arm_rate_gate(&self, secs: u64) {
        let mut g = self.inner.rate_gate.lock().unwrap();
        let new = Instant::now() + Duration::from_secs(secs);
        // Concurrent 429s can only lengthen the cooldown.
        *g = Some(g.map_or(new, |old| old.max(new)));
    }

    /// A valid access token, refreshing first if it is about to expire.
    pub async fn access_token(&self) -> Result<String> {
        let stale = match self.inner.tokens.read().await.as_ref() {
            None => return Err(Error::AuthRequired),
            Some(t) if t.expires_within(REFRESH_MARGIN_SECS) => Some(t.access_token.clone()),
            Some(_) => None,
        };
        if let Some(seen) = stale {
            self.refresh_if_still(Some(&seen)).await?;
        }
        self.inner
            .tokens
            .read()
            .await
            .as_ref()
            .map(|t| t.access_token.clone())
            .ok_or(Error::AuthRequired)
    }

    /// Refresh under a single-flight lock: concurrent callers wait, then see
    /// the already-refreshed token and return without a second call.
    pub async fn refresh_tokens(&self) -> Result<()> {
        self.refresh_if_still(None).await
    }

    /// Refresh unless the access token changed since the caller observed
    /// `seen_access_token` (another waiter already refreshed).
    async fn refresh_if_still(&self, seen_access_token: Option<&str>) -> Result<()> {
        let _guard = self.inner.refresh_lock.lock().await;
        let current = match self.inner.tokens.read().await.clone() {
            None => return Err(Error::AuthRequired),
            Some(t) => t,
        };
        if let Some(seen) = seen_access_token {
            if current.access_token != seen {
                return Ok(());
            }
        }
        let refresh_token = current.refresh_token.clone().ok_or_else(|| {
            Error::Auth("no refresh token stored; log in again".into())
        })?;
        let mut form: Vec<(&str, String)> = vec![
            ("grant_type", "refresh_token".into()),
            ("refresh_token", refresh_token),
            ("client_id", self.inner.creds.client_id.clone()),
            ("scope", current.scope.clone()),
        ];
        if let Some(secret) = &self.inner.creds.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        let url = self.inner.auth_base.join("v1/oauth2/token").unwrap();
        let resp = match self.inner.http.post(url).form(&form).send().await {
            Ok(r) => r,
            // Offline: keep the credentials, report the network error.
            Err(e) => return Err(Error::Network(e)),
        };
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if status.is_success() {
            let parsed: TokenResponse = serde_json::from_str(&text)?;
            let new = parsed.into_token_set(&self.inner.creds.client_id, Some(&current));
            self.inner.store.save(&new)?;
            *self.inner.tokens.write().await = Some(new);
            return Ok(());
        }
        let api = parse_error_body(status.as_u16(), &text);
        if status.is_client_error() {
            // Fatal: the refresh token is dead. Wipe and force re-login.
            tracing::warn!(%api, "token refresh rejected; clearing credentials");
            self.inner.store.clear()?;
            *self.inner.tokens.write().await = None;
            return Err(Error::AuthRequired);
        }
        // 5xx: retryable, keep credentials.
        Err(Error::Api(api))
    }

    /// Authenticated GET against the API base. Handles the cooldown gate,
    /// one refresh-and-retry on a genuine auth 401, and both error shapes.
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
        headers: &[(&str, String)],
    ) -> Result<T> {
        let url = self
            .inner
            .api_base
            .join(path.trim_start_matches('/'))
            .map_err(|e| Error::Config(format!("bad path {path:?}: {e}")))?;
        let mut refreshed = false;
        loop {
            self.wait_rate_gate().await;
            let token = self.access_token().await?;
            let mut req = self.inner.http.get(url.clone()).query(query).bearer_auth(&token);
            for (k, v) in headers {
                req = req.header(*k, v);
            }
            let resp = req.send().await?;
            let status = resp.status();
            if status.is_success() {
                let text = resp.text().await?;
                return serde_json::from_str::<T>(&text).map_err(|e| {
                    Error::Manifest(format!("unexpected response shape from {path}: {e}"))
                });
            }
            if status == StatusCode::TOO_MANY_REQUESTS {
                let secs = retry_after_secs(resp.headers());
                self.arm_rate_gate(secs);
                return Err(Error::RateLimited { retry_after_secs: secs });
            }
            let text = resp.text().await.unwrap_or_default();
            let api = parse_error_body(status.as_u16(), &text);
            if status == StatusCode::UNAUTHORIZED && !api.is_playback_sub_status() && !refreshed {
                // A real auth failure: refresh once and retry once.
                refreshed = true;
                self.refresh_if_still(Some(&token)).await?;
                continue;
            }
            if api.is_auth_sub_status() && !refreshed {
                refreshed = true;
                self.refresh_if_still(Some(&token)).await?;
                continue;
            }
            return Err(Error::Api(api));
        }
    }

    /// Unauthenticated POST of a form to the auth host (device flow, refresh).
    pub(crate) async fn post_auth_form(
        &self,
        path: &str,
        form: &[(&str, String)],
    ) -> Result<(StatusCode, String)> {
        self.wait_rate_gate().await;
        let url = self.inner.auth_base.join(path.trim_start_matches('/')).unwrap();
        let resp = self.inner.http.post(url).form(form).send().await?;
        let status = resp.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            let secs = retry_after_secs(resp.headers());
            self.arm_rate_gate(secs);
            return Err(Error::RateLimited { retry_after_secs: secs });
        }
        let text = resp.text().await.unwrap_or_default();
        Ok((status, text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v1_error_with_float_substatus() {
        let e = parse_error_body(401, r#"{"status":401,"subStatus":4005.0,"userMessage":"nope"}"#);
        assert_eq!(e.sub_status, Some(4005));
        assert_eq!(e.message, "nope");
        assert!(e.is_terminal_playback());
    }

    #[test]
    fn parses_v2_error_shape() {
        let e = parse_error_body(404, r#"{"errors":[{"detail":"gone","code":"NOT_FOUND"}]}"#);
        assert_eq!(e.message, "NOT_FOUND: gone");
        assert_eq!(e.sub_status, None);
    }

    #[test]
    fn parses_oauth_error_shape() {
        let e = parse_error_body(400, r#"{"error":"authorization_pending","error_description":"x"}"#);
        assert_eq!(e.message, "authorization_pending: x");
    }

    #[test]
    fn recoverable_substatuses_are_not_terminal() {
        for s in [4006, 4033] {
            let e = ApiError { status: 401, sub_status: Some(s), message: String::new() };
            assert!(!e.is_terminal_playback(), "{s}");
            assert!(e.is_playback_sub_status());
        }
    }
}
