use crate::calendar::{
    ActivityResponse, CalendarDay, CalendarEntry, EntityDatesResponse, UpcomingResponse,
};
use crate::relations::Count;
use crate::types::{
    AppConfig, CanonicalStatus, Entity, EntitySummary, EntityTypeConfig, EpisodeTracking,
    HomeConfig, KizunaConfig, LibraryDiagnostic, Relation, VaultConfig,
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
    pub asset_download_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsConfigResponse {
    /// The inline app config (vault root + write mode), owned by the runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<AppConfig>,
    /// Path to the vault config file (`<vaultRoot>/KizunaShelf/config.yaml`).
    /// `None` until a vault root is configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_config_path: Option<String>,
    /// Whether the vault config file exists on disk.
    pub vault_exists: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<VaultConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettingsRequest {
    /// The vault config (the schema) to write. The vault root and write mode are
    /// owned by the runtime (env vars / the native vault switcher / `@AppStorage`),
    /// so they are never sent here. Omitting `vault` is a no-op.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<VaultConfig>,
}

/// The raw YAML text of the vault config (`KizunaShelf/config.yaml`), for the
/// plain-text "advanced" editor that bypasses the structured schema form.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RawConfigResponse {
    /// Path to the vault config file (`<vaultRoot>/KizunaShelf/config.yaml`).
    pub vault_config_path: String,
    /// Whether the vault config file exists on disk.
    pub vault_exists: bool,
    /// The raw YAML text of the config file, verbatim (empty when it doesn't
    /// exist yet).
    pub content: String,
}

/// Raw YAML text to validate and write verbatim to the vault config. The text is
/// strictly parsed first: type errors, missing required fields, invalid enum
/// values, and any unknown field are rejected (`400`) rather than dropped.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveRawConfigRequest {
    pub content: String,
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
    /// Absolute vault root, used by the desktop runtime to resolve local assets
    /// directly from disk (the web runtime uses the `/api/assets` route instead).
    pub vault_root: String,
    /// Vault-relative directory where downloaded assets are stored.
    pub asset_root: String,
    /// The resolved frontmatter key for the built-in tags field (configured via
    /// `tags.field`, defaulting to `tags`). Clients use this for the tag editor,
    /// the Library tag filter, and to hide tags from the generic "Details" view —
    /// so the name lives in one place instead of being hardcoded per client.
    pub tags_field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<HomeConfig>,
    pub types: Vec<EntityTypeConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProviderCatalogResponse {
    pub providers: Vec<ExternalProviderCatalogItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProviderCatalogItem {
    pub id: String,
    pub label: String,
    pub fields: Vec<ExternalProviderFieldOption>,
    pub types: Vec<ExternalProviderTypeOption>,
    pub default_external_types: Vec<String>,
    /// Credential fields this provider needs. Empty for keyless providers. The
    /// frontend and iOS render their credential editors from this list rather
    /// than hard-coding per-provider inputs — core is the single source.
    #[serde(default)]
    pub credentials: Vec<ExternalProviderCredentialField>,
    /// Whether the provider supports free-text search. `false` means it only
    /// resolves a pasted URL/ID (the query box should hint that to the user).
    pub search_supported: bool,
}

/// One credential input a provider requires (e.g. an API key). The `key` is the
/// secret-store key (also the `KIZUNASHELF_<UPPER_KEY>` env var on web/desktop).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProviderCredentialField {
    pub key: String,
    pub label: String,
    /// Render as a masked/password input and never echo back the stored value.
    pub secret: bool,
    /// A required credential gates the provider; an optional one only refines it.
    pub required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProviderFieldOption {
    pub field: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProviderTypeOption {
    pub value: String,
    pub label: String,
}

// --- Built-in type presets (onboarding / "add built-in type") ------------------
//
// A preset is a fully-wired [`EntityTypeConfig`] plus picker metadata, served by
// `GET /api/type-presets`. The actual config is materialized (with the chosen
// title language and relation wiring) only by `POST /api/type-presets/resolve`,
// so the picker payload stays small and language-agnostic. Single source of truth
// for every frontend — see [`crate::presets`].

/// One built-in type the picker can offer. Metadata only: the concrete
/// [`EntityTypeConfig`] comes from the resolve endpoint, since it depends on the
/// chosen title language and which other presets are being added alongside it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypePresetSummary {
    /// Stable preset id — also the default [`EntityTypeConfig::id`] and the key a
    /// client uses to match an already-added type (id equality). Never localized.
    pub id: String,
    pub category: TypePresetCategory,
    pub icon: String,
    /// English display label. Kept as data (keyed by `id`) so clients may localize
    /// by id later without a contract change; English is the fallback.
    pub label: String,
    /// One-line, plain-language description for the picker card.
    pub description: String,
    /// Providers this preset wires up, in priority order — rendered as chips.
    pub providers: Vec<TypePresetProvider>,
    /// Preset ids this type links to via relation fields (e.g. `["franchise"]`).
    /// Drives the picker's "pairs well with" suggestions entirely client-side.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_targets: Vec<String>,
}

/// A provider chip on a preset card (id + display label from the catalog).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypePresetProvider {
    pub id: String,
    pub label: String,
}

/// A preset category — the "what do you want to track?" grouping. A flat string
/// enum (not doc-commented variants) so swift-openapi-generator renders proper
/// Swift cases, matching `FieldType`/`DateRole`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum TypePresetCategory {
    Watch,
    Play,
    Read,
    Listen,
    People,
    Life,
}

/// A category with its English label, so the picker can render group headers
/// without hardcoding the set. Order in the response is the display order.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypePresetCategoryInfo {
    pub id: TypePresetCategory,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypePresetsResponse {
    pub presets: Vec<TypePresetSummary>,
    /// Categories in display order — the picker renders a group per entry.
    pub categories: Vec<TypePresetCategoryInfo>,
}

/// Request to materialize one or more presets into concrete types, merged against
/// the schema the client currently holds (empty during onboarding). Stateless: the
/// endpoint reads no vault, so onboarding and the settings editor call it the same
/// way.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolveTypePresetsRequest {
    /// The types already in the editor/vault. Used to wire relations, detect
    /// id/path collisions, and propose back-fills. Empty for a fresh vault.
    #[serde(default)]
    pub current_types: Vec<EntityTypeConfig>,
    /// Preset ids the user selected, in the order to add them.
    pub preset_ids: Vec<String>,
    /// ISO 639-1 title language to stamp onto title fields, filenames, and season
    /// language. Absent → the preset's language-neutral default (English).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_language: Option<String>,
}

/// The result of resolving presets: ready-to-insert types plus proposed edits to
/// existing types. The client shows any back-fills/collisions for confirmation and
/// merges the accepted result into its editor state — no merge logic is duplicated
/// per runtime.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolveTypePresetsResponse {
    /// New entity types to append, with relations to absent types stripped and
    /// relations to co-selected/existing types kept, and any id/path collisions
    /// already suffixed (see `collisions`).
    pub types: Vec<EntityTypeConfig>,
    /// Proposed relation fields to add to *existing* types so they can link to a
    /// newly added type (the "add one, then another later" case). Proposals only —
    /// the client applies the ones the user accepts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backfills: Vec<TypePresetBackfill>,
    /// Home sections (one per added type) the client can offer to add to Home.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub home_sections: Vec<crate::types::HomeSectionConfig>,
    /// Presets whose id/path collided with an existing type and were suffixed, so
    /// the UI can surface a rename.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collisions: Vec<TypePresetCollision>,
}

/// A proposed relation field to add to an existing type.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypePresetBackfill {
    /// The existing type to add the field to.
    pub type_id: String,
    pub type_label: String,
    /// The preset (newly added) that this relation targets.
    pub preset_id: String,
    pub preset_label: String,
    /// The relation field to add to `type_id`.
    pub field: crate::types::FieldConfig,
}

/// A preset whose default id/path collided with an existing type and was suffixed.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypePresetCollision {
    pub preset_id: String,
    pub requested_id: String,
    pub assigned_id: String,
    pub requested_path: String,
    pub assigned_path: String,
}

/// A title-language option for the schema editor: an ISO 639-1 `code`. The
/// display name is rendered client-side from the code with the platform's
/// localized language-name API (`Locale.localizedString` / `Intl.DisplayNames`),
/// so it follows the UI language instead of being a hardcoded English exonym.
/// See [`crate::languages`].
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    pub code: String,
}

/// A user-language preference option for the clients' single language picker:
/// the preference code (which, unlike a title language, may carry a script
/// subtag — `zh-Hans`/`zh-Hant`), its endonym label, and the bare title/content
/// language it maps to. Whether the UI is *translated* into a code is a
/// per-client build fact, not core data, so each client derives it from its own
/// shipped-locale set. See [`crate::languages`].
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserLanguage {
    pub code: String,
    pub label: String,
    pub title_language: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LanguagesResponse {
    pub languages: Vec<Language>,
    /// The language-picker options the preference is chosen from; every
    /// language-sensitive behavior (UI locale, title language, provider request
    /// language) derives from the picked entry.
    pub user_languages: Vec<UserLanguage>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HomeSectionResponse {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub type_label: String,
    /// The section's criteria, echoed from the config when defined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criteria: Option<SmartFilterGroup>,
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
    pub by_canonical_status: CanonicalStatusCounts,
    pub date_fields: Vec<String>,
}

/// Entity counts per canonical lifecycle status (schema-driven per type via
/// `statusValues`), scoped like the rest of the stats response. Entities with
/// no status field, no value, or an unmapped value are counted in none of them.
/// Fixed fields (not a map) for codegen-friendliness, mirroring [`StatusValues`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalStatusCounts {
    pub planning: usize,
    pub ongoing: usize,
    pub paused: usize,
    pub completed: usize,
    pub dropped: usize,
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
    /// The parsed episodes/tracks list, when the entity's type declares an
    /// `episodes` body section. `None` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episodes: Option<EntityEpisodes>,
    /// The entity body to render in the generic "Notes" view: identical to
    /// `entity.body` except sections that have a dedicated UI (the episodes
    /// section) are removed, so they aren't shown twice. `entity.body` stays the
    /// raw source of truth for editing.
    pub notes_body: String,
}

/// The parsed contents of an entity's episodes/tracks body section: groups
/// (season/disc sub-headings; ungrouped items land in one unlabeled group) and a
/// watched/total roll-up. Derived from the Markdown body, which stays the source
/// of truth.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityEpisodes {
    pub heading: String,
    pub tracking: EpisodeTracking,
    pub groups: Vec<EpisodeGroup>,
    pub total: usize,
    pub watched: usize,
    /// Free-text Markdown the user wrote *above* the list (between the heading and
    /// the first item/sub-heading); empty when there is none. Shown by the
    /// dedicated episodes UI since the body's generic render drops this section.
    pub description: String,
    /// Free-text Markdown *below* the last list item; empty when there is none.
    pub trailing: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeGroup {
    /// Sub-heading label (season/disc); empty for the ungrouped group.
    pub label: String,
    pub items: Vec<Episode>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    /// The episode/track number or identifier (e.g. `0`, `12.5`, `OVA1`); may be
    /// empty for an item with no parseable number.
    pub key: String,
    pub title: String,
    pub watched: bool,
    /// The air/release date (`YYYY-MM-DD`), rendered as an Obsidian Tasks
    /// `📅 YYYY-MM-DD` suffix on the list item. Absent when the provider has no
    /// per-item date (e.g. album tracks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The completion date (`YYYY-MM-DD`), rendered as an Obsidian Tasks
    /// `✅ YYYY-MM-DD` suffix. Stamped when the item is checked and cleared when
    /// unchecked; preserved across external syncs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<String>,
}

/// Episodes/tracks fetched from an external provider for an entity, mirroring the
/// provider's structure: a flat list comes back as one unlabeled group; a
/// season/disc-structured set as one group per season/disc.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEpisodes {
    pub groups: Vec<ProviderEpisodeGroup>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEpisodeGroup {
    /// Season/disc label (e.g. "Season 1"); empty for a flat provider.
    pub label: String,
    pub items: Vec<ProviderEpisodeItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEpisodeItem {
    pub key: String,
    pub title: String,
    /// The item's air/release date (`YYYY-MM-DD`) when the provider exposes one;
    /// absent otherwise (e.g. CD tracks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// A provider that can supply episodes for an entity (it supports episode import
/// and the entity has a matching external ref with a value).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeSource {
    pub provider: String,
    pub label: String,
}

/// Response of the episodes `fetch`: the providers that can supply episodes for
/// this entity, plus the structured episodes from the chosen one.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeSyncResponse {
    pub sources: Vec<EpisodeSource>,
    /// The provider these `groups` came from (empty when there are no sources).
    pub provider: String,
    pub groups: Vec<ProviderEpisodeGroup>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FetchEpisodesRequest {
    /// Provider id to fetch from; defaults to the entity's first episode source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Preferred episode-title language: the viewer's language preference, which
    /// may carry a script subtag (`zh-Hans`/`zh-Hant`). Providers that support
    /// translations honor it, normalizing to whatever their API distinguishes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Checks or unchecks a single episode/track, identified by its group label and
/// key — so toggling watched state sends just the changed item, not the whole
/// list. Checking stamps `date` as the completion date (`✅`); unchecking clears it.
/// Independent of daily-note logging (`/log`), which never touches episodes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToggleEpisodeRequest {
    pub revision: String,
    /// The season/disc group label of the episode (empty for the ungrouped list).
    #[serde(default)]
    pub group: String,
    /// The episode/track key within the group (its number/identifier). Used to
    /// locate the item when it uniquely identifies one; otherwise `index` wins.
    pub key: String,
    /// The item's 0-based position within its group — the fallback locator when
    /// `key` is empty or duplicated (titles can repeat too, so position is the
    /// stable tiebreaker; the revision guard keeps it valid).
    pub index: u32,
    pub watched: bool,
    /// The completion date (`YYYY-MM-DD`) to stamp when checking — **required**, the
    /// client's local date, so the `✅` matches the user's day rather than a UTC
    /// server clock. Ignored when unchecking. Re-checking a watched episode with a
    /// different `date` is how the completion date is edited.
    pub date: String,
}

/// Imports provider episodes (the chosen subset, already grouped/flattened by the
/// client) by merging them into the entity's existing episodes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportEpisodesRequest {
    pub revision: String,
    pub groups: Vec<EpisodeGroup>,
    /// When true, a matched existing item's title is overwritten with the incoming
    /// one (the client ticked it). Defaults false — matched titles are only filled
    /// when empty, never replacing a hand edit — so older clients keep that behavior.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityMutationResponse {
    pub entity: Entity,
    /// Present only for a rename that repointed `[[wikilinks]]` in other managed
    /// files (configured type folders, daily notes, and list pages): how many
    /// links changed and across how many files. Absent for non-rename updates and
    /// for renames that touched nothing, so a client can show "Updated N links"
    /// only when it happened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_links: Option<RenameLinkUpdate>,
}

/// Tally of the wikilink repointing a rename performed. See
/// [`EntityMutationResponse::updated_links`].
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameLinkUpdate {
    pub files: u32,
    pub links: u32,
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

/// Quick-add: create a library entity directly from an external search candidate
/// in one server-side step — schema-map its fields/body, derive a safe filename,
/// download covers (fail-safe), and import episodes (fail-safe). The client echoes
/// the candidate it picked from search; the core re-runs the schema mapping itself
/// and never trusts client-mapped values.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuickAddRequest {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub candidate: ExternalCandidate,
    /// Override the derived basename. When absent, the core derives it from the
    /// type's filename title language (falling back to the candidate title).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basename: Option<String>,
    /// The viewer's language preference (may carry a script subtag), used for the
    /// fail-safe episode import so episode titles arrive localized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// The episode-import outcome of a quick-add. Present only when the type declares
/// an episodes section and the candidate's provider supports episodes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuickAddEpisodeResult {
    pub provider: String,
    pub imported: usize,
    /// A fetch/import failure. The entity is still created; the client can retry
    /// from the detail page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuickAddResponse {
    pub entity: Entity,
    /// True when the candidate already resolved to a library entity (via an
    /// external ref) and nothing new was created — the returned entity is the
    /// existing one, so the client just navigates to it.
    pub already_existed: bool,
    /// True when a title collision forced a disambiguated basename (` (2023)`, …).
    pub basename_adjusted: bool,
    /// Per-cover download outcomes. A failed cover keeps its remote URL in
    /// frontmatter (fail-safe) and never fails the request.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cover: Vec<AssetDownloadItemResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episodes: Option<QuickAddEpisodeResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteEntityRequest {
    pub revision: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteEntityResponse {
    pub deleted_id: String,
    pub backup_path: String,
}

/// Which kind of list a summary row is: a hand-curated Markdown list or a
/// criteria-driven smart list (`.base` file). The two live in the same index
/// but are served by different detail endpoints.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ListKind {
    #[default]
    Static,
    Smart,
}

/// One row in the lists index. `description` is the prose above the first list;
/// `itemCount` and `ordered` summarize the list without its full contents.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListSummary {
    /// Stable identifier: the list file's basename (without its extension).
    /// Static and smart lists are separate id namespaces (different detail
    /// endpoints), so the same basename may appear once per kind.
    pub id: String,
    pub kind: ListKind,
    /// Display name (the basename).
    pub name: String,
    /// Vault-relative path of the Markdown file.
    pub path: String,
    pub description: String,
    /// Total item count across every section.
    pub item_count: usize,
    /// Number of named (`## heading`) sections; `0` for a flat list.
    pub section_count: usize,
    /// Whether the list contains the entity named by the `entity` query param.
    /// Only present when that param was supplied (drives the entity page's
    /// "manage lists" membership toggles); omitted otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<bool>,
}

/// The vault's full tag vocabulary (sorted, deduped) — backs tag autocomplete in
/// the editor and the Library tag filter.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagsResponse {
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListsResponse {
    pub items: Vec<ListSummary>,
}

/// How a list section renders: plain bullets, a numbered list, or a task list
/// with checkboxes. Per-section, since each Markdown list block is independent.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ListMarker {
    #[default]
    Unordered,
    Ordered,
    Todo,
}

/// One item of a list. `text` is the Markdown content after the list marker (and
/// after any task checkbox), preserving annotations; `target` is the first
/// wikilink target in it; `entity` is the resolved entity when the target matches
/// an indexed note (else `null`); and `checked` is the task state for items in a
/// `todo` section (`null` for non-task items).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<EntitySummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
}

/// One section of a list: an optional `## heading`, its marker style, and the
/// resolved items beneath it. `heading` is `null` for the ungrouped block of items
/// above the first heading (and for a flat list, which is a single such section).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    pub marker: ListMarker,
    pub items: Vec<ListItem>,
}

/// A list's full editable state: description, its sections (each with its own
/// heading, marker style, and resolved items), and the trailing Markdown.
/// `revision` guards writes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListDetail {
    pub id: String,
    pub name: String,
    pub path: String,
    pub description: String,
    pub sections: Vec<ListSection>,
    pub trailing: String,
    pub revision: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateListRequest {
    pub name: String,
}

/// One item as supplied by the client on a full list rewrite. Only the raw `text`
/// is sent; the server re-derives the target/entity on the next read.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListItemInput {
    pub text: String,
    /// Task state for an item in a `todo` section; `null`/omitted otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
}

/// One section as supplied by the client on a full list rewrite: an optional
/// heading (`null`/empty for the ungrouped block), its marker style, and its items
/// in their authoritative order.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListSectionInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    // Keep serde's backward-compatible default, but make OpenAPI clients send the
    // enum directly instead of generating a default+allOf wrapper.
    #[serde(default)]
    #[schemars(!default)]
    pub marker: ListMarker,
    #[serde(default)]
    pub items: Vec<ListItemInput>,
}

/// Full rewrite of a list (reorder within and across sections, section add/rename,
/// per-section marker toggle, description/trailing edits, item removal). The server
/// renders the whole body from these parts, so the section and item order is
/// authoritative.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateListRequest {
    pub revision: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub trailing: String,
    #[serde(default)]
    pub sections: Vec<ListSectionInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_to: Option<String>,
}

/// Adds a single entity to a list's first list block. Used cross-page (from an
/// entity detail page), so it carries no client revision — it reads the file
/// fresh, appends, and writes. The server computes the disambiguated wikilink.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddListItemRequest {
    pub entity_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteListResponse {
    pub deleted_id: String,
    pub backup_path: String,
}

// --- Smart lists -------------------------------------------------------------
//
// A smart list is an Obsidian Bases `.base` file; see `crate::smart_lists`.
// The criteria model here is deliberately depth-limited (a group of rules plus
// one level of subgroups — the iTunes shape) so the generated clients never
// see a recursive schema. Deeper nesting in hand-edited files still evaluates
// in the core; it surfaces here as an `unsupported` rule carrying the raw YAML,
// which round-trips verbatim on save.

/// How a smart-list filter group combines its members: every rule must match,
/// any rule may match, or no rule may match (Bases `and`/`or`/`not`).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SmartFilterConjunction {
    #[default]
    All,
    Any,
    #[serde(rename = "none")]
    NoneOf,
}

/// The editable smart-list rule shapes. `unsupported` is the read-mostly
/// escape hatch: a construct the editor can't model, carried as raw YAML.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SmartFilterRuleKind {
    Compare,
    Contains,
    StartsWith,
    EndsWith,
    IsEmpty,
    HasTag,
    LinksTo,
    InFolder,
    #[default]
    Unsupported,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SmartCompareOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SmartContainsMode {
    #[default]
    Any,
    All,
}

/// The calendar unit of a relative-date rule ("in the last N …").
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SmartDurationUnit {
    Days,
    Weeks,
    Months,
    Years,
}

/// A date relative to today: `amount`×`unit` into the past (default) or the
/// future (`future: true`) — "started in the last 90 days", "airing in the
/// next 2 weeks". On `file.mtime` rules it is relative to `now()` instead.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SmartRelativeDate {
    pub amount: u32,
    pub unit: SmartDurationUnit,
    #[serde(default)]
    pub future: bool,
}

/// One smart-list criterion. Which of the optional members apply depends on
/// `kind`:
///
/// - `compare` — `field`, `op`, and exactly one of `value`/`number`/`boolean`/
///   `date` (ISO `YYYY-MM-DD`)/`relative`.
/// - `contains` — `field`, `values` (with `mode`, default any-of).
/// - `startsWith` / `endsWith` — `field`, `values[0]`.
/// - `isEmpty` — `field` (`negated: true` reads as "has a value").
/// - `hasTag` — `values` (any listed tag).
/// - `linksTo` — `values[0]`: an entity basename/path the note must link to.
/// - `inFolder` — `values[0]`: a vault-relative folder.
/// - `unsupported` — `raw` only; preserved verbatim, ignored by evaluation.
///
/// `field` is a frontmatter key, or the special `file.name` / `file.mtime`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmartFilterRule {
    pub kind: SmartFilterRuleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Logical negation of the rule (supported on every kind but `compare`,
    /// where the operator itself expresses it).
    #[serde(default)]
    pub negated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<SmartCompareOp>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<SmartContainsMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boolean: Option<bool>,
    /// An absolute date literal, ISO `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative: Option<SmartRelativeDate>,
    /// `kind = unsupported`: the construct's raw YAML, round-tripped verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

/// A nested rule group — one level deep only (see the module note above).
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmartFilterSubgroup {
    #[serde(default)]
    #[schemars(!default)]
    pub conjunction: SmartFilterConjunction,
    #[serde(default)]
    pub rules: Vec<SmartFilterRule>,
}

/// A smart list's criteria: a conjunction over rules and (one level of)
/// subgroups. The type scope is *not* in here — it rides separately as
/// `scope` on the detail/requests and the server maintains its
/// `file.inFolder(...)` atom.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmartFilterGroup {
    #[serde(default)]
    #[schemars(!default)]
    pub conjunction: SmartFilterConjunction,
    #[serde(default)]
    pub rules: Vec<SmartFilterRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<SmartFilterSubgroup>,
}

/// The app layout of one smart-list view: `list` ⇔ a Bases `table` view,
/// `grid` ⇔ a Bases `cards` view.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SmartViewLayout {
    List,
    Grid,
}

/// One sort key of a smart-list view. `property` is a Bases property
/// reference: `note.<field>`, `file.name`, or `file.mtime`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmartSortSpec {
    pub property: String,
    pub direction: crate::types::SortDirection,
}

/// One view of a smart list — a named tab with its own layout, extra filters
/// (AND-ed with the global criteria), sort, and result limit.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmartListView {
    pub name: String,
    pub layout: SmartViewLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<SmartFilterGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<SmartSortSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Cards image property reference (grid views), e.g. `note.cover`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// A smart list's full editable state. `warnings` lists every construct in
/// the underlying `.base` file that the app ignores (unsupported filters,
/// views, sorts) — all of it preserved on save.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmartListDetail {
    pub id: String,
    pub name: String,
    pub path: String,
    /// The entity type this list is scoped to, when the file carries the
    /// recognized `file.inFolder(<type folder>)` idiom; `null` = all types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub filters: SmartFilterGroup,
    pub views: Vec<SmartListView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub revision: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSmartListRequest {
    pub name: String,
    /// Entity type id to scope the new list to (writes the `file.inFolder`
    /// atom and derives the grid view's cover image from the type's schema).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Full rewrite of a smart list's criteria and views. Constructs the editor
/// doesn't model (`unsupported` rules, non-table/cards views, unknown YAML
/// keys) are preserved server-side.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSmartListRequest {
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<SmartFilterGroup>,
    #[serde(default)]
    pub views: Vec<SmartListView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_to: Option<String>,
}

/// Evaluates an unsaved smart-list definition — the live preview while the
/// rule builder is open. Returns the standard entity page shape.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmartListPreviewRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<SmartFilterGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<SmartSortSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_language: Option<String>,
    /// Today's date (`YYYY-MM-DD`), the client's **local** date, so `today()`
    /// date criteria are judged against the user's day rather than the host's
    /// clock. Falls back to the host's local date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub today: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetDownloadRequest {
    pub revision: String,
    /// Restrict to these field names; when omitted, all image fields with remote
    /// URLs are downloaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AssetDownloadStatus {
    Downloaded,
    Skipped,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetDownloadItemResult {
    pub field: String,
    pub status: AssetDownloadStatus,
    /// Source URL that was downloaded (or attempted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// Vault-relative local path written on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Why the item was skipped or failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// True when an existing file referenced by another entity forced a
    /// disambiguated filename.
    #[serde(default, skip_serializing_if = "is_false")]
    pub conflict_resolved: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetDownloadResponse {
    pub entity: Entity,
    pub results: Vec<AssetDownloadItemResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetDownloadJobRequest {
    /// Restrict the batch to one entity type; when omitted, the whole library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AssetDownloadJobStatus {
    Queued,
    Running,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetDownloadJobError {
    pub entity_id: String,
    pub entity_title: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetDownloadJob {
    pub id: String,
    pub status: AssetDownloadJobStatus,
    /// Human-readable scope, e.g. `all` or `type:anime`.
    pub scope: String,
    pub total: u32,
    pub processed: u32,
    pub downloaded: u32,
    pub failed: u32,
    pub skipped: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<AssetDownloadJobError>,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetDownloadJobListResponse {
    pub jobs: Vec<AssetDownloadJob>,
}

/// One remote image awaiting download, returned by the `plan` endpoint so a host
/// (e.g. iOS) can fetch it itself — via a background `URLSession` — instead of
/// having the core download it through `reqwest`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetDownloadPlanItem {
    pub entity_id: String,
    pub entity_title: String,
    pub entity_type: String,
    pub field: String,
    /// For image-list fields, the element's URL (used to derive a stable
    /// filename); absent for single image fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_key: Option<String>,
    pub source_url: String,
    /// Entity revision at plan time (advisory; ingest re-reads the entity).
    pub revision: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetDownloadPlan {
    /// Human-readable scope, e.g. `all` or `type:anime`.
    pub scope: String,
    pub items: Vec<AssetDownloadPlanItem>,
}

/// Hands the core one externally-downloaded image to validate, place under the
/// vault, and write into a single frontmatter field.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetIngestRequest {
    pub field: String,
    /// Present iff `field` is an image-list field; the list element's URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_key: Option<String>,
    pub source_url: String,
    /// Absolute host path of the file the client already downloaded. The core
    /// reads it directly (like `indexCacheDir`) and deletes it afterward.
    pub source_path: String,
    /// Content-Type the host observed, used as an image sniff hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Advisory; ingest re-reads the entity rather than enforcing this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetIngestResponse {
    pub entity: Entity,
    pub result: AssetDownloadItemResult,
}

/// Places raw image bytes a web/desktop client picked from the device under the
/// entity's asset directory. Unlike download/ingest, the core only *places* the
/// file and returns its vault-relative path — it does not rewrite frontmatter.
/// The editor stages that path into its draft and persists it on the normal save.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetUploadRequest {
    pub field: String,
    /// Reserved for callers; the core derives a stable content-hash key for
    /// image-list elements regardless.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_key: Option<String>,
    /// The image bytes, base64-encoded (standard alphabet).
    pub data_base64: String,
    /// Client-observed Content-Type, used as an image-type/extension hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Original filename, used only as an extension fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetUploadResponse {
    /// Vault-relative path of the placed asset.
    pub path: String,
    /// Whether the destination collided with another entity's asset and was
    /// disambiguated.
    pub conflict_resolved: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProviderSummary {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    /// Whether the provider supports free-text search (vs. URL/ID resolution only).
    pub search_supported: bool,
    /// Why the provider is disabled (no mapping, missing credentials, …). Absent
    /// when the provider is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Set when this provider was queried but its request failed, so the UI can
    /// surface "search failed" instead of silently implying zero results. One
    /// failing provider never fails the whole search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalCandidate {
    pub provider: String,
    pub source_id: String,
    pub url: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub titles: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

/// One field's value resolved from a candidate against the entity-type schema.
/// The core does the schema-driven mapping (field-type normalization, list vs
/// scalar, `externalRef`→url, date→season) so every runtime applies identical
/// values; clients add only the human-facing label.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MappedFieldValue {
    pub field: String,
    /// The resolved value: a scalar for scalar fields, an array for list fields.
    pub value: Value,
    /// The external source (provider id) the value came from.
    pub source: String,
    /// The provider field the value was read from. Absent for an `externalRef`
    /// field (its value is the candidate URL, not a metadata field).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_field: Option<String>,
    /// Whether the resolved value carries content (drives default selection).
    pub has_value: bool,
}

/// A body-section heading filled from a candidate's metadata, pre-rendered to
/// Markdown. Clients merge it into the entity body (replace/append by heading).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MappedBodySection {
    /// Stable selection key (`heading:source:field`).
    pub key: String,
    pub heading: String,
    pub source: String,
    pub external_field: String,
    pub markdown: String,
    pub has_value: bool,
}

/// A library entity a candidate already resolves to, via one of the entity's
/// `externalRef` fields matching the candidate's provider + URL/id. Lets the UI
/// mark a result "in library" and link straight to it instead of re-adding.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExistingEntityRef {
    pub id: String,
    pub title: String,
}

/// A search result: the raw candidate plus its schema-resolved field and body
/// previews for a specific entity type.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMatch {
    /// The entity type this candidate was resolved against. Always set — with a
    /// concrete `type` it echoes that type; in cross-type (`all`) search it is the
    /// type whose schema produced these field/body previews.
    pub entity_type: String,
    pub candidate: ExternalCandidate,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<MappedFieldValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_sections: Vec<MappedBodySection>,
    /// The existing library entity this candidate already maps to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing: Option<ExistingEntityRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSearchResponse {
    pub providers: Vec<ExternalProviderSummary>,
    pub items: Vec<ExternalMatch>,
}

// ---- Batch import ----------------------------------------------------------
//
// Import a user's library from an external service (public profile) or a file
// export (CSV) into vault entities. A `plan` job fetches and resolves items
// against the same "in library" dedup quick capture uses; a `commit` job then
// creates the approved entities through the quick-add primitives (schema
// mapping, atomic write, episode import). See `docs/batch-import-plan.md`.

/// How an import source receives its input.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ImportInputKind {
    // Plain `//` comments (see `FieldType`): keep this a flat string enum so
    // swift-openapi-generator renders proper cases.
    //
    // A public profile fetched by username/id.
    Profile,
    // A file export pasted as text (CSV).
    Csv,
}

/// One selectable import source in the catalog.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceCatalogItem {
    pub id: String,
    pub label: String,
    pub input: ImportInputKind,
    /// Human label for the input field ("Bangumi username", "Yamtrack CSV export").
    pub input_label: String,
    /// Provider ids this source resolves items to.
    pub providers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<ExternalProviderCredentialField>,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceCatalogResponse {
    pub sources: Vec<ImportSourceCatalogItem>,
}

/// Input for a source fetch: a username (profile sources) or the pasted text of
/// a CSV export (CSV sources). CSV arrives as a string field, not multipart, so
/// the generated clients and the iOS in-process tunnel stay trivial.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csv_text: Option<String>,
    /// The viewer's language preference (request context, not user-entered),
    /// used to localize review-list display titles where the source distinguishes
    /// languages (e.g. Bangumi's `name`/`name_cn`). Mirrors the external-search
    /// `language` param. Optional; absent falls back to the source default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateImportJobRequest {
    pub source: String,
    pub input: ImportInput,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ImportJobStatus {
    Queued,
    Fetching,
    Planned,
    Committing,
    Completed,
    Cancelled,
    Failed,
}

/// A planned item's disposition against the resident library.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ImportPlanItemState {
    // Already in the library (loose "in library" match) — skipped, linked.
    Exists,
    // Resolvable to a target type — will be created on commit.
    WillCreate,
    // Needs manual resolution (`review_reason` says why) — not created by default.
    NeedsReview,
}

/// Why a planned item needs manual review.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ImportReviewReason {
    // The source exposed no supported provider id for this item.
    NoSupportedId,
    // No entity type maps the item's provider + bucket.
    NoTypeMatch,
    // A duplicate of another item in the same import (merged away).
    DuplicateInBatch,
    // The resolving provider is unavailable (missing credentials).
    ProviderUnavailable,
}

/// A source "bucket" (the provider's own media kind, e.g. Bangumi `anime`) and
/// the entity types it can map to. One candidate type is auto-selected; several
/// leave `selected_type` empty for the user to pick.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlanBucket {
    pub bucket: String,
    pub provider: String,
    pub candidate_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_type: Option<String>,
}

/// The user data resolved for a planned item, summarized for the review UI.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlanUserData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<CanonicalStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score10: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watched_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed: Option<String>,
    pub has_notes: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlanItem {
    pub index: u32,
    pub title: String,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_url: Option<String>,
    pub bucket: String,
    pub state: ImportPlanItemState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing: Option<ExistingEntityRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_reason: Option<ImportReviewReason>,
    pub user_data: ImportPlanUserData,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlan {
    pub buckets: Vec<ImportPlanBucket>,
    pub items: Vec<ImportPlanItem>,
}

/// An in-memory batch import job. Like the asset-download job it does not survive
/// a restart; re-running is safe because the dedup gate skips already-created
/// entities. `plan` is populated once `status` reaches `planned`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportJob {
    pub id: String,
    pub source: String,
    pub status: ImportJobStatus,
    pub total: u32,
    pub processed: u32,
    pub created: u32,
    pub skipped: u32,
    pub needs_review: u32,
    pub failed: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<ImportPlan>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportJobListResponse {
    pub jobs: Vec<ImportJob>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ImportDecisionAction {
    Create,
    Skip,
}

/// A per-item override applied at commit: force a create/skip, choose a target
/// type (for ambiguous buckets), or supply a hand-picked candidate for a
/// `needsReview` item.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportDecision {
    pub index: u32,
    pub action: ImportDecisionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_override: Option<ExternalCandidate>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportCommitOptions {
    pub import_user_data: bool,
    pub import_episodes: bool,
    pub mark_progress: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommitImportJobRequest {
    /// Bucket → chosen entity type id, for buckets with multiple candidate types.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub types: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<ImportDecision>,
    pub options: ImportCommitOptions,
}

/// Year-over-year activity: a year × month matrix of dated entities, filterable
/// by type. Backs the statistics heatmap + per-year totals. (Browsing dated
/// entities by period lives in the calendar's year/season views; this is the
/// at-a-glance cross-year comparison.)
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsActivity {
    /// Distinct entities with at least one parseable date.
    pub total_dated: usize,
    /// Type filter options (types that have dated entities), most first.
    pub types: Vec<AnalyticsActivityType>,
    /// Per-year rows, most recent year first.
    pub years: Vec<AnalyticsActivityYear>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsActivityType {
    pub id: String,
    pub label: String,
    pub total: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsActivityYear {
    pub year: i32,
    /// Dated occurrences in the year across all types.
    pub total: usize,
    /// Twelve monthly counts (Jan..Dec) across all types. Year-only dates count
    /// toward `total` but not toward any month bucket.
    pub months: Vec<u32>,
    /// Per-type monthly breakdown, for the type filter.
    pub by_type: Vec<AnalyticsActivityYearType>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsActivityYearType {
    pub type_id: String,
    pub total: usize,
    /// Twelve monthly counts (Jan..Dec) for this type within the year.
    pub months: Vec<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsDataQuality {
    pub missing_cover: Vec<EntitySummary>,
    pub missing_external_refs: Vec<EntitySummary>,
    pub isolated: Vec<EntitySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CleanupQueueSummary {
    /// Stable machine id (e.g. `missing-cover`); the client localizes the display
    /// label from this. The core intentionally emits no English queue text.
    pub id: String,
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
    pub isolated: Vec<EntitySummary>,
    /// Entities whose local cover path points to a file that no longer exists.
    pub broken_assets: Vec<EntitySummary>,
    pub unresolved_relations: Vec<CleanupUnresolvedRelation>,
    /// Entities whose canonical status contradicts a dated field: a `completed`
    /// entity with a completion/event date in the *future* (impossible), or a still
    /// -`planning` entity whose *event* date has already passed (a missed event you
    /// probably forgot to update). Date-relative, so computed against the client's
    /// local `today`.
    pub status_mismatch: Vec<EntitySummary>,
    /// Entities whose filename collides with another entity's after wikilink
    /// normalization (NFC, case-insensitive, last path segment). Such names are
    /// ambiguous targets for `[[wikilinks]]` — a bare `[[Name]]` resolves to only
    /// one of them — so they're surfaced for renaming. Grouped so colliding
    /// entries sit adjacent.
    pub duplicate_filenames: Vec<EntitySummary>,
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
    pub activity: AnalyticsActivity,
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
    pub episodes: usize,
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

/// Logs an activity to the day's daily note, and — for a `started`/`completed`
/// log — stamps the matching frontmatter date field. Episode watching is a
/// separate concern (`/episodes/watch`); logging never reads or writes the
/// episode list.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogActivityRequest {
    /// `add` (default) records the activity; `remove` is its exact inverse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<LogOp>,
    /// The entity's current revision — required whenever the log mutates the
    /// entity: a `started`/`completed` log that stamps a `dateRole` field or flips
    /// a mapped `enumRole: status` field. Ignored for a daily-note-only log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// The log's date (`YYYY-MM-DD`), **required** — the client supplies the user's
    /// local date, so the server never assumes "today" in UTC and past actions can
    /// be logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Whether this records progress, a start, or a completion. Drives the
    /// frontmatter date-stamp; does not affect the daily-note line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<LogKind>,
    /// Freeform text for the `{note}` token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LogKind {
    #[default]
    Progress,
    Started,
    Completed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LogOp {
    #[default]
    Add,
    Remove,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogActivityResponse {
    pub dry_run: bool,
    /// The daily note the line was (or would be) written to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_path: Option<String>,
    /// The note doesn't exist yet and would be created (from the template).
    pub note_will_be_created: bool,
    /// The heading the line is written under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// The rendered line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// `add`: the exact line is already in the section, so nothing was written.
    pub line_already_present: bool,
    /// `remove`: whether the exact line was found (and removed). `None` on `add`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_matched: Option<bool>,
    /// The date field this log stamped (`add`) or cleared (`remove`) on the entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub will_stamp_date: Option<StampedDate>,
    /// The status this log flips the entity to. Present only for an `add` of a
    /// `started`/`completed` log when the type has a mapped `enumRole: status`
    /// field and the flip is a promotion (never a demotion, never from `dropped`).
    /// Always `None` on `remove` — a status flip has no safe inverse, so removing a
    /// log deliberately leaves status untouched (see `docs/status-role-plan.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub will_flip_status: Option<FlippedStatus>,
    /// The refreshed entity detail when the log mutated the entity (a date stamp or
    /// a status flip). `None` on a daily-note-only log or a dry run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<EntityDetailResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StampedDate {
    pub field: String,
    pub value: String,
}

/// The status field, the value written, and the canonical it represents, for a
/// log that flips the entity's status.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FlippedStatus {
    pub field: String,
    pub value: String,
    pub canonical: CanonicalStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiSchemas {
    pub error: ErrorResponse,
    pub health: HealthResponse,
    pub settings_config: SettingsConfigResponse,
    pub save_settings_request: SaveSettingsRequest,
    pub raw_config: RawConfigResponse,
    pub save_raw_config_request: SaveRawConfigRequest,
    pub path_suggestions: PathSuggestionsResponse,
    pub kizuna_config: KizunaConfig,
    pub app_config: AppConfig,
    pub vault_config: VaultConfig,
    pub config: ConfigResponse,
    pub home: HomeResponse,
    pub stats: StatsResponse,
    pub analytics: AnalyticsResponse,
    pub external_provider_catalog: ExternalProviderCatalogResponse,
    pub entities: EntityListResponse,
    pub entity_detail: EntityDetailResponse,
    pub entity_dates: EntityDatesResponse,
    pub calendar: CalendarResponse,
    pub calendar_entry: CalendarEntry,
    pub activity: ActivityResponse,
    pub upcoming: UpcomingResponse,
    pub log_activity: LogActivityResponse,
    pub asset_download: AssetDownloadResponse,
    pub asset_upload: AssetUploadResponse,
    pub asset_download_job: AssetDownloadJob,
    pub asset_download_jobs: AssetDownloadJobListResponse,
    pub library: crate::types::Library,
}
