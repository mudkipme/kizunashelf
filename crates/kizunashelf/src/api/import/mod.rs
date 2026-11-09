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

/// The durable phase of an import, independent of worker ownership. `Enriching`
/// is the post-create episode pass; it is a phase rather than a pair of optional
/// progress fields so resume can tell it apart from the create loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImportPhase {
    Planning,
    Creating,
    Enriching,
}

/// The lifecycle authority for an import. The wire [`ImportJob::status`] +
/// [`ImportJob::paused`] pair is a projection of this; every transition goes
/// through [`ImportJobRecord::set_state`]. `Queued` exists because a cancel or
/// pause can land between insertion and the plan worker's first transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImportState {
    Queued,
    Running(ImportPhase),
    Paused(ImportPhase),
    /// The plan is complete and waiting for review/commit.
    Planned,
    Completed,
    Cancelled,
    Failed,
}

impl ImportPhase {
    fn status(self) -> ImportJobStatus {
        match self {
            Self::Planning => ImportJobStatus::Fetching,
            Self::Creating | Self::Enriching => ImportJobStatus::Committing,
        }
    }
}

impl ImportState {
    pub(crate) fn is_paused(self) -> bool {
        matches!(self, Self::Paused(_))
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    /// Whether a worker is queued/live or the job is parked at a resumable
    /// boundary. `Planned` and terminal states occupy no execution slot.
    pub(crate) fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running(_) | Self::Paused(_))
    }

    /// The phase a stopped job resumes into. `None` for a resting plan or a
    /// terminal job, which have nothing to resume.
    pub(crate) fn resumable_phase(self) -> Option<ImportPhase> {
        match self {
            Self::Queued => Some(ImportPhase::Planning),
            Self::Running(phase) | Self::Paused(phase) => Some(phase),
            Self::Planned | Self::Completed | Self::Cancelled | Self::Failed => None,
        }
    }

    fn wire(self) -> (ImportJobStatus, bool) {
        match self {
            Self::Queued => (ImportJobStatus::Queued, false),
            Self::Running(phase) => (phase.status(), false),
            Self::Paused(phase) => (phase.status(), true),
            Self::Planned => (ImportJobStatus::Planned, false),
            Self::Completed => (ImportJobStatus::Completed, false),
            Self::Cancelled => (ImportJobStatus::Cancelled, false),
            Self::Failed => (ImportJobStatus::Failed, false),
        }
    }
}

impl ImportJobRecord {
    /// Derives the lifecycle state from the persisted wire fields, so journals
    /// written before this enum existed still resume correctly.
    pub(crate) fn state(&self) -> ImportState {
        match self.job.status {
            ImportJobStatus::Queued => ImportState::Queued,
            ImportJobStatus::Fetching => {
                if self.job.paused {
                    ImportState::Paused(ImportPhase::Planning)
                } else {
                    ImportState::Running(ImportPhase::Planning)
                }
            }
            ImportJobStatus::Planned => ImportState::Planned,
            ImportJobStatus::Committing => {
                let phase = if self.job.episodes_total.is_some() {
                    ImportPhase::Enriching
                } else {
                    ImportPhase::Creating
                };
                if self.job.paused {
                    ImportState::Paused(phase)
                } else {
                    ImportState::Running(phase)
                }
            }
            ImportJobStatus::Completed => ImportState::Completed,
            ImportJobStatus::Cancelled => ImportState::Cancelled,
            ImportJobStatus::Failed => ImportState::Failed,
        }
    }

    /// Transitions the job and keeps the wire projection in sync. Callers must
    /// not assign `job.status`/`job.paused` directly.
    pub(crate) fn set_state(&mut self, state: ImportState) {
        let (status, paused) = state.wire();
        self.job.status = status;
        self.job.paused = paused;
    }

    /// Starts a spawned plan worker. A queued job advances; a pause or cancel
    /// that landed before the task ran keeps the requested state instead.
    pub(crate) fn begin_plan(&mut self) {
        if self.state() == ImportState::Queued {
            self.set_state(ImportState::Running(ImportPhase::Planning));
        }
    }
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
        if record.state() != ImportState::Planned || record.running {
            return Err(ApiError::conflict("Import job is not ready to commit"));
        }
        let previous = record.clone();
        record.set_state(ImportState::Running(ImportPhase::Creating));
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
    if !record.state().is_terminal() {
        record.set_state(ImportState::Cancelled);
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
    if let Some(phase) = record.state().resumable_phase() {
        record
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        record.set_state(ImportState::Paused(phase));
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
    let ImportState::Paused(phase) = record.state() else {
        return Err(ApiError::conflict("This import is not paused"));
    };
    let input = record.input.clone();
    let commit = record.commit.clone();
    let fetch = if phase == ImportPhase::Planning {
        if input.is_none() {
            return Err(ApiError::conflict("This import has no saved input"));
        }
        Some(
            sources::registry()
                .into_iter()
                .find(|source| source.id == record.job.source)
                .map(|source| source.fetch)
                .ok_or_else(|| ApiError::bad_request("Unknown import source"))?,
        )
    } else {
        if commit.is_none() {
            return Err(ApiError::conflict("This import has no saved commit"));
        }
        None
    };
    record.set_state(ImportState::Running(phase));
    record.running = true;
    record.cancel = new_cancel();
    if let Err(error) = state.import_recovery().save(record).await {
        record.running = false;
        record.set_state(ImportState::Paused(phase));
        return Err(error);
    }
    let job = record.job.clone();
    let cancel = record.cancel.clone();
    let planned = record.items.clone();
    drop(jobs);
    let id = path.id;
    tokio::spawn(async move {
        match phase {
            ImportPhase::Planning => {
                if let (Some(fetch), Some(input)) = (fetch, input) {
                    job::run_plan_job(state.clone(), id.clone(), fetch, input, cancel).await;
                }
            }
            ImportPhase::Creating | ImportPhase::Enriching => {
                if let Some(commit) = commit {
                    job::run_commit_job(state.clone(), id.clone(), planned, commit, cancel).await;
                }
            }
        }
        state.finish_import_worker(&id).await;
    });
    Ok(Json(job))
}

pub(super) fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod state_tests {
    use super::*;

    fn record(status: ImportJobStatus, paused: bool, enriching: bool) -> ImportJobRecord {
        ImportJobRecord {
            input: None,
            commit: None,
            outcomes: Vec::new(),
            creations: Default::default(),
            running: false,
            job: ImportJob {
                id: "job".into(),
                source: "test".into(),
                status,
                paused,
                total: 0,
                processed: 0,
                created: 0,
                skipped: 0,
                needs_review: 0,
                failed: 0,
                episodes_total: enriching.then_some(1),
                episodes_processed: enriching.then_some(0),
                errors: Vec::new(),
                started_at: "2026-01-01T00:00:00Z".into(),
                finished_at: None,
                plan: None,
            },
            items: Vec::new(),
            schema: None,
            cancel: new_cancel(),
        }
    }

    #[test]
    fn every_state_round_trips_through_the_wire_projection() {
        for state in [
            ImportState::Queued,
            ImportState::Running(ImportPhase::Planning),
            ImportState::Running(ImportPhase::Creating),
            ImportState::Paused(ImportPhase::Planning),
            ImportState::Paused(ImportPhase::Creating),
            ImportState::Planned,
            ImportState::Completed,
            ImportState::Cancelled,
            ImportState::Failed,
        ] {
            let mut job = record(ImportJobStatus::Queued, false, false);
            job.set_state(state);
            assert_eq!(job.state(), state, "{state:?} did not round-trip");
        }
        for state in [
            ImportState::Running(ImportPhase::Enriching),
            ImportState::Paused(ImportPhase::Enriching),
        ] {
            let mut job = record(ImportJobStatus::Committing, false, true);
            job.set_state(state);
            assert_eq!(job.state(), state, "{state:?} did not round-trip");
        }
    }

    #[test]
    fn the_enrichment_phase_comes_from_the_episode_checkpoint() {
        assert_eq!(
            record(ImportJobStatus::Committing, false, true).state(),
            ImportState::Running(ImportPhase::Enriching)
        );
        assert_eq!(
            record(ImportJobStatus::Committing, true, true).state(),
            ImportState::Paused(ImportPhase::Enriching)
        );
        assert_eq!(
            record(ImportJobStatus::Fetching, true, false).state(),
            ImportState::Paused(ImportPhase::Planning)
        );
        assert_eq!(
            record(ImportJobStatus::Committing, false, false).state(),
            ImportState::Running(ImportPhase::Creating)
        );
    }

    #[test]
    fn only_active_states_are_resumable_and_occupying_a_slot() {
        assert_eq!(
            ImportState::Queued.resumable_phase(),
            Some(ImportPhase::Planning)
        );
        assert_eq!(
            ImportState::Paused(ImportPhase::Creating).resumable_phase(),
            Some(ImportPhase::Creating)
        );
        assert_eq!(ImportState::Planned.resumable_phase(), None);
        assert_eq!(ImportState::Completed.resumable_phase(), None);

        assert!(ImportState::Queued.is_active());
        assert!(ImportState::Running(ImportPhase::Creating).is_active());
        assert!(ImportState::Paused(ImportPhase::Planning).is_active());
        assert!(!ImportState::Planned.is_active());
        assert!(!ImportState::Cancelled.is_active());
    }

    #[test]
    fn set_state_clears_stale_projection_bits() {
        let mut job = record(ImportJobStatus::Fetching, true, false);
        job.set_state(ImportState::Planned);
        assert_eq!(job.job.status, ImportJobStatus::Planned);
        assert!(!job.job.paused);

        job.set_state(ImportState::Cancelled);
        assert_eq!(job.job.status, ImportJobStatus::Cancelled);
        assert!(!job.job.paused);
    }

    #[test]
    fn a_pause_before_the_worker_starts_is_not_overwritten() {
        let mut paused = record(ImportJobStatus::Queued, false, false);
        paused.set_state(ImportState::Paused(ImportPhase::Planning));
        paused.begin_plan();
        assert_eq!(paused.state(), ImportState::Paused(ImportPhase::Planning));

        let mut cancelled = record(ImportJobStatus::Queued, false, false);
        cancelled.set_state(ImportState::Cancelled);
        cancelled.begin_plan();
        assert_eq!(cancelled.state(), ImportState::Cancelled);

        let mut fresh = record(ImportJobStatus::Queued, false, false);
        fresh.begin_plan();
        assert_eq!(fresh.state(), ImportState::Running(ImportPhase::Planning));
    }
}
