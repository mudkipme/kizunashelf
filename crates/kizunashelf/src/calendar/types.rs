use crate::types::{DateRole, EntitySummary, EpisodeDateRole};
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

#[derive(Clone, Debug)]
pub struct ActivityBuildOptions {
    /// Exclusive upper bound as a `YYYY-MM` month key — the feed returns months
    /// strictly older than this. `None` starts at the most recent activity.
    pub before: Option<String>,
    /// How many *non-empty* months to include in this page.
    pub months: u32,
    pub entity_type: Option<String>,
    pub source: CalendarSource,
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
    /// Present only for `episode`-source entries: the episode this date belongs
    /// to (its number, title, and whether the date is its air or completion).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode: Option<CalendarEpisode>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CalendarEntrySource {
    Taxonomy,
    DailyNote,
    /// A dated episode/track from an entity's episodes section.
    Episode,
}

/// The item behind an `episode`-source calendar entry.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEpisode {
    pub key: String,
    pub title: String,
    pub role: EpisodeDateRole,
    /// The schema-defined heading of the section this item lives under (e.g.
    /// "Episodes", "Tracks", "Tasks"), so clients label it per the entity's type
    /// rather than with a hardcoded term.
    pub heading: String,
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
    pub episodes: usize,
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
    pub just_started: usize,
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
    pub just_started: Vec<CalendarPlanningDatePoint>,
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

/// One reverse-chronological activity item: everything that happened to a single
/// entity on a single date, collapsed together. A daily-note mention, a
/// started/completed date stamp, and episode air/completion dates that share a
/// `(date, entity)` all land in one item's `entries`.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub date: String,
    pub entity: EntitySummary,
    pub entries: Vec<ActivityEntry>,
}

/// A constituent of an [`ActivityItem`], discriminated by `source` — the same
/// flat-struct-plus-discriminator shape as [`CalendarEntry`] (the contract has no
/// tagged enums). Only the fields for that source are populated.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub source: CalendarEntrySource,
    /// `taxonomy`: the date field's name, its raw value, and its resolved role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<DateRole>,
    /// `daily-note`: the note path and the mention snippets for this entity/date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippets: Option<Vec<CalendarSnippet>>,
    /// `episode`: the air-vs-completion role, the schema section heading, and the
    /// episodes sharing this date (a binge collapses into one entry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_role: Option<EpisodeDateRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episodes: Option<Vec<ActivityEpisodeRef>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEpisodeRef {
    pub key: String,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityResponse {
    pub generated_at: String,
    /// The next `before` cursor (`YYYY-MM`) to load the following page, or `None`
    /// at the end of history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub items: Vec<ActivityItem>,
}
