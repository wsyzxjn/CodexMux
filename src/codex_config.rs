use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use toml_edit::DocumentMut;

use crate::fsutil::{atomic_write, sha256_bytes};

const START_MARKER: &str = "# >>> ModelMux managed";
const END_MARKER: &str = "# <<< ModelMux managed";
pub const PROXY_TOKEN_ENV: &str = "MODELMUX_PROXY_TOKEN";

#[derive(Clone, Debug)]
pub struct ConfigManager {
    config_path: PathBuf,
    state_path: PathBuf,
    backup_dir: PathBuf,
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
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub enabled: bool,
    pub config_path: PathBuf,
    pub unchanged_since_enable: bool,
}

impl ConfigManager {
    pub fn new(config_path: PathBuf, state_path: PathBuf, backup_dir: PathBuf) -> Self {
        Self {
            config_path,
            state_path,
            backup_dir,
        }
    }

    pub fn enable(&self, loopback_base_url: &str) -> Result<()> {
        if self.state_path.exists() {
            return self.replace(loopback_base_url);
        }
        let original_exists = self.config_path.exists();
        let original = if original_exists {
            fs::read(&self.config_path)?
        } else {
            Vec::new()
        };
        let text = std::str::from_utf8(&original).context("Codex config is not UTF-8")?;
        validate_existing(text)?;
        let block = managed_block(loopback_base_url);
        let section = if text.is_empty() {
            block
        } else {
            format!("{block}\n")
        };
        let output = format!("{section}{text}");
        fs::create_dir_all(&self.backup_dir)?;
        let backup_path = self.backup_dir.join(format!(
            "config.{}.toml",
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        atomic_write(&backup_path, &original)?;
        atomic_write(&self.config_path, output.as_bytes())?;
        let state = ConfigState {
            config_path: self.config_path.clone(),
            backup_path,
            original_exists,
            original_sha256: sha256_bytes(&original),
            managed_sha256: sha256_bytes(output.as_bytes()),
            managed_block: section,
        };
        if let Err(error) = atomic_write(&self.state_path, &serde_json::to_vec_pretty(&state)?) {
            if original_exists {
                atomic_write(&self.config_path, &original)?;
            } else if self.config_path.exists() {
                fs::remove_file(&self.config_path)?;
            }
            return Err(error).context("failed to save state; original Codex config restored");
        }
        Ok(())
    }

    fn replace(&self, loopback_base_url: &str) -> Result<()> {
        let mut state = self.load_state()?;
        ensure_same_path(&state.config_path, &self.config_path)?;
        let current = fs::read_to_string(&self.config_path)?;
        if current.matches(&state.managed_block).count() != 1 {
            bail!("Codex config changed inside the managed ModelMux block");
        }
        let block = managed_block(loopback_base_url);
        let section = if state.managed_block.ends_with("\n\n") {
            format!("{block}\n")
        } else {
            block
        };
        let next = current.replacen(&state.managed_block, &section, 1);
        let unchanged_outside_managed_block =
            sha256_bytes(current.as_bytes()) == state.managed_sha256;
        state.managed_block = section;
        if unchanged_outside_managed_block {
            state.managed_sha256 = sha256_bytes(next.as_bytes());
        }
        let state_bytes = serde_json::to_vec_pretty(&state)?;
        atomic_write(&self.config_path, next.as_bytes())?;
        if let Err(error) = atomic_write(&self.state_path, &state_bytes) {
            if let Err(rollback) = atomic_write(&self.config_path, current.as_bytes()) {
                bail!(
                    "failed to save managed state ({error}); failed to restore previous Codex config ({rollback})"
                );
            }
            return Err(error)
                .context("failed to save managed state; previous Codex config restored");
        }
        Ok(())
    }

    pub fn disable(&self) -> Result<()> {
        let state = self.load_state()?;
        ensure_same_path(&state.config_path, &self.config_path)?;
        let current = fs::read(&self.config_path)?;
        if sha256_bytes(&current) == state.managed_sha256 {
            let original = fs::read(&state.backup_path)?;
            anyhow::ensure!(
                sha256_bytes(&original) == state.original_sha256,
                "Codex config backup hash mismatch"
            );
            if state.original_exists {
                atomic_write(&self.config_path, &original)?;
            } else {
                fs::remove_file(&self.config_path)?;
            }
        } else {
            let current = std::str::from_utf8(&current).context("Codex config is not UTF-8")?;
            if current.matches(&state.managed_block).count() != 1 {
                bail!("Codex config changed inside the managed ModelMux block");
            }
            let restored = current.replacen(&state.managed_block, "", 1);
            if !restored.trim().is_empty() {
                restored
                    .parse::<DocumentMut>()
                    .context("removing ModelMux would leave invalid TOML")?;
            }
            atomic_write(&self.config_path, restored.as_bytes())?;
        }
        fs::remove_file(&self.state_path)?;
        Ok(())
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
        let current = fs::read(&self.config_path).unwrap_or_default();
        Ok(Status {
            enabled: true,
            config_path: self.config_path.clone(),
            unchanged_since_enable: sha256_bytes(&current) == state.managed_sha256,
        })
    }

    fn load_state(&self) -> Result<ConfigState> {
        Ok(serde_json::from_slice(&fs::read(&self.state_path)?)?)
    }
}

pub fn codex_config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    if let Some(home) = std::env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(home).join("config.toml"));
    }
    Ok(dirs::home_dir()
        .context("cannot locate home directory")?
        .join(".codex/config.toml"))
}

fn ensure_same_path(recorded: &Path, current: &Path) -> Result<()> {
    if recorded != current {
        bail!("managed state points to a different Codex config file");
    }
    Ok(())
}

fn managed_block(loopback_base_url: &str) -> String {
    let base_url = toml_edit::Value::from(loopback_base_url).to_string();
    format!(
        "{START_MARKER}\n\
         model_provider = \"modelmux\"\n\
         model_providers.modelmux = {{ name = \"ModelMux\", base_url = {base_url}, wire_api = \"responses\", requires_openai_auth = true, supports_websockets = false, env_http_headers = {{ x-modelmux-token = \"{PROXY_TOKEN_ENV}\" }} }}\n\
         {END_MARKER}\n"
    )
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

    #[test]
    fn enable_and_disable_restore_exact_bytes() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        fs::write(&config, b"model = \"gpt\"\n").unwrap();
        let manager = ConfigManager::new(
            config.clone(),
            root.path().join("state.json"),
            root.path().join("backups"),
        );
        manager.enable(LOOPBACK_BASE_URL).unwrap();
        let enabled = fs::read_to_string(&config).unwrap();
        assert!(enabled.contains("requires_openai_auth = true"));
        assert!(!enabled.contains("model_catalog_json"));
        assert!(enabled.contains("env_http_headers"));
        assert!(enabled.contains(PROXY_TOKEN_ENV));
        manager.disable().unwrap();
        assert_eq!(fs::read(&config).unwrap(), b"model = \"gpt\"\n");
    }

    #[test]
    fn reenable_never_discards_changes_outside_the_managed_block() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        fs::write(&config, b"model = \"gpt\"\n").unwrap();
        let manager = ConfigManager::new(
            config.clone(),
            root.path().join("state.json"),
            root.path().join("backups"),
        );
        manager.enable(LOOPBACK_BASE_URL).unwrap();
        let current = fs::read_to_string(&config).unwrap();
        fs::write(&config, format!("{current}approval_policy = \"never\"\n")).unwrap();

        manager.enable(LOOPBACK_BASE_URL).unwrap();
        manager.disable().unwrap();

        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "model = \"gpt\"\napproval_policy = \"never\"\n"
        );
    }
}
