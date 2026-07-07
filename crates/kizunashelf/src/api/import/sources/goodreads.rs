//! Goodreads CSV import. Goodreads exports carry an ISBN13 but no id for any of
//! our book providers, so each row is resolved by ISBN to an Open Library edition
//! at fetch time (keyless). Rows with no ISBN or no Open Library match go to the
//! review queue. Candidates are minimal and detail-fetched from Open Library at
//! commit.

use super::super::csv_util::{field, parse_csv, CsvRow};
use super::super::model::{ImportItem, ImportUserData, ProviderRef};
use super::ImportSource;
use crate::api::error::ApiError;
use crate::api::external::USER_AGENT;
use crate::api::state::AppState;
use crate::contract::{ExternalCandidate, ImportInput, ImportInputKind};
use crate::types::CanonicalStatus;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(in crate::api::import) struct GoodreadsSource;

impl ImportSource for GoodreadsSource {
    const ID: &'static str = "goodreads";
    const LABEL: &'static str = "Goodreads";
    const INPUT: ImportInputKind = ImportInputKind::Csv;
    const INPUT_LABEL: &'static str = "Goodreads CSV export";

    fn providers() -> &'static [&'static str] {
        &["openlibrary"]
    }

    async fn fetch(state: &AppState, input: &ImportInput) -> Result<Vec<ImportItem>, ApiError> {
        let text = input
            .csv_text
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| ApiError::bad_request("A Goodreads CSV export is required"))?;
        let rows = parse_csv(text)?;

        let mut items = Vec::new();
        for row in &rows {
            let Some(parsed) = parse_row(row) else {
                continue;
            };
            let resolved = match &parsed.isbn {
                Some(isbn) => resolve_isbn(state, isbn).await?,
                None => None,
            };
            items.push(build_item(parsed, resolved));
        }
        Ok(items)
    }
}

struct ParsedRow {
    isbn: Option<String>,
    title: String,
    user: ImportUserData,
}

fn parse_row(row: &CsvRow) -> Option<ParsedRow> {
    let title = field(row, "Title")?.to_string();
    let isbn = field(row, "ISBN13")
        .or_else(|| field(row, "ISBN"))
        .and_then(clean_isbn);
    let user = ImportUserData {
        status: field(row, "Exclusive Shelf").and_then(shelf_status),
        // Goodreads rates 0–5; scale to 0–10.
        score10: field(row, "My Rating")
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|rating| *rating > 0.0)
            .map(|rating| rating * 2.0),
        watched_count: None,
        started: None,
        completed: field(row, "Date Read").and_then(goodreads_date),
        notes: field(row, "My Review")
            .map(str::trim)
            .filter(|review| !review.is_empty())
            .map(str::to_string),
    };
    Some(ParsedRow { isbn, title, user })
}

fn build_item(parsed: ParsedRow, resolved: Option<(String, String)>) -> ImportItem {
    let mut titles = BTreeMap::new();
    titles.insert("en".to_string(), parsed.title.clone());

    let (refs, candidate) = match resolved {
        Some((olid, url)) => {
            let candidate = ExternalCandidate {
                provider: "openlibrary".to_string(),
                source_id: olid.clone(),
                url: url.clone(),
                title: parsed.title.clone(),
                original_title: None,
                brief: None,
                cover_url: None,
                titles: titles.clone(),
                metadata: Map::new(),
            };
            (
                vec![ProviderRef {
                    provider: "openlibrary".to_string(),
                    id: olid,
                    url,
                }],
                Some(candidate),
            )
        }
        None => (Vec::new(), None),
    };

    ImportItem {
        refs,
        bucket: "book".to_string(),
        title: parsed.title,
        titles,
        candidate,
        user: parsed.user,
    }
}

/// Resolves an ISBN to an Open Library edition `(olid, url)` via the keyless
/// `/isbn/{isbn}.json` endpoint. `Ok(None)` on a 404 (unknown ISBN); a transport
/// or server error propagates so the user sees Open Library was unreachable
/// rather than a silent all-review result.
async fn resolve_isbn(state: &AppState, isbn: &str) -> Result<Option<(String, String)>, ApiError> {
    let response = state
        .http_client()
        .get(format!("https://openlibrary.org/isbn/{isbn}.json"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(|_| ApiError::bad_gateway("The Open Library request failed"))?;
    // The shared client doesn't follow redirects; `/isbn` may 3xx to `/books/OLID`.
    if response.status().is_redirection() {
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim_end_matches(".json").to_string());
        return Ok(location.and_then(|location| olid_url(&location)));
    }
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let value: Value = response
        .error_for_status()
        .map_err(|_| ApiError::bad_gateway("The Open Library request failed"))?
        .json()
        .await
        .map_err(|_| ApiError::bad_gateway("Invalid Open Library response"))?;
    Ok(value.get("key").and_then(Value::as_str).and_then(olid_url))
}

/// `(olid, absolute url)` from an Open Library edition key like `/books/OL123M`.
fn olid_url(key: &str) -> Option<(String, String)> {
    let olid = key.trim_end_matches('/').rsplit('/').next()?;
    (!olid.is_empty()).then(|| {
        (
            olid.to_string(),
            format!("https://openlibrary.org/books/{olid}"),
        )
    })
}

/// Keeps only ISBN characters (digits and a trailing `X`); returns a 10- or
/// 13-length ISBN. Goodreads wraps the field as `="9780..."`.
fn clean_isbn(value: &str) -> Option<String> {
    let cleaned: String = value
        .chars()
        .filter(|character| character.is_ascii_digit() || matches!(character, 'X' | 'x'))
        .collect();
    matches!(cleaned.len(), 10 | 13).then(|| cleaned.to_uppercase())
}

fn shelf_status(shelf: &str) -> Option<CanonicalStatus> {
    match shelf.trim() {
        "read" => Some(CanonicalStatus::Completed),
        "currently-reading" => Some(CanonicalStatus::Ongoing),
        "to-read" => Some(CanonicalStatus::Planning),
        _ => None,
    }
}

/// Goodreads dates are `YYYY/MM/DD`; normalize to `YYYY-MM-DD`.
fn goodreads_date(value: &str) -> Option<String> {
    let mut parts = value.split('/');
    let year = parts.next()?;
    let month = parts.next()?;
    let day = parts.next()?;
    if year.len() == 4 && month.len() <= 2 && day.len() <= 2 {
        Some(format!(
            "{year}-{:02}-{:02}",
            month.parse::<u8>().ok()?,
            day.parse::<u8>().ok()?
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(csv: &str) -> CsvRow {
        parse_csv(csv)
            .unwrap_or_else(|_| panic!("valid csv"))
            .into_iter()
            .next()
            .expect("one row")
    }

    #[test]
    fn parses_a_read_book_row() {
        let parsed = parse_row(&row(
            "Title,ISBN13,My Rating,Exclusive Shelf,Date Read,My Review\nDune,=\"9780441013593\",4,read,2021/03/09,Loved it\n",
        ))
        .expect("parsed");
        assert_eq!(parsed.isbn.as_deref(), Some("9780441013593"));
        assert_eq!(parsed.title, "Dune");
        assert_eq!(parsed.user.status, Some(CanonicalStatus::Completed));
        assert_eq!(parsed.user.score10, Some(8.0));
        assert_eq!(parsed.user.completed.as_deref(), Some("2021-03-09"));
        assert_eq!(parsed.user.notes.as_deref(), Some("Loved it"));

        let item = build_item(
            parsed,
            Some((
                "OL123M".to_string(),
                "https://openlibrary.org/books/OL123M".to_string(),
            )),
        );
        assert_eq!(item.bucket, "book");
        assert_eq!(item.refs[0].url, "https://openlibrary.org/books/OL123M");
    }

    #[test]
    fn a_row_without_isbn_has_no_ref() {
        let parsed =
            parse_row(&row("Title,ISBN13,Exclusive Shelf\nRare Book,,to-read\n")).expect("parsed");
        assert_eq!(parsed.isbn, None);
        let item = build_item(parsed, None);
        assert!(item.refs.is_empty());
        assert_eq!(item.user.status, Some(CanonicalStatus::Planning));
    }

    #[test]
    fn olid_url_extracts_edition_id() {
        assert_eq!(
            olid_url("/books/OL7353617M"),
            Some((
                "OL7353617M".to_string(),
                "https://openlibrary.org/books/OL7353617M".to_string()
            ))
        );
    }
}
