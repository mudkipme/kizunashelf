use super::{
    external_client, field_option, provider_error, type_option, ExternalProvider,
    ProviderResponseExt, ProviderSearchConfig, USER_AGENT,
};
use crate::api::ApiError;
use crate::contract::{
    ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption,
    ProviderEpisodeGroup, ProviderEpisodeItem, ProviderEpisodes,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Albums via the keyless iTunes Search API — the same API the Apple Podcasts
/// provider uses (`entity=album` to search, `lookup?entity=song` for the track
/// list). No credentials, unlike the Spotify Web API it replaces.
pub(super) struct AppleMusicProvider;

impl ExternalProvider for AppleMusicProvider {
    const ID: &'static str = "applemusic";
    const LABEL: &'static str = "Apple Music";

    fn recognizes_url(q: &str) -> bool {
        q.contains("music.apple.com/") && apple_music_id(q).is_some()
    }

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        apple_music_supported(provider_config)
    }

    fn default_external_types() -> &'static [&'static str] {
        &["album"]
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
        search_apple_music(q, page, page_size, provider_config).await
    }

    const SUPPORTS_EPISODES: bool = true;

    async fn fetch_episodes(
        _state: &super::AppState,
        ref_value: &str,
        _language: Option<&str>,
    ) -> Result<ProviderEpisodes, ApiError> {
        fetch_apple_music_tracks(ref_value).await
    }
}

/// Fetches an album's tracks via the iTunes lookup API (`entity=song`), grouped
/// by disc when the album spans more than one.
async fn fetch_apple_music_tracks(ref_value: &str) -> Result<ProviderEpisodes, ApiError> {
    let id = apple_music_id(ref_value)
        .ok_or_else(|| ApiError::bad_request("Not an Apple Music album link or id"))?;
    let client = external_client();
    let value = client
        .get(format!(
            "https://itunes.apple.com/lookup?id={id}&entity=song&limit=200"
        ))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
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
    Ok(ProviderEpisodes {
        groups: apple_music_track_groups(&results),
    })
}

/// Builds track groups from an iTunes `entity=song` result list (the first row
/// is the album itself). Grouped by `discNumber` (labels appear only when
/// multi-disc); the key is the in-disc `trackNumber`.
fn apple_music_track_groups(results: &[Value]) -> Vec<ProviderEpisodeGroup> {
    let mut by_disc: BTreeMap<i64, Vec<ProviderEpisodeItem>> = BTreeMap::new();
    for row in results {
        if row.get("wrapperType").and_then(Value::as_str) != Some("track") {
            continue;
        }
        let disc = row.get("discNumber").and_then(Value::as_i64).unwrap_or(1);
        let key = row
            .get("trackNumber")
            .and_then(Value::as_i64)
            .map(|number| number.to_string())
            .unwrap_or_default();
        let title = row
            .get("trackName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if key.is_empty() && title.is_empty() {
            continue;
        }
        // Album tracks carry no per-track date.
        by_disc.entry(disc).or_default().push(ProviderEpisodeItem {
            key,
            title,
            date: None,
        });
    }
    let multi_disc = by_disc.len() > 1;
    by_disc
        .into_iter()
        .map(|(disc, items)| ProviderEpisodeGroup {
            label: if multi_disc {
                format!("Disc {disc}")
            } else {
                String::new()
            },
            items,
        })
        .collect()
}

pub(super) fn apple_music_supported(provider_config: &ProviderSearchConfig) -> bool {
    match provider_config.external_types() {
        None => true,
        Some(types) => types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case("album")),
    }
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("title", "Title"),
        field_option("cover_url", "Cover URL"),
        field_option("release_date", "Release date"),
        field_option("track_count", "Track count"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("artists", "Artists"),
        field_option("genre", "Genres"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("album", "Album")]
}

async fn search_apple_music(
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !apple_music_supported(provider_config) {
        return Ok(Vec::new());
    }
    let client = external_client();
    // A pasted Apple Music URL or bare numeric id resolves via the lookup API.
    if let Some(id) = apple_music_id(q) {
        let value = client
            .get(format!("https://itunes.apple.com/lookup?id={id}"))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status_body()
            .await?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        let result = value
            .get("results")
            .and_then(Value::as_array)
            .and_then(|results| results.first());
        return Ok(result.and_then(apple_music_candidate).into_iter().collect());
    }
    // iTunes search has no offset; ask for `page * page_size` then skip prior pages.
    let limit = page * page_size;
    let value = client
        .get("https://itunes.apple.com/search")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[
            ("entity", "album"),
            ("limit", &limit.to_string()),
            ("term", q),
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
        .skip((page - 1) * page_size)
        .filter_map(apple_music_candidate)
        .collect())
}

/// Extracts the numeric album id from an Apple Music URL
/// (`music.apple.com/<cc>/album/<slug>/<id>`, optionally with a `?i=` track
/// query, or the legacy `/album/id<id>` shape) or a bare numeric id.
fn apple_music_id(q: &str) -> Option<String> {
    let trimmed = q.trim().trim_end_matches('/');
    if trimmed.chars().all(|character| character.is_ascii_digit()) && !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }
    let (_, rest) = trimmed.split_once("/album/")?;
    let id = rest
        .split('/')
        .next_back()
        .unwrap_or_default()
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches("id");
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .then(|| id.to_string())
}

fn apple_music_candidate(item: &Value) -> Option<ExternalCandidate> {
    // Albums only — the lookup API can return songs/artists too.
    if item.get("wrapperType").and_then(Value::as_str) != Some("collection") {
        return None;
    }
    let id = item
        .get("collectionId")
        .and_then(Value::as_i64)?
        .to_string();
    let title = item
        .get("collectionName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = item
        .get("collectionViewUrl")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://music.apple.com/album/{id}"));
    // Search returns 100px artwork; the same CDN path serves larger renditions.
    let cover_url = item
        .get("artworkUrl600")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            item.get("artworkUrl100")
                .and_then(Value::as_str)
                .map(|artwork| artwork.replace("100x100", "600x600"))
        });
    let artist = item
        .get("artistName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let mut metadata = Map::new();
    metadata.insert("title".to_string(), Value::String(title.clone()));
    if let Some(date) = item
        .get("releaseDate")
        .and_then(Value::as_str)
        .and_then(crate::dates::iso_date)
    {
        metadata.insert("release_date".to_string(), Value::String(date));
    }
    if let Some(artist) = artist {
        metadata.insert(
            "artists".to_string(),
            Value::Array(vec![Value::String(artist.to_string())]),
        );
    }
    if let Some(genre) = item
        .get("primaryGenreName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "genre".to_string(),
            Value::Array(vec![Value::String(genre.to_string())]),
        );
    }
    if let Some(track_count) = item.get("trackCount").and_then(Value::as_i64) {
        metadata.insert("track_count".to_string(), Value::from(track_count));
    }
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    Some(ExternalCandidate {
        provider: AppleMusicProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(title.clone()),
        title,
        brief: artist.map(str::to_string),
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::{apple_music_candidate, apple_music_id, apple_music_track_groups};
    use serde_json::json;

    #[test]
    fn id_parses_urls_and_bare_ids() {
        assert_eq!(
            apple_music_id("https://music.apple.com/us/album/thriller/269572838"),
            Some("269572838".to_string())
        );
        // A track link still resolves its album id.
        assert_eq!(
            apple_music_id("https://music.apple.com/us/album/thriller/269572838?i=269573364"),
            Some("269572838".to_string())
        );
        // Legacy iTunes shape.
        assert_eq!(
            apple_music_id("https://itunes.apple.com/us/album/id269572838"),
            Some("269572838".to_string())
        );
        assert_eq!(apple_music_id("269572838"), Some("269572838".to_string()));
        assert_eq!(apple_music_id("thriller"), None);
    }

    #[test]
    fn candidate_surfaces_album_metadata_and_upsizes_artwork() {
        let candidate = apple_music_candidate(&json!({
            "wrapperType": "collection",
            "collectionId": 269572838,
            "collectionName": "Thriller",
            "artistName": "Michael Jackson",
            "releaseDate": "1982-11-30T08:00:00Z",
            "primaryGenreName": "Pop",
            "trackCount": 9,
            "collectionViewUrl": "https://music.apple.com/us/album/thriller/269572838",
            "artworkUrl100": "https://example.mzstatic.com/image/thumb/a/100x100bb.jpg"
        }))
        .unwrap();

        assert_eq!(candidate.source_id, "269572838");
        assert_eq!(candidate.brief.as_deref(), Some("Michael Jackson"));
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://example.mzstatic.com/image/thumb/a/600x600bb.jpg")
        );
        assert_eq!(
            candidate.metadata.get("release_date"),
            Some(&json!("1982-11-30"))
        );
        assert_eq!(
            candidate.metadata.get("artists"),
            Some(&json!(["Michael Jackson"]))
        );
        assert_eq!(candidate.metadata.get("genre"), Some(&json!(["Pop"])));
        assert_eq!(candidate.metadata.get("track_count"), Some(&json!(9)));
    }

    #[test]
    fn song_and_artist_rows_are_not_album_candidates() {
        assert!(apple_music_candidate(&json!({
            "wrapperType": "track",
            "kind": "song",
            "trackId": 1,
            "trackName": "Beat It"
        }))
        .is_none());
    }

    #[test]
    fn track_groups_split_by_disc_and_skip_the_album_row() {
        let results = vec![
            json!({ "wrapperType": "collection", "collectionName": "Album" }),
            json!({ "wrapperType": "track", "discNumber": 1, "trackNumber": 1, "trackName": "A" }),
            json!({ "wrapperType": "track", "discNumber": 2, "trackNumber": 1, "trackName": "B" }),
        ];
        let groups = apple_music_track_groups(&results);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label, "Disc 1");
        assert_eq!(groups[1].items[0].title, "B");

        let single = vec![
            json!({ "wrapperType": "collection" }),
            json!({ "wrapperType": "track", "trackNumber": 1, "trackName": "Only" }),
        ];
        let groups = apple_music_track_groups(&single);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].label, "");
    }
}
