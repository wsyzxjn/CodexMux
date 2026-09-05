use anyhow::{Context, Result};
use http::{HeaderMap, HeaderName, HeaderValue, header};

const OFFICIAL_PASSTHROUGH_HEADERS: &[&str] = &[
    "accept",
    "user-agent",
    "originator",
    "session_id",
    "conversation_id",
    "openai-beta",
    "x-codex-turn-metadata",
    "x-openai-internal-codex-responses-lite",
    "x-codex-imagegen-request-id",
];
const CPA_PASSTHROUGH_HEADERS: &[&str] = &["accept", "user-agent"];

pub fn official_headers(incoming: &HeaderMap) -> Result<HeaderMap> {
    let mut output = selected_headers(incoming, OFFICIAL_PASSTHROUGH_HEADERS);
    let authorization = incoming
        .get(header::AUTHORIZATION)
        .context("official route requires the incoming Codex OAuth Authorization header")?
        .clone();
    output.insert(header::AUTHORIZATION, authorization);
    for name in [
        "chatgpt-account-id",
        "openai-organization",
        "openai-project",
    ] {
        if let Some(value) = incoming.get(name) {
            output.insert(HeaderName::from_static(name), value.clone());
        }
    }
    Ok(output)
}

/// Headers for the fixed official image endpoints. Credential handling is
/// identical to `official_headers`; only the caller's `content-type` is
/// preserved, because an image edit may arrive as `multipart/form-data` and
/// its body is forwarded byte for byte.
pub fn official_image_headers(incoming: &HeaderMap) -> Result<HeaderMap> {
    Ok(preserve_content_type(official_headers(incoming)?, incoming))
}

/// Same as `cpa_headers` for an image request pinned to a CPA or direct
/// upstream: the caller's `content-type` is preserved and only that route's
/// own token is attached.
pub fn cpa_image_headers(incoming: &HeaderMap, token: &str) -> Result<HeaderMap> {
    Ok(preserve_content_type(
        cpa_headers(incoming, token)?,
        incoming,
    ))
}

fn preserve_content_type(mut output: HeaderMap, incoming: &HeaderMap) -> HeaderMap {
    if let Some(content_type) = incoming.get(header::CONTENT_TYPE) {
        output.insert(header::CONTENT_TYPE, content_type.clone());
    }
    output
}

pub fn cpa_headers(incoming: &HeaderMap, cpa_token: &str) -> Result<HeaderMap> {
    let mut output = selected_headers(incoming, CPA_PASSTHROUGH_HEADERS);
    let mut value = HeaderValue::from_str(&format!("Bearer {cpa_token}"))?;
    value.set_sensitive(true);
    output.insert(header::AUTHORIZATION, value);
    Ok(output)
}

fn selected_headers(incoming: &HeaderMap, names: &'static [&'static str]) -> HeaderMap {
    let mut output = HeaderMap::new();
    for &name in names {
        for value in incoming.get_all(name) {
            output.append(HeaderName::from_static(name), value.clone());
        }
    }
    output.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpa_headers_allow_only_protocol_headers_and_the_cpa_token() {
        let incoming = HeaderMap::from_iter([
            (
                header::AUTHORIZATION,
                HeaderValue::from_static("Bearer oauth"),
            ),
            (header::COOKIE, HeaderValue::from_static("session=private")),
            (
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_static("provider-secret"),
            ),
            (
                HeaderName::from_static("x-amz-security-token"),
                HeaderValue::from_static("unknown-secret"),
            ),
            (
                HeaderName::from_static("chatgpt-account-id"),
                HeaderValue::from_static("account"),
            ),
            (
                header::ACCEPT,
                HeaderValue::from_static("text/event-stream"),
            ),
        ]);
        let output = cpa_headers(&incoming, "cpa-secret").unwrap();
        assert_eq!(output[header::AUTHORIZATION], "Bearer cpa-secret");
        assert_eq!(output[header::ACCEPT], "text/event-stream");
        assert!(!output.contains_key(header::COOKIE));
        assert!(!output.contains_key("x-api-key"));
        assert!(!output.contains_key("x-amz-security-token"));
        assert!(!output.contains_key("chatgpt-account-id"));
    }

    #[test]
    fn official_image_headers_preserve_content_type_without_leaking_credentials() {
        let incoming = HeaderMap::from_iter([
            (
                header::AUTHORIZATION,
                HeaderValue::from_static("Bearer oauth"),
            ),
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("multipart/form-data; boundary=abc"),
            ),
            (
                HeaderName::from_static("x-codex-imagegen-request-id"),
                HeaderValue::from_static("req-1"),
            ),
            (header::COOKIE, HeaderValue::from_static("session=private")),
            (
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_static("provider-secret"),
            ),
        ]);
        let output = official_image_headers(&incoming).unwrap();
        assert_eq!(output[header::AUTHORIZATION], "Bearer oauth");
        assert_eq!(
            output[header::CONTENT_TYPE],
            "multipart/form-data; boundary=abc"
        );
        assert_eq!(output["x-codex-imagegen-request-id"], "req-1");
        assert!(!output.contains_key(header::COOKIE));
        assert!(!output.contains_key("x-api-key"));
    }

    /// A JSON image request keeps the default content type, and a request with
    /// no official OAuth header fails closed instead of being sent anonymously.
    #[test]
    fn official_image_headers_default_to_json_and_require_oauth() {
        let json_only = HeaderMap::from_iter([(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer oauth"),
        )]);
        let output = official_image_headers(&json_only).unwrap();
        assert_eq!(output[header::CONTENT_TYPE], "application/json");

        assert!(official_image_headers(&HeaderMap::new()).is_err());
    }

    #[test]
    fn official_headers_never_receive_the_cpa_token() {
        let incoming = HeaderMap::from_iter([
            (
                header::AUTHORIZATION,
                HeaderValue::from_static("Bearer oauth"),
            ),
            (
                HeaderName::from_static("originator"),
                HeaderValue::from_static("codex_cli_rs"),
            ),
            (
                HeaderName::from_static("x-provider-token"),
                HeaderValue::from_static("private"),
            ),
        ]);
        let output = official_headers(&incoming).unwrap();
        assert_eq!(output[header::AUTHORIZATION], "Bearer oauth");
        assert_eq!(output["originator"], "codex_cli_rs");
        assert!(!output.contains_key("x-provider-token"));
    }
}
