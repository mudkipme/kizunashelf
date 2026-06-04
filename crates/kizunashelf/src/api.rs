use crate::calendar::{
    build_calendar, build_entity_dates, CalendarBuildOptions, CalendarSource, EntityDatesResponse,
};
use crate::contract::{
    AnalyticsCoverageMetric, AnalyticsDataQuality, AnalyticsDistributions, AnalyticsRelations,
    AnalyticsResponse, AnalyticsTimeline, AnalyticsTimelineYear, AnalyticsTotals,
    AnalyticsUnresolvedRelations, CalendarResponse, CleanupQueueSummary, CleanupQueuesResponse,
    CleanupUnresolvedRelation, ConfigResponse, EntityDetailResponse, EntityListResponse,
    ErrorResponse, HealthResponse, HomeResponse, HomeSectionResponse, RelationFieldResponse,
    RelationGroupsResponse, RelationListResponse, RelationTargetGroup, RelationTargetResponse,
    StatsResponse, TypeConfigResponse, TypeCount,
};
use crate::dates::{clamp_number, date_sort_key, parse_entity_date, season_compare_value};
use crate::library::{compare_string, read_library_from_config};
use crate::relations::{
    build_relation_field_summary_with_index, build_relation_hubs, build_relation_target_summary,
    build_relation_targets, count_by, get_status_tracked_type_ids, outgoing_relations,
    relation_fields, relation_type_pairs, sort_entities, summary_by_id, target_key, Count,
    SortDirection,
};
use crate::types::{EntitySummary, HomeSectionConfig, Library};
use aide::axum::routing::get_with;
use aide::axum::ApiRouter;
use aide::openapi::{Info, OpenApi};
use aide::OperationOutput;
use anyhow::Result;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct ApiOptions {
    pub config_path: PathBuf,
    pub cache_ttl: Duration,
    pub web_dist_path: Option<PathBuf>,
}

#[derive(Clone)]
struct AppState {
    options: ApiOptions,
    cache: Arc<Mutex<Option<CachedLibrary>>>,
}

#[derive(Clone)]
struct CachedLibrary {
    library: Arc<Library>,
    cached_at: Instant,
}

pub fn router(options: ApiOptions) -> Router {
    let web_dist_path = options.web_dist_path.clone();
    let state = AppState {
        options,
        cache: Arc::new(Mutex::new(None)),
    };

    let mut api = openapi_base();
    let app = api_router()
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))
        .with_state(state)
        .finish_api(&mut api);

    if let Some(web_dist_path) = web_dist_path {
        app.fallback_service(
            ServeDir::new(&web_dist_path)
                .fallback(ServeFile::new(web_dist_path.join("index.html"))),
        )
    } else {
        app
    }
}

pub fn openapi() -> OpenApi {
    let mut api = openapi_base();
    let _ = api_router().finish_api(&mut api);
    api
}

fn openapi_base() -> OpenApi {
    OpenApi {
        info: Info {
            title: "KizunaShelf API".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            ..Info::default()
        },
        ..OpenApi::default()
    }
}

fn api_router() -> ApiRouter<AppState> {
    aide::generate::infer_responses(false);
    ApiRouter::new()
        .api_route(
            "/api/health",
            get_with(health, |op| {
                op.id("getHealth")
                    .response::<200, Json<HealthResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/config",
            get_with(config, |op| {
                op.id("getConfig")
                    .response::<200, Json<ConfigResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/home",
            get_with(home, |op| {
                op.id("getHome")
                    .response::<200, Json<HomeResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/stats",
            get_with(stats, |op| {
                op.id("getStats")
                    .response::<200, Json<StatsResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/analytics",
            get_with(analytics, |op| {
                op.id("getAnalytics")
                    .response::<200, Json<AnalyticsResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/cleanup-queues",
            get_with(cleanup_queues, |op| {
                op.id("getCleanupQueues")
                    .response::<200, Json<CleanupQueuesResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/calendar",
            get_with(calendar, |op| {
                op.id("getCalendar")
                    .response::<200, Json<CalendarResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities",
            get_with(entities, |op| {
                op.id("getEntities")
                    .response::<200, Json<EntityListResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities/{id}/dates",
            get_with(entity_dates, |op| {
                op.id("getEntityDates")
                    .response::<200, Json<EntityDatesResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities/{id}",
            get_with(entity_detail, |op| {
                op.id("getEntity")
                    .response::<200, Json<EntityDetailResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/relations",
            get_with(relations, |op| {
                op.id("getRelations")
                    .response::<200, Json<RelationListResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/relation-groups",
            get_with(relation_groups, |op| {
                op.id("getRelationGroups")
                    .response::<200, Json<RelationGroupsResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/relation-groups/{field}",
            get_with(relation_group_field, |op| {
                op.id("getRelationGroup")
                    .response::<200, Json<RelationFieldResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/relation-groups/{field}/{target}",
            get_with(relation_group_target, |op| {
                op.id("getRelationTarget")
                    .response::<200, Json<RelationTargetResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
}

async fn get_library(state: &AppState) -> Result<Arc<Library>> {
    let mut cache = state.cache.lock().await;
    if let Some(cached) = cache.as_ref() {
        if cached.cached_at.elapsed() < state.options.cache_ttl {
            return Ok(Arc::clone(&cached.library));
        }
    }
    let library = Arc::new(read_library_from_config(&state.options.config_path).await?);
    *cache = Some(CachedLibrary {
        library: Arc::clone(&library),
        cached_at: Instant::now(),
    });
    Ok(library)
}

async fn health(State(state): State<AppState>) -> ApiResult<HealthResponse> {
    let library = get_library(&state).await?;
    Ok(Json(HealthResponse {
        ok: true,
        generated_at: library.generated_at.clone(),
        entity_count: library.entities.len(),
        relation_count: library.relations.len(),
    }))
}

async fn config(State(state): State<AppState>) -> ApiResult<ConfigResponse> {
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
                title_languages: item.fields.title_languages.keys().cloned().collect(),
            })
            .collect(),
    }))
}

async fn home(State(state): State<AppState>) -> ApiResult<HomeResponse> {
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
struct StatsQuery {
    #[serde(rename = "type")]
    entity_type: Option<String>,
}

async fn stats(
    State(state): State<AppState>,
    Query(query): Query<StatsQuery>,
) -> ApiResult<StatsResponse> {
    let library = get_library(&state).await?;
    let summaries: Vec<_> = if query
        .entity_type
        .as_ref()
        .is_some_and(|entity_type| entity_type != "all")
    {
        library
            .summaries
            .iter()
            .filter(|entity| Some(&entity.entity_type) == query.entity_type.as_ref())
            .cloned()
            .collect()
    } else {
        library.summaries.clone()
    };
    let ids: std::collections::HashSet<_> =
        summaries.iter().map(|entity| entity.id.clone()).collect();
    let status_tracked_type_ids = get_status_tracked_type_ids(&library);
    let status_summaries: Vec<_> = summaries
        .iter()
        .filter(|entity| status_tracked_type_ids.contains(&entity.entity_type))
        .cloned()
        .collect();
    let mut top_relations = summaries.clone();
    top_relations.sort_by(|a, b| b.relation_count.cmp(&a.relation_count));
    top_relations.truncate(12);

    Ok(Json(StatsResponse {
        generated_at: library.generated_at.clone(),
        total: summaries.len(),
        relations: library
            .relations
            .iter()
            .filter(|relation| ids.contains(&relation.source_id))
            .count(),
        by_type: library
            .config
            .types
            .iter()
            .map(|entity_type| TypeCount {
                id: entity_type.id.clone(),
                label: entity_type.label.clone(),
                icon: entity_type.icon.clone(),
                count: library
                    .entities
                    .iter()
                    .filter(|entity| entity.summary.entity_type == entity_type.id)
                    .count(),
            })
            .collect(),
        date_fields: query
            .entity_type
            .as_ref()
            .filter(|entity_type| entity_type.as_str() != "all")
            .and_then(|entity_type| {
                library
                    .config
                    .types
                    .iter()
                    .find(|item| item.id == *entity_type)
            })
            .map(|entity_type| entity_type.fields.date.clone())
            .unwrap_or_default(),
        by_status: count_by(&status_summaries, |entity| {
            entity
                .status
                .clone()
                .unwrap_or_else(|| "Unknown".to_string())
        }),
        top_relations,
    }))
}

async fn analytics(State(state): State<AppState>) -> ApiResult<AnalyticsResponse> {
    let library = get_library(&state).await?;
    Ok(Json(build_analytics(&library)))
}

async fn cleanup_queues(State(state): State<AppState>) -> ApiResult<CleanupQueuesResponse> {
    let library = get_library(&state).await?;
    Ok(Json(build_cleanup_queues(&library)))
}

#[derive(Deserialize, JsonSchema)]
struct CalendarQuery {
    year: Option<f64>,
    month: Option<f64>,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    source: Option<String>,
}

async fn calendar(
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
struct EntitiesQuery {
    #[serde(rename = "type")]
    entity_type: Option<String>,
    status: Option<String>,
    refs: Option<String>,
    cover: Option<String>,
    sort: Option<String>,
    direction: Option<String>,
    q: Option<String>,
    relation: Option<String>,
    #[serde(rename = "pageSize")]
    page_size: Option<f64>,
    page: Option<f64>,
}

async fn entities(
    State(state): State<AppState>,
    Query(query): Query<EntitiesQuery>,
) -> ApiResult<EntityListResponse> {
    let library = get_library(&state).await?;
    let mut entities = library.summaries.clone();
    if query
        .entity_type
        .as_ref()
        .is_some_and(|entity_type| entity_type != "all")
    {
        entities.retain(|entity| Some(&entity.entity_type) == query.entity_type.as_ref());
    }
    if query.status.as_ref().is_some_and(|status| status != "all") {
        if query.status.as_deref() == Some("Unknown") {
            entities.retain(|entity| entity.status.is_none());
        } else {
            entities.retain(|entity| entity.status.as_ref() == query.status.as_ref());
        }
    }
    if query.refs.as_deref() == Some("with") {
        entities.retain(|entity| !entity.external_refs.is_empty());
    }
    if query.refs.as_deref() == Some("without") {
        entities.retain(|entity| entity.external_refs.is_empty());
    }
    if query.cover.as_deref() == Some("with") {
        entities.retain(|entity| entity.image.is_some());
    }
    if query.cover.as_deref() == Some("without") {
        entities.retain(|entity| entity.image.is_none());
    }
    if let Some(q) = query
        .q
        .as_ref()
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty())
    {
        entities.retain(|entity| {
            [
                Some(entity.title.as_str()),
                entity.subtitle.as_deref(),
                entity.summary.as_deref(),
                Some(entity.basename.as_str()),
                Some(entity.path.as_str()),
            ]
            .into_iter()
            .flatten()
            .chain(entity.titles.values().map(|value| value.as_str()))
            .any(|value| value.to_lowercase().contains(&q))
        });
    }
    if let Some(relation) = query
        .relation
        .as_ref()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
    {
        let ids: std::collections::HashSet<_> = library
            .relations
            .iter()
            .filter(|item| {
                item.target_title == relation || item.target_id.as_deref() == Some(relation)
            })
            .map(|item| item.source_id.clone())
            .collect();
        entities.retain(|entity| ids.contains(&entity.id));
    }

    let direction = if query.direction.as_deref() == Some("desc") {
        SortDirection::Desc
    } else {
        SortDirection::Asc
    };
    entities = sort_entities(entities, query.sort.as_deref().unwrap_or("type"), direction);

    let page_size = clamp_number(query.page_size.unwrap_or(40.0), 1, 100);
    let requested_page = clamp_number(query.page.unwrap_or(1.0), 1, i64::MAX);
    let total = entities.len();
    let total_pages = std::cmp::max(1, ((total as f64) / (page_size as f64)).ceil() as i64);
    let page = requested_page.min(total_pages);
    let start = ((page - 1) * page_size) as usize;
    let items = entities
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .collect();

    Ok(Json(EntityListResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    }))
}

#[derive(Deserialize, JsonSchema)]
struct EntityPath {
    id: String,
}

async fn entity_dates(
    State(state): State<AppState>,
    Path(path): Path<EntityPath>,
) -> ApiResult<EntityDatesResponse> {
    let library = get_library(&state).await?;
    let Some(entity) = library
        .entities
        .iter()
        .find(|item| item.summary.id == path.id)
    else {
        return Err(ApiError::not_found("Entity not found"));
    };
    Ok(Json(build_entity_dates(&library, entity).await?))
}

async fn entity_detail(
    State(state): State<AppState>,
    Path(path): Path<EntityPath>,
) -> ApiResult<EntityDetailResponse> {
    let library = get_library(&state).await?;
    let Some(entity) = library
        .entities
        .iter()
        .find(|item| item.summary.id == path.id)
    else {
        return Err(ApiError::not_found("Entity not found"));
    };
    Ok(Json(EntityDetailResponse {
        entity: entity.clone(),
        relations: library
            .relations
            .iter()
            .filter(|relation| relation.source_id == entity.summary.id)
            .cloned()
            .collect(),
    }))
}

#[derive(Deserialize, JsonSchema)]
struct RelationsQuery {
    #[serde(rename = "sourceId")]
    source_id: Option<String>,
    field: Option<String>,
}

async fn relations(
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

async fn relation_groups(State(state): State<AppState>) -> ApiResult<RelationGroupsResponse> {
    let library = get_library(&state).await?;
    let entity_by_id = summary_by_id(&library);
    let fields: Vec<_> = relation_fields(&library)
        .iter()
        .map(|field| build_relation_field_summary_with_index(&library, &entity_by_id, field))
        .filter(|group| group.edge_count > 0)
        .collect();
    Ok(Json(RelationGroupsResponse {
        generated_at: library.generated_at.clone(),
        fields,
    }))
}

#[derive(Deserialize, JsonSchema)]
struct RelationFieldPath {
    field: String,
}

#[derive(Deserialize, JsonSchema)]
struct RelationFieldQuery {
    q: Option<String>,
    #[serde(rename = "pageSize")]
    page_size: Option<f64>,
    page: Option<f64>,
}

async fn relation_group_field(
    State(state): State<AppState>,
    Path(path): Path<RelationFieldPath>,
    Query(query): Query<RelationFieldQuery>,
) -> ApiResult<RelationFieldResponse> {
    let library = get_library(&state).await?;
    let field = path.field;
    let q = query
        .q
        .as_ref()
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    let edge_count = outgoing_relations(&library, Some(&field)).len();
    let mut targets = build_relation_targets(&library, &field);
    let unique_targets = targets.len();
    targets = targets
        .into_iter()
        .filter(|target| {
            let Some(q) = q.as_ref() else {
                return true;
            };
            [
                Some(target.target_title.as_str()),
                target.target_id.as_deref(),
                target.target_type_label.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|value| value.to_lowercase().contains(q))
        })
        .collect::<Vec<_>>();
    let total = targets.len();
    let page_size = clamp_number(query.page_size.unwrap_or(40.0), 1, 100);
    let requested_page = clamp_number(query.page.unwrap_or(1.0), 1, i64::MAX);
    let total_pages = std::cmp::max(1, ((total as f64) / (page_size as f64)).ceil() as i64);
    let page = requested_page.min(total_pages);
    let start = ((page - 1) * page_size) as usize;
    targets = targets
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .collect();

    Ok(Json(RelationFieldResponse {
        generated_at: library.generated_at.clone(),
        field,
        edge_count,
        unique_targets,
        targets,
        total,
        page,
        page_size,
        total_pages,
    }))
}

#[derive(Deserialize, JsonSchema)]
struct RelationTargetPath {
    field: String,
    target: String,
}

async fn relation_group_target(
    State(state): State<AppState>,
    Path(path): Path<RelationTargetPath>,
) -> ApiResult<RelationTargetResponse> {
    let library = get_library(&state).await?;
    let field = path.field;
    let target = path.target;
    let relations = outgoing_relations(&library, Some(&field))
        .into_iter()
        .filter(|relation| {
            relation.target_id.as_deref() == Some(&target) || relation.target_title == target
        })
        .collect::<Vec<_>>();
    if relations.is_empty() {
        return Err(ApiError::not_found("Relation target not found"));
    }
    let target_summary =
        build_relation_target_summary(&library, target_key(&relations[0]), &relations);
    let entity_by_id = summary_by_id(&library);
    let source_entities: Vec<_> = relations
        .iter()
        .filter_map(|relation| {
            entity_by_id
                .get(relation.source_id.as_str())
                .map(|entity| (*entity).clone())
        })
        .collect();
    let groups = count_by(&source_entities, |entity| entity.type_label.clone())
        .into_iter()
        .map(|group| {
            let items = source_entities
                .iter()
                .filter(|entity| entity.type_label == group.name)
                .cloned()
                .collect::<Vec<_>>();
            RelationTargetGroup {
                type_label: group.name,
                count: group.count,
                items: sort_entities(items, "title", SortDirection::Asc),
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(RelationTargetResponse {
        generated_at: library.generated_at.clone(),
        field,
        target: target_summary,
        groups,
        total: relations.len(),
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
    filtered = sort_entities(filtered, sort, direction);
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

fn build_analytics(library: &Library) -> AnalyticsResponse {
    let summaries = &library.summaries;
    let status_tracked_type_ids = get_status_tracked_type_ids(library);
    let status_summaries: Vec<_> = summaries
        .iter()
        .filter(|entity| status_tracked_type_ids.contains(&entity.entity_type))
        .cloned()
        .collect();
    let outgoing = outgoing_relations(library, None);
    let unresolved: Vec<_> = outgoing
        .iter()
        .filter(|relation| relation.target_id.is_none())
        .map(|relation| (*relation).to_owned())
        .collect();
    let dated: Vec<_> = summaries
        .iter()
        .flat_map(|entity| {
            entity
                .dates
                .iter()
                .filter_map(|item| {
                    parse_entity_date(Some(&item.value)).map(|date| (entity.clone(), date))
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let dated_entity_ids: std::collections::HashSet<_> =
        dated.iter().map(|(entity, _)| entity.id.clone()).collect();
    let with_cover_count = summaries
        .iter()
        .filter(|entity| entity.image.is_some())
        .count();
    let with_refs_count = summaries
        .iter()
        .filter(|entity| !entity.external_refs.is_empty())
        .count();
    let with_summary_count = summaries
        .iter()
        .filter(|entity| entity.summary.is_some())
        .count();
    let connected_count = summaries
        .iter()
        .filter(|entity| entity.relation_count > 0)
        .count();

    let entity_by_id = summary_by_id(library);
    AnalyticsResponse {
        generated_at: library.generated_at.clone(),
        totals: AnalyticsTotals {
            entities: summaries.len(),
            relations: outgoing.len(),
            unresolved_relations: unresolved.len(),
            dated_entities: dated_entity_ids.len(),
            connected_entities: connected_count,
        },
        distributions: AnalyticsDistributions {
            by_type: library
                .config
                .types
                .iter()
                .map(|entity_type| TypeCount {
                    id: entity_type.id.clone(),
                    label: entity_type.label.clone(),
                    icon: entity_type.icon.clone(),
                    count: summaries
                        .iter()
                        .filter(|entity| entity.entity_type == entity_type.id)
                        .count(),
                })
                .collect(),
            by_status: count_by(&status_summaries, |entity| {
                entity
                    .status
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string())
            }),
            by_relation_field: count_by(&outgoing, |relation| relation.field.clone())
                .into_iter()
                .take(16)
                .collect::<Vec<Count>>(),
            by_source_target_type: relation_type_pairs(library).into_iter().take(16).collect(),
        },
        coverage: vec![
            build_coverage_metric("Cover", with_cover_count, summaries.len()),
            build_coverage_metric("External refs", with_refs_count, summaries.len()),
            build_coverage_metric("Summary", with_summary_count, summaries.len()),
            build_coverage_metric("Relations", connected_count, summaries.len()),
            build_coverage_metric(
                "Resolved relation targets",
                outgoing.len() - unresolved.len(),
                outgoing.len(),
            ),
        ],
        timeline: build_timeline(dated),
        relations: AnalyticsRelations {
            top_fields: relation_fields(library)
                .iter()
                .map(|field| build_relation_field_summary_with_index(library, &entity_by_id, field))
                .filter(|field| field.edge_count > 0)
                .take(12)
                .collect(),
            top_targets: build_relation_hubs(library).into_iter().take(12).collect(),
            unresolved: AnalyticsUnresolvedRelations {
                count: unresolved.len(),
                examples: unresolved.into_iter().take(12).collect(),
            },
        },
        data_quality: AnalyticsDataQuality {
            missing_cover: summaries
                .iter()
                .filter(|entity| entity.image.is_none())
                .take(12)
                .cloned()
                .collect(),
            missing_external_refs: summaries
                .iter()
                .filter(|entity| entity.external_refs.is_empty())
                .take(12)
                .cloned()
                .collect(),
            missing_summary: summaries
                .iter()
                .filter(|entity| entity.summary.is_none())
                .take(12)
                .cloned()
                .collect(),
            isolated: summaries
                .iter()
                .filter(|entity| entity.relation_count == 0)
                .take(12)
                .cloned()
                .collect(),
        },
    }
}

fn build_cleanup_queues(library: &Library) -> CleanupQueuesResponse {
    let summaries = &library.summaries;
    let source_by_id = summary_by_id(library);
    let outgoing = outgoing_relations(library, None);
    let unresolved_relations: Vec<_> = outgoing
        .iter()
        .filter(|relation| relation.target_id.is_none())
        .filter_map(|relation| {
            source_by_id
                .get(relation.source_id.as_str())
                .map(|source| CleanupUnresolvedRelation {
                    source: (*source).clone(),
                    relation: (*relation).to_owned(),
                })
        })
        .collect();
    let missing_cover: Vec<_> = summaries
        .iter()
        .filter(|entity| entity.image.is_none())
        .cloned()
        .collect();
    let missing_external_refs: Vec<_> = summaries
        .iter()
        .filter(|entity| entity.external_refs.is_empty())
        .cloned()
        .collect();
    let missing_summary: Vec<_> = summaries
        .iter()
        .filter(|entity| entity.summary.is_none())
        .cloned()
        .collect();
    let isolated: Vec<_> = summaries
        .iter()
        .filter(|entity| entity.relation_count == 0)
        .cloned()
        .collect();

    CleanupQueuesResponse {
        generated_at: library.generated_at.clone(),
        queues: vec![
            cleanup_queue_summary(
                "missing-cover",
                "Missing Cover",
                missing_cover.len(),
                summaries.len(),
            ),
            cleanup_queue_summary(
                "missing-refs",
                "Missing External Refs",
                missing_external_refs.len(),
                summaries.len(),
            ),
            cleanup_queue_summary(
                "missing-summary",
                "Missing Summary",
                missing_summary.len(),
                summaries.len(),
            ),
            cleanup_queue_summary(
                "isolated",
                "Isolated Nodes",
                isolated.len(),
                summaries.len(),
            ),
            cleanup_queue_summary(
                "unresolved-relations",
                "Unresolved Relations",
                unresolved_relations.len(),
                outgoing.len(),
            ),
        ],
        missing_cover,
        missing_external_refs,
        missing_summary,
        isolated,
        unresolved_relations,
    }
}

fn cleanup_queue_summary(
    id: &str,
    label: &str,
    remaining: usize,
    total: usize,
) -> CleanupQueueSummary {
    CleanupQueueSummary {
        id: id.to_string(),
        label: label.to_string(),
        remaining,
        total,
    }
}

fn build_coverage_metric(name: &str, count: usize, total: usize) -> AnalyticsCoverageMetric {
    AnalyticsCoverageMetric {
        name: name.to_string(),
        count,
        missing: total - count,
        total,
        percent: if total == 0 {
            0
        } else {
            ((count as f64 / total as f64) * 100.0).round() as i64
        },
    }
}

fn build_timeline(
    dated: Vec<(EntitySummary, crate::dates::ParsedEntityDate)>,
) -> AnalyticsTimeline {
    let mut by_year: HashMap<i32, Vec<EntitySummary>> = HashMap::new();
    let mut by_season: HashMap<String, (i32, String, Vec<EntitySummary>)> = HashMap::new();
    let mut by_month: HashMap<String, Vec<EntitySummary>> = HashMap::new();
    for (entity, date) in &dated {
        by_year.entry(date.year).or_default().push(entity.clone());
        if let Some(season) = &date.season {
            by_season
                .entry(format!("{} {}", date.year, season))
                .or_insert_with(|| (date.year, season.clone(), Vec::new()))
                .2
                .push(entity.clone());
        }
        if let Some(month) = date.month {
            by_month
                .entry(format!("{}-{month:02}", date.year))
                .or_default()
                .push(entity.clone());
        }
    }
    let mut years: Vec<_> = by_year.into_iter().collect();
    years.sort_by(|a, b| b.0.cmp(&a.0));
    let years = years
        .into_iter()
        .map(|(year, mut entities)| {
            let by_type = count_by(&entities, |entity| entity.type_label.clone());
            entities.sort_by(|a, b| {
                compare_string(
                    date_sort_key(b.dates.first().map(|item| item.value.as_str()))
                        .as_deref()
                        .unwrap_or_default(),
                    date_sort_key(a.dates.first().map(|item| item.value.as_str()))
                        .as_deref()
                        .unwrap_or_default(),
                )
            });
            AnalyticsTimelineYear {
                year,
                count: entities.len(),
                by_type,
                examples: unique_entities_by_id(entities)
                    .into_iter()
                    .take(6)
                    .collect(),
            }
        })
        .collect::<Vec<_>>();

    let mut seasons: Vec<_> = by_season.into_iter().collect();
    seasons.sort_by(|(_, a), (_, b)| {
        b.0.cmp(&a.0)
            .then_with(|| season_compare_value(&b.1).cmp(&season_compare_value(&a.1)))
    });
    let seasons = seasons
        .into_iter()
        .take(12)
        .map(|(name, (_, _, entities))| Count {
            name,
            count: entities.len(),
        })
        .collect::<Vec<_>>();

    let mut months: Vec<_> = by_month.into_iter().collect();
    months.sort_by(|a, b| compare_string(&b.0, &a.0));
    let months = months
        .into_iter()
        .take(18)
        .map(|(name, entities)| Count {
            name,
            count: entities.len(),
        })
        .collect::<Vec<_>>();

    AnalyticsTimeline {
        total_dated: dated.len(),
        years,
        seasons,
        months,
    }
}

fn unique_entities_by_id(entities: Vec<EntitySummary>) -> Vec<EntitySummary> {
    let mut seen = HashSet::new();
    entities
        .into_iter()
        .filter(|entity| seen.insert(entity.id.clone()))
        .collect()
}

type ApiResult<T> = Result<Json<T>, ApiError>;

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.to_string(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

impl OperationOutput for ApiError {
    type Inner = ErrorResponse;

    fn operation_response(
        ctx: &mut aide::generate::GenContext,
        operation: &mut aide::openapi::Operation,
    ) -> Option<aide::openapi::Response> {
        <Json<ErrorResponse> as OperationOutput>::operation_response(ctx, operation)
    }
}
