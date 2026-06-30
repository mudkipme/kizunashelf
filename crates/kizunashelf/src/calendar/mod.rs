mod mentions;
mod types;

pub use types::*;

use crate::contract::{CalendarFilters, CalendarResponse, CalendarTotals};
use crate::daily_notes::{
    daily_note_candidates, daily_note_files, normalize_wikilink_target, read_daily_note_contents,
    strip_frontmatter,
};
use crate::dates::{is_in_month, normalize_date, parse_exact_date};
use crate::library::{
    compare_string, parse_daily_note_source_id, wikilink_regex, DAILY_NOTE_RELATION_FIELD,
};
use crate::relations::summary_by_id;
use crate::types::{
    DateRole, EntitySummary, EntityTypeConfig, EpisodeDateRole, FieldConfig, FieldType, Library,
};
use crate::vfs::Vfs;
use anyhow::Result;
use chrono::Datelike;
use mentions::{clean_mention_snippet, mention_blocks};
use std::collections::{HashMap, HashSet};

/// Maximum character length of the cleaned context preview shown for a
/// daily-note mention before it is truncated with an ellipsis.
const SNIPPET_MAX_LENGTH: usize = 260;

pub async fn build_calendar(
    library: &Library,
    vfs: &dyn Vfs,
    options: CalendarBuildOptions,
) -> Result<CalendarResponse> {
    let mut entries = Vec::new();
    if options.source != CalendarSource::DailyNote {
        entries.extend(taxonomy_calendar_entries(library, &options));
        // Episodes are entity-derived, so they ride with the taxonomy side.
        entries.extend(episode_calendar_entries(library, &options));
    }
    if options.source != CalendarSource::Taxonomy {
        entries.extend(daily_note_calendar_entries(library, vfs, &options).await?);
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
            episodes: entries
                .iter()
                .filter(|entry| entry.source == CalendarEntrySource::Episode)
                .count(),
            days_with_entries: days.iter().filter(|day| !day.entries.is_empty()).count(),
        },
        days,
    })
}

pub fn build_calendar_planning(
    library: &Library,
    options: CalendarPlanningOptions,
) -> CalendarPlanningResponse {
    let type_options = planning_type_options(library);
    let entity_type = options
        .entity_type
        .filter(|entity_type| type_options.iter().any(|option| option.id == *entity_type));
    let entities = selected_planning_entities(library, entity_type.as_deref());
    let mut points = planning_date_points(library, &entities);
    points.sort_by(compare_planning_points_asc);

    let year_months = planning_months(options.year, &points);
    let seasons = planning_seasons(options.year, &points);
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let upcoming = unique_planning_points_by_entity(
        points
            .iter()
            .filter(|point| point.sort_key >= today && point.role == DateRole::Planning)
            .cloned()
            .collect(),
    )
    .into_iter()
    .take(12)
    .collect::<Vec<_>>();

    let mut completed = points
        .iter()
        .filter(|point| point.sort_key <= today && point.role == DateRole::Completed)
        .cloned()
        .collect::<Vec<_>>();
    completed.sort_by(compare_planning_points_desc);
    let recently_completed = unique_planning_points_by_entity(completed)
        .into_iter()
        .take(12)
        .collect::<Vec<_>>();

    let mut started = points
        .iter()
        .filter(|point| point.sort_key <= today && point.role == DateRole::Started)
        .cloned()
        .collect::<Vec<_>>();
    started.sort_by(compare_planning_points_desc);
    let just_started = unique_planning_points_by_entity(started)
        .into_iter()
        .take(12)
        .collect::<Vec<_>>();

    CalendarPlanningResponse {
        generated_at: library.generated_at.clone(),
        filters: CalendarPlanningFilters {
            year: options.year,
            entity_type,
        },
        type_options,
        totals: CalendarPlanningTotals {
            entities: entities.len(),
            dated_entries: points.len(),
            upcoming: upcoming.len(),
            recently_completed: recently_completed.len(),
            just_started: just_started.len(),
        },
        year_months,
        seasons,
        board: CalendarPlanningBoard {
            upcoming,
            recently_completed,
            just_started,
        },
    }
}

pub async fn build_entity_dates(
    library: &Library,
    vfs: &dyn Vfs,
    summary: &EntitySummary,
) -> Result<EntityDatesResponse> {
    let metadata = metadata_date_entries(library, summary);
    let daily_notes = entity_daily_note_entries(library, vfs, summary).await?;
    let snippets = daily_notes.iter().map(|item| item.snippets.len()).sum();
    Ok(EntityDatesResponse {
        generated_at: library.generated_at.clone(),
        entity_id: summary.id.clone(),
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
    for record in &library.records {
        let entity = &record.summary;
        if options
            .entity_type
            .as_ref()
            .is_some_and(|entity_type| entity.entity_type != *entity_type)
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
                    id: format!("taxonomy:{}:{date}:{}", item.field, entity.id),
                    date,
                    source: CalendarEntrySource::Taxonomy,
                    entity: summaries
                        .get(entity.id.as_str())
                        .map(|summary| (*summary).clone())
                        .unwrap_or_else(|| entity.clone()),
                    date_field: Some(item.field),
                    raw_date: Some(item.value),
                    note_path: None,
                    snippets: None,
                    episode: None,
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

/// Calendar entries for dated episodes/tracks, read from each record's cached
/// `episode_dates` (parsed once at index time) — never re-reading bodies. One
/// entry per air (`📅`) and completion (`✅`) date that falls in the month.
fn episode_calendar_entries(
    library: &Library,
    options: &CalendarBuildOptions,
) -> Vec<CalendarEntry> {
    let summaries = summary_by_id(library);
    let mut entries = Vec::new();
    for record in &library.records {
        let entity = &record.summary;
        if options
            .entity_type
            .as_ref()
            .is_some_and(|entity_type| entity.entity_type != *entity_type)
        {
            continue;
        }
        // The section heading is schema config (not body-derived), so resolve it
        // from the type config — no need to cache it per item.
        let heading = library
            .config
            .type_config(&entity.entity_type)
            .and_then(crate::episodes::episode_section)
            .map(|section| section.heading.clone())
            .unwrap_or_default();
        for item in &record.episode_dates {
            if !is_in_month(&item.date, options.year, options.month) {
                continue;
            }
            let role = match item.role {
                EpisodeDateRole::Scheduled => "scheduled",
                EpisodeDateRole::Completed => "completed",
            };
            entries.push(CalendarEntry {
                id: format!("episode:{role}:{}:{}:{}", item.key, item.date, entity.id),
                date: item.date.clone(),
                source: CalendarEntrySource::Episode,
                entity: summaries
                    .get(entity.id.as_str())
                    .map(|summary| (*summary).clone())
                    .unwrap_or_else(|| entity.clone()),
                date_field: None,
                raw_date: None,
                note_path: None,
                snippets: None,
                episode: Some(CalendarEpisode {
                    key: item.key.clone(),
                    title: item.title.clone(),
                    role: item.role,
                    heading: heading.clone(),
                }),
            });
        }
    }
    entries
}

fn metadata_date_entries(
    library: &Library,
    entity: &EntitySummary,
) -> Vec<EntityDateMetadataEntry> {
    let fields: Vec<String> = library
        .config
        .types
        .iter()
        .find(|item| item.id == entity.entity_type)
        .map(|item| {
            item.fields
                .iter()
                .filter(|field| {
                    matches!(field.field_type, FieldType::Date | FieldType::Season)
                        && matches!(
                            field.date_role,
                            Some(DateRole::Planning | DateRole::Started | DateRole::Completed)
                        )
                })
                .map(|field| field.field.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut seen = Vec::<String>::new();
    let mut entries = Vec::new();
    for item in &entity.dates {
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

fn planning_type_options(library: &Library) -> Vec<CalendarPlanningTypeOption> {
    library
        .config
        .types
        .iter()
        .filter(|entity_type| has_planning_surface(entity_type))
        .map(|entity_type| CalendarPlanningTypeOption {
            id: entity_type.id.clone(),
            label: if entity_type.label.trim().is_empty() {
                entity_type.id.clone()
            } else {
                entity_type.label.clone()
            },
        })
        .collect()
}

fn has_planning_surface(entity_type: &EntityTypeConfig) -> bool {
    entity_type.fields.iter().any(|field| {
        matches!(field.field_type, FieldType::Enum)
            || matches!(
                field.date_role,
                Some(DateRole::Planning | DateRole::Started | DateRole::Completed)
            )
    })
}

fn selected_planning_entities(library: &Library, entity_type: Option<&str>) -> Vec<EntitySummary> {
    library
        .summaries()
        .filter(|entity| {
            entity_type.is_none_or(|expected| entity.entity_type == expected)
                && library
                    .config
                    .types
                    .iter()
                    .find(|item| item.id == entity.entity_type)
                    .is_some_and(has_planning_surface)
        })
        .cloned()
        .collect()
}

fn planning_date_points(
    library: &Library,
    entities: &[EntitySummary],
) -> Vec<CalendarPlanningDatePoint> {
    let fields_by_type = library
        .config
        .types
        .iter()
        .map(|entity_type| {
            (
                entity_type.id.as_str(),
                entity_type
                    .fields
                    .iter()
                    .filter_map(|field| {
                        field
                            .date_role
                            .map(|role| (field.field.as_str(), (role, field_display_label(field))))
                    })
                    .collect::<HashMap<_, _>>(),
            )
        })
        .collect::<HashMap<_, HashMap<_, _>>>();

    entities
        .iter()
        .flat_map(|entity| {
            let fields = fields_by_type.get(entity.entity_type.as_str());
            entity.dates.iter().filter_map(move |date| {
                let (role, field_label) = fields?.get(date.field.as_str())?.clone();
                let parsed = date.parsed.as_ref()?;
                let sort_key = date.sort_key.clone()?;
                Some(CalendarPlanningDatePoint {
                    entity: entity.clone(),
                    field: date.field.clone(),
                    field_label,
                    value: date.value.clone(),
                    year: parsed.year,
                    month: parsed.month.unwrap_or(1),
                    sort_key,
                    season: parsed.season_key.clone().or_else(|| {
                        Some(season_key_for_month(parsed.month.unwrap_or(1)).to_string())
                    }),
                    role,
                })
            })
        })
        .collect()
}

fn planning_months(year: i32, points: &[CalendarPlanningDatePoint]) -> Vec<CalendarPlanningMonth> {
    month_labels()
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let month = index as u32 + 1;
            CalendarPlanningMonth {
                month,
                label: (*label).to_string(),
                entries: unique_planning_points_by_entity(
                    points
                        .iter()
                        .filter(|point| point.year == year && point.month == month)
                        .cloned()
                        .collect(),
                ),
            }
        })
        .collect()
}

fn planning_seasons(
    year: i32,
    points: &[CalendarPlanningDatePoint],
) -> Vec<CalendarPlanningSeason> {
    season_options()
        .iter()
        .map(|season| CalendarPlanningSeason {
            key: season.0.to_string(),
            label: season.1.to_string(),
            months: season.2.to_string(),
            entries: unique_planning_points_by_entity(
                points
                    .iter()
                    .filter(|point| {
                        point.year == year
                            && point
                                .season
                                .as_deref()
                                .unwrap_or_else(|| season_key_for_month(point.month))
                                == season.0
                    })
                    .cloned()
                    .collect(),
            ),
        })
        .collect()
}

fn field_display_label(field: &FieldConfig) -> String {
    field
        .display_name
        .as_ref()
        .filter(|label| !label.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| field.field.clone())
}

fn unique_planning_points_by_entity(
    points: Vec<CalendarPlanningDatePoint>,
) -> Vec<CalendarPlanningDatePoint> {
    let mut seen = std::collections::HashSet::<String>::new();
    points
        .into_iter()
        .filter(|point| seen.insert(point.entity.id.clone()))
        .collect()
}

fn compare_planning_points_asc(
    a: &CalendarPlanningDatePoint,
    b: &CalendarPlanningDatePoint,
) -> std::cmp::Ordering {
    compare_string(&a.sort_key, &b.sort_key)
        .then_with(|| compare_string(&a.entity.title, &b.entity.title))
}

fn compare_planning_points_desc(
    a: &CalendarPlanningDatePoint,
    b: &CalendarPlanningDatePoint,
) -> std::cmp::Ordering {
    compare_string(&b.sort_key, &a.sort_key)
        .then_with(|| compare_string(&a.entity.title, &b.entity.title))
}

fn month_labels() -> [&'static str; 12] {
    [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
}

fn season_options() -> [(&'static str, &'static str, &'static str); 4] {
    [
        ("winter", "Winter", "Jan-Mar"),
        ("spring", "Spring", "Apr-Jun"),
        ("summer", "Summer", "Jul-Sep"),
        ("autumn", "Autumn", "Oct-Dec"),
    ]
}

fn season_key_for_month(month: u32) -> &'static str {
    if (4..=6).contains(&month) {
        "spring"
    } else if (7..=9).contains(&month) {
        "summer"
    } else if (10..=12).contains(&month) {
        "autumn"
    } else {
        "winter"
    }
}

async fn entity_daily_note_entries(
    library: &Library,
    vfs: &dyn Vfs,
    entity: &EntitySummary,
) -> Result<Vec<EntityDateDailyNoteEntry>> {
    // The resident relation graph already records, for every daily note, which
    // entity basenames it wikilinks — so we read only the notes that can mention
    // this entity rather than every daily note in the vault. Match on the link's
    // basename (not the resolved `target_id`): relation resolution picks the first
    // basename candidate, while the per-block check below is type-aware, so the
    // basename set is a complete superset and the block check does the precise
    // matching — the result is identical to scanning every note.
    let target_key = normalize_wikilink_target(&entity.basename);
    let wanted: HashSet<&str> = library
        .relations
        .iter()
        .filter(|relation| relation.field == DAILY_NOTE_RELATION_FIELD)
        .filter(|relation| normalize_wikilink_target(&relation.target_title) == target_key)
        .filter_map(|relation| parse_daily_note_source_id(&relation.source_id))
        .map(|(_, path)| path)
        .collect();
    if wanted.is_empty() {
        return Ok(Vec::new());
    }

    // Authoritative dates (and existence) come from the cheap, body-free discovery
    // walk; only the matched notes' bodies are then read.
    let pending: Vec<_> = daily_note_candidates(&library.config, vfs, None, None, true)
        .await?
        .into_iter()
        .filter(|note| wanted.contains(note.relative_path.as_str()))
        .collect();
    let paths: Vec<String> = pending
        .iter()
        .map(|note| note.relative_path.clone())
        .collect();
    let mut contents_by_path: HashMap<String, String> = read_daily_note_contents(vfs, &paths)
        .await?
        .into_iter()
        .collect();

    let by_basename = entity_basename_index(library);
    let mut grouped: HashMap<String, EntityDateDailyNoteEntry> = HashMap::new();
    for note in pending {
        let Some(file_date) = note.date.as_ref() else {
            continue;
        };
        let Some(raw) = contents_by_path.remove(&note.relative_path) else {
            continue;
        };
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
                        note_path: note.relative_path.clone(),
                        snippets: Vec::new(),
                    });
            let snippet = CalendarSnippet {
                text: clean_mention_snippet(&block.text, SNIPPET_MAX_LENGTH),
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
    vfs: &dyn Vfs,
    options: &CalendarBuildOptions,
) -> Result<Vec<CalendarEntry>> {
    let daily_files = daily_note_files(
        &library.config,
        vfs,
        Some(options.year),
        Some(options.month),
        true,
    )
    .await?;
    let by_basename = entity_basename_index(library);
    let mut grouped: HashMap<String, CalendarEntry> = HashMap::new();
    for file in daily_files {
        let Some(file_date) = file.date.as_ref() else {
            continue;
        };
        let raw = file.contents.clone();
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
                    episode: None,
                });
                let snippet = CalendarSnippet {
                    text: clean_mention_snippet(&block.text, SNIPPET_MAX_LENGTH),
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
        // `filter_map` rather than `unwrap`: callers clamp `month` to 1..=12, but
        // an out-of-range value here yields no day instead of panicking.
        .filter_map(|day| {
            let date = normalize_date(year, month, day)?;
            let day_entries: Vec<_> = entries
                .iter()
                .filter(|entry| entry.date == date)
                .cloned()
                .collect();
            Some(CalendarDay {
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
                    episodes: day_entries
                        .iter()
                        .filter(|entry| entry.source == CalendarEntrySource::Episode)
                        .count(),
                },
                entries: day_entries,
            })
        })
        .collect()
}

fn compare_calendar_entries(a: &CalendarEntry, b: &CalendarEntry) -> std::cmp::Ordering {
    let date_compare = compare_string(&a.date, &b.date);
    if !date_compare.is_eq() {
        return date_compare;
    }
    if a.source != b.source {
        // Stable per-source ordering within a day: taxonomy, then episodes, then
        // daily notes.
        return source_rank(a.source).cmp(&source_rank(b.source));
    }
    let type_compare = compare_string(&a.entity.type_label, &b.entity.type_label);
    if !type_compare.is_eq() {
        return type_compare;
    }
    compare_string(&a.entity.title, &b.entity.title)
}

fn source_rank(source: CalendarEntrySource) -> u8 {
    match source {
        CalendarEntrySource::Taxonomy => 0,
        CalendarEntrySource::Episode => 1,
        CalendarEntrySource::DailyNote => 2,
    }
}

fn entity_basename_index(library: &Library) -> HashMap<String, Vec<EntitySummary>> {
    let mut by_basename: HashMap<String, Vec<EntitySummary>> = HashMap::new();
    for entity in library.summaries() {
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
    candidates.first().cloned()
}

#[cfg(test)]
mod tests;
