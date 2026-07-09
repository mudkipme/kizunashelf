use axum::body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use kizunashelf::api::{router_native, ApiOptions};
use kizunashelf::secrets::NativeSecretStore;
use kizunashelf::types::AppConfig;
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tower::ServiceExt;

/// Builds the production-shape (inline) router for tests: the app config (vault
/// root + write mode) is passed inline like every real runtime (web/desktop/iOS),
/// and only the vault config lives on disk inside the vault.
fn inline_router(vault_root: &Path, settings_writable: bool, content_writable: bool) -> Router {
    build_inline_router(vault_root, settings_writable, content_writable, false)
}

/// Like [`inline_router`] but with host-driven asset ingest enabled — the iOS
/// in-process-host posture the `plan`/`ingest` endpoints require. The network
/// server uses [`inline_router`] (off), so the host-path surface is gated there.
fn host_inline_router(vault_root: &Path, content_writable: bool) -> Router {
    build_inline_router(vault_root, true, content_writable, true)
}

fn build_inline_router(
    vault_root: &Path,
    settings_writable: bool,
    content_writable: bool,
    host_asset_ingest: bool,
) -> Router {
    let token_path = vault_root
        .parent()
        .map(|parent| parent.join(".tokens.json"))
        .unwrap_or_else(|| PathBuf::from(".tokens.json"));
    router_native(
        ApiOptions {
            config_path: PathBuf::new(),
            cache_ttl: Duration::from_millis(0),
            web_dist_path: None,
            settings_writable,
            content_writable,
            index_cache_dir: None,
            host_asset_ingest,
        },
        AppConfig {
            vault_root: vault_root.to_string_lossy().to_string(),
            content_writable: Some(content_writable),
        },
        Arc::new(NativeSecretStore::with_token_path(token_path)),
    )
}

struct TestServer {
    app: Router,
    vault: PathBuf,
    _temp: TempDir,
}

#[tokio::test]
async fn system_and_entity_endpoints_read_a_temp_vault() {
    let server = TestServer::new();

    let health = server.ok_json("/api/health").await;
    assert_eq!(health["ok"], true);
    assert_eq!(health["entityCount"], 4);
    assert_eq!(health["relationCount"], 12);
    assert!(health["generatedAt"].as_str().is_some());

    let capabilities = server.ok_json("/api/capabilities").await;
    assert_eq!(capabilities["settingsWritable"], true);
    assert_eq!(capabilities["contentWritable"], true);
    assert_eq!(capabilities["externalSearchEnabled"], true);
    assert_eq!(capabilities["externalApplyEnabled"], true);

    let external_providers = server.ok_json("/api/external/providers").await;
    let provider_list = external_providers["providers"].as_array().unwrap();
    // The first three providers are stable; the registry grows as sources are
    // added, so assert a lower bound rather than an exact count.
    assert!(provider_list.len() >= 12);
    assert_eq!(external_providers["providers"][0]["id"], "bangumi");
    assert_eq!(
        external_providers["providers"][0]["fields"][0]["field"],
        "name"
    );
    // The provider catalog deliberately carries no role→field guesses; that
    // preset-seeding data lives only in `crate::presets`.
    assert!(external_providers["providers"][0]
        .get("defaultFieldMappings")
        .is_none());
    // Bangumi is keyless and supports free-text search; the contract surfaces
    // both facts for the frontend/iOS to render from.
    assert_eq!(external_providers["providers"][0]["credentials"], json!([]));
    assert_eq!(external_providers["providers"][0]["searchSupported"], true);
    assert_eq!(external_providers["providers"][1]["id"], "igdb");
    assert_eq!(
        external_providers["providers"][1]["defaultExternalTypes"],
        json!(["game"])
    );
    // IGDB declares its credential fields in the catalog (the single source the
    // desktop/iOS credential editors render from).
    assert_eq!(
        external_providers["providers"][1]["credentials"][0]["key"],
        "igdb_client_id"
    );

    let languages = server.ok_json("/api/languages").await;
    let language_list = languages["languages"].as_array().unwrap();
    assert!(
        language_list.len() > 3,
        "expects the full TheTVDB language set"
    );
    assert!(language_list
        .iter()
        .any(|language| language["code"] == "ja" && language["label"] == "Japanese"));

    let config = server.ok_json("/api/config").await;
    assert_eq!(config["taxonomyRoot"], "Taxonomy");
    assert_eq!(config["types"].as_array().unwrap().len(), 4);
    assert_eq!(config["types"][0]["icon"], "📺");
    assert_eq!(
        config["types"][0]["filename"],
        json!({ "titleLanguage": "zh" })
    );
    assert_eq!(
        config["types"][0]["fields"][1],
        json!({
            "field": "title",
            "fieldType": "title",
            "displayName": "Title",
            "titleLanguage": "zh"
        })
    );
    assert_eq!(
        config["types"][0]["fields"][5],
        json!({
            "field": "status",
            "fieldType": "enum",
            "displayName": "Status",
            "enumOptions": ["Backlog", "Watching", "Completed", "Paused", "Dropped"],
            "enumRole": "status",
            "statusValues": { "ongoing": ["Watching"], "paused": ["Paused"], "completed": ["Completed"], "dropped": ["Dropped"] }
        })
    );
    assert_eq!(config["types"][0]["fields"][3]["field"], "title_original");
    assert_eq!(config["types"][0]["fields"][3]["titleRole"], "original");
    assert_eq!(config["types"][0]["fields"][6]["fieldType"], "season");
    assert_eq!(config["types"][0]["fields"][6]["dateRole"], "planning");
    assert_eq!(config["types"][0]["fields"][6]["seasonLanguage"], "zh");

    let home = server.ok_json("/api/home").await;
    assert_eq!(home["title"], "Fixture Home");
    assert_eq!(home["sections"][0]["title"], "Recent Anime");
    assert_eq!(home["sections"][0]["total"], 1);
    // Resolved title now falls back to the `original`-role title when no viewer
    // language is matched (the core no longer has a `defaultTitle`).
    assert_eq!(home["sections"][0]["items"][0]["title"], "星之航路");
    assert_eq!(home["sections"][2]["title"], "Completed Anime");
    assert_eq!(home["sections"][2]["total"], 0);

    let stats = server.ok_json("/api/stats").await;
    assert_eq!(stats["total"], 4);
    assert_eq!(stats["relations"], 10);
    assert_eq!(count_for(&stats["byType"], "Anime"), 1);
    assert_eq!(count_for(&stats["byType"], "Games"), 1);
    assert_eq!(stats["byType"][0]["icon"], "📺");

    let anime_stats = server.ok_json("/api/stats?type=anime").await;
    assert_eq!(anime_stats["total"], 1);
    assert_eq!(
        anime_stats["dateFields"],
        json!(["season", "complete_date"])
    );

    let analytics = server.ok_json("/api/analytics").await;
    assert_eq!(analytics["totals"]["entities"], 4);
    assert_eq!(analytics["totals"]["relations"], 9);
    assert_eq!(analytics["totals"]["unresolvedRelations"], 2);
    assert_eq!(analytics["totals"]["datedEntities"], 3);
    // Activity buckets dated entities into a year × month matrix. Star Voyager
    // completed 2025-04-20 → year 2025, April (month index 3).
    let activity = &analytics["activity"];
    assert_eq!(activity["totalDated"], 3);
    assert!(!activity["types"].as_array().unwrap().is_empty());
    let years = activity["years"].as_array().unwrap();
    let year_2025 = years
        .iter()
        .find(|year| year["year"] == 2025)
        .expect("2025 activity row");
    assert!(year_2025["total"].as_u64().unwrap() >= 1);
    assert!(year_2025["months"][3].as_u64().unwrap() >= 1);

    let cleanup = server.ok_json("/api/cleanup-queues").await;
    let missing_cover = queue_summary(&cleanup["queues"], "missing-cover");
    assert_eq!(missing_cover["total"], 2);
    assert_eq!(missing_cover["remaining"], 1);
    let missing_refs = queue_summary(&cleanup["queues"], "missing-refs");
    assert_eq!(missing_refs["total"], 3);
    assert_eq!(missing_refs["remaining"], 1);
    let isolated = queue_summary(&cleanup["queues"], "isolated");
    assert_eq!(isolated["total"], 3);
    assert_eq!(isolated["remaining"], 0);
    assert_eq!(cleanup["missingCover"][0]["id"], "games:Moon Quest");
    assert_eq!(
        cleanup["missingExternalRefs"][0]["id"],
        "music:Opening Theme"
    );
    assert_eq!(cleanup["isolated"].as_array().unwrap().len(), 0);

    let entities = server.ok_json("/api/entities").await;
    assert_eq!(entities["total"], 4);
    assert!(has_entity_title(&entities["items"], "星之航路"));
    assert!(has_entity_title(&entities["items"], "Moon Quest"));
    assert_eq!(entities["items"][0]["titles"]["zh"], "Star Voyager");
    assert_eq!(entities["items"][0]["titles"]["en"], "A Voyage of Stars");
    assert_eq!(entities["items"][0]["titles"]["title_original"], "星之航路");
    let star_voyager_summary = entity_by_title(&entities["items"], "星之航路");
    assert_eq!(star_voyager_summary["relationCount"], 4);
    let completed_date = star_voyager_summary["dates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["field"] == "complete_date")
        .unwrap();
    assert_eq!(completed_date["sortKey"], "2025-04-20");
    assert_eq!(completed_date["parsed"]["year"], 2025);
    assert_eq!(completed_date["parsed"]["month"], 4);
    assert_eq!(completed_date["parsed"]["day"], 20);
    assert_eq!(completed_date["parsed"]["seasonKey"], "spring");

    let english_title_sort = server
        .ok_json("/api/entities?sort=title&titleLanguage=en")
        .await;
    assert_eq!(english_title_sort["items"][0]["id"], "anime:Star Voyager");

    let relation_count_sort = server
        .ok_json("/api/entities?sort=relationCount&direction=desc")
        .await;
    assert_eq!(relation_count_sort["items"][0]["id"], "anime:Star Voyager");

    // Relation-field filter: entities whose `franchise` relation points to Star Saga.
    let franchise_filter = urlencoding::encode(r#"[{"field":"franchise","values":["Star Saga"]}]"#);
    let franchise_filtered = server
        .ok_json(&format!("/api/entities?filters={franchise_filter}"))
        .await;
    assert_eq!(franchise_filtered["total"], 2);
    assert!(has_entity_title(&franchise_filtered["items"], "星之航路"));
    assert!(has_entity_title(&franchise_filtered["items"], "Moon Quest"));

    let status_filters =
        urlencoding::encode(r#"[{"field":"status","values":["Watching","Playing"]}]"#);
    let status_filtered = server
        .ok_json(&format!("/api/entities?filters={status_filters}"))
        .await;
    assert_eq!(status_filtered["total"], 2);
    assert!(has_entity_title(&status_filtered["items"], "星之航路"));
    assert!(has_entity_title(&status_filtered["items"], "Moon Quest"));

    let genre_filters = urlencoding::encode(r#"[{"field":"genres","values":["Strategy","RPG"]}]"#);
    let genre_filtered = server
        .ok_json(&format!("/api/entities?type=games&filters={genre_filters}"))
        .await;
    assert_eq!(genre_filtered["total"], 1);
    assert_eq!(genre_filtered["items"][0]["id"], "games:Moon Quest");

    let missing_genre_filters = urlencoding::encode(r#"[{"field":"genres","values":["RPG"]}]"#);
    let missing_genre_filtered = server
        .ok_json(&format!(
            "/api/entities?type=games&filters={missing_genre_filters}"
        ))
        .await;
    assert_eq!(missing_genre_filtered["total"], 0);

    let favorite_filters = urlencoding::encode(r#"[{"field":"favorite","values":["true"]}]"#);
    let favorite_filtered = server
        .ok_json(&format!("/api/entities?filters={favorite_filters}"))
        .await;
    assert_eq!(favorite_filtered["total"], 1);
    assert_eq!(favorite_filtered["items"][0]["id"], "anime:Star Voyager");

    let not_favorite_filters = urlencoding::encode(r#"[{"field":"favorite","values":["false"]}]"#);
    let not_favorite_filtered = server
        .ok_json(&format!("/api/entities?filters={not_favorite_filters}"))
        .await;
    assert_eq!(not_favorite_filtered["total"], 1);
    assert_eq!(not_favorite_filtered["items"][0]["id"], "games:Moon Quest");

    let searched = server.ok_json("/api/entities?q=starlanes").await;
    assert_eq!(searched["total"], 1);
    assert_eq!(searched["items"][0]["id"], "anime:Star Voyager");

    let searched_title_language = server
        .ok_json(&format!(
            "/api/entities?q={}",
            urlencoding::encode("Lunar Errand")
        ))
        .await;
    assert_eq!(searched_title_language["total"], 1);
    assert_eq!(
        searched_title_language["items"][0]["id"],
        "games:Moon Quest"
    );

    let by_relation = server
        .ok_json(&format!(
            "/api/entities?relation={}",
            urlencoding::encode("franchise:Star Saga")
        ))
        .await;
    assert_eq!(by_relation["total"], 2);
    assert!(has_entity_title(&by_relation["items"], "星之航路"));
    assert!(has_entity_title(&by_relation["items"], "Moon Quest"));

    let detail = server
        .ok_json(&format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager")
        ))
        .await;
    assert_eq!(detail["entity"]["title"], "星之航路");
    assert_eq!(detail["entity"]["titles"]["zh"], "Star Voyager");
    assert_eq!(detail["entity"]["titles"]["en"], "A Voyage of Stars");
    assert_eq!(detail["entity"]["titles"]["title_original"], "星之航路");
    assert_eq!(detail["entity"]["path"], "Taxonomy/Anime/Star Voyager.md");
    assert_eq!(detail["relations"].as_array().unwrap().len(), 4);
    assert_eq!(relation_field_count(&detail["relations"], "daily-note"), 0);
    assert_eq!(relation_field_count(&detail["relations"], "body"), 2);
    assert!(has_entity_title(&detail["relatedEntities"], "Moon Quest"));
    assert!(has_entity_title(&detail["relatedEntities"], "Star Saga"));
    assert_eq!(
        unique_relation_target_count("anime:Star Voyager", &detail["relations"]),
        3
    );
    assert_eq!(detail["entity"]["relationCount"], 4);

    let (status, missing) = server
        .json(&format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Missing")
        ))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["error"], "Entity not found");
}

#[tokio::test]
async fn type_preset_endpoints_list_and_resolve() {
    let server = TestServer::new();

    // The catalog lists presets grouped into categories, with provider chips.
    let catalog = server.ok_json("/api/type-presets").await;
    let presets = catalog["presets"].as_array().unwrap();
    assert!(presets.iter().any(|p| p["id"] == "anime"));
    assert!(presets.iter().any(|p| p["id"] == "franchise"));
    let anime = presets.iter().find(|p| p["id"] == "anime").unwrap();
    assert_eq!(anime["category"], "watch");
    assert!(anime["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["id"] == "bangumi"));
    // Anime advertises franchise as a relation target (drives "pairs well with").
    assert!(anime["relationTargets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t == "franchise"));

    // Resolving anime + franchise together keeps the franchise link and produces
    // a home section per type — with no back-fills (both are new).
    let (status, resolved) = request_json(
        &server.app,
        Method::POST,
        "/api/type-presets/resolve",
        Some(json!({
            "presetIds": ["anime", "franchise"],
            "titleLanguage": "en"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resolved}");
    let types = resolved["types"].as_array().unwrap();
    assert_eq!(types.len(), 2);
    let anime_type = types.iter().find(|t| t["id"] == "anime").unwrap();
    assert!(anime_type["fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["field"] == "franchise" && f["relationType"] == "franchise"));
    // Title language stamped through.
    let title = anime_type["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"] == "title")
        .unwrap();
    assert_eq!(title["titleLanguage"], "en");
    // Only anime has a release/season date → one Home shelf; franchise (no date
    // field) is left out of the default Home.
    assert_eq!(resolved["homeSections"].as_array().unwrap().len(), 1);
    assert_eq!(resolved["homeSections"][0]["type"], "anime");
    assert!(resolved
        .get("backfills")
        .and_then(Value::as_array)
        .map(|b| b.is_empty())
        .unwrap_or(true));
}

#[tokio::test]
async fn entity_mutation_endpoints_edit_create_and_trash_markdown_files() {
    let server = TestServer::new();

    let detail = server
        .ok_json(&format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager")
        ))
        .await;
    let revision = detail["entity"]["revision"].as_str().unwrap();
    let updated = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager")
        ),
        Some(json!({
            "revision": revision,
            "frontmatter": {
                "status": "Completed",
                "progress": 12,
                "bgm_url": null
            },
            "body": "Updated body with [[Moon Quest]]."
        })),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);
    assert_eq!(updated.1["entity"]["frontmatter"]["status"], "Completed");
    assert_eq!(updated.1["entity"]["frontmatter"]["progress"], 12);
    assert!(updated.1["entity"]["frontmatter"].get("bgm_url").is_none());
    assert_eq!(
        updated.1["entity"]["body"],
        "Updated body with [[Moon Quest]]."
    );

    let stale = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager")
        ),
        Some(json!({
            "revision": revision,
            "frontmatter": {
                "status": "Watching"
            }
        })),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT);

    let rename_revision = updated.1["entity"]["revision"].as_str().unwrap();
    let renamed = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager")
        ),
        Some(json!({
            "revision": rename_revision,
            "renameTo": "  Star Voyager Renamed  "
        })),
    )
    .await;
    assert_eq!(renamed.0, StatusCode::OK, "{}", renamed.1);
    assert_eq!(renamed.1["entity"]["id"], "anime:Star Voyager Renamed");
    assert_eq!(renamed.1["entity"]["basename"], "Star Voyager Renamed");
    assert_eq!(
        renamed.1["entity"]["path"],
        "Taxonomy/Anime/Star Voyager Renamed.md"
    );

    let invalid_rename_revision = renamed.1["entity"]["revision"].as_str().unwrap();
    let invalid_rename = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager Renamed")
        ),
        Some(json!({
            "revision": invalid_rename_revision,
            "renameTo": "Bad:Name?"
        })),
    )
    .await;
    assert_eq!(invalid_rename.0, StatusCode::BAD_REQUEST);

    let created = request_json(
        &server.app,
        Method::POST,
        "/api/entities",
        Some(json!({
            "type": "games",
            "basename": "Solar Tactics",
            "frontmatter": {
                "title": "Solar Tactics",
                "status": "Backlog",
                "igdb_url": "https://www.igdb.com/games/solar-tactics"
            },
            "body": "A new strategy game."
        })),
    )
    .await;
    assert_eq!(created.0, StatusCode::OK, "{}", created.1);
    assert_eq!(created.1["entity"]["id"], "games:Solar Tactics");
    assert_eq!(
        created.1["entity"]["path"],
        "Taxonomy/Games/Solar Tactics.md"
    );

    let invalid_created = request_json(
        &server.app,
        Method::POST,
        "/api/entities",
        Some(json!({
            "type": "games",
            "basename": "Solar/Tactics.md",
            "frontmatter": {},
            "body": ""
        })),
    )
    .await;
    assert_eq!(invalid_created.0, StatusCode::BAD_REQUEST);

    let delete_revision = created.1["entity"]["revision"].as_str().unwrap();
    let deleted = request_json(
        &server.app,
        Method::DELETE,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("games:Solar Tactics")
        ),
        Some(json!({ "revision": delete_revision })),
    )
    .await;
    assert_eq!(deleted.0, StatusCode::OK, "{}", deleted.1);
    assert_eq!(deleted.1["deletedId"], "games:Solar Tactics");
    assert_eq!(deleted.1["backupPath"], ".trash/Solar Tactics.md");

    let entities = server.ok_json("/api/entities").await;
    assert_eq!(entities["total"], 4);
}

#[tokio::test]
async fn rename_repoints_inbound_wikilinks_in_managed_files() {
    let server = TestServer::new();

    // A user-curated list under `KizunaShelf/Lists/` links Star Voyager. Lists are
    // never indexed into the relation graph (read live from the VFS), so a rename
    // must sweep the directory directly. The aliased Moon Quest link must survive.
    write_file(
        &server.vault.join("KizunaShelf/Lists/Favorites.md"),
        "Personal favorites.\n\n- [[Star Voyager]]\n- [[Moon Quest|the quest]]\n",
    );

    // Star Voyager is linked from Moon Quest's body and from the daily note.
    let detail = server
        .ok_json(&format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager")
        ))
        .await;
    let revision = detail["entity"]["revision"].as_str().unwrap();

    let renamed = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager")
        ),
        Some(json!({
            "revision": revision,
            "renameTo": "Star Voyager Redux"
        })),
    )
    .await;
    assert_eq!(renamed.0, StatusCode::OK, "{}", renamed.1);
    assert_eq!(renamed.1["entity"]["id"], "anime:Star Voyager Redux");
    // Three inbound links across three files: Moon Quest's body, the daily note,
    // and the Favorites list page.
    assert_eq!(renamed.1["updatedLinks"]["files"], 3);
    assert_eq!(renamed.1["updatedLinks"]["links"], 3);

    // The raw files were rewritten in place (no YAML round-trip): the inbound
    // links now point at the new basename while the unrelated `[[Star Saga]]`
    // franchise link and the `[[Moon Quest|the quest]]` alias survive verbatim.
    let moon = fs::read_to_string(server.vault.join("Taxonomy/Games/Moon Quest.md")).unwrap();
    assert!(moon.contains("[[Star Voyager Redux]]"), "{moon}");
    assert!(!moon.contains("[[Star Voyager]]"), "{moon}");
    assert!(moon.contains("franchise: \"[[Star Saga]]\""), "{moon}");

    let daily = fs::read_to_string(server.vault.join("Daily Notes/2025-04-21.md")).unwrap();
    assert!(daily.contains("[[Star Voyager Redux]]"), "{daily}");
    assert!(
        daily.contains("[[Moon Quest|the quest]]"),
        "alias link preserved: {daily}"
    );

    // The list page (not in the relation graph — swept directly) was repointed
    // too, and its unrelated aliased Moon Quest link is untouched.
    let list = fs::read_to_string(server.vault.join("KizunaShelf/Lists/Favorites.md")).unwrap();
    assert!(list.contains("[[Star Voyager Redux]]"), "{list}");
    assert!(!list.contains("[[Star Voyager]]"), "{list}");
    assert!(list.contains("[[Moon Quest|the quest]]"), "{list}");

    // A plain (non-rename) update reports no link changes.
    let redux = server
        .ok_json(&format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager Redux")
        ))
        .await;
    let redux_revision = redux["entity"]["revision"].as_str().unwrap();
    let touched = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager Redux")
        ),
        Some(json!({ "revision": redux_revision, "frontmatter": { "favorite": false } })),
    )
    .await;
    assert_eq!(touched.0, StatusCode::OK, "{}", touched.1);
    assert!(
        touched.1["updatedLinks"].is_null(),
        "a non-rename update carries no updatedLinks: {}",
        touched.1
    );
}

#[tokio::test]
async fn external_search_lists_providers_without_querying_network_for_empty_searches() {
    let server = TestServer::new();

    let providers = server.ok_json("/api/external/search?type=anime&q=").await;
    assert_eq!(providers["items"].as_array().unwrap().len(), 0);
    assert!(providers["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["id"] == "bangumi" && provider["enabled"] == true));
    assert!(providers["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["id"] == "igdb"));

    let unknown = server
        .json("/api/external/search?provider=missing&q=Star")
        .await;
    assert_eq!(unknown.0, StatusCode::BAD_REQUEST);
    assert_eq!(unknown.1["error"], "Unknown external provider");

    // Cross-type search: `type=all` (and an omitted type) is now valid and lists
    // providers merged across every configured type. Probed with an empty query so
    // it stays offline.
    let all_type = server.ok_json("/api/external/search?type=all&q=").await;
    assert_eq!(all_type["items"].as_array().unwrap().len(), 0);
    assert!(all_type["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["id"] == "bangumi" && provider["enabled"] == true));

    let missing_type = server.ok_json("/api/external/search?q=").await;
    assert!(missing_type["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["id"] == "bangumi"));

    let unknown_type = server
        .json("/api/external/search?type=animation&q=Star")
        .await;
    assert_eq!(unknown_type.0, StatusCode::BAD_REQUEST);
    assert_eq!(unknown_type.1["error"], "Unknown entity type");
}

#[tokio::test]
async fn quick_add_creates_entity_from_candidate_and_dedupes_on_second_add() {
    let server = TestServer::new();

    // A Bangumi candidate for the anime type. No `coverUrl` and the anime type has
    // no episodes section, so the whole flow stays offline. The zh title carries a
    // forbidden `/` to exercise full-width filename derivation.
    let candidate = json!({
        "provider": "bangumi",
        "sourceId": "998877",
        "url": "https://bgm.tv/subject/998877",
        "title": "Fate/stay night",
        "titles": { "zh": "命运之夜/UBW", "en": "Fate stay night" },
        "metadata": {}
    });

    let (status, created) = request_json(
        &server.app,
        Method::POST,
        "/api/external/quick-add",
        Some(json!({ "type": "anime", "candidate": candidate.clone() })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["alreadyExisted"], false);
    assert!(
        created["episodes"].is_null(),
        "anime has no episodes section"
    );
    assert_eq!(created["cover"].as_array().map(Vec::len).unwrap_or(0), 0);

    // The externalRef field is filled with the candidate URL, the title comes from
    // the zh title (the filename language), and the basename has the `/` replaced
    // by its full-width form.
    let entity = &created["entity"];
    assert_eq!(
        entity["frontmatter"]["bgm_url"],
        "https://bgm.tv/subject/998877"
    );
    assert_eq!(entity["frontmatter"]["title"], "命运之夜/UBW");
    assert_eq!(entity["basename"], "命运之夜／UBW");
    let entity_id = entity["id"].as_str().unwrap().to_string();

    // Re-adding the same candidate finds it via the stored bgm_url and returns the
    // existing entity instead of creating a duplicate.
    let (status, again) = request_json(
        &server.app,
        Method::POST,
        "/api/external/quick-add",
        Some(json!({ "type": "anime", "candidate": candidate })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["alreadyExisted"], true);
    assert_eq!(again["entity"]["id"], entity_id);
}

#[tokio::test]
async fn quick_add_is_forbidden_in_read_only_mode() {
    let server = TestServer::read_only();
    let (status, body) = request_json(
        &server.app,
        Method::POST,
        "/api/external/quick-add",
        Some(json!({
            "type": "anime",
            "candidate": {
                "provider": "bangumi",
                "sourceId": "1",
                "url": "https://bgm.tv/subject/1",
                "title": "Whatever",
                "titles": {},
                "metadata": {}
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

#[tokio::test]
async fn calendar_endpoints_include_metadata_and_daily_notes_from_temp_vault() {
    let server = TestServer::new();

    let calendar = server
        .ok_json("/api/calendar?year=2025&month=4&source=all")
        .await;
    assert_eq!(calendar["totals"]["entries"], 4);
    assert_eq!(calendar["totals"]["taxonomy"], 2);
    assert_eq!(calendar["totals"]["dailyNotes"], 2);
    assert_eq!(calendar["totals"]["daysWithEntries"], 3);

    let april_21 = calendar["days"]
        .as_array()
        .unwrap()
        .iter()
        .find(|day| day["date"] == "2025-04-21")
        .unwrap();
    assert_eq!(april_21["counts"]["dailyNotes"], 2);
    assert!(has_entity_title(&april_21["entries"], "星之航路"));
    assert!(has_entity_title(&april_21["entries"], "Moon Quest"));

    let taxonomy_only = server
        .ok_json("/api/calendar?year=2025&month=4&source=taxonomy&type=anime")
        .await;
    assert_eq!(taxonomy_only["filters"]["type"], "anime");
    assert_eq!(taxonomy_only["totals"]["entries"], 1);
    assert_eq!(taxonomy_only["totals"]["dailyNotes"], 0);

    let daily_note_only = server
        .ok_json("/api/calendar?year=2025&month=4&source=daily-note&type=games")
        .await;
    assert_eq!(daily_note_only["totals"]["entries"], 1);
    assert_eq!(
        daily_note_only["days"][20]["entries"][0]["notePath"],
        "Daily Notes/2025-04-21.md"
    );

    // The activity feed surfaces the same dated entities, grouped by (date, entity).
    let activity = server.ok_json("/api/activity").await;
    let items = activity["items"].as_array().unwrap();
    assert!(!items.is_empty());
    // Star Voyager's completed date stamp shows up among its activity items (it
    // appears in several — a daily-note mention and date stamps on other dates).
    assert!(items
        .iter()
        .filter(|item| item["entity"]["id"] == "anime:Star Voyager")
        .flat_map(|item| item["entries"].as_array().unwrap())
        .any(|entry| entry["source"] == "taxonomy"
            && entry["dateField"] == "complete_date"
            && entry["role"] == "completed"));

    let game_activity = server.ok_json("/api/activity?type=games").await;
    assert!(game_activity["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["entity"]["type"] == "games"));

    let dates = server
        .ok_json(&format!(
            "/api/entities/{}/dates",
            urlencoding::encode("anime:Star Voyager")
        ))
        .await;
    assert_eq!(dates["entityId"], "anime:Star Voyager");
    assert_eq!(dates["totals"]["metadata"], 2);
    assert_eq!(dates["totals"]["dailyNotes"], 1);
    assert_eq!(dates["totals"]["snippets"], 1);
    assert_eq!(dates["metadata"][0]["date"], "2025-04-20");
    assert_eq!(dates["dailyNotes"][0]["date"], "2025-04-21");
    assert_eq!(dates["dailyNotes"][0]["snippets"][0]["heading"], "Watched");
}

#[tokio::test]
async fn upcoming_lists_future_dates_and_honors_client_today() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "games", "label": "Games", "path": "Games",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                    { "field": "cover_url", "fieldType": "image" },
                    { "field": "release_date", "fieldType": "date", "dateRole": "planning" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Games/PRAGMATA.md"),
        "---\ntitle: PRAGMATA\ncover_url: https://example.com/p.jpg\nrelease_date: 2030-06-01\n---\n",
    );
    let app = inline_router(&vault, true, true);

    // Before the release: it's upcoming, carrying the entity title, cover, and the
    // planning date field it came from. `today` is the client's local date.
    let ahead = request_json(
        &app,
        Method::GET,
        "/api/upcoming?today=2030-01-01&months=12",
        None,
    )
    .await;
    assert_eq!(ahead.0, StatusCode::OK, "{}", ahead.1);
    let items = ahead.1["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{}", ahead.1);
    assert_eq!(items[0]["date"], "2030-06-01");
    assert_eq!(items[0]["entity"]["title"], "PRAGMATA");
    assert_eq!(items[0]["entity"]["image"], "https://example.com/p.jpg");
    let entry = &items[0]["entries"][0];
    assert_eq!(entry["source"], "taxonomy");
    assert_eq!(entry["dateField"], "release_date");
    assert_eq!(entry["role"], "planning");

    // After that date, it's no longer upcoming — the client's `today` drives it.
    let after = request_json(
        &app,
        Method::GET,
        "/api/upcoming?today=2030-08-01&months=12",
        None,
    )
    .await;
    assert_eq!(after.1["items"].as_array().unwrap().len(), 0, "{}", after.1);
}

#[tokio::test]
async fn log_endpoint_writes_a_daily_note_line() {
    let server = TestServer::new();
    let path = format!(
        "/api/entities/{}/log",
        urlencoding::encode("anime:Star Voyager")
    );

    // Dry run: previews the line + the completed-date it would stamp; writes nothing.
    let (status, preview) = request_json(
        &server.app,
        Method::POST,
        &format!("{path}?dryRun=true"),
        Some(json!({ "kind": "completed", "note": "rewatch done", "date": "2024-08-20" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["dryRun"], true);
    assert_eq!(preview["section"], "Log");
    let line = preview["line"].as_str().unwrap();
    assert!(line.contains("[[Star Voyager]]"), "{line}");
    assert!(line.contains("rewatch done #Anime"), "{line}");
    assert_eq!(preview["willStampDate"]["field"], "complete_date");

    // Real write (kind=progress → daily-note line only, no entity mutation, so no
    // revision needed), then the same log again is idempotent.
    let (status, written) = request_json(
        &server.app,
        Method::POST,
        &path,
        Some(json!({ "kind": "progress", "note": "rewatch done", "date": "2024-08-20" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{written}");
    assert_eq!(written["dryRun"], false);
    assert_eq!(written["lineAlreadyPresent"], false);

    let (_, again) = request_json(
        &server.app,
        Method::POST,
        &path,
        Some(json!({ "kind": "progress", "note": "rewatch done", "date": "2024-08-20" })),
    )
    .await;
    assert_eq!(again["lineAlreadyPresent"], true, "{again}");
}

#[tokio::test]
async fn log_endpoint_applies_and_reverses_date_stamp() {
    let server = TestServer::new();
    let entity = urlencoding::encode("anime:Star Voyager");
    let log = format!("/api/entities/{entity}/log");

    async fn revision_of(server: &TestServer, entity: &str) -> String {
        server.ok_json(&format!("/api/entities/{entity}")).await["entity"]["revision"]
            .as_str()
            .unwrap()
            .to_string()
    }

    // Complete it on a fixed date: stamps `complete_date`, writes a daily-note line.
    let revision = revision_of(&server, &entity).await;
    let (status, added) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "add", "kind": "completed", "date": "2024-08-20",
            "note": "fin", "revision": revision,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert_eq!(added["willStampDate"]["field"], "complete_date");
    assert_eq!(added["willStampDate"]["value"], "2024-08-20");
    // The returned entity detail carries the new stamp.
    assert_eq!(
        added["entity"]["entity"]["frontmatter"]["complete_date"], "2024-08-20",
        "{added}"
    );
    assert_eq!(added["lineAlreadyPresent"], false);
    let note_path = added["notePath"].as_str().unwrap().to_string();

    // Missing revision on a mutating log is rejected.
    let (status, _) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({ "op": "add", "kind": "completed", "date": "2024-08-20" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Remove: clears the stamp (it equals the log date) and removes the exact line.
    let revision = revision_of(&server, &entity).await;
    let (status, removed) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "remove", "kind": "completed", "date": "2024-08-20",
            "note": "fin", "revision": revision,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(removed["lineMatched"], true); // the exact line was found and removed
    assert!(
        removed["entity"]["entity"]["frontmatter"]["complete_date"].is_null(),
        "{removed}"
    );
    let _ = note_path;
}

#[tokio::test]
async fn log_endpoint_flips_status_monotonically_and_never_reverts_on_remove() {
    let server = TestServer::new();
    let entity = urlencoding::encode("anime:Star Voyager");
    let log = format!("/api/entities/{entity}/log");

    async fn revision_of(server: &TestServer, entity: &str) -> String {
        server.ok_json(&format!("/api/entities/{entity}")).await["entity"]["revision"]
            .as_str()
            .unwrap()
            .to_string()
    }

    // Star Voyager starts as `Watching` (ongoing). A dry-run `completed` log
    // previews the flip to the mapped write value `Completed` without writing.
    let (status, preview) = request_json(
        &server.app,
        Method::POST,
        &format!("{log}?dryRun=true"),
        Some(json!({ "op": "add", "kind": "completed", "date": "2024-08-20" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["willFlipStatus"]["field"], "status");
    assert_eq!(preview["willFlipStatus"]["value"], "Completed");
    assert_eq!(preview["willFlipStatus"]["canonical"], "completed");

    // Real completion flips the status field and returns the refreshed entity.
    let revision = revision_of(&server, &entity).await;
    let (status, done) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "add", "kind": "completed", "date": "2024-08-20", "revision": revision,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(
        done["entity"]["entity"]["frontmatter"]["status"], "Completed",
        "{done}"
    );

    // A `started` log now would demote completed→ongoing — it must not. No flip is
    // planned, and status stays `Completed`.
    let (status, started) = request_json(
        &server.app,
        Method::POST,
        &format!("{log}?dryRun=true"),
        Some(json!({ "op": "add", "kind": "started", "date": "2024-08-21" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert!(started["willFlipStatus"].is_null(), "{started}");

    // Removing the completion log clears the date stamp but leaves status alone —
    // a status flip has no safe inverse.
    let revision = revision_of(&server, &entity).await;
    let (status, removed) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "remove", "kind": "completed", "date": "2024-08-20", "revision": revision,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert!(removed["willFlipStatus"].is_null(), "{removed}");
    assert_eq!(
        removed["entity"]["entity"]["frontmatter"]["status"], "Completed",
        "status is not reverted by remove: {removed}"
    );
}

#[tokio::test]
async fn log_started_resumes_a_paused_entity() {
    let server = TestServer::new();
    let id = urlencoding::encode("anime:Star Voyager");
    let entity_path = format!("/api/entities/{id}");
    let log = format!("{entity_path}/log");

    async fn revision_of(server: &TestServer, path: &str) -> String {
        server.ok_json(path).await["entity"]["revision"]
            .as_str()
            .unwrap()
            .to_string()
    }

    // Pause it first (Paused is off the progression chain).
    let revision = revision_of(&server, &entity_path).await;
    let (status, _) = request_json(
        &server.app,
        Method::POST,
        &entity_path,
        Some(json!({ "revision": revision, "frontmatter": { "status": "Paused" }, "body": "" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // A `started` log resumes it: paused promotes forward to the `ongoing` value.
    let revision = revision_of(&server, &entity_path).await;
    let (status, done) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "add", "kind": "started", "date": "2024-08-20", "revision": revision,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["willFlipStatus"]["canonical"], "ongoing", "{done}");
    assert_eq!(
        done["entity"]["entity"]["frontmatter"]["status"], "Watching",
        "{done}"
    );
}

#[tokio::test]
async fn log_conflict_leaves_no_partial_write() {
    let server = TestServer::new();
    let entity = urlencoding::encode("anime:Star Voyager");
    let log = format!("/api/entities/{entity}/log");

    // A `completed` log stamps a frontmatter date *and* writes a daily-note line —
    // two files that can't be committed atomically. A stale revision must fail with
    // neither applied: the preflight rejects it before the note is touched, so the
    // request is all-or-nothing rather than leaving a half-written state.
    let (status, _) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "add", "kind": "completed", "date": "2029-03-14",
            "note": "fin", "revision": "stale-revision",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // The daily note was never written: a dry-run still reports the note as new and
    // the line as not-yet-present — both would be false had the failed attempt
    // written anything.
    let (status, preview) = request_json(
        &server.app,
        Method::POST,
        &format!("{log}?dryRun=true"),
        Some(json!({ "op": "add", "kind": "completed", "date": "2029-03-14", "note": "fin" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["noteWillBeCreated"], true, "{preview}");
    assert_eq!(preview["lineAlreadyPresent"], false, "{preview}");
}

#[tokio::test]
async fn settings_save_and_read_vault_config() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let app = inline_router(&vault, true, true);

    let missing = request_json(&app, Method::GET, "/api/settings/config", None).await;
    assert_eq!(missing.0, StatusCode::OK);
    assert_eq!(missing.1["vaultExists"], false);

    let config = json!({
        "taxonomyRoot": "Taxonomy",
        "dailyNotes": {
            "paths": ["Daily Notes"],
            "dateFormat": "YYYY-MM-DD"
        },
        "home": {
            "title": "Settings Fixture",
            "sections": []
        },
        "types": [
            {
                "id": "anime",
                "label": "Anime",
                "icon": "📺",
                "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                    { "field": "title_en", "fieldType": "title", "titleLanguage": "en" },
                    { "field": "cover_url", "fieldType": "image" },
                    { "field": "status", "fieldType": "enum", "enumOptions": ["Backlog", "Watching", "Completed"] },
                    { "field": "season", "fieldType": "season", "dateRole": "planning", "seasonLanguage": "zh" },
                    { "field": "complete_date", "fieldType": "date", "dateRole": "completed" },
                        { "field": "bgm_url", "fieldType": "externalRef", "externalRef": "bangumi" },
                        { "field": "franchise", "fieldType": "relation", "relationType": "franchise" },
                        { "field": "studio", "fieldType": "relation", "relationType": "studio" }
                ]
            }
        ]
    });

    let saved = request_json(
        &app,
        Method::PUT,
        "/api/settings/config",
        Some(vault_settings_body(&config)),
    )
    .await;
    assert_eq!(saved.0, StatusCode::OK);
    assert_eq!(saved.1["vaultExists"], true);
    assert_eq!(saved.1["vault"]["types"].as_array().unwrap().len(), 1);
    assert!(vault.join("KizunaShelf/config.yaml").is_file());

    let read_back = request_json(&app, Method::GET, "/api/settings/config", None).await;
    assert_eq!(read_back.0, StatusCode::OK);
    assert_eq!(read_back.1["vaultExists"], true);
    assert_eq!(read_back.1["vault"]["home"]["title"], "Settings Fixture");
}

#[tokio::test]
async fn home_sections_evaluate_smart_list_criteria() {
    let server = TestServer::new();
    let app = &server.app;

    // Add a criteria-driven section next to the fixture sections: the
    // smart-list rule model, stored structurally in the vault config.
    let settings = request_json(app, Method::GET, "/api/settings/config", None).await;
    assert_eq!(settings.0, StatusCode::OK, "{}", settings.1);
    let mut vault_config = settings.1["vault"].clone();
    vault_config["home"]["sections"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": "watching-favorites",
            "title": "Watching Favorites",
            "type": "anime",
            "criteria": {
                "conjunction": "all",
                "rules": [
                    { "kind": "compare", "field": "status", "op": "eq", "value": "Watching" },
                    { "kind": "compare", "field": "favorite", "op": "eq", "boolean": true }
                ]
            },
            "limit": 4
        }));
    let saved = request_json(
        app,
        Method::PUT,
        "/api/settings/config",
        Some(vault_settings_body(&vault_config)),
    )
    .await;
    assert_eq!(saved.0, StatusCode::OK, "{}", saved.1);

    // The criteria survive the strict config round-trip…
    let read_back = request_json(app, Method::GET, "/api/settings/config", None).await;
    let section = &read_back.1["vault"]["home"]["sections"][3];
    assert_eq!(section["criteria"]["rules"].as_array().unwrap().len(), 2);

    // …and the section evaluates through the smart-list engine: Star Voyager
    // is Watching + favorite. The fixture sections keep working beside it.
    let home = request_json(app, Method::GET, "/api/home", None).await;
    assert_eq!(home.0, StatusCode::OK, "{}", home.1);
    let sections = home.1["sections"].as_array().unwrap();
    let section = sections
        .iter()
        .find(|section| section["id"] == "watching-favorites")
        .expect("criteria section present");
    assert_eq!(section["total"], 1);
    assert_eq!(section["items"][0]["id"], "anime:Star Voyager");
    assert_eq!(
        section["criteria"]["rules"][0]["value"], "Watching",
        "criteria echoed on the response"
    );
    assert_eq!(
        sections
            .iter()
            .find(|section| section["id"] == "recent-anime")
            .map(|section| &section["total"]),
        Some(&json!(1)),
        "existing criteria section unchanged"
    );

    // A none-conjunction excludes: no anime that is Watching → only non-watching.
    let mut vault_config = read_back.1["vault"].clone();
    vault_config["home"]["sections"][3]["criteria"] = json!({
        "conjunction": "none",
        "rules": [
            { "kind": "compare", "field": "status", "op": "eq", "value": "Watching" }
        ]
    });
    let saved = request_json(
        app,
        Method::PUT,
        "/api/settings/config",
        Some(vault_settings_body(&vault_config)),
    )
    .await;
    assert_eq!(saved.0, StatusCode::OK, "{}", saved.1);
    let home = request_json(app, Method::GET, "/api/home", None).await;
    let section = home.1["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|section| section["id"] == "watching-favorites")
        .expect("criteria section present");
    // The fixture vault's only anime is Watching, so none-of matches nothing.
    assert_eq!(section["total"], 0);
}

#[tokio::test]
async fn raw_settings_config_round_trips_yaml_verbatim() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let app = inline_router(&vault, true, true);

    let missing = request_json(&app, Method::GET, "/api/settings/config/raw", None).await;
    assert_eq!(missing.0, StatusCode::OK);
    assert_eq!(missing.1["vaultExists"], false);
    assert_eq!(missing.1["content"], "");

    // Comments and exact formatting must survive the round trip (the point of the
    // raw editor): the bytes are written verbatim, not re-serialized.
    let yaml = "# my vault\ntaxonomyRoot: Taxonomy\ntypes: []\n";
    let saved = request_json(
        &app,
        Method::PUT,
        "/api/settings/config/raw",
        Some(json!({ "content": yaml })),
    )
    .await;
    assert_eq!(saved.0, StatusCode::OK);
    assert_eq!(saved.1["vaultExists"], true);
    assert_eq!(saved.1["content"], yaml);
    assert_eq!(
        std::fs::read_to_string(vault.join("KizunaShelf/config.yaml")).unwrap(),
        yaml
    );

    let read_back = request_json(&app, Method::GET, "/api/settings/config/raw", None).await;
    assert_eq!(read_back.1["content"], yaml);
}

#[tokio::test]
async fn raw_settings_config_rejects_unknown_fields() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let app = inline_router(&vault, true, true);

    let rejected = request_json(
        &app,
        Method::PUT,
        "/api/settings/config/raw",
        Some(json!({ "content": "taxonomyRoot: Taxonomy\ntypes: []\nmystery: 42\n" })),
    )
    .await;
    assert_eq!(rejected.0, StatusCode::BAD_REQUEST);
    assert!(rejected.1["error"]
        .as_str()
        .unwrap()
        .contains("unknown config field"));
    // A rejected write must not touch the file.
    assert!(!vault.join("KizunaShelf/config.yaml").exists());
}

#[tokio::test]
async fn raw_settings_config_rejects_malformed_config() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let app = inline_router(&vault, true, true);

    // Missing the required `types` field.
    let missing_required = request_json(
        &app,
        Method::PUT,
        "/api/settings/config/raw",
        Some(json!({ "content": "taxonomyRoot: Taxonomy\n" })),
    )
    .await;
    assert_eq!(missing_required.0, StatusCode::BAD_REQUEST);

    // Not valid YAML at all.
    let broken = request_json(
        &app,
        Method::PUT,
        "/api/settings/config/raw",
        Some(json!({ "content": "taxonomyRoot: [unterminated\n" })),
    )
    .await;
    assert_eq!(broken.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn raw_settings_config_write_is_disabled_in_read_only_mode() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let app = inline_router(&vault, false, false);

    let saved = request_json(
        &app,
        Method::PUT,
        "/api/settings/config/raw",
        Some(json!({ "content": "taxonomyRoot: Taxonomy\ntypes: []\n" })),
    )
    .await;
    assert_eq!(saved.0, StatusCode::FORBIDDEN);
    assert_eq!(saved.1["error"], "Settings writes are disabled");
}

#[tokio::test]
async fn settings_config_rejects_paths_that_escape_the_vault_root() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let app = inline_router(&vault, true, true);

    let escaped_taxonomy = request_json(
        &app,
        Method::PUT,
        "/api/settings/config",
        Some(vault_settings_body(&json!({
                "taxonomyRoot": "../outside",
            "types": []
        }))),
    )
    .await;
    assert_eq!(escaped_taxonomy.0, StatusCode::BAD_REQUEST);
    assert!(escaped_taxonomy.1["error"]
        .as_str()
        .unwrap()
        .contains("taxonomyRoot cannot contain parent directory components"));

    let absolute_type_path = request_json(
        &app,
        Method::PUT,
        "/api/settings/config",
        Some(vault_settings_body(&json!({
                "taxonomyRoot": "Taxonomy",
            "types": [
                {
                    "id": "anime",
                    "label": "Anime",
                    "path": "/tmp/anime",
                    "fields": []
                }
            ]
        }))),
    )
    .await;
    assert_eq!(absolute_type_path.0, StatusCode::BAD_REQUEST);
    assert!(absolute_type_path.1["error"]
        .as_str()
        .unwrap()
        .contains("type path for anime must be relative to vaultRoot"));
}

#[tokio::test]
async fn path_suggestions_list_vault_directories_and_omit_hidden() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(vault.join("Taxonomy/Anime")).unwrap();
    std::fs::create_dir_all(vault.join("Assets")).unwrap();
    std::fs::create_dir_all(vault.join(".obsidian")).unwrap();
    let app = inline_router(&vault, true, true);

    let names = |response: &serde_json::Value| -> Vec<String> {
        response["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect()
    };

    // An empty prefix lists the vault root; dotfile folders are hidden.
    let root = request_json(
        &app,
        Method::GET,
        "/api/settings/path-suggestions?path=",
        None,
    )
    .await;
    assert_eq!(root.0, StatusCode::OK);
    let root_names = names(&root.1);
    assert!(root_names.iter().any(|name| name == "Taxonomy"));
    assert!(root_names.iter().any(|name| name == "Assets"));
    assert!(root_names.iter().all(|name| !name.contains(".obsidian")));

    // A vault-relative prefix lists matching subdirectories, returned vault-relative.
    let nested = request_json(
        &app,
        Method::GET,
        "/api/settings/path-suggestions?path=Taxonomy/",
        None,
    )
    .await;
    assert!(names(&nested.1).iter().any(|name| name == "Taxonomy/Anime"));

    // `base` roots the listing (a type's folder path is taxonomy-relative):
    // suggestions come back relative to `base`, not the vault root.
    let based = request_json(
        &app,
        Method::GET,
        "/api/settings/path-suggestions?base=Taxonomy&path=An",
        None,
    )
    .await;
    assert_eq!(based.0, StatusCode::OK);
    assert!(names(&based.1).iter().any(|name| name == "Anime"));
    assert!(names(&based.1)
        .iter()
        .all(|name| !name.starts_with("Taxonomy/")));

    // An explicit leading dot reveals the hidden folder.
    let typed = request_json(
        &app,
        Method::GET,
        "/api/settings/path-suggestions?path=.o",
        None,
    )
    .await;
    assert!(names(&typed.1).iter().any(|name| name == ".obsidian"));

    // Traversal escapes are rejected outright — never the host filesystem.
    let escape = request_json(
        &app,
        Method::GET,
        "/api/settings/path-suggestions?path=../",
        None,
    )
    .await;
    assert_eq!(escape.0, StatusCode::OK);
    assert!(names(&escape.1).is_empty());
}

#[tokio::test]
async fn settings_mutation_endpoints_can_be_disabled() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_fixture_vault(&vault);
    let app = inline_router(&vault, false, false);
    let config = json!({
        "taxonomyRoot": "Taxonomy",
        "types": []
    });

    let saved = request_json(
        &app,
        Method::PUT,
        "/api/settings/config",
        Some(vault_settings_body(&config)),
    )
    .await;
    assert_eq!(saved.0, StatusCode::FORBIDDEN);
    assert_eq!(saved.1["error"], "Settings writes are disabled");

    let suggestions = request_json(
        &app,
        Method::GET,
        "/api/settings/path-suggestions?path=.",
        None,
    )
    .await;
    assert_eq!(suggestions.0, StatusCode::FORBIDDEN);
    assert_eq!(suggestions.1["error"], "Path suggestions are disabled");
}

#[tokio::test]
async fn content_mutation_endpoints_can_be_disabled() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_fixture_vault(&vault);
    let config = json!({
        "taxonomyRoot": "Taxonomy",
        "types": [
            {
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                    { "field": "status", "fieldType": "enum", "enumOptions": ["Backlog", "Watching", "Completed"] }
                ]
            }
        ]
    });
    write_vault_config(&vault, &config);
    let app = inline_router(&vault, true, false);

    let capabilities = request_json(&app, Method::GET, "/api/capabilities", None).await;
    assert_eq!(capabilities.0, StatusCode::OK);
    assert_eq!(capabilities.1["contentWritable"], false);

    let detail = request_json(
        &app,
        Method::GET,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager")
        ),
        None,
    )
    .await;
    assert_eq!(detail.0, StatusCode::OK);
    let revision = detail.1["entity"]["revision"].as_str().unwrap();
    let updated = request_json(
        &app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager")
        ),
        Some(json!({
            "revision": revision,
            "frontmatter": {
                "status": "Completed"
            }
        })),
    )
    .await;
    assert_eq!(updated.0, StatusCode::FORBIDDEN);
    assert_eq!(updated.1["error"], "Content writes are disabled");
}

impl TestServer {
    fn new() -> Self {
        Self::build(true)
    }

    fn read_only() -> Self {
        Self::build(false)
    }

    fn build(content_writable: bool) -> Self {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        write_fixture_vault(&vault);
        let config = json!({
                "taxonomyRoot": "Taxonomy",
            "dailyNotes": {
                "paths": ["Daily Notes"],
                "dateFormat": "YYYY-MM-DD"
            },
            "home": {
                "title": "Fixture Home",
                "sections": [
                    {
                        "id": "recent-anime",
                        "title": "Recent Anime",
                        "type": "anime",
                        "criteria": {
                            "conjunction": "all",
                            "rules": [{ "kind": "compare", "field": "status", "op": "eq", "value": "Watching" }]
                        },
                        "limit": 4,
                        "sort": "title",
                        "direction": "asc"
                    },
                    {
                        "id": "games",
                        "title": "Games",
                        "type": "games",
                        "criteria": {
                            "conjunction": "all",
                            "rules": [{ "kind": "compare", "field": "status", "op": "eq", "value": "Playing" }]
                        },
                        "limit": 4,
                        "sort": "title",
                        "direction": "asc"
                    },
                    {
                        "id": "completed-anime",
                        "title": "Completed Anime",
                        "type": "anime",
                        "criteria": {
                            "conjunction": "all",
                            "rules": [{ "kind": "compare", "field": "status", "op": "eq", "value": "Completed" }]
                        },
                        "limit": 4,
                        "sort": "title",
                        "direction": "asc"
                    }
                ]
            },
            "types": [
                {
                    "id": "anime",
                    "label": "Anime",
                    "icon": "📺",
                    "path": "Anime",
                    "filename": { "titleLanguage": "zh" },
                    "fields": [
                        { "field": "id", "fieldType": "id", "displayName": "ID" },
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                        { "field": "title_en", "fieldType": "title", "displayName": "Title (English)", "titleLanguage": "en" },
                        { "field": "title_original", "fieldType": "title", "displayName": "Title (Original)", "titleRole": "original" },
                        { "field": "cover_url", "fieldType": "image", "displayName": "Cover" },
                        { "field": "status", "fieldType": "enum", "displayName": "Status", "enumOptions": ["Backlog", "Watching", "Completed", "Paused", "Dropped"], "enumRole": "status", "statusValues": { "ongoing": ["Watching"], "paused": ["Paused"], "completed": ["Completed"], "dropped": ["Dropped"] } },
                        { "field": "season", "fieldType": "season", "displayName": "Season", "dateRole": "planning", "seasonLanguage": "zh" },
                        { "field": "complete_date", "fieldType": "date", "displayName": "Completed date", "dateRole": "completed" },
                        { "field": "favorite", "fieldType": "bool", "displayName": "Favorite" },
                        { "field": "bgm_url", "fieldType": "externalRef", "displayName": "BGM", "externalRef": "bangumi" },
                        { "field": "franchise", "fieldType": "relation", "displayName": "Franchise", "relationType": "franchise" },
                        { "field": "studio", "fieldType": "relation", "displayName": "Studio", "relationType": "studio" }
                    ],
                    "log": { "lineFormat": "- [[{title}]] {note} #Anime" }
                },
                {
                    "id": "games",
                    "label": "Games",
                    "path": "Games",
                    "filename": { "titleLanguage": "zh" },
                    "fields": [
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                        { "field": "title_en", "fieldType": "title", "displayName": "Title (English)", "titleLanguage": "en" },
                        { "field": "cover_url", "fieldType": "image", "displayName": "Cover" },
                        { "field": "status", "fieldType": "enum", "displayName": "Status", "enumOptions": ["Backlog", "Playing", "Completed", "Paused", "Dropped"] },
                        { "field": "favorite", "fieldType": "bool", "displayName": "Favorite" },
                        { "field": "release_date", "fieldType": "date", "displayName": "Release date", "dateRole": "planning" },
                        { "field": "igdb_url", "fieldType": "externalRef", "displayName": "IGDB", "externalRef": "igdb" },
                        { "field": "franchise", "fieldType": "relation", "displayName": "Franchise", "relationType": "franchise" },
                        { "field": "developer", "fieldType": "relation", "displayName": "Developer", "relationType": "developer" },
                        { "field": "genres", "fieldType": "enumList", "displayName": "Genres", "enumOptions": ["Adventure", "Strategy", "RPG"] }
                    ]
                },
                {
                    "id": "franchise",
                    "label": "Franchise",
                    "path": "Franchise",
                    "filename": { "titleLanguage": "zh" },
                    "fields": [
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                        { "field": "related", "fieldType": "relation", "displayName": "Related", "relationType": "related" }
                    ]
                },
                {
                    "id": "music",
                    "label": "Music",
                    "path": "Music",
                    "filename": {},
                    "fields": [
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                        { "field": "release_date", "fieldType": "date", "displayName": "Release date", "dateRole": "planning" },
                        { "field": "musicbrainz_url", "fieldType": "externalRef", "displayName": "MusicBrainz", "externalRef": "musicbrainz" }
                    ]
                }
            ]
        });
        write_vault_config(&vault, &config);

        Self {
            app: inline_router(&vault, true, content_writable),
            vault,
            _temp: temp,
        }
    }

    async fn ok_json(&self, path: &str) -> Value {
        let (status, value) = self.json(path).await;
        assert_eq!(status, StatusCode::OK, "{path}: {value}");
        value
    }

    async fn json(&self, path: &str) -> (StatusCode, Value) {
        request_json(&self.app, Method::GET, path, None).await
    }
}

/// Writes a vault config (the schema) to `<vault_root>/KizunaShelf/config.yaml`.
/// The vault root is owned by the runtime, so it is passed separately rather than
/// embedded in the config object.
fn write_vault_config(vault_root: &Path, config: &Value) {
    write_yaml_file(&vault_root.join("KizunaShelf/config.yaml"), config);
}

fn write_yaml_file(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_yaml::to_string(value).unwrap()).unwrap();
}

/// Wraps a vault config object in the `{ vault }` PUT `/api/settings/config` body
/// (the vault root + write mode are owned by the runtime and never sent).
fn vault_settings_body(config: &Value) -> Value {
    json!({ "vault": config.clone() })
}

async fn request_json(
    app: &Router,
    method: Method,
    path: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let body = payload
        .map(|value| body::Body::from(serde_json::to_vec(&value).unwrap()))
        .unwrap_or_else(body::Body::empty);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "failed to parse JSON from {path}: {error}\n{}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, value)
}

fn write_fixture_vault(vault: &Path) {
    write_file(
        &vault.join("Taxonomy/Anime/Star Voyager.md"),
        r#"---
title: Star Voyager
title_en: A Voyage of Stars
title_original: 星之航路
status: Watching
favorite: true
season: "2025"
complete_date: 2025-04-20
cover_url: https://img.example/star.jpg
bgm_url: https://bgm.example/star
franchise: "[[Star Saga]]"
studio: "[[Nova Studio]]"
---
## Summary
Star Voyager follows a crew crossing old starlanes.

It shares continuity with [[Moon Quest]].
"#,
    );
    write_file(
        &vault.join("Taxonomy/Games/Moon Quest.md"),
        r#"---
title: Moon Quest
title_en: Lunar Errand
status: Playing
favorite: false
genres: [Adventure, Strategy]
release_date: 2025-04-05
igdb_url: https://igdb.example/moon
franchise: "[[Star Saga]]"
developer: "[[Orbit Dev]]"
---
Moon Quest is a tactical adventure that references [[Star Voyager]].
"#,
    );
    write_file(
        &vault.join("Taxonomy/Franchise/Star Saga.md"),
        r#"---
title: Star Saga
related: "[[Moon Quest]]"
---
The shared setting for the fixture.
"#,
    );
    write_file(
        &vault.join("Taxonomy/Music/Opening Theme.md"),
        r#"---
title: Opening Theme
release_date: 2024-01-01
---
"#,
    );
    write_file(
        &vault.join("Daily Notes/2025-04-21.md"),
        r#"---
tags: [daily]
---
## Watched
Revisited [[Star Voyager]] and [[Moon Quest|the quest]] after dinner.

```text
[[Opening Theme]] inside a code fence should not count.
```
"#,
    );
}

fn write_file(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn count_for(items: &Value, name: &str) -> i64 {
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == name || item["label"] == name || item["typeLabel"] == name)
        .and_then(|item| item["count"].as_i64())
        .unwrap_or_default()
}

fn queue_summary<'a>(items: &'a Value, id: &str) -> &'a Value {
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap_or_else(|| panic!("queue not found: {id}"))
}

fn relation_field_count(items: &Value, field: &str) -> usize {
    items
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["field"] == field)
        .count()
}

fn has_entity_title(items: &Value, title: &str) -> bool {
    items
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["title"] == title || item["entity"]["title"] == title)
}

fn entity_by_title<'value>(items: &'value Value, title: &str) -> &'value Value {
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["title"] == title || item["entity"]["title"] == title)
        .unwrap_or_else(|| panic!("entity not found: {title}"))
}

fn unique_relation_target_count(entity_id: &str, relations: &Value) -> usize {
    relations
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|relation| {
            if relation["sourceId"] == entity_id {
                Some(
                    relation["targetId"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("unresolved:{}", relation["targetTitle"])),
                )
            } else {
                relation["sourceId"].as_str().map(str::to_owned)
            }
        })
        .collect::<HashSet<_>>()
        .len()
}

// ---------------------------------------------------------------------------
// Asset download
// ---------------------------------------------------------------------------

/// Minimal valid 1x1 PNG.
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

async fn upload_asset(app: &Router, id: &str, body: Value) -> (StatusCode, Value) {
    request_json(
        app,
        Method::POST,
        &format!("/api/entities/{}/assets/upload", urlencoding::encode(id)),
        Some(body),
    )
    .await
}

async fn start_mock_image_server() -> std::net::SocketAddr {
    // The mock server binds to loopback, which the SSRF guard blocks by default.
    // Enable the documented escape hatch so the download path can reach it.
    std::env::set_var("KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS", "1");
    let app = Router::new()
        .route(
            "/image.png",
            axum::routing::get(|| async {
                ([(header::CONTENT_TYPE, "image/png")], PNG_1X1.to_vec())
            }),
        )
        .route(
            "/notimage",
            axum::routing::get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html")],
                    b"<html>nope</html>".to_vec(),
                )
            }),
        )
        .route(
            "/missing",
            axum::routing::get(|| async { StatusCode::NOT_FOUND }),
        )
        .route(
            "/slow.png",
            axum::routing::get(|| async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                ([(header::CONTENT_TYPE, "image/png")], PNG_1X1.to_vec())
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

fn asset_test_app(
    content_writable: bool,
    write_entities: impl FnOnce(&Path),
) -> (Router, TempDir, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir_all(vault.join("Taxonomy/Anime")).unwrap();
    write_entities(&vault);
    let config = json!({
        "taxonomyRoot": "Taxonomy",
        "assetRoot": "Assets",
        "types": [
            {
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                    { "field": "cover_url", "fieldType": "image", "displayName": "Cover" },
                    { "field": "shots", "fieldType": "imageList", "displayName": "Shots" }
                ]
            }
        ]
    });
    write_vault_config(&vault, &config);
    // Asset plan/ingest is the host-driven (iOS) flow, so the asset tests run with
    // host-path ingest enabled. A dedicated test covers the network posture (off).
    let app = host_inline_router(&vault, content_writable);
    (app, temp, vault)
}

async fn entity_revision(app: &Router, id: &str) -> String {
    let detail = request_json(
        app,
        Method::GET,
        &format!("/api/entities/{}", urlencoding::encode(id)),
        None,
    )
    .await;
    assert_eq!(detail.0, StatusCode::OK, "detail: {}", detail.1);
    detail.1["entity"]["revision"].as_str().unwrap().to_string()
}

async fn download_assets(app: &Router, id: &str, revision: &str) -> (StatusCode, Value) {
    request_json(
        app,
        Method::POST,
        &format!("/api/entities/{}/assets/download", urlencoding::encode(id)),
        Some(json!({ "revision": revision })),
    )
    .await
}

async fn request_raw(app: &Router, path: &str) -> (StatusCode, Option<String>, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(path)
                .body(body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, content_type, bytes)
}

#[tokio::test]
async fn asset_download_writes_local_file_and_serves_it() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/image.png");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            &format!("---\ntitle: Star Voyager\ncover_url: {cover}\n---\nBody\n"),
        );
    });

    let id = "anime:Star Voyager";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["field"], "cover_url");
    assert_eq!(results[0]["status"], "downloaded");
    let local_path = results[0]["path"].as_str().unwrap();
    assert_eq!(
        local_path,
        "Assets/Taxonomy/Anime/Star Voyager/cover_url.png"
    );

    // Frontmatter (and therefore the summary cover) now points at the local file.
    assert_eq!(body["entity"]["image"], local_path);
    assert_eq!(body["entity"]["frontmatter"]["cover_url"], local_path);

    // The file is written under the vault.
    let written = vault.join(local_path);
    assert_eq!(fs::read(&written).unwrap(), PNG_1X1);

    // And it is served back through the asset route with an image content type.
    let serve_path = format!("/api/assets/{}", local_path.replace(' ', "%20"));
    let (serve_status, content_type, bytes) = request_raw(&app, &serve_path).await;
    assert_eq!(serve_status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("image/png"));
    assert_eq!(bytes, PNG_1X1);
}

#[tokio::test]
async fn asset_download_failure_keeps_remote_url() {
    let addr = start_mock_image_server().await;
    let missing = format!("http://{addr}/missing");
    let not_image = format!("http://{addr}/notimage");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            &format!("---\ntitle: Star Voyager\ncover_url: {missing}\nshots:\n  - {not_image}\n---\nBody\n"),
        );
    });

    let id = "anime:Star Voyager";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let results = body["results"].as_array().unwrap();
    assert!(results.iter().all(|item| item["status"] == "failed"));

    // Remote URLs are left untouched so the user can retry.
    assert_eq!(body["entity"]["frontmatter"]["cover_url"], missing);
    assert_eq!(body["entity"]["frontmatter"]["shots"][0], not_image);

    // No asset directory was created for this entity.
    assert!(!vault.join("Assets/Taxonomy/Anime/Star Voyager").exists());
}

#[tokio::test]
async fn asset_download_handles_image_list_partially() {
    let addr = start_mock_image_server().await;
    let ok = format!("http://{addr}/image.png");
    let bad = format!("http://{addr}/missing");
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            &format!("---\ntitle: Star Voyager\nshots:\n  - {ok}\n  - {bad}\n---\nBody\n"),
        );
    });

    let id = "anime:Star Voyager";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let shots = body["entity"]["frontmatter"]["shots"].as_array().unwrap();
    // First element became a local hashed path; the failed one keeps its URL.
    assert!(shots[0]
        .as_str()
        .unwrap()
        .starts_with("Assets/Taxonomy/Anime/Star Voyager/shots/"));
    assert!(shots[0].as_str().unwrap().ends_with(".png"));
    assert_eq!(shots[1], bad);
}

#[tokio::test]
async fn plan_lists_remote_image_fields_only() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\ncover_url: https://img.example/cover.jpg\nshots:\n  - https://img.example/a.png\n  - Assets/Taxonomy/Anime/Star Voyager/shots/local.png\n---\nBody\n",
        );
    });

    let (status, body) = request_json(&app, Method::GET, "/api/asset-downloads/plan", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["scope"], "all");

    let items = body["items"].as_array().unwrap();
    // cover_url (single) + one remote shots element; the local shot is excluded.
    assert_eq!(items.len(), 2, "{body}");

    let cover = items
        .iter()
        .find(|item| item["field"] == "cover_url")
        .unwrap();
    assert_eq!(cover["entityId"], "anime:Star Voyager");
    assert_eq!(cover["sourceUrl"], "https://img.example/cover.jpg");
    assert!(cover["listKey"].is_null());

    let shot = items.iter().find(|item| item["field"] == "shots").unwrap();
    assert_eq!(shot["sourceUrl"], "https://img.example/a.png");
    // List fields carry the element URL as the list key.
    assert_eq!(shot["listKey"], "https://img.example/a.png");

    let (status, _) = request_json(
        &app,
        Method::GET,
        "/api/asset-downloads/plan?entityType=nope",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ingest_places_host_downloaded_file_and_rewrites_single_field() {
    let (app, temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\ncover_url: https://img.example/cover.jpg\n---\nBody\n",
        );
    });

    // The host (iOS) already downloaded the bytes to an app-temp file outside the vault.
    let source = temp.path().join("ingest-cover.png");
    fs::write(&source, PNG_1X1).unwrap();

    let id = "anime:Star Voyager";
    let (status, body) = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}/assets/ingest", urlencoding::encode(id)),
        Some(json!({
            "field": "cover_url",
            "sourceUrl": "https://img.example/cover.jpg",
            "sourcePath": source.to_string_lossy(),
            "contentType": "image/png",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["status"], "downloaded");
    assert_eq!(
        body["entity"]["frontmatter"]["cover_url"],
        "Assets/Taxonomy/Anime/Star Voyager/cover_url.png"
    );
    assert!(vault
        .join("Assets/Taxonomy/Anime/Star Voyager/cover_url.png")
        .exists());
    // The host-temp source file is consumed (deleted) by the core.
    assert!(!source.exists());
}

#[tokio::test]
async fn ingest_rewrites_one_list_element() {
    let (app, temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\nshots:\n  - https://img.example/a.png\n  - https://img.example/b.png\n---\nBody\n",
        );
    });

    let source = temp.path().join("shot-a.png");
    fs::write(&source, PNG_1X1).unwrap();

    let id = "anime:Star Voyager";
    let (status, body) = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}/assets/ingest", urlencoding::encode(id)),
        Some(json!({
            "field": "shots",
            "listKey": "https://img.example/a.png",
            "sourceUrl": "https://img.example/a.png",
            "sourcePath": source.to_string_lossy(),
            "contentType": "image/png",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["status"], "downloaded");

    let shots = body["entity"]["frontmatter"]["shots"].as_array().unwrap();
    // The ingested element became a local hashed path; the other keeps its URL.
    assert!(shots[0]
        .as_str()
        .unwrap()
        .starts_with("Assets/Taxonomy/Anime/Star Voyager/shots/"));
    assert!(shots[0].as_str().unwrap().ends_with(".png"));
    assert_eq!(shots[1], "https://img.example/b.png");
}

#[tokio::test]
async fn ingest_skips_when_source_url_no_longer_matches() {
    let (app, temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\ncover_url: https://img.example/new.jpg\n---\nBody\n",
        );
    });

    let source = temp.path().join("stale.png");
    fs::write(&source, PNG_1X1).unwrap();

    let id = "anime:Star Voyager";
    let (status, body) = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}/assets/ingest", urlencoding::encode(id)),
        Some(json!({
            "field": "cover_url",
            // A download that began against a URL the user has since changed.
            "sourceUrl": "https://img.example/old.jpg",
            "sourcePath": source.to_string_lossy(),
            "contentType": "image/png",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["status"], "skipped");
    // Frontmatter keeps the user's current value; nothing is written.
    assert_eq!(
        body["entity"]["frontmatter"]["cover_url"],
        "https://img.example/new.jpg"
    );
    assert!(!vault.join("Assets/Taxonomy/Anime/Star Voyager").exists());
    // Even on skip, the stale temp file is cleaned up.
    assert!(!source.exists());
}

#[tokio::test]
async fn upload_places_single_image_and_leaves_frontmatter_for_save() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\ncover_url: https://img.example/cover.jpg\n---\nBody\n",
        );
    });

    let id = "anime:Star Voyager";
    let (status, body) = upload_asset(
        &app,
        id,
        json!({
            "field": "cover_url",
            "dataBase64": b64(PNG_1X1),
            "contentType": "image/png",
            "filename": "cover.png",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let path = body["path"].as_str().unwrap();
    assert_eq!(path, "Assets/Taxonomy/Anime/Star Voyager/cover_url.png");
    assert_eq!(body["conflictResolved"], false);
    // The file is placed under the vault.
    assert_eq!(fs::read(vault.join(path)).unwrap(), PNG_1X1);

    // Upload stages into the editor draft only — frontmatter is untouched until the
    // normal save mutation runs.
    let detail = request_json(
        &app,
        Method::GET,
        &format!("/api/entities/{}", urlencoding::encode(id)),
        None,
    )
    .await;
    assert_eq!(
        detail.1["entity"]["frontmatter"]["cover_url"],
        "https://img.example/cover.jpg"
    );
}

#[tokio::test]
async fn upload_places_image_list_element_by_content_hash() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\n---\nBody\n",
        );
    });

    let id = "anime:Star Voyager";
    let (status, body) = upload_asset(
        &app,
        id,
        json!({
            "field": "shots",
            "dataBase64": b64(PNG_1X1),
            "contentType": "image/png",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let path = body["path"].as_str().unwrap();
    assert!(path.starts_with("Assets/Taxonomy/Anime/Star Voyager/shots/"));
    assert!(path.ends_with(".png"));
    assert!(vault.join(path).exists());
}

#[tokio::test]
async fn upload_rejects_non_image_bytes() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\n---\nBody\n",
        );
    });

    let (status, _body) = upload_asset(
        &app,
        "anime:Star Voyager",
        json!({
            "field": "cover_url",
            "dataBase64": b64(b"<html>nope</html>"),
            "contentType": "text/html",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_rejects_non_image_field_and_bad_base64() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\n---\nBody\n",
        );
    });

    // `title` is not an image field.
    let (status, _body) = upload_asset(
        &app,
        "anime:Star Voyager",
        json!({ "field": "title", "dataBase64": b64(PNG_1X1) }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Malformed base64 payload.
    let (status, _body) = upload_asset(
        &app,
        "anime:Star Voyager",
        json!({ "field": "cover_url", "dataBase64": "not valid base64!!!" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_is_disabled_in_read_only_mode() {
    let (app, _temp, vault) = asset_test_app(false, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\n---\nBody\n",
        );
    });

    let (status, body) = upload_asset(
        &app,
        "anime:Star Voyager",
        json!({
            "field": "cover_url",
            "dataBase64": b64(PNG_1X1),
            "contentType": "image/png",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "Content writes are disabled");
    // Nothing was written under the vault.
    assert!(!vault.join("Assets/Taxonomy/Anime/Star Voyager").exists());
}

#[tokio::test]
async fn host_driven_asset_endpoints_are_rejected_on_the_network_runtime() {
    // The web/desktop runtimes leave `host_asset_ingest` off (TestServer uses
    // `inline_router`), so the host-path `plan`/`ingest` endpoints — which read and
    // *delete* a client-supplied absolute path — must be unreachable by an
    // untrusted caller. (iOS opts in via the FFI host; the asset tests cover that.)
    let server = TestServer::new();

    // A file outside the vault that an attacker might target for deletion.
    let temp = TempDir::new().unwrap();
    let victim = temp.path().join("victim.txt");
    fs::write(&victim, b"do not delete").unwrap();

    let (status, body) = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}/assets/ingest",
            urlencoding::encode("anime:Star Voyager")
        ),
        Some(json!({
            "field": "cover_url",
            "sourceUrl": "https://img.example/cover.jpg",
            "sourcePath": victim.to_string_lossy(),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    // The gate fires before any filesystem access — the host file is untouched.
    assert!(
        victim.exists(),
        "gated ingest must not read or delete host paths"
    );

    let (status, body) = server.json("/api/asset-downloads/plan").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

#[tokio::test]
async fn asset_download_avoids_overwriting_another_entitys_file() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/image.png");
    // "Old Show" references a local cover that sits inside "Star Voyager"'s asset
    // directory (as if filenames were swapped outside the app).
    let collide = "Assets/Taxonomy/Anime/Star Voyager/cover_url.png";
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            &format!("---\ntitle: Star Voyager\ncover_url: {cover}\n---\nBody\n"),
        );
        write_file(
            &vault.join("Taxonomy/Anime/Old Show.md"),
            &format!("---\ntitle: Old Show\ncover_url: {collide}\n---\nBody\n"),
        );
    });

    let id = "anime:Star Voyager";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let new_path = body["entity"]["frontmatter"]["cover_url"].as_str().unwrap();
    assert_ne!(new_path, collide, "must not reuse another entity's file");
    assert!(new_path.starts_with("Assets/Taxonomy/Anime/Star Voyager/cover_url-"));
    assert_eq!(body["results"][0]["conflictResolved"], true);

    // The other entity's reference is untouched.
    let other_revision = entity_revision(&app, "anime:Old Show").await;
    assert!(!other_revision.is_empty());
}

#[tokio::test]
async fn asset_serve_route_rejects_path_escape() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\n---\nBody\n",
        );
        // A secret outside the asset root.
        write_file(&vault.join("secret.txt"), "top secret");
    });

    let (status, _content_type, _bytes) =
        request_raw(&app, "/api/assets/..%2f..%2fsecret.txt").await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
        "escape must not succeed, got {status}"
    );
}

#[tokio::test]
async fn asset_download_is_disabled_in_read_only_mode() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/image.png");
    let (app, _temp, _vault) = asset_test_app(false, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            &format!("---\ntitle: Star Voyager\ncover_url: {cover}\n---\nBody\n"),
        );
    });

    let capabilities = request_json(&app, Method::GET, "/api/capabilities", None).await;
    assert_eq!(capabilities.1["assetDownloadEnabled"], false);

    let (status, body) = download_assets(&app, "anime:Star Voyager", "any-revision").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "Content writes are disabled");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn asset_batch_job_downloads_remote_covers() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/image.png");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            &format!("---\ntitle: Star Voyager\ncover_url: {cover}\n---\nBody\n"),
        );
        write_file(
            &vault.join("Taxonomy/Anime/Moon Quest.md"),
            &format!("---\ntitle: Moon Quest\ncover_url: {cover}\n---\nBody\n"),
        );
    });

    let created = request_json(&app, Method::POST, "/api/asset-jobs", Some(json!({}))).await;
    assert_eq!(created.0, StatusCode::OK, "{}", created.1);
    assert_eq!(created.1["total"], 2);
    let job_id = created.1["id"].as_str().unwrap().to_string();

    let mut job = created.1;
    for _ in 0..100 {
        let polled = request_json(
            &app,
            Method::GET,
            &format!("/api/asset-jobs/{}", urlencoding::encode(&job_id)),
            None,
        )
        .await;
        assert_eq!(polled.0, StatusCode::OK);
        job = polled.1;
        if job["status"] == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(job["status"], "completed", "job did not finish: {job}");
    assert_eq!(job["processed"], 2);
    assert_eq!(job["downloaded"], 2);
    assert_eq!(job["failed"], 0);

    assert!(vault
        .join("Assets/Taxonomy/Anime/Star Voyager/cover_url.png")
        .exists());
    assert!(vault
        .join("Assets/Taxonomy/Anime/Moon Quest/cover_url.png")
        .exists());

    let list = request_json(&app, Method::GET, "/api/asset-jobs", None).await;
    assert!(list.1["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == job_id));
}

#[tokio::test]
async fn asset_batch_job_rejects_a_second_job_while_one_is_running() {
    let addr = start_mock_image_server().await;
    // The slow endpoint keeps the first job in flight long enough to fire a second.
    let cover = format!("http://{addr}/slow.png");
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            &format!("---\ntitle: Star Voyager\ncover_url: {cover}\n---\nBody\n"),
        );
    });

    let first = request_json(&app, Method::POST, "/api/asset-jobs", Some(json!({}))).await;
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    let job_id = first.1["id"].as_str().unwrap().to_string();

    // The first job is inserted (queued/running) before its POST returns, so a
    // second request must be rejected while it is still in flight.
    let second = request_json(&app, Method::POST, "/api/asset-jobs", Some(json!({}))).await;
    assert_eq!(second.0, StatusCode::CONFLICT, "{}", second.1);

    // Once the first job finishes, a new job is allowed again.
    for _ in 0..100 {
        let polled = request_json(
            &app,
            Method::GET,
            &format!("/api/asset-jobs/{}", urlencoding::encode(&job_id)),
            None,
        )
        .await;
        if polled.1["status"] == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let third = request_json(&app, Method::POST, "/api/asset-jobs", Some(json!({}))).await;
    assert_eq!(third.0, StatusCode::OK, "{}", third.1);
}

#[tokio::test]
async fn rename_moves_asset_directory_and_rewrites_paths() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\ncover_url: Assets/Taxonomy/Anime/Star Voyager/cover_url.png\n---\nBody\n",
        );
        write_file(
            &vault.join("Assets/Taxonomy/Anime/Star Voyager/cover_url.png"),
            "fake-bytes",
        );
    });

    let id = "anime:Star Voyager";
    let revision = entity_revision(&app, id).await;
    let updated = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}", urlencoding::encode(id)),
        Some(json!({ "revision": revision, "renameTo": "Star Voyager 2" })),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);

    assert!(!vault.join("Assets/Taxonomy/Anime/Star Voyager").exists());
    assert!(vault
        .join("Assets/Taxonomy/Anime/Star Voyager 2/cover_url.png")
        .exists());
    assert_eq!(
        updated.1["entity"]["frontmatter"]["cover_url"],
        "Assets/Taxonomy/Anime/Star Voyager 2/cover_url.png"
    );
}

#[tokio::test]
async fn delete_trashes_asset_directory() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\ncover_url: Assets/Taxonomy/Anime/Star Voyager/cover_url.png\n---\nBody\n",
        );
        write_file(
            &vault.join("Assets/Taxonomy/Anime/Star Voyager/cover_url.png"),
            "fake-bytes",
        );
    });

    let id = "anime:Star Voyager";
    let revision = entity_revision(&app, id).await;
    let deleted = request_json(
        &app,
        Method::DELETE,
        &format!("/api/entities/{}", urlencoding::encode(id)),
        Some(json!({ "revision": revision })),
    )
    .await;
    assert_eq!(deleted.0, StatusCode::OK, "{}", deleted.1);

    // The note and its asset directory are moved into the vault's `.trash`.
    assert!(!vault.join("Taxonomy/Anime/Star Voyager.md").exists());
    assert!(vault.join(".trash/Star Voyager.md").exists());
    assert!(!vault.join("Assets/Taxonomy/Anime/Star Voyager").exists());
    assert!(vault
        .join(".trash/Assets/Taxonomy/Anime/Star Voyager")
        .exists());
}

#[tokio::test]
async fn broken_asset_cleanup_queue_flags_missing_files() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        // References a local cover that does not exist on disk.
        write_file(
            &vault.join("Taxonomy/Anime/Star Voyager.md"),
            "---\ntitle: Star Voyager\ncover_url: Assets/Taxonomy/Anime/Star Voyager/cover_url.png\n---\nBody\n",
        );
    });

    let cleanup = request_json(&app, Method::GET, "/api/cleanup-queues", None).await;
    assert_eq!(cleanup.0, StatusCode::OK);
    let broken = cleanup.1["brokenAssets"].as_array().unwrap();
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0]["title"], "Star Voyager");
    assert!(cleanup.1["queues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|queue| queue["id"] == "broken-asset"));
}

#[tokio::test]
async fn lists_crud_add_reorder_and_delete() {
    let server = TestServer::new();
    let app = &server.app;

    // No lists yet.
    let empty = request_json(app, Method::GET, "/api/lists", None).await;
    assert_eq!(empty.0, StatusCode::OK, "{}", empty.1);
    assert_eq!(empty.1["items"].as_array().unwrap().len(), 0);

    // Create a list (plain Markdown under KizunaShelf/Lists).
    let created = request_json(
        app,
        Method::POST,
        "/api/lists",
        Some(json!({ "name": "Watchlist" })),
    )
    .await;
    assert_eq!(created.0, StatusCode::OK, "{}", created.1);
    assert_eq!(created.1["id"], "Watchlist");
    assert_eq!(created.1["path"], "KizunaShelf/Lists/Watchlist.md");
    assert_eq!(created.1["sections"].as_array().unwrap().len(), 0);

    // Add an entity from "another page" (no revision needed). The server resolves
    // the entity and writes the disambiguated wikilink.
    let added = request_json(
        app,
        Method::POST,
        "/api/lists/Watchlist/items",
        Some(json!({ "entityId": "anime:Star Voyager" })),
    )
    .await;
    assert_eq!(added.0, StatusCode::OK, "{}", added.1);
    // New items land in the ungrouped block (the heading-less first section).
    assert_eq!(added.1["sections"].as_array().unwrap().len(), 1);
    assert!(added.1["sections"][0]["heading"].is_null());
    assert_eq!(added.1["sections"][0]["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        added.1["sections"][0]["items"][0]["text"],
        "[[Star Voyager]]"
    );
    assert_eq!(
        added.1["sections"][0]["items"][0]["entity"]["id"],
        "anime:Star Voyager"
    );

    // Adding the same entity again is idempotent.
    let again = request_json(
        app,
        Method::POST,
        "/api/lists/Watchlist/items",
        Some(json!({ "entityId": "anime:Star Voyager" })),
    )
    .await;
    assert_eq!(again.0, StatusCode::OK, "{}", again.1);
    assert_eq!(again.1["sections"][0]["items"].as_array().unwrap().len(), 1);

    // Add a second entity.
    let added2 = request_json(
        app,
        Method::POST,
        "/api/lists/Watchlist/items",
        Some(json!({ "entityId": "games:Moon Quest" })),
    )
    .await;
    assert_eq!(added2.0, StatusCode::OK, "{}", added2.1);
    assert_eq!(
        added2.1["sections"][0]["items"].as_array().unwrap().len(),
        2
    );

    // Read the list back and capture the revision for the optimistic write.
    let detail = request_json(app, Method::GET, "/api/lists/Watchlist", None).await;
    assert_eq!(detail.0, StatusCode::OK, "{}", detail.1);
    let revision = detail.1["revision"].as_str().unwrap().to_string();

    // Full rewrite: move one item into a numbered "Finished" section, put the other
    // in a "Todo" task section (checked), set description + trailing.
    let update_body = json!({
        "revision": revision,
        "description": "My picks",
        "trailing": "More below.",
        "sections": [
            {
                "heading": "Todo",
                "marker": "todo",
                "items": [{ "text": "[[Moon Quest]]", "checked": true }]
            },
            {
                "heading": "Finished",
                "marker": "ordered",
                "items": [{ "text": "[[Star Voyager]]" }]
            }
        ]
    });
    let updated = request_json(
        app,
        Method::POST,
        "/api/lists/Watchlist",
        Some(update_body.clone()),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);
    assert_eq!(updated.1["description"], "My picks");
    assert_eq!(updated.1["sections"].as_array().unwrap().len(), 2);
    assert_eq!(updated.1["sections"][0]["heading"], "Todo");
    assert_eq!(updated.1["sections"][0]["marker"], "todo");
    assert_eq!(updated.1["sections"][0]["items"][0]["checked"], true);
    assert_eq!(
        updated.1["sections"][0]["items"][0]["entity"]["id"],
        "games:Moon Quest"
    );
    assert_eq!(updated.1["sections"][1]["heading"], "Finished");
    assert_eq!(updated.1["sections"][1]["marker"], "ordered");
    // A non-task item carries no `checked` field.
    assert!(updated.1["sections"][1]["items"][0]
        .get("checked")
        .is_none());
    assert_eq!(
        updated.1["sections"][1]["items"][0]["entity"]["id"],
        "anime:Star Voyager"
    );

    // The stale revision is now rejected.
    let stale = request_json(app, Method::POST, "/api/lists/Watchlist", Some(update_body)).await;
    assert_eq!(stale.0, StatusCode::CONFLICT, "{}", stale.1);

    // The index reflects the summary (item + section count + description).
    let index = request_json(app, Method::GET, "/api/lists", None).await;
    assert_eq!(index.1["items"].as_array().unwrap().len(), 1);
    assert_eq!(index.1["items"][0]["itemCount"], 2);
    assert_eq!(index.1["items"][0]["sectionCount"], 2);
    assert_eq!(index.1["items"][0]["description"], "My picks");

    // Membership: ?entity= reports whether each list contains that entity (drives
    // the entity page's "manage lists"). `%3A`/`%20` encode the `:` and space.
    let member = request_json(
        app,
        Method::GET,
        "/api/lists?entity=anime%3AStar%20Voyager",
        None,
    )
    .await;
    assert_eq!(member.0, StatusCode::OK, "{}", member.1);
    assert_eq!(member.1["items"][0]["contains"], true);
    // Without the param, `contains` is omitted entirely.
    assert!(index.1["items"][0].get("contains").is_none());

    // Remove the entity from the list; it drops out and membership flips.
    let removed = request_json(
        app,
        Method::DELETE,
        "/api/lists/Watchlist/items/anime%3AStar%20Voyager",
        None,
    )
    .await;
    assert_eq!(removed.0, StatusCode::OK, "{}", removed.1);
    // It drops out of its section; the now-empty "Finished" heading is preserved.
    assert_eq!(
        removed.1["sections"][0]["items"][0]["entity"]["id"],
        "games:Moon Quest"
    );
    assert_eq!(removed.1["sections"][1]["heading"], "Finished");
    assert_eq!(
        removed.1["sections"][1]["items"].as_array().unwrap().len(),
        0
    );
    let member_after = request_json(
        app,
        Method::GET,
        "/api/lists?entity=anime%3AStar%20Voyager",
        None,
    )
    .await;
    assert_eq!(member_after.1["items"][0]["contains"], false);

    // Delete moves it to the trash; the list is then gone.
    let deleted = request_json(app, Method::DELETE, "/api/lists/Watchlist", None).await;
    assert_eq!(deleted.0, StatusCode::OK, "{}", deleted.1);
    assert!(deleted.1["backupPath"]
        .as_str()
        .unwrap()
        .starts_with(".trash/"));
    let missing = request_json(app, Method::GET, "/api/lists/Watchlist", None).await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn smart_lists_create_update_evaluate_and_delete() {
    let server = TestServer::new();
    let app = &server.app;

    // Create a smart list scoped to the anime type. The default document gets
    // a table ("List") and a cards ("Grid") view, the grid inheriting the
    // type's first image field as its cover property.
    let created = request_json(
        app,
        Method::POST,
        "/api/smart-lists",
        Some(json!({ "name": "Watching Now", "scope": "anime" })),
    )
    .await;
    assert_eq!(created.0, StatusCode::OK, "{}", created.1);
    assert_eq!(created.1["id"], "Watching Now");
    assert_eq!(created.1["path"], "KizunaShelf/Lists/Watching Now.base");
    assert_eq!(created.1["scope"], "anime");
    assert_eq!(created.1["filters"]["conjunction"], "all");
    assert_eq!(created.1["filters"]["rules"].as_array().unwrap().len(), 0);
    let views = created.1["views"].as_array().unwrap();
    assert_eq!(views.len(), 2);
    assert_eq!(views[0]["layout"], "list");
    assert_eq!(views[1]["layout"], "grid");
    assert_eq!(views[1]["image"], "note.cover_url");

    // The written file is plain Bases YAML with the scope idiom.
    let raw = fs::read_to_string(server.vault.join("KizunaShelf/Lists/Watching Now.base")).unwrap();
    assert!(raw.contains(r#"file.inFolder("Taxonomy/Anime")"#), "{raw}");
    assert!(raw.contains("type: table"), "{raw}");
    assert!(raw.contains("type: cards"), "{raw}");

    // With no criteria beyond the scope, results = every anime entity.
    let results = request_json(
        app,
        Method::GET,
        "/api/smart-lists/Watching%20Now/results",
        None,
    )
    .await;
    assert_eq!(results.0, StatusCode::OK, "{}", results.1);
    assert_eq!(results.1["total"], 1);
    assert_eq!(results.1["items"][0]["id"], "anime:Star Voyager");

    // Update: add criteria (status is Watching AND favorite) plus a sorted,
    // limited table view.
    let update = json!({
        "revision": created.1["revision"],
        "scope": "anime",
        "filters": {
            "conjunction": "all",
            "rules": [
                { "kind": "compare", "field": "status", "op": "eq", "value": "Watching" },
                { "kind": "compare", "field": "favorite", "op": "eq", "boolean": true }
            ]
        },
        "views": [
            {
                "name": "List",
                "layout": "list",
                "sort": [ { "property": "note.complete_date", "direction": "desc" } ],
                "limit": 25
            },
            { "name": "Grid", "layout": "grid" }
        ]
    });
    let updated = request_json(
        app,
        Method::POST,
        "/api/smart-lists/Watching%20Now",
        Some(update.clone()),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);
    assert_eq!(updated.1["scope"], "anime");
    assert_eq!(updated.1["filters"]["rules"].as_array().unwrap().len(), 2);
    assert_eq!(updated.1["views"][0]["limit"], 25);
    // The grid view kept its derived image property across the rewrite.
    assert_eq!(updated.1["views"][1]["image"], "note.cover_url");

    // A stale revision is rejected.
    let stale = request_json(
        app,
        Method::POST,
        "/api/smart-lists/Watching%20Now",
        Some(update),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT, "{}", stale.1);

    // The criteria evaluate: Star Voyager is Watching + favorite.
    let results = request_json(
        app,
        Method::GET,
        "/api/smart-lists/Watching%20Now/results?view=List",
        None,
    )
    .await;
    assert_eq!(results.0, StatusCode::OK, "{}", results.1);
    assert_eq!(results.1["total"], 1);

    // An unknown view 404s.
    let missing_view = request_json(
        app,
        Method::GET,
        "/api/smart-lists/Watching%20Now/results?view=Nope",
        None,
    )
    .await;
    assert_eq!(missing_view.0, StatusCode::NOT_FOUND);

    // The lists index interleaves the smart list, with evaluated membership.
    let index = request_json(
        app,
        Method::GET,
        "/api/lists?entity=anime%3AStar%20Voyager",
        None,
    )
    .await;
    assert_eq!(index.0, StatusCode::OK, "{}", index.1);
    let items = index.1["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"], "smart");
    assert_eq!(items[0]["itemCount"], 1);
    assert_eq!(items[0]["contains"], true);

    // Preview evaluates unsaved criteria without a file.
    let preview = request_json(
        app,
        Method::POST,
        "/api/smart-lists/preview",
        Some(json!({
            "scope": "anime",
            "filters": {
                "conjunction": "all",
                "rules": [
                    { "kind": "compare", "field": "status", "op": "eq", "value": "Completed" }
                ]
            }
        })),
    )
    .await;
    assert_eq!(preview.0, StatusCode::OK, "{}", preview.1);
    assert_eq!(preview.1["total"], 0);

    // Delete moves the file to the trash.
    let deleted = request_json(app, Method::DELETE, "/api/smart-lists/Watching%20Now", None).await;
    assert_eq!(deleted.0, StatusCode::OK, "{}", deleted.1);
    assert!(!server
        .vault
        .join("KizunaShelf/Lists/Watching Now.base")
        .exists());
    assert!(server.vault.join(".trash/Watching Now.base").exists());
}

#[tokio::test]
async fn smart_lists_today_criteria_honor_client_today() {
    // A `today()` date criterion must be judged against the client's local date
    // (the `today` query param), not the host's clock — the same fix as
    // `/api/upcoming`, threaded through the smart-list evaluation context.
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "games", "label": "Games", "path": "Games",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                    { "field": "release_date", "fieldType": "date", "dateRole": "planning" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Games/PRAGMATA.md"),
        "---\ntitle: PRAGMATA\nrelease_date: 2030-06-01\n---\n",
    );
    // "Released" = release_date already in the past relative to today().
    write_file(
        &vault.join("KizunaShelf/Lists/Released.base"),
        "filters:\n  and:\n    - file.inFolder(\"Taxonomy/Games\")\n    - note.release_date < today()\nviews:\n  - type: table\n    name: List\n",
    );
    let app = inline_router(&vault, true, true);

    // Before the release date, the client's `today` leaves it out.
    let before = request_json(
        &app,
        Method::GET,
        "/api/smart-lists/Released/results?today=2030-01-01",
        None,
    )
    .await;
    assert_eq!(before.0, StatusCode::OK, "{}", before.1);
    assert_eq!(before.1["total"], 0, "{}", before.1);

    // After it, the same list now includes it — driven purely by the param.
    let after = request_json(
        &app,
        Method::GET,
        "/api/smart-lists/Released/results?today=2030-08-01",
        None,
    )
    .await;
    assert_eq!(after.0, StatusCode::OK, "{}", after.1);
    assert_eq!(after.1["total"], 1, "{}", after.1);
    assert_eq!(after.1["items"][0]["id"], "games:PRAGMATA");

    // The lists index computes `itemCount` through the same context, so it moves
    // with `today` too.
    let index_before = request_json(&app, Method::GET, "/api/lists?today=2030-01-01", None).await;
    assert_eq!(
        index_before.1["items"][0]["itemCount"], 0,
        "{}",
        index_before.1
    );
    let index_after = request_json(&app, Method::GET, "/api/lists?today=2030-08-01", None).await;
    assert_eq!(
        index_after.1["items"][0]["itemCount"], 1,
        "{}",
        index_after.1
    );

    // Preview of unsaved `today()` criteria honors the request's `today`. The
    // rule builder expresses bare `today()` as a zero-amount relative date, so
    // `release_date > today()` selects the still-unreleased entity.
    let preview = request_json(
        &app,
        Method::POST,
        "/api/smart-lists/preview",
        Some(json!({
            "scope": "games",
            "filters": {
                "conjunction": "all",
                "rules": [{
                    "kind": "compare",
                    "field": "release_date",
                    "op": "gt",
                    "relative": { "amount": 0, "unit": "days", "future": false }
                }]
            },
            "today": "2030-01-01"
        })),
    )
    .await;
    assert_eq!(preview.0, StatusCode::OK, "{}", preview.1);
    assert_eq!(preview.1["total"], 1, "{}", preview.1);
}

#[tokio::test]
async fn smart_lists_preserve_hand_authored_syntax_across_edits() {
    let server = TestServer::new();
    let app = &server.app;

    // A hand-edited file: an unsupported formula filter, a formulas block, and
    // a map view — all beyond the supported profile.
    write_file(
        &server.vault.join("KizunaShelf/Lists/Backlog.base"),
        r#"filters:
  and:
    - file.inFolder("Taxonomy/Anime")
    - status == "Watching"
    - formula.score > 5
formulas:
  score: "rating * 2"
views:
  - type: table
    name: List
    order:
      - file.name
      - status
  - type: map
    name: Places
"#,
    );

    let detail = request_json(app, Method::GET, "/api/smart-lists/Backlog", None).await;
    assert_eq!(detail.0, StatusCode::OK, "{}", detail.1);
    // The unsupported filter surfaces as a raw rule + a warning; the map view
    // is skipped with a warning.
    let rules = detail.1["filters"]["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1]["kind"], "unsupported");
    let warnings = detail.1["warnings"].as_array().unwrap();
    assert!(warnings
        .iter()
        .any(|w| w.as_str().unwrap().contains("formula.score")));
    assert!(warnings
        .iter()
        .any(|w| w.as_str().unwrap().contains("Places")));
    // Unsupported criteria are ignored, not disqualifying: results still match.
    let results = request_json(app, Method::GET, "/api/smart-lists/Backlog/results", None).await;
    assert_eq!(results.0, StatusCode::OK, "{}", results.1);
    assert_eq!(results.1["total"], 1);

    // Save the detail straight back (the editor round-trip): everything we
    // don't model must survive in the file.
    let update = json!({
        "revision": detail.1["revision"],
        "scope": detail.1["scope"],
        "filters": detail.1["filters"],
        "views": detail.1["views"],
    });
    let updated = request_json(app, Method::POST, "/api/smart-lists/Backlog", Some(update)).await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);

    let raw = fs::read_to_string(server.vault.join("KizunaShelf/Lists/Backlog.base")).unwrap();
    assert!(raw.contains("formula.score > 5"), "{raw}");
    assert!(
        raw.contains("score: rating * 2") || raw.contains("score: \"rating * 2\""),
        "{raw}"
    );
    assert!(raw.contains("type: map"), "{raw}");
    // The table view kept its hand-written column order.
    assert!(raw.contains("- status"), "{raw}");
}

#[tokio::test]
async fn smart_list_writes_blocked_in_read_only_mode() {
    let server = TestServer::read_only();
    let created = request_json(
        &server.app,
        Method::POST,
        "/api/smart-lists",
        Some(json!({ "name": "Nope" })),
    )
    .await;
    assert_eq!(created.0, StatusCode::FORBIDDEN, "{}", created.1);
}

#[tokio::test]
async fn lists_writes_blocked_in_read_only_mode() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_fixture_vault(&vault);
    write_vault_config(&vault, &json!({ "taxonomyRoot": "Taxonomy", "types": [] }));
    let app = inline_router(&vault, true, false);

    let created = request_json(
        &app,
        Method::POST,
        "/api/lists",
        Some(json!({ "name": "Watchlist" })),
    )
    .await;
    assert_eq!(created.0, StatusCode::FORBIDDEN, "{}", created.1);
}

#[tokio::test]
async fn tags_surface_on_summary_filter_and_vocabulary() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    // A configured `tags` field (here a relation) must be IGNORED — the built-in
    // tags feature owns the name, so no relations are built from it.
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "anime", "label": "Anime", "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                    { "field": "tags", "fieldType": "relation", "displayName": "Tags", "relationType": "tag" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Anime/Alpha.md"),
        "---\ntitle: Alpha\ntags: [action, rpg]\n---\nBody\n",
    );
    write_file(
        &vault.join("Taxonomy/Anime/Beta.md"),
        "---\ntitle: Beta\ntags: [rpg, drama]\n---\nBody\n",
    );
    write_file(
        &vault.join("Taxonomy/Anime/Gamma.md"),
        "---\ntitle: Gamma\n---\nBody\n",
    );
    let app = inline_router(&vault, true, true);

    // Tags surface on the entity summary (empty array when absent).
    let entities = request_json(&app, Method::GET, "/api/entities?type=anime", None).await;
    assert_eq!(entities.0, StatusCode::OK, "{}", entities.1);
    let items = entities.1["items"].as_array().unwrap();
    let alpha = items.iter().find(|e| e["title"] == "Alpha").unwrap();
    assert_eq!(alpha["tags"], json!(["action", "rpg"]));
    let gamma = items.iter().find(|e| e["title"] == "Gamma").unwrap();
    assert_eq!(gamma["tags"], json!([]));
    // The configured `tags` relation field was ignored — no relation built from it.
    assert_eq!(alpha["relationCount"], 0);

    // The vocabulary is the sorted union of every tag.
    let tags = request_json(&app, Method::GET, "/api/tags", None).await;
    assert_eq!(tags.0, StatusCode::OK, "{}", tags.1);
    assert_eq!(tags.1["tags"], json!(["action", "drama", "rpg"]));

    // Filter by a single tag via the shared `filters` param:
    // [{"field":"tags","values":["action"]}]
    let one = request_json(
        &app,
        Method::GET,
        "/api/entities?type=anime&filters=%5B%7B%22field%22%3A%22tags%22%2C%22values%22%3A%5B%22action%22%5D%7D%5D",
        None,
    )
    .await;
    assert_eq!(one.0, StatusCode::OK, "{}", one.1);
    let one_titles: Vec<_> = one.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap())
        .collect();
    assert_eq!(one_titles, ["Alpha"]);

    // ANY/OR semantics: action OR drama → Alpha + Beta.
    let many = request_json(
        &app,
        Method::GET,
        "/api/entities?type=anime&filters=%5B%7B%22field%22%3A%22tags%22%2C%22values%22%3A%5B%22action%22%2C%22drama%22%5D%7D%5D",
        None,
    )
    .await;
    let mut many_titles: Vec<_> = many.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap())
        .collect();
    many_titles.sort();
    assert_eq!(many_titles, ["Alpha", "Beta"]);
}

#[tokio::test]
async fn tags_field_name_is_configurable() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    // Rename the built-in tags field to `labels` at the vault level.
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "tags": { "field": "labels" },
            "types": [{
                "id": "anime", "label": "Anime", "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Anime/Alpha.md"),
        "---\ntitle: Alpha\nlabels: [action, rpg]\ntags: ignored\n---\nBody\n",
    );
    let app = inline_router(&vault, true, true);

    // The config surfaces the resolved name for clients.
    let config = request_json(&app, Method::GET, "/api/config", None).await;
    assert_eq!(config.0, StatusCode::OK, "{}", config.1);
    assert_eq!(config.1["tagsField"], "labels");

    // Tags are derived from `labels`, not the default `tags` key.
    let entities = request_json(&app, Method::GET, "/api/entities?type=anime", None).await;
    let alpha = entities.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["title"] == "Alpha")
        .unwrap();
    assert_eq!(alpha["tags"], json!(["action", "rpg"]));

    // The vocabulary + filter use the configured field too.
    let tags = request_json(&app, Method::GET, "/api/tags", None).await;
    assert_eq!(tags.1["tags"], json!(["action", "rpg"]));
    let filtered = request_json(
        &app,
        Method::GET,
        "/api/entities?type=anime&filters=%5B%7B%22field%22%3A%22labels%22%2C%22values%22%3A%5B%22rpg%22%5D%7D%5D",
        None,
    )
    .await;
    let titles: Vec<_> = filtered.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Alpha"]);
}

#[tokio::test]
async fn episodes_detail_progress_update_and_revision_guard() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "anime", "label": "Anime", "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "bodySections": [
                    { "heading": "Episodes", "kind": "episodes", "tracking": "checklist" }
                ],
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Anime/Show.md"),
        "---\ntitle: Show\n---\n## Episodes\n### Season 1\n- [x] 1 · Pilot\n- [ ] 12.5 · Recap\n",
    );
    let app = inline_router(&vault, true, true);

    // Resident progress reaches the list view.
    let list = request_json(&app, Method::GET, "/api/entities?type=anime", None).await;
    let item = &list.1["items"][0];
    assert_eq!(item["episodeProgress"], json!({ "watched": 1, "total": 2 }));

    // Detail parses the grouped episodes (including the 12.5 special).
    let detail = request_json(&app, Method::GET, "/api/entities/anime%3AShow", None).await;
    assert_eq!(detail.0, StatusCode::OK, "{}", detail.1);
    let episodes = &detail.1["episodes"];
    assert_eq!(episodes["total"], 2);
    assert_eq!(episodes["watched"], 1);
    assert_eq!(episodes["groups"][0]["label"], "Season 1");
    assert_eq!(episodes["groups"][0]["items"][1]["key"], "12.5");
    assert_eq!(episodes["groups"][0]["items"][1]["title"], "Recap");
    let revision = detail.1["entity"]["revision"].as_str().unwrap().to_string();

    // Check the recap (the second item in Season 1) through `/episodes/watch` —
    // the dedicated episode write path, independent of daily-note logging. The
    // completion date is the client's local date.
    let body = json!({
        "revision": revision, "group": "Season 1", "key": "12.5", "index": 1,
        "watched": true, "date": "2024-08-20",
    });
    let updated = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/episodes/watch",
        Some(body.clone()),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);
    assert_eq!(updated.1["episodes"]["watched"], 2);
    // The `✅` carries the client-supplied date.
    assert_eq!(
        updated.1["episodes"]["groups"][0]["items"][1]["done"],
        "2024-08-20"
    );

    // The stale revision is now rejected.
    let stale = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/episodes/watch",
        Some(body),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT, "{}", stale.1);
}

#[tokio::test]
async fn episodes_import_merges_and_fetch_lists_sources() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "anime", "label": "Anime", "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "bodySections": [
                    { "heading": "Episodes", "kind": "episodes", "tracking": "checklist" }
                ],
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                    { "field": "igdb_url", "fieldType": "externalRef", "displayName": "IGDB", "externalRef": "igdb" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Anime/Show.md"),
        "---\ntitle: Show\nigdb_url: https://www.igdb.com/games/x\n---\n## Episodes\n- [x] 1 · Pilot\n- [ ] 2\n- [x] 99 · My Extra\n",
    );
    let app = inline_router(&vault, true, true);

    // Fetch lists only episode-capable sources — IGDB doesn't support episodes, so none.
    let fetched = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/episodes/fetch",
        Some(json!({})),
    )
    .await;
    assert_eq!(fetched.0, StatusCode::OK, "{}", fetched.1);
    assert_eq!(fetched.1["sources"], json!([]));
    assert_eq!(fetched.1["groups"], json!([]));

    let detail = request_json(&app, Method::GET, "/api/entities/anime%3AShow", None).await;
    let revision = detail.1["entity"]["revision"].as_str().unwrap().to_string();

    // Import (as a provider would supply): merge preserves watched + extras, fills only empty titles.
    let body = json!({
        "revision": revision,
        "groups": [{
            "label": "",
            "items": [
                { "key": "1", "title": "Pilot (provider)", "watched": false },
                { "key": "2", "title": "Journey", "watched": false },
                { "key": "3", "title": "Dawn", "watched": false }
            ]
        }]
    });
    let imported = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/episodes/import",
        Some(body.clone()),
    )
    .await;
    assert_eq!(imported.0, StatusCode::OK, "{}", imported.1);
    let group = &imported.1["episodes"]["groups"][0]["items"];
    assert_eq!(imported.1["episodes"]["total"], 4);
    assert_eq!(imported.1["episodes"]["watched"], 2); // ep1 + ep99 stay watched
    assert_eq!(
        group[0],
        json!({ "key": "1", "title": "Pilot", "watched": true })
    ); // title not overwritten
    assert_eq!(group[1]["title"], "Journey"); // empty title filled
    assert!(group
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["key"] == "99" && e["watched"] == true)); // extra kept
    assert!(group.as_array().unwrap().iter().any(|e| e["key"] == "3")); // new added

    // Stale revision rejected.
    let stale = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/episodes/import",
        Some(body),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT, "{}", stale.1);
}

/// Live, network-hitting smoke test for provider episode/track sync. Ignored by
/// default; run with real credentials exported from `.env`:
///
/// ```sh
/// set -a; . ./.env; set +a
/// cargo test -p kizunashelf --test api -- --ignored --nocapture provider_episode_sync_live
/// ```
///
/// TMDB and Discogs read their keys from `KIZUNASHELF_*` env vars (via the native
/// secret store); MusicBrainz is keyless.
#[tokio::test]
#[ignore = "hits live TMDB/Discogs/MusicBrainz APIs; needs credentials in env"]
async fn provider_episode_sync_live() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "media", "label": "Media", "path": "Media",
                "externalPriority": ["tmdb", "discogs", "musicbrainz", "myanimelist", "applepodcast", "comicvine"],
                "bodySections": [
                    { "heading": "Episodes", "kind": "episodes", "tracking": "checklist" }
                ],
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title" },
                    { "field": "tmdb_url", "fieldType": "externalRef", "displayName": "TMDB", "externalRef": "tmdb" },
                    { "field": "discogs_url", "fieldType": "externalRef", "displayName": "Discogs", "externalRef": "discogs" },
                    { "field": "musicbrainz_url", "fieldType": "externalRef", "displayName": "MusicBrainz", "externalRef": "musicbrainz" },
                    { "field": "mal_url", "fieldType": "externalRef", "displayName": "MyAnimeList", "externalRef": "myanimelist" },
                    { "field": "podcast_url", "fieldType": "externalRef", "displayName": "Apple Podcasts", "externalRef": "applepodcast" },
                    { "field": "comicvine_url", "fieldType": "externalRef", "displayName": "Comic Vine", "externalRef": "comicvine" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Media/Sample.md"),
        "---\ntitle: Sample\n\
         tmdb_url: https://www.themoviedb.org/tv/1399\n\
         discogs_url: https://www.discogs.com/release/249504\n\
         musicbrainz_url: https://musicbrainz.org/release/4b3d18cc-8937-36f4-8de0-481088be58e6\n\
         mal_url: https://myanimelist.net/anime/1\n\
         podcast_url: https://podcasts.apple.com/us/podcast/the-daily/id1200361736\n\
         comicvine_url: https://comicvine.gamespot.com/volume/4050-18166/\n\
         ---\n## Episodes\n",
    );
    let app = inline_router(&vault, true, true);

    // All three providers are linked and episode-capable, in priority order.
    let listed = request_json(
        &app,
        Method::POST,
        "/api/entities/media%3ASample/episodes/fetch",
        Some(json!({})),
    )
    .await;
    assert_eq!(listed.0, StatusCode::OK, "{}", listed.1);
    let providers: Vec<&str> = listed.1["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["provider"].as_str().unwrap())
        .collect();
    assert_eq!(
        providers,
        vec![
            "tmdb",
            "discogs",
            "musicbrainz",
            "myanimelist",
            "applepodcast",
            "comicvine"
        ]
    );

    let fetch = |provider: &'static str| {
        let app = app.clone();
        async move {
            let response = request_json(
                &app,
                Method::POST,
                "/api/entities/media%3ASample/episodes/fetch",
                Some(json!({ "provider": provider })),
            )
            .await;
            assert_eq!(response.0, StatusCode::OK, "{provider}: {}", response.1);
            response.1
        }
    };

    // TMDB — TV episodes grouped by season.
    let tmdb = fetch("tmdb").await;
    println!("TMDB groups: {:?}", group_summary(&tmdb["groups"]));
    let season1 = find_group(&tmdb["groups"], "Season 1");
    assert_eq!(season1[0]["key"], "1");
    assert_eq!(season1[0]["title"], "Winter Is Coming");
    assert!(
        tmdb["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| group["label"] == "Specials"),
        "expected a Specials group"
    );

    // Discogs — vinyl tracklist (positions A/B), one flat group.
    let discogs = fetch("discogs").await;
    println!("Discogs groups: {:?}", group_summary(&discogs["groups"]));
    let tracks = discogs["groups"][0]["items"].as_array().unwrap();
    assert_eq!(tracks[0]["key"], "A");
    assert!(tracks[0]["title"]
        .as_str()
        .unwrap()
        .contains("Never Gonna Give You Up"));

    // MusicBrainz — single-disc release, one flat group keyed by track number.
    let musicbrainz = fetch("musicbrainz").await;
    println!(
        "MusicBrainz groups: {:?}",
        group_summary(&musicbrainz["groups"])
    );
    let tracks = musicbrainz["groups"][0]["items"].as_array().unwrap();
    assert_eq!(musicbrainz["groups"][0]["label"], "");
    assert_eq!(tracks[0]["key"], "1");
    assert_eq!(tracks[0]["title"], "Airbag");

    // MyAnimeList — anime episodes (via keyless Jikan), one flat group keyed by number.
    let mal = fetch("myanimelist").await;
    println!("MyAnimeList groups: {:?}", group_summary(&mal["groups"]));
    let episodes = mal["groups"][0]["items"].as_array().unwrap();
    assert_eq!(mal["groups"][0]["label"], "");
    assert_eq!(episodes[0]["key"], "1");
    assert_eq!(episodes[0]["title"], "Asteroid Blues");

    // Apple Podcasts — one flat group, episodes numbered 1..N oldest-first. (Titles
    // change as the feed updates, so assert structure, not specific names.)
    let podcast = fetch("applepodcast").await;
    println!(
        "Apple Podcasts groups: {:?}",
        group_summary(&podcast["groups"])
    );
    let episodes = podcast["groups"][0]["items"].as_array().unwrap();
    assert_eq!(podcast["groups"][0]["label"], "");
    assert!(!episodes.is_empty(), "expected podcast episodes");
    for (index, episode) in episodes.iter().enumerate() {
        assert_eq!(episode["key"], (index + 1).to_string());
        assert!(!episode["title"].as_str().unwrap_or_default().is_empty());
    }

    // Comic Vine — a volume's issues as one flat list, keyed by issue number.
    let comicvine = fetch("comicvine").await;
    println!(
        "Comic Vine groups: {:?}",
        group_summary(&comicvine["groups"])
    );
    let issues = comicvine["groups"][0]["items"].as_array().unwrap();
    assert_eq!(comicvine["groups"][0]["label"], "");
    assert_eq!(issues[0]["key"], "1");
    assert_eq!(issues[0]["title"], "Days Gone Bye, Pt. 1");
}

/// Finds a fetched group by label and returns its items (panics if absent).
fn find_group<'a>(groups: &'a Value, label: &str) -> &'a Vec<Value> {
    groups
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["label"] == label)
        .unwrap_or_else(|| panic!("missing group {label:?}"))["items"]
        .as_array()
        .unwrap()
}

/// `[(label, item_count)]` for a fetched groups array — compact test output.
fn group_summary(groups: &Value) -> Vec<(String, usize)> {
    groups
        .as_array()
        .unwrap()
        .iter()
        .map(|group| {
            (
                group["label"].as_str().unwrap_or_default().to_string(),
                group["items"].as_array().map(Vec::len).unwrap_or(0),
            )
        })
        .collect()
}

// ---- Batch import ----------------------------------------------------------

/// A vault whose `anime` type maps the `myanimelist` provider (matching what the
/// Yamtrack CSV importer resolves) and declares no external-field metadata
/// mappings — so a Yamtrack partial candidate (title + cover) maps with no
/// network detail fetch, keeping the whole plan→commit flow offline.
fn build_import_server(content_writable: bool) -> (Router, PathBuf, TempDir) {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir_all(vault.join("Taxonomy/Anime")).unwrap();
    let config = json!({
        "taxonomyRoot": "Taxonomy",
        "dailyNotes": { "paths": ["Daily Notes"], "dateFormat": "YYYY-MM-DD" },
        "types": [
            {
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "filename": { "titleLanguage": "en" },
                "fields": [
                    // Preset-generated schemas map every provider's `title`
                    // metadata key onto the title field; import candidates carry
                    // no language-tagged titles, so this mapping is what fills it.
                    { "field": "title", "fieldType": "title", "titleLanguage": "en",
                      "externalFields": [{ "source": "myanimelist", "field": "title" }] },
                    { "field": "cover", "fieldType": "image" },
                    { "field": "status", "fieldType": "enum",
                      "enumOptions": ["Planning", "Watching", "Completed", "Paused", "Dropped"],
                      "enumRole": "status",
                      "statusValues": {
                          "planning": ["Planning"], "ongoing": ["Watching"],
                          "paused": ["Paused"], "completed": ["Completed"], "dropped": ["Dropped"]
                      } },
                    { "field": "rating", "fieldType": "rating" },
                    { "field": "started", "fieldType": "date", "dateRole": "started" },
                    { "field": "finished", "fieldType": "date", "dateRole": "completed" },
                    { "field": "mal_url", "fieldType": "externalRef",
                      "externalRef": "myanimelist", "externalTypes": ["anime"] }
                ]
            }
        ]
    });
    write_vault_config(&vault, &config);
    let app = inline_router(&vault, true, content_writable);
    (app, vault, temp)
}

const YAMTRACK_CSV: &str = "media_id,source,media_type,title,image,season_number,episode_number,score,progress,status,start_date,end_date,notes,progressed_at\n\
1,mal,anime,Cowboy Bebop,https://img.example/cb.jpg,,,9,26,Completed,2020-01-01,2020-02-01,Loved it,2020-02-01\n\
99,igdb,game,Some Game,,,,7,0,Planning,,,,\n";

/// Polls the import job until it reaches `target`, letting the spawned worker run.
async fn await_import_status(app: &Router, id: &str, target: &str) -> Value {
    for _ in 0..200 {
        let (status, job) =
            request_json(app, Method::GET, &format!("/api/import-jobs/{id}"), None).await;
        assert_eq!(status, StatusCode::OK, "{job}");
        if job["status"] == target {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("import job {id} never reached status {target}");
}

#[tokio::test]
async fn import_plan_then_commit_creates_entities_with_user_data() {
    let (app, vault, _temp) = build_import_server(true);

    let (status, job) = request_json(
        &app,
        Method::POST,
        "/api/import-jobs",
        Some(json!({ "source": "yamtrack", "input": { "csvText": YAMTRACK_CSV } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{job}");
    let id = job["id"].as_str().unwrap().to_string();

    let planned = await_import_status(&app, &id, "planned").await;
    assert_eq!(planned["total"], 2);
    // The mal row resolves to the anime type (auto-selected single bucket); the
    // igdb row has no supported id and goes to review.
    assert_eq!(planned["needsReview"], 1);
    let plan = &planned["plan"];
    let bucket = plan["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|bucket| bucket["provider"] == "myanimelist")
        .expect("myanimelist bucket");
    assert_eq!(bucket["selectedType"], "anime");
    let mal_item = plan["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["provider"] == "myanimelist")
        .expect("mal item");
    assert_eq!(mal_item["state"], "willCreate");
    let review_item = plan["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["bucket"] == "game")
        .expect("game item");
    assert_eq!(review_item["state"], "needsReview");
    assert_eq!(review_item["reviewReason"], "noSupportedId");

    let (status, committing) = request_json(
        &app,
        Method::POST,
        &format!("/api/import-jobs/{id}/commit"),
        Some(json!({
            "options": { "importUserData": true, "importEpisodes": false, "markProgress": false }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{committing}");

    let done = await_import_status(&app, &id, "completed").await;
    assert_eq!(done["created"], 1, "{done}");
    assert_eq!(done["failed"], 0, "{done}");

    // The entity file carries the mapped ref + cover + title and the user data
    // applied through the schema roles (status via statusValues, rating, the
    // completed date, and notes as a `## Notes` section).
    let markdown = fs::read_to_string(vault.join("Taxonomy/Anime/Cowboy Bebop.md")).unwrap();
    assert!(markdown.contains("title: Cowboy Bebop"), "{markdown}");
    assert!(
        markdown.contains("mal_url: https://myanimelist.net/anime/1"),
        "{markdown}"
    );
    assert!(
        markdown.contains("cover: https://img.example/cb.jpg"),
        "{markdown}"
    );
    assert!(markdown.contains("status: Completed"), "{markdown}");
    assert!(markdown.contains("rating: 9"), "{markdown}");
    assert!(markdown.contains("started: 2020-01-01"), "{markdown}");
    assert!(markdown.contains("finished: 2020-02-01"), "{markdown}");
    assert!(markdown.contains("## Notes"), "{markdown}");
    assert!(markdown.contains("Loved it"), "{markdown}");
}

#[tokio::test]
async fn import_dedupes_already_created_entities_on_rerun() {
    let (app, _vault, _temp) = build_import_server(true);

    // First import creates the entity.
    let commit = json!({
        "options": { "importUserData": true, "importEpisodes": false, "markProgress": false }
    });
    for expected_created in [1, 0] {
        let (_, job) = request_json(
            &app,
            Method::POST,
            "/api/import-jobs",
            Some(json!({ "source": "yamtrack", "input": { "csvText": YAMTRACK_CSV } })),
        )
        .await;
        let id = job["id"].as_str().unwrap().to_string();
        await_import_status(&app, &id, "planned").await;
        request_json(
            &app,
            Method::POST,
            &format!("/api/import-jobs/{id}/commit"),
            Some(commit.clone()),
        )
        .await;
        let done = await_import_status(&app, &id, "completed").await;
        assert_eq!(done["created"], expected_created, "run created: {done}");
        if expected_created == 0 {
            // The second run finds it in the library and skips (idempotent rerun).
            assert!(done["skipped"].as_u64().unwrap() >= 1, "{done}");
        }
    }
}

#[tokio::test]
async fn import_is_forbidden_in_read_only_mode() {
    let (app, _vault, _temp) = build_import_server(false);
    let (status, body) = request_json(
        &app,
        Method::POST,
        "/api/import-jobs",
        Some(json!({ "source": "yamtrack", "input": { "csvText": YAMTRACK_CSV } })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "Content writes are disabled");
}
