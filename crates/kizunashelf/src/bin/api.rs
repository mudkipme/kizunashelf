use anyhow::Result;
use kizunashelf::api::{router, ApiOptions};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = std::env::var("KIZUNASHELF_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("config/kizunashelf.config.json"));
    let port = std::env::var("PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or(8787);
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
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
    let app = router(ApiOptions {
        config_path,
        cache_ttl: Duration::from_millis(cache_ttl),
        web_dist_path,
        load_on_blocking_thread: false,
    });
    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("KizunaShelf listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
