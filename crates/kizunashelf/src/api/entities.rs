//! Transport for the entity list/detail endpoints: parse the request query into
//! domain params, call into [`crate::entities`], and serialize. The filtering,
//! sorting, pagination, and relation walks are pure domain code there.

use super::error::{ApiError, ApiResult};
use super::state::{get_library, AppState};
use crate::calendar::{build_entity_dates, EntityDatesResponse};
use crate::contract::{EntityDetailResponse, EntityListResponse};
use crate::entities::{
    build_entity_list, entity_detail_related_entities, entity_detail_relations,
    parse_entity_field_filters, EntityListParams,
};
use crate::library::load_entity;
use crate::relations::SortDirection;
use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;

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

pub(crate) async fn entities(
    State(state): State<AppState>,
    Query(query): Query<EntitiesQuery>,
) -> ApiResult<EntityListResponse> {
    let library = get_library(&state).await?;
    let field_filters = parse_entity_field_filters(query.filters.as_deref())
        .map_err(|message| ApiError::bad_request(&message))?;
    let direction = if query.direction.as_deref() == Some("desc") {
        SortDirection::Desc
    } else {
        SortDirection::Asc
    };
    let params = EntityListParams {
        entity_type: query.entity_type.as_deref(),
        refs: query.refs.as_deref(),
        cover: query.cover.as_deref(),
        field_filters,
        query: query.q.as_deref(),
        relation: query.relation.as_deref(),
        sort: query.sort.as_deref().unwrap_or("type"),
        direction,
        title_language: query.title_language.as_deref(),
        page: query.page.unwrap_or(1.0),
        page_size: query.page_size.unwrap_or(40.0),
    };
    Ok(Json(build_entity_list(&library, &params)))
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
    let Some(record) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let vfs = state.vault_vfs(&library.config.vault_root);
    Ok(Json(
        build_entity_dates(&library, vfs.as_ref(), &record.summary).await?,
    ))
}

pub(crate) async fn entity_detail(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
) -> ApiResult<EntityDetailResponse> {
    let library = get_library(&state).await?;
    let Some(record) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    Ok(Json(
        build_entity_detail(&state, &library, &record.summary).await?,
    ))
}

/// Assembles the full entity-detail response (relations, related entities, the
/// on-demand-loaded body, and the parsed episodes section). Shared by the detail
/// `GET` and the episodes write so both return the identical shape.
pub(super) async fn build_entity_detail(
    state: &AppState,
    library: &crate::types::Library,
    summary: &crate::types::EntitySummary,
) -> Result<EntityDetailResponse, ApiError> {
    let relations = entity_detail_relations(library, &summary.id);
    let related_entities = entity_detail_related_entities(library, &summary.id, &relations);
    // The full body/raw is not resident; load it from disk on demand.
    let vfs = state.vault_vfs(&library.config.vault_root);
    let entity = load_entity(&library.config, vfs.as_ref(), summary).await?;
    // Parse the episodes/tracks section (if the type declares one) from the body.
    let episodes = library
        .config
        .type_config(&summary.entity_type)
        .and_then(crate::episodes::episode_section)
        .map(|section| crate::episodes::parse_episodes(&entity.body, section));
    Ok(EntityDetailResponse {
        entity,
        relations,
        related_entities,
        episodes,
    })
}
