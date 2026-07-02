//! Lifecycle **status** — the schema-driven concept behind up-next/recent feed
//! accuracy, log flips, and cross-type "ongoing" shelves.
//!
//! Meaning flows from the field's `enumRole: status`, never its name (the two
//! invariants). This module is transport-free: it resolves an entity's canonical
//! status from its type config + frontmatter, and answers the write-side question
//! "what value sets canonical X" via [`StatusValues`]. The canonical set, the
//! `planning→ongoing→completed` rank, and the option↔canonical maps live on the
//! contract types in [`crate::types`]; the resolution/lookup logic lives here.

use crate::types::{EntityTypeConfig, EnumRole, FieldConfig, ResolvedStatus};
use serde_json::{Map, Value};

/// The type's status field: the first `enum` field with `enumRole: status`
/// (first-wins, mirroring `stamp_target_field` for date roles). Config validation
/// rejects a second one, so in a valid config there is at most one.
pub fn status_field(type_config: &EntityTypeConfig) -> Option<&FieldConfig> {
    type_config
        .fields
        .iter()
        .find(|field| field.enum_role == Some(EnumRole::Status))
}

/// The raw status value from frontmatter for a given field name — a trimmed,
/// non-empty string, or `None`. Enum values are stored as plain string scalars.
fn frontmatter_status_value(frontmatter: &Map<String, Value>, field: &str) -> Option<String> {
    match frontmatter.get(field) {
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        _ => None,
    }
}

/// Resolve an entity's status from its type config + frontmatter.
///
/// `None` when the type has no status field or the entity carries no value for it.
/// Otherwise `ResolvedStatus { field, value, canonical }` where `canonical` is the
/// reverse lookup into the field's `statusValues` (`None` when the value is
/// unmapped — the value is still reported and preserved).
pub fn resolve_status(
    type_config: &EntityTypeConfig,
    frontmatter: &Map<String, Value>,
) -> Option<ResolvedStatus> {
    let field = status_field(type_config)?;
    let value = frontmatter_status_value(frontmatter, &field.field)?;
    let canonical = field
        .status_values
        .as_ref()
        .and_then(|values| values.canonical_of(&value));
    Some(ResolvedStatus {
        field: field.field.clone(),
        value,
        canonical,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CanonicalStatus, FieldType, StatusValues};

    fn status_field_config() -> FieldConfig {
        FieldConfig {
            field: "状态".to_string(),
            field_type: FieldType::Enum,
            display_name: None,
            title_language: None,
            title_role: None,
            external_fields: Vec::new(),
            enum_options: vec![
                "想看".to_string(),
                "在看".to_string(),
                "看完".to_string(),
                "抛弃".to_string(),
            ],
            enum_role: Some(EnumRole::Status),
            status_values: Some(StatusValues {
                planning: vec!["想看".to_string()],
                ongoing: vec!["在看".to_string()],
                completed: vec!["看完".to_string(), "刷过".to_string()],
                dropped: vec!["抛弃".to_string()],
            }),
            total_progress_field: None,
            date_role: None,
            season_language: None,
            external_ref: None,
            external_types: Vec::new(),
            relation_type: None,
        }
    }

    fn type_config_with(fields: Vec<FieldConfig>) -> EntityTypeConfig {
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

    fn frontmatter(pairs: &[(&str, &str)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
            .collect()
    }

    #[test]
    fn resolves_mapped_value_to_canonical() {
        let type_config = type_config_with(vec![status_field_config()]);
        let resolved = resolve_status(&type_config, &frontmatter(&[("状态", "在看")])).unwrap();
        assert_eq!(resolved.field, "状态");
        assert_eq!(resolved.value, "在看");
        assert_eq!(resolved.canonical, Some(CanonicalStatus::Ongoing));
    }

    #[test]
    fn second_mapped_option_still_resolves_to_its_canonical() {
        let type_config = type_config_with(vec![status_field_config()]);
        // 刷过 is the second option under `completed`; only the first is the write
        // target, but any listed option resolves to the canonical.
        let resolved = resolve_status(&type_config, &frontmatter(&[("状态", "刷过")])).unwrap();
        assert_eq!(resolved.canonical, Some(CanonicalStatus::Completed));
    }

    #[test]
    fn unmapped_value_is_reported_with_no_canonical() {
        let type_config = type_config_with(vec![status_field_config()]);
        let resolved = resolve_status(&type_config, &frontmatter(&[("状态", "重看中")])).unwrap();
        assert_eq!(resolved.value, "重看中");
        assert_eq!(resolved.canonical, None);
    }

    #[test]
    fn no_status_field_resolves_to_none() {
        let mut field = status_field_config();
        field.enum_role = None;
        let type_config = type_config_with(vec![field]);
        assert!(resolve_status(&type_config, &frontmatter(&[("状态", "在看")])).is_none());
    }

    #[test]
    fn missing_or_blank_value_resolves_to_none() {
        let type_config = type_config_with(vec![status_field_config()]);
        assert!(resolve_status(&type_config, &frontmatter(&[])).is_none());
        assert!(resolve_status(&type_config, &frontmatter(&[("状态", "  ")])).is_none());
    }

    #[test]
    fn first_status_field_wins() {
        let mut second = status_field_config();
        second.field = "second".to_string();
        let type_config = type_config_with(vec![status_field_config(), second]);
        assert_eq!(status_field(&type_config).unwrap().field, "状态");
    }

    #[test]
    fn plain_status_field_without_mapping_resolves_canonical_none() {
        let mut field = status_field_config();
        field.status_values = None;
        let type_config = type_config_with(vec![field]);
        let resolved = resolve_status(&type_config, &frontmatter(&[("状态", "在看")])).unwrap();
        assert_eq!(resolved.value, "在看");
        assert_eq!(resolved.canonical, None);
    }

    #[test]
    fn write_value_is_first_option() {
        let values = status_field_config().status_values.unwrap();
        assert_eq!(values.write_value(CanonicalStatus::Completed), Some("看完"));
        assert_eq!(values.write_value(CanonicalStatus::Ongoing), Some("在看"));
    }

    #[test]
    fn rank_orders_progression_and_excludes_dropped() {
        assert!(CanonicalStatus::Planning.rank() < CanonicalStatus::Ongoing.rank());
        assert!(CanonicalStatus::Ongoing.rank() < CanonicalStatus::Completed.rank());
        assert_eq!(CanonicalStatus::Dropped.rank(), None);
    }
}
