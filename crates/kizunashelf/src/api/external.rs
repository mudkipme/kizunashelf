use super::error::{ApiError, ApiResult};
use crate::contract::{ExternalCandidate, ExternalProviderSummary, ExternalSearchResponse};
use crate::dates::clamp_number;
use crate::types::{FieldType, KizunaConfig};
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

use super::state::{get_library, unix_seconds_now, AppState, CachedAccessToken};
use std::time::{Duration, Instant};

const USER_AGENT: &str = concat!("KizunaShelf/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, Default)]
struct ProviderSearchConfig {
    unconstrained: bool,
    external_types: BTreeSet<String>,
}

impl ProviderSearchConfig {
    fn add_external_types(&mut self, external_types: &[String]) {
        let external_types = external_types
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        if external_types.is_empty() {
            self.unconstrained = true;
            return;
        }
        self.external_types
            .extend(external_types.into_iter().map(str::to_string));
    }

    fn external_types(&self) -> Option<&BTreeSet<String>> {
        (!self.unconstrained).then_some(&self.external_types)
    }
}

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
    let requested_provider = query.provider.as_deref().filter(|value| *value != "all");
    if let Some(provider) = requested_provider {
        if !is_known_provider(provider) {
            return Err(ApiError::bad_request("Unknown external provider"));
        }
    }

    let library = get_library(&state).await?;
    let Some(entity_type) = query
        .entity_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "all")
    else {
        return Err(ApiError::bad_request(
            "External search requires a concrete entity type",
        ));
    };
    if !library
        .config
        .types
        .iter()
        .any(|type_config| type_config.id == entity_type)
    {
        return Err(ApiError::bad_request("Unknown entity type"));
    }
    let configured_providers = configured_external_providers(&library.config, entity_type);
    let providers = provider_summaries(&configured_providers);

    let q = query.q.as_deref().unwrap_or_default().trim();
    if q.is_empty() {
        return Ok(Json(ExternalSearchResponse {
            providers,
            items: Vec::new(),
        }));
    }
    let page_size = clamp_number(query.page_size.unwrap_or(10.0), 1, 25) as usize;
    let page = clamp_number(query.page.unwrap_or(1.0), 1, i64::MAX) as usize;

    let mut items = Vec::new();
    if should_search_provider(requested_provider, &providers, "bangumi") {
        if let Some(provider_config) = configured_providers.get("bangumi") {
            items.extend(search_bangumi(q, page, page_size, provider_config).await?);
        }
    }
    if should_search_provider(requested_provider, &providers, "igdb") {
        if let Some(provider_config) = configured_providers.get("igdb") {
            items.extend(search_igdb(&state, q, page, page_size, provider_config).await?);
        }
    }
    if should_search_provider(requested_provider, &providers, "thetvdb") {
        if let Some(provider_config) = configured_providers.get("thetvdb") {
            items.extend(search_thetvdb(&state, q, page_size, provider_config).await?);
        }
    }
    Ok(Json(ExternalSearchResponse { providers, items }))
}

fn provider_summaries(
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
) -> Vec<ExternalProviderSummary> {
    vec![
        ExternalProviderSummary {
            id: "bangumi".to_string(),
            label: "Bangumi".to_string(),
            enabled: provider_configured_and_supported(configured_providers, "bangumi"),
            reason: provider_reason(configured_providers, "bangumi", None),
        },
        ExternalProviderSummary {
            id: "igdb".to_string(),
            label: "IGDB".to_string(),
            enabled: provider_configured_and_supported(configured_providers, "igdb")
                && igdb_credentials().is_some(),
            reason: provider_reason(
                configured_providers,
                "igdb",
                igdb_credentials().is_none().then(|| {
                    "Set KIZUNASHELF_IGDB_CLIENT_ID and KIZUNASHELF_IGDB_CLIENT_SECRET".to_string()
                }),
            ),
        },
        ExternalProviderSummary {
            id: "thetvdb".to_string(),
            label: "TheTVDB".to_string(),
            enabled: provider_configured_and_supported(configured_providers, "thetvdb")
                && std::env::var("KIZUNASHELF_TVDB_API_KEY").ok().is_some(),
            reason: provider_reason(
                configured_providers,
                "thetvdb",
                std::env::var("KIZUNASHELF_TVDB_API_KEY")
                    .ok()
                    .is_none()
                    .then(|| "Set KIZUNASHELF_TVDB_API_KEY".to_string()),
            ),
        },
    ]
}

fn provider_reason(
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
    provider: &'static str,
    unavailable_reason: Option<String>,
) -> Option<String> {
    if !configured_providers.contains_key(provider) {
        return Some("No externalRef field configured for this source".to_string());
    }
    if !provider_configured_and_supported(configured_providers, provider) {
        return Some("No supported externalTypes configured for this source".to_string());
    }
    unavailable_reason
}

fn provider_configured_and_supported(
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
    provider: &'static str,
) -> bool {
    let Some(provider_config) = configured_providers.get(provider) else {
        return false;
    };
    match provider {
        "bangumi" => bangumi_types(provider_config).is_some(),
        "igdb" => igdb_external_types_match(provider_config),
        "thetvdb" => thetvdb_type_filters(provider_config).is_some(),
        _ => false,
    }
}

fn should_search_provider(
    requested_provider: Option<&str>,
    providers: &[ExternalProviderSummary],
    provider: &str,
) -> bool {
    if requested_provider.is_some_and(|requested| requested != provider) {
        return false;
    }
    providers
        .iter()
        .any(|item| item.id == provider && item.enabled)
}

fn is_known_provider(provider: &str) -> bool {
    matches!(provider, "bangumi" | "igdb" | "thetvdb")
}

fn configured_external_providers(
    config: &KizunaConfig,
    entity_type: &str,
) -> BTreeMap<&'static str, ProviderSearchConfig> {
    let mut providers = BTreeMap::new();
    for type_config in &config.types {
        if type_config.id != entity_type {
            continue;
        }
        for field in &type_config.fields {
            if field.field_type != FieldType::ExternalRef {
                continue;
            }
            if let Some(provider) = field
                .external_ref
                .as_deref()
                .and_then(provider_for_external_ref)
            {
                providers
                    .entry(provider)
                    .or_insert_with(ProviderSearchConfig::default)
                    .add_external_types(&field.external_types);
            }
        }
    }
    providers
}

fn provider_for_external_ref(external_ref: &str) -> Option<&'static str> {
    match external_ref.trim().to_ascii_lowercase().as_str() {
        "bangumi" | "bgm" => Some("bangumi"),
        "igdb" => Some("igdb"),
        "thetvdb" | "tvdb" => Some("thetvdb"),
        _ => None,
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

fn bangumi_types(provider_config: &ProviderSearchConfig) -> Option<Vec<u32>> {
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

fn bangumi_type(external_type: &str) -> Option<u32> {
    match external_type.trim().to_ascii_lowercase().as_str() {
        "1" | "book" | "books" => Some(1),
        "2" | "anime" | "animation" => Some(2),
        "3" | "music" | "cd" => Some(3),
        "4" | "game" | "games" => Some(4),
        "6" | "real" | "drama" | "movie" => Some(6),
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

async fn search_igdb(
    state: &AppState,
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !igdb_external_types_match(provider_config) {
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

fn igdb_external_types_match(provider_config: &ProviderSearchConfig) -> bool {
    let Some(external_types) = provider_config.external_types() else {
        return true;
    };
    external_types.iter().any(|external_type| {
        matches!(
            external_type.trim().to_ascii_lowercase().as_str(),
            "game" | "games"
        )
    })
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
        original_title: Some(title.clone()),
        title,
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
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
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
    let Some(type_filters) = thetvdb_type_filters(provider_config) else {
        return Ok(Vec::new());
    };
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    for type_filter in type_filters {
        let data =
            search_thetvdb_type(state, &client, &login, &token, q, type_filter.as_deref()).await?;
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

fn thetvdb_type_filters(provider_config: &ProviderSearchConfig) -> Option<Vec<Option<String>>> {
    let Some(external_types) = provider_config.external_types() else {
        return Some(vec![None]);
    };
    let filters = external_types
        .iter()
        .filter_map(|external_type| thetvdb_type_filter(external_type).map(Some))
        .collect::<Vec<_>>();
    (!filters.is_empty()).then_some(filters)
}

fn thetvdb_type_filter(external_type: &str) -> Option<String> {
    match external_type.trim().to_ascii_lowercase().as_str() {
        "series" | "serie" | "show" | "tv" | "anime" | "drama" => Some("series".to_string()),
        "movie" | "movies" => Some("movie".to_string()),
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

fn provider_error(error: reqwest::Error) -> ApiError {
    ApiError::bad_request(&format!("External provider request failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EntityTypeConfig, FieldConfig};

    #[test]
    fn external_providers_are_derived_from_external_ref_fields() {
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            content_writable: None,
            read_concurrency: None,
            home: None,
            daily_notes: None,
            types: vec![
                entity_type("animation", "BGM Link", "bgm"),
                entity_type("interactive", "IGDB Link", "igdb"),
                entity_type("series", "TVDB Link", "tvdb"),
            ],
        };

        assert!(configured_external_providers(&config, "animation").contains_key("bangumi"));
        assert!(configured_external_providers(&config, "interactive").contains_key("igdb"));
        assert!(configured_external_providers(&config, "series").contains_key("thetvdb"));
        assert!(configured_external_providers(&config, "all").is_empty());
    }

    #[test]
    fn external_provider_type_gates_are_provider_specific() {
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            content_writable: None,
            read_concurrency: None,
            home: None,
            daily_notes: None,
            types: vec![
                entity_type_with_external_types("animation", "BGM Link", "bgm", &["anime"]),
                entity_type_with_external_types("series", "TVDB Link", "thetvdb", &["series"]),
                entity_type_with_external_types("video", "TVDB Link", "thetvdb", &["movie"]),
                entity_type_with_external_types("bad", "TVDB Link", "thetvdb", &["game"]),
            ],
        };

        let bangumi = configured_external_providers(&config, "animation");
        assert_eq!(
            bangumi_types(bangumi.get("bangumi").unwrap()),
            Some(vec![2])
        );

        let series = configured_external_providers(&config, "series");
        assert_eq!(
            thetvdb_type_filters(series.get("thetvdb").unwrap()),
            Some(vec![Some("series".to_string())])
        );

        let movie = configured_external_providers(&config, "video");
        assert_eq!(
            thetvdb_type_filters(movie.get("thetvdb").unwrap()),
            Some(vec![Some("movie".to_string())])
        );

        let invalid = configured_external_providers(&config, "bad");
        assert_eq!(thetvdb_type_filters(invalid.get("thetvdb").unwrap()), None);
    }

    #[test]
    fn unknown_type_has_no_configured_external_providers() {
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            content_writable: None,
            read_concurrency: None,
            home: None,
            daily_notes: None,
            types: vec![entity_type("animation", "Bangumi Link", "bangumi")],
        };

        assert!(configured_external_providers(&config, "anime").is_empty());
    }

    fn entity_type(id: &str, field: &str, external_ref: &str) -> EntityTypeConfig {
        entity_type_with_external_types(id, field, external_ref, &[])
    }

    fn entity_type_with_external_types(
        id: &str,
        field: &str,
        external_ref: &str,
        external_types: &[&str],
    ) -> EntityTypeConfig {
        EntityTypeConfig {
            id: id.to_string(),
            label: id.to_string(),
            icon: None,
            path: id.to_string(),
            filename: None,
            fields: vec![FieldConfig {
                field: field.to_string(),
                field_type: FieldType::ExternalRef,
                display_name: None,
                title_language: None,
                title_role: None,
                default_title: None,
                enum_options: Vec::new(),
                total_progress_field: None,
                date_role: None,
                season_language: None,
                external_ref: Some(external_ref.to_string()),
                external_types: external_types
                    .iter()
                    .map(|value| value.to_string())
                    .collect(),
                relation_type: None,
            }],
        }
    }
}
