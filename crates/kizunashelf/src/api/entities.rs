use super::error::{ApiError, ApiResult};
use super::state::{get_library, AppState};
use crate::calendar::{build_entity_dates, EntityDatesResponse};
use crate::contract::{EntityDetailResponse, EntityListResponse};
use crate::dates::clamp_number;
use crate::library::compare_string_for_title_language;
use crate::relations::{
    sort_entities, sort_entities_with_title_language, summary_by_id, SortDirection,
};
use crate::types::{Entity, EntitySummary, FieldType, Library, Relation, RelationDirection};
use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct EntitiesQuery {
    #[serde(rename = "type")]
    entity_type: Option<String>,
    refs: Option<String>,
    cover: Option<String>,
    sort: Option<String>,
    direction: Option<String>,
    #[serde(rename = "titleLanguage")]
    title_language: Option<String>,
    q: Option<String>,
    relation: Option<String>,
    filters: Option<String>,
    #[serde(rename = "pageSize")]
    page_size: Option<f64>,
    page: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntityFieldFilter {
    field: String,
    values: Vec<String>,
}

pub(crate) async fn entities(
    State(state): State<AppState>,
    Query(query): Query<EntitiesQuery>,
) -> ApiResult<EntityListResponse> {
    let library = get_library(&state).await?;
    let field_filters = parse_entity_field_filters(query.filters.as_deref())?;
    let mut entities: Vec<_> = library.entities.iter().collect();
    if query
        .entity_type
        .as_ref()
        .is_some_and(|entity_type| entity_type != "all")
    {
        entities.retain(|entity| Some(&entity.summary.entity_type) == query.entity_type.as_ref());
    }
    if query.refs.as_deref() == Some("with") {
        entities.retain(|entity| !entity.summary.external_refs.is_empty());
    }
    if query.refs.as_deref() == Some("without") {
        entities.retain(|entity| entity.summary.external_refs.is_empty());
    }
    if query.cover.as_deref() == Some("with") {
        entities.retain(|entity| entity.summary.image.is_some());
    }
    if query.cover.as_deref() == Some("without") {
        entities.retain(|entity| entity.summary.image.is_none());
    }
    if !field_filters.is_empty() {
        entities.retain(|entity| entity_matches_field_filters(entity, &library, &field_filters));
    }
    if let Some(q) = query
        .q
        .as_ref()
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty())
    {
        entities.retain(|entity| {
            [
                Some(entity.summary.title.as_str()),
                entity.summary.summary.as_deref(),
                Some(entity.summary.basename.as_str()),
                Some(entity.summary.path.as_str()),
            ]
            .into_iter()
            .flatten()
            .chain(entity.summary.titles.values().map(|value| value.as_str()))
            .any(|value| value.to_lowercase().contains(&q))
        });
    }
    if let Some(relation) = query
        .relation
        .as_ref()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
    {
        let ids: std::collections::HashSet<_> = library
            .relations
            .iter()
            .filter(|item| {
                item.target_title == relation || item.target_id.as_deref() == Some(relation)
            })
            .map(|item| item.source_id.clone())
            .collect();
        entities.retain(|entity| ids.contains(&entity.summary.id));
    }

    let direction = if query.direction.as_deref() == Some("desc") {
        SortDirection::Desc
    } else {
        SortDirection::Asc
    };
    let mut summaries = entities
        .into_iter()
        .map(|entity| entity.summary.clone())
        .collect::<Vec<_>>();
    summaries = sort_entities_for_entity_list(
        summaries,
        query.sort.as_deref().unwrap_or("type"),
        direction,
        query.title_language.as_deref(),
    );

    let page_size = clamp_number(query.page_size.unwrap_or(40.0), 1, 100);
    let requested_page = clamp_number(query.page.unwrap_or(1.0), 1, i64::MAX);
    let total = summaries.len();
    let total_pages = std::cmp::max(1, ((total as f64) / (page_size as f64)).ceil() as i64);
    let page = requested_page.min(total_pages);
    let start = ((page - 1) * page_size) as usize;
    let items = summaries
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .collect();

    Ok(Json(EntityListResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    }))
}

fn parse_entity_field_filters(filters: Option<&str>) -> Result<Vec<EntityFieldFilter>, ApiError> {
    let Some(filters) = filters.map(str::trim).filter(|filters| !filters.is_empty()) else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<EntityFieldFilter>>(filters)
        .map(|filters| {
            filters
                .into_iter()
                .filter_map(|filter| {
                    let field = filter.field.trim().to_string();
                    let values = filter
                        .values
                        .into_iter()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .collect::<Vec<_>>();
                    (!field.is_empty() && !values.is_empty())
                        .then_some(EntityFieldFilter { field, values })
                })
                .collect()
        })
        .map_err(|_| ApiError::bad_request("Invalid entity filters"))
}

fn entity_matches_field_filters(
    entity: &Entity,
    library: &Library,
    filters: &[EntityFieldFilter],
) -> bool {
    filters.iter().all(|filter| {
        let Some(field_type) = field_type_for_entity_filter(entity, library, &filter.field) else {
            return false;
        };
        let Some(value) = entity.frontmatter.get(&filter.field) else {
            return false;
        };
        field_value_matches_filter(value, field_type, &filter.values)
    })
}

fn field_type_for_entity_filter(
    entity: &Entity,
    library: &Library,
    field: &str,
) -> Option<FieldType> {
    library
        .config
        .types
        .iter()
        .find(|type_config| type_config.id == entity.summary.entity_type)
        .and_then(|type_config| {
            type_config
                .fields
                .iter()
                .find(|field_config| field_config.field == field)
        })
        .map(|field_config| field_config.field_type)
        .filter(|field_type| {
            matches!(
                field_type,
                FieldType::Enum | FieldType::EnumList | FieldType::Bool
            )
        })
}

fn field_value_matches_filter(
    value: &serde_json::Value,
    field_type: FieldType,
    expected: &[String],
) -> bool {
    match field_type {
        FieldType::Enum => frontmatter_scalar_matches_any(value, expected),
        FieldType::EnumList => match value {
            serde_json::Value::Array(items) => items
                .iter()
                .any(|item| frontmatter_scalar_matches_any(item, expected)),
            _ => false,
        },
        FieldType::Bool => match value {
            serde_json::Value::Bool(value) => {
                let value = if *value { "true" } else { "false" };
                expected.iter().any(|item| item == value)
            }
            _ => false,
        },
        _ => false,
    }
}

fn frontmatter_scalar_matches_any(value: &serde_json::Value, expected: &[String]) -> bool {
    match value {
        serde_json::Value::String(value) => expected.iter().any(|item| item == value),
        serde_json::Value::Bool(value) => {
            let value = if *value { "true" } else { "false" };
            expected.iter().any(|item| item == value)
        }
        serde_json::Value::Number(value) => {
            let value = value.to_string();
            expected.iter().any(|item| item == &value)
        }
        _ => false,
    }
}

pub(crate) fn sort_entities_for_entity_list(
    mut entities: Vec<EntitySummary>,
    sort: &str,
    direction: SortDirection,
    title_language: Option<&str>,
) -> Vec<EntitySummary> {
    let explicit_title_language = title_language
        .map(str::trim)
        .filter(|language| !language.is_empty() && *language != "default");
    if sort != "title" {
        return sort_entities_with_title_language(
            entities,
            sort,
            direction,
            explicit_title_language,
        );
    }

    let multiplier = if direction == SortDirection::Asc {
        1
    } else {
        -1
    };

    entities.sort_by(|a, b| {
        let title_a = entity_sort_title(a, explicit_title_language);
        let title_b = entity_sort_title(b, explicit_title_language);
        let ordering = compare_string_for_title_language(title_a, title_b, explicit_title_language);
        if multiplier == 1 {
            ordering
        } else {
            ordering.reverse()
        }
    });
    entities
}

fn entity_sort_title<'entity>(
    entity: &'entity EntitySummary,
    explicit_title_language: Option<&str>,
) -> &'entity str {
    if let Some(language) = explicit_title_language {
        return entity
            .titles
            .get(language)
            .unwrap_or(&entity.title)
            .as_str();
    }

    entity.title.as_str()
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct EntityPath {
    id: String,
}

pub(crate) async fn entity_dates(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
) -> ApiResult<EntityDatesResponse> {
    let library = get_library(&state).await?;
    let Some(entity) = library
        .entities
        .iter()
        .find(|item| item.summary.id == path.id)
    else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let vfs = state.vault_vfs(&library.config.vault_root);
    Ok(Json(
        build_entity_dates(&library, vfs.as_ref(), entity).await?,
    ))
}

pub(crate) async fn entity_detail(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
) -> ApiResult<EntityDetailResponse> {
    let library = get_library(&state).await?;
    let Some(entity) = library
        .entities
        .iter()
        .find(|item| item.summary.id == path.id)
    else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let relations = entity_detail_relations(&library, &entity.summary.id);
    let related_entities = entity_detail_related_entities(&library, &entity.summary.id, &relations);
    Ok(Json(EntityDetailResponse {
        entity: entity.clone(),
        relations,
        related_entities,
    }))
}

pub(crate) fn entity_detail_relations(library: &Library, entity_id: &str) -> Vec<Relation> {
    library
        .relations
        .iter()
        .filter(|relation| {
            relation.field != "daily-note"
                && !relation.source_id.starts_with("daily-note:")
                && (relation.source_id == entity_id
                    || (relation.target_id.as_deref() == Some(entity_id)
                        && relation.direction == RelationDirection::Out
                        && !has_mirrored_incoming_relation(library, entity_id, relation)))
        })
        .cloned()
        .collect()
}

fn has_mirrored_incoming_relation(library: &Library, entity_id: &str, relation: &Relation) -> bool {
    library.relations.iter().any(|candidate| {
        candidate.source_id == entity_id
            && candidate.target_id.as_deref() == Some(relation.source_id.as_str())
            && candidate.field == relation.field
            && candidate.direction == RelationDirection::In
    })
}

fn entity_detail_related_entities(
    library: &Library,
    entity_id: &str,
    relations: &[Relation],
) -> Vec<EntitySummary> {
    let summary_by_id = summary_by_id(library);
    let mut seen = HashSet::new();
    let mut related = Vec::new();
    for relation in relations {
        let related_id = if relation.source_id == entity_id {
            relation.target_id.as_deref()
        } else if relation.target_id.as_deref() == Some(entity_id) {
            Some(relation.source_id.as_str())
        } else {
            None
        };
        let Some(related_id) = related_id else {
            continue;
        };
        if seen.insert(related_id.to_string()) {
            if let Some(summary) = summary_by_id.get(related_id) {
                related.push((*summary).clone());
            }
        }
    }
    sort_entities(related, "title", SortDirection::Asc)
}
