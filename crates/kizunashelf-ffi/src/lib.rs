//! UniFFI bridge that lets a native host (the iOS/SwiftUI app) drive the
//! KizunaShelf Axum router in-process — the same pattern the Tauri desktop app
//! uses via its `api_request` command, but exposed through UniFFI so it appears
//! to Swift as a plain class with an `async throws` method (no manual C, no
//! pointer/continuation plumbing on the Swift side).
//!
//! The bridge intentionally exposes a *single* tunnel — `request(method, url,
//! body)` — rather than one function per endpoint. Strong per-endpoint typing is
//! layered on top in Swift by swift-openapi-generator, which drives this tunnel
//! through a custom `ClientTransport`. See docs/ios-port-plan.md §4.

use std::sync::Arc;
use std::time::Duration;

use axum::body::{self, Body};
use axum::http::{header, Method, Request};
use axum::Router;
use kizunashelf::api::{router, ApiOptions as CoreApiOptions};
use std::path::PathBuf;
use tokio::runtime::Runtime;
use tower::ServiceExt;

uniffi::setup_scaffolding!();

/// Options passed to the core at init. On iOS these come from `@AppStorage`
/// (there is no on-device config file); `config_path` points into the app
/// container. Mirrors the desktop `ApiOptions` minus the desktop-only fields.
#[derive(uniffi::Record)]
pub struct ApiOptions {
    pub config_path: String,
    pub cache_ttl_ms: Option<u64>,
    pub settings_writable: Option<bool>,
    pub content_writable: Option<bool>,
}

/// One response from the core: HTTP-like status, body, and content type.
#[derive(uniffi::Record)]
pub struct ApiResponse {
    pub status: u16,
    pub body: String,
    pub content_type: Option<String>,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum KizunaError {
    #[error("failed to initialize core: {message}")]
    Init { message: String },
    #[error("bridge error: {message}")]
    Bridge { message: String },
}

/// The in-process KizunaShelf core. Owns a Tokio runtime that drives request
/// futures and the cloneable Axum router. Exposed to Swift as a class.
#[derive(uniffi::Object)]
pub struct KizunaEngine {
    runtime: Runtime,
    router: Router,
}

#[uniffi::export]
impl KizunaEngine {
    /// Builds the runtime + router from `options`.
    #[uniffi::constructor]
    pub fn new(options: ApiOptions) -> Result<Arc<Self>, KizunaError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| KizunaError::Init {
                message: error.to_string(),
            })?;

        let router = router(CoreApiOptions {
            config_path: PathBuf::from(options.config_path),
            cache_ttl: Duration::from_millis(options.cache_ttl_ms.unwrap_or(10_000)),
            web_dist_path: None,
            settings_writable: options.settings_writable.unwrap_or(true),
            content_writable: options.content_writable.unwrap_or(true),
        });

        Ok(Arc::new(Self { runtime, router }))
    }

    /// Runs one request through the router and returns the response. Appears to
    /// Swift as `func request(...) async throws -> ApiResponse`.
    ///
    /// The work runs on the owned Tokio runtime (so reqwest / tokio::fs have a
    /// proper runtime context); the awaited future is just a oneshot receiver,
    /// which is executor-agnostic, so UniFFI's foreign async driver can poll it
    /// without needing its own Tokio integration.
    pub async fn request(
        &self,
        method: String,
        url: String,
        body: Option<String>,
    ) -> Result<ApiResponse, KizunaError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let router = self.router.clone();
        let body = body.unwrap_or_default();

        self.runtime.spawn(async move {
            let _ = tx.send(run_request(router, method, url, body).await);
        });

        rx.await.map_err(|_| KizunaError::Bridge {
            message: "request task was dropped".to_string(),
        })?
    }
}

async fn run_request(
    router: Router,
    method: String,
    url: String,
    body: String,
) -> Result<ApiResponse, KizunaError> {
    let method = method.parse::<Method>().map_err(|_| KizunaError::Bridge {
        message: format!("invalid method: {method}"),
    })?;

    let request = Request::builder()
        .method(method)
        .uri(&url)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .map_err(|error| KizunaError::Bridge {
            message: format!("invalid request: {error}"),
        })?;

    let response = router
        .oneshot(request)
        .await
        .map_err(|error| KizunaError::Bridge {
            message: format!("router error: {error}"),
        })?;

    let (parts, response_body) = response.into_parts();
    let status = parts.status.as_u16();
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let bytes = body::to_bytes(response_body, usize::MAX)
        .await
        .map_err(|error| KizunaError::Bridge {
            message: format!("failed to read body: {error}"),
        })?;
    let body = String::from_utf8(bytes.to_vec()).map_err(|_| KizunaError::Bridge {
        message: "response body was not valid UTF-8".to_string(),
    })?;

    Ok(ApiResponse {
        status,
        body,
        content_type,
    })
}
