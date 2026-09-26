use std::{collections::HashSet, fs, path::Path};

use anyhow::{Context, Result, bail};

use super::{
    config_path, lock, management_html_path,
    service::{Launchd, ServiceControl, bootstrap_service, restart_service},
};
use crate::{
    config::{Cpa, Paths, toml_parse_error},
    fsutil::{atomic_write, atomic_write_private},
};

/// Managed config marker; if a config exists without it, CodexMux never rewrites it.
const MANAGED_MARKER: &str = "# Managed by CodexMux";
const PROVIDERS_HEADER: &str = "# CodexMux providers (managed by `codexmux cpa provider`)";
const PROVIDER_KINDS: [&str; 2] = ["openai-compatibility", "codex-api-key"];

/// Write the managed `config.yaml` unless the user edited it away from CodexMux.
///
/// The file ends with a `# CodexMux providers` section that provider imports
/// own; it is preserved byte-for-byte across rewrites.
pub(super) fn write_config(paths: &Paths, cpa: &Cpa, token: &str) -> Result<()> {
    let port = managed_port(cpa)?;
    let management_key = crate::secrets::load(&paths.credentials)?.cpa_management_key;
    let management_key =
        yaml_scalar(&management_key).context("the CPA management key cannot be written")?;
    let token = yaml_scalar(token).context("the CPA token cannot be written")?;
    let path = config_path(paths);
    let existing = match fs::read_to_string(&path) {
        Ok(existing) => {
            ensure_managed(&path, &existing)?;
            existing
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let providers = provider_section(&existing);
    let mut yaml = String::new();
    yaml.push_str(
        "# Managed by CodexMux; manual edits will be overwritten. Use `codexmux cpa` commands.\n",
    );
    yaml.push_str("host: \"127.0.0.1\"\n");
    yaml.push_str(&format!("port: {port}\n"));
    yaml.push_str("remote-management:\n  allow-remote: false\n");
    yaml.push_str(&format!("  secret-key: {management_key}\n"));
    yaml.push_str("  disable-auto-update-panel: true\n");
    yaml.push_str("auth-dir: \"~/.cli-proxy-api\"\n");
    yaml.push_str("api-keys:\n");
    yaml.push_str(&format!("  - {token}\n"));
    yaml.push_str("debug: false\n");
    yaml.push_str("logging-to-file: false\n");
    yaml.push_str("usage-statistics-enabled: false\n");
    yaml.push_str(&providers);
    atomic_write_private(&path, yaml.as_bytes())
}

/// The port the managed CPA listens on. It binds 127.0.0.1 over plain HTTP,
/// so only a loopback HTTP endpoint can describe it.
fn managed_port(cpa: &Cpa) -> Result<u16> {
    cpa.validate()?;
    let url = reqwest::Url::parse(&cpa.base_url).context("CPA has an invalid base_url")?;
    anyhow::ensure!(
        cpa.is_loopback() && url.scheme() == "http",
        "the managed local CPA serves plain HTTP on 127.0.0.1, but CodexMux is configured for {}",
        cpa.base_url
    );
    url.port_or_known_default()
        .context("CPA base_url has no port")
}

fn ensure_managed(path: &Path, config: &str) -> Result<()> {
    anyhow::ensure!(
        config.contains(MANAGED_MARKER),
        "CPA config {} exists but is not managed by CodexMux; refusing to overwrite it",
        path.display()
    );
    Ok(())
}

fn replace_management_key(config: &str, management_key: &str) -> Result<String> {
    let management_key =
        yaml_scalar(management_key).context("the CPA management key cannot be written")?;
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
            output.push_str("secret-key: ");
            output.push_str(&management_key);
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
    let _lock = lock(paths)?;
    sync_management_key_with(&Launchd, paths, management_key)
}

fn sync_management_key_with(
    services: &dyn ServiceControl,
    paths: &Paths,
    management_key: &str,
) -> Result<()> {
    let path = config_path(paths);
    let existing = match fs::read_to_string(&path) {
        Ok(existing) => existing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    ensure_managed(&path, &existing)?;
    let replacement = replace_management_key(&existing, management_key)?;
    if replacement == existing {
        return Ok(());
    }

    let was_loaded = services.cpa_loaded()?;
    atomic_write_private(&path, replacement.as_bytes())?;
    if !was_loaded {
        return Ok(());
    }
    let Err(error) = services
        .stop_cpa()
        .and_then(|()| bootstrap_service(services, paths))
    else {
        return Ok(());
    };
    let restore = atomic_write_private(&path, existing.as_bytes()).and_then(|()| {
        services.stop_cpa()?;
        bootstrap_service(services, paths)
    });
    Err(match restore {
        Ok(()) => error.context(
            "failed to reload CPA with the management key; restored and restarted the previous config",
        ),
        Err(restore_error) => error.context(format!(
            "failed to reload CPA with the management key, and restoring the previous config failed: {restore_error:#}"
        )),
    })
}

const CONNECT_SCRIPT_ID: &str = "codexmux-connect";

/// Sign the bundled Web UI in from `#cmk=<key>`. The key is read only from the
/// fragment and the API base only from the page location, so no URL can point
/// the stored key at another server. The fragment is removed once stored.
fn connect_bootstrap_script() -> String {
    format!(
        r#"<script id="{CONNECT_SCRIPT_ID}">
(function(){{
  try {{
    var fragment = window.location.hash.replace(/^#/, "");
    if (!fragment) return;
    var key = new URLSearchParams(fragment).get("cmk");
    if (!key) return;
    var path = window.location.pathname;
    var base = window.location.origin + path.slice(0, path.lastIndexOf("/"));
    localStorage.setItem("apiBase", JSON.stringify(base));
    localStorage.setItem("managementKey", JSON.stringify(key));
    localStorage.setItem("isLoggedIn", "true");
    history.replaceState(null, "", path + window.location.search);
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

/// Make the local CPA management page accept the CodexMux sign-in fragment.
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

/// The provider section of a managed config, starting with the blank line
/// that separates it from the CodexMux-owned settings.
fn provider_section(config: &str) -> String {
    match config.find(PROVIDERS_HEADER) {
        Some(index) => format!("\n{}\n", config[index..].trim_end()),
        None => format!("\n{PROVIDERS_HEADER}\n"),
    }
}

/// Replace the managed provider section with the tables in `providers_file`
/// and restart CPA so they take effect.
///
/// The TOML holds `[[openai-compatibility]]` / `[[codex-api-key]]` tables and
/// is converted to the YAML list form CPA expects. Keys and secrets stay in
/// the private managed config.
pub fn import_providers(
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
    providers_file: &Path,
) -> Result<()> {
    let text = fs::read_to_string(providers_file)
        .with_context(|| format!("failed to read {}", providers_file.display()))?;
    let yaml = providers_yaml(providers_file, &text)?;
    let _lock = lock(paths)?;
    replace_providers(&Launchd, paths, cpa, token, &yaml)
}

fn replace_providers(
    services: &dyn ServiceControl,
    paths: &Paths,
    cpa: &Cpa,
    token: &str,
    providers_yaml: &str,
) -> Result<()> {
    let path = config_path(paths);
    let config =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    ensure_managed(&path, &config)?;
    let settings = match config.find(PROVIDERS_HEADER) {
        Some(index) => &config[..index],
        None => &config,
    };
    let config = format!(
        "{}\n\n{PROVIDERS_HEADER}\n{providers_yaml}",
        settings.trim_end()
    );
    atomic_write_private(&path, config.as_bytes())?;
    restart_service(services, paths, cpa, token)
}

/// Render a string as a YAML double-quoted scalar.
///
/// JSON string syntax is valid YAML, but JSON leaves DEL, C1 controls, and a
/// few Unicode separators unescaped, and libyaml / yaml.v3 reject or fold
/// those raw. Such values are refused instead of silently changed.
fn yaml_scalar(value: &str) -> Result<String> {
    if let Some(character) = value.chars().find(|&character| yaml_rejects(character)) {
        bail!(
            "value contains unsupported character U+{:04X}",
            u32::from(character)
        );
    }
    Ok(serde_json::to_string(value)?)
}

fn yaml_rejects(character: char) -> bool {
    matches!(
        character,
        '\u{0}'..='\u{8}'
            | '\u{b}'
            | '\u{c}'
            | '\u{e}'..='\u{1f}'
            | '\u{7f}'..='\u{9f}'
            | '\u{feff}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

/// Convert `[[openai-compatibility]]` / `[[codex-api-key]]` TOML tables to CPA YAML.
fn providers_yaml(source: &Path, providers_toml: &str) -> Result<String> {
    let value: toml::Table = toml::from_str(providers_toml)
        .map_err(|error| toml_parse_error(source, providers_toml, &error))?;
    if let Some(kind) = value
        .keys()
        .find(|kind| !PROVIDER_KINDS.contains(&kind.as_str()))
    {
        bail!(
            "unsupported provider table {kind:?} in {}; supported tables: {}",
            source.display(),
            PROVIDER_KINDS.join(", ")
        );
    }
    validate_provider_model_aliases(&value)?;
    let mut yaml = String::new();
    for kind in PROVIDER_KINDS {
        let Some(entries) = value.get(kind) else {
            continue;
        };
        let entries = entries
            .as_array()
            .with_context(|| format!("{kind} must be an array of tables ([[{kind}]])"))?;
        if entries.is_empty() {
            continue;
        }
        yaml.push_str(&format!("{kind}:\n"));
        for (index, entry) in entries.iter().enumerate() {
            let table = entry
                .as_table()
                .with_context(|| format!("{kind}[{index}] must be a table"))?;
            for (field_index, (key, value)) in table.iter().enumerate() {
                let prefix = if field_index == 0 { "  - " } else { "    " };
                yaml.push_str(
                    &entry_field(key, value, prefix, "    ")
                        .with_context(|| format!("{kind}[{index}]"))?,
                );
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
fn validate_provider_model_aliases(value: &toml::Table) -> Result<()> {
    let mut seen = HashSet::new();
    for kind in PROVIDER_KINDS {
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
                let model = model.as_table().with_context(|| {
                    format!("{kind}[{provider_index}].models[{model_index}] must be a table")
                })?;
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
    let quoted_key = yaml_scalar(key).context("a provider field name cannot be written")?;
    let scalar = |text: &str| yaml_scalar(text).with_context(|| format!("provider field {key}"));
    let mut yaml = String::new();
    match value {
        toml::Value::String(text) => {
            yaml.push_str(&format!("{prefix}{quoted_key}: {}\n", scalar(text)?));
        }
        toml::Value::Integer(number) => {
            yaml.push_str(&format!("{prefix}{quoted_key}: {number}\n"));
        }
        toml::Value::Boolean(flag) => yaml.push_str(&format!("{prefix}{quoted_key}: {flag}\n")),
        toml::Value::Array(items) if items.iter().all(toml::Value::is_str) => {
            let items = items
                .iter()
                .filter_map(toml::Value::as_str)
                .map(scalar)
                .collect::<Result<Vec<_>>>()?;
            yaml.push_str(&format!("{prefix}{quoted_key}: [{}]\n", items.join(", ")));
        }
        toml::Value::Array(items) if items.iter().all(toml::Value::is_table) => {
            yaml.push_str(&format!("{prefix}{quoted_key}:\n"));
            for item in items.iter().filter_map(toml::Value::as_table) {
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
            yaml.push_str(&format!("{prefix}{quoted_key}:\n"));
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
        _ => bail!("unsupported value for provider field {key}"),
    }
    Ok(yaml)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::cpa::{
        binary_path,
        test_support::{FakeServices, local_cpa, test_root},
    };

    fn providers(toml: &str) -> Result<String> {
        providers_yaml(Path::new("/tmp/providers.toml"), toml)
    }

    #[test]
    fn managed_config_writes_loopback_port_and_token() {
        let root = test_root();
        write_config(&root.paths, &local_cpa(8317), "token-a").unwrap();
        let text = fs::read_to_string(config_path(&root.paths)).unwrap();
        assert!(text.contains(MANAGED_MARKER));
        assert!(text.contains("host: \"127.0.0.1\""));
        assert!(text.contains("port: 8317"));
        assert!(text.contains("  secret-key: \"management-key\"\n"));
        assert!(text.contains("  disable-auto-update-panel: true\n"));
        assert!(text.contains("  - \"token-a\"\n"));
    }

    #[test]
    fn managed_config_escapes_tokens_and_enforces_private_permissions() {
        let root = test_root();
        write_config(&root.paths, &local_cpa(8317), "token\"\\\\\nnext\ttab").unwrap();
        let path = config_path(&root.paths);
        let text = fs::read_to_string(&path).unwrap();
        let yaml: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
        assert_eq!(yaml["api-keys"][0].as_str(), Some("token\"\\\\\nnext\ttab"));
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn managed_config_refuses_characters_yaml_parsers_reject_or_fold() {
        let root = test_root();
        for character in [
            '\u{0}', '\u{1b}', '\u{7f}', '\u{85}', '\u{9f}', '\u{feff}', '\u{2028}', '\u{2029}',
        ] {
            let token = format!("secret{character}token");
            let error = write_config(&root.paths, &local_cpa(8317), &token).unwrap_err();
            let rendered = format!("{error:#}");
            assert!(rendered.contains("unsupported character"), "{rendered}");
            assert!(!rendered.contains("secret"), "{rendered}");
        }
        assert!(!config_path(&root.paths).exists());
    }

    #[test]
    fn managed_config_is_only_written_for_a_local_http_endpoint() {
        let root = test_root();
        for base_url in ["https://cpa.example.com/v1", "https://127.0.0.1:8317/v1"] {
            let cpa = Cpa {
                base_url: base_url.into(),
            };
            let error = write_config(&root.paths, &cpa, "token-a").unwrap_err();
            assert!(error.to_string().contains("plain HTTP on 127.0.0.1"));
        }
        assert!(!config_path(&root.paths).exists());
    }

    #[test]
    fn management_key_replacement_only_touches_remote_management() {
        let config = concat!(
            "# Managed by CodexMux\n",
            "remote-management:\n",
            "  allow-remote: false\n",
            "  secret-key: \"old\"\n",
            "provider:\n",
            "  secret-key: \"provider-secret\"\n",
        );
        let replaced = replace_management_key(config, "new-management-key").unwrap();
        assert!(replaced.contains("  secret-key: \"new-management-key\"\n"));
        assert!(replaced.contains("  secret-key: \"provider-secret\"\n"));
        assert!(!replaced.contains("  secret-key: \"old\"\n"));
    }

    #[test]
    fn management_key_sync_reloads_a_running_cpa_and_rolls_back_on_failure() {
        let root = test_root();
        let paths = &root.paths;
        fs::create_dir_all(binary_path(paths).parent().unwrap()).unwrap();
        fs::write(binary_path(paths), "cpa-binary").unwrap();
        write_config(paths, &local_cpa(8317), "token-a").unwrap();
        let before = fs::read(config_path(paths)).unwrap();

        let services = FakeServices::loaded();
        sync_management_key_with(&services, paths, "rotated-key").unwrap();
        assert_eq!(services.events(), ["stop", "bootstrap cpa-binary"]);
        let synced = fs::read_to_string(config_path(paths)).unwrap();
        assert!(synced.contains("secret-key: \"rotated-key\""));

        // A failed reload restores the previous bytes.
        fs::write(config_path(paths), &before).unwrap();
        let services = FakeServices::loaded();
        *services.fail_bootstrap.borrow_mut() = true;
        let error = sync_management_key_with(&services, paths, "rotated-key").unwrap_err();
        assert!(format!("{error:#}").contains("restoring the previous config failed"));
        assert_eq!(fs::read(config_path(paths)).unwrap(), before);
    }

    #[test]
    fn management_connect_bootstrap_reads_only_the_fragment_and_page_origin() {
        let html = "<!doctype html>\n<html>\n  <head>\n    <meta charset=\"UTF-8\" />\n  </head>\n</html>\n";
        let first = inject_connect_bootstrap(html);
        assert_eq!(first.matches(CONNECT_SCRIPT_ID).count(), 1);
        assert!(first.contains("window.location.hash"));
        assert!(first.contains("get(\"cmk\")"));
        assert!(first.contains("window.location.origin"));
        assert!(first.contains("history.replaceState(null, \"\", path + window.location.search)"));
        // The key and API base never come from the query string.
        assert!(!first.contains("cmb"));
        assert!(!first.contains("URLSearchParams(window.location.search)"));

        let second = inject_connect_bootstrap(&first);
        assert_eq!(second, first);
        assert_eq!(second.matches("<head>").count(), 1);

        // An older injected script is replaced, not duplicated.
        let stale = html.replace(
            "<head>",
            &format!("<head>\n<script id=\"{CONNECT_SCRIPT_ID}\">var cmb;</script>"),
        );
        let refreshed = inject_connect_bootstrap(&stale);
        assert_eq!(refreshed.matches(CONNECT_SCRIPT_ID).count(), 1);
        assert!(!refreshed.contains("var cmb"));
    }

    #[test]
    fn hand_edited_config_is_never_overwritten() {
        let root = test_root();
        write_config(&root.paths, &local_cpa(8317), "token-a").unwrap();
        fs::write(
            config_path(&root.paths),
            "# my own CPA config\nport: 9999\n",
        )
        .unwrap();
        let error = write_config(&root.paths, &local_cpa(8317), "token-a").unwrap_err();
        assert!(error.to_string().contains("refusing to overwrite"));
    }

    #[test]
    fn provider_section_survives_config_rewrites() {
        let root = test_root();
        let paths = &root.paths;
        write_config(paths, &local_cpa(8317), "token-a").unwrap();
        let section =
            format!("\n{PROVIDERS_HEADER}\nopenai-compatibility:\n  - \"name\": \"example\"\n");
        let base = fs::read_to_string(config_path(paths)).unwrap();
        let with_providers = base.replace(&format!("\n{PROVIDERS_HEADER}\n"), &section);
        fs::write(config_path(paths), with_providers).unwrap();
        write_config(paths, &local_cpa(9317), "token-b").unwrap();
        let rewritten = fs::read_to_string(config_path(paths)).unwrap();
        assert!(rewritten.contains("port: 9317"));
        assert!(rewritten.contains("token-b"));
        assert!(rewritten.contains(section.trim_end()));
        assert_eq!(rewritten.matches(PROVIDERS_HEADER).count(), 1);
    }

    #[test]
    fn import_providers_converts_toml_tables_to_cpa_yaml() {
        let yaml = providers(
            r#"
[[openai-compatibility]]
name = "kunbot-ris"
base-url = "https://example.com/ris/v1"
headers = { "X-Team: #1" = "blue" }

[[openai-compatibility.models]]
name = "zai-org/GLM-5.3-Flash"
alias = "glm-5.3-flash"
"#,
        )
        .unwrap();
        assert!(yaml.starts_with("openai-compatibility:\n"));
        assert!(yaml.contains("\"name\": \"kunbot-ris\"\n"));
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        let providers = parsed["openai-compatibility"].as_sequence().unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0]["name"].as_str(), Some("kunbot-ris"));
        assert_eq!(
            providers[0]["base-url"].as_str(),
            Some("https://example.com/ris/v1")
        );
        // Keys are quoted like values, so YAML syntax in a key stays literal.
        assert_eq!(providers[0]["headers"]["X-Team: #1"].as_str(), Some("blue"));
        let models = providers[0]["models"].as_sequence().unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["name"].as_str(), Some("zai-org/GLM-5.3-Flash"));
        assert_eq!(models[0]["alias"].as_str(), Some("glm-5.3-flash"));
    }

    #[test]
    fn import_providers_rewrites_the_section_keeps_the_rest_and_restarts_cpa() {
        let root = test_root();
        let paths = &root.paths;
        fs::create_dir_all(binary_path(paths).parent().unwrap()).unwrap();
        fs::write(binary_path(paths), "cpa-binary").unwrap();
        write_config(paths, &local_cpa(8317), "token-a").unwrap();
        let yaml = providers(
            "
[[codex-api-key]]
api-key = \"sk-test\"
base-url = \"https://example.com/acid/v1\"

[[codex-api-key.models]]
name = \"gpt-5.6-terra\"
alias = \"gpt-5.6-terra\"
",
        )
        .unwrap();
        let services = FakeServices::loaded();
        replace_providers(&services, paths, &local_cpa(8317), "token-a", &yaml).unwrap();
        let first = fs::read_to_string(config_path(paths)).unwrap();
        // Re-import replaces rather than appends.
        replace_providers(&services, paths, &local_cpa(8317), "token-a", &yaml).unwrap();
        let config = fs::read_to_string(config_path(paths)).unwrap();
        assert_eq!(config, first);
        assert_eq!(
            services.events(),
            [
                "stop",
                "bootstrap cpa-binary",
                "stop",
                "bootstrap cpa-binary"
            ]
        );

        assert!(config.contains("port: 8317"));
        let parsed: serde_yaml::Value = serde_yaml::from_str(&config).unwrap();
        let codex_keys = parsed["codex-api-key"].as_sequence().unwrap();
        assert_eq!(codex_keys.len(), 1);
        assert_eq!(codex_keys[0]["api-key"].as_str(), Some("sk-test"));
        assert_eq!(
            codex_keys[0]["models"][0]["alias"].as_str(),
            Some("gpt-5.6-terra")
        );
        assert_eq!(config.matches(PROVIDERS_HEADER).count(), 1);
        assert_eq!(config.matches("codex-api-key:").count(), 1);
        assert_eq!(
            fs::metadata(config_path(paths))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn provider_import_rejects_unsupported_tables_and_values() {
        let error = providers("[[claude-api-key]]\napi-key = \"sk\"\n").unwrap_err();
        assert!(error.to_string().contains("unsupported provider table"));

        let error = providers("codex-api-key = \"sk\"\n").unwrap_err();
        assert!(error.to_string().contains("must be an array of tables"));

        let error = providers("[[codex-api-key]]\nweight = 1.5\n").unwrap_err();
        assert!(format!("{error:#}").contains("unsupported value for provider field weight"));

        // TOML accepts DEL only escaped and U+2028 raw; YAML rejects or folds both.
        for (toml, code) in [
            ("[[codex-api-key]]\napi-key = \"sk\\u007F\"\n", "U+007F"),
            ("[[codex-api-key]]\napi-key = \"sk\u{2028}\"\n", "U+2028"),
        ] {
            let error = providers(toml).unwrap_err();
            let rendered = format!("{error:#}");
            assert!(rendered.contains("provider field api-key"), "{rendered}");
            assert!(rendered.contains(code), "{rendered}");
        }
    }

    #[test]
    fn provider_toml_errors_never_quote_the_source() {
        let error = providers("[[codex-api-key]]\napi-key = sk-live-secret\n").unwrap_err();
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("/tmp/providers.toml at line 2"),
            "{rendered}"
        );
        assert!(!rendered.contains("sk-live-secret"), "{rendered}");
    }

    #[test]
    fn provider_import_rejects_empty_or_duplicate_model_aliases() {
        let error = providers(
            r#"
[[codex-api-key]]
base-url = "https://example.com/acid/v1"

[[codex-api-key.models]]
name = "gpt-5.6-terra"
alias = ""
"#,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("nonempty provider-specific alias")
        );

        let error = providers(
            r#"
[[openai-compatibility]]
name = "provider-a"

[[openai-compatibility.models]]
name = "model-a"
alias = "shared-model"

[[openai-compatibility]]
name = "provider-b"

[[openai-compatibility.models]]
name = "model-b"
alias = "shared-model"
"#,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicates provider model alias shared-model")
        );
    }
}
