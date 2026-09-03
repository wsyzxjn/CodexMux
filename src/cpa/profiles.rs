/// A saved CPA endpoint: name, base URL, and the client token to use for it.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CpaProfile {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub token: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ProfileStore {
    /// Name of the currently active profile, if any.
    #[serde(rename = "active")]
    active: Option<String>,
    #[serde(rename = "profile", default)]
    profiles: Vec<CpaProfile>,
    /// Review model override: when set, `codex-auto-review` routes explicitly
    /// to this CPA model instead of the official route.
    #[serde(
        rename = "review_override",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    review_override: Option<String>,
    /// Shared Responses `web_search` backend selected from the menu bar.
    /// `None` means the process-level `config.toml` setting is authoritative.
    #[serde(rename = "search_backend", default, skip_serializing_if = "Option::is_none")]
    search_backend: Option<String>,
    /// Menu override for the shared search feature. `None` follows
    /// `config.toml`; `Some(false)` disables it even when config enables it.
    #[serde(
        rename = "search_backend_enabled",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    search_backend_enabled: Option<bool>,
    /// Direct routes: model slugs CodexMux proxies straight to an upstream,
    /// bypassing CPA entirely (used to sidestep CPA executor bugs).
    #[serde(
        rename = "direct-route",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    direct_routes: Vec<DirectRoute>,
    /// Whether the CPA service should run when CodexMux starts. Updated by
    /// `cpa start` / `cpa stop` so the menu bar follows the user's last choice.
    #[serde(
        rename = "cpa_autostart",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    cpa_autostart: Option<bool>,
}

/// A model slug routed by CodexMux itself, bypassing CPA.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DirectRoute {
    /// Upstream base URL (Responses API root, no trailing slash).
    pub base_url: String,
    /// Bearer token for the upstream.
    #[serde(default)]
    pub token: String,
    /// Local slugs exposed as `cpa/<model>` (stored without the prefix).
    #[serde(default)]
    pub models: Vec<String>,
    /// Local CPA aliases mapped to a different native Responses model id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_aliases: BTreeMap<String, String>,
}

fn load_profile_store(paths: &Paths) -> ProfileStore {
    load_profile_store_from(&paths.cpa_profiles)
}

fn load_profile_store_from(path: &Path) -> ProfileStore {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_profile_store(paths: &Paths, store: &ProfileStore) -> Result<()> {
    save_profile_store_to(&paths.cpa_profiles, store)
}

fn save_profile_store_to(path: &Path, store: &ProfileStore) -> Result<()> {
    atomic_write(path, toml::to_string_pretty(store)?.as_bytes())?;
    set_private(path)
}

/// List all saved CPA profiles and the active one.
pub fn profiles(paths: &Paths) -> (Option<String>, Vec<CpaProfile>) {
    let store = load_profile_store(paths);
    (store.active, store.profiles)
}

/// Add or update a profile by name; validates the endpoint shape only.
pub fn save_profile(paths: &Paths, profile: CpaProfile) -> Result<()> {
    anyhow::ensure!(
        !profile.name.trim().is_empty(),
        "profile name must not be empty"
    );
    profile_base_url(&Cpa {
        base_url: profile.base_url.clone(),
    })?;
    let mut store = load_profile_store(paths);
    if let Some(existing) = store.profiles.iter_mut().find(|p| p.name == profile.name) {
        *existing = profile;
    } else {
        store.profiles.push(profile);
    }
    save_profile_store(paths, &store)
}

/// Remove a saved profile. Refuses to remove the active one first.
pub fn remove_profile(paths: &Paths, name: &str) -> Result<()> {
    let mut store = load_profile_store(paths);
    let before = store.profiles.len();
    store.profiles.retain(|p| p.name != name);
    anyhow::ensure!(
        store.profiles.len() < before,
        "profile {name} does not exist"
    );
    if store.active.as_deref() == Some(name) {
        store.active = None;
    }
    save_profile_store(paths, &store)
}

/// Read the current review model override (None = the official route).
/// Reads the profile file on every call so menu-bar switches take effect
/// without restarting the proxy.
pub fn review_override(profiles_path: &Path) -> Option<String> {
    load_profile_store_from(profiles_path)
        .review_override
        .map(|slug| slug.trim().to_owned())
        .filter(|slug| !slug.is_empty())
}

/// Set or clear (`None`) the review model override and persist it.
pub fn set_review_override(profiles_path: &Path, slug: Option<String>) -> Result<()> {
    let mut store = load_profile_store_from(profiles_path);
    if let Some(slug) = &slug {
        anyhow::ensure!(
            !slug.trim().is_empty(),
            "review override slug must not be empty"
        );
        anyhow::ensure!(
            !slug.trim().starts_with(crate::config::CPA_MODEL_PREFIX),
            "review override uses the upstream slug without the cpa/ prefix"
        );
        // The live catalog is checked when a request uses this override so a
        // stale selection fails closed after an upstream catalog change.
        store.review_override = Some(slug.trim().to_owned());
    } else {
        store.review_override = None;
    }
    save_profile_store_to(profiles_path, &store)
}

/// A menu bar search-backend override. `None` means use `config.toml`.
#[derive(Clone, Debug)]
pub struct SearchBackendSetting {
    pub enabled: bool,
    pub backend_model: String,
}

/// Read the menu bar shared search override, if one has been persisted.
pub fn search_backend_setting(profiles_path: &Path) -> Option<SearchBackendSetting> {
    let store = load_profile_store_from(profiles_path);
    let enabled = store.search_backend_enabled?;
    let backend_model = store
        .search_backend
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
        .unwrap_or_default();
    if !enabled {
        return Some(SearchBackendSetting {
            enabled: false,
            backend_model: String::new(),
        });
    }
    if backend_model.is_empty() {
        return None;
    }
    Some(SearchBackendSetting {
        enabled: true,
        backend_model,
    })
}

/// Enable (`Some(slug)`), disable (`None`), or clear the menu override
/// (`default`) for the shared Responses web search backend.
pub fn set_search_backend_setting(
    profiles_path: &Path,
    override_kind: Option<Option<String>>,
) -> Result<()> {
    let mut store = load_profile_store_from(profiles_path);
    match override_kind {
        Some(Some(slug)) => {
            let slug = slug.trim().to_owned();
            anyhow::ensure!(!slug.is_empty(), "search backend slug must not be empty");
            store.search_backend = Some(slug);
            store.search_backend_enabled = Some(true);
        }
        Some(None) => {
            store.search_backend = None;
            store.search_backend_enabled = Some(false);
        }
        None => {
            store.search_backend = None;
            store.search_backend_enabled = None;
        }
    }
    save_profile_store_to(profiles_path, &store)
}

/// A matching direct route for a `cpa/`-prefixed slug, resolved per request.
pub struct DirectRouteMatch {
    pub base_url: String,
    pub token: String,
    pub upstream_model: String,
}

/// Find the direct route for a `cpa/`-prefixed slug (per-request read so
/// menu-bar edits apply without restarting the proxy).
pub fn direct_route_for(profiles_path: &Path, slug: &str) -> Result<Option<DirectRouteMatch>> {
    let store = load_profile_store_from(profiles_path);
    let mut matching = store
        .direct_routes
        .iter()
        .filter_map(|route| direct_upstream_model(route, slug).map(|upstream| (route, upstream)));
    let Some((route, upstream_model)) = matching.next() else {
        return Ok(None);
    };
    anyhow::ensure!(
        matching.next().is_none(),
        "direct route for cpa/{slug} is ambiguous"
    );
    validate_direct_route(route)?;
    Ok(Some(DirectRouteMatch {
        base_url: route.base_url.trim_end_matches('/').to_owned(),
        token: route.token.clone(),
        upstream_model: upstream_model.to_owned(),
    }))
}

/// Replace all direct routes and persist them.
pub fn set_direct_routes(paths: &Paths, routes: Vec<DirectRoute>) -> Result<()> {
    let credentials = crate::secrets::load(&paths.credentials)?;
    let mut models = HashSet::new();
    for route in &routes {
        validate_direct_route(route)?;
        anyhow::ensure!(
            route.token != credentials.proxy_token
                && route.token != credentials.cpa_token
                && route.token != credentials.cpa_management_key,
            "direct route token must be distinct from CodexMux credentials"
        );
        for (model, _) in direct_model_mappings(route) {
            anyhow::ensure!(
                models.insert(model),
                "direct route model {model} is duplicated"
            );
        }
    }
    let mut store = load_profile_store(paths);
    store.direct_routes = routes;
    save_profile_store(paths, &store)
}

fn validate_direct_route(route: &DirectRoute) -> Result<()> {
    profile_base_url(&Cpa {
        base_url: route.base_url.clone(),
    })?;
    anyhow::ensure!(
        !route.token.trim().is_empty(),
        "direct route token must not be empty"
    );
    let mappings: Vec<(&str, &str)> = direct_model_mappings(route).collect();
    anyhow::ensure!(!mappings.is_empty(), "direct route must contain models");
    anyhow::ensure!(
        mappings.iter().all(|(local, upstream)| {
            !local.trim().is_empty()
                && !upstream.trim().is_empty()
                && !local.trim().starts_with(crate::config::CPA_MODEL_PREFIX)
                && !upstream.trim().starts_with(crate::config::CPA_MODEL_PREFIX)
        }),
        "direct route models use nonempty slugs without the cpa/ prefix"
    );
    Ok(())
}

fn direct_model_mappings(route: &DirectRoute) -> impl Iterator<Item = (&str, &str)> {
    route
        .models
        .iter()
        .map(|model| (model.as_str(), model.as_str()))
        .chain(
            route
                .model_aliases
                .iter()
                .map(|(local, upstream)| (local.as_str(), upstream.as_str())),
        )
}

fn direct_upstream_model<'a>(route: &'a DirectRoute, local: &str) -> Option<&'a str> {
    route
        .model_aliases
        .get(local)
        .map(String::as_str)
        .or_else(|| {
            route
                .models
                .iter()
                .find(|model| model.as_str() == local)
                .map(String::as_str)
        })
}

/// List current direct routes.
pub fn direct_routes(paths: &Paths) -> Vec<DirectRoute> {
    load_profile_store(paths).direct_routes
}

/// Add one direct route entry (base URL + models), merging with any existing
/// route that shares the same base URL. Fails on model conflicts or invalid
/// shapes so the menu bar can surface the error.
pub fn add_direct_route(
    paths: &Paths,
    base_url: String,
    token: String,
    models: Vec<String>,
) -> Result<()> {
    let mut routes = direct_routes(paths);
    if let Some(existing) = routes.iter_mut().find(|route| route.base_url == base_url) {
        existing.token = token;
        for model in models {
            if !existing.models.contains(&model) {
                existing.model_aliases.remove(&model);
                existing.models.push(model);
            }
        }
    } else {
        routes.push(DirectRoute {
            base_url,
            token,
            models,
            model_aliases: BTreeMap::new(),
        });
    }
    set_direct_routes(paths, routes)
}

/// Add one local CPA slug that maps to a different native upstream model id.
pub fn add_direct_route_mapping(
    paths: &Paths,
    base_url: String,
    token: String,
    local_model: String,
    upstream_model: String,
) -> Result<()> {
    let mut routes = direct_routes(paths);
    if let Some(existing) = routes.iter_mut().find(|route| route.base_url == base_url) {
        existing.token = token;
        existing.models.retain(|model| model != &local_model);
        existing.model_aliases.insert(local_model, upstream_model);
    } else {
        routes.push(DirectRoute {
            base_url,
            token,
            models: Vec::new(),
            model_aliases: BTreeMap::from([(local_model, upstream_model)]),
        });
    }
    set_direct_routes(paths, routes)
}

/// Remove direct route entries by base URL, or just the given models from
/// them. An empty model list removes the whole entry.
pub fn remove_direct_routes(paths: &Paths, base_url: &str, models: &[String]) -> Result<()> {
    let mut routes = direct_routes(paths);
    for route in &mut routes {
        if route.base_url != base_url {
            continue;
        }
        if models.is_empty() {
            route.models.clear();
            route.model_aliases.clear();
        } else {
            route.models.retain(|model| !models.contains(model));
            route
                .model_aliases
                .retain(|local, _| !models.contains(local));
        }
    }
    routes.retain(|route| !route.models.is_empty() || !route.model_aliases.is_empty());
    set_direct_routes(paths, routes)
}

/// Read the persisted CPA autostart preference. When no explicit choice has
/// been saved yet, autostart defaults to enabled.
pub fn cpa_autostart(profiles_path: &Path) -> Option<bool> {
    load_profile_store_from(profiles_path)
        .cpa_autostart
        .or(Some(true))
}

/// Persist the CPA autostart preference.
pub fn set_cpa_autostart(profiles_path: &Path, enabled: bool) -> Result<()> {
    let mut store = load_profile_store_from(profiles_path);
    store.cpa_autostart = Some(enabled);
    save_profile_store_to(profiles_path, &store)
}

/// All model slugs declared by direct routes, for catalog merging.
pub fn declared_direct_models(profiles_path: &Path) -> Vec<crate::catalog::DirectModel> {
    load_profile_store_from(profiles_path)
        .direct_routes
        .iter()
        .flat_map(|route| {
            direct_model_mappings(route).map(|(local, _)| crate::catalog::DirectModel {
                upstream_model: local.to_owned(),
                base_url: route.base_url.clone(),
            })
        })
        .collect()
}

/// Validate that a profile endpoint is well-formed and reachable.
///
/// Sends the CPA models request with the profile token and requires a
/// successful response before switching.
fn validate_endpoint(base_url: &str, token: &str) -> Result<()> {
    let parsed = profile_base_url(&Cpa {
        base_url: base_url.to_owned(),
    })?;
    let models_url = format!("{}/models", parsed.trim_end_matches('/'));
    let mut request = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()?
        .get(&models_url);
    if !token.trim().is_empty() {
        request = request.header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = request
        .send()
        .with_context(|| format!("endpoint {base_url} is unreachable"))?;
    anyhow::ensure!(
        response.status().is_success(),
        "endpoint {base_url} responded with HTTP {}",
        response.status()
    );
    Ok(())
}

fn profile_base_url(cpa: &Cpa) -> Result<String> {
    cpa.validate()?;
    Ok(cpa.base_url.trim_end_matches('/').to_owned())
}

/// Switch the active CPA profile.
///
/// The candidate endpoint is validated BEFORE CodexMux's `config.toml` is
/// touched; on validation failure nothing changes. After switching, the local
/// CPA service is restarted with the new endpoint's token so the change takes
/// effect; if that restart fails the previous profile is restored.
pub fn switch_profile(paths: &Paths, name: &str) -> Result<()> {
    let store = load_profile_store(paths);
    let profile = store
        .profiles
        .iter()
        .find(|p| p.name == name)
        .with_context(|| format!("profile {name} does not exist"))?
        .clone();
    let previous_active = store.active.clone();
    let previous = previous_active
        .as_deref()
        .and_then(|active| store.profiles.iter().find(|p| p.name == active).cloned());

    validate_endpoint(&profile.base_url, &profile.token)
        .with_context(|| format!("profile {name} failed validation; not switching"))?;

    let cpa = Cpa {
        base_url: profile.base_url.clone(),
    };
    let switch_result = (|| -> Result<()> {
        write_config(paths, &cpa, &profile.token)?;
        restart(paths, &cpa, &profile.token)
    })();
    if let Err(error) = switch_result {
        // Roll back: restore the previous profile (or leave config untouched
        // if there was none) so CodexMux keeps pointing at a known state.
        if let Some(previous) = previous {
            let previous_cpa = Cpa {
                base_url: previous.base_url.clone(),
            };
            write_config(paths, &previous_cpa, &previous.token).ok();
            restart(paths, &previous_cpa, &previous.token).ok();
            let mut store = load_profile_store(paths);
            store.active = previous_active;
            save_profile_store(paths, &store).ok();
        }
        return Err(error).context(format!("switching to profile {name} failed; rolled back"));
    }

    let mut store = load_profile_store(paths);
    store.active = Some(profile.name.clone());
    save_profile_store(paths, &store)?;
    Ok(())
}
