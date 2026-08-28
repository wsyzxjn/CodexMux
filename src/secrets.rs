use std::{fs, io::Write, path::Path};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;

use crate::config::Credentials;

pub fn load(path: &Path) -> Result<Credentials> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to stat credentials file {}", path.display()))?;
    if !metadata.file_type().is_file() {
        bail!("credentials path must be a regular file");
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o777 != 0o600 {
        bail!("credentials file permissions must be exactly 0600");
    }
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read credentials file {}", path.display()))?;
    let credentials: Credentials =
        serde_json::from_slice(&bytes).context("invalid credentials.json")?;
    if credentials.schema_version != 1 {
        bail!(
            "unsupported credentials schema version {}",
            credentials.schema_version
        );
    }
    if credentials.proxy_token.trim().is_empty() {
        bail!("credentials.json has an empty proxy_token");
    }
    for (provider, credential) in &credentials.providers {
        if credential.trim().is_empty() {
            bail!("provider {provider} has an empty credential");
        }
    }
    Ok(credentials)
}

pub fn save(path: &Path, credentials: &Credentials) -> Result<()> {
    if credentials.schema_version != 1 {
        bail!(
            "unsupported credentials schema version {}",
            credentials.schema_version
        );
    }
    if credentials.proxy_token.trim().is_empty() {
        bail!("credentials.json has an empty proxy_token");
    }
    for (provider, credential) in &credentials.providers {
        if credential.trim().is_empty() {
            bail!("provider {provider} has an empty credential");
        }
    }
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(&serde_json::to_vec_pretty(credentials)?)?;
    temporary.as_file().sync_all()?;
    #[cfg(unix)]
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to atomically replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn writes_private_permissions() {
        let root = tempdir().unwrap();
        let path = root.path().join("credentials.json");
        save(
            &path,
            &Credentials {
                schema_version: 1,
                proxy_token: "token".into(),
                providers: HashMap::new(),
            },
        )
        .unwrap();
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
