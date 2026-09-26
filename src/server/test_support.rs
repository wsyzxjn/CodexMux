//! Fixtures shared by the server unit tests.

use std::sync::Arc;

use axum::{
    Router,
    body::Bytes,
    http::{HeaderMap, header},
    response::Json,
};
use serde_json::{Value, json};

use super::{AppState, responses::ResponseRequest};
use crate::{
    catalog::{self, CatalogStore},
    config::{Credentials, Paths, Settings},
};

pub(super) async fn spawn_test_app(
    app: Router,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (address, handle)
}

/// Serve the CodexMux router for `state` on an ephemeral loopback port.
pub(super) async fn spawn_proxy(
    state: AppState,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    spawn_test_app(super::router(state)).await
}

#[derive(Clone, Default)]
pub(super) struct TestCapture(pub(super) Arc<tokio::sync::Mutex<Vec<(HeaderMap, Value)>>>);

pub(super) fn auto_review_state(
    root: &std::path::Path,
    official_address: std::net::SocketAddr,
    cpa_address: std::net::SocketAddr,
) -> AppState {
    test_state(
        root,
        official_address,
        cpa_address,
        json!({"models":[
            {"slug":catalog::AUTO_REVIEW_MODEL, "visibility":"hide"},
            {"slug":"gpt-5.6"}
        ]}),
        json!({"models":[
            {"slug":"codex-auto-review", "visibility":"hide"},
            {"slug":"glm-5.3-flash", "visibility":"list"}
        ]}),
        |_| {},
    )
}

/// A state with a stored catalog whose official and CPA routes point at
/// local test servers.
pub(super) fn test_state(
    root: &std::path::Path,
    official_address: std::net::SocketAddr,
    cpa_address: std::net::SocketAddr,
    official_catalog: Value,
    cpa_catalog: Value,
    configure: impl FnOnce(&mut Settings),
) -> AppState {
    let store = CatalogStore::load(root.join("model-catalog.json")).unwrap();
    store.replace(&official_catalog, &cpa_catalog).unwrap();
    drop(store);
    let mut settings = Settings {
        cpa: crate::config::Cpa {
            base_url: format!("http://{cpa_address}/v1"),
        },
        ..Settings::default()
    };
    configure(&mut settings);
    let mut state = AppState::new(
        settings,
        Credentials {
            proxy_token: "proxy".into(),
            cpa_token: "cpa-secret".into(),
            cpa_management_key: "management-secret".into(),
        },
        &Paths::from_root(root.to_path_buf()),
    )
    .unwrap();
    state.official_base_url = format!("http://{official_address}/v1");
    state
}

pub(super) fn unused_address() -> std::net::SocketAddr {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap()
}

pub(super) fn post_json(
    address: std::net::SocketAddr,
    path: &str,
    body: Value,
) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .post(format!("http://{address}{path}"))
        .header("x-codexmux-token", "proxy")
        .header(header::AUTHORIZATION, "Bearer oauth")
        .json(&body)
}

pub(super) fn completed_json(id: &str) -> Json<Value> {
    Json(json!({"id": id, "object": "response", "status": "completed", "output": []}))
}

pub(super) fn request(value: Value) -> ResponseRequest {
    ResponseRequest::parse(Bytes::from(serde_json::to_vec(&value).unwrap())).unwrap()
}

pub(super) fn forwarded(request: ResponseRequest, upstream_model: Option<&str>) -> Value {
    serde_json::from_slice(&request.into_body(upstream_model).unwrap()).unwrap()
}
