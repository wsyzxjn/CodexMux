use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt, fs,
    path::PathBuf,
    sync::{Mutex, RwLock},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{config::CPA_MODEL_PREFIX, fsutil::atomic_write};

pub const MAX_CATALOG_BYTES: usize = 16 * 1024 * 1024;
pub const AUTO_REVIEW_MODEL: &str = "codex-auto-review";

type RouteTable = HashMap<String, CatalogRoute>;

#[derive(Debug)]
struct Snapshot {
    catalog: Value,
    routes: RouteTable,
    /// True when the view was built from both upstream catalogs. An
    /// official-only view exists only so official models stay routable before
    /// any complete catalog has been seen, and it never replaces one.
    complete: bool,
}

#[derive(Debug)]
struct StoreState {
    snapshot: Option<Snapshot>,
    official: Option<Value>,
}

#[derive(Debug)]
pub struct CatalogStore {
    path: PathBuf,
    state: RwLock<StoreState>,
    /// Serializes every view change so the persisted snapshot and the
    /// in-memory view are always installed in the same order.
    updates: Mutex<()>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogRoute {
    Official,
    Cpa { upstream_model: String },
    AutoReview,
}

impl fmt::Display for CatalogRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Official => formatter.write_str("official"),
            Self::Cpa { upstream_model } => write!(formatter, "cpa:{upstream_model}"),
            Self::AutoReview => formatter.write_str("auto-review"),
        }
    }
}

/// Optional serve-time metadata for a merged catalog model. These fields do
/// not affect routing and are intentionally not persisted into the upstream
/// snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct CatalogModelOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_window: Option<u64>,
}

impl CatalogModelOverride {
    pub fn is_empty(&self) -> bool {
        self.context_window.is_none() && self.max_context_window.is_none()
    }
}

impl CatalogStore {
    /// Load the persisted snapshot. A snapshot that cannot be read is only a
    /// cache of upstream data, so it is ignored with a warning and rebuilt by
    /// the next complete refresh instead of preventing startup.
    pub fn load(path: PathBuf) -> Result<Self> {
        let snapshot = if path.exists() {
            match load_snapshot(&path) {
                Ok(snapshot) => Some(snapshot),
                Err(error) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %format!("{error:#}"),
                        "ignoring unreadable catalog snapshot"
                    );
                    None
                }
            }
        } else {
            None
        };
        let official = snapshot
            .as_ref()
            .map(|snapshot| official_catalog_from_merged(&snapshot.catalog))
            .transpose()?;
        Ok(Self {
            path,
            state: RwLock::new(StoreState { snapshot, official }),
            updates: Mutex::new(()),
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

    pub fn current(&self) -> Option<Value> {
        self.state
            .read()
            .expect("catalog snapshot lock poisoned")
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.catalog.clone())
    }

    /// True once a valid official catalog is known. The CPA-only background
    /// sync may then refresh the in-memory view from this base, but it must
    /// not persist without validating both catalogs.
    pub fn has_validated_official(&self) -> bool {
        self.state
            .read()
            .expect("catalog snapshot lock poisoned")
            .official
            .is_some()
    }

    /// Install a complete view built from both upstream catalogs and persist
    /// it. Unchanged bytes are not rewritten.
    pub fn replace(&self, official: &Value, cpa: &Value) -> Result<Value> {
        let _update = self.updates.lock().expect("catalog update lock poisoned");
        let catalog = merge(official, cpa)?;
        let routes = route_table_from_merged(&catalog)?;
        let bytes = serde_json::to_vec_pretty(&catalog)?;
        if bytes.len() > MAX_CATALOG_BYTES {
            bail!("merged model catalog exceeds 16 MiB");
        }
        if fs::read(&self.path).ok().as_deref() != Some(bytes.as_slice()) {
            atomic_write(&self.path, &bytes)?;
        }
        let mut state = self.state.write().expect("catalog snapshot lock poisoned");
        state.snapshot = Some(Snapshot {
            catalog: catalog.clone(),
            routes,
            complete: true,
        });
        state.official = Some(official.clone());
        Ok(catalog)
    }

    /// Refresh the served catalog and route table from the latest CPA catalog
    /// without writing a new snapshot. The persisted snapshot still requires a
    /// full refresh that validates both upstream catalogs.
    pub fn replace_memory_from_cpa(&self, cpa: &Value) -> Result<Value> {
        let _update = self.updates.lock().expect("catalog update lock poisoned");
        let official = self
            .state
            .read()
            .expect("catalog snapshot lock poisoned")
            .official
            .clone()
            .context("official model catalog has not been validated yet")?;
        let catalog = merge(&official, cpa)?;
        let routes = route_table_from_merged(&catalog)?;
        let mut state = self.state.write().expect("catalog snapshot lock poisoned");
        state.snapshot = Some(Snapshot {
            catalog: catalog.clone(),
            routes,
            complete: true,
        });
        Ok(catalog)
    }

    /// The official-only view served while CPA is unavailable. It is never
    /// persisted. Its routes are installed only when no complete view exists,
    /// so official models work before CPA has ever answered, while a CPA
    /// outage never removes the `cpa/` routes of the last complete view.
    pub fn official_only(&self, official: &Value) -> Result<Value> {
        let _update = self.updates.lock().expect("catalog update lock poisoned");
        let catalog = prepare_official(official)?;
        let routes = route_table_from_merged(&catalog)?;
        let mut state = self.state.write().expect("catalog snapshot lock poisoned");
        state.official = Some(official.clone());
        if state
            .snapshot
            .as_ref()
            .is_none_or(|snapshot| !snapshot.complete)
        {
            state.snapshot = Some(Snapshot {
                catalog: catalog.clone(),
                routes,
                complete: false,
            });
        }
        Ok(catalog)
    }
}

fn load_snapshot(path: &std::path::Path) -> Result<Snapshot> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read catalog snapshot {}", path.display()))?;
    if bytes.len() > MAX_CATALOG_BYTES {
        bail!("catalog snapshot exceeds 16 MiB");
    }
    let mut catalog: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid catalog snapshot {}", path.display()))?;
    hide_internal_models(&mut catalog)?;
    Ok(Snapshot {
        routes: route_table_from_merged(&catalog)?,
        catalog,
        complete: true,
    })
}

fn official_catalog_from_merged(catalog: &Value) -> Result<Value> {
    let mut official = catalog.clone();
    let models = models_mut(&mut official)?;
    models
        .retain(|model| model_slug(model).is_some_and(|slug| !slug.starts_with(CPA_MODEL_PREFIX)));
    Ok(official)
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

/// Advertise the Codex-side `ultra` preset in a served catalog. Codex maps the
/// preset to a real model-supported effort before the request leaves the
/// client, so only models that already declare reasoning levels gain it; a
/// model without levels keeps its upstream metadata untouched.
pub fn advertise_ultra(catalog: &mut Value) -> Result<()> {
    for model in models_mut(catalog)? {
        let object = model
            .as_object_mut()
            .context("catalog contains a non-object model")?;
        let Some(levels) = object
            .get_mut("supported_reasoning_levels")
            .and_then(Value::as_array_mut)
            .filter(|levels| !levels.is_empty())
        else {
            continue;
        };
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

/// Advertise search support for every model in a merged catalog so the shared
/// `web_search` backend can serve models that do not expose native search.
pub fn advertise_search_all(catalog: &mut Value) -> Result<()> {
    for model in models_mut(catalog)? {
        let object = model
            .as_object_mut()
            .context("catalog contains a non-object model")?;
        object.insert("supports_search_tool".into(), json!(true));
        object
            .entry("web_search_tool_type")
            .or_insert_with(|| json!("text_and_image"));
    }
    Ok(())
}

const ULTRA_REASONING_EFFORT: &str = "ultra";
const COMP_HASH_FIELD: &str = "comp_hash";

/// Serve one shared compaction-compatibility hash across the merged catalog.
///
/// Codex runs a pre-sampling compaction whenever two consecutive turns
/// advertise different `comp_hash` values, and it sends that compaction to the
/// *previous* model. Upstream metadata mixes hashes across families (the 5.6
/// family and the CPA catalog disagree today), so switching model
/// mid-conversation asks the model being left behind to compact first. When
/// that model is the exhausted one the user is trying to escape, the
/// conversation cannot continue at all.
///
/// The shared value tracks the official catalog rather than a constant: when
/// upstream really does change its compaction format, every model rotates
/// together and Codex still recompacts exactly once. A merged catalog without
/// any official hash drops the field, which Codex reads as "no information"
/// and never compacts on.
pub fn unify_comp_hash_all(catalog: &mut Value) -> Result<()> {
    let reference = default_official_comp_hash(catalog)?;
    for model in models_mut(catalog)? {
        let object = model
            .as_object_mut()
            .context("catalog contains a non-object model")?;
        match reference.as_deref() {
            Some(hash) => object.insert(COMP_HASH_FIELD.into(), json!(hash)),
            None => object.remove(COMP_HASH_FIELD),
        };
    }
    Ok(())
}

/// Apply local serve-time metadata overrides to exact merged catalog slugs.
pub fn apply_model_overrides(
    catalog: &mut Value,
    overrides: &BTreeMap<String, CatalogModelOverride>,
) -> Result<()> {
    if overrides.is_empty() {
        return Ok(());
    }
    for model in models_mut(catalog)? {
        let slug =
            model_slug(model).context("merged catalog contains a model without a nonempty slug")?;
        let Some(overrides) = overrides.get(slug) else {
            continue;
        };
        if overrides.is_empty() {
            continue;
        }
        let object = model
            .as_object_mut()
            .context("merged catalog contains a non-object model")?;
        if let Some(window) = overrides.context_window {
            object.insert("context_window".into(), json!(window));
            if overrides.max_context_window.is_none() {
                let current_max = object
                    .get("max_context_window")
                    .and_then(Value::as_u64)
                    .unwrap_or(window);
                object.insert("max_context_window".into(), json!(window.max(current_max)));
            }
        }
        if let Some(window) = overrides.max_context_window {
            object.insert("max_context_window".into(), json!(window));
        }
    }
    Ok(())
}

/// The hash Codex already applies to its default official model: the listed
/// official entry with the strongest display precedence. Hidden entries are a
/// last resort, and catalog order breaks ties so the choice stays stable
/// across refreshes.
fn default_official_comp_hash(catalog: &Value) -> Result<Option<String>> {
    let mut best: Option<((bool, i64), &str)> = None;
    for model in models(catalog)? {
        let Some(slug) = model_slug(model) else {
            continue;
        };
        if slug.starts_with(CPA_MODEL_PREFIX) {
            continue;
        }
        let Some(hash) = model
            .get(COMP_HASH_FIELD)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|hash| !hash.is_empty())
        else {
            continue;
        };
        let hidden = model.get("visibility").and_then(Value::as_str) == Some("hide");
        let priority = model
            .get("priority")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX);
        let rank = (hidden, priority);
        if best.as_ref().is_none_or(|(best_rank, _)| rank < *best_rank) {
            best = Some((rank, hash));
        }
    }
    Ok(best.map(|(_, hash)| hash.to_owned()))
}

/// Validate the official catalog for serving: slugs are unique, the `cpa/`
/// namespace stays reserved, and internal review and image models are hidden
/// from the picker without losing their routes.
fn prepare_official(official: &Value) -> Result<Value> {
    validate_catalog(official).context("invalid official model catalog")?;
    let mut output = official.clone();
    for model in models_mut(&mut output)? {
        let slug = model_slug(model)
            .context("official model catalog contains a model without a nonempty slug")?;
        if slug.starts_with(CPA_MODEL_PREFIX) {
            bail!("official model catalog reserves CPA namespace slug {slug}");
        }
        if should_hide_from_picker(slug) {
            model
                .as_object_mut()
                .context("internal official model is not an object")?
                .insert("visibility".into(), json!("hide"));
        }
    }
    Ok(output)
}

pub fn merge(official: &Value, cpa: &Value) -> Result<Value> {
    validate_catalog(cpa).context("invalid CPA model catalog")?;
    let mut output = prepare_official(official)?;
    let output_models = models_mut(&mut output)?;
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
        let is_internal = should_hide_from_picker(upstream);
        if source.get("visibility").and_then(Value::as_str) == Some("hide") && !is_internal
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
        if is_internal {
            object.insert("visibility".into(), json!("hide"));
        }
        output_models.push(model);
    }
    Ok(output)
}

fn hide_internal_models(catalog: &mut Value) -> Result<()> {
    for model in models_mut(catalog)? {
        let slug =
            model_slug(model).context("merged catalog contains a model without a nonempty slug")?;
        let upstream = slug.strip_prefix(CPA_MODEL_PREFIX).unwrap_or(slug);
        if should_hide_from_picker(upstream) {
            model
                .as_object_mut()
                .context("internal model is not an object")?
                .insert("visibility".into(), json!("hide"));
        }
    }
    Ok(())
}

fn should_hide_from_picker(slug: &str) -> bool {
    let slug = slug.trim().to_ascii_lowercase().replace([' ', '_'], "-");
    is_auto_review_slug(&slug) || is_image_model_slug(&slug)
}

fn is_auto_review_slug(slug: &str) -> bool {
    slug == AUTO_REVIEW_MODEL || slug.ends_with("-codex-auto-review")
}

fn is_image_model_slug(slug: &str) -> bool {
    slug.contains("gpt-image") || slug.contains("imagine-image")
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
    fn hides_review_and_image_models_without_dropping_their_routes() {
        let official = json!({"models":[
            {"slug":"gpt-6-astra", "visibility":"list", "priority":1},
            {"slug":"codex-auto-review", "visibility":"list", "priority":2}
        ]});
        let cpa = json!({"models":[
            {"slug":"codeapi-codex-auto-review", "display_name":"Codex Auto Review · CodeAPI"},
            {"slug":"acid-cpa-codex-auto-review", "display_name":"Codex Auto Review · Acid CPA"},
            {"slug":"codeapi-gpt-image-2", "display_name":"GPT Image 2 · CodeAPI"},
            {"slug":"grok-imagine-image-quality", "display_name":"Grok Imagine Image Quality"},
            {"slug":"codeapi-gpt-6-astra", "display_name":"GPT 6.0 Astra · CodeAPI"}
        ]});

        let merged = merge(&official, &cpa).unwrap();
        let visibility = |slug: &str| {
            merged["models"]
                .as_array()
                .unwrap()
                .iter()
                .find(|model| model["slug"] == slug)
                .and_then(|model| model.get("visibility"))
                .cloned()
                .unwrap_or_else(|| json!("list"))
        };
        assert_eq!(visibility("codex-auto-review"), "hide");
        for slug in [
            "cpa/codeapi-codex-auto-review",
            "cpa/acid-cpa-codex-auto-review",
            "cpa/codeapi-gpt-image-2",
            "cpa/grok-imagine-image-quality",
        ] {
            assert_eq!(visibility(slug), "hide", "{slug} stayed visible");
        }
        assert_eq!(visibility("cpa/codeapi-gpt-6-astra"), "list");

        let routes = route_table_from_merged(&merged).unwrap();
        assert!(routes.contains_key("cpa/codeapi-codex-auto-review"));
        assert!(routes.contains_key("cpa/codeapi-gpt-image-2"));
    }

    /// Codex compacts on the *previous* model whenever consecutive turns
    /// disagree about `comp_hash`, so every route must report the value Codex
    /// already uses for its default official model.
    #[test]
    fn comp_hash_unification_adopts_the_default_official_hash() {
        let mut catalog = json!({"models":[
            {"slug":"gpt-reserve", "priority":3, "visibility":"hide", "comp_hash":"4200"},
            {"slug":"gpt-5.6-sol", "priority":6, "comp_hash":"3000"},
            {"slug":"gpt-5.5", "priority":12, "comp_hash":"2911"},
            {"slug":"cpa/claude-opus-5", "priority":143, "display_name":"Opus · CPA", "comp_hash":"2911"},
            {"slug":"cpa/glm-5.3-flash", "priority":151}
        ]});
        unify_comp_hash_all(&mut catalog).unwrap();
        let models = catalog["models"].as_array().unwrap();
        for model in models {
            assert_eq!(
                model["comp_hash"], "3000",
                "{} kept a mismatched comp_hash",
                model["slug"]
            );
        }
        // Identity and ordering metadata stay exactly as merged.
        assert_eq!(models[3]["slug"], "cpa/claude-opus-5");
        assert_eq!(models[3]["display_name"], "Opus · CPA");
        assert_eq!(models[3]["priority"], 143);
    }

    /// A missing hash is "no information" to Codex, so a catalog without any
    /// official hash drops the field instead of inventing one.
    #[test]
    fn comp_hash_unification_drops_the_field_without_an_official_hash() {
        let mut catalog = json!({"models":[
            {"slug":"gpt-5.6-sol", "priority":6},
            {"slug":"cpa/claude-opus-5", "priority":143, "comp_hash":"2911"}
        ]});
        unify_comp_hash_all(&mut catalog).unwrap();
        for model in catalog["models"].as_array().unwrap() {
            assert!(
                model.get("comp_hash").is_none(),
                "{} still advertises a comp_hash",
                model["slug"]
            );
        }
    }

    #[test]
    fn search_advertisement_flags_every_model() {
        let mut catalog = json!({"models":[
            {"slug":"custom-a", "supports_search_tool": false},
            {"slug":"custom-b"}
        ]});
        advertise_search_all(&mut catalog).unwrap();
        for model in catalog["models"].as_array().unwrap() {
            assert_eq!(model["supports_search_tool"], true);
            assert_eq!(model["web_search_tool_type"], "text_and_image");
        }
    }

    #[test]
    fn model_overrides_only_change_exact_serve_time_metadata() {
        let mut catalog = json!({"models":[
            {"slug":"cpa/gpt-6-astra", "context_window":272000, "max_context_window":872000, "priority":10},
            {"slug":"cpa/codeapi-gpt-6-astra", "context_window":272000, "max_context_window":872000}
        ]});
        let overrides = BTreeMap::from([(
            "cpa/gpt-6-astra".to_owned(),
            CatalogModelOverride {
                context_window: Some(1_000_000),
                max_context_window: Some(1_000_000),
            },
        )]);

        apply_model_overrides(&mut catalog, &overrides).unwrap();
        assert_eq!(catalog["models"][0]["context_window"], 1_000_000);
        assert_eq!(catalog["models"][0]["max_context_window"], 1_000_000);
        assert_eq!(catalog["models"][0]["priority"], 10);
        assert_eq!(catalog["models"][1]["context_window"], 272_000);
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
    fn memory_refresh_from_cpa_updates_routes_without_persisting() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .replace(
                &json!({"models":[{"slug":"gpt-5.6"}]}),
                &json!({"models":[{"slug":"claude"}]}),
            )
            .unwrap();
        let persisted = std::fs::read(&path).unwrap();

        let refreshed = store
            .replace_memory_from_cpa(&json!({"models":[{"slug":"claude"},{"slug":"gemini"}]}))
            .unwrap();
        assert_eq!(refreshed["models"].as_array().unwrap().len(), 3);
        assert_eq!(
            store.resolve("cpa/gemini").unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "gemini".into()
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), persisted);

        drop(store);
        let restored = CatalogStore::load(path).unwrap();
        assert!(restored.resolve("cpa/gemini").is_err());
        assert!(restored.resolve("cpa/claude").is_ok());
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
            )
            .unwrap();
        assert!(store.resolve("cpa/glm-5.3-uni").is_ok());

        let after_omission = store
            .replace(&json!({"models":[]}), &json!({"models":[]}))
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

    #[test]
    fn official_only_view_keeps_the_complete_routes_during_a_cpa_outage() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .replace(
                &json!({"models":[{"slug":"gpt-5.6"}]}),
                &json!({"models":[{"slug":"claude"}]}),
            )
            .unwrap();
        let persisted = std::fs::read(&path).unwrap();

        let served = store
            .official_only(&json!({"models":[{"slug":"gpt-5.6"},{"slug":"gpt-5.7"}]}))
            .unwrap();
        let slugs: Vec<_> = served["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["slug"].as_str().unwrap())
            .collect();
        assert_eq!(slugs, ["gpt-5.6", "gpt-5.7"]);
        assert_eq!(
            store.resolve("cpa/claude").unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "claude".into()
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), persisted);
    }

    #[test]
    fn official_only_view_is_routable_before_any_complete_catalog() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        store
            .official_only(&json!({"models":[
                {"slug":"gpt-5.6"},
                {"slug":"codex-auto-review", "visibility":"list"}
            ]}))
            .unwrap();
        assert_eq!(store.resolve("gpt-5.6").unwrap(), CatalogRoute::Official);
        assert_eq!(
            store.resolve(AUTO_REVIEW_MODEL).unwrap(),
            CatalogRoute::AutoReview
        );
        assert_eq!(store.current().unwrap()["models"][1]["visibility"], "hide");
        assert!(
            !path.exists(),
            "the official-only view must not be persisted"
        );

        // A later official-only refresh replaces an earlier official-only view.
        store
            .official_only(&json!({"models":[{"slug":"gpt-5.7"}]}))
            .unwrap();
        assert!(store.resolve("gpt-5.6").is_err());
        assert_eq!(store.resolve("gpt-5.7").unwrap(), CatalogRoute::Official);

        // The first complete catalog replaces the official-only view.
        store
            .replace(
                &json!({"models":[{"slug":"gpt-5.7"}]}),
                &json!({"models":[{"slug":"claude"}]}),
            )
            .unwrap();
        assert!(store.resolve("cpa/claude").is_ok());
    }

    #[test]
    fn official_only_view_reserves_the_cpa_namespace() {
        let root = tempdir().unwrap();
        let store = CatalogStore::load(root.path().join("catalog.json")).unwrap();
        let error = store
            .official_only(&json!({"models":[{"slug":"cpa/claude"}]}))
            .unwrap_err();
        assert!(error.to_string().contains("reserves CPA namespace"));
        assert!(store.current().is_none());
    }

    #[test]
    fn ultra_is_served_only_for_models_with_reasoning_levels() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        let catalog = store
            .replace(
                &json!({"models":[{"slug":"gpt-5.6","supported_reasoning_levels":[{"effort":"high","description":"High"}]}]}),
                &json!({"models":[{"slug":"plain"},{"slug":"empty","supported_reasoning_levels":[]}]}),
            )
            .unwrap();
        let persisted = std::fs::read_to_string(&path).unwrap();
        assert!(
            !persisted.contains("ultra"),
            "ultra leaked into the snapshot"
        );

        let mut served = catalog;
        advertise_ultra(&mut served).unwrap();
        advertise_ultra(&mut served).unwrap();
        let levels = |slug: &str| {
            served["models"]
                .as_array()
                .unwrap()
                .iter()
                .find(|model| model["slug"] == slug)
                .unwrap()
                .get("supported_reasoning_levels")
                .cloned()
        };
        let official = levels("gpt-5.6").unwrap();
        assert_eq!(
            official
                .as_array()
                .unwrap()
                .iter()
                .filter(|entry| entry["effort"] == "ultra")
                .count(),
            1
        );
        assert_eq!(levels("cpa/plain"), None);
        assert_eq!(levels("cpa/empty"), Some(json!([])));
    }

    #[test]
    fn unreadable_snapshot_is_ignored_instead_of_blocking_startup() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        std::fs::write(&path, b"{not json").unwrap();
        let store = CatalogStore::load(path.clone()).unwrap();
        assert!(store.current().is_none());
        assert!(!store.has_validated_official());

        store
            .replace(
                &json!({"models":[{"slug":"gpt-5.6"}]}),
                &json!({"models":[{"slug":"claude"}]}),
            )
            .unwrap();
        assert!(
            CatalogStore::load(path)
                .unwrap()
                .resolve("cpa/claude")
                .is_ok()
        );
    }

    #[test]
    fn unchanged_catalog_is_not_rewritten() {
        let root = tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let store = CatalogStore::load(path.clone()).unwrap();
        let official = json!({"models":[{"slug":"gpt-5.6"}]});
        let cpa = json!({"models":[{"slug":"claude"}]});
        store.replace(&official, &cpa).unwrap();
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        store.replace(&official, &cpa).unwrap();
        let after = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(before, after);
    }
}
