use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::config_path;
use crate::{
    catalog::{AUTO_REVIEW_MODEL, CatalogStore},
    config::{CPA_MODEL_PREFIX, Credentials, Paths, Settings},
    fsutil::atomic_write,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchCapabilityStatus {
    /// A real `web_search` probe produced a search call, or the model runs
    /// on the official route where `web_search` is a native platform
    /// feature and the CLI holds no credentials to probe.
    Verified,
    /// The provider family is expected to accept `web_search`, or a real
    /// probe succeeded without producing search-call evidence.
    Supported,
    Unsupported,
    Unknown,
    /// Probe failed for an error unrelated to tool capability.
    Error,
}

impl SearchCapabilityStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Supported => "supported",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchCapability {
    pub status: SearchCapabilityStatus,
    pub checked_at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SearchCapabilityStore {
    #[serde(default)]
    pub entries: BTreeMap<String, SearchCapability>,
}

impl SearchCapabilityStore {
    pub fn status(&self, slug: &str) -> Option<SearchCapabilityStatus> {
        self.entries.get(slug).map(|entry| entry.status)
    }

    /// Record one detection result. A failed probe carries no capability
    /// information, so it never replaces earlier verified, supported, or
    /// unsupported knowledge; it lands only on empty, unknown, or
    /// already-failed entries.
    pub fn apply(&mut self, slug: &str, status: SearchCapabilityStatus, checked_at: i64) {
        if status == SearchCapabilityStatus::Error
            && self.entries.get(slug).is_some_and(|entry| {
                !matches!(
                    entry.status,
                    SearchCapabilityStatus::Error | SearchCapabilityStatus::Unknown
                )
            })
        {
            return;
        }
        self.entries
            .insert(slug.to_owned(), SearchCapability { status, checked_at });
    }
}

pub fn load_search_capabilities(path: &Path) -> Result<SearchCapabilityStore> {
    if !path.exists() {
        return Ok(SearchCapabilityStore::default());
    }
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).context("invalid search capabilities cache")
}

fn save_search_capabilities(path: &Path, store: &SearchCapabilityStore) -> Result<()> {
    atomic_write(
        path,
        &serde_json::to_vec_pretty(store).context("serialize search capabilities")?,
    )
}

/// Slugs of the persisted merged catalog in served order, or `None` before
/// the first complete refresh. A snapshot that exists but cannot be read is
/// an error.
pub fn catalog_slugs(paths: &Paths) -> Result<Option<Vec<String>>> {
    let store = CatalogStore::load(paths.catalog.clone())?;
    let Some(catalog) = store.current() else {
        anyhow::ensure!(
            !paths.catalog.exists(),
            "model catalog snapshot {} is unreadable",
            paths.catalog.display()
        );
        return Ok(None);
    };
    let models = catalog
        .get("models")
        .and_then(serde_json::Value::as_array)
        .context("model catalog has no models array")?;
    Ok(Some(
        models
            .iter()
            .filter_map(|model| model.get("slug").and_then(serde_json::Value::as_str))
            .map(str::to_owned)
            .collect(),
    ))
}

pub fn detect_search_capabilities(
    paths: &Paths,
    only: Option<&str>,
    verify: bool,
) -> Result<Vec<(String, SearchCapabilityStatus)>> {
    anyhow::ensure!(
        !verify || only.is_some(),
        "--verify requires --model to avoid probing every provider"
    );
    enum Detector {
        Verify(Settings, Credentials),
        Quick(QuickDetect),
    }
    let detector = if verify {
        Detector::Verify(
            Settings::load(&paths.settings)?,
            crate::secrets::load(&paths.credentials)?,
        )
    } else {
        Detector::Quick(QuickDetect::load(paths))
    };
    let mut store = load_search_capabilities(&paths.search_capabilities)?;
    let checked_at = crate::fsutil::unix_time_secs();
    let mut results = Vec::new();
    let slugs = catalog_slugs(paths)?.context("model catalog snapshot has not been built yet")?;
    for slug in slugs {
        if let Some(only) = only
            && !only.eq_ignore_ascii_case(&slug)
        {
            continue;
        }
        let status = match &detector {
            Detector::Verify(settings, credentials) => verify_one(settings, credentials, &slug),
            Detector::Quick(quick) => quick.capability(&slug),
        };
        store.apply(&slug, status, checked_at);
        results.push((slug, status));
    }
    save_search_capabilities(&paths.search_capabilities, &store)?;
    Ok(results)
}

fn verify_one(
    settings: &Settings,
    credentials: &Credentials,
    slug: &str,
) -> SearchCapabilityStatus {
    if slug == AUTO_REVIEW_MODEL {
        // The proxy refuses codex-auto-review as a shared search backend.
        return SearchCapabilityStatus::Unsupported;
    }
    let Some(upstream) = slug.strip_prefix(CPA_MODEL_PREFIX) else {
        // Official models search natively on the official route; the CLI
        // holds no ChatGPT OAuth, so there is nothing it could probe.
        return SearchCapabilityStatus::Verified;
    };
    match probe_via_cpa(settings, credentials, upstream) {
        Ok(status) => status,
        Err(_) => SearchCapabilityStatus::Error,
    }
}

/// Inputs for quick capability detection, loaded once per detection run
/// instead of once per catalog slug.
struct QuickDetect {
    config: Option<CpaConfig>,
}

impl QuickDetect {
    fn load(paths: &Paths) -> Self {
        let config = fs::read_to_string(config_path(paths))
            .ok()
            .and_then(|text| serde_yaml::from_str::<CpaConfig>(&text).ok());
        Self { config }
    }

    /// Local heuristic only, with no network traffic. `Supported` means the
    /// provider family is expected to accept `web_search`; run
    /// `search-detect --model <slug> --verify` for real confirmation.
    fn capability(&self, slug: &str) -> SearchCapabilityStatus {
        if slug == AUTO_REVIEW_MODEL {
            // The proxy refuses codex-auto-review as a shared search backend.
            return SearchCapabilityStatus::Unsupported;
        }
        let Some(upstream) = slug.strip_prefix(CPA_MODEL_PREFIX) else {
            return SearchCapabilityStatus::Verified;
        };
        let lower = upstream.to_lowercase();
        if lower.starts_with("claude-") || lower.contains("gemini") || lower.starts_with("grok") {
            return SearchCapabilityStatus::Supported;
        }
        let Some(config) = &self.config else {
            return SearchCapabilityStatus::Unknown;
        };
        if config
            .claude
            .iter()
            .any(|provider| provider.has_model(upstream))
            || config
                .xai
                .iter()
                .any(|provider| provider.has_model(upstream))
            || config
                .gemini
                .iter()
                .any(|provider| provider.has_model(upstream))
            || config
                .antigravity
                .iter()
                .any(|provider| provider.has_model(upstream))
        {
            return SearchCapabilityStatus::Supported;
        }
        SearchCapabilityStatus::Unknown
    }
}

#[derive(Default, Deserialize)]
struct CpaConfig {
    #[serde(rename = "claude-api-key", default)]
    claude: Vec<ProviderConfig>,
    #[serde(rename = "xai-api-key", default)]
    xai: Vec<ProviderConfig>,
    #[serde(rename = "gemini-api-key", default)]
    gemini: Vec<ProviderConfig>,
    #[serde(rename = "antigravity-api-key", default)]
    antigravity: Vec<ProviderConfig>,
}

#[derive(Default, Deserialize)]
struct ProviderConfig {
    #[serde(default)]
    models: Vec<ProviderModel>,
}

#[derive(Default, Deserialize)]
struct ProviderModel {
    #[serde(default)]
    name: String,
    #[serde(default)]
    alias: String,
}

impl ProviderConfig {
    fn has_model(&self, upstream: &str) -> bool {
        self.models.iter().any(|model| {
            model.name.eq_ignore_ascii_case(upstream) || model.alias.eq_ignore_ascii_case(upstream)
        })
    }
}

fn probe_via_cpa(
    settings: &Settings,
    credentials: &Credentials,
    upstream_model: &str,
) -> Result<SearchCapabilityStatus> {
    let url = format!("{}/responses", settings.cpa.base_url.trim_end_matches('/'));
    let payload = serde_json::json!({
        "model": upstream_model,
        "input": "Search for codexmux-probe-unique-20260904",
        "tools": [{"type": "web_search"}],
        "tool_choice": "required",
        "stream": false
    });
    let client = reqwest::blocking::Client::builder()
        // A forced real search can run several search rounds plus reasoning;
        // grok-4.6 was observed at ~30s. Verify probes are single-model and
        // user-initiated, so a generous budget beats a false `error`.
        .timeout(Duration::from_secs(90))
        .build()?;
    let response = client
        .post(&url)
        .bearer_auth(&credentials.cpa_token)
        .json(&payload)
        .send()
        .with_context(|| format!("search probe for {upstream_model} failed"))?;
    let status = response.status();
    let body = response.text().unwrap_or_default();
    Ok(classify_search_probe(status.as_u16(), &body))
}

/// Classify one real `web_search` probe. `Unsupported` requires the error to
/// actually reject the tool; unrelated 4xx bodies that merely contain a word
/// like "unsupported" stay `Error`.
fn classify_search_probe(http_status: u16, body: &str) -> SearchCapabilityStatus {
    if (200..300).contains(&http_status) {
        if body.contains("web_search_call") {
            return SearchCapabilityStatus::Verified;
        }
        return SearchCapabilityStatus::Supported;
    }
    let lower = body.to_lowercase();
    let rejects_tool = lower.contains("web_search")
        || (lower.contains("tool")
            && (lower.contains("unsupported")
                || lower.contains("not supported")
                || lower.contains("unknown")
                || lower.contains("not available")));
    if matches!(http_status, 400 | 404 | 422) && rejects_tool {
        return SearchCapabilityStatus::Unsupported;
    }
    SearchCapabilityStatus::Error
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpa::test_support::test_root;

    #[test]
    fn search_capability_cache_round_trips() {
        let root = test_root();
        let path = &root.paths.search_capabilities;
        assert!(load_search_capabilities(path).unwrap().entries.is_empty());
        let mut store = SearchCapabilityStore::default();
        store.apply("gpt-5.6-sol", SearchCapabilityStatus::Verified, 1);
        store.apply("cpa/deepseek", SearchCapabilityStatus::Unsupported, 2);
        save_search_capabilities(path, &store).unwrap();

        let store = load_search_capabilities(path).unwrap();
        assert_eq!(
            store.status("gpt-5.6-sol"),
            Some(SearchCapabilityStatus::Verified)
        );
        assert_eq!(
            store.status("cpa/deepseek"),
            Some(SearchCapabilityStatus::Unsupported)
        );
    }

    #[test]
    fn catalog_slugs_follow_the_snapshot_and_report_unreadable_ones() {
        let root = test_root();
        let paths = &root.paths;
        assert_eq!(catalog_slugs(paths).unwrap(), None);

        CatalogStore::load(paths.catalog.clone())
            .unwrap()
            .replace(
                &serde_json::json!({"models": [{"slug": "gpt-5.6-sol"}]}),
                &serde_json::json!({"models": [{"slug": "glm-5.3"}]}),
            )
            .unwrap();
        let slugs = catalog_slugs(paths).unwrap().unwrap();
        assert!(slugs.contains(&"gpt-5.6-sol".to_owned()));
        assert!(slugs.contains(&"cpa/glm-5.3".to_owned()));

        fs::write(&paths.catalog, b"{").unwrap();
        assert!(catalog_slugs(paths).is_err());
    }

    #[test]
    fn search_probe_classification() {
        assert_eq!(
            classify_search_probe(200, r#"{"output":[{"type":"web_search_call"}]}"#),
            SearchCapabilityStatus::Verified
        );
        assert_eq!(
            classify_search_probe(200, "{}"),
            SearchCapabilityStatus::Supported
        );
        assert_eq!(
            classify_search_probe(400, "unsupported web_search"),
            SearchCapabilityStatus::Unsupported
        );
        assert_eq!(
            classify_search_probe(400, "tool type not supported"),
            SearchCapabilityStatus::Unsupported
        );
        // A 4xx that merely mentions an unsupported parameter is not a tool
        // rejection.
        assert_eq!(
            classify_search_probe(400, "Unsupported parameter: 'stream'"),
            SearchCapabilityStatus::Error
        );
        assert_eq!(
            classify_search_probe(401, "auth"),
            SearchCapabilityStatus::Error
        );
    }

    #[test]
    fn failed_probes_never_erase_prior_capability_knowledge() {
        let mut store = SearchCapabilityStore::default();
        store.apply("model", SearchCapabilityStatus::Supported, 1);
        store.apply("model", SearchCapabilityStatus::Error, 2);
        let entry = store.entries.get("model").unwrap();
        assert_eq!(entry.status, SearchCapabilityStatus::Supported);
        assert_eq!(entry.checked_at, 1);

        // Real knowledge changes still overwrite.
        store.apply("model", SearchCapabilityStatus::Unsupported, 3);
        assert_eq!(
            store.status("model"),
            Some(SearchCapabilityStatus::Unsupported)
        );

        // Errors land on empty, unknown, or already-failed entries.
        store.apply("fresh", SearchCapabilityStatus::Error, 4);
        assert_eq!(store.status("fresh"), Some(SearchCapabilityStatus::Error));
        store.apply("unknown", SearchCapabilityStatus::Unknown, 5);
        store.apply("unknown", SearchCapabilityStatus::Error, 6);
        assert_eq!(store.status("unknown"), Some(SearchCapabilityStatus::Error));
    }

    #[test]
    fn quick_search_capability_is_local_and_non_aborting() {
        let root = test_root();
        let quick = QuickDetect::load(&root.paths);
        assert_eq!(
            quick.capability("gpt-5.6-sol"),
            SearchCapabilityStatus::Verified
        );
        assert_eq!(
            quick.capability("cpa/claude-opus-5"),
            SearchCapabilityStatus::Supported
        );
        assert_eq!(
            quick.capability("cpa/gpt-5.6-sol"),
            SearchCapabilityStatus::Unknown
        );
        assert_eq!(
            quick.capability("cpa/deepseek-ai/DeepSeek-V4-Flash-Vision-Exp"),
            SearchCapabilityStatus::Unknown
        );
        // The proxy refuses codex-auto-review as a shared search backend, so
        // detection must not advertise it.
        assert_eq!(
            quick.capability(AUTO_REVIEW_MODEL),
            SearchCapabilityStatus::Unsupported
        );
    }
}
