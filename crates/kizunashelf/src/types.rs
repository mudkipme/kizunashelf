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
    // swift-openapi-generator renders proper `.checklist`/`.progress`/`.none` cases.
    //
    // Per-item checkboxes (`- [ ]` / `- [x]`) — tracks exactly which are watched.
    Checklist,
    // Count only; pairs with a `progress` field rather than per-item checkboxes.
    Progress,
    // No tracking — a plain ordered list.
    None,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DateRole {
    Planning,
    Started,
    Completed,
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

impl KizunaConfig {
    /// The type config whose `id` matches `type_id`, or `None` for an unknown
    /// type. Field meaning is schema-driven, so callers resolve a type's config
    /// here rather than reasoning about entity types directly.
    pub fn type_config(&self, type_id: &str) -> Option<&EntityTypeConfig> {
        self.types.iter().find(|item| item.id == type_id)
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
