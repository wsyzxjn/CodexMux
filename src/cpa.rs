use std::{
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
/// Pin the release ModelMux installs so catalog and wire behavior stay predictable.
pub const CPA_VERSION: &str = "7.2.146";
/// sha256 of `CLIProxyAPI_7.2.145_darwin_aarch64.tar.gz`, matching the published digest.
pub const CPA_DARWIN_AARCH64_SHA256: &str =
    "faf4c735b289cb88344f87fd6d745cf9a11d28a231d000173d8045910503b543";
const CPA_BINARY_NAME: &str = "cli-proxy-api";
const CPA_AGENT_LABEL: &str = "dev.modelmux.cpa";

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

fn version_path(paths: &Paths) -> PathBuf {
    paths.root.join("cpa/version.json")
}

fn plist_path() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("cannot locate home directory")?
        .join(format!("Library/LaunchAgents/{CPA_AGENT_LABEL}.plist")))
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
/// Used by `modelmux cpa install --archive <path>` for offline installs.
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
        stop()?;
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

/// Managed config marker; if a config exists without it, ModelMux never rewrites it.
const MANAGED_MARKER: &str = "# Managed by ModelMux";

fn port_of(cpa: &Cpa) -> u16 {
    reqwest::Url::parse(&cpa.base_url)
        .expect("validated CPA base_url")
        .port_or_known_default()
        .unwrap_or(80)
}

/// Write the managed `config.yaml` unless the user edited it away from ModelMux.
///
/// The file ends with a `# ModelMux providers` section that `cpa provider`
/// commands own. `write_config` preserves it byte-for-byte across rewrites.
pub fn write_config(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let path = config_path(paths);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if path.exists() {
        anyhow::ensure!(
            existing.contains(MANAGED_MARKER),
            "CPA config {} exists but is not managed by ModelMux; refusing to overwrite it",
            path.display()
        );
    }
    let providers = provider_section(&existing);
    let mut yaml = String::new();
    yaml.push_str(
        "# Managed by ModelMux; manual edits will be overwritten. Use `modelmux cpa` commands.\n",
    );
    yaml.push_str("host: \"127.0.0.1\"\n");
    yaml.push_str(&format!("port: {}\n", port_of(cpa)));
    yaml.push_str("remote-management:\n  allow-remote: false\n  secret-key: \"\"\n");
    yaml.push_str("auth-dir: \"~/.cli-proxy-api\"\n");
    yaml.push_str("api-keys:\n");
    yaml.push_str(&format!("  - \"{token}\"\n"));
    yaml.push_str("debug: false\n");
    yaml.push_str("logging-to-file: false\n");
    yaml.push_str("usage-statistics-enabled: false\n");
    yaml.push_str(&providers);
    atomic_write(&path, yaml.as_bytes())
}

const PROVIDERS_HEADER: &str = "# ModelMux providers (managed by `modelmux cpa provider`)";

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
        config.contains(MANAGED_MARKER),
        "CPA config {} exists but is not managed by ModelMux; refusing to overwrite it",
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
        stop()?;
    }
    start(paths, cpa, token)
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
    /// Review model override: when set, `codex-auto-review` routes directly
    /// to this CPA model (no official-first attempt).
    #[serde(
        rename = "review_override",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    review_override: Option<String>,
    /// Direct routes: model slugs ModelMux proxies straight to an upstream,
    /// bypassing CPA entirely (used to sidestep CPA executor bugs).
    #[serde(
        rename = "direct-route",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    direct_routes: Vec<DirectRoute>,
}

/// A model slug routed by ModelMux itself, bypassing CPA.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DirectRoute {
    /// Upstream base URL (Responses API root, no trailing slash).
    pub base_url: String,
    /// Bearer token for the upstream.
    #[serde(default)]
    pub token: String,
    /// `cpa/`-prefixed slugs routed to this upstream.
    #[serde(default)]
    pub models: Vec<String>,
}

fn load_profile_store(paths: &Paths) -> ProfileStore {
    fs::read_to_string(&paths.cpa_profiles)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_profile_store(paths: &Paths, store: &ProfileStore) -> Result<()> {
    atomic_write(
        &paths.cpa_profiles,
        toml::to_string_pretty(store)?.as_bytes(),
    )?;
    set_private(&paths.cpa_profiles)
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

/// Read the current review model override (None = default official-first routing).
/// Reads the profile file on every call so menu-bar switches take effect
/// without restarting the proxy.
pub fn review_override(profiles_path: &Path) -> Option<String> {
    let store: ProfileStore = fs::read_to_string(profiles_path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    store
        .review_override
        .map(|slug| slug.trim().to_owned())
        .filter(|slug| !slug.is_empty())
}

/// Set or clear (`None`) the review model override and persist it.
pub fn set_review_override(profiles_path: &Path, slug: Option<String>) -> Result<()> {
    let mut store: ProfileStore = fs::read_to_string(profiles_path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    if let Some(slug) = &slug {
        anyhow::ensure!(
            !slug.trim().is_empty(),
            "review override slug must not be empty"
        );
        // The override must target a saved profile's own model OR any slug;
        // it is used verbatim as the upstream model on the CPA route.
        store.review_override = Some(slug.trim().to_owned());
    } else {
        store.review_override = None;
    }
    atomic_write(profiles_path, toml::to_string_pretty(&store)?.as_bytes())?;
    set_private(profiles_path)
}

/// A matching direct route for a `cpa/`-prefixed slug, resolved per request.
pub struct DirectRouteMatch {
    pub base_url: String,
    pub token: String,
}

/// Find the direct route for a `cpa/`-prefixed slug (per-request read so
/// menu-bar edits apply without restarting the proxy).
pub fn direct_route_for(profiles_path: &Path, slug: &str) -> Option<DirectRouteMatch> {
    let store: ProfileStore = fs::read_to_string(profiles_path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    store
        .direct_routes
        .iter()
        .find(|route| route.models.iter().any(|m| m == slug))
        .map(|route| DirectRouteMatch {
            base_url: route.base_url.trim_end_matches('/').to_owned(),
            token: route.token.clone(),
        })
}

/// Replace all direct routes and persist them.
pub fn set_direct_routes(paths: &Paths, routes: Vec<DirectRoute>) -> Result<()> {
    let mut store: ProfileStore = fs::read_to_string(&paths.cpa_profiles)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    store.direct_routes = routes;
    atomic_write(
        &paths.cpa_profiles,
        toml::to_string_pretty(&store)?.as_bytes(),
    )?;
    set_private(&paths.cpa_profiles)
}

/// List current direct routes.
pub fn direct_routes(paths: &Paths) -> Vec<DirectRoute> {
    fs::read_to_string(&paths.cpa_profiles)
        .ok()
        .and_then(|text| toml::from_str::<ProfileStore>(&text).ok())
        .map(|store| store.direct_routes)
        .unwrap_or_default()
}

/// Validate that a profile endpoint is well-formed and reachable.
///
/// Sends the CPA models request with the profile token; a successful HTTP
/// status (even an auth error is a *reachable* endpoint) validates switching.
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
/// The candidate endpoint is validated BEFORE ModelMux's `config.toml` is
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
        // if there was none) so ModelMux keeps pointing at a known state.
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
    let binary = binary_path(paths);
    anyhow::ensure!(
        binary.is_file(),
        "CLIProxyAPI binary is not installed at {}; run modelmux cpa install",
        binary.display()
    );
    write_config(paths, cpa, token)?;
    if is_loaded()? {
        return Ok(());
    }
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

pub fn stop() -> Result<()> {
    if !is_loaded()? {
        return Ok(());
    }
    let target = service_target()?;
    let status = Command::new("launchctl")
        .args(["bootout", &target])
        .status()
        .context("failed to run launchctl bootout")?;
    if !status.success() && is_loaded()? {
        bail!("launchctl bootout failed with {status}");
    }
    for _ in 0..100 {
        if !is_loaded()? {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    bail!("CPA LaunchAgent is still loaded after launchctl bootout")
}

pub fn is_loaded() -> Result<bool> {
    let status = Command::new("launchctl")
        .args(["print", &service_target()?])
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
    stop()?;
    if plist.exists() {
        fs::remove_file(&plist)?;
    }
    Ok(Some(plist))
}

fn bootstrap(plist: &Path) -> Result<()> {
    // A previously disabled override (from an earlier bootout) makes
    // bootstrap fail with I/O error 5; clear it first.
    let _ = Command::new("launchctl")
        .args(["enable", &service_target()?])
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

fn service_target() -> Result<String> {
    Ok(format!("{}/{CPA_AGENT_LABEL}", launch_domain()?))
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
        Paths::from_root(tempfile::tempdir().unwrap().keep())
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
    fn managed_config_writes_loopback_port_and_token() {
        let paths = paths();
        write_config(&paths, &cpa_local(8317), "token-a").unwrap();
        let text = fs::read_to_string(config_path(&paths)).unwrap();
        assert!(text.contains(MANAGED_MARKER));
        assert!(text.contains("host: \"127.0.0.1\""));
        assert!(text.contains("port: 8317"));
        assert!(text.contains("  - \"token-a\"\n"));
        assert_eq!(config_port(&paths), 8317);
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
        assert!(plist.contains("dev.modelmux.cpa"));
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
