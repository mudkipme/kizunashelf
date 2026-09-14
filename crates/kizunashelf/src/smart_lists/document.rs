//! The `.base` document layer: lenient YAML parse into [`SmartList`],
//! serialization, and the round-trip-preserving mutations (structural edits
//! patch the raw document tree, so hand-authored syntax the app doesn't model
//! survives every save).

use super::expr::parse_expression;
use super::model::{
    AtomKind, Conjunction, FilterAtom, FilterNode, SmartList, SmartView, SortProperty, ViewLayout,
    ViewSort,
};
use crate::relations::SortDirection;
use serde_yaml::{Mapping, Value};

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

/// Home membership travels with the list, including a rename of its file.
pub fn shows_on_home(list: &SmartList) -> bool {
    list.doc
        .get("kizunashelf")
        .and_then(Value::as_mapping)
        .and_then(|metadata| metadata.get("showOnHome"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn suggestion_id(list: &SmartList) -> Option<&str> {
    list.doc
        .get("kizunashelf")
        .and_then(Value::as_mapping)
        .and_then(|metadata| metadata.get("suggestion"))
        .and_then(Value::as_str)
}

/// Change only the requested app metadata; preserve all other YAML keys.
pub fn set_home_visibility(doc: &mut Mapping, visible: bool) -> Result<(), String> {
    let key = Value::String("kizunashelf".to_string());
    let metadata = doc
        .entry(key)
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    let metadata = metadata
        .as_mapping_mut()
        .ok_or("kizunashelf metadata must be a YAML mapping")?;
    metadata.insert(
        Value::String("showOnHome".to_string()),
        Value::Bool(visible),
    );
    Ok(())
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
