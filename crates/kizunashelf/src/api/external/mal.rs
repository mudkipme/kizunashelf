use super::{
    external_client, field_option, insert_str, named_list, provider_error, type_option,
    CredentialSpec, ExternalProvider, ProviderSearchConfig, USER_AGENT,
};
use crate::api::state::AppState;
use crate::api::ApiError;
use crate::contract::{
    ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption,
    ProviderEpisodeGroup, ProviderEpisodeItem, ProviderEpisodes,
};
use crate::secrets::SECRET_MAL_CLIENT_ID;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct MyAnimeListProvider;

const BASE: &str = "https://api.myanimelist.net/v2";
const DETAIL_FIELDS: &str = "title,main_picture,media_type,start_date,end_date,synopsis,status,\
genres,mean,num_episodes,num_chapters,average_episode_duration,studios,start_season,source";

impl ExternalProvider for MyAnimeListProvider {
    const ID: &'static str = "myanimelist";
    const LABEL: &'static str = "MyAnimeList";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        !mal_types(provider_config).is_empty()
    }

    fn credentials() -> &'static [CredentialSpec] {
        &[CredentialSpec {
            key: SECRET_MAL_CLIENT_ID,
            label: "MyAnimeList Client ID",
            secret: false,
            required: true,
        }]
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        mal_client_id(state)
            .is_none()
            .then(|| "Set the MyAnimeList client ID".to_string())
    }

    fn available(state: &AppState) -> bool {
        mal_client_id(state).is_some()
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
        search_mal(state, q, page, page_size, provider_config).await
    }

    const SUPPORTS_EPISODES: bool = true;

    async fn fetch_episodes(
        _state: &AppState,
        ref_value: &str,
        _language: Option<&str>,
    ) -> Result<ProviderEpisodes, ApiError> {
        fetch_mal_episodes(ref_value).await
    }
}

/// The MAL anime id from a stored ref: an `…/anime/{id}` URL, or a bare numeric id
/// (assumed anime). A manga link has no episode list and yields `None`.
fn mal_anime_id(ref_value: &str) -> Option<String> {
    match mal_ref(ref_value) {
        Some(("anime", id)) => Some(id),
        Some(_) => None,
        None => {
            let trimmed = ref_value.trim();
            (!trimmed.is_empty() && trimmed.chars().all(|character| character.is_ascii_digit()))
                .then(|| trimmed.to_string())
        }
    }
}

/// Fetches an anime's episodes as one flat list. MAL's official v2 API exposes only
/// an episode *count*, not titles, so this uses the keyless Jikan API
/// (`/v4/anime/{id}/episodes`). The provider is still gated on the MAL client id
/// being configured (it's the search credential), keeping source selection uniform.
async fn fetch_mal_episodes(ref_value: &str) -> Result<ProviderEpisodes, ApiError> {
    let id = mal_anime_id(ref_value)
        .ok_or_else(|| ApiError::bad_request("Not a MyAnimeList anime link or id"))?;
    let client = external_client();
    let mut items: Vec<ProviderEpisodeItem> = Vec::new();
    let mut page = 1usize;
    // Page through Jikan (100/page) until it reports no next page, with a hard cap
    // so a malformed response can't loop forever.
    loop {
        let value = client
            .get(format!(
                "https://api.jikan.moe/v4/anime/{id}/episodes?page={page}"
            ))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        let Some(data) = value.get("data").and_then(Value::as_array) else {
            break;
        };
        if data.is_empty() {
            break;
        }
        items.extend(jikan_episode_items(data));
        let has_next = value
            .pointer("/pagination/has_next_page")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        page += 1;
        if !has_next || page > 50 {
            break;
        }
    }
    Ok(ProviderEpisodes {
        groups: vec![ProviderEpisodeGroup {
            label: String::new(),
            items,
        }],
    })
}

/// Maps a Jikan `/episodes` page (`data[]`) into items keyed by the MAL episode id
/// (its 1-based number within the anime).
fn jikan_episode_items(data: &[Value]) -> Vec<ProviderEpisodeItem> {
    data.iter()
        .filter_map(|episode| {
            let key = episode.get("mal_id").and_then(Value::as_i64)?;
            let title = episode
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            Some(ProviderEpisodeItem {
                key: key.to_string(),
                title,
            })
        })
        .collect()
}

fn mal_client_id(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_MAL_CLIENT_ID)
        .filter(|value| !value.is_empty())
}

/// The MAL media kinds (`anime`, `manga`) this field searches. Unconstrained
/// searches both.
fn mal_types(provider_config: &ProviderSearchConfig) -> Vec<&'static str> {
    let Some(types) = provider_config.external_types() else {
        return vec!["anime", "manga"];
    };
    let mut media = Vec::new();
    for kind in ["anime", "manga"] {
        if types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case(kind))
            && !media.contains(&kind)
        {
            media.push(kind);
        }
    }
    media
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("title", "Title"),
        field_option("cover_url", "Cover URL"),
        field_option("format", "Type"),
        field_option("start_date", "Start date"),
        field_option("end_date", "End date"),
        field_option("status", "Status"),
        field_option("episodes", "Episodes"),
        field_option("chapters", "Chapters"),
        field_option("runtime", "Runtime (minutes)"),
        field_option("season", "Season"),
        field_option("source", "Source material"),
        field_option("score", "Score"),
        field_option("synopsis", "Synopsis"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("genres", "Genres"),
        field_option("studios", "Studios"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("anime", "Anime"), type_option("manga", "Manga")]
}

async fn search_mal(
    state: &AppState,
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let media = mal_types(provider_config);
    if media.is_empty() {
        return Ok(Vec::new());
    }
    let Some(client_id) = mal_client_id(state) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    // A pasted MyAnimeList URL resolves a single record (with full fields).
    if let Some((media_type, id)) = mal_ref(q) {
        return resolve_mal(client, &client_id, media_type, &id).await;
    }
    let offset = (page - 1) * page_size;
    let mut items = Vec::new();
    for media_type in media {
        let value = client
            .get(format!("{BASE}/{media_type}"))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header("X-MAL-CLIENT-ID", &client_id)
            .query(&[
                ("q", q),
                ("fields", "media_type"),
                ("limit", &page_size.to_string()),
                ("offset", &offset.to_string()),
            ])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        if let Some(data) = value.get("data").and_then(Value::as_array) {
            items.extend(
                data.iter()
                    .filter_map(|entry| mal_search_node(entry.get("node")?, media_type)),
            );
        }
    }
    Ok(items)
}

async fn resolve_mal(
    client: &reqwest::Client,
    client_id: &str,
    media_type: &str,
    id: &str,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let value = client
        .get(format!("{BASE}/{media_type}/{id}"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header("X-MAL-CLIENT-ID", client_id)
        .query(&[("fields", DETAIL_FIELDS)])
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    Ok(mal_detail(media_type, id, &value).into_iter().collect())
}

/// Detects a `myanimelist.net/{anime|manga}/{id}` URL.
fn mal_ref(q: &str) -> Option<(&'static str, String)> {
    let trimmed = q.trim();
    let (_, rest) = trimmed.split_once("myanimelist.net/")?;
    let mut parts = rest.split('/');
    let media_type = match parts.next()? {
        "anime" => "anime",
        "manga" => "manga",
        _ => return None,
    };
    let id = parts
        .next()?
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .then(|| (media_type, id.to_string()))
}

fn mal_search_node(node: &Value, media_type: &str) -> Option<ExternalCandidate> {
    let id = node.get("id").and_then(Value::as_i64)?.to_string();
    let title = node
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let cover_url = mal_image(node);
    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: MyAnimeListProvider::ID.to_string(),
        source_id: id.clone(),
        url: format!("https://myanimelist.net/{media_type}/{id}"),
        original_title: Some(title.clone()),
        title,
        brief: None,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

fn mal_detail(media_type: &str, id: &str, response: &Value) -> Option<ExternalCandidate> {
    let title = response
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let cover_url = mal_image(response);
    let synopsis = response
        .get("synopsis")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(format) = mal_format(response) {
        metadata.insert("format".to_string(), Value::String(format));
    }
    insert_str(&mut metadata, "start_date", response.get("start_date"));
    insert_str(&mut metadata, "end_date", response.get("end_date"));
    if let Some(status) = mal_status(response) {
        metadata.insert("status".to_string(), Value::String(status));
    }
    if let Some(episodes) = response
        .get("num_episodes")
        .and_then(Value::as_i64)
        .filter(|count| *count > 0)
    {
        metadata.insert("episodes".to_string(), Value::Number(episodes.into()));
    }
    if let Some(chapters) = response
        .get("num_chapters")
        .and_then(Value::as_i64)
        .filter(|count| *count > 0)
    {
        metadata.insert("chapters".to_string(), Value::Number(chapters.into()));
    }
    // `average_episode_duration` is in seconds; surface whole minutes.
    if let Some(minutes) = response
        .get("average_episode_duration")
        .and_then(Value::as_i64)
        .filter(|seconds| *seconds > 0)
        .map(|seconds| seconds / 60)
        .filter(|minutes| *minutes > 0)
    {
        metadata.insert("runtime".to_string(), Value::Number(minutes.into()));
    }
    if let Some(season) = response.get("start_season") {
        if let (Some(name), Some(year)) = (
            season.get("season").and_then(Value::as_str),
            season.get("year").and_then(Value::as_i64),
        ) {
            metadata.insert(
                "season".to_string(),
                Value::String(format!("{} {year}", title_case(name))),
            );
        }
    }
    if let Some(source) = response
        .get("source")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("source".to_string(), Value::String(title_case(source)));
    }
    if let Some(score) = response
        .get("mean")
        .and_then(Value::as_f64)
        .filter(|score| *score > 0.0)
    {
        metadata.insert("score".to_string(), serde_json::json!(score));
    }
    if let Some(genres) = named_list(response.get("genres")) {
        metadata.insert("genres".to_string(), genres);
    }
    if let Some(studios) = named_list(response.get("studios")) {
        metadata.insert("studios".to_string(), studios);
    }
    if let Some(synopsis) = &synopsis {
        metadata.insert("synopsis".to_string(), Value::String(synopsis.clone()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: MyAnimeListProvider::ID.to_string(),
        source_id: id.to_string(),
        url: format!("https://myanimelist.net/{media_type}/{id}"),
        original_title: Some(title.clone()),
        title,
        brief: synopsis,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

fn mal_image(value: &Value) -> Option<String> {
    value
        .get("main_picture")
        .and_then(|picture| picture.get("large").or_else(|| picture.get("medium")))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Maps MAL's `media_type` to a readable format (tv→Anime, ova/ona uppercased).
fn mal_format(response: &Value) -> Option<String> {
    let media_type = response.get("media_type").and_then(Value::as_str)?;
    let format = match media_type {
        "" => return None,
        "tv" => "Anime".to_string(),
        "ova" | "ona" => media_type.to_uppercase(),
        other => title_case(other),
    };
    Some(format)
}

/// Maps MAL's machine status to a human-readable label.
fn mal_status(response: &Value) -> Option<String> {
    let status = response.get("status").and_then(Value::as_str)?;
    let readable = match status {
        "finished_airing" | "finished" => "Finished",
        "currently_airing" => "Airing",
        "currently_publishing" => "Publishing",
        "not_yet_aired" | "not_yet_published" => "Upcoming",
        "on_hiatus" => "On Hiatus",
        "discontinued" => "Discontinued",
        "" => return None,
        other => return Some(title_case(other)),
    };
    Some(readable.to_string())
}

/// Title-cases an underscore/space separated string (`light_novel` → `Light Novel`).
fn title_case(value: &str) -> String {
    value
        .split(['_', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{jikan_episode_items, mal_anime_id, mal_detail, mal_ref};
    use serde_json::json;

    #[test]
    fn anime_detail_surfaces_metadata() {
        let candidate = mal_detail(
            "anime",
            "1",
            &json!({
                "title": "Cowboy Bebop",
                "main_picture": { "large": "https://img/cb.jpg" },
                "media_type": "tv",
                "start_date": "1998-04-03",
                "status": "finished_airing",
                "num_episodes": 26,
                "average_episode_duration": 1440,
                "start_season": { "season": "spring", "year": 1998 },
                "source": "original",
                "mean": 8.75,
                "genres": [{ "name": "Action" }, { "name": "Sci-Fi" }],
                "studios": [{ "name": "Sunrise" }]
            }),
        )
        .unwrap();

        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("format"), Some(&json!("Anime")));
        assert_eq!(metadata.get("status"), Some(&json!("Finished")));
        assert_eq!(metadata.get("episodes"), Some(&json!(26)));
        assert_eq!(metadata.get("runtime"), Some(&json!(24)));
        assert_eq!(metadata.get("season"), Some(&json!("Spring 1998")));
        assert_eq!(metadata.get("source"), Some(&json!("Original")));
        assert_eq!(metadata.get("score"), Some(&json!(8.75)));
        assert_eq!(metadata.get("studios"), Some(&json!(["Sunrise"])));
        assert_eq!(candidate.url, "https://myanimelist.net/anime/1");
    }

    #[test]
    fn ref_parses_anime_and_manga_urls() {
        assert_eq!(
            mal_ref("https://myanimelist.net/anime/1/Cowboy_Bebop"),
            Some(("anime", "1".to_string()))
        );
        assert_eq!(
            mal_ref("https://myanimelist.net/manga/2/Berserk"),
            Some(("manga", "2".to_string()))
        );
        assert_eq!(mal_ref("cowboy bebop"), None);
    }

    #[test]
    fn anime_id_accepts_anime_url_and_bare_id_but_not_manga() {
        assert_eq!(
            mal_anime_id("https://myanimelist.net/anime/1/Cowboy_Bebop"),
            Some("1".to_string())
        );
        assert_eq!(mal_anime_id("1"), Some("1".to_string()));
        assert_eq!(
            mal_anime_id("https://myanimelist.net/manga/2/Berserk"),
            None
        );
    }

    #[test]
    fn jikan_items_key_by_mal_id() {
        let data = vec![
            json!({ "mal_id": 1, "title": "Asteroid Blues" }),
            json!({ "mal_id": 2, "title": "Stray Dog Strut" }),
        ];
        let items = jikan_episode_items(&data);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].key, "1");
        assert_eq!(items[0].title, "Asteroid Blues");
        assert_eq!(items[1].key, "2");
    }
}
