use crate::types::{
    Entity, EntityDateValue, EntitySummary, EntityTypeConfig, KizunaConfig, Library, Relation,
    RelationDirection,
};
use anyhow::{Context, Result};
use regex::Regex;
use serde_json::{Map, Number, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
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
    serde_json::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))
}

pub async fn read_library(config: KizunaConfig) -> Result<Library> {
    validate_library_roots(&config).await?;
    let mut entities = read_entities(&config).await?;
    let relations = build_relations(&config, &entities);
    let mut relation_count_by_id: HashMap<String, u32> = HashMap::new();

    for relation in &relations {
        *relation_count_by_id
            .entry(relation.source_id.clone())
            .or_insert(0) += 1;
        if let Some(target_id) = &relation.target_id {
            *relation_count_by_id.entry(target_id.clone()).or_insert(0) += 1;
        }
    }

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
        generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    })
}

pub async fn read_library_from_config(config_path: impl AsRef<Path>) -> Result<Library> {
    let config = load_config(config_path).await?;
    read_library(config).await
}

async fn validate_library_roots(config: &KizunaConfig) -> Result<()> {
    let vault_root = Path::new(&config.vault_root);
    let vault_metadata = fs::metadata(vault_root)
        .await
        .with_context(|| format!("failed to access vault root {}", vault_root.display()))?;
    if !vault_metadata.is_dir() {
        anyhow::bail!("vault root is not a directory: {}", vault_root.display());
    }

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

    Ok(())
}

fn summary_for(entity: &Entity) -> EntitySummary {
    entity.summary.clone()
}

async fn read_entities(config: &KizunaConfig) -> Result<Vec<Entity>> {
    let mut entities = Vec::new();
    for type_config in &config.types {
        entities.extend(read_entities_for_type(config, type_config).await?);
    }
    entities.sort_by(|a, b| {
        let type_compare = compare_string(&a.summary.type_label, &b.summary.type_label);
        if !type_compare.is_eq() {
            return type_compare;
        }
        compare_string(&a.summary.title, &b.summary.title)
    });
    Ok(entities)
}

async fn read_entities_for_type(
    config: &KizunaConfig,
    type_config: &EntityTypeConfig,
) -> Result<Vec<Entity>> {
    let absolute_dir = Path::new(&config.vault_root)
        .join(&config.taxonomy_root)
        .join(&type_config.path);
    let mut entries = match fs::read_dir(&absolute_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
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
    while let Some(result) = tasks.join_next().await {
        entities.push(result.context("entity read task failed")??);
    }
    Ok(entities)
}

async fn read_entity_file(
    vault_root: String,
    type_config: EntityTypeConfig,
    entry: String,
    absolute_path: PathBuf,
) -> Result<Entity> {
    let raw = fs::read_to_string(&absolute_path)
        .await
        .with_context(|| format!("failed to read entity {}", absolute_path.display()))?;
    let parsed = parse_markdown(&raw);
    let note_basename = entry.strip_suffix(".md").unwrap_or(&entry).to_string();
    let titles = title_languages(&parsed.frontmatter, &note_basename, &type_config);
    let title = default_title(&titles, &note_basename, &type_config);
    let relative_path = relative_path(Path::new(&vault_root), &absolute_path);

    let summary = EntitySummary {
        id: format!("{}:{note_basename}", type_config.id),
        entity_type: type_config.id.clone(),
        type_label: type_config.label.clone(),
        title,
        titles,
        subtitle: first_string(&parsed.frontmatter, &type_config.fields.subtitle),
        status: first_string(&parsed.frontmatter, &type_config.fields.status),
        dates: date_values(&parsed.frontmatter, &type_config.fields.date),
        image: first_string(&parsed.frontmatter, &type_config.fields.image),
        summary: extract_summary(&parsed.body),
        path: relative_path,
        basename: note_basename,
        external_refs: external_refs(&parsed.frontmatter, &type_config.fields.external_refs),
        relation_count: 0,
    };

    Ok(Entity {
        summary,
        frontmatter: parsed.frontmatter,
        body: parsed.body,
        raw,
    })
}

fn effective_read_concurrency(config: &KizunaConfig) -> usize {
    config
        .read_concurrency
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_READ_CONCURRENCY)
        .clamp(1, MAX_READ_CONCURRENCY)
}

struct ParsedMarkdown {
    frontmatter: Map<String, Value>,
    body: String,
}

fn parse_markdown(raw: &str) -> ParsedMarkdown {
    if !raw.starts_with("---\n") {
        return ParsedMarkdown {
            frontmatter: Map::new(),
            body: raw.trim().to_string(),
        };
    }
    let Some(end) = raw[4..].find("\n---").map(|index| index + 4) else {
        return ParsedMarkdown {
            frontmatter: Map::new(),
            body: raw.trim().to_string(),
        };
    };
    let yaml_text = &raw[4..end];
    let body = raw[end + 4..].trim().to_string();
    let frontmatter = serde_yaml::from_str::<serde_yaml::Value>(yaml_text)
        .ok()
        .and_then(yaml_to_json_value)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    ParsedMarkdown { frontmatter, body }
}

fn yaml_to_json_value(value: serde_yaml::Value) -> Option<Value> {
    match value {
        serde_yaml::Value::Null => Some(Value::Null),
        serde_yaml::Value::Bool(value) => Some(Value::Bool(value)),
        serde_yaml::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Some(Value::Number(Number::from(value)))
            } else if let Some(value) = value.as_u64() {
                Some(Value::Number(Number::from(value)))
            } else {
                value.as_f64().and_then(Number::from_f64).map(Value::Number)
            }
        }
        serde_yaml::Value::String(value) => Some(Value::String(value)),
        serde_yaml::Value::Sequence(values) => Some(Value::Array(
            values.into_iter().filter_map(yaml_to_json_value).collect(),
        )),
        serde_yaml::Value::Mapping(mapping) => {
            let mut map = Map::new();
            for (key, value) in mapping {
                let serde_yaml::Value::String(key) = key else {
                    continue;
                };
                if let Some(value) = yaml_to_json_value(value) {
                    map.insert(key, value);
                }
            }
            Some(Value::Object(map))
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json_value(tagged.value),
    }
}

fn external_refs(frontmatter: &Map<String, Value>, keys: &[String]) -> BTreeMap<String, String> {
    let mut refs = BTreeMap::new();
    for key in keys {
        if let Some(value) = first_string(frontmatter, std::slice::from_ref(key)) {
            refs.insert(key.clone(), value);
        }
    }
    refs
}

fn date_values(frontmatter: &Map<String, Value>, keys: &[String]) -> Vec<EntityDateValue> {
    let mut dates = Vec::new();
    for field in keys {
        for value in normalize_values(frontmatter.get(field)) {
            dates.push(EntityDateValue {
                field: field.clone(),
                value,
            });
        }
    }
    dates
}

fn first_string(frontmatter: &Map<String, Value>, keys: &[String]) -> Option<String> {
    keys.iter()
        .filter_map(|key| normalize_value(frontmatter.get(key)))
        .next()
}

fn first_string_with_basename(
    frontmatter: &Map<String, Value>,
    keys: &[String],
    basename: &str,
) -> Option<String> {
    keys.iter()
        .filter_map(|key| normalize_title_field(frontmatter, key, basename))
        .next()
}

fn title_languages(
    frontmatter: &Map<String, Value>,
    basename: &str,
    type_config: &EntityTypeConfig,
) -> BTreeMap<String, String> {
    type_config
        .fields
        .title_languages
        .iter()
        .filter_map(|(language, fields)| {
            first_string_with_basename(frontmatter, fields, basename)
                .map(|title| (language.clone(), title))
        })
        .collect()
}

pub fn effective_default_title_language(type_config: &EntityTypeConfig) -> Option<String> {
    type_config
        .default_title_language
        .as_ref()
        .filter(|language| type_config.fields.title_languages.contains_key(*language))
        .cloned()
        .or_else(|| type_config.fields.title_languages.keys().next().cloned())
}

fn default_title(
    titles: &BTreeMap<String, String>,
    basename: &str,
    type_config: &EntityTypeConfig,
) -> String {
    effective_default_title_language(type_config)
        .and_then(|language| titles.get(&language).cloned())
        .or_else(|| titles.values().next().cloned())
        .unwrap_or_else(|| basename.to_string())
}

fn normalize_title_field(
    frontmatter: &Map<String, Value>,
    key: &str,
    basename: &str,
) -> Option<String> {
    if matches!(key, "filename" | "basename" | "$filename" | "$basename") {
        return Some(basename.to_string());
    }
    normalize_value(frontmatter.get(key))
}

fn normalize_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Null => None,
        Value::Bool(value) => Some(if *value { "Yes" } else { "No" }.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) if value.is_empty() => None,
        Value::String(value) => Some(strip_wikilink(value)),
        Value::Array(values) => {
            let normalized: Vec<_> = values
                .iter()
                .filter_map(|item| normalize_value(Some(item)))
                .collect();
            (!normalized.is_empty()).then(|| normalized.join(", "))
        }
        Value::Object(_) => None,
    }
}

fn normalize_values(value: Option<&Value>) -> Vec<String> {
    match value {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(value)) if value.is_empty() => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .flat_map(|item| normalize_values(Some(item)))
            .collect(),
        Some(value) => normalize_value(Some(value)).into_iter().collect(),
    }
}

fn strip_wikilink(value: &str) -> String {
    let value = value.trim();
    strip_wikilink_regex()
        .captures(value)
        .and_then(|captures| captures.get(2).or_else(|| captures.get(1)))
        .map(|capture| capture.as_str().to_string())
        .unwrap_or_else(|| value.to_string())
}

fn extract_summary(body: &str) -> Option<String> {
    let source = summary_heading_regex()
        .find(body)
        .map(|mat| &body[mat.end()..])
        .unwrap_or(body);
    let source = source
        .split_once("\n## ")
        .map(|(head, _)| head)
        .unwrap_or(source);
    let mut cleaned = fence_regex().replace_all(source, "").to_string();
    cleaned = image_markdown_regex().replace_all(&cleaned, "").to_string();
    cleaned = markdown_link_regex().replace_all(&cleaned, "").to_string();
    cleaned = wikilink_regex().replace_all(&cleaned, "$1").to_string();
    let cleaned = cleaned
        .lines()
        .map(|line| {
            line.trim_start_matches(|c: char| "-*># ".contains(c))
                .trim()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        None
    } else if cleaned.chars().count() > 220 {
        Some(format!(
            "{}...",
            cleaned.chars().take(220).collect::<String>()
        ))
    } else {
        Some(cleaned)
    }
}

fn build_relations(config: &KizunaConfig, entities: &[Entity]) -> Vec<Relation> {
    let mut by_basename: HashMap<String, Vec<&Entity>> = HashMap::new();
    for entity in entities {
        by_basename
            .entry(entity.summary.basename.clone())
            .or_default()
            .push(entity);
    }

    let mut relations = Vec::new();
    for entity in entities {
        for field in relation_fields(config, &entity.summary.entity_type) {
            for target_title in relation_values(entity.frontmatter.get(&field)) {
                let target = find_target(&target_title, &by_basename);
                relations.push(Relation {
                    source_id: entity.summary.id.clone(),
                    target_id: target.map(|target| target.summary.id.clone()),
                    target_title: target_title.clone(),
                    target_type: target.map(|target| target.summary.entity_type.clone()),
                    field: field.clone(),
                    direction: RelationDirection::Out,
                });
                if let Some(target) = target {
                    relations.push(Relation {
                        source_id: target.summary.id.clone(),
                        target_id: Some(entity.summary.id.clone()),
                        target_title: entity.summary.title.clone(),
                        target_type: Some(entity.summary.entity_type.clone()),
                        field: field.clone(),
                        direction: RelationDirection::In,
                    });
                }
            }
        }

        for target_title in body_wikilinks(&entity.body) {
            let Some(target) = find_target(&target_title, &by_basename) else {
                continue;
            };
            if target.summary.id == entity.summary.id {
                continue;
            }
            relations.push(Relation {
                source_id: entity.summary.id.clone(),
                target_id: Some(target.summary.id.clone()),
                target_title,
                target_type: Some(target.summary.entity_type.clone()),
                field: "body".to_string(),
                direction: RelationDirection::Out,
            });
        }
    }

    dedupe_relations(relations)
}

fn relation_fields(config: &KizunaConfig, entity_type: &str) -> Vec<String> {
    let type_config = config.types.iter().find(|item| item.id == entity_type);
    let mut fields = Vec::new();
    for field in config.relationship_fields.iter().chain(
        type_config
            .into_iter()
            .flat_map(|item| item.fields.relations.iter()),
    ) {
        if !fields.contains(field) {
            fields.push(field.clone());
        }
    }
    fields
}

fn relation_values(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(values)) => values
            .iter()
            .flat_map(|item| relation_values(Some(item)))
            .collect(),
        Some(Value::String(value)) => {
            let matches: Vec<_> = wikilink_regex()
                .captures_iter(value)
                .filter_map(|captures| captures.get(1))
                .map(|capture| capture.as_str().trim().to_string())
                .filter(|value| !value.is_empty())
                .collect();
            if matches.is_empty() {
                let stripped = strip_wikilink(value.trim());
                (!stripped.is_empty())
                    .then_some(stripped)
                    .into_iter()
                    .collect()
            } else {
                matches
            }
        }
        _ => Vec::new(),
    }
}

fn body_wikilinks(body: &str) -> Vec<String> {
    wikilink_regex()
        .captures_iter(body)
        .filter_map(|captures| captures.get(1))
        .map(|capture| capture.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn find_target<'a>(
    target_title: &str,
    by_basename: &HashMap<String, Vec<&'a Entity>>,
) -> Option<&'a Entity> {
    let candidates = by_basename.get(target_title)?;
    candidates
        .iter()
        .find(|candidate| candidate.summary.entity_type == "franchise")
        .copied()
        .or_else(|| candidates.first().copied())
}

fn dedupe_relations(relations: Vec<Relation>) -> Vec<Relation> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for relation in relations {
        let key = format!(
            "{}\0{}\0{}\0{}\0{:?}",
            relation.source_id,
            relation.target_id.clone().unwrap_or_default(),
            relation.target_title,
            relation.field,
            relation.direction
        );
        if seen.insert(key) {
            deduped.push(relation);
        }
    }
    deduped
}

pub fn to_summary(entity: &Entity) -> EntitySummary {
    summary_for(entity)
}

pub fn compare_string(a: &str, b: &str) -> std::cmp::Ordering {
    collator().compare(a, b).then_with(|| a.cmp(b))
}

fn collator() -> &'static icu_collator::CollatorBorrowed<'static> {
    use icu_collator::{options::CollatorOptions, CollatorBorrowed};
    use icu_locale_core::locale;
    static COLLATOR: OnceLock<CollatorBorrowed<'static>> = OnceLock::new();
    COLLATOR.get_or_init(|| {
        CollatorBorrowed::try_new(locale!("zh-Hans-CN").into(), CollatorOptions::default()).unwrap()
    })
}

pub fn compare_optional_string(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(a), Some(b)) => compare_string(a, b),
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn strip_wikilink_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|([^\]]+))?\]\]$").unwrap())
}

pub fn wikilink_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]").unwrap())
}

fn summary_heading_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?im)^##\s+(摘要|概览|简介|Summary)\s*$").unwrap())
}

fn fence_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)```.*?```").unwrap())
}

fn image_markdown_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"!\[[^\]]*\]\([^)]+\)").unwrap())
}

fn markdown_link_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[[^\]]+\]\([^)]+\)").unwrap())
}

#[cfg(test)]
mod tests {
    use super::{compare_string, read_library};
    use crate::types::{EntityFields, EntityTypeConfig, KizunaConfig};

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

    fn test_config(vault_root: &str) -> KizunaConfig {
        KizunaConfig {
            vault_root: vault_root.to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            relationship_fields: Vec::new(),
            read_concurrency: None,
            home: None,
            daily_notes: None,
            types: vec![EntityTypeConfig {
                id: "anime".to_string(),
                label: "Anime".to_string(),
                icon: None,
                path: "Anime".to_string(),
                default_title_language: Some("primary".to_string()),
                fields: EntityFields {
                    title_languages: std::collections::BTreeMap::from([(
                        "primary".to_string(),
                        vec!["title".to_string()],
                    )]),
                    subtitle: Vec::new(),
                    image: Vec::new(),
                    status: Vec::new(),
                    date: Vec::new(),
                    external_refs: Vec::new(),
                    relations: Vec::new(),
                },
            }],
        }
    }
}
