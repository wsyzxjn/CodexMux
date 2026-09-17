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
    Status {
        /// Skip reading the managed Codex configuration. Used by the menu bar
        /// status poll so it never touches a TCC-gated external volume.
        #[arg(long)]
        no_codex_config: bool,
    },
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
    /// Show the image route override (None = official route).
    ImageGet,
    /// List image model slugs the configured CPA endpoint reports.
    ImageModelList,
    /// Show the shared Responses web search backend override.
    SearchGet,
    /// Select the shared web search backend. Use `default` to follow
    /// `config.toml`, or an empty slug to disable the menu override.
    SearchSet {
        /// `default`, empty, or a merged catalog model slug.
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
        /// Catalog context window for these local slugs.
        #[arg(long)]
        context_window: Option<u64>,
        /// Catalog max context window for these local slugs.
        #[arg(long)]
        max_context_window: Option<u64>,
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
        /// Catalog context window for these local slugs.
        #[arg(long)]
        context_window: Option<u64>,
        /// Catalog max context window for these local slugs.
        #[arg(long)]
        max_context_window: Option<u64>,
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
    /// Set catalog metadata for an existing direct-route model.
    DirectMetadataSet {
        /// cpa/ model slug (without the cpa/ prefix).
        model: String,
        /// Catalog context window.
        #[arg(long)]
        context_window: Option<u64>,
        /// Catalog max context window.
        #[arg(long)]
        max_context_window: Option<u64>,
        /// Picker display name; CodexMux appends ` · Direct`.
        #[arg(long)]
        display_name: Option<String>,
    },
    /// Remove all direct routes (everything goes through CPA again).
    DirectClear,
    /// List model ids exposed by a direct Responses endpoint.
    DirectDiscover {
        /// Upstream Responses API base URL.
        #[arg(long)]
        base_url: String,
        /// Environment variable containing the direct upstream token.
        #[arg(long, default_value = "CODEXMUX_DIRECT_TOKEN")]
        token_env: String,
    },
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
    /// Route Codex image generation to a CPA image model (empty to clear).
    ImageSet {
        /// Upstream CPA image model slug; empty string clears the override.
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

fn main() -> Result<()> {
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
        } => run_async(serve(&paths, no_codex_config, launchd_socket)),
        Command::Status { no_codex_config } => status(&paths, no_codex_config),
        Command::Doctor => run_async(doctor(&paths)),
        Command::Install => {
            ensure_initialized(&paths)?;
            let settings = Settings::load(&paths.settings)?;
            let credentials = secrets::load(&paths.credentials)?;
            // Enable the Codex configuration from the caller's context: the
            // LaunchAgent runs serve --no-codex-config because background
            // processes may be denied access to the Codex config's volume by
            // macOS TCC, which blocks open() indefinitely.
            let manager = config_manager(&paths)?;
            let lease = manager
                .enable(&format!("http://{}/v1", settings.listen))
                .context("failed to enable the managed Codex configuration")?;
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
        Command::Catalog { command } => catalog(&paths, command),
    }
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

fn catalog(paths: &Paths, command: CatalogCommand) -> Result<()> {
    let mut settings = Settings::load(&paths.settings)?;
    match command {
        CatalogCommand::UltraGet => {
            println!("ultra: {}", settings.catalog.advertise_ultra);
            Ok(())
        }
        CatalogCommand::UltraSet { enabled } => {
            let enabled = enabled
                .parse::<bool>()
                .context("enabled must be true or false")?;
            settings.catalog.advertise_ultra = enabled;
            settings.save(&paths.settings)?;
            println!("ultra: {}", enabled);
            Ok(())
        }
        CatalogCommand::CompHashGet => {
            println!("unify-comp-hash: {}", settings.catalog.unify_comp_hash);
            Ok(())
        }
        CatalogCommand::CompHashSet { enabled } => {
            let enabled = enabled
                .parse::<bool>()
                .context("enabled must be true or false")?;
            settings.catalog.unify_comp_hash = enabled;
            settings.save(&paths.settings)?;
            println!("unify-comp-hash: {}", enabled);
            Ok(())
        }
        CatalogCommand::Models => {
            let store = codexmux::catalog::CatalogStore::load(paths.catalog.clone())?;
            let catalog = store
                .current()
                .context("model catalog snapshot has not been built yet")?;
            let models = catalog
                .get("models")
                .and_then(serde_json::Value::as_array)
                .context("model catalog has no models array")?;
            for model in models {
                if let Some(slug) = model.get("slug").and_then(serde_json::Value::as_str) {
                    println!("{slug}");
                }
            }
            Ok(())
        }
    }
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
    let mut launched_cpa = false;
    if lifecycle_manages_cpa && !codexmux::cpa::is_loaded()? {
        codexmux::cpa::start(paths, &settings.cpa, &credentials.cpa_token)?;
        tracing::info!("CPA started for active Codex client");
        launched_cpa = true;
    }
    let state = AppState::new(
        settings.clone(),
        credentials,
        paths.catalog.clone(),
        paths.cpa_profiles.clone(),
    )?;
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

fn status(paths: &Paths, no_codex_config: bool) -> Result<()> {
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

fn apply_direct_context_metadata(
    paths: &Paths,
    slugs: &[String],
    context_window: Option<u64>,
    max_context_window: Option<u64>,
) -> Result<()> {
    if context_window.is_none() && max_context_window.is_none() {
        return Ok(());
    }
    for slug in slugs {
        codexmux::cpa::set_direct_model_metadata(
            paths,
            slug,
            codexmux::cpa::DirectModelMetadata {
                context_window,
                max_context_window,
                display_name: None,
            },
        )?;
    }
    Ok(())
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
                codexmux::cpa::installed_version(paths)
                    .map(|version| version.version)
                    .unwrap_or_else(|| codexmux::cpa::CPA_VERSION.to_owned()),
                codexmux::cpa::binary_path(paths).display()
            );
            println!("config: {}", codexmux::cpa::config_path(paths).display());
            println!(
                "service: started ({}); logs: {}/logs/cpa-*.log",
                codexmux::cpa::agent_label(),
                paths.root.display()
            );
        }
        CpaCommand::UpdateCheck => {
            let check = codexmux::cpa::check_cpa_update(paths, None)?;
            println!(
                "current: {}",
                check.current_version.as_deref().unwrap_or("not installed")
            );
            println!("latest: {}", check.latest_version);
            println!("update available: {}", check.update_available);
        }
        CpaCommand::Update { version, dry_run } => {
            anyhow::ensure!(
                settings.cpa.is_loopback(),
                "CPA update only applies to the managed local CLIProxyAPI; remote profiles are not updated"
            );
            let credentials = secrets::load(&paths.credentials)?;
            let outcome = codexmux::cpa::update_cpa(
                paths,
                &settings.cpa,
                &credentials.cpa_token,
                version.as_deref(),
                dry_run,
            )?;
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
            anyhow::ensure!(
                settings.cpa.is_loopback(),
                "CPA rollback only applies to the managed local CLIProxyAPI; remote profiles are not updated"
            );
            let credentials = secrets::load(&paths.credentials)?;
            codexmux::cpa::rollback_cpa(paths, &settings.cpa, &credentials.cpa_token)?;
            println!(
                "CPA rolled back to {}",
                codexmux::cpa::installed_version(paths)
                    .map(|version| version.version)
                    .unwrap_or_else(|| "unknown".into())
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
            println!(
                "rollback: {}",
                if codexmux::cpa::rollback_available(paths) {
                    "available"
                } else {
                    "no previous version"
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
        CpaCommand::ImageGet => match codexmux::cpa::image_override(&paths.cpa_profiles) {
            Some(slug) => println!("image override: {slug}"),
            None => println!("image override: (none; official route)"),
        },
        CpaCommand::ImageModelList => {
            let credentials = secrets::load(&paths.credentials)?;
            for slug in codexmux::cpa::image_model_slugs(&settings.cpa, &credentials.cpa_token)? {
                println!("{slug}");
            }
        }
        CpaCommand::SearchGet => match codexmux::cpa::search_backend_setting(&paths.cpa_profiles) {
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
            codexmux::cpa::set_search_backend_setting(&paths.cpa_profiles, override_kind)?;
            println!("{message}");
        }
        CpaCommand::SearchCapabilities => {
            let store = codexmux::cpa::load_search_capabilities(&paths.search_capabilities)?;
            for (slug, entry) in store.entries {
                println!("{slug} {} {}", entry.status.label(), entry.checked_at);
            }
        }
        CpaCommand::SearchDetect { model, verify } => {
            let results =
                codexmux::cpa::detect_search_capabilities(paths, model.as_deref(), verify)?;
            let count = results.len();
            for (slug, status) in &results {
                println!("{slug} {}", status.label());
            }
            println!(
                "cached {} results in {}",
                count,
                paths.search_capabilities.display()
            );
        }
        CpaCommand::DirectSet {
            models,
            base_url,
            token_env,
            upstream_model,
            context_window,
            max_context_window,
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
                None => (slugs.clone(), std::collections::BTreeMap::new()),
            };
            codexmux::cpa::set_direct_routes(
                paths,
                vec![codexmux::cpa::DirectRoute {
                    base_url,
                    token,
                    models,
                    model_aliases,
                    model_metadata: Default::default(),
                }],
            )?;
            apply_direct_context_metadata(paths, &slugs, context_window, max_context_window)?;
            println!("direct route configured for {configured_models}");
        }
        CpaCommand::DirectAdd {
            models,
            base_url,
            token_env,
            upstream_model,
            context_window,
            max_context_window,
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
            apply_direct_context_metadata(paths, &slugs, context_window, max_context_window)?;
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
                for (slug, metadata) in &route.model_metadata {
                    let context = metadata
                        .context_window
                        .map(|window| format!("context_window={window}"))
                        .unwrap_or_else(|| "context_window=default".into());
                    let max_context = metadata
                        .max_context_window
                        .map(|window| format!("max_context_window={window}"))
                        .unwrap_or_else(|| "max_context_window=default".into());
                    let display_name = metadata
                        .display_name
                        .as_deref()
                        .map(|name| format!("display_name={name:?}"))
                        .unwrap_or_else(|| "display_name=default".into());
                    println!("  {slug}: {context} {max_context} {display_name}");
                }
            }
        }
        CpaCommand::DirectMetadataSet {
            model,
            context_window,
            max_context_window,
            display_name,
        } => {
            codexmux::cpa::set_direct_model_metadata(
                paths,
                &model,
                codexmux::cpa::DirectModelMetadata {
                    context_window,
                    max_context_window,
                    display_name,
                },
            )?;
            println!("direct model metadata updated for {model}");
        }
        CpaCommand::DirectClear => {
            codexmux::cpa::set_direct_routes(paths, Vec::new())?;
            println!("direct routes cleared");
        }
        CpaCommand::DirectDiscover {
            base_url,
            token_env,
        } => {
            let token = std::env::var(&token_env)
                .with_context(|| format!("{token_env} must contain the direct route token"))?;
            for model in codexmux::cpa::direct_model_slugs(&base_url, &token)? {
                println!("{model}");
            }
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
        CpaCommand::ImageSet { slug } => {
            let slug = slug.trim().to_owned();
            if slug.is_empty() {
                codexmux::cpa::set_image_override(&paths.cpa_profiles, None)?;
                println!("image override cleared");
            } else {
                codexmux::cpa::set_image_override(&paths.cpa_profiles, Some(slug.clone()))?;
                println!("image generation now routes to {slug}");
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
