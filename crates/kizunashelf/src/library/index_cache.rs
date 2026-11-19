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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Structural format of the on-disk blob. A mismatch is rejected outright.
const CACHE_FORMAT: u32 = 1;

/// Bump when the *meaning* of a cached parse result changes without the crate
/// version changing (e.g. a derivation tweak shipped in the same version during
/// development). Folded into the engine version below.
const CACHE_LOGIC_VERSION: u32 = 7;

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
/// keyed by NFC path and shared behind `Arc` so warm loads don't deep-clone
/// every unchanged record.
#[derive(Default)]
pub(super) struct LoadedCache {
    pub(super) entries: HashMap<String, Arc<CachedEntry>>,
    pub(super) daily_notes: HashMap<String, Arc<CachedDailyNote>>,
}

/// The serialized blob: a header of global gates plus the per-file entries.
#[derive(Deserialize)]
struct IndexCacheFile {
    cache_format: u32,
    engine_version: String,
    schema_fingerprint: String,
    vault_identity: String,
    entries: BTreeMap<String, CachedEntry>,
    #[serde(default)]
    daily_notes: BTreeMap<String, CachedDailyNote>,
}

/// Borrowed view used for serialization so persisting the cache doesn't clone
/// every `Arc<CachedEntry>`'s payload.
#[derive(Serialize)]
struct IndexCacheFileRef<'a> {
    cache_format: u32,
    engine_version: String,
    schema_fingerprint: &'a str,
    vault_identity: &'a str,
    entries: BTreeMap<&'a str, &'a CachedEntry>,
    daily_notes: BTreeMap<&'a str, &'a CachedDailyNote>,
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

/// Process-resident index cache, shared across library loads. Serves as the sole
/// cache when no persistent cache dir is configured, and as an L1 cache in front
/// of the on-disk JSON blob when one is configured so warm reloads skip disk
/// reads and JSON parsing. Gated by the same schema/vault identity as the disk
/// form (engine version is implicit — it's the same running process).
#[derive(Default)]
pub(crate) struct MemoryIndexCache {
    populated: bool,
    schema_fingerprint: String,
    vault_identity: String,
    entries: HashMap<String, Arc<CachedEntry>>,
    daily_notes: HashMap<String, Arc<CachedDailyNote>>,
}

/// Context for using the index cache during a single library load. May be backed
/// by memory only, disk only, or memory (L1) + disk (L2); all forms apply the
/// same schema/vault gates so a schema edit or vault switch rebuilds.
pub(crate) struct IndexCacheContext {
    disk: Option<PathBuf>,
    memory: Option<Arc<Mutex<MemoryIndexCache>>>,
    schema_fingerprint: String,
    vault_identity: String,
}

impl IndexCacheContext {
    /// Builds a disk-only context for the host cache directory `dir`.
    /// `raw_vault_config` is the verbatim config text whose hash gates the whole
    /// cache; `vault_identity` distinguishes vaults that share one cache dir and
    /// is folded into the cache filename.
    #[cfg(test)]
    pub(crate) fn new(dir: PathBuf, raw_vault_config: &str, vault_identity: &str) -> Self {
        let identity = hash_str(vault_identity);
        let path = dir.join(format!("index-{identity}.json"));
        Self {
            disk: Some(path),
            memory: None,
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
            disk: None,
            memory: Some(store),
            schema_fingerprint: hash_str(raw_vault_config),
            vault_identity: hash_str(vault_identity),
        }
    }

    /// Builds a context backed by a process-resident L1 [`MemoryIndexCache`]
    /// paired with a persistent disk blob in `dir`. Warm reloads read from L1
    /// without touching disk; cold starts hydrate L1 from disk.
    pub(crate) fn with_memory_and_disk(
        store: Arc<Mutex<MemoryIndexCache>>,
        dir: PathBuf,
        raw_vault_config: &str,
        vault_identity: &str,
    ) -> Self {
        let identity = hash_str(vault_identity);
        let path = dir.join(format!("index-{identity}.json"));
        Self {
            disk: Some(path),
            memory: Some(store),
            schema_fingerprint: hash_str(raw_vault_config),
            vault_identity: identity,
        }
    }

    /// Loads the cached entity + daily-note entries, returning an empty cache
    /// whenever the source is absent, unreadable, structurally stale, or fails any
    /// gate. Never returns data we aren't certain matches the current
    /// engine + schema + vault.
    pub(super) async fn load(&self) -> LoadedCache {
        if let Some(store) = &self.memory {
            if let Some(hit) = self.load_memory(store) {
                return hit;
            }
        }
        let loaded = match &self.disk {
            Some(path) => {
                let path = path.clone();
                let schema_fingerprint = self.schema_fingerprint.clone();
                let vault_identity = self.vault_identity.clone();
                tokio::task::spawn_blocking(move || {
                    load_disk(&path, &schema_fingerprint, &vault_identity)
                })
                .await
                .unwrap_or_default()
            }
            None => LoadedCache::default(),
        };
        if let Some(store) = &self.memory {
            if let Ok(mut cache) = store.lock() {
                *cache = MemoryIndexCache {
                    populated: true,
                    schema_fingerprint: self.schema_fingerprint.clone(),
                    vault_identity: self.vault_identity.clone(),
                    entries: loaded.entries.clone(),
                    daily_notes: loaded.daily_notes.clone(),
                };
            }
        }
        loaded
    }

    fn load_memory(&self, store: &Mutex<MemoryIndexCache>) -> Option<LoadedCache> {
        let cache = store.lock().ok()?;
        if !cache.populated
            || cache.schema_fingerprint != self.schema_fingerprint
            || cache.vault_identity != self.vault_identity
        {
            return None;
        }
        Some(LoadedCache {
            entries: cache.entries.clone(),
            daily_notes: cache.daily_notes.clone(),
        })
    }

    /// Persists the per-file entries. Updates the resident L1 cache immediately
    /// and, when configured, writes the disk blob atomically on a blocking thread
    /// (unique temp file + rename). Failures are swallowed — a cache we couldn't
    /// write just means the next cold start re-parses.
    pub(super) async fn save(
        &self,
        entries: BTreeMap<String, Arc<CachedEntry>>,
        daily_notes: BTreeMap<String, Arc<CachedDailyNote>>,
    ) {
        if let Some(store) = &self.memory {
            if let Ok(mut cache) = store.lock() {
                *cache = MemoryIndexCache {
                    populated: true,
                    schema_fingerprint: self.schema_fingerprint.clone(),
                    vault_identity: self.vault_identity.clone(),
                    entries: entries
                        .iter()
                        .map(|(k, v)| (k.clone(), Arc::clone(v)))
                        .collect(),
                    daily_notes: daily_notes
                        .iter()
                        .map(|(k, v)| (k.clone(), Arc::clone(v)))
                        .collect(),
                };
            }
        }
        if let Some(path) = &self.disk {
            let path = path.clone();
            let schema_fingerprint = self.schema_fingerprint.clone();
            let vault_identity = self.vault_identity.clone();
            let _ = tokio::task::spawn_blocking(move || {
                save_disk(
                    &path,
                    &schema_fingerprint,
                    &vault_identity,
                    &entries,
                    &daily_notes,
                );
            })
            .await;
        }
    }
}

fn load_disk(path: &Path, schema_fingerprint: &str, vault_identity: &str) -> LoadedCache {
    let Ok(bytes) = std::fs::read(path) else {
        return LoadedCache::default();
    };
    let Ok(file) = serde_json::from_slice::<IndexCacheFile>(&bytes) else {
        return LoadedCache::default();
    };
    if file.cache_format != CACHE_FORMAT
        || file.engine_version != engine_version()
        || file.schema_fingerprint != schema_fingerprint
        || file.vault_identity != vault_identity
    {
        return LoadedCache::default();
    }
    LoadedCache {
        entries: file
            .entries
            .into_iter()
            .map(|(key, entry)| (key, Arc::new(entry)))
            .collect(),
        daily_notes: file
            .daily_notes
            .into_iter()
            .map(|(key, note)| (key, Arc::new(note)))
            .collect(),
    }
}

fn save_disk(
    path: &Path,
    schema_fingerprint: &str,
    vault_identity: &str,
    entries: &BTreeMap<String, Arc<CachedEntry>>,
    daily_notes: &BTreeMap<String, Arc<CachedDailyNote>>,
) {
    let file = IndexCacheFileRef {
        cache_format: CACHE_FORMAT,
        engine_version: engine_version(),
        schema_fingerprint,
        vault_identity,
        entries: entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_ref()))
            .collect(),
        daily_notes: daily_notes
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_ref()))
            .collect(),
    };
    let Ok(serialized) = serde_json::to_vec(&file) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let seq = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.{}.{}.tmp", std::process::id(), seq));
    if std::fs::write(&tmp, &serialized).is_ok() {
        if std::fs::rename(&tmp, path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk_path(context: &IndexCacheContext) -> &Path {
        context.disk.as_deref().expect("expected disk cache")
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
