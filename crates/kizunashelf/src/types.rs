use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityFields {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub id: Vec<String>,
    #[serde(default, alias = "titles", skip_serializing_if = "BTreeMap::is_empty")]
    pub title_languages: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitle: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status: Vec<String>,
    #[serde(default, skip_serializing_if = "DateRoleConfig::is_empty")]
    pub date_roles: DateRoleConfig,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DateRoleConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planning: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed: Vec<String>,
}

impl DateRoleConfig {
    pub fn is_empty(&self) -> bool {
        self.planning.is_empty() && self.completed.is_empty()
    }

    pub fn fields(&self) -> Vec<String> {
        let mut fields = Vec::new();
        for field in self.planning.iter().chain(self.completed.iter()) {
            if !fields.contains(field) {
                fields.push(field.clone());
            }
        }
        fields
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityTypeConfig {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_title_language: Option<String>,
    pub fields: EntityFields,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HomeSectionConfig {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<SortDirection>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum StatusConfig {
    One(String),
    Many(Vec<String>),
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

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KizunaConfig {
    pub vault_root: String,
    pub taxonomy_root: String,
    #[serde(default)]
    pub relationship_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_concurrency: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<HomeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_notes: Option<DailyNotesConfig>,
    pub types: Vec<EntityTypeConfig>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntityDateValue {
    pub field: String,
    pub value: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
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
