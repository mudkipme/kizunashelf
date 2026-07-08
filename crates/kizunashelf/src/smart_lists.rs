//! Smart lists: dynamic, criteria-based lists stored as **Obsidian Bases**
//! `.base` YAML files in `KizunaShelf/Lists/`, next to the static Markdown
//! lists — "files over apps" style, openable in Obsidian.
//!
//! The compatibility contract is one-way: every file *we* write uses only
//! well-supported Bases constructs, so Obsidian renders the identical result
//! set. Hand-edited files are parsed leniently — anything we don't understand
//! becomes an [`FilterNode::Opaque`] node that evaluates to *unknown* (see the
//! tri-state logic on [`eval_node`]) and is preserved **verbatim** on every
//! rewrite, because mutations edit the raw YAML document tree
//! ([`SmartList::doc`]) rather than re-serializing a lossy model.
//!
//! Like [`crate::lists`], this module is pure — no I/O and no knowledge of the
//! API contract. The handlers in `api/smart_lists.rs` compose it with the
//! [`Vfs`](crate::vfs::Vfs) and the cached [`Library`].
//!
//! Evaluation is deliberately *value-driven*, exactly like Bases: a comparison
//! is a date comparison because the right-hand side is a date expression, and
//! numeric because the literal is a number — never because of what a field is
//! called. The schema is consulted only for the built-in tags field name and
//! for sort-key derivation.

use crate::daily_notes::normalize_wikilink_target;
use crate::dates::parsed_date_sort_key;
use crate::library::{compare_string_for_title_language, find_target};
use crate::relations::SortDirection;
use crate::types::{EntityRecord, KizunaConfig, Library};
use chrono::{DateTime, Months, NaiveDate, TimeDelta, Utc};
use regex::Regex;
use serde_yaml::{Mapping, Value};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::OnceLock;

/// File extension of a smart list. Smart lists live in the same directory as
/// static lists ([`crate::lists::LISTS_DIR`]); the extension is the kind.
pub const SMART_LIST_EXTENSION: &str = "base";

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
    fn yaml_key(self) -> &'static str {
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
    fn flipped(self) -> Self {
        match self {
            CompareOp::Eq => CompareOp::Ne,
            CompareOp::Ne => CompareOp::Eq,
            CompareOp::Gt => CompareOp::Lte,
            CompareOp::Gte => CompareOp::Lt,
            CompareOp::Lt => CompareOp::Gte,
            CompareOp::Lte => CompareOp::Gt,
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            CompareOp::Eq => "==",
            CompareOp::Ne => "!=",
            CompareOp::Gt => ">",
            CompareOp::Gte => ">=",
            CompareOp::Lt => "<",
            CompareOp::Lte => "<=",
        }
    }

    fn compare(self, ordering: Ordering) -> bool {
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
    fn total_months(&self) -> u32 {
        self.years.saturating_mul(12).saturating_add(self.months)
    }

    fn sub_month_delta(&self) -> TimeDelta {
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
    fn base_type(self) -> &'static str {
        match self {
            ViewLayout::List => "table",
            ViewLayout::Grid => "cards",
        }
    }

    fn from_base_type(value: &str) -> Option<Self> {
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
// Document parse / render
// ---------------------------------------------------------------------------

/// Parses a `.base` file. Invalid YAML (or a non-mapping document) is an error;
/// everything else parses, with unrecognized constructs collected into
/// `warnings` and preserved in `doc`.
pub fn parse_smart_list(raw: &str) -> Result<SmartList, String> {
    let doc = if raw.trim().is_empty() {
        Mapping::new()
    } else {
        match serde_yaml::from_str::<Value>(raw) {
            Ok(Value::Mapping(mapping)) => mapping,
            Ok(Value::Null) => Mapping::new(),
            Ok(_) => return Err("smart list file is not a YAML mapping".to_string()),
            Err(error) => return Err(format!("invalid YAML: {error}")),
        }
    };

    let mut warnings = Vec::new();
    let filters = match doc.get("filters") {
        Some(value) => parse_filter_value(value, &mut warnings),
        None => FilterNode::empty(),
    };
    let views = parse_views(&doc, &mut warnings);
    if doc.get("groupBy").is_some() {
        warnings.push("Grouping (groupBy) is not supported and is ignored".to_string());
    }

    Ok(SmartList {
        doc,
        filters,
        views,
        warnings,
    })
}

/// Serializes a document tree back to YAML text.
pub fn render_smart_list(doc: &Mapping) -> String {
    serde_yaml::to_string(&Value::Mapping(doc.clone())).unwrap_or_default()
}

/// Parses one `filters` value: a group mapping (`and:`/`or:`/`not:` over a
/// sequence) or a single expression string. Anything else is opaque.
fn parse_filter_value(value: &Value, warnings: &mut Vec<String>) -> FilterNode {
    match value {
        Value::String(expression) => parse_expression_node(expression, warnings),
        Value::Mapping(mapping) if mapping.len() == 1 => {
            let (key, children) = mapping.iter().next().expect("len checked");
            let conjunction = match key.as_str() {
                Some("and") => Conjunction::All,
                Some("or") => Conjunction::Any,
                Some("not") => Conjunction::NoneOf,
                _ => {
                    warnings.push(format!("Ignored filter: {}", yaml_summary(value)));
                    return FilterNode::Opaque(value.clone());
                }
            };
            let Value::Sequence(items) = children else {
                warnings.push(format!("Ignored filter: {}", yaml_summary(value)));
                return FilterNode::Opaque(value.clone());
            };
            FilterNode::Group {
                conjunction,
                children: items
                    .iter()
                    .map(|item| parse_filter_value(item, warnings))
                    .collect(),
            }
        }
        _ => {
            warnings.push(format!("Ignored filter: {}", yaml_summary(value)));
            FilterNode::Opaque(value.clone())
        }
    }
}

fn parse_expression_node(expression: &str, warnings: &mut Vec<String>) -> FilterNode {
    match parse_expression(expression) {
        Some((kind, negated)) => FilterNode::Expr(FilterAtom {
            kind,
            negated,
            raw: expression.to_string(),
        }),
        None => {
            warnings.push(format!("Ignored filter: {expression}"));
            FilterNode::Opaque(Value::String(expression.to_string()))
        }
    }
}

/// One-line rendering of a YAML value for a warning message.
fn yaml_summary(value: &Value) -> String {
    serde_yaml::to_string(value)
        .unwrap_or_default()
        .trim()
        .replace('\n', " ")
}

/// Serializes a filter node back to its YAML form. Parsed expressions emit
/// their `raw` text (byte-stable round-trip); opaque nodes emit verbatim.
pub fn filter_node_to_yaml(node: &FilterNode) -> Value {
    match node {
        FilterNode::Group {
            conjunction,
            children,
        } => {
            let mut mapping = Mapping::new();
            mapping.insert(
                Value::String(conjunction.yaml_key().to_string()),
                Value::Sequence(children.iter().map(filter_node_to_yaml).collect()),
            );
            Value::Mapping(mapping)
        }
        FilterNode::Expr(atom) => Value::String(atom.raw.clone()),
        FilterNode::Opaque(value) => value.clone(),
    }
}

fn parse_views(doc: &Mapping, warnings: &mut Vec<String>) -> Vec<SmartView> {
    let Some(Value::Sequence(entries)) = doc.get("views") else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Value::Mapping(mapping) = entry else {
            warnings.push(format!("Ignored view: {}", yaml_summary(entry)));
            continue;
        };
        let view_type = mapping
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let name = mapping
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(view_type)
            .to_string();
        let Some(layout) = ViewLayout::from_base_type(view_type) else {
            warnings.push(format!(
                "Ignored view \"{name}\": unsupported type \"{view_type}\""
            ));
            continue;
        };
        let filters = mapping
            .get("filters")
            .map(|value| parse_filter_value(value, warnings));
        let sort = parse_view_sort(mapping.get("sort"), &name, warnings);
        let limit = mapping.get("limit").and_then(Value::as_u64);
        let image = mapping
            .get("image")
            .and_then(Value::as_str)
            .map(str::to_string);
        views.push(SmartView {
            layout,
            name,
            filters,
            sort,
            limit,
            image,
            source_index: index,
        });
    }
    views
}

fn parse_view_sort(value: Option<&Value>, view: &str, warnings: &mut Vec<String>) -> Vec<ViewSort> {
    let Some(Value::Sequence(entries)) = value else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let mapping = entry.as_mapping()?;
            let property = mapping.get("property").and_then(Value::as_str)?;
            let direction = match mapping.get("direction").and_then(Value::as_str) {
                Some(direction) if direction.eq_ignore_ascii_case("desc") => SortDirection::Desc,
                _ => SortDirection::Asc,
            };
            let property = parse_sort_property(property);
            if let SortProperty::Unsupported(raw) = &property {
                warnings.push(format!(
                    "Ignored sort on \"{raw}\" in view \"{view}\": unsupported property"
                ));
            }
            Some(ViewSort {
                property,
                direction,
            })
        })
        .collect()
}

/// Parses a Bases sort-property reference (`note.<field>`, bare `<field>`,
/// `file.name`, `file.mtime`).
pub fn parse_sort_property(raw: &str) -> SortProperty {
    match raw {
        "file.name" => SortProperty::FileName,
        "file.mtime" => SortProperty::FileMtime,
        _ => {
            if let Some(field) = raw.strip_prefix("note.") {
                SortProperty::Note(field.to_string())
            } else if raw.contains('.') {
                // Any other dotted reference (file.*, formula.*) is unsupported.
                SortProperty::Unsupported(raw.to_string())
            } else {
                SortProperty::Note(raw.to_string())
            }
        }
    }
}

pub fn print_sort_property(property: &SortProperty) -> String {
    match property {
        SortProperty::Note(field) => format!("note.{field}"),
        SortProperty::FileName => "file.name".to_string(),
        SortProperty::FileMtime => "file.mtime".to_string(),
        SortProperty::Unsupported(raw) => raw.clone(),
    }
}

// ---------------------------------------------------------------------------
// Document mutation (round-trip preserving)
// ---------------------------------------------------------------------------

/// Replaces the document's global `filters`. An empty group removes the key.
pub fn set_global_filters(doc: &mut Mapping, node: &FilterNode) {
    if node.is_empty_group() {
        doc.remove("filters");
    } else {
        doc.insert(
            Value::String("filters".to_string()),
            filter_node_to_yaml(node),
        );
    }
}

/// One view as supplied by an editor for a full views rewrite.
#[derive(Clone, Debug)]
pub struct ViewSpec {
    pub layout: ViewLayout,
    pub name: String,
    pub filters: Option<FilterNode>,
    pub sort: Vec<ViewSort>,
    pub limit: Option<u64>,
    pub image: Option<String>,
}

/// Rewrites the document's `views:` from `specs`, preserving what the editor
/// doesn't own: a spec matching an existing supported view **by name** mutates
/// that entry in place (keeping unknown keys like `order`, `cardSize`,
/// `columnSize`), unmatched specs become fresh entries, and every *unsupported*
/// view entry (`list`, `map`, plugin types) is retained after the specs in its
/// original order. Supported views absent from `specs` are removed — that is
/// the editor deleting a tab.
pub fn apply_views(doc: &mut Mapping, specs: &[ViewSpec]) {
    let originals = match doc.get("views") {
        Some(Value::Sequence(entries)) => entries.clone(),
        _ => Vec::new(),
    };
    // Split the original entries into reusable supported views (matched by
    // name below) and unsupported entries that are always kept.
    let mut reusable: Vec<Option<Mapping>> = Vec::new();
    let mut unsupported: Vec<Value> = Vec::new();
    for entry in originals {
        match &entry {
            Value::Mapping(mapping)
                if mapping
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|value| ViewLayout::from_base_type(value).is_some()) =>
            {
                reusable.push(Some(mapping.clone()));
            }
            _ => unsupported.push(entry),
        }
    }

    let mut entries: Vec<Value> = specs
        .iter()
        .map(|spec| {
            let existing = reusable.iter_mut().find_map(|slot| {
                let matches = slot.as_ref().is_some_and(|mapping| {
                    mapping.get("name").and_then(Value::as_str) == Some(spec.name.as_str())
                });
                if matches {
                    slot.take()
                } else {
                    None
                }
            });
            Value::Mapping(apply_view_spec(existing.unwrap_or_default(), spec))
        })
        .collect();
    entries.append(&mut unsupported);
    doc.insert(Value::String("views".to_string()), Value::Sequence(entries));
}

fn apply_view_spec(mut mapping: Mapping, spec: &ViewSpec) -> Mapping {
    mapping.insert(
        Value::String("type".to_string()),
        Value::String(spec.layout.base_type().to_string()),
    );
    mapping.insert(
        Value::String("name".to_string()),
        Value::String(spec.name.clone()),
    );
    match &spec.filters {
        Some(filters) if !filters.is_empty_group() => {
            mapping.insert(
                Value::String("filters".to_string()),
                filter_node_to_yaml(filters),
            );
        }
        _ => {
            mapping.remove("filters");
        }
    }
    if spec.sort.is_empty() {
        mapping.remove("sort");
    } else {
        let entries: Vec<Value> = spec
            .sort
            .iter()
            .map(|sort| {
                let mut entry = Mapping::new();
                entry.insert(
                    Value::String("property".to_string()),
                    Value::String(print_sort_property(&sort.property)),
                );
                entry.insert(
                    Value::String("direction".to_string()),
                    Value::String(
                        match sort.direction {
                            SortDirection::Asc => "ASC",
                            SortDirection::Desc => "DESC",
                        }
                        .to_string(),
                    ),
                );
                Value::Mapping(entry)
            })
            .collect();
        mapping.insert(Value::String("sort".to_string()), Value::Sequence(entries));
    }
    match spec.limit {
        Some(limit) => {
            mapping.insert(
                Value::String("limit".to_string()),
                Value::Number(limit.into()),
            );
        }
        None => {
            mapping.remove("limit");
        }
    }
    // The image property only makes sense on cards views.
    match (&spec.image, spec.layout) {
        (Some(image), ViewLayout::Grid) => {
            mapping.insert(
                Value::String("image".to_string()),
                Value::String(image.clone()),
            );
        }
        _ => {
            mapping.remove("image");
        }
    }
    mapping
}

/// Builds the document for a freshly created smart list: an optional type
/// scope, one table ("List") view and one cards ("Grid") view, so Obsidian
/// opens it with both layouts too.
pub fn default_smart_list_doc(scope_folder: Option<&str>, image_property: Option<&str>) -> Mapping {
    let mut doc = Mapping::new();
    if let Some(folder) = scope_folder {
        let scope = FilterNode::Group {
            conjunction: Conjunction::All,
            children: vec![FilterNode::Expr(FilterAtom::new(
                AtomKind::InFolder {
                    folder: folder.to_string(),
                },
                false,
            ))],
        };
        set_global_filters(&mut doc, &scope);
    }
    let mut table = Mapping::new();
    table.insert(
        Value::String("type".to_string()),
        Value::String("table".to_string()),
    );
    table.insert(
        Value::String("name".to_string()),
        Value::String("List".to_string()),
    );
    table.insert(
        Value::String("order".to_string()),
        Value::Sequence(vec![Value::String("file.name".to_string())]),
    );
    let mut cards = Mapping::new();
    cards.insert(
        Value::String("type".to_string()),
        Value::String("cards".to_string()),
    );
    cards.insert(
        Value::String("name".to_string()),
        Value::String("Grid".to_string()),
    );
    if let Some(image) = image_property {
        cards.insert(
            Value::String("image".to_string()),
            Value::String(image.to_string()),
        );
    }
    doc.insert(
        Value::String("views".to_string()),
        Value::Sequence(vec![Value::Mapping(table), Value::Mapping(cards)]),
    );
    doc
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

// ---------------------------------------------------------------------------
// Expression parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Num(f64),
    Symbol(&'static str),
}

/// Lexes a filter expression. `None` on any character we don't understand
/// (including `&&`/`||`, which we deliberately don't support) — the whole atom
/// then becomes opaque.
fn tokenize(input: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' || c == '\'' {
            chars.next();
            let mut value = String::new();
            loop {
                let next = chars.next()?;
                if next == '\\' {
                    value.push(chars.next()?);
                } else if next == c {
                    break;
                } else {
                    value.push(next);
                }
            }
            tokens.push(Token::Str(value));
        } else if c.is_ascii_digit() {
            let mut number = String::new();
            while chars
                .peek()
                .is_some_and(|c| c.is_ascii_digit() || *c == '.')
            {
                number.push(chars.next().expect("peeked"));
            }
            tokens.push(Token::Num(number.parse().ok()?));
        } else if c.is_alphabetic() || c == '_' {
            let mut ident = String::new();
            while chars
                .peek()
                .is_some_and(|c| c.is_alphanumeric() || *c == '_')
            {
                ident.push(chars.next().expect("peeked"));
            }
            tokens.push(Token::Ident(ident));
        } else {
            chars.next();
            let symbol = match c {
                '=' if chars.peek() == Some(&'=') => {
                    chars.next();
                    "=="
                }
                '=' => return None,
                '!' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        "!="
                    } else {
                        "!"
                    }
                }
                '>' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        ">="
                    } else {
                        ">"
                    }
                }
                '<' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        "<="
                    } else {
                        "<"
                    }
                }
                '+' => "+",
                '-' => "-",
                '(' => "(",
                ')' => ")",
                '[' => "[",
                ']' => "]",
                '.' => ".",
                ',' => ",",
                _ => return None,
            };
            tokens.push(Token::Symbol(symbol));
        }
    }
    Some(tokens)
}

/// The small expression AST the classifier pattern-matches into [`AtomKind`]s.
#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Ident(String),
    Str(String),
    Num(f64),
    Bool(bool),
    Member(Box<Expr>, String),
    Call(Box<Expr>, Vec<Expr>),
    Not(Box<Expr>),
    Cmp(Box<Expr>, CompareOp, Box<Expr>),
    Add(Box<Expr>, bool, Box<Expr>),
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn next(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.position);
        self.position += 1;
        token
    }

    fn eat_symbol(&mut self, symbol: &str) -> bool {
        match self.peek() {
            Some(Token::Symbol(s)) if *s == symbol => {
                self.position += 1;
                true
            }
            _ => false,
        }
    }

    fn parse_expr(&mut self) -> Option<Expr> {
        let lhs = self.parse_add()?;
        let op = match self.peek() {
            Some(Token::Symbol("==")) => Some(CompareOp::Eq),
            Some(Token::Symbol("!=")) => Some(CompareOp::Ne),
            Some(Token::Symbol(">")) => Some(CompareOp::Gt),
            Some(Token::Symbol(">=")) => Some(CompareOp::Gte),
            Some(Token::Symbol("<")) => Some(CompareOp::Lt),
            Some(Token::Symbol("<=")) => Some(CompareOp::Lte),
            _ => None,
        };
        match op {
            Some(op) => {
                self.position += 1;
                let rhs = self.parse_add()?;
                Some(Expr::Cmp(Box::new(lhs), op, Box::new(rhs)))
            }
            None => Some(lhs),
        }
    }

    fn parse_add(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let negative = match self.peek() {
                Some(Token::Symbol("+")) => false,
                Some(Token::Symbol("-")) => true,
                _ => break,
            };
            self.position += 1;
            let rhs = self.parse_unary()?;
            lhs = Expr::Add(Box::new(lhs), negative, Box::new(rhs));
        }
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        if self.eat_symbol("!") {
            return Some(Expr::Not(Box::new(self.parse_unary()?)));
        }
        if self.eat_symbol("-") {
            if let Some(Token::Num(value)) = self.peek().cloned() {
                self.position += 1;
                return Some(Expr::Num(-value));
            }
            return None;
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.eat_symbol(".") {
                let Some(Token::Ident(name)) = self.next().cloned() else {
                    return None;
                };
                expr = Expr::Member(Box::new(expr), name);
                if self.eat_symbol("(") {
                    let args = self.parse_args()?;
                    expr = Expr::Call(Box::new(expr), args);
                }
            } else if self.eat_symbol("[") {
                let Some(Token::Str(key)) = self.next().cloned() else {
                    return None;
                };
                if !self.eat_symbol("]") {
                    return None;
                }
                // `note["画像"]` reads the same property as `note.画像`.
                expr = Expr::Member(Box::new(expr), key);
            } else {
                break;
            }
        }
        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        match self.next().cloned()? {
            Token::Ident(name) => {
                let expr = match name.as_str() {
                    "true" => Expr::Bool(true),
                    "false" => Expr::Bool(false),
                    _ => Expr::Ident(name),
                };
                if matches!(expr, Expr::Ident(_)) && self.eat_symbol("(") {
                    let args = self.parse_args()?;
                    return Some(Expr::Call(Box::new(expr), args));
                }
                Some(expr)
            }
            Token::Str(value) => Some(Expr::Str(value)),
            Token::Num(value) => Some(Expr::Num(value)),
            Token::Symbol("(") => {
                let expr = self.parse_expr()?;
                if self.eat_symbol(")") {
                    Some(expr)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn parse_args(&mut self) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        if self.eat_symbol(")") {
            return Some(args);
        }
        loop {
            args.push(self.parse_expr()?);
            if self.eat_symbol(")") {
                return Some(args);
            }
            if !self.eat_symbol(",") {
                return None;
            }
        }
    }
}

/// Parses one filter-expression string into a supported atom (plus a negation
/// flag), or `None` when any part of it falls outside the supported profile.
pub fn parse_expression(input: &str) -> Option<(AtomKind, bool)> {
    let tokens = tokenize(input)?;
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
    };
    let expr = parser.parse_expr()?;
    if parser.position != tokens.len() {
        return None;
    }
    classify(&expr)
}

fn classify(expr: &Expr) -> Option<(AtomKind, bool)> {
    match expr {
        Expr::Not(inner) => {
            let (kind, negated) = classify(inner)?;
            // A negated comparison flips the operator instead of carrying a
            // negation flag, so the printer emits `!=` rather than `!(a == b)`.
            if let AtomKind::Compare { field, op, value } = kind {
                Some((
                    AtomKind::Compare {
                        field,
                        op: op.flipped(),
                        value,
                    },
                    negated,
                ))
            } else {
                Some((kind, !negated))
            }
        }
        Expr::Cmp(lhs, op, rhs) => {
            let field = field_ref(lhs)?;
            let value = compare_value(rhs)?;
            Some((
                AtomKind::Compare {
                    field,
                    op: *op,
                    value,
                },
                false,
            ))
        }
        Expr::Call(callee, args) => classify_call(callee, args),
        _ => None,
    }
}

fn classify_call(callee: &Expr, args: &[Expr]) -> Option<(AtomKind, bool)> {
    let Expr::Member(receiver, method) = callee else {
        return None;
    };
    if matches!(receiver.as_ref(), Expr::Ident(name) if name == "file") {
        let kind = match method.as_str() {
            "hasTag" => AtomKind::HasTag {
                tags: string_args(args)?,
            },
            "inFolder" => AtomKind::InFolder {
                folder: single_string_arg(args)?,
            },
            "hasLink" => AtomKind::HasLink {
                target: single_string_arg(args)?,
            },
            _ => return None,
        };
        return Some((kind, false));
    }
    let field = field_ref(receiver)?;
    let kind = match method.as_str() {
        "contains" => AtomKind::Contains {
            field,
            mode: ContainsMode::Any,
            values: string_args(args)?,
        },
        "containsAny" => AtomKind::Contains {
            field,
            mode: ContainsMode::Any,
            values: string_args(args)?,
        },
        "containsAll" => AtomKind::Contains {
            field,
            mode: ContainsMode::All,
            values: string_args(args)?,
        },
        "startsWith" => AtomKind::StartsWith {
            field,
            value: single_string_arg(args)?,
        },
        "endsWith" => AtomKind::EndsWith {
            field,
            value: single_string_arg(args)?,
        },
        "isEmpty" => {
            if !args.is_empty() {
                return None;
            }
            AtomKind::IsEmpty { field }
        }
        _ => return None,
    };
    Some((kind, false))
}

fn string_args(args: &[Expr]) -> Option<Vec<String>> {
    if args.is_empty() {
        return None;
    }
    args.iter()
        .map(|arg| match arg {
            Expr::Str(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn single_string_arg(args: &[Expr]) -> Option<String> {
    match args {
        [Expr::Str(value)] => Some(value.clone()),
        _ => None,
    }
}

/// Reserved bare identifiers that are never a frontmatter-field shorthand.
const RESERVED_IDENTS: [&str; 6] = ["file", "note", "formula", "date", "now", "today"];

fn field_ref(expr: &Expr) -> Option<FieldRef> {
    match expr {
        Expr::Ident(name) if !RESERVED_IDENTS.contains(&name.as_str()) => {
            Some(FieldRef::Note(name.clone()))
        }
        Expr::Member(receiver, name) => match receiver.as_ref() {
            Expr::Ident(base) if base == "note" => Some(FieldRef::Note(name.clone())),
            Expr::Ident(base) if base == "file" && name == "name" => Some(FieldRef::FileName),
            Expr::Ident(base) if base == "file" && name == "mtime" => Some(FieldRef::FileMtime),
            _ => None,
        },
        _ => None,
    }
}

fn compare_value(expr: &Expr) -> Option<CompareValue> {
    match expr {
        Expr::Str(value) => Some(CompareValue::String(value.clone())),
        Expr::Num(value) => Some(CompareValue::Number(*value)),
        Expr::Bool(value) => Some(CompareValue::Bool(*value)),
        _ => date_expr(expr).map(CompareValue::Date),
    }
}

fn date_expr(expr: &Expr) -> Option<DateExpr> {
    match expr {
        Expr::Call(callee, args) => {
            let Expr::Ident(name) = callee.as_ref() else {
                return None;
            };
            let base = match (name.as_str(), args.as_slice()) {
                ("today", []) => DateBase::Today,
                ("now", []) => DateBase::Now,
                ("date", [Expr::Str(value)]) => {
                    let (year, month, day) = crate::dates::exact_date_parts(Some(value))?;
                    DateBase::Absolute(NaiveDate::from_ymd_opt(year, month, day)?)
                }
                _ => return None,
            };
            Some(DateExpr {
                base,
                offsets: Vec::new(),
            })
        }
        Expr::Add(lhs, negative, rhs) => {
            let mut date = date_expr(lhs)?;
            let Expr::Str(duration) = rhs.as_ref() else {
                return None;
            };
            date.offsets.push(DateOffset {
                negative: *negative,
                duration: parse_duration(duration)?,
            });
            Some(date)
        }
        _ => None,
    }
}

fn duration_piece_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d+)\s*([A-Za-z]+)").unwrap())
}

/// Parses a Bases duration literal (`"90d"`, `"1M 4h"`). Unit letters are
/// case-sensitive: `M` is months, `m` is minutes.
pub fn parse_duration(input: &str) -> Option<DurationSpec> {
    let mut spec = DurationSpec::default();
    let mut consumed = 0usize;
    for captures in duration_piece_regex().captures_iter(input) {
        let full = captures.get(0).expect("full match");
        // Everything between pieces must be whitespace, or the literal is junk.
        if !input[consumed..full.start()].trim().is_empty() {
            return None;
        }
        consumed = full.end();
        let amount: u32 = captures[1].parse().ok()?;
        let slot = match &captures[2] {
            "y" | "year" | "years" => &mut spec.years,
            "M" | "month" | "months" => &mut spec.months,
            "w" | "week" | "weeks" => &mut spec.weeks,
            "d" | "day" | "days" => &mut spec.days,
            "h" | "hour" | "hours" => &mut spec.hours,
            "m" | "minute" | "minutes" => &mut spec.minutes,
            "s" | "second" | "seconds" => &mut spec.seconds,
            _ => return None,
        };
        *slot = slot.saturating_add(amount);
    }
    if consumed == 0 || !input[consumed..].trim().is_empty() {
        return None;
    }
    Some(spec)
}

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
        AtomKind::HasLink { target } => format!("file.hasLink({})", escape_string(target)),
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
fn note_value(name: &str, record: &EntityRecord, ctx: &EvalContext) -> Option<serde_json::Value> {
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

fn json_scalar_string(value: &serde_json::Value) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        EntitySummary, EntityTypeConfig, FieldConfig, FieldType, KizunaConfig, Relation,
        RelationDirection,
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    // --- fixtures --------------------------------------------------------------

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

    fn config() -> KizunaConfig {
        KizunaConfig {
            vault_root: "/virtual-vault".to_string(),
            taxonomy_root: "Media".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            tags: None,
            types: vec![EntityTypeConfig {
                id: "anime".to_string(),
                label: "Anime".to_string(),
                icon: None,
                path: "Anime".to_string(),
                external_priority: Vec::new(),
                filename: None,
                body_sections: Vec::new(),
                log: None,
                fields: vec![
                    field("status", FieldType::Enum),
                    field("genres", FieldType::EnumList),
                    field("rating", FieldType::Rating),
                    field("started", FieldType::Date),
                    field("cover", FieldType::Image),
                    field("studio", FieldType::Relation),
                ],
            }],
        }
    }

    fn record(id: &str, title: &str, frontmatter: serde_json::Value) -> EntityRecord {
        EntityRecord {
            body_links: Vec::new(),
            summary: EntitySummary {
                id: id.to_string(),
                entity_type: "anime".to_string(),
                type_label: "Anime".to_string(),
                title: title.to_string(),
                titles: BTreeMap::new(),
                dates: Vec::new(),
                image: None,
                summary: None,
                path: format!("Media/Anime/{title}.md"),
                basename: title.to_string(),
                external_refs: BTreeMap::new(),
                tags: Vec::new(),
                episode_progress: None,
                status: None,
                relation_count: 0,
            },
            revision: "rev".to_string(),
            frontmatter: frontmatter.as_object().cloned().unwrap_or_default(),
            file_modified_unix_nanos: 0,
            episode_dates: Vec::new(),
        }
    }

    fn library(records: Vec<EntityRecord>, relations: Vec<Relation>) -> Library {
        Library::new(config(), records, relations, Vec::new(), "gen".to_string())
    }

    fn fixed_ctx(library: &Library) -> EvalContext<'_> {
        let today = NaiveDate::from_ymd_opt(2026, 7, 8).unwrap();
        let now = today.and_hms_opt(12, 0, 0).unwrap().and_utc();
        EvalContext::new(library, now, today)
    }

    fn atom(expression: &str) -> FilterNode {
        let (kind, negated) =
            parse_expression(expression).unwrap_or_else(|| panic!("unsupported: {expression}"));
        FilterNode::Expr(FilterAtom {
            kind,
            negated,
            raw: expression.to_string(),
        })
    }

    fn eval_expr(expression: &str, record: &EntityRecord, ctx: &EvalContext) -> Option<bool> {
        eval_node(&atom(expression), record, ctx)
    }

    // --- expression parsing ----------------------------------------------------

    #[test]
    fn parses_the_supported_expression_profile() {
        for expression in [
            r#"status == "watching""#,
            r#"note.status != "done""#,
            r#"note["画像"].isEmpty()"#,
            r#"rating >= 7.5"#,
            r#"favorite == true"#,
            r#"genres.containsAny("comedy", "drama")"#,
            r#"genres.containsAll("comedy", "drama")"#,
            r#"title.contains("hero")"#,
            r#"title.startsWith("My")"#,
            r#"title.endsWith("!")"#,
            r#"!note.summary.isEmpty()"#,
            r#"file.hasTag("favorites")"#,
            r#"file.inFolder("Media/Anime")"#,
            r#"file.hasLink("Kyoto Animation")"#,
            r#"file.name.contains("2026")"#,
            r#"note.started >= date("2026-01-01")"#,
            r#"note.started >= today() - "90d""#,
            r#"file.mtime > now() - "1w""#,
            r#"note.started < today() + "1M 4h""#,
        ] {
            assert!(
                parse_expression(expression).is_some(),
                "should parse: {expression}"
            );
        }
    }

    #[test]
    fn unsupported_expressions_are_rejected_not_guessed() {
        for expression in [
            r#"formula.ppu > 5"#,
            r#"price * 2 > 10"#,
            r#"a == b"#,                      // field-to-field comparison
            r#"status == "a" && rating > 5"#, // boolean operators
            r#"file.size > 100"#,             // unsupported file property
            r#"if(price, price)"#,
            r#"note.title.lower() == "x""#,
            r#"status"#, // bare truthiness
            r#"note.started >= today() - "banana""#,
        ] {
            assert!(
                parse_expression(expression).is_none(),
                "should reject: {expression}"
            );
        }
    }

    #[test]
    fn negated_comparison_flips_the_operator() {
        let (kind, negated) = parse_expression(r#"!(rating > 5)"#).unwrap();
        assert!(!negated);
        assert_eq!(
            kind,
            AtomKind::Compare {
                field: FieldRef::Note("rating".to_string()),
                op: CompareOp::Lte,
                value: CompareValue::Number(5.0),
            }
        );
    }

    #[test]
    fn printed_atoms_reparse_to_the_same_atom() {
        let atoms = [
            FilterAtom::new(
                AtomKind::Compare {
                    field: FieldRef::Note("status".to_string()),
                    op: CompareOp::Eq,
                    value: CompareValue::String("watching".to_string()),
                },
                false,
            ),
            FilterAtom::new(
                AtomKind::Compare {
                    field: FieldRef::Note("started".to_string()),
                    op: CompareOp::Gte,
                    value: CompareValue::Date(DateExpr {
                        base: DateBase::Today,
                        offsets: vec![DateOffset {
                            negative: true,
                            duration: DurationSpec {
                                days: 90,
                                ..Default::default()
                            },
                        }],
                    }),
                },
                false,
            ),
            FilterAtom::new(
                AtomKind::Contains {
                    field: FieldRef::Note("genres".to_string()),
                    mode: ContainsMode::Any,
                    values: vec!["comedy".to_string(), "slice-of-life".to_string()],
                },
                false,
            ),
            FilterAtom::new(
                AtomKind::IsEmpty {
                    field: FieldRef::Note("画像".to_string()),
                },
                true,
            ),
            FilterAtom::new(
                AtomKind::HasLink {
                    target: "He said \"hi\"".to_string(),
                },
                false,
            ),
            FilterAtom::new(
                AtomKind::InFolder {
                    folder: "Media/Anime".to_string(),
                },
                false,
            ),
        ];
        for original in atoms {
            let (kind, negated) = parse_expression(&original.raw)
                .unwrap_or_else(|| panic!("printer emitted unparseable: {}", original.raw));
            assert_eq!(kind, original.kind, "round-trip of {}", original.raw);
            assert_eq!(negated, original.negated, "round-trip of {}", original.raw);
        }
    }

    #[test]
    fn duration_literals_parse_case_sensitively() {
        assert_eq!(
            parse_duration("1M"),
            Some(DurationSpec {
                months: 1,
                ..Default::default()
            })
        );
        assert_eq!(
            parse_duration("30m"),
            Some(DurationSpec {
                minutes: 30,
                ..Default::default()
            })
        );
        assert_eq!(
            parse_duration("1M 4h"),
            Some(DurationSpec {
                months: 1,
                hours: 4,
                ..Default::default()
            })
        );
        assert_eq!(
            parse_duration("2 weeks"),
            Some(DurationSpec {
                weeks: 2,
                ..Default::default()
            })
        );
        assert_eq!(parse_duration("banana"), None);
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("1x"), None);
    }

    // --- evaluation --------------------------------------------------------------

    #[test]
    fn evaluates_value_driven_comparisons() {
        let entity = record(
            "anime:a",
            "Alpha",
            json!({
                "status": "watching",
                "rating": 8,
                "favorite": true,
                "genres": ["comedy", "drama"],
                "started": "2026-06-01",
                "summary": "",
            }),
        );
        let library = library(vec![entity], Vec::new());
        let entity = &library.records[0];
        let ctx = fixed_ctx(&library);

        for (expression, expected) in [
            (r#"status == "watching""#, true),
            (r#"status != "watching""#, false),
            (r#"rating >= 7"#, true),
            (r#"rating < 7"#, false),
            (r#"favorite == true"#, true),
            (r#"genres.containsAny("comedy", "horror")"#, true),
            (r#"genres.containsAll("comedy", "horror")"#, false),
            (r#"genres.contains("drama")"#, true),
            (r#"status.contains("watch")"#, true), // substring on a string value
            (r#"status.startsWith("watch")"#, true),
            (r#"status.endsWith("watch")"#, false),
            (r#"summary.isEmpty()"#, true), // empty string is empty
            (r#"!summary.isEmpty()"#, false),
            (r#"genres.isEmpty()"#, false),
            (r#"missing.isEmpty()"#, true),
            // Missing fields: only != is definitely true (Bases null semantics).
            (r#"missing == "x""#, false),
            (r#"missing != "x""#, true),
            (r#"missing > 3"#, false),
            // Date comparisons, absolute and relative to the injected today.
            (r#"note.started >= date("2026-01-01")"#, true),
            (r#"note.started < date("2026-01-01")"#, false),
            (r#"note.started >= today() - "90d""#, true),
            (r#"note.started >= today() - "7d""#, false),
            // File scope + name.
            (r#"file.inFolder("Media/Anime")"#, true),
            (r#"file.inFolder("Media/Books")"#, false),
            (r#"file.name.contains("Alph")"#, true),
        ] {
            assert_eq!(
                eval_expr(expression, entity, &ctx),
                Some(expected),
                "{expression}"
            );
        }
    }

    #[test]
    fn evaluates_fuzzy_dates_by_their_sort_key() {
        let entity = record("anime:a", "Alpha", json!({"started": "2024 Spring"}));
        let library = library(vec![entity], Vec::new());
        let ctx = fixed_ctx(&library);
        assert_eq!(
            eval_expr(
                r#"note.started >= date("2024-01-01")"#,
                &library.records[0],
                &ctx
            ),
            Some(true)
        );
    }

    #[test]
    fn evaluates_mtime_against_relative_instants() {
        let mut entity = record("anime:a", "Alpha", json!({}));
        // 2026-07-05 00:00 UTC — three days before the fixed "now".
        let mtime = NaiveDate::from_ymd_opt(2026, 7, 5)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_nanos_opt()
            .unwrap() as u128;
        entity.file_modified_unix_nanos = mtime;
        let unknown = record("anime:b", "Beta", json!({}));
        let library = library(vec![entity, unknown], Vec::new());
        let ctx = fixed_ctx(&library);

        assert_eq!(
            eval_expr(r#"file.mtime > now() - "1w""#, &library.records[0], &ctx),
            Some(true)
        );
        assert_eq!(
            eval_expr(r#"file.mtime > now() - "1d""#, &library.records[0], &ctx),
            Some(false)
        );
        // Unknown mtime (0) never matches a comparison.
        assert_eq!(
            eval_expr(r#"file.mtime > now() - "1w""#, &library.records[1], &ctx),
            Some(false)
        );
    }

    #[test]
    fn has_tag_matches_exact_and_nested() {
        let mut entity = record("anime:a", "Alpha", json!({}));
        entity.summary.tags = vec!["favorites".to_string(), "mood/cozy".to_string()];
        let library = library(vec![entity], Vec::new());
        let ctx = fixed_ctx(&library);
        for (expression, expected) in [
            (r#"file.hasTag("favorites")"#, true),
            (r#"file.hasTag("mood")"#, true), // nested tag under mood/
            (r#"file.hasTag("cozy")"#, false),
            (r#"file.hasTag("nope", "favorites")"#, true), // any of
        ] {
            assert_eq!(
                eval_expr(expression, &library.records[0], &ctx),
                Some(expected),
                "{expression}"
            );
        }
    }

    #[test]
    fn has_link_matches_through_the_relation_graph() {
        let source = record("anime:a", "Alpha", json!({}));
        let target = record("anime:kyoani", "Kyoto Animation", json!({}));
        let relations = vec![Relation {
            source_id: "anime:a".to_string(),
            target_id: Some("anime:kyoani".to_string()),
            target_title: "Kyoto Animation".to_string(),
            target_type: Some("anime".to_string()),
            field: "studio".to_string(),
            direction: RelationDirection::Out,
        }];
        let library = library(vec![source, target], relations);
        let ctx = fixed_ctx(&library);
        assert_eq!(
            eval_expr(
                r#"file.hasLink("Kyoto Animation")"#,
                &library.records[0],
                &ctx
            ),
            Some(true)
        );
        assert_eq!(
            eval_expr(r#"file.hasLink("Someone Else")"#, &library.records[0], &ctx),
            Some(false)
        );
        // The target itself has no outgoing link.
        assert_eq!(
            eval_expr(
                r#"file.hasLink("Kyoto Animation")"#,
                &library.records[1],
                &ctx
            ),
            Some(false)
        );
    }

    #[test]
    fn tags_field_reads_the_resident_tag_list() {
        let mut entity = record("anime:a", "Alpha", json!({}));
        entity.summary.tags = vec!["cozy".to_string()];
        let library = library(vec![entity], Vec::new());
        let ctx = fixed_ctx(&library);
        assert_eq!(
            eval_expr(r#"tags.contains("cozy")"#, &library.records[0], &ctx),
            Some(true)
        );
        assert_eq!(
            eval_expr(r#"tags.isEmpty()"#, &library.records[0], &ctx),
            Some(false)
        );
    }

    // --- tri-state groups ---------------------------------------------------------

    #[test]
    fn unsupported_atoms_are_neutral_in_every_conjunction() {
        let entity = record("anime:a", "Alpha", json!({"status": "watching"}));
        let library = library(vec![entity], Vec::new());
        let entity = &library.records[0];
        let ctx = fixed_ctx(&library);
        let opaque = FilterNode::Opaque(Value::String("formula.ppu > 5".to_string()));

        // and: unknown children are ignored; known children decide.
        let and_true = FilterNode::Group {
            conjunction: Conjunction::All,
            children: vec![atom(r#"status == "watching""#), opaque.clone()],
        };
        assert_eq!(eval_node(&and_true, entity, &ctx), Some(true));

        // or: an unknown child must NOT swallow the vault — known false wins.
        let or_false = FilterNode::Group {
            conjunction: Conjunction::Any,
            children: vec![atom(r#"status == "done""#), opaque.clone()],
        };
        assert_eq!(eval_node(&or_false, entity, &ctx), Some(false));

        // A fully-unknown group propagates unknown; top-level unknown includes.
        let unknown = FilterNode::Group {
            conjunction: Conjunction::Any,
            children: vec![opaque.clone()],
        };
        assert_eq!(eval_node(&unknown, entity, &ctx), None);
        assert!(record_matches(&unknown, entity, &ctx));

        // not: none of the known children may match.
        let none_of = FilterNode::Group {
            conjunction: Conjunction::NoneOf,
            children: vec![atom(r#"status == "done""#), opaque],
        };
        assert_eq!(eval_node(&none_of, entity, &ctx), Some(true));

        // The empty group is "no constraints".
        assert_eq!(eval_node(&FilterNode::empty(), entity, &ctx), Some(true));
    }

    // --- document parse / round-trip ---------------------------------------------

    const HAND_EDITED: &str = r#"filters:
  and:
    - file.inFolder("Media/Anime")
    - status == "watching"
    - formula.ppu > 5
formulas:
  ppu: "(price / age).toFixed(2)"
views:
  - type: table
    name: List
    order:
      - file.name
      - status
    sort:
      - property: note.started
        direction: DESC
    limit: 25
  - type: cards
    name: Grid
    image: note.cover
    cardSize: 220
  - type: map
    name: Places
"#;

    #[test]
    fn parses_a_hand_edited_document_leniently() {
        let list = parse_smart_list(HAND_EDITED).unwrap();

        // Two supported atoms plus one opaque, all inside the and-group.
        let FilterNode::Group {
            conjunction: Conjunction::All,
            children,
        } = &list.filters
        else {
            panic!("expected group")
        };
        assert_eq!(children.len(), 3);
        assert!(matches!(&children[2], FilterNode::Opaque(_)));

        // The scope idiom is recognized from the inFolder atom.
        let scope = scope_from_filters(&list.filters, &config());
        assert_eq!(scope, Some(("anime".to_string(), 0)));

        // table + cards views; map skipped with a warning.
        assert_eq!(list.views.len(), 2);
        assert_eq!(list.views[0].layout, ViewLayout::List);
        assert_eq!(list.views[0].limit, Some(25));
        assert_eq!(list.views[0].sort.len(), 1);
        assert_eq!(list.views[0].sort[0].direction, SortDirection::Desc);
        assert_eq!(list.views[1].layout, ViewLayout::Grid);
        assert_eq!(list.views[1].image.as_deref(), Some("note.cover"));
        assert_eq!(list.views[1].source_index, 1);

        assert!(list.warnings.iter().any(|w| w.contains("formula.ppu")));
        assert!(list.warnings.iter().any(|w| w.contains("Places")));
    }

    #[test]
    fn structural_edits_preserve_everything_we_do_not_own() {
        let list = parse_smart_list(HAND_EDITED).unwrap();
        let mut doc = list.doc.clone();

        // Edit: change the status criterion, keep scope, drop nothing else.
        let filters = FilterNode::Group {
            conjunction: Conjunction::All,
            children: vec![
                atom(r#"file.inFolder("Media/Anime")"#),
                FilterNode::Expr(FilterAtom::new(
                    AtomKind::Compare {
                        field: FieldRef::Note("status".to_string()),
                        op: CompareOp::Eq,
                        value: CompareValue::String("done".to_string()),
                    },
                    false,
                )),
                FilterNode::Opaque(Value::String("formula.ppu > 5".to_string())),
            ],
        };
        set_global_filters(&mut doc, &filters);
        apply_views(
            &mut doc,
            &[
                ViewSpec {
                    layout: ViewLayout::List,
                    name: "List".to_string(),
                    filters: None,
                    sort: vec![ViewSort {
                        property: SortProperty::Note("rating".to_string()),
                        direction: SortDirection::Desc,
                    }],
                    limit: Some(10),
                    image: None,
                },
                ViewSpec {
                    layout: ViewLayout::Grid,
                    name: "Grid".to_string(),
                    filters: None,
                    sort: Vec::new(),
                    limit: None,
                    image: Some("note.cover".to_string()),
                },
            ],
        );

        let rendered = render_smart_list(&doc);
        let reparsed = parse_smart_list(&rendered).unwrap();

        // The hand-written formula block survives.
        assert!(rendered.contains("ppu:"));
        // The opaque filter expression survives verbatim.
        assert!(rendered.contains("formula.ppu > 5"));
        // The unsupported map view survives, after the supported views.
        assert!(rendered.contains("type: map"));
        assert_eq!(reparsed.views.len(), 2);
        // The table view kept its unknown `order` key and its identity.
        assert!(rendered.contains("- file.name"));
        // The cards view kept its unknown cardSize key.
        assert!(rendered.contains("cardSize: 220"));
        // The edits took: new sort + limit on the table view.
        assert_eq!(
            reparsed.views[0].sort[0].property,
            SortProperty::Note("rating".to_string())
        );
        assert_eq!(reparsed.views[0].limit, Some(10));
        // Idempotence: rendering the reparsed doc yields the same text.
        assert_eq!(render_smart_list(&reparsed.doc), rendered);
    }

    #[test]
    fn default_document_is_valid_and_scoped() {
        let doc = default_smart_list_doc(Some("Media/Anime"), Some("note.cover"));
        let rendered = render_smart_list(&doc);
        let list = parse_smart_list(&rendered).unwrap();
        assert!(list.warnings.is_empty(), "warnings: {:?}", list.warnings);
        assert_eq!(
            scope_from_filters(&list.filters, &config()),
            Some(("anime".to_string(), 0))
        );
        assert_eq!(list.views.len(), 2);
        assert_eq!(list.views[0].layout, ViewLayout::List);
        assert_eq!(list.views[1].layout, ViewLayout::Grid);
        assert_eq!(list.views[1].image.as_deref(), Some("note.cover"));

        // Unscoped, imageless: no filters key at all, still two views.
        let bare = render_smart_list(&default_smart_list_doc(None, None));
        let bare = parse_smart_list(&bare).unwrap();
        assert!(bare.filters.is_empty_group());
        assert_eq!(bare.views.len(), 2);
    }

    #[test]
    fn rejects_invalid_yaml_and_non_mappings() {
        assert!(parse_smart_list("views: [").is_err());
        assert!(parse_smart_list("- just\n- a list\n").is_err());
        // Empty files are a valid, empty smart list.
        let empty = parse_smart_list("").unwrap();
        assert!(empty.filters.is_empty_group());
        assert!(empty.views.is_empty());
    }

    // --- results: filter → sort → limit --------------------------------------------

    #[test]
    fn smart_list_records_filters_sorts_and_limits() {
        let raw = r#"filters:
  and:
    - file.inFolder("Media/Anime")
    - status == "watching"
views:
  - type: table
    name: List
    sort:
      - property: note.rating
        direction: DESC
    limit: 2
"#;
        let list = parse_smart_list(raw).unwrap();
        let library = library(
            vec![
                record(
                    "anime:a",
                    "Alpha",
                    json!({"status": "watching", "rating": 6}),
                ),
                record(
                    "anime:b",
                    "Beta",
                    json!({"status": "watching", "rating": 9}),
                ),
                record("anime:c", "Gamma", json!({"status": "done", "rating": 10})),
                record("anime:d", "Delta", json!({"status": "watching"})), // no rating → last
                record("anime:e", "Eps", json!({"status": "watching", "rating": 7})),
            ],
            Vec::new(),
        );
        let ctx = fixed_ctx(&library);
        let records = smart_list_records(&list, list.views.first(), &ctx, None);
        let ids: Vec<&str> = records.iter().map(|r| r.summary.id.as_str()).collect();
        // Top 2 by rating desc among "watching": Beta (9), Eps (7).
        assert_eq!(ids, ["anime:b", "anime:e"]);

        // Without the view: unsorted-by-rating default (title asc), no limit.
        let all = smart_list_records(&list, None, &ctx, None);
        let ids: Vec<&str> = all.iter().map(|r| r.summary.id.as_str()).collect();
        assert_eq!(ids, ["anime:a", "anime:b", "anime:d", "anime:e"]);
    }

    #[test]
    fn view_filters_stack_on_global_filters() {
        let raw = r#"filters:
  and:
    - file.inFolder("Media/Anime")
views:
  - type: table
    name: Favorites
    filters:
      and:
        - rating >= 8
"#;
        let list = parse_smart_list(raw).unwrap();
        let library = library(
            vec![
                record("anime:a", "Alpha", json!({"rating": 9})),
                record("anime:b", "Beta", json!({"rating": 5})),
            ],
            Vec::new(),
        );
        let ctx = fixed_ctx(&library);
        let records = smart_list_records(&list, list.views.first(), &ctx, None);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].summary.id, "anime:a");
    }

    #[test]
    fn date_fields_sort_by_their_normalized_key() {
        let raw = r#"views:
  - type: table
    name: List
    sort:
      - property: note.started
        direction: ASC
"#;
        let list = parse_smart_list(raw).unwrap();
        let library = library(
            vec![
                record("anime:b", "Beta", json!({"started": "2024 Spring"})),
                record("anime:a", "Alpha", json!({"started": "2023-11-01"})),
                record("anime:c", "Gamma", json!({})), // absent → last
            ],
            Vec::new(),
        );
        let ctx = fixed_ctx(&library);
        let records = smart_list_records(&list, list.views.first(), &ctx, None);
        let ids: Vec<&str> = records.iter().map(|r| r.summary.id.as_str()).collect();
        assert_eq!(ids, ["anime:a", "anime:b", "anime:c"]);
    }

    #[test]
    fn type_scope_folder_joins_taxonomy_and_type_path() {
        let config = config();
        assert_eq!(
            type_scope_folder(&config, "anime").as_deref(),
            Some("Media/Anime")
        );
        assert_eq!(type_scope_folder(&config, "missing"), None);
    }

    /// Verbatim view config as serialized by real Obsidian (≥1.9): sort
    /// entries are `{property, direction}` with UPPERCASE directions and
    /// *bare* note-field references (`date`, not `note.date`), while the cards
    /// image uses the prefixed `note.cover_url`. Both reference forms must
    /// parse to the same properties our writer emits.
    #[test]
    fn parses_obsidian_native_view_serialization() {
        let raw = r#"views:
  - type: table
    name: Table
    sort:
      - property: file.name
        direction: ASC
      - property: date
        direction: ASC
  - type: cards
    name: Cards
    image: note.cover_url
"#;
        let list = parse_smart_list(raw).unwrap();
        assert!(list.warnings.is_empty(), "warnings: {:?}", list.warnings);
        assert_eq!(
            list.views[0].sort,
            vec![
                ViewSort {
                    property: SortProperty::FileName,
                    direction: SortDirection::Asc,
                },
                ViewSort {
                    property: SortProperty::Note("date".to_string()),
                    direction: SortDirection::Asc,
                },
            ]
        );
        assert_eq!(list.views[1].image.as_deref(), Some("note.cover_url"));
        // The bare and prefixed forms name the same property.
        assert_eq!(
            parse_sort_property("date"),
            parse_sort_property("note.date")
        );
    }
}
