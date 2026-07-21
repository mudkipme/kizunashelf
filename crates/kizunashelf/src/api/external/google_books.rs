use super::{
    external_client, field_option, insert_str, normalize_isbn, provider_error, send_limited,
    string_list, strip_html, type_option, CredentialSpec, ExternalProvider, ProviderResponseExt,
    ProviderSearchConfig, USER_AGENT,
};
use crate::api::state::AppState;
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use crate::secrets::SECRET_GOOGLE_BOOKS_API_KEY;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct GoogleBooksProvider;

impl ExternalProvider for GoogleBooksProvider {
    const ID: &'static str = "googlebooks";
    const LABEL: &'static str = "Google Books";

    fn recognizes_url(q: &str) -> bool {
        google_books_volume_id(q).is_some()
    }

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        google_books_supported(provider_config)
    }

    fn credentials() -> &'static [CredentialSpec] {
        &[CredentialSpec {
            key: SECRET_GOOGLE_BOOKS_API_KEY,
            label: "Google Books API Key",
            secret: true,
            required: true,
        }]
    }

    fn available(state: &AppState) -> bool {
        google_books_api_key(state).is_some()
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        google_books_api_key(state)
            .is_none()
            .then(|| "Set the Google Books API key".to_string())
    }

    fn default_external_types() -> &'static [&'static str] {
        &["book"]
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
        search_google_books(state, q, page, page_size, provider_config).await
    }
}

fn google_books_api_key(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_GOOGLE_BOOKS_API_KEY)
        .filter(|value| !value.is_empty())
}

/// Google Books only catalogs books; honor an explicit `book` constraint but
/// accept an unconstrained source too.
pub(super) fn google_books_supported(provider_config: &ProviderSearchConfig) -> bool {
    match provider_config.external_types() {
        None => true,
        Some(types) => types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case("book")),
    }
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("title", "Title"),
        field_option("cover_url", "Cover URL"),
        field_option("subtitle", "Subtitle"),
        field_option("publisher", "Publisher"),
        field_option("published_date", "Published date"),
        field_option("pages", "Pages"),
        field_option("isbn", "ISBN"),
        field_option("language", "Language"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("authors", "Authors"),
        field_option("categories", "Categories"),
        field_option("description", "Description"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("book", "Book")]
}

async fn search_google_books(
    state: &AppState,
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !google_books_supported(provider_config) {
        return Ok(Vec::new());
    }
    // Keyless access to the Google Books API shares one exhausted anonymous quota
    // and returns HTTP 429 for every request, so a key is required.
    let api_key = google_books_api_key(state)
        .ok_or_else(|| ApiError::bad_request("Set the Google Books API key"))?;
    let client = external_client();
    // A pasted Google Books URL/volume id resolves to a single volume.
    if let Some(volume_id) = google_books_volume_id(q) {
        let value = send_limited(
            client
                .get(format!(
                    "https://www.googleapis.com/books/v1/volumes/{volume_id}"
                ))
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .query(&[("country", "us"), ("key", api_key.as_str())]),
        )
        .await
        .map_err(provider_error)?
        .error_for_status_body()
        .await?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
        return Ok(google_books_candidate(&value).into_iter().collect());
    }
    let start_index = (page - 1) * page_size;
    let response = send_limited(
        client
            .get("https://www.googleapis.com/books/v1/volumes")
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .query(&[
                ("country", "us"),
                ("q", q),
                ("startIndex", &start_index.to_string()),
                ("maxResults", &page_size.to_string()),
                ("maxAllowedMaturityRating", "MATURE"),
                ("key", api_key.as_str()),
            ]),
    )
    .await
    .map_err(provider_error)?
    .error_for_status_body()
    .await?
    .json::<Value>()
    .await
    .map_err(provider_error)?;
    let items = response
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items.iter().filter_map(google_books_candidate).collect())
}

/// Extracts a Google Books volume id from a pasted URL, or `None` for a plain search term.
fn google_books_volume_id(q: &str) -> Option<String> {
    let trimmed = q.trim();
    if !trimmed.contains("google.") {
        return None;
    }
    if let Some((_, rest)) = trimmed.split_once("id=") {
        let id = rest.split(['&', '#']).next().unwrap_or_default().trim();
        return (!id.is_empty()).then(|| id.to_string());
    }
    if let Some((_, rest)) = trimmed.split_once("/books/edition/") {
        // `<slug>/<ID>` — the id is the second path segment.
        let id = rest
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .split('/')
            .nth(1)
            .unwrap_or_default()
            .trim();
        return (!id.is_empty()).then(|| id.to_string());
    }
    None
}

fn google_books_candidate(item: &Value) -> Option<ExternalCandidate> {
    let id = item.get("id").and_then(Value::as_str)?.to_string();
    let info = item.get("volumeInfo")?;
    let title = info
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = format!("https://books.google.com/books?id={id}");
    let cover_url = info
        .get("imageLinks")
        .and_then(|links| {
            ["extraLarge", "large", "medium", "small", "thumbnail"]
                .iter()
                .find_map(|key| links.get(*key).and_then(Value::as_str))
        })
        .map(|value| value.replacen("http://", "https://", 1));
    let description = info
        .get("description")
        .and_then(Value::as_str)
        .or_else(|| {
            item.get("searchInfo")
                .and_then(|search| search.get("textSnippet"))
                .and_then(Value::as_str)
        })
        .map(strip_html)
        .filter(|value| !value.is_empty());

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    insert_str(&mut metadata, "subtitle", info.get("subtitle"));
    insert_str(&mut metadata, "publisher", info.get("publisher"));
    insert_str(&mut metadata, "published_date", info.get("publishedDate"));
    insert_str(&mut metadata, "language", info.get("language"));
    if let Some(pages) = info.get("pageCount").and_then(Value::as_i64) {
        if pages > 0 {
            metadata.insert("pages".to_string(), Value::Number(pages.into()));
        }
    }
    if let Some(isbn) = google_books_isbn(info) {
        // Prefer a 13-digit ISBN: convert a lone ISBN-10.
        metadata.insert("isbn".to_string(), Value::String(normalize_isbn(&isbn)));
    }
    if let Some(authors) = string_list(info.get("authors")) {
        metadata.insert("authors".to_string(), authors);
    }
    if let Some(categories) = string_list(info.get("categories")) {
        metadata.insert("categories".to_string(), categories);
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
        provider: GoogleBooksProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(title.clone()),
        title,
        brief: description,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

/// Prefer ISBN-13, falling back to ISBN-10, from `industryIdentifiers`.
fn google_books_isbn(info: &Value) -> Option<String> {
    let identifiers = info.get("industryIdentifiers").and_then(Value::as_array)?;
    let by_type = |wanted: &str| {
        identifiers.iter().find_map(|entry| {
            (entry.get("type").and_then(Value::as_str) == Some(wanted))
                .then(|| entry.get("identifier").and_then(Value::as_str))
                .flatten()
                .map(str::to_string)
        })
    };
    by_type("ISBN_13").or_else(|| by_type("ISBN_10"))
}

#[cfg(test)]
mod tests {
    use super::{google_books_candidate, google_books_volume_id};
    use serde_json::json;

    #[test]
    fn candidate_surfaces_book_metadata() {
        let candidate = google_books_candidate(&json!({
            "id": "zyTCAlFPjgYC",
            "volumeInfo": {
                "title": "The Rust Programming Language",
                "subtitle": "2nd Edition",
                "authors": ["Steve Klabnik", "Carol Nichols"],
                "publisher": "No Starch Press",
                "publishedDate": "2023-02-28",
                "pageCount": 560,
                "categories": ["Computers"],
                "language": "en",
                "description": "A <b>great</b> book.<br>Read it.",
                "industryIdentifiers": [
                    { "type": "ISBN_10", "identifier": "1718503105" },
                    { "type": "ISBN_13", "identifier": "9781718503106" }
                ],
                "imageLinks": { "thumbnail": "http://example.com/c.jpg" }
            }
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "zyTCAlFPjgYC");
        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("subtitle"), Some(&json!("2nd Edition")));
        assert_eq!(
            metadata.get("authors"),
            Some(&json!(["Steve Klabnik", "Carol Nichols"]))
        );
        assert_eq!(metadata.get("pages"), Some(&json!(560)));
        assert_eq!(metadata.get("isbn"), Some(&json!("9781718503106")));
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://example.com/c.jpg")
        );
        assert_eq!(
            metadata.get("description"),
            Some(&json!("A great book.\nRead it."))
        );
    }

    #[test]
    fn volume_id_parses_known_url_shapes() {
        assert_eq!(
            google_books_volume_id("https://books.google.com/books?id=zyTCAlFPjgYC&hl=en"),
            Some("zyTCAlFPjgYC".to_string())
        );
        assert_eq!(
            google_books_volume_id("https://www.google.com/books/edition/Title/zyTCAlFPjgYC"),
            Some("zyTCAlFPjgYC".to_string())
        );
        assert_eq!(
            google_books_volume_id("the rust programming language"),
            None
        );
    }
}
