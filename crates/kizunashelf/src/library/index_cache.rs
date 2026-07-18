//! Persistent index cache for cold-start library loads.
//!
//! Reading + parsing every entity (and every daily note) is the dominant cost of
//! a cold start, especially on iOS where each vault read crosses the FFI tunnel
//! into a security-scoped, file-coordinated Swift VFS. This cache lets a cold
//! start reuse the previously extracted result for any file whose cheap
//! directory-listing fingerprint (size + mtime, from the same `read_dir` pass —
//! *no* content read) is unchanged, re-reading only the files that actually
//! changed. Entities cache their parsed [`CachedEntry`]; daily notes cache only
//! their wikilink targets ([`CachedDailyNote`]), never the body.
//!
//! **The cache is a pure optimization and never authoritative.** Any doubt — a
//! missing/corrupt blob, a version/schema/vault mismatch, a `0` mtime, a
//! size/mtime difference — falls back to re-reading and re-parsing the file. It
//! lives *outside* the vault (a host app-container path), so it never pollutes
//! the Markdown vault, never syncs to Obsidian/iCloud, and is freely deletable.
//!
//! Correctness gates, in order:
//! - **Engine version** (crate version + [`CACHE_LOGIC_VERSION`]) — every app
//!   upgrade rebuilds, so a change to derivation logic can never serve stale
//!   derived values.
//! - **Schema fingerprint** (hash of the raw vault config) — all derivation flows
//!   schema → behavior, so a schema edit invalidates every cached record even
//!   when file contents are untouched.
//! - **Vault identity** — distinguishes vaults that share one cache dir.
//! - **Per-file `(size, mtime)`** — equality only (never ordering), so clock skew
//!   or a vault copy that resets mtimes simply triggers a one-time rebuild.
//!
//! Relations and `relation_count` are deliberately *not* cached: they are
//! cross-entity, derived after parsing, and recomputed in-memory on every load.

use crate::types::{EntityRecord, LibraryDiagnostic};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Structural format of the on-disk blob. A mismatch is rejected outright.
const CACHE_FORMAT: u32 = 1;

/// Bump when the *meaning* of a cached parse result changes without the crate
/// version changing (e.g. a derivation tweak shipped in the same version during
/// development). Folded into the engine version below.
const CACHE_LOGIC_VERSION: u32 = 6;

/// One cached per-file parse result: the slim resident [`EntityRecord`] (which
/// itself carries the body wikilinks) plus the diagnostics the load pass
/// extracts. Stored keyed by the file's NFC-normalized vault-relative path,
/// alongside its `(len, modified_unix_nanos)` fingerprint.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct CachedEntry {
    pub(super) len: u64,
    pub(super) modified_unix_nanos: u128,
    pub(super) record: EntityRecord,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) diagnostics: Vec<LibraryDiagnostic>,
}

/// One cached daily note: just its body's wikilink targets (NOT the body), keyed
/// by NFC path, alongside the `(len, modified_unix_nanos)` fingerprint. Reading
/// daily-note bodies is a cold-start cost for vaults with many notes; caching the
/// extracted links lets an unchanged note skip the read.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct CachedDailyNote {
    pub(super) len: u64,
    pub(super) modified_unix_nanos: u128,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) links: Vec<String>,
}

/// The cache as loaded into memory: entity entries + daily-note entries, each
/// keyed by NFC path.
#[derive(Default)]
pub(super) struct LoadedCache {
    pub(super) entries: HashMap<String, CachedEntry>,
    pub(super) daily_notes: HashMap<String, CachedDailyNote>,
}

/// The serialized blob: a header of global gates plus the per-file entries.
#[derive(Serialize, Deserialize)]
struct IndexCacheFile {
    cache_format: u32,
    engine_version: String,
    schema_fingerprint: String,
    vault_identity: String,
    entries: BTreeMap<String, CachedEntry>,
    #[serde(default)]
    daily_notes: BTreeMap<String, CachedDailyNote>,
}

/// Whether a listed file's current `(len, mtime)` matches a cached fingerprint
/// and may reuse the cached result. A `0` mtime — a backend that can't report one
/// during listing — is never a hit, so such a backend stays correct, just
/// uncached.
pub(super) fn fingerprint_hit(
    cached_len: u64,
    cached_modified_unix_nanos: u128,
    len: u64,
    modified_unix_nanos: u128,
) -> bool {
    modified_unix_nanos != 0
        && cached_modified_unix_nanos == modified_unix_nanos
        && cached_len == len
}

fn engine_version() -> String {
    format!("{}+{}", env!("CARGO_PKG_VERSION"), CACHE_LOGIC_VERSION)
}

fn hash_str(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Process-resident index cache, shared across library loads. Used when no
/// persistent cache dir is configured (e.g. the web server's default): a changed
/// reload re-parses only the files that changed instead of the whole vault. It is
/// lost on restart, and gated by the same schema/vault identity as the on-disk
/// form (engine version is implicit — it's the same running process).
#[derive(Default)]
pub(crate) struct MemoryIndexCache {
    schema_fingerprint: String,
    vault_identity: String,
    entries: HashMap<String, CachedEntry>,
    daily_notes: HashMap<String, CachedDailyNote>,
}

/// Where an [`IndexCacheContext`] reads/writes its per-file entries.
enum IndexCacheStore {
    /// A persistent JSON blob outside the vault (survives restarts).
    Disk(PathBuf),
    /// A process-resident cache (lost on restart, no disk needed).
    Memory(Arc<Mutex<MemoryIndexCache>>),
}

/// Context for using the index cache during a single library load. Backed by
/// either a persistent file or a process-resident store; both apply the same
/// schema/vault gates so a schema edit or vault switch rebuilds.
pub(crate) struct IndexCacheContext {
    store: IndexCacheStore,
    schema_fingerprint: String,
    vault_identity: String,
}

impl IndexCacheContext {
    /// Builds a context for the host cache directory `dir`. `raw_vault_config` is
    /// the verbatim config text whose hash gates the whole cache;
    /// `vault_identity` (e.g. the vault root) distinguishes vaults that share one
    /// cache dir and is folded into the cache filename.
    pub(crate) fn new(dir: PathBuf, raw_vault_config: &str, vault_identity: &str) -> Self {
        let identity = hash_str(vault_identity);
        let path = dir.join(format!("index-{identity}.json"));
        Self {
            store: IndexCacheStore::Disk(path),
            schema_fingerprint: hash_str(raw_vault_config),
            vault_identity: identity,
        }
    }

    /// Builds a context backed by a process-resident [`MemoryIndexCache`] for
    /// runtimes with no persistent cache dir.
    pub(crate) fn memory(
        store: Arc<Mutex<MemoryIndexCache>>,
        raw_vault_config: &str,
        vault_identity: &str,
    ) -> Self {
        Self {
            store: IndexCacheStore::Memory(store),
            schema_fingerprint: hash_str(raw_vault_config),
            vault_identity: hash_str(vault_identity),
        }
    }

    /// Loads the cached entity + daily-note entries, returning an empty cache
    /// whenever the source is absent, unreadable, structurally stale, or fails any
    /// gate. Never returns data we aren't certain matches the current
    /// engine + schema + vault.
    pub(super) fn load(&self) -> LoadedCache {
        match &self.store {
            IndexCacheStore::Disk(path) => self.load_disk(path),
            IndexCacheStore::Memory(store) => self.load_memory(store),
        }
    }

    fn load_disk(&self, path: &Path) -> LoadedCache {
        let Ok(bytes) = std::fs::read(path) else {
            return LoadedCache::default();
        };
        let Ok(file) = serde_json::from_slice::<IndexCacheFile>(&bytes) else {
            return LoadedCache::default();
        };
        if file.cache_format != CACHE_FORMAT
            || file.engine_version != engine_version()
            || file.schema_fingerprint != self.schema_fingerprint
            || file.vault_identity != self.vault_identity
        {
            return LoadedCache::default();
        }
        LoadedCache {
            entries: file.entries.into_iter().collect(),
            daily_notes: file.daily_notes.into_iter().collect(),
        }
    }

    fn load_memory(&self, store: &Mutex<MemoryIndexCache>) -> LoadedCache {
        let Ok(cache) = store.lock() else {
            return LoadedCache::default();
        };
        if cache.schema_fingerprint != self.schema_fingerprint
            || cache.vault_identity != self.vault_identity
        {
            return LoadedCache::default();
        }
        LoadedCache {
            entries: cache.entries.clone(),
            daily_notes: cache.daily_notes.clone(),
        }
    }

    /// Persists the per-file entries. The disk form writes atomically (temp +
    /// rename); the memory form replaces the resident map. Failures are swallowed
    /// — a cache we couldn't write just means the next load is cold.
    pub(super) fn save(
        &self,
        entries: BTreeMap<String, CachedEntry>,
        daily_notes: BTreeMap<String, CachedDailyNote>,
    ) {
        match &self.store {
            IndexCacheStore::Disk(path) => self.save_disk(path, entries, daily_notes),
            IndexCacheStore::Memory(store) => self.save_memory(store, entries, daily_notes),
        }
    }

    fn save_disk(
        &self,
        path: &Path,
        entries: BTreeMap<String, CachedEntry>,
        daily_notes: BTreeMap<String, CachedDailyNote>,
    ) {
        let file = IndexCacheFile {
            cache_format: CACHE_FORMAT,
            engine_version: engine_version(),
            schema_fingerprint: self.schema_fingerprint.clone(),
            vault_identity: self.vault_identity.clone(),
            entries,
            daily_notes,
        };
        let Ok(serialized) = serde_json::to_vec(&file) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &serialized).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    fn save_memory(
        &self,
        store: &Mutex<MemoryIndexCache>,
        entries: BTreeMap<String, CachedEntry>,
        daily_notes: BTreeMap<String, CachedDailyNote>,
    ) {
        if let Ok(mut cache) = store.lock() {
            *cache = MemoryIndexCache {
                schema_fingerprint: self.schema_fingerprint.clone(),
                vault_identity: self.vault_identity.clone(),
                entries: entries.into_iter().collect(),
                daily_notes: daily_notes.into_iter().collect(),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk_path(context: &IndexCacheContext) -> &Path {
        match &context.store {
            IndexCacheStore::Disk(path) => path,
            IndexCacheStore::Memory(_) => panic!("expected disk cache"),
        }
    }

    #[test]
    fn stable_vault_identity_separates_same_named_vault_caches() {
        let root = PathBuf::from("cache");
        let first = IndexCacheContext::new(root.clone(), "same schema", "vault-uuid-a");
        let second = IndexCacheContext::new(root.clone(), "same schema", "vault-uuid-b");
        let reopened = IndexCacheContext::new(root, "same schema", "vault-uuid-a");

        assert_ne!(disk_path(&first), disk_path(&second));
        assert_eq!(disk_path(&first), disk_path(&reopened));
    }
}
