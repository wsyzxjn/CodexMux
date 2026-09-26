//! Shared upstream transport: route credentials, response header
//! allow-listing, and bounded body reads.

use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, HeaderName, Response, StatusCode, header},
};

use super::{AppState, ProxyError, Route};
use crate::router;

/// Largest search backend response or upstream error body CodexMux buffers.
pub(super) const MAX_AUXILIARY_BODY_BYTES: usize = 16 * 1024 * 1024;

/// A transport error with its cause chain but without the request URL.
pub(super) fn upstream_error_message(error: reqwest::Error) -> String {
    let error = error.without_url();
    let mut message = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// Upstream response headers a client may act on: rate limits, request ids,
/// retry hints, and Codex's own `x-codex-*` signals. Everything else stays
/// behind, in particular cookies and hop-by-hop headers, and
/// `content-length`, which describes framing CodexMux does not preserve.
fn forwards_response_header(name: &HeaderName) -> bool {
    const EXACT: &[&str] = &[
        "content-type",
        "content-encoding",
        "x-request-id",
        "retry-after",
        "x-models-etag",
    ];
    const PREFIXES: &[&str] = &["x-codex-", "x-ratelimit-", "openai-"];
    let name = name.as_str();
    EXACT.contains(&name) || PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

/// A client response carrying the upstream status and allow-listed headers.
pub(super) fn upstream_response(
    status: StatusCode,
    upstream_headers: &HeaderMap,
    body: Body,
) -> Response<Body> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    for (name, value) in upstream_headers {
        if forwards_response_header(name) {
            headers.append(name.clone(), value.clone());
        }
    }
    response
}

/// Stream an upstream response to the client unchanged.
pub(super) fn passthrough_response(upstream: reqwest::Response) -> Response<Body> {
    let mut response = upstream_response(upstream.status(), upstream.headers(), Body::empty());
    *response.body_mut() = Body::from_stream(upstream.bytes_stream());
    response
}

pub(super) fn is_event_stream(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"))
}

/// Buffer an upstream body that CodexMux must inspect, refusing one larger
/// than `limit` before it is held in memory.
pub(super) async fn read_body_limited(
    mut response: reqwest::Response,
    limit: usize,
    code: &'static str,
    what: &str,
) -> Result<Bytes, ProxyError> {
    let too_large = || ProxyError::bad_gateway(code, format!("{what} exceeds {} MiB", limit >> 20));
    let declared = response.content_length();
    if declared.is_some_and(|length| length > limit as u64) {
        return Err(too_large());
    }
    let mut body = Vec::with_capacity(declared.map_or(0, |length| length as usize));
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        ProxyError::bad_gateway(code, format!("{what}: {}", upstream_error_message(error)))
    })? {
        if body.len() + chunk.len() > limit {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(body))
}

pub(super) async fn post_upstream(
    state: &AppState,
    route: Route,
    endpoint: &str,
    headers: HeaderMap,
    body: Bytes,
) -> Result<reqwest::Response, ProxyError> {
    state
        .client
        .post(state.target_url(route, endpoint))
        .headers(headers)
        .body(body)
        .send()
        .await
        .map_err(ProxyError::upstream)
}

/// Send a JSON request on `route` with exactly that route's credential.
pub(super) async fn send_upstream(
    state: &AppState,
    incoming: &HeaderMap,
    endpoint: &str,
    route: Route,
    body: Bytes,
) -> Result<reqwest::Response, ProxyError> {
    let headers = match route {
        Route::Official => router::official_headers(incoming),
        Route::Cpa => router::cpa_headers(incoming, &state.credentials.cpa_token),
    }
    .map_err(ProxyError::credential)?;
    post_upstream(state, route, endpoint, headers, body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::server::test_support::*;
    use axum::{Router, routing::post};
    use serde_json::json;

    #[test]
    fn only_allow_listed_upstream_headers_reach_the_client() {
        for name in [
            "content-type",
            "content-encoding",
            "x-request-id",
            "retry-after",
            "x-models-etag",
            "x-codex-primary-used-percent",
            "x-codex-turn-state",
            "x-ratelimit-remaining-requests",
            "openai-processing-ms",
        ] {
            assert!(
                forwards_response_header(&HeaderName::from_static(name)),
                "{name}"
            );
        }
        for name in [
            "set-cookie",
            "connection",
            "keep-alive",
            "transfer-encoding",
            "content-length",
            "location",
            "www-authenticate",
            "x-internal-debug",
        ] {
            assert!(
                !forwards_response_header(&HeaderName::from_static(name)),
                "{name}"
            );
        }
    }

    /// Official credentials never follow a redirect to another endpoint; the
    /// 3xx is returned to the client without its `location`.
    #[tokio::test]
    async fn official_redirects_are_returned_not_followed() {
        let followed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let followed_counter = followed.clone();
        let (elsewhere, elsewhere_handle) = spawn_test_app(Router::new().route(
            "/{*path}",
            post(move || {
                followed_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async { StatusCode::OK }
            }),
        ))
        .await;
        let (official_address, official_handle) = spawn_test_app(Router::new().route(
            "/v1/responses",
            post(move || async move {
                Response::builder()
                    .status(StatusCode::TEMPORARY_REDIRECT)
                    .header(header::LOCATION, format!("http://{elsewhere}/collect"))
                    .body(Body::empty())
                    .unwrap()
            }),
        ))
        .await;
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), official_address, unused_address());
        let (proxy_address, proxy_handle) = spawn_proxy(state).await;

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let response = client
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .json(&json!({"model": "gpt-5.6", "input": "hi"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert!(response.headers().get(header::LOCATION).is_none());
        assert_eq!(followed.load(std::sync::atomic::Ordering::SeqCst), 0);

        proxy_handle.abort();
        official_handle.abort();
        elsewhere_handle.abort();
    }
}
