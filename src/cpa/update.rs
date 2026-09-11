use std::cmp::Ordering;

use fs2::FileExt;
use time::OffsetDateTime;

const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/router-for-me/CLIProxyAPI/releases";
const CHECKSUMS_ASSET: &str = "checksums.txt";

#[derive(Clone, Debug)]
pub struct UpdateCheck {
    pub current_version: Option<String>,
    pub latest_version: String,
    pub update_available: bool,
}

#[derive(Clone, Debug)]
pub struct UpdateOutcome {
    pub from_version: String,
    pub to_version: String,
    pub dry_run: bool,
    pub changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReleaseVersion(Vec<u64>);

impl ReleaseVersion {
    fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        let value = value.strip_prefix('v').unwrap_or(value);
        ensure!(!value.is_empty(), "release version must not be empty");
        let parts = value
            .split('.')
            .map(|part| {
                part.parse::<u64>()
                    .with_context(|| format!("invalid release version component {part:?}"))
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            !parts.is_empty() && parts.len() <= 4,
            "unsupported release version format {value:?}"
        );
        Ok(Self(parts))
    }
}

impl Ord for ReleaseVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        for (left, right) in self.0.iter().zip(&other.0) {
            match left.cmp(right) {
                Ordering::Equal => {}
                ordering => return ordering,
            }
        }
        self.0.len().cmp(&other.0.len())
    }
}

impl PartialOrd for ReleaseVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    published_at: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

#[derive(Clone, Debug)]
struct ResolvedRelease {
    version: String,
    published_at: Option<String>,
    asset: ReleaseAsset,
    checksums_url: String,
}

#[derive(Clone, Debug)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    sha256: String,
}

pub fn check_cpa_update(paths: &Paths, version: Option<&str>) -> Result<UpdateCheck> {
    let release = resolve_release(version)?;
    let current = installed_version(paths).map(|installed| installed.version);
    let update_available = current.as_ref().is_some_and(|current| {
        ReleaseVersion::parse(&release.version)
            .and_then(|target| ReleaseVersion::parse(current).map(|current| target > current))
            .unwrap_or(false)
    });
    Ok(UpdateCheck {
        current_version: current,
        latest_version: release.version,
        update_available,
    })
}

pub fn update_cpa(
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
    version: Option<&str>,
    dry_run: bool,
) -> Result<UpdateOutcome> {
    let current = installed_version(paths)
        .with_context(|| "CPA is not installed; run `codexmux cpa install` first")?;
    let release = resolve_release(version)?;
    let target = ReleaseVersion::parse(&release.version)?;
    let installed = ReleaseVersion::parse(&current.version)?;
    let changed = target > installed;
    if !changed && !dry_run {
        bail!(
            "CPA {} is already up to date; latest available is {}",
            current.version,
            release.version
        );
    }

    let outcome = UpdateOutcome {
        from_version: current.version.clone(),
        to_version: release.version.clone(),
        dry_run,
        changed,
    };
    if dry_run || !changed {
        return Ok(outcome);
    }

    ensure!(
        binary_path(paths).is_file(),
        "CPA binary is missing; run `codexmux cpa install` first"
    );
    let _update_guard = update_guard(paths)?;
    let archive = download_verified_asset(paths, &release)?;
    let result = install_verified_archive(paths, cpa, token, &archive, &release);
    let _ = std::fs::remove_file(&archive);
    result?;
    Ok(outcome)
}

pub fn rollback_available(paths: &Paths) -> bool {
    previous_binary_path(paths).is_file() && previous_version_path(paths).is_file()
}

pub fn rollback_cpa(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let _update_guard = update_guard(paths)?;
    ensure!(
        rollback_available(paths),
        "no previous CPA version is available; perform an update first"
    );
    let was_loaded = is_loaded()?;
    if was_loaded {
        stop_service()?;
    }

    let current_binary = std::fs::read(binary_path(paths))
        .with_context(|| format!("failed to read {}", binary_path(paths).display()))?;
    let current_version = std::fs::read(version_path(paths)).ok();
    let previous_binary = std::fs::read(previous_binary_path(paths))
        .with_context(|| format!("failed to read {}", previous_binary_path(paths).display()))?;
    let previous_version = std::fs::read(previous_version_path(paths))
        .with_context(|| format!("failed to read {}", previous_version_path(paths).display()))?;

    match write_rollback(paths, cpa, token, &previous_binary, &previous_version, was_loaded) {
        Ok(()) => {
            let _ = std::fs::remove_file(previous_binary_path(paths));
            let _ = std::fs::remove_file(previous_version_path(paths));
            Ok(())
        }
        Err(error) => {
            let restore = write_rollback(
                paths,
                cpa,
                token,
                &current_binary,
                current_version.as_deref().unwrap_or_default(),
                was_loaded,
            );
            if let Err(restore_error) = restore {
                return Err(error).context(format!(
                    "CPA rollback failed and restoring the current version also failed: {restore_error}"
                ));
            }
            Err(error).context("CPA rollback failed; kept the current version")
        }
    }
}

fn write_rollback(
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
    binary: &[u8],
    version_json: &[u8],
    was_loaded: bool,
) -> Result<()> {
    atomic_write(&binary_path(paths), binary)?;
    set_executable(&binary_path(paths))?;
    atomic_write(&version_path(paths), version_json)?;
    if was_loaded {
        start_service(paths, cpa, token)?;
        wait_for_model_slugs(cpa, token)?;
    }
    Ok(())
}

fn resolve_release(version: Option<&str>) -> Result<ResolvedRelease> {
    let client = http_client()?;
    let (url, explicit) = match version {
        Some(version) => {
            let tag_version = version
                .trim()
                .strip_prefix('v')
                .unwrap_or(version.trim())
                .to_owned();
            ReleaseVersion::parse(version)?;
            (
                format!("{GITHUB_RELEASES_API}/tags/v{tag_version}"),
                true,
            )
        }
        None => (format!("{GITHUB_RELEASES_API}/latest"), false),
    };
    let release = fetch_release(&client, &url)?;
    ensure!(!release.draft, "GitHub returned a draft release");
    if !explicit {
        ensure!(
            !release.prerelease,
            "GitHub returned a prerelease; no stable release found"
        );
    }
    let version = release
        .tag_name
        .trim()
        .strip_prefix('v')
        .unwrap_or(release.tag_name.trim())
        .to_owned();
    ReleaseVersion::parse(&version)?;

    let asset_name = platform_asset_name(&version)?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .with_context(|| format!("release v{version} has no {asset_name} asset"))?;
    let checksums = release
        .assets
        .iter()
        .find(|asset| asset.name == CHECKSUMS_ASSET)
        .with_context(|| format!("release v{version} has no {CHECKSUMS_ASSET}"))?;

    Ok(ResolvedRelease {
        version,
        published_at: release.published_at,
        asset: ReleaseAsset {
            name: asset.name.clone(),
            browser_download_url: asset.browser_download_url.clone(),
            sha256: asset_digest(asset)?,
        },
        checksums_url: checksums.browser_download_url.clone(),
    })
}

fn fetch_release(client: &reqwest::blocking::Client, url: &str) -> Result<GithubRelease> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to reach GitHub Releases at {url}"))?;
    ensure!(
        response.status().is_success(),
        "GitHub Releases request failed with HTTP {}",
        response.status()
    );
    response
        .json::<GithubRelease>()
        .context("GitHub returned invalid release JSON")
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("codexmux/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to build the update HTTP client")
}

fn asset_digest(asset: &GithubAsset) -> Result<String> {
    let digest = asset
        .digest
        .as_deref()
        .with_context(|| format!("GitHub asset {} has no digest", asset.name))?;
    let digest = digest.strip_prefix("sha256:").unwrap_or(digest);
    ensure!(
        digest.len() == 64 && digest.chars().all(|character| character.is_ascii_hexdigit()),
        "GitHub asset {} has an invalid SHA-256 digest",
        asset.name
    );
    Ok(digest.to_owned())
}

fn platform_asset_name(version: &str) -> Result<String> {
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "darwin_aarch64",
        ("macos", "x86_64") => "darwin_amd64",
        (os, arch) => bail!(
            "CPA updates are not supported on {os}/{arch}; only macOS arm64 or x86_64 are supported"
        ),
    };
    Ok(asset_name_for(platform, version))
}

fn asset_name_for(platform: &str, version: &str) -> String {
    format!("CLIProxyAPI_{version}_{platform}.tar.gz")
}

fn download_verified_asset(paths: &Paths, release: &ResolvedRelease) -> Result<PathBuf> {
    let client = http_client()?;
    let staging = paths.root.join("cpa/update");
    std::fs::create_dir_all(&staging)
        .with_context(|| format!("failed to create {}", staging.display()))?;
    let archive = staging.join(&release.asset.name);
    let _ = std::fs::remove_file(&archive);

    let result = (|| -> Result<()> {
        let mut response = client
            .get(&release.asset.browser_download_url)
            .send()
            .with_context(|| format!("failed to download {}", release.asset.browser_download_url))?;
        ensure!(
            response.status().is_success(),
            "download {} failed with HTTP {}",
            release.asset.name,
            response.status()
        );
        let mut file = std::fs::File::create(&archive)
            .with_context(|| format!("failed to create {}", archive.display()))?;
        std::io::copy(&mut response, &mut file)
            .with_context(|| format!("failed to stream {}", archive.display()))?;
        file.sync_all()?;

        let actual = sha256_file(&archive)?;
        ensure!(
            actual == release.asset.sha256,
            "archive digest mismatch: expected {}, got {}",
            release.asset.sha256,
            actual
        );
        let checksums_response = client
            .get(&release.checksums_url)
            .send()
            .with_context(|| format!("failed to download {CHECKSUMS_ASSET}"))?;
        ensure!(
            checksums_response.status().is_success(),
            "downloading {CHECKSUMS_ASSET} failed with HTTP {}",
            checksums_response.status()
        );
        let checksums = checksums_response
            .text()
            .context("failed to read published checksums")?;
        let published = checksum_for(&checksums, &release.asset.name)?;
        ensure!(
            published == release.asset.sha256 && published == actual,
            "checksums.txt disagrees with the release asset digest"
        );
        Ok(())
    })();

    if let Err(error) = result {
        let _ = std::fs::remove_file(&archive);
        return Err(error);
    }
    Ok(archive)
}

fn checksum_for(checksums: &str, asset_name: &str) -> Result<String> {
    for line in checksums.lines() {
        let mut parts = line.split_whitespace();
        let digest = parts.next().unwrap_or_default();
        let name = parts.next().unwrap_or_default();
        if name != asset_name {
            continue;
        }
        ensure!(
            digest.len() == 64 && digest.chars().all(|character| character.is_ascii_hexdigit()),
            "checksums.txt contains an invalid digest for {asset_name}"
        );
        return Ok(digest.to_owned());
    }
    bail!("checksums.txt has no entry for {asset_name}")
}

fn install_verified_archive(
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
    archive: &std::path::Path,
    release: &ResolvedRelease,
) -> Result<()> {
    let was_loaded = is_loaded()?;
    if was_loaded {
        stop_service()?;
    }

    let binary = extract_binary(archive)?;
    let had_binary = binary_path(paths).is_file();
    let had_version = version_path(paths).is_file();
    let _ = std::fs::remove_file(previous_binary_path(paths));
    let _ = std::fs::remove_file(previous_version_path(paths));
    let backup_result = (|| -> Result<()> {
        if had_binary {
            std::fs::rename(binary_path(paths), previous_binary_path(paths))
                .with_context(|| format!("failed to back up {}", binary_path(paths).display()))?;
        }
        if had_version {
            std::fs::rename(version_path(paths), previous_version_path(paths))
                .with_context(|| format!("failed to back up {}", version_path(paths).display()))?;
        }
        Ok(())
    })();
    if let Err(error) = backup_result {
        restore_previous(paths)?;
        if was_loaded {
            let _ = start_service(paths, cpa, token);
        }
        return Err(error).context("failed to back up the installed CPA; no changes were made");
    }

    if let Err(error) = write_install_files(paths, &binary, release) {
        restore_previous(paths)?;
        if !had_binary {
            let _ = std::fs::remove_file(binary_path(paths));
        }
        if !had_version {
            let _ = std::fs::remove_file(version_path(paths));
        }
        if was_loaded {
            let _ = start_service(paths, cpa, token);
        }
        return Err(error).context("failed to install the updated CPA binary; previous version restored");
    }

    if was_loaded {
        let validation = start_service(paths, cpa, token).and_then(|_| {
            wait_for_model_slugs(cpa, token).map(|_| ())
        });
        if let Err(error) = validation {
            restore_previous(paths)?;
            if !had_binary {
                let _ = std::fs::remove_file(binary_path(paths));
            }
            if !had_version {
                let _ = std::fs::remove_file(version_path(paths));
            }
            let _ = start_service(paths, cpa, token);
            return Err(error).context("updated CPA failed validation; previous version restored");
        }
    }
    Ok(())
}

fn write_install_files(
    paths: &Paths,
    binary: &[u8],
    release: &ResolvedRelease,
) -> Result<()> {
    atomic_write(&binary_path(paths), binary)?;
    set_executable(&binary_path(paths))?;
    atomic_write(
        &version_path(paths),
        &serde_json::to_vec_pretty(&InstalledVersion {
            version: release.version.clone(),
            sha256: release.asset.sha256.clone(),
            source: Some(format!("github:{CPA_REPO}")),
            published_at: release.published_at.clone(),
            updated_at: Some(OffsetDateTime::now_utc().unix_timestamp_nanos().to_string()),
        })?,
    )?;
    Ok(())
}

fn restore_previous(paths: &Paths) -> Result<()> {
    let previous_binary = previous_binary_path(paths);
    if previous_binary.is_file() {
        let _ = std::fs::remove_file(binary_path(paths));
        std::fs::rename(&previous_binary, binary_path(paths))
            .with_context(|| format!("failed to restore {}", binary_path(paths).display()))?;
    }
    let previous_version = previous_version_path(paths);
    if previous_version.is_file() {
        let _ = std::fs::remove_file(version_path(paths));
        std::fs::rename(&previous_version, version_path(paths))
            .with_context(|| format!("failed to restore {}", version_path(paths).display()))?;
    }
    Ok(())
}

fn previous_binary_path(paths: &Paths) -> PathBuf {
    binary_path(paths).with_extension("previous")
}

fn previous_version_path(paths: &Paths) -> PathBuf {
    version_path(paths).with_file_name("version.json.previous")
}

fn update_guard(paths: &Paths) -> Result<std::fs::File> {
    let directory = paths.root.join("cpa");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("failed to create {}", directory.display()))?;
    let file = std::fs::File::create(directory.join("update.lock"))
        .with_context(|| format!("failed to create {}", directory.join("update.lock").display()))?;
    file.lock_exclusive()
        .with_context(|| format!("failed to lock {}", directory.join("update.lock").display()))?;
    Ok(file)
}
