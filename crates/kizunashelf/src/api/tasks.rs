//! Body task-item writes: the checkbox toggle behind a `- [ ]` a user typed into
//! an entity's notes. One line is rewritten in place, revision-guarded like every
//! other entity edit. The episodes list has its own section-aware write path
//! (`api/episodes.rs`); this one deliberately never touches it.

use super::entities::{load_entity_detail, EntityPath};
use super::error::{ApiError, ApiResult};
use super::mutations::edit_entity_document;
use super::state::{require_content_writes, AppState};
use crate::body_tasks::set_task_done;
use crate::contract::{EntityDetailResponse, ToggleTaskRequest};
use axum::extract::{Path as AxumPath, State};
use axum::Json;

/// Checks/unchecks one Markdown task item in the entity's body, stamping or
/// clearing its `✅` completion date. The item is located by its line in the
/// rendered notes body plus that line's source text; a mismatch is a 404 rather
/// than a write to the wrong line.
pub(crate) async fn toggle_task(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<ToggleTaskRequest>,
) -> ApiResult<EntityDetailResponse> {
    let library = require_content_writes(&state).await?;
    let entity_id = path.id;
    let Some(record) = library.record_by_id(&entity_id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    // An unconfigured type is fine here — tasks are plain Markdown, not schema.
    // The section (when the type declares one) only marks what to leave alone.
    let section = library
        .config
        .type_config(&record.summary.entity_type)
        .and_then(crate::episodes::episode_section)
        .cloned();
    let source_rel = record.summary.path.clone();

    // The completion date is always the client's local date (never a UTC server
    // clock) — so the `✅` matches the user's day, like the episode checkbox.
    let date = request.date.trim();
    if date.is_empty() {
        return Err(ApiError::bad_request("A date is required"));
    }
    if request.line == 0 {
        return Err(ApiError::bad_request("A task line is required"));
    }

    let vfs = state.vault_vfs(&library.config.vault_root);
    edit_entity_document(
        &state,
        vfs.as_ref(),
        &source_rel,
        &request.revision,
        |document| {
            let Some(body) = set_task_done(
                &document.body,
                section.as_ref(),
                request.line as usize,
                &request.text,
                request.done,
                date,
            ) else {
                return Err(ApiError::not_found("Task not found"));
            };
            document.body = body;
            Ok(())
        },
    )
    .await?;

    Ok(Json(load_entity_detail(&state, &entity_id).await?))
}
