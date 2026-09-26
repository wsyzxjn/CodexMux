use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use toml_edit::DocumentMut;

use crate::fsutil::{
    atomic_create, atomic_remove_if_unchanged, atomic_replace_if_unchanged, atomic_write,
    sha256_bytes,
};

const START_MARKER: &str = "# >>> CodexMux managed";
const END_MARKER: &str = "# <<< CodexMux managed";
const SELECTOR_KEY: &str = "model_provider";
const PROVIDER_KEY: &str = "model_providers.codexmux";
pub const PROXY_TOKEN_ENV: &str = "CODEXMUX_PROXY_TOKEN";

/// Owns the reversible CodexMux block at the top of the Codex config.
///
/// The block is prepended to the user's file. Everything between its two
/// marker lines belongs to CodexMux except lines another writer inserted
/// there (Codex Desktop appends new root keys after the last root key, which
/// can be inside the block). Those lines are user content: healing moves them
/// below the block and disabling keeps them, and both refuse to write unless
/// the resulting document is the user's document minus the CodexMux keys.
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
    /// Pre-enable bytes, kept only until the enable is recorded as active.
    backup_path: PathBuf,
    original_exists: bool,
    original_sha256: String,
    managed_sha256: String,
    managed_block: String,
    phase: Phase,
}

/// Returned by `enable`. `restore` undoes only an enable this lease made, so
/// a foreground `serve` never removes a configuration that `install` owns.
#[derive(Debug)]
pub struct ConfigLease {
    manager: ConfigManager,
    enabled_here: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub enabled: bool,
    pub config_path: PathBuf,
    pub unchanged_since_enable: bool,
}

impl ConfigManager {
    /// A symbolic link to the config is followed: the target file is edited
    /// in place and the link itself is left untouched.
    pub fn new(config_path: PathBuf, state_path: PathBuf, backup_dir: PathBuf) -> Self {
        let lock_path = state_path.with_extension("lock");
        Self {
            config_path: resolve_config_path(config_path),
            state_path,
            backup_dir,
            lock_path,
        }
    }

    pub fn enable(&self, loopback_base_url: &str) -> Result<ConfigLease> {
        let _lock = self.lock()?;
        let enabled_here = self.enable_locked(loopback_base_url)?;
        Ok(ConfigLease {
            manager: self.clone(),
            enabled_here,
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
        let enabled = std::str::from_utf8(&current)
            .ok()
            .and_then(|text| find_region(text, &state.managed_block).ok().flatten())
            .is_some_and(|region| region.managed_lines_canonical);
        Ok(Status {
            enabled,
            config_path: self.config_path.clone(),
            unchanged_since_enable: enabled && sha256_bytes(&current) == state.managed_sha256,
        })
    }

    /// Returns true when this call changed the configuration to enabled.
    fn enable_locked(&self, loopback_base_url: &str) -> Result<bool> {
        if self.state_path.exists() {
            let state = self.load_state()?;
            ensure_same_path(&state.config_path, &self.config_path)?;
            match state.phase {
                Phase::Enabling => {
                    // Complete the interrupted enable, then apply the
                    // requested endpoint on top of it.
                    self.finish_enabling(state)?;
                    self.enable_locked(loopback_base_url)?;
                    return Ok(true);
                }
                Phase::Active => {
                    let same_endpoint = state.managed_block
                        == managed_section(loopback_base_url, false)
                        || state.managed_block == managed_section(loopback_base_url, true);
                    if same_endpoint && self.ensure_managed_block(&state)? {
                        return Ok(false);
                    }
                    self.begin_disabling(state)?;
                }
                Phase::Disabling { .. } => self.finish_disabling(state)?,
            }
        }
        self.begin_enabling(loopback_base_url)?;
        Ok(true)
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
        let output = format!("{section}{text}");
        ensure_same_user_document(text, &output).context(
            "the CodexMux provider cannot be added to this Codex config; \
             define providers as [model_providers.<name>] tables instead of a \
             [model_providers] header or an inline table",
        )?;
        let output = output.into_bytes();

        fs::create_dir_all(&self.backup_dir)?;
        let backup_path = self
            .backup_dir
            .join(format!("config.{}.toml", crate::fsutil::unix_time_nanos()));
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
        self.save_state(&state)?;
        remove_backup(&state);
        Ok(())
    }

    fn finish_enabling(&self, mut state: ConfigState) -> Result<()> {
        let current = read_optional(&self.config_path)?;
        let has_block = std::str::from_utf8(&current)
            .ok()
            .and_then(|text| find_region(text, &state.managed_block).ok().flatten())
            .is_some();
        if !has_block {
            let original = self.read_backup(&state)?;
            anyhow::ensure!(
                current == original,
                "Codex config changed while CodexMux activation was incomplete"
            );
            let output = managed_output(&state, &original)?;
            replace_if_unchanged(&self.config_path, &current, &output, state.original_exists)?;
        }
        state.phase = Phase::Active;
        self.save_state(&state)?;
        remove_backup(&state);
        Ok(())
    }

    /// Repair the managed block in place. Returns false when the block is
    /// gone entirely (the file was deleted, reset, or the block was removed),
    /// so the caller starts a fresh enable instead.
    fn ensure_managed_block(&self, state: &ConfigState) -> Result<bool> {
        let Some(current) = read_existing(&self.config_path)? else {
            return Ok(false);
        };
        let text = std::str::from_utf8(&current).context("Codex config is not UTF-8")?;
        let Some(region) = find_region(text, &state.managed_block)? else {
            return Ok(false);
        };
        if region.is_canonical() {
            return Ok(true);
        }
        // Desktop rewrites have replaced the selector line and appended root
        // keys inside the block before. Put the canonical block back and move
        // any user lines below it, keeping every byte outside the block.
        let rebuilt = format!(
            "{}{}{}{}",
            &text[..region.start],
            state.managed_block,
            region.user_lines,
            &text[region.end..]
        );
        ensure_same_user_document(text, &rebuilt)
            .context("refusing to repair the CodexMux block: it would change other settings")?;
        replace_if_unchanged(&self.config_path, &current, rebuilt.as_bytes(), true)?;
        let mut state = state.clone();
        state.managed_sha256 = sha256_bytes(rebuilt.as_bytes());
        self.save_state(&state)?;
        Ok(true)
    }

    fn disable_locked(&self) -> Result<()> {
        if !self.state_path.exists() {
            return Ok(());
        }
        let state = self.load_state()?;
        ensure_same_path(&state.config_path, &self.config_path)?;
        match state.phase {
            Phase::Enabling => {
                let current = read_optional(&self.config_path)?;
                if let Ok(original) = self.read_backup(&state)
                    && current == original
                {
                    self.forget(&state)?;
                    return Ok(());
                }
                self.begin_disabling(state)
            }
            Phase::Active => self.begin_disabling(state),
            Phase::Disabling { .. } => self.finish_disabling(state),
        }
    }

    fn begin_disabling(&self, mut state: ConfigState) -> Result<()> {
        let Some(current) = read_existing(&self.config_path)? else {
            // The config is gone; there is nothing left to restore.
            return self.forget(&state);
        };
        let Some(restored) = restored_config(&state, &current)? else {
            // The block was removed by hand; the file is already restored.
            return self.forget(&state);
        };
        state.phase = Phase::Disabling {
            restored_sha256: sha256_bytes(&restored),
        };
        self.save_state(&state)?;
        replace_if_unchanged(&self.config_path, &current, &restored, true)?;
        self.remove_restored_config_if_absent(&state, &restored)?;
        self.forget(&state)
    }

    fn finish_disabling(&self, state: ConfigState) -> Result<()> {
        let Phase::Disabling { restored_sha256 } = &state.phase else {
            unreachable!("finish_disabling requires the disabling phase");
        };
        let Some(current) = read_existing(&self.config_path)? else {
            return self.forget(&state);
        };
        if sha256_bytes(&current) == *restored_sha256 {
            self.remove_restored_config_if_absent(&state, &current)?;
            return self.forget(&state);
        }
        let Some(restored) = restored_config(&state, &current)? else {
            return self.forget(&state);
        };
        anyhow::ensure!(
            sha256_bytes(&restored) == *restored_sha256,
            "Codex config changed while CodexMux restoration was incomplete"
        );
        replace_if_unchanged(&self.config_path, &current, &restored, true)?;
        self.remove_restored_config_if_absent(&state, &restored)?;
        self.forget(&state)
    }

    fn remove_restored_config_if_absent(&self, state: &ConfigState, restored: &[u8]) -> Result<()> {
        if !state.original_exists && restored.is_empty() && self.config_path.exists() {
            atomic_remove_if_unchanged(&self.config_path, restored)?;
        }
        Ok(())
    }

    /// Drop the managed state and its backup once nothing remains to undo.
    fn forget(&self, state: &ConfigState) -> Result<()> {
        remove_backup(state);
        match fs::remove_file(&self.state_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error)
                .with_context(|| format!("failed to remove {}", self.state_path.display())),
        }
    }

    fn read_backup(&self, state: &ConfigState) -> Result<Vec<u8>> {
        let original = fs::read(&state.backup_path).with_context(|| {
            format!(
                "Codex config backup {} is missing",
                state.backup_path.display()
            )
        })?;
        anyhow::ensure!(
            sha256_bytes(&original) == state.original_sha256,
            "Codex config backup hash mismatch"
        );
        Ok(original)
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
        file.lock()
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
        if !self.enabled_here {
            return Ok(());
        }
        self.manager.disable()
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
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(resolve_config_path(path))
}

/// Resolve symbolic links so every read and atomic replacement targets the
/// real file. A config that does not exist yet resolves through its parent.
fn resolve_config_path(path: PathBuf) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map(|parent| parent.join(name))
            .unwrap_or(path),
        _ => path,
    }
}

fn same_config_file(left: &Path, right: &Path) -> bool {
    left == right || resolve_config_path(left.to_owned()) == resolve_config_path(right.to_owned())
}

fn ensure_same_path(recorded: &Path, current: &Path) -> Result<()> {
    anyhow::ensure!(
        same_config_file(recorded, current),
        "managed state points to a different Codex config file"
    );
    Ok(())
}

fn managed_section(loopback_base_url: &str, trailing_blank_line: bool) -> String {
    let base_url = toml_edit::Value::from(loopback_base_url).to_string();
    let block = format!(
        "{START_MARKER}\n\
         {SELECTOR_KEY} = \"codexmux\"\n\
         {PROVIDER_KEY} = {{ name = \"CodexMux\", base_url = {base_url}, wire_api = \"responses\", requires_openai_auth = true, supports_websockets = false, env_http_headers = {{ x-codexmux-token = \"{PROXY_TOKEN_ENV}\" }} }}\n\
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

/// The user's file without the CodexMux block, or `None` when no block is
/// left to remove. Lines another writer placed inside the block stay where
/// the block was.
fn restored_config(state: &ConfigState, current: &[u8]) -> Result<Option<Vec<u8>>> {
    let text = std::str::from_utf8(current).context("Codex config is not UTF-8")?;
    let Some(region) = find_region(text, &state.managed_block)? else {
        return Ok(None);
    };
    let restored = format!(
        "{}{}{}",
        &text[..region.start],
        region.user_lines,
        &text[region.end..]
    );
    ensure_same_user_document(&restored, text)
        .context("removing CodexMux would change other Codex settings")?;
    Ok(Some(restored.into_bytes()))
}

/// The marker-delimited CodexMux block inside a config.
struct Region {
    /// Byte offset of the start marker line.
    start: usize,
    /// Byte offset just past the end marker line and the block's own blank
    /// separator line.
    end: usize,
    /// Lines between the markers that CodexMux did not write.
    user_lines: String,
    /// The selector and provider lines are exactly the ones CodexMux wrote.
    managed_lines_canonical: bool,
    /// The bytes of the region equal the managed block.
    exact: bool,
}

impl Region {
    fn is_canonical(&self) -> bool {
        self.exact
    }
}

fn find_region(text: &str, managed_block: &str) -> Result<Option<Region>> {
    let starts = marker_lines(text, START_MARKER);
    let ends = marker_lines(text, END_MARKER);
    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => return Ok(None),
        ([_], [_]) => {}
        _ => bail!(
            "the CodexMux block markers in the Codex config are damaged; \
             remove the lines between \"{START_MARKER}\" and \"{END_MARKER}\" by hand"
        ),
    }
    let (start, start_line_end) = starts[0];
    let (end_marker, mut end) = ends[0];
    anyhow::ensure!(
        start < end_marker,
        "the CodexMux block markers in the Codex config are out of order"
    );
    if managed_block.ends_with("\n\n") && text[end..].starts_with('\n') {
        end += 1;
    }
    let canonical_lines: Vec<&str> = managed_block
        .lines()
        .filter(|line| line.starts_with(SELECTOR_KEY) || line.starts_with(PROVIDER_KEY))
        .collect();
    let mut managed = Vec::new();
    let mut user_lines = String::new();
    for line in text[start_line_end..end_marker].split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            continue;
        }
        if is_managed_key_line(content) {
            managed.push(content);
        } else {
            user_lines.push_str(line);
            if !line.ends_with('\n') {
                user_lines.push('\n');
            }
        }
    }
    Ok(Some(Region {
        start,
        end,
        managed_lines_canonical: managed == canonical_lines,
        exact: &text[start..end] == managed_block,
        user_lines,
    }))
}

/// `(line start, offset past the line break)` of every line equal to `marker`.
fn marker_lines(text: &str, marker: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == marker {
            found.push((offset, offset + line.len()));
        }
        offset += line.len();
    }
    found
}

/// Lines for the two keys CodexMux owns inside its block.
fn is_managed_key_line(line: &str) -> bool {
    let line = line.trim_start();
    [SELECTOR_KEY, PROVIDER_KEY].iter().any(|key| {
        line.strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

/// Require `with_block` to be exactly `user` plus the CodexMux keys, compared
/// as parsed TOML so formatting cannot hide a semantic change.
fn ensure_same_user_document(user: &str, with_block: &str) -> Result<()> {
    let user = parse_table(user).context("the Codex config is not valid TOML")?;
    let with_block = parse_table(with_block).context("the result would not be valid TOML")?;
    anyhow::ensure!(
        without_managed_keys(with_block) == without_managed_keys(user),
        "the result would change settings outside the CodexMux block"
    );
    Ok(())
}

fn parse_table(text: &str) -> Result<toml::Table> {
    if text.trim().is_empty() {
        return Ok(toml::Table::new());
    }
    Ok(toml::from_str(text)?)
}

fn without_managed_keys(mut table: toml::Table) -> toml::Table {
    table.remove(SELECTOR_KEY);
    if let Some(toml::Value::Table(providers)) = table.get_mut("model_providers") {
        providers.remove("codexmux");
        if providers.is_empty() {
            table.remove("model_providers");
        }
    }
    table
}

fn remove_backup(state: &ConfigState) {
    let _ = fs::remove_file(&state.backup_path);
}

fn read_optional(path: &Path) -> Result<Vec<u8>> {
    Ok(read_existing(path)?.unwrap_or_default())
}

fn read_existing(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
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
    if [START_MARKER, END_MARKER]
        .iter()
        .any(|marker| text.contains(marker))
    {
        bail!("Codex config already contains a CodexMux managed marker");
    }
    if text.trim().is_empty() {
        return Ok(());
    }
    let document = text
        .parse::<DocumentMut>()
        .context("existing Codex config is invalid TOML")?;
    for key in ["model_catalog_json", "model_provider"] {
        if document.get(key).is_some() {
            bail!("Codex config already defines {key}; refusing to overwrite it");
        }
    }
    if let Some(providers) = document.get("model_providers") {
        let providers = providers
            .as_table_like()
            .context("existing model_providers must be a TOML table")?;
        if providers.contains_key("codexmux") {
            bail!("Codex config already defines model_providers.codexmux");
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

    #[cfg(unix)]
    #[test]
    fn directory_symlink_is_the_same_codex_config() {
        let root = tempdir().unwrap();
        let real_dir = root.path().join("real");
        fs::create_dir(&real_dir).unwrap();
        let config = real_dir.join("config.toml");
        fs::write(&config, b"model = \"gpt\"\n").unwrap();
        let link_dir = root.path().join("link");
        std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();

        let state = root.path().join("state.json");
        let backups = root.path().join("backups");
        let via_link =
            ConfigManager::new(link_dir.join("config.toml"), state.clone(), backups.clone());
        via_link.enable(LOOPBACK_BASE_URL).unwrap();

        let via_real = ConfigManager::new(config.clone(), state, backups);
        let status = via_real.status().unwrap();
        assert!(status.enabled);
        via_real.disable().unwrap();
        assert_eq!(fs::read(&config).unwrap(), b"model = \"gpt\"\n");
    }

    #[test]
    fn existing_model_providers_are_preserved() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        let original = b"[model_providers.deepseek]\nname = \"DeepSeek\"\nbase_url = \"https://example.com/v1\"\n";
        fs::write(&config, original).unwrap();

        let lease = manager(root.path(), config.clone())
            .enable(LOOPBACK_BASE_URL)
            .unwrap();
        let enabled = fs::read_to_string(&config).unwrap();
        let document = enabled.parse::<DocumentMut>().unwrap();
        let providers = document["model_providers"].as_table_like().unwrap();
        assert!(providers.contains_key("codexmux"));
        assert!(providers.contains_key("deepseek"));

        lease.restore().unwrap();
        assert_eq!(fs::read(&config).unwrap(), original);
    }

    #[test]
    fn existing_codexmux_provider_is_rejected() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        fs::write(&config, b"[model_providers.codexmux]\nname = \"custom\"\n").unwrap();

        let error = manager(root.path(), config)
            .enable(LOOPBACK_BASE_URL)
            .unwrap_err();
        assert!(error.to_string().contains("model_providers.codexmux"));
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
    fn enable_heals_a_selector_edit_inside_the_managed_block() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        fs::write(&config, b"model = \"gpt\"\n").unwrap();
        let manager = manager(root.path(), config.clone());
        let lease = manager.enable(LOOPBACK_BASE_URL).unwrap();
        drop(lease);

        let enabled = fs::read_to_string(&config).unwrap();
        let stomped = enabled.replace(
            "model_provider = \"codexmux\"",
            "model_provider = \"openai\"",
        );
        fs::write(&config, &stomped).unwrap();

        let lease = manager.enable(LOOPBACK_BASE_URL).unwrap();
        let healed = fs::read_to_string(&config).unwrap();
        // The selector is restored and the file is exactly the enabled shape
        // again, so a future desktop rewrite is healed the same way.
        assert_eq!(healed, enabled);
        let document = healed.parse::<DocumentMut>().unwrap();
        assert_eq!(document["model_provider"].as_str().unwrap(), "codexmux");
        // The second lease only repaired an existing enable.
        lease.restore().unwrap();
        assert!(manager.status().unwrap().enabled);
        manager.disable().unwrap();
        assert_eq!(fs::read(&config).unwrap(), b"model = \"gpt\"\n");
    }

    #[test]
    fn enable_adds_the_block_again_after_it_was_removed_by_hand() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        fs::write(&config, b"model = \"gpt\"\n").unwrap();
        let manager = manager(root.path(), config.clone());
        drop(manager.enable(LOOPBACK_BASE_URL).unwrap());

        let current = fs::read_to_string(&config).unwrap();
        let start = current.find(START_MARKER).unwrap();
        let end = current.find(END_MARKER).unwrap() + END_MARKER.len() + 2;
        let removed = format!("{}{}", &current[..start], &current[end..]);
        assert_eq!(removed, "model = \"gpt\"\n");
        fs::write(&config, &removed).unwrap();
        assert!(!manager.status().unwrap().enabled);

        let lease = manager.enable(LOOPBACK_BASE_URL).unwrap();
        assert!(manager.status().unwrap().enabled);
        lease.restore().unwrap();
        assert_eq!(fs::read(&config).unwrap(), b"model = \"gpt\"\n");
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
        let restored = restored_config(&state, &current).unwrap().unwrap();
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
    fn symbolic_link_config_is_followed_and_kept() {
        use std::os::unix::fs::symlink;

        let root = tempdir().unwrap();
        let target = root.path().join("target.toml");
        let config = root.path().join("config.toml");
        fs::write(&target, b"model = \"gpt\"\n").unwrap();
        symlink(&target, &config).unwrap();

        let lease = manager(root.path(), config.clone())
            .enable(LOOPBACK_BASE_URL)
            .unwrap();
        assert!(
            fs::symlink_metadata(&config)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fs::read_to_string(&target).unwrap().contains(START_MARKER));
        lease.restore().unwrap();
        assert!(
            fs::symlink_metadata(&config)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&target).unwrap(), b"model = \"gpt\"\n");
    }

    fn enabled_text(root: &Path, config: &Path, original: &str) -> (ConfigManager, String) {
        fs::write(config, original).unwrap();
        let manager = manager(root, config.to_owned());
        drop(manager.enable(LOOPBACK_BASE_URL).unwrap());
        let text = fs::read_to_string(config).unwrap();
        (manager, text)
    }

    #[test]
    fn enable_rejects_configs_that_cannot_take_the_provider_block() {
        for original in [
            "[model_providers]\nother = { name = \"Other\" }\n",
            "model_providers = { other = { name = \"Other\" } }\n",
        ] {
            let root = tempdir().unwrap();
            let config = root.path().join("config.toml");
            fs::write(&config, original).unwrap();
            let manager = manager(root.path(), config.clone());
            let error = manager.enable(LOOPBACK_BASE_URL).unwrap_err();
            assert!(
                format!("{error:#}").contains("[model_providers.<name>]"),
                "{error:#}"
            );
            assert_eq!(fs::read_to_string(&config).unwrap(), original);
            assert!(!manager.state_path.exists());
        }
    }

    #[test]
    fn backup_is_removed_once_the_enable_is_active() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        let (manager, _) = enabled_text(root.path(), &config, "model = \"gpt\"\n");
        let state = manager.load_state().unwrap();
        assert!(!state.backup_path.exists());
        manager.disable().unwrap();
        assert_eq!(fs::read(&config).unwrap(), b"model = \"gpt\"\n");
    }

    #[test]
    fn disable_restores_the_original_after_a_selector_rewrite() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        let (manager, enabled) = enabled_text(root.path(), &config, "model = \"gpt\"\n");
        let stomped = enabled.replace(
            "model_provider = \"codexmux\"",
            "model_provider = \"openai\"",
        );
        fs::write(&config, stomped).unwrap();
        manager.disable().unwrap();
        assert_eq!(fs::read(&config).unwrap(), b"model = \"gpt\"\n");
        assert!(!manager.state_path.exists());
    }

    /// Desktop appends a new root key after the last root key, which is
    /// inside the block when the user's file starts with a table.
    fn with_key_inside_block(enabled: &str) -> String {
        enabled.replace(
            &format!("{END_MARKER}\n"),
            &format!("model = \"gpt-5\"\n{END_MARKER}\n"),
        )
    }

    #[test]
    fn disable_keeps_user_lines_that_landed_inside_the_block() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        let (manager, enabled) = enabled_text(root.path(), &config, "[features]\nx = true\n");
        fs::write(&config, with_key_inside_block(&enabled)).unwrap();
        manager.disable().unwrap();
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "model = \"gpt-5\"\n[features]\nx = true\n"
        );
    }

    #[test]
    fn heal_moves_user_lines_below_the_block() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        let (manager, enabled) = enabled_text(root.path(), &config, "[features]\nx = true\n");
        fs::write(&config, with_key_inside_block(&enabled)).unwrap();

        let lease = manager.enable(LOOPBACK_BASE_URL).unwrap();
        let healed = fs::read_to_string(&config).unwrap();
        let state = manager.load_state().unwrap();
        assert!(healed.starts_with(&state.managed_block));
        let document = healed.parse::<DocumentMut>().unwrap();
        assert_eq!(document["model"].as_str(), Some("gpt-5"));
        assert_eq!(document["model_provider"].as_str(), Some("codexmux"));
        assert!(manager.status().unwrap().unchanged_since_enable);
        // This lease found an existing enable, so it must not undo it.
        lease.restore().unwrap();
        assert!(manager.status().unwrap().enabled);

        manager.disable().unwrap();
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "model = \"gpt-5\"\n[features]\nx = true\n"
        );
    }

    #[test]
    fn heal_refuses_to_change_settings_outside_the_block() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        let (manager, enabled) = enabled_text(root.path(), &config, "model = \"gpt\"\n");
        // A table header above the CodexMux keys turns them into keys of
        // that table. Rebuilding the block would move them back to the root,
        // which changes what the file means, so the repair must refuse.
        let damaged = enabled.replace(
            &format!("{START_MARKER}\n"),
            &format!("{START_MARKER}\n[projects.\"/tmp\"]\ntrust_level = \"trusted\"\n"),
        );
        fs::write(&config, &damaged).unwrap();
        let error = manager.enable(LOOPBACK_BASE_URL).unwrap_err();
        assert!(
            format!("{error:#}").contains("refusing to repair"),
            "{error:#}"
        );
        assert_eq!(fs::read_to_string(&config).unwrap(), damaged);
    }

    #[test]
    fn leading_blank_lines_survive_a_heal() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        let original = "\n\nmodel = \"gpt\"\n";
        let (manager, enabled) = enabled_text(root.path(), &config, original);
        let stomped = enabled.replace(
            "model_provider = \"codexmux\"",
            "model_provider = \"openai\"",
        );
        fs::write(&config, stomped).unwrap();
        drop(manager.enable(LOOPBACK_BASE_URL).unwrap());
        assert_eq!(fs::read_to_string(&config).unwrap(), enabled);
        manager.disable().unwrap();
        assert_eq!(fs::read_to_string(&config).unwrap(), original);
    }

    #[test]
    fn disable_clears_state_when_the_config_was_deleted() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        let (manager, _) = enabled_text(root.path(), &config, "model = \"gpt\"\n");
        fs::remove_file(&config).unwrap();
        manager.disable().unwrap();
        assert!(!manager.state_path.exists());
        assert!(!config.exists());
    }

    #[test]
    fn enable_recreates_a_deleted_config() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        let (manager, _) = enabled_text(root.path(), &config, "model = \"gpt\"\n");
        fs::remove_file(&config).unwrap();
        let lease = manager.enable(LOOPBACK_BASE_URL).unwrap();
        assert!(manager.status().unwrap().enabled);
        lease.restore().unwrap();
        assert!(!config.exists());
    }

    #[test]
    fn damaged_markers_are_reported_without_writing() {
        let root = tempdir().unwrap();
        let config = root.path().join("config.toml");
        let (manager, enabled) = enabled_text(root.path(), &config, "model = \"gpt\"\n");
        let damaged = enabled.replace(&format!("{END_MARKER}\n"), "");
        fs::write(&config, &damaged).unwrap();
        let error = manager.disable().unwrap_err();
        assert!(format!("{error:#}").contains("damaged"), "{error:#}");
        assert_eq!(fs::read_to_string(&config).unwrap(), damaged);
    }
}
