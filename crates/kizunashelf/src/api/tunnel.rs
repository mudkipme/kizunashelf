//! Shared host tunnel: the request → router → response plumbing common to every
//! in-process host shell — the desktop Tauri `api_request` / asset-scheme handler
//! and the iOS UniFFI `request` / `asset` tunnel. Each host keeps its own public
//! surface (its response Record/struct, UTF-8-string vs raw-bytes body handling,
//! and host-specific error type); this owns the part that is byte-for-byte
//! identical across all of them: build the request, drive it through the `Router`
//! via a `tower` oneshot, and buffer the full response. See the "one core, three
//! runtimes" section of ARCHITECTURE.md.

use axum::body::{self, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

/// A fully-buffered response from the in-process router. Hosts convert this into
/// their own shape (a UTF-8 `String` body for the JSON tunnel, raw bytes for
/// assets) and decide how non-success statuses are handled.
pub struct TunnelResponse {
    pub status: StatusCode,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Failure while tunneling a request: the request couldn't be built, the router
/// service errored, or the body couldn't be buffered. Hosts map this to their own
/// error type via its `Display`.
#[derive(Debug)]
pub enum TunnelError {
    /// The HTTP request could not be constructed (e.g. an invalid URI).
    InvalidRequest(String),
    /// The router service errored while driving the request.
    Router(String),
    /// The response body could not be read into memory.
    Body(String),
}

impl std::fmt::Display for TunnelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TunnelError::InvalidRequest(message) => write!(f, "invalid request: {message}"),
            TunnelError::Router(message) => write!(f, "router error: {message}"),
            TunnelError::Body(message) => write!(f, "failed to read body: {message}"),
        }
    }
}

impl std::error::Error for TunnelError {}

/// Run a JSON request through the router. Sets `content-type: application/json`
/// (matching the web transport), which the contract-typed handlers expect.
/// `body` is the raw request body — an empty string for bodyless methods.
pub async fn run_json(
    router: Router,
    method: Method,
    uri: &str,
    body: String,
) -> Result<TunnelResponse, TunnelError> {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .map_err(|error| TunnelError::InvalidRequest(error.to_string()))?;
    collect(router, request).await
}

/// Run a `GET` against an already-built asset URI (`/api/assets/{path}`). The
/// caller assembles the URI because the encoding differs per host: the desktop
/// forwards an already-encoded webview path, while iOS percent-encodes a raw
/// vault-relative path.
pub async fn run_asset(router: Router, uri: &str) -> Result<TunnelResponse, TunnelError> {
    let request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .map_err(|error| TunnelError::InvalidRequest(error.to_string()))?;
    collect(router, request).await
}

async fn collect(router: Router, request: Request<Body>) -> Result<TunnelResponse, TunnelError> {
    let response = router
        .oneshot(request)
        .await
        .map_err(|error| TunnelError::Router(error.to_string()))?;
    let (parts, body) = response.into_parts();
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = body::to_bytes(body, usize::MAX)
        .await
        .map_err(|error| TunnelError::Body(error.to_string()))?;
    Ok(TunnelResponse {
        status: parts.status,
        content_type,
        body: bytes.to_vec(),
    })
}
