//! Transport for the entity list/detail endpoints: parse the request query into
//! domain params, call into [`crate::entities`], and serialize. The filtering,
//! sorting, pagination, and relation walks are pure domain code there.

use super::error::{ApiError, ApiResult};
use super::state::{get_library, AppState};
use crate::calendar::{build_entity_dates, EntityDatesResponse};
use crate::contract::{EntityDetailResponse, EntityListResponse};
use crate::entities::{
    build_entity_list, entity_detail_related_entities, entity_detail_relations, EntityListParams,
};
use crate::library::load_entity;
use crate::relations::SortDirection;
use crate::types::CanonicalStatus;
use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct EntitiesQuery {
    #[serde(rename = "type")]
    entity_type: Option<String>,
    /// Filter to one canonical lifecycle status (`planning`/`ongoing`/…),
    /// resolved per type from its `statusValues` — usable with or without `type`
    /// for cross-type status shelves.
    #[serde(rename = "canonicalStatus")]
    canonical_status: Option<CanonicalStatus>,
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
    let direction = if query.direction.as_deref() == Some("desc") {
        SortDirection::Desc
    } else {
        SortDirection::Asc
    };
    let params = EntityListParams {
        entity_type: query.entity_type.as_deref(),
        canonical_status: query.canonical_status,
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
    // Parse the episodes/tracks section (if the type declares one) from the body,
    // and drop that section from the body shown in the generic "Notes" view so it
    // isn't rendered twice. `entity.body` stays raw — the edit path reads it, so a
    // round-trip through the editor preserves the episodes section verbatim.
    let section = library
        .config
        .type_config(&summary.entity_type)
        .and_then(crate::episodes::episode_section);
    let episodes = section.map(|section| crate::episodes::parse_episodes(&entity.body, section));
    let notes_body = crate::episodes::notes_body(&entity.body, section);
    Ok(EntityDetailResponse {
        entity,
        relations,
        related_entities,
        episodes,
        notes_body,
    })
}
