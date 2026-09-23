use super::analytics::{analytics, cleanup_queues, stats};
use super::assets::{
    cancel_asset_job, create_asset_job, download_entity_assets, get_asset_job, ingest_entity_asset,
    list_asset_jobs, plan_asset_downloads, serve_asset, upload_entity_asset,
};
use super::entities::{entities, entity_dates, entity_detail};
use super::entity_edit_review::review_entity_edit;
use super::episodes::{fetch_episodes, import_episodes, toggle_episode};
use super::error::ApiErrorLogged;
use super::external::{
    apply_external_candidate, external_provider_catalog, external_search, quick_add_entity,
    review_external_candidate,
};
use super::handlers::{
    activity, calendar, capabilities, config, health, home, languages, raw_settings_config,
    refresh, resolve_type_presets, save_raw_settings_config, save_settings_config, settings_config,
    type_presets, upcoming, vault_changes,
};
use super::import::{
    cancel_import_job, commit_import_job, create_import_job, export_import_plan, get_import_job,
    list_import_jobs, list_import_sources, restore_import_plan,
};
use super::lists::{
    add_list_item, create_list, delete_list, get_list, get_lists, remove_list_item, update_list,
};
use super::log::log_activity;
use super::mutations::{create_entity, delete_entity, update_entity};
use super::path_suggestions::path_suggestions;
use super::smart_lists::{
    create_smart_list, create_suggested_smart_lists, delete_smart_list, get_smart_list,
    preview_smart_list, set_smart_list_home, smart_list_results, smart_list_suggestions,
    update_smart_list,
};
use super::state::{ApiOptions, AppState};
use super::tags::tags;
use super::tasks::toggle_task;
use crate::calendar::{ActivityResponse, EntityDatesResponse, UpcomingResponse};
use crate::contract::ImportPlanSnapshot;
use crate::contract::{
    AnalyticsResponse, AssetDownloadJob, AssetDownloadJobListResponse, AssetDownloadPlan,
    AssetDownloadResponse, AssetIngestResponse, AssetUploadResponse, CalendarResponse,
    CapabilitiesResponse, CleanupQueuesResponse, ConfigResponse, CreateSuggestedSmartListsResponse,
    DeleteEntityResponse, DeleteListResponse, EntityDetailResponse, EntityEditReviewResponse,
    EntityListResponse, EntityMutationResponse, EpisodeSyncResponse, ErrorResponse,
    ExternalProviderCatalogResponse, ExternalReviewResponse, ExternalSearchResponse,
    HealthResponse, HomeResponse, ImportJob, ImportJobListResponse, ImportSourceCatalogResponse,
    LanguagesResponse, ListDetail, ListsResponse, LogActivityResponse, PathSuggestionsResponse,
    QuickAddResponse, RawConfigResponse, ResolveTypePresetsResponse, SettingsConfigResponse,
    SmartListDetail, SmartListSuggestionsResponse, StatsResponse, TagsResponse,
    TypePresetsResponse, VaultChangesResponse,
};
use crate::secrets::SecretStore;
use crate::types::AppConfig;
use crate::vfs::Vfs;
use aide::axum::routing::{delete_with, get_with, post_with};
use aide::axum::ApiRouter;
use aide::openapi::{Info, OpenApi};
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;
use std::time::Instant;
use tower_http::services::{ServeDir, ServeFile};
use tracing::Instrument;

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
        vault_fs,
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
    build_router(AppState::with_native_vault(
        options,
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
        .route("/api/{*path}", get(api_not_found))
        // Applied before the static-file fallback below, so it traces the API
        // routes only: a missing web asset is the browser's business, not a
        // server fault worth a log line.
        .layer(middleware::from_fn(trace_requests));

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
            "/api/vault/changes",
            get_with(vault_changes, |op| {
                op.id("getVaultChanges")
                    .response::<200, Json<VaultChangesResponse>>()
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
            "/api/type-presets",
            get_with(type_presets, |op| {
                op.id("getTypePresets")
                    .response::<200, Json<TypePresetsResponse>>()
            }),
        )
        .api_route(
            "/api/type-presets/resolve",
            post_with(resolve_type_presets, |op| {
                op.id("resolveTypePresets")
                    .response::<200, Json<ResolveTypePresetsResponse>>()
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
            "/api/upcoming",
            get_with(upcoming, |op| {
                op.id("getUpcoming")
                    .response::<200, Json<UpcomingResponse>>()
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
            "/api/external/quick-add",
            post_with(quick_add_entity, |op| {
                op.id("quickAddExternalEntity")
                    .response::<200, Json<QuickAddResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
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
            "/api/entities/{id}/edit/review",
            post_with(review_entity_edit, |op| {
                op.id("reviewEntityEdit")
                    .response::<200, Json<EntityEditReviewResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities/{id}/external/review",
            post_with(review_external_candidate, |op| {
                op.id("reviewExternalCandidate")
                    .response::<200, Json<ExternalReviewResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/entities/{id}/external/apply",
            post_with(apply_external_candidate, |op| {
                op.id("applyExternalCandidate")
                    .response::<200, Json<EntityMutationResponse>>()
                    .response::<400, Json<ErrorResponse>>()
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
            "/api/entities/{id}/tasks/toggle",
            post_with(toggle_task, |op| {
                op.id("toggleTask")
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
            "/api/import/sources",
            get_with(list_import_sources, |op| {
                op.id("listImportSources")
                    .response::<200, Json<ImportSourceCatalogResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/import-jobs",
            get_with(list_import_jobs, |op| {
                op.id("listImportJobs")
                    .response::<200, Json<ImportJobListResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .post_with(create_import_job, |op| {
                op.id("createImportJob")
                    .response::<200, Json<ImportJob>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/import-jobs/{id}",
            get_with(get_import_job, |op| {
                op.id("getImportJob")
                    .response::<200, Json<ImportJob>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/import-jobs/{id}/snapshot",
            get_with(export_import_plan, |op| {
                op.id("exportImportPlan")
                    .response::<200, Json<ImportPlanSnapshot>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/import-jobs/restore-plan",
            post_with(restore_import_plan, |op| {
                op.id("restoreImportPlan")
                    .response::<200, Json<ImportJob>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .api_route(
            "/api/import-jobs/{id}/commit",
            post_with(commit_import_job, |op| {
                op.id("commitImportJob")
                    .response::<200, Json<ImportJob>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/import-jobs/{id}/cancel",
            post_with(cancel_import_job, |op| {
                op.id("cancelImportJob")
                    .response::<200, Json<ImportJob>>()
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
            "/api/entities/{id}/assets/upload",
            post_with(upload_entity_asset, |op| {
                op.id("uploadEntityAsset")
                    .response::<200, Json<AssetUploadResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
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
        .api_route(
            "/api/smart-lists",
            post_with(create_smart_list, |op| {
                op.id("createSmartList")
                    .response::<200, Json<SmartListDetail>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/smart-list-suggestions",
            get_with(smart_list_suggestions, |op| {
                op.id("getSmartListSuggestions")
                    .response::<200, Json<SmartListSuggestionsResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .post_with(create_suggested_smart_lists, |op| {
                op.id("createSuggestedSmartLists")
                    .response::<200, Json<CreateSuggestedSmartListsResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/smart-lists/{id}/home",
            post_with(set_smart_list_home, |op| {
                op.id("setSmartListHome")
                    .response::<200, Json<SmartListDetail>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/smart-lists/preview",
            post_with(preview_smart_list, |op| {
                op.id("previewSmartList")
                    .response::<200, Json<EntityListResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/smart-lists/{id}",
            get_with(get_smart_list, |op| {
                op.id("getSmartList")
                    .response::<200, Json<SmartListDetail>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .post_with(update_smart_list, |op| {
                op.id("updateSmartList")
                    .response::<200, Json<SmartListDetail>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<409, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            })
            .delete_with(delete_smart_list, |op| {
                op.id("deleteSmartList")
                    .response::<200, Json<DeleteListResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<403, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
        .api_route(
            "/api/smart-lists/{id}/results",
            get_with(smart_list_results, |op| {
                op.id("getSmartListResults")
                    .response::<200, Json<EntityListResponse>>()
                    .response::<400, Json<ErrorResponse>>()
                    .response::<404, Json<ErrorResponse>>()
                    .response::<500, Json<ErrorResponse>>()
            }),
        )
}
/// The one place a request is traced, for all three runtimes. It opens the span
/// every other event in the request is nested under (so an `ApiError` failure
/// says which request produced it), times the round trip, and logs the failures
/// [`ApiError`] never sees.
///
/// That last part matters: every failure the app *decides* on logs its own
/// message on the way out, but a request axum rejects before a handler runs — a
/// query that didn't deserialize, a method the route doesn't take — builds no
/// `ApiError` at all, and would otherwise vanish without a trace. Those are
/// contract mismatches between a client and this router, which is exactly the
/// sort of thing that should not fail silently.
///
/// Access logging sits at `DEBUG`: a catalog rendering a page of covers produces
/// a burst of asset requests that would drown the events worth reading at `INFO`.
async fn trace_requests(request: Request, next: Next) -> Response {
    let span = tracing::info_span!(
        "request",
        method = %request.method(),
        path = %request.uri().path(),
    );
    async move {
        let started = Instant::now();
        let response = next.run(request).await;
        let status = response.status();
        let elapsed_ms = started.elapsed().as_millis();

        let handled = response.extensions().get::<ApiErrorLogged>().is_some();
        if !handled && (status.is_client_error() || status.is_server_error()) {
            if status == StatusCode::NOT_FOUND {
                tracing::debug!(elapsed_ms, "no route matched");
            } else {
                tracing::warn!(
                    status = status.as_u16(),
                    elapsed_ms,
                    "request rejected before reaching a handler",
                );
            }
        } else {
            tracing::debug!(status = status.as_u16(), elapsed_ms, "handled");
        }
        response
    }
    .instrument(span)
    .await
}

async fn api_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: "API route not found".to_string(),
        }),
    )
}
