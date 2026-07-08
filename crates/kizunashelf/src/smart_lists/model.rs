//! The smart-list data model: the filter tree ([`FilterNode`]) and its
//! supported expression shapes ([`AtomKind`]), the view model, and the
//! type-scope idiom. Pure vocabulary — parsing, printing, and evaluation live
//! in the sibling modules.

use super::print::print_atom;
use crate::relations::SortDirection;
use crate::types::KizunaConfig;
use chrono::{NaiveDate, TimeDelta};
use serde_yaml::{Mapping, Value};
use std::cmp::Ordering;

// ---------------------------------------------------------------------------
// Filter model
// ---------------------------------------------------------------------------

/// How a group combines its children — Bases `and:` / `or:` / `not:`. `not`
/// follows Obsidian's "none of the subfilters may match".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conjunction {
    All,
    Any,
    NoneOf,
}

impl Conjunction {
    pub(super) fn yaml_key(self) -> &'static str {
        match self {
            Conjunction::All => "and",
            Conjunction::Any => "or",
            Conjunction::NoneOf => "not",
        }
    }
}

/// One node of a filter tree: a group, a parsed (supported) expression, or an
/// opaque value we don't understand — kept verbatim so a rewrite never destroys
/// hand-authored syntax, and evaluated as *unknown* (ignored).
#[derive(Clone, Debug, PartialEq)]
pub enum FilterNode {
    Group {
        conjunction: Conjunction,
        children: Vec<FilterNode>,
    },
    Expr(FilterAtom),
    Opaque(Value),
}

impl FilterNode {
    /// An empty `and` group — the "no constraints" filter.
    pub fn empty() -> Self {
        FilterNode::Group {
            conjunction: Conjunction::All,
            children: Vec::new(),
        }
    }

    pub fn is_empty_group(&self) -> bool {
        matches!(self, FilterNode::Group { children, .. } if children.is_empty())
    }
}

/// A parsed, supported filter expression. `raw` is the exact expression string
/// serialized back into the file: for atoms parsed from a file it is the
/// original text (so a no-op rewrite is byte-stable), and for atoms built
/// programmatically it comes from [`print_atom`].
#[derive(Clone, Debug, PartialEq)]
pub struct FilterAtom {
    pub kind: AtomKind,
    /// Logical negation (`!expr`). Comparisons are never negated — a parsed
    /// `!(a == b)` flips to `!=` instead.
    pub negated: bool,
    pub raw: String,
}

impl FilterAtom {
    /// Builds an atom programmatically, deriving its canonical `raw` text.
    pub fn new(kind: AtomKind, negated: bool) -> Self {
        let raw = print_atom(&kind, negated);
        FilterAtom { kind, negated, raw }
    }
}

/// The supported filter-expression shapes (the KizunaShelf profile of Bases).
#[derive(Clone, Debug, PartialEq)]
pub enum AtomKind {
    /// `file.inFolder("Media/Anime")` — the type-scope idiom.
    InFolder { folder: String },
    /// `file.hasTag("a", "b")` — any listed tag (nested `a/b` tags included).
    HasTag { tags: Vec<String> },
    /// `file.hasLink("Name")` — an outgoing wikilink/relation to the target.
    HasLink { target: String },
    /// `note.field <op> <literal>` — value-driven comparison.
    Compare {
        field: FieldRef,
        op: CompareOp,
        value: CompareValue,
    },
    /// `note.field.contains("x")` / `.containsAny(…)` / `.containsAll(…)` —
    /// list membership on list values, substring on string values.
    Contains {
        field: FieldRef,
        mode: ContainsMode,
        values: Vec<String>,
    },
    /// `note.field.startsWith("x")`.
    StartsWith { field: FieldRef, value: String },
    /// `note.field.endsWith("x")`.
    EndsWith { field: FieldRef, value: String },
    /// `note.field.isEmpty()` — missing, null, `""`, or `[]`.
    IsEmpty { field: FieldRef },
}

/// What a supported expression reads: a frontmatter property or one of the two
/// supported `file.*` properties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldRef {
    /// A frontmatter key — `note.x`, `note["画像"]`, or the bare shorthand `x`.
    Note(String),
    FileName,
    FileMtime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
}

impl CompareOp {
    pub(super) fn flipped(self) -> Self {
        match self {
            CompareOp::Eq => CompareOp::Ne,
            CompareOp::Ne => CompareOp::Eq,
            CompareOp::Gt => CompareOp::Lte,
            CompareOp::Gte => CompareOp::Lt,
            CompareOp::Lt => CompareOp::Gte,
            CompareOp::Lte => CompareOp::Gt,
        }
    }

    pub(super) fn symbol(self) -> &'static str {
        match self {
            CompareOp::Eq => "==",
            CompareOp::Ne => "!=",
            CompareOp::Gt => ">",
            CompareOp::Gte => ">=",
            CompareOp::Lt => "<",
            CompareOp::Lte => "<=",
        }
    }

    pub(super) fn compare(self, ordering: Ordering) -> bool {
        match self {
            CompareOp::Eq => ordering == Ordering::Equal,
            CompareOp::Ne => ordering != Ordering::Equal,
            CompareOp::Gt => ordering == Ordering::Greater,
            CompareOp::Gte => ordering != Ordering::Less,
            CompareOp::Lt => ordering == Ordering::Less,
            CompareOp::Lte => ordering != Ordering::Greater,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainsMode {
    Any,
    All,
}

/// The right-hand side of a comparison. The literal's own type picks the
/// comparison domain (Bases semantics) — never the field's name or schema.
#[derive(Clone, Debug, PartialEq)]
pub enum CompareValue {
    String(String),
    Number(f64),
    Bool(bool),
    Date(DateExpr),
}

/// A date expression: `date("2026-01-01")`, `today()`, `now()`, each optionally
/// offset by duration literals (`today() - "90d"`).
#[derive(Clone, Debug, PartialEq)]
pub struct DateExpr {
    pub base: DateBase,
    pub offsets: Vec<DateOffset>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DateBase {
    Absolute(NaiveDate),
    Today,
    Now,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DateOffset {
    pub negative: bool,
    pub duration: DurationSpec,
}

/// A parsed Bases duration literal like `"90d"` or `"1M 4h"`. Calendar parts
/// (years/months) apply as calendar arithmetic; the rest as fixed spans.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DurationSpec {
    pub years: u32,
    pub months: u32,
    pub weeks: u32,
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
}

impl DurationSpec {
    pub(super) fn total_months(&self) -> u32 {
        self.years.saturating_mul(12).saturating_add(self.months)
    }

    pub(super) fn sub_month_delta(&self) -> TimeDelta {
        TimeDelta::days(i64::from(self.weeks) * 7 + i64::from(self.days))
            + TimeDelta::hours(i64::from(self.hours))
            + TimeDelta::minutes(i64::from(self.minutes))
            + TimeDelta::seconds(i64::from(self.seconds))
    }
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

/// The app view a Bases view type maps to: `table` ⇔ the list rows, `cards` ⇔
/// the grid. Other Bases view types (`list`, `map`, plugin types) are skipped
/// with a warning and preserved in the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewLayout {
    List,
    Grid,
}

impl ViewLayout {
    pub(super) fn base_type(self) -> &'static str {
        match self {
            ViewLayout::List => "table",
            ViewLayout::Grid => "cards",
        }
    }

    pub(super) fn from_base_type(value: &str) -> Option<Self> {
        match value {
            "table" => Some(ViewLayout::List),
            "cards" => Some(ViewLayout::Grid),
            _ => None,
        }
    }
}

/// What a view sorts on. Unsupported properties are kept in the document but
/// skipped by the comparator (with a warning).
#[derive(Clone, Debug, PartialEq)]
pub enum SortProperty {
    Note(String),
    FileName,
    FileMtime,
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewSort {
    pub property: SortProperty,
    pub direction: SortDirection,
}

/// One supported view parsed out of the document's `views:` sequence.
/// `source_index` points back at the underlying YAML entry so mutations can
/// edit it in place (preserving unknown keys like `order`/`cardSize`).
#[derive(Clone, Debug, PartialEq)]
pub struct SmartView {
    pub layout: ViewLayout,
    pub name: String,
    pub filters: Option<FilterNode>,
    pub sort: Vec<ViewSort>,
    pub limit: Option<u64>,
    /// Cards image property reference (e.g. `note.cover`), verbatim.
    pub image: Option<String>,
    pub source_index: usize,
}

/// A parsed `.base` document: the raw YAML tree (the round-trip source of
/// truth) plus the recognized filters/views and the human-readable warnings for
/// everything that was ignored.
#[derive(Clone, Debug)]
pub struct SmartList {
    pub doc: Mapping,
    pub filters: FilterNode,
    pub views: Vec<SmartView>,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Type scope
// ---------------------------------------------------------------------------

/// The vault folder a type's entities live in — the `file.inFolder` target
/// that scopes a smart list to that type.
pub fn type_scope_folder(config: &KizunaConfig, type_id: &str) -> Option<String> {
    let type_config = config.type_config(type_id)?;
    Some(format!(
        "{}/{}",
        config.taxonomy_root.trim_matches('/'),
        type_config.path.trim_matches('/')
    ))
}

/// Recognizes the type-scope idiom: a direct `file.inFolder(...)` child of the
/// top-level `and` group whose folder is a configured type's folder. Returns
/// the type id and the child's index (so an editor can hide that rule).
pub fn scope_from_filters(node: &FilterNode, config: &KizunaConfig) -> Option<(String, usize)> {
    let FilterNode::Group {
        conjunction: Conjunction::All,
        children,
    } = node
    else {
        return None;
    };
    children.iter().enumerate().find_map(|(index, child)| {
        let FilterNode::Expr(FilterAtom {
            kind: AtomKind::InFolder { folder },
            negated: false,
            ..
        }) = child
        else {
            return None;
        };
        let folder = folder.trim_matches('/');
        config
            .types
            .iter()
            .find(|type_config| {
                type_scope_folder(config, &type_config.id).as_deref() == Some(folder)
            })
            .map(|type_config| (type_config.id.clone(), index))
    })
}
