use super::error::{ApiError, ApiResult};
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{EpisodeRef, LogActivityRequest, LogActivityResponse, LogKind, StampedDate};
use crate::daily_notes::{render_log_line, write_log_line, LogWriteError};
use crate::types::{DateRole, KizunaConfig};
use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct LogPath {
    pub(super) id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogQuery {
    /// Preview the write without touching disk. `Option` is already optional to
    /// serde; no `#[serde(default)]` (it makes schemars emit a `null` default that
    /// the zod generator can't apply to a boolean).
    dry_run: Option<bool>,
}

/// Logs an activity for one entity. This phase writes a single daily-note line
/// (side-effect #1) and *reports* the date-stamp / episodes it would also touch
/// (applied in a later phase). `?dryRun=1` previews without writing.
pub(crate) async fn log_activity(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<LogPath>,
    Query(query): Query<LogQuery>,
    Json(request): Json<LogActivityRequest>,
) -> ApiResult<LogActivityResponse> {
    let dry_run = query.dry_run.unwrap_or(false);
    // A real write needs content writes; a dry run is read-only.
    let library = if dry_run {
        get_library(&state).await?
    } else {
        require_content_writes(&state).await?
    };
    let Some(entity) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let Some(resolved) = library
        .config
        .resolve_log_config(&entity.summary.entity_type)
    else {
        return Err(ApiError::bad_request(
            "This entity type is not configured for daily-note logging",
        ));
    };

    let now = chrono::Utc::now();
    let date = request
        .date
        .as_deref()
        .map(str::trim)
        .filter(|date| !date.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| now.format("%Y-%m-%d").to_string());
    let time = now.format("%H:%M").to_string();

    let progress = request
        .episode
        .as_ref()
        .map(|episode| episode.key.as_str())
        .unwrap_or_default();
    let note = request.note.as_deref().unwrap_or_default();
    let line = render_log_line(
        &resolved.line_format,
        &entity.summary.basename,
        progress,
        note,
        &date,
        &time,
    );

    let vfs = state.vault_vfs(&library.config.vault_root);
    let outcome = write_log_line(
        &library.config,
        vfs.as_ref(),
        &date,
        &resolved.section,
        &line,
        dry_run,
    )
    .await
    .map_err(|error| match error {
        LogWriteError::InvalidDate => ApiError::bad_request("Invalid log date"),
        LogWriteError::Conflict => {
            ApiError::conflict("The daily note changed during the write; retry")
        }
        LogWriteError::Vfs(error) => ApiError::from(error),
    })?;

    // A real write changes the vault (the new mention isn't in the resident
    // relation graph), so drop the cache to reindex on the next read.
    if !dry_run && !outcome.line_already_present {
        state.invalidate_cache().await;
    }

    // Computed-and-reported now; applied in a later phase.
    let will_stamp_date = stamp_target(
        &library.config,
        &entity.summary.entity_type,
        request.kind,
        &date,
    );
    let episodes_resolved = request
        .episode
        .as_ref()
        .map(|episode| {
            vec![EpisodeRef {
                key: episode.key.clone(),
                title: String::new(),
            }]
        })
        .unwrap_or_default();

    Ok(Json(LogActivityResponse {
        dry_run,
        note_path: Some(outcome.relative_path),
        note_will_be_created: outcome.note_created,
        section: Some(resolved.section),
        line: Some(line),
        line_already_present: outcome.line_already_present,
        will_stamp_date,
        episodes_resolved,
    }))
}

/// The date field (if any) that `kind` would stamp on this type — reported in the
/// response; the actual write lands in a later phase.
fn stamp_target(
    config: &KizunaConfig,
    entity_type: &str,
    kind: LogKind,
    date: &str,
) -> Option<StampedDate> {
    let role = match kind {
        LogKind::Started => DateRole::Started,
        LogKind::Completed => DateRole::Completed,
        LogKind::Progress => return None,
    };
    let field = config
        .type_config(entity_type)?
        .fields
        .iter()
        .find(|field| field.date_role == Some(role))?;
    Some(StampedDate {
        field: field.field.clone(),
        value: date.to_string(),
    })
}
