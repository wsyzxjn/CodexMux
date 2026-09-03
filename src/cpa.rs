use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use serde_yaml;
use tar::Archive;

use crate::{
    config::{Cpa, Paths},
    fsutil::{atomic_write, sha256_file},
};

/// GitHub repository that publishes the CLIProxyAPI release archive.
pub const CPA_REPO: &str = "router-for-me/CLIProxyAPI";
/// Offline archive fallback; normal `cpa install` resolves the latest stable release.
pub const CPA_VERSION: &str = "7.2.147";
/// sha256 of `CLIProxyAPI_7.2.147_darwin_aarch64.tar.gz`, matching the published digest.
pub const CPA_DARWIN_AARCH64_SHA256: &str =
    "4ac1db83b00591265ebb93a3277d812aaf6e45e8b21bb3b4786598520afdf4be";
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct InstalledVersion {
    pub version: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
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

/// Fetch model ids from a direct Responses endpoint without persisting them.
pub fn direct_model_slugs(base_url: &str, token: &str) -> Result<Vec<String>> {
    model_slugs(
        &Cpa {
            base_url: base_url.to_owned(),
        },
        token,
    )
}

// These files share this module's private namespace. Keeping the boundaries here
// avoids widening internal APIs merely to organize the implementation.
include!("cpa/install.rs");
include!("cpa/managed_config.rs");
include!("cpa/profiles.rs");
include!("cpa/service.rs");
include!("cpa/update.rs");
include!("cpa/search.rs");

#[cfg(test)]
include!("cpa/tests.rs");
