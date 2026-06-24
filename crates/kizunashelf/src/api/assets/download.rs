//! The asset download engine: fetch remote image fields for one entity, write
//! them under the entity's asset directory, and rewrite/persist its frontmatter.
//! Shared by the single-entity endpoint and the batch worker.

use super::ssrf::validate_download_url;
use super::util::{
    entity_asset_dir, entity_local_asset_paths, extension_for_content_type, extension_from_url,
    is_remote_url, short_hash, skip_reason, sniff_image_ext, value_to_list,
};
use crate::api::error::ApiError;
use crate::api::mutations::{parent_dir, write_entity_raw};
use crate::contract::{AssetDownloadItemResult, AssetDownloadStatus};
use crate::library::{serialize_markdown_document, split_markdown_document};
use crate::types::{EntityRecord, EntityTypeConfig, FieldType};
use crate::vfs::Vfs;
use axum::http::header;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

const MAX_ASSET_BYTES: u64 = 25 * 1024 * 1024;
const ASSET_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// Maximum number of redirects followed (and re-validated) per asset download.
const MAX_ASSET_REDIRECTS: u32 = 5;

/// Downloads remote image fields for one entity, rewriting and persisting its
/// frontmatter. Shared by the single-entity endpoint and the batch worker. Does
/// not touch the library cache; the caller decides when to invalidate.
pub(super) async fn download_entity_core(
    client: &reqwest::Client,
    vfs: &dyn Vfs,
    asset_root: &str,
    entity: &EntityRecord,
    type_config: &EntityTypeConfig,
    all_local: &HashSet<String>,
    fields_filter: Option<&[String]>,
) -> Result<Vec<AssetDownloadItemResult>, ApiError> {
    let fields = image_fields(type_config, fields_filter);
    if fields.is_empty() {
        return Ok(Vec::new());
    }
    let asset_dir = entity_asset_dir(asset_root, &entity.summary.path);
    let owned = entity_local_asset_paths(&entity.frontmatter, type_config);

    let source_rel = entity.summary.path.clone();
    let raw = vfs
        .read_to_string(&source_rel)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {source_rel}: {error}"))?;
    let mut document = split_markdown_document(&raw);

    let ctx = DownloadContext {
        vfs,
        asset_dir: &asset_dir,
        referenced: all_local,
        owned: &owned,
        entity_id: &entity.summary.id,
    };

    let mut results = Vec::new();
    let mut changed = false;
    for field in fields {
        let field_changed = process_field(
            client,
            &ctx,
            &mut document.frontmatter,
            &field,
            &mut results,
        )
        .await?;
        changed = changed || field_changed;
    }

    if changed {
        let new_raw = serialize_markdown_document(&document.frontmatter, &document.body);
        write_entity_raw(vfs, &source_rel, &new_raw).await?;
    }

    Ok(results)
}

pub(super) struct DownloadContext<'a> {
    pub(super) vfs: &'a dyn Vfs,
    pub(super) asset_dir: &'a str,
    /// All local asset paths across the library (collision detection).
    pub(super) referenced: &'a HashSet<String>,
    /// Local asset paths owned by the current entity (safe to overwrite).
    pub(super) owned: &'a HashSet<String>,
    pub(super) entity_id: &'a str,
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
    client: &reqwest::Client,
    ctx: &DownloadContext<'_>,
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
            match download_to_asset(client, ctx, &field.name, Some(url), url).await {
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
        match download_to_asset(client, ctx, &field.name, None, &url).await {
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
    client: &reqwest::Client,
    ctx: &DownloadContext<'_>,
    field_name: &str,
    list_key: Option<&str>,
    url: &str,
) -> Result<AssetOutcome, DownloadError> {
    let asset = download_one(client, url).await?;
    place_asset(ctx, field_name, list_key, asset).await
}

/// Writes an already-fetched asset under the entity's asset directory, applying
/// the cross-entity collision rule. Shared by the reqwest path and the ingest
/// path (where an iOS client fetched the bytes via a background URLSession).
async fn place_asset(
    ctx: &DownloadContext<'_>,
    field_name: &str,
    list_key: Option<&str>,
    asset: DownloadedAsset,
) -> Result<AssetOutcome, DownloadError> {
    // Compute the intended vault-relative destination.
    let (dir, stem) = match list_key {
        Some(key) => (format!("{}/{}", ctx.asset_dir, field_name), short_hash(key)),
        None => (ctx.asset_dir.to_string(), field_name.to_string()),
    };
    let mut relative = format!("{dir}/{stem}.{}", asset.ext);
    let mut conflict_resolved = false;

    // Never overwrite a file referenced by another entity (a path is safe if it
    // is unreferenced or owned by this entity).
    if ctx.referenced.contains(&relative) && !ctx.owned.contains(&relative) {
        let disambiguated = format!("{dir}/{stem}-{}.{}", short_hash(ctx.entity_id), asset.ext);
        if ctx.referenced.contains(&disambiguated) && !ctx.owned.contains(&disambiguated) {
            return Err(DownloadError::Collision);
        }
        relative = disambiguated;
        conflict_resolved = true;
    }

    write_asset_file(ctx.vfs, &relative, &asset.bytes).await?;
    Ok(AssetOutcome {
        path: relative,
        conflict_resolved,
    })
}

/// Identifies the single image field an ingest targets.
pub(super) struct IngestField<'a> {
    pub(super) name: &'a str,
    pub(super) is_list: bool,
    /// Stable key for list-field filenames (the element URL); `None` for single
    /// image fields.
    pub(super) list_key: Option<&'a str>,
    /// The remote URL the host downloaded, used for the idempotency guard.
    pub(super) source_url: &'a str,
}

/// Ingests externally-downloaded bytes for ONE field: validates + places the file
/// and rewrites that field in `frontmatter`. Honors an idempotency guard so a
/// replayed or out-of-order ingest won't clobber a value the user has since
/// changed. Returns whether the frontmatter changed and the per-field result.
pub(super) async fn ingest_field_bytes(
    ctx: &DownloadContext<'_>,
    frontmatter: &mut Map<String, Value>,
    field: &IngestField<'_>,
    bytes: Vec<u8>,
    content_type: &str,
) -> (bool, AssetDownloadItemResult) {
    let source_url = field.source_url;
    let still_present = if field.is_list {
        value_to_list(frontmatter.get(field.name))
            .iter()
            .any(|value| value.trim() == source_url)
    } else {
        frontmatter
            .get(field.name)
            .and_then(Value::as_str)
            .is_some_and(|value| value.trim() == source_url)
    };
    if !still_present {
        return (
            false,
            skipped(field.name, source_url, "Source URL is no longer present"),
        );
    }

    let asset = match process_downloaded_bytes(bytes, content_type, source_url) {
        Ok(asset) => asset,
        Err(error) => return (false, failed(field.name, source_url, &error.to_string())),
    };
    let outcome = match place_asset(ctx, field.name, field.list_key, asset).await {
        Ok(outcome) => outcome,
        Err(error) => return (false, failed(field.name, source_url, &error.to_string())),
    };

    if field.is_list {
        let rewritten: Vec<Value> = value_to_list(frontmatter.get(field.name))
            .into_iter()
            .map(|element| {
                if element.trim() == source_url {
                    Value::String(outcome.path.clone())
                } else {
                    Value::String(element)
                }
            })
            .collect();
        frontmatter.insert(field.name.to_string(), Value::Array(rewritten));
    } else {
        frontmatter.insert(field.name.to_string(), Value::String(outcome.path.clone()));
    }
    let result = downloaded(field.name, source_url, &outcome);
    (true, result)
}

struct DownloadedAsset {
    bytes: Vec<u8>,
    ext: String,
}

async fn download_one(
    client: &reqwest::Client,
    url: &str,
) -> Result<DownloadedAsset, DownloadError> {
    // The shared asset client has redirects disabled (see AppState) so we can
    // re-run the SSRF guard against every hop. Following redirects manually is
    // what closes the redirect-into-internal-host bypass.
    let mut current = reqwest::Url::parse(url).map_err(|_| DownloadError::BlockedUrl)?;
    let mut redirects = 0;
    let response = loop {
        validate_download_url(&current).await?;
        let response = client
            .get(current.clone())
            .timeout(ASSET_DOWNLOAD_TIMEOUT)
            .send()
            .await
            .map_err(|error| DownloadError::Request(error.to_string()))?;
        if response.status().is_redirection() {
            redirects += 1;
            if redirects > MAX_ASSET_REDIRECTS {
                return Err(DownloadError::TooManyRedirects);
            }
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or(DownloadError::BlockedUrl)?;
            // Resolve relative redirects against the current URL, then loop to
            // re-validate the new destination before fetching it.
            current = current
                .join(location)
                .map_err(|_| DownloadError::BlockedUrl)?;
            continue;
        }
        break response;
    };
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
        .unwrap_or_default()
        .to_string();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| DownloadError::Request(error.to_string()))?;
    process_downloaded_bytes(bytes.to_vec(), &content_type, url)
}

/// Validates already-downloaded bytes are an image within the size limit and
/// resolves the file extension. Shared by the reqwest path and the ingest path
/// (where an iOS client fetched the bytes via a background URLSession).
fn process_downloaded_bytes(
    bytes: Vec<u8>,
    content_type: &str,
    url: &str,
) -> Result<DownloadedAsset, DownloadError> {
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err(DownloadError::TooLarge);
    }
    let content_type = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .to_ascii_lowercase();
    let sniffed = sniff_image_ext(&bytes);
    if !content_type.starts_with("image/") && sniffed.is_none() {
        return Err(DownloadError::NotAnImage);
    }
    let ext = extension_for_content_type(&content_type)
        .map(str::to_string)
        .or_else(|| sniffed.map(str::to_string))
        .or_else(|| extension_from_url(url))
        .unwrap_or_else(|| "img".to_string());
    Ok(DownloadedAsset { bytes, ext })
}

/// Writes bytes atomically under the vault. Containment and the backend-specific
/// replacement mechanism are owned by the VFS.
async fn write_asset_file(
    vfs: &dyn Vfs,
    relative: &str,
    bytes: &[u8],
) -> Result<(), DownloadError> {
    if let Some(parent) = parent_dir(relative) {
        vfs.create_dir_all(parent)
            .await
            .map_err(|error| DownloadError::Io(error.to_string()))?;
    }
    vfs.write_atomic(relative, bytes)
        .await
        .map_err(|error| DownloadError::Io(error.to_string()))?;
    Ok(())
}

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
pub(super) enum DownloadError {
    Request(String),
    Status(u16),
    TooLarge,
    NotAnImage,
    Collision,
    Io(String),
    BlockedUrl,
    TooManyRedirects,
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
            DownloadError::BlockedUrl => {
                write!(f, "Source URL is not an allowed public address")
            }
            DownloadError::TooManyRedirects => write!(f, "Source URL redirected too many times"),
        }
    }
}
