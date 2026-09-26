use std::{
    collections::BTreeMap,
    future::Future,
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use codexmux::{
    codex_config::{ConfigLease, ConfigManager, PROXY_TOKEN_ENV},
    config::{Credentials, Paths, Settings},
    cpa::{self, SearchCapabilityStatus},
    launch_agent::{self, RuntimeState},
    secrets,
    server::{self, AppState},
};
use serde::Serialize;
use tracing_appender::{non_blocking::WorkerGuard, rolling::RollingFileAppender};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create the private data directory and starter configuration.
    Init,
    /// Run the proxy in the foreground; `--no-codex-config` skips Codex
    /// configuration management (used by the LaunchAgent so the proxy never
    /// touches paths that macOS TCC may gate for background processes).
    Serve {
        /// Do not enable or restore the managed Codex configuration.
        #[arg(long)]
        no_codex_config: bool,
        /// Accept the listener socket supplied by launchd and exit after an
        /// idle period. Used by the installed on-demand LaunchAgent.
        #[arg(long)]
        launchd_socket: bool,
    },
    /// Show paths and managed configuration state.
    Status {
        /// Skip reading the managed Codex configuration.
        #[arg(long)]
        no_codex_config: bool,
    },
    /// Validate config, credentials, snapshot, and the managed Codex block.
    Doctor,
    /// Install and start the per-user macOS LaunchAgent.
    Install,
    /// Restore the Codex configuration, then remove the LaunchAgent.
    Uninstall,
    /// Print the menu bar state as one JSON object. Reads local files and
    /// launchd only: no Codex config, network, proxy connection, or locks.
    MenubarState,
    /// Manage the local CLIProxyAPI instance that serves CPA models.
    Cpa {
        #[command(subcommand)]
        command: CpaCommand,
    },
    /// Manage Codex-facing catalog metadata.
    Catalog {
        #[command(subcommand)]
        command: CatalogCommand,
    },
}

#[derive(Subcommand)]
enum CatalogCommand {
    /// Show whether the `ultra` preset is advertised for every model.
    UltraGet,
    /// Enable or disable `ultra` advertisement for every model.
    UltraSet {
        /// `true`/`false`: whether every model advertises `ultra`.
        enabled: String,
    },
    /// Show whether every model is served with one shared `comp_hash`.
    CompHashGet,
    /// Enable or disable serving one shared `comp_hash` for every model.
    CompHashSet {
        /// `true`/`false`: whether every model shares one `comp_hash`.
        enabled: String,
    },
    /// List every model slug in the merged catalog.
    Models,
}

#[derive(Subcommand)]
enum CpaCommand {
    /// Download, verify, and install the latest stable CLIProxyAPI release, then start it.
    Install {
        /// Install from a local release archive instead of downloading it.
        #[arg(long)]
        archive: Option<PathBuf>,
    },
    /// Query the latest stable CLIProxyAPI release without installing it.
    UpdateCheck,
    /// Update the managed local CPA to the latest stable CLIProxyAPI release.
    Update {
        /// Specific stable version to install, for example 7.2.147.
        #[arg(long)]
        version: Option<String>,
        /// Resolve and verify the release without replacing the installed binary.
        #[arg(long)]
        dry_run: bool,
    },
    /// Restore the previous CPA binary after an update.
    Rollback,
    /// Start the local CPA and make starting it the startup preference.
    Start,
    /// Stop the local CPA.
    Stop {
        /// Stop without updating the startup preference (app shutdown).
        #[arg(long)]
        no_preference: bool,
    },
    /// Show the installed version and service state.
    Status,
    /// List model slugs from the configured CPA endpoint.
    ModelList,
    /// Print the CPA web management page URL.
    ManagementUrl {
        /// For the local CPA, sign the Web UI in with the management key,
        /// which travels only in the URL fragment.
        #[arg(long)]
        connect: bool,
    },
    /// Print the local CPA web management key for explicit user handoff.
    ManagementKey,
    /// Import provider definitions from a TOML file and restart CPA.
    ProviderImport {
        /// TOML file with [[openai-compatibility]] / [[codex-api-key]] tables.
        file: PathBuf,
    },
    /// List saved CPA endpoint profiles and the active one.
    ProfileList,
    /// Save or update a CPA endpoint profile.
    ProfileSave {
        /// Profile name.
        name: String,
        /// CPA base URL, for example http://127.0.0.1:8317/v1.
        #[arg(long)]
        base_url: String,
        /// Environment variable containing the endpoint token.
        #[arg(long, default_value = "CODEXMUX_CPA_PROFILE_TOKEN")]
        token_env: String,
    },
    /// Remove a saved profile.
    ProfileRemove {
        /// Profile name.
        name: String,
    },
    /// Make a saved profile the CPA endpoint CodexMux uses; rolls back on failure.
    ProfileSwitch {
        /// Profile name.
        name: String,
    },
    /// Show the review model override (None = official route).
    ReviewGet,
    /// Route `codex-auto-review` straight to a CPA model (empty to clear).
    ReviewSet {
        /// Upstream CPA model slug; empty string clears the override.
        slug: String,
    },
    /// Show the image route override (None = official route).
    ImageGet,
    /// Route Codex image generation to a CPA image model (empty to clear).
    ImageSet {
        /// Upstream CPA image model slug; empty string clears the override.
        slug: String,
    },
    /// List image model slugs the configured CPA endpoint reports.
    ImageModelList,
    /// Show the shared Responses web search backend override.
    SearchGet,
    /// Select the shared web search backend: a merged catalog slug enables
    /// it, `off` (or an empty value) disables it, and `default` follows
    /// `config.toml`.
    SearchSet {
        /// A merged catalog model slug, `off`, or `default`.
        value: String,
    },
    /// Print the cached shared web search capability status.
    SearchCapabilities,
    /// Detect Responses `web_search` support for catalog models and cache
    /// the results. The default pass is local heuristics only; `--verify`
    /// runs one real search.
    SearchDetect {
        /// Only detect this exact catalog slug.
        #[arg(long)]
        model: Option<String>,
        /// Run one real search against the selected model (requires --model).
        #[arg(long)]
        verify: bool,
    },
    /// Show whether the local CPA runs while Codex is active.
    AutostartGet,
    /// Set whether the local CPA runs while Codex is active.
    AutostartSet {
        /// `true`/`false`: whether CPA starts together with CodexMux.
        enabled: String,
    },
    /// Stop CPA and remove its job definition (keeps the binary, config, and auth files).
    Uninstall,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = Paths::discover()?;
    // Held until exit so the rolling log writer flushes everything.
    let _log_guard = init_logging(&cli.command, &paths);
    match cli.command {
        Command::Init => init(&paths),
        Command::Serve {
            no_codex_config,
            launchd_socket,
        } => run_async(serve(&paths, no_codex_config, launchd_socket)),
        Command::Status { no_codex_config } => status(&paths, no_codex_config),
        Command::Doctor => run_async(doctor(&paths)),
        Command::Install => install(&paths),
        Command::Uninstall => uninstall(&paths),
        Command::MenubarState => print_menubar_state(&paths),
        Command::Cpa { command } => cpa_command(&paths, command),
        Command::Catalog { command } => catalog(&paths, command),
    }
}

/// Route tracing output. The on-demand LaunchAgent writes a daily rolling
/// file; every other command writes stderr, so stdout carries only command
/// output such as `menubar-state` JSON or model lists.
fn init_logging(command: &Command, paths: &Paths) -> Option<WorkerGuard> {
    let filter =
        || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("codexmux=info"));
    if matches!(
        command,
        Command::Serve {
            launchd_socket: true,
            ..
        }
    ) {
        match log_file_appender(&paths.root.join("logs")) {
            Ok(appender) => {
                let (writer, guard) = tracing_appender::non_blocking(appender);
                tracing_subscriber::fmt()
                    .with_env_filter(filter())
                    .with_ansi(false)
                    .with_writer(writer)
                    .init();
                return Some(guard);
            }
            Err(error) => {
                eprintln!("codexmux: cannot open the log file, logging to stderr: {error:#}");
            }
        }
    }
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
    tracing_subscriber::fmt()
        .with_env_filter(filter())
        .with_ansi(std::io::stderr().is_terminal() && !no_color)
        .with_writer(std::io::stderr)
        .init();
    None
}

/// `logs/codexmux.YYYY-MM-DD.log`, rotated daily, keeping seven files. The
/// LaunchAgent's stdout/stderr files still catch panics.
fn log_file_appender(directory: &Path) -> Result<RollingFileAppender> {
    std::fs::create_dir_all(directory)
        .with_context(|| format!("failed to create {}", directory.display()))?;
    RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("codexmux")
        .filename_suffix("log")
        .max_log_files(7)
        .build(directory)
        .with_context(|| format!("failed to open a log file in {}", directory.display()))
}

fn run_async<F>(future: F) -> Result<()>
where
    F: Future<Output = Result<()>>,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to start the async runtime")?;
    runtime.block_on(future)
}

fn init(paths: &Paths) -> Result<()> {
    ensure_initialized(paths)?;
    println!("initialized {}", paths.root.display());
    println!(
        "edit {} to configure the CPA endpoint",
        paths.settings.display()
    );
    println!(
        "set cpa_token in {} to the token accepted by CPA",
        paths.credentials.display()
    );
    println!("set {PROXY_TOKEN_ENV} from credentials.json before starting Codex");
    Ok(())
}

fn ensure_initialized(paths: &Paths) -> Result<()> {
    paths.ensure()?;
    if !paths.settings.exists() {
        Settings::default().save(&paths.settings)?;
    }
    if !paths.credentials.exists() {
        secrets::save(
            &paths.credentials,
            &Credentials {
                proxy_token: uuid::Uuid::new_v4().simple().to_string(),
                cpa_token: uuid::Uuid::new_v4().simple().to_string(),
                cpa_management_key: uuid::Uuid::new_v4().simple().to_string(),
            },
        )?;
    }
    Ok(())
}

fn install(paths: &Paths) -> Result<()> {
    ensure_initialized(paths)?;
    let settings = Settings::load(&paths.settings)?;
    let credentials = secrets::load(&paths.credentials)?;
    // Enable the Codex configuration from the caller's context: the
    // LaunchAgent runs serve --no-codex-config because background processes
    // may be denied access to the Codex config's volume by macOS TCC, which
    // blocks open() indefinitely.
    let lease = config_manager(paths)?
        .enable(&format!("http://{}/v1", settings.listen))
        .context("failed to enable the managed Codex configuration")?;
    let executable = std::env::current_exe()?.canonicalize()?;
    // Keep the configuration enabled only when the agent was installed.
    match launch_agent::install(paths, &executable, &codex_config_path()?, settings.listen) {
        Ok(plist) => {
            if let Err(error) = set_launchctl_proxy_token(&credentials.proxy_token) {
                launch_agent::uninstall().ok();
                lease.restore().ok();
                return Err(error)
                    .context("failed to prepare the Codex environment; installation rolled back");
            }
            println!("installed {}", plist.display());
            std::mem::forget(lease);
            Ok(())
        }
        Err(error) => {
            lease.restore().ok();
            Err(error)
        }
    }
}

fn set_launchctl_proxy_token(token: &str) -> Result<()> {
    let status = std::process::Command::new("launchctl")
        .args(["setenv", PROXY_TOKEN_ENV, token])
        .status()
        .context("failed to run launchctl setenv")?;
    anyhow::ensure!(
        status.success(),
        "launchctl setenv {PROXY_TOKEN_ENV} failed with {status}"
    );
    Ok(())
}

async fn serve(paths: &Paths, no_codex_config: bool, launchd_socket: bool) -> Result<()> {
    let settings = Settings::load(&paths.settings)?;
    let credentials = secrets::load(&paths.credentials)?;
    let listener = if launchd_socket {
        let listener = launch_agent::activated_listener()?;
        anyhow::ensure!(
            listener.local_addr()? == settings.listen,
            "launchd listener {} does not match configured {}",
            listener.local_addr()?,
            settings.listen
        );
        tokio::net::TcpListener::from_std(listener)?
    } else {
        tokio::net::TcpListener::bind(settings.listen)
            .await
            .with_context(|| format!("failed to bind {}", settings.listen))?
    };
    let manage_cpa = launchd_socket && lifecycle_manages_cpa(paths, &settings);
    let launched_cpa = manage_cpa && start_lifecycle_cpa(paths, &settings, &credentials).await;
    let state = AppState::new(settings.clone(), credentials, paths)?;
    // The Codex request that socket-activated this process is already queued,
    // so let the CPA instance we just launched finish binding before answering
    // it; otherwise the first model refresh sees no CPA models at all.
    if launched_cpa {
        server::wait_for_cpa_catalog(&state, server::CPA_STARTUP_READY_TIMEOUT).await;
    }
    let shutdown = shutdown_signal()?;
    let lease = if no_codex_config {
        None
    } else {
        let manager = config_manager(paths)?;
        Some(manager.enable(&format!("http://{}/v1", settings.listen))?)
    };
    if lease.is_some() {
        tracing::info!(config = %codex_config_path()?.display(), "Codex configuration enabled");
    }

    let result = {
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let idle_state = state.clone();
        let proxy = server::serve(listener, state, async move {
            let _ = stopped.await;
        });
        let idle = async move {
            if launchd_socket {
                idle_state.wait_for_idle().await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::pin!(proxy);
        tokio::pin!(idle);
        let finished = tokio::select! {
            result = &mut proxy => Some(result),
            _ = shutdown => None,
            _ = &mut idle => None,
        };
        match finished {
            Some(result) => result,
            None => {
                let _ = stop.send(());
                match tokio::time::timeout(std::time::Duration::from_secs(30), &mut proxy).await {
                    Ok(result) => result,
                    Err(_) => {
                        tracing::warn!("graceful shutdown timed out after 30 seconds");
                        Ok(())
                    }
                }
            }
        }
    };
    if manage_cpa {
        stop_lifecycle_cpa(paths).await;
    }
    let restore = lease.map(ConfigLease::restore).transpose();
    match (result, restore) {
        (Ok(()), Ok(_)) => Ok(()),
        (Err(error), Ok(_)) => Err(error),
        (Ok(()), Err(error)) => Err(error).context("failed to restore Codex configuration"),
        (Err(serve_error), Err(restore_error)) => Err(serve_error).context(format!(
            "proxy stopped and Codex configuration could not be restored: {restore_error:#}"
        )),
    }
}

/// The on-demand proxy runs the local CPA while Codex is active only for a
/// local endpoint with an installed binary and an enabled startup preference.
fn lifecycle_manages_cpa(paths: &Paths, settings: &Settings) -> bool {
    if !settings.cpa.is_loopback() || !cpa::binary_path(paths).is_file() {
        return false;
    }
    match cpa::cpa_autostart(&paths.cpa_profiles) {
        Ok(enabled) => enabled,
        Err(error) => {
            tracing::warn!(
                error = %format!("{error:#}"),
                "cannot read the CPA startup preference; leaving the local CPA alone"
            );
            false
        }
    }
}

/// Start the local CPA for this proxy's lifetime. Failure only costs the CPA
/// models: the proxy keeps serving official ones. Returns whether this call
/// started CPA.
async fn start_lifecycle_cpa(
    paths: &Paths,
    settings: &Settings,
    credentials: &Credentials,
) -> bool {
    let (paths, cpa_settings, token) = (
        paths.clone(),
        settings.cpa.clone(),
        credentials.cpa_token.clone(),
    );
    let started = tokio::task::spawn_blocking(move || -> Result<bool> {
        if cpa::is_loaded()? {
            return Ok(false);
        }
        cpa::start_service_only(&paths, &cpa_settings, &token)?;
        Ok(true)
    })
    .await;
    match started {
        Ok(Ok(started)) => {
            if started {
                tracing::info!("CPA started for active Codex client");
            }
            started
        }
        Ok(Err(error)) => {
            tracing::warn!(
                error = %format!("{error:#}"),
                "failed to start the local CPA; serving without CPA models"
            );
            false
        }
        Err(error) => {
            tracing::warn!(%error, "the CPA start task failed; serving without CPA models");
            false
        }
    }
}

async fn stop_lifecycle_cpa(paths: &Paths) {
    let paths = paths.clone();
    let stopped = tokio::task::spawn_blocking(move || -> Result<bool> {
        if !cpa::is_loaded()? {
            return Ok(false);
        }
        cpa::stop_service_only(&paths)?;
        Ok(true)
    })
    .await;
    match stopped {
        Ok(Ok(true)) => tracing::info!("CPA stopped after Codex client became idle"),
        Ok(Ok(false)) => {}
        Ok(Err(error)) => {
            tracing::warn!(error = %format!("{error:#}"), "failed to stop lifecycle-managed CPA");
        }
        Err(error) => tracing::warn!(%error, "the CPA stop task failed"),
    }
}

fn shutdown_signal() -> Result<impl Future<Output = ()> + Send + 'static> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut interrupt =
        signal(SignalKind::interrupt()).context("failed to install SIGINT handler")?;
    let mut terminate =
        signal(SignalKind::terminate()).context("failed to install SIGTERM handler")?;
    let mut hangup = signal(SignalKind::hangup()).context("failed to install SIGHUP handler")?;
    Ok(async move {
        tokio::select! {
            _ = interrupt.recv() => {},
            _ = terminate.recv() => {},
            _ = hangup.recv() => {},
        }
    })
}

/// Restore Codex first; the LaunchAgent is removed only afterwards, so Codex
/// never points at a proxy that is gone. Either failure exits nonzero.
fn uninstall(paths: &Paths) -> Result<()> {
    let was_managed = paths.state.exists();
    config_manager(paths)?.disable().context(
        "failed to restore the Codex configuration; the CodexMux LaunchAgent was left installed",
    )?;
    if was_managed {
        println!("restored Codex configuration");
    }
    match launch_agent::uninstall()? {
        Some(path) => println!("removed {}", path.display()),
        None => println!("LaunchAgent is not installed"),
    }
    unset_launchctl_proxy_token()
}

fn unset_launchctl_proxy_token() -> Result<()> {
    let status = std::process::Command::new("launchctl")
        .args(["unsetenv", PROXY_TOKEN_ENV])
        .status()
        .context("failed to run launchctl unsetenv")?;
    anyhow::ensure!(
        status.success(),
        "launchctl unsetenv {PROXY_TOKEN_ENV} failed with {status}"
    );
    Ok(())
}

fn status(paths: &Paths, no_codex_config: bool) -> Result<()> {
    println!("root: {}", paths.root.display());
    println!("settings: {}", paths.settings.display());
    println!("credentials: {}", paths.credentials.display());
    println!("catalog: {}", paths.catalog.display());
    println!(
        "proxy service: {}",
        match launch_agent::runtime_state()? {
            RuntimeState::Running => "running",
            RuntimeState::Idle => "idle",
            RuntimeState::NotInstalled => "not installed",
        }
    );
    if no_codex_config {
        println!("enabled: not checked (--no-codex-config)");
    } else {
        let status = config_manager(paths)?.status()?;
        println!("enabled: {}", status.enabled);
        println!("codex config: {}", status.config_path.display());
        println!("config unchanged: {}", status.unchanged_since_enable);
    }
    Ok(())
}

async fn doctor(paths: &Paths) -> Result<()> {
    let settings = Settings::load(&paths.settings).context("settings check failed")?;
    let credentials = secrets::load(&paths.credentials).context("credentials check failed")?;
    anyhow::ensure!(
        proxy_token_is_available(&credentials.proxy_token),
        "{PROXY_TOKEN_ENV} is missing or does not match credentials.json"
    );
    let status = config_manager(paths)?.status()?;
    match reqwest::Client::new()
        .get(format!("http://{}/health", settings.listen))
        .header("x-codexmux-token", &credentials.proxy_token)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => println!("proxy: reachable"),
        _ => println!("proxy: not running"),
    }
    let cpa_models = format!("{}/models", settings.cpa.base_url.trim_end_matches('/'));
    match reqwest::Client::new()
        .get(cpa_models)
        .bearer_auth(&credentials.cpa_token)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => println!("CPA: reachable"),
        _ => println!("CPA: not reachable"),
    }
    println!("settings: ok");
    println!("credentials: ok");
    println!(
        "catalog snapshot: {}",
        if paths.catalog.is_file() {
            "available"
        } else {
            "not fetched yet"
        }
    );
    println!(
        "managed config: {}",
        if status.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    Ok(())
}

fn proxy_token_is_available(expected: &str) -> bool {
    if std::env::var(PROXY_TOKEN_ENV).as_deref() == Ok(expected) {
        return true;
    }
    std::process::Command::new("launchctl")
        .args(["getenv", PROXY_TOKEN_ENV])
        .output()
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == expected
        })
}

fn config_manager(paths: &Paths) -> Result<ConfigManager> {
    Ok(ConfigManager::new(
        codex_config_path()?,
        paths.state.clone(),
        paths.backups.clone(),
    ))
}

fn codex_config_path() -> Result<PathBuf> {
    codexmux::codex_config::codex_config_path()
}

fn catalog(paths: &Paths, command: CatalogCommand) -> Result<()> {
    let mut settings = Settings::load(&paths.settings)?;
    match command {
        CatalogCommand::UltraGet => println!("ultra: {}", settings.catalog.advertise_ultra),
        CatalogCommand::UltraSet { enabled } => {
            settings.catalog.advertise_ultra = parse_flag(&enabled)?;
            settings.save(&paths.settings)?;
            println!("ultra: {}", settings.catalog.advertise_ultra);
        }
        CatalogCommand::CompHashGet => {
            println!("unify-comp-hash: {}", settings.catalog.unify_comp_hash);
        }
        CatalogCommand::CompHashSet { enabled } => {
            settings.catalog.unify_comp_hash = parse_flag(&enabled)?;
            settings.save(&paths.settings)?;
            println!("unify-comp-hash: {}", settings.catalog.unify_comp_hash);
        }
        CatalogCommand::Models => {
            let slugs = cpa::catalog_slugs(paths)?
                .context("model catalog snapshot has not been built yet")?;
            for slug in slugs {
                println!("{slug}");
            }
        }
    }
    Ok(())
}

fn parse_flag(value: &str) -> Result<bool> {
    match value.trim() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => bail!("expected true or false, got {other:?}"),
    }
}

/// Commands that manage the local CPA binary or service refuse to run while
/// CodexMux uses a remote endpoint; its URL says nothing about a local CPA.
fn require_local_cpa(settings: &Settings, command: &str) -> Result<()> {
    anyhow::ensure!(
        settings.cpa.is_loopback(),
        "`codexmux cpa {command}` manages the local CLIProxyAPI, but CodexMux uses the remote CPA endpoint {}; switch to a local profile first",
        settings.cpa.base_url
    );
    Ok(())
}

fn cpa_command(paths: &Paths, command: CpaCommand) -> Result<()> {
    let settings = Settings::load(&paths.settings)?;
    match command {
        CpaCommand::Install { archive } => {
            require_local_cpa(&settings, "install")?;
            let token = secrets::load(&paths.credentials)?.cpa_token;
            let installed = match archive {
                Some(archive) => cpa::install_from_archive(
                    paths,
                    &settings.cpa,
                    &archive,
                    cpa::CPA_VERSION,
                    cpa::CPA_DARWIN_AARCH64_SHA256,
                    &token,
                )?,
                None => cpa::install(paths, &settings.cpa, &token)?,
            };
            println!(
                "CLIProxyAPI {} installed at {}",
                installed.version,
                cpa::binary_path(paths).display()
            );
            println!("config: {}", cpa::config_path(paths).display());
            println!(
                "service: started ({}); logs: {}/logs/cpa-*.log",
                cpa::agent_label(),
                paths.root.display()
            );
        }
        CpaCommand::UpdateCheck => {
            let check = cpa::check_cpa_update(paths, None)?;
            println!(
                "current: {}",
                check.current_version.as_deref().unwrap_or("not installed")
            );
            println!("latest: {}", check.latest_version);
            println!("update available: {}", check.update_available);
        }
        CpaCommand::Update { version, dry_run } => {
            require_local_cpa(&settings, "update")?;
            let token = secrets::load(&paths.credentials)?.cpa_token;
            let outcome =
                cpa::update_cpa(paths, &settings.cpa, &token, version.as_deref(), dry_run)?;
            if outcome.dry_run {
                println!("current: {}", outcome.from_version);
                println!("target: {}", outcome.to_version);
                println!("dry run: true");
                println!(
                    "change: {}",
                    if outcome.changed { "available" } else { "none" }
                );
            } else {
                println!(
                    "updated CPA {} -> {}",
                    outcome.from_version, outcome.to_version
                );
            }
        }
        CpaCommand::Rollback => {
            require_local_cpa(&settings, "rollback")?;
            let token = secrets::load(&paths.credentials)?.cpa_token;
            cpa::rollback_cpa(paths, &settings.cpa, &token)?;
            println!(
                "CPA rolled back to {}",
                cpa::installed_version(paths)?.map_or_else(|| "unknown".into(), |v| v.version)
            );
        }
        CpaCommand::Start => {
            require_local_cpa(&settings, "start")?;
            let token = secrets::load(&paths.credentials)?.cpa_token;
            cpa::start(paths, &settings.cpa, &token)?;
            println!("CPA service started");
        }
        CpaCommand::Stop { no_preference } => {
            require_local_cpa(&settings, "stop")?;
            if no_preference {
                cpa::stop_service_only(paths)?;
            } else {
                cpa::stop(paths)?;
            }
            println!("CPA service stopped");
        }
        CpaCommand::Status => {
            match cpa::installed_version(paths) {
                Ok(Some(version)) => {
                    println!("version: {} (sha256 {})", version.version, version.sha256);
                }
                Ok(None) => println!("version: not installed"),
                Err(error) => println!("version: unreadable ({error:#})"),
            }
            println!(
                "binary: {}",
                if cpa::binary_path(paths).is_file() {
                    "installed"
                } else {
                    "missing"
                }
            );
            println!(
                "service: {}",
                if cpa::is_loaded()? {
                    "running"
                } else {
                    "stopped"
                }
            );
            println!(
                "autostart: {}",
                if cpa::cpa_autostart(&paths.cpa_profiles)? {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!(
                "rollback: {}",
                if cpa::rollback_available(paths) {
                    "available"
                } else {
                    "no previous version"
                }
            );
            println!("config: {}", cpa::config_path(paths).display());
        }
        CpaCommand::ModelList => {
            let token = secrets::load(&paths.credentials)?.cpa_token;
            for slug in cpa::model_slugs(&settings.cpa, &token)? {
                println!("{slug}");
            }
        }
        CpaCommand::ManagementUrl { connect } => {
            if connect {
                require_local_cpa(&settings, "management-url --connect")?;
                let management_key = secrets::load(&paths.credentials)?.cpa_management_key;
                cpa::sync_management_key(paths, &management_key)?;
                cpa::ensure_management_connect_bootstrap(paths)?;
                println!("{}", settings.cpa.management_connect_url(&management_key)?);
            } else {
                println!("{}", settings.cpa.management_url()?);
            }
        }
        CpaCommand::ManagementKey => {
            require_local_cpa(&settings, "management-key")?;
            let management_key = secrets::load(&paths.credentials)?.cpa_management_key;
            cpa::sync_management_key(paths, &management_key)?;
            println!("management-key: {management_key}");
        }
        CpaCommand::ProviderImport { file } => {
            require_local_cpa(&settings, "provider-import")?;
            let token = secrets::load(&paths.credentials)?.cpa_token;
            cpa::import_providers(paths, &settings.cpa, &token, &file)?;
            println!("imported providers from {}", file.display());
            println!(
                "CPA service restarted with the new providers; config: {}",
                cpa::config_path(paths).display()
            );
        }
        CpaCommand::Uninstall => {
            let outcome = cpa::uninstall(paths)?;
            if outcome.stopped {
                println!("stopped the CPA service");
            }
            match outcome.removed {
                Some(plist) => println!("removed {}", plist.display()),
                None if !outcome.stopped => println!("CPA service is not installed"),
                None => {}
            }
            println!(
                "kept binary and config under {}",
                paths.root.join("cpa").display()
            );
        }
        CpaCommand::ProfileList => {
            let (active, profiles) = cpa::profiles(paths)?;
            match active {
                Some(active) => println!("active: {active}"),
                None => println!("active: (none; using config.toml settings)"),
            }
            if profiles.is_empty() {
                println!("no saved profiles; add one with `codexmux cpa profile-save`");
            }
            for profile in profiles {
                println!("  {} — {}", profile.name, profile.base_url);
            }
        }
        CpaCommand::ProfileSave {
            name,
            base_url,
            token_env,
        } => {
            let token = std::env::var(&token_env)
                .with_context(|| format!("{token_env} must contain the CPA profile token"))?;
            cpa::save_profile(
                paths,
                cpa::CpaProfile {
                    name,
                    base_url,
                    token,
                },
            )?;
            println!("profile saved to {}", paths.cpa_profiles.display());
        }
        CpaCommand::ProfileRemove { name } => {
            cpa::remove_profile(paths, &name)?;
            println!("profile {name} removed");
        }
        CpaCommand::ProfileSwitch { name } => {
            cpa::switch_profile(paths, &name)?;
            println!("switched to profile {name}");
        }
        CpaCommand::ReviewGet => match cpa::review_override(&paths.cpa_profiles)? {
            Some(slug) => println!("review override: {slug}"),
            None => println!("review override: (none; official route)"),
        },
        CpaCommand::ReviewSet { slug } => {
            let slug = slug.trim().to_owned();
            if slug.is_empty() {
                cpa::set_review_override(&paths.cpa_profiles, None)?;
                println!("review override cleared");
            } else {
                cpa::set_review_override(&paths.cpa_profiles, Some(slug.clone()))?;
                println!("codex-auto-review now routes directly to {slug}");
            }
        }
        CpaCommand::ImageGet => match cpa::image_override(&paths.cpa_profiles)? {
            Some(slug) => println!("image override: {slug}"),
            None => println!("image override: (none; official route)"),
        },
        CpaCommand::ImageSet { slug } => {
            let slug = slug.trim().to_owned();
            if slug.is_empty() {
                cpa::set_image_override(&paths.cpa_profiles, None)?;
                println!("image override cleared");
            } else {
                cpa::set_image_override(&paths.cpa_profiles, Some(slug.clone()))?;
                println!("image generation now routes to {slug}");
            }
        }
        CpaCommand::ImageModelList => {
            let token = secrets::load(&paths.credentials)?.cpa_token;
            for slug in cpa::image_model_slugs(&settings.cpa, &token)? {
                println!("{slug}");
            }
        }
        CpaCommand::SearchGet => match cpa::search_backend_setting(&paths.cpa_profiles)? {
            Some(setting) if setting.enabled => {
                println!("shared search: enabled: {}", setting.backend_model);
            }
            Some(_) => println!("shared search: disabled (menu override)"),
            None => println!("shared search: (using config.toml settings)"),
        },
        CpaCommand::SearchSet { value } => {
            let value = value.trim();
            let override_kind = if value.eq_ignore_ascii_case("default") {
                None
            } else if value.is_empty() || value.eq_ignore_ascii_case("off") {
                Some(None)
            } else {
                Some(Some(value.to_owned()))
            };
            let message = match &override_kind {
                None => "shared search override cleared; config.toml is authoritative".to_owned(),
                Some(None) => "shared search disabled".to_owned(),
                Some(Some(slug)) => format!("shared search backend: {slug}"),
            };
            cpa::set_search_backend_setting(&paths.cpa_profiles, override_kind)?;
            println!("{message}");
        }
        CpaCommand::SearchCapabilities => {
            let store = cpa::load_search_capabilities(&paths.search_capabilities)?;
            for (slug, entry) in store.entries {
                println!("{slug} {} {}", entry.status.label(), entry.checked_at);
            }
        }
        CpaCommand::SearchDetect { model, verify } => {
            let results = cpa::detect_search_capabilities(paths, model.as_deref(), verify)?;
            for (slug, status) in &results {
                println!("{slug} {}", status.label());
            }
            println!(
                "cached {} results in {}",
                results.len(),
                paths.search_capabilities.display()
            );
        }
        CpaCommand::AutostartGet => println!(
            "cpa autostart: {}",
            if cpa::cpa_autostart(&paths.cpa_profiles)? {
                "enabled"
            } else {
                "disabled"
            }
        ),
        CpaCommand::AutostartSet { enabled } => {
            let enabled = parse_flag(&enabled)?;
            cpa::set_cpa_autostart(&paths.cpa_profiles, enabled)?;
            println!(
                "cpa autostart: {}",
                if enabled { "enabled" } else { "disabled" }
            );
        }
    }
    Ok(())
}

/// `codexmux menubar-state` output. Field names are a contract with the menu
/// bar app.
#[derive(Debug, Serialize)]
struct MenubarState {
    version: &'static str,
    proxy: ProxyStatus,
    cpa: MenubarCpa,
    catalog: MenubarCatalog,
    profiles: MenubarProfiles,
    /// Stored upstream slug, without the `cpa/` prefix.
    review_override: Option<String>,
    /// Stored upstream slug, without the `cpa/` prefix.
    image_override: Option<String>,
    search: MenubarSearch,
    search_capabilities: BTreeMap<String, SearchCapabilityStatus>,
    errors: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProxyStatus {
    Running,
    Idle,
    NotInstalled,
}

#[derive(Debug, Serialize)]
struct MenubarCpa {
    local: bool,
    installed: bool,
    running: bool,
    version: Option<String>,
    rollback_available: bool,
    autostart: bool,
}

#[derive(Debug, Serialize)]
struct MenubarCatalog {
    advertise_ultra: bool,
    unify_comp_hash: bool,
    models: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MenubarProfiles {
    active: Option<String>,
    saved: Vec<MenubarProfile>,
}

#[derive(Debug, Serialize)]
struct MenubarProfile {
    name: String,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct MenubarSearch {
    mode: SearchMode,
    backend_model: Option<String>,
    default_backend_model: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SearchMode {
    Default,
    Enabled,
    Disabled,
}

fn print_menubar_state(paths: &Paths) -> Result<()> {
    let state = menubar_state(paths, launch_agent::runtime_state(), cpa::is_loaded());
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, &state)?;
    writeln!(stdout)?;
    Ok(())
}

/// Gather the menu bar state from local files. Nothing here takes a lock,
/// opens a network connection, or reads the Codex config, and a section that
/// cannot be read falls back to its default with the reason in `errors`.
fn menubar_state(
    paths: &Paths,
    proxy: Result<RuntimeState>,
    cpa_loaded: Result<bool>,
) -> MenubarState {
    let mut errors = Vec::new();
    let mut note = |section: &str, error: anyhow::Error| {
        errors.push(format!("{section}: {error:#}"));
    };

    let proxy = match proxy {
        Ok(RuntimeState::Running) => ProxyStatus::Running,
        Ok(RuntimeState::Idle) => ProxyStatus::Idle,
        Ok(RuntimeState::NotInstalled) => ProxyStatus::NotInstalled,
        Err(error) => {
            note("proxy", error);
            ProxyStatus::NotInstalled
        }
    };
    let settings = Settings::load(&paths.settings).unwrap_or_else(|error| {
        note("settings", error);
        Settings::default()
    });
    let profile_settings = cpa::profile_settings(&paths.cpa_profiles)
        .map_err(|error| note("profiles", error))
        .ok();
    let running = cpa_loaded.unwrap_or_else(|error| {
        note("cpa", error);
        false
    });
    let version = cpa::installed_version(paths).unwrap_or_else(|error| {
        note("cpa version", error);
        None
    });
    let models = cpa::catalog_slugs(paths).unwrap_or_else(|error| {
        note("catalog", error);
        None
    });
    let search_capabilities = cpa::load_search_capabilities(&paths.search_capabilities)
        .map(|store| {
            store
                .entries
                .into_iter()
                .map(|(slug, entry)| (slug, entry.status))
                .collect()
        })
        .unwrap_or_else(|error| {
            note("search capabilities", error);
            BTreeMap::new()
        });

    let profile_settings = profile_settings.as_ref();
    let search_override = profile_settings.and_then(|profile| profile.search_backend.clone());
    let (mode, backend_model) = match search_override {
        None => (SearchMode::Default, None),
        Some(setting) if setting.enabled => (SearchMode::Enabled, Some(setting.backend_model)),
        Some(_) => (SearchMode::Disabled, None),
    };
    MenubarState {
        version: env!("CARGO_PKG_VERSION"),
        proxy,
        cpa: MenubarCpa {
            local: settings.cpa.is_loopback(),
            installed: cpa::binary_path(paths).is_file(),
            running,
            version: version.map(|installed| installed.version),
            rollback_available: cpa::rollback_available(paths),
            // An unreadable preference means the proxy lifecycle leaves CPA
            // alone, so it reads as disabled.
            autostart: profile_settings.is_some_and(|profile| profile.cpa_autostart),
        },
        catalog: MenubarCatalog {
            advertise_ultra: settings.catalog.advertise_ultra,
            unify_comp_hash: settings.catalog.unify_comp_hash,
            models: models.unwrap_or_default(),
        },
        profiles: MenubarProfiles {
            active: profile_settings.and_then(|profile| profile.active.clone()),
            saved: profile_settings
                .map(|profile| {
                    profile
                        .profiles
                        .iter()
                        .map(|saved| MenubarProfile {
                            name: saved.name.clone(),
                            base_url: saved.base_url.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        review_override: profile_settings.and_then(|profile| profile.review_override.clone()),
        image_override: profile_settings.and_then(|profile| profile.image_override.clone()),
        search: MenubarSearch {
            mode,
            backend_model,
            default_backend_model: settings
                .web_search
                .enabled
                .then(|| settings.web_search.backend_model.clone()),
        },
        search_capabilities,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use serde_json::json;

    use super::*;

    fn initialized_root() -> (tempfile::TempDir, Paths) {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(root.path().join("CodexMux"));
        ensure_initialized(&paths).unwrap();
        (root, paths)
    }

    #[test]
    fn initialization_creates_private_state_and_preserves_it() {
        let (_root, paths) = initialized_root();
        let first_settings = std::fs::read(&paths.settings).unwrap();
        let first_credentials = std::fs::read(&paths.credentials).unwrap();
        secrets::load(&paths.credentials)
            .unwrap()
            .validate()
            .unwrap();

        ensure_initialized(&paths).unwrap();
        assert_eq!(std::fs::read(&paths.settings).unwrap(), first_settings);
        assert_eq!(
            std::fs::read(&paths.credentials).unwrap(),
            first_credentials
        );
        assert_eq!(
            std::fs::metadata(&paths.credentials)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn menubar_state_field_names_and_values_are_a_stable_contract() {
        let (_root, paths) = initialized_root();
        let mut settings = Settings::load(&paths.settings).unwrap();
        settings.catalog.advertise_ultra = true;
        settings.web_search.enabled = true;
        settings.web_search.backend_model = "gpt-5.6-sol".into();
        settings.save(&paths.settings).unwrap();
        std::fs::create_dir_all(cpa::binary_path(&paths).parent().unwrap()).unwrap();
        std::fs::write(cpa::binary_path(&paths), "binary").unwrap();
        std::fs::write(
            paths.root.join("cpa/version.json"),
            r#"{"version":"7.2.150","sha256":""}"#,
        )
        .unwrap();
        cpa::save_profile(
            &paths,
            cpa::CpaProfile {
                name: "remote".into(),
                base_url: "https://cpa.example.com/v1".into(),
                token: "remote-secret-token".into(),
            },
        )
        .unwrap();
        cpa::set_review_override(&paths.cpa_profiles, Some("glm-5.3-flash".into())).unwrap();
        cpa::set_search_backend_setting(&paths.cpa_profiles, Some(Some("cpa/grok-4.6".into())))
            .unwrap();
        codexmux::catalog::CatalogStore::load(paths.catalog.clone())
            .unwrap()
            .replace(
                &json!({"models": [{"slug": "gpt-5.6-sol"}]}),
                &json!({"models": [{"slug": "grok-4.6"}]}),
            )
            .unwrap();
        std::fs::write(
            &paths.search_capabilities,
            r#"{"entries":{"cpa/grok-4.6":{"status":"supported","checked_at":1}}}"#,
        )
        .unwrap();

        let state = menubar_state(&paths, Ok(RuntimeState::Idle), Ok(true));
        let value = serde_json::to_value(&state).unwrap();
        assert_eq!(
            value,
            json!({
                "version": env!("CARGO_PKG_VERSION"),
                "proxy": "idle",
                "cpa": {
                    "local": true,
                    "installed": true,
                    "running": true,
                    "version": "7.2.150",
                    "rollback_available": false,
                    "autostart": true
                },
                "catalog": {
                    "advertise_ultra": true,
                    "unify_comp_hash": true,
                    "models": value["catalog"]["models"].clone()
                },
                "profiles": {
                    "active": null,
                    "saved": [{"name": "remote", "base_url": "https://cpa.example.com/v1"}]
                },
                "review_override": "glm-5.3-flash",
                "image_override": null,
                "search": {
                    "mode": "enabled",
                    "backend_model": "cpa/grok-4.6",
                    "default_backend_model": "gpt-5.6-sol"
                },
                "search_capabilities": {"cpa/grok-4.6": "supported"},
                "errors": []
            })
        );
        let models = value["catalog"]["models"].as_array().unwrap();
        assert!(models.contains(&json!("gpt-5.6-sol")));
        assert!(models.contains(&json!("cpa/grok-4.6")));
        assert!(!value.to_string().contains("remote-secret-token"));
    }

    #[test]
    fn menubar_state_reports_unreadable_sections_and_falls_back_to_defaults() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(root.path().join("CodexMux"));
        std::fs::create_dir_all(paths.root.join("cpa")).unwrap();
        std::fs::write(
            &paths.cpa_profiles,
            "[[profile]]\nname = \"remote\"\ntoken = sk-live-secret\n",
        )
        .unwrap();
        std::fs::write(paths.root.join("cpa/version.json"), "{").unwrap();
        std::fs::write(&paths.catalog, "{").unwrap();

        let state = menubar_state(
            &paths,
            Err(anyhow::anyhow!("launchctl unavailable")),
            Err(anyhow::anyhow!("launchctl unavailable")),
        );
        let value = serde_json::to_value(&state).unwrap();
        assert_eq!(value["proxy"], "not_installed");
        assert_eq!(value["cpa"]["running"], false);
        assert_eq!(value["cpa"]["autostart"], false);
        assert_eq!(value["cpa"]["version"], serde_json::Value::Null);
        assert_eq!(value["catalog"]["models"], json!([]));
        assert_eq!(value["profiles"], json!({"active": null, "saved": []}));
        assert_eq!(value["search"]["mode"], "default");
        let sections = state
            .errors
            .iter()
            .map(|error| error.split(':').next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            sections,
            [
                "proxy",
                "settings",
                "profiles",
                "cpa",
                "cpa version",
                "catalog"
            ]
        );
        assert!(!value.to_string().contains("sk-live-secret"));
    }

    #[test]
    fn lifecycle_manages_only_an_installed_local_cpa_with_autostart() {
        let (_root, paths) = initialized_root();
        let settings = Settings::load(&paths.settings).unwrap();
        // A new user without a CPA binary: the proxy serves official models only.
        assert!(!lifecycle_manages_cpa(&paths, &settings));

        std::fs::create_dir_all(cpa::binary_path(&paths).parent().unwrap()).unwrap();
        std::fs::write(cpa::binary_path(&paths), "binary").unwrap();
        assert!(lifecycle_manages_cpa(&paths, &settings));

        let mut remote = settings.clone();
        remote.cpa.base_url = "https://cpa.example.com/v1".into();
        assert!(!lifecycle_manages_cpa(&paths, &remote));

        cpa::set_cpa_autostart(&paths.cpa_profiles, false).unwrap();
        assert!(!lifecycle_manages_cpa(&paths, &settings));

        std::fs::write(&paths.cpa_profiles, "[broken").unwrap();
        assert!(!lifecycle_manages_cpa(&paths, &settings));
    }

    #[test]
    fn local_cpa_commands_refuse_a_remote_endpoint() {
        let mut settings = Settings::default();
        require_local_cpa(&settings, "start").unwrap();
        settings.cpa.base_url = "https://cpa.example.com/v1".into();
        let error = require_local_cpa(&settings, "update").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("codexmux cpa update"));
        assert!(message.contains("https://cpa.example.com/v1"));
    }

    #[test]
    fn launchd_logs_rotate_daily_and_keep_seven_files() {
        let root = tempfile::tempdir().unwrap();
        let logs = root.path().join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        for day in 1..=9 {
            std::fs::write(logs.join(format!("codexmux.2020-01-0{day}.log")), "old").unwrap();
        }
        std::fs::write(logs.join("stderr.log"), "panic output").unwrap();

        let mut appender = log_file_appender(&logs).unwrap();
        appender.write_all(b"started\n").unwrap();
        appender.flush().unwrap();

        let mut names = std::fs::read_dir(&logs)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        names.sort();
        assert!(names.contains(&"stderr.log".to_owned()));
        let rolling = names
            .iter()
            .filter(|name| name.starts_with("codexmux.") && name.ends_with(".log"))
            .collect::<Vec<_>>();
        assert_eq!(rolling.len(), 7, "{names:?}");
        let today = rolling
            .iter()
            .find(|name| !name.starts_with("codexmux.2020-"))
            .expect("today's log file");
        let date = today
            .strip_prefix("codexmux.")
            .and_then(|name| name.strip_suffix(".log"))
            .unwrap();
        assert_eq!(date.len(), "YYYY-MM-DD".len());
        assert_eq!(
            std::fs::read_to_string(logs.join(today)).unwrap(),
            "started\n"
        );
    }

    #[test]
    fn flags_accept_only_boolean_words() {
        assert!(parse_flag("true").unwrap());
        assert!(!parse_flag(" no ").unwrap());
        assert!(parse_flag("maybe").is_err());
    }
}
