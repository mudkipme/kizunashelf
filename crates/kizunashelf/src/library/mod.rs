mod collation;
mod frontmatter;
mod index_cache;
mod relations;

pub(crate) use index_cache::IndexCacheContext;

pub use collation::{compare_optional_string, compare_string, compare_string_for_title_language};
pub use frontmatter::{
    serialize_markdown_document, split_markdown_document, wikilink_regex, MarkdownDocument,
};

use crate::types::{
    DateRole, Entity, EntityRecord, EntitySummary, EntityTypeConfig, FieldType, KizunaConfig,
    Library, LibraryDiagnostic, Relation, VaultConfig,
};
use crate::vfs::{Vfs, VfsError};
use anyhow::{Context, Result};
use frontmatter::{
    date_values, external_refs, extract_summary, first_string, parse_markdown, resolve_title,
    title_languages,
};
use index_cache::{fingerprint_hit, CachedEntry};
use relations::{
    build_entity_relations, build_relations, dedupe_relations, extract_body_links,
    read_daily_note_links, DAILY_NOTE_RELATION_FIELD,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Component, Path};
use std::sync::Arc;
use unicode_normalization::UnicodeNormalization;

/// Vault-relative location of the vault config file inside `<vaultRoot>`.
pub const VAULT_CONFIG_RELATIVE_PATH: &str = ".kizunashelf/config.yaml";

/// Writes the vault config to `.kizunashelf/config.yaml` inside the vault through
/// the VFS (the iOS settings path).
pub async fn save_vault_config_via_vfs(vfs: &dyn Vfs, config: &VaultConfig) -> Result<()> {
    let raw = serde_yaml::to_string(config).context("failed to serialize vault config")?;
    if let Some(parent) = Path::new(VAULT_CONFIG_RELATIVE_PATH)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        vfs.create_dir_all(&parent.to_string_lossy())
            .await
            .map_err(|error| anyhow::anyhow!("failed to create vault config directory: {error}"))?;
    }
    vfs.write_atomic(VAULT_CONFIG_RELATIVE_PATH, format!("{raw}\n").as_bytes())
        .await
        .map_err(|error| anyhow::anyhow!("failed to write vault config: {error}"))
}

/// Creates the taxonomy / entity-type / daily-note directories through the VFS,
/// so editing the schema from iOS can add new type paths. Containment is enforced
/// by the VFS path normalization.
pub async fn ensure_config_directories_via_vfs(config: &KizunaConfig, vfs: &dyn Vfs) -> Result<()> {
    validate_config_paths(config)?;
    let create = |path: String| async move {
        vfs.create_dir_all(&path)
            .await
            .map_err(|error| anyhow::anyhow!("failed to create directory {path}: {error}"))
    };
    create(config.taxonomy_root.clone()).await?;
    for type_config in &config.types {
        create(format!(
            "{}/{}",
            config.taxonomy_root.trim_end_matches('/'),
            type_config.path
        ))
        .await?;
    }
    if let Some(daily_notes) = &config.daily_notes {
        for path in &daily_notes.paths {
            create(path.clone()).await?;
        }
    }
    Ok(())
}

/// Writes raw vault-config YAML verbatim to `.kizunashelf/config.yaml` through
/// the VFS, preserving the user's exact formatting and comments. This is the
/// raw-editor counterpart to [`save_vault_config_via_vfs`] (which re-serializes
/// a typed config). Callers MUST validate the text with
/// [`parse_vault_config_strict`] before writing — this helper writes whatever it
/// is given.
pub async fn save_raw_vault_config_via_vfs(vfs: &dyn Vfs, content: &str) -> Result<()> {
    if let Some(parent) = Path::new(VAULT_CONFIG_RELATIVE_PATH)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        vfs.create_dir_all(&parent.to_string_lossy())
            .await
            .map_err(|error| anyhow::anyhow!("failed to create vault config directory: {error}"))?;
    }
    vfs.write_atomic(VAULT_CONFIG_RELATIVE_PATH, content.as_bytes())
        .await
        .map_err(|error| anyhow::anyhow!("failed to write vault config: {error}"))
}

/// Reads the raw vault-config YAML text verbatim through the VFS (the raw-editor
/// path). Returns `None` when the config file doesn't exist yet.
pub async fn read_raw_vault_config_via_vfs(vfs: &dyn Vfs) -> Result<Option<String>> {
    match vfs.read_to_string(VAULT_CONFIG_RELATIVE_PATH).await {
        Ok(raw) => Ok(Some(raw)),
        Err(VfsError::NotFound) => Ok(None),
        Err(other) => Err(anyhow::anyhow!("failed to read vault config: {other}")),
    }
}

/// Strictly parses raw vault-config YAML into a [`VaultConfig`]. Unlike
/// [`load_vault_config_via_vfs`] — which tolerates extra keys for
/// forward-compatibility — this is the raw-editor validation path: it surfaces
/// type errors, missing required fields, invalid enum values, AND any field the
/// schema doesn't recognize as an error, so a typo or stray key is rejected
/// rather than silently dropped on the next save.
pub fn parse_vault_config_strict(content: &str) -> Result<VaultConfig> {
    let de = serde_yaml::Deserializer::from_str(content);
    let mut unknown = Vec::new();
    let config: VaultConfig = serde_ignored::deserialize(de, |path| unknown.push(path.to_string()))
        .context("invalid vault config")?;
    if !unknown.is_empty() {
        anyhow::bail!("unknown config field(s): {}", unknown.join(", "));
    }
    Ok(config)
}

/// Reads the vault config from inside the vault (`.kizunashelf/config.yaml`)
/// through the VFS. Works for both `NativeVfs` (desktop) and the injected iOS
/// VFS, so library loading never needs an absolute vault-config path.
pub async fn load_vault_config_via_vfs(vfs: &dyn Vfs) -> Result<VaultConfig> {
    let raw = vfs
        .read_to_string(VAULT_CONFIG_RELATIVE_PATH)
        .await
        .map_err(|error| match error {
            VfsError::NotFound => anyhow::anyhow!(
                "vault config not found at {VAULT_CONFIG_RELATIVE_PATH}; run onboarding to create one"
            ),
            other => anyhow::anyhow!("failed to read vault config: {other}"),
        })?;
    serde_yaml::from_str(&raw).context("invalid vault config")
}

/// Reads the whole library from the vault, parsing every entity fresh. This is
/// the uncached path (web/desktop without a configured cache dir, and tests).
pub async fn read_library(config: KizunaConfig, vfs: Arc<dyn Vfs>) -> Result<Library> {
    read_library_inner(config, vfs, None).await
}

/// Reads the library using the persistent index cache: files whose
/// directory-listing fingerprint is unchanged reuse their cached parse result;
/// only changed/new files are read and re-parsed. The result is value-equivalent
/// (up to `generated_at`) to [`read_library`]; the cache only avoids redundant
/// reads + parses. See [`index_cache`].
pub(crate) async fn read_library_cached(
    config: KizunaConfig,
    vfs: Arc<dyn Vfs>,
    cache: IndexCacheContext,
) -> Result<Library> {
    read_library_inner(config, vfs, Some(cache)).await
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

/// Loads a single full [`Entity`] (including `body`/`raw`) from disk for the
/// given resident summary. This is the on-demand counterpart to the slim
/// [`EntityRecord`] kept in the cache: detail views, mutations, and asset writes
/// call it when they need the complete document, instead of keeping every body
/// resident. Re-parsing from disk also returns the freshest content.
pub(crate) async fn load_entity(
    config: &KizunaConfig,
    vfs: &dyn Vfs,
    summary: &EntitySummary,
) -> Result<Entity> {
    let type_config = config
        .types
        .iter()
        .find(|type_config| type_config.id == summary.entity_type)
        .with_context(|| format!("unknown entity type {}", summary.entity_type))?;
    let bytes = vfs
        .read(&summary.path)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {}: {error}", summary.path))?;
    let mut entity = parse_entity(type_config, summary.path.clone(), bytes)?.entity;
    // `relation_count` is a library-wide derived value (from the relation graph),
    // not something a single file knows; carry it over from the resident summary.
    entity.summary.relation_count = summary.relation_count;
    Ok(entity)
}

/// Surgically rebuilds the library after a single entity file was edited in
/// place, without re-reading the whole vault from disk. Returns `Ok(None)` when
/// the edit is *structural* — the path is unknown, or the re-parse changed the
/// entity's id or path (a rename / id-field change) — because those can alter
/// wikilink resolution for arbitrary other entities and so need a full reload.
///
/// On success it re-reads only `edited_path`, swaps that one record, and replaces
/// exactly the relations the entity owns (its outgoing frontmatter/body links and
/// the `In` reflections of its resolved targets), leaving every other entity's
/// links intact. Derived `relation_count`s are recomputed from the updated graph.
/// The result is value-equivalent (up to ordering and `generated_at`) to a full
/// [`read_library`] — see the surgical-update tests.
pub(crate) async fn rebuild_for_edited_entity(
    base: &Library,
    vfs: &dyn Vfs,
    edited_path: &str,
) -> Result<Option<Library>> {
    let Some(index) = base.record_index_by_path(edited_path) else {
        return Ok(None);
    };
    let old_summary = &base.records[index].summary;
    let old_id = old_summary.id.clone();
    let Some(type_config) = base
        .config
        .types
        .iter()
        .find(|type_config| type_config.id == old_summary.entity_type)
    else {
        return Ok(None);
    };

    let bytes = match vfs.read(edited_path).await {
        Ok(bytes) => bytes,
        // The file vanished or is unreadable: let a full reload reconcile it.
        Err(_) => return Ok(None),
    };
    let EntityReadResult {
        entity,
        body_links,
        diagnostics: parse_diagnostics,
    } = parse_entity(type_config, edited_path.to_string(), bytes)?;
    let new_record = EntityRecord {
        body_links,
        summary: entity.summary,
        revision: entity.revision,
        frontmatter: entity.frontmatter,
    };

    // Structural change → full reload (it can re-resolve other entities' links).
    if new_record.summary.id != old_id || new_record.summary.path != edited_path {
        return Ok(None);
    }

    let mut records = base.records.clone();
    records[index] = new_record;

    // A title edit can change this record's sort position; re-establish the same
    // resident order a full load would produce.
    sort_records(&mut records);

    // Recompute the entity-to-entity relations in memory from the resident
    // records — each carries its own `body_links`, so this matches a full reload
    // without re-reading the other (unchanged) files. Daily-note relations resolve
    // daily-note wikilinks to entities by basename and store the entity's id/type;
    // an in-place edit changes none of those (the structural bailout above keeps
    // the id/path, and the type/basename come from the unchanged file
    // location/name), so the daily notes can't have gained or lost a link to this
    // entity. Reuse the resident daily-note relations from `base` instead of
    // re-reading every daily note from disk.
    let mut relations = build_entity_relations(&base.config, &records);
    relations.extend(
        base.relations
            .iter()
            .filter(|relation| relation.field == DAILY_NOTE_RELATION_FIELD)
            .cloned(),
    );
    let relations = dedupe_relations(relations);
    let relation_count_by_id = unique_relation_count_by_id(&relations);
    for record in &mut records {
        record.summary.relation_count = *relation_count_by_id.get(&record.summary.id).unwrap_or(&0);
    }

    // Replace this file's diagnostics with the fresh parse's.
    let mut diagnostics: Vec<LibraryDiagnostic> = base
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.path != edited_path)
        .cloned()
        .collect();
    diagnostics.extend(parse_diagnostics);

    Ok(Some(Library::new(
        base.config.clone(),
        records,
        relations,
        diagnostics,
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    )))
}

fn unique_relation_count_by_id(relations: &[Relation]) -> HashMap<String, u32> {
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

fn validate_config_paths(config: &KizunaConfig) -> Result<()> {
    if config.vault_root.trim().is_empty() {
        anyhow::bail!("vaultRoot cannot be empty");
    }
    validate_relative_config_path("taxonomyRoot", &config.taxonomy_root)?;
    for type_config in &config.types {
        validate_relative_config_path(
            &format!("type path for {}", type_config.id),
            &type_config.path,
        )?;
    }
    if let Some(daily_notes) = &config.daily_notes {
        for path in &daily_notes.paths {
            validate_relative_config_path("daily notes path", path)?;
        }
    }
    Ok(())
}

fn validate_relative_config_path(label: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("{label} cannot be empty");
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        anyhow::bail!("{label} must be relative to vaultRoot");
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        anyhow::bail!("{label} cannot contain parent directory components");
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
/// produced by a full load, which the surgical update also re-establishes so an
/// edited title lands in the same position it would after a reload.
fn sort_records(records: &mut [EntityRecord]) {
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

struct EntityReadResult {
    entity: Entity,
    /// Body wikilink targets, extracted from the body during the parse (while it
    /// is in hand) so the resident [`EntityRecord`] can carry them after the body
    /// is dropped. See [`EntityRecord::body_links`].
    body_links: Vec<String>,
    diagnostics: Vec<LibraryDiagnostic>,
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
            diagnostics,
        } = parse_entity(type_config, relative_path, bytes)?;
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
                },
                diagnostics,
            },
        ));
    }
    Ok(TypeRead { entries, misses })
}

/// Parses one entity from its raw bytes — no I/O. The revision is derived from
/// the content (hash + length); a separate `metadata` call for the mtime is not
/// worth its per-file cost, and the content hash already detects edits. Body
/// wikilinks are extracted here too (while the body is in hand) and returned in
/// the result, so callers building a resident [`EntityRecord`] don't re-scan.
fn parse_entity(
    type_config: &EntityTypeConfig,
    relative_path: String,
    bytes: Vec<u8>,
) -> Result<EntityReadResult> {
    let raw = String::from_utf8(bytes)
        .map_err(|error| anyhow::anyhow!("entity {relative_path} is not valid UTF-8: {error}"))?;
    let revision = file_revision(&raw);
    let parsed = parse_markdown(&raw);
    let entry = relative_path.rsplit('/').next().unwrap_or(&relative_path);
    let note_basename = entry.strip_suffix(".md").unwrap_or(entry).to_string();
    let titles = title_languages(&parsed.frontmatter, &note_basename, type_config);
    let title = resolve_title(&parsed.frontmatter, &titles, &note_basename, type_config);
    let entity_key = entity_key(&parsed.frontmatter, &note_basename, type_config);
    let diagnostics = parsed
        .diagnostics
        .iter()
        .map(|message| LibraryDiagnostic {
            path: relative_path.clone(),
            kind: "frontmatter".to_string(),
            message: message.clone(),
        })
        .collect();

    let summary = EntitySummary {
        id: format!("{}:{entity_key}", type_config.id),
        entity_type: type_config.id.clone(),
        type_label: type_config.label.clone(),
        title,
        titles,
        dates: date_values(&parsed.frontmatter, &date_field_names(type_config)),
        image: first_field_string_for_types(
            &parsed.frontmatter,
            type_config,
            &[FieldType::Image, FieldType::ImageList],
        ),
        summary: extract_summary(&parsed.body),
        path: relative_path,
        basename: note_basename,
        external_refs: external_refs(
            &parsed.frontmatter,
            &field_names(type_config, FieldType::ExternalRef),
        ),
        relation_count: 0,
    };

    Ok(EntityReadResult {
        body_links: extract_body_links(&parsed.body),
        entity: Entity {
            summary,
            revision,
            frontmatter: parsed.frontmatter,
            body: parsed.body,
            raw,
        },
        diagnostics,
    })
}

pub(crate) fn file_revision(raw: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hasher);
    format!("{:x}-{}", hasher.finish(), raw.len())
}

fn entity_key(
    frontmatter: &serde_json::Map<String, serde_json::Value>,
    basename: &str,
    type_config: &EntityTypeConfig,
) -> String {
    first_string(frontmatter, &field_names(type_config, FieldType::Id))
        .unwrap_or_else(|| basename.to_string())
}

fn first_field_string_for_types(
    frontmatter: &serde_json::Map<String, serde_json::Value>,
    type_config: &EntityTypeConfig,
    field_types: &[FieldType],
) -> Option<String> {
    first_string(
        frontmatter,
        &field_names_for_types(type_config, field_types),
    )
}

fn field_names(type_config: &EntityTypeConfig, field_type: FieldType) -> Vec<String> {
    field_names_for_types(type_config, &[field_type])
}

fn field_names_for_types(type_config: &EntityTypeConfig, field_types: &[FieldType]) -> Vec<String> {
    type_config
        .fields
        .iter()
        .filter(|field| field_types.contains(&field.field_type))
        .map(|field| field.field.clone())
        .collect()
}

fn date_field_names(type_config: &EntityTypeConfig) -> Vec<String> {
    type_config
        .fields
        .iter()
        .filter(|field| {
            matches!(field.field_type, FieldType::Date | FieldType::Season)
                && matches!(
                    field.date_role,
                    Some(DateRole::Planning | DateRole::Completed)
                )
        })
        .map(|field| field.field.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        compare_string, load_entity, read_library, read_library_cached, rebuild_for_edited_entity,
        IndexCacheContext,
    };
    use crate::types::{
        EntityTypeConfig, FieldConfig, FieldType, FilenameConfig, KizunaConfig, Library, Relation,
    };
    use crate::vfs::{InMemoryVfs, NativeVfs, Vfs};
    use std::sync::Arc;

    fn native_vfs(config: &KizunaConfig) -> Arc<dyn Vfs> {
        Arc::new(NativeVfs::new(&config.vault_root))
    }

    #[test]
    fn compare_string_supports_non_english_collation_without_system_icu_data() {
        assert!(compare_string("星旅", "月城").is_ne());
    }

    #[test]
    fn compare_string_is_total_for_mixed_ascii_and_non_ascii_values() {
        let values = [
            "Anime",
            "anime",
            "Zeta",
            "zeta",
            "アニメ",
            "星旅",
            "月城",
            " Pokémon",
            "Pokemon",
            "ポケモン",
            "音乐",
            "Music",
        ];

        for a in values {
            assert_eq!(compare_string(a, a), std::cmp::Ordering::Equal);
            for b in values {
                assert_eq!(compare_string(a, b), compare_string(b, a).reverse());
                for c in values {
                    if compare_string(a, b).is_le() && compare_string(b, c).is_le() {
                        assert!(
                            compare_string(a, c).is_le(),
                            "compare_string is not transitive for {a:?}, {b:?}, {c:?}"
                        );
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn read_library_errors_when_vault_root_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let config = test_config(temp.path().join("missing").to_string_lossy().as_ref());
        let vfs = native_vfs(&config);

        let error = read_library(config, vfs).await.unwrap_err().to_string();

        assert!(error.contains("failed to access vault root"));
    }

    #[tokio::test]
    async fn read_library_is_empty_when_taxonomy_root_is_missing() {
        // A freshly-created vault has no taxonomy directory yet; the load should
        // succeed with an empty library rather than failing.
        let temp = tempfile::tempdir().unwrap();
        let config = test_config(temp.path().to_string_lossy().as_ref());
        let vfs = native_vfs(&config);

        let library = read_library(config, vfs).await.unwrap();

        assert!(library.records.is_empty());
        assert!(library.summaries().next().is_none());
    }

    #[tokio::test]
    async fn read_library_uses_configured_id_fields_and_reports_frontmatter_errors() {
        let temp = tempfile::tempdir().unwrap();
        let taxonomy = temp.path().join("Taxonomy/Anime");
        std::fs::create_dir_all(&taxonomy).unwrap();
        std::fs::write(
            taxonomy.join("Renamed Later.md"),
            "---\nuid: anime-001\ntitle: Stable Title\n---\n",
        )
        .unwrap();
        std::fs::write(taxonomy.join("Broken.md"), "---\ntitle: [broken\n---\n").unwrap();
        let mut config = test_config(temp.path().to_string_lossy().as_ref());
        config.types[0].fields.insert(
            0,
            FieldConfig {
                field: "uid".to_string(),
                field_type: FieldType::Id,
                display_name: Some("UID".to_string()),
                title_language: None,
                title_role: None,
                external_fields: Vec::new(),
                enum_options: Vec::new(),
                total_progress_field: None,
                date_role: None,
                season_language: None,
                external_ref: None,
                external_types: Vec::new(),
                relation_type: None,
            },
        );

        let vfs = native_vfs(&config);
        let library = read_library(config, vfs).await.unwrap();

        assert!(library
            .summaries()
            .any(|entity| entity.id == "anime:anime-001"));
        assert_eq!(library.diagnostics.len(), 1);
        assert_eq!(library.diagnostics[0].path, "Taxonomy/Anime/Broken.md");
        assert_eq!(library.diagnostics[0].kind, "frontmatter");
        assert!(library.diagnostics[0]
            .message
            .contains("invalid frontmatter YAML"));
    }

    #[tokio::test]
    async fn read_library_reads_entities_from_an_in_memory_vfs() {
        let config = test_config("/virtual-vault");
        let vfs = Arc::new(InMemoryVfs::new());
        vfs.insert_dir("Taxonomy/Anime");
        vfs.insert_file(
            "Taxonomy/Anime/Star Voyager.md",
            "---\ntitle: Star Voyager\nstatus: Watching\n---\n\nBody.\n",
        );

        let library = read_library(config, vfs).await.unwrap();

        assert_eq!(library.summaries().count(), 1);
        let summary = library.summaries().next().unwrap();
        assert_eq!(summary.title, "Star Voyager");
        assert_eq!(summary.path, "Taxonomy/Anime/Star Voyager.md");
        // Revision is derived from content + metadata, both supplied by the VFS.
        assert!(!library.records[0].revision.is_empty());
        // The resident record keeps frontmatter (for in-memory filtering) but the
        // body/raw are not part of it — they are loaded on demand. (The absence of
        // `body`/`raw` on `EntityRecord` is enforced at compile time.)
        assert!(library.records[0].frontmatter.contains_key("status"));
    }

    #[tokio::test]
    async fn load_entity_reads_full_body_and_raw_on_demand() {
        let config = test_config("/virtual-vault");
        let vfs = Arc::new(InMemoryVfs::new());
        vfs.insert_dir("Taxonomy/Anime");
        vfs.insert_file(
            "Taxonomy/Anime/Star Voyager.md",
            "---\ntitle: Star Voyager\nstatus: Watching\n---\n\nBody text.\n",
        );

        let library = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
            .await
            .unwrap();
        let summary = library.summaries().next().unwrap();

        let entity = load_entity(&library.config, vfs.as_ref(), summary)
            .await
            .unwrap();

        // The on-demand load reconstructs the full document...
        assert!(entity.body.contains("Body text."));
        assert!(entity.raw.contains("title: Star Voyager"));
        assert!(entity.frontmatter.contains_key("status"));
        // ...and carries the library-wide relation count from the resident summary.
        assert_eq!(entity.summary.relation_count, summary.relation_count);
        assert_eq!(entity.summary.id, summary.id);
    }

    // --- Surgical-update equivalence (Phase 3) -----------------------------

    /// The contract of [`rebuild_for_edited_entity`]: a surgical update of one
    /// edited file must produce a library value-equivalent (records, relations,
    /// derived counts) to re-reading the whole vault from disk.
    #[tokio::test]
    async fn surgical_update_matches_full_reload() {
        // (initial Alpha.md, edited Alpha.md) — covering retarget, add, remove,
        // title change (touches the In-reflection title + sort order), an
        // unresolved target, and a body-wikilink change.
        let cases = [
            (
                "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n\nSee [[Gamma]].\n",
                "---\ntitle: Alpha\nrelated: \"[[Gamma]]\"\n---\n\nSee [[Gamma]].\n",
            ),
            (
                "---\ntitle: Alpha\n---\n",
                "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n",
            ),
            (
                "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n",
                "---\ntitle: Alpha\n---\n",
            ),
            (
                "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n",
                "---\ntitle: Alphaz\nrelated: \"[[Beta]]\"\n---\n",
            ),
            (
                "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n",
                "---\ntitle: Alpha\nrelated: \"[[Ghost]]\"\n---\n",
            ),
            (
                "---\ntitle: Alpha\n---\n\nSee [[Beta]].\n",
                "---\ntitle: Alpha\n---\n\nSee [[Gamma]].\n",
            ),
        ];

        for (initial, edited) in cases {
            let config = relation_test_config();
            let vfs = make_relation_vault(initial);
            let base = read_library(config.clone(), Arc::clone(&vfs) as Arc<dyn Vfs>)
                .await
                .unwrap();

            vfs.insert_file("Taxonomy/Anime/Alpha.md", edited);
            let surgical =
                rebuild_for_edited_entity(&base, vfs.as_ref(), "Taxonomy/Anime/Alpha.md")
                    .await
                    .unwrap()
                    .expect("edit should take the surgical path");
            let full = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
                .await
                .unwrap();

            assert_libraries_equivalent(&surgical, &full, initial, edited);
        }
    }

    #[tokio::test]
    async fn surgical_update_falls_back_on_structural_changes() {
        let config = relation_test_config();
        let vfs = make_relation_vault("---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n");
        let base = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
            .await
            .unwrap();

        // Unknown path (e.g. a rename's new file the base hasn't indexed) → None.
        let renamed = rebuild_for_edited_entity(&base, vfs.as_ref(), "Taxonomy/Anime/Renamed.md")
            .await
            .unwrap();
        assert!(renamed.is_none());

        // A changed id (here: the file vanished from its indexed path) → None.
        let missing = rebuild_for_edited_entity(&base, vfs.as_ref(), "Taxonomy/Anime/Ghost.md")
            .await
            .unwrap();
        assert!(missing.is_none());
    }

    fn relation_test_config() -> KizunaConfig {
        let mut config = test_config("/virtual-vault");
        config.types[0]
            .fields
            .push(relation_field("related", FieldType::Relation));
        config
    }

    fn relation_field(name: &str, field_type: FieldType) -> FieldConfig {
        FieldConfig {
            field: name.to_string(),
            field_type,
            display_name: None,
            title_language: None,
            title_role: None,
            external_fields: Vec::new(),
            enum_options: Vec::new(),
            total_progress_field: None,
            date_role: None,
            season_language: None,
            external_ref: None,
            external_types: Vec::new(),
            relation_type: None,
        }
    }

    fn make_relation_vault(alpha: &str) -> Arc<InMemoryVfs> {
        let vfs = Arc::new(InMemoryVfs::new());
        vfs.insert_dir("Taxonomy/Anime");
        vfs.insert_file("Taxonomy/Anime/Alpha.md", alpha);
        vfs.insert_file("Taxonomy/Anime/Beta.md", "---\ntitle: Beta\n---\n");
        vfs.insert_file("Taxonomy/Anime/Gamma.md", "---\ntitle: Gamma\n---\n");
        vfs
    }

    fn assert_libraries_equivalent(
        surgical: &Library,
        full: &Library,
        initial: &str,
        edited: &str,
    ) {
        let context = format!("\n  initial: {initial:?}\n  edited:  {edited:?}");

        let mut surgical_records = surgical.records.clone();
        let mut full_records = full.records.clone();
        surgical_records.sort_by(|a, b| a.summary.id.cmp(&b.summary.id));
        full_records.sort_by(|a, b| a.summary.id.cmp(&b.summary.id));
        assert_eq!(
            surgical_records, full_records,
            "records (incl. derived relation_count) diverge from a full reload{context}"
        );

        let mut surgical_relations: Vec<String> =
            surgical.relations.iter().map(relation_key).collect();
        let mut full_relations: Vec<String> = full.relations.iter().map(relation_key).collect();
        surgical_relations.sort();
        full_relations.sort();
        assert_eq!(
            surgical_relations, full_relations,
            "relation graph diverges from a full reload{context}"
        );
    }

    fn relation_key(relation: &Relation) -> String {
        format!(
            "{}|{}|{}|{}|{:?}",
            relation.source_id,
            relation.target_id.clone().unwrap_or_default(),
            relation.target_title,
            relation.field,
            relation.direction
        )
    }

    fn test_config(vault_root: &str) -> KizunaConfig {
        KizunaConfig {
            vault_root: vault_root.to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            types: vec![EntityTypeConfig {
                id: "anime".to_string(),
                label: "Anime".to_string(),
                icon: None,
                path: "Anime".to_string(),
                external_priority: Vec::new(),
                filename: Some(FilenameConfig {
                    title_language: Some("zh".to_string()),
                    title_role: None,
                }),
                body_mappings: Vec::new(),
                fields: vec![FieldConfig {
                    field: "title".to_string(),
                    field_type: FieldType::Title,
                    display_name: Some("Title".to_string()),
                    title_language: Some("zh".to_string()),
                    title_role: None,
                    external_fields: Vec::new(),
                    enum_options: Vec::new(),
                    total_progress_field: None,
                    date_role: None,
                    season_language: None,
                    external_ref: None,
                    external_types: Vec::new(),
                    relation_type: None,
                }],
            }],
        }
    }

    // --- Persistent index cache ----------------------------------------------

    fn cache_ctx(dir: &std::path::Path, schema: &str) -> IndexCacheContext {
        IndexCacheContext::new(dir.to_path_buf(), schema, "test-vault")
    }

    fn seed_one(vfs: &InMemoryVfs, title: &str) {
        vfs.insert_dir("Taxonomy/Anime");
        vfs.insert_file(
            "Taxonomy/Anime/Star Voyager.md",
            &format!("---\ntitle: {title}\n---\n\nBody.\n"),
        );
    }

    /// An unchanged fingerprint reuses the cached parse without re-reading the
    /// file: we edit the bytes but preserve size + mtime, then assert the load
    /// still serves the *old* title — only possible if the file was never read.
    #[tokio::test]
    async fn index_cache_reuses_unchanged_entries_without_rereading() {
        let dir = tempfile::tempdir().unwrap();
        let vfs = Arc::new(InMemoryVfs::new());
        seed_one(&vfs, "AAAA");

        let first = read_library_cached(
            test_config("/virtual-vault"),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();
        assert_eq!(first.summaries().next().unwrap().title, "AAAA");

        vfs.overwrite_preserving_stamp(
            "Taxonomy/Anime/Star Voyager.md",
            "---\ntitle: BBBB\n---\n\nBody.\n",
        );
        let second = read_library_cached(
            test_config("/virtual-vault"),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();
        assert_eq!(
            second.summaries().next().unwrap().title,
            "AAAA",
            "unchanged fingerprint must serve the cached parse"
        );
    }

    /// A changed fingerprint (mtime bump from a normal edit) re-reads and
    /// re-parses the file.
    #[tokio::test]
    async fn index_cache_reparses_when_fingerprint_changes() {
        let dir = tempfile::tempdir().unwrap();
        let vfs = Arc::new(InMemoryVfs::new());
        seed_one(&vfs, "AAAA");

        read_library_cached(
            test_config("/virtual-vault"),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();

        vfs.insert_file(
            "Taxonomy/Anime/Star Voyager.md",
            "---\ntitle: CCCC\n---\n\nBody.\n",
        );
        let updated = read_library_cached(
            test_config("/virtual-vault"),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();
        assert_eq!(updated.summaries().next().unwrap().title, "CCCC");
    }

    /// A different schema fingerprint discards the whole cache even when every
    /// file's fingerprint is unchanged — because all derivation flows from the
    /// schema.
    #[tokio::test]
    async fn index_cache_busts_when_schema_fingerprint_changes() {
        let dir = tempfile::tempdir().unwrap();
        let vfs = Arc::new(InMemoryVfs::new());
        seed_one(&vfs, "AAAA");

        read_library_cached(
            test_config("/virtual-vault"),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();

        vfs.overwrite_preserving_stamp(
            "Taxonomy/Anime/Star Voyager.md",
            "---\ntitle: BBBB\n---\n\nBody.\n",
        );
        let updated = read_library_cached(
            test_config("/virtual-vault"),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v2"),
        )
        .await
        .unwrap();
        assert_eq!(
            updated.summaries().next().unwrap().title,
            "BBBB",
            "a schema change must invalidate every cached record"
        );
    }

    /// Files absent from the current listing drop out of the cache (delete).
    #[tokio::test]
    async fn index_cache_drops_deleted_entries() {
        let dir = tempfile::tempdir().unwrap();
        let vfs = Arc::new(InMemoryVfs::new());
        vfs.insert_dir("Taxonomy/Anime");
        vfs.insert_file("Taxonomy/Anime/A.md", "---\ntitle: A\n---\n");
        vfs.insert_file("Taxonomy/Anime/B.md", "---\ntitle: B\n---\n");

        read_library_cached(
            test_config("/virtual-vault"),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();

        vfs.remove_file("Taxonomy/Anime/B.md").await.unwrap();
        let after = read_library_cached(
            test_config("/virtual-vault"),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();
        let titles: Vec<&str> = after
            .summaries()
            .map(|summary| summary.title.as_str())
            .collect();
        assert_eq!(titles, vec!["A"]);
    }

    /// The cached load (cold and warm) is value-equivalent to a full
    /// [`read_library`], including the recomputed relation graph and counts.
    #[tokio::test]
    async fn cached_load_matches_uncached_read_library() {
        let dir = tempfile::tempdir().unwrap();
        let vfs = make_relation_vault(
            "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n\nSee [[Gamma]].\n",
        );

        let cold = read_library_cached(
            relation_test_config(),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();
        let uncached = read_library(relation_test_config(), Arc::clone(&vfs) as Arc<dyn Vfs>)
            .await
            .unwrap();
        assert_libraries_equivalent(&cold, &uncached, "cold-cached", "uncached");

        // A second cached load reuses every entry from the warm cache.
        let warm = read_library_cached(
            relation_test_config(),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();
        assert_libraries_equivalent(&warm, &uncached, "warm-cached", "uncached");
    }

    // --- Daily-note relation reuse on edit -----------------------------------

    fn daily_notes_config() -> KizunaConfig {
        let mut config = relation_test_config();
        config.daily_notes = Some(crate::types::DailyNotesConfig {
            paths: vec!["Journal".to_string()],
            date_format: None,
        });
        config
    }

    fn daily_notes_vault(alpha: &str, journal: &str) -> Arc<InMemoryVfs> {
        let vfs = Arc::new(InMemoryVfs::new());
        vfs.insert_dir("Taxonomy/Anime");
        vfs.insert_file("Taxonomy/Anime/Alpha.md", alpha);
        vfs.insert_file("Taxonomy/Anime/Beta.md", "---\ntitle: Beta\n---\n");
        vfs.insert_dir("Journal");
        vfs.insert_file("Journal/2026-06-16.md", journal);
        vfs
    }

    fn daily_note_relation_count(library: &Library) -> usize {
        library
            .relations
            .iter()
            .filter(|relation| relation.field == "daily-note")
            .count()
    }

    /// An in-place entity edit, recomputed surgically, keeps the daily-note
    /// relations and stays value-equivalent to a full reload.
    #[tokio::test]
    async fn surgical_update_preserves_daily_note_relations() {
        let config = daily_notes_config();
        let vfs = daily_notes_vault("---\ntitle: Alpha\n---\n", "Watched [[Alpha]] today.\n");

        let base = read_library(config.clone(), Arc::clone(&vfs) as Arc<dyn Vfs>)
            .await
            .unwrap();
        assert_eq!(daily_note_relation_count(&base), 1, "fixture sanity");

        vfs.insert_file("Taxonomy/Anime/Alpha.md", "---\ntitle: Alphaz\n---\n");
        let surgical = rebuild_for_edited_entity(&base, vfs.as_ref(), "Taxonomy/Anime/Alpha.md")
            .await
            .unwrap()
            .expect("in-place edit takes the surgical path");
        let full = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
            .await
            .unwrap();

        assert_libraries_equivalent(&surgical, &full, "daily-note base", "daily-note edited");
    }

    /// The surgical path must reuse the resident daily-note relations rather than
    /// re-reading the journal: we drop the link from the daily note on disk *after*
    /// the base load, then edit the entity — the relation survives only if it came
    /// from memory. (This would fail if the edit path re-read daily notes.)
    #[tokio::test]
    async fn surgical_update_reuses_daily_note_relations_without_rereading() {
        let config = daily_notes_config();
        let vfs = daily_notes_vault("---\ntitle: Alpha\n---\n", "Watched [[Alpha]] today.\n");

        let base = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
            .await
            .unwrap();
        assert_eq!(daily_note_relation_count(&base), 1, "fixture sanity");

        // Disk no longer has the link; only memory (base) still does.
        vfs.insert_file("Journal/2026-06-16.md", "Nothing linked today.\n");
        vfs.insert_file("Taxonomy/Anime/Alpha.md", "---\ntitle: Alphaz\n---\n");
        let surgical = rebuild_for_edited_entity(&base, vfs.as_ref(), "Taxonomy/Anime/Alpha.md")
            .await
            .unwrap()
            .expect("in-place edit takes the surgical path");

        assert_eq!(
            daily_note_relation_count(&surgical),
            1,
            "daily-note relations must be reused from memory, not re-read from disk"
        );
    }

    /// A cached cold start reuses an unchanged daily note's links without reading
    /// its body: we edit the journal bytes but keep size + mtime, then assert the
    /// relation persists — only possible if the note was not re-read.
    #[tokio::test]
    async fn index_cache_reuses_daily_note_links_without_rereading() {
        let dir = tempfile::tempdir().unwrap();
        let journal = "Watched [[Alpha]] today\n";
        let vfs = daily_notes_vault("---\ntitle: Alpha\n---\n", journal);

        let first = read_library_cached(
            daily_notes_config(),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();
        assert_eq!(daily_note_relation_count(&first), 1, "fixture sanity");

        // Same byte length (so the size matches) + preserved mtime → a change the
        // fingerprint can't see. A hit must reuse the cached links.
        let linkless = format!("{:width$}\n", "no link", width = journal.len() - 1);
        assert_eq!(linkless.len(), journal.len());
        vfs.overwrite_preserving_stamp("Journal/2026-06-16.md", &linkless);

        let second = read_library_cached(
            daily_notes_config(),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();
        assert_eq!(
            daily_note_relation_count(&second),
            1,
            "unchanged daily-note fingerprint must serve cached links, not re-read"
        );
    }

    /// When a daily note actually changes (mtime bumped), its links are re-read.
    #[tokio::test]
    async fn index_cache_reparses_daily_note_when_fingerprint_changes() {
        let dir = tempfile::tempdir().unwrap();
        let vfs = daily_notes_vault("---\ntitle: Alpha\n---\n", "Watched [[Alpha]] today.\n");

        read_library_cached(
            daily_notes_config(),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();

        // A normal write bumps the mtime, so the note is re-read and the link drops.
        vfs.insert_file("Journal/2026-06-16.md", "Nothing linked today.\n");
        let updated = read_library_cached(
            daily_notes_config(),
            Arc::clone(&vfs) as Arc<dyn Vfs>,
            cache_ctx(dir.path(), "schema-v1"),
        )
        .await
        .unwrap();
        assert_eq!(daily_note_relation_count(&updated), 0);
    }
}
