//! In-flight background jobs: batch asset downloads and imports.

use super::super::error::ApiError;
use super::super::import::recovery::ImportRecovery;
use super::super::import::{ImportJobRecord, ImportState};
use super::unix_seconds_now;
use crate::contract::{AssetDownloadJob, ImportJob};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// Maximum number of finished jobs (of each kind) kept in memory.
const MAX_RETAINED_JOBS: usize = 20;

/// In-memory record for a batch asset-download job. Jobs do not survive a
/// restart by design; the user simply re-runs and already-downloaded images are
/// skipped.
pub(crate) struct AssetJobRecord {
    pub(crate) job: AssetDownloadJob,
    pub(crate) cancel: Arc<AtomicBool>,
}

/// The router's job tables. At most one job of each kind runs at a time; the
/// most recent finished ones are retained so clients can read their outcome.
/// Import jobs are additionally journaled through [`ImportRecovery`] so an
/// in-process host can resume them after a restart.
pub(crate) struct JobRegistry {
    assets: Arc<Mutex<HashMap<String, AssetJobRecord>>>,
    asset_counter: AtomicU64,
    import_recovery: Arc<ImportRecovery>,
    import_counter: AtomicU64,
}

impl JobRegistry {
    pub(super) fn new(import_recovery: Arc<ImportRecovery>) -> Self {
        Self {
            assets: Arc::new(Mutex::new(HashMap::new())),
            asset_counter: AtomicU64::new(0),
            import_recovery,
            import_counter: AtomicU64::new(0),
        }
    }

    pub(crate) fn asset_jobs(&self) -> &Arc<Mutex<HashMap<String, AssetJobRecord>>> {
        &self.assets
    }

    pub(crate) fn next_asset_job_id(&self) -> String {
        let counter = self.asset_counter.fetch_add(1, Ordering::Relaxed);
        format!("job-{}-{}", unix_seconds_now(), counter)
    }

    /// Inserts a new job unless one is already queued or running, pruning the
    /// oldest finished jobs beyond the retention limit. The running-check and the
    /// insert happen under a single lock so concurrent callers can't both start a
    /// job. Returns `false` (and inserts nothing) when a job is already in flight.
    pub(crate) async fn insert_asset_job_if_idle(&self, record: AssetJobRecord) -> bool {
        let mut jobs = self.assets.lock().await;
        if jobs.values().any(|existing| existing.job.status.is_busy()) {
            return false;
        }
        jobs.insert(record.job.id.clone(), record);
        if jobs.len() > MAX_RETAINED_JOBS {
            let mut finished: Vec<(String, String)> = jobs
                .values()
                .filter(|record| !record.job.status.is_busy())
                .map(|record| (record.job.id.clone(), record.job.started_at.clone()))
                .collect();
            finished.sort_by(|a, b| a.1.cmp(&b.1));
            let remove = jobs.len().saturating_sub(MAX_RETAINED_JOBS);
            for (id, _) in finished.into_iter().take(remove) {
                jobs.remove(&id);
            }
        }
        true
    }

    /// Applies `update` to the stored job, if it still exists.
    pub(crate) async fn update_asset_job(
        &self,
        job_id: &str,
        update: impl FnOnce(&mut AssetDownloadJob),
    ) {
        let mut jobs = self.assets.lock().await;
        if let Some(record) = jobs.get_mut(job_id) {
            update(&mut record.job);
        }
    }

    pub(crate) fn import_jobs(&self) -> &Arc<Mutex<HashMap<String, ImportJobRecord>>> {
        &self.import_recovery.jobs
    }

    pub(crate) fn next_import_job_id(&self) -> String {
        let counter = self.import_counter.fetch_add(1, Ordering::Relaxed);
        format!(
            "import-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            counter
        )
    }

    /// Inserts a new import job unless one is already actively fetching or
    /// committing, pruning the oldest finished jobs beyond the retention limit.
    /// A `Planned` job (waiting for commit) does not block a new plan — committing
    /// a stale plan is safe because the dedup gate skips already-created entities.
    /// The check-and-insert is atomic (one lock). Returns `false` (and inserts
    /// nothing) when a job is already in flight.
    pub(crate) async fn insert_import_job_if_idle(
        &self,
        record: ImportJobRecord,
    ) -> Result<bool, ApiError> {
        self.import_recovery.check()?;
        let mut jobs = self.import_jobs().lock().await;
        if jobs.contains_key(&record.job.id) || jobs.values().any(|existing| existing.running) {
            return Ok(false);
        }
        self.import_recovery.save(&record).await?;
        jobs.insert(record.job.id.clone(), record);
        if jobs.len() > MAX_RETAINED_JOBS {
            let mut finished: Vec<(String, String)> = jobs
                .values()
                .filter(|record| record.state().is_terminal())
                .map(|record| (record.job.id.clone(), record.job.started_at.clone()))
                .collect();
            finished.sort_by(|a, b| a.1.cmp(&b.1));
            let remove = jobs.len().saturating_sub(MAX_RETAINED_JOBS);
            for (id, _) in finished.into_iter().take(remove) {
                jobs.remove(&id);
                self.import_recovery.remove(&id).await;
            }
        }
        Ok(true)
    }

    pub(crate) fn import_recovery(&self) -> &ImportRecovery {
        &self.import_recovery
    }

    pub(crate) async fn finish_import_worker(&self, id: &str) {
        let mut jobs = self.import_jobs().lock().await;
        if let Some(record) = jobs.get_mut(id) {
            record.running = false;
            let _ = self.import_recovery.save(record).await;
        }
    }

    /// Applies `update` to the stored import job, if it still exists.
    pub(crate) async fn update_import_job(
        &self,
        job_id: &str,
        update: impl FnOnce(&mut ImportJob),
    ) {
        self.update_import_record(job_id, |record| update(&mut record.job))
            .await;
    }

    /// Applies `update` to the whole import record, so a transition can update the
    /// lifecycle state and its counters under one journal write.
    pub(crate) async fn update_import_record(
        &self,
        job_id: &str,
        update: impl FnOnce(&mut ImportJobRecord),
    ) {
        let mut jobs = self.import_jobs().lock().await;
        if let Some(record) = jobs.get_mut(job_id) {
            let previous = record.clone();
            update(record);
            if let Err(error) = self.import_recovery.save(record).await {
                *record = previous;
                record.cancel.store(true, Ordering::Relaxed);
                // Keep the phase the update was leaving behind, so a native host
                // can resume it; a resting/terminal job has nothing to pause.
                if let Some(phase) = record.state().resumable_phase() {
                    record.set_state(ImportState::Paused(phase));
                }
                record.job.errors.push(format!(
                    "Could not save import progress: {}",
                    error.message()
                ));
            }
        }
    }
}
