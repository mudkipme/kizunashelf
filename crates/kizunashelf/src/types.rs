use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

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
    pub body_mappings: Vec<ExternalBodyMapping>,
    #[serde(default)]
    pub fields: Vec<FieldConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FilenameConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_language: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub default_title: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_title: Option<bool>,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalBodyMapping {
    pub source: String,
    pub field: String,
    pub heading: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DateRole {
    Planning,
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

fn is_false(value: &bool) -> bool {
    !*value
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

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyNotesConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet_max_length: Option<u32>,
}

/// App-level configuration. Describes how *this machine* runs KizunaShelf and
/// where the vault lives on disk. Stored in the local app config file
/// (`~/.config/kizunashelf.yaml` and friends) and never synced with the vault.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub vault_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_writable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_concurrency: Option<u32>,
}

/// Vault-level configuration. Describes the vault's content schema (taxonomy,
/// assets, entity types, home dashboard, daily notes). Stored inside the vault
/// at `<vaultRoot>/.kizunashelf/config.yaml` so it travels with the vault and is
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
    pub types: Vec<EntityTypeConfig>,
}

/// Merged runtime view of the app and vault config. This is the shape consumed
/// throughout the library, relations, calendar, and API handlers. It is built
/// from [`AppConfig`] + [`VaultConfig`] at load time and split back into them
/// when persisting.
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
    pub read_concurrency: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<HomeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_notes: Option<DailyNotesConfig>,
    pub types: Vec<EntityTypeConfig>,
}

pub const DEFAULT_ASSET_ROOT: &str = "Assets";

impl KizunaConfig {
    /// Vault-relative directory where downloaded assets are stored.
    pub fn resolved_asset_root(&self) -> &str {
        self.asset_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_ASSET_ROOT)
    }

    /// Builds the merged runtime config from its on-disk parts.
    pub fn from_parts(app: AppConfig, vault: VaultConfig) -> Self {
        Self {
            vault_root: app.vault_root,
            content_writable: app.content_writable,
            read_concurrency: app.read_concurrency,
            taxonomy_root: vault.taxonomy_root,
            asset_root: vault.asset_root,
            home: vault.home,
            daily_notes: vault.daily_notes,
            types: vault.types,
        }
    }

    /// Splits the merged config back into the app and vault parts for persisting.
    pub fn into_parts(self) -> (AppConfig, VaultConfig) {
        (
            AppConfig {
                vault_root: self.vault_root,
                content_writable: self.content_writable,
                read_concurrency: self.read_concurrency,
            },
            VaultConfig {
                taxonomy_root: self.taxonomy_root,
                asset_root: self.asset_root,
                home: self.home,
                daily_notes: self.daily_notes,
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
    pub relation_count: u32,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RelationDirection {
    Out,
    In,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub config: KizunaConfig,
    pub entities: Vec<Entity>,
    pub summaries: Vec<EntitySummary>,
    pub relations: Vec<Relation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<LibraryDiagnostic>,
    pub generated_at: String,
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
