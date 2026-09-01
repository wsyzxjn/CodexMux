use std::{future::Future, path::PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use codexmux::{
    codex_config::{ConfigLease, ConfigManager, PROXY_TOKEN_ENV},
    config::{Credentials, Paths, Settings},
    launch_agent, secrets,
    server::{self, AppState},
};

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
    Status,
    /// Validate config, credentials, snapshot, and the managed Codex block.
    Doctor,
    /// Install and start the per-user macOS LaunchAgent.
    Install,
    /// Stop the LaunchAgent and restore Codex configuration.
    Uninstall,
    /// Manage the local CLIProxyAPI instance that serves CPA models.
    Cpa {
        #[command(subcommand)]
        command: CpaCommand,
    },
}

#[derive(Subcommand)]
enum CpaCommand {
    /// Download, verify, and install the pinned CLIProxyAPI release, then start it.
    Install {
        /// Install from a local release archive instead of downloading it.
        #[arg(long)]
        archive: Option<PathBuf>,
    },
    /// Start the CPA LaunchAgent (installs the managed config).
    Start,
    /// Bring the CPA service in line with the saved startup preference:
    /// start it when enabled, stop it when disabled, leave it untouched
    /// when no preference exists.
    SyncStart,
    /// Stop the CPA LaunchAgent.
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
        /// For loopback CPA, attach the current endpoint and management key
        /// so the Web UI can auto-connect. Remote URLs stay unmodified.
        #[arg(long)]
        connect: bool,
    },
    /// Print the CPA web management key for explicit user handoff.
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
    /// Show the review model override (None = official route).
    ReviewGet,
    /// Route `cpa/<slug>` requests directly to an upstream, bypassing CPA.
    DirectSet {
        /// Comma-separated cpa/ model slugs (without the cpa/ prefix).
        models: String,
        /// Upstream Responses API base URL.
        #[arg(long)]
        base_url: String,
        /// Environment variable containing the direct upstream token.
        #[arg(long, default_value = "CODEXMUX_DIRECT_TOKEN")]
        token_env: String,
        /// Native upstream model id when it differs from the local slug.
        /// Requires exactly one local model.
        #[arg(long)]
        upstream_model: Option<String>,
    },
    /// Add models to a direct upstream, merging with an existing route.
    DirectAdd {
        /// Comma-separated local model slugs (without the cpa/ prefix).
        models: String,
        /// Upstream Responses API base URL.
        #[arg(long)]
        base_url: String,
        /// Environment variable containing the direct upstream token.
        #[arg(long, default_value = "CODEXMUX_DIRECT_TOKEN")]
        token_env: String,
        /// Native upstream model id when it differs from the local slug.
        /// Requires exactly one local model.
        #[arg(long)]
        upstream_model: Option<String>,
    },
    /// Remove models (or a whole route) from direct upstreams.
    DirectRemove {
        /// Upstream Responses API base URL of the route.
        #[arg(long)]
        base_url: String,
        /// Comma-separated upstream model slugs to remove; empty removes the
        /// whole route.
        models: Option<String>,
    },
    /// List direct routes without printing their tokens.
    DirectList,
    /// Remove all direct routes (everything goes through CPA again).
    DirectClear,
    /// Show whether the CPA service should start with CodexMux.
    AutostartGet,
    /// Set whether the CPA service should start with CodexMux.
    AutostartSet {
        /// `true`/`false`: whether CPA starts together with CodexMux.
        enabled: String,
    },
    /// Route `codex-auto-review` straight to a CPA model (empty to clear).
    ReviewSet {
        /// Upstream CPA model slug; empty string clears the override.
        slug: String,
    },
    /// Validate a saved endpoint and switch to it; rolls back on failure.
    ProfileSwitch {
        /// Profile name.
        name: String,
    },
    /// Remove the CPA LaunchAgent (keeps the binary, config, and auth files).
    Uninstall,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "codexmux=info".into()),
        )
        .init();
    let paths = Paths::discover()?;
    match Cli::parse().command {
        Command::Init => init(&paths),
        Command::Serve {
            no_codex_config,
            launchd_socket,
        } => serve(&paths, no_codex_config, launchd_socket).await,
        Command::Status => status(&paths),
        Command::Doctor => doctor(&paths).await,
        Command::Install => {
            ensure_initialized(&paths)?;
            let settings = Settings::load(&paths.settings)?;
            let credentials = secrets::load(&paths.credentials)?;
            // Enable the Codex configuration from the caller's context: the
            // LaunchAgent runs serve --no-codex-config because background
            // processes may be denied access to the Codex config's volume by
            // macOS TCC, which blocks open() indefinitely.
            let manager = config_manager(&paths)?;
            let lease = manager.enable(&format!("http://{}/v1", settings.listen))?;
            let executable = std::env::current_exe()?.canonicalize()?;
            let install_result =
                launch_agent::install(&paths, &executable, &codex_config_path()?, settings.listen);
            // Keep the configuration enabled only when the agent was installed.
            match install_result {
                Ok(plist) => {
                    if let Err(error) = set_launchctl_proxy_token(&credentials.proxy_token) {
                        launch_agent::uninstall().ok();
                        lease.restore().ok();
                        return Err(error).context(
                            "failed to prepare the Codex environment; installation rolled back",
                        );
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
        Command::Uninstall => uninstall(&paths),
        Command::Cpa { command } => cpa(&paths, command),
    }
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

fn set_launchctl_proxy_token(token: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("launchctl")
            .args(["setenv", PROXY_TOKEN_ENV, token])
            .status()
            .context("failed to run launchctl setenv")?;
        anyhow::ensure!(
            status.success(),
            "launchctl setenv {PROXY_TOKEN_ENV} failed with {status}"
        );
    }
    #[cfg(not(target_os = "macos"))]
    let _ = token;
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
    let lifecycle_manages_cpa =
        launchd_socket && codexmux::cpa::cpa_autostart(&paths.cpa_profiles) == Some(true);
    if lifecycle_manages_cpa && !codexmux::cpa::is_loaded()? {
        codexmux::cpa::start(paths, &settings.cpa, &credentials.cpa_token)?;
        tracing::info!("CPA started for active Codex client");
    }
    let state = AppState::new(
        settings.clone(),
        credentials,
        paths.catalog.clone(),
        paths.cpa_profiles.clone(),
    )?;
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
    if lifecycle_manages_cpa && codexmux::cpa::is_loaded()? {
        if let Err(error) = codexmux::cpa::stop_service_only() {
            tracing::warn!(%error, "failed to stop lifecycle-managed CPA");
        } else {
            tracing::info!("CPA stopped after Codex client became idle");
        }
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

fn shutdown_signal() -> Result<impl Future<Output = ()> + Send + 'static> {
    #[cfg(unix)]
    {
        let mut interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .context("failed to install SIGINT handler")?;
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .context("failed to install SIGTERM handler")?;
        let mut hangup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .context("failed to install SIGHUP handler")?;
        Ok(async move {
            tokio::select! {
                _ = interrupt.recv() => {},
                _ = terminate.recv() => {},
                _ = hangup.recv() => {},
            }
        })
    }
    #[cfg(not(unix))]
    {
        Ok(async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl-C handler");
        })
    }
}

fn uninstall(paths: &Paths) -> Result<()> {
    match launch_agent::uninstall()? {
        Some(path) => println!("removed {}", path.display()),
        None => println!("LaunchAgent is not installed"),
    }
    let manager = config_manager(paths)?;
    let was_managed = paths.state.exists();
    manager.disable()?;
    if was_managed {
        println!("restored Codex configuration");
    }
    unset_launchctl_proxy_token()?;
    Ok(())
}

fn unset_launchctl_proxy_token() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("launchctl")
            .args(["unsetenv", PROXY_TOKEN_ENV])
            .status()
            .context("failed to run launchctl unsetenv")?;
        anyhow::ensure!(
            status.success(),
            "launchctl unsetenv {PROXY_TOKEN_ENV} failed with {status}"
        );
    }
    Ok(())
}

fn status(paths: &Paths) -> Result<()> {
    println!("root: {}", paths.root.display());
    println!("settings: {}", paths.settings.display());
    println!("credentials: {}", paths.credentials.display());
    println!("catalog: {}", paths.catalog.display());
    println!(
        "proxy service: {}",
        match launch_agent::runtime_state()? {
            launch_agent::RuntimeState::Running => "running",
            launch_agent::RuntimeState::Idle => "idle",
            launch_agent::RuntimeState::NotInstalled => "not installed",
        }
    );
    let status = config_manager(paths)?.status()?;
    println!("enabled: {}", status.enabled);
    println!("codex config: {}", status.config_path.display());
    println!("config unchanged: {}", status.unchanged_since_enable);
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
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", credentials.cpa_token),
        )
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
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("launchctl")
            .args(["getenv", PROXY_TOKEN_ENV])
            .output()
            .is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).trim() == expected
            })
    }
    #[cfg(not(target_os = "macos"))]
    false
}

fn config_manager(paths: &Paths) -> Result<ConfigManager> {
    Ok(ConfigManager::new(
        codex_config_path()?,
        paths.state.clone(),
        paths.backups.clone(),
    ))
}

fn cpa(paths: &Paths, command: CpaCommand) -> Result<()> {
    let settings = Settings::load(&paths.settings)?;
    match command {
        CpaCommand::Install { archive } => {
            let credentials = secrets::load(&paths.credentials)?;
            match archive {
                Some(archive) => codexmux::cpa::install_from_archive(
                    paths,
                    &settings.cpa,
                    &archive,
                    codexmux::cpa::CPA_VERSION,
                    codexmux::cpa::CPA_DARWIN_AARCH64_SHA256,
                    &credentials.cpa_token,
                )?,
                None => codexmux::cpa::install(paths, &settings.cpa, &credentials.cpa_token)?,
            }
            println!(
                "CLIProxyAPI {} installed at {}",
                codexmux::cpa::CPA_VERSION,
                codexmux::cpa::binary_path(paths).display()
            );
            println!("config: {}", codexmux::cpa::config_path(paths).display());
            println!(
                "service: started ({}); logs: {}/logs/cpa-*.log",
                codexmux::cpa::agent_label(),
                paths.root.display()
            );
        }
        CpaCommand::Start => {
            let credentials = secrets::load(&paths.credentials)?;
            codexmux::cpa::start(paths, &settings.cpa, &credentials.cpa_token)?;
            println!("CPA service started");
        }
        CpaCommand::SyncStart => match codexmux::cpa::cpa_autostart(&paths.cpa_profiles) {
            Some(true) => {
                if !codexmux::cpa::is_loaded()? {
                    let credentials = secrets::load(&paths.credentials)?;
                    codexmux::cpa::start(paths, &settings.cpa, &credentials.cpa_token)?;
                    println!("CPA service started (startup preference: enabled)");
                } else {
                    println!("CPA service already running (startup preference: enabled)");
                }
            }
            Some(false) => {
                if codexmux::cpa::is_loaded()? {
                    codexmux::cpa::stop_service_only()?;
                    println!("CPA service stopped (startup preference: disabled)");
                } else {
                    println!("CPA service already stopped (startup preference: disabled)");
                }
            }
            None => println!("CPA startup preference not set; service left as-is"),
        },
        CpaCommand::Stop { no_preference } => {
            if no_preference {
                codexmux::cpa::stop_service_only()?;
            } else {
                codexmux::cpa::stop(paths)?;
            }
            println!("CPA service stopped");
        }
        CpaCommand::Status => {
            match codexmux::cpa::installed_version(paths) {
                Some(version) => {
                    println!("version: {} (sha256 {})", version.version, version.sha256);
                }
                None => println!("version: not installed"),
            }
            println!(
                "binary: {}",
                if codexmux::cpa::binary_path(paths).is_file() {
                    "installed"
                } else {
                    "missing"
                }
            );
            println!(
                "service: {}",
                if codexmux::cpa::is_loaded()? {
                    "running"
                } else {
                    "stopped"
                }
            );
            println!(
                "autostart: {}",
                match codexmux::cpa::cpa_autostart(&paths.cpa_profiles) {
                    Some(true) => "enabled",
                    Some(false) => "disabled",
                    None => "not set",
                }
            );
            println!("config: {}", codexmux::cpa::config_path(paths).display());
        }
        CpaCommand::ModelList => {
            let credentials = secrets::load(&paths.credentials)?;
            for slug in codexmux::cpa::model_slugs(&settings.cpa, &credentials.cpa_token)? {
                println!("{slug}");
            }
        }
        CpaCommand::ManagementUrl { connect } => {
            if connect && settings.cpa.is_loopback() {
                let management_key = secrets::load(&paths.credentials)?.cpa_management_key;
                codexmux::cpa::sync_management_key(paths, &management_key)?;
                codexmux::cpa::ensure_management_connect_bootstrap(paths)?;
                println!("{}", settings.cpa.management_connect_url(&management_key)?);
            } else {
                println!("{}", settings.cpa.management_url()?);
            }
        }
        CpaCommand::ManagementKey => {
            let management_key = secrets::load(&paths.credentials)?.cpa_management_key;
            if settings.cpa.is_loopback() {
                codexmux::cpa::sync_management_key(paths, &management_key)?;
            }
            println!("management-key: {management_key}");
        }
        CpaCommand::ProviderImport { file } => {
            let credentials = secrets::load(&paths.credentials)?;
            let providers = std::fs::read_to_string(&file)
                .with_context(|| format!("failed to read {}", file.display()))?;
            codexmux::cpa::import_providers(paths, &providers)?;
            codexmux::cpa::restart(paths, &settings.cpa, &credentials.cpa_token)?;
            println!("imported providers from {}", file.display());
            println!(
                "CPA service restarted with the new providers; config: {}",
                codexmux::cpa::config_path(paths).display()
            );
        }
        CpaCommand::Uninstall => {
            match codexmux::cpa::uninstall()? {
                Some(plist) => println!("removed {}", plist.display()),
                None => println!("CPA service is not installed"),
            }
            println!(
                "kept binary and config under {}",
                paths.root.join("cpa").display()
            );
        }
        CpaCommand::ProfileList => {
            let (active, profiles) = codexmux::cpa::profiles(paths);
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
            codexmux::cpa::save_profile(
                paths,
                codexmux::cpa::CpaProfile {
                    name,
                    base_url,
                    token,
                },
            )?;
            println!("profile saved to {}", paths.cpa_profiles.display());
        }
        CpaCommand::ProfileRemove { name } => {
            codexmux::cpa::remove_profile(paths, &name)?;
            println!("profile {name} removed");
        }
        CpaCommand::ProfileSwitch { name } => {
            codexmux::cpa::switch_profile(paths, &name)?;
            println!("switched to profile {name}");
        }
        CpaCommand::ReviewGet => match codexmux::cpa::review_override(&paths.cpa_profiles) {
            Some(slug) => println!("review override: {slug}"),
            None => println!("review override: (none; official route)"),
        },
        CpaCommand::DirectSet {
            models,
            base_url,
            token_env,
            upstream_model,
        } => {
            let token = std::env::var(&token_env)
                .with_context(|| format!("{token_env} must contain the direct route token"))?;
            let slugs: Vec<String> = models
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            anyhow::ensure!(!slugs.is_empty(), "no model slugs given");
            let configured_models = slugs.join(", ");
            let (models, model_aliases) = match upstream_model {
                Some(upstream) => {
                    anyhow::ensure!(
                        slugs.len() == 1,
                        "--upstream-model requires exactly one local model"
                    );
                    (
                        Vec::new(),
                        std::collections::BTreeMap::from([(slugs[0].clone(), upstream)]),
                    )
                }
                None => (slugs, std::collections::BTreeMap::new()),
            };
            codexmux::cpa::set_direct_routes(
                paths,
                vec![codexmux::cpa::DirectRoute {
                    base_url,
                    token,
                    models,
                    model_aliases,
                }],
            )?;
            println!("direct route configured for {configured_models}");
        }
        CpaCommand::DirectAdd {
            models,
            base_url,
            token_env,
            upstream_model,
        } => {
            let token = std::env::var(&token_env)
                .with_context(|| format!("{token_env} must contain the direct route token"))?;
            let slugs: Vec<String> = models
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            anyhow::ensure!(!slugs.is_empty(), "no model slugs given");
            if let Some(upstream) = upstream_model {
                anyhow::ensure!(
                    slugs.len() == 1,
                    "--upstream-model requires exactly one local model"
                );
                codexmux::cpa::add_direct_route_mapping(
                    paths,
                    base_url,
                    token,
                    slugs[0].clone(),
                    upstream,
                )?;
            } else {
                codexmux::cpa::add_direct_route(paths, base_url, token, slugs.clone())?;
            }
            println!("direct route updated for {}", slugs.join(", "));
        }
        CpaCommand::DirectRemove { base_url, models } => {
            let slugs: Vec<String> = models
                .map(|models| {
                    models
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            codexmux::cpa::remove_direct_routes(paths, &base_url, &slugs)?;
            if slugs.is_empty() {
                println!("direct route removed for {base_url}");
            } else {
                println!("removed {} from {base_url}", slugs.join(", "));
            }
        }
        CpaCommand::DirectList => {
            for route in codexmux::cpa::direct_routes(paths) {
                let models: Vec<&str> = route
                    .models
                    .iter()
                    .map(String::as_str)
                    .chain(route.model_aliases.keys().map(String::as_str))
                    .collect();
                println!("{} -> {}", models.join(", "), route.base_url);
            }
        }
        CpaCommand::DirectClear => {
            codexmux::cpa::set_direct_routes(paths, Vec::new())?;
            println!("direct routes cleared");
        }
        CpaCommand::AutostartGet => match codexmux::cpa::cpa_autostart(&paths.cpa_profiles) {
            Some(true) => println!("cpa autostart: enabled"),
            Some(false) => println!("cpa autostart: disabled"),
            None => println!("cpa autostart: (not set; CPA stays as-is)"),
        },
        CpaCommand::AutostartSet { enabled } => {
            let enabled = match enabled.trim() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                other => bail!("enabled must be true or false, got {other:?}"),
            };
            codexmux::cpa::set_cpa_autostart(&paths.cpa_profiles, enabled)?;
            println!(
                "cpa autostart: {}",
                if enabled { "enabled" } else { "disabled" }
            );
        }
        CpaCommand::ReviewSet { slug } => {
            let slug = slug.trim().to_owned();
            if slug.is_empty() {
                codexmux::cpa::set_review_override(&paths.cpa_profiles, None)?;
                println!("review override cleared");
            } else {
                codexmux::cpa::set_review_override(&paths.cpa_profiles, Some(slug.clone()))?;
                println!("codex-auto-review now routes directly to {slug}");
            }
        }
    }
    Ok(())
}

fn codex_config_path() -> Result<PathBuf> {
    codexmux::codex_config::codex_config_path()
}

#[cfg(test)]
mod bootstrap_tests {
    use super::*;

    #[test]
    fn initialization_creates_private_state_and_preserves_it() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(root.path().join("CodexMux"));

        ensure_initialized(&paths).unwrap();
        let first_settings = std::fs::read(&paths.settings).unwrap();
        let first_credentials = std::fs::read(&paths.credentials).unwrap();
        let credentials = secrets::load(&paths.credentials).unwrap();
        credentials.validate().unwrap();

        ensure_initialized(&paths).unwrap();
        assert_eq!(std::fs::read(&paths.settings).unwrap(), first_settings);
        assert_eq!(
            std::fs::read(&paths.credentials).unwrap(),
            first_credentials
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&paths.credentials)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
