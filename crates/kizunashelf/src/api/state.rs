use super::error::ApiError;
use crate::contract::{AnalyticsResponse, AssetDownloadJob};
use crate::library::{
    compute_listing_fingerprint, load_vault_config_via_vfs, read_library, read_library_cached,
    read_raw_vault_config_via_vfs, IndexCacheContext,
};
use crate::secrets::{SecretStore, SECRET_PROVIDER_TOKENS};
use crate::types::{AppConfig, KizunaConfig, Library};
use crate::vfs::{NativeVfs, Vfs};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// Maximum number of finished asset-download jobs kept in memory.
const MAX_RETAINED_JOBS: usize = 20;

/// In-memory record for a batch asset-download job. Jobs do not survive a
/// restart by design; the user simply re-runs and already-downloaded images are
/// skipped.
pub(crate) struct AssetJobRecord {
    pub(crate) job: AssetDownloadJob,
    pub(crate) cancel: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct ApiOptions {
    pub config_path: PathBuf,
    pub cache_ttl: Duration,
    pub web_dist_path: Option<PathBuf>,
    pub settings_writable: bool,
    pub content_writable: bool,
    /// Host directory for the persistent index cache (NOT inside the vault — a
    /// real app-container path the in-process core can touch with `std::fs`).
    /// `None` disables the cache (cold starts re-parse the whole vault). It is a
    /// pure optimization; see [`crate::library`]'s index cache.
    pub index_cache_dir: Option<PathBuf>,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) options: ApiOptions,
    /// Injected vault filesystem (iOS). When `None`, a [`NativeVfs`] is built per
    /// load from the configured vault root (desktop/web).
    vault_fs: Option<Arc<dyn Vfs>>,
    /// Inline app config (the vault root + write mode). Every runtime owns this
    /// server-side and passes it in: env vars (web), the native vault switcher
    /// (desktop), or `@AppStorage` (iOS). There is no app config file.
    app_config: AppConfig,
    /// Provider credentials + token cache. Desktop uses env vars + a file; iOS
    /// uses the Keychain via an injected store.
    secret_store: Arc<dyn SecretStore>,
    cache: Arc<Mutex<Option<CachedLibrary>>>,
    reload: Arc<Mutex<()>>,
    external_tokens: Arc<Mutex<HashMap<String, CachedAccessToken>>>,
    /// Per-provider locks that single-flight token acquisition so a cold cache
    /// under concurrent searches does not stampede the upstream token endpoint.
    token_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    /// Serializes the read-modify-write of the *shared* on-disk token blob across
    /// providers. `token_locks` is per provider, so it does not stop provider A
    /// and provider B from both reading the same blob and the second write
    /// clobbering the first's token (a lost update that forces a needless re-auth
    /// after restart). Every disk-token mutation holds this lock around its whole
    /// read→modify→write.
    token_disk_lock: Arc<Mutex<()>>,
    /// Memoized analytics keyed on the library's `content_revision`. Analytics is
    /// an expensive whole-library scan; the cache turns repeated `/analytics` hits
    /// into a single build per distinct library content. Keying on the
    /// content fingerprint (not `generated_at`) means a warm reload that re-reads
    /// identical content keeps the memoized result instead of rebuilding it.
    analytics_cache: Arc<Mutex<Option<CachedAnalytics>>>,
    /// Single-flights the analytics build so a cold memo under concurrent
    /// `/analytics` hits does the expensive whole-library scan once, not once per
    /// request. Callers take it, re-check the memo, and only then build (mirrors
    /// the [`get_library`] reload lock and [`token_fetch_lock`]).
    analytics_build_lock: Arc<Mutex<()>>,
    http_client: reqwest::Client,
    asset_jobs: Arc<Mutex<HashMap<String, AssetJobRecord>>>,
    asset_job_counter: Arc<AtomicU64>,
}

struct CachedAnalytics {
    content_revision: String,
    response: Arc<AnalyticsResponse>,
}

#[derive(Clone)]
struct CachedLibrary {
    library: Arc<Library>,
    cached_at: Instant,
    /// Fingerprint of the vault file listing this library was built from (schema +
    /// every entity/daily-note file's `(path, len, mtime)`). On a reload, an
    /// unchanged fingerprint means the library is still valid and can be reused
    /// without re-reading the vault. `None` disables that fast path for the next
    /// reload (the listing couldn't be fingerprinted — e.g. a backend that can't
    /// report mtimes, or an unreadable config).
    listing_fingerprint: Option<u64>,
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
    /// Builds state with an inline app config, an optional injected vault
    /// filesystem (iOS) or a [`NativeVfs`] derived from the vault root
    /// (web/desktop), and a secret store. See ../kizunashelf-ios/docs/ios-port-plan.md §5/§7.
    pub(crate) fn with_vault(
        options: ApiOptions,
        vault_fs: Option<Arc<dyn Vfs>>,
        app_config: AppConfig,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("KizunaShelf/", env!("CARGO_PKG_VERSION")))
            // Asset downloads follow redirects manually (see assets.rs) so each
            // hop's destination can be re-validated against the SSRF guard; never
            // let reqwest follow a redirect into an unvalidated host.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            options,
            vault_fs,
            app_config,
            secret_store,
            cache: Arc::new(Mutex::new(None)),
            reload: Arc::new(Mutex::new(())),
            external_tokens: Arc::new(Mutex::new(HashMap::new())),
            token_locks: Arc::new(Mutex::new(HashMap::new())),
            token_disk_lock: Arc::new(Mutex::new(())),
            analytics_cache: Arc::new(Mutex::new(None)),
            analytics_build_lock: Arc::new(Mutex::new(())),
            http_client,
            asset_jobs: Arc::new(Mutex::new(HashMap::new())),
            asset_job_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Shared HTTP client (connection-pooled) for outbound requests such as
    /// asset downloads.
    pub(crate) fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Returns the vault filesystem for the given vault root. This is the single
    /// injection seam for the iOS port: desktop/web use [`NativeVfs`]; iOS will
    /// return a Swift-backed VFS (security-scoped bookmark + `NSFileCoordinator`).
    /// See ../kizunashelf-ios/docs/ios-port-plan.md §5.
    pub(crate) fn vault_vfs(&self, vault_root: &str) -> Arc<dyn Vfs> {
        match &self.vault_fs {
            Some(vfs) => Arc::clone(vfs),
            None => Arc::new(NativeVfs::new(vault_root)),
        }
    }

    pub(crate) fn asset_jobs(&self) -> &Arc<Mutex<HashMap<String, AssetJobRecord>>> {
        &self.asset_jobs
    }

    pub(crate) fn next_asset_job_id(&self) -> String {
        let counter = self.asset_job_counter.fetch_add(1, Ordering::Relaxed);
        format!("job-{}-{}", unix_seconds_now(), counter)
    }

    /// Inserts a new job, pruning the oldest finished jobs beyond the retention
    /// limit.
    pub(crate) async fn insert_asset_job(&self, record: AssetJobRecord) {
        use crate::contract::AssetDownloadJobStatus;
        let mut jobs = self.asset_jobs.lock().await;
        jobs.insert(record.job.id.clone(), record);
        if jobs.len() > MAX_RETAINED_JOBS {
            let mut finished: Vec<(String, String)> = jobs
                .values()
                .filter(|record| {
                    matches!(
                        record.job.status,
                        AssetDownloadJobStatus::Completed | AssetDownloadJobStatus::Cancelled
                    )
                })
                .map(|record| (record.job.id.clone(), record.job.started_at.clone()))
                .collect();
            finished.sort_by(|a, b| a.1.cmp(&b.1));
            let remove = jobs.len().saturating_sub(MAX_RETAINED_JOBS);
            for (id, _) in finished.into_iter().take(remove) {
                jobs.remove(&id);
            }
        }
    }

    /// Applies `update` to the stored job, if it still exists.
    pub(crate) async fn update_asset_job(
        &self,
        job_id: &str,
        update: impl FnOnce(&mut AssetDownloadJob),
    ) {
        let mut jobs = self.asset_jobs.lock().await;
        if let Some(record) = jobs.get_mut(job_id) {
            update(&mut record.job);
        }
    }

    pub(crate) async fn invalidate_cache(&self) {
        let mut cache = self.cache.lock().await;
        *cache = None;
    }

    /// Returns the per-provider lock used to single-flight token acquisition.
    /// Callers acquire it, re-check the token cache, and only then fetch.
    pub(crate) async fn token_fetch_lock(&self, key: &str) -> Arc<Mutex<()>> {
        let mut locks = self.token_locks.lock().await;
        Arc::clone(
            locks
                .entry(key.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    /// Returns memoized analytics if it was built for this `content_revision`.
    pub(crate) async fn cached_analytics(
        &self,
        content_revision: &str,
    ) -> Option<Arc<AnalyticsResponse>> {
        let cache = self.analytics_cache.lock().await;
        cache
            .as_ref()
            .filter(|cached| cached.content_revision == content_revision)
            .map(|cached| Arc::clone(&cached.response))
    }

    /// The lock that single-flights the analytics build. Callers acquire it,
    /// re-check [`cached_analytics`], and only then build + [`store_analytics`].
    pub(crate) fn analytics_build_lock(&self) -> &Arc<Mutex<()>> {
        &self.analytics_build_lock
    }

    /// Stores analytics keyed on the library content it was built from.
    pub(crate) async fn store_analytics(
        &self,
        content_revision: &str,
        response: Arc<AnalyticsResponse>,
    ) {
        let mut cache = self.analytics_cache.lock().await;
        *cache = Some(CachedAnalytics {
            content_revision: content_revision.to_string(),
            response,
        });
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
        let token = {
            // Hold the disk lock across the read and the prune-write so a
            // concurrent store for another provider can't be lost.
            let _disk = self.token_disk_lock.lock().await;
            let mut disk_tokens = self.read_disk_tokens().ok()?;
            let disk_token = disk_tokens.remove(key)?;
            match cached_token_from_disk(disk_token) {
                Some(token) => token,
                None => {
                    let _ = self.write_disk_tokens(&disk_tokens);
                    return None;
                }
            }
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
        // Hold the disk lock across read→modify→write so a concurrent mutation for
        // another provider can't clobber this token in the shared blob.
        let _disk = self.token_disk_lock.lock().await;
        let mut tokens = self.read_disk_tokens().unwrap_or_default();
        tokens.insert(
            key.to_string(),
            DiskCachedAccessToken {
                access_token: token.access_token.clone(),
                refresh_token: token.refresh_token.clone(),
                expires_at_unix_seconds: token.expires_at_unix_seconds,
            },
        );
        self.write_disk_tokens(&tokens)
    }

    async fn remove_disk_access_token(&self, key: &str) -> Result<()> {
        let _disk = self.token_disk_lock.lock().await;
        let mut tokens = self.read_disk_tokens().unwrap_or_default();
        tokens.remove(key);
        self.write_disk_tokens(&tokens)
    }

    /// The cached provider tokens, read from the secret store (Keychain on iOS, a
    /// `0600` JSON file on desktop). A missing/unset value is an empty map.
    fn read_disk_tokens(&self) -> Result<HashMap<String, DiskCachedAccessToken>> {
        match self.secret_store.get(SECRET_PROVIDER_TOKENS) {
            Some(raw) => {
                serde_json::from_str(&raw).context("invalid provider token cache in secret store")
            }
            None => Ok(HashMap::new()),
        }
    }

    fn write_disk_tokens(&self, tokens: &HashMap<String, DiskCachedAccessToken>) -> Result<()> {
        let raw =
            serde_json::to_string_pretty(tokens).context("failed to serialize token cache")?;
        self.secret_store.set(SECRET_PROVIDER_TOKENS, &raw)
    }

    /// The secret store, used by external providers to read their credentials.
    pub(crate) fn secret_store(&self) -> &Arc<dyn SecretStore> {
        &self.secret_store
    }

    /// The inline app config (vault root + write mode), owned by the runtime.
    /// Unlike [`get_library`] it does not require a vault config to exist, so it
    /// is usable before a schema is created.
    pub(crate) fn app_config(&self) -> AppConfig {
        self.app_config.clone()
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
    // Re-check: a concurrent reload may have just refreshed the cache.
    let previous = {
        let cache = state.cache.lock().await;
        match cache.as_ref() {
            Some(cached) if cached.cached_at.elapsed() < state.options.cache_ttl => {
                return Ok(Arc::clone(&cached.library));
            }
            Some(cached) => cached
                .listing_fingerprint
                .map(|fingerprint| (Arc::clone(&cached.library), fingerprint)),
            None => None,
        }
    };

    // Fast path: the cache is stale, but if the vault's file listing (schema +
    // every entity/daily-note file's size+mtime) is unchanged since the cached
    // library was built, that library is still valid. Reuse it, skipping the whole
    // read + parse + relation rebuild — the directory listing is the only cost.
    if let Some((library, fingerprint)) = previous {
        let vfs = state.vault_vfs(&library.config.vault_root);
        if compute_listing_fingerprint(&library.config, vfs.as_ref()).await == Some(fingerprint) {
            let mut cache = state.cache.lock().await;
            *cache = Some(CachedLibrary {
                library: Arc::clone(&library),
                cached_at: Instant::now(),
                listing_fingerprint: Some(fingerprint),
            });
            return Ok(library);
        }
    }

    let library = Arc::new(load_library(state).await?);
    let listing_fingerprint = {
        let vfs = state.vault_vfs(&library.config.vault_root);
        compute_listing_fingerprint(&library.config, vfs.as_ref()).await
    };
    let mut cache = state.cache.lock().await;
    *cache = Some(CachedLibrary {
        library: Arc::clone(&library),
        cached_at: Instant::now(),
        listing_fingerprint,
    });
    Ok(library)
}

/// Loads the library from the current configuration. The app config is the
/// inline value; the vault config and all entities are read through the vault
/// filesystem (a [`NativeVfs`] rooted at the vault root, or the injected iOS VFS).
async fn load_library(state: &AppState) -> Result<Library> {
    let app = state.app_config.clone();
    // Desktop derives the vault filesystem from the (absolute) vault root; iOS
    // injects one and the root is just a display label.
    if state.vault_fs.is_none() && app.vault_root.trim().is_empty() {
        anyhow::bail!("config does not set a vault root; run onboarding to create one");
    }
    let vfs = state.vault_vfs(&app.vault_root);
    let vault = load_vault_config_via_vfs(vfs.as_ref()).await?;
    let config = KizunaConfig::from_parts(app, vault);
    match build_index_cache_context(state, vfs.as_ref(), &config).await {
        Some(cache) => read_library_cached(config, vfs, cache).await,
        None => read_library(config, vfs).await,
    }
}

/// Builds the index-cache context when a cache dir is configured. The schema
/// fingerprint comes from the raw vault config text (so any schema edit busts the
/// whole cache); the vault root is the per-vault identity. Returns `None` — i.e.
/// the uncached path — when no cache dir is set or the raw config can't be read.
async fn build_index_cache_context(
    state: &AppState,
    vfs: &dyn Vfs,
    config: &KizunaConfig,
) -> Option<IndexCacheContext> {
    let dir = state.options.index_cache_dir.clone()?;
    let raw = read_raw_vault_config_via_vfs(vfs).await.ok().flatten()?;
    Some(IndexCacheContext::new(dir, &raw, &config.vault_root))
}

pub(crate) fn content_writes_enabled(state: &AppState, library: &Library) -> bool {
    state.options.content_writable && library.config.content_writable.unwrap_or(true)
}

/// Loads the library and rejects the request with 403 when content writes are
/// disabled. The single entry point every mutation/asset-download handler uses to
/// gate writes, so the read-only check (and its error) lives in one place.
pub(crate) async fn require_content_writes(state: &AppState) -> Result<Arc<Library>, ApiError> {
    let library = get_library(state).await?;
    if !content_writes_enabled(state, &library) {
        return Err(ApiError::forbidden("Content writes are disabled"));
    }
    Ok(library)
}
