//! Batch import: fetch a user's library from an external service (public
//! profile) or a file export (CSV) and create vault entities, reusing the
//! quick-capture pipeline (schema mapping, "in library" dedup, atomic writes,
//! episode import). A `plan` job fetches + resolves items; a `commit` job then
//! creates the approved ones. Jobs live in memory like asset-download jobs;
//! re-running is safe because the dedup gate skips already-created entities.
//! See `docs/batch-import-plan.md`.

mod csv_util;
mod job;
mod model;
mod sources;

use self::job::PlannedItem;
use super::error::{ApiError, ApiResult};
use super::state::{require_content_writes, AppState};
use crate::contract::{
    CommitImportJobRequest, CreateImportJobRequest, ExternalProviderCredentialField,
    ImportInputKind, ImportJob, ImportJobListResponse, ImportJobStatus, ImportSourceCatalogItem,
    ImportSourceCatalogResponse,
};
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use chrono::Utc;
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// In-memory record for a batch import job: the wire job plus the resolved items
/// commit needs (the wire `plan` is a projection of `items`). Like the asset job,
/// it does not survive a restart.
pub(crate) struct ImportJobRecord {
    pub(crate) job: ImportJob,
    pub(in crate::api::import) items: Vec<PlannedItem>,
    pub(in crate::api::import) cancel: Arc<AtomicBool>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ImportJobPath {
    id: String,
}

pub(crate) async fn list_import_sources(
    State(state): State<AppState>,
) -> Json<ImportSourceCatalogResponse> {
    let sources = sources::registry()
        .iter()
        .map(|entry| ImportSourceCatalogItem {
            id: entry.id.to_string(),
            label: entry.label.to_string(),
            input: entry.input,
            input_label: entry.input_label.to_string(),
            providers: entry.providers.iter().map(|p| p.to_string()).collect(),
            credentials: entry
                .credentials
                .iter()
                .map(|credential| ExternalProviderCredentialField {
                    key: credential.key.to_string(),
                    label: credential.label.to_string(),
                    secret: credential.secret,
                    required: credential.required,
                })
                .collect(),
            available: (entry.available)(&state),
            unavailable_reason: (entry.unavailable_reason)(&state),
        })
        .collect();
    Json(ImportSourceCatalogResponse { sources })
}

pub(crate) async fn create_import_job(
    State(state): State<AppState>,
    Json(request): Json<CreateImportJobRequest>,
) -> ApiResult<ImportJob> {
    // Gate the plan too: a read-only vault never even fetches.
    require_content_writes(&state).await?;

    let registry = sources::registry();
    let Some(entry) = registry.iter().find(|entry| entry.id == request.source) else {
        return Err(ApiError::bad_request("Unknown import source"));
    };
    if !(entry.available)(&state) {
        return Err(ApiError::bad_request(
            &(entry.unavailable_reason)(&state)
                .unwrap_or_else(|| "This import source is not available".to_string()),
        ));
    }
    match entry.input {
        ImportInputKind::Profile => {
            if request
                .input
                .username
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err(ApiError::bad_request("A username is required"));
            }
        }
        ImportInputKind::Csv => {
            if request
                .input
                .csv_text
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .is_none()
            {
                return Err(ApiError::bad_request("A CSV export is required"));
            }
        }
    }

    let job = ImportJob {
        id: state.next_import_job_id(),
        source: request.source.clone(),
        status: ImportJobStatus::Queued,
        total: 0,
        processed: 0,
        created: 0,
        skipped: 0,
        needs_review: 0,
        failed: 0,
        episodes_total: None,
        episodes_processed: None,
        errors: Vec::new(),
        started_at: now_iso(),
        finished_at: None,
        plan: None,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let inserted = state
        .insert_import_job_if_idle(ImportJobRecord {
            job: job.clone(),
            items: Vec::new(),
            cancel: Arc::clone(&cancel),
        })
        .await;
    if !inserted {
        return Err(ApiError::conflict("An import job is already running"));
    }

    let fetch = entry.fetch;
    let worker_state = state.clone();
    let job_id = job.id.clone();
    let input = request.input.clone();
    tokio::spawn(async move {
        job::run_plan_job(worker_state, job_id, fetch, input, cancel).await;
    });

    Ok(Json(job))
}

pub(crate) async fn list_import_jobs(
    State(state): State<AppState>,
) -> ApiResult<ImportJobListResponse> {
    let jobs = state.import_jobs().lock().await;
    let mut jobs: Vec<ImportJob> = jobs.values().map(|record| record.job.clone()).collect();
    jobs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(Json(ImportJobListResponse { jobs }))
}

pub(crate) async fn get_import_job(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<ImportJobPath>,
) -> ApiResult<ImportJob> {
    let jobs = state.import_jobs().lock().await;
    jobs.get(&path.id)
        .map(|record| Json(record.job.clone()))
        .ok_or_else(|| ApiError::not_found("Import job not found"))
}

pub(crate) async fn commit_import_job(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<ImportJobPath>,
    Json(request): Json<CommitImportJobRequest>,
) -> ApiResult<ImportJob> {
    require_content_writes(&state).await?;

    // Flip the job to Committing and take its resolved items under one lock.
    let (planned, cancel) = {
        let mut jobs = state.import_jobs().lock().await;
        let Some(record) = jobs.get_mut(&path.id) else {
            return Err(ApiError::not_found("Import job not found"));
        };
        if record.job.status != ImportJobStatus::Planned {
            return Err(ApiError::conflict("Import job is not ready to commit"));
        }
        record.job.status = ImportJobStatus::Committing;
        record.job.finished_at = None;
        (record.items.clone(), Arc::clone(&record.cancel))
    };

    let worker_state = state.clone();
    let job_id = path.id.clone();
    tokio::spawn(async move {
        job::run_commit_job(worker_state, job_id, planned, request, cancel).await;
    });

    let jobs = state.import_jobs().lock().await;
    jobs.get(&path.id)
        .map(|record| Json(record.job.clone()))
        .ok_or_else(|| ApiError::not_found("Import job not found"))
}

pub(crate) async fn cancel_import_job(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<ImportJobPath>,
) -> ApiResult<ImportJob> {
    use std::sync::atomic::Ordering;
    let mut jobs = state.import_jobs().lock().await;
    let Some(record) = jobs.get_mut(&path.id) else {
        return Err(ApiError::not_found("Import job not found"));
    };
    record.cancel.store(true, Ordering::Relaxed);
    if matches!(
        record.job.status,
        ImportJobStatus::Queued | ImportJobStatus::Fetching | ImportJobStatus::Committing
    ) {
        record.job.status = ImportJobStatus::Cancelled;
    }
    Ok(Json(record.job.clone()))
}

pub(super) fn now_iso() -> String {
    Utc::now().to_rfc3339()
}
