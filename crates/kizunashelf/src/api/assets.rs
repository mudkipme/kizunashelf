use super::error::{ApiError, ApiResult};
use super::mutations::{
    backup_file, ensure_path_inside_root, entity_absolute_path, write_entity_raw, EntityPath,
};
use super::state::{content_writes_enabled, get_library, AppState};
use crate::contract::{
    AssetDownloadItemResult, AssetDownloadRequest, AssetDownloadResponse, AssetDownloadStatus,
};
use crate::library::{serialize_markdown_document, split_markdown_document};
use crate::types::{EntityTypeConfig, FieldType, Library};
use anyhow::Context;
use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::header;
use axum::response::Response;
use axum::Json;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path};
use std::time::Duration;
use tokio::fs;

const MAX_ASSET_BYTES: u64 = 25 * 1024 * 1024;
const ASSET_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

// ----------------------------------------------------------------------------
// Single-entity download endpoint
// ----------------------------------------------------------------------------

pub(crate) async fn download_entity_assets(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<AssetDownloadRequest>,
) -> ApiResult<AssetDownloadResponse> {
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
    let Some(type_config) = library
        .config
        .types
        .iter()
        .find(|item| item.id == entity.summary.entity_type)
    else {
        return Err(ApiError::bad_request("Unknown entity type"));
    };

    let fields = image_fields(type_config, request.fields.as_deref());
    let referenced = referenced_by_others(&library, &entity.summary.id);
    let asset_dir = entity_asset_dir(library.config.resolved_asset_root(), &entity.summary.path);
    let vault_root = library.config.vault_root.clone();

    let source_path = entity_absolute_path(&vault_root, &entity.summary.path).await?;
    let raw = fs::read_to_string(&source_path)
        .await
        .with_context(|| format!("failed to read entity {}", source_path.display()))?;
    let mut document = split_markdown_document(&raw);

    let mut ctx = DownloadContext {
        client: state.http_client(),
        vault_root: &vault_root,
        asset_dir: &asset_dir,
        referenced: &referenced,
        entity_id: &entity.summary.id,
    };

    let mut results = Vec::new();
    let mut changed = false;
    for field in fields {
        let field_changed =
            process_field(&mut ctx, &mut document.frontmatter, &field, &mut results).await?;
        changed = changed || field_changed;
    }

    if changed {
        let new_raw = serialize_markdown_document(&document.frontmatter, &document.body);
        backup_file(&vault_root, &source_path, "update").await?;
        write_entity_raw(&vault_root, &source_path, &new_raw).await?;
        state.invalidate_cache().await;
    }

    let reloaded = get_library(&state).await?;
    let entity = reloaded
        .entities
        .iter()
        .find(|item| item.summary.id == path.id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Entity was not indexed"))?;
    Ok(Json(AssetDownloadResponse { entity, results }))
}

struct DownloadContext<'a> {
    client: &'a reqwest::Client,
    vault_root: &'a str,
    asset_dir: &'a str,
    referenced: &'a HashSet<String>,
    entity_id: &'a str,
}

struct ImageField {
    name: String,
    is_list: bool,
}

fn image_fields(type_config: &EntityTypeConfig, only: Option<&[String]>) -> Vec<ImageField> {
    type_config
        .fields
        .iter()
        .filter(|field| matches!(field.field_type, FieldType::Image | FieldType::ImageList))
        .filter(|field| only.is_none_or(|names| names.iter().any(|name| name == &field.field)))
        .map(|field| ImageField {
            name: field.field.clone(),
            is_list: field.field_type == FieldType::ImageList,
        })
        .collect()
}

/// Downloads remote URLs for one field, rewriting `frontmatter` in place.
/// Returns whether the frontmatter value changed.
async fn process_field(
    ctx: &mut DownloadContext<'_>,
    frontmatter: &mut Map<String, Value>,
    field: &ImageField,
    results: &mut Vec<AssetDownloadItemResult>,
) -> Result<bool, ApiError> {
    if field.is_list {
        let elements = value_to_list(frontmatter.get(&field.name));
        if elements.is_empty() {
            return Ok(false);
        }
        let mut rewritten = Vec::with_capacity(elements.len());
        let mut changed = false;
        for element in elements {
            let url = element.trim();
            if url.is_empty() {
                rewritten.push(element);
                continue;
            }
            if !is_remote_url(url) {
                results.push(skipped(&field.name, url, skip_reason(url)));
                rewritten.push(element);
                continue;
            }
            match download_to_asset(ctx, &field.name, Some(url), url).await {
                Ok(outcome) => {
                    results.push(downloaded(&field.name, url, &outcome));
                    rewritten.push(outcome.path);
                    changed = true;
                }
                Err(error) => {
                    results.push(failed(&field.name, url, &error.to_string()));
                    rewritten.push(element);
                }
            }
        }
        if changed {
            frontmatter.insert(
                field.name.clone(),
                Value::Array(rewritten.into_iter().map(Value::String).collect()),
            );
        }
        Ok(changed)
    } else {
        let Some(value) = frontmatter.get(&field.name).and_then(Value::as_str) else {
            return Ok(false);
        };
        let url = value.trim().to_string();
        if url.is_empty() {
            return Ok(false);
        }
        if !is_remote_url(&url) {
            results.push(skipped(&field.name, &url, skip_reason(&url)));
            return Ok(false);
        }
        match download_to_asset(ctx, &field.name, None, &url).await {
            Ok(outcome) => {
                results.push(downloaded(&field.name, &url, &outcome));
                frontmatter.insert(field.name.clone(), Value::String(outcome.path));
                Ok(true)
            }
            Err(error) => {
                results.push(failed(&field.name, &url, &error.to_string()));
                Ok(false)
            }
        }
    }
}

struct AssetOutcome {
    /// Vault-relative path written into frontmatter.
    path: String,
    conflict_resolved: bool,
}

/// Downloads a single URL and writes it under the entity's asset directory,
/// applying the cross-entity collision rule.
async fn download_to_asset(
    ctx: &DownloadContext<'_>,
    field_name: &str,
    list_key: Option<&str>,
    url: &str,
) -> Result<AssetOutcome, DownloadError> {
    let asset = download_one(ctx.client, url).await?;

    // Compute the intended vault-relative destination.
    let (dir, stem) = match list_key {
        Some(key) => (format!("{}/{}", ctx.asset_dir, field_name), short_hash(key)),
        None => (ctx.asset_dir.to_string(), field_name.to_string()),
    };
    let mut relative = format!("{dir}/{stem}.{}", asset.ext);
    let mut conflict_resolved = false;

    // Never overwrite a file referenced by another entity.
    if ctx.referenced.contains(&relative) {
        let disambiguated = format!("{dir}/{stem}-{}.{}", short_hash(ctx.entity_id), asset.ext);
        if ctx.referenced.contains(&disambiguated) {
            return Err(DownloadError::Collision);
        }
        relative = disambiguated;
        conflict_resolved = true;
    }

    write_asset_file(ctx.vault_root, &relative, &asset.bytes).await?;
    Ok(AssetOutcome {
        path: relative,
        conflict_resolved,
    })
}

struct DownloadedAsset {
    bytes: Vec<u8>,
    ext: String,
}

async fn download_one(
    client: &reqwest::Client,
    url: &str,
) -> Result<DownloadedAsset, DownloadError> {
    let response = client
        .get(url)
        .timeout(ASSET_DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|error| DownloadError::Request(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(DownloadError::Status(status.as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_ASSET_BYTES)
    {
        return Err(DownloadError::TooLarge);
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or(value)
                .trim()
                .to_lowercase()
        })
        .unwrap_or_default();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| DownloadError::Request(error.to_string()))?;
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err(DownloadError::TooLarge);
    }
    let sniffed = sniff_image_ext(&bytes);
    if !content_type.starts_with("image/") && sniffed.is_none() {
        return Err(DownloadError::NotAnImage);
    }
    let ext = extension_for_content_type(&content_type)
        .map(str::to_string)
        .or_else(|| sniffed.map(str::to_string))
        .or_else(|| extension_from_url(url))
        .unwrap_or_else(|| "img".to_string());
    Ok(DownloadedAsset {
        bytes: bytes.to_vec(),
        ext,
    })
}

/// Writes bytes atomically (`.tmp` then rename) under the vault root.
async fn write_asset_file(
    vault_root: &str,
    relative: &str,
    bytes: &[u8],
) -> Result<(), DownloadError> {
    let root = Path::new(vault_root)
        .canonicalize()
        .map_err(|error| DownloadError::Io(error.to_string()))?;
    let target = root.join(relative);
    ensure_path_inside_root(&root, &target)
        .await
        .map_err(|error| DownloadError::Io(error.to_string()))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| DownloadError::Io(error.to_string()))?;
    }
    let tmp = target.with_extension("tmp");
    fs::write(&tmp, bytes)
        .await
        .map_err(|error| DownloadError::Io(error.to_string()))?;
    fs::rename(&tmp, &target)
        .await
        .map_err(|error| DownloadError::Io(error.to_string()))?;
    Ok(())
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
    // into frontmatter. Resolve it from the vault root and constrain it to the
    // asset directory.
    if Path::new(&path)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ApiError::forbidden("Asset path is outside the asset root"));
    }
    let canonical_vault = Path::new(&library.config.vault_root)
        .canonicalize()
        .map_err(|_| ApiError::not_found("Asset not found"))?;
    let canonical_assets = canonical_vault
        .join(library.config.resolved_asset_root())
        .canonicalize()
        .map_err(|_| ApiError::not_found("Asset not found"))?;

    let canonical = canonical_vault
        .join(&path)
        .canonicalize()
        .map_err(|_| ApiError::not_found("Asset not found"))?;
    if !canonical.starts_with(&canonical_assets) {
        return Err(ApiError::forbidden("Asset path is outside the asset root"));
    }

    let bytes = fs::read(&canonical)
        .await
        .map_err(|_| ApiError::not_found("Asset not found"))?;
    let content_type = canonical
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

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

/// Vault-relative asset directory for an entity: `<assetRoot>/<entity path minus .md>`.
fn entity_asset_dir(asset_root: &str, entity_relative_path: &str) -> String {
    let stem = entity_relative_path
        .strip_suffix(".md")
        .unwrap_or(entity_relative_path);
    format!("{}/{stem}", asset_root.trim_end_matches('/'))
}

/// Builds the set of asset paths referenced by entities other than `current_id`,
/// so we never overwrite another entity's image.
fn referenced_by_others(library: &Library, current_id: &str) -> HashSet<String> {
    let mut referenced = HashSet::new();
    for entity in &library.entities {
        if entity.summary.id == current_id {
            continue;
        }
        let Some(type_config) = library
            .config
            .types
            .iter()
            .find(|item| item.id == entity.summary.entity_type)
        else {
            continue;
        };
        for field in &type_config.fields {
            if !matches!(field.field_type, FieldType::Image | FieldType::ImageList) {
                continue;
            }
            for value in value_to_list(entity.frontmatter.get(&field.field)) {
                let value = value.trim();
                if !value.is_empty() && !is_remote_url(value) {
                    referenced.insert(value.to_string());
                }
            }
        }
    }
    referenced
}

fn value_to_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn is_remote_url(value: &str) -> bool {
    let lowered = value.trim().to_ascii_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

fn skip_reason(value: &str) -> &'static str {
    if value.contains("://") || value.starts_with("data:") {
        "Unsupported source URL"
    } else {
        "Already a local asset"
    }
}

fn short_hash(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())[..12].to_string()
}

fn extension_for_content_type(content_type: &str) -> Option<&'static str> {
    match content_type {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        "image/avif" => Some("avif"),
        "image/svg+xml" => Some("svg"),
        "image/bmp" | "image/x-ms-bmp" => Some("bmp"),
        "image/tiff" => Some("tiff"),
        _ => None,
    }
}

fn content_type_for_extension(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "tiff" | "tif" => "image/tiff",
        _ => "application/octet-stream",
    }
}

fn extension_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = Path::new(path).extension()?.to_str()?.to_lowercase();
    if ext.is_empty() || ext.len() > 5 {
        return None;
    }
    Some(ext)
}

fn sniff_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && matches!(&bytes[8..12], b"avif" | b"avis") {
        return Some("avif");
    }
    if bytes.starts_with(b"BM") {
        return Some("bmp");
    }
    None
}

// ----------------------------------------------------------------------------
// Result builders
// ----------------------------------------------------------------------------

fn downloaded(field: &str, url: &str, outcome: &AssetOutcome) -> AssetDownloadItemResult {
    AssetDownloadItemResult {
        field: field.to_string(),
        status: AssetDownloadStatus::Downloaded,
        source_url: Some(url.to_string()),
        path: Some(outcome.path.clone()),
        message: None,
        conflict_resolved: outcome.conflict_resolved,
    }
}

fn skipped(field: &str, url: &str, message: &str) -> AssetDownloadItemResult {
    AssetDownloadItemResult {
        field: field.to_string(),
        status: AssetDownloadStatus::Skipped,
        source_url: Some(url.to_string()),
        path: None,
        message: Some(message.to_string()),
        conflict_resolved: false,
    }
}

fn failed(field: &str, url: &str, message: &str) -> AssetDownloadItemResult {
    AssetDownloadItemResult {
        field: field.to_string(),
        status: AssetDownloadStatus::Failed,
        source_url: Some(url.to_string()),
        path: None,
        message: Some(message.to_string()),
        conflict_resolved: false,
    }
}

#[derive(Debug)]
enum DownloadError {
    Request(String),
    Status(u16),
    TooLarge,
    NotAnImage,
    Collision,
    Io(String),
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DownloadError::Request(error) => write!(f, "Download failed: {error}"),
            DownloadError::Status(status) => write!(f, "Upstream returned HTTP {status}"),
            DownloadError::TooLarge => {
                write!(
                    f,
                    "Image exceeds the {} MB limit",
                    MAX_ASSET_BYTES / 1024 / 1024
                )
            }
            DownloadError::NotAnImage => write!(f, "Response is not an image"),
            DownloadError::Collision => {
                write!(f, "Destination is already used by another entity")
            }
            DownloadError::Io(error) => write!(f, "Failed to write asset: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_asset_dir_strips_md_suffix() {
        assert_eq!(
            entity_asset_dir("Assets", "Taxonomy/Anime/Foo.md"),
            "Assets/Taxonomy/Anime/Foo"
        );
    }

    #[test]
    fn remote_url_detection() {
        assert!(is_remote_url("https://example.com/a.jpg"));
        assert!(is_remote_url("HTTP://example.com/a.jpg"));
        assert!(!is_remote_url("Assets/Anime/Foo/cover.jpg"));
        assert!(!is_remote_url("data:image/png;base64,AAAA"));
    }

    #[test]
    fn sniffs_common_image_signatures() {
        assert_eq!(
            sniff_image_ext(&[0x89, b'P', b'N', b'G', 0x0D]),
            Some("png")
        );
        assert_eq!(sniff_image_ext(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(sniff_image_ext(b"GIF89a"), Some("gif"));
        assert_eq!(sniff_image_ext(b"not an image"), None);
    }

    #[test]
    fn short_hash_is_stable_and_short() {
        let a = short_hash("https://example.com/a.jpg");
        let b = short_hash("https://example.com/a.jpg");
        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
    }
}
