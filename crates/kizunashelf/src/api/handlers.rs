use super::entities::sort_entities_for_entity_list;
use super::error::{ApiError, ApiResult};
use super::state::{content_writes_enabled, get_library, AppState};
use crate::calendar::{
    build_calendar, build_calendar_planning, CalendarBuildOptions, CalendarPlanningOptions,
    CalendarPlanningResponse, CalendarSource,
};
use crate::contract::{
    CalendarResponse, CapabilitiesResponse, ConfigResponse, HealthResponse, HomeResponse,
    HomeSectionResponse, RelationGroupsResponse, RelationListResponse, SaveSettingsRequest,
    SettingsConfigResponse,
};
use crate::dates::clamp_number;
use crate::relations::{build_relation_target_type_summaries, SortDirection};
use crate::types::{Entity, HomeSectionConfig, Library};
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;

pub(crate) async fn health(State(state): State<AppState>) -> ApiResult<HealthResponse> {
    let library = get_library(&state).await?;
    Ok(Json(HealthResponse {
        ok: true,
        generated_at: library.generated_at.clone(),
        entity_count: library.entities.len(),
        relation_count: library.relations.len(),
        diagnostic_count: library.diagnostics.len(),
        diagnostics: library.diagnostics.iter().take(20).cloned().collect(),
    }))
}

/// Drops the in-memory library cache and re-reads the vault, returning fresh
/// health. Backs an explicit pull-to-refresh that bypasses the read cache (the
/// iOS/desktop runtimes use a long cache TTL, so external edits otherwise only
/// surface after the TTL or a relaunch).
pub(crate) async fn refresh(State(state): State<AppState>) -> ApiResult<HealthResponse> {
    state.invalidate_cache().await;
    health(State(state)).await
}

pub(crate) async fn capabilities(State(state): State<AppState>) -> ApiResult<CapabilitiesResponse> {
    let library = get_library(&state).await?;
    let content_writable = content_writes_enabled(&state, &library);
    Ok(Json(CapabilitiesResponse {
        settings_writable: state.options.settings_writable,
        content_writable,
        external_search_enabled: true,
        external_apply_enabled: content_writable,
        asset_download_enabled: content_writable,
    }))
}

pub(crate) async fn config(State(state): State<AppState>) -> ApiResult<ConfigResponse> {
    let library = get_library(&state).await?;
    Ok(Json(ConfigResponse {
        taxonomy_root: library.config.taxonomy_root.clone(),
        vault_root: library.config.vault_root.clone(),
        asset_root: library.config.resolved_asset_root().to_string(),
        home: library.config.home.clone(),
        types: library.config.types.clone(),
    }))
}

pub(crate) async fn settings_config(
    State(state): State<AppState>,
) -> ApiResult<SettingsConfigResponse> {
    // iOS: app config is inline (no file); the vault config is read through the VFS.
    if let Some(app) = state.inline_app_config() {
        let vfs = state.vault_vfs(&app.vault_root);
        let vault = crate::library::load_vault_config_via_vfs(vfs.as_ref())
            .await
            .ok();
        return Ok(Json(SettingsConfigResponse {
            app_config_path: String::new(),
            app_exists: true,
            app: Some(app),
            vault_config_path: Some(crate::library::VAULT_CONFIG_RELATIVE_PATH.to_string()),
            vault_exists: vault.is_some(),
            vault,
            error: None,
        }));
    }

    let app_config_path = state.options.config_path.clone();

    // Load the local app config, if present.
    let (app_exists, app, mut error) = match crate::library::load_app_config(&app_config_path).await
    {
        Ok(app) => (true, Some(app), None),
        Err(_) if !app_config_path.exists() => (false, None, None),
        Err(error) => (true, None, Some(error.to_string())),
    };

    // Load the vault config from inside the configured vault, if a vault root is set.
    let vault_path = app.as_ref().and_then(crate::library::vault_config_path);
    let vault_config_path = vault_path.as_ref().map(|path| path.display().to_string());
    let (vault_exists, vault) = match &vault_path {
        Some(path) => match crate::library::load_vault_config(path).await {
            Ok(vault) => (true, Some(vault)),
            Err(_) if !path.exists() => (false, None),
            Err(load_error) => {
                error.get_or_insert_with(|| load_error.to_string());
                (true, None)
            }
        },
        None => (false, None),
    };

    Ok(Json(SettingsConfigResponse {
        app_config_path: app_config_path.display().to_string(),
        app_exists,
        app,
        vault_config_path,
        vault_exists,
        vault,
        error,
    }))
}

pub(crate) async fn save_settings_config(
    State(state): State<AppState>,
    Json(request): Json<SaveSettingsRequest>,
) -> ApiResult<SettingsConfigResponse> {
    if !state.options.settings_writable {
        return Err(ApiError::forbidden("Settings writes are disabled"));
    }
    let SaveSettingsRequest { app, vault } = request;

    // iOS: the app config is owned by Swift (@AppStorage) and never persisted to a
    // file; only the vault config (the schema) is written, through the VFS.
    if state.inline_app_config().is_some() {
        let vfs = state.vault_vfs(&app.vault_root);
        if let Some(vault) = &vault {
            let merged = crate::types::KizunaConfig::from_parts(app.clone(), vault.clone());
            crate::library::ensure_config_directories_via_vfs(&merged, vfs.as_ref())
                .await
                .map_err(|error| ApiError::bad_request(&error.to_string()))?;
            crate::library::save_vault_config_via_vfs(vfs.as_ref(), vault)
                .await
                .map_err(ApiError::from)?;
        }
        state.invalidate_cache().await;
        let saved = crate::library::load_vault_config_via_vfs(vfs.as_ref())
            .await
            .ok();
        return Ok(Json(SettingsConfigResponse {
            app_config_path: String::new(),
            app_exists: true,
            app: Some(app),
            vault_config_path: Some(crate::library::VAULT_CONFIG_RELATIVE_PATH.to_string()),
            vault_exists: saved.is_some(),
            vault: saved,
            error: None,
        }));
    }

    let app_config_path = state.options.config_path.clone();

    match &vault {
        Some(vault) => {
            // Full save: write both the app config and the vault config.
            let merged = crate::types::KizunaConfig::from_parts(app.clone(), vault.clone());
            crate::library::ensure_config_directories(&merged)
                .await
                .map_err(|error| ApiError::bad_request(&error.to_string()))?;
            let vault_path = crate::library::vault_config_path(&app)
                .ok_or_else(|| ApiError::bad_request("vaultRoot is required"))?;
            crate::library::save_app_config(&app_config_path, &app).await?;
            crate::library::save_vault_config(&vault_path, vault).await?;
        }
        None => {
            // App-only save (onboarding vault picker): persist the chosen vault
            // root without touching any existing, synced vault config.
            crate::library::ensure_vault_root(&app)
                .await
                .map_err(|error| ApiError::bad_request(&error.to_string()))?;
            crate::library::save_app_config(&app_config_path, &app).await?;
        }
    }

    state.invalidate_cache().await;

    // Report the vault config currently on disk so the client can decide whether
    // configuration is still needed.
    let vault_path = crate::library::vault_config_path(&app);
    let vault_config_path = vault_path.as_ref().map(|path| path.display().to_string());
    let (vault_exists, vault) = match &vault_path {
        Some(path) => match crate::library::load_vault_config(path).await {
            Ok(vault) => (true, Some(vault)),
            Err(_) => (false, None),
        },
        None => (false, None),
    };

    Ok(Json(SettingsConfigResponse {
        app_config_path: app_config_path.display().to_string(),
        app_exists: true,
        app: Some(app),
        vault_config_path,
        vault_exists,
        vault,
        error: None,
    }))
}

pub(crate) async fn home(State(state): State<AppState>) -> ApiResult<HomeResponse> {
    let library = get_library(&state).await?;
    let sections = library
        .config
        .home
        .as_ref()
        .map(|home| home.sections.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|section| build_home_section(&library, section))
        .collect::<Vec<_>>();

    Ok(Json(HomeResponse {
        generated_at: library.generated_at.clone(),
        title: library
            .config
            .home
            .as_ref()
            .and_then(|home| home.title.clone())
            .unwrap_or_else(|| "Home".to_string()),
        sections,
    }))
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct CalendarQuery {
    year: Option<f64>,
    month: Option<f64>,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    source: Option<String>,
}

pub(crate) async fn calendar(
    State(state): State<AppState>,
    Query(query): Query<CalendarQuery>,
) -> ApiResult<CalendarResponse> {
    let library = get_library(&state).await?;
    let now = chrono::Utc::now();
    let year = clamp_number(
        query
            .year
            .unwrap_or(now.format("%Y").to_string().parse().unwrap_or(1970.0)),
        1970,
        2100,
    ) as i32;
    let month = clamp_number(
        query
            .month
            .unwrap_or(now.format("%m").to_string().parse().unwrap_or(1.0)),
        1,
        12,
    ) as u32;
    let source = match query.source.as_deref() {
        Some("taxonomy") => CalendarSource::Taxonomy,
        Some("daily-note") => CalendarSource::DailyNote,
        _ => CalendarSource::All,
    };
    let vfs = state.vault_vfs(&library.config.vault_root);
    Ok(Json(
        build_calendar(
            &library,
            vfs.as_ref(),
            CalendarBuildOptions {
                year,
                month,
                entity_type: query.entity_type.filter(|item| item != "all"),
                source,
            },
        )
        .await?,
    ))
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct CalendarPlanningQuery {
    year: Option<f64>,
    #[serde(rename = "type")]
    entity_type: Option<String>,
}

pub(crate) async fn calendar_planning(
    State(state): State<AppState>,
    Query(query): Query<CalendarPlanningQuery>,
) -> ApiResult<CalendarPlanningResponse> {
    let library = get_library(&state).await?;
    let now = chrono::Utc::now();
    let year = clamp_number(
        query
            .year
            .unwrap_or(now.format("%Y").to_string().parse().unwrap_or(1970.0)),
        1970,
        2100,
    ) as i32;
    Ok(Json(build_calendar_planning(
        &library,
        CalendarPlanningOptions {
            year,
            entity_type: query.entity_type.filter(|item| item != "all"),
        },
    )))
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct RelationsQuery {
    #[serde(rename = "sourceId")]
    source_id: Option<String>,
    field: Option<String>,
}

pub(crate) async fn relations(
    State(state): State<AppState>,
    Query(query): Query<RelationsQuery>,
) -> ApiResult<RelationListResponse> {
    let library = get_library(&state).await?;
    // Filter by reference and clone only the matching relations, instead of
    // cloning the entire shared Vec up front and then discarding most of it.
    let source_id = query.source_id.as_deref();
    let field = query.field.as_deref();
    let relations: Vec<_> = library
        .relations
        .iter()
        .filter(|relation| source_id.is_none_or(|id| relation.source_id == id))
        .filter(|relation| field.is_none_or(|field| relation.field == field))
        .cloned()
        .collect();
    Ok(Json(RelationListResponse {
        total: relations.len(),
        items: relations,
    }))
}

pub(crate) async fn relation_groups(
    State(state): State<AppState>,
) -> ApiResult<RelationGroupsResponse> {
    let library = get_library(&state).await?;
    Ok(Json(RelationGroupsResponse {
        generated_at: library.generated_at.clone(),
        target_types: build_relation_target_type_summaries(&library),
    }))
}
fn build_home_section(library: &Library, section: &HomeSectionConfig) -> HomeSectionResponse {
    let entity_type = library
        .config
        .types
        .iter()
        .find(|item| item.id == section.entity_type);
    let limit = clamp_number(section.limit.unwrap_or(12) as f64, 1, 48) as u32;
    let direction = if section.direction == Some(crate::types::SortDirection::Desc) {
        SortDirection::Desc
    } else {
        SortDirection::Asc
    };
    let sort = section.sort.as_deref().unwrap_or("title");
    let mut filtered: Vec<_> = library
        .entities
        .iter()
        .filter(|entity| entity.summary.entity_type == section.entity_type)
        .filter(|entity| home_section_filters_match(entity, section))
        .map(|entity| entity.summary.clone())
        .collect();
    filtered = sort_entities_for_entity_list(filtered, sort, direction, None);
    let total = filtered.len();
    let items = filtered
        .into_iter()
        .take(limit as usize)
        .collect::<Vec<_>>();

    HomeSectionResponse {
        id: section.id.clone(),
        title: section.title.clone(),
        entity_type: section.entity_type.clone(),
        type_label: entity_type
            .map(|entity_type| entity_type.label.clone())
            .unwrap_or_else(|| section.entity_type.clone()),
        filters: section.filters.clone(),
        limit,
        sort: sort.to_string(),
        direction: if direction == SortDirection::Desc {
            "desc"
        } else {
            "asc"
        }
        .to_string(),
        total,
        items,
    }
}

fn home_section_filters_match(entity: &Entity, section: &HomeSectionConfig) -> bool {
    section.filters.iter().all(|filter| {
        let field = filter.field.trim();
        if field.is_empty() {
            return true;
        }
        let Some(value) = entity.frontmatter.get(field) else {
            return false;
        };
        if filter.values.is_empty() {
            return !value.is_null();
        }
        frontmatter_value_matches_any(value, &filter.values)
    })
}

fn frontmatter_value_matches_any(value: &serde_json::Value, expected: &[String]) -> bool {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .any(|item| frontmatter_value_matches_any(item, expected)),
        serde_json::Value::String(value) => expected.iter().any(|item| item == value),
        serde_json::Value::Bool(value) => {
            let value = if *value { "true" } else { "false" };
            expected.iter().any(|item| item == value)
        }
        serde_json::Value::Number(value) => {
            let value = value.to_string();
            expected.iter().any(|item| item == &value)
        }
        _ => false,
    }
}
