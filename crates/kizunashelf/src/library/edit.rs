//! Surgically rebuilding the library after a single entity file was edited in
//! place, without re-reading the whole vault.

use super::parse::{parse_entity, EntityReadResult};
use super::read::{sort_records, unique_relation_count_by_id};
use super::relations::{build_entity_relations, dedupe_relations, DAILY_NOTE_RELATION_FIELD};
use crate::types::{EntityRecord, Library, LibraryDiagnostic};
use crate::vfs::Vfs;
use anyhow::Result;

/// Surgically rebuilds the library after a single entity file was edited in
/// place, without re-reading the whole vault from disk. Returns `Ok(None)` when
/// the edit is *structural* — the path is unknown, or the re-parse changed the
/// entity's id or path (a rename / id-field change) — because those can alter
/// wikilink resolution for arbitrary other entities and so need a full reload.
///
/// On success it re-reads only `edited_path`, swaps that one record, and replaces
/// exactly the relations the entity owns (its outgoing frontmatter/body links and
/// the `In` reflections of its resolved targets), leaving every other entity's
/// links intact. Derived `relation_count`s are recomputed from the updated graph.
/// The result is value-equivalent (up to ordering and `generated_at`) to a full
/// [`super::read_library`] — see the surgical-update tests.
pub(crate) async fn rebuild_for_edited_entity(
    base: &Library,
    vfs: &dyn Vfs,
    edited_path: &str,
) -> Result<Option<Library>> {
    let Some(index) = base.record_index_by_path(edited_path) else {
        return Ok(None);
    };
    let old_summary = &base.records[index].summary;
    let old_id = old_summary.id.clone();
    let Some(type_config) = base
        .config
        .types
        .iter()
        .find(|type_config| type_config.id == old_summary.entity_type)
    else {
        return Ok(None);
    };

    let bytes = match vfs.read(edited_path).await {
        Ok(bytes) => bytes,
        // The file vanished or is unreadable: let a full reload reconcile it.
        Err(_) => return Ok(None),
    };
    let EntityReadResult {
        entity,
        body_links,
        diagnostics: parse_diagnostics,
    } = parse_entity(type_config, edited_path.to_string(), bytes)?;
    let new_record = EntityRecord {
        body_links,
        summary: entity.summary,
        revision: entity.revision,
        frontmatter: entity.frontmatter,
    };

    // Structural change → full reload (it can re-resolve other entities' links).
    if new_record.summary.id != old_id || new_record.summary.path != edited_path {
        return Ok(None);
    }

    let mut records = base.records.clone();
    records[index] = new_record;

    // A title edit can change this record's sort position; re-establish the same
    // resident order a full load would produce.
    sort_records(&mut records);

    // Recompute the entity-to-entity relations in memory from the resident
    // records — each carries its own `body_links`, so this matches a full reload
    // without re-reading the other (unchanged) files. Daily-note relations resolve
    // daily-note wikilinks to entities by basename and store the entity's id/type;
    // an in-place edit changes none of those (the structural bailout above keeps
    // the id/path, and the type/basename come from the unchanged file
    // location/name), so the daily notes can't have gained or lost a link to this
    // entity. Reuse the resident daily-note relations from `base` instead of
    // re-reading every daily note from disk.
    let mut relations = build_entity_relations(&base.config, &records);
    relations.extend(
        base.relations
            .iter()
            .filter(|relation| relation.field == DAILY_NOTE_RELATION_FIELD)
            .cloned(),
    );
    let relations = dedupe_relations(relations);
    let relation_count_by_id = unique_relation_count_by_id(&relations);
    for record in &mut records {
        record.summary.relation_count = *relation_count_by_id.get(&record.summary.id).unwrap_or(&0);
    }

    // Replace this file's diagnostics with the fresh parse's.
    let mut diagnostics: Vec<LibraryDiagnostic> = base
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.path != edited_path)
        .cloned()
        .collect();
    diagnostics.extend(parse_diagnostics);

    Ok(Some(Library::new(
        base.config.clone(),
        records,
        relations,
        diagnostics,
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    )))
}
