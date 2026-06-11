use super::assets::entity_asset_dir;
use super::error::{ApiError, ApiResult};
use super::state::{content_writes_enabled, get_library, AppState};
use crate::contract::{
    CreateEntityRequest, DeleteEntityRequest, DeleteEntityResponse, EntityMutationResponse,
    UpdateEntityRequest,
};
use crate::library::{serialize_markdown_document, split_markdown_document};
use crate::types::EntityTypeConfig;
use anyhow::{Context, Result};
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::{Component, Path, PathBuf};
use tokio::fs;

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
    let Some(entity) = library
        .entities
        .iter()
        .find(|item| item.summary.id == path.id)
    else {
        return Err(ApiError::not_found("Entity not found"));
    };
    if request.revision != entity.revision {
        return Err(ApiError::conflict("Entity changed since it was loaded"));
    }

    let source_path =
        entity_absolute_path(&library.config.vault_root, &entity.summary.path).await?;
    let raw = fs::read_to_string(&source_path)
        .await
        .with_context(|| format!("failed to read entity {}", source_path.display()))?;
    let mut document = split_markdown_document(&raw);
    if let Some(frontmatter) = request.frontmatter {
        apply_frontmatter_patch(&mut document.frontmatter, frontmatter);
    }
    if let Some(body) = request.body {
        document.body = body;
    }
    let target_path = if let Some(rename_to) = &request.rename_to {
        let basename = sanitize_basename(rename_to)
            .map_err(|error| ApiError::bad_request(&error.to_string()))?;
        let parent = source_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("entity path has no parent"))?;
        let target = parent.join(format!("{basename}.md"));
        if target != source_path
            && fs::try_exists(&target)
                .await
                .context("failed to check target entity path")?
        {
            return Err(ApiError::conflict("Target entity file already exists"));
        }
        // Move the entity's asset directory alongside the rename and rewrite any
        // frontmatter image paths that lived inside it (best-effort, non-fatal).
        if target != source_path {
            let new_relative = match entity.summary.path.rsplit_once('/') {
                Some((dir, _)) => format!("{dir}/{basename}.md"),
                None => format!("{basename}.md"),
            };
            move_entity_assets(
                &library.config.vault_root,
                library.config.resolved_asset_root(),
                &entity.summary.path,
                &new_relative,
                &mut document.frontmatter,
            )
            .await;
        }
        target
    } else {
        source_path.clone()
    };

    let raw = serialize_markdown_document(&document.frontmatter, &document.body);
    backup_file(&library.config.vault_root, &source_path, "update").await?;
    write_entity_raw(&library.config.vault_root, &target_path, &raw).await?;
    if target_path != source_path {
        fs::remove_file(&source_path)
            .await
            .with_context(|| format!("failed to remove old entity {}", source_path.display()))?;
    }
    state.invalidate_cache().await;
    let reloaded = get_library(&state).await?;
    let relative_target = relative_path(Path::new(&reloaded.config.vault_root), &target_path);
    let entity = reloaded
        .entities
        .iter()
        .find(|item| item.summary.path == relative_target)
        .cloned()
        .or_else(|| {
            reloaded
                .entities
                .iter()
                .find(|item| item.summary.id == path.id)
                .cloned()
        })
        .ok_or_else(|| ApiError::not_found("Updated entity was not indexed"))?;
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
    let path = entity_create_path(
        &library.config.vault_root,
        &library.config.taxonomy_root,
        type_config,
        &basename,
    )
    .await?;
    if fs::try_exists(&path)
        .await
        .context("failed to check entity path")?
    {
        return Err(ApiError::conflict("Entity file already exists"));
    }
    let raw =
        serialize_markdown_document(&request.frontmatter, request.body.as_deref().unwrap_or(""));
    write_entity_raw(&library.config.vault_root, &path, &raw).await?;
    state.invalidate_cache().await;
    let reloaded = get_library(&state).await?;
    let relative = relative_path(Path::new(&reloaded.config.vault_root), &path);
    let entity = reloaded
        .entities
        .iter()
        .find(|item| item.summary.path == relative)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Created entity was not indexed"))?;
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
    let Some(entity) = library
        .entities
        .iter()
        .find(|item| item.summary.id == path.id)
    else {
        return Err(ApiError::not_found("Entity not found"));
    };
    if request.revision != entity.revision {
        return Err(ApiError::conflict("Entity changed since it was loaded"));
    }
    let source_path =
        entity_absolute_path(&library.config.vault_root, &entity.summary.path).await?;
    let bucket = if request.mode.as_deref() == Some("delete") {
        "backups"
    } else {
        "trash"
    };
    let backup_path = backup_file(&library.config.vault_root, &source_path, bucket).await?;
    fs::remove_file(&source_path)
        .await
        .with_context(|| format!("failed to delete entity {}", source_path.display()))?;
    let asset_dir = entity_asset_dir(library.config.resolved_asset_root(), &entity.summary.path);
    trash_entity_assets(&library.config.vault_root, &asset_dir, bucket).await;
    state.invalidate_cache().await;
    Ok(Json(DeleteEntityResponse {
        deleted_id: path.id,
        backup_path: relative_path(Path::new(&library.config.vault_root), &backup_path),
    }))
}

/// Moves an entity's asset directory to follow an in-app rename and rewrites
/// frontmatter image paths that pointed inside it. Best-effort: if the source is
/// absent or the destination already exists, nothing is moved and the existing
/// (still valid) paths are left in place.
async fn move_entity_assets(
    vault_root: &str,
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
    let root = Path::new(vault_root);
    let old_abs = root.join(&old_dir);
    let new_abs = root.join(&new_dir);
    if !fs::try_exists(&old_abs).await.unwrap_or(false) {
        return;
    }
    if fs::try_exists(&new_abs).await.unwrap_or(false) {
        return;
    }
    if let Some(parent) = new_abs.parent() {
        if fs::create_dir_all(parent).await.is_err() {
            return;
        }
    }
    if fs::rename(&old_abs, &new_abs).await.is_err() {
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

/// Moves an entity's asset directory into the `.kizunashelf/<bucket>` area when
/// the entity is deleted. Best-effort and non-fatal.
async fn trash_entity_assets(vault_root: &str, asset_dir_relative: &str, bucket: &str) {
    let Ok(root) = Path::new(vault_root).canonicalize() else {
        return;
    };
    let source = root.join(asset_dir_relative);
    if !fs::try_exists(&source).await.unwrap_or(false) {
        return;
    }
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
    let dest = root
        .join(".kizunashelf")
        .join(bucket)
        .join(timestamp)
        .join(asset_dir_relative);
    if let Some(parent) = dest.parent() {
        if fs::create_dir_all(parent).await.is_err() {
            return;
        }
    }
    let _ = fs::rename(&source, &dest).await;
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

pub(super) async fn entity_absolute_path(vault_root: &str, relative: &str) -> Result<PathBuf> {
    let root = Path::new(vault_root).canonicalize()?;
    let path = root.join(relative);
    ensure_path_inside_root(&root, &path).await?;
    Ok(path)
}

async fn entity_create_path(
    vault_root: &str,
    taxonomy_root: &str,
    type_config: &EntityTypeConfig,
    basename: &str,
) -> Result<PathBuf> {
    let root = Path::new(vault_root).canonicalize()?;
    let dir = root.join(taxonomy_root).join(&type_config.path);
    ensure_path_inside_root(&root, &dir).await?;
    Ok(dir.join(format!("{basename}.md")))
}

pub(super) async fn write_entity_raw(vault_root: &str, path: &Path, raw: &str) -> Result<()> {
    let root = Path::new(vault_root).canonicalize()?;
    ensure_path_inside_root(&root, path).await?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create entity directory {}", parent.display()))?;
    }
    fs::write(path, raw)
        .await
        .with_context(|| format!("failed to write entity {}", path.display()))
}

pub(super) async fn backup_file(
    vault_root: &str,
    source_path: &Path,
    bucket: &str,
) -> Result<PathBuf> {
    let root = Path::new(vault_root).canonicalize()?;
    ensure_path_inside_root(&root, source_path).await?;
    let relative = source_path.strip_prefix(&root).unwrap_or(source_path);
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
    let backup_path = root
        .join(".kizunashelf")
        .join(bucket)
        .join(timestamp)
        .join(relative);
    if let Some(parent) = backup_path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create backup directory {}", parent.display()))?;
    }
    fs::copy(source_path, &backup_path)
        .await
        .with_context(|| format!("failed to back up entity {}", source_path.display()))?;
    Ok(backup_path)
}

pub(super) async fn ensure_path_inside_root(root: &Path, path: &Path) -> Result<()> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        anyhow::bail!("entity path cannot contain parent directory components");
    }
    let parent = if path.extension().is_some() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let canonical_parent = match parent.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            let mut existing = parent;
            while !existing.exists() {
                existing = existing.parent().unwrap_or(root);
                if existing == root {
                    break;
                }
            }
            existing.canonicalize()?
        }
    };
    if !canonical_parent.starts_with(root) {
        anyhow::bail!("entity path is outside the vault root");
    }
    Ok(())
}

pub(super) fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
