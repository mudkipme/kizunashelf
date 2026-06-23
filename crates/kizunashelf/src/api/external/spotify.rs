use super::{
    external_client, field_option, provider_error, type_option, CredentialSpec, ExternalProvider,
    ProviderSearchConfig, USER_AGENT,
};
use crate::api::state::{unix_seconds_now, AppState, CachedAccessToken};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use crate::secrets::{SECRET_SPOTIFY_CLIENT_ID, SECRET_SPOTIFY_CLIENT_SECRET};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub(super) struct SpotifyProvider;

impl ExternalProvider for SpotifyProvider {
    const ID: &'static str = "spotify";
    const LABEL: &'static str = "Spotify";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        !spotify_types(provider_config).is_empty()
    }

    fn credentials() -> &'static [CredentialSpec] {
        &[
            CredentialSpec {
                key: SECRET_SPOTIFY_CLIENT_ID,
                label: "Spotify Client ID",
                secret: false,
                required: true,
            },
            CredentialSpec {
                key: SECRET_SPOTIFY_CLIENT_SECRET,
                label: "Spotify Client Secret",
                secret: true,
                required: true,
            },
        ]
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        spotify_credentials(state)
            .is_none()
            .then(|| "Set the Spotify client ID and client secret".to_string())
    }

    fn available(state: &AppState) -> bool {
        spotify_credentials(state).is_some()
    }

    fn field_options() -> Vec<ExternalProviderFieldOption> {
        field_options()
    }

    fn type_options() -> Vec<ExternalProviderTypeOption> {
        type_options()
    }

    async fn search(
        state: &AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        search_spotify(state, q, page, page_size, provider_config).await
    }
}

fn spotify_credentials(state: &AppState) -> Option<(String, String)> {
    let store = state.secret_store();
    let client_id = store.get(SECRET_SPOTIFY_CLIENT_ID)?;
    let client_secret = store.get(SECRET_SPOTIFY_CLIENT_SECRET)?;
    (!client_id.is_empty() && !client_secret.is_empty()).then_some((client_id, client_secret))
}

/// The Spotify entity kinds (`album`, `artist`) this field searches.
/// Unconstrained defaults to `album` (the music case); `artist` is opt-in.
fn spotify_types(provider_config: &ProviderSearchConfig) -> Vec<&'static str> {
    let Some(types) = provider_config.external_types() else {
        return vec!["album"];
    };
    let mut entities = Vec::new();
    for kind in ["album", "artist"] {
        if types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case(kind))
            && !entities.contains(&kind)
        {
            entities.push(kind);
        }
    }
    entities
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("title", "Title / Name"),
        field_option("release_date", "Release date"),
        field_option("label", "Label"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("artists", "Artists"),
        field_option("genres", "Genres"),
        field_option("company", "Companies (copyright)"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![
        type_option("album", "Album"),
        type_option("artist", "Artist"),
    ]
}

async fn search_spotify(
    state: &AppState,
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let types = spotify_types(provider_config);
    if types.is_empty() {
        return Ok(Vec::new());
    }
    let Some((client_id, client_secret)) = spotify_credentials(state) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    let token = spotify_access_token(state, client, &client_id, &client_secret, false).await?;
    // A pasted Spotify URL resolves a single album/artist.
    if let Some((kind, id)) = spotify_ref(q) {
        let endpoint = format!("https://api.spotify.com/v1/{kind}s/{id}");
        let value = spotify_get(
            state,
            client,
            &endpoint,
            &[],
            &client_id,
            &client_secret,
            token,
        )
        .await?;
        let candidate = if kind == "artist" {
            spotify_artist(&value)
        } else {
            spotify_album(&value)
        };
        return Ok(candidate.into_iter().collect());
    }
    let offset = (page - 1) * page_size;
    let value = spotify_get(
        state,
        client,
        "https://api.spotify.com/v1/search",
        &[
            ("q", q.to_string()),
            ("type", types.join(",")),
            ("limit", page_size.to_string()),
            ("offset", offset.to_string()),
        ],
        &client_id,
        &client_secret,
        token,
    )
    .await?;
    let mut items = Vec::new();
    if let Some(albums) = value
        .get("albums")
        .and_then(|albums| albums.get("items"))
        .and_then(Value::as_array)
    {
        items.extend(albums.iter().filter_map(spotify_album));
    }
    if let Some(artists) = value
        .get("artists")
        .and_then(|artists| artists.get("items"))
        .and_then(Value::as_array)
    {
        items.extend(artists.iter().filter_map(spotify_artist));
    }
    Ok(items)
}

/// GETs a Spotify endpoint with the bearer token, refreshing once on a 401.
async fn spotify_get(
    state: &AppState,
    client: &reqwest::Client,
    url: &str,
    query: &[(&str, String)],
    client_id: &str,
    client_secret: &str,
    token: String,
) -> Result<Value, ApiError> {
    let request = |token: &str| {
        client
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .query(query)
            .bearer_auth(token)
    };
    let response = request(&token).send().await.map_err(provider_error)?;
    let response = if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        state.invalidate_access_token("spotify").await;
        let token = spotify_access_token(state, client, client_id, client_secret, true).await?;
        request(&token).send().await.map_err(provider_error)?
    } else {
        response
    };
    response
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)
}

/// Detects a `open.spotify.com/[locale/]{album|artist}/{id}` URL.
fn spotify_ref(q: &str) -> Option<(&'static str, String)> {
    let trimmed = q.trim();
    let (_, rest) = trimmed.split_once("open.spotify.com/")?;
    let segments: Vec<&str> = rest.split('/').collect();
    // The kind/id pair is the last two path segments (an optional locale precedes).
    let position = segments
        .iter()
        .position(|segment| *segment == "album" || *segment == "artist")?;
    let kind = match segments[position] {
        "album" => "album",
        "artist" => "artist",
        _ => return None,
    };
    let id = segments
        .get(position + 1)?
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    (!id.is_empty()).then(|| (kind, id.to_string()))
}

fn spotify_album(item: &Value) -> Option<ExternalCandidate> {
    let id = item.get("id").and_then(Value::as_str)?.to_string();
    let title = item
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = spotify_url(item, "album", &id);
    let cover_url = spotify_image(item);
    let artists = spotify_artist_names(item.get("artists"));

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(date) = item
        .get("release_date")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("release_date".to_string(), Value::String(date.to_string()));
    }
    if !artists.is_empty() {
        metadata.insert(
            "artists".to_string(),
            Value::Array(artists.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(genres) = string_list(item.get("genres")) {
        metadata.insert("genres".to_string(), genres);
    }
    // Album detail carries copyright/label text; search results don't.
    if let Some(label) = item
        .get("label")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("label".to_string(), Value::String(label.to_string()));
    }
    // Copyright lines are the "company" list (publisher/(P)/(C) holders).
    let companies: Vec<Value> = item
        .get("copyrights")
        .and_then(Value::as_array)
        .map(|copyrights| {
            let mut texts = Vec::new();
            for copyright in copyrights {
                if let Some(text) = copyright
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    let text = Value::String(text.to_string());
                    if !texts.contains(&text) {
                        texts.push(text);
                    }
                }
            }
            texts
        })
        .unwrap_or_default();
    if !companies.is_empty() {
        metadata.insert("company".to_string(), Value::Array(companies));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: SpotifyProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(title.clone()),
        title,
        brief: (!artists.is_empty()).then(|| artists.join(", ")),
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

fn spotify_artist(item: &Value) -> Option<ExternalCandidate> {
    let id = item.get("id").and_then(Value::as_str)?.to_string();
    let title = item
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = spotify_url(item, "artist", &id);
    let cover_url = spotify_image(item);

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(genres) = string_list(item.get("genres")) {
        metadata.insert("genres".to_string(), genres);
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: SpotifyProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(title.clone()),
        title,
        brief: None,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

fn spotify_url(item: &Value, kind: &str, id: &str) -> String {
    item.get("external_urls")
        .and_then(|urls| urls.get("spotify"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://open.spotify.com/{kind}/{id}"))
}

fn spotify_image(item: &Value) -> Option<String> {
    item.get("images")
        .and_then(Value::as_array)
        .and_then(|images| images.first())
        .and_then(|image| image.get("url"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn spotify_artist_names(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|artists| {
            artists
                .iter()
                .filter_map(|artist| artist.get("name").and_then(Value::as_str))
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn string_list(value: Option<&Value>) -> Option<Value> {
    let items: Vec<Value> = value?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| Value::String(text.to_string()))
        .collect();
    (!items.is_empty()).then_some(Value::Array(items))
}

async fn spotify_access_token(
    state: &AppState,
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    force_refresh: bool,
) -> Result<String, ApiError> {
    if !force_refresh {
        if let Some(token) = state.cached_access_token("spotify").await {
            return Ok(token.access_token);
        }
    }
    // Single-flight the token fetch so concurrent searches don't stampede the
    // Spotify accounts endpoint on a cold cache.
    let fetch_lock = state.token_fetch_lock("spotify").await;
    let _guard = fetch_lock.lock().await;
    if let Some(token) = state.cached_access_token("spotify").await {
        return Ok(token.access_token);
    }
    let value = client
        .post("https://accounts.spotify.com/api/token")
        // `basic_auth` base64-encodes `client_id:client_secret` into the
        // Authorization header, so the secret never appears in the URL or body.
        .basic_auth(client_id, Some(client_secret))
        .form(&[("grant_type", "client_credentials")])
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
            ApiError::bad_request("Spotify token response did not include an access token")
        })?;
    let expires_in = value
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(3600)
        .saturating_sub(60)
        .max(60);
    state
        .store_access_token(
            "spotify",
            CachedAccessToken {
                access_token: access_token.clone(),
                refresh_token: None,
                expires_at: Instant::now() + Duration::from_secs(expires_in),
                expires_at_unix_seconds: unix_seconds_now() + expires_in,
            },
        )
        .await
        .map_err(|error| {
            ApiError::bad_request(&format!("failed to cache Spotify token: {error}"))
        })?;
    Ok(access_token)
}

#[cfg(test)]
mod tests {
    use super::{spotify_album, spotify_ref};
    use serde_json::json;

    #[test]
    fn album_surfaces_metadata() {
        let candidate = spotify_album(&json!({
            "id": "4aawyAB9vmqN3uQ7FjRGTy",
            "name": "Global Warming",
            "release_date": "2012-11-16",
            "artists": [{ "name": "Pitbull" }],
            "label": "RCA Records",
            "external_urls": { "spotify": "https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy" },
            "images": [{ "url": "https://i.scdn.co/image/cover.jpg" }]
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "4aawyAB9vmqN3uQ7FjRGTy");
        assert_eq!(candidate.metadata.get("artists"), Some(&json!(["Pitbull"])));
        assert_eq!(
            candidate.metadata.get("release_date"),
            Some(&json!("2012-11-16"))
        );
        assert_eq!(candidate.metadata.get("label"), Some(&json!("RCA Records")));
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://i.scdn.co/image/cover.jpg")
        );
    }

    #[test]
    fn ref_parses_album_and_artist_urls() {
        assert_eq!(
            spotify_ref("https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy?si=x"),
            Some(("album", "4aawyAB9vmqN3uQ7FjRGTy".to_string()))
        );
        assert_eq!(
            spotify_ref("https://open.spotify.com/intl-ja/artist/0TnOYISbd1XYRBk9myaseg"),
            Some(("artist", "0TnOYISbd1XYRBk9myaseg".to_string()))
        );
        assert_eq!(spotify_ref("pitbull"), None);
    }
}
