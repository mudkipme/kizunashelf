use super::{
    external_client, field_option, provider_error, type_option, ExternalProvider,
    ProviderSearchConfig, USER_AGENT,
};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(super) struct MangaUpdatesProvider;

impl ExternalProvider for MangaUpdatesProvider {
    const ID: &'static str = "mangaupdates";
    const LABEL: &'static str = "MangaUpdates";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        manga_supported(provider_config)
    }

    fn default_external_types() -> &'static [&'static str] {
        &["manga"]
    }

    fn field_options() -> Vec<ExternalProviderFieldOption> {
        field_options()
    }

    fn type_options() -> Vec<ExternalProviderTypeOption> {
        type_options()
    }

    async fn search(
        _state: &super::AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        search_mangaupdates(q, page, page_size, provider_config).await
    }
}

pub(super) fn manga_supported(provider_config: &ProviderSearchConfig) -> bool {
    match provider_config.external_types() {
        None => true,
        Some(types) => types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case("manga")),
    }
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("title", "Title"),
        field_option("format", "Type"),
        field_option("year", "Year"),
        field_option("status", "Status"),
        field_option("latest_chapter", "Latest chapter"),
        field_option("score", "Score"),
        field_option("synopsis", "Synopsis"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("authors", "Authors"),
        field_option("genres", "Genres"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("manga", "Manga")]
}

async fn search_mangaupdates(
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !manga_supported(provider_config) {
        return Ok(Vec::new());
    }
    let client = external_client();
    // A pasted MangaUpdates URL or bare numeric id resolves a single series.
    if let Some(id) = mangaupdates_id(q) {
        let value = client
            .get(format!("https://api.mangaupdates.com/v1/series/{id}"))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        return Ok(mangaupdates_detail(&value).into_iter().collect());
    }
    let response = client
        .post("https://api.mangaupdates.com/v1/series/search")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .json(&json!({
            "search": q,
            "stype": "title",
            "perpage": page_size,
            "page": page,
        }))
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    let results = response
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(results
        .iter()
        .filter_map(|result| mangaupdates_search_record(result.get("record")?))
        .collect())
}

/// Extracts a MangaUpdates numeric series id from a URL (`series.html?id=…` or
/// `/series/<id>`) or a bare numeric id. The base-36 slug URLs aren't resolved.
fn mangaupdates_id(q: &str) -> Option<String> {
    let trimmed = q.trim().trim_end_matches('/');
    if !trimmed.is_empty() && trimmed.chars().all(|character| character.is_ascii_digit()) {
        return Some(trimmed.to_string());
    }
    if !trimmed.contains("mangaupdates.com") {
        return None;
    }
    let candidate = if let Some((_, rest)) = trimmed.split_once("id=") {
        rest.split(['&', '#']).next().unwrap_or_default()
    } else if let Some((_, rest)) = trimmed.split_once("/series/") {
        rest.split(['/', '?', '#']).next().unwrap_or_default()
    } else {
        ""
    };
    (!candidate.is_empty()
        && candidate
            .chars()
            .all(|character| character.is_ascii_digit()))
    .then(|| candidate.to_string())
}

fn mangaupdates_search_record(record: &Value) -> Option<ExternalCandidate> {
    let id = record
        .get("series_id")
        .and_then(Value::as_i64)
        .map(|id| id.to_string())?;
    let title = record
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let cover_url = mangaupdates_image(record);
    let url = mangaupdates_url(&id);
    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: MangaUpdatesProvider::ID.to_string(),
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

fn mangaupdates_detail(series: &Value) -> Option<ExternalCandidate> {
    let id = series
        .get("series_id")
        .and_then(Value::as_i64)
        .map(|id| id.to_string())?;
    let title = series
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    // Use the id-based URL (not the API's slug/base-36 `url`) so the same series
    // yields an identical, round-trippable `externalRef` from search and resolve.
    let url = mangaupdates_url(&id);
    let cover_url = mangaupdates_image(series);
    let synopsis = series
        .get("description")
        .and_then(Value::as_str)
        .map(strip_html)
        .filter(|value| !value.is_empty());

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    insert_str(&mut metadata, "format", series.get("type"));
    insert_str(&mut metadata, "status", series.get("status"));
    if let Some(year) = series
        .get("year")
        .and_then(non_empty_string_or_integer)
        .filter(|year| year != "0")
    {
        metadata.insert("year".to_string(), Value::String(year));
    }
    if let Some(latest_chapter) = series
        .get("latest_chapter")
        .and_then(Value::as_i64)
        .filter(|chapter| *chapter > 0)
    {
        metadata.insert(
            "latest_chapter".to_string(),
            Value::Number(latest_chapter.into()),
        );
    }
    // `bayesian_rating` is a 0–10 score.
    if let Some(score) = series
        .get("bayesian_rating")
        .and_then(Value::as_f64)
        .filter(|score| *score > 0.0)
    {
        metadata.insert("score".to_string(), json!((score * 10.0).round() / 10.0));
    }
    if let Some(authors) = named_list(series.get("authors"), "name") {
        metadata.insert("authors".to_string(), authors);
    }
    if let Some(genres) = named_list(series.get("genres"), "genre") {
        metadata.insert("genres".to_string(), genres);
    }
    if let Some(synopsis) = &synopsis {
        metadata.insert("synopsis".to_string(), Value::String(synopsis.clone()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: MangaUpdatesProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(title.clone()),
        title,
        brief: synopsis,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

/// Canonical id-based MangaUpdates URL. The legacy `series.html?id=` form is
/// permanent, redirects to the current slug page, and round-trips through
/// [`mangaupdates_id`] — unlike the base-36 slug URL the API returns.
fn mangaupdates_url(id: &str) -> String {
    format!("https://www.mangaupdates.com/series.html?id={id}")
}

fn mangaupdates_image(value: &Value) -> Option<String> {
    value
        .get("image")
        .and_then(|image| image.get("url"))
        .and_then(|url| url.get("original").or_else(|| url.get("thumb")))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn named_list(value: Option<&Value>, key: &str) -> Option<Value> {
    let names: Vec<Value> = value?
        .as_array()?
        .iter()
        .filter_map(|entry| entry.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| Value::String(name.to_string()))
        .collect();
    (!names.is_empty()).then_some(Value::Array(names))
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

fn non_empty_string_or_integer(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
}

/// Strips HTML tags from a description, keeping the text content.
fn strip_html(value: &str) -> String {
    let value = value.replace("<br", "\n<br");
    let mut out = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(character),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{mangaupdates_detail, mangaupdates_id, mangaupdates_search_record};
    use serde_json::json;

    #[test]
    fn detail_surfaces_metadata() {
        let candidate = mangaupdates_detail(&json!({
            "series_id": 51239621230_i64,
            "title": "Berserk",
            "url": "https://www.mangaupdates.com/series/abc/berserk",
            "description": "A <b>dark</b> fantasy.",
            "type": "Manga",
            "year": "1989",
            "status": "Ongoing",
            "latest_chapter": 374,
            "bayesian_rating": 9.12,
            "authors": [{ "name": "Kentaro Miura" }],
            "genres": [{ "genre": "Action" }, { "genre": "Fantasy" }],
            "image": { "url": { "original": "https://img/berserk.jpg" } }
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "51239621230");
        assert_eq!(candidate.metadata.get("format"), Some(&json!("Manga")));
        assert_eq!(candidate.metadata.get("year"), Some(&json!("1989")));
        assert_eq!(candidate.metadata.get("latest_chapter"), Some(&json!(374)));
        assert_eq!(candidate.metadata.get("score"), Some(&json!(9.1)));
        assert_eq!(
            candidate.metadata.get("authors"),
            Some(&json!(["Kentaro Miura"]))
        );
        assert_eq!(
            candidate.metadata.get("genres"),
            Some(&json!(["Action", "Fantasy"]))
        );
        assert_eq!(
            candidate.metadata.get("synopsis"),
            Some(&json!("A dark fantasy."))
        );
    }

    #[test]
    fn search_record_builds_candidate() {
        let candidate = mangaupdates_search_record(&json!({
            "series_id": 12345,
            "title": "One Piece",
            "image": { "url": { "original": "https://img/op.jpg" } }
        }))
        .unwrap();
        assert_eq!(candidate.source_id, "12345");
        assert_eq!(
            candidate.url,
            "https://www.mangaupdates.com/series.html?id=12345"
        );
    }

    #[test]
    fn search_url_round_trips_through_resolve() {
        let candidate = mangaupdates_search_record(&json!({
            "series_id": 12345, "title": "One Piece"
        }))
        .unwrap();
        assert_eq!(
            super::mangaupdates_id(&candidate.url),
            Some("12345".to_string())
        );
    }

    #[test]
    fn id_parses_url_and_bare_id() {
        assert_eq!(
            mangaupdates_id("https://www.mangaupdates.com/series.html?id=51239621230"),
            Some("51239621230".to_string())
        );
        assert_eq!(mangaupdates_id("12345"), Some("12345".to_string()));
        assert_eq!(mangaupdates_id("berserk"), None);
    }
}
