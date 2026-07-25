//! The expression printer: one supported atom → its canonical Bases text,
//! the exact inverse of [`super::expr`] (round-trip covered by tests).

use super::model::{
    AtomKind, CompareValue, ContainsMode, DateBase, DateExpr, DurationSpec, FieldRef,
};

// ---------------------------------------------------------------------------
// Expression printing
// ---------------------------------------------------------------------------

fn escape_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn print_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn print_field(field: &FieldRef) -> String {
    match field {
        FieldRef::Note(name) => {
            let dotted = name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if dotted {
                format!("note.{name}")
            } else {
                format!("note[{}]", escape_string(name))
            }
        }
        FieldRef::FileName => "file.name".to_string(),
        FieldRef::FileMtime => "file.mtime".to_string(),
    }
}

fn print_duration(spec: &DurationSpec) -> String {
    let parts = [
        (spec.years, "y"),
        (spec.months, "M"),
        (spec.weeks, "w"),
        (spec.days, "d"),
        (spec.hours, "h"),
        (spec.minutes, "m"),
        (spec.seconds, "s"),
    ];
    let rendered: Vec<String> = parts
        .into_iter()
        .filter(|(amount, _)| *amount > 0)
        .map(|(amount, unit)| format!("{amount}{unit}"))
        .collect();
    if rendered.is_empty() {
        "0d".to_string()
    } else {
        rendered.join(" ")
    }
}

fn print_date(date: &DateExpr) -> String {
    let mut out = match date.base {
        DateBase::Absolute(day) => format!("date({})", escape_string(&day.to_string())),
        DateBase::Today => "today()".to_string(),
        DateBase::Now => "now()".to_string(),
    };
    for offset in &date.offsets {
        let sign = if offset.negative { "-" } else { "+" };
        out.push_str(&format!(
            " {sign} {}",
            escape_string(&print_duration(&offset.duration))
        ));
    }
    out
}

fn print_value(value: &CompareValue) -> String {
    match value {
        CompareValue::String(value) => escape_string(value),
        CompareValue::Number(value) => print_number(*value),
        CompareValue::Bool(value) => value.to_string(),
        CompareValue::Date(date) => print_date(date),
    }
}

fn print_string_args(values: &[String]) -> String {
    values
        .iter()
        .map(|value| escape_string(value))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The canonical Bases text of a supported atom — what the writer emits into a
/// file for programmatically-built filters.
pub fn print_atom(kind: &AtomKind, negated: bool) -> String {
    let body = match kind {
        AtomKind::InFolder { folder } => format!("file.inFolder({})", escape_string(folder)),
        AtomKind::HasTag { tags } => format!("file.hasTag({})", print_string_args(tags)),
        AtomKind::HasLink { field, target } => match field {
            Some(field) => format!(
                "{}.contains(link({}))",
                print_field(&FieldRef::Note(field.clone())),
                escape_string(target)
            ),
            None => format!("file.hasLink({})", escape_string(target)),
        },
        AtomKind::Compare { field, op, value } => format!(
            "{} {} {}",
            print_field(field),
            op.symbol(),
            print_value(value)
        ),
        AtomKind::Contains {
            field,
            mode,
            values,
        } => {
            let method = match (mode, values.len()) {
                (ContainsMode::Any, 1) => "contains",
                (ContainsMode::Any, _) => "containsAny",
                (ContainsMode::All, _) => "containsAll",
            };
            format!(
                "{}.{method}({})",
                print_field(field),
                print_string_args(values)
            )
        }
        AtomKind::StartsWith { field, value } => {
            format!(
                "{}.startsWith({})",
                print_field(field),
                escape_string(value)
            )
        }
        AtomKind::EndsWith { field, value } => {
            format!("{}.endsWith({})", print_field(field), escape_string(value))
        }
        AtomKind::IsEmpty { field } => format!("{}.isEmpty()", print_field(field)),
    };
    if negated {
        format!("!{body}")
    } else {
        body
    }
}
