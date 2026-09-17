/// Managed config marker; if a config exists without it, CodexMux never rewrites it.
const MANAGED_MARKER: &str = "# Managed by CodexMux";

fn port_of(cpa: &Cpa) -> u16 {
    reqwest::Url::parse(&cpa.base_url)
        .expect("validated CPA base_url")
        .port_or_known_default()
        .unwrap_or(80)
}

/// Write the managed `config.yaml` unless the user edited it away from CodexMux.
///
/// The file ends with a `# CodexMux providers` section that `cpa provider`
/// commands own. `write_config` preserves it byte-for-byte across rewrites.
pub fn write_config(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let management_key = crate::secrets::load(&paths.credentials)?.cpa_management_key;
    let path = config_path(paths);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if path.exists() {
        anyhow::ensure!(
            is_managed_config(&existing),
            "CPA config {} exists but is not managed by CodexMux; refusing to overwrite it",
            path.display()
        );
    }
    let providers = provider_section(&existing);
    let mut yaml = String::new();
    yaml.push_str(
        "# Managed by CodexMux; manual edits will be overwritten. Use `codexmux cpa` commands.\n",
    );
    yaml.push_str("host: \"127.0.0.1\"\n");
    yaml.push_str(&format!("port: {}\n", port_of(cpa)));
    yaml.push_str("remote-management:\n  allow-remote: false\n");
    yaml.push_str(&format!("  secret-key: \"{management_key}\"\n"));
    yaml.push_str("  disable-auto-update-panel: true\n");
    yaml.push_str("auth-dir: \"~/.cli-proxy-api\"\n");
    yaml.push_str("api-keys:\n");
    yaml.push_str(&format!("  - \"{token}\"\n"));
    yaml.push_str("debug: false\n");
    yaml.push_str("logging-to-file: false\n");
    yaml.push_str("usage-statistics-enabled: false\n");
    yaml.push_str(&providers);
    atomic_write(&path, yaml.as_bytes())
}

fn replace_management_key(config: &str, management_key: &str) -> Result<String> {
    let mut output = String::with_capacity(config.len() + management_key.len());
    let mut in_remote_management = false;
    let mut replaced = false;

    for line in config.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        if !content.starts_with(char::is_whitespace) {
            in_remote_management = content.trim_end() == "remote-management:";
        }
        if in_remote_management && content.trim_start().starts_with("secret-key:") {
            let indent = &content[..content.len() - content.trim_start().len()];
            output.push_str(indent);
            output.push_str("secret-key: \"");
            output.push_str(management_key);
            output.push('"');
            if line.ends_with('\n') {
                output.push('\n');
            }
            replaced = true;
        } else {
            output.push_str(line);
        }
    }

    anyhow::ensure!(
        replaced,
        "managed CPA config is missing remote-management.secret-key"
    );
    Ok(output)
}

/// Install the private management key into an existing managed CPA config.
/// If CPA is running, reload it so the copied key is immediately usable.
pub fn sync_management_key(paths: &Paths, management_key: &str) -> Result<()> {
    let path = config_path(paths);
    if !path.exists() {
        return Ok(());
    }
    let existing =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    anyhow::ensure!(
        is_managed_config(&existing),
        "CPA config {} exists but is not managed by CodexMux; refusing to overwrite it",
        path.display()
    );
    let replacement = replace_management_key(&existing, management_key)?;
    if replacement == existing {
        return Ok(());
    }

    let was_loaded = is_loaded()?;
    atomic_write(&path, replacement.as_bytes())?;
    set_private(&path)?;
    if !was_loaded {
        return Ok(());
    }

    if let Err(error) = stop_service() {
        atomic_write(&path, existing.as_bytes()).ok();
        set_private(&path).ok();
        return Err(error).context("failed to stop CPA for management-key reload; rolled back");
    }
    if let Err(error) = bootstrap_service(paths) {
        atomic_write(&path, existing.as_bytes()).ok();
        set_private(&path).ok();
        bootstrap_service(paths).ok();
        return Err(error).context("failed to reload CPA with the management key; rolled back");
    }
    Ok(())
}

const CONNECT_SCRIPT_ID: &str = "codexmux-connect";

fn connect_bootstrap_script() -> String {
    format!(
        r#"<script id="{CONNECT_SCRIPT_ID}">
(function(){{
  try {{
    var params = new URLSearchParams(window.location.search);
    var key = params.get("cmk");
    var base = params.get("cmb");
    if (!key && !base) return;
    if (base) localStorage.setItem("apiBase", JSON.stringify(base));
    if (key) {{
      localStorage.setItem("managementKey", JSON.stringify(key));
      localStorage.setItem("isLoggedIn", "true");
    }}
    params.delete("cmk");
    params.delete("cmb");
    var search = params.toString();
    history.replaceState(null, "", window.location.pathname + (search ? "?" + search : "") + window.location.hash);
  }} catch (e) {{}}
}})();
</script>"#
    )
}

fn inject_connect_bootstrap(html: &str) -> String {
    let script = connect_bootstrap_script();
    let marker = format!("<script id=\"{CONNECT_SCRIPT_ID}\">");
    if let Some(start) = html.find(&marker)
        && let Some(rel_end) = html[start..].find("</script>")
    {
        let end = start + rel_end + "</script>".len();
        let mut output = String::with_capacity(html.len() + script.len());
        output.push_str(&html[..start]);
        output.push_str(&script);
        output.push_str(&html[end..]);
        return output;
    }
    if let Some(head) = html.find("<head>") {
        let insert_at = head + "<head>".len();
        let mut output = String::with_capacity(html.len() + script.len() + 8);
        output.push_str(&html[..insert_at]);
        output.push('\n');
        output.push_str(&script);
        output.push_str(&html[insert_at..]);
        return output;
    }
    format!("{script}{html}")
}

/// Make the local CPA management page accept CodexMux login query parameters.
///
/// The bundled Web UI does not read connection info from the URL. CPA may also
/// overwrite this file on panel auto-update, so the bootstrap is reapplied
/// before each connect.
pub fn ensure_management_connect_bootstrap(paths: &Paths) -> Result<()> {
    let path = management_html_path(paths);
    if !path.is_file() {
        return Ok(());
    }
    let existing =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let rewritten = inject_connect_bootstrap(&existing);
    if rewritten != existing {
        atomic_write(&path, rewritten.as_bytes())?;
    }
    Ok(())
}

const PROVIDERS_HEADER: &str = "# CodexMux providers (managed by `codexmux cpa provider`)";

fn is_managed_config(config: &str) -> bool {
    config.contains(MANAGED_MARKER)
}

fn provider_section(config: &str) -> String {
    let start = match config.find(PROVIDERS_HEADER) {
        Some(index) => index,
        None => return format!("\n{PROVIDERS_HEADER}\n"),
    };
    config[start..].trim_end().to_owned() + "\n"
}

/// Replace the provider section with the contents of `providers_toml`.
///
/// The TOML is expected to hold `[[openai-compatibility]]`-style tables; it is
/// converted to the YAML list form CPA expects. Keys and secrets stay inside
/// the managed config, which the caller keeps mode-0600.
pub fn import_providers(paths: &Paths, providers_toml: &str) -> Result<()> {
    let yaml = providers_yaml(providers_toml)?;
    let path = config_path(paths);
    let mut config =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    anyhow::ensure!(
        is_managed_config(&config),
        "CPA config {} exists but is not managed by CodexMux; refusing to overwrite it",
        path.display()
    );
    let replacement = format!("\n{PROVIDERS_HEADER}\n{yaml}");
    match config.find(PROVIDERS_HEADER) {
        Some(index) => config.replace_range(index.., &replacement),
        None => config.push_str(&replacement),
    }
    atomic_write(&path, config.as_bytes())?;
    set_private(&path)
}

/// Restart CPA so config changes take effect; starts it if it was stopped.
pub fn restart(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    if is_loaded()? {
        stop_service()?;
    }
    // Internal restarts keep the user's startup preference untouched.
    start_service(paths, cpa, token)
}

fn set_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("failed to restrict {}", path.display()))
}

/// Convert `[[openai-compatibility]]` / `[[codex-api-key]]` TOML tables to CPA YAML.
fn providers_yaml(providers_toml: &str) -> Result<String> {
    let value: toml::Value = toml::from_str(providers_toml).context("provider TOML is invalid")?;
    validate_provider_model_aliases(&value)?;
    let mut yaml = String::new();
    for kind in ["openai-compatibility", "codex-api-key"] {
        let Some(entries) = value.get(kind).and_then(|v| v.as_array()) else {
            continue;
        };
        if entries.is_empty() {
            continue;
        }
        yaml.push_str(&format!("{kind}:\n"));
        for entry in entries {
            let table = entry
                .as_table()
                .context("each provider entry must be a table")?;
            for (index, (key, value)) in table.iter().enumerate() {
                let prefix = if index == 0 { "  - " } else { "    " };
                yaml.push_str(&entry_field(key, value, prefix, "    ")?);
            }
        }
    }
    anyhow::ensure!(
        !yaml.is_empty(),
        "provider TOML contains no provider tables"
    );
    Ok(yaml)
}

/// Provider imports must use explicit, globally unique model aliases. The
/// alias is the only stable identity CodexMux receives from CPA, so an empty
/// alias or the same alias on two providers makes exact routing ambiguous.
fn validate_provider_model_aliases(value: &toml::Value) -> Result<()> {
    let mut seen = HashSet::new();
    for kind in ["openai-compatibility", "codex-api-key"] {
        let Some(entries) = value.get(kind).and_then(toml::Value::as_array) else {
            continue;
        };
        for (provider_index, entry) in entries.iter().enumerate() {
            let provider = entry
                .as_table()
                .with_context(|| format!("{kind}[{provider_index}] must be a table"))?;
            let models = provider
                .get("models")
                .and_then(toml::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            for (model_index, model) in models.iter().enumerate() {
                let model = model
                    .as_table()
                    .with_context(|| format!("{kind}[{provider_index}].models[{model_index}] must be a table"))?;
                let name = model
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .unwrap_or("<unnamed>");
                let alias = model
                    .get("alias")
                    .and_then(toml::Value::as_str)
                    .map(str::trim)
                    .filter(|alias| !alias.is_empty())
                    .with_context(|| {
                        format!(
                            "{kind}[{provider_index}].models[{model_index}] ({name}) must define a nonempty provider-specific alias"
                        )
                    })?;
                anyhow::ensure!(
                    seen.insert(alias.to_ascii_lowercase()),
                    "{kind}[{provider_index}].models[{model_index}] duplicates provider model alias {alias}"
                );
            }
        }
    }
    Ok(())
}

/// Render one provider field. `prefix` precedes this field (a `- ` list dash
/// for the first field of an entry, plain indentation otherwise) and `indent`
/// is the entry's base indent used for nested blocks.
fn entry_field(key: &str, value: &toml::Value, prefix: &str, indent: &str) -> Result<String> {
    let mut yaml = String::new();
    match value {
        toml::Value::String(text) => yaml.push_str(&format!("{prefix}{key}: {text:?}\n")),
        toml::Value::Integer(number) => yaml.push_str(&format!("{prefix}{key}: {number}\n")),
        toml::Value::Boolean(flag) => yaml.push_str(&format!("{prefix}{key}: {flag}\n")),
        toml::Value::Array(items) if items.iter().all(|item| item.is_str()) => {
            let names = items
                .iter()
                .map(|item| format!("{:?}", item.as_str().expect("checked above")))
                .collect::<Vec<_>>();
            yaml.push_str(&format!("{prefix}{key}: [{}]\n", names.join(", ")));
        }
        toml::Value::Array(items) if items.iter().all(|item| item.is_table()) => {
            yaml.push_str(&format!("{prefix}{key}:\n"));
            for item in items {
                let item = item.as_table().expect("checked above");
                for (nested_index, (nested_key, nested_value)) in item.iter().enumerate() {
                    let nested_prefix = if nested_index == 0 {
                        format!("{indent}  - ")
                    } else {
                        format!("{indent}    ")
                    };
                    yaml.push_str(&entry_field(
                        nested_key,
                        nested_value,
                        &nested_prefix,
                        &format!("{indent}    "),
                    )?);
                }
            }
        }
        toml::Value::Table(nested) => {
            yaml.push_str(&format!("{prefix}{key}:\n"));
            let nested_indent = format!("{indent}  ");
            for (nested_key, nested_value) in nested {
                yaml.push_str(&entry_field(
                    nested_key,
                    nested_value,
                    &nested_indent,
                    &nested_indent,
                )?);
            }
        }
        _ => bail!("unsupported value for {key} in provider entries"),
    }
    Ok(yaml)
}
