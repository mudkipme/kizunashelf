use super::{
    external_client, field_option, named_strings, provider_error, string_list, type_option,
    CredentialSpec, ExternalProvider, ProviderSearchConfig, USER_AGENT,
};
use crate::api::state::AppState;
use crate::api::ApiError;
use crate::contract::{
    ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption,
    ProviderEpisodeGroup, ProviderEpisodeItem, ProviderEpisodes,
};
use crate::secrets::SECRET_DISCOGS_TOKEN;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct DiscogsProvider;

impl ExternalProvider for DiscogsProvider {
    const ID: &'static str = "discogs";
    const LABEL: &'static str = "Discogs";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        !discogs_types(provider_config).is_empty()
    }

    fn credentials() -> &'static [CredentialSpec] {
        &[CredentialSpec {
            key: SECRET_DISCOGS_TOKEN,
            label: "Discogs Token",
            secret: true,
            required: true,
        }]
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        discogs_token(state)
            .is_none()
            .then(|| "Set the Discogs token".to_string())
    }

    fn available(state: &AppState) -> bool {
        discogs_token(state).is_some()
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
        search_discogs(state, q, page, page_size, provider_config).await
    }

    const SUPPORTS_EPISODES: bool = true;

    async fn fetch_episodes(
        state: &AppState,
        ref_value: &str,
        _language: Option<&str>,
    ) -> Result<ProviderEpisodes, ApiError> {
        fetch_discogs_tracks(state, ref_value).await
    }
}

/// Fetches a release/master tracklist. Discogs `heading` rows (vinyl sides, suites)
/// become group labels; the actual tracks key off their `position` (e.g. `A1`, `1`).
async fn fetch_discogs_tracks(
    state: &AppState,
    ref_value: &str,
) -> Result<ProviderEpisodes, ApiError> {
    let (kind, id) =
        discogs_ref(ref_value).ok_or_else(|| ApiError::bad_request("Not a Discogs link"))?;
    let token = discogs_token(state)
        .ok_or_else(|| ApiError::bad_request("Discogs token is not configured"))?;
    let client = external_client();
    let value = client
        .get(format!("https://api.discogs.com/{kind}s/{id}"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Discogs token={token}"),
        )
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    Ok(ProviderEpisodes {
        groups: discogs_track_groups(&value),
    })
}

/// Splits a release `tracklist` into groups on `heading` rows. Tracks before any
/// heading land in a leading unlabeled group; a release with no headings is one
/// flat group.
fn discogs_track_groups(release: &Value) -> Vec<ProviderEpisodeGroup> {
    let Some(tracklist) = release.get("tracklist").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut groups: Vec<ProviderEpisodeGroup> = Vec::new();
    let mut current = ProviderEpisodeGroup {
        label: String::new(),
        items: Vec::new(),
    };
    for entry in tracklist {
        let entry_type = entry
            .get("type_")
            .and_then(Value::as_str)
            .unwrap_or("track");
        let title = entry
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if entry_type == "heading" {
            if !current.label.is_empty() || !current.items.is_empty() {
                groups.push(std::mem::replace(
                    &mut current,
                    ProviderEpisodeGroup {
                        label: String::new(),
                        items: Vec::new(),
                    },
                ));
            }
            current.label = title;
            continue;
        }
        if entry_type != "track" {
            continue; // index/sub-headings without their own line aren't tracks.
        }
        let key = entry
            .get("position")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if key.is_empty() && title.is_empty() {
            continue;
        }
        current.items.push(ProviderEpisodeItem { key, title });
    }
    if !current.label.is_empty() || !current.items.is_empty() {
        groups.push(current);
    }
    groups
}

fn discogs_token(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_DISCOGS_TOKEN)
        .filter(|value| !value.is_empty())
}

/// The Discogs entity kinds (`release`, `master`) this field searches.
/// Unconstrained defaults to `release` (concrete pressings).
fn discogs_types(provider_config: &ProviderSearchConfig) -> Vec<&'static str> {
    let Some(types) = provider_config.external_types() else {
        return vec!["release"];
    };
    let mut entities = Vec::new();
    for kind in ["release", "master"] {
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
        field_option("title", "Title"),
        field_option("cover_url", "Cover URL"),
        field_option("year", "Year"),
        field_option("format", "Format"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("artists", "Artists"),
        field_option("genres", "Genres"),
        field_option("styles", "Styles"),
        field_option("label", "Labels"),
        field_option("company", "Companies"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![
        type_option("release", "Release"),
        type_option("master", "Master"),
    ]
}

async fn search_discogs(
    state: &AppState,
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let types = discogs_types(provider_config);
    if types.is_empty() {
        return Ok(Vec::new());
    }
    let Some(token) = discogs_token(state) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    let authorization = format!("Discogs token={token}");
    // A pasted Discogs URL resolves a single release/master.
    if let Some((kind, id)) = discogs_ref(q) {
        let value = client
            .get(format!("https://api.discogs.com/{kind}s/{id}"))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::AUTHORIZATION, &authorization)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        return Ok(discogs_detail(kind, &id, &value).into_iter().collect());
    }
    let value = client
        .get("https://api.discogs.com/database/search")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::AUTHORIZATION, &authorization)
        .query(&[
            ("q", q),
            ("type", &types.join(",")),
            ("per_page", &page_size.to_string()),
            ("page", &page.to_string()),
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
    Ok(results.iter().filter_map(discogs_search_result).collect())
}

/// Detects a `discogs.com/[locale/]{release|master}/{id}` URL.
fn discogs_ref(q: &str) -> Option<(&'static str, String)> {
    let trimmed = q.trim();
    let (_, rest) = trimmed.split_once("discogs.com/")?;
    let segments: Vec<&str> = rest.split('/').collect();
    let position = segments
        .iter()
        .position(|segment| *segment == "release" || *segment == "master")?;
    let kind = match segments[position] {
        "release" => "release",
        "master" => "master",
        _ => return None,
    };
    // The id is the leading digits of the next segment (Discogs appends a slug).
    let id: String = segments
        .get(position + 1)?
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    (!id.is_empty()).then_some((kind, id))
}

fn discogs_search_result(item: &Value) -> Option<ExternalCandidate> {
    let id = item.get("id").and_then(Value::as_i64)?.to_string();
    let kind = item
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("release");
    if kind != "release" && kind != "master" {
        return None;
    }
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    // Canonical id-based URL (Discogs' `uri` is slug-decorated and differs from
    // the form built on resolve, which would break URL-equality dedup).
    let url = discogs_url(kind, &id);
    let cover_url = item
        .get("cover_image")
        .or_else(|| item.get("thumb"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    insert_year(&mut metadata, item.get("year"));
    if let Some(format) = item
        .get("format")
        .and_then(Value::as_array)
        .and_then(|formats| formats.first())
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("format".to_string(), Value::String(format.to_string()));
    }
    if let Some(genres) = string_list(item.get("genre")) {
        metadata.insert("genres".to_string(), genres);
    }
    if let Some(styles) = string_list(item.get("style")) {
        metadata.insert("styles".to_string(), styles);
    }
    if let Some(label) = string_list(item.get("label")) {
        metadata.insert("label".to_string(), label);
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: DiscogsProvider::ID.to_string(),
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

fn discogs_detail(kind: &str, id: &str, data: &Value) -> Option<ExternalCandidate> {
    let title = data
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = discogs_url(kind, id);
    let cover_url = data
        .get("images")
        .and_then(Value::as_array)
        .and_then(|images| images.first())
        .and_then(|image| image.get("uri").or_else(|| image.get("resource_url")))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let artists = named_list(data.get("artists"));

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    insert_year(&mut metadata, data.get("year"));
    if !artists.is_empty() {
        metadata.insert(
            "artists".to_string(),
            Value::Array(artists.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(genres) = string_list(data.get("genres")) {
        metadata.insert("genres".to_string(), genres);
    }
    if let Some(styles) = string_list(data.get("styles")) {
        metadata.insert("styles".to_string(), styles);
    }
    // `labels` are record labels; `companies` are pressing plants / distributors
    // / etc. — surface them as distinct fields.
    let labels = named_list(data.get("labels"));
    if !labels.is_empty() {
        metadata.insert(
            "label".to_string(),
            Value::Array(labels.into_iter().map(Value::String).collect()),
        );
    }
    let companies = named_list(data.get("companies"));
    if !companies.is_empty() {
        metadata.insert(
            "company".to_string(),
            Value::Array(companies.into_iter().map(Value::String).collect()),
        );
    }
    if let Some(format) = data
        .get("formats")
        .and_then(Value::as_array)
        .and_then(|formats| formats.first())
        .and_then(|format| format.get("name"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("format".to_string(), Value::String(format.to_string()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: DiscogsProvider::ID.to_string(),
        source_id: id.to_string(),
        url,
        original_title: Some(title.clone()),
        title,
        brief: (!artists.is_empty()).then(|| artists.join(", ")),
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

/// Canonical Discogs URL — a pure function of (kind, id) so the same release
/// yields the same `externalRef` from search and from URL resolution.
fn discogs_url(kind: &str, id: &str) -> String {
    format!("https://www.discogs.com/{kind}/{id}")
}

fn insert_year(metadata: &mut Map<String, Value>, value: Option<&Value>) {
    let year = value.and_then(|year| {
        year.as_i64()
            .map(|year| year.to_string())
            .or_else(|| year.as_str().map(str::to_string))
    });
    if let Some(year) = year.filter(|year| !year.is_empty() && year != "0") {
        metadata.insert("year".to_string(), Value::String(year));
    }
}

/// Names from an array of `{ name }` objects, order-preserving and deduplicated
/// (discogs repeats artists/labels across roles).
fn named_list(value: Option<&Value>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for name in named_strings(value, "name") {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::{discogs_detail, discogs_ref, discogs_search_result, discogs_track_groups};
    use serde_json::json;

    #[test]
    fn tracklist_without_headings_is_one_flat_group() {
        let release = json!({
            "tracklist": [
                { "type_": "track", "position": "1", "title": "Never Gonna Give You Up" },
                { "type_": "track", "position": "2", "title": "Whenever You Need Somebody" },
            ]
        });
        let groups = discogs_track_groups(&release);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].label, "");
        assert_eq!(groups[0].items[0].key, "1");
    }

    #[test]
    fn headings_split_tracks_into_groups() {
        let release = json!({
            "tracklist": [
                { "type_": "heading", "title": "Side A" },
                { "type_": "track", "position": "A1", "title": "One" },
                { "type_": "heading", "title": "Side B" },
                { "type_": "track", "position": "B1", "title": "Two" },
            ]
        });
        let groups = discogs_track_groups(&release);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label, "Side A");
        assert_eq!(groups[0].items[0].key, "A1");
        assert_eq!(groups[1].label, "Side B");
        assert_eq!(groups[1].items[0].title, "Two");
    }

    #[test]
    fn search_result_surfaces_metadata() {
        let candidate = discogs_search_result(&json!({
            "id": 249504,
            "type": "release",
            "title": "Rick Astley - Whenever You Need Somebody",
            "year": "1987",
            "format": ["Vinyl", "LP", "Album"],
            "genre": ["Electronic", "Pop"],
            "style": ["Synth-pop"],
            "label": ["RCA"],
            "cover_image": "https://img.discogs.com/cover.jpg",
            "uri": "/release/249504-Rick-Astley"
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "249504");
        // Canonical id-based URL regardless of the slug-decorated search `uri`.
        assert_eq!(candidate.url, "https://www.discogs.com/release/249504");
        assert_eq!(candidate.metadata.get("year"), Some(&json!("1987")));
        assert_eq!(candidate.metadata.get("format"), Some(&json!("Vinyl")));
        assert_eq!(
            candidate.metadata.get("styles"),
            Some(&json!(["Synth-pop"]))
        );
    }

    #[test]
    fn detail_surfaces_artists_and_labels() {
        let candidate = discogs_detail(
            "release",
            "249504",
            &json!({
                "title": "Whenever You Need Somebody",
                "year": 1987,
                "artists": [{ "name": "Rick Astley" }],
                "genres": ["Electronic"],
                "labels": [{ "name": "RCA" }],
                "formats": [{ "name": "Vinyl" }],
                "images": [{ "uri": "https://img.discogs.com/cover.jpg" }]
            }),
        )
        .unwrap();

        assert_eq!(
            candidate.metadata.get("artists"),
            Some(&json!(["Rick Astley"]))
        );
        assert_eq!(candidate.metadata.get("label"), Some(&json!(["RCA"])));
        assert_eq!(candidate.metadata.get("year"), Some(&json!("1987")));
    }

    #[test]
    fn search_url_round_trips_through_resolve() {
        // The canonical URL a search candidate carries must parse back to the
        // same (kind, id) — the property dedup/externalRef relies on.
        let candidate = discogs_search_result(&json!({
            "id": 249504, "type": "release", "title": "X", "uri": "/release/249504-X"
        }))
        .unwrap();
        assert_eq!(
            super::discogs_ref(&candidate.url),
            Some(("release", "249504".to_string()))
        );
    }

    #[test]
    fn ref_parses_release_and_master_urls() {
        assert_eq!(
            discogs_ref("https://www.discogs.com/release/249504-Rick-Astley"),
            Some(("release", "249504".to_string()))
        );
        assert_eq!(
            discogs_ref("https://www.discogs.com/fr/master/96559-Whenever"),
            Some(("master", "96559".to_string()))
        );
        assert_eq!(discogs_ref("rick astley"), None);
    }
}
