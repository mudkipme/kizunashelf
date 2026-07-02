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

/// Which slice of activity to show. `All` is the full reverse-chronological feed;
/// `Recent` hides forward-looking dates (planning fields, scheduled episodes,
/// future daily notes) — so it's the recent past, not just completions; `UpNext`
/// shows only what's still ahead (planning fields + scheduled episodes from today
/// on, and future daily notes), in ascending order; `CatchUp` is the mirror of
/// `UpNext` — planning dates that have *passed* while the entity is still
/// `planning` (released/aired, still on your list), reverse-chronological so the
/// most recently-available sits on top and the old tail pages away.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ActivityMode {
    #[default]
    All,
    Recent,
    UpNext,
    CatchUp,
}

#[derive(Clone, Debug)]
pub struct ActivityBuildOptions {
    /// Opaque `YYYY-MM` cursor from the previous page; the next page continues
    /// strictly past it — older for `All`/`Recent`, newer for `UpNext`.
    pub cursor: Option<String>,
    /// How many *non-empty* months to include in this page. Used only when
    /// [`Self::min_items`] is `None` (the `/upcoming` month-horizon path).
    pub months: u32,
    /// Target number of *items* per page. When set, the page accumulates whole
    /// months (in feed order) until it holds at least this many items — so a sparse
    /// feed with one item each in Dec/Sep/Jun fills a single page instead of paging
    /// month-by-month. Months stay atomic (never split across pages), so the cursor
    /// remains `YYYY-MM` and a page may slightly exceed the target. `None` falls
    /// back to the month count in [`Self::months`].
    pub min_items: Option<u32>,
    pub entity_type: Option<String>,
    pub source: CalendarSource,
    pub mode: ActivityMode,
    /// Today's date (`YYYY-MM-DD`) — the reference point for the mode filters.
    pub today: String,
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

/// Identifies whether a calendar entry came from taxonomy metadata, a daily
/// note, or a dated episode/track in the entity body.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CalendarEntrySource {
    Taxonomy,
    DailyNote,
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
    /// The next cursor (`YYYY-MM`) to load the following page, or `None` at the
    /// end of the feed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub items: Vec<ActivityItem>,
}

/// The upcoming-window feed: future release/planning dates and scheduled episode
/// air dates within a horizon, ascending and flattened (no cursor). Daily-note
/// mentions are excluded — this is the "what's coming" source for the Home "Coming
/// up" section and the iOS reminder scheduler. Reuses [`ActivityItem`], so each
/// item already carries the entity (its `title` + `titles` map for localization +
/// cover `image`) and the date's source (a date field with its role, or the
/// episodes airing that day).
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingResponse {
    pub generated_at: String,
    pub items: Vec<ActivityItem>,
}
