use std::{fs, io::Read, os::unix::fs::PermissionsExt, path::Path};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use tar::Archive;

use super::{
    CPA_BINARY_NAME, CPA_REPO, InstalledVersion, binary_path, lock,
    profiles::set_cpa_autostart,
    service::{Launchd, ServiceControl, restart_service},
    supported_platform,
    update::{download_verified_asset, resolve_release},
    version_path,
};
use crate::{
    config::{Cpa, Paths},
    fsutil::{atomic_write, sha256_bytes},
};

/// Resolve the latest stable CPA release, verify it, install it, and start it.
///
/// The archive is checked against the published digests and unpacked in
/// memory before anything on disk or in launchd changes.
pub fn install(paths: &Paths, cpa: &Cpa, token: &str) -> Result<InstalledVersion> {
    supported_platform()?;
    let _lock = lock(paths)?;
    let release = resolve_release(None)?;
    let archive = download_verified_asset(&release)?;
    let binary = extract_binary(&archive)?;
    let version = InstalledVersion {
        version: release.version,
        sha256: release.asset.sha256,
        source: Some(format!("github:{CPA_REPO}")),
        published_at: release.published_at,
        updated_at: Some(crate::fsutil::unix_time_nanos().to_string()),
    };
    install_and_start(&Launchd, paths, cpa, token, binary, version).context("CPA install failed")
}

/// Install from a local release archive whose SHA-256 must equal `sha256`.
/// Used by `codexmux cpa install --archive <path>` for offline installs.
pub fn install_from_archive(
    paths: &Paths,
    cpa: &Cpa,
    archive: &Path,
    version: &str,
    sha256: &str,
    token: &str,
) -> Result<InstalledVersion> {
    supported_platform()?;
    // Hash and unpack the same bytes, so the file cannot change in between.
    let bytes =
        fs::read(archive).with_context(|| format!("failed to read {}", archive.display()))?;
    let actual = sha256_bytes(&bytes);
    anyhow::ensure!(
        actual == sha256,
        "CLIProxyAPI archive digest mismatch: expected {sha256}, got {actual}"
    );
    let binary = extract_binary(&bytes)?;
    let version = InstalledVersion {
        version: version.to_owned(),
        sha256: actual,
        // The pinned digest is the one GitHub publishes for this release.
        source: Some(format!("github:{CPA_REPO}")),
        published_at: None,
        updated_at: Some(crate::fsutil::unix_time_nanos().to_string()),
    };
    let _lock = lock(paths)?;
    install_and_start(&Launchd, paths, cpa, token, binary, version)
        .context("offline CPA install failed")
}

/// Install a verified binary, start it, and require `/models` to answer.
/// Only a validated install records the startup preference, as an explicit
/// start would.
fn install_and_start(
    services: &dyn ServiceControl,
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
    binary: Vec<u8>,
    version: InstalledVersion,
) -> Result<InstalledVersion> {
    let target = Installation {
        binary: Some(binary),
        version: Some(serde_json::to_vec_pretty(&version)?),
    };
    let replaced = replace_installation(services, paths, cpa, token, &target, Validation::Always)?;
    save_rollback_point(paths, &replaced);
    if let Err(error) = set_cpa_autostart(&paths.cpa_profiles, true) {
        tracing::warn!(error = %format!("{error:#}"), "failed to save CPA autostart preference");
    }
    Ok(version)
}

/// The files that make up one installed CPA version; `None` means absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Installation {
    pub(super) binary: Option<Vec<u8>>,
    pub(super) version: Option<Vec<u8>>,
}

impl Installation {
    pub(super) fn read(paths: &Paths) -> Result<Self> {
        Ok(Self {
            binary: read_optional(&binary_path(paths))?,
            version: read_optional(&version_path(paths))?,
        })
    }

    /// Write every present file and remove every absent one; an absent
    /// version is never written as an empty record.
    fn write(&self, paths: &Paths) -> Result<()> {
        write_optional(&binary_path(paths), self.binary.as_deref(), true)?;
        write_optional(&version_path(paths), self.version.as_deref(), false)
    }
}

/// When a replaced installation must prove it serves `/models`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Validation {
    /// Start CPA and validate it even if it was not running (install).
    Always,
    /// Restart and validate only a running CPA (update, rollback).
    IfRunning,
}

/// Replace the installed files with `target` and, when validation applies,
/// restart CPA and require `/models` to answer. On failure the new service is
/// stopped, the previous files are restored, and a previously running CPA is
/// restarted and revalidated; the returned error says exactly which of those
/// steps worked. Returns the replaced installation.
pub(super) fn replace_installation(
    services: &dyn ServiceControl,
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
    target: &Installation,
    validation: Validation,
) -> Result<Installation> {
    let previous = Installation::read(paths)?;
    let was_running = services.cpa_loaded()?;
    let result = target.write(paths).and_then(|()| {
        if validation == Validation::Always || was_running {
            restart_service(services, paths, cpa, token)?;
            services.wait_for_cpa(cpa, token)?;
        }
        Ok(())
    });
    match result {
        Ok(()) => Ok(previous),
        Err(error) => Err(recover(
            services,
            paths,
            cpa,
            token,
            &previous,
            was_running,
            error,
        )),
    }
}

/// Undo a failed replacement and describe the outcome on top of `error`.
fn recover(
    services: &dyn ServiceControl,
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
    previous: &Installation,
    was_running: bool,
    error: anyhow::Error,
) -> anyhow::Error {
    // Stop first: a start that found the job loaded with an unchanged config
    // would otherwise leave the failed version running.
    if let Err(stop_error) = services.stop_cpa() {
        let outcome = match previous.write(paths) {
            Ok(()) => "the previously installed CPA files are restored, but the failed version may still be running".to_owned(),
            Err(restore_error) => {
                format!("restoring the previously installed CPA files also failed: {restore_error:#}")
            }
        };
        return error.context(format!(
            "stopping the failed CPA also failed: {stop_error:#}; {outcome}"
        ));
    }
    if let Err(restore_error) = previous.write(paths) {
        return error.context(format!(
            "restoring the previously installed CPA files also failed: {restore_error:#}"
        ));
    }
    if previous.binary.is_none() {
        return error.context("removed the incomplete CPA installation");
    }
    if !was_running {
        return error.context("restored the previously installed CPA version");
    }
    match restart_service(services, paths, cpa, token)
        .and_then(|()| services.wait_for_cpa(cpa, token))
    {
        Ok(()) => error.context("restored and restarted the previously installed CPA version"),
        Err(restart_error) => error.context(format!(
            "restored the previously installed CPA files, but restarting them failed: {restart_error:#}"
        )),
    }
}

/// Keep the version a successful install or update replaced as the rollback
/// point. The version record is written last, so an interrupted save never
/// offers a rollback to an unmatched binary.
pub(super) fn save_rollback_point(paths: &Paths, replaced: &Installation) {
    let result = (|| -> Result<()> {
        remove_optional(&previous_version_path(paths))?;
        remove_optional(&previous_binary_path(paths))?;
        if let (Some(binary), Some(version)) = (&replaced.binary, &replaced.version) {
            write_optional(&previous_binary_path(paths), Some(binary), true)?;
            write_optional(&previous_version_path(paths), Some(version), false)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        tracing::warn!(error = %format!("{error:#}"), "failed to save the CPA rollback point");
    }
}

pub(super) fn previous_binary_path(paths: &Paths) -> std::path::PathBuf {
    binary_path(paths).with_extension("previous")
}

pub(super) fn previous_version_path(paths: &Paths) -> std::path::PathBuf {
    version_path(paths).with_file_name("version.json.previous")
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn write_optional(path: &Path, bytes: Option<&[u8]>, executable: bool) -> Result<()> {
    let Some(bytes) = bytes else {
        return remove_optional(path);
    };
    atomic_write(path, bytes)?;
    if executable {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to make {} executable", path.display()))?;
    }
    Ok(())
}

pub(super) fn remove_optional(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

/// Unpack the CPA binary from an in-memory release archive, accepting only
/// the expected entry name.
/// Upper bound for the extracted CLIProxyAPI executable (about 60 MiB today),
/// so a malformed archive cannot expand without limit in memory.
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;

pub(super) fn extract_binary(archive: &[u8]) -> Result<Vec<u8>> {
    let mut archive = Archive::new(GzDecoder::new(archive));
    for entry in archive
        .entries()
        .context("failed to read the CLIProxyAPI release archive")?
    {
        let entry = entry.context("failed to read the CLIProxyAPI release archive")?;
        let is_binary = entry
            .path()
            .context("invalid entry name in the CLIProxyAPI release archive")?
            .file_name()
            .is_some_and(|name| name == CPA_BINARY_NAME);
        if !is_binary {
            continue;
        }
        let mut binary = Vec::new();
        entry
            .take(MAX_BINARY_BYTES + 1)
            .read_to_end(&mut binary)
            .context("failed to extract the CLIProxyAPI binary")?;
        anyhow::ensure!(
            !binary.is_empty(),
            "CLIProxyAPI archive contains an empty binary"
        );
        anyhow::ensure!(
            binary.len() as u64 <= MAX_BINARY_BYTES,
            "CLIProxyAPI binary in the archive exceeds {} MiB",
            MAX_BINARY_BYTES / (1024 * 1024)
        );
        return Ok(binary);
    }
    bail!("CLIProxyAPI archive does not contain {CPA_BINARY_NAME}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpa::{
        config_path, installed_version,
        profiles::cpa_autostart,
        test_support::{FakeServices, local_cpa, test_root},
    };

    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/cpa_release/mini_release.tar.gz");

    fn version_record(version: &str) -> Vec<u8> {
        serde_json::to_vec(&InstalledVersion {
            version: version.into(),
            sha256: "00".repeat(32),
            ..Default::default()
        })
        .unwrap()
    }

    fn installed(binary: &str, version: &str) -> Installation {
        Installation {
            binary: Some(binary.as_bytes().to_vec()),
            version: Some(version_record(version)),
        }
    }

    fn put(paths: &Paths, installation: &Installation) {
        installation.write(paths).unwrap();
    }

    #[test]
    fn extract_accepts_only_the_expected_binary_entry() {
        assert_eq!(
            extract_binary(FIXTURE).unwrap(),
            b"#!/bin/sh\necho fake-cli-proxy-api\n"
        );
        assert!(extract_binary(b"not a tarball").is_err());
    }

    #[test]
    fn a_validated_install_starts_cpa_and_records_the_version() {
        let root = test_root();
        let paths = &root.paths;
        let services = FakeServices::default();
        let binary = extract_binary(FIXTURE).unwrap();
        let version = InstalledVersion {
            version: "7.2.150".into(),
            sha256: sha256_bytes(FIXTURE),
            ..Default::default()
        };
        install_and_start(
            &services,
            paths,
            &local_cpa(8317),
            "cpa-token",
            binary,
            version,
        )
        .unwrap();

        assert_eq!(
            installed_version(paths).unwrap().unwrap().version,
            "7.2.150"
        );
        assert_eq!(
            fs::metadata(binary_path(paths))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert!(config_path(paths).is_file());
        assert_eq!(
            services.events(),
            [
                "bootstrap #!/bin/sh\necho fake-cli-proxy-api",
                "ready http://127.0.0.1:8317/v1"
            ]
        );
        assert!(cpa_autostart(&paths.cpa_profiles).unwrap());
        // A first install has nothing to roll back to.
        assert!(!previous_binary_path(paths).exists());
    }

    #[test]
    fn a_failed_first_install_removes_itself_without_claiming_a_restore() {
        let root = test_root();
        let paths = &root.paths;
        set_cpa_autostart(&paths.cpa_profiles, false).unwrap();
        let services = FakeServices::default();
        services.fail_next_ready("models never answered");

        let error = install_and_start(
            &services,
            paths,
            &local_cpa(8317),
            "cpa-token",
            b"new-binary".to_vec(),
            InstalledVersion::default(),
        )
        .unwrap_err();
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("removed the incomplete CPA installation"),
            "{rendered}"
        );
        assert!(rendered.contains("models never answered"), "{rendered}");
        assert!(!rendered.contains("restored"), "{rendered}");
        assert!(!binary_path(paths).exists());
        assert!(!version_path(paths).exists());
        assert!(!*services.loaded.borrow());
        // Validation never touched the startup preference.
        assert!(!cpa_autostart(&paths.cpa_profiles).unwrap());
    }

    #[test]
    fn a_failed_update_of_a_running_cpa_restarts_the_previous_version() {
        let root = test_root();
        let paths = &root.paths;
        let old = installed("old-binary", "7.2.147");
        put(paths, &old);
        let rollback_point = installed("older-binary", "7.2.140");
        write_optional(
            &previous_binary_path(paths),
            rollback_point.binary.as_deref(),
            true,
        )
        .unwrap();
        write_optional(
            &previous_version_path(paths),
            rollback_point.version.as_deref(),
            false,
        )
        .unwrap();
        let services = FakeServices::loaded();
        services.fail_next_ready("new version never answered");

        let error = replace_installation(
            &services,
            paths,
            &local_cpa(8317),
            "cpa-token",
            &installed("new-binary", "7.2.150"),
            Validation::IfRunning,
        )
        .unwrap_err();
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("restored and restarted the previously installed CPA version"),
            "{rendered}"
        );
        assert_eq!(Installation::read(paths).unwrap(), old);
        assert_eq!(
            services.events(),
            [
                "stop",
                "bootstrap new-binary",
                "ready http://127.0.0.1:8317/v1",
                "stop",
                "bootstrap old-binary",
                "ready http://127.0.0.1:8317/v1"
            ]
        );
        // The earlier rollback point is untouched by a failed update.
        assert_eq!(
            fs::read(previous_binary_path(paths)).unwrap(),
            b"older-binary"
        );
        assert_eq!(
            fs::read(previous_version_path(paths)).unwrap(),
            rollback_point.version.unwrap()
        );
    }

    #[test]
    fn a_failed_restart_of_the_previous_version_is_reported() {
        let root = test_root();
        let paths = &root.paths;
        put(paths, &installed("old-binary", "7.2.147"));
        let services = FakeServices::loaded();
        services.fail_next_ready("new version never answered");
        services.fail_next_ready("old version never answered");

        let error = replace_installation(
            &services,
            paths,
            &local_cpa(8317),
            "cpa-token",
            &installed("new-binary", "7.2.150"),
            Validation::IfRunning,
        )
        .unwrap_err();
        let rendered = format!("{error:#}");
        assert!(rendered.contains("restarting them failed"), "{rendered}");
        assert!(
            rendered.contains("old version never answered"),
            "{rendered}"
        );
        assert!(
            rendered.contains("new version never answered"),
            "{rendered}"
        );
        assert!(!rendered.contains("restored and restarted"), "{rendered}");
    }

    #[test]
    fn a_stopped_cpa_is_updated_in_place_without_starting_it() {
        let root = test_root();
        let paths = &root.paths;
        let old = installed("old-binary", "7.2.147");
        put(paths, &old);
        let services = FakeServices::default();
        let replaced = replace_installation(
            &services,
            paths,
            &local_cpa(8317),
            "cpa-token",
            &installed("new-binary", "7.2.150"),
            Validation::IfRunning,
        )
        .unwrap();
        assert_eq!(replaced, old);
        assert!(services.events().is_empty());
        assert_eq!(fs::read(binary_path(paths)).unwrap(), b"new-binary");
    }

    #[test]
    fn the_rollback_point_is_replaced_only_by_a_complete_previous_version() {
        let root = test_root();
        let paths = &root.paths;
        save_rollback_point(paths, &installed("old-binary", "7.2.147"));
        assert_eq!(
            fs::read(previous_binary_path(paths)).unwrap(),
            b"old-binary"
        );
        assert_eq!(
            fs::metadata(previous_binary_path(paths))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert!(previous_version_path(paths).is_file());

        // A version without a record cannot be rolled back to; the stale
        // rollback point, which no longer precedes it, is dropped.
        save_rollback_point(
            paths,
            &Installation {
                binary: Some(b"unknown".to_vec()),
                version: None,
            },
        );
        assert!(!previous_binary_path(paths).exists());
        assert!(!previous_version_path(paths).exists());
    }

    #[test]
    fn restoring_an_installation_without_a_version_removes_the_record() {
        let root = test_root();
        let paths = &root.paths;
        put(paths, &installed("new-binary", "7.2.150"));
        Installation {
            binary: Some(b"old-binary".to_vec()),
            version: None,
        }
        .write(paths)
        .unwrap();
        assert!(!version_path(paths).exists());
        assert_eq!(fs::read(binary_path(paths)).unwrap(), b"old-binary");
    }
}
