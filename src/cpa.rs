//! Management of the local CLIProxyAPI (CPA) instance and of the settings the
//! menu bar persists in `cpa-profiles.toml`.

mod install;
mod managed_config;
mod profiles;
mod search;
mod service;
mod update;

use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    config::{Cpa, Paths},
    fsutil::{FileLock, try_lock_exclusive_for},
};

pub use install::{install, install_from_archive};
pub use managed_config::{
    ensure_management_connect_bootstrap, import_providers, sync_management_key,
};
pub use profiles::{
    CpaProfile, ProfileSettings, SearchBackendSetting, catalog_model_overrides, cpa_autostart,
    image_override, profile_settings, profiles, remove_profile, review_override, save_profile,
    search_backend_setting, set_cpa_autostart, set_image_override, set_review_override,
    set_search_backend_setting, switch_profile,
};
pub use search::{
    SearchCapability, SearchCapabilityStatus, SearchCapabilityStore, catalog_slugs,
    detect_search_capabilities, load_search_capabilities,
};
pub use service::{
    UninstallOutcome, is_loaded, start, start_service_only, stop, stop_service_only, uninstall,
};
pub use update::{
    UpdateCheck, UpdateOutcome, check_cpa_update, rollback_available, rollback_cpa, update_cpa,
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
/// How long a CPA operation waits for another one to finish.
const CPA_LOCK_WAIT: Duration = Duration::from_secs(60);
const CPA_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub fn agent_label() -> &'static str {
    CPA_AGENT_LABEL
}

pub fn binary_path(paths: &Paths) -> PathBuf {
    paths.root.join("cpa/cli-proxy-api")
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

/// The CPA job definition lives with the binary rather than in
/// `~/Library/LaunchAgents`, so launchd never loads CPA at login; only
/// CodexMux bootstraps it.
fn plist_path(paths: &Paths) -> PathBuf {
    paths.root.join(format!("cpa/{CPA_AGENT_LABEL}.plist"))
}

fn lock_path(paths: &Paths) -> PathBuf {
    paths.root.join("cpa/cpa.lock")
}

/// Serialize every operation that changes the CPA binary, its version record,
/// its job definition, or its launchd state across CodexMux processes.
fn lock(paths: &Paths) -> Result<FileLock> {
    lock_within(paths, CPA_LOCK_WAIT)
}

fn lock_within(paths: &Paths, timeout: Duration) -> Result<FileLock> {
    let path = lock_path(paths);
    try_lock_exclusive_for(&path, timeout, CPA_LOCK_POLL_INTERVAL)?.with_context(|| {
        format!(
            "another CodexMux CPA operation is still running (waited {timeout:?} for {}); try again when it finishes",
            path.display()
        )
    })
}

/// The release platform CodexMux can install CPA on.
fn supported_platform() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("darwin_aarch64"),
        (os, arch) => bail!(
            "CPA installs are not supported on {os}/{arch}; CodexMux supports macOS Apple Silicon only"
        ),
    }
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

/// The installed CPA version record; `None` when CPA is not installed.
pub fn installed_version(paths: &Paths) -> Result<Option<InstalledVersion>> {
    let path = version_path(paths);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .with_context(|| format!("invalid CPA version record {}", path.display()))
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
        .bearer_auth(token)
        .send()
        .with_context(|| format!("CPA endpoint {url} is unreachable"))?;
    anyhow::ensure!(
        response.status().is_success(),
        "CPA models request to {url} failed with HTTP {}",
        response.status()
    );
    let value: serde_json::Value = response.json().context("CPA returned invalid model JSON")?;
    model_slugs_from_value(&value)
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
        .bearer_auth(token)
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

/// Test fixtures shared by the CPA submodules. Every test gets its own data
/// root and a recording [`service::ServiceControl`], so no test ever talks to
/// launchd or the network.
#[cfg(test)]
mod test_support {
    use std::{cell::RefCell, collections::VecDeque, path::Path};

    use anyhow::{Result, anyhow};
    use tempfile::TempDir;

    use super::service::ServiceControl;
    use crate::config::{Cpa, Credentials, Paths};

    pub(crate) struct TestRoot {
        _root: TempDir,
        pub(crate) paths: Paths,
    }

    pub(crate) fn test_root() -> TestRoot {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(root.path().join("CodexMux"));
        crate::secrets::save(
            &paths.credentials,
            &Credentials {
                proxy_token: "proxy-token".into(),
                cpa_token: "cpa-token".into(),
                cpa_management_key: "management-key".into(),
            },
        )
        .unwrap();
        crate::config::Settings::default()
            .save(&paths.settings)
            .unwrap();
        TestRoot { _root: root, paths }
    }

    pub(crate) fn local_cpa(port: u16) -> Cpa {
        Cpa {
            base_url: format!("http://127.0.0.1:{port}/v1"),
        }
    }

    /// Records launchd and readiness calls. The CPA job counts as loaded
    /// between a bootstrap and the next stop; readiness and model probes
    /// replay queued results and succeed once the queue is empty.
    #[derive(Default)]
    pub(crate) struct FakeServices {
        pub(crate) loaded: RefCell<bool>,
        pub(crate) events: RefCell<Vec<String>>,
        pub(crate) ready: RefCell<VecDeque<Result<(), String>>>,
        pub(crate) models: RefCell<VecDeque<Result<Vec<String>, String>>>,
        pub(crate) fail_bootstrap: RefCell<bool>,
        pub(crate) fail_proxy_restart: RefCell<bool>,
    }

    impl FakeServices {
        pub(crate) fn loaded() -> Self {
            let services = Self::default();
            *services.loaded.borrow_mut() = true;
            services
        }

        pub(crate) fn events(&self) -> Vec<String> {
            self.events.borrow().clone()
        }

        pub(crate) fn fail_next_ready(&self, message: &str) {
            self.ready.borrow_mut().push_back(Err(message.to_owned()));
        }

        fn record(&self, event: String) {
            self.events.borrow_mut().push(event);
        }
    }

    impl ServiceControl for FakeServices {
        fn cpa_loaded(&self) -> Result<bool> {
            Ok(*self.loaded.borrow())
        }

        fn stop_cpa(&self) -> Result<()> {
            if *self.loaded.borrow() {
                self.record("stop".into());
            }
            *self.loaded.borrow_mut() = false;
            Ok(())
        }

        fn bootstrap_cpa(&self, plist: &Path) -> Result<()> {
            assert!(plist.is_file(), "bootstrap needs a written plist");
            let binary = super::binary_path(&Paths::from_root(
                plist.parent().unwrap().parent().unwrap().to_path_buf(),
            ));
            let content = std::fs::read(&binary).unwrap_or_default();
            self.record(format!(
                "bootstrap {}",
                String::from_utf8_lossy(&content).trim()
            ));
            if *self.fail_bootstrap.borrow() {
                return Err(anyhow!("bootstrap refused"));
            }
            *self.loaded.borrow_mut() = true;
            Ok(())
        }

        fn cpa_models(&self, cpa: &Cpa, _token: &str) -> Result<Vec<String>> {
            self.record(format!("models {}", cpa.base_url));
            match self.models.borrow_mut().pop_front() {
                Some(Ok(models)) => Ok(models),
                Some(Err(message)) => Err(anyhow!(message)),
                None => Ok(vec!["model".into()]),
            }
        }

        fn wait_for_cpa(&self, cpa: &Cpa, _token: &str) -> Result<()> {
            self.record(format!("ready {}", cpa.base_url));
            match self.ready.borrow_mut().pop_front() {
                Some(Err(message)) => Err(anyhow!(message)),
                _ => Ok(()),
            }
        }

        fn restart_proxy_if_running(&self) -> Result<()> {
            self.record("restart proxy".into());
            if *self.fail_proxy_restart.borrow() {
                return Err(anyhow!("kickstart refused"));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{test_support::test_root, *};

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
    fn model_validation_waits_for_cpa_startup() {
        let attempts = AtomicUsize::new(0);
        let slugs = wait_for_model_slugs_with(
            || {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(anyhow::anyhow!("connection refused"))
                } else {
                    Ok(vec!["model-a".into()])
                }
            },
            Duration::from_millis(200),
            Duration::from_millis(1),
        )
        .unwrap();

        assert_eq!(slugs, vec!["model-a".to_string()]);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn model_validation_fails_after_timeout() {
        let result = wait_for_model_slugs_with(
            || Err(anyhow::anyhow!("connection refused")),
            Duration::from_millis(20),
            Duration::from_millis(1),
        );

        assert!(result.is_err());
    }

    /// CPA names its image models only when it rejects an unknown one, so the
    /// picker parses that message. An unrecognized message yields no models
    /// rather than an error, because routing never depends on this list.
    #[test]
    fn image_model_discovery_parses_cpa_rejection_and_tolerates_anything_else() {
        let message = "Model gpt-image-1 is not supported on /v1/images/generations or \
             /v1/images/edits. Use gpt-image-1.5, gpt-image-2, grok-imagine-image, \
             grok-imagine-image-quality, grok-imagine-image-2.0, or a configured \
             openai-compatibility image model.";
        assert_eq!(
            image_slugs_from_message(message),
            vec![
                "gpt-image-1.5",
                "gpt-image-2",
                "grok-imagine-image",
                "grok-imagine-image-2.0",
                "grok-imagine-image-quality",
            ]
        );

        assert!(image_slugs_from_message("").is_empty());
        assert!(image_slugs_from_message("Invalid request: prompt is required").is_empty());
        assert!(image_slugs_from_message("Use the force.").is_empty());
    }

    #[test]
    fn installed_version_distinguishes_missing_from_corrupt_records() {
        let root = test_root();
        let paths = &root.paths;
        assert!(installed_version(paths).unwrap().is_none());

        fs::create_dir_all(version_path(paths).parent().unwrap()).unwrap();
        fs::write(
            version_path(paths),
            r#"{"version":"7.2.147","sha256":"4ac1db83b00591265ebb93a3277d812aaf6e45e8b21bb3b4786598520afdf4be"}"#,
        )
        .unwrap();
        let installed = installed_version(paths).unwrap().unwrap();
        assert_eq!(installed.version, "7.2.147");
        assert!(installed.source.is_none());

        fs::write(version_path(paths), b"").unwrap();
        assert!(installed_version(paths).is_err());
    }

    #[test]
    fn cpa_operations_fail_clearly_while_another_holds_the_lock() {
        let root = test_root();
        let held = lock(&root.paths).unwrap();
        let error = lock_within(&root.paths, Duration::from_millis(50)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("another CodexMux CPA operation is still running")
        );
        drop(held);
        lock_within(&root.paths, Duration::from_millis(50)).unwrap();
    }

    #[test]
    fn cpa_job_definition_lives_outside_launch_agents() {
        let root = test_root();
        let plist = plist_path(&root.paths);
        assert_eq!(plist, root.paths.root.join("cpa/dev.codexmux.cpa.plist"));
        assert!(!plist.to_string_lossy().contains("LaunchAgents"));
    }
}
