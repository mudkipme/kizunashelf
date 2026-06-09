use crate::dates::{parse_entity_date, parsed_date_sort_key};
use crate::types::{EntityDateValue, EntityTypeConfig, FieldType};
use regex::Regex;
use serde_json::{Map, Number, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub(super) struct ParsedMarkdown {
    pub(super) frontmatter: Map<String, Value>,
    pub(super) body: String,
    pub(super) diagnostics: Vec<String>,
}

pub struct MarkdownDocument {
    pub frontmatter: Map<String, Value>,
    pub body: String,
}

pub(super) fn parse_markdown(raw: &str) -> ParsedMarkdown {
    if !raw.starts_with("---\n") {
        return ParsedMarkdown {
            frontmatter: Map::new(),
            body: raw.trim().to_string(),
            diagnostics: Vec::new(),
        };
    }
    let Some(end) = raw[4..].find("\n---").map(|index| index + 4) else {
        return ParsedMarkdown {
            frontmatter: Map::new(),
            body: raw.trim().to_string(),
            diagnostics: vec![
                "frontmatter starts with --- but has no closing delimiter".to_string()
            ],
        };
    };
    let yaml_text = &raw[4..end];
    let body = raw[end + 4..].trim().to_string();
    let mut diagnostics = Vec::new();
    let frontmatter = match serde_yaml::from_str::<serde_yaml::Value>(yaml_text) {
        Ok(value) => match yaml_to_json_value(value) {
            Some(Value::Object(map)) => map,
            Some(_) => {
                diagnostics.push("frontmatter must be a YAML mapping/object".to_string());
                Map::new()
            }
            None => {
                diagnostics.push("frontmatter contains unsupported YAML values".to_string());
                Map::new()
            }
        },
        Err(error) => {
            diagnostics.push(format!("invalid frontmatter YAML: {error}"));
            Map::new()
        }
    };

    ParsedMarkdown {
        frontmatter,
        body,
        diagnostics,
    }
}

pub fn split_markdown_document(raw: &str) -> MarkdownDocument {
    if !raw.starts_with("---\n") {
        return MarkdownDocument {
            frontmatter: Map::new(),
            body: raw.to_string(),
        };
    }
    let Some(end) = raw[4..].find("\n---").map(|index| index + 4) else {
        return MarkdownDocument {
            frontmatter: Map::new(),
            body: raw.to_string(),
        };
    };
    let yaml_text = &raw[4..end];
    let frontmatter = serde_yaml::from_str::<serde_yaml::Value>(yaml_text)
        .ok()
        .and_then(yaml_to_json_value)
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default();
    MarkdownDocument {
        frontmatter,
        body: raw[end + 4..].to_string(),
    }
}

pub fn serialize_markdown_document(frontmatter: &Map<String, Value>, body: &str) -> String {
    if frontmatter.is_empty() {
        return body.to_string();
    }
    let mut yaml = serde_yaml::to_string(frontmatter).unwrap_or_default();
    if let Some(stripped) = yaml.strip_prefix("---\n") {
        yaml = stripped.to_string();
    }
    if !yaml.ends_with('\n') {
        yaml.push('\n');
    }
    if body.is_empty() {
        format!("---\n{yaml}---\n")
    } else if body.starts_with('\n') {
        format!("---\n{yaml}---{body}")
    } else {
        format!("---\n{yaml}---\n{body}")
    }
}

fn yaml_to_json_value(value: serde_yaml::Value) -> Option<Value> {
    match value {
        serde_yaml::Value::Null => Some(Value::Null),
        serde_yaml::Value::Bool(value) => Some(Value::Bool(value)),
        serde_yaml::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Some(Value::Number(Number::from(value)))
            } else if let Some(value) = value.as_u64() {
                Some(Value::Number(Number::from(value)))
            } else {
                value.as_f64().and_then(Number::from_f64).map(Value::Number)
            }
        }
        serde_yaml::Value::String(value) => Some(Value::String(value)),
        serde_yaml::Value::Sequence(values) => Some(Value::Array(
            values.into_iter().filter_map(yaml_to_json_value).collect(),
        )),
        serde_yaml::Value::Mapping(mapping) => {
            let mut map = Map::new();
            for (key, value) in mapping {
                let serde_yaml::Value::String(key) = key else {
                    continue;
                };
                if let Some(value) = yaml_to_json_value(value) {
                    map.insert(key, value);
                }
            }
            Some(Value::Object(map))
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json_value(tagged.value),
    }
}

pub(super) fn external_refs(
    frontmatter: &Map<String, Value>,
    keys: &[String],
) -> BTreeMap<String, String> {
    let mut refs = BTreeMap::new();
    for key in keys {
        if let Some(value) = first_string(frontmatter, std::slice::from_ref(key)) {
            refs.insert(key.clone(), value);
        }
    }
    refs
}

pub(super) fn date_values(
    frontmatter: &Map<String, Value>,
    keys: &[String],
) -> Vec<EntityDateValue> {
    let mut dates = Vec::new();
    for field in keys {
        for value in normalize_values(frontmatter.get(field)) {
            let parsed = parse_entity_date(Some(&value));
            let sort_key = parsed_date_sort_key(Some(&value));
            dates.push(EntityDateValue {
                field: field.clone(),
                value,
                parsed,
                sort_key,
            });
        }
    }
    dates
}

pub(super) fn first_string(frontmatter: &Map<String, Value>, keys: &[String]) -> Option<String> {
    keys.iter()
        .filter_map(|key| normalize_value(frontmatter.get(key)))
        .next()
}

pub(super) fn title_languages(
    frontmatter: &Map<String, Value>,
    basename: &str,
    type_config: &EntityTypeConfig,
) -> BTreeMap<String, String> {
    let mut titles = BTreeMap::new();
    if let Some(filename) = &type_config.filename {
        if let Some(language) = &filename.title_language {
            titles.insert(language.clone(), basename.to_string());
        }
    }
    for field in type_config
        .fields
        .iter()
        .filter(|field| field.field_type == FieldType::Title)
    {
        let key = field.title_language.as_ref().unwrap_or(&field.field);
        if titles.contains_key(key) {
            continue;
        }
        if let Some(title) = normalize_title_field(frontmatter, &field.field, basename) {
            titles.insert(key.clone(), title);
        }
    }
    titles
}

pub(super) fn default_title(
    frontmatter: &Map<String, Value>,
    titles: &BTreeMap<String, String>,
    basename: &str,
    type_config: &EntityTypeConfig,
) -> String {
    if type_config
        .filename
        .as_ref()
        .is_some_and(|filename| filename.default_title)
    {
        return basename.to_string();
    }

    type_config
        .fields
        .iter()
        .find(|field| field.field_type == FieldType::Title && field.default_title.unwrap_or(false))
        .and_then(|field| normalize_title_field(frontmatter, &field.field, basename))
        .or_else(|| type_config.filename.as_ref().map(|_| basename.to_string()))
        .or_else(|| {
            type_config
                .fields
                .iter()
                .find(|field| field.field_type == FieldType::Title)
                .and_then(|field| normalize_title_field(frontmatter, &field.field, basename))
        })
        .or_else(|| titles.values().next().cloned())
        .unwrap_or_else(|| basename.to_string())
}

fn normalize_title_field(
    frontmatter: &Map<String, Value>,
    key: &str,
    basename: &str,
) -> Option<String> {
    if matches!(key, "filename" | "basename" | "$filename" | "$basename") {
        return Some(basename.to_string());
    }
    normalize_value(frontmatter.get(key))
}

fn normalize_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Null => None,
        Value::Bool(value) => Some(if *value { "Yes" } else { "No" }.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) if value.is_empty() => None,
        Value::String(value) => Some(strip_wikilink(value)),
        Value::Array(values) => {
            let normalized: Vec<_> = values
                .iter()
                .filter_map(|item| normalize_value(Some(item)))
                .collect();
            (!normalized.is_empty()).then(|| normalized.join(", "))
        }
        Value::Object(_) => None,
    }
}

fn normalize_values(value: Option<&Value>) -> Vec<String> {
    match value {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(value)) if value.is_empty() => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .flat_map(|item| normalize_values(Some(item)))
            .collect(),
        Some(value) => normalize_value(Some(value)).into_iter().collect(),
    }
}

pub(super) fn strip_wikilink(value: &str) -> String {
    let value = value.trim();
    strip_wikilink_regex()
        .captures(value)
        .and_then(|captures| captures.get(2).or_else(|| captures.get(1)))
        .map(|capture| capture.as_str().to_string())
        .unwrap_or_else(|| value.to_string())
}

pub(super) fn extract_summary(body: &str) -> Option<String> {
    let source = summary_heading_regex()
        .find(body)
        .map(|mat| &body[mat.end()..])
        .unwrap_or(body);
    let source = source
        .split_once("\n## ")
        .map(|(head, _)| head)
        .unwrap_or(source);
    let mut cleaned = fence_regex().replace_all(source, "").to_string();
    cleaned = image_markdown_regex().replace_all(&cleaned, "").to_string();
    cleaned = markdown_link_regex().replace_all(&cleaned, "").to_string();
    cleaned = wikilink_regex().replace_all(&cleaned, "$1").to_string();
    let cleaned = cleaned
        .lines()
        .map(|line| {
            line.trim_start_matches(|c: char| "-*># ".contains(c))
                .trim()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        None
    } else if cleaned.chars().count() > 220 {
        Some(format!(
            "{}...",
            cleaned.chars().take(220).collect::<String>()
        ))
    } else {
        Some(cleaned)
    }
}
fn strip_wikilink_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|([^\]]+))?\]\]$").unwrap())
}

pub fn wikilink_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]").unwrap())
}

fn summary_heading_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?im)^##\s+(摘要|概览|简介|Summary)\s*$").unwrap())
}

pub(super) fn fence_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)```.*?```").unwrap())
}

fn image_markdown_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"!\[[^\]]*\]\([^)]+\)").unwrap())
}

fn markdown_link_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[[^\]]+\]\([^)]+\)").unwrap())
}
