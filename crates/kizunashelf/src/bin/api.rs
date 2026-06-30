use anyhow::Result;
use kizunashelf::api::{router_native, ApiOptions};
use kizunashelf::secrets::NativeSecretStore;
use kizunashelf::types::AppConfig;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Self-hosted web server. Single-vault by design: the vault directory is mounted
/// and pointed at by `KIZUNASHELF_VAULT_ROOT` (the schema still lives inside it at
/// `KizunaShelf/config.yaml`). There is no app config file — runtime behavior is
/// controlled entirely by environment variables, so an extra config file beside a
/// mounted vault would be redundant. For multiple vaults, run multiple instances.
#[tokio::main]
async fn main() -> Result<()> {
    let vault_root =
        std::env::var("KIZUNASHELF_VAULT_ROOT").unwrap_or_else(|_| "/vault".to_string());
    let port = std::env::var("PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or(8787);
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let cache_ttl = std::env::var("KIZUNASHELF_CACHE_TTL_MS")
        .ok()
        .and_then(|ttl| ttl.parse::<u64>().ok())
        .unwrap_or(10_000);
    let serve_web = std::env::var("KIZUNASHELF_SERVE_WEB")
        .map(|value| value != "false")
        .unwrap_or(true);
    let web_dist_path = serve_web.then(|| {
        std::env::var("KIZUNASHELF_WEB_DIST")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("apps/web/dist"))
    });
    let settings_writable = std::env::var("KIZUNASHELF_SETTINGS_WRITABLE")
        .ok()
        .and_then(|value| parse_bool(&value))
        .unwrap_or_else(|| is_loopback_host(&host));
    let content_writable = std::env::var("KIZUNASHELF_CONTENT_WRITABLE")
        .ok()
        .and_then(|value| parse_bool(&value))
        .unwrap_or_else(|| is_loopback_host(&host));

    // The provider token cache (derived OAuth tokens) needs a writable path. It
    // deliberately defaults *outside* the vault (the system temp dir) so it is
    // never synced to other machines along with the vault; it only holds
    // re-derivable OAuth tokens, so losing it on reboot is harmless. Override
    // with `KIZUNASHELF_TOKEN_CACHE` to persist it somewhere durable.
    let token_cache = std::env::var("KIZUNASHELF_TOKEN_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join(".kizunashelf.tokens.json"));

    let app_config = AppConfig {
        vault_root,
        content_writable: Some(content_writable),
    };
    let secret_store = Arc::new(NativeSecretStore::with_token_path(token_cache));
    // Optional *persistent* index cache (outside the vault). When unset, the core
    // falls back to a process-resident in-memory index cache, so a changed reload
    // still re-parses only changed files (not the whole vault) — it's just lost on
    // restart. Set `KIZUNASHELF_INDEX_CACHE_DIR` to also persist it across
    // restarts (worthwhile for large vaults / frequent restarts).
    let index_cache_dir = std::env::var("KIZUNASHELF_INDEX_CACHE_DIR")
        .ok()
        .map(PathBuf::from);
    let options = ApiOptions {
        // No app config file in the web runtime; the app config is inline.
        config_path: PathBuf::new(),
        cache_ttl: Duration::from_millis(cache_ttl),
        web_dist_path,
        settings_writable,
        content_writable,
        index_cache_dir,
        // The network server never does host-path ingest; the web client uses the
        // reqwest-based download path. Keep this off so the `source_path`
        // read/delete surface is unreachable here.
        host_asset_ingest: false,
    };
    let app = router_native(options, app_config, secret_store);

    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("KizunaShelf listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    println!("KizunaShelf shut down");
    Ok(())
}

/// Resolves when the process is asked to stop: SIGINT (Ctrl-C, local runs) or
/// SIGTERM (`docker stop` / container restart). Installing an explicit handler
/// matters in containers: as PID 1 the process otherwise *ignores* SIGTERM, so
/// Docker would wait out its grace period and then SIGKILL it. With this, the
/// server stops accepting connections, drains in-flight requests, and exits
/// promptly.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}
