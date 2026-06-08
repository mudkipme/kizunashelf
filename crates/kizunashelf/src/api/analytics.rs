use super::error::ApiResult;
use super::state::{get_library, AppState};
use crate::contract::{
    AnalyticsCoverageMetric, AnalyticsDataQuality, AnalyticsDistributions, AnalyticsRelations,
    AnalyticsResponse, AnalyticsTimeline, AnalyticsTimelineYear, AnalyticsTotals,
    AnalyticsUnresolvedRelations, CleanupQueueSummary, CleanupQueuesResponse,
    CleanupUnresolvedRelation, StatsResponse, TypeCount,
};
use crate::dates::{date_sort_key, parse_entity_date, season_compare_value};
use crate::library::compare_string;
use crate::relations::{
    build_relation_field_summary_with_index, build_relation_hubs, count_by, outgoing_relations,
    relation_fields, relation_type_pairs, summary_by_id, Count,
};
use crate::types::{DateRole, EntitySummary, FieldType, Library};
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
            .summaries
            .iter()
            .filter(|entity| Some(&entity.entity_type) == query.entity_type.as_ref())
            .cloned()
            .collect()
    } else {
        library.summaries.clone()
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
                    .entities
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
    Ok(Json(build_analytics(&library)))
}

pub(crate) async fn cleanup_queues(
    State(state): State<AppState>,
) -> ApiResult<CleanupQueuesResponse> {
    let library = get_library(&state).await?;
    Ok(Json(build_cleanup_queues(&library)))
}

fn build_analytics(library: &Library) -> AnalyticsResponse {
    let summaries = &library.summaries;
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
    let with_cover_count = summaries
        .iter()
        .filter(|entity| quality.requires_cover(entity))
        .filter(|entity| entity.image.is_some())
        .count();
    let cover_total = summaries
        .iter()
        .filter(|entity| quality.requires_cover(entity))
        .count();
    let with_refs_count = summaries
        .iter()
        .filter(|entity| quality.requires_external_refs(entity))
        .filter(|entity| !entity.external_refs.is_empty())
        .count();
    let refs_total = summaries
        .iter()
        .filter(|entity| quality.requires_external_refs(entity))
        .count();
    let connected_count = summaries
        .iter()
        .filter(|entity| quality.requires_relations(entity))
        .filter(|entity| entity.relation_count > 0)
        .count();
    let relations_total = summaries
        .iter()
        .filter(|entity| quality.requires_relations(entity))
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
        coverage: build_coverage_metrics(
            with_cover_count,
            cover_total,
            with_refs_count,
            refs_total,
            connected_count,
            relations_total,
            outgoing.len() - unresolved.len(),
            outgoing.len(),
        ),
        timeline: build_timeline(dated),
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
            missing_summary: Vec::new(),
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

fn build_cleanup_queues(library: &Library) -> CleanupQueuesResponse {
    let summaries = &library.summaries;
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
    let missing_summary: Vec<EntitySummary> = Vec::new();
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
    let queues = cleanup_queue_summaries(
        &missing_cover,
        cover_total,
        &missing_external_refs,
        refs_total,
        &isolated,
        relations_total,
        unresolved_relations.len(),
        outgoing.len(),
    );

    CleanupQueuesResponse {
        generated_at: library.generated_at.clone(),
        queues,
        missing_cover,
        missing_external_refs,
        missing_summary,
        isolated,
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

fn build_coverage_metrics(
    with_cover_count: usize,
    cover_total: usize,
    with_refs_count: usize,
    refs_total: usize,
    connected_count: usize,
    relations_total: usize,
    resolved_relations: usize,
    relations_count: usize,
) -> Vec<AnalyticsCoverageMetric> {
    let mut metrics = Vec::new();
    push_coverage_metric(&mut metrics, "Cover", with_cover_count, cover_total);
    push_coverage_metric(&mut metrics, "External refs", with_refs_count, refs_total);
    push_coverage_metric(&mut metrics, "Relations", connected_count, relations_total);
    push_coverage_metric(
        &mut metrics,
        "Resolved relation targets",
        resolved_relations,
        relations_count,
    );
    metrics
}

fn push_coverage_metric(
    metrics: &mut Vec<AnalyticsCoverageMetric>,
    name: &str,
    count: usize,
    total: usize,
) {
    if total > 0 {
        metrics.push(build_coverage_metric(name, count, total));
    }
}

fn cleanup_queue_summaries(
    missing_cover: &[EntitySummary],
    cover_total: usize,
    missing_external_refs: &[EntitySummary],
    refs_total: usize,
    isolated: &[EntitySummary],
    relations_total: usize,
    unresolved_relations: usize,
    outgoing_relations: usize,
) -> Vec<CleanupQueueSummary> {
    let mut queues = Vec::new();
    push_cleanup_queue(
        &mut queues,
        "missing-cover",
        "Missing Cover",
        missing_cover.len(),
        cover_total,
    );
    push_cleanup_queue(
        &mut queues,
        "missing-refs",
        "Missing External Refs",
        missing_external_refs.len(),
        refs_total,
    );
    push_cleanup_queue(
        &mut queues,
        "isolated",
        "Isolated Nodes",
        isolated.len(),
        relations_total,
    );
    push_cleanup_queue(
        &mut queues,
        "unresolved-relations",
        "Unresolved Relations",
        unresolved_relations,
        outgoing_relations,
    );
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

fn build_coverage_metric(name: &str, count: usize, total: usize) -> AnalyticsCoverageMetric {
    AnalyticsCoverageMetric {
        name: name.to_string(),
        count,
        missing: total - count,
        total,
        percent: if total == 0 {
            0
        } else {
            ((count as f64 / total as f64) * 100.0).round() as i64
        },
    }
}

fn build_timeline(
    dated: Vec<(EntitySummary, crate::dates::ParsedEntityDate)>,
) -> AnalyticsTimeline {
    let mut by_year: HashMap<i32, Vec<EntitySummary>> = HashMap::new();
    let mut by_season: HashMap<String, (i32, String, Vec<EntitySummary>)> = HashMap::new();
    let mut by_month: HashMap<String, Vec<EntitySummary>> = HashMap::new();
    for (entity, date) in &dated {
        by_year.entry(date.year).or_default().push(entity.clone());
        if let Some(season) = &date.season {
            by_season
                .entry(format!("{} {}", date.year, season))
                .or_insert_with(|| (date.year, season.clone(), Vec::new()))
                .2
                .push(entity.clone());
        }
        if let Some(month) = date.month {
            by_month
                .entry(format!("{}-{month:02}", date.year))
                .or_default()
                .push(entity.clone());
        }
    }
    let mut years: Vec<_> = by_year.into_iter().collect();
    years.sort_by_key(|item| Reverse(item.0));
    let years = years
        .into_iter()
        .map(|(year, mut entities)| {
            let by_type = count_by(&entities, |entity| entity.type_label.clone());
            entities.sort_by(|a, b| {
                compare_string(
                    date_sort_key(b.dates.first().map(|item| item.value.as_str()))
                        .as_deref()
                        .unwrap_or_default(),
                    date_sort_key(a.dates.first().map(|item| item.value.as_str()))
                        .as_deref()
                        .unwrap_or_default(),
                )
            });
            AnalyticsTimelineYear {
                year,
                count: entities.len(),
                by_type,
                examples: unique_entities_by_id(entities)
                    .into_iter()
                    .take(6)
                    .collect(),
            }
        })
        .collect::<Vec<_>>();

    let mut seasons: Vec<_> = by_season.into_iter().collect();
    seasons.sort_by(|(_, a), (_, b)| {
        b.0.cmp(&a.0)
            .then_with(|| season_compare_value(&b.1).cmp(&season_compare_value(&a.1)))
    });
    let seasons = seasons
        .into_iter()
        .take(12)
        .map(|(name, (_, _, entities))| Count {
            name,
            count: entities.len(),
        })
        .collect::<Vec<_>>();

    let mut months: Vec<_> = by_month.into_iter().collect();
    months.sort_by(|a, b| compare_string(&b.0, &a.0));
    let months = months
        .into_iter()
        .take(18)
        .map(|(name, entities)| Count {
            name,
            count: entities.len(),
        })
        .collect::<Vec<_>>();

    AnalyticsTimeline {
        total_dated: dated.len(),
        years,
        seasons,
        months,
    }
}

fn unique_entities_by_id(entities: Vec<EntitySummary>) -> Vec<EntitySummary> {
    let mut seen = HashSet::new();
    entities
        .into_iter()
        .filter(|entity| seen.insert(entity.id.clone()))
        .collect()
}
