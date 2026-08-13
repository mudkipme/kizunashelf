use super::{
    cached_or_fetch_token, external_client, field_option, insert_str, named_list,
    non_empty_string_or_integer, provider_error, send_limited, send_with_token_retry, string_list,
    type_option, url_type_allowed, CredentialSpec, ExternalProvider, ProviderResponseExt,
    ProviderSearchConfig,
};
use crate::api::state::{unix_seconds_now, AppState, CachedAccessToken};
use crate::api::ApiError;
use crate::contract::{
    ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption,
    ProviderEpisodeGroup, ProviderEpisodeItem, ProviderEpisodes,
};
use crate::languages::{thetvdb_iso_language, thetvdb_language};
use crate::secrets::{SECRET_TVDB_API_KEY, SECRET_TVDB_PIN};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

pub(super) struct ThetvdbProvider;

impl ExternalProvider for ThetvdbProvider {
    const ID: &'static str = "thetvdb";
    const LABEL: &'static str = "TheTVDB";

    fn recognizes_url(q: &str) -> bool {
        q.contains("thetvdb.com/")
    }

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        thetvdb_type_filters(provider_config).is_some()
    }

    fn credentials() -> &'static [CredentialSpec] {
        &[
            CredentialSpec {
                key: SECRET_TVDB_API_KEY,
                label: "TheTVDB API Key",
                secret: true,
                required: true,
            },
            CredentialSpec {
                key: SECRET_TVDB_PIN,
                label: "TheTVDB PIN (optional)",
                secret: false,
                required: false,
            },
        ]
    }

    fn field_options() -> Vec<ExternalProviderFieldOption> {
        field_options()
    }

    fn type_options() -> Vec<ExternalProviderTypeOption> {
        type_options()
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

    const SUPPORTS_EPISODES: bool = true;

    async fn fetch_episodes(
        state: &AppState,
        ref_value: &str,
        language: Option<&str>,
    ) -> Result<ProviderEpisodes, ApiError> {
        fetch_thetvdb_episodes(state, ref_value, language).await
    }
}

/// The TheTVDB login body (`{ apikey, pin? }`) from the configured credentials.
fn thetvdb_login(state: &AppState) -> Option<Map<String, Value>> {
    let mut login = Map::new();
    login.insert("apikey".to_string(), Value::String(tvdb_api_key(state)?));
    if let Some(pin) = state
        .secret_store()
        .get(SECRET_TVDB_PIN)
        .filter(|value| !value.is_empty())
    {
        login.insert("pin".to_string(), Value::String(pin));
    }
    Some(login)
}

/// Which kind of record a TheTVDB link names. Only the two kinds a library entity
/// can be; people/companies are parsed as `None` and fall back to a text search.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RecordKind {
    Series,
    Movie,
}

impl RecordKind {
    /// The path segment on both the site (`thetvdb.com/movies/{slug}`) and the
    /// API (`/v4/movies/{id}`) — they agree for these two kinds.
    fn path(self) -> &'static str {
        match self {
            RecordKind::Series => "series",
            RecordKind::Movie => "movies",
        }
    }

    /// The singular form a search result's `type` and a field's `externalTypes`
    /// use (`movie`, not `movies`), which is also the `/dereferrer/{kind}/{id}`
    /// segment.
    fn record_type(self) -> &'static str {
        match self {
            RecordKind::Series => "series",
            RecordKind::Movie => "movie",
        }
    }

    fn from_record_type(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "series" => Some(RecordKind::Series),
            "movie" | "movies" => Some(RecordKind::Movie),
            _ => None,
        }
    }
}

/// How a TheTVDB link addresses its record.
enum RecordLocator {
    /// A numeric id (the `…/dereferrer/{kind}/{id}` form, or a bare id).
    Id(String),
    /// A URL slug (the human `thetvdb.com/{kind}/{slug}` form) — resolved to an id.
    Slug(String),
}

/// Parses a TheTVDB link (or a stored ref) into the exact record it names.
/// `None` when it names something else (a person, a company, a list) or carries
/// no record segment — those degrade to a text search.
fn thetvdb_record_ref(value: &str) -> Option<(RecordKind, RecordLocator)> {
    let trimmed = value.trim().trim_end_matches('/');
    // Stored refs are usually full URLs, but a hand-written `series/{slug}` is
    // just as unambiguous, so the host is optional.
    let path = trimmed
        .split_once("thetvdb.com/")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let mut parts = path
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty());
    let mut segment = parts.next()?;
    if segment.eq_ignore_ascii_case("dereferrer") {
        segment = parts.next()?;
    }
    let kind = RecordKind::from_record_type(segment)?;
    let locator = parts.next()?;
    if locator.is_empty() {
        return None;
    }
    Some((
        kind,
        if locator.chars().all(|c| c.is_ascii_digit()) {
            RecordLocator::Id(locator.to_string())
        } else {
            RecordLocator::Slug(locator.to_string())
        },
    ))
}

/// Parses a stored TheTVDB ref into a series id or slug. `None` for movie links
/// or anything without a series segment (movies have no episodes).
fn thetvdb_series_ref(ref_value: &str) -> Option<RecordLocator> {
    let trimmed = ref_value.trim().trim_end_matches('/');
    if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Some(RecordLocator::Id(trimmed.to_string()));
    }
    match thetvdb_record_ref(trimmed)? {
        (RecordKind::Series, locator) => Some(locator),
        (RecordKind::Movie, _) => None,
    }
}

/// A bearer GET against the TheTVDB v4 API returning the parsed JSON.
async fn thetvdb_get(client: &reqwest::Client, token: &str, url: &str) -> Result<Value, ApiError> {
    send_limited(client.get(url).bearer_auth(token))
        .await
        .map_err(provider_error)?
        .error_for_status_body()
        .await?
        .json::<Value>()
        .await
        .map_err(provider_error)
}

/// One parsed episode row: `(season, number-key, title)`.
type EpisodeRow = (i64, String, String, Option<String>);

/// Paginates an `episodes/{season-type}[/{lang}]` endpoint into ordered rows.
async fn thetvdb_episode_pages(
    client: &reqwest::Client,
    token: &str,
    start_url: &str,
) -> Result<Vec<EpisodeRow>, ApiError> {
    let mut rows: Vec<EpisodeRow> = Vec::new();
    let mut url = start_url.to_string();
    let mut pages = 0;
    loop {
        let value = thetvdb_get(client, token, &url).await?;
        if let Some(episodes) = value.pointer("/data/episodes").and_then(Value::as_array) {
            for episode in episodes {
                let season = episode
                    .get("seasonNumber")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let key = episode
                    .get("number")
                    .and_then(Value::as_i64)
                    .map(|number| number.to_string())
                    .unwrap_or_default();
                let title = episode
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|name| name.trim().to_string())
                    .unwrap_or_default();
                let date = episode
                    .get("aired")
                    .and_then(Value::as_str)
                    .and_then(crate::dates::iso_date);
                rows.push((season, key, title, date));
            }
        }
        pages += 1;
        match value.pointer("/links/next").and_then(Value::as_str) {
            Some(next) if !next.is_empty() && pages < 50 => url = next.to_string(),
            _ => break,
        }
    }
    Ok(rows)
}

/// Fetches a series' episodes grouped by season (season 0 → "Specials"). A slug ref
/// is resolved to an id first. When the viewer's `language` maps to a TheTVDB code,
/// translated titles are overlaid onto the default-language list — so episodes with
/// no translation keep their default title instead of going blank.
async fn fetch_thetvdb_episodes(
    state: &AppState,
    ref_value: &str,
    language: Option<&str>,
) -> Result<ProviderEpisodes, ApiError> {
    let series_ref = thetvdb_series_ref(ref_value)
        .ok_or_else(|| ApiError::bad_request("Not a TheTVDB series link"))?;
    let login = thetvdb_login(state)
        .ok_or_else(|| ApiError::bad_request("TheTVDB API key is not configured"))?;
    let client = external_client();
    let token = thetvdb_access_token(state, client, &login, false).await?;

    let series_id = match series_ref {
        RecordLocator::Id(id) => id,
        RecordLocator::Slug(slug) => {
            let value = thetvdb_get(
                client,
                &token,
                &format!("https://api4.thetvdb.com/v4/series/slug/{slug}"),
            )
            .await?;
            value
                .pointer("/data/id")
                .and_then(Value::as_i64)
                .map(|id| id.to_string())
                .ok_or_else(|| ApiError::bad_request("TheTVDB series not found"))?
        }
    };

    let base_url =
        format!("https://api4.thetvdb.com/v4/series/{series_id}/episodes/official?page=0");
    let mut rows = thetvdb_episode_pages(client, &token, &base_url).await?;

    // Overlay translated titles when a supported language is requested. Best-effort:
    // a failed/empty translation leaves the default-language titles in place.
    if let Some(language) = language.and_then(thetvdb_language) {
        let translated_url = format!(
            "https://api4.thetvdb.com/v4/series/{series_id}/episodes/official/{language}?page=0"
        );
        if let Ok(translated) = thetvdb_episode_pages(client, &token, &translated_url).await {
            let by_key: HashMap<(i64, &str), &str> = translated
                .iter()
                .filter(|(_, _, title, _)| !title.is_empty())
                .map(|(season, key, title, _)| ((*season, key.as_str()), title.as_str()))
                .collect();
            for (season, key, title, _date) in rows.iter_mut() {
                if let Some(translated_title) = by_key.get(&(*season, key.as_str())) {
                    *title = (*translated_title).to_string();
                }
            }
        }
    }

    let mut by_season: BTreeMap<i64, Vec<ProviderEpisodeItem>> = BTreeMap::new();
    for (season, key, title, date) in rows {
        by_season
            .entry(season)
            .or_default()
            .push(ProviderEpisodeItem { key, title, date });
    }
    let groups = by_season
        .into_iter()
        .map(|(season, items)| ProviderEpisodeGroup {
            label: if season == 0 {
                "Specials".to_string()
            } else {
                format!("Season {season}")
            },
            items,
        })
        .collect();
    Ok(ProviderEpisodes { groups })
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
    // The viewer's language picks which entry of a result's `translations` map is
    // the display title (see `thetvdb_candidate`).
    let language = provider_config.language.as_deref();

    // A TheTVDB link is exact-match intent, so fetch the record it names instead
    // of degrading it to a text search. The search endpoint takes free text only:
    // it would match the id or slug as a *word* and happily rank another work
    // first (`…/dereferrer/series/5239` scoring the movie `pu-239` above the one
    // asked for), which then became the record that got added.
    if let Some((kind, locator)) = thetvdb_record_ref(q).filter(|_| q.contains("thetvdb.com/")) {
        if !url_type_allowed(provider_config, kind.record_type()) {
            return Ok(Vec::new());
        }
        let record =
            resolve_thetvdb_record(state, client, &login, &token, kind, &locator, language).await?;
        return Ok(record.into_iter().collect());
    }
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
        for candidate in data
            .iter()
            .filter_map(|item| thetvdb_candidate(item, language))
        {
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
    let response = send_with_token_retry(
        state,
        "thetvdb",
        token,
        |token| thetvdb_search_request(client, token, q, type_filter),
        || thetvdb_access_token(state, client, login, true),
    )
    .await?;
    Ok(response
        .error_for_status_body()
        .await?
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

/// A bearer GET that mints a fresh token and retries once on a `401`, for the
/// record lookups that run outside the search endpoint's own retry.
async fn thetvdb_get_with_retry(
    state: &AppState,
    client: &reqwest::Client,
    login: &Map<String, Value>,
    token: &str,
    url: &str,
) -> Result<Value, ApiError> {
    send_with_token_retry(
        state,
        "thetvdb",
        token,
        |token| client.get(url).bearer_auth(token),
        || thetvdb_access_token(state, client, login, true),
    )
    .await?
    .error_for_status_body()
    .await?
    .json::<Value>()
    .await
    .map_err(provider_error)
}

/// Fetches the one record a TheTVDB link names. A slug is resolved to an id
/// first (the extended endpoints are id-only), then the extended record — with
/// translations, and without the character/artwork/trailer bulk — is reshaped
/// into the search-result form [`thetvdb_candidate`] maps, so a link and a
/// search hit for the same work produce the same candidate.
async fn resolve_thetvdb_record(
    state: &AppState,
    client: &reqwest::Client,
    login: &Map<String, Value>,
    token: &str,
    kind: RecordKind,
    locator: &RecordLocator,
    language: Option<&str>,
) -> Result<Option<ExternalCandidate>, ApiError> {
    let path = kind.path();
    let id = match locator {
        RecordLocator::Id(id) => id.clone(),
        RecordLocator::Slug(slug) => {
            let value = thetvdb_get_with_retry(
                state,
                client,
                login,
                token,
                &format!("https://api4.thetvdb.com/v4/{path}/slug/{slug}"),
            )
            .await?;
            value
                .pointer("/data/id")
                .and_then(non_empty_string_or_integer)
                .ok_or_else(|| ApiError::bad_request("TheTVDB record not found"))?
        }
    };
    let value = thetvdb_get_with_retry(
        state,
        client,
        login,
        token,
        &format!("https://api4.thetvdb.com/v4/{path}/{id}/extended?meta=translations&short=true"),
    )
    .await?;
    let Some(record) = value.get("data").filter(|data| data.is_object()) else {
        return Ok(None);
    };
    Ok(thetvdb_candidate(
        &thetvdb_record_item(kind, record),
        language,
    ))
}

/// Reshapes an extended series/movie record into the search-result shape.
/// The two endpoints name the same data differently (`image` vs `image_url`,
/// object lists vs string lists, a translation *list* vs a language map), so
/// this is where they converge — one mapping in [`thetvdb_candidate`] then
/// serves both.
fn thetvdb_record_item(kind: RecordKind, record: &Value) -> Value {
    let mut item = Map::new();
    if let Some(id) = record.get("id").and_then(non_empty_string_or_integer) {
        item.insert("tvdb_id".to_string(), Value::String(id));
    }
    item.insert(
        "type".to_string(),
        Value::String(kind.record_type().to_string()),
    );
    insert_str(&mut item, "name", record.get("name"));
    insert_str(&mut item, "slug", record.get("slug"));
    insert_str(&mut item, "image_url", record.get("image"));
    insert_str(
        &mut item,
        "primary_language",
        record.get("originalLanguage"),
    );
    insert_str(&mut item, "country", record.get("originalCountry"));
    insert_str(&mut item, "year", record.get("year"));
    // Series air on a date; a movie's is its first release.
    let first_air_time = match kind {
        RecordKind::Series => record
            .get("firstAired")
            .and_then(non_empty_string_or_integer),
        RecordKind::Movie => record
            .pointer("/first_release/date")
            .and_then(non_empty_string_or_integer)
            .or_else(|| {
                record
                    .pointer("/releases/0/date")
                    .and_then(non_empty_string_or_integer)
            }),
    };
    if let Some(first_air_time) = first_air_time {
        item.insert("first_air_time".to_string(), Value::String(first_air_time));
    }
    // `status`/`network` are `{ name }` objects here; `thetvdb_candidate` reads
    // either form.
    if let Some(status) = record.get("status") {
        item.insert("status".to_string(), status.clone());
    }
    if let Some(network) = record
        .get("originalNetwork")
        .or_else(|| record.get("latestNetwork"))
        .filter(|value| !value.is_null())
    {
        item.insert("network".to_string(), network.clone());
    }
    for (key, source) in [
        ("genres", "genres"),
        ("studios", "studios"),
        ("aliases", "aliases"),
    ] {
        if let Some(values) = named_list(record.get(source)) {
            item.insert(key.to_string(), values);
        }
    }
    if let Some(remote_ids) = record.get("remoteIds") {
        item.insert("remote_ids".to_string(), remote_ids.clone());
    }
    // `translations` is a list of per-language records here and a language map in
    // search results; fold it into the map form. The record's own (primary)
    // language doubles as the default overview, which movies carry nowhere else.
    for (key, source, text_key) in [
        ("translations", "/translations/nameTranslations", "name"),
        (
            "overviews",
            "/translations/overviewTranslations",
            "overview",
        ),
    ] {
        let Some(translations) = record.pointer(source).and_then(Value::as_array) else {
            continue;
        };
        let mut map = Map::new();
        let mut primary = None;
        for translation in translations {
            let (Some(language), Some(text)) = (
                translation.get("language").and_then(Value::as_str),
                translation
                    .get(text_key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty()),
            ) else {
                continue;
            };
            if translation.get("isPrimary").and_then(Value::as_bool) == Some(true) {
                primary = Some(text.to_string());
            }
            map.insert(language.to_string(), Value::String(text.to_string()));
        }
        if key == "overviews" {
            let default = primary.or_else(|| {
                record
                    .get("originalLanguage")
                    .and_then(Value::as_str)
                    .and_then(|language| map.get(language))
                    .or_else(|| map.get("eng"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
            insert_str(&mut item, "overview", default.map(Value::String).as_ref());
        }
        if !map.is_empty() {
            item.insert(key.to_string(), Value::Object(map));
        }
    }
    // A series carries a default-language overview directly; keep it when the
    // translation list didn't supply one.
    if !item.contains_key("overview") {
        insert_str(&mut item, "overview", record.get("overview"));
    }
    Value::Object(item)
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
        field_option("country", "Region"),
        field_option("network", "Network"),
        field_option("director", "Director"),
        field_option("slug", "Slug"),
        field_option("imdb_code", "IMDb id"),
        field_option("tmdb_id", "TheMovieDB id"),
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
    cached_or_fetch_token(state, "thetvdb", "TheTVDB", force_refresh, || async {
        let value = send_limited(client.post("https://api4.thetvdb.com/v4/login").json(login))
            .await
            .map_err(provider_error)?
            .error_for_status_body()
            .await?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        let access_token = value
            .pointer("/data/token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                ApiError::bad_request("TheTVDB login response did not include a token")
            })?;
        Ok(CachedAccessToken {
            access_token,
            refresh_token: None,
            expires_at: Instant::now() + Duration::from_secs(23 * 60 * 60),
            expires_at_unix_seconds: unix_seconds_now() + 23 * 60 * 60,
        })
    })
    .await
}

fn thetvdb_candidate(item: &Value, language: Option<&str>) -> Option<ExternalCandidate> {
    let source_id = item
        .get("tvdb_id")
        .or_else(|| item.get("id"))
        .and_then(|value| {
            value
                .as_i64()
                .map(|id| id.to_string())
                .or_else(|| value.as_str().map(str::to_string))
        })?;
    // The default name is the original title; a search result also carries the
    // full per-language `translations`/`overviews` maps (3-letter codes), so we
    // localize the display title and blurb to the viewer where a translation
    // exists — mirroring TMDB, but with a client-side pick since TheTVDB returns
    // every language at once rather than translating server-side.
    let name = item
        .get("name")
        .or_else(|| item.get("title"))
        .and_then(Value::as_str)?
        .to_string();
    let translations = item.get("translations").and_then(Value::as_object);
    let overviews = item.get("overviews").and_then(Value::as_object);
    let viewer_tvdb = language.and_then(thetvdb_language);
    let localized = |map: Option<&Map<String, Value>>| {
        viewer_tvdb
            .and_then(|code| map?.get(code))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let title = localized(translations).unwrap_or_else(|| name.clone());
    // The candidate URL is what gets stored as the external ref *and* what
    // re-resolves the record later, so it has to name this exact record: the
    // human `{kind}/{slug}` form when the result carries both (it always does),
    // else the id dereferrer for the record's own kind. Assuming `series` here —
    // as this did — points a movie's ref at an unrelated series.
    let kind = item
        .get("type")
        .or_else(|| item.get("primary_type"))
        .and_then(Value::as_str)
        .and_then(RecordKind::from_record_type);
    let slug = item
        .get("slug")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|slug| !slug.is_empty());
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
        .or_else(|| {
            let kind = kind?;
            Some(format!("https://thetvdb.com/{}/{}", kind.path(), slug?))
        })
        .unwrap_or_else(|| {
            format!(
                "https://thetvdb.com/dereferrer/{}/{source_id}",
                kind.unwrap_or(RecordKind::Series).record_type()
            )
        });
    let cover_url = item
        .get("image_url")
        .or_else(|| item.get("thumbnail"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let release_date = item
        .get("first_air_time")
        .and_then(non_empty_string_or_integer)
        .or_else(|| item.get("year").and_then(non_empty_string_or_integer));
    // Localize the blurb to the viewer, falling back to the default/original
    // overview. Both the display field (`brief`) and the value persisted by the
    // schema mapper (`metadata["overview"]`) read from this, so a Chinese viewer
    // sees the Chinese overview in search *and* in the created entity.
    let overview = localized(overviews).or_else(|| {
        item.get("overview")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    });
    let mut metadata = Map::new();
    metadata.insert("name".to_string(), Value::String(name.clone()));
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
    if let Some(overview) = &overview {
        metadata.insert("overview".to_string(), Value::String(overview.clone()));
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
    // Search hits carry a `remote_ids` array cross-linking to IMDb/TheMovieDB;
    // surface those ids so a TVDB pick can seed other providers.
    if let Some(remote_ids) = item.get("remote_ids").and_then(Value::as_array) {
        for (field, source_name) in [("imdb_code", "imdb"), ("tmdb_id", "themoviedb")] {
            if let Some(remote_id) = remote_ids
                .iter()
                .find(|remote| {
                    remote
                        .get("sourceName")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name.to_ascii_lowercase().contains(source_name))
                })
                .and_then(|remote| remote.get("id"))
                .and_then(non_empty_string_or_integer)
            {
                metadata.insert(field.to_string(), Value::String(remote_id));
            }
        }
    }
    // Tag every translation we can map to a known ISO language so quick-add can
    // populate per-language title fields and same-language matching works — plus
    // the default name under the series' primary language when translations don't
    // already carry it.
    let mut titles = BTreeMap::new();
    if let Some(translations) = translations {
        for (code, value) in translations {
            if let (Some(iso), Some(value)) = (
                thetvdb_iso_language(code),
                value
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
            ) {
                titles.insert(iso.to_string(), value.to_string());
            }
        }
    }
    if !name.is_empty() {
        if let Some(iso) = item
            .get("primary_language")
            .and_then(Value::as_str)
            .and_then(thetvdb_iso_language)
        {
            titles
                .entry(iso.to_string())
                .or_insert_with(|| name.clone());
        }
    }
    Some(ExternalCandidate {
        provider: "thetvdb".to_string(),
        source_id,
        url,
        original_title: Some(name.clone()),
        title,
        brief: overview,
        cover_url,
        titles,
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

#[cfg(test)]
mod tests {
    use super::{
        thetvdb_candidate, thetvdb_record_item, thetvdb_record_ref, thetvdb_series_ref, RecordKind,
        RecordLocator,
    };
    use serde_json::{json, Value};

    #[test]
    fn series_ref_parses_id_dereferrer_and_slug() {
        assert!(matches!(
            thetvdb_series_ref("https://thetvdb.com/dereferrer/series/289882"),
            Some(RecordLocator::Id(id)) if id == "289882"
        ));
        assert!(matches!(
            thetvdb_series_ref("289882"),
            Some(RecordLocator::Id(id)) if id == "289882"
        ));
        // The human URL is a slug, resolved to an id at fetch time.
        assert!(matches!(
            thetvdb_series_ref("https://thetvdb.com/series/answer-me-1988"),
            Some(RecordLocator::Slug(slug)) if slug == "answer-me-1988"
        ));
        // A movie link has no series segment.
        assert!(thetvdb_series_ref("https://thetvdb.com/dereferrer/movie/100").is_none());
        assert!(thetvdb_series_ref("https://thetvdb.com/movies/pu-239").is_none());
    }

    #[test]
    fn record_ref_parses_both_kinds_by_slug_and_id() {
        assert!(matches!(
            thetvdb_record_ref("https://thetvdb.com/movies/pu-239"),
            Some((RecordKind::Movie, RecordLocator::Slug(slug))) if slug == "pu-239"
        ));
        assert!(matches!(
            thetvdb_record_ref("https://thetvdb.com/dereferrer/movie/5239"),
            Some((RecordKind::Movie, RecordLocator::Id(id))) if id == "5239"
        ));
        // Trailing segments and query/fragment noise don't change the record.
        assert!(matches!(
            thetvdb_record_ref("https://thetvdb.com/series/answer-me-1988/episodes/official?tab=1"),
            Some((RecordKind::Series, RecordLocator::Slug(slug))) if slug == "answer-me-1988"
        ));
        // A numeric segment is an id, not a slug.
        assert!(matches!(
            thetvdb_record_ref("https://thetvdb.com/series/289882/"),
            Some((RecordKind::Series, RecordLocator::Id(id))) if id == "289882"
        ));
        // Kinds we can't turn into an entity, and links naming no record at all,
        // fall back to a text search.
        assert!(thetvdb_record_ref("https://thetvdb.com/people/1234-someone").is_none());
        assert!(thetvdb_record_ref("https://thetvdb.com/movies").is_none());
        assert!(thetvdb_record_ref("Evangelion 3.0+1.0").is_none());
    }

    #[test]
    fn candidate_uses_numeric_year_metadata() {
        let candidate = thetvdb_candidate(
            &json!({
                "tvdb_id": 123,
                "name": "Example Series",
                "year": 2026
            }),
            None,
        )
        .unwrap();

        assert_eq!(candidate.source_id, "123");
        assert_eq!(candidate.metadata.get("year"), Some(&json!("2026")));
        assert_eq!(candidate.metadata.get("first_air_time"), None);
    }

    #[test]
    fn candidate_prefers_first_air_time_over_year() {
        let candidate = thetvdb_candidate(
            &json!({
                "id": "series-123",
                "name": "Example Series",
                "first_air_time": "2026-04-12",
                "year": 2026
            }),
            None,
        )
        .unwrap();

        assert_eq!(
            candidate.metadata.get("first_air_time"),
            Some(&Value::String("2026-04-12".to_string()))
        );
        assert_eq!(candidate.metadata.get("year"), None);
    }

    #[test]
    fn candidate_surfaces_extended_metadata() {
        let candidate = thetvdb_candidate(
            &json!({
                "tvdb_id": 123,
                "name": "Example Series",
                "primary_language": "jpn",
                "country": "jpn",
                "status": "Continuing",
                "network": "TV Tokyo",
                "genres": ["Anime", "Action"],
                "studios": ["Studio X"],
                "remote_ids": [
                    { "id": "tt1234567", "sourceName": "IMDB" },
                    { "id": "98765", "sourceName": "TheMovieDB" }
                ]
            }),
            None,
        )
        .unwrap();

        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("primary_language"), Some(&json!("jpn")));
        assert_eq!(metadata.get("status"), Some(&json!("Continuing")));
        assert_eq!(metadata.get("network"), Some(&json!("TV Tokyo")));
        assert_eq!(metadata.get("genres"), Some(&json!(["Anime", "Action"])));
        assert_eq!(metadata.get("studios"), Some(&json!(["Studio X"])));
        assert_eq!(metadata.get("imdb_code"), Some(&json!("tt1234567")));
        assert_eq!(metadata.get("tmdb_id"), Some(&json!("98765")));
    }

    #[test]
    fn candidate_localizes_title_from_translations() {
        let item = json!({
            "tvdb_id": 456,
            "name": "Frieren: Beyond Journey's End",
            "primary_language": "jpn",
            "overview": "An elf mage's journey.",
            "translations": {
                "jpn": "葬送のフリーレン",
                "zho": "葬送的芙莉莲",
                "eng": "Frieren: Beyond Journey's End"
            },
            "overviews": { "zho": "一位精灵魔法师的旅程。" }
        });
        // A zh viewer sees the Chinese title and blurb…
        let zh = thetvdb_candidate(&item, Some("zh-Hans")).unwrap();
        assert_eq!(zh.title, "葬送的芙莉莲");
        assert_eq!(zh.brief.as_deref(), Some("一位精灵魔法师的旅程。"));
        // …and the localized blurb is what gets persisted, not the default one:
        // the schema mapper reads the overview from `metadata`, so it must match
        // `brief`, or quick-add would write the English overview it displayed in zh.
        assert_eq!(
            zh.metadata.get("overview"),
            Some(&json!("一位精灵魔法师的旅程。"))
        );
        // …a ja viewer the Japanese one…
        assert_eq!(
            thetvdb_candidate(&item, Some("ja")).unwrap().title,
            "葬送のフリーレン"
        );
        // …and a language without a translation falls back to the default name and
        // the default overview.
        let de = thetvdb_candidate(&item, Some("de")).unwrap();
        assert_eq!(de.title, "Frieren: Beyond Journey's End");
        assert_eq!(de.brief.as_deref(), Some("An elf mage's journey."));
        assert_eq!(
            de.metadata.get("overview"),
            Some(&json!("An elf mage's journey."))
        );
        // The original title stays the default name; known translations are tagged.
        assert_eq!(
            zh.original_title.as_deref(),
            Some("Frieren: Beyond Journey's End")
        );
        assert_eq!(zh.titles.get("zh"), Some(&"葬送的芙莉莲".to_string()));
        assert_eq!(zh.titles.get("ja"), Some(&"葬送のフリーレン".to_string()));
        assert_eq!(
            zh.titles.get("en"),
            Some(&"Frieren: Beyond Journey's End".to_string())
        );
    }

    #[test]
    fn candidate_url_names_the_records_own_kind_and_slug() {
        // A movie's ref must not claim to be a series: the URL is stored as the
        // external ref and is what re-resolves the record at quick-add time.
        let movie = thetvdb_candidate(
            &json!({ "tvdb_id": 5239, "name": "Evangelion", "type": "movie", "slug": "evangelion" }),
            None,
        )
        .unwrap();
        assert_eq!(movie.url, "https://thetvdb.com/movies/evangelion");

        let series = thetvdb_candidate(
            &json!({ "tvdb_id": 289882, "name": "Answer Me 1988", "type": "series", "slug": "answer-me-1988" }),
            None,
        )
        .unwrap();
        assert_eq!(series.url, "https://thetvdb.com/series/answer-me-1988");

        // Without a slug, the dereferrer form still carries the right kind.
        let slugless = thetvdb_candidate(
            &json!({ "tvdb_id": 5239, "name": "Evangelion", "type": "movie" }),
            None,
        )
        .unwrap();
        assert_eq!(slugless.url, "https://thetvdb.com/dereferrer/movie/5239");
    }

    #[test]
    fn extended_movie_record_maps_like_a_search_hit() {
        let record = json!({
            "id": 5239,
            "name": "Evangelion: 3.0+1.0 Thrice Upon a Time",
            "slug": "evangelion-3-0-1-0-thrice-upon-a-time",
            "image": "https://artworks.thetvdb.com/banners/movies/5239/poster.jpg",
            "originalLanguage": "jpn",
            "originalCountry": "jpn",
            "year": "2021",
            "first_release": { "country": "jpn", "date": "2021-03-08" },
            "status": { "id": 5, "name": "Released" },
            "genres": [{ "id": 1, "name": "Anime" }, { "id": 2, "name": "Science Fiction" }],
            "studios": [{ "id": 9, "name": "Studio Khara" }],
            "remoteIds": [{ "id": "tt2458948", "sourceName": "IMDB" }],
            "translations": {
                "nameTranslations": [
                    { "language": "jpn", "name": "シン・エヴァンゲリオン劇場版", "isPrimary": true },
                    { "language": "eng", "name": "Evangelion: 3.0+1.0 Thrice Upon a Time" }
                ],
                "overviewTranslations": [
                    { "language": "jpn", "overview": "終劇。", "isPrimary": true },
                    { "language": "eng", "overview": "The final Rebuild film." }
                ]
            }
        });
        let candidate =
            thetvdb_candidate(&thetvdb_record_item(RecordKind::Movie, &record), Some("en"))
                .unwrap();

        assert_eq!(candidate.source_id, "5239");
        assert_eq!(
            candidate.url,
            "https://thetvdb.com/movies/evangelion-3-0-1-0-thrice-upon-a-time"
        );
        assert_eq!(candidate.title, "Evangelion: 3.0+1.0 Thrice Upon a Time");
        assert_eq!(
            candidate.titles.get("ja").map(String::as_str),
            Some("シン・エヴァンゲリオン劇場版")
        );
        assert_eq!(candidate.brief.as_deref(), Some("The final Rebuild film."));
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://artworks.thetvdb.com/banners/movies/5239/poster.jpg")
        );
        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("first_air_time"), Some(&json!("2021-03-08")));
        assert_eq!(metadata.get("status"), Some(&json!("Released")));
        assert_eq!(metadata.get("primary_language"), Some(&json!("jpn")));
        assert_eq!(
            metadata.get("genres"),
            Some(&json!(["Anime", "Science Fiction"]))
        );
        assert_eq!(metadata.get("studios"), Some(&json!(["Studio Khara"])));
        assert_eq!(metadata.get("imdb_code"), Some(&json!("tt2458948")));
        // A movie carries no top-level overview; the primary translation is the
        // default a viewer without a translation falls back to.
        let ja = thetvdb_candidate(&thetvdb_record_item(RecordKind::Movie, &record), Some("de"))
            .unwrap();
        assert_eq!(ja.brief.as_deref(), Some("終劇。"));
    }
}
