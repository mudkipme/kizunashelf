//! AniList import. Fetches a public user's anime + manga lists via the AniList
//! GraphQL API (no credentials). Items carry an `anilist` ref first and, when
//! the entry embeds a MyAnimeList id (`idMal`), a `myanimelist` ref second —
//! the plan/commit pipeline resolves each item to the first ref the vault's
//! schema maps (see `ImportItem::ref_for_config`), so an anilist-wired type
//! links AniList and a MAL-only vault keeps working. The candidate is minimal
//! (title + cover + ref), so commit detail-fetches for types that map richer
//! fields.

use super::super::model::{ImportItem, ImportUserData, ProviderRef};
use super::ImportSource;
use crate::api::error::ApiError;
use crate::api::external::{anilist_fuzzy_date, anilist_origin_language, send_limited, USER_AGENT};
use crate::api::state::AppState;
use crate::contract::{ExternalCandidate, ImportInput, ImportInputKind};
use crate::types::CanonicalStatus;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

const ENDPOINT: &str = "https://graphql.anilist.co";

/// One request pulls both lists; each entry carries the AniList id, the embedded
/// MAL id, the user data, and enough of the media to build a minimal candidate.
const QUERY: &str = "\
query ($userName: String) {
  anime: MediaListCollection(userName: $userName, type: ANIME) { lists { entries { ...entry } } }
  manga: MediaListCollection(userName: $userName, type: MANGA) { lists { entries { ...entry } } }
}
fragment entry on MediaList {
  status
  score(format: POINT_10_DECIMAL)
  progress
  notes
  startedAt { year month day }
  completedAt { year month day }
  media { id idMal title { romaji english native } countryOfOrigin coverImage { large } }
}";

pub(in crate::api::import) struct AniListSource;

impl ImportSource for AniListSource {
    const ID: &'static str = "anilist";
    const LABEL: &'static str = "AniList";
    const INPUT: ImportInputKind = ImportInputKind::Profile;
    const INPUT_LABEL: &'static str = "AniList username";

    fn providers() -> &'static [&'static str] {
        &["anilist", "myanimelist"]
    }

    async fn fetch(state: &AppState, input: &ImportInput) -> Result<Vec<ImportItem>, ApiError> {
        let username = input
            .username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_request("An AniList username is required"))?;

        let body = json!({ "query": QUERY, "variables": { "userName": username } });
        let value: Value = send_limited(
            state
                .http_client()
                .post(ENDPOINT)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .json(&body),
        )
        .await
        .map_err(|_| ApiError::bad_gateway("The AniList request failed"))?
        .json()
        .await
        .map_err(|_| ApiError::bad_gateway("Invalid AniList response"))?;

        // AniList reports a missing/private user as a GraphQL error with null data.
        if value.get("data").map(Value::is_null).unwrap_or(true) {
            let message = value
                .get("errors")
                .and_then(Value::as_array)
                .and_then(|errors| errors.first())
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("AniList request failed");
            return Err(ApiError::bad_request(&format!("AniList: {message}")));
        }

        let mut items = Vec::new();
        for media_type in ["anime", "manga"] {
            if let Some(lists) = value
                .pointer(&format!("/data/{media_type}/lists"))
                .and_then(Value::as_array)
            {
                for list in lists {
                    if let Some(entries) = list.get("entries").and_then(Value::as_array) {
                        items.extend(entries.iter().map(|entry| anilist_item(entry, media_type)));
                    }
                }
            }
        }
        Ok(items)
    }
}

/// Builds an item from one AniList list entry. Refs are in preference order:
/// `anilist` (always — every entry has an AniList id), then `myanimelist` when
/// the entry embeds an `idMal`. The in-batch dedup (keyed on the primary
/// AniList ref) collapses the same media appearing across AniList's per-status
/// and custom lists.
fn anilist_item(entry: &Value, media_type: &str) -> ImportItem {
    let media = entry.get("media");
    let id_anilist = media
        .and_then(|media| media.get("id"))
        .and_then(Value::as_i64);
    let id_mal = media
        .and_then(|media| media.get("idMal"))
        .and_then(Value::as_i64);

    let english = media
        .and_then(|media| media.pointer("/title/english"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let romaji = media
        .and_then(|media| media.pointer("/title/romaji"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let native = media
        .and_then(|media| media.pointer("/title/native"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let title = english
        .or(romaji)
        .or(native)
        .unwrap_or("Untitled")
        .to_string();
    let cover_url = media
        .and_then(|media| media.pointer("/coverImage/large"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut titles = BTreeMap::new();
    if let Some(english) = english {
        titles.insert("en".to_string(), english.to_string());
    }
    // The native title's language comes from the media's origin country —
    // AniList also lists Korean/Chinese works, so `native` must not be assumed
    // Japanese. Romaji stays untagged (it is no language's display title).
    let native_language = media
        .and_then(|media| media.get("countryOfOrigin"))
        .and_then(Value::as_str)
        .and_then(anilist_origin_language);
    if let (Some(native), Some(language)) = (native, native_language) {
        titles.insert(language.to_string(), native.to_string());
    }

    let user = ImportUserData {
        status: entry
            .get("status")
            .and_then(Value::as_str)
            .and_then(status_from),
        score10: entry
            .get("score")
            .and_then(Value::as_f64)
            .filter(|score| *score > 0.0),
        watched_count: entry
            .get("progress")
            .and_then(Value::as_u64)
            .map(|count| count as u32)
            .filter(|count| *count > 0),
        started: entry.get("startedAt").and_then(anilist_fuzzy_date),
        completed: entry.get("completedAt").and_then(anilist_fuzzy_date),
        notes: entry
            .get("notes")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    };

    let mut refs = Vec::new();
    let mut candidate = None;
    if let Some(id_anilist) = id_anilist {
        let id = id_anilist.to_string();
        let url = format!("https://anilist.co/{media_type}/{id}");
        // The partial candidate is an AniList one (the primary ref's provider);
        // commit discards it when the item resolves to another provider's ref
        // and detail-fetches that provider instead.
        candidate = Some(ExternalCandidate {
            needs_detail: false,
            provider: "anilist".to_string(),
            source_id: id.clone(),
            url: url.clone(),
            title: title.clone(),
            original_title: None,
            brief: None,
            cover_url,
            titles: titles.clone(),
            metadata: Map::new(),
        });
        refs.push(ProviderRef {
            provider: "anilist".to_string(),
            id,
            url,
        });
    }
    if let Some(id_mal) = id_mal {
        let id = id_mal.to_string();
        refs.push(ProviderRef {
            provider: "myanimelist".to_string(),
            id: id.clone(),
            url: format!("https://myanimelist.net/{media_type}/{id}"),
        });
    }

    ImportItem {
        refs,
        bucket: media_type.to_string(),
        title,
        titles,
        candidate,
        user,
    }
}

/// AniList list status → canonical. `CURRENT`/`REPEATING` mean in progress.
fn status_from(value: &str) -> Option<CanonicalStatus> {
    match value {
        "CURRENT" | "REPEATING" => Some(CanonicalStatus::Ongoing),
        "PLANNING" => Some(CanonicalStatus::Planning),
        "COMPLETED" => Some(CanonicalStatus::Completed),
        "PAUSED" => Some(CanonicalStatus::Paused),
        "DROPPED" => Some(CanonicalStatus::Dropped),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_an_entry_with_anilist_ref_first_then_mal() {
        let entry = json!({
            "status": "COMPLETED",
            "score": 8.5,
            "progress": 26,
            "notes": "Great.",
            "startedAt": { "year": 2020, "month": 1, "day": 3 },
            "completedAt": { "year": 2020, "month": 2, "day": null },
            "media": {
                "id": 1,
                "idMal": 1,
                "title": { "romaji": "Cowboy Bebop", "english": "Cowboy Bebop", "native": "カウボーイビバップ" },
                "coverImage": { "large": "http://img/cb.jpg" }
            }
        });
        let item = anilist_item(&entry, "anime");
        assert_eq!(item.refs[0].provider, "anilist");
        assert_eq!(item.refs[0].url, "https://anilist.co/anime/1");
        assert_eq!(item.refs[1].provider, "myanimelist");
        assert_eq!(item.refs[1].url, "https://myanimelist.net/anime/1");
        assert_eq!(item.bucket, "anime");
        assert_eq!(item.user.status, Some(CanonicalStatus::Completed));
        assert_eq!(item.user.score10, Some(8.5));
        assert_eq!(item.user.watched_count, Some(26));
        assert_eq!(item.user.started.as_deref(), Some("2020-01-03"));
        assert_eq!(item.user.completed.as_deref(), Some("2020-02"));
        assert_eq!(
            item.candidate.as_ref().map(|c| c.provider.as_str()),
            Some("anilist")
        );
    }

    #[test]
    fn an_entry_without_mal_id_still_carries_the_anilist_ref() {
        let entry = json!({
            "status": "PLANNING",
            "media": { "id": 99, "idMal": null, "title": { "romaji": "Original Work" } }
        });
        let item = anilist_item(&entry, "manga");
        assert_eq!(item.refs.len(), 1);
        assert_eq!(item.refs[0].provider, "anilist");
        assert_eq!(item.refs[0].url, "https://anilist.co/manga/99");
        assert_eq!(item.title, "Original Work");
    }
}
