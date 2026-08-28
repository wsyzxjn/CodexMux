use std::path::PathBuf;

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
    /// Run the loopback proxy in the foreground.
    Serve,
    /// Point Codex at ModelMux's dynamic model catalog and Responses proxy.
    Enable,
    /// Restore the Codex configuration ModelMux previously changed.
    Disable,
    /// Show paths and managed configuration state.
    Status,
    /// Validate config, credentials, snapshot, and the managed Codex block.
    Doctor,
    /// Install and start the per-user macOS LaunchAgent.
    Install,
    /// Stop and remove the per-user macOS LaunchAgent.
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
        Command::Serve => {
            let settings = Settings::load(&paths.settings)?;
            let credentials = secrets::load(&paths.credentials)?;
            server::serve(AppState::new(settings, credentials, paths.catalog.clone())?).await
        }
        Command::Enable => enable(&paths),
        Command::Disable => config_manager(&paths)?.disable(),
        Command::Status => status(&paths),
        Command::Doctor => doctor(&paths).await,
        Command::Install => {
            let executable = std::env::current_exe()?.canonicalize()?;
            let plist = launch_agent::install(&paths, &executable)?;
            println!("installed {}", plist.display());
            Ok(())
        }
        Command::Uninstall => {
            match launch_agent::uninstall()? {
                Some(path) => println!("removed {}", path.display()),
                None => println!("LaunchAgent is not installed"),
            }
            Ok(())
        }
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

fn enable(paths: &Paths) -> Result<()> {
    let settings = Settings::load(&paths.settings)?;
    secrets::load(&paths.credentials)?;
    config_manager(paths)?.enable(&format!("http://{}/v1", settings.listen))?;
    println!("enabled ModelMux in {}", codex_config_path()?.display());
    println!("catalog: http://{}/v1/models", settings.listen);
    println!("proxy: http://{}/v1", settings.listen);
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
