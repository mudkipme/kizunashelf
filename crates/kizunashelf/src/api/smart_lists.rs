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
use super::mutations::{check_revision, move_to_trash, sanitize_basename, write_entity_raw};
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
    let path = smart_list_path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let raw = read_smart_list_raw(vfs.as_ref(), &path).await?;
    let list = parse_list_raw(&path, &raw)?;
    Ok(Json(detail_from_list(&path, &list, &raw, &library.config)))
}

pub(crate) async fn create_smart_list(
    State(state): State<AppState>,
    Json(request): Json<CreateSmartListRequest>,
) -> ApiResult<SmartListDetail> {
    let library = require_content_writes(&state).await?;
    let basename = sanitize_basename(&request.name)
        .map_err(|error| ApiError::bad_request(&error.to_string()))?;
    let scope_folder = resolve_scope(&library.config, request.scope.as_deref())?;
    let image = request
        .scope
        .as_deref()
        .and_then(|type_id| cover_property(&library.config, type_id));
    let vfs = state.vault_vfs(&library.config.vault_root);
    let path = format!("{LISTS_DIR}/{basename}.{SMART_LIST_EXTENSION}");
    if vfs
        .exists(&path)
        .await
        .map_err(|err| anyhow::anyhow!("failed to check smart list path: {err}"))?
    {
        return Err(ApiError::conflict("Smart list already exists"));
    }
    let doc = default_smart_list_doc(scope_folder.as_deref(), image.as_deref());
    let raw = render_smart_list(&doc);
    write_entity_raw(vfs.as_ref(), &path, &raw).await?;
    let list = parse_list_raw(&path, &raw)?;
    Ok(Json(detail_from_list(&path, &list, &raw, &library.config)))
}

pub(crate) async fn update_smart_list(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<SmartListPath>,
    Json(request): Json<UpdateSmartListRequest>,
) -> ApiResult<SmartListDetail> {
    let library = require_content_writes(&state).await?;
    let source_path = smart_list_path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);

    let raw = read_smart_list_raw(vfs.as_ref(), &source_path).await?;
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

    let target_path = match &request.rename_to {
        Some(rename_to) => {
            let basename = sanitize_basename(rename_to)
                .map_err(|error| ApiError::bad_request(&error.to_string()))?;
            let target = format!("{LISTS_DIR}/{basename}.{SMART_LIST_EXTENSION}");
            if target != source_path
                && vfs
                    .exists(&target)
                    .await
                    .map_err(|err| anyhow::anyhow!("failed to check smart list path: {err}"))?
            {
                return Err(ApiError::conflict("Target smart list already exists"));
            }
            target
        }
        None => source_path.clone(),
    };

    let new_raw = render_smart_list(&doc);
    write_entity_raw(vfs.as_ref(), &target_path, &new_raw).await?;
    if target_path != source_path {
        vfs.remove_file(&source_path).await.map_err(|err| {
            anyhow::anyhow!("failed to remove old smart list {source_path}: {err}")
        })?;
    }

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
    let path = smart_list_path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    if !vfs
        .exists(&path)
        .await
        .map_err(|err| anyhow::anyhow!("failed to check smart list path: {err}"))?
    {
        return Err(ApiError::not_found("Smart list not found"));
    }
    let backup_path = move_to_trash(vfs.as_ref(), &path).await?;
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
    let path = smart_list_path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let raw = read_smart_list_raw(vfs.as_ref(), &path).await?;
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
    let records =
        smart_lists::smart_list_records(&list, view, &ctx, query.title_language.as_deref());
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
        request.title_language.as_deref(),
    );
    Ok(Json(paginate(
        records,
        request.page.unwrap_or(1.0),
        request.page_size.unwrap_or(40.0),
    )))
}

// ---------------------------------------------------------------------------
// Lists-index integration
// ---------------------------------------------------------------------------

/// Vault-relative paths of every smart list — the `*.base` files directly
/// under [`LISTS_DIR`].
pub(crate) async fn smart_list_file_paths(vfs: &dyn Vfs) -> VfsResult<Vec<String>> {
    let entries = vfs.read_dir(LISTS_DIR).await?;
    let suffix = format!(".{SMART_LIST_EXTENSION}");
    Ok(entries
        .into_iter()
        .filter(|entry| entry.is_file && entry.name.ends_with(&suffix))
        .map(|entry| format!("{LISTS_DIR}/{}", entry.name))
        .collect())
}

/// The smart-list rows of the lists index: one [`ListSummary`] per parseable
/// `.base` file, with `itemCount` evaluated from the global filters (view
/// limits intentionally ignored — membership means "matches the criteria").
/// Unparseable files are skipped; the detail endpoint reports their error.
pub(crate) async fn smart_list_summaries(
    vfs: &dyn Vfs,
    library: &Library,
    entity: Option<&str>,
    today: Option<&str>,
) -> VfsResult<Vec<ListSummary>> {
    let paths = match smart_list_file_paths(vfs).await {
        Ok(paths) => paths,
        Err(error) if error.is_not_found() => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let files = vfs.read_files(&paths).await?;
    let ctx = eval_context(library, today);
    let wanted = entity.and_then(|id| library.record_by_id(id));
    Ok(files
        .into_iter()
        .filter_map(|(path, bytes)| String::from_utf8(bytes).ok().map(|raw| (path, raw)))
        .filter_map(|(path, raw)| {
            let list = parse_smart_list(&raw).ok()?;
            let item_count = library
                .records
                .iter()
                .filter(|record| smart_lists::record_matches(&list.filters, record, &ctx))
                .count();
            let contains = entity.map(|_| {
                wanted
                    .is_some_and(|record| smart_lists::record_matches(&list.filters, record, &ctx))
            });
            let id = smart_list_id(&path);
            Some(ListSummary {
                name: id.clone(),
                id,
                kind: ListKind::Smart,
                path,
                description: String::new(),
                item_count,
                section_count: 0,
                contains,
            })
        })
        .collect())
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

/// Vault-relative path of a smart list from its id, validated for containment.
fn smart_list_path(id: &str) -> Result<String, ApiError> {
    let basename =
        sanitize_basename(id).map_err(|_| ApiError::bad_request("Invalid smart list id"))?;
    Ok(format!("{LISTS_DIR}/{basename}.{SMART_LIST_EXTENSION}"))
}

/// The smart list id (basename without `.base`) from a vault-relative path.
fn smart_list_id(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(&format!(".{SMART_LIST_EXTENSION}"))
        .unwrap_or(name)
        .to_string()
}

async fn read_smart_list_raw(vfs: &dyn Vfs, path: &str) -> Result<String, ApiError> {
    match vfs.read_to_string(path).await {
        Ok(raw) => Ok(raw),
        Err(err) if err.is_not_found() => Err(ApiError::not_found("Smart list not found")),
        Err(err) => Err(anyhow::anyhow!("failed to read smart list {path}: {err}").into()),
    }
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
    let id = smart_list_id(path);
    SmartListDetail {
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
        AtomKind::HasLink { target } => SmartFilterRule {
            kind: SmartFilterRuleKind::LinksTo,
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
