use crate::dates::{clamp_number, is_in_month, normalize_date, parse_exact_date};
use crate::library::{compare_string, wikilink_regex};
use crate::relations::summary_by_id;
use crate::types::{Entity, EntitySummary, Library};
use anyhow::Result;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::fs;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CalendarSource {
    All,
    Taxonomy,
    DailyNote,
}

#[derive(Clone, Debug)]
pub struct CalendarBuildOptions {
    pub year: i32,
    pub month: u32,
    pub entity_type: Option<String>,
    pub source: CalendarSource,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSnippet {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    pub line: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEntry {
    pub id: String,
    pub date: String,
    pub source: CalendarEntrySource,
    pub entity: EntitySummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippets: Option<Vec<CalendarSnippet>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CalendarEntrySource {
    Taxonomy,
    DailyNote,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDay {
    pub date: String,
    pub entries: Vec<CalendarEntry>,
    pub counts: CalendarDayCounts,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDayCounts {
    pub total: usize,
    pub taxonomy: usize,
    pub daily_notes: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDateMetadataEntry {
    pub id: String,
    pub field: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDateDailyNoteEntry {
    pub id: String,
    pub date: String,
    pub note_path: String,
    pub snippets: Vec<CalendarSnippet>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDatesResponse {
    pub generated_at: String,
    pub entity_id: String,
    pub totals: EntityDatesTotals,
    pub metadata: Vec<EntityDateMetadataEntry>,
    pub daily_notes: Vec<EntityDateDailyNoteEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDatesTotals {
    pub metadata: usize,
    pub daily_notes: usize,
    pub snippets: usize,
}

pub async fn build_calendar(
    library: &Library,
    options: CalendarBuildOptions,
) -> Result<serde_json::Value> {
    let mut entries = Vec::new();
    if options.source != CalendarSource::DailyNote {
        entries.extend(taxonomy_calendar_entries(library, &options));
    }
    if options.source != CalendarSource::Taxonomy {
        entries.extend(daily_note_calendar_entries(library, &options).await?);
    }
    entries.sort_by(compare_calendar_entries);
    let days = calendar_days(options.year, options.month, &entries);

    let mut filters = serde_json::Map::new();
    if let Some(entity_type) = &options.entity_type {
        filters.insert("type".to_string(), entity_type.clone().into());
    }
    filters.insert(
        "source".to_string(),
        match options.source {
            CalendarSource::All => "all",
            CalendarSource::Taxonomy => "taxonomy",
            CalendarSource::DailyNote => "daily-note",
        }
        .into(),
    );

    Ok(serde_json::json!({
        "generatedAt": library.generated_at,
        "year": options.year,
        "month": options.month,
        "filters": filters,
        "totals": {
            "entries": entries.len(),
            "taxonomy": entries.iter().filter(|entry| entry.source == CalendarEntrySource::Taxonomy).count(),
            "dailyNotes": entries.iter().filter(|entry| entry.source == CalendarEntrySource::DailyNote).count(),
            "daysWithEntries": days.iter().filter(|day| !day.entries.is_empty()).count(),
        },
        "days": days,
    }))
}

pub async fn build_entity_dates(library: &Library, entity: &Entity) -> Result<EntityDatesResponse> {
    let metadata = metadata_date_entries(library, entity);
    let daily_notes = entity_daily_note_entries(library, &entity.summary).await?;
    let snippets = daily_notes.iter().map(|item| item.snippets.len()).sum();
    Ok(EntityDatesResponse {
        generated_at: library.generated_at.clone(),
        entity_id: entity.summary.id.clone(),
        totals: EntityDatesTotals {
            metadata: metadata.len(),
            daily_notes: daily_notes.len(),
            snippets,
        },
        metadata,
        daily_notes,
    })
}

fn taxonomy_calendar_entries(
    library: &Library,
    options: &CalendarBuildOptions,
) -> Vec<CalendarEntry> {
    let summaries = summary_by_id(library);
    let mut entries = Vec::new();
    for entity in &library.entities {
        if options
            .entity_type
            .as_ref()
            .is_some_and(|entity_type| entity.summary.entity_type != *entity_type)
        {
            continue;
        }
        for item in metadata_date_entries(library, entity) {
            if item
                .date
                .as_ref()
                .is_some_and(|date| is_in_month(date, options.year, options.month))
            {
                let date = item.date.clone().unwrap();
                entries.push(CalendarEntry {
                    id: format!("taxonomy:{}:{date}:{}", item.field, entity.summary.id),
                    date,
                    source: CalendarEntrySource::Taxonomy,
                    entity: summaries
                        .get(entity.summary.id.as_str())
                        .map(|summary| (*summary).clone())
                        .unwrap_or_else(|| entity.summary.clone()),
                    date_field: Some(item.field),
                    raw_date: Some(item.value),
                    note_path: None,
                    snippets: None,
                });
            }
        }
    }
    let mut seen = Vec::<String>::new();
    entries
        .into_iter()
        .filter(|entry| {
            if seen.contains(&entry.id) {
                false
            } else {
                seen.push(entry.id.clone());
                true
            }
        })
        .collect()
}

fn metadata_date_entries(library: &Library, entity: &Entity) -> Vec<EntityDateMetadataEntry> {
    let fields = library
        .config
        .types
        .iter()
        .find(|item| item.id == entity.summary.entity_type)
        .map(|item| item.fields.date.clone())
        .unwrap_or_default();
    let mut seen = Vec::<String>::new();
    let mut entries = Vec::new();
    for item in &entity.summary.dates {
        if !fields.is_empty() && !fields.contains(&item.field) {
            continue;
        }
        let key = format!("{}\0{}", item.field, item.value);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        entries.push(EntityDateMetadataEntry {
            id: format!("metadata:{}:{}", item.field, entries.len()),
            field: item.field.clone(),
            value: item.value.clone(),
            date: parse_exact_date(Some(&item.value)),
        });
    }
    entries.sort_by(|a, b| {
        let date_compare = compare_string(
            b.date.as_deref().unwrap_or(&b.value),
            a.date.as_deref().unwrap_or(&a.value),
        );
        if !date_compare.is_eq() {
            date_compare
        } else {
            compare_string(&a.field, &b.field)
        }
    });
    entries
}

async fn entity_daily_note_entries(
    library: &Library,
    entity: &EntitySummary,
) -> Result<Vec<EntityDateDailyNoteEntry>> {
    let daily_files = daily_note_files(library, None, None).await?;
    let by_basename = entity_basename_index(library);
    let mut grouped: HashMap<String, EntityDateDailyNoteEntry> = HashMap::new();
    let snippet_max_length = clamp_number(
        library
            .config
            .daily_notes
            .as_ref()
            .and_then(|item| item.snippet_max_length)
            .unwrap_or(260) as f64,
        80,
        600,
    ) as usize;

    for file in daily_files {
        let raw = fs::read_to_string(&file.absolute_path).await?;
        for block in mention_blocks(&strip_frontmatter(&raw)) {
            let mentions_entity = wikilink_regex().captures_iter(&block.text).any(|captures| {
                captures
                    .get(1)
                    .and_then(|target| {
                        find_entity_for_wikilink(target.as_str(), library, &by_basename)
                    })
                    .is_some_and(|candidate| candidate.id == entity.id)
            });
            if !mentions_entity {
                continue;
            }

            let entry =
                grouped
                    .entry(file.date.clone())
                    .or_insert_with(|| EntityDateDailyNoteEntry {
                        id: format!("daily-note:{}:{}", file.date, entity.id),
                        date: file.date.clone(),
                        note_path: file.relative_path.clone(),
                        snippets: Vec::new(),
                    });
            let snippet = CalendarSnippet {
                text: clean_mention_snippet(&block.text, snippet_max_length),
                heading: block.heading,
                line: block.line,
            };
            if !entry.snippets.iter().any(|item| item.text == snippet.text) {
                entry.snippets.push(snippet);
            }
        }
    }

    let mut entries: Vec<_> = grouped
        .into_values()
        .map(|mut entry| {
            entry.snippets.truncate(5);
            entry
        })
        .collect();
    entries.sort_by(|a, b| compare_string(&b.date, &a.date));
    Ok(entries)
}

async fn daily_note_calendar_entries(
    library: &Library,
    options: &CalendarBuildOptions,
) -> Result<Vec<CalendarEntry>> {
    let daily_files = daily_note_files(library, Some(options.year), Some(options.month)).await?;
    let by_basename = entity_basename_index(library);
    let mut grouped: HashMap<String, CalendarEntry> = HashMap::new();
    let snippet_max_length = clamp_number(
        library
            .config
            .daily_notes
            .as_ref()
            .and_then(|item| item.snippet_max_length)
            .unwrap_or(260) as f64,
        80,
        600,
    ) as usize;

    for file in daily_files {
        let raw = fs::read_to_string(&file.absolute_path).await?;
        for block in mention_blocks(&strip_frontmatter(&raw)) {
            for captures in wikilink_regex().captures_iter(&block.text) {
                let Some(target) = captures.get(1) else {
                    continue;
                };
                let Some(entity) = find_entity_for_wikilink(target.as_str(), library, &by_basename)
                else {
                    continue;
                };
                if options
                    .entity_type
                    .as_ref()
                    .is_some_and(|entity_type| entity.entity_type != *entity_type)
                {
                    continue;
                }

                let key = format!("daily-note:{}:{}", file.date, entity.id);
                let entry = grouped.entry(key.clone()).or_insert_with(|| CalendarEntry {
                    id: key,
                    date: file.date.clone(),
                    source: CalendarEntrySource::DailyNote,
                    entity: entity.clone(),
                    date_field: None,
                    raw_date: None,
                    note_path: Some(file.relative_path.clone()),
                    snippets: Some(Vec::new()),
                });
                let snippet = CalendarSnippet {
                    text: clean_mention_snippet(&block.text, snippet_max_length),
                    heading: block.heading.clone(),
                    line: block.line,
                };
                if let Some(snippets) = &mut entry.snippets {
                    if !snippets.iter().any(|item| item.text == snippet.text) {
                        snippets.push(snippet);
                    }
                }
            }
        }
    }

    Ok(grouped
        .into_values()
        .map(|mut entry| {
            if let Some(snippets) = &mut entry.snippets {
                snippets.truncate(5);
            }
            entry
        })
        .collect())
}

#[derive(Clone)]
struct DailyFile {
    absolute_path: PathBuf,
    relative_path: String,
    date: String,
}

async fn daily_note_files(
    library: &Library,
    year: Option<i32>,
    month: Option<u32>,
) -> Result<Vec<DailyFile>> {
    let paths = library
        .config
        .daily_notes
        .as_ref()
        .filter(|daily| !daily.paths.is_empty())
        .map(|daily| daily.paths.clone())
        .unwrap_or_else(|| vec!["Daily Notes".to_string()]);
    let pattern = library
        .config
        .daily_notes
        .as_ref()
        .and_then(|daily| daily.date_pattern.as_ref())
        .and_then(|pattern| Regex::new(pattern).ok())
        .unwrap_or_else(|| Regex::new(r"^(?<date>\d{4}-\d{2}-\d{2})\.md$").unwrap());
    let mut all_files = Vec::new();
    for path in paths {
        all_files
            .extend(walk_markdown_files(&Path::new(&library.config.vault_root).join(path)).await?);
    }

    let mut files = Vec::new();
    for absolute_path in all_files {
        let relative_path = relative_path(Path::new(&library.config.vault_root), &absolute_path);
        let basename = absolute_path
            .file_name()
            .map(|item| item.to_string_lossy().to_string())
            .unwrap_or_default();
        let date = daily_note_date(&relative_path, &pattern)
            .or_else(|| daily_note_date(&basename, &pattern));
        let Some(date) = date else {
            continue;
        };
        if year
            .zip(month)
            .is_some_and(|(year, month)| !is_in_month(&date, year, month))
        {
            continue;
        }
        files.push(DailyFile {
            absolute_path,
            relative_path,
            date,
        });
    }
    Ok(files)
}

async fn walk_markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let mut entries = match fs::read_dir(&path).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            let path = entry.path();
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && path.extension().is_some_and(|extension| extension == "md")
            {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn daily_note_date(path: &str, pattern: &Regex) -> Option<String> {
    let captures = pattern.captures(path)?;
    let date = captures
        .name("date")
        .or_else(|| captures.get(1))
        .map(|capture| capture.as_str())?;
    parse_exact_date(Some(date))
}

fn calendar_days(year: i32, month: u32, entries: &[CalendarEntry]) -> Vec<CalendarDay> {
    let count = chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
        .and_then(|date| date.pred_opt())
        .or_else(|| {
            chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1).and_then(|date| date.pred_opt())
        })
        .map(|date| date.day())
        .unwrap_or(30);
    (1..=count)
        .map(|day| {
            let date = normalize_date(year, month, day).unwrap();
            let day_entries: Vec<_> = entries
                .iter()
                .filter(|entry| entry.date == date)
                .cloned()
                .collect();
            CalendarDay {
                date,
                counts: CalendarDayCounts {
                    total: day_entries.len(),
                    taxonomy: day_entries
                        .iter()
                        .filter(|entry| entry.source == CalendarEntrySource::Taxonomy)
                        .count(),
                    daily_notes: day_entries
                        .iter()
                        .filter(|entry| entry.source == CalendarEntrySource::DailyNote)
                        .count(),
                },
                entries: day_entries,
            }
        })
        .collect()
}

fn compare_calendar_entries(a: &CalendarEntry, b: &CalendarEntry) -> std::cmp::Ordering {
    let date_compare = compare_string(&a.date, &b.date);
    if !date_compare.is_eq() {
        return date_compare;
    }
    if a.source != b.source {
        return if a.source == CalendarEntrySource::Taxonomy {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        };
    }
    let type_compare = compare_string(&a.entity.type_label, &b.entity.type_label);
    if !type_compare.is_eq() {
        return type_compare;
    }
    compare_string(&a.entity.title, &b.entity.title)
}

fn entity_basename_index(library: &Library) -> HashMap<String, Vec<EntitySummary>> {
    let mut by_basename: HashMap<String, Vec<EntitySummary>> = HashMap::new();
    for entity in &library.summaries {
        by_basename
            .entry(normalize_wikilink_target(&entity.basename))
            .or_default()
            .push(entity.clone());
    }
    by_basename
}

fn find_entity_for_wikilink(
    target: &str,
    library: &Library,
    by_basename: &HashMap<String, Vec<EntitySummary>>,
) -> Option<EntitySummary> {
    let normalized = normalize_wikilink_target(target);
    let candidates = by_basename.get(&normalized)?;
    let path_parts: Vec<_> = target
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let type_path = if path_parts.len() > 1 {
        path_parts.get(path_parts.len() - 2)
    } else {
        None
    };
    if let Some(type_path) = type_path {
        if let Some(entity_type) = library
            .config
            .types
            .iter()
            .find(|item| item.path == *type_path)
        {
            if let Some(typed) = candidates
                .iter()
                .find(|candidate| candidate.entity_type == entity_type.id)
            {
                return Some(typed.clone());
            }
        }
    }
    candidates
        .iter()
        .find(|candidate| candidate.entity_type == "franchise")
        .cloned()
        .or_else(|| candidates.first().cloned())
}

fn normalize_wikilink_target(target: &str) -> String {
    target
        .split('/')
        .last()
        .unwrap_or(target)
        .trim()
        .to_lowercase()
}

#[derive(Clone)]
struct MarkdownMentionBlock {
    text: String,
    heading: Option<String>,
    line: usize,
}

fn strip_frontmatter(raw: &str) -> String {
    if !raw.starts_with("---\n") {
        return raw.to_string();
    }
    raw[4..]
        .find("\n---")
        .map(|end| raw[end + 8..].to_string())
        .unwrap_or_else(|| raw.to_string())
}

fn mention_blocks(markdown: &str) -> Vec<MarkdownMentionBlock> {
    let mut blocks = Vec::new();
    let mut heading: Option<String> = None;
    let mut paragraph: Vec<(String, usize)> = Vec::new();
    let mut in_fence = false;

    fn push_block(
        blocks: &mut Vec<MarkdownMentionBlock>,
        text: String,
        line: usize,
        heading: Option<String>,
    ) {
        if wikilink_regex().is_match(&text) {
            blocks.push(MarkdownMentionBlock {
                text,
                heading,
                line,
            });
        }
    }

    fn flush_paragraph(
        blocks: &mut Vec<MarkdownMentionBlock>,
        paragraph: &mut Vec<(String, usize)>,
        heading: Option<String>,
    ) {
        if paragraph.is_empty() {
            return;
        }
        let text = paragraph
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        push_block(blocks, text, paragraph[0].1, heading);
        paragraph.clear();
    }

    for (index, line) in markdown.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if fence_line_regex().is_match(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph, heading.clone());
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(captures) = heading_regex().captures(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph, heading.clone());
            heading = captures
                .get(2)
                .map(|capture| clean_mention_snippet(capture.as_str(), 120));
            push_block(
                &mut blocks,
                trimmed.to_string(),
                line_number,
                heading.clone(),
            );
            continue;
        }
        if trimmed.is_empty() {
            flush_paragraph(&mut blocks, &mut paragraph, heading.clone());
            continue;
        }
        if list_or_quote_regex().is_match(trimmed) || trimmed.contains('|') {
            flush_paragraph(&mut blocks, &mut paragraph, heading.clone());
            push_block(
                &mut blocks,
                trimmed.to_string(),
                line_number,
                heading.clone(),
            );
            continue;
        }
        paragraph.push((trimmed.to_string(), line_number));
    }
    flush_paragraph(&mut blocks, &mut paragraph, heading);
    blocks
}

fn clean_mention_snippet(text: &str, max_length: usize) -> String {
    let mut cleaned = wikilink_with_alias_regex()
        .replace_all(text, |captures: &regex::Captures| {
            captures
                .get(2)
                .or_else(|| captures.get(1))
                .map(|capture| capture.as_str())
                .unwrap_or_default()
                .to_string()
        })
        .to_string();
    cleaned = image_markdown_regex().replace_all(&cleaned, "").to_string();
    cleaned = markdown_link_regex()
        .replace_all(&cleaned, "$1")
        .to_string();
    cleaned = heading_prefix_regex().replace_all(&cleaned, "").to_string();
    cleaned = quote_prefix_regex().replace_all(&cleaned, "").to_string();
    cleaned = list_prefix_regex().replace_all(&cleaned, "").to_string();
    cleaned = whitespace_regex()
        .replace_all(&cleaned, " ")
        .trim()
        .to_string();
    if cleaned.chars().count() <= max_length {
        cleaned
    } else {
        format!(
            "{}...",
            cleaned
                .chars()
                .take(max_length.saturating_sub(3))
                .collect::<String>()
                .trim()
        )
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn fence_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(```|~~~)").unwrap())
}

fn heading_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(#{1,6})\s+(.+)$").unwrap())
}

fn list_or_quote_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^([-*+]|\d+\.)\s+|^>\s+").unwrap())
}

fn wikilink_with_alias_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"!?\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|([^\]]+))?\]\]").unwrap())
}

fn image_markdown_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"!\[[^\]]*]\([^)]+\)").unwrap())
}

fn markdown_link_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[([^\]]+)]\([^)]+\)").unwrap())
}

fn heading_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^#{1,6}\s+").unwrap())
}

fn quote_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^>\s+").unwrap())
}

fn list_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^([-*+]|\d+\.)\s+").unwrap())
}

fn whitespace_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").unwrap())
}

use chrono::Datelike;
