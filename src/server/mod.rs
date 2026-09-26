//! The loopback listener: authentication, request activity, and routing to
//! the handlers of each endpoint family.

mod catalog_sync;
mod images;
mod logging;
mod responses;
mod search;
#[cfg(test)]
mod test_support;
mod upstream;

use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, RwLock},
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    Router,
    body::{Body, BodyDataStream, Bytes, HttpBody},
    extract::{
        DefaultBodyLimit, FromRequest, Request, State,
        rejection::{BytesRejection, FailedToBufferBody},
    },
    http::{HeaderMap, HeaderValue, Response, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Json},
    routing::{get, post},
};
use futures_util::Stream;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{sync::watch, time::Instant};

use crate::{
    catalog::CatalogStore,
    config::{Credentials, OFFICIAL_BASE_URL, Paths, Settings},
    continuity::ContinuityStore,
};

pub use catalog_sync::{CPA_STARTUP_READY_TIMEOUT, wait_for_cpa_catalog};
use catalog_sync::{CachedCpaCatalog, handle_models, synchronize_cpa_catalog};
use images::{handle_image_edits, handle_image_generations};
use responses::handle_responses;
use search::handle_alpha_search;
use upstream::upstream_error_message;

const PROXY_TOKEN_HEADER: &str = "x-codexmux-token";
const PROXY_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    Official,
    Cpa,
}

impl Route {
    fn name(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Cpa => "cpa",
        }
    }

    fn identity(self, model: &str) -> String {
        format!("{}:{model}", self.name())
    }

    fn always_replays_history(self) -> bool {
        matches!(self, Self::Cpa)
    }
}

#[derive(Clone)]
pub struct AppState {
    client: reqwest::Client,
    official_base_url: String,
    settings: Arc<Settings>,
    credentials: Arc<Credentials>,
    proxy_token_digest: [u8; 32],
    catalog: Arc<CatalogStore>,
    continuity: Arc<ContinuityStore>,
    /// Profile store path. It is read only by the requests that depend on a
    /// menu-bar choice, so those changes apply without a proxy restart.
    cpa_profiles_path: PathBuf,
    /// Cached `search-detect` results, read per request.
    search_capabilities_path: PathBuf,
    activity: watch::Sender<Activity>,
    idle_timeout: Duration,
    cpa_catalog: Arc<RwLock<Option<CachedCpaCatalog>>>,
}

/// Requests in flight and the moment the proxy last became idle.
#[derive(Clone, Copy, Debug)]
struct Activity {
    in_flight: usize,
    idle_since: Instant,
}

/// Counts one authenticated request as in flight. The guard travels with the
/// response body, so a long streaming response keeps the proxy busy until the
/// stream ends or the client drops it.
struct ActivityGuard(watch::Sender<Activity>);

impl ActivityGuard {
    fn begin(activity: &watch::Sender<Activity>) -> Self {
        activity.send_modify(|activity| activity.in_flight += 1);
        Self(activity.clone())
    }
}

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        self.0.send_modify(|activity| {
            activity.in_flight = activity.in_flight.saturating_sub(1);
            if activity.in_flight == 0 {
                activity.idle_since = Instant::now();
            }
        });
    }
}

impl AppState {
    pub fn new(
        settings: Settings,
        credentials: Credentials,
        paths: &Paths,
    ) -> anyhow::Result<Self> {
        settings.validate()?;
        credentials.validate()?;
        let catalog = CatalogStore::load(paths.catalog.clone())?;
        let (activity, _) = watch::channel(Activity {
            in_flight: 0,
            idle_since: Instant::now(),
        });
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(UPSTREAM_CONNECT_TIMEOUT)
                // Credentials never follow a redirect: every upstream gets
                // exactly the endpoint its route allows, and a 3xx goes back
                // to the client as a response.
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            official_base_url: OFFICIAL_BASE_URL.into(),
            proxy_token_digest: token_digest(credentials.proxy_token.as_bytes()),
            settings: Arc::new(settings),
            credentials: Arc::new(credentials),
            catalog: Arc::new(catalog),
            continuity: Arc::new(ContinuityStore::new()),
            cpa_profiles_path: paths.cpa_profiles.clone(),
            search_capabilities_path: paths.search_capabilities.clone(),
            activity,
            idle_timeout: PROXY_IDLE_TIMEOUT,
            cpa_catalog: Arc::new(RwLock::new(None)),
        })
    }

    /// Resolve once no request has been in flight for the idle timeout. A
    /// request counts until its response body has been fully sent or dropped.
    pub async fn wait_for_idle(&self) {
        let mut activity = self.activity.subscribe();
        loop {
            let current = *activity.borrow_and_update();
            if current.in_flight > 0 {
                if activity.changed().await.is_err() {
                    return;
                }
                continue;
            }
            tokio::select! {
                biased;
                changed = activity.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
                () = tokio::time::sleep_until(current.idle_since + self.idle_timeout) => {
                    tracing::info!(
                        idle_seconds = self.idle_timeout.as_secs(),
                        "proxy idle timeout reached"
                    );
                    return;
                }
            }
        }
    }

    fn request_body_limit_bytes(&self) -> usize {
        self.settings.server.max_request_mib * 1024 * 1024
    }

    fn accepts_proxy_token(&self, headers: &HeaderMap) -> bool {
        let supplied = headers
            .get(PROXY_TOKEN_HEADER)
            .map_or(&[][..], HeaderValue::as_bytes);
        digests_equal(&token_digest(supplied), &self.proxy_token_digest)
    }

    fn target_url(&self, route: Route, endpoint: &str) -> String {
        let base_url = match route {
            Route::Official => &self.official_base_url,
            Route::Cpa => &self.settings.cpa.base_url,
        };
        format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            endpoint.trim_start_matches('/')
        )
    }
}

fn token_digest(token: &[u8]) -> [u8; 32] {
    Sha256::digest(token).into()
}

/// Compare two digests without an early exit. Hashing both tokens first
/// already hides where a guess diverges; this keeps the comparison itself
/// independent of the position of the first differing byte too.
fn digests_equal(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
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
    let body_limit = state.request_body_limit_bytes();
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(handle_models))
        .route("/v1/responses", post(handle_responses))
        .route("/v1/responses/compact", post(handle_responses))
        .route("/v1/alpha/search", post(handle_alpha_search))
        .route("/v1/images/generations", post(handle_image_generations))
        .route("/v1/images/edits", post(handle_image_edits))
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_proxy_token,
        ))
        .with_state(state)
}

/// A request body buffered under `server.max_request_mib`.
struct RequestBody(Bytes);

impl FromRequest<AppState> for RequestBody {
    type Rejection = ProxyError;

    async fn from_request(request: Request, state: &AppState) -> Result<Self, Self::Rejection> {
        let method = request.method().clone();
        let path = request.uri().path().to_owned();
        let content_length = request
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        match Bytes::from_request(request, state).await {
            Ok(bytes) => Ok(Self(bytes)),
            Err(BytesRejection::FailedToBufferBody(FailedToBufferBody::LengthLimitError(_))) => {
                tracing::warn!(
                    %method,
                    %path,
                    max_request_mib = state.settings.server.max_request_mib,
                    content_length = content_length.as_deref().unwrap_or("unknown"),
                    "request body exceeded configured limit"
                );
                Err(ProxyError::payload_too_large(format!(
                    "request body exceeds server.max_request_mib={}",
                    state.settings.server.max_request_mib
                )))
            }
            Err(rejection) => {
                let error = ProxyError::bad_request("request_body", rejection.body_text());
                tracing::warn!(%method, %path, error = %error.message, "request body could not be read");
                Err(error)
            }
        }
    }
}

async fn require_proxy_token(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<axum::response::Response, ProxyError> {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    if !state.accepts_proxy_token(request.headers()) {
        let error = ProxyError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "missing or invalid CodexMux token",
        );
        tracing::warn!(
            %method,
            %path,
            status = error.status.as_u16(),
            error_code = error.code,
            "request rejected"
        );
        return Err(error);
    }
    let activity = ActivityGuard::begin(&state.activity);
    let started = Instant::now();
    let response = next.run(request).await;
    tracing::debug!(
        %method,
        %path,
        status = response.status().as_u16(),
        latency_ms = started.elapsed().as_millis() as u64,
        "request handled"
    );
    Ok(hold_until_body_ends(response, activity))
}

/// Move the activity guard into the response body. A body whose size is
/// already known keeps its `content-length`, which the wrapper would hide.
fn hold_until_body_ends(
    response: axum::response::Response,
    activity: ActivityGuard,
) -> axum::response::Response {
    let (mut parts, body) = response.into_parts();
    let status = parts.status;
    let carries_body = !(status.is_informational()
        || status == StatusCode::NO_CONTENT
        || status == StatusCode::NOT_MODIFIED);
    if let Some(length) = body.size_hint().exact()
        && carries_body
        && !parts.headers.contains_key(header::CONTENT_LENGTH)
    {
        parts
            .headers
            .insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    let body = Body::from_stream(GuardedBody {
        inner: body.into_data_stream(),
        activity: Some(activity),
    });
    Response::from_parts(parts, body)
}

struct GuardedBody {
    inner: BodyDataStream,
    activity: Option<ActivityGuard>,
}

impl Stream for GuardedBody {
    type Item = Result<Bytes, axum::Error>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let polled = Pin::new(&mut self.inner).poll_next(context);
        if matches!(polled, Poll::Ready(None)) {
            self.activity = None;
        }
        polled
    }
}

async fn health() -> Json<Value> {
    Json(json!({"ok": true, "service": "codexmux"}))
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

    fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    fn bad_gateway(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, code, message)
    }

    fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", message)
    }

    /// The incoming request lacks a credential its route needs.
    fn credential(error: anyhow::Error) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "credential", format!("{error:#}"))
    }

    /// A local CodexMux file this request depends on could not be read.
    /// The request fails closed instead of guessing the user's choice.
    fn local_config(error: anyhow::Error) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "local_config",
            format!("{error:#}"),
        )
    }

    fn catalog(error: anyhow::Error) -> Self {
        Self::bad_gateway("catalog", format!("{error:#}"))
    }

    /// An upstream request failed before it produced a response.
    fn upstream(error: reqwest::Error) -> Self {
        Self::bad_gateway("upstream", upstream_error_message(error))
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
    use std::io;

    use axum::http::HeaderName;
    use futures_util::StreamExt;

    use crate::catalog;
    use crate::server::test_support::*;

    #[tokio::test]
    async fn request_body_limit_is_configurable_and_reports_payload_too_large() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        settings.server.max_request_mib = 1;
        let state = AppState::new(
            settings,
            Credentials {
                proxy_token: "proxy".into(),
                cpa_token: "cpa".into(),
                cpa_management_key: "management".into(),
            },
            &Paths::from_root(root.path().to_path_buf()),
        )
        .unwrap();
        let (proxy_address, proxy_handle) = spawn_proxy(state).await;
        let client = reqwest::Client::new();

        let response = client
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .body(vec![b'x'; 1024 * 1024])
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let value: Value = response.json().await.unwrap();
        assert_eq!(value["error"]["code"], "invalid_json");

        let response = client
            .post(format!("http://{proxy_address}/v1/responses"))
            .header("x-codexmux-token", "proxy")
            .body(vec![b'x'; 1024 * 1024 + 1])
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let value: Value = response.json().await.unwrap();
        assert_eq!(value["error"]["code"], "payload_too_large");
        assert_eq!(
            value["error"]["message"],
            "request body exceeds server.max_request_mib=1"
        );

        proxy_handle.abort();
    }

    /// In launchd mode the proxy exits after the idle timeout. A streaming
    /// response that outlives it must keep the proxy alive until it ends.
    #[tokio::test]
    async fn idle_timeout_waits_for_streaming_responses_to_finish() {
        let release = Arc::new(tokio::sync::Notify::new());
        let upstream_release = release.clone();
        let upstream = Router::new().route(
            "/v1/responses",
            post(move || {
                let release = upstream_release.clone();
                async move {
                    let events = async_stream::stream! {
                        yield Ok::<Bytes, io::Error>(Bytes::from_static(
                            b"data: {\"type\":\"response.created\"}\n\n",
                        ));
                        release.notified().await;
                        yield Ok(Bytes::from_static(b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_idle\",\"status\":\"completed\",\"output\":[]}}\n\n"));
                    };
                    Response::builder()
                        .header(header::CONTENT_TYPE, "text/event-stream")
                        .body(Body::from_stream(events))
                        .unwrap()
                }
            }),
        );
        let (cpa_address, cpa_handle) = spawn_test_app(upstream).await;
        let root = tempfile::tempdir().unwrap();
        let mut state = auto_review_state(root.path(), unused_address(), cpa_address);
        state.idle_timeout = Duration::from_millis(200);
        let (proxy_address, proxy_handle) = spawn_proxy(state.clone()).await;

        let mut response = post_json(
            proxy_address,
            "/v1/responses",
            json!({"model":"cpa/glm-5.3-flash", "input":"hi", "stream":true, "store":false}),
        )
        .send()
        .await
        .unwrap();
        assert!(response.chunk().await.unwrap().is_some());
        let idle = tokio::spawn(async move { state.wait_for_idle().await });
        tokio::time::sleep(Duration::from_millis(700)).await;
        assert!(
            !idle.is_finished(),
            "the idle timeout fired while a response was still streaming"
        );

        release.notify_one();
        while response.chunk().await.unwrap().is_some() {}
        tokio::time::timeout(Duration::from_secs(5), idle)
            .await
            .expect("the proxy became idle after the stream ended")
            .unwrap();

        proxy_handle.abort();
        cpa_handle.abort();
    }

    #[tokio::test]
    async fn activity_ends_when_a_response_body_ends_or_is_dropped() {
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), unused_address(), unused_address());
        let in_flight = || state.activity.borrow().in_flight;

        let pending =
            Body::from_stream(futures_util::stream::pending::<Result<Bytes, io::Error>>());
        let response = hold_until_body_ends(
            Response::new(pending),
            ActivityGuard::begin(&state.activity),
        );
        assert_eq!(in_flight(), 1);
        drop(response);
        assert_eq!(in_flight(), 0);

        // A buffered body keeps its length and bytes, and the guard is
        // released once the body has been read to the end.
        let response = hold_until_body_ends(
            Json(json!({"ok": true})).into_response(),
            ActivityGuard::begin(&state.activity),
        );
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "11");
        let mut body = response.into_body().into_data_stream();
        assert_eq!(body.next().await.unwrap().unwrap(), r#"{"ok":true}"#);
        assert_eq!(in_flight(), 1);
        assert!(body.next().await.is_none());
        assert_eq!(in_flight(), 0);
    }

    #[test]
    fn proxy_token_must_match_exactly() {
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), unused_address(), unused_address());
        let supplied = |token: &str| {
            HeaderMap::from_iter([(
                HeaderName::from_static(PROXY_TOKEN_HEADER),
                HeaderValue::from_str(token).unwrap(),
            )])
        };
        assert!(state.accepts_proxy_token(&supplied("proxy")));
        for wrong in ["prox", "proxy2", "PROXY", "", "cpa-secret"] {
            assert!(!state.accepts_proxy_token(&supplied(wrong)), "{wrong}");
        }
        assert!(!state.accepts_proxy_token(&HeaderMap::new()));
    }

    /// A body that fails mid-read is a client error, distinct from one that
    /// exceeds `server.max_request_mib`.
    #[tokio::test]
    async fn unreadable_request_bodies_are_bad_requests() {
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), unused_address(), unused_address());
        let body = Body::from_stream(futures_util::stream::iter([
            Ok(Bytes::from_static(b"{\"model\":")),
            Err(io::Error::other("connection reset")),
        ]));
        let request = Request::builder()
            .method("POST")
            .uri("/v1/responses")
            .body(body)
            .unwrap();
        let error = RequestBody::from_request(request, &state)
            .await
            .err()
            .unwrap();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "request_body");
    }

    /// Profiles only matter to the requests that depend on a menu-bar
    /// choice. A broken profile store must not take plain official or CPA
    /// traffic down, and the dependent requests must fail closed.
    #[tokio::test]
    async fn unreadable_profiles_fail_only_the_requests_that_depend_on_them() {
        let official = Router::new()
            .route(
                "/v1/responses",
                post(|| async { completed_json("resp_official") }),
            )
            .route(
                "/v1/images/generations",
                post(|| async { Json(json!({"data": []})) }),
            )
            .route(
                "/v1/models",
                get(|| async { Json(json!({"models":[{"slug":"gpt-5.6"}]})) }),
            );
        let cpa = Router::new()
            .route(
                "/v1/responses",
                post(|| async { completed_json("resp_cpa") }),
            )
            .route(
                "/v1/models",
                get(|| async { Json(json!({"models":[{"slug":"glm-5.3-flash"}]})) }),
            );
        let (official_address, official_handle) = spawn_test_app(official).await;
        let (cpa_address, cpa_handle) = spawn_test_app(cpa).await;
        let root = tempfile::tempdir().unwrap();
        let state = auto_review_state(root.path(), official_address, cpa_address);
        std::fs::write(&state.cpa_profiles_path, "review_override = [not toml").unwrap();
        let (proxy_address, proxy_handle) = spawn_proxy(state).await;

        for model in ["gpt-5.6", "cpa/glm-5.3-flash"] {
            let response = post_json(
                proxy_address,
                "/v1/responses",
                json!({"model": model, "input": "hi", "stream": false}),
            )
            .send()
            .await
            .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{model}");
        }
        // Official models search natively, so the search setting is not read.
        let searched = post_json(
            proxy_address,
            "/v1/responses",
            json!({"model": "gpt-5.6", "input": "hi", "tools": [{"type": "web_search"}]}),
        )
        .send()
        .await
        .unwrap();
        assert_eq!(searched.status(), StatusCode::OK);

        for (path, body) in [
            (
                "/v1/responses",
                json!({"model": catalog::AUTO_REVIEW_MODEL, "input": "review"}),
            ),
            (
                "/v1/responses",
                json!({"model": "cpa/glm-5.3-flash", "input": "hi", "tools": [{"type": "web_search"}]}),
            ),
            (
                "/v1/images/generations",
                json!({"model": "gpt-image-2", "prompt": "a cat"}),
            ),
        ] {
            let response = post_json(proxy_address, path, body.clone())
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "{body}"
            );
            let error: Value = response.json().await.unwrap();
            assert_eq!(error["error"]["code"], "local_config", "{body}");
        }

        // Catalog overrides are display metadata: the catalog is still served.
        let models = reqwest::Client::new()
            .get(format!(
                "http://{proxy_address}/v1/models?client_version=test"
            ))
            .header("x-codexmux-token", "proxy")
            .header(header::AUTHORIZATION, "Bearer oauth")
            .send()
            .await
            .unwrap();
        assert_eq!(models.status(), StatusCode::OK);
        let models: Value = models.json().await.unwrap();
        assert_eq!(models["models"][0]["slug"], "gpt-5.6");

        proxy_handle.abort();
        official_handle.abort();
        cpa_handle.abort();
    }

    #[test]
    fn route_identity_is_model_specific() {
        assert_ne!(Route::Cpa.identity("one"), Route::Cpa.identity("two"));
        assert_ne!(Route::Cpa.identity("one"), Route::Official.identity("one"));
        assert_ne!(
            Route::Official.identity("one"),
            Route::Official.identity("two")
        );
    }
}
