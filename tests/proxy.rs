use std::{convert::Infallible, sync::Arc};

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, Response, StatusCode, header},
    routing::post,
};
use modelmux::{
    config::{Cpa, Credentials, Settings},
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

async fn spawn_modelmux(
    cpa_base_url: String,
    official_models: &[&str],
    cpa_models: &[&str],
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let root = tempfile::tempdir().unwrap().keep();
    let path = root.join("model-catalog.json");
    let store = modelmux::catalog::CatalogStore::load(path.clone()).unwrap();
    store
        .replace(
            &json!({"models": official_models.iter().map(|slug| json!({"slug":slug})).collect::<Vec<_>>()}),
            &json!({"models": cpa_models.iter().map(|slug| json!({"slug":slug})).collect::<Vec<_>>()}),
        )
        .unwrap();
    drop(store);
    let settings = Settings {
        cpa: Cpa {
            base_url: cpa_base_url,
        },
        ..Settings::default()
    };
    let state = AppState::new(
        settings,
        Credentials {
            proxy_token: "proxy-token".into(),
            cpa_token: "cpa-token".into(),
        },
        root.join("model-catalog.json"),
        root.join("cpa-profiles.toml"),
    )
    .unwrap();
    spawn(server::router(state)).await
}
#[derive(Clone, Default)]
struct RawCapture(Arc<Mutex<Vec<(HeaderMap, Bytes)>>>);

#[tokio::test]
async fn cpa_json_rewrites_only_the_model_and_preserves_response_bytes() {
    async fn upstream(
        State(capture): State<RawCapture>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response<Body> {
        capture.0.lock().await.push((headers, body));
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .body(Body::from(
                include_bytes!("fixtures/native_responses/passthrough_response.json").as_slice(),
            ))
            .unwrap()
    }
    let capture = RawCapture::default();
    let (cpa_address, cpa_handle) = spawn(
        Router::new()
            .route("/v1/responses", post(upstream))
            .with_state(capture.clone()),
    )
    .await;
    let (proxy_address, proxy_handle) =
        spawn_modelmux(format!("http://{cpa_address}/v1"), &[], &["external-model"]).await;
    let request = include_bytes!("fixtures/native_responses/passthrough_request.json");
    let response = reqwest::Client::new()
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(request.as_slice())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/json; charset=utf-8"
    );
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        include_bytes!("fixtures/native_responses/passthrough_response.json")
    );
    let captured = capture.0.lock().await;
    let forwarded: Value = serde_json::from_slice(&captured[0].1).unwrap();
    assert_eq!(forwarded["model"], "external-model");
    assert_eq!(forwarded["input"], "你好 CPA");
    assert_eq!(forwarded["metadata"], json!({"unknown_field": true}));
    assert_eq!(forwarded["stream"], false);
    drop(captured);
    proxy_handle.abort();
    cpa_handle.abort();
}

#[tokio::test]
async fn responses_accept_bodies_larger_than_axums_two_megabyte_default() {
    async fn upstream(State(capture): State<RawCapture>, body: Bytes) -> Response<Body> {
        capture.0.lock().await.push((HeaderMap::new(), body));
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                include_bytes!("fixtures/native_responses/passthrough_response.json").as_slice(),
            ))
            .unwrap()
    }

    let capture = RawCapture::default();
    let upstream = Router::new()
        .route("/v1/responses", post(upstream))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .with_state(capture.clone());
    let (cpa_address, cpa_handle) = spawn(upstream).await;
    let (proxy_address, proxy_handle) =
        spawn_modelmux(format!("http://{cpa_address}/v1"), &[], &["large-model"]).await;
    let request = serde_json::to_vec(&json!({
        "model": "cpa/large-model",
        "input": "x".repeat(2 * 1024 * 1024 + 1024),
        "stream": false
    }))
    .unwrap();
    assert!(request.len() > 2 * 1024 * 1024);

    let response = reqwest::Client::new()
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(request.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let captured = capture.0.lock().await;
    let forwarded: Value = serde_json::from_slice(&captured[0].1).unwrap();
    assert_eq!(forwarded["model"], "large-model");
    assert_eq!(
        forwarded["input"].as_str().unwrap().len(),
        2 * 1024 * 1024 + 1024
    );
    drop(captured);
    proxy_handle.abort();
    cpa_handle.abort();
}

#[tokio::test]
async fn cpa_route_is_responses_passthrough_with_isolated_credentials() {
    async fn upstream(
        State(capture): State<Capture>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        capture.0.lock().await.push((headers, body.clone()));
        Json(json!({
            "id": "resp_cpa_1", "object": "response", "status": "completed",
            "model": body["model"],
            "output": [{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hello"}]}]
        }))
    }
    let capture = Capture::default();
    let (cpa_address, cpa_handle) = spawn(
        Router::new()
            .route("/v1/responses", post(upstream))
            .with_state(capture.clone()),
    )
    .await;
    let (proxy_address, proxy_handle) =
        spawn_modelmux(format!("http://{cpa_address}/v1"), &[], &["external-model"]).await;
    let request = json!({
        "model": "cpa/external-model", "input": "hi", "stream": false,
        "metadata": {"kept": true}
    });
    let response = reqwest::Client::new()
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .header(header::AUTHORIZATION, "Bearer official-oauth")
        .header("chatgpt-account-id", "official-account")
        .header("x-api-key", "incoming-provider-secret")
        .header("api-key", "another-provider-secret")
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response: Value = response.json().await.unwrap();
    assert_eq!(response["id"], "resp_cpa_1");

    let captured = capture.0.lock().await;
    assert_eq!(captured[0].0[header::AUTHORIZATION], "Bearer cpa-token");
    assert!(!captured[0].0.contains_key("chatgpt-account-id"));
    assert!(!captured[0].0.contains_key("x-api-key"));
    assert!(!captured[0].0.contains_key("api-key"));
    let mut expected = request;
    expected["model"] = json!("external-model");
    assert_eq!(captured[0].1, expected);
    drop(captured);
    proxy_handle.abort();
    cpa_handle.abort();
}

#[tokio::test]
async fn same_cpa_model_replays_public_history_on_every_followup() {
    async fn upstream(
        State(capture): State<Capture>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        let mut captured = capture.0.lock().await;
        let index = captured.len() + 1;
        captured.push((headers, body.clone()));
        Json(json!({
            "id": format!("resp_cpa_{index}"), "object": "response", "status": "completed",
            "model": body["model"],
            "output": [{"id": format!("msg_{index}"), "type":"message", "status":"completed", "role":"assistant",
                "content":[{"type":"output_text","text":format!("answer {index}"), "provider_state":"private"}]},
                {"type":"reasoning","encrypted_content":"never-forward"}]
        }))
    }
    let capture = Capture::default();
    let (cpa_address, cpa_handle) = spawn(
        Router::new()
            .route("/v1/responses", post(upstream))
            .with_state(capture.clone()),
    )
    .await;
    let (proxy_address, proxy_handle) =
        spawn_modelmux(format!("http://{cpa_address}/v1"), &[], &["external-model"]).await;
    let client = reqwest::Client::new();
    let first = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({"model":"cpa/external-model","input":"first","stream":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let second = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({
            "model":"cpa/external-model", "previous_response_id":"resp_cpa_1",
            "input":"second", "stream":false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);

    let captured = capture.0.lock().await;
    let replay = &captured[1].1;
    assert!(replay.get("previous_response_id").is_none());
    let input = replay["input"].as_array().unwrap();
    assert_eq!(input.len(), 3);
    assert_eq!(input[0]["content"][0]["text"], "first");
    assert_eq!(input[1]["content"][0]["text"], "answer 1");
    assert_eq!(input[2]["content"][0]["text"], "second");
    let replay_text = replay.to_string();
    assert!(!replay_text.contains("never-forward"));
    assert!(!replay_text.contains("provider_state"));
    assert!(!replay_text.contains("msg_1"));
    drop(captured);
    proxy_handle.abort();
    cpa_handle.abort();
}

#[tokio::test]
async fn switching_between_cpa_models_replays_history_and_changes_model() {
    async fn upstream(
        State(capture): State<Capture>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        let mut captured = capture.0.lock().await;
        let index = captured.len() + 1;
        captured.push((headers, body.clone()));
        Json(json!({
            "id": format!("resp_switch_{index}"), "object":"response", "status":"completed",
            "model":body["model"], "output":[{"type":"message","role":"assistant",
            "content":[{"type":"output_text","text":format!("answer {index}")}]}]
        }))
    }
    let capture = Capture::default();
    let (cpa_address, cpa_handle) = spawn(
        Router::new()
            .route("/v1/responses", post(upstream))
            .with_state(capture.clone()),
    )
    .await;
    let (proxy_address, proxy_handle) = spawn_modelmux(
        format!("http://{cpa_address}/v1"),
        &[],
        &["model-a", "model-b"],
    )
    .await;
    let client = reqwest::Client::new();
    client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({"model":"cpa/model-a","input":"first"}))
        .send()
        .await
        .unwrap();
    let response = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({
            "model":"cpa/model-b", "previous_response_id":"resp_switch_1", "input":"second"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let captured = capture.0.lock().await;
    assert_eq!(captured[1].1["model"], "model-b");
    assert!(captured[1].1.get("previous_response_id").is_none());
    assert_eq!(captured[1].1["input"].as_array().unwrap().len(), 3);
    drop(captured);
    proxy_handle.abort();
    cpa_handle.abort();
}

#[tokio::test]
async fn cpa_sse_is_byte_for_byte_passthrough_and_records_completed_history() {
    async fn upstream(
        State(capture): State<Capture>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Response<Body> {
        capture.0.lock().await.push((headers, body));
        let fixture = include_bytes!("fixtures/native_responses/completed_with_tool_delta.sse");
        let split = fixture
            .windows("杭".len())
            .position(|window| window == "杭".as_bytes())
            .unwrap()
            + 1;
        let chunks = vec![
            Ok::<Bytes, Infallible>(Bytes::copy_from_slice(&fixture[..split])),
            Ok::<Bytes, Infallible>(Bytes::copy_from_slice(&fixture[split..])),
        ];
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "Text/Event-Stream; charset=utf-8")
            .body(Body::from_stream(futures_util::stream::iter(chunks)))
            .unwrap()
    }
    let capture = Capture::default();
    let (cpa_address, cpa_handle) = spawn(
        Router::new()
            .route("/v1/responses", post(upstream))
            .with_state(capture.clone()),
    )
    .await;
    let (proxy_address, proxy_handle) =
        spawn_modelmux(format!("http://{cpa_address}/v1"), &[], &["stream-model"]).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({"model":"cpa/stream-model","input":"first","stream":true}))
        .send()
        .await
        .unwrap();
    let received = response.bytes().await.unwrap();
    assert_eq!(
        received.as_ref(),
        include_bytes!("fixtures/native_responses/completed_with_tool_delta.sse")
    );

    let followup = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({
            "model":"cpa/stream-model", "previous_response_id":"resp_stream",
            "input":"second", "stream":true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(followup.status(), StatusCode::OK);
    let captured = capture.0.lock().await;
    assert!(captured[1].1.get("previous_response_id").is_none());
    assert_eq!(captured[1].1["input"].as_array().unwrap().len(), 3);
    drop(captured);
    proxy_handle.abort();
    cpa_handle.abort();
}

#[tokio::test]
async fn completed_sse_is_recorded_before_upstream_eof() {
    async fn upstream(
        State(capture): State<Capture>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Response<Body> {
        let mut captured = capture.0.lock().await;
        let first = captured.is_empty();
        captured.push((headers, body));
        drop(captured);
        if first {
            let completed = Bytes::from_static(
                b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_open\",\"status\":\"completed\",\"output\":[]}}\n\n",
            );
            let stream = async_stream::stream! {
                yield Ok::<Bytes, Infallible>(completed);
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            };
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .body(Body::from_stream(stream))
                .unwrap()
        } else {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .body(Body::from(
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_followup\",\"status\":\"completed\",\"output\":[]}}\n\n",
                ))
                .unwrap()
        }
    }
    let capture = Capture::default();
    let (cpa_address, cpa_handle) = spawn(
        Router::new()
            .route("/v1/responses", post(upstream))
            .with_state(capture.clone()),
    )
    .await;
    let (proxy_address, proxy_handle) =
        spawn_modelmux(format!("http://{cpa_address}/v1"), &[], &["stream-model"]).await;
    let client = reqwest::Client::new();
    let mut first = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({"model":"cpa/stream-model","input":"first","stream":true}))
        .send()
        .await
        .unwrap();
    assert!(first.chunk().await.unwrap().is_some());

    let followup = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({
            "model":"cpa/stream-model", "previous_response_id":"resp_open",
            "input":"second", "stream":true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(followup.status(), StatusCode::OK);
    let captured = capture.0.lock().await;
    assert!(captured[1].1.get("previous_response_id").is_none());
    drop(captured);
    proxy_handle.abort();
    cpa_handle.abort();
}

#[tokio::test]
async fn responses_compact_is_forwarded_to_cpa() {
    async fn compact(Json(body): Json<Value>) -> Json<Value> {
        Json(json!({"object":"response.compaction","model":body["model"],"output":[]}))
    }
    let (cpa_address, cpa_handle) =
        spawn(Router::new().route("/v1/responses/compact", post(compact))).await;
    let (proxy_address, proxy_handle) =
        spawn_modelmux(format!("http://{cpa_address}/v1"), &[], &["external-model"]).await;
    let response = reqwest::Client::new()
        .post(format!("http://{proxy_address}/v1/responses/compact"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({"model":"cpa/external-model","input":"compact this"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["object"],
        "response.compaction"
    );
    proxy_handle.abort();
    cpa_handle.abort();
}

#[tokio::test]
async fn cpa_error_status_content_type_and_body_are_passthrough() {
    async fn upstream() -> Response<Body> {
        Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header(header::CONTENT_TYPE, "application/problem+json")
            .body(Body::from("{ \"error\" : \"rate limited\" }\n"))
            .unwrap()
    }
    let (cpa_address, cpa_handle) =
        spawn(Router::new().route("/v1/responses", post(upstream))).await;
    let (proxy_address, proxy_handle) =
        spawn_modelmux(format!("http://{cpa_address}/v1"), &[], &["external-model"]).await;
    let response = reqwest::Client::new()
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({"model":"cpa/external-model","input":"hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );
    assert_eq!(
        response.text().await.unwrap(),
        "{ \"error\" : \"rate limited\" }\n"
    );
    proxy_handle.abort();
    cpa_handle.abort();
}

#[tokio::test]
async fn unknown_models_and_history_fail_closed() {
    let (proxy_address, proxy_handle) = spawn_modelmux(
        "http://127.0.0.1:9/v1".into(),
        &["gpt-official"],
        &["external-model"],
    )
    .await;
    let client = reqwest::Client::new();
    let unknown_model = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({"model":"gpt-typo","input":"hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown_model.status(), StatusCode::BAD_REQUEST);
    let unknown_history = client
        .post(format!("http://{proxy_address}/v1/responses"))
        .header("x-modelmux-token", "proxy-token")
        .json(&json!({
            "model":"cpa/external-model", "previous_response_id":"resp_unknown", "input":"hello"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown_history.status(), StatusCode::CONFLICT);
    proxy_handle.abort();
}

#[tokio::test]
async fn ambiguous_routing_fields_fail_before_forwarding() {
    let (proxy_address, proxy_handle) =
        spawn_modelmux("http://127.0.0.1:9/v1".into(), &[], &["external-model"]).await;
    let client = reqwest::Client::new();
    for body in [
        r#"{"model":"cpa/external-model","model":"other","input":"hello"}"#,
        r#"{"model":"cpa/external-model","previous_response_id":42,"input":"hello"}"#,
        r#"{"model":"cpa/external-model","previous_response_id":"","input":"hello"}"#,
    ] {
        let response = client
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-modelmux-token", "proxy-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
    proxy_handle.abort();
}

#[tokio::test]
async fn every_route_requires_the_proxy_token() {
    let (proxy_address, proxy_handle) =
        spawn_modelmux("http://127.0.0.1:9/v1".into(), &["gpt"], &[]).await;
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
