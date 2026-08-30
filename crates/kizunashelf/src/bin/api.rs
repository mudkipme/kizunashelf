use anyhow::Result;
use argon2::{Argon2, PasswordHasher};
use kizunashelf::api::{protect_web_router, router_native, ApiOptions, WebAuth};
use kizunashelf::secrets::NativeSecretStore;
use kizunashelf::types::AppConfig;
use std::future::IntoFuture;
use std::io::{self, IsTerminal, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

/// Self-hosted web server. Single-vault by design: the vault directory is mounted
/// and pointed at by `KIZUNASHELF_VAULT_ROOT` (the schema still lives inside it at
/// `KizunaShelf/config.yaml`). There is no app config file — runtime behavior is
/// controlled entirely by environment variables, so an extra config file beside a
/// mounted vault would be redundant. For multiple vaults, run multiple instances.
#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("hash-password") {
        return hash_password_interactive();
    }
    init_tracing();

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
        index_cache_identity: None,
        // The network server never does host-path ingest; the web client uses the
        // reqwest-based download path. Keep this off so the `source_path`
        // read/delete surface is unreachable here.
        host_asset_ingest: false,
    };
    let mut app = router_native(options, app_config, secret_store);
    match std::env::var("KIZUNASHELF_AUTH_PASSWORD_HASH") {
        Ok(password_hash) => {
            app = protect_web_router(app, WebAuth::new(password_hash)?);
            tracing::info!("password authentication enabled");
        }
        Err(std::env::VarError::NotPresent) => {}
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("KIZUNASHELF_AUTH_PASSWORD_HASH must be valid UTF-8");
        }
    }

    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    // Kept clickable in a terminal, as the previous plain `println!` was.
    let url = format!("http://{}", listener.local_addr()?);
    tracing::info!(
        %url,
        content_writable,
        settings_writable,
        "KizunaShelf listening",
    );
    // `with_connect_info` puts the peer address in each request's extensions.
    // The login throttle needs it to tell one client from another; without it
    // every attempt lands in one shared bucket and any visitor can lock the
    // owner out. See `ClientAddr` in `api/web_auth.rs`.
    let service = app.into_make_service_with_connect_info::<SocketAddr>();

    // Draining is bounded, not open-ended. A graceful shutdown waits for every
    // in-flight request, and `/api/vault/changes` is a 25-second long-poll that
    // each open browser tab keeps one of — so with the app open anywhere, an
    // unbounded drain outlives a container runtime's stop timeout (10s for
    // Docker and Podman) and the process is SIGKILLed instead, which reports as
    // a failed unit. Five seconds is longer than any real request here and
    // comfortably inside that window.
    let (drain, mut draining) = tokio::sync::watch::channel(false);
    // `IntoFuture`, not `Future`, so it has to be converted before it can be
    // polled alongside the drain timer.
    let server = axum::serve(listener, service)
        .with_graceful_shutdown(async move {
            let _ = draining.changed().await;
        })
        .into_future();
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result?,
        () = async {
            shutdown_signal().await;
            tracing::info!("draining in-flight requests");
            let _ = drain.send(true);
            tokio::time::sleep(DRAIN_GRACE).await;
        } => {
            // Dropping the server here closes the listener and the connections
            // still open on it. Nothing is lost that was not already going to be
            // lost to the SIGKILL this replaces.
            tracing::warn!(
                seconds = DRAIN_GRACE.as_secs(),
                "requests still in flight after the drain window; exiting anyway",
            );
        }
    }
    tracing::info!("KizunaShelf shut down");
    Ok(())
}

/// Sends the core's `tracing` events to stdout. Filtering is the standard
/// `RUST_LOG` (`info` when unset), so a container can be turned up to
/// `RUST_LOG=kizunashelf=debug` without a rebuild. ANSI is decided by whether
/// stdout is a terminal, so `docker logs` stays free of escape codes.
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_ansi(io::stdout().is_terminal())
        .init();
}

fn hash_password_interactive() -> Result<()> {
    let password = rpassword::prompt_password("Password: ")?;
    if password.is_empty() {
        anyhow::bail!("password must not be empty");
    }
    if password.len() > 1024 {
        anyhow::bail!("password must be at most 1024 bytes");
    }
    let confirmation = rpassword::prompt_password("Confirm password: ")?;
    if password != confirmation {
        anyhow::bail!("passwords do not match");
    }

    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|error| anyhow::anyhow!("failed to hash password: {error}"))?;
    writeln!(io::stdout().lock(), "{hash}")?;
    Ok(())
}

/// How long a shutdown waits for in-flight requests before closing the listener
/// regardless. Must stay well under a container runtime's stop timeout.
const DRAIN_GRACE: Duration = Duration::from_secs(5);

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
