//! Analytics and cleanup-queue derivation over a [`Library`] — pure domain
//! computation, no HTTP. These are whole-library scans; the `api::analytics`
//! handlers extract request state, memoize, single-flight, and serialize, while
//! the scans themselves live here so they can be unit-tested over a hand-built
//! library without any transport.

use crate::contract::{
    AnalyticsActivity, AnalyticsActivityType, AnalyticsActivityYear, AnalyticsActivityYearType,
    AnalyticsDataQuality, AnalyticsDistributions, AnalyticsRelations, AnalyticsResponse,
    AnalyticsTotals, AnalyticsUnresolvedRelations, CleanupQueueSummary, CleanupQueuesResponse,
    CleanupUnresolvedRelation, StatsResponse, TypeCount,
};
use crate::dates::{parse_entity_date, ParsedEntityDate};
use crate::library::compare_string;
use crate::relations::{
    build_relation_field_summary_with_index, build_relation_hubs, count_by, outgoing_relations,
    relation_fields, relation_type_pairs, summary_by_id, Count,
};
use crate::types::{DateRole, EntitySummary, FieldType, Library};
use crate::vfs::Vfs;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

/// Builds the `/stats` summary for the whole library, or for a single entity type
/// when `entity_type` is `Some(id)` (`None` or `"all"` covers every type).
pub fn build_stats(library: &Library, entity_type: Option<&str>) -> StatsResponse {
    let type_filter = entity_type.filter(|value| *value != "all");
    let summaries: Vec<EntitySummary> = match type_filter {
        Some(entity_type) => library
            .summaries()
            .filter(|entity| entity.entity_type == entity_type)
            .cloned()
            .collect(),
        None => library.summaries().cloned().collect(),
    };
    let ids: HashSet<_> = summaries.iter().map(|entity| entity.id.clone()).collect();
    let mut top_relations = summaries.clone();
    top_relations.sort_by_key(|item| Reverse(item.relation_count));
    top_relations.truncate(12);

    StatsResponse {
        generated_at: library.generated_at.clone(),
        total: summaries.len(),
        relations: library
            .relations
            .iter()
            .filter(|relation| ids.contains(&relation.source_id))
            .count(),
        by_type: library
            .config
            .types
            .iter()
            .map(|entity_type| TypeCount {
                id: entity_type.id.clone(),
                label: entity_type.label.clone(),
                icon: entity_type.icon.clone(),
                count: library
                    .records
                    .iter()
                    .filter(|entity| entity.summary.entity_type == entity_type.id)
                    .count(),
            })
            .collect(),
        date_fields: type_filter
            .and_then(|entity_type| {
                library
                    .config
                    .types
                    .iter()
                    .find(|item| item.id == entity_type)
            })
            .map(|entity_type| {
                entity_type
                    .fields
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
            .unwrap_or_default(),
        top_relations,
    }
}

/// The full `/analytics` whole-library scan: totals, distributions, the activity
/// heatmap, relation hubs, and data-quality buckets.
pub fn build_analytics(library: &Library) -> AnalyticsResponse {
    // Materialize once for the multiple passes below; freed when the (memoized)
    // build returns, unlike a resident duplicate.
    let summaries: Vec<EntitySummary> = library.summaries().cloned().collect();
    let summaries = &summaries;
    let quality = QualityEligibility::new(library);
    let outgoing = outgoing_relations(library, None);
    let unresolved: Vec<_> = outgoing
        .iter()
        .filter(|relation| relation.target_id.is_none())
        .map(|relation| (*relation).to_owned())
        .collect();
    let dated: Vec<_> = summaries
        .iter()
        .flat_map(|entity| {
            entity
                .dates
                .iter()
                .filter_map(|item| {
                    parse_entity_date(Some(&item.value)).map(|date| (entity.clone(), date))
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let dated_entity_ids: std::collections::HashSet<_> =
        dated.iter().map(|(entity, _)| entity.id.clone()).collect();
    let connected_count = summaries
        .iter()
        .filter(|entity| quality.requires_relations(entity))
        .filter(|entity| entity.relation_count > 0)
        .count();

    let entity_by_id = summary_by_id(library);
    AnalyticsResponse {
        generated_at: library.generated_at.clone(),
        totals: AnalyticsTotals {
            entities: summaries.len(),
            relations: outgoing.len(),
            unresolved_relations: unresolved.len(),
            dated_entities: dated_entity_ids.len(),
            connected_entities: connected_count,
        },
        distributions: AnalyticsDistributions {
            by_type: library
                .config
                .types
                .iter()
                .map(|entity_type| TypeCount {
                    id: entity_type.id.clone(),
                    label: entity_type.label.clone(),
                    icon: entity_type.icon.clone(),
                    count: summaries
                        .iter()
                        .filter(|entity| entity.entity_type == entity_type.id)
                        .count(),
                })
                .collect(),
            by_relation_field: count_by(&outgoing, |relation| relation.field.clone())
                .into_iter()
                .take(16)
                .collect::<Vec<Count>>(),
            by_source_target_type: relation_type_pairs(library).into_iter().take(16).collect(),
        },
        activity: build_activity(dated),
        relations: AnalyticsRelations {
            top_fields: relation_fields(library)
                .iter()
                .map(|field| build_relation_field_summary_with_index(library, &entity_by_id, field))
                .filter(|field| field.edge_count > 0)
                .take(12)
                .collect(),
            top_targets: build_relation_hubs(library).into_iter().take(12).collect(),
            unresolved: AnalyticsUnresolvedRelations {
                count: unresolved.len(),
                examples: unresolved.into_iter().take(12).collect(),
            },
        },
        data_quality: AnalyticsDataQuality {
            missing_cover: summaries
                .iter()
                .filter(|entity| quality.requires_cover(entity))
                .filter(|entity| entity.image.is_none())
                .take(12)
                .cloned()
                .collect(),
            missing_external_refs: summaries
                .iter()
                .filter(|entity| quality.requires_external_refs(entity))
                .filter(|entity| entity.external_refs.is_empty())
                .take(12)
                .cloned()
                .collect(),
            isolated: summaries
                .iter()
                .filter(|entity| quality.requires_relations(entity))
                .filter(|entity| entity.relation_count == 0)
                .take(12)
                .cloned()
                .collect(),
        },
    }
}

/// The `/cleanup-queues` response: the per-queue summaries plus the full lists.
/// `broken_assets`/`broken_total` are passed in because finding them needs the
/// VFS (see [`broken_local_assets`]); everything else is pure over the library.
pub fn build_cleanup_queues(
    library: &Library,
    broken_assets: Vec<EntitySummary>,
    broken_total: usize,
) -> CleanupQueuesResponse {
    let summaries: Vec<EntitySummary> = library.summaries().cloned().collect();
    let summaries = &summaries;
    let quality = QualityEligibility::new(library);
    let source_by_id = summary_by_id(library);
    let outgoing = outgoing_relations(library, None);
    let unresolved_relations: Vec<_> = outgoing
        .iter()
        .filter(|relation| relation.target_id.is_none())
        .filter_map(|relation| {
            source_by_id
                .get(relation.source_id.as_str())
                .map(|source| CleanupUnresolvedRelation {
                    source: (*source).clone(),
                    relation: (*relation).to_owned(),
                })
        })
        .collect();
    let missing_cover: Vec<_> = summaries
        .iter()
        .filter(|entity| quality.requires_cover(entity))
        .filter(|entity| entity.image.is_none())
        .cloned()
        .collect();
    let cover_total = summaries
        .iter()
        .filter(|entity| quality.requires_cover(entity))
        .count();
    let missing_external_refs: Vec<_> = summaries
        .iter()
        .filter(|entity| quality.requires_external_refs(entity))
        .filter(|entity| entity.external_refs.is_empty())
        .cloned()
        .collect();
    let refs_total = summaries
        .iter()
        .filter(|entity| quality.requires_external_refs(entity))
        .count();
    let isolated: Vec<_> = summaries
        .iter()
        .filter(|entity| quality.requires_relations(entity))
        .filter(|entity| entity.relation_count == 0)
        .cloned()
        .collect();
    let relations_total = summaries
        .iter()
        .filter(|entity| quality.requires_relations(entity))
        .count();
    let queues = cleanup_queue_summaries(&[
        (
            "missing-cover",
            "Missing Cover",
            missing_cover.len(),
            cover_total,
        ),
        (
            "missing-refs",
            "Missing External Refs",
            missing_external_refs.len(),
            refs_total,
        ),
        (
            "isolated",
            "Isolated Nodes",
            isolated.len(),
            relations_total,
        ),
        (
            "broken-asset",
            "Broken Assets",
            broken_assets.len(),
            broken_total,
        ),
        (
            "unresolved-relations",
            "Unresolved Relations",
            unresolved_relations.len(),
            outgoing.len(),
        ),
    ]);

    CleanupQueuesResponse {
        generated_at: library.generated_at.clone(),
        queues,
        missing_cover,
        missing_external_refs,
        isolated,
        broken_assets,
        unresolved_relations,
    }
}

/// Entities whose local cover path points at a file that no longer exists, plus
/// the number of entities that reference a local cover (the denominator). Needs
/// the VFS to check file existence, so it's `async` and lives apart from the pure
/// [`build_cleanup_queues`].
pub async fn broken_local_assets(library: &Library, vfs: &dyn Vfs) -> (Vec<EntitySummary>, usize) {
    let mut broken = Vec::new();
    let mut local_total = 0;
    for summary in library.summaries() {
        let Some(image) = summary.image.as_deref() else {
            continue;
        };
        let image = image.trim();
        if image.is_empty() || is_remote_or_data_url(image) {
            continue;
        }
        local_total += 1;
        if !vfs.exists(image).await.unwrap_or(false) {
            broken.push(summary.clone());
        }
    }
    (broken, local_total)
}

fn is_remote_or_data_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("data:")
        || lower.starts_with("blob:")
}

struct QualityEligibility {
    cover_types: HashSet<String>,
    external_ref_types: HashSet<String>,
    relation_types: HashSet<String>,
}

impl QualityEligibility {
    fn new(library: &Library) -> Self {
        let mut cover_types = HashSet::new();
        let mut external_ref_types = HashSet::new();
        let mut relation_types = HashSet::new();
        for type_config in &library.config.types {
            for field in &type_config.fields {
                match field.field_type {
                    FieldType::Image | FieldType::ImageList => {
                        cover_types.insert(type_config.id.clone());
                    }
                    FieldType::ExternalRef => {
                        external_ref_types.insert(type_config.id.clone());
                    }
                    FieldType::Relation => {
                        relation_types.insert(type_config.id.clone());
                    }
                    _ => {}
                }
            }
        }
        Self {
            cover_types,
            external_ref_types,
            relation_types,
        }
    }

    fn requires_cover(&self, entity: &EntitySummary) -> bool {
        self.cover_types.contains(&entity.entity_type)
    }

    fn requires_external_refs(&self, entity: &EntitySummary) -> bool {
        self.external_ref_types.contains(&entity.entity_type)
    }

    fn requires_relations(&self, entity: &EntitySummary) -> bool {
        self.relation_types.contains(&entity.entity_type)
    }
}

/// Builds cleanup-queue summaries from `(id, label, remaining, total)` rows,
/// skipping any whose total is zero.
fn cleanup_queue_summaries(entries: &[(&str, &str, usize, usize)]) -> Vec<CleanupQueueSummary> {
    let mut queues = Vec::new();
    for &(id, label, remaining, total) in entries {
        push_cleanup_queue(&mut queues, id, label, remaining, total);
    }
    queues
}

fn push_cleanup_queue(
    queues: &mut Vec<CleanupQueueSummary>,
    id: &str,
    label: &str,
    remaining: usize,
    total: usize,
) {
    if total > 0 {
        queues.push(cleanup_queue_summary(id, label, remaining, total));
    }
}

fn cleanup_queue_summary(
    id: &str,
    label: &str,
    remaining: usize,
    total: usize,
) -> CleanupQueueSummary {
    CleanupQueueSummary {
        id: id.to_string(),
        label: label.to_string(),
        remaining,
        total,
    }
}

/// Buckets dated entities into a year × month matrix (Jan..Dec), with a per-type
/// breakdown so the client can filter by type. Each parseable date is one
/// occurrence; year-only dates count toward the year total but no month bucket.
fn build_activity(dated: Vec<(EntitySummary, ParsedEntityDate)>) -> AnalyticsActivity {
    struct TypeAcc {
        total: usize,
        months: [u32; 12],
    }
    struct YearAcc {
        total: usize,
        months: [u32; 12],
        by_type: HashMap<String, TypeAcc>,
    }

    let total_dated = dated
        .iter()
        .map(|(entity, _)| entity.id.as_str())
        .collect::<HashSet<_>>()
        .len();

    let mut years: HashMap<i32, YearAcc> = HashMap::new();
    let mut type_totals: HashMap<String, (String, usize)> = HashMap::new();
    for (entity, date) in &dated {
        let year = years.entry(date.year).or_insert_with(|| YearAcc {
            total: 0,
            months: [0; 12],
            by_type: HashMap::new(),
        });
        year.total += 1;
        let type_acc = year
            .by_type
            .entry(entity.entity_type.clone())
            .or_insert_with(|| TypeAcc {
                total: 0,
                months: [0; 12],
            });
        type_acc.total += 1;
        if let Some(index) = date.month.and_then(month_index) {
            year.months[index] += 1;
            type_acc.months[index] += 1;
        }
        type_totals
            .entry(entity.entity_type.clone())
            .or_insert_with(|| (entity.type_label.clone(), 0))
            .1 += 1;
    }

    let mut year_rows: Vec<_> = years.into_iter().collect();
    year_rows.sort_by_key(|(year, _)| Reverse(*year));
    let years = year_rows
        .into_iter()
        .map(|(year, acc)| {
            let mut by_type = acc
                .by_type
                .into_iter()
                .map(|(type_id, type_acc)| AnalyticsActivityYearType {
                    type_id,
                    total: type_acc.total,
                    months: type_acc.months.to_vec(),
                })
                .collect::<Vec<_>>();
            by_type.sort_by(|a, b| {
                b.total
                    .cmp(&a.total)
                    .then_with(|| compare_string(&a.type_id, &b.type_id))
            });
            AnalyticsActivityYear {
                year,
                total: acc.total,
                months: acc.months.to_vec(),
                by_type,
            }
        })
        .collect::<Vec<_>>();

    let mut types = type_totals
        .into_iter()
        .map(|(id, (label, total))| AnalyticsActivityType { id, label, total })
        .collect::<Vec<_>>();
    types.sort_by(|a, b| {
        b.total
            .cmp(&a.total)
            .then_with(|| compare_string(&a.label, &b.label))
    });

    AnalyticsActivity {
        total_dated,
        types,
        years,
    }
}

/// Zero-based month bucket (0 = January) for a 1..=12 month value.
fn month_index(month: u32) -> Option<usize> {
    // `then` (lazy), not `then_some` (eager): `month - 1` must not be evaluated
    // for an out-of-range month like 0, which would underflow `u32` and panic.
    (1..=12).contains(&month).then(|| (month - 1) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::read_library;
    use crate::types::{EntityTypeConfig, FieldConfig, KizunaConfig};
    use crate::vfs::InMemoryVfs;
    use std::sync::Arc;

    fn field(name: &str, field_type: FieldType, date_role: Option<DateRole>) -> FieldConfig {
        FieldConfig {
            field: name.to_string(),
            field_type,
            display_name: None,
            title_language: None,
            title_role: None,
            external_fields: Vec::new(),
            enum_options: Vec::new(),
            total_progress_field: None,
            date_role,
            season_language: None,
            external_ref: None,
            external_types: Vec::new(),
            relation_type: None,
        }
    }

    fn entity_type(
        id: &str,
        label: &str,
        path: &str,
        fields: Vec<FieldConfig>,
    ) -> EntityTypeConfig {
        EntityTypeConfig {
            id: id.to_string(),
            label: label.to_string(),
            icon: None,
            path: path.to_string(),
            external_priority: Vec::new(),
            filename: None,
            body_mappings: Vec::new(),
            fields,
        }
    }

    // Two types: `anime` declares cover/externalRef/relation/date fields (so its
    // entities are eligible for the cover/refs/relations quality queues and feed
    // the activity heatmap); `note` declares none (so its entities never count
    // toward those denominators — the schema decides eligibility, not the data).
    fn config() -> KizunaConfig {
        KizunaConfig {
            vault_root: "/virtual-vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            types: vec![
                entity_type(
                    "anime",
                    "Anime",
                    "Anime",
                    vec![
                        field("title", FieldType::Title, None),
                        field("cover", FieldType::Image, None),
                        field("bangumi", FieldType::ExternalRef, None),
                        field("related", FieldType::Relation, None),
                        field("aired", FieldType::Date, Some(DateRole::Completed)),
                    ],
                ),
                entity_type(
                    "note",
                    "Note",
                    "Note",
                    vec![field("title", FieldType::Title, None)],
                ),
            ],
        }
    }

    // A is fully populated and connected; B is missing cover/refs but linked from A
    // (so not isolated); C is missing everything and isolated; N is a note (not
    // eligible for any quality queue).
    async fn fixture() -> (Library, Arc<InMemoryVfs>) {
        let vfs = Arc::new(InMemoryVfs::new());
        vfs.insert_dir("Taxonomy/Anime");
        vfs.insert_file(
            "Taxonomy/Anime/A.md",
            "---\ntitle: A\ncover: Assets/Taxonomy/Anime/A/cover.jpg\nbangumi: \"123\"\nrelated: \"[[B]]\"\naired: 2023-05-01\n---\n",
        );
        vfs.insert_file("Taxonomy/Anime/B.md", "---\ntitle: B\n---\n");
        vfs.insert_file("Taxonomy/Anime/C.md", "---\ntitle: C\n---\n");
        vfs.insert_dir("Taxonomy/Note");
        vfs.insert_file("Taxonomy/Note/N.md", "---\ntitle: N\n---\n");
        let library = read_library(config(), vfs.clone()).await.unwrap();
        (library, vfs)
    }

    fn titles(items: &[EntitySummary]) -> Vec<&str> {
        items.iter().map(|item| item.title.as_str()).collect()
    }

    #[test]
    fn month_index_buckets_valid_months_only() {
        assert_eq!(month_index(1), Some(0));
        assert_eq!(month_index(12), Some(11));
        assert_eq!(month_index(0), None);
        assert_eq!(month_index(13), None);
    }

    #[test]
    fn cleanup_queue_summaries_skips_zero_total_queues() {
        let queues =
            cleanup_queue_summaries(&[("a", "A", 2, 5), ("b", "B", 0, 0), ("c", "C", 1, 3)]);
        assert_eq!(queues.len(), 2);
        assert_eq!(queues[0].id, "a");
        assert_eq!(queues[0].remaining, 2);
        assert_eq!(queues[0].total, 5);
        assert_eq!(queues[1].id, "c");
    }

    #[test]
    fn is_remote_or_data_url_detects_remote_and_inline() {
        assert!(is_remote_or_data_url("https://example.com/a.jpg"));
        assert!(is_remote_or_data_url("HTTP://example.com/a.jpg"));
        assert!(is_remote_or_data_url("data:image/png;base64,AAAA"));
        assert!(is_remote_or_data_url("blob:abc"));
        assert!(!is_remote_or_data_url("Assets/Anime/A/cover.jpg"));
    }

    #[tokio::test]
    async fn build_analytics_totals_and_distributions() {
        let (library, _vfs) = fixture().await;
        let analytics = build_analytics(&library);

        assert_eq!(analytics.totals.entities, 4);
        assert_eq!(analytics.totals.relations, 1); // A -> B (Out)
        assert_eq!(analytics.totals.unresolved_relations, 0);
        assert_eq!(analytics.totals.dated_entities, 1); // only A has a parseable date
        assert_eq!(analytics.totals.connected_entities, 2); // A and B (anime, relation_count > 0)

        let by_type: HashMap<_, _> = analytics
            .distributions
            .by_type
            .iter()
            .map(|item| (item.id.as_str(), item.count))
            .collect();
        assert_eq!(by_type["anime"], 3);
        assert_eq!(by_type["note"], 1);
    }

    #[tokio::test]
    async fn build_analytics_data_quality_respects_schema_eligibility() {
        let (library, _vfs) = fixture().await;
        let analytics = build_analytics(&library);

        // Eligible (anime) entities with no cover/refs; the note N is never eligible.
        assert_eq!(titles(&analytics.data_quality.missing_cover), ["B", "C"]);
        assert_eq!(
            titles(&analytics.data_quality.missing_external_refs),
            ["B", "C"]
        );
        // Only the entity with relation_count 0 is isolated (B is linked from A).
        assert_eq!(titles(&analytics.data_quality.isolated), ["C"]);
    }

    #[tokio::test]
    async fn build_analytics_activity_buckets_by_year_and_month() {
        let (library, _vfs) = fixture().await;
        let analytics = build_analytics(&library);

        assert_eq!(analytics.activity.total_dated, 1);
        assert_eq!(analytics.activity.years.len(), 1);
        let year = &analytics.activity.years[0];
        assert_eq!(year.year, 2023);
        assert_eq!(year.total, 1);
        assert_eq!(year.months[4], 1); // May (0-based)
        assert_eq!(year.months.iter().sum::<u32>(), 1);
        assert_eq!(analytics.activity.types.len(), 1);
        assert_eq!(analytics.activity.types[0].id, "anime");
    }

    #[tokio::test]
    async fn build_cleanup_queues_summaries_and_lists() {
        let (library, _vfs) = fixture().await;
        let response = build_cleanup_queues(&library, Vec::new(), 0);

        let queue_ids: Vec<_> = response
            .queues
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        assert!(queue_ids.contains(&"missing-cover"));
        assert!(queue_ids.contains(&"isolated"));
        // broken-asset total is 0 here, so that queue is omitted.
        assert!(!queue_ids.contains(&"broken-asset"));

        let cover = response
            .queues
            .iter()
            .find(|item| item.id == "missing-cover")
            .unwrap();
        assert_eq!(cover.total, 3); // all anime entities are cover-eligible
        assert_eq!(cover.remaining, 2); // B and C lack a cover

        assert_eq!(titles(&response.missing_cover), ["B", "C"]);
        assert_eq!(titles(&response.isolated), ["C"]);
        assert!(response.unresolved_relations.is_empty());
    }

    #[tokio::test]
    async fn broken_local_assets_flags_missing_local_covers_only() {
        let (library, vfs) = fixture().await;
        let (broken, local_total) = broken_local_assets(&library, vfs.as_ref()).await;

        // A is the only entity with a local cover path, and that file doesn't exist.
        assert_eq!(local_total, 1);
        assert_eq!(titles(&broken), ["A"]);
    }

    #[tokio::test]
    async fn build_stats_filters_by_type_and_lists_date_fields() {
        let (library, _vfs) = fixture().await;

        // Whole library: every entity, no per-type date fields.
        let all = build_stats(&library, None);
        assert_eq!(all.total, 4);
        assert!(all.date_fields.is_empty());
        let all_explicit = build_stats(&library, Some("all"));
        assert_eq!(all_explicit.total, 4);

        // Single type: only its entities, and its planning/completed date fields.
        let anime = build_stats(&library, Some("anime"));
        assert_eq!(anime.total, 3);
        assert_eq!(anime.date_fields, ["aired"]);
        // The A↔B link contributes two relations whose source is an anime entity:
        // A's outgoing edge and B's `In` reflection (stats counts both directions).
        assert_eq!(anime.relations, 2);
    }
}
