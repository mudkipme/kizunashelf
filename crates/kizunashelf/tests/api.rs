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
        config["types"][0]["fields"][4],
        json!({
            "field": "status",
            "fieldType": "enum",
            "displayName": "Status",
            "enumOptions": ["Backlog", "Watching", "Completed", "Paused", "Dropped"]
        })
    );
    assert_eq!(config["types"][0]["fields"][5]["fieldType"], "season");
    assert_eq!(config["types"][0]["fields"][5]["dateRole"], "planning");
    assert_eq!(config["types"][0]["fields"][5]["seasonLanguage"], "zh");

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
    let star_voyager_summary = entity_by_title(&entities["items"], "Star Voyager");
    assert_eq!(star_voyager_summary["relationCount"], 4);

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
    assert_eq!(missing.1["exists"], false);
    assert_eq!(
        missing.1["configPath"].as_str().unwrap(),
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

    let saved = request_json(&app, Method::PUT, "/api/settings/config", Some(config)).await;
    assert_eq!(saved.0, StatusCode::OK);
    assert_eq!(saved.1["exists"], true);
    assert_eq!(saved.1["config"]["types"].as_array().unwrap().len(), 1);
    assert!(config_path.is_file());

    let read_back = request_json(&app, Method::GET, "/api/settings/config", None).await;
    assert_eq!(read_back.0, StatusCode::OK);
    assert_eq!(read_back.1["exists"], true);
    assert_eq!(read_back.1["config"]["home"]["title"], "Settings Fixture");

    let health = request_json(&app, Method::GET, "/api/health", None).await;
    assert_eq!(health.0, StatusCode::OK);
    assert_eq!(health.1["entityCount"], 1);
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

    let saved = request_json(&app, Method::PUT, "/api/settings/config", Some(config)).await;
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
    fs::write(&config_path, serde_yaml::to_string(&config).unwrap()).unwrap();
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
                        { "field": "cover_url", "fieldType": "image", "displayName": "Cover" },
                        { "field": "status", "fieldType": "enum", "displayName": "Status", "enumOptions": ["Backlog", "Watching", "Completed", "Paused", "Dropped"] },
                        { "field": "season", "fieldType": "season", "displayName": "Season", "dateRole": "planning", "seasonLanguage": "zh" },
                        { "field": "complete_date", "fieldType": "date", "displayName": "Completed date", "dateRole": "completed" },
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
                        { "field": "release_date", "fieldType": "date", "displayName": "Release date", "dateRole": "planning" },
                        { "field": "igdb_url", "fieldType": "externalRef", "displayName": "IGDB", "externalRef": "igdb" },
                        { "field": "franchise", "fieldType": "relation", "displayName": "Franchise", "relationType": "franchise" },
                        { "field": "developer", "fieldType": "relation", "displayName": "Developer", "relationType": "developer" }
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
        fs::write(&config_path, serde_yaml::to_string(&config).unwrap()).unwrap();

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
status: Watching
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
