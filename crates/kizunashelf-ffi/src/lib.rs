//! UniFFI bridge that lets a native host (the iOS/SwiftUI app) drive the
//! KizunaShelf Axum router in-process — the same pattern the Tauri desktop app
//! uses via its `api_request` command, but exposed through UniFFI so it appears
//! to Swift as a plain class with an `async throws` method (no manual C, no
//! pointer/continuation plumbing on the Swift side).
//!
//! The bridge intentionally exposes a *single* tunnel — `request(method, url,
//! body)` — rather than one function per endpoint. Strong per-endpoint typing is
//! layered on top in Swift by swift-openapi-generator, which drives this tunnel
//! through a custom `ClientTransport`. See ../kizunashelf-ios/docs/ios-port-plan.md §4.

mod secrets;
mod vfs;

pub use secrets::{FfiSecretStore, HostSecretStore};
pub use vfs::{FfiVfs, VaultFileSystem, VfsDirEntry, VfsError, VfsFile, VfsMetadata};

use std::sync::Arc;
use std::time::Duration;

use axum::body::{self, Body};
use axum::http::{header, Method, Request};
use axum::Router;
use kizunashelf::api::{router, router_with_vault, ApiOptions as CoreApiOptions};
use kizunashelf::secrets::SecretStore;
use kizunashelf::types::AppConfig;
use kizunashelf::vfs::Vfs;
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

/// Options for an iOS engine backed by a Swift [`VaultFileSystem`]. The vault is
/// addressed by a security-scoped bookmark Swift owns, so `vault_root_label` is
/// only a display string; there is no on-device config file.
#[derive(uniffi::Record)]
pub struct VaultOptions {
    pub vault_root_label: String,
    /// Whether content writes (create/update/delete, asset downloads) are
    /// allowed. Phase 2 browsing uses `false`.
    pub content_writable: bool,
    pub cache_ttl_ms: Option<u64>,
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

    /// Builds an engine backed by a Swift [`VaultFileSystem`] — the iOS path. The
    /// app config is supplied inline (no config file) and the vault config plus
    /// all content are read through `vault`.
    #[uniffi::constructor]
    pub fn with_vault(
        options: VaultOptions,
        vault: Box<dyn VaultFileSystem>,
        secrets: Box<dyn HostSecretStore>,
    ) -> Result<Arc<Self>, KizunaError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| KizunaError::Init {
                message: error.to_string(),
            })?;

        let vault_fs: Arc<dyn Vfs> = Arc::new(FfiVfs::new(vault));
        let secret_store: Arc<dyn SecretStore> = Arc::new(FfiSecretStore::new(secrets));
        let app_config = AppConfig {
            vault_root: options.vault_root_label,
            content_writable: Some(options.content_writable),
        };
        let core_options = CoreApiOptions {
            config_path: PathBuf::new(),
            cache_ttl: Duration::from_millis(options.cache_ttl_ms.unwrap_or(10_000)),
            web_dist_path: None,
            // The vault config (schema) is editable on iOS via the VFS-aware
            // settings handlers; the app config stays in Swift's @AppStorage.
            settings_writable: true,
            content_writable: options.content_writable,
        };
        let router = router_with_vault(core_options, vault_fs, app_config, secret_store);

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

    /// Loads a vault asset (e.g. a downloaded cover) as raw bytes via the
    /// `/api/assets/{path}` route. Separate from [`request`] because asset bodies
    /// are binary and must not be forced through a UTF-8 `String`. This is the
    /// `kizasset://` analogue for iOS image loading.
    pub async fn asset(&self, path: String) -> Result<AssetData, KizunaError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let router = self.router.clone();

        self.runtime.spawn(async move {
            let _ = tx.send(run_asset(router, path).await);
        });

        rx.await.map_err(|_| KizunaError::Bridge {
            message: "asset task was dropped".to_string(),
        })?
    }
}

/// Binary asset payload returned by [`KizunaEngine::asset`].
#[derive(uniffi::Record)]
pub struct AssetData {
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
}

/// Percent-encodes each path segment (keeping `/` separators) so the
/// vault-relative asset path survives as a single wildcard route parameter.
fn encode_asset_path(path: &str) -> String {
    path.split('/')
        .map(|segment| urlencoding::encode(segment).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

async fn run_asset(router: Router, path: String) -> Result<AssetData, KizunaError> {
    let uri = format!("/api/assets/{}", encode_asset_path(&path));
    let request = Request::builder()
        .method(Method::GET)
        .uri(&uri)
        .body(Body::empty())
        .map_err(|error| KizunaError::Bridge {
            message: format!("invalid asset request: {error}"),
        })?;

    let response = router
        .oneshot(request)
        .await
        .map_err(|error| KizunaError::Bridge {
            message: format!("router error: {error}"),
        })?;

    let (parts, body) = response.into_parts();
    if !parts.status.is_success() {
        return Err(KizunaError::Bridge {
            message: format!("asset {path}: HTTP {}", parts.status.as_u16()),
        });
    }
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = body::to_bytes(body, usize::MAX)
        .await
        .map_err(|error| KizunaError::Bridge {
            message: format!("failed to read asset body: {error}"),
        })?
        .to_vec();

    Ok(AssetData {
        bytes,
        content_type,
    })
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
