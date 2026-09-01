use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use tar::Archive;

use crate::{
    config::{Cpa, Paths},
    fsutil::{atomic_write, sha256_file},
};

/// GitHub repository that publishes the CLIProxyAPI release archive.
pub const CPA_REPO: &str = "router-for-me/CLIProxyAPI";
/// Pin the release CodexMux installs so catalog and wire behavior stay predictable.
pub const CPA_VERSION: &str = "7.2.146";
/// sha256 of `CLIProxyAPI_7.2.146_darwin_aarch64.tar.gz`, matching the published digest.
pub const CPA_DARWIN_AARCH64_SHA256: &str =
    "faf4c735b289cb88344f87fd6d745cf9a11d28a231d000173d8045910503b543";
const CPA_BINARY_NAME: &str = "cli-proxy-api";
const CPA_AGENT_LABEL: &str = "dev.codexmux.cpa";

pub fn agent_label() -> &'static str {
    CPA_AGENT_LABEL
}

pub fn binary_path(paths: &Paths) -> PathBuf {
    paths.root.join("cpa/cli-proxy-api")
}

pub fn archive_path(paths: &Paths) -> PathBuf {
    paths.root.join("cpa/release.tar.gz")
}

pub fn config_path(paths: &Paths) -> PathBuf {
    paths.root.join("cpa/config.yaml")
}

fn management_html_path(paths: &Paths) -> PathBuf {
    paths.root.join("cpa/static/management.html")
}

fn version_path(paths: &Paths) -> PathBuf {
    paths.root.join("cpa/version.json")
}

fn plist_path() -> Result<PathBuf> {
    plist_path_for(CPA_AGENT_LABEL)
}

fn plist_path_for(label: &str) -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("cannot locate home directory")?
        .join(format!("Library/LaunchAgents/{label}.plist")))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstalledVersion {
    pub version: String,
    pub sha256: String,
}

pub fn installed_version(paths: &Paths) -> Option<InstalledVersion> {
    let bytes = fs::read(version_path(paths)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Fetch model slugs from the configured CPA endpoint without exposing its token.
pub fn model_slugs(cpa: &Cpa, token: &str) -> Result<Vec<String>> {
    cpa.validate()?;
    let url = format!("{}/models", cpa.base_url.trim_end_matches('/'));
    let response = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()?
        .get(&url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .with_context(|| format!("CPA endpoint {url} is unreachable"))?;
    anyhow::ensure!(
        response.status().is_success(),
        "CPA models request failed with HTTP {}",
        response.status()
    );
    let value: serde_json::Value = response.json().context("CPA returned invalid model JSON")?;
    model_slugs_from_value(&value)
}

fn model_slugs_from_value(value: &serde_json::Value) -> Result<Vec<String>> {
    let (models, key) = if let Some(models) = value.get("models").and_then(|value| value.as_array())
    {
        (models, "slug")
    } else if let Some(models) = value.get("data").and_then(|value| value.as_array()) {
        (models, "id")
    } else {
        bail!("CPA model response has no models or data array");
    };
    let mut slugs = models
        .iter()
        .filter_map(|model| model.get(key).and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|slug| !slug.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    slugs.sort();
    slugs.dedup();
    Ok(slugs)
}

fn asset_url(version: &str) -> String {
    format!(
        "https://github.com/{CPA_REPO}/releases/download/v{version}/CLIProxyAPI_{version}_darwin_aarch64.tar.gz"
    )
}

/// Download the pinned CPA release, verify its digest, and extract the binary.
///
/// The archive is fully streamed to disk first so the digest can be checked
/// before anything is executed; extraction only accepts the expected binary
/// entry and rejects anything else.
pub fn install(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let archive = archive_path(paths);
    let parent = archive
        .parent()
        .context("archive path has no parent directory")?;
    fs::create_dir_all(parent).context("failed to create the CPA install directory")?;
    let mut response = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()?
        .get(asset_url(CPA_VERSION))
        .send()
        .context("failed to download the CLIProxyAPI release archive")?;
    anyhow::ensure!(
        response.status().is_success(),
        "downloading the CLIProxyAPI release archive failed with HTTP {}",
        response.status()
    );
    let mut file = fs::File::create(&archive)
        .with_context(|| format!("failed to create {}", archive.display()))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .context("failed to stream the CLIProxyAPI release archive")?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
    }
    file.sync_all()?;
    install_from_archive(
        paths,
        cpa,
        &archive,
        CPA_VERSION,
        CPA_DARWIN_AARCH64_SHA256,
        token,
    )
}

/// Install from an already downloaded archive, verifying `sha256` first.
/// Used by `codexmux cpa install --archive <path>` for offline installs.
pub fn install_from_archive(
    paths: &Paths,
    cpa: &Cpa,
    archive: &Path,
    version: &str,
    sha256: &str,
    token: &str,
) -> Result<()> {
    let actual = sha256_file(archive)?;
    anyhow::ensure!(
        actual == sha256,
        "CLIProxyAPI archive digest mismatch: expected {sha256}, got {actual}"
    );
    let binary = extract_binary(archive)?;
    let binary_path = binary_path(paths);
    let was_loaded = is_loaded()?;
    if was_loaded {
        stop_service()?;
    }
    let install_result = (|| -> Result<()> {
        atomic_write(&binary_path, &binary)?;
        set_executable(&binary_path)?;
        atomic_write(
            &version_path(paths),
            &serde_json::to_vec_pretty(&InstalledVersion {
                version: version.to_owned(),
                sha256: sha256.to_owned(),
            })?,
        )
    })();
    if let Err(error) = install_result {
        if was_loaded {
            start(paths, cpa, token).ok();
        }
        return Err(error);
    }
    start(paths, cpa, token)
}

fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("failed to make {} executable", path.display()))
}

fn extract_binary(archive: &Path) -> Result<Vec<u8>> {
    let file =
        fs::File::open(archive).with_context(|| format!("failed to open {}", archive.display()))?;
    let mut archive = Archive::new(GzDecoder::new(file));
    for entry in archive
        .entries()
        .context("failed to read the CLIProxyAPI release archive")?
    {
        let mut entry = entry.context("failed to read the CLIProxyAPI release archive")?;
        let is_binary = entry
            .path()
            .context("invalid entry name in the CLIProxyAPI release archive")?
            .file_name()
            .is_some_and(|name| name == CPA_BINARY_NAME);
        if !is_binary {
            continue;
        }
        let mut binary = Vec::new();
        entry
            .read_to_end(&mut binary)
            .context("failed to extract the CLIProxyAPI binary")?;
        anyhow::ensure!(
            !binary.is_empty(),
            "CLIProxyAPI archive contains an empty binary"
        );
        return Ok(binary);
    }
    bail!("CLIProxyAPI archive does not contain {CPA_BINARY_NAME}")
}

/// Managed config marker; if a config exists without it, CodexMux never rewrites it.
const MANAGED_MARKER: &str = "# Managed by CodexMux";

fn port_of(cpa: &Cpa) -> u16 {
    reqwest::Url::parse(&cpa.base_url)
        .expect("validated CPA base_url")
        .port_or_known_default()
        .unwrap_or(80)
}

/// Write the managed `config.yaml` unless the user edited it away from CodexMux.
///
/// The file ends with a `# CodexMux providers` section that `cpa provider`
/// commands own. `write_config` preserves it byte-for-byte across rewrites.
pub fn write_config(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let management_key = crate::secrets::load(&paths.credentials)?.cpa_management_key;
    let path = config_path(paths);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if path.exists() {
        anyhow::ensure!(
            is_managed_config(&existing),
            "CPA config {} exists but is not managed by CodexMux; refusing to overwrite it",
            path.display()
        );
    }
    let providers = provider_section(&existing);
    let mut yaml = String::new();
    yaml.push_str(
        "# Managed by CodexMux; manual edits will be overwritten. Use `codexmux cpa` commands.\n",
    );
    yaml.push_str("host: \"127.0.0.1\"\n");
    yaml.push_str(&format!("port: {}\n", port_of(cpa)));
    yaml.push_str("remote-management:\n  allow-remote: false\n");
    yaml.push_str(&format!("  secret-key: \"{management_key}\"\n"));
    yaml.push_str("  disable-auto-update-panel: true\n");
    yaml.push_str("auth-dir: \"~/.cli-proxy-api\"\n");
    yaml.push_str("api-keys:\n");
    yaml.push_str(&format!("  - \"{token}\"\n"));
    yaml.push_str("debug: false\n");
    yaml.push_str("logging-to-file: false\n");
    yaml.push_str("usage-statistics-enabled: false\n");
    yaml.push_str(&providers);
    atomic_write(&path, yaml.as_bytes())
}

fn replace_management_key(config: &str, management_key: &str) -> Result<String> {
    let mut output = String::with_capacity(config.len() + management_key.len());
    let mut in_remote_management = false;
    let mut replaced = false;

    for line in config.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        if !content.starts_with(char::is_whitespace) {
            in_remote_management = content.trim_end() == "remote-management:";
        }
        if in_remote_management && content.trim_start().starts_with("secret-key:") {
            let indent = &content[..content.len() - content.trim_start().len()];
            output.push_str(indent);
            output.push_str("secret-key: \"");
            output.push_str(management_key);
            output.push('"');
            if line.ends_with('\n') {
                output.push('\n');
            }
            replaced = true;
        } else {
            output.push_str(line);
        }
    }

    anyhow::ensure!(
        replaced,
        "managed CPA config is missing remote-management.secret-key"
    );
    Ok(output)
}

/// Install the private management key into an existing managed CPA config.
/// If CPA is running, reload it so the copied key is immediately usable.
pub fn sync_management_key(paths: &Paths, management_key: &str) -> Result<()> {
    let path = config_path(paths);
    if !path.exists() {
        return Ok(());
    }
    let existing =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    anyhow::ensure!(
        is_managed_config(&existing),
        "CPA config {} exists but is not managed by CodexMux; refusing to overwrite it",
        path.display()
    );
    let replacement = replace_management_key(&existing, management_key)?;
    if replacement == existing {
        return Ok(());
    }

    let was_loaded = is_loaded()?;
    atomic_write(&path, replacement.as_bytes())?;
    set_private(&path)?;
    if !was_loaded {
        return Ok(());
    }

    if let Err(error) = stop_service() {
        atomic_write(&path, existing.as_bytes()).ok();
        set_private(&path).ok();
        return Err(error).context("failed to stop CPA for management-key reload; rolled back");
    }
    if let Err(error) = bootstrap_service(paths) {
        atomic_write(&path, existing.as_bytes()).ok();
        set_private(&path).ok();
        bootstrap_service(paths).ok();
        return Err(error).context("failed to reload CPA with the management key; rolled back");
    }
    Ok(())
}

const CONNECT_SCRIPT_ID: &str = "codexmux-connect";

fn connect_bootstrap_script() -> String {
    format!(
        r#"<script id="{CONNECT_SCRIPT_ID}">
(function(){{
  try {{
    var params = new URLSearchParams(window.location.search);
    var key = params.get("cmk");
    var base = params.get("cmb");
    if (!key && !base) return;
    if (base) localStorage.setItem("apiBase", JSON.stringify(base));
    if (key) {{
      localStorage.setItem("managementKey", JSON.stringify(key));
      localStorage.setItem("isLoggedIn", "true");
    }}
    params.delete("cmk");
    params.delete("cmb");
    var search = params.toString();
    history.replaceState(null, "", window.location.pathname + (search ? "?" + search : "") + window.location.hash);
  }} catch (e) {{}}
}})();
</script>"#
    )
}

fn inject_connect_bootstrap(html: &str) -> String {
    let script = connect_bootstrap_script();
    let marker = format!("<script id=\"{CONNECT_SCRIPT_ID}\">");
    if let Some(start) = html.find(&marker)
        && let Some(rel_end) = html[start..].find("</script>")
    {
        let end = start + rel_end + "</script>".len();
        let mut output = String::with_capacity(html.len() + script.len());
        output.push_str(&html[..start]);
        output.push_str(&script);
        output.push_str(&html[end..]);
        return output;
    }
    if let Some(head) = html.find("<head>") {
        let insert_at = head + "<head>".len();
        let mut output = String::with_capacity(html.len() + script.len() + 8);
        output.push_str(&html[..insert_at]);
        output.push('\n');
        output.push_str(&script);
        output.push_str(&html[insert_at..]);
        return output;
    }
    format!("{script}{html}")
}

/// Make the local CPA management page accept CodexMux login query parameters.
///
/// The bundled Web UI does not read connection info from the URL. CPA may also
/// overwrite this file on panel auto-update, so the bootstrap is reapplied
/// before each connect.
pub fn ensure_management_connect_bootstrap(paths: &Paths) -> Result<()> {
    let path = management_html_path(paths);
    if !path.is_file() {
        return Ok(());
    }
    let existing =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let rewritten = inject_connect_bootstrap(&existing);
    if rewritten != existing {
        atomic_write(&path, rewritten.as_bytes())?;
    }
    Ok(())
}

const PROVIDERS_HEADER: &str = "# CodexMux providers (managed by `codexmux cpa provider`)";

fn is_managed_config(config: &str) -> bool {
    config.contains(MANAGED_MARKER)
}

fn provider_section(config: &str) -> String {
    let start = match config.find(PROVIDERS_HEADER) {
        Some(index) => index,
        None => return format!("\n{PROVIDERS_HEADER}\n"),
    };
    config[start..].trim_end().to_owned() + "\n"
}

/// Replace the provider section with the contents of `providers_toml`.
///
/// The TOML is expected to hold `[[openai-compatibility]]`-style tables; it is
/// converted to the YAML list form CPA expects. Keys and secrets stay inside
/// the managed config, which the caller keeps mode-0600.
pub fn import_providers(paths: &Paths, providers_toml: &str) -> Result<()> {
    let yaml = providers_yaml(providers_toml)?;
    let path = config_path(paths);
    let mut config =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    anyhow::ensure!(
        is_managed_config(&config),
        "CPA config {} exists but is not managed by CodexMux; refusing to overwrite it",
        path.display()
    );
    let replacement = format!("\n{PROVIDERS_HEADER}\n{yaml}");
    match config.find(PROVIDERS_HEADER) {
        Some(index) => config.replace_range(index.., &replacement),
        None => config.push_str(&replacement),
    }
    atomic_write(&path, config.as_bytes())?;
    set_private(&path)
}

/// Restart CPA so config changes take effect; starts it if it was stopped.
pub fn restart(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    if is_loaded()? {
        stop_service()?;
    }
    // Internal restarts keep the user's startup preference untouched.
    start_service(paths, cpa, token)
}

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

/// Read the persisted CPA autostart preference (`None` = never chosen).
pub fn cpa_autostart(profiles_path: &Path) -> Option<bool> {
    load_profile_store_from(profiles_path).cpa_autostart
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

fn set_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("failed to restrict {}", path.display()))
}

/// Convert `[[openai-compatibility]]` / `[[codex-api-key]]` TOML tables to CPA YAML.
fn providers_yaml(providers_toml: &str) -> Result<String> {
    let value: toml::Value = toml::from_str(providers_toml).context("provider TOML is invalid")?;
    let mut yaml = String::new();
    for kind in ["openai-compatibility", "codex-api-key"] {
        let Some(entries) = value.get(kind).and_then(|v| v.as_array()) else {
            continue;
        };
        if entries.is_empty() {
            continue;
        }
        yaml.push_str(&format!("{kind}:\n"));
        for entry in entries {
            let table = entry
                .as_table()
                .context("each provider entry must be a table")?;
            for (index, (key, value)) in table.iter().enumerate() {
                let prefix = if index == 0 { "  - " } else { "    " };
                yaml.push_str(&entry_field(key, value, prefix, "    ")?);
            }
        }
    }
    anyhow::ensure!(
        !yaml.is_empty(),
        "provider TOML contains no provider tables"
    );
    Ok(yaml)
}

/// Render one provider field. `prefix` precedes this field (a `- ` list dash
/// for the first field of an entry, plain indentation otherwise) and `indent`
/// is the entry's base indent used for nested blocks.
fn entry_field(key: &str, value: &toml::Value, prefix: &str, indent: &str) -> Result<String> {
    let mut yaml = String::new();
    match value {
        toml::Value::String(text) => yaml.push_str(&format!("{prefix}{key}: {text:?}\n")),
        toml::Value::Integer(number) => yaml.push_str(&format!("{prefix}{key}: {number}\n")),
        toml::Value::Boolean(flag) => yaml.push_str(&format!("{prefix}{key}: {flag}\n")),
        toml::Value::Array(items) if items.iter().all(|item| item.is_str()) => {
            let names = items
                .iter()
                .map(|item| format!("{:?}", item.as_str().expect("checked above")))
                .collect::<Vec<_>>();
            yaml.push_str(&format!("{prefix}{key}: [{}]\n", names.join(", ")));
        }
        toml::Value::Array(items) if items.iter().all(|item| item.is_table()) => {
            yaml.push_str(&format!("{prefix}{key}:\n"));
            for item in items {
                let item = item.as_table().expect("checked above");
                for (nested_index, (nested_key, nested_value)) in item.iter().enumerate() {
                    let nested_prefix = if nested_index == 0 {
                        format!("{indent}  - ")
                    } else {
                        format!("{indent}    ")
                    };
                    yaml.push_str(&entry_field(
                        nested_key,
                        nested_value,
                        &nested_prefix,
                        &format!("{indent}    "),
                    )?);
                }
            }
        }
        toml::Value::Table(nested) => {
            yaml.push_str(&format!("{prefix}{key}:\n"));
            let nested_indent = format!("{indent}  ");
            for (nested_key, nested_value) in nested {
                yaml.push_str(&entry_field(
                    nested_key,
                    nested_value,
                    &nested_indent,
                    &nested_indent,
                )?);
            }
        }
        _ => bail!("unsupported value for {key} in provider entries"),
    }
    Ok(yaml)
}

/// Read the CPA port from the managed config, defaulting to 8317.
pub fn config_port(paths: &Paths) -> u16 {
    let text = fs::read_to_string(config_path(paths)).unwrap_or_default();
    text.lines()
        .find_map(|line| line.trim().strip_prefix("port:"))
        .and_then(|port| port.trim().parse().ok())
        .unwrap_or(8317)
}

pub fn start(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let result = start_service(paths, cpa, token);
    // An explicit start (CLI or menu bar) becomes the user's startup
    // preference, but only when the service actually ended up running.
    if is_loaded().unwrap_or(false) {
        set_cpa_autostart(&paths.cpa_profiles, true).ok();
    }
    result
}

fn start_service(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let previous_config = fs::read(config_path(paths)).ok();
    write_config(paths, cpa, token)?;
    let config_changed = previous_config.as_deref() != fs::read(config_path(paths)).ok().as_deref();
    if is_loaded()? && !config_changed {
        ensure_management_connect_bootstrap(paths)?;
        return Ok(());
    }
    stop_service()?;
    bootstrap_service(paths)?;
    ensure_management_connect_bootstrap(paths)
}

fn bootstrap_service(paths: &Paths) -> Result<()> {
    let binary = binary_path(paths);
    anyhow::ensure!(
        binary.is_file(),
        "CLIProxyAPI binary is not installed at {}; run codexmux cpa install",
        binary.display()
    );
    let logs = paths.root.join("logs");
    fs::create_dir_all(&logs)?;
    let document = render_agent(
        &binary,
        &config_path(paths),
        &logs.join("cpa-stdout.log"),
        &logs.join("cpa-stderr.log"),
    );
    let plist = plist_path()?;
    atomic_write(&plist, document.as_bytes())?;
    bootstrap(&plist)
}

pub fn stop(paths: &Paths) -> Result<()> {
    let result = stop_service();
    // An explicit stop (CLI or menu bar) becomes the user's startup
    // preference; the menu bar reads it when CodexMux next starts.
    if !is_loaded().unwrap_or(false) {
        set_cpa_autostart(&paths.cpa_profiles, false).ok();
    }
    result
}

/// Stop the service without touching the startup preference (app shutdown).
pub fn stop_service_only() -> Result<()> {
    stop_service()
}

fn stop_service() -> Result<()> {
    stop_label_if_loaded(CPA_AGENT_LABEL)
}

fn stop_label_if_loaded(label: &str) -> Result<()> {
    if !is_label_loaded(label)? {
        return Ok(());
    }
    let target = service_target_for(label)?;
    let status = Command::new("launchctl")
        .args(["bootout", &target])
        .status()
        .context("failed to run launchctl bootout")?;
    if !status.success() && is_label_loaded(label)? {
        bail!("launchctl bootout failed with {status}");
    }
    for _ in 0..100 {
        if !is_label_loaded(label)? {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    bail!("CPA LaunchAgent is still loaded after launchctl bootout")
}

pub fn is_loaded() -> Result<bool> {
    is_label_loaded(CPA_AGENT_LABEL)
}

fn is_label_loaded(label: &str) -> Result<bool> {
    let status = Command::new("launchctl")
        .args(["print", &service_target_for(label)?])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to inspect the CPA LaunchAgent")?;
    Ok(status.success())
}

pub fn uninstall() -> Result<Option<PathBuf>> {
    let plist = plist_path()?;
    let installed = plist.exists() || is_loaded()?;
    if !installed {
        return Ok(None);
    }
    stop_service()?;
    if plist.exists() {
        fs::remove_file(&plist)?;
        return Ok(Some(plist));
    }
    Ok(None)
}

fn bootstrap(plist: &Path) -> Result<()> {
    bootstrap_label(plist, CPA_AGENT_LABEL)
}

fn bootstrap_label(plist: &Path, label: &str) -> Result<()> {
    // A previously disabled override (from an earlier bootout) makes
    // bootstrap fail with I/O error 5; clear it first.
    let _ = Command::new("launchctl")
        .args(["enable", &service_target_for(label)?])
        .status();
    let status = Command::new("launchctl")
        .args([
            "bootstrap",
            &launch_domain()?,
            plist.to_string_lossy().as_ref(),
        ])
        .status()
        .context("failed to run launchctl bootstrap")?;
    if !status.success() {
        bail!("launchctl bootstrap failed with {status}");
    }
    Ok(())
}

fn service_target_for(label: &str) -> Result<String> {
    Ok(format!("{}/{label}", launch_domain()?))
}

fn launch_domain() -> Result<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to resolve user id")?;
    anyhow::ensure!(output.status.success(), "id -u failed");
    Ok(format!("gui/{}", String::from_utf8(output.stdout)?.trim()))
}

fn render_agent(binary: &Path, config: &Path, stdout: &Path, stderr: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{CPA_AGENT_LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>-config</string><string>{}</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Background</string>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
        xml(binary),
        xml(config),
        xml(stdout),
        xml(stderr),
    )
}

fn xml(path: &Path) -> String {
    path.to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Paths {
        let paths = Paths::from_root(tempfile::tempdir().unwrap().keep());
        crate::secrets::save(
            &paths.credentials,
            &crate::config::Credentials {
                proxy_token: "proxy-token".into(),
                cpa_token: "cpa-token".into(),
                cpa_management_key: "management-key".into(),
            },
        )
        .unwrap();
        paths
    }

    fn cpa_local(port: u16) -> Cpa {
        Cpa {
            base_url: format!("http://127.0.0.1:{port}/v1"),
        }
    }

    #[test]
    fn extract_accepts_only_the_expected_binary_entry() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("release.tar.gz");
        fs::write(
            &archive,
            include_bytes!("../tests/fixtures/cpa_release/mini_release.tar.gz"),
        )
        .unwrap();
        let binary = extract_binary(&archive).unwrap();
        assert_eq!(binary, b"#!/bin/sh\necho fake-cli-proxy-api\n");
    }

    #[test]
    fn model_slug_parser_accepts_native_and_openai_catalog_shapes() {
        assert_eq!(
            model_slugs_from_value(&serde_json::json!({
                "models": [{"slug":"b"}, {"slug":"a"}, {"slug":"a"}]
            }))
            .unwrap(),
            ["a", "b"]
        );
        assert_eq!(
            model_slugs_from_value(&serde_json::json!({
                "data": [{"id":"model-b"}, {"id":"model-a"}]
            }))
            .unwrap(),
            ["model-a", "model-b"]
        );
    }

    #[test]
    fn direct_routes_validate_endpoint_models_and_token_isolation() {
        let paths = paths();
        let route = DirectRoute {
            base_url: "http://127.0.0.1:9000/v1".into(),
            token: "direct-token".into(),
            models: vec!["model-a".into()],
            model_aliases: BTreeMap::new(),
        };
        set_direct_routes(&paths, vec![route]).unwrap();
        let direct = direct_route_for(&paths.cpa_profiles, "model-a")
            .unwrap()
            .unwrap();
        assert_eq!(direct.base_url, "http://127.0.0.1:9000/v1");
        assert_eq!(direct.token, "direct-token");
        assert_eq!(direct.upstream_model, "model-a");

        add_direct_route_mapping(
            &paths,
            "http://127.0.0.1:9000/v1".into(),
            "direct-token".into(),
            "local-alias".into(),
            "provider/native-model".into(),
        )
        .unwrap();
        let mapped = direct_route_for(&paths.cpa_profiles, "local-alias")
            .unwrap()
            .unwrap();
        assert_eq!(mapped.upstream_model, "provider/native-model");
        assert!(
            declared_direct_models(&paths.cpa_profiles)
                .iter()
                .any(|model| model.upstream_model == "local-alias")
        );

        let remote_http = DirectRoute {
            base_url: "http://example.com/v1".into(),
            token: "other-direct-token".into(),
            models: vec!["model-b".into()],
            model_aliases: BTreeMap::new(),
        };
        assert!(set_direct_routes(&paths, vec![remote_http]).is_err());

        let shared_token = DirectRoute {
            base_url: "https://example.com/v1".into(),
            token: "cpa-token".into(),
            models: vec!["model-b".into()],
            model_aliases: BTreeMap::new(),
        };
        assert!(set_direct_routes(&paths, vec![shared_token]).is_err());

        let prefixed_model = DirectRoute {
            base_url: "https://example.com/v1".into(),
            token: "other-direct-token".into(),
            models: vec!["cpa/model-b".into()],
            model_aliases: BTreeMap::new(),
        };
        assert!(set_direct_routes(&paths, vec![prefixed_model]).is_err());
        assert!(set_review_override(&paths.cpa_profiles, Some("cpa/model-b".into())).is_err());
    }

    #[test]
    fn direct_routes_add_and_remove_single_entries() {
        let paths = paths();
        add_direct_route(
            &paths,
            "http://127.0.0.1:9000/v1".into(),
            "direct-token".into(),
            vec!["model-a".into()],
        )
        .unwrap();
        // Same base URL merges models; a second route is not created.
        add_direct_route(
            &paths,
            "http://127.0.0.1:9000/v1".into(),
            "direct-token".into(),
            vec!["model-b".into()],
        )
        .unwrap();
        let routes = direct_routes(&paths);
        assert_eq!(routes.len(), 1);
        assert_eq!(
            routes[0].models,
            vec!["model-a".to_string(), "model-b".to_string()]
        );

        add_direct_route_mapping(
            &paths,
            "http://127.0.0.1:9000/v1".into(),
            "direct-token".into(),
            "local-c".into(),
            "native-c".into(),
        )
        .unwrap();
        assert_eq!(
            direct_routes(&paths)[0].model_aliases["local-c"],
            "native-c"
        );

        // Removing one model keeps the other; removing all drops the entry.
        remove_direct_routes(&paths, "http://127.0.0.1:9000/v1", &["model-a".into()]).unwrap();
        assert_eq!(direct_routes(&paths)[0].models, vec!["model-b".to_string()]);
        remove_direct_routes(&paths, "http://127.0.0.1:9000/v1", &["model-b".into()]).unwrap();
        assert_eq!(direct_routes(&paths).len(), 1);
        remove_direct_routes(&paths, "http://127.0.0.1:9000/v1", &["local-c".into()]).unwrap();
        assert!(direct_routes(&paths).is_empty());

        // An empty model list removes the whole entry.
        add_direct_route(
            &paths,
            "http://127.0.0.1:9001/v1".into(),
            "direct-token".into(),
            vec!["model-c".into()],
        )
        .unwrap();
        remove_direct_routes(&paths, "http://127.0.0.1:9001/v1", &[]).unwrap();
        assert!(direct_routes(&paths).is_empty());
    }

    #[test]
    fn cpa_autostart_preference_round_trips() {
        let paths = paths();
        assert_eq!(cpa_autostart(&paths.cpa_profiles), None);
        set_cpa_autostart(&paths.cpa_profiles, true).unwrap();
        assert_eq!(cpa_autostart(&paths.cpa_profiles), Some(true));
        set_cpa_autostart(&paths.cpa_profiles, false).unwrap();
        assert_eq!(cpa_autostart(&paths.cpa_profiles), Some(false));
    }

    #[test]
    fn managed_config_writes_loopback_port_and_token() {
        let paths = paths();
        write_config(&paths, &cpa_local(8317), "token-a").unwrap();
        let text = fs::read_to_string(config_path(&paths)).unwrap();
        assert!(text.contains(MANAGED_MARKER));
        assert!(text.contains("host: \"127.0.0.1\""));
        assert!(text.contains("port: 8317"));
        assert!(text.contains("  secret-key: \"management-key\"\n"));
        assert!(text.contains("  disable-auto-update-panel: true\n"));
        assert!(text.contains("  - \"token-a\"\n"));
        assert_eq!(config_port(&paths), 8317);
    }

    #[test]
    fn management_key_replacement_only_touches_remote_management() {
        let config = concat!(
            "# Managed by CodexMux\n",
            "remote-management:\n",
            "  allow-remote: false\n",
            "  secret-key: \"old\"\n",
            "provider:\n",
            "  secret-key: \"provider-secret\"\n",
        );
        let replaced = replace_management_key(config, "new-management-key").unwrap();
        assert!(replaced.contains("  secret-key: \"new-management-key\"\n"));
        assert!(replaced.contains("  secret-key: \"provider-secret\"\n"));
        assert!(!replaced.contains("  secret-key: \"old\"\n"));
    }

    #[test]
    fn management_connect_bootstrap_is_injected_once() {
        let html = "<!doctype html>\n<html>\n  <head>\n    <meta charset=\"UTF-8\" />\n  </head>\n</html>\n";
        let first = inject_connect_bootstrap(html);
        assert!(first.contains(CONNECT_SCRIPT_ID));
        assert!(first.contains("params.get(\"cmk\")"));
        assert_eq!(first.matches(CONNECT_SCRIPT_ID).count(), 1);

        let second = inject_connect_bootstrap(&first);
        assert_eq!(second.matches(CONNECT_SCRIPT_ID).count(), 1);
        assert_eq!(second.matches("<head>").count(), 1);
    }

    #[test]
    fn hand_edited_config_is_never_overwritten() {
        let paths = paths();
        write_config(&paths, &cpa_local(8317), "token-a").unwrap();
        fs::write(config_path(&paths), "# my own CPA config\nport: 9999\n").unwrap();
        let error = write_config(&paths, &cpa_local(8317), "token-a").unwrap_err();
        assert!(error.to_string().contains("refusing to overwrite"));
    }

    #[test]
    fn provider_section_survives_config_rewrites() {
        let paths = paths();
        // First write has no provider section yet.
        write_config(&paths, &cpa_local(8317), "token-a").unwrap();
        let section =
            format!("\n{PROVIDERS_HEADER}\nopenai-compatibility:\n  - name: \"example\"\n");
        let base = fs::read_to_string(config_path(&paths)).unwrap();
        // A rewrite that already contains a provider section keeps exactly one.
        let with_providers = base.replace(&format!("\n{PROVIDERS_HEADER}\n"), &section);
        fs::write(config_path(&paths), with_providers).unwrap();
        write_config(&paths, &cpa_local(9317), "token-b").unwrap();
        let rewritten = fs::read_to_string(config_path(&paths)).unwrap();
        assert!(rewritten.contains("port: 9317"));
        assert!(rewritten.contains("token-b"));
        assert!(rewritten.contains(section.trim_end()));
        assert_eq!(rewritten.matches(PROVIDERS_HEADER).count(), 1);
    }

    #[test]
    fn import_providers_converts_toml_tables_to_cpa_yaml() {
        let toml = r#"
[[openai-compatibility]]
name = "kunbot-ris"
base-url = "https://example.com/ris/v1"

[[openai-compatibility.models]]
name = "zai-org/GLM-5.3-Flash"
alias = "glm-5.3-flash"
"#
        .trim()
        .to_owned();
        let yaml = providers_yaml(&toml).unwrap();
        assert!(yaml.starts_with("openai-compatibility:\n"));
        assert!(yaml.contains("name: \"kunbot-ris\"\n"));
        assert!(yaml.contains("base-url: \"https://example.com/ris/v1\"\n"));
        // The rendered YAML must parse back as a list of provider entries.
        let parsed =
            serde_yaml::from_str::<serde_yaml::Value>(&yaml).expect("rendered YAML is valid");
        let providers = parsed
            .get("openai-compatibility")
            .and_then(|value| value.as_sequence())
            .expect("providers parse as a list");
        assert_eq!(providers.len(), 1);
        assert_eq!(
            providers[0].get("name").and_then(|v| v.as_str()),
            Some("kunbot-ris")
        );
        assert_eq!(
            providers[0].get("base-url").and_then(|v| v.as_str()),
            Some("https://example.com/ris/v1")
        );
        let models = providers[0]
            .get("models")
            .and_then(|value| value.as_sequence())
            .expect("models parse as a list");
        assert_eq!(models.len(), 1);
        assert_eq!(
            models[0].get("name").and_then(|v| v.as_str()),
            Some("zai-org/GLM-5.3-Flash")
        );
        assert_eq!(
            models[0].get("alias").and_then(|v| v.as_str()),
            Some("glm-5.3-flash")
        );
    }

    #[test]
    fn import_providers_rewrites_the_section_and_keeps_the_rest() {
        let paths = paths();
        write_config(&paths, &cpa_local(8317), "token-a").unwrap();
        let toml = "
[[codex-api-key]]
api-key = \"sk-test\"
base-url = \"https://example.com/acid/v1\"

[[codex-api-key.models]]
name = \"gpt-5.6-terra\"
alias = \"gpt-5.6-terra\"
"
        .trim()
        .to_owned();
        import_providers(&paths, &toml).unwrap();
        let config = fs::read_to_string(config_path(&paths)).unwrap();
        assert!(config.contains("port: 8317"));
        assert!(config.contains(PROVIDERS_HEADER));
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&config).expect("managed config stays valid YAML");
        let codex_keys = parsed
            .get("codex-api-key")
            .and_then(|value| value.as_sequence())
            .expect("codex-api-key parses as a list");
        assert_eq!(codex_keys.len(), 1);
        assert_eq!(
            codex_keys[0].get("api-key").and_then(|v| v.as_str()),
            Some("sk-test")
        );
        let models = codex_keys[0]
            .get("models")
            .and_then(|value| value.as_sequence())
            .expect("models parse as a list");
        assert_eq!(models.len(), 1);
        assert_eq!(
            models[0].get("alias").and_then(|v| v.as_str()),
            Some("gpt-5.6-terra")
        );
        assert_eq!(config.matches(PROVIDERS_HEADER).count(), 1);
        // Re-import replaces rather than appends.
        import_providers(&paths, &toml).unwrap();
        let again = fs::read_to_string(config_path(&paths)).unwrap();
        assert_eq!(again.matches("codex-api-key:").count(), 1);
    }

    #[test]
    fn plist_renders_config_path_and_escapes_paths() {
        let plist = render_agent(
            Path::new("/tmp/a&b/cli-proxy-api"),
            Path::new("/tmp/root/cpa/config.yaml"),
            Path::new("/tmp/root/logs/out.log"),
            Path::new("/tmp/root/logs/err.log"),
        );
        assert!(plist.contains("/tmp/a&amp;b/cli-proxy-api"));
        assert!(plist.contains("<string>-config</string>"));
        assert!(plist.contains("/tmp/root/cpa/config.yaml"));
        assert!(plist.contains("dev.codexmux.cpa"));
    }

    #[test]
    fn profiles_save_list_and_remove() {
        let paths = paths();
        save_profile(
            &paths,
            CpaProfile {
                name: "local".into(),
                base_url: "http://127.0.0.1:8317/v1".into(),
                token: "token-a".into(),
            },
        )
        .unwrap();
        save_profile(
            &paths,
            CpaProfile {
                name: "remote".into(),
                base_url: "https://cpa.example.com/v1".into(),
                token: "token-b".into(),
            },
        )
        .unwrap();
        let (active, saved) = profiles(&paths);
        assert_eq!(active, None);
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].name, "local");

        // Updating an existing name replaces the entry instead of duplicating.
        save_profile(
            &paths,
            CpaProfile {
                name: "local".into(),
                base_url: "http://127.0.0.1:9317/v1".into(),
                token: "token-a2".into(),
            },
        )
        .unwrap();
        let (_, updated) = profiles(&paths);
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[0].base_url, "http://127.0.0.1:9317/v1");

        remove_profile(&paths, "local").unwrap();
        let (_, remaining) = profiles(&paths);
        assert_eq!(remaining.len(), 1);
        let error = remove_profile(&paths, "local").unwrap_err();
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn profile_validation_rejects_bad_urls_before_any_switch() {
        let paths = paths();
        // Remote HTTP is rejected at save time by the endpoint shape check.
        let error = save_profile(
            &paths,
            CpaProfile {
                name: "bad".into(),
                base_url: "http://external.example.com/v1".into(),
                token: String::new(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("must use HTTPS"));
        // Nothing was switched: no active profile recorded.
        let (active, _) = profiles(&paths);
        assert_eq!(active, None);
        // An unreachable-but-valid endpoint fails switch validation.
        save_profile(
            &paths,
            CpaProfile {
                name: "unreachable".into(),
                base_url: "https://cpa-unreachable.example.com/v1".into(),
                token: String::new(),
            },
        )
        .unwrap();
        let error = switch_profile(&paths, "unreachable").unwrap_err();
        assert!(error.to_string().contains("failed validation"));
        let (active, _) = profiles(&paths);
        assert_eq!(active, None);
    }

    #[test]
    fn switch_to_unknown_profile_fails_without_changes() {
        let paths = paths();
        let error = switch_profile(&paths, "missing").unwrap_err();
        assert!(error.to_string().contains("does not exist"));
        let (active, _) = profiles(&paths);
        assert_eq!(active, None);
    }
}
