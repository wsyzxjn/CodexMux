#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchCapabilityStatus {
    /// A real `web_search` call produced a search call.
    Verified,
    /// The backend accepted the `web_search` tool schema without running it.
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
    store.entries.insert(
        slug.to_owned(),
        SearchCapability {
            status,
            checked_at: time::OffsetDateTime::now_utc().unix_timestamp(),
        },
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
    let mut results = Vec::new();
    for slug in catalog_slugs(paths)? {
        if let Some(only) = only
            && !only.eq_ignore_ascii_case(&slug)
        {
            continue;
        }
        let status = if verify {
            let settings = crate::config::Settings::load(&paths.settings)?;
            let credentials = crate::secrets::load(&paths.credentials)?;
            verify_one(paths, &settings, &credentials, &slug)
        } else {
            quick_capability(paths, &slug)
        };
        record_search_capability(&paths.search_capabilities, &slug, status)?;
        results.push((slug, status));
    }
    Ok(results)
}

fn verify_one(
    paths: &Paths,
    settings: &crate::config::Settings,
    credentials: &crate::config::Credentials,
    slug: &str,
) -> SearchCapabilityStatus {
    let Some(upstream) = slug.strip_prefix(crate::config::CPA_MODEL_PREFIX) else {
        return SearchCapabilityStatus::Verified;
    };
    let direct = crate::cpa::direct_route_for(&paths.cpa_profiles, upstream);
    if direct.ok().flatten().is_some() {
        return SearchCapabilityStatus::Unknown;
    }
    match probe_via_cpa(settings, credentials, upstream, true) {
        Ok(status) => status,
        Err(_) => SearchCapabilityStatus::Error,
    }
}

fn quick_capability(paths: &Paths, slug: &str) -> SearchCapabilityStatus {
    if !slug.starts_with(crate::config::CPA_MODEL_PREFIX) {
        return SearchCapabilityStatus::Verified;
    }
    let Some(upstream) = slug.strip_prefix(crate::config::CPA_MODEL_PREFIX) else {
        return SearchCapabilityStatus::Unknown;
    };
    let direct = crate::cpa::direct_route_for(&paths.cpa_profiles, upstream);
    if direct.ok().flatten().is_some() {
        return SearchCapabilityStatus::Unknown;
    }
    let lower = upstream.to_lowercase();
    if lower.starts_with("claude-") || lower.contains("gemini") || lower.starts_with("grok") {
        return SearchCapabilityStatus::Supported;
    }
    let config_path = crate::cpa::config_path(paths);
    let Ok(text) = fs::read_to_string(config_path) else {
        return SearchCapabilityStatus::Unknown;
    };
    let Ok(config) = serde_yaml::from_str::<CpaConfig>(&text) else {
        return SearchCapabilityStatus::Unknown;
    };
    if config.claude.iter().any(|provider| provider.has_model(upstream))
        || config.xai.iter().any(|provider| provider.has_model(upstream))
        || config.gemini.iter().any(|provider| provider.has_model(upstream))
        || config.antigravity.iter().any(|provider| provider.has_model(upstream))
    {
        return SearchCapabilityStatus::Supported;
    }
    SearchCapabilityStatus::Unknown
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
    verify: bool,
) -> Result<SearchCapabilityStatus> {
    let url = format!("{}/responses", settings.cpa.base_url.trim_end_matches('/'));
    let payload = if verify {
        serde_json::json!({
            "model": upstream_model,
            "input": "Search for codexmux-probe-unique-20260904",
            "tools": [{"type": "web_search"}],
            "tool_choice": "required",
            "stream": false
        })
    } else {
        serde_json::json!({
            "model": upstream_model,
            "input": "probe",
            "tools": [{"type": "web_search"}],
            "tool_choice": {"type": "none"},
            "stream": false
        })
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;
    let response = client
        .post(&url)
        .bearer_auth(&credentials.cpa_token)
        .json(&payload)
        .send()
        .with_context(|| format!("search probe for {upstream_model} failed"))?;
    let status = response.status();
    let body = response.text().unwrap_or_default();
    Ok(classify_search_probe(status.as_u16(), &body, verify))
}

fn classify_search_probe(
    http_status: u16,
    body: &str,
    verify: bool,
) -> SearchCapabilityStatus {
    if (200..300).contains(&http_status) {
        if verify && body.contains("web_search_call") {
            return SearchCapabilityStatus::Verified;
        }
        return SearchCapabilityStatus::Supported;
    }
    let lower = body.to_lowercase();
    if matches!(http_status, 400 | 404 | 422)
        && (lower.contains("web_search")
            || lower.contains("unsupported")
            || lower.contains("unknown tool")
            || lower.contains("tool not"))
    {
        return SearchCapabilityStatus::Unsupported;
    }
    if http_status == 401 || http_status == 403 {
        return SearchCapabilityStatus::Error;
    }
    SearchCapabilityStatus::Error
}
