use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use anyhow::{Context, Result, bail};

use crate::{config::Credentials, fsutil::atomic_write_private};

/// Load and validate `credentials.json`, which must be a private regular file.
pub fn load(path: &Path) -> Result<Credentials> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to stat credentials file {}", path.display()))?;
    if !metadata.file_type().is_file() {
        bail!("credentials path must be a regular file");
    }
    if metadata.permissions().mode() & 0o777 != 0o600 {
        bail!("credentials file permissions must be exactly 0600");
    }
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read credentials file {}", path.display()))?;
    let credentials: Credentials =
        serde_json::from_slice(&bytes).context("invalid credentials.json")?;
    credentials.validate()?;
    Ok(credentials)
}

pub fn save(path: &Path, credentials: &Credentials) -> Result<()> {
    credentials.validate()?;
    atomic_write_private(path, &serde_json::to_vec_pretty(credentials)?)
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
    fn writes_private_permissions_and_round_trips() {
        let root = tempdir().unwrap();
        let path = root.path().join("credentials.json");
        save(&path, &credentials()).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(load(&path).unwrap().cpa_token, "cpa-token");
    }

    #[test]
    fn rejects_credentials_readable_by_others() {
        let root = tempdir().unwrap();
        let path = root.path().join("credentials.json");
        save(&path, &credentials()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.to_string().contains("exactly 0600"));
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
    fn credentials_without_a_management_key_are_rejected() {
        let root = tempdir().unwrap();
        let path = root.path().join("credentials.json");
        atomic_write_private(
            &path,
            br#"{"proxy_token":"proxy-token","cpa_token":"cpa-token"}"#,
        )
        .unwrap();
        let error = load(&path).unwrap_err();
        assert!(format!("{error:#}").contains("cpa_management_key"));
    }
}
