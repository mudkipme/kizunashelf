use super::*;
use crate::types::{
    DateRole, EntityDateValue, EntityRecord, EntitySummary, EntityTypeConfig, EpisodeDate,
    EpisodeDateRole, FieldConfig, FieldType, KizunaConfig, Library, Relation, RelationDirection,
};
use crate::vfs::InMemoryVfs;
use std::collections::BTreeMap;

#[test]
fn ambiguous_daily_note_wikilinks_do_not_prefer_franchise_type() {
    let anime = summary("anime", "Anime", "Shared");
    let franchise = summary("franchise", "Franchise", "Shared");
    let library = Library::new(
        KizunaConfig {
            vault_root: String::new(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            tags: None,
            types: vec![
                entity_type("anime", "Anime"),
                entity_type("franchise", "Franchise"),
            ],
        },
        vec![record(anime.clone()), record(franchise)],
        Vec::new(),
        Vec::new(),
        String::new(),
    );
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
        body_sections: Vec::new(),
        log: None,
        fields: Vec::new(),
    }
}

fn record(summary: EntitySummary) -> EntityRecord {
    EntityRecord {
        summary,
        revision: String::new(),
        frontmatter: serde_json::Map::new(),
        body_links: Vec::new(),
        file_modified_unix_nanos: 0,
        episode_dates: Vec::new(),
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
        tags: Vec::new(),
        episode_progress: None,
        relation_count: 0,
    }
}

// --- date bucketing -------------------------------------------------------

fn entry(id: &str, date: &str, source: CalendarEntrySource, title: &str) -> CalendarEntry {
    CalendarEntry {
        id: id.to_string(),
        date: date.to_string(),
        source,
        entity: summary("anime", "Anime", title),
        date_field: None,
        raw_date: None,
        note_path: None,
        snippets: None,
        episode: None,
    }
}

#[test]
fn calendar_days_length_matches_the_month() {
    assert_eq!(calendar_days(2023, 2, &[]).len(), 28); // non-leap February
    assert_eq!(calendar_days(2024, 2, &[]).len(), 29); // leap February
    assert_eq!(calendar_days(2024, 4, &[]).len(), 30); // April
    assert_eq!(calendar_days(2024, 12, &[]).len(), 31); // December
    assert!(calendar_days(2024, 13, &[]).is_empty()); // out-of-range month
}

#[test]
fn calendar_days_buckets_entries_and_counts_by_source() {
    let entries = vec![
        entry("e1", "2024-02-10", CalendarEntrySource::Taxonomy, "A"),
        entry("e2", "2024-02-10", CalendarEntrySource::DailyNote, "B"),
        entry("e3", "2024-02-11", CalendarEntrySource::Taxonomy, "C"),
        entry("e4", "2024-03-01", CalendarEntrySource::Taxonomy, "D"), // other month
    ];
    let days = calendar_days(2024, 2, &entries);

    let tenth = days.iter().find(|day| day.date == "2024-02-10").unwrap();
    assert_eq!(tenth.counts.total, 2);
    assert_eq!(tenth.counts.taxonomy, 1);
    assert_eq!(tenth.counts.daily_notes, 1);

    let eleventh = days.iter().find(|day| day.date == "2024-02-11").unwrap();
    assert_eq!(eleventh.counts.total, 1);

    // The March entry never lands in a February day.
    assert!(days
        .iter()
        .flat_map(|day| &day.entries)
        .all(|entry| entry.date.starts_with("2024-02")));
}

#[test]
fn compare_calendar_entries_orders_by_date_then_source_then_title() {
    use std::cmp::Ordering;
    let tax = |date: &str, title: &str| entry("x", date, CalendarEntrySource::Taxonomy, title);
    let daily = |date: &str, title: &str| entry("x", date, CalendarEntrySource::DailyNote, title);

    assert_eq!(
        compare_calendar_entries(&tax("2024-01-01", "A"), &tax("2024-01-02", "A")),
        Ordering::Less
    );
    // Same date: taxonomy sorts before daily-note regardless of title.
    assert_eq!(
        compare_calendar_entries(&tax("2024-01-01", "Z"), &daily("2024-01-01", "A")),
        Ordering::Less
    );
    // Same date + source: by title.
    assert_eq!(
        compare_calendar_entries(&tax("2024-01-01", "A"), &tax("2024-01-01", "B")),
        Ordering::Less
    );
}

// --- metadata date entries (schema-driven) -------------------------------

fn date_field(name: &str, role: Option<DateRole>) -> FieldConfig {
    FieldConfig {
        field: name.to_string(),
        field_type: FieldType::Date,
        display_name: None,
        title_language: None,
        title_role: None,
        external_fields: Vec::new(),
        enum_options: Vec::new(),
        total_progress_field: None,
        date_role: role,
        season_language: None,
        external_ref: None,
        external_types: Vec::new(),
        relation_type: None,
    }
}

fn date_value(field: &str, value: &str) -> EntityDateValue {
    EntityDateValue {
        field: field.to_string(),
        value: value.to_string(),
        parsed: None,
        sort_key: None,
    }
}

#[test]
fn metadata_date_entries_only_includes_schema_date_fields_with_a_role() {
    let mut entity = summary("anime", "Anime", "Alpha");
    entity.dates = vec![
        date_value("aired", "2023-05-01"),
        date_value("planned", "2024-01-01"),
        date_value("random", "2022-01-01"), // Date field but no date_role → excluded
        date_value("unknown", "2021-01-01"), // not a schema field → excluded
    ];
    let config = KizunaConfig {
        vault_root: String::new(),
        taxonomy_root: "Taxonomy".to_string(),
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
                date_field("aired", Some(DateRole::Completed)),
                date_field("planned", Some(DateRole::Planning)),
                date_field("random", None),
            ],
        }],
    };
    let library = Library::new(
        config,
        vec![record(entity.clone())],
        Vec::new(),
        Vec::new(),
        String::new(),
    );

    let entries = metadata_date_entries(&library, &entity);
    // Only the two role-bearing date fields, sorted by date descending.
    let fields: Vec<_> = entries.iter().map(|entry| entry.field.as_str()).collect();
    assert_eq!(fields, ["planned", "aired"]);
}

#[test]
fn episode_calendar_entries_place_cached_dates_in_the_month() {
    let mut entity = summary("anime", "Anime", "Star Voyager");
    entity.id = "anime:sv".to_string();
    let mut record = record(entity);
    record.episode_dates = vec![
        EpisodeDate {
            key: "1".to_string(),
            title: "Pilot".to_string(),
            date: "2024-02-10".to_string(),
            role: EpisodeDateRole::Scheduled,
        },
        EpisodeDate {
            key: "1".to_string(),
            title: "Pilot".to_string(),
            date: "2024-02-12".to_string(),
            role: EpisodeDateRole::Completed,
        },
        EpisodeDate {
            key: "2".to_string(),
            title: "Dawn".to_string(),
            date: "2024-03-01".to_string(), // other month — excluded
            role: EpisodeDateRole::Scheduled,
        },
    ];
    let mut anime_type = entity_type("anime", "Anime");
    anime_type.body_sections = vec![crate::types::BodySection {
        heading: "Tracks".to_string(),
        kind: crate::types::BodySectionKind::Episodes,
        external_fields: Vec::new(),
        tracking: None,
    }];
    let library = Library::new(
        KizunaConfig {
            vault_root: String::new(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            tags: None,
            types: vec![anime_type],
        },
        vec![record],
        Vec::new(),
        Vec::new(),
        String::new(),
    );
    let options = CalendarBuildOptions {
        year: 2024,
        month: 2,
        entity_type: None,
        source: CalendarSource::All,
    };

    let entries = episode_calendar_entries(&library, &options);
    assert_eq!(entries.len(), 2); // March entry filtered out
    assert!(entries
        .iter()
        .all(|entry| entry.source == CalendarEntrySource::Episode));
    let completed = entries
        .iter()
        .find(|entry| entry.date == "2024-02-12")
        .unwrap();
    let episode = completed.episode.as_ref().unwrap();
    assert_eq!(episode.role, EpisodeDateRole::Completed);
    assert_eq!(episode.title, "Pilot");
    // The heading comes from the type's episodes section (schema-driven).
    assert_eq!(episode.heading, "Tracks");
}

// --- activity feed --------------------------------------------------------

fn activity_config(daily_paths: Option<Vec<String>>) -> KizunaConfig {
    let mut anime = entity_type("anime", "Anime");
    anime.fields = vec![
        date_field("aired", Some(DateRole::Completed)),
        date_field("planned", Some(DateRole::Planning)),
    ];
    anime.body_sections = vec![crate::types::BodySection {
        heading: "Episodes".to_string(),
        kind: crate::types::BodySectionKind::Episodes,
        external_fields: Vec::new(),
        tracking: None,
    }];
    KizunaConfig {
        vault_root: String::new(),
        taxonomy_root: "Taxonomy".to_string(),
        asset_root: None,
        content_writable: None,
        home: None,
        daily_notes: daily_paths.map(|paths| crate::types::DailyNotesConfig {
            paths,
            date_format: None,
            template: None,
            log: None,
        }),
        tags: None,
        types: vec![anime],
    }
}

fn activity_options(cursor: Option<&str>, months: u32) -> ActivityBuildOptions {
    activity_options_mode(cursor, months, ActivityMode::All, "2024-06-15")
}

fn activity_options_mode(
    cursor: Option<&str>,
    months: u32,
    mode: ActivityMode,
    today: &str,
) -> ActivityBuildOptions {
    ActivityBuildOptions {
        cursor: cursor.map(str::to_string),
        months,
        entity_type: None,
        source: CalendarSource::All,
        mode,
        today: today.to_string(),
    }
}

/// An entity completed in the past (2024-05-10) and planned for the future
/// (2024-07-01), with one completed and one scheduled episode on those dates.
fn forward_and_back_record() -> EntityRecord {
    let mut entity = summary("anime", "Anime", "Star Voyager");
    entity.id = "anime:sv".to_string();
    entity.dates = vec![
        date_value("aired", "2024-05-10"),
        date_value("planned", "2024-07-01"),
    ];
    let mut rec = record(entity);
    rec.episode_dates = vec![
        EpisodeDate {
            key: "1".to_string(),
            title: "Pilot".to_string(),
            date: "2024-05-10".to_string(),
            role: EpisodeDateRole::Completed,
        },
        EpisodeDate {
            key: "2".to_string(),
            title: "Dawn".to_string(),
            date: "2024-07-01".to_string(),
            role: EpisodeDateRole::Scheduled,
        },
    ];
    rec
}

#[tokio::test]
async fn build_activity_collapses_all_sources_for_one_date_and_entity() {
    let mut entity = summary("anime", "Anime", "Star Voyager");
    entity.id = "anime:sv".to_string();
    entity.dates = vec![date_value("aired", "2024-02-12")];
    let mut rec = record(entity);
    rec.episode_dates = vec![EpisodeDate {
        key: "1".to_string(),
        title: "Pilot".to_string(),
        date: "2024-02-12".to_string(),
        role: EpisodeDateRole::Completed,
    }];
    let library = Library::new(
        activity_config(Some(vec!["Journal".to_string()])),
        vec![rec],
        Vec::new(),
        Vec::new(),
        String::new(),
    );

    let vfs = InMemoryVfs::new();
    vfs.insert_file(
        "Journal/2024-02-12.md",
        "- watched [[Star Voyager]] 12 #Anime\n",
    );

    let response = build_activity(&library, &vfs, activity_options(None, 12))
        .await
        .unwrap();

    // The daily-note line, the completed date stamp, and the episode completion
    // all share (2024-02-12, anime:sv) and collapse into one item.
    assert_eq!(response.items.len(), 1);
    let item = &response.items[0];
    assert_eq!(item.date, "2024-02-12");
    assert_eq!(item.entity.id, "anime:sv");
    assert_eq!(item.entries.len(), 3);
    let sources: Vec<_> = item.entries.iter().map(|entry| entry.source).collect();
    assert!(sources.contains(&CalendarEntrySource::Taxonomy));
    assert!(sources.contains(&CalendarEntrySource::Episode));
    assert!(sources.contains(&CalendarEntrySource::DailyNote));

    let date_field = item
        .entries
        .iter()
        .find(|entry| entry.source == CalendarEntrySource::Taxonomy)
        .unwrap();
    // The role is resolved from the schema, not guessed from the field name.
    assert_eq!(date_field.role, Some(DateRole::Completed));
    assert_eq!(date_field.date_field.as_deref(), Some("aired"));

    let note = item
        .entries
        .iter()
        .find(|entry| entry.source == CalendarEntrySource::DailyNote)
        .unwrap();
    assert!(note
        .snippets
        .as_ref()
        .unwrap()
        .iter()
        .any(|snippet| snippet.text.contains("12")));
}

#[tokio::test]
async fn build_activity_aggregates_an_episode_binge_into_one_entry() {
    let mut entity = summary("anime", "Anime", "Star Voyager");
    entity.id = "anime:sv".to_string();
    let mut rec = record(entity);
    rec.episode_dates = (3..=6)
        .map(|number| EpisodeDate {
            key: number.to_string(),
            title: format!("Episode {number}"),
            date: "2024-05-01".to_string(),
            role: EpisodeDateRole::Completed,
        })
        .collect();
    let library = Library::new(
        activity_config(None),
        vec![rec],
        Vec::new(),
        Vec::new(),
        String::new(),
    );
    let vfs = InMemoryVfs::new();

    let response = build_activity(&library, &vfs, activity_options(None, 12))
        .await
        .unwrap();

    assert_eq!(response.items.len(), 1);
    let entries = &response.items[0].entries;
    assert_eq!(entries.len(), 1);
    let episode = &entries[0];
    assert_eq!(episode.source, CalendarEntrySource::Episode);
    assert_eq!(episode.episode_role, Some(EpisodeDateRole::Completed));
    let keys: Vec<_> = episode
        .episodes
        .as_ref()
        .unwrap()
        .iter()
        .map(|item| item.key.as_str())
        .collect();
    assert_eq!(keys, ["3", "4", "5", "6"]);
}

#[tokio::test]
async fn build_activity_pages_by_month_and_terminates() {
    let mut records = Vec::new();
    for (index, date) in ["2024-01-15", "2024-03-15", "2024-05-15"]
        .iter()
        .enumerate()
    {
        let mut entity = summary("anime", "Anime", &format!("Show {index}"));
        entity.id = format!("anime:{index}");
        entity.dates = vec![date_value("aired", date)];
        records.push(record(entity));
    }
    let library = Library::new(
        activity_config(None),
        records,
        Vec::new(),
        Vec::new(),
        String::new(),
    );
    let vfs = InMemoryVfs::new();

    let page1 = build_activity(&library, &vfs, activity_options(None, 1))
        .await
        .unwrap();
    assert_eq!(page1.items.len(), 1);
    assert_eq!(page1.items[0].date, "2024-05-15");
    assert_eq!(page1.cursor.as_deref(), Some("2024-05"));

    let page2 = build_activity(&library, &vfs, activity_options(page1.cursor.as_deref(), 1))
        .await
        .unwrap();
    assert_eq!(page2.items[0].date, "2024-03-15");
    assert_eq!(page2.cursor.as_deref(), Some("2024-03"));

    let page3 = build_activity(&library, &vfs, activity_options(page2.cursor.as_deref(), 1))
        .await
        .unwrap();
    assert_eq!(page3.items[0].date, "2024-01-15");
    assert_eq!(page3.cursor, None);
}

#[tokio::test]
async fn build_activity_recent_hides_forward_looking() {
    let library = Library::new(
        activity_config(None),
        vec![forward_and_back_record()],
        Vec::new(),
        Vec::new(),
        String::new(),
    );
    let vfs = InMemoryVfs::new();

    let response = build_activity(
        &library,
        &vfs,
        activity_options_mode(None, 12, ActivityMode::Recent, "2024-06-15"),
    )
    .await
    .unwrap();

    // Only the completed date + completed episode on 2024-05-10 survive; the
    // planning date and scheduled episode (both 2024-07-01) are hidden.
    assert_eq!(response.items.len(), 1);
    let item = &response.items[0];
    assert_eq!(item.date, "2024-05-10");
    assert!(item
        .entries
        .iter()
        .any(|entry| entry.role == Some(DateRole::Completed)));
    assert!(item
        .entries
        .iter()
        .any(|entry| entry.episode_role == Some(EpisodeDateRole::Completed)));
    assert!(response
        .items
        .iter()
        .flat_map(|item| &item.entries)
        .all(|entry| entry.role != Some(DateRole::Planning)
            && entry.episode_role != Some(EpisodeDateRole::Scheduled)));
}

#[tokio::test]
async fn build_activity_up_next_shows_only_future_ascending() {
    let library = Library::new(
        activity_config(None),
        vec![forward_and_back_record()],
        Vec::new(),
        Vec::new(),
        String::new(),
    );
    let vfs = InMemoryVfs::new();

    let response = build_activity(
        &library,
        &vfs,
        activity_options_mode(None, 12, ActivityMode::UpNext, "2024-06-15"),
    )
    .await
    .unwrap();

    // Only the future month: the planning date + scheduled episode on 2024-07-01.
    assert_eq!(response.items.len(), 1);
    let item = &response.items[0];
    assert_eq!(item.date, "2024-07-01");
    assert!(item
        .entries
        .iter()
        .any(|entry| entry.role == Some(DateRole::Planning)));
    assert!(item
        .entries
        .iter()
        .any(|entry| entry.episode_role == Some(EpisodeDateRole::Scheduled)));
    assert!(response
        .items
        .iter()
        .flat_map(|item| &item.entries)
        .all(|entry| entry.role != Some(DateRole::Completed)
            && entry.episode_role != Some(EpisodeDateRole::Completed)));
}

#[tokio::test]
async fn build_activity_up_next_hides_today_items_already_done() {
    let mut entity = summary("anime", "Anime", "Star Voyager");
    entity.id = "anime:sv".to_string();
    entity.dates = vec![
        date_value("planned", "2024-06-15"),
        date_value("aired", "2024-06-15"),
    ];
    let mut rec = record(entity);
    rec.episode_dates = vec![
        EpisodeDate {
            key: "5".to_string(),
            title: "Five".to_string(),
            date: "2024-06-15".to_string(),
            role: EpisodeDateRole::Scheduled,
        },
        EpisodeDate {
            key: "5".to_string(),
            title: "Five".to_string(),
            date: "2024-06-15".to_string(),
            role: EpisodeDateRole::Completed,
        },
    ];
    let library = Library::new(
        activity_config(None),
        vec![rec],
        Vec::new(),
        Vec::new(),
        String::new(),
    );
    let vfs = InMemoryVfs::new();

    let response = build_activity(
        &library,
        &vfs,
        activity_options_mode(None, 12, ActivityMode::UpNext, "2024-06-15"),
    )
    .await
    .unwrap();

    // Planning-today is hidden (the entity is completed today) and scheduled
    // episode 5 is hidden (episode 5 is completed today) → nothing remains.
    assert!(response.items.is_empty());
}

#[tokio::test]
async fn build_activity_up_next_hides_episode_completed_on_a_different_day() {
    // Episode 5 airs 2024-07-01 (future) but was watched early on 2024-06-10. The
    // completion lives in a *different* (date, entity) group, so only a global
    // reconciliation — reading the entity's full episode-date set — hides the future
    // air date from "up next".
    let mut entity = summary("anime", "Anime", "Star Voyager");
    entity.id = "anime:sv".to_string();
    let mut rec = record(entity);
    rec.episode_dates = vec![
        EpisodeDate {
            key: "5".to_string(),
            title: "Five".to_string(),
            date: "2024-07-01".to_string(),
            role: EpisodeDateRole::Scheduled,
        },
        EpisodeDate {
            key: "5".to_string(),
            title: "Five".to_string(),
            date: "2024-06-10".to_string(),
            role: EpisodeDateRole::Completed,
        },
    ];
    let library = Library::new(
        activity_config(None),
        vec![rec],
        Vec::new(),
        Vec::new(),
        String::new(),
    );
    let vfs = InMemoryVfs::new();

    let response = build_activity(
        &library,
        &vfs,
        activity_options_mode(None, 12, ActivityMode::UpNext, "2024-06-15"),
    )
    .await
    .unwrap();

    // The early watch (2024-06-10) is past, and the future air date (2024-07-01) is
    // suppressed because episode 5 is already completed — so nothing is up next.
    assert!(response.items.is_empty());
}

#[tokio::test]
async fn build_activity_discovers_daily_note_only_months_from_relations() {
    // No dates and no episodes — August's only signal is a daily-note mention, so
    // the month must be discovered from the resident (index-cached) relation graph
    // rather than a VFS walk. This pins the cache-driven discovery.
    let mut entity = summary("anime", "Anime", "Star Voyager");
    entity.id = "anime:sv".to_string();
    let relation = Relation {
        source_id: "daily-note:2024-08-20:Journal/2024-08-20.md".to_string(),
        target_id: Some(entity.id.clone()),
        target_title: "Star Voyager".to_string(),
        target_type: Some("anime".to_string()),
        field: "daily-note".to_string(),
        direction: RelationDirection::Out,
    };
    let library = Library::new(
        activity_config(Some(vec!["Journal".to_string()])),
        vec![record(entity)],
        vec![relation],
        Vec::new(),
        String::new(),
    );

    let vfs = InMemoryVfs::new();
    vfs.insert_file(
        "Journal/2024-08-20.md",
        "- watched [[Star Voyager]] 1 #Anime\n",
    );

    let response = build_activity(&library, &vfs, activity_options(None, 12))
        .await
        .unwrap();

    assert_eq!(response.items.len(), 1);
    let item = &response.items[0];
    assert_eq!(item.date, "2024-08-20");
    assert_eq!(item.entity.id, "anime:sv");
    assert!(item
        .entries
        .iter()
        .any(|entry| entry.source == CalendarEntrySource::DailyNote));
}
