//! Kitsu import. Resolves a public username to a user id, then pages the library
//! entries for anime and manga. Kitsu's own ids aren't stored — each entry's
//! Kitsu `mappings` are used to resolve a MyAnimeList id (entries with only a
//! non-MAL mapping, e.g. MangaUpdates, go to review). The candidate is minimal
//! and detail-fetched from MAL at commit.

use super::super::model::{ImportItem, ImportUserData, ProviderRef};
use super::ImportSource;
use crate::api::error::ApiError;
use crate::api::external::USER_AGENT;
use crate::api::state::AppState;
use crate::contract::{ExternalCandidate, ImportInput, ImportInputKind};
use crate::types::CanonicalStatus;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

const BASE: &str = "https://kitsu.app/api/edge";
/// Safety cap on pagination (500 × 200 = 100k entries).
const MAX_PAGES: u32 = 200;

pub(in crate::api::import) struct KitsuSource;

impl ImportSource for KitsuSource {
    const ID: &'static str = "kitsu";
    const LABEL: &'static str = "Kitsu";
    const INPUT: ImportInputKind = ImportInputKind::Profile;
    const INPUT_LABEL: &'static str = "Kitsu username";

    fn providers() -> &'static [&'static str] {
        &["myanimelist"]
    }

    async fn fetch(state: &AppState, input: &ImportInput) -> Result<Vec<ImportItem>, ApiError> {
        let username = input
            .username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_request("A Kitsu username is required"))?;
        let client = state.http_client();

        let users: Value = kitsu_get(
            client,
            &format!(
                "{BASE}/users?filter[name]={}&page[limit]=1",
                urlencoding::encode(username)
            ),
        )
        .await?;
        let user_id = users
            .pointer("/data/0/id")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::bad_request("Kitsu user not found"))?
            .to_string();

        let mut items = Vec::new();
        for kind in ["anime", "manga"] {
            let mut url = format!(
                "{BASE}/library-entries?filter[user_id]={user_id}&filter[kind]={kind}&include={kind},{kind}.mappings&page[limit]=500"
            );
            for _ in 0..MAX_PAGES {
                let page = kitsu_get(client, &url).await?;
                items.extend(build_items(&page, kind));
                match page
                    .pointer("/links/next")
                    .and_then(Value::as_str)
                    .filter(|next| !next.is_empty())
                {
                    Some(next) => url = next.to_string(),
                    None => break,
                }
            }
        }
        Ok(items)
    }
}

async fn kitsu_get(client: &reqwest::Client, url: &str) -> Result<Value, ApiError> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.api+json")
        .send()
        .await
        .map_err(|_| ApiError::bad_gateway("The Kitsu request failed"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(ApiError::bad_request("Kitsu user not found"));
    }
    response
        .error_for_status()
        .map_err(|_| ApiError::bad_gateway("The Kitsu request failed"))?
        .json()
        .await
        .map_err(|_| ApiError::bad_gateway("Invalid Kitsu response"))
}

/// Turns one JSON:API page of library entries into items (pure; no network).
/// `kind` is `anime`/`manga` — the relationship name and bucket.
fn build_items(page: &Value, kind: &str) -> Vec<ImportItem> {
    let included = page.get("included").and_then(Value::as_array);
    // mapping id → (externalSite, externalId)
    let mut mappings: HashMap<&str, (&str, &str)> = HashMap::new();
    // media id → mapping ids, and media id → canonical title
    let mut media_mappings: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut media_titles: HashMap<&str, &str> = HashMap::new();
    if let Some(included) = included {
        for resource in included {
            let (Some(res_type), Some(id)) = (
                resource.get("type").and_then(Value::as_str),
                resource.get("id").and_then(Value::as_str),
            ) else {
                continue;
            };
            if res_type == "mappings" {
                let site = resource
                    .pointer("/attributes/externalSite")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let external_id = resource
                    .pointer("/attributes/externalId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                mappings.insert(id, (site, external_id));
            } else if res_type == kind {
                if let Some(title) = resource
                    .pointer("/attributes/canonicalTitle")
                    .and_then(Value::as_str)
                {
                    media_titles.insert(id, title);
                }
                let mapping_ids = resource
                    .pointer("/relationships/mappings/data")
                    .and_then(Value::as_array)
                    .map(|data| {
                        data.iter()
                            .filter_map(|entry| entry.get("id").and_then(Value::as_str))
                            .collect()
                    })
                    .unwrap_or_default();
                media_mappings.insert(id, mapping_ids);
            }
        }
    }

    let Some(entries) = page.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .map(|entry| {
            let media_id = entry
                .pointer(&format!("/relationships/{kind}/data/id"))
                .and_then(Value::as_str);
            let mal_id = media_id
                .and_then(|media_id| media_mappings.get(media_id))
                .into_iter()
                .flatten()
                .find_map(|mapping_id| {
                    let (site, external_id) = mappings.get(mapping_id)?;
                    site.starts_with("myanimelist").then_some(*external_id)
                });
            let title = media_id
                .and_then(|media_id| media_titles.get(media_id))
                .copied()
                .unwrap_or("Untitled")
                .to_string();
            build_item(entry, kind, mal_id, title)
        })
        .collect()
}

fn build_item(entry: &Value, kind: &str, mal_id: Option<&str>, title: String) -> ImportItem {
    let attributes = entry.get("attributes");
    let user = ImportUserData {
        status: attributes
            .and_then(|attributes| attributes.get("status"))
            .and_then(Value::as_str)
            .and_then(status_from),
        // ratingTwenty is a 0–20 scale; halve to 0–10.
        score10: attributes
            .and_then(|attributes| attributes.get("ratingTwenty"))
            .and_then(Value::as_f64)
            .filter(|rating| *rating > 0.0)
            .map(|rating| rating / 2.0),
        watched_count: attributes
            .and_then(|attributes| attributes.get("progress"))
            .and_then(Value::as_u64)
            .map(|count| count as u32)
            .filter(|count| *count > 0),
        started: attributes
            .and_then(|attributes| attributes.get("startedAt"))
            .and_then(Value::as_str)
            .and_then(date_part),
        completed: attributes
            .and_then(|attributes| attributes.get("finishedAt"))
            .and_then(Value::as_str)
            .and_then(date_part),
        notes: attributes
            .and_then(|attributes| attributes.get("notes"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|notes| !notes.is_empty())
            .map(str::to_string),
    };

    let mut titles = BTreeMap::new();
    titles.insert("en".to_string(), title.clone());

    let (refs, candidate) = match mal_id {
        Some(mal_id) => {
            let url = format!("https://myanimelist.net/{kind}/{mal_id}");
            let candidate = ExternalCandidate {
                provider: "myanimelist".to_string(),
                source_id: mal_id.to_string(),
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
                    provider: "myanimelist".to_string(),
                    id: mal_id.to_string(),
                    url,
                }],
                Some(candidate),
            )
        }
        None => (Vec::new(), None),
    };

    ImportItem {
        refs,
        bucket: kind.to_string(),
        title,
        titles,
        candidate,
        user,
    }
}

fn status_from(value: &str) -> Option<CanonicalStatus> {
    match value {
        "completed" => Some(CanonicalStatus::Completed),
        "current" => Some(CanonicalStatus::Ongoing),
        "planned" => Some(CanonicalStatus::Planning),
        "on_hold" => Some(CanonicalStatus::Paused),
        "dropped" => Some(CanonicalStatus::Dropped),
        _ => None,
    }
}

fn date_part(value: &str) -> Option<String> {
    let date = value.split(['T', ' ']).next().unwrap_or_default();
    (date.len() == 10 && date.as_bytes()[4] == b'-').then(|| date.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolves_mal_mapping_and_maps_user_data() {
        let page = json!({
            "data": [{
                "type": "libraryEntries",
                "id": "10",
                "attributes": {
                    "status": "completed", "progress": 26, "ratingTwenty": 18,
                    "startedAt": "2020-01-01", "finishedAt": "2020-02-01", "notes": "Nice"
                },
                "relationships": { "anime": { "data": { "type": "anime", "id": "1" } } }
            }],
            "included": [
                { "type": "anime", "id": "1",
                  "attributes": { "canonicalTitle": "Cowboy Bebop" },
                  "relationships": { "mappings": { "data": [{ "type": "mappings", "id": "m1" }] } } },
                { "type": "mappings", "id": "m1",
                  "attributes": { "externalSite": "myanimelist/anime", "externalId": "1" } }
            ]
        });
        let items = build_items(&page, "anime");
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.refs[0].url, "https://myanimelist.net/anime/1");
        assert_eq!(item.title, "Cowboy Bebop");
        assert_eq!(item.user.status, Some(CanonicalStatus::Completed));
        assert_eq!(item.user.score10, Some(9.0));
        assert_eq!(item.user.watched_count, Some(26));
        assert_eq!(item.user.completed.as_deref(), Some("2020-02-01"));
    }

    #[test]
    fn a_non_mal_mapping_goes_to_review() {
        let page = json!({
            "data": [{
                "type": "libraryEntries", "id": "11",
                "attributes": { "status": "current" },
                "relationships": { "manga": { "data": { "type": "manga", "id": "2" } } }
            }],
            "included": [
                { "type": "manga", "id": "2",
                  "attributes": { "canonicalTitle": "Some Manga" },
                  "relationships": { "mappings": { "data": [{ "type": "mappings", "id": "m2" }] } } },
                { "type": "mappings", "id": "m2",
                  "attributes": { "externalSite": "mangaupdates", "externalId": "999" } }
            ]
        });
        let items = build_items(&page, "manga");
        assert_eq!(items.len(), 1);
        assert!(items[0].refs.is_empty());
        assert!(items[0].candidate.is_none());
    }
}
