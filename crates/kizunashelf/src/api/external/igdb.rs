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
        field_option("rating", "User rating"),
        field_option("aggregated_rating", "Critic rating"),
        field_option("total_rating", "Total rating"),
        field_option("franchise", "Franchise"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("alternative_names", "Alternative names"),
        field_option("genres", "Genres"),
        field_option("platforms", "Platforms"),
        field_option("themes", "Themes"),
        field_option("game_modes", "Game modes"),
        field_option("player_perspectives", "Player perspectives"),
        field_option("game_engines", "Game engines"),
        field_option("developers", "Developers"),
        field_option("publishers", "Publishers"),
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

/// Collects an array of `{ name }` references into a JSON string array, returning
/// `None` when the source is missing or empty.
fn named_list(value: Option<&Value>) -> Option<Value> {
    let names: Vec<Value> = value?
        .as_array()?
        .iter()
        .filter_map(|element| element.get("name").and_then(Value::as_str))
        .filter(|name| !name.is_empty())
        .map(|name| Value::String(name.to_string()))
        .collect();
    (!names.is_empty()).then_some(Value::Array(names))
}

fn type_option(value: &str, label: &str) -> ExternalProviderTypeOption {
    ExternalProviderTypeOption {
        value: value.to_string(),
        label: label.to_string(),
    }
}

fn igdb_query_body(q: &str, page_size: usize, offset: usize) -> String {
    let fields = "fields name,url,summary,storyline,first_release_date,cover.url,\
rating,aggregated_rating,total_rating,\
alternative_names.name,genres.name,platforms.name,themes.name,game_modes.name,\
player_perspectives.name,game_engines.name,franchises.name,collection.name,\
involved_companies.developer,involved_companies.publisher,involved_companies.company.name;";
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
    // Single-flight the token fetch: under concurrent searches a cold cache would
    // otherwise stampede the Twitch token endpoint and risk rate limits.
    let fetch_lock = state.token_fetch_lock("igdb").await;
    let _guard = fetch_lock.lock().await;
    // Another task may have populated the cache while we waited for the lock.
    if let Some(token) = state.cached_access_token("igdb").await {
        return Ok(token.access_token);
    }
    let value = client
        .post("https://id.twitch.tv/oauth2/token")
        // Credentials go in the form body, never the query string, so they are
        // not echoed back in any error/log carrying the request URL.
        .form(&[
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
    // Ratings are 0–100 floats; round to whole numbers for a tidy rating field.
    for key in ["rating", "aggregated_rating", "total_rating"] {
        if let Some(rating) = item
            .get(key)
            .and_then(Value::as_f64)
            .filter(|value| *value > 0.0)
        {
            metadata.insert(
                key.to_string(),
                Value::Number((rating.round() as i64).into()),
            );
        }
    }
    // `*.name` reference arrays → JSON string arrays for list-type fields.
    for key in [
        "alternative_names",
        "genres",
        "platforms",
        "themes",
        "game_modes",
        "player_perspectives",
        "game_engines",
    ] {
        if let Some(values) = named_list(item.get(key)) {
            metadata.insert(key.to_string(), values);
        }
    }
    // Franchise: prefer an explicit franchise, fall back to the collection name.
    if let Some(franchise) = item
        .get("franchises")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(|value| value.get("name"))
        .or_else(|| {
            item.get("collection")
                .and_then(|collection| collection.get("name"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "franchise".to_string(),
            Value::String(franchise.to_string()),
        );
    }
    if let Some(companies) = item.get("involved_companies").and_then(Value::as_array) {
        let company_names = |role: &str| -> Vec<Value> {
            companies
                .iter()
                .filter(|company| company.get(role).and_then(Value::as_bool).unwrap_or(false))
                .filter_map(|company| {
                    company
                        .get("company")
                        .and_then(|company| company.get("name"))
                        .and_then(Value::as_str)
                })
                .filter(|name| !name.is_empty())
                .map(|name| Value::String(name.to_string()))
                .collect()
        };
        let developers = company_names("developer");
        if !developers.is_empty() {
            metadata.insert("developers".to_string(), Value::Array(developers));
        }
        let publishers = company_names("publisher");
        if !publishers.is_empty() {
            metadata.insert("publishers".to_string(), Value::Array(publishers));
        }
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

#[cfg(test)]
mod tests {
    use super::igdb_candidate;
    use serde_json::json;

    #[test]
    fn candidate_surfaces_extended_metadata() {
        let candidate = igdb_candidate(&json!({
            "id": 1,
            "name": "Hollow Knight",
            "url": "https://www.igdb.com/games/hollow-knight",
            "total_rating": 91.4,
            "genres": [{ "name": "Platform" }, { "name": "Adventure" }],
            "game_modes": [{ "name": "Single player" }],
            "franchises": [{ "name": "Hollow Knight" }],
            "involved_companies": [
                { "developer": true, "publisher": true, "company": { "name": "Team Cherry" } },
                { "developer": false, "publisher": true, "company": { "name": "Some Publisher" } }
            ]
        }))
        .unwrap();

        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("total_rating"), Some(&json!(91)));
        assert_eq!(
            metadata.get("genres"),
            Some(&json!(["Platform", "Adventure"]))
        );
        assert_eq!(metadata.get("game_modes"), Some(&json!(["Single player"])));
        assert_eq!(metadata.get("franchise"), Some(&json!("Hollow Knight")));
        assert_eq!(metadata.get("developers"), Some(&json!(["Team Cherry"])));
        assert_eq!(
            metadata.get("publishers"),
            Some(&json!(["Team Cherry", "Some Publisher"]))
        );
    }
}
