use super::analytics::{analytics, cleanup_queues, stats};
use super::assets::{
    cancel_asset_job, create_asset_job, download_entity_assets, get_asset_job, ingest_entity_asset,
    list_asset_jobs, plan_asset_downloads, serve_asset,
};
use super::entities::{entities, entity_dates, entity_detail};
use super::episodes::{fetch_episodes, import_episodes, toggle_episode};
use super::external::{external_provider_catalog, external_search};
use super::handlers::{
    activity, calendar, capabilities, config, health, home, languages, raw_settings_config,
    refresh, save_raw_settings_config, save_settings_config, settings_config, vault_templates,
};
use super::lists::{
    add_list_item, create_list, delete_list, get_list, get_lists, remove_list_item, update_list,
};
use super::log::log_activity;
use super::mutations::{create_entity, delete_entity, update_entity};
use super::path_suggestions::path_suggestions;
use super::state::{ApiOptions, AppState};
use super::tags::tags;
use crate::calendar::{ActivityResponse, EntityDatesResponse};
use crate::contract::{
    AnalyticsResponse, AssetDownloadJob, AssetDownloadJobListResponse, AssetDownloadPlan,
    AssetDownloadResponse, AssetIngestResponse, CalendarResponse, CapabilitiesResponse,
    CleanupQueuesResponse, ConfigResponse, DeleteEntityResponse, DeleteListResponse,
    EntityDetailResponse, EntityListResponse, EntityMutationResponse, EpisodeSyncResponse,
    ErrorResponse, ExternalProviderCatalogResponse, ExternalSearchResponse, HealthResponse,
    HomeResponse, LanguagesResponse, ListDetail, ListsResponse, LogActivityResponse,
    PathSuggestionsResponse, RawConfigResponse, SettingsConfigResponse, StatsResponse,
    TagsResponse, VaultTemplatesResponse,
};
use crate::secrets::SecretStore;
use crate::types::AppConfig;
use crate::vfs::Vfs;
use aide::axum::routing::{delete_with, get_with, post_with};
use aide::axum::ApiRouter;
use aide::openapi::{Info, OpenApi};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

/// Builds the router with an injected vault filesystem and inline app config —
/// the iOS entry point. The vault config and entities are read through `vault_fs`
/// and the app config comes from `app_config` instead of a file on disk. See
/// ../kizunashelf-ios/docs/ios-port-plan.md §4/§5.
pub fn router_with_vault(
    options: ApiOptions,
    vault_fs: Arc<dyn Vfs>,
    app_config: AppConfig,
    secret_store: Arc<dyn SecretStore>,
) -> Router {
    build_router(AppState::with_vault(
        options,
        Some(vault_fs),
        app_config,
        secret_store,
    ))
}

/// Builds the router for the native runtimes (desktop / self-hosted web) with an
/// inline app config — so no app config file is required — and an injected secret
/// store. The vault filesystem is a [`NativeVfs`](crate::vfs::NativeVfs) rooted
/// at `app_config.vault_root`. Desktop passes a keyring-backed store and switches
/// vaults by rebuilding the router; web passes a
/// [`NativeSecretStore`](crate::secrets::NativeSecretStore) (env credentials + a
/// token-cache file).
pub fn router_native(
    options: ApiOptions,
    app_config: AppConfig,
    secret_store: Arc<dyn SecretStore>,
) -> Router {
    build_router(AppState::with_vault(
        options,
        None,
        app_config,
        secret_store,
    ))
}

fn build_router(state: AppState) -> Router {
    let web_dist_path = state.options.web_dist_path.clone();

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
            "/api/refresh",
            post_with(refresh, |op| {
                op.id("refreshLibrary")
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
            "/api/settings/config/raw",
            get_with(raw_settings_config, |op| {
                op.id("getRawSettingsConfig")
                    .response::<200, Json<RawConfigResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .put_with(save_raw_settings_config, |op| {
                op.id("saveRawSettingsConfig")
                    .response::<200, Json<RawConfigResponse>>()
                    .response::<400, Json<ErrorResponse>>()
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
            "/api/vault-templates",
            get_with(vault_templates, |op| {
                op.id("getVaultTemplates")
                    .response::<200, Json<VaultTemplatesResponse>>()
            }),
        )
        .api_route(
            "/api/languages",
            get_with(languages, |op| {
                op.id("getLanguages")
                    .response::<200, Json<LanguagesResponse>>()
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
            "/api/activity",
            get_with(activity, |op| {
                op.id("getActivity")
                    .response::<200, Json<ActivityResponse>>()
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
            "/api/entities/{id}/episodes/watch",
            post_with(toggle_episode, |op| {
                op.id("toggleEpisode")
                    .response::<200, Json<EntityDetailResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities/{id}/episodes/fetch",
            post_with(fetch_episodes, |op| {
                op.id("fetchEpisodes")
                    .response::<200, Json<EpisodeSyncResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities/{id}/episodes/import",
            post_with(import_episodes, |op| {
                op.id("importEpisodes")
                    .response::<200, Json<EntityDetailResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities/{id}/log",
            post_with(log_activity, |op| {
                op.id("logActivity")
                    .response::<200, Json<LogActivityResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities/{id}/assets/download",
            post_with(download_entity_assets, |op| {
                op.id("downloadEntityAssets")
                    .response::<200, Json<AssetDownloadResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .route("/api/assets/{*path}", get(serve_asset))
        .api_route(
            "/api/asset-jobs",
            get_with(list_asset_jobs, |op| {
                op.id("listAssetJobs")
                    .response::<200, Json<AssetDownloadJobListResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .post_with(create_asset_job, |op| {
                op.id("createAssetJob")
                    .response::<200, Json<AssetDownloadJob>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/asset-jobs/{id}",
            get_with(get_asset_job, |op| {
                op.id("getAssetJob")
                    .response::<200, Json<AssetDownloadJob>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/asset-jobs/{id}/cancel",
            post_with(cancel_asset_job, |op| {
                op.id("cancelAssetJob")
                    .response::<200, Json<AssetDownloadJob>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/asset-downloads/plan",
            get_with(plan_asset_downloads, |op| {
                op.id("planAssetDownloads")
                    .response::<200, Json<AssetDownloadPlan>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities/{id}/assets/ingest",
            post_with(ingest_entity_asset, |op| {
                op.id("ingestEntityAsset")
                    .response::<200, Json<AssetIngestResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/tags",
            get_with(tags, |op| {
                op.id("getTags")
                    .response::<200, Json<TagsResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/lists",
            get_with(get_lists, |op| {
                op.id("getLists")
                    .response::<200, Json<ListsResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .post_with(create_list, |op| {
                op.id("createList")
                    .response::<200, Json<ListDetail>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/lists/{id}",
            get_with(get_list, |op| {
                op.id("getList")
                    .response::<200, Json<ListDetail>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .post_with(update_list, |op| {
                op.id("updateList")
                    .response::<200, Json<ListDetail>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .delete_with(delete_list, |op| {
                op.id("deleteList")
                    .response::<200, Json<DeleteListResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/lists/{id}/items",
            post_with(add_list_item, |op| {
                op.id("addListItem")
                    .response::<200, Json<ListDetail>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/lists/{id}/items/{entityId}",
            delete_with(remove_list_item, |op| {
                op.id("removeListItem")
                    .response::<200, Json<ListDetail>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
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
