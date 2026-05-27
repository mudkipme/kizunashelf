use crate::library::compare_string;
use crate::types::{EntitySummary, Library, Relation, RelationDirection};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

pub fn build_relation_hubs(library: &Library) -> Vec<serde_json::Value> {
    let mut grouped: HashMap<String, Vec<Relation>> = HashMap::new();
    for relation in outgoing_relations(library, None) {
        grouped
            .entry(target_key(relation).to_string())
            .or_default()
            .push(relation.clone());
    }

    let mut hubs: Vec<_> = grouped
        .into_iter()
        .map(|(key, relations)| {
            let mut value = build_relation_target_summary(library, &key, &relations);
            value.as_object_mut().unwrap().insert(
                "fields".to_string(),
                serde_json::to_value(count_by(&relations, |relation| relation.field.clone()))
                    .unwrap(),
            );
            value
        })
        .collect();
    hubs.sort_by(|a, b| {
        let count_a = a["count"].as_u64().unwrap_or_default();
        let count_b = b["count"].as_u64().unwrap_or_default();
        count_b.cmp(&count_a).then_with(|| {
            compare_string(
                a["targetTitle"].as_str().unwrap_or_default(),
                b["targetTitle"].as_str().unwrap_or_default(),
            )
        })
    });
    hubs
}

pub fn relation_type_pairs(library: &Library) -> Vec<Count> {
    let by_id = summary_by_id(library);
    count_by(&outgoing_relations(library, None), |relation| {
        let source = by_id.get(&relation.source_id);
        let target_label = relation
            .target_id
            .as_ref()
            .and_then(|target_id| by_id.get(target_id))
            .map(|target| target.type_label.clone())
            .or_else(|| {
                relation
                    .target_type
                    .as_ref()
                    .and_then(|target_type| type_label(library, Some(target_type)))
            });
        format!(
            "{} -> {}",
            source
                .map(|source| source.type_label.as_str())
                .unwrap_or("Unknown"),
            target_label.as_deref().unwrap_or("Unresolved")
        )
    })
}

pub fn relation_fields(library: &Library) -> Vec<String> {
    let mut fields: Vec<String> = library.config.relationship_fields.clone();
    for relation in &library.relations {
        if relation.direction == RelationDirection::Out && !fields.contains(&relation.field) {
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

pub fn build_relation_field_summary(library: &Library, field: &str) -> serde_json::Value {
    let relations = outgoing_relations(library, Some(field));
    let targets = build_relation_targets(library, field);
    let sources: HashSet<_> = relations
        .iter()
        .map(|relation| &relation.source_id)
        .collect();

    serde_json::json!({
        "field": field,
        "edgeCount": relations.len(),
        "sourceCount": sources.len(),
        "uniqueTargets": targets.len(),
        "resolvedTargets": targets.iter().filter(|target| target.get("targetId").is_some()).count(),
        "topTargets": targets.into_iter().take(8).collect::<Vec<_>>(),
    })
}

pub fn build_relation_targets(library: &Library, field: &str) -> Vec<serde_json::Value> {
    let mut grouped: HashMap<String, Vec<Relation>> = HashMap::new();
    for relation in outgoing_relations(library, Some(field)) {
        grouped
            .entry(target_key(relation).to_string())
            .or_default()
            .push(relation.clone());
    }

    let mut targets: Vec<_> = grouped
        .into_iter()
        .map(|(key, relations)| build_relation_target_summary(library, &key, &relations))
        .collect();
    targets.sort_by(|a, b| {
        let count_a = a["count"].as_u64().unwrap_or_default();
        let count_b = b["count"].as_u64().unwrap_or_default();
        count_b.cmp(&count_a).then_with(|| {
            compare_string(
                a["targetTitle"].as_str().unwrap_or_default(),
                b["targetTitle"].as_str().unwrap_or_default(),
            )
        })
    });
    targets
}

pub fn build_relation_target_summary(
    library: &Library,
    key: &str,
    relations: &[Relation],
) -> serde_json::Value {
    let entity_by_id = summary_by_id(library);
    let first = &relations[0];
    let target_entity = first
        .target_id
        .as_ref()
        .and_then(|target_id| entity_by_id.get(target_id));
    let mut sources: Vec<EntitySummary> = relations
        .iter()
        .filter_map(|relation| entity_by_id.get(&relation.source_id).cloned())
        .collect();

    let mut object = serde_json::Map::new();
    object.insert("key".to_string(), key.into());
    object.insert(
        "targetTitle".to_string(),
        target_entity
            .map(|entity| entity.title.as_str())
            .unwrap_or(&first.target_title)
            .into(),
    );
    if let Some(target_id) = &first.target_id {
        object.insert("targetId".to_string(), target_id.clone().into());
    }
    if let Some(target_type) = target_entity
        .map(|entity| entity.entity_type.clone())
        .or_else(|| first.target_type.clone())
    {
        object.insert("targetType".to_string(), target_type.into());
    }
    if let Some(target_type_label) = target_entity
        .map(|entity| entity.type_label.clone())
        .or_else(|| type_label(library, first.target_type.as_ref()))
    {
        object.insert("targetTypeLabel".to_string(), target_type_label.into());
    }
    object.insert("count".to_string(), relations.len().into());
    object.insert(
        "sourceTypes".to_string(),
        serde_json::to_value(count_by(&sources, |entity| entity.type_label.clone())).unwrap(),
    );
    sources = sort_entities(sources, "title", SortDirection::Asc);
    object.insert(
        "examples".to_string(),
        serde_json::to_value(sources.into_iter().take(5).collect::<Vec<_>>()).unwrap(),
    );
    serde_json::Value::Object(object)
}

pub fn target_key(relation: &Relation) -> &str {
    relation
        .target_id
        .as_deref()
        .unwrap_or(&relation.target_title)
}

pub fn summary_by_id(library: &Library) -> HashMap<String, EntitySummary> {
    library
        .summaries
        .iter()
        .map(|entity| (entity.id.clone(), entity.clone()))
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
    let mut counts = Vec::<Count>::new();
    for item in items {
        let key = select(item);
        if let Some(existing) = counts.iter_mut().find(|count| count.name == key) {
            existing.count += 1;
        } else {
            counts.push(Count {
                name: key,
                count: 1,
            });
        }
    }
    counts.sort_by(|a, b| b.count.cmp(&a.count));
    counts
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortDirection {
    Asc,
    Desc,
}

pub fn sort_entities(
    mut entities: Vec<EntitySummary>,
    sort: &str,
    direction: SortDirection,
) -> Vec<EntitySummary> {
    let multiplier = if direction == SortDirection::Asc {
        1
    } else {
        -1
    };
    entities.sort_by(|a, b| {
        let ordering = if sort == "title" {
            compare_string(&a.title, &b.title)
        } else if sort == "status" {
            compare_optional_string(a.status.as_deref(), b.status.as_deref())
        } else if let Some(field) = sort.strip_prefix("date:") {
            compare_optional_string(
                entity_date_sort_value(a, field).as_deref(),
                entity_date_sort_value(b, field).as_deref(),
            )
        } else if sort == "relations" {
            a.relation_count.cmp(&b.relation_count)
        } else if sort == "path" {
            compare_string(&a.path, &b.path)
        } else {
            let type_compare = compare_string(&a.type_label, &b.type_label);
            if type_compare != Ordering::Equal {
                type_compare
            } else {
                compare_string(&a.title, &b.title)
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

pub fn get_status_tracked_type_ids(library: &Library) -> HashSet<String> {
    library
        .config
        .types
        .iter()
        .filter(|entity_type| !entity_type.fields.status.is_empty())
        .map(|entity_type| entity_type.id.clone())
        .collect()
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
