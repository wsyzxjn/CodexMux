use std::{collections::HashSet, io, sync::Arc, time::Duration};

use async_stream::stream;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{OriginalUri, Request, State},
    http::{HeaderMap, HeaderValue, Response, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Json},
    routing::{get, post},
};
use futures_util::StreamExt;
use serde_json::{Value, json};

use crate::{
    config::{Credentials, Dialect, Provider, Settings},
    continuity::{ContinuityStore, portable_input_items, portable_output_items},
    dialect::{
        anthropic_messages::{self, MessagesTranslator},
        openai_chat::{self, ChatTranslator},
        sse,
    },
    router,
};

const MAX_CAPTURE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    client: reqwest::Client,
    settings: Arc<Settings>,
    official_models: Arc<HashSet<String>>,
    credentials: Arc<Credentials>,
    continuity: Arc<ContinuityStore>,
}

impl AppState {
    pub fn new(
        settings: Settings,
        credentials: Credentials,
        official_models: HashSet<String>,
    ) -> anyhow::Result<Self> {
        settings.validate()?;
        let mut external_models = HashSet::new();
        for provider in settings.providers.iter().filter(|provider| {
            provider.enabled && provider.kind == crate::config::ProviderKind::External
        }) {
            for model in &provider.models {
                anyhow::ensure!(
                    !official_models.contains(&model.slug),
                    "external model {} collides with an official model",
                    model.slug
                );
                anyhow::ensure!(
                    external_models.insert(model.slug.clone()),
                    "model {} is routed by more than one enabled provider",
                    model.slug
                );
            }
        }
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(180))
                .build()?,
            settings: Arc::new(settings),
            official_models: Arc::new(official_models),
            credentials: Arc::new(credentials),
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
        .route("/v1/responses", post(handle_responses))
        .route("/responses", post(handle_responses))
        .route("/v1/responses/compact", post(handle_responses))
        .route("/responses/compact", post(handle_responses))
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

async fn handle_responses(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response<Body>, ProxyError> {
    let route = if uri.path().ends_with("/responses/compact") {
        "responses/compact"
    } else {
        "responses"
    };
    let mut request: Value = serde_json::from_slice(&body)
        .map_err(|error| ProxyError::bad_request("invalid_json", error.to_string()))?;
    let model = request
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ProxyError::bad_request("missing_model", "request has no model"))?;
    let provider = router::resolve(&state.settings, &state.official_models, &model)
        .map_err(|error| ProxyError::bad_request("route", error.to_string()))?
        .clone();
    if route == "responses/compact" && provider.dialect != Dialect::Responses {
        return Err(ProxyError::new(
            StatusCode::NOT_IMPLEMENTED,
            "compact_unavailable",
            "responses/compact translation is not implemented for this provider dialect",
        ));
    }

    let parent = request
        .get("previous_response_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let turn_input = portable_input_items(request.get("input"));
    let route_id = route_id(&provider, &model);
    let mut rewritten = false;
    if let Some(parent_id) = parent.as_deref() {
        let previous_route = state.continuity.route_of(parent_id).ok_or_else(|| {
            ProxyError::conflict(
                "unknown_history",
                "cannot continue because the previous response id is not in local history",
            )
        })?;
        if !state.continuity.is_complete(parent_id) {
            return Err(ProxyError::conflict(
                "incomplete_history",
                "cannot continue because the previous response chain is incomplete",
            ));
        }
        let needs_replay = previous_route != route_id || provider.dialect != Dialect::Responses;
        if needs_replay {
            let history = state.continuity.materialize(parent_id).ok_or_else(|| {
                ProxyError::conflict(
                    "incomplete_history",
                    "cannot continue because the previous response chain is incomplete",
                )
            })?;
            let mut replay = history;
            replay.extend(turn_input.clone());
            let object = request.as_object_mut().ok_or_else(|| {
                ProxyError::bad_request("invalid_json", "request must be an object")
            })?;
            object.insert("input".into(), Value::Array(replay));
            object.remove("previous_response_id");
            rewritten = true;
        }
    }

    let outgoing = match provider.dialect {
        Dialect::Responses if !rewritten => body,
        Dialect::Responses => Bytes::from(
            serde_json::to_vec(&request)
                .map_err(|error| ProxyError::bad_request("json", error.to_string()))?,
        ),
        Dialect::OpenaiChat => Bytes::from(
            serde_json::to_vec(
                &openai_chat::request_from_responses(&request)
                    .map_err(|error| ProxyError::bad_request("translate", error.to_string()))?,
            )
            .map_err(|error| ProxyError::bad_request("json", error.to_string()))?,
        ),
        Dialect::AnthropicMessages => Bytes::from(
            serde_json::to_vec(
                &anthropic_messages::request_from_responses(&request)
                    .map_err(|error| ProxyError::bad_request("translate", error.to_string()))?,
            )
            .map_err(|error| ProxyError::bad_request("json", error.to_string()))?,
        ),
    };
    let upstream_headers =
        router::upstream_headers(&headers, &provider, &state.settings, &state.credentials)
            .map_err(|error| ProxyError::unauthorized("credential", error.to_string()))?;
    let target = target_url(&provider, route);
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
        .is_some_and(|value| value.contains("text/event-stream"));
    if !status.is_success() {
        let message = upstream
            .text()
            .await
            .unwrap_or_else(|_| "upstream request failed".into());
        return Err(ProxyError::new(status, "upstream_error", message));
    }

    if is_sse {
        Ok(streaming_response(
            upstream,
            provider.dialect,
            model,
            parent,
            turn_input,
            route_id,
            state.continuity,
        ))
    } else {
        non_streaming_response(
            upstream,
            provider.dialect,
            parent,
            turn_input,
            route_id,
            &state.continuity,
        )
        .await
    }
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

fn route_id(provider: &Provider, model: &str) -> String {
    if provider.allow_cross_model_previous_response_id {
        provider.id.clone()
    } else {
        format!("{}:{model}", provider.id)
    }
}

fn target_url(provider: &Provider, route: &str) -> String {
    let endpoint = match provider.dialect {
        Dialect::Responses => route,
        Dialect::OpenaiChat => "chat/completions",
        Dialect::AnthropicMessages => "messages",
    };
    format!(
        "{}/{}",
        provider.base_url.trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    )
}

async fn non_streaming_response(
    upstream: reqwest::Response,
    dialect: Dialect,
    parent: Option<String>,
    turn_input: Vec<Value>,
    route_id: String,
    continuity: &ContinuityStore,
) -> Result<Response<Body>, ProxyError> {
    let bytes = upstream
        .bytes()
        .await
        .map_err(|error| ProxyError::bad_gateway("upstream_body", error.to_string()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| ProxyError::bad_gateway("upstream_json", error.to_string()))?;
    let response = match dialect {
        Dialect::Responses => value,
        Dialect::OpenaiChat => openai_chat::response_from_chat(&value)
            .map_err(|error| ProxyError::bad_gateway("translate", error.to_string()))?,
        Dialect::AnthropicMessages => anthropic_messages::response_from_messages(&value)
            .map_err(|error| ProxyError::bad_gateway("translate", error.to_string()))?,
    };
    record_response(
        continuity,
        &response,
        parent.as_deref(),
        &route_id,
        turn_input,
    );
    let bytes = if dialect == Dialect::Responses {
        bytes
    } else {
        Bytes::from(serde_json::to_vec(&response).map_err(|error| {
            ProxyError::new(StatusCode::INTERNAL_SERVER_ERROR, "json", error.to_string())
        })?)
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(bytes))
        .map_err(|error| {
            ProxyError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "response",
                error.to_string(),
            )
        })
}

enum Translator {
    Native { buffer: Vec<u8> },
    Chat(ChatTranslator),
    Messages(MessagesTranslator),
}

impl Translator {
    fn new(dialect: Dialect, model: String) -> Self {
        match dialect {
            Dialect::Responses => Self::Native { buffer: Vec::new() },
            Dialect::OpenaiChat => Self::Chat(ChatTranslator::new(model)),
            Dialect::AnthropicMessages => Self::Messages(MessagesTranslator::new(model)),
        }
    }

    fn push(&mut self, bytes: &[u8]) -> (Vec<Bytes>, Option<Value>) {
        match self {
            Self::Native { buffer } => {
                let mut completed = None;
                for frame in sse::frames(buffer, bytes) {
                    if let Some(data) = sse::data(&frame)
                        && let Ok(event) = serde_json::from_str::<Value>(&data)
                        && event.get("type").and_then(Value::as_str) == Some("response.completed")
                    {
                        completed = event.get("response").cloned();
                    }
                }
                (vec![Bytes::copy_from_slice(bytes)], completed)
            }
            Self::Chat(translator) => encode_events(translator.push(bytes)),
            Self::Messages(translator) => encode_events(translator.push(bytes)),
        }
    }

    fn finish(&mut self) -> (Vec<Bytes>, Option<Value>) {
        match self {
            Self::Native { buffer } => {
                let data = sse::data(buffer);
                buffer.clear();
                let completed = data
                    .and_then(|data| serde_json::from_str::<Value>(&data).ok())
                    .filter(|event| {
                        event.get("type").and_then(Value::as_str) == Some("response.completed")
                    })
                    .and_then(|event| event.get("response").cloned());
                (Vec::new(), completed)
            }
            Self::Chat(translator) => encode_events(translator.finish()),
            Self::Messages(translator) => encode_events(translator.finish()),
        }
    }
}

fn encode_events(events: Vec<Value>) -> (Vec<Bytes>, Option<Value>) {
    let completed = events
        .iter()
        .find(|event| {
            matches!(
                event.get("type").and_then(Value::as_str),
                Some("response.completed" | "response.incomplete")
            )
        })
        .and_then(|event| event.get("response"))
        .cloned();
    (
        events
            .iter()
            .map(|event| Bytes::from(sse::encode(event)))
            .collect(),
        completed,
    )
}

fn streaming_response(
    upstream: reqwest::Response,
    dialect: Dialect,
    model: String,
    parent: Option<String>,
    turn_input: Vec<Value>,
    route_id: String,
    continuity: Arc<ContinuityStore>,
) -> Response<Body> {
    let source = upstream.bytes_stream();
    let output = stream! {
        let mut source = Box::pin(source);
        let mut translator = Translator::new(dialect, model);
        let mut completed = None;
        let mut capture_bytes = 0usize;
        while let Some(chunk) = source.next().await {
            match chunk {
                Ok(chunk) => {
                    capture_bytes = capture_bytes.saturating_add(chunk.len());
                    if capture_bytes > MAX_CAPTURE_BYTES {
                        completed = None;
                    }
                    let (encoded, response) = translator.push(&chunk);
                    if capture_bytes <= MAX_CAPTURE_BYTES && response.is_some() {
                        completed = response;
                    }
                    for bytes in encoded {
                        yield Ok::<Bytes, io::Error>(bytes);
                    }
                }
                Err(error) => {
                    yield Err(io::Error::other(error));
                    return;
                }
            }
        }
        let (encoded, response) = translator.finish();
        if capture_bytes <= MAX_CAPTURE_BYTES && response.is_some() {
            completed = response;
        }
        for bytes in encoded {
            yield Ok::<Bytes, io::Error>(bytes);
        }
        if let Some(response) = completed {
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
    use crate::config::Provider;

    #[test]
    fn rejects_runtime_official_external_model_collision() {
        let mut external = Provider::official();
        external.id = "external".into();
        external.kind = crate::config::ProviderKind::External;
        external.base_url = "https://example.com/v1".into();
        external.models = vec![crate::config::Model {
            slug: "same".into(),
            display_name: String::new(),
            description: None,
            context_window: 128_000,
            accepts_images: false,
        }];
        let settings = Settings {
            providers: vec![Provider::official(), external],
            ..Settings::default()
        };
        let credentials = Credentials {
            schema_version: 1,
            proxy_token: "proxy".into(),
            providers: std::collections::HashMap::from([("external".into(), "key".into())]),
        };
        assert!(AppState::new(settings, credentials, HashSet::from(["same".into()])).is_err());
    }

    #[test]
    fn route_identity_can_force_replay_on_model_change() {
        let mut provider = Provider::official();
        provider.allow_cross_model_previous_response_id = false;
        assert_ne!(route_id(&provider, "one"), route_id(&provider, "two"));
        provider.allow_cross_model_previous_response_id = true;
        assert_eq!(route_id(&provider, "one"), route_id(&provider, "two"));
    }
}
