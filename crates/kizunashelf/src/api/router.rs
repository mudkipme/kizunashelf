use super::analytics::{analytics, cleanup_queues, stats};
use super::entities::{entities, entity_dates, entity_detail};
use super::external::{external_provider_catalog, external_search};
use super::handlers::{
    calendar, calendar_planning, capabilities, config, health, home, relation_groups, relations,
    save_settings_config, settings_config,
};
use super::mutations::{create_entity, delete_entity, update_entity};
use super::path_suggestions::path_suggestions;
use super::state::{ApiOptions, AppState};
use crate::calendar::{CalendarPlanningResponse, EntityDatesResponse};
use crate::contract::{
    AnalyticsResponse, CalendarResponse, CapabilitiesResponse, CleanupQueuesResponse,
    ConfigResponse, DeleteEntityResponse, EntityDetailResponse, EntityListResponse,
    EntityMutationResponse, ErrorResponse, ExternalProviderCatalogResponse, ExternalSearchResponse,
    HealthResponse, HomeResponse, PathSuggestionsResponse, RelationGroupsResponse,
    RelationListResponse, SettingsConfigResponse, StatsResponse,
};
use aide::axum::routing::get_with;
use aide::axum::ApiRouter;
use aide::openapi::{Info, OpenApi};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use tower_http::services::{ServeDir, ServeFile};

pub fn router(options: ApiOptions) -> Router {
    let web_dist_path = options.web_dist_path.clone();
    let state = AppState::new(options);

    let mut api = openapi_base();
    let app = api_router()
        .with_state(state)
        .finish_api(&mut api)
        .route("/api/{*path}", get(api_not_found));

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
            "/api/capabilities",
            get_with(capabilities, |op| {
                op.id("getCapabilities")
                    .response::<200, Json<CapabilitiesResponse>>()
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
            "/api/settings/config",
            get_with(settings_config, |op| {
                op.id("getSettingsConfig")
                    .response::<200, Json<SettingsConfigResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .put_with(save_settings_config, |op| {
                op.id("saveSettingsConfig")
                    .response::<200, Json<SettingsConfigResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/settings/path-suggestions",
            get_with(path_suggestions, |op| {
                op.id("getPathSuggestions")
                    .response::<200, Json<PathSuggestionsResponse>>()
                    .response::<403, Json<ErrorResponse>>()
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
            "/api/calendar/planning",
            get_with(calendar_planning, |op| {
                op.id("getCalendarPlanning")
                    .response::<200, Json<CalendarPlanningResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/external/providers",
            get_with(external_provider_catalog, |op| {
                op.id("getExternalProviderCatalog")
                    .response::<200, Json<ExternalProviderCatalogResponse>>()
            }),
        )
        .api_route(
            "/api/external/search",
            get_with(external_search, |op| {
                op.id("searchExternalSources")
                    .response::<200, Json<ExternalSearchResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities",
            get_with(entities, |op| {
                op.id("getEntities")
                    .response::<200, Json<EntityListResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .post_with(create_entity, |op| {
                op.id("createEntity")
                    .response::<200, Json<EntityMutationResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
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
            })
            .post_with(update_entity, |op| {
                op.id("updateEntity")
                    .response::<200, Json<EntityMutationResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .delete_with(delete_entity, |op| {
                op.id("deleteEntity")
                    .response::<200, Json<DeleteEntityResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
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
}
async fn api_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: "API route not found".to_string(),
        }),
    )
}
