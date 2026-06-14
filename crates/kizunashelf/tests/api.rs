use axum::body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use kizunashelf::api::{router, ApiOptions};
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;
use tower::ServiceExt;

struct TestServer {
    app: Router,
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
    assert_eq!(external_providers["providers"].as_array().unwrap().len(), 3);
    assert_eq!(external_providers["providers"][0]["id"], "bangumi");
    assert_eq!(
        external_providers["providers"][0]["fields"][0]["field"],
        "name"
    );
    assert_eq!(
        external_providers["providers"][0]["defaultFieldMappings"][0],
        json!({ "roles": ["title"], "field": "name_cn" })
    );
    assert_eq!(
        external_providers["providers"][1]["defaultExternalTypes"],
        json!(["game"])
    );

    let config = server.ok_json("/api/config").await;
    assert_eq!(config["taxonomyRoot"], "Taxonomy");
    assert_eq!(config["types"].as_array().unwrap().len(), 4);
    assert_eq!(config["types"][0]["icon"], "📺");
    assert_eq!(
        config["types"][0]["filename"],
        json!({ "titleLanguage": "zh", "defaultTitle": true })
    );
    assert_eq!(
        config["types"][0]["fields"][1],
        json!({
            "field": "title",
            "fieldType": "title",
            "displayName": "Title",
            "titleLanguage": "zh",
            "defaultTitle": true
        })
    );
    assert_eq!(
        config["types"][0]["fields"][5],
        json!({
            "field": "status",
            "fieldType": "enum",
            "displayName": "Status",
            "enumOptions": ["Backlog", "Watching", "Completed", "Paused", "Dropped"]
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
    assert_eq!(home["sections"][0]["items"][0]["title"], "Star Voyager");
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
    let cover_coverage = coverage_metric(&analytics["coverage"], "Cover");
    assert_eq!(cover_coverage["total"], 2);
    assert_eq!(cover_coverage["missing"], 1);
    let refs_coverage = coverage_metric(&analytics["coverage"], "External refs");
    assert_eq!(refs_coverage["total"], 3);
    assert_eq!(refs_coverage["missing"], 1);
    let relation_coverage = coverage_metric(&analytics["coverage"], "Relations");
    assert_eq!(relation_coverage["total"], 3);
    assert_eq!(relation_coverage["missing"], 0);
    assert!(analytics["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .all(|metric| metric["name"] != "Summary"));

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
    assert!(has_entity_title(&entities["items"], "Star Voyager"));
    assert!(has_entity_title(&entities["items"], "Moon Quest"));
    assert_eq!(entities["items"][0]["titles"]["zh"], "Star Voyager");
    assert_eq!(entities["items"][0]["titles"]["en"], "A Voyage of Stars");
    assert_eq!(entities["items"][0]["titles"]["title_original"], "星之航路");
    let star_voyager_summary = entity_by_title(&entities["items"], "Star Voyager");
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

    let filtered = server
        .ok_json("/api/entities?type=games&refs=with&cover=without")
        .await;
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["items"][0]["id"], "games:Moon Quest");

    let status_filters =
        urlencoding::encode(r#"[{"field":"status","values":["Watching","Playing"]}]"#);
    let status_filtered = server
        .ok_json(&format!("/api/entities?filters={status_filters}"))
        .await;
    assert_eq!(status_filtered["total"], 2);
    assert!(has_entity_title(&status_filtered["items"], "Star Voyager"));
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
    assert!(has_entity_title(&by_relation["items"], "Star Voyager"));
    assert!(has_entity_title(&by_relation["items"], "Moon Quest"));

    let detail = server
        .ok_json(&format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Star Voyager")
        ))
        .await;
    assert_eq!(detail["entity"]["title"], "Star Voyager");
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
        Some(json!({
            "revision": delete_revision,
            "mode": "trash"
        })),
    )
    .await;
    assert_eq!(deleted.0, StatusCode::OK, "{}", deleted.1);
    assert_eq!(deleted.1["deletedId"], "games:Solar Tactics");
    assert!(deleted.1["backupPath"]
        .as_str()
        .unwrap()
        .contains(".kizunashelf/trash"));

    let entities = server.ok_json("/api/entities").await;
    assert_eq!(entities["total"], 4);
}

#[tokio::test]
async fn relation_endpoints_group_temp_vault_links() {
    let server = TestServer::new();

    let relations = server.ok_json("/api/relations").await;
    assert_eq!(relations["total"], 12);

    let franchise_relations = server.ok_json("/api/relations?field=franchise").await;
    assert_eq!(franchise_relations["total"], 4);

    let star_voyager_relations = server
        .ok_json(&format!(
            "/api/relations?sourceId={}",
            urlencoding::encode("anime:Star Voyager")
        ))
        .await;
    assert_eq!(star_voyager_relations["total"], 3);

    let daily_note_relations = server.ok_json("/api/relations?field=daily-note").await;
    assert_eq!(daily_note_relations["total"], 2);

    let groups = server.ok_json("/api/relation-groups").await;
    let anime_targets = groups["targetTypes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["type"] == "anime")
        .unwrap();
    assert_eq!(anime_targets["typeLabel"], "Anime");
    assert_eq!(anime_targets["edgeCount"], 2);
    assert_eq!(anime_targets["uniqueTargets"], 1);
    assert_eq!(count_for(&anime_targets["fields"], "body"), 1);
    assert_eq!(count_for(&anime_targets["fields"], "daily-note"), 1);
    assert_eq!(
        anime_targets["topTargets"][0]["targetTitle"],
        "Star Voyager"
    );
    assert_eq!(anime_targets["topTargets"][0]["count"], 2);
    assert!(anime_targets["topTargets"][0].get("fields").is_none());

    let (status, value) = server
        .json("/api/relation-groups/franchise?pageSize=1&page=1")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(value["error"], "API route not found");
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

    let all_type = server.json("/api/external/search?type=all&q=Star").await;
    assert_eq!(all_type.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        all_type.1["error"],
        "External search requires a concrete entity type"
    );

    let missing_type = server.json("/api/external/search?q=Star").await;
    assert_eq!(missing_type.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        missing_type.1["error"],
        "External search requires a concrete entity type"
    );

    let unknown_type = server
        .json("/api/external/search?type=animation&q=Star")
        .await;
    assert_eq!(unknown_type.0, StatusCode::BAD_REQUEST);
    assert_eq!(unknown_type.1["error"], "Unknown entity type");
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
    assert!(has_entity_title(&april_21["entries"], "Star Voyager"));
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

    let planning = server.ok_json("/api/calendar/planning?year=2025").await;
    assert_eq!(planning["filters"]["year"], 2025);
    assert!(planning["totals"]["entities"].as_u64().unwrap() > 0);
    assert!(planning["totals"]["datedEntries"].as_u64().unwrap() > 0);
    assert!(planning["typeOptions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == "anime"));
    assert!(planning["yearMonths"][3]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["entity"]["id"] == "anime:Star Voyager"
            && item["field"] == "complete_date"
            && item["fieldLabel"] == "Completed date"
            && item["role"] == "completed"));

    let game_planning = server
        .ok_json("/api/calendar/planning?year=2025&type=games")
        .await;
    assert_eq!(game_planning["filters"]["type"], "games");
    assert!(game_planning["board"]["unscheduled"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["type"] == "games"));

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
async fn settings_endpoints_create_and_read_config_files() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("missing/kizunashelf.yaml");
    let app = router(ApiOptions {
        config_path: config_path.clone(),
        cache_ttl: Duration::from_millis(0),
        web_dist_path: None,
        settings_writable: true,
        content_writable: true,
    });

    let missing = request_json(&app, Method::GET, "/api/settings/config", None).await;
    assert_eq!(missing.0, StatusCode::OK);
    assert_eq!(missing.1["appExists"], false);
    assert_eq!(missing.1["vaultExists"], false);
    assert_eq!(
        missing.1["appConfigPath"].as_str().unwrap(),
        config_path.to_string_lossy()
    );

    let vault = temp.path().join("vault");
    write_fixture_vault(&vault);
    let config = json!({
        "vaultRoot": vault,
        "taxonomyRoot": "Taxonomy",
        "readConcurrency": 4,
        "dailyNotes": {
            "paths": ["Daily Notes"],
            "datePattern": "^(\\d{4}-\\d{2}-\\d{2})\\.md$",
            "snippetMaxLength": 120
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
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh", "defaultTitle": true },
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
        Some(split_settings_body(&config)),
    )
    .await;
    assert_eq!(saved.0, StatusCode::OK);
    assert_eq!(saved.1["appExists"], true);
    assert_eq!(saved.1["vaultExists"], true);
    assert_eq!(saved.1["vault"]["types"].as_array().unwrap().len(), 1);
    assert!(config_path.is_file());
    assert!(vault.join(".kizunashelf/config.yaml").is_file());

    let read_back = request_json(&app, Method::GET, "/api/settings/config", None).await;
    assert_eq!(read_back.0, StatusCode::OK);
    assert_eq!(read_back.1["appExists"], true);
    assert_eq!(read_back.1["vaultExists"], true);
    assert_eq!(read_back.1["vault"]["home"]["title"], "Settings Fixture");

    let health = request_json(&app, Method::GET, "/api/health", None).await;
    assert_eq!(health.0, StatusCode::OK);
    assert_eq!(health.1["entityCount"], 1);
}

#[tokio::test]
async fn app_only_save_persists_vault_root_and_detects_existing_vault_config() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("kizunashelf.yaml");
    let app = router(ApiOptions {
        config_path: config_path.clone(),
        cache_ttl: Duration::from_millis(0),
        web_dist_path: None,
        settings_writable: true,
        content_writable: true,
    });

    // App-only save (vault omitted) against an empty vault: creates the vault
    // directory, writes only the app config, and reports no vault config yet.
    let empty_vault = temp.path().join("empty-vault");
    let app_only = request_json(
        &app,
        Method::PUT,
        "/api/settings/config",
        Some(json!({ "app": { "vaultRoot": empty_vault } })),
    )
    .await;
    assert_eq!(app_only.0, StatusCode::OK);
    assert_eq!(app_only.1["appExists"], true);
    assert_eq!(app_only.1["vaultExists"], false);
    assert!(config_path.is_file());
    assert!(empty_vault.is_dir());
    assert!(!empty_vault.join(".kizunashelf/config.yaml").exists());

    // App-only save against a vault that already carries a config: reports it and
    // leaves the file byte-for-byte intact (never rewritten).
    let synced_vault = temp.path().join("synced-vault");
    let vault_config_path = synced_vault.join(".kizunashelf/config.yaml");
    fs::create_dir_all(vault_config_path.parent().unwrap()).unwrap();
    let original = "taxonomyRoot: Library\nassetRoot: Media\ntypes: []\n# keep me\n";
    fs::write(&vault_config_path, original).unwrap();

    let detect = request_json(
        &app,
        Method::PUT,
        "/api/settings/config",
        Some(json!({ "app": { "vaultRoot": synced_vault } })),
    )
    .await;
    assert_eq!(detect.0, StatusCode::OK);
    assert_eq!(detect.1["vaultExists"], true);
    assert_eq!(detect.1["vault"]["taxonomyRoot"], "Library");
    assert_eq!(fs::read_to_string(&vault_config_path).unwrap(), original);
}

#[tokio::test]
async fn settings_config_rejects_paths_that_escape_the_vault_root() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("kizunashelf.yaml");
    let app = router(ApiOptions {
        config_path,
        cache_ttl: Duration::from_millis(0),
        web_dist_path: None,
        settings_writable: true,
        content_writable: true,
    });
    let vault = temp.path().join("vault");

    let escaped_taxonomy = request_json(
        &app,
        Method::PUT,
        "/api/settings/config",
        Some(split_settings_body(&json!({
            "vaultRoot": vault,
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
        Some(split_settings_body(&json!({
            "vaultRoot": vault,
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
async fn path_suggestions_omit_hidden_directories() {
    let temp = TempDir::new().unwrap();
    let base = temp.path().join("picker");
    std::fs::create_dir_all(base.join("Vault")).unwrap();
    std::fs::create_dir_all(base.join(".obsidian")).unwrap();
    let app = router(ApiOptions {
        config_path: temp.path().join("kizunashelf.yaml"),
        cache_ttl: Duration::from_millis(0),
        web_dist_path: None,
        settings_writable: true,
        content_writable: true,
    });

    // Listing a directory hides dotfile folders such as `.obsidian`.
    let visible = request_json(
        &app,
        Method::GET,
        &format!("/api/settings/path-suggestions?path={}", base.display()),
        None,
    )
    .await;
    assert_eq!(visible.0, StatusCode::OK);
    let names: Vec<String> = visible.1["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect();
    assert!(names.iter().any(|name| name.ends_with("Vault")));
    assert!(names.iter().all(|name| !name.contains(".obsidian")));

    // Typing an explicit leading dot still reveals the hidden folder.
    let typed = request_json(
        &app,
        Method::GET,
        &format!("/api/settings/path-suggestions?path={}/.o", base.display()),
        None,
    )
    .await;
    assert_eq!(typed.0, StatusCode::OK);
    let typed_names: Vec<String> = typed.1["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect();
    assert!(typed_names.iter().any(|name| name.ends_with(".obsidian")));
}

#[tokio::test]
async fn settings_mutation_endpoints_can_be_disabled() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_fixture_vault(&vault);
    let config_path = temp.path().join("kizunashelf.yaml");
    let app = router(ApiOptions {
        config_path,
        cache_ttl: Duration::from_millis(0),
        web_dist_path: None,
        settings_writable: false,
        content_writable: false,
    });
    let config = json!({
        "vaultRoot": vault,
        "taxonomyRoot": "Taxonomy",
        "types": []
    });

    let saved = request_json(
        &app,
        Method::PUT,
        "/api/settings/config",
        Some(split_settings_body(&config)),
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
    let config_path = temp.path().join("kizunashelf.yaml");
    let config = json!({
        "vaultRoot": vault,
        "taxonomyRoot": "Taxonomy",
        "types": [
            {
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh", "defaultTitle": true },
                    { "field": "status", "fieldType": "enum", "enumOptions": ["Backlog", "Watching", "Completed"] }
                ]
            }
        ]
    });
    write_split_config(&config_path, &config);
    let app = router(ApiOptions {
        config_path,
        cache_ttl: Duration::from_millis(0),
        web_dist_path: None,
        settings_writable: true,
        content_writable: false,
    });

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
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        write_fixture_vault(&vault);
        let config_path = temp.path().join("kizunashelf.yaml");
        let config = json!({
            "vaultRoot": vault,
            "taxonomyRoot": "Taxonomy",
            "dailyNotes": {
                "paths": ["Daily Notes"],
                "datePattern": "^(?:Daily Notes/)?(?<date>\\d{4}-\\d{2}-\\d{2})\\.md$",
                "snippetMaxLength": 120
            },
            "home": {
                "title": "Fixture Home",
                "sections": [
                    {
                        "id": "recent-anime",
                        "title": "Recent Anime",
                        "type": "anime",
                        "filters": [{ "field": "status", "values": ["Watching"] }],
                        "limit": 4,
                        "sort": "title",
                        "direction": "asc"
                    },
                    {
                        "id": "games",
                        "title": "Games",
                        "type": "games",
                        "filters": [{ "field": "status", "values": ["Playing"] }],
                        "limit": 4,
                        "sort": "title",
                        "direction": "asc"
                    },
                    {
                        "id": "completed-anime",
                        "title": "Completed Anime",
                        "type": "anime",
                        "filters": [{ "field": "status", "values": ["Completed"] }],
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
                    "filename": { "titleLanguage": "zh", "defaultTitle": true },
                    "fields": [
                        { "field": "id", "fieldType": "id", "displayName": "ID" },
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh", "defaultTitle": true },
                        { "field": "title_en", "fieldType": "title", "displayName": "Title (English)", "titleLanguage": "en" },
                        { "field": "title_original", "fieldType": "title", "displayName": "Title (Original)", "titleRole": "original" },
                        { "field": "cover_url", "fieldType": "image", "displayName": "Cover" },
                        { "field": "status", "fieldType": "enum", "displayName": "Status", "enumOptions": ["Backlog", "Watching", "Completed", "Paused", "Dropped"] },
                        { "field": "season", "fieldType": "season", "displayName": "Season", "dateRole": "planning", "seasonLanguage": "zh" },
                        { "field": "complete_date", "fieldType": "date", "displayName": "Completed date", "dateRole": "completed" },
                        { "field": "favorite", "fieldType": "bool", "displayName": "Favorite" },
                        { "field": "bgm_url", "fieldType": "externalRef", "displayName": "BGM", "externalRef": "bangumi" },
                        { "field": "franchise", "fieldType": "relation", "displayName": "Franchise", "relationType": "franchise" },
                        { "field": "studio", "fieldType": "relation", "displayName": "Studio", "relationType": "studio" }
                    ]
                },
                {
                    "id": "games",
                    "label": "Games",
                    "path": "Games",
                    "filename": { "titleLanguage": "zh" },
                    "fields": [
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh", "defaultTitle": true },
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
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh", "defaultTitle": true },
                        { "field": "related", "fieldType": "relation", "displayName": "Related", "relationType": "related" }
                    ]
                },
                {
                    "id": "music",
                    "label": "Music",
                    "path": "Music",
                    "filename": { "defaultTitle": true },
                    "fields": [
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh", "defaultTitle": true },
                        { "field": "release_date", "fieldType": "date", "displayName": "Release date", "dateRole": "planning" },
                        { "field": "musicbrainz_url", "fieldType": "externalRef", "displayName": "MusicBrainz", "externalRef": "musicbrainz" }
                    ]
                }
            ]
        });
        write_split_config(&config_path, &config);

        Self {
            app: router(ApiOptions {
                config_path,
                cache_ttl: Duration::from_millis(0),
                web_dist_path: None,
                settings_writable: true,
                content_writable: true,
            }),
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

/// Splits a combined config object into the app file (at `config_path`) and the
/// vault file (at `<vaultRoot>/.kizunashelf/config.yaml`), mirroring how the app
/// stores configuration on disk.
fn write_split_config(config_path: &Path, config: &Value) {
    let object = config.as_object().expect("config must be an object");
    let vault_root = object
        .get("vaultRoot")
        .and_then(Value::as_str)
        .expect("config must set vaultRoot");
    let mut app = serde_json::Map::new();
    let mut vault = serde_json::Map::new();
    for (key, value) in object {
        match key.as_str() {
            "vaultRoot" | "contentWritable" | "readConcurrency" => {
                app.insert(key.clone(), value.clone());
            }
            _ => {
                vault.insert(key.clone(), value.clone());
            }
        }
    }
    write_yaml_file(config_path, &Value::Object(app));
    let vault_config_path = Path::new(vault_root).join(".kizunashelf/config.yaml");
    write_yaml_file(&vault_config_path, &Value::Object(vault));
}

fn write_yaml_file(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_yaml::to_string(value).unwrap()).unwrap();
}

/// Builds a `{ app, vault }` settings request body from a combined config object,
/// matching the shape the PUT `/api/settings/config` endpoint expects.
fn split_settings_body(config: &Value) -> Value {
    let object = config.as_object().expect("config must be an object");
    let mut app = serde_json::Map::new();
    let mut vault = serde_json::Map::new();
    for (key, value) in object {
        match key.as_str() {
            "vaultRoot" | "contentWritable" | "readConcurrency" => {
                app.insert(key.clone(), value.clone());
            }
            _ => {
                vault.insert(key.clone(), value.clone());
            }
        }
    }
    json!({ "app": Value::Object(app), "vault": Value::Object(vault) })
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

fn coverage_metric<'a>(items: &'a Value, name: &str) -> &'a Value {
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == name)
        .unwrap_or_else(|| panic!("coverage metric not found: {name}"))
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

async fn start_mock_image_server() -> std::net::SocketAddr {
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
    let config_path = temp.path().join("kizunashelf.yaml");
    let config = json!({
        "vaultRoot": vault,
        "taxonomyRoot": "Taxonomy",
        "assetRoot": "Assets",
        "types": [
            {
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "filename": { "titleLanguage": "zh", "defaultTitle": true },
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh", "defaultTitle": true },
                    { "field": "cover_url", "fieldType": "image", "displayName": "Cover" },
                    { "field": "shots", "fieldType": "imageList", "displayName": "Shots" }
                ]
            }
        ]
    });
    write_split_config(&config_path, &config);
    let app = router(ApiOptions {
        config_path,
        cache_ttl: Duration::from_millis(0),
        web_dist_path: None,
        settings_writable: true,
        content_writable,
    });
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
        Some(json!({ "revision": revision, "mode": "trash" })),
    )
    .await;
    assert_eq!(deleted.0, StatusCode::OK, "{}", deleted.1);

    assert!(!vault.join("Assets/Taxonomy/Anime/Star Voyager").exists());
    assert!(vault.join(".kizunashelf/trash").exists());
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
