use super::{external_client, provider_error, ExternalProvider, ProviderSearchConfig};
use crate::api::state::{unix_seconds_now, AppState, CachedAccessToken};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
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

    fn unavailable_reason() -> Option<String> {
        std::env::var("KIZUNASHELF_TVDB_API_KEY")
            .ok()
            .is_none()
            .then(|| "Set KIZUNASHELF_TVDB_API_KEY".to_string())
    }

    fn available() -> bool {
        std::env::var("KIZUNASHELF_TVDB_API_KEY").ok().is_some()
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

async fn search_thetvdb(
    state: &AppState,
    q: &str,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let Some(api_key) = std::env::var("KIZUNASHELF_TVDB_API_KEY")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        return Ok(Vec::new());
    };
    let client = external_client()?;
    let mut login = Map::new();
    login.insert("apikey".to_string(), Value::String(api_key));
    if let Ok(pin) = std::env::var("KIZUNASHELF_TVDB_PIN") {
        if !pin.is_empty() {
            login.insert("pin".to_string(), Value::String(pin));
        }
    }
    let token = thetvdb_access_token(state, &client, &login, false).await?;
    let Some(type_filters) = thetvdb_type_filters(provider_config) else {
        return Ok(Vec::new());
    };
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    for type_filter in type_filters {
        let data = search_thetvdb_type(
            state,
            &client,
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
        field_option("overview", "Overview"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("series", "Series"), type_option("movie", "Movie")]
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
        .or_else(|| item.get("year"))
        .and_then(|value| {
            value
                .as_str()
                .or_else(|| value.as_i64().map(|_| "").filter(|_| false))
        })
        .filter(|value| !value.is_empty())
        .map(str::to_string);
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
