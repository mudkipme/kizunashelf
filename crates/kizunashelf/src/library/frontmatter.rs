use crate::dates::{parse_entity_date, parsed_date_sort_key};
use crate::types::{EntityDateValue, EntityTypeConfig, FieldType, TitleRole};
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

/// The built-in tags: the frontmatter value under `tags_field` read as a list of
/// strings (a bare scalar is treated as a single-element list). Empty when absent.
/// `tags_field` is the configured key (see [`crate::types::KizunaConfig::tags_field`]).
pub(super) fn extract_tags(frontmatter: &Map<String, Value>, tags_field: &str) -> Vec<String> {
    normalize_values(frontmatter.get(tags_field))
}

pub(super) fn first_string(frontmatter: &Map<String, Value>, keys: &[String]) -> Option<String> {
    keys.iter()
        .filter_map(|key| normalize_value(frontmatter.get(key)))
        .next()
}

/// The first individual value across `keys`, taking the *first element* of a list
/// field rather than joining the whole list the way [`first_string`] does. Used to
/// derive a single cover path from an image field that may be an `ImageList`: a
/// joined `"a.jpg, b.jpg"` is not a usable path (it breaks cover display and the
/// broken-asset check), so the cover is the first image.
pub(super) fn first_list_value(
    frontmatter: &Map<String, Value>,
    keys: &[String],
) -> Option<String> {
    keys.iter()
        .find_map(|key| normalize_values(frontmatter.get(key)).into_iter().next())
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

/// The language-agnostic display title. Clients resolve a viewer-specific title
/// as `titles[language] ?? title`, so this is the fallback when the viewer's
/// language has no title: the `original`-role title (filename or field), then the
/// first title field, then any title, then the basename. (There is no
/// `defaultTitle` flag anymore — the viewer's language picks the title; this is
/// just the floor.)
pub(super) fn resolve_title(
    frontmatter: &Map<String, Value>,
    titles: &BTreeMap<String, String>,
    basename: &str,
    type_config: &EntityTypeConfig,
) -> String {
    // An `original`-role filename means the basename is the original title.
    if type_config
        .filename
        .as_ref()
        .is_some_and(|filename| filename.title_role == Some(TitleRole::Original))
    {
        return basename.to_string();
    }

    type_config
        .fields
        .iter()
        .find(|field| {
            field.field_type == FieldType::Title && field.title_role == Some(TitleRole::Original)
        })
        .and_then(|field| normalize_title_field(frontmatter, &field.field, basename))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EntityTypeConfig, FieldConfig, FieldType, FilenameConfig, TitleRole};
    use serde_json::json;

    fn fm(value: Value) -> Map<String, Value> {
        value
            .as_object()
            .expect("test frontmatter must be an object")
            .clone()
    }

    fn title_field(field: &str, language: Option<&str>, role: Option<TitleRole>) -> FieldConfig {
        FieldConfig {
            field: field.to_string(),
            field_type: FieldType::Title,
            display_name: None,
            title_language: language.map(str::to_string),
            title_role: role,
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

    fn type_config(filename: Option<FilenameConfig>, fields: Vec<FieldConfig>) -> EntityTypeConfig {
        EntityTypeConfig {
            id: "anime".to_string(),
            label: "Anime".to_string(),
            icon: None,
            path: "Anime".to_string(),
            external_priority: Vec::new(),
            filename,
            body_sections: Vec::new(),
            log: None,
            fields,
        }
    }

    // --- parse_markdown -------------------------------------------------------

    #[test]
    fn parse_markdown_without_frontmatter_returns_trimmed_body() {
        let parsed = parse_markdown("\n  Hello body  \n");
        assert!(parsed.frontmatter.is_empty());
        assert_eq!(parsed.body, "Hello body");
        assert!(parsed.diagnostics.is_empty());
    }

    #[test]
    fn parse_markdown_reads_frontmatter_and_body() {
        let parsed =
            parse_markdown("---\ntitle: Star Voyager\nstatus: Watching\n---\n\nBody text.\n");
        assert_eq!(parsed.frontmatter.get("title").unwrap(), "Star Voyager");
        assert_eq!(parsed.frontmatter.get("status").unwrap(), "Watching");
        assert_eq!(parsed.body, "Body text.");
        assert!(parsed.diagnostics.is_empty());
    }

    #[test]
    fn parse_markdown_reports_unclosed_frontmatter() {
        let parsed = parse_markdown("---\ntitle: x\n");
        assert!(parsed.frontmatter.is_empty());
        assert_eq!(parsed.diagnostics.len(), 1);
        assert!(parsed.diagnostics[0].contains("no closing delimiter"));
    }

    #[test]
    fn parse_markdown_reports_invalid_yaml() {
        let parsed = parse_markdown("---\ntitle: [unclosed\n---\n");
        assert!(parsed.frontmatter.is_empty());
        assert!(parsed.diagnostics[0].contains("invalid frontmatter YAML"));
    }

    #[test]
    fn parse_markdown_reports_non_mapping_frontmatter() {
        let parsed = parse_markdown("---\n- just\n- a list\n---\nbody");
        assert!(parsed.frontmatter.is_empty());
        assert!(parsed.diagnostics[0].contains("must be a YAML mapping"));
    }

    // --- split / serialize round-trip ----------------------------------------

    #[test]
    fn split_then_serialize_round_trips() {
        let raw = "---\ntitle: Star Voyager\n---\n\nBody text.\n";
        let doc = split_markdown_document(raw);
        assert_eq!(doc.frontmatter.get("title").unwrap(), "Star Voyager");
        let serialized = serialize_markdown_document(&doc.frontmatter, &doc.body);
        let reparsed = split_markdown_document(&serialized);
        assert_eq!(reparsed.frontmatter, doc.frontmatter);
        assert_eq!(reparsed.body, doc.body);
    }

    #[test]
    fn serialize_with_empty_frontmatter_returns_body_only() {
        assert_eq!(
            serialize_markdown_document(&Map::new(), "Just body"),
            "Just body"
        );
    }

    #[test]
    fn serialize_with_empty_body_closes_frontmatter() {
        let mut frontmatter = Map::new();
        frontmatter.insert("title".to_string(), Value::String("X".to_string()));
        assert_eq!(
            serialize_markdown_document(&frontmatter, ""),
            "---\ntitle: X\n---\n"
        );
    }

    // --- value normalization --------------------------------------------------

    #[test]
    fn first_string_normalizes_scalars_lists_and_wikilinks() {
        assert_eq!(
            first_string(&fm(json!({"a": "Hello"})), &["a".to_string()]),
            Some("Hello".to_string())
        );
        assert_eq!(
            first_string(&fm(json!({"a": true})), &["a".to_string()]),
            Some("Yes".to_string())
        );
        assert_eq!(
            first_string(&fm(json!({"a": 42})), &["a".to_string()]),
            Some("42".to_string())
        );
        assert_eq!(
            first_string(&fm(json!({"a": ["X", "Y"]})), &["a".to_string()]),
            Some("X, Y".to_string())
        );
        assert_eq!(
            first_string(&fm(json!({"a": ""})), &["a".to_string()]),
            None
        );
        // strip_wikilink prefers the alias (after `|`) on display values.
        assert_eq!(
            first_string(&fm(json!({"a": "[[Link|Alias]]"})), &["a".to_string()]),
            Some("Alias".to_string())
        );
        // The first present key wins.
        assert_eq!(
            first_string(
                &fm(json!({"b": "second"})),
                &["a".to_string(), "b".to_string()]
            ),
            Some("second".to_string())
        );
    }

    #[test]
    fn strip_wikilink_handles_plain_alias_and_heading() {
        assert_eq!(strip_wikilink("[[Star Voyager]]"), "Star Voyager");
        assert_eq!(strip_wikilink("[[Star Voyager|SV]]"), "SV");
        assert_eq!(strip_wikilink("[[Star Voyager#Episodes]]"), "Star Voyager");
        assert_eq!(strip_wikilink("Plain text"), "Plain text");
        assert_eq!(strip_wikilink("  [[Trimmed]]  "), "Trimmed");
    }

    #[test]
    fn date_values_flattens_arrays_and_skips_empties() {
        let dates = date_values(
            &fm(json!({"aired": ["2023-01-01", "2024-06-01"], "blank": ""})),
            &["aired".to_string(), "blank".to_string()],
        );
        let values: Vec<&str> = dates.iter().map(|date| date.value.as_str()).collect();
        assert_eq!(values, vec!["2023-01-01", "2024-06-01"]);
        assert_eq!(dates[0].field, "aired");
    }

    #[test]
    fn external_refs_collects_present_non_empty_keys_only() {
        let refs = external_refs(
            &fm(json!({"bangumi": "123", "igdb": ""})),
            &[
                "bangumi".to_string(),
                "igdb".to_string(),
                "missing".to_string(),
            ],
        );
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.get("bangumi"), Some(&"123".to_string()));
    }

    // --- title derivation (schema-driven) ------------------------------------

    #[test]
    fn title_languages_keys_by_filename_then_title_fields() {
        let tc = type_config(
            Some(FilenameConfig {
                title_language: Some("zh".to_string()),
                title_role: None,
            }),
            vec![
                title_field("name_jp", Some("ja"), None),
                title_field("name_en", Some("en"), None),
            ],
        );
        let titles = title_languages(
            &fm(json!({"name_jp": "スター", "name_en": "Star"})),
            "星旅",
            &tc,
        );
        assert_eq!(titles.get("zh"), Some(&"星旅".to_string())); // filename language → basename
        assert_eq!(titles.get("ja"), Some(&"スター".to_string()));
        assert_eq!(titles.get("en"), Some(&"Star".to_string()));
    }

    #[test]
    fn title_languages_filename_language_takes_precedence_over_a_field() {
        // A title field also claiming `zh` must not overwrite the filename-derived zh.
        let tc = type_config(
            Some(FilenameConfig {
                title_language: Some("zh".to_string()),
                title_role: None,
            }),
            vec![title_field("name_zh", Some("zh"), None)],
        );
        let titles = title_languages(&fm(json!({"name_zh": "中文标题"})), "星旅", &tc);
        assert_eq!(titles.get("zh"), Some(&"星旅".to_string()));
    }

    #[test]
    fn title_languages_uses_the_field_name_as_key_when_no_language_is_set() {
        let tc = type_config(None, vec![title_field("title", None, None)]);
        let titles = title_languages(&fm(json!({"title": "Field-Keyed"})), "base", &tc);
        assert_eq!(titles.get("title"), Some(&"Field-Keyed".to_string()));
    }

    #[test]
    fn resolve_title_prefers_an_original_role_filename() {
        let tc = type_config(
            Some(FilenameConfig {
                title_language: Some("zh".to_string()),
                title_role: Some(TitleRole::Original),
            }),
            vec![title_field("name_en", Some("en"), None)],
        );
        assert_eq!(
            resolve_title(
                &fm(json!({"name_en": "Star"})),
                &BTreeMap::new(),
                "星旅",
                &tc
            ),
            "星旅"
        );
    }

    #[test]
    fn resolve_title_prefers_an_original_role_title_field() {
        let tc = type_config(
            None,
            vec![
                title_field("name_en", Some("en"), None),
                title_field("name_orig", Some("ja"), Some(TitleRole::Original)),
            ],
        );
        let frontmatter = fm(json!({"name_en": "Star", "name_orig": "スター"}));
        assert_eq!(
            resolve_title(&frontmatter, &BTreeMap::new(), "base", &tc),
            "スター"
        );
    }

    #[test]
    fn resolve_title_falls_back_to_first_title_field_then_basename() {
        let tc = type_config(None, vec![title_field("name_en", Some("en"), None)]);
        assert_eq!(
            resolve_title(
                &fm(json!({"name_en": "Star"})),
                &BTreeMap::new(),
                "base",
                &tc
            ),
            "Star"
        );
        // No title value anywhere → the basename is the floor.
        assert_eq!(
            resolve_title(&fm(json!({})), &BTreeMap::new(), "base", &tc),
            "base"
        );
    }

    // --- summary extraction ---------------------------------------------------

    #[test]
    fn extract_summary_reads_after_a_summary_heading_and_strips_markup() {
        let body = "Intro line\n\n## Summary\n\nThis is the summary with a [link](http://x) and [[Wiki|Alias]].\n\n## Other\n\nIgnored.\n";
        let summary = extract_summary(body).unwrap();
        assert!(summary.contains("This is the summary with a"));
        assert!(summary.contains("Wiki")); // wikilink rendered to its target text
        assert!(!summary.contains("Intro line")); // before the heading
        assert!(!summary.contains("Ignored")); // after the next heading
        assert!(!summary.contains("http://x")); // markdown link stripped
    }

    #[test]
    fn extract_summary_without_heading_uses_the_whole_body() {
        assert_eq!(
            extract_summary("Just a paragraph.").unwrap(),
            "Just a paragraph."
        );
        assert_eq!(extract_summary("   "), None);
    }

    #[test]
    fn extract_summary_truncates_long_text() {
        let summary = extract_summary(&"a".repeat(300)).unwrap();
        assert!(summary.ends_with("..."));
        assert_eq!(summary.chars().count(), 223); // 220 + "..."
    }
}
