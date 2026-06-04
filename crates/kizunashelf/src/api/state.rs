use crate::library::read_library_from_config;
use crate::types::Library;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ApiOptions {
    pub config_path: PathBuf,
    pub cache_ttl: Duration,
    pub web_dist_path: Option<PathBuf>,
    pub settings_writable: bool,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) options: ApiOptions,
    cache: Arc<Mutex<Option<CachedLibrary>>>,
    reload: Arc<Mutex<()>>,
}

#[derive(Clone)]
struct CachedLibrary {
    library: Arc<Library>,
    cached_at: Instant,
}

impl AppState {
    pub(crate) fn new(options: ApiOptions) -> Self {
        Self {
            options,
            cache: Arc::new(Mutex::new(None)),
            reload: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) async fn invalidate_cache(&self) {
        let mut cache = self.cache.lock().await;
        *cache = None;
    }
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
