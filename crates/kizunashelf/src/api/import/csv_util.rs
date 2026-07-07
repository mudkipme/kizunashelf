//! CSV parsing for file-export import sources. A thin wrapper over the `csv`
//! crate that yields header-keyed rows; export files (Goodreads, IMDB) carry
//! quoted multi-line fields, so hand-rolling is not an option.

use crate::api::error::ApiError;
use std::collections::HashMap;

/// One header-keyed row.
pub(super) type CsvRow = HashMap<String, String>;

/// Parses CSV text (with a header row) into header-keyed rows. Trims header
/// names. Flexible on row length (a short row just omits trailing columns).
pub(super) fn parse_csv(text: &str) -> Result<Vec<CsvRow>, ApiError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .has_headers(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|error| ApiError::bad_request(&format!("Invalid CSV header: {error}")))?
        .iter()
        .map(|header| header.trim().to_string())
        .collect();
    if headers.is_empty() {
        return Err(ApiError::bad_request("CSV has no header row"));
    }
    let mut rows = Vec::new();
    for record in reader.records() {
        let record =
            record.map_err(|error| ApiError::bad_request(&format!("Invalid CSV row: {error}")))?;
        let mut row = HashMap::new();
        for (index, value) in record.iter().enumerate() {
            if let Some(header) = headers.get(index) {
                row.insert(header.clone(), value.to_string());
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

/// A trimmed, non-empty value for `name` (exact match first, then
/// case-insensitive). `None` for a missing or blank cell.
pub(super) fn field<'a>(row: &'a CsvRow, name: &str) -> Option<&'a str> {
    let exact = row.get(name).map(String::as_str);
    let value = exact.or_else(|| {
        row.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    })?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}
