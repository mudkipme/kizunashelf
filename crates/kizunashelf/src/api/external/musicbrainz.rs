use super::{
    external_client, field_option, provider_error, type_option, ExternalProvider,
    ProviderSearchConfig, USER_AGENT,
};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct MusicBrainzProvider;

impl ExternalProvider for MusicBrainzProvider {
    const ID: &'static str = "musicbrainz";
    const LABEL: &'static str = "MusicBrainz";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        !musicbrainz_entities(provider_config).is_empty()
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
        search_musicbrainz(q, page, page_size, provider_config).await
    }
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("title", "Title / Name"),
        field_option("release_date", "Release / begin date"),
        field_option("end_date", "End date"),
        field_option("country", "Country"),
        field_option("barcode", "Barcode"),
        field_option("type", "Type"),
        field_option("disambiguation", "Disambiguation"),
        field_option("official_site", "Official site"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("artists", "Artists"),
        field_option("label", "Labels"),
        field_option("genres", "Genres"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![
        type_option("release", "Release (album)"),
        type_option("release-group", "Release group"),
        type_option("artist", "Artist"),
    ]
}

/// Which MusicBrainz entities this field searches. Unconstrained defaults to
/// `release` (the common music case); `artist`/`release-group` are opt-in so an
/// album field doesn't surface people. `release-group` searches are not offered
/// (MB's search endpoint is per-entity and release covers the album case), but
/// the type stays selectable for URL resolution.
fn musicbrainz_entities(provider_config: &ProviderSearchConfig) -> Vec<&'static str> {
    let Some(types) = provider_config.external_types() else {
        return vec!["release"];
    };
    let mut entities = Vec::new();
    for entity in ["release", "artist"] {
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

async fn search_musicbrainz(
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let entities = musicbrainz_entities(provider_config);
    if entities.is_empty() {
        return Ok(Vec::new());
    }
    let client = external_client();
    // A pasted MusicBrainz URL resolves a single entity regardless of the filter.
    if let Some((entity, mbid)) = musicbrainz_ref(q) {
        return resolve_musicbrainz(client, entity, &mbid).await;
    }
    let offset = (page - 1) * page_size;
    let mut items = Vec::new();
    for entity in entities {
        let plural = format!("{entity}s");
        let value = client
            .get(format!("https://musicbrainz.org/ws/2/{entity}"))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::ACCEPT, "application/json")
            .query(&[
                ("query", q),
                ("fmt", "json"),
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
        if let Some(found) = value.get(&plural).and_then(Value::as_array) {
            items.extend(found.iter().filter_map(|item| match entity {
                "artist" => musicbrainz_artist(item),
                _ => musicbrainz_release(item, entity),
            }));
        }
    }
    Ok(items)
}

async fn resolve_musicbrainz(
    client: &reqwest::Client,
    entity: &str,
    mbid: &str,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let inc = match entity {
        "artist" => "aliases+url-rels",
        "release" => "artists+labels+release-groups+genres+tags",
        _ => "artists+genres+tags",
    };
    let value = client
        .get(format!("https://musicbrainz.org/ws/2/{entity}/{mbid}"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/json")
        .query(&[("fmt", "json"), ("inc", inc)])
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    let candidate = if entity == "artist" {
        musicbrainz_artist(&value)
    } else {
        musicbrainz_release(&value, entity)
    };
    Ok(candidate.into_iter().collect())
}

/// Detects a `musicbrainz.org/{entity}/{mbid}` URL, returning the entity kind and
/// the 36-char MBID. Bare ids aren't resolved (entity kind is unknowable).
fn musicbrainz_ref(q: &str) -> Option<(&'static str, String)> {
    let trimmed = q.trim();
    let (_, rest) = trimmed.split_once("musicbrainz.org/")?;
    let mut parts = rest.split('/');
    let entity = match parts.next()? {
        "release" => "release",
        "release-group" => "release-group",
        "artist" => "artist",
        _ => return None,
    };
    let mbid = parts
        .next()?
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    is_mbid(mbid).then(|| (entity, mbid.to_string()))
}

/// A MusicBrainz id is a 36-char UUID (hex digits and dashes).
fn is_mbid(value: &str) -> bool {
    value.len() == 36
        && value
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-')
}

fn musicbrainz_release(item: &Value, entity: &str) -> Option<ExternalCandidate> {
    let id = item.get("id").and_then(Value::as_str)?.to_string();
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = format!("https://musicbrainz.org/{entity}/{id}");
    // Cover Art Archive serves a predictable front-image URL per release/group.
    let cover_url = Some(format!(
        "https://coverartarchive.org/{entity}/{id}/front-500"
    ));
    let artists = artist_credit_names(item.get("artist-credit"));

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    // Releases expose `date`; release-groups expose `first-release-date` (and a
    // `date` only on embedded releases) — fall back across both.
    if let Some(date) = item
        .get("date")
        .or_else(|| item.get("first-release-date"))
        .or_else(|| {
            item.get("releases")
                .and_then(Value::as_array)
                .and_then(|releases| releases.first())
                .and_then(|release| release.get("date"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("release_date".to_string(), Value::String(date.to_string()));
    }
    insert_str(&mut metadata, "country", item.get("country"));
    insert_str(&mut metadata, "barcode", item.get("barcode"));
    insert_str(&mut metadata, "disambiguation", item.get("disambiguation"));
    // Surface every label, not just the first.
    let labels: Vec<Value> = item
        .get("label-info")
        .and_then(Value::as_array)
        .map(|label_infos| {
            let mut names = Vec::new();
            for label_info in label_infos {
                if let Some(name) = label_info
                    .get("label")
                    .and_then(|label| label.get("name"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    let name = Value::String(name.to_string());
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            names
        })
        .unwrap_or_default();
    if !labels.is_empty() {
        metadata.insert("label".to_string(), Value::Array(labels));
    }
    if !artists.is_empty() {
        metadata.insert(
            "artists".to_string(),
            Value::Array(artists.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(genres) = genre_names(item) {
        metadata.insert("genres".to_string(), genres);
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    let original_title = title.clone();
    Some(ExternalCandidate {
        provider: MusicBrainzProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(original_title),
        title,
        brief: (!artists.is_empty()).then(|| artists.join(" / ")),
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

fn musicbrainz_artist(item: &Value) -> Option<ExternalCandidate> {
    let id = item.get("id").and_then(Value::as_str)?.to_string();
    let title = item
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = format!("https://musicbrainz.org/artist/{id}");

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    insert_str(&mut metadata, "type", item.get("type"));
    insert_str(&mut metadata, "country", item.get("country"));
    insert_str(&mut metadata, "disambiguation", item.get("disambiguation"));
    if let Some(life_span) = item.get("life-span") {
        insert_str(&mut metadata, "release_date", life_span.get("begin"));
        insert_str(&mut metadata, "end_date", life_span.get("end"));
    }
    // `url-rels` (requested via inc) carries the official homepage relation.
    if let Some(official_site) = item
        .get("relations")
        .and_then(Value::as_array)
        .and_then(|relations| {
            relations.iter().find(|relation| {
                relation.get("type").and_then(Value::as_str) == Some("official homepage")
            })
        })
        .and_then(|relation| relation.get("url"))
        .and_then(|url| url.get("resource"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "official_site".to_string(),
            Value::String(official_site.to_string()),
        );
    }
    // Locale-tagged display aliases (requested via inc) populate the title map.
    let titles = artist_alias_titles(item);
    Some(ExternalCandidate {
        provider: MusicBrainzProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(title.clone()),
        title,
        brief: item
            .get("disambiguation")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        cover_url: None,
        titles,
        metadata,
    })
}

/// Builds a lang→name map from a MusicBrainz artist's display aliases. Only
/// `Artist name` aliases with an explicit locale are used (sort/search/legal
/// names are metadata-only).
fn artist_alias_titles(item: &Value) -> BTreeMap<String, String> {
    let mut titles = BTreeMap::new();
    let Some(aliases) = item.get("aliases").and_then(Value::as_array) else {
        return titles;
    };
    for alias in aliases {
        let alias_type = alias
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !alias_type.is_empty() && alias_type != "Artist name" {
            continue;
        }
        let Some(locale) = alias
            .get("locale")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if let Some(name) = alias
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            titles
                .entry(locale.to_string())
                .or_insert_with(|| name.to_string());
        }
    }
    titles
}

/// Flattens a MusicBrainz `artist-credit` array into display names. Entries are
/// either `{ "artist": { "name": … } }`, `{ "name": … }`, or bare strings.
fn artist_credit_names(value: Option<&Value>) -> Vec<String> {
    let Some(credits) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    credits
        .iter()
        .filter_map(|credit| {
            credit
                .as_str()
                .or_else(|| {
                    credit
                        .get("artist")
                        .and_then(|artist| artist.get("name"))
                        .and_then(Value::as_str)
                })
                .or_else(|| credit.get("name").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .collect()
}

/// Merges `genres` and positive-count `tags`, on the entity and its embedded
/// release-group, de-duplicated.
fn genre_names(item: &Value) -> Option<Value> {
    let mut names = Vec::new();
    let mut push = |value: &Value| {
        if let Some(name) = value
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let name = name.to_string();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    };
    for source in [Some(item), item.get("release-group")]
        .into_iter()
        .flatten()
    {
        if let Some(genres) = source.get("genres").and_then(Value::as_array) {
            genres.iter().for_each(&mut push);
        }
        if let Some(tags) = source.get("tags").and_then(Value::as_array) {
            tags.iter()
                .filter(|tag| tag.get("count").and_then(Value::as_i64).unwrap_or(0) > 0)
                .for_each(&mut push);
        }
    }
    (!names.is_empty()).then(|| Value::Array(names.into_iter().map(Value::String).collect()))
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

#[cfg(test)]
mod tests {
    use super::{musicbrainz_artist, musicbrainz_ref, musicbrainz_release};
    use serde_json::json;

    #[test]
    fn release_surfaces_metadata() {
        let candidate = musicbrainz_release(
            &json!({
                "id": "f5093c06-23e3-404f-aeaa-40f72885ee3a",
                "title": "OK Computer",
                "date": "1997-06-16",
                "country": "GB",
                "artist-credit": [{ "artist": { "name": "Radiohead" } }],
                "genres": [{ "name": "alternative rock" }],
                "tags": [{ "name": "rock", "count": 3 }, { "name": "noise", "count": 0 }]
            }),
            "release",
        )
        .unwrap();

        assert_eq!(
            candidate.metadata.get("release_date"),
            Some(&json!("1997-06-16"))
        );
        assert_eq!(
            candidate.metadata.get("artists"),
            Some(&json!(["Radiohead"]))
        );
        assert_eq!(
            candidate.metadata.get("genres"),
            Some(&json!(["alternative rock", "rock"]))
        );
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://coverartarchive.org/release/f5093c06-23e3-404f-aeaa-40f72885ee3a/front-500")
        );
    }

    #[test]
    fn release_group_falls_back_to_first_release_date_and_all_labels() {
        let candidate = musicbrainz_release(
            &json!({
                "id": "b1392450-e666-3926-a536-22c65f834433",
                "title": "OK Computer",
                "first-release-date": "1997-05-21",
                "label-info": [
                    { "label": { "name": "Parlophone" } },
                    { "label": { "name": "Capitol" } }
                ]
            }),
            "release-group",
        )
        .unwrap();

        // Release-groups lack `date`; fall back to `first-release-date`.
        assert_eq!(
            candidate.metadata.get("release_date"),
            Some(&json!("1997-05-21"))
        );
        // Every label is surfaced, not just the first.
        assert_eq!(
            candidate.metadata.get("label"),
            Some(&json!(["Parlophone", "Capitol"]))
        );
    }

    #[test]
    fn artist_surfaces_metadata() {
        let candidate = musicbrainz_artist(&json!({
            "id": "a74b1b7f-71a5-4011-9441-d0b5e4122711",
            "name": "Radiohead",
            "type": "Group",
            "country": "GB",
            "life-span": { "begin": "1991" },
            "aliases": [
                { "type": "Artist name", "locale": "ja", "name": "レディオヘッド" },
                { "type": "Sort name", "locale": "en", "name": "Radiohead" }
            ],
            "relations": [
                { "type": "official homepage", "url": { "resource": "https://radiohead.com" } }
            ]
        }))
        .unwrap();

        assert_eq!(candidate.metadata.get("type"), Some(&json!("Group")));
        assert_eq!(candidate.metadata.get("release_date"), Some(&json!("1991")));
        // url-rels official homepage → official_site; only display aliases with a
        // locale feed the title map (the "Sort name" alias is excluded).
        assert_eq!(
            candidate.metadata.get("official_site"),
            Some(&json!("https://radiohead.com"))
        );
        assert_eq!(
            candidate.titles.get("ja"),
            Some(&"レディオヘッド".to_string())
        );
        assert_eq!(candidate.titles.get("en"), None);
    }

    #[test]
    fn ref_parses_entity_urls() {
        assert_eq!(
            musicbrainz_ref("https://musicbrainz.org/release/f5093c06-23e3-404f-aeaa-40f72885ee3a"),
            Some((
                "release",
                "f5093c06-23e3-404f-aeaa-40f72885ee3a".to_string()
            ))
        );
        assert_eq!(
            musicbrainz_ref("https://musicbrainz.org/artist/a74b1b7f-71a5-4011-9441-d0b5e4122711"),
            Some(("artist", "a74b1b7f-71a5-4011-9441-d0b5e4122711".to_string()))
        );
        assert_eq!(musicbrainz_ref("radiohead"), None);
    }
}
