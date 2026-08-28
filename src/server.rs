use std::{fmt, io, path::PathBuf, sync::Arc, time::Duration};

use async_stream::stream;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{OriginalUri, Query, Request, State},
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

use crate::{
    catalog::{self, CatalogRoute, CatalogStore},
    config::{Credentials, OFFICIAL_BASE_URL, Settings},
    continuity::{ContinuityStore, portable_input_items, portable_output_items},
    dialect::sse,
    router,
};

const MAX_CAPTURE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    Official,
    Cpa,
}

impl Route {
    fn identity(self, model: &str) -> String {
        match self {
            Self::Official => format!("official:{model}"),
            Self::Cpa => format!("cpa:{model}"),
        }
    }

    fn always_replays_history(self) -> bool {
        self == Self::Cpa
    }
}

#[derive(Clone)]
pub struct AppState {
    client: reqwest::Client,
    settings: Arc<Settings>,
    credentials: Arc<Credentials>,
    catalog: Arc<CatalogStore>,
    continuity: Arc<ContinuityStore>,
}

impl AppState {
    pub fn new(
        settings: Settings,
        credentials: Credentials,
        catalog_path: PathBuf,
    ) -> anyhow::Result<Self> {
        settings.validate()?;
        credentials.validate()?;
        let catalog = CatalogStore::load(catalog_path)?;
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .build()?,
            settings: Arc::new(settings),
            credentials: Arc::new(credentials),
            catalog: Arc::new(catalog),
            continuity: Arc::new(ContinuityStore::new()),
        })
    }
}

pub async fn serve(state: AppState) -> anyhow::Result<()> {
    let address = state.settings.listen;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "ModelMux listening");
    axum::serve(listener, router(state)).await?;
    Ok(())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(handle_models))
        .route("/v1/responses", post(handle_responses))
        .route("/v1/responses/compact", post(handle_responses))
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
    Ok(next.run(request).await)
}

async fn health() -> Json<Value> {
    Json(json!({"ok": true, "service": "modelmux"}))
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
    let cpa_headers = router::cpa_headers(incoming_headers, &state.credentials.cpa_token)
        .map_err(|error| ProxyError::unauthorized("credential", error.to_string()))?;
    let official_url = models_url(OFFICIAL_BASE_URL, client_version)?;
    let cpa_url = models_url(&state.settings.cpa.base_url, client_version)?;
    refresh_catalog_from_urls(state, official_headers, cpa_headers, official_url, cpa_url).await
}

async fn refresh_catalog_from_urls(
    state: &AppState,
    official_headers: HeaderMap,
    cpa_headers: HeaderMap,
    official_url: reqwest::Url,
    cpa_url: reqwest::Url,
) -> Result<Value, ProxyError> {
    let (official, cpa) = tokio::join!(
        fetch_catalog(&state.client, official_url, official_headers, "official"),
        fetch_catalog(&state.client, cpa_url, cpa_headers, "CPA")
    );
    let official = official?;
    let cpa = cpa?;
    state
        .catalog
        .replace(&official, &cpa)
        .map_err(|error| ProxyError::bad_gateway("catalog", error.to_string()))
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
    let mut request = Value::Object(object);
    let model = request
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ProxyError::bad_request("missing_model", "request has no string model"))?;
    let catalog_route = state
        .catalog
        .resolve(&model)
        .map_err(|error| ProxyError::bad_request("route", error.to_string()))?;
    let (route, cpa_upstream_model) = match catalog_route {
        CatalogRoute::Official => (Route::Official, None),
        CatalogRoute::Cpa { upstream_model } => (Route::Cpa, Some(upstream_model)),
    };

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
    let route_id = route.identity(&model);
    let continuity_rewritten = rewrite_for_continuity(
        &mut request,
        parent.as_deref(),
        &turn_input,
        route,
        &route_id,
        &state.continuity,
    )?;
    let model_rewritten = if let Some(upstream_model) = cpa_upstream_model {
        request
            .as_object_mut()
            .expect("top-level request was parsed as an object")
            .insert("model".into(), Value::String(upstream_model));
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
        body
    };
    let upstream_headers = match route {
        Route::Official => router::official_headers(&headers),
        Route::Cpa => router::cpa_headers(&headers, &state.credentials.cpa_token),
    }
    .map_err(|error| ProxyError::unauthorized("credential", error.to_string()))?;
    let target = target_url(&state.settings, route, endpoint);
    let upstream = state
        .client
        .post(target)
        .headers(upstream_headers)
        .body(outgoing)
        .send()
        .await
        .map_err(|error| ProxyError::bad_gateway("upstream", error.to_string()))?;
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
            upstream,
            parent,
            turn_input,
            route_id,
            state.continuity,
        ))
    } else {
        non_streaming_response(upstream, parent, turn_input, route_id, &state.continuity).await
    }
}

fn rewrite_for_continuity(
    request: &mut Value,
    parent: Option<&str>,
    turn_input: &[Value],
    route: Route,
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
        .get("x-modelmux-token")
        .and_then(|value| value.to_str().ok());
    if supplied != Some(expected) {
        return Err(ProxyError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "missing or invalid ModelMux token",
        ));
    }
    Ok(())
}

fn target_url(settings: &Settings, route: Route, endpoint: &str) -> String {
    let base_url = match route {
        Route::Official => OFFICIAL_BASE_URL,
        Route::Cpa => &settings.cpa.base_url,
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
                    if !recorded && capture_bytes <= MAX_CAPTURE_BYTES {
                        if let Some(response) = capture.push(&chunk) {
                            record_response(
                                &continuity,
                                &response,
                                parent.as_deref(),
                                &route_id,
                                turn_input.clone(),
                            );
                            recorded = true;
                        }
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
            },
            root.path().join("catalog.json"),
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
    async fn models_endpoint_serves_saved_snapshot_when_refresh_fails() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("catalog.json");
        let seed = CatalogStore::load(path.clone()).unwrap();
        seed.replace(
            &json!({"models":[{"slug":"official-saved"}]}),
            &json!({"models":[{"slug":"cpa-saved"}]}),
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
            },
            path,
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
            ("resp_official", Route::Cpa, "claude"),
            ("resp_cpa", Route::Official, "gpt"),
            ("resp_cpa", Route::Cpa, "claude"),
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
                Route::Official,
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
                Route::Official,
                &Route::Official.identity("gpt-b"),
                &store,
            )
            .unwrap()
        );
        assert!(switched.get("previous_response_id").is_none());
    }
}
