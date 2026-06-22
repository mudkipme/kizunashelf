use super::error::ApiResult;
use super::state::{get_library, AppState};
use crate::contract::{
    AnalyticsActivity, AnalyticsActivityType, AnalyticsActivityYear, AnalyticsActivityYearType,
    AnalyticsDataQuality, AnalyticsDistributions, AnalyticsRelations, AnalyticsResponse,
    AnalyticsTotals, AnalyticsUnresolvedRelations, CleanupQueueSummary, CleanupQueuesResponse,
    CleanupUnresolvedRelation, StatsResponse, TypeCount,
};
use crate::dates::parse_entity_date;
use crate::library::compare_string;
use crate::relations::{
    build_relation_field_summary_with_index, build_relation_hubs, count_by, outgoing_relations,
    relation_fields, relation_type_pairs, summary_by_id, Count,
};
use crate::types::{DateRole, EntitySummary, FieldType, Library};
use crate::vfs::Vfs;
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize, JsonSchema)]
pub(crate) struct StatsQuery {
    #[serde(rename = "type")]
    entity_type: Option<String>,
}

pub(crate) async fn stats(
    State(state): State<AppState>,
    Query(query): Query<StatsQuery>,
) -> ApiResult<StatsResponse> {
    let library = get_library(&state).await?;
    let summaries: Vec<_> = if query
        .entity_type
        .as_ref()
        .is_some_and(|entity_type| entity_type != "all")
    {
        library
            .summaries()
            .filter(|entity| Some(&entity.entity_type) == query.entity_type.as_ref())
            .cloned()
            .collect()
    } else {
        library.summaries().cloned().collect()
    };
    let ids: std::collections::HashSet<_> =
        summaries.iter().map(|entity| entity.id.clone()).collect();
    let mut top_relations = summaries.clone();
    top_relations.sort_by_key(|item| Reverse(item.relation_count));
    top_relations.truncate(12);

    Ok(Json(StatsResponse {
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
        date_fields: query
            .entity_type
            .as_ref()
            .filter(|entity_type| entity_type.as_str() != "all")
            .and_then(|entity_type| {
                library
                    .config
                    .types
                    .iter()
                    .find(|item| item.id == *entity_type)
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
    }))
}

pub(crate) async fn analytics(State(state): State<AppState>) -> ApiResult<AnalyticsResponse> {
    let library = get_library(&state).await?;
    if let Some(cached) = state.cached_analytics(&library.generated_at).await {
        return Ok(Json((*cached).clone()));
    }
    let response = std::sync::Arc::new(build_analytics(&library));
    state
        .store_analytics(&library.generated_at, std::sync::Arc::clone(&response))
        .await;
    Ok(Json((*response).clone()))
}

pub(crate) async fn cleanup_queues(
    State(state): State<AppState>,
) -> ApiResult<CleanupQueuesResponse> {
    let library = get_library(&state).await?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let (broken_assets, broken_total) = broken_local_assets(&library, vfs.as_ref()).await;
    Ok(Json(build_cleanup_queues(
        &library,
        broken_assets,
        broken_total,
    )))
}

/// Entities whose local cover path points at a file that no longer exists, plus
/// the number of entities that reference a local cover (the denominator).
async fn broken_local_assets(library: &Library, vfs: &dyn Vfs) -> (Vec<EntitySummary>, usize) {
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

fn build_analytics(library: &Library) -> AnalyticsResponse {
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

fn build_cleanup_queues(
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
fn build_activity(
    dated: Vec<(EntitySummary, crate::dates::ParsedEntityDate)>,
) -> AnalyticsActivity {
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
    (1..=12).contains(&month).then_some((month - 1) as usize)
}
