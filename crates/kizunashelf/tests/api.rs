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

    let config = server.ok_json("/api/config").await;
    assert_eq!(config["taxonomyRoot"], "Taxonomy");
    assert_eq!(config["types"].as_array().unwrap().len(), 4);
    assert_eq!(config["types"][0]["icon"], "📺");
    assert_eq!(config["types"][0]["defaultTitleLanguage"], "primary");
    assert_eq!(
        config["types"][0]["titleLanguages"],
        json!(["en", "primary", "zh"])
    );
    assert_eq!(
        config["types"][0]["dateRoles"],
        json!({
            "planning": ["season"],
            "completed": ["complete_date"]
        })
    );

    let home = server.ok_json("/api/home").await;
    assert_eq!(home["title"], "Fixture Home");
    assert_eq!(home["sections"][0]["title"], "Watching Anime");
    assert_eq!(home["sections"][0]["total"], 1);
    assert_eq!(home["sections"][0]["items"][0]["title"], "Star Voyager");

    let stats = server.ok_json("/api/stats").await;
    assert_eq!(stats["total"], 4);
    assert_eq!(stats["relations"], 10);
    assert_eq!(count_for(&stats["byType"], "Anime"), 1);
    assert_eq!(count_for(&stats["byType"], "Games"), 1);
    assert_eq!(stats["byType"][0]["icon"], "📺");
    assert_eq!(count_for(&stats["byStatus"], "Watching"), 1);

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

    let filtered = server
        .ok_json("/api/entities?type=games&status=Playing&refs=with&cover=without")
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
    let config_path = temp.path().join("missing/kizunashelf.config.json");
    let app = router(ApiOptions {
        config_path: config_path.clone(),
        cache_ttl: Duration::from_millis(0),
        web_dist_path: None,
        settings_writable: true,
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
        "relationshipFields": ["franchise"],
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
                "defaultTitleLanguage": "primary",
                "fields": {
                    "titleLanguages": {
                        "primary": ["title"],
                        "zh": ["filename"]
                    },
                    "subtitle": ["title_en"],
                    "image": ["cover_url"],
                    "status": ["status"],
                    "dateRoles": {
                        "planning": ["season"],
                        "completed": ["complete_date"]
                    },
                    "externalRefs": ["bgm_url"],
                    "relations": ["studio"]
                }
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
    let config_path = temp.path().join("kizunashelf.config.json");
    let app = router(ApiOptions {
        config_path,
        cache_ttl: Duration::from_millis(0),
        web_dist_path: None,
        settings_writable: false,
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

impl TestServer {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        write_fixture_vault(&vault);
        let config_path = temp.path().join("kizunashelf.config.json");
        let config = json!({
            "vaultRoot": vault,
            "taxonomyRoot": "Taxonomy",
            "relationshipFields": ["franchise", "related"],
            "dailyNotes": {
                "paths": ["Daily Notes"],
                "datePattern": "^(?:Daily Notes/)?(?<date>\\d{4}-\\d{2}-\\d{2})\\.md$",
                "snippetMaxLength": 120
            },
            "home": {
                "title": "Fixture Home",
                "sections": [
                    {
                        "id": "watching-anime",
                        "title": "Watching Anime",
                        "type": "anime",
                        "status": "Watching",
                        "limit": 4,
                        "sort": "title",
                        "direction": "asc"
                    },
                    {
                        "id": "playing-games",
                        "title": "Playing Games",
                        "type": "games",
                        "status": ["Playing"],
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
                    "defaultTitleLanguage": "primary",
                    "fields": {
                        "titleLanguages": {
                            "primary": ["title"],
                            "zh": ["filename"],
                            "en": ["title_en"]
                        },
                        "subtitle": ["title_en"],
                        "image": ["cover_url"],
                        "status": ["status"],
                        "dateRoles": {
                            "planning": ["season"],
                            "completed": ["complete_date"]
                        },
                        "externalRefs": ["bgm_url"],
                        "relations": ["studio"]
                    }
                },
                {
                    "id": "games",
                    "label": "Games",
                    "path": "Games",
                    "defaultTitleLanguage": "primary",
                    "fields": {
                        "titleLanguages": {
                            "primary": ["title"],
                            "zh": ["filename"],
                            "en": ["title_en"]
                        },
                        "subtitle": ["title_en"],
                        "image": ["cover_url"],
                        "status": ["status"],
                        "dateRoles": {
                            "planning": ["release_date"]
                        },
                        "externalRefs": ["igdb_url"],
                        "relations": ["developer"]
                    }
                },
                {
                    "id": "franchise",
                    "label": "Franchise",
                    "path": "Franchise",
                    "defaultTitleLanguage": "primary",
                    "fields": {
                        "titleLanguages": {
                            "primary": ["title"],
                            "zh": ["filename"]
                        },
                        "relations": ["related"]
                    }
                },
                {
                    "id": "music",
                    "label": "Music",
                    "path": "Music",
                    "defaultTitleLanguage": "primary",
                    "fields": {
                        "titleLanguages": {
                            "primary": ["title"],
                        "original": ["filename"]
                    },
                    "dateRoles": {
                        "planning": ["release_date"]
                    },
                    "externalRefs": ["musicbrainz_url"]
                }
                }
            ]
        });
        fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

        Self {
            app: router(ApiOptions {
                config_path,
                cache_ttl: Duration::from_millis(0),
                web_dist_path: None,
                settings_writable: true,
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
