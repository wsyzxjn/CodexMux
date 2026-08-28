use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    routing::post,
};
use modelmux::{
    config::{Credentials, Dialect, Model, Provider, ProviderKind, Settings},
    server::{self, AppState},
};
use serde_json::{Value, json};
use tokio::sync::Mutex;

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<(HeaderMap, Value)>>>);

async fn spawn(app: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (address, handle)
}

fn model(slug: &str) -> Model {
    Model {
        slug: slug.into(),
        display_name: slug.into(),
        description: None,
        context_window: 128_000,
        accepts_images: false,
    }
}

fn external(id: &str, dialect: Dialect, base_url: String, model_slug: &str) -> Provider {
    Provider {
        id: id.into(),
        name: id.into(),
        kind: ProviderKind::External,
        dialect,
        base_url,
        credential_header: None,
        headers: BTreeMap::new(),
        models: vec![model(model_slug)],
        enabled: true,
        allow_cross_model_previous_response_id: false,
    }
}

async fn spawn_modelmux(
    providers: Vec<Provider>,
    provider_credentials: HashMap<String, String>,
    official_models: &[&str],
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let settings = Settings {
        providers,
        ..Settings::default()
    };
    let state = AppState::new(
        settings,
        Credentials {
            schema_version: 1,
            proxy_token: "proxy-token".into(),
            providers: provider_credentials,
        },
        official_models
            .iter()
            .map(|model| (*model).to_owned())
            .collect(),
    )
    .unwrap();
    spawn(server::router(state)).await
}

#[tokio::test]
async fn external_chat_route_strips_oauth_and_returns_responses_json() {
    async fn upstream(
        State(capture): State<Capture>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        capture.0.lock().await.push((headers, body));
        Json(json!({
            "id": "chat_1", "model": "chat-model", "created": 1,
            "choices": [{"message": {"role": "assistant", "content": "hello"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2}
        }))
    }
    let capture = Capture::default();
    let (upstream_address, upstream_handle) = spawn(
        Router::new()
            .route("/v1/chat/completions", post(upstream))
            .with_state(capture.clone()),
    )
    .await;
    let provider = external(
        "chat",
        Dialect::OpenaiChat,
        format!("http://{upstream_address}/v1"),
        "chat-model",
    );
    let (proxy_address, proxy_handle) = spawn_modelmux(
        vec![Provider::official(), provider],
        HashMap::from([("chat".into(), "external-key".into())]),
        &[],
    )
    .await;

    let response = reqwest::Client::new()
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .header(header::AUTHORIZATION, "Bearer oauth-secret")
        .header("x-api-key", "incoming-external-secret")
        .header("api-key", "another-incoming-secret")
        .json(&json!({"model": "chat-model", "input": "hi", "stream": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response: Value = response.json().await.unwrap();
    assert_eq!(response["id"], "resp_chat_1");
    assert_eq!(response["output"][0]["content"][0]["text"], "hello");

    let captured = capture.0.lock().await;
    assert_eq!(captured[0].0[header::AUTHORIZATION], "Bearer external-key");
    assert!(!captured[0].0.contains_key("x-api-key"));
    assert!(!captured[0].0.contains_key("api-key"));
    assert_eq!(captured[0].1["messages"][0]["content"], "hi");
    drop(captured);
    proxy_handle.abort();
    upstream_handle.abort();
}

#[tokio::test]
async fn native_responses_route_is_byte_semantic_passthrough() {
    async fn upstream(
        State(capture): State<Capture>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        capture.0.lock().await.push((headers, body.clone()));
        Json(json!({
            "id": "resp_official", "object": "response", "status": "completed",
            "model": body["model"], "output": []
        }))
    }
    let capture = Capture::default();
    let (upstream_address, upstream_handle) = spawn(
        Router::new()
            .route("/v1/responses", post(upstream))
            .with_state(capture.clone()),
    )
    .await;
    let provider = external(
        "responses",
        Dialect::Responses,
        format!("http://{upstream_address}/v1"),
        "native-model",
    );
    let (proxy_address, proxy_handle) = spawn_modelmux(
        vec![Provider::official(), provider],
        HashMap::from([("responses".into(), "external-key".into())]),
        &[],
    )
    .await;
    let request = json!({
        "model": "native-model", "input": "hello", "stream": false,
        "metadata": {"kept": true}
    });
    let response = reqwest::Client::new()
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .header(header::AUTHORIZATION, "Bearer oauth-secret")
        .header("x-api-key", "external-secret")
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let captured = capture.0.lock().await;
    assert_eq!(captured[0].0[header::AUTHORIZATION], "Bearer external-key");
    assert!(!captured[0].0.contains_key("x-api-key"));
    assert_eq!(captured[0].1, request);
    drop(captured);
    proxy_handle.abort();
    upstream_handle.abort();
}

#[tokio::test]
async fn switching_provider_replays_public_history_without_previous_id() {
    async fn responses_upstream(Json(body): Json<Value>) -> Json<Value> {
        Json(json!({
            "id": "resp_first", "object": "response", "status": "completed",
            "model": body["model"],
            "output": [{"id": "msg_private", "type": "message", "status": "completed", "role": "assistant",
                "content": [{"type": "output_text", "text": "first answer"}]},
                {"type": "compaction", "encrypted_content": "provider-private"}]
        }))
    }
    async fn chat_upstream(
        State(capture): State<Capture>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        capture.0.lock().await.push((headers, body));
        Json(json!({
            "id": "chat_second", "model": "chat-model", "created": 2,
            "choices": [{"message": {"role": "assistant", "content": "second answer"}, "finish_reason": "stop"}]
        }))
    }
    let (responses_address, responses_handle) =
        spawn(Router::new().route("/v1/responses", post(responses_upstream))).await;
    let capture = Capture::default();
    let (chat_address, chat_handle) = spawn(
        Router::new()
            .route("/v1/chat/completions", post(chat_upstream))
            .with_state(capture.clone()),
    )
    .await;
    let first = external(
        "responses-a",
        Dialect::Responses,
        format!("http://{responses_address}/v1"),
        "responses-model",
    );
    let second = external(
        "chat-b",
        Dialect::OpenaiChat,
        format!("http://{chat_address}/v1"),
        "chat-model",
    );
    let (proxy_address, proxy_handle) = spawn_modelmux(
        vec![Provider::official(), first, second],
        HashMap::from([
            ("responses-a".into(), "key-a".into()),
            ("chat-b".into(), "key-b".into()),
        ]),
        &[],
    )
    .await;
    let client = reqwest::Client::new();
    let first_response = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({"model": "responses-model", "input": "first question", "stream": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(first_response.status(), StatusCode::OK);

    let second_response = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({
            "model": "chat-model", "previous_response_id": "resp_first",
            "input": "second question", "stream": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(second_response.status(), StatusCode::OK);
    let captured = capture.0.lock().await;
    let messages = captured[0].1["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["content"][0]["text"], "first question");
    assert_eq!(messages[1]["content"][0]["text"], "first answer");
    assert_eq!(messages[2]["content"][0]["text"], "second question");
    assert!(!captured[0].1.to_string().contains("provider-private"));
    drop(captured);
    proxy_handle.abort();
    responses_handle.abort();
    chat_handle.abort();
}

#[tokio::test]
async fn rejects_missing_proxy_token_before_routing() {
    let (proxy_address, proxy_handle) =
        spawn_modelmux(vec![Provider::official()], HashMap::new(), &["gpt"]).await;
    let response = reqwest::Client::new()
        .post(format!("http://{proxy_address}/v1/responses"))
        .json(&json!({"model": "gpt", "input": "hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    proxy_handle.abort();
}

#[tokio::test]
async fn translated_provider_replays_history_on_its_own_second_turn() {
    async fn upstream(
        State(capture): State<Capture>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        let mut captured = capture.0.lock().await;
        let index = captured.len() + 1;
        captured.push((headers, body));
        Json(json!({
            "id": format!("chat_{index}"), "model": "chat-model", "created": index,
            "choices": [{"message": {"role": "assistant", "content": format!("answer {index}")}, "finish_reason": "stop"}]
        }))
    }
    let capture = Capture::default();
    let (upstream_address, upstream_handle) = spawn(
        Router::new()
            .route("/v1/chat/completions", post(upstream))
            .with_state(capture.clone()),
    )
    .await;
    let provider = external(
        "chat",
        Dialect::OpenaiChat,
        format!("http://{upstream_address}/v1"),
        "chat-model",
    );
    let (proxy_address, proxy_handle) = spawn_modelmux(
        vec![Provider::official(), provider],
        HashMap::from([("chat".into(), "external-key".into())]),
        &[],
    )
    .await;
    let client = reqwest::Client::new();
    let first = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({"model": "chat-model", "input": "first", "stream": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let second = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({
            "model": "chat-model", "previous_response_id": "resp_chat_1",
            "input": "second", "stream": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let captured = capture.0.lock().await;
    let messages = captured[1].1["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["content"][0]["text"], "first");
    assert_eq!(messages[1]["content"][0]["text"], "answer 1");
    assert_eq!(messages[2]["content"][0]["text"], "second");
    drop(captured);
    proxy_handle.abort();
    upstream_handle.abort();
}

#[tokio::test]
async fn unknown_models_and_history_fail_closed() {
    let provider = external(
        "chat",
        Dialect::OpenaiChat,
        "http://127.0.0.1:9/v1".into(),
        "chat-model",
    );
    let (proxy_address, proxy_handle) = spawn_modelmux(
        vec![Provider::official(), provider],
        HashMap::from([("chat".into(), "external-key".into())]),
        &["gpt-official"],
    )
    .await;
    let client = reqwest::Client::new();
    let unknown_model = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({"model": "gpt-typo", "input": "hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown_model.status(), StatusCode::BAD_REQUEST);
    let unknown_history = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({
            "model": "chat-model", "previous_response_id": "resp_unknown", "input": "hello"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown_history.status(), StatusCode::CONFLICT);
    proxy_handle.abort();
}

#[tokio::test]
async fn every_route_requires_the_proxy_token() {
    let (proxy_address, proxy_handle) =
        spawn_modelmux(vec![Provider::official()], HashMap::new(), &["gpt"]).await;
    let client = reqwest::Client::new();
    let missing = client
        .get(format!("http://{proxy_address}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::FORBIDDEN);
    let missing_unknown = client
        .get(format!("http://{proxy_address}/unknown"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_unknown.status(), StatusCode::FORBIDDEN);
    let accepted_unknown = client
        .get(format!("http://{proxy_address}/unknown"))
        .header("x-modelmux-token", "proxy-token")
        .send()
        .await
        .unwrap();
    assert_eq!(accepted_unknown.status(), StatusCode::NOT_FOUND);
    let accepted = client
        .get(format!("http://{proxy_address}/health"))
        .header("x-modelmux-token", "proxy-token")
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    proxy_handle.abort();
}
