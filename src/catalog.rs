use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{config::CPA_MODEL_PREFIX, fsutil::atomic_write};

pub const MAX_CATALOG_BYTES: usize = 16 * 1024 * 1024;
pub const AUTO_REVIEW_MODEL: &str = "codex-auto-review";
const MAX_RETENTION_STATE_BYTES: usize = 1024 * 1024;
const RETENTION_STATE_VERSION: u8 = 1;
const CPA_MODEL_REMOVAL_GRACE: Duration = Duration::from_secs(24 * 60 * 60);

type RouteTable = HashMap<String, CatalogRoute>;

#[derive(Debug)]
struct Snapshot {
    catalog: Value,
    routes: RouteTable,
}

#[derive(Debug)]
struct StoreState {
    snapshot: Option<Snapshot>,
    retention: CpaRetentionState,
    retention_initialized: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CpaRetentionState {
    version: u8,
    models: BTreeMap<String, TrackedCpaModel>,
}

impl Default for CpaRetentionState {
    fn default() -> Self {
        Self {
            version: RETENTION_STATE_VERSION,
            models: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TrackedCpaModel {
    #[serde(skip_serializing_if = "Option::is_none")]
    first_missing_unix_seconds: Option<u64>,
}

#[derive(Debug)]
pub struct CatalogStore {
    path: PathBuf,
    retention_path: PathBuf,
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
        let retention_path = retention_path(&path);
        let (retention, retention_initialized) = load_retention_state(&retention_path)?;
        Ok(Self {
            path,
            retention_path,
            state: RwLock::new(StoreState {
                snapshot,
                retention,
                retention_initialized,
            }),
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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_secs();
        self.replace_at(official, cpa, direct, now)
    }

    fn replace_at(
        &self,
        official: &Value,
        cpa: &Value,
        direct: &[DirectModel],
        now_unix_seconds: u64,
    ) -> Result<Value> {
        let mut catalog = merge(official, cpa)?;
        let current_cpa_slugs = cpa_slugs(&catalog)?;
        let mut state = self.state.write().expect("catalog snapshot lock poisoned");
        if !state.retention_initialized {
            if let Some(snapshot) = &state.snapshot {
                state.retention.models = inferred_cpa_models(&snapshot.catalog)?;
            }
            state.retention_initialized = true;
        }
        let previous_catalog = state
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.catalog.clone());
        retain_temporarily_missing_models(
            &mut catalog,
            previous_catalog.as_ref(),
            &mut state.retention,
            &current_cpa_slugs,
            now_unix_seconds,
        )?;
        merge_declared_direct(&mut catalog, direct)?;
        let routes = route_table_from_merged(&catalog)?;
        let bytes = serde_json::to_vec_pretty(&catalog)?;
        if bytes.len() > MAX_CATALOG_BYTES {
            bail!("merged model catalog exceeds 16 MiB");
        }
        let retention_bytes = serde_json::to_vec_pretty(&state.retention)?;
        if retention_bytes.len() > MAX_RETENTION_STATE_BYTES {
            bail!("CPA model retention state exceeds 1 MiB");
        }
        // Persist the timer state first. If the process stops between these
        // two atomic writes, the next refresh can safely reconcile the newer
        // timers with the still-valid older catalog snapshot.
        atomic_write(&self.retention_path, &retention_bytes)?;
        atomic_write(&self.path, &bytes)?;
        state.snapshot = Some(Snapshot {
            catalog: catalog.clone(),
            routes,
        });
        Ok(catalog)
    }
}

fn retention_path(catalog_path: &Path) -> PathBuf {
    catalog_path.with_extension("retention.json")
}

fn load_retention_state(path: &Path) -> Result<(CpaRetentionState, bool)> {
    if !path.exists() {
        return Ok((CpaRetentionState::default(), false));
    }
    let bytes = fs::read(path).with_context(|| {
        format!(
            "failed to read CPA model retention state {}",
            path.display()
        )
    })?;
    if bytes.len() > MAX_RETENTION_STATE_BYTES {
        bail!("CPA model retention state exceeds 1 MiB");
    }
    let state: CpaRetentionState = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid CPA model retention state {}", path.display()))?;
    anyhow::ensure!(
        state.version == RETENTION_STATE_VERSION,
        "unsupported CPA model retention state version {}",
        state.version
    );
    for slug in state.models.keys() {
        anyhow::ensure!(
            slug.strip_prefix(CPA_MODEL_PREFIX)
                .is_some_and(|upstream| !upstream.is_empty()),
            "CPA model retention state contains invalid slug {slug}"
        );
    }
    Ok((state, true))
}

fn cpa_slugs(catalog: &Value) -> Result<HashSet<String>> {
    Ok(models(catalog)?
        .iter()
        .filter_map(model_slug)
        .filter(|slug| slug.starts_with(CPA_MODEL_PREFIX))
        .map(str::to_owned)
        .collect())
}

fn inferred_cpa_models(catalog: &Value) -> Result<BTreeMap<String, TrackedCpaModel>> {
    let mut tracked = BTreeMap::new();
    for model in models(catalog)? {
        let Some(slug) = model_slug(model).filter(|slug| slug.starts_with(CPA_MODEL_PREFIX)) else {
            continue;
        };
        // Older snapshots predate explicit provenance state. The exact small
        // synthetic shape identifies models contributed only by a direct
        // declaration so removing a direct route does not leave it routable.
        if is_synthetic_direct_model(model) {
            continue;
        }
        tracked.insert(slug.to_owned(), TrackedCpaModel::default());
    }
    Ok(tracked)
}

fn is_synthetic_direct_model(model: &Value) -> bool {
    let Some(object) = model.as_object() else {
        return false;
    };
    let Some(slug) = model_slug(model) else {
        return false;
    };
    let Some(upstream) = slug.strip_prefix(CPA_MODEL_PREFIX) else {
        return false;
    };
    object.get("display_name").and_then(Value::as_str)
        == Some(format!("{upstream} · Direct").as_str())
        && object.get("priority").and_then(Value::as_i64).is_some()
        && object.keys().all(|key| {
            matches!(
                key.as_str(),
                "slug" | "display_name" | "priority" | "visibility"
            )
        })
}

fn retain_temporarily_missing_models(
    catalog: &mut Value,
    previous_catalog: Option<&Value>,
    retention: &mut CpaRetentionState,
    current_cpa_slugs: &HashSet<String>,
    now_unix_seconds: u64,
) -> Result<()> {
    for slug in current_cpa_slugs {
        retention
            .models
            .entry(slug.clone())
            .or_default()
            .first_missing_unix_seconds = None;
    }

    let previous_models: HashMap<&str, &Value> = previous_catalog
        .map(models)
        .transpose()?
        .into_iter()
        .flatten()
        .filter_map(|model| model_slug(model).map(|slug| (slug, model)))
        .collect();
    let mut present: HashSet<String> = models(catalog)?
        .iter()
        .filter_map(model_slug)
        .map(str::to_owned)
        .collect();
    let tracked_slugs: Vec<String> = retention.models.keys().cloned().collect();
    let mut retained = Vec::new();
    for slug in tracked_slugs {
        if current_cpa_slugs.contains(&slug) {
            continue;
        }
        let Some(previous_model) = previous_models.get(slug.as_str()) else {
            retention.models.remove(&slug);
            continue;
        };
        let tracked = retention
            .models
            .get_mut(&slug)
            .expect("tracked CPA model disappeared during refresh");
        let first_missing = *tracked
            .first_missing_unix_seconds
            .get_or_insert(now_unix_seconds);
        if now_unix_seconds.saturating_sub(first_missing) < CPA_MODEL_REMOVAL_GRACE.as_secs() {
            if present.insert(slug) {
                retained.push((*previous_model).clone());
            }
        } else {
            retention.models.remove(&slug);
        }
    }
    models_mut(catalog)?.extend(retained);
    Ok(())
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
    fn temporarily_missing_cpa_model_keeps_its_snapshot_metadata() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .replace_at(
                &json!({"models":[{"slug":"official"}]}),
                &json!({"models":[{
                    "slug":"glm-5.3-uni", "display_name":"GLM 5.3 Uni",
                    "context_window":202_752, "future_field":{"kept":true}
                }]}),
                &[],
                1_000,
            )
            .unwrap();

        let retained = store
            .replace_at(
                &json!({"models":[{"slug":"official"}]}),
                &json!({"models":[]}),
                &[],
                1_010,
            )
            .unwrap();
        let model = retained["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|model| model["slug"] == "cpa/glm-5.3-uni")
            .unwrap();
        assert_eq!(model["display_name"], "GLM 5.3 Uni · CPA");
        assert_eq!(model["context_window"], 202_752);
        assert_eq!(model["future_field"]["kept"], true);
        assert_eq!(
            store.resolve("cpa/glm-5.3-uni").unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "glm-5.3-uni".into()
            }
        );

        let persisted: Value =
            serde_json::from_slice(&fs::read(retention_path(&path)).unwrap()).unwrap();
        assert_eq!(
            persisted["models"]["cpa/glm-5.3-uni"]["first_missing_unix_seconds"],
            1_010
        );
    }

    #[test]
    fn missing_cpa_model_expires_after_the_persisted_grace_period() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .replace_at(
                &json!({"models":[]}),
                &json!({"models":[{"slug":"temporary"}]}),
                &[],
                1_000,
            )
            .unwrap();
        store
            .replace_at(&json!({"models":[]}), &json!({"models":[]}), &[], 1_010)
            .unwrap();
        drop(store);

        let restored = CatalogStore::load(path).unwrap();
        let before_deadline = restored
            .replace_at(
                &json!({"models":[]}),
                &json!({"models":[]}),
                &[],
                1_010 + CPA_MODEL_REMOVAL_GRACE.as_secs() - 1,
            )
            .unwrap();
        assert!(
            before_deadline["models"]
                .as_array()
                .unwrap()
                .iter()
                .any(|model| model["slug"] == "cpa/temporary")
        );

        let expired = restored
            .replace_at(
                &json!({"models":[]}),
                &json!({"models":[]}),
                &[],
                1_010 + CPA_MODEL_REMOVAL_GRACE.as_secs(),
            )
            .unwrap();
        assert!(expired["models"].as_array().unwrap().is_empty());
        assert!(restored.resolve("cpa/temporary").is_err());
    }

    #[test]
    fn cpa_model_reappearance_resets_the_missing_deadline() {
        let root = tempdir().unwrap();
        let store = CatalogStore::load(root.path().join("catalog.json")).unwrap();
        let official = json!({"models":[]});
        let present = json!({"models":[{"slug":"returns"}]});
        let absent = json!({"models":[]});
        store.replace_at(&official, &present, &[], 1_000).unwrap();
        store.replace_at(&official, &absent, &[], 1_010).unwrap();
        store.replace_at(&official, &present, &[], 1_020).unwrap();
        store.replace_at(&official, &absent, &[], 1_030).unwrap();

        let after_original_deadline = store
            .replace_at(
                &official,
                &absent,
                &[],
                1_010 + CPA_MODEL_REMOVAL_GRACE.as_secs(),
            )
            .unwrap();
        assert!(
            after_original_deadline["models"]
                .as_array()
                .unwrap()
                .iter()
                .any(|model| model["slug"] == "cpa/returns")
        );
    }

    #[test]
    fn removing_a_direct_declaration_does_not_start_a_grace_period() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let direct = [DirectModel {
            upstream_model: "direct-only".into(),
            base_url: "https://direct.example/v1".into(),
        }];
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .replace_at(&json!({"models":[]}), &json!({"models":[]}), &direct, 1_000)
            .unwrap();
        drop(store);

        // Simulate upgrading an older snapshot that has no provenance state.
        fs::remove_file(retention_path(&path)).unwrap();
        let restored = CatalogStore::load(path).unwrap();
        let refreshed = restored
            .replace_at(&json!({"models":[]}), &json!({"models":[]}), &[], 1_010)
            .unwrap();
        assert!(refreshed["models"].as_array().unwrap().is_empty());
        assert!(restored.resolve("cpa/direct-only").is_err());
    }
}
