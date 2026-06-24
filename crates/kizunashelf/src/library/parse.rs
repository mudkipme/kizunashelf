//! Parsing a single entity from its raw bytes (no I/O), the on-demand full-entity
//! load, and the schema-driven field-name lookups the summary derivation needs.

use super::frontmatter::{
    date_values, external_refs, extract_summary, extract_tags, first_list_value, first_string,
    parse_markdown, resolve_title, title_languages,
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
    let mut entity = parse_entity(
        type_config,
        config.tags_field(),
        summary.path.clone(),
        bytes,
    )?
    .entity;
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
    tags_field: &str,
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
        image: first_cover_value(
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
        tags: extract_tags(&parsed.frontmatter, tags_field),
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

/// The cover path: the first individual value of the earliest field (by schema
/// order) whose type is one of `field_types` (`Image`/`ImageList`). An `ImageList`
/// contributes its first element, not the whole list joined, so the result is a
/// single usable path.
fn first_cover_value(
    frontmatter: &serde_json::Map<String, serde_json::Value>,
    type_config: &EntityTypeConfig,
    field_types: &[FieldType],
) -> Option<String> {
    first_list_value(
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
                    Some(DateRole::Planning | DateRole::Started | DateRole::Completed)
                )
        })
        .map(|field| field.field.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    //! Tests for the schema-driven field selection that drives summary derivation.
    //! The invariant under test (see CLAUDE.md): meaning comes from a field's
    //! `FieldType`/role, never its name. These exercise the glue through
    //! `parse_entity` end-to-end rather than the private helpers in isolation.
    use super::*;
    use crate::types::{EntityDateValue, EntitySummary, FieldConfig, TitleRole};

    fn field(name: &str, field_type: FieldType) -> FieldConfig {
        FieldConfig {
            field: name.to_string(),
            field_type,
            display_name: None,
            title_language: None,
            title_role: None,
            external_fields: Vec::new(),
            enum_options: Vec::new(),
            total_progress_field: None,
            date_role: None,
            season_language: None,
            external_ref: None,
            external_types: Vec::new(),
            relation_type: None,
        }
    }

    fn dated(name: &str, field_type: FieldType, role: DateRole) -> FieldConfig {
        FieldConfig {
            date_role: Some(role),
            ..field(name, field_type)
        }
    }

    fn type_config(fields: Vec<FieldConfig>) -> EntityTypeConfig {
        EntityTypeConfig {
            id: "anime".to_string(),
            label: "Anime".to_string(),
            icon: None,
            path: "Anime".to_string(),
            external_priority: Vec::new(),
            filename: None,
            body_mappings: Vec::new(),
            fields,
        }
    }

    fn summary_of(type_config: &EntityTypeConfig, path: &str, raw: &str) -> EntitySummary {
        parse_entity(
            type_config,
            crate::types::DEFAULT_TAGS_FIELD,
            path.to_string(),
            raw.as_bytes().to_vec(),
        )
        .expect("parse")
        .entity
        .summary
    }

    fn date_fields(dates: &[EntityDateValue]) -> Vec<&str> {
        dates.iter().map(|date| date.field.as_str()).collect()
    }

    #[test]
    fn image_is_the_first_image_typed_field_not_a_field_named_cover() {
        // A `Text` field literally named "cover" must be ignored; the `Image`-typed
        // field is the cover, whatever it is called.
        let tc = type_config(vec![
            field("title", FieldType::Title),
            field("cover", FieldType::Text),
            field("poster", FieldType::Image),
        ]);
        let summary = summary_of(
            &tc,
            "Taxonomy/Anime/T.md",
            "---\ntitle: T\ncover: not-an-image\nposter: real.jpg\n---\n",
        );
        assert_eq!(summary.image.as_deref(), Some("real.jpg"));
    }

    #[test]
    fn image_from_an_image_list_is_the_first_element_not_the_joined_list() {
        // `ImageList` also qualifies, and selection follows schema field order. The
        // cover must be a single usable path — the list's first element, never the
        // whole list joined (which would break cover display and the broken-asset
        // check).
        let tc = type_config(vec![
            field("title", FieldType::Title),
            field("gallery", FieldType::ImageList),
            field("poster", FieldType::Image),
        ]);
        let summary = summary_of(
            &tc,
            "Taxonomy/Anime/T.md",
            "---\ntitle: T\ngallery:\n  - a.jpg\n  - b.jpg\nposter: p.jpg\n---\n",
        );
        // `gallery` precedes `poster` in the schema, so it is the cover source, and
        // its first element is the cover.
        assert_eq!(summary.image.as_deref(), Some("a.jpg"));
    }

    #[test]
    fn image_is_none_when_no_image_typed_field_exists() {
        let tc = type_config(vec![
            field("title", FieldType::Title),
            field("cover", FieldType::Text),
        ]);
        let summary = summary_of(
            &tc,
            "Taxonomy/Anime/T.md",
            "---\ntitle: T\ncover: looks-like-an-image.png\n---\n",
        );
        assert_eq!(summary.image, None);
    }

    #[test]
    fn dates_select_only_date_and_season_fields_with_a_planning_or_completed_role() {
        let tc = type_config(vec![
            field("title", FieldType::Title),
            dated("aired", FieldType::Date, DateRole::Completed),
            dated("planned", FieldType::Date, DateRole::Planning),
            // A `Date` field with no role is excluded even though it holds a date.
            field("touched", FieldType::Date),
            // A `Season` field with a role is included (seasons are date-like).
            dated("season", FieldType::Season, DateRole::Planning),
        ]);
        let summary = summary_of(
            &tc,
            "Taxonomy/Anime/T.md",
            "---\ntitle: T\naired: 2023-01-01\nplanned: 2024-06-01\ntouched: 2020-01-01\nseason: 2023 Spring\n---\n",
        );
        let fields = date_fields(&summary.dates);
        assert!(fields.contains(&"aired"), "completed date included");
        assert!(fields.contains(&"planned"), "planning date included");
        assert!(fields.contains(&"season"), "roled season included");
        assert!(
            !fields.contains(&"touched"),
            "a roleless date field is not a planning/completed date"
        );
    }

    #[test]
    fn external_refs_collect_only_external_ref_typed_fields() {
        let tc = type_config(vec![
            field("title", FieldType::Title),
            field("bangumi", FieldType::ExternalRef),
            // `notes` holds a value but isn't an external ref by type.
            field("notes", FieldType::Text),
        ]);
        let summary = summary_of(
            &tc,
            "Taxonomy/Anime/T.md",
            "---\ntitle: T\nbangumi: 12345\nnotes: hello\n---\n",
        );
        assert_eq!(
            summary.external_refs.get("bangumi"),
            Some(&"12345".to_string())
        );
        assert!(!summary.external_refs.contains_key("notes"));
    }

    #[test]
    fn id_uses_the_configured_id_field_and_falls_back_to_the_basename() {
        // An `Id`-typed field becomes the entity key...
        let with_id = type_config(vec![
            field("uid", FieldType::Id),
            field("title", FieldType::Title),
        ]);
        let summary = summary_of(
            &with_id,
            "Taxonomy/Anime/Renamed Later.md",
            "---\nuid: anime-001\ntitle: T\n---\n",
        );
        assert_eq!(summary.id, "anime:anime-001");

        // ...and without one, the filename basename is the key.
        let without_id = type_config(vec![field("title", FieldType::Title)]);
        let summary = summary_of(
            &without_id,
            "Taxonomy/Anime/My Note.md",
            "---\ntitle: T\n---\n",
        );
        assert_eq!(summary.id, "anime:My Note");
    }

    #[test]
    fn original_role_filename_makes_the_basename_the_canonical_title() {
        // The schema-driven title resolution wired through `parse_entity`: an
        // `original`-role filename means the basename is the fallback title even
        // when a title field carries another language.
        let mut tc = type_config(vec![field("name_ja", FieldType::Title)]);
        tc.filename = Some(crate::types::FilenameConfig {
            title_language: Some("zh".to_string()),
            title_role: Some(TitleRole::Original),
        });
        tc.fields[0].title_language = Some("ja".to_string());
        let summary = summary_of(&tc, "Taxonomy/Anime/星旅.md", "---\nname_ja: スター\n---\n");
        // Original-role filename → the basename is the canonical fallback title...
        assert_eq!(summary.title, "星旅");
        // ...while the per-language titles still expose both the filename language
        // (zh → basename) and the field language (ja → its value).
        assert_eq!(summary.titles.get("zh").map(String::as_str), Some("星旅"));
        assert_eq!(summary.titles.get("ja").map(String::as_str), Some("スター"));
    }
}
