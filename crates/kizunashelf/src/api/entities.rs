use super::error::{ApiError, ApiResult};
use super::state::{get_library, AppState};
use crate::calendar::{build_entity_dates, EntityDatesResponse};
use crate::contract::{EntityDetailResponse, EntityListResponse};
use crate::dates::clamp_number;
use crate::library::{compare_string_for_title_language, effective_default_title_language};
use crate::relations::{
    sort_entities, sort_entities_with_title_language, summary_by_id, SortDirection,
};
use crate::types::{EntitySummary, Library, Relation, RelationDirection};
use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize, JsonSchema)]
pub(crate) struct EntitiesQuery {
    #[serde(rename = "type")]
    entity_type: Option<String>,
    status: Option<String>,
    refs: Option<String>,
    cover: Option<String>,
    sort: Option<String>,
    direction: Option<String>,
    #[serde(rename = "titleLanguage")]
    title_language: Option<String>,
    q: Option<String>,
    relation: Option<String>,
    #[serde(rename = "pageSize")]
    page_size: Option<f64>,
    page: Option<f64>,
}

pub(crate) async fn entities(
    State(state): State<AppState>,
    Query(query): Query<EntitiesQuery>,
) -> ApiResult<EntityListResponse> {
    let library = get_library(&state).await?;
    let mut entities = library.summaries.clone();
    if query
        .entity_type
        .as_ref()
        .is_some_and(|entity_type| entity_type != "all")
    {
        entities.retain(|entity| Some(&entity.entity_type) == query.entity_type.as_ref());
    }
    if query.status.as_ref().is_some_and(|status| status != "all") {
        if query.status.as_deref() == Some("Unknown") {
            entities.retain(|entity| entity.status.is_none());
        } else {
            entities.retain(|entity| entity.status.as_ref() == query.status.as_ref());
        }
    }
    if query.refs.as_deref() == Some("with") {
        entities.retain(|entity| !entity.external_refs.is_empty());
    }
    if query.refs.as_deref() == Some("without") {
        entities.retain(|entity| entity.external_refs.is_empty());
    }
    if query.cover.as_deref() == Some("with") {
        entities.retain(|entity| entity.image.is_some());
    }
    if query.cover.as_deref() == Some("without") {
        entities.retain(|entity| entity.image.is_none());
    }
    if let Some(q) = query
        .q
        .as_ref()
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty())
    {
        entities.retain(|entity| {
            [
                Some(entity.title.as_str()),
                entity.subtitle.as_deref(),
                entity.summary.as_deref(),
                Some(entity.basename.as_str()),
                Some(entity.path.as_str()),
            ]
            .into_iter()
            .flatten()
            .chain(entity.titles.values().map(|value| value.as_str()))
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
        entities.retain(|entity| ids.contains(&entity.id));
    }

    let direction = if query.direction.as_deref() == Some("desc") {
        SortDirection::Desc
    } else {
        SortDirection::Asc
    };
    entities = sort_entities_for_entity_list(
        &library,
        entities,
        query.sort.as_deref().unwrap_or("type"),
        direction,
        query.title_language.as_deref(),
    );

    let page_size = clamp_number(query.page_size.unwrap_or(40.0), 1, 100);
    let requested_page = clamp_number(query.page.unwrap_or(1.0), 1, i64::MAX);
    let total = entities.len();
    let total_pages = std::cmp::max(1, ((total as f64) / (page_size as f64)).ceil() as i64);
    let page = requested_page.min(total_pages);
    let start = ((page - 1) * page_size) as usize;
    let items = entities
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

pub(crate) fn sort_entities_for_entity_list(
    library: &Library,
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

    let default_title_languages: HashMap<_, _> = library
        .config
        .types
        .iter()
        .filter_map(|item| {
            effective_default_title_language(item).map(|language| (item.id.clone(), language))
        })
        .collect();
    let multiplier = if direction == SortDirection::Asc {
        1
    } else {
        -1
    };

    entities.sort_by(|a, b| {
        let (title_a, language_a) =
            entity_sort_title(a, explicit_title_language, &default_title_languages);
        let (title_b, language_b) =
            entity_sort_title(b, explicit_title_language, &default_title_languages);
        let language = (language_a == language_b).then_some(language_a).flatten();
        let ordering = compare_string_for_title_language(title_a, title_b, language);
        if multiplier == 1 {
            ordering
        } else {
            ordering.reverse()
        }
    });
    entities
}

fn entity_sort_title<'entity, 'language>(
    entity: &'entity EntitySummary,
    explicit_title_language: Option<&'language str>,
    default_title_languages: &'language HashMap<String, String>,
) -> (&'entity str, Option<&'language str>) {
    if let Some(language) = explicit_title_language {
        return (
            entity
                .titles
                .get(language)
                .unwrap_or(&entity.title)
                .as_str(),
            Some(language),
        );
    }

    (
        entity.title.as_str(),
        default_title_languages
            .get(&entity.entity_type)
            .map(|language| language.as_str()),
    )
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
    Ok(Json(build_entity_dates(&library, entity).await?))
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
