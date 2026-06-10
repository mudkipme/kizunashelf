mod collation;
mod frontmatter;
mod relations;

pub use collation::{compare_optional_string, compare_string, compare_string_for_title_language};
pub use frontmatter::{
    serialize_markdown_document, split_markdown_document, wikilink_regex, MarkdownDocument,
};

use crate::types::{
    DateRole, Entity, EntitySummary, EntityTypeConfig, FieldType, KizunaConfig, Library,
    LibraryDiagnostic, Relation,
};
use anyhow::{Context, Result};
use frontmatter::{
    date_values, default_title, external_refs, extract_summary, first_string, parse_markdown,
    title_languages,
};
use relations::build_relations;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use tokio::fs;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const DEFAULT_READ_CONCURRENCY: usize = 8;
const MAX_READ_CONCURRENCY: usize = 16;

pub async fn load_config(config_path: impl AsRef<Path>) -> Result<KizunaConfig> {
    let path = config_path.as_ref();
    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read config {}", path.display()))?;
    serde_yaml::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))
}

pub async fn save_config(config_path: impl AsRef<Path>, config: &KizunaConfig) -> Result<()> {
    let path = config_path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }
    let raw = serde_yaml::to_string(config).context("failed to serialize config")?;
    fs::write(path, format!("{raw}\n"))
        .await
        .with_context(|| format!("failed to write config {}", path.display()))
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

pub async fn read_library(config: KizunaConfig) -> Result<Library> {
    validate_library_roots(&config).await?;
    let (mut entities, diagnostics) = read_entities(&config).await?;
    let relations = build_relations(&config, &entities).await?;
    let relation_count_by_id = unique_relation_count_by_id(&relations);

    let summaries: Vec<EntitySummary> = entities
        .iter()
        .map(|entity| {
            let mut summary = entity.summary.clone();
            summary.relation_count = *relation_count_by_id.get(&summary.id).unwrap_or(&0);
            summary
        })
        .collect();
    let relation_count_by_summary_id: HashMap<String, u32> = summaries
        .iter()
        .map(|summary| (summary.id.clone(), summary.relation_count))
        .collect();

    for entity in &mut entities {
        entity.summary.relation_count = *relation_count_by_summary_id
            .get(&entity.summary.id)
            .unwrap_or(&0);
    }

    Ok(Library {
        config,
        entities,
        summaries,
        relations,
        diagnostics,
        generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    })
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

pub async fn read_library_from_config(config_path: impl AsRef<Path>) -> Result<Library> {
    let config = load_config(config_path).await?;
    read_library(config).await
}

async fn validate_library_roots(config: &KizunaConfig) -> Result<()> {
    validate_config_paths(config)?;
    let vault_root = Path::new(&config.vault_root);
    let vault_metadata = fs::metadata(vault_root)
        .await
        .with_context(|| format!("failed to access vault root {}", vault_root.display()))?;
    if !vault_metadata.is_dir() {
        anyhow::bail!("vault root is not a directory: {}", vault_root.display());
    }
    let canonical_vault_root = vault_root
        .canonicalize()
        .with_context(|| format!("failed to resolve vault root {}", vault_root.display()))?;

    let taxonomy_root = vault_root.join(&config.taxonomy_root);
    let taxonomy_metadata = fs::metadata(&taxonomy_root)
        .await
        .with_context(|| format!("failed to access taxonomy root {}", taxonomy_root.display()))?;
    if !taxonomy_metadata.is_dir() {
        anyhow::bail!(
            "taxonomy root is not a directory: {}",
            taxonomy_root.display()
        );
    }
    ensure_path_inside_root(&canonical_vault_root, &taxonomy_root, "taxonomy root")?;
    for type_config in &config.types {
        let path = taxonomy_root.join(&type_config.path);
        ensure_existing_path_inside_root(&canonical_vault_root, &path, "entity type directory")?;
    }
    if let Some(daily_notes) = &config.daily_notes {
        for path in &daily_notes.paths {
            let path = vault_root.join(path);
            ensure_existing_path_inside_root(
                &canonical_vault_root,
                &path,
                "daily notes directory",
            )?;
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

fn ensure_existing_path_inside_root(root: &Path, path: &Path, label: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    ensure_path_inside_root(root, path, label)
}

async fn read_entities(config: &KizunaConfig) -> Result<(Vec<Entity>, Vec<LibraryDiagnostic>)> {
    let mut entities = Vec::new();
    let mut diagnostics = Vec::new();
    for type_config in &config.types {
        let result = read_entities_for_type(config, type_config).await?;
        entities.extend(result.entities);
        diagnostics.extend(result.diagnostics);
    }
    entities.sort_by(|a, b| {
        let type_compare = compare_string(&a.summary.type_label, &b.summary.type_label);
        if !type_compare.is_eq() {
            return type_compare;
        }
        compare_string(&a.summary.title, &b.summary.title)
    });
    Ok((entities, diagnostics))
}

struct EntityReadBatch {
    entities: Vec<Entity>,
    diagnostics: Vec<LibraryDiagnostic>,
}

struct EntityReadResult {
    entity: Entity,
    diagnostics: Vec<LibraryDiagnostic>,
}

async fn read_entities_for_type(
    config: &KizunaConfig,
    type_config: &EntityTypeConfig,
) -> Result<EntityReadBatch> {
    let absolute_dir = Path::new(&config.vault_root)
        .join(&config.taxonomy_root)
        .join(&type_config.path);
    let mut entries = match fs::read_dir(&absolute_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(EntityReadBatch {
                entities: Vec::new(),
                diagnostics: Vec::new(),
            });
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to read taxonomy directory {}",
                    absolute_dir.display()
                )
            });
        }
    };
    let mut file_names = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        let name = entry.file_name().to_string_lossy().to_string();
        if file_type.is_file() && name.ends_with(".md") {
            file_names.push(name);
        }
    }

    let concurrency = effective_read_concurrency(config);
    let semaphore = std::sync::Arc::new(Semaphore::new(concurrency));
    let mut tasks = JoinSet::new();
    for entry in file_names {
        let absolute_path = absolute_dir.join(&entry);
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .context("failed to acquire read concurrency permit")?;
        let vault_root = config.vault_root.clone();
        let type_config = type_config.clone();
        tasks.spawn(async move {
            let _permit = permit;
            read_entity_file(vault_root, type_config, entry, absolute_path).await
        });
    }

    let mut entities = Vec::new();
    let mut diagnostics = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let result = result.context("entity read task failed")??;
        entities.push(result.entity);
        diagnostics.extend(result.diagnostics);
    }
    Ok(EntityReadBatch {
        entities,
        diagnostics,
    })
}

async fn read_entity_file(
    vault_root: String,
    type_config: EntityTypeConfig,
    entry: String,
    absolute_path: PathBuf,
) -> Result<EntityReadResult> {
    let raw = fs::read_to_string(&absolute_path)
        .await
        .with_context(|| format!("failed to read entity {}", absolute_path.display()))?;
    let metadata = fs::metadata(&absolute_path)
        .await
        .with_context(|| format!("failed to stat entity {}", absolute_path.display()))?;
    let revision = file_revision(&raw, &metadata);
    let parsed = parse_markdown(&raw);
    let note_basename = entry.strip_suffix(".md").unwrap_or(&entry).to_string();
    let titles = title_languages(&parsed.frontmatter, &note_basename, &type_config);
    let title = default_title(&parsed.frontmatter, &titles, &note_basename, &type_config);
    let relative_path = relative_path(Path::new(&vault_root), &absolute_path);
    let entity_key = entity_key(&parsed.frontmatter, &note_basename, &type_config);
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
        dates: date_values(&parsed.frontmatter, &date_field_names(&type_config)),
        image: first_field_string_for_types(
            &parsed.frontmatter,
            &type_config,
            &[FieldType::Image, FieldType::ImageList],
        ),
        summary: extract_summary(&parsed.body),
        path: relative_path,
        basename: note_basename,
        external_refs: external_refs(
            &parsed.frontmatter,
            &field_names(&type_config, FieldType::ExternalRef),
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

fn file_revision(raw: &str, metadata: &std::fs::Metadata) -> String {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hasher);
    format!("{:x}-{}-{}", hasher.finish(), metadata.len(), modified)
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

fn effective_read_concurrency(config: &KizunaConfig) -> usize {
    config
        .read_concurrency
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_READ_CONCURRENCY)
        .clamp(1, MAX_READ_CONCURRENCY)
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::{compare_string, read_library};
    use crate::types::{EntityTypeConfig, FieldConfig, FieldType, FilenameConfig, KizunaConfig};

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

        let error = read_library(config).await.unwrap_err().to_string();

        assert!(error.contains("failed to access vault root"));
    }

    #[tokio::test]
    async fn read_library_errors_when_taxonomy_root_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let config = test_config(temp.path().to_string_lossy().as_ref());

        let error = read_library(config).await.unwrap_err().to_string();

        assert!(error.contains("failed to access taxonomy root"));
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

        let library = read_library(config).await.unwrap();

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

    fn test_config(vault_root: &str) -> KizunaConfig {
        KizunaConfig {
            vault_root: vault_root.to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            content_writable: None,
            read_concurrency: None,
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
