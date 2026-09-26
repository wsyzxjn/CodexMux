//! Codex's built-in image tool endpoints.

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, Response},
};
use serde_json::Value;
use tokio::time::Instant;

use super::{
    AppState, ProxyError, RequestBody, Route,
    upstream::{passthrough_response, post_upstream},
};
use crate::router;

/// Codex's built-in `image_gen` tool posts to `{base_url}/images/generations`
/// and `{base_url}/images/edits`. Those requests carry a `gpt-image-*` model
/// that no catalog lists, and they are unrelated to the conversation model, so
/// there is nothing to route by slug. They use the fixed official endpoint
/// unless the user explicitly pins an image model, mirroring how
/// `codex-auto-review` may be pinned. Upstream failure never changes the route.
///
/// The `model` field is not a selector on the official endpoint: it requires
/// only `prompt` and generates even when `model` is missing or unknown, and it
/// returns no model of its own. CPA, by contrast, dispatches on `model` and
/// rejects one it cannot serve, which is why the slug is rewritten only for a
/// pinned request.
pub(super) async fn handle_image_generations(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: RequestBody,
) -> Result<Response<Body>, ProxyError> {
    forward_image(&state, &headers, "images/generations", body.0).await
}

pub(super) async fn handle_image_edits(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: RequestBody,
) -> Result<Response<Body>, ProxyError> {
    forward_image(&state, &headers, "images/edits", body.0).await
}

/// Byte-preserving passthrough to the selected image endpoint. Only the model
/// field changes, and only when an override redirects the request off the
/// official route.
async fn forward_image(
    state: &AppState,
    headers: &HeaderMap,
    endpoint: &'static str,
    body: Bytes,
) -> Result<Response<Body>, ProxyError> {
    let started = Instant::now();
    let (route, body) = match crate::cpa::image_override(&state.cpa_profiles_path)
        .map_err(ProxyError::local_config)?
    {
        None => (Route::Official, body),
        Some(slug) => (Route::Cpa, rewrite_image_model(body, &slug)),
    };
    let upstream_headers = match route {
        Route::Official => router::official_image_headers(headers),
        Route::Cpa => router::cpa_image_headers(headers, &state.credentials.cpa_token),
    }
    .map_err(ProxyError::credential)?;
    let request_bytes = body.len();
    let upstream = post_upstream(state, route, endpoint, upstream_headers, body).await?;
    tracing::info!(
        endpoint,
        route = route.name(),
        status = upstream.status().as_u16(),
        request_bytes,
        latency_ms = started.elapsed().as_millis() as u64,
        "image request forwarded"
    );
    Ok(passthrough_response(upstream))
}

/// Point a redirected image request at the pinned model. A JSON body has its
/// `model` replaced; any other encoding is forwarded untouched, because the
/// override only chooses a destination and CodexMux does not rewrite formats
/// it did not parse.
fn rewrite_image_model(body: Bytes, upstream_model: &str) -> Bytes {
    let Ok(Value::Object(mut object)) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    object.insert("model".into(), Value::String(upstream_model.to_owned()));
    match serde_json::to_vec(&object) {
        Ok(bytes) => Bytes::from(bytes),
        Err(_) => body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::server::test_support::*;
    use axum::{
        Router,
        http::{StatusCode, header},
        routing::post,
    };
    use serde_json::json;

    /// Codex's built-in `image_gen` tool posts to `{base_url}/images/*`.
    /// Those paths had no route at all, so the local listener answered every
    /// image request with 404. They must reach the fixed official endpoint,
    /// carry official OAuth only, and forward the body byte for byte.
    #[tokio::test]
    async fn image_endpoints_reach_the_official_route_with_isolated_credentials() {
        type CapturedImage = (&'static str, HeaderMap, Bytes);

        #[derive(Clone, Default)]
        struct RawCapture(Arc<tokio::sync::Mutex<Vec<CapturedImage>>>);

        async fn generations(
            State(capture): State<RawCapture>,
            headers: HeaderMap,
            body: Bytes,
        ) -> Response<Body> {
            capture.0.lock().await.push(("generations", headers, body));
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"data":[{"b64_json":"AA=="}]}"#))
                .unwrap()
        }

        async fn edits(
            State(capture): State<RawCapture>,
            headers: HeaderMap,
            body: Bytes,
        ) -> Response<Body> {
            capture.0.lock().await.push(("edits", headers, body));
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"data":[]}"#))
                .unwrap()
        }

        let capture = RawCapture::default();
        let (official_address, official_handle) = spawn_test_app(
            Router::new()
                .route("/v1/images/generations", post(generations))
                .route("/v1/images/edits", post(edits))
                .with_state(capture.clone()),
        )
        .await;
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), official_address, official_address);
        let (proxy_address, proxy_handle) = spawn_proxy(state).await;
        let client = reqwest::Client::new();

        let body = r#"{"model":"gpt-image-1","prompt":"a cat","n":1}"#;
        let generated = client
            .post(format!("http://{proxy_address}/v1/images/generations"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .header("chatgpt-account-id", "account")
            .header("x-codex-imagegen-request-id", "req-1")
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(generated.status(), StatusCode::OK);
        assert_eq!(
            generated.json::<Value>().await.unwrap(),
            json!({"data":[{"b64_json":"AA=="}]})
        );

        // An edit may arrive as multipart; the content type must survive.
        let multipart = "--boundary\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nmake it blue\r\n--boundary--\r\n";
        let edited = client
            .post(format!("http://{proxy_address}/v1/images/edits"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .header(
                header::CONTENT_TYPE,
                "multipart/form-data; boundary=boundary",
            )
            .body(multipart)
            .send()
            .await
            .unwrap();
        assert_eq!(edited.status(), StatusCode::OK);

        let captured = capture.0.lock().await;
        assert_eq!(captured.len(), 2);

        let (endpoint, headers, forwarded) = &captured[0];
        assert_eq!(*endpoint, "generations");
        assert_eq!(headers[header::AUTHORIZATION], "Bearer oauth");
        assert_eq!(headers["chatgpt-account-id"], "account");
        assert_eq!(headers["x-codex-imagegen-request-id"], "req-1");
        assert_eq!(headers[header::CONTENT_TYPE], "application/json");
        // Official OAuth only: the CPA token never reaches an image request.
        assert_ne!(headers[header::AUTHORIZATION], "Bearer cpa-secret");
        assert_eq!(forwarded.as_ref(), body.as_bytes());

        let (endpoint, headers, forwarded) = &captured[1];
        assert_eq!(*endpoint, "edits");
        assert_eq!(headers[header::AUTHORIZATION], "Bearer oauth");
        assert_eq!(
            headers[header::CONTENT_TYPE],
            "multipart/form-data; boundary=boundary"
        );
        assert_eq!(forwarded.as_ref(), multipart.as_bytes());
        drop(captured);

        proxy_handle.abort();
        official_handle.abort();
    }

    /// A pinned image model sends image requests to CPA with the CPA token and
    /// the pinned model, and the official route is not touched. Mirrors how
    /// `codex-auto-review` may be pinned; upstream failure never re-routes.
    #[tokio::test]
    async fn pinned_image_model_routes_to_cpa_with_isolated_credentials() {
        type CapturedImage = (HeaderMap, Bytes);

        #[derive(Clone, Default)]
        struct RawCapture(Arc<tokio::sync::Mutex<Vec<CapturedImage>>>);

        async fn images(
            State(capture): State<RawCapture>,
            headers: HeaderMap,
            body: Bytes,
        ) -> Response<Body> {
            capture.0.lock().await.push((headers, body));
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"data":[{"b64_json":"AA=="}]}"#))
                .unwrap()
        }

        let cpa_capture = RawCapture::default();
        let (cpa_address, cpa_handle) = spawn_test_app(
            Router::new()
                .route("/v1/images/generations", post(images))
                .with_state(cpa_capture.clone()),
        )
        .await;
        let official_capture = RawCapture::default();
        let (official_address, official_handle) = spawn_test_app(
            Router::new()
                .route("/v1/images/generations", post(images))
                .with_state(official_capture.clone()),
        )
        .await;

        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), official_address, cpa_address);
        let profiles_path = state.cpa_profiles_path.clone();
        crate::cpa::set_image_override(&profiles_path, Some("grok-imagine-image".into())).unwrap();
        let (proxy_address, proxy_handle) = spawn_proxy(state).await;
        let client = reqwest::Client::new();

        let response = client
            .post(format!("http://{proxy_address}/v1/images/generations"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .header("chatgpt-account-id", "account")
            .json(&json!({"model":"gpt-image-2","prompt":"a cat"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let captured = cpa_capture.0.lock().await;
        assert_eq!(captured.len(), 1);
        let (headers, body) = &captured[0];
        // The CPA token replaces the incoming OAuth header entirely.
        assert_eq!(headers[header::AUTHORIZATION], "Bearer cpa-secret");
        assert_ne!(headers[header::AUTHORIZATION], "Bearer oauth");
        assert!(!headers.contains_key("chatgpt-account-id"));
        // The pinned model replaces the client's gpt-image-2.
        let forwarded: Value = serde_json::from_slice(body).unwrap();
        assert_eq!(forwarded["model"], "grok-imagine-image");
        assert_eq!(forwarded["prompt"], "a cat");
        drop(captured);
        // The official endpoint was never contacted.
        assert!(official_capture.0.lock().await.is_empty());

        // Clearing the override restores the official route.
        crate::cpa::set_image_override(&profiles_path, None).unwrap();
        let response = client
            .post(format!("http://{proxy_address}/v1/images/generations"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .json(&json!({"model":"gpt-image-2","prompt":"a cat"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let official = official_capture.0.lock().await;
        assert_eq!(official.len(), 1);
        assert_eq!(official[0].0[header::AUTHORIZATION], "Bearer oauth");
        drop(official);
        assert_eq!(cpa_capture.0.lock().await.len(), 1);

        proxy_handle.abort();
        cpa_handle.abort();
        official_handle.abort();
    }

    /// The override only chooses a destination. A body CodexMux did not parse
    /// as JSON is forwarded untouched rather than rewritten by guesswork.
    #[test]
    fn image_model_rewrite_only_touches_parsed_json_bodies() {
        let json = Bytes::from(r#"{"model":"gpt-image-2","prompt":"a cat"}"#);
        let rewritten = rewrite_image_model(json, "grok-imagine-image");
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["model"], "grok-imagine-image");
        assert_eq!(value["prompt"], "a cat");

        let multipart =
            Bytes::from_static(b"--b\r\nContent-Disposition: form-data\r\n\r\nx\r\n--b--");
        assert_eq!(
            rewrite_image_model(multipart.clone(), "grok-imagine-image"),
            multipart
        );
    }

    /// Without the incoming Codex OAuth header an image request must fail
    /// closed rather than fall back to any other credential.
    #[tokio::test]
    async fn image_requests_without_official_oauth_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let unused = listener.local_addr().unwrap();
        drop(listener);
        let state = auto_review_state(root.path(), unused, unused);
        let (proxy_address, proxy_handle) = spawn_proxy(state).await;

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_address}/v1/images/generations"))
            .header("x-codexmux-token", "proxy")
            .json(&json!({"model":"gpt-image-1","prompt":"a cat"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        proxy_handle.abort();
    }
}
