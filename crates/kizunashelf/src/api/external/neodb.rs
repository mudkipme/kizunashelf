//! NeoDB (https://neodb.social) — a federated, community-run media catalog
//! covering books, movies, TV, music, games, podcasts, and performances. The
//! catalog read API is fully anonymous (no key, no app registration): free-text
//! search via `/api/catalog/search` and per-item detail via `/api/{path}/{uuid}`.
//! Only the flagship instance is supported; the search index covers its own
//! catalog. Search results carry the generic item shape (titles, cover, rating,
//! tags); the typed per-category fields (authors, ISBN, directors, platforms, …)
//! come from the by-URL resolve, which the detail-fetch flows trigger.

use super::{
    external_client, field_option, insert_str, provider_error, send_limited, string_list,
    string_list_with, type_option, ExternalProvider, ProviderResponseExt, ProviderSearchConfig,
    USER_AGENT,
};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use crate::languages::primary_language;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct NeoDbProvider;

const BASE: &str = "https://neodb.social";

/// NeoDB's searchable categories — also this provider's external-type taxonomy.
const CATEGORIES: &[&str] = &[
    "book",
    "movie",
    "tv",
    "music",
    "game",
    "podcast",
    "performance",
];

impl ExternalProvider for NeoDbProvider {
    const ID: &'static str = "neodb";
    const LABEL: &'static str = "NeoDB";

    fn recognizes_url(q: &str) -> bool {
        neodb_item_path(q).is_some()
    }

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        neodb_supported(provider_config)
    }

    fn field_options() -> Vec<ExternalProviderFieldOption> {
        field_options()
    }

    fn type_options() -> Vec<ExternalProviderTypeOption> {
        CATEGORIES
            .iter()
            .map(|category| type_option(category, &capitalized(category)))
            .collect()
    }

    async fn search(
        _state: &super::AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        search_neodb(q, page, page_size, provider_config).await
    }
}

fn capitalized(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

fn neodb_supported(provider_config: &ProviderSearchConfig) -> bool {
    match provider_config.external_types() {
        None => true,
        Some(types) => types.iter().any(|external_type| {
            CATEGORIES.contains(&external_type.trim().to_lowercase().as_str())
        }),
    }
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("title", "Title"),
        field_option("original_title", "Original title"),
        field_option("cover_url", "Cover URL"),
        field_option("description", "Description"),
        field_option("rating", "Rating"),
        field_option("rating_count", "Rating count"),
        field_option("release_date", "Release date"),
        field_option("published_date", "Published date"),
        field_option("pages", "Pages"),
        field_option("isbn", "ISBN"),
        field_option("language", "Language"),
        field_option("series", "Series"),
        field_option("format", "Format"),
        field_option("episode_count", "Episode count"),
        field_option("season_count", "Season count"),
        field_option("season_number", "Season number"),
        field_option("official_site", "Official site"),
        field_option("opening_date", "Opening date"),
        field_option("closing_date", "Closing date"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("tags", "Tags"),
        field_option("authors", "Authors"),
        field_option("translators", "Translators"),
        field_option("publishers", "Publishers"),
        field_option("directors", "Directors"),
        field_option("playwrights", "Playwrights"),
        field_option("original_creators", "Original creators"),
        field_option("composers", "Composers"),
        field_option("choreographers", "Choreographers"),
        field_option("performers", "Performers"),
        field_option("actors", "Actors"),
        field_option("crew", "Crew"),
        field_option("genres", "Genres"),
        field_option("artists", "Artists"),
        field_option("developers", "Developers"),
        field_option("platforms", "Platforms"),
        field_option("hosts", "Hosts"),
        field_option("company", "Company"),
    ]
}

async fn search_neodb(
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !neodb_supported(provider_config) {
        return Ok(Vec::new());
    }
    let client = external_client();
    // A pasted NeoDB item URL resolves to that single record.
    if let Some(path) = neodb_item_path(q) {
        return resolve_neodb(client, &path).await;
    }
    let configured = configured_categories(provider_config);
    let mut request = client
        .get(format!("{BASE}/api/catalog/search"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[("query", q), ("page", &page.to_string())]);
    if let Some(category) = category_param(configured.as_deref()) {
        request = request.query(&[("category", category)]);
    }
    let response = send_limited(request)
        .await
        .map_err(provider_error)?
        .error_for_status_body()
        .await?
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
        .filter_map(neodb_item)
        .filter(|candidate| {
            // The API's category filter only covers the single-category (and
            // movie+tv) cases; any other combination is filtered here.
            match &configured {
                Some(categories) => candidate
                    .metadata
                    .get("category")
                    .and_then(Value::as_str)
                    .is_some_and(|category| categories.contains(&category)),
                None => true,
            }
        })
        .take(page_size)
        .collect())
}

/// The configured external types that are real NeoDB categories, in
/// [`CATEGORIES`] order. `None` = unconstrained.
fn configured_categories(provider_config: &ProviderSearchConfig) -> Option<Vec<&'static str>> {
    let types = provider_config.external_types()?;
    Some(
        CATEGORIES
            .iter()
            .filter(|category| {
                types
                    .iter()
                    .any(|external_type| external_type.trim().eq_ignore_ascii_case(category))
            })
            .copied()
            .collect(),
    )
}

/// The `category` query parameter for a configured-category set, when the API
/// can express it: a single category, or the special `movie,tv` pair.
fn category_param(configured: Option<&[&'static str]>) -> Option<&'static str> {
    let categories = configured?;
    match categories {
        [single] => Some(single),
        ["movie", "tv"] => Some("movie,tv"),
        _ => None,
    }
}

/// Resolves one item by its site path (e.g. `book/7RTU2H1234`), which maps
/// mechanically onto the API path.
async fn resolve_neodb(
    client: &reqwest::Client,
    path: &str,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let value = send_limited(
        client
            .get(format!("{BASE}/api/{path}"))
            .header(reqwest::header::USER_AGENT, USER_AGENT),
    )
    .await
    .map_err(provider_error)?
    .error_for_status_body()
    .await?
    .json::<Value>()
    .await
    .map_err(provider_error)?;
    Ok(neodb_item(&value).into_iter().collect())
}

/// Maps one NeoDB item payload (generic search shape or typed detail shape) to
/// a candidate. Typed fields are keyed on presence, so the same mapper serves
/// every category.
fn neodb_item(value: &Value) -> Option<ExternalCandidate> {
    let uuid = value
        .get("uuid")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?;
    let title = value
        .get("title")
        .or_else(|| value.get("display_title"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    // `url` is the vault-relative site path (`/book/<uuid>`), which encodes the
    // item's exact kind (`tv/season/…` vs `tv/…`) — the category alone doesn't.
    let url = match value.get("url").and_then(Value::as_str) {
        Some(path) if path.starts_with('/') => format!("{BASE}{path}"),
        Some(absolute) if absolute.starts_with("http") => absolute.to_string(),
        _ => return None,
    };

    // Localized titles: NeoDB's lang codes may carry region/script subtags
    // (`zh-cn`, `zh-tw`); stored title keys are always the bare primary subtag,
    // first entry per language wins.
    let mut titles = BTreeMap::new();
    if let Some(localized) = value.get("localized_title").and_then(Value::as_array) {
        for entry in localized {
            let (Some(lang), Some(text)) = (
                entry.get("lang").and_then(Value::as_str),
                entry.get("text").and_then(Value::as_str),
            ) else {
                continue;
            };
            let lang = primary_language(lang);
            if lang.is_empty() || text.is_empty() {
                continue;
            }
            titles.entry(lang).or_insert_with(|| text.to_string());
        }
    }

    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let cover_url = value
        .get("cover_image_url")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let original_title = value
        .get("orig_title")
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
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(description) = &description {
        metadata.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }
    insert_str(&mut metadata, "category", value.get("category"));
    if let Some(rating) = value.get("rating").and_then(Value::as_f64) {
        if let Some(rating) = serde_json::Number::from_f64(rating) {
            metadata.insert("rating".to_string(), Value::Number(rating));
        }
    }
    if let Some(count) = value.get("rating_count").and_then(Value::as_i64) {
        if count > 0 {
            metadata.insert("rating_count".to_string(), Value::Number(count.into()));
        }
    }
    insert_str(&mut metadata, "release_date", value.get("release_date"));
    // Books carry a year/month pair instead of a date string.
    if let Some(year) = value.get("pub_year").and_then(Value::as_i64) {
        let published = match value.get("pub_month").and_then(Value::as_i64) {
            Some(month @ 1..=12) => format!("{year}-{month:02}"),
            _ => year.to_string(),
        };
        metadata.insert("published_date".to_string(), Value::String(published));
    }
    // Edition pages may be an integer or a numeric string.
    if let Some(pages) = value.get("pages").and_then(|pages| {
        pages
            .as_i64()
            .or_else(|| pages.as_str().and_then(|pages| pages.trim().parse().ok()))
    }) {
        if pages > 0 {
            metadata.insert("pages".to_string(), Value::Number(pages.into()));
        }
    }
    insert_str(&mut metadata, "isbn", value.get("isbn"));
    insert_str(&mut metadata, "series", value.get("series"));
    insert_str(&mut metadata, "format", value.get("format"));
    insert_str(&mut metadata, "official_site", value.get("official_site"));
    // Performances carry a run window instead of a single release date.
    insert_str(&mut metadata, "opening_date", value.get("opening_date"));
    insert_str(&mut metadata, "closing_date", value.get("closing_date"));
    for (key, source) in [
        ("episode_count", "episode_count"),
        ("season_count", "season_count"),
        ("season_number", "season_number"),
    ] {
        if let Some(count) = value.get(source).and_then(Value::as_i64) {
            if count > 0 {
                metadata.insert(key.to_string(), Value::Number(count.into()));
            }
        }
    }
    // The item's language list; surface the first entry.
    if let Some(language) = value
        .get("language")
        .and_then(Value::as_array)
        .and_then(|languages| languages.first())
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("language".to_string(), Value::String(language.to_string()));
    }
    for (key, source) in [
        ("tags", "tags"),
        ("authors", "author"),
        ("translators", "translator"),
        ("publishers", "publisher"),
        ("directors", "director"),
        ("playwrights", "playwright"),
        ("original_creators", "orig_creator"),
        ("composers", "composer"),
        ("choreographers", "choreographer"),
        ("performers", "performer"),
        ("genres", "genre"),
        ("artists", "artist"),
        ("developers", "developer"),
        ("platforms", "platform"),
        ("hosts", "host"),
        ("company", "company"),
    ] {
        if let Some(list) = string_list(value.get(source)) {
            metadata.insert(key.to_string(), list);
        }
    }
    // Cast/crew credits are plain names on movies and TV but `{name, role}`
    // objects on performances; both collapse to the name list.
    for (key, source) in [("actors", "actor"), ("crew", "crew")] {
        if let Some(list) = credit_names(value.get(source)) {
            metadata.insert(key.to_string(), list);
        }
    }

    Some(ExternalCandidate {
        needs_detail: false,
        provider: NeoDbProvider::ID.to_string(),
        source_id: uuid.to_string(),
        url,
        original_title: original_title.or_else(|| Some(title.clone())),
        title,
        brief: description,
        cover_url,
        titles,
        metadata,
    })
}

/// Names from a NeoDB credit list, whose entries are plain strings (movie/TV
/// `actor`) or `{name, role}` objects (performance `actor`/`crew`).
fn credit_names(value: Option<&Value>) -> Option<Value> {
    string_list_with(value, |entry| {
        entry
            .as_str()
            .or_else(|| entry.get("name").and_then(Value::as_str))
    })
}

/// Extracts the item path (`book/<uuid>`, `tv/season/<uuid>`, …) from a pasted
/// NeoDB URL. Host-anchored to the flagship instance.
fn neodb_item_path(q: &str) -> Option<String> {
    let trimmed = q.trim();
    let rest = trimmed
        .strip_prefix("https://neodb.social/")
        .or_else(|| trimmed.strip_prefix("http://neodb.social/"))?;
    let mut segments = rest.split(['?', '#']).next().unwrap_or_default().split('/');
    let kind = segments.next()?;
    let second = segments.next()?;
    let (path_kind, uuid) = match (kind, second) {
        ("tv", "season")
        | ("tv", "episode")
        | ("podcast", "episode")
        | ("performance", "production") => (format!("{kind}/{second}"), segments.next()?),
        ("book" | "movie" | "tv" | "album" | "game" | "podcast" | "performance", uuid) => {
            (kind.to_string(), uuid)
        }
        _ => return None,
    };
    let uuid = uuid.trim();
    let valid = !uuid.is_empty()
        && uuid
            .chars()
            .all(|character| character.is_ascii_alphanumeric());
    valid.then(|| format!("{path_kind}/{uuid}"))
}

#[cfg(test)]
mod tests {
    use super::{neodb_item, neodb_item_path};
    use serde_json::json;

    #[test]
    fn item_path_detection() {
        assert_eq!(
            neodb_item_path("https://neodb.social/book/2lNref8kib9XCBHPRdiJgo"),
            Some("book/2lNref8kib9XCBHPRdiJgo".to_string())
        );
        assert_eq!(
            neodb_item_path("https://neodb.social/tv/season/6D8oiJ2rBjfEnGdbaXwPeV?x=1"),
            Some("tv/season/6D8oiJ2rBjfEnGdbaXwPeV".to_string())
        );
        assert_eq!(neodb_item_path("https://neodb.social/discover"), None);
        assert_eq!(
            neodb_item_path("https://other.instance/book/2lNref8kib9XCBHPRdiJgo"),
            None
        );
        assert_eq!(neodb_item_path("some book title"), None);
    }

    #[test]
    fn item_maps_generic_search_shape() {
        let candidate = neodb_item(&json!({
            "uuid": "2lNref8kib9XCBHPRdiJgo",
            "url": "/book/2lNref8kib9XCBHPRdiJgo",
            "category": "book",
            "title": "三体",
            "localized_title": [
                { "lang": "zh-cn", "text": "三体" },
                { "lang": "zh-tw", "text": "三體" },
                { "lang": "en", "text": "The Three-Body Problem" }
            ],
            "description": "地球文明向宇宙发出的第一声啼鸣……",
            "cover_image_url": "https://neodb.social/m/book/cover.jpg",
            "rating": 8.9,
            "rating_count": 1234,
            "tags": ["科幻", "刘慈欣"]
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "2lNref8kib9XCBHPRdiJgo");
        assert_eq!(
            candidate.url,
            "https://neodb.social/book/2lNref8kib9XCBHPRdiJgo"
        );
        assert_eq!(candidate.title, "三体");
        // Script/region subtags never reach stored title keys; first entry per
        // bare language wins.
        assert_eq!(candidate.titles.get("zh"), Some(&"三体".to_string()));
        assert_eq!(
            candidate.titles.get("en"),
            Some(&"The Three-Body Problem".to_string())
        );
        assert!(!candidate.titles.contains_key("zh-cn"));
        assert_eq!(candidate.metadata.get("rating"), Some(&json!(8.9)));
        assert_eq!(candidate.metadata.get("rating_count"), Some(&json!(1234)));
        assert_eq!(
            candidate.metadata.get("tags"),
            Some(&json!(["科幻", "刘慈欣"]))
        );
    }

    #[test]
    fn item_maps_typed_book_detail() {
        let candidate = neodb_item(&json!({
            "uuid": "2lNref8kib9XCBHPRdiJgo",
            "url": "/book/2lNref8kib9XCBHPRdiJgo",
            "category": "book",
            "title": "The Three-Body Problem",
            "orig_title": "三体",
            "author": ["Liu Cixin"],
            "translator": ["Ken Liu"],
            "publisher": ["Tor Books"],
            "language": ["en"],
            "pub_year": 2014,
            "pub_month": 11,
            "pages": "416",
            "isbn": "9780765377067",
            "rating": 8.9
        }))
        .unwrap();

        assert_eq!(candidate.original_title, Some("三体".to_string()));
        assert_eq!(
            candidate.metadata.get("authors"),
            Some(&json!(["Liu Cixin"]))
        );
        assert_eq!(
            candidate.metadata.get("translators"),
            Some(&json!(["Ken Liu"]))
        );
        assert_eq!(
            candidate.metadata.get("published_date"),
            Some(&json!("2014-11"))
        );
        assert_eq!(candidate.metadata.get("pages"), Some(&json!(416)));
        assert_eq!(
            candidate.metadata.get("isbn"),
            Some(&json!("9780765377067"))
        );
        assert_eq!(candidate.metadata.get("language"), Some(&json!("en")));
    }

    // Field values mirror the real neodb.social payload for
    // /api/performance/3mlQvenprPworABqWVWz3o (阿波罗尼亚 / Mia Famiglia).
    #[test]
    fn item_maps_typed_performance_detail() {
        let candidate = neodb_item(&json!({
            "uuid": "3mlQvenprPworABqWVWz3o",
            "url": "/performance/3mlQvenprPworABqWVWz3o",
            "category": "performance",
            "title": "阿波罗尼亚",
            "orig_title": "Mia Famiglia",
            "localized_title": [
                { "lang": "zh-cn", "text": "阿波罗尼亚" },
                { "lang": "en", "text": "Mia Famiglia" }
            ],
            "genre": ["musical"],
            "language": [],
            "opening_date": "2020-08-28",
            "closing_date": null,
            "director": ["高瑞嘉"],
            "playwright": ["金琪 赵阳"],
            "orig_creator": [],
            "composer": [],
            "choreographer": [],
            "performer": [],
            "actor": [
                { "name": "李磊", "role": "" },
                { "name": "李苏霖", "role": "" }
            ],
            "crew": [],
            "official_site": null
        }))
        .unwrap();

        assert_eq!(candidate.original_title, Some("Mia Famiglia".to_string()));
        assert_eq!(candidate.titles.get("zh"), Some(&"阿波罗尼亚".to_string()));
        assert_eq!(
            candidate.metadata.get("opening_date"),
            Some(&json!("2020-08-28"))
        );
        assert_eq!(
            candidate.metadata.get("directors"),
            Some(&json!(["高瑞嘉"]))
        );
        assert_eq!(
            candidate.metadata.get("playwrights"),
            Some(&json!(["金琪 赵阳"]))
        );
        // `{name, role}` credit objects collapse to the name list.
        assert_eq!(
            candidate.metadata.get("actors"),
            Some(&json!(["李磊", "李苏霖"]))
        );
        assert_eq!(candidate.metadata.get("genres"), Some(&json!(["musical"])));
        // Null and empty-list fields are omitted, never stored as null/[].
        for absent in ["closing_date", "composers", "crew", "language"] {
            assert!(!candidate.metadata.contains_key(absent), "{absent}");
        }
    }

    #[test]
    fn credit_lists_accept_plain_names() {
        // Movie/TV payloads carry `actor` as plain strings.
        let candidate = neodb_item(&json!({
            "uuid": "0TzoWm9YrObiCm4Pj5UJTA",
            "url": "/movie/0TzoWm9YrObiCm4Pj5UJTA",
            "category": "movie",
            "title": "银河护卫队",
            "actor": ["Chris Pratt", "Zoe Saldana"]
        }))
        .unwrap();
        assert_eq!(
            candidate.metadata.get("actors"),
            Some(&json!(["Chris Pratt", "Zoe Saldana"]))
        );
    }

    #[test]
    fn item_without_uuid_or_title_is_skipped() {
        assert!(neodb_item(&json!({ "uuid": "x", "url": "/book/x" })).is_none());
        assert!(neodb_item(&json!({ "title": "t", "url": "/book/x" })).is_none());
    }
}
