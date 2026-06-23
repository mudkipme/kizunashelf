//! Transport for the analytics/cleanup endpoints: extract request state,
//! memoize + single-flight the expensive build, and serialize. The whole-library
//! scans themselves are pure domain code in [`crate::analytics`].

use super::error::ApiResult;
use super::state::{get_library, AppState};
use crate::analytics::{broken_local_assets, build_analytics, build_cleanup_queues, build_stats};
use crate::contract::{AnalyticsResponse, CleanupQueuesResponse, StatsResponse};
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct StatsQuery {
    #[serde(rename = "type")]
    entity_type: Option<String>,
}

pub(crate) async fn stats(
    State(state): State<AppState>,
    Query(query): Query<StatsQuery>,
) -> ApiResult<StatsResponse> {
    let library = get_library(&state).await?;
    Ok(Json(build_stats(&library, query.entity_type.as_deref())))
}

pub(crate) async fn analytics(State(state): State<AppState>) -> ApiResult<AnalyticsResponse> {
    let library = get_library(&state).await?;
    if let Some(cached) = state.cached_analytics(&library.content_revision).await {
        return Ok(Json((*cached).clone()));
    }
    // Single-flight the build: concurrent first hits would otherwise each run the
    // whole-library scan. Serialize, then re-check the memo a winner may have just
    // filled before doing the work ourselves.
    let _build = state.analytics_build_lock().lock().await;
    if let Some(cached) = state.cached_analytics(&library.content_revision).await {
        return Ok(Json((*cached).clone()));
    }
    let response = std::sync::Arc::new(build_analytics(&library));
    state
        .store_analytics(&library.content_revision, std::sync::Arc::clone(&response))
        .await;
    Ok(Json((*response).clone()))
}

pub(crate) async fn cleanup_queues(
    State(state): State<AppState>,
) -> ApiResult<CleanupQueuesResponse> {
    let library = get_library(&state).await?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let (broken_assets, broken_total) = broken_local_assets(&library, vfs.as_ref()).await;
    Ok(Json(build_cleanup_queues(
        &library,
        broken_assets,
        broken_total,
    )))
}
