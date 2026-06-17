use super::{external_client, provider_error, ExternalProvider, ProviderSearchConfig};
use crate::api::state::{unix_seconds_now, AppState, CachedAccessToken};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use crate::secrets::{SECRET_IGDB_CLIENT_ID, SECRET_IGDB_CLIENT_SECRET};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub(super) struct IgdbProvider;

impl ExternalProvider for IgdbProvider {
    const ID: &'static str = "igdb";
    const LABEL: &'static str = "IGDB";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        igdb_external_types_match(provider_config)
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        igdb_credentials(state)
            .is_none()
            .then(|| "Set the IGDB client ID and client secret".to_string())
    }

    fn available(state: &AppState) -> bool {
        igdb_credentials(state).is_some()
    }

    async fn search(
        state: &AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        search_igdb(state, q, page, page_size, provider_config).await
    }
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
    let Some((client_id, client_secret)) = igdb_credentials(state) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    let token = igdb_access_token(state, client, &client_id, &client_secret, false).await?;
    let offset = (page - 1) * page_size;
    let body = igdb_query_body(q, page_size, offset);
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
        let token = igdb_access_token(state, client, &client_id, &client_secret, true).await?;
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

pub(super) fn igdb_external_types_match(provider_config: &ProviderSearchConfig) -> bool {
    let Some(external_types) = provider_config.external_types() else {
        return true;
    };
    external_types
        .iter()
        .any(|external_type| matches!(external_type.trim().to_ascii_lowercase().as_str(), "game"))
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("name", "Name"),
        field_option("cover_url", "Cover URL"),
        field_option("first_release_date", "First release date"),
        field_option("summary", "Summary"),
        field_option("storyline", "Storyline"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("game", "Game")]
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

fn igdb_query_body(q: &str, page_size: usize, offset: usize) -> String {
    let fields = "fields name,url,summary,storyline,first_release_date,cover.url,genres.name,platforms.name;";
    let trimmed = q.trim();
    if trimmed.chars().all(|character| character.is_ascii_digit()) {
        return format!("{fields} where id = {trimmed}; limit {page_size}; offset {offset};");
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return format!(
            r#"{fields} where url = "{}"; limit {page_size}; offset {offset};"#,
            escape_igdb_string(trimmed)
        );
    }
    format!(
        r#"{fields} search "{}"; limit {page_size}; offset {offset};"#,
        escape_igdb_string(trimmed)
    )
}

fn escape_igdb_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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

fn igdb_credentials(state: &AppState) -> Option<(String, String)> {
    let store = state.secret_store();
    let client_id = store.get(SECRET_IGDB_CLIENT_ID)?;
    let client_secret = store.get(SECRET_IGDB_CLIENT_SECRET)?;
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
    metadata.insert("name".to_string(), Value::String(title.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(release_date) = &release_date {
        metadata.insert(
            "first_release_date".to_string(),
            Value::String(release_date.clone()),
        );
    }
    if let Some(summary) = item
        .get("summary")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("summary".to_string(), Value::String(summary.to_string()));
    }
    if let Some(storyline) = item
        .get("storyline")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "storyline".to_string(),
            Value::String(storyline.to_string()),
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
