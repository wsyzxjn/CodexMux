use std::{
    ffi::CString,
    fs,
    net::{SocketAddr, TcpListener},
    os::fd::{FromRawFd, RawFd},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::{config::Paths, fsutil::atomic_write};

const LABEL: &str = "dev.codexmux.proxy";

pub fn install(
    paths: &Paths,
    executable: &Path,
    codex_config: &Path,
    listen: SocketAddr,
) -> Result<PathBuf> {
    paths.ensure()?;
    let plist = plist_path()?;
    let logs = paths.root.join("logs");
    fs::create_dir_all(&logs)?;
    let document = render(
        executable,
        listen,
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
        "LaunchAgent is loaded but its plist is missing; run codexmux uninstall first"
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
        return Ok(Some(plist));
    }
    Ok(None)
}

pub fn plist_path() -> Result<PathBuf> {
    plist_path_for(LABEL)
}

fn plist_path_for(label: &str) -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("cannot locate home directory")?
        .join(format!("Library/LaunchAgents/{label}.plist")))
}

fn bootstrap(plist: &Path) -> Result<()> {
    bootstrap_label(plist, LABEL)
}

fn bootstrap_label(plist: &Path, label: &str) -> Result<()> {
    let domain = launch_domain()?;
    // A previously disabled override (from an earlier bootout) makes
    // bootstrap fail with I/O error 5; clear it first.
    let _ = Command::new("launchctl")
        .args(["enable", &service_target_for(label)?])
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
    stop_label_if_loaded(LABEL)
}

fn stop_label_if_loaded(label: &str) -> Result<()> {
    if !is_label_loaded(label)? {
        return Ok(());
    }
    let target = service_target_for(label)?;
    let status = Command::new("launchctl")
        .args(["bootout", &target])
        .status()
        .context("failed to run launchctl bootout")?;
    if !status.success() && is_label_loaded(label)? {
        bail!("launchctl bootout failed with {status}");
    }
    for _ in 0..100 {
        if !is_label_loaded(label)? {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    bail!("LaunchAgent is still loaded after launchctl bootout")
}

fn is_loaded() -> Result<bool> {
    is_label_loaded(LABEL)
}

fn is_label_loaded(label: &str) -> Result<bool> {
    let target = service_target_for(label)?;
    let status = Command::new("launchctl")
        .args(["print", &target])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to inspect LaunchAgent")?;
    Ok(status.success())
}

fn service_target_for(label: &str) -> Result<String> {
    Ok(format!("{}/{label}", launch_domain()?))
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
    listen: SocketAddr,
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
  <array><string>{}</string><string>serve</string><string>--no-codex-config</string><string>--launchd-socket</string></array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>CODEXMUX_HOME</key><string>{}</string>
    <key>CODEX_CONFIG</key><string>{}</string>
  </dict>
  <key>Sockets</key>
  <dict>
    <key>Listener</key>
    <dict>
      <key>SockFamily</key><string>{}</string>
      <key>SockNodeName</key><string>{}</string>
      <key>SockServiceName</key><string>{}</string>
      <key>SockType</key><string>stream</string>
      <key>SockPassive</key><true/>
    </dict>
  </dict>
  <key>ProcessType</key><string>Background</string>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
        xml(executable),
        xml(root),
        xml(codex_config),
        if listen.is_ipv4() { "IPv4" } else { "IPv6" },
        listen.ip(),
        listen.port(),
        xml(stdout),
        xml(stderr),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeState {
    NotInstalled,
    Idle,
    Running,
}

pub fn runtime_state() -> Result<RuntimeState> {
    if !plist_path()?.exists() && !is_loaded()? {
        return Ok(RuntimeState::NotInstalled);
    }
    let output = Command::new("launchctl")
        .args(["print", &service_target_for(LABEL)?])
        .output()
        .context("failed to inspect CodexMux LaunchAgent")?;
    if !output.status.success() {
        return Ok(RuntimeState::NotInstalled);
    }
    let text = String::from_utf8(output.stdout)?;
    if text
        .lines()
        .any(|line| line.trim_start().starts_with("pid = "))
    {
        Ok(RuntimeState::Running)
    } else {
        Ok(RuntimeState::Idle)
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn launch_activate_socket(
        name: *const libc::c_char,
        fds: *mut *mut libc::c_int,
        count: *mut libc::size_t,
    ) -> libc::c_int;
}

pub fn activated_listener() -> Result<TcpListener> {
    #[cfg(not(target_os = "macos"))]
    bail!("launchd socket activation is only supported on macOS");

    #[cfg(target_os = "macos")]
    {
        let name = CString::new("Listener")?;
        let mut fds: *mut libc::c_int = std::ptr::null_mut();
        let mut count: libc::size_t = 0;
        let result = unsafe { launch_activate_socket(name.as_ptr(), &mut fds, &mut count) };
        if result != 0 {
            return Err(std::io::Error::from_raw_os_error(result))
                .context("failed to activate launchd listener socket");
        }
        anyhow::ensure!(count == 1, "launchd supplied {count} listener sockets");
        let fd: RawFd = unsafe { *fds };
        unsafe { libc::free(fds.cast()) };
        let listener = unsafe { TcpListener::from_raw_fd(fd) };
        listener.set_nonblocking(true)?;
        Ok(listener)
    }
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
            Path::new("/tmp/a&b/codexmux"),
            "127.0.0.1:48682".parse().unwrap(),
            Path::new("/tmp/root"),
            Path::new("/tmp/codex&config.toml"),
            Path::new("/tmp/out"),
            Path::new("/tmp/err"),
        );
        assert!(plist.contains("/tmp/a&amp;b/codexmux"));
        assert!(plist.contains("/tmp/codex&amp;config.toml"));
        assert!(plist.contains("<string>serve</string>"));
        assert!(plist.contains("<string>--no-codex-config</string>"));
        assert!(plist.contains("<string>--launchd-socket</string>"));
        assert!(plist.contains("<key>Sockets</key>"));
        assert!(plist.contains("<string>48682</string>"));
        assert!(!plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("dev.codexmux.proxy"));
        assert!(plist.contains("<key>CODEXMUX_HOME</key>"));
    }
}
