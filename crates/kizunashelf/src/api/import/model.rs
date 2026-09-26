//! The normalized unit of a batch import and the schema-driven helpers that turn
//! it into an entity. Source adapters produce [`ImportItem`]s (all source-specific
//! vocabulary already translated to canonicals); the plan/commit pipeline only
//! ever sees this shape, so no per-source logic leaks into orchestration.

use crate::api::external::provider_for_external_ref;
use crate::contract::{ExternalCandidate, ImportPlanUserData};
use crate::status::status_field;
use crate::types::{CanonicalStatus, DateRole, EntityTypeConfig, FieldType, KizunaConfig};
use serde_json::Map;
use serde_json::{Number, Value};
use std::collections::BTreeMap;

/// One external reference an item carries, in preference order. `provider` is a
/// registry id (e.g. `myanimelist`); `url` is the canonical URL an `externalRef`
/// field stores.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct ProviderRef {
    pub provider: String,
    pub id: String,
    pub url: String,
}

/// The per-item user data an import carries, already normalized: status to a
/// canonical, score to a 0–10 scale, dates to ISO strings. The target type's
/// schema roles decide which fields these land on (see [`apply_user_data`]).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct ImportUserData {
    pub status: Option<CanonicalStatus>,
    pub score10: Option<f64>,
    pub watched_count: Option<u32>,
    pub started: Option<String>,
    pub completed: Option<String>,
    pub notes: Option<String>,
}

/// A normalized import unit. `candidate` may be partial (or absent) — commit
/// detail-fetches only when the target type maps a metadata key it lacks.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct ImportItem {
    pub refs: Vec<ProviderRef>,
    /// The source's own media kind (e.g. Bangumi `anime`), matched against a
    /// type's `externalTypes` — never a KizunaShelf entity-type id.
    pub bucket: String,
    pub title: String,
    pub titles: BTreeMap<String, String>,
    pub candidate: Option<ExternalCandidate>,
    pub user: ImportUserData,
}

impl ImportItem {
    pub(super) fn primary_ref(&self) -> Option<&ProviderRef> {
        self.refs.first()
    }

    /// The ref this item resolves to against a vault's schema: the first ref
    /// (they're in source preference order) whose provider some type maps for
    /// this bucket, else the primary ref — so an item none of whose providers
    /// are mapped still plans (and reads in review) under its preferred one.
    pub(super) fn ref_for_config(&self, config: &KizunaConfig) -> Option<&ProviderRef> {
        self.refs
            .iter()
            .find(|reference| {
                !candidate_types_for(config, &reference.provider, &self.bucket).is_empty()
            })
            .or_else(|| self.primary_ref())
    }

    /// The in-batch dedup key: the primary ref's provider + normalized id. `None`
    /// for an item with no ref (it can't be deduped by id).
    pub(super) fn dedup_key(&self) -> Option<(String, String)> {
        self.primary_ref().map(|reference| {
            (
                reference.provider.to_lowercase(),
                normalize_id(&reference.id),
            )
        })
    }

    /// Folds a duplicate (same dedup key) into this item: keep this item's refs
    /// and candidate, but fill any missing user-data field from `other` and keep
    /// the further-along status.
    pub(super) fn merge(&mut self, other: ImportItem) {
        self.user.status = merge_status(self.user.status, other.user.status);
        self.user.score10 = self.user.score10.or(other.user.score10);
        self.user.watched_count = self.user.watched_count.max(other.user.watched_count);
        self.user.started = self.user.started.take().or(other.user.started);
        self.user.completed = self.user.completed.take().or(other.user.completed);
        self.user.notes = self.user.notes.take().or(other.user.notes);
        if self.candidate.is_none() {
            self.candidate = other.candidate;
        }
        for (language, title) in other.titles {
            self.titles.entry(language).or_insert(title);
        }
    }

    pub(super) fn plan_user_data(&self) -> ImportPlanUserData {
        ImportPlanUserData {
            status: self.user.status,
            score10: self.user.score10,
            watched_count: self.user.watched_count,
            started: self.user.started.clone(),
            completed: self.user.completed.clone(),
            has_notes: self
                .user
                .notes
                .as_deref()
                .is_some_and(|notes| !notes.trim().is_empty()),
        }
    }

    /// Backfills the conventional `title` metadata key on the item's candidate.
    /// Every provider exposes its display title as `title` metadata; import-source
    /// candidates are partial, so without this a schema's external-field title
    /// mapping would trigger a needless detail fetch (or stay empty). The title's
    /// language is unknown, so it is deliberately NOT tagged into `titles`.
    pub(super) fn fill_title_metadata(&mut self) {
        let Some(candidate) = self.candidate.as_mut() else {
            return;
        };
        if candidate.title.trim().is_empty() {
            return;
        }
        candidate
            .metadata
            .entry("title".to_string())
            .or_insert_with(|| Value::String(candidate.title.clone()));
    }

    /// A synthetic candidate for the "in library" lookup — enough of an
    /// [`ExternalCandidate`] for `lookup_existing` to match by ref or title.
    /// `reference` is the ref the item resolved to ([`Self::ref_for_config`]).
    pub(super) fn lookup_candidate(&self, reference: Option<&ProviderRef>) -> ExternalCandidate {
        ExternalCandidate {
            needs_detail: false,
            provider: reference.map(|r| r.provider.clone()).unwrap_or_default(),
            source_id: reference.map(|r| r.id.clone()).unwrap_or_default(),
            url: reference.map(|r| r.url.clone()).unwrap_or_default(),
            title: self.title.clone(),
            original_title: None,
            brief: None,
            cover_url: None,
            titles: self.titles.clone(),
            metadata: Map::new(),
        }
    }
}

fn normalize_id(id: &str) -> String {
    id.trim().to_lowercase()
}

/// Merge two statuses, keeping the further-along one (Completed > Ongoing >
/// Planning/Paused > Dropped). Used when the same work appears in several source
/// lists (e.g. Trakt history + ratings).
fn merge_status(a: Option<CanonicalStatus>, b: Option<CanonicalStatus>) -> Option<CanonicalStatus> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if merge_rank(b) > merge_rank(a) { b } else { a }),
        (a, b) => a.or(b),
    }
}

fn merge_rank(status: CanonicalStatus) -> i8 {
    match status {
        CanonicalStatus::Dropped => 0,
        CanonicalStatus::Planning => 1,
        CanonicalStatus::Paused => 2,
        CanonicalStatus::Ongoing => 3,
        CanonicalStatus::Completed => 4,
    }
}

/// The entity types that can receive an item of `provider` + `bucket`: any type
/// with an `externalRef` field for `provider` whose `externalTypes` intersect the
/// bucket (an empty `externalTypes` matches any bucket). This is the schema-driven
/// bucket→type match — a provider is never mapped to a type by name.
pub(super) fn candidate_types_for(
    config: &KizunaConfig,
    provider: &str,
    bucket: &str,
) -> Vec<String> {
    let mut types = Vec::new();
    for type_config in &config.types {
        let matches = type_config.fields.iter().any(|field| {
            field.field_type == FieldType::ExternalRef
                && field
                    .external_ref
                    .as_deref()
                    .and_then(provider_for_external_ref)
                    == Some(provider)
                && (field.external_types.is_empty()
                    || field
                        .external_types
                        .iter()
                        .any(|external_type| external_type.trim().eq_ignore_ascii_case(bucket)))
        });
        if matches && !types.contains(&type_config.id) {
            types.push(type_config.id.clone());
        }
    }
    types
}

/// Whether commit must detail-fetch a provider before mapping: true when the
/// candidate is absent, or the target type maps a metadata key the candidate
/// doesn't carry. The `externalRef` URL and title/cover fallbacks are always
/// available, so only external-field metadata keys are checked.
pub(super) fn needs_detail_fetch(
    candidate: Option<&ExternalCandidate>,
    type_config: &EntityTypeConfig,
    provider: &str,
) -> bool {
    let Some(candidate) = candidate else {
        return true;
    };
    for key in required_metadata_keys(type_config, provider) {
        match candidate.metadata.get(&key) {
            Some(value) if !value.is_null() => {}
            _ => return true,
        }
    }
    false
}

fn required_metadata_keys(type_config: &EntityTypeConfig, provider: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let field_mappings = type_config
        .fields
        .iter()
        .flat_map(|field| field.external_fields.iter());
    let section_mappings = type_config
        .body_sections
        .iter()
        .flat_map(|section| section.external_fields.iter());
    for mapping in field_mappings.chain(section_mappings) {
        if mapping.source.trim().eq_ignore_ascii_case(provider)
            && !keys.iter().any(|key| key == &mapping.field)
        {
            keys.push(mapping.field.clone());
        }
    }
    keys
}

/// Applies an item's user data onto a freshly-mapped entity document, driven
/// entirely by schema roles: status via `enumRole: status` + `statusValues`,
/// score onto the first `rating` field, started/completed onto the matching
/// `dateRole` date fields, and notes as an unmanaged `## Notes` body section.
/// User data wins over provider-mapped values (a personal score is not the
/// provider's aggregate). Unmapped canonicals / absent roles are skipped.
pub(super) fn apply_user_data(
    frontmatter: &mut Map<String, Value>,
    body: &mut String,
    type_config: &EntityTypeConfig,
    user: &ImportUserData,
) {
    if let Some(canonical) = user.status {
        if let Some(field) = status_field(type_config) {
            if let Some(value) = field
                .status_values
                .as_ref()
                .and_then(|values| values.write_value(canonical))
            {
                frontmatter.insert(field.field.clone(), Value::String(value.to_string()));
            }
        }
    }

    if let Some(score) = user.score10 {
        if let Some(field) = type_config
            .fields
            .iter()
            .find(|field| field.field_type == FieldType::Rating)
        {
            if let Some(value) = crate::ratings::from_ten(score, field).and_then(number_value) {
                frontmatter.insert(field.field.clone(), value);
            }
        }
    }

    if let Some(started) = user
        .started
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(field) = date_field(type_config, DateRole::Started) {
            frontmatter.insert(field.field.clone(), Value::String(started.to_string()));
        }
    }
    if let Some(completed) = user
        .completed
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(field) = date_field(type_config, DateRole::Completed) {
            frontmatter.insert(field.field.clone(), Value::String(completed.to_string()));
        }
    }

    if let Some(notes) = user
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let section = format!("## Notes\n\n{notes}\n");
        if body.trim().is_empty() {
            *body = section;
        } else {
            *body = format!("{}\n\n{section}", body.trim_end());
        }
    }
}

fn date_field(
    type_config: &EntityTypeConfig,
    role: DateRole,
) -> Option<&crate::types::FieldConfig> {
    type_config
        .fields
        .iter()
        .find(|field| field.field_type == FieldType::Date && field.date_role == Some(role))
}

/// A JSON number for a score: an integer when the value is whole (so a 8.0 writes
/// `8`, not `8.0`), else a float. `None` for a non-finite value.
fn number_value(score: f64) -> Option<Value> {
    if score.fract() == 0.0 && score.abs() < i64::MAX as f64 {
        return Some(Value::Number((score as i64).into()));
    }
    Number::from_f64(score).map(Value::Number)
}
