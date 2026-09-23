//! Server-side serialization of an entity editor draft into frontmatter. The
//! editors (web, iOS) hold every value the way the user typed it — strings,
//! bools, and string lists — and send that draft verbatim; this module applies
//! the schema: trimming, empty-value dropping, numeric coercion for
//! progress/total-progress/rating fields, `[[wikilink]]` wrapping for relations,
//! and (for updates) the null-marking merge diff that deletes cleared keys.
//! This used to live in each client (`frontmatterPatch` on the web,
//! `EntityDraft.frontmatter` on iOS); it lives here so every runtime serializes
//! edits identically.

use crate::types::{EntityTypeConfig, FieldType};
use regex::Regex;
use serde_json::{Map, Number, Value};
use std::sync::OnceLock;

/// Normalize an editor draft into the concrete frontmatter for a *new* entity.
/// Empty values are dropped rather than written.
pub(super) fn normalize_draft(
    draft: Map<String, Value>,
    type_config: &EntityTypeConfig,
) -> Map<String, Value> {
    normalize(draft, type_config, None)
}

/// Build the merge patch for an *update*: the normalized draft, plus `null` for
/// every currently-present key the draft no longer carries a value for — so the
/// merge deletes cleared fields instead of keeping their stale values.
pub(super) fn draft_update_patch(
    draft: Map<String, Value>,
    type_config: &EntityTypeConfig,
    current: &Map<String, Value>,
) -> Map<String, Value> {
    let mut patch = normalize(draft, type_config, Some(current));
    for key in current.keys() {
        if !patch.contains_key(key) {
            patch.insert(key.clone(), Value::Null);
        }
    }
    patch
}

pub(super) fn normalize_existing_draft(
    draft: Map<String, Value>,
    type_config: &EntityTypeConfig,
    current: &Map<String, Value>,
) -> Map<String, Value> {
    normalize(draft, type_config, Some(current))
}

fn normalize(
    draft: Map<String, Value>,
    type_config: &EntityTypeConfig,
    current: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    let mut result = Map::new();
    for (key, value) in draft {
        let field_type = type_config
            .fields
            .iter()
            .find(|field| field.field == key)
            .map(|field| field.field_type);
        let current_value = current.and_then(|map| map.get(&key));
        if let Some(normalized) = normalize_value(value, field_type, current_value) {
            result.insert(key, normalized);
        }
    }
    result
}

/// Normalize one draft value per the field's schema type; `None` means "empty,
/// drop the key" (which the update diff then turns into a deletion).
fn normalize_value(
    value: Value,
    field_type: Option<FieldType>,
    current: Option<&Value>,
) -> Option<Value> {
    match value {
        Value::Null => None,
        Value::Bool(_) | Value::Number(_) | Value::Object(_) => Some(value),
        Value::Array(items) => {
            let relation = field_type == Some(FieldType::Relation);
            let normalized: Vec<Value> = items
                .into_iter()
                .filter_map(|item| match item {
                    Value::String(text) => {
                        let trimmed = text.trim();
                        if trimmed.is_empty() {
                            None
                        } else if relation {
                            Some(Value::String(to_wikilink(trimmed)))
                        } else {
                            Some(Value::String(trimmed.to_string()))
                        }
                    }
                    Value::Null => None,
                    other => Some(other),
                })
                .collect();
            (!normalized.is_empty()).then_some(Value::Array(normalized))
        }
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            if field_type == Some(FieldType::Relation) {
                return Some(Value::String(to_wikilink(trimmed)));
            }
            if matches!(field_type, Some(FieldType::Number | FieldType::Rating)) {
                return Some(coerce_number(trimmed));
            }
            // Editors that flatten values to text (iOS) round-trip an untouched
            // number/bool as its string form; writing that back would silently
            // retype the field (`year: 2020` → `year: "2020"`). If the string is
            // exactly the current value's canonical form, keep the typed value.
            if let Some(current) = current {
                if matches!(current, Value::Number(_) | Value::Bool(_))
                    && canonical_string(current).as_deref() == Some(trimmed)
                {
                    return Some(current.clone());
                }
            }
            Some(Value::String(trimmed.to_string()))
        }
    }
}

/// Parse a numeric field's text into a JSON number when the number round-trips
/// to the same text (so `"08"` or `"1e3"` stay strings rather than being
/// silently rewritten), else keep the text.
fn coerce_number(text: &str) -> Value {
    if let Ok(int) = text.parse::<i64>() {
        if int.to_string() == text {
            return Value::Number(Number::from(int));
        }
    }
    if let Ok(float) = text.parse::<f64>() {
        if let Some(number) = Number::from_f64(float) {
            if number.to_string() == text {
                return Value::Number(number);
            }
        }
    }
    Value::String(text.to_string())
}

fn canonical_string(value: &Value) -> Option<String> {
    match value {
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// `target` → `[[target]]`, tolerating values that already arrive wrapped (or
/// with an `[[target|alias]]` alias, which is reduced to the bare target).
fn to_wikilink(value: &str) -> String {
    format!("[[{}]]", strip_wikilink(value))
}

fn strip_wikilink(value: &str) -> &str {
    static WIKILINK: OnceLock<Regex> = OnceLock::new();
    let regex = WIKILINK
        .get_or_init(|| Regex::new(r"^\[\[(.*?)(?:\|.*?)?\]\]$").expect("static wikilink regex"));
    match regex.captures(value.trim()) {
        Some(captures) => captures
            .get(1)
            .expect("group 1 is unconditional")
            .as_str()
            .trim(),
        None => value.trim(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FieldConfig;
    use serde_json::json;

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
            date_role: None,
            season_language: None,
            external_ref: None,
            external_types: Vec::new(),
            relation_type: None,
        }
    }

    fn config(fields: Vec<FieldConfig>) -> EntityTypeConfig {
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

    fn draft(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => panic!("draft fixture must be an object"),
        }
    }

    #[test]
    fn numeric_fields_coerce_round_trippable_text() {
        let config = config(vec![
            field("progress", FieldType::Number),
            field("rating", FieldType::Rating),
            field("total", FieldType::Number),
        ]);
        let normalized = normalize_draft(
            draft(json!({
                "progress": "12",
                "rating": "8.5",
                "total": " 24 ",
            })),
            &config,
        );
        assert_eq!(normalized["progress"], json!(12));
        assert_eq!(normalized["rating"], json!(8.5));
        assert_eq!(normalized["total"], json!(24));
    }

    #[test]
    fn non_round_trippable_numeric_text_stays_text() {
        let config = config(vec![field("rating", FieldType::Rating)]);
        let normalized = normalize_draft(draft(json!({ "rating": "S-tier" })), &config);
        assert_eq!(normalized["rating"], json!("S-tier"));
        let padded = normalize_draft(draft(json!({ "rating": "08" })), &config);
        assert_eq!(padded["rating"], json!("08"));
    }

    #[test]
    fn relations_are_wrapped_without_double_bracketing() {
        let config = config(vec![field("sequel", FieldType::Relation)]);
        let normalized = normalize_draft(
            draft(json!({ "sequel": ["Frieren S2", "[[Frieren S1]]", "  "] })),
            &config,
        );
        assert_eq!(
            normalized["sequel"],
            json!(["[[Frieren S2]]", "[[Frieren S1]]"])
        );
        let scalar = normalize_draft(draft(json!({ "sequel": "[[Frieren S1|alias]]" })), &config);
        assert_eq!(scalar["sequel"], json!("[[Frieren S1]]"));
    }

    #[test]
    fn empty_values_are_dropped_and_deleted_on_update() {
        let config = config(vec![field("note", FieldType::Text)]);
        let current = draft(json!({ "note": "old", "kept": "value", "gone": 3 }));
        let patch = draft_update_patch(
            draft(json!({ "note": "  ", "kept": "value" })),
            &config,
            &current,
        );
        // Cleared and absent keys are marked for deletion; kept keys pass through.
        assert_eq!(patch["note"], Value::Null);
        assert_eq!(patch["gone"], Value::Null);
        assert_eq!(patch["kept"], json!("value"));
    }

    #[test]
    fn stringified_numbers_keep_their_current_type_on_update() {
        let config = config(vec![field("title", FieldType::Title)]);
        let current = draft(json!({ "year": 2020, "flag": true, "title": "A" }));
        let patch = draft_update_patch(
            draft(json!({ "year": "2020", "flag": "true", "title": "A" })),
            &config,
            &current,
        );
        assert_eq!(patch["year"], json!(2020));
        assert_eq!(patch["flag"], json!(true));
        // A genuinely edited value stays what the user typed.
        let edited = draft_update_patch(draft(json!({ "year": "2021" })), &config, &current);
        assert_eq!(edited["year"], json!("2021"));
    }

    #[test]
    fn booleans_and_lists_pass_through() {
        let config = config(vec![field("tags", FieldType::TextList)]);
        let normalized = normalize_draft(
            draft(json!({ "done": true, "tags": [" a ", "", "b"] })),
            &config,
        );
        assert_eq!(normalized["done"], json!(true));
        assert_eq!(normalized["tags"], json!(["a", "b"]));
        let empty_list = normalize_draft(draft(json!({ "tags": ["", " "] })), &config);
        assert!(!empty_list.contains_key("tags"));
    }
}
