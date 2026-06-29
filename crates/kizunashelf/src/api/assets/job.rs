//! Batch asset-download jobs: queue a job over a type (or the whole library),
//! run it with bounded concurrency, and report progress.

use super::download::download_entity_core;
use super::util::{all_local_asset_paths, entity_has_remote_image};
use crate::api::error::{ApiError, ApiResult};
use crate::api::state::{get_library, require_content_writes, AppState, AssetJobRecord};
use crate::contract::{
    AssetDownloadItemResult, AssetDownloadJob, AssetDownloadJobError, AssetDownloadJobListResponse,
    AssetDownloadJobRequest, AssetDownloadJobStatus, AssetDownloadStatus,
};
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use chrono::Utc;
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const ASSET_JOB_CONCURRENCY: usize = 4;
const MAX_JOB_ERRORS: usize = 50;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct JobPath {
    id: String,
}

pub(crate) async fn create_asset_job(
    State(state): State<AppState>,
    Json(request): Json<AssetDownloadJobRequest>,
) -> ApiResult<AssetDownloadJob> {
    let library = require_content_writes(&state).await?;
    if let Some(entity_type) = request.entity_type.as_deref() {
        if library.config.type_config(entity_type).is_none() {
            return Err(ApiError::bad_request("Unknown entity type"));
        }
    }

    let mut entity_ids = Vec::new();
    for entity in &library.records {
        if let Some(entity_type) = request.entity_type.as_deref() {
            if entity.summary.entity_type != entity_type {
                continue;
            }
        }
        let Some(type_config) = library.config.type_config(&entity.summary.entity_type) else {
            continue;
        };
        if entity_has_remote_image(&entity.frontmatter, type_config) {
            entity_ids.push(entity.summary.id.clone());
        }
    }

    let scope = request
        .entity_type
        .as_deref()
        .map(|entity_type| format!("type:{entity_type}"))
        .unwrap_or_else(|| "all".to_string());
    let job = AssetDownloadJob {
        id: state.next_asset_job_id(),
        status: AssetDownloadJobStatus::Queued,
        scope,
        total: entity_ids.len() as u32,
        processed: 0,
        downloaded: 0,
        failed: 0,
        skipped: 0,
        errors: Vec::new(),
        started_at: now_iso(),
        finished_at: None,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    // Only one batch download may run at a time: a second job would allocate its
    // own concurrency semaphore, so N concurrent jobs mean 4*N parallel downloads
    // against the same hosts. The check-and-insert is atomic (one lock) so two
    // simultaneous requests can't both slip through.
    let inserted = state
        .insert_asset_job_if_idle(AssetJobRecord {
            job: job.clone(),
            cancel: Arc::clone(&cancel),
        })
        .await;
    if !inserted {
        return Err(ApiError::conflict("A download job is already running"));
    }

    let worker_state = state.clone();
    let job_id = job.id.clone();
    tokio::spawn(async move {
        run_asset_job(worker_state, job_id, entity_ids, cancel).await;
    });

    Ok(Json(job))
}

pub(crate) async fn list_asset_jobs(
    State(state): State<AppState>,
) -> ApiResult<AssetDownloadJobListResponse> {
    let jobs = state.asset_jobs().lock().await;
    let mut jobs: Vec<AssetDownloadJob> = jobs.values().map(|record| record.job.clone()).collect();
    jobs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(Json(AssetDownloadJobListResponse { jobs }))
}

pub(crate) async fn get_asset_job(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<JobPath>,
) -> ApiResult<AssetDownloadJob> {
    let jobs = state.asset_jobs().lock().await;
    jobs.get(&path.id)
        .map(|record| Json(record.job.clone()))
        .ok_or_else(|| ApiError::not_found("Job not found"))
}

pub(crate) async fn cancel_asset_job(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<JobPath>,
) -> ApiResult<AssetDownloadJob> {
    let mut jobs = state.asset_jobs().lock().await;
    let Some(record) = jobs.get_mut(&path.id) else {
        return Err(ApiError::not_found("Job not found"));
    };
    record.cancel.store(true, Ordering::Relaxed);
    if matches!(
        record.job.status,
        AssetDownloadJobStatus::Queued | AssetDownloadJobStatus::Running
    ) {
        record.job.status = AssetDownloadJobStatus::Cancelled;
    }
    Ok(Json(record.job.clone()))
}

async fn run_asset_job(
    state: AppState,
    job_id: String,
    entity_ids: Vec<String>,
    cancel: Arc<AtomicBool>,
) {
    state
        .update_asset_job(&job_id, |job| job.status = AssetDownloadJobStatus::Running)
        .await;

    let library = match get_library(&state).await {
        Ok(library) => library,
        Err(error) => {
            let message = error.to_string();
            state
                .update_asset_job(&job_id, |job| {
                    job.status = AssetDownloadJobStatus::Completed;
                    job.finished_at = Some(now_iso());
                    job.errors.push(AssetDownloadJobError {
                        entity_id: String::new(),
                        entity_title: String::new(),
                        message,
                    });
                })
                .await;
            return;
        }
    };

    let all_local = Arc::new(all_local_asset_paths(&library));
    let vfs = state.vault_vfs(&library.config.vault_root);
    let asset_root = Arc::new(library.config.resolved_asset_root().to_string());
    let semaphore = Arc::new(Semaphore::new(ASSET_JOB_CONCURRENCY));
    let mut tasks = JoinSet::new();

    for entity_id in entity_ids {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let task_state = state.clone();
        let library = Arc::clone(&library);
        let all_local = Arc::clone(&all_local);
        let vfs = Arc::clone(&vfs);
        let asset_root = Arc::clone(&asset_root);
        let semaphore = Arc::clone(&semaphore);
        let job_id = job_id.clone();
        let cancel = Arc::clone(&cancel);
        tasks.spawn(async move {
            let Ok(_permit) = semaphore.acquire_owned().await else {
                return;
            };
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let Some(entity) = library.record_by_id(&entity_id).cloned() else {
                return;
            };
            let Some(type_config) = library
                .config
                .type_config(&entity.summary.entity_type)
                .cloned()
            else {
                return;
            };

            let outcome = download_entity_core(
                task_state.http_client(),
                vfs.as_ref(),
                asset_root.as_str(),
                &entity,
                &type_config,
                all_local.as_ref(),
                None,
            )
            .await;

            let (downloaded, failed, skipped, error_message) = match outcome {
                Ok(items) => {
                    let downloaded = count_status(&items, AssetDownloadStatus::Downloaded);
                    let failed = count_status(&items, AssetDownloadStatus::Failed);
                    let skipped = count_status(&items, AssetDownloadStatus::Skipped);
                    let error_message = items
                        .iter()
                        .find(|item| item.status == AssetDownloadStatus::Failed)
                        .and_then(|item| item.message.clone());
                    (downloaded, failed, skipped, error_message)
                }
                Err(error) => (0, 1, 0, Some(error.message().to_string())),
            };

            task_state
                .update_asset_job(&job_id, |job| {
                    job.processed += 1;
                    job.downloaded += downloaded;
                    job.failed += failed;
                    job.skipped += skipped;
                    if let Some(message) = error_message {
                        if job.errors.len() < MAX_JOB_ERRORS {
                            job.errors.push(AssetDownloadJobError {
                                entity_id: entity.summary.id.clone(),
                                entity_title: entity.summary.title.clone(),
                                message,
                            });
                        }
                    }
                })
                .await;
        });
    }

    while tasks.join_next().await.is_some() {}

    let cancelled = cancel.load(Ordering::Relaxed);
    state
        .update_asset_job(&job_id, |job| {
            job.status = if cancelled {
                AssetDownloadJobStatus::Cancelled
            } else {
                AssetDownloadJobStatus::Completed
            };
            job.finished_at = Some(now_iso());
        })
        .await;
    state.invalidate_cache().await;
}

fn count_status(items: &[AssetDownloadItemResult], status: AssetDownloadStatus) -> u32 {
    items.iter().filter(|item| item.status == status).count() as u32
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}
