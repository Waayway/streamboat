//! Pattern-based redaction for anything that reaches a log line or a crash
//! report (D-029): bearer tokens, `Authorization` headers, refresh tokens,
//! session ids, user ids, and the control API's bearer token. Built by
//! construction — every log/crash write path in this module routes through
//! [`redact`] — rather than by remembering to call it at each call site.
//!
//! This scrubs *values*, not structure: a redacted line keeps its shape
//! (`"userId": "<redacted>"`, `Authorization: Bearer <redacted>`) so a log
//! is still useful for debugging control flow and timing, just not for
//! recovering a credential.

use std::sync::LazyLock;

use regex::Regex;

/// One pattern plus which capture group to keep, which to hide, and what
/// replaces the hidden one.
struct Pattern {
    re: Regex,
    replacement: &'static str,
}

static PATTERNS: LazyLock<Vec<Pattern>> = LazyLock::new(|| {
    vec![
        // `Authorization: Bearer <token>` (any casing on the header name and
        // "Bearer") and a bare `Bearer <token>` with no header prefix (a log
        // line that only prints the header's value, not its name).
        Pattern {
            re: Regex::new(r"(?i)(bearer\s+)[A-Za-z0-9\-_.~+/=]+").unwrap(),
            replacement: "${1}<redacted>",
        },
        // `refreshToken`/`refresh_token`/`accessToken`/`access_token` as a
        // JSON field, a form field, or a `key=value` log line.
        Pattern {
            re: Regex::new(
                r#"(?i)("?(?:refresh|access)[_-]?token"?\s*[:=]\s*"?)[A-Za-z0-9\-_.~+/=]+"#,
            )
            .unwrap(),
            replacement: "${1}<redacted>",
        },
        // `sessionId`/`session_id`/`streamingSessionId`/`x-tidal-streamingsessionid`.
        Pattern {
            re: Regex::new(
                r#"(?i)("?(?:x-tidal-)?(?:streaming[_-]?)?session[_-]?id"?\s*[:=]\s*"?)[A-Za-z0-9\-]+"#,
            )
            .unwrap(),
            replacement: "${1}<redacted>",
        },
        // `userId`/`user_id` (numeric TIDAL account id).
        Pattern {
            re: Regex::new(r#"(?i)("?user[_-]?id"?\s*[:=]\s*"?)[0-9]+"#).unwrap(),
            replacement: "${1}<redacted>",
        },
        // The control API's own bearer token, by name (D-030) — the value
        // itself is also caught by the generic 64-hex-char pattern below,
        // this one just keeps the field name legible.
        Pattern {
            re: Regex::new(r#"(?i)("?control[_-]?token"?\s*[:=]\s*"?)[A-Za-z0-9]+"#).unwrap(),
            replacement: "${1}<redacted>",
        },
        // A bare 64-hex-char run — the control token's and every SHA-256's
        // shape — with no labelled key nearby. Deliberately broad: it also
        // catches `manifest_hash`, which is not secret, but over-redacting a
        // hash costs nothing a debug session needs.
        Pattern {
            re: Regex::new(r"\b[0-9a-fA-F]{64}\b").unwrap(),
            replacement: "<redacted-hex64>",
        },
    ]
});

/// Redacts every known secret pattern in `line`, in order, and returns the
/// result. Idempotent (redacting an already-redacted line is a no-op) and
/// safe on plain text (nothing matches, the string comes back unchanged).
pub fn redact(line: &str) -> String {
    let mut out = line.to_string();
    for pattern in PATTERNS.iter() {
        out = pattern
            .re
            .replace_all(&out, pattern.replacement)
            .into_owned();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_a_bearer_authorization_header() {
        let line = r#"sending GET /v1/sessions Authorization: Bearer abcDEF123.456-_~+/="#;
        let out = redact(line);
        assert!(!out.contains("abcDEF123"));
        assert!(out.contains("Authorization: Bearer <redacted>"));
    }

    #[test]
    fn masks_a_bare_bearer_token_with_no_header_name() {
        let out = redact("token: Bearer xyz987");
        assert!(!out.contains("xyz987"));
        assert!(out.contains("Bearer <redacted>"));
    }

    #[test]
    fn masks_refresh_and_access_tokens_in_json() {
        let line = r#"{"refreshToken":"rt_secret_value","accessToken":"at_secret_value"}"#;
        let out = redact(line);
        assert!(!out.contains("rt_secret_value"));
        assert!(!out.contains("at_secret_value"));
        assert!(out.contains(r#""refreshToken":"<redacted>"#));
        assert!(out.contains(r#""accessToken":"<redacted>"#));
    }

    #[test]
    fn masks_session_ids_including_the_streaming_session_header() {
        let line = r#"x-tidal-streamingsessionid: 9f1c2e3a-uuid-like-value session_id=plain-one"#;
        let out = redact(line);
        assert!(!out.contains("9f1c2e3a-uuid-like-value"));
        assert!(!out.contains("plain-one"));
    }

    #[test]
    fn masks_user_ids() {
        let out = redact(r#""userId": 123456789"#);
        assert!(!out.contains("123456789"));
        assert!(out.contains("<redacted>"));
    }

    #[test]
    fn masks_the_control_token_by_name_and_by_shape() {
        let hex64 = "a".repeat(64);
        let named = redact(&format!("control_token={hex64}"));
        assert!(!named.contains(&hex64));
        let bare = redact(&format!("bearer token in file: {hex64}"));
        assert!(!bare.contains(&hex64));
    }

    #[test]
    fn plain_text_is_left_untouched() {
        let line = "player: track 12345 started at 0ms, quality LOSSLESS";
        assert_eq!(redact(line), line);
    }

    #[test]
    fn redaction_is_idempotent() {
        let line = "Authorization: Bearer supersecrettoken1234";
        let once = redact(line);
        let twice = redact(&once);
        assert_eq!(once, twice);
    }
}
