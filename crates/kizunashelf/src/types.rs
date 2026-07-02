use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityTypeConfig {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_priority: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<FilenameConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_sections: Vec<BodySection>,
    /// Daily-note logging config. **Its presence opts the type into logging** — the
    /// quick-log / "check episode" flows write a line to the daily note only for
    /// types that declare a `log` block. The type's hashtag lives as a literal in
    /// `lineFormat`; unset fields fall back to `dailyNotes.log`. Resolve via
    /// [`KizunaConfig::resolve_log_config`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<TypeLogConfig>,
    // Required (and so non-optional in generated clients): a type always carries
    // a `fields` array, even if empty. The editor and templates always write it.
    pub fields: Vec<FieldConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FilenameConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_language: Option<String>,
    /// When `original`, the filename basename is the language-agnostic fallback
    /// title (mirrors a title field's `titleRole`). The viewer's language picks
    /// the displayed title; this is the floor when no language matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_role: Option<TitleRole>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FieldConfig {
    pub field: String,
    pub field_type: FieldType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_role: Option<TitleRole>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_fields: Vec<ExternalFieldMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_options: Vec<String>,
    /// Semantic role of an `enum` field. `status` marks the one field the engine
    /// treats as the entity's lifecycle status; behavior keys off this role, never
    /// the field name. See [`EnumRole`] and `docs/status-role-plan.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_role: Option<EnumRole>,
    /// For an `enumRole: status` field: maps each canonical status to the user
    /// option strings that mean it. Absent (or a canonical absent from it) leaves
    /// that canonical unmapped — the field is still the status field, but no
    /// canonical behavior fires. See [`StatusValues`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_values: Option<StatusValues>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_progress_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_role: Option<DateRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season_language: Option<SeasonLanguage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_type: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FieldType {
    Id,
    Title,
    Image,
    ImageList,
    Enum,
    EnumList,
    Progress,
    TotalProgress,
    Rating,
    Bool,
    Season,
    Date,
    ExternalRef,
    Relation,
    Text,
    TextList,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TitleRole {
    Original,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalFieldMapping {
    pub source: String,
    pub field: String,
}

/// A declared section of an entity's Markdown body, addressed by its heading.
/// Generalizes the old `bodyMappings`: a flat struct discriminated by `kind`
/// (mirroring `FieldConfig`), so the same per-type mechanism covers
/// external-metadata sections *and* the built-in episodes/tracks list.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BodySection {
    /// The Markdown heading (text only, no `#`s) this section lives under.
    pub heading: String,
    pub kind: BodySectionKind,
    /// `kind = external`: the provider field(s) that fill this heading. The matched
    /// candidate's source is chosen, exactly like [`FieldConfig::external_fields`],
    /// so one heading can be filled from multiple providers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_fields: Vec<ExternalFieldMapping>,
    /// `kind = episodes`: how watched/read state is tracked. Defaults to `checklist`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking: Option<EpisodeTracking>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BodySectionKind {
    // Plain `//` (not `///`): doc comments on variants make schemars emit the enum
    // as `oneOf` of consts, which swift-openapi-generator can't render as a Swift
    // enum with named cases. Keep it a flat string enum, like `FieldType`/`DateRole`.
    //
    // Filled from an external provider field on match (the old `bodyMappings`).
    External,
    // The built-in episodes/tracks list (an ordered, optionally-checkable list,
    // optionally grouped by season/disc sub-headings).
    Episodes,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum EpisodeTracking {
    // Plain `//` comments (see `BodySectionKind`): keep this a flat string enum so
    // swift-openapi-generator renders proper `.checklist`/`.none` cases.
    //
    // Per-item checkboxes (`- [ ]` / `- [x]`) — tracks exactly which are watched.
    Checklist,
    // No tracking — a plain ordered list. (A count-only mode belongs on a
    // `progress`/`total` field, not the section, so there's no `progress` variant.)
    None,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DateRole {
    Planning,
    Started,
    Completed,
    /// A date the user *attends* (a concert, exhibition, release event) rather than
    /// a release they passively consume. Whether it reads as an intention (up next)
    /// or a record (recent) is **derived from the entity's status**, not encoded as
    /// separate roles — see `docs/status-role-plan.md`. Wired into the feed in a
    /// later phase; harmless everywhere that matches only the other three roles.
    Event,
}

/// A field's semantic *role* beyond its raw `FieldType`. Currently only `status`:
/// it marks the one enum field the engine treats as the entity's lifecycle status.
/// Like `DateRole`/`TitleRole`, meaning flows from this role, never from the field
/// name. Kept a flat string enum so swift-openapi-generator renders proper cases.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EnumRole {
    Status,
}

/// The small fixed set of lifecycle statuses the engine can reason about. User
/// option strings map onto these via [`StatusValues`]; an entity's own value may
/// resolve to `None` (unmapped) and is still preserved. `Dropped` sits *outside*
/// the planning→ongoing→completed progression (see [`CanonicalStatus::rank`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CanonicalStatus {
    Planning,
    Ongoing,
    Completed,
    Dropped,
}

impl CanonicalStatus {
    /// Position on the planning→ongoing→completed chain, used for the monotonic
    /// log flip (never demote). `Dropped` is off the chain and returns `None` —
    /// callers never auto-flip *from* or *to* it via the rank.
    pub fn rank(self) -> Option<u8> {
        match self {
            CanonicalStatus::Planning => Some(0),
            CanonicalStatus::Ongoing => Some(1),
            CanonicalStatus::Completed => Some(2),
            CanonicalStatus::Dropped => None,
        }
    }
}

/// Maps each canonical status to the user-defined option strings that mean it. The
/// **first** option listed for a canonical is the *write target* — what a log flip
/// writes when it sets that status. An empty vec means that canonical is unmapped
/// (no behavior fires for it). Fixed optional keys (not an open map) for
/// codegen-friendliness and clean "unmapped" semantics.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusValues {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planning: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ongoing: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped: Vec<String>,
}

impl StatusValues {
    /// The user option strings mapped to `canonical`, in declared order (the first
    /// is the write target).
    pub fn options(&self, canonical: CanonicalStatus) -> &[String] {
        match canonical {
            CanonicalStatus::Planning => &self.planning,
            CanonicalStatus::Ongoing => &self.ongoing,
            CanonicalStatus::Completed => &self.completed,
            CanonicalStatus::Dropped => &self.dropped,
        }
    }

    /// The value written to set `canonical` (the first mapped option), or `None`
    /// when that canonical is unmapped.
    pub fn write_value(&self, canonical: CanonicalStatus) -> Option<&str> {
        self.options(canonical).first().map(String::as_str)
    }

    /// The canonical a raw user value maps to (case-sensitive, exact match), or
    /// `None` when the value is unmapped. First canonical that lists the value wins.
    pub fn canonical_of(&self, value: &str) -> Option<CanonicalStatus> {
        [
            CanonicalStatus::Planning,
            CanonicalStatus::Ongoing,
            CanonicalStatus::Completed,
            CanonicalStatus::Dropped,
        ]
        .into_iter()
        .find(|&canonical| self.options(canonical).iter().any(|option| option == value))
    }
}

/// An entity's resolved status: the status field's name, the raw user value, and
/// the canonical it maps to (`None` when the value is unmapped or no mapping is
/// configured). Present on [`EntitySummary`] only when the type declares a status
/// field and the entity carries a value for it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedStatus {
    pub field: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical: Option<CanonicalStatus>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
pub enum SeasonLanguage {
    #[default]
    #[serde(rename = "zh")]
    Zh,
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "en")]
    En,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HomeSectionConfig {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<HomeSectionFilterConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<SortDirection>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HomeSectionFilterConfig {
    pub field: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HomeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<HomeSectionConfig>,
}

/// Configuration for the built-in **tags** field — a universal, cross-type label
/// list. Tags are a vault-level "well-known field": the *name* is configured here
/// (defaulting to `tags`), so the engine reads the field name from config rather
/// than hardcoding it. A schema field that happens to share this name is ignored
/// in favor of the built-in. This is a deliberate, narrow extension of the
/// schema-driven model — meaning still flows config → behavior, just at the vault
/// scope rather than the per-type scope.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagsConfig {
    /// The frontmatter key holding the entity's tag list. Defaults to `tags`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyNotesConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// Moment.js-style date format (as used by Obsidian Daily Notes) for the file
    /// path relative to the daily-notes folder, without the `.md` extension —
    /// e.g. `YYYY-MM-DD` or `YYYY/MM/YYYY-MM-DD`. Defaults to `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_format: Option<String>,
    /// Vault-relative path to a template used to seed a daily note that does not
    /// exist yet (date tokens substituted). Absent → a new note starts empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Global defaults for daily-note logging — the heading written under and the
    /// line format. Per-type `log` blocks override these; see
    /// [`KizunaConfig::resolve_log_config`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<DailyNoteLogDefaults>,
}

/// Global defaults for daily-note logging, under `dailyNotes.log`. Both fields are
/// optional; a per-type [`TypeLogConfig`] overrides them, and the built-ins
/// ([`DEFAULT_LOG_SECTION`] / [`DEFAULT_LOG_LINE_FORMAT`]) fill any remaining gap.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyNoteLogDefaults {
    /// Heading to write log lines under, as raw heading text (no `#`, default h2) —
    /// consistent with `bodySections[].heading`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Line template. Tokens: `{title}` (the entity — **always rendered as a
    /// `[[wikilink]]`**, since that link is what ties the line back to the entity;
    /// write `{title}`, not `[[{title}]]`, though the latter isn't doubled),
    /// `{note}` (freeform), `{date}`. Empty tokens collapse with surrounding
    /// whitespace. There is no episode token — logging is independent of the
    /// episode list, so put an episode number in `{note}` if you want one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_format: Option<String>,
}

/// Per-type daily-note logging config, under `types[].log`. **Presence opts the
/// type into logging.** The type's hashtag is written as a literal inside
/// `lineFormat` (e.g. `- {title} {note} #Anime`), not a separate field —
/// so it's explicit, never inferred from the type name. Unset fields fall back to
/// `dailyNotes.log`, then the built-ins.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeLogConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_format: Option<String>,
}

/// App-level configuration. Describes how *this machine* runs KizunaShelf and
/// where the vault lives on disk. This is owned by the runtime and passed
/// inline: env vars for web, the native vault switcher for desktop, and the
/// host app's local state for iOS. It is never stored in the synced vault config.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub vault_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_writable: Option<bool>,
}

/// Vault-level configuration. Describes the vault's content schema (taxonomy,
/// assets, entity types, home dashboard, daily notes). Stored inside the vault
/// at `<vaultRoot>/KizunaShelf/config.yaml` so it travels with the vault and is
/// synced by the vault's own syncing method.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VaultConfig {
    pub taxonomy_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<HomeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_notes: Option<DailyNotesConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<TagsConfig>,
    pub types: Vec<EntityTypeConfig>,
}

/// Merged runtime view of the app and vault config. This is the shape consumed
/// throughout the library, relations, calendar, and API handlers. It is built
/// from [`AppConfig`] + [`VaultConfig`] at load time; only the vault part is
/// persisted by the core settings endpoints.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KizunaConfig {
    pub vault_root: String,
    pub taxonomy_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_writable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<HomeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_notes: Option<DailyNotesConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<TagsConfig>,
    pub types: Vec<EntityTypeConfig>,
}

pub const DEFAULT_ASSET_ROOT: &str = "Assets";

/// Default frontmatter key for the built-in tags field when the vault config does
/// not set one (the common case). This is a *default value*, not a hardcoded
/// branch: behavior reads [`KizunaConfig::tags_field`], which returns this only as
/// a fallback.
pub const DEFAULT_TAGS_FIELD: &str = "tags";

/// Built-in daily-note log heading when neither the type nor `dailyNotes.log` sets
/// one — raw heading text (default h2), matching `bodySections`.
pub const DEFAULT_LOG_SECTION: &str = "Log";

/// Built-in daily-note log line template. `{title}` renders as a `[[wikilink]]`
/// and `{note}` collapses when empty, so a note-less log of a type with no
/// per-type format renders the bare `- [[Title]]`.
pub const DEFAULT_LOG_LINE_FORMAT: &str = "- {title} {note}";

/// The resolved daily-note logging config for one entity type (see
/// [`KizunaConfig::resolve_log_config`]).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLog {
    pub section: String,
    pub line_format: String,
}

/// The first non-empty (trimmed) value, in priority order.
fn first_nonempty(values: [Option<&str>; 2]) -> Option<&str> {
    values
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

impl KizunaConfig {
    /// The type config whose `id` matches `type_id`, or `None` for an unknown
    /// type. Field meaning is schema-driven, so callers resolve a type's config
    /// here rather than reasoning about entity types directly.
    pub fn type_config(&self, type_id: &str) -> Option<&EntityTypeConfig> {
        self.types.iter().find(|item| item.id == type_id)
    }

    /// The resolved daily-note logging config for `type_id`, or `None` when the
    /// type isn't loggable (it has no `log` block). Section and line format fall
    /// back: the type's overrides → the `dailyNotes.log` defaults → the built-ins.
    pub fn resolve_log_config(&self, type_id: &str) -> Option<ResolvedLog> {
        let type_log = self.type_config(type_id)?.log.as_ref()?;
        let defaults = self
            .daily_notes
            .as_ref()
            .and_then(|daily| daily.log.as_ref());
        Some(ResolvedLog {
            section: first_nonempty([
                type_log.section.as_deref(),
                defaults.and_then(|item| item.section.as_deref()),
            ])
            .unwrap_or(DEFAULT_LOG_SECTION)
            .to_string(),
            line_format: first_nonempty([
                type_log.line_format.as_deref(),
                defaults.and_then(|item| item.line_format.as_deref()),
            ])
            .unwrap_or(DEFAULT_LOG_LINE_FORMAT)
            .to_string(),
        })
    }

    /// Vault-relative directory where downloaded assets are stored.
    pub fn resolved_asset_root(&self) -> &str {
        self.asset_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_ASSET_ROOT)
    }

    /// The frontmatter key for the built-in tags field — configured via
    /// `tags.field`, defaulting to [`DEFAULT_TAGS_FIELD`]. The single resolution
    /// point so no code hardcodes the tag field name.
    pub fn tags_field(&self) -> &str {
        self.tags
            .as_ref()
            .and_then(|tags| tags.field.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_TAGS_FIELD)
    }

    /// Builds the merged runtime config from its on-disk parts.
    pub fn from_parts(app: AppConfig, vault: VaultConfig) -> Self {
        Self {
            vault_root: app.vault_root,
            content_writable: app.content_writable,
            taxonomy_root: vault.taxonomy_root,
            asset_root: vault.asset_root,
            home: vault.home,
            daily_notes: vault.daily_notes,
            tags: vault.tags,
            types: vault.types,
        }
    }

    /// Splits the merged config back into the app and vault parts for persisting.
    pub fn into_parts(self) -> (AppConfig, VaultConfig) {
        (
            AppConfig {
                vault_root: self.vault_root,
                content_writable: self.content_writable,
            },
            VaultConfig {
                taxonomy_root: self.taxonomy_root,
                asset_root: self.asset_root,
                home: self.home,
                daily_notes: self.daily_notes,
                tags: self.tags,
                types: self.types,
            },
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDateValue {
    pub field: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed: Option<crate::dates::ParsedEntityDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntitySummary {
    pub id: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub type_label: String,
    pub title: String,
    pub titles: BTreeMap<String, String>,
    pub dates: Vec<EntityDateValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub path: String,
    pub basename: String,
    pub external_refs: BTreeMap<String, String>,
    /// The entity's built-in tags (the frontmatter `tags` list). Always present
    /// (empty when none) so clients can render it without a null check.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Watched/total for the type's episodes section, when it declares one — a
    /// resident derived stat (computed at parse time) so list/grid views can show
    /// progress without reading bodies. `None` for types without episodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_progress: Option<EpisodeProgress>,
    /// The entity's resolved lifecycle status — present only when the type declares
    /// an `enumRole: status` field and the entity carries a value for it. Resolved
    /// at parse time (see [`crate::status::resolve_status`]) so feed/filters/badges
    /// read it without re-deriving. `None` for types without a status field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResolvedStatus>,
    pub relation_count: u32,
}

/// A watched/total count for an entity's episodes/tracks section.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeProgress {
    pub watched: usize,
    pub total: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    #[serde(flatten)]
    pub summary: EntitySummary,
    pub revision: String,
    pub frontmatter: Map<String, Value>,
    pub body: String,
    pub raw: String,
}

/// The slim, resident representation of an entity kept in the cached [`Library`].
/// It carries the summary, frontmatter (for in-memory list/home/asset filtering),
/// revision, and the body's wikilink targets (`body_links`), but deliberately
/// omits the full `body`/`raw` — those are loaded on demand from disk via
/// [`crate::library::load_entity`] when a full [`Entity`] is needed (detail page,
/// mutations, asset writes). Keeping `body_links` resident (they are small — just
/// the link targets, not the body) lets the relation graph be rebuilt wholly in
/// memory from the resident records, without re-reading every entity body.
///
/// This is an internal/resident type: API responses are always built from
/// [`EntitySummary`], so `body_links` never reaches a client.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecord {
    #[serde(flatten)]
    pub summary: EntitySummary,
    pub revision: String,
    pub frontmatter: Map<String, Value>,
    /// Wikilink targets extracted from the body at parse time (for relation
    /// building). Empty for entities with no body links.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_links: Vec<String>,
    /// The vault file's last-modified time (unix nanos), captured from the
    /// directory enumeration at index time and cached. Resident core-only data
    /// used ONLY for the "recently updated" sort — never serialized to clients
    /// and never fed into the calendar (which derives dates from schema fields).
    /// `0` when the backend can't report a modification time.
    #[serde(default)]
    pub file_modified_unix_nanos: u128,
    /// Per-episode dates (air `📅` / completion `✅`) parsed from the body's
    /// episodes section at index time and cached, so the calendar can place
    /// episodes without re-reading raw files. Resident core-only data — never
    /// serialized to a client (the calendar builds its own entries from it).
    /// Empty unless the type has an episodes section with dated items.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub episode_dates: Vec<EpisodeDate>,
}

/// One dated episode/track from an entity's episodes section, flattened for the
/// calendar: an item with both an air and a completion date yields two of these.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeDate {
    pub key: String,
    pub title: String,
    /// The calendar date (`YYYY-MM-DD`).
    pub date: String,
    pub role: EpisodeDateRole,
}

/// Which episode date a calendar placement came from: the air/release date
/// (`📅`, "scheduled") or the completion date (`✅`, "completed").
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum EpisodeDateRole {
    Scheduled,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    pub target_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    pub field: String,
    pub direction: RelationDirection,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RelationDirection {
    Out,
    In,
}

/// Hashes everything the memoized derived views depend on into a stable
/// fingerprint: the schema (all derivation flows schema → behavior), each
/// record's content `revision`, and the relation graph (which also captures
/// daily-note-driven changes that no entity `revision` would reflect). Two loads
/// of identical vault content produce the same value, so a warm reload keeps a
/// memoized analytics result valid; any content or schema change flips it.
fn content_revision(
    config: &KizunaConfig,
    records: &[EntityRecord],
    relations: &[Relation],
) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    // Serialization failure is implausible for an in-memory config; fall back to a
    // constant so the rest of the fingerprint still discriminates content.
    serde_json::to_vec(config)
        .unwrap_or_default()
        .hash(&mut hasher);
    // Records carry a deterministic resident order (type then title), so hash them
    // in order.
    for record in records {
        record.summary.id.hash(&mut hasher);
        record.revision.hash(&mut hasher);
    }
    // The relation list's order is not a guaranteed-stable part of the library —
    // value-equivalence compares it as a *set* (see the cache tests' relation
    // assertions) — so fold each relation order-independently (XOR of per-relation
    // hashes) to keep the fingerprint stable regardless of relation ordering.
    let relations_digest = relations.iter().fold(0u64, |acc, relation| {
        let mut relation_hasher = DefaultHasher::new();
        relation.source_id.hash(&mut relation_hasher);
        relation.target_id.hash(&mut relation_hasher);
        relation.target_title.hash(&mut relation_hasher);
        relation.field.hash(&mut relation_hasher);
        relation.direction.hash(&mut relation_hasher);
        acc ^ relation_hasher.finish()
    });
    relations_digest.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub config: KizunaConfig,
    pub records: Vec<EntityRecord>,
    pub relations: Vec<Relation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<LibraryDiagnostic>,
    pub generated_at: String,
    // A content-derived fingerprint of everything the memoized derived views
    // (analytics) depend on: the schema, every record's content `revision`, and
    // the relation graph. Unlike `generated_at` (a fresh wall-clock stamp on every
    // load) it is stable across a no-op reload, so a warm reload that re-reads the
    // same content keeps memoized views valid instead of discarding them. Derived
    // state, so `#[serde(skip)]` and only ever set by `Library::new`.
    #[serde(skip)]
    pub content_revision: String,
    // Lookup indices over `records`/`relations`, rebuilt by `reindex`. Private and
    // `#[serde(skip)]`: they are pure derived state (so construction must go
    // through `Library::new`), and they are not part of the serialized shape.
    #[serde(skip)]
    by_id: HashMap<String, usize>,
    #[serde(skip)]
    by_path: HashMap<String, usize>,
    #[serde(skip)]
    relations_by_source: HashMap<String, Vec<usize>>,
    #[serde(skip)]
    relations_by_target: HashMap<String, Vec<usize>>,
}

impl Library {
    /// Builds a library from its collections and derives the lookup indices.
    pub fn new(
        config: KizunaConfig,
        records: Vec<EntityRecord>,
        relations: Vec<Relation>,
        diagnostics: Vec<LibraryDiagnostic>,
        generated_at: String,
    ) -> Self {
        let content_revision = content_revision(&config, &records, &relations);
        let mut library = Self {
            config,
            records,
            relations,
            diagnostics,
            generated_at,
            content_revision,
            by_id: HashMap::new(),
            by_path: HashMap::new(),
            relations_by_source: HashMap::new(),
            relations_by_target: HashMap::new(),
        };
        library.reindex();
        library
    }

    /// Rebuilds the id/path/relation lookup indices from `records`/`relations`.
    /// Must be called whenever those collections are mutated in place (the
    /// constructor does this).
    pub fn reindex(&mut self) {
        self.by_id = self
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| (record.summary.id.clone(), index))
            .collect();
        self.by_path = self
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| (record.summary.path.clone(), index))
            .collect();
        let mut by_source: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_target: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, relation) in self.relations.iter().enumerate() {
            by_source
                .entry(relation.source_id.clone())
                .or_default()
                .push(index);
            if let Some(target_id) = &relation.target_id {
                by_target.entry(target_id.clone()).or_default().push(index);
            }
        }
        self.relations_by_source = by_source;
        self.relations_by_target = by_target;
    }

    /// The entity summaries, borrowed from the resident records. There is no
    /// separate stored `summaries` collection — it would just duplicate
    /// `records[*].summary` and double the resident summary memory.
    pub fn summaries(&self) -> impl Iterator<Item = &EntitySummary> {
        self.records.iter().map(|record| &record.summary)
    }

    /// O(1) lookup of a resident record by its entity id.
    pub fn record_by_id(&self, id: &str) -> Option<&EntityRecord> {
        self.by_id.get(id).map(|&index| &self.records[index])
    }

    /// O(1) lookup of a resident record by its vault-relative path.
    pub fn record_by_path(&self, path: &str) -> Option<&EntityRecord> {
        self.by_path.get(path).map(|&index| &self.records[index])
    }

    /// Relations whose `source_id` is `id`. Because every incoming link's `In`
    /// reflection is stored with the linked entity as its source, this yields the
    /// full set of relations an entity is the subject of (its outgoing edges plus
    /// the reflections of resolved incoming edges).
    pub fn relations_from(&self, id: &str) -> impl Iterator<Item = &Relation> {
        self.relations_by_source
            .get(id)
            .into_iter()
            .flatten()
            .map(move |&index| &self.relations[index])
    }

    /// Relation indices (in stored order, deduplicated) where `id` appears as the
    /// source or as a resolved target. Used to scan only an entity's local
    /// neighbourhood instead of the whole relation graph.
    pub fn relation_indices_touching(&self, id: &str) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .relations_by_source
            .get(id)
            .into_iter()
            .flatten()
            .chain(self.relations_by_target.get(id).into_iter().flatten())
            .copied()
            .collect();
        indices.sort_unstable();
        indices.dedup();
        indices
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDiagnostic {
    pub path: String,
    pub kind: String,
    pub message: String,
}

impl Entity {
    pub fn id(&self) -> &str {
        &self.summary.id
    }
}

#[cfg(test)]
mod log_config_tests {
    use super::*;

    fn config(
        daily_log: Option<DailyNoteLogDefaults>,
        type_log: Option<TypeLogConfig>,
    ) -> KizunaConfig {
        KizunaConfig {
            vault_root: String::new(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: Some(DailyNotesConfig {
                paths: Vec::new(),
                date_format: None,
                template: None,
                log: daily_log,
            }),
            tags: None,
            types: vec![EntityTypeConfig {
                id: "anime".to_string(),
                label: "Anime".to_string(),
                icon: None,
                path: "Anime".to_string(),
                external_priority: Vec::new(),
                filename: None,
                body_sections: Vec::new(),
                log: type_log,
                fields: Vec::new(),
            }],
        }
    }

    fn type_log(section: Option<&str>, line_format: Option<&str>) -> TypeLogConfig {
        TypeLogConfig {
            section: section.map(str::to_string),
            line_format: line_format.map(str::to_string),
        }
    }

    fn defaults(section: Option<&str>, line_format: Option<&str>) -> DailyNoteLogDefaults {
        DailyNoteLogDefaults {
            section: section.map(str::to_string),
            line_format: line_format.map(str::to_string),
        }
    }

    #[test]
    fn type_without_log_block_is_not_loggable() {
        // A `dailyNotes.log` default alone doesn't make a type loggable.
        let config = config(Some(defaults(Some("Log"), None)), None);
        assert!(config.resolve_log_config("anime").is_none());
        assert!(config.resolve_log_config("missing").is_none());
    }

    #[test]
    fn resolution_prefers_type_then_defaults_then_builtins() {
        // Line format from the type; section falls back to `dailyNotes.log`.
        let mixed = config(
            Some(defaults(Some("Journal"), None)),
            Some(type_log(None, Some("- [[{title}]] #Anime"))),
        );
        let resolved = mixed.resolve_log_config("anime").unwrap();
        assert_eq!(resolved.section, "Journal");
        assert_eq!(resolved.line_format, "- [[{title}]] #Anime");

        // The type override wins over the `dailyNotes.log` default.
        let both = config(
            Some(defaults(Some("Journal"), None)),
            Some(type_log(Some("Watched"), None)),
        );
        assert_eq!(both.resolve_log_config("anime").unwrap().section, "Watched");

        // Nothing set anywhere → the built-ins.
        let bare = config(None, Some(type_log(None, None)));
        let resolved = bare.resolve_log_config("anime").unwrap();
        assert_eq!(resolved.section, DEFAULT_LOG_SECTION);
        assert_eq!(resolved.line_format, DEFAULT_LOG_LINE_FORMAT);

        // Blank / whitespace-only overrides are treated as unset.
        let blank = config(
            Some(defaults(Some("  "), None)),
            Some(type_log(Some(""), None)),
        );
        assert_eq!(
            blank.resolve_log_config("anime").unwrap().section,
            DEFAULT_LOG_SECTION
        );
    }
}
