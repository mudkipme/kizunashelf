use super::{
    external_client, field_option, provider_error, string_list, strip_html, type_option,
    ExternalProvider, ProviderResponseExt, ProviderSearchConfig, USER_AGENT,
};
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct SteamProvider;

impl ExternalProvider for SteamProvider {
    const ID: &'static str = "steam";
    const LABEL: &'static str = "Steam";
    // Steam has no public catalog search; it only resolves a pasted store URL or
    // app id.
    const SEARCHABLE: bool = false;

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        steam_supported(provider_config)
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

    async fn search(
        _state: &super::AppState,
        q: &str,
        _page: usize,
        _page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        resolve_steam(q, provider_config).await
    }
}

pub(super) fn steam_supported(provider_config: &ProviderSearchConfig) -> bool {
    match provider_config.external_types() {
        None => true,
        Some(types) => types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case("game")),
    }
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("name", "Name"),
        field_option("cover_url", "Cover URL"),
        field_option("release_date", "Release date"),
        field_option("platform", "Platform"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("developers", "Developers"),
        field_option("publishers", "Publishers"),
        field_option("genres", "Genres"),
        field_option("description", "Description"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![type_option("game", "Game")]
}

/// Steam's store-API language for a viewer language preference: the
/// `appdetails` `l` value and the matching `Accept-Language` header. Steam is
/// one of the sources that genuinely distinguish Simplified/Traditional
/// Chinese; languages its store doesn't ship fall back to English.
fn steam_language(language: Option<&str>) -> (&'static str, &'static str) {
    let language = language.map(str::trim).unwrap_or_default();
    match language.to_ascii_lowercase().as_str() {
        "zh" | "zh-hans" => ("schinese", "zh-CN"),
        "zh-hant" => ("tchinese", "zh-TW"),
        "ja" => ("japanese", "ja"),
        "ko" => ("koreana", "ko"),
        "cs" => ("czech", "cs"),
        "da" => ("danish", "da"),
        "nl" => ("dutch", "nl"),
        "fi" => ("finnish", "fi"),
        "fr" => ("french", "fr"),
        "de" => ("german", "de"),
        "el" => ("greek", "el"),
        "hu" => ("hungarian", "hu"),
        "it" => ("italian", "it"),
        "no" => ("norwegian", "no"),
        "pl" => ("polish", "pl"),
        "pt" => ("portuguese", "pt"),
        "ru" => ("russian", "ru"),
        "es" => ("spanish", "es"),
        "sv" => ("swedish", "sv"),
        "tr" => ("turkish", "tr"),
        _ => ("english", "en"),
    }
}

async fn resolve_steam(
    q: &str,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !steam_supported(provider_config) {
        return Ok(Vec::new());
    }
    let Some(appid) = steam_appid(q) else {
        return Ok(Vec::new());
    };
    let (store_language, accept_language) = steam_language(provider_config.language.as_deref());
    let client = external_client();
    let value = client
        .get("https://store.steampowered.com/api/appdetails")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT_LANGUAGE, accept_language)
        .query(&[("appids", appid.as_str()), ("l", store_language)])
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status_body()
        .await?
        .json::<Value>()
        .await
        .map_err(provider_error)?;
    let data = value
        .get(&appid)
        .filter(|entry| entry.get("success").and_then(Value::as_bool) == Some(true))
        .and_then(|entry| entry.get("data"));
    Ok(data
        .and_then(|data| steam_candidate(&appid, data))
        .into_iter()
        .collect())
}

/// Extracts a Steam app id from a `store.steampowered.com/app/<id>` URL or a bare
/// numeric id.
fn steam_appid(q: &str) -> Option<String> {
    let trimmed = q.trim().trim_end_matches('/');
    if !trimmed.is_empty() && trimmed.chars().all(|character| character.is_ascii_digit()) {
        return Some(trimmed.to_string());
    }
    let (_, rest) = trimmed.split_once("/app/")?;
    let id = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .then(|| id.to_string())
}

fn steam_candidate(appid: &str, data: &Value) -> Option<ExternalCandidate> {
    let title = data
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let url = format!("https://store.steampowered.com/app/{appid}");
    // Prefer the tall library cover; fall back to the wide header image.
    let cover_url = data
        .get("header_image")
        .and_then(Value::as_str)
        .map(|header| {
            if header.ends_with("header.jpg") {
                header.replace("header.jpg", "library_600x900_2x.jpg")
            } else {
                header.to_string()
            }
        });
    let description = data
        .get("short_description")
        .and_then(Value::as_str)
        .map(strip_html)
        .filter(|value| !value.is_empty());

    let mut metadata = Map::new();
    metadata.insert("name".to_string(), Value::String(title.clone()));
    metadata.insert(
        "platform".to_string(),
        Value::Array(vec![Value::String("PC".to_string())]),
    );
    if let Some(release_date) = data
        .get("release_date")
        .and_then(|release| release.get("date"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "release_date".to_string(),
            Value::String(release_date.to_string()),
        );
    }
    if let Some(developers) = string_list(data.get("developers")) {
        metadata.insert("developers".to_string(), developers);
    }
    if let Some(publishers) = string_list(data.get("publishers")) {
        metadata.insert("publishers".to_string(), publishers);
    }
    if let Some(genres) = data.get("genres").and_then(Value::as_array) {
        let genres: Vec<Value> = genres
            .iter()
            .filter_map(|genre| genre.get("description").and_then(Value::as_str))
            .filter(|value| !value.is_empty())
            .map(|value| Value::String(value.to_string()))
            .collect();
        if !genres.is_empty() {
            metadata.insert("genres".to_string(), Value::Array(genres));
        }
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
        provider: SteamProvider::ID.to_string(),
        source_id: appid.to_string(),
        url,
        original_title: Some(title.clone()),
        title,
        brief: description,
        cover_url,
        titles: BTreeMap::new(),
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::{steam_appid, steam_candidate};
    use serde_json::json;

    #[test]
    fn candidate_surfaces_game_metadata() {
        let candidate = steam_candidate(
            "367520",
            &json!({
                "name": "Hollow Knight",
                "developers": ["Team Cherry"],
                "publishers": ["Team Cherry"],
                "release_date": { "date": "24 Feb, 2017" },
                "genres": [{ "description": "Action" }, { "description": "Indie" }],
                "header_image": "https://cdn.example.com/367520/header.jpg",
                "short_description": "A <b>hand-drawn</b> action adventure."
            }),
        )
        .unwrap();

        assert_eq!(candidate.source_id, "367520");
        assert_eq!(
            candidate.metadata.get("release_date"),
            Some(&json!("24 Feb, 2017"))
        );
        assert_eq!(
            candidate.metadata.get("genres"),
            Some(&json!(["Action", "Indie"]))
        );
        assert_eq!(candidate.metadata.get("platform"), Some(&json!(["PC"])));
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://cdn.example.com/367520/library_600x900_2x.jpg")
        );
        assert_eq!(
            candidate.metadata.get("description"),
            Some(&json!("A hand-drawn action adventure."))
        );
    }

    #[test]
    fn appid_parses_url_and_bare_id() {
        assert_eq!(
            steam_appid("https://store.steampowered.com/app/367520/Hollow_Knight/"),
            Some("367520".to_string())
        );
        assert_eq!(steam_appid("367520"), Some("367520".to_string()));
        assert_eq!(steam_appid("hollow knight"), None);
    }
}
