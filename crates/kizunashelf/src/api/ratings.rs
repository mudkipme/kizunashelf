use super::entities::EntityPath;
use super::entity_edit_review::read_schema;
use super::error::{ApiError, ApiResult};
use super::mutations::edit_entity_document_locked;
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{RatingSnapshot, SetEntityRatingRequest, SetEntityRatingResponse};
use crate::library::load_entity;
use crate::types::FieldType;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;

pub(crate) async fn set_entity_rating(
    State(state): State<AppState>,
    Path(path): Path<EntityPath>,
    Json(request): Json<SetEntityRatingRequest>,
) -> ApiResult<SetEntityRatingResponse> {
    let library = require_content_writes(&state).await?;
    let guard = state.content_mutation_lock().await;
    // A rejected write can reveal externally changed config. Make the explicit
    // client Reload see it even when this host keeps a long-lived index.
    state.invalidate_cache().await;
    let record = library
        .record_by_id(&path.id)
        .ok_or_else(|| ApiError::not_found("Entity not found"))?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let (_, schema) = read_schema(&state, vfs.as_ref()).await?;
    let field = schema
        .types
        .iter()
        .find(|t| t.id == record.summary.entity_type)
        .and_then(|t| {
            t.fields
                .iter()
                .find(|f| f.field == request.field && f.field_type == FieldType::Rating)
        })
        .ok_or_else(|| ApiError::bad_request("This field is not a configured rating"))?;
    if field.rating_max != request.max {
        return Err(ApiError::conflict(
            "The rating scale changed. Reload the item and try again.",
        ));
    }
    if request.restore.is_some() && request.value.is_some() {
        return Err(ApiError::bad_request(
            "Choose a score or restore a previous rating, not both",
        ));
    }
    if let Some(value) = request.value {
        crate::ratings::validate(value, field.rating_max).map_err(ApiError::bad_request)?;
    }
    let (previous, _) = edit_entity_document_locked(
        &guard,
        vfs.as_ref(),
        &record.summary.path,
        &request.revision,
        |document| {
            let previous = RatingSnapshot {
                present: document.frontmatter.contains_key(&request.field),
                value: document
                    .frontmatter
                    .get(&request.field)
                    .cloned()
                    .unwrap_or(Value::Null),
            };
            if let Some(restore) = request.restore {
                if restore.present {
                    document.frontmatter.insert(request.field, restore.value);
                } else {
                    document.frontmatter.remove(&request.field);
                }
            } else if let Some(value) = request.value {
                document
                    .frontmatter
                    .insert(request.field, Value::from(value));
            } else {
                document.frontmatter.remove(&request.field);
            }
            Ok(previous)
        },
    )
    .await?;
    state.invalidate_cache().await;
    let library = get_library(&state).await?;
    let record = library
        .record_by_id(&path.id)
        .ok_or_else(|| ApiError::not_found("Updated entity was not indexed"))?;
    let entity = load_entity(&library.config, vfs.as_ref(), &record.summary).await?;
    Ok(Json(SetEntityRatingResponse { entity, previous }))
}
