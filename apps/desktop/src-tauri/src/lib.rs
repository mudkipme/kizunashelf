use axum::http::{header, Method, Response, StatusCode};
use axum::Router;
use kizunashelf::api::{provider_credential_keys, router_native, tunnel, ApiOptions};
use kizunashelf::secrets::SecretStore;
use kizunashelf::types::AppConfig;
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tracing_subscriber::EnvFilter;

mod secret_store;
mod vaults;

use secret_store::KeyringSecretStore;
use vaults::VaultStoreData;

/// Custom URI scheme used by the webview to load locally stored vault assets
/// (downloaded covers). It forwards to the in-process API `/api/assets` route so
/// binary image data is served directly rather than through the JSON command
/// bridge. Must match `DESKTOP_ASSET_SCHEME` in `apps/web/src/lib/asset-src.ts`,
/// which builds the URLs the webview requests against this scheme.
const ASSET_SCHEME: &str = "kizasset";

/// Desktop runtime state. The API router is rebuilt whenever the active vault
/// changes (multi-vault switching), so it lives behind a mutex; it is `None`
/// when no vault is selected yet. Provider credentials + the OAuth token cache
/// are stored in the OS keychain via `secret_store`, shared across vaults.
struct DesktopState {
    api: Mutex<Option<Router>>,
    vaults_path: PathBuf,
    cache_ttl: Duration,
    secret_store: Arc<dyn SecretStore>,
    /// Persistent index-cache directory (the OS cache dir, resolved at setup).
    /// Shared across vaults; per-vault entries are keyed inside it.
    index_cache_dir: PathBuf,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopApiResponse {
    status: u16,
    body: String,
    content_type: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultInfo {
    name: String,
    path: String,
    active: bool,
}

/// Provider credentials as a `secret-key → value` map (e.g. `igdb_client_id`).
/// The key set is owned by the core provider registry, not duplicated here, so
/// adding a provider needs no change to this file or the credentials UI.
type Credentials = std::collections::BTreeMap<String, String>;

#[tauri::command]
async fn api_request(
    state: State<'_, DesktopState>,
    method: String,
    url: String,
    body: Option<String>,
) -> Result<DesktopApiResponse, String> {
    // Clone the current router out of the mutex so the lock isn't held across
    // the await; respond 503 when no vault is open (the UI shows the chooser).
    let router = state.api.lock().unwrap().clone();
    let Some(router) = router else {
        return Ok(DesktopApiResponse {
            status: 503,
            body: "{\"error\":\"No vault is open\"}".to_string(),
            content_type: Some("application/json".to_string()),
        });
    };
    let method = method
        .parse::<Method>()
        .map_err(|error| format!("Invalid method {method}: {error}"))?;
    let response = tunnel::run_json(router, method, &url, body.unwrap_or_default())
        .await
        .map_err(|error| error.to_string())?;
    let body = String::from_utf8(response.body)
        .map_err(|error| format!("API returned non-UTF-8 response: {error}"))?;

    Ok(DesktopApiResponse {
        status: response.status.as_u16(),
        body,
        content_type: response.content_type,
    })
}

// MARK: Vault management commands

#[tauri::command]
fn list_vaults(state: State<DesktopState>) -> Vec<VaultInfo> {
    vault_infos(&vaults::load(&state.vaults_path))
}

#[tauri::command]
fn add_vault(state: State<DesktopState>, path: String) -> Result<Vec<VaultInfo>, String> {
    let name = vaults::name_for(&path);
    apply(&state, |data| data.add(name, path.clone()))
}

#[tauri::command]
fn create_vault(
    state: State<DesktopState>,
    parent: String,
    name: String,
) -> Result<Vec<VaultInfo>, String> {
    let path = vaults::create_vault(&parent, &name).map_err(|error| error.to_string())?;
    let display = name.trim().to_string();
    apply(&state, |data| data.add(display, path.clone()))
}

#[tauri::command]
fn switch_vault(state: State<DesktopState>, path: String) -> Result<Vec<VaultInfo>, String> {
    apply(&state, |data| data.set_active(&path))
}

#[tauri::command]
fn remove_vault(state: State<DesktopState>, path: String) -> Result<Vec<VaultInfo>, String> {
    apply(&state, |data| data.remove(&path))
}

// MARK: Credentials commands (stored in the OS keychain)

#[tauri::command]
fn get_credentials(state: State<DesktopState>) -> Credentials {
    let store = &state.secret_store;
    provider_credential_keys()
        .into_iter()
        .map(|key| (key.to_string(), store.get(key).unwrap_or_default()))
        .collect()
}

#[tauri::command]
fn set_credentials(state: State<DesktopState>, credentials: Credentials) -> Result<(), String> {
    let store = &state.secret_store;
    // Only write keys the core registry actually declares, so a stale or crafted
    // payload can't stash arbitrary entries in the keychain.
    for key in provider_credential_keys() {
        if let Some(value) = credentials.get(key) {
            store
                .set(key, value.trim())
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

// MARK: Helpers

/// Applies a mutation to the persisted vault list, saves it, rebuilds the active
/// router, and returns the updated list for the UI.
fn apply(
    state: &DesktopState,
    mutate: impl FnOnce(&mut VaultStoreData),
) -> Result<Vec<VaultInfo>, String> {
    let mut data = vaults::load(&state.vaults_path);
    mutate(&mut data);
    vaults::save(&state.vaults_path, &data).map_err(|error| error.to_string())?;
    rebuild_active(state, &data);
    Ok(vault_infos(&data))
}

fn rebuild_active(state: &DesktopState, data: &VaultStoreData) {
    let router = data.active.as_ref().map(|path| {
        build_router(
            path,
            state.cache_ttl,
            Arc::clone(&state.secret_store),
            state.index_cache_dir.clone(),
        )
    });
    *state.api.lock().unwrap() = router;
}

fn vault_infos(data: &VaultStoreData) -> Vec<VaultInfo> {
    data.vaults
        .iter()
        .map(|entry| VaultInfo {
            name: entry.name.clone(),
            path: entry.path.clone(),
            active: data.active.as_deref() == Some(entry.path.as_str()),
        })
        .collect()
}

fn build_router(
    vault_root: &str,
    cache_ttl: Duration,
    secret_store: Arc<dyn SecretStore>,
    index_cache_dir: PathBuf,
) -> Router {
    router_native(
        ApiOptions {
            // Desktop has no app config file; the app config is inline and the
            // frontend is served by Tauri (not this in-process router).
            config_path: PathBuf::new(),
            cache_ttl,
            web_dist_path: None,
            settings_writable: true,
            content_writable: true,
            // Persistent index cache (outside the vault, in the OS cache dir) to
            // speed up cold starts on large vaults. A disposable cache; per-vault
            // entries are keyed by vault identity inside the dir, so one dir is
            // shared across vaults.
            index_cache_dir: Some(index_cache_dir),
            index_cache_identity: None,
            // The desktop frontend uses the reqwest-based download path; it never
            // hands the core a host file path, so this host-only surface stays off.
            host_asset_ingest: false,
        },
        AppConfig {
            vault_root: vault_root.to_string(),
            content_writable: Some(true),
        },
        secret_store,
    )
}

/// Serves a local vault asset by forwarding to the active router's
/// `/api/assets/{path}` route.
async fn forward_asset_request(
    app: AppHandle,
    request: tauri::http::Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let router = app.state::<DesktopState>().api.lock().unwrap().clone();
    let Some(api) = router else {
        return asset_error_response();
    };
    let uri = format!("/api/assets{}", request.uri().path());
    match tunnel::run_asset(api, &uri).await {
        Ok(response) => {
            let content_type = response
                .content_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            Response::builder()
                .status(response.status)
                .header(header::CONTENT_TYPE, content_type)
                .body(response.body)
                .unwrap_or_else(|_| asset_error_response())
        }
        Err(_) => asset_error_response(),
    }
}

fn asset_error_response() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Vec::new())
        .expect("static asset error response is valid")
}

pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(ASSET_SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                responder.respond(forward_asset_request(app, request).await);
            });
        })
        .setup(|app| {
            let vaults_path = app.path().app_data_dir()?.join("vaults.json");
            // The OS cache dir (XDG_CACHE_HOME / ~/.cache on Linux, ~/Library/Caches
            // on macOS, %LOCALAPPDATA% on Windows), resolved the same way as the app
            // data dir above. Holds the disposable persistent index cache.
            let index_cache_dir = app.path().app_cache_dir()?.join("KizunaIndexCache");
            let cache_ttl = env::var("KIZUNASHELF_CACHE_TTL_MS")
                .ok()
                .and_then(|ttl| ttl.parse::<u64>().ok())
                .map(Duration::from_millis)
                .unwrap_or_else(|| Duration::from_millis(10_000));
            let secret_store: Arc<dyn SecretStore> = Arc::new(KeyringSecretStore::new());

            let data = vaults::load(&vaults_path);
            let api = data.active.as_ref().map(|path| {
                build_router(
                    path,
                    cache_ttl,
                    Arc::clone(&secret_store),
                    index_cache_dir.clone(),
                )
            });
            app.manage(DesktopState {
                api: Mutex::new(api),
                vaults_path,
                cache_ttl,
                secret_store,
                index_cache_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api_request,
            list_vaults,
            add_vault,
            create_vault,
            switch_vault,
            remove_vault,
            get_credentials,
            set_credentials
        ])
        .run(tauri::generate_context!())
        .expect("failed to run KizunaShelf desktop app");
}

/// Sends the core's `tracing` events to stderr, filtered by the standard
/// `RUST_LOG` (`info` when unset). `try_init` rather than `init` so a second
/// call — or a host that already installed a subscriber — is a no-op instead of
/// a panic on startup.
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();
}
