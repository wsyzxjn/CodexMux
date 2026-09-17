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

pub fn save_search_capabilities(
    path: &Path,
    store: &SearchCapabilityStore,
) -> Result<()> {
    atomic_write(
        path,
        &serde_json::to_vec_pretty(store).context("serialize search capabilities")?,
    )
}

pub fn record_search_capability(
    path: &Path,
    slug: &str,
    status: SearchCapabilityStatus,
) -> Result<()> {
    let mut store = load_search_capabilities(path)?;
    store.apply(
        slug,
        status,
        time::OffsetDateTime::now_utc().unix_timestamp(),
    );
    save_search_capabilities(path, &store)
}

pub fn catalog_slugs(paths: &Paths) -> Result<Vec<String>> {
    let store = crate::catalog::CatalogStore::load(paths.catalog.clone())?;
    let catalog = store
        .current()
        .context("model catalog snapshot has not been built yet")?;
    let models = catalog
        .get("models")
        .and_then(serde_json::Value::as_array)
        .context("model catalog has no models array")?;
    Ok(models
        .iter()
        .filter_map(|model| model.get("slug").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect())
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
        Verify(crate::config::Settings, crate::config::Credentials),
        Quick(QuickDetect),
    }
    let detector = if verify {
        Detector::Verify(
            crate::config::Settings::load(&paths.settings)?,
            crate::secrets::load(&paths.credentials)?,
        )
    } else {
        Detector::Quick(QuickDetect::load(paths))
    };
    let mut store = load_search_capabilities(&paths.search_capabilities)?;
    let checked_at = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut results = Vec::new();
    for slug in catalog_slugs(paths)? {
        if let Some(only) = only
            && !only.eq_ignore_ascii_case(&slug)
        {
            continue;
        }
        let status = match &detector {
            Detector::Verify(settings, credentials) => {
                verify_one(paths, settings, credentials, &slug)
            }
            Detector::Quick(quick) => quick.capability(&slug),
        };
        store.apply(&slug, status, checked_at);
        results.push((slug, status));
    }
    save_search_capabilities(&paths.search_capabilities, &store)?;
    Ok(results)
}

fn verify_one(
    paths: &Paths,
    settings: &crate::config::Settings,
    credentials: &crate::config::Credentials,
    slug: &str,
) -> SearchCapabilityStatus {
    if slug == crate::catalog::AUTO_REVIEW_MODEL {
        // The proxy refuses codex-auto-review as a shared search backend.
        return SearchCapabilityStatus::Unsupported;
    }
    let Some(upstream) = slug.strip_prefix(crate::config::CPA_MODEL_PREFIX) else {
        // Official models search natively on the official route; the CLI
        // holds no ChatGPT OAuth, so there is nothing it could probe.
        return SearchCapabilityStatus::Verified;
    };
    let direct = crate::cpa::direct_route_for(&paths.cpa_profiles, upstream);
    if direct.ok().flatten().is_some() {
        return SearchCapabilityStatus::Unknown;
    }
    match probe_via_cpa(settings, credentials, upstream) {
        Ok(status) => status,
        Err(_) => SearchCapabilityStatus::Error,
    }
}

/// Inputs for quick capability detection, loaded once per detection run
/// instead of once per catalog slug.
struct QuickDetect {
    direct_models: HashSet<String>,
    config: Option<CpaConfig>,
}

impl QuickDetect {
    fn load(paths: &Paths) -> Self {
        let direct_models = crate::cpa::declared_direct_models(&paths.cpa_profiles)
            .into_iter()
            .map(|model| model.upstream_model)
            .collect();
        let config = fs::read_to_string(crate::cpa::config_path(paths))
            .ok()
            .and_then(|text| serde_yaml::from_str::<CpaConfig>(&text).ok());
        Self {
            direct_models,
            config,
        }
    }

    /// Local heuristic only, with no network traffic. `Supported` means the
    /// provider family is expected to accept `web_search`; run
    /// `search-detect --model <slug> --verify` for real confirmation.
    fn capability(&self, slug: &str) -> SearchCapabilityStatus {
        if slug == crate::catalog::AUTO_REVIEW_MODEL {
            // The proxy refuses codex-auto-review as a shared search backend.
            return SearchCapabilityStatus::Unsupported;
        }
        let Some(upstream) = slug.strip_prefix(crate::config::CPA_MODEL_PREFIX) else {
            return SearchCapabilityStatus::Verified;
        };
        if self.direct_models.contains(upstream) {
            return SearchCapabilityStatus::Unknown;
        }
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
            || config.xai.iter().any(|provider| provider.has_model(upstream))
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
    settings: &crate::config::Settings,
    credentials: &crate::config::Credentials,
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
