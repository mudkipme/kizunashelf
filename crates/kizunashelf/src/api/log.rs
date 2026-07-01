use super::entities::build_entity_detail;
use super::error::{ApiError, ApiResult};
use super::mutations::{check_revision, write_entity_raw};
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{LogActivityRequest, LogActivityResponse, LogKind, LogOp, StampedDate};
use crate::daily_notes::{remove_log_line, render_log_line, write_log_line, LogWriteError};
use crate::library::{file_revision, serialize_markdown_document, split_markdown_document};
use crate::types::{DateRole, EntityTypeConfig};
use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};

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

/// Logs an activity for one entity, applying up to two side effects, each gated by
/// the schema: a daily-note line (when the type is loggable) and a
/// `started`/`completed` frontmatter date stamp (when the type has that `dateRole`
/// field). `op: remove` is the exact inverse. `?dryRun=true` previews everything
/// without writing. Episode watching is a separate endpoint (`/episodes/watch`);
/// logging never reads or writes the episode list.
pub(crate) async fn log_activity(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<LogPath>,
    Query(query): Query<LogQuery>,
    Json(request): Json<LogActivityRequest>,
) -> ApiResult<LogActivityResponse> {
    let dry_run = query.dry_run.unwrap_or(false);
    let op = request.op.unwrap_or_default();
    let kind = request.kind.unwrap_or_default();
    let library = if dry_run {
        get_library(&state).await?
    } else {
        require_content_writes(&state).await?
    };
    let Some(record) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let entity_type = record.summary.entity_type.clone();
    let basename = record.summary.basename.clone();
    let source_rel = record.summary.path.clone();

    let type_config = library.config.type_config(&entity_type);
    let stamp_field = stamp_target_field(type_config, kind).map(str::to_string);
    let (log_section, log_line_format) = match library.config.resolve_log_config(&entity_type) {
        Some(resolved) => (Some(resolved.section), Some(resolved.line_format)),
        None => (None, None),
    };

    // Only a started/completed date stamp mutates the entity now (episode ticks
    // moved back to `/episodes/watch`).
    let mutates_entity = stamp_field.is_some();
    if log_section.is_none() && !mutates_entity {
        return Err(ApiError::bad_request(
            "Nothing to log: this type isn't configured for logging and has no started/completed date field to stamp",
        ));
    }

    let vfs = state.vault_vfs(&library.config.vault_root);

    // Read the body once — it backs the guarded date-stamp mutation.
    let raw = if mutates_entity {
        Some(
            vfs.read_to_string(&source_rel)
                .await
                .map_err(|error| anyhow::anyhow!("failed to read entity {source_rel}: {error}"))?,
        )
    } else {
        None
    };

    // The log's date is always client-supplied — the user's local date, so the
    // server never assumes "today" in UTC and past actions can be logged.
    let Some(date) = request
        .date
        .as_deref()
        .map(str::trim)
        .filter(|date| !date.is_empty())
    else {
        return Err(ApiError::bad_request("A date is required"));
    };
    let date = date.to_string();

    // The rendered line (only when the type is loggable).
    let note = request.note.as_deref().unwrap_or("");
    let line = log_line_format
        .as_deref()
        .map(|format| render_log_line(format, &basename, note, &date));

    // --- Frontmatter date stamp (started/completed): one guarded write.
    let mut entity = None;
    if mutates_entity && !dry_run {
        let raw = raw.as_deref().unwrap();
        let Some(revision) = request.revision.as_deref() else {
            return Err(ApiError::bad_request(
                "A revision is required to stamp a date",
            ));
        };
        check_revision(revision, &file_revision(raw))?;
        let mut document = split_markdown_document(raw);
        if let Some(field) = &stamp_field {
            apply_date_stamp(&mut document.frontmatter, field, &date, op);
        }
        let new_raw = serialize_markdown_document(&document.frontmatter, &document.body);
        write_entity_raw(vfs.as_ref(), &source_rel, &new_raw).await?;

        state.invalidate_cache().await;
        let reloaded = get_library(&state).await?;
        if let Some(record) = reloaded.record_by_id(&path.id) {
            entity = Some(build_entity_detail(&state, &reloaded, &record.summary).await?);
        }
    }

    // --- Daily-note line: appended on `add`, removed on `remove`.
    let mut note_path = None;
    let mut note_will_be_created = false;
    let mut line_already_present = false;
    let mut line_matched = None;
    if let (Some(line), Some(heading)) = (&line, &log_section) {
        match op {
            LogOp::Add => {
                let outcome =
                    write_log_line(&library.config, vfs.as_ref(), &date, heading, line, dry_run)
                        .await
                        .map_err(map_log_error)?;
                note_path = Some(outcome.relative_path);
                note_will_be_created = outcome.note_created;
                line_already_present = outcome.line_already_present;
                if !dry_run && !outcome.line_already_present {
                    state.invalidate_cache().await;
                }
            }
            LogOp::Remove => {
                let outcome =
                    remove_log_line(&library.config, vfs.as_ref(), &date, heading, line, dry_run)
                        .await
                        .map_err(map_log_error)?;
                note_path = Some(outcome.relative_path);
                line_matched = Some(outcome.line_matched);
                if !dry_run && outcome.line_matched {
                    state.invalidate_cache().await;
                }
            }
        }
    }

    let will_stamp_date = stamp_field.map(|field| StampedDate {
        field,
        value: date.clone(),
    });

    Ok(Json(LogActivityResponse {
        dry_run,
        note_path,
        note_will_be_created,
        section: log_section,
        line,
        line_already_present,
        line_matched,
        will_stamp_date,
        entity,
    }))
}

/// The first dated field a `started`/`completed` log would stamp, by `DateRole`.
fn stamp_target_field(type_config: Option<&EntityTypeConfig>, kind: LogKind) -> Option<&str> {
    let role = match kind {
        LogKind::Started => DateRole::Started,
        LogKind::Completed => DateRole::Completed,
        LogKind::Progress => return None,
    };
    type_config?
        .fields
        .iter()
        .find(|field| field.date_role == Some(role))
        .map(|field| field.field.as_str())
}

/// Sets the frontmatter `field` to `date` (`add`), or clears it only when it
/// equals `date` (`remove` — never clobbers a different hand-set value).
fn apply_date_stamp(frontmatter: &mut Map<String, Value>, field: &str, date: &str, op: LogOp) {
    match op {
        LogOp::Add => {
            frontmatter.insert(field.to_string(), Value::String(date.to_string()));
        }
        LogOp::Remove => {
            if frontmatter.get(field).and_then(Value::as_str) == Some(date) {
                frontmatter.remove(field);
            }
        }
    }
}

fn map_log_error(error: LogWriteError) -> ApiError {
    match error {
        LogWriteError::InvalidDate => ApiError::bad_request("Invalid log date"),
        LogWriteError::Conflict => {
            ApiError::conflict("The daily note changed during the write; retry")
        }
        LogWriteError::Vfs(error) => ApiError::from(error),
    }
}
