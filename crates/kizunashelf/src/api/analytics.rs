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
) -> ApiResult<CleanupQueuesResponse> {
    let library = get_library(&state).await?;
    // The memo single-flights the build, which scans the whole library and stats
    // every local cover through the VFS. See [`RevisionMemo::get_or_build`].
    let response = state
        .cleanup()
        .get_or_build(&library.content_revision, || async {
            let vfs = state.vault_vfs(&library.config.vault_root);
            let (broken_assets, broken_total) = broken_local_assets(&library, vfs.as_ref()).await;
            std::sync::Arc::new(build_cleanup_queues(&library, broken_assets, broken_total))
        })
        .await;
    Ok(Json((*response).clone()))
}
