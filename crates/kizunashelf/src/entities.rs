//! Entity list/detail derivation over a [`Library`] — pure domain computation,
//! no HTTP. The `api::entities` handlers parse the request query, call into here,
//! and serialize; the filtering, sorting, pagination, and relation-graph walks
//! live here so they can be unit-tested over a hand-built library.

use crate::contract::EntityListResponse;
use crate::dates::clamp_number;
use crate::library::compare_string_for_title_language;
use crate::relations::{
    sort_entities, sort_entities_with_title_language, sort_records_by_modified, summary_by_id,
    SortDirection,
};
use crate::types::{
    CanonicalStatus, EntityRecord, EntitySummary, Library, Relation, RelationDirection,
};
use std::collections::{HashMap, HashSet};

/// The normalized inputs to [`build_entity_list`], parsed from the HTTP query by
/// the handler so the builder itself is transport-agnostic.
///
/// Deliberately coarse: this endpoint is the *lookup* surface (autocomplete,
/// relation pickers, status shelves, list membership). Criteria-shaped browsing
/// goes through [`crate::smart_lists`] instead, so the vault has exactly one
/// filter engine rather than a weaker duplicate here.
pub struct EntityListParams<'a> {
    /// `None` or `"all"` lists every type; otherwise restricts to that type id.
    pub entity_type: Option<&'a str>,
    /// Restricts to entities whose resolved status maps to this canonical —
    /// schema-driven (each type's own `statusValues`), so it composes with
    /// `entity_type: None` into cross-type shelves ("everything ongoing").
    /// Entities with no status or an unmapped value never match.
    pub canonical_status: Option<CanonicalStatus>,
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

/// Every distinct tag across the library, sorted. The vocabulary backing tag
/// autocomplete and the Library tag filter; derived from the resident
/// [`EntitySummary::tags`], so no bodies are read.
pub fn all_tags(library: &Library) -> Vec<String> {
    let mut set = std::collections::BTreeSet::new();
    for record in &library.records {
        for tag in &record.summary.tags {
            set.insert(tag.clone());
        }
    }
    set.into_iter().collect()
}

/// Upper bound on a requested page size, shared by the entity list and smart-list
/// results endpoints. Generous because the in-process native clients (no network
/// hop) fetch a whole type/list in one page to keep scroll position across
/// navigation; the web still paginates in small pages of its own choosing. It
/// only caps abusive requests, not normal ones.
pub(crate) const MAX_PAGE_SIZE: i64 = 10_000;

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
    if let Some(canonical) = params.canonical_status {
        entities.retain(|entity| {
            entity
                .summary
                .status
                .as_ref()
                .and_then(|status| status.canonical)
                == Some(canonical)
        });
    }
    // Free-text search both filters and scores: each surviving entity keeps its
    // best relevance tier (by id) so the `relevance` sort can rank exact/prefix
    // matches above incidental substring hits.
    let mut relevance_scores: Option<HashMap<String, u32>> = None;
    if let Some(query) = params
        .query
        .map(|query| query.trim().to_lowercase())
        .filter(|query| !query.is_empty())
    {
        let mut scores = HashMap::new();
        entities.retain(|entity| match entity_match_score(entity, &query) {
            Some(score) => {
                scores.insert(entity.summary.id.clone(), score);
                true
            }
            None => false,
        });
        relevance_scores = Some(scores);
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

    // "recentlyUpdated" sorts on the file mtime, which is resident only on the
    // record (not the serialized summary), so it sorts records before mapping.
    let summaries = if params.sort == "recentlyUpdated" {
        sort_records_by_modified(entities, params.direction, params.title_language)
            .into_iter()
            .map(|entity| entity.summary.clone())
            .collect::<Vec<_>>()
    } else {
        let summaries = entities
            .into_iter()
            .map(|entity| entity.summary.clone())
            .collect::<Vec<_>>();
        match relevance_scores {
            // "relevance" ranks best-match-first (direction is ignored — it's
            // always best-first), tie-broken by the collated title. With no
            // active query it's meaningless, so fall back to the title order.
            Some(scores) if params.sort == "relevance" => {
                sort_entities_by_relevance(summaries, &scores, params.title_language)
            }
            _ if params.sort == "relevance" => sort_entities_for_entity_list(
                summaries,
                "title",
                params.direction,
                params.title_language,
            ),
            _ => sort_entities_for_entity_list(
                summaries,
                params.sort,
                params.direction,
                params.title_language,
            ),
        }
    };

    let page_size = clamp_number(params.page_size, 1, MAX_PAGE_SIZE);
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

/// Relevance score of an entity against a lowercased, non-empty query, or `None`
/// when nothing matches (the entity is filtered out). Higher is more relevant.
/// *Primary* fields — the canonical title, every localized title, and the
/// basename — always dominate *secondary* fields (summary, path), so a title
/// match outranks an entity that merely has the query in its file path.
///
/// Shared with the smart-list pipeline (`smart_lists::results`) so browsing a
/// saved list and browsing the library rank a search identically.
pub(crate) fn entity_match_score(entity: &EntityRecord, query: &str) -> Option<u32> {
    let primary = std::iter::once(entity.summary.title.as_str())
        .chain(entity.summary.titles.values().map(String::as_str))
        .chain(std::iter::once(entity.summary.basename.as_str()))
        .map(|value| match_tier(value, query))
        .max()
        .unwrap_or(0);
    let secondary = [
        entity.summary.summary.as_deref(),
        Some(entity.summary.path.as_str()),
    ]
    .into_iter()
    .flatten()
    .map(|value| match_tier(value, query))
    .max()
    .unwrap_or(0);
    if primary == 0 && secondary == 0 {
        None
    } else {
        // Lexicographic (primary, secondary): the primary tier dominates, with
        // the secondary tier only breaking ties between equal primary matches.
        Some(primary * 10 + secondary)
    }
}

/// How well one field value matches the (already lowercased) query, higher is
/// better: 4 exact, 3 prefix, 2 word-start substring, 1 substring, 0 none.
fn match_tier(value: &str, query: &str) -> u32 {
    let value = value.to_lowercase();
    if value == query {
        4
    } else if value.starts_with(query) {
        3
    } else if is_word_start_match(&value, query) {
        2
    } else if value.contains(query) {
        1
    } else {
        0
    }
}

/// Whether `query` begins a word inside `value` (both already lowercased) — a
/// match that follows a non-alphanumeric boundary. Lets "titan" rank above a
/// mid-word hit for a multi-word title like "attack on titan".
fn is_word_start_match(value: &str, query: &str) -> bool {
    value.match_indices(query).any(|(index, _)| {
        index == 0
            || value[..index]
                .chars()
                .next_back()
                .is_some_and(|character| !character.is_alphanumeric())
    })
}

/// Orders entities best-match-first by their precomputed relevance `scores`,
/// tie-broken by the collated title (in `title_language` when given). Direction
/// is intentionally not honored — relevance is always highest-score-first.
fn sort_entities_by_relevance(
    mut entities: Vec<EntitySummary>,
    scores: &HashMap<String, u32>,
    title_language: Option<&str>,
) -> Vec<EntitySummary> {
    let explicit_title_language = title_language
        .map(str::trim)
        .filter(|language| !language.is_empty() && *language != "default");
    entities.sort_by(|a, b| {
        let score_a = scores.get(&a.id).copied().unwrap_or(0);
        let score_b = scores.get(&b.id).copied().unwrap_or(0);
        score_b.cmp(&score_a).then_with(|| {
            let title_a = entity_sort_title(a, explicit_title_language);
            let title_b = entity_sort_title(b, explicit_title_language);
            compare_string_for_title_language(title_a, title_b, explicit_title_language)
        })
    });
    entities
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
    let summaries = summary_by_id(library);
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
        .map(|relation| orient_for_entity(entity_id, relation, &summaries))
        .collect()
}

/// Relations are stored from their source's perspective. One whose *target* is
/// this entity — an unmirrored inbound link, e.g. a body wikilink written in
/// another entity's note — is rewritten into this entity's perspective: an
/// incoming edge whose target is the *linking* entity, carrying that entity's
/// title and type so clients group it by the linking entity rather than by this
/// one. Relations already sourced from this entity pass through unchanged.
fn orient_for_entity(
    entity_id: &str,
    relation: &Relation,
    summaries: &HashMap<&str, &EntitySummary>,
) -> Relation {
    if relation.source_id == entity_id {
        return relation.clone();
    }
    let source = summaries.get(relation.source_id.as_str());
    Relation {
        source_id: entity_id.to_string(),
        target_id: Some(relation.source_id.clone()),
        target_title: source
            .map(|summary| summary.title.clone())
            .unwrap_or_else(|| relation.source_id.clone()),
        target_type: source.map(|summary| summary.entity_type.clone()),
        field: relation.field.clone(),
        direction: RelationDirection::In,
    }
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
    use crate::types::{EntityTypeConfig, FieldConfig, FieldType, KizunaConfig, ResolvedStatus};
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
            enum_role: None,
            status_values: None,
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
                    field("favorite", FieldType::Bool),
                    field("notes", FieldType::Text),
                    field("franchise", FieldType::Relation),
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
            tags: Vec::new(),
            episode_progress: None,
            status: None,
            relation_count: 0,
        }
    }

    fn record(id: &str, title: &str, frontmatter: serde_json::Value) -> EntityRecord {
        EntityRecord {
            body_links: Vec::new(),
            summary: summary(id, title),
            revision: "rev".to_string(),
            frontmatter: frontmatter.as_object().cloned().unwrap_or_default(),
            file_modified_unix_nanos: 0,
            episode_dates: Vec::new(),
        }
    }

    // --- build_entity_list ----------------------------------------------------

    fn params<'a>() -> EntityListParams<'a> {
        EntityListParams {
            entity_type: None,
            canonical_status: None,
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

    #[test]
    fn build_entity_list_filters_by_canonical_status_across_types() {
        // The user labels differ per record ("Watching" / "在看" could each map
        // to ongoing in their own types); the filter reads only the resolved
        // canonical, so it spans types and label languages.
        let with_status = |id: &str, title: &str, value: &str, canonical| {
            let mut entity = record(id, title, json!({}));
            entity.summary.status = Some(ResolvedStatus {
                field: "status".to_string(),
                value: value.to_string(),
                canonical,
            });
            entity
        };
        let library = Library::new(
            config(),
            vec![
                with_status("anime:a", "Alpha", "在看", Some(CanonicalStatus::Ongoing)),
                with_status("anime:b", "Beta", "想看", Some(CanonicalStatus::Planning)),
                // Unmapped value and no status at all: never match a canonical.
                with_status("anime:c", "Gamma", "重看中", None),
                record("anime:d", "Delta", json!({})),
            ],
            Vec::new(),
            Vec::new(),
            "gen".to_string(),
        );

        let ongoing = build_entity_list(
            &library,
            &EntityListParams {
                canonical_status: Some(CanonicalStatus::Ongoing),
                ..params()
            },
        );
        assert_eq!(ongoing.total, 1);
        assert_eq!(ongoing.items[0].title, "Alpha");

        // Composes with the other filters (here: free-text search).
        let searched = build_entity_list(
            &library,
            &EntityListParams {
                canonical_status: Some(CanonicalStatus::Planning),
                query: Some("beta"),
                ..params()
            },
        );
        assert_eq!(searched.total, 1);
        assert_eq!(searched.items[0].title, "Beta");

        let completed = build_entity_list(
            &library,
            &EntityListParams {
                canonical_status: Some(CanonicalStatus::Completed),
                ..params()
            },
        );
        assert_eq!(completed.total, 0);
    }

    #[test]
    fn match_tier_ranks_exact_over_prefix_over_word_start_over_substring() {
        assert_eq!(match_tier("titan", "titan"), 4); // exact
        assert_eq!(match_tier("titan attack", "titan"), 3); // prefix
        assert_eq!(match_tier("attack on titan", "titan"), 2); // word start
        assert_eq!(match_tier("subtitание", "titan"), 0); // no substring hit
        assert_eq!(match_tier("subtitans", "titan"), 1); // mid-word substring
        assert_eq!(match_tier("TITAN", "titan"), 4); // case-insensitive
        assert_eq!(match_tier("nothing", "titan"), 0);
    }

    #[test]
    fn build_entity_list_relevance_ranks_exact_and_prefix_first() {
        let library = Library::new(
            config(),
            vec![
                // Only the path contains "hero" — must rank last.
                {
                    let mut r = record("anime:p", "Unrelated", json!({}));
                    r.summary.path = "Taxonomy/Anime/hero-notes.md".to_string();
                    r
                },
                record("anime:sub", "Superhero Squad", json!({})), // substring
                record("anime:prefix", "Hero Academia", json!({})), // prefix
                record("anime:exact", "Hero", json!({})),          // exact
                record("anime:word", "My Hero", json!({})),        // word start
            ],
            Vec::new(),
            Vec::new(),
            "gen".to_string(),
        );

        let ranked = build_entity_list(
            &library,
            &EntityListParams {
                query: Some("hero"),
                sort: "relevance",
                ..params()
            },
        );
        assert_eq!(
            ids(&ranked.items),
            [
                "anime:exact",
                "anime:prefix",
                "anime:word",
                "anime:sub",
                "anime:p"
            ],
        );
    }

    #[test]
    fn build_entity_list_relevance_scores_localized_titles_as_primary() {
        // A per-language title exact-matches; a path-only hit on another entity
        // must still rank below it (primary title beats secondary path).
        let mut hit = record("anime:jp", "Shingeki no Kyojin", json!({}));
        hit.summary
            .titles
            .insert("ja".to_string(), "進撃".to_string());
        let mut path_only = record("anime:path", "Other", json!({}));
        path_only.summary.path = "Taxonomy/Anime/進撃-draft.md".to_string();

        let library = Library::new(
            config(),
            vec![path_only, hit],
            Vec::new(),
            Vec::new(),
            "gen".to_string(),
        );
        let ranked = build_entity_list(
            &library,
            &EntityListParams {
                query: Some("進撃"),
                sort: "relevance",
                ..params()
            },
        );
        assert_eq!(ids(&ranked.items), ["anime:jp", "anime:path"]);
    }

    #[test]
    fn build_entity_list_relevance_tie_breaks_by_title() {
        // Two equal-tier prefix matches fall back to collated title order.
        let library = Library::new(
            config(),
            vec![
                record("anime:b", "Hero Zeta", json!({})),
                record("anime:a", "Hero Alpha", json!({})),
            ],
            Vec::new(),
            Vec::new(),
            "gen".to_string(),
        );
        let ranked = build_entity_list(
            &library,
            &EntityListParams {
                query: Some("hero"),
                sort: "relevance",
                ..params()
            },
        );
        assert_eq!(ids(&ranked.items), ["anime:a", "anime:b"]);
    }

    #[test]
    fn build_entity_list_relevance_without_query_falls_back_to_title() {
        let library = Library::new(
            config(),
            vec![
                record("anime:b", "Beta", json!({})),
                record("anime:a", "Alpha", json!({})),
            ],
            Vec::new(),
            Vec::new(),
            "gen".to_string(),
        );
        let ranked = build_entity_list(
            &library,
            &EntityListParams {
                sort: "relevance",
                ..params()
            },
        );
        assert_eq!(ids(&ranked.items), ["anime:a", "anime:b"]);
    }

    #[test]
    fn build_entity_list_sorts_by_recently_updated() {
        let with_mtime = |id: &str, title: &str, mtime: u128| {
            let mut entity = record(id, title, json!({}));
            entity.file_modified_unix_nanos = mtime;
            entity
        };
        let library = Library::new(
            config(),
            vec![
                with_mtime("anime:a", "Alpha", 100),
                with_mtime("anime:b", "Beta", 300),
                with_mtime("anime:c", "Gamma", 0), // unknown mtime → always last
                with_mtime("anime:d", "Delta", 200),
            ],
            Vec::new(),
            Vec::new(),
            "gen".to_string(),
        );

        // Ascending: oldest first, unknown last.
        let asc = build_entity_list(
            &library,
            &EntityListParams {
                sort: "recentlyUpdated",
                ..params()
            },
        );
        assert_eq!(
            ids(&asc.items),
            ["anime:a", "anime:d", "anime:b", "anime:c"]
        );

        // Descending: newest (most recently updated) first, unknown still last.
        let desc = build_entity_list(
            &library,
            &EntityListParams {
                sort: "recentlyUpdated",
                direction: SortDirection::Desc,
                ..params()
            },
        );
        assert_eq!(
            ids(&desc.items),
            ["anime:b", "anime:d", "anime:a", "anime:c"]
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

    #[test]
    fn entity_detail_relations_orients_unmirrored_inbound_body_links_as_incoming() {
        // Beta's note body-links to Alpha; with no frontmatter relation, the core
        // stored only the single Out edge b->a (body links get no In reflection).
        // From Alpha's perspective this must read as an *incoming* link from Beta,
        // carrying Beta's title/type — not an outgoing edge bucketed by Alpha's own
        // type (the bug where it surfaced as "body · <Alpha's type>" → Beta).
        let records = vec![
            record("anime:a", "Alpha", json!({})),
            record("anime:b", "Beta", json!({})),
        ];
        let relations = vec![relation(
            "anime:b",
            "anime:a",
            "body",
            RelationDirection::Out,
        )];
        let library = Library::new(config(), records, relations, Vec::new(), "gen".to_string());

        let oriented = entity_detail_relations(&library, "anime:a");
        assert_eq!(oriented.len(), 1);
        let edge = &oriented[0];
        assert_eq!(edge.source_id, "anime:a");
        assert_eq!(edge.target_id.as_deref(), Some("anime:b"));
        assert_eq!(edge.target_title, "Beta"); // the linking entity's title, not its id
        assert_eq!(edge.target_type.as_deref(), Some("anime"));
        assert_eq!(edge.field, "body");
        assert_eq!(edge.direction, RelationDirection::In);

        // Beta still surfaces as a related entity.
        let related = entity_detail_related_entities(&library, "anime:a", &oriented);
        assert_eq!(
            related.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
            ["Beta"]
        );
    }
}
