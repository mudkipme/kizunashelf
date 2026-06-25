use super::{
    external_client, field_option, provider_error, type_option, ExternalProvider,
    ProviderSearchConfig, USER_AGENT,
};
use crate::api::ApiError;
use crate::contract::{
    ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption,
    ProviderEpisodeGroup, ProviderEpisodeItem, ProviderEpisodes,
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct BangumiProvider;

impl ExternalProvider for BangumiProvider {
    const ID: &'static str = "bangumi";
    const LABEL: &'static str = "Bangumi";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        bangumi_types(provider_config).is_some()
            || bangumi_wants_characters(provider_config)
            || bangumi_wants_persons(provider_config)
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
        search_bangumi(q, page, page_size, provider_config).await
    }

    const SUPPORTS_EPISODES: bool = true;

    async fn fetch_episodes(
        _state: &super::AppState,
        ref_value: &str,
        language: Option<&str>,
    ) -> Result<ProviderEpisodes, ApiError> {
        fetch_bangumi_episodes(ref_value, language).await
    }
}

/// Fetches a Bangumi subject's episodes as a single flat group. A subject is one
/// season, so there are no sub-groups (matching the per-season-entity convention).
/// Bangumi carries a Chinese (`name_cn`) and an original (`name`) title; the
/// viewer's `language` picks which to prefer (Chinese for `zh`, else the original).
async fn fetch_bangumi_episodes(
    ref_value: &str,
    language: Option<&str>,
) -> Result<ProviderEpisodes, ApiError> {
    let subject_id = bangumi_subject_id(ref_value)
        .ok_or_else(|| ApiError::bad_request("Not a Bangumi subject link or id"))?;
    let client = external_client();
    let mut items: Vec<ProviderEpisodeItem> = Vec::new();
    let mut offset = 0usize;
    // Page through `/v0/episodes` (limit 100) until we've collected `total`, with a
    // hard cap so a malformed response can't loop forever.
    loop {
        let url = format!(
            "https://api.bgm.tv/v0/episodes?subject_id={subject_id}&limit=100&offset={offset}"
        );
        let page = bangumi_get(client, &url).await?;
        let data = page.get("data").and_then(Value::as_array);
        let Some(data) = data else { break };
        if data.is_empty() {
            break;
        }
        for episode in data {
            let key = episode
                .get("sort")
                .or_else(|| episode.get("ep"))
                .and_then(format_episode_number)
                .unwrap_or_default();
            let title_keys: &[&str] = if language.unwrap_or("zh").starts_with("zh") {
                &["name_cn", "name"]
            } else {
                &["name", "name_cn"]
            };
            let title = first_non_empty(episode, title_keys);
            if key.is_empty() && title.is_empty() {
                continue;
            }
            items.push(ProviderEpisodeItem { key, title });
        }
        offset += 100;
        let total = page.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
        if offset >= total || offset >= 2000 {
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

/// Formats a JSON number as an episode key: integers as `12`, decimals as `12.5`.
fn format_episode_number(value: &Value) -> Option<String> {
    let number = value.as_f64()?;
    if number == number.trunc() {
        Some((number as i64).to_string())
    } else {
        Some(format!("{number}"))
    }
}

fn first_non_empty(value: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    String::new()
}

async fn search_bangumi(
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let subject_types = bangumi_types(provider_config);
    let wants_characters = bangumi_wants_characters(provider_config);
    let wants_persons = bangumi_wants_persons(provider_config);
    if subject_types.is_none() && !wants_characters && !wants_persons {
        return Ok(Vec::new());
    }
    let client = external_client();
    // Resolve a pasted person URL (or a bare id when the field is persons-only).
    if let Some(person_id) = bangumi_person_id(
        q,
        subject_types.is_none() && wants_persons && !wants_characters,
    ) {
        let value = bangumi_get(
            client,
            &format!("https://api.bgm.tv/v0/persons/{person_id}"),
        )
        .await?;
        return Ok(bangumi_person_candidate(&value).into_iter().collect());
    }
    // Resolve a pasted character URL (or a bare id when the field is
    // characters-only) via the dedicated characters endpoint.
    if let Some(character_id) = bangumi_character_id(
        q,
        subject_types.is_none() && wants_characters && !wants_persons,
    ) {
        let value = bangumi_get(
            client,
            &format!("https://api.bgm.tv/v0/characters/{character_id}"),
        )
        .await?;
        return Ok(bangumi_character_candidate(&value).into_iter().collect());
    }
    // Resolve a pasted subject URL/id (subjects mode only).
    if subject_types.is_some() {
        if let Some(subject_id) = bangumi_subject_id(q) {
            let value = bangumi_get(
                client,
                &format!("https://api.bgm.tv/v0/subjects/{subject_id}"),
            )
            .await?;
            return Ok(bangumi_candidate(&value).into_iter().collect());
        }
    }

    let offset = (page - 1) * page_size;
    let mut items = Vec::new();
    if let Some(filter_types) = &subject_types {
        let response = client
            .post(format!(
                "https://api.bgm.tv/v0/search/subjects?limit={page_size}&offset={offset}"
            ))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .json(&json!({ "keyword": q, "filter": { "type": filter_types } }))
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        if let Some(data) = response.get("data").and_then(Value::as_array) {
            items.extend(data.iter().filter_map(bangumi_candidate));
        }
    }
    if wants_characters {
        let response = client
            .post(format!(
                "https://api.bgm.tv/v0/search/characters?limit={page_size}&offset={offset}"
            ))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .json(&json!({ "keyword": q }))
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        if let Some(data) = response.get("data").and_then(Value::as_array) {
            items.extend(data.iter().filter_map(bangumi_character_candidate));
        }
    }
    if wants_persons {
        let response = client
            .post(format!(
                "https://api.bgm.tv/v0/search/persons?limit={page_size}&offset={offset}"
            ))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .json(&json!({ "keyword": q }))
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        if let Some(data) = response.get("data").and_then(Value::as_array) {
            items.extend(data.iter().filter_map(bangumi_person_candidate));
        }
    }
    Ok(items)
}

async fn bangumi_get(client: &reqwest::Client, url: &str) -> Result<Value, ApiError> {
    client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)
}

fn bangumi_subject_id(q: &str) -> Option<String> {
    let trimmed = q.trim().trim_end_matches('/');
    if trimmed.chars().all(|character| character.is_ascii_digit()) {
        return Some(trimmed.to_string());
    }
    let marker = "/subject/";
    let (_, rest) = trimmed.split_once(marker)?;
    let id = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .then(|| id.to_string())
}

/// Whether the field opted into Bangumi character search/resolution via an
/// explicit `character` external type. Unconstrained fields stay subjects-only.
pub(super) fn bangumi_wants_characters(provider_config: &ProviderSearchConfig) -> bool {
    provider_config
        .external_types()
        .is_some_and(|external_types| {
            external_types
                .iter()
                .any(|external_type| external_type.trim().eq_ignore_ascii_case("character"))
        })
}

/// Whether the field opted into Bangumi person search/resolution via an explicit
/// `person` external type.
pub(super) fn bangumi_wants_persons(provider_config: &ProviderSearchConfig) -> bool {
    provider_config
        .external_types()
        .is_some_and(|external_types| {
            external_types
                .iter()
                .any(|external_type| external_type.trim().eq_ignore_ascii_case("person"))
        })
}

/// Extracts a Bangumi character id from a `bgm.tv/character/<id>` URL, or — when
/// `allow_bare` (a characters-only field) — a bare numeric id.
fn bangumi_character_id(q: &str, allow_bare: bool) -> Option<String> {
    bangumi_people_id(q, "/character/", allow_bare)
}

/// Extracts a Bangumi person id from a `bgm.tv/person/<id>` URL, or — when
/// `allow_bare` (a persons-only field) — a bare numeric id.
fn bangumi_person_id(q: &str, allow_bare: bool) -> Option<String> {
    bangumi_people_id(q, "/person/", allow_bare)
}

fn bangumi_people_id(q: &str, marker: &str, allow_bare: bool) -> Option<String> {
    let trimmed = q.trim().trim_end_matches('/');
    if let Some((_, rest)) = trimmed.split_once(marker) {
        let id = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .trim();
        return (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
            .then(|| id.to_string());
    }
    (allow_bare
        && !trimmed.is_empty()
        && trimmed.chars().all(|character| character.is_ascii_digit()))
    .then(|| trimmed.to_string())
}

fn bangumi_character_candidate(item: &Value) -> Option<ExternalCandidate> {
    bangumi_people_candidate(item, "character")
}

fn bangumi_person_candidate(item: &Value) -> Option<ExternalCandidate> {
    bangumi_people_candidate(item, "person")
}

/// Builds a candidate for a Bangumi character or person (the two endpoints share
/// a shape). `kind` is `character` or `person` and selects the URL path; persons
/// additionally carry `career` and an official site.
fn bangumi_people_candidate(item: &Value, kind: &str) -> Option<ExternalCandidate> {
    let id = item.get("id")?.as_i64()?.to_string();
    let url = format!("https://bgm.tv/{kind}/{id}");
    let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
    let infobox = item.get("infobox").and_then(Value::as_array);
    // The zh name and aliases live in the wiki infobox.
    let name_cn = infobox
        .map(|infobox| infobox_collect(infobox, &["简体中文名"]))
        .and_then(|values| values.into_iter().next())
        .unwrap_or_default();
    let title = if name_cn.is_empty() { name } else { &name_cn };
    if title.is_empty() {
        return None;
    }
    let cover_url = item
        .get("images")
        .and_then(|images| {
            images
                .get("large")
                .or_else(|| images.get("medium"))
                .or_else(|| images.get("grid"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let summary = item
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut titles = BTreeMap::new();
    if !name_cn.is_empty() {
        titles.insert("zh".to_string(), name_cn.clone());
    }
    let mut metadata = Map::new();
    metadata.insert("name".to_string(), Value::String(name.to_string()));
    if !name_cn.is_empty() {
        metadata.insert("name_cn".to_string(), Value::String(name_cn.clone()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(gender) = item
        .get("gender")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("gender".to_string(), Value::String(gender.to_string()));
    }
    // Birthday: persons usually carry a year, characters usually don't, so the
    // value may be `YYYY-MM-DD`, `YYYY-MM`, `YYYY`, or a year-less `MM-DD`.
    if let Some(birthday) = bangumi_birthday(item) {
        metadata.insert("birthday".to_string(), Value::String(birthday));
    }
    if let Some(aliases) = infobox
        .map(|infobox| infobox_collect(infobox, &["别名"]))
        .filter(|values| !values.is_empty())
    {
        metadata.insert(
            "aliases".to_string(),
            Value::Array(aliases.into_iter().map(Value::String).collect()),
        );
    }
    // Persons carry a career list and an official site in the infobox.
    if kind == "person" {
        if let Some(career) = item.get("career").and_then(Value::as_array) {
            let career: Vec<Value> = career
                .iter()
                .filter_map(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(|value| Value::String(value.to_string()))
                .collect();
            if !career.is_empty() {
                metadata.insert("career".to_string(), Value::Array(career));
            }
        }
        if let Some(site) = infobox
            .map(|infobox| infobox_collect(infobox, &["官网", "官方网站", "website"]))
            .and_then(|values| values.into_iter().next())
        {
            metadata.insert("official_site".to_string(), Value::String(site));
        }
    }
    if let Some(summary) = &summary {
        metadata.insert("summary".to_string(), Value::String(summary.clone()));
    }
    Some(ExternalCandidate {
        provider: "bangumi".to_string(),
        source_id: id,
        url,
        original_title: (!name.is_empty()).then(|| name.to_string()),
        title: title.to_string(),
        brief: summary,
        cover_url,
        titles,
        metadata,
    })
}

/// Assembles a birthday from Bangumi's separate `birth_year`/`birth_mon`/
/// `birth_day` integers, tolerating any of them being absent.
fn bangumi_birthday(item: &Value) -> Option<String> {
    let part = |key: &str| {
        item.get(key)
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
    };
    match (part("birth_year"), part("birth_mon"), part("birth_day")) {
        (Some(year), Some(month), Some(day)) => Some(format!("{year:04}-{month:02}-{day:02}")),
        (Some(year), Some(month), None) => Some(format!("{year:04}-{month:02}")),
        (Some(year), None, None) => Some(format!("{year:04}")),
        (None, Some(month), Some(day)) => Some(format!("{month:02}-{day:02}")),
        _ => None,
    }
}

pub(super) fn bangumi_types(provider_config: &ProviderSearchConfig) -> Option<Vec<u32>> {
    let Some(external_types) = provider_config.external_types() else {
        return Some(vec![1, 2, 3, 4, 6]);
    };
    let mut types = BTreeSet::new();
    for external_type in external_types {
        if let Some(value) = bangumi_type(external_type) {
            types.insert(value);
        }
    }
    (!types.is_empty()).then(|| types.into_iter().collect())
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("name", "Name"),
        field_option("name_cn", "Chinese name"),
        field_option("cover_url", "Cover URL"),
        field_option("date", "Release date"),
        field_option("platform", "Platform"),
        // `eps` is the count declared in the wiki infobox; `total_episodes` is how
        // many episode records actually exist in Bangumi's database. They differ
        // for ongoing or sparsely-maintained subjects, so both are offered.
        field_option("eps", "Episodes (declared)"),
        field_option("total_episodes", "Episodes (in database)"),
        field_option("volumes", "Volumes"),
        field_option("score", "Score"),
        field_option("rank", "Rank"),
        // Derived from the wiki `infobox` (present on a subject fetch, not search).
        field_option("language", "Language"),
        field_option("publisher", "Publisher"),
        field_option("official_site", "Official site"),
        field_option("isbn", "ISBN"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("aliases", "Aliases"),
        field_option("director", "Director"),
        field_option("author", "Author"),
        field_option("genre", "Genre"),
        field_option("tags", "Tags"),
        field_option("meta_tags", "Meta tags"),
        // Character/person fields (from the `/v0/characters` & `/v0/persons`
        // endpoints). `birthday` may be `YYYY-MM-DD`, `YYYY`, or a year-less
        // `MM-DD` (characters often lack a year); `career` is persons-only.
        field_option("gender", "Gender"),
        field_option("birthday", "Birthday"),
        field_option("career", "Career"),
        field_option("summary", "Summary"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![
        type_option("1", "Book (1)"),
        type_option("2", "Anime (2)"),
        type_option("3", "Music (3)"),
        type_option("4", "Game (4)"),
        type_option("6", "Real (6)"),
        type_option("character", "Character"),
        type_option("person", "Person"),
    ]
}

/// Collects values for the given `infobox` keys (tried in order) into a
/// de-duplicated string list. A Bangumi infobox entry's `value` is either a plain
/// string or an array of `{ v }` (and sometimes `{ k, v }`) objects.
fn infobox_collect(infobox: &[Value], keys: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    let mut push = |text: &str| {
        let text = text.trim();
        if !text.is_empty() && !values.iter().any(|existing| existing == text) {
            values.push(text.to_string());
        }
    };
    for key in keys {
        for entry in infobox {
            if entry.get("key").and_then(Value::as_str) != Some(*key) {
                continue;
            }
            match entry.get("value") {
                Some(Value::String(text)) => push(text),
                Some(Value::Array(items)) => {
                    for item in items {
                        if let Some(text) = item
                            .get("v")
                            .and_then(Value::as_str)
                            .or_else(|| item.as_str())
                        {
                            push(text);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    values
}

/// Collects an array's elements into a JSON string array via `extract`, dropping
/// empties and returning `None` when the source is missing or yields nothing.
/// Used for list-shaped metadata (tags, genres) that maps onto list-type fields.
fn string_list<'a>(
    value: Option<&'a Value>,
    extract: impl Fn(&'a Value) -> Option<&'a str>,
) -> Option<Value> {
    let items: Vec<Value> = value?
        .as_array()?
        .iter()
        .filter_map(extract)
        .filter(|text| !text.is_empty())
        .map(|text| Value::String(text.to_string()))
        .collect();
    (!items.is_empty()).then_some(Value::Array(items))
}

fn bangumi_type(external_type: &str) -> Option<u32> {
    match external_type.trim().to_ascii_lowercase().as_str() {
        "1" => Some(1),
        "2" => Some(2),
        "3" => Some(3),
        "4" => Some(4),
        "6" => Some(6),
        _ => None,
    }
}

fn bangumi_candidate(item: &Value) -> Option<ExternalCandidate> {
    let id = item.get("id")?.as_i64()?.to_string();
    let url = format!("https://bgm.tv/subject/{id}");
    let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
    let name_cn = item
        .get("name_cn")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let title = if name_cn.is_empty() { name } else { name_cn };
    if title.is_empty() {
        return None;
    }
    let cover_url = item
        .get("images")
        .and_then(|images| {
            // Prefer the largest available.
            images
                .get("large")
                .or_else(|| images.get("common"))
                .or_else(|| images.get("grid"))
        })
        .and_then(Value::as_str)
        .map(str::to_string);
    let release_date = item.get("date").and_then(Value::as_str).unwrap_or_default();
    let mut titles = BTreeMap::new();
    // Only `name_cn` has a reliable language (Chinese). `name` is the original
    // title in an unknown language, so it is left untagged rather than guessed.
    if !name_cn.is_empty() {
        titles.insert("zh".to_string(), name_cn.to_string());
    }
    let mut metadata = Map::new();
    metadata.insert("name".to_string(), Value::String(name.to_string()));
    if !name_cn.is_empty() {
        metadata.insert("name_cn".to_string(), Value::String(name_cn.to_string()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if !release_date.is_empty() {
        metadata.insert("date".to_string(), Value::String(release_date.to_string()));
    }
    if let Some(platform) = item
        .get("platform")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("platform".to_string(), Value::String(platform.to_string()));
    }
    for (key, label) in [
        ("total_episodes", "total_episodes"),
        ("eps", "eps"),
        ("volumes", "volumes"),
    ] {
        if let Some(count) = item
            .get(key)
            .and_then(Value::as_i64)
            .filter(|count| *count > 0)
        {
            metadata.insert(label.to_string(), Value::Number(count.into()));
        }
    }
    // Score/rank live under `rating` on a subject fetch but at the top level on
    // some search responses; accept either shape.
    let rating = item.get("rating");
    if let Some(score) = rating
        .and_then(|rating| rating.get("score"))
        .or_else(|| item.get("score"))
        .and_then(Value::as_f64)
        .filter(|score| *score > 0.0)
    {
        metadata.insert("score".to_string(), json!(score));
    }
    if let Some(rank) = rating
        .and_then(|rating| rating.get("rank"))
        .or_else(|| item.get("rank"))
        .and_then(Value::as_i64)
        .filter(|rank| *rank > 0)
    {
        metadata.insert("rank".to_string(), Value::Number(rank.into()));
    }
    if let Some(tags) = string_list(item.get("tags"), |tag| {
        tag.get("name").and_then(Value::as_str)
    }) {
        metadata.insert("tags".to_string(), tags);
    }
    if let Some(meta_tags) = string_list(item.get("meta_tags"), Value::as_str) {
        metadata.insert("meta_tags".to_string(), meta_tags);
    }
    if let Some(summary) = item
        .get("summary")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("summary".to_string(), Value::String(summary.to_string()));
    }
    // The wiki `infobox` (present on a subject fetch, not in search results) holds
    // structured production metadata under Chinese keys; surface the useful ones.
    if let Some(infobox) = item.get("infobox").and_then(Value::as_array) {
        for (field, keys) in [
            ("aliases", &["别名"][..]),
            ("director", &["导演", "演出"][..]),
            ("author", &["作者", "作画", "原作"][..]),
            ("genre", &["类型", "游戏类型"][..]),
        ] {
            let values = infobox_collect(infobox, keys);
            if !values.is_empty() {
                metadata.insert(
                    field.to_string(),
                    Value::Array(values.into_iter().map(Value::String).collect()),
                );
            }
        }
        for (field, keys) in [
            ("language", &["语言"][..]),
            ("publisher", &["出版社"][..]),
            ("official_site", &["官方网站", "website"][..]),
            // Books expose `ISBN` (13-digit) and a separate `ISBN-10`; prefer the 13.
            ("isbn", &["ISBN", "ISBN-10"][..]),
        ] {
            if let Some(value) = infobox_collect(infobox, keys).into_iter().next() {
                metadata.insert(field.to_string(), Value::String(value));
            }
        }
    }
    Some(ExternalCandidate {
        provider: "bangumi".to_string(),
        source_id: id,
        url,
        title: title.to_string(),
        original_title: (!name.is_empty()).then(|| name.to_string()),
        brief: item
            .get("summary")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        cover_url,
        titles,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::{bangumi_candidate, format_episode_number};
    use serde_json::json;

    #[test]
    fn episode_number_keeps_specials_and_drops_trailing_zero() {
        assert_eq!(format_episode_number(&json!(12)), Some("12".to_string()));
        assert_eq!(format_episode_number(&json!(0)), Some("0".to_string()));
        assert_eq!(
            format_episode_number(&json!(12.5)),
            Some("12.5".to_string())
        );
        assert_eq!(format_episode_number(&json!(13.0)), Some("13".to_string()));
        assert_eq!(format_episode_number(&json!("nope")), None);
    }

    #[test]
    fn candidate_surfaces_extended_metadata() {
        let candidate = bangumi_candidate(&json!({
            "id": 8,
            "name": "Cowboy Bebop",
            "name_cn": "星际牛仔",
            "platform": "TV",
            "eps": 26,
            "total_episodes": 26,
            "rating": { "score": 8.7, "rank": 42 },
            "tags": [{ "name": "Sci-Fi", "count": 100 }, { "name": "Space", "count": 50 }],
            "meta_tags": ["TV", "Original"]
        }))
        .unwrap();

        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("platform"), Some(&json!("TV")));
        assert_eq!(metadata.get("eps"), Some(&json!(26)));
        assert_eq!(metadata.get("score"), Some(&json!(8.7)));
        assert_eq!(metadata.get("rank"), Some(&json!(42)));
        assert_eq!(metadata.get("tags"), Some(&json!(["Sci-Fi", "Space"])));
        assert_eq!(metadata.get("meta_tags"), Some(&json!(["TV", "Original"])));
    }

    #[test]
    fn character_candidate_surfaces_metadata() {
        let candidate = super::bangumi_character_candidate(&json!({
            "id": 47,
            "name": "キョン",
            "gender": "male",
            "birth_mon": 10,
            "birth_day": 11,
            "images": { "large": "https://img/large.jpg", "grid": "https://img/grid.jpg" },
            "summary": "本作的主角。",
            "infobox": [
                { "key": "简体中文名", "value": "阿虚" },
                { "key": "别名", "value": [{ "k": "罗马字", "v": "Kyon" }] }
            ]
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "47");
        assert_eq!(candidate.url, "https://bgm.tv/character/47");
        // zh name is the display title; the original (ja) name is preserved.
        assert_eq!(candidate.title, "阿虚");
        assert_eq!(candidate.original_title.as_deref(), Some("キョン"));
        assert_eq!(candidate.titles.get("zh"), Some(&"阿虚".to_string()));
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://img/large.jpg")
        );
        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("gender"), Some(&json!("male")));
        assert_eq!(metadata.get("aliases"), Some(&json!(["Kyon"])));
        // A character without a birth year yields a year-less `MM-DD`.
        assert_eq!(metadata.get("birthday"), Some(&json!("10-11")));
        assert_eq!(metadata.get("career"), None);
    }

    #[test]
    fn person_candidate_surfaces_birthday_and_career() {
        let candidate = super::bangumi_person_candidate(&json!({
            "id": 4,
            "name": "水樹奈々",
            "career": ["artist", "seiyu"],
            "birth_year": 1980,
            "birth_mon": 1,
            "birth_day": 21,
            "images": { "large": "https://img/p.jpg" },
            "infobox": [
                { "key": "简体中文名", "value": "水树奈奈" },
                { "key": "官网", "value": "https://www.mizukinana.jp" }
            ]
        }))
        .unwrap();

        assert_eq!(candidate.url, "https://bgm.tv/person/4");
        assert_eq!(candidate.title, "水树奈奈");
        assert_eq!(candidate.original_title.as_deref(), Some("水樹奈々"));
        let metadata = &candidate.metadata;
        // Persons usually carry a year → full ISO date.
        assert_eq!(metadata.get("birthday"), Some(&json!("1980-01-21")));
        assert_eq!(metadata.get("career"), Some(&json!(["artist", "seiyu"])));
        assert_eq!(
            metadata.get("official_site"),
            Some(&json!("https://www.mizukinana.jp"))
        );
    }

    #[test]
    fn character_and_person_ids_parse_url_and_bare_when_allowed() {
        assert_eq!(
            super::bangumi_character_id("https://bgm.tv/character/47", false),
            Some("47".to_string())
        );
        assert_eq!(
            super::bangumi_person_id("https://bgm.tv/person/4", false),
            Some("4".to_string())
        );
        // A bare id resolves only when the field is character-/person-only.
        assert_eq!(super::bangumi_person_id("4", false), None);
        assert_eq!(super::bangumi_person_id("4", true), Some("4".to_string()));
    }

    #[test]
    fn candidate_parses_infobox_cover_and_titles() {
        let candidate = bangumi_candidate(&json!({
            "id": 8,
            "name": "Cowboy Bebop",
            "name_cn": "星际牛仔",
            "images": { "large": "https://img/large.jpg", "common": "https://img/common.jpg" },
            "infobox": [
                { "key": "别名", "value": [{ "v": "カウボーイビバップ" }, { "v": "COWBOY BEBOP" }] },
                { "key": "导演", "value": "渡边信一郎" },
                { "key": "类型", "value": "科幻" },
                { "key": "语言", "value": "日语" },
                { "key": "官方网站", "value": "https://example.com" },
                { "key": "ISBN", "value": "9784040000000" },
                { "key": "ISBN-10", "value": "4040000009" }
            ]
        }))
        .unwrap();

        // Cover prefers `large`; the original name is left untagged (only zh known).
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://img/large.jpg")
        );
        assert_eq!(candidate.titles.get("zh"), Some(&"星际牛仔".to_string()));
        assert_eq!(candidate.titles.get("ja"), None);

        let metadata = &candidate.metadata;
        assert_eq!(
            metadata.get("aliases"),
            Some(&json!(["カウボーイビバップ", "COWBOY BEBOP"]))
        );
        assert_eq!(metadata.get("director"), Some(&json!(["渡边信一郎"])));
        assert_eq!(metadata.get("genre"), Some(&json!(["科幻"])));
        assert_eq!(metadata.get("language"), Some(&json!("日语")));
        assert_eq!(
            metadata.get("official_site"),
            Some(&json!("https://example.com"))
        );
        // Prefer the 13-digit `ISBN` over the separate `ISBN-10`.
        assert_eq!(metadata.get("isbn"), Some(&json!("9784040000000")));
    }
}
