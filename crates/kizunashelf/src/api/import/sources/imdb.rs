//! IMDb CSV import. IMDb exports (ratings or a list) carry an IMDb id (`Const`)
//! but no TMDB id, so each row is resolved to a TMDB ref via `/find` at fetch
//! time (requires the TMDB API key). Rows TMDB can't match, and unsupported
//! title types (episodes, games), go to the review queue. Candidates are minimal
//! and detail-fetched from TMDB at commit.

use super::super::csv_util::{field, parse_csv, CsvRow};
use super::super::model::{ImportItem, ImportUserData, ProviderRef};
use super::ImportSource;
use crate::api::error::ApiError;
use crate::api::external::tmdb_find_imdb;
use crate::api::state::AppState;
use crate::contract::{ExternalCandidate, ImportInput, ImportInputKind};
use crate::types::CanonicalStatus;
use serde_json::Map;
use std::collections::BTreeMap;

pub(in crate::api::import) struct ImdbSource;

impl ImportSource for ImdbSource {
    const ID: &'static str = "imdb";
    const LABEL: &'static str = "IMDb";
    const INPUT: ImportInputKind = ImportInputKind::Csv;
    const INPUT_LABEL: &'static str = "IMDb CSV export";

    fn providers() -> &'static [&'static str] {
        &["tmdb"]
    }

    fn available(state: &AppState) -> bool {
        state
            .secret_store()
            .get(crate::secrets::SECRET_TMDB_API_KEY)
            .filter(|value| !value.is_empty())
            .is_some()
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        (!Self::available(state)).then(|| "Set the TMDB API key".to_string())
    }

    async fn fetch(state: &AppState, input: &ImportInput) -> Result<Vec<ImportItem>, ApiError> {
        let text = input
            .csv_text
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| ApiError::bad_request("An IMDb CSV export is required"))?;
        let rows = parse_csv(text)?;

        let mut items = Vec::new();
        for row in &rows {
            let Some(parsed) = parse_row(row) else {
                continue;
            };
            // Resolve the IMDb id to a TMDB ref; a miss still surfaces in review.
            let resolved = tmdb_find_imdb(state, &parsed.imdb_id).await?;
            items.push(build_item(parsed, resolved));
        }
        Ok(items)
    }
}

struct ParsedRow {
    imdb_id: String,
    title: String,
    /// Bucket hint from the IMDb title type, used for review items when TMDB
    /// can't be reached; the authoritative bucket comes from the `/find` result.
    hint: &'static str,
    rating: Option<f64>,
    date: Option<String>,
}

/// Parses one IMDb CSV row, or `None` for a row with no `Const` or an
/// unsupported title type (episode/game/etc. we can't import as an entity).
fn parse_row(row: &CsvRow) -> Option<ParsedRow> {
    let imdb_id = normalize_imdb_id(field(row, "Const")?);
    let title = field(row, "Title").unwrap_or("Untitled").to_string();
    let title_type = field(row, "Title Type").unwrap_or("");
    let hint = bucket_hint(title_type)?;
    let rating = field(row, "Your Rating").and_then(|value| value.parse::<f64>().ok());
    // Ratings exports use `Date Rated`; list exports use `Created`/`Modified`.
    let date = field(row, "Date Rated")
        .or_else(|| field(row, "Created"))
        .or_else(|| field(row, "Modified"))
        .and_then(iso_date);
    Some(ParsedRow {
        imdb_id,
        title,
        hint,
        rating,
        date,
    })
}

fn build_item(parsed: ParsedRow, resolved: Option<(String, &'static str)>) -> ImportItem {
    // A rating means the user watched it (completed); otherwise it's a watchlist
    // entry (planning).
    let completed_status = parsed.rating.is_some();
    let user = ImportUserData {
        status: Some(if completed_status {
            CanonicalStatus::Completed
        } else {
            CanonicalStatus::Planning
        }),
        score10: parsed.rating.filter(|rating| *rating > 0.0),
        completed: completed_status.then_some(parsed.date).flatten(),
        ..Default::default()
    };

    let mut titles = BTreeMap::new();
    titles.insert("en".to_string(), parsed.title.clone());

    let (refs, candidate, bucket) = match resolved {
        Some((tmdb_id, media_type)) => {
            let url = format!("https://www.themoviedb.org/{media_type}/{tmdb_id}");
            let candidate = ExternalCandidate {
                provider: "tmdb".to_string(),
                source_id: tmdb_id.clone(),
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
                    provider: "tmdb".to_string(),
                    id: tmdb_id,
                    url,
                }],
                Some(candidate),
                media_type.to_string(),
            )
        }
        None => (Vec::new(), None, parsed.hint.to_string()),
    };

    ImportItem {
        refs,
        bucket,
        title: parsed.title,
        titles,
        candidate,
        user,
    }
}

/// A TMDB bucket from an IMDb title type, or `None` for types we don't import.
fn bucket_hint(title_type: &str) -> Option<&'static str> {
    match title_type {
        "Movie" | "Short" | "TV Movie" | "TV Special" | "Video" => Some("movie"),
        "TV Series" | "TV Mini Series" | "TV Mini-Series" => Some("tv"),
        // TV Episode / TV Short / Video Game / Music Video / Podcast* — skip.
        _ => None,
    }
}

/// Ensures a `tt` prefix on an IMDb id (`1375666` → `tt1375666`).
fn normalize_imdb_id(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("tt") {
        trimmed.to_string()
    } else {
        format!("tt{trimmed}")
    }
}

/// An `YYYY-MM-DD` date from an IMDb date cell (already ISO), or `None`.
fn iso_date(value: &str) -> Option<String> {
    let date = value.split(['T', ' ']).next().unwrap_or_default();
    (date.len() == 10 && date.as_bytes()[4] == b'-').then(|| date.to_string())
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
    fn parses_a_rated_movie_and_builds_a_tmdb_ref() {
        let parsed = parse_row(&row(
            "Const,Your Rating,Date Rated,Title,Title Type\ntt1375666,9,2020-01-01,Inception,Movie\n",
        ))
        .expect("parsed");
        assert_eq!(parsed.imdb_id, "tt1375666");
        assert_eq!(parsed.hint, "movie");
        assert_eq!(parsed.rating, Some(9.0));

        let item = build_item(parsed, Some(("27205".to_string(), "movie")));
        assert_eq!(item.refs[0].url, "https://www.themoviedb.org/movie/27205");
        assert_eq!(item.bucket, "movie");
        assert_eq!(item.user.status, Some(CanonicalStatus::Completed));
        assert_eq!(item.user.score10, Some(9.0));
        assert_eq!(item.user.completed.as_deref(), Some("2020-01-01"));
    }

    #[test]
    fn an_unresolved_row_goes_to_review_with_a_bucket_hint() {
        let parsed = parse_row(&row(
            "Const,Title,Title Type\ntt999,Obscure Show,TV Series\n",
        ))
        .expect("parsed");
        assert_eq!(parsed.rating, None);
        let item = build_item(parsed, None);
        assert!(item.refs.is_empty());
        assert_eq!(item.bucket, "tv");
        assert_eq!(item.user.status, Some(CanonicalStatus::Planning));
    }

    #[test]
    fn skips_unsupported_title_types() {
        assert!(parse_row(&row("Const,Title,Title Type\ntt1,Ep,TV Episode\n")).is_none());
        assert!(parse_row(&row("Const,Title,Title Type\ntt2,Game,Video Game\n")).is_none());
    }
}
