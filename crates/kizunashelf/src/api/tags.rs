//! The tags endpoint: the vault's full tag vocabulary. Pure aggregation lives in
//! [`crate::entities::all_tags`]; this just memoizes it on the library's
//! `content_revision` (the "cache for all tags") and serializes.

use super::error::ApiResult;
use super::state::{get_library, AppState};
use crate::contract::TagsResponse;
use crate::entities::all_tags;
use axum::extract::State;
use axum::Json;
use std::sync::Arc;

pub(crate) async fn tags(State(state): State<AppState>) -> ApiResult<TagsResponse> {
    let library = get_library(&state).await?;
    if let Some(cached) = state.cached_all_tags(&library.content_revision).await {
        return Ok(Json(TagsResponse {
            tags: (*cached).clone(),
        }));
    }
    let tags = Arc::new(all_tags(&library));
    state
        .store_all_tags(&library.content_revision, Arc::clone(&tags))
        .await;
    Ok(Json(TagsResponse {
        tags: (*tags).clone(),
    }))
}
