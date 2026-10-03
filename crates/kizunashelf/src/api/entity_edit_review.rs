//! Schema-aware three-way editor review. Never writes vault content or guesses
//! which file replaced a missing entity. Arrays/objects and the note body are
//! indivisible conflict values, so overlap always requires an explicit choice.
use super::entities::EntityPath;
use super::error::{ApiError, ApiResult};
use super::frontmatter_draft::normalize_existing_draft;
use super::mutations::sanitize_basename;
use super::state::{get_library, AppState};
use crate::contract::{EntityEditDraft, EntityEditReviewRequest, EntityEditReviewResponse};
use crate::library::{
    file_revision, inspect_vault_config_text, load_entity, read_raw_vault_config_via_vfs,
    VaultConfigInspection,
};
use crate::types::{Entity, EntityTypeConfig, VaultConfig};
use crate::vfs::Vfs;
use axum::extract::{Path, State};
use axum::Json;
use std::collections::BTreeSet;

pub(super) async fn read_schema(
    state: &AppState,
    vfs: &dyn Vfs,
) -> Result<(String, VaultConfig), ApiError> {
    let raw = read_raw_vault_config_via_vfs(vfs)
        .await?
        .ok_or_else(|| ApiError::conflict("The vault configuration is missing"))?;
    match inspect_vault_config_text(Some(&raw), &state.app_config()) {
        VaultConfigInspection::Ready(config) => Ok((file_revision(&raw), *config)),
        VaultConfigInspection::Invalid(error) => Err(ApiError::bad_request(&error)),
        VaultConfigInspection::Missing => {
            Err(ApiError::conflict("The vault configuration is missing"))
        }
    }
}

pub(crate) async fn review_entity_edit(
    State(state): State<AppState>,
    Path(path): Path<EntityPath>,
    Json(request): Json<EntityEditReviewRequest>,
) -> ApiResult<EntityEditReviewResponse> {
    let _guard = state.content_mutation_lock().await;
    let vfs = state.vault_vfs(&state.app_config().vault_root);
    let (schema_revision, schema) = read_schema(&state, vfs.as_ref()).await?;
    // A review is an explicit fresh read, including in native engines with a long TTL.
    state.invalidate_cache().await;
    let library = get_library(&state).await?;
    let record = library
        .record_by_id(&path.id)
        .ok_or_else(|| ApiError::not_found("Entity not found"))?;
    if record.summary.entity_type != request.entity_type {
        return Err(ApiError::bad_request(
            "This draft belongs to another collection",
        ));
    }
    let config = schema
        .types
        .iter()
        .find(|config| config.id == request.entity_type)
        .ok_or_else(|| ApiError::conflict("This collection is no longer configured"))?;
    let current = load_entity(&library.config, vfs.as_ref(), &record.summary).await?;
    let (after, _) = read_schema(&state, vfs.as_ref()).await?;
    if after != schema_revision {
        return Err(ApiError::conflict(
            "The vault schema changed during review. Try again.",
        ));
    }
    Ok(Json(review(config, current, request, schema_revision)?))
}

fn review(
    config: &EntityTypeConfig,
    current: Entity,
    request: EntityEditReviewRequest,
    schema_revision: String,
) -> Result<EntityEditReviewResponse, ApiError> {
    let base = normalize_existing_draft(
        request.baseline.frontmatter.clone(),
        config,
        &request.baseline.frontmatter,
    );
    let local_fields = normalize_existing_draft(
        request.draft.frontmatter,
        config,
        &request.baseline.frontmatter,
    );
    let latest =
        normalize_existing_draft(current.frontmatter.clone(), config, &current.frontmatter);
    let local = EntityEditDraft {
        basename: sanitize_basename(&request.draft.basename)
            .map_err(|e| ApiError::bad_request(&e.to_string()))?,
        body: request.draft.body,
        frontmatter: local_fields,
    };
    let mut merged = EntityEditDraft {
        basename: current.summary.basename.clone(),
        body: current.body.clone(),
        frontmatter: current.frontmatter.clone(),
    };
    let mut conflict_fields = Vec::new();
    for key in base
        .keys()
        .chain(local.frontmatter.keys())
        .collect::<BTreeSet<_>>()
    {
        let original = base.get(key);
        let mine = local.frontmatter.get(key);
        let theirs = latest.get(key);
        if mine == original || mine == theirs {
            continue;
        }
        if theirs != original {
            conflict_fields.push(key.clone());
            continue;
        }
        match mine {
            Some(value) => {
                merged.frontmatter.insert(key.clone(), value.clone());
            }
            None => {
                merged.frontmatter.remove(key);
            }
        }
    }
    let body_conflict = merge_text(&request.baseline.body, &local.body, &mut merged.body);
    let name_conflict = merge_text(
        &request.baseline.basename,
        &local.basename,
        &mut merged.basename,
    );
    Ok(EntityEditReviewResponse {
        entity: current,
        local,
        merged,
        conflict_fields,
        body_conflict,
        name_conflict,
        schema_revision,
    })
}

fn merge_text(base: &str, local: &str, latest: &mut String) -> bool {
    if local == base || local == latest {
        return false;
    }
    if latest != base {
        return true;
    }
    *latest = local.to_string();
    false
}
