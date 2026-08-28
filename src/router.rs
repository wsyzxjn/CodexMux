use anyhow::{Context, Result};
use http::{HeaderMap, HeaderName, HeaderValue, header};

pub fn official_headers(incoming: &HeaderMap) -> Result<HeaderMap> {
    let mut output = sanitized_headers(incoming);
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
    let mut output = sanitized_headers(incoming);
    let mut value = HeaderValue::from_str(&format!("Bearer {cpa_token}"))?;
    value.set_sensitive(true);
    output.insert(header::AUTHORIZATION, value);
    Ok(output)
}

fn sanitized_headers(incoming: &HeaderMap) -> HeaderMap {
    let mut output = HeaderMap::new();
    for (name, value) in incoming {
        if !is_sensitive_request_header(name.as_str()) {
            output.append(name.clone(), value.clone());
        }
    }
    output.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    output
}

fn is_sensitive_request_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "cookie"
            | "host"
            | "content-length"
            | "connection"
            | "proxy-connection"
            | "transfer-encoding"
            | "upgrade"
            | "x-modelmux-token"
            | "chatgpt-account-id"
            | "openai-organization"
            | "openai-project"
            | "x-api-key"
            | "api-key"
            | "proxy-authorization"
            | "accept-encoding"
            | "if-match"
            | "if-none-match"
            | "if-modified-since"
            | "if-unmodified-since"
            | "if-range"
            | "range"
            | "x-goog-api-key"
            | "x-groq-api-key"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpa_headers_drop_oauth_and_inject_only_the_cpa_token() {
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
                HeaderName::from_static("chatgpt-account-id"),
                HeaderValue::from_static("account"),
            ),
        ]);
        let output = cpa_headers(&incoming, "cpa-secret").unwrap();
        assert_eq!(output[header::AUTHORIZATION], "Bearer cpa-secret");
        assert!(!output.contains_key(header::COOKIE));
        assert!(!output.contains_key("x-api-key"));
        assert!(!output.contains_key("chatgpt-account-id"));
    }

    #[test]
    fn official_headers_never_receive_the_cpa_token() {
        let incoming = HeaderMap::from_iter([(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer oauth"),
        )]);
        let output = official_headers(&incoming).unwrap();
        assert_eq!(output[header::AUTHORIZATION], "Bearer oauth");
    }
}
