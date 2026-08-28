use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

use crate::{
    config::{ProviderKind, Settings},
    fsutil::atomic_write,
};

pub fn query_bundled_catalog(codex: &Path) -> Result<Value> {
    let output = Command::new(codex)
        .args(["debug", "models", "--bundled"])
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to run {} debug models --bundled", codex.display()))?;
    anyhow::ensure!(
        output.status.success(),
        "codex debug models --bundled failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    serde_json::from_slice(&output.stdout).context("Codex returned an invalid bundled catalog")
}

pub fn merge(base: &Value, settings: &Settings) -> Result<Value> {
    let mut output = base.clone();
    let models = output
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .context("bundled catalog has no models array")?;
    let template = models
        .first()
        .and_then(Value::as_object)
        .cloned()
        .context("bundled catalog has no model template")?;
    let mut by_slug = BTreeMap::new();
    for (index, model) in models.iter().enumerate() {
        if let Some(slug) = model.get("slug").and_then(Value::as_str) {
            by_slug.insert(slug.to_string(), index);
        }
    }
    let mut priority = 900;
    for provider in settings
        .providers
        .iter()
        .filter(|provider| provider.enabled && provider.kind == ProviderKind::External)
    {
        for configured in &provider.models {
            anyhow::ensure!(
                !by_slug.contains_key(&configured.slug),
                "external model {} collides with an official or previously contributed model",
                configured.slug
            );
            let model = Value::Object(external_model(
                &template,
                configured,
                &provider.name,
                priority,
            ));
            priority += 1;
            by_slug.insert(configured.slug.clone(), models.len());
            models.push(model);
        }
    }
    Ok(output)
}

pub fn save(path: &Path, catalog: &Value) -> Result<()> {
    atomic_write(path, &serde_json::to_vec_pretty(catalog)?)
}

pub fn model_slugs(catalog: &Value) -> Result<HashSet<String>> {
    let models = catalog
        .get("models")
        .and_then(Value::as_array)
        .context("bundled catalog has no models array")?;
    models
        .iter()
        .map(|model| {
            model
                .get("slug")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .context("bundled catalog contains a model without a slug")
        })
        .collect()
}

fn external_model(
    template: &Map<String, Value>,
    configured: &crate::config::Model,
    provider_name: &str,
    priority: i64,
) -> Map<String, Value> {
    let mut model = template.clone();
    model.insert("slug".into(), json!(configured.slug));
    model.insert("display_name".into(), json!(configured.display_name()));
    model.insert(
        "description".into(),
        json!(configured.description.clone().unwrap_or_else(|| format!(
            "{} through {}.",
            configured.display_name(),
            provider_name
        ))),
    );
    model.insert("priority".into(), json!(priority));
    model.insert("use_responses_lite".into(), json!(false));
    model.insert("tool_mode".into(), Value::Null);
    model.insert(
        "input_modalities".into(),
        if configured.accepts_images {
            json!(["text", "image"])
        } else {
            json!(["text"])
        },
    );
    model.insert(
        "supports_image_detail_original".into(),
        json!(configured.accepts_images),
    );
    model.insert("web_search_tool_type".into(), json!("text"));
    model.insert("availability_nux".into(), Value::Null);
    model.insert("upgrade".into(), Value::Null);
    model.insert("auto_review_model_override".into(), Value::Null);
    model.insert("default_service_tier".into(), Value::Null);
    model.insert("context_window".into(), json!(configured.context_window));
    model.insert(
        "max_context_window".into(),
        json!(configured.context_window),
    );
    model.insert(
        "auto_compact_token_limit".into(),
        json!((configured.context_window as f64 * 0.9) as i64),
    );
    for field in [
        "service_tiers",
        "additional_speed_tiers",
        "include_apps_usage_instructions",
        "include_plugin_usage_instructions",
        "node_repl_disabled",
        "node_repl_auto_review_required",
    ] {
        model.remove(field);
    }
    model
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::config::{Dialect, Model, Provider, ProviderKind};

    #[test]
    fn keeps_official_models_and_appends_external_model() {
        let base = json!({"models": [{
            "slug": "gpt-official", "display_name": "GPT", "use_responses_lite": true,
            "service_tiers": ["fast"], "input_modalities": ["text", "image"]
        }]});
        let settings = Settings {
            providers: vec![
                Provider::official(),
                Provider {
                    id: "acme".into(),
                    name: "Acme".into(),
                    kind: ProviderKind::External,
                    dialect: Dialect::OpenaiChat,
                    base_url: "https://example.com/v1".into(),
                    credential_header: None,
                    headers: BTreeMap::new(),
                    enabled: true,
                    allow_cross_model_previous_response_id: false,
                    models: vec![Model {
                        slug: "acme-large".into(),
                        display_name: "Acme Large".into(),
                        description: None,
                        context_window: 100_000,
                        accepts_images: false,
                    }],
                },
            ],
            ..Settings::default()
        };
        let catalog = merge(&base, &settings).unwrap();
        assert_eq!(catalog["models"].as_array().unwrap().len(), 2);
        assert_eq!(catalog["models"][1]["use_responses_lite"], false);
        assert!(catalog["models"][1].get("service_tiers").is_none());
        assert_eq!(catalog["models"][1]["input_modalities"], json!(["text"]));
    }

    #[test]
    fn refuses_to_replace_an_official_slug() {
        let base = json!({"models": [{"slug": "same", "display_name": "Official"}]});
        let settings = Settings {
            providers: vec![
                Provider::official(),
                Provider {
                    id: "acme".into(),
                    name: "Acme".into(),
                    kind: ProviderKind::External,
                    dialect: Dialect::Responses,
                    base_url: "https://example.com/v1".into(),
                    credential_header: None,
                    headers: BTreeMap::new(),
                    enabled: true,
                    allow_cross_model_previous_response_id: false,
                    models: vec![Model {
                        slug: "same".into(),
                        display_name: String::new(),
                        description: None,
                        context_window: 128_000,
                        accepts_images: false,
                    }],
                },
            ],
            ..Settings::default()
        };
        assert!(merge(&base, &settings).is_err());
    }

    #[test]
    fn extracts_exact_official_slugs() {
        let catalog = json!({"models": [{"slug": "one"}, {"slug": "two"}]});
        assert_eq!(
            model_slugs(&catalog).unwrap(),
            HashSet::from(["one".to_owned(), "two".to_owned()])
        );
    }
}
