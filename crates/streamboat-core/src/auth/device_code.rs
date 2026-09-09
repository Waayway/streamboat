//! Device-code flow against `auth.tidal.com/v1/oauth2/`.
//! Gotchas encoded here (`tidal-api` auth §1): the response is camelCase,
//! `verificationUriComplete` has no scheme, 400 + `authorization_pending` /
//! `slow_down` means keep polling, and a non-device client id answers with
//! "not a Limited Input Device" / subStatus 1002.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::SCOPES;
use crate::error::{Error, Result};
use crate::http::{ApiClient, TokenResponse, parse_error_body};
use crate::token_store::{AuthFlow, TokenSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    5
}

impl DeviceAuthorization {
    /// The URL to show the user, with the scheme TIDAL leaves off.
    pub fn verification_url(&self) -> String {
        let raw = self
            .verification_uri_complete
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.verification_uri);
        if raw.starts_with("http://") || raw.starts_with("https://") {
            raw.to_string()
        } else {
            format!("https://{raw}")
        }
    }
}

#[derive(Debug)]
pub enum PollOutcome {
    Pending,
    /// The server asked us to back off; the interval has been increased.
    SlowDown,
    Authorized(TokenSet),
}

pub async fn start_device_flow(client: &ApiClient) -> Result<DeviceAuthorization> {
    let pair = client
        .credentials()
        .device_pair()
        .ok_or(Error::NoCredentials)?;
    let mut form = vec![
        ("client_id", pair.id.clone()),
        ("scope", SCOPES.to_string()),
    ];
    if let Some(secret) = &pair.secret {
        form.push(("client_secret", secret.clone()));
    }
    let (status, text) = client
        .post_auth_form("v1/oauth2/device_authorization", &form)
        .await?;
    if !status.is_success() {
        let api = parse_error_body(status.as_u16(), &text);
        return Err(classify_auth_error(api.sub_status, &api.message));
    }
    serde_json::from_str(&text)
        .map_err(|e| Error::Auth(format!("unexpected device_authorization response: {e}")))
}

pub async fn poll_device_token_once(
    client: &ApiClient,
    auth: &DeviceAuthorization,
) -> Result<PollOutcome> {
    let pair = client
        .credentials()
        .device_pair()
        .ok_or(Error::NoCredentials)?;
    let mut form = vec![
        ("client_id", pair.id.clone()),
        ("device_code", auth.device_code.clone()),
        (
            "grant_type",
            "urn:ietf:params:oauth:grant-type:device_code".to_string(),
        ),
        ("scope", SCOPES.to_string()),
    ];
    if let Some(secret) = &pair.secret {
        form.push(("client_secret", secret.clone()));
    }
    let (status, text) = client.post_auth_form("v1/oauth2/token", &form).await?;
    if status.is_success() {
        let resp: TokenResponse = serde_json::from_str(&text)
            .map_err(|e| Error::Auth(format!("unexpected token response: {e}")))?;
        return Ok(PollOutcome::Authorized(resp.into_token_set(
            &pair.id,
            AuthFlow::DeviceCode,
            None,
            None,
        )));
    }
    let api = parse_error_body(status.as_u16(), &text);
    let msg = api.message.to_ascii_lowercase();
    if msg.contains("authorization_pending") {
        return Ok(PollOutcome::Pending);
    }
    if msg.contains("slow_down") {
        return Ok(PollOutcome::SlowDown);
    }
    if msg.contains("expired_token") || msg.contains("expired") {
        return Err(Error::Auth(
            "the login code expired before it was used; run login again".into(),
        ));
    }
    Err(classify_auth_error(api.sub_status, &api.message))
}

fn classify_auth_error(sub_status: Option<u32>, message: &str) -> Error {
    if sub_status == Some(1002)
        || message
            .to_ascii_lowercase()
            .contains("not a limited input device")
    {
        return Error::Auth(
            "this client id is not registered for the device-code flow (TIDAL says it is not a \
             Limited Input Device client); it is probably a web-player id. Use a device-flow client id"
                .into(),
        );
    }
    Error::Auth(message.to_string())
}

/// Poll until the user finishes in the browser, the code expires, or
/// `should_continue` returns false. `on_wait` is called before each sleep so a
/// CLI can show a spinner.
pub async fn wait_for_device_token(
    client: &ApiClient,
    auth: &DeviceAuthorization,
    mut should_continue: impl FnMut() -> bool,
) -> Result<TokenSet> {
    let start = tokio::time::Instant::now();
    let mut interval = auth.interval.max(1);
    loop {
        if start.elapsed() > Duration::from_secs(auth.expires_in) {
            return Err(Error::Auth(
                "the login code expired before it was used; run login again".into(),
            ));
        }
        if !should_continue() {
            return Err(Error::Auth("login cancelled".into()));
        }
        match poll_device_token_once(client, auth).await? {
            PollOutcome::Authorized(tokens) => {
                client.set_tokens(tokens.clone()).await?;
                return Ok(tokens);
            }
            PollOutcome::SlowDown => interval += 5,
            PollOutcome::Pending => {}
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_url_gets_a_scheme() {
        let a = DeviceAuthorization {
            device_code: "d".into(),
            user_code: "ABCDE".into(),
            verification_uri: "link.tidal.com".into(),
            verification_uri_complete: Some("link.tidal.com/ABCDE".into()),
            expires_in: 300,
            interval: 2,
        };
        assert_eq!(a.verification_url(), "https://link.tidal.com/ABCDE");
    }

    #[test]
    fn camel_case_response_parses() {
        let a: DeviceAuthorization = serde_json::from_str(
            r#"{"deviceCode":"d","userCode":"U","verificationUri":"link.tidal.com","verificationUriComplete":"link.tidal.com/U","expiresIn":300,"interval":2}"#,
        )
        .unwrap();
        assert_eq!(a.interval, 2);
        assert_eq!(a.user_code, "U");
    }
}
