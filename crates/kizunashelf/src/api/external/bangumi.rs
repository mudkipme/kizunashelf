use super::{external_client, provider_error, ExternalProvider, ProviderSearchConfig, USER_AGENT};
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
    let client = external_client()?;
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
    Ok(data
        .iter()
        .filter_map(|item| bangumi_candidate(item))
        .collect())
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
        field_option("total_episodes", "Total episodes"),
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

fn field_option(field: &str, label: &str) -> ExternalProviderFieldOption {
    ExternalProviderFieldOption {
        field: field.to_string(),
        label: label.to_string(),
    }
}

fn type_option(value: &str, label: &str) -> ExternalProviderTypeOption {
    ExternalProviderTypeOption {
        value: value.to_string(),
        label: label.to_string(),
    }
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
    if let Some(episodes) = item.get("total_episodes").and_then(Value::as_i64) {
        if episodes > 0 {
            metadata.insert("total_episodes".to_string(), Value::Number(episodes.into()));
        }
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
