//! Responses forwarding: routing by model, continuity replay, and relaying
//! upstream responses while capturing the turns that are recorded.

use std::{fmt, io, sync::Arc};

use async_stream::stream;
use axum::{
    body::{Body, Bytes},
    extract::{OriginalUri, State},
    http::{HeaderMap, HeaderValue, Response, StatusCode, header},
    response::IntoResponse,
};
use futures_util::StreamExt;
use memchr::memmem;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::{Map, Value, json};
use tokio::time::Instant;

use super::{
    AppState, ProxyError, RequestBody, Route,
    logging::RequestLogSummary,
    search::apply_shared_search,
    upstream::{
        MAX_AUXILIARY_BODY_BYTES, is_event_stream, passthrough_response, read_body_limited,
        send_upstream, upstream_error_message, upstream_response,
    },
};
use crate::{
    catalog::CatalogRoute,
    config::CPA_MODEL_PREFIX,
    continuity::{ContinuityStore, portable_input_items, portable_output_items},
    sse,
};

/// Largest non-streaming Responses body CodexMux buffers to record it.
const MAX_RESPONSE_BODY_BYTES: usize = 64 * 1024 * 1024;
const CODEX_SERVER_OVERLOADED_MESSAGE: &str =
    "Selected model is at capacity. Please try a different model.";

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Endpoint {
    Responses,
    Compact,
}

impl Endpoint {
    pub(super) fn path(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::Compact => "responses/compact",
        }
    }
}

/// Where one Responses request goes once `codex-auto-review` is resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Target {
    Official,
    /// `slug` is the merged catalog slug, which search capability checks use;
    /// `upstream_model` replaces the request's `model` on the wire.
    Cpa {
        slug: String,
        upstream_model: String,
    },
}

impl Target {
    pub(super) fn route(&self) -> Route {
        match self {
            Self::Official => Route::Official,
            Self::Cpa { .. } => Route::Cpa,
        }
    }

    pub(super) fn upstream_model(&self) -> Option<&str> {
        match self {
            Self::Official => None,
            Self::Cpa { upstream_model, .. } => Some(upstream_model),
        }
    }
}

/// One parsed Responses request. The client's bytes are forwarded unchanged
/// unless continuity, search, or the CPA model slug requires a rewrite.
pub(super) struct ResponseRequest {
    body: Bytes,
    object: Map<String, Value>,
    model: String,
    parent: Option<String>,
    rewritten: bool,
}

impl ResponseRequest {
    pub(super) fn parse(body: Bytes) -> Result<Self, ProxyError> {
        let UniqueObject(object) = serde_json::from_slice(&body)
            .map_err(|error| ProxyError::bad_request("invalid_json", error.to_string()))?;
        let model = object
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                ProxyError::bad_request("missing_model", "request has no string model")
            })?;
        let parent = match object.get("previous_response_id") {
            Some(Value::String(parent)) if !parent.is_empty() => Some(parent.clone()),
            None | Some(Value::Null) => None,
            Some(_) => {
                return Err(ProxyError::bad_request(
                    "invalid_previous_response_id",
                    "previous_response_id must be a nonempty string or null",
                ));
            }
        };
        Ok(Self {
            body,
            object,
            model,
            parent,
            rewritten: false,
        })
    }

    /// Codex sends `store: false` and never chains with
    /// `previous_response_id`, so recording its turns would only cost memory.
    /// A turn is recorded when the client stores responses or continues a
    /// chain that later turns may reference.
    fn records_history(&self) -> bool {
        self.parent.is_some() || self.object.get("store") != Some(&Value::Bool(false))
    }

    pub(super) fn model(&self) -> &str {
        &self.model
    }

    pub(super) fn has_parent(&self) -> bool {
        self.parent.is_some()
    }

    pub(super) fn body_len(&self) -> usize {
        self.body.len()
    }

    pub(super) fn object(&self) -> &Map<String, Value> {
        &self.object
    }

    /// Mutable access to the request object. The request is re-serialized
    /// instead of forwarding the client's bytes.
    pub(super) fn edit(&mut self) -> &mut Map<String, Value> {
        self.rewritten = true;
        &mut self.object
    }

    /// Replace `previous_response_id` with the replayable history, followed
    /// by this turn's own input.
    fn replay_after(&mut self, mut history: Vec<Value>) {
        let object = self.edit();
        history.extend(current_turn_items(object.remove("input")));
        crate::continuity::balance_tool_calls(&mut history);
        object.insert("input".into(), Value::Array(history));
        object.remove("previous_response_id");
    }

    pub(super) fn into_body(mut self, upstream_model: Option<&str>) -> Result<Bytes, ProxyError> {
        if let Some(upstream_model) = upstream_model {
            self.edit()
                .insert("model".into(), Value::String(upstream_model.to_owned()));
        }
        if !self.rewritten {
            return Ok(self.body);
        }
        drop(self.body);
        serde_json::to_vec(&self.object)
            .map(Bytes::from)
            .map_err(|error| ProxyError::bad_request("json", error.to_string()))
    }
}

pub(super) async fn handle_responses(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: RequestBody,
) -> Result<Response<Body>, ProxyError> {
    let endpoint = if uri.path().ends_with("/responses/compact") {
        Endpoint::Compact
    } else {
        Endpoint::Responses
    };
    let request = ResponseRequest::parse(body.0)?;
    let summary = RequestLogSummary::of(&request);
    let started = Instant::now();
    let target = match resolve_target(&state, &request.model) {
        Ok(target) => target,
        Err(error) => {
            summary.log_failure(endpoint, None, started, &error);
            return Err(error);
        }
    };
    tracing::debug!(
        model = %request.model,
        endpoint = endpoint.path(),
        route = target.route().name(),
        "response route resolved"
    );
    let result = forward_turn(&state, &headers, endpoint, &target, request).await;
    match &result {
        Ok(response) => summary.log_success(endpoint, &target, started, response.status()),
        Err(error) => summary.log_failure(endpoint, Some(&target), started, error),
    }
    result
}

/// Resolve the request model to its target. `codex-auto-review` stays on the
/// official route unless the user pinned a CPA model for it; only that model
/// reads the review override.
fn resolve_target(state: &AppState, model: &str) -> Result<Target, ProxyError> {
    let route = state
        .catalog
        .resolve(model)
        .map_err(|error| ProxyError::bad_request("route", format!("{error:#}")))?;
    match route {
        CatalogRoute::Official => Ok(Target::Official),
        CatalogRoute::Cpa { upstream_model } => Ok(Target::Cpa {
            slug: model.to_owned(),
            upstream_model,
        }),
        CatalogRoute::AutoReview => {
            let Some(pinned) = crate::cpa::review_override(&state.cpa_profiles_path)
                .map_err(ProxyError::local_config)?
            else {
                return Ok(Target::Official);
            };
            let slug = format!("{CPA_MODEL_PREFIX}{pinned}");
            match state.catalog.resolve(&slug) {
                Ok(CatalogRoute::Cpa { upstream_model }) => Ok(Target::Cpa {
                    slug,
                    upstream_model,
                }),
                _ => Err(ProxyError::bad_request(
                    "review_override",
                    format!("review override model {slug} is not in the catalog"),
                )),
            }
        }
    }
}

async fn forward_turn(
    state: &AppState,
    headers: &HeaderMap,
    endpoint: Endpoint,
    target: &Target,
    mut request: ResponseRequest,
) -> Result<Response<Body>, ProxyError> {
    let route = target.route();
    let route_id = route.identity(&request.model);
    // Continuity is settled before anything slow runs, so a turn that must
    // fail does not first wait for a shared search.
    let history = replay_history(
        &state.continuity,
        request.parent.as_deref(),
        route,
        &route_id,
    )?;
    // The recorded turn is the client's own input, so injected search
    // context never accumulates in replayed history.
    let recorder = request.records_history().then(|| Recorder {
        continuity: state.continuity.clone(),
        parent: request.parent.clone(),
        route_id: route_id.clone(),
        input: portable_input_items(request.object.get("input")),
    });
    apply_shared_search(state, headers, endpoint, target, &mut request).await?;
    if let Some(history) = history {
        request.replay_after(history);
    }
    let body = request.into_body(target.upstream_model())?;
    let upstream = send_upstream(state, headers, endpoint.path(), route, body).await?;
    if route == Route::Cpa
        && state.settings.map_capacity_errors
        && matches!(upstream.status().as_u16(), 429 | 502 | 503)
        && !upstream.headers().contains_key(header::CONTENT_ENCODING)
    {
        return cpa_capacity_error_response(upstream).await;
    }
    finish_response(upstream, recorder, route_id).await
}

/// The history that replaces `previous_response_id`, or `None` when the id
/// may be forwarded as is. Unknown, ambiguous, and incomplete chains fail
/// closed rather than sending an id to a route that did not issue it.
fn replay_history(
    continuity: &ContinuityStore,
    parent: Option<&str>,
    route: Route,
    route_id: &str,
) -> Result<Option<Vec<Value>>, ProxyError> {
    let Some(parent) = parent else {
        return Ok(None);
    };
    let incomplete = || {
        ProxyError::conflict(
            "incomplete_history",
            "cannot continue because the previous response chain is incomplete",
        )
    };
    let previous_route = continuity.route_of(parent).ok_or_else(|| {
        ProxyError::conflict(
            "unknown_history",
            "cannot continue because the previous response id is not in local history",
        )
    })?;
    if !continuity.is_complete(parent) {
        return Err(incomplete());
    }
    if !route.always_replays_history() && previous_route == route_id {
        return Ok(None);
    }
    continuity
        .materialize(parent)
        .map(Some)
        .ok_or_else(incomplete)
}

/// This turn's input as the client sent it, minus provider-private state:
/// reasoning items, encrypted content, signatures, and the item ids and
/// statuses another provider assigned. Unlike replayed history, the images
/// and files attached to this turn stay.
fn current_turn_items(input: Option<Value>) -> Vec<Value> {
    match input {
        Some(Value::String(text)) => vec![user_text_item(&text)],
        Some(Value::Array(items)) => items.into_iter().filter_map(current_turn_item).collect(),
        _ => Vec::new(),
    }
}

fn current_turn_item(mut item: Value) -> Option<Value> {
    if let Some(object) = item.as_object_mut() {
        if object.get("type").and_then(Value::as_str) == Some("reasoning") {
            return None;
        }
        object.remove("id");
        object.remove("status");
    }
    strip_private_fields(&mut item);
    match &item {
        // An item that carried nothing but private state, such as an
        // encrypted compaction or an item reference, has nothing left to send.
        Value::Object(object) if object.keys().all(|key| key == "type") => None,
        _ => Some(item),
    }
}

fn strip_private_fields(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("encrypted_content");
            object.remove("signature");
            object.values_mut().for_each(strip_private_fields);
        }
        Value::Array(values) => values.iter_mut().for_each(strip_private_fields),
        _ => {}
    }
}

pub(super) fn user_text_item(text: &str) -> Value {
    json!({
        "role": "user",
        "content": [{"type": "input_text", "text": text}]
    })
}

/// Records one completed turn for continuity: the client's portable input
/// followed by the portable output of the completed response.
struct Recorder {
    continuity: Arc<ContinuityStore>,
    parent: Option<String>,
    route_id: String,
    input: Vec<Value>,
}

impl Recorder {
    fn record(self, response: &Value) {
        if response.get("status").and_then(Value::as_str) != Some("completed") {
            return;
        }
        let Some(id) = response.get("id").and_then(Value::as_str) else {
            return;
        };
        let mut items = self.input;
        items.extend(portable_output_items(response.get("output")));
        self.continuity
            .record(id, self.parent.as_deref(), &self.route_id, items);
    }
}

async fn finish_response(
    upstream: reqwest::Response,
    recorder: Option<Recorder>,
    route_id: String,
) -> Result<Response<Body>, ProxyError> {
    if !upstream.status().is_success() || upstream.headers().contains_key(header::CONTENT_ENCODING)
    {
        return Ok(passthrough_response(upstream));
    }
    if is_event_stream(upstream.headers()) {
        return Ok(streaming_response(upstream, recorder, route_id));
    }
    let Some(recorder) = recorder else {
        return Ok(passthrough_response(upstream));
    };
    let status = upstream.status();
    let headers = upstream.headers().clone();
    let bytes = read_body_limited(
        upstream,
        MAX_RESPONSE_BODY_BYTES,
        "upstream_body",
        "upstream response body",
    )
    .await?;
    if let Ok(response) = serde_json::from_slice::<Value>(&bytes) {
        recorder.record(&response);
    }
    Ok(upstream_response(status, &headers, Body::from(bytes)))
}

/// Relay an SSE response chunk by chunk. When the turn is recorded, the
/// `response.completed` event is captured on the way through, without
/// waiting for the upstream to close the stream.
fn streaming_response(
    upstream: reqwest::Response,
    recorder: Option<Recorder>,
    route_id: String,
) -> Response<Body> {
    let mut response = upstream_response(upstream.status(), upstream.headers(), Body::empty());
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    let source = upstream.bytes_stream();
    let output = stream! {
        let mut source = Box::pin(source);
        let mut capture = recorder.map(|recorder| (recorder, CompletedCapture::default()));
        while let Some(chunk) = source.next().await {
            match chunk {
                Ok(chunk) => {
                    let completed = capture.as_mut().and_then(|(_, events)| events.push(&chunk));
                    if let Some(completed) = completed
                        && let Some((recorder, _)) = capture.take()
                    {
                        recorder.record(&completed);
                    }
                    yield Ok::<Bytes, io::Error>(chunk);
                }
                Err(error) => {
                    let error = upstream_error_message(error);
                    tracing::warn!(route_id = %route_id, %error, "upstream response stream failed");
                    yield Err(io::Error::other(error));
                    return;
                }
            }
        }
        tracing::debug!(route_id = %route_id, "upstream response stream completed");
        if let Some((recorder, mut events)) = capture
            && let Some(completed) = events.finish()
        {
            recorder.record(&completed);
        }
    };
    *response.body_mut() = Body::from_stream(output);
    response
}

/// Finds the `response.completed` event of an SSE stream.
#[derive(Default)]
pub(super) struct CompletedCapture {
    events: sse::EventSplitter,
}

impl CompletedCapture {
    pub(super) fn push(&mut self, chunk: &[u8]) -> Option<Value> {
        let mut completed = None;
        self.events.push(chunk, |event| {
            if let Some(response) = completed_response(event) {
                completed = Some(response);
            }
        });
        completed
    }

    /// End of stream, including a final event that lost its blank line.
    pub(super) fn finish(&mut self) -> Option<Value> {
        let mut completed = None;
        self.events
            .finish(|event| completed = completed_response(event));
        completed
    }
}

/// The `response` object of a `response.completed` event. Only events that
/// mention that type are parsed, so text and tool-call deltas cost no JSON
/// work.
fn completed_response(event: &[u8]) -> Option<Value> {
    memmem::find(event, b"response.completed")?;
    let mut event: Value = serde_json::from_slice(&sse::data(event)?).ok()?;
    if event.get("type").and_then(Value::as_str) != Some("response.completed") {
        return None;
    }
    event.get_mut("response").map(Value::take)
}

async fn cpa_capacity_error_response(
    upstream: reqwest::Response,
) -> Result<Response<Body>, ProxyError> {
    let status = upstream.status();
    let headers = upstream.headers().clone();
    let bytes = read_body_limited(
        upstream,
        MAX_AUXILIARY_BODY_BYTES,
        "upstream_body",
        "upstream error body",
    )
    .await?;
    if serde_json::from_slice::<Value>(&bytes)
        .is_ok_and(|response| is_cpa_capacity_error(status, &response))
    {
        tracing::info!(
            upstream_status = %status,
            "CPA capacity error mapped to Codex server_is_overloaded"
        );
        return Ok(codex_server_overloaded_response());
    }
    Ok(upstream_response(status, &headers, Body::from(bytes)))
}

fn is_cpa_capacity_error(status: StatusCode, response: &Value) -> bool {
    let Some(error) = response.get("error").and_then(Value::as_object) else {
        return false;
    };
    let code = error.get("code").and_then(Value::as_str).unwrap_or("");
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(code, "server_is_overloaded" | "slow_down") {
        return false;
    }
    if status == StatusCode::TOO_MANY_REQUESTS && code == "model_cooldown" {
        return true;
    }
    code == "unavailable"
        || message.contains("temporarily unavailable")
        || message.contains("auth unavailable")
        || message.contains("no auth available")
        || message.contains("cooldown")
        || message.contains("cloudflare challenge")
}

/// Codex shows its capacity retry countdown for exactly this error.
fn codex_server_overloaded_response() -> Response<Body> {
    ProxyError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "server_is_overloaded",
        CODEX_SERVER_OVERLOADED_MESSAGE,
    )
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, response::Json, routing::post};

    use crate::catalog;
    use crate::server::test_support::*;

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
        let (proxy_address, proxy_handle) = spawn_proxy(state).await;

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .header("chatgpt-account-id", "account")
            .header("x-openai-subagent", "guardian")
            .header("session-id", "sess")
            .header("thread-id", "thread")
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
        assert_eq!(captured[0].0["x-openai-subagent"], "guardian");
        assert_eq!(captured[0].0["session-id"], "sess");
        assert_eq!(captured[0].0["thread-id"], "thread");
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
        let (proxy_address, proxy_handle) = spawn_proxy(state).await;

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

        let (proxy_address, proxy_handle) = spawn_proxy(state).await;
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
        let (proxy_address2, proxy_handle2) = spawn_proxy(state2).await;

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

        for (parent, route, model) in [
            ("resp_official", Route::Cpa, "claude"),
            ("resp_cpa", Route::Official, "gpt"),
            ("resp_cpa", Route::Cpa, "claude"),
        ] {
            let history = replay_history(&store, Some(parent), route, &route.identity(model))
                .unwrap()
                .expect("the id cannot be forwarded as is");
            let mut turn = request(json!({
                "model":model, "previous_response_id":parent, "input":"second"
            }));
            turn.replay_after(history);
            let body = forwarded(turn, None);
            assert!(body.get("previous_response_id").is_none());
            assert_eq!(body["input"].as_array().unwrap().len(), 2);
            assert_eq!(body["input"][1]["content"][0]["text"], "second");
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
        let same = replay_history(
            &store,
            Some("resp_official"),
            Route::Official,
            &Route::Official.identity("gpt-a"),
        )
        .unwrap();
        assert!(same.is_none());
        let body = Bytes::from_static(
            br#"{"model":"gpt-a","previous_response_id":"resp_official","input":"second"}"#,
        );
        let unchanged = ResponseRequest::parse(body.clone()).unwrap();
        assert_eq!(unchanged.into_body(None).unwrap(), body);

        let switched = replay_history(
            &store,
            Some("resp_official"),
            Route::Official,
            &Route::Official.identity("gpt-b"),
        )
        .unwrap();
        assert_eq!(switched.unwrap().len(), 1);
    }

    #[test]
    fn unknown_and_incomplete_chains_fail_closed() {
        let store = ContinuityStore::new();
        let error = replay_history(&store, Some("resp_unknown"), Route::Cpa, "cpa:x").unwrap_err();
        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.code, "unknown_history");

        store.record("resp_child", Some("resp_missing"), "cpa:x", Vec::new());
        let error = replay_history(&store, Some("resp_child"), Route::Cpa, "cpa:x").unwrap_err();
        assert_eq!(error.code, "incomplete_history");
        assert!(
            replay_history(&store, None, Route::Cpa, "cpa:x")
                .unwrap()
                .is_none()
        );
    }

    /// The replayed history keeps only portable items, but the turn being
    /// sent now is the client's own input: its images and files stay, while
    /// reasoning, encrypted state, signatures, and foreign ids go.
    #[test]
    fn replay_keeps_this_turns_attachments_and_drops_private_state() {
        let items = current_turn_items(Some(json!([
            {"type":"reasoning", "id":"rs_1", "encrypted_content":"secret-reasoning", "summary":[]},
            {"type":"message", "role":"user", "id":"msg_foreign", "status":"completed", "content":[
                {"type":"input_text", "text":"look at these"},
                {"type":"input_image", "image_url":"data:image/png;base64,AAAA"},
                {"type":"input_file", "filename":"notes.pdf", "file_data":"data:application/pdf;base64,JVBE"}
            ]},
            {"type":"function_call", "id":"fc_foreign", "call_id":"call_1", "name":"view_image",
             "arguments":"{}", "signature":"secret-signature"},
            {"type":"function_call_output", "call_id":"call_1", "output":[
                {"type":"input_image", "image_url":"data:image/png;base64,BBBB", "encrypted_content":"secret-part"}
            ]},
            {"type":"compaction", "encrypted_content":"secret-compaction"},
            {"type":"item_reference", "id":"msg_stored"}
        ])));
        assert_eq!(
            items,
            vec![
                json!({"type":"message", "role":"user", "content":[
                    {"type":"input_text", "text":"look at these"},
                    {"type":"input_image", "image_url":"data:image/png;base64,AAAA"},
                    {"type":"input_file", "filename":"notes.pdf", "file_data":"data:application/pdf;base64,JVBE"}
                ]}),
                json!({"type":"function_call", "call_id":"call_1", "name":"view_image", "arguments":"{}"}),
                json!({"type":"function_call_output", "call_id":"call_1", "output":[
                    {"type":"input_image", "image_url":"data:image/png;base64,BBBB"}
                ]}),
            ]
        );
        assert_eq!(
            current_turn_items(Some(json!("plain"))),
            vec![user_text_item("plain")]
        );
        assert!(current_turn_items(None).is_empty());
    }

    /// Every replayed call keeps its output: a call whose output did not
    /// survive (or an output whose call was evicted) is dropped as a pair,
    /// while a call answered in this turn stays.
    #[test]
    fn replay_keeps_tool_calls_paired_with_their_outputs() {
        let mut request = ResponseRequest::parse(Bytes::from(
            serde_json::to_vec(&json!({
                "model":"cpa/x", "previous_response_id":"resp_1",
                "input":[{"type":"function_call_output", "call_id":"call_answered", "output":"done"}]
            }))
            .unwrap(),
        ))
        .unwrap();
        request.replay_after(vec![
            json!({"type":"function_call", "call_id":"call_answered", "name":"a", "arguments":"{}"}),
            json!({"type":"function_call", "call_id":"call_lonely", "name":"b", "arguments":"{}"}),
        ]);
        let body: Value = serde_json::from_slice(&request.into_body(None).unwrap()).unwrap();
        let call_ids: Vec<_> = body["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["call_id"].as_str().unwrap())
            .collect();
        assert_eq!(call_ids, ["call_answered", "call_answered"]);
        assert!(body.get("previous_response_id").is_none());
    }

    /// Restoring the upstream model id reserializes the request; every
    /// other key, including tool schema properties, keeps the client's order.
    #[test]
    fn model_rewrite_keeps_the_clients_key_order() {
        let body = br#"{"model":"cpa/x","tools":[{"type":"function","name":"f","parameters":{"type":"object","properties":{"zeta":{"type":"string"},"alpha":{"type":"string"}}}}],"input":"hi"}"#;
        let request = ResponseRequest::parse(Bytes::from_static(body)).unwrap();
        let rewritten = request.into_body(Some("x")).unwrap();
        assert_eq!(
            std::str::from_utf8(&rewritten).unwrap(),
            r#"{"model":"x","tools":[{"type":"function","name":"f","parameters":{"type":"object","properties":{"zeta":{"type":"string"},"alpha":{"type":"string"}}}}],"input":"hi"}"#
        );
    }

    #[test]
    fn a_request_without_rewrites_is_forwarded_byte_for_byte() {
        let body =
            Bytes::from_static(b"{ \"model\" : \"gpt\", \"input\" : \"hi\", \"store\" : false }");
        let request = ResponseRequest::parse(body.clone()).unwrap();
        assert!(!request.records_history());
        assert_eq!(request.into_body(None).unwrap(), body);

        let request = ResponseRequest::parse(body).unwrap();
        assert_eq!(forwarded(request, Some("upstream"))["model"], "upstream");
    }

    /// Codex sends `store: false` without chaining, so nothing is recorded;
    /// stored responses and explicit chains are.
    #[test]
    fn only_stored_or_chained_turns_are_recorded() {
        assert!(!request(json!({"model":"m", "store":false})).records_history());
        assert!(request(json!({"model":"m"})).records_history());
        assert!(request(json!({"model":"m", "store":true})).records_history());
        assert!(
            request(json!({"model":"m", "store":false, "previous_response_id":"resp_1"}))
                .records_history()
        );
    }

    fn crlf_fixture() -> &'static [u8] {
        include_bytes!("../../tests/fixtures/native_responses/crlf_completed_with_tool_delta.sse")
    }

    fn capture_completed(chunks: &[&[u8]]) -> Option<Value> {
        let mut capture = CompletedCapture::default();
        let mut completed = None;
        for chunk in chunks {
            if let Some(response) = capture.push(chunk) {
                completed = Some(response);
            }
        }
        completed.or_else(|| capture.finish())
    }

    #[test]
    fn completed_capture_reassembles_crlf_events_split_inside_utf8() {
        let fixture = crlf_fixture();
        let completed_at = memmem::find(fixture, b"\"type\":\"response.completed\"").unwrap();
        let sunny = memmem::find(&fixture[completed_at..], "晴".as_bytes()).unwrap();
        let split = completed_at + sunny + 1;
        let byte_chunks: Vec<&[u8]> = fixture.chunks(1).collect();
        for chunks in [vec![&fixture[..split], &fixture[split..]], byte_chunks] {
            let response = capture_completed(&chunks).unwrap();
            assert_eq!(response["id"], "resp_crlf");
            assert_eq!(response["output"][0]["arguments"], "{\"city\":\"杭州\"}");
            assert_eq!(response["output"][1]["content"][0]["text"], "杭州今天晴 🌤");
        }
    }

    #[test]
    fn completed_capture_accepts_a_final_event_without_its_blank_line() {
        let fixture: &[u8] =
            include_bytes!("../../tests/fixtures/native_responses/unterminated_completed.sse");
        let mut capture = CompletedCapture::default();
        assert!(capture.push(fixture).is_none());
        assert_eq!(capture.finish().unwrap()["id"], "resp_tail");

        let without_newline = fixture.strip_suffix(b"\n").unwrap();
        assert_eq!(
            capture_completed(&[without_newline]).unwrap()["id"],
            "resp_tail"
        );
    }

    #[test]
    fn completed_capture_parses_only_completed_events() {
        let stream = b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"response.completed\"}\n\n\
            data: not json\n\n\
            event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_ok\"}}\n\n";
        assert_eq!(capture_completed(&[stream]).unwrap()["id"], "resp_ok");
        assert!(completed_response(b"data: {\"type\":\"response.created\"}\n").is_none());
    }
}
