//! Trakt import. Reads a public profile's watched, watchlist, and ratings for
//! movies and shows using a Trakt client id (its `trakt-api-key`; no OAuth for
//! public data). Items key to the `tmdb` provider by `ids.tmdb` (entries with no
//! TMDB id go to review). Candidates are minimal and detail-fetched from TMDB at
//! commit. The same work across lists is merged by the in-batch dedup (watched →
//! ongoing/completed, watchlist → planning, ratings → score).

use super::super::model::{ImportItem, ImportUserData, ProviderRef};
use super::ImportSource;
use crate::api::error::ApiError;
use crate::api::external::{CredentialSpec, USER_AGENT};
use crate::api::state::AppState;
use crate::contract::{ExternalCandidate, ImportInput, ImportInputKind};
use crate::secrets::SECRET_TRAKT_CLIENT_ID;
use crate::types::CanonicalStatus;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const BASE: &str = "https://api.trakt.tv";

const CREDENTIALS: &[CredentialSpec] = &[CredentialSpec {
    key: SECRET_TRAKT_CLIENT_ID,
    label: "Trakt Client ID",
    secret: false,
    required: true,
}];

pub(in crate::api::import) struct TraktSource;

impl ImportSource for TraktSource {
    const ID: &'static str = "trakt";
    const LABEL: &'static str = "Trakt";
    const INPUT: ImportInputKind = ImportInputKind::Profile;
    const INPUT_LABEL: &'static str = "Trakt username (slug)";

    fn credentials() -> &'static [CredentialSpec] {
        CREDENTIALS
    }

    fn providers() -> &'static [&'static str] {
        &["tmdb"]
    }

    fn available(state: &AppState) -> bool {
        client_id(state).is_some()
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        client_id(state)
            .is_none()
            .then(|| "Set the Trakt client ID".to_string())
    }

    async fn fetch(state: &AppState, input: &ImportInput) -> Result<Vec<ImportItem>, ApiError> {
        let slug = input
            .username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_request("A Trakt username is required"))?;
        let client_id =
            client_id(state).ok_or_else(|| ApiError::bad_request("Set the Trakt client ID"))?;
        let slug = urlencoding::encode(slug);

        let movies = trakt_get(state, &client_id, &format!("/users/{slug}/watched/movies")).await?;
        let shows = trakt_get(state, &client_id, &format!("/users/{slug}/watched/shows")).await?;
        let watchlist = trakt_get(state, &client_id, &format!("/users/{slug}/watchlist")).await?;
        let ratings = trakt_get(state, &client_id, &format!("/users/{slug}/ratings")).await?;

        Ok(build_items(&movies, &shows, &watchlist, &ratings))
    }
}

fn client_id(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_TRAKT_CLIENT_ID)
        .filter(|value| !value.is_empty())
}

async fn trakt_get(state: &AppState, client_id: &str, path: &str) -> Result<Value, ApiError> {
    let response = state
        .http_client()
        .get(format!("{BASE}{path}"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header("trakt-api-version", "2")
        .header("trakt-api-key", client_id)
        .send()
        .await
        .map_err(|_| ApiError::bad_gateway("The Trakt request failed"))?;
    match response.status().as_u16() {
        404 => return Err(ApiError::bad_request("Trakt user not found")),
        401 => {
            return Err(ApiError::bad_request(
                "Trakt profile is private (public profiles only)",
            ))
        }
        _ => {}
    }
    response
        .error_for_status()
        .map_err(|_| ApiError::bad_gateway("The Trakt request failed"))?
        .json()
        .await
        .map_err(|_| ApiError::bad_gateway("Invalid Trakt response"))
}

/// Builds items from the four lists (pure; no network). Each list contributes a
/// facet of the same works; the in-batch dedup merges them by TMDB id.
fn build_items(
    movies: &Value,
    shows: &Value,
    watchlist: &Value,
    ratings: &Value,
) -> Vec<ImportItem> {
    let mut items = Vec::new();

    for entry in movies.as_array().into_iter().flatten() {
        let user = ImportUserData {
            status: Some(CanonicalStatus::Completed),
            completed: entry
                .get("last_watched_at")
                .and_then(Value::as_str)
                .and_then(date_part),
            ..Default::default()
        };
        if let Some(item) = media_item(entry.get("movie"), "movie", user) {
            items.push(item);
        }
    }

    for entry in shows.as_array().into_iter().flatten() {
        let user = ImportUserData {
            status: Some(CanonicalStatus::Ongoing),
            watched_count: watched_episodes(entry.get("seasons")),
            ..Default::default()
        };
        if let Some(item) = media_item(entry.get("show"), "tv", user) {
            items.push(item);
        }
    }

    for entry in watchlist.as_array().into_iter().flatten() {
        if let Some((node, media_type)) = trakt_node(entry) {
            let user = ImportUserData {
                status: Some(CanonicalStatus::Planning),
                ..Default::default()
            };
            if let Some(item) = media_item(Some(node), media_type, user) {
                items.push(item);
            }
        }
    }

    for entry in ratings.as_array().into_iter().flatten() {
        if let Some((node, media_type)) = trakt_node(entry) {
            let user = ImportUserData {
                score10: entry
                    .get("rating")
                    .and_then(Value::as_f64)
                    .filter(|rating| *rating > 0.0),
                ..Default::default()
            };
            if let Some(item) = media_item(Some(node), media_type, user) {
                items.push(item);
            }
        }
    }

    items
}

/// The (node, media_type) of a watchlist/ratings entry, or `None` for a
/// season/episode entry we don't import as a top-level item.
fn trakt_node(entry: &Value) -> Option<(&Value, &'static str)> {
    match entry.get("type").and_then(Value::as_str)? {
        "movie" => Some((entry.get("movie")?, "movie")),
        "show" => Some((entry.get("show")?, "tv")),
        _ => None,
    }
}

/// Builds an item from a Trakt movie/show node. `media_type` is `movie`/`tv`
/// (both the TMDB bucket and the URL segment). No TMDB id → no ref (review).
fn media_item(node: Option<&Value>, media_type: &str, user: ImportUserData) -> Option<ImportItem> {
    let node = node?;
    let title = node
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let tmdb = node.pointer("/ids/tmdb").and_then(Value::as_i64);

    let mut titles = BTreeMap::new();
    titles.insert("en".to_string(), title.clone());

    let (refs, candidate) = match tmdb {
        Some(tmdb) => {
            let id = tmdb.to_string();
            let url = format!("https://www.themoviedb.org/{media_type}/{id}");
            let candidate = ExternalCandidate {
                provider: "tmdb".to_string(),
                source_id: id.clone(),
                url: url.clone(),
                title: title.clone(),
                original_title: None,
                brief: None,
                cover_url: None,
                titles: titles.clone(),
                metadata: Map::new(),
            };
            (
                vec![ProviderRef {
                    provider: "tmdb".to_string(),
                    id,
                    url,
                }],
                Some(candidate),
            )
        }
        None => (Vec::new(), None),
    };

    Some(ImportItem {
        refs,
        bucket: media_type.to_string(),
        title,
        titles,
        candidate,
        user,
    })
}

fn watched_episodes(seasons: Option<&Value>) -> Option<u32> {
    let total: usize = seasons
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|season| season.get("episodes").and_then(Value::as_array))
        .map(|episodes| episodes.len())
        .sum();
    (total > 0).then_some(total as u32)
}

fn date_part(datetime: &str) -> Option<String> {
    let date = datetime.split(['T', ' ']).next().unwrap_or_default();
    (date.len() == 10 && date.as_bytes()[4] == b'-').then(|| date.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merges_watched_and_rated_facets_by_tmdb_id() {
        let movies = json!([{
            "last_watched_at": "2020-05-01T00:00:00Z",
            "movie": { "title": "Inception", "ids": { "tmdb": 27205 } }
        }]);
        let shows = json!([{
            "seasons": [{ "episodes": [{ "number": 1 }, { "number": 2 }] }],
            "show": { "title": "Dark", "ids": { "tmdb": 70523 } }
        }]);
        let watchlist = json!([]);
        let ratings = json!([{
            "rating": 9, "type": "movie",
            "movie": { "title": "Inception", "ids": { "tmdb": 27205 } }
        }]);
        let items = build_items(&movies, &shows, &watchlist, &ratings);
        // Inception appears in watched + ratings (two items, merged later); Dark once.
        let inception: Vec<_> = items
            .iter()
            .filter(|item| item.title == "Inception")
            .collect();
        assert_eq!(inception.len(), 2);
        assert_eq!(
            inception[0].refs[0].url,
            "https://www.themoviedb.org/movie/27205"
        );
        assert_eq!(inception[0].user.status, Some(CanonicalStatus::Completed));
        assert_eq!(inception[0].user.completed.as_deref(), Some("2020-05-01"));
        assert_eq!(inception[1].user.score10, Some(9.0));

        let dark = items.iter().find(|item| item.title == "Dark").unwrap();
        assert_eq!(dark.bucket, "tv");
        assert_eq!(dark.refs[0].url, "https://www.themoviedb.org/tv/70523");
        assert_eq!(dark.user.watched_count, Some(2));
    }

    #[test]
    fn an_entry_without_tmdb_id_has_no_ref() {
        let movies = json!([{ "movie": { "title": "Obscure", "ids": { "imdb": "tt999" } } }]);
        let items = build_items(&movies, &json!([]), &json!([]), &json!([]));
        assert_eq!(items.len(), 1);
        assert!(items[0].refs.is_empty());
    }
}
