use super::error::{ApiError, ApiResult};
use super::mutations::{parent_dir, write_entity_raw, EntityPath};
use super::state::{content_writes_enabled, get_library, AppState, AssetJobRecord};
use crate::contract::{
    AssetDownloadItemResult, AssetDownloadJob, AssetDownloadJobError, AssetDownloadJobListResponse,
    AssetDownloadJobRequest, AssetDownloadJobStatus, AssetDownloadRequest, AssetDownloadResponse,
    AssetDownloadStatus,
};
use crate::library::{load_entity, serialize_markdown_document, split_markdown_document};
use crate::types::{EntityRecord, EntityTypeConfig, FieldType, Library};
use crate::vfs::{normalize_relative, Vfs};
use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::header;
use axum::response::Response;
use axum::Json;
use chrono::Utc;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const ASSET_JOB_CONCURRENCY: usize = 4;
const MAX_JOB_ERRORS: usize = 50;

const MAX_ASSET_BYTES: u64 = 25 * 1024 * 1024;
const ASSET_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// Maximum number of redirects followed (and re-validated) per asset download.
const MAX_ASSET_REDIRECTS: u32 = 5;

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
        .records
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

    let mut ctx = DownloadContext {
        client,
        vfs,
        asset_dir: &asset_dir,
        referenced: all_local,
        owned: &owned,
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
        write_entity_raw(vfs, &source_rel, &new_raw).await?;
    }

    Ok(results)
}

struct DownloadContext<'a> {
    client: &'a reqwest::Client,
    vfs: &'a dyn Vfs,
    asset_dir: &'a str,
    /// All local asset paths across the library (collision detection).
    referenced: &'a HashSet<String>,
    /// Local asset paths owned by the current entity (safe to overwrite).
    owned: &'a HashSet<String>,
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

struct DownloadedAsset {
    bytes: Vec<u8>,
    ext: String,
}

/// SSRF guard. Rejects any URL whose scheme is not http(s), and any URL that
/// resolves to a non-public address (loopback, private, link-local, CGNAT, the
/// cloud metadata IP, etc.). Every resolved address must be public — a host that
/// maps to even one internal address is refused, which blocks DNS-based attacks.
/// The 198.18.0.0/15 benchmarking range is explicitly allowed.
async fn validate_download_url(url: &reqwest::Url) -> Result<(), DownloadError> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(DownloadError::BlockedUrl),
    }
    let host = url.host_str().ok_or(DownloadError::BlockedUrl)?;
    let port = url.port_or_known_default().unwrap_or(80);
    let mut resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| DownloadError::BlockedUrl)?
        .peekable();
    if resolved.peek().is_none() {
        return Err(DownloadError::BlockedUrl);
    }
    if allow_private_asset_hosts() {
        return Ok(());
    }
    for addr in resolved {
        if is_blocked_ip(addr.ip()) {
            return Err(DownloadError::BlockedUrl);
        }
    }
    Ok(())
}

/// Opt-in escape hatch (`KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS=1`) for trusted
/// single-user setups that intentionally fetch covers from a LAN/private host.
/// Off by default so the SSRF guard is the safe default.
fn allow_private_asset_hosts() -> bool {
    std::env::var("KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

/// Whether an IP is in a range we refuse to fetch from. IPv4-mapped IPv6
/// addresses are unwrapped first so `::ffff:127.0.0.1` is treated as loopback.
fn is_blocked_ip(ip: IpAddr) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        v4 => v4,
    };
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // Explicit allowlist: 198.18.0.0/15 (benchmarking range).
            if octets[0] == 198 && (octets[1] == 18 || octets[1] == 19) {
                return false;
            }
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local() // 169.254.0.0/16, incl. the 169.254.169.254 metadata IP
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || octets[0] == 0
                || (octets[0] == 100 && (64..=127).contains(&octets[1])) // CGNAT 100.64.0.0/10
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0) // 192.0.0.0/24
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique local fc00::/7
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        }
    }
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

/// Writes bytes atomically (`.tmp` then rename) under the vault. Containment is
/// enforced by the VFS's path normalization.
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
    let tmp = format!("{relative}.tmp");
    vfs.write(&tmp, bytes)
        .await
        .map_err(|error| DownloadError::Io(error.to_string()))?;
    vfs.rename(&tmp, relative)
        .await
        .map_err(|error| DownloadError::Io(error.to_string()))?;
    Ok(())
}

// ----------------------------------------------------------------------------
// Batch download jobs
// ----------------------------------------------------------------------------

#[derive(Deserialize, JsonSchema)]
pub(crate) struct JobPath {
    id: String,
}

pub(crate) async fn create_asset_job(
    State(state): State<AppState>,
    Json(request): Json<AssetDownloadJobRequest>,
) -> ApiResult<AssetDownloadJob> {
    let library = get_library(&state).await?;
    if !content_writes_enabled(&state, &library) {
        return Err(ApiError::forbidden("Content writes are disabled"));
    }
    if let Some(entity_type) = request.entity_type.as_deref() {
        if !library
            .config
            .types
            .iter()
            .any(|item| item.id == entity_type)
        {
            return Err(ApiError::bad_request("Unknown entity type"));
        }
    }

    let mut entity_ids = Vec::new();
    for entity in &library.records {
        if let Some(entity_type) = request.entity_type.as_deref() {
            if entity.summary.entity_type != entity_type {
                continue;
            }
        }
        let Some(type_config) = library
            .config
            .types
            .iter()
            .find(|item| item.id == entity.summary.entity_type)
        else {
            continue;
        };
        if entity_has_remote_image(&entity.frontmatter, type_config) {
            entity_ids.push(entity.summary.id.clone());
        }
    }

    let scope = request
        .entity_type
        .as_deref()
        .map(|entity_type| format!("type:{entity_type}"))
        .unwrap_or_else(|| "all".to_string());
    let job = AssetDownloadJob {
        id: state.next_asset_job_id(),
        status: AssetDownloadJobStatus::Queued,
        scope,
        total: entity_ids.len() as u32,
        processed: 0,
        downloaded: 0,
        failed: 0,
        skipped: 0,
        errors: Vec::new(),
        started_at: now_iso(),
        finished_at: None,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .insert_asset_job(AssetJobRecord {
            job: job.clone(),
            cancel: Arc::clone(&cancel),
        })
        .await;

    let worker_state = state.clone();
    let job_id = job.id.clone();
    tokio::spawn(async move {
        run_asset_job(worker_state, job_id, entity_ids, cancel).await;
    });

    Ok(Json(job))
}

pub(crate) async fn list_asset_jobs(
    State(state): State<AppState>,
) -> ApiResult<AssetDownloadJobListResponse> {
    let jobs = state.asset_jobs().lock().await;
    let mut jobs: Vec<AssetDownloadJob> = jobs.values().map(|record| record.job.clone()).collect();
    jobs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(Json(AssetDownloadJobListResponse { jobs }))
}

pub(crate) async fn get_asset_job(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<JobPath>,
) -> ApiResult<AssetDownloadJob> {
    let jobs = state.asset_jobs().lock().await;
    jobs.get(&path.id)
        .map(|record| Json(record.job.clone()))
        .ok_or_else(|| ApiError::not_found("Job not found"))
}

pub(crate) async fn cancel_asset_job(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<JobPath>,
) -> ApiResult<AssetDownloadJob> {
    let mut jobs = state.asset_jobs().lock().await;
    let Some(record) = jobs.get_mut(&path.id) else {
        return Err(ApiError::not_found("Job not found"));
    };
    record.cancel.store(true, Ordering::Relaxed);
    if matches!(
        record.job.status,
        AssetDownloadJobStatus::Queued | AssetDownloadJobStatus::Running
    ) {
        record.job.status = AssetDownloadJobStatus::Cancelled;
    }
    Ok(Json(record.job.clone()))
}

async fn run_asset_job(
    state: AppState,
    job_id: String,
    entity_ids: Vec<String>,
    cancel: Arc<AtomicBool>,
) {
    state
        .update_asset_job(&job_id, |job| job.status = AssetDownloadJobStatus::Running)
        .await;

    let library = match get_library(&state).await {
        Ok(library) => library,
        Err(error) => {
            let message = error.to_string();
            state
                .update_asset_job(&job_id, |job| {
                    job.status = AssetDownloadJobStatus::Completed;
                    job.finished_at = Some(now_iso());
                    job.errors.push(AssetDownloadJobError {
                        entity_id: String::new(),
                        entity_title: String::new(),
                        message,
                    });
                })
                .await;
            return;
        }
    };

    let all_local = Arc::new(all_local_asset_paths(&library));
    let vfs = state.vault_vfs(&library.config.vault_root);
    let asset_root = Arc::new(library.config.resolved_asset_root().to_string());
    let semaphore = Arc::new(Semaphore::new(ASSET_JOB_CONCURRENCY));
    let mut tasks = JoinSet::new();

    for entity_id in entity_ids {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let task_state = state.clone();
        let library = Arc::clone(&library);
        let all_local = Arc::clone(&all_local);
        let vfs = Arc::clone(&vfs);
        let asset_root = Arc::clone(&asset_root);
        let semaphore = Arc::clone(&semaphore);
        let job_id = job_id.clone();
        let cancel = Arc::clone(&cancel);
        tasks.spawn(async move {
            let Ok(_permit) = semaphore.acquire_owned().await else {
                return;
            };
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let Some(entity) = library
                .records
                .iter()
                .find(|item| item.summary.id == entity_id)
                .cloned()
            else {
                return;
            };
            let Some(type_config) = library
                .config
                .types
                .iter()
                .find(|item| item.id == entity.summary.entity_type)
                .cloned()
            else {
                return;
            };

            let outcome = download_entity_core(
                task_state.http_client(),
                vfs.as_ref(),
                asset_root.as_str(),
                &entity,
                &type_config,
                all_local.as_ref(),
                None,
            )
            .await;

            let (downloaded, failed, skipped, error_message) = match outcome {
                Ok(items) => {
                    let downloaded = count_status(&items, AssetDownloadStatus::Downloaded);
                    let failed = count_status(&items, AssetDownloadStatus::Failed);
                    let skipped = count_status(&items, AssetDownloadStatus::Skipped);
                    let error_message = items
                        .iter()
                        .find(|item| item.status == AssetDownloadStatus::Failed)
                        .and_then(|item| item.message.clone());
                    (downloaded, failed, skipped, error_message)
                }
                Err(error) => (0, 1, 0, Some(error.message().to_string())),
            };

            task_state
                .update_asset_job(&job_id, |job| {
                    job.processed += 1;
                    job.downloaded += downloaded;
                    job.failed += failed;
                    job.skipped += skipped;
                    if let Some(message) = error_message {
                        if job.errors.len() < MAX_JOB_ERRORS {
                            job.errors.push(AssetDownloadJobError {
                                entity_id: entity.summary.id.clone(),
                                entity_title: entity.summary.title.clone(),
                                message,
                            });
                        }
                    }
                })
                .await;
        });
    }

    while tasks.join_next().await.is_some() {}

    let cancelled = cancel.load(Ordering::Relaxed);
    state
        .update_asset_job(&job_id, |job| {
            job.status = if cancelled {
                AssetDownloadJobStatus::Cancelled
            } else {
                AssetDownloadJobStatus::Completed
            };
            job.finished_at = Some(now_iso());
        })
        .await;
    state.invalidate_cache().await;
}

fn count_status(items: &[AssetDownloadItemResult], status: AssetDownloadStatus) -> u32 {
    items.iter().filter(|item| item.status == status).count() as u32
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
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

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

/// Vault-relative asset directory for an entity: `<assetRoot>/<entity path minus .md>`.
pub(super) fn entity_asset_dir(asset_root: &str, entity_relative_path: &str) -> String {
    let stem = entity_relative_path
        .strip_suffix(".md")
        .unwrap_or(entity_relative_path);
    format!("{}/{stem}", asset_root.trim_end_matches('/'))
}

/// All local (non-remote) asset paths referenced anywhere in the library, used
/// for cross-entity collision detection.
fn all_local_asset_paths(library: &Library) -> HashSet<String> {
    let mut set = HashSet::new();
    for entity in &library.records {
        if let Some(type_config) = library
            .config
            .types
            .iter()
            .find(|item| item.id == entity.summary.entity_type)
        {
            collect_local_asset_paths(&entity.frontmatter, type_config, &mut set);
        }
    }
    set
}

/// Local asset paths referenced by a single entity.
fn entity_local_asset_paths(
    frontmatter: &Map<String, Value>,
    type_config: &EntityTypeConfig,
) -> HashSet<String> {
    let mut set = HashSet::new();
    collect_local_asset_paths(frontmatter, type_config, &mut set);
    set
}

fn collect_local_asset_paths(
    frontmatter: &Map<String, Value>,
    type_config: &EntityTypeConfig,
    set: &mut HashSet<String>,
) {
    for field in &type_config.fields {
        if !matches!(field.field_type, FieldType::Image | FieldType::ImageList) {
            continue;
        }
        for value in value_to_list(frontmatter.get(&field.field)) {
            let value = value.trim();
            if !value.is_empty() && !is_remote_url(value) {
                set.insert(value.to_string());
            }
        }
    }
}

/// Whether an entity has at least one remote image URL eligible for download.
pub(super) fn entity_has_remote_image(
    frontmatter: &Map<String, Value>,
    type_config: &EntityTypeConfig,
) -> bool {
    type_config
        .fields
        .iter()
        .filter(|field| matches!(field.field_type, FieldType::Image | FieldType::ImageList))
        .any(|field| {
            value_to_list(frontmatter.get(&field.field))
                .iter()
                .any(|value| is_remote_url(value))
        })
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
    fn blocks_internal_ip_ranges() {
        use std::net::IpAddr;
        let blocked = [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "100.64.0.1",      // CGNAT
            "0.0.0.0",
            "::1",
            "::ffff:127.0.0.1", // IPv4-mapped loopback
            "fe80::1",
            "fc00::1",
        ];
        for ip in blocked {
            assert!(
                is_blocked_ip(ip.parse::<IpAddr>().unwrap()),
                "{ip} should be blocked"
            );
        }
        // Public addresses, and the explicitly allowlisted benchmarking range.
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "198.18.0.1",
            "198.19.255.255",
            "2606:4700::1",
        ] {
            assert!(
                !is_blocked_ip(ip.parse::<IpAddr>().unwrap()),
                "{ip} should be allowed"
            );
        }
    }

    #[test]
    fn short_hash_is_stable_and_short() {
        let a = short_hash("https://example.com/a.jpg");
        let b = short_hash("https://example.com/a.jpg");
        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
    }
}
