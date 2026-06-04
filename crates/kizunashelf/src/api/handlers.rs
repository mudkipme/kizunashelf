use super::entities::sort_entities_for_entity_list;
use super::error::{ApiError, ApiResult};
use super::state::{get_library, AppState};
use crate::calendar::{build_calendar, CalendarBuildOptions, CalendarSource};
use crate::contract::{
    CalendarResponse, ConfigResponse, HealthResponse, HomeResponse, HomeSectionResponse,
    RelationGroupsResponse, RelationListResponse, SettingsConfigResponse, TypeConfigResponse,
};
use crate::dates::clamp_number;
use crate::library::effective_default_title_language;
use crate::relations::{build_relation_target_type_summaries, SortDirection};
use crate::types::{HomeSectionConfig, Library};
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

pub(crate) async fn config(State(state): State<AppState>) -> ApiResult<ConfigResponse> {
    let library = get_library(&state).await?;
    Ok(Json(ConfigResponse {
        taxonomy_root: library.config.taxonomy_root.clone(),
        home: library.config.home.clone(),
        types: library
            .config
            .types
            .iter()
            .map(|item| TypeConfigResponse {
                id: item.id.clone(),
                label: item.label.clone(),
                icon: item.icon.clone(),
                path: item.path.clone(),
                default_title_language: effective_default_title_language(item),
                title_languages: item.fields.title_languages.keys().cloned().collect(),
                status_fields: item.fields.status.clone(),
                date_roles: item.fields.date_roles.clone(),
            })
            .collect(),
    }))
}

pub(crate) async fn settings_config(
    State(state): State<AppState>,
) -> ApiResult<SettingsConfigResponse> {
    let config_path = state.options.config_path.clone();
    match crate::library::load_config(&config_path).await {
        Ok(config) => Ok(Json(SettingsConfigResponse {
            config_path: config_path.display().to_string(),
            exists: true,
            config: Some(config),
            error: None,
        })),
        Err(_) if !config_path.exists() => Ok(Json(SettingsConfigResponse {
            config_path: config_path.display().to_string(),
            exists: false,
            config: None,
            error: None,
        })),
        Err(error) => Ok(Json(SettingsConfigResponse {
            config_path: config_path.display().to_string(),
            exists: true,
            config: None,
            error: Some(error.to_string()),
        })),
    }
}

pub(crate) async fn save_settings_config(
    State(state): State<AppState>,
    Json(config): Json<crate::types::KizunaConfig>,
) -> ApiResult<SettingsConfigResponse> {
    if !state.options.settings_writable {
        return Err(ApiError::forbidden("Settings writes are disabled"));
    }
    let config_path = state.options.config_path.clone();
    crate::library::save_config(&config_path, &config).await?;
    state.invalidate_cache().await;
    Ok(Json(SettingsConfigResponse {
        config_path: config_path.display().to_string(),
        exists: true,
        config: Some(config),
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
    Ok(Json(
        build_calendar(
            &library,
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
    let mut relations = library.relations.clone();
    if let Some(source_id) = query.source_id {
        relations.retain(|relation| relation.source_id == source_id);
    }
    if let Some(field) = query.field {
        relations.retain(|relation| relation.field == field);
    }
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
    let statuses = normalize_statuses(section);
    let limit = clamp_number(section.limit.unwrap_or(12) as f64, 1, 48) as u32;
    let direction = if section.direction == Some(crate::types::SortDirection::Desc) {
        SortDirection::Desc
    } else {
        SortDirection::Asc
    };
    let sort = section.sort.as_deref().unwrap_or("title");
    let mut filtered: Vec<_> = library
        .summaries
        .iter()
        .filter(|entity| {
            if entity.entity_type != section.entity_type {
                return false;
            }
            statuses.is_empty()
                || statuses
                    .iter()
                    .any(|status| status == entity.status.as_deref().unwrap_or("Unknown"))
        })
        .cloned()
        .collect();
    filtered = sort_entities_for_entity_list(library, filtered, sort, direction, None);
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
        status: statuses,
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

fn normalize_statuses(section: &HomeSectionConfig) -> Vec<String> {
    match &section.status {
        None => Vec::new(),
        Some(crate::types::StatusConfig::One(value)) if value.is_empty() => Vec::new(),
        Some(crate::types::StatusConfig::One(value)) => vec![value.clone()],
        Some(crate::types::StatusConfig::Many(values)) => values
            .iter()
            .filter(|value| !value.is_empty())
            .cloned()
            .collect(),
    }
}
