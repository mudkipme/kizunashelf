use super::{
    external_client, field_option, provider_error, type_option, ExternalProvider,
    ProviderSearchConfig, USER_AGENT,
};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct BangumiProvider;

impl ExternalProvider for BangumiProvider {
    const ID: &'static str = "bangumi";
    const LABEL: &'static str = "Bangumi";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        bangumi_types(provider_config).is_some()
    }

    async fn search(
        _state: &super::AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        search_bangumi(q, page, page_size, provider_config).await
    }
}

async fn search_bangumi(
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let Some(filter_types) = bangumi_types(provider_config) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    if let Some(subject_id) = bangumi_subject_id(q) {
        let value = client
            .get(format!("https://api.bgm.tv/v0/subjects/{subject_id}"))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        return Ok(bangumi_candidate(&value).into_iter().collect());
    }
    let response = client
        .post(format!(
            "https://api.bgm.tv/v0/search/subjects?limit={page_size}&offset={}",
            (page - 1) * page_size
        ))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .json(&json!({
            "keyword": q,
            "filter": {
                "type": filter_types
            }
        }))
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    let data = response
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(data.iter().filter_map(bangumi_candidate).collect())
}

fn bangumi_subject_id(q: &str) -> Option<String> {
    let trimmed = q.trim().trim_end_matches('/');
    if trimmed.chars().all(|character| character.is_ascii_digit()) {
        return Some(trimmed.to_string());
    }
    let marker = "/subject/";
    let (_, rest) = trimmed.split_once(marker)?;
    let id = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .then(|| id.to_string())
}

pub(super) fn bangumi_types(provider_config: &ProviderSearchConfig) -> Option<Vec<u32>> {
    let Some(external_types) = provider_config.external_types() else {
        return Some(vec![1, 2, 3, 4, 6]);
    };
    let mut types = BTreeSet::new();
    for external_type in external_types {
        if let Some(value) = bangumi_type(external_type) {
            types.insert(value);
        }
    }
    (!types.is_empty()).then(|| types.into_iter().collect())
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("name", "Name"),
        field_option("name_cn", "Chinese name"),
        field_option("cover_url", "Cover URL"),
        field_option("date", "Release date"),
        field_option("platform", "Platform"),
        // `eps` is the count declared in the wiki infobox; `total_episodes` is how
        // many episode records actually exist in Bangumi's database. They differ
        // for ongoing or sparsely-maintained subjects, so both are offered.
        field_option("eps", "Episodes (declared)"),
        field_option("total_episodes", "Episodes (in database)"),
        field_option("volumes", "Volumes"),
        field_option("score", "Score"),
        field_option("rank", "Rank"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("tags", "Tags"),
        field_option("meta_tags", "Meta tags"),
        field_option("summary", "Summary"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![
        type_option("1", "Book (1)"),
        type_option("2", "Anime (2)"),
        type_option("3", "Music (3)"),
        type_option("4", "Game (4)"),
        type_option("6", "Real (6)"),
    ]
}

/// Collects an array's elements into a JSON string array via `extract`, dropping
/// empties and returning `None` when the source is missing or yields nothing.
/// Used for list-shaped metadata (tags, genres) that maps onto list-type fields.
fn string_list<'a>(
    value: Option<&'a Value>,
    extract: impl Fn(&'a Value) -> Option<&'a str>,
) -> Option<Value> {
    let items: Vec<Value> = value?
        .as_array()?
        .iter()
        .filter_map(extract)
        .filter(|text| !text.is_empty())
        .map(|text| Value::String(text.to_string()))
        .collect();
    (!items.is_empty()).then_some(Value::Array(items))
}

fn bangumi_type(external_type: &str) -> Option<u32> {
    match external_type.trim().to_ascii_lowercase().as_str() {
        "1" => Some(1),
        "2" => Some(2),
        "3" => Some(3),
        "4" => Some(4),
        "6" => Some(6),
        _ => None,
    }
}

fn bangumi_candidate(item: &Value) -> Option<ExternalCandidate> {
    let id = item.get("id")?.as_i64()?.to_string();
    let url = format!("https://bgm.tv/subject/{id}");
    let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
    let name_cn = item
        .get("name_cn")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let title = if name_cn.is_empty() { name } else { name_cn };
    if title.is_empty() {
        return None;
    }
    let cover_url = item
        .get("images")
        .and_then(|images| images.get("common").or_else(|| images.get("grid")))
        .and_then(Value::as_str)
        .map(str::to_string);
    let release_date = item.get("date").and_then(Value::as_str).unwrap_or_default();
    let mut titles = BTreeMap::new();
    if !name_cn.is_empty() {
        titles.insert("zh".to_string(), name_cn.to_string());
    }
    if !name.is_empty() {
        titles.insert("ja".to_string(), name.to_string());
    }
    let mut metadata = Map::new();
    metadata.insert("name".to_string(), Value::String(name.to_string()));
    if !name_cn.is_empty() {
        metadata.insert("name_cn".to_string(), Value::String(name_cn.to_string()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if !release_date.is_empty() {
        metadata.insert("date".to_string(), Value::String(release_date.to_string()));
    }
    if let Some(platform) = item
        .get("platform")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("platform".to_string(), Value::String(platform.to_string()));
    }
    for (key, label) in [
        ("total_episodes", "total_episodes"),
        ("eps", "eps"),
        ("volumes", "volumes"),
    ] {
        if let Some(count) = item
            .get(key)
            .and_then(Value::as_i64)
            .filter(|count| *count > 0)
        {
            metadata.insert(label.to_string(), Value::Number(count.into()));
        }
    }
    // Score/rank live under `rating` on a subject fetch but at the top level on
    // some search responses; accept either shape.
    let rating = item.get("rating");
    if let Some(score) = rating
        .and_then(|rating| rating.get("score"))
        .or_else(|| item.get("score"))
        .and_then(Value::as_f64)
        .filter(|score| *score > 0.0)
    {
        metadata.insert("score".to_string(), json!(score));
    }
    if let Some(rank) = rating
        .and_then(|rating| rating.get("rank"))
        .or_else(|| item.get("rank"))
        .and_then(Value::as_i64)
        .filter(|rank| *rank > 0)
    {
        metadata.insert("rank".to_string(), Value::Number(rank.into()));
    }
    if let Some(tags) = string_list(item.get("tags"), |tag| {
        tag.get("name").and_then(Value::as_str)
    }) {
        metadata.insert("tags".to_string(), tags);
    }
    if let Some(meta_tags) = string_list(item.get("meta_tags"), Value::as_str) {
        metadata.insert("meta_tags".to_string(), meta_tags);
    }
    if let Some(summary) = item
        .get("summary")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("summary".to_string(), Value::String(summary.to_string()));
    }
    Some(ExternalCandidate {
        provider: "bangumi".to_string(),
        source_id: id,
        url,
        title: title.to_string(),
        original_title: (!name.is_empty()).then(|| name.to_string()),
        brief: item
            .get("summary")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        cover_url,
        titles,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::bangumi_candidate;
    use serde_json::json;

    #[test]
    fn candidate_surfaces_extended_metadata() {
        let candidate = bangumi_candidate(&json!({
            "id": 8,
            "name": "Cowboy Bebop",
            "name_cn": "星际牛仔",
            "platform": "TV",
            "eps": 26,
            "total_episodes": 26,
            "rating": { "score": 8.7, "rank": 42 },
            "tags": [{ "name": "Sci-Fi", "count": 100 }, { "name": "Space", "count": 50 }],
            "meta_tags": ["TV", "Original"]
        }))
        .unwrap();

        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("platform"), Some(&json!("TV")));
        assert_eq!(metadata.get("eps"), Some(&json!(26)));
        assert_eq!(metadata.get("score"), Some(&json!(8.7)));
        assert_eq!(metadata.get("rank"), Some(&json!(42)));
        assert_eq!(metadata.get("tags"), Some(&json!(["Sci-Fi", "Space"])));
        assert_eq!(metadata.get("meta_tags"), Some(&json!(["TV", "Original"])));
    }
}
