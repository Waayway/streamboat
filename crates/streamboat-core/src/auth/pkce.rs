//! PKCE / authorization-code flow (RFC 7636) against `login.tidal.com`
//! (`tidal-api` auth §2, §9, §13).
//!
//! The ecosystem PKCE client id only accepts TIDAL's own redirect target,
//! `https://tidal.com/android/login/auth`, a page that does not exist; the
//! client captures the URL the browser lands on. streamboat supports two
//! capture mechanisms here: the user pastes the URL (works everywhere,
//! headless included) or a loopback HTTP listener receives it (only with a
//! client id whose registration allows a loopback redirect, which is
//! unverified for the ecosystem id). A `streamboat://` handler is a third
//! mechanism for the desktop shell; `tidal://` is never claimed (auth §13).

use std::net::SocketAddr;
use std::time::Duration;

use base64::Engine as _;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use zeroize::Zeroizing;

use super::PKCE_SCOPES;
use crate::error::{Error, Result};
use crate::http::{ApiClient, TokenResponse, parse_error_body};
use crate::token_store::{AuthFlow, TokenSet};

pub const DEFAULT_REDIRECT_URI: &str = "https://tidal.com/android/login/auth";

/// 32 random bytes, base64url without padding (43 chars).
pub fn generate_verifier() -> Zeroizing<String> {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    Zeroizing::new(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// `base64url(sha256(verifier))`, unpadded (RFC 7636 §4.2).
pub fn challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub struct PkceSession {
    pub authorize_url: String,
    pub redirect_uri: String,
    pub client_id: String,
    pub client_unique_key: String,
    verifier: Zeroizing<String>,
}

impl std::fmt::Debug for PkceSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkceSession")
            .field("authorize_url", &self.authorize_url)
            .field("redirect_uri", &self.redirect_uri)
            .field("client_id", &self.client_id)
            .field("client_unique_key", &self.client_unique_key)
            .field("verifier", &"<redacted>")
            .finish()
    }
}

impl PkceSession {
    /// Build the authorize URL. `client_unique_key` is the persistent device
    /// identity (`DeviceIdentity`), sent identically on authorize, exchange
    /// and refresh.
    pub fn start(
        client: &ApiClient,
        client_unique_key: &str,
        redirect_uri: Option<&str>,
    ) -> Result<Self> {
        let pair = client
            .credentials()
            .pkce_pair()
            .ok_or(Error::NoCredentials)?;
        let verifier = generate_verifier();
        let redirect_uri = redirect_uri
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_REDIRECT_URI)
            .to_string();
        let mut url = client.login_base().join("authorize").unwrap();
        // Same parameter set and order as python-tidal and Sone.
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("client_id", &pair.id)
            .append_pair("lang", "EN")
            .append_pair("appMode", "android")
            .append_pair("client_unique_key", client_unique_key)
            .append_pair("code_challenge", &challenge(&verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("restrict_signup", "true");
        Ok(Self {
            authorize_url: url.to_string(),
            redirect_uri,
            client_id: pair.id.clone(),
            client_unique_key: client_unique_key.to_string(),
            verifier,
        })
    }

    /// Exchange the authorization code, install and persist the tokens.
    pub async fn exchange(&self, client: &ApiClient, code: &str) -> Result<TokenSet> {
        let form = vec![
            ("code", code.trim().to_string()),
            ("client_id", self.client_id.clone()),
            ("grant_type", "authorization_code".to_string()),
            ("redirect_uri", self.redirect_uri.clone()),
            ("scope", PKCE_SCOPES.to_string()),
            ("code_verifier", self.verifier.to_string()),
            ("client_unique_key", self.client_unique_key.clone()),
        ];
        let (status, text) = client.post_auth_form("v1/oauth2/token", &form).await?;
        if !status.is_success() {
            let api = parse_error_body(status.as_u16(), &text);
            return Err(Error::Auth(format!("token exchange failed: {api}")));
        }
        let resp: TokenResponse = serde_json::from_str(&text)
            .map_err(|e| Error::Auth(format!("unexpected token response: {e}")))?;
        let tokens = resp.into_token_set(
            &self.client_id,
            AuthFlow::Pkce,
            Some(&self.client_unique_key),
            None,
        );
        client.set_tokens(tokens.clone()).await?;
        Ok(tokens)
    }
}

/// Pull the authorization code out of whatever the user pasted: the full
/// redirected URL, just its query string, or the bare code.
pub fn code_from_redirect(input: &str) -> Result<String> {
    let s = input.trim();
    if s.is_empty() {
        return Err(Error::Auth("nothing pasted".into()));
    }
    let candidate = if s.contains("://") {
        s.to_string()
    } else if s.starts_with('?') || s.contains("code=") {
        format!(
            "https://placeholder.invalid/{}",
            if s.starts_with('?') {
                s.to_string()
            } else {
                format!("?{s}")
            }
        )
    } else if !s.contains(['/', ' ', '&', '=']) {
        return Ok(s.to_string());
    } else {
        return Err(Error::Auth(
            "that does not look like a redirect URL or a code".into(),
        ));
    };
    let url = url::Url::parse(&candidate)
        .map_err(|e| Error::Auth(format!("cannot parse the pasted URL: {e}")))?;
    let mut code = None;
    for (k, v) in url.query_pairs() {
        match &*k {
            "code" => code = Some(v.to_string()),
            "error" => {
                return Err(Error::Auth(format!(
                    "TIDAL returned an error instead of a code: {v}{}",
                    url.query_pairs()
                        .find(|(k, _)| k == "error_description")
                        .map(|(_, d)| format!(" ({d})"))
                        .unwrap_or_default()
                )));
            }
            _ => {}
        }
    }
    code.filter(|c| !c.is_empty())
        .ok_or_else(|| Error::Auth("the pasted URL carries no `code` parameter".into()))
}

/// Listen on `addr` for the browser's redirect to `path` and return the
/// code. Only ever binds loopback.
pub async fn capture_code_loopback(
    addr: SocketAddr,
    path: &str,
    timeout: Duration,
) -> Result<String> {
    if !addr.ip().is_loopback() {
        return Err(Error::Config(
            "the redirect listener only binds loopback addresses".into(),
        ));
    }
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| Error::Config(format!("cannot listen on {addr}: {e}")))?;
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(Error::Auth(
                "timed out waiting for the browser redirect".into(),
            ));
        }
        let (mut stream, _) = match tokio::time::timeout(remaining, listener.accept()).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => return Err(Error::Config(format!("accept: {e}"))),
            Err(_) => {
                return Err(Error::Auth(
                    "timed out waiting for the browser redirect".into(),
                ));
            }
        };
        let mut buf = vec![0u8; 8192];
        let n = match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await {
            Ok(Ok(n)) => n,
            _ => continue,
        };
        let head = String::from_utf8_lossy(&buf[..n]);
        let Some(request_line) = head.lines().next() else {
            continue;
        };
        let target = request_line.split_whitespace().nth(1).unwrap_or("/");
        let (req_path, query) = target.split_once('?').unwrap_or((target, ""));
        if req_path != path {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
            continue;
        }
        let result = code_from_redirect(&format!("?{query}"));
        let (status, body) = match &result {
            Ok(_) => (
                "200 OK",
                "<!doctype html><title>streamboat</title><p>Logged in. You can close this window.",
            ),
            Err(_) => (
                "400 Bad Request",
                "<!doctype html><title>streamboat</title><p>Login failed; see the terminal.",
            ),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        return result;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc7636_appendix_b_vector() {
        // https://www.rfc-editor.org/rfc/rfc7636#appendix-B
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_shape() {
        let v = generate_verifier();
        assert_eq!(v.len(), 43);
        assert!(
            v.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn code_extraction_accepts_url_query_and_bare_code() {
        assert_eq!(
            code_from_redirect("https://tidal.com/android/login/auth?code=abc.def&state=x")
                .unwrap(),
            "abc.def"
        );
        assert_eq!(code_from_redirect("?code=xyz").unwrap(), "xyz");
        assert_eq!(code_from_redirect("code=xyz").unwrap(), "xyz");
        assert_eq!(
            code_from_redirect("  bare-code_123  ").unwrap(),
            "bare-code_123"
        );
        let err = code_from_redirect(
            "https://tidal.com/android/login/auth?error=access_denied&error_description=nope",
        )
        .unwrap_err();
        assert!(matches!(err, Error::Auth(m) if m.contains("access_denied") && m.contains("nope")));
        assert!(code_from_redirect("https://tidal.com/android/login/auth").is_err());
        assert!(code_from_redirect("").is_err());
    }

    #[tokio::test]
    async fn loopback_capture_returns_the_code_and_ignores_other_paths() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TcpListener::bind(addr).await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let capture = tokio::spawn(capture_code_loopback(
            addr,
            "/callback",
            Duration::from_secs(10),
        ));
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        s.write_all(b"GET /favicon.ico HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let mut buf = String::new();
        let _ = s.read_to_string(&mut buf).await;
        assert!(buf.starts_with("HTTP/1.1 404"));
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        s.write_all(b"GET /callback?code=the-code&x=1 HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let mut buf = String::new();
        let _ = s.read_to_string(&mut buf).await;
        assert!(buf.starts_with("HTTP/1.1 200"), "{buf}");
        assert_eq!(capture.await.unwrap().unwrap(), "the-code");
    }

    #[tokio::test]
    async fn loopback_capture_refuses_non_loopback() {
        let err =
            capture_code_loopback("0.0.0.0:0".parse().unwrap(), "/cb", Duration::from_secs(1))
                .await
                .unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }
}
