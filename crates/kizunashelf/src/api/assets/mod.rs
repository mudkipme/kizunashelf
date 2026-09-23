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
use super::mutations::{check_revision, type_config_or_err, write_entity_raw, EntityPath};
use super::state::{get_library, require_content_writes, require_host_asset_ingest, AppState};
use crate::contract::{
    AssetDownloadItemResult, AssetDownloadPlan, AssetDownloadPlanItem, AssetDownloadRequest,
    AssetDownloadResponse, AssetIngestRequest, AssetIngestResponse, AssetUploadRequest,
    AssetUploadResponse,
};
use crate::library::{
    file_revision, load_entity, serialize_markdown_document, split_markdown_document,
};
use crate::types::{FieldType, Library};
use crate::vfs::{normalize_relative, read_nfc_tolerant};
use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::header;
use axum::response::Response;
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::Path;

use base64::Engine;
use download::{
    download_entity_core, ingest_field_bytes, place_uploaded_asset, upload_error, DownloadContext,
    IngestField,
};
use util::{
    all_local_asset_paths, content_type_for_extension, entity_local_asset_paths, is_remote_url,
    resolve_entity_asset_dir, value_to_list,
};

// ----------------------------------------------------------------------------
// Single-entity download endpoint
// ----------------------------------------------------------------------------

pub(crate) async fn download_entity_assets(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<AssetDownloadRequest>,
) -> ApiResult<AssetDownloadResponse> {
    let library = require_content_writes(&state).await?;
    let Some(entity) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    check_revision(&request.revision, &entity.revision)?;
    let type_config = type_config_or_err(&library.config, &entity.summary.entity_type)?;

    let all_local = all_local_asset_paths(&library);
    let vfs = state.vault_vfs(&library.config.vault_root);
    let results = download_entity_core(
        &state,
        vfs.as_ref(),
        library.config.resolved_asset_root(),
        entity,
        type_config,
        &all_local,
        request.fields.as_deref(),
    )
    .await?;

    let reloaded = get_library(&state).await?;
    let record = reloaded
        .record_by_id(&path.id)
        .ok_or_else(|| ApiError::not_found("Entity was not indexed"))?;
    let entity = load_entity(&reloaded.config, vfs.as_ref(), &record.summary).await?;
    Ok(Json(AssetDownloadResponse { entity, results }))
}

/// Downloads every remote cover for a freshly-created entity, rewriting each into
/// a local asset path in frontmatter. A failed download keeps its remote URL (a
/// `Failed` result item) — fail-safe. The entity's just-created revision guards
/// the short final commit, and the quick-add caller reloads after all enrichment.
/// Returns an empty list if the entity vanished or has no cover fields.
pub(crate) async fn download_new_entity_covers(
    state: &AppState,
    library: &Library,
    entity_id: &str,
) -> Result<Vec<AssetDownloadItemResult>, ApiError> {
    let Some(entity) = library.record_by_id(entity_id) else {
        return Ok(Vec::new());
    };
    let type_config = type_config_or_err(&library.config, &entity.summary.entity_type)?;
    let all_local = all_local_asset_paths(library);
    let vfs = state.vault_vfs(&library.config.vault_root);
    download_entity_core(
        state,
        vfs.as_ref(),
        library.config.resolved_asset_root(),
        entity,
        type_config,
        &all_local,
        None,
    )
    .await
}

// ----------------------------------------------------------------------------
// Host-driven download: plan + ingest
//
// These let a host (iOS) do the HTTP itself via a background URLSession while the
// core keeps owning validation, placement, and the frontmatter rewrite.
// ----------------------------------------------------------------------------

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanQuery {
    /// Restrict the plan to one entity type; when omitted, the whole library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    entity_type: Option<String>,
}

/// Enumerates every remote image URL eligible for download. The host fetches each
/// `source_url` itself and calls `ingest_entity_asset` per result.
///
/// Note: unlike the reqwest path, the host follows redirects without per-hop SSRF
/// re-validation. Acceptable for a local single-user app fetching user-entered
/// cover URLs; the core still validates the bytes are an image at ingest time.
pub(crate) async fn plan_asset_downloads(
    State(state): State<AppState>,
    Query(query): Query<PlanQuery>,
) -> ApiResult<AssetDownloadPlan> {
    require_host_asset_ingest(&state)?;
    let library = get_library(&state).await?;
    if let Some(entity_type) = query.entity_type.as_deref() {
        if library.config.type_config(entity_type).is_none() {
            return Err(ApiError::bad_request("Unknown entity type"));
        }
    }

    let mut items = Vec::new();
    for entity in &library.records {
        if let Some(entity_type) = query.entity_type.as_deref() {
            if entity.summary.entity_type != entity_type {
                continue;
            }
        }
        let Some(type_config) = library.config.type_config(&entity.summary.entity_type) else {
            continue;
        };
        for field in &type_config.fields {
            let is_list = match field.field_type {
                FieldType::Image => false,
                FieldType::ImageList => true,
                _ => continue,
            };
            for url in value_to_list(entity.frontmatter.get(&field.field)) {
                let url = url.trim();
                if !is_remote_url(url) {
                    continue;
                }
                items.push(AssetDownloadPlanItem {
                    entity_id: entity.summary.id.clone(),
                    entity_title: entity.summary.title.clone(),
                    entity_type: entity.summary.entity_type.clone(),
                    field: field.field.clone(),
                    list_key: is_list.then(|| url.to_string()),
                    source_url: url.to_string(),
                    revision: entity.revision.clone(),
                });
            }
        }
    }

    let scope = query
        .entity_type
        .as_deref()
        .map(|entity_type| format!("type:{entity_type}"))
        .unwrap_or_else(|| "all".to_string());
    Ok(Json(AssetDownloadPlan { scope, items }))
}

/// Validates + places one already-downloaded image and rewrites its frontmatter
/// field. The entity is re-read fresh so out-of-order ingests don't clobber each
/// other, and an idempotency guard skips fields the user has since changed.
pub(crate) async fn ingest_entity_asset(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<AssetIngestRequest>,
) -> ApiResult<AssetIngestResponse> {
    // Reading + deleting a client-supplied host path is a host-only capability;
    // reject it before any content-write or filesystem work on every other runtime.
    require_host_asset_ingest(&state)?;
    // Native foreground and background routers share this guard and cache
    // generation. Resolve the schema inside it so another router cannot change
    // an image field's meaning between this read and the conditional commit.
    let mutation = state.content_mutation_lock().await;
    let library = require_content_writes(&state).await?;
    let Some(entity) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let type_config = type_config_or_err(&library.config, &entity.summary.entity_type)?;
    let Some(field_config) = type_config
        .fields
        .iter()
        .find(|field| field.field == request.field)
    else {
        return Err(ApiError::bad_request("Unknown field"));
    };
    let is_list = match field_config.field_type {
        FieldType::Image => false,
        FieldType::ImageList => true,
        _ => return Err(ApiError::bad_request("Field is not an image field")),
    };

    let entity_id = entity.summary.id.clone();
    let entity_path = entity.summary.path.clone();
    let asset_root = library.config.resolved_asset_root().to_string();
    let all_local = all_local_asset_paths(&library);

    // Read the host-temp file the client downloaded (mirrors `indexCacheDir`'s
    // direct host-path access), then delete it regardless of outcome.
    let bytes = tokio::fs::read(&request.source_path)
        .await
        .map_err(|error| ApiError::bad_request(&format!("Cannot read downloaded file: {error}")))?;
    let _ = tokio::fs::remove_file(&request.source_path).await;

    let vfs = state.vault_vfs(&library.config.vault_root);
    let asset_dir = resolve_entity_asset_dir(vfs.as_ref(), &asset_root, &entity_path).await;
    let raw = vfs
        .read_to_string(&entity_path)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {entity_path}: {error}"))?;
    if let Some(revision) = request.revision.as_deref() {
        check_revision(revision, &file_revision(&raw))?;
    }
    let mut document = split_markdown_document(&raw);
    let owned = entity_local_asset_paths(&document.frontmatter, type_config);

    let ctx = DownloadContext {
        vfs: vfs.as_ref(),
        asset_dir: &asset_dir,
        referenced: &all_local,
        owned: &owned,
        entity_id: &entity_id,
    };
    let content_type = request.content_type.clone().unwrap_or_default();
    let field = IngestField {
        name: &request.field,
        is_list,
        list_key: request.list_key.as_deref(),
        source_url: &request.source_url,
    };
    let (changed, result) = ingest_field_bytes(
        &mutation,
        &ctx,
        &mut document.frontmatter,
        &field,
        bytes,
        &content_type,
    )
    .await;

    if changed {
        let new_raw = serialize_markdown_document(&document.frontmatter, &document.body);
        write_entity_raw(&mutation, vfs.as_ref(), &entity_path, &new_raw).await?;
        state.invalidate_cache().await;
    }

    let reloaded = get_library(&state).await?;
    let record = reloaded
        .record_by_id(&entity_id)
        .ok_or_else(|| ApiError::not_found("Entity was not indexed"))?;
    let entity = load_entity(&reloaded.config, vfs.as_ref(), &record.summary).await?;
    Ok(Json(AssetIngestResponse { entity, result }))
}

// ----------------------------------------------------------------------------
// Client upload: place device-picked bytes for one image field
//
// Unlike download/ingest, this does NOT rewrite frontmatter — it validates and
// places the file, then returns its vault-relative path. The editor stages that
// path into its draft and persists it on the normal save mutation, so an upload
// never races with other unsaved edits.
// ----------------------------------------------------------------------------

pub(crate) async fn upload_entity_asset(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<AssetUploadRequest>,
) -> ApiResult<AssetUploadResponse> {
    let library = require_content_writes(&state).await?;
    let Some(entity) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let type_config = type_config_or_err(&library.config, &entity.summary.entity_type)?;
    let Some(field_config) = type_config
        .fields
        .iter()
        .find(|field| field.field == request.field)
    else {
        return Err(ApiError::bad_request("Unknown field"));
    };
    let is_list = match field_config.field_type {
        FieldType::Image => false,
        FieldType::ImageList => true,
        _ => return Err(ApiError::bad_request("Field is not an image field")),
    };

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(request.data_base64.as_bytes())
        .map_err(|error| ApiError::bad_request(&format!("Invalid base64 payload: {error}")))?;
    if bytes.is_empty() {
        return Err(ApiError::bad_request("Upload payload is empty"));
    }

    let entity_id = entity.summary.id.clone();
    let entity_path = entity.summary.path.clone();
    let asset_root = library.config.resolved_asset_root().to_string();
    let all_local = all_local_asset_paths(&library);

    // Serialize placement with other content writes. Upload paths are immutable
    // and never replace a saved cover, even when the editor is later discarded.
    let mutation = state.content_mutation_lock().await;
    // Read current frontmatter without rewriting the entity.
    let vfs = state.vault_vfs(&library.config.vault_root);
    let asset_dir = resolve_entity_asset_dir(vfs.as_ref(), &asset_root, &entity_path).await;
    let raw = vfs
        .read_to_string(&entity_path)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {entity_path}: {error}"))?;
    let document = split_markdown_document(&raw);
    let owned = entity_local_asset_paths(&document.frontmatter, type_config);

    let ctx = DownloadContext {
        vfs: vfs.as_ref(),
        asset_dir: &asset_dir,
        referenced: &all_local,
        owned: &owned,
        entity_id: &entity_id,
    };
    let content_type = request.content_type.unwrap_or_default();
    let filename = request.filename.unwrap_or_default();
    let outcome = place_uploaded_asset(
        &mutation,
        &ctx,
        &request.field,
        is_list,
        bytes,
        &content_type,
        &filename,
    )
    .await
    .map_err(upload_error)?;

    Ok(Json(AssetUploadResponse {
        path: outcome.path,
        conflict_resolved: outcome.conflict_resolved,
    }))
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
    // Read NFC/NFD-tolerant: the stored path is NFC-composed, but Apple
    // filesystems hand back decomposed (NFD) directory entries, so a byte-exact
    // read would 404 an asset that exists (matching how entity loading tolerates
    // the same divide).
    let bytes = read_nfc_tolerant(vfs.as_ref(), &normalized)
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
