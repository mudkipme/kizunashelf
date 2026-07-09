use super::{
    external_client, field_option, named_strings, non_empty_string_or_integer, provider_error,
    string_array, strip_html_collapsed, type_option, CredentialSpec, ExternalProvider,
    ProviderResponseExt, ProviderSearchConfig, USER_AGENT,
};
use crate::api::state::AppState;
use crate::api::ApiError;
use crate::contract::{
    ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption,
    ProviderEpisodeGroup, ProviderEpisodeItem, ProviderEpisodes,
};
use crate::secrets::SECRET_COMICVINE_API_KEY;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::collections::BTreeMap;

pub(super) struct ComicVineProvider;

const BASE: &str = "https://comicvine.gamespot.com/api";
// ComicVine prefixes volume ids with `4050-` in API paths.
const VOLUME_PREFIX: &str = "4050-";

impl ExternalProvider for ComicVineProvider {
    const ID: &'static str = "comicvine";
    const LABEL: &'static str = "Comic Vine";

    fn recognizes_url(q: &str) -> bool {
        q.contains("comicvine.gamespot.com/")
    }

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        comic_supported(provider_config)
    }

    fn credentials() -> &'static [CredentialSpec] {
        &[CredentialSpec {
            key: SECRET_COMICVINE_API_KEY,
            label: "Comic Vine API Key",
            secret: true,
            required: true,
        }]
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        comicvine_api_key(state)
            .is_none()
            .then(|| "Set the Comic Vine API key".to_string())
    }

    fn available(state: &AppState) -> bool {
        comicvine_api_key(state).is_some()
    }

    fn default_external_types() -> &'static [&'static str] {
        &["comic"]
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
        search_comicvine(state, q, page, page_size, provider_config).await
    }

    const SUPPORTS_EPISODES: bool = true;

    async fn fetch_episodes(
        state: &AppState,
        ref_value: &str,
        _language: Option<&str>,
    ) -> Result<ProviderEpisodes, ApiError> {
        fetch_comicvine_issues(state, ref_value).await
    }
}

/// Fetches a volume's issues as one flat list, keyed by issue number. The volume
/// detail's `issues` array already carries each issue's number and name, so no
/// per-issue request is needed.
async fn fetch_comicvine_issues(
    state: &AppState,
    ref_value: &str,
) -> Result<ProviderEpisodes, ApiError> {
    let id = comicvine_id(ref_value)
        .ok_or_else(|| ApiError::bad_request("Not a Comic Vine volume link or id"))?;
    let api_key = comicvine_api_key(state)
        .ok_or_else(|| ApiError::bad_request("Comic Vine API key is not configured"))?;
    let client = external_client();
    let value = client
        .get(format!("{BASE}/volume/{VOLUME_PREFIX}{id}/"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[
            ("api_key", api_key.as_str()),
            ("format", "json"),
            ("field_list", "issues"),
        ])
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status_body()
        .await?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    let issues = value
        .pointer("/results/issues")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let items = comicvine_issue_items(&issues);
    let groups = if items.is_empty() {
        Vec::new()
    } else {
        vec![ProviderEpisodeGroup {
            label: String::new(),
            items,
        }]
    };
    Ok(ProviderEpisodes { groups })
}

/// Maps a volume's `issues` into items keyed by `issue_number`, sorted ascending by
/// number (issues with a non-numeric number — e.g. "Annual 1" — sort last).
fn comicvine_issue_items(issues: &[Value]) -> Vec<ProviderEpisodeItem> {
    let mut ordered: Vec<(Option<f64>, ProviderEpisodeItem)> = issues
        .iter()
        .filter_map(|issue| {
            let key = issue
                .get("issue_number")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let title = issue
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            if key.is_empty() && title.is_empty() {
                return None;
            }
            let order = key.parse::<f64>().ok();
            // The volume's issue list carries no per-issue date (only the issue
            // detail does), so leave it unset.
            Some((
                order,
                ProviderEpisodeItem {
                    key,
                    title,
                    date: None,
                },
            ))
        })
        .collect();
    ordered.sort_by(|left, right| match (left.0, right.0) {
        (Some(left), Some(right)) => left.total_cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
    ordered.into_iter().map(|(_, item)| item).collect()
}

fn comicvine_api_key(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_COMICVINE_API_KEY)
        .filter(|value| !value.is_empty())
}

pub(super) fn comic_supported(provider_config: &ProviderSearchConfig) -> bool {
    match provider_config.external_types() {
        None => true,
        Some(types) => types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case("comic")),
    }
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("title", "Title"),
        field_option("cover_url", "Cover URL"),
        field_option("publisher", "Publisher"),
        field_option("start_year", "Start year"),
        field_option("issues_count", "Issue count"),
        field_option("description", "Description"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("genres", "Genres"),
        field_option("people", "People"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("comic", "Comic")]
}

async fn search_comicvine(
    state: &AppState,
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !comic_supported(provider_config) {
        return Ok(Vec::new());
    }
    let Some(api_key) = comicvine_api_key(state) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    // A pasted Comic Vine URL or bare id resolves a single volume.
    if let Some(id) = comicvine_id(q) {
        let value = client
            .get(format!("{BASE}/volume/{VOLUME_PREFIX}{id}/"))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .query(&[
                ("api_key", api_key.as_str()),
                ("format", "json"),
                (
                    "field_list",
                    "id,name,image,description,publisher,start_year,count_of_issues,concepts,people",
                ),
            ])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status_body()
            .await?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        let results = value.get("results");
        return Ok(results
            .filter(|results| results.is_object())
            .and_then(|volume| comicvine_volume(&id, volume))
            .into_iter()
            .collect());
    }
    let value = client
        .get(format!("{BASE}/search/"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[
            ("api_key", api_key.as_str()),
            ("format", "json"),
            ("query", q),
            ("resources", "volume"),
            (
                "field_list",
                "id,name,image,publisher,start_year,count_of_issues",
            ),
            ("limit", &page_size.to_string()),
            ("page", &page.to_string()),
        ])
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status_body()
        .await?
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
        .filter_map(|volume| {
            let id = volume.get("id").and_then(Value::as_i64)?.to_string();
            comicvine_volume(&id, volume)
        })
        .collect())
}

/// Extracts a Comic Vine volume id from a `…/4050-<id>` URL, a bare `4050-<id>`,
/// or a bare numeric id.
fn comicvine_id(q: &str) -> Option<String> {
    let trimmed = q.trim().trim_end_matches('/');
    if !trimmed.is_empty() && trimmed.chars().all(|character| character.is_ascii_digit()) {
        return Some(trimmed.to_string());
    }
    let candidate = trimmed.rsplit('/').next().unwrap_or(trimmed);
    let id = candidate.strip_prefix(VOLUME_PREFIX).unwrap_or(candidate);
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .then(|| id.to_string())
}

fn comicvine_volume(id: &str, volume: &Value) -> Option<ExternalCandidate> {
    let title = volume
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    // Canonical id-based URL (the API's `site_detail_url` is slug-decorated and
    // differs from the resolve form, which would break URL-equality dedup).
    let url = format!("https://comicvine.gamespot.com/volume/{VOLUME_PREFIX}{id}/");
    let cover_url = volume
        .get("image")
        .and_then(|image| {
            image
                .get("medium_url")
                .or_else(|| image.get("original_url"))
                .or_else(|| image.get("screen_url"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let description = volume
        .get("description")
        .and_then(Value::as_str)
        .map(strip_html_collapsed)
        .filter(|value| !value.is_empty());

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(publisher) = volume
        .get("publisher")
        .and_then(|publisher| publisher.get("name"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "publisher".to_string(),
            Value::String(publisher.to_string()),
        );
    }
    if let Some(start_year) = volume
        .get("start_year")
        .and_then(non_empty_string_or_integer)
    {
        metadata.insert("start_year".to_string(), Value::String(start_year));
    }
    if let Some(issues) = volume
        .get("count_of_issues")
        .and_then(Value::as_i64)
        .filter(|count| *count > 0)
    {
        metadata.insert("issues_count".to_string(), Value::Number(issues.into()));
    }
    // `concepts` are ComicVine's closest analog to genres.
    if let Some(genres) = named_list(volume.get("concepts"), 5) {
        metadata.insert("genres".to_string(), genres);
    }
    if let Some(people) = named_list(volume.get("people"), 5) {
        metadata.insert("people".to_string(), people);
    }
    if let Some(description) = &description {
        metadata.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: ComicVineProvider::ID.to_string(),
        source_id: id.to_string(),
        url,
        original_title: Some(title.clone()),
        title,
        brief: description,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

/// Up to `limit` names from an array of `{ name }` objects.
fn named_list(value: Option<&Value>, limit: usize) -> Option<Value> {
    string_array(
        named_strings(value, "name")
            .into_iter()
            .take(limit)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::{comicvine_id, comicvine_issue_items, comicvine_volume};
    use serde_json::json;

    #[test]
    fn issue_items_sort_numerically_with_non_numeric_last() {
        let issues = vec![
            json!({ "issue_number": "2", "name": "Second" }),
            json!({ "issue_number": "Annual 1", "name": "Annual" }),
            json!({ "issue_number": "1", "name": "First" }),
            json!({ "issue_number": "1.5", "name": "Point One" }),
        ];
        let items = comicvine_issue_items(&issues);
        let keys: Vec<&str> = items.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(keys, vec!["1", "1.5", "2", "Annual 1"]);
        assert_eq!(items[0].title, "First");
    }

    #[test]
    fn volume_surfaces_metadata() {
        let candidate = comicvine_volume(
            "18166",
            &json!({
                "id": 18166,
                "name": "Saga",
                "image": { "medium_url": "https://img/saga.jpg" },
                "description": "<p>An epic <i>space</i> opera.</p>",
                "publisher": { "name": "Image Comics" },
                "start_year": "2012",
                "count_of_issues": 66,
                "concepts": [{ "name": "Space Opera" }, { "name": "War" }],
                "people": [{ "name": "Brian K. Vaughan" }, { "name": "Fiona Staples" }]
            }),
        )
        .unwrap();

        assert_eq!(candidate.source_id, "18166");
        // Canonical id-based URL, identical from search and resolve.
        assert_eq!(
            candidate.url,
            "https://comicvine.gamespot.com/volume/4050-18166/"
        );
        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("publisher"), Some(&json!("Image Comics")));
        assert_eq!(metadata.get("start_year"), Some(&json!("2012")));
        assert_eq!(metadata.get("issues_count"), Some(&json!(66)));
        assert_eq!(metadata.get("genres"), Some(&json!(["Space Opera", "War"])));
        assert_eq!(
            metadata.get("people"),
            Some(&json!(["Brian K. Vaughan", "Fiona Staples"]))
        );
        assert_eq!(
            metadata.get("description"),
            Some(&json!("An epic space opera."))
        );
    }

    #[test]
    fn volume_url_round_trips_through_resolve() {
        let candidate = comicvine_volume("18166", &json!({ "id": 18166, "name": "Saga" })).unwrap();
        assert_eq!(
            super::comicvine_id(&candidate.url),
            Some("18166".to_string())
        );
    }

    #[test]
    fn id_parses_url_prefix_and_bare() {
        assert_eq!(
            comicvine_id("https://comicvine.gamespot.com/saga/4050-18166/"),
            Some("18166".to_string())
        );
        assert_eq!(comicvine_id("4050-18166"), Some("18166".to_string()));
        assert_eq!(comicvine_id("18166"), Some("18166".to_string()));
        assert_eq!(comicvine_id("saga"), None);
    }
}
