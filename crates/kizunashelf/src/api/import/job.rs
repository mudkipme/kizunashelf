//! Plan and commit workers for a batch import job. The plan worker fetches +
//! resolves items (in-batch dedup, bucket→type match, "in library" lookup); the
//! commit worker creates the approved entities through the quick-add primitives.

use super::model::{
    apply_user_data, candidate_types_for, needs_detail_fetch, ImportItem, ProviderRef,
};
use super::sources::SourceFetchFn;
use super::{now_iso, CreationCheckpoint, ItemOutcome};
use crate::api::episodes::import_new_entity_episodes_cancellable;
use crate::api::error::ApiError;
use crate::api::external::{
    build_existing_index, build_mapped_document, candidate_basename_base, candidate_year,
    lookup_existing, resolve_candidate,
};
use crate::api::mutations::{
    entity_create_path, resolve_free_basename, write_entity_raw, write_new_entity_file,
};
use crate::api::state::{get_library, require_content_writes, AppState};
use crate::contract::{
    CommitImportJobRequest, ExistingEntityRef, ImportDecisionAction, ImportInput, ImportJob,
    ImportJobStatus, ImportPlan, ImportPlanBucket, ImportPlanItem, ImportPlanItemState,
    ImportReviewReason,
};
use crate::library::{serialize_markdown_document, split_markdown_document};
use crate::types::KizunaConfig;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const MAX_IMPORT_ERRORS: usize = 50;

/// A resolved item held on the job record for commit. The wire [`ImportPlanItem`]
/// is a projection of this.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct PlannedItem {
    pub item: ImportItem,
    pub state: ImportPlanItemState,
    pub candidate_types: Vec<String>,
    /// The ref the item resolved to against the plan-time schema — the first of
    /// the item's refs some type maps ([`ImportItem::ref_for_config`]). Commit
    /// creates under this ref's provider.
    pub resolved_ref: Option<ProviderRef>,
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

    let fetched = tokio::select! {
        result = fetch(&state, &input) => result,
        _ = super::import_cancelled(&cancel) => {
            finish_cancelled(&state, &job_id).await;
            return;
        }
    };
    let items = match fetched {
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
    let schema = match serde_json::to_value(library.config.clone().into_parts().1) {
        Ok(schema) => schema,
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
        let resolved_ref = item.ref_for_config(&library.config).cloned();
        let provider = resolved_ref
            .as_ref()
            .map(|reference| reference.provider.clone());
        let ref_url = resolved_ref.as_ref().map(|reference| reference.url.clone());
        let (state_kind, candidate_types, existing, review_reason) =
            resolve_plan_state(&library.config, &index, &item, resolved_ref.as_ref());
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
            resolved_ref,
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
        record.schema = Some(schema);
        record.job.status = ImportJobStatus::Planned;
        record.job.total = total;
        record.job.processed = total;
        record.job.needs_review = needs_review;
        record.job.plan = Some(ImportPlan {
            buckets,
            items: plan_items,
        });
        if let Err(error) = state.import_recovery().save(record).await {
            record.job.paused = true;
            record.cancel.store(true, Ordering::Relaxed);
            push_error(
                &mut record.job,
                format!("Couldn't save import review: {}", error.message()),
            );
        }
    }
}

/// The disposition of one item against the library: which types can receive it,
/// whether it already exists, and why (if) it needs review.
fn resolve_plan_state(
    config: &KizunaConfig,
    index: &crate::api::external::ExistingIndex,
    item: &ImportItem,
    reference: Option<&ProviderRef>,
) -> (
    ImportPlanItemState,
    Vec<String>,
    Option<ExistingEntityRef>,
    Option<ImportReviewReason>,
) {
    let Some(reference) = reference else {
        return (
            ImportPlanItemState::NeedsReview,
            Vec::new(),
            None,
            Some(ImportReviewReason::NoSupportedId),
        );
    };
    let types = candidate_types_for(config, &reference.provider, &item.bucket);
    if types.is_empty() {
        return (
            ImportPlanItemState::NeedsReview,
            types,
            None,
            Some(ImportReviewReason::NoTypeMatch),
        );
    }
    let candidate = item.lookup_candidate(Some(reference));
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
        let Some(reference) = planned_item.resolved_ref.as_ref() else {
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
    let reviewed_schema = state
        .import_jobs()
        .lock()
        .await
        .get(&job_id)
        .and_then(|record| record.schema.clone());
    if reviewed_schema.is_some()
        && serde_json::to_value(library.config.clone().into_parts().1).ok() != reviewed_schema
    {
        fail_commit(
            &state,
            &job_id,
            "The vault schema changed since this plan was reviewed. Review a new import plan.",
            cancel.load(Ordering::Relaxed),
        )
        .await;
        return;
    }
    let vfs = state.vault_vfs(&library.config.vault_root);
    let taxonomy_root = library.config.taxonomy_root.clone();
    let schema = match serde_json::to_value(&library.config) {
        Ok(schema) => schema,
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

    // Retain in-batch dedup even when a custom mapping does not index the
    // external reference on the created file.
    let saved = state.import_jobs().lock().await.get(&job_id).cloned();
    let mut created_refs: HashSet<(String, String)> = saved
        .as_ref()
        .into_iter()
        .flat_map(|record| record.creations.values())
        .filter(|intent| intent.created)
        .map(|intent| intent.provider_key.clone())
        .collect();

    for (position, planned_item) in planned.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let checkpoint = state.import_jobs().lock().await.get(&job_id).map(|record| {
            (
                record.outcomes.get(position).copied().flatten(),
                record.creations.get(&position).cloned(),
            )
        });
        if let Some((Some(_), _)) = checkpoint {
            continue;
        }
        if let Some((_, Some(intent))) = checkpoint {
            if !recover_creation(&state, &job_id, position, &intent, &reviewed_schema).await {
                return;
            }
            created_refs.insert(intent.provider_key);
            continue;
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
            checkpoint_outcome(&state, &job_id, position, ItemOutcome::Skipped, None).await;
            continue;
        }

        let title = planned_item.item.title.clone();
        let override_candidate = decision.and_then(|decision| decision.candidate_override.as_ref());
        let (provider, ref_url) = if let Some(candidate) = override_candidate {
            (candidate.provider.clone(), candidate.url.clone())
        } else if let Some(reference) = planned_item.resolved_ref.as_ref() {
            (reference.provider.clone(), reference.url.clone())
        } else {
            checkpoint_outcome(
                &state,
                &job_id,
                position,
                ItemOutcome::Failed,
                Some(format!(
                    "{title}: no resolvable source; provide a candidate"
                )),
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
            checkpoint_outcome(
                &state,
                &job_id,
                position,
                ItemOutcome::Failed,
                Some(format!("{title}: no entity type selected for this item")),
            )
            .await;
            continue;
        };
        let Some(type_config) = library.config.type_config(&target).cloned() else {
            checkpoint_outcome(
                &state,
                &job_id,
                position,
                ItemOutcome::Failed,
                Some(format!("{title}: unknown entity type '{target}'")),
            )
            .await;
            continue;
        };

        // Candidate: override or the item's partial, detail-fetched when the type
        // maps a metadata key the partial candidate lacks. A partial from a
        // provider other than the resolved one is discarded (a multi-ref item's
        // partial carries its primary provider) so mapping never mixes providers.
        let mut candidate = override_candidate.cloned().or_else(|| {
            planned_item
                .item
                .candidate
                .clone()
                .filter(|candidate| candidate.provider.eq_ignore_ascii_case(&provider))
        });
        if needs_detail_fetch(candidate.as_ref(), &type_config, &provider) {
            let resolved = tokio::select! {
                result = resolve_candidate(&state, &provider, &ref_url, std::slice::from_ref(&planned_item.item.bucket), None) => result,
                _ = super::import_cancelled(&cancel) => { finish_cancelled(&state, &job_id).await; return; }
            };
            match resolved {
                Ok(Some(fetched)) => candidate = Some(fetched),
                Ok(None) => {}
                Err(error) => {
                    if candidate.is_none() {
                        checkpoint_outcome(
                            &state,
                            &job_id,
                            position,
                            ItemOutcome::Failed,
                            Some(format!("{title}: {}", error.message())),
                        )
                        .await;
                        continue;
                    }
                }
            }
        }
        let Some(candidate) = candidate else {
            checkpoint_outcome(
                &state,
                &job_id,
                position,
                ItemOutcome::Failed,
                Some(format!("{title}: could not resolve a candidate")),
            )
            .await;
            continue;
        };

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
                checkpoint_outcome(
                    &state,
                    &job_id,
                    position,
                    ItemOutcome::Failed,
                    Some(format!("{title}: {}", error.message())),
                )
                .await;
                continue;
            }
        };
        let year = candidate_year(&type_config, &mapped_fields);
        // Provider traffic stays outside the mutation lock. Everything that
        // authorizes a write is checked again inside it: cancellation, current
        // schema/access, duplicates and filename availability. SAF has no native
        // watcher, so the earlier library snapshot is not authoritative here.
        let mutation = state.content_mutation_lock().await;
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        state.invalidate_cache().await;
        let current = match require_content_writes(&state).await {
            Ok(library) => library,
            Err(error) => {
                fail_commit(
                    &state,
                    &job_id,
                    error.message(),
                    cancel.load(Ordering::Relaxed),
                )
                .await;
                return;
            }
        };
        match serde_json::to_value(&current.config) {
            Ok(current_schema) if current_schema == schema => {}
            _ => {
                fail_commit(&state, &job_id, "The vault schema changed during import. Review a new plan before importing the remaining items.", cancel.load(Ordering::Relaxed)).await;
                return;
            }
        }
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let created_key = (provider.to_lowercase(), candidate.source_id.to_lowercase());
        if lookup_existing(&build_existing_index(&current), &candidate, &type_config.id).is_some()
            || created_refs.contains(&created_key)
        {
            checkpoint_outcome(&state, &job_id, position, ItemOutcome::Skipped, None).await;
            continue;
        }
        let basename = match resolve_free_basename(
            vfs.as_ref(),
            &taxonomy_root,
            &type_config,
            &current,
            &base,
            year.as_deref(),
            Some(&provider),
        )
        .await
        {
            Ok((basename, _)) => basename,
            Err(error) => {
                checkpoint_outcome(
                    &state,
                    &job_id,
                    position,
                    ItemOutcome::Failed,
                    Some(format!("{title}: {}", error.message())),
                )
                .await;
                continue;
            }
        };

        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let watched = request
            .options
            .mark_progress
            .then_some(planned_item.item.user.watched_count)
            .flatten();
        let intent = CreationCheckpoint {
            candidate: Some(candidate.clone()),
            entity_type: type_config.id.clone(),
            path: entity_create_path(&taxonomy_root, &type_config, &basename),
            raw: serialize_markdown_document(&frontmatter, &body),
            provider_key: created_key.clone(),
            watched,
            created: false,
            episodes_done: !request.options.import_episodes,
        };
        if !save_creation_intent(&state, &job_id, position, intent).await {
            return;
        }
        let written = write_new_entity_file(
            &mutation,
            vfs.as_ref(),
            &taxonomy_root,
            &type_config,
            &basename,
            &frontmatter,
            &body,
        )
        .await;
        // Publish each item before releasing the shared gate so another host
        // engine cannot retain an index from before this creation.
        state.invalidate_cache().await;
        match written {
            Ok(_) => {
                created_refs.insert(created_key);
                if !checkpoint_outcome(&state, &job_id, position, ItemOutcome::Created, None).await
                {
                    return;
                }
            }
            Err(error) => {
                pause_with_error(&state, &job_id, error.message().to_owned()).await;
                return;
            }
        }
        drop(mutation);
    }

    // Rebuild enrichment from durable creation checkpoints, not just entities
    // created during this particular run. Episode merging preserves existing
    // items and watched state, making a lost acknowledgement safe to replay.
    let created = state
        .import_jobs()
        .lock()
        .await
        .get(&job_id)
        .map(|record| {
            record
                .creations
                .iter()
                .filter(|(_, intent)| intent.created)
                .map(|(&position, intent)| (position, intent.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if request.options.import_episodes && !created.is_empty() && !cancel.load(Ordering::Relaxed) {
        state
            .update_import_job(&job_id, |job| {
                job.episodes_total = Some(created.len() as u32);
                job.episodes_processed = Some(
                    created
                        .iter()
                        .filter(|(_, intent)| intent.episodes_done)
                        .count() as u32,
                );
            })
            .await;
        for (position, intent) in &created {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if intent.episodes_done {
                continue;
            }
            state.invalidate_cache().await;
            let current = match require_content_writes(&state).await {
                Ok(current) => current,
                Err(error) => {
                    pause_with_error(&state, &job_id, error.message().to_owned()).await;
                    return;
                }
            };
            if serde_json::to_value(current.config.clone().into_parts().1).ok() != reviewed_schema {
                fail_commit(
                    &state,
                    &job_id,
                    "The vault schema changed during episode import.",
                    false,
                )
                .await;
                return;
            }
            if let Some(record) = current.record_by_path(&intent.path) {
                let original = split_markdown_document(&intent.raw);
                let source_changed = current
                    .config
                    .type_config(&record.summary.entity_type)
                    .is_none_or(|config| {
                        config
                            .fields
                            .iter()
                            .filter(|field| {
                                field.field_type == crate::types::FieldType::ExternalRef
                            })
                            .any(|field| {
                                original.frontmatter.get(&field.field)
                                    != record.frontmatter.get(&field.field)
                            })
                    });
                if source_changed {
                    pause_with_error(&state, &job_id, "The imported entity's source changed. Resolve it before resuming episode enrichment.".into()).await;
                    return;
                }
                if let Some(result) = import_new_entity_episodes_cancellable(
                    &state,
                    &current,
                    &record.summary.id,
                    intent.watched,
                    &cancel,
                )
                .await
                {
                    if let Some(error) = result.error {
                        if !record_episode_error(&state, &job_id, format!("episodes: {error}"))
                            .await
                        {
                            return;
                        }
                    }
                }
            }
            let mut jobs = state.import_jobs().lock().await;
            if let Some(record) = jobs.get_mut(&job_id) {
                if let Some(intent) = record.creations.get_mut(position) {
                    intent.episodes_done = true;
                }
                record.job.episodes_processed = Some(
                    record
                        .creations
                        .values()
                        .filter(|intent| intent.created && intent.episodes_done)
                        .count() as u32,
                );
                if let Err(error) = state.import_recovery().save(record).await {
                    record.job.paused = true;
                    record.cancel.store(true, Ordering::Relaxed);
                    push_error(
                        &mut record.job,
                        format!("Couldn't save episode checkpoint: {}", error.message()),
                    );
                    return;
                }
            }
        }
    }

    state.invalidate_cache().await;
    let cancelled = cancel.load(Ordering::Relaxed);
    state
        .update_import_job(&job_id, |job| {
            if job.paused {
                return;
            }
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

/// Outcomes and tallies are saved together; replay never increments them twice.
async fn checkpoint_outcome(
    state: &AppState,
    id: &str,
    position: usize,
    outcome: ItemOutcome,
    message: Option<String>,
) -> bool {
    let mut jobs = state.import_jobs().lock().await;
    let Some(record) = jobs.get_mut(id) else {
        return false;
    };
    if record.outcomes.len() < record.items.len() {
        record.outcomes.resize(record.items.len(), None);
    }
    if record.outcomes[position].is_none() {
        record.outcomes[position] = Some(outcome);
        record.job.processed += 1;
        match outcome {
            ItemOutcome::Created => {
                record.job.created += 1;
                if let Some(intent) = record.creations.get_mut(&position) {
                    intent.created = true;
                }
            }
            ItemOutcome::Skipped => record.job.skipped += 1,
            ItemOutcome::Failed => record.job.failed += 1,
        }
        if let Some(message) = message {
            push_error(&mut record.job, message);
        }
    }
    if let Err(error) = state.import_recovery().save(record).await {
        record.job.paused = true;
        record.cancel.store(true, Ordering::Relaxed);
        push_error(
            &mut record.job,
            format!("Couldn't save import checkpoint: {}", error.message()),
        );
        return false;
    }
    true
}

async fn save_creation_intent(
    state: &AppState,
    id: &str,
    position: usize,
    intent: CreationCheckpoint,
) -> bool {
    let mut jobs = state.import_jobs().lock().await;
    let Some(record) = jobs.get_mut(id) else {
        return false;
    };
    record.creations.insert(position, intent);
    if let Err(error) = state.import_recovery().save(record).await {
        record.job.paused = true;
        record.cancel.store(true, Ordering::Relaxed);
        push_error(
            &mut record.job,
            format!("Couldn't save import checkpoint: {}", error.message()),
        );
        return false;
    }
    true
}

/// Native hosts can resume pending enrichment. The web/desktop job registry
/// cannot, so retain its fail-safe episode behavior: report the error and finish
/// the remaining items instead of leaving an unresumable committing job.
async fn record_episode_error(state: &AppState, id: &str, message: String) -> bool {
    if state.options.host_asset_ingest {
        pause_with_error(state, id, message).await;
        false
    } else {
        state
            .update_import_job(id, |job| push_error(job, message))
            .await;
        true
    }
}

async fn pause_with_error(state: &AppState, id: &str, message: String) {
    let mut jobs = state.import_jobs().lock().await;
    if let Some(record) = jobs.get_mut(id) {
        if record.job.status == ImportJobStatus::Cancelled || record.job.paused {
            return;
        }
        if state.options.host_asset_ingest {
            record.job.paused = true;
        } else {
            record.job.status = ImportJobStatus::Failed;
            record.job.finished_at = Some(now_iso());
        }
        record.cancel.store(true, Ordering::Relaxed);
        push_error(&mut record.job, message);
        let _ = state.import_recovery().save(record).await;
    }
}

/// A write may have finished immediately before process termination. Adopt only
/// the exact document this job intended to create; don't clobber an occupied path.
async fn recover_creation(
    state: &AppState,
    id: &str,
    position: usize,
    intent: &CreationCheckpoint,
    reviewed_schema: &Option<serde_json::Value>,
) -> bool {
    let mutation = state.content_mutation_lock().await;
    if state
        .import_jobs()
        .lock()
        .await
        .get(id)
        .is_none_or(|record| record.cancel.load(Ordering::Relaxed))
    {
        return false;
    }
    state.invalidate_cache().await;
    let library = match require_content_writes(state).await {
        Ok(library) => library,
        Err(error) => {
            pause_with_error(state, id, error.message().to_owned()).await;
            return false;
        }
    };
    if serde_json::to_value(library.config.clone().into_parts().1).ok() != *reviewed_schema {
        fail_commit(
            state,
            id,
            "The vault schema changed. Review a new import plan.",
            false,
        )
        .await;
        return false;
    }
    let vfs = state.vault_vfs(&library.config.vault_root);
    let written = match vfs.exists(&intent.path).await {
        Ok(true) => match vfs.read_to_string(&intent.path).await {
            Ok(raw) if raw == intent.raw => Ok(()),
            Ok(_) => Err(ApiError::conflict(
                "An interrupted import's target file changed. Resolve it before resuming.",
            )),
            Err(error) => Err(anyhow::anyhow!(error).into()),
        },
        Ok(false) => {
            if intent.candidate.as_ref().is_some_and(|candidate| {
                lookup_existing(
                    &build_existing_index(&library),
                    candidate,
                    &intent.entity_type,
                )
                .is_some()
            }) {
                return checkpoint_outcome(state, id, position, ItemOutcome::Skipped, None).await;
            }
            write_entity_raw(&mutation, vfs.as_ref(), &intent.path, &intent.raw)
                .await
                .map_err(ApiError::from)
        }
        Err(error) => Err(anyhow::anyhow!(error).into()),
    };
    if let Err(error) = written {
        pause_with_error(state, id, error.message().to_owned()).await;
        return false;
    }
    state.invalidate_cache().await;
    checkpoint_outcome(state, id, position, ItemOutcome::Created, None).await
}

async fn fail_plan(state: &AppState, job_id: &str, message: &str) {
    let message = message.to_string();
    state
        .update_import_job(job_id, |job| {
            if job.paused {
                return;
            }
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
            if job.paused {
                return;
            }
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
            if job.paused {
                return;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::import::{create_import_job, ImportJobRecord};
    use crate::api::ApiOptions;
    use crate::contract::CreateImportJobRequest;
    use crate::secrets::NativeSecretStore;
    use crate::types::AppConfig;
    use crate::vfs::{InMemoryVfs, Vfs};
    use axum::{extract::State, Json};
    use serde_json::json;
    use std::time::Duration;

    const CONFIG: &str = "taxonomyRoot: Shelf\ntypes:\n  - id: anime\n    label: Anime\n    path: Anime\n    fields:\n      - field: title\n        fieldType: title\n        externalFields:\n          - source: myanimelist\n            field: title\n      - field: source\n        fieldType: externalRef\n        externalRef: myanimelist\n        externalTypes: [anime]\n";

    async fn fixture() -> (
        AppState,
        Arc<InMemoryVfs>,
        String,
        Vec<PlannedItem>,
        Arc<AtomicBool>,
    ) {
        let fs = Arc::new(InMemoryVfs::new());
        fs.insert_file("KizunaShelf/config.yaml", CONFIG);
        let state = AppState::with_vault(
            ApiOptions {
                cache_ttl: Duration::from_secs(3600),
                web_dist_path: None,
                settings_writable: true,
                content_writable: true,
                host_asset_ingest: false,
                index_cache_dir: None,
                import_state_dir: None,
                index_cache_identity: None,
            },
            fs.clone(),
            AppConfig {
                vault_root: "test".into(),
                content_writable: Some(true),
            },
            Arc::new(NativeSecretStore::with_token_path(
                std::path::PathBuf::from("/tmp/import-race-unused-tokens.json"),
            )),
        );
        let request: CreateImportJobRequest = serde_json::from_value(json!({"source":"yamtrack","input":{"csvText":"media_id,source,media_type,title,status\n1,mal,anime,Imported title,Planning\n"}})).unwrap();
        let job = create_import_job(State(state.clone()), Json(request))
            .await
            .unwrap_or_else(|error| panic!("{}", error.message()))
            .0;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if state.import_jobs().lock().await[&job.id].job.status == ImportJobStatus::Planned
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let (items, cancel) = {
            let mut jobs = state.import_jobs().lock().await;
            let ImportJobRecord {
                job, items, cancel, ..
            } = jobs.get_mut(&job.id).unwrap();
            job.status = ImportJobStatus::Committing;
            (items.clone(), cancel.clone())
        };
        (state, fs, job.id, items, cancel)
    }

    async fn race(change: &str) -> (ImportJob, Vec<String>) {
        let (state, fs, id, items, cancel) = fixture().await;
        let held = state.content_mutation_lock().await;
        let request = serde_json::from_value(
            json!({"options":{"importUserData":true,"importEpisodes":false,"markProgress":false}}),
        )
        .unwrap();
        let worker = run_commit_job(state.clone(), id.clone(), items, request, cancel.clone());
        tokio::pin!(worker);
        // The worker has a warm library and a complete offline CSV candidate.
        // Drive it until it is waiting for the held mutation lock.
        assert!(
            tokio::time::timeout(Duration::from_millis(20), worker.as_mut())
                .await
                .is_err()
        );
        match change {
            "duplicate" => fs.insert_file("Shelf/Anime/Kept.md", "---\ntitle: Different local title\nsource: https://myanimelist.net/anime/1\n---\nKeep original notes\n"),
            "schema" => fs.insert_file("KizunaShelf/config.yaml", &CONFIG.replace("path: Anime", "path: Changed")),
            "malformed" => fs.insert_file("KizunaShelf/config.yaml", "types: ["),
            "cancel" => cancel.store(true, Ordering::Relaxed),
            _ => unreachable!(),
        }
        // No manual cache invalidation: SAF/cloud changes have no native watcher.
        drop(held);
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .unwrap();
        let files = fs
            .read_dir("Shelf/Anime")
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|f| f.name)
            .collect();
        let job = state.import_jobs().lock().await[&id].job.clone();
        (job, files)
    }

    #[tokio::test]
    async fn import_rechecks_duplicates_after_waiting_for_mutation() {
        let (job, files) = race("duplicate").await;
        assert_eq!(job.created, 0);
        assert_eq!(job.skipped, 1);
        assert_eq!(files, vec!["Kept.md"]);
    }
    #[tokio::test]
    async fn import_rejects_schema_changes_while_waiting_for_mutation() {
        let (job, files) = race("schema").await;
        assert_eq!(job.status, ImportJobStatus::Failed);
        assert_eq!(job.created, 0);
        assert!(files.is_empty());
    }
    #[tokio::test]
    async fn import_rejects_malformed_config_after_waiting_for_mutation() {
        let (job, files) = race("malformed").await;
        assert_eq!(job.status, ImportJobStatus::Failed);
        assert_eq!(job.created, 0);
        assert!(files.is_empty());
    }
    #[tokio::test]
    async fn import_cancelled_while_waiting_never_starts_a_write() {
        let (job, files) = race("cancel").await;
        assert_eq!(job.status, ImportJobStatus::Cancelled);
        assert_eq!(job.created, 0);
        assert!(files.is_empty());
    }
    fn durable_state(
        fs: Arc<InMemoryVfs>,
        directory: &std::path::Path,
        identity: &str,
    ) -> AppState {
        AppState::with_vault(
            ApiOptions {
                cache_ttl: Duration::ZERO,
                web_dist_path: None,
                settings_writable: true,
                content_writable: true,
                host_asset_ingest: true,
                index_cache_dir: None,
                import_state_dir: Some(directory.to_path_buf()),
                index_cache_identity: Some(identity.into()),
            },
            fs,
            AppConfig {
                vault_root: "test".into(),
                content_writable: Some(true),
            },
            Arc::new(NativeSecretStore::with_token_path(directory.join("tokens"))),
        )
    }

    async fn interrupted_fixture(
        written: bool,
        acknowledged: bool,
    ) -> (tempfile::TempDir, Arc<InMemoryVfs>, String) {
        let (state, fs, id, _, _) = fixture().await;
        let dir = tempfile::tempdir().unwrap();
        let durable = durable_state(fs.clone(), dir.path(), "recovery-vault");
        let mut record = state.import_jobs().lock().await[&id].clone();
        let request: CommitImportJobRequest = serde_json::from_value(
            json!({"options":{"importUserData":true,"importEpisodes":true,"markProgress":true}}),
        )
        .unwrap();
        record.commit = Some(request);
        record.running = true;
        record.job.paused = false;
        record.job.processed = u32::from(acknowledged);
        record.job.created = u32::from(acknowledged);
        record.outcomes = vec![acknowledged.then_some(ItemOutcome::Created)];
        let raw =
            "---\ntitle: Imported title\nsource: https://myanimelist.net/anime/1\n---\n".to_owned();
        let intent = CreationCheckpoint {
            candidate: record.items[0].item.candidate.clone(),
            entity_type: "anime".into(),
            path: "Shelf/Anime/Imported title.md".into(),
            raw: raw.clone(),
            provider_key: ("myanimelist".into(), "1".into()),
            watched: Some(5),
            created: acknowledged,
            episodes_done: false,
        };
        record.creations.insert(0, intent);
        if written {
            fs.insert_file("Shelf/Anime/Imported title.md", &raw);
        }
        durable
            .import_recovery()
            .save(&record)
            .await
            .unwrap_or_else(|e| panic!("{}", e.message()));
        drop(durable);
        (dir, fs, id)
    }

    async fn resume_and_wait(state: &AppState, id: &str) -> ImportJob {
        use crate::api::import::{resume_import_job, ImportJobPath};
        let resumed = resume_import_job(
            State(state.clone()),
            axum::extract::Path(ImportJobPath { id: id.into() }),
        )
        .await
        .unwrap_or_else(|e| panic!("{}", e.message()))
        .0;
        assert!(!resumed.paused);
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let jobs = state.import_jobs().lock().await;
                let record = &jobs[id];
                if !record.running {
                    break record.job.clone();
                }
                drop(jobs);
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn resumes_creation_intent_before_or_after_vault_write_without_duplicates() {
        for written in [false, true] {
            let (dir, fs, id) = interrupted_fixture(written, false).await;
            let state = durable_state(fs.clone(), dir.path(), "recovery-vault");
            assert!(state.import_jobs().lock().await[&id].job.paused);
            let job = resume_and_wait(&state, &id).await;
            assert_eq!(job.status, ImportJobStatus::Completed);
            assert_eq!(job.created, 1);
            assert_eq!(job.processed, 1);
            assert_eq!(job.episodes_processed, Some(1));
            assert_eq!(fs.read_dir("Shelf/Anime").await.unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn resumes_episode_enrichment_for_previously_created_entities() {
        let (dir, fs, id) = interrupted_fixture(true, true).await;
        let state = durable_state(fs.clone(), dir.path(), "recovery-vault");
        assert!(!state.import_jobs().lock().await[&id].creations[&0].episodes_done);
        let job = resume_and_wait(&state, &id).await;
        assert_eq!(job.created, 1);
        assert_eq!(job.skipped, 0);
        assert_eq!(job.episodes_total, Some(1));
        assert_eq!(job.episodes_processed, Some(1));
        assert!(state.import_jobs().lock().await[&id].creations[&0].episodes_done);
        // A new engine sees the acknowledged terminal job, not another import.
        drop(state);
        let reloaded = durable_state(fs, dir.path(), "recovery-vault");
        assert_eq!(
            reloaded.import_jobs().lock().await[&id].job.status,
            ImportJobStatus::Completed
        );
    }

    #[tokio::test]
    async fn interrupted_creation_never_overwrites_an_occupied_target() {
        let (dir, fs, id) = interrupted_fixture(true, false).await;
        fs.insert_file(
            "Shelf/Anime/Imported title.md",
            "Keep this user's document\n",
        );
        let state = durable_state(fs.clone(), dir.path(), "recovery-vault");
        let job = resume_and_wait(&state, &id).await;
        assert!(job.paused);
        assert_eq!(job.created, 0);
        assert_eq!(
            fs.read_to_string("Shelf/Anime/Imported title.md")
                .await
                .unwrap(),
            "Keep this user's document\n"
        );
    }

    #[tokio::test]
    async fn saved_imports_are_isolated_by_vault_identity() {
        let (dir, fs, id) = interrupted_fixture(true, true).await;
        let other = durable_state(fs, dir.path(), "another-vault");
        assert!(!other.import_jobs().lock().await.contains_key(&id));
    }

    #[tokio::test]
    async fn an_unwritable_journal_prevents_new_vault_mutations() {
        let (state, fs, id, _, _) = fixture().await;
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("not-a-directory");
        std::fs::write(&blocked, "occupied").unwrap();
        let durable = durable_state(fs.clone(), &blocked, "recovery-vault");
        let mut record = state.import_jobs().lock().await[&id].clone();
        record.running = false;
        durable
            .import_jobs()
            .lock()
            .await
            .insert(id.clone(), record);
        let request: CommitImportJobRequest = serde_json::from_value(
            json!({"options":{"importUserData":true,"importEpisodes":false,"markProgress":false}}),
        )
        .unwrap();
        let result = crate::api::import::commit_import_job(
            State(durable.clone()),
            axum::extract::Path(super::super::ImportJobPath { id }),
            Json(request),
        )
        .await;
        assert!(result.is_err());
        assert!(fs
            .read_dir("Shelf/Anime")
            .await
            .unwrap_or_default()
            .is_empty());
    }
    #[tokio::test]
    async fn resumed_intent_rechecks_duplicates_created_during_suspension() {
        let (dir, fs, id) = interrupted_fixture(false, false).await;
        fs.insert_file(
            "Shelf/Anime/Elsewhere.md",
            "---\ntitle: Another title\nsource: https://myanimelist.net/anime/1\n---\n",
        );
        let state = durable_state(fs.clone(), dir.path(), "recovery-vault");
        let job = resume_and_wait(&state, &id).await;
        assert_eq!(job.created, 0);
        assert_eq!(job.skipped, 1);
        assert_eq!(fs.read_dir("Shelf/Anime").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn recovered_episode_enrichment_does_not_follow_a_repointed_source() {
        let (dir, fs, id) = interrupted_fixture(true, true).await;
        fs.insert_file(
            "Shelf/Anime/Imported title.md",
            "---\ntitle: Imported title\nsource: https://myanimelist.net/anime/2\n---\n",
        );
        let state = durable_state(fs.clone(), dir.path(), "recovery-vault");
        let job = resume_and_wait(&state, &id).await;
        assert!(job.paused);
        assert!(!state.import_jobs().lock().await[&id].creations[&0].episodes_done);
    }

    #[tokio::test]
    async fn unreadable_recovery_fails_closed_and_can_be_retried_after_repair() {
        let (dir, fs, id) = interrupted_fixture(true, true).await;
        let file = dir
            .path()
            .join(crate::library::file_revision("recovery-vault"))
            .join(format!("{}.json", crate::library::file_revision(&id)));
        let original = std::fs::read(&file).unwrap();
        std::fs::write(&file, "unreadable journal").unwrap();
        let state = durable_state(fs, dir.path(), "recovery-vault");
        assert!(state.import_recovery().check().is_err());
        assert!(state.import_jobs().lock().await.is_empty());
        std::fs::write(&file, original).unwrap();
        assert!(state.import_recovery().check().is_ok());
        assert!(state.import_jobs().lock().await[&id].job.paused);
    }
    #[tokio::test]
    async fn a_late_worker_error_does_not_resurrect_a_cancelled_import() {
        let (state, _, id, _, _) = fixture().await;
        state
            .import_jobs()
            .lock()
            .await
            .get_mut(&id)
            .unwrap()
            .job
            .status = ImportJobStatus::Cancelled;
        pause_with_error(&state, &id, "A provider request stopped".into()).await;
        let jobs = state.import_jobs().lock().await;
        assert_eq!(jobs[&id].job.status, ImportJobStatus::Cancelled);
        assert!(!jobs[&id].job.paused);
        assert!(jobs[&id].job.errors.is_empty());
    }
    #[tokio::test]
    async fn web_import_errors_become_terminal_instead_of_unresumable_pauses() {
        let (state, _, id, _, _) = fixture().await;
        pause_with_error(&state, &id, "The vault could not be written".into()).await;
        let jobs = state.import_jobs().lock().await;
        assert_eq!(jobs[&id].job.status, ImportJobStatus::Failed);
        assert!(!jobs[&id].job.paused);
        assert!(jobs[&id].job.finished_at.is_some());
        assert_eq!(jobs[&id].job.errors, vec!["The vault could not be written"]);
    }

    #[tokio::test]
    async fn web_episode_errors_are_reported_without_stalling_the_create_job() {
        let (state, _, id, _, _) = fixture().await;
        assert!(record_episode_error(&state, &id, "episodes: provider unavailable".into()).await);
        let jobs = state.import_jobs().lock().await;
        assert_eq!(jobs[&id].job.status, ImportJobStatus::Committing);
        assert!(!jobs[&id].job.paused);
        assert!(!jobs[&id].cancel.load(Ordering::Relaxed));
        assert_eq!(jobs[&id].job.errors, vec!["episodes: provider unavailable"]);
    }

    #[tokio::test]
    async fn native_episode_errors_keep_enrichment_pending_for_resume() {
        let (dir, fs, id) = interrupted_fixture(true, true).await;
        let state = durable_state(fs, dir.path(), "recovery-vault");
        state
            .import_jobs()
            .lock()
            .await
            .get_mut(&id)
            .unwrap()
            .job
            .paused = false;
        assert!(!record_episode_error(&state, &id, "episodes: provider unavailable".into()).await);
        let jobs = state.import_jobs().lock().await;
        assert!(jobs[&id].job.paused);
        assert_eq!(jobs[&id].job.status, ImportJobStatus::Committing);
        assert!(!jobs[&id].creations[&0].episodes_done);
    }
}
