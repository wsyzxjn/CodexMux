use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::fsutil::atomic_write;

pub const DEFAULT_PORT: u16 = 48682;
pub const OFFICIAL_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    pub settings: PathBuf,
    pub credentials: PathBuf,
    pub catalog: PathBuf,
    pub state: PathBuf,
    pub backups: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let root = if let Some(root) = std::env::var_os("MODELMUX_HOME") {
            PathBuf::from(root)
        } else {
            dirs::data_dir()
                .context("cannot locate the user data directory")?
                .join("ModelMux")
        };
        Ok(Self::from_root(root))
    }

    pub fn from_root(root: PathBuf) -> Self {
        Self {
            settings: root.join("config.toml"),
            credentials: root.join("credentials.json"),
            catalog: root.join("model-catalog.json"),
            state: root.join("state/codex-config.json"),
            backups: root.join("backups"),
            root,
        }
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub listen: SocketAddr,
    pub providers: Vec<Provider>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT),
            providers: vec![Provider::official()],
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        let settings: Self = toml::from_slice(&bytes).context("invalid ModelMux config.toml")?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        atomic_write(path, toml::to_string_pretty(self)?.as_bytes())
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!(
                "unsupported ModelMux config schema version {}",
                self.schema_version
            );
        }
        if !self.listen.ip().is_loopback() {
            bail!("listen address must be loopback");
        }
        let mut ids = HashSet::new();
        let mut models = HashSet::new();
        let mut official = 0;
        for provider in &self.providers {
            provider.validate()?;
            if !ids.insert(provider.id.clone()) {
                bail!("duplicate provider id {}", provider.id);
            }
            if provider.kind == ProviderKind::Official {
                official += 1;
            }
            for model in &provider.models {
                if !models.insert(model.slug.clone()) {
                    bail!("model {} is routed by more than one provider", model.slug);
                }
            }
        }
        if official != 1 {
            bail!("exactly one official provider is required");
        }
        Ok(())
    }

    pub fn provider_for_model(
        &self,
        model: &str,
        official_models: &HashSet<String>,
    ) -> Result<&Provider> {
        let mut external = self
            .providers
            .iter()
            .filter(|provider| provider.enabled)
            .filter(|provider| provider.kind == ProviderKind::External)
            .filter(|provider| provider.models.iter().any(|entry| entry.slug == model));
        let provider = external.next();
        if external.next().is_some() {
            bail!("model {model} has an ambiguous route");
        }
        if let Some(provider) = provider {
            return Ok(provider);
        }
        if !official_models.contains(model) {
            bail!("no enabled provider routes model {model}");
        }
        self.providers
            .iter()
            .find(|provider| provider.enabled && provider.kind == ProviderKind::Official)
            .with_context(|| format!("no enabled provider routes model {model}"))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub dialect: Dialect,
    pub base_url: String,
    pub credential_header: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub models: Vec<Model>,
    pub enabled: bool,
    #[serde(default)]
    pub allow_cross_model_previous_response_id: bool,
}

impl Default for Provider {
    fn default() -> Self {
        Self::official()
    }
}

impl Provider {
    pub fn official() -> Self {
        Self {
            id: "openai".into(),
            name: "OpenAI".into(),
            kind: ProviderKind::Official,
            dialect: Dialect::Responses,
            base_url: OFFICIAL_BASE_URL.into(),
            credential_header: None,
            headers: BTreeMap::new(),
            models: Vec::new(),
            enabled: true,
            allow_cross_model_previous_response_id: true,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            bail!("provider id must use lowercase ASCII letters, digits, and hyphens");
        }
        let url = reqwest::Url::parse(&self.base_url)
            .with_context(|| format!("provider {} has an invalid base_url", self.id))?;
        if url.scheme() != "https"
            && url.host_str() != Some("127.0.0.1")
            && url.host_str() != Some("localhost")
        {
            bail!("provider {} must use HTTPS or loopback HTTP", self.id);
        }
        if self.kind == ProviderKind::Official {
            if self.dialect != Dialect::Responses {
                bail!("the official provider must use native Responses");
            }
            if self.id != "openai" || self.base_url != OFFICIAL_BASE_URL {
                bail!("the official provider must use the canonical ChatGPT Codex endpoint");
            }
        }
        let reserved = [
            "authorization",
            "api-key",
            "cookie",
            "host",
            "content-length",
            "content-type",
            "connection",
            "proxy-connection",
            "transfer-encoding",
            "upgrade",
            "proxy-authorization",
            "x-api-key",
            "x-modelmux-token",
            "chatgpt-account-id",
            "openai-organization",
            "openai-project",
        ];
        if let Some(header) = &self.credential_header {
            let name = header.to_ascii_lowercase();
            if reserved.contains(&name.as_str()) {
                bail!("provider {} uses a reserved credential header", self.id);
            }
            http::HeaderName::from_bytes(header.as_bytes())
                .context("invalid credential header name")?;
        }
        for (name, value) in &self.headers {
            let lower = name.to_ascii_lowercase();
            if lower != "anthropic-version"
                || reserved.contains(&lower.as_str())
                || self
                    .credential_header
                    .as_deref()
                    .is_some_and(|credential| credential.eq_ignore_ascii_case(name))
            {
                bail!(
                    "provider {} declares unsupported static header {name}",
                    self.id
                );
            }
            http::HeaderName::from_bytes(name.as_bytes()).context("invalid static header name")?;
            http::HeaderValue::from_str(value).context("invalid static header value")?;
        }
        if self.kind == ProviderKind::External && self.models.is_empty() {
            bail!(
                "external provider {} must declare at least one model",
                self.id
            );
        }
        for model in &self.models {
            if model.slug.trim().is_empty() {
                bail!("provider {} declares an empty model slug", self.id);
            }
            if model.context_window <= 0 {
                bail!(
                    "model {} on provider {} has a non-positive context window",
                    model.slug,
                    self.id
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Official,
    External,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Dialect {
    Responses,
    OpenaiChat,
    AnthropicMessages,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Model {
    pub slug: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_context_window")]
    pub context_window: i64,
    #[serde(default)]
    pub accepts_images: bool,
}

fn default_context_window() -> i64 {
    128_000
}

impl Model {
    pub fn display_name(&self) -> &str {
        if self.display_name.is_empty() {
            &self.slug
        } else {
            &self.display_name
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Credentials {
    pub schema_version: u32,
    pub proxy_token: String,
    #[serde(default)]
    pub providers: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_external_models() {
        let model = Model {
            slug: "same".into(),
            display_name: String::new(),
            description: None,
            context_window: 128_000,
            accepts_images: false,
        };
        let external = |id: &str| Provider {
            id: id.into(),
            name: id.into(),
            kind: ProviderKind::External,
            dialect: Dialect::Responses,
            base_url: "https://example.com/v1".into(),
            credential_header: None,
            headers: BTreeMap::new(),
            models: vec![model.clone()],
            enabled: true,
            allow_cross_model_previous_response_id: false,
        };
        let settings = Settings {
            providers: vec![Provider::official(), external("one"), external("two")],
            ..Settings::default()
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn routing_requires_an_exact_external_or_official_slug() {
        let settings = Settings::default();
        let official = HashSet::from(["gpt-official".to_owned()]);
        assert_eq!(
            settings
                .provider_for_model("gpt-official", &official)
                .unwrap()
                .kind,
            ProviderKind::Official
        );
        assert!(settings.provider_for_model("gpt-typo", &official).is_err());
    }

    #[test]
    fn rejects_noncanonical_official_endpoint() {
        let mut settings = Settings::default();
        settings.providers[0].base_url = "https://example.com/v1".into();
        assert!(settings.validate().is_err());
    }

    #[test]
    fn external_cross_model_ids_are_opt_in() {
        let provider: Provider = toml::from_str(
            r#"
            id = "external"
            name = "External"
            kind = "external"
            dialect = "responses"
            base_url = "https://example.com/v1"
            enabled = true

            [[models]]
            slug = "external-model"
            "#,
        )
        .unwrap();
        assert!(!provider.allow_cross_model_previous_response_id);
    }

    #[test]
    fn rejects_static_credential_headers() {
        let mut provider = Provider::official();
        provider.kind = ProviderKind::External;
        provider.id = "external".into();
        provider.base_url = "https://example.com/v1".into();
        provider.models = vec![Model {
            slug: "external-model".into(),
            display_name: String::new(),
            description: None,
            context_window: 128_000,
            accepts_images: false,
        }];
        provider
            .headers
            .insert("api-key".into(), "secret-in-config".into());
        assert!(provider.validate().is_err());
        provider.headers.clear();
        provider.credential_header = Some("x-provider-key".into());
        provider
            .headers
            .insert("X-Provider-Key".into(), "override".into());
        assert!(provider.validate().is_err());
    }

    #[test]
    fn rejects_unknown_settings_schema() {
        let settings = Settings {
            schema_version: 2,
            ..Settings::default()
        };
        assert!(settings.validate().is_err());
    }
}
