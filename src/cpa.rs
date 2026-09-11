use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
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
const CPA_MODEL_VALIDATION_TIMEOUT: Duration = Duration::from_secs(8);
const CPA_MODEL_VALIDATION_POLL_INTERVAL: Duration = Duration::from_millis(250);

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

fn wait_for_model_slugs(cpa: &Cpa, token: &str) -> Result<Vec<String>> {
    wait_for_model_slugs_with(
        || model_slugs(cpa, token),
        CPA_MODEL_VALIDATION_TIMEOUT,
        CPA_MODEL_VALIDATION_POLL_INTERVAL,
    )
}

fn wait_for_model_slugs_with<F>(
    mut fetch: F,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<Vec<String>>
where
    F: FnMut() -> Result<Vec<String>>,
{
    let deadline = Instant::now() + timeout;
    loop {
        match fetch() {
            Ok(slugs) => return Ok(slugs),
            Err(error) => {
                if Instant::now() >= deadline {
                    return Err(error).context(format!(
                        "CPA did not become ready within {} ms",
                        timeout.as_millis()
                    ));
                }
            }
        }
        std::thread::sleep(poll_interval);
    }
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

/// Best-effort discovery of the image models a CPA instance will accept.
///
/// CPA serves `/v1/images/*` but never lists image models in `/v1/models`, and
/// exposes no endpoint that enumerates them. The only machine-reachable source
/// is the rejection CPA returns for an unknown image model, which names the
/// ones it supports. This drives the menu bar picker only: routing accepts any
/// declared slug, so an empty or stale result never blocks a selection.
pub fn image_model_slugs(cpa: &Cpa, token: &str) -> Result<Vec<String>> {
    cpa.validate()?;
    let url = format!("{}/images/generations", cpa.base_url.trim_end_matches('/'));
    let response = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()?
        .post(&url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({"model": IMAGE_PROBE_MODEL}))
        .send()
        .with_context(|| format!("CPA endpoint {url} is unreachable"))?;
    let value: serde_json::Value = response.json().context("CPA returned invalid image JSON")?;
    let message = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
        .unwrap_or_default();
    Ok(image_slugs_from_message(message))
}

/// Sentinel model used only to make CPA name the image models it accepts.
const IMAGE_PROBE_MODEL: &str = "codexmux-image-probe";

/// Pull image model slugs out of CPA's rejection message. Deliberately
/// tolerant: an unrecognized message yields an empty list rather than an
/// error, because this only populates a menu.
fn image_slugs_from_message(message: &str) -> Vec<String> {
    let Some((_, listed)) = message.split_once("Use ") else {
        return Vec::new();
    };
    let mut slugs = listed
        .split(',')
        .flat_map(|part| part.split(" or "))
        .map(|part| part.trim().trim_end_matches('.').trim())
        .filter(|part| {
            !part.is_empty()
                && part.len() <= 64
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || ".-_".contains(c))
                && (part.contains("image") || part.contains("imagine"))
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    slugs.sort();
    slugs.dedup();
    slugs
}

/// Fetch model ids from a direct Responses endpoint without persisting them./// Fetch model ids from a direct Responses endpoint without persisting them.
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
