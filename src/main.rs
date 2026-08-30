use std::{future::Future, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use modelmux::{
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
    /// Stop the CPA LaunchAgent.
    Stop,
    /// Show the installed version and service state.
    Status,
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
        /// Client token for this endpoint (stored in the mode-0600 profile file).
        #[arg(long)]
        token: String,
    },
    /// Remove a saved profile.
    ProfileRemove {
        /// Profile name.
        name: String,
    },
    /// Show the review model override (None = official first, CPA fallback).
    ReviewGet,
    /// Route `cpa/<slug>` requests directly to an upstream, bypassing CPA.
    DirectSet {
        /// Comma-separated cpa/ model slugs (without the cpa/ prefix).
        models: String,
        /// Upstream Responses API base URL.
        #[arg(long)]
        base_url: String,
        /// Bearer token for the upstream.
        #[arg(long)]
        token: String,
    },
    /// Remove all direct routes (everything goes through CPA again).
    DirectClear,
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
                .unwrap_or_else(|_| "modelmux=info".into()),
        )
        .init();
    let paths = Paths::discover()?;
    match Cli::parse().command {
        Command::Init => init(&paths),
        Command::Serve { no_codex_config } => serve(&paths, no_codex_config).await,
        Command::Status => status(&paths),
        Command::Doctor => doctor(&paths).await,
        Command::Install => {
            let settings = Settings::load(&paths.settings)?;
            // Enable the Codex configuration from the caller's context: the
            // LaunchAgent runs serve --no-codex-config because background
            // processes may be denied access to the Codex config's volume by
            // macOS TCC, which blocks open() indefinitely.
            let manager = config_manager(&paths)?;
            let lease = manager.enable(&format!("http://{}/v1", settings.listen))?;
            let executable = std::env::current_exe()?.canonicalize()?;
            let install_result = launch_agent::install(&paths, &executable, &codex_config_path()?)
                .map(|plist| {
                    println!("installed {}", plist.display());
                });
            // Keep the configuration enabled only when the agent was installed.
            match install_result {
                Ok(()) => {
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
            },
        )?;
    }
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

async fn serve(paths: &Paths, no_codex_config: bool) -> Result<()> {
    let settings = Settings::load(&paths.settings)?;
    let credentials = secrets::load(&paths.credentials)?;
    let listener = tokio::net::TcpListener::bind(settings.listen)
        .await
        .with_context(|| format!("failed to bind {}", settings.listen))?;
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
        let proxy = server::serve(listener, state, async move {
            let _ = stopped.await;
        });
        tokio::pin!(proxy);
        tokio::select! {
            result = &mut proxy => result,
            _ = shutdown => {
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
    Ok(())
}

fn status(paths: &Paths) -> Result<()> {
    println!("root: {}", paths.root.display());
    println!("settings: {}", paths.settings.display());
    println!("credentials: {}", paths.credentials.display());
    println!("catalog: {}", paths.catalog.display());
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
        std::env::var(PROXY_TOKEN_ENV).as_deref() == Ok(credentials.proxy_token.as_str()),
        "{PROXY_TOKEN_ENV} is missing or does not match credentials.json"
    );
    let status = config_manager(paths)?.status()?;
    match reqwest::Client::new()
        .get(format!("http://{}/health", settings.listen))
        .header("x-modelmux-token", &credentials.proxy_token)
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
                Some(archive) => modelmux::cpa::install_from_archive(
                    paths,
                    &settings.cpa,
                    &archive,
                    modelmux::cpa::CPA_VERSION,
                    modelmux::cpa::CPA_DARWIN_AARCH64_SHA256,
                    &credentials.cpa_token,
                )?,
                None => modelmux::cpa::install(paths, &settings.cpa, &credentials.cpa_token)?,
            }
            println!(
                "CLIProxyAPI {} installed at {}",
                modelmux::cpa::CPA_VERSION,
                modelmux::cpa::binary_path(paths).display()
            );
            println!("config: {}", modelmux::cpa::config_path(paths).display());
            println!(
                "service: started ({}); logs: {}/logs/cpa-*.log",
                modelmux::cpa::agent_label(),
                paths.root.display()
            );
        }
        CpaCommand::Start => {
            let credentials = secrets::load(&paths.credentials)?;
            modelmux::cpa::start(paths, &settings.cpa, &credentials.cpa_token)?;
            println!("CPA service started");
        }
        CpaCommand::Stop => {
            modelmux::cpa::stop()?;
            println!("CPA service stopped");
        }
        CpaCommand::Status => {
            match modelmux::cpa::installed_version(paths) {
                Some(version) => {
                    println!("version: {} (sha256 {})", version.version, version.sha256);
                }
                None => println!("version: not installed"),
            }
            println!(
                "binary: {}",
                if modelmux::cpa::binary_path(paths).is_file() {
                    "installed"
                } else {
                    "missing"
                }
            );
            println!(
                "service: {}",
                if modelmux::cpa::is_loaded()? {
                    "running"
                } else {
                    "stopped"
                }
            );
            println!("config: {}", modelmux::cpa::config_path(paths).display());
        }
        CpaCommand::ProviderImport { file } => {
            let credentials = secrets::load(&paths.credentials)?;
            let providers = std::fs::read_to_string(&file)
                .with_context(|| format!("failed to read {}", file.display()))?;
            modelmux::cpa::import_providers(paths, &providers)?;
            modelmux::cpa::restart(paths, &settings.cpa, &credentials.cpa_token)?;
            println!("imported providers from {}", file.display());
            println!(
                "CPA service restarted with the new providers; config: {}",
                modelmux::cpa::config_path(paths).display()
            );
        }
        CpaCommand::Uninstall => {
            match modelmux::cpa::uninstall()? {
                Some(plist) => println!("removed {}", plist.display()),
                None => println!("CPA service is not installed"),
            }
            println!(
                "kept binary and config under {}",
                paths.root.join("cpa").display()
            );
        }
        CpaCommand::ProfileList => {
            let (active, profiles) = modelmux::cpa::profiles(paths);
            match active {
                Some(active) => println!("active: {active}"),
                None => println!("active: (none; using config.toml settings)"),
            }
            if profiles.is_empty() {
                println!("no saved profiles; add one with `modelmux cpa profile-save`");
            }
            for profile in profiles {
                println!("  {} — {}", profile.name, profile.base_url);
            }
        }
        CpaCommand::ProfileSave {
            name,
            base_url,
            token,
        } => {
            modelmux::cpa::save_profile(
                paths,
                modelmux::cpa::CpaProfile {
                    name,
                    base_url,
                    token,
                },
            )?;
            println!("profile saved to {}", paths.cpa_profiles.display());
        }
        CpaCommand::ProfileRemove { name } => {
            modelmux::cpa::remove_profile(paths, &name)?;
            println!("profile {name} removed");
        }
        CpaCommand::ProfileSwitch { name } => {
            modelmux::cpa::switch_profile(paths, &name)?;
            println!("switched to profile {name}");
        }
        CpaCommand::ReviewGet => match modelmux::cpa::review_override(&paths.cpa_profiles) {
            Some(slug) => println!("review override: {slug}"),
            None => println!("review override: (none; official first, CPA fallback)"),
        },
        CpaCommand::DirectSet {
            models,
            base_url,
            token,
        } => {
            let slugs: Vec<String> = models
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            anyhow::ensure!(!slugs.is_empty(), "no model slugs given");
            for slug in &slugs {
                modelmux::cpa::direct_route_for(&paths.cpa_profiles, slug);
            }
            modelmux::cpa::set_direct_routes(
                paths,
                vec![modelmux::cpa::DirectRoute {
                    base_url,
                    token,
                    models: slugs,
                }],
            )?;
            println!("direct route configured; see modelmux cpa direct-list");
        }
        CpaCommand::DirectClear => {
            modelmux::cpa::set_direct_routes(paths, Vec::new())?;
            println!("direct routes cleared");
        }
        CpaCommand::ReviewSet { slug } => {
            let slug = slug.trim().to_owned();
            if slug.is_empty() {
                modelmux::cpa::set_review_override(&paths.cpa_profiles, None)?;
                println!("review override cleared");
            } else {
                modelmux::cpa::set_review_override(&paths.cpa_profiles, Some(slug.clone()))?;
                println!("codex-auto-review now routes directly to {slug}");
            }
        }
    }
    Ok(())
}

fn codex_config_path() -> Result<PathBuf> {
    modelmux::codex_config::codex_config_path()
}
