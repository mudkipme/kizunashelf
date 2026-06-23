//! Asset download + serving. Split into:
//! - [`download`]: the per-entity download engine (fetch, collision rule, write)
//! - [`ssrf`]: the SSRF guard re-validated on every redirect hop
//! - [`job`]: batch download jobs (queue/run/report)
//! - [`util`]: pure path/URL/image-type helpers
//!
//! This module wires the single-entity download and the asset-serve routes, and
//! re-exports the handlers the router registers.

mod download;
mod job;
mod ssrf;
mod util;

pub(crate) use job::{cancel_asset_job, create_asset_job, get_asset_job, list_asset_jobs};
pub(crate) use util::entity_asset_dir;

use super::error::{ApiError, ApiResult};
use super::mutations::{check_revision, EntityPath};
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{AssetDownloadRequest, AssetDownloadResponse, AssetDownloadStatus};
use crate::library::load_entity;
use crate::vfs::normalize_relative;
use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::header;
use axum::response::Response;
use axum::Json;
use std::path::Path;

use download::download_entity_core;
use util::{all_local_asset_paths, content_type_for_extension};

// ----------------------------------------------------------------------------
// Single-entity download endpoint
// ----------------------------------------------------------------------------

pub(crate) async fn download_entity_assets(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<AssetDownloadRequest>,
) -> ApiResult<AssetDownloadResponse> {
    let library = require_content_writes(&state).await?;
    let Some(entity) = library
        .records
        .iter()
        .find(|item| item.summary.id == path.id)
    else {
        return Err(ApiError::not_found("Entity not found"));
    };
    check_revision(&request.revision, &entity.revision)?;
    let Some(type_config) = library.config.type_config(&entity.summary.entity_type) else {
        return Err(ApiError::bad_request("Unknown entity type"));
    };

    let all_local = all_local_asset_paths(&library);
    let vfs = state.vault_vfs(&library.config.vault_root);
    let results = download_entity_core(
        state.http_client(),
        vfs.as_ref(),
        library.config.resolved_asset_root(),
        entity,
        type_config,
        &all_local,
        request.fields.as_deref(),
    )
    .await?;

    if results
        .iter()
        .any(|item| item.status == AssetDownloadStatus::Downloaded)
    {
        state.invalidate_cache().await;
    }

    let reloaded = get_library(&state).await?;
    let record = reloaded
        .records
        .iter()
        .find(|item| item.summary.id == path.id)
        .ok_or_else(|| ApiError::not_found("Entity was not indexed"))?;
    let entity = load_entity(&reloaded.config, vfs.as_ref(), &record.summary).await?;
    Ok(Json(AssetDownloadResponse { entity, results }))
}

// ----------------------------------------------------------------------------
// Serve route: GET /api/assets/{*path}
// ----------------------------------------------------------------------------

pub(crate) async fn serve_asset(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Result<Response, ApiError> {
    let library = get_library(&state).await?;
    // The wildcard path is vault-relative and includes the asset-root prefix
    // (e.g. `Assets/Taxonomy/Anime/Foo/cover_url.jpg`), matching what is written
    // into frontmatter. Normalize it (rejecting traversal) and constrain it to
    // the asset directory before reading through the VFS.
    let normalized = normalize_relative(&path)
        .map_err(|_| ApiError::forbidden("Asset path is outside the asset root"))?;
    let asset_root = library.config.resolved_asset_root().trim_end_matches('/');
    let inside_assets =
        normalized == asset_root || normalized.starts_with(&format!("{asset_root}/"));
    if !inside_assets {
        return Err(ApiError::forbidden("Asset path is outside the asset root"));
    }

    let vfs = state.vault_vfs(&library.config.vault_root);
    let bytes = vfs
        .read(&normalized)
        .await
        .map_err(|_| ApiError::not_found("Asset not found"))?;
    let content_type = Path::new(&normalized)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| content_type_for_extension(&ext.to_lowercase()))
        .unwrap_or("application/octet-stream");

    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::from(bytes))
        .map_err(|error| ApiError::from(anyhow::anyhow!(error.to_string())))
}
