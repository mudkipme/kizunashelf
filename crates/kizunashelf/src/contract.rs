use crate::calendar::{CalendarDay, CalendarEntry, EntityDatesResponse};
use crate::relations::Count;
use crate::types::{
    Entity, EntitySummary, EntityTypeConfig, HomeConfig, KizunaConfig, LibraryDiagnostic, Relation,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub ok: bool,
    pub generated_at: String,
    pub entity_count: usize,
    pub relation_count: usize,
    pub diagnostic_count: usize,
    pub diagnostics: Vec<LibraryDiagnostic>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesResponse {
    pub settings_writable: bool,
    pub content_writable: bool,
    pub external_search_enabled: bool,
    pub external_apply_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsConfigResponse {
    pub config_path: String,
    pub exists: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<KizunaConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PathSuggestionsResponse {
    pub suggestions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    pub taxonomy_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<HomeConfig>,
    pub types: Vec<EntityTypeConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HomeSectionResponse {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub type_label: String,
    pub limit: u32,
    pub sort: String,
    pub direction: String,
    pub total: usize,
    pub items: Vec<EntitySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HomeResponse {
    pub generated_at: String,
    pub title: String,
    pub sections: Vec<HomeSectionResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsResponse {
    pub generated_at: String,
    pub total: usize,
    pub relations: usize,
    pub by_type: Vec<TypeCount>,
    pub date_fields: Vec<String>,
    pub top_relations: Vec<EntitySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeCount {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityListResponse {
    pub items: Vec<EntitySummary>,
    pub total: usize,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDetailResponse {
    pub entity: Entity,
    pub relations: Vec<Relation>,
    pub related_entities: Vec<EntitySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityMutationResponse {
    pub entity: Entity,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEntityRequest {
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_to: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateEntityRequest {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub basename: String,
    #[serde(default)]
    pub frontmatter: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteEntityRequest {
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteEntityResponse {
    pub deleted_id: String,
    pub backup_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProviderSummary {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalCandidate {
    pub provider: String,
    pub source_id: String,
    pub url: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub titles: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSearchResponse {
    pub providers: Vec<ExternalProviderSummary>,
    pub items: Vec<ExternalCandidate>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelationListResponse {
    pub items: Vec<Relation>,
    pub total: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelationTargetSummary {
    pub key: String,
    pub target_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_type_label: Option<String>,
    pub count: usize,
    pub source_types: Vec<Count>,
    pub examples: Vec<EntitySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelationTargetTypeSummary {
    #[serde(rename = "type")]
    pub target_type: String,
    pub type_label: String,
    pub edge_count: usize,
    pub unique_targets: usize,
    pub resolved_targets: usize,
    pub fields: Vec<Count>,
    pub top_targets: Vec<AnalyticsRelationHub>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelationFieldSummary {
    pub field: String,
    pub edge_count: usize,
    pub source_count: usize,
    pub unique_targets: usize,
    pub resolved_targets: usize,
    pub top_targets: Vec<RelationTargetSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelationGroupsResponse {
    pub generated_at: String,
    pub target_types: Vec<RelationTargetTypeSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsCoverageMetric {
    pub name: String,
    pub count: usize,
    pub missing: usize,
    pub total: usize,
    pub percent: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsTimelineYear {
    pub year: i32,
    pub count: usize,
    pub by_type: Vec<Count>,
    pub examples: Vec<EntitySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsTimeline {
    pub total_dated: usize,
    pub years: Vec<AnalyticsTimelineYear>,
    pub seasons: Vec<Count>,
    pub months: Vec<Count>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsRelationHub {
    #[serde(flatten)]
    pub target: RelationTargetSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsRelations {
    pub top_fields: Vec<RelationFieldSummary>,
    pub top_targets: Vec<AnalyticsRelationHub>,
    pub unresolved: AnalyticsUnresolvedRelations,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsUnresolvedRelations {
    pub count: usize,
    pub examples: Vec<Relation>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsDataQuality {
    pub missing_cover: Vec<EntitySummary>,
    pub missing_external_refs: Vec<EntitySummary>,
    pub missing_summary: Vec<EntitySummary>,
    pub isolated: Vec<EntitySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CleanupQueueSummary {
    pub id: String,
    pub label: String,
    pub remaining: usize,
    pub total: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CleanupUnresolvedRelation {
    pub source: EntitySummary,
    pub relation: Relation,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CleanupQueuesResponse {
    pub generated_at: String,
    pub queues: Vec<CleanupQueueSummary>,
    pub missing_cover: Vec<EntitySummary>,
    pub missing_external_refs: Vec<EntitySummary>,
    pub missing_summary: Vec<EntitySummary>,
    pub isolated: Vec<EntitySummary>,
    pub unresolved_relations: Vec<CleanupUnresolvedRelation>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsTotals {
    pub entities: usize,
    pub relations: usize,
    pub unresolved_relations: usize,
    pub dated_entities: usize,
    pub connected_entities: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsDistributions {
    pub by_type: Vec<TypeCount>,
    pub by_relation_field: Vec<Count>,
    pub by_source_target_type: Vec<Count>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsResponse {
    pub generated_at: String,
    pub totals: AnalyticsTotals,
    pub distributions: AnalyticsDistributions,
    pub coverage: Vec<AnalyticsCoverageMetric>,
    pub timeline: AnalyticsTimeline,
    pub relations: AnalyticsRelations,
    pub data_quality: AnalyticsDataQuality,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarFilters {
    #[serde(rename = "type")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarTotals {
    pub entries: usize,
    pub taxonomy: usize,
    pub daily_notes: usize,
    pub days_with_entries: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarResponse {
    pub generated_at: String,
    pub year: i32,
    pub month: u32,
    pub filters: CalendarFilters,
    pub totals: CalendarTotals,
    pub days: Vec<CalendarDay>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiSchemas {
    pub error: ErrorResponse,
    pub health: HealthResponse,
    pub settings_config: SettingsConfigResponse,
    pub path_suggestions: PathSuggestionsResponse,
    pub kizuna_config: KizunaConfig,
    pub config: ConfigResponse,
    pub home: HomeResponse,
    pub stats: StatsResponse,
    pub analytics: AnalyticsResponse,
    pub entities: EntityListResponse,
    pub entity_detail: EntityDetailResponse,
    pub entity_dates: EntityDatesResponse,
    pub relations: RelationListResponse,
    pub relation_groups: RelationGroupsResponse,
    pub calendar: CalendarResponse,
    pub calendar_entry: CalendarEntry,
    pub library: crate::types::Library,
}
