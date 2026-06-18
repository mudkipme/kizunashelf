mod collation;
mod frontmatter;
mod relations;

pub use collation::{compare_optional_string, compare_string, compare_string_for_title_language};
pub use frontmatter::{
    serialize_markdown_document, split_markdown_document, wikilink_regex, MarkdownDocument,
};

use crate::types::{
    AppConfig, DateRole, Entity, EntityRecord, EntitySummary, EntityTypeConfig, FieldType,
    KizunaConfig, Library, LibraryDiagnostic, Relation, VaultConfig,
};
use crate::vfs::{NativeVfs, Vfs, VfsError};
use anyhow::{Context, Result};
use frontmatter::{
    date_values, default_title, external_refs, extract_summary, first_string, parse_markdown,
    title_languages,
};
use relations::{build_relations, extract_body_links};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

/// Vault-relative location of the vault config file inside `<vaultRoot>`.
pub const VAULT_CONFIG_RELATIVE_PATH: &str = ".kizunashelf/config.yaml";

/// Resolves the vault config path (`<vaultRoot>/.kizunashelf/config.yaml`) for
/// the given app config. Returns `None` when no vault root is configured yet.
pub fn vault_config_path(app: &AppConfig) -> Option<PathBuf> {
    let vault_root = app.vault_root.trim();
    if vault_root.is_empty() {
        return None;
    }
    Some(Path::new(vault_root).join(VAULT_CONFIG_RELATIVE_PATH))
}

pub async fn load_app_config(config_path: impl AsRef<Path>) -> Result<AppConfig> {
    let path = config_path.as_ref();
    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read app config {}", path.display()))?;
    serde_yaml::from_str(&raw).with_context(|| format!("invalid app config {}", path.display()))
}

pub async fn load_vault_config(config_path: impl AsRef<Path>) -> Result<VaultConfig> {
    let path = config_path.as_ref();
    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read vault config {}", path.display()))?;
    serde_yaml::from_str(&raw).with_context(|| format!("invalid vault config {}", path.display()))
}

/// Writes the vault config to `.kizunashelf/config.yaml` inside the vault through
/// the VFS (the iOS settings path).
pub async fn save_vault_config_via_vfs(vfs: &dyn Vfs, config: &VaultConfig) -> Result<()> {
    let raw = serde_yaml::to_string(config).context("failed to serialize vault config")?;
    vfs.write(VAULT_CONFIG_RELATIVE_PATH, format!("{raw}\n").as_bytes())
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

async fn write_yaml(path: &Path, raw: String, label: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {label} directory {}", parent.display()))?;
    }
    fs::write(path, format!("{raw}\n"))
        .await
        .with_context(|| format!("failed to write {label} {}", path.display()))
}

pub async fn save_app_config(config_path: impl AsRef<Path>, config: &AppConfig) -> Result<()> {
    let raw = serde_yaml::to_string(config).context("failed to serialize app config")?;
    write_yaml(config_path.as_ref(), raw, "app config").await
}

pub async fn save_vault_config(config_path: impl AsRef<Path>, config: &VaultConfig) -> Result<()> {
    let raw = serde_yaml::to_string(config).context("failed to serialize vault config")?;
    write_yaml(config_path.as_ref(), raw, "vault config").await
}

/// Ensures the vault root directory exists, creating it if necessary. Used by
/// onboarding's app-only save (web "create if missing", desktop "create new
/// vault") before any vault config is written.
pub async fn ensure_vault_root(app: &AppConfig) -> Result<()> {
    let vault_root = app.vault_root.trim();
    if vault_root.is_empty() {
        anyhow::bail!("vaultRoot is required");
    }
    fs::create_dir_all(vault_root)
        .await
        .with_context(|| format!("failed to create vault root {vault_root}"))
}

pub async fn ensure_config_directories(config: &KizunaConfig) -> Result<()> {
    validate_config_paths(config)?;
    let vault_root = Path::new(&config.vault_root);
    fs::create_dir_all(vault_root)
        .await
        .with_context(|| format!("failed to create vault root {}", vault_root.display()))?;
    let canonical_vault_root = vault_root
        .canonicalize()
        .with_context(|| format!("failed to resolve vault root {}", vault_root.display()))?;
    let taxonomy_root = vault_root.join(&config.taxonomy_root);
    fs::create_dir_all(&taxonomy_root)
        .await
        .with_context(|| format!("failed to create taxonomy root {}", taxonomy_root.display()))?;
    ensure_path_inside_root(&canonical_vault_root, &taxonomy_root, "taxonomy root")?;
    for type_config in &config.types {
        let path = taxonomy_root.join(&type_config.path);
        fs::create_dir_all(&path).await.with_context(|| {
            format!("failed to create entity type directory {}", path.display())
        })?;
        ensure_path_inside_root(&canonical_vault_root, &path, "entity type directory")?;
    }
    if let Some(daily_notes) = &config.daily_notes {
        for path in &daily_notes.paths {
            let path = vault_root.join(path);
            fs::create_dir_all(&path).await.with_context(|| {
                format!("failed to create daily notes directory {}", path.display())
            })?;
            ensure_path_inside_root(&canonical_vault_root, &path, "daily notes directory")?;
        }
    }
    Ok(())
}

pub async fn read_library(config: KizunaConfig, vfs: Arc<dyn Vfs>) -> Result<Library> {
    validate_library_roots(&config, vfs.as_ref()).await?;
    // Per-directory streaming: bodies/raw are extracted into compact records +
    // body-link lists and dropped before the next directory, so resident memory
    // never holds the whole vault's content.
    let (mut records, body_links, diagnostics) = read_entities(&config, &vfs).await?;
    let body_links: HashMap<String, Vec<String>> = body_links.into_iter().collect();
    let relations = build_relations(&config, &records, &body_links, vfs.as_ref()).await?;
    let relation_count_by_id = unique_relation_count_by_id(&relations);

    for record in &mut records {
        record.summary.relation_count = *relation_count_by_id.get(&record.summary.id).unwrap_or(&0);
    }
    let summaries: Vec<EntitySummary> = records
        .iter()
        .map(|record| record.summary.clone())
        .collect();

    Ok(Library {
        config,
        records,
        summaries,
        relations,
        diagnostics,
        generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    })
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

pub async fn read_library_from_config(app_config_path: impl AsRef<Path>) -> Result<Library> {
    let app = load_app_config(app_config_path).await?;
    let vault_path = vault_config_path(&app)
        .context("config does not set a vault root; run onboarding to create one")?;
    let vault = load_vault_config(&vault_path).await?;
    let config = KizunaConfig::from_parts(app, vault);
    let vfs: Arc<dyn Vfs> = Arc::new(NativeVfs::new(&config.vault_root));
    read_library(config, vfs).await
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

fn ensure_path_inside_root(root: &Path, path: &Path, label: &str) -> Result<()> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to resolve {label} {}", path.display()))?;
    if !canonical.starts_with(root) {
        anyhow::bail!("{label} is outside vaultRoot: {}", path.display());
    }
    Ok(())
}

/// Reads every entity, returning the slim resident records, the per-entity body
/// wikilink targets (id → targets, for relation building), and any diagnostics.
/// Each type directory is read and reduced to records before the next, so the
/// full file contents of at most one directory are in memory at a time.
async fn read_entities(
    config: &KizunaConfig,
    vfs: &Arc<dyn Vfs>,
) -> Result<(
    Vec<EntityRecord>,
    Vec<(String, Vec<String>)>,
    Vec<LibraryDiagnostic>,
)> {
    let mut records = Vec::new();
    let mut body_links = Vec::new();
    let mut diagnostics = Vec::new();
    for type_config in &config.types {
        let result = read_entities_for_type(config, type_config, vfs).await?;
        records.extend(result.records);
        body_links.extend(result.body_links);
        diagnostics.extend(result.diagnostics);
    }
    records.sort_by(|a, b| {
        let type_compare = compare_string(&a.summary.type_label, &b.summary.type_label);
        if !type_compare.is_eq() {
            return type_compare;
        }
        compare_string(&a.summary.title, &b.summary.title)
    });
    Ok((records, body_links, diagnostics))
}

struct EntityReadBatch {
    records: Vec<EntityRecord>,
    body_links: Vec<(String, Vec<String>)>,
    diagnostics: Vec<LibraryDiagnostic>,
}

struct EntityReadResult {
    entity: Entity,
    diagnostics: Vec<LibraryDiagnostic>,
}

async fn read_entities_for_type(
    config: &KizunaConfig,
    type_config: &EntityTypeConfig,
    vfs: &Arc<dyn Vfs>,
) -> Result<EntityReadBatch> {
    let relative_dir = format!(
        "{}/{}",
        config.taxonomy_root.trim_end_matches('/'),
        type_config.path
    );
    let entries = match vfs.read_dir(&relative_dir).await {
        Ok(entries) => entries,
        Err(VfsError::NotFound) => {
            return Ok(EntityReadBatch {
                records: Vec::new(),
                body_links: Vec::new(),
                diagnostics: Vec::new(),
            });
        }
        Err(error) => {
            return Err(anyhow::anyhow!(
                "failed to read taxonomy directory {relative_dir}: {error}"
            ));
        }
    };
    let md_paths: Vec<String> = entries
        .into_iter()
        .filter(|entry| entry.is_file && entry.name.ends_with(".md"))
        .map(|entry| format!("{relative_dir}/{}", entry.name))
        .collect();

    // One batched read for the whole type directory, rather than a read (and a
    // stat) per file — the per-call FFI + file-coordination overhead on iOS makes
    // per-file round trips the dominant load cost.
    let files = vfs
        .read_files(&md_paths)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entities in {relative_dir}: {error}"))?;

    let mut records = Vec::new();
    let mut body_links = Vec::new();
    let mut diagnostics = Vec::new();
    for (relative_path, bytes) in files {
        let result = parse_entity(type_config, relative_path, bytes)?;
        diagnostics.extend(result.diagnostics);
        let entity = result.entity;
        // Extract body wikilinks now, while the body is in hand, so it can be
        // dropped along with `raw` instead of staying resident.
        let links = extract_body_links(&entity.body);
        if !links.is_empty() {
            body_links.push((entity.summary.id.clone(), links));
        }
        records.push(EntityRecord {
            summary: entity.summary,
            revision: entity.revision,
            frontmatter: entity.frontmatter,
        });
    }
    Ok(EntityReadBatch {
        records,
        body_links,
        diagnostics,
    })
}

/// Parses one entity from its raw bytes — no I/O. The revision is derived from
/// the content (hash + length); a separate `metadata` call for the mtime is not
/// worth its per-file cost, and the content hash already detects edits.
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
    let title = default_title(&parsed.frontmatter, &titles, &note_basename, type_config);
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
    use super::{compare_string, load_entity, read_library};
    use crate::types::{EntityTypeConfig, FieldConfig, FieldType, FilenameConfig, KizunaConfig};
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
        assert!(library.summaries.is_empty());
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
                default_title: None,
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
            .summaries
            .iter()
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

        assert_eq!(library.summaries.len(), 1);
        let summary = &library.summaries[0];
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
        let summary = &library.summaries[0];

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
                    default_title: false,
                }),
                body_mappings: Vec::new(),
                fields: vec![FieldConfig {
                    field: "title".to_string(),
                    field_type: FieldType::Title,
                    display_name: Some("Title".to_string()),
                    title_language: Some("zh".to_string()),
                    title_role: None,
                    external_fields: Vec::new(),
                    default_title: Some(true),
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
}
