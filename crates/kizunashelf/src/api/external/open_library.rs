use super::{
    external_client, field_option, insert_str, normalize_isbn, provider_error, string_list,
    strip_html, type_option, ExternalProvider, ProviderResponseExt, ProviderSearchConfig,
    USER_AGENT,
};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct OpenLibraryProvider;

impl ExternalProvider for OpenLibraryProvider {
    const ID: &'static str = "openlibrary";
    const LABEL: &'static str = "Open Library";

    fn recognizes_url(q: &str) -> bool {
        open_library_olid(q).is_some()
    }

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        open_library_supported(provider_config)
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
        _state: &super::AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        search_open_library(q, page, page_size, provider_config).await
    }
}

pub(super) fn open_library_supported(provider_config: &ProviderSearchConfig) -> bool {
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
        field_option("published_date", "Published date"),
        field_option("first_published", "First published"),
        field_option("pages", "Pages"),
        field_option("isbn", "ISBN"),
        field_option("language", "Language"),
        field_option("physical_format", "Physical format"),
        field_option("description", "Description"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("authors", "Authors"),
        field_option("publishers", "Publishers"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("book", "Book")]
}

async fn search_open_library(
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !open_library_supported(provider_config) {
        return Ok(Vec::new());
    }
    let client = external_client();
    // A pasted Open Library URL or bare OLID resolves to a single record.
    if let Some(olid) = open_library_olid(q) {
        return resolve_open_library(client, &olid).await;
    }
    let offset = (page - 1) * page_size;
    let response = client
        .get("https://openlibrary.org/search.json")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[
            ("q", q),
            ("limit", &page_size.to_string()),
            ("offset", &offset.to_string()),
            (
                "fields",
                "key,title,subtitle,author_name,first_publish_year,cover_edition_key,cover_i,editions,editions.key,editions.title",
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
    let docs = response
        .get("docs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(docs.iter().filter_map(open_library_search_doc).collect())
}

/// Resolves an edition (`OL…M`) or a work (`OL…W`) by id.
async fn resolve_open_library(
    client: &reqwest::Client,
    olid: &str,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    let is_work = olid.ends_with('W') || olid.ends_with('w');
    let path = if is_work { "works" } else { "books" };
    let value = client
        .get(format!("https://openlibrary.org/{path}/{olid}.json"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status_body()
        .await?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    let authors = fetch_author_names(client, &value, is_work).await;
    Ok(open_library_record(olid, &value, is_work, authors)
        .into_iter()
        .collect())
}

/// Open Library returns authors as `/authors/OL…A` references; resolve a bounded
/// number of them to display names. Bounded so a book with a huge contributor
/// list doesn't fan out into dozens of requests.
async fn fetch_author_names(client: &reqwest::Client, value: &Value, is_work: bool) -> Vec<String> {
    let keys: Vec<String> = value
        .get("authors")
        .and_then(Value::as_array)
        .map(|authors| {
            authors
                .iter()
                .filter_map(|author| {
                    // Editions: `{ "key": "/authors/OL..A" }`. Works:
                    // `{ "author": { "key": "/authors/OL..A" } }`.
                    let key = if is_work {
                        author.get("author").and_then(|inner| inner.get("key"))
                    } else {
                        author.get("key")
                    };
                    key.and_then(Value::as_str).map(str::to_string)
                })
                .take(5)
                .collect()
        })
        .unwrap_or_default();
    let mut names = Vec::new();
    for key in keys {
        let url = format!("https://openlibrary.org{key}.json");
        let name = client
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .ok()
            .and_then(|response| response.error_for_status().ok());
        if let Some(response) = name {
            if let Ok(author) = response.json::<Value>().await {
                if let Some(name) = author
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

fn open_library_record(
    olid: &str,
    value: &Value,
    is_work: bool,
    authors: Vec<String>,
) -> Option<ExternalCandidate> {
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let path = if is_work { "works" } else { "books" };
    let url = format!("https://openlibrary.org/{path}/{olid}");
    // Editions have an OLID-addressable cover; works expose a numeric `covers`
    // array instead (`b/id/<id>-L.jpg`).
    let cover_url = if is_work {
        value
            .get("covers")
            .and_then(Value::as_array)
            .and_then(|covers| covers.first())
            .and_then(Value::as_i64)
            .filter(|cover| *cover > 0)
            .map(|cover| format!("https://covers.openlibrary.org/b/id/{cover}-L.jpg"))
    } else {
        Some(format!(
            "https://covers.openlibrary.org/b/olid/{olid}-L.jpg"
        ))
    };

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    insert_str(&mut metadata, "subtitle", value.get("subtitle"));
    insert_str(
        &mut metadata,
        "physical_format",
        value.get("physical_format"),
    );
    // Surface up to five publishers, as a list.
    if let Some(publishers) = value.get("publishers").and_then(Value::as_array) {
        let publishers: Vec<Value> = publishers
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .take(5)
            .map(|value| Value::String(value.to_string()))
            .collect();
        if !publishers.is_empty() {
            metadata.insert("publishers".to_string(), Value::Array(publishers));
        }
    }
    insert_str(&mut metadata, "published_date", value.get("publish_date"));
    insert_str(
        &mut metadata,
        "first_published",
        value.get("first_publish_date"),
    );
    if let Some(pages) = value.get("number_of_pages").and_then(Value::as_i64) {
        if pages > 0 {
            metadata.insert("pages".to_string(), Value::Number(pages.into()));
        }
    }
    if let Some(isbn) = value
        .get("isbn_13")
        .and_then(Value::as_array)
        .and_then(|isbns| isbns.first())
        .or_else(|| {
            value
                .get("isbn_10")
                .and_then(Value::as_array)
                .and_then(|isbns| isbns.first())
        })
        .and_then(Value::as_str)
    {
        // Convert a lone ISBN-10 to ISBN-13.
        metadata.insert("isbn".to_string(), Value::String(normalize_isbn(isbn)));
    }
    // Editions carry `languages: [{ key: "/languages/eng" }]`; surface the first.
    if let Some(language) = value
        .get("languages")
        .and_then(Value::as_array)
        .and_then(|languages| languages.first())
        .and_then(|language| language.get("key"))
        .and_then(Value::as_str)
        .and_then(|key| key.rsplit('/').next())
        .filter(|value| !value.is_empty())
    {
        metadata.insert("language".to_string(), Value::String(language.to_string()));
    }
    if !authors.is_empty() {
        metadata.insert(
            "authors".to_string(),
            Value::Array(authors.into_iter().map(Value::String).collect()),
        );
    }
    // Prefer `notes`, then `description`.
    let description = open_library_text(value.get("notes"))
        .or_else(|| open_library_text(value.get("description")))
        .map(|text| strip_html(&text))
        .filter(|value| !value.is_empty());
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
        provider: OpenLibraryProvider::ID.to_string(),
        source_id: olid.to_string(),
        url,
        original_title: Some(title.clone()),
        title,
        brief: description,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

fn open_library_search_doc(doc: &Value) -> Option<ExternalCandidate> {
    // Prefer an edition (so the resulting reference resolves to a concrete book);
    // fall back to the work key.
    let edition_key = doc
        .get("editions")
        .and_then(|editions| editions.get("docs"))
        .and_then(Value::as_array)
        .and_then(|docs| docs.first())
        .and_then(|edition| edition.get("key"))
        .and_then(Value::as_str)
        .and_then(|key| key.rsplit('/').next())
        .map(str::to_string);
    let cover_edition = doc
        .get("cover_edition_key")
        .and_then(Value::as_str)
        .map(str::to_string);
    let olid = edition_key.clone().or_else(|| cover_edition.clone());
    let (source_id, url, cover_url) = match &olid {
        Some(olid) => (
            olid.clone(),
            format!("https://openlibrary.org/books/{olid}"),
            Some(format!(
                "https://covers.openlibrary.org/b/olid/{olid}-M.jpg"
            )),
        ),
        None => {
            let key = doc.get("key").and_then(Value::as_str)?;
            let id = key.rsplit('/').next().unwrap_or(key).to_string();
            let cover = doc
                .get("cover_i")
                .and_then(Value::as_i64)
                .map(|cover| format!("https://covers.openlibrary.org/b/id/{cover}-M.jpg"));
            (id, format!("https://openlibrary.org{key}"), cover)
        }
    };
    let title = doc
        .get("editions")
        .and_then(|editions| editions.get("docs"))
        .and_then(Value::as_array)
        .and_then(|docs| docs.first())
        .and_then(|edition| edition.get("title"))
        .and_then(Value::as_str)
        .or_else(|| doc.get("title").and_then(Value::as_str))
        .filter(|value| !value.is_empty())?
        .to_string();

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    insert_str(&mut metadata, "subtitle", doc.get("subtitle"));
    if let Some(authors) = string_list(doc.get("author_name")) {
        metadata.insert("authors".to_string(), authors);
    }
    if let Some(year) = doc.get("first_publish_year").and_then(Value::as_i64) {
        metadata.insert(
            "first_published".to_string(),
            Value::String(year.to_string()),
        );
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: OpenLibraryProvider::ID.to_string(),
        source_id,
        url,
        original_title: Some(title.clone()),
        title,
        brief: None,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

/// Extracts an Open Library OLID from a pasted URL or a bare `OL…M` / `OL…W` id.
fn open_library_olid(q: &str) -> Option<String> {
    let trimmed = q.trim().trim_end_matches('/');
    if let Some((_, rest)) = trimmed
        .split_once("/books/")
        .or_else(|| trimmed.split_once("/works/"))
    {
        let id = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .trim();
        return is_olid(id).then(|| id.to_uppercase());
    }
    is_olid(trimmed).then(|| trimmed.to_uppercase())
}

/// `OL` followed by digits and a trailing `M` (edition) or `W` (work).
fn is_olid(value: &str) -> bool {
    let upper = value.to_uppercase();
    let Some(middle) = upper
        .strip_prefix("OL")
        .and_then(|rest| rest.strip_suffix(['M', 'W']))
    else {
        return false;
    };
    !middle.is_empty() && middle.chars().all(|character| character.is_ascii_digit())
}

/// Open Library descriptions are either a plain string or `{ "value": "…" }`.
fn open_library_text(value: Option<&Value>) -> Option<String> {
    let value = value?;
    value
        .as_str()
        .or_else(|| value.get("value").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{is_olid, open_library_olid, open_library_search_doc};
    use serde_json::json;

    #[test]
    fn search_doc_uses_first_edition() {
        let candidate = open_library_search_doc(&json!({
            "key": "/works/OL45804W",
            "title": "Fantastic Mr Fox",
            "author_name": ["Roald Dahl"],
            "first_publish_year": 1970,
            "editions": { "docs": [{ "key": "/books/OL7353617M", "title": "Fantastic Mr. Fox" }] }
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "OL7353617M");
        assert_eq!(candidate.title, "Fantastic Mr. Fox");
        assert_eq!(
            candidate.metadata.get("authors"),
            Some(&json!(["Roald Dahl"]))
        );
        assert_eq!(
            candidate.metadata.get("first_published"),
            Some(&json!("1970"))
        );
    }

    #[test]
    fn olid_detection() {
        assert!(is_olid("OL7353617M"));
        assert!(is_olid("OL45804W"));
        assert!(!is_olid("OL45804"));
        assert_eq!(
            open_library_olid("https://openlibrary.org/books/OL7353617M/Fantastic"),
            Some("OL7353617M".to_string())
        );
        assert_eq!(open_library_olid("fantastic mr fox"), None);
    }
}
