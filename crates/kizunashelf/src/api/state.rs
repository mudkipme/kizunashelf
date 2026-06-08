use crate::library::read_library_from_config;
use crate::types::Library;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ApiOptions {
    pub config_path: PathBuf,
    pub cache_ttl: Duration,
    pub web_dist_path: Option<PathBuf>,
    pub settings_writable: bool,
    pub content_writable: bool,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) options: ApiOptions,
    cache: Arc<Mutex<Option<CachedLibrary>>>,
    reload: Arc<Mutex<()>>,
    external_tokens: Arc<Mutex<HashMap<String, CachedAccessToken>>>,
}

#[derive(Clone)]
struct CachedLibrary {
    library: Arc<Library>,
    cached_at: Instant,
}

#[derive(Clone)]
pub(crate) struct CachedAccessToken {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_at: Instant,
    pub(crate) expires_at_unix_seconds: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskCachedAccessToken {
    access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    expires_at_unix_seconds: u64,
}

impl AppState {
    pub(crate) fn new(options: ApiOptions) -> Self {
        Self {
            options,
            cache: Arc::new(Mutex::new(None)),
            reload: Arc::new(Mutex::new(())),
            external_tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn invalidate_cache(&self) {
        let mut cache = self.cache.lock().await;
        *cache = None;
    }

    pub(crate) async fn cached_access_token(&self, key: &str) -> Option<CachedAccessToken> {
        {
            let tokens = self.external_tokens.lock().await;
            if let Some(token) = tokens
                .get(key)
                .filter(|token| token.expires_at > Instant::now() + Duration::from_secs(60))
                .cloned()
            {
                return Some(token);
            }
        }
        let mut disk_tokens = self.read_disk_tokens().await.ok()?;
        let disk_token = disk_tokens.remove(key)?;
        let Some(token) = cached_token_from_disk(disk_token) else {
            let _ = self.write_disk_tokens(&disk_tokens).await;
            return None;
        };
        self.store_memory_access_token(key, token.clone()).await;
        Some(token)
    }

    pub(crate) async fn store_access_token(
        &self,
        key: &str,
        token: CachedAccessToken,
    ) -> Result<()> {
        self.store_memory_access_token(key, token.clone()).await;
        self.store_disk_access_token(key, &token).await
    }

    pub(crate) async fn invalidate_access_token(&self, key: &str) {
        let mut tokens = self.external_tokens.lock().await;
        tokens.remove(key);
        drop(tokens);
        let _ = self.remove_disk_access_token(key).await;
    }

    async fn store_memory_access_token(&self, key: &str, token: CachedAccessToken) {
        let mut tokens = self.external_tokens.lock().await;
        tokens.insert(key.to_string(), token);
    }

    async fn store_disk_access_token(&self, key: &str, token: &CachedAccessToken) -> Result<()> {
        let mut tokens = self.read_disk_tokens().await.unwrap_or_default();
        tokens.insert(
            key.to_string(),
            DiskCachedAccessToken {
                access_token: token.access_token.clone(),
                refresh_token: token.refresh_token.clone(),
                expires_at_unix_seconds: token.expires_at_unix_seconds,
            },
        );
        self.write_disk_tokens(&tokens).await
    }

    async fn remove_disk_access_token(&self, key: &str) -> Result<()> {
        let mut tokens = self.read_disk_tokens().await.unwrap_or_default();
        tokens.remove(key);
        self.write_disk_tokens(&tokens).await
    }

    async fn read_disk_tokens(&self) -> Result<HashMap<String, DiskCachedAccessToken>> {
        let path = self.token_cache_path();
        let raw = match fs::read_to_string(&path).await {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(HashMap::new());
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to read provider token cache {}", path.display())
                });
            }
        };
        serde_json::from_str(&raw)
            .with_context(|| format!("invalid provider token cache {}", path.display()))
    }

    async fn write_disk_tokens(
        &self,
        tokens: &HashMap<String, DiskCachedAccessToken>,
    ) -> Result<()> {
        let path = self.token_cache_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!(
                    "failed to create token cache directory {}",
                    parent.display()
                )
            })?;
        }
        let raw =
            serde_json::to_string_pretty(tokens).context("failed to serialize token cache")?;
        fs::write(&path, format!("{raw}\n"))
            .await
            .with_context(|| format!("failed to write provider token cache {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).await;
        }
        Ok(())
    }

    fn token_cache_path(&self) -> PathBuf {
        self.options
            .config_path
            .parent()
            .map(|parent| parent.join(".kizunashelf.tokens.json"))
            .unwrap_or_else(|| PathBuf::from(".kizunashelf.tokens.json"))
    }
}

pub(crate) fn unix_seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn cached_token_from_disk(token: DiskCachedAccessToken) -> Option<CachedAccessToken> {
    let now = unix_seconds_now();
    if token.expires_at_unix_seconds <= now + 60 {
        return None;
    }
    Some(CachedAccessToken {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: Instant::now() + Duration::from_secs(token.expires_at_unix_seconds - now),
        expires_at_unix_seconds: token.expires_at_unix_seconds,
    })
}

pub(crate) async fn get_library(state: &AppState) -> Result<Arc<Library>> {
    {
        let cache = state.cache.lock().await;
        if let Some(cached) = cache.as_ref() {
            if cached.cached_at.elapsed() < state.options.cache_ttl {
                return Ok(Arc::clone(&cached.library));
            }
        }
    }

    let _reload = state.reload.lock().await;
    {
        let cache = state.cache.lock().await;
        if let Some(cached) = cache.as_ref() {
            if cached.cached_at.elapsed() < state.options.cache_ttl {
                return Ok(Arc::clone(&cached.library));
            }
        }
    }

    let library = Arc::new(read_library_from_config(&state.options.config_path).await?);
    let mut cache = state.cache.lock().await;
    *cache = Some(CachedLibrary {
        library: Arc::clone(&library),
        cached_at: Instant::now(),
    });
    Ok(library)
}

pub(crate) fn content_writes_enabled(state: &AppState, library: &Library) -> bool {
    state.options.content_writable && library.config.content_writable.unwrap_or(true)
}
