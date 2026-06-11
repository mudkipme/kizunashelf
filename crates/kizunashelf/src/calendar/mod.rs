mod mentions;
mod types;

pub use types::*;

use crate::contract::{CalendarFilters, CalendarResponse, CalendarTotals};
use crate::daily_notes::{daily_note_files, normalize_wikilink_target, strip_frontmatter};
use crate::dates::{clamp_number, is_in_month, normalize_date, parse_exact_date};
use crate::library::{compare_string, wikilink_regex};
use crate::relations::summary_by_id;
use crate::types::{
    DateRole, Entity, EntitySummary, EntityTypeConfig, FieldConfig, FieldType, Library,
};
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

    let future_planning_entity_ids = points
        .iter()
        .filter(|point| point.sort_key >= today && point.role == DateRole::Planning)
        .map(|point| point.entity.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut unscheduled = entities
        .iter()
        .filter(|entity| !future_planning_entity_ids.contains(entity.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    unscheduled.sort_by(compare_entity_summaries);
    unscheduled.truncate(12);

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
            unscheduled: unscheduled.len(),
        },
        year_months,
        seasons,
        board: CalendarPlanningBoard {
            upcoming,
            recently_completed,
            unscheduled,
        },
    }
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
    let fields: Vec<String> = library
        .config
        .types
        .iter()
        .find(|item| item.id == entity.summary.entity_type)
        .map(|item| {
            item.fields
                .iter()
                .filter(|field| {
                    matches!(field.field_type, FieldType::Date | FieldType::Season)
                        && matches!(
                            field.date_role,
                            Some(DateRole::Planning | DateRole::Completed)
                        )
                })
                .map(|field| field.field.clone())
                .collect()
        })
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
                Some(DateRole::Planning | DateRole::Completed)
            )
    })
}

fn selected_planning_entities(library: &Library, entity_type: Option<&str>) -> Vec<EntitySummary> {
    library
        .summaries
        .iter()
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

fn compare_entity_summaries(a: &EntitySummary, b: &EntitySummary) -> std::cmp::Ordering {
    compare_string(&a.type_label, &b.type_label).then_with(|| compare_string(&a.title, &b.title))
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
    candidates.first().cloned()
}

#[cfg(test)]
mod tests {
    use super::{entity_basename_index, find_entity_for_wikilink};
    use crate::types::{EntitySummary, EntityTypeConfig, KizunaConfig, Library};
    use std::collections::BTreeMap;

    #[test]
    fn ambiguous_daily_note_wikilinks_do_not_prefer_franchise_type() {
        let anime = summary("anime", "Anime", "Shared");
        let franchise = summary("franchise", "Franchise", "Shared");
        let library = Library {
            config: KizunaConfig {
                vault_root: String::new(),
                taxonomy_root: "Taxonomy".to_string(),
                asset_root: None,
                content_writable: None,
                read_concurrency: None,
                home: None,
                daily_notes: None,
                types: vec![
                    entity_type("anime", "Anime"),
                    entity_type("franchise", "Franchise"),
                ],
            },
            entities: Vec::new(),
            summaries: vec![anime.clone(), franchise],
            relations: Vec::new(),
            diagnostics: Vec::new(),
            generated_at: String::new(),
        };
        let by_basename = entity_basename_index(&library);

        let resolved = find_entity_for_wikilink("Shared", &library, &by_basename).unwrap();

        assert_eq!(resolved.id, anime.id);
    }

    fn entity_type(id: &str, label: &str) -> EntityTypeConfig {
        EntityTypeConfig {
            id: id.to_string(),
            label: label.to_string(),
            icon: None,
            path: label.to_string(),
            external_priority: Vec::new(),
            filename: None,
            body_mappings: Vec::new(),
            fields: Vec::new(),
        }
    }

    fn summary(entity_type: &str, type_label: &str, basename: &str) -> EntitySummary {
        EntitySummary {
            id: format!("{entity_type}:{basename}"),
            entity_type: entity_type.to_string(),
            type_label: type_label.to_string(),
            title: basename.to_string(),
            titles: BTreeMap::new(),
            dates: Vec::new(),
            image: None,
            summary: None,
            path: format!("Taxonomy/{type_label}/{basename}.md"),
            basename: basename.to_string(),
            external_refs: BTreeMap::new(),
            relation_count: 0,
        }
    }
}
