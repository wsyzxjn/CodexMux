use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::{config::Paths, fsutil::atomic_write};

const LABEL: &str = "dev.modelmux.proxy";

pub fn install(paths: &Paths, executable: &Path, codex_config: &Path) -> Result<PathBuf> {
    paths.ensure()?;
    let plist = plist_path()?;
    let logs = paths.root.join("logs");
    fs::create_dir_all(&logs)?;
    let document = render(
        executable,
        &paths.root,
        codex_config,
        &logs.join("stdout.log"),
        &logs.join("stderr.log"),
    );
    let previous = match fs::read(&plist) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("failed to read the existing LaunchAgent"),
    };
    let was_loaded = is_loaded()?;
    anyhow::ensure!(
        !(was_loaded && previous.is_none()),
        "LaunchAgent is loaded but its plist is missing; run modelmux uninstall first"
    );
    stop_if_loaded()?;
    if let Err(error) = atomic_write(&plist, document.as_bytes()) {
        if was_loaded {
            bootstrap(&plist).context("failed to restart the previous LaunchAgent")?;
        }
        return Err(error);
    }
    if let Err(error) = bootstrap(&plist) {
        rollback(&plist, previous.as_deref(), was_loaded, &error)?;
        return Err(error);
    }
    Ok(plist)
}

fn rollback(
    plist: &Path,
    previous: Option<&[u8]>,
    was_loaded: bool,
    install_error: &anyhow::Error,
) -> Result<()> {
    match previous {
        Some(previous) => atomic_write(plist, previous)?,
        None if plist.exists() => fs::remove_file(plist)?,
        None => {}
    }
    if was_loaded && let Err(rollback) = bootstrap(plist) {
        bail!(
            "failed to start new LaunchAgent ({install_error:#}); failed to restore previous LaunchAgent ({rollback:#})"
        );
    }
    Ok(())
}

pub fn uninstall() -> Result<Option<PathBuf>> {
    let plist = plist_path()?;
    let installed = plist.exists() || is_loaded()?;
    if !installed {
        return Ok(None);
    }
    stop_if_loaded()?;
    if plist.exists() {
        fs::remove_file(&plist)?;
    }
    Ok(Some(plist))
}

pub fn plist_path() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("cannot locate home directory")?
        .join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

fn bootstrap(plist: &Path) -> Result<()> {
    let domain = launch_domain()?;
    // A previously disabled override (from an earlier bootout) makes
    // bootstrap fail with I/O error 5; clear it first.
    let _ = Command::new("launchctl")
        .args(["enable", &service_target()?])
        .status();
    let status = Command::new("launchctl")
        .args(["bootstrap", &domain, plist.to_string_lossy().as_ref()])
        .status()
        .context("failed to run launchctl bootstrap")?;
    if !status.success() {
        bail!("launchctl bootstrap failed with {status}");
    }
    Ok(())
}

fn stop_if_loaded() -> Result<()> {
    if !is_loaded()? {
        return Ok(());
    }
    let target = service_target()?;
    let status = Command::new("launchctl")
        .args(["bootout", &target])
        .status()
        .context("failed to run launchctl bootout")?;
    if !status.success() && is_loaded()? {
        bail!("launchctl bootout failed with {status}");
    }
    for _ in 0..100 {
        if !is_loaded()? {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    bail!("LaunchAgent is still loaded after launchctl bootout")
}

fn is_loaded() -> Result<bool> {
    let target = service_target()?;
    let status = Command::new("launchctl")
        .args(["print", &target])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to inspect LaunchAgent")?;
    Ok(status.success())
}

fn service_target() -> Result<String> {
    Ok(format!("{}/{LABEL}", launch_domain()?))
}

fn launch_domain() -> Result<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to resolve user id")?;
    anyhow::ensure!(output.status.success(), "id -u failed");
    Ok(format!("gui/{}", String::from_utf8(output.stdout)?.trim()))
}

fn render(
    executable: &Path,
    root: &Path,
    codex_config: &Path,
    stdout: &Path,
    stderr: &Path,
) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>serve</string><string>--no-codex-config</string></array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>MODELMUX_HOME</key><string>{}</string>
    <key>CODEX_CONFIG</key><string>{}</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Background</string>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
        xml(executable),
        xml(root),
        xml(codex_config),
        xml(stdout),
        xml(stderr),
    )
}

fn xml(path: &Path) -> String {
    path.to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_escapes_paths() {
        let plist = render(
            Path::new("/tmp/a&b/modelmux"),
            Path::new("/tmp/root"),
            Path::new("/tmp/codex&config.toml"),
            Path::new("/tmp/out"),
            Path::new("/tmp/err"),
        );
        assert!(plist.contains("/tmp/a&amp;b/modelmux"));
        assert!(plist.contains("/tmp/codex&amp;config.toml"));
        assert!(plist.contains("<string>serve</string>"));
        assert!(plist.contains("<string>--no-codex-config</string>"));
    }
}
