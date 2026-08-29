use std::{future::Future, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use modelmux::{
    codex_config::{ConfigManager, PROXY_TOKEN_ENV},
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
    /// Run the proxy in the foreground and manage Codex configuration.
    Serve,
    /// Show paths and managed configuration state.
    Status,
    /// Validate config, credentials, snapshot, and the managed Codex block.
    Doctor,
    /// Install and start the per-user macOS LaunchAgent.
    Install,
    /// Stop the LaunchAgent and restore Codex configuration.
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
        Command::Serve => serve(&paths).await,
        Command::Status => status(&paths),
        Command::Doctor => doctor(&paths).await,
        Command::Install => {
            let executable = std::env::current_exe()?.canonicalize()?;
            let plist = launch_agent::install(&paths, &executable, &codex_config_path()?)?;
            println!("installed {}", plist.display());
            Ok(())
        }
        Command::Uninstall => uninstall(&paths),
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

async fn serve(paths: &Paths) -> Result<()> {
    let settings = Settings::load(&paths.settings)?;
    let credentials = secrets::load(&paths.credentials)?;
    let listener = tokio::net::TcpListener::bind(settings.listen)
        .await
        .with_context(|| format!("failed to bind {}", settings.listen))?;
    let state = AppState::new(settings.clone(), credentials, paths.catalog.clone())?;
    let shutdown = shutdown_signal()?;
    let manager = config_manager(paths)?;
    let lease = manager.enable(&format!("http://{}/v1", settings.listen))?;
    tracing::info!(config = %codex_config_path()?.display(), "Codex configuration enabled");

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
    let restore = lease.restore();
    match (result, restore) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
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
    let cpa_base = settings.cpa.base_url.trim_end_matches('/');
    let cpa_health = format!(
        "{}/health",
        cpa_base.strip_suffix("/v1").unwrap_or(cpa_base)
    );
    match reqwest::Client::new()
        .get(cpa_health)
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

fn codex_config_path() -> Result<PathBuf> {
    modelmux::codex_config::codex_config_path()
}
