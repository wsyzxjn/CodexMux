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
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub listen: SocketAddr,
    pub cpa: Cpa,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT),
            cpa: Cpa::default(),
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
        if !self.listen.ip().is_loopback() {
            bail!("listen address must be loopback");
        }
        self.cpa.validate()
    }
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
    fn validate(&self) -> Result<()> {
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
}

impl Credentials {
    pub fn validate(&self) -> Result<()> {
        if self.proxy_token.trim().is_empty() {
            bail!("proxy token must not be empty");
        }
        if self.cpa_token.trim().is_empty() {
            bail!("CPA token must not be empty");
        }
        if self.proxy_token == self.cpa_token {
            bail!("proxy token and CPA token must be different");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpa_allows_remote_https_and_local_http_without_url_state() {
        let mut settings = Settings::default();
        settings.cpa.base_url = "https://cpa.example.com/v1".into();
        assert!(settings.validate().is_ok());
        settings.cpa.base_url = "http://cpa.example.com/v1".into();
        assert!(settings.validate().is_err());
        settings.cpa.base_url = "http://127.0.0.1:8317/v1?tenant=x".into();
        assert!(settings.validate().is_err());
        settings.cpa.base_url = "http://[::1]:8317/v1".into();
        assert!(settings.validate().is_ok());
    }
}
