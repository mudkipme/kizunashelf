use super::{
    external_client, field_option, non_empty_string_or_integer, normalize_isbn, provider_error,
    type_option, CredentialSpec, ExternalProvider, ProviderResponseExt, ProviderSearchConfig,
    USER_AGENT,
};
use crate::api::state::AppState;
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use crate::secrets::SECRET_HARDCOVER_API_KEY;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(super) struct HardcoverProvider;

const ENDPOINT: &str = "https://api.hardcover.app/v1/graphql";
/// The book field selection shared by the id and slug detail queries.
const BOOK_SELECTION: &str = r#"
  id
  title
  slug
  cached_image(path: "url")
  description
  cached_tags(path: "Genre")
  rating
  ratings_count
  pages
  release_date
  cached_contributors(path: "[0]['author']['name']")
  default_cover_edition {
    edition_format
    isbn_13
    isbn_10
    release_date
    publisher { name }
  }
"#;

impl ExternalProvider for HardcoverProvider {
    const ID: &'static str = "hardcover";
    const LABEL: &'static str = "Hardcover";

    fn recognizes_url(q: &str) -> bool {
        hardcover_ref(q).is_some()
    }

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        book_supported(provider_config)
    }

    fn credentials() -> &'static [CredentialSpec] {
        &[CredentialSpec {
            key: SECRET_HARDCOVER_API_KEY,
            label: "Hardcover API Token",
            secret: true,
            required: true,
        }]
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        hardcover_token(state)
            .is_none()
            .then(|| "Set the Hardcover API token".to_string())
    }

    fn available(state: &AppState) -> bool {
        hardcover_token(state).is_some()
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
        search_hardcover(state, q, page, page_size, provider_config).await
    }
}

fn hardcover_token(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_HARDCOVER_API_KEY)
        .filter(|value| !value.is_empty())
}

pub(super) fn book_supported(provider_config: &ProviderSearchConfig) -> bool {
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
        field_option("format", "Format"),
        field_option("pages", "Pages"),
        field_option("publish_date", "Publish date"),
        field_option("publisher", "Publisher"),
        field_option("isbn", "ISBN"),
        field_option("score", "Score"),
        field_option("synopsis", "Synopsis"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("authors", "Authors"),
        field_option("genres", "Genres"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("book", "Book")]
}

async fn search_hardcover(
    state: &AppState,
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !book_supported(provider_config) {
        return Ok(Vec::new());
    }
    let Some(token) = hardcover_token(state) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    // A pasted Hardcover URL (by slug) or bare numeric id resolves a single book.
    if let Some(reference) = hardcover_ref(q) {
        let (query, variables) = match &reference {
            HardcoverRef::Id(id) => (
                format!("query($id: Int!) {{ books_by_pk(id: $id) {{ {BOOK_SELECTION} }} }}"),
                json!({ "id": id }),
            ),
            HardcoverRef::Slug(slug) => (
                format!(
                    "query($slug: String!) {{ books(where: {{ slug: {{ _eq: $slug }} }}, limit: 1) {{ {BOOK_SELECTION} }} }}"
                ),
                json!({ "slug": slug }),
            ),
        };
        let value = hardcover_post(client, &token, &query, variables).await?;
        let book = match &reference {
            HardcoverRef::Id(_) => value.pointer("/data/books_by_pk").cloned(),
            HardcoverRef::Slug(_) => value
                .pointer("/data/books/0")
                .filter(|book| !book.is_null())
                .cloned(),
        };
        return Ok(book
            .as_ref()
            .filter(|book| !book.is_null())
            .and_then(hardcover_book)
            .into_iter()
            .collect());
    }
    let query = r#"
      query SearchBooks($query: String!, $per_page: Int!, $page: Int!) {
        search(query: $query, query_type: "Book", per_page: $per_page, page: $page) {
          results
        }
      }
    "#;
    let value = hardcover_post(
        client,
        &token,
        query,
        json!({ "query": q, "per_page": page_size, "page": page }),
    )
    .await?;
    let hits = value
        .pointer("/data/search/results/hits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(hits
        .iter()
        .filter_map(|hit| hardcover_search_hit(hit.get("document")?))
        .collect())
}

async fn hardcover_post(
    client: &reqwest::Client,
    token: &str,
    query: &str,
    variables: Value,
) -> Result<Value, ApiError> {
    client
        .post(ENDPOINT)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(
            reqwest::header::AUTHORIZATION,
            hardcover_authorization(token),
        )
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status_body()
        .await?
        .json::<Value>()
        .await
        .map_err(provider_error)
}

/// Hardcover expects `Authorization: Bearer <token>`. Accept a token that
/// already carries the `Bearer ` prefix so users can paste either form.
fn hardcover_authorization(token: &str) -> String {
    if token.to_ascii_lowercase().starts_with("bearer ") {
        token.to_string()
    } else {
        format!("Bearer {token}")
    }
}

enum HardcoverRef {
    Id(i64),
    Slug(String),
}

/// Resolves a bare numeric id or a `hardcover.app/books/<slug>` URL.
fn hardcover_ref(q: &str) -> Option<HardcoverRef> {
    let trimmed = q.trim().trim_end_matches('/');
    if let Ok(id) = trimmed.parse::<i64>() {
        return Some(HardcoverRef::Id(id));
    }
    let (_, rest) = trimmed.split_once("hardcover.app/books/")?;
    let segment = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if segment.is_empty() {
        return None;
    }
    // A numeric path segment is a book id; otherwise it's a slug.
    match segment.parse::<i64>() {
        Ok(id) => Some(HardcoverRef::Id(id)),
        Err(_) => Some(HardcoverRef::Slug(segment.to_string())),
    }
}

fn hardcover_search_hit(document: &Value) -> Option<ExternalCandidate> {
    let id = document.get("id").and_then(non_empty_string_or_integer)?;
    let title = document
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let slug = document.get("slug").and_then(Value::as_str);
    let url = hardcover_url(slug, &id);
    let cover_url = document
        .get("image")
        .and_then(|image| image.get("url"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: HardcoverProvider::ID.to_string(),
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

fn hardcover_book(book: &Value) -> Option<ExternalCandidate> {
    let id = book.get("id").and_then(non_empty_string_or_integer)?;
    let title = book
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let slug = book.get("slug").and_then(Value::as_str);
    let url = hardcover_url(slug, &id);
    let cover_url = book
        .get("cached_image")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let synopsis = book
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let edition = book.get("default_cover_edition");

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(format) = edition
        .and_then(|edition| edition.get("edition_format"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("format".to_string(), Value::String(format.to_string()));
    }
    if let Some(pages) = book
        .get("pages")
        .and_then(Value::as_i64)
        .filter(|pages| *pages > 0)
    {
        metadata.insert("pages".to_string(), Value::Number(pages.into()));
    }
    // Edition release date is most specific; fall back to the work release date.
    if let Some(publish_date) = edition
        .and_then(|edition| edition.get("release_date"))
        .and_then(Value::as_str)
        .or_else(|| book.get("release_date").and_then(Value::as_str))
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "publish_date".to_string(),
            Value::String(publish_date.to_string()),
        );
    }
    if let Some(publisher) = edition
        .and_then(|edition| edition.get("publisher"))
        .and_then(|publisher| publisher.get("name"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "publisher".to_string(),
            Value::String(publisher.to_string()),
        );
    }
    if let Some(isbn) = edition.and_then(|edition| {
        edition
            .get("isbn_13")
            .and_then(Value::as_str)
            .or_else(|| edition.get("isbn_10").and_then(Value::as_str))
            .filter(|value| !value.is_empty())
    }) {
        metadata.insert("isbn".to_string(), Value::String(normalize_isbn(isbn)));
    }
    // Hardcover ratings are 0–5.
    if let Some(score) = book
        .get("rating")
        .and_then(Value::as_f64)
        .filter(|rating| *rating > 0.0)
    {
        metadata.insert(
            "score".to_string(),
            json!((score * 2.0 * 10.0).round() / 10.0),
        );
    }
    // `cached_contributors` resolves to the first author's name (a string).
    if let Some(author) = book
        .get("cached_contributors")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "authors".to_string(),
            Value::Array(vec![Value::String(author.to_string())]),
        );
    }
    if let Some(genres) = book.get("cached_tags").and_then(Value::as_array) {
        let genres: Vec<Value> = genres
            .iter()
            .filter_map(|tag| tag.get("tag").and_then(Value::as_str))
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(|tag| Value::String(tag.to_string()))
            .collect();
        if !genres.is_empty() {
            metadata.insert("genres".to_string(), Value::Array(genres));
        }
    }
    if let Some(synopsis) = &synopsis {
        metadata.insert("synopsis".to_string(), Value::String(synopsis.clone()));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: HardcoverProvider::ID.to_string(),
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

fn hardcover_url(slug: Option<&str>, id: &str) -> String {
    match slug.filter(|slug| !slug.is_empty()) {
        Some(slug) => format!("https://hardcover.app/books/{slug}"),
        None => format!("https://hardcover.app/books/{id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{hardcover_authorization, hardcover_book};
    use serde_json::json;

    #[test]
    fn book_surfaces_metadata() {
        let candidate = hardcover_book(&json!({
            "id": 12345,
            "title": "Project Hail Mary",
            "slug": "project-hail-mary",
            "cached_image": "https://img/phm.jpg",
            "description": "A lone astronaut.",
            "cached_tags": [{ "tag": "Science Fiction" }, { "tag": "Space" }],
            "rating": 4.5,
            "pages": 496,
            "release_date": "2021-05-04",
            "cached_contributors": "Andy Weir",
            "default_cover_edition": {
                "edition_format": "Hardcover",
                "isbn_13": "9780593135204",
                "release_date": "2021-05-04",
                "publisher": { "name": "Ballantine Books" }
            }
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "12345");
        assert_eq!(
            candidate.url,
            "https://hardcover.app/books/project-hail-mary"
        );
        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("format"), Some(&json!("Hardcover")));
        assert_eq!(metadata.get("pages"), Some(&json!(496)));
        assert_eq!(metadata.get("isbn"), Some(&json!("9780593135204")));
        assert_eq!(metadata.get("publisher"), Some(&json!("Ballantine Books")));
        // rating 4.5 → 9.0 on a 0–10 scale.
        assert_eq!(metadata.get("score"), Some(&json!(9.0)));
        assert_eq!(metadata.get("authors"), Some(&json!(["Andy Weir"])));
        assert_eq!(
            metadata.get("genres"),
            Some(&json!(["Science Fiction", "Space"]))
        );
    }

    #[test]
    fn authorization_adds_bearer_prefix_once() {
        assert_eq!(hardcover_authorization("abc123"), "Bearer abc123");
        assert_eq!(hardcover_authorization("Bearer abc123"), "Bearer abc123");
    }
}
