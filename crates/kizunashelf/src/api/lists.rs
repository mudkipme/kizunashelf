//! Handlers for the Lists feature. Lists are plain Markdown files under
//! [`LISTS_DIR`](crate::lists::LISTS_DIR); see [`crate::lists`] for the
//! parsing/rendering. All vault I/O goes through the [`Vfs`], all writes are gated
//! by [`require_content_writes`], and full rewrites are revision-guarded.

use super::error::{ApiError, ApiResult};
use super::mutations::{check_revision, move_to_trash, sanitize_basename, write_entity_raw};
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{
    AddListItemRequest, CreateListRequest, DeleteListResponse, ListDetail, ListItem, ListSummary,
    ListsResponse, UpdateListRequest,
};
use crate::library::{file_revision, find_target, normalized_entity_basename_index};
use crate::lists::{
    basename_ambiguous, compose_document, entity_wikilink, item_target, parse_list, render_list,
    split_frontmatter, ParsedList, LISTS_DIR,
};
use crate::types::{EntityRecord, Library};
use crate::vfs::Vfs;
use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ListPath {
    id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListItemPath {
    id: String,
    entity_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ListsQuery {
    /// When set, each summary reports whether it contains this entity id
    /// (membership for the entity page's "manage lists").
    entity: Option<String>,
}

pub(crate) async fn get_lists(
    State(state): State<AppState>,
    Query(query): Query<ListsQuery>,
) -> ApiResult<ListsResponse> {
    let library = get_library(&state).await?;
    let vfs = state.vault_vfs(&library.config.vault_root);

    let entries = match vfs.read_dir(LISTS_DIR).await {
        Ok(entries) => entries,
        Err(err) if err.is_not_found() => Vec::new(),
        Err(err) => return Err(anyhow::anyhow!("failed to list lists: {err}").into()),
    };
    let paths: Vec<String> = entries
        .into_iter()
        .filter(|entry| entry.is_file && entry.name.ends_with(".md"))
        .map(|entry| format!("{LISTS_DIR}/{}", entry.name))
        .collect();

    let files = vfs
        .read_files(&paths)
        .await
        .map_err(|err| anyhow::anyhow!("failed to read lists: {err}"))?;

    // Only build the (O(n) records) resolution index when membership is requested.
    let index = query
        .entity
        .as_ref()
        .map(|_| normalized_entity_basename_index(&library.records));

    let mut items: Vec<ListSummary> = files
        .into_iter()
        .filter_map(|(path, bytes)| String::from_utf8(bytes).ok().map(|raw| (path, raw)))
        .map(|(path, raw)| {
            let (_, body) = split_frontmatter(&raw);
            let parsed = parse_list(&body);
            let id = list_id(&path);
            let contains = match (&query.entity, &index) {
                (Some(entity_id), Some(index)) => {
                    Some(list_contains_entity(&parsed.items, entity_id, index))
                }
                _ => None,
            };
            ListSummary {
                name: id.clone(),
                description: parsed.description.trim().to_string(),
                item_count: parsed.items.len(),
                ordered: parsed.ordered,
                id,
                path,
                contains,
            }
        })
        .collect();
    items.sort_by_key(|item| item.name.to_lowercase());

    Ok(Json(ListsResponse { items }))
}

pub(crate) async fn create_list(
    State(state): State<AppState>,
    Json(request): Json<CreateListRequest>,
) -> ApiResult<ListDetail> {
    let library = require_content_writes(&state).await?;
    let basename = sanitize_basename(&request.name)
        .map_err(|error| ApiError::bad_request(&error.to_string()))?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let path = format!("{LISTS_DIR}/{basename}.md");
    if vfs
        .exists(&path)
        .await
        .map_err(|err| anyhow::anyhow!("failed to check list path: {err}"))?
    {
        return Err(ApiError::conflict("List already exists"));
    }
    write_entity_raw(vfs.as_ref(), &path, "").await?;
    Ok(Json(detail_from_raw(&path, "", &library)))
}

pub(crate) async fn get_list(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<ListPath>,
) -> ApiResult<ListDetail> {
    let library = get_library(&state).await?;
    let path = list_path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let raw = read_list_raw(vfs.as_ref(), &path).await?;
    Ok(Json(detail_from_raw(&path, &raw, &library)))
}

pub(crate) async fn update_list(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<ListPath>,
    Json(request): Json<UpdateListRequest>,
) -> ApiResult<ListDetail> {
    let library = require_content_writes(&state).await?;
    let source_path = list_path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);

    let raw = read_list_raw(vfs.as_ref(), &source_path).await?;
    check_revision(&request.revision, &file_revision(&raw))?;

    // Preserve any frontmatter verbatim; rewrite the body from the request parts.
    let (frontmatter, _) = split_frontmatter(&raw);
    let items: Vec<String> = request.items.into_iter().map(|item| item.text).collect();
    let body = render_list(
        &request.description,
        &items,
        request.ordered,
        &request.trailing,
    );
    let new_raw = compose_document(&frontmatter, &body);

    let target_path = match &request.rename_to {
        Some(rename_to) => {
            let basename = sanitize_basename(rename_to)
                .map_err(|error| ApiError::bad_request(&error.to_string()))?;
            let target = format!("{LISTS_DIR}/{basename}.md");
            if target != source_path
                && vfs
                    .exists(&target)
                    .await
                    .map_err(|err| anyhow::anyhow!("failed to check list path: {err}"))?
            {
                return Err(ApiError::conflict("Target list already exists"));
            }
            target
        }
        None => source_path.clone(),
    };

    write_entity_raw(vfs.as_ref(), &target_path, &new_raw).await?;
    if target_path != source_path {
        vfs.remove_file(&source_path)
            .await
            .map_err(|err| anyhow::anyhow!("failed to remove old list {source_path}: {err}"))?;
    }

    Ok(Json(detail_from_raw(&target_path, &new_raw, &library)))
}

pub(crate) async fn delete_list(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<ListPath>,
) -> ApiResult<DeleteListResponse> {
    let library = require_content_writes(&state).await?;
    let path = list_path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    if !vfs
        .exists(&path)
        .await
        .map_err(|err| anyhow::anyhow!("failed to check list path: {err}"))?
    {
        return Err(ApiError::not_found("List not found"));
    }
    let backup_path = move_to_trash(vfs.as_ref(), &path).await?;
    Ok(Json(DeleteListResponse {
        deleted_id: path_param.id,
        backup_path,
    }))
}

pub(crate) async fn add_list_item(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<ListPath>,
    Json(request): Json<AddListItemRequest>,
) -> ApiResult<ListDetail> {
    let library = require_content_writes(&state).await?;
    let path = list_path(&path_param.id)?;
    let Some(record) = library.record_by_id(&request.entity_id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let vfs = state.vault_vfs(&library.config.vault_root);

    let raw = read_list_raw(vfs.as_ref(), &path).await?;
    let (frontmatter, body) = split_frontmatter(&raw);
    let mut parsed = parse_list(&body);

    let index = normalized_entity_basename_index(&library.records);
    // Skip if the entity is already on the list (idempotent add).
    let already_present = parsed.items.iter().any(|item| {
        item_target(item)
            .and_then(|target| find_target(&target, None, &index))
            .is_some_and(|found| found.summary.id == record.summary.id)
    });
    if !already_present {
        let ambiguous = basename_ambiguous(&index, &record.summary.basename);
        let wikilink = entity_wikilink(&record.summary.basename, &record.summary.path, ambiguous);
        parsed.items.push(wikilink);
        let new_body = render_list(
            &parsed.description,
            &parsed.items,
            parsed.ordered,
            &parsed.trailing,
        );
        let new_raw = compose_document(&frontmatter, &new_body);
        write_entity_raw(vfs.as_ref(), &path, &new_raw).await?;
        return Ok(Json(detail_from_parts(&path, parsed, &new_raw, &index)));
    }
    Ok(Json(detail_from_parts(&path, parsed, &raw, &index)))
}

pub(crate) async fn remove_list_item(
    State(state): State<AppState>,
    AxumPath(path_param): AxumPath<ListItemPath>,
) -> ApiResult<ListDetail> {
    let library = require_content_writes(&state).await?;
    let path = list_path(&path_param.id)?;
    let vfs = state.vault_vfs(&library.config.vault_root);

    let raw = read_list_raw(vfs.as_ref(), &path).await?;
    let (frontmatter, body) = split_frontmatter(&raw);
    let mut parsed = parse_list(&body);

    let index = normalized_entity_basename_index(&library.records);
    let before = parsed.items.len();
    // Drop every item that resolves to the named entity (unresolved items are kept).
    parsed.items.retain(|item| {
        item_target(item)
            .and_then(|target| find_target(&target, None, &index))
            .is_none_or(|found| found.summary.id != path_param.entity_id)
    });
    if parsed.items.len() != before {
        let new_body = render_list(
            &parsed.description,
            &parsed.items,
            parsed.ordered,
            &parsed.trailing,
        );
        let new_raw = compose_document(&frontmatter, &new_body);
        write_entity_raw(vfs.as_ref(), &path, &new_raw).await?;
        return Ok(Json(detail_from_parts(&path, parsed, &new_raw, &index)));
    }
    Ok(Json(detail_from_parts(&path, parsed, &raw, &index)))
}

/// Whether any item in `items` resolves to `entity_id`.
fn list_contains_entity(
    items: &[String],
    entity_id: &str,
    index: &HashMap<String, Vec<&EntityRecord>>,
) -> bool {
    items.iter().any(|item| {
        item_target(item)
            .and_then(|target| find_target(&target, None, index))
            .is_some_and(|found| found.summary.id == entity_id)
    })
}

/// Vault-relative path of a list from its id, validated for containment.
fn list_path(id: &str) -> Result<String, ApiError> {
    let basename = sanitize_basename(id).map_err(|_| ApiError::bad_request("Invalid list id"))?;
    Ok(format!("{LISTS_DIR}/{basename}.md"))
}

/// The list id (basename without `.md`) from a vault-relative path.
fn list_id(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .strip_suffix(".md")
        .unwrap_or(path)
        .to_string()
}

async fn read_list_raw(vfs: &dyn Vfs, path: &str) -> Result<String, ApiError> {
    match vfs.read_to_string(path).await {
        Ok(raw) => Ok(raw),
        Err(err) if err.is_not_found() => Err(ApiError::not_found("List not found")),
        Err(err) => Err(anyhow::anyhow!("failed to read list {path}: {err}").into()),
    }
}

fn detail_from_raw(path: &str, raw: &str, library: &Library) -> ListDetail {
    let (_, body) = split_frontmatter(raw);
    let parsed = parse_list(&body);
    let index = normalized_entity_basename_index(&library.records);
    detail_from_parts(path, parsed, raw, &index)
}

fn detail_from_parts(
    path: &str,
    parsed: ParsedList,
    raw: &str,
    index: &HashMap<String, Vec<&EntityRecord>>,
) -> ListDetail {
    let id = list_id(path);
    ListDetail {
        name: id.clone(),
        id,
        path: path.to_string(),
        items: resolve_items(&parsed.items, index),
        // Trim the section separators (the blank line around the list) so the
        // editor shows clean text; `render_list` re-adds them, keeping writes
        // idempotent.
        description: parsed.description.trim().to_string(),
        ordered: parsed.ordered,
        trailing: parsed.trailing.trim().to_string(),
        revision: file_revision(raw),
    }
}

/// Resolves each raw item to its wikilink target and (when matched) the indexed
/// entity. Unresolved items keep their text/target with a `None` entity.
fn resolve_items(items: &[String], index: &HashMap<String, Vec<&EntityRecord>>) -> Vec<ListItem> {
    items
        .iter()
        .map(|content| {
            let target = item_target(content);
            let entity = target
                .as_deref()
                .and_then(|target| find_target(target, None, index))
                .map(|record| record.summary.clone());
            ListItem {
                text: content.clone(),
                target,
                entity,
            }
        })
        .collect()
}
