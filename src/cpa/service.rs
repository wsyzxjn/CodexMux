/// Read the CPA port from the managed config, defaulting to 8317.
pub fn config_port(paths: &Paths) -> u16 {
    let text = fs::read_to_string(config_path(paths)).unwrap_or_default();
    text.lines()
        .find_map(|line| line.trim().strip_prefix("port:"))
        .and_then(|port| port.trim().parse().ok())
        .unwrap_or(8317)
}

pub fn start(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let result = start_service(paths, cpa, token);
    // An explicit start (CLI or menu bar) becomes the user's startup
    // preference, but only when the service actually ended up running.
    if is_loaded().unwrap_or(false) {
        set_cpa_autostart(&paths.cpa_profiles, true).ok();
    }
    result
}

fn start_service(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let previous_config = fs::read(config_path(paths)).ok();
    write_config(paths, cpa, token)?;
    let config_changed = previous_config.as_deref() != fs::read(config_path(paths)).ok().as_deref();
    if is_loaded()? && !config_changed {
        ensure_management_connect_bootstrap(paths)?;
        return Ok(());
    }
    stop_service()?;
    bootstrap_service(paths)?;
    ensure_management_connect_bootstrap(paths)
}

fn bootstrap_service(paths: &Paths) -> Result<()> {
    let binary = binary_path(paths);
    anyhow::ensure!(
        binary.is_file(),
        "CLIProxyAPI binary is not installed at {}; run codexmux cpa install",
        binary.display()
    );
    let logs = paths.root.join("logs");
    fs::create_dir_all(&logs)?;
    let document = render_agent(
        &binary,
        &config_path(paths),
        &logs.join("cpa-stdout.log"),
        &logs.join("cpa-stderr.log"),
    )?;
    let plist = plist_path()?;
    atomic_write(&plist, document.as_bytes())?;
    bootstrap(&plist)
}

pub fn stop(paths: &Paths) -> Result<()> {
    let result = stop_service();
    // An explicit stop (CLI or menu bar) becomes the user's startup
    // preference; the menu bar reads it when CodexMux next starts.
    if !is_loaded().unwrap_or(false) {
        set_cpa_autostart(&paths.cpa_profiles, false).ok();
    }
    result
}

/// Stop the service without touching the startup preference (app shutdown).
pub fn stop_service_only() -> Result<()> {
    stop_service()
}

fn stop_service() -> Result<()> {
    stop_label_if_loaded(CPA_AGENT_LABEL)
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
        std::thread::sleep(Duration::from_millis(50));
    }
    bail!("CPA LaunchAgent is still loaded after launchctl bootout")
}

pub fn is_loaded() -> Result<bool> {
    is_label_loaded(CPA_AGENT_LABEL)
}

fn is_label_loaded(label: &str) -> Result<bool> {
    let status = Command::new("launchctl")
        .args(["print", &service_target_for(label)?])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to inspect the CPA LaunchAgent")?;
    Ok(status.success())
}

pub fn uninstall() -> Result<Option<PathBuf>> {
    let plist = plist_path()?;
    let installed = plist.exists() || is_loaded()?;
    if !installed {
        return Ok(None);
    }
    stop_service()?;
    if plist.exists() {
        fs::remove_file(&plist)?;
        return Ok(Some(plist));
    }
    Ok(None)
}

fn bootstrap(plist: &Path) -> Result<()> {
    bootstrap_label(plist, CPA_AGENT_LABEL)
}

fn bootstrap_label(plist: &Path, label: &str) -> Result<()> {
    // A previously disabled override (from an earlier bootout) makes
    // bootstrap fail with I/O error 5; clear it first.
    let _ = Command::new("launchctl")
        .args(["enable", &service_target_for(label)?])
        .status();
    let status = Command::new("launchctl")
        .args([
            "bootstrap",
            &launch_domain()?,
            plist.to_string_lossy().as_ref(),
        ])
        .status()
        .context("failed to run launchctl bootstrap")?;
    if !status.success() {
        bail!("launchctl bootstrap failed with {status}");
    }
    Ok(())
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

fn render_agent(binary: &Path, config: &Path, stdout: &Path, stderr: &Path) -> Result<String> {
    let working_dir = binary
        .parent()
        .context("CLIProxyAPI binary path has no parent directory")?;
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{CPA_AGENT_LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>-config</string><string>{}</string></array>
  <key>WorkingDirectory</key><string>{}</string>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Background</string>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
        xml(binary),
        xml(config),
        xml(working_dir),
        xml(stdout),
        xml(stderr),
    ))
}

fn xml(path: &Path) -> String {
    path.to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
