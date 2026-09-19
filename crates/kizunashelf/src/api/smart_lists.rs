//! Handlers for smart lists — criteria-driven lists stored as Obsidian Bases
//! `.base` files under [`LISTS_DIR`](crate::lists::LISTS_DIR); see
//! [`crate::smart_lists`] for the file model and evaluation. All vault I/O
//! goes through the [`Vfs`], writes are gated by [`require_content_writes`]
//! and revision-guarded, and every edit preserves unsupported constructs in
//! the underlying YAML verbatim.
//!
//! This module also owns the mapping between the depth-limited contract
//! criteria model ([`SmartFilterGroup`]) and the core filter tree
//! ([`FilterNode`]) — the core stays contract-free, like `crate::lists`.

use super::error::{ApiError, ApiResult};
use super::list_files::{basename_is_free, SMART_LIST};
use super::mutations::{check_revision, derive_basename, write_entity_raw};
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{
    CreateSmartListRequest, DeleteListResponse, EntityListResponse, ListKind, ListSummary,
    SmartCompareOp, SmartContainsMode, SmartDurationUnit, SmartFilterConjunction, SmartFilterGroup,
    SmartFilterRule, SmartFilterRuleKind, SmartFilterSubgroup, SmartListDetail,
    SmartListPreviewRequest, SmartListView, SmartSortSpec, SmartViewLayout, UpdateSmartListRequest,
};
use crate::dates::clamp_number;
use crate::library::file_revision;
use crate::lists::LISTS_DIR;
use crate::relations::SortDirection as CoreDirection;
use crate::smart_lists::{
    self, apply_views, default_smart_list_doc, filter_node_to_yaml, parse_smart_list,
    print_sort_property, render_smart_list, scope_from_filters, set_global_filters,
    type_scope_folder, AtomKind, CompareOp, CompareValue, Conjunction, ContainsMode, DateBase,
    DateExpr, DateOffset, DurationSpec, EvalContext, FieldRef, FilterAtom, FilterNode, SmartList,
    SmartView, ViewLayout, ViewSort, ViewSpec, SMART_LIST_EXTENSION,
};
use crate::types::{FieldType, KizunaConfig, Library, SortDirection as ContractDirection};
use crate::vfs::{Vfs, VfsResult};
use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct SmartListPath {
    id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SmartListResultsQuery {
    /// View (tab) name to evaluate; defaults to the file's first supported view.
    view: Option<String>,
    page: Option<f64>,
    page_size: Option<f64>,
    title_language: Option<String>,
    /// Free-text search within the list's matches (titles, summary, path),
    /// ranked by relevance when the view declares no sort of its own.
    q: Option<String>,
    /// Today's date (`YYYY-MM-DD`), the client's **local** date, so `today()`
    /// date criteria are judged against the user's day rather than the host's
    /// clock. Falls back to the host's local date.
    today: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub(crate) async fn get_smart_list(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<SmartListPath>,
) -> ApiResult<SmartListDetail> {
    let library = get_library(&state).await?;
    let path = SMART_LIST.path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let raw = SMART_LIST.read_raw(vfs.as_ref(), &path).await?;
    let list = parse_list_raw(&path, &raw)?;
    Ok(Json(detail_from_list(&path, &list, &raw, &library.config)))
}

pub(crate) async fn create_smart_list(
    State(state): State<AppState>,
    Json(request): Json<CreateSmartListRequest>,
) -> ApiResult<SmartListDetail> {
    let library = require_content_writes(&state).await?;
    let mutation = state.content_mutation_lock().await;
    let scope_folder = resolve_scope(&library.config, request.scope.as_deref())?;
    let image = request
        .scope
        .as_deref()
        .and_then(|type_id| cover_property(&library.config, type_id));
    let vfs = state.vault_vfs(&library.config.vault_root);
    let path = SMART_LIST.new_path(vfs.as_ref(), &request.name).await?;
    let doc = default_smart_list_doc(scope_folder.as_deref(), image.as_deref());
    let raw = render_smart_list(&doc);
    write_entity_raw(&mutation, vfs.as_ref(), &path, &raw).await?;
    let list = parse_list_raw(&path, &raw)?;
    Ok(Json(detail_from_list(&path, &list, &raw, &library.config)))
}

pub(crate) async fn update_smart_list(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<SmartListPath>,
    Json(request): Json<UpdateSmartListRequest>,
) -> ApiResult<SmartListDetail> {
    let library = require_content_writes(&state).await?;
    let mutation = state.content_mutation_lock().await;
    let source_path = SMART_LIST.path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);

    let raw = SMART_LIST.read_raw(vfs.as_ref(), &source_path).await?;
    check_revision(&request.revision, &file_revision(&raw))?;
    let mut doc = parse_list_raw(&source_path, &raw)?.doc;

    let scope_folder = resolve_scope(&library.config, request.scope.as_deref())?;
    let filters_group = request.filters.unwrap_or_default();
    let filters = group_to_node(&filters_group, scope_folder.as_deref())?;
    set_global_filters(&mut doc, &filters);

    let default_image = request
        .scope
        .as_deref()
        .and_then(|type_id| cover_property(&library.config, type_id));
    let specs = request
        .views
        .iter()
        .map(|view| contract_view_to_spec(view, default_image.as_deref()))
        .collect::<Result<Vec<_>, _>>()?;
    apply_views(&mut doc, &specs);

    let target_path = SMART_LIST
        .rename_target(vfs.as_ref(), &source_path, request.rename_to.as_deref())
        .await?;
    let new_raw = render_smart_list(&doc);
    SMART_LIST
        .write_replacing(
            &mutation,
            vfs.as_ref(),
            &source_path,
            &target_path,
            &request.revision,
            &new_raw,
        )
        .await?;

    let list = parse_list_raw(&target_path, &new_raw)?;
    Ok(Json(detail_from_list(
        &target_path,
        &list,
        &new_raw,
        &library.config,
    )))
}

pub(crate) async fn delete_smart_list(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<SmartListPath>,
) -> ApiResult<DeleteListResponse> {
    let library = require_content_writes(&state).await?;
    let mutation = state.content_mutation_lock().await;
    let path = SMART_LIST.path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let backup_path = SMART_LIST.trash(&mutation, vfs.as_ref(), &path).await?;
    Ok(Json(DeleteListResponse {
        deleted_id: path_param.id,
        backup_path,
    }))
}

pub(crate) async fn smart_list_results(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<SmartListPath>,
    Query(query): Query<SmartListResultsQuery>,
) -> ApiResult<EntityListResponse> {
    let library = get_library(&state).await?;
    let path = SMART_LIST.path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let raw = SMART_LIST.read_raw(vfs.as_ref(), &path).await?;
    let list = parse_list_raw(&path, &raw)?;

    let view = match query.view.as_deref() {
        Some(name) => Some(
            list.views
                .iter()
                .find(|view| view.name == name)
                .ok_or_else(|| ApiError::not_found("Smart list view not found"))?,
        ),
        None => list.views.first(),
    };
    let ctx = eval_context(&library, query.today.as_deref());
    let records = smart_lists::smart_list_records(
        &list,
        view,
        &ctx,
        &smart_lists::ResultOptions {
            title_language: query.title_language.as_deref(),
            query: query.q.as_deref(),
        },
    );
    Ok(Json(paginate(
        records,
        query.page.unwrap_or(1.0),
        query.page_size.unwrap_or(40.0),
    )))
}

pub(crate) async fn preview_smart_list(
    State(state): State<AppState>,
    Json(request): Json<SmartListPreviewRequest>,
) -> ApiResult<EntityListResponse> {
    let library = get_library(&state).await?;
    let scope_folder = resolve_scope(&library.config, request.scope.as_deref())?;
    let filters_group = request.filters.unwrap_or_default();
    let filters = group_to_node(&filters_group, scope_folder.as_deref())?;
    let list = SmartList {
        doc: serde_yaml::Mapping::new(),
        filters,
        views: Vec::new(),
        warnings: Vec::new(),
    };
    let view = SmartView {
        layout: ViewLayout::List,
        name: "preview".to_string(),
        filters: None,
        sort: request.sort.iter().map(contract_sort_to_core).collect(),
        limit: request.limit.map(u64::from),
        image: None,
        source_index: 0,
    };
    let ctx = eval_context(&library, request.today.as_deref());
    let records = smart_lists::smart_list_records(
        &list,
        Some(&view),
        &ctx,
        &smart_lists::ResultOptions {
            title_language: request.title_language.as_deref(),
            query: request.q.as_deref(),
        },
    );
    Ok(Json(paginate(
        records,
        request.page.unwrap_or(1.0),
        request.page_size.unwrap_or(40.0),
    )))
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct SmartListSuggestionsQuery {
    language: Option<String>,
}

/// A Home toggle edits only metadata, never a stale copy of the criteria.
pub(crate) async fn set_smart_list_home(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<SmartListPath>,
    Json(request): Json<crate::contract::SetSmartListHomeRequest>,
) -> ApiResult<SmartListDetail> {
    let library = require_content_writes(&state).await?;
    let mutation = state.content_mutation_lock().await;
    let path = SMART_LIST.path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let raw = SMART_LIST.read_raw(vfs.as_ref(), &path).await?;
    check_revision(&request.revision, &file_revision(&raw))?;
    let mut doc = parse_list_raw(&path, &raw)?.doc;
    smart_lists::set_home_visibility(&mut doc, request.show_on_home)
        .map_err(|error| ApiError::bad_request(&error))?;
    let latest = SMART_LIST.read_raw(vfs.as_ref(), &path).await?;
    check_revision(&request.revision, &file_revision(&latest))?;
    let raw = render_smart_list(&doc);
    write_entity_raw(&mutation, vfs.as_ref(), &path, &raw).await?;
    let list = parse_list_raw(&path, &raw)?;
    Ok(Json(detail_from_list(&path, &list, &raw, &library.config)))
}

/// Used by Home and onboarding; suggestions derive from the live schema, so
/// users can recreate them after setup without re-adding types or resetting files.
pub(crate) async fn smart_list_suggestions(
    State(state): State<AppState>,
    Query(query): Query<SmartListSuggestionsQuery>,
) -> ApiResult<crate::contract::SmartListSuggestionsResponse> {
    let library = get_library(&state).await?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let files = read_smart_list_files(vfs.as_ref()).await?;
    let suggestions =
        crate::presets::suggested_lists(&library.config.types, query.language.as_deref())
            .into_iter()
            .map(|suggestion| {
                let existing = files.iter().find(|(_, _, list)| {
                    smart_lists::suggestion_id(list) == Some(suggestion.id.as_str())
                });
                crate::contract::SmartListSuggestion {
                    id: suggestion.id,
                    name: existing
                        .map(|(path, _, _)| SMART_LIST.id(path))
                        .unwrap_or(suggestion.title),
                    entity_type: suggestion.entity_type,
                    existing_list_id: existing.map(|(path, _, _)| SMART_LIST.id(path)),
                    show_on_home: existing
                        .is_some_and(|(_, _, list)| smart_lists::shows_on_home(list)),
                }
            })
            .collect();
    Ok(Json(crate::contract::SmartListSuggestionsResponse {
        suggestions,
    }))
}

pub(crate) async fn create_suggested_smart_lists(
    State(state): State<AppState>,
    Json(request): Json<crate::contract::CreateSuggestedSmartListsRequest>,
) -> ApiResult<crate::contract::CreateSuggestedSmartListsResponse> {
    let library = require_content_writes(&state).await?;
    let mutation = state.content_mutation_lock().await;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let files = read_smart_list_files(vfs.as_ref()).await?;
    let suggestions =
        crate::presets::suggested_lists(&library.config.types, request.language.as_deref());
    if request.suggestion_ids.as_ref().is_some_and(|ids| {
        ids.iter()
            .any(|id| !suggestions.iter().any(|suggestion| &suggestion.id == id))
    }) {
        return Err(ApiError::bad_request(
            "Unknown smart list suggestion; refresh and try again",
        ));
    }
    let mut lists = Vec::new();
    for suggestion in suggestions.iter().filter(|suggestion| {
        request
            .suggestion_ids
            .as_ref()
            .is_none_or(|ids| ids.contains(&suggestion.id))
    }) {
        if let Some((path, raw, list)) = files
            .iter()
            .find(|(_, _, list)| smart_lists::suggestion_id(list) == Some(suggestion.id.as_str()))
        {
            // Reuse even a renamed/edited suggestion. Only Home membership changes.
            if smart_lists::shows_on_home(list) {
                lists.push(detail_from_list(path, list, raw, &library.config));
                continue;
            }
            let mut doc = list.doc.clone();
            smart_lists::set_home_visibility(&mut doc, true)
                .map_err(|error| ApiError::bad_request(&error))?;
            let latest = SMART_LIST.read_raw(vfs.as_ref(), path).await?;
            check_revision(&file_revision(raw), &file_revision(&latest))?;
            let raw = render_smart_list(&doc);
            write_entity_raw(&mutation, vfs.as_ref(), path, &raw).await?;
            let list = parse_list_raw(path, &raw)?;
            lists.push(detail_from_list(path, &list, &raw, &library.config));
            continue;
        }
        let basename = derive_basename(&suggestion.title)
            .or_else(|| derive_basename(&suggestion.id))
            .ok_or_else(|| ApiError::bad_request("Cannot derive a filename for this suggestion"))?;
        // A same-named list of either kind is never replaced or shadowed.
        let mut name = basename.clone();
        let mut suffix = 2;
        while !basename_is_free(vfs.as_ref(), &name, None).await? {
            name = format!("{basename} {suffix}");
            suffix += 1;
        }
        let path = SMART_LIST.path(&name)?;
        let scope = resolve_scope(&library.config, Some(&suggestion.entity_type))?;
        let image = cover_property(&library.config, &suggestion.entity_type);
        let mut doc = default_smart_list_doc(scope.as_deref(), image.as_deref());
        let filters = group_to_node(
            &suggestion.criteria.clone().unwrap_or_default(),
            scope.as_deref(),
        )?;
        set_global_filters(&mut doc, &filters);
        let initial = parse_list_raw(&path, &render_smart_list(&doc))?;
        let views = initial
            .views
            .iter()
            .map(|view| {
                let mut view = view_to_contract(view);
                view.sort = vec![suggestion.sort.clone()];
                contract_view_to_spec(&view, image.as_deref())
            })
            .collect::<Result<Vec<_>, _>>()?;
        apply_views(&mut doc, &views);
        let mut metadata = serde_yaml::Mapping::new();
        metadata.insert("suggestion".into(), suggestion.id.clone().into());
        doc.insert("kizunashelf".into(), serde_yaml::Value::Mapping(metadata));
        smart_lists::set_home_visibility(&mut doc, true)
            .map_err(|error| ApiError::bad_request(&error))?;
        let raw = render_smart_list(&doc);
        write_entity_raw(&mutation, vfs.as_ref(), &path, &raw).await?;
        let list = parse_list_raw(&path, &raw)?;
        lists.push(detail_from_list(&path, &list, &raw, &library.config));
    }
    Ok(Json(crate::contract::CreateSuggestedSmartListsResponse {
        lists,
    }))
}

async fn read_smart_list_files(
    vfs: &dyn Vfs,
) -> Result<Vec<(String, String, SmartList)>, ApiError> {
    let listing = match smart_list_file_listing(vfs).await {
        Ok(listing) => listing,
        Err(error) if error.is_not_found() => return Ok(Vec::new()),
        Err(error) => return Err(anyhow::anyhow!(error).into()),
    };
    let files = vfs
        .read_files(&listing.paths)
        .await
        .map_err(|error| anyhow::anyhow!(error))?;
    Ok(files
        .into_iter()
        .filter_map(|(path, bytes)| {
            let raw = String::from_utf8(bytes).ok()?;
            let list = parse_smart_list(&raw).ok()?;
            Some((path, raw, list))
        })
        .collect())
}

pub(crate) async fn home_lists(
    state: &AppState,
    library: &Library,
    today: Option<&str>,
    language: Option<&str>,
) -> Result<crate::contract::HomeResponse, ApiError> {
    let vfs = state.vault_vfs(&library.config.vault_root);
    let files = read_smart_list_files(vfs.as_ref()).await?;
    let ctx = EvalContext::new(library, chrono::Utc::now(), resolve_today(today));
    let lists = files
        .iter()
        .filter(|(_, _, list)| smart_lists::shows_on_home(list))
        .map(|(path, _, list)| {
            let view = list.views.first();
            let records = smart_lists::smart_list_records(
                list,
                view,
                &ctx,
                &smart_lists::ResultOptions {
                    title_language: language,
                    query: None,
                },
            );
            crate::contract::HomeListResponse {
                id: SMART_LIST.id(path),
                name: SMART_LIST.id(path),
                view: view.map(|view| view.name.clone()),
                total: records.len(),
                items: records
                    .into_iter()
                    .take(12)
                    .map(|record| record.summary.clone())
                    .collect(),
            }
        })
        .collect();
    Ok(crate::contract::HomeResponse {
        generated_at: library.generated_at.clone(),
        lists,
    })
}

// ---------------------------------------------------------------------------
// Lists-index integration
// ---------------------------------------------------------------------------

struct SmartListFileListing {
    paths: Vec<String>,
    /// Hash of every `.base` path, length, and mtime. `None` means the VFS
    /// cannot report a trustworthy mtime, so callers must bypass the memo.
    fingerprint: Option<u64>,
}

async fn smart_list_file_listing(vfs: &dyn Vfs) -> VfsResult<SmartListFileListing> {
    let entries = vfs.read_dir(LISTS_DIR).await?;
    let suffix = format!(".{SMART_LIST_EXTENSION}");
    let mut files: Vec<_> = entries
        .into_iter()
        .filter(|entry| entry.is_file && entry.name.ends_with(&suffix))
        .map(|entry| {
            (
                format!("{LISTS_DIR}/{}", entry.name),
                entry.len,
                entry.modified_unix_nanos,
            )
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let fingerprint = files.iter().try_fold(
        std::collections::hash_map::DefaultHasher::new(),
        |mut hasher, (path, len, modified)| {
            if *modified == 0 {
                return None;
            }
            path.hash(&mut hasher);
            len.hash(&mut hasher);
            modified.hash(&mut hasher);
            Some(hasher)
        },
    );
    Ok(SmartListFileListing {
        paths: files.into_iter().map(|(path, _, _)| path).collect(),
        fingerprint: fingerprint.map(|hasher| hasher.finish()),
    })
}

#[derive(Clone)]
struct CachedSmartListSummary {
    summary: ListSummary,
    filters: FilterNode,
    uses_now: bool,
}

/// Resident smart-list index data. Counts and parsed filters are reused for an
/// unchanged library/list catalog/day; membership remains request-specific and
/// is evaluated against the one requested entity rather than the whole vault.
pub(crate) struct SmartListSummaryCache {
    rows: Vec<CachedSmartListSummary>,
}

/// The smart-list rows of the lists index: one [`ListSummary`] per parseable
/// `.base` file, with `itemCount` evaluated from the global filters (view
/// limits intentionally ignored — membership means "matches the criteria").
/// Unparseable files are skipped; the detail endpoint reports their error.
pub(crate) async fn smart_list_summaries(
    state: &AppState,
    vfs: &dyn Vfs,
    library: &Library,
    entity: Option<&str>,
    today: Option<&str>,
) -> VfsResult<Vec<ListSummary>> {
    let listing = match smart_list_file_listing(vfs).await {
        Ok(listing) => listing,
        Err(error) if error.is_not_found() => SmartListFileListing {
            paths: Vec::new(),
            fingerprint: Some(0),
        },
        Err(error) => return Err(error),
    };
    let resolved_today = resolve_today(today);
    let build = || async {
        let files = vfs.read_files(&listing.paths).await?;
        let ctx = EvalContext::new(library, chrono::Utc::now(), resolved_today);
        let rows = files
            .into_iter()
            .filter_map(|(path, bytes)| String::from_utf8(bytes).ok().map(|raw| (path, raw)))
            .filter_map(|(path, raw)| {
                let list = parse_smart_list(&raw).ok()?;
                let item_count = library
                    .records
                    .iter()
                    .filter(|record| smart_lists::record_matches(&list.filters, record, &ctx))
                    .count();
                let id = SMART_LIST.id(&path);
                Some(CachedSmartListSummary {
                    summary: ListSummary {
                        name: id.clone(),
                        id,
                        kind: ListKind::Smart,
                        path,
                        description: String::new(),
                        item_count,
                        section_count: 0,
                        contains: None,
                    },
                    uses_now: filters_use_now(&list.filters),
                    filters: list.filters,
                })
            })
            .collect();
        Ok(Arc::new(SmartListSummaryCache { rows }))
    };

    let cached = match listing.fingerprint {
        Some(fingerprint) => {
            let library_revision = smart_list_library_revision(library);
            let memo_key = format!("{library_revision}|{fingerprint}|{resolved_today}");
            state
                .smart_list_summaries()
                .get_or_try_build(&memo_key, build)
                .await?
        }
        // A backend without listing mtimes cannot prove the definitions are
        // unchanged. Build fresh instead of risking stale externally edited lists.
        None => build().await?,
    };

    let ctx = EvalContext::new(library, chrono::Utc::now(), resolved_today);
    let wanted = entity.and_then(|id| library.record_by_id(id));
    Ok(cached
        .rows
        .iter()
        .map(|row| {
            let mut summary = row.summary.clone();
            // `now()` is an instant, not content. Keep those uncommon counts
            // live while still reusing the parsed filter tree.
            if row.uses_now {
                summary.item_count = library
                    .records
                    .iter()
                    .filter(|record| smart_lists::record_matches(&row.filters, record, &ctx))
                    .count();
            }
            summary.contains = entity.map(|_| {
                wanted.is_some_and(|record| smart_lists::record_matches(&row.filters, record, &ctx))
            });
            summary
        })
        .collect())
}

fn filters_use_now(node: &FilterNode) -> bool {
    match node {
        FilterNode::Group { children, .. } => children.iter().any(filters_use_now),
        FilterNode::Expr(FilterAtom {
            kind:
                AtomKind::Compare {
                    value:
                        CompareValue::Date(DateExpr {
                            base: DateBase::Now,
                            ..
                        }),
                    ..
                },
            ..
        }) => true,
        FilterNode::Expr(_) | FilterNode::Opaque(_) => false,
    }
}

/// Smart-list filters can observe `file.mtime`, which is intentionally absent
/// from `Library::content_revision` (touching a file does not change its content).
/// Add resident entity mtimes to this feature-local revision so such touches
/// invalidate summary counts without invalidating unrelated analytics memos.
fn smart_list_library_revision(library: &Library) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    library.content_revision.hash(&mut hasher);
    for record in &library.records {
        record.summary.id.hash(&mut hasher);
        record.file_modified_unix_nanos.hash(&mut hasher);
    }
    hasher.finish()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn eval_context<'a>(library: &'a Library, today: Option<&str>) -> EvalContext<'a> {
    // `now()` is a true instant (timezone-independent), so it keeps the real UTC
    // clock; only `today()` is a calendar day and rides on the client's local date.
    EvalContext::new(library, chrono::Utc::now(), resolve_today(today))
}

/// The client's local calendar day for `today()` date criteria, or the host's
/// local day as a fallback — correct for the in-process desktop/iOS runtimes,
/// where the core shares the user's clock; the web client always sends its own.
pub(crate) fn resolve_today(client_today: Option<&str>) -> chrono::NaiveDate {
    client_today
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .unwrap_or_else(|| chrono::Local::now().date_naive())
}

fn parse_list_raw(path: &str, raw: &str) -> Result<SmartList, ApiError> {
    parse_smart_list(raw)
        .map_err(|error| ApiError::bad_request(&format!("Cannot parse {path}: {error}")))
}

/// Validates a requested type scope and resolves it to the type's folder.
fn resolve_scope(config: &KizunaConfig, scope: Option<&str>) -> Result<Option<String>, ApiError> {
    match scope {
        None => Ok(None),
        Some(type_id) => type_scope_folder(config, type_id)
            .map(Some)
            .ok_or_else(|| ApiError::bad_request("Unknown entity type for smart list scope")),
    }
}

/// The default cards-image property for a type: its first `image`/`imageList`
/// schema field, as a `note.<field>` reference.
fn cover_property(config: &KizunaConfig, type_id: &str) -> Option<String> {
    config
        .type_config(type_id)?
        .fields
        .iter()
        .find(|field| matches!(field.field_type, FieldType::Image | FieldType::ImageList))
        .map(|field| format!("note.{}", field.field))
}

fn paginate(
    records: Vec<&crate::types::EntityRecord>,
    page: f64,
    page_size: f64,
) -> EntityListResponse {
    let page_size = clamp_number(page_size, 1, crate::entities::MAX_PAGE_SIZE);
    let requested_page = clamp_number(page, 1, i64::MAX);
    let total = records.len();
    let total_pages = std::cmp::max(1, ((total as f64) / (page_size as f64)).ceil() as i64);
    let page = requested_page.min(total_pages);
    let start = ((page - 1) * page_size) as usize;
    let items = records
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .map(|record| record.summary.clone())
        .collect();
    EntityListResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    }
}

fn detail_from_list(
    path: &str,
    list: &SmartList,
    raw: &str,
    config: &KizunaConfig,
) -> SmartListDetail {
    let scope = scope_from_filters(&list.filters, config);
    let id = SMART_LIST.id(path);
    SmartListDetail {
        show_on_home: smart_lists::shows_on_home(list),
        name: id.clone(),
        id,
        path: path.to_string(),
        scope: scope.as_ref().map(|(type_id, _)| type_id.clone()),
        filters: root_group(&list.filters, scope.map(|(_, index)| index)),
        views: list.views.iter().map(view_to_contract).collect(),
        warnings: list.warnings.clone(),
        revision: file_revision(raw),
    }
}

// ---------------------------------------------------------------------------
// Core filter tree ⇄ contract criteria model
// ---------------------------------------------------------------------------

fn conjunction_to_contract(conjunction: Conjunction) -> SmartFilterConjunction {
    match conjunction {
        Conjunction::All => SmartFilterConjunction::All,
        Conjunction::Any => SmartFilterConjunction::Any,
        Conjunction::NoneOf => SmartFilterConjunction::NoneOf,
    }
}

fn conjunction_to_core(conjunction: SmartFilterConjunction) -> Conjunction {
    match conjunction {
        SmartFilterConjunction::All => Conjunction::All,
        SmartFilterConjunction::Any => Conjunction::Any,
        SmartFilterConjunction::NoneOf => Conjunction::NoneOf,
    }
}

/// Maps the root filter node into the contract group, hiding the scope atom
/// at `skip_index` (it rides as `scope` on the detail instead).
fn root_group(node: &FilterNode, skip_index: Option<usize>) -> SmartFilterGroup {
    match node {
        FilterNode::Group {
            conjunction,
            children,
        } => {
            let mut group = SmartFilterGroup {
                conjunction: conjunction_to_contract(*conjunction),
                rules: Vec::new(),
                groups: Vec::new(),
            };
            for (index, child) in children.iter().enumerate() {
                if Some(index) == skip_index {
                    continue;
                }
                match child {
                    FilterNode::Group {
                        conjunction,
                        children,
                    } => group.groups.push(SmartFilterSubgroup {
                        conjunction: conjunction_to_contract(*conjunction),
                        rules: children.iter().map(node_to_rule).collect(),
                    }),
                    _ => group.rules.push(node_to_rule(child)),
                }
            }
            group
        }
        // A single-expression `filters:` value reads as an and-group of one.
        _ => SmartFilterGroup {
            conjunction: SmartFilterConjunction::All,
            rules: vec![node_to_rule(node)],
            groups: Vec::new(),
        },
    }
}

/// Maps one filter node to a contract rule. Groups below the supported depth
/// and opaque constructs become `unsupported` rules carrying their raw YAML,
/// which [`rule_to_node`] parses back verbatim — the round-trip that lets an
/// editor save without destroying what it can't model.
fn node_to_rule(node: &FilterNode) -> SmartFilterRule {
    match node {
        FilterNode::Expr(atom) => atom_to_rule(atom),
        _ => opaque_rule(&filter_node_to_yaml(node)),
    }
}

fn opaque_rule(value: &serde_yaml::Value) -> SmartFilterRule {
    SmartFilterRule {
        kind: SmartFilterRuleKind::Unsupported,
        raw: Some(serde_yaml::to_string(value).unwrap_or_default()),
        ..Default::default()
    }
}

fn field_to_contract(field: &FieldRef) -> String {
    match field {
        FieldRef::Note(name) => name.clone(),
        FieldRef::FileName => "file.name".to_string(),
        FieldRef::FileMtime => "file.mtime".to_string(),
    }
}

fn field_to_core(field: Option<&str>) -> Result<FieldRef, ApiError> {
    let field = field
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .ok_or_else(|| ApiError::bad_request("Smart list rule is missing its field"))?;
    Ok(match field {
        "file.name" => FieldRef::FileName,
        "file.mtime" => FieldRef::FileMtime,
        other => FieldRef::Note(other.strip_prefix("note.").unwrap_or(other).to_string()),
    })
}

/// The relation field a `linksTo` rule is scoped to, or `None` for the
/// file-wide form. Only a frontmatter key can hold links, so the two `file.*`
/// properties fall back to file-wide rather than erroring.
fn link_scope_to_core(field: Option<&str>) -> Option<String> {
    field
        .map(str::trim)
        .filter(|field| !field.is_empty() && *field != "file.name" && *field != "file.mtime")
        .map(|field| field.strip_prefix("note.").unwrap_or(field).to_string())
}

fn op_to_contract(op: CompareOp) -> SmartCompareOp {
    match op {
        CompareOp::Eq => SmartCompareOp::Eq,
        CompareOp::Ne => SmartCompareOp::Ne,
        CompareOp::Gt => SmartCompareOp::Gt,
        CompareOp::Gte => SmartCompareOp::Gte,
        CompareOp::Lt => SmartCompareOp::Lt,
        CompareOp::Lte => SmartCompareOp::Lte,
    }
}

fn op_to_core(op: SmartCompareOp) -> CompareOp {
    match op {
        SmartCompareOp::Eq => CompareOp::Eq,
        SmartCompareOp::Ne => CompareOp::Ne,
        SmartCompareOp::Gt => CompareOp::Gt,
        SmartCompareOp::Gte => CompareOp::Gte,
        SmartCompareOp::Lt => CompareOp::Lt,
        SmartCompareOp::Lte => CompareOp::Lte,
    }
}

fn atom_to_rule(atom: &FilterAtom) -> SmartFilterRule {
    let base = SmartFilterRule {
        negated: atom.negated,
        ..Default::default()
    };
    match &atom.kind {
        AtomKind::InFolder { folder } => SmartFilterRule {
            kind: SmartFilterRuleKind::InFolder,
            values: vec![folder.clone()],
            ..base
        },
        AtomKind::HasTag { tags } => SmartFilterRule {
            kind: SmartFilterRuleKind::HasTag,
            values: tags.clone(),
            ..base
        },
        AtomKind::HasLink { field, target } => SmartFilterRule {
            kind: SmartFilterRuleKind::LinksTo,
            field: field.clone(),
            values: vec![target.clone()],
            ..base
        },
        AtomKind::Contains {
            field,
            mode,
            values,
        } => SmartFilterRule {
            kind: SmartFilterRuleKind::Contains,
            field: Some(field_to_contract(field)),
            mode: Some(match mode {
                ContainsMode::Any => SmartContainsMode::Any,
                ContainsMode::All => SmartContainsMode::All,
            }),
            values: values.clone(),
            ..base
        },
        AtomKind::StartsWith { field, value } => SmartFilterRule {
            kind: SmartFilterRuleKind::StartsWith,
            field: Some(field_to_contract(field)),
            values: vec![value.clone()],
            ..base
        },
        AtomKind::EndsWith { field, value } => SmartFilterRule {
            kind: SmartFilterRuleKind::EndsWith,
            field: Some(field_to_contract(field)),
            values: vec![value.clone()],
            ..base
        },
        AtomKind::IsEmpty { field } => SmartFilterRule {
            kind: SmartFilterRuleKind::IsEmpty,
            field: Some(field_to_contract(field)),
            ..base
        },
        AtomKind::Compare { field, op, value } => {
            let rule = SmartFilterRule {
                kind: SmartFilterRuleKind::Compare,
                field: Some(field_to_contract(field)),
                op: Some(op_to_contract(*op)),
                ..base
            };
            match value {
                CompareValue::String(value) => SmartFilterRule {
                    value: Some(value.clone()),
                    ..rule
                },
                CompareValue::Number(value) => SmartFilterRule {
                    number: Some(*value),
                    ..rule
                },
                CompareValue::Bool(value) => SmartFilterRule {
                    boolean: Some(*value),
                    ..rule
                },
                CompareValue::Date(date) => match date_to_contract(date) {
                    Some(rule_with_date) => SmartFilterRule {
                        date: rule_with_date.0,
                        relative: rule_with_date.1,
                        ..rule
                    },
                    // A date shape the editor can't model (multiple offsets,
                    // absolute-with-offset, sub-day units) stays raw.
                    None => opaque_rule(&serde_yaml::Value::String(atom.raw.clone())),
                },
            }
        }
    }
}

type ContractDate = (Option<String>, Option<crate::contract::SmartRelativeDate>);

/// A date expression in the editor's vocabulary: an absolute ISO day, or a
/// single whole-unit offset from today/now. Anything richer returns `None`.
fn date_to_contract(date: &DateExpr) -> Option<ContractDate> {
    match (date.base, date.offsets.as_slice()) {
        (DateBase::Absolute(day), []) => Some((Some(day.to_string()), None)),
        (DateBase::Today | DateBase::Now, []) => Some((
            None,
            Some(crate::contract::SmartRelativeDate {
                amount: 0,
                unit: SmartDurationUnit::Days,
                future: false,
            }),
        )),
        (DateBase::Today | DateBase::Now, [offset]) => {
            let duration = offset.duration;
            let (amount, unit) = match duration {
                DurationSpec {
                    years: amount,
                    months: 0,
                    weeks: 0,
                    days: 0,
                    hours: 0,
                    minutes: 0,
                    seconds: 0,
                } if amount > 0 => (amount, SmartDurationUnit::Years),
                DurationSpec {
                    years: 0,
                    months: amount,
                    weeks: 0,
                    days: 0,
                    hours: 0,
                    minutes: 0,
                    seconds: 0,
                } if amount > 0 => (amount, SmartDurationUnit::Months),
                DurationSpec {
                    years: 0,
                    months: 0,
                    weeks: amount,
                    days: 0,
                    hours: 0,
                    minutes: 0,
                    seconds: 0,
                } if amount > 0 => (amount, SmartDurationUnit::Weeks),
                DurationSpec {
                    years: 0,
                    months: 0,
                    weeks: 0,
                    days: amount,
                    hours: 0,
                    minutes: 0,
                    seconds: 0,
                } if amount > 0 => (amount, SmartDurationUnit::Days),
                _ => return None,
            };
            Some((
                None,
                Some(crate::contract::SmartRelativeDate {
                    amount,
                    unit,
                    future: !offset.negative,
                }),
            ))
        }
        _ => None,
    }
}

fn rule_to_node(rule: &SmartFilterRule) -> Result<FilterNode, ApiError> {
    let first_value = || {
        rule.values
            .first()
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let kind = match rule.kind {
        SmartFilterRuleKind::Unsupported => {
            let raw = rule
                .raw
                .as_deref()
                .ok_or_else(|| ApiError::bad_request("Unsupported rule is missing its raw form"))?;
            let value = serde_yaml::from_str::<serde_yaml::Value>(raw)
                .map_err(|_| ApiError::bad_request("Unsupported rule has invalid raw YAML"))?;
            return Ok(FilterNode::Opaque(value));
        }
        SmartFilterRuleKind::InFolder => AtomKind::InFolder {
            folder: first_value()
                .ok_or_else(|| ApiError::bad_request("Folder rule needs a folder"))?,
        },
        SmartFilterRuleKind::HasTag => {
            if rule.values.is_empty() {
                return Err(ApiError::bad_request("Tag rule needs at least one tag"));
            }
            AtomKind::HasTag {
                tags: rule.values.clone(),
            }
        }
        SmartFilterRuleKind::LinksTo => AtomKind::HasLink {
            field: link_scope_to_core(rule.field.as_deref()),
            target: first_value()
                .ok_or_else(|| ApiError::bad_request("Link rule needs a target"))?,
        },
        SmartFilterRuleKind::Contains => {
            if rule.values.is_empty() {
                return Err(ApiError::bad_request("Contains rule needs values"));
            }
            AtomKind::Contains {
                field: field_to_core(rule.field.as_deref())?,
                mode: match rule.mode.unwrap_or_default() {
                    SmartContainsMode::Any => ContainsMode::Any,
                    SmartContainsMode::All => ContainsMode::All,
                },
                values: rule.values.clone(),
            }
        }
        SmartFilterRuleKind::StartsWith => AtomKind::StartsWith {
            field: field_to_core(rule.field.as_deref())?,
            value: first_value()
                .ok_or_else(|| ApiError::bad_request("Starts-with rule needs a value"))?,
        },
        SmartFilterRuleKind::EndsWith => AtomKind::EndsWith {
            field: field_to_core(rule.field.as_deref())?,
            value: first_value()
                .ok_or_else(|| ApiError::bad_request("Ends-with rule needs a value"))?,
        },
        SmartFilterRuleKind::IsEmpty => AtomKind::IsEmpty {
            field: field_to_core(rule.field.as_deref())?,
        },
        SmartFilterRuleKind::Compare => {
            let field = field_to_core(rule.field.as_deref())?;
            let mut op = op_to_core(
                rule.op
                    .ok_or_else(|| ApiError::bad_request("Compare rule needs an operator"))?,
            );
            // Negation on a comparison is expressed by the operator itself.
            if rule.negated {
                op = match op {
                    CompareOp::Eq => CompareOp::Ne,
                    CompareOp::Ne => CompareOp::Eq,
                    CompareOp::Gt => CompareOp::Lte,
                    CompareOp::Gte => CompareOp::Lt,
                    CompareOp::Lt => CompareOp::Gte,
                    CompareOp::Lte => CompareOp::Gt,
                };
            }
            let value = compare_value_to_core(rule, &field)?;
            return Ok(FilterNode::Expr(FilterAtom::new(
                AtomKind::Compare { field, op, value },
                false,
            )));
        }
    };
    Ok(FilterNode::Expr(FilterAtom::new(kind, rule.negated)))
}

fn compare_value_to_core(
    rule: &SmartFilterRule,
    field: &FieldRef,
) -> Result<CompareValue, ApiError> {
    if let Some(relative) = &rule.relative {
        let duration = match relative.unit {
            SmartDurationUnit::Days => DurationSpec {
                days: relative.amount,
                ..Default::default()
            },
            SmartDurationUnit::Weeks => DurationSpec {
                weeks: relative.amount,
                ..Default::default()
            },
            SmartDurationUnit::Months => DurationSpec {
                months: relative.amount,
                ..Default::default()
            },
            SmartDurationUnit::Years => DurationSpec {
                years: relative.amount,
                ..Default::default()
            },
        };
        // File timestamps compare against instants, note dates against days.
        let base = if *field == FieldRef::FileMtime {
            DateBase::Now
        } else {
            DateBase::Today
        };
        let offsets = if relative.amount == 0 {
            Vec::new()
        } else {
            vec![DateOffset {
                negative: !relative.future,
                duration,
            }]
        };
        return Ok(CompareValue::Date(DateExpr { base, offsets }));
    }
    if let Some(date) = rule.date.as_deref() {
        let (year, month, day) = crate::dates::exact_date_parts(Some(date))
            .ok_or_else(|| ApiError::bad_request("Compare rule has an invalid date"))?;
        let day = chrono::NaiveDate::from_ymd_opt(year, month, day)
            .ok_or_else(|| ApiError::bad_request("Compare rule has an invalid date"))?;
        return Ok(CompareValue::Date(DateExpr {
            base: DateBase::Absolute(day),
            offsets: Vec::new(),
        }));
    }
    if let Some(number) = rule.number {
        return Ok(CompareValue::Number(number));
    }
    if let Some(boolean) = rule.boolean {
        return Ok(CompareValue::Bool(boolean));
    }
    if let Some(value) = &rule.value {
        return Ok(CompareValue::String(value.clone()));
    }
    Err(ApiError::bad_request("Compare rule needs a value"))
}

/// Builds the core filter tree from the contract group plus the maintained
/// scope atom. With a scope and a non-`all` root conjunction, the scope wraps
/// the group in an outer `and` so it always constrains the result.
///
/// Also the bridge for home-section `criteria` (`api/handlers.rs`), which
/// share the contract group model and this evaluator.
pub(super) fn group_to_node(
    group: &SmartFilterGroup,
    scope_folder: Option<&str>,
) -> Result<FilterNode, ApiError> {
    let mut children: Vec<FilterNode> = Vec::new();
    for rule in &group.rules {
        children.push(rule_to_node(rule)?);
    }
    for subgroup in &group.groups {
        let mut sub_children = Vec::new();
        for rule in &subgroup.rules {
            sub_children.push(rule_to_node(rule)?);
        }
        children.push(FilterNode::Group {
            conjunction: conjunction_to_core(subgroup.conjunction),
            children: sub_children,
        });
    }
    let conjunction = conjunction_to_core(group.conjunction);
    let Some(folder) = scope_folder else {
        return Ok(FilterNode::Group {
            conjunction,
            children,
        });
    };
    let scope_atom = FilterNode::Expr(FilterAtom::new(
        AtomKind::InFolder {
            folder: folder.to_string(),
        },
        false,
    ));
    if conjunction == Conjunction::All {
        children.insert(0, scope_atom);
        return Ok(FilterNode::Group {
            conjunction,
            children,
        });
    }
    Ok(FilterNode::Group {
        conjunction: Conjunction::All,
        children: vec![
            scope_atom,
            FilterNode::Group {
                conjunction,
                children,
            },
        ],
    })
}

// ---------------------------------------------------------------------------
// Views ⇄ contract
// ---------------------------------------------------------------------------

fn direction_to_contract(direction: CoreDirection) -> ContractDirection {
    match direction {
        CoreDirection::Asc => ContractDirection::Asc,
        CoreDirection::Desc => ContractDirection::Desc,
    }
}

fn direction_to_core(direction: ContractDirection) -> CoreDirection {
    match direction {
        ContractDirection::Asc => CoreDirection::Asc,
        ContractDirection::Desc => CoreDirection::Desc,
    }
}

fn view_to_contract(view: &SmartView) -> SmartListView {
    SmartListView {
        name: view.name.clone(),
        layout: match view.layout {
            ViewLayout::List => SmartViewLayout::List,
            ViewLayout::Grid => SmartViewLayout::Grid,
        },
        filters: view
            .filters
            .as_ref()
            .map(|filters| root_group(filters, None)),
        sort: view
            .sort
            .iter()
            .map(|sort| SmartSortSpec {
                property: print_sort_property(&sort.property),
                direction: direction_to_contract(sort.direction),
            })
            .collect(),
        limit: view
            .limit
            .map(|limit| limit.min(u64::from(u32::MAX)) as u32),
        image: view.image.clone(),
    }
}

fn contract_sort_to_core(sort: &SmartSortSpec) -> ViewSort {
    ViewSort {
        property: smart_lists::parse_sort_property(&sort.property),
        direction: direction_to_core(sort.direction),
    }
}

fn contract_view_to_spec(
    view: &SmartListView,
    default_image: Option<&str>,
) -> Result<ViewSpec, ApiError> {
    let layout = match view.layout {
        SmartViewLayout::List => ViewLayout::List,
        SmartViewLayout::Grid => ViewLayout::Grid,
    };
    let filters = view
        .filters
        .as_ref()
        .map(|filters| group_to_node(filters, None))
        .transpose()?
        .filter(|filters| !filters.is_empty_group());
    let image = match (layout, &view.image) {
        (ViewLayout::Grid, Some(image)) => Some(image.clone()),
        (ViewLayout::Grid, None) => default_image.map(str::to_string),
        _ => None,
    };
    Ok(ViewSpec {
        layout,
        name: view.name.clone(),
        filters,
        sort: view.sort.iter().map(contract_sort_to_core).collect(),
        limit: view.limit.map(u64::from),
        image,
    })
}

#[cfg(test)]
mod mapping_tests {
    //! Round-trip tests for the hand-written contract ⇄ core criteria mapping
    //! above. An asymmetry between the two directions silently changes what a
    //! saved filter *means*, so every rule kind and value shape the editor can
    //! produce must survive contract → core → contract unchanged — and a
    //! hand-authored core tree must survive core → contract → core.

    use super::*;
    use crate::contract::SmartRelativeDate;

    /// Contract structs don't derive `PartialEq`; their serialized form is the
    /// wire truth anyway, so compare that.
    fn contract_json<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value).expect("contract value serializes")
    }

    fn compare(field: &str, op: SmartCompareOp) -> SmartFilterRule {
        SmartFilterRule {
            kind: SmartFilterRuleKind::Compare,
            field: Some(field.to_string()),
            op: Some(op),
            ..Default::default()
        }
    }

    fn relative(amount: u32, unit: SmartDurationUnit, future: bool) -> Option<SmartRelativeDate> {
        Some(SmartRelativeDate {
            amount,
            unit,
            future,
        })
    }

    fn all_group(rules: Vec<SmartFilterRule>) -> SmartFilterGroup {
        SmartFilterGroup {
            conjunction: SmartFilterConjunction::All,
            rules,
            groups: Vec::new(),
        }
    }

    /// `ApiError` has no `Debug` impl, so unwrap through its message.
    fn to_core(group: &SmartFilterGroup, scope: Option<&str>) -> FilterNode {
        group_to_node(group, scope)
            .unwrap_or_else(|error| panic!("maps to core: {}", error.message()))
    }

    fn round_trip(group: &SmartFilterGroup) -> SmartFilterGroup {
        root_group(&to_core(group, None), None)
    }

    #[test]
    fn every_rule_kind_round_trips_from_the_contract() {
        let group = SmartFilterGroup {
            conjunction: SmartFilterConjunction::All,
            rules: vec![
                SmartFilterRule {
                    kind: SmartFilterRuleKind::InFolder,
                    values: vec!["Media/Anime".into()],
                    ..Default::default()
                },
                SmartFilterRule {
                    kind: SmartFilterRuleKind::HasTag,
                    values: vec!["favorite".into(), "seasonal/2026".into()],
                    negated: true,
                    ..Default::default()
                },
                SmartFilterRule {
                    kind: SmartFilterRuleKind::LinksTo,
                    values: vec!["Steins;Gate".into()],
                    ..Default::default()
                },
                SmartFilterRule {
                    kind: SmartFilterRuleKind::Contains,
                    field: Some("genres".into()),
                    mode: Some(SmartContainsMode::Any),
                    values: vec!["SF".into(), "Space".into()],
                    ..Default::default()
                },
                SmartFilterRule {
                    kind: SmartFilterRuleKind::Contains,
                    field: Some("file.name".into()),
                    mode: Some(SmartContainsMode::All),
                    values: vec!["OVA".into()],
                    ..Default::default()
                },
                SmartFilterRule {
                    kind: SmartFilterRuleKind::StartsWith,
                    field: Some("title".into()),
                    values: vec!["Star".into()],
                    ..Default::default()
                },
                SmartFilterRule {
                    kind: SmartFilterRuleKind::EndsWith,
                    field: Some("title".into()),
                    values: vec!["Voyager".into()],
                    negated: true,
                    ..Default::default()
                },
                SmartFilterRule {
                    kind: SmartFilterRuleKind::IsEmpty,
                    field: Some("rating".into()),
                    ..Default::default()
                },
                // Every operator, and every compare value shape: string,
                // number, bool, absolute date, and each relative-date unit in
                // both directions (past and future), including zero ("today")
                // and the `file.mtime` instant base.
                SmartFilterRule {
                    value: Some("Watching".into()),
                    ..compare("status", SmartCompareOp::Eq)
                },
                SmartFilterRule {
                    number: Some(7.5),
                    ..compare("rating", SmartCompareOp::Gt)
                },
                SmartFilterRule {
                    boolean: Some(true),
                    ..compare("favorite", SmartCompareOp::Eq)
                },
                SmartFilterRule {
                    date: Some("2026-04-03".into()),
                    ..compare("complete_date", SmartCompareOp::Lte)
                },
                SmartFilterRule {
                    relative: relative(7, SmartDurationUnit::Days, true),
                    ..compare("complete_date", SmartCompareOp::Gte)
                },
                SmartFilterRule {
                    relative: relative(2, SmartDurationUnit::Weeks, false),
                    ..compare("complete_date", SmartCompareOp::Lt)
                },
                SmartFilterRule {
                    relative: relative(3, SmartDurationUnit::Months, false),
                    ..compare("start_date", SmartCompareOp::Gte)
                },
                SmartFilterRule {
                    relative: relative(1, SmartDurationUnit::Years, true),
                    ..compare("start_date", SmartCompareOp::Lt)
                },
                SmartFilterRule {
                    relative: relative(0, SmartDurationUnit::Days, false),
                    ..compare("complete_date", SmartCompareOp::Gte)
                },
                SmartFilterRule {
                    relative: relative(30, SmartDurationUnit::Days, false),
                    ..compare("file.mtime", SmartCompareOp::Gte)
                },
                SmartFilterRule {
                    value: Some("Steins;Gate 0 (Anime)".into()),
                    ..compare("file.name", SmartCompareOp::Ne)
                },
            ],
            groups: vec![
                SmartFilterSubgroup {
                    conjunction: SmartFilterConjunction::Any,
                    rules: vec![SmartFilterRule {
                        value: Some("Paused".into()),
                        ..compare("status", SmartCompareOp::Eq)
                    }],
                },
                SmartFilterSubgroup {
                    conjunction: SmartFilterConjunction::NoneOf,
                    rules: vec![SmartFilterRule {
                        kind: SmartFilterRuleKind::HasTag,
                        values: vec!["dropped".into()],
                        ..Default::default()
                    }],
                },
            ],
        };
        assert_eq!(contract_json(&group), contract_json(&round_trip(&group)));
    }

    #[test]
    fn a_core_tree_round_trips_through_the_contract() {
        let node = FilterNode::Group {
            conjunction: Conjunction::All,
            children: vec![
                FilterNode::Expr(FilterAtom::new(
                    AtomKind::InFolder {
                        folder: "Media/Games".into(),
                    },
                    false,
                )),
                FilterNode::Expr(FilterAtom::new(
                    AtomKind::HasTag {
                        tags: vec!["backlog".into()],
                    },
                    true,
                )),
                FilterNode::Expr(FilterAtom::new(
                    AtomKind::Compare {
                        field: FieldRef::Note("rating".into()),
                        op: CompareOp::Gte,
                        value: CompareValue::Number(8.0),
                    },
                    false,
                )),
                FilterNode::Expr(FilterAtom::new(
                    AtomKind::Compare {
                        field: FieldRef::FileMtime,
                        op: CompareOp::Gte,
                        value: CompareValue::Date(DateExpr {
                            base: DateBase::Now,
                            offsets: vec![DateOffset {
                                negative: true,
                                duration: DurationSpec {
                                    days: 90,
                                    ..Default::default()
                                },
                            }],
                        }),
                    },
                    false,
                )),
                FilterNode::Expr(FilterAtom::new(
                    AtomKind::Compare {
                        field: FieldRef::Note("complete_date".into()),
                        op: CompareOp::Lte,
                        value: CompareValue::Date(DateExpr {
                            base: DateBase::Absolute(
                                chrono::NaiveDate::from_ymd_opt(2026, 4, 3).expect("valid date"),
                            ),
                            offsets: Vec::new(),
                        }),
                    },
                    false,
                )),
                // An opaque construct must survive as `unsupported` raw YAML.
                FilterNode::Opaque(serde_yaml::Value::String("custom.magic()".into())),
                FilterNode::Group {
                    conjunction: Conjunction::Any,
                    children: vec![
                        FilterNode::Expr(FilterAtom::new(
                            AtomKind::IsEmpty {
                                field: FieldRef::Note("status".into()),
                            },
                            false,
                        )),
                        FilterNode::Expr(FilterAtom::new(
                            AtomKind::EndsWith {
                                field: FieldRef::FileName,
                                value: "OVA".into(),
                            },
                            false,
                        )),
                    ],
                },
            ],
        };
        let back = to_core(&root_group(&node, None), None);
        assert_eq!(node, back);
    }

    #[test]
    fn a_negated_compare_normalizes_to_the_flipped_operator() {
        // The contract can say "negated eq"; the core expresses that as `!=`.
        // The round trip therefore normalizes rather than reproduces — assert
        // the exact normalized form, and that it is a fixed point from there.
        let group = all_group(vec![SmartFilterRule {
            negated: true,
            value: Some("Dropped".into()),
            ..compare("status", SmartCompareOp::Eq)
        }]);
        let normalized = round_trip(&group);
        let rule = contract_json(&normalized.rules[0]);
        assert_eq!(rule["op"], "ne");
        assert_eq!(rule["negated"], false);
        assert_eq!(rule["value"], "Dropped");
        assert_eq!(
            contract_json(&normalized),
            contract_json(&round_trip(&normalized))
        );
    }

    #[test]
    fn a_note_prefixed_field_normalizes_to_the_bare_key() {
        let group = all_group(vec![SmartFilterRule {
            value: Some("x".into()),
            ..compare("note.rating", SmartCompareOp::Eq)
        }]);
        let rule = contract_json(&round_trip(&group).rules[0]);
        assert_eq!(rule["field"], "rating");
    }

    #[test]
    fn the_scope_atom_is_prepended_and_hidden_for_an_all_group() {
        let group = all_group(vec![SmartFilterRule {
            value: Some("Watching".into()),
            ..compare("status", SmartCompareOp::Eq)
        }]);
        let node = to_core(&group, Some("Media/Anime"));
        let FilterNode::Group { children, .. } = &node else {
            panic!("root must be a group");
        };
        assert_eq!(
            children[0],
            FilterNode::Expr(FilterAtom::new(
                AtomKind::InFolder {
                    folder: "Media/Anime".into()
                },
                false,
            ))
        );
        // Reading back with the scope hidden reproduces the original group.
        assert_eq!(
            contract_json(&group),
            contract_json(&root_group(&node, Some(0)))
        );
    }

    #[test]
    fn a_non_all_scope_wraps_once_and_is_stable_after_the_first_round_trip() {
        // An `any` group with a scope gets an outer `and` wrapper so the scope
        // always constrains. The first read-back moves the rules into one
        // subgroup of an `all` root — a deliberate reshaping — and from then
        // on the shape must be a fixed point, not wrap again on every save.
        let group = SmartFilterGroup {
            conjunction: SmartFilterConjunction::Any,
            rules: vec![
                SmartFilterRule {
                    value: Some("Watching".into()),
                    ..compare("status", SmartCompareOp::Eq)
                },
                SmartFilterRule {
                    value: Some("Paused".into()),
                    ..compare("status", SmartCompareOp::Eq)
                },
            ],
            groups: Vec::new(),
        };
        let node = to_core(&group, Some("Media/Anime"));
        let first = root_group(&node, Some(0));
        assert_eq!(contract_json(&first.conjunction), "all");
        assert_eq!(first.rules.len(), 0);
        assert_eq!(first.groups.len(), 1);
        assert_eq!(contract_json(&first.groups[0].conjunction), "any");
        assert_eq!(first.groups[0].rules.len(), 2);

        let again = root_group(&to_core(&first, Some("Media/Anime")), Some(0));
        assert_eq!(contract_json(&first), contract_json(&again));
    }
}
