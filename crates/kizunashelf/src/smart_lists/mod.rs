//! Smart lists: dynamic, criteria-based lists stored as **Obsidian Bases**
//! `.base` YAML files in `KizunaShelf/Lists/`, next to the static Markdown
//! lists — "files over apps" style, openable in Obsidian.
//!
//! The compatibility contract is one-way: every file *we* write uses only
//! well-supported Bases constructs, so Obsidian renders the identical result
//! set. Hand-edited files are parsed leniently — anything we don't understand
//! becomes an opaque node that evaluates to *unknown* (see the tri-state logic
//! in [`eval`]) and is preserved **verbatim** on every rewrite, because
//! mutations edit the raw YAML document tree rather than re-serializing a
//! lossy model.
//!
//! Like [`crate::lists`], this module is pure — no I/O and no knowledge of the
//! API contract. The handlers in `api/smart_lists.rs` compose it with the
//! [`Vfs`](crate::vfs::Vfs) and the cached [`Library`](crate::types::Library).
//!
//! Evaluation is deliberately *value-driven*, exactly like Bases: a comparison
//! is a date comparison because the right-hand side is a date expression, and
//! numeric because the literal is a number — never because of what a field is
//! called. The schema is consulted only for the built-in tags field name and
//! for sort-key derivation.
//!
//! The submodules are the pipeline stages, in data-flow order:
//!
//! - [`model`] — the filter/view vocabulary everything else speaks.
//! - [`expr`] ⇄ [`print`] — expression text ⇄ supported atoms (inverses).
//! - [`document`] — `.base` YAML ⇄ [`document::SmartList`], round-trip safe.
//! - [`eval`] — tri-state matching of records against a filter tree.
//! - [`results`] — filter → sort → limit into a view's record list.

mod document;
mod eval;
mod expr;
mod model;
mod print;
mod results;

#[cfg(test)]
mod tests;

pub use document::{
    apply_views, default_smart_list_doc, filter_node_to_yaml, parse_smart_list,
    parse_sort_property, print_sort_property, render_smart_list, set_global_filters, ViewSpec,
};
pub use eval::{eval_node, record_matches, EvalContext};
pub use expr::{parse_duration, parse_expression};
pub use model::{
    scope_from_filters, type_scope_folder, AtomKind, CompareOp, CompareValue, Conjunction,
    ContainsMode, DateBase, DateExpr, DateOffset, DurationSpec, FieldRef, FilterAtom, FilterNode,
    SmartList, SmartView, SortProperty, ViewLayout, ViewSort,
};
pub use print::print_atom;
pub use results::smart_list_records;

/// File extension of a smart list. Smart lists live in the same directory as
/// static lists ([`crate::lists::LISTS_DIR`]); the extension is the kind.
pub const SMART_LIST_EXTENSION: &str = "base";
