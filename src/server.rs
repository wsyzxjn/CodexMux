use std::{
    fmt,
    future::Future,
    io,
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

use async_stream::stream;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, OriginalUri, Query, Request, State},
    http::{HeaderMap, HeaderValue, Response, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Json},
    routing::{get, post},
};
use futures_util::StreamExt;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::{Map, Value, json};
use tokio::{sync::watch, time::Instant};

use crate::{
    catalog::{self, CatalogRoute, CatalogStore},
    config::{Credentials, OFFICIAL_BASE_URL, Settings},
    continuity::{ContinuityStore, portable_input_items, portable_output_items},
    dialect::sse,
    router,
};

const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
const MAX_CAPTURE_BYTES: usize = 32 * 1024 * 1024;
const PROXY_TOKEN_HEADER: &str = "x-codexmux-token";
const PROXY_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const CPA_CATALOG_FRESH_FOR: Duration = Duration::from_secs(20);
const CPA_SYNC_INTERVAL: Duration = Duration::from_secs(30);
const CPA_SYNC_MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Debug, Eq, PartialEq)]
enum Route {
    Official,
    Cpa,
    /// CodexMux proxies straight to this upstream, bypassing CPA.
    Direct {
        base_url: String,
        token: String,
    },
}

impl Route {
    fn identity(&self, model: &str) -> String {
        match self {
            Self::Official => format!("official:{model}"),
            Self::Cpa => format!("cpa:{model}"),
            Self::Direct { base_url, .. } => format!("direct:{base_url}:{model}"),
        }
    }

    fn always_replays_history(&self) -> bool {
        matches!(self, Self::Cpa | Self::Direct { .. })
    }
}

#[derive(Clone)]
pub struct AppState {
    client: reqwest::Client,
    official_base_url: String,
    settings: Arc<Settings>,
    credentials: Arc<Credentials>,
    catalog: Arc<CatalogStore>,
    continuity: Arc<ContinuityStore>,
    /// Profile store path; read per-request for the review model override so
    /// menu-bar changes apply without a proxy restart.
    cpa_profiles_path: PathBuf,
    activity: watch::Sender<Instant>,
    cpa_catalog: Arc<RwLock<Option<CachedCpaCatalog>>>,
}

#[derive(Clone)]
struct CachedCpaCatalog {
    value: Value,
    fetched_at: Instant,
    generation: u64,
}

impl AppState {
    pub fn new(
        settings: Settings,
        credentials: Credentials,
        catalog_path: PathBuf,
        cpa_profiles_path: PathBuf,
    ) -> anyhow::Result<Self> {
        settings.validate()?;
        credentials.validate()?;
        let catalog = CatalogStore::load(catalog_path)?;
        let (activity, _) = watch::channel(Instant::now());
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .build()?,
            official_base_url: OFFICIAL_BASE_URL.into(),
            settings: Arc::new(settings),
            credentials: Arc::new(credentials),
            catalog: Arc::new(catalog),
            continuity: Arc::new(ContinuityStore::new()),
            cpa_profiles_path,
            activity,
            cpa_catalog: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn wait_for_idle(&self) {
        let mut activity = self.activity.subscribe();
        loop {
            let deadline = *activity.borrow() + PROXY_IDLE_TIMEOUT;
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    if Instant::now().duration_since(*activity.borrow()) >= PROXY_IDLE_TIMEOUT {
                        tracing::info!(idle_seconds = PROXY_IDLE_TIMEOUT.as_secs(), "proxy idle timeout reached");
                        return;
                    }
                }
                changed = activity.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
            }
        }
    }

    fn touch(&self) {
        self.activity.send_replace(Instant::now());
    }

    fn cached_cpa_catalog(&self, max_age: Duration) -> Option<Value> {
        self.cpa_catalog
            .read()
            .expect("CPA catalog cache lock poisoned")
            .as_ref()
            .filter(|cached| cached.fetched_at.elapsed() <= max_age)
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
}

pub async fn serve<F>(
    listener: tokio::net::TcpListener,
    state: AppState,
    shutdown: F,
) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let address = listener.local_addr()?;
    tracing::info!(%address, "CodexMux listening");
    let sync_state = state.clone();
    let sync_task = tokio::spawn(async move { synchronize_cpa_catalog(sync_state).await });
    let result = axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown)
        .await;
    sync_task.abort();
    result?;
    Ok(())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(handle_models))
        .route("/v1/responses", post(handle_responses))
        .route("/v1/responses/compact", post(handle_responses))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_proxy_token,
        ))
        .with_state(state)
}

async fn require_proxy_token(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<axum::response::Response, ProxyError> {
    authenticate(request.headers(), &state.credentials.proxy_token)?;
    state.touch();
    Ok(next.run(request).await)
}

async fn health() -> Json<Value> {
    Json(json!({"ok": true, "service": "codexmux"}))
}

#[derive(Deserialize)]
struct ModelsQuery {
    #[serde(default)]
    client_version: String,
}

async fn handle_models(
    State(state): State<AppState>,
    Query(query): Query<ModelsQuery>,
    headers: HeaderMap,
) -> Result<Json<Value>, ProxyError> {
    match refresh_catalog(&state, &headers, &query.client_version).await {
        Ok(catalog) => Ok(Json(catalog)),
        Err(error) => {
            if let Some(catalog) = state.catalog.current() {
                tracing::warn!(%error.message, "model catalog refresh failed; serving saved snapshot");
                Ok(Json(catalog))
            } else {
                Err(error)
            }
        }
    }
}

async fn refresh_catalog(
    state: &AppState,
    incoming_headers: &HeaderMap,
    client_version: &str,
) -> Result<Value, ProxyError> {
    let official_headers = router::official_headers(incoming_headers)
        .map_err(|error| ProxyError::unauthorized("credential", error.to_string()))?;
    let official_url = models_url(OFFICIAL_BASE_URL, client_version)?;
    let (official, cpa) = tokio::join!(
        fetch_catalog(&state.client, official_url, official_headers, "official"),
        async {
            if let Some(cached) = state.cached_cpa_catalog(CPA_CATALOG_FRESH_FOR) {
                Ok(cached)
            } else {
                fetch_cpa_catalog(state, client_version).await
            }
        }
    );
    merge_catalog_results(state, official, cpa)
}

async fn fetch_cpa_catalog(state: &AppState, client_version: &str) -> Result<Value, ProxyError> {
    let headers = router::cpa_headers(&HeaderMap::new(), &state.credentials.cpa_token)
        .map_err(|error| ProxyError::unauthorized("credential", error.to_string()))?;
    let url = models_url(&state.settings.cpa.base_url, client_version)?;
    let catalog = fetch_catalog(&state.client, url, headers, "CPA").await?;
    state.store_cpa_catalog(catalog.clone());
    Ok(catalog)
}

async fn synchronize_cpa_catalog(state: AppState) {
    let mut retry = Duration::from_secs(5);
    loop {
        let delay = match fetch_cpa_catalog(&state, "").await {
            Ok(_) => {
                retry = Duration::from_secs(5);
                CPA_SYNC_INTERVAL
            }
            Err(error) => {
                tracing::warn!(%error.message, retry_seconds = retry.as_secs(), "CPA catalog synchronization failed");
                let delay = retry;
                retry = retry.saturating_mul(2).min(CPA_SYNC_MAX_BACKOFF);
                delay
            }
        };
        tokio::time::sleep(delay).await;
    }
}

#[cfg(test)]
async fn refresh_catalog_from_urls(
    state: &AppState,
    official_headers: HeaderMap,
    cpa_headers: HeaderMap,
    official_url: reqwest::Url,
    cpa_url: reqwest::Url,
) -> Result<Value, ProxyError> {
    let (official, cpa) = tokio::join!(
        fetch_catalog(
            &state.client,
            official_url.clone(),
            official_headers,
            "official"
        ),
        fetch_catalog(&state.client, cpa_url, cpa_headers, "CPA")
    );
    merge_catalog_results(state, official, cpa)
}

fn merge_catalog_results(
    state: &AppState,
    official: Result<Value, ProxyError>,
    cpa: Result<Value, ProxyError>,
) -> Result<Value, ProxyError> {
    match (official, cpa) {
        (Ok(official), Ok(cpa)) => {
            let direct = crate::cpa::declared_direct_models(&state.cpa_profiles_path);
            state
                .catalog
                .replace(&official, &cpa, &direct)
                .map_err(|error| ProxyError::bad_gateway("catalog", error.to_string()))
        }
        // CPA unreachable or absent: serve the official catalog plus declared
        // direct models. The merged view is never persisted; a stored
        // snapshot still requires both upstream catalogs to validate.
        (Ok(official), Err(cpa_error)) => {
            tracing::warn!(%cpa_error.message, "CPA catalog unavailable; merging direct routes only");
            let direct = crate::cpa::declared_direct_models(&state.cpa_profiles_path);
            catalog::merge_official_direct(&official, &direct)
                .map_err(|error| ProxyError::bad_gateway("catalog", error.to_string()))
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
        .send()
        .await
        .map_err(|error| {
            ProxyError::bad_gateway("catalog_upstream", format!("{source}: {error}"))
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProxyError::bad_gateway(
            "catalog_upstream",
            format!("{source} models request failed with HTTP {status}"),
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > catalog::MAX_CATALOG_BYTES as u64)
    {
        return Err(ProxyError::bad_gateway(
            "catalog",
            format!("{source} model catalog exceeds 16 MiB"),
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            ProxyError::bad_gateway("catalog_upstream", format!("{source}: {error}"))
        })?;
        if bytes.len().saturating_add(chunk.len()) > catalog::MAX_CATALOG_BYTES {
            return Err(ProxyError::bad_gateway(
                "catalog",
                format!("{source} model catalog exceeds 16 MiB"),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    catalog::parse(&bytes, source)
        .map_err(|error| ProxyError::bad_gateway("catalog", error.to_string()))
}

struct UniqueObject(Map<String, Value>);

impl<'de> Deserialize<'de> for UniqueObject {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ObjectVisitor;

        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = UniqueObject;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object with unique top-level keys")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut object = Map::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    if object.insert(key.clone(), value).is_some() {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate top-level field {key}"
                        )));
                    }
                }
                Ok(UniqueObject(object))
            }
        }

        deserializer.deserialize_map(ObjectVisitor)
    }
}

struct ResponseRequest {
    body: Bytes,
    value: Value,
    model: String,
    parent: Option<String>,
    turn_input: Vec<Value>,
}

async fn handle_responses(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response<Body>, ProxyError> {
    let endpoint = if uri.path().ends_with("/responses/compact") {
        "responses/compact"
    } else {
        "responses"
    };
    let UniqueObject(object) = serde_json::from_slice(&body)
        .map_err(|error| ProxyError::bad_request("invalid_json", error.to_string()))?;
    let request = Value::Object(object);
    let model = request
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ProxyError::bad_request("missing_model", "request has no string model"))?;
    let parent = match request.get("previous_response_id") {
        Some(Value::String(parent)) if !parent.is_empty() => Some(parent.clone()),
        None | Some(Value::Null) => None,
        Some(_) => {
            return Err(ProxyError::bad_request(
                "invalid_previous_response_id",
                "previous_response_id must be a nonempty string or null",
            ));
        }
    };
    let turn_input = portable_input_items(request.get("input"));
    let request = ResponseRequest {
        body,
        value: request,
        model,
        parent,
        turn_input,
    };

    let catalog_route = state
        .catalog
        .resolve_with_direct(
            &request.model,
            &crate::cpa::declared_direct_models(&state.cpa_profiles_path),
        )
        .map_err(|error| ProxyError::bad_request("route", error.to_string()))?;

    match catalog_route {
        CatalogRoute::Official => {
            forward_response(&state, &headers, endpoint, &request, &Route::Official, None).await
        }
        CatalogRoute::Cpa { upstream_model } => {
            let (route, routed_model) = cpa_route(&state, &upstream_model)?;
            forward_response(
                &state,
                &headers,
                endpoint,
                &request,
                &route,
                Some(&routed_model),
            )
            .await
        }
        CatalogRoute::AutoReview => {
            // An explicit override sends codex-auto-review to the selected CPA
            // model. Without one, it remains on the official route.
            let override_model = crate::cpa::review_override(&state.cpa_profiles_path);
            if let Some(slug) = override_model {
                let local_model = format!("{}{slug}", crate::config::CPA_MODEL_PREFIX);
                let upstream_model = match state.catalog.resolve_with_direct(
                    &local_model,
                    &crate::cpa::declared_direct_models(&state.cpa_profiles_path),
                ) {
                    Ok(CatalogRoute::Cpa { upstream_model }) => upstream_model,
                    _ => {
                        return Err(ProxyError::bad_request(
                            "review_override",
                            format!("review override model {local_model} is not in the catalog"),
                        ));
                    }
                };
                let (route, routed_model) = cpa_route(&state, &upstream_model)?;
                forward_response(
                    &state,
                    &headers,
                    endpoint,
                    &request,
                    &route,
                    Some(&routed_model),
                )
                .await
            } else {
                forward_response(&state, &headers, endpoint, &request, &Route::Official, None).await
            }
        }
    }
}

fn cpa_route(state: &AppState, upstream_model: &str) -> Result<(Route, String), ProxyError> {
    let direct = crate::cpa::direct_route_for(&state.cpa_profiles_path, upstream_model)
        .map_err(|error| ProxyError::bad_gateway("direct_route", error.to_string()))?;
    Ok(match direct {
        Some(direct) => {
            if direct.token == state.credentials.proxy_token
                || direct.token == state.credentials.cpa_token
                || direct.token == state.credentials.cpa_management_key
            {
                return Err(ProxyError::bad_gateway(
                    "direct_route",
                    "direct route token must be distinct from CodexMux credentials",
                ));
            }
            (
                Route::Direct {
                    base_url: direct.base_url,
                    token: direct.token,
                },
                direct.upstream_model,
            )
        }
        None => (Route::Cpa, upstream_model.to_owned()),
    })
}

async fn forward_response(
    state: &AppState,
    headers: &HeaderMap,
    endpoint: &str,
    request: &ResponseRequest,
    route: &Route,
    cpa_upstream_model: Option<&str>,
) -> Result<Response<Body>, ProxyError> {
    let (outgoing, route_id) = prepare_request(
        &request.body,
        &request.value,
        &request.model,
        request.parent.as_deref(),
        &request.turn_input,
        route,
        cpa_upstream_model,
        &state.continuity,
    )?;
    let upstream = send_upstream(state, headers, endpoint, route, outgoing).await?;
    finish_response(
        upstream,
        request.parent.clone(),
        request.turn_input.clone(),
        route_id,
        state.continuity.clone(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
fn prepare_request(
    body: &Bytes,
    request: &Value,
    model: &str,
    parent: Option<&str>,
    turn_input: &[Value],
    route: &Route,
    cpa_upstream_model: Option<&str>,
    continuity: &ContinuityStore,
) -> Result<(Bytes, String), ProxyError> {
    let mut request = request.clone();
    let route_id = route.identity(model);
    let continuity_rewritten = rewrite_for_continuity(
        &mut request,
        parent,
        turn_input,
        route,
        &route_id,
        continuity,
    )?;
    let model_rewritten = if let Some(upstream_model) = cpa_upstream_model {
        request
            .as_object_mut()
            .expect("top-level request was parsed as an object")
            .insert("model".into(), Value::String(upstream_model.to_owned()));
        true
    } else {
        false
    };
    let outgoing = if continuity_rewritten || model_rewritten {
        Bytes::from(
            serde_json::to_vec(&request)
                .map_err(|error| ProxyError::bad_request("json", error.to_string()))?,
        )
    } else {
        body.clone()
    };
    Ok((outgoing, route_id))
}

async fn send_upstream(
    state: &AppState,
    headers: &HeaderMap,
    endpoint: &str,
    route: &Route,
    body: Bytes,
) -> Result<reqwest::Response, ProxyError> {
    let upstream_headers = match route {
        Route::Official => router::official_headers(headers)
            .map_err(|error| ProxyError::unauthorized("credential", error.to_string()))?,
        Route::Cpa => router::cpa_headers(headers, &state.credentials.cpa_token)
            .map_err(|error| ProxyError::unauthorized("credential", error.to_string()))?,
        Route::Direct { token, .. } => router::cpa_headers(headers, token)
            .map_err(|error| ProxyError::unauthorized("credential", error.to_string()))?,
    };
    let target = target_url(&state.settings, &state.official_base_url, route, endpoint);
    state
        .client
        .post(target)
        .headers(upstream_headers)
        .body(body)
        .send()
        .await
        .map_err(|error| ProxyError::bad_gateway("upstream", error.to_string()))
}

async fn finish_response(
    upstream: reqwest::Response,
    parent: Option<String>,
    turn_input: Vec<Value>,
    route_id: String,
    continuity: Arc<ContinuityStore>,
) -> Result<Response<Body>, ProxyError> {
    let status = upstream.status();
    let is_sse = upstream
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"))
        });
    if !status.is_success() || upstream.headers().contains_key(header::CONTENT_ENCODING) {
        return Ok(passthrough_response(upstream));
    }

    if is_sse {
        Ok(streaming_response(
            upstream, parent, turn_input, route_id, continuity,
        ))
    } else {
        non_streaming_response(upstream, parent, turn_input, route_id, &continuity).await
    }
}

fn rewrite_for_continuity(
    request: &mut Value,
    parent: Option<&str>,
    turn_input: &[Value],
    route: &Route,
    route_id: &str,
    continuity: &ContinuityStore,
) -> Result<bool, ProxyError> {
    let Some(parent_id) = parent else {
        return Ok(false);
    };
    let previous_route = continuity.route_of(parent_id).ok_or_else(|| {
        ProxyError::conflict(
            "unknown_history",
            "cannot continue because the previous response id is not in local history",
        )
    })?;
    if !continuity.is_complete(parent_id) {
        return Err(ProxyError::conflict(
            "incomplete_history",
            "cannot continue because the previous response chain is incomplete",
        ));
    }
    if !route.always_replays_history() && previous_route == route_id {
        return Ok(false);
    }
    let mut replay = continuity.materialize(parent_id).ok_or_else(|| {
        ProxyError::conflict(
            "incomplete_history",
            "cannot continue because the previous response chain is incomplete",
        )
    })?;
    replay.extend_from_slice(turn_input);
    let object = request
        .as_object_mut()
        .ok_or_else(|| ProxyError::bad_request("invalid_json", "request must be an object"))?;
    object.insert("input".into(), Value::Array(replay));
    object.remove("previous_response_id");
    Ok(true)
}

fn authenticate(headers: &HeaderMap, expected: &str) -> Result<(), ProxyError> {
    let supplied = headers
        .get(PROXY_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    if supplied != Some(expected) {
        return Err(ProxyError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "missing or invalid CodexMux token",
        ));
    }
    Ok(())
}

fn target_url(
    settings: &Settings,
    official_base_url: &str,
    route: &Route,
    endpoint: &str,
) -> String {
    let base_url = match route {
        Route::Official => official_base_url,
        Route::Cpa => &settings.cpa.base_url,
        Route::Direct { base_url, .. } => base_url,
    };
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    )
}

fn passthrough_response(upstream: reqwest::Response) -> Response<Body> {
    let status = upstream.status();
    let content_type = upstream.headers().get(header::CONTENT_TYPE).cloned();
    let content_encoding = upstream.headers().get(header::CONTENT_ENCODING).cloned();
    let mut builder = Response::builder().status(status);
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    if let Some(content_encoding) = content_encoding {
        builder = builder.header(header::CONTENT_ENCODING, content_encoding);
    }
    builder
        .body(Body::from_stream(upstream.bytes_stream()))
        .expect("upstream status and content type are valid response metadata")
}

async fn non_streaming_response(
    upstream: reqwest::Response,
    parent: Option<String>,
    turn_input: Vec<Value>,
    route_id: String,
    continuity: &ContinuityStore,
) -> Result<Response<Body>, ProxyError> {
    let status = upstream.status();
    let content_type = upstream
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/json"));
    let content_encoding = upstream.headers().get(header::CONTENT_ENCODING).cloned();
    let bytes = upstream
        .bytes()
        .await
        .map_err(|error| ProxyError::bad_gateway("upstream_body", error.to_string()))?;
    if let Ok(response) = serde_json::from_slice::<Value>(&bytes) {
        record_response(
            continuity,
            &response,
            parent.as_deref(),
            &route_id,
            turn_input,
        );
    }
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type);
    if let Some(content_encoding) = content_encoding {
        builder = builder.header(header::CONTENT_ENCODING, content_encoding);
    }
    builder.body(Body::from(bytes)).map_err(|error| {
        ProxyError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "response",
            error.to_string(),
        )
    })
}

struct ResponseCapture {
    buffer: Vec<u8>,
}

impl ResponseCapture {
    fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    fn push(&mut self, bytes: &[u8]) -> Option<Value> {
        let mut completed = None;
        for frame in sse::frames(&mut self.buffer, bytes) {
            if let Some(response) = completed_response(&frame) {
                completed = Some(response);
            }
        }
        completed
    }

    fn finish(&mut self) -> Option<Value> {
        if self.buffer.is_empty() {
            return None;
        }
        let frame = std::mem::take(&mut self.buffer);
        completed_response(&frame)
    }
}

fn completed_response(frame: &[u8]) -> Option<Value> {
    let data = sse::data(frame)?;
    let event = serde_json::from_str::<Value>(&data).ok()?;
    (event.get("type").and_then(Value::as_str) == Some("response.completed"))
        .then(|| event.get("response").cloned())
        .flatten()
}

fn streaming_response(
    upstream: reqwest::Response,
    parent: Option<String>,
    turn_input: Vec<Value>,
    route_id: String,
    continuity: Arc<ContinuityStore>,
) -> Response<Body> {
    let source = upstream.bytes_stream();
    let output = stream! {
        let mut source = Box::pin(source);
        let mut capture = ResponseCapture::new();
        let mut recorded = false;
        let mut capture_bytes = 0usize;
        while let Some(chunk) = source.next().await {
            match chunk {
                Ok(chunk) => {
                    capture_bytes = capture_bytes.saturating_add(chunk.len());
                    if !recorded
                        && capture_bytes <= MAX_CAPTURE_BYTES
                        && let Some(response) = capture.push(&chunk)
                    {
                        record_response(
                            &continuity,
                            &response,
                            parent.as_deref(),
                            &route_id,
                            turn_input.clone(),
                        );
                        recorded = true;
                    }
                    yield Ok::<Bytes, io::Error>(chunk);
                }
                Err(error) => {
                    yield Err(io::Error::other(error));
                    return;
                }
            }
        }
        if !recorded && capture_bytes <= MAX_CAPTURE_BYTES
            && let Some(response) = capture.finish()
        {
            record_response(&continuity, &response, parent.as_deref(), &route_id, turn_input);
        }
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        )
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
        .body(Body::from_stream(output))
        .expect("stream response uses valid static headers")
}

fn record_response(
    continuity: &ContinuityStore,
    response: &Value,
    parent: Option<&str>,
    route_id: &str,
    mut input: Vec<Value>,
) {
    if response.get("status").and_then(Value::as_str) != Some("completed") {
        return;
    }
    let Some(id) = response.get("id").and_then(Value::as_str) else {
        return;
    };
    input.extend(portable_output_items(response.get("output")));
    continuity.record(id, parent, route_id, input);
}

#[derive(Debug)]
struct ProxyError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ProxyError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    fn unauthorized(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code, message)
    }

    fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    fn bad_gateway(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, code, message)
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(json!({
                "error": {
                    "message": self.message,
                    "type": self.code,
                    "code": self.code,
                    "param": Value::Null,
                }
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn spawn_test_app(app: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (address, handle)
    }

    #[derive(Clone, Default)]
    struct TestCapture(Arc<tokio::sync::Mutex<Vec<(HeaderMap, Value)>>>);

    fn auto_review_state(
        root: &std::path::Path,
        official_address: std::net::SocketAddr,
        cpa_address: std::net::SocketAddr,
    ) -> AppState {
        let catalog_path = root.join("catalog.json");
        let store = CatalogStore::load(catalog_path.clone()).unwrap();
        store
            .replace(
                &json!({"models":[{
                    "slug":catalog::AUTO_REVIEW_MODEL, "visibility":"hide"
                }]}),
                &json!({"models":[
                    {"slug":"codex-auto-review", "visibility":"hide"},
                    {"slug":"glm-5.3-flash", "visibility":"list"}
                ]}),
                &[],
            )
            .unwrap();
        drop(store);
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
            catalog_path,
            root.join("cpa-profiles.toml"),
        )
        .unwrap();
        state.official_base_url = format!("http://{official_address}/v1");
        state
    }

    #[tokio::test]
    async fn auto_review_uses_official_route_without_touching_cpa_when_it_succeeds() {
        async fn official(
            State(capture): State<TestCapture>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> Json<Value> {
            capture.0.lock().await.push((headers, body));
            Json(json!({
                "id":"resp_official_review", "status":"completed", "output":[]
            }))
        }
        async fn cpa(State(capture): State<TestCapture>) -> StatusCode {
            capture.0.lock().await.push((HeaderMap::new(), Value::Null));
            StatusCode::OK
        }

        let official_capture = TestCapture::default();
        let cpa_capture = TestCapture::default();
        let (official_address, official_handle) = spawn_test_app(
            Router::new()
                .route("/v1/responses", post(official))
                .with_state(official_capture.clone()),
        )
        .await;
        let (cpa_address, cpa_handle) = spawn_test_app(
            Router::new()
                .route("/v1/responses", post(cpa))
                .with_state(cpa_capture.clone()),
        )
        .await;
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), official_address, cpa_address);
        let (proxy_address, proxy_handle) = spawn_test_app(router(state)).await;

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .header("chatgpt-account-id", "account")
            .json(&json!({
                "model":catalog::AUTO_REVIEW_MODEL, "input":"review", "stream":false
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(official_capture.0.lock().await.len(), 1);
        assert!(cpa_capture.0.lock().await.is_empty());
        let captured = official_capture.0.lock().await;
        assert_eq!(captured[0].0[header::AUTHORIZATION], "Bearer oauth");
        assert_eq!(captured[0].0["chatgpt-account-id"], "account");
        assert_eq!(captured[0].1["model"], catalog::AUTO_REVIEW_MODEL);

        proxy_handle.abort();
        official_handle.abort();
        cpa_handle.abort();
    }

    #[tokio::test]
    async fn auto_review_does_not_fail_over_after_official_failure() {
        async fn official(
            State(capture): State<TestCapture>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> Response<Body> {
            capture.0.lock().await.push((headers, body));
            Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"error":"official quota exhausted"}"#))
                .unwrap()
        }
        async fn cpa(State(capture): State<TestCapture>) -> StatusCode {
            capture.0.lock().await.push((HeaderMap::new(), Value::Null));
            StatusCode::OK
        }

        let official_capture = TestCapture::default();
        let cpa_capture = TestCapture::default();
        let (official_address, official_handle) = spawn_test_app(
            Router::new()
                .route("/v1/responses", post(official))
                .with_state(official_capture.clone()),
        )
        .await;
        let (cpa_address, cpa_handle) = spawn_test_app(
            Router::new()
                .route("/v1/responses", post(cpa))
                .with_state(cpa_capture.clone()),
        )
        .await;
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), official_address, cpa_address);
        let (proxy_address, proxy_handle) = spawn_test_app(router(state)).await;

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .header("chatgpt-account-id", "account")
            .header("x-api-key", "incoming-secret")
            .json(&json!({
                "model":catalog::AUTO_REVIEW_MODEL, "input":"review", "stream":true
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.text().await.unwrap(),
            r#"{"error":"official quota exhausted"}"#
        );
        let official = official_capture.0.lock().await;
        assert_eq!(official.len(), 1);
        assert_eq!(official[0].0[header::AUTHORIZATION], "Bearer oauth");
        assert_eq!(official[0].0["chatgpt-account-id"], "account");
        assert_ne!(official[0].0[header::AUTHORIZATION], "Bearer cpa-secret");
        assert_eq!(official[0].1["model"], catalog::AUTO_REVIEW_MODEL);
        drop(official);
        assert!(cpa_capture.0.lock().await.is_empty());

        proxy_handle.abort();
        official_handle.abort();
        cpa_handle.abort();
    }

    #[tokio::test]
    async fn review_override_routes_straight_to_the_chosen_cpa_model() {
        async fn official(
            State(capture): State<TestCapture>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> Response<Body> {
            capture.0.lock().await.push((headers, body));
            // The official route MUST NOT be contacted when the override is set.
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
        async fn cpa(
            State(capture): State<TestCapture>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> Json<Value> {
            capture.0.lock().await.push((headers, body.clone()));
            Json(json!({
                "id": "resp_override", "object": "response", "status": "completed",
                "model": body["model"],
                "output": [{"type":"message","role":"assistant",
                "content":[{"type":"output_text","text":"overridden"}]}]
            }))
        }

        let official_capture = TestCapture::default();
        let cpa_capture = TestCapture::default();
        let (official_address, official_handle) = spawn_test_app(
            Router::new()
                .route("/v1/responses", post(official))
                .with_state(official_capture.clone()),
        )
        .await;
        let (cpa_address, cpa_handle) = spawn_test_app(
            Router::new()
                .route("/v1/responses", post(cpa))
                .with_state(cpa_capture.clone()),
        )
        .await;
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), official_address, cpa_address);

        // Set the override to glm-5.3-flash (a different CPA model).
        crate::cpa::set_review_override(
            root.path().join("cpa-profiles.toml").as_path(),
            Some("glm-5.3-flash".into()),
        )
        .unwrap();

        let (proxy_address, proxy_handle) = spawn_test_app(router(state)).await;
        let response = reqwest::Client::new()
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .json(&json!({
                "model":catalog::AUTO_REVIEW_MODEL, "input":"review", "stream":false
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["model"], "glm-5.3-flash");
        assert!(official_capture.0.lock().await.is_empty());
        let cpa = cpa_capture.0.lock().await;
        assert_eq!(cpa.len(), 1);
        assert_eq!(cpa[0].0[header::AUTHORIZATION], "Bearer cpa-secret");
        assert_eq!(cpa[0].1["model"], "glm-5.3-flash");
        drop(cpa);

        // Clearing the override restores the official route.
        let profiles_path = root.path().join("cpa-profiles.toml");
        crate::cpa::set_review_override(&profiles_path, None).unwrap();
        let state2 = auto_review_state(root.path(), official_address, cpa_address);
        let (proxy_address2, proxy_handle2) = spawn_test_app(router(state2)).await;

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_address2}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .json(&json!({
                "model":catalog::AUTO_REVIEW_MODEL, "input":"review", "stream":false
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(official_capture.0.lock().await.len(), 1);

        proxy_handle.abort();
        official_handle.abort();
        cpa_handle.abort();
        proxy_handle2.abort();
    }

    #[tokio::test]
    async fn direct_route_is_catalog_bound_and_credential_isolated() {
        async fn direct(
            State(capture): State<TestCapture>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> Json<Value> {
            capture.0.lock().await.push((headers, body.clone()));
            Json(json!({
                "id":"resp_direct", "status":"completed", "model":body["model"], "output":[]
            }))
        }

        let direct_capture = TestCapture::default();
        let (direct_address, direct_handle) = spawn_test_app(
            Router::new()
                .route("/v1/responses", post(direct))
                .with_state(direct_capture.clone()),
        )
        .await;
        let root = tempfile::tempdir().unwrap();
        let credentials = Credentials {
            proxy_token: "proxy".into(),
            cpa_token: "cpa-secret".into(),
            cpa_management_key: "management-secret".into(),
        };
        crate::secrets::save(&root.path().join("credentials.json"), &credentials).unwrap();
        let paths = crate::config::Paths::from_root(root.path().to_owned());
        crate::cpa::set_direct_routes(
            &paths,
            vec![crate::cpa::DirectRoute {
                base_url: format!("http://{direct_address}/v1"),
                token: "direct-secret".into(),
                models: vec!["direct-model".into(), "not-in-catalog".into()],
                model_aliases: std::collections::BTreeMap::from([(
                    "mapped-model".into(),
                    "provider/native-model".into(),
                )]),
            }],
        )
        .unwrap();

        let catalog_path = root.path().join("catalog.json");
        CatalogStore::load(catalog_path.clone())
            .unwrap()
            .replace(
                &json!({"models":[]}),
                &json!({"models":[{"slug":"direct-model"}]}),
                &[],
            )
            .unwrap();
        let state = AppState::new(
            Settings::default(),
            credentials,
            catalog_path,
            paths.cpa_profiles,
        )
        .unwrap();
        let (proxy_address, proxy_handle) = spawn_test_app(router(state)).await;
        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer official-oauth")
            .header("chatgpt-account-id", "official-account")
            .header("x-api-key", "incoming-secret")
            .json(&json!({"model":"cpa/direct-model","input":"hi"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let captured = direct_capture.0.lock().await;
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].0[header::AUTHORIZATION], "Bearer direct-secret");
        assert!(!captured[0].0.contains_key("chatgpt-account-id"));
        assert!(!captured[0].0.contains_key("x-api-key"));
        assert_ne!(captured[0].0[header::AUTHORIZATION], "Bearer cpa-secret");
        assert_eq!(captured[0].1["model"], "direct-model");
        drop(captured);

        let unknown = client
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .json(&json!({"model":"cpa/unknown-model","input":"hi"}))
            .send()
            .await
            .unwrap();
        assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
        assert_eq!(direct_capture.0.lock().await.len(), 1);

        // A declared direct model routes even when absent from the stored
        // catalog snapshot, so direct endpoints work without CPA.
        let declared = client
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .json(&json!({"model":"cpa/not-in-catalog","input":"hi"}))
            .send()
            .await
            .unwrap();
        assert_eq!(declared.status(), StatusCode::OK);
        assert_eq!(direct_capture.0.lock().await.len(), 2);
        assert_eq!(
            direct_capture.0.lock().await[1].1["model"],
            "not-in-catalog"
        );

        let mapped = client
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .json(&json!({"model":"cpa/mapped-model","input":"hi"}))
            .send()
            .await
            .unwrap();
        assert_eq!(mapped.status(), StatusCode::OK);
        assert_eq!(direct_capture.0.lock().await.len(), 3);
        assert_eq!(
            direct_capture.0.lock().await[2].1["model"],
            "provider/native-model"
        );

        proxy_handle.abort();
        direct_handle.abort();
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
        let state = AppState::new(
            settings,
            Credentials {
                proxy_token: "proxy".into(),
                cpa_token: "cpa-secret".into(),
                cpa_management_key: "management-secret".into(),
            },
            root.path().join("catalog.json"),
            root.path().join("cpa-profiles.toml"),
        )
        .unwrap();
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
        let catalog = refresh_catalog_from_urls(
            &state,
            router::official_headers(&incoming).unwrap(),
            router::cpa_headers(&incoming, &state.credentials.cpa_token).unwrap(),
            models_url(&format!("http://{official_address}"), "0.146.1").unwrap(),
            models_url(&format!("http://{cpa_address}/v1"), "0.146.1").unwrap(),
        )
        .await
        .unwrap();
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
        let state = AppState::new(
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
            root.path().join("catalog.json"),
            root.path().join("cpa-profiles.toml"),
        )
        .unwrap();
        let official_headers = router::official_headers(&HeaderMap::from_iter([(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer oauth"),
        )]))
        .unwrap();
        let cpa_headers =
            router::cpa_headers(&HeaderMap::new(), &state.credentials.cpa_token).unwrap();
        let official_url = models_url(&format!("http://{official_address}"), "test").unwrap();
        let cpa_url = models_url(&format!("http://{cpa_address}/v1"), "test").unwrap();

        refresh_catalog_from_urls(
            &state,
            official_headers.clone(),
            cpa_headers.clone(),
            official_url.clone(),
            cpa_url.clone(),
        )
        .await
        .unwrap();
        let after_omission =
            refresh_catalog_from_urls(&state, official_headers, cpa_headers, official_url, cpa_url)
                .await
                .unwrap();
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
        let path = root.path().join("catalog.json");
        let seed = CatalogStore::load(path.clone()).unwrap();
        seed.replace(
            &json!({"models":[{"slug":"official-saved"}]}),
            &json!({"models":[{"slug":"cpa-saved"}]}),
            &[],
        )
        .unwrap();
        drop(seed);
        let state = AppState::new(
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
            path,
            root.path().join("cpa-profiles.toml"),
        )
        .unwrap();

        let refresh = refresh_catalog_from_urls(
            &state,
            router::official_headers(&HeaderMap::from_iter([(
                header::AUTHORIZATION,
                HeaderValue::from_static("Bearer oauth"),
            )]))
            .unwrap(),
            router::cpa_headers(&HeaderMap::new(), &state.credentials.cpa_token).unwrap(),
            models_url("http://127.0.0.1:9", "test").unwrap(),
            models_url("http://127.0.0.1:9/v1", "test").unwrap(),
        )
        .await;
        assert!(refresh.is_err());
        let response = state.catalog.current().unwrap();
        assert_eq!(response["models"][0]["slug"], "official-saved");
        assert_eq!(response["models"][1]["slug"], "cpa/cpa-saved");
    }

    #[tokio::test]
    async fn models_endpoint_degrades_to_official_plus_direct_when_cpa_is_down() {
        async fn official(Query(query): Query<ModelsQuery>, _headers: HeaderMap) -> Json<Value> {
            assert_eq!(query.client_version, "degraded");
            Json(json!({"models":[{"slug":"gpt-5.6", "priority":1}]}))
        }
        let (official_address, official_handle) =
            spawn_test_app(Router::new().route("/models", get(official))).await;

        let root = tempfile::tempdir().unwrap();
        let credentials = Credentials {
            proxy_token: "proxy".into(),
            cpa_token: "cpa-secret".into(),
            cpa_management_key: "management-secret".into(),
        };
        crate::secrets::save(&root.path().join("credentials.json"), &credentials).unwrap();
        let paths = crate::config::Paths::from_root(root.path().to_owned());
        crate::cpa::add_direct_route(
            &paths,
            "https://direct.example/v1".into(),
            "direct-secret".into(),
            vec!["gpt-5.6-sol".into()],
        )
        .unwrap();

        // No stored snapshot: the fresh state has never seen a catalog.
        let declared = crate::cpa::declared_direct_models(&paths.cpa_profiles);
        let state = AppState::new(
            Settings {
                cpa: crate::config::Cpa {
                    // Port 9 (discard) is unreachable: CPA is down.
                    base_url: "http://127.0.0.1:9/v1".into(),
                },
                ..Settings::default()
            },
            credentials,
            root.path().join("catalog.json"),
            paths.cpa_profiles,
        )
        .unwrap();

        let catalog = refresh_catalog_from_urls(
            &state,
            router::official_headers(&HeaderMap::from_iter([(
                header::AUTHORIZATION,
                HeaderValue::from_static("Bearer oauth"),
            )]))
            .unwrap(),
            router::cpa_headers(&HeaderMap::new(), &state.credentials.cpa_token).unwrap(),
            models_url(&format!("http://{official_address}"), "degraded").unwrap(),
            models_url("http://127.0.0.1:9/v1", "degraded").unwrap(),
        )
        .await
        .unwrap();
        let slugs: Vec<&str> = catalog["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["slug"].as_str().unwrap())
            .collect();
        assert_eq!(slugs, vec!["gpt-5.6", "cpa/gpt-5.6-sol"]);
        // The degraded view is not persisted as a snapshot.
        assert!(state.catalog.current().is_none());
        // Declared direct models still resolve without any snapshot.
        assert_eq!(
            state
                .catalog
                .resolve_with_direct("cpa/gpt-5.6-sol", &declared)
                .unwrap(),
            CatalogRoute::Cpa {
                upstream_model: "gpt-5.6-sol".into()
            }
        );

        official_handle.abort();
    }

    #[test]
    fn route_identity_is_model_specific() {
        assert_ne!(Route::Cpa.identity("one"), Route::Cpa.identity("two"));
        assert_ne!(
            Route::Official.identity("one"),
            Route::Official.identity("two")
        );
    }

    #[test]
    fn continuity_rewrites_every_cpa_followup_and_cross_route_switch() {
        let store = ContinuityStore::new();
        let history = vec![json!({
            "type":"message", "role":"user",
            "content":[{"type":"input_text","text":"first"}]
        })];
        store.record(
            "resp_official",
            None,
            &Route::Official.identity("gpt"),
            history.clone(),
        );
        store.record("resp_cpa", None, &Route::Cpa.identity("claude"), history);
        let turn = portable_input_items(Some(&json!("second")));

        for (parent, route, model) in [
            ("resp_official", &Route::Cpa, "claude"),
            ("resp_cpa", &Route::Official, "gpt"),
            ("resp_cpa", &Route::Cpa, "claude"),
        ] {
            let mut request = json!({
                "model":model, "previous_response_id":parent, "input":"second"
            });
            assert!(
                rewrite_for_continuity(
                    &mut request,
                    Some(parent),
                    &turn,
                    route,
                    &route.identity(model),
                    &store,
                )
                .unwrap()
            );
            assert!(request.get("previous_response_id").is_none());
            assert_eq!(request["input"].as_array().unwrap().len(), 2);
        }
    }

    #[test]
    fn official_same_model_keeps_id_but_model_switch_replays() {
        let store = ContinuityStore::new();
        store.record(
            "resp_official",
            None,
            &Route::Official.identity("gpt-a"),
            portable_input_items(Some(&json!("first"))),
        );
        let turn = portable_input_items(Some(&json!("second")));

        let mut same = json!({
            "model":"gpt-a", "previous_response_id":"resp_official", "input":"second"
        });
        assert!(
            !rewrite_for_continuity(
                &mut same,
                Some("resp_official"),
                &turn,
                &Route::Official,
                &Route::Official.identity("gpt-a"),
                &store,
            )
            .unwrap()
        );
        assert_eq!(same["previous_response_id"], "resp_official");

        let mut switched = json!({
            "model":"gpt-b", "previous_response_id":"resp_official", "input":"second"
        });
        assert!(
            rewrite_for_continuity(
                &mut switched,
                Some("resp_official"),
                &turn,
                &Route::Official,
                &Route::Official.identity("gpt-b"),
                &store,
            )
            .unwrap()
        );
        assert!(switched.get("previous_response_id").is_none());
    }
}
