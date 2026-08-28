use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::{config::Paths, fsutil::atomic_write};

const LABEL: &str = "dev.modelmux.proxy";

pub fn install(paths: &Paths, executable: &Path) -> Result<PathBuf> {
    paths.ensure()?;
    let plist = plist_path()?;
    let logs = paths.root.join("logs");
    fs::create_dir_all(&logs)?;
    let document = render(
        executable,
        &paths.root,
        &logs.join("stdout.log"),
        &logs.join("stderr.log"),
    );
    atomic_write(&plist, document.as_bytes())?;
    let domain = launch_domain()?;
    let _ = Command::new("launchctl")
        .args(["bootout", &domain, plist.to_string_lossy().as_ref()])
        .status();
    let status = Command::new("launchctl")
        .args(["bootstrap", &domain, plist.to_string_lossy().as_ref()])
        .status()
        .context("failed to run launchctl bootstrap")?;
    if !status.success() {
        bail!("launchctl bootstrap failed with {status}");
    }
    Ok(plist)
}

pub fn uninstall() -> Result<Option<PathBuf>> {
    let plist = plist_path()?;
    if !plist.exists() {
        return Ok(None);
    }
    let domain = launch_domain()?;
    let status = Command::new("launchctl")
        .args(["bootout", &domain, plist.to_string_lossy().as_ref()])
        .status()
        .context("failed to run launchctl bootout")?;
    if !status.success() {
        tracing::warn!(%status, "launchctl bootout did not report success");
    }
    fs::remove_file(&plist)?;
    Ok(Some(plist))
}

pub fn plist_path() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("cannot locate home directory")?
        .join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

fn launch_domain() -> Result<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to resolve user id")?;
    anyhow::ensure!(output.status.success(), "id -u failed");
    Ok(format!("gui/{}", String::from_utf8(output.stdout)?.trim()))
}

fn render(executable: &Path, root: &Path, stdout: &Path, stderr: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>serve</string></array>
  <key>EnvironmentVariables</key>
  <dict><key>MODELMUX_HOME</key><string>{}</string></dict>
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
            Path::new("/tmp/out"),
            Path::new("/tmp/err"),
        );
        assert!(plist.contains("/tmp/a&amp;b/modelmux"));
        assert!(plist.contains("<string>serve</string>"));
    }
}
