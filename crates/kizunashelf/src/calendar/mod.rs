mod mentions;
mod types;

pub use types::*;

use crate::contract::{CalendarFilters, CalendarResponse, CalendarTotals};
use crate::daily_notes::{
    daily_note_candidates, daily_note_files, normalize_wikilink_target, read_daily_note_contents,
    strip_frontmatter, DailyNoteFile, PendingDailyNote,
};
use crate::dates::{is_in_month, normalize_date, parse_exact_date};
use crate::library::{
    compare_string, parse_daily_note_source_id, wikilink_regex, DAILY_NOTE_RELATION_FIELD,
};
use crate::relations::summary_by_id;
use crate::types::{CanonicalStatus, DateRole, EntitySummary, EpisodeDateRole, FieldType, Library};
use crate::vfs::Vfs;
use anyhow::Result;
use chrono::Datelike;
use mentions::{clean_mention_snippet, mention_blocks, MarkdownMentionBlock};
use std::collections::{HashMap, HashSet};

/// Maximum character length of the cleaned context preview shown for a
/// daily-note mention before it is truncated with an ellipsis.
const SNIPPET_MAX_LENGTH: usize = 260;

/// Maximum number of distinct mention snippets kept per note for one entity/date.
const SNIPPET_MAX_PER_NOTE: usize = 5;

/// Appends a cleaned snippet for a mention `block`, skipping it when a snippet
/// with the same text is already present (the same mention line can recur across
/// a note's blocks). Shared by the calendar/feed entry builder and the per-entity
/// dates endpoint so both clean, dedup, and shape snippets identically.
fn push_mention_snippet(snippets: &mut Vec<CalendarSnippet>, block: &MarkdownMentionBlock) {
    let text = clean_mention_snippet(&block.text, SNIPPET_MAX_LENGTH);
    if snippets.iter().any(|snippet| snippet.text == text) {
        return;
    }
    snippets.push(CalendarSnippet {
        text,
        heading: block.heading.clone(),
        line: block.line,
    });
}

pub async fn build_calendar(
    library: &Library,
    vfs: &dyn Vfs,
    options: CalendarBuildOptions,
) -> Result<CalendarResponse> {
    // The single-month calendar view walks/indexes for just this month (no
    // page-level daily-note context to reuse).
    let entries = collect_calendar_entries(library, vfs, &options, None).await?;
    // Collapse every source into one item per (date, entity), exactly as the
    // activity feed does. `All` mode keeps every fact — it's a pure merge with no
    // recent/up-next filtering — so a client renders one card per entity per day.
    let items = group_activity_items(library, entries, &calendar_activity_options(&options));
    let days = calendar_days(options.year, options.month, &items);

    let totals = CalendarTotals {
        entries: days.iter().map(|day| day.items.len()).sum(),
        taxonomy: days.iter().map(|day| day.counts.taxonomy).sum(),
        daily_notes: days.iter().map(|day| day.counts.daily_notes).sum(),
        episodes: days.iter().map(|day| day.counts.episodes).sum(),
        days_with_entries: days.iter().filter(|day| !day.items.is_empty()).count(),
    };
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
        totals,
        days,
    })
}

/// Activity-build options that turn [`group_activity_items`] into a pure per-
/// `(date, entity)` collapse for the calendar: `All` mode keeps every fact (no
/// recent/up-next filtering), and the single-month view needs no paging cursor.
/// `today` is only read by the mode filters, so `All` leaves it empty.
fn calendar_activity_options(options: &CalendarBuildOptions) -> ActivityBuildOptions {
    ActivityBuildOptions {
        cursor: None,
        months: 1,
        min_items: None,
        entity_type: options.entity_type.clone(),
        source: options.source,
        mode: ActivityMode::All,
        today: String::new(),
    }
}

/// One month's raw calendar entries from every requested source (taxonomy dates,
/// episode dates, daily-note mentions), before any per-`(date, entity)` merge.
/// Shared by the calendar (single month, `daily_ctx: None`) and the activity feed
/// (per page, reusing a prewalked [`DailyNoteFeedContext`]).
async fn collect_calendar_entries(
    library: &Library,
    vfs: &dyn Vfs,
    options: &CalendarBuildOptions,
    daily_ctx: Option<&DailyNoteFeedContext>,
) -> Result<Vec<CalendarEntry>> {
    let mut entries = Vec::new();
    if options.source != CalendarSource::DailyNote {
        entries.extend(taxonomy_calendar_entries(library, options));
        // Episodes are entity-derived, so they ride with the taxonomy side.
        entries.extend(episode_calendar_entries(library, options));
    }
    if options.source != CalendarSource::Taxonomy {
        entries.extend(daily_note_calendar_entries(library, vfs, options, daily_ctx).await?);
    }
    Ok(entries)
}

/// Builds one page of the reverse-chronological activity feed. Reuses the three
/// month-scoped calendar builders, but pages by month: cheap discovery finds the
/// months that have any activity (taxonomy/episodes from memory, daily notes from
/// the body-free candidate walk), then only the page's months read bodies. Each
/// `(date, entity)` is collapsed into one [`ActivityItem`].
pub async fn build_activity(
    library: &Library,
    vfs: &dyn Vfs,
    options: ActivityBuildOptions,
) -> Result<ActivityResponse> {
    let ascending = options.mode == ActivityMode::UpNext;
    let current_month = month_key(&options.today);
    let mut months = active_activity_months(library, &options);
    months.retain(|month| {
        if ascending {
            // Up next is forward-only: the current month onward. A past date is
            // never "up next" — a released/aired/overdue thing you haven't done is
            // not something to act on *now*, and surfacing every past release date
            // would flood the feed. (It still appears in "recent".)
            current_month
                .as_deref()
                .is_some_and(|current| month.as_str() >= current)
                && options
                    .cursor
                    .as_deref()
                    .is_none_or(|cursor| month.as_str() > cursor)
        } else {
            // Catch up is strictly the current month and earlier (its dates have
            // passed); All/Recent look across every month.
            let within = options.mode != ActivityMode::CatchUp
                || current_month
                    .as_deref()
                    .is_some_and(|current| month.as_str() <= current);
            within
                && options
                    .cursor
                    .as_deref()
                    .is_none_or(|cursor| month.as_str() < cursor)
        }
    });
    if ascending {
        months.sort();
    } else {
        months.sort_by(|a, b| b.cmp(a));
    }

    // Walk the daily-note directory ONCE for the whole page and build the wikilink
    // index once. Both are invariant across months, but the per-month builder used
    // to re-walk every note and re-index every entity each month — the dominant
    // cost of a multi-month page. Skipped entirely for a taxonomy-only feed.
    let daily_ctx = if options.source != CalendarSource::Taxonomy {
        Some(DailyNoteFeedContext::build(library, vfs).await?)
    } else {
        None
    };

    // Two page-fill strategies: by item count (the feed — accumulate whole months
    // until the page holds enough items, so sparse months don't each cost a
    // request) or by non-empty-month count (the `/upcoming` horizon). Either way a
    // month is atomic: it's fully included or not started, keeping the cursor a
    // plain `YYYY-MM`.
    let item_target = options.min_items.map(|value| value.max(1) as usize);
    let month_target = options.months.max(1) as usize;
    let mut items = Vec::new();
    let mut filled = 0usize;
    let mut consumed = 0usize;
    for month in &months {
        consumed += 1;
        let Some((year, month_number)) = parse_month_key(month) else {
            continue;
        };
        let month_items = group_activity_items(
            library,
            month_activity_entries(
                library,
                vfs,
                year,
                month_number,
                &options,
                daily_ctx.as_ref(),
            )
            .await?,
            &options,
        );
        if month_items.is_empty() {
            continue;
        }
        items.extend(month_items);
        filled += 1;
        let page_full = match item_target {
            Some(target) => items.len() >= target,
            None => filled >= month_target,
        };
        if page_full {
            break;
        }
    }
    items.sort_by(|a, b| compare_activity_items(a, b, ascending));

    // The cursor is the last month we advanced past; the next page continues
    // strictly beyond it (older when descending, newer when ascending), so
    // already-shown months never repeat.
    let cursor = (consumed < months.len())
        .then(|| months.get(consumed - 1).cloned())
        .flatten();

    Ok(ActivityResponse {
        generated_at: library.generated_at.clone(),
        cursor,
        items,
    })
}

/// The set of `YYYY-MM` months that have any activity, newest first. Fully
/// cache-driven, no VFS I/O: taxonomy/episode dates from the resident
/// summaries/records, and daily-note months from the resident (index-cached)
/// relation graph — so paging touches the VFS only to read the chosen page's note
/// bodies, never to discover which months exist.
fn active_activity_months(library: &Library, options: &ActivityBuildOptions) -> Vec<String> {
    let mut months: HashSet<String> = HashSet::new();
    if options.source != CalendarSource::DailyNote {
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
                if let Some(month) = item.date.as_deref().and_then(month_key) {
                    months.insert(month);
                }
            }
            for item in &record.episode_dates {
                if let Some(month) = month_key(&item.date) {
                    months.insert(month);
                }
            }
        }
    }
    if options.source != CalendarSource::Taxonomy {
        // Daily-note months come from the resident relation graph (a relation per
        // mention, cached at index time), not a VFS walk — so this also skips
        // months whose notes mention nothing. A note can mention any type, so the
        // `entity_type` filter isn't applied here; the per-entry builder filters
        // precisely when the page's month is read.
        for relation in &library.relations {
            if relation.field != DAILY_NOTE_RELATION_FIELD {
                continue;
            }
            if let Some((date, _)) = parse_daily_note_source_id(&relation.source_id) {
                if let Some(month) = month_key(date) {
                    months.insert(month);
                }
            }
        }
    }
    let mut months: Vec<String> = months.into_iter().collect();
    months.sort_by(|a, b| b.cmp(a));
    months
}

/// One month's calendar entries for the activity feed: the same source assembly
/// as the calendar, but reusing the page-level prewalked daily-note context.
async fn month_activity_entries(
    library: &Library,
    vfs: &dyn Vfs,
    year: i32,
    month: u32,
    options: &ActivityBuildOptions,
    daily_ctx: Option<&DailyNoteFeedContext>,
) -> Result<Vec<CalendarEntry>> {
    let build = CalendarBuildOptions {
        year,
        month,
        entity_type: options.entity_type.clone(),
        source: options.source,
    };
    collect_calendar_entries(library, vfs, &build, daily_ctx).await
}

/// Groups a month's calendar entries by `(date, entity)` into activity items,
/// preserving first-seen order (the caller re-sorts the merged page). Items whose
/// entries are all filtered out by the mode are dropped.
fn group_activity_items(
    library: &Library,
    entries: Vec<CalendarEntry>,
    options: &ActivityBuildOptions,
) -> Vec<ActivityItem> {
    let mut order: Vec<(String, String)> = Vec::new();
    let mut groups: HashMap<(String, String), (EntitySummary, Vec<CalendarEntry>)> = HashMap::new();
    for entry in entries {
        let key = (entry.date.clone(), entry.entity.id.clone());
        groups
            .entry(key.clone())
            .or_insert_with(|| {
                order.push(key.clone());
                (entry.entity.clone(), Vec::new())
            })
            .1
            .push(entry);
    }
    order
        .into_iter()
        .filter_map(|key| {
            let (entity, grouped) = groups.remove(&key)?;
            let entries = fold_activity_entries(library, &entity, &key.0, grouped, options);
            if entries.is_empty() {
                return None;
            }
            Some(ActivityItem {
                date: key.0,
                entity,
                entries,
            })
        })
        .collect()
}

/// Folds the calendar entries for one `(date, entity)` into activity entries,
/// applying the mode filter: date-field stamps (role resolved from the schema),
/// episode dates aggregated per air/completion role, and the daily-note mention.
/// Ordered taxonomy → episode → daily-note, matching the calendar's per-source
/// ranking.
fn fold_activity_entries(
    library: &Library,
    entity: &EntitySummary,
    date: &str,
    entries: Vec<CalendarEntry>,
    options: &ActivityBuildOptions,
) -> Vec<ActivityEntry> {
    let mode = options.mode;
    let today = options.today.as_str();

    // Up-next reconciliation, read off the full group: if the entity is already
    // started/completed today, its planning-today stamp is hidden; if an episode
    // is completed today, its scheduled-today date is hidden.
    let started_or_completed_today = date == today
        && entries.iter().any(|entry| {
            entry.source == CalendarEntrySource::Taxonomy
                && matches!(
                    entry
                        .date_field
                        .as_deref()
                        .and_then(|field| date_field_role(library, &entity.entity_type, field)),
                    Some(DateRole::Started | DateRole::Completed)
                )
        });
    // Global reconciliation (up next only): an episode completed on *any* date must
    // not resurface as "up next" on its (possibly later) recorded air date — you've
    // already watched it. Read the entity's full episode-date set, not just this
    // (date, entity) group, so a watch on a different day still hides the schedule.
    let completed_episode_keys: HashSet<&str> = if mode == ActivityMode::UpNext {
        library
            .record_by_id(&entity.id)
            .map(|record| {
                record
                    .episode_dates
                    .iter()
                    .filter(|episode| episode.role == EpisodeDateRole::Completed)
                    .map(|episode| episode.key.as_str())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        HashSet::new()
    };

    // Canonical status drives the intention/record axis. A `completed`/`dropped`
    // entity never appears "up next" (the intention is fulfilled or abandoned);
    // an `ongoing` entity's *future* planning date is a spent intention; an
    // overdue planning date nags only while the entity is still `planning`.
    let status = entity.status.as_ref().and_then(|status| status.canonical);
    // Completed/dropped/paused entities are never "up next": completed is done,
    // dropped is abandoned, and paused is deliberately deferred (don't nag). Their
    // records still surface in "recent" (that path doesn't consult this).
    let up_next_blocked = matches!(
        status,
        Some(CanonicalStatus::Completed | CanonicalStatus::Dropped | CanonicalStatus::Paused)
    );
    // Recent proxy record: a `completed` entity with no explicit completed-role
    // stamp lets its planning date stand in as the completion record on that date.
    let planning_is_proxy_record = mode == ActivityMode::Recent
        && status == Some(CanonicalStatus::Completed)
        && !entity.dates.iter().any(|value| {
            date_field_role(library, &entity.entity_type, &value.field) == Some(DateRole::Completed)
        });

    let mut out = Vec::new();

    let mut date_fields: Vec<&CalendarEntry> = entries
        .iter()
        .filter(|entry| entry.source == CalendarEntrySource::Taxonomy)
        .collect();
    date_fields.sort_by(|a, b| {
        compare_string(
            a.date_field.as_deref().unwrap_or_default(),
            b.date_field.as_deref().unwrap_or_default(),
        )
    });
    for entry in date_fields {
        let role = entry
            .date_field
            .as_deref()
            .and_then(|field| date_field_role(library, &entity.entity_type, field));
        let keep = match mode {
            ActivityMode::All => true,
            ActivityMode::Recent => match role {
                // Planning is an intention, not a record — except as a proxy when
                // the entity is completed with no explicit completed-role stamp.
                Some(DateRole::Planning) => planning_is_proxy_record && date <= today,
                // Started/completed are records: shown once past-or-today. A
                // *future* record is contradictory (time travel) — kept out of
                // "recent" (a cleanup queue surfaces it instead).
                _ => date <= today,
            },
            ActivityMode::UpNext => match role {
                // Forward-only: a planning date is "up next" only when it's today or
                // later and the intention isn't spent (already ongoing) or blocked
                // (completed/dropped/paused). A *past* planning date never nags.
                Some(DateRole::Planning) => {
                    date >= today
                        && !up_next_blocked
                        && !started_or_completed_today
                        && status != Some(CanonicalStatus::Ongoing)
                }
                // An event is "up next" until it's attended: today-or-future and
                // not yet completed/dropped. A past event is missed, not upcoming.
                Some(DateRole::Event) => date >= today && !up_next_blocked,
                // Started/completed stamps are records, never "up next".
                _ => false,
            },
            ActivityMode::CatchUp => match role {
                // "Catch up": a planning date that has already passed while the
                // entity is *still* planning — released/aired, still on your list.
                // Strictly past (today belongs to "up next"); other roles never
                // qualify.
                Some(DateRole::Planning) => {
                    date < today && status == Some(CanonicalStatus::Planning)
                }
                _ => false,
            },
        };
        if !keep {
            continue;
        }
        let mut activity = activity_entry(CalendarEntrySource::Taxonomy);
        activity.role = role;
        activity.date_field = entry.date_field.clone();
        activity.raw_date = entry.raw_date.clone();
        out.push(activity);
    }

    for role in [EpisodeDateRole::Completed, EpisodeDateRole::Scheduled] {
        let keep_role = match mode {
            ActivityMode::All => true,
            ActivityMode::Recent => role == EpisodeDateRole::Completed,
            // A dropped/completed entity's remaining air dates aren't "up next".
            ActivityMode::UpNext => role == EpisodeDateRole::Scheduled && !up_next_blocked,
            // Catch up is the entity's planning date, not per-episode.
            ActivityMode::CatchUp => false,
        };
        if !keep_role {
            continue;
        }
        let episodes: Vec<ActivityEpisodeRef> = entries
            .iter()
            .filter_map(|entry| entry.episode.as_ref())
            .filter(|episode| episode.role == role)
            .filter(|episode| {
                // Up next: today-or-later, and not already completed (on any date).
                mode != ActivityMode::UpNext
                    || (date >= today && !completed_episode_keys.contains(episode.key.as_str()))
            })
            .map(|episode| ActivityEpisodeRef {
                key: episode.key.clone(),
                title: episode.title.clone(),
            })
            .collect();
        if episodes.is_empty() {
            continue;
        }
        let heading = entries
            .iter()
            .filter_map(|entry| entry.episode.as_ref())
            .find(|episode| episode.role == role)
            .map(|episode| episode.heading.clone())
            .unwrap_or_default();
        let mut activity = activity_entry(CalendarEntrySource::Episode);
        activity.episode_role = Some(role);
        activity.heading = Some(heading);
        activity.episodes = Some(episodes);
        out.push(activity);
    }

    // Daily notes carry no role, so the date decides: future is up-next,
    // today-or-past is recent.
    let keep_daily = match mode {
        ActivityMode::All => true,
        ActivityMode::Recent => date <= today,
        ActivityMode::UpNext => date > today,
        // Catch up is about unconsumed planning dates, not diary mentions.
        ActivityMode::CatchUp => false,
    };
    if keep_daily {
        let snippets: Vec<CalendarSnippet> = entries
            .iter()
            .filter(|entry| entry.source == CalendarEntrySource::DailyNote)
            .filter_map(|entry| entry.snippets.clone())
            .flatten()
            .collect();
        if let Some(note) = entries
            .iter()
            .find(|entry| entry.source == CalendarEntrySource::DailyNote)
        {
            let mut activity = activity_entry(CalendarEntrySource::DailyNote);
            activity.note_path = note.note_path.clone();
            activity.snippets = Some(snippets);
            out.push(activity);
        }
    }

    out
}

fn compare_activity_items(
    a: &ActivityItem,
    b: &ActivityItem,
    ascending: bool,
) -> std::cmp::Ordering {
    let date = if ascending {
        compare_string(&a.date, &b.date)
    } else {
        compare_string(&b.date, &a.date)
    };
    date.then_with(|| compare_string(&a.entity.type_label, &b.entity.type_label))
        .then_with(|| compare_string(&a.entity.title, &b.entity.title))
}

fn date_field_role(library: &Library, entity_type: &str, field: &str) -> Option<DateRole> {
    library
        .config
        .types
        .iter()
        .find(|item| item.id == entity_type)
        .and_then(|item| item.fields.iter().find(|item| item.field == field))
        .and_then(|item| item.date_role)
}

fn activity_entry(source: CalendarEntrySource) -> ActivityEntry {
    ActivityEntry {
        source,
        date_field: None,
        raw_date: None,
        role: None,
        note_path: None,
        snippets: None,
        episode_role: None,
        heading: None,
        episodes: None,
    }
}

/// The `YYYY-MM` prefix of a normalized `YYYY-MM-DD` date, validated.
fn month_key(date: &str) -> Option<String> {
    let prefix = date.get(..7)?;
    let bytes = prefix.as_bytes();
    let valid = bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit();
    valid.then(|| prefix.to_string())
}

fn parse_month_key(month: &str) -> Option<(i32, u32)> {
    let (year, month) = month.split_once('-')?;
    Some((year.parse().ok()?, month.parse().ok()?))
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
                            Some(
                                DateRole::Planning
                                    | DateRole::Started
                                    | DateRole::Completed
                                    | DateRole::Event
                            )
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
            push_mention_snippet(&mut entry.snippets, &block);
        }
    }

    let mut entries: Vec<_> = grouped
        .into_values()
        .map(|mut entry| {
            entry.snippets.truncate(SNIPPET_MAX_PER_NOTE);
            entry
        })
        .collect();
    entries.sort_by(|a, b| compare_string(&b.date, &a.date));
    Ok(entries)
}

/// Daily-note discovery reused across a whole activity page. Walking the daily-note
/// directory and indexing entities by basename are both invariant across months,
/// but the per-month builder used to redo them for every month — the dominant cost
/// of a multi-month feed. Built once in [`build_activity`]; each month reads only
/// its own note bodies.
struct DailyNoteFeedContext {
    /// Every dated daily-note candidate, walked once (no month filter).
    candidates: Vec<PendingDailyNote>,
    /// Entities indexed by NFC-normalized basename, for wikilink resolution.
    basename_index: HashMap<String, Vec<EntitySummary>>,
}

impl DailyNoteFeedContext {
    async fn build(library: &Library, vfs: &dyn Vfs) -> Result<Self> {
        Ok(Self {
            candidates: daily_note_candidates(&library.config, vfs, None, None, true).await?,
            basename_index: entity_basename_index(library),
        })
    }

    /// The daily-note files for one month: filter the pre-walked candidates, then
    /// read only those bodies (so an empty month costs no file reads at all).
    async fn month_files(
        &self,
        vfs: &dyn Vfs,
        year: i32,
        month: u32,
    ) -> Result<Vec<DailyNoteFile>> {
        let month_notes: Vec<&PendingDailyNote> = self
            .candidates
            .iter()
            .filter(|note| {
                note.date
                    .as_deref()
                    .is_some_and(|date| is_in_month(date, year, month))
            })
            .collect();
        let paths: Vec<String> = month_notes
            .iter()
            .map(|note| note.relative_path.clone())
            .collect();
        let mut contents: HashMap<String, String> = read_daily_note_contents(vfs, &paths)
            .await?
            .into_iter()
            .collect();
        Ok(month_notes
            .into_iter()
            .filter_map(|note| {
                contents
                    .remove(&note.relative_path)
                    .map(|contents| DailyNoteFile {
                        relative_path: note.relative_path.clone(),
                        date: note.date.clone(),
                        source_label: note.source_label.clone(),
                        contents,
                    })
            })
            .collect())
    }
}

async fn daily_note_calendar_entries(
    library: &Library,
    vfs: &dyn Vfs,
    options: &CalendarBuildOptions,
    daily_ctx: Option<&DailyNoteFeedContext>,
) -> Result<Vec<CalendarEntry>> {
    // With a page-level context, reuse the once-walked candidates + prebuilt index
    // and read only this month's bodies; without it (the single-month calendar
    // view), walk and index for just this month.
    match daily_ctx {
        Some(ctx) => {
            let files = ctx.month_files(vfs, options.year, options.month).await?;
            Ok(daily_note_entries_from_files(
                library,
                &files,
                &ctx.basename_index,
                options,
            ))
        }
        None => {
            let files = daily_note_files(
                &library.config,
                vfs,
                Some(options.year),
                Some(options.month),
                true,
            )
            .await?;
            let by_basename = entity_basename_index(library);
            Ok(daily_note_entries_from_files(
                library,
                &files,
                &by_basename,
                options,
            ))
        }
    }
}

/// Resolves the wikilink mentions in a month's daily-note files into calendar
/// entries, grouped per `(date, entity)`. Shared by the calendar and the feed.
fn daily_note_entries_from_files(
    library: &Library,
    files: &[DailyNoteFile],
    by_basename: &HashMap<String, Vec<EntitySummary>>,
    options: &CalendarBuildOptions,
) -> Vec<CalendarEntry> {
    let mut grouped: HashMap<String, CalendarEntry> = HashMap::new();
    for file in files {
        let Some(file_date) = file.date.as_ref() else {
            continue;
        };
        let raw = file.contents.clone();
        for block in mention_blocks(&strip_frontmatter(&raw)) {
            for captures in wikilink_regex().captures_iter(&block.text) {
                let Some(target) = captures.get(1) else {
                    continue;
                };
                let Some(entity) = find_entity_for_wikilink(target.as_str(), library, by_basename)
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
                if let Some(snippets) = &mut entry.snippets {
                    push_mention_snippet(snippets, &block);
                }
            }
        }
    }

    grouped
        .into_values()
        .map(|mut entry| {
            if let Some(snippets) = &mut entry.snippets {
                snippets.truncate(SNIPPET_MAX_PER_NOTE);
            }
            entry
        })
        .collect()
}

fn calendar_days(year: i32, month: u32, items: &[ActivityItem]) -> Vec<CalendarDay> {
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
            let mut day_items: Vec<ActivityItem> = items
                .iter()
                .filter(|item| item.date == date)
                .cloned()
                .collect();
            // Stable within-day order: by type label then title — the tail of the
            // old per-entry ordering, now applied to the merged items. (Each item's
            // own entries are already taxonomy → episode → daily-note from the fold.)
            day_items.sort_by(|a, b| {
                compare_string(&a.entity.type_label, &b.entity.type_label)
                    .then_with(|| compare_string(&a.entity.title, &b.entity.title))
            });
            Some(CalendarDay {
                date,
                counts: calendar_day_counts(&day_items),
                items: day_items,
            })
        })
        .collect()
}

/// Per-day counts over the merged items: `total` is the number of entity cards,
/// and the per-source counts tally the source facets across those cards (so an
/// episode binge folded into one entry counts once).
fn calendar_day_counts(items: &[ActivityItem]) -> CalendarDayCounts {
    let mut counts = CalendarDayCounts {
        total: items.len(),
        taxonomy: 0,
        daily_notes: 0,
        episodes: 0,
    };
    for entry in items.iter().flat_map(|item| &item.entries) {
        match entry.source {
            CalendarEntrySource::Taxonomy => counts.taxonomy += 1,
            CalendarEntrySource::DailyNote => counts.daily_notes += 1,
            CalendarEntrySource::Episode => counts.episodes += 1,
        }
    }
    counts
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
