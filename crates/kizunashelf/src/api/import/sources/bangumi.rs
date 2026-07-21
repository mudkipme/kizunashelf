//! Bangumi collection import. Fetches a public user's subject collections from
//! the official v0 API and normalizes each into an [`ImportItem`]. The bucket is
//! the numeric `subject_type` (matching the `bangumi` provider's `externalTypes`
//! vocabulary), and the candidate is built from the embedded slim subject with
//! the same metadata keys the provider emits, so most items map without a
//! detail fetch.

use super::super::model::{ImportItem, ImportUserData, ProviderRef};
use super::ImportSource;
use crate::api::error::ApiError;
use crate::api::external::{send_limited, USER_AGENT};
use crate::api::state::AppState;
use crate::contract::{ExternalCandidate, ImportInput, ImportInputKind};
use crate::types::CanonicalStatus;
use serde_json::{Map, Number, Value};
use std::collections::BTreeMap;

const PAGE_LIMIT: u32 = 50;
/// Safety cap on pagination (50 × 400 = 20k items) so a malformed `total` can't
/// loop forever.
const MAX_PAGES: u32 = 400;

pub(in crate::api::import) struct BangumiSource;

impl ImportSource for BangumiSource {
    const ID: &'static str = "bangumi";
    const LABEL: &'static str = "Bangumi";
    const INPUT: ImportInputKind = ImportInputKind::Profile;
    const INPUT_LABEL: &'static str = "Bangumi username";

    fn providers() -> &'static [&'static str] {
        &["bangumi"]
    }

    async fn fetch(state: &AppState, input: &ImportInput) -> Result<Vec<ImportItem>, ApiError> {
        let username = input
            .username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_request("A Bangumi username is required"))?;
        let client = state.http_client();

        let mut items = Vec::new();
        let mut offset = 0u32;
        for _ in 0..MAX_PAGES {
            let url = format!(
                "https://api.bgm.tv/v0/users/{}/collections?limit={PAGE_LIMIT}&offset={offset}",
                urlencoding::encode(username)
            );
            let response = send_limited(
                client
                    .get(&url)
                    .header(reqwest::header::USER_AGENT, USER_AGENT),
            )
            .await
            .map_err(|_| ApiError::bad_gateway("The Bangumi request failed"))?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(ApiError::bad_request("Bangumi user not found"));
            }
            let page: Value = response
                .error_for_status()
                .map_err(|_| ApiError::bad_gateway("The Bangumi request failed"))?
                .json()
                .await
                .map_err(|_| ApiError::bad_gateway("Invalid Bangumi response"))?;

            let data = page.get("data").and_then(Value::as_array);
            let count = data.map(|data| data.len() as u32).unwrap_or(0);
            if let Some(data) = data {
                let language = input.language.as_deref();
                items.extend(
                    data.iter()
                        .filter_map(|entry| collection_item(entry, language)),
                );
            }
            let total = page.get("total").and_then(Value::as_u64).unwrap_or(0) as u32;
            offset += count;
            if count == 0 || offset >= total {
                break;
            }
        }
        Ok(items)
    }
}

/// The display title for the viewer's `language`: Chinese (`name_cn`) for a `zh`
/// viewer, otherwise the original (`name`), each falling back to the other. Mirrors
/// `bangumi_display_title` in `api/external/bangumi.rs` (kept a local copy per this
/// module's mirror-the-provider convention rather than a cross-module dependency).
fn bangumi_display_title<'a>(name: &'a str, name_cn: &'a str, language: Option<&str>) -> &'a str {
    let (first, second) = if language.unwrap_or("zh").starts_with("zh") {
        (name_cn, name)
    } else {
        (name, name_cn)
    };
    if first.is_empty() {
        second
    } else {
        first
    }
}

/// Builds an item from one `UserSubjectCollection` entry, or `None` when it has
/// no subject id.
fn collection_item(entry: &Value, language: Option<&str>) -> Option<ImportItem> {
    let subject_id = entry.get("subject_id").and_then(Value::as_i64)?.to_string();
    let bucket = entry
        .get("subject_type")
        .and_then(Value::as_i64)
        .map(|value| value.to_string())
        .unwrap_or_default();
    let status = status_from(entry.get("type").and_then(Value::as_i64).unwrap_or(0));

    let subject = entry.get("subject");
    let name = subject
        .and_then(|subject| subject.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name_cn = subject
        .and_then(|subject| subject.get("name_cn"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let display = bangumi_display_title(name, name_cn, language);
    let title = if display.is_empty() {
        subject_id.clone()
    } else {
        display.to_string()
    };

    let mut titles = BTreeMap::new();
    if !name_cn.is_empty() {
        titles.insert("zh".to_string(), name_cn.to_string());
    }

    let cover_url = subject
        .and_then(|subject| subject.get("images"))
        .and_then(|images| {
            images
                .get("large")
                .or_else(|| images.get("common"))
                .or_else(|| images.get("medium"))
                .or_else(|| images.get("grid"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    // Mirror the keys `external/bangumi.rs` emits for a subject so field mappings
    // hit without a detail fetch (full summary / infobox keys still trigger one).
    let mut metadata = Map::new();
    if !name.is_empty() {
        metadata.insert("name".to_string(), Value::String(name.to_string()));
    }
    if !name_cn.is_empty() {
        metadata.insert("name_cn".to_string(), Value::String(name_cn.to_string()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(date) = subject
        .and_then(|subject| subject.get("date"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("date".to_string(), Value::String(date.to_string()));
    }
    for key in ["eps", "volumes"] {
        if let Some(count) = subject
            .and_then(|subject| subject.get(key))
            .and_then(Value::as_i64)
            .filter(|count| *count > 0)
        {
            metadata.insert(key.to_string(), Value::Number(count.into()));
        }
    }
    if let Some(score) = subject
        .and_then(|subject| subject.get("score"))
        .and_then(Value::as_f64)
        .filter(|score| *score > 0.0)
        .and_then(Number::from_f64)
    {
        metadata.insert("score".to_string(), Value::Number(score));
    }
    if let Some(rank) = subject
        .and_then(|subject| subject.get("rank"))
        .and_then(Value::as_i64)
        .filter(|rank| *rank > 0)
    {
        metadata.insert("rank".to_string(), Value::Number(rank.into()));
    }
    if let Some(tags) = subject
        .and_then(|subject| subject.get("tags"))
        .and_then(Value::as_array)
    {
        let names: Vec<Value> = tags
            .iter()
            .filter_map(|tag| tag.get("name").and_then(Value::as_str))
            .map(|name| Value::String(name.to_string()))
            .collect();
        if !names.is_empty() {
            metadata.insert("tags".to_string(), Value::Array(names));
        }
    }

    let url = format!("https://bgm.tv/subject/{subject_id}");
    let candidate = ExternalCandidate {
        provider: "bangumi".to_string(),
        source_id: subject_id.clone(),
        url: url.clone(),
        title: title.clone(),
        original_title: (!name.is_empty()).then(|| name.to_string()),
        brief: subject
            .and_then(|subject| subject.get("short_summary"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        cover_url,
        titles: titles.clone(),
        metadata,
    };

    // Bangumi gives only `updated_at`, not started/finished; treat it as the
    // completion date only when the collection is marked done.
    let completed = (status == Some(CanonicalStatus::Completed))
        .then(|| {
            entry
                .get("updated_at")
                .and_then(Value::as_str)
                .and_then(date_part)
        })
        .flatten();

    let user = ImportUserData {
        status,
        score10: entry
            .get("rate")
            .and_then(Value::as_f64)
            .filter(|rate| *rate > 0.0),
        watched_count: entry
            .get("ep_status")
            .and_then(Value::as_u64)
            .map(|count| count as u32)
            .filter(|count| *count > 0),
        started: None,
        completed,
        notes: entry
            .get("comment")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|comment| !comment.is_empty())
            .map(str::to_string),
    };

    Some(ImportItem {
        refs: vec![ProviderRef {
            provider: "bangumi".to_string(),
            id: subject_id,
            url,
        }],
        bucket,
        title,
        titles,
        candidate: Some(candidate),
        user,
    })
}

/// Bangumi collection `type` → canonical status: 1 wish, 2 done, 3 doing,
/// 4 on-hold, 5 dropped.
fn status_from(collection_type: i64) -> Option<CanonicalStatus> {
    match collection_type {
        1 => Some(CanonicalStatus::Planning),
        2 => Some(CanonicalStatus::Completed),
        3 => Some(CanonicalStatus::Ongoing),
        4 => Some(CanonicalStatus::Paused),
        5 => Some(CanonicalStatus::Dropped),
        _ => None,
    }
}

/// The `YYYY-MM-DD` date part of an ISO datetime, or `None` if it doesn't look
/// like one.
fn date_part(datetime: &str) -> Option<String> {
    let date = datetime.split(['T', ' ']).next().unwrap_or_default();
    (date.len() == 10 && date.as_bytes()[4] == b'-').then(|| date.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_a_done_anime_collection() {
        let entry = json!({
            "subject_id": 253,
            "subject_type": 2,
            "type": 2,
            "rate": 9,
            "ep_status": 26,
            "comment": "Classic.",
            "updated_at": "2020-05-01T10:00:00Z",
            "subject": {
                "name": "カウボーイビバップ",
                "name_cn": "星际牛仔",
                "images": { "large": "http://img/large.jpg" },
                "date": "1998-04-03",
                "eps": 26,
                "score": 8.9,
                "rank": 100,
                "tags": [{ "name": "SF", "count": 10 }],
                "short_summary": "A short blurb."
            }
        });
        let item = collection_item(&entry, Some("zh")).expect("item");
        assert_eq!(item.bucket, "2");
        assert_eq!(item.refs[0].url, "https://bgm.tv/subject/253");
        assert_eq!(item.title, "星际牛仔");
        assert_eq!(item.titles.get("zh").map(String::as_str), Some("星际牛仔"));
        assert_eq!(item.user.status, Some(CanonicalStatus::Completed));
        assert_eq!(item.user.score10, Some(9.0));
        assert_eq!(item.user.watched_count, Some(26));
        assert_eq!(item.user.completed.as_deref(), Some("2020-05-01"));
        let candidate = item.candidate.expect("candidate");
        assert_eq!(
            candidate.metadata.get("name_cn").and_then(Value::as_str),
            Some("星际牛仔")
        );
        assert_eq!(
            candidate.metadata.get("cover_url").and_then(Value::as_str),
            Some("http://img/large.jpg")
        );
    }

    #[test]
    fn a_planned_collection_has_no_completion_date() {
        let entry = json!({
            "subject_id": 1,
            "subject_type": 2,
            "type": 1,
            "updated_at": "2021-01-01T00:00:00Z",
            "subject": { "name": "Planned" }
        });
        let item = collection_item(&entry, None).expect("item");
        assert_eq!(item.user.status, Some(CanonicalStatus::Planning));
        assert_eq!(item.user.completed, None);
        assert_eq!(item.user.watched_count, None);
    }

    #[test]
    fn display_title_follows_viewer_language() {
        let entry = json!({
            "subject_id": 253,
            "subject_type": 2,
            "type": 2,
            "subject": { "name": "カウボーイビバップ", "name_cn": "星际牛仔" }
        });
        // zh viewer keeps the Chinese title; en/ja viewer sees the original.
        assert_eq!(
            collection_item(&entry, Some("zh")).unwrap().title,
            "星际牛仔"
        );
        assert_eq!(
            collection_item(&entry, Some("ja")).unwrap().title,
            "カウボーイビバップ"
        );
        // The Chinese title is still tagged in the titles map regardless.
        let item = collection_item(&entry, Some("en")).unwrap();
        assert_eq!(item.title, "カウボーイビバップ");
        assert_eq!(item.titles.get("zh").map(String::as_str), Some("星际牛仔"));
    }
}
