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

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CleanupQuery {
    /// The client's **local** date (`YYYY-MM-DD`), so the date-relative
    /// status-mismatch queue ("completed in the future", "missed event") is scoped
    /// to the user's today rather than UTC. Defaults to the server's UTC date.
    today: Option<String>,
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
    // The memo single-flights the whole-library scan: concurrent first hits would
    // otherwise each run it. See [`RevisionMemo::get_or_build`].
    let response = state
        .analytics()
        .get_or_build(&library.content_revision, || async {
            std::sync::Arc::new(build_analytics(&library))
        })
        .await;
    Ok(Json((*response).clone()))
}

pub(crate) async fn cleanup_queues(
    State(state): State<AppState>,
    Query(query): Query<CleanupQuery>,
) -> ApiResult<CleanupQueuesResponse> {
    let library = get_library(&state).await?;
    let today = query
        .today
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    // The memo single-flights the build, which scans the whole library and stats
    // every local cover through the VFS. The status-mismatch queue is date-relative,
    // so `today` joins the content revision in the memo key — the cached result is
    // reused within a day and recomputed when the local date rolls over.
    let memo_key = format!("{}|{today}", library.content_revision);
    let response = state
        .cleanup()
        .get_or_build(&memo_key, || async {
            let vfs = state.vault_vfs(&library.config.vault_root);
            let (broken_assets, broken_total) = broken_local_assets(&library, vfs.as_ref()).await;
            std::sync::Arc::new(build_cleanup_queues(
                &library,
                broken_assets,
                broken_total,
                &today,
            ))
        })
        .await;
    Ok(Json((*response).clone()))
}
