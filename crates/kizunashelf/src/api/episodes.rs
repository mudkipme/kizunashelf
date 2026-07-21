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
use super::mutations::{edit_entity_document, type_config_or_err, EntityPath};
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{
    EntityDetailResponse, Episode, EpisodeGroup, EpisodeSource, EpisodeSyncResponse,
    FetchEpisodesRequest, ImportEpisodesRequest, QuickAddEpisodeResult, ToggleEpisodeRequest,
};
use crate::episodes::{
    apply_episodes, episode_section, merge_episodes, parse_episodes, set_episode_watched,
};
use crate::types::{BodySection, EntityRecord, EntityTypeConfig, FieldType, Library};
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde_json::{Map, Value};

/// Checks/unchecks one episode, stamping/clearing its `✅` completion date. Only
/// the changed item is sent (group + key); the section is otherwise re-rendered
/// verbatim. Revision-guarded like the other episode writes. This is the sole
/// path for the episode checkbox — daily-note logging (`/log`) is separate and
/// never touches the episode list, so the two features share no state.
pub(crate) async fn toggle_episode(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<ToggleEpisodeRequest>,
) -> ApiResult<EntityDetailResponse> {
    let library = require_content_writes(&state).await?;
    let entity_id = path.id;
    let Some(record) = library.record_by_id(&entity_id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    let type_config = type_config_or_err(&library.config, &record.summary.entity_type)?;
    let Some(section) = episode_section(type_config) else {
        return Err(ApiError::bad_request("This type has no episodes section"));
    };
    let section = section.clone();
    let source_rel = record.summary.path.clone();

    // The completion date is always the client's local date (never a UTC server
    // clock) — so the `✅` matches the user's day and can be back-dated/edited.
    let date = request.date.trim();
    if date.is_empty() {
        return Err(ApiError::bad_request("A date is required"));
    }

    let vfs = state.vault_vfs(&library.config.vault_root);
    edit_entity_document(
        &state,
        vfs.as_ref(),
        &source_rel,
        &request.revision,
        |document| {
            let Some(body) = set_episode_watched(
                &document.body,
                &section,
                &request.group,
                &request.key,
                request.index as usize,
                request.watched,
                date,
            ) else {
                return Err(ApiError::not_found("Episode not found"));
            };
            document.body = body;
            Ok(())
        },
    )
    .await?;
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
    let type_config = type_config_or_err(&library.config, &record.summary.entity_type)?;
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
    let episodes = provider_fetch_episodes(
        &state,
        chosen.provider,
        &chosen.ref_value,
        request.language.as_deref(),
    )
    .await?;
    Ok(Json(EpisodeSyncResponse {
        sources: source_list,
        provider: chosen.provider.to_string(),
        groups: episodes.groups,
    }))
}

/// Fetches and imports episodes for a freshly-created entity from its
/// highest-priority episode source, fail-safe: any provider or write error is
/// captured into the returned result rather than propagated, so a flaky provider
/// never undoes the entity creation. Returns `None` when the type has no episodes
/// section or the entity has no episode-capable source. No revision guard (the
/// entity was just created); the caller reloads afterward.
pub(crate) async fn import_new_entity_episodes(
    state: &AppState,
    library: &Library,
    entity_id: &str,
    language: Option<&str>,
) -> Option<QuickAddEpisodeResult> {
    import_new_entity_episodes_marked(state, library, entity_id, None, language).await
}

/// Like [`import_new_entity_episodes`], but marks the first `watched_count`
/// episodes (across groups, in order) as watched — how batch import stamps a
/// source's "watched N episodes" progress onto a freshly-created entity. `None`
/// leaves every item unwatched (the quick-add behavior).
pub(crate) async fn import_new_entity_episodes_marked(
    state: &AppState,
    library: &Library,
    entity_id: &str,
    watched_count: Option<u32>,
    language: Option<&str>,
) -> Option<QuickAddEpisodeResult> {
    let record = library.record_by_id(entity_id)?;
    let type_config = library.config.type_config(&record.summary.entity_type)?;
    let section = episode_section(type_config)?.clone();
    let sources = episode_sources(state, type_config, &record.frontmatter);
    let chosen = sources.first()?;
    let provider = chosen.provider.to_string();
    match fetch_and_write_new_episodes(
        state,
        library,
        record,
        &section,
        chosen,
        watched_count,
        language,
    )
    .await
    {
        Ok(imported) => Some(QuickAddEpisodeResult {
            provider,
            imported,
            error: None,
        }),
        Err(error) => Some(QuickAddEpisodeResult {
            provider,
            imported: 0,
            error: Some(error.message().to_string()),
        }),
    }
}

/// Fetches a source's episodes and merges them into the entity body (preserving
/// any existing items; never overwriting), returning how many were imported.
/// `watched_count` marks the first N items (across groups, in order) watched.
async fn fetch_and_write_new_episodes(
    state: &AppState,
    library: &Library,
    record: &EntityRecord,
    section: &BodySection,
    chosen: &ResolvedSource,
    watched_count: Option<u32>,
    language: Option<&str>,
) -> Result<usize, ApiError> {
    let episodes =
        provider_fetch_episodes(state, chosen.provider, &chosen.ref_value, language).await?;
    let imported: usize = episodes.groups.iter().map(|group| group.items.len()).sum();

    // A provider group carries no watched/completion state; a fresh entity starts
    // every item unwatched (the same shape the client posts to `importEpisodes`),
    // except the first `watched_count` items which batch import marks watched.
    let mut remaining = watched_count.unwrap_or(0);
    let incoming: Vec<EpisodeGroup> = episodes
        .groups
        .iter()
        .map(|group| EpisodeGroup {
            label: group.label.clone(),
            items: group
                .items
                .iter()
                .map(|item| {
                    let watched = remaining > 0;
                    if watched {
                        remaining -= 1;
                    }
                    Episode {
                        key: item.key.clone(),
                        title: item.title.clone(),
                        watched,
                        date: item.date.clone(),
                        done: None,
                    }
                })
                .collect(),
        })
        .collect();

    let vfs = state.vault_vfs(&library.config.vault_root);
    edit_entity_document(
        state,
        vfs.as_ref(),
        &record.summary.path,
        &record.revision,
        |document| {
            let existing = parse_episodes(&document.body, section);
            let merged = merge_episodes(&existing, &incoming, false);
            document.body = apply_episodes(&document.body, section, &merged);
            Ok(())
        },
    )
    .await?;
    Ok(imported)
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
    let type_config = type_config_or_err(&library.config, &record.summary.entity_type)?;
    let Some(section) = episode_section(type_config) else {
        return Err(ApiError::bad_request("This type has no episodes section"));
    };
    let section = section.clone();
    let source_rel = record.summary.path.clone();

    let vfs = state.vault_vfs(&library.config.vault_root);
    edit_entity_document(
        &state,
        vfs.as_ref(),
        &source_rel,
        &request.revision,
        |document| {
            // Merge the incoming groups into the existing episodes (preserve
            // watched + extras, fill empty titles only), then write the merged
            // result back.
            let existing = parse_episodes(&document.body, &section);
            let merged = merge_episodes(&existing, &request.groups, request.overwrite);
            document.body = apply_episodes(&document.body, &section, &merged);
            Ok(())
        },
    )
    .await?;
    let reloaded = get_library(&state).await?;
    let Some(record) = reloaded.record_by_id(&entity_id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    Ok(Json(
        build_entity_detail(&state, &reloaded, &record.summary).await?,
    ))
}
