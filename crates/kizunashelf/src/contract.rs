use crate::calendar::{
    ActivityResponse, CalendarDay, CalendarEntry, EntityDatesResponse, UpcomingResponse,
};
use crate::relations::Count;
use crate::types::{
    AppConfig, CanonicalStatus, Entity, EntitySummary, EntityTypeConfig, EpisodeTracking,
    HomeConfig, HomeSectionFilterConfig, KizunaConfig, LibraryDiagnostic, Relation, VaultConfig,
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

/// A ready-made starter vault schema offered during onboarding / vault creation.
/// The single source of truth for every frontend (web onboarding, desktop &
/// iOS create-vault) — see [`crate::templates`].
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VaultTemplate {
    pub id: String,
    pub label: String,
    pub config: VaultConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VaultTemplatesResponse {
    pub templates: Vec<VaultTemplate>,
}

/// A title-language option for the schema editor: an ISO 639-1 code and its
/// English display name. See [`crate::languages`].
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    pub code: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LanguagesResponse {
    pub languages: Vec<Language>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HomeSectionResponse {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub type_label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<HomeSectionFilterConfig>,
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
    /// Preferred episode-title language (ISO 639-1, the viewer's content language);
    /// providers that support translations use it.
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
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteEntityResponse {
    pub deleted_id: String,
    pub backup_path: String,
}

/// One row in the lists index. `description` is the prose above the first list;
/// `itemCount` and `ordered` summarize the list without its full contents.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListSummary {
    /// Stable identifier: the list file's basename (without `.md`).
    pub id: String,
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProviderSummary {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    /// Whether the provider supports free-text search (vs. URL/ID resolution only).
    pub search_supported: bool,
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

/// A search result: the raw candidate plus its schema-resolved field and body
/// previews for the searched entity type.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMatch {
    pub candidate: ExternalCandidate,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<MappedFieldValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_sections: Vec<MappedBodySection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSearchResponse {
    pub providers: Vec<ExternalProviderSummary>,
    pub items: Vec<ExternalMatch>,
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
    pub asset_download_job: AssetDownloadJob,
    pub asset_download_jobs: AssetDownloadJobListResponse,
    pub library: crate::types::Library,
}
