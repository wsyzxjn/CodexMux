use std::{
    fs,
    net::{SocketAddr, TcpListener},
    os::fd::{FromRawFd, OwnedFd},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};

use crate::{config::Paths, fsutil::atomic_write};

const LABEL: &str = "dev.codexmux.proxy";
const BOOTOUT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const BOOTOUT_POLLS: usize = 100;

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
    let was_loaded = is_loaded(LABEL)?;
    anyhow::ensure!(
        !(was_loaded && previous.is_none()),
        "LaunchAgent is loaded but its plist is missing; run codexmux uninstall first"
    );
    bootout(LABEL)?;
    if let Err(error) = atomic_write(&plist, document.as_bytes()) {
        if was_loaded {
            bootstrap(&plist, LABEL).context("failed to restart the previous LaunchAgent")?;
        }
        return Err(error);
    }
    if let Err(error) = bootstrap(&plist, LABEL) {
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
    if was_loaded && let Err(rollback) = bootstrap(plist, LABEL) {
        bail!(
            "failed to start new LaunchAgent ({install_error:#}); failed to restore previous LaunchAgent ({rollback:#})"
        );
    }
    Ok(())
}

pub fn uninstall() -> Result<Option<PathBuf>> {
    let plist = plist_path()?;
    let installed = plist.exists() || is_loaded(LABEL)?;
    if !installed {
        return Ok(None);
    }
    bootout(LABEL)?;
    if plist.exists() {
        fs::remove_file(&plist)?;
        return Ok(Some(plist));
    }
    Ok(None)
}

fn plist_path() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("cannot locate home directory")?
        .join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeState {
    NotInstalled,
    Idle,
    Running,
}

/// Report the proxy job's launchd state with a single `launchctl print`.
pub fn runtime_state() -> Result<RuntimeState> {
    Ok(match print(LABEL)? {
        None => RuntimeState::NotInstalled,
        Some(text) if has_running_pid(&text) => RuntimeState::Running,
        Some(_) => RuntimeState::Idle,
    })
}

/// Restart a running proxy so it reloads settings and credentials. An idle
/// job needs nothing: its next socket activation reads the files anew.
pub fn restart_if_running() -> Result<()> {
    if runtime_state()? != RuntimeState::Running {
        return Ok(());
    }
    let status = Command::new("launchctl")
        .args(["kickstart", "-k", &service_target(LABEL)])
        .status()
        .context("failed to run launchctl kickstart")?;
    anyhow::ensure!(
        status.success(),
        "launchctl kickstart {LABEL} failed with {status}"
    );
    Ok(())
}

fn has_running_pid(print_output: &str) -> bool {
    print_output
        .lines()
        .any(|line| line.trim_start().starts_with("pid = "))
}

unsafe extern "C" {
    fn launch_activate_socket(
        name: *const libc::c_char,
        fds: *mut *mut libc::c_int,
        count: *mut libc::size_t,
    ) -> libc::c_int;
}

pub fn activated_listener() -> Result<TcpListener> {
    let mut fds: *mut libc::c_int = std::ptr::null_mut();
    let mut count: libc::size_t = 0;
    // SAFETY: the name is NUL-terminated and both out-pointers are valid.
    let result = unsafe { launch_activate_socket(c"Listener".as_ptr(), &mut fds, &mut count) };
    if result != 0 {
        return Err(std::io::Error::from_raw_os_error(result))
            .context("failed to activate launchd listener socket");
    }
    // Own every descriptor launchd handed over before inspecting the count,
    // so an unexpected number of sockets is closed instead of leaked.
    let owned = if fds.is_null() {
        Vec::new()
    } else {
        // SAFETY: launchd returned `count` descriptors in a malloc'd array
        // that this process now owns.
        unsafe { std::slice::from_raw_parts(fds, count) }
            .iter()
            .map(|&fd| unsafe { OwnedFd::from_raw_fd(fd) })
            .collect()
    };
    // SAFETY: the array came from malloc; free(NULL) is a no-op.
    unsafe { libc::free(fds.cast()) };
    single_listener(owned)
}

fn single_listener(fds: Vec<OwnedFd>) -> Result<TcpListener> {
    let count = fds.len();
    let [fd]: [OwnedFd; 1] = fds
        .try_into()
        .map_err(|_| anyhow!("launchd supplied {count} listener sockets"))?;
    let listener = TcpListener::from(fd);
    listener.set_nonblocking(true)?;
    Ok(listener)
}

// launchctl helpers shared by the proxy agent and the CPA service.

/// The current user's GUI launchd domain.
fn launch_domain() -> String {
    // SAFETY: getuid has no preconditions and cannot fail.
    format!("gui/{}", unsafe { libc::getuid() })
}

fn service_target(label: &str) -> String {
    format!("{}/{label}", launch_domain())
}

/// `launchctl print` output for a loaded job, or `None` when it is not loaded.
fn print(label: &str) -> Result<Option<String>> {
    let output = Command::new("launchctl")
        .args(["print", &service_target(label)])
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("failed to inspect LaunchAgent {label}"))?;
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned()))
}

pub(crate) fn is_loaded(label: &str) -> Result<bool> {
    let status = Command::new("launchctl")
        .args(["print", &service_target(label)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("failed to inspect LaunchAgent {label}"))?;
    Ok(status.success())
}

pub(crate) fn bootstrap(plist: &Path, label: &str) -> Result<()> {
    // A disabled override left by an earlier bootout makes bootstrap fail
    // with I/O error 5; clear it first.
    let _ = Command::new("launchctl")
        .args(["enable", &service_target(label)])
        .status();
    let status = Command::new("launchctl")
        .args(["bootstrap", &launch_domain()])
        .arg(plist)
        .status()
        .context("failed to run launchctl bootstrap")?;
    anyhow::ensure!(
        status.success(),
        "launchctl bootstrap {label} failed with {status}"
    );
    Ok(())
}

/// Unload a job if it is loaded and wait until launchd reports it gone.
pub(crate) fn bootout(label: &str) -> Result<()> {
    if !is_loaded(label)? {
        return Ok(());
    }
    let status = Command::new("launchctl")
        .args(["bootout", &service_target(label)])
        .status()
        .context("failed to run launchctl bootout")?;
    if !status.success() && is_loaded(label)? {
        bail!("launchctl bootout {label} failed with {status}");
    }
    for _ in 0..BOOTOUT_POLLS {
        if !is_loaded(label)? {
            return Ok(());
        }
        std::thread::sleep(BOOTOUT_POLL_INTERVAL);
    }
    bail!("LaunchAgent {label} is still loaded after launchctl bootout")
}

pub(crate) fn xml_escape(path: &Path) -> String {
    path.to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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
        xml_escape(executable),
        xml_escape(root),
        xml_escape(codex_config),
        if listen.is_ipv4() { "IPv4" } else { "IPv6" },
        listen.ip(),
        listen.port(),
        xml_escape(stdout),
        xml_escape(stderr),
    )
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;

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

    #[test]
    fn launch_domain_is_the_current_gui_user() {
        let uid = unsafe { libc::getuid() };
        assert_eq!(launch_domain(), format!("gui/{uid}"));
        assert_eq!(
            service_target("dev.example"),
            format!("gui/{uid}/dev.example")
        );
    }

    #[test]
    fn runtime_state_distinguishes_running_from_idle_jobs() {
        assert!(has_running_pid(
            "gui/501/dev.codexmux.proxy = {\n\tpid = 4242\n}"
        ));
        assert!(!has_running_pid(
            "gui/501/dev.codexmux.proxy = {\n\tstate = not running\n}"
        ));
    }

    /// An unexpected descriptor count must close every descriptor launchd
    /// handed over. Each one is either closed or reused by an unrelated file
    /// afterwards, so compare identities instead of assuming the number is free.
    #[test]
    fn unexpected_listener_count_closes_every_descriptor() {
        let root = tempfile::tempdir().unwrap();
        let mut identities = Vec::new();
        let mut descriptors = Vec::new();
        for name in ["a", "b"] {
            let file = fs::File::create(root.path().join(name)).unwrap();
            let metadata = file.metadata().unwrap();
            identities.push((
                std::os::fd::AsRawFd::as_raw_fd(&file),
                metadata.dev(),
                metadata.ino(),
            ));
            descriptors.push(OwnedFd::from(file));
        }

        let error = single_listener(descriptors).unwrap_err();
        assert!(error.to_string().contains("supplied 2 listener sockets"));
        for (fd, dev, ino) in identities {
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            let result = unsafe { libc::fstat(fd, stat.as_mut_ptr()) };
            if result == 0 {
                let stat = unsafe { stat.assume_init() };
                assert!(
                    (stat.st_dev as u64, stat.st_ino) != (dev, ino),
                    "descriptor {fd} leaked"
                );
            }
        }
        assert!(single_listener(Vec::new()).is_err());
    }
}
