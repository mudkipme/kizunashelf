use crate::types::{DateRole, EntitySummary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CalendarSource {
    All,
    Taxonomy,
    DailyNote,
}

#[derive(Clone, Debug)]
pub struct CalendarBuildOptions {
    pub year: i32,
    pub month: u32,
    pub entity_type: Option<String>,
    pub source: CalendarSource,
}

#[derive(Clone, Debug)]
pub struct CalendarPlanningOptions {
    pub year: i32,
    pub entity_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSnippet {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    pub line: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEntry {
    pub id: String,
    pub date: String,
    pub source: CalendarEntrySource,
    pub entity: EntitySummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippets: Option<Vec<CalendarSnippet>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CalendarEntrySource {
    Taxonomy,
    DailyNote,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDay {
    pub date: String,
    pub entries: Vec<CalendarEntry>,
    pub counts: CalendarDayCounts,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDayCounts {
    pub total: usize,
    pub taxonomy: usize,
    pub daily_notes: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDateMetadataEntry {
    pub id: String,
    pub field: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDateDailyNoteEntry {
    pub id: String,
    pub date: String,
    pub note_path: String,
    pub snippets: Vec<CalendarSnippet>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDatesResponse {
    pub generated_at: String,
    pub entity_id: String,
    pub totals: EntityDatesTotals,
    pub metadata: Vec<EntityDateMetadataEntry>,
    pub daily_notes: Vec<EntityDateDailyNoteEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDatesTotals {
    pub metadata: usize,
    pub daily_notes: usize,
    pub snippets: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPlanningTypeOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPlanningFilters {
    pub year: i32,
    #[serde(rename = "type")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPlanningTotals {
    pub entities: usize,
    pub dated_entries: usize,
    pub upcoming: usize,
    pub recently_completed: usize,
    pub unscheduled: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPlanningDatePoint {
    pub entity: EntitySummary,
    pub field: String,
    pub field_label: String,
    pub value: String,
    pub year: i32,
    pub month: u32,
    pub sort_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
    pub role: DateRole,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPlanningMonth {
    pub month: u32,
    pub label: String,
    pub entries: Vec<CalendarPlanningDatePoint>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPlanningSeason {
    pub key: String,
    pub label: String,
    pub months: String,
    pub entries: Vec<CalendarPlanningDatePoint>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPlanningBoard {
    pub upcoming: Vec<CalendarPlanningDatePoint>,
    pub recently_completed: Vec<CalendarPlanningDatePoint>,
    pub unscheduled: Vec<EntitySummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPlanningResponse {
    pub generated_at: String,
    pub filters: CalendarPlanningFilters,
    pub type_options: Vec<CalendarPlanningTypeOption>,
    pub totals: CalendarPlanningTotals,
    pub year_months: Vec<CalendarPlanningMonth>,
    pub seasons: Vec<CalendarPlanningSeason>,
    pub board: CalendarPlanningBoard,
}
