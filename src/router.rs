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
