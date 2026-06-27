use super::{
    external_client, field_option, provider_error, type_option, CredentialSpec, ExternalProvider,
    ProviderSearchConfig, USER_AGENT,
};
use crate::api::state::AppState;
use crate::api::ApiError;
use crate::contract::{
    ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption,
    ProviderEpisodeGroup, ProviderEpisodeItem, ProviderEpisodes,
};
use crate::secrets::SECRET_TMDB_API_KEY;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct TmdbProvider;

const TMDB_LANG: &str = "en-US";
const IMAGE_BASE: &str = "https://image.tmdb.org/t/p/";

impl ExternalProvider for TmdbProvider {
    const ID: &'static str = "tmdb";
    const LABEL: &'static str = "TMDB";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        !tmdb_media_types(provider_config).is_empty()
    }

    fn credentials() -> &'static [CredentialSpec] {
        &[CredentialSpec {
            key: SECRET_TMDB_API_KEY,
            label: "TMDB API Key",
            secret: true,
            required: true,
        }]
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        tmdb_api_key(state)
            .is_none()
            .then(|| "Set the TMDB API key".to_string())
    }

    fn available(state: &AppState) -> bool {
        tmdb_api_key(state).is_some()
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
        search_tmdb(state, q, page, page_size, provider_config).await
    }

    const SUPPORTS_EPISODES: bool = true;

    async fn fetch_episodes(
        state: &AppState,
        ref_value: &str,
        language: Option<&str>,
    ) -> Result<ProviderEpisodes, ApiError> {
        fetch_tmdb_episodes(state, ref_value, language).await
    }
}

/// The TMDB TV id from a stored ref: a `…/tv/{id}` URL, or a bare numeric id
/// (assumed to be a series, since only TV has episodes). A movie/person link has
/// no episode list and yields `None`.
fn tmdb_tv_id(ref_value: &str) -> Option<String> {
    match tmdb_ref(ref_value) {
        Some(("tv", id)) => Some(id),
        Some(_) => None,
        None => {
            let trimmed = ref_value.trim();
            (!trimmed.is_empty() && trimmed.chars().all(|character| character.is_ascii_digit()))
                .then(|| trimmed.to_string())
        }
    }
}

/// Fetches a TV series' episodes from TMDB, grouped by season (`season 0` →
/// "Specials"). Seasons are fetched concurrently. `language` (ISO 639-1) selects
/// localized episode names where TMDB has them, falling back to the default.
async fn fetch_tmdb_episodes(
    state: &AppState,
    ref_value: &str,
    language: Option<&str>,
) -> Result<ProviderEpisodes, ApiError> {
    let id =
        tmdb_tv_id(ref_value).ok_or_else(|| ApiError::bad_request("Not a TMDB TV series link"))?;
    let api_key = tmdb_api_key(state)
        .ok_or_else(|| ApiError::bad_request("TMDB API key is not configured"))?;
    let client = external_client();
    let language = language
        .map(str::to_string)
        .unwrap_or_else(|| TMDB_LANG.to_string());

    let detail = tmdb_get(client, format!("tv/{id}"), &api_key, &language).await?;
    let seasons: Vec<i64> = detail
        .get("seasons")
        .and_then(Value::as_array)
        .map(|seasons| {
            seasons
                .iter()
                .filter_map(|season| season.get("season_number").and_then(Value::as_i64))
                .collect()
        })
        .unwrap_or_default();

    // Fetch every season concurrently rather than summing their latencies. `join_all`
    // preserves input order, so seasons stay sorted as TMDB returns them.
    let season_values = futures_util::future::join_all(seasons.iter().map(|number| {
        tmdb_get(
            client,
            format!("tv/{id}/season/{number}"),
            &api_key,
            &language,
        )
    }))
    .await;

    let mut groups = Vec::new();
    for (number, season) in seasons.iter().zip(season_values) {
        let items = tmdb_season_items(&season?);
        if !items.is_empty() {
            groups.push(ProviderEpisodeGroup {
                label: tmdb_season_label(*number),
                items,
            });
        }
    }
    Ok(ProviderEpisodes { groups })
}

fn tmdb_season_label(season_number: i64) -> String {
    if season_number == 0 {
        "Specials".to_string()
    } else {
        format!("Season {season_number}")
    }
}

/// Maps a TMDB season detail's `episodes` into items keyed by `episode_number`.
fn tmdb_season_items(season: &Value) -> Vec<ProviderEpisodeItem> {
    season
        .get("episodes")
        .and_then(Value::as_array)
        .map(|episodes| {
            episodes
                .iter()
                .filter_map(|episode| {
                    let key = episode.get("episode_number").and_then(Value::as_i64)?;
                    let title = episode
                        .get("name")
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
        })
        .unwrap_or_default()
}

/// GETs a TMDB v3 path (relative to `/3/`) as JSON, with the api key and language.
async fn tmdb_get(
    client: &reqwest::Client,
    path: String,
    api_key: &str,
    language: &str,
) -> Result<Value, ApiError> {
    client
        .get(format!("https://api.themoviedb.org/3/{path}"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[("api_key", api_key), ("language", language)])
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)
}

fn tmdb_api_key(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_TMDB_API_KEY)
        .filter(|value| !value.is_empty())
}

/// The TMDB media kinds (`movie`, `tv`, `person`) this field searches.
/// Unconstrained means all three.
fn tmdb_media_types(provider_config: &ProviderSearchConfig) -> Vec<&'static str> {
    let Some(types) = provider_config.external_types() else {
        return vec!["movie", "tv", "person"];
    };
    let mut media = Vec::new();
    for kind in ["movie", "tv", "person"] {
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
        field_option("title", "Title / Name"),
        field_option("original_title", "Original title"),
        field_option("year", "Year"),
        field_option("release_date", "Release / air date"),
        field_option("runtime", "Runtime (minutes)"),
        field_option("status", "Status"),
        field_option("season_count", "Season count"),
        field_option("episode_count", "Episode count"),
        field_option("last_air_date", "Last air date"),
        field_option("country", "Country"),
        field_option("original_language", "Original language"),
        field_option("score", "Score"),
        field_option("score_count", "Score count"),
        field_option("imdb_code", "IMDb id"),
        field_option("tvdb_id", "TheTVDB id"),
        field_option("wikidata_id", "Wikidata id"),
        field_option("official_site", "Official site"),
        field_option("birthday", "Birthday"),
        field_option("death_date", "Death date"),
        field_option("known_for", "Known for"),
        field_option("overview", "Overview"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("genres", "Genres"),
        field_option("language", "Languages"),
        field_option("studios", "Studios"),
        field_option("directors", "Directors"),
        field_option("writers", "Writers"),
        field_option("producers", "Producers"),
        field_option("cast", "Cast"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![
        type_option("movie", "Movie"),
        type_option("tv", "TV"),
        type_option("person", "Person"),
    ]
}

async fn search_tmdb(
    state: &AppState,
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let media = tmdb_media_types(provider_config);
    if media.is_empty() {
        return Ok(Vec::new());
    }
    let Some(api_key) = tmdb_api_key(state) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    // A pasted TMDB URL resolves a single record (with full credits/genres).
    if let Some((media_type, id)) = tmdb_ref(q) {
        return resolve_tmdb(client, &api_key, media_type, &id).await;
    }
    // With a single configured type, query that type's endpoint so pagination is
    // accurate (results aren't diluted by other media). With several, fall back
    // to /search/multi and slice — its results carry a `media_type` to filter on.
    let (endpoint, forced_media_type) = match media.as_slice() {
        [single] => (
            format!("https://api.themoviedb.org/3/search/{single}"),
            Some(*single),
        ),
        _ => (
            "https://api.themoviedb.org/3/search/multi".to_string(),
            None,
        ),
    };
    // TMDB pages hold 20 results; translate our (page, page_size) into a TMDB
    // page plus an in-page offset, then slice.
    let tmdb_page = (page - 1) * page_size / 20 + 1;
    let offset = (page - 1) * page_size % 20;
    let value = client
        .get(&endpoint)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[
            ("query", q),
            ("page", &tmdb_page.to_string()),
            ("api_key", &api_key),
            ("language", TMDB_LANG),
            ("include_adult", "true"),
        ])
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    let results = value
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(results
        .iter()
        .filter_map(|item| tmdb_search_result(item, &media, forced_media_type))
        .skip(offset)
        .take(page_size)
        .collect())
}

async fn resolve_tmdb(
    client: &reqwest::Client,
    api_key: &str,
    media_type: &str,
    id: &str,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let value = client
        .get(format!("https://api.themoviedb.org/3/{media_type}/{id}"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[
            ("api_key", api_key),
            ("language", TMDB_LANG),
            ("append_to_response", "external_ids,credits"),
        ])
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    Ok(tmdb_detail(media_type, id, &value).into_iter().collect())
}

/// Detects a `themoviedb.org/{movie|tv|person}/{id}` URL.
fn tmdb_ref(q: &str) -> Option<(&'static str, String)> {
    let trimmed = q.trim();
    let (_, rest) = trimmed.split_once("themoviedb.org/")?;
    let mut parts = rest.split('/');
    let media_type = match parts.next()? {
        "movie" => "movie",
        "tv" => "tv",
        "person" => "person",
        _ => return None,
    };
    // The id is the leading digits of the next segment (TMDB appends a slug).
    let segment = parts.next()?;
    let id: String = segment
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    (!id.is_empty()).then_some((media_type, id))
}

fn tmdb_search_result(
    item: &Value,
    media: &[&str],
    forced_media_type: Option<&str>,
) -> Option<ExternalCandidate> {
    // Type-specific endpoints (/search/movie etc.) omit `media_type`; the caller
    // supplies it. /search/multi includes it, so filter to the configured types.
    let media_type = match forced_media_type {
        Some(media_type) => media_type,
        None => {
            let media_type = item.get("media_type").and_then(Value::as_str)?;
            if !media.contains(&media_type) {
                return None;
            }
            media_type
        }
    };
    let id = item.get("id").and_then(Value::as_i64)?.to_string();
    let url = format!("https://www.themoviedb.org/{media_type}/{id}");
    let (title, original_title, date_field) = match media_type {
        "tv" => ("name", "original_name", "first_air_date"),
        _ => ("title", "original_title", "release_date"),
    };
    let title = item
        .get(title)
        .or_else(|| item.get("name"))
        .or_else(|| item.get("title"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let original_title = item
        .get(original_title)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let cover_url = tmdb_image(item, "w500");
    let overview = item
        .get("overview")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(original_title) = &original_title {
        metadata.insert(
            "original_title".to_string(),
            Value::String(original_title.clone()),
        );
    }
    if let Some(date) = item
        .get(date_field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("release_date".to_string(), Value::String(date.to_string()));
        if let Some(year) = date.split('-').next().filter(|year| year.len() == 4) {
            metadata.insert("year".to_string(), Value::String(year.to_string()));
        }
    }
    if let Some(overview) = &overview {
        metadata.insert("overview".to_string(), Value::String(overview.clone()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: TmdbProvider::ID.to_string(),
        source_id: id,
        url,
        original_title,
        title,
        brief: overview,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

fn tmdb_detail(media_type: &str, id: &str, data: &Value) -> Option<ExternalCandidate> {
    if media_type == "person" {
        return tmdb_person(id, data);
    }
    let is_tv = media_type == "tv";
    let title = data
        .get(if is_tv { "name" } else { "title" })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let original_title = data
        .get(if is_tv {
            "original_name"
        } else {
            "original_title"
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let url = format!("https://www.themoviedb.org/{media_type}/{id}");
    let cover_url = tmdb_image(data, "original");
    let overview = data
        .get("overview")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let date_field = if is_tv {
        "first_air_date"
    } else {
        "release_date"
    };

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(original_title) = &original_title {
        metadata.insert(
            "original_title".to_string(),
            Value::String(original_title.clone()),
        );
    }
    if let Some(date) = data
        .get(date_field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("release_date".to_string(), Value::String(date.to_string()));
        if let Some(year) = date.split('-').next().filter(|year| year.len() == 4) {
            metadata.insert("year".to_string(), Value::String(year.to_string()));
        }
    }
    if let Some(runtime) = data
        .get("runtime")
        // Movies carry `runtime`; TV carries `episode_run_time: [n, …]`.
        .or_else(|| {
            data.get("episode_run_time")
                .and_then(Value::as_array)
                .and_then(|runtimes| runtimes.first())
        })
        .and_then(Value::as_i64)
        .filter(|runtime| *runtime > 0)
    {
        metadata.insert("runtime".to_string(), Value::Number(runtime.into()));
    }
    insert_str(&mut metadata, "status", data.get("status"));
    insert_str(&mut metadata, "last_air_date", data.get("last_air_date"));
    if let Some(country) = data
        .get("production_countries")
        .and_then(Value::as_array)
        .and_then(|countries| countries.first())
        .and_then(|country| country.get("name"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("country".to_string(), Value::String(country.to_string()));
    }
    if let Some(score) = data
        .get("vote_average")
        .and_then(Value::as_f64)
        .filter(|score| *score > 0.0)
    {
        metadata.insert("score".to_string(), serde_json::json!(score));
        if let Some(count) = data
            .get("vote_count")
            .and_then(Value::as_i64)
            .filter(|count| *count > 0)
        {
            metadata.insert("score_count".to_string(), Value::Number(count.into()));
        }
    }
    // Production companies → studios (cap at 3).
    if let Some(studios) = data.get("production_companies").and_then(Value::as_array) {
        let studios: Vec<Value> = studios
            .iter()
            .filter_map(|company| company.get("name").and_then(Value::as_str))
            .filter(|name| !name.is_empty())
            .take(3)
            .map(|name| Value::String(name.to_string()))
            .collect();
        if !studios.is_empty() {
            metadata.insert("studios".to_string(), Value::Array(studios));
        }
    }
    if let Some(imdb) = data
        .get("imdb_id")
        .or_else(|| {
            data.get("external_ids")
                .and_then(|external| external.get("imdb_id"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("imdb_code".to_string(), Value::String(imdb.to_string()));
    }
    if let Some(language) = data
        .get("original_language")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "original_language".to_string(),
            Value::String(language.to_string()),
        );
    }
    if is_tv {
        if let Some(seasons) = data
            .get("number_of_seasons")
            .and_then(Value::as_i64)
            .filter(|seasons| *seasons > 0)
        {
            metadata.insert("season_count".to_string(), Value::Number(seasons.into()));
        }
        if let Some(episodes) = data
            .get("number_of_episodes")
            .and_then(Value::as_i64)
            .filter(|episodes| *episodes > 0)
        {
            metadata.insert("episode_count".to_string(), Value::Number(episodes.into()));
        }
    }
    for (field, key) in [("wikidata_id", "wikidata_id"), ("tvdb_id", "tvdb_id")] {
        if let Some(value) = data
            .get("external_ids")
            .and_then(|external| external.get(key))
            .and_then(non_empty_id)
        {
            metadata.insert(field.to_string(), Value::String(value));
        }
    }
    if let Some(homepage) = data
        .get("homepage")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "official_site".to_string(),
            Value::String(homepage.to_string()),
        );
    }
    if let Some(genres) = named_list(data.get("genres")) {
        metadata.insert("genres".to_string(), genres);
    }
    if let Some(languages) = named_list(data.get("spoken_languages")) {
        metadata.insert("language".to_string(), languages);
    }
    let credits = data.get("credits");
    let crew = credits.and_then(|credits| credits.get("crew"));
    let directors = crew_names(crew, |job| job == "Director");
    let writers = crew_names(crew, |job| {
        matches!(job, "Writer" | "Screenplay" | "Teleplay" | "Story")
    });
    let producers = crew_names(crew, |job| matches!(job, "Producer" | "Executive Producer"));
    // TV shows rarely credit a series-level director; fall back to creators.
    let directors = if directors.is_empty() && is_tv {
        named_list_strings(data.get("created_by"))
    } else {
        directors
    };
    let cast = credits
        .and_then(|credits| credits.get("cast"))
        .and_then(Value::as_array)
        .map(|cast| {
            cast.iter()
                .take(10)
                .filter_map(|member| member.get("name").and_then(Value::as_str))
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    insert_string_list(&mut metadata, "directors", directors);
    insert_string_list(&mut metadata, "writers", writers);
    insert_string_list(&mut metadata, "producers", producers);
    insert_string_list(&mut metadata, "cast", cast);
    if let Some(overview) = &overview {
        metadata.insert("overview".to_string(), Value::String(overview.clone()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    // Tag the original title with its language so the candidate carries a
    // localized title entry (cheap: no extra request needed).
    let titles = match (
        data.get("original_language").and_then(Value::as_str),
        &original_title,
    ) {
        (Some(language), Some(original_title)) if !language.is_empty() => {
            BTreeMap::from([(language.to_string(), original_title.clone())])
        }
        _ => BTreeMap::new(),
    };
    Some(ExternalCandidate {
        provider: TmdbProvider::ID.to_string(),
        source_id: id.to_string(),
        url,
        original_title,
        title,
        brief: overview,
        cover_url,
        titles,
        metadata,
    })
}

fn tmdb_person(id: &str, data: &Value) -> Option<ExternalCandidate> {
    let title = data
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = format!("https://www.themoviedb.org/person/{id}");
    let cover_url = data
        .get("profile_path")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|path| format!("{IMAGE_BASE}original{path}"));
    let biography = data
        .get("biography")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(birthday) = data
        .get("birthday")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("birthday".to_string(), Value::String(birthday.to_string()));
    }
    if let Some(death_date) = data
        .get("deathday")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "death_date".to_string(),
            Value::String(death_date.to_string()),
        );
    }
    if let Some(homepage) = data
        .get("homepage")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "official_site".to_string(),
            Value::String(homepage.to_string()),
        );
    }
    if let Some(known_for) = data
        .get("known_for_department")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "known_for".to_string(),
            Value::String(known_for.to_string()),
        );
    }
    if let Some(biography) = &biography {
        metadata.insert("overview".to_string(), Value::String(biography.clone()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: TmdbProvider::ID.to_string(),
        source_id: id.to_string(),
        url,
        original_title: Some(title.clone()),
        title,
        brief: biography,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

fn tmdb_image(item: &Value, size: &str) -> Option<String> {
    item.get("poster_path")
        .or_else(|| item.get("profile_path"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|path| format!("{IMAGE_BASE}{size}{path}"))
}

fn crew_names(crew: Option<&Value>, job_matches: impl Fn(&str) -> bool) -> Vec<String> {
    let Some(crew) = crew.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for member in crew {
        let job = member
            .get("job")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if job_matches(job) {
            if let Some(name) = member
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
            {
                let name = name.to_string();
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    names
}

fn named_list(value: Option<&Value>) -> Option<Value> {
    let names = named_list_strings(value);
    (!names.is_empty()).then(|| Value::Array(names.into_iter().map(Value::String).collect()))
}

fn named_list_strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|entry| entry.get("name").and_then(Value::as_str))
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn insert_str(metadata: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(text) = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(key.to_string(), Value::String(text.to_string()));
    }
}

/// `external_ids` values are strings (`imdb_id`, `wikidata_id`) or integers
/// (`tvdb_id`); normalize either to a non-empty string.
fn non_empty_id(value: &Value) -> Option<String> {
    value
        .as_i64()
        .filter(|id| *id > 0)
        .map(|id| id.to_string())
        .or_else(|| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

fn insert_string_list(metadata: &mut Map<String, Value>, key: &str, values: Vec<String>) {
    if !values.is_empty() {
        metadata.insert(
            key.to_string(),
            Value::Array(values.into_iter().map(Value::String).collect()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{tmdb_detail, tmdb_ref, tmdb_search_result, tmdb_season_items, tmdb_tv_id};
    use serde_json::json;

    #[test]
    fn search_result_respects_media_filter() {
        let movie = json!({
            "media_type": "movie",
            "id": 27205,
            "title": "Inception",
            "original_title": "Inception",
            "release_date": "2010-07-15",
            "overview": "A thief…",
            "poster_path": "/poster.jpg"
        });
        // /search/multi path: media_type filters the mixed results.
        assert!(tmdb_search_result(&movie, &["tv"], None).is_none());
        let candidate = tmdb_search_result(&movie, &["movie"], None).unwrap();
        assert_eq!(candidate.source_id, "27205");
        assert_eq!(candidate.metadata.get("year"), Some(&json!("2010")));
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w500/poster.jpg")
        );

        // Type-specific endpoint path: results omit media_type; it's forced.
        let no_type = json!({
            "id": 27205,
            "title": "Inception",
            "release_date": "2010-07-15"
        });
        let candidate = tmdb_search_result(&no_type, &["movie"], Some("movie")).unwrap();
        assert_eq!(candidate.source_id, "27205");
        assert_eq!(candidate.url, "https://www.themoviedb.org/movie/27205");
    }

    #[test]
    fn detail_extracts_credits_and_genres() {
        let candidate = tmdb_detail(
            "movie",
            "27205",
            &json!({
                "title": "Inception",
                "original_title": "Inception",
                "original_language": "en",
                "release_date": "2010-07-15",
                "runtime": 148,
                "status": "Released",
                "vote_average": 8.4,
                "vote_count": 35000,
                "production_companies": [{ "name": "Legendary Pictures" }, { "name": "Syncopy" }],
                "production_countries": [{ "name": "United States of America" }],
                "imdb_id": "tt1375666",
                "external_ids": { "wikidata_id": "Q25188", "tvdb_id": 12345 },
                "genres": [{ "name": "Action" }, { "name": "Science Fiction" }],
                "spoken_languages": [{ "name": "English" }],
                "credits": {
                    "crew": [
                        { "job": "Director", "name": "Christopher Nolan" },
                        { "job": "Screenplay", "name": "Christopher Nolan" },
                        { "job": "Producer", "name": "Emma Thomas" }
                    ],
                    "cast": [{ "name": "Leonardo DiCaprio" }, { "name": "Joseph Gordon-Levitt" }]
                }
            }),
        )
        .unwrap();

        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("runtime"), Some(&json!(148)));
        assert_eq!(metadata.get("status"), Some(&json!("Released")));
        assert_eq!(metadata.get("score"), Some(&json!(8.4)));
        assert_eq!(metadata.get("score_count"), Some(&json!(35000)));
        assert_eq!(
            metadata.get("studios"),
            Some(&json!(["Legendary Pictures", "Syncopy"]))
        );
        assert_eq!(
            metadata.get("country"),
            Some(&json!("United States of America"))
        );
        assert_eq!(metadata.get("imdb_code"), Some(&json!("tt1375666")));
        assert_eq!(metadata.get("wikidata_id"), Some(&json!("Q25188")));
        assert_eq!(metadata.get("tvdb_id"), Some(&json!("12345")));
        assert_eq!(metadata.get("original_language"), Some(&json!("en")));
        assert_eq!(
            metadata.get("genres"),
            Some(&json!(["Action", "Science Fiction"]))
        );
        assert_eq!(
            metadata.get("directors"),
            Some(&json!(["Christopher Nolan"]))
        );
        assert_eq!(metadata.get("writers"), Some(&json!(["Christopher Nolan"])));
        assert_eq!(metadata.get("producers"), Some(&json!(["Emma Thomas"])));
        assert_eq!(
            metadata.get("cast"),
            Some(&json!(["Leonardo DiCaprio", "Joseph Gordon-Levitt"]))
        );
        // The original title is tagged with the original language in `titles`.
        assert_eq!(candidate.titles.get("en"), Some(&"Inception".to_string()));
    }

    #[test]
    fn tv_detail_surfaces_season_count() {
        let candidate = tmdb_detail(
            "tv",
            "1399",
            &json!({
                "name": "Game of Thrones",
                "original_name": "Game of Thrones",
                "first_air_date": "2011-04-17",
                "last_air_date": "2019-05-19",
                "number_of_seasons": 8,
                "number_of_episodes": 73,
                "episode_run_time": [60],
                "created_by": [{ "name": "David Benioff" }, { "name": "D. B. Weiss" }],
                "external_ids": { "imdb_id": "tt0944947", "tvdb_id": 121361 }
            }),
        )
        .unwrap();

        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("season_count"), Some(&json!(8)));
        assert_eq!(metadata.get("episode_count"), Some(&json!(73)));
        assert_eq!(metadata.get("last_air_date"), Some(&json!("2019-05-19")));
        assert_eq!(metadata.get("runtime"), Some(&json!(60)));
        assert_eq!(metadata.get("tvdb_id"), Some(&json!("121361")));
        assert_eq!(metadata.get("imdb_code"), Some(&json!("tt0944947")));
        // No crew Director on a series → fall back to created_by.
        assert_eq!(
            metadata.get("directors"),
            Some(&json!(["David Benioff", "D. B. Weiss"]))
        );
    }

    #[test]
    fn ref_parses_media_urls() {
        assert_eq!(
            tmdb_ref("https://www.themoviedb.org/movie/27205-inception"),
            Some(("movie", "27205".to_string()))
        );
        assert_eq!(
            tmdb_ref("https://www.themoviedb.org/tv/1399/seasons"),
            Some(("tv", "1399".to_string()))
        );
        assert_eq!(tmdb_ref("inception"), None);
    }

    #[test]
    fn tv_id_accepts_tv_url_and_bare_id_but_not_movies() {
        assert_eq!(
            tmdb_tv_id("https://www.themoviedb.org/tv/1399/seasons"),
            Some("1399".to_string())
        );
        assert_eq!(tmdb_tv_id("1399"), Some("1399".to_string()));
        assert_eq!(tmdb_tv_id("https://www.themoviedb.org/movie/27205"), None);
    }

    #[test]
    fn season_items_key_by_episode_number() {
        let season = json!({
            "episodes": [
                { "episode_number": 1, "name": "Winter Is Coming" },
                { "episode_number": 2, "name": "The Kingsroad" },
            ]
        });
        let items = tmdb_season_items(&season);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].key, "1");
        assert_eq!(items[0].title, "Winter Is Coming");
        assert_eq!(items[1].key, "2");
    }
}
