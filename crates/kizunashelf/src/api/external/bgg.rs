use super::{
    external_client, field_option, provider_error, send_limited, type_option, CredentialSpec,
    ExternalProvider, ProviderResponseExt, ProviderSearchConfig, USER_AGENT,
};
use crate::api::state::AppState;
use crate::api::ApiError;
use crate::contract::{ExternalCandidate, ExternalProviderFieldOption, ExternalProviderTypeOption};
use crate::secrets::SECRET_BGG_API_TOKEN;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) struct BoardGameGeekProvider;

impl ExternalProvider for BoardGameGeekProvider {
    const ID: &'static str = "bgg";
    const LABEL: &'static str = "BoardGameGeek";

    fn recognizes_url(q: &str) -> bool {
        q.contains("boardgamegeek.com/")
    }

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool {
        bgg_supported(provider_config)
    }

    /// BGG restricted the XML API to registered applications (401 otherwise —
    /// <https://boardgamegeek.com/using_the_xml_api>): register the app once,
    /// get an application token, send it as a Bearer header. An app identity,
    /// not a user account — clients may bundle it.
    fn credentials() -> &'static [CredentialSpec] {
        &[CredentialSpec {
            key: SECRET_BGG_API_TOKEN,
            label: "BGG XML API Token",
            secret: true,
            required: true,
        }]
    }

    fn unavailable_reason(state: &AppState) -> Option<String> {
        bgg_token(state).is_none().then(|| {
            "Set a BGG XML API token (register at boardgamegeek.com/using_the_xml_api)".to_string()
        })
    }

    fn available(state: &AppState) -> bool {
        bgg_token(state).is_some()
    }

    fn field_options() -> Vec<ExternalProviderFieldOption> {
        field_options()
    }

    fn type_options() -> Vec<ExternalProviderTypeOption> {
        type_options()
    }

    async fn search(
        state: &super::AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError> {
        search_bgg(state, q, page, page_size, provider_config).await
    }
}

fn bgg_token(state: &AppState) -> Option<String> {
    state
        .secret_store()
        .get(SECRET_BGG_API_TOKEN)
        .filter(|value| !value.is_empty())
}

const BGG_TYPES: &str = "boardgame,boardgameexpansion";

pub(super) fn bgg_supported(provider_config: &ProviderSearchConfig) -> bool {
    match provider_config.external_types() {
        None => true,
        Some(types) => types.iter().any(|external_type| {
            matches!(
                external_type.trim().to_ascii_lowercase().as_str(),
                "boardgame" | "boardgameexpansion"
            )
        }),
    }
}

pub(super) fn field_options() -> Vec<ExternalProviderFieldOption> {
    vec![
        field_option("name", "Name"),
        field_option("cover_url", "Cover URL"),
        field_option("year", "Year published"),
        field_option("platform", "Platform"),
        field_option("players", "Players"),
        field_option("playtime", "Playing time"),
        field_option("min_age", "Minimum age"),
        field_option("score", "Score"),
        field_option("score_count", "Score count"),
        field_option("rank", "Rank"),
        field_option("description", "Description"),
        // Lists — map these to list-type fields (enum list / text list / relation).
        field_option("designers", "Designers"),
        field_option("artists", "Artists"),
        field_option("publishers", "Publishers"),
        field_option("developers", "Developers"),
        field_option("categories", "Categories"),
    ]
}

pub(super) fn type_options() -> Vec<ExternalProviderTypeOption> {
    vec![
        type_option("boardgame", "Board game"),
        type_option("boardgameexpansion", "Expansion"),
    ]
}

async fn search_bgg(
    state: &AppState,
    q: &str,
    page: usize,
    page_size: usize,
    provider_config: &ProviderSearchConfig,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !bgg_supported(provider_config) {
        return Ok(Vec::new());
    }
    let Some(token) = bgg_token(state) else {
        return Ok(Vec::new());
    };
    let client = external_client();
    // A pasted BGG URL or bare numeric id resolves to a single game.
    if let Some(id) = bgg_id(q) {
        let items = bgg_thing(client, &token, &id).await?;
        return Ok(items.iter().filter_map(bgg_candidate).collect());
    }
    // Search returns ids + names with no pagination; resolve the requested page's
    // ids via one `thing` call so each candidate carries full metadata.
    let xml = send_limited(
        client
            .get("https://boardgamegeek.com/xmlapi2/search")
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .bearer_auth(&token)
            .query(&[("query", q), ("type", BGG_TYPES)]),
    )
    .await
    .map_err(provider_error)?
    .error_for_status_body()
    .await?
    .text()
    .await
    .map_err(provider_error)?;
    let mut ids: Vec<String> = parse_bgg_items(&xml)
        .into_iter()
        .filter_map(|item| item.id)
        .collect();
    ids.dedup();
    let page_ids: Vec<String> = ids
        .into_iter()
        .skip((page - 1) * page_size)
        .take(page_size)
        .collect();
    if page_ids.is_empty() {
        return Ok(Vec::new());
    }
    let items = bgg_thing(client, &token, &page_ids.join(",")).await?;
    Ok(items.iter().filter_map(bgg_candidate).collect())
}

async fn bgg_thing(
    client: &reqwest::Client,
    token: &str,
    ids: &str,
) -> Result<Vec<BggItem>, ApiError> {
    let xml = send_limited(
        client
            .get("https://boardgamegeek.com/xmlapi2/thing")
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .bearer_auth(token)
            // `stats=1` includes the ratings/rank block (average, usersrated, rank).
            .query(&[("type", BGG_TYPES), ("stats", "1"), ("id", ids)]),
    )
    .await
    .map_err(provider_error)?
    .error_for_status_body()
    .await?
    .text()
    .await
    .map_err(provider_error)?;
    Ok(parse_bgg_items(&xml))
}

/// Extracts a BGG game id from a `boardgamegeek.com/boardgame[expansion]/<id>`
/// URL or a bare numeric id.
fn bgg_id(q: &str) -> Option<String> {
    let trimmed = q.trim().trim_end_matches('/');
    if !trimmed.is_empty() && trimmed.chars().all(|character| character.is_ascii_digit()) {
        return Some(trimmed.to_string());
    }
    let (_, rest) = trimmed
        .split_once("/boardgameexpansion/")
        .or_else(|| trimmed.split_once("/boardgame/"))?;
    let id = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .then(|| id.to_string())
}

/// One `<item>` from a BGG `thing`/`search` response, flattened to the bits the
/// candidate needs.
#[derive(Default)]
struct BggItem {
    id: Option<String>,
    item_type: Option<String>,
    /// `(name-type, value)` — `primary` is the display title.
    names: Vec<(String, String)>,
    year: Option<String>,
    image: Option<String>,
    thumbnail: Option<String>,
    description: Option<String>,
    /// `(link-type, value)` — designers, publishers, categories, …
    links: Vec<(String, String)>,
    min_players: Option<String>,
    max_players: Option<String>,
    playing_time: Option<String>,
    min_age: Option<String>,
    /// From the `stats=1` block: rating average, users rated, overall rank.
    rating_average: Option<String>,
    users_rated: Option<String>,
    rank: Option<String>,
}

/// Streams a BGG XML payload into `BggItem`s. BGG uses attribute-valued empty
/// elements (`<name value="…"/>`, `<link .../>`) plus text-bearing `description`
/// and `image`, so we track the open element to capture text into the right slot.
fn parse_bgg_items(xml: &str) -> Vec<BggItem> {
    let mut reader = Reader::from_str(xml);
    let mut items: Vec<BggItem> = Vec::new();
    let mut current: Option<BggItem> = None;
    // Which text-bearing field the open element feeds, if any.
    let mut text_target: Option<&'static str> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                let name = element.local_name();
                let tag = name.as_ref();
                match tag {
                    b"item" => {
                        let mut item = BggItem::default();
                        for (key, value) in attributes(&element) {
                            match key.as_str() {
                                "id" => item.id = Some(value),
                                "type" => item.item_type = Some(value),
                                _ => {}
                            }
                        }
                        current = Some(item);
                    }
                    b"name" => {
                        if let Some(item) = current.as_mut() {
                            let attrs = attributes(&element);
                            let name_type = attrs
                                .iter()
                                .find(|(key, _)| key == "type")
                                .map(|(_, value)| value.clone())
                                .unwrap_or_default();
                            if let Some((_, value)) =
                                attrs.into_iter().find(|(key, _)| key == "value")
                            {
                                item.names.push((name_type, value));
                            }
                        }
                    }
                    b"yearpublished" => {
                        set_value(&mut current, &element, |item, v| item.year = Some(v))
                    }
                    b"minplayers" => {
                        set_value(&mut current, &element, |item, v| item.min_players = Some(v))
                    }
                    b"maxplayers" => {
                        set_value(&mut current, &element, |item, v| item.max_players = Some(v))
                    }
                    b"playingtime" => set_value(&mut current, &element, |item, v| {
                        item.playing_time = Some(v)
                    }),
                    b"minage" => {
                        set_value(&mut current, &element, |item, v| item.min_age = Some(v))
                    }
                    b"average" => set_value(&mut current, &element, |item, v| {
                        item.rating_average = Some(v)
                    }),
                    b"usersrated" => {
                        set_value(&mut current, &element, |item, v| item.users_rated = Some(v))
                    }
                    b"rank" => {
                        // Several rank rows; keep the overall "boardgame" rank only.
                        if let Some(item) = current.as_mut() {
                            let attrs = attributes(&element);
                            let is_overall = attrs
                                .iter()
                                .any(|(key, value)| key == "name" && value == "boardgame");
                            if is_overall {
                                if let Some((_, value)) =
                                    attrs.into_iter().find(|(key, _)| key == "value")
                                {
                                    if value.chars().all(|c| c.is_ascii_digit())
                                        && !value.is_empty()
                                    {
                                        item.rank = Some(value);
                                    }
                                }
                            }
                        }
                    }
                    b"link" => {
                        if let Some(item) = current.as_mut() {
                            let attrs = attributes(&element);
                            let link_type = attrs
                                .iter()
                                .find(|(key, _)| key == "type")
                                .map(|(_, value)| value.clone());
                            let value = attrs
                                .into_iter()
                                .find(|(key, _)| key == "value")
                                .map(|(_, value)| value);
                            if let (Some(link_type), Some(value)) = (link_type, value) {
                                item.links.push((link_type, value));
                            }
                        }
                    }
                    b"image" => text_target = Some("image"),
                    b"thumbnail" => text_target = Some("thumbnail"),
                    b"description" => text_target = Some("description"),
                    _ => {}
                }
            }
            Ok(Event::Text(text)) => {
                if let (Some(target), Some(item)) = (text_target, current.as_mut()) {
                    // Entity references (`&amp;`) split one logical text node into
                    // several Text events, so accumulate fragments rather than
                    // overwriting; trimming happens once the field is consumed.
                    // BGG's API declares `<?xml version="1.0"?>`, so decode with
                    // 1.0 end-of-line rules rather than 1.1's wider set.
                    if let Ok(decoded) = text.xml10_content() {
                        let Some(slot) = text_slot(item, target) else {
                            continue;
                        };
                        slot.get_or_insert_with(String::new).push_str(&decoded);
                    }
                }
            }
            // quick-xml emits entity references (`&amp;`, `&#10;`) as their own
            // events between text fragments; resolve them back into the field.
            Ok(Event::GeneralRef(entity)) => {
                if let (Some(target), Some(item)) = (text_target, current.as_mut()) {
                    let resolved = match entity.resolve_char_ref() {
                        Ok(Some(character)) => Some(character.to_string()),
                        _ => entity.decode().ok().and_then(|name| {
                            match name.as_ref() {
                                "amp" => Some("&"),
                                "lt" => Some("<"),
                                "gt" => Some(">"),
                                "quot" => Some("\""),
                                "apos" => Some("'"),
                                _ => None,
                            }
                            .map(str::to_string)
                        }),
                    };
                    if let Some(resolved) = resolved {
                        let Some(slot) = text_slot(item, target) else {
                            continue;
                        };
                        slot.get_or_insert_with(String::new).push_str(&resolved);
                    }
                }
            }
            Ok(Event::End(element)) => {
                let name = element.local_name();
                match name.as_ref() {
                    b"item" => {
                        if let Some(item) = current.take() {
                            items.push(item);
                        }
                    }
                    b"image" | b"thumbnail" | b"description" => text_target = None,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    items
}

/// Returns the mutable text field an open text-bearing element feeds into.
fn text_slot<'a>(item: &'a mut BggItem, target: &str) -> Option<&'a mut Option<String>> {
    match target {
        "image" => Some(&mut item.image),
        "thumbnail" => Some(&mut item.thumbnail),
        "description" => Some(&mut item.description),
        _ => None,
    }
}

/// Reads an element's `value` attribute and stores it via `assign` on the current
/// item. Used for BGG's attribute-valued empty elements (yearpublished, minage…).
fn set_value(
    current: &mut Option<BggItem>,
    element: &quick_xml::events::BytesStart,
    assign: impl FnOnce(&mut BggItem, String),
) {
    if let Some(item) = current.as_mut() {
        if let Some((_, value)) = attributes(element)
            .into_iter()
            .find(|(key, _)| key == "value")
        {
            assign(item, value);
        }
    }
}

fn attributes(element: &quick_xml::events::BytesStart) -> Vec<(String, String)> {
    element
        .attributes()
        .filter_map(Result::ok)
        .filter_map(|attr| {
            let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).to_string();
            let value = attr
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()?
                .to_string();
            Some((key, value))
        })
        .collect()
}

fn bgg_candidate(item: &BggItem) -> Option<ExternalCandidate> {
    let id = item.id.clone()?;
    // Prefer the `primary` name; fall back to the first available.
    let title = item
        .names
        .iter()
        .find(|(name_type, _)| name_type == "primary")
        .or_else(|| item.names.first())
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())?;
    let path = if item.item_type.as_deref() == Some("boardgameexpansion") {
        "boardgameexpansion"
    } else {
        "boardgame"
    };
    let url = format!("https://boardgamegeek.com/{path}/{id}");

    let mut metadata = Map::new();
    metadata.insert("name".to_string(), Value::String(title.clone()));
    metadata.insert(
        "platform".to_string(),
        Value::Array(vec![Value::String("Boardgame".to_string())]),
    );
    if let Some(year) = item
        .year
        .as_deref()
        .filter(|year| !year.is_empty() && *year != "0")
    {
        metadata.insert("year".to_string(), Value::String(year.to_string()));
    }
    if let Some(players) = bgg_players(item) {
        metadata.insert("players".to_string(), Value::String(players));
    }
    if let Some(playtime) = item
        .playing_time
        .as_deref()
        .filter(|value| !value.is_empty() && *value != "0")
    {
        metadata.insert(
            "playtime".to_string(),
            Value::String(format!("{playtime} min")),
        );
    }
    if let Some(min_age) = item
        .min_age
        .as_deref()
        .filter(|value| !value.is_empty() && *value != "0")
    {
        metadata.insert("min_age".to_string(), Value::String(format!("{min_age}+")));
    }
    if let Some(score) = item
        .rating_average
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|score| *score > 0.0)
    {
        metadata.insert(
            "score".to_string(),
            serde_json::json!((score * 10.0).round() / 10.0),
        );
    }
    if let Some(count) = item
        .users_rated
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|count| *count > 0)
    {
        metadata.insert("score_count".to_string(), Value::Number(count.into()));
    }
    if let Some(rank) = item.rank.as_deref().filter(|value| !value.is_empty()) {
        metadata.insert("rank".to_string(), Value::String(rank.to_string()));
    }
    let description = item
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(description) = description {
        metadata.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    // Prefer the full image; fall back to the thumbnail when absent.
    let cover_url = item
        .image
        .as_deref()
        .or(item.thumbnail.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    for (field, link_type) in [
        ("designers", "boardgamedesigner"),
        ("artists", "boardgameartist"),
        ("publishers", "boardgamepublisher"),
        ("developers", "boardgamedeveloper"),
        ("categories", "boardgamecategory"),
    ] {
        let values: Vec<Value> = item
            .links
            .iter()
            .filter(|(item_link_type, _)| item_link_type == link_type)
            .map(|(_, value)| Value::String(value.clone()))
            .collect();
        if !values.is_empty() {
            metadata.insert(field.to_string(), Value::Array(values));
        }
    }
    if let Some(cover_url) = cover_url {
        metadata.insert(
            "cover_url".to_string(),
            Value::String(cover_url.to_string()),
        );
    }
    Some(ExternalCandidate {
        provider: BoardGameGeekProvider::ID.to_string(),
        source_id: id,
        url,
        original_title: Some(title.clone()),
        title,
        brief: description.map(str::to_string),
        cover_url: cover_url.map(str::to_string),
        titles: BTreeMap::new(),
        metadata,
    })
}

/// Formats the player count: "N players" or "N-M players" (omitting zeros).
fn bgg_players(item: &BggItem) -> Option<String> {
    let parse = |value: &Option<String>| {
        value
            .as_deref()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|count| *count > 0)
    };
    let min = parse(&item.min_players);
    let max = parse(&item.max_players);
    match (min, max) {
        (Some(min), Some(max)) if min != max => Some(format!("{min}-{max} players")),
        (Some(count), _) | (_, Some(count)) => Some(format!("{count} players")),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{bgg_candidate, bgg_id, parse_bgg_items};

    const THING_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<items>
  <item type="boardgame" id="13">
    <thumbnail>https://example.com/thumb.jpg</thumbnail>
    <image>https://example.com/catan.jpg</image>
    <name type="primary" sortindex="1" value="CATAN" />
    <name type="alternate" sortindex="1" value="Die Siedler von Catan" />
    <description>Trade, build &amp; settle.</description>
    <yearpublished value="1995" />
    <minplayers value="3" />
    <maxplayers value="4" />
    <playingtime value="120" />
    <minage value="10" />
    <link type="boardgamecategory" id="1015" value="Civilization" />
    <link type="boardgamedesigner" id="11" value="Klaus Teuber" />
    <link type="boardgamepublisher" id="93" value="Kosmos" />
    <statistics>
      <ratings>
        <average value="7.1" />
        <usersrated value="100000" />
        <ranks>
          <rank type="subtype" id="1" name="boardgame" value="450" />
          <rank type="family" id="5497" name="strategygames" value="200" />
        </ranks>
      </ratings>
    </statistics>
  </item>
</items>"#;

    #[test]
    fn parses_thing_into_candidate() {
        let items = parse_bgg_items(THING_XML);
        assert_eq!(items.len(), 1);
        let candidate = bgg_candidate(&items[0]).unwrap();

        assert_eq!(candidate.source_id, "13");
        assert_eq!(candidate.title, "CATAN");
        assert_eq!(
            candidate.cover_url.as_deref(),
            Some("https://example.com/catan.jpg")
        );
        let metadata = &candidate.metadata;
        assert_eq!(metadata.get("year"), Some(&serde_json::json!("1995")));
        assert_eq!(
            metadata.get("designers"),
            Some(&serde_json::json!(["Klaus Teuber"]))
        );
        assert_eq!(
            metadata.get("categories"),
            Some(&serde_json::json!(["Civilization"]))
        );
        assert_eq!(
            metadata.get("platform"),
            Some(&serde_json::json!(["Boardgame"]))
        );
        assert_eq!(
            metadata.get("description"),
            Some(&serde_json::json!("Trade, build & settle."))
        );
        assert_eq!(
            metadata.get("players"),
            Some(&serde_json::json!("3-4 players"))
        );
        assert_eq!(
            metadata.get("playtime"),
            Some(&serde_json::json!("120 min"))
        );
        assert_eq!(metadata.get("min_age"), Some(&serde_json::json!("10+")));
        assert_eq!(metadata.get("score"), Some(&serde_json::json!(7.1)));
        assert_eq!(
            metadata.get("score_count"),
            Some(&serde_json::json!(100000))
        );
        // Only the overall "boardgame" rank is captured, not the family rank.
        assert_eq!(metadata.get("rank"), Some(&serde_json::json!("450")));
    }

    #[test]
    fn id_parses_url_and_bare_id() {
        assert_eq!(
            bgg_id("https://boardgamegeek.com/boardgame/13/catan"),
            Some("13".to_string())
        );
        assert_eq!(
            bgg_id("https://boardgamegeek.com/boardgameexpansion/926/catan-seafarers"),
            Some("926".to_string())
        );
        assert_eq!(bgg_id("13"), Some("13".to_string()));
        assert_eq!(bgg_id("catan"), None);
    }
}
