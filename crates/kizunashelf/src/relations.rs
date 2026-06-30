use crate::library::{compare_string, compare_string_for_title_language};
use crate::types::{EntityRecord, EntitySummary, Library, Relation, RelationDirection};
use std::cmp::Ordering;
use std::collections::HashMap;

pub fn relation_type_pairs(library: &Library) -> Vec<Count> {
    let by_id = summary_by_id(library);
    count_by(&outgoing_relations(library, None), |relation| {
        let target_label = relation
            .target_id
            .as_ref()
            .and_then(|target_id| by_id.get(target_id.as_str()))
            .map(|target| target.type_label.clone())
            .or_else(|| {
                relation
                    .target_type
                    .as_ref()
                    .and_then(|target_type| type_label(library, Some(target_type)))
            });
        format!(
            "{} -> {}",
            relation_source_type_label(relation, &by_id).as_str(),
            target_label.as_deref().unwrap_or("Unresolved")
        )
    })
}

pub fn outgoing_relations<'a>(library: &'a Library, field: Option<&str>) -> Vec<&'a Relation> {
    library
        .relations
        .iter()
        .filter(|relation| {
            relation.direction == RelationDirection::Out
                && field.map(|field| relation.field == field).unwrap_or(true)
        })
        .collect()
}

pub fn relation_source_type_label(
    relation: &Relation,
    entity_by_id: &HashMap<&str, &EntitySummary>,
) -> String {
    entity_by_id
        .get(relation.source_id.as_str())
        .map(|source| source.type_label.clone())
        .unwrap_or_else(|| {
            if relation.source_id.starts_with("daily-note:") {
                "Daily Note".to_string()
            } else {
                "Unknown".to_string()
            }
        })
}

pub fn target_key(relation: &Relation) -> &str {
    relation
        .target_id
        .as_deref()
        .unwrap_or(&relation.target_title)
}

pub fn summary_by_id(library: &Library) -> HashMap<&str, &EntitySummary> {
    library
        .summaries()
        .map(|entity| (entity.id.as_str(), entity))
        .collect()
}

pub fn type_label(library: &Library, entity_type: Option<&String>) -> Option<String> {
    let entity_type = entity_type?;
    library
        .config
        .types
        .iter()
        .find(|item| item.id == *entity_type)
        .map(|item| item.label.clone())
        .or_else(|| Some(entity_type.clone()))
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct Count {
    pub name: String,
    pub count: usize,
}

pub fn count_by<T, F>(items: &[T], select: F) -> Vec<Count>
where
    F: Fn(&T) -> String,
{
    let mut counts = HashMap::<String, usize>::new();
    for item in items {
        let key = select(item);
        *counts.entry(key).or_insert(0) += 1;
    }
    let mut counts = counts
        .into_iter()
        .map(|(name, count)| Count { name, count })
        .collect::<Vec<_>>();
    counts.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| compare_string(&a.name, &b.name))
    });
    counts
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortDirection {
    Asc,
    Desc,
}

pub fn sort_entities(
    entities: Vec<EntitySummary>,
    sort: &str,
    direction: SortDirection,
) -> Vec<EntitySummary> {
    sort_entities_with_title_language(entities, sort, direction, None)
}

pub fn sort_entities_with_title_language(
    mut entities: Vec<EntitySummary>,
    sort: &str,
    direction: SortDirection,
    title_language: Option<&str>,
) -> Vec<EntitySummary> {
    entities.sort_by(|a, b| {
        // Field-based sorts can have an absent value for some entities (e.g. no
        // release date). Those entities always sort to the bottom regardless of
        // direction; only the present values are ordered by `direction`.
        if let Some(field) = sort.strip_prefix("date:") {
            return compare_optional_empty_last(
                entity_date_sort_value(a, field).as_deref(),
                entity_date_sort_value(b, field).as_deref(),
                direction,
            );
        }

        let ordering = if sort == "title" {
            compare_entity_title(a, b, title_language)
        } else if sort == "relationCount" {
            a.relation_count.cmp(&b.relation_count)
        } else {
            let type_compare = compare_string(&a.type_label, &b.type_label);
            if type_compare != Ordering::Equal {
                type_compare
            } else {
                compare_entity_title(a, b, title_language)
            }
        };
        apply_direction(ordering, direction)
    });
    entities
}

fn apply_direction(ordering: Ordering, direction: SortDirection) -> Ordering {
    if direction == SortDirection::Asc {
        ordering
    } else {
        ordering.reverse()
    }
}

/// Sorts records by the vault file's modification time, ordered by `direction`
/// like a date field: `Asc` is oldest-first, `Desc` is newest-first (most
/// recently updated). Records with an unknown mtime (`0`) always sort last
/// regardless of direction. Ties break by title.
///
/// This is record-level (not summary-level) because the timestamp is resident
/// only on [`EntityRecord`] and is never serialized to clients.
pub fn sort_records_by_modified<'a>(
    mut records: Vec<&'a EntityRecord>,
    direction: SortDirection,
    title_language: Option<&str>,
) -> Vec<&'a EntityRecord> {
    records.sort_by(|a, b| {
        match compare_modified_time(
            a.file_modified_unix_nanos,
            b.file_modified_unix_nanos,
            direction,
        ) {
            Ordering::Equal => compare_entity_title(&a.summary, &b.summary, title_language),
            ordering => ordering,
        }
    });
    records
}

/// Orders two file timestamps by `direction` (older first under `Asc`, newer
/// first under `Desc`); an unknown time (`0`) always sorts last, independent of
/// direction.
fn compare_modified_time(a: u128, b: u128, direction: SortDirection) -> Ordering {
    match (a, b) {
        (0, 0) => Ordering::Equal,
        (0, _) => Ordering::Greater,
        (_, 0) => Ordering::Less,
        (a, b) => apply_direction(a.cmp(&b), direction),
    }
}

/// Orders two optional sort values so that an absent value always sorts last,
/// independent of `direction`; present values are compared against each other and
/// only that comparison follows `direction`. This pins entities with an empty
/// field (e.g. no release date) to the bottom in both ascending and descending
/// order, instead of letting them flip to the top in descending.
fn compare_optional_empty_last(
    a: Option<&str>,
    b: Option<&str>,
    direction: SortDirection,
) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => apply_direction(compare_string(a, b), direction),
    }
}

fn compare_entity_title(
    a: &EntitySummary,
    b: &EntitySummary,
    title_language: Option<&str>,
) -> Ordering {
    let title_a = title_language
        .and_then(|language| a.titles.get(language))
        .unwrap_or(&a.title);
    let title_b = title_language
        .and_then(|language| b.titles.get(language))
        .unwrap_or(&b.title);
    compare_string_for_title_language(title_a, title_b, title_language)
}

fn entity_date_sort_value(entity: &EntitySummary, field: &str) -> Option<String> {
    entity
        .dates
        .iter()
        .find(|item| item.field == field)
        .and_then(|item| crate::dates::date_sort_key(Some(&item.value)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        EntityDateValue, EntityRecord, EntitySummary, EntityTypeConfig, FieldConfig, FieldType,
        KizunaConfig, Library, Relation, RelationDirection,
    };
    use std::collections::{BTreeMap, HashMap};

    fn entity(id: &str, title: &str, release: Option<&str>) -> EntitySummary {
        EntitySummary {
            id: id.to_string(),
            entity_type: "game".to_string(),
            type_label: "Game".to_string(),
            title: title.to_string(),
            titles: BTreeMap::new(),
            dates: release
                .map(|value| EntityDateValue {
                    field: "release".to_string(),
                    value: value.to_string(),
                    parsed: None,
                    sort_key: None,
                })
                .into_iter()
                .collect(),
            image: None,
            summary: None,
            path: format!("Taxonomy/Game/{title}.md"),
            basename: title.to_string(),
            external_refs: BTreeMap::new(),
            tags: Vec::new(),
            episode_progress: None,
            relation_count: 0,
        }
    }

    fn ids(entities: &[EntitySummary]) -> Vec<&str> {
        entities.iter().map(|entity| entity.id.as_str()).collect()
    }

    #[test]
    fn date_sort_keeps_entities_without_a_value_at_the_bottom_in_both_directions() {
        let entities = || {
            vec![
                entity("undated", "Undated", None),
                entity("early", "Early", Some("2020-01-01")),
                entity("late", "Late", Some("2022-01-01")),
            ]
        };

        let asc =
            sort_entities_with_title_language(entities(), "date:release", SortDirection::Asc, None);
        assert_eq!(ids(&asc), ["early", "late", "undated"]);

        // Present values reverse (late before early), but the undated entity must
        // still sink to the bottom rather than flipping to the top.
        let desc = sort_entities_with_title_language(
            entities(),
            "date:release",
            SortDirection::Desc,
            None,
        );
        assert_eq!(ids(&desc), ["late", "early", "undated"]);
    }

    // --- relation-graph aggregation ------------------------------------------

    fn summary(id: &str, entity_type: &str, type_label: &str, title: &str) -> EntitySummary {
        EntitySummary {
            id: id.to_string(),
            entity_type: entity_type.to_string(),
            type_label: type_label.to_string(),
            title: title.to_string(),
            titles: BTreeMap::new(),
            dates: Vec::new(),
            image: None,
            summary: None,
            path: format!("Taxonomy/{title}.md"),
            basename: title.to_string(),
            external_refs: BTreeMap::new(),
            tags: Vec::new(),
            episode_progress: None,
            relation_count: 0,
        }
    }

    fn record(summary: EntitySummary) -> EntityRecord {
        EntityRecord {
            body_links: Vec::new(),
            summary,
            revision: "rev".to_string(),
            frontmatter: serde_json::Map::new(),
            file_modified_unix_nanos: 0,
            episode_dates: Vec::new(),
        }
    }

    fn relation(
        source: &str,
        target_id: Option<&str>,
        target_title: &str,
        target_type: Option<&str>,
        field: &str,
        direction: RelationDirection,
    ) -> Relation {
        Relation {
            source_id: source.to_string(),
            target_id: target_id.map(str::to_string),
            target_title: target_title.to_string(),
            target_type: target_type.map(str::to_string),
            field: field.to_string(),
            direction,
        }
    }

    fn entity_type(id: &str, label: &str, relation_field: bool) -> EntityTypeConfig {
        let mut fields = Vec::new();
        if relation_field {
            fields.push(FieldConfig {
                field: "related".to_string(),
                field_type: FieldType::Relation,
                display_name: None,
                title_language: None,
                title_role: None,
                external_fields: Vec::new(),
                enum_options: Vec::new(),
                total_progress_field: None,
                date_role: None,
                season_language: None,
                external_ref: None,
                external_types: Vec::new(),
                relation_type: None,
            });
        }
        EntityTypeConfig {
            id: id.to_string(),
            label: label.to_string(),
            icon: None,
            path: id.to_string(),
            external_priority: Vec::new(),
            filename: None,
            body_sections: Vec::new(),
            fields,
        }
    }

    // Alpha(anime) -> Beta(anime), Gamma(game) [related + body], and Ghost
    // (unresolved); Beta -> Alpha. One incoming reflection that the outgoing
    // helpers must ignore.
    fn graph() -> Library {
        let config = KizunaConfig {
            vault_root: "/virtual-vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            tags: None,
            types: vec![
                entity_type("anime", "Anime", true),
                entity_type("game", "Game", false),
            ],
        };
        let records = vec![
            record(summary("anime:a", "anime", "Anime", "Alpha")),
            record(summary("anime:b", "anime", "Anime", "Beta")),
            record(summary("game:g", "game", "Game", "Gamma")),
        ];
        let relations = vec![
            relation(
                "anime:a",
                Some("anime:b"),
                "Beta",
                Some("anime"),
                "related",
                RelationDirection::Out,
            ),
            relation(
                "anime:a",
                Some("game:g"),
                "Gamma",
                Some("game"),
                "related",
                RelationDirection::Out,
            ),
            relation(
                "anime:a",
                None,
                "Ghost",
                None,
                "related",
                RelationDirection::Out,
            ),
            relation(
                "anime:b",
                Some("anime:a"),
                "Alpha",
                Some("anime"),
                "related",
                RelationDirection::Out,
            ),
            relation(
                "anime:a",
                Some("game:g"),
                "Gamma",
                Some("game"),
                "body",
                RelationDirection::Out,
            ),
            relation(
                "anime:b",
                Some("anime:a"),
                "Alpha",
                Some("anime"),
                "related",
                RelationDirection::In,
            ),
        ];
        Library::new(config, records, relations, Vec::new(), "gen".to_string())
    }

    #[test]
    fn count_by_sorts_by_count_desc_then_name() {
        let counts = count_by(&["a", "b", "a", "c", "a", "b"], |item| item.to_string());
        assert_eq!(
            counts
                .iter()
                .map(|item| (item.name.as_str(), item.count))
                .collect::<Vec<_>>(),
            [("a", 3), ("b", 2), ("c", 1)]
        );
        // Ties broken by name ascending.
        let tie = count_by(&["y", "x"], |item| item.to_string());
        assert_eq!(
            tie.iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            ["x", "y"]
        );
    }

    #[test]
    fn relation_source_type_label_falls_back_to_daily_note_or_unknown() {
        let entity = summary("anime:a", "anime", "Anime", "Alpha");
        let by_id = HashMap::from([("anime:a", &entity)]);
        let from = |source: &str| relation(source, None, "x", None, "f", RelationDirection::Out);
        assert_eq!(
            relation_source_type_label(&from("anime:a"), &by_id),
            "Anime"
        );
        assert_eq!(
            relation_source_type_label(&from("daily-note:2026-06-16"), &by_id),
            "Daily Note"
        );
        assert_eq!(
            relation_source_type_label(&from("missing:z"), &by_id),
            "Unknown"
        );
    }

    #[test]
    fn type_label_looks_up_label_or_falls_back_to_id() {
        let library = graph();
        assert_eq!(
            type_label(&library, Some(&"anime".to_string())),
            Some("Anime".to_string())
        );
        assert_eq!(
            type_label(&library, Some(&"unknown".to_string())),
            Some("unknown".to_string())
        );
        assert_eq!(type_label(&library, None), None);
    }

    #[test]
    fn outgoing_relations_excludes_incoming_and_filters_by_field() {
        let library = graph();
        assert_eq!(outgoing_relations(&library, None).len(), 5); // 5 Out, the 1 In excluded
        assert_eq!(outgoing_relations(&library, Some("related")).len(), 4);
        assert_eq!(outgoing_relations(&library, Some("body")).len(), 1);
    }

    #[test]
    fn relation_type_pairs_labels_source_and_target_types() {
        let pairs: HashMap<_, _> = relation_type_pairs(&graph())
            .into_iter()
            .map(|count| (count.name, count.count))
            .collect();
        assert_eq!(pairs.get("Anime -> Anime"), Some(&2)); // a->b and b->a
        assert_eq!(pairs.get("Anime -> Game"), Some(&2)); // a->g related + body
        assert_eq!(pairs.get("Anime -> Unresolved"), Some(&1)); // a->Ghost
    }

    // --- remaining sort modes -------------------------------------------------

    #[test]
    fn sort_entities_by_title_respects_direction() {
        let entities = || vec![entity("b", "Beta", None), entity("a", "Alpha", None)];
        let asc = sort_entities_with_title_language(entities(), "title", SortDirection::Asc, None);
        assert_eq!(ids(&asc), ["a", "b"]);
        let desc =
            sort_entities_with_title_language(entities(), "title", SortDirection::Desc, None);
        assert_eq!(ids(&desc), ["b", "a"]);
    }

    #[test]
    fn sort_entities_by_relation_count() {
        let mut low = entity("low", "Low", None);
        let mut high = entity("high", "High", None);
        low.relation_count = 1;
        high.relation_count = 5;
        let sorted = sort_entities_with_title_language(
            vec![low, high],
            "relationCount",
            SortDirection::Desc,
            None,
        );
        assert_eq!(ids(&sorted), ["high", "low"]);
    }

    #[test]
    fn sort_entities_by_title_uses_the_language_specific_title_when_present() {
        let mut a = entity("a", "Z-fallback", None);
        let mut b = entity("b", "A-fallback", None);
        a.titles.insert("ja".to_string(), "Apple".to_string());
        b.titles.insert("ja".to_string(), "Banana".to_string());
        // By the fallback title, b (A) precedes a (Z)...
        let by_fallback = sort_entities_with_title_language(
            vec![a.clone(), b.clone()],
            "title",
            SortDirection::Asc,
            None,
        );
        assert_eq!(ids(&by_fallback), ["b", "a"]);
        // ...but by the `ja` title, a (Apple) precedes b (Banana).
        let by_ja =
            sort_entities_with_title_language(vec![a, b], "title", SortDirection::Asc, Some("ja"));
        assert_eq!(ids(&by_ja), ["a", "b"]);
    }
}
