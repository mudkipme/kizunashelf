use super::{
    cached_or_fetch_token, external_client, field_option, named_list, provider_error,
    send_with_token_retry, type_option, CredentialSpec, ExternalProvider, ProviderSearchConfig,
};
use crate::api::state::{unix_seconds_now, AppState, CachedAccessToken};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use crate::secrets::{SECRET_IGDB_CLIENT_ID, SECRET_IGDB_CLIENT_SECRET};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub(super) struct IgdbProvider;

impl ExternalProvider for IgdbProvider {
    const ID: &'static str = "igdb";
    const LABEL: &'static str = "IGDB";

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        igdb_external_types_match(provider_config)
    }

    fn credentials() -> &'static [CredentialSpec] {
        &[
            CredentialSpec {
                key: SECRET_IGDB_CLIENT_ID,
                label: "IGDB Client ID",
                secret: false,
                required: true,
            },
            CredentialSpec {
                key: SECRET_IGDB_CLIENT_SECRET,
                label: "IGDB Client Secret",
                secret: true,
                required: true,
            },
        ]
    }

    fn default_external_types() -> &'static [&'static str] {
        &["game"]
    }

    fn field_options() -> Vec<ExternalProviderFieldOption> {
        field_options()
    }

    fn type_options() -> Vec<ExternalProviderTypeOption> {
        type_options()
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        igdb_credentials(state)
            .is_none()
            .then(|| "Set the IGDB client ID and client secret".to_string())
    }

    fn available(state: &AppState) -> bool {
        igdb_credentials(state).is_some()
    }

    async fn search(
        state: &AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        search_igdb(state, q, page, page_size, provider_config).await
    }
}

async fn search_igdb(
    state: &AppState,
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !igdb_external_types_match(provider_config) {
        return Ok(Vec::new());
    }
    let Some((client_id, client_secret)) = igdb_credentials(state) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    let token = igdb_access_token(state, client, &client_id, &client_secret, false).await?;
    let offset = (page - 1) * page_size;
    let body = igdb_query_body(q, page_size, offset);
    let response = send_with_token_retry(
        state,
        "igdb",
        &token,
        |token| {
            client
                .post("https://api.igdb.com/v4/games")
                .header("Client-ID", &client_id)
                .bearer_auth(token)
                .body(body.clone())
        },
        || igdb_access_token(state, client, &client_id, &client_secret, true),
    )
    .await?;
    let data = response
        .error_for_status()
        .map_err(provider_error)?
        .json::<Value>()
        .await
        .map_err(provider_error)?
        .as_array()
        .cloned()
        .unwrap_or_default();
    // The viewer's language selects which region's `game_localizations` name
    // becomes the display title (see `igdb_candidate`).
    let language = provider_config.language.as_deref();
    Ok(data
        .iter()
        .filter_map(|item| igdb_candidate(item, language))
        .collect())
}

/// Maps a viewer language preference to an IGDB region `identifier` slug, used to
/// pick a localized title out of a game's `game_localizations`. IGDB has no
/// request-language parameter and keys localizations by *region*, not language,
/// so this mapping is deliberately coarse (region ≠ language) and best-effort.
fn igdb_region_identifier(language: Option<&str>) -> Option<&'static str> {
    let language = language.map(str::trim).filter(|value| !value.is_empty())?;
    Some(match language.to_ascii_lowercase().as_str() {
        code if code.starts_with("ja") => "japan",
        code if code.starts_with("ko") => "korea",
        "zh-hant" | "zh-tw" | "zh-hk" => "taiwan",
        code if code.starts_with("zh") => "china",
        code if code.starts_with("en") => "north_america",
        _ => return None,
    })
}

/// Reverse of `igdb_region_identifier` for building the `titles` map: maps an
/// IGDB region `identifier` to the bare content language its localized title is
/// written in. Both China and Taiwan fold to `zh` (the stored-data invariant is
/// script-agnostic); regions that don't cleanly imply one language are skipped.
fn igdb_region_language(identifier: &str) -> Option<&'static str> {
    Some(match identifier {
        "japan" => "ja",
        "korea" => "ko",
        "china" | "taiwan" | "hong_kong" => "zh",
        "north_america" => "en",
        _ => return None,
    })
}

pub(super) fn igdb_external_types_match(provider_config: &ProviderSearchConfig) -> bool {
    let Some(external_types) = provider_config.external_types() else {
        return true;
    };
    external_types
        .iter()
        .any(|external_type| matches!(external_type.trim().to_ascii_lowercase().as_str(), "game"))
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("name", "Name"),
        field_option("cover_url", "Cover URL"),
        field_option("first_release_date", "First release date"),
        field_option("rating", "User rating"),
        field_option("aggregated_rating", "Critic rating"),
        field_option("total_rating", "Total rating"),
        field_option("total_rating_count", "Total rating count"),
        field_option("format", "Type"),
        field_option("franchise", "Franchise"),
        field_option("official_site", "Official site"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("alternative_names", "Alternative names"),
        field_option("genres", "Genres"),
        field_option("platforms", "Platforms"),
        field_option("themes", "Themes"),
        field_option("game_modes", "Game modes"),
        field_option("player_perspectives", "Player perspectives"),
        field_option("game_engines", "Game engines"),
        field_option("developers", "Developers"),
        field_option("publishers", "Publishers"),
        field_option("summary", "Summary"),
        field_option("storyline", "Storyline"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("game", "Game")]
}

fn igdb_query_body(q: &str, page_size: usize, offset: usize) -> String {
    let fields = "fields name,url,summary,storyline,first_release_date,cover.url,game_type,\
rating,aggregated_rating,total_rating,total_rating_count,websites.url,websites.category,\
alternative_names.name,genres.name,platforms.id,platforms.name,themes.name,game_modes.name,\
player_perspectives.name,game_engines.name,franchises.name,collection.name,\
game_localizations.name,game_localizations.region.identifier,\
involved_companies.developer,involved_companies.publisher,involved_companies.company.name;";
    let trimmed = q.trim();
    if trimmed.chars().all(|character| character.is_ascii_digit()) {
        return format!("{fields} where id = {trimmed}; limit {page_size}; offset {offset};");
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return format!(
            r#"{fields} where url = "{}"; limit {page_size}; offset {offset};"#,
            escape_igdb_string(trimmed)
        );
    }
    format!(
        r#"{fields} search "{}"; limit {page_size}; offset {offset};"#,
        escape_igdb_string(trimmed)
    )
}

fn escape_igdb_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

async fn igdb_access_token(
    state: &AppState,
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    force_refresh: bool,
) -> Result<String, ApiError> {
    cached_or_fetch_token(state, "igdb", "IGDB", force_refresh, || async {
        let value = client
            .post("https://id.twitch.tv/oauth2/token")
            // Credentials go in the form body, never the query string, so they are
            // not echoed back in any error/log carrying the request URL.
            .form(&[
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("grant_type", "client_credentials"),
            ])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json::<Value>()
            .await
            .map_err(provider_error)?;
        let access_token = value
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                ApiError::bad_request("IGDB token response did not include an access token")
            })?;
        let expires_in = value
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(3600)
            .saturating_sub(60)
            .max(60);
        Ok(CachedAccessToken {
            access_token,
            refresh_token: value
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::to_string),
            expires_at: Instant::now() + Duration::from_secs(expires_in),
            expires_at_unix_seconds: unix_seconds_now() + expires_in,
        })
    })
    .await
}

fn igdb_credentials(state: &AppState) -> Option<(String, String)> {
    let store = state.secret_store();
    let client_id = store.get(SECRET_IGDB_CLIENT_ID)?;
    let client_secret = store.get(SECRET_IGDB_CLIENT_SECRET)?;
    (!client_id.is_empty() && !client_secret.is_empty()).then_some((client_id, client_secret))
}

fn igdb_candidate(item: &Value, language: Option<&str>) -> Option<ExternalCandidate> {
    // The canonical `name` is IGDB's single title (usually the original/romanized
    // one). A game may also carry region-keyed `game_localizations`; when the
    // viewer's language maps to a region that has one, it becomes the display
    // title while `name` stays the original. Localizations we can tag by language
    // also seed the `titles` map for per-language title fields and matching.
    let name = item.get("name")?.as_str()?.to_string();
    let localizations = item.get("game_localizations").and_then(Value::as_array);
    let localized_name = |region: &str| -> Option<String> {
        localizations?.iter().find_map(|localization| {
            let matches_region = localization
                .get("region")
                .and_then(|region| region.get("identifier"))
                .and_then(Value::as_str)
                == Some(region);
            matches_region
                .then(|| localization.get("name").and_then(Value::as_str))
                .flatten()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
    };
    let title = igdb_region_identifier(language)
        .and_then(localized_name)
        .unwrap_or_else(|| name.clone());
    let url = item
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let source_id = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            item.get("id")
                .and_then(Value::as_i64)
                .map(|id| id.to_string())
        })?;
    let release_date = item
        .get("first_release_date")
        .and_then(Value::as_i64)
        .and_then(|timestamp| chrono::DateTime::from_timestamp(timestamp, 0))
        .map(|date| date.format("%Y-%m-%d").to_string());
    let cover_url = item
        .get("cover")
        .and_then(|cover| cover.get("url"))
        .and_then(Value::as_str)
        .map(|value| format!("https:{}", value.replace("t_thumb", "t_cover_big")));
    let mut metadata = Map::new();
    metadata.insert("name".to_string(), Value::String(name.clone()));
    if let Some(cover_url) = &cover_url {
        metadata.insert("cover_url".to_string(), Value::String(cover_url.clone()));
    }
    if let Some(release_date) = &release_date {
        metadata.insert(
            "first_release_date".to_string(),
            Value::String(release_date.clone()),
        );
    }
    if let Some(summary) = item
        .get("summary")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("summary".to_string(), Value::String(summary.to_string()));
    }
    if let Some(storyline) = item
        .get("storyline")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "storyline".to_string(),
            Value::String(storyline.to_string()),
        );
    }
    // Ratings are 0–100 floats; round to whole numbers for a tidy rating field.
    for key in ["rating", "aggregated_rating", "total_rating"] {
        if let Some(rating) = item
            .get(key)
            .and_then(Value::as_f64)
            .filter(|value| *value > 0.0)
        {
            metadata.insert(
                key.to_string(),
                Value::Number((rating.round() as i64).into()),
            );
        }
    }
    if let Some(count) = item
        .get("total_rating_count")
        .and_then(Value::as_i64)
        .filter(|count| *count > 0)
    {
        metadata.insert(
            "total_rating_count".to_string(),
            Value::Number(count.into()),
        );
    }
    if let Some(format) = igdb_game_type(item.get("game_type").and_then(Value::as_i64)) {
        metadata.insert("format".to_string(), Value::String(format.to_string()));
    }
    // `*.name` reference arrays → JSON string arrays for list-type fields.
    for key in [
        "alternative_names",
        "genres",
        "themes",
        "game_modes",
        "player_perspectives",
        "game_engines",
    ] {
        if let Some(values) = named_list(item.get(key)) {
            metadata.insert(key.to_string(), values);
        }
    }
    // Platforms get the same treatment, but IGDB id 6 is "PC (Microsoft Windows)".
    if let Some(platforms) = item.get("platforms").and_then(Value::as_array) {
        let platforms: Vec<Value> = platforms
            .iter()
            .filter_map(|platform| {
                let name = platform.get("name").and_then(Value::as_str)?;
                let name = if platform.get("id").and_then(Value::as_i64) == Some(6) {
                    "Windows"
                } else {
                    name
                };
                (!name.is_empty()).then(|| Value::String(name.to_string()))
            })
            .collect();
        if !platforms.is_empty() {
            metadata.insert("platforms".to_string(), Value::Array(platforms));
        }
    }
    // Official site: IGDB website category 1 is the official homepage.
    if let Some(official_site) = item
        .get("websites")
        .and_then(Value::as_array)
        .and_then(|websites| {
            websites
                .iter()
                .find(|website| website.get("category").and_then(Value::as_i64) == Some(1))
        })
        .and_then(|website| website.get("url"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "official_site".to_string(),
            Value::String(official_site.to_string()),
        );
    }
    // Franchise: prefer an explicit franchise, fall back to the collection name.
    if let Some(franchise) = item
        .get("franchises")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(|value| value.get("name"))
        .or_else(|| {
            item.get("collection")
                .and_then(|collection| collection.get("name"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "franchise".to_string(),
            Value::String(franchise.to_string()),
        );
    }
    if let Some(companies) = item.get("involved_companies").and_then(Value::as_array) {
        let company_names = |role: &str| -> Vec<Value> {
            companies
                .iter()
                .filter(|company| company.get(role).and_then(Value::as_bool).unwrap_or(false))
                .filter_map(|company| {
                    company
                        .get("company")
                        .and_then(|company| company.get("name"))
                        .and_then(Value::as_str)
                })
                .filter(|name| !name.is_empty())
                .map(|name| Value::String(name.to_string()))
                .collect()
        };
        let developers = company_names("developer");
        if !developers.is_empty() {
            metadata.insert("developers".to_string(), Value::Array(developers));
        }
        let publishers = company_names("publisher");
        if !publishers.is_empty() {
            metadata.insert("publishers".to_string(), Value::Array(publishers));
        }
    }
    // Tag every localization we can map to a bare content language so quick-add
    // can populate per-language title fields and same-language matching works.
    let mut titles = BTreeMap::new();
    if let Some(localizations) = localizations {
        for localization in localizations {
            let language = localization
                .get("region")
                .and_then(|region| region.get("identifier"))
                .and_then(Value::as_str)
                .and_then(igdb_region_language);
            let localized = localization
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let (Some(language), Some(localized)) = (language, localized) {
                titles
                    .entry(language.to_string())
                    .or_insert_with(|| localized.to_string());
            }
        }
    }
    Some(ExternalCandidate {
        provider: "igdb".to_string(),
        source_id,
        url,
        original_title: Some(name.clone()),
        title,
        brief: igdb_brief(item),
        cover_url,
        titles,
        metadata,
    })
}

/// Maps IGDB's `game_type` enum to a readable label (Main game/DLC/Expansion/…).
fn igdb_game_type(game_type: Option<i64>) -> Option<&'static str> {
    Some(match game_type? {
        0 => "Main game",
        1 => "DLC / Add-on",
        2 => "Expansion",
        3 => "Bundle",
        4 => "Standalone expansion",
        5 => "Mod",
        6 => "Episode",
        7 => "Season",
        8 => "Remake",
        9 => "Remaster",
        10 => "Expanded game",
        11 => "Port",
        12 => "Fork",
        13 => "Pack",
        14 => "Update",
        _ => return None,
    })
}

/// Combines summary and storyline into one brief, falling back to whichever is present.
fn igdb_brief(item: &Value) -> Option<String> {
    let text = |key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    match (text("summary"), text("storyline")) {
        (Some(summary), Some(storyline)) => Some(format!("{summary}\n\n{storyline}")),
        (Some(summary), None) => Some(summary.to_string()),
        (None, Some(storyline)) => Some(storyline.to_string()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::igdb_candidate;
    use serde_json::json;

    #[test]
    fn candidate_surfaces_extended_metadata() {
        let candidate = igdb_candidate(&json!({
            "id": 1,
            "name": "Hollow Knight",
            "url": "https://www.igdb.com/games/hollow-knight",
            "total_rating": 91.4,
            "total_rating_count": 1200,
            "game_type": 0,
            "genres": [{ "name": "Platform" }, { "name": "Adventure" }],
            "game_modes": [{ "name": "Single player" }],
            "franchises": [{ "name": "Hollow Knight" }],
            "involved_companies": [
                { "developer": true, "publisher": true, "company": { "name": "Team Cherry" } },
                { "developer": false, "publisher": true, "company": { "name": "Some Publisher" } }
            ]
        }), None)
        .unwrap();

        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("total_rating"), Some(&json!(91)));
        assert_eq!(metadata.get("total_rating_count"), Some(&json!(1200)));
        assert_eq!(metadata.get("format"), Some(&json!("Main game")));
        assert_eq!(
            metadata.get("genres"),
            Some(&json!(["Platform", "Adventure"]))
        );
        assert_eq!(metadata.get("game_modes"), Some(&json!(["Single player"])));
        assert_eq!(metadata.get("franchise"), Some(&json!("Hollow Knight")));
        assert_eq!(metadata.get("developers"), Some(&json!(["Team Cherry"])));
        assert_eq!(
            metadata.get("publishers"),
            Some(&json!(["Team Cherry", "Some Publisher"]))
        );
    }

    #[test]
    fn candidate_combines_brief_and_normalizes_platform_and_site() {
        let candidate = igdb_candidate(
            &json!({
                "id": 2,
                "name": "Celeste",
                "url": "https://www.igdb.com/games/celeste",
                "summary": "Climb the mountain.",
                "storyline": "Help Madeline.",
                "platforms": [
                    { "id": 6, "name": "PC (Microsoft Windows)" },
                    { "id": 130, "name": "Nintendo Switch" }
                ],
                "websites": [
                    { "category": 13, "url": "https://store.steampowered.com/app/504230" },
                    { "category": 1, "url": "https://www.celestegame.com" }
                ]
            }),
            None,
        )
        .unwrap();

        // summary + storyline are concatenated into the brief.
        assert_eq!(
            candidate.brief.as_deref(),
            Some("Climb the mountain.\n\nHelp Madeline.")
        );
        // IGDB platform id 6 is shortened to "Windows".
        assert_eq!(
            candidate.metadata.get("platforms"),
            Some(&json!(["Windows", "Nintendo Switch"]))
        );
        // Official site is website category 1 (not the Steam link, category 13).
        assert_eq!(
            candidate.metadata.get("official_site"),
            Some(&json!("https://www.celestegame.com"))
        );
    }

    #[test]
    fn candidate_localizes_title_from_region() {
        let game = json!({
            "id": 3,
            "name": "Final Fantasy VII",
            "url": "https://www.igdb.com/games/final-fantasy-vii",
            "game_localizations": [
                { "name": "ファイナルファンタジーVII", "region": { "identifier": "japan" } },
                { "name": "最终幻想VII", "region": { "identifier": "china" } },
                { "name": "最終幻想VII", "region": { "identifier": "taiwan" } }
            ]
        });

        // A Japanese viewer sees the Japan localization; the canonical name stays
        // the original title, and every mappable region seeds the titles map.
        let ja = igdb_candidate(&game, Some("ja-JP")).unwrap();
        assert_eq!(ja.title, "ファイナルファンタジーVII");
        assert_eq!(ja.original_title.as_deref(), Some("Final Fantasy VII"));
        assert_eq!(
            ja.titles.get("ja").map(String::as_str),
            Some("ファイナルファンタジーVII")
        );
        // Chinese folds to bare `zh`; the first-seen (China) localization wins.
        assert_eq!(ja.titles.get("zh").map(String::as_str), Some("最终幻想VII"));

        // A Traditional-Chinese viewer resolves to the Taiwan localization.
        let hant = igdb_candidate(&game, Some("zh-Hant")).unwrap();
        assert_eq!(hant.title, "最終幻想VII");

        // No localization for the region → fall back to the canonical name.
        let en = igdb_candidate(&game, Some("en-US")).unwrap();
        assert_eq!(en.title, "Final Fantasy VII");
    }
}
