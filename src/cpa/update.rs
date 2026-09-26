use std::{cmp::Ordering, fs, io::Read, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use super::{
    CPA_REPO, InstalledVersion, binary_path,
    install::{
        Installation, Validation, extract_binary, previous_binary_path, previous_version_path,
        remove_optional, replace_installation, save_rollback_point,
    },
    installed_version, lock,
    service::{Launchd, ServiceControl},
    supported_platform,
};
use crate::{
    config::{Cpa, Paths},
    fsutil::sha256_bytes,
};

const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/router-for-me/CLIProxyAPI/releases";
const CHECKSUMS_ASSET: &str = "checksums.txt";
/// Upper bound for a downloaded release archive held in memory.
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;

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
pub(super) struct ResolvedRelease {
    pub(super) version: String,
    pub(super) published_at: Option<String>,
    pub(super) asset: ReleaseAsset,
    checksums_url: String,
}

#[derive(Clone, Debug)]
pub(super) struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    pub(super) sha256: String,
}

pub fn check_cpa_update(paths: &Paths, version: Option<&str>) -> Result<UpdateCheck> {
    let release = resolve_release(version)?;
    let current = installed_version(paths)?.map(|installed| installed.version);
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

/// Update the local CPA to the latest (or the given) stable release.
///
/// The installed version is read under the CPA lock, so two updates cannot
/// both decide to install. The release is downloaded, verified, and unpacked
/// in memory before the running service is touched; a running CPA is
/// restarted and must answer `/models`, or the previous version comes back.
pub fn update_cpa(
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
    version: Option<&str>,
    dry_run: bool,
) -> Result<UpdateOutcome> {
    let _lock = if dry_run { None } else { Some(lock(paths)?) };
    let current = installed_version(paths)?
        .context("CPA is not installed; run `codexmux cpa install` first")?;
    let release = resolve_release(version)?;
    let changed =
        ReleaseVersion::parse(&release.version)? > ReleaseVersion::parse(&current.version)?;
    let outcome = UpdateOutcome {
        from_version: current.version.clone(),
        to_version: release.version.clone(),
        dry_run,
        changed,
    };
    if dry_run {
        return Ok(outcome);
    }
    ensure!(
        changed,
        "CPA {} is already up to date; latest available is {}",
        current.version,
        release.version
    );
    ensure!(
        binary_path(paths).is_file(),
        "CPA binary is missing; run `codexmux cpa install` first"
    );
    let archive = download_verified_asset(&release)?;
    let binary = extract_binary(&archive)?;
    let target = Installation {
        binary: Some(binary),
        version: Some(serde_json::to_vec_pretty(&InstalledVersion {
            version: release.version.clone(),
            sha256: release.asset.sha256.clone(),
            source: Some(format!("github:{CPA_REPO}")),
            published_at: release.published_at.clone(),
            updated_at: Some(crate::fsutil::unix_time_nanos().to_string()),
        })?),
    };
    let replaced =
        replace_installation(&Launchd, paths, cpa, token, &target, Validation::IfRunning)
            .with_context(|| format!("CPA update to {} failed", release.version))?;
    save_rollback_point(paths, &replaced);
    Ok(outcome)
}

pub fn rollback_available(paths: &Paths) -> bool {
    previous_binary_path(paths).is_file() && previous_version_path(paths).is_file()
}

/// Restore the version the last install or update replaced. The rollback
/// point is consumed only once the restored version is in place (and, if CPA
/// was running, answers `/models`).
pub fn rollback_cpa(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let _lock = lock(paths)?;
    rollback_with(&Launchd, paths, cpa, token).context("CPA rollback failed")
}

fn rollback_with(
    services: &dyn ServiceControl,
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
) -> Result<()> {
    ensure!(
        rollback_available(paths),
        "no previous CPA version is available; perform an update first"
    );
    let read = |path: std::path::PathBuf| {
        fs::read(&path).with_context(|| format!("failed to read {}", path.display()))
    };
    let target = Installation {
        binary: Some(read(previous_binary_path(paths))?),
        version: Some(read(previous_version_path(paths))?),
    };
    replace_installation(services, paths, cpa, token, &target, Validation::IfRunning)?;
    // The rollback succeeded; a leftover rollback point only offers the
    // restored version again.
    if let Err(error) = remove_optional(&previous_version_path(paths))
        .and_then(|()| remove_optional(&previous_binary_path(paths)))
    {
        tracing::warn!(error = %format!("{error:#}"), "failed to remove the used CPA rollback point");
    }
    Ok(())
}

pub(super) fn resolve_release(version: Option<&str>) -> Result<ResolvedRelease> {
    let client = http_client()?;
    let (url, explicit) = match version {
        Some(version) => {
            ReleaseVersion::parse(version)?;
            let version = version.trim();
            let tag_version = version.strip_prefix('v').unwrap_or(version);
            (format!("{GITHUB_RELEASES_API}/tags/v{tag_version}"), true)
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
    let tag = release.tag_name.trim();
    let version = tag.strip_prefix('v').unwrap_or(tag).to_owned();
    ReleaseVersion::parse(&version)?;

    let asset_name = asset_name_for(supported_platform()?, &version);
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
        is_sha256_hex(digest),
        "GitHub asset {} has an invalid SHA-256 digest",
        asset.name
    );
    Ok(digest.to_ascii_lowercase())
}

fn is_sha256_hex(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn asset_name_for(platform: &str, version: &str) -> String {
    format!("CLIProxyAPI_{version}_{platform}.tar.gz")
}

/// Download the release archive into memory and require its SHA-256 to match
/// both the GitHub asset digest and the published `checksums.txt`.
pub(super) fn download_verified_asset(release: &ResolvedRelease) -> Result<Vec<u8>> {
    let client = http_client()?;
    let response = client
        .get(&release.asset.browser_download_url)
        .send()
        .with_context(|| format!("failed to download {}", release.asset.browser_download_url))?;
    ensure!(
        response.status().is_success(),
        "download {} failed with HTTP {}",
        release.asset.name,
        response.status()
    );
    let mut archive = Vec::new();
    response
        .take(MAX_ARCHIVE_BYTES + 1)
        .read_to_end(&mut archive)
        .with_context(|| format!("failed to download {}", release.asset.name))?;
    ensure!(
        archive.len() as u64 <= MAX_ARCHIVE_BYTES,
        "{} exceeds {} MiB",
        release.asset.name,
        MAX_ARCHIVE_BYTES / 1024 / 1024
    );
    let actual = sha256_bytes(&archive);
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
        published == actual,
        "checksums.txt disagrees with the release asset digest"
    );
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
            is_sha256_hex(digest),
            "checksums.txt contains an invalid digest for {asset_name}"
        );
        return Ok(digest.to_ascii_lowercase());
    }
    bail!("checksums.txt has no entry for {asset_name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpa::{
        test_support::{FakeServices, local_cpa, test_root},
        version_path,
    };

    #[test]
    fn release_versions_compare_numeric_segments() {
        assert!(
            ReleaseVersion::parse("7.2.148").unwrap() > ReleaseVersion::parse("7.2.147").unwrap()
        );
        assert!(ReleaseVersion::parse("7.10.0").unwrap() > ReleaseVersion::parse("7.9.9").unwrap());
        assert!(
            ReleaseVersion::parse("v7.2.147").unwrap() == ReleaseVersion::parse("7.2.147").unwrap()
        );
        assert!(ReleaseVersion::parse("1.2").unwrap() < ReleaseVersion::parse("1.2.0").unwrap());
        assert!(ReleaseVersion::parse("not-a-version").is_err());
    }

    #[test]
    fn update_asset_names_match_the_supported_platform() {
        assert_eq!(supported_platform().unwrap(), "darwin_aarch64");
        assert_eq!(
            asset_name_for("darwin_aarch64", "7.2.147"),
            "CLIProxyAPI_7.2.147_darwin_aarch64.tar.gz"
        );
    }

    #[test]
    fn published_checksums_require_an_exact_asset_digest() {
        let checksums = "4AC1DB83B00591265EBB93A3277D812AAF6E45E8B21BB3B4786598520AFDF4BE  CLIProxyAPI_7.2.147_darwin_aarch64.tar.gz";
        assert_eq!(
            checksum_for(checksums, "CLIProxyAPI_7.2.147_darwin_aarch64.tar.gz").unwrap(),
            "4ac1db83b00591265ebb93a3277d812aaf6e45e8b21bb3b4786598520afdf4be"
        );
        assert!(checksum_for(checksums, "missing.tar.gz").is_err());
        assert!(checksum_for("not-a-digest file.tar.gz", "file.tar.gz").is_err());
    }

    #[test]
    fn asset_digest_parser_accepts_only_sha256_hex() {
        let asset = GithubAsset {
            name: "CLIProxyAPI_7.2.147_darwin_aarch64.tar.gz".into(),
            browser_download_url: "https://example.com/asset".into(),
            digest: Some(
                "sha256:4ac1db83b00591265ebb93a3277d812aaf6e45e8b21bb3b4786598520afdf4be".into(),
            ),
        };
        assert_eq!(
            asset_digest(&asset).unwrap(),
            "4ac1db83b00591265ebb93a3277d812aaf6e45e8b21bb3b4786598520afdf4be"
        );
        let no_digest = GithubAsset {
            name: asset.name,
            browser_download_url: asset.browser_download_url,
            digest: None,
        };
        assert!(asset_digest(&no_digest).is_err());
    }

    fn installation(binary: &str, version: &str) -> Installation {
        Installation {
            binary: Some(binary.as_bytes().to_vec()),
            version: Some(version.as_bytes().to_vec()),
        }
    }

    fn put_rollback_point(paths: &Paths, previous: &Installation) {
        fs::create_dir_all(binary_path(paths).parent().unwrap()).unwrap();
        fs::write(
            previous_binary_path(paths),
            previous.binary.as_ref().unwrap(),
        )
        .unwrap();
        fs::write(
            previous_version_path(paths),
            previous.version.as_ref().unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn rollback_restores_the_previous_version_and_consumes_the_rollback_point() {
        let root = test_root();
        let paths = &root.paths;
        let current = installation("new-binary", r#"{"version":"7.2.150","sha256":""}"#);
        let previous = installation("old-binary", r#"{"version":"7.2.147","sha256":""}"#);
        fs::create_dir_all(binary_path(paths).parent().unwrap()).unwrap();
        fs::write(binary_path(paths), current.binary.as_ref().unwrap()).unwrap();
        fs::write(version_path(paths), current.version.as_ref().unwrap()).unwrap();
        put_rollback_point(paths, &previous);
        assert!(rollback_available(paths));

        let services = FakeServices::loaded();
        rollback_with(&services, paths, &local_cpa(8317), "cpa-token").unwrap();
        assert_eq!(Installation::read(paths).unwrap(), previous);
        assert!(!rollback_available(paths));
        assert_eq!(
            services.events(),
            [
                "stop",
                "bootstrap old-binary",
                "ready http://127.0.0.1:8317/v1"
            ]
        );
    }

    #[test]
    fn a_failed_rollback_keeps_the_current_version_and_the_rollback_point() {
        let root = test_root();
        let paths = &root.paths;
        // The current version has no record; a restore must not invent an
        // empty version.json for it.
        fs::create_dir_all(binary_path(paths).parent().unwrap()).unwrap();
        fs::write(binary_path(paths), "new-binary").unwrap();
        let previous = installation("old-binary", r#"{"version":"7.2.147","sha256":""}"#);
        put_rollback_point(paths, &previous);

        let services = FakeServices::loaded();
        services.fail_next_ready("old version never answered");
        let error = rollback_with(&services, paths, &local_cpa(8317), "cpa-token").unwrap_err();
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("restored and restarted the previously installed CPA version"),
            "{rendered}"
        );
        assert_eq!(fs::read(binary_path(paths)).unwrap(), b"new-binary");
        assert!(!version_path(paths).exists());
        assert!(rollback_available(paths));
    }

    #[test]
    fn rollback_requires_a_complete_rollback_point() {
        let root = test_root();
        let paths = &root.paths;
        fs::create_dir_all(binary_path(paths).parent().unwrap()).unwrap();
        fs::write(previous_binary_path(paths), "old-binary").unwrap();
        let services = FakeServices::loaded();
        let error = rollback_with(&services, paths, &local_cpa(8317), "cpa-token").unwrap_err();
        assert!(error.to_string().contains("no previous CPA version"));
        assert!(services.events().is_empty());
    }
}
