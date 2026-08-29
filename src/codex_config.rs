use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use toml_edit::DocumentMut;

use crate::fsutil::{
    atomic_create, atomic_remove_if_unchanged, atomic_replace_if_unchanged, atomic_write,
    sha256_bytes,
};

const START_MARKER: &str = "# >>> ModelMux managed";
const END_MARKER: &str = "# <<< ModelMux managed";
pub const PROXY_TOKEN_ENV: &str = "MODELMUX_PROXY_TOKEN";

#[derive(Clone, Debug)]
pub struct ConfigManager {
    config_path: PathBuf,
    state_path: PathBuf,
    backup_dir: PathBuf,
    lock_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Enabling,
    Active,
    Disabling { restored_sha256: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConfigState {
    config_path: PathBuf,
    backup_path: PathBuf,
    original_exists: bool,
    original_sha256: String,
    managed_sha256: String,
    managed_block: String,
    phase: Phase,
}

#[derive(Debug)]
pub struct ConfigLease {
    manager: ConfigManager,
    lock: File,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub enabled: bool,
    pub config_path: PathBuf,
    pub unchanged_since_enable: bool,
}

impl ConfigManager {
    pub fn new(config_path: PathBuf, state_path: PathBuf, backup_dir: PathBuf) -> Self {
        let lock_path = state_path.with_extension("lock");
        Self {
            config_path,
            state_path,
            backup_dir,
            lock_path,
        }
    }

    pub fn enable(&self, loopback_base_url: &str) -> Result<ConfigLease> {
        let lock = self.lock()?;
        self.reject_symlink()?;
        self.enable_locked(loopback_base_url)?;
        Ok(ConfigLease {
            manager: self.clone(),
            lock,
        })
    }

    pub fn disable(&self) -> Result<()> {
        let _lock = self.lock()?;
        self.disable_locked()
    }

    pub fn status(&self) -> Result<Status> {
        if !self.state_path.exists() {
            return Ok(Status {
                enabled: false,
                config_path: self.config_path.clone(),
                unchanged_since_enable: false,
            });
        }
        let state = self.load_state()?;
        ensure_same_path(&state.config_path, &self.config_path)?;
        let current = read_optional(&self.config_path)?;
        let enabled = contains_block(&current, &state.managed_block);
        Ok(Status {
            enabled,
            config_path: self.config_path.clone(),
            unchanged_since_enable: enabled && sha256_bytes(&current) == state.managed_sha256,
        })
    }

    fn enable_locked(&self, loopback_base_url: &str) -> Result<()> {
        if self.state_path.exists() {
            let state = self.load_state()?;
            ensure_same_path(&state.config_path, &self.config_path)?;
            match state.phase {
                Phase::Enabling => return self.finish_enabling(state),
                Phase::Active => {
                    let same_endpoint = state.managed_block
                        == managed_section(loopback_base_url, false)
                        || state.managed_block == managed_section(loopback_base_url, true);
                    if same_endpoint {
                        self.ensure_managed_block(&state)?;
                        return Ok(());
                    }
                    self.begin_disabling(state)?;
                }
                Phase::Disabling { .. } => self.finish_disabling(state)?,
            }
        }
        self.begin_enabling(loopback_base_url)
    }

    fn begin_enabling(&self, loopback_base_url: &str) -> Result<()> {
        let original_exists = self.config_path.exists();
        let original = if original_exists {
            let metadata = fs::metadata(&self.config_path)?;
            anyhow::ensure!(metadata.is_file(), "Codex config must be a regular file");
            fs::read(&self.config_path)?
        } else {
            Vec::new()
        };
        let text = std::str::from_utf8(&original).context("Codex config is not UTF-8")?;
        validate_existing(text)?;
        let section = managed_section(loopback_base_url, !text.is_empty());
        let output = format!("{section}{text}").into_bytes();

        fs::create_dir_all(&self.backup_dir)?;
        let backup_path = self.backup_dir.join(format!(
            "config.{}.toml",
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        atomic_write(&backup_path, &original)?;
        let mut state = ConfigState {
            config_path: self.config_path.clone(),
            backup_path,
            original_exists,
            original_sha256: sha256_bytes(&original),
            managed_sha256: sha256_bytes(&output),
            managed_block: section,
            phase: Phase::Enabling,
        };
        self.save_state(&state)?;
        replace_if_unchanged(&self.config_path, &original, &output, original_exists)?;
        state.phase = Phase::Active;
        self.save_state(&state)
    }

    fn finish_enabling(&self, mut state: ConfigState) -> Result<()> {
        let original = self.read_backup(&state)?;
        let output = managed_output(&state, &original)?;
        let current = read_optional(&self.config_path)?;
        if current == output || contains_block(&current, &state.managed_block) {
            state.phase = Phase::Active;
            return self.save_state(&state);
        }
        anyhow::ensure!(
            current == original,
            "Codex config changed while ModelMux activation was incomplete"
        );
        replace_if_unchanged(&self.config_path, &current, &output, state.original_exists)?;
        state.phase = Phase::Active;
        self.save_state(&state)
    }

    fn ensure_managed_block(&self, state: &ConfigState) -> Result<()> {
        let current = fs::read(&self.config_path)?;
        anyhow::ensure!(
            contains_block(&current, &state.managed_block),
            "Codex config changed inside the managed ModelMux block"
        );
        Ok(())
    }

    fn disable_locked(&self) -> Result<()> {
        if !self.state_path.exists() {
            return Ok(());
        }
        self.reject_symlink()?;
        let state = self.load_state()?;
        ensure_same_path(&state.config_path, &self.config_path)?;
        match state.phase {
            Phase::Enabling => {
                let original = self.read_backup(&state)?;
                let current = read_optional(&self.config_path)?;
                if current == original {
                    fs::remove_file(&self.state_path)?;
                    return Ok(());
                }
                self.begin_disabling(state)
            }
            Phase::Active => self.begin_disabling(state),
            Phase::Disabling { .. } => self.finish_disabling(state),
        }
    }

    fn begin_disabling(&self, mut state: ConfigState) -> Result<()> {
        let current = fs::read(&self.config_path)?;
        let restored = restored_config(&state, &current)?;
        state.phase = Phase::Disabling {
            restored_sha256: sha256_bytes(&restored),
        };
        self.save_state(&state)?;
        replace_if_unchanged(&self.config_path, &current, &restored, true)?;
        self.remove_restored_config_if_absent(&state, &restored)?;
        fs::remove_file(&self.state_path)?;
        Ok(())
    }

    fn finish_disabling(&self, state: ConfigState) -> Result<()> {
        let current = read_optional(&self.config_path)?;
        let restored_sha256 = match &state.phase {
            Phase::Disabling { restored_sha256 } => restored_sha256,
            _ => unreachable!(),
        };
        if sha256_bytes(&current) == *restored_sha256 {
            self.remove_restored_config_if_absent(&state, &current)?;
            fs::remove_file(&self.state_path)?;
            return Ok(());
        }
        let restored = restored_config(&state, &current)?;
        anyhow::ensure!(
            sha256_bytes(&restored) == *restored_sha256,
            "Codex config changed while ModelMux restoration was incomplete"
        );
        replace_if_unchanged(&self.config_path, &current, &restored, true)?;
        self.remove_restored_config_if_absent(&state, &restored)?;
        fs::remove_file(&self.state_path)?;
        Ok(())
    }

    fn remove_restored_config_if_absent(&self, state: &ConfigState, restored: &[u8]) -> Result<()> {
        if !state.original_exists && restored.is_empty() && self.config_path.exists() {
            atomic_remove_if_unchanged(&self.config_path, restored)?;
        }
        Ok(())
    }

    fn read_backup(&self, state: &ConfigState) -> Result<Vec<u8>> {
        let original = fs::read(&state.backup_path)?;
        anyhow::ensure!(
            sha256_bytes(&original) == state.original_sha256,
            "Codex config backup hash mismatch"
        );
        Ok(original)
    }

    fn reject_symlink(&self) -> Result<()> {
        if let Ok(metadata) = fs::symlink_metadata(&self.config_path) {
            anyhow::ensure!(
                !metadata.file_type().is_symlink(),
                "Codex config must not be a symbolic link"
            );
        }
        Ok(())
    }

    fn lock(&self) -> Result<File> {
        if let Some(parent) = self.lock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&self.lock_path)?;
        file.lock_exclusive()
            .context("failed to lock the Codex configuration state")?;
        Ok(file)
    }

    fn load_state(&self) -> Result<ConfigState> {
        Ok(serde_json::from_slice(&fs::read(&self.state_path)?)?)
    }

    fn save_state(&self, state: &ConfigState) -> Result<()> {
        atomic_write(&self.state_path, &serde_json::to_vec_pretty(state)?)
    }
}

impl ConfigLease {
    pub fn restore(self) -> Result<()> {
        let result = self.manager.disable_locked();
        self.lock.unlock()?;
        result
    }
}

pub fn codex_config_path() -> Result<PathBuf> {
    let path = if let Some(path) = std::env::var_os("CODEX_CONFIG") {
        anyhow::ensure!(!path.is_empty(), "CODEX_CONFIG must not be empty");
        PathBuf::from(path)
    } else if let Some(home) = std::env::var_os("CODEX_HOME") {
        anyhow::ensure!(!home.is_empty(), "CODEX_HOME must not be empty");
        PathBuf::from(home).join("config.toml")
    } else {
        dirs::home_dir()
            .context("cannot locate home directory")?
            .join(".codex/config.toml")
    };
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn ensure_same_path(recorded: &Path, current: &Path) -> Result<()> {
    if recorded != current {
        bail!("managed state points to a different Codex config file");
    }
    Ok(())
}

fn managed_section(loopback_base_url: &str, trailing_blank_line: bool) -> String {
    let base_url = toml_edit::Value::from(loopback_base_url).to_string();
    let block = format!(
        "{START_MARKER}\n\
         model_provider = \"modelmux\"\n\
         model_providers.modelmux = {{ name = \"ModelMux\", base_url = {base_url}, wire_api = \"responses\", requires_openai_auth = true, supports_websockets = false, env_http_headers = {{ x-modelmux-token = \"{PROXY_TOKEN_ENV}\" }} }}\n\
         {END_MARKER}\n"
    );
    if trailing_blank_line {
        format!("{block}\n")
    } else {
        block
    }
}

fn managed_output(state: &ConfigState, original: &[u8]) -> Result<Vec<u8>> {
    let output = [state.managed_block.as_bytes(), original].concat();
    anyhow::ensure!(
        sha256_bytes(&output) == state.managed_sha256,
        "managed Codex config hash mismatch"
    );
    Ok(output)
}

fn restored_config(state: &ConfigState, current: &[u8]) -> Result<Vec<u8>> {
    let current = std::str::from_utf8(current).context("Codex config is not UTF-8")?;
    anyhow::ensure!(
        current.matches(&state.managed_block).count() == 1,
        "Codex config changed inside the managed ModelMux block"
    );
    let restored = current.replacen(&state.managed_block, "", 1);
    if !restored.trim().is_empty() {
        restored
            .parse::<DocumentMut>()
            .context("removing ModelMux would leave invalid TOML")?;
    }
    Ok(restored.into_bytes())
}

fn contains_block(current: &[u8], block: &str) -> bool {
    std::str::from_utf8(current).is_ok_and(|current| current.matches(block).count() == 1)
}

fn read_optional(path: &Path) -> Result<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn replace_if_unchanged(
    path: &Path,
    expected: &[u8],
    replacement: &[u8],
    expected_exists: bool,
) -> Result<()> {
    let current_exists = path.exists();
    anyhow::ensure!(
        current_exists == expected_exists,
        "Codex config changed concurrently; refusing to overwrite it"
    );
    if expected_exists {
        atomic_replace_if_unchanged(path, expected, replacement)
    } else {
        atomic_create(path, replacement)
    }
}

fn validate_existing(text: &str) -> Result<()> {
    if text.contains(START_MARKER) || text.contains(END_MARKER) {
        bail!("Codex config already contains a ModelMux managed marker");
    }
    if text.trim().is_empty() {
        return Ok(());
    }
    let document = text
        .parse::<DocumentMut>()
        .context("existing Codex config is invalid TOML")?;
    for key in ["model_catalog_json", "model_provider", "model_providers"] {
        if document.get(key).is_some() {
            bail!("Codex config already defines {key}; refusing to overwrite it");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    const LOOPBACK_BASE_URL: &str = "http://127.0.0.1:48682/v1";

    fn manager(root: &Path, config: PathBuf) -> ConfigManager {
        ConfigManager::new(config, root.join("state.json"), root.join("backups"))
    }

    #[test]
    fn enable_and_disable_restore_exact_bytes() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        fs::write(&config, b"model = \"gpt\"\n").unwrap();
        let lease = manager(root.path(), config.clone())
            .enable(LOOPBACK_BASE_URL)
            .unwrap();
        let enabled = fs::read_to_string(&config).unwrap();
        assert!(enabled.contains("requires_openai_auth = true"));
        assert!(!enabled.contains("model_catalog_json"));
        assert!(enabled.contains("env_http_headers"));
        assert!(enabled.contains(PROXY_TOKEN_ENV));
        lease.restore().unwrap();
        assert_eq!(fs::read(&config).unwrap(), b"model = \"gpt\"\n");
    }

    #[test]
    fn restore_preserves_changes_outside_the_managed_block() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        fs::write(&config, b"model = \"gpt\"\n").unwrap();
        let lease = manager(root.path(), config.clone())
            .enable(LOOPBACK_BASE_URL)
            .unwrap();
        let current = fs::read_to_string(&config).unwrap();
        fs::write(&config, format!("{current}approval_policy = \"never\"\n")).unwrap();
        lease.restore().unwrap();
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "model = \"gpt\"\napproval_policy = \"never\"\n"
        );
    }

    #[test]
    fn incomplete_enable_and_disable_are_recovered() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        fs::write(&config, b"model = \"gpt\"\n").unwrap();
        let manager = manager(root.path(), config.clone());
        let lease = manager.enable(LOOPBACK_BASE_URL).unwrap();
        drop(lease);

        let mut state = manager.load_state().unwrap();
        state.phase = Phase::Enabling;
        manager.save_state(&state).unwrap();
        let lease = manager.enable(LOOPBACK_BASE_URL).unwrap();
        lease.restore().unwrap();
        assert_eq!(fs::read(&config).unwrap(), b"model = \"gpt\"\n");

        let lease = manager.enable(LOOPBACK_BASE_URL).unwrap();
        drop(lease);
        let mut state = manager.load_state().unwrap();
        let current = fs::read(&config).unwrap();
        let restored = restored_config(&state, &current).unwrap();
        state.phase = Phase::Disabling {
            restored_sha256: sha256_bytes(&restored),
        };
        manager.save_state(&state).unwrap();
        atomic_write(&config, &restored).unwrap();
        manager.disable().unwrap();
        assert!(!manager.state_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_config_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempdir().unwrap();
        let target = root.path().join("target.toml");
        let config = root.path().join("config.toml");
        fs::write(&target, b"model = \"gpt\"\n").unwrap();
        symlink(&target, &config).unwrap();
        let error = manager(root.path(), config)
            .enable(LOOPBACK_BASE_URL)
            .unwrap_err();
        assert!(error.to_string().contains("symbolic link"));
    }
}
