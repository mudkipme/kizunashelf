use super::{
    compare_string, compute_listing_fingerprint, load_entity, read_library, read_library_cached,
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
            enum_role: None,
            status_values: None,
            date_role: None,
            season_language: None,
            external_ref: None,
            external_types: Vec::new(),
            relation_type: None,
            rating_max: None,
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
async fn read_library_skips_an_unparseable_file_instead_of_failing() {
    // One bad file (here: not valid UTF-8) must never take the whole library
    // down — it is skipped with a diagnostic and every other entity still loads.
    let temp = tempfile::tempdir().unwrap();
    let taxonomy = temp.path().join("Taxonomy/Anime");
    std::fs::create_dir_all(&taxonomy).unwrap();
    std::fs::write(
        taxonomy.join("Good.md"),
        "---\ntitle: Steins;Gate 0\n---\n\nBody.\n",
    )
    .unwrap();
    // Shift-JIS bytes (「アニメ」), invalid as UTF-8.
    std::fs::write(
        taxonomy.join("Legacy.md"),
        [0x83u8, 0x41, 0x83, 0x6a, 0x83, 0x81],
    )
    .unwrap();
    let config = test_config(temp.path().to_string_lossy().as_ref());
    let vfs = native_vfs(&config);

    let library = read_library(config, vfs).await.unwrap();

    assert!(library
        .summaries()
        .any(|entity| entity.title == "Steins;Gate 0"));
    assert_eq!(library.diagnostics.len(), 1);
    assert_eq!(library.diagnostics[0].path, "Taxonomy/Anime/Legacy.md");
    assert_eq!(library.diagnostics[0].kind, "file");
    assert!(library.diagnostics[0].message.contains("not valid UTF-8"));
}

#[tokio::test]
async fn in_memory_library_indexes_metadata_and_loads_full_entities_on_demand() {
    let config = test_config("/virtual-vault");
    let vfs = Arc::new(InMemoryVfs::new());
    vfs.insert_dir("Taxonomy/Anime");
    vfs.insert_file(
        "Taxonomy/Anime/Steins;Gate 0 (Anime).md",
        "---\ntitle: Steins;Gate 0\nstatus: Watching\n---\n\nBody text.\n",
    );

    let library = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
        .await
        .unwrap();

    assert_eq!(library.summaries().count(), 1);
    let summary = library.summaries().next().unwrap();
    assert_eq!(summary.title, "Steins;Gate 0");
    assert_eq!(summary.path, "Taxonomy/Anime/Steins;Gate 0 (Anime).md");
    // Revision is derived from content + metadata, both supplied by the VFS.
    assert!(!library.records[0].revision.is_empty());
    // The resident record keeps frontmatter (for in-memory filtering) but the
    // body/raw are not part of it — they are loaded on demand. (The absence of
    // `body`/`raw` on `EntityRecord` is enforced at compile time.)
    assert!(library.records[0].frontmatter.contains_key("status"));

    let entity = load_entity(&library.config, vfs.as_ref(), summary)
        .await
        .unwrap();

    // The on-demand load reconstructs the full document...
    assert!(entity.body.contains("Body text."));
    assert!(entity.raw.contains("title: Steins;Gate 0"));
    assert!(entity.frontmatter.contains_key("status"));
    // ...and carries the library-wide relation count from the resident summary.
    assert_eq!(entity.summary.relation_count, summary.relation_count);
    assert_eq!(entity.summary.id, summary.id);
}

/// The analytics memo keys on `content_revision`, so it must be stable across a
/// content-identical reload (no spurious rebuild) and flip when content changes.
#[tokio::test]
async fn content_revision_tracks_content_not_load_time() {
    let config = relation_test_config();
    let vfs = make_relation_vault("---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n");
    let base = read_library(config.clone(), Arc::clone(&vfs) as Arc<dyn Vfs>)
        .await
        .unwrap();

    // Identical content, re-read: the fingerprint is stable across the reload
    // (whereas `generated_at` is a fresh wall-clock stamp every load).
    let reloaded = read_library(config.clone(), Arc::clone(&vfs) as Arc<dyn Vfs>)
        .await
        .unwrap();
    assert_eq!(base.content_revision, reloaded.content_revision);

    // A content edit flips the fingerprint.
    vfs.insert_file(
        "Taxonomy/Anime/Alpha.md",
        "---\ntitle: Alpha\nrelated: \"[[Gamma]]\"\n---\n",
    );
    let edited = read_library(config, Arc::clone(&vfs) as Arc<dyn Vfs>)
        .await
        .unwrap();
    assert_ne!(base.content_revision, edited.content_revision);
}

/// The listing fingerprint backs `get_library`'s "nothing changed → reuse the
/// cached library" fast path, so it must be stable across a content-identical
/// re-list and flip on any add, remove, content edit, or schema change.
#[tokio::test]
async fn listing_fingerprint_tracks_the_vault_listing() {
    let config = relation_test_config();
    let vfs = make_relation_vault("---\ntitle: Alpha\nrelated: \"[[Beta]]\"\n---\n");

    let base = compute_listing_fingerprint(&config, vfs.as_ref())
        .await
        .expect("in-memory vfs reports mtimes");
    // Re-listing identical content yields the same fingerprint.
    assert_eq!(
        Some(base),
        compute_listing_fingerprint(&config, vfs.as_ref()).await
    );

    // Editing a file (bumps its mtime) flips the fingerprint.
    vfs.insert_file("Taxonomy/Anime/Alpha.md", "---\ntitle: Alpha\n---\n");
    let after_edit = compute_listing_fingerprint(&config, vfs.as_ref())
        .await
        .unwrap();
    assert_ne!(base, after_edit);

    // Adding a file flips it.
    vfs.insert_file("Taxonomy/Anime/Delta.md", "---\ntitle: Delta\n---\n");
    let after_add = compute_listing_fingerprint(&config, vfs.as_ref())
        .await
        .unwrap();
    assert_ne!(after_edit, after_add);

    // An external schema edit (the config file is neither an entity nor a note)
    // must still flip the fingerprint so the fast path can't serve a stale schema.
    vfs.insert_file(
        crate::library::VAULT_CONFIG_RELATIVE_PATH,
        "taxonomyRoot: Taxonomy\ntypes: []\n",
    );
    let after_config = compute_listing_fingerprint(&config, vfs.as_ref())
        .await
        .unwrap();
    assert_ne!(after_add, after_config);
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
        enum_role: None,
        status_values: None,
        date_role: None,
        season_language: None,
        external_ref: None,
        external_types: Vec::new(),
        relation_type: None,
        rating_max: None,
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

/// Asserts two libraries are value-equivalent: the same records (incl. derived
/// `relation_count`) and the same relation graph as a *set* (order isn't part of
/// the contract). `actual_label`/`expected_label` name the two sides in failures.
fn assert_libraries_equivalent(
    actual: &Library,
    expected: &Library,
    actual_label: &str,
    expected_label: &str,
) {
    let context = format!("\n  actual:   {actual_label:?}\n  expected: {expected_label:?}");

    let mut actual_records = actual.records.clone();
    let mut expected_records = expected.records.clone();
    actual_records.sort_by(|a, b| a.summary.id.cmp(&b.summary.id));
    expected_records.sort_by(|a, b| a.summary.id.cmp(&b.summary.id));
    assert_eq!(
        actual_records, expected_records,
        "records (incl. derived relation_count) diverge{context}"
    );

    let mut actual_relations: Vec<String> = actual.relations.iter().map(relation_key).collect();
    let mut expected_relations: Vec<String> = expected.relations.iter().map(relation_key).collect();
    actual_relations.sort();
    expected_relations.sort();
    assert_eq!(
        actual_relations, expected_relations,
        "relation graph diverges{context}"
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
        daily_notes: None,
        tags: None,
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
            body_sections: Vec::new(),
            log: None,
            fields: vec![FieldConfig {
                field: "title".to_string(),
                field_type: FieldType::Title,
                display_name: Some("Title".to_string()),
                title_language: Some("zh".to_string()),
                title_role: None,
                external_fields: Vec::new(),
                enum_options: Vec::new(),
                enum_role: None,
                status_values: None,
                date_role: None,
                season_language: None,
                external_ref: None,
                external_types: Vec::new(),
                relation_type: None,
                rating_max: None,
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
        "Taxonomy/Anime/Steins;Gate 0 (Anime).md",
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
        "Taxonomy/Anime/Steins;Gate 0 (Anime).md",
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
        "Taxonomy/Anime/Steins;Gate 0 (Anime).md",
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
        "Taxonomy/Anime/Steins;Gate 0 (Anime).md",
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

/// The process-resident (in-memory) index cache reuses an unchanged file's
/// parse across reloads without re-reading it — the property the web server
/// relies on when no persistent cache dir is configured.
#[tokio::test]
async fn memory_index_cache_reuses_unchanged_entries() {
    use crate::library::MemoryIndexCache;
    use std::sync::{Arc as StdArc, Mutex as StdMutex};

    let store = StdArc::new(StdMutex::new(MemoryIndexCache::default()));
    let vfs = Arc::new(InMemoryVfs::new());
    seed_one(&vfs, "AAAA");

    let first = read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        IndexCacheContext::memory(StdArc::clone(&store), "schema-v1", "test-vault"),
    )
    .await
    .unwrap();
    assert_eq!(first.summaries().next().unwrap().title, "AAAA");

    // Edit the bytes but keep size + mtime: a re-read would surface "BBBB", so
    // serving "AAAA" proves the cached parse (held in `store`) was reused.
    vfs.overwrite_preserving_stamp(
        "Taxonomy/Anime/Steins;Gate 0 (Anime).md",
        "---\ntitle: BBBB\n---\n\nBody.\n",
    );
    let second = read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        IndexCacheContext::memory(StdArc::clone(&store), "schema-v1", "test-vault"),
    )
    .await
    .unwrap();
    assert_eq!(
        second.summaries().next().unwrap().title,
        "AAAA",
        "the in-memory cache must serve the unchanged file's cached parse"
    );

    // A schema change busts the whole in-memory cache (gate mismatch).
    vfs.overwrite_preserving_stamp(
        "Taxonomy/Anime/Steins;Gate 0 (Anime).md",
        "---\ntitle: CCCC\n---\n\nBody.\n",
    );
    let rebuilt = read_library_cached(
        test_config("/virtual-vault"),
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        IndexCacheContext::memory(StdArc::clone(&store), "schema-v2", "test-vault"),
    )
    .await
    .unwrap();
    assert_eq!(rebuilt.summaries().next().unwrap().title, "CCCC");
}

// --- Daily-note relation handling ----------------------------------------

fn daily_notes_config() -> KizunaConfig {
    let mut config = relation_test_config();
    config.daily_notes = Some(crate::types::DailyNotesConfig {
        paths: vec!["Journal".to_string()],
        date_format: None,
        template: None,
        log: None,
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
