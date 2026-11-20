//! The resident library: a cached, generation-guarded snapshot of the indexed
//! vault, reloaded on demand.

use super::AppState;
use crate::library::{
    compute_listing_fingerprint, load_vault_config_with_raw_via_vfs, read_library_with_fingerprint,
    IndexCacheContext, MemoryIndexCache,
};
use crate::types::{KizunaConfig, Library};
use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// The cached library plus everything that keeps it coherent.
pub(super) struct LibraryCache {
    entry: Mutex<Option<CachedLibrary>>,
    /// Monotonic generation bumped before every cache invalidation. A library
    /// reload only publishes its result when the generation is unchanged across
    /// the load, so a write landing mid-reload cannot be hidden by that older
    /// reload storing stale state afterward. Shared across routers that share a
    /// host vault identity, so one router's write invalidates the others.
    generation: Arc<AtomicU64>,
    /// Single-flights reloads: concurrent misses wait for one load.
    reload: Mutex<()>,
    /// Process-resident per-file index cache, used when no persistent
    /// `index_cache_dir` is configured (e.g. the web server default) so a changed
    /// reload re-parses only changed files instead of the whole vault. Lost on
    /// restart. A plain `std::sync::Mutex`: it is only held for the brief
    /// clone/replace inside the cache load/save, never across an `.await`.
    index_cache_memory: Arc<std::sync::Mutex<MemoryIndexCache>>,
}

#[derive(Clone)]
struct CachedLibrary {
    library: Arc<Library>,
    cached_at: Instant,
    generation: u64,
    /// Fingerprint of the vault file listing this library was built from (schema +
    /// every entity/daily-note file's `(path, len, mtime)`). On a reload, an
    /// unchanged fingerprint means the library is still valid and can be reused
    /// without re-reading the vault. `None` disables that fast path for the next
    /// reload (the listing couldn't be fingerprinted — e.g. a backend that can't
    /// report mtimes, or an unreadable config).
    listing_fingerprint: Option<u64>,
}

impl LibraryCache {
    pub(super) fn new(generation: Arc<AtomicU64>) -> Self {
        Self {
            entry: Mutex::new(None),
            generation,
            reload: Mutex::new(()),
            index_cache_memory: Arc::new(std::sync::Mutex::new(MemoryIndexCache::default())),
        }
    }

    /// Drops the cached library so the next read reloads, and bumps the
    /// generation so any reload already in flight won't publish stale state.
    pub(super) async fn invalidate(&self) {
        let mut entry = self.entry.lock().await;
        self.generation.fetch_add(1, Ordering::AcqRel);
        *entry = None;
    }

    /// The cached library if it belongs to `generation` and is within the TTL.
    async fn fresh(&self, generation: u64, state: &AppState) -> Option<Arc<Library>> {
        let entry = self.entry.lock().await;
        entry
            .as_ref()
            .filter(|cached| {
                cached.generation == generation
                    && cached.cached_at.elapsed() < state.options.cache_ttl
            })
            .map(|cached| Arc::clone(&cached.library))
    }

    async fn publish(
        &self,
        library: &Arc<Library>,
        listing_fingerprint: Option<u64>,
        generation: u64,
    ) {
        *self.entry.lock().await = Some(CachedLibrary {
            library: Arc::clone(library),
            cached_at: Instant::now(),
            listing_fingerprint,
            generation,
        });
    }
}

pub(crate) async fn get_library(state: &AppState) -> Result<Arc<Library>> {
    state.ensure_vault_watch().await;
    let cache = &state.library;
    let generation = cache.generation.load(Ordering::Acquire);
    if let Some(library) = cache.fresh(generation, state).await {
        return Ok(library);
    }

    let _reload = cache.reload.lock().await;
    let generation = cache.generation.load(Ordering::Acquire);
    // Re-check: a concurrent reload may have just refreshed the cache.
    if let Some(library) = cache.fresh(generation, state).await {
        return Ok(library);
    }

    let (library, _) = revalidate_locked(state, generation).await?;
    Ok(library)
}

/// Checks whether the cached library still matches the vault's current listing
/// fingerprint, reusing it without re-indexing when unchanged and reloading
/// otherwise. Returns `(library, changed)` where `changed` is `false` whenever
/// the cached library (and its `generated_at` stamp) was kept unchanged.
pub(crate) async fn revalidate_library(state: &AppState) -> Result<(Arc<Library>, bool)> {
    state.ensure_vault_watch().await;
    let _reload = state.library.reload.lock().await;
    let generation = state.library.generation.load(Ordering::Acquire);
    revalidate_locked(state, generation).await
}

async fn revalidate_locked(state: &AppState, generation: u64) -> Result<(Arc<Library>, bool)> {
    let cache = &state.library;
    let previous = {
        let entry = cache.entry.lock().await;
        match entry.as_ref() {
            Some(cached) if cached.generation == generation => {
                Some((Arc::clone(&cached.library), cached.listing_fingerprint))
            }
            _ => None,
        }
    };

    // Fast path: if the vault's file listing (schema + every entity/daily-note
    // file's size+mtime) is unchanged since the cached library was built, that
    // library is still valid. Reuse it, skipping the whole read + parse +
    // relation rebuild — the directory listing is the only cost.
    if let Some((library, Some(fingerprint))) = &previous {
        let vfs = state.vault_vfs(&library.config.vault_root);
        if compute_listing_fingerprint(&library.config, vfs.as_ref()).await == Some(*fingerprint) {
            tracing::debug!("vault listing unchanged; reusing the cached library");
            cache.publish(library, Some(*fingerprint), generation).await;
            return Ok((Arc::clone(library), false));
        }
    }

    // The dominant cost in the whole app, and the one an operator most needs a
    // number for: "is my vault slow to index, and is the index cache helping?"
    // is unanswerable without timing the reload that actually reads the vault.
    let load_started = Instant::now();
    let (loaded, listing_fingerprint) = load_library(state).await?;
    tracing::info!(
        entities = loaded.records.len(),
        relations = loaded.relations.len(),
        diagnostics = loaded.diagnostics.len(),
        elapsed_ms = load_started.elapsed().as_millis(),
        "reloaded the library from the vault",
    );

    let (library, changed) = match previous {
        Some((prev, _))
            if prev.content_revision == loaded.content_revision
                && prev.diagnostics == loaded.diagnostics =>
        {
            (prev, false)
        }
        _ => (Arc::new(loaded), true),
    };

    cache
        .publish(&library, listing_fingerprint, generation)
        .await;
    Ok((library, changed))
}

/// Loads the library from the current configuration, returning both the built
/// [`Library`] and the listing fingerprint computed during the same directory
/// pass. The app config is the inline value; the vault config and all entities
/// are read through the vault filesystem (a [`crate::vfs::NativeVfs`] rooted at
/// the vault root, or the injected iOS VFS).
async fn load_library(state: &AppState) -> Result<(Library, Option<u64>)> {
    let app = state.app_config();
    // Desktop derives the vault filesystem from the (absolute) vault root; iOS
    // injects one and the root is just a display label.
    if state.native_vault && app.vault_root.trim().is_empty() {
        anyhow::bail!("config does not set a vault root; run onboarding to create one");
    }
    let vfs = state.vault_vfs(&app.vault_root);
    let (vault, raw_config) = load_vault_config_with_raw_via_vfs(vfs.as_ref()).await?;
    let config = KizunaConfig::from_parts(app, vault);
    let index_cache = build_index_cache_context(state, &raw_config, &config);
    read_library_with_fingerprint(config, &raw_config, vfs, Some(index_cache)).await
}

/// Builds the index-cache context from the already-read raw vault config text.
/// Always keeps the process-resident L1 cache warm; when `index_cache_dir` is
/// configured, layers the persistent disk cache underneath it so cold starts
/// reuse prior parses while warm reloads skip disk I/O.
fn build_index_cache_context(
    state: &AppState,
    raw_config: &str,
    config: &KizunaConfig,
) -> IndexCacheContext {
    let vault_identity = state
        .options
        .index_cache_identity
        .as_deref()
        .unwrap_or(&config.vault_root);
    let memory = Arc::clone(&state.library.index_cache_memory);
    match state.options.index_cache_dir.clone() {
        Some(dir) => {
            IndexCacheContext::with_memory_and_disk(memory, dir, raw_config, vault_identity)
        }
        None => IndexCacheContext::memory(memory, raw_config, vault_identity),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{test_state, test_state_with_identity, NOTE_CONFIG};
    use super::*;
    use crate::vfs::InMemoryVfs;

    #[tokio::test]
    async fn host_identity_invalidates_other_router_without_a_watcher() {
        let vfs = Arc::new(InMemoryVfs::new());
        vfs.insert_file("KizunaShelf/config.yaml", NOTE_CONFIG);
        let path = "Taxonomy/Notes/Example.md";
        vfs.insert_file(path, "---\ntitle: Before\n---\n");
        let identity = Some("host-cache-test".to_string());
        let front = test_state_with_identity(Arc::clone(&vfs), identity.clone());
        let back = test_state_with_identity(Arc::clone(&vfs), identity);
        let unrelated =
            test_state_with_identity(Arc::clone(&vfs), Some("host-cache-unrelated".to_string()));
        for state in [&front, &back, &unrelated] {
            assert_eq!(
                get_library(state).await.unwrap().records[0].frontmatter["title"],
                "Before"
            );
        }
        vfs.insert_file(path, "---\ntitle: After\n---\n");
        back.invalidate_cache().await;
        assert_eq!(
            get_library(&front).await.unwrap().records[0].frontmatter["title"],
            "After"
        );
        assert_eq!(
            get_library(&unrelated).await.unwrap().records[0].frontmatter["title"],
            "Before"
        );
    }

    #[tokio::test]
    async fn stale_reload_publication_is_ignored_after_invalidation() {
        let vfs = Arc::new(InMemoryVfs::new());
        vfs.insert_file("KizunaShelf/config.yaml", NOTE_CONFIG);
        let entity_path = "Taxonomy/Notes/Example.md";
        vfs.insert_file(entity_path, "---\ntitle: Before\n---\n");
        let state = test_state(Arc::clone(&vfs));

        let before = get_library(&state).await.unwrap();
        assert_eq!(before.records[0].frontmatter["title"], "Before");
        let stale_generation = state.library.generation.load(Ordering::Acquire);

        vfs.insert_file(entity_path, "---\ntitle: After\n---\n");
        state.invalidate_cache().await;

        // Simulate an older reload finishing after the invalidation. Its cached
        // library carries the generation from when that reload began.
        state.library.publish(&before, None, stale_generation).await;

        let after = get_library(&state).await.unwrap();
        assert_eq!(after.records[0].frontmatter["title"], "After");
    }

    #[tokio::test]
    async fn revalidate_library_reuses_cache_when_unchanged_and_reloads_on_edit() {
        let vfs = Arc::new(InMemoryVfs::new());
        vfs.insert_file("KizunaShelf/config.yaml", NOTE_CONFIG);
        let path = "Taxonomy/Notes/Example.md";
        vfs.insert_file(path, "---\ntitle: Before\n---\n");
        let state = test_state(Arc::clone(&vfs));

        let initial = get_library(&state).await.unwrap();
        let (reused, changed) = revalidate_library(&state).await.unwrap();
        assert!(!changed);
        assert_eq!(reused.generated_at, initial.generated_at);

        vfs.insert_file(path, "---\ntitle: After\n---\n");
        let (reloaded, changed) = revalidate_library(&state).await.unwrap();
        assert!(changed);
        assert_eq!(reloaded.records[0].frontmatter["title"], "After");
    }
}
