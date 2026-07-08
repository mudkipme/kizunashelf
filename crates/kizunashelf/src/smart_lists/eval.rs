//! Tri-state evaluation of a filter tree against library records: definite
//! `Some(bool)` per supported atom, `None` for opaque nodes — which groups
//! ignore, so unsupported syntax never distorts results.

use super::model::{
    AtomKind, CompareOp, CompareValue, Conjunction, ContainsMode, DateBase, DateExpr, DateOffset,
    FieldRef, FilterNode,
};
use crate::daily_notes::normalize_wikilink_target;
use crate::dates::parsed_date_sort_key;
use crate::library::find_target;
use crate::types::{EntityRecord, Library};
use chrono::{DateTime, Months, NaiveDate, Utc};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Everything an evaluation needs beyond the record itself. `now`/`today` are
/// injected so `today() - "30d"`-style criteria are testable and consistent
/// across one evaluation pass.
pub struct EvalContext<'a> {
    pub library: &'a Library,
    pub tags_field: &'a str,
    pub today: NaiveDate,
    pub now: DateTime<Utc>,
    basename_index: HashMap<String, Vec<&'a EntityRecord>>,
}

impl<'a> EvalContext<'a> {
    pub fn new(library: &'a Library, now: DateTime<Utc>, today: NaiveDate) -> Self {
        EvalContext {
            library,
            tags_field: library.config.tags_field(),
            today,
            now,
            basename_index: crate::library::normalized_entity_basename_index(&library.records),
        }
    }
}

/// Tri-state evaluation of a filter node: `Some(bool)` for a definite answer,
/// `None` for *unknown* (an opaque node, or a group with only unknown
/// children). Unknown children are ignored by their group — this is what
/// "unsupported filters are ignored" means without letting an unsupported
/// condition inside an `or` swallow the whole vault.
pub fn eval_node(node: &FilterNode, record: &EntityRecord, ctx: &EvalContext) -> Option<bool> {
    match node {
        FilterNode::Opaque(_) => None,
        FilterNode::Expr(atom) => Some(eval_atom(&atom.kind, record, ctx) != atom.negated),
        FilterNode::Group {
            conjunction,
            children,
        } => {
            let mut any_true = false;
            let mut any_false = false;
            let mut any_known = false;
            for child in children {
                match eval_node(child, record, ctx) {
                    Some(true) => {
                        any_true = true;
                        any_known = true;
                    }
                    Some(false) => {
                        any_false = true;
                        any_known = true;
                    }
                    None => {}
                }
            }
            match conjunction {
                Conjunction::All => {
                    if any_false {
                        Some(false)
                    } else if children.is_empty() || any_known {
                        Some(true)
                    } else {
                        None
                    }
                }
                Conjunction::Any => {
                    if any_true {
                        Some(true)
                    } else if any_known {
                        Some(false)
                    } else {
                        None
                    }
                }
                Conjunction::NoneOf => {
                    if any_true {
                        Some(false)
                    } else if any_known {
                        Some(true)
                    } else {
                        None
                    }
                }
            }
        }
    }
}

/// Whether a record passes a filter — the top-level reading of the tri-state:
/// an unknown result is "no constraint", so the record is included.
pub fn record_matches(node: &FilterNode, record: &EntityRecord, ctx: &EvalContext) -> bool {
    eval_node(node, record, ctx) != Some(false)
}

fn eval_atom(kind: &AtomKind, record: &EntityRecord, ctx: &EvalContext) -> bool {
    match kind {
        AtomKind::InFolder { folder } => {
            let folder = folder.trim_matches('/');
            folder.is_empty() || record.summary.path.starts_with(&format!("{folder}/"))
        }
        AtomKind::HasTag { tags } => tags.iter().any(|wanted| {
            record
                .summary
                .tags
                .iter()
                .any(|tag| tag == wanted || tag.starts_with(&format!("{wanted}/")))
        }),
        AtomKind::HasLink { target } => eval_has_link(target, record, ctx),
        AtomKind::Compare { field, op, value } => eval_compare(field, *op, value, record, ctx),
        AtomKind::Contains {
            field,
            mode,
            values,
        } => {
            let matches = |wanted: &String| field_contains(field, wanted, record, ctx);
            match mode {
                ContainsMode::Any => values.iter().any(matches),
                ContainsMode::All => values.iter().all(matches),
            }
        }
        AtomKind::StartsWith { field, value } => {
            field_string(field, record).is_some_and(|s| s.starts_with(value.as_str()))
        }
        AtomKind::EndsWith { field, value } => {
            field_string(field, record).is_some_and(|s| s.ends_with(value.as_str()))
        }
        AtomKind::IsEmpty { field } => match field {
            FieldRef::FileName => record.summary.basename.is_empty(),
            FieldRef::FileMtime => record.file_modified_unix_nanos == 0,
            FieldRef::Note(name) => match note_value(name, record, ctx) {
                None | Some(serde_json::Value::Null) => true,
                Some(serde_json::Value::String(value)) => value.is_empty(),
                Some(serde_json::Value::Array(items)) => items.is_empty(),
                Some(_) => false,
            },
        },
    }
}

/// An outgoing wikilink/relation to `target` — matched through the resolved
/// relation graph (so frontmatter relations *and* body links count), by the
/// resolved entity when the target names one, else by the raw link text.
/// NFC-normalized, like all wikilink matching.
fn eval_has_link(target: &str, record: &EntityRecord, ctx: &EvalContext) -> bool {
    let wanted = normalize_wikilink_target(target);
    let resolved_id =
        find_target(target, None, &ctx.basename_index).map(|resolved| resolved.summary.id.clone());
    ctx.library
        .relations_from(&record.summary.id)
        .any(|relation| {
            relation.direction == crate::types::RelationDirection::Out
                && ((resolved_id.is_some() && relation.target_id == resolved_id)
                    || normalize_wikilink_target(&relation.target_title) == wanted)
        })
}

/// The frontmatter value a `note.<name>` reference reads. The built-in tags
/// field reads the *normalized* resident tag list rather than raw frontmatter,
/// consistent with the rest of the engine.
pub(super) fn note_value(
    name: &str,
    record: &EntityRecord,
    ctx: &EvalContext,
) -> Option<serde_json::Value> {
    if name == ctx.tags_field {
        return Some(serde_json::Value::Array(
            record
                .summary
                .tags
                .iter()
                .map(|tag| serde_json::Value::String(tag.clone()))
                .collect(),
        ));
    }
    record.frontmatter.get(name).cloned()
}

/// The string a text operation (`contains`, `startsWith`, …) reads for a field
/// reference; `None` for missing/non-scalar values.
fn field_string(field: &FieldRef, record: &EntityRecord) -> Option<String> {
    match field {
        FieldRef::FileName => Some(record.summary.basename.clone()),
        FieldRef::FileMtime => None,
        FieldRef::Note(name) => match record.frontmatter.get(name) {
            Some(serde_json::Value::String(value)) => Some(value.clone()),
            Some(serde_json::Value::Number(value)) => Some(value.to_string()),
            Some(serde_json::Value::Bool(value)) => Some(value.to_string()),
            _ => None,
        },
    }
}

/// `contains` semantics per the value's own type: membership on a list value,
/// substring on a string value (both case-sensitive, like Bases).
fn field_contains(
    field: &FieldRef,
    wanted: &str,
    record: &EntityRecord,
    ctx: &EvalContext,
) -> bool {
    if let FieldRef::Note(name) = field {
        if let Some(serde_json::Value::Array(items)) = note_value(name, record, ctx) {
            return items
                .iter()
                .any(|item| json_scalar_string(item).as_deref() == Some(wanted));
        }
    }
    field_string(field, record).is_some_and(|value| value.contains(wanted))
}

pub(super) fn json_scalar_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn eval_compare(
    field: &FieldRef,
    op: CompareOp,
    value: &CompareValue,
    record: &EntityRecord,
    ctx: &EvalContext,
) -> bool {
    match field {
        FieldRef::FileMtime => {
            let CompareValue::Date(date) = value else {
                return false;
            };
            if record.file_modified_unix_nanos == 0 {
                return false;
            }
            let Some(rhs) = resolve_instant(date, ctx) else {
                return false;
            };
            let Some(rhs_nanos) = rhs.timestamp_nanos_opt() else {
                return false;
            };
            op.compare((record.file_modified_unix_nanos as i128).cmp(&i128::from(rhs_nanos)))
        }
        FieldRef::FileName => compare_scalar(
            Some(serde_json::Value::String(record.summary.basename.clone())),
            op,
            value,
            ctx,
        ),
        FieldRef::Note(name) => compare_scalar(note_value(name, record, ctx), op, value, ctx),
    }
}

/// Compares a frontmatter value against a literal, in the literal's domain. A
/// missing/mistyped value is unequal: `==` → false, `!=` → true (matching
/// Bases, where `null != "x"` holds), ordering ops → false.
fn compare_scalar(
    actual: Option<serde_json::Value>,
    op: CompareOp,
    value: &CompareValue,
    ctx: &EvalContext,
) -> bool {
    let ordering = actual.and_then(|actual| match value {
        CompareValue::Number(rhs) => {
            let lhs = match &actual {
                serde_json::Value::Number(value) => value.as_f64(),
                serde_json::Value::String(value) => value.trim().parse::<f64>().ok(),
                _ => None,
            }?;
            lhs.partial_cmp(rhs)
        }
        CompareValue::Bool(rhs) => match actual {
            serde_json::Value::Bool(lhs) => Some(lhs.cmp(rhs)),
            _ => None,
        },
        CompareValue::String(rhs) => {
            let lhs = json_scalar_string(&actual)?;
            Some(lhs.as_str().cmp(rhs.as_str()))
        }
        CompareValue::Date(date) => {
            let lhs = json_scalar_string(&actual)
                .as_deref()
                .and_then(|value| parsed_date_sort_key(Some(value)))?;
            let rhs = resolve_naive_date(date, ctx)?.to_string();
            Some(lhs.cmp(&rhs))
        }
    });
    match ordering {
        Some(ordering) => op.compare(ordering),
        // No comparable value: only "not equal" is definitely true.
        None => op == CompareOp::Ne,
    }
}

/// Resolves a date expression to a calendar day (for `note.*` comparisons, day
/// granularity — our frontmatter dates are dates, not instants).
fn resolve_naive_date(date: &DateExpr, ctx: &EvalContext) -> Option<NaiveDate> {
    let base = match date.base {
        DateBase::Absolute(day) => day,
        DateBase::Today | DateBase::Now => ctx.today,
    };
    let mut datetime = base.and_hms_opt(0, 0, 0)?;
    for offset in &date.offsets {
        datetime = apply_offset_naive(datetime, offset)?;
    }
    Some(datetime.date())
}

fn apply_offset_naive(
    datetime: chrono::NaiveDateTime,
    offset: &DateOffset,
) -> Option<chrono::NaiveDateTime> {
    let months = Months::new(offset.duration.total_months());
    let shifted = if offset.negative {
        datetime.checked_sub_months(months)?
    } else {
        datetime.checked_add_months(months)?
    };
    let delta = offset.duration.sub_month_delta();
    if offset.negative {
        shifted.checked_sub_signed(delta)
    } else {
        shifted.checked_add_signed(delta)
    }
}

/// Resolves a date expression to an instant (for `file.mtime` comparisons).
/// Calendar-day bases resolve to midnight UTC.
fn resolve_instant(date: &DateExpr, ctx: &EvalContext) -> Option<DateTime<Utc>> {
    let mut instant = match date.base {
        DateBase::Now => ctx.now,
        DateBase::Today => ctx.today.and_hms_opt(0, 0, 0)?.and_utc(),
        DateBase::Absolute(day) => day.and_hms_opt(0, 0, 0)?.and_utc(),
    };
    for offset in &date.offsets {
        let months = Months::new(offset.duration.total_months());
        instant = if offset.negative {
            instant.checked_sub_months(months)?
        } else {
            instant.checked_add_months(months)?
        };
        let delta = offset.duration.sub_month_delta();
        instant = if offset.negative {
            instant.checked_sub_signed(delta)?
        } else {
            instant.checked_add_signed(delta)?
        };
    }
    Some(instant)
}
