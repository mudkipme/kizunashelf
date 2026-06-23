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
use std::path::PathBuf;

/// Structural format of the on-disk blob. A mismatch is rejected outright.
const CACHE_FORMAT: u32 = 1;

/// Bump when the *meaning* of a cached parse result changes without the crate
/// version changing (e.g. a derivation tweak shipped in the same version during
/// development). Folded into the engine version below.
const CACHE_LOGIC_VERSION: u32 = 2;

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

/// Context for using the persistent index cache during a single library load.
pub(crate) struct IndexCacheContext {
    path: PathBuf,
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
            path,
            schema_fingerprint: hash_str(raw_vault_config),
            vault_identity: identity,
        }
    }

    /// Loads the cached entity + daily-note entries, returning an empty cache
    /// whenever the file is absent, unreadable, structurally stale, or fails any
    /// global gate. Never returns data we aren't certain matches the current
    /// engine + schema.
    pub(super) fn load(&self) -> LoadedCache {
        let Ok(bytes) = std::fs::read(&self.path) else {
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

    /// Persists the per-file entries atomically (temp + rename). Failures are
    /// swallowed — a cache we couldn't write just means the next load is cold.
    pub(super) fn save(
        &self,
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
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = self.path.with_extension("json.tmp");
        if std::fs::write(&tmp, &serialized).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}
