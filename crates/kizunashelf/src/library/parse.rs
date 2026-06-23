//! Parsing a single entity from its raw bytes (no I/O), the on-demand full-entity
//! load, and the schema-driven field-name lookups the summary derivation needs.

use super::frontmatter::{
    date_values, external_refs, extract_summary, first_string, parse_markdown, resolve_title,
    title_languages,
};
use super::relations::extract_body_links;
use crate::types::{
    DateRole, Entity, EntitySummary, EntityTypeConfig, FieldType, KizunaConfig, LibraryDiagnostic,
};
use crate::vfs::Vfs;
use anyhow::{Context, Result};
use std::hash::{Hash, Hasher};

/// Loads a single full [`Entity`] (including `body`/`raw`) from disk for the
/// given resident summary. This is the on-demand counterpart to the slim
/// [`crate::types::EntityRecord`] kept in the cache: detail views, mutations, and
/// asset writes call it when they need the complete document, instead of keeping
/// every body resident. Re-parsing from disk also returns the freshest content.
pub(crate) async fn load_entity(
    config: &KizunaConfig,
    vfs: &dyn Vfs,
    summary: &EntitySummary,
) -> Result<Entity> {
    let type_config = config
        .types
        .iter()
        .find(|type_config| type_config.id == summary.entity_type)
        .with_context(|| format!("unknown entity type {}", summary.entity_type))?;
    let bytes = vfs
        .read(&summary.path)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {}: {error}", summary.path))?;
    let mut entity = parse_entity(type_config, summary.path.clone(), bytes)?.entity;
    // `relation_count` is a library-wide derived value (from the relation graph),
    // not something a single file knows; carry it over from the resident summary.
    entity.summary.relation_count = summary.relation_count;
    Ok(entity)
}

pub(super) struct EntityReadResult {
    pub(super) entity: Entity,
    /// Body wikilink targets, extracted from the body during the parse (while it
    /// is in hand) so the resident [`crate::types::EntityRecord`] can carry them
    /// after the body is dropped. See [`crate::types::EntityRecord::body_links`].
    pub(super) body_links: Vec<String>,
    pub(super) diagnostics: Vec<LibraryDiagnostic>,
}

/// Parses one entity from its raw bytes — no I/O. The revision is derived from
/// the content (hash + length); a separate `metadata` call for the mtime is not
/// worth its per-file cost, and the content hash already detects edits. Body
/// wikilinks are extracted here too (while the body is in hand) and returned in
/// the result, so callers building a resident [`crate::types::EntityRecord`]
/// don't re-scan.
pub(super) fn parse_entity(
    type_config: &EntityTypeConfig,
    relative_path: String,
    bytes: Vec<u8>,
) -> Result<EntityReadResult> {
    let raw = String::from_utf8(bytes)
        .map_err(|error| anyhow::anyhow!("entity {relative_path} is not valid UTF-8: {error}"))?;
    let revision = file_revision(&raw);
    let parsed = parse_markdown(&raw);
    let entry = relative_path.rsplit('/').next().unwrap_or(&relative_path);
    let note_basename = entry.strip_suffix(".md").unwrap_or(entry).to_string();
    let titles = title_languages(&parsed.frontmatter, &note_basename, type_config);
    let title = resolve_title(&parsed.frontmatter, &titles, &note_basename, type_config);
    let entity_key = entity_key(&parsed.frontmatter, &note_basename, type_config);
    let diagnostics = parsed
        .diagnostics
        .iter()
        .map(|message| LibraryDiagnostic {
            path: relative_path.clone(),
            kind: "frontmatter".to_string(),
            message: message.clone(),
        })
        .collect();

    let summary = EntitySummary {
        id: format!("{}:{entity_key}", type_config.id),
        entity_type: type_config.id.clone(),
        type_label: type_config.label.clone(),
        title,
        titles,
        dates: date_values(&parsed.frontmatter, &date_field_names(type_config)),
        image: first_field_string_for_types(
            &parsed.frontmatter,
            type_config,
            &[FieldType::Image, FieldType::ImageList],
        ),
        summary: extract_summary(&parsed.body),
        path: relative_path,
        basename: note_basename,
        external_refs: external_refs(
            &parsed.frontmatter,
            &field_names(type_config, FieldType::ExternalRef),
        ),
        relation_count: 0,
    };

    Ok(EntityReadResult {
        body_links: extract_body_links(&parsed.body),
        entity: Entity {
            summary,
            revision,
            frontmatter: parsed.frontmatter,
            body: parsed.body,
            raw,
        },
        diagnostics,
    })
}

pub(crate) fn file_revision(raw: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hasher);
    format!("{:x}-{}", hasher.finish(), raw.len())
}

fn entity_key(
    frontmatter: &serde_json::Map<String, serde_json::Value>,
    basename: &str,
    type_config: &EntityTypeConfig,
) -> String {
    first_string(frontmatter, &field_names(type_config, FieldType::Id))
        .unwrap_or_else(|| basename.to_string())
}

fn first_field_string_for_types(
    frontmatter: &serde_json::Map<String, serde_json::Value>,
    type_config: &EntityTypeConfig,
    field_types: &[FieldType],
) -> Option<String> {
    first_string(
        frontmatter,
        &field_names_for_types(type_config, field_types),
    )
}

fn field_names(type_config: &EntityTypeConfig, field_type: FieldType) -> Vec<String> {
    field_names_for_types(type_config, &[field_type])
}

fn field_names_for_types(type_config: &EntityTypeConfig, field_types: &[FieldType]) -> Vec<String> {
    type_config
        .fields
        .iter()
        .filter(|field| field_types.contains(&field.field_type))
        .map(|field| field.field.clone())
        .collect()
}

fn date_field_names(type_config: &EntityTypeConfig) -> Vec<String> {
    type_config
        .fields
        .iter()
        .filter(|field| {
            matches!(field.field_type, FieldType::Date | FieldType::Season)
                && matches!(
                    field.date_role,
                    Some(DateRole::Planning | DateRole::Completed)
                )
        })
        .map(|field| field.field.clone())
        .collect()
}
