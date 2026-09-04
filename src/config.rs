use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::fsutil::atomic_write;

pub const DEFAULT_PORT: u16 = 48682;
pub const DEFAULT_CPA_BASE_URL: &str = "http://127.0.0.1:8317/v1";
pub const OFFICIAL_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const CPA_MODEL_PREFIX: &str = "cpa/";
pub const HOME_ENV: &str = "CODEXMUX_HOME";
const DATA_DIR_NAME: &str = "CodexMux";

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    pub settings: PathBuf,
    pub credentials: PathBuf,
    pub catalog: PathBuf,
    pub state: PathBuf,
    pub backups: PathBuf,
    pub cpa_profiles: PathBuf,
    pub search_capabilities: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let root = if let Some(root) = env_root(HOME_ENV)? {
            root
        } else {
            default_root(&dirs::data_dir().context("cannot locate the user data directory")?)
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
            cpa_profiles: root.join("cpa-profiles.toml"),
            search_capabilities: root.join("search-capabilities.json"),
            root,
        }
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))
    }
}

fn env_root(name: &str) -> Result<Option<PathBuf>> {
    let Some(root) = std::env::var_os(name) else {
        return Ok(None);
    };
    anyhow::ensure!(!root.is_empty(), "{name} must not be empty");
    Ok(Some(PathBuf::from(root)))
}

fn default_root(data_dir: &Path) -> PathBuf {
    data_dir.join(DATA_DIR_NAME)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub listen: SocketAddr,
    pub cpa: Cpa,
    pub catalog: Catalog,
    pub web_search: WebSearch,
    /// Translate selected transient CPA failures into Codex's
    /// `server_is_overloaded` response so the desktop UI can show its
    /// automatic capacity retry countdown.
    #[serde(default = "default_true")]
    pub map_capacity_errors: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT),
            cpa: Cpa::default(),
            catalog: Catalog::default(),
            web_search: WebSearch::default(),
            map_capacity_errors: default_true(),
        }
    }
}

fn default_true() -> bool {
    true
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        let settings: Self = toml::from_slice(&bytes).context("invalid CodexMux config.toml")?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        atomic_write(path, toml::to_string_pretty(self)?.as_bytes())
    }

    pub fn validate(&self) -> Result<()> {
        if !self.listen.ip().is_loopback() {
            bail!("listen address must be loopback");
        }
        self.cpa.validate()?;
        if self.web_search.enabled && self.web_search.backend_model.trim().is_empty() {
            bail!("web_search.enabled requires a nonempty backend_model");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Catalog {
    /// Advertise the Codex-side `ultra` preset for every merged model. Codex
    /// maps `ultra` to a real model-supported effort before sending requests,
    /// so this only changes catalog metadata exposed to the Codex client.
    pub advertise_ultra: bool,
    /// Serve one shared compaction-compatibility hash for every merged model.
    /// Codex compacts before sampling whenever two consecutive turns advertise
    /// different `comp_hash` values, and it runs that compaction on the
    /// previous model, so a mid-conversation switch away from an exhausted
    /// model would otherwise deadlock on the model being left behind.
    pub unify_comp_hash: bool,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            advertise_ultra: false,
            unify_comp_hash: default_true(),
        }
    }
}

/// Shared Responses API `web_search` backend. When enabled, CodexMux runs
/// search through this model, injects the results into the original request,
/// and forwards that request to the user-selected model.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebSearch {
    pub enabled: bool,
    /// Catalog slug of the search-capable backend model.
    pub backend_model: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cpa {
    pub base_url: String,
}

impl Default for Cpa {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_CPA_BASE_URL.into(),
        }
    }
}

impl Cpa {
    pub fn is_loopback(&self) -> bool {
        reqwest::Url::parse(&self.base_url)
            .ok()
            .is_some_and(|url| is_loopback_host(url.host_str()))
    }

    pub fn validate(&self) -> Result<()> {
        let url = reqwest::Url::parse(&self.base_url).context("CPA has an invalid base_url")?;
        if !matches!(url.scheme(), "http" | "https") {
            bail!("CPA base_url must use HTTP or HTTPS");
        }
        if url.scheme() == "http" && !is_loopback_host(url.host_str()) {
            bail!("remote CPA base_url must use HTTPS");
        }
        if !url.username().is_empty() || url.password().is_some() {
            bail!("CPA base_url must not contain credentials");
        }
        if url.query().is_some() || url.fragment().is_some() {
            bail!("CPA base_url must not contain a query or fragment");
        }
        Ok(())
    }

    /// Return the CPA management page corresponding to this API base URL.
    /// CPA serves its management UI at the service root, while CodexMux
    /// normally stores the Responses API root with a trailing `/v1`.
    pub fn management_url(&self) -> Result<reqwest::Url> {
        self.validate()?;
        let mut url = reqwest::Url::parse(&self.base_url).context("CPA has an invalid base_url")?;
        let path = url.path().trim_end_matches('/');
        let prefix = path.strip_suffix("/v1").unwrap_or(path);
        let management_path = if prefix.is_empty() {
            "/management.html".to_owned()
        } else {
            format!("{prefix}/management.html")
        };
        url.set_path(&management_path);
        Ok(url)
    }

    /// Origin the management UI should call for `/v0/management`.
    pub fn management_api_base(&self) -> Result<String> {
        let mut base = self.management_url()?;
        let path = base
            .path()
            .trim_end_matches("/management.html")
            .trim_end_matches('/')
            .to_owned();
        base.set_query(None);
        base.set_fragment(None);
        base.set_path(if path.is_empty() { "/" } else { &path });
        Ok(base.as_str().trim_end_matches('/').to_owned())
    }

    /// Loopback management URL with login query so the bundled Web UI can
    /// auto-fill the current endpoint and management key. Remote endpoints
    /// keep a bare page URL; CodexMux must not put their key in a query.
    pub fn management_connect_url(&self, management_key: &str) -> Result<reqwest::Url> {
        let mut url = self.management_url()?;
        if self.is_loopback() && !management_key.trim().is_empty() {
            let api_base = self.management_api_base()?;
            url.query_pairs_mut()
                .append_pair("cmb", &api_base)
                .append_pair("cmk", management_key);
        }
        Ok(url)
    }
}

fn is_loopback_host(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    host == "localhost"
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub proxy_token: String,
    pub cpa_token: String,
    #[serde(default)]
    pub cpa_management_key: String,
}

impl Credentials {
    pub fn validate(&self) -> Result<()> {
        if self.proxy_token.trim().is_empty() {
            bail!("proxy token must not be empty");
        }
        if self.cpa_token.trim().is_empty() {
            bail!("CPA token must not be empty");
        }
        if self.cpa_management_key.trim().is_empty() {
            bail!("CPA management key must not be empty");
        }
        if !self
            .cpa_management_key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        {
            bail!("CPA management key contains unsupported characters");
        }
        if self.proxy_token == self.cpa_token
            || self.proxy_token == self.cpa_management_key
            || self.cpa_token == self.cpa_management_key
        {
            bail!("proxy token, CPA token, and CPA management key must be different");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn default_data_root_is_codexmux() {
        let data = tempdir().unwrap();
        assert_eq!(default_root(data.path()), data.path().join(DATA_DIR_NAME));
    }

    #[test]
    fn cpa_allows_remote_https_and_local_http_without_url_state() {
        let mut settings = Settings::default();
        assert!(settings.cpa.is_loopback());
        settings.cpa.base_url = "https://cpa.example.com/v1".into();
        assert!(!settings.cpa.is_loopback());
        assert!(settings.validate().is_ok());
        settings.cpa.base_url = "http://cpa.example.com/v1".into();
        assert!(settings.validate().is_err());
        settings.cpa.base_url = "http://127.0.0.1:8317/v1?tenant=x".into();
        assert!(settings.validate().is_err());
        settings.cpa.base_url = "http://[::1]:8317/v1".into();
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn cpa_management_url_maps_api_root_to_service_root() {
        let cpa = Cpa {
            base_url: "http://127.0.0.1:8317/v1/".into(),
        };
        assert_eq!(
            cpa.management_url().unwrap().as_str(),
            "http://127.0.0.1:8317/management.html"
        );
        assert_eq!(cpa.management_api_base().unwrap(), "http://127.0.0.1:8317");

        let cpa = Cpa {
            base_url: "https://cpa.example.com/proxy/v1".into(),
        };
        assert_eq!(
            cpa.management_url().unwrap().as_str(),
            "https://cpa.example.com/proxy/management.html"
        );
        assert_eq!(
            cpa.management_api_base().unwrap(),
            "https://cpa.example.com/proxy"
        );
    }

    #[test]
    fn loopback_management_connect_url_carries_current_endpoint() {
        let cpa = Cpa {
            base_url: "http://127.0.0.1:8317/v1".into(),
        };
        let url = cpa.management_connect_url("mgmt-key").unwrap();
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.path(), "/management.html");
        let query: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert!(query.contains(&("cmb".into(), "http://127.0.0.1:8317".into())));
        assert!(query.contains(&("cmk".into(), "mgmt-key".into())));

        let remote = Cpa {
            base_url: "https://cpa.example.com/v1".into(),
        };
        assert_eq!(
            remote.management_connect_url("mgmt-key").unwrap().as_str(),
            "https://cpa.example.com/management.html"
        );
    }

    #[test]
    fn shared_search_requires_a_backend_when_enabled() {
        let mut settings = Settings::default();
        settings.web_search.enabled = true;
        assert!(settings.validate().is_err());
        settings.web_search.backend_model = "gpt-5.6-sol".into();
        assert!(settings.validate().is_ok());
    }
}
