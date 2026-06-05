use super::error::{ApiError, ApiResult};
use crate::contract::{ExternalCandidate, ExternalProviderSummary, ExternalSearchResponse};
use crate::dates::clamp_number;
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use super::state::{unix_seconds_now, AppState, CachedAccessToken};
use std::time::{Duration, Instant};

const USER_AGENT: &str = concat!("KizunaShelf/", env!("CARGO_PKG_VERSION"));

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ExternalSearchQuery {
    provider: Option<String>,
    q: Option<String>,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    #[serde(rename = "pageSize")]
    page_size: Option<f64>,
    page: Option<f64>,
}

pub(crate) async fn external_search(
    State(state): State<AppState>,
    Query(query): Query<ExternalSearchQuery>,
) -> ApiResult<ExternalSearchResponse> {
    let providers = provider_summaries();
    let q = query.q.as_deref().unwrap_or_default().trim();
    if q.is_empty() {
        return Ok(Json(ExternalSearchResponse {
            providers,
            items: Vec::new(),
        }));
    }
    let requested_provider = query.provider.as_deref().filter(|value| *value != "all");
    let entity_type = query.entity_type.as_deref().unwrap_or("all");
    let page_size = clamp_number(query.page_size.unwrap_or(10.0), 1, 25) as usize;
    let page = clamp_number(query.page.unwrap_or(1.0), 1, i64::MAX) as usize;

    let mut items = Vec::new();
    if requested_provider.is_none() || requested_provider == Some("bangumi") {
        items.extend(search_bangumi(q, entity_type, page, page_size).await?);
    }
    if requested_provider.is_none() || requested_provider == Some("igdb") {
        items.extend(search_igdb(&state, q, entity_type, page, page_size).await?);
    }
    if requested_provider.is_none() || requested_provider == Some("thetvdb") {
        items.extend(search_thetvdb(&state, q, entity_type, page_size).await?);
    }
    if let Some(provider) = requested_provider {
        if !providers.iter().any(|item| item.id == provider) {
            return Err(ApiError::bad_request("Unknown external provider"));
        }
    }
    Ok(Json(ExternalSearchResponse { providers, items }))
}

fn provider_summaries() -> Vec<ExternalProviderSummary> {
    vec![
        ExternalProviderSummary {
            id: "bangumi".to_string(),
            label: "Bangumi".to_string(),
            enabled: true,
            reason: None,
        },
        ExternalProviderSummary {
            id: "igdb".to_string(),
            label: "IGDB".to_string(),
            enabled: igdb_credentials().is_some(),
            reason: igdb_credentials().is_none().then(|| {
                "Set KIZUNASHELF_IGDB_CLIENT_ID and KIZUNASHELF_IGDB_CLIENT_SECRET".to_string()
            }),
        },
        ExternalProviderSummary {
            id: "thetvdb".to_string(),
            label: "TheTVDB".to_string(),
            enabled: std::env::var("KIZUNASHELF_TVDB_API_KEY").ok().is_some(),
            reason: std::env::var("KIZUNASHELF_TVDB_API_KEY")
                .ok()
                .is_none()
                .then(|| "Set KIZUNASHELF_TVDB_API_KEY".to_string()),
        },
    ]
}

async fn search_bangumi(
    q: &str,
    entity_type: &str,
    page: usize,
    page_size: usize,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let Some(filter_types) = bangumi_types(entity_type) else {
        return Ok(Vec::new());
    };
    let client = reqwest::Client::new();
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

fn bangumi_types(entity_type: &str) -> Option<Vec<u32>> {
    match entity_type {
        "all" => Some(vec![1, 2, 3, 4, 6]),
        "anime" | "movie" | "drama" => Some(vec![2, 6]),
        "games" | "game" => Some(vec![4]),
        "music" | "cd" => Some(vec![3]),
        "book" | "books" => Some(vec![1]),
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
        titles.insert("original".to_string(), name.to_string());
    }
    let mut metadata = Map::new();
    metadata.insert("bgm_url".to_string(), Value::String(url.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if !name.is_empty() && name != title {
        metadata.insert(
            "title_original".to_string(),
            Value::String(name.to_string()),
        );
    }
    if !name_cn.is_empty() {
        metadata.insert("title".to_string(), Value::String(name_cn.to_string()));
    }
    if !release_date.is_empty() {
        metadata.insert(
            "release_date".to_string(),
            Value::String(release_date.to_string()),
        );
    }
    if let Some(episodes) = item.get("total_episodes").and_then(Value::as_i64) {
        if episodes > 0 {
            metadata.insert("episodes".to_string(), Value::Number(episodes.into()));
        }
    }
    Some(ExternalCandidate {
        provider: "bangumi".to_string(),
        source_id: id,
        url,
        title: title.to_string(),
        subtitle: (!release_date.is_empty()).then(|| release_date.to_string()),
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

async fn search_igdb(
    state: &AppState,
    q: &str,
    entity_type: &str,
    page: usize,
    page_size: usize,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !matches!(entity_type, "all" | "games" | "game") {
        return Ok(Vec::new());
    }
    let Some((client_id, client_secret)) = igdb_credentials() else {
        return Ok(Vec::new());
    };
    let client = reqwest::Client::new();
    let token = igdb_access_token(state, &client, &client_id, &client_secret, false).await?;
    let offset = (page - 1) * page_size;
    let body = format!(
        r#"fields name,url,summary,storyline,first_release_date,cover.url,genres.name,platforms.name; search "{}"; limit {page_size}; offset {offset};"#,
        q.replace('"', "\\\"")
    );
    let response = client
        .post("https://api.igdb.com/v4/games")
        .header("Client-ID", &client_id)
        .bearer_auth(token)
        .body(body.clone())
        .send()
        .await
        .map_err(provider_error)?;
    let response = if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        state.invalidate_access_token("igdb").await;
        let token = igdb_access_token(state, &client, &client_id, &client_secret, true).await?;
        client
            .post("https://api.igdb.com/v4/games")
            .header("Client-ID", &client_id)
            .bearer_auth(token)
            .body(body)
            .send()
            .await
            .map_err(provider_error)?
    } else {
        response
    };
    let data = response
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?
        .as_array()
        .cloned()
        .unwrap_or_default();
    Ok(data.iter().filter_map(igdb_candidate).collect())
}

async fn igdb_access_token(
    state: &AppState,
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    force_refresh: bool,
) -> Result<String, ApiError> {
    if !force_refresh {
        if let Some(token) = state.cached_access_token("igdb").await {
            return Ok(token.access_token);
        }
    }
    let value = client
        .post("https://id.twitch.tv/oauth2/token")
        .query(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "client_credentials"),
        ])
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            ApiError::bad_request("IGDB token response did not include an access token")
        })?;
    let expires_in = value
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(3600)
        .saturating_sub(60)
        .max(60);
    let expires_at_unix_seconds = unix_seconds_now() + expires_in;
    state
        .store_access_token(
            "igdb",
            CachedAccessToken {
                access_token: access_token.clone(),
                refresh_token: value
                    .get("refresh_token")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                expires_at: Instant::now() + Duration::from_secs(expires_in),
                expires_at_unix_seconds,
            },
        )
        .await
        .map_err(|error| ApiError::bad_request(&format!("failed to cache IGDB token: {error}")))?;
    Ok(access_token)
}

fn igdb_credentials() -> Option<(String, String)> {
    let client_id = std::env::var("KIZUNASHELF_IGDB_CLIENT_ID").ok()?;
    let client_secret = std::env::var("KIZUNASHELF_IGDB_CLIENT_SECRET").ok()?;
    (!client_id.is_empty() && !client_secret.is_empty()).then_some((client_id, client_secret))
}

fn igdb_candidate(item: &Value) -> Option<ExternalCandidate> {
    let title = item.get("name")?.as_str()?.to_string();
    let url = item
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let source_id = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            item.get("id")
                .and_then(Value::as_i64)
                .map(|id| id.to_string())
        })?;
    let release_date = item
        .get("first_release_date")
        .and_then(Value::as_i64)
        .and_then(|timestamp| chrono::DateTime::from_timestamp(timestamp, 0))
        .map(|date| date.format("%Y-%m-%d").to_string());
    let cover_url = item
        .get("cover")
        .and_then(|cover| cover.get("url"))
        .and_then(Value::as_str)
        .map(|value| format!("https:{}", value.replace("t_thumb", "t_cover_big")));
    let mut metadata = Map::new();
    if !url.is_empty() {
        metadata.insert("igdb_url".to_string(), Value::String(url.clone()));
    }
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(release_date) = &release_date {
        metadata.insert(
            "release_date".to_string(),
            Value::String(release_date.clone()),
        );
    }
    Some(ExternalCandidate {
        provider: "igdb".to_string(),
        source_id,
        url,
        title,
        subtitle: release_date,
        brief: item
            .get("summary")
            .or_else(|| item.get("storyline"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

async fn search_thetvdb(
    state: &AppState,
    q: &str,
    entity_type: &str,
    page_size: usize,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !matches!(entity_type, "all" | "anime" | "drama" | "movie") {
        return Ok(Vec::new());
    }
    let Some(api_key) = std::env::var("KIZUNASHELF_TVDB_API_KEY")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        return Ok(Vec::new());
    };
    let client = reqwest::Client::new();
    let mut login = Map::new();
    login.insert("apikey".to_string(), Value::String(api_key));
    if let Ok(pin) = std::env::var("KIZUNASHELF_TVDB_PIN") {
        if !pin.is_empty() {
            login.insert("pin".to_string(), Value::String(pin));
        }
    }
    let token = thetvdb_access_token(state, &client, &login, false).await?;
    let type_filter = match entity_type {
        "movie" => Some("movie"),
        "anime" | "drama" => Some("series"),
        _ => None,
    };
    let mut request = client
        .get("https://api4.thetvdb.com/v4/search")
        .bearer_auth(token)
        .query(&[("query", q)]);
    if let Some(type_filter) = type_filter {
        request = request.query(&[("type", type_filter)]);
    }
    let response = request.send().await.map_err(provider_error)?;
    let response = if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        state.invalidate_access_token("thetvdb").await;
        let token = thetvdb_access_token(state, &client, &login, true).await?;
        let mut request = client
            .get("https://api4.thetvdb.com/v4/search")
            .bearer_auth(token)
            .query(&[("query", q)]);
        if let Some(type_filter) = type_filter {
            request = request.query(&[("type", type_filter)]);
        }
        request.send().await.map_err(provider_error)?
    } else {
        response
    };
    let data = response
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(data
        .iter()
        .take(page_size)
        .filter_map(thetvdb_candidate)
        .collect())
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
    metadata.insert("thetvdb_url".to_string(), Value::String(url.clone()));
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(release_date) = &release_date {
        metadata.insert(
            "release_date".to_string(),
            Value::String(release_date.clone()),
        );
    }
    Some(ExternalCandidate {
        provider: "thetvdb".to_string(),
        source_id,
        url,
        title,
        subtitle: release_date,
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

fn provider_error(error: reqwest::Error) -> ApiError {
    ApiError::bad_request(&format!("External provider request failed: {error}"))
}
