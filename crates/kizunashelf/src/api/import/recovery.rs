//! Host-private, durable import checkpoints. Only native hosts may configure
//! this real directory; all vault reads and writes still use the VFS.
use super::{ImportJobRecord, ItemOutcome};
use crate::api::{error::ApiError, state::ApiOptions};
use crate::contract::ImportJobStatus;
use crate::library::file_revision;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Write,
    path::PathBuf,
    sync::{Arc, OnceLock, Weak},
};
use tokio::sync::Mutex;

pub(crate) struct ImportRecovery {
    pub jobs: Arc<Mutex<HashMap<String, ImportJobRecord>>>,
    error: std::sync::Mutex<Option<String>>,
    directory: Option<PathBuf>,
    identity: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    identity: String,
    record: ImportJobRecord,
}

impl ImportRecovery {
    pub fn open(options: &ApiOptions) -> Arc<Self> {
        let directory = options
            .import_state_dir
            .as_ref()
            .filter(|_| options.host_asset_ingest)
            .zip(options.index_cache_identity.as_ref())
            .map(|(root, id)| root.join(file_revision(id)));
        let identity = options.index_cache_identity.clone().unwrap_or_default();
        // Foreground/headless engines share jobs and cancellation in one process.
        // The weak entry disappears on process/engine teardown, then disk is authoritative.
        static OPEN: OnceLock<std::sync::Mutex<HashMap<PathBuf, Weak<ImportRecovery>>>> =
            OnceLock::new();
        let mut open = OPEN
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        open.retain(|_, value| value.strong_count() > 0);
        if let Some(existing) = directory
            .as_ref()
            .and_then(|p| open.get(p))
            .and_then(Weak::upgrade)
        {
            return existing;
        }
        let loaded = Self::read_jobs(directory.as_ref(), &identity);
        let (jobs, error) = match loaded {
            Ok(jobs) => (jobs, None),
            Err(error) => (
                HashMap::new(),
                Some(format!("Couldn't read saved imports: {error}")),
            ),
        };
        let recovery = Arc::new(Self {
            jobs: Arc::new(Mutex::new(jobs)),
            error: std::sync::Mutex::new(error),
            directory: directory.clone(),
            identity,
        });
        if let Some(directory) = directory {
            open.insert(directory, Arc::downgrade(&recovery));
        }
        recovery
    }

    fn read_jobs(
        directory: Option<&PathBuf>,
        identity: &str,
    ) -> anyhow::Result<HashMap<String, ImportJobRecord>> {
        let mut jobs = HashMap::new();
        let Some(directory) = directory else {
            return Ok(jobs);
        };
        if !directory.try_exists()? {
            return Ok(jobs);
        }
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            anyhow::ensure!(
                std::fs::metadata(&path)?.len() <= 64 * 1024 * 1024,
                "Import journal is too large"
            );
            let journal: Journal = serde_json::from_slice(&std::fs::read(&path)?)?;
            anyhow::ensure!(
                journal.version == 1 && journal.identity == identity,
                "Import journal identity/version mismatch"
            );
            let mut record = journal.record;
            anyhow::ensure!(
                record.outcomes.len() <= record.items.len(),
                "Invalid import checkpoints"
            );
            for (&index, intent) in &record.creations {
                anyhow::ensure!(index < record.items.len(), "Invalid creation checkpoint");
                crate::vfs::normalize_relative(&intent.path)?;
                anyhow::ensure!(
                    record.outcomes.get(index) != Some(&Some(ItemOutcome::Created))
                        || intent.created,
                    "Invalid created checkpoint"
                );
            }
            if matches!(
                record.job.status,
                ImportJobStatus::Queued | ImportJobStatus::Fetching | ImportJobStatus::Committing
            ) {
                record.job.paused = true;
            }
            record.running = false;
            jobs.insert(record.job.id.clone(), record);
        }
        Ok(jobs)
    }

    pub fn check(&self) -> Result<(), ApiError> {
        let mut error = self.error.lock().unwrap_or_else(|e| e.into_inner());
        if error.is_some() {
            // A File Provider/device-protection failure must be retryable without
            // rebuilding the UI or interpreting unreadable journals as absent.
            match Self::read_jobs(self.directory.as_ref(), &self.identity) {
                Ok(loaded) => {
                    let mut jobs = self
                        .jobs
                        .try_lock()
                        .map_err(|_| ApiError::conflict("Import recovery is busy. Retry."))?;
                    *jobs = loaded;
                    *error = None;
                }
                Err(failure) => {
                    *error = Some(format!("Couldn't read saved imports: {failure}"));
                }
            }
        }
        match error.as_ref() {
            Some(message) => Err(anyhow::anyhow!(message.clone()).into()),
            None => Ok(()),
        }
    }

    pub async fn save(&self, record: &ImportJobRecord) -> Result<(), ApiError> {
        self.check()?;
        let Some(directory) = &self.directory else {
            return Ok(());
        };
        let bytes = serde_json::to_vec(&Journal {
            version: 1,
            identity: self.identity.clone(),
            record: record.clone(),
        })?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(ApiError::bad_request(
                "This import is too large to save for recovery",
            ));
        }
        let directory = directory.clone();
        let filename = format!("{}.json", file_revision(&record.job.id));
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            std::fs::create_dir_all(&directory)?;
            let path = directory.join(filename);
            let temporary = path.with_extension("pending");
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temporary, &path)?;
            // Flush the rename on filesystems that support directory fsync.
            if let Ok(dir) = std::fs::File::open(&directory) {
                let _ = dir.sync_all();
            }
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!(e))??;
        Ok(())
    }

    pub async fn remove(&self, id: &str) {
        if let Some(directory) = &self.directory {
            let _ =
                tokio::fs::remove_file(directory.join(format!("{}.json", file_revision(id)))).await;
        }
    }
}
