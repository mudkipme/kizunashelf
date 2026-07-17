//! Plan and commit workers for a batch import job. The plan worker fetches +
//! resolves items (in-batch dedup, bucket→type match, "in library" lookup); the
//! commit worker creates the approved entities through the quick-add primitives.

use super::model::{apply_user_data, candidate_types_for, needs_detail_fetch, ImportItem};
use super::now_iso;
use super::sources::SourceFetchFn;
use crate::api::episodes::import_new_entity_episodes_marked;
use crate::api::external::{
    build_existing_index, build_mapped_document, candidate_basename_base, candidate_year,
    lookup_existing, resolve_candidate,
};
use crate::api::mutations::{resolve_free_basename, write_new_entity_file};
use crate::api::state::{get_library, AppState};
use crate::contract::{
    CommitImportJobRequest, ExistingEntityRef, ImportDecisionAction, ImportInput, ImportJob,
    ImportJobStatus, ImportPlan, ImportPlanBucket, ImportPlanItem, ImportPlanItemState,
    ImportReviewReason,
};
use crate::types::KizunaConfig;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const MAX_IMPORT_ERRORS: usize = 50;

/// Minimum spacing between per-item provider detail fetches in the commit loop.
/// Interactive flows fire one fetch at a time, but a large import would
/// otherwise hammer a provider with hundreds of back-to-back requests and trip
/// its rate limit (AniList and MAL throttle around one request per second).
const DETAIL_FETCH_SPACING: std::time::Duration = std::time::Duration::from_secs(1);

/// A resolved item held on the job record for commit. The wire [`ImportPlanItem`]
/// is a projection of this.
#[derive(Clone)]
pub(super) struct PlannedItem {
    pub item: ImportItem,
    pub state: ImportPlanItemState,
    pub candidate_types: Vec<String>,
}

// ---- Plan ------------------------------------------------------------------

pub(super) async fn run_plan_job(
    state: AppState,
    job_id: String,
    fetch: SourceFetchFn,
    input: ImportInput,
    cancel: Arc<AtomicBool>,
) {
    state
        .update_import_job(&job_id, |job| job.status = ImportJobStatus::Fetching)
        .await;

    let items = match fetch(&state, &input).await {
        Ok(items) => {
            let mut items = dedup_items(items);
            for item in &mut items {
                item.fill_title_metadata();
            }
            items
        }
        Err(error) => {
            fail_plan(&state, &job_id, error.message()).await;
            return;
        }
    };
    if cancel.load(Ordering::Relaxed) {
        finish_cancelled(&state, &job_id).await;
        return;
    }

    let library = match get_library(&state).await {
        Ok(library) => library,
        Err(error) => {
            fail_plan(&state, &job_id, &error.to_string()).await;
            return;
        }
    };
    let index = build_existing_index(&library);

    let mut planned = Vec::with_capacity(items.len());
    let mut plan_items = Vec::with_capacity(items.len());
    let mut needs_review = 0u32;

    for (position, item) in items.into_iter().enumerate() {
        let provider = item
            .primary_ref()
            .map(|reference| reference.provider.clone());
        let ref_url = item.primary_ref().map(|reference| reference.url.clone());
        let (state_kind, candidate_types, existing, review_reason) =
            resolve_plan_state(&library.config, &index, &item, provider.as_deref());
        if state_kind == ImportPlanItemState::NeedsReview {
            needs_review += 1;
        }
        plan_items.push(ImportPlanItem {
            index: position as u32,
            title: item.title.clone(),
            provider: provider.unwrap_or_default(),
            ref_url,
            bucket: item.bucket.clone(),
            state: state_kind,
            existing,
            review_reason,
            user_data: item.plan_user_data(),
        });
        planned.push(PlannedItem {
            item,
            state: state_kind,
            candidate_types,
        });
    }

    let buckets = build_buckets(&planned);
    let total = planned.len() as u32;

    let mut jobs = state.import_jobs().lock().await;
    if let Some(record) = jobs.get_mut(&job_id) {
        // Don't resurrect a job cancelled while we were fetching.
        if record.job.status == ImportJobStatus::Cancelled {
            return;
        }
        record.items = planned;
        record.job.status = ImportJobStatus::Planned;
        record.job.total = total;
        record.job.processed = total;
        record.job.needs_review = needs_review;
        record.job.plan = Some(ImportPlan {
            buckets,
            items: plan_items,
        });
    }
}

/// The disposition of one item against the library: which types can receive it,
/// whether it already exists, and why (if) it needs review.
fn resolve_plan_state(
    config: &KizunaConfig,
    index: &crate::api::external::ExistingIndex,
    item: &ImportItem,
    provider: Option<&str>,
) -> (
    ImportPlanItemState,
    Vec<String>,
    Option<ExistingEntityRef>,
    Option<ImportReviewReason>,
) {
    let Some(provider) = provider else {
        return (
            ImportPlanItemState::NeedsReview,
            Vec::new(),
            None,
            Some(ImportReviewReason::NoSupportedId),
        );
    };
    let types = candidate_types_for(config, provider, &item.bucket);
    if types.is_empty() {
        return (
            ImportPlanItemState::NeedsReview,
            types,
            None,
            Some(ImportReviewReason::NoTypeMatch),
        );
    }
    let candidate = item.lookup_candidate();
    let existing = types
        .iter()
        .find_map(|entity_type| lookup_existing(index, &candidate, entity_type));
    if existing.is_some() {
        (ImportPlanItemState::Exists, types, existing, None)
    } else {
        (ImportPlanItemState::WillCreate, types, None, None)
    }
}

/// Distinct (provider, bucket) pairs across items that have a ref, each with its
/// candidate types and an auto-selection when exactly one type matches.
fn build_buckets(planned: &[PlannedItem]) -> Vec<ImportPlanBucket> {
    let mut order: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut types_for: HashMap<(String, String), Vec<String>> = HashMap::new();
    for planned_item in planned {
        let Some(reference) = planned_item.item.primary_ref() else {
            continue;
        };
        let key = (reference.provider.clone(), planned_item.item.bucket.clone());
        if seen.insert(key.clone()) {
            order.push(key.clone());
        }
        types_for
            .entry(key)
            .or_insert_with(|| planned_item.candidate_types.clone());
    }
    order
        .into_iter()
        .map(|(provider, bucket)| {
            let candidate_types = types_for
                .get(&(provider.clone(), bucket.clone()))
                .cloned()
                .unwrap_or_default();
            let selected_type = (candidate_types.len() == 1).then(|| candidate_types[0].clone());
            ImportPlanBucket {
                bucket,
                provider,
                candidate_types,
                selected_type,
            }
        })
        .collect()
}

/// Collapses items sharing a primary-ref dedup key (the same work appearing in
/// several source lists), merging their user data into the first occurrence.
fn dedup_items(items: Vec<ImportItem>) -> Vec<ImportItem> {
    let mut order: Vec<ImportItem> = Vec::new();
    let mut index_of: HashMap<(String, String), usize> = HashMap::new();
    for item in items {
        if let Some(key) = item.dedup_key() {
            if let Some(&position) = index_of.get(&key) {
                order[position].merge(item);
                continue;
            }
            index_of.insert(key, order.len());
        }
        order.push(item);
    }
    order
}

// ---- Commit ----------------------------------------------------------------

pub(super) async fn run_commit_job(
    state: AppState,
    job_id: String,
    planned: Vec<PlannedItem>,
    request: CommitImportJobRequest,
    cancel: Arc<AtomicBool>,
) {
    state
        .update_import_job(&job_id, |job| {
            job.created = 0;
            job.skipped = 0;
            job.failed = 0;
            job.processed = 0;
            job.errors.clear();
        })
        .await;

    let library = match get_library(&state).await {
        Ok(library) => library,
        Err(error) => {
            fail_commit(
                &state,
                &job_id,
                &error.to_string(),
                cancel.load(Ordering::Relaxed),
            )
            .await;
            return;
        }
    };
    let vfs = state.vault_vfs(&library.config.vault_root);
    let taxonomy_root = library.config.taxonomy_root.clone();
    let index = build_existing_index(&library);

    // Refs created this run, so a second item can't re-create the same work
    // against the (pre-run) library index.
    let mut created_refs: HashSet<(String, String)> = HashSet::new();
    let mut last_detail_fetch: Option<std::time::Instant> = None;
    // (path, watched_count) for episode enrichment after the create pass.
    let mut created: Vec<(String, Option<u32>)> = Vec::new();

    for (position, planned_item) in planned.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let decision = request
            .decisions
            .iter()
            .find(|decision| decision.index as usize == position);
        let action = decision.map(|decision| decision.action).unwrap_or({
            if planned_item.state == ImportPlanItemState::WillCreate {
                ImportDecisionAction::Create
            } else {
                ImportDecisionAction::Skip
            }
        });
        if action == ImportDecisionAction::Skip {
            mark_skipped(&state, &job_id).await;
            continue;
        }

        let title = planned_item.item.title.clone();
        let override_candidate = decision.and_then(|decision| decision.candidate_override.as_ref());
        let (provider, ref_url) = if let Some(candidate) = override_candidate {
            (candidate.provider.clone(), candidate.url.clone())
        } else if let Some(reference) = planned_item.item.primary_ref() {
            (reference.provider.clone(), reference.url.clone())
        } else {
            mark_failed(
                &state,
                &job_id,
                format!("{title}: no resolvable source; provide a candidate"),
            )
            .await;
            continue;
        };

        // Target type: explicit override, else a single auto-selected candidate
        // type, else the per-bucket choice from the request.
        let target = decision
            .and_then(|decision| decision.type_override.clone())
            .or_else(|| {
                (planned_item.candidate_types.len() == 1)
                    .then(|| planned_item.candidate_types[0].clone())
            })
            .or_else(|| request.types.get(&planned_item.item.bucket).cloned());
        let Some(target) = target else {
            mark_failed(
                &state,
                &job_id,
                format!("{title}: no entity type selected for this item"),
            )
            .await;
            continue;
        };
        let Some(type_config) = library.config.type_config(&target).cloned() else {
            mark_failed(
                &state,
                &job_id,
                format!("{title}: unknown entity type '{target}'"),
            )
            .await;
            continue;
        };

        // Candidate: override or the item's partial, detail-fetched when the type
        // maps a metadata key the partial candidate lacks.
        let mut candidate = override_candidate
            .cloned()
            .or_else(|| planned_item.item.candidate.clone());
        if needs_detail_fetch(candidate.as_ref(), &type_config, &provider) {
            if let Some(last) = last_detail_fetch {
                let elapsed = last.elapsed();
                if elapsed < DETAIL_FETCH_SPACING {
                    tokio::time::sleep(DETAIL_FETCH_SPACING - elapsed).await;
                }
            }
            last_detail_fetch = Some(std::time::Instant::now());
            match resolve_candidate(
                &state,
                &provider,
                &ref_url,
                std::slice::from_ref(&planned_item.item.bucket),
                None,
            )
            .await
            {
                Ok(Some(fetched)) => candidate = Some(fetched),
                Ok(None) => {}
                Err(error) => {
                    if candidate.is_none() {
                        mark_failed(&state, &job_id, format!("{title}: {}", error.message())).await;
                        continue;
                    }
                }
            }
        }
        let Some(candidate) = candidate else {
            mark_failed(
                &state,
                &job_id,
                format!("{title}: could not resolve a candidate"),
            )
            .await;
            continue;
        };

        // Already in the library (or created earlier this run)? Skip, create nothing.
        let created_key = (provider.to_lowercase(), candidate.source_id.to_lowercase());
        if lookup_existing(&index, &candidate, &type_config.id).is_some()
            || created_refs.contains(&created_key)
        {
            mark_skipped(&state, &job_id).await;
            continue;
        }

        let (mut frontmatter, mut body, mapped_fields) =
            build_mapped_document(&candidate, &type_config);
        if request.options.import_user_data {
            apply_user_data(
                &mut frontmatter,
                &mut body,
                &type_config,
                &planned_item.item.user,
            );
        }

        let base = match candidate_basename_base(&candidate, &type_config) {
            Ok(base) => base,
            Err(error) => {
                mark_failed(&state, &job_id, format!("{title}: {}", error.message())).await;
                continue;
            }
        };
        let year = candidate_year(&type_config, &mapped_fields);
        let basename = match resolve_free_basename(
            vfs.as_ref(),
            &taxonomy_root,
            &type_config,
            &library,
            &base,
            year.as_deref(),
            Some(&provider),
        )
        .await
        {
            Ok((basename, _)) => basename,
            Err(error) => {
                mark_failed(&state, &job_id, format!("{title}: {}", error.message())).await;
                continue;
            }
        };

        match write_new_entity_file(
            vfs.as_ref(),
            &taxonomy_root,
            &type_config,
            &basename,
            &frontmatter,
            &body,
        )
        .await
        {
            Ok(path) => {
                created_refs.insert(created_key);
                let watched = request
                    .options
                    .mark_progress
                    .then_some(planned_item.item.user.watched_count)
                    .flatten();
                created.push((path, watched));
                mark_created(&state, &job_id).await;
            }
            Err(error) => {
                mark_failed(&state, &job_id, format!("{title}: {}", error.message())).await;
            }
        }
    }

    // Episode enrichment for the created entities (fail-safe): reload once so the
    // new files are indexed, then import each source's episodes. One provider
    // fetch per entity means this phase can dwarf the create phase on big
    // imports, so it reports its own `episodes_*` progress — without it, clients
    // sit on a full `processed`/`total` bar while the job is still `committing`.
    if request.options.import_episodes && !created.is_empty() && !cancel.load(Ordering::Relaxed) {
        state
            .update_import_job(&job_id, |job| {
                job.episodes_total = Some(created.len() as u32);
                job.episodes_processed = Some(0);
            })
            .await;
        state.invalidate_cache().await;
        if let Ok(reloaded) = get_library(&state).await {
            for (path, watched) in &created {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let entity_id = reloaded
                    .record_by_path(path)
                    .map(|record| record.summary.id.clone());
                if let Some(entity_id) = entity_id {
                    if let Some(result) = import_new_entity_episodes_marked(
                        &state, &reloaded, &entity_id, *watched, None,
                    )
                    .await
                    {
                        if let Some(error) = result.error {
                            state
                                .update_import_job(&job_id, |job| {
                                    push_error(job, format!("episodes: {error}"));
                                })
                                .await;
                        }
                    }
                }
                state
                    .update_import_job(&job_id, |job| {
                        job.episodes_processed =
                            Some(job.episodes_processed.unwrap_or(0).saturating_add(1));
                    })
                    .await;
            }
        }
    }

    state.invalidate_cache().await;
    let cancelled = cancel.load(Ordering::Relaxed);
    state
        .update_import_job(&job_id, |job| {
            job.status = if cancelled {
                ImportJobStatus::Cancelled
            } else {
                ImportJobStatus::Completed
            };
            job.finished_at = Some(now_iso());
        })
        .await;
}

// ---- Job-status helpers ----------------------------------------------------

async fn mark_skipped(state: &AppState, job_id: &str) {
    state
        .update_import_job(job_id, |job| {
            job.skipped += 1;
            job.processed += 1;
        })
        .await;
}

async fn mark_created(state: &AppState, job_id: &str) {
    state
        .update_import_job(job_id, |job| {
            job.created += 1;
            job.processed += 1;
        })
        .await;
}

async fn mark_failed(state: &AppState, job_id: &str, message: String) {
    state
        .update_import_job(job_id, |job| {
            push_error(job, message);
            job.failed += 1;
            job.processed += 1;
        })
        .await;
}

async fn fail_plan(state: &AppState, job_id: &str, message: &str) {
    let message = message.to_string();
    state
        .update_import_job(job_id, |job| {
            push_error(job, message);
            job.status = ImportJobStatus::Failed;
            job.finished_at = Some(now_iso());
        })
        .await;
}

async fn fail_commit(state: &AppState, job_id: &str, message: &str, cancelled: bool) {
    let message = message.to_string();
    state
        .update_import_job(job_id, |job| {
            push_error(job, message);
            job.status = if cancelled {
                ImportJobStatus::Cancelled
            } else {
                ImportJobStatus::Failed
            };
            job.finished_at = Some(now_iso());
        })
        .await;
}

async fn finish_cancelled(state: &AppState, job_id: &str) {
    state
        .update_import_job(job_id, |job| {
            job.status = ImportJobStatus::Cancelled;
            job.finished_at = Some(now_iso());
        })
        .await;
}

fn push_error(job: &mut ImportJob, message: String) {
    if job.errors.len() < MAX_IMPORT_ERRORS {
        job.errors.push(message);
    }
}
