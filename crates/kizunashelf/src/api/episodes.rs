//! Episodes write + external sync. The replace endpoint rewrites the section from
//! the client's groups; the import endpoint merges provider-fetched episodes into
//! the existing ones; the fetch endpoint lists episode sources and pulls a
//! provider's structured episodes. All writes splice only the episodes section and
//! are revision-guarded, like entity edits.

use super::entities::build_entity_detail;
use super::error::{ApiError, ApiResult};
use super::external::{
    provider_fetch_episodes, provider_for_external_ref, provider_label, provider_supports_episodes,
};
use super::mutations::{check_revision, write_entity_raw, EntityPath};
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{
    EntityDetailResponse, EpisodeSource, EpisodeSyncResponse, FetchEpisodesRequest,
    ImportEpisodesRequest, UpdateEpisodesRequest,
};
use crate::episodes::{apply_episodes, episode_section, merge_episodes, parse_episodes};
use crate::library::{file_revision, serialize_markdown_document, split_markdown_document};
use crate::types::{EntityTypeConfig, FieldType};
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde_json::{Map, Value};

pub(crate) async fn update_episodes(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<UpdateEpisodesRequest>,
) -> ApiResult<EntityDetailResponse> {
    let library = require_content_writes(&state).await?;
    let entity_id = path.id;
    let Some(record) = library.record_by_id(&entity_id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let Some(type_config) = library.config.type_config(&record.summary.entity_type) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let Some(section) = episode_section(type_config) else {
        return Err(ApiError::bad_request("This type has no episodes section"));
    };
    let section = section.clone();
    let source_rel = record.summary.path.clone();

    let vfs = state.vault_vfs(&library.config.vault_root);
    let raw = vfs
        .read_to_string(&source_rel)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {source_rel}: {error}"))?;
    // Re-check the revision against the freshly read content (TOCTOU), like entity edits.
    check_revision(&request.revision, &file_revision(&raw))?;

    let mut document = split_markdown_document(&raw);
    document.body = apply_episodes(&document.body, &section, &request.groups);
    let new_raw = serialize_markdown_document(&document.frontmatter, &document.body);
    write_entity_raw(vfs.as_ref(), &source_rel, &new_raw).await?;

    state.invalidate_cache().await;
    let reloaded = get_library(&state).await?;
    let Some(record) = reloaded.record_by_id(&entity_id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    Ok(Json(
        build_entity_detail(&state, &reloaded, &record.summary).await?,
    ))
}

/// One episode source resolved for an entity: provider id, label, and the entity's
/// stored ref value to fetch from.
struct ResolvedSource {
    provider: &'static str,
    label: String,
    ref_value: String,
}

/// The episode-capable providers for an entity: each `externalRef` field whose
/// provider supports episode import and that has a value, ordered by the type's
/// `externalPriority`.
fn episode_sources(
    state: &AppState,
    type_config: &EntityTypeConfig,
    frontmatter: &Map<String, Value>,
) -> Vec<ResolvedSource> {
    let mut sources: Vec<ResolvedSource> = Vec::new();
    for field in &type_config.fields {
        if field.field_type != FieldType::ExternalRef {
            continue;
        }
        let Some(external_ref) = &field.external_ref else {
            continue;
        };
        let Some(provider) = provider_for_external_ref(external_ref) else {
            continue;
        };
        if !provider_supports_episodes(state, provider) {
            continue;
        }
        let Some(ref_value) = frontmatter
            .get(&field.field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if sources.iter().any(|source| source.provider == provider) {
            continue;
        }
        sources.push(ResolvedSource {
            provider,
            label: provider_label(provider).unwrap_or(provider).to_string(),
            ref_value: ref_value.to_string(),
        });
    }
    let priority = |provider: &str| {
        type_config
            .external_priority
            .iter()
            .position(|item| provider_for_external_ref(item) == Some(provider))
            .unwrap_or(usize::MAX)
    };
    sources.sort_by_key(|source| priority(source.provider));
    sources
}

pub(crate) async fn fetch_episodes(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<FetchEpisodesRequest>,
) -> ApiResult<EpisodeSyncResponse> {
    let library = get_library(&state).await?;
    let Some(record) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let Some(type_config) = library.config.type_config(&record.summary.entity_type) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let sources = episode_sources(&state, type_config, &record.frontmatter);
    let source_list: Vec<EpisodeSource> = sources
        .iter()
        .map(|source| EpisodeSource {
            provider: source.provider.to_string(),
            label: source.label.clone(),
        })
        .collect();

    // Pick the requested provider, else the highest-priority source.
    let chosen = match &request.provider {
        Some(provider) => sources.iter().find(|source| source.provider == provider),
        None => sources.first(),
    };
    let Some(chosen) = chosen else {
        return Ok(Json(EpisodeSyncResponse {
            sources: source_list,
            provider: String::new(),
            groups: Vec::new(),
        }));
    };
    let episodes = provider_fetch_episodes(&state, chosen.provider, &chosen.ref_value).await?;
    Ok(Json(EpisodeSyncResponse {
        sources: source_list,
        provider: chosen.provider.to_string(),
        groups: episodes.groups,
    }))
}

pub(crate) async fn import_episodes(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<ImportEpisodesRequest>,
) -> ApiResult<EntityDetailResponse> {
    let library = require_content_writes(&state).await?;
    let entity_id = path.id;
    let Some(record) = library.record_by_id(&entity_id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let Some(type_config) = library.config.type_config(&record.summary.entity_type) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let Some(section) = episode_section(type_config) else {
        return Err(ApiError::bad_request("This type has no episodes section"));
    };
    let section = section.clone();
    let source_rel = record.summary.path.clone();

    let vfs = state.vault_vfs(&library.config.vault_root);
    let raw = vfs
        .read_to_string(&source_rel)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {source_rel}: {error}"))?;
    check_revision(&request.revision, &file_revision(&raw))?;

    let mut document = split_markdown_document(&raw);
    // Merge the incoming groups into the existing episodes (preserve watched + extras,
    // fill empty titles only), then write the merged result back.
    let existing = parse_episodes(&document.body, &section);
    let merged = merge_episodes(&existing, &request.groups);
    document.body = apply_episodes(&document.body, &section, &merged);
    let new_raw = serialize_markdown_document(&document.frontmatter, &document.body);
    write_entity_raw(vfs.as_ref(), &source_rel, &new_raw).await?;

    state.invalidate_cache().await;
    let reloaded = get_library(&state).await?;
    let Some(record) = reloaded.record_by_id(&entity_id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    Ok(Json(
        build_entity_detail(&state, &reloaded, &record.summary).await?,
    ))
}
