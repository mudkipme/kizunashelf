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

use axum::http::Method;
use axum::Router;
use kizunashelf::api::{router_with_vault, tunnel, ApiOptions as CoreApiOptions};
use kizunashelf::secrets::SecretStore;
use kizunashelf::types::AppConfig;
use kizunashelf::vfs::Vfs;
use std::path::PathBuf;
use tokio::runtime::Runtime;

uniffi::setup_scaffolding!();

/// The default starter vault schema (the "Media Library" preset) serialized to
/// the YAML written into `<vault>/KizunaShelf/config.yaml`. Defined once in the
/// core (`kizunashelf::templates`) and shared with web onboarding and the desktop
/// create-vault flow; iOS writes it directly through its `VaultFileSystem`.
#[uniffi::export]
pub fn starter_vault_config_yaml() -> String {
    kizunashelf::templates::starter_vault_config_yaml()
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
    /// Host directory for the persistent index cache, e.g. the app container's
    /// Caches dir. It is a real on-device path the in-process core touches with
    /// `std::fs` (NOT the security-scoped vault), so it needs no bookmark. `None`
    /// disables the cache; entries are keyed per vault inside the dir.
    pub index_cache_dir: Option<String>,
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
            index_cache_dir: options.index_cache_dir.map(PathBuf::from),
            // The in-process host (the Swift app) downloads covers itself and hands
            // the core a sandboxed temp-file path to ingest. This runtime is the
            // only trusted caller, so it's the only one allowed host-path ingest.
            host_asset_ingest: true,
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
    let response = tunnel::run_asset(router, &uri)
        .await
        .map_err(bridge_error)?;
    if !response.status.is_success() {
        return Err(KizunaError::Bridge {
            message: format!("asset {path}: HTTP {}", response.status.as_u16()),
        });
    }
    Ok(AssetData {
        bytes: response.body,
        content_type: response.content_type,
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
    let response = tunnel::run_json(router, method, &url, body)
        .await
        .map_err(bridge_error)?;
    let body = String::from_utf8(response.body).map_err(|_| KizunaError::Bridge {
        message: "response body was not valid UTF-8".to_string(),
    })?;
    Ok(ApiResponse {
        status: response.status.as_u16(),
        body,
        content_type: response.content_type,
    })
}

fn bridge_error(error: tunnel::TunnelError) -> KizunaError {
    KizunaError::Bridge {
        message: error.to_string(),
    }
}
