use super::{
    compare_string, load_entity, read_library, read_library_cached, rebuild_for_edited_entity,
    IndexCacheContext,
};
use crate::types::{
    EntityTypeConfig, FieldConfig, FieldType, FilenameConfig, KizunaConfig, Library, Relation,
};
use crate::vfs::{InMemoryVfs, NativeVfs, Vfs};
use std::sync::Arc;

fn native_vfs(config: &KizunaConfig) -> Arc<dyn Vfs> {
    Arc::new(NativeVfs::new(&config.vault_root))
}

#[test]
fn compare_string_supports_non_english_collation_without_system_icu_data() {
    assert!(compare_string("星旅", "月城").is_ne());
}

#[test]
fn compare_string_is_total_for_mixed_ascii_and_non_ascii_values() {
    let values = [
        "Anime",
        "anime",
        "Zeta",
        "zeta",
        "アニメ",
        "星旅",
        "月城",
        " Pokémon",
        "Pokemon",
        "ポケモン",
        "音乐",
        "Music",
    ];

    for a in values {
        assert_eq!(compare_string(a, a), std::cmp::Ordering::Equal);
        for b in values {
            assert_eq!(compare_string(a, b), compare_string(b, a).reverse());
            for c in values {
                if compare_string(a, b).is_le() && compare_string(b, c).is_le() {
                    assert!(
                        compare_string(a, c).is_le(),
                        "compare_string is not transitive for {a:?}, {b:?}, {c:?}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn read_library_errors_when_vault_root_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(temp.path().join("missing").to_string_lossy().as_ref());
    let vfs = native_vfs(&config);

    let error = read_library(config, vfs).await.unwrap_err().to_string();

    assert!(error.contains("failed to access vault root"));
}

#[tokio::test]
async fn read_library_is_empty_when_taxonomy_root_is_missing() {
    // A freshly-created vault has no taxonomy directory yet; the load should
    // succeed with an empty library rather than failing.
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(temp.path().to_string_lossy().as_ref());
    let vfs = native_vfs(&config);

    let library = read_library(config, vfs).await.unwrap();

    assert!(library.records.is_empty());
    assert!(library.summaries().next().is_none());
}

#[tokio::test]
async fn read_library_uses_configured_id_fields_and_reports_frontmatter_errors() {
    let temp = tempfile::tempdir().unwrap();
    let taxonomy = temp.path().join("Taxonomy/Anime");
    std::fs::create_dir_all(&taxonomy).unwrap();
    std::fs::write(
        taxonomy.join("Renamed Later.md"),
        "---\nuid: anime-001\ntitle: Stable Title\n---\n",
    )
    .unwrap();
    std::fs::write(taxonomy.join("Broken.md"), "---\ntitle: [broken\n---\n").unwrap();
    let mut config = test_config(temp.path().to_string_lossy().as_ref());
    config.types[0].fields.insert(
        0,
        FieldConfig {
            field: "uid".to_string(),
            field_type: FieldType::Id,
            display_name: Some("UID".to_string()),
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
        },
    );

    let vfs = native_vfs(&config);
    let library = read_library(config, vfs).await.unwrap();

    assert!(library
        .summaries()
        .any(|entity| entity.id == "anime:anime-001"));
    assert_eq!(library.diagnostics.len(), 1);
    assert_eq!(library.diagnostics[0].path, "Taxonomy/Anime/Broken.md");
    assert_eq!(library.diagnostics[0].kind, "frontmatter");
    assert!(library.diagnostics[0]
        .message
        .contains("invalid frontmatter YAML"));
}

#[tokio::test]
async fn read_library_reads_entities_from_an_in_memory_vfs() {
    let config = test_config("/virtual-vault");
    let vfs = Arc::new(InMemoryVfs::new());
    vfs.insert_dir("Taxonomy/Anime");
    vfs.insert_file(
        "Taxonomy/Anime/Star Voyager.md",
        "---\ntitle: Star Voyager\nstatus: Watching\n---\n\nBody.\n",
    );

    let library = read_library(config, vfs).await.unwrap();

    assert_eq!(library.summaries().count(), 1);
    let summary = library.summaries().next().unwrap();
    assert_eq!(summary.title, "Star Voyager");
    assert_eq!(summary.path, "Taxonomy/Anime/Star Voyager.md");
    // Revision is derived from content + metadata, both supplied by the VFS.
    assert!(!library.records[0].revision.is_empty());
    // The resident record keeps frontmatter (for in-memory filtering) but the
    // body/raw are not part of it — they are loaded on demand. (The absence of
    // `body`/`raw` on `EntityRecord` is enforced at compile time.)
    assert!(library.records[0].frontmatter.contains_key("status"));
}

#[tokio::test]
async fn load_entity_reads_full_body_and_raw_on_demand() {
    let config = test_config("/virtual-vault");
    let vfs = Arc::new(InMemoryVfs::new());
    vfs.insert_dir("Taxonomy/Anime");
    vfs.insert_file(
        "Taxonomy/Anime/Star Voyager.md",
        "---\ntitle: Star Voyager\nstatus: Watching\n---\n\nBody text.\n",
    );

    let library = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
        .await
        .unwrap();
    let summary = library.summaries().next().unwrap();

    let entity = load_entity(&library.config, vfs.as_ref(), summary)
        .await
        .unwrap();

    // The on-demand load reconstructs the full document...
    assert!(entity.body.contains("Body text."));
    assert!(entity.raw.contains("title: Star Voyager"));
    assert!(entity.frontmatter.contains_key("status"));
    // ...and carries the library-wide relation count from the resident summary.
    assert_eq!(entity.summary.relation_count, summary.relation_count);
    assert_eq!(entity.summary.id, summary.id);
}

// --- Surgical-update equivalence (Phase 3) -----------------------------

/// The contract of [`rebuild_for_edited_entity`]: a surgical update of one
/// edited file must produce a library value-equivalent (records, relations,
/// derived counts) to re-reading the whole vault from disk.
#[tokio::test]
async fn surgical_update_matches_full_reload() {
    // (initial Alpha.md, edited Alpha.md) — covering retarget, add, remove,
    // title change (touches the In-reflection title + sort order), an
    // unresolved target, and a body-wikilink change.
    let cases = [
        (
            "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n\nSee [[Gamma]].\n",
            "---\ntitle: Alpha\nrelated: \"[[Gamma]]\"\n---\n\nSee [[Gamma]].\n",
        ),
        (
            "---\ntitle: Alpha\n---\n",
            "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n",
        ),
        (
            "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n",
            "---\ntitle: Alpha\n---\n",
        ),
        (
            "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n",
            "---\ntitle: Alphaz\nrelated: \"[[Beta]]\"\n---\n",
        ),
        (
            "---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n",
            "---\ntitle: Alpha\nrelated: \"[[Ghost]]\"\n---\n",
        ),
        (
            "---\ntitle: Alpha\n---\n\nSee [[Beta]].\n",
            "---\ntitle: Alpha\n---\n\nSee [[Gamma]].\n",
        ),
    ];

    for (initial, edited) in cases {
        let config = relation_test_config();
        let vfs = make_relation_vault(initial);
        let base = read_library(config.clone(), Arc::clone(&vfs) as Arc<dyn Vfs>)
            .await
            .unwrap();

        vfs.insert_file("Taxonomy/Anime/Alpha.md", edited);
        let surgical = rebuild_for_edited_entity(&base, vfs.as_ref(), "Taxonomy/Anime/Alpha.md")
            .await
            .unwrap()
            .expect("edit should take the surgical path");
        let full = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
            .await
            .unwrap();

        assert_libraries_equivalent(&surgical, &full, initial, edited);
    }
}

#[tokio::test]
async fn surgical_update_falls_back_on_structural_changes() {
    let config = relation_test_config();
    let vfs = make_relation_vault("---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n");
    let base = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
        .await
        .unwrap();

    // Unknown path (e.g. a rename's new file the base hasn't indexed) → None.
    let renamed = rebuild_for_edited_entity(&base, vfs.as_ref(), "Taxonomy/Anime/Renamed.md")
        .await
        .unwrap();
    assert!(renamed.is_none());

    // A changed id (here: the file vanished from its indexed path) → None.
    let missing = rebuild_for_edited_entity(&base, vfs.as_ref(), "Taxonomy/Anime/Ghost.md")
        .await
        .unwrap();
    assert!(missing.is_none());
}

fn relation_test_config() -> KizunaConfig {
    let mut config = test_config("/virtual-vault");
    config.types[0]
        .fields
        .push(relation_field("related", FieldType::Relation));
    config
}

fn relation_field(name: &str, field_type: FieldType) -> FieldConfig {
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

fn make_relation_vault(alpha: &str) -> Arc<InMemoryVfs> {
    let vfs = Arc::new(InMemoryVfs::new());
    vfs.insert_dir("Taxonomy/Anime");
    vfs.insert_file("Taxonomy/Anime/Alpha.md", alpha);
    vfs.insert_file("Taxonomy/Anime/Beta.md", "---\ntitle: Beta\n---\n");
    vfs.insert_file("Taxonomy/Anime/Gamma.md", "---\ntitle: Gamma\n---\n");
    vfs
}

fn assert_libraries_equivalent(surgical: &Library, full: &Library, initial: &str, edited: &str) {
    let context = format!("\n  initial: {initial:?}\n  edited:  {edited:?}");

    let mut surgical_records = surgical.records.clone();
    let mut full_records = full.records.clone();
    surgical_records.sort_by(|a, b| a.summary.id.cmp(&b.summary.id));
    full_records.sort_by(|a, b| a.summary.id.cmp(&b.summary.id));
    assert_eq!(
        surgical_records, full_records,
        "records (incl. derived relation_count) diverge from a full reload{context}"
    );

    let mut surgical_relations: Vec<String> = surgical.relations.iter().map(relation_key).collect();
    let mut full_relations: Vec<String> = full.relations.iter().map(relation_key).collect();
    surgical_relations.sort();
    full_relations.sort();
    assert_eq!(
        surgical_relations, full_relations,
        "relation graph diverges from a full reload{context}"
    );
}

fn relation_key(relation: &Relation) -> String {
    format!(
        "{}|{}|{}|{}|{:?}",
        relation.source_id,
        relation.target_id.clone().unwrap_or_default(),
        relation.target_title,
        relation.field,
        relation.direction
    )
}

fn test_config(vault_root: &str) -> KizunaConfig {
    KizunaConfig {
        vault_root: vault_root.to_string(),
        taxonomy_root: "Taxonomy".to_string(),
        asset_root: None,
        content_writable: None,
        home: None,
        daily_notes: None,
        types: vec![EntityTypeConfig {
            id: "anime".to_string(),
            label: "Anime".to_string(),
            icon: None,
            path: "Anime".to_string(),
            external_priority: Vec::new(),
            filename: Some(FilenameConfig {
                title_language: Some("zh".to_string()),
                title_role: None,
            }),
            body_mappings: Vec::new(),
            fields: vec![FieldConfig {
                field: "title".to_string(),
                field_type: FieldType::Title,
                display_name: Some("Title".to_string()),
                title_language: Some("zh".to_string()),
                title_role: None,
                external_fields: Vec::new(),
                enum_options: Vec::new(),
                total_progress_field: None,
                date_role: None,
                season_language: None,
                external_ref: None,
                external_types: Vec::new(),
                relation_type: None,
            }],
        }],
    }
}

// --- Persistent index cache ----------------------------------------------

fn cache_ctx(dir: &std::path::Path, schema: &str) -> IndexCacheContext {
    IndexCacheContext::new(dir.to_path_buf(), schema, "test-vault")
}

fn seed_one(vfs: &InMemoryVfs, title: &str) {
    vfs.insert_dir("Taxonomy/Anime");
    vfs.insert_file(
        "Taxonomy/Anime/Star Voyager.md",
        &format!("---\ntitle: {title}\n---\n\nBody.\n"),
    );
}

/// An unchanged fingerprint reuses the cached parse without re-reading the
/// file: we edit the bytes but preserve size + mtime, then assert the load
/// still serves the *old* title — only possible if the file was never read.
#[tokio::test]
async fn index_cache_reuses_unchanged_entries_without_rereading() {
    let dir = tempfile::tempdir().unwrap();
    let vfs = Arc::new(InMemoryVfs::new());
    seed_one(&vfs, "AAAA");

    let first = read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();
    assert_eq!(first.summaries().next().unwrap().title, "AAAA");

    vfs.overwrite_preserving_stamp(
        "Taxonomy/Anime/Star Voyager.md",
        "---\ntitle: BBBB\n---\n\nBody.\n",
    );
    let second = read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();
    assert_eq!(
        second.summaries().next().unwrap().title,
        "AAAA",
        "unchanged fingerprint must serve the cached parse"
    );
}

/// A changed fingerprint (mtime bump from a normal edit) re-reads and
/// re-parses the file.
#[tokio::test]
async fn index_cache_reparses_when_fingerprint_changes() {
    let dir = tempfile::tempdir().unwrap();
    let vfs = Arc::new(InMemoryVfs::new());
    seed_one(&vfs, "AAAA");

    read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();

    vfs.insert_file(
        "Taxonomy/Anime/Star Voyager.md",
        "---\ntitle: CCCC\n---\n\nBody.\n",
    );
    let updated = read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();
    assert_eq!(updated.summaries().next().unwrap().title, "CCCC");
}

/// A different schema fingerprint discards the whole cache even when every
/// file's fingerprint is unchanged — because all derivation flows from the
/// schema.
#[tokio::test]
async fn index_cache_busts_when_schema_fingerprint_changes() {
    let dir = tempfile::tempdir().unwrap();
    let vfs = Arc::new(InMemoryVfs::new());
    seed_one(&vfs, "AAAA");

    read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();

    vfs.overwrite_preserving_stamp(
        "Taxonomy/Anime/Star Voyager.md",
        "---\ntitle: BBBB\n---\n\nBody.\n",
    );
    let updated = read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v2"),
    )
    .await
    .unwrap();
    assert_eq!(
        updated.summaries().next().unwrap().title,
        "BBBB",
        "a schema change must invalidate every cached record"
    );
}

/// Files absent from the current listing drop out of the cache (delete).
#[tokio::test]
async fn index_cache_drops_deleted_entries() {
    let dir = tempfile::tempdir().unwrap();
    let vfs = Arc::new(InMemoryVfs::new());
    vfs.insert_dir("Taxonomy/Anime");
    vfs.insert_file("Taxonomy/Anime/A.md", "---\ntitle: A\n---\n");
    vfs.insert_file("Taxonomy/Anime/B.md", "---\ntitle: B\n---\n");

    read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();

    vfs.remove_file("Taxonomy/Anime/B.md").await.unwrap();
    let after = read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();
    let titles: Vec<&str> = after
        .summaries()
        .map(|summary| summary.title.as_str())
        .collect();
    assert_eq!(titles, vec!["A"]);
}

/// The cached load (cold and warm) is value-equivalent to a full
/// [`read_library`], including the recomputed relation graph and counts.
#[tokio::test]
async fn cached_load_matches_uncached_read_library() {
    let dir = tempfile::tempdir().unwrap();
    let vfs =
        make_relation_vault("---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n\nSee [[Gamma]].\n");

    let cold = read_library_cached(
        relation_test_config(),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();
    let uncached = read_library(relation_test_config(), Arc::clone(&vfs) as Arc<dyn Vfs>)
        .await
        .unwrap();
    assert_libraries_equivalent(&cold, &uncached, "cold-cached", "uncached");

    // A second cached load reuses every entry from the warm cache.
    let warm = read_library_cached(
        relation_test_config(),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();
    assert_libraries_equivalent(&warm, &uncached, "warm-cached", "uncached");
}

// --- Daily-note relation reuse on edit -----------------------------------

fn daily_notes_config() -> KizunaConfig {
    let mut config = relation_test_config();
    config.daily_notes = Some(crate::types::DailyNotesConfig {
        paths: vec!["Journal".to_string()],
        date_format: None,
    });
    config
}

fn daily_notes_vault(alpha: &str, journal: &str) -> Arc<InMemoryVfs> {
    let vfs = Arc::new(InMemoryVfs::new());
    vfs.insert_dir("Taxonomy/Anime");
    vfs.insert_file("Taxonomy/Anime/Alpha.md", alpha);
    vfs.insert_file("Taxonomy/Anime/Beta.md", "---\ntitle: Beta\n---\n");
    vfs.insert_dir("Journal");
    vfs.insert_file("Journal/2026-06-16.md", journal);
    vfs
}

fn daily_note_relation_count(library: &Library) -> usize {
    library
        .relations
        .iter()
        .filter(|relation| relation.field == "daily-note")
        .count()
}

/// An in-place entity edit, recomputed surgically, keeps the daily-note
/// relations and stays value-equivalent to a full reload.
#[tokio::test]
async fn surgical_update_preserves_daily_note_relations() {
    let config = daily_notes_config();
    let vfs = daily_notes_vault("---\ntitle: Alpha\n---\n", "Watched [[Alpha]] today.\n");

    let base = read_library(config.clone(), Arc::clone(&vfs) as Arc<dyn Vfs>)
        .await
        .unwrap();
    assert_eq!(daily_note_relation_count(&base), 1, "fixture sanity");

    vfs.insert_file("Taxonomy/Anime/Alpha.md", "---\ntitle: Alphaz\n---\n");
    let surgical = rebuild_for_edited_entity(&base, vfs.as_ref(), "Taxonomy/Anime/Alpha.md")
        .await
        .unwrap()
        .expect("in-place edit takes the surgical path");
    let full = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
        .await
        .unwrap();

    assert_libraries_equivalent(&surgical, &full, "daily-note base", "daily-note edited");
}

/// The surgical path must reuse the resident daily-note relations rather than
/// re-reading the journal: we drop the link from the daily note on disk *after*
/// the base load, then edit the entity — the relation survives only if it came
/// from memory. (This would fail if the edit path re-read daily notes.)
#[tokio::test]
async fn surgical_update_reuses_daily_note_relations_without_rereading() {
    let config = daily_notes_config();
    let vfs = daily_notes_vault("---\ntitle: Alpha\n---\n", "Watched [[Alpha]] today.\n");

    let base = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
        .await
        .unwrap();
    assert_eq!(daily_note_relation_count(&base), 1, "fixture sanity");

    // Disk no longer has the link; only memory (base) still does.
    vfs.insert_file("Journal/2026-06-16.md", "Nothing linked today.\n");
    vfs.insert_file("Taxonomy/Anime/Alpha.md", "---\ntitle: Alphaz\n---\n");
    let surgical = rebuild_for_edited_entity(&base, vfs.as_ref(), "Taxonomy/Anime/Alpha.md")
        .await
        .unwrap()
        .expect("in-place edit takes the surgical path");

    assert_eq!(
        daily_note_relation_count(&surgical),
        1,
        "daily-note relations must be reused from memory, not re-read from disk"
    );
}

/// A cached cold start reuses an unchanged daily note's links without reading
/// its body: we edit the journal bytes but keep size + mtime, then assert the
/// relation persists — only possible if the note was not re-read.
#[tokio::test]
async fn index_cache_reuses_daily_note_links_without_rereading() {
    let dir = tempfile::tempdir().unwrap();
    let journal = "Watched [[Alpha]] today\n";
    let vfs = daily_notes_vault("---\ntitle: Alpha\n---\n", journal);

    let first = read_library_cached(
        daily_notes_config(),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();
    assert_eq!(daily_note_relation_count(&first), 1, "fixture sanity");

    // Same byte length (so the size matches) + preserved mtime → a change the
    // fingerprint can't see. A hit must reuse the cached links.
    let linkless = format!("{:width$}\n", "no link", width = journal.len() - 1);
    assert_eq!(linkless.len(), journal.len());
    vfs.overwrite_preserving_stamp("Journal/2026-06-16.md", &linkless);

    let second = read_library_cached(
        daily_notes_config(),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();
    assert_eq!(
        daily_note_relation_count(&second),
        1,
        "unchanged daily-note fingerprint must serve cached links, not re-read"
    );
}

/// When a daily note actually changes (mtime bumped), its links are re-read.
#[tokio::test]
async fn index_cache_reparses_daily_note_when_fingerprint_changes() {
    let dir = tempfile::tempdir().unwrap();
    let vfs = daily_notes_vault("---\ntitle: Alpha\n---\n", "Watched [[Alpha]] today.\n");

    read_library_cached(
        daily_notes_config(),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();

    // A normal write bumps the mtime, so the note is re-read and the link drops.
    vfs.insert_file("Journal/2026-06-16.md", "Nothing linked today.\n");
    let updated = read_library_cached(
        daily_notes_config(),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        cache_ctx(dir.path(), "schema-v1"),
    )
    .await
    .unwrap();
    assert_eq!(daily_note_relation_count(&updated), 0);
}
