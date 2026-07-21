//! MyAnimeList import. Fetches a public user's anime + manga lists from the
//! official v2 API using the same client-id credential the `myanimelist`
//! provider uses. Each list entry is requested with the full node fields, so the
//! candidate is built by the provider's own mapping ([`mal_list_candidate`]) with
//! no per-item detail fetch.

use super::super::model::{ImportItem, ImportUserData, ProviderRef};
use super::ImportSource;
use crate::api::error::ApiError;
use crate::api::external::{mal_list_candidate, send_limited, MAL_LIST_FIELDS, USER_AGENT};
use crate::api::state::AppState;
use crate::contract::{ImportInput, ImportInputKind};
use crate::secrets::SECRET_MAL_CLIENT_ID;
use crate::types::CanonicalStatus;
use serde_json::Value;
use std::collections::BTreeMap;

const CREDENTIALS: &[crate::api::external::CredentialSpec] =
    &[crate::api::external::CredentialSpec {
        key: SECRET_MAL_CLIENT_ID,
        label: "MyAnimeList Client ID",
        secret: false,
        required: true,
    }];

pub(in crate::api::import) struct MyAnimeListSource;

impl ImportSource for MyAnimeListSource {
    const ID: &'static str = "myanimelist";
    const LABEL: &'static str = "MyAnimeList";
    const INPUT: ImportInputKind = ImportInputKind::Profile;
    const INPUT_LABEL: &'static str = "MyAnimeList username";

    fn credentials() -> &'static [crate::api::external::CredentialSpec] {
        CREDENTIALS
    }

    fn providers() -> &'static [&'static str] {
        &["myanimelist"]
    }

    fn available(state: &AppState) -> bool {
        client_id(state).is_some()
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        client_id(state)
            .is_none()
            .then(|| "Set the MyAnimeList client ID".to_string())
    }

    async fn fetch(state: &AppState, input: &ImportInput) -> Result<Vec<ImportItem>, ApiError> {
        let username = input
            .username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_request("A MyAnimeList username is required"))?;
        let client_id = client_id(state)
            .ok_or_else(|| ApiError::bad_request("Set the MyAnimeList client ID"))?;

        let mut items = Vec::new();
        for media_type in ["anime", "manga"] {
            fetch_list(state, &client_id, username, media_type, &mut items).await?;
        }
        Ok(items)
    }
}

fn client_id(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_MAL_CLIENT_ID)
        .filter(|value| !value.is_empty())
}

async fn fetch_list(
    state: &AppState,
    client_id: &str,
    username: &str,
    media_type: &str,
    items: &mut Vec<ImportItem>,
) -> Result<(), ApiError> {
    let client = state.http_client();
    let mut url = format!(
        "https://api.myanimelist.net/v2/users/{}/{media_type}list?fields={MAL_LIST_FIELDS}&nsfw=true&limit=1000",
        urlencoding::encode(username)
    );
    loop {
        let response = send_limited(
            client
                .get(&url)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .header("X-MAL-CLIENT-ID", client_id),
        )
        .await
        .map_err(|_| ApiError::bad_gateway("The MyAnimeList request failed"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ApiError::bad_request("MyAnimeList user not found"));
        }
        let page: Value = response
            .error_for_status()
            .map_err(|_| ApiError::bad_gateway("The MyAnimeList request failed"))?
            .json()
            .await
            .map_err(|_| ApiError::bad_gateway("Invalid MyAnimeList response"))?;

        if let Some(data) = page.get("data").and_then(Value::as_array) {
            items.extend(data.iter().filter_map(|entry| list_item(entry, media_type)));
        }
        match page
            .get("paging")
            .and_then(|paging| paging.get("next"))
            .and_then(Value::as_str)
            .filter(|next| !next.is_empty())
        {
            Some(next) => url = next.to_string(),
            None => return Ok(()),
        }
    }
}

/// Builds an item from one list entry (`{ node, list_status }`), or `None` when
/// the node has no usable candidate.
fn list_item(entry: &Value, media_type: &str) -> Option<ImportItem> {
    let node = entry.get("node")?;
    let candidate = mal_list_candidate(media_type, node)?;
    let list_status = entry.get("list_status");

    let progress_key = if media_type == "manga" {
        "num_chapters_read"
    } else {
        "num_episodes_watched"
    };
    let user = ImportUserData {
        status: list_status
            .and_then(|status| status.get("status"))
            .and_then(Value::as_str)
            .and_then(status_from),
        score10: list_status
            .and_then(|status| status.get("score"))
            .and_then(Value::as_f64)
            .filter(|score| *score > 0.0),
        watched_count: list_status
            .and_then(|status| status.get(progress_key))
            .and_then(Value::as_u64)
            .map(|count| count as u32)
            .filter(|count| *count > 0),
        started: list_status
            .and_then(|status| status.get("start_date"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        completed: list_status
            .and_then(|status| status.get("finish_date"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        notes: list_status
            .and_then(|status| status.get("comments"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    };

    // MAL's default title is romaji, not English — leave it untagged rather
    // than mislabeled `en` (wrong tags pollute language-keyed matching).
    let titles = BTreeMap::new();

    Some(ImportItem {
        refs: vec![ProviderRef {
            provider: candidate.provider.clone(),
            id: candidate.source_id.clone(),
            url: candidate.url.clone(),
        }],
        bucket: media_type.to_string(),
        title: candidate.title.clone(),
        titles,
        candidate: Some(candidate),
        user,
    })
}

/// MAL list status → canonical. `reading`/`watching` mean in progress;
/// `plan_to_watch`/`plan_to_read` mean planning.
fn status_from(value: &str) -> Option<CanonicalStatus> {
    match value {
        "completed" => Some(CanonicalStatus::Completed),
        "watching" | "reading" => Some(CanonicalStatus::Ongoing),
        "plan_to_watch" | "plan_to_read" => Some(CanonicalStatus::Planning),
        "on_hold" => Some(CanonicalStatus::Paused),
        "dropped" => Some(CanonicalStatus::Dropped),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_a_completed_anime_entry() {
        let entry = json!({
            "node": {
                "id": 1,
                "title": "Cowboy Bebop",
                "main_picture": { "large": "http://img/cb.jpg" },
                "media_type": "tv",
                "num_episodes": 26,
                "genres": [{ "id": 1, "name": "Action" }]
            },
            "list_status": {
                "status": "completed",
                "score": 9,
                "num_episodes_watched": 26,
                "start_date": "2020-01-01",
                "finish_date": "2020-02-01",
                "comments": "Classic."
            }
        });
        let item = list_item(&entry, "anime").expect("item");
        assert_eq!(item.bucket, "anime");
        assert_eq!(item.refs[0].url, "https://myanimelist.net/anime/1");
        assert_eq!(item.user.status, Some(CanonicalStatus::Completed));
        assert_eq!(item.user.score10, Some(9.0));
        assert_eq!(item.user.watched_count, Some(26));
        assert_eq!(item.user.completed.as_deref(), Some("2020-02-01"));
        // The candidate carries the provider's mapped metadata (no detail fetch).
        let candidate = item.candidate.expect("candidate");
        assert_eq!(
            candidate.metadata.get("episodes").and_then(Value::as_i64),
            Some(26)
        );
    }

    #[test]
    fn maps_manga_progress_from_chapters() {
        let entry = json!({
            "node": { "id": 2, "title": "Berserk", "media_type": "manga", "num_chapters": 0 },
            "list_status": { "status": "reading", "num_chapters_read": 370 }
        });
        let item = list_item(&entry, "manga").expect("item");
        assert_eq!(item.refs[0].url, "https://myanimelist.net/manga/2");
        assert_eq!(item.user.status, Some(CanonicalStatus::Ongoing));
        assert_eq!(item.user.watched_count, Some(370));
    }
}
