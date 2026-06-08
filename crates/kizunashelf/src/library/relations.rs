use crate::daily_notes::{daily_note_files, normalize_wikilink_target, strip_frontmatter};
use crate::types::{Entity, FieldType, KizunaConfig, Relation, RelationDirection};
use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tokio::fs;

use super::frontmatter::{fence_regex, strip_wikilink, wikilink_regex};

pub(super) async fn build_relations(
    config: &KizunaConfig,
    entities: &[Entity],
) -> Result<Vec<Relation>> {
    let mut by_basename: HashMap<String, Vec<&Entity>> = HashMap::new();
    for entity in entities {
        by_basename
            .entry(normalize_wikilink_target(&entity.summary.basename))
            .or_default()
            .push(entity);
    }

    let mut relations = Vec::new();
    for entity in entities {
        for relation_field in relation_fields(config, &entity.summary.entity_type) {
            for target_title in relation_values(entity.frontmatter.get(&relation_field.field)) {
                let target = find_target(
                    &target_title,
                    relation_field.relation_type.as_deref(),
                    &by_basename,
                );
                relations.push(Relation {
                    source_id: entity.summary.id.clone(),
                    target_id: target.map(|target| target.summary.id.clone()),
                    target_title: target_title.clone(),
                    target_type: target.map(|target| target.summary.entity_type.clone()),
                    field: relation_field.field.clone(),
                    direction: RelationDirection::Out,
                });
                if let Some(target) = target {
                    relations.push(Relation {
                        source_id: target.summary.id.clone(),
                        target_id: Some(entity.summary.id.clone()),
                        target_title: entity.summary.title.clone(),
                        target_type: Some(entity.summary.entity_type.clone()),
                        field: relation_field.field.clone(),
                        direction: RelationDirection::In,
                    });
                }
            }
        }

        for target_title in body_wikilinks(&entity.body) {
            let Some(target) = find_target(&target_title, None, &by_basename) else {
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

    relations.extend(daily_note_relations(config, entities).await?);

    Ok(dedupe_relations(relations))
}

#[derive(Clone)]
struct RelationFieldConfig {
    field: String,
    relation_type: Option<String>,
}

fn relation_fields(config: &KizunaConfig, entity_type: &str) -> Vec<RelationFieldConfig> {
    let type_config = config.types.iter().find(|item| item.id == entity_type);
    let mut fields = Vec::new();
    for field in type_config.into_iter().flat_map(|item| {
        item.fields
            .iter()
            .filter(|field| field.field_type == FieldType::Relation)
    }) {
        if !fields
            .iter()
            .any(|item: &RelationFieldConfig| item.field == field.field)
        {
            fields.push(RelationFieldConfig {
                field: field.field.clone(),
                relation_type: field.relation_type.clone(),
            });
        }
    }
    fields
}

async fn daily_note_relations(config: &KizunaConfig, entities: &[Entity]) -> Result<Vec<Relation>> {
    let by_basename = normalized_entity_basename_index(entities);
    let mut relations = Vec::new();

    for file in daily_note_files(config, None, None, false).await? {
        let source_id = format!("daily-note:{}:{}", file.source_label, file.relative_path);
        let raw = fs::read_to_string(&file.absolute_path).await?;
        for target_title in daily_note_wikilinks(&raw) {
            let Some(target) = find_target_for_wikilink(&target_title, &by_basename) else {
                continue;
            };
            relations.push(Relation {
                source_id: source_id.clone(),
                target_id: Some(target.summary.id.clone()),
                target_title,
                target_type: Some(target.summary.entity_type.clone()),
                field: "daily-note".to_string(),
                direction: RelationDirection::Out,
            });
        }
    }

    Ok(relations)
}

fn daily_note_wikilinks(raw: &str) -> Vec<String> {
    body_wikilinks(&fence_regex().replace_all(&strip_frontmatter(raw), ""))
}

fn normalized_entity_basename_index(entities: &[Entity]) -> HashMap<String, Vec<&Entity>> {
    let mut by_basename: HashMap<String, Vec<&Entity>> = HashMap::new();
    for entity in entities {
        by_basename
            .entry(normalize_wikilink_target(&entity.summary.basename))
            .or_default()
            .push(entity);
    }
    by_basename
}

fn find_target_for_wikilink<'a>(
    target_title: &str,
    by_basename: &HashMap<String, Vec<&'a Entity>>,
) -> Option<&'a Entity> {
    let candidates = by_basename.get(&normalize_wikilink_target(target_title))?;
    candidates.first().copied()
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
    relation_type: Option<&str>,
    by_basename: &HashMap<String, Vec<&'a Entity>>,
) -> Option<&'a Entity> {
    let normalized = normalize_wikilink_target(target_title);
    let candidates = by_basename.get(&normalized)?;
    let path_parts: Vec<_> = target_title
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let type_path = if path_parts.len() > 1 {
        path_parts.get(path_parts.len() - 2)
    } else {
        None
    };
    if let Some(type_path) = type_path {
        if let Some(candidate) = candidates.iter().find(|candidate| {
            candidate
                .summary
                .path
                .split('/')
                .any(|part| part == *type_path)
        }) {
            return Some(*candidate);
        }
    }
    let relation_type = relation_type
        .map(normalize_relation_type)
        .filter(|value| !value.is_empty());
    if let Some(relation_type) = relation_type {
        if let Some(candidate) = candidates.iter().find(|candidate| {
            normalize_relation_type(&candidate.summary.entity_type) == relation_type
                || normalize_relation_type(&candidate.summary.type_label) == relation_type
        }) {
            return Some(*candidate);
        }
    }
    candidates.first().copied()
}

fn normalize_relation_type(value: &str) -> String {
    value
        .chars()
        .filter(|character| !matches!(character, ' ' | '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
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
