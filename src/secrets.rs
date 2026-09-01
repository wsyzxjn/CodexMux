use std::{fs, io::Write, path::Path};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;

use crate::config::Credentials;

/// Load credentials, atomically adding a CPA management key when the file omits one.
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
    let mut credentials: Credentials =
        serde_json::from_slice(&bytes).context("invalid credentials.json")?;
    let migrated = credentials.cpa_management_key.trim().is_empty();
    if migrated {
        credentials.cpa_management_key = uuid::Uuid::new_v4().simple().to_string();
    }
    credentials.validate()?;
    if migrated {
        save(path, &credentials)?;
    }
    Ok(credentials)
}

pub fn save(path: &Path, credentials: &Credentials) -> Result<()> {
    credentials.validate()?;
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
    use tempfile::tempdir;

    use super::*;

    fn credentials() -> Credentials {
        Credentials {
            proxy_token: "proxy-token".into(),
            cpa_token: "cpa-token".into(),
            cpa_management_key: "management-key".into(),
        }
    }

    #[test]
    fn writes_private_permissions() {
        let root = tempdir().unwrap();
        let path = root.path().join("credentials.json");
        save(&path, &credentials()).unwrap();
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn requires_separate_nonempty_tokens() {
        let mut credentials = credentials();
        credentials.cpa_token = credentials.proxy_token.clone();
        assert!(credentials.validate().is_err());
        credentials.cpa_token.clear();
        assert!(credentials.validate().is_err());
    }

    #[test]
    fn credentials_without_management_key_are_upgraded() {
        let root = tempdir().unwrap();
        let path = root.path().join("credentials.json");
        fs::write(
            &path,
            br#"{"proxy_token":"proxy-token","cpa_token":"cpa-token"}"#,
        )
        .unwrap();
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let credentials = load(&path).unwrap();
        assert!(!credentials.cpa_management_key.is_empty());
        assert_ne!(credentials.cpa_management_key, credentials.proxy_token);
        assert_ne!(credentials.cpa_management_key, credentials.cpa_token);

        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            persisted["cpa_management_key"],
            credentials.cpa_management_key
        );
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
