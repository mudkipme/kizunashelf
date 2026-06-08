use crate::contract::{
    AnalyticsRelationHub, RelationFieldSummary, RelationTargetSummary, RelationTargetTypeSummary,
};
use crate::library::{compare_string, compare_string_for_title_language};
use crate::types::{EntitySummary, Library, Relation, RelationDirection};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

pub fn build_relation_hubs(library: &Library) -> Vec<AnalyticsRelationHub> {
    let entity_by_id = summary_by_id(library);
    let relations = outgoing_relations(library, None);
    build_relation_hubs_from_relations(library, &entity_by_id, &relations)
}

pub fn build_relation_target_type_summaries(library: &Library) -> Vec<RelationTargetTypeSummary> {
    let entity_by_id = summary_by_id(library);
    let mut grouped: HashMap<String, Vec<&Relation>> = HashMap::new();
    for relation in outgoing_relations(library, None) {
        grouped
            .entry(relation_target_type_key(library, relation, &entity_by_id))
            .or_default()
            .push(relation);
    }

    let mut summaries = grouped
        .into_iter()
        .map(|(key, relations)| {
            let targets = build_relation_hubs_from_relations(library, &entity_by_id, &relations);
            RelationTargetTypeSummary {
                target_type: key.clone(),
                type_label: relation_target_type_label(library, &key, &relations, &entity_by_id),
                edge_count: relations.len(),
                unique_targets: targets.len(),
                resolved_targets: targets
                    .iter()
                    .filter(|target| target.target.target_id.is_some())
                    .count(),
                fields: count_by(&relations, |relation| relation.field.clone()),
                top_targets: targets.into_iter().take(8).collect(),
            }
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|a, b| {
        b.edge_count
            .cmp(&a.edge_count)
            .then_with(|| compare_string(&a.type_label, &b.type_label))
    });
    summaries
}

pub fn relation_type_pairs(library: &Library) -> Vec<Count> {
    let by_id = summary_by_id(library);
    count_by(&outgoing_relations(library, None), |relation| {
        let target_label = relation
            .target_id
            .as_ref()
            .and_then(|target_id| by_id.get(target_id.as_str()))
            .map(|target| target.type_label.clone())
            .or_else(|| {
                relation
                    .target_type
                    .as_ref()
                    .and_then(|target_type| type_label(library, Some(target_type)))
            });
        format!(
            "{} -> {}",
            relation_source_type_label(relation, &by_id).as_str(),
            target_label.as_deref().unwrap_or("Unresolved")
        )
    })
}

pub fn relation_fields(library: &Library) -> Vec<String> {
    let mut fields = Vec::new();
    let mut seen = HashSet::new();
    for field in library.config.types.iter().flat_map(|entity_type| {
        entity_type
            .fields
            .iter()
            .filter(|field| field.field_type == crate::types::FieldType::Relation)
            .map(|field| field.field.clone())
    }) {
        if seen.insert(field.clone()) {
            fields.push(field);
        }
    }
    for relation in &library.relations {
        if relation.direction == RelationDirection::Out && seen.insert(relation.field.clone()) {
            fields.push(relation.field.clone());
        }
    }
    fields.sort_by(|a, b| compare_string(a, b));
    fields
}

pub fn outgoing_relations<'a>(library: &'a Library, field: Option<&str>) -> Vec<&'a Relation> {
    library
        .relations
        .iter()
        .filter(|relation| {
            relation.direction == RelationDirection::Out
                && field.map(|field| relation.field == field).unwrap_or(true)
        })
        .collect()
}

pub fn build_relation_field_summary_with_index(
    library: &Library,
    entity_by_id: &HashMap<&str, &EntitySummary>,
    field: &str,
) -> RelationFieldSummary {
    let relations = outgoing_relations(library, Some(field));
    let targets = build_relation_targets_from_relations(library, entity_by_id, &relations);
    let sources: HashSet<_> = relations
        .iter()
        .map(|relation| &relation.source_id)
        .collect();

    RelationFieldSummary {
        field: field.to_string(),
        edge_count: relations.len(),
        source_count: sources.len(),
        unique_targets: targets.len(),
        resolved_targets: targets
            .iter()
            .filter(|target| target.target_id.is_some())
            .count(),
        top_targets: targets.into_iter().take(8).collect(),
    }
}

fn build_relation_targets_from_relations<'a>(
    library: &'a Library,
    entity_by_id: &HashMap<&'a str, &'a EntitySummary>,
    relations: &[&'a Relation],
) -> Vec<RelationTargetSummary> {
    let mut grouped: HashMap<String, Vec<&Relation>> = HashMap::new();
    for relation in relations {
        grouped
            .entry(target_key(relation).to_string())
            .or_default()
            .push(*relation);
    }

    let mut targets: Vec<_> = grouped
        .into_iter()
        .map(|(key, relations)| {
            build_relation_target_summary_with_index(library, entity_by_id, &key, &relations)
        })
        .collect();
    targets.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| compare_string(&a.target_title, &b.target_title))
    });
    targets
}

fn build_relation_hubs_from_relations<'a>(
    library: &'a Library,
    entity_by_id: &HashMap<&'a str, &'a EntitySummary>,
    relations: &[&'a Relation],
) -> Vec<AnalyticsRelationHub> {
    let mut grouped: HashMap<String, Vec<&Relation>> = HashMap::new();
    for relation in relations {
        grouped
            .entry(target_key(relation).to_string())
            .or_default()
            .push(*relation);
    }

    let mut hubs = grouped
        .into_iter()
        .map(|(key, relations)| AnalyticsRelationHub {
            target: build_relation_target_summary_with_index(
                library,
                entity_by_id,
                &key,
                &relations,
            ),
        })
        .collect::<Vec<_>>();
    hubs.sort_by(|a, b| {
        b.target
            .count
            .cmp(&a.target.count)
            .then_with(|| compare_string(&a.target.target_title, &b.target.target_title))
    });
    hubs
}

fn relation_target_type_key(
    library: &Library,
    relation: &Relation,
    entity_by_id: &HashMap<&str, &EntitySummary>,
) -> String {
    relation
        .target_id
        .as_ref()
        .and_then(|target_id| entity_by_id.get(target_id.as_str()))
        .map(|target| target.entity_type.clone())
        .or_else(|| relation.target_type.clone())
        .filter(|target_type| {
            library
                .config
                .types
                .iter()
                .any(|type_config| type_config.id == *target_type)
        })
        .unwrap_or_else(|| "unresolved".to_string())
}

fn relation_target_type_label(
    library: &Library,
    key: &str,
    relations: &[&Relation],
    entity_by_id: &HashMap<&str, &EntitySummary>,
) -> String {
    if key == "unresolved" {
        return "Unresolved".to_string();
    }
    relations
        .iter()
        .find_map(|relation| {
            relation
                .target_id
                .as_ref()
                .and_then(|target_id| entity_by_id.get(target_id.as_str()))
                .map(|target| target.type_label.clone())
                .or_else(|| type_label(library, relation.target_type.as_ref()))
        })
        .unwrap_or_else(|| key.to_string())
}

fn build_relation_target_summary_with_index(
    library: &Library,
    entity_by_id: &HashMap<&str, &EntitySummary>,
    key: &str,
    relations: &[&Relation],
) -> RelationTargetSummary {
    let first = &relations[0];
    let target_entity = first
        .target_id
        .as_ref()
        .and_then(|target_id| entity_by_id.get(target_id.as_str()));
    let mut sources: Vec<EntitySummary> = relations
        .iter()
        .filter_map(|relation| {
            entity_by_id
                .get(relation.source_id.as_str())
                .map(|entity| (*entity).clone())
        })
        .collect();

    let target_type = target_entity
        .map(|entity| entity.entity_type.clone())
        .or_else(|| first.target_type.clone());
    let target_type_label = target_entity
        .map(|entity| entity.type_label.clone())
        .or_else(|| type_label(library, first.target_type.as_ref()));
    sources = sort_entities(sources, "title", SortDirection::Asc);
    RelationTargetSummary {
        key: key.to_string(),
        target_title: target_entity
            .map(|entity| entity.title.clone())
            .unwrap_or_else(|| first.target_title.clone()),
        target_id: first.target_id.clone(),
        target_type,
        target_type_label,
        count: relations.len(),
        source_types: count_by(relations, |relation| {
            relation_source_type_label(relation, entity_by_id)
        }),
        examples: sources.into_iter().take(5).collect(),
    }
}

pub fn relation_source_type_label(
    relation: &Relation,
    entity_by_id: &HashMap<&str, &EntitySummary>,
) -> String {
    entity_by_id
        .get(relation.source_id.as_str())
        .map(|source| source.type_label.clone())
        .unwrap_or_else(|| {
            if relation.source_id.starts_with("daily-note:") {
                "Daily Note".to_string()
            } else {
                "Unknown".to_string()
            }
        })
}

pub fn target_key(relation: &Relation) -> &str {
    relation
        .target_id
        .as_deref()
        .unwrap_or(&relation.target_title)
}

pub fn summary_by_id(library: &Library) -> HashMap<&str, &EntitySummary> {
    library
        .summaries
        .iter()
        .map(|entity| (entity.id.as_str(), entity))
        .collect()
}

pub fn type_label(library: &Library, entity_type: Option<&String>) -> Option<String> {
    let entity_type = entity_type?;
    library
        .config
        .types
        .iter()
        .find(|item| item.id == *entity_type)
        .map(|item| item.label.clone())
        .or_else(|| Some(entity_type.clone()))
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct Count {
    pub name: String,
    pub count: usize,
}

pub fn count_by<T, F>(items: &[T], select: F) -> Vec<Count>
where
    F: Fn(&T) -> String,
{
    let mut counts = HashMap::<String, usize>::new();
    for item in items {
        let key = select(item);
        *counts.entry(key).or_insert(0) += 1;
    }
    let mut counts = counts
        .into_iter()
        .map(|(name, count)| Count { name, count })
        .collect::<Vec<_>>();
    counts.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| compare_string(&a.name, &b.name))
    });
    counts
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortDirection {
    Asc,
    Desc,
}

pub fn sort_entities(
    entities: Vec<EntitySummary>,
    sort: &str,
    direction: SortDirection,
) -> Vec<EntitySummary> {
    sort_entities_with_title_language(entities, sort, direction, None)
}

pub fn sort_entities_with_title_language(
    mut entities: Vec<EntitySummary>,
    sort: &str,
    direction: SortDirection,
    title_language: Option<&str>,
) -> Vec<EntitySummary> {
    let multiplier = if direction == SortDirection::Asc {
        1
    } else {
        -1
    };
    entities.sort_by(|a, b| {
        let ordering = if sort == "title" {
            compare_entity_title(a, b, title_language)
        } else if let Some(field) = sort.strip_prefix("date:") {
            compare_optional_string(
                entity_date_sort_value(a, field).as_deref(),
                entity_date_sort_value(b, field).as_deref(),
            )
        } else if sort == "relationCount" {
            a.relation_count.cmp(&b.relation_count)
        } else if sort == "path" {
            compare_string(&a.path, &b.path)
        } else {
            let type_compare = compare_string(&a.type_label, &b.type_label);
            if type_compare != Ordering::Equal {
                type_compare
            } else {
                compare_entity_title(a, b, title_language)
            }
        };
        if multiplier == 1 {
            ordering
        } else {
            ordering.reverse()
        }
    });
    entities
}

fn compare_entity_title(
    a: &EntitySummary,
    b: &EntitySummary,
    title_language: Option<&str>,
) -> Ordering {
    let title_a = title_language
        .and_then(|language| a.titles.get(language))
        .unwrap_or(&a.title);
    let title_b = title_language
        .and_then(|language| b.titles.get(language))
        .unwrap_or(&b.title);
    compare_string_for_title_language(title_a, title_b, title_language)
}

fn compare_optional_string(a: Option<&str>, b: Option<&str>) -> Ordering {
    crate::library::compare_optional_string(a, b)
}

fn entity_date_sort_value(entity: &EntitySummary, field: &str) -> Option<String> {
    entity
        .dates
        .iter()
        .find(|item| item.field == field)
        .and_then(|item| crate::dates::date_sort_key(Some(&item.value)))
}
