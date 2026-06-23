//! Entity list/detail derivation over a [`Library`] — pure domain computation,
//! no HTTP. The `api::entities` handlers parse the request query, call into here,
//! and serialize; the filtering, sorting, pagination, and relation-graph walks
//! live here so they can be unit-tested over a hand-built library.

use crate::contract::EntityListResponse;
use crate::dates::clamp_number;
use crate::library::compare_string_for_title_language;
use crate::relations::{
    sort_entities, sort_entities_with_title_language, summary_by_id, SortDirection,
};
use crate::types::{EntityRecord, EntitySummary, FieldType, Library, Relation, RelationDirection};
use serde::Deserialize;
use std::collections::HashSet;

/// One parsed `filters` entry: a field and the set of values that match it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityFieldFilter {
    field: String,
    values: Vec<String>,
}

/// The normalized inputs to [`build_entity_list`], parsed from the HTTP query by
/// the handler so the builder itself is transport-agnostic. Borrows from the
/// request where it can; `field_filters` is owned because it's parsed from JSON.
pub struct EntityListParams<'a> {
    /// `None` or `"all"` lists every type; otherwise restricts to that type id.
    pub entity_type: Option<&'a str>,
    /// `"with"`/`"without"` filters on having any external ref; else ignored.
    pub refs: Option<&'a str>,
    /// `"with"`/`"without"` filters on having a cover image; else ignored.
    pub cover: Option<&'a str>,
    pub field_filters: Vec<EntityFieldFilter>,
    /// Free-text search across titles/summary/basename/path (case-insensitive).
    pub query: Option<&'a str>,
    /// Restricts to entities that link to this target (title or id).
    pub relation: Option<&'a str>,
    pub sort: &'a str,
    pub direction: SortDirection,
    pub title_language: Option<&'a str>,
    pub page: f64,
    pub page_size: f64,
}

/// Filters, sorts, and paginates the library's entities into an
/// [`EntityListResponse`] per the parsed `params`.
pub fn build_entity_list(library: &Library, params: &EntityListParams) -> EntityListResponse {
    let mut entities: Vec<&EntityRecord> = library.records.iter().collect();
    if let Some(entity_type) = params
        .entity_type
        .filter(|entity_type| *entity_type != "all")
    {
        entities.retain(|entity| entity.summary.entity_type == entity_type);
    }
    match params.refs {
        Some("with") => entities.retain(|entity| !entity.summary.external_refs.is_empty()),
        Some("without") => entities.retain(|entity| entity.summary.external_refs.is_empty()),
        _ => {}
    }
    match params.cover {
        Some("with") => entities.retain(|entity| entity.summary.image.is_some()),
        Some("without") => entities.retain(|entity| entity.summary.image.is_none()),
        _ => {}
    }
    if !params.field_filters.is_empty() {
        entities
            .retain(|entity| entity_matches_field_filters(entity, library, &params.field_filters));
    }
    if let Some(query) = params
        .query
        .map(|query| query.trim().to_lowercase())
        .filter(|query| !query.is_empty())
    {
        entities.retain(|entity| {
            [
                Some(entity.summary.title.as_str()),
                entity.summary.summary.as_deref(),
                Some(entity.summary.basename.as_str()),
                Some(entity.summary.path.as_str()),
            ]
            .into_iter()
            .flatten()
            .chain(entity.summary.titles.values().map(|value| value.as_str()))
            .any(|value| value.to_lowercase().contains(&query))
        });
    }
    if let Some(relation) = params
        .relation
        .map(str::trim)
        .filter(|relation| !relation.is_empty())
    {
        let ids: HashSet<_> = library
            .relations
            .iter()
            .filter(|item| {
                item.target_title == relation || item.target_id.as_deref() == Some(relation)
            })
            .map(|item| item.source_id.clone())
            .collect();
        entities.retain(|entity| ids.contains(&entity.summary.id));
    }

    let summaries = entities
        .into_iter()
        .map(|entity| entity.summary.clone())
        .collect::<Vec<_>>();
    let summaries = sort_entities_for_entity_list(
        summaries,
        params.sort,
        params.direction,
        params.title_language,
    );

    let page_size = clamp_number(params.page_size, 1, 100);
    let requested_page = clamp_number(params.page, 1, i64::MAX);
    let total = summaries.len();
    let total_pages = std::cmp::max(1, ((total as f64) / (page_size as f64)).ceil() as i64);
    let page = requested_page.min(total_pages);
    let start = ((page - 1) * page_size) as usize;
    let items = summaries
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .collect();

    EntityListResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    }
}

/// Parses the `filters` query value (a JSON array) into normalized
/// [`EntityFieldFilter`]s, trimming and dropping entries with no field or no
/// values. Returns `Err` with a human-readable message on malformed JSON; the
/// handler maps that to a `400`.
pub fn parse_entity_field_filters(filters: Option<&str>) -> Result<Vec<EntityFieldFilter>, String> {
    let Some(filters) = filters.map(str::trim).filter(|filters| !filters.is_empty()) else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<EntityFieldFilter>>(filters)
        .map(|filters| {
            filters
                .into_iter()
                .filter_map(|filter| {
                    let field = filter.field.trim().to_string();
                    let values = filter
                        .values
                        .into_iter()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .collect::<Vec<_>>();
                    (!field.is_empty() && !values.is_empty())
                        .then_some(EntityFieldFilter { field, values })
                })
                .collect()
        })
        .map_err(|_| "Invalid entity filters".to_string())
}

fn entity_matches_field_filters(
    entity: &EntityRecord,
    library: &Library,
    filters: &[EntityFieldFilter],
) -> bool {
    filters.iter().all(|filter| {
        let Some(field_type) = field_type_for_entity_filter(entity, library, &filter.field) else {
            return false;
        };
        let Some(value) = entity.frontmatter.get(&filter.field) else {
            return false;
        };
        field_value_matches_filter(value, field_type, &filter.values)
    })
}

fn field_type_for_entity_filter(
    entity: &EntityRecord,
    library: &Library,
    field: &str,
) -> Option<FieldType> {
    library
        .config
        .types
        .iter()
        .find(|type_config| type_config.id == entity.summary.entity_type)
        .and_then(|type_config| {
            type_config
                .fields
                .iter()
                .find(|field_config| field_config.field == field)
        })
        .map(|field_config| field_config.field_type)
        .filter(|field_type| {
            matches!(
                field_type,
                FieldType::Enum | FieldType::EnumList | FieldType::Bool
            )
        })
}

fn field_value_matches_filter(
    value: &serde_json::Value,
    field_type: FieldType,
    expected: &[String],
) -> bool {
    match field_type {
        FieldType::Enum => frontmatter_scalar_matches_any(value, expected),
        FieldType::EnumList => match value {
            serde_json::Value::Array(items) => items
                .iter()
                .any(|item| frontmatter_scalar_matches_any(item, expected)),
            _ => false,
        },
        FieldType::Bool => match value {
            serde_json::Value::Bool(value) => {
                let value = if *value { "true" } else { "false" };
                expected.iter().any(|item| item == value)
            }
            _ => false,
        },
        _ => false,
    }
}

fn frontmatter_scalar_matches_any(value: &serde_json::Value, expected: &[String]) -> bool {
    match value {
        serde_json::Value::String(value) => expected.iter().any(|item| item == value),
        serde_json::Value::Bool(value) => {
            let value = if *value { "true" } else { "false" };
            expected.iter().any(|item| item == value)
        }
        serde_json::Value::Number(value) => {
            let value = value.to_string();
            expected.iter().any(|item| item == &value)
        }
        _ => false,
    }
}

pub fn sort_entities_for_entity_list(
    mut entities: Vec<EntitySummary>,
    sort: &str,
    direction: SortDirection,
    title_language: Option<&str>,
) -> Vec<EntitySummary> {
    let explicit_title_language = title_language
        .map(str::trim)
        .filter(|language| !language.is_empty() && *language != "default");
    if sort != "title" {
        return sort_entities_with_title_language(
            entities,
            sort,
            direction,
            explicit_title_language,
        );
    }

    let multiplier = if direction == SortDirection::Asc {
        1
    } else {
        -1
    };

    entities.sort_by(|a, b| {
        let title_a = entity_sort_title(a, explicit_title_language);
        let title_b = entity_sort_title(b, explicit_title_language);
        let ordering = compare_string_for_title_language(title_a, title_b, explicit_title_language);
        if multiplier == 1 {
            ordering
        } else {
            ordering.reverse()
        }
    });
    entities
}

fn entity_sort_title<'entity>(
    entity: &'entity EntitySummary,
    explicit_title_language: Option<&str>,
) -> &'entity str {
    if let Some(language) = explicit_title_language {
        return entity
            .titles
            .get(language)
            .unwrap_or(&entity.title)
            .as_str();
    }

    entity.title.as_str()
}

pub fn entity_detail_relations(library: &Library, entity_id: &str) -> Vec<Relation> {
    // Only the relations that touch this entity (source or resolved target),
    // visited in stored order so the result matches a full-graph scan.
    library
        .relation_indices_touching(entity_id)
        .into_iter()
        .map(|index| &library.relations[index])
        .filter(|relation| {
            relation.field != "daily-note"
                && !relation.source_id.starts_with("daily-note:")
                && (relation.source_id == entity_id
                    || (relation.target_id.as_deref() == Some(entity_id)
                        && relation.direction == RelationDirection::Out
                        && !has_mirrored_incoming_relation(library, entity_id, relation)))
        })
        .cloned()
        .collect()
}

fn has_mirrored_incoming_relation(library: &Library, entity_id: &str, relation: &Relation) -> bool {
    library.relations_from(entity_id).any(|candidate| {
        candidate.target_id.as_deref() == Some(relation.source_id.as_str())
            && candidate.field == relation.field
            && candidate.direction == RelationDirection::In
    })
}

pub fn entity_detail_related_entities(
    library: &Library,
    entity_id: &str,
    relations: &[Relation],
) -> Vec<EntitySummary> {
    let summary_by_id = summary_by_id(library);
    let mut seen = HashSet::new();
    let mut related = Vec::new();
    for relation in relations {
        let related_id = if relation.source_id == entity_id {
            relation.target_id.as_deref()
        } else if relation.target_id.as_deref() == Some(entity_id) {
            Some(relation.source_id.as_str())
        } else {
            None
        };
        let Some(related_id) = related_id else {
            continue;
        };
        if seen.insert(related_id.to_string()) {
            if let Some(summary) = summary_by_id.get(related_id) {
                related.push((*summary).clone());
            }
        }
    }
    sort_entities(related, "title", SortDirection::Asc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EntityTypeConfig, FieldConfig, KizunaConfig};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn field(name: &str, field_type: FieldType) -> FieldConfig {
        FieldConfig {
            field: name.to_string(),
            field_type,
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
        }
    }

    fn config() -> KizunaConfig {
        KizunaConfig {
            vault_root: "/virtual-vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            types: vec![EntityTypeConfig {
                id: "anime".to_string(),
                label: "Anime".to_string(),
                icon: None,
                path: "Anime".to_string(),
                external_priority: Vec::new(),
                filename: None,
                body_mappings: Vec::new(),
                fields: vec![
                    field("status", FieldType::Enum),
                    field("genres", FieldType::EnumList),
                    field("favorite", FieldType::Bool),
                    field("notes", FieldType::Text),
                ],
            }],
        }
    }

    fn summary(id: &str, title: &str) -> EntitySummary {
        EntitySummary {
            id: id.to_string(),
            entity_type: "anime".to_string(),
            type_label: "Anime".to_string(),
            title: title.to_string(),
            titles: BTreeMap::new(),
            dates: Vec::new(),
            image: None,
            summary: None,
            path: format!("Taxonomy/Anime/{title}.md"),
            basename: title.to_string(),
            external_refs: BTreeMap::new(),
            relation_count: 0,
        }
    }

    fn record(id: &str, title: &str, frontmatter: serde_json::Value) -> EntityRecord {
        EntityRecord {
            body_links: Vec::new(),
            summary: summary(id, title),
            revision: "rev".to_string(),
            frontmatter: frontmatter.as_object().cloned().unwrap_or_default(),
        }
    }

    fn filter(field: &str, values: &[&str]) -> EntityFieldFilter {
        EntityFieldFilter {
            field: field.to_string(),
            values: values.iter().map(|value| value.to_string()).collect(),
        }
    }

    // --- parse_entity_field_filters ------------------------------------------

    // `EntityFieldFilter` doesn't derive `Debug`, so unwrap the Result by hand.
    fn parsed(input: Option<&str>) -> Vec<EntityFieldFilter> {
        match parse_entity_field_filters(input) {
            Ok(filters) => filters,
            Err(error) => panic!("expected Ok, got: {error}"),
        }
    }

    #[test]
    fn parse_entity_field_filters_empty_input_is_no_filters() {
        assert!(parsed(None).is_empty());
        assert!(parsed(Some("   ")).is_empty());
    }

    #[test]
    fn parse_entity_field_filters_parses_trims_and_drops_empties() {
        let filters = parsed(Some(r#"[{"field":" status ","values":[" Watching ",""]}]"#));
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].field, "status");
        assert_eq!(filters[0].values, vec!["Watching".to_string()]);
    }

    #[test]
    fn parse_entity_field_filters_drops_filters_with_no_field_or_no_values() {
        assert!(parsed(Some(
            r#"[{"field":"","values":["x"]},{"field":"f","values":[]}]"#
        ))
        .is_empty());
    }

    #[test]
    fn parse_entity_field_filters_rejects_invalid_json() {
        let result = parse_entity_field_filters(Some("not json"));
        assert_eq!(result.err().as_deref(), Some("Invalid entity filters"));
    }

    // --- value matching -------------------------------------------------------

    #[test]
    fn field_value_matches_filter_by_field_type() {
        // Enum: scalar string membership.
        assert!(field_value_matches_filter(
            &json!("Watching"),
            FieldType::Enum,
            &["Watching".to_string()]
        ));
        assert!(!field_value_matches_filter(
            &json!("Watching"),
            FieldType::Enum,
            &["Completed".to_string()]
        ));
        // EnumList: any array member matches.
        assert!(field_value_matches_filter(
            &json!(["SF", "Space"]),
            FieldType::EnumList,
            &["Space".to_string()]
        ));
        assert!(!field_value_matches_filter(
            &json!("SF"),
            FieldType::EnumList,
            &["SF".to_string()]
        )); // not an array
            // Bool: only a real boolean, rendered as "true"/"false".
        assert!(field_value_matches_filter(
            &json!(true),
            FieldType::Bool,
            &["true".to_string()]
        ));
        assert!(!field_value_matches_filter(
            &json!(true),
            FieldType::Bool,
            &["false".to_string()]
        ));
        assert!(!field_value_matches_filter(
            &json!("true"),
            FieldType::Bool,
            &["true".to_string()]
        )); // string, not bool
            // Non-filterable field types never match.
        assert!(!field_value_matches_filter(
            &json!("x"),
            FieldType::Text,
            &["x".to_string()]
        ));
    }

    #[test]
    fn frontmatter_scalar_matches_any_covers_string_bool_number() {
        assert!(frontmatter_scalar_matches_any(
            &json!("a"),
            &["a".to_string()]
        ));
        assert!(frontmatter_scalar_matches_any(
            &json!(false),
            &["false".to_string()]
        ));
        assert!(frontmatter_scalar_matches_any(
            &json!(5),
            &["5".to_string()]
        ));
        assert!(!frontmatter_scalar_matches_any(
            &json!(["a"]),
            &["a".to_string()]
        )); // arrays don't match here
    }

    // --- schema-driven field eligibility + matching --------------------------

    #[test]
    fn field_type_for_entity_filter_only_returns_filterable_types() {
        let entity = record("anime:a", "Alpha", json!({}));
        let library = Library::new(
            config(),
            vec![record("anime:a", "Alpha", json!({}))],
            Vec::new(),
            Vec::new(),
            "gen".to_string(),
        );
        assert_eq!(
            field_type_for_entity_filter(&entity, &library, "status"),
            Some(FieldType::Enum)
        );
        assert_eq!(
            field_type_for_entity_filter(&entity, &library, "genres"),
            Some(FieldType::EnumList)
        );
        assert_eq!(
            field_type_for_entity_filter(&entity, &library, "favorite"),
            Some(FieldType::Bool)
        );
        assert_eq!(
            field_type_for_entity_filter(&entity, &library, "notes"),
            None
        ); // Text isn't filterable
        assert_eq!(
            field_type_for_entity_filter(&entity, &library, "missing"),
            None
        );
    }

    #[test]
    fn entity_matches_field_filters_requires_all_filters() {
        let entity = record(
            "anime:a",
            "Alpha",
            json!({"status": "Watching", "genres": ["SF", "Space"], "favorite": true, "notes": "blah"}),
        );
        let library = Library::new(
            config(),
            vec![record("anime:a", "Alpha", json!({}))],
            Vec::new(),
            Vec::new(),
            "gen".to_string(),
        );

        assert!(entity_matches_field_filters(
            &entity,
            &library,
            &[filter("status", &["Watching"])]
        ));
        assert!(!entity_matches_field_filters(
            &entity,
            &library,
            &[filter("status", &["Completed"])]
        ));
        assert!(entity_matches_field_filters(
            &entity,
            &library,
            &[filter("genres", &["Space"])]
        ));
        assert!(entity_matches_field_filters(
            &entity,
            &library,
            &[filter("favorite", &["true"])]
        ));
        // All filters must hold (AND).
        assert!(entity_matches_field_filters(
            &entity,
            &library,
            &[
                filter("status", &["Watching"]),
                filter("favorite", &["true"])
            ]
        ));
        assert!(!entity_matches_field_filters(
            &entity,
            &library,
            &[
                filter("status", &["Watching"]),
                filter("favorite", &["false"])
            ]
        ));
        // A non-filterable or unknown field makes the entity fail the filter.
        assert!(!entity_matches_field_filters(
            &entity,
            &library,
            &[filter("notes", &["blah"])]
        ));
        assert!(!entity_matches_field_filters(
            &entity,
            &library,
            &[filter("missing", &["x"])]
        ));
    }

    // --- build_entity_list ----------------------------------------------------

    fn params<'a>() -> EntityListParams<'a> {
        EntityListParams {
            entity_type: None,
            refs: None,
            cover: None,
            field_filters: Vec::new(),
            query: None,
            relation: None,
            sort: "title",
            direction: SortDirection::Asc,
            title_language: None,
            page: 1.0,
            page_size: 40.0,
        }
    }

    #[test]
    fn build_entity_list_searches_and_paginates() {
        let library = Library::new(
            config(),
            vec![
                record("anime:a", "Alpha", json!({})),
                record("anime:b", "Beta", json!({})),
                record("anime:c", "Gamma", json!({})),
            ],
            Vec::new(),
            Vec::new(),
            "gen".to_string(),
        );

        // Free-text search narrows to matching titles.
        let searched = build_entity_list(
            &library,
            &EntityListParams {
                query: Some("eta"),
                ..params()
            },
        );
        assert_eq!(searched.total, 1);
        assert_eq!(searched.items[0].title, "Beta");

        // Pagination: page 2 of size 2 over 3 title-sorted entities yields the last.
        let paged = build_entity_list(
            &library,
            &EntityListParams {
                page: 2.0,
                page_size: 2.0,
                ..params()
            },
        );
        assert_eq!(paged.total, 3);
        assert_eq!(paged.total_pages, 2);
        assert_eq!(paged.page, 2);
        assert_eq!(
            paged
                .items
                .iter()
                .map(|e| e.title.as_str())
                .collect::<Vec<_>>(),
            ["Gamma"]
        );
    }

    // --- sort_entities_for_entity_list ---------------------------------------

    fn titled(id: &str, title: &str, ja: Option<&str>) -> EntitySummary {
        let mut entity = summary(id, title);
        if let Some(ja) = ja {
            entity.titles.insert("ja".to_string(), ja.to_string());
        }
        entity
    }

    fn ids(entities: &[EntitySummary]) -> Vec<&str> {
        entities.iter().map(|entity| entity.id.as_str()).collect()
    }

    #[test]
    fn sort_entities_for_entity_list_sorts_titles_by_direction() {
        let entities = || vec![titled("b", "Beta", None), titled("a", "Alpha", None)];
        let asc = sort_entities_for_entity_list(entities(), "title", SortDirection::Asc, None);
        assert_eq!(ids(&asc), ["a", "b"]);
        let desc = sort_entities_for_entity_list(entities(), "title", SortDirection::Desc, None);
        assert_eq!(ids(&desc), ["b", "a"]);
    }

    #[test]
    fn sort_entities_for_entity_list_treats_default_and_blank_language_as_none() {
        // "default"/"" must fall back to entity.title, not a per-language title.
        let entities = || {
            vec![
                titled("a", "Zeta", Some("Apple")),
                titled("b", "Alpha", Some("Banana")),
            ]
        };
        for language in [None, Some(""), Some("default")] {
            let sorted =
                sort_entities_for_entity_list(entities(), "title", SortDirection::Asc, language);
            assert_eq!(
                ids(&sorted),
                ["b", "a"],
                "language {language:?} should use the fallback title"
            );
        }
    }

    #[test]
    fn sort_entities_for_entity_list_uses_a_language_specific_title() {
        let entities = vec![
            titled("a", "Zeta", Some("Apple")),
            titled("b", "Alpha", Some("Banana")),
        ];
        // By `ja` title, Apple(a) precedes Banana(b), reversing the fallback order.
        let sorted =
            sort_entities_for_entity_list(entities, "title", SortDirection::Asc, Some("ja"));
        assert_eq!(ids(&sorted), ["a", "b"]);
    }

    // --- entity_detail relation filtering ------------------------------------

    fn relation(source: &str, target: &str, field: &str, direction: RelationDirection) -> Relation {
        Relation {
            source_id: source.to_string(),
            target_id: Some(target.to_string()),
            target_title: target.to_string(),
            target_type: Some("anime".to_string()),
            field: field.to_string(),
            direction,
        }
    }

    fn detail_library() -> Library {
        let records = vec![
            record("anime:a", "Alpha", json!({})),
            record("anime:b", "Beta", json!({})),
            record("anime:c", "Gamma", json!({})),
        ];
        let relations = vec![
            // a relates to b (and b carries the In reflection).
            relation("anime:a", "anime:b", "related", RelationDirection::Out),
            relation("anime:b", "anime:a", "related", RelationDirection::In),
            // c relates to a (a carries the In reflection).
            relation("anime:c", "anime:a", "related", RelationDirection::Out),
            relation("anime:a", "anime:c", "related", RelationDirection::In),
            // A daily note links a — must be excluded from the detail relations.
            relation(
                "daily-note:2026-06-16",
                "anime:a",
                "daily-note",
                RelationDirection::Out,
            ),
        ];
        Library::new(config(), records, relations, Vec::new(), "gen".to_string())
    }

    #[test]
    fn entity_detail_relations_dedupes_mirrors_and_excludes_daily_notes() {
        let library = detail_library();
        let relations = entity_detail_relations(&library, "anime:a");

        // a's own outgoing edge (a->b) plus the In reflection of c->a (a->c In);
        // the raw c->a Out is suppressed as a mirror, b->a In is dropped (target,
        // not Out), and the daily-note edge is excluded.
        assert_eq!(relations.len(), 2);
        let mut targets: Vec<_> = relations
            .iter()
            .filter_map(|item| item.target_id.clone())
            .collect();
        targets.sort();
        assert_eq!(targets, vec!["anime:b".to_string(), "anime:c".to_string()]);
        assert!(relations.iter().all(|item| item.field == "related"));
    }

    #[test]
    fn entity_detail_related_entities_are_deduped_and_title_sorted() {
        let library = detail_library();
        let relations = entity_detail_relations(&library, "anime:a");
        let related = entity_detail_related_entities(&library, "anime:a", &relations);
        assert_eq!(
            related
                .iter()
                .map(|entity| entity.title.as_str())
                .collect::<Vec<_>>(),
            ["Beta", "Gamma"]
        );
    }
}
