use crate::daily_notes::{
    daily_note_candidates, normalize_wikilink_target, read_daily_note_contents, strip_frontmatter,
    DAILY_NOTE_READ_CHUNK,
};
use crate::types::{EntityRecord, FieldType, KizunaConfig, Relation, RelationDirection};
use crate::vfs::Vfs;
use anyhow::Result;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

use super::frontmatter::{fence_regex, strip_wikilink, wikilink_regex};
use super::index_cache::{fingerprint_hit, CachedDailyNote};
use super::read::cache_key;

/// The `field` value marking a relation that originates from a daily note (the
/// only relation kind that requires reading files outside the resident records).
pub(super) const DAILY_NOTE_RELATION_FIELD: &str = "daily-note";

/// The shared entity-to-entity relation pass: each record's owned outgoing edges
/// against `by_basename`. Factored out of [`build_relations`] so the loop has a
/// single home.
fn entity_outgoing(
    config: &KizunaConfig,
    records: &[EntityRecord],
    by_basename: &HashMap<String, Vec<&EntityRecord>>,
) -> Vec<Relation> {
    let mut relations = Vec::new();
    for record in records {
        relations.extend(build_record_outgoing(config, record, by_basename));
    }
    relations
}

/// Builds the full relation graph from resident data only: the entity-to-entity
/// relations plus the pre-gathered daily-note links (see [`read_daily_note_links`],
/// which does the I/O). Pure — no file reads — so the daily-note reading can be
/// cached in the load pass like entities.
pub(super) fn build_relations(
    config: &KizunaConfig,
    records: &[EntityRecord],
    daily_note_links: &[(String, Vec<String>)],
) -> Vec<Relation> {
    let by_basename = normalized_entity_basename_index(records);
    let mut relations = entity_outgoing(config, records, &by_basename);

    // Resolve each daily note's wikilinks against the current entity set.
    for (source_id, links) in daily_note_links {
        for target_title in links {
            let Some(target) = find_target_for_wikilink(target_title, &by_basename) else {
                continue;
            };
            relations.push(Relation {
                source_id: source_id.clone(),
                target_id: Some(target.summary.id.clone()),
                target_title: target_title.clone(),
                target_type: Some(target.summary.entity_type.clone()),
                field: DAILY_NOTE_RELATION_FIELD.to_string(),
                direction: RelationDirection::Out,
            });
        }
    }

    dedupe_relations(relations)
}

/// Builds one record's outgoing relations — its frontmatter relation fields and
/// its body wikilinks ([`EntityRecord::body_links`]) — plus the `In` reflection
/// on each *resolved* frontmatter target.
pub(super) fn build_record_outgoing(
    config: &KizunaConfig,
    record: &EntityRecord,
    by_basename: &HashMap<String, Vec<&EntityRecord>>,
) -> Vec<Relation> {
    let mut relations = Vec::new();
    for relation_field in relation_fields(config, &record.summary.entity_type) {
        for target_title in relation_values(record.frontmatter.get(&relation_field.field)) {
            let target = find_target(
                &target_title,
                relation_field.relation_type.as_deref(),
                by_basename,
            );
            relations.push(Relation {
                source_id: record.summary.id.clone(),
                target_id: target.map(|target| target.summary.id.clone()),
                target_title: target_title.clone(),
                target_type: target.map(|target| target.summary.entity_type.clone()),
                field: relation_field.field.clone(),
                direction: RelationDirection::Out,
            });
            if let Some(target) = target {
                relations.push(Relation {
                    source_id: target.summary.id.clone(),
                    target_id: Some(record.summary.id.clone()),
                    target_title: record.summary.title.clone(),
                    target_type: Some(record.summary.entity_type.clone()),
                    field: relation_field.field.clone(),
                    direction: RelationDirection::In,
                });
            }
        }
    }

    for target_title in &record.body_links {
        let Some(target) = find_target(target_title, None, by_basename) else {
            continue;
        };
        if target.summary.id == record.summary.id {
            continue;
        }
        relations.push(Relation {
            source_id: record.summary.id.clone(),
            target_id: Some(target.summary.id.clone()),
            target_title: target_title.clone(),
            target_type: Some(target.summary.entity_type.clone()),
            field: "body".to_string(),
            direction: RelationDirection::Out,
        });
    }

    relations
}

/// Extracts the body wikilink targets for one entity. Called during the load
/// pass (while the body is still in hand) so [`build_relations`] can resolve them
/// later without the body resident. See [`body_wikilinks`].
pub(super) fn extract_body_links(body: &str) -> Vec<String> {
    body_wikilinks(body)
}

#[derive(Clone)]
struct RelationFieldConfig {
    field: String,
    relation_type: Option<String>,
}

fn relation_fields(config: &KizunaConfig, entity_type: &str) -> Vec<RelationFieldConfig> {
    let type_config = config.type_config(entity_type);
    let mut fields = Vec::new();
    for field in type_config.into_iter().flat_map(|item| {
        item.fields
            .iter()
            .filter(|field| field.field_type == FieldType::Relation)
    }) {
        if !fields
            .iter()
            .any(|item: &RelationFieldConfig| item.field == field.field)
        {
            fields.push(RelationFieldConfig {
                field: field.field.clone(),
                relation_type: field.relation_type.clone(),
            });
        }
    }
    fields
}

/// The daily-note wikilinks gathered for a load: the `(source_id, link targets)`
/// pairs to resolve into relations, the per-note cache map to persist (current
/// notes only, so deletions drop out), and whether anything was re-read.
pub(super) struct DailyNoteRead {
    pub(super) links: Vec<(String, Vec<String>)>,
    pub(super) cache: BTreeMap<String, CachedDailyNote>,
    pub(super) dirty: bool,
}

/// Gathers each daily note's wikilink targets, reusing `loaded` cache entries for
/// notes whose `(size, mtime)` fingerprint is unchanged and reading + extracting
/// only the changed/new ones. Only the link targets are cached — never the body —
/// so a cold start over a vault with thousands of daily notes skips re-reading
/// every unchanged note. Discovery (`read_dir`, which carries the fingerprint) is
/// always done; the saved cost is the per-note content read.
pub(super) async fn read_daily_note_links(
    config: &KizunaConfig,
    vfs: &dyn Vfs,
    loaded: &HashMap<String, CachedDailyNote>,
) -> Result<DailyNoteRead> {
    let candidates = daily_note_candidates(config, vfs, None, None, false).await?;

    let mut links: Vec<(String, Vec<String>)> = Vec::new();
    let mut cache: BTreeMap<String, CachedDailyNote> = BTreeMap::new();
    let mut miss_paths: Vec<String> = Vec::new();
    // path -> (source_id, len, mtime) for the notes we must read.
    let mut miss_meta: HashMap<String, (String, u64, u128)> = HashMap::new();

    for note in candidates {
        let source_id = format!("daily-note:{}:{}", note.source_label, note.relative_path);
        let key = cache_key(&note.relative_path);
        if let Some(cached) = loaded.get(&key) {
            if fingerprint_hit(
                cached.len,
                cached.modified_unix_nanos,
                note.len,
                note.modified_unix_nanos,
            ) {
                links.push((source_id, cached.links.clone()));
                cache.insert(key, cached.clone());
                continue;
            }
        }
        miss_meta.insert(
            note.relative_path.clone(),
            (source_id, note.len, note.modified_unix_nanos),
        );
        miss_paths.push(note.relative_path);
    }

    let misses = miss_paths.len();
    // Read changed/new notes in bounded chunks so a vault with very many/large
    // daily notes can't OOM here.
    for chunk in miss_paths.chunks(DAILY_NOTE_READ_CHUNK) {
        for (relative_path, contents) in read_daily_note_contents(vfs, chunk).await? {
            let Some((source_id, len, modified_unix_nanos)) = miss_meta.remove(&relative_path)
            else {
                continue;
            };
            let note_links = daily_note_wikilinks(&contents);
            cache.insert(
                cache_key(&relative_path),
                CachedDailyNote {
                    len,
                    modified_unix_nanos,
                    links: note_links.clone(),
                },
            );
            links.push((source_id, note_links));
        }
    }

    // Rewrite the cache when anything was re-read, or when the note set shrank (a
    // deletion the `loaded` map still held).
    let dirty = misses > 0 || cache.len() != loaded.len();
    Ok(DailyNoteRead {
        links,
        cache,
        dirty,
    })
}

fn daily_note_wikilinks(raw: &str) -> Vec<String> {
    body_wikilinks(&fence_regex().replace_all(&strip_frontmatter(raw), ""))
}

pub(super) fn normalized_entity_basename_index(
    records: &[EntityRecord],
) -> HashMap<String, Vec<&EntityRecord>> {
    let mut by_basename: HashMap<String, Vec<&EntityRecord>> = HashMap::new();
    for record in records {
        by_basename
            .entry(normalize_wikilink_target(&record.summary.basename))
            .or_default()
            .push(record);
    }
    by_basename
}

fn find_target_for_wikilink<'a>(
    target_title: &str,
    by_basename: &HashMap<String, Vec<&'a EntityRecord>>,
) -> Option<&'a EntityRecord> {
    let candidates = by_basename.get(&normalize_wikilink_target(target_title))?;
    candidates.first().copied()
}

fn relation_values(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(values)) => values
            .iter()
            .flat_map(|item| relation_values(Some(item)))
            .collect(),
        Some(Value::String(value)) => {
            let matches: Vec<_> = wikilink_regex()
                .captures_iter(value)
                .filter_map(|captures| captures.get(1))
                .map(|capture| capture.as_str().trim().to_string())
                .filter(|value| !value.is_empty())
                .collect();
            if matches.is_empty() {
                let stripped = strip_wikilink(value.trim());
                (!stripped.is_empty())
                    .then_some(stripped)
                    .into_iter()
                    .collect()
            } else {
                matches
            }
        }
        _ => Vec::new(),
    }
}

fn body_wikilinks(body: &str) -> Vec<String> {
    wikilink_regex()
        .captures_iter(body)
        .filter_map(|captures| captures.get(1))
        .map(|capture| capture.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn find_target<'a>(
    target_title: &str,
    relation_type: Option<&str>,
    by_basename: &HashMap<String, Vec<&'a EntityRecord>>,
) -> Option<&'a EntityRecord> {
    let normalized = normalize_wikilink_target(target_title);
    let candidates = by_basename.get(&normalized)?;
    let path_parts: Vec<_> = target_title
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let type_path = if path_parts.len() > 1 {
        path_parts.get(path_parts.len() - 2)
    } else {
        None
    };
    if let Some(type_path) = type_path {
        if let Some(candidate) = candidates.iter().find(|candidate| {
            candidate
                .summary
                .path
                .split('/')
                .any(|part| part == *type_path)
        }) {
            return Some(*candidate);
        }
    }
    let relation_type = relation_type
        .map(normalize_relation_type)
        .filter(|value| !value.is_empty());
    if let Some(relation_type) = relation_type {
        if let Some(candidate) = candidates.iter().find(|candidate| {
            normalize_relation_type(&candidate.summary.entity_type) == relation_type
                || normalize_relation_type(&candidate.summary.type_label) == relation_type
        }) {
            return Some(*candidate);
        }
    }
    candidates.first().copied()
}

fn normalize_relation_type(value: &str) -> String {
    value
        .chars()
        .filter(|character| !matches!(character, ' ' | '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn dedupe_relations(relations: Vec<Relation>) -> Vec<Relation> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for relation in relations {
        let key = format!(
            "{}\0{}\0{}\0{}\0{:?}",
            relation.source_id,
            relation.target_id.clone().unwrap_or_default(),
            relation.target_title,
            relation.field,
            relation.direction
        );
        if seen.insert(key) {
            deduped.push(relation);
        }
    }
    deduped
}
