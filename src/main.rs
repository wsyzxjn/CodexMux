use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use modelmux::{
    catalog,
    codex_config::{ConfigManager, ManagedConfig, PROXY_TOKEN_ENV},
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
    /// Merge the current bundled catalog and point Codex at ModelMux.
    Enable,
    /// Restore the Codex configuration ModelMux previously changed.
    Disable,
    /// Show paths and managed configuration state.
    Status,
    /// Validate config, credentials, catalog, and the managed Codex block.
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
            let codex = codex_binary()?;
            let bundled = catalog::query_bundled_catalog(&codex)?;
            let official_models = catalog::model_slugs(&bundled)?;
            server::serve(AppState::new(settings, credentials, official_models)?).await
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
                schema_version: 1,
                proxy_token: uuid::Uuid::new_v4().simple().to_string(),
                providers: HashMap::new(),
            },
        )?;
    }
    println!("initialized {}", paths.root.display());
    println!("edit {} to add providers", paths.settings.display());
    println!(
        "edit {} to add provider credentials",
        paths.credentials.display()
    );
    println!("set {PROXY_TOKEN_ENV} from credentials.json before starting Codex");
    Ok(())
}

fn enable(paths: &Paths) -> Result<()> {
    let settings = Settings::load(&paths.settings)?;
    secrets::load(&paths.credentials)?;
    let codex = codex_binary()?;
    let base = catalog::query_bundled_catalog(&codex)?;
    let merged = catalog::merge(&base, &settings)?;
    catalog::save(&paths.catalog, &merged)?;
    config_manager(paths)?.enable(&ManagedConfig {
        catalog_path: paths.catalog.clone(),
        loopback_base_url: format!("http://{}/v1", settings.listen),
    })?;
    println!("enabled ModelMux in {}", codex_config_path()?.display());
    println!("catalog: {}", paths.catalog.display());
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
    for provider in settings.providers.iter().filter(|provider| {
        provider.enabled && provider.kind == modelmux::config::ProviderKind::External
    }) {
        anyhow::ensure!(
            credentials.providers.contains_key(&provider.id),
            "provider {} has no credential",
            provider.id
        );
    }
    let codex = codex_binary().context("codex binary check failed")?;
    let bundled = catalog::query_bundled_catalog(&codex).context("bundled catalog check failed")?;
    catalog::merge(&bundled, &settings).context("catalog merge check failed")?;
    let status = config_manager(paths)?.status()?;
    if status.enabled {
        anyhow::ensure!(paths.catalog.is_file(), "managed catalog is missing");
    }
    match reqwest::Client::new()
        .get(format!("http://{}/health", settings.listen))
        .header("x-modelmux-token", &credentials.proxy_token)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => println!("proxy: reachable"),
        _ => println!("proxy: not running"),
    }
    println!("settings: ok");
    println!("credentials: ok");
    println!("bundled catalog: ok ({})", codex.display());
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

fn codex_binary() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_BINARY") {
        let path = PathBuf::from(path);
        anyhow::ensure!(path.is_file(), "CODEX_BINARY does not point to a file");
        return Ok(path);
    }
    let bundled = Path::new("/Applications/ChatGPT.app/Contents/Resources/codex");
    if bundled.is_file() {
        return Ok(bundled.to_path_buf());
    }
    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    if let Some(path) = std::env::var_os("PATH").as_deref().and_then(|search| {
        std::env::split_paths(search)
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
    }) {
        return Ok(path);
    }
    anyhow::bail!("cannot find codex; set CODEX_BINARY")
}
