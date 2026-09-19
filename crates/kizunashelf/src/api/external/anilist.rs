//! AniList provider: keyless GraphQL (`graphql.anilist.co`) search/resolve for
//! anime and manga, plus episode import. AniList has no first-class episode
//! list; episodes are assembled from two complementary sources — the airing
//! schedule (numbers + air dates, present for shows that aired while AniList
//! tracked them) and streaming episodes (`"Episode N - Title"` strings from
//! streaming partners, present for licensed shows) — with the plain episode
//! count as a last-resort numbered fallback.

use super::{
    external_client, field_option, provider_error, send_limited, strip_html, title_case,
    type_option, url_type_allowed, ExternalProvider, ProviderResponseExt, ProviderSearchConfig,
    USER_AGENT,
};
use crate::api::state::AppState;
use crate::api::ApiError;
use crate::contract::{
    ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption,
    ProviderEpisodeGroup, ProviderEpisodeItem, ProviderEpisodes,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(super) struct AniListProvider;

const ENDPOINT: &str = "https://graphql.anilist.co";

/// The media selection shared by search and URL resolve — one GraphQL request
/// returns full candidate detail, so both paths build the same candidate.
const MEDIA_FRAGMENT: &str = "\
fragment media on Media {
  id type format status(version: 2) description
  startDate { year month day } endDate { year month day }
  season seasonYear episodes chapters volumes duration source averageScore genres
  title { romaji english native } countryOfOrigin
  coverImage { extraLarge large }
  studios(isMain: true) { nodes { name } }
}";

impl ExternalProvider for AniListProvider {
    const ID: &'static str = "anilist";
    const LABEL: &'static str = "AniList";

    fn recognizes_url(q: &str) -> bool {
        anilist_ref(q).is_some()
    }

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        !anilist_kinds(provider_config).is_empty()
    }

    fn field_options() -> Vec<ExternalProviderFieldOption> {
        vec![
            field_option("title", "Title"),
            field_option("romaji_title", "Romaji title"),
            field_option("native_title", "Native title"),
            field_option("cover_url", "Cover URL"),
            field_option("format", "Type"),
            field_option("start_date", "Start date"),
            field_option("end_date", "End date"),
            field_option("status", "Status"),
            field_option("episodes", "Episodes"),
            field_option("chapters", "Chapters"),
            field_option("volumes", "Volumes"),
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

    fn type_options() -> Vec<ExternalProviderTypeOption> {
        vec![type_option("anime", "Anime"), type_option("manga", "Manga")]
    }

    async fn search(
        _state: &AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        search_anilist(q, page, page_size, provider_config).await
    }

    const SUPPORTS_EPISODES: bool = true;

    async fn fetch_episodes(
        _state: &AppState,
        ref_value: &str,
        _language: Option<&str>,
    ) -> Result<ProviderEpisodes, ApiError> {
        fetch_anilist_episodes(ref_value).await
    }
}

/// The media kinds (`anime`, `manga`) this field searches. Unconstrained
/// searches both.
fn anilist_kinds(provider_config: &ProviderSearchConfig) -> Vec<&'static str> {
    let Some(types) = provider_config.external_types() else {
        return vec!["anime", "manga"];
    };
    let mut kinds = Vec::new();
    for kind in ["anime", "manga"] {
        if types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case(kind))
            && !kinds.contains(&kind)
        {
            kinds.push(kind);
        }
    }
    kinds
}

/// Detects an `anilist.co/{anime|manga}/{id}` URL.
fn anilist_ref(q: &str) -> Option<(&'static str, String)> {
    let trimmed = q.trim();
    let (_, rest) = trimmed.split_once("anilist.co/")?;
    let mut parts = rest.split('/');
    let kind = match parts.next()? {
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
        .then(|| (kind, id.to_string()))
}

/// The AniList anime id from a stored ref: an `…/anime/{id}` URL, or a bare
/// numeric id (assumed anime). A manga link has no episode list and yields `None`.
fn anilist_anime_id(ref_value: &str) -> Option<String> {
    match anilist_ref(ref_value) {
        Some(("anime", id)) => Some(id),
        Some(_) => None,
        None => {
            let trimmed = ref_value.trim();
            (!trimmed.is_empty() && trimmed.chars().all(|character| character.is_ascii_digit()))
                .then(|| trimmed.to_string())
        }
    }
}

/// The GraphQL `MediaType` enum value for a media kind.
fn gql_media_type(kind: &str) -> &'static str {
    if kind == "manga" {
        "MANGA"
    } else {
        "ANIME"
    }
}

/// Posts one GraphQL request and returns its `data`, surfacing the first
/// GraphQL error message when `data` is null (AniList reports user errors —
/// not found, bad variables — this way alongside the HTTP status).
async fn anilist_graphql(query: &str, variables: Value) -> Result<Value, ApiError> {
    let client = external_client();
    let value = send_limited(
        client
            .post(ENDPOINT)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .json(&json!({ "query": query, "variables": variables })),
    )
    .await
    .map_err(provider_error)?
    .error_for_status_body()
    .await?
    .json::<Value>()
    .await
    .map_err(provider_error)?;
    if value.get("data").map(Value::is_null).unwrap_or(true) {
        let message = value
            .get("errors")
            .and_then(Value::as_array)
            .and_then(|errors| errors.first())
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("AniList request failed");
        return Err(ApiError::bad_gateway(&format!("AniList: {message}")));
    }
    Ok(value)
}

async fn search_anilist(
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let kinds = anilist_kinds(provider_config);
    if kinds.is_empty() {
        return Ok(Vec::new());
    }
    // A pasted AniList URL resolves a single record. Only surface it under a
    // field that accepts that media kind, so it doesn't appear once per
    // anilist-mapped entity type.
    if let Some((kind, id)) = anilist_ref(q) {
        if !url_type_allowed(provider_config, kind) {
            return Ok(Vec::new());
        }
        return resolve_anilist(kind, &id).await;
    }
    let query = format!(
        "query ($search: String, $type: MediaType, $page: Int, $perPage: Int) {{\n\
         Page(page: $page, perPage: $perPage) {{ media(search: $search, type: $type) {{ ...media }} }} }}\n\
         {MEDIA_FRAGMENT}"
    );
    let mut items = Vec::new();
    for kind in kinds {
        let value = anilist_graphql(
            &query,
            json!({
                "search": q,
                "type": gql_media_type(kind),
                "page": page,
                "perPage": page_size,
            }),
        )
        .await?;
        if let Some(media) = value.pointer("/data/Page/media").and_then(Value::as_array) {
            items.extend(media.iter().filter_map(anilist_candidate));
        }
    }
    Ok(items)
}

async fn resolve_anilist(kind: &str, id: &str) -> Result<Vec<ExternalCandidate>, ApiError> {
    let query = format!(
        "query ($id: Int, $type: MediaType) {{ Media(id: $id, type: $type) {{ ...media }} }}\n\
         {MEDIA_FRAGMENT}"
    );
    let value = anilist_graphql(
        &query,
        json!({ "id": id.parse::<i64>().ok(), "type": gql_media_type(kind) }),
    )
    .await?;
    Ok(value
        .pointer("/data/Media")
        .and_then(anilist_candidate)
        .into_iter()
        .collect())
}

/// Builds a full candidate from one media node ([`MEDIA_FRAGMENT`]'s selection).
fn anilist_candidate(media: &Value) -> Option<ExternalCandidate> {
    let id = media.get("id").and_then(Value::as_i64)?.to_string();
    let kind = match media.get("type").and_then(Value::as_str) {
        Some("MANGA") => "manga",
        _ => "anime",
    };
    let english = media
        .pointer("/title/english")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let romaji = media
        .pointer("/title/romaji")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let native = media
        .pointer("/title/native")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let title = english.or(romaji).or(native)?.to_string();
    let cover_url = media
        .get("coverImage")
        .and_then(|image| image.get("extraLarge").or_else(|| image.get("large")))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let synopsis = media
        .get("description")
        .and_then(Value::as_str)
        .map(strip_html)
        .filter(|value| !value.is_empty());

    // The native title's language comes from the media's origin country —
    // AniList also lists Korean/Chinese works, so `native` must not be assumed
    // Japanese. Romaji stays untagged (it is no language's display title).
    let mut titles = BTreeMap::new();
    if let Some(english) = english {
        titles.insert("en".to_string(), english.to_string());
    }
    let native_language = media
        .get("countryOfOrigin")
        .and_then(Value::as_str)
        .and_then(anilist_origin_language);
    if let (Some(native), Some(language)) = (native, native_language) {
        titles.insert(language.to_string(), native.to_string());
    }

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(romaji) = romaji {
        metadata.insert(
            "romaji_title".to_string(),
            Value::String(romaji.to_string()),
        );
    }
    if let Some(native) = native {
        metadata.insert(
            "native_title".to_string(),
            Value::String(native.to_string()),
        );
    }
    if let Some(format) = anilist_format(media) {
        metadata.insert("format".to_string(), Value::String(format));
    }
    if let Some(status) = anilist_status(media, kind) {
        metadata.insert("status".to_string(), Value::String(status));
    }
    if let Some(date) = media.get("startDate").and_then(anilist_fuzzy_date) {
        metadata.insert("start_date".to_string(), Value::String(date));
    }
    if let Some(date) = media.get("endDate").and_then(anilist_fuzzy_date) {
        metadata.insert("end_date".to_string(), Value::String(date));
    }
    for (key, source) in [
        ("episodes", "episodes"),
        ("chapters", "chapters"),
        ("volumes", "volumes"),
        ("runtime", "duration"),
    ] {
        if let Some(count) = media
            .get(source)
            .and_then(Value::as_i64)
            .filter(|count| *count > 0)
        {
            metadata.insert(key.to_string(), Value::Number(count.into()));
        }
    }
    if let (Some(season), Some(year)) = (
        media.get("season").and_then(Value::as_str),
        media.get("seasonYear").and_then(Value::as_i64),
    ) {
        metadata.insert(
            "season".to_string(),
            Value::String(format!("{} {year}", title_case(&season.to_lowercase()))),
        );
    }
    if let Some(source) = media
        .get("source")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "source".to_string(),
            Value::String(title_case(&source.to_lowercase())),
        );
    }
    // `averageScore` is 0–100; surface it on the 10-point scale other trackers use.
    if let Some(score) = media
        .get("averageScore")
        .and_then(Value::as_i64)
        .filter(|score| *score > 0)
    {
        metadata.insert("score".to_string(), json!(score as f64 / 10.0));
    }
    if let Some(genres) = media.get("genres").and_then(Value::as_array) {
        let genres: Vec<Value> = genres
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| Value::String(value.to_string()))
            .collect();
        if !genres.is_empty() {
            metadata.insert("genres".to_string(), Value::Array(genres));
        }
    }
    if let Some(studios) = super::named_list(media.pointer("/studios/nodes")) {
        metadata.insert("studios".to_string(), studios);
    }
    if let Some(synopsis) = &synopsis {
        metadata.insert("synopsis".to_string(), Value::String(synopsis.clone()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        needs_detail: false,
        provider: AniListProvider::ID.to_string(),
        source_id: id.clone(),
        url: format!("https://anilist.co/{kind}/{id}"),
        original_title: native.or(romaji).map(str::to_string),
        title,
        brief: synopsis,
        cover_url,
        titles,
        metadata,
    })
}

/// Maps AniList's `MediaFormat` to a readable format (TV→Anime, matching the
/// MyAnimeList provider's wording so shared enum fields line up).
fn anilist_format(media: &Value) -> Option<String> {
    let format = media.get("format").and_then(Value::as_str)?;
    let readable = match format {
        "" => return None,
        "TV" => "Anime".to_string(),
        "TV_SHORT" => "TV Short".to_string(),
        "OVA" | "ONA" => format.to_string(),
        "NOVEL" => "Light Novel".to_string(),
        "ONE_SHOT" => "One-shot".to_string(),
        other => title_case(&other.to_lowercase()),
    };
    Some(readable)
}

/// Maps AniList's `MediaStatus` (version 2) to a human-readable label.
/// `RELEASING` reads per medium: anime airs, manga publishes.
fn anilist_status(media: &Value, kind: &str) -> Option<String> {
    let status = media.get("status").and_then(Value::as_str)?;
    let readable = match status {
        "FINISHED" => "Finished",
        "RELEASING" if kind == "manga" => "Publishing",
        "RELEASING" => "Airing",
        "NOT_YET_RELEASED" => "Upcoming",
        "CANCELLED" => "Cancelled",
        "HIATUS" => "On Hiatus",
        "" => return None,
        other => return Some(title_case(&other.to_lowercase())),
    };
    Some(readable.to_string())
}

/// The title language an AniList `countryOfOrigin` implies for `native`.
/// Chinese works are tagged bare `zh` regardless of origin script — script
/// subtags never enter title maps.
pub(crate) fn anilist_origin_language(region: &str) -> Option<&'static str> {
    match region {
        "JP" => Some("ja"),
        "KR" => Some("ko"),
        "CN" | "TW" => Some("zh"),
        _ => None,
    }
}

/// An AniList fuzzy date `{ year, month, day }` → an ISO string, using whatever
/// parts are present. `None` when there's no year.
pub(crate) fn anilist_fuzzy_date(value: &Value) -> Option<String> {
    let year = value.get("year").and_then(Value::as_i64)?;
    let month = value.get("month").and_then(Value::as_i64);
    let day = value.get("day").and_then(Value::as_i64);
    Some(match (month, day) {
        (Some(month), Some(day)) => format!("{year:04}-{month:02}-{day:02}"),
        (Some(month), None) => format!("{year:04}-{month:02}"),
        _ => format!("{year:04}"),
    })
}

/// Fetches an anime's episodes as one flat list, merged from the airing
/// schedule (numbers + dates; includes announced future episodes, like the
/// TheTVDB provider) and streaming-episode titles, falling back to numbered
/// items from the plain episode count when neither source has data.
async fn fetch_anilist_episodes(ref_value: &str) -> Result<ProviderEpisodes, ApiError> {
    let id = anilist_anime_id(ref_value)
        .ok_or_else(|| ApiError::bad_request("Not an AniList anime link or id"))?;
    let query = "\
query ($id: Int, $page: Int) {
  Media(id: $id, type: ANIME) {
    episodes
    streamingEpisodes { title }
    airingSchedule(page: $page, perPage: 50) {
      nodes { episode airingAt }
      pageInfo { hasNextPage }
    }
  }
}";
    let mut items: BTreeMap<i64, ProviderEpisodeItem> = BTreeMap::new();
    let mut count = None;
    let mut streaming: Vec<String> = Vec::new();
    let mut page = 1i64;
    // Page through the airing schedule until it reports no next page, with a
    // hard cap so a malformed response can't loop forever. The episode count
    // and streaming list ride on the first page only.
    loop {
        let value =
            anilist_graphql(query, json!({ "id": id.parse::<i64>().ok(), "page": page })).await?;
        let Some(media) = value
            .pointer("/data/Media")
            .filter(|media| !media.is_null())
        else {
            break;
        };
        if page == 1 {
            count = media.get("episodes").and_then(Value::as_i64);
            if let Some(list) = media.get("streamingEpisodes").and_then(Value::as_array) {
                streaming = list
                    .iter()
                    .filter_map(|episode| episode.get("title").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect();
            }
        }
        if let Some(nodes) = media
            .pointer("/airingSchedule/nodes")
            .and_then(Value::as_array)
        {
            for node in nodes {
                let Some(number) = node.get("episode").and_then(Value::as_i64) else {
                    continue;
                };
                let date = node
                    .get("airingAt")
                    .and_then(Value::as_i64)
                    .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
                    .map(|moment| moment.date_naive().to_string());
                items.entry(number).or_insert(ProviderEpisodeItem {
                    key: number.to_string(),
                    title: String::new(),
                    date,
                });
            }
        }
        let has_next = media
            .pointer("/airingSchedule/pageInfo/hasNextPage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        page += 1;
        if !has_next || page > 50 {
            break;
        }
    }
    // Overlay streaming titles onto the schedule; a title for an episode the
    // schedule doesn't know (older shows have no schedule at all) adds an item
    // without a date. Duplicate numbers (sub + dub listings) keep the first.
    for title in &streaming {
        let Some((number, name)) = parse_streaming_title(title) else {
            continue;
        };
        let item = items.entry(number).or_insert(ProviderEpisodeItem {
            key: number.to_string(),
            title: String::new(),
            date: None,
        });
        if item.title.is_empty() {
            item.title = name;
        }
    }
    // Neither source knew anything: fall back to numbered items from the count
    // (finished shows carry one), so tracking still works for unlicensed shows.
    if items.is_empty() {
        for number in 1..=count.unwrap_or(0).max(0) {
            items.insert(
                number,
                ProviderEpisodeItem {
                    key: number.to_string(),
                    title: String::new(),
                    date: None,
                },
            );
        }
    }
    Ok(ProviderEpisodes {
        groups: vec![ProviderEpisodeGroup {
            label: String::new(),
            items: items.into_values().collect(),
        }],
    })
}

/// Parses a streaming-episode title of the form `"Episode N - Title"` (or bare
/// `"Episode N"`). Anything else — recaps, movies, oddly labeled entries —
/// yields `None` and is skipped.
fn parse_streaming_title(value: &str) -> Option<(i64, String)> {
    let rest = value.trim().strip_prefix("Episode ")?;
    let digits_end = rest
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(rest.len());
    let number = rest[..digits_end].parse::<i64>().ok()?;
    let remainder = rest[digits_end..].trim_start();
    if remainder.is_empty() {
        return Some((number, String::new()));
    }
    // A non-empty remainder must be a `- Title` separator; anything else
    // (e.g. "Episode 10.5") isn't a plain numbered episode.
    let title = remainder.strip_prefix('-')?.trim();
    Some((number, title.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        anilist_anime_id, anilist_candidate, anilist_fuzzy_date, anilist_ref, parse_streaming_title,
    };
    use serde_json::json;

    #[test]
    fn detail_surfaces_metadata() {
        let candidate = anilist_candidate(&json!({
            "id": 1,
            "type": "ANIME",
            "format": "TV",
            "status": "FINISHED",
            "description": "A <b>space</b> western.<br>Bounty hunters.",
            "startDate": { "year": 1998, "month": 4, "day": 3 },
            "endDate": { "year": 1999, "month": 4, "day": null },
            "season": "SPRING",
            "seasonYear": 1998,
            "episodes": 26,
            "duration": 24,
            "source": "ORIGINAL",
            "averageScore": 86,
            "genres": ["Action", "Sci-Fi"],
            "title": { "romaji": "Cowboy Bebop", "english": "Cowboy Bebop", "native": "カウボーイビバップ" },
            "countryOfOrigin": "JP",
            "coverImage": { "extraLarge": "https://img/cb.jpg" },
            "studios": { "nodes": [{ "name": "Sunrise" }] }
        }))
        .unwrap();

        assert_eq!(candidate.url, "https://anilist.co/anime/1");
        assert_eq!(
            candidate.original_title.as_deref(),
            Some("カウボーイビバップ")
        );
        assert_eq!(
            candidate.titles.get("ja").map(String::as_str),
            Some("カウボーイビバップ")
        );
        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("format"), Some(&json!("Anime")));
        assert_eq!(metadata.get("status"), Some(&json!("Finished")));
        assert_eq!(metadata.get("start_date"), Some(&json!("1998-04-03")));
        assert_eq!(metadata.get("end_date"), Some(&json!("1999-04")));
        assert_eq!(metadata.get("episodes"), Some(&json!(26)));
        assert_eq!(metadata.get("runtime"), Some(&json!(24)));
        assert_eq!(metadata.get("season"), Some(&json!("Spring 1998")));
        assert_eq!(metadata.get("source"), Some(&json!("Original")));
        assert_eq!(metadata.get("score"), Some(&json!(8.6)));
        assert_eq!(metadata.get("studios"), Some(&json!(["Sunrise"])));
        assert_eq!(
            metadata.get("synopsis"),
            Some(&json!("A space western.\nBounty hunters."))
        );
    }

    #[test]
    fn releasing_manga_reads_publishing() {
        let candidate = anilist_candidate(&json!({
            "id": 30002,
            "type": "MANGA",
            "status": "RELEASING",
            "chapters": null,
            "title": { "romaji": "Berserk" }
        }))
        .unwrap();
        assert_eq!(candidate.url, "https://anilist.co/manga/30002");
        assert_eq!(candidate.metadata.get("status"), Some(&json!("Publishing")));
    }

    #[test]
    fn ref_parses_anime_and_manga_urls() {
        assert_eq!(
            anilist_ref("https://anilist.co/anime/198376/BanG-Dream-YumeMita/"),
            Some(("anime", "198376".to_string()))
        );
        assert_eq!(
            anilist_ref("https://anilist.co/manga/30002/Berserk"),
            Some(("manga", "30002".to_string()))
        );
        assert_eq!(anilist_ref("https://anilist.co/user/somebody"), None);
        assert_eq!(anilist_ref("cowboy bebop"), None);
    }

    #[test]
    fn anime_id_accepts_anime_url_and_bare_id_but_not_manga() {
        assert_eq!(
            anilist_anime_id("https://anilist.co/anime/1/Cowboy-Bebop"),
            Some("1".to_string())
        );
        assert_eq!(anilist_anime_id("1"), Some("1".to_string()));
        assert_eq!(
            anilist_anime_id("https://anilist.co/manga/30002/Berserk"),
            None
        );
    }

    #[test]
    fn streaming_titles_parse_numbered_episodes_only() {
        assert_eq!(
            parse_streaming_title("Episode 1 - Asteroid Blues"),
            Some((1, "Asteroid Blues".to_string()))
        );
        assert_eq!(
            parse_streaming_title("Episode 12"),
            Some((12, String::new()))
        );
        assert_eq!(parse_streaming_title("Episode 10.5 - Recap"), None);
        assert_eq!(parse_streaming_title("The Movie"), None);
    }

    #[test]
    fn fuzzy_dates_use_available_parts() {
        assert_eq!(
            anilist_fuzzy_date(&json!({ "year": 2020, "month": 1, "day": 3 })).as_deref(),
            Some("2020-01-03")
        );
        assert_eq!(
            anilist_fuzzy_date(&json!({ "year": 2020, "month": 2, "day": null })).as_deref(),
            Some("2020-02")
        );
        assert_eq!(anilist_fuzzy_date(&json!({ "year": null })), None);
    }
}
