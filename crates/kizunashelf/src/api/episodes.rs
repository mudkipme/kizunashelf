//! The episodes write endpoint: a full rewrite of an entity's episodes section
//! (toggle / add / remove / reorder / rename / regroup). The core renders the
//! groups back into the body — replacing only the episodes section — and writes
//! atomically, revision-guarded, through the same path as entity edits.

use super::entities::build_entity_detail;
use super::error::{ApiError, ApiResult};
use super::mutations::{check_revision, write_entity_raw, EntityPath};
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{EntityDetailResponse, UpdateEpisodesRequest};
use crate::episodes::{apply_episodes, episode_section};
use crate::library::{file_revision, serialize_markdown_document, split_markdown_document};
use axum::extract::{Path as AxumPath, State};
use axum::Json;

pub(crate) async fn update_episodes(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<UpdateEpisodesRequest>,
) -> ApiResult<EntityDetailResponse> {
    let library = require_content_writes(&state).await?;
    let entity_id = path.id;
    let Some(record) = library.record_by_id(&entity_id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let Some(type_config) = library.config.type_config(&record.summary.entity_type) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let Some(section) = episode_section(type_config) else {
        return Err(ApiError::bad_request("This type has no episodes section"));
    };
    let section = section.clone();
    let source_rel = record.summary.path.clone();

    let vfs = state.vault_vfs(&library.config.vault_root);
    let raw = vfs
        .read_to_string(&source_rel)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {source_rel}: {error}"))?;
    // Re-check the revision against the freshly read content (TOCTOU), like entity edits.
    check_revision(&request.revision, &file_revision(&raw))?;

    let mut document = split_markdown_document(&raw);
    document.body = apply_episodes(&document.body, &section, &request.groups);
    let new_raw = serialize_markdown_document(&document.frontmatter, &document.body);
    write_entity_raw(vfs.as_ref(), &source_rel, &new_raw).await?;

    state.invalidate_cache().await;
    let reloaded = get_library(&state).await?;
    let Some(record) = reloaded.record_by_id(&entity_id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    Ok(Json(
        build_entity_detail(&state, &reloaded, &record.summary).await?,
    ))
}
