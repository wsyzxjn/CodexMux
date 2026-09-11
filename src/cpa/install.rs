/// Resolve the latest stable CPA release, verify its digest, and install the binary.
///
/// The archive is downloaded and checked against the published digest before
/// anything is executed; extraction only accepts the expected binary entry.
pub fn install(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let release = resolve_release(None)?;
    let archive = download_verified_asset(paths, &release)?;
    let result = install_verified_archive(paths, cpa, token, &archive, &release);
    let _ = fs::remove_file(&archive);
    result?;
    if !is_loaded()? {
        start(paths, cpa, token)?;
    }
    wait_for_model_slugs(cpa, token)?;
    Ok(())
}

#[cfg(test)]
fn finish_download_install(archive: &Path, result: Result<()>) -> Result<()> {
    result?;
    // The downloaded archive is staging data; offline archives supplied by
    // the caller go through install_from_archive directly and are untouched.
    fs::remove_file(archive)
        .with_context(|| format!("failed to remove CPA archive {}", archive.display()))
}

/// Install from an already downloaded archive, verifying `sha256` first.
/// Used by `codexmux cpa install --archive <path>` for offline installs.
pub fn install_from_archive(
    paths: &Paths,
    cpa: &Cpa,
    archive: &Path,
    version: &str,
    sha256: &str,
    token: &str,
) -> Result<()> {
    let actual = sha256_file(archive)?;
    anyhow::ensure!(
        actual == sha256,
        "CLIProxyAPI archive digest mismatch: expected {sha256}, got {actual}"
    );
    let binary = extract_binary(archive)?;
    let binary_path = binary_path(paths);
    let was_loaded = is_loaded()?;
    if was_loaded {
        stop_service()?;
    }
    let install_result = (|| -> Result<()> {
        atomic_write(&binary_path, &binary)?;
        set_executable(&binary_path)?;
        atomic_write(
            &version_path(paths),
            &serde_json::to_vec_pretty(&InstalledVersion {
                version: version.to_owned(),
                sha256: sha256.to_owned(),
                ..Default::default()
            })?,
        )
    })();
    if let Err(error) = install_result {
        if was_loaded {
            start(paths, cpa, token).ok();
        }
        return Err(error);
    }
    start(paths, cpa, token).and_then(|_| wait_for_model_slugs(cpa, token).map(|_| ()))
}

fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("failed to make {} executable", path.display()))
}

fn extract_binary(archive: &Path) -> Result<Vec<u8>> {
    let file =
        fs::File::open(archive).with_context(|| format!("failed to open {}", archive.display()))?;
    let mut archive = Archive::new(GzDecoder::new(file));
    for entry in archive
        .entries()
        .context("failed to read the CLIProxyAPI release archive")?
    {
        let mut entry = entry.context("failed to read the CLIProxyAPI release archive")?;
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
            .read_to_end(&mut binary)
            .context("failed to extract the CLIProxyAPI binary")?;
        anyhow::ensure!(
            !binary.is_empty(),
            "CLIProxyAPI archive contains an empty binary"
        );
        return Ok(binary);
    }
    bail!("CLIProxyAPI archive does not contain {CPA_BINARY_NAME}")
}
