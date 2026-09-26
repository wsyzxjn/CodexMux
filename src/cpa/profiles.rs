use std::{
    collections::BTreeMap,
    fmt, fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{
    config_path, lock,
    service::{Launchd, ServiceControl, bootstrap_service, restart_service},
};
use crate::{
    catalog::CatalogModelOverride,
    config::{CPA_MODEL_PREFIX, Cpa, Paths, REDACTED, Settings, toml_parse_error},
    fsutil::{atomic_write, atomic_write_private, lock_exclusive},
};

/// A saved CPA endpoint: name, base URL, and the client token to use for it.
#[derive(Clone, Deserialize, Serialize)]
pub struct CpaProfile {
    pub name: String,
    pub base_url: String,
    pub token: String,
}

impl fmt::Debug for CpaProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CpaProfile")
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("token", &REDACTED)
            .finish()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ProfileStore {
    /// Name of the profile whose endpoint CodexMux currently uses, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active: Option<String>,
    #[serde(rename = "profile", default)]
    profiles: Vec<CpaProfile>,
    /// Review model override: when set, `codex-auto-review` routes explicitly
    /// to this CPA model instead of the official route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    review_override: Option<String>,
    /// Image route override: when set, Codex's built-in image requests go to
    /// this CPA image model instead of the official route. Image models are
    /// not part of any catalog, so this slug is user-declared and validated
    /// only by the upstream that receives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    image_override: Option<String>,
    /// Shared Responses `web_search` backend selected from the menu bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_backend: Option<String>,
    /// Menu override for the shared search feature. `None` follows
    /// `config.toml`; `Some(false)` disables it even when config enables it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_backend_enabled: Option<bool>,
    /// Serve-time metadata overrides for exact merged catalog slugs. These do
    /// not change routing and are applied after the catalog is merged.
    #[serde(
        rename = "model-overrides",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    model_overrides: BTreeMap<String, CatalogModelOverride>,
    /// Whether the proxy lifecycle runs the local CPA while Codex is active.
    /// Updated by `cpa start` / `cpa stop`; enabled when never chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cpa_autostart: Option<bool>,
}

impl ProfileStore {
    fn review_override(&self) -> Option<String> {
        nonempty(self.review_override.as_deref())
    }

    fn image_override(&self) -> Option<String> {
        nonempty(self.image_override.as_deref())
    }

    fn search_backend_setting(&self) -> Option<SearchBackendSetting> {
        match self.search_backend_enabled? {
            false => Some(SearchBackendSetting {
                enabled: false,
                backend_model: String::new(),
            }),
            true => {
                nonempty(self.search_backend.as_deref()).map(|backend_model| SearchBackendSetting {
                    enabled: true,
                    backend_model,
                })
            }
        }
    }

    fn cpa_autostart(&self) -> bool {
        self.cpa_autostart.unwrap_or(true)
    }
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn load_profile_store(paths: &Paths) -> Result<ProfileStore> {
    load_profile_store_from(&paths.cpa_profiles)
}

/// Read the profile store. The file holds endpoint tokens, so it is kept
/// private: wider permissions are tightened to 0600 with a warning, and a
/// parse error names only the position, never the offending line.
fn load_profile_store_from(path: &Path) -> Result<ProfileStore> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProfileStore::default());
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to open CPA profiles {}", path.display()));
        }
    };
    let mode = file
        .metadata()
        .with_context(|| format!("failed to stat CPA profiles {}", path.display()))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        match file.set_permissions(fs::Permissions::from_mode(0o600)) {
            Ok(()) => tracing::warn!(
                path = %path.display(),
                mode = format!("{:o}", mode & 0o777),
                "CPA profiles were accessible to other users; restricted them to 0600"
            ),
            Err(error) => tracing::warn!(
                path = %path.display(),
                %error,
                "CPA profiles are accessible to other users and could not be restricted"
            ),
        }
    }
    let mut text = String::new();
    file.read_to_string(&mut text)
        .with_context(|| format!("failed to read CPA profiles {}", path.display()))?;
    toml::from_str(&text).map_err(|error| toml_parse_error(path, &text, &error))
}

fn save_profile_store_to(path: &Path, store: &ProfileStore) -> Result<()> {
    atomic_write_private(path, toml::to_string_pretty(store)?.as_bytes())
}

fn profiles_lock_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    path.with_file_name(name)
}

/// Read-modify-write the profile store under its exclusive lock, so writers
/// in concurrent CodexMux processes never drop each other's updates.
fn update_profile_store<T>(
    path: &Path,
    change: impl FnOnce(&mut ProfileStore) -> Result<T>,
) -> Result<T> {
    let _lock = lock_exclusive(&profiles_lock_path(path))?;
    let mut store = load_profile_store_from(path)?;
    let value = change(&mut store)?;
    save_profile_store_to(path, &store)?;
    Ok(value)
}

/// List all saved CPA profiles and the active one.
pub fn profiles(paths: &Paths) -> Result<(Option<String>, Vec<CpaProfile>)> {
    let store = load_profile_store(paths)?;
    Ok((store.active, store.profiles))
}

/// Add or update a profile by name. Only its shape is validated here; the
/// endpoint itself is checked when a switch makes it active.
pub fn save_profile(paths: &Paths, profile: CpaProfile) -> Result<()> {
    let name = profile.name.trim().to_owned();
    anyhow::ensure!(!name.is_empty(), "profile name must not be empty");
    anyhow::ensure!(
        !profile.token.trim().is_empty(),
        "profile token must not be empty"
    );
    Cpa {
        base_url: profile.base_url.clone(),
    }
    .validate()?;
    let credentials = crate::secrets::load(&paths.credentials)?;
    anyhow::ensure!(
        profile.token != credentials.proxy_token && profile.token != credentials.cpa_management_key,
        "the profile token must differ from the CodexMux proxy token and the CPA management key"
    );
    let profile = CpaProfile { name, ..profile };
    update_profile_store(&paths.cpa_profiles, |store| {
        match store
            .profiles
            .iter_mut()
            .find(|saved| saved.name == profile.name)
        {
            Some(saved) => *saved = profile,
            None => store.profiles.push(profile),
        }
        Ok(())
    })
}

/// Remove a saved profile. Removing the active one clears `active`; CodexMux
/// keeps using its endpoint until another profile is switched to.
pub fn remove_profile(paths: &Paths, name: &str) -> Result<()> {
    update_profile_store(&paths.cpa_profiles, |store| {
        let before = store.profiles.len();
        store.profiles.retain(|profile| profile.name != name);
        anyhow::ensure!(
            store.profiles.len() < before,
            "profile {name} does not exist"
        );
        if store.active.as_deref() == Some(name) {
            store.active = None;
        }
        Ok(())
    })
}

/// Read the current review model override (None = the official route).
/// Reads the profile file on every call so menu-bar switches take effect
/// without restarting the proxy.
pub fn review_override(profiles_path: &Path) -> Result<Option<String>> {
    Ok(load_profile_store_from(profiles_path)?.review_override())
}

/// Set or clear (`None`) the review model override and persist it. The live
/// catalog is checked when a request uses the override, so a stale selection
/// fails closed after an upstream catalog change.
pub fn set_review_override(profiles_path: &Path, slug: Option<String>) -> Result<()> {
    let slug = slug
        .map(|slug| upstream_override_slug("review", &slug))
        .transpose()?;
    update_profile_store(profiles_path, |store| {
        store.review_override = slug;
        Ok(())
    })
}

/// Read the current image route override (None = the official route).
/// Read per request so menu-bar switches apply without a proxy restart.
pub fn image_override(profiles_path: &Path) -> Result<Option<String>> {
    Ok(load_profile_store_from(profiles_path)?.image_override())
}

/// Set or clear (`None`) the image route override and persist it.
///
/// Unlike the review override there is no catalog to check the slug against:
/// CPA serves image models that its `/v1/models` response never lists. The
/// slug is therefore stored as declared and validated by the upstream, whose
/// error is passed back to Codex unchanged.
pub fn set_image_override(profiles_path: &Path, slug: Option<String>) -> Result<()> {
    let slug = slug
        .map(|slug| upstream_override_slug("image", &slug))
        .transpose()?;
    update_profile_store(profiles_path, |store| {
        store.image_override = slug;
        Ok(())
    })
}

fn upstream_override_slug(kind: &str, slug: &str) -> Result<String> {
    let slug = slug.trim();
    anyhow::ensure!(!slug.is_empty(), "{kind} override slug must not be empty");
    anyhow::ensure!(
        !slug.starts_with(CPA_MODEL_PREFIX),
        "{kind} override uses the upstream slug without the cpa/ prefix"
    );
    Ok(slug.to_owned())
}

/// A menu bar shared search override.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchBackendSetting {
    pub enabled: bool,
    pub backend_model: String,
}

/// Read the menu bar shared search override; `None` means `config.toml`
/// decides.
pub fn search_backend_setting(profiles_path: &Path) -> Result<Option<SearchBackendSetting>> {
    Ok(load_profile_store_from(profiles_path)?.search_backend_setting())
}

/// Enable (`Some(Some(slug))`), disable (`Some(None)`), or clear (`None`) the
/// menu override for the shared Responses web search backend.
pub fn set_search_backend_setting(
    profiles_path: &Path,
    override_kind: Option<Option<String>>,
) -> Result<()> {
    let (backend, enabled) = match override_kind {
        Some(Some(slug)) => {
            let slug = slug.trim().to_owned();
            anyhow::ensure!(!slug.is_empty(), "search backend slug must not be empty");
            (Some(slug), Some(true))
        }
        Some(None) => (None, Some(false)),
        None => (None, None),
    };
    update_profile_store(profiles_path, |store| {
        store.search_backend = backend;
        store.search_backend_enabled = enabled;
        Ok(())
    })
}

/// Whether the proxy lifecycle runs the local CPA; enabled until the user
/// explicitly stops CPA or turns it off.
pub fn cpa_autostart(profiles_path: &Path) -> Result<bool> {
    Ok(load_profile_store_from(profiles_path)?.cpa_autostart())
}

/// Persist the CPA autostart preference.
pub fn set_cpa_autostart(profiles_path: &Path, enabled: bool) -> Result<()> {
    update_profile_store(profiles_path, |store| {
        store.cpa_autostart = Some(enabled);
        Ok(())
    })
}

/// Serve-time metadata overrides for exact merged catalog slugs.
pub fn catalog_model_overrides(
    profiles_path: &Path,
) -> Result<BTreeMap<String, CatalogModelOverride>> {
    Ok(load_profile_store_from(profiles_path)?
        .model_overrides
        .into_iter()
        .filter(|(_, metadata)| !metadata.is_empty())
        .collect())
}

/// Everything the menu bar shows from `cpa-profiles.toml`, read in one pass.
#[derive(Clone, Debug)]
pub struct ProfileSettings {
    pub active: Option<String>,
    pub profiles: Vec<CpaProfile>,
    pub review_override: Option<String>,
    pub image_override: Option<String>,
    pub search_backend: Option<SearchBackendSetting>,
    pub cpa_autostart: bool,
}

pub fn profile_settings(profiles_path: &Path) -> Result<ProfileSettings> {
    let store = load_profile_store_from(profiles_path)?;
    Ok(ProfileSettings {
        review_override: store.review_override(),
        image_override: store.image_override(),
        search_backend: store.search_backend_setting(),
        cpa_autostart: store.cpa_autostart(),
        active: store.active,
        profiles: store.profiles,
    })
}

/// Make a saved profile the CPA endpoint CodexMux uses.
///
/// A loopback profile rewrites the managed local CPA config with the
/// profile's port and token, restarts CPA, and waits for `/models`. A remote
/// profile must answer `/models` with its token before the local CPA is
/// stopped (the startup preference stays untouched). Either way CodexMux's
/// `config.toml` and `credentials.json` then point at the profile, and a
/// running proxy restarts to load them. Any failure restores the previous
/// files and the previous local CPA state.
pub fn switch_profile(paths: &Paths, name: &str) -> Result<()> {
    switch_profile_with(&Launchd, paths, name)
}

fn switch_profile_with(services: &dyn ServiceControl, paths: &Paths, name: &str) -> Result<()> {
    let cpa_lock = lock(paths)?;
    let profile = load_profile_store(paths)?
        .profiles
        .into_iter()
        .find(|profile| profile.name == name)
        .with_context(|| format!("profile {name} does not exist"))?;
    let target = Cpa {
        base_url: profile.base_url.clone(),
    };
    let mut settings = Settings::load(&paths.settings)?;
    settings.cpa = target.clone();
    settings.validate()?;
    let mut credentials = crate::secrets::load(&paths.credentials)?;
    credentials.cpa_token = profile.token.clone();
    credentials
        .validate()
        .with_context(|| format!("profile {name} cannot be used"))?;
    if !target.is_loopback() {
        services
            .cpa_models(&target, &profile.token)
            .with_context(|| format!("profile {name} failed validation; not switching"))?;
    }

    let snapshot = SwitchSnapshot::capture(services, paths)?;
    let applied = (|| -> Result<()> {
        if target.is_loopback() {
            restart_service(services, paths, &target, &profile.token)?;
            services.wait_for_cpa(&target, &profile.token)?;
        } else {
            services.stop_cpa()?;
        }
        settings.save(&paths.settings)?;
        crate::secrets::save(&paths.credentials, &credentials)?;
        update_profile_store(&paths.cpa_profiles, |store| {
            anyhow::ensure!(
                store.profiles.iter().any(|saved| saved.name == profile.name
                    && saved.base_url == profile.base_url
                    && saved.token == profile.token),
                "profile {name} changed while switching"
            );
            store.active = Some(profile.name.clone());
            Ok(())
        })
    })();
    if let Err(error) = applied {
        return Err(snapshot.roll_back(services, paths, name, error));
    }

    // The proxy's own shutdown may stop a lifecycle-managed CPA, which takes
    // the CPA lock, so release it before restarting the proxy.
    drop(cpa_lock);
    let Err(error) = services.restart_proxy_if_running() else {
        return Ok(());
    };
    let error = error.context("failed to restart the running CodexMux proxy");
    match lock(paths) {
        Ok(_cpa_lock) => Err(snapshot.roll_back(services, paths, name, error)),
        Err(lock_error) => Err(error.context(format!(
            "switched to profile {name}, but could not roll back: {lock_error:#}"
        ))),
    }
}

/// The state a failed profile switch restores, byte for byte.
struct SwitchSnapshot {
    settings: Vec<u8>,
    credentials: Vec<u8>,
    managed_config: Option<Vec<u8>>,
    active: Option<String>,
    cpa_was_loaded: bool,
}

impl SwitchSnapshot {
    fn capture(services: &dyn ServiceControl, paths: &Paths) -> Result<Self> {
        let managed_config = match fs::read(config_path(paths)) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read {}", config_path(paths).display()));
            }
        };
        Ok(Self {
            settings: fs::read(&paths.settings)
                .with_context(|| format!("failed to read {}", paths.settings.display()))?,
            credentials: fs::read(&paths.credentials)
                .with_context(|| format!("failed to read {}", paths.credentials.display()))?,
            managed_config,
            active: load_profile_store(paths)?.active,
            cpa_was_loaded: services.cpa_loaded()?,
        })
    }

    fn restore_files(&self, paths: &Paths) -> Result<()> {
        atomic_write(&paths.settings, &self.settings)?;
        atomic_write_private(&paths.credentials, &self.credentials)?;
        match &self.managed_config {
            Some(bytes) => atomic_write_private(&config_path(paths), bytes)?,
            None => match fs::remove_file(config_path(paths)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            },
        }
        update_profile_store(&paths.cpa_profiles, |store| {
            store.active = self.active.clone();
            Ok(())
        })
    }

    /// Put the local CPA back the way the switch found it: restarted on the
    /// restored config when it was running, stopped otherwise.
    fn restore_service(&self, services: &dyn ServiceControl, paths: &Paths) -> Result<()> {
        services.stop_cpa()?;
        if self.cpa_was_loaded {
            bootstrap_service(services, paths)?;
        }
        Ok(())
    }

    fn roll_back(
        &self,
        services: &dyn ServiceControl,
        paths: &Paths,
        name: &str,
        error: anyhow::Error,
    ) -> anyhow::Error {
        if let Err(restore_error) = self.restore_files(paths) {
            return error.context(format!(
                "switching to profile {name} failed, and restoring the previous settings also failed: {restore_error:#}"
            ));
        }
        match self.restore_service(services, paths) {
            Ok(()) => error.context(format!(
                "switching to profile {name} failed; restored the previous CPA endpoint"
            )),
            Err(restart_error) => error.context(format!(
                "switching to profile {name} failed; restored the previous settings, but restoring the local CPA service failed: {restart_error:#}"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::cpa::{
        binary_path,
        managed_config::write_config,
        test_support::{FakeServices, TestRoot, local_cpa, test_root},
    };

    fn profile(name: &str, base_url: &str, token: &str) -> CpaProfile {
        CpaProfile {
            name: name.into(),
            base_url: base_url.into(),
            token: token.into(),
        }
    }

    /// A data root with a running local CPA and two saved profiles.
    fn switch_root() -> TestRoot {
        let root = test_root();
        let paths = &root.paths;
        fs::create_dir_all(binary_path(paths).parent().unwrap()).unwrap();
        fs::write(binary_path(paths), "cpa-binary").unwrap();
        write_config(paths, &local_cpa(8317), "cpa-token").unwrap();
        save_profile(
            paths,
            profile("alt-local", "http://127.0.0.1:9317/v1", "alt-token"),
        )
        .unwrap();
        save_profile(
            paths,
            profile("remote", "https://cpa.example.com/v1", "remote-token"),
        )
        .unwrap();
        root
    }

    fn file_bytes(paths: &Paths) -> [Vec<u8>; 3] {
        [
            fs::read(&paths.settings).unwrap(),
            fs::read(&paths.credentials).unwrap(),
            fs::read(config_path(paths)).unwrap(),
        ]
    }

    #[test]
    fn profile_debug_output_redacts_the_token() {
        let debug = format!(
            "{:?}",
            profile("remote", "https://cpa.example.com/v1", "sk-secret")
        );
        assert!(debug.contains("cpa.example.com"));
        assert!(!debug.contains("sk-secret"));
    }

    #[test]
    fn profiles_save_list_and_remove() {
        let root = test_root();
        let paths = &root.paths;
        save_profile(
            paths,
            profile("local", "http://127.0.0.1:8317/v1", "token-a"),
        )
        .unwrap();
        save_profile(
            paths,
            profile("remote", "https://cpa.example.com/v1", "token-b"),
        )
        .unwrap();
        let (active, saved) = profiles(paths).unwrap();
        assert_eq!(active, None);
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].name, "local");

        // Updating an existing name replaces the entry instead of duplicating.
        save_profile(
            paths,
            profile("local", "http://127.0.0.1:9317/v1", "token-a2"),
        )
        .unwrap();
        let (_, updated) = profiles(paths).unwrap();
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[0].base_url, "http://127.0.0.1:9317/v1");

        remove_profile(paths, "local").unwrap();
        let (_, remaining) = profiles(paths).unwrap();
        assert_eq!(remaining.len(), 1);
        let error = remove_profile(paths, "local").unwrap_err();
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn saving_a_profile_rejects_unusable_names_tokens_and_urls() {
        let root = test_root();
        let paths = &root.paths;
        let url = "https://cpa.example.com/v1";
        for (candidate, expected) in [
            (profile(" ", url, "token"), "name must not be empty"),
            (profile("p", url, " "), "token must not be empty"),
            (profile("p", url, "proxy-token"), "must differ"),
            (profile("p", url, "management-key"), "must differ"),
            (
                profile("p", "http://external.example.com/v1", "token"),
                "must use HTTPS",
            ),
        ] {
            let error = save_profile(paths, candidate).unwrap_err();
            assert!(error.to_string().contains(expected), "{error:#}");
        }
        assert!(!paths.cpa_profiles.exists());
    }

    #[test]
    fn concurrent_writers_never_lose_updates() {
        let root = Arc::new(test_root());
        let handles = (0..8)
            .map(|index| {
                let root = Arc::clone(&root);
                std::thread::spawn(move || {
                    save_profile(
                        &root.paths,
                        profile(
                            &format!("profile-{index}"),
                            "https://cpa.example.com/v1",
                            &format!("token-{index}"),
                        ),
                    )
                    .unwrap();
                    if index % 2 == 0 {
                        set_review_override(&root.paths.cpa_profiles, Some("glm".into())).unwrap();
                    } else {
                        set_cpa_autostart(&root.paths.cpa_profiles, false).unwrap();
                    }
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
        let settings = profile_settings(&root.paths.cpa_profiles).unwrap();
        assert_eq!(settings.profiles.len(), 8);
        assert_eq!(settings.review_override.as_deref(), Some("glm"));
        assert!(!settings.cpa_autostart);
    }

    #[test]
    fn every_setting_round_trips_through_one_store() {
        let root = test_root();
        let path = &root.paths.cpa_profiles;
        save_profile(
            &root.paths,
            profile("remote", "https://cpa.example.com/v1", "token-b"),
        )
        .unwrap();
        set_review_override(path, Some(" glm-5.3-flash ".into())).unwrap();
        set_image_override(path, Some("grok-imagine-image".into())).unwrap();
        set_search_backend_setting(path, Some(Some("gpt-5.6-sol".into()))).unwrap();
        set_cpa_autostart(path, false).unwrap();

        let settings = profile_settings(path).unwrap();
        assert_eq!(settings.profiles.len(), 1);
        assert_eq!(settings.review_override.as_deref(), Some("glm-5.3-flash"));
        assert_eq!(
            settings.image_override.as_deref(),
            Some("grok-imagine-image")
        );
        assert_eq!(
            settings.search_backend,
            Some(SearchBackendSetting {
                enabled: true,
                backend_model: "gpt-5.6-sol".into()
            })
        );
        assert!(!settings.cpa_autostart);
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn overrides_reject_prefixed_or_empty_slugs() {
        let root = test_root();
        let path = &root.paths.cpa_profiles;
        assert!(image_override(path).unwrap().is_none());
        assert!(set_image_override(path, Some("cpa/gpt-image-2".into())).is_err());
        assert!(set_image_override(path, Some("   ".into())).is_err());
        assert!(set_review_override(path, Some("cpa/glm".into())).is_err());

        set_image_override(path, Some("grok-imagine-image".into())).unwrap();
        set_review_override(path, Some("glm-5.3-flash".into())).unwrap();
        set_image_override(path, None).unwrap();
        assert!(image_override(path).unwrap().is_none());
        assert_eq!(
            review_override(path).unwrap().as_deref(),
            Some("glm-5.3-flash")
        );
    }

    #[test]
    fn shared_search_backend_override_round_trips() {
        let root = test_root();
        let path = &root.paths.cpa_profiles;
        assert!(search_backend_setting(path).unwrap().is_none());

        set_search_backend_setting(path, Some(Some("gpt-5.6-sol".into()))).unwrap();
        let setting = search_backend_setting(path).unwrap().unwrap();
        assert!(setting.enabled);
        assert_eq!(setting.backend_model, "gpt-5.6-sol");

        set_search_backend_setting(path, Some(None)).unwrap();
        assert!(!search_backend_setting(path).unwrap().unwrap().enabled);

        set_search_backend_setting(path, None).unwrap();
        assert!(search_backend_setting(path).unwrap().is_none());
    }

    #[test]
    fn cpa_autostart_defaults_to_enabled_and_round_trips() {
        let root = test_root();
        let path = &root.paths.cpa_profiles;
        assert!(cpa_autostart(path).unwrap());
        set_cpa_autostart(path, false).unwrap();
        assert!(!cpa_autostart(path).unwrap());
        set_cpa_autostart(path, true).unwrap();
        assert!(cpa_autostart(path).unwrap());
    }

    #[test]
    fn catalog_model_overrides_load_from_profile_config() {
        let root = test_root();
        atomic_write_private(
            &root.paths.cpa_profiles,
            br#"
[model-overrides."cpa/gpt-6-astra"]
context_window = 1000000
max_context_window = 1000000
"#,
        )
        .unwrap();

        let overrides = catalog_model_overrides(&root.paths.cpa_profiles).unwrap();
        let metadata = overrides.get("cpa/gpt-6-astra").unwrap();
        assert_eq!(metadata.context_window, Some(1_000_000));
        assert_eq!(metadata.max_context_window, Some(1_000_000));
    }

    #[test]
    fn malformed_profile_file_fails_closed_without_quoting_or_overwriting_it() {
        let root = test_root();
        let path = &root.paths.cpa_profiles;
        let original = b"[[profile]]\nname = \"remote\"\ntoken = sk-live-secret\n";
        atomic_write_private(path, original).unwrap();

        let error = review_override(path).unwrap_err();
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains(&format!("invalid {} at line 3, column 9", path.display())),
            "{rendered}"
        );
        assert!(!rendered.contains("sk-live-secret"), "{rendered}");
        assert!(set_cpa_autostart(path, false).is_err());
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn loading_tightens_a_profile_file_readable_by_others() {
        let root = test_root();
        let path = &root.paths.cpa_profiles;
        fs::write(path, "cpa_autostart = false\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!cpa_autostart(path).unwrap());
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn switch_to_unknown_profile_fails_without_changes() {
        let root = switch_root();
        let before = file_bytes(&root.paths);
        let services = FakeServices::loaded();
        let error = switch_profile_with(&services, &root.paths, "missing").unwrap_err();
        assert!(error.to_string().contains("does not exist"));
        assert_eq!(file_bytes(&root.paths), before);
        assert!(services.events().is_empty());
    }

    #[test]
    fn unreachable_remote_profile_changes_nothing() {
        let root = switch_root();
        let before = file_bytes(&root.paths);
        let services = FakeServices::loaded();
        services
            .models
            .borrow_mut()
            .push_back(Err("connection refused".into()));
        let error = switch_profile_with(&services, &root.paths, "remote").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("failed validation; not switching")
        );
        assert_eq!(services.events(), ["models https://cpa.example.com/v1"]);
        assert_eq!(file_bytes(&root.paths), before);
        assert_eq!(profiles(&root.paths).unwrap().0, None);
    }

    #[test]
    fn switching_to_a_remote_profile_points_codexmux_at_it_and_stops_local_cpa() {
        let root = switch_root();
        let paths = &root.paths;
        let services = FakeServices::loaded();
        switch_profile_with(&services, paths, "remote").unwrap();

        assert_eq!(
            services.events(),
            ["models https://cpa.example.com/v1", "stop", "restart proxy"]
        );
        let settings = Settings::load(&paths.settings).unwrap();
        assert_eq!(settings.cpa.base_url, "https://cpa.example.com/v1");
        let credentials = crate::secrets::load(&paths.credentials).unwrap();
        assert_eq!(credentials.cpa_token, "remote-token");
        assert_eq!(credentials.proxy_token, "proxy-token");
        let store = load_profile_store(paths).unwrap();
        assert_eq!(store.active.as_deref(), Some("remote"));
        // Stopping for a remote profile is not a startup preference change.
        assert_eq!(store.cpa_autostart, None);
    }

    #[test]
    fn switching_to_a_local_profile_restarts_cpa_on_the_profile_port_and_token() {
        let root = switch_root();
        let paths = &root.paths;
        let services = FakeServices::loaded();
        switch_profile_with(&services, paths, "alt-local").unwrap();

        assert_eq!(
            services.events(),
            [
                "stop",
                "bootstrap cpa-binary",
                "ready http://127.0.0.1:9317/v1",
                "restart proxy"
            ]
        );
        let config = fs::read_to_string(config_path(paths)).unwrap();
        assert!(config.contains("port: 9317"));
        assert!(config.contains("  - \"alt-token\"\n"));
        assert_eq!(
            Settings::load(&paths.settings).unwrap().cpa.base_url,
            "http://127.0.0.1:9317/v1"
        );
        assert_eq!(
            crate::secrets::load(&paths.credentials).unwrap().cpa_token,
            "alt-token"
        );
        assert_eq!(profiles(paths).unwrap().0.as_deref(), Some("alt-local"));
    }

    #[test]
    fn a_failed_local_switch_restores_files_and_restarts_the_previous_cpa() {
        let root = switch_root();
        let paths = &root.paths;
        let before = file_bytes(paths);
        let services = FakeServices::loaded();
        services.fail_next_ready("CPA never answered");

        let error = switch_profile_with(&services, paths, "alt-local").unwrap_err();
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("restored the previous CPA endpoint"),
            "{rendered}"
        );
        assert!(rendered.contains("CPA never answered"), "{rendered}");
        assert_eq!(file_bytes(paths), before);
        assert_eq!(profiles(paths).unwrap().0, None);
        assert_eq!(
            services.events(),
            [
                "stop",
                "bootstrap cpa-binary",
                "ready http://127.0.0.1:9317/v1",
                "stop",
                "bootstrap cpa-binary"
            ]
        );
        assert!(*services.loaded.borrow());
    }

    #[test]
    fn a_failed_proxy_restart_rolls_the_switch_back() {
        let root = switch_root();
        let paths = &root.paths;
        let before = file_bytes(paths);
        // The local CPA was stopped before the switch, so the rollback
        // leaves it stopped.
        let services = FakeServices::default();
        *services.fail_proxy_restart.borrow_mut() = true;

        let error = switch_profile_with(&services, paths, "remote").unwrap_err();
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("failed to restart the running CodexMux proxy"),
            "{rendered}"
        );
        assert_eq!(file_bytes(paths), before);
        assert_eq!(profiles(paths).unwrap().0, None);
        assert!(!*services.loaded.borrow());
    }
}
