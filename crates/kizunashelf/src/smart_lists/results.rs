//! A smart list's results: filter through the evaluator, sort by the view's
//! keys (schema-aware date keys, absent-last), truncate to its limit.

use super::eval::{json_scalar_string, note_value, record_matches, EvalContext};
use super::model::{SmartList, SmartView, SortProperty, ViewSort};
use crate::dates::parsed_date_sort_key;
use crate::library::compare_string_for_title_language;
use crate::relations::SortDirection;
use crate::types::EntityRecord;
use std::cmp::Ordering;

// ---------------------------------------------------------------------------
// Results: filter → sort → limit
// ---------------------------------------------------------------------------

/// The records a smart list view resolves to: global filters AND the view's
/// own filters, sorted by the view's sort (title-ish `file.name` ascending when
/// unset), truncated to the view's `limit`. Pagination is the caller's.
pub fn smart_list_records<'a>(
    list: &SmartList,
    view: Option<&SmartView>,
    ctx: &EvalContext<'a>,
    title_language: Option<&str>,
) -> Vec<&'a EntityRecord> {
    let mut records: Vec<&EntityRecord> = ctx
        .library
        .records
        .iter()
        .filter(|record| {
            record_matches(&list.filters, record, ctx)
                && view
                    .and_then(|view| view.filters.as_ref())
                    .is_none_or(|filters| record_matches(filters, record, ctx))
        })
        .collect();

    let default_sort = [ViewSort {
        property: SortProperty::FileName,
        direction: SortDirection::Asc,
    }];
    let sort: &[ViewSort] = match view {
        Some(view) if !view.sort.is_empty() => &view.sort,
        _ => &default_sort,
    };
    sort_smart_records(&mut records, sort, ctx, title_language);

    if let Some(limit) = view.and_then(|view| view.limit) {
        records.truncate(limit as usize);
    }
    records
}

/// One record's value under a sort key, in a domain-ordered shape: absent
/// always sorts last (independent of direction), numbers before strings when a
/// field is mixed across records.
enum SortValue {
    Number(f64),
    Text(String),
}

fn sort_smart_records(
    records: &mut [&EntityRecord],
    sort: &[ViewSort],
    ctx: &EvalContext,
    title_language: Option<&str>,
) {
    let explicit_language = title_language
        .map(str::trim)
        .filter(|language| !language.is_empty() && *language != "default");
    records.sort_by(|a, b| {
        for key in sort {
            let ordering = compare_by_sort_key(a, b, key, ctx, explicit_language);
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        // Stable final tie-break so pagination is deterministic.
        compare_title(a, b, explicit_language)
    });
}

fn compare_by_sort_key(
    a: &EntityRecord,
    b: &EntityRecord,
    key: &ViewSort,
    ctx: &EvalContext,
    language: Option<&str>,
) -> Ordering {
    let directed = |ordering: Ordering| match key.direction {
        SortDirection::Asc => ordering,
        SortDirection::Desc => ordering.reverse(),
    };
    match &key.property {
        SortProperty::Unsupported(_) => Ordering::Equal,
        // In-app, `file.name` orders like the library's title sort (localized
        // titles honored); Obsidian orders by the raw file name — the basename
        // is the canonical title, so the two agree except across languages.
        SortProperty::FileName => directed(compare_title(a, b, language)),
        SortProperty::FileMtime => match (a.file_modified_unix_nanos, b.file_modified_unix_nanos) {
            (0, 0) => Ordering::Equal,
            (0, _) => Ordering::Greater,
            (_, 0) => Ordering::Less,
            (a, b) => directed(a.cmp(&b)),
        },
        SortProperty::Note(field) => {
            let value_a = note_sort_value(field, a, ctx);
            let value_b = note_sort_value(field, b, ctx);
            match (value_a, value_b) {
                (None, None) => Ordering::Equal,
                // Absent values pin to the bottom regardless of direction.
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => directed(compare_sort_values(&a, &b)),
            }
        }
    }
}

fn compare_sort_values(a: &SortValue, b: &SortValue) -> Ordering {
    match (a, b) {
        (SortValue::Number(a), SortValue::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (SortValue::Number(_), SortValue::Text(_)) => Ordering::Less,
        (SortValue::Text(_), SortValue::Number(_)) => Ordering::Greater,
        (SortValue::Text(a), SortValue::Text(b)) => compare_string_for_title_language(a, b, None),
    }
}

/// A record's sort value for a `note.<field>` key. Schema-declared date fields
/// sort by the normalized date key (so fuzzy values like `2024 Spring` order
/// correctly); everything else sorts by its own value type.
fn note_sort_value(field: &str, record: &EntityRecord, ctx: &EvalContext) -> Option<SortValue> {
    let is_date_field = ctx
        .library
        .config
        .type_config(&record.summary.entity_type)
        .is_some_and(|type_config| {
            type_config.fields.iter().any(|field_config| {
                field_config.field == field
                    && field_config.field_type == crate::types::FieldType::Date
            })
        });
    let value = note_value(field, record, ctx)?;
    if is_date_field {
        return json_scalar_string(&value)
            .as_deref()
            .and_then(|value| parsed_date_sort_key(Some(value)))
            .map(SortValue::Text);
    }
    match value {
        serde_json::Value::Number(value) => value.as_f64().map(SortValue::Number),
        serde_json::Value::Bool(value) => Some(SortValue::Number(if value { 1.0 } else { 0.0 })),
        serde_json::Value::String(value) if !value.is_empty() => Some(SortValue::Text(value)),
        _ => None,
    }
}

fn compare_title(a: &EntityRecord, b: &EntityRecord, language: Option<&str>) -> Ordering {
    let title = |record: &EntityRecord| -> String {
        language
            .and_then(|language| record.summary.titles.get(language))
            .unwrap_or(&record.summary.title)
            .clone()
    };
    compare_string_for_title_language(&title(a), &title(b), language)
}
