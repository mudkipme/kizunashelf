//! Reading the whole library from the vault — the uncached and index-cached
//! paths, the per-directory streaming read, and the derived relation counts.

use super::collation::compare_string;
use super::config_io::{read_raw_vault_config_via_vfs, validate_config_paths};
use super::index_cache::{fingerprint_hit, CachedEntry, IndexCacheContext};
use super::parse::{parse_entity, EntityReadResult};
use super::relations::{build_relations, read_daily_note_links};
use crate::daily_notes::daily_note_candidates;
use crate::types::{
    EntityRecord, EntityTypeConfig, KizunaConfig, Library, LibraryDiagnostic, Relation,
};
use crate::vfs::{Vfs, VfsError};
use anyhow::Result;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use unicode_normalization::UnicodeNormalization;

/// Reads the whole library from the vault, parsing every entity fresh. This is
/// the uncached path (web/desktop without a configured cache dir, and tests).
pub async fn read_library(config: KizunaConfig, vfs: Arc<dyn Vfs>) -> Result<Library> {
    read_library_inner(config, vfs, None).await
}

/// Reads the library using the persistent index cache: files whose
/// directory-listing fingerprint is unchanged reuse their cached parse result;
/// only changed/new files are read and re-parsed. The result is value-equivalent
/// (up to `generated_at`) to [`read_library`]; the cache only avoids redundant
/// reads + parses. See [`super::index_cache`].
pub(crate) async fn read_library_cached(
    config: KizunaConfig,
    vfs: Arc<dyn Vfs>,
    cache: IndexCacheContext,
) -> Result<Library> {
    read_library_inner(config, vfs, Some(cache)).await
}

/// A cheap fingerprint of everything that determines the derived library,
/// computed from `read_dir` listings alone (no content reads): the raw vault
/// config (the schema, which gates all derivation), plus every entity `.md` file's
/// and every daily note's `(path, len, mtime)`. Two reloads with equal
/// fingerprints have an unchanged schema and an unchanged set of file contents, so
/// the previously built [`Library`] is still valid and can be reused without
/// re-reading, re-parsing, or rebuilding relations.
///
/// Returns `None` — "can't vouch for unchangedness, reload fully" — when the
/// config can't be read or any file reports a `0` mtime (a backend that can't
/// report one during listing, the same case [`fingerprint_hit`] treats as
/// never-cacheable). Folded order-independently (XOR), so listing order is
/// irrelevant.
pub(crate) async fn compute_listing_fingerprint(
    config: &KizunaConfig,
    vfs: &dyn Vfs,
) -> Option<u64> {
    // The schema gates all derivation, so an external config edit must bust the
    // fingerprint even though the config file is neither an entity nor a note.
    let raw_config = read_raw_vault_config_via_vfs(vfs).await.ok()?;
    let mut config_hasher = std::collections::hash_map::DefaultHasher::new();
    raw_config.hash(&mut config_hasher);
    let mut digest: u64 = config_hasher.finish();

    for type_config in &config.types {
        let relative_dir = format!(
            "{}/{}",
            config.taxonomy_root.trim_end_matches('/'),
            type_config.path
        );
        let dir_entries = match vfs.read_dir(&relative_dir).await {
            Ok(entries) => entries,
            Err(VfsError::NotFound) => continue,
            Err(_) => return None,
        };
        for entry in dir_entries {
            if !(entry.is_file && entry.name.ends_with(".md")) {
                continue;
            }
            let path = format!("{relative_dir}/{}", entry.name);
            digest ^= listing_entry_hash(&path, entry.len, entry.modified_unix_nanos)?;
        }
    }

    // Daily notes use the same discovery pass as the load itself, so the fingerprint
    // covers exactly the notes that contribute relations.
    let candidates = daily_note_candidates(config, vfs, None, None, false)
        .await
        .ok()?;
    for note in candidates {
        digest ^= listing_entry_hash(&note.relative_path, note.len, note.modified_unix_nanos)?;
    }

    Some(digest)
}

/// Hash of one listed file's `(path, len, mtime)`, or `None` when its mtime is `0`
/// — which makes the whole fingerprint `None`, so a backend that can't report
/// modification times never lets us conclude "unchanged".
fn listing_entry_hash(path: &str, len: u64, modified_unix_nanos: u128) -> Option<u64> {
    if modified_unix_nanos == 0 {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    len.hash(&mut hasher);
    modified_unix_nanos.hash(&mut hasher);
    Some(hasher.finish())
}

async fn read_library_inner(
    config: KizunaConfig,
    vfs: Arc<dyn Vfs>,
    cache: Option<IndexCacheContext>,
) -> Result<Library> {
    validate_library_roots(&config, vfs.as_ref()).await?;
    // Load whatever the cache can vouch for (empty when absent/stale/mismatched);
    // every file not reused from here is read and parsed fresh below.
    let loaded = cache.as_ref().map(|cache| cache.load()).unwrap_or_default();

    // Per-directory streaming: bodies/raw are extracted into compact records +
    // body-link lists and dropped before the next directory, so resident memory
    // never holds the whole vault's content.
    let read = read_entities(&config, &vfs, &loaded.entries).await?;
    let mut records = read.records;
    let diagnostics = read.diagnostics;
    // Daily-note wikilinks, reusing the cache for unchanged notes so a vault with
    // many daily notes doesn't re-read every one on a cold start. Relation
    // building itself is then pure (no I/O).
    let daily = read_daily_note_links(&config, vfs.as_ref(), &loaded.daily_notes).await?;
    let relations = build_relations(&config, &records, &daily.links);
    let relation_count_by_id = unique_relation_count_by_id(&relations);

    for record in &mut records {
        record.summary.relation_count = *relation_count_by_id.get(&record.summary.id).unwrap_or(&0);
    }

    // Persist the freshly assembled per-file entries + daily-note links (current
    // listing only, so deleted files drop out) — but only when something actually
    // changed, to avoid rewriting an unchanged cache on every load.
    if let Some(cache) = &cache {
        if read.cache_dirty || daily.dirty {
            cache.save(read.entries, daily.cache);
        }
    }

    Ok(Library::new(
        config,
        records,
        relations,
        diagnostics,
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    ))
}

pub(super) fn unique_relation_count_by_id(relations: &[Relation]) -> HashMap<String, u32> {
    let mut related_by_id: HashMap<String, HashSet<String>> = HashMap::new();

    for relation in relations {
        let target_key = relation
            .target_id
            .clone()
            .unwrap_or_else(|| format!("unresolved:{}", relation.target_title));
        related_by_id
            .entry(relation.source_id.clone())
            .or_default()
            .insert(target_key.clone());

        if let Some(target_id) = &relation.target_id {
            related_by_id
                .entry(target_id.clone())
                .or_default()
                .insert(relation.source_id.clone());
        }
    }

    related_by_id
        .into_iter()
        .map(|(id, related)| (id, related.len() as u32))
        .collect()
}

/// Validates that the vault root and taxonomy root exist and are directories.
/// Containment of the configured (relative) sub-paths is guaranteed by
/// [`validate_config_paths`] plus the VFS's path normalization, so no
/// canonicalization is needed (and none is available on a non-local VFS).
async fn validate_library_roots(config: &KizunaConfig, vfs: &dyn Vfs) -> Result<()> {
    validate_config_paths(config)?;
    let vault_metadata = vfs.metadata("").await.map_err(|error| match error {
        VfsError::NotFound => {
            anyhow::anyhow!("failed to access vault root {}", config.vault_root)
        }
        other => anyhow::anyhow!("failed to access vault root {}: {other}", config.vault_root),
    })?;
    if !vault_metadata.is_dir {
        anyhow::bail!("vault root is not a directory: {}", config.vault_root);
    }

    match vfs.metadata(&config.taxonomy_root).await {
        Ok(metadata) => {
            if !metadata.is_dir {
                anyhow::bail!("taxonomy root is not a directory: {}", config.taxonomy_root);
            }
        }
        // A freshly-created or not-yet-populated vault may not have the taxonomy
        // root directory yet (it's created on the first entity write). Treat that
        // as an empty library rather than failing the load — consistent with how
        // missing per-type directories are tolerated.
        Err(VfsError::NotFound) => {}
        Err(other) => {
            anyhow::bail!(
                "failed to access taxonomy root {}: {other}",
                config.taxonomy_root
            );
        }
    }
    Ok(())
}

/// The assembled result of reading every entity: the slim resident records (each
/// carrying its body wikilinks), diagnostics, the per-file cache entries to
/// persist (current listing only, so deletions drop out), and whether the cache
/// changed and should be rewritten.
struct LibraryRead {
    records: Vec<EntityRecord>,
    diagnostics: Vec<LibraryDiagnostic>,
    entries: BTreeMap<String, CachedEntry>,
    cache_dirty: bool,
}

/// Reads every entity. Files whose directory-listing fingerprint matches a
/// `loaded` cache entry reuse it (no read/parse); the rest are read and parsed.
/// Each type directory is read and reduced before the next, so the full file
/// contents of at most one directory are in memory at a time.
async fn read_entities(
    config: &KizunaConfig,
    vfs: &Arc<dyn Vfs>,
    loaded: &HashMap<String, CachedEntry>,
) -> Result<LibraryRead> {
    let mut records = Vec::new();
    let mut diagnostics = Vec::new();
    let mut entries: BTreeMap<String, CachedEntry> = BTreeMap::new();
    let mut misses = 0usize;
    for type_config in &config.types {
        let batch = read_entities_for_type(config, type_config, vfs, loaded).await?;
        misses += batch.misses;
        for (key, entry) in batch.entries {
            records.push(entry.record.clone());
            diagnostics.extend(entry.diagnostics.clone());
            entries.insert(key, entry);
        }
    }
    sort_records(&mut records);
    // Rewrite the cache when anything was re-parsed, or when the set of files
    // shrank (a deletion the `loaded` map still held). A clean warm load (no
    // misses, same count) leaves the cache file untouched.
    let cache_dirty = misses > 0 || entries.len() != loaded.len();
    Ok(LibraryRead {
        records,
        diagnostics,
        entries,
        cache_dirty,
    })
}

/// The NFC-normalized cache key for a vault-relative path. Apple filesystems hand
/// back NFD-decomposed names; normalizing here keeps cache keys stable across the
/// NFC/NFD divide so a file isn't perpetually missed (or duplicated) by composing
/// marks in its name.
pub(super) fn cache_key(path: &str) -> String {
    path.nfc().collect()
}

/// Orders resident records by type label then title — the stable resident order
/// every load produces, so an entity lands in the same position regardless of the
/// order its file was read in.
pub(super) fn sort_records(records: &mut [EntityRecord]) {
    records.sort_by(|a, b| {
        let type_compare = compare_string(&a.summary.type_label, &b.summary.type_label);
        if !type_compare.is_eq() {
            return type_compare;
        }
        compare_string(&a.summary.title, &b.summary.title)
    });
}

/// One type directory's read: the per-file cache entries (cached hits + freshly
/// parsed misses, keyed by NFC path) and how many files were re-parsed.
struct TypeRead {
    entries: Vec<(String, CachedEntry)>,
    misses: usize,
}

async fn read_entities_for_type(
    config: &KizunaConfig,
    type_config: &EntityTypeConfig,
    vfs: &Arc<dyn Vfs>,
    loaded: &HashMap<String, CachedEntry>,
) -> Result<TypeRead> {
    let relative_dir = format!(
        "{}/{}",
        config.taxonomy_root.trim_end_matches('/'),
        type_config.path
    );
    let dir_entries = match vfs.read_dir(&relative_dir).await {
        Ok(entries) => entries,
        Err(VfsError::NotFound) => {
            return Ok(TypeRead {
                entries: Vec::new(),
                misses: 0,
            });
        }
        Err(error) => {
            return Err(anyhow::anyhow!(
                "failed to read taxonomy directory {relative_dir}: {error}"
            ));
        }
    };

    // Split the listing into cache hits (fingerprint unchanged, reused without a
    // read) and misses (read + parsed below). The fingerprint — size + mtime —
    // comes from this same enumeration pass; no per-file stat or read.
    let mut entries: Vec<(String, CachedEntry)> = Vec::new();
    let mut miss_paths: Vec<String> = Vec::new();
    let mut miss_fingerprints: HashMap<String, (u64, u128)> = HashMap::new();
    for entry in dir_entries {
        if !(entry.is_file && entry.name.ends_with(".md")) {
            continue;
        }
        let path = format!("{relative_dir}/{}", entry.name);
        let key = cache_key(&path);
        if let Some(cached) = loaded.get(&key) {
            if fingerprint_hit(
                cached.len,
                cached.modified_unix_nanos,
                entry.len,
                entry.modified_unix_nanos,
            ) {
                entries.push((key, cached.clone()));
                continue;
            }
        }
        miss_fingerprints.insert(path.clone(), (entry.len, entry.modified_unix_nanos));
        miss_paths.push(path);
    }

    let misses = miss_paths.len();
    // One batched read for the changed/new files only, rather than a read (and a
    // stat) per file — the per-call FFI + file-coordination overhead on iOS makes
    // per-file round trips the dominant load cost.
    let files = vfs
        .read_files(&miss_paths)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entities in {relative_dir}: {error}"))?;

    for (relative_path, bytes) in files {
        let (len, modified_unix_nanos) = miss_fingerprints
            .get(&relative_path)
            .copied()
            .unwrap_or((0, 0));
        let key = cache_key(&relative_path);
        let EntityReadResult {
            entity,
            body_links,
            episode_dates,
            diagnostics,
        } = parse_entity(type_config, config.tags_field(), relative_path, bytes)?;
        entries.push((
            key,
            CachedEntry {
                len,
                modified_unix_nanos,
                record: EntityRecord {
                    body_links,
                    summary: entity.summary,
                    revision: entity.revision,
                    frontmatter: entity.frontmatter,
                    file_modified_unix_nanos: modified_unix_nanos,
                    episode_dates,
                },
                diagnostics,
            },
        ));
    }
    Ok(TypeRead { entries, misses })
}
