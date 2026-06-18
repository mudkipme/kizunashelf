use super::{external_client, provider_error, ExternalProvider, ProviderSearchConfig};
use crate::api::state::{unix_seconds_now, AppState, CachedAccessToken};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use crate::secrets::{SECRET_TVDB_API_KEY, SECRET_TVDB_PIN};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

pub(super) struct ThetvdbProvider;

impl ExternalProvider for ThetvdbProvider {
    const ID: &'static str = "thetvdb";
    const LABEL: &'static str = "TheTVDB";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        thetvdb_type_filters(provider_config).is_some()
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        tvdb_api_key(state)
            .is_none()
            .then(|| "Set the TheTVDB API key".to_string())
    }

    fn available(state: &AppState) -> bool {
        tvdb_api_key(state).is_some()
    }

    async fn search(
        state: &AppState,
        q: &str,
        _page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        search_thetvdb(state, q, page_size, provider_config).await
    }
}

fn tvdb_api_key(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_TVDB_API_KEY)
        .filter(|value| !value.is_empty())
}

async fn search_thetvdb(
    state: &AppState,
    q: &str,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let Some(api_key) = tvdb_api_key(state) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    let mut login = Map::new();
    login.insert("apikey".to_string(), Value::String(api_key));
    if let Some(pin) = state
        .secret_store()
        .get(SECRET_TVDB_PIN)
        .filter(|value| !value.is_empty())
    {
        login.insert("pin".to_string(), Value::String(pin));
    }
    let token = thetvdb_access_token(state, client, &login, false).await?;
    let Some(type_filters) = thetvdb_type_filters(provider_config) else {
        return Ok(Vec::new());
    };
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    for type_filter in type_filters {
        let data = search_thetvdb_type(
            state,
            client,
            &login,
            &token,
            &thetvdb_query(q),
            type_filter.as_deref(),
        )
        .await?;
        for candidate in data.iter().filter_map(thetvdb_candidate) {
            if seen.insert(format!("{}:{}", candidate.provider, candidate.source_id)) {
                items.push(candidate);
            }
            if items.len() >= page_size {
                return Ok(items);
            }
        }
    }
    Ok(items)
}

async fn search_thetvdb_type(
    state: &AppState,
    client: &reqwest::Client,
    login: &Map<String, Value>,
    token: &str,
    q: &str,
    type_filter: Option<&str>,
) -> Result<Vec<Value>, ApiError> {
    let request = thetvdb_search_request(client, token, q, type_filter);
    let response = request.send().await.map_err(provider_error)?;
    let response = if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        state.invalidate_access_token("thetvdb").await;
        let token = thetvdb_access_token(state, client, login, true).await?;
        thetvdb_search_request(client, &token, q, type_filter)
            .send()
            .await
            .map_err(provider_error)?
    } else {
        response
    };
    Ok(response
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn thetvdb_search_request<'a>(
    client: &'a reqwest::Client,
    token: &'a str,
    q: &'a str,
    type_filter: Option<&'a str>,
) -> reqwest::RequestBuilder {
    let request = client
        .get("https://api4.thetvdb.com/v4/search")
        .bearer_auth(token)
        .query(&[("query", q)]);
    if let Some(type_filter) = type_filter {
        request.query(&[("type", type_filter)])
    } else {
        request
    }
}

fn thetvdb_query(q: &str) -> String {
    let trimmed = q.trim().trim_end_matches('/');
    let Some((_, rest)) = trimmed.split_once("thetvdb.com/") else {
        return trimmed.to_string();
    };
    let parts = rest
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    parts
        .last()
        .map(|value| value.replace('-', " "))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| trimmed.to_string())
}

pub(super) fn thetvdb_type_filters(
    provider_config: &ProviderSearchConfig,
) -> Option<Vec<Option<String>>> {
    let Some(external_types) = provider_config.external_types() else {
        return Some(vec![None]);
    };
    let filters = external_types
        .iter()
        .filter_map(|external_type| thetvdb_type_filter(external_type).map(Some))
        .collect::<Vec<_>>();
    (!filters.is_empty()).then_some(filters)
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("name", "Name"),
        field_option("cover_url", "Cover URL"),
        field_option("first_air_time", "First air time"),
        field_option("year", "Year"),
        field_option("status", "Status"),
        field_option("primary_language", "Primary language"),
        field_option("country", "Country"),
        field_option("network", "Network"),
        field_option("director", "Director"),
        field_option("slug", "Slug"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("genres", "Genres"),
        field_option("studios", "Studios"),
        field_option("aliases", "Aliases"),
        field_option("overview", "Overview"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![
        type_option("series", "Series"),
        type_option("movie", "Movie"),
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

fn thetvdb_type_filter(external_type: &str) -> Option<String> {
    match external_type.trim().to_ascii_lowercase().as_str() {
        "series" => Some("series".to_string()),
        "movie" => Some("movie".to_string()),
        _ => None,
    }
}

async fn thetvdb_access_token(
    state: &AppState,
    client: &reqwest::Client,
    login: &Map<String, Value>,
    force_refresh: bool,
) -> Result<String, ApiError> {
    if !force_refresh {
        if let Some(token) = state.cached_access_token("thetvdb").await {
            return Ok(token.access_token);
        }
    }
    let value = client
        .post("https://api4.thetvdb.com/v4/login")
        .json(login)
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    let access_token = value
        .pointer("/data/token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ApiError::bad_request("TheTVDB login response did not include a token"))?;
    state
        .store_access_token(
            "thetvdb",
            CachedAccessToken {
                access_token: access_token.clone(),
                refresh_token: None,
                expires_at: Instant::now() + Duration::from_secs(23 * 60 * 60),
                expires_at_unix_seconds: unix_seconds_now() + 23 * 60 * 60,
            },
        )
        .await
        .map_err(|error| {
            ApiError::bad_request(&format!("failed to cache TheTVDB token: {error}"))
        })?;
    Ok(access_token)
}

fn thetvdb_candidate(item: &Value) -> Option<ExternalCandidate> {
    let source_id = item
        .get("tvdb_id")
        .or_else(|| item.get("id"))
        .and_then(|value| {
            value
                .as_i64()
                .map(|id| id.to_string())
                .or_else(|| value.as_str().map(str::to_string))
        })?;
    let title = item
        .get("name")
        .or_else(|| item.get("title"))
        .and_then(Value::as_str)?
        .to_string();
    let url = item
        .get("url")
        .and_then(Value::as_str)
        .map(|value| {
            if value.starts_with("http") {
                value.to_string()
            } else {
                format!("https://thetvdb.com{value}")
            }
        })
        .unwrap_or_else(|| format!("https://thetvdb.com/dereferrer/series/{source_id}"));
    let cover_url = item
        .get("image_url")
        .or_else(|| item.get("thumbnail"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let release_date = item
        .get("first_air_time")
        .and_then(non_empty_string_or_integer)
        .or_else(|| item.get("year").and_then(non_empty_string_or_integer));
    let mut metadata = Map::new();
    metadata.insert("name".to_string(), Value::String(title.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(release_date) = &release_date {
        metadata.insert(
            if release_date.len() == 4 {
                "year"
            } else {
                "first_air_time"
            }
            .to_string(),
            Value::String(release_date.clone()),
        );
    }
    if let Some(overview) = item
        .get("overview")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("overview".to_string(), Value::String(overview.to_string()));
    }
    for key in ["primary_language", "country", "director", "slug"] {
        if let Some(value) = item
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            metadata.insert(key.to_string(), Value::String(value.to_string()));
        }
    }
    // `status`/`network` are strings in search results but objects elsewhere.
    for key in ["status", "network"] {
        if let Some(value) = item.get(key).and_then(string_or_named) {
            metadata.insert(key.to_string(), Value::String(value));
        }
    }
    // String arrays → JSON arrays for list-type fields.
    for key in ["genres", "studios", "aliases"] {
        if let Some(values) = string_list(item.get(key)) {
            metadata.insert(key.to_string(), values);
        }
    }
    Some(ExternalCandidate {
        provider: "thetvdb".to_string(),
        source_id,
        url,
        original_title: Some(title.clone()),
        title,
        brief: item
            .get("overview")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

/// A field that is a plain string in search results but a `{ name }` object in
/// some other TheTVDB shapes (e.g. `status`, `network`).
fn string_or_named(value: &Value) -> Option<String> {
    value
        .as_str()
        .or_else(|| value.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Collects a string array into a JSON string array for list-type fields,
/// returning `None` when missing or empty.
fn string_list(value: Option<&Value>) -> Option<Value> {
    let items: Vec<Value> = value?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(|text| Value::String(text.to_string()))
        .collect();
    (!items.is_empty()).then_some(Value::Array(items))
}

fn non_empty_string_or_integer(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .or_else(|| value.as_u64().map(|value| value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::thetvdb_candidate;
    use serde_json::{json, Value};

    #[test]
    fn candidate_uses_numeric_year_metadata() {
        let candidate = thetvdb_candidate(&json!({
            "tvdb_id": 123,
            "name": "Example Series",
            "year": 2026
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "123");
        assert_eq!(candidate.metadata.get("year"), Some(&json!("2026")));
        assert_eq!(candidate.metadata.get("first_air_time"), None);
    }

    #[test]
    fn candidate_prefers_first_air_time_over_year() {
        let candidate = thetvdb_candidate(&json!({
            "id": "series-123",
            "name": "Example Series",
            "first_air_time": "2026-04-12",
            "year": 2026
        }))
        .unwrap();

        assert_eq!(
            candidate.metadata.get("first_air_time"),
            Some(&Value::String("2026-04-12".to_string()))
        );
        assert_eq!(candidate.metadata.get("year"), None);
    }

    #[test]
    fn candidate_surfaces_extended_metadata() {
        let candidate = thetvdb_candidate(&json!({
            "tvdb_id": 123,
            "name": "Example Series",
            "primary_language": "jpn",
            "country": "jpn",
            "status": "Continuing",
            "network": "TV Tokyo",
            "genres": ["Anime", "Action"],
            "studios": ["Studio X"]
        }))
        .unwrap();

        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("primary_language"), Some(&json!("jpn")));
        assert_eq!(metadata.get("status"), Some(&json!("Continuing")));
        assert_eq!(metadata.get("network"), Some(&json!("TV Tokyo")));
        assert_eq!(metadata.get("genres"), Some(&json!(["Anime", "Action"])));
        assert_eq!(metadata.get("studios"), Some(&json!(["Studio X"])));
    }
}
