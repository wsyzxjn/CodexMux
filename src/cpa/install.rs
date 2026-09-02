fn asset_url(version: &str) -> String {
    format!(
        "https://github.com/{CPA_REPO}/releases/download/v{version}/CLIProxyAPI_{version}_darwin_aarch64.tar.gz"
    )
}

/// Download the pinned CPA release, verify its digest, and extract the binary.
///
/// The archive is fully streamed to disk first so the digest can be checked
/// before anything is executed; extraction only accepts the expected binary
/// entry and rejects anything else.
pub fn install(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let archive = archive_path(paths);
    let parent = archive
        .parent()
        .context("archive path has no parent directory")?;
    fs::create_dir_all(parent).context("failed to create the CPA install directory")?;
    let mut response = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()?
        .get(asset_url(CPA_VERSION))
        .send()
        .context("failed to download the CLIProxyAPI release archive")?;
    anyhow::ensure!(
        response.status().is_success(),
        "downloading the CLIProxyAPI release archive failed with HTTP {}",
        response.status()
    );
    let mut file = fs::File::create(&archive)
        .with_context(|| format!("failed to create {}", archive.display()))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .context("failed to stream the CLIProxyAPI release archive")?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
    }
    file.sync_all()?;
    finish_download_install(
        &archive,
        install_from_archive(
        paths,
        cpa,
        &archive,
        CPA_VERSION,
        CPA_DARWIN_AARCH64_SHA256,
            token,
        ),
    )
}

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
            })?,
        )
    })();
    if let Err(error) = install_result {
        if was_loaded {
            start(paths, cpa, token).ok();
        }
        return Err(error);
    }
    start(paths, cpa, token)
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
