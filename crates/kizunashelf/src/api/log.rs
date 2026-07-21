use super::entities::build_entity_detail;
use super::error::{ApiError, ApiResult};
use super::mutations::{check_revision, edit_entity_document_locked};
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{
    FlippedStatus, LogActivityRequest, LogActivityResponse, LogKind, LogOp, StampedDate,
};
use crate::daily_notes::{remove_log_line, render_log_line, write_log_line, LogWriteError};
use crate::library::file_revision;
use crate::types::{CanonicalStatus, DateRole, EntityTypeConfig, ResolvedStatus};
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

/// Logs an activity for one entity, applying up to three side effects, each gated
/// by the schema: a daily-note line (when the type is loggable), a
/// `started`/`completed` frontmatter date stamp (when the type has that `dateRole`
/// field), and a monotonic status flip (when the type has a mapped `enumRole:
/// status` field). `op: remove` is the inverse of the first two; the status flip
/// has no safe inverse, so `remove` never touches status. `?dryRun=true` previews
/// everything without writing. Episode watching is a separate endpoint
/// (`/episodes/watch`); logging never reads or writes the episode list.
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
    let _mutation = if dry_run {
        None
    } else {
        Some(state.content_mutation_lock().await)
    };
    let Some(record) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let entity_type = record.summary.entity_type.clone();
    let basename = record.summary.basename.clone();
    let source_rel = record.summary.path.clone();

    let type_config = library.config.type_config(&entity_type);
    let stamp_field = stamp_target_field(type_config, kind).map(str::to_string);
    // A `started`/`completed` log also promotes the entity's status (schema-gated,
    // monotonic). Only `add` flips — `remove` has no safe inverse and leaves status
    // alone. Computed from the resident status; the entity write is revision-guarded,
    // so the fresh frontmatter matches what we planned against.
    let status_flip = if op == LogOp::Add {
        plan_status_flip(type_config, record.summary.status.as_ref(), kind)
    } else {
        None
    };
    let (log_section, log_line_format) = match library.config.resolve_log_config(&entity_type) {
        Some(resolved) => (Some(resolved.section), Some(resolved.line_format)),
        None => (None, None),
    };

    // A started/completed date stamp and/or a status flip mutate the entity
    // frontmatter (episode ticks moved back to `/episodes/watch`).
    let mutates_entity = stamp_field.is_some() || status_flip.is_some();
    if log_section.is_none() && !mutates_entity {
        return Err(ApiError::bad_request(
            "Nothing to log: this type isn't configured for logging and has no started/completed date field or status field to update",
        ));
    }

    let vfs = state.vault_vfs(&library.config.vault_root);

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

    // The two side effects (a daily-note line and an entity date stamp) touch two
    // files that can't be renamed atomically together. To keep the request
    // all-or-nothing we: (1) preflight the entity revision here, before the note is
    // touched, so the common "stale entity" conflict fails with nothing written;
    // (2) write the daily note first (it's the contended file, edited live in
    // Obsidian); (3) stamp the entity, and if that fails after the line landed, undo
    // the line.
    let revision = if mutates_entity && !dry_run {
        let Some(revision) = request.revision.as_deref() else {
            return Err(ApiError::bad_request(
                "A revision is required to update the entity",
            ));
        };
        let raw = vfs
            .read_to_string(&source_rel)
            .await
            .map_err(|error| anyhow::anyhow!("failed to read entity {source_rel}: {error}"))?;
        check_revision(revision, &file_revision(&raw))?;
        Some(revision.to_string())
    } else {
        None
    };

    // --- Daily-note line first. `add` appends the exact line (idempotent); `remove`
    //     strips the exact match. `line_applied` records whether the file actually
    //     changed, so the date-stamp step below can undo it on failure.
    let mut note_path = None;
    let mut note_will_be_created = false;
    let mut line_already_present = false;
    let mut line_matched = None;
    let mut line_applied = false;
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
                line_applied = !dry_run && !outcome.line_already_present;
            }
            LogOp::Remove => {
                let outcome =
                    remove_log_line(&library.config, vfs.as_ref(), &date, heading, line, dry_run)
                        .await
                        .map_err(map_log_error)?;
                note_path = Some(outcome.relative_path);
                line_matched = Some(outcome.line_matched);
                line_applied = !dry_run && outcome.line_matched;
            }
        }
        if line_applied {
            state.invalidate_cache().await;
        }
    }

    // --- Frontmatter date stamp second, revision-guarded against a fresh read (the
    //     note write widened the window since the preflight). On failure, undo the
    //     daily-note line so nothing is left half-applied.
    let mut entity = None;
    if let Some(revision) = &revision {
        if let Err(error) = stamp_entity_date(
            vfs.as_ref(),
            &source_rel,
            revision,
            stamp_field.as_deref(),
            &date,
            op,
            status_flip.as_ref(),
        )
        .await
        {
            if line_applied {
                if let (Some(line), Some(heading)) = (&line, &log_section) {
                    let undone = match op {
                        LogOp::Add => remove_log_line(
                            &library.config,
                            vfs.as_ref(),
                            &date,
                            heading,
                            line,
                            false,
                        )
                        .await
                        .map(drop),
                        LogOp::Remove => write_log_line(
                            &library.config,
                            vfs.as_ref(),
                            &date,
                            heading,
                            line,
                            false,
                        )
                        .await
                        .map(drop),
                    };
                    state.invalidate_cache().await;
                    if undone.is_err() {
                        return Err(anyhow::anyhow!(
                            "The date stamp failed and the daily-note line couldn't be undone — the log is partially applied; re-check the entity and the daily note."
                        )
                        .into());
                    }
                }
            }
            return Err(error);
        }

        state.invalidate_cache().await;
        let reloaded = get_library(&state).await?;
        if let Some(record) = reloaded.record_by_id(&path.id) {
            entity = Some(build_entity_detail(&state, &reloaded, &record.summary).await?);
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
        will_flip_status: status_flip,
        entity,
    }))
}

/// Applies the `started`/`completed` date stamp to the entity through the
/// already-locked guarded Markdown edit primitive. The caller holds the content
/// mutation lock across both the daily-note side effect and this entity write.
async fn stamp_entity_date(
    vfs: &dyn crate::vfs::Vfs,
    source_rel: &str,
    revision: &str,
    field: Option<&str>,
    date: &str,
    op: LogOp,
    status_flip: Option<&FlippedStatus>,
) -> Result<(), ApiError> {
    edit_entity_document_locked(vfs, source_rel, revision, |document| {
        if let Some(field) = field {
            apply_date_stamp(&mut document.frontmatter, field, date, op);
        }
        // The status flip (`add` only, precomputed as a promotion) writes the
        // mapped value directly into the status field, alongside any date stamp
        // — one atomic entity write.
        if let Some(flip) = status_flip {
            document
                .frontmatter
                .insert(flip.field.clone(), Value::String(flip.value.clone()));
        }
        Ok(())
    })
    .await?;
    Ok(())
}

/// Plans the monotonic status flip for a `started`/`completed` log, or `None` when
/// nothing should change. The engine promotes along `planning → ongoing →
/// completed`, never demotes, and never flips away from `dropped` or over a value
/// it doesn't recognize (an unmapped/hand-set status is preserved). `None` current
/// status (empty field) promotes straight to the target.
fn plan_status_flip(
    type_config: Option<&EntityTypeConfig>,
    current: Option<&ResolvedStatus>,
    kind: LogKind,
) -> Option<FlippedStatus> {
    let target = match kind {
        LogKind::Started => CanonicalStatus::Ongoing,
        LogKind::Completed => CanonicalStatus::Completed,
        LogKind::Progress => return None,
    };
    let target_rank = target.rank()?;
    let field = crate::status::status_field(type_config?)?;
    let write_value = field.status_values.as_ref()?.write_value(target)?;

    match current.and_then(|status| status.canonical) {
        // Never auto-un-drop; dropping is an explicit user decision.
        Some(CanonicalStatus::Dropped) => return None,
        // Known status: promote only when strictly below the target.
        Some(canonical) => {
            if canonical.rank().is_none_or(|rank| rank >= target_rank) {
                return None;
            }
        }
        // A present-but-unmapped value is a deliberate custom status — preserve it
        // (never overwrite an unknown value). Only an *empty* status field (no
        // resolved status at all) is promoted.
        None => {
            if current.is_some() {
                return None;
            }
        }
    }

    // Defensive: nothing to do if the field already holds the target value.
    if current.map(|status| status.value.as_str()) == Some(write_value) {
        return None;
    }

    Some(FlippedStatus {
        field: field.field.clone(),
        value: write_value.to_string(),
        canonical: target,
    })
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
