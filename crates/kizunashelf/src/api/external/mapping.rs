//! Schema-driven mapping of an external candidate onto an entity type's fields
//! and body sections. This is the single source of truth for *what value* a
//! provider field becomes once the schema's field type is applied — field-type
//! normalization (scalar vs list), `externalRef`→url, the title/cover fallbacks,
//! the scalar-flatten of a list, body-section Markdown, and the date→season
//! conversion. Every runtime
//! (web, desktop, iOS) consumes the result instead of re-deriving it, so they
//! can't drift. Clients add only the human-facing label (a presentation concern)
//! and the merge of a body section into live editor text.

use crate::contract::{ExternalCandidate, ExternalMatch, MappedBodySection, MappedFieldValue};
use crate::dates::parse_entity_date;
use crate::types::{BodySectionKind, EntityTypeConfig, FieldConfig, FieldType, SeasonLanguage};
use serde_json::Value;
use std::collections::HashSet;

/// Resolve a candidate against a type's schema, returning the candidate plus its
/// field and body previews.
pub(super) fn match_candidate(
    candidate: ExternalCandidate,
    type_config: &EntityTypeConfig,
) -> ExternalMatch {
    let fields = map_fields(&candidate, type_config);
    let body_sections = map_body_sections(&candidate, type_config);
    ExternalMatch {
        entity_type: type_config.id.clone(),
        candidate,
        fields,
        body_sections,
        // The caller resolves this against the whole library (a candidate for one
        // type can already exist under another), so mapping stays library-free.
        existing: None,
    }
}

fn map_fields(
    candidate: &ExternalCandidate,
    type_config: &EntityTypeConfig,
) -> Vec<MappedFieldValue> {
    let mut entries = Vec::new();
    let mut used = HashSet::new();
    for field in &type_config.fields {
        let Some(mapped) = mapped_value_for_field(candidate, field) else {
            continue;
        };
        // Field names are unique within a type, but guard against a malformed
        // config that repeats one (only the first mapping wins, as on the web).
        if !used.insert(field.field.as_str()) {
            continue;
        }
        let value = normalize_value_for_field(field, mapped.value);
        entries.push(MappedFieldValue {
            field: field.field.clone(),
            has_value: value_has_content(&value),
            value,
            source: mapped.source,
            external_field: mapped.external_field,
        });
    }
    entries
}

struct MappedRaw {
    source: String,
    external_field: Option<String>,
    value: Value,
}

fn mapped_value_for_field(candidate: &ExternalCandidate, field: &FieldConfig) -> Option<MappedRaw> {
    // An externalRef field carries the candidate URL of its own provider.
    if field.field_type == FieldType::ExternalRef
        && external_source_matches(&candidate.provider, field.external_ref.as_deref())
    {
        return Some(MappedRaw {
            source: candidate.provider.clone(),
            external_field: None,
            value: Value::String(candidate.url.clone()),
        });
    }

    // The first external-field mapping whose source matches the candidate's
    // provider. A present, non-null metadata value wins outright.
    let mapping = field
        .external_fields
        .iter()
        .find(|mapping| external_source_matches(&candidate.provider, Some(&mapping.source)));
    if let Some(mapping) = mapping {
        if let Some(value) = candidate.metadata.get(&mapping.field) {
            if !value.is_null() {
                return Some(MappedRaw {
                    source: mapping.source.clone(),
                    external_field: Some(mapping.field.clone()),
                    value: value.clone(),
                });
            }
        }
    }

    // No mapped value: fall back to the candidate's own title/cover, keyed off
    // the schema (a title field's language, an image field's nature).
    if let Some(fallback) = field_fallback_value(candidate, field) {
        return Some(fallback);
    }

    // A mapping matched but carried no value: still surface an empty entry so the
    // UI shows the field matched this provider (disabled, "no value returned").
    mapping.map(|mapping| MappedRaw {
        source: mapping.source.clone(),
        external_field: Some(mapping.field.clone()),
        value: Value::Null,
    })
}

/// Schema-driven defaults drawn from the candidate's own fields when no external
/// mapping resolved: a title field takes its language's title from the
/// candidate's `titles`; an image field takes the candidate's cover.
fn field_fallback_value(candidate: &ExternalCandidate, field: &FieldConfig) -> Option<MappedRaw> {
    let value = match field.field_type {
        FieldType::Title => {
            let title = candidate.titles.get(field.title_language.as_deref()?)?;
            non_empty(title)?
        }
        FieldType::Image | FieldType::ImageList => non_empty(candidate.cover_url.as_deref()?)?,
        _ => return None,
    };
    Some(MappedRaw {
        source: candidate.provider.clone(),
        external_field: None,
        value: Value::String(value),
    })
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| value.to_string())
}

fn normalize_value_for_field(field: &FieldConfig, value: Value) -> Value {
    if field.field_type == FieldType::Season {
        // Providers usually expose a date (e.g. `first_air_date`), not a season,
        // so coerce each value into a season label in the field's language.
        // Season is a list field, so the result is always a list.
        let language = field.season_language.unwrap_or_default();
        let items = match value {
            Value::Array(items) => items,
            other => vec![other],
        };
        let mapped = items
            .iter()
            .filter_map(|item| coerce_season_value(item, language))
            .collect();
        return Value::Array(mapped);
    }

    if is_list_field_type(field.field_type) {
        // A list field keeps an external list as-is; a scalar is wrapped into a
        // single-item list. An empty/absent scalar yields an empty list (rather
        // than `[null]`) so the entry reads as "no value".
        return match value {
            Value::Array(items) => Value::Array(items),
            Value::Null => Value::Array(Vec::new()),
            Value::String(text) if text.trim().is_empty() => Value::Array(Vec::new()),
            other => Value::Array(vec![other]),
        };
    }

    // A scalar field can't hold a list-shaped external value (e.g. genres), so
    // flatten it to comma-separated text rather than writing a YAML array.
    if let Value::Array(items) = value {
        let joined = items
            .iter()
            .filter_map(value_to_join_text)
            .collect::<Vec<_>>()
            .join(", ");
        return Value::String(joined);
    }
    value
}

/// Convert a date (`2026-04-03`) or an existing season label (in any language)
/// into a season string in `language`. Returns `None` for empty/null values;
/// values with no derivable season (e.g. a bare year, or free text) are kept
/// as-is so hand-curated or unexpected strings survive.
fn coerce_season_value(value: &Value, language: SeasonLanguage) -> Option<Value> {
    let text = match value {
        Value::Null => return None,
        Value::String(text) => text.trim(),
        other => return Some(other.clone()),
    };
    if text.is_empty() {
        return None;
    }
    if let Some(parsed) = parse_entity_date(Some(text)) {
        if let Some(season_key) = parsed.season_key.as_deref() {
            return Some(Value::String(format_season_label(
                parsed.year,
                season_key,
                language,
            )));
        }
    }
    Some(value.clone())
}

/// Render `<season> <year>` in the requested language. Mirrors the editor's
/// `formatSeasonValue` (web) / `formatSeasonValue` (iOS) so a coerced value
/// round-trips with what the editors produce.
fn format_season_label(year: i32, season_key: &str, language: SeasonLanguage) -> String {
    let label = season_label(season_key, language);
    match language {
        SeasonLanguage::En => format!("{label} {year}"),
        // Chinese keeps the 季 suffix (春季); Japanese drops it (春).
        SeasonLanguage::Zh | SeasonLanguage::Ja => format!("{year}年{label}"),
    }
}

fn season_label(season_key: &str, language: SeasonLanguage) -> &'static str {
    match (language, season_key) {
        (SeasonLanguage::Zh, "winter") => "冬季",
        (SeasonLanguage::Zh, "spring") => "春季",
        (SeasonLanguage::Zh, "summer") => "夏季",
        (SeasonLanguage::Zh, "autumn") => "秋季",
        (SeasonLanguage::Ja, "winter") => "冬",
        (SeasonLanguage::Ja, "spring") => "春",
        (SeasonLanguage::Ja, "summer") => "夏",
        (SeasonLanguage::Ja, "autumn") => "秋",
        (SeasonLanguage::En, "winter") => "Winter",
        (SeasonLanguage::En, "spring") => "Spring",
        (SeasonLanguage::En, "summer") => "Summer",
        (SeasonLanguage::En, "autumn") => "Autumn",
        // `parse_entity_date` only emits the four keys above; fall back to spring
        // (which always resolves) for any unexpected key rather than panicking.
        (language, _) => season_label("spring", language),
    }
}

fn map_body_sections(
    candidate: &ExternalCandidate,
    type_config: &EntityTypeConfig,
) -> Vec<MappedBodySection> {
    let mut entries = Vec::new();
    // One heading can be filled from multiple sources; emit a preview per
    // external field whose source matches the candidate's provider.
    for section in &type_config.body_sections {
        if section.kind != BodySectionKind::External {
            continue;
        }
        for external in &section.external_fields {
            if !external_source_matches(&candidate.provider, Some(&external.source)) {
                continue;
            }
            let markdown = format_external_body_value(candidate.metadata.get(&external.field));
            entries.push(MappedBodySection {
                key: format!("{}:{}:{}", section.heading, external.source, external.field),
                heading: section.heading.clone(),
                source: external.source.clone(),
                external_field: external.field.clone(),
                has_value: !markdown.trim().is_empty(),
                markdown,
            });
        }
    }
    entries
}

fn format_external_body_value(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| format_external_body_value(Some(item)))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(boolean)) => boolean.to_string(),
        Some(other) => format!(
            "```json\n{}\n```",
            serde_json::to_string_pretty(other).unwrap_or_default()
        ),
    }
}

fn is_list_field_type(field_type: FieldType) -> bool {
    matches!(
        field_type,
        FieldType::ImageList
            | FieldType::EnumList
            | FieldType::Season
            | FieldType::Relation
            | FieldType::TextList
    )
}

/// Whether a resolved value carries content. Drives default field selection in
/// the match UI (an empty entry stays visible but unchecked).
fn value_has_content(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        _ => true,
    }
}

/// The string form of a value when flattening a list onto a scalar field; `None`
/// drops null/empty/complex items from the comma-joined text.
fn value_to_join_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if text.is_empty() => None,
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        _ => None,
    }
}

/// Case-insensitive match of a configured source id against a candidate's
/// provider. An absent/blank source never matches.
fn external_source_matches(provider: &str, source: Option<&str>) -> bool {
    match source.map(str::trim) {
        Some(source) if !source.is_empty() => provider == source.to_lowercase(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ExternalFieldMapping;
    use serde_json::{json, Map};

    fn candidate(provider: &str, metadata: Value) -> ExternalCandidate {
        ExternalCandidate {
            provider: provider.to_string(),
            source_id: "1".to_string(),
            url: "https://example.test/1".to_string(),
            title: "Star Voyager".to_string(),
            original_title: None,
            brief: None,
            cover_url: None,
            titles: Default::default(),
            metadata: match metadata {
                Value::Object(map) => map,
                _ => Map::new(),
            },
        }
    }

    fn field(name: &str, field_type: FieldType) -> FieldConfig {
        FieldConfig {
            field: name.to_string(),
            field_type,
            display_name: None,
            title_language: None,
            title_role: None,
            external_fields: Vec::new(),
            enum_options: Vec::new(),
            enum_role: None,
            status_values: None,
            total_progress_field: None,
            date_role: None,
            season_language: None,
            external_ref: None,
            external_types: Vec::new(),
            relation_type: None,
        }
    }

    fn mapped(field: &str, source: &str, provider_field: &str) -> FieldConfig {
        let mut config = self::field(field, FieldType::Text);
        config.external_fields = vec![ExternalFieldMapping {
            source: source.to_string(),
            field: provider_field.to_string(),
        }];
        config
    }

    fn type_with(fields: Vec<FieldConfig>) -> EntityTypeConfig {
        EntityTypeConfig {
            id: "anime".to_string(),
            label: "Anime".to_string(),
            icon: None,
            path: "Anime".to_string(),
            external_priority: Vec::new(),
            filename: None,
            body_sections: Vec::new(),
            log: None,
            fields,
        }
    }

    fn field_value(
        candidate: &ExternalCandidate,
        type_config: &EntityTypeConfig,
        name: &str,
    ) -> Value {
        map_fields(candidate, type_config)
            .into_iter()
            .find(|entry| entry.field == name)
            .map(|entry| entry.value)
            .unwrap_or(Value::Null)
    }

    #[test]
    fn date_maps_to_a_season_list_in_the_field_language() {
        let mut zh = mapped("aired", "tmdb", "first_air_date");
        zh.field_type = FieldType::Season;
        zh.season_language = Some(SeasonLanguage::Zh);
        let mut ja = zh.clone();
        ja.field = "aired_ja".to_string();
        ja.season_language = Some(SeasonLanguage::Ja);
        let mut en = zh.clone();
        en.field = "aired_en".to_string();
        en.season_language = Some(SeasonLanguage::En);
        // The same provider field feeds all three; give each its own mapping.
        let cand = candidate("tmdb", json!({ "first_air_date": "2026-04-03" }));
        let config = type_with(vec![zh, ja, en]);

        assert_eq!(field_value(&cand, &config, "aired"), json!(["2026年春季"]));
        assert_eq!(field_value(&cand, &config, "aired_ja"), json!(["2026年春"]));
        assert_eq!(
            field_value(&cand, &config, "aired_en"),
            json!(["Spring 2026"])
        );
    }

    #[test]
    fn an_existing_season_label_is_reformatted_into_the_field_language() {
        let mut english = mapped("aired", "tmdb", "season");
        english.field_type = FieldType::Season;
        english.season_language = Some(SeasonLanguage::Zh);
        let cand = candidate("tmdb", json!({ "season": "Spring 2026" }));
        let config = type_with(vec![english]);
        assert_eq!(field_value(&cand, &config, "aired"), json!(["2026年春季"]));
    }

    #[test]
    fn an_unconvertible_season_value_is_preserved() {
        let mut season = mapped("aired", "tmdb", "when");
        season.field_type = FieldType::Season;
        let cand = candidate("tmdb", json!({ "when": "sometime in 2026" }));
        let config = type_with(vec![season]);
        // A bare year has no derivable season; keep the original string.
        assert_eq!(
            field_value(&cand, &config, "aired"),
            json!(["sometime in 2026"])
        );
    }

    #[test]
    fn a_list_field_wraps_a_scalar_and_keeps_a_list() {
        let mut genres = mapped("genres", "tmdb", "tags");
        genres.field_type = FieldType::EnumList;
        let config = type_with(vec![genres]);
        assert_eq!(
            field_value(
                &candidate("tmdb", json!({ "tags": "SF" })),
                &config,
                "genres"
            ),
            json!(["SF"])
        );
        assert_eq!(
            field_value(
                &candidate("tmdb", json!({ "tags": ["SF", "Space"] })),
                &config,
                "genres"
            ),
            json!(["SF", "Space"])
        );
    }

    #[test]
    fn an_absent_list_value_is_an_empty_list_not_null() {
        let mut genres = mapped("genres", "tmdb", "tags");
        genres.field_type = FieldType::EnumList;
        let config = type_with(vec![genres]);
        let entries = map_fields(&candidate("tmdb", json!({})), &config);
        let entry = entries
            .iter()
            .find(|entry| entry.field == "genres")
            .unwrap();
        assert_eq!(entry.value, json!([]));
        assert!(!entry.has_value);
    }

    #[test]
    fn a_list_value_flattens_onto_a_scalar_field_as_comma_text() {
        let config = type_with(vec![mapped("studio", "tmdb", "studios")]);
        let cand = candidate("tmdb", json!({ "studios": ["A", "", "B"] }));
        assert_eq!(field_value(&cand, &config, "studio"), json!("A, B"));
    }

    #[test]
    fn an_external_ref_field_maps_to_the_candidate_url() {
        let mut id = field("tmdb_id", FieldType::ExternalRef);
        id.external_ref = Some("tmdb".to_string());
        let config = type_with(vec![id]);
        let cand = candidate("tmdb", json!({}));
        assert_eq!(
            field_value(&cand, &config, "tmdb_id"),
            json!("https://example.test/1")
        );
        // A different provider's candidate doesn't fill it.
        assert_eq!(
            field_value(&candidate("igdb", json!({})), &config, "tmdb_id"),
            Value::Null
        );
    }

    #[test]
    fn only_fields_for_the_candidate_provider_are_mapped() {
        let config = type_with(vec![
            mapped("name", "tmdb", "title"),
            mapped("alias", "bangumi", "name"),
        ]);
        let entries = map_fields(&candidate("tmdb", json!({ "title": "SV" })), &config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].field, "name");
        assert_eq!(entries[0].external_field.as_deref(), Some("title"));
    }

    #[test]
    fn a_title_field_falls_back_to_the_candidates_title_for_its_language() {
        let mut name = field("name_jp", FieldType::Title);
        name.title_language = Some("ja".to_string());
        let mut cand = candidate("bangumi", json!({}));
        cand.titles
            .insert("ja".to_string(), "スターボイジャー".to_string());
        let config = type_with(vec![name]);
        assert_eq!(
            field_value(&cand, &config, "name_jp"),
            json!("スターボイジャー")
        );
    }

    #[test]
    fn image_fields_fall_back_to_the_candidate_cover() {
        let config = type_with(vec![
            field("cover", FieldType::Image),
            field("covers", FieldType::ImageList),
        ]);
        let mut cand = candidate("tmdb", json!({}));
        cand.cover_url = Some("https://img.test/c.jpg".to_string());
        assert_eq!(
            field_value(&cand, &config, "cover"),
            json!("https://img.test/c.jpg")
        );
        // imageList wraps the single cover into a list.
        assert_eq!(
            field_value(&cand, &config, "covers"),
            json!(["https://img.test/c.jpg"])
        );
    }

    #[test]
    fn a_mapped_value_beats_the_title_fallback() {
        let mut name = mapped("name_jp", "bangumi", "name");
        name.field_type = FieldType::Title;
        name.title_language = Some("ja".to_string());
        let mut cand = candidate("bangumi", json!({ "name": "Mapped Name" }));
        cand.titles
            .insert("ja".to_string(), "Fallback Name".to_string());
        let config = type_with(vec![name]);
        assert_eq!(field_value(&cand, &config, "name_jp"), json!("Mapped Name"));
    }

    #[test]
    fn body_sections_render_matching_provider_fields_to_markdown() {
        let mut config = type_with(Vec::new());
        config.body_sections = vec![crate::types::BodySection {
            heading: "Summary".to_string(),
            kind: BodySectionKind::External,
            external_fields: vec![ExternalFieldMapping {
                source: "tmdb".to_string(),
                field: "overview".to_string(),
            }],
            tracking: None,
        }];
        let cand = candidate("tmdb", json!({ "overview": "A long voyage." }));
        let sections = map_body_sections(&cand, &config);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].key, "Summary:tmdb:overview");
        assert_eq!(sections[0].markdown, "A long voyage.");
        assert!(sections[0].has_value);
    }
}
