use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use http::{HeaderMap, HeaderName, HeaderValue, header};

use crate::config::{Credentials, Dialect, Provider, ProviderKind, Settings};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteIdentity {
    pub provider_id: String,
    pub model: String,
}

pub fn resolve<'a>(
    settings: &'a Settings,
    official_models: &HashSet<String>,
    model: &str,
) -> Result<&'a Provider> {
    settings.provider_for_model(model, official_models)
}

pub fn should_replay(
    previous_provider: &str,
    previous_model: Option<&str>,
    target: &Provider,
    target_model: &str,
) -> bool {
    previous_provider != target.id
        || (previous_model.is_some_and(|model| model != target_model)
            && !target.allow_cross_model_previous_response_id)
}

pub fn upstream_headers(
    incoming: &HeaderMap,
    provider: &Provider,
    settings: &Settings,
    credentials: &Credentials,
) -> Result<HeaderMap> {
    let mut output = HeaderMap::new();
    let credential_headers: HashSet<String> = settings
        .providers
        .iter()
        .filter_map(|provider| provider.credential_header.as_deref())
        .map(str::to_ascii_lowercase)
        .collect();
    for (name, value) in incoming {
        let lower = name.as_str().to_ascii_lowercase();
        if credential_headers.contains(&lower)
            || matches!(
                lower.as_str(),
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
            )
        {
            continue;
        }
        output.append(name.clone(), value.clone());
    }
    output.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );

    match provider.kind {
        ProviderKind::Official => {
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
        }
        ProviderKind::External => {
            let secret = credentials.providers.get(&provider.id).with_context(|| {
                format!("provider {} has no configured credential", provider.id)
            })?;
            let mut value = if provider.credential_header.is_none() {
                HeaderValue::from_str(&format!("Bearer {secret}"))?
            } else {
                HeaderValue::from_str(secret)?
            };
            value.set_sensitive(true);
            let name = match &provider.credential_header {
                Some(name) => HeaderName::from_bytes(name.as_bytes())?,
                None => header::AUTHORIZATION,
            };
            output.insert(name, value);
        }
    }

    for (name, value) in &provider.headers {
        output.insert(
            HeaderName::from_bytes(name.as_bytes())?,
            HeaderValue::from_str(value)?,
        );
    }
    if provider.dialect == Dialect::AnthropicMessages && !output.contains_key("anthropic-version") {
        output.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
    }
    if provider.kind == ProviderKind::External && output.contains_key("cookie") {
        bail!("external route retained a Cookie header");
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::Provider;

    #[test]
    fn external_route_drops_oauth_and_injects_its_credential() {
        let mut provider = Provider::official();
        provider.id = "external".into();
        provider.kind = ProviderKind::External;
        provider.models = vec![crate::config::Model {
            slug: "external-model".into(),
            display_name: String::new(),
            description: None,
            context_window: 128_000,
            accepts_images: false,
        }];
        let incoming = HeaderMap::from_iter([
            (
                header::AUTHORIZATION,
                HeaderValue::from_static("Bearer oauth"),
            ),
            (header::COOKIE, HeaderValue::from_static("session=private")),
        ]);
        let settings = Settings {
            providers: vec![Provider::official(), provider.clone()],
            ..Settings::default()
        };
        let output = upstream_headers(
            &incoming,
            &provider,
            &settings,
            &Credentials {
                schema_version: 1,
                proxy_token: "proxy".into(),
                providers: HashMap::from([("external".into(), "external-key".into())]),
            },
        )
        .unwrap();
        assert_eq!(output[header::AUTHORIZATION], "Bearer external-key");
        assert!(!output.contains_key(header::COOKIE));
    }

    #[test]
    fn credential_headers_for_other_providers_never_cross_routes() {
        let mut first = Provider::official();
        first.id = "first".into();
        first.kind = ProviderKind::External;
        first.base_url = "https://example.com/v1".into();
        first.credential_header = Some("x-first-key".into());
        first.models = vec![crate::config::Model {
            slug: "first-model".into(),
            display_name: String::new(),
            description: None,
            context_window: 128_000,
            accepts_images: false,
        }];
        let mut second = first.clone();
        second.id = "second".into();
        second.credential_header = Some("x-second-key".into());
        second.models[0].slug = "second-model".into();
        let settings = Settings {
            providers: vec![Provider::official(), first, second.clone()],
            ..Settings::default()
        };
        let incoming = HeaderMap::from_iter([
            (
                HeaderName::from_static("x-first-key"),
                HeaderValue::from_static("first-secret"),
            ),
            (
                HeaderName::from_static("x-second-key"),
                HeaderValue::from_static("incoming-second-secret"),
            ),
        ]);
        let output = upstream_headers(
            &incoming,
            &second,
            &settings,
            &Credentials {
                schema_version: 1,
                proxy_token: "proxy".into(),
                providers: HashMap::from([("second".into(), "second-secret".into())]),
            },
        )
        .unwrap();
        assert!(!output.contains_key("x-first-key"));
        assert_eq!(output["x-second-key"], "second-secret");
    }
}
