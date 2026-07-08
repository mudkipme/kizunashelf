//! The module's test suite — everything exercises the public surface, so the
//! tests double as a usage map of the seams between the submodules.

use crate::relations::SortDirection;
use crate::types::{EntityRecord, Library};
use chrono::NaiveDate;
use serde_yaml::Value;

use super::*;
use crate::types::{
    EntitySummary, EntityTypeConfig, FieldConfig, FieldType, KizunaConfig, Relation,
    RelationDirection,
};
use serde_json::json;
use std::collections::BTreeMap;

// --- fixtures --------------------------------------------------------------

fn field(name: &str, field_type: FieldType) -> FieldConfig {
    FieldConfig {
        field: name.to_string(),
        field_type,
        display_name: None,
        title_language: None,
        title_role: None,
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

fn config() -> KizunaConfig {
    KizunaConfig {
        vault_root: "/virtual-vault".to_string(),
        taxonomy_root: "Media".to_string(),
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
                field("status", FieldType::Enum),
                field("genres", FieldType::EnumList),
                field("rating", FieldType::Rating),
                field("started", FieldType::Date),
                field("cover", FieldType::Image),
                field("studio", FieldType::Relation),
            ],
        }],
    }
}

fn record(id: &str, title: &str, frontmatter: serde_json::Value) -> EntityRecord {
    EntityRecord {
        body_links: Vec::new(),
        summary: EntitySummary {
            id: id.to_string(),
            entity_type: "anime".to_string(),
            type_label: "Anime".to_string(),
            title: title.to_string(),
            titles: BTreeMap::new(),
            dates: Vec::new(),
            image: None,
            summary: None,
            path: format!("Media/Anime/{title}.md"),
            basename: title.to_string(),
            external_refs: BTreeMap::new(),
            tags: Vec::new(),
            episode_progress: None,
            status: None,
            relation_count: 0,
        },
        revision: "rev".to_string(),
        frontmatter: frontmatter.as_object().cloned().unwrap_or_default(),
        file_modified_unix_nanos: 0,
        episode_dates: Vec::new(),
    }
}

fn library(records: Vec<EntityRecord>, relations: Vec<Relation>) -> Library {
    Library::new(config(), records, relations, Vec::new(), "gen".to_string())
}

fn fixed_ctx(library: &Library) -> EvalContext<'_> {
    let today = NaiveDate::from_ymd_opt(2026, 7, 8).unwrap();
    let now = today.and_hms_opt(12, 0, 0).unwrap().and_utc();
    EvalContext::new(library, now, today)
}

fn atom(expression: &str) -> FilterNode {
    let (kind, negated) =
        parse_expression(expression).unwrap_or_else(|| panic!("unsupported: {expression}"));
    FilterNode::Expr(FilterAtom {
        kind,
        negated,
        raw: expression.to_string(),
    })
}

fn eval_expr(expression: &str, record: &EntityRecord, ctx: &EvalContext) -> Option<bool> {
    eval_node(&atom(expression), record, ctx)
}

// --- expression parsing ----------------------------------------------------

#[test]
fn parses_the_supported_expression_profile() {
    for expression in [
        r#"status == "watching""#,
        r#"note.status != "done""#,
        r#"note["画像"].isEmpty()"#,
        r#"rating >= 7.5"#,
        r#"favorite == true"#,
        r#"genres.containsAny("comedy", "drama")"#,
        r#"genres.containsAll("comedy", "drama")"#,
        r#"title.contains("hero")"#,
        r#"title.startsWith("My")"#,
        r#"title.endsWith("!")"#,
        r#"!note.summary.isEmpty()"#,
        r#"file.hasTag("favorites")"#,
        r#"file.inFolder("Media/Anime")"#,
        r#"file.hasLink("Kyoto Animation")"#,
        r#"file.name.contains("2026")"#,
        r#"note.started >= date("2026-01-01")"#,
        r#"note.started >= today() - "90d""#,
        r#"file.mtime > now() - "1w""#,
        r#"note.started < today() + "1M 4h""#,
    ] {
        assert!(
            parse_expression(expression).is_some(),
            "should parse: {expression}"
        );
    }
}

#[test]
fn unsupported_expressions_are_rejected_not_guessed() {
    for expression in [
        r#"formula.ppu > 5"#,
        r#"price * 2 > 10"#,
        r#"a == b"#,                      // field-to-field comparison
        r#"status == "a" && rating > 5"#, // boolean operators
        r#"file.size > 100"#,             // unsupported file property
        r#"if(price, price)"#,
        r#"note.title.lower() == "x""#,
        r#"status"#, // bare truthiness
        r#"note.started >= today() - "banana""#,
    ] {
        assert!(
            parse_expression(expression).is_none(),
            "should reject: {expression}"
        );
    }
}

#[test]
fn negated_comparison_flips_the_operator() {
    let (kind, negated) = parse_expression(r#"!(rating > 5)"#).unwrap();
    assert!(!negated);
    assert_eq!(
        kind,
        AtomKind::Compare {
            field: FieldRef::Note("rating".to_string()),
            op: CompareOp::Lte,
            value: CompareValue::Number(5.0),
        }
    );
}

#[test]
fn printed_atoms_reparse_to_the_same_atom() {
    let atoms = [
        FilterAtom::new(
            AtomKind::Compare {
                field: FieldRef::Note("status".to_string()),
                op: CompareOp::Eq,
                value: CompareValue::String("watching".to_string()),
            },
            false,
        ),
        FilterAtom::new(
            AtomKind::Compare {
                field: FieldRef::Note("started".to_string()),
                op: CompareOp::Gte,
                value: CompareValue::Date(DateExpr {
                    base: DateBase::Today,
                    offsets: vec![DateOffset {
                        negative: true,
                        duration: DurationSpec {
                            days: 90,
                            ..Default::default()
                        },
                    }],
                }),
            },
            false,
        ),
        FilterAtom::new(
            AtomKind::Contains {
                field: FieldRef::Note("genres".to_string()),
                mode: ContainsMode::Any,
                values: vec!["comedy".to_string(), "slice-of-life".to_string()],
            },
            false,
        ),
        FilterAtom::new(
            AtomKind::IsEmpty {
                field: FieldRef::Note("画像".to_string()),
            },
            true,
        ),
        FilterAtom::new(
            AtomKind::HasLink {
                target: "He said \"hi\"".to_string(),
            },
            false,
        ),
        FilterAtom::new(
            AtomKind::InFolder {
                folder: "Media/Anime".to_string(),
            },
            false,
        ),
    ];
    for original in atoms {
        let (kind, negated) = parse_expression(&original.raw)
            .unwrap_or_else(|| panic!("printer emitted unparseable: {}", original.raw));
        assert_eq!(kind, original.kind, "round-trip of {}", original.raw);
        assert_eq!(negated, original.negated, "round-trip of {}", original.raw);
    }
}

#[test]
fn duration_literals_parse_case_sensitively() {
    assert_eq!(
        parse_duration("1M"),
        Some(DurationSpec {
            months: 1,
            ..Default::default()
        })
    );
    assert_eq!(
        parse_duration("30m"),
        Some(DurationSpec {
            minutes: 30,
            ..Default::default()
        })
    );
    assert_eq!(
        parse_duration("1M 4h"),
        Some(DurationSpec {
            months: 1,
            hours: 4,
            ..Default::default()
        })
    );
    assert_eq!(
        parse_duration("2 weeks"),
        Some(DurationSpec {
            weeks: 2,
            ..Default::default()
        })
    );
    assert_eq!(parse_duration("banana"), None);
    assert_eq!(parse_duration(""), None);
    assert_eq!(parse_duration("1x"), None);
}

// --- evaluation --------------------------------------------------------------

#[test]
fn evaluates_value_driven_comparisons() {
    let entity = record(
        "anime:a",
        "Alpha",
        json!({
            "status": "watching",
            "rating": 8,
            "favorite": true,
            "genres": ["comedy", "drama"],
            "started": "2026-06-01",
            "summary": "",
        }),
    );
    let library = library(vec![entity], Vec::new());
    let entity = &library.records[0];
    let ctx = fixed_ctx(&library);

    for (expression, expected) in [
        (r#"status == "watching""#, true),
        (r#"status != "watching""#, false),
        (r#"rating >= 7"#, true),
        (r#"rating < 7"#, false),
        (r#"favorite == true"#, true),
        (r#"genres.containsAny("comedy", "horror")"#, true),
        (r#"genres.containsAll("comedy", "horror")"#, false),
        (r#"genres.contains("drama")"#, true),
        (r#"status.contains("watch")"#, true), // substring on a string value
        (r#"status.startsWith("watch")"#, true),
        (r#"status.endsWith("watch")"#, false),
        (r#"summary.isEmpty()"#, true), // empty string is empty
        (r#"!summary.isEmpty()"#, false),
        (r#"genres.isEmpty()"#, false),
        (r#"missing.isEmpty()"#, true),
        // Missing fields: only != is definitely true (Bases null semantics).
        (r#"missing == "x""#, false),
        (r#"missing != "x""#, true),
        (r#"missing > 3"#, false),
        // Date comparisons, absolute and relative to the injected today.
        (r#"note.started >= date("2026-01-01")"#, true),
        (r#"note.started < date("2026-01-01")"#, false),
        (r#"note.started >= today() - "90d""#, true),
        (r#"note.started >= today() - "7d""#, false),
        // File scope + name.
        (r#"file.inFolder("Media/Anime")"#, true),
        (r#"file.inFolder("Media/Books")"#, false),
        (r#"file.name.contains("Alph")"#, true),
    ] {
        assert_eq!(
            eval_expr(expression, entity, &ctx),
            Some(expected),
            "{expression}"
        );
    }
}

#[test]
fn evaluates_fuzzy_dates_by_their_sort_key() {
    let entity = record("anime:a", "Alpha", json!({"started": "2024 Spring"}));
    let library = library(vec![entity], Vec::new());
    let ctx = fixed_ctx(&library);
    assert_eq!(
        eval_expr(
            r#"note.started >= date("2024-01-01")"#,
            &library.records[0],
            &ctx
        ),
        Some(true)
    );
}

#[test]
fn evaluates_mtime_against_relative_instants() {
    let mut entity = record("anime:a", "Alpha", json!({}));
    // 2026-07-05 00:00 UTC — three days before the fixed "now".
    let mtime = NaiveDate::from_ymd_opt(2026, 7, 5)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_nanos_opt()
        .unwrap() as u128;
    entity.file_modified_unix_nanos = mtime;
    let unknown = record("anime:b", "Beta", json!({}));
    let library = library(vec![entity, unknown], Vec::new());
    let ctx = fixed_ctx(&library);

    assert_eq!(
        eval_expr(r#"file.mtime > now() - "1w""#, &library.records[0], &ctx),
        Some(true)
    );
    assert_eq!(
        eval_expr(r#"file.mtime > now() - "1d""#, &library.records[0], &ctx),
        Some(false)
    );
    // Unknown mtime (0) never matches a comparison.
    assert_eq!(
        eval_expr(r#"file.mtime > now() - "1w""#, &library.records[1], &ctx),
        Some(false)
    );
}

#[test]
fn has_tag_matches_exact_and_nested() {
    let mut entity = record("anime:a", "Alpha", json!({}));
    entity.summary.tags = vec!["favorites".to_string(), "mood/cozy".to_string()];
    let library = library(vec![entity], Vec::new());
    let ctx = fixed_ctx(&library);
    for (expression, expected) in [
        (r#"file.hasTag("favorites")"#, true),
        (r#"file.hasTag("mood")"#, true), // nested tag under mood/
        (r#"file.hasTag("cozy")"#, false),
        (r#"file.hasTag("nope", "favorites")"#, true), // any of
    ] {
        assert_eq!(
            eval_expr(expression, &library.records[0], &ctx),
            Some(expected),
            "{expression}"
        );
    }
}

#[test]
fn has_link_matches_through_the_relation_graph() {
    let source = record("anime:a", "Alpha", json!({}));
    let target = record("anime:kyoani", "Kyoto Animation", json!({}));
    let relations = vec![Relation {
        source_id: "anime:a".to_string(),
        target_id: Some("anime:kyoani".to_string()),
        target_title: "Kyoto Animation".to_string(),
        target_type: Some("anime".to_string()),
        field: "studio".to_string(),
        direction: RelationDirection::Out,
    }];
    let library = library(vec![source, target], relations);
    let ctx = fixed_ctx(&library);
    assert_eq!(
        eval_expr(
            r#"file.hasLink("Kyoto Animation")"#,
            &library.records[0],
            &ctx
        ),
        Some(true)
    );
    assert_eq!(
        eval_expr(r#"file.hasLink("Someone Else")"#, &library.records[0], &ctx),
        Some(false)
    );
    // The target itself has no outgoing link.
    assert_eq!(
        eval_expr(
            r#"file.hasLink("Kyoto Animation")"#,
            &library.records[1],
            &ctx
        ),
        Some(false)
    );
}

#[test]
fn tags_field_reads_the_resident_tag_list() {
    let mut entity = record("anime:a", "Alpha", json!({}));
    entity.summary.tags = vec!["cozy".to_string()];
    let library = library(vec![entity], Vec::new());
    let ctx = fixed_ctx(&library);
    assert_eq!(
        eval_expr(r#"tags.contains("cozy")"#, &library.records[0], &ctx),
        Some(true)
    );
    assert_eq!(
        eval_expr(r#"tags.isEmpty()"#, &library.records[0], &ctx),
        Some(false)
    );
}

// --- tri-state groups ---------------------------------------------------------

#[test]
fn unsupported_atoms_are_neutral_in_every_conjunction() {
    let entity = record("anime:a", "Alpha", json!({"status": "watching"}));
    let library = library(vec![entity], Vec::new());
    let entity = &library.records[0];
    let ctx = fixed_ctx(&library);
    let opaque = FilterNode::Opaque(Value::String("formula.ppu > 5".to_string()));

    // and: unknown children are ignored; known children decide.
    let and_true = FilterNode::Group {
        conjunction: Conjunction::All,
        children: vec![atom(r#"status == "watching""#), opaque.clone()],
    };
    assert_eq!(eval_node(&and_true, entity, &ctx), Some(true));

    // or: an unknown child must NOT swallow the vault — known false wins.
    let or_false = FilterNode::Group {
        conjunction: Conjunction::Any,
        children: vec![atom(r#"status == "done""#), opaque.clone()],
    };
    assert_eq!(eval_node(&or_false, entity, &ctx), Some(false));

    // A fully-unknown group propagates unknown; top-level unknown includes.
    let unknown = FilterNode::Group {
        conjunction: Conjunction::Any,
        children: vec![opaque.clone()],
    };
    assert_eq!(eval_node(&unknown, entity, &ctx), None);
    assert!(record_matches(&unknown, entity, &ctx));

    // not: none of the known children may match.
    let none_of = FilterNode::Group {
        conjunction: Conjunction::NoneOf,
        children: vec![atom(r#"status == "done""#), opaque],
    };
    assert_eq!(eval_node(&none_of, entity, &ctx), Some(true));

    // The empty group is "no constraints".
    assert_eq!(eval_node(&FilterNode::empty(), entity, &ctx), Some(true));
}

// --- document parse / round-trip ---------------------------------------------

const HAND_EDITED: &str = r#"filters:
  and:
    - file.inFolder("Media/Anime")
    - status == "watching"
    - formula.ppu > 5
formulas:
  ppu: "(price / age).toFixed(2)"
views:
  - type: table
    name: List
    order:
      - file.name
      - status
    sort:
      - property: note.started
        direction: DESC
    limit: 25
  - type: cards
    name: Grid
    image: note.cover
    cardSize: 220
  - type: map
    name: Places
"#;

#[test]
fn parses_a_hand_edited_document_leniently() {
    let list = parse_smart_list(HAND_EDITED).unwrap();

    // Two supported atoms plus one opaque, all inside the and-group.
    let FilterNode::Group {
        conjunction: Conjunction::All,
        children,
    } = &list.filters
    else {
        panic!("expected group")
    };
    assert_eq!(children.len(), 3);
    assert!(matches!(&children[2], FilterNode::Opaque(_)));

    // The scope idiom is recognized from the inFolder atom.
    let scope = scope_from_filters(&list.filters, &config());
    assert_eq!(scope, Some(("anime".to_string(), 0)));

    // table + cards views; map skipped with a warning.
    assert_eq!(list.views.len(), 2);
    assert_eq!(list.views[0].layout, ViewLayout::List);
    assert_eq!(list.views[0].limit, Some(25));
    assert_eq!(list.views[0].sort.len(), 1);
    assert_eq!(list.views[0].sort[0].direction, SortDirection::Desc);
    assert_eq!(list.views[1].layout, ViewLayout::Grid);
    assert_eq!(list.views[1].image.as_deref(), Some("note.cover"));
    assert_eq!(list.views[1].source_index, 1);

    assert!(list.warnings.iter().any(|w| w.contains("formula.ppu")));
    assert!(list.warnings.iter().any(|w| w.contains("Places")));
}

#[test]
fn structural_edits_preserve_everything_we_do_not_own() {
    let list = parse_smart_list(HAND_EDITED).unwrap();
    let mut doc = list.doc.clone();

    // Edit: change the status criterion, keep scope, drop nothing else.
    let filters = FilterNode::Group {
        conjunction: Conjunction::All,
        children: vec![
            atom(r#"file.inFolder("Media/Anime")"#),
            FilterNode::Expr(FilterAtom::new(
                AtomKind::Compare {
                    field: FieldRef::Note("status".to_string()),
                    op: CompareOp::Eq,
                    value: CompareValue::String("done".to_string()),
                },
                false,
            )),
            FilterNode::Opaque(Value::String("formula.ppu > 5".to_string())),
        ],
    };
    set_global_filters(&mut doc, &filters);
    apply_views(
        &mut doc,
        &[
            ViewSpec {
                layout: ViewLayout::List,
                name: "List".to_string(),
                filters: None,
                sort: vec![ViewSort {
                    property: SortProperty::Note("rating".to_string()),
                    direction: SortDirection::Desc,
                }],
                limit: Some(10),
                image: None,
            },
            ViewSpec {
                layout: ViewLayout::Grid,
                name: "Grid".to_string(),
                filters: None,
                sort: Vec::new(),
                limit: None,
                image: Some("note.cover".to_string()),
            },
        ],
    );

    let rendered = render_smart_list(&doc);
    let reparsed = parse_smart_list(&rendered).unwrap();

    // The hand-written formula block survives.
    assert!(rendered.contains("ppu:"));
    // The opaque filter expression survives verbatim.
    assert!(rendered.contains("formula.ppu > 5"));
    // The unsupported map view survives, after the supported views.
    assert!(rendered.contains("type: map"));
    assert_eq!(reparsed.views.len(), 2);
    // The table view kept its unknown `order` key and its identity.
    assert!(rendered.contains("- file.name"));
    // The cards view kept its unknown cardSize key.
    assert!(rendered.contains("cardSize: 220"));
    // The edits took: new sort + limit on the table view.
    assert_eq!(
        reparsed.views[0].sort[0].property,
        SortProperty::Note("rating".to_string())
    );
    assert_eq!(reparsed.views[0].limit, Some(10));
    // Idempotence: rendering the reparsed doc yields the same text.
    assert_eq!(render_smart_list(&reparsed.doc), rendered);
}

#[test]
fn default_document_is_valid_and_scoped() {
    let doc = default_smart_list_doc(Some("Media/Anime"), Some("note.cover"));
    let rendered = render_smart_list(&doc);
    let list = parse_smart_list(&rendered).unwrap();
    assert!(list.warnings.is_empty(), "warnings: {:?}", list.warnings);
    assert_eq!(
        scope_from_filters(&list.filters, &config()),
        Some(("anime".to_string(), 0))
    );
    assert_eq!(list.views.len(), 2);
    assert_eq!(list.views[0].layout, ViewLayout::List);
    assert_eq!(list.views[1].layout, ViewLayout::Grid);
    assert_eq!(list.views[1].image.as_deref(), Some("note.cover"));

    // Unscoped, imageless: no filters key at all, still two views.
    let bare = render_smart_list(&default_smart_list_doc(None, None));
    let bare = parse_smart_list(&bare).unwrap();
    assert!(bare.filters.is_empty_group());
    assert_eq!(bare.views.len(), 2);
}

#[test]
fn rejects_invalid_yaml_and_non_mappings() {
    assert!(parse_smart_list("views: [").is_err());
    assert!(parse_smart_list("- just\n- a list\n").is_err());
    // Empty files are a valid, empty smart list.
    let empty = parse_smart_list("").unwrap();
    assert!(empty.filters.is_empty_group());
    assert!(empty.views.is_empty());
}

// --- results: filter → sort → limit --------------------------------------------

#[test]
fn smart_list_records_filters_sorts_and_limits() {
    let raw = r#"filters:
  and:
    - file.inFolder("Media/Anime")
    - status == "watching"
views:
  - type: table
    name: List
    sort:
      - property: note.rating
        direction: DESC
    limit: 2
"#;
    let list = parse_smart_list(raw).unwrap();
    let library = library(
        vec![
            record(
                "anime:a",
                "Alpha",
                json!({"status": "watching", "rating": 6}),
            ),
            record(
                "anime:b",
                "Beta",
                json!({"status": "watching", "rating": 9}),
            ),
            record("anime:c", "Gamma", json!({"status": "done", "rating": 10})),
            record("anime:d", "Delta", json!({"status": "watching"})), // no rating → last
            record("anime:e", "Eps", json!({"status": "watching", "rating": 7})),
        ],
        Vec::new(),
    );
    let ctx = fixed_ctx(&library);
    let records = smart_list_records(&list, list.views.first(), &ctx, None);
    let ids: Vec<&str> = records.iter().map(|r| r.summary.id.as_str()).collect();
    // Top 2 by rating desc among "watching": Beta (9), Eps (7).
    assert_eq!(ids, ["anime:b", "anime:e"]);

    // Without the view: unsorted-by-rating default (title asc), no limit.
    let all = smart_list_records(&list, None, &ctx, None);
    let ids: Vec<&str> = all.iter().map(|r| r.summary.id.as_str()).collect();
    assert_eq!(ids, ["anime:a", "anime:b", "anime:d", "anime:e"]);
}

#[test]
fn view_filters_stack_on_global_filters() {
    let raw = r#"filters:
  and:
    - file.inFolder("Media/Anime")
views:
  - type: table
    name: Favorites
    filters:
      and:
        - rating >= 8
"#;
    let list = parse_smart_list(raw).unwrap();
    let library = library(
        vec![
            record("anime:a", "Alpha", json!({"rating": 9})),
            record("anime:b", "Beta", json!({"rating": 5})),
        ],
        Vec::new(),
    );
    let ctx = fixed_ctx(&library);
    let records = smart_list_records(&list, list.views.first(), &ctx, None);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].summary.id, "anime:a");
}

#[test]
fn date_fields_sort_by_their_normalized_key() {
    let raw = r#"views:
  - type: table
    name: List
    sort:
      - property: note.started
        direction: ASC
"#;
    let list = parse_smart_list(raw).unwrap();
    let library = library(
        vec![
            record("anime:b", "Beta", json!({"started": "2024 Spring"})),
            record("anime:a", "Alpha", json!({"started": "2023-11-01"})),
            record("anime:c", "Gamma", json!({})), // absent → last
        ],
        Vec::new(),
    );
    let ctx = fixed_ctx(&library);
    let records = smart_list_records(&list, list.views.first(), &ctx, None);
    let ids: Vec<&str> = records.iter().map(|r| r.summary.id.as_str()).collect();
    assert_eq!(ids, ["anime:a", "anime:b", "anime:c"]);
}

#[test]
fn type_scope_folder_joins_taxonomy_and_type_path() {
    let config = config();
    assert_eq!(
        type_scope_folder(&config, "anime").as_deref(),
        Some("Media/Anime")
    );
    assert_eq!(type_scope_folder(&config, "missing"), None);
}

/// Verbatim view config as serialized by real Obsidian (≥1.9): sort
/// entries are `{property, direction}` with UPPERCASE directions and
/// *bare* note-field references (`date`, not `note.date`), while the cards
/// image uses the prefixed `note.cover_url`. Both reference forms must
/// parse to the same properties our writer emits.
#[test]
fn parses_obsidian_native_view_serialization() {
    let raw = r#"views:
  - type: table
    name: Table
    sort:
      - property: file.name
        direction: ASC
      - property: date
        direction: ASC
  - type: cards
    name: Cards
    image: note.cover_url
"#;
    let list = parse_smart_list(raw).unwrap();
    assert!(list.warnings.is_empty(), "warnings: {:?}", list.warnings);
    assert_eq!(
        list.views[0].sort,
        vec![
            ViewSort {
                property: SortProperty::FileName,
                direction: SortDirection::Asc,
            },
            ViewSort {
                property: SortProperty::Note("date".to_string()),
                direction: SortDirection::Asc,
            },
        ]
    );
    assert_eq!(list.views[1].image.as_deref(), Some("note.cover_url"));
    // The bare and prefixed forms name the same property.
    assert_eq!(
        parse_sort_property("date"),
        parse_sort_property("note.date")
    );
}
