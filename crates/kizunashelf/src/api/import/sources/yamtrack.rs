//! Yamtrack CSV import. Yamtrack's export is self-describing: each row names its
//! `source` + `media_id`, so an item's provider is read straight off the row.
//!
//! Phase 1 maps the sources whose canonical URL we can construct for our
//! providers — `mal` → MyAnimeList and `tmdb` → TMDB. Rows for other sources
//! (`igdb`, `hardcover`, `mangaupdates`, `openlibrary`, `manual`) carry no ref
//! we can resolve yet and go to the review queue.

use super::super::csv_util::{field, parse_csv, CsvRow};
use super::super::model::{ImportItem, ImportUserData, ProviderRef};
use super::ImportSource;
use crate::api::error::ApiError;
use crate::api::state::AppState;
use crate::contract::{ExternalCandidate, ImportInput, ImportInputKind};
use crate::types::CanonicalStatus;
use serde_json::Map;
use std::collections::{BTreeMap, HashMap};

pub(in crate::api::import) struct YamtrackSource;

impl ImportSource for YamtrackSource {
    const ID: &'static str = "yamtrack";
    const LABEL: &'static str = "Yamtrack";
    const INPUT: ImportInputKind = ImportInputKind::Csv;
    const INPUT_LABEL: &'static str = "Yamtrack CSV export";

    fn providers() -> &'static [&'static str] {
        &["myanimelist", "tmdb"]
    }

    async fn fetch(_state: &AppState, input: &ImportInput) -> Result<Vec<ImportItem>, ApiError> {
        let text = input
            .csv_text
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| ApiError::bad_request("A Yamtrack CSV export is required"))?;
        Ok(build_items(&parse_csv(text)?))
    }
}

/// Turns parsed CSV rows into normalized items (pure; no network). Extracted from
/// `fetch` so it can be unit-tested without an `AppState`.
fn build_items(rows: &[CsvRow]) -> Vec<ImportItem> {
    // Per-episode rows (a season/episode number) are watch records for their
    // parent show; count them per media so the parent's watched_count is right.
    let mut watched_episodes: HashMap<(String, String), u32> = HashMap::new();
    for row in rows {
        if field(row, "episode_number").is_some() {
            if let (Some(source), Some(media_id)) = (field(row, "source"), field(row, "media_id")) {
                *watched_episodes
                    .entry((source.to_lowercase(), media_id.to_string()))
                    .or_default() += 1;
            }
        }
    }

    let mut items = Vec::new();
    for row in rows {
        // Only top-level rows become entities; season/episode rows folded above.
        if field(row, "season_number").is_some() || field(row, "episode_number").is_some() {
            continue;
        }
        let title = field(row, "title")
            .or_else(|| field(row, "media_id"))
            .unwrap_or("Untitled")
            .to_string();
        let media_type = field(row, "media_type").unwrap_or("").to_lowercase();
        let source = field(row, "source").unwrap_or("").to_lowercase();
        let media_id = field(row, "media_id");

        let reference: Vec<ProviderRef> = media_id
            .and_then(|id| provider_ref(&source, &media_type, id))
            .into_iter()
            .collect();

        let watched_count = media_id
            .and_then(|id| {
                watched_episodes
                    .get(&(source.clone(), id.to_string()))
                    .copied()
            })
            .or_else(|| field(row, "progress").and_then(|value| value.parse::<u32>().ok()))
            .filter(|count| *count > 0);

        let user = ImportUserData {
            status: field(row, "status").and_then(status_from),
            score10: field(row, "score")
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|score| *score > 0.0),
            watched_count,
            started: field(row, "start_date").map(str::to_string),
            completed: field(row, "end_date").map(str::to_string),
            notes: field(row, "notes").map(str::to_string),
        };

        // The export doesn't say what language the title is in — leave it
        // untagged rather than mislabeled `en`.
        let titles = BTreeMap::new();
        let candidate = reference.first().map(|reference| ExternalCandidate {
            needs_detail: false,
            provider: reference.provider.clone(),
            source_id: reference.id.clone(),
            url: reference.url.clone(),
            title: title.clone(),
            original_title: None,
            brief: None,
            cover_url: field(row, "image").map(str::to_string),
            titles: titles.clone(),
            metadata: Map::new(),
        });

        items.push(ImportItem {
            refs: reference,
            bucket: media_type,
            title,
            titles,
            candidate,
            user,
        });
    }
    items
}

/// Maps a Yamtrack `source` + `media_type` + id to a provider ref with a URL our
/// provider can parse. `None` for a source we don't resolve yet.
fn provider_ref(source: &str, media_type: &str, id: &str) -> Option<ProviderRef> {
    match source {
        "mal" => {
            let kind = if media_type == "manga" {
                "manga"
            } else {
                "anime"
            };
            Some(ProviderRef {
                provider: "myanimelist".to_string(),
                id: id.to_string(),
                url: format!("https://myanimelist.net/{kind}/{id}"),
            })
        }
        "tmdb" => {
            let kind = if media_type == "tv" { "tv" } else { "movie" };
            Some(ProviderRef {
                provider: "tmdb".to_string(),
                id: id.to_string(),
                url: format!("https://www.themoviedb.org/{kind}/{id}"),
            })
        }
        _ => None,
    }
}

fn status_from(value: &str) -> Option<CanonicalStatus> {
    match value.trim().to_lowercase().as_str() {
        "completed" => Some(CanonicalStatus::Completed),
        "in progress" => Some(CanonicalStatus::Ongoing),
        "planning" => Some(CanonicalStatus::Planning),
        "paused" => Some(CanonicalStatus::Paused),
        "dropped" => Some(CanonicalStatus::Dropped),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(csv: &str) -> Vec<ImportItem> {
        build_items(&parse_csv(csv).unwrap_or_else(|_| panic!("valid csv")))
    }

    #[test]
    fn maps_supported_sources_and_reviews_others() {
        let csv = "media_id,source,media_type,title,image,season_number,episode_number,score,progress,status,start_date,end_date,notes,progressed_at\n\
            1,mal,anime,Cowboy Bebop,http://img/1.jpg,,,9,26,Completed,2020-01-01,2020-02-01,Great,2020-02-01\n\
            27205,tmdb,movie,Inception,,,,8,1,Completed,,,,\n\
            99,igdb,game,Some Game,,,,7,0,Planning,,,,\n";
        let items = items(csv);
        assert_eq!(items.len(), 3);

        let bebop = &items[0];
        assert_eq!(bebop.refs.len(), 1);
        assert_eq!(bebop.refs[0].provider, "myanimelist");
        assert_eq!(bebop.refs[0].url, "https://myanimelist.net/anime/1");
        assert_eq!(bebop.user.status, Some(CanonicalStatus::Completed));
        assert_eq!(bebop.user.score10, Some(9.0));
        assert_eq!(bebop.user.watched_count, Some(26));
        assert_eq!(bebop.user.completed.as_deref(), Some("2020-02-01"));

        assert_eq!(items[1].refs[0].provider, "tmdb");
        assert_eq!(
            items[1].refs[0].url,
            "https://www.themoviedb.org/movie/27205"
        );

        // igdb is unsupported in phase 1 → no ref, goes to review at plan time.
        assert!(items[2].refs.is_empty());
        assert_eq!(items[2].bucket, "game");
    }

    #[test]
    fn folds_episode_rows_into_parent_watched_count() {
        let csv = "media_id,source,media_type,title,season_number,episode_number,status\n\
            42,tmdb,tv,Some Show,,,In progress\n\
            42,tmdb,season,Some Show,1,,In progress\n\
            42,tmdb,episode,Some Show,1,1,Completed\n\
            42,tmdb,episode,Some Show,1,2,Completed\n";
        let items = items(csv);
        // Only the top-level show row becomes an item.
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].bucket, "tv");
        assert_eq!(items[0].user.watched_count, Some(2));
    }
}
