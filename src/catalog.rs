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
pub struct CatalogStore {
    path: PathBuf,
    snapshot: RwLock<Option<Snapshot>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogRoute {
    Official,
    Cpa { upstream_model: String },
    AutoReview { cpa_upstream_model: Option<String> },
}

impl CatalogStore {
    pub fn load(path: PathBuf) -> Result<Self> {
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
            snapshot: RwLock::new(snapshot),
        })
    }

    pub fn resolve(&self, model: &str) -> Result<CatalogRoute> {
        let snapshot = self
            .snapshot
            .read()
            .expect("catalog snapshot lock poisoned");
        snapshot
            .as_ref()
            .context("model catalog has not been fetched yet")?
            .routes
            .get(model)
            .cloned()
            .with_context(|| format!("no route for model {model}"))
    }

    pub fn current(&self) -> Option<Value> {
        self.snapshot
            .read()
            .expect("catalog snapshot lock poisoned")
            .as_ref()
            .map(|snapshot| snapshot.catalog.clone())
    }

    pub fn replace(&self, official: &Value, cpa: &Value) -> Result<Value> {
        let catalog = merge(official, cpa)?;
        let routes = route_table_from_merged(&catalog)?;
        let bytes = serde_json::to_vec_pretty(&catalog)?;
        if bytes.len() > MAX_CATALOG_BYTES {
            bail!("merged model catalog exceeds 16 MiB");
        }
        let mut snapshot = self
            .snapshot
            .write()
            .expect("catalog snapshot lock poisoned");
        atomic_write(&self.path, &bytes)?;
        *snapshot = Some(Snapshot {
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
        let is_auto_review = is_cpa_auto_review_model(upstream);
        if source.get("supported_in_api").and_then(Value::as_bool) == Some(false)
            || (!is_auto_review && source.get("visibility").and_then(Value::as_str) == Some("hide"))
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

fn is_cpa_auto_review_model(model: &str) -> bool {
    matches!(model, AUTO_REVIEW_MODEL)
}

fn hide_auto_review_models(catalog: &mut Value) -> Result<()> {
    for model in models_mut(catalog)? {
        let slug =
            model_slug(model).context("merged catalog contains a model without a nonempty slug")?;
        let is_auto_review = slug == AUTO_REVIEW_MODEL
            || slug
                .strip_prefix(CPA_MODEL_PREFIX)
                .is_some_and(is_cpa_auto_review_model);
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
    let catalog_models = models(catalog)?;
    let cpa_auto_review_model = catalog_models.iter().find_map(|model| {
        let slug = model_slug(model)?
            .strip_prefix(CPA_MODEL_PREFIX)?
            .to_owned();
        (slug == AUTO_REVIEW_MODEL).then_some(slug)
    });
    let mut routes = RouteTable::new();
    for model in catalog_models {
        let slug = model_slug(model).context("merged catalog contains a model without a slug")?;
        let route = if slug == AUTO_REVIEW_MODEL {
            CatalogRoute::AutoReview {
                cpa_upstream_model: cpa_auto_review_model.clone(),
            }
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
    if !routes.contains_key(AUTO_REVIEW_MODEL) {
        routes.insert(
            AUTO_REVIEW_MODEL.into(),
            CatalogRoute::AutoReview {
                cpa_upstream_model: cpa_auto_review_model,
            },
        );
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
    fn auto_review_models_are_hidden_and_use_the_cpa_fallback_route() {
        let root = tempdir().unwrap();
        let store = CatalogStore::load(root.path().join("catalog.json")).unwrap();
        let merged = store
            .replace(
                &json!({"models":[{
                    "slug":AUTO_REVIEW_MODEL, "visibility":"list"
                }]}),
                &json!({"models":[{
                    "slug":AUTO_REVIEW_MODEL, "visibility":"hide"
                }]}),
            )
            .unwrap();

        let models = merged["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert!(models.iter().all(|model| model["visibility"] == "hide"));
        assert_eq!(models[0]["slug"], AUTO_REVIEW_MODEL);
        assert_eq!(
            models[1]["slug"],
            format!("{CPA_MODEL_PREFIX}{AUTO_REVIEW_MODEL}")
        );
        assert_eq!(
            store.resolve(AUTO_REVIEW_MODEL).unwrap(),
            CatalogRoute::AutoReview {
                cpa_upstream_model: Some(AUTO_REVIEW_MODEL.into())
            }
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
}
