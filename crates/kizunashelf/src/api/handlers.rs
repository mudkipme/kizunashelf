use super::error::{ApiError, ApiResult};
use super::mutations::check_revision;
use super::state::{content_writes_enabled, get_library, AppState};
use crate::calendar::{
    build_activity, build_calendar, ActivityBuildOptions, ActivityMode, ActivityResponse,
    CalendarBuildOptions, UpcomingResponse,
};
use crate::contract::{
    CalendarResponse, CapabilitiesResponse, ConfigResponse, HealthResponse, HomeResponse,
    HomeSectionResponse, LanguagesResponse, RawConfigResponse, ResolveTypePresetsRequest,
    ResolveTypePresetsResponse, SaveRawConfigRequest, SaveSettingsRequest, SettingsConfigResponse,
    TypePresetsResponse, VaultChangesResponse,
};
use crate::dates::clamp_number;
use crate::entities::sort_entities_for_entity_list;
use crate::library::file_revision;
use crate::relations::{sort_records_by_modified, SortDirection};
use crate::types::{EntityRecord, HomeSectionConfig, Library};
use crate::vfs::Vfs;
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct TypePresetsQuery {
    /// The user's language preference (may carry a script subtag, e.g.
    /// `zh-Hant`) — picks the language of the preset display text (labels,
    /// descriptions, category headers). English when absent or untranslated.
    language: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct VaultChangesQuery {
    /// Generation returned by the previous long poll. Defaults to zero for a
    /// newly mounted client.
    #[serde(default)]
    after: u64,
}

/// The built-in **type presets** for the onboarding / settings type picker.
/// Static (no library access) — metadata only; concrete configs come from
/// [`resolve_type_presets`].
pub(crate) async fn type_presets(
    Query(query): Query<TypePresetsQuery>,
) -> Json<TypePresetsResponse> {
    Json(crate::presets::type_presets_response(
        query.language.as_deref(),
    ))
}

/// Materialize the selected presets into concrete types, merged against the
/// schema the caller currently holds. Pure and stateless (reads no vault), so
/// onboarding and the settings editor call it identically.
pub(crate) async fn resolve_type_presets(
    Json(request): Json<ResolveTypePresetsRequest>,
) -> Json<ResolveTypePresetsResponse> {
    Json(crate::presets::resolve_presets(&request))
}

/// The title-language options for the schema editor (TheTVDB's supported set)
/// and the user-language options for the clients' language picker. Static, so
/// every frontend shares one source.
pub(crate) async fn languages() -> Json<LanguagesResponse> {
    Json(LanguagesResponse {
        languages: crate::languages::supported_languages(),
        user_languages: crate::languages::user_languages(),
    })
}

pub(crate) async fn health(State(state): State<AppState>) -> ApiResult<HealthResponse> {
    let library = get_library(&state).await?;
    Ok(Json(HealthResponse {
        ok: true,
        generated_at: library.generated_at.clone(),
        entity_count: library.records.len(),
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
        vault_watch_enabled: state.vault_watch_enabled(),
        external_search_enabled: true,
        external_apply_enabled: content_writable,
        asset_download_enabled: content_writable,
    }))
}

/// Bounded long poll over the VFS watcher. Twenty-five seconds stays below
/// common reverse-proxy idle timeouts while still avoiding periodic vault scans.
pub(crate) async fn vault_changes(
    State(state): State<AppState>,
    Query(query): Query<VaultChangesQuery>,
) -> Json<VaultChangesResponse> {
    let (supported, generation, changed) = state
        .wait_for_vault_change(query.after, Duration::from_secs(25))
        .await;
    Json(VaultChangesResponse {
        supported,
        generation,
        changed,
    })
}

pub(crate) async fn config(State(state): State<AppState>) -> ApiResult<ConfigResponse> {
    let library = get_library(&state).await?;
    Ok(Json(ConfigResponse {
        taxonomy_root: library.config.taxonomy_root.clone(),
        vault_root: library.config.vault_root.clone(),
        asset_root: library.config.resolved_asset_root().to_string(),
        tags_field: library.config.tags_field().map(str::to_string),
        home: library.config.home.clone(),
        types: library.config.types.clone(),
    }))
}

pub(crate) async fn settings_config(
    State(state): State<AppState>,
) -> ApiResult<SettingsConfigResponse> {
    // The app config (vault root + write mode) is owned by the runtime and inline;
    // the vault config (the schema) is read through the VFS.
    let app = state.app_config();
    let vfs = state.vault_vfs(&app.vault_root);
    settings_config_response(app, vfs.as_ref()).await
}

async fn settings_config_response(
    app: crate::types::AppConfig,
    vfs: &dyn crate::vfs::Vfs,
) -> ApiResult<SettingsConfigResponse> {
    use crate::library::VaultConfigInspection;

    let inspection = crate::library::inspect_vault_config_via_vfs(vfs, &app)
        .await
        .map_err(ApiError::from)?;
    let raw = crate::library::read_raw_vault_config_via_vfs(vfs)
        .await
        .map_err(ApiError::from)?;
    let revision = raw.as_deref().map(file_revision);
    let (vault_exists, vault, error) = match inspection {
        VaultConfigInspection::Missing => (false, None, None),
        VaultConfigInspection::Ready(vault) => (true, Some(*vault), None),
        VaultConfigInspection::Invalid(error) => (true, None, Some(error)),
    };
    Ok(Json(SettingsConfigResponse {
        app: Some(app),
        vault_config_path: Some(crate::library::VAULT_CONFIG_RELATIVE_PATH.to_string()),
        vault_exists,
        revision,
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
    let SaveSettingsRequest { vault, revision } = request;

    // The vault root is owned by the runtime (env vars / the native vault switcher
    // / @AppStorage); the schema editor saves the vault config (the schema) only
    // and can never repoint or overwrite the vault root.
    let app = state.app_config();
    let vfs = state.vault_vfs(&app.vault_root);
    if let Some(vault) = &vault {
        let merged = crate::types::KizunaConfig::from_parts(app.clone(), vault.clone());
        let _mutation = state.content_mutation_lock().await;
        check_config_revision(vfs.as_ref(), revision.as_deref()).await?;
        crate::library::ensure_config_directories_via_vfs(&merged, vfs.as_ref())
            .await
            .map_err(|error| ApiError::bad_request(&error.to_string()))?;
        crate::library::save_vault_config_via_vfs(vfs.as_ref(), vault)
            .await
            .map_err(ApiError::from)?;
    }
    state.invalidate_cache().await;
    settings_config_response(app, vfs.as_ref()).await
}

/// Returns the raw YAML text of the vault config for the plain-text editor.
pub(crate) async fn raw_settings_config(
    State(state): State<AppState>,
) -> ApiResult<RawConfigResponse> {
    let app = state.app_config();
    let vfs = state.vault_vfs(&app.vault_root);
    let content = crate::library::read_raw_vault_config_via_vfs(vfs.as_ref())
        .await
        .map_err(ApiError::from)?;
    let revision = content.as_deref().map(file_revision);
    Ok(Json(RawConfigResponse {
        vault_config_path: crate::library::VAULT_CONFIG_RELATIVE_PATH.to_string(),
        vault_exists: content.is_some(),
        revision,
        content: content.unwrap_or_default(),
    }))
}

/// Validates and writes the raw YAML text of the vault config verbatim. The text
/// is strictly parsed first — type errors, missing required fields, invalid enum
/// values, and *any unknown field* are rejected with `400` instead of being
/// silently dropped or corrupting the on-disk schema.
pub(crate) async fn save_raw_settings_config(
    State(state): State<AppState>,
    Json(request): Json<SaveRawConfigRequest>,
) -> ApiResult<RawConfigResponse> {
    if !state.options.settings_writable {
        return Err(ApiError::forbidden("Settings writes are disabled"));
    }
    let SaveRawConfigRequest { content, revision } = request;

    let app = state.app_config();
    let vfs = state.vault_vfs(&app.vault_root);

    // Strict parse: rejects unknown keys and any malformed value before the write.
    let vault = crate::library::parse_vault_config_strict(&content)
        .map_err(|error| ApiError::bad_request(&error.to_string()))?;
    let merged = crate::types::KizunaConfig::from_parts(app, vault);
    let _mutation = state.content_mutation_lock().await;
    check_config_revision(vfs.as_ref(), revision.as_deref()).await?;
    crate::library::ensure_config_directories_via_vfs(&merged, vfs.as_ref())
        .await
        .map_err(|error| ApiError::bad_request(&error.to_string()))?;
    crate::library::save_raw_vault_config_via_vfs(vfs.as_ref(), &content)
        .await
        .map_err(ApiError::from)?;
    state.invalidate_cache().await;

    let saved = crate::library::read_raw_vault_config_via_vfs(vfs.as_ref())
        .await
        .map_err(ApiError::from)?;
    let saved_revision = saved.as_deref().map(file_revision);
    Ok(Json(RawConfigResponse {
        vault_config_path: crate::library::VAULT_CONFIG_RELATIVE_PATH.to_string(),
        vault_exists: saved.is_some(),
        revision: saved_revision,
        content: saved.unwrap_or(content),
    }))
}

async fn check_config_revision(vfs: &dyn Vfs, expected: Option<&str>) -> Result<(), ApiError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let latest = crate::library::read_raw_vault_config_via_vfs(vfs)
        .await
        .map_err(ApiError::from)?;
    let actual = latest.as_deref().map(file_revision).unwrap_or_default();
    check_revision(expected, &actual)
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct HomeQuery {
    /// Today's date (`YYYY-MM-DD`), the client's **local** date, so date-relative
    /// home-section criteria (`today() - "30d"`) are judged against the user's day
    /// rather than the host's clock. Falls back to the host's local date.
    today: Option<String>,
}

pub(crate) async fn home(
    State(state): State<AppState>,
    Query(query): Query<HomeQuery>,
) -> ApiResult<HomeResponse> {
    let library = get_library(&state).await?;
    // One evaluation context for every criteria-driven section — the same
    // engine smart lists run on, so home sections can't drift from them.
    let ctx = crate::smart_lists::EvalContext::new(
        &library,
        chrono::Utc::now(),
        super::smart_lists::resolve_today(query.today.as_deref()),
    );
    let sections = library
        .config
        .home
        .as_ref()
        .map(|home| home.sections.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|section| build_home_section(&library, section, &ctx))
        .collect::<Vec<_>>();

    Ok(Json(HomeResponse {
        generated_at: library.generated_at.clone(),
        sections,
    }))
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct CalendarQuery {
    year: Option<f64>,
    month: Option<f64>,
    #[serde(rename = "type")]
    entity_type: Option<String>,
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
    let vfs = state.vault_vfs(&library.config.vault_root);
    Ok(Json(
        build_calendar(
            &library,
            vfs.as_ref(),
            CalendarBuildOptions {
                year,
                month,
                entity_type: query.entity_type.filter(|item| item != "all"),
                include_daily_notes: true,
                // The grid places days, and a season isn't one.
                season: None,
            },
        )
        .await?,
    ))
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ActivityQuery {
    /// Opaque `YYYY-MM` cursor from the previous page.
    cursor: Option<String>,
    /// Today's date (`YYYY-MM-DD`), the client's **local** date — so `recent` /
    /// `up-next` / `catch-up` are judged against the user's day rather than a UTC
    /// server clock. Falls back to the server's UTC date.
    today: Option<String>,
    /// Target number of items per page (1–100, default 20). A page gathers whole
    /// months until it holds at least this many, so a sparse feed (one item each in
    /// scattered months) fills a single page instead of one request per month.
    limit: Option<f64>,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    /// `all` (default), `recent`, `up-next`, or `catch-up` (passed but unconsumed:
    /// planning dates still in `planning` status, and `ongoing` entities'
    /// aired-but-unwatched episodes, one item per missed air date).
    mode: Option<String>,
}

pub(crate) async fn activity(
    State(state): State<AppState>,
    Query(query): Query<ActivityQuery>,
) -> ApiResult<ActivityResponse> {
    let library = get_library(&state).await?;
    let limit = clamp_number(query.limit.unwrap_or(20.0), 1, 100) as u32;
    let mode = match query.mode.as_deref() {
        Some("recent") => ActivityMode::Recent,
        Some("up-next") => ActivityMode::UpNext,
        Some("catch-up") => ActivityMode::CatchUp,
        _ => ActivityMode::All,
    };
    let today = query
        .today
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    let vfs = state.vault_vfs(&library.config.vault_root);
    Ok(Json(
        build_activity(
            &library,
            vfs.as_ref(),
            ActivityBuildOptions {
                cursor: query.cursor.filter(|item| !item.is_empty()),
                months: 1,
                min_items: Some(limit),
                entity_type: query.entity_type.filter(|item| item != "all"),
                include_daily_notes: true,
                mode,
                today,
            },
        )
        .await?,
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpcomingQuery {
    /// Today's date (`YYYY-MM-DD`), the client's **local** date — so "upcoming" is
    /// judged against the user's day rather than a UTC server clock (matters for a
    /// reminder that fires at a local morning). Falls back to the server's UTC date.
    today: Option<String>,
    /// Horizon: how many non-empty months of upcoming items to include (1–12,
    /// default 3). Items are ascending, so the soonest come first.
    months: Option<f64>,
    #[serde(rename = "type")]
    entity_type: Option<String>,
}

/// The upcoming window: future planning/release dates + scheduled episode air
/// dates, ascending, within a horizon. Excludes daily-note mentions. Reuses the
/// up-next activity derivation (cache-driven discovery + today-reconciliation), so
/// something already released/aired today doesn't resurface. Backs the Home
/// "Coming up" section and the iOS reminder scheduler.
pub(crate) async fn upcoming(
    State(state): State<AppState>,
    Query(query): Query<UpcomingQuery>,
) -> ApiResult<UpcomingResponse> {
    let library = get_library(&state).await?;
    let months = clamp_number(query.months.unwrap_or(3.0), 1, 12) as u32;
    let today = query
        .today
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    let vfs = state.vault_vfs(&library.config.vault_root);
    let activity = build_activity(
        &library,
        vfs.as_ref(),
        ActivityBuildOptions {
            cursor: None,
            months,
            // The homepage widget keeps its month horizon, not an item target.
            min_items: None,
            entity_type: query.entity_type.filter(|item| item != "all"),
            // Date fields + scheduled episodes, never daily-note mentions: a
            // mention in a journal is a record of something, not something ahead.
            include_daily_notes: false,
            mode: ActivityMode::UpNext,
            today,
        },
    )
    .await?;
    Ok(Json(UpcomingResponse {
        generated_at: activity.generated_at,
        items: activity.items,
    }))
}

fn build_home_section(
    library: &Library,
    section: &HomeSectionConfig,
    ctx: &crate::smart_lists::EvalContext,
) -> HomeSectionResponse {
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
    // The section's criteria — the smart-list rule model. Absent criteria (or
    // structurally invalid rules, possible only in hand-edited config) degrade
    // to "no constraint" rather than failing the whole home page.
    let criteria_node = section.criteria.as_ref().map(|criteria| {
        super::smart_lists::group_to_node(criteria, None)
            .unwrap_or_else(|_| crate::smart_lists::FilterNode::empty())
    });
    let matched: Vec<&EntityRecord> = library
        .records
        .iter()
        .filter(|entity| entity.summary.entity_type == section.entity_type)
        .filter(|entity| match &criteria_node {
            Some(node) => crate::smart_lists::record_matches(node, entity, ctx),
            None => true,
        })
        .collect();
    // "recentlyUpdated" sorts on the file mtime, resident only on the record, so
    // it sorts records before mapping to summaries (mirrors `build_entity_list`).
    let filtered = if sort == "recentlyUpdated" {
        sort_records_by_modified(matched, direction, None)
            .into_iter()
            .map(|entity| entity.summary.clone())
            .collect::<Vec<_>>()
    } else {
        let summaries = matched
            .into_iter()
            .map(|entity| entity.summary.clone())
            .collect::<Vec<_>>();
        sort_entities_for_entity_list(summaries, sort, direction, None)
    };
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
        criteria: section.criteria.clone(),
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
