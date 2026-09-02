use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::RwLock,
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::{config::CPA_MODEL_PREFIX, fsutil::atomic_write};

pub const MAX_CATALOG_BYTES: usize = 16 * 1024 * 1024;
pub const AUTO_REVIEW_MODEL: &str = "codex-auto-review";

type RouteTable = HashMap<String, CatalogRoute>;

#[derive(Debug)]
struct Snapshot {
    catalog: Value,
    routes: RouteTable,
}

#[derive(Debug)]
struct StoreState {
    snapshot: Option<Snapshot>,
}

#[derive(Debug)]
pub struct CatalogStore {
    path: PathBuf,
    advertise_ultra: bool,
    state: RwLock<StoreState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogRoute {
    Official,
    Cpa { upstream_model: String },
    AutoReview,
}

/// A model declared by a direct route: proxied straight to an upstream even
/// when it is absent from the CPA catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct DirectModel {
    /// Upstream model slug (no `cpa/` prefix).
    pub upstream_model: String,
    /// Upstream base URL, used to group models per route in listings.
    pub base_url: String,
}

impl CatalogStore {
    pub fn load(path: PathBuf) -> Result<Self> {
        Self::load_with_options(path, false)
    }

    pub fn load_with_options(path: PathBuf, advertise_ultra: bool) -> Result<Self> {
        let snapshot = if path.exists() {
            let bytes = fs::read(&path)
                .with_context(|| format!("failed to read catalog snapshot {}", path.display()))?;
            if bytes.len() > MAX_CATALOG_BYTES {
                bail!("catalog snapshot exceeds 16 MiB");
            }
            let mut catalog: Value = serde_json::from_slice(&bytes)
                .with_context(|| format!("invalid catalog snapshot {}", path.display()))?;
            hide_auto_review_models(&mut catalog)?;
            Some(Snapshot {
                routes: route_table_from_merged(&catalog)?,
                catalog,
            })
        } else {
            None
        };
        Ok(Self {
            path,
            advertise_ultra,
            state: RwLock::new(StoreState { snapshot }),
        })
    }

    pub fn resolve(&self, model: &str) -> Result<CatalogRoute> {
        let state = self.state.read().expect("catalog snapshot lock poisoned");
        state
            .snapshot
            .as_ref()
            .context("model catalog has not been fetched yet")?
            .routes
            .get(model)
            .cloned()
            .with_context(|| format!("no route for model {model}"))
    }

    /// Resolve a model, falling back to declared direct routes when the
    /// catalog has no snapshot at all. Existing snapshots stay authoritative:
    /// a slug present in the snapshot resolves exactly as stored.
    pub fn resolve_with_direct(&self, model: &str, direct: &[DirectModel]) -> Result<CatalogRoute> {
        if let Ok(route) = self.resolve(model) {
            return Ok(route);
        }
        let upstream = model
            .strip_prefix(CPA_MODEL_PREFIX)
            .filter(|upstream| !upstream.is_empty());
        if let Some(upstream) = upstream
            && direct.iter().any(|entry| entry.upstream_model == upstream)
        {
            return Ok(CatalogRoute::Cpa {
                upstream_model: upstream.to_owned(),
            });
        }
        self.resolve(model)
    }

    pub fn current(&self) -> Option<Value> {
        self.state
            .read()
            .expect("catalog snapshot lock poisoned")
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.catalog.clone())
    }

    pub fn replace(&self, official: &Value, cpa: &Value, direct: &[DirectModel]) -> Result<Value> {
        let mut catalog = merge(official, cpa)?;
        merge_declared_direct(&mut catalog, direct)?;
        if self.advertise_ultra {
            advertise_ultra_all(&mut catalog)?;
        }
        let routes = route_table_from_merged(&catalog)?;
        let bytes = serde_json::to_vec_pretty(&catalog)?;
        if bytes.len() > MAX_CATALOG_BYTES {
            bail!("merged model catalog exceeds 16 MiB");
        }
        atomic_write(&self.path, &bytes)?;
        let mut state = self.state.write().expect("catalog snapshot lock poisoned");
        state.snapshot = Some(Snapshot {
            catalog: catalog.clone(),
            routes,
        });
        Ok(catalog)
    }
}

pub fn parse(bytes: &[u8], source: &str) -> Result<Value> {
    if bytes.len() > MAX_CATALOG_BYTES {
        bail!("{source} model catalog exceeds 16 MiB");
    }
    let value: Value =
        serde_json::from_slice(bytes).with_context(|| format!("{source} returned invalid JSON"))?;
    validate_catalog(&value).with_context(|| format!("invalid {source} model catalog"))?;
    Ok(value)
}

/// Merge only the official catalog with declared direct-route models. Used
/// when CPA is unreachable (or never installed) so Codex still gets a usable
/// model list. The result is served but never persisted: a stored snapshot
/// still requires both upstream catalogs to validate.
pub fn merge_official_direct(official: &Value, direct: &[DirectModel]) -> Result<Value> {
    validate_catalog(official).context("invalid official model catalog")?;
    let mut output = official.clone();
    merge_declared_direct(&mut output, direct)?;
    Ok(output)
}

/// Degraded-view variant that also advertises the Codex-side `ultra` preset.
pub fn merge_official_direct_with_ultra(official: &Value, direct: &[DirectModel]) -> Result<Value> {
    let mut output = merge_official_direct(official, direct)?;
    advertise_ultra_all(&mut output)?;
    Ok(output)
}

/// Codex maps the `ultra` preset to a real model-supported effort before the
/// request leaves the client, so advertising it only changes catalog metadata.
fn advertise_ultra_all(catalog: &mut Value) -> Result<()> {
    for model in models_mut(catalog)? {
        let object = model
            .as_object_mut()
            .context("merged catalog contains a non-object model")?;
        let levels = object
            .entry("supported_reasoning_levels")
            .or_insert_with(|| json!([]));
        let levels = levels
            .as_array_mut()
            .context("supported_reasoning_levels must be an array")?;
        if levels.is_empty() {
            for (effort, description) in [
                ("low", "Fast responses with lighter reasoning"),
                (
                    "medium",
                    "Balances speed and reasoning depth for everyday tasks",
                ),
                ("high", "Greater reasoning depth for complex problems"),
                (
                    "max",
                    "Maximum available reasoning depth for complex problems",
                ),
            ] {
                levels.push(json!({ "effort": effort, "description": description }));
            }
        }
        let has_ultra = levels.iter().any(|entry| {
            entry.get("effort").and_then(Value::as_str) == Some(ULTRA_REASONING_EFFORT)
        });
        if !has_ultra {
            levels.push(json!({
                "effort": ULTRA_REASONING_EFFORT,
                "description": "Maximum reasoning with proactive multi-agent delegation"
            }));
        }
    }
    Ok(())
}

const ULTRA_REASONING_EFFORT: &str = "ultra";

/// Append direct-route model declarations to an already-merged catalog so
/// declared models stay routable even when CPA's catalog omits them.
fn merge_declared_direct(catalog: &mut Value, direct: &[DirectModel]) -> Result<()> {
    if direct.is_empty() {
        return Ok(());
    }
    let output_models = models_mut(catalog)?;
    let mut known: HashSet<String> = output_models
        .iter()
        .filter_map(model_slug)
        .map(str::to_owned)
        .collect();
    let max_priority = output_models
        .iter()
        .filter_map(|model| model.get("priority").and_then(Value::as_i64))
        .max()
        .unwrap_or(0);
    let mut next_priority = max_priority.saturating_add(100);
    for model in direct {
        let local = format!("{CPA_MODEL_PREFIX}{}", model.upstream_model);
        if !known.insert(local.clone()) {
            continue;
        }
        let mut declared = json!({
            "slug": local,
            "display_name": format!("{} · Direct", model.upstream_model),
            "priority": next_priority,
        });
        next_priority = next_priority.saturating_add(1);
        if model.upstream_model == AUTO_REVIEW_MODEL {
            declared
                .as_object_mut()
                .expect("declared direct model is an object")
                .insert("visibility".into(), json!("hide"));
        }
        output_models.push(declared);
    }
    Ok(())
}

pub fn merge(official: &Value, cpa: &Value) -> Result<Value> {
    validate_catalog(official).context("invalid official model catalog")?;
    validate_catalog(cpa).context("invalid CPA model catalog")?;

    let mut output = official.clone();
    let output_models = models_mut(&mut output)?;
    let official_slugs: HashSet<String> = output_models
        .iter()
        .filter_map(model_slug)
        .map(str::to_owned)
        .collect();
    if let Some(reserved) = official_slugs
        .iter()
        .find(|slug| slug.starts_with(CPA_MODEL_PREFIX))
    {
        bail!("official model catalog reserves CPA namespace slug {reserved}");
    }
    if let Some(official_auto_review) = output_models
        .iter_mut()
        .find(|model| model_slug(model) == Some(AUTO_REVIEW_MODEL))
    {
        official_auto_review
            .as_object_mut()
            .context("official auto-review model is not an object")?
            .insert("visibility".into(), json!("hide"));
    }
    let max_priority = output_models
        .iter()
        .filter_map(|model| model.get("priority").and_then(Value::as_i64))
        .max()
        .unwrap_or(0);

    let mut seen_upstream = HashSet::new();
    for (index, source) in models(cpa)?.iter().enumerate() {
        let upstream = model_slug(source)
            .context("CPA model catalog contains a model without a nonempty slug")?;
        if !seen_upstream.insert(upstream.to_owned()) {
            bail!("CPA model catalog contains duplicate slug {upstream}");
        }
        let is_auto_review = upstream == AUTO_REVIEW_MODEL;
        if source.get("visibility").and_then(Value::as_str) == Some("hide") && !is_auto_review
            || source.get("supported_in_api").and_then(Value::as_bool) == Some(false)
        {
            continue;
        }
        let local = format!("{CPA_MODEL_PREFIX}{upstream}");
        let mut model = source.clone();
        let object = model
            .as_object_mut()
            .context("CPA model catalog contains a non-object model")?;
        object.insert("slug".into(), json!(local));
        let display_name = object
            .get("display_name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(upstream);
        object.insert(
            "display_name".into(),
            json!(format!("{display_name} · CPA")),
        );
        object.insert(
            "priority".into(),
            json!(
                max_priority
                    .saturating_add(100)
                    .saturating_add(index as i64)
            ),
        );
        if is_auto_review {
            object.insert("visibility".into(), json!("hide"));
        }
        output_models.push(model);
    }
    Ok(output)
}

fn hide_auto_review_models(catalog: &mut Value) -> Result<()> {
    for model in models_mut(catalog)? {
        let slug =
            model_slug(model).context("merged catalog contains a model without a nonempty slug")?;
        let is_auto_review = slug == AUTO_REVIEW_MODEL
            || slug
                .strip_prefix(CPA_MODEL_PREFIX)
                .is_some_and(|model| model == AUTO_REVIEW_MODEL);
        if is_auto_review {
            model
                .as_object_mut()
                .context("auto-review model is not an object")?
                .insert("visibility".into(), json!("hide"));
        }
    }
    Ok(())
}

fn route_table_from_merged(catalog: &Value) -> Result<RouteTable> {
    let mut routes = RouteTable::new();
    for model in models(catalog)? {
        let slug = model_slug(model).context("merged catalog contains a model without a slug")?;
        let route = if slug == AUTO_REVIEW_MODEL {
            CatalogRoute::AutoReview
        } else if let Some(upstream) = slug.strip_prefix(CPA_MODEL_PREFIX) {
            if upstream.is_empty() {
                bail!("merged catalog contains an invalid CPA slug {slug}");
            }
            CatalogRoute::Cpa {
                upstream_model: upstream.to_owned(),
            }
        } else {
            CatalogRoute::Official
        };
        if routes.insert(slug.to_owned(), route).is_some() {
            bail!("merged catalog contains duplicate slug {slug}");
        }
    }
    Ok(routes)
}

fn validate_catalog(catalog: &Value) -> Result<()> {
    let mut slugs = HashSet::new();
    for model in models(catalog)? {
        let slug =
            model_slug(model).context("model catalog contains a model without a nonempty slug")?;
        if !slugs.insert(slug) {
            bail!("model catalog contains duplicate slug {slug}");
        }
    }
    Ok(())
}

fn models(catalog: &Value) -> Result<&Vec<Value>> {
    catalog
        .get("models")
        .and_then(Value::as_array)
        .context("model catalog has no models array")
}

fn models_mut(catalog: &mut Value) -> Result<&mut Vec<Value>> {
    catalog
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .context("model catalog has no models array")
}

fn model_slug(model: &Value) -> Option<&str> {
    model
        .as_object()?
        .get("slug")?
        .as_str()
        .map(str::trim)
        .filter(|slug| !slug.is_empty())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn merges_full_cpa_metadata_and_namespaces_only_identity() {
        let official = json!({"models":[{
            "slug":"gpt-5.6", "display_name":"5.6", "priority":3,
            "supported_reasoning_levels":[{"effort":"high"}]
        }]});
        let cpa = json!({"models":[{
            "slug":"gpt-5.6", "display_name":"5.6", "priority":1,
            "context_window":1000000, "input_modalities":["text","image"],
            "unknown_future_field":{"kept":true}
        }, {
            "slug":"hidden", "visibility":"hide"
        }]});

        let merged = merge(&official, &cpa).unwrap();
        assert_eq!(merged["models"].as_array().unwrap().len(), 2);
        assert_eq!(merged["models"][0]["slug"], "gpt-5.6");
        assert_eq!(merged["models"][1]["slug"], "cpa/gpt-5.6");
        assert_eq!(merged["models"][1]["display_name"], "5.6 · CPA");
        assert_eq!(merged["models"][1]["context_window"], 1_000_000);
        assert_eq!(merged["models"][1]["unknown_future_field"]["kept"], true);
    }

    #[test]
    fn ultra_advertisement_applies_to_every_merged_entry_when_enabled() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load_with_options(path.clone(), true).unwrap();
        store
            .replace(
                &json!({"models":[{"slug":"gpt-5.6","priority":1,"supported_reasoning_levels":[{"effort":"high","description":"High"}]}]}),
                &json!({"models":[{"slug":"deepseek","priority":1,"supported_reasoning_levels":[{"effort":"max","description":"Max"}]}]}),
                &[DirectModel {
                    upstream_model: "direct-model".into(),
                    base_url: "https://direct.example/v1".into(),
                }],
            )
            .unwrap();
        let catalog = store.current().unwrap();
        let models = catalog["models"].as_array().unwrap();
        for slug in ["gpt-5.6", "cpa/deepseek", "cpa/direct-model"] {
            let model = models.iter().find(|model| model["slug"] == slug).unwrap();
            let has_ultra = model["supported_reasoning_levels"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["effort"] == "ultra");
            assert!(has_ultra, "{slug} did not advertise ultra");
        }
    }

    #[test]
    fn ultra_advertisement_remains_off_by_default() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .replace(
                &json!({"models":[{"slug":"gpt-5.6","priority":1,"supported_reasoning_levels":[{"effort":"high","description":"High"}]}]}),
                &json!({"models":[]}),
                &[],
            )
            .unwrap();
        let catalog = store.current().unwrap();
        let model = &catalog["models"][0];
        assert!(
            !model["supported_reasoning_levels"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["effort"] == "ultra")
        );
    }

    #[test]
    fn snapshot_restores_exact_routes_after_restart() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .replace(
                &json!({"models":[{"slug":"gpt-5.6"}]}),
                &json!({"models":[{"slug":"gpt-5.6"},{"slug":"claude"}]}),
                &[],
            )
            .unwrap();
        drop(store);

        let restored = CatalogStore::load(path).unwrap();
        assert_eq!(restored.resolve("gpt-5.6").unwrap(), CatalogRoute::Official);
        assert_eq!(
            restored.resolve("cpa/gpt-5.6").unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "gpt-5.6".into()
            }
        );
        assert!(restored.resolve("unknown").is_err());
    }

    #[test]
    fn refuses_official_use_of_the_cpa_namespace() {
        let error = merge(
            &json!({"models":[{"slug":"cpa/claude"}]}),
            &json!({"models":[{"slug":"claude"}]}),
        )
        .unwrap_err();
        assert!(error.to_string().contains("reserves CPA namespace"));
    }

    #[test]
    fn declared_direct_models_merge_into_the_stored_catalog() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .replace(
                &json!({"models":[{"slug":"gpt-5.6","priority":3}]}),
                &json!({"models":[]}),
                &[DirectModel {
                    upstream_model: "gpt-5.6-sol".into(),
                    base_url: "https://direct.example/v1".into(),
                }],
            )
            .unwrap();
        assert_eq!(
            store.resolve("cpa/gpt-5.6-sol").unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "gpt-5.6-sol".into()
            }
        );
        let catalog = store.current().unwrap();
        let merged = catalog["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|model| model["slug"] == "cpa/gpt-5.6-sol")
            .unwrap();
        assert_eq!(merged["display_name"], "gpt-5.6-sol · Direct");
        // A CPA model with the same slug wins over the declaration.
        drop(store);
        let store = CatalogStore::load(path).unwrap();
        store
            .replace(
                &json!({"models":[]}),
                &json!({"models":[{"slug":"gpt-5.6-sol","display_name":"Sol"}]}),
                &[DirectModel {
                    upstream_model: "gpt-5.6-sol".into(),
                    base_url: "https://direct.example/v1".into(),
                }],
            )
            .unwrap();
        let catalog = store.current().unwrap();
        let from_cpa = catalog["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|model| model["slug"] == "cpa/gpt-5.6-sol")
            .unwrap();
        assert_eq!(from_cpa["display_name"], "Sol · CPA");
    }

    #[test]
    fn merge_official_direct_serves_without_cpa_and_respects_declarations() {
        let official = json!({"models":[{"slug":"gpt-5.6","priority":1}]});
        let direct = [DirectModel {
            upstream_model: "gpt-5.6-sol".into(),
            base_url: "https://direct.example/v1".into(),
        }];
        let merged = merge_official_direct(&official, &direct).unwrap();
        let slugs: Vec<&str> = merged["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["slug"].as_str().unwrap())
            .collect();
        assert_eq!(slugs, vec!["gpt-5.6", "cpa/gpt-5.6-sol"]);
        let routes = route_table_from_merged(&merged).unwrap();
        assert_eq!(routes["gpt-5.6"], CatalogRoute::Official);
        assert_eq!(
            routes["cpa/gpt-5.6-sol"],
            CatalogRoute::Cpa {
                upstream_model: "gpt-5.6-sol".into()
            }
        );
    }

    #[test]
    fn resolve_with_direct_falls_back_when_no_snapshot_exists() {
        let root = tempdir().unwrap();
        let store = CatalogStore::load(root.path().join("catalog.json")).unwrap();
        let direct = [DirectModel {
            upstream_model: "gpt-5.6-sol".into(),
            base_url: "https://direct.example/v1".into(),
        }];
        assert!(store.resolve("cpa/gpt-5.6-sol").is_err());
        assert_eq!(
            store
                .resolve_with_direct("cpa/gpt-5.6-sol", &direct)
                .unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "gpt-5.6-sol".into()
            }
        );
        assert!(store.resolve_with_direct("cpa/other", &direct).is_err());
        assert!(store.resolve_with_direct("gpt-5.6", &direct).is_err());
    }

    #[test]
    fn resolve_with_direct_prefers_a_stored_snapshot() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .replace(
                &json!({"models":[]}),
                &json!({"models":[{"slug":"declared"}]}),
                &[DirectModel {
                    upstream_model: "declared".into(),
                    base_url: "https://direct.example/v1".into(),
                }],
            )
            .unwrap();
        let direct = [DirectModel {
            upstream_model: "declared".into(),
            base_url: "https://other.example/v1".into(),
        }];
        // The snapshot already routes cpa/declared; the fallback never wins.
        assert_eq!(
            store.resolve_with_direct("cpa/declared", &direct).unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "declared".into()
            }
        );
        assert!(store.resolve_with_direct("cpa/absent", &direct).is_err());
    }

    #[test]
    fn cpa_model_omission_drops_the_model_immediately() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .replace(
                &json!({"models":[]}),
                &json!({"models":[{
                    "slug":"glm-5.3-uni", "display_name":"GLM 5.3 Uni"
                }]}),
                &[],
            )
            .unwrap();
        assert!(store.resolve("cpa/glm-5.3-uni").is_ok());

        let after_omission = store
            .replace(&json!({"models":[]}), &json!({"models":[]}), &[])
            .unwrap();
        assert!(
            after_omission["models"]
                .as_array()
                .unwrap()
                .iter()
                .all(|model| model["slug"] != "cpa/glm-5.3-uni")
        );
        assert!(store.resolve("cpa/glm-5.3-uni").is_err());
    }
}
