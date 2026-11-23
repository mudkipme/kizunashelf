//! Eventernote (https://www.eventernote.com) — Japanese live-event, voice-actor,
//! artist, and venue catalog. Fully keyless: free-text search uses the JSON
//! `/api/{events,actors,places}/search` endpoints (with lazy `meta#crumb`
//! fallback if Eventernote requires it), and direct URL resolution parses the
//! canonical `/events/{id}`, `/actors/{id}`, and `/places/{id}` HTML pages.

use super::{
    external_client, field_option, insert_str, non_empty_string_or_integer, provider_error,
    send_limited, string_array, type_option, url_type_allowed, ExternalProvider,
    ProviderResponseExt, ProviderSearchConfig, USER_AGENT,
};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;
use tokio::sync::Mutex;

pub(super) struct EventernoteProvider;

const BASE: &str = "https://www.eventernote.com";

impl ExternalProvider for EventernoteProvider {
    const ID: &'static str = "eventernote";
    const LABEL: &'static str = "Eventernote";

    fn recognizes_url(q: &str) -> bool {
        eventernote_ref(q).is_some()
    }

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        !eventernote_entities(provider_config).is_empty()
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
        search_eventernote(q, page, page_size, provider_config).await
    }
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("title", "Title / Name"),
        field_option("cover_url", "Cover URL"),
        field_option("date", "Event date"),
        field_option("open_time", "Open time"),
        field_option("start_time", "Start time"),
        field_option("end_time", "End time"),
        field_option("place", "Venue / Place"),
        field_option("address", "Address"),
        field_option("postal_code", "Postal code"),
        field_option("tel", "Phone"),
        field_option("capacity", "Capacity"),
        field_option("official_site", "Official site"),
        field_option("seat_url", "Seating info URL"),
        field_option("hashtag", "Hashtag"),
        field_option("participants_count", "Participants count"),
        field_option("kana", "Kana"),
        field_option("favorite_count", "Favorites count"),
        field_option("description", "Description"),
        field_option("actors", "Performers / Actors"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![
        type_option("event", "Event"),
        type_option("actor", "Artist / Voice actor"),
        type_option("place", "Venue / Place"),
    ]
}

fn eventernote_entities(provider_config: &ProviderSearchConfig) -> Vec<&'static str> {
    let Some(types) = provider_config.external_types() else {
        return vec!["event"];
    };
    let mut entities = Vec::new();
    for entity in ["event", "actor", "place"] {
        if types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case(entity))
            && !entities.contains(&entity)
        {
            entities.push(entity);
        }
    }
    entities
}

async fn search_eventernote(
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let entities = eventernote_entities(provider_config);
    if entities.is_empty() {
        return Ok(Vec::new());
    }
    let client = external_client();
    if let Some((entity, id)) = eventernote_ref(q) {
        if !url_type_allowed(provider_config, entity) {
            return Ok(Vec::new());
        }
        return resolve_eventernote(client, entity, &id).await;
    }
    let offset = page
        .saturating_sub(1)
        .saturating_mul(page_size)
        .saturating_add(1);
    let mut items = Vec::new();
    for entity in entities {
        let path = match entity {
            "actor" => "/api/actors/search",
            "place" => "/api/places/search",
            _ => "/api/events/search",
        };
        let results = eventernote_api_search(client, path, q, offset, page_size).await?;
        items.extend(results.iter().filter_map(|item| match entity {
            "actor" => eventernote_actor_api(item),
            "place" => eventernote_place_api(item),
            _ => eventernote_event_api(item),
        }));
    }
    Ok(items)
}

fn crumb_slot() -> &'static Mutex<Option<String>> {
    static CRUMB: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    CRUMB.get_or_init(|| Mutex::new(None))
}

async fn refresh_crumb(client: &reqwest::Client) -> Result<Option<String>, ApiError> {
    let html = send_limited(
        client
            .get(format!("{BASE}/"))
            .header(reqwest::header::USER_AGENT, USER_AGENT),
    )
    .await
    .map_err(provider_error)?
    .error_for_status_body()
    .await?
    .text()
    .await
    .map_err(provider_error)?;
    let crumb = parse_crumb(&html);
    *crumb_slot().lock().await = crumb.clone();
    Ok(crumb)
}

async fn eventernote_api_search(
    client: &reqwest::Client,
    path: &str,
    q: &str,
    offset: usize,
    limit: usize,
) -> Result<Vec<Value>, ApiError> {
    let build = |crumb: Option<&str>| {
        let mut req = client
            .get(format!("{BASE}{path}"))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .query(&[
                ("keyword", q),
                ("offset", &offset.to_string()),
                ("limit", &limit.to_string()),
            ]);
        if let Some(crumb) = crumb.filter(|value| !value.is_empty()) {
            req = req.query(&[("crumb", crumb)]);
        }
        req
    };

    let cached = crumb_slot().lock().await.clone();
    let response = send_limited(build(cached.as_deref()))
        .await
        .map_err(provider_error)?;
    let status = response.status();
    if status.is_success() {
        let value = response.json::<Value>().await.map_err(provider_error)?;
        if let Some(results) = unwrap_api_results(&value) {
            return Ok(results);
        }
        if let Some(fresh) = refresh_crumb(client).await? {
            if cached.as_deref() != Some(fresh.as_str()) {
                let retry = send_limited(build(Some(&fresh)))
                    .await
                    .map_err(provider_error)?
                    .error_for_status_body()
                    .await?
                    .json::<Value>()
                    .await
                    .map_err(provider_error)?;
                if let Some(results) = unwrap_api_results(&retry) {
                    return Ok(results);
                }
            }
        }
        return Err(ApiError::bad_gateway(
            "Eventernote returned an unexpected API response",
        ));
    }

    if matches!(
        status,
        reqwest::StatusCode::BAD_REQUEST | reqwest::StatusCode::FORBIDDEN
    ) {
        if let Some(fresh) = refresh_crumb(client).await? {
            if cached.as_deref() != Some(fresh.as_str()) {
                let retry = send_limited(build(Some(&fresh)))
                    .await
                    .map_err(provider_error)?
                    .error_for_status_body()
                    .await?
                    .json::<Value>()
                    .await
                    .map_err(provider_error)?;
                return unwrap_api_results(&retry).ok_or_else(|| {
                    ApiError::bad_gateway("Eventernote returned an unexpected API response")
                });
            }
        }
    }

    let _ = response.error_for_status_body().await?;
    Ok(Vec::new())
}

fn unwrap_api_results(value: &Value) -> Option<Vec<Value>> {
    if value.get("code").and_then(Value::as_i64) != Some(200) {
        return None;
    }
    value.get("results").and_then(Value::as_array).cloned()
}

async fn resolve_eventernote(
    client: &reqwest::Client,
    entity: &str,
    id: &str,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let segment = match entity {
        "actor" => "actors",
        "place" => "places",
        _ => "events",
    };
    let html = send_limited(
        client
            .get(format!("{BASE}/{segment}/{id}"))
            .header(reqwest::header::USER_AGENT, USER_AGENT),
    )
    .await
    .map_err(provider_error)?
    .error_for_status_body()
    .await?
    .text()
    .await
    .map_err(provider_error)?;
    let candidate = match entity {
        "actor" => parse_actor_detail_html(&html, id),
        "place" => parse_place_detail_html(&html, id),
        _ => parse_event_detail_html(&html, id),
    };
    Ok(candidate.into_iter().collect())
}

fn eventernote_event_api(item: &Value) -> Option<ExternalCandidate> {
    let id = item.get("id").and_then(non_empty_string_or_integer)?;
    let title = item
        .get("event_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = format!("{BASE}/events/{id}");

    let has_image = item.get("has_image").and_then(Value::as_i64);
    let cover_url = if has_image == Some(0) {
        None
    } else {
        item.get("image_url")
            .or_else(|| item.get("thumb_url"))
            .and_then(Value::as_str)
            .and_then(usable_image_url)
    };

    let date = item
        .get("event_date")
        .and_then(Value::as_str)
        .and_then(extract_iso_date);
    let open_time = trimmed_str(item.get("open_time"));
    let start_time = trimmed_str(item.get("start_time"));
    let end_time = trimmed_str(item.get("end_time"));
    let place = item
        .get("place")
        .and_then(|place| place.get("place_name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let address = item
        .get("place")
        .and_then(|place| place.get("address"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let actors: Vec<String> = item
        .get("actors")
        .and_then(Value::as_array)
        .map(|actors| {
            actors
                .iter()
                .filter_map(|actor| actor.get("name").and_then(Value::as_str))
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let description = item
        .get("description")
        .and_then(Value::as_str)
        .map(normalize_multiline)
        .filter(|value| !value.is_empty());
    let hashtag = item
        .get("hashtag")
        .and_then(Value::as_str)
        .and_then(normalize_hashtag);
    let participants_count = item.get("note_count").and_then(Value::as_i64);

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(date) = &date {
        metadata.insert("date".to_string(), Value::String(date.clone()));
    }
    if let Some(open_time) = &open_time {
        metadata.insert("open_time".to_string(), Value::String(open_time.clone()));
    }
    if let Some(start_time) = &start_time {
        metadata.insert("start_time".to_string(), Value::String(start_time.clone()));
    }
    if let Some(end_time) = &end_time {
        metadata.insert("end_time".to_string(), Value::String(end_time.clone()));
    }
    if let Some(place) = &place {
        metadata.insert("place".to_string(), Value::String(place.clone()));
    }
    if let Some(address) = &address {
        metadata.insert("address".to_string(), Value::String(address.clone()));
    }
    if let Some(actors) = string_array(actors) {
        metadata.insert("actors".to_string(), actors);
    }
    insert_str(&mut metadata, "official_site", item.get("link"));
    if let Some(hashtag) = &hashtag {
        metadata.insert("hashtag".to_string(), Value::String(hashtag.clone()));
    }
    if let Some(count) = participants_count {
        if count >= 0 {
            metadata.insert(
                "participants_count".to_string(),
                Value::Number(count.into()),
            );
        }
    }
    if let Some(description) = &description {
        metadata.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }

    let brief = event_brief(
        date.as_deref(),
        start_time.as_deref(),
        place.as_deref(),
        description.as_deref(),
    );
    let mut titles = BTreeMap::new();
    titles.insert("ja".to_string(), title.clone());

    Some(ExternalCandidate {
        needs_detail: false,
        provider: EventernoteProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(title.clone()),
        title,
        brief,
        cover_url,
        titles,
        metadata,
    })
}

fn eventernote_actor_api(item: &Value) -> Option<ExternalCandidate> {
    let id = item.get("id").and_then(non_empty_string_or_integer)?;
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = format!("{BASE}/actors/{id}");
    let kana = trimmed_str(item.get("kana"));
    let cover_url = item
        .get("image")
        .and_then(Value::as_str)
        .and_then(usable_image_url);
    let favorite_count = item.get("favorite_count").and_then(Value::as_i64);

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(name.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(kana) = &kana {
        metadata.insert("kana".to_string(), Value::String(kana.clone()));
    }
    if let Some(count) = favorite_count {
        if count >= 0 {
            metadata.insert("favorite_count".to_string(), Value::Number(count.into()));
        }
    }

    let mut titles = BTreeMap::new();
    titles.insert("ja".to_string(), name.clone());

    Some(ExternalCandidate {
        needs_detail: false,
        provider: EventernoteProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(name.clone()),
        title: name,
        brief: kana,
        cover_url,
        titles,
        metadata,
    })
}

fn eventernote_place_api(item: &Value) -> Option<ExternalCandidate> {
    let id = item.get("id").and_then(non_empty_string_or_integer)?;
    let name = item
        .get("place_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = format!("{BASE}/places/{id}");
    let address = trimmed_str(item.get("address"));
    let description = item
        .get("tips")
        .and_then(Value::as_str)
        .map(normalize_multiline)
        .filter(|value| !value.is_empty());

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(name.clone()));
    metadata.insert("place".to_string(), Value::String(name.clone()));
    if let Some(address) = &address {
        metadata.insert("address".to_string(), Value::String(address.clone()));
    }
    insert_str(&mut metadata, "postal_code", item.get("postalcode"));
    insert_str(&mut metadata, "tel", item.get("tel"));
    insert_str(&mut metadata, "capacity", item.get("capacity"));
    insert_str(&mut metadata, "official_site", item.get("web_url"));
    insert_str(&mut metadata, "seat_url", item.get("seat_url"));
    if let Some(description) = &description {
        metadata.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }

    let mut titles = BTreeMap::new();
    titles.insert("ja".to_string(), name.clone());

    Some(ExternalCandidate {
        needs_detail: false,
        provider: EventernoteProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(name.clone()),
        title: name,
        brief: address.or(description),
        cover_url: None,
        titles,
        metadata,
    })
}

fn parse_event_detail_html(html: &str, id: &str) -> Option<ExternalCandidate> {
    let title = extract_element_by_class(html, "div", "gb_events_detail_title")
        .and_then(|block| first_tag_inner(block, "h2"))
        .map(html_to_text_collapsed)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            first_tag_inner(html, "h1")
                .map(html_to_text_collapsed)
                .filter(|value| !value.is_empty())
        })?;

    let mut date: Option<String> = None;
    let mut open_time: Option<String> = None;
    let mut start_time: Option<String> = None;
    let mut end_time: Option<String> = None;
    let mut place: Option<String> = None;
    let mut actors: Vec<String> = Vec::new();
    let mut official_site: Option<String> = None;
    let mut hashtag: Option<String> = None;
    let mut table_image: Option<String> = None;
    let mut description: Option<String> = None;

    if let Some(table_block) = extract_element_by_class(html, "div", "gb_events_info_table") {
        for (cell0, cell1) in extract_table_rows(table_block) {
            let label = html_to_text_collapsed(&cell0);
            match label.as_str() {
                "開催日時" => {
                    date = extract_iso_date(&html_to_text_collapsed(&cell1));
                }
                "時間" => {
                    let time_text = html_to_text_collapsed(&cell1);
                    open_time = extract_time(&time_text, "開場");
                    start_time = extract_time(&time_text, "開演");
                    end_time = extract_time(&time_text, "終演");
                }
                "開催場所" => {
                    place = extract_links(&cell1)
                        .into_iter()
                        .find(|(href, _)| href.starts_with("/places/"))
                        .map(|(_, text)| text)
                        .filter(|text| !text.is_empty())
                        .or_else(|| {
                            let text = html_to_text_collapsed(&cell1);
                            (!text.is_empty()).then_some(text)
                        });
                }
                "出演者" => {
                    actors = extract_links(&cell1)
                        .into_iter()
                        .filter(|(href, _)| href.starts_with("/actors/"))
                        .map(|(_, text)| text)
                        .filter(|text| !text.is_empty())
                        .collect();
                }
                "関連リンク" => {
                    official_site = extract_links(&cell1)
                        .into_iter()
                        .find_map(|(href, _)| resolve_url(&href));
                }
                "Twitterハッシュタグ" => {
                    let raw = extract_links(&cell1)
                        .into_iter()
                        .map(|(_, text)| text)
                        .find(|text| !text.is_empty())
                        .unwrap_or_else(|| html_to_text_collapsed(&cell1));
                    hashtag = normalize_hashtag(&raw);
                }
                "" => {
                    if let Some(src) = extract_img_src(&cell0) {
                        if table_image.is_none() {
                            table_image = usable_image_url(&src);
                        }
                        let desc = html_to_text_multiline(&cell1);
                        if !desc.is_empty() && description.is_none() {
                            description = Some(desc);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let cover_url = table_image.or_else(|| {
        find_meta_content(html, "property", "og:image")
            .as_deref()
            .and_then(usable_image_url)
    });
    let participants_count = parse_participants_count(html);

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(date) = &date {
        metadata.insert("date".to_string(), Value::String(date.clone()));
    }
    if let Some(open_time) = &open_time {
        metadata.insert("open_time".to_string(), Value::String(open_time.clone()));
    }
    if let Some(start_time) = &start_time {
        metadata.insert("start_time".to_string(), Value::String(start_time.clone()));
    }
    if let Some(end_time) = &end_time {
        metadata.insert("end_time".to_string(), Value::String(end_time.clone()));
    }
    if let Some(place) = &place {
        metadata.insert("place".to_string(), Value::String(place.clone()));
    }
    if let Some(actors) = string_array(actors) {
        metadata.insert("actors".to_string(), actors);
    }
    if let Some(official_site) = &official_site {
        metadata.insert(
            "official_site".to_string(),
            Value::String(official_site.clone()),
        );
    }
    if let Some(hashtag) = &hashtag {
        metadata.insert("hashtag".to_string(), Value::String(hashtag.clone()));
    }
    if let Some(count) = participants_count {
        metadata.insert(
            "participants_count".to_string(),
            Value::Number(count.into()),
        );
    }
    if let Some(description) = &description {
        metadata.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }

    let brief = event_brief(
        date.as_deref(),
        start_time.as_deref(),
        place.as_deref(),
        description.as_deref(),
    );
    let mut titles = BTreeMap::new();
    titles.insert("ja".to_string(), title.clone());

    Some(ExternalCandidate {
        needs_detail: false,
        provider: EventernoteProvider::ID.to_string(),
        source_id: id.to_string(),
        url: format!("{BASE}/events/{id}"),
        original_title: Some(title.clone()),
        title,
        brief,
        cover_url,
        titles,
        metadata,
    })
}

fn parse_actor_detail_html(html: &str, id: &str) -> Option<ExternalCandidate> {
    let title_block = extract_element_by_class(html, "div", "gb_actors_title")?;
    let mut name: Option<String> = None;
    let mut kana: Option<String> = None;
    for (open_tag, inner) in all_tags_with_attrs(title_block, "h2") {
        let text = html_to_text_collapsed(inner);
        if text.is_empty() {
            continue;
        }
        if tag_has_class(open_tag, "kana") {
            if kana.is_none() {
                kana = Some(text);
            }
        } else if name.is_none() {
            name = Some(text);
        }
    }
    let name = name?;
    let favorite_count = parse_fan_count(html);

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(name.clone()));
    if let Some(kana) = &kana {
        metadata.insert("kana".to_string(), Value::String(kana.clone()));
    }
    if let Some(count) = favorite_count {
        metadata.insert("favorite_count".to_string(), Value::Number(count.into()));
    }

    let mut titles = BTreeMap::new();
    titles.insert("ja".to_string(), name.clone());

    Some(ExternalCandidate {
        needs_detail: false,
        provider: EventernoteProvider::ID.to_string(),
        source_id: id.to_string(),
        url: format!("{BASE}/actors/{id}"),
        original_title: Some(name.clone()),
        title: name,
        brief: kana,
        cover_url: None,
        titles,
        metadata,
    })
}

fn parse_place_detail_html(html: &str, id: &str) -> Option<ExternalCandidate> {
    let name = extract_element_by_class(html, "div", "gb_place_detail_title")
        .and_then(|block| first_tag_inner(block, "h2"))
        .map(html_to_text_collapsed)
        .filter(|value| !value.is_empty())?;

    let mut address: Option<String> = None;
    let mut postal_code: Option<String> = None;
    let mut tel: Option<String> = None;
    let mut capacity: Option<String> = None;
    let mut official_site: Option<String> = None;
    let mut seat_url: Option<String> = None;

    if let Some(table_block) = extract_element_by_class(html, "div", "gb_place_detail_table") {
        for (cell0, cell1) in extract_table_rows(table_block) {
            let label = html_to_text_collapsed(&cell0);
            match label.as_str() {
                "所在地" => {
                    let raw = html_to_text_collapsed(&cell1);
                    if let Some(code) = extract_postal_code(&raw) {
                        let stripped = raw.replace(&code, "").trim().to_string();
                        postal_code = Some(code);
                        if !stripped.is_empty() {
                            address = Some(stripped);
                        }
                    } else if !raw.is_empty() {
                        address = Some(raw);
                    }
                }
                "電話番号" => {
                    let value = html_to_text_collapsed(&cell1);
                    if !value.is_empty() {
                        tel = Some(value);
                    }
                }
                "収容人数" => {
                    let value = html_to_text_collapsed(&cell1);
                    if !value.is_empty() {
                        capacity = Some(value);
                    }
                }
                "公式サイト" => {
                    official_site = extract_links(&cell1)
                        .into_iter()
                        .find_map(|(href, _)| resolve_url(&href));
                }
                "座席情報" => {
                    seat_url = extract_links(&cell1)
                        .into_iter()
                        .find_map(|(href, _)| resolve_url(&href));
                }
                _ => {}
            }
        }
    }

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(name.clone()));
    metadata.insert("place".to_string(), Value::String(name.clone()));
    if let Some(address) = &address {
        metadata.insert("address".to_string(), Value::String(address.clone()));
    }
    if let Some(postal_code) = &postal_code {
        metadata.insert(
            "postal_code".to_string(),
            Value::String(postal_code.clone()),
        );
    }
    if let Some(tel) = &tel {
        metadata.insert("tel".to_string(), Value::String(tel.clone()));
    }
    if let Some(capacity) = &capacity {
        metadata.insert("capacity".to_string(), Value::String(capacity.clone()));
    }
    if let Some(official_site) = &official_site {
        metadata.insert(
            "official_site".to_string(),
            Value::String(official_site.clone()),
        );
    }
    if let Some(seat_url) = &seat_url {
        metadata.insert("seat_url".to_string(), Value::String(seat_url.clone()));
    }

    let mut titles = BTreeMap::new();
    titles.insert("ja".to_string(), name.clone());

    Some(ExternalCandidate {
        needs_detail: false,
        provider: EventernoteProvider::ID.to_string(),
        source_id: id.to_string(),
        url: format!("{BASE}/places/{id}"),
        original_title: Some(name.clone()),
        title: name,
        brief: address,
        cover_url: None,
        titles,
        metadata,
    })
}

/// Extracts `(entity, id)` from a host-anchored Eventernote URL:
/// `/events/<id>`, `/actors/<id>`, `/actors/<slug>/<id>`, or `/places/<id>`.
fn eventernote_ref(q: &str) -> Option<(&'static str, String)> {
    let rest = strip_eventernote_origin(q.trim())?;
    let path = rest.split(['?', '#']).next().unwrap_or_default();
    let segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    match segments.as_slice() {
        ["events", id] if is_numeric_id(id) => Some(("event", (*id).to_string())),
        ["actors", id] if is_numeric_id(id) => Some(("actor", (*id).to_string())),
        ["actors", _, id] if is_numeric_id(id) => Some(("actor", (*id).to_string())),
        ["places", id] if is_numeric_id(id) => Some(("place", (*id).to_string())),
        _ => None,
    }
}

fn strip_eventernote_origin(input: &str) -> Option<&str> {
    let after_scheme = if input.len() >= 8
        && input.is_char_boundary(8)
        && input[..8].eq_ignore_ascii_case("https://")
    {
        &input[8..]
    } else if input.len() >= 7
        && input.is_char_boundary(7)
        && input[..7].eq_ignore_ascii_case("http://")
    {
        &input[7..]
    } else {
        return None;
    };
    let (host, rest) = after_scheme.split_once('/')?;
    (host.eq_ignore_ascii_case("www.eventernote.com")
        || host.eq_ignore_ascii_case("eventernote.com"))
    .then_some(rest)
}

fn is_numeric_id(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
}

fn event_brief(
    date: Option<&str>,
    start_time: Option<&str>,
    place: Option<&str>,
    description: Option<&str>,
) -> Option<String> {
    let summary = [date, start_time, place]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    if !summary.is_empty() {
        return Some(summary);
    }
    description
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn trimmed_str(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalize_hashtag(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('#') {
        Some(trimmed.to_string())
    } else {
        Some(format!("#{trimmed}"))
    }
}

fn normalize_multiline(raw: &str) -> String {
    raw.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn resolve_url(href: &str) -> Option<String> {
    let href = href.trim();
    if href.starts_with("https://") || href.starts_with("http://") {
        Some(href.to_string())
    } else if href.starts_with("//") {
        Some(format!("https:{href}"))
    } else if href.starts_with('/') {
        Some(format!("{BASE}{href}"))
    } else {
        None
    }
}

fn usable_image_url(raw: &str) -> Option<String> {
    let url = resolve_url(raw)?;
    let lower = url.to_ascii_lowercase();
    if lower.contains("no_image")
        || lower.contains("no_profile_image")
        || lower.contains("/images/logo")
    {
        return None;
    }
    Some(url)
}

fn extract_iso_date(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\b(\d{4}-\d{2}-\d{2})\b").expect("date regex"));
    re.captures(text).map(|cap| cap[1].to_string())
}

fn extract_time(text: &str, label: &str) -> Option<String> {
    let rest = text.split_once(label)?.1.trim_start();
    let time: String = rest
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == ':')
        .collect();
    time.contains(':').then_some(time)
}

fn extract_postal_code(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"〒\d{3}-\d{4}").expect("postal code regex"));
    re.find(text).map(|m| m.as_str().to_string())
}

fn parse_participants_count(html: &str) -> Option<i64> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"このイベントに参加のイベンター\s*\(([0-9,]+)人\)").expect("participants regex")
    });
    re.captures(html)?[1].replace(',', "").parse::<i64>().ok()
}

fn parse_fan_count(html: &str) -> Option<i64> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"ファン一覧\s*\((?:<[^>]*>|\s)*([0-9,]+)").expect("fan count regex")
    });
    re.captures(html)?[1].replace(',', "").parse::<i64>().ok()
}

fn parse_crumb(html: &str) -> Option<String> {
    find_meta_content(html, "id", "crumb").or_else(|| find_meta_content(html, "name", "crumb"))
}

fn find_meta_content(html: &str, attr_name: &str, attr_value: &str) -> Option<String> {
    static META_RE: OnceLock<Regex> = OnceLock::new();
    let re = META_RE.get_or_init(|| Regex::new(r"(?is)<meta\b[^>]*>").expect("meta regex"));
    for matched in re.find_iter(html) {
        let tag = matched.as_str();
        if html_attr(tag, attr_name).is_some_and(|value| value.eq_ignore_ascii_case(attr_value)) {
            if let Some(content) = html_attr(tag, "content") {
                let decoded = decode_html_entities(&content).trim().to_string();
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            }
        }
    }
    None
}

fn html_attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut offset = 0;
    while let Some(pos) = lower[offset..].find(&needle) {
        let start = offset + pos;
        let before_ok = start > 0 && bytes[start - 1].is_ascii_whitespace();
        let after_name = start + needle.len();
        if before_ok {
            let mut cursor = after_name;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor < bytes.len() && bytes[cursor] == b'=' {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                if cursor < bytes.len() {
                    let quote = bytes[cursor];
                    if quote == b'"' || quote == b'\'' {
                        let val_start = cursor + 1;
                        if let Some(rel_end) = tag[val_start..].find(quote as char) {
                            return Some(tag[val_start..val_start + rel_end].to_string());
                        }
                    } else {
                        let val_end = tag[cursor..]
                            .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
                            .map(|rel| cursor + rel)
                            .unwrap_or(tag.len());
                        return Some(tag[cursor..val_end].to_string());
                    }
                }
            }
        }
        offset = after_name;
    }
    None
}

fn tag_has_class(open_tag: &str, class_name: &str) -> bool {
    html_attr(open_tag, "class").is_some_and(|classes| {
        classes
            .split_whitespace()
            .any(|token| token.eq_ignore_ascii_case(class_name))
    })
}

/// Extracts the inner HTML of the first `<{tag} ... class="...{class_name}...">`
/// element, tracking nested same-name tags so inner `<div>`s do not truncate it.
fn extract_element_by_class<'a>(html: &'a str, tag: &str, class_name: &str) -> Option<&'a str> {
    let lower = html.to_ascii_lowercase();
    let open_prefix = format!("<{}", tag.to_ascii_lowercase());
    let close_prefix = format!("</{}", tag.to_ascii_lowercase());
    let mut search = 0;

    while let Some(rel) = lower[search..].find(&open_prefix) {
        let tag_start = search + rel;
        let after_name = tag_start + open_prefix.len();
        let next_byte = lower.as_bytes().get(after_name).copied();
        let boundary_ok =
            next_byte.is_some_and(|b| b.is_ascii_whitespace() || b == b'>' || b == b'/');
        let tag_end_rel = lower[tag_start..].find('>')?;
        let open_end = tag_start + tag_end_rel + 1;
        if boundary_ok && tag_has_class(&html[tag_start..open_end], class_name) {
            let mut depth = 1usize;
            let mut cursor = open_end;
            while cursor < lower.len() {
                let next_open = find_tag_token(&lower, cursor, &open_prefix);
                let next_close = find_tag_token(&lower, cursor, &close_prefix);
                match (next_open, next_close) {
                    (Some(o), Some(c)) if o < c => {
                        let Some(o_end_rel) = lower[o..].find('>') else {
                            break;
                        };
                        depth += 1;
                        cursor = o + o_end_rel + 1;
                    }
                    (_, Some(c)) => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(&html[open_end..c]);
                        }
                        let Some(c_end_rel) = lower[c..].find('>') else {
                            break;
                        };
                        cursor = c + c_end_rel + 1;
                    }
                    _ => break,
                }
            }
            return Some(&html[open_end..]);
        }
        search = open_end;
    }
    None
}

fn find_tag_token(lower: &str, from: usize, prefix: &str) -> Option<usize> {
    let mut cursor = from;
    while let Some(rel) = lower[cursor..].find(prefix) {
        let pos = cursor + rel;
        let after = pos + prefix.len();
        if lower
            .as_bytes()
            .get(after)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/')
        {
            return Some(pos);
        }
        cursor = after;
    }
    None
}

fn first_tag_inner<'a>(html: &'a str, tag: &str) -> Option<&'a str> {
    all_tags_with_attrs(html, tag)
        .into_iter()
        .next()
        .map(|(_, inner)| inner)
}

fn all_tags_with_attrs<'a>(html: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let lower = html.to_ascii_lowercase();
    let open_prefix = format!("<{}", tag.to_ascii_lowercase());
    let close_prefix = format!("</{}", tag.to_ascii_lowercase());
    let mut results = Vec::new();
    let mut cursor = 0;

    while let Some(start) = find_tag_token(&lower, cursor, &open_prefix) {
        let Some(open_end_rel) = lower[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_rel + 1;
        let Some(close_start) = find_tag_token(&lower, open_end, &close_prefix) else {
            break;
        };
        let close_end = lower[close_start..]
            .find('>')
            .map(|rel| close_start + rel + 1)
            .unwrap_or(lower.len());
        results.push((&html[start..open_end], &html[open_end..close_start]));
        cursor = close_end;
    }
    results
}

fn extract_table_rows(table_html: &str) -> Vec<(String, String)> {
    all_tags_with_attrs(table_html, "tr")
        .into_iter()
        .filter_map(|(_, row_inner)| {
            let mut cells = all_tags_with_attrs(row_inner, "td").into_iter();
            let (_, first) = cells.next()?;
            let (_, second) = cells.next()?;
            Some((first.to_string(), second.to_string()))
        })
        .collect()
}

fn extract_links(fragment: &str) -> Vec<(String, String)> {
    all_tags_with_attrs(fragment, "a")
        .into_iter()
        .filter_map(|(open_tag, inner)| {
            let href = decode_html_entities(&html_attr(open_tag, "href")?);
            let text = html_to_text_collapsed(inner);
            Some((href.trim().to_string(), text))
        })
        .collect()
}

fn extract_img_src(fragment: &str) -> Option<String> {
    static IMG_RE: OnceLock<Regex> = OnceLock::new();
    let re = IMG_RE.get_or_init(|| Regex::new(r"(?is)<img\b[^>]*>").expect("img regex"));
    for matched in re.find_iter(fragment) {
        if let Some(src) = html_attr(matched.as_str(), "src") {
            let decoded = decode_html_entities(&src).trim().to_string();
            if !decoded.is_empty() {
                return Some(decoded);
            }
        }
    }
    None
}

fn html_to_text_collapsed(fragment: &str) -> String {
    let stripped = strip_html_tags(fragment);
    decode_html_entities(&stripped)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn html_to_text_multiline(fragment: &str) -> String {
    static BR_RE: OnceLock<Regex> = OnceLock::new();
    let br_re = BR_RE.get_or_init(|| Regex::new(r"(?i)<br\s*/?>|</p>").expect("br regex"));
    let with_newlines = br_re.replace_all(fragment, "\n");
    let stripped = strip_html_tags(&with_newlines);
    normalize_multiline(&decode_html_entities(&stripped))
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for character in input.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(character),
            _ => {}
        }
    }
    out
}

fn decode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let Some(semi) = after.find(';').filter(|&idx| idx <= 12) else {
            out.push('&');
            rest = after;
            continue;
        };
        let entity = &after[..semi];
        if let Some(decoded) = decode_entity(entity) {
            out.push(decoded);
            rest = &after[semi + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok().and_then(char::from_u32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn url_detection_handles_events_actors_and_places() {
        assert_eq!(
            eventernote_ref("https://www.eventernote.com/events/464506"),
            Some(("event", "464506".to_string()))
        );
        assert_eq!(
            eventernote_ref("http://eventernote.com/events/464506/?tab=1#top"),
            Some(("event", "464506".to_string()))
        );
        assert_eq!(
            eventernote_ref("https://www.eventernote.com/actors/28"),
            Some(("actor", "28".to_string()))
        );
        assert_eq!(
            eventernote_ref(
                "https://www.eventernote.com/actors/TRUE%28%E5%94%90%E6%B2%A2%E7%BE%8E%E5%B8%86%29/9094"
            ),
            Some(("actor", "9094".to_string()))
        );
        assert_eq!(
            eventernote_ref("https://www.eventernote.com/places/138"),
            Some(("place", "138".to_string()))
        );
        assert_eq!(
            eventernote_ref("https://www.eventernote.com/places/prefecture/17"),
            None
        );
        assert_eq!(
            eventernote_ref("https://www.eventernote.com/events/search?keyword=TRUE"),
            None
        );
        assert_eq!(
            eventernote_ref("https://www.eventernote.com/actors/28/events"),
            None
        );
        assert_eq!(eventernote_ref("https://example.com/events/464506"), None);
        assert_eq!(eventernote_ref("464506"), None);
    }

    #[test]
    fn api_event_maps_fields_and_normalizes_values() {
        let candidate = eventernote_event_api(&json!({
            "id": 495039,
            "event_name": "『響け！ユーフォニアム』定期演奏会",
            "event_date": "2027-04-11",
            "open_time": "13:00",
            "start_time": "14:00",
            "end_time": "16:00",
            "link": "https://anime-eupho.com/concert10th/",
            "description": "第10回定期演奏会\r\n名古屋公演",
            "hashtag": "ユーフォ定演 #anime_eupho",
            "note_count": 52,
            "has_image": 1,
            "image_url": "https://eventernote.s3.amazonaws.com/images/events/495039.jpg",
            "thumb_url": "https://eventernote.s3.amazonaws.com/images/events/495039_s.jpg",
            "place": {
                "id": 71,
                "place_name": "名古屋国際会議場 センチュリーホール",
                "address": "愛知県名古屋市熱田区熱田西町1番1号"
            },
            "actors": [
                { "id": 9094, "name": "TRUE(唐沢美帆)" },
                { "id": 4154, "name": "黒沢ともよ" }
            ]
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "495039");
        assert_eq!(candidate.url, "https://www.eventernote.com/events/495039");
        assert_eq!(candidate.title, "『響け！ユーフォニアム』定期演奏会");
        assert_eq!(
            candidate.titles.get("ja").map(String::as_str),
            Some("『響け！ユーフォニアム』定期演奏会")
        );
        assert_eq!(
            candidate.brief.as_deref(),
            Some("2027-04-11 · 14:00 · 名古屋国際会議場 センチュリーホール")
        );
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://eventernote.s3.amazonaws.com/images/events/495039.jpg")
        );
        assert_eq!(candidate.metadata.get("date"), Some(&json!("2027-04-11")));
        assert_eq!(candidate.metadata.get("open_time"), Some(&json!("13:00")));
        assert_eq!(candidate.metadata.get("start_time"), Some(&json!("14:00")));
        assert_eq!(candidate.metadata.get("end_time"), Some(&json!("16:00")));
        assert_eq!(
            candidate.metadata.get("place"),
            Some(&json!("名古屋国際会議場 センチュリーホール"))
        );
        assert_eq!(
            candidate.metadata.get("address"),
            Some(&json!("愛知県名古屋市熱田区熱田西町1番1号"))
        );
        assert_eq!(
            candidate.metadata.get("actors"),
            Some(&json!(["TRUE(唐沢美帆)", "黒沢ともよ"]))
        );
        assert_eq!(
            candidate.metadata.get("official_site"),
            Some(&json!("https://anime-eupho.com/concert10th/"))
        );
        assert_eq!(
            candidate.metadata.get("hashtag"),
            Some(&json!("#ユーフォ定演 #anime_eupho"))
        );
        assert_eq!(
            candidate.metadata.get("participants_count"),
            Some(&json!(52))
        );
        assert_eq!(
            candidate.metadata.get("description"),
            Some(&json!("第10回定期演奏会\n名古屋公演"))
        );
    }

    #[test]
    fn html_event_detail_parses_nested_table_description_and_image() {
        let html = r#"
        <html>
          <head>
            <meta property="og:image" content="https://eventernote.s3.amazonaws.com/images/events/464506_s.jpg" />
          </head>
          <body>
            <div class="gb_events_detail_title clearfix bg">
              <h2>乱舞 -外伝- 金沢公演 &amp; Live</h2>
            </div>
            <div class="gb_events_info_table">
              <table class="table table-striped table-bordered">
                <tr>
                  <td class="span2">開催日時</td>
                  <td><a href="/events/search?year=2026&month=6&day=12">2026-06-12&nbsp;(金)</a></td>
                </tr>
                <tr>
                  <td class="span2">時間</td>
                  <td>開場 18:30 開演 19:00 終演 21:00<br>※目安</td>
                </tr>
                <tr>
                  <td class="span2">開催場所</td>
                  <td><a href="/places/138">金沢AZ(アズ)</a></td>
                </tr>
                <tr>
                  <td>出演者</td>
                  <td>
                    <div class="gb_listview">
                      <ul class="actors inline unstyled">
                        <li><a href="/actors/TRUE/9094">TRUE(唐沢美帆)</a></li>
                        <li><a href="/actors/%E5%AE%AE%E7%94%B0/94927">宮田潤</a></li>
                      </ul>
                    </div>
                  </td>
                </tr>
                <tr>
                  <td>関連リンク</td>
                  <td><a href="https://true-singer.com/contents/1047098">https://true-singer.com/contents/1047098</a></td>
                </tr>
                <tr>
                  <td><img src="https://eventernote.s3.amazonaws.com/images/events/464506.jpg" width="165" height="165" alt=""></td>
                  <td>金沢公演の詳細です。<br />詳細：<a href="https://true-singer.com/">公式</a></td>
                </tr>
                <tr>
                  <td>Twitterハッシュタグ</td>
                  <td><a href="https://twitter.com/search/?q=%23TRUE">#TRUEさん</a></td>
                </tr>
              </table>
            </div>
            <h2>このイベントに参加のイベンター(67人)</h2>
          </body>
        </html>
        "#;

        let candidate = parse_event_detail_html(html, "464506").unwrap();
        assert_eq!(candidate.source_id, "464506");
        assert_eq!(candidate.title, "乱舞 -外伝- 金沢公演 & Live");
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://eventernote.s3.amazonaws.com/images/events/464506.jpg")
        );
        assert_eq!(candidate.metadata.get("date"), Some(&json!("2026-06-12")));
        assert_eq!(candidate.metadata.get("open_time"), Some(&json!("18:30")));
        assert_eq!(candidate.metadata.get("start_time"), Some(&json!("19:00")));
        assert_eq!(candidate.metadata.get("end_time"), Some(&json!("21:00")));
        assert_eq!(
            candidate.metadata.get("place"),
            Some(&json!("金沢AZ(アズ)"))
        );
        assert_eq!(
            candidate.metadata.get("actors"),
            Some(&json!(["TRUE(唐沢美帆)", "宮田潤"]))
        );
        assert_eq!(
            candidate.metadata.get("official_site"),
            Some(&json!("https://true-singer.com/contents/1047098"))
        );
        assert_eq!(candidate.metadata.get("hashtag"), Some(&json!("#TRUEさん")));
        assert_eq!(
            candidate.metadata.get("participants_count"),
            Some(&json!(67))
        );
        assert_eq!(
            candidate.metadata.get("description"),
            Some(&json!("金沢公演の詳細です。\n詳細：公式"))
        );
    }

    #[test]
    fn html_actor_and_place_details_parse_fields() {
        let actor_html = r#"
        <div class="gb_actors_title"><div class="name"><h2>水樹奈々</h2><h2 class="kana">みずきなな</h2></div></div>
        <h2>水樹奈々のファン一覧(<span class="number">6,105</span>)</h2>
        "#;
        let actor = parse_actor_detail_html(actor_html, "28").unwrap();
        assert_eq!(actor.title, "水樹奈々");
        assert_eq!(actor.url, "https://www.eventernote.com/actors/28");
        assert_eq!(actor.metadata.get("kana"), Some(&json!("みずきなな")));
        assert_eq!(actor.metadata.get("favorite_count"), Some(&json!(6105)));

        let place_html = r#"
        <div class="gb_place_detail_title"><h2>金沢AZ(アズ)</h2></div>
        <div class="gb_place_detail_table"><table>
          <tr><td>所在地</td><td><a href="http://maps.google.com/maps?q=x">〒920-0971 石川県金沢市鱗町107番地</a></td></tr>
          <tr><td>電話番号</td><td>076-264-2008</td></tr>
          <tr><td>公式サイト</td><td><a href="https://www.kanazawa-az.com/">ウェブサイト</a></td></tr>
          <tr><td>収容人数</td><td>300人</td></tr>
          <tr><td>座席情報</td><td><a href="https://example.com/seat.pdf">座席情報</a></td></tr>
        </table></div>
        "#;
        let place = parse_place_detail_html(place_html, "138").unwrap();
        assert_eq!(place.title, "金沢AZ(アズ)");
        assert_eq!(place.url, "https://www.eventernote.com/places/138");
        assert_eq!(
            place.metadata.get("postal_code"),
            Some(&json!("〒920-0971"))
        );
        assert_eq!(
            place.metadata.get("address"),
            Some(&json!("石川県金沢市鱗町107番地"))
        );
        assert_eq!(place.metadata.get("tel"), Some(&json!("076-264-2008")));
        assert_eq!(place.metadata.get("capacity"), Some(&json!("300人")));
        assert_eq!(
            place.metadata.get("official_site"),
            Some(&json!("https://www.kanazawa-az.com/"))
        );
        assert_eq!(
            place.metadata.get("seat_url"),
            Some(&json!("https://example.com/seat.pdf"))
        );
    }

    #[test]
    fn crumb_parser_handles_attribute_order_and_empty_value() {
        assert_eq!(
            parse_crumb(r#"<meta id="crumb" content="abc123">"#),
            Some("abc123".to_string())
        );
        assert_eq!(
            parse_crumb(r#"<meta name="crumb" content="xyz789" id="crumb">"#),
            Some("xyz789".to_string())
        );
        assert_eq!(
            parse_crumb(r#"<meta name="crumb" content="" id="crumb">"#),
            None
        );
    }

    #[tokio::test]
    #[ignore = "hits live Eventernote"]
    async fn live_search_and_resolve_work() {
        let mut config = ProviderSearchConfig::default();
        config.add_external_types(&["event".to_string()]);
        let results = search_eventernote("TRUE", 1, 5, &config)
            .await
            .unwrap_or_else(|error| panic!("live search failed: {}", error.message()));
        assert!(!results.is_empty(), "expected live event search results");
        let first = &results[0];
        assert!(first.url.starts_with("https://www.eventernote.com/events/"));
        assert!(first.metadata.contains_key("date"));

        let resolved =
            search_eventernote("https://www.eventernote.com/events/464506", 1, 5, &config)
                .await
                .unwrap_or_else(|error| panic!("live resolve failed: {}", error.message()));
        assert_eq!(resolved.len(), 1);
        let event = &resolved[0];
        assert_eq!(event.source_id, "464506");
        assert_eq!(event.metadata.get("date"), Some(&json!("2026-06-12")));
        assert_eq!(event.metadata.get("place"), Some(&json!("金沢AZ(アズ)")));
        assert!(event.metadata.contains_key("actors"));
    }
}
