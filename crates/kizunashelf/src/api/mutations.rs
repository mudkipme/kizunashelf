use super::assets::entity_asset_dir;
use super::error::{ApiError, ApiResult};
use super::state::{content_writes_enabled, get_library, AppState};
use crate::contract::{
    CreateEntityRequest, DeleteEntityRequest, DeleteEntityResponse, EntityMutationResponse,
    UpdateEntityRequest,
};
use crate::library::{
    file_revision, load_entity, serialize_markdown_document, split_markdown_document,
};
use crate::types::EntityTypeConfig;
use crate::vfs::Vfs;
use anyhow::Result;
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct EntityPath {
    pub(super) id: String,
}

pub(crate) async fn update_entity(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<UpdateEntityRequest>,
) -> ApiResult<EntityMutationResponse> {
    let library = get_library(&state).await?;
    if !content_writes_enabled(&state, &library) {
        return Err(ApiError::forbidden("Content writes are disabled"));
    }
    let Some(entity) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    if request.revision != entity.revision {
        return Err(ApiError::conflict("Entity changed since it was loaded"));
    }

    let vfs = state.vault_vfs(&library.config.vault_root);
    let source_rel = entity.summary.path.clone();
    let raw = vfs
        .read_to_string(&source_rel)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {source_rel}: {error}"))?;
    // Re-check the revision against the *freshly read* content, not just the
    // cached index. Otherwise an external edit landing between the cache snapshot
    // and this read would still match the client's revision and be silently
    // overwritten (TOCTOU).
    if request.revision != file_revision(&raw) {
        return Err(ApiError::conflict("Entity changed since it was loaded"));
    }
    let mut document = split_markdown_document(&raw);
    if let Some(frontmatter) = request.frontmatter {
        apply_frontmatter_patch(&mut document.frontmatter, frontmatter);
    }
    if let Some(body) = request.body {
        document.body = body;
    }
    let target_rel = if let Some(rename_to) = &request.rename_to {
        let basename = sanitize_basename(rename_to)
            .map_err(|error| ApiError::bad_request(&error.to_string()))?;
        let target = match source_rel.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/{basename}.md"),
            None => format!("{basename}.md"),
        };
        if target != source_rel
            && vfs
                .exists(&target)
                .await
                .map_err(|error| anyhow::anyhow!("failed to check target entity path: {error}"))?
        {
            return Err(ApiError::conflict("Target entity file already exists"));
        }
        // Move the entity's asset directory alongside the rename and rewrite any
        // frontmatter image paths that lived inside it (best-effort, non-fatal).
        if target != source_rel {
            move_entity_assets(
                vfs.as_ref(),
                library.config.resolved_asset_root(),
                &source_rel,
                &target,
                &mut document.frontmatter,
            )
            .await;
        }
        target
    } else {
        source_rel.clone()
    };

    let raw = serialize_markdown_document(&document.frontmatter, &document.body);
    write_entity_raw(vfs.as_ref(), &target_rel, &raw).await?;
    if target_rel != source_rel {
        vfs.remove_file(&source_rel).await.map_err(|error| {
            anyhow::anyhow!("failed to remove old entity {source_rel}: {error}")
        })?;
    }
    // Patch the cached library in place for this edit (full reload only on a
    // structural change), instead of re-reading the whole vault from disk.
    let reloaded = state.apply_entity_edit(&target_rel).await?;
    let record = reloaded
        .record_by_path(&target_rel)
        .or_else(|| reloaded.record_by_id(&path.id))
        .ok_or_else(|| ApiError::not_found("Updated entity was not indexed"))?;
    let entity = load_entity(&reloaded.config, vfs.as_ref(), &record.summary).await?;
    Ok(Json(EntityMutationResponse { entity }))
}

pub(crate) async fn create_entity(
    State(state): State<AppState>,
    Json(request): Json<CreateEntityRequest>,
) -> ApiResult<EntityMutationResponse> {
    let library = get_library(&state).await?;
    if !content_writes_enabled(&state, &library) {
        return Err(ApiError::forbidden("Content writes are disabled"));
    }
    let Some(type_config) = library
        .config
        .types
        .iter()
        .find(|item| item.id == request.entity_type)
    else {
        return Err(ApiError::bad_request("Unknown entity type"));
    };
    let basename = sanitize_basename(&request.basename)
        .map_err(|error| ApiError::bad_request(&error.to_string()))?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let path = entity_create_path(&library.config.taxonomy_root, type_config, &basename);
    if vfs
        .exists(&path)
        .await
        .map_err(|error| anyhow::anyhow!("failed to check entity path: {error}"))?
    {
        return Err(ApiError::conflict("Entity file already exists"));
    }
    let raw =
        serialize_markdown_document(&request.frontmatter, request.body.as_deref().unwrap_or(""));
    write_entity_raw(vfs.as_ref(), &path, &raw).await?;
    state.invalidate_cache().await;
    let reloaded = get_library(&state).await?;
    let record = reloaded
        .record_by_path(&path)
        .ok_or_else(|| ApiError::not_found("Created entity was not indexed"))?;
    let entity = load_entity(&reloaded.config, vfs.as_ref(), &record.summary).await?;
    Ok(Json(EntityMutationResponse { entity }))
}

pub(crate) async fn delete_entity(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<DeleteEntityRequest>,
) -> ApiResult<DeleteEntityResponse> {
    let library = get_library(&state).await?;
    if !content_writes_enabled(&state, &library) {
        return Err(ApiError::forbidden("Content writes are disabled"));
    }
    let Some(entity) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    if request.revision != entity.revision {
        return Err(ApiError::conflict("Entity changed since it was loaded"));
    }
    let vfs = state.vault_vfs(&library.config.vault_root);
    let trash_path = move_to_trash(vfs.as_ref(), &entity.summary.path).await?;
    let asset_dir = entity_asset_dir(library.config.resolved_asset_root(), &entity.summary.path);
    trash_entity_assets(vfs.as_ref(), &asset_dir).await;
    state.invalidate_cache().await;
    Ok(Json(DeleteEntityResponse {
        deleted_id: path.id,
        backup_path: trash_path,
    }))
}

/// Moves an entity's asset directory to follow an in-app rename and rewrites
/// frontmatter image paths that pointed inside it. Best-effort: if the source is
/// absent or the destination already exists, nothing is moved and the existing
/// (still valid) paths are left in place.
async fn move_entity_assets(
    vfs: &dyn Vfs,
    asset_root: &str,
    old_relative: &str,
    new_relative: &str,
    frontmatter: &mut Map<String, Value>,
) {
    let old_dir = entity_asset_dir(asset_root, old_relative);
    let new_dir = entity_asset_dir(asset_root, new_relative);
    if old_dir == new_dir {
        return;
    }
    if !vfs.exists(&old_dir).await.unwrap_or(false) {
        return;
    }
    if vfs.exists(&new_dir).await.unwrap_or(false) {
        return;
    }
    if let Some(parent) = parent_dir(&new_dir) {
        if vfs.create_dir_all(parent).await.is_err() {
            return;
        }
    }
    if vfs.rename(&old_dir, &new_dir).await.is_err() {
        return;
    }
    rewrite_asset_prefix(frontmatter, &old_dir, &new_dir);
}

/// Rewrites string (and string-array) frontmatter values that begin with
/// `<old_dir>/` so they point at `<new_dir>/` instead.
fn rewrite_asset_prefix(frontmatter: &mut Map<String, Value>, old_dir: &str, new_dir: &str) {
    let old_prefix = format!("{old_dir}/");
    let rewrite = |value: &mut Value| {
        if let Value::String(text) = value {
            if let Some(rest) = text.strip_prefix(&old_prefix) {
                *text = format!("{new_dir}/{rest}");
            }
        }
    };
    for value in frontmatter.values_mut() {
        match value {
            Value::Array(items) => items.iter_mut().for_each(rewrite),
            other => rewrite(other),
        }
    }
}

/// Moves an entity's Markdown file into the vault's `.trash` folder, mirroring
/// Obsidian's local trash. Returns the vault-relative trash path. Disambiguates
/// name collisions by appending ` 1`, ` 2`, … before the extension.
async fn move_to_trash(vfs: &dyn Vfs, source_relative: &str) -> Result<String> {
    let file_name = source_relative
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow::anyhow!("entity path has no file name"))?;
    vfs.create_dir_all(".trash")
        .await
        .map_err(|error| anyhow::anyhow!("failed to create trash directory: {error}"))?;
    let dest = unique_path(vfs, ".trash", file_name).await;
    vfs.rename(source_relative, &dest)
        .await
        .map_err(|error| anyhow::anyhow!("failed to move entity to trash {dest}: {error}"))?;
    Ok(dest)
}

/// Returns a vault-relative path for `file_name` inside `dir`, appending ` 1`,
/// ` 2`, … before the extension if a file with that name already exists.
async fn unique_path(vfs: &dyn Vfs, dir: &str, file_name: &str) -> String {
    let candidate = format!("{dir}/{file_name}");
    if !vfs.exists(&candidate).await.unwrap_or(false) {
        return candidate;
    }
    let name = Path::new(file_name);
    let stem = name
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    let extension = name.extension().and_then(|s| s.to_str());
    let mut counter = 1;
    loop {
        let candidate_name = match extension {
            Some(extension) => format!("{stem} {counter}.{extension}"),
            None => format!("{stem} {counter}"),
        };
        let candidate = format!("{dir}/{candidate_name}");
        if !vfs.exists(&candidate).await.unwrap_or(false) {
            return candidate;
        }
        counter += 1;
    }
}

/// Moves an entity's asset directory into the vault's `.trash` folder, mirroring
/// its vault-relative path. Best-effort and non-fatal.
async fn trash_entity_assets(vfs: &dyn Vfs, asset_dir_relative: &str) {
    if !vfs.exists(asset_dir_relative).await.unwrap_or(false) {
        return;
    }
    let mut dest = format!(".trash/{asset_dir_relative}");
    if vfs.exists(&dest).await.unwrap_or(false) {
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
        dest = format!(".trash/{asset_dir_relative}-{timestamp}");
    }
    if let Some(parent) = parent_dir(&dest) {
        if vfs.create_dir_all(parent).await.is_err() {
            return;
        }
    }
    let _ = vfs.rename(asset_dir_relative, &dest).await;
}

fn apply_frontmatter_patch(target: &mut Map<String, Value>, patch: Map<String, Value>) {
    for (key, value) in patch {
        if value.is_null() {
            target.remove(&key);
        } else {
            target.insert(key, value);
        }
    }
}

fn sanitize_basename(value: &str) -> Result<String> {
    let basename = value.trim();
    if basename.is_empty() {
        anyhow::bail!("Entity filename cannot be empty");
    }
    if basename.to_lowercase().ends_with(".md") {
        anyhow::bail!("Entity filename must be a basename without .md");
    }
    let path = Path::new(basename);
    if path.components().count() != 1
        || basename.contains('/')
        || basename.contains('\\')
        || basename == "."
        || basename == ".."
    {
        anyhow::bail!("Entity filename must be a single Markdown basename");
    }
    if basename.chars().any(is_forbidden_obsidian_filename_char) {
        anyhow::bail!("Entity filename cannot contain / \\ : * ? \" < > | or control characters");
    }
    Ok(basename.to_string())
}

fn is_forbidden_obsidian_filename_char(character: char) -> bool {
    matches!(
        character,
        '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
    ) || character.is_control()
}

/// Vault-relative path for a new entity file under its type directory.
fn entity_create_path(
    taxonomy_root: &str,
    type_config: &EntityTypeConfig,
    basename: &str,
) -> String {
    format!(
        "{}/{}/{basename}.md",
        taxonomy_root.trim_end_matches('/'),
        type_config.path
    )
}

/// Writes an entity's raw Markdown to a vault-relative path, creating parent
/// directories. Containment is enforced by the VFS's path normalization.
///
/// The active VFS supplies the atomic replacement mechanism, so a crash or a
/// concurrent reader never observes a truncated half-written Markdown file.
pub(super) async fn write_entity_raw(vfs: &dyn Vfs, relative: &str, raw: &str) -> Result<()> {
    if let Some(parent) = parent_dir(relative) {
        vfs.create_dir_all(parent).await.map_err(|error| {
            anyhow::anyhow!("failed to create entity directory {parent}: {error}")
        })?;
    }
    vfs.write_atomic(relative, raw.as_bytes())
        .await
        .map_err(|error| anyhow::anyhow!("failed to write entity {relative}: {error}"))
}

/// Parent directory of a vault-relative path, or `None` when it lives at the
/// vault root.
pub(super) fn parent_dir(relative: &str) -> Option<&str> {
    relative.rsplit_once('/').map(|(parent, _)| parent)
}
