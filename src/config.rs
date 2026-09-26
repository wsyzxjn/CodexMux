use std::{
    fmt, fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::fsutil::atomic_write;

pub const DEFAULT_PORT: u16 = 48682;
pub const DEFAULT_CPA_BASE_URL: &str = "http://127.0.0.1:8317/v1";
pub const DEFAULT_MAX_REQUEST_MIB: usize = 128;
pub const MAX_MAX_REQUEST_MIB: usize = 1024;
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
    pub server: Server,
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
            server: Server::default(),
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
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let settings: Self =
            toml::from_str(&text).map_err(|error| toml_parse_error(path, &text, &error))?;
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
        if self.server.max_request_mib == 0 || self.server.max_request_mib > MAX_MAX_REQUEST_MIB {
            bail!("server.max_request_mib must be between 1 and {MAX_MAX_REQUEST_MIB}");
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
pub struct Server {
    /// Maximum request body CodexMux buffers before routing it upstream.
    pub max_request_mib: usize,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            max_request_mib: DEFAULT_MAX_REQUEST_MIB,
        }
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

    /// Local management page URL that signs the bundled Web UI in.
    ///
    /// The key travels only in the fragment (`#cmk=`), which browsers never
    /// send to a server or write to access logs. The injected bootstrap stores
    /// it and strips the fragment, and it derives the API base from the page
    /// location, so the URL carries nothing else.
    pub fn management_connect_url(&self, management_key: &str) -> Result<reqwest::Url> {
        anyhow::ensure!(
            self.is_loopback(),
            "management auto-connect only applies to a local CPA"
        );
        anyhow::ensure!(
            !management_key.trim().is_empty(),
            "CPA management key must not be empty"
        );
        let mut url = self.management_url()?;
        // Reuse the URL library's form encoder; URLSearchParams decodes it.
        let mut encoder = reqwest::Url::parse("http://127.0.0.1/").expect("static URL");
        encoder.query_pairs_mut().append_pair("cmk", management_key);
        url.set_fragment(encoder.query());
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

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub proxy_token: String,
    pub cpa_token: String,
    pub cpa_management_key: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Credentials")
            .field("proxy_token", &REDACTED)
            .field("cpa_token", &REDACTED)
            .field("cpa_management_key", &REDACTED)
            .finish()
    }
}

/// Placeholder shown instead of a credential in `Debug` output.
pub const REDACTED: &str = "[redacted]";

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

/// Describe a TOML parse failure by file, position, and message only.
///
/// The rendered `toml` error quotes the offending source line, and a private
/// file's line can hold a token (a missing quote is enough), so the snippet is
/// never included.
pub(crate) fn toml_parse_error(path: &Path, text: &str, error: &toml::de::Error) -> anyhow::Error {
    let message = error.message().trim_end();
    match error.span() {
        Some(span) => {
            let mut offset = span.start.min(text.len());
            while !text.is_char_boundary(offset) {
                offset -= 1;
            }
            let before = &text[..offset];
            let line = before.matches('\n').count() + 1;
            let line_start = before.rfind('\n').map_or(0, |index| index + 1);
            let column = before[line_start..].chars().count() + 1;
            anyhow!(
                "invalid {} at line {line}, column {column}: {message}",
                path.display()
            )
        }
        None => anyhow!("invalid {}: {message}", path.display()),
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
    fn max_request_mib_defaults_to_128_and_is_range_checked() {
        let settings = Settings::default();
        assert_eq!(settings.server.max_request_mib, DEFAULT_MAX_REQUEST_MIB);

        let mut settings = Settings::default();
        settings.server.max_request_mib = 0;
        assert!(settings.validate().is_err());

        settings.server.max_request_mib = MAX_MAX_REQUEST_MIB + 1;
        assert!(settings.validate().is_err());

        settings.server.max_request_mib = MAX_MAX_REQUEST_MIB;
        assert!(settings.validate().is_ok());
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

        let cpa = Cpa {
            base_url: "https://cpa.example.com/proxy/v1".into(),
        };
        assert_eq!(
            cpa.management_url().unwrap().as_str(),
            "https://cpa.example.com/proxy/management.html"
        );
    }

    #[test]
    fn management_connect_url_carries_the_key_only_in_the_fragment() {
        let cpa = Cpa {
            base_url: "http://127.0.0.1:8317/v1".into(),
        };
        let url = cpa.management_connect_url("mgmt-key").unwrap();
        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:8317/management.html#cmk=mgmt-key"
        );
        assert_eq!(url.query(), None);

        // The fragment is form-encoded so URLSearchParams restores the key.
        let url = cpa.management_connect_url("a b&c=d#e").unwrap();
        assert_eq!(url.fragment(), Some("cmk=a+b%26c%3Dd%23e"));
        assert_eq!(url.query(), None);

        let remote = Cpa {
            base_url: "https://cpa.example.com/v1".into(),
        };
        assert!(remote.management_connect_url("mgmt-key").is_err());
        assert!(cpa.management_connect_url(" ").is_err());
    }

    #[test]
    fn credentials_debug_output_never_contains_secrets() {
        let credentials = Credentials {
            proxy_token: "proxy-secret".into(),
            cpa_token: "cpa-secret".into(),
            cpa_management_key: "management-secret".into(),
        };
        let debug = format!("{credentials:?}");
        assert!(!debug.contains("secret"));
        assert_eq!(debug.matches(REDACTED).count(), 3);
    }

    #[test]
    fn toml_errors_report_position_without_quoting_the_source() {
        let root = tempdir().unwrap();
        let path = root.path().join("config.toml");
        let text = "listen = \"127.0.0.1:48682\"\n[cpa]\nbase_url = sk-secret-token\n";
        std::fs::write(&path, text).unwrap();
        let error = Settings::load(&path).unwrap_err();
        let rendered = format!("{error:#}");
        assert!(rendered.contains("at line 3, column 12"), "{rendered}");
        assert!(!rendered.contains("sk-secret-token"), "{rendered}");
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
