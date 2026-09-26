//! The merged model catalog: the `/v1/models` endpoint, CPA catalog caching,
//! the startup wait for a freshly launched CPA, and background sync.

use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::Json,
};
use serde::Deserialize;
use serde_json::Value;
use tokio::time::Instant;

use super::{
    AppState, ProxyError,
    upstream::{read_body_limited, upstream_error_message},
};
use crate::{catalog, router};

/// Whole-request budget for one upstream model catalog fetch. Streaming
/// Responses traffic has no total timeout; a catalog is small and a hung
/// upstream must not stall a model refresh or the background sync forever.
const CATALOG_FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const CPA_CATALOG_FRESH_FOR: Duration = Duration::from_secs(20);
const CPA_SYNC_INITIAL: Duration = Duration::from_secs(1);
const CPA_SYNC_INTERVAL: Duration = Duration::from_secs(30);
const CPA_SYNC_RETRY: Duration = Duration::from_secs(5);
const CPA_SYNC_MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);
/// How long a cold start waits for a CPA instance it just launched.
pub const CPA_STARTUP_READY_TIMEOUT: Duration = Duration::from_secs(8);
const CPA_STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone)]
pub(super) struct CachedCpaCatalog {
    value: Value,
    fetched_at: Instant,
    generation: u64,
}

impl AppState {
    fn cached_cpa_catalog(&self, max_age: Duration) -> Option<Value> {
        self.cpa_catalog
            .read()
            .expect("CPA catalog cache lock poisoned")
            .as_ref()
            .filter(|cached| cached.fetched_at.elapsed() <= max_age)
            .map(|cached| cached.value.clone())
    }

    fn current_cpa_catalog(&self) -> Option<Value> {
        self.cpa_catalog
            .read()
            .expect("CPA catalog cache lock poisoned")
            .as_ref()
            .map(|cached| cached.value.clone())
    }

    fn store_cpa_catalog(&self, value: Value) {
        let mut cache = self
            .cpa_catalog
            .write()
            .expect("CPA catalog cache lock poisoned");
        let changed = cache.as_ref().is_none_or(|cached| cached.value != value);
        let generation = cache.as_ref().map_or(1, |cached| {
            cached.generation.saturating_add(u64::from(changed))
        });
        *cache = Some(CachedCpaCatalog {
            value,
            fetched_at: Instant::now(),
            generation,
        });
        if changed {
            tracing::info!(generation, "CPA model catalog synchronized");
        }
    }

    fn apply_cpa_catalog_to_memory(&self, cpa: &Value) -> anyhow::Result<()> {
        self.catalog.replace_memory_from_cpa(cpa)?;
        Ok(())
    }
}

#[derive(Deserialize)]
pub(super) struct ModelsQuery {
    #[serde(default)]
    client_version: String,
}

pub(super) async fn handle_models(
    State(state): State<AppState>,
    Query(query): Query<ModelsQuery>,
    headers: HeaderMap,
) -> Result<Json<Value>, ProxyError> {
    let catalog = match refresh_catalog(&state, &headers, &query.client_version).await {
        Ok(catalog) => catalog,
        Err(error) => {
            let Some(catalog) = state.catalog.current() else {
                return Err(error);
            };
            tracing::warn!(
                error = %error.message,
                "model catalog refresh failed; serving the catalog view already in memory"
            );
            catalog
        }
    };
    Ok(Json(served_catalog(&state, catalog)?))
}

/// Build the catalog view handed to the Codex client. Serve-time adjustments
/// stay out of the persisted snapshot, so they apply to a fresh merge, a saved
/// snapshot, and the CPA-unavailable degraded view alike, and disabling one
/// restores the upstream metadata on the next model refresh.
fn served_catalog(state: &AppState, mut catalog: Value) -> Result<Value, ProxyError> {
    // Overrides only adjust display metadata, so an unreadable profile
    // store degrades to the upstream values instead of hiding every model.
    match crate::cpa::catalog_model_overrides(&state.cpa_profiles_path) {
        Ok(overrides) => {
            catalog::apply_model_overrides(&mut catalog, &overrides)
                .map_err(ProxyError::catalog)?;
        }
        Err(error) => tracing::warn!(
            error = %format!("{error:#}"),
            "catalog model overrides are unreadable; serving upstream metadata"
        ),
    }
    if state.settings.catalog.unify_comp_hash {
        catalog::unify_comp_hash_all(&mut catalog).map_err(ProxyError::catalog)?;
    }
    if state.settings.catalog.advertise_ultra {
        catalog::advertise_ultra(&mut catalog).map_err(ProxyError::catalog)?;
    }
    // Custom models advertise search support even when the shared backend is
    // disabled or unavailable; a backend failure surfaces in the answer
    // instead of hiding the feature from the Codex client.
    catalog::advertise_search_all(&mut catalog).map_err(ProxyError::catalog)?;
    Ok(catalog)
}

async fn refresh_catalog(
    state: &AppState,
    incoming_headers: &HeaderMap,
    client_version: &str,
) -> Result<Value, ProxyError> {
    let official_headers =
        router::official_headers(incoming_headers).map_err(ProxyError::credential)?;
    let official_url = models_url(&state.official_base_url, client_version)?;
    let (official, cpa) = tokio::join!(
        fetch_catalog(&state.client, official_url, official_headers, "official"),
        async {
            match state.cached_cpa_catalog(CPA_CATALOG_FRESH_FOR) {
                Some(cached) => Ok(cached),
                None => fetch_cpa_catalog(state, client_version).await,
            }
        }
    );
    merge_catalog_results(state, official, cpa)
}

fn merge_catalog_results(
    state: &AppState,
    official: Result<Value, ProxyError>,
    cpa: Result<Value, ProxyError>,
) -> Result<Value, ProxyError> {
    match (official, cpa) {
        (Ok(official), Ok(cpa)) => state
            .catalog
            .replace(&official, &cpa)
            .map_err(ProxyError::catalog),
        // CPA unreachable or absent: serve the official catalog alone. The
        // view is never persisted, and the routes of the last complete view
        // stay installed so `cpa/` requests keep working once CPA answers.
        (Ok(official), Err(cpa_error)) => {
            tracing::warn!(%cpa_error.message, "CPA catalog unavailable; serving official models only");
            state
                .catalog
                .official_only(&official)
                .map_err(ProxyError::catalog)
        }
        (Err(official_error), cpa_result) => {
            if let Err(cpa_error) = cpa_result {
                tracing::warn!(%cpa_error.message, "CPA catalog unavailable during failed refresh");
            }
            Err(official_error)
        }
    }
}

fn models_url(base_url: &str, client_version: &str) -> Result<reqwest::Url, ProxyError> {
    let mut url = reqwest::Url::parse(&format!("{}/models", base_url.trim_end_matches('/')))
        .map_err(|error| ProxyError::bad_gateway("catalog_url", error.to_string()))?;
    url.query_pairs_mut()
        .append_pair("client_version", client_version);
    Ok(url)
}

async fn fetch_catalog(
    client: &reqwest::Client,
    url: reqwest::Url,
    headers: HeaderMap,
    source: &'static str,
) -> Result<Value, ProxyError> {
    let response = client
        .get(url)
        .headers(headers)
        .timeout(CATALOG_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            ProxyError::bad_gateway(
                "catalog_upstream",
                format!("{source}: {}", upstream_error_message(error)),
            )
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProxyError::bad_gateway(
            "catalog_upstream",
            format!("{source} models request failed with HTTP {status}"),
        ));
    }
    let bytes = read_body_limited(
        response,
        catalog::MAX_CATALOG_BYTES,
        "catalog_upstream",
        &format!("{source} model catalog"),
    )
    .await?;
    catalog::parse(&bytes, source).map_err(ProxyError::catalog)
}

async fn fetch_cpa_catalog(state: &AppState, client_version: &str) -> Result<Value, ProxyError> {
    let headers = router::cpa_headers(&HeaderMap::new(), &state.credentials.cpa_token)
        .map_err(ProxyError::credential)?;
    let url = models_url(&state.settings.cpa.base_url, client_version)?;
    let catalog = fetch_catalog(&state.client, url, headers, "CPA").await?;
    state.store_cpa_catalog(catalog.clone());
    Ok(catalog)
}

/// Prime the CPA catalog cache before the first Codex request is answered.
///
/// `launchctl bootstrap` returns before CPA binds its port, and CPA may keep
/// re-registering provider models for a few seconds after that. A model refresh
/// arriving in that window merges without a single `cpa/` model, so keep polling
/// through the startup settle period before answering. Every fetch is bounded
/// by the time left, and a CPA that never answers still falls through to the
/// persisted snapshot.
pub async fn wait_for_cpa_catalog(state: &AppState, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            if let Some(catalog) = state.current_cpa_catalog() {
                if let Err(error) = state.apply_cpa_catalog_to_memory(&catalog) {
                    tracing::warn!(%error, "failed to apply the settled CPA catalog");
                }
            } else {
                tracing::warn!(
                    timeout_seconds = timeout.as_secs(),
                    "CPA did not become ready during startup; serving the persisted snapshot"
                );
            }
            return;
        }
        match tokio::time::timeout(remaining, fetch_cpa_catalog(state, "")).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => tracing::debug!(%error.message, "CPA not ready during startup"),
            Err(_) => tracing::debug!("CPA catalog request outlasted the startup wait"),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        tokio::time::sleep(CPA_STARTUP_POLL_INTERVAL.min(remaining)).await;
    }
}

pub(super) async fn synchronize_cpa_catalog(state: AppState) {
    let mut retry = CPA_SYNC_RETRY;
    let mut delay = CPA_SYNC_INITIAL;
    // Only the first failure of a streak is a warning; a CPA that is simply
    // not installed would otherwise log one every backoff period forever.
    let mut failing = false;
    loop {
        let previous = state.current_cpa_catalog();
        delay = match fetch_cpa_catalog(&state, "").await {
            Ok(cpa) => {
                if failing {
                    tracing::info!("CPA catalog synchronization recovered");
                    failing = false;
                }
                retry = CPA_SYNC_RETRY;
                if state.catalog.has_validated_official() {
                    if let Err(error) = state.apply_cpa_catalog_to_memory(&cpa) {
                        tracing::warn!(%error, "failed to apply CPA catalog to in-memory model list");
                    } else {
                        tracing::debug!("CPA catalog applied to in-memory model list");
                    }
                } else {
                    tracing::debug!("CPA catalog cached; waiting for an official catalog refresh");
                }
                if previous.as_ref() != Some(&cpa) {
                    CPA_SYNC_INITIAL
                } else {
                    delay.saturating_mul(2).min(CPA_SYNC_INTERVAL)
                }
            }
            Err(error) => {
                if failing {
                    tracing::debug!(%error.message, retry_seconds = retry.as_secs(), "CPA catalog synchronization still failing");
                } else {
                    tracing::warn!(%error.message, retry_seconds = retry.as_secs(), "CPA catalog synchronization failed");
                    failing = true;
                }
                let delay = retry;
                retry = retry.saturating_mul(2).min(CPA_SYNC_MAX_BACKOFF);
                delay
            }
        };
        tokio::time::sleep(delay).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axum::{
        Router,
        http::{HeaderValue, StatusCode, header},
        routing::get,
    };
    use serde_json::json;

    use crate::server::test_support::*;
    use crate::{
        catalog::{CatalogRoute, CatalogStore},
        config::{Credentials, Paths, Settings},
    };

    /// A cold start must not answer the queued Codex model refresh until the
    /// CPA instance it just launched is reachable, or the client caches a
    /// catalog with no `cpa/` models for the rest of its session.
    #[tokio::test]
    async fn startup_wait_primes_the_catalog_once_cpa_finishes_binding() {
        let root = tempfile::tempdir().unwrap();
        // Reserve the port first so the state points at an endpoint that only
        // starts answering later, exactly like a CPA still binding its socket.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let cpa_address = listener.local_addr().unwrap();
        drop(listener);
        let state = auto_review_state(root.path(), cpa_address, cpa_address);
        assert!(state.cached_cpa_catalog(CPA_CATALOG_FRESH_FOR).is_none());

        let late_start = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let app = Router::new().route(
                "/v1/models",
                get(|| async { Json(json!({"models":[{"slug":"glm-5.3-flash"}]})) }),
            );
            let listener = tokio::net::TcpListener::bind(cpa_address).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });

        wait_for_cpa_catalog(&state, Duration::from_millis(800)).await;
        let cached = state.cached_cpa_catalog(CPA_CATALOG_FRESH_FOR).unwrap();
        assert_eq!(cached["models"][0]["slug"], "glm-5.3-flash");
        late_start.abort();
    }

    #[tokio::test]
    async fn startup_wait_gives_up_when_cpa_never_answers() {
        let root = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let dead_address = listener.local_addr().unwrap();
        drop(listener);
        let state = auto_review_state(root.path(), dead_address, dead_address);

        let started = Instant::now();
        wait_for_cpa_catalog(&state, Duration::from_millis(600)).await;
        // Bounded: an absent CPA degrades instead of blocking the listener.
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(state.cached_cpa_catalog(CPA_CATALOG_FRESH_FOR).is_none());
    }

    #[tokio::test]
    async fn startup_wait_waits_for_cpa_catalog_changes_to_stabilize() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route(
                "/v1/models",
                get(|State(counter): State<Arc<AtomicUsize>>| async move {
                    let value = counter.fetch_add(1, Ordering::SeqCst);
                    if value == 0 {
                        Json(json!({"models":[{"slug":"glm-5.3-flash"}]}))
                    } else {
                        Json(json!({"models":[
                            {"slug":"glm-5.3-flash"},
                            {"slug":"gemini"}
                        ]}))
                    }
                }),
            )
            .with_state(counter);
        let (address, handle) = spawn_test_app(app).await;
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), address, address);

        wait_for_cpa_catalog(&state, Duration::from_millis(1200)).await;
        assert_eq!(
            state.catalog.resolve("cpa/gemini").unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "gemini".into()
            }
        );

        handle.abort();
    }

    /// A CPA that accepts connections but never answers must not hold the
    /// startup wait past its deadline.
    #[tokio::test]
    async fn startup_wait_is_bounded_when_cpa_never_responds() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let silent = tokio::spawn(async move {
            let mut connections = Vec::new();
            loop {
                let (connection, _) = listener.accept().await.unwrap();
                connections.push(connection);
            }
        });
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), address, address);

        let started = Instant::now();
        wait_for_cpa_catalog(&state, Duration::from_millis(500)).await;
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(state.cached_cpa_catalog(CPA_CATALOG_FRESH_FOR).is_none());
        silent.abort();
    }

    #[test]
    fn cpa_sync_updates_memory_routes_without_persisting_until_full_refresh() {
        let root = tempfile::tempdir().unwrap();
        let catalog_path = root.path().join("model-catalog.json");
        {
            let store = CatalogStore::load(catalog_path.clone()).unwrap();
            store
                .replace(
                    &json!({"models":[{"slug":"gpt-5.6"}]}),
                    &json!({"models":[{"slug":"claude"}]}),
                )
                .unwrap();
        }
        let persisted = std::fs::read(&catalog_path).unwrap();
        let state = AppState::new(
            Settings::default(),
            Credentials {
                proxy_token: "proxy".into(),
                cpa_token: "cpa-secret".into(),
                cpa_management_key: "management-secret".into(),
            },
            &Paths::from_root(root.path().to_path_buf()),
        )
        .unwrap();
        let cpa = json!({"models":[{"slug":"claude"},{"slug":"gemini"}]});

        state.store_cpa_catalog(cpa.clone());
        assert!(state.catalog.has_validated_official());
        state.apply_cpa_catalog_to_memory(&cpa).unwrap();

        assert_eq!(
            state.catalog.resolve("cpa/gemini").unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "gemini".into()
            }
        );
        assert_eq!(std::fs::read(&catalog_path).unwrap(), persisted);
    }

    #[tokio::test]
    async fn models_endpoint_merges_catalogs_with_isolated_credentials() {
        async fn official(Query(query): Query<ModelsQuery>, headers: HeaderMap) -> Json<Value> {
            assert_eq!(query.client_version, "0.146.1");
            assert_eq!(headers[header::AUTHORIZATION], "Bearer oauth");
            assert_eq!(headers["chatgpt-account-id"], "account");
            assert_ne!(headers[header::AUTHORIZATION], "Bearer cpa-secret");
            Json(json!({"models":[{
                "slug":"gpt-5.6", "display_name":"5.6", "priority":1
            }]}))
        }

        async fn cpa(Query(query): Query<ModelsQuery>, headers: HeaderMap) -> Json<Value> {
            assert_eq!(query.client_version, "0.146.1");
            assert_eq!(headers[header::AUTHORIZATION], "Bearer cpa-secret");
            assert!(!headers.contains_key("chatgpt-account-id"));
            Json(json!({"models":[{
                "slug":"gpt-5.6", "display_name":"5.6", "context_window":1000000,
                "future_capability":{"kept":true}
            }, {
                "slug":"claude", "display_name":"Claude"
            }]}))
        }

        let (official_address, official_handle) =
            spawn_test_app(Router::new().route("/models", get(official))).await;
        let (cpa_address, cpa_handle) =
            spawn_test_app(Router::new().route("/v1/models", get(cpa))).await;
        let root = tempfile::tempdir().unwrap();
        let settings = Settings {
            cpa: crate::config::Cpa {
                base_url: format!("http://{cpa_address}/v1"),
            },
            ..Settings::default()
        };
        let mut state = AppState::new(
            settings,
            Credentials {
                proxy_token: "proxy".into(),
                cpa_token: "cpa-secret".into(),
                cpa_management_key: "management-secret".into(),
            },
            &Paths::from_root(root.path().to_path_buf()),
        )
        .unwrap();
        state.official_base_url = format!("http://{official_address}");
        let incoming = HeaderMap::from_iter([
            (
                header::AUTHORIZATION,
                HeaderValue::from_static("Bearer oauth"),
            ),
            (
                http::HeaderName::from_static("chatgpt-account-id"),
                HeaderValue::from_static("account"),
            ),
        ]);
        let catalog = refresh_catalog(&state, &incoming, "0.146.1").await.unwrap();
        assert_eq!(catalog["models"].as_array().unwrap().len(), 3);
        assert_eq!(catalog["models"][0]["slug"], "gpt-5.6");
        assert_eq!(catalog["models"][1]["slug"], "cpa/gpt-5.6");
        assert_eq!(catalog["models"][1]["display_name"], "5.6 · CPA");
        assert_eq!(catalog["models"][1]["future_capability"]["kept"], true);
        assert_eq!(
            state.catalog.resolve("cpa/gpt-5.6").unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "gpt-5.6".into()
            }
        );

        official_handle.abort();
        cpa_handle.abort();
    }

    #[tokio::test]
    async fn valid_cpa_catalog_omission_drops_the_previous_model() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        async fn official() -> Json<Value> {
            Json(json!({"models":[{"slug":"official","priority":1}]}))
        }
        async fn cpa(State(calls): State<Arc<AtomicUsize>>) -> Json<Value> {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Json(json!({"models":[{
                    "slug":"glm-5.3-uni", "display_name":"GLM 5.3 Uni",
                    "context_window":202_752
                }]}))
            } else {
                // CPA cooling can return HTTP 200 with a structurally valid
                // catalog that omits the affected model; it is served as-is.
                Json(json!({"models":[]}))
            }
        }

        let (official_address, official_handle) =
            spawn_test_app(Router::new().route("/models", get(official))).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let (cpa_address, cpa_handle) = spawn_test_app(
            Router::new()
                .route("/v1/models", get(cpa))
                .with_state(calls),
        )
        .await;
        let root = tempfile::tempdir().unwrap();
        let mut state = AppState::new(
            Settings {
                cpa: crate::config::Cpa {
                    base_url: format!("http://{cpa_address}/v1"),
                },
                ..Settings::default()
            },
            Credentials {
                proxy_token: "proxy".into(),
                cpa_token: "cpa-secret".into(),
                cpa_management_key: "management-secret".into(),
            },
            &Paths::from_root(root.path().to_path_buf()),
        )
        .unwrap();
        state.official_base_url = format!("http://{official_address}");
        let incoming = HeaderMap::from_iter([(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer oauth"),
        )]);

        refresh_catalog(&state, &incoming, "test").await.unwrap();
        // Skip the 20 s CPA cache so the second refresh sees the new catalog.
        *state.cpa_catalog.write().unwrap() = None;
        let after_omission = refresh_catalog(&state, &incoming, "test").await.unwrap();
        let present = after_omission["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model["slug"] == "cpa/glm-5.3-uni");
        assert!(!present);
        assert!(state.catalog.resolve("cpa/glm-5.3-uni").is_err());

        official_handle.abort();
        cpa_handle.abort();
    }

    #[tokio::test]
    async fn models_endpoint_serves_saved_snapshot_when_refresh_fails() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("model-catalog.json");
        let seed = CatalogStore::load(path.clone()).unwrap();
        seed.replace(
            &json!({"models":[{"slug":"official-saved"}]}),
            &json!({"models":[{"slug":"cpa-saved"}]}),
        )
        .unwrap();
        drop(seed);
        let mut state = AppState::new(
            Settings {
                cpa: crate::config::Cpa {
                    base_url: "http://127.0.0.1:9/v1".into(),
                },
                ..Settings::default()
            },
            Credentials {
                proxy_token: "proxy".into(),
                cpa_token: "cpa".into(),
                cpa_management_key: "management".into(),
            },
            &Paths::from_root(root.path().to_path_buf()),
        )
        .unwrap();
        state.official_base_url = "http://127.0.0.1:9".into();
        let (proxy_address, proxy_handle) = spawn_proxy(state).await;

        let response = reqwest::Client::new()
            .get(format!(
                "http://{proxy_address}/v1/models?client_version=test"
            ))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response: Value = response.json().await.unwrap();
        assert_eq!(response["models"][0]["slug"], "official-saved");
        assert_eq!(response["models"][1]["slug"], "cpa/cpa-saved");
        proxy_handle.abort();
    }

    #[tokio::test]
    async fn models_endpoint_serves_official_only_when_cpa_is_down() {
        async fn official(Query(query): Query<ModelsQuery>, _headers: HeaderMap) -> Json<Value> {
            assert_eq!(query.client_version, "degraded");
            Json(json!({"models":[{"slug":"gpt-5.6", "priority":1}]}))
        }
        let (official_address, official_handle) =
            spawn_test_app(Router::new().route("/models", get(official))).await;

        let root = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(root.path().to_owned());
        // No stored snapshot: the fresh state has never seen a catalog.
        let mut state = AppState::new(
            Settings {
                cpa: crate::config::Cpa {
                    // Port 9 (discard) is unreachable: CPA is down.
                    base_url: "http://127.0.0.1:9/v1".into(),
                },
                ..Settings::default()
            },
            Credentials {
                proxy_token: "proxy".into(),
                cpa_token: "cpa-secret".into(),
                cpa_management_key: "management-secret".into(),
            },
            &paths,
        )
        .unwrap();
        state.official_base_url = format!("http://{official_address}");

        let incoming = HeaderMap::from_iter([(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer oauth"),
        )]);
        let catalog = refresh_catalog(&state, &incoming, "degraded")
            .await
            .unwrap();
        let slugs: Vec<&str> = catalog["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["slug"].as_str().unwrap())
            .collect();
        assert_eq!(slugs, vec!["gpt-5.6"]);
        // The official-only view is memory-only, but its routes are installed
        // because no complete view exists yet.
        assert_eq!(
            state.catalog.resolve("gpt-5.6").unwrap(),
            CatalogRoute::Official
        );
        assert!(!paths.catalog.exists(), "the degraded view was persisted");

        official_handle.abort();
    }

    fn comp_hash_state(root: &std::path::Path, unify: bool) -> AppState {
        let catalog_path = root.join("model-catalog.json");
        let store = CatalogStore::load(catalog_path.clone()).unwrap();
        store
            .replace(
                &json!({"models":[
                    {"slug":"gpt-5.6-sol", "priority":6, "comp_hash":"3000"},
                    {"slug":"gpt-5.5", "priority":12, "comp_hash":"2911"}
                ]}),
                &json!({"models":[{"slug":"claude-opus-5", "priority":1, "comp_hash":"2911"}]}),
            )
            .unwrap();
        drop(store);
        let mut settings = Settings::default();
        settings.catalog.unify_comp_hash = unify;
        AppState::new(
            settings,
            Credentials {
                proxy_token: "proxy".into(),
                cpa_token: "cpa".into(),
                cpa_management_key: "management".into(),
            },
            &Paths::from_root(root.to_path_buf()),
        )
        .unwrap()
    }

    /// The served view unifies `comp_hash` so switching model mid-conversation
    /// never asks the model being left behind to compact first, while the
    /// persisted snapshot keeps upstream metadata.
    #[test]
    fn served_catalog_unifies_comp_hash_and_leaves_the_snapshot_upstream() {
        let root = tempfile::tempdir().unwrap();
        let state = comp_hash_state(root.path(), true);
        let snapshot = state.catalog.current().unwrap();
        let served = served_catalog(&state, snapshot.clone()).unwrap();

        let served_hashes: Vec<&str> = served["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["comp_hash"].as_str().unwrap())
            .collect();
        assert_eq!(served_hashes, ["3000", "3000", "3000"]);
        assert_eq!(served["models"][2]["slug"], "cpa/claude-opus-5");

        let stored_hashes: Vec<&str> = snapshot["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["comp_hash"].as_str().unwrap())
            .collect();
        assert_eq!(stored_hashes, ["3000", "2911", "2911"]);
    }

    #[test]
    fn served_catalog_always_advertises_search_support() {
        let root = tempfile::tempdir().unwrap();
        let state = comp_hash_state(root.path(), false);
        let served = served_catalog(
            &state,
            json!({"models":[{"slug":"custom-model", "supports_search_tool": false}]}),
        )
        .unwrap();
        let model = &served["models"][0];
        assert_eq!(model["supports_search_tool"], true);
        assert_eq!(model["web_search_tool_type"], "text_and_image");
    }

    #[test]
    fn served_catalog_keeps_upstream_comp_hash_when_unification_is_disabled() {
        let root = tempfile::tempdir().unwrap();
        let state = comp_hash_state(root.path(), false);
        let served = served_catalog(&state, state.catalog.current().unwrap()).unwrap();
        let hashes: Vec<&str> = served["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["comp_hash"].as_str().unwrap())
            .collect();
        assert_eq!(hashes, ["3000", "2911", "2911"]);
    }
}
