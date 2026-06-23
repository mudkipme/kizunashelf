use super::{
    external_client, field_option, provider_error, type_option, ExternalProvider,
    ProviderSearchConfig, USER_AGENT,
};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct ApplePodcastProvider;

impl ExternalProvider for ApplePodcastProvider {
    const ID: &'static str = "applepodcast";
    const LABEL: &'static str = "Apple Podcasts";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        apple_podcast_supported(provider_config)
    }

    fn default_external_types() -> &'static [&'static str] {
        &["podcast"]
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
        search_apple_podcast(q, page, page_size, provider_config).await
    }
}

pub(super) fn apple_podcast_supported(provider_config: &ProviderSearchConfig) -> bool {
    match provider_config.external_types() {
        None => true,
        Some(types) => types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case("podcast")),
    }
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("title", "Title"),
        field_option("host", "Host"),
        field_option("feed_url", "Feed URL"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("genre", "Genres"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("podcast", "Podcast")]
}

async fn search_apple_podcast(
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !apple_podcast_supported(provider_config) {
        return Ok(Vec::new());
    }
    let client = external_client();
    // A pasted Apple Podcasts URL or bare numeric id resolves via the lookup API.
    if let Some(id) = apple_podcast_id(q) {
        let value = client
            .get(format!("https://itunes.apple.com/lookup?id={id}"))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        let result = value
            .get("results")
            .and_then(Value::as_array)
            .and_then(|results| results.first());
        return Ok(result
            .and_then(apple_podcast_candidate)
            .into_iter()
            .collect());
    }
    // iTunes search has no offset; ask for `page * page_size` then skip prior pages.
    let limit = page * page_size;
    let value = client
        .get("https://itunes.apple.com/search")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[
            ("entity", "podcast"),
            ("limit", &limit.to_string()),
            ("term", q),
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
        .skip((page - 1) * page_size)
        .filter_map(apple_podcast_candidate)
        .collect())
}

/// Extracts the Apple Podcasts numeric id from a URL (`…/podcast/<slug>/id123`)
/// or a bare numeric id.
fn apple_podcast_id(q: &str) -> Option<String> {
    let trimmed = q.trim().trim_end_matches('/');
    if trimmed.chars().all(|character| character.is_ascii_digit()) && !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }
    let (_, rest) = trimmed.split_once("/id")?;
    let id = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .then(|| id.to_string())
}

fn apple_podcast_candidate(item: &Value) -> Option<ExternalCandidate> {
    // Podcasts only — the lookup API can return episodes/artists too.
    if let Some(kind) = item.get("kind").and_then(Value::as_str) {
        if kind != "podcast" {
            return None;
        }
    }
    let id = item
        .get("collectionId")
        .or_else(|| item.get("trackId"))
        .and_then(Value::as_i64)?
        .to_string();
    let title = item
        .get("collectionName")
        .or_else(|| item.get("trackName"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = item
        .get("collectionViewUrl")
        .or_else(|| item.get("trackViewUrl"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://podcasts.apple.com/us/podcast/id{id}"));
    let cover_url = item
        .get("artworkUrl600")
        .or_else(|| item.get("artworkUrl100"))
        .or_else(|| item.get("artworkUrl60"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(host) = item
        .get("artistName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("host".to_string(), Value::String(host.to_string()));
    }
    if let Some(feed_url) = item
        .get("feedUrl")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("feed_url".to_string(), Value::String(feed_url.to_string()));
    }
    if let Some(genre) = string_list(item.get("genres")) {
        metadata.insert("genre".to_string(), genre);
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: ApplePodcastProvider::ID.to_string(),
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

fn string_list(value: Option<&Value>) -> Option<Value> {
    let items: Vec<Value> = value?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| Value::String(text.to_string()))
        .collect();
    (!items.is_empty()).then_some(Value::Array(items))
}

#[cfg(test)]
mod tests {
    use super::{apple_podcast_candidate, apple_podcast_id};
    use serde_json::json;

    #[test]
    fn candidate_surfaces_podcast_metadata() {
        let candidate = apple_podcast_candidate(&json!({
            "kind": "podcast",
            "collectionId": 1535809341,
            "collectionName": "The Rust Podcast",
            "artistName": "Rustaceans",
            "feedUrl": "https://example.com/feed.xml",
            "genres": ["Technology", "Education"],
            "artworkUrl600": "https://example.com/art.jpg"
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "1535809341");
        assert_eq!(candidate.metadata.get("host"), Some(&json!("Rustaceans")));
        assert_eq!(
            candidate.metadata.get("feed_url"),
            Some(&json!("https://example.com/feed.xml"))
        );
        assert_eq!(
            candidate.metadata.get("genre"),
            Some(&json!(["Technology", "Education"]))
        );
    }

    #[test]
    fn id_parses_url_and_bare_id() {
        assert_eq!(
            apple_podcast_id("https://podcasts.apple.com/us/podcast/the-show/id1535809341"),
            Some("1535809341".to_string())
        );
        assert_eq!(
            apple_podcast_id("1535809341"),
            Some("1535809341".to_string())
        );
        assert_eq!(apple_podcast_id("the rust podcast"), None);
    }
}
