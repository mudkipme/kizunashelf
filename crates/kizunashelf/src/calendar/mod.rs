mod mentions;
mod types;

pub use types::*;

use crate::contract::{CalendarFilters, CalendarResponse, CalendarTotals};
use crate::daily_notes::{daily_note_files, normalize_wikilink_target, strip_frontmatter};
use crate::dates::{clamp_number, is_in_month, normalize_date, parse_exact_date};
use crate::library::{compare_string, wikilink_regex};
use crate::relations::summary_by_id;
use crate::types::{Entity, EntitySummary, Library};
use anyhow::Result;
use chrono::Datelike;
use mentions::{clean_mention_snippet, mention_blocks};
use std::collections::HashMap;
use tokio::fs;

pub async fn build_calendar(
    library: &Library,
    options: CalendarBuildOptions,
) -> Result<CalendarResponse> {
    let mut entries = Vec::new();
    if options.source != CalendarSource::DailyNote {
        entries.extend(taxonomy_calendar_entries(library, &options));
    }
    if options.source != CalendarSource::Taxonomy {
        entries.extend(daily_note_calendar_entries(library, &options).await?);
    }
    entries.sort_by(compare_calendar_entries);
    let days = calendar_days(options.year, options.month, &entries);

    Ok(CalendarResponse {
        generated_at: library.generated_at.clone(),
        year: options.year,
        month: options.month,
        filters: CalendarFilters {
            entity_type: options.entity_type.clone(),
            source: match options.source {
                CalendarSource::All => "all",
                CalendarSource::Taxonomy => "taxonomy",
                CalendarSource::DailyNote => "daily-note",
            }
            .to_string(),
        },
        totals: CalendarTotals {
            entries: entries.len(),
            taxonomy: entries
                .iter()
                .filter(|entry| entry.source == CalendarEntrySource::Taxonomy)
                .count(),
            daily_notes: entries
                .iter()
                .filter(|entry| entry.source == CalendarEntrySource::DailyNote)
                .count(),
            days_with_entries: days.iter().filter(|day| !day.entries.is_empty()).count(),
        },
        days,
    })
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
        .map(|item| item.fields.date_roles.fields())
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
    let daily_files = daily_note_files(&library.config, None, None, true).await?;
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
        let Some(file_date) = file.date.as_ref() else {
            continue;
        };
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
                    .entry(file_date.clone())
                    .or_insert_with(|| EntityDateDailyNoteEntry {
                        id: format!("daily-note:{}:{}", file_date, entity.id),
                        date: file_date.clone(),
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
    let daily_files = daily_note_files(
        &library.config,
        Some(options.year),
        Some(options.month),
        true,
    )
    .await?;
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
        let Some(file_date) = file.date.as_ref() else {
            continue;
        };
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

                let key = format!("daily-note:{}:{}", file_date, entity.id);
                let entry = grouped.entry(key.clone()).or_insert_with(|| CalendarEntry {
                    id: key,
                    date: file_date.clone(),
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
