//! Steam import. Fetches a public profile's owned games via the Steam Web API
//! (`GetOwnedGames`, which needs a Web API key — distinct from the keyless store
//! API the `steam` provider uses for detail). Items key to the `steam` provider
//! by appid; the minimal candidate is detail-fetched at commit. Status is derived
//! from playtime, mirroring Yamtrack.

use super::super::model::{ImportItem, ImportUserData, ProviderRef};
use super::ImportSource;
use crate::api::error::ApiError;
use crate::api::external::{CredentialSpec, USER_AGENT};
use crate::api::state::AppState;
use crate::contract::{ExternalCandidate, ImportInput, ImportInputKind};
use crate::secrets::SECRET_STEAM_API_KEY;
use crate::types::CanonicalStatus;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const CREDENTIALS: &[CredentialSpec] = &[CredentialSpec {
    key: SECRET_STEAM_API_KEY,
    label: "Steam Web API key",
    secret: true,
    required: true,
}];

pub(in crate::api::import) struct SteamSource;

impl ImportSource for SteamSource {
    const ID: &'static str = "steam";
    const LABEL: &'static str = "Steam";
    const INPUT: ImportInputKind = ImportInputKind::Profile;
    const INPUT_LABEL: &'static str = "SteamID64";

    fn credentials() -> &'static [CredentialSpec] {
        CREDENTIALS
    }

    fn providers() -> &'static [&'static str] {
        &["steam"]
    }

    fn available(state: &AppState) -> bool {
        api_key(state).is_some()
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        api_key(state)
            .is_none()
            .then(|| "Set the Steam Web API key".to_string())
    }

    async fn fetch(state: &AppState, input: &ImportInput) -> Result<Vec<ImportItem>, ApiError> {
        let steam_id = input
            .username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_request("A SteamID64 is required"))?;
        let api_key =
            api_key(state).ok_or_else(|| ApiError::bad_request("Set the Steam Web API key"))?;

        let url = format!(
            "https://api.steampowered.com/IPlayerService/GetOwnedGames/v0001/?key={}&steamid={}&include_appinfo=1&include_played_free_games=1&format=json",
            urlencoding::encode(&api_key),
            urlencoding::encode(steam_id)
        );
        let response = state
            .http_client()
            .get(&url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(|_| ApiError::bad_gateway("The Steam request failed"))?;
        match response.status().as_u16() {
            403 => return Err(ApiError::bad_request("Steam profile is private")),
            401 => return Err(ApiError::bad_request("Invalid Steam Web API key")),
            _ => {}
        }
        let body: Value = response
            .error_for_status()
            .map_err(|_| ApiError::bad_gateway("The Steam request failed"))?
            .json()
            .await
            .map_err(|_| ApiError::bad_gateway("Invalid Steam response"))?;

        let games = body
            .pointer("/response/games")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(games.iter().filter_map(game_item).collect())
    }
}

fn api_key(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_STEAM_API_KEY)
        .filter(|value| !value.is_empty())
}

fn game_item(game: &Value) -> Option<ImportItem> {
    let appid = game.get("appid").and_then(Value::as_i64)?.to_string();
    let title = game
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(&appid)
        .to_string();
    let url = format!("https://store.steampowered.com/app/{appid}");
    let playtime = game
        .get("playtime_forever")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let recent = game
        .get("playtime_2weeks")
        .and_then(Value::as_i64)
        .is_some();

    // Steam app names aren't language-tagged — leave the title untagged rather
    // than mislabeled `en` (wrong tags pollute language-keyed matching).
    let titles = BTreeMap::new();

    let candidate = ExternalCandidate {
        provider: "steam".to_string(),
        source_id: appid.clone(),
        url: url.clone(),
        title: title.clone(),
        original_title: None,
        brief: None,
        cover_url: None,
        titles: titles.clone(),
        metadata: Map::new(),
    };

    Some(ImportItem {
        refs: vec![ProviderRef {
            provider: "steam".to_string(),
            id: appid,
            url,
        }],
        bucket: "game".to_string(),
        title,
        titles,
        candidate: Some(candidate),
        user: ImportUserData {
            status: Some(status_from(playtime, recent)),
            ..ImportUserData::default()
        },
    })
}

/// Steam has no explicit status, so derive it from playtime: unplayed → planning,
/// played in the last two weeks → ongoing, otherwise paused.
fn status_from(playtime_minutes: i64, played_recently: bool) -> CanonicalStatus {
    if playtime_minutes == 0 {
        CanonicalStatus::Planning
    } else if played_recently {
        CanonicalStatus::Ongoing
    } else {
        CanonicalStatus::Paused
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_owned_games_with_playtime_status() {
        let unplayed = game_item(&json!({ "appid": 400, "name": "Portal", "playtime_forever": 0 }))
            .expect("item");
        assert_eq!(
            unplayed.refs[0].url,
            "https://store.steampowered.com/app/400"
        );
        assert_eq!(unplayed.bucket, "game");
        assert_eq!(unplayed.user.status, Some(CanonicalStatus::Planning));

        let recent = game_item(&json!({
            "appid": 367520, "name": "Hollow Knight",
            "playtime_forever": 1200, "playtime_2weeks": 60
        }))
        .expect("item");
        assert_eq!(recent.user.status, Some(CanonicalStatus::Ongoing));

        let old =
            game_item(&json!({ "appid": 1, "name": "Old", "playtime_forever": 5 })).expect("item");
        assert_eq!(old.user.status, Some(CanonicalStatus::Paused));
    }
}
