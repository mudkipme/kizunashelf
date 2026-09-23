//! Durable review snapshots owned and interpreted only by the core. Hosts keep
//! the opaque envelope in private storage alongside their selection UI state.
use super::{job::PlannedItem, ImportJobPath, ImportJobRecord};
use crate::api::{
    error::{ApiError, ApiResult},
    state::{require_content_writes, AppState},
};
use crate::contract::{ImportJob, ImportJobStatus, ImportPlanSnapshot};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::{atomic::AtomicBool, Arc};

const VERSION: u32 = 1;
const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    vault_identity: String,
    schema: serde_json::Value,
    job: ImportJob,
    items: Vec<PlannedItem>,
}

fn identity(state: &AppState) -> Result<&str, ApiError> {
    if !state.options.host_asset_ingest {
        return Err(ApiError::forbidden(
            "Import snapshots require an in-process host",
        ));
    }
    state
        .options
        .index_cache_identity
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ApiError::forbidden("Import snapshots require a stable vault identity"))
}

pub(crate) async fn export_import_plan(
    State(state): State<AppState>,
    Path(path): Path<ImportJobPath>,
) -> ApiResult<ImportPlanSnapshot> {
    let identity = identity(&state)?.to_owned();
    let _mutation = state.content_mutation_lock().await;
    state.invalidate_cache().await;
    let library = require_content_writes(&state).await?;
    let jobs = state.import_jobs().lock().await;
    let record = jobs
        .get(&path.id)
        .ok_or_else(|| ApiError::not_found("Import job not found"))?;
    if record.job.status != ImportJobStatus::Planned {
        return Err(ApiError::conflict(
            "Only an uncommitted review can be saved",
        ));
    }
    let schema = serde_json::to_value(library.config.clone().into_parts().1)?;
    if record.schema.as_ref() != Some(&schema) {
        return Err(ApiError::conflict(
            "The vault schema changed. Review a new import plan.",
        ));
    }
    let snapshot = serde_json::to_string(&Snapshot {
        version: VERSION,
        vault_identity: identity,
        schema,
        job: record.job.clone(),
        items: record.items.clone(),
    })?;
    if snapshot.len() > MAX_SNAPSHOT_BYTES {
        return Err(ApiError::bad_request(
            "This import plan is too large to save for recovery",
        ));
    }
    Ok(Json(ImportPlanSnapshot { snapshot }))
}

pub(crate) async fn restore_import_plan(
    State(state): State<AppState>,
    Json(request): Json<ImportPlanSnapshot>,
) -> ApiResult<ImportJob> {
    let identity = identity(&state)?;
    if request.snapshot.len() > MAX_SNAPSHOT_BYTES {
        return Err(ApiError::bad_request(
            "Import snapshot exceeds the recovery size limit",
        ));
    }
    let snapshot: Snapshot = serde_json::from_str(&request.snapshot)
        .map_err(|_| ApiError::bad_request("The saved import plan is malformed"))?;
    if snapshot.version != VERSION || snapshot.vault_identity != identity {
        return Err(ApiError::conflict(
            "The saved import plan belongs to another vault or core version",
        ));
    }
    let Some(plan) = &snapshot.job.plan else {
        return Err(ApiError::bad_request("The saved import has no review plan"));
    };
    if snapshot.job.status != ImportJobStatus::Planned
        || snapshot.job.created != 0
        || snapshot.job.total as usize != snapshot.items.len()
        || plan.items.len() != snapshot.items.len()
        || plan
            .items
            .iter()
            .zip(&snapshot.items)
            .enumerate()
            .any(|(index, (visible, item))| {
                visible.index as usize != index
                    || visible.state != item.state
                    || visible.title != item.item.title
                    || visible.bucket != item.item.bucket
                    || visible.provider
                        != item
                            .resolved_ref
                            .as_ref()
                            .map(|r| r.provider.as_str())
                            .unwrap_or("")
                    || visible.ref_url.as_deref()
                        != item.resolved_ref.as_ref().map(|r| r.url.as_str())
            })
    {
        return Err(ApiError::bad_request(
            "The saved import is not an untouched review plan",
        ));
    }
    let _mutation = state.content_mutation_lock().await;
    state.invalidate_cache().await;
    let library = require_content_writes(&state).await?;
    if serde_json::to_value(library.config.clone().into_parts().1)? != snapshot.schema {
        return Err(ApiError::conflict(
            "The vault schema changed. Review a new import plan.",
        ));
    }
    // Never replace a live/terminal job with an older journal copy, even after a
    // lost restore response. A repeated restore is allowed only for the same plan.
    let mut jobs = state.import_jobs().lock().await;
    if let Some(existing) = jobs.get(&snapshot.job.id) {
        if serde_json::to_value(&existing.job)? == serde_json::to_value(&snapshot.job)?
            && serde_json::to_value(&existing.items)? == serde_json::to_value(&snapshot.items)?
        {
            return Ok(Json(existing.job.clone()));
        }
        return Err(ApiError::conflict(
            "This import has already changed. Reload its current state.",
        ));
    }
    if jobs.values().any(|r| {
        matches!(
            r.job.status,
            ImportJobStatus::Queued | ImportJobStatus::Fetching | ImportJobStatus::Committing
        )
    }) {
        return Err(ApiError::conflict("Another import job is running"));
    }
    let job = snapshot.job;
    jobs.insert(
        job.id.clone(),
        ImportJobRecord {
            job: job.clone(),
            items: snapshot.items,
            schema: Some(snapshot.schema),
            cancel: Arc::new(AtomicBool::new(false)),
        },
    );
    Ok(Json(job))
}
