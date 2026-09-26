use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use super::{
    CPA_AGENT_LABEL, CPA_MODEL_VALIDATION_POLL_INTERVAL, CPA_MODEL_VALIDATION_TIMEOUT, binary_path,
    config_path, lock,
    managed_config::{ensure_management_connect_bootstrap, write_config},
    model_slugs, plist_path,
    profiles::set_cpa_autostart,
    wait_for_model_slugs_with,
};
use crate::{
    config::{Cpa, Paths},
    fsutil::atomic_write,
    launch_agent,
};

/// Process-level side effects of managing the local CPA: its launchd job,
/// readiness probes, and the proxy restart that reloads settings. Production
/// code uses [`Launchd`]; tests substitute a recorder so no test touches the
/// user's launchd domain or the network.
pub(crate) trait ServiceControl {
    fn cpa_loaded(&self) -> Result<bool>;
    /// Unload the CPA job if it is loaded.
    fn stop_cpa(&self) -> Result<()>;
    fn bootstrap_cpa(&self, plist: &Path) -> Result<()>;
    /// One authenticated `/models` request to a CPA endpoint.
    fn cpa_models(&self, cpa: &Cpa, token: &str) -> Result<Vec<String>>;
    /// Wait until a freshly started CPA answers `/models`.
    fn wait_for_cpa(&self, cpa: &Cpa, token: &str) -> Result<()>;
    fn restart_proxy_if_running(&self) -> Result<()>;
}

pub(crate) struct Launchd;

impl ServiceControl for Launchd {
    fn cpa_loaded(&self) -> Result<bool> {
        launch_agent::is_loaded(CPA_AGENT_LABEL)
    }

    fn stop_cpa(&self) -> Result<()> {
        launch_agent::bootout(CPA_AGENT_LABEL)
    }

    fn bootstrap_cpa(&self, plist: &Path) -> Result<()> {
        launch_agent::bootstrap(plist, CPA_AGENT_LABEL)
    }

    fn cpa_models(&self, cpa: &Cpa, token: &str) -> Result<Vec<String>> {
        model_slugs(cpa, token)
    }

    fn wait_for_cpa(&self, cpa: &Cpa, token: &str) -> Result<()> {
        wait_for_model_slugs_with(
            || model_slugs(cpa, token),
            CPA_MODEL_VALIDATION_TIMEOUT,
            CPA_MODEL_VALIDATION_POLL_INTERVAL,
        )
        .map(|_| ())
    }

    fn restart_proxy_if_running(&self) -> Result<()> {
        launch_agent::restart_if_running()
    }
}

pub fn is_loaded() -> Result<bool> {
    Launchd.cpa_loaded()
}

/// Start CPA on explicit user request (CLI or menu bar). The choice becomes
/// the startup preference, but only when the service actually ended up
/// running.
pub fn start(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let _lock = lock(paths)?;
    let result = start_service(&Launchd, paths, cpa, token);
    if Launchd.cpa_loaded().unwrap_or(false)
        && let Err(error) = set_cpa_autostart(&paths.cpa_profiles, true)
    {
        tracing::warn!(error = %format!("{error:#}"), "failed to save CPA autostart preference");
    }
    result
}

/// Start CPA without touching the startup preference (proxy lifecycle).
pub fn start_service_only(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let _lock = lock(paths)?;
    start_service(&Launchd, paths, cpa, token)
}

/// Stop CPA on explicit user request; the choice becomes the startup
/// preference that the proxy lifecycle follows.
pub fn stop(paths: &Paths) -> Result<()> {
    let _lock = lock(paths)?;
    let result = Launchd.stop_cpa();
    if !Launchd.cpa_loaded().unwrap_or(true)
        && let Err(error) = set_cpa_autostart(&paths.cpa_profiles, false)
    {
        tracing::warn!(error = %format!("{error:#}"), "failed to save CPA autostart preference");
    }
    result
}

/// Stop CPA without touching the startup preference (proxy idle exit, app
/// shutdown).
pub fn stop_service_only(paths: &Paths) -> Result<()> {
    let _lock = lock(paths)?;
    Launchd.stop_cpa()
}

/// Write the managed config and make sure CPA runs it. A loaded job whose
/// config did not change is left alone.
pub(super) fn start_service(
    services: &dyn ServiceControl,
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
) -> Result<()> {
    let previous_config = fs::read(config_path(paths)).ok();
    write_config(paths, cpa, token)?;
    let config_changed = previous_config.as_deref() != fs::read(config_path(paths)).ok().as_deref();
    if services.cpa_loaded()? && !config_changed {
        return ensure_management_connect_bootstrap(paths);
    }
    services.stop_cpa()?;
    bootstrap_service(services, paths)?;
    ensure_management_connect_bootstrap(paths)
}

/// Stop CPA if it runs, then start it with the current managed config.
pub(super) fn restart_service(
    services: &dyn ServiceControl,
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
) -> Result<()> {
    services.stop_cpa()?;
    start_service(services, paths, cpa, token)
}

/// Write the job definition next to the binary and bootstrap it.
pub(super) fn bootstrap_service(services: &dyn ServiceControl, paths: &Paths) -> Result<()> {
    let binary = binary_path(paths);
    anyhow::ensure!(
        binary.is_file(),
        "CLIProxyAPI binary is not installed at {}; run codexmux cpa install",
        binary.display()
    );
    let logs = paths.root.join("logs");
    fs::create_dir_all(&logs).with_context(|| format!("failed to create {}", logs.display()))?;
    let document = render_agent(
        &binary,
        &config_path(paths),
        &logs.join("cpa-stdout.log"),
        &logs.join("cpa-stderr.log"),
    )?;
    let plist = plist_path(paths);
    atomic_write(&plist, document.as_bytes())?;
    services.bootstrap_cpa(&plist)
}

#[derive(Debug)]
pub struct UninstallOutcome {
    /// The CPA job was loaded and has been stopped.
    pub stopped: bool,
    /// The job definition that was removed.
    pub removed: Option<PathBuf>,
}

/// Stop CPA and remove its job definition, keeping the binary, config, and
/// provider auth files.
pub fn uninstall(paths: &Paths) -> Result<UninstallOutcome> {
    let _lock = lock(paths)?;
    uninstall_with(&Launchd, paths)
}

fn uninstall_with(services: &dyn ServiceControl, paths: &Paths) -> Result<UninstallOutcome> {
    let stopped = services.cpa_loaded()?;
    services.stop_cpa()?;
    let plist = plist_path(paths);
    let removed = match fs::remove_file(&plist) {
        Ok(()) => Some(plist),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("failed to remove {}", plist.display()));
        }
    };
    Ok(UninstallOutcome { stopped, removed })
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
        launch_agent::xml_escape(binary),
        launch_agent::xml_escape(config),
        launch_agent::xml_escape(working_dir),
        launch_agent::xml_escape(stdout),
        launch_agent::xml_escape(stderr),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpa::test_support::{FakeServices, local_cpa, test_root};

    #[test]
    fn plist_renders_config_path_and_escapes_paths() {
        let plist = render_agent(
            Path::new("/tmp/a&b/cli-proxy-api"),
            Path::new("/tmp/root/cpa/config.yaml"),
            Path::new("/tmp/root/logs/out.log"),
            Path::new("/tmp/root/logs/err.log"),
        )
        .unwrap();
        assert!(plist.contains("/tmp/a&amp;b/cli-proxy-api"));
        assert!(plist.contains("<string>-config</string>"));
        assert!(plist.contains("/tmp/root/cpa/config.yaml"));
        assert!(plist.contains("dev.codexmux.cpa"));
        assert!(plist.contains("<key>WorkingDirectory</key><string>/tmp/a&amp;b</string>"));
    }

    #[test]
    fn start_bootstraps_the_job_definition_stored_with_the_binary() {
        let root = test_root();
        let paths = &root.paths;
        fs::create_dir_all(binary_path(paths).parent().unwrap()).unwrap();
        fs::write(binary_path(paths), "cpa-binary").unwrap();
        let services = FakeServices::default();

        start_service(&services, paths, &local_cpa(8317), "cpa-token").unwrap();
        assert_eq!(services.events(), ["bootstrap cpa-binary"]);
        let plist = fs::read_to_string(plist_path(paths)).unwrap();
        assert!(plist.contains(&binary_path(paths).display().to_string()));

        // A loaded job with an unchanged config is left running.
        start_service(&services, paths, &local_cpa(8317), "cpa-token").unwrap();
        assert_eq!(services.events(), ["bootstrap cpa-binary"]);

        // A changed config restarts it.
        start_service(&services, paths, &local_cpa(9317), "cpa-token").unwrap();
        assert_eq!(
            services.events(),
            ["bootstrap cpa-binary", "stop", "bootstrap cpa-binary"]
        );
    }

    #[test]
    fn start_requires_an_installed_binary() {
        let root = test_root();
        let services = FakeServices::default();
        let error =
            start_service(&services, &root.paths, &local_cpa(8317), "cpa-token").unwrap_err();
        assert!(error.to_string().contains("run codexmux cpa install"));
        assert!(services.events().is_empty());
    }

    #[test]
    fn uninstall_stops_the_job_and_removes_its_definition() {
        let root = test_root();
        let paths = &root.paths;
        fs::create_dir_all(binary_path(paths).parent().unwrap()).unwrap();
        fs::write(binary_path(paths), "cpa-binary").unwrap();
        let services = FakeServices::default();
        start_service(&services, paths, &local_cpa(8317), "cpa-token").unwrap();

        let outcome = uninstall_with(&services, paths).unwrap();
        assert!(outcome.stopped);
        assert_eq!(
            outcome.removed.as_deref(),
            Some(plist_path(paths).as_path())
        );
        assert!(binary_path(paths).is_file());
        assert!(config_path(paths).is_file());

        let outcome = uninstall_with(&services, paths).unwrap();
        assert!(!outcome.stopped);
        assert!(outcome.removed.is_none());
    }
}
