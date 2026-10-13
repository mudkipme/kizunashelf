//! Batch import: fetch a user's library from an external service (public
//! profile) or a file export (CSV) and create vault entities, reusing the
//! quick-capture pipeline (schema mapping, "in library" dedup, atomic writes,
//! episode import). A `plan` job fetches + resolves items; a `commit` job then
//! creates the approved ones. Native hosts can persist input, review, creation
//! intents, and enrichment checkpoints in host-private storage. Other runtimes
//! retain memory-only jobs. Opaque review snapshots support host presentation
//! recovery; the core owns execution replay and its conditional vault writes.

mod csv_util;
mod job;
mod model;
pub(crate) mod recovery;
mod snapshot;
mod sources;
pub(crate) use snapshot::{export_import_plan, restore_import_plan};

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

/// A batch import record: the wire job, resolved review, and execution ledger.
/// Native journals serialize portable checkpoints; cancellation and worker
/// ownership remain process-local. The wire `plan` is a projection of `items`.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct ImportJobRecord {
    #[serde(default)]
    pub(crate) input: Option<crate::contract::ImportInput>,
    #[serde(default)]
    pub(crate) commit: Option<CommitImportJobRequest>,
    #[serde(default)]
    pub(crate) outcomes: Vec<Option<ItemOutcome>>,
    #[serde(default)]
    pub(crate) creations: std::collections::HashMap<usize, CreationCheckpoint>,
    #[serde(skip)]
    pub(crate) running: bool,
    pub(crate) job: ImportJob,
    pub(in crate::api::import) items: Vec<PlannedItem>,
    pub(in crate::api::import) schema: Option<serde_json::Value>,
    #[serde(skip, default = "new_cancel")]
    pub(crate) cancel: Arc<AtomicBool>,
}

pub(crate) async fn import_cancelled(cancel: &AtomicBool) {
    while !cancel.load(std::sync::atomic::Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

fn new_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ItemOutcome {
    Created,
    Skipped,
    Failed,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct CreationCheckpoint {
    #[serde(default)]
    pub candidate: Option<crate::contract::ExternalCandidate>,
    #[serde(default)]
    pub entity_type: String,
    pub path: String,
    pub raw: String,
    pub provider_key: (String, String),
    pub watched: Option<u32>,
    pub created: bool,
    pub episodes_done: bool,
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
        paused: false,
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
            input: Some(request.input.clone()),
            commit: None,
            outcomes: Vec::new(),
            creations: Default::default(),
            running: true,
            job: job.clone(),
            items: Vec::new(),
            schema: None,
            cancel: Arc::clone(&cancel),
        })
        .await?;
    if !inserted {
        return Err(ApiError::conflict("An import job is already running"));
    }

    let fetch = entry.fetch;
    let worker_state = state.clone();
    let job_id = job.id.clone();
    let input = request.input.clone();
    tokio::spawn(async move {
        job::run_plan_job(worker_state.clone(), job_id.clone(), fetch, input, cancel).await;
        worker_state.finish_import_worker(&job_id).await;
    });

    Ok(Json(job))
}

pub(crate) async fn list_import_jobs(
    State(state): State<AppState>,
) -> ApiResult<ImportJobListResponse> {
    state.import_recovery().check()?;
    let jobs = state.import_jobs().lock().await;
    let mut jobs: Vec<ImportJob> = jobs.values().map(|record| record.job.clone()).collect();
    jobs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(Json(ImportJobListResponse { jobs }))
}

pub(crate) async fn get_import_job(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<ImportJobPath>,
) -> ApiResult<ImportJob> {
    state.import_recovery().check()?;
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
    state.import_recovery().check()?;
    require_content_writes(&state).await?;

    // Flip the job to Committing and take its resolved items under one lock.
    let (planned, cancel) = {
        let mut jobs = state.import_jobs().lock().await;
        if jobs.values().any(|record| record.running) {
            return Err(ApiError::conflict("An import job is already running"));
        }
        let Some(record) = jobs.get_mut(&path.id) else {
            return Err(ApiError::not_found("Import job not found"));
        };
        if record.job.status != ImportJobStatus::Planned || record.running {
            return Err(ApiError::conflict("Import job is not ready to commit"));
        }
        let previous = record.clone();
        record.job.status = ImportJobStatus::Committing;
        record.job.paused = false;
        record.job.processed = 0;
        record.job.created = 0;
        record.job.skipped = 0;
        record.job.failed = 0;
        record.job.errors.clear();
        record.commit = Some(request.clone());
        record.outcomes = vec![None; record.items.len()];
        record.running = true;
        record
            .cancel
            .store(false, std::sync::atomic::Ordering::Relaxed);
        if let Err(error) = state.import_recovery().save(record).await {
            *record = previous;
            return Err(error);
        }
        record.job.finished_at = None;
        (record.items.clone(), Arc::clone(&record.cancel))
    };

    let worker_state = state.clone();
    let job_id = path.id.clone();
    tokio::spawn(async move {
        job::run_commit_job(
            worker_state.clone(),
            job_id.clone(),
            planned,
            request,
            cancel,
        )
        .await;
        worker_state.finish_import_worker(&job_id).await;
    });

    state.import_recovery().check()?;
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
        ImportJobStatus::Queued
            | ImportJobStatus::Fetching
            | ImportJobStatus::Planned
            | ImportJobStatus::Committing
    ) {
        record.job.status = ImportJobStatus::Cancelled;
        record.job.paused = false;
        record.job.finished_at = Some(now_iso());
    }
    state.import_recovery().save(record).await?;
    Ok(Json(record.job.clone()))
}

/// Stops at a durable boundary without discarding reviewed decisions or pending
/// episode enrichment. A paused worker may still be finishing a bounded write.
pub(crate) async fn pause_import_job(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<ImportJobPath>,
) -> ApiResult<ImportJob> {
    crate::api::state::require_host_asset_ingest(&state)?;
    state.import_recovery().check()?;
    let mut jobs = state.import_jobs().lock().await;
    let record = jobs
        .get_mut(&path.id)
        .ok_or_else(|| ApiError::not_found("Import job not found"))?;
    if matches!(
        record.job.status,
        ImportJobStatus::Queued | ImportJobStatus::Fetching | ImportJobStatus::Committing
    ) {
        record
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        record.job.paused = true;
        state.import_recovery().save(record).await?;
    }
    Ok(Json(record.job.clone()))
}

/// Resumes the saved phase. Creation intents and enrichment checkpoints belong
/// to the core; the host only selects the vault and asks for execution time.
pub(crate) async fn resume_import_job(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<ImportJobPath>,
) -> ApiResult<ImportJob> {
    crate::api::state::require_host_asset_ingest(&state)?;
    require_content_writes(&state).await?;
    state.import_recovery().check()?;
    let mut jobs = state.import_jobs().lock().await;
    if jobs.values().any(|record| record.running) {
        return Err(ApiError::conflict(
            "An import worker is still stopping or running",
        ));
    }
    let record = jobs
        .get_mut(&path.id)
        .ok_or_else(|| ApiError::not_found("Import job not found"))?;
    if !record.job.paused
        || !matches!(
            record.job.status,
            ImportJobStatus::Queued | ImportJobStatus::Fetching | ImportJobStatus::Committing
        )
    {
        return Err(ApiError::conflict("This import is not paused"));
    }
    let fetch = sources::registry()
        .iter()
        .find(|source| source.id == record.job.source)
        .map(|source| source.fetch)
        .ok_or_else(|| ApiError::bad_request("Unknown import source"))?;
    let input = record.input.clone();
    let commit = record.commit.clone();
    if commit.is_none() && input.is_none() {
        return Err(ApiError::conflict("This import has no saved input"));
    }
    record.job.paused = false;
    record.running = true;
    record.cancel = new_cancel();
    if let Err(error) = state.import_recovery().save(record).await {
        record.running = false;
        record.job.paused = true;
        return Err(error);
    }
    let job = record.job.clone();
    let cancel = record.cancel.clone();
    let planned = record.items.clone();
    drop(jobs);
    let id = path.id;
    tokio::spawn(async move {
        if let Some(commit) = commit {
            job::run_commit_job(state.clone(), id.clone(), planned, commit, cancel).await;
        } else if let Some(input) = input {
            job::run_plan_job(state.clone(), id.clone(), fetch, input, cancel).await;
        }
        state.finish_import_worker(&id).await;
    });
    Ok(Json(job))
}

pub(super) fn now_iso() -> String {
    Utc::now().to_rfc3339()
}
