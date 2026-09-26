//! Portable rating semantics. A missing scale is never inferred from a score.
use crate::types::{EntityRating, EntityTypeConfig, FieldConfig, FieldType};
use serde_json::{Map, Value};

pub fn number(value: &Value) -> Option<f64> {
    let value = match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) if !value.trim().is_empty() => value.trim().parse().ok(),
        _ => None,
    }?;
    value.is_finite().then_some(value)
}

pub fn resolve_ratings(
    config: &EntityTypeConfig,
    frontmatter: &Map<String, Value>,
) -> Vec<EntityRating> {
    config
        .fields
        .iter()
        .filter(|field| field.field_type == FieldType::Rating)
        .map(|field| EntityRating {
            field: field.field.clone(),
            label: field
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(&field.field)
                .to_string(),
            value: frontmatter.get(&field.field).and_then(number),
            max: field.rating_max,
        })
        .collect()
}

/// Imports normalize source ratings to ten first. Legacy unscaled fields retain
/// that established representation; declared scales explicitly convert it.
pub fn from_ten(score: f64, field: &FieldConfig) -> Option<f64> {
    (score.is_finite() && (0.0..=10.0).contains(&score))
        .then(|| score * (field.rating_max.unwrap_or(10.0) / 10.0))
}

pub fn validate(value: f64, max: Option<f64>) -> Result<(), &'static str> {
    if !value.is_finite() {
        return Err("A rating must be a finite number");
    }
    if let Some(max) = max {
        if !(0.0..=max).contains(&value) {
            return Err("The rating is outside the configured scale");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn empty_and_non_numeric_values_are_not_zero() {
        for value in [
            json!(""),
            json!("  "),
            json!(true),
            json!(null),
            json!("NaN"),
            json!([]),
        ] {
            assert_eq!(number(&value), None);
        }
        assert_eq!(number(&json!(0)), Some(0.0));
        assert_eq!(number(&json!("8.25")), Some(8.25));
    }
    #[test]
    fn scales_are_explicit_and_scores_are_not_rounded() {
        let field: FieldConfig = serde_json::from_value(
            json!({"field":"arbitrary", "fieldType":"rating", "ratingMax":5}),
        )
        .unwrap();
        assert_eq!(from_ten(8.5, &field), Some(4.25));
        assert!(validate(5.5, Some(5.0)).is_err());
        assert!(validate(5.5, Some(10.0)).is_ok());
        assert!(validate(87.25, None).is_ok());
    }
}
