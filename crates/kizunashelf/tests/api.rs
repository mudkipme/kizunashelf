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
    build_inline_router(
        vault_root,
        settings_writable,
        content_writable,
        false,
        Duration::ZERO,
    )
}

/// Keeps the resident library snapshot alive so mutation tests can reproduce an
/// external edit landing behind the native runtimes' long cache TTL.
fn cached_inline_router(
    vault_root: &Path,
    settings_writable: bool,
    content_writable: bool,
) -> Router {
    build_inline_router(
        vault_root,
        settings_writable,
        content_writable,
        false,
        Duration::from_secs(60 * 60),
    )
}

/// Like [`inline_router`] but with host-driven asset ingest enabled — the iOS
/// in-process-host posture the `plan`/`ingest` endpoints require. The network
/// server uses [`inline_router`] (off), so the host-path surface is gated there.
fn host_inline_router(vault_root: &Path, content_writable: bool) -> Router {
    build_inline_router(vault_root, true, content_writable, true, Duration::ZERO)
}

fn build_inline_router(
    vault_root: &Path,
    settings_writable: bool,
    content_writable: bool,
    host_asset_ingest: bool,
    cache_ttl: Duration,
) -> Router {
    build_inline_router_with_identity(
        vault_root,
        settings_writable,
        content_writable,
        host_asset_ingest,
        cache_ttl,
        None,
    )
}

fn build_inline_router_with_identity(
    vault_root: &Path,
    settings_writable: bool,
    content_writable: bool,
    host_asset_ingest: bool,
    cache_ttl: Duration,
    identity: Option<String>,
) -> Router {
    let token_path = vault_root
        .parent()
        .map(|parent| parent.join(".tokens.json"))
        .unwrap_or_else(|| PathBuf::from(".tokens.json"));
    router_native(
        ApiOptions {
            cache_ttl,
            web_dist_path: None,
            settings_writable,
            content_writable,
            index_cache_dir: None,
            index_cache_identity: identity,
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
async fn manual_example_schema_and_entity_work_together() {
    // Execute the published examples themselves so renamed fields and roles
    // cannot silently drift between the introductory guide and reference.
    let config_page = include_str!("../../../manual/content/reference/config.md");
    let entity_page = include_str!("../../../manual/content/concepts/anatomy-of-an-entity.md");
    let fenced = |text: &'static str, language: &str| {
        text.split_once(&format!("```{language}\n"))
            .unwrap()
            .1
            .split_once("```")
            .unwrap()
            .0
    };
    let temp = TempDir::new().unwrap();
    let vault = temp.path();
    write_file(
        &vault.join("KizunaShelf/config.yaml"),
        fenced(config_page, "yaml"),
    );
    write_file(
        &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
        fenced(entity_page, "markdown"),
    );
    write_file(
        &vault.join("Taxonomy/Franchise/Steins;Gate.md"),
        "Notes about this franchise.\n",
    );
    let app = inline_router(vault, true, true);
    let (status, detail) = request_json(
        &app,
        Method::GET,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["entity"]["titles"]["en"], "Steins;Gate 0 (Anime)");
    assert_eq!(detail["entity"]["titles"]["zh"], "命运石之门0");
    assert_eq!(detail["entity"]["title"], "シュタインズ・ゲート ゼロ");
    assert!(has_entity_title(&detail["relatedEntities"], "Steins;Gate"));
    let (_, settings) = request_json(&app, Method::GET, "/api/settings/config", None).await;
    assert!(settings["error"].is_null(), "{settings}");
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
    assert_eq!(
        capabilities["vaultWatchEnabled"],
        cfg!(feature = "native-vfs-watch")
    );
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
    // Languages carry only their code now; clients render the localized display
    // name from it (the core owns no English language label).
    assert!(language_list
        .iter()
        .any(|language| language["code"] == "ja"));

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
    assert_eq!(
        home["lists"],
        json!([]),
        "retired Home config is ignored safely"
    );

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
    // Activity buckets dated entities into a year × month matrix. Steins;Gate 0 (Anime)
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
    assert_eq!(cleanup["missingCover"][0]["id"], "games:Robotics;Notes");
    assert_eq!(
        cleanup["missingExternalRefs"][0]["id"],
        "music:Opening Theme"
    );
    assert_eq!(cleanup["isolated"].as_array().unwrap().len(), 0);

    let entities = server.ok_json("/api/entities").await;
    assert_eq!(entities["total"], 4);
    assert!(has_entity_title(
        &entities["items"],
        "シュタインズ・ゲート ゼロ"
    ));
    assert!(has_entity_title(&entities["items"], "Robotics;Notes"));
    assert_eq!(
        entities["items"][0]["titles"]["zh"],
        "Steins;Gate 0 (Anime)"
    );
    assert_eq!(entities["items"][0]["titles"]["en"], "Amadeus of Zero");
    assert_eq!(
        entities["items"][0]["titles"]["title_original"],
        "シュタインズ・ゲート ゼロ"
    );
    let steins_gate_summary = entity_by_title(&entities["items"], "シュタインズ・ゲート ゼロ");
    assert_eq!(steins_gate_summary["relationCount"], 4);
    let completed_date = steins_gate_summary["dates"]
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
    assert_eq!(
        english_title_sort["items"][0]["id"],
        "anime:Steins;Gate 0 (Anime)"
    );

    let relation_count_sort = server
        .ok_json("/api/entities?sort=relationCount&direction=desc")
        .await;
    assert_eq!(
        relation_count_sort["items"][0]["id"],
        "anime:Steins;Gate 0 (Anime)"
    );

    // Criteria-shaped browsing lives on the smart-list evaluator (the library
    // browser is an unsaved smart list), so these are the same schema-driven
    // questions the entity list's `filters` param used to answer.
    // A field-scoped link rule: entities whose `franchise` relation points to
    // Steins;Gate.
    let franchise_filtered = server
        .preview(
            None,
            json!([{ "kind": "linksTo", "field": "franchise", "values": ["Steins;Gate"] }]),
        )
        .await;
    assert_eq!(franchise_filtered["total"], 2);
    assert!(has_entity_title(
        &franchise_filtered["items"],
        "シュタインズ・ゲート ゼロ"
    ));
    assert!(has_entity_title(
        &franchise_filtered["items"],
        "Robotics;Notes"
    ));

    // An enum field, any-of: one rule per value under an "any" subgroup.
    let (status, status_filtered) = request_json(
        &server.app,
        Method::POST,
        "/api/smart-lists/preview",
        Some(json!({
            "filters": {
                "conjunction": "all",
                "groups": [{
                    "conjunction": "any",
                    "rules": [
                        { "kind": "compare", "field": "status", "op": "eq", "value": "Watching" },
                        { "kind": "compare", "field": "status", "op": "eq", "value": "Playing" },
                    ],
                }],
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{status_filtered}");
    assert_eq!(status_filtered["total"], 2);
    assert!(has_entity_title(
        &status_filtered["items"],
        "シュタインズ・ゲート ゼロ"
    ));
    assert!(has_entity_title(
        &status_filtered["items"],
        "Robotics;Notes"
    ));

    // A list field matches by membership, any-of.
    let genre_filtered = server
        .preview(
            Some("games"),
            json!([{ "kind": "contains", "field": "genres", "mode": "any", "values": ["Strategy", "RPG"] }]),
        )
        .await;
    assert_eq!(genre_filtered["total"], 1);
    assert_eq!(genre_filtered["items"][0]["id"], "games:Robotics;Notes");

    let missing_genre_filtered = server
        .preview(
            Some("games"),
            json!([{ "kind": "contains", "field": "genres", "mode": "any", "values": ["RPG"] }]),
        )
        .await;
    assert_eq!(missing_genre_filtered["total"], 0);

    let favorite_filtered = server
        .preview(
            None,
            json!([{ "kind": "compare", "field": "favorite", "op": "eq", "boolean": true }]),
        )
        .await;
    assert_eq!(favorite_filtered["total"], 1);
    assert_eq!(
        favorite_filtered["items"][0]["id"],
        "anime:Steins;Gate 0 (Anime)"
    );

    let not_favorite_filtered = server
        .preview(
            None,
            json!([{ "kind": "compare", "field": "favorite", "op": "eq", "boolean": false }]),
        )
        .await;
    assert_eq!(not_favorite_filtered["total"], 1);
    assert_eq!(
        not_favorite_filtered["items"][0]["id"],
        "games:Robotics;Notes"
    );

    let searched = server.ok_json("/api/entities?q=worldline").await;
    assert_eq!(searched["total"], 1);
    assert_eq!(searched["items"][0]["id"], "anime:Steins;Gate 0 (Anime)");

    let searched_title_language = server
        .ok_json(&format!(
            "/api/entities?q={}",
            urlencoding::encode("Robot Club Diary")
        ))
        .await;
    assert_eq!(searched_title_language["total"], 1);
    assert_eq!(
        searched_title_language["items"][0]["id"],
        "games:Robotics;Notes"
    );

    let by_relation = server
        .ok_json(&format!(
            "/api/entities?relation={}",
            urlencoding::encode("franchise:Steins;Gate")
        ))
        .await;
    assert_eq!(by_relation["total"], 2);
    assert!(has_entity_title(
        &by_relation["items"],
        "シュタインズ・ゲート ゼロ"
    ));
    assert!(has_entity_title(&by_relation["items"], "Robotics;Notes"));

    let detail = server
        .ok_json(&format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ))
        .await;
    assert_eq!(detail["entity"]["title"], "シュタインズ・ゲート ゼロ");
    assert_eq!(detail["entity"]["titles"]["zh"], "Steins;Gate 0 (Anime)");
    assert_eq!(detail["entity"]["titles"]["en"], "Amadeus of Zero");
    assert_eq!(
        detail["entity"]["titles"]["title_original"],
        "シュタインズ・ゲート ゼロ"
    );
    assert_eq!(
        detail["entity"]["path"],
        "Taxonomy/Anime/Steins;Gate 0 (Anime).md"
    );
    assert_eq!(detail["relations"].as_array().unwrap().len(), 4);
    assert_eq!(relation_field_count(&detail["relations"], "daily-note"), 0);
    assert_eq!(relation_field_count(&detail["relations"], "body"), 2);
    assert!(has_entity_title(
        &detail["relatedEntities"],
        "Robotics;Notes"
    ));
    assert!(has_entity_title(&detail["relatedEntities"], "Steins;Gate"));
    assert_eq!(
        unique_relation_target_count("anime:Steins;Gate 0 (Anime)", &detail["relations"]),
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
#[cfg(feature = "native-vfs-watch")]
async fn vault_changes_long_poll_wakes_after_an_external_edit() {
    let server = TestServer::new();
    // Loading the library starts the native watcher before the external write.
    server.ok_json("/api/health").await;
    let app = server.app.clone();
    let waiter = tokio::spawn(async move {
        request_json(&app, Method::GET, "/api/vault/changes?after=0", None).await
    });
    tokio::task::yield_now().await;

    write_file(
        &server.vault.join("Taxonomy/Anime/External Edit.md"),
        "---\ntitle: External Edit\n---\n",
    );
    let response = tokio::time::timeout(Duration::from_secs(5), waiter)
        .await
        .expect("long poll woke after the watcher debounce")
        .unwrap();
    assert_eq!(response.0, StatusCode::OK, "{}", response.1);
    assert_eq!(response.1["supported"], true);
    assert_eq!(response.1["changed"], true);
    assert!(response.1["generation"].as_u64().unwrap() > 0);
}

#[tokio::test]
#[cfg(not(feature = "native-vfs-watch"))]
async fn vault_changes_reports_unsupported_without_native_watch() {
    let server = TestServer::new();
    let response = request_json(&server.app, Method::GET, "/api/vault/changes?after=0", None).await;
    assert_eq!(response.0, StatusCode::OK, "{}", response.1);
    assert_eq!(response.1["supported"], false);
    assert_eq!(response.1["changed"], false);
    assert_eq!(response.1["generation"], 0);
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
    assert!(resolved.get("homeSections").is_none());
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
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ))
        .await;
    let revision = detail["entity"]["revision"].as_str().unwrap();
    let updated = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ),
        Some(json!({
            "revision": revision,
            "frontmatter": {
                "status": "Completed",
                "progress": 12,
                "bgm_url": null
            },
            "body": "Updated body with [[Robotics;Notes]]."
        })),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);
    assert_eq!(updated.1["entity"]["frontmatter"]["status"], "Completed");
    assert_eq!(updated.1["entity"]["frontmatter"]["progress"], 12);
    assert!(updated.1["entity"]["frontmatter"].get("bgm_url").is_none());
    assert_eq!(
        updated.1["entity"]["body"],
        "Updated body with [[Robotics;Notes]]."
    );

    let stale = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
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
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ),
        Some(json!({
            "revision": rename_revision,
            "renameTo": "  Steins;Gate 0 (Anime) Renamed  "
        })),
    )
    .await;
    assert_eq!(renamed.0, StatusCode::OK, "{}", renamed.1);
    assert_eq!(
        renamed.1["entity"]["id"],
        "anime:Steins;Gate 0 (Anime) Renamed"
    );
    assert_eq!(
        renamed.1["entity"]["basename"],
        "Steins;Gate 0 (Anime) Renamed"
    );
    assert_eq!(
        renamed.1["entity"]["path"],
        "Taxonomy/Anime/Steins;Gate 0 (Anime) Renamed.md"
    );

    let invalid_rename_revision = renamed.1["entity"]["revision"].as_str().unwrap();
    let invalid_rename = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Steins;Gate 0 (Anime) Renamed")
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
async fn concurrent_entity_mutations_accept_one_revision_once() {
    let server = TestServer::new();
    let path = format!(
        "/api/entities/{}",
        urlencoding::encode("anime:Steins;Gate 0 (Anime)")
    );
    let detail = server.ok_json(&path).await;
    let revision = detail["entity"]["revision"].as_str().unwrap().to_string();

    let status_update = request_json(
        &server.app,
        Method::POST,
        &path,
        Some(json!({
            "revision": revision,
            "frontmatter": { "status": "Completed" }
        })),
    );
    let progress_update = request_json(
        &server.app,
        Method::POST,
        &path,
        Some(json!({
            "revision": detail["entity"]["revision"],
            "frontmatter": { "progress": 99 }
        })),
    );
    let (status_result, progress_result) = tokio::join!(status_update, progress_update);

    let statuses = [status_result.0, progress_result.0];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1,
        "exactly one concurrent mutation should commit: {statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CONFLICT)
            .count(),
        1,
        "the other mutation must observe the consumed revision: {statuses:?}"
    );

    let current = server.ok_json(&path).await;
    let entity = &current["entity"];
    let status_won = entity["frontmatter"]["status"] == "Completed";
    let progress_won = entity["frontmatter"]["progress"] == 99;
    assert_ne!(
        status_won, progress_won,
        "the losing mutation must not leak into the file: {entity}"
    );
}

#[tokio::test]
async fn rename_repoints_inbound_wikilinks_in_managed_files() {
    let server = TestServer::new();

    // A user-curated list under `KizunaShelf/Lists/` links Steins;Gate 0 (Anime). Lists are
    // never indexed into the relation graph (read live from the VFS), so a rename
    // must sweep the directory directly. The aliased Robotics;Notes link must survive.
    write_file(
        &server.vault.join("KizunaShelf/Lists/Favorites.md"),
        "Personal favorites.\n\n- [[Steins;Gate 0 (Anime)]]\n- [[Robotics;Notes|the quest]]\n",
    );

    // Steins;Gate 0 (Anime) is linked from Robotics;Notes's body and from the daily note.
    let detail = server
        .ok_json(&format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ))
        .await;
    let revision = detail["entity"]["revision"].as_str().unwrap();

    let renamed = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ),
        Some(json!({
            "revision": revision,
            "renameTo": "Steins;Gate 0 (Anime) Redux"
        })),
    )
    .await;
    assert_eq!(renamed.0, StatusCode::OK, "{}", renamed.1);
    assert_eq!(
        renamed.1["entity"]["id"],
        "anime:Steins;Gate 0 (Anime) Redux"
    );
    // Three inbound links across three files: Robotics;Notes's body, the daily note,
    // and the Favorites list page.
    assert_eq!(renamed.1["updatedLinks"]["files"], 3);
    assert_eq!(renamed.1["updatedLinks"]["links"], 3);

    // The raw files were rewritten in place (no YAML round-trip): the inbound
    // links now point at the new basename while the unrelated `[[Steins;Gate]]`
    // franchise link and the `[[Robotics;Notes|the quest]]` alias survive verbatim.
    let moon = fs::read_to_string(server.vault.join("Taxonomy/Games/Robotics;Notes.md")).unwrap();
    assert!(moon.contains("[[Steins;Gate 0 (Anime) Redux]]"), "{moon}");
    assert!(!moon.contains("[[Steins;Gate 0 (Anime)]]"), "{moon}");
    assert!(moon.contains("franchise: \"[[Steins;Gate]]\""), "{moon}");

    let daily = fs::read_to_string(server.vault.join("Daily Notes/2025-04-21.md")).unwrap();
    assert!(daily.contains("[[Steins;Gate 0 (Anime) Redux]]"), "{daily}");
    assert!(
        daily.contains("[[Robotics;Notes|the quest]]"),
        "alias link preserved: {daily}"
    );

    // The list page (not in the relation graph — swept directly) was repointed
    // too, and its unrelated aliased Robotics;Notes link is untouched.
    let list = fs::read_to_string(server.vault.join("KizunaShelf/Lists/Favorites.md")).unwrap();
    assert!(list.contains("[[Steins;Gate 0 (Anime) Redux]]"), "{list}");
    assert!(!list.contains("[[Steins;Gate 0 (Anime)]]"), "{list}");
    assert!(list.contains("[[Robotics;Notes|the quest]]"), "{list}");

    // A plain (non-rename) update reports no link changes.
    let redux = server
        .ok_json(&format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Steins;Gate 0 (Anime) Redux")
        ))
        .await;
    let redux_revision = redux["entity"]["revision"].as_str().unwrap();
    let touched = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}",
            urlencoding::encode("anime:Steins;Gate 0 (Anime) Redux")
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
async fn external_search_routes_a_pasted_url_to_the_owning_provider() {
    // In the fixture, `anime` maps only bangumi and `games` maps only igdb.
    let server = TestServer::new();

    // An IGDB URL under `type=anime` routes to igdb — which anime doesn't map — so
    // nothing is searched: no results, and bangumi (configured + keyless) is never
    // queried, so no provider records an error. Proves the URL overrides the
    // configured provider set while still honoring the type filter.
    let igdb_under_anime = server
        .ok_json("/api/external/search?type=anime&q=https://www.igdb.com/games/celeste")
        .await;
    assert_eq!(igdb_under_anime["items"].as_array().unwrap().len(), 0);
    for provider in igdb_under_anime["providers"].as_array().unwrap() {
        assert!(
            provider["error"].is_null(),
            "no provider should be queried for a routed-away URL: {provider}"
        );
    }

    // A URL no known provider claims returns an empty list without any request.
    let unknown = server
        .ok_json("/api/external/search?type=anime&q=https://example.com/foo/123456")
        .await;
    assert_eq!(unknown["items"].as_array().unwrap().len(), 0);

    // A Bangumi URL under `type=games` routes to bangumi, which games doesn't map:
    // igdb (the configured provider) is ignored and nothing is searched.
    let bangumi_under_games = server
        .ok_json("/api/external/search?type=games&q=https://bgm.tv/subject/998877")
        .await;
    assert_eq!(bangumi_under_games["items"].as_array().unwrap().len(), 0);
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
        .json("/api/external/search?provider=missing&type=anime&q=Star")
        .await;
    assert_eq!(unknown.0, StatusCode::BAD_REQUEST);
    assert_eq!(unknown.1["error"], "Unknown external provider");

    // Cross-type search is gone: every search is scoped to one type, so `type=all`
    // resolves like any other unknown type id and an omitted type is rejected at
    // deserialization.
    let all_type = server.json("/api/external/search?type=all&q=").await;
    assert_eq!(all_type.0, StatusCode::BAD_REQUEST);
    assert_eq!(all_type.1["error"], "Unknown entity type");

    let missing_type = server.status("/api/external/search?q=").await;
    assert_eq!(missing_type, StatusCode::BAD_REQUEST);

    let unknown_type = server
        .json("/api/external/search?type=animation&q=Star")
        .await;
    assert_eq!(unknown_type.0, StatusCode::BAD_REQUEST);
    assert_eq!(unknown_type.1["error"], "Unknown entity type");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_quick_adds_of_one_candidate_create_one_entity() {
    // A double-click (or the share extension racing the app) must not mint a
    // disambiguated duplicate: the "already exists?" check and the write happen
    // under one lock, so every other request returns the first one's entity.
    let server = TestServer::new();
    let candidate = json!({
        "provider": "bangumi",
        "sourceId": "556677",
        "url": "https://bgm.tv/subject/556677",
        "title": "Raced Show",
        "titles": { "zh": "竞速番" },
        "metadata": {}
    });
    let adds: Vec<_> = (0..4)
        .map(|_| {
            let app = server.app.clone();
            let payload = json!({ "type": "anime", "candidate": candidate.clone() });
            tokio::spawn(async move {
                request_json(&app, Method::POST, "/api/external/quick-add", Some(payload)).await
            })
        })
        .collect();
    let mut ids = HashSet::new();
    let mut created = 0;
    for add in adds {
        let (status, body) = add.await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        ids.insert(body["entity"]["id"].as_str().unwrap().to_string());
        if body["alreadyExisted"] == false {
            created += 1;
        }
    }
    assert_eq!(created, 1, "exactly one request creates the entity");
    assert_eq!(
        ids.len(),
        1,
        "every request resolves to that entity: {ids:?}"
    );
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
async fn quick_add_defaults_status_to_first_option_for_requested_canonical() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir_all(vault.join("Taxonomy/Anime")).unwrap();
    // Anime maps the bangumi provider (so a candidate maps offline) and models a
    // status field whose planning canonical lists two options — the *first*
    // ("Backlog") is the write target the default should pick.
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
                    { "field": "title", "fieldType": "title", "titleLanguage": "en" },
                    { "field": "status", "fieldType": "enum",
                      "enumOptions": ["Backlog", "Planned", "Watching", "Completed"],
                      "enumRole": "status",
                      "statusValues": {
                          "planning": ["Backlog", "Planned"], "ongoing": ["Watching"],
                          "completed": ["Completed"]
                      } },
                    { "field": "bgm_url", "fieldType": "externalRef",
                      "externalRef": "bangumi", "externalTypes": ["anime"] }
                ]
            }
        ]
    });
    write_vault_config(&vault, &config);
    let app = inline_router(&vault, true, true);

    // No `coverUrl` and no episodes section, so the whole flow stays offline.
    let candidate = json!({
        "provider": "bangumi",
        "sourceId": "112233",
        "url": "https://bgm.tv/subject/112233",
        "title": "Some Show",
        "titles": { "en": "Some Show" },
        "metadata": {}
    });
    let (status, created) = request_json(
        &app,
        Method::POST,
        "/api/external/quick-add",
        Some(json!({ "type": "anime", "candidate": candidate.clone() })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    // The candidate mapped no status, so quick-add seeds the first planning option.
    assert_eq!(created["entity"]["frontmatter"]["status"], "Backlog");

    // A status-specific entry point can opt into the first ongoing write target.
    let mut ongoing_candidate = candidate;
    ongoing_candidate["sourceId"] = json!("445566");
    ongoing_candidate["url"] = json!("https://bgm.tv/subject/445566");
    ongoing_candidate["title"] = json!("Another Show");
    ongoing_candidate["titles"] = json!({ "en": "Another Show" });
    let (status, ongoing) = request_json(
        &app,
        Method::POST,
        "/api/external/quick-add",
        Some(json!({
            "type": "anime",
            "candidate": ongoing_candidate,
            "defaultStatus": "ongoing"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ongoing}");
    assert_eq!(ongoing["entity"]["frontmatter"]["status"], "Watching");
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
async fn entity_update_serializes_a_frontmatter_draft() {
    let server = TestServer::new();
    let path = format!(
        "/api/entities/{}",
        urlencoding::encode("anime:Steins;Gate 0 (Anime)")
    );
    let detail = server.ok_json(&path).await;
    let revision = detail["entity"]["revision"].as_str().unwrap();

    // The editor sends its draft verbatim: strings/bools/string-lists. The core
    // trims, wraps relations, and deletes keys the draft no longer carries
    // (`season` and `complete_date` were cleared).
    let (status, updated) = request_json(
        &server.app,
        Method::POST,
        &path,
        Some(json!({
            "revision": revision,
            "frontmatterDraft": {
                "title": "Steins;Gate 0",
                "title_en": "Amadeus of Zero",
                "title_original": "シュタインズ・ゲート ゼロ",
                "status": " Completed ",
                "favorite": true,
                "cover_url": "https://img.example/star.jpg",
                "bgm_url": "https://bgm.example/star",
                "franchise": ["Steins;Gate"],
                "studio": ["Nova Studio", "[[Second Studio]]", "  "]
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    let frontmatter = &updated["entity"]["frontmatter"];
    assert_eq!(frontmatter["status"], "Completed");
    assert_eq!(frontmatter["favorite"], true);
    assert_eq!(frontmatter["franchise"], json!(["[[Steins;Gate]]"]));
    assert_eq!(
        frontmatter["studio"],
        json!(["[[Nova Studio]]", "[[Second Studio]]"])
    );
    assert!(
        frontmatter.get("season").is_none(),
        "cleared key is deleted"
    );
    assert!(frontmatter.get("complete_date").is_none());
    // The body was not part of the request and is untouched.
    assert_eq!(updated["entity"]["body"], detail["entity"]["body"]);
}

#[tokio::test]
async fn notes_only_draft_save_preserves_frontmatter_values_on_disk() {
    let server = TestServer::new();
    let original = json!({
        "title": "Preservation", "nested": {"edition": 2, "optional": null},
        "mixed": [1, true, null, {"code": "007"}], "empty": null,
        "empty_list": [], "empty_text": "", "padded": "  unchanged  ",
        "franchise": ["[[Steins;Gate|Alias]]"]
    });
    let (status, created) = request_json(
        &server.app,
        Method::POST,
        "/api/entities",
        Some(json!({
            "type": "anime", "basename": "Preservation", "frontmatter": original, "body": "Before"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let path = format!(
        "/api/entities/{}",
        urlencoding::encode(created["entity"]["id"].as_str().unwrap())
    );
    let (status, updated) = request_json(&server.app, Method::POST, &path, Some(json!({
        "revision": created["entity"]["revision"], "frontmatterDraft": created["entity"]["frontmatter"], "body": "After"
    }))).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["entity"]["frontmatter"], original);
    let reread = server.ok_json(&path).await;
    assert_eq!(reread["entity"]["frontmatter"], original);
    assert_eq!(reread["entity"]["body"].as_str().unwrap().trim(), "After");
}

#[tokio::test]
async fn entity_create_serializes_a_frontmatter_draft() {
    let server = TestServer::new();
    let (status, created) = request_json(
        &server.app,
        Method::POST,
        "/api/entities",
        Some(json!({
            "type": "games",
            "basename": "Solar Draft",
            "frontmatterDraft": {
                "title": "Solar Draft",
                "status": "Backlog",
                "genres": [" RPG ", ""],
                "developer": ["Orbit Dev"],
                "favorite": ""
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let frontmatter = &created["entity"]["frontmatter"];
    assert_eq!(frontmatter["genres"], json!(["RPG"]));
    assert_eq!(frontmatter["developer"], json!(["[[Orbit Dev]]"]));
    assert!(
        frontmatter.get("favorite").is_none(),
        "an empty draft value is never written"
    );
}

/// A vault whose anime type maps a bangumi text field and an external body
/// section, plus one entity with hand-written Summary content — the review/apply
/// fixture.
fn external_apply_fixture() -> (Router, PathBuf, TempDir) {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir_all(vault.join("Taxonomy/Anime")).unwrap();
    fs::write(
        vault.join("Taxonomy/Anime/Show.md"),
        r#"---
title: Show
---
## Summary

Old words.

## Keep

Kept.
"#,
    )
    .unwrap();
    let config = json!({
        "taxonomyRoot": "Taxonomy",
        "dailyNotes": { "paths": ["Daily Notes"], "dateFormat": "YYYY-MM-DD" },
        "types": [
            {
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                    { "field": "note", "fieldType": "text",
                      "externalFields": [{ "source": "bangumi", "field": "note" }] },
                    { "field": "bgm_url", "fieldType": "externalRef", "externalRef": "bangumi" }
                ],
                "bodySections": [
                    { "heading": "Summary", "kind": "external",
                      "externalFields": [{ "source": "bangumi", "field": "summary" }] }
                ]
            }
        ]
    });
    write_vault_config(&vault, &config);
    let app = inline_router(&vault, true, true);
    (app, vault, temp)
}

fn bangumi_candidate() -> Value {
    json!({
        "provider": "bangumi",
        "sourceId": "5",
        "url": "https://bgm.tv/subject/5",
        "title": "Show",
        "titles": { "zh": "Show" },
        "metadata": { "note": "Great.", "summary": "From provider." }
    })
}

/// Exercises the thin-search → detail → Markdown path against TMDB. Run with
/// KIZUNASHELF_TMDB_API_KEY set; ordinary tests never require network access.
#[tokio::test]
#[ignore = "hits live TMDB API; needs KIZUNASHELF_TMDB_API_KEY"]
async fn external_match_preserves_chinese_summary_live() {
    for language in ["zh-Hans", "zh-Hant"] {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        write_vault_config(
            &vault,
            &json!({
                "taxonomyRoot": "Taxonomy",
                "types": [{
                    "id": "movie", "label": "Movie", "path": "Movies",
                    "fields": [
                        { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                        { "field": "source", "fieldType": "externalRef",
                          "externalRef": "tmdb", "externalTypes": ["movie"] }
                    ],
                    "bodySections": [{ "heading": "Summary", "kind": "external",
                        "externalFields": [{ "source": "tmdb", "field": "overview" }] }]
                }]
            }),
        );
        write_file(
            &vault.join("Taxonomy/Movies/Film.md"),
            "---\ntitle: Film\n---\n",
        );
        let app = inline_router(&vault, true, true);
        let (status, search) = request_json(
            &app,
            Method::GET,
            &format!(
                "/api/external/search?type=movie&provider=tmdb&q=Inception&language={language}"
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let candidate = search["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| &item["candidate"])
            .find(|candidate| {
                candidate["url"]
                    .as_str()
                    .is_some_and(|url| url.ends_with("/27205"))
            })
            .expect("TMDB should return Inception");
        assert_eq!(candidate["needsDetail"], true);
        let overview = candidate["metadata"]["overview"].as_str().unwrap();
        assert!(overview
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
        let path = "/api/entities/movie%3AFilm";
        let (status, review) = request_json(
            &app,
            Method::POST,
            &format!("{path}/external/review"),
            Some(json!({ "candidate": candidate, "language": language })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(review["sections"][0]["markdown"], overview);
        assert_ne!(review["candidate"]["needsDetail"], true);

        // Both the normal reviewed-candidate path and direct apply of a thin
        // candidate must preserve the language. Each apply gets a fresh revision.
        for incoming in [&review["candidate"], candidate] {
            let (_, detail) = request_json(&app, Method::GET, path, None).await;
            let (status, applied) = request_json(
                &app,
                Method::POST,
                &format!("{path}/external/apply"),
                Some(json!({
                    "revision": detail["entity"]["revision"],
                    "candidate": incoming, "language": language,
                    "fields": [], "sections": [review["sections"][0]["key"]]
                })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                applied["entity"]["body"].as_str().unwrap().trim(),
                format!("## Summary\n\n{overview}")
            );
        }
        let saved = fs::read_to_string(vault.join("Taxonomy/Movies/Film.md")).unwrap();
        assert!(saved.contains(overview));
    }
}

#[tokio::test]
async fn external_review_reports_defaults_against_the_entity() {
    let (app, _vault, _temp) = external_apply_fixture();
    let (status, review) = request_json(
        &app,
        Method::POST,
        &format!(
            "/api/entities/{}/external/review",
            urlencoding::encode("anime:Show")
        ),
        Some(json!({ "candidate": bangumi_candidate() })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert_eq!(review["entityType"], "anime");

    let field = |name: &str| {
        review["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["field"] == name)
            .unwrap_or_else(|| panic!("field {name} missing: {review}"))
            .clone()
    };
    // The candidate title equals the current one: a no-op, locked off.
    let title = field("title");
    assert_eq!(title["current"], "Show");
    assert_eq!(title["selected"], false);
    assert_eq!(title["locked"], true);
    // An empty target defaults on.
    let note = field("note");
    assert_eq!(note["value"], "Great.");
    assert!(note.get("current").is_none());
    assert_eq!(note["selected"], true);
    assert_eq!(note["locked"], false);
    // The external ref is the refresh anchor: locked on.
    let bgm = field("bgm_url");
    assert_eq!(bgm["value"], "https://bgm.tv/subject/5");
    assert_eq!(bgm["selected"], true);
    assert_eq!(bgm["locked"], true);

    // The Summary section exists with different content: off but editable, and
    // applying would replace it rather than append.
    let section = &review["sections"][0];
    assert_eq!(section["key"], "Summary:bangumi:summary");
    assert_eq!(section["markdown"], "From provider.");
    assert_eq!(section["selected"], false);
    assert_eq!(section["locked"], false);
    assert_eq!(section["exists"], true);
}

#[tokio::test]
async fn external_apply_merges_fields_and_splices_sections() {
    let (app, _vault, _temp) = external_apply_fixture();
    let path = format!("/api/entities/{}", urlencoding::encode("anime:Show"));
    let detail = request_json(&app, Method::GET, &path, None).await.1;
    let revision = detail["entity"]["revision"].as_str().unwrap();

    // Nothing selected is rejected before any write.
    let (status, body) = request_json(
        &app,
        Method::POST,
        &format!("{path}/external/apply"),
        Some(json!({
            "revision": revision,
            "candidate": bangumi_candidate(),
            "fields": [],
            "sections": []
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, applied) = request_json(
        &app,
        Method::POST,
        &format!("{path}/external/apply"),
        Some(json!({
            "revision": revision,
            "candidate": bangumi_candidate(),
            "fields": ["bgm_url", "note"],
            "sections": ["Summary:bangumi:summary"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    let frontmatter = &applied["entity"]["frontmatter"];
    assert_eq!(frontmatter["bgm_url"], "https://bgm.tv/subject/5");
    assert_eq!(frontmatter["note"], "Great.");
    // The unselected title stays untouched, the Summary content is replaced, and
    // the neighboring section survives byte-for-byte.
    assert_eq!(frontmatter["title"], "Show");
    assert_eq!(
        applied["entity"]["body"],
        "## Summary\n\nFrom provider.\n\n## Keep\n\nKept."
    );

    // The pre-apply revision is now stale: a second apply must 409.
    let (status, conflict) = request_json(
        &app,
        Method::POST,
        &format!("{path}/external/apply"),
        Some(json!({
            "revision": revision,
            "candidate": bangumi_candidate(),
            "fields": ["note"],
            "sections": []
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
}

#[tokio::test]
async fn external_apply_is_forbidden_in_read_only_mode() {
    let server = TestServer::read_only();
    let (status, body) = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}/external/apply",
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ),
        Some(json!({
            "revision": "whatever",
            "candidate": bangumi_candidate(),
            "fields": ["bgm_url"],
            "sections": []
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

#[tokio::test]
async fn calendar_endpoints_include_metadata_and_daily_notes_from_temp_vault() {
    let server = TestServer::new();

    let calendar = server.ok_json("/api/calendar?year=2025&month=4").await;
    let days = calendar["days"].as_array().unwrap();
    // Every source a date can come from lands on the same grid — there is no
    // filter that narrows it, so the month holds taxonomy dates and daily-note
    // mentions together.
    assert_eq!(
        days.iter()
            .filter(|day| !day["items"].as_array().unwrap().is_empty())
            .count(),
        3
    );
    let sum = |key: &str| -> i64 {
        days.iter()
            .map(|day| day["counts"][key].as_i64().unwrap_or_default())
            .sum()
    };
    assert_eq!(sum("total"), 4);
    assert_eq!(sum("taxonomy"), 2);
    assert_eq!(sum("dailyNotes"), 2);

    let april_21 = calendar["days"]
        .as_array()
        .unwrap()
        .iter()
        .find(|day| day["date"] == "2025-04-21")
        .unwrap();
    assert_eq!(april_21["counts"]["dailyNotes"], 2);
    assert!(has_entity_title(
        &april_21["items"],
        "シュタインズ・ゲート ゼロ"
    ));
    assert!(has_entity_title(&april_21["items"], "Robotics;Notes"));

    // The type filter is the one filter left, and it narrows every source at once.
    let anime_only = server
        .ok_json("/api/calendar?year=2025&month=4&type=anime")
        .await;
    assert_eq!(anime_only["filters"]["type"], "anime");
    assert!(anime_only["days"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|day| day["items"].as_array().unwrap())
        .all(|item| item["entity"]["type"] == "anime"));

    let games_only = server
        .ok_json("/api/calendar?year=2025&month=4&type=games")
        .await;
    assert_eq!(
        games_only["days"][20]["items"][0]["entries"][0]["notePath"],
        "Daily Notes/2025-04-21.md"
    );

    // The activity feed surfaces the same dated entities, grouped by (date, entity).
    let activity = server.ok_json("/api/activity").await;
    let items = activity["items"].as_array().unwrap();
    assert!(!items.is_empty());
    // Steins;Gate 0 (Anime)'s completed date stamp shows up among its activity items (it
    // appears in several — a daily-note mention and date stamps on other dates).
    assert!(items
        .iter()
        .filter(|item| item["entity"]["id"] == "anime:Steins;Gate 0 (Anime)")
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
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ))
        .await;
    assert_eq!(dates["entityId"], "anime:Steins;Gate 0 (Anime)");
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
        urlencoding::encode("anime:Steins;Gate 0 (Anime)")
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
    assert!(line.contains("[[Steins;Gate 0 (Anime)]]"), "{line}");
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
    let entity = urlencoding::encode("anime:Steins;Gate 0 (Anime)");
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
    let entity = urlencoding::encode("anime:Steins;Gate 0 (Anime)");
    let log = format!("/api/entities/{entity}/log");

    async fn revision_of(server: &TestServer, entity: &str) -> String {
        server.ok_json(&format!("/api/entities/{entity}")).await["entity"]["revision"]
            .as_str()
            .unwrap()
            .to_string()
    }

    // Steins;Gate 0 (Anime) starts as `Watching` (ongoing). A dry-run `completed` log
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
    let id = urlencoding::encode("anime:Steins;Gate 0 (Anime)");
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
    let entity = urlencoding::encode("anime:Steins;Gate 0 (Anime)");
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
    assert!(read_back.1["vault"].get("home").is_none());
}

#[tokio::test]
async fn settings_config_keeps_malformed_existing_file_out_of_onboarding() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    let config_path = vault.join("KizunaShelf/config.yaml");
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    let malformed = "taxonomyRoot: [unterminated\n";
    std::fs::write(&config_path, malformed).unwrap();
    let app = inline_router(&vault, true, true);

    let response = request_json(&app, Method::GET, "/api/settings/config", None).await;

    assert_eq!(response.0, StatusCode::OK, "{}", response.1);
    assert_eq!(response.1["vaultExists"], true, "{}", response.1);
    assert!(response.1["vault"].is_null(), "{}", response.1);
    assert!(response.1["error"]
        .as_str()
        .unwrap_or_default()
        .contains("invalid vault config"));
    assert_eq!(std::fs::read_to_string(config_path).unwrap(), malformed);
}

#[tokio::test]
async fn settings_config_reports_invalid_paths_as_existing_config_errors() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({ "taxonomyRoot": "../outside", "types": [] }),
    );
    let app = inline_router(&vault, true, true);

    let response = request_json(&app, Method::GET, "/api/settings/config", None).await;

    assert_eq!(response.0, StatusCode::OK, "{}", response.1);
    assert_eq!(response.1["vaultExists"], true, "{}", response.1);
    assert!(response.1["vault"].is_null(), "{}", response.1);
    assert!(response.1["error"]
        .as_str()
        .unwrap_or_default()
        .contains("taxonomyRoot cannot contain parent directory components"));
}

#[tokio::test]
async fn home_uses_the_pinned_smart_lists_first_view_and_preserves_its_document() {
    let server = TestServer::new();
    let dir = server.vault.join("KizunaShelf/Lists");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Pinned.base");
    fs::write(
        &path,
        r#"
filters:
  and:
    - file.inFolder("Taxonomy/Anime")
views:
  - type: table
    name: Favorites
    filters: note.favorite == true
    sort:
      - property: file.name
        direction: desc
    limit: 1
    columnSize: {note.title: 200}
  - type: cards
    name: Everything
kizunashelf:
  custom: keep-me
"#,
    )
    .unwrap();
    fs::write(dir.join("Broken.base"), "views: [broken").unwrap();
    assert_eq!(server.ok_json("/api/home").await["lists"], json!([]));
    let detail = server.ok_json("/api/smart-lists/Pinned").await;
    let pinned = request_json(
        &server.app,
        Method::POST,
        "/api/smart-lists/Pinned/home",
        Some(json!({"revision": detail["revision"], "showOnHome": true})),
    )
    .await;
    assert_eq!(pinned.0, StatusCode::OK, "{}", pinned.1);
    assert_eq!(pinned.1["showOnHome"], true);
    assert_eq!(pinned.1["filters"], detail["filters"]);
    assert_eq!(pinned.1["views"], detail["views"]);
    let home = server.ok_json("/api/home").await;
    let results = server
        .ok_json("/api/smart-lists/Pinned/results?view=Favorites")
        .await;
    assert_eq!(home["lists"][0]["items"], results["items"]);
    assert_eq!(home["lists"][0]["total"], results["total"]);
    assert_eq!(home["lists"][0]["view"], "Favorites");
    let stale = request_json(
        &server.app,
        Method::POST,
        "/api/smart-lists/Pinned/home",
        Some(json!({"revision": detail["revision"], "showOnHome": false})),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT);
    let unpinned = request_json(
        &server.app,
        Method::POST,
        "/api/smart-lists/Pinned/home",
        Some(json!({"revision": pinned.1["revision"], "showOnHome": false})),
    )
    .await;
    assert_eq!(unpinned.0, StatusCode::OK);
    assert_eq!(server.ok_json("/api/home").await["lists"], json!([]));
    let doc: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(doc["kizunashelf"]["custom"].as_str(), Some("keep-me"));
    assert_eq!(
        doc["views"][0]["columnSize"]["note.title"].as_u64(),
        Some(200)
    );
}

#[tokio::test]
async fn suggested_lists_are_repeatable_and_keep_renames_and_edits() {
    let server = TestServer::new();
    let suggestions = server
        .ok_json("/api/smart-list-suggestions?language=en")
        .await;
    let suggestion = suggestions["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == "anime")
        .unwrap();
    let dir = server.vault.join("KizunaShelf/Lists");
    fs::create_dir_all(&dir).unwrap();
    let collision = dir.join(format!("{}.base", suggestion["name"].as_str().unwrap()));
    fs::write(&collision, "filters: note.custom == true\n").unwrap();
    let request = json!({"suggestionIds": [suggestion["id"]], "language": "en"});
    let created = request_json(
        &server.app,
        Method::POST,
        "/api/smart-list-suggestions",
        Some(request.clone()),
    )
    .await;
    assert_eq!(created.0, StatusCode::OK, "{}", created.1);
    assert_eq!(created.1["lists"].as_array().unwrap().len(), 1);
    let list = &created.1["lists"][0];
    assert!(list["name"].as_str().unwrap().ends_with(" 2"));
    assert_eq!(
        fs::read_to_string(&collision).unwrap(),
        "filters: note.custom == true\n"
    );
    assert!(
        list["views"]
            .as_array()
            .unwrap()
            .iter()
            .all(|view| view["limit"].is_null()),
        "suggested lists are not capped to Home's preview size"
    );
    let original = server.vault.join(list["path"].as_str().unwrap());
    let renamed = dir.join("My List.base");
    let raw = fs::read_to_string(&original)
        .unwrap()
        .replace("showOnHome: true", "showOnHome: false");
    fs::write(&renamed, format!("{raw}\ncustom: keep-me\n")).unwrap();
    fs::remove_file(original).unwrap();
    let regenerated = request_json(
        &server.app,
        Method::POST,
        "/api/smart-list-suggestions",
        Some(request.clone()),
    )
    .await;
    assert_eq!(regenerated.0, StatusCode::OK, "{}", regenerated.1);
    assert_eq!(regenerated.1["lists"][0]["name"], "My List");
    assert_eq!(regenerated.1["lists"][0]["showOnHome"], true);
    assert_eq!(regenerated.1["lists"][0]["filters"], list["filters"]);
    let saved = fs::read_to_string(&renamed).unwrap();
    assert!(saved.contains("custom: keep-me"));
    let repeated = request_json(
        &server.app,
        Method::POST,
        "/api/smart-list-suggestions",
        Some(request.clone()),
    )
    .await;
    assert_eq!(repeated.0, StatusCode::OK);
    assert_eq!(
        fs::read_to_string(&renamed).unwrap(),
        saved,
        "no-op recreation never rewrites edits"
    );
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    fs::remove_file(renamed).unwrap();
    let recreated = request_json(
        &server.app,
        Method::POST,
        "/api/smart-list-suggestions",
        Some(request),
    )
    .await;
    assert_eq!(recreated.0, StatusCode::OK);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
}

#[tokio::test]
async fn home_and_suggestions_work_with_no_lists_and_block_read_only_writes() {
    let server = TestServer::new();
    // The old home block is still in this test vault. It is ignored on load,
    // rather than making an otherwise valid vault inaccessible.
    assert_eq!(server.ok_json("/api/home").await["lists"], json!([]));
    let settings = server.ok_json("/api/settings/config").await;
    assert!(settings["vaultExists"].as_bool().unwrap());
    assert!(settings["error"].is_null());
    assert!(settings["vault"].get("home").is_none());
    let readonly = inline_router(&server.vault, true, false);
    for (url, body) in [
        ("/api/smart-list-suggestions", json!({"language":"en"})),
        (
            "/api/smart-lists/Any/home",
            json!({"revision":"old","showOnHome":true}),
        ),
    ] {
        let response = request_json(&readonly, Method::POST, url, Some(body)).await;
        assert_eq!(response.0, StatusCode::FORBIDDEN);
    }
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
async fn config_writes_reject_an_external_edit_after_loading() {
    let server = TestServer::new();
    let loaded = request_json(&server.app, Method::GET, "/api/settings/config/raw", None).await;
    let revision = loaded.1["revision"].as_str().unwrap();
    // The fixture retains an obsolete Home block to exercise old-vault loading.
    // Submit valid current YAML so this test isolates the stale revision guard.
    let mut submitted: serde_yaml::Mapping =
        serde_yaml::from_str(loaded.1["content"].as_str().unwrap()).unwrap();
    submitted.remove(serde_yaml::Value::String("home".into()));
    let submitted = serde_yaml::to_string(&submitted).unwrap();
    let externally_edited = format!(
        "{}\n# edited in Obsidian\n",
        loaded.1["content"].as_str().unwrap()
    );
    let config_path = server.vault.join("KizunaShelf/config.yaml");
    write_file(&config_path, &externally_edited);

    let stale = request_json(
        &server.app,
        Method::PUT,
        "/api/settings/config/raw",
        Some(json!({
            "content": submitted,
            "revision": revision,
        })),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT, "{}", stale.1);
    assert_eq!(fs::read_to_string(config_path).unwrap(), externally_edited);
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
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
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
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
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

    /// Evaluates an unsaved smart-list definition — how criteria-shaped browsing
    /// is answered (the library browser is an unsaved smart list). `rules` are
    /// AND-ed; `scope` is a type id or `None` for the whole library.
    async fn preview(&self, scope: Option<&str>, rules: Value) -> Value {
        let mut body = json!({ "filters": { "conjunction": "all", "rules": rules } });
        if let Some(scope) = scope {
            body["scope"] = json!(scope);
        }
        let (status, value) = request_json(
            &self.app,
            Method::POST,
            "/api/smart-lists/preview",
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "preview: {value}");
        value
    }

    /// The response status alone, for rejections whose body isn't JSON (e.g. a
    /// missing required query parameter, rejected by the extractor).
    async fn status(&self, path: &str) -> StatusCode {
        let response = self
            .app
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
        response.status()
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

// Titles, dates, relations, and provider hosts below are synthetic fixtures for
// search, filtering, and network isolation; they are not release metadata.
fn write_fixture_vault(vault: &Path) {
    write_file(
        &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
        r#"---
title: Steins;Gate 0
title_en: Amadeus of Zero
title_original: シュタインズ・ゲート ゼロ
status: Watching
favorite: true
season: "2025"
complete_date: 2025-04-20
cover_url: https://img.example/star.jpg
bgm_url: https://bgm.example/star
franchise: "[[Steins;Gate]]"
studio: "[[Nova Studio]]"
---
## Summary
Steins;Gate 0 (Anime) follows Rintaro across the beta worldline.

It shares continuity with [[Robotics;Notes]].
"#,
    );
    write_file(
        &vault.join("Taxonomy/Games/Robotics;Notes.md"),
        r#"---
title: Robotics;Notes
title_en: Robot Club Diary
status: Playing
favorite: false
genres: [Adventure, Strategy]
release_date: 2025-04-05
igdb_url: https://igdb.example/moon
franchise: "[[Steins;Gate]]"
developer: "[[Orbit Dev]]"
---
Robotics;Notes is a robotics-club adventure that references [[Steins;Gate 0 (Anime)]].
"#,
    );
    write_file(
        &vault.join("Taxonomy/Franchise/Steins;Gate.md"),
        r#"---
title: Steins;Gate
related: "[[Robotics;Notes]]"
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
Revisited [[Steins;Gate 0 (Anime)]] and [[Robotics;Notes|the quest]] after dinner.

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
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nBody\n"),
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
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
        "Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png"
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
async fn asset_download_does_not_overwrite_an_edit_during_network_fetch() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/slow.png");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nOriginal body\n"),
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let task_app = app.clone();
    let task_revision = revision.clone();
    let download =
        tokio::spawn(async move { download_assets(&task_app, id, &task_revision).await });

    // The mock response stays in flight long enough for an Obsidian-style
    // external edit to land after the request has begun but before its commit.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let entity_path = vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md");
    write_file(
        &entity_path,
        &format!(
            "---\ntitle: Steins;Gate 0\ncover_url: {cover}\nexternal_note: keep me\n---\nExternally edited body\n"
        ),
    );

    let (status, body) = download.await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    let raw = fs::read_to_string(entity_path).unwrap();
    assert!(raw.contains("external_note: keep me"), "{raw}");
    assert!(raw.contains("Externally edited body"), "{raw}");
    assert!(raw.contains(&cover), "{raw}");
}

#[tokio::test]
async fn asset_download_failure_keeps_remote_url() {
    let addr = start_mock_image_server().await;
    let missing = format!("http://{addr}/missing");
    let not_image = format!("http://{addr}/notimage");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {missing}\nshots:\n  - {not_image}\n---\nBody\n"),
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let results = body["results"].as_array().unwrap();
    assert!(results.iter().all(|item| item["status"] == "failed"));

    // Remote URLs are left untouched so the user can retry.
    assert_eq!(body["entity"]["frontmatter"]["cover_url"], missing);
    assert_eq!(body["entity"]["frontmatter"]["shots"][0], not_image);

    // No asset directory was created for this entity.
    assert!(!vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
}

#[tokio::test]
async fn asset_download_handles_image_list_partially() {
    let addr = start_mock_image_server().await;
    let ok = format!("http://{addr}/image.png");
    let bad = format!("http://{addr}/missing");
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\nshots:\n  - {ok}\n  - {bad}\n---\nBody\n"),
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let shots = body["entity"]["frontmatter"]["shots"].as_array().unwrap();
    // First element became a local hashed path; the failed one keeps its URL.
    assert!(shots[0]
        .as_str()
        .unwrap()
        .starts_with("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/shots/"));
    assert!(shots[0].as_str().unwrap().ends_with(".png"));
    assert_eq!(shots[1], bad);
}

#[tokio::test]
async fn plan_lists_remote_image_fields_only() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.jpg\nshots:\n  - https://img.example/a.png\n  - Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/shots/local.png\n---\nBody\n",
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
    assert_eq!(cover["entityId"], "anime:Steins;Gate 0 (Anime)");
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
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.jpg\n---\nBody\n",
        );
    });

    // The host (iOS) already downloaded the bytes to an app-temp file outside the vault.
    let source = temp.path().join("ingest-cover.png");
    fs::write(&source, PNG_1X1).unwrap();

    let id = "anime:Steins;Gate 0 (Anime)";
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
        "Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png"
    );
    assert!(vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png")
        .exists());
    // The host-temp source file is consumed (deleted) by the core.
    assert!(!source.exists());
}

#[tokio::test]
async fn ingest_checks_expected_revision_against_fresh_file_before_asset_writes() {
    let (_, temp, vault) = asset_test_app(true, |vault| {
        write_file(&vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.jpg\n---\nOriginal notes\n");
    });
    let app = build_inline_router(&vault, true, true, true, Duration::from_secs(3600));
    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let path = vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md");
    let changed = fs::read_to_string(&path)
        .unwrap()
        .replace("Original notes", "External edit to keep");
    fs::write(&path, &changed).unwrap();
    let source = temp.path().join("stale-cover.png");
    fs::write(&source, PNG_1X1).unwrap();
    let result = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}/assets/ingest", urlencoding::encode(id)),
        Some(
            json!({ "field": "cover_url", "sourceUrl": "https://img.example/cover.jpg",
            "sourcePath": source, "contentType": "image/png", "revision": revision }),
        ),
    )
    .await;
    assert_eq!(result.0, StatusCode::CONFLICT, "{}", result.1);
    assert_eq!(fs::read_to_string(&path).unwrap(), changed);
    assert!(
        !vault.join("Assets").exists(),
        "Rejected ingest must not stage vault assets"
    );
    assert!(!source.exists(), "The host staging file was consumed");
}

#[tokio::test]
async fn host_routers_with_one_identity_cannot_ingest_the_same_revision_twice() {
    let (_, temp, vault) = asset_test_app(true, |vault| {
        write_file(&vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.jpg\nshots: [https://img.example/a.png]\n---\nKeep notes\n");
    });
    let identity = format!("host-ingest:{}", temp.path().display());
    let front = build_inline_router_with_identity(
        &vault,
        true,
        true,
        true,
        Duration::from_secs(3600),
        Some(identity.clone()),
    );
    let back = build_inline_router_with_identity(
        &vault,
        true,
        true,
        true,
        Duration::from_secs(3600),
        Some(identity),
    );
    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&front, id).await;
    assert_eq!(entity_revision(&back, id).await, revision);
    let endpoint = format!("/api/entities/{}/assets/ingest", urlencoding::encode(id));
    let first = temp.path().join("front.png");
    let second = temp.path().join("back.png");
    fs::write(&first, PNG_1X1).unwrap();
    fs::write(&second, PNG_1X1).unwrap();
    let payload = |source: &Path| {
        json!({ "field": "cover_url", "sourceUrl": "https://img.example/cover.jpg",
        "sourcePath": source, "contentType": "image/png", "revision": revision })
    };
    let (a, b) = tokio::join!(
        request_json(&front, Method::POST, &endpoint, Some(payload(&first))),
        request_json(&back, Method::POST, &endpoint, Some(payload(&second)))
    );
    assert_eq!(
        [a.0, b.0].iter().filter(|s| **s == StatusCode::OK).count(),
        1,
        "{a:?} {b:?}"
    );
    assert_eq!(
        [a.0, b.0]
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1,
        "{a:?} {b:?}"
    );
    let successful = if a.0 == StatusCode::OK { a.1 } else { b.1 };
    let next_revision = successful["entity"]["revision"].as_str().unwrap();
    assert_ne!(next_revision, revision);
    // A later field in the same batch carries the revision returned by its own previous commit.
    let source = temp.path().join("next.png");
    fs::write(&source, PNG_1X1).unwrap();
    let result = request_json(
        &back,
        Method::POST,
        &endpoint,
        Some(json!({ "field": "shots",
        "listKey": "https://img.example/a.png", "sourceUrl": "https://img.example/a.png",
        "sourcePath": source, "revision": next_revision, "contentType": "image/png" })),
    )
    .await;
    assert_eq!(result.0, StatusCode::OK, "{}", result.1);
    assert_eq!(result.1["result"]["status"], "downloaded");
    assert_eq!(
        entity_revision(&front, id).await,
        result.1["entity"]["revision"].as_str().unwrap()
    );
    assert!(
        fs::read_to_string(vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"))
            .unwrap()
            .contains("Keep notes")
    );
}

#[tokio::test]
async fn ingest_preserves_an_existing_asset_referenced_by_the_same_entity() {
    let existing = "Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png";
    let old_bytes = [PNG_1X1, b"original"].concat();
    let (app, temp, vault) = asset_test_app(true, |vault| {
        write_file(&vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.png\nshots: [\"{existing}\"]\n---\nBody\n"));
        let path = vault.join(existing);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &old_bytes).unwrap();
    });
    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let source = temp.path().join("new.png");
    fs::write(&source, PNG_1X1).unwrap();
    let result = request_json(&app, Method::POST,
        &format!("/api/entities/{}/assets/ingest", urlencoding::encode(id)),
        Some(json!({ "field": "cover_url", "sourceUrl": "https://img.example/cover.png", "sourcePath": source, "revision": revision }))).await;
    assert_eq!(result.0, StatusCode::OK, "{}", result.1);
    assert_eq!(result.1["result"]["status"], "downloaded");
    assert_eq!(fs::read(vault.join(existing)).unwrap(), old_bytes);
    let saved = result.1["entity"]["frontmatter"]["cover_url"]
        .as_str()
        .unwrap();
    assert_ne!(saved, existing);
    assert_eq!(fs::read(vault.join(saved)).unwrap(), PNG_1X1);
    assert_eq!(
        result.1["entity"]["frontmatter"]["shots"],
        json!([existing])
    );
}

#[tokio::test]
async fn ingest_rewrites_one_list_element() {
    let (app, temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\nshots:\n  - https://img.example/a.png\n  - https://img.example/b.png\n---\nBody\n",
        );
    });

    let source = temp.path().join("shot-a.png");
    fs::write(&source, PNG_1X1).unwrap();

    let id = "anime:Steins;Gate 0 (Anime)";
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
        .starts_with("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/shots/"));
    assert!(shots[0].as_str().unwrap().ends_with(".png"));
    assert_eq!(shots[1], "https://img.example/b.png");
}

#[tokio::test]
async fn ingest_skips_when_source_url_no_longer_matches() {
    let (app, temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/new.jpg\n---\nBody\n",
        );
    });

    let source = temp.path().join("stale.png");
    fs::write(&source, PNG_1X1).unwrap();

    let id = "anime:Steins;Gate 0 (Anime)";
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
    assert!(!vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
    // Even on skip, the stale temp file is cleaned up.
    assert!(!source.exists());
}

#[tokio::test]
async fn upload_places_single_image_and_leaves_frontmatter_for_save() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.jpg\n---\nBody\n",
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
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
    assert!(path.starts_with("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url-"));
    assert!(path.ends_with(".png"));
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
async fn upload_preserves_saved_and_pending_assets_and_rejects_path_collisions() {
    let old_path = "Assets/Taxonomy/Anime/Show/cover_url.png";
    let original = format!("---\ntitle: Show\ncover_url: {old_path}\n---\nKeep notes\n");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(&vault.join("Taxonomy/Anime/Show.md"), &original);
        write_file(&vault.join(old_path), "saved cover bytes");
    });
    let payload =
        json!({"field": "cover_url", "dataBase64": b64(PNG_1X1), "contentType": "image/png"});
    let (status, first) = upload_asset(&app, "anime:Show", payload.clone()).await;
    assert_eq!(status, StatusCode::OK);
    let path = first["path"].as_str().unwrap();
    assert_ne!(path, old_path);
    assert_eq!(
        fs::read(vault.join(old_path)).unwrap(),
        b"saved cover bytes"
    );
    assert_eq!(
        fs::read_to_string(vault.join("Taxonomy/Anime/Show.md")).unwrap(),
        original
    );
    let (_, repeated) = upload_asset(&app, "anime:Show", payload.clone()).await;
    assert_eq!(first["path"], repeated["path"]);
    let mut second_bytes = PNG_1X1.to_vec();
    second_bytes.extend_from_slice(b"another image payload");
    let (status, second) = upload_asset(
        &app,
        "anime:Show",
        json!({"field": "cover_url", "dataBase64": b64(&second_bytes), "contentType": "image/png"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(first["path"], second["path"]);
    assert_eq!(fs::read(vault.join(path)).unwrap(), PNG_1X1);
    fs::write(vault.join(path), b"unrelated existing file").unwrap();
    let (status, _) = upload_asset(&app, "anime:Show", payload).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        fs::read(vault.join(path)).unwrap(),
        b"unrelated existing file"
    );
}

#[tokio::test]
async fn upload_places_image_list_element_by_content_hash() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\n---\nBody\n",
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
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
    assert!(path.starts_with("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/shots/"));
    assert!(path.ends_with(".png"));
    assert!(vault.join(path).exists());
}

#[tokio::test]
async fn upload_rejects_non_image_bytes() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\n---\nBody\n",
        );
    });

    let (status, _body) = upload_asset(
        &app,
        "anime:Steins;Gate 0 (Anime)",
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
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\n---\nBody\n",
        );
    });

    // `title` is not an image field.
    let (status, _body) = upload_asset(
        &app,
        "anime:Steins;Gate 0 (Anime)",
        json!({ "field": "title", "dataBase64": b64(PNG_1X1) }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Malformed base64 payload.
    let (status, _body) = upload_asset(
        &app,
        "anime:Steins;Gate 0 (Anime)",
        json!({ "field": "cover_url", "dataBase64": "not valid base64!!!" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_is_disabled_in_read_only_mode() {
    let (app, _temp, vault) = asset_test_app(false, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\n---\nBody\n",
        );
    });

    let (status, body) = upload_asset(
        &app,
        "anime:Steins;Gate 0 (Anime)",
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
    assert!(!vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
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
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
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
    // "Old Show" references a local cover that sits inside "Steins;Gate 0 (Anime)"'s asset
    // directory (as if filenames were swapped outside the app).
    let collide = "Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png";
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nBody\n"),
        );
        write_file(
            &vault.join("Taxonomy/Anime/Old Show.md"),
            &format!("---\ntitle: Old Show\ncover_url: {collide}\n---\nBody\n"),
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let new_path = body["entity"]["frontmatter"]["cover_url"].as_str().unwrap();
    assert_ne!(new_path, collide, "must not reuse another entity's file");
    assert!(new_path.starts_with("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url-"));
    assert_eq!(body["results"][0]["conflictResolved"], true);

    // The other entity's reference is untouched.
    let other_revision = entity_revision(&app, "anime:Old Show").await;
    assert!(!other_revision.is_empty());
}

#[tokio::test]
async fn asset_serve_route_rejects_path_escape() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\n---\nBody\n",
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
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nBody\n"),
        );
    });

    let capabilities = request_json(&app, Method::GET, "/api/capabilities", None).await;
    assert_eq!(capabilities.1["assetDownloadEnabled"], false);

    let (status, body) = download_assets(&app, "anime:Steins;Gate 0 (Anime)", "any-revision").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "Content writes are disabled");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn asset_batch_job_downloads_remote_covers() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/image.png");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nBody\n"),
        );
        write_file(
            &vault.join("Taxonomy/Anime/Robotics;Notes.md"),
            &format!("---\ntitle: Robotics;Notes\ncover_url: {cover}\n---\nBody\n"),
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
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png")
        .exists());
    assert!(vault
        .join("Assets/Taxonomy/Anime/Robotics;Notes/cover_url.png")
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
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nBody\n"),
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
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png\n---\nBody\n",
        );
        write_file(
            &vault.join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png"),
            "fake-bytes",
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let updated = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}", urlencoding::encode(id)),
        Some(json!({ "revision": revision, "renameTo": "Steins;Gate 0 (Anime) 2" })),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);

    assert!(!vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
    assert!(vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime) 2/cover_url.png")
        .exists());
    assert_eq!(
        updated.1["entity"]["frontmatter"]["cover_url"],
        "Assets/Taxonomy/Anime/Steins;Gate 0 (Anime) 2/cover_url.png"
    );
}

#[tokio::test]
async fn delete_trashes_asset_directory() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png\n---\nBody\n",
        );
        write_file(
            &vault.join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png"),
            "fake-bytes",
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
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
    assert!(!vault
        .join("Taxonomy/Anime/Steins;Gate 0 (Anime).md")
        .exists());
    assert!(vault.join(".trash/Steins;Gate 0 (Anime).md").exists());
    assert!(!vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
    assert!(vault
        .join(".trash/Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
}

#[tokio::test]
async fn delete_rechecks_revision_against_fresh_file() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    let entity_path = vault.join("Taxonomy/Anime/Show.md");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "filename": { "titleLanguage": "en" },
                "fields": [{
                    "field": "title",
                    "fieldType": "title",
                    "titleLanguage": "en"
                }]
            }]
        }),
    );
    write_file(&entity_path, "---\ntitle: Show\n---\nOriginal\n");
    let app = cached_inline_router(&vault, true, true);

    // Prime the long-lived resident snapshot, then change the file without
    // notifying the core, as Files/iCloud/an external editor would.
    let revision = entity_revision(&app, "anime:Show").await;
    let externally_edited = "---\ntitle: Show\n---\nEdited elsewhere\n";
    write_file(&entity_path, externally_edited);

    let deleted = request_json(
        &app,
        Method::DELETE,
        "/api/entities/anime%3AShow",
        Some(json!({ "revision": revision })),
    )
    .await;
    assert_eq!(deleted.0, StatusCode::CONFLICT, "{}", deleted.1);
    assert_eq!(fs::read_to_string(&entity_path).unwrap(), externally_edited);
    assert!(!vault.join(".trash/Show.md").exists());
}

#[tokio::test]
async fn broken_asset_cleanup_queue_flags_missing_files() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        // References a local cover that does not exist on disk.
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png\n---\nBody\n",
        );
    });

    let cleanup = request_json(&app, Method::GET, "/api/cleanup-queues", None).await;
    assert_eq!(cleanup.0, StatusCode::OK);
    let broken = cleanup.1["brokenAssets"].as_array().unwrap();
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0]["title"], "Steins;Gate 0");
    assert!(cleanup.1["queues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|queue| queue["id"] == "broken-asset"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_list_item_adds_are_not_lost() {
    // Each add is a read-modify-write of the same list file. Without the content
    // mutation lock, concurrent adds read the same bytes and the last write wins,
    // silently dropping the other entities.
    let server = TestServer::new();
    let app = &server.app;
    let created = request_json(
        app,
        Method::POST,
        "/api/lists",
        Some(json!({ "name": "Race" })),
    )
    .await;
    assert_eq!(created.0, StatusCode::OK, "{}", created.1);

    let entities = server.ok_json("/api/entities").await;
    let ids: Vec<String> = entities["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.len() >= 3, "fixture needs several entities: {ids:?}");

    // Separate tasks on a multi-threaded runtime, so the handlers really overlap.
    let adds: Vec<_> = ids
        .iter()
        .map(|id| {
            let app = app.clone();
            let payload = json!({ "entityId": id });
            tokio::spawn(async move {
                request_json(&app, Method::POST, "/api/lists/Race/items", Some(payload)).await
            })
        })
        .collect();
    for add in adds {
        let (status, body) = add.await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    let detail = server.ok_json("/api/lists/Race").await;
    let listed: HashSet<&str> = detail["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|section| section["items"].as_array().unwrap())
        .filter_map(|item| item["entity"]["id"].as_str())
        .collect();
    let expected: HashSet<&str> = ids.iter().map(String::as_str).collect();
    assert_eq!(listed, expected);
}

#[tokio::test]
async fn static_and_smart_lists_share_one_name_space() {
    // Item edits refuse any id that also names a smart list, so a static list
    // sharing a smart list's name could be created but never edited. Creating
    // or renaming into a name either kind holds is a conflict instead.
    let server = TestServer::new();
    let app = &server.app;
    let smart = request_json(
        app,
        Method::POST,
        "/api/smart-lists",
        Some(json!({ "name": "Shared", "scope": "anime" })),
    )
    .await;
    assert_eq!(smart.0, StatusCode::OK, "{}", smart.1);

    let clash = request_json(
        app,
        Method::POST,
        "/api/lists",
        Some(json!({ "name": "Shared" })),
    )
    .await;
    assert_eq!(clash.0, StatusCode::CONFLICT, "{}", clash.1);
    assert_eq!(clash.1["error"], "List already exists");

    let other = request_json(
        app,
        Method::POST,
        "/api/lists",
        Some(json!({ "name": "Other" })),
    )
    .await;
    assert_eq!(other.0, StatusCode::OK, "{}", other.1);
    let rename = request_json(
        app,
        Method::POST,
        "/api/lists/Other",
        Some(json!({
            "revision": other.1["revision"],
            "renameTo": "Shared",
            "description": "",
            "trailing": "",
            "sections": [],
        })),
    )
    .await;
    assert_eq!(rename.0, StatusCode::CONFLICT, "{}", rename.1);
    assert_eq!(rename.1["error"], "Target list already exists");
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
        Some(json!({ "entityId": "anime:Steins;Gate 0 (Anime)" })),
    )
    .await;
    assert_eq!(added.0, StatusCode::OK, "{}", added.1);
    // New items land in the ungrouped block (the heading-less first section).
    assert_eq!(added.1["sections"].as_array().unwrap().len(), 1);
    assert!(added.1["sections"][0]["heading"].is_null());
    assert_eq!(added.1["sections"][0]["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        added.1["sections"][0]["items"][0]["text"],
        "[[Steins;Gate 0 (Anime)]]"
    );
    assert_eq!(
        added.1["sections"][0]["items"][0]["entity"]["id"],
        "anime:Steins;Gate 0 (Anime)"
    );

    // Adding the same entity again is idempotent.
    let again = request_json(
        app,
        Method::POST,
        "/api/lists/Watchlist/items",
        Some(json!({ "entityId": "anime:Steins;Gate 0 (Anime)" })),
    )
    .await;
    assert_eq!(again.0, StatusCode::OK, "{}", again.1);
    assert_eq!(again.1["sections"][0]["items"].as_array().unwrap().len(), 1);

    // Add a second entity.
    let added2 = request_json(
        app,
        Method::POST,
        "/api/lists/Watchlist/items",
        Some(json!({ "entityId": "games:Robotics;Notes" })),
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
                "items": [{ "text": "[[Robotics;Notes]]", "checked": true }]
            },
            {
                "heading": "Finished",
                "marker": "ordered",
                "items": [{ "text": "[[Steins;Gate 0 (Anime)]]" }]
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
        "games:Robotics;Notes"
    );
    assert_eq!(updated.1["sections"][1]["heading"], "Finished");
    assert_eq!(updated.1["sections"][1]["marker"], "ordered");
    // A non-task item carries no `checked` field.
    assert!(updated.1["sections"][1]["items"][0]
        .get("checked")
        .is_none());
    assert_eq!(
        updated.1["sections"][1]["items"][0]["entity"]["id"],
        "anime:Steins;Gate 0 (Anime)"
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
        "/api/lists?entity=anime%3ASteins;Gate%200%20(Anime)",
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
        "/api/lists/Watchlist/items/anime%3ASteins;Gate%200%20(Anime)",
        None,
    )
    .await;
    assert_eq!(removed.0, StatusCode::OK, "{}", removed.1);
    // It drops out of its section; the now-empty "Finished" heading is preserved.
    assert_eq!(
        removed.1["sections"][0]["items"][0]["entity"]["id"],
        "games:Robotics;Notes"
    );
    assert_eq!(removed.1["sections"][1]["heading"], "Finished");
    assert_eq!(
        removed.1["sections"][1]["items"].as_array().unwrap().len(),
        0
    );
    let member_after = request_json(
        app,
        Method::GET,
        "/api/lists?entity=anime%3ASteins;Gate%200%20(Anime)",
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
    assert_eq!(results.1["items"][0]["id"], "anime:Steins;Gate 0 (Anime)");

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

    // The criteria evaluate: Steins;Gate 0 (Anime) is Watching + favorite.
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
        "/api/lists?entity=anime%3ASteins;Gate%200%20(Anime)",
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
async fn smart_list_summary_cache_tracks_library_and_definition_revisions() {
    let server = TestServer::new();
    let list_path = server.vault.join("KizunaShelf/Lists/All Anime.base");
    write_file(
        &list_path,
        "filters:\n  and:\n    - file.inFolder(\"Taxonomy/Anime\")\nviews:\n  - type: table\n    name: List\n",
    );

    // The first request builds the memo; the second takes its unchanged-key hit.
    let first = request_json(
        &server.app,
        Method::GET,
        "/api/lists?today=2030-01-01",
        None,
    )
    .await;
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    assert_eq!(first.1["items"][0]["itemCount"], 1);
    let warm = request_json(
        &server.app,
        Method::GET,
        "/api/lists?today=2030-01-01&entity=anime%3ASteins;Gate%200%20(Anime)",
        None,
    )
    .await;
    assert_eq!(warm.0, StatusCode::OK, "{}", warm.1);
    assert_eq!(warm.1["items"][0]["itemCount"], 1);
    assert_eq!(warm.1["items"][0]["contains"], true);

    // An entity mutation changes Library::content_revision, so the cached count
    // cannot survive even though the smart-list definition is unchanged.
    let created = request_json(
        &server.app,
        Method::POST,
        "/api/entities",
        Some(json!({
            "type": "anime",
            "basename": "Cache Miss",
            "frontmatter": {
                "title": "Cache Miss",
                "status": "Watching"
            },
            "body": ""
        })),
    )
    .await;
    assert_eq!(created.0, StatusCode::OK, "{}", created.1);
    let after_entity = request_json(
        &server.app,
        Method::GET,
        "/api/lists?today=2030-01-01",
        None,
    )
    .await;
    assert_eq!(after_entity.0, StatusCode::OK, "{}", after_entity.1);
    assert_eq!(after_entity.1["items"][0]["itemCount"], 2);

    // Smart-list files deliberately do not participate in the library revision.
    // Their independent listing fingerprint must therefore catch external edits.
    write_file(
        &list_path,
        "filters:\n  and:\n    - file.inFolder(\"Taxonomy/Anime\")\n    - note.status == \"Never\"\nviews:\n  - type: table\n    name: List\n",
    );
    let after_definition = request_json(
        &server.app,
        Method::GET,
        "/api/lists?today=2030-01-01",
        None,
    )
    .await;
    assert_eq!(after_definition.0, StatusCode::OK, "{}", after_definition.1);
    assert_eq!(after_definition.1["items"][0]["itemCount"], 0);
}

#[tokio::test]
async fn smart_list_membership_cannot_be_edited_by_hand() {
    // Smart-list membership is derived from filters, so the manual add/remove
    // item endpoints must reject a smart-list id rather than 404 or (worse)
    // mutate a same-named static list.
    let server = TestServer::new();
    let app = &server.app;

    let created = request_json(
        app,
        Method::POST,
        "/api/smart-lists",
        Some(json!({ "name": "Watching Now", "scope": "anime" })),
    )
    .await;
    assert_eq!(created.0, StatusCode::OK, "{}", created.1);

    let added = request_json(
        app,
        Method::POST,
        "/api/lists/Watching%20Now/items",
        Some(json!({ "entityId": "anime:Steins;Gate 0 (Anime)" })),
    )
    .await;
    assert_eq!(added.0, StatusCode::BAD_REQUEST, "{}", added.1);

    let removed = request_json(
        app,
        Method::DELETE,
        "/api/lists/Watching%20Now/items/anime%3ASteins;Gate%200%20(Anime)",
        None,
    )
    .await;
    assert_eq!(removed.0, StatusCode::BAD_REQUEST, "{}", removed.1);
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
    // Tags are opt-in: `tags.field` enables the feature. A schema field sharing
    // the configured name (here a relation) must be IGNORED — the built-in tags
    // feature owns the name, so no relations are built from it.
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "tags": { "field": "tags" },
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

    // Tags narrow a browse through the smart-list `hasTag` rule — the built-in
    // tags field is matched against the normalized tag list, not frontmatter.
    let titles = async |rules: Value| -> Vec<String> {
        let (status, value) = request_json(
            &app,
            Method::POST,
            "/api/smart-lists/preview",
            Some(json!({
                "scope": "anime",
                "filters": { "conjunction": "all", "rules": rules },
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{value}");
        let mut titles: Vec<String> = value["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entity| entity["title"].as_str().unwrap().to_string())
            .collect();
        titles.sort();
        titles
    };

    assert_eq!(
        titles(json!([{ "kind": "hasTag", "values": ["action"] }])).await,
        ["Alpha"]
    );
    // ANY/OR semantics within one rule: action OR drama → Alpha + Beta.
    assert_eq!(
        titles(json!([{ "kind": "hasTag", "values": ["action", "drama"] }])).await,
        ["Alpha", "Beta"]
    );
}

#[tokio::test]
async fn tags_are_disabled_without_a_configured_tags_field() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    // No `tags` block: the opt-in feature is off. A schema field named `tags`
    // is then an ordinary schema field — here a relation that must be honored.
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
        "---\ntitle: Alpha\ntags: [Beta]\n---\nBody\n",
    );
    write_file(
        &vault.join("Taxonomy/Anime/Beta.md"),
        "---\ntitle: Beta\n---\nBody\n",
    );
    let app = inline_router(&vault, true, true);

    // The config carries no tags field — the signal for clients to hide tag UI.
    let config = request_json(&app, Method::GET, "/api/config", None).await;
    assert_eq!(config.0, StatusCode::OK, "{}", config.1);
    assert_eq!(config.1["tagsField"], serde_json::Value::Null);

    // No tags are derived, even from a frontmatter key literally named `tags`...
    let entities = request_json(&app, Method::GET, "/api/entities?type=anime", None).await;
    assert_eq!(entities.0, StatusCode::OK, "{}", entities.1);
    let items = entities.1["items"].as_array().unwrap();
    let alpha = items.iter().find(|e| e["title"] == "Alpha").unwrap();
    assert_eq!(alpha["tags"], json!([]));
    // ...and the schema relation field named `tags` is a real relation source.
    assert_eq!(alpha["relationCount"], 1);

    // The vocabulary is empty.
    let tags = request_json(&app, Method::GET, "/api/tags", None).await;
    assert_eq!(tags.0, StatusCode::OK, "{}", tags.1);
    assert_eq!(tags.1["tags"], json!([]));
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

    // The vocabulary + the `hasTag` rule use the configured field too.
    let tags = request_json(&app, Method::GET, "/api/tags", None).await;
    assert_eq!(tags.1["tags"], json!(["action", "rpg"]));
    let filtered = request_json(
        &app,
        Method::POST,
        "/api/smart-lists/preview",
        Some(json!({
            "scope": "anime",
            "filters": {
                "conjunction": "all",
                "rules": [{ "kind": "hasTag", "values": ["rpg"] }],
            },
        })),
    )
    .await;
    assert_eq!(filtered.0, StatusCode::OK, "{}", filtered.1);
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
async fn body_tasks_toggle_stamps_done_date_and_skips_the_episodes_section() {
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
    // Notes on both sides of the episodes section, and a blank line after the
    // frontmatter: the read path trims that leading whitespace off `entity.body`
    // while the write path keeps it, so a line the client counted only resolves
    // if the two coordinate spaces are reconciled.
    write_file(
        &vault.join("Taxonomy/Anime/Show.md"),
        "---\ntitle: Show\n---\n\n## Notes\n\n- [ ] rewatch with subs\n\n## Episodes\n\n- [ ] 1 · Pilot\n\n## More\n\n- [ ] read the manga\n",
    );
    let app = inline_router(&vault, true, true);

    let detail = request_json(&app, Method::GET, "/api/entities/anime%3AShow", None).await;
    assert_eq!(detail.0, StatusCode::OK, "{}", detail.1);
    // The notes view drops the episodes section and closes the gap, so the two
    // checkboxes the client renders sit on its lines 3 and 7 — the coordinate
    // space the toggle addresses.
    let notes = detail.1["notesBody"].as_str().unwrap().to_string();
    assert_eq!(
        notes,
        "## Notes\n\n- [ ] rewatch with subs\n\n## More\n\n- [ ] read the manga"
    );
    let revision = detail.1["entity"]["revision"].as_str().unwrap().to_string();

    let body = json!({
        "revision": revision, "line": 7, "text": "- [ ] read the manga",
        "done": true, "date": "2024-08-20",
    });
    let updated = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/tasks/toggle",
        Some(body.clone()),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);
    assert_eq!(
        updated.1["notesBody"],
        "## Notes\n\n- [ ] rewatch with subs\n\n## More\n\n- [x] read the manga ✅ 2024-08-20"
    );
    // Only that line changed: the episodes checkbox between the two notes
    // sections is untouched, and so is the blank line after the frontmatter.
    assert_eq!(updated.1["episodes"]["watched"], 0);
    assert_eq!(updated.1["episodes"]["total"], 1);
    let raw = std::fs::read_to_string(vault.join("Taxonomy/Anime/Show.md")).unwrap();
    assert_eq!(
        raw,
        "---\ntitle: Show\n---\n\n## Notes\n\n- [ ] rewatch with subs\n\n## Episodes\n\n- [ ] 1 · Pilot\n\n## More\n\n- [x] read the manga ✅ 2024-08-20\n"
    );

    // The stale revision is now rejected.
    let stale = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/tasks/toggle",
        Some(body),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT, "{}", stale.1);

    // A locator whose text no longer matches is refused rather than toggling a
    // neighbour.
    let revision = updated.1["entity"]["revision"]
        .as_str()
        .unwrap()
        .to_string();
    let drifted = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/tasks/toggle",
        Some(json!({
            "revision": revision, "line": 3, "text": "- [ ] read the manga",
            "done": true, "date": "2024-08-20",
        })),
    )
    .await;
    assert_eq!(drifted.0, StatusCode::NOT_FOUND, "{}", drifted.1);
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
    // Enrichment was off, so the episodes-phase counters never appear.
    assert!(done["episodesTotal"].is_null(), "{done}");

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
async fn import_commit_reports_episode_enrichment_progress() {
    let (app, _vault, _temp) = build_import_server(true);
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
        Some(json!({
            "options": { "importUserData": true, "importEpisodes": true, "markProgress": true }
        })),
    )
    .await;
    let done = await_import_status(&app, &id, "completed").await;
    assert_eq!(done["created"], 1, "{done}");
    // The enrichment phase reports its own progress — one step per created
    // entity (here a no-op step: the type has no episodes section), all counted
    // by the time the job completes.
    assert_eq!(done["episodesTotal"], 1, "{done}");
    assert_eq!(done["episodesProcessed"], 1, "{done}");
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

#[tokio::test]
async fn import_cancel_planned_job_is_terminal_and_cannot_be_committed() {
    let (app, vault, _temp) = build_import_server(true);
    let (status, job) = request_json(
        &app,
        Method::POST,
        "/api/import-jobs",
        Some(json!({ "source": "yamtrack", "input": { "csvText": YAMTRACK_CSV } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = job["id"].as_str().unwrap();
    let planned = await_import_status(&app, id, "planned").await;
    let (status, cancelled) = request_json(
        &app,
        Method::POST,
        &format!("/api/import-jobs/{id}/cancel"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cancelled["status"], "cancelled");
    assert!(cancelled["finishedAt"].is_string());
    assert_eq!(cancelled["plan"], planned["plan"]);
    let (_, again) = request_json(
        &app,
        Method::POST,
        &format!("/api/import-jobs/{id}/cancel"),
        None,
    )
    .await;
    assert_eq!(again, cancelled);
    let (status, _) = request_json(
        &app, Method::POST, &format!("/api/import-jobs/{id}/commit"),
        Some(json!({ "options": { "importUserData": true, "importEpisodes": false, "markProgress": false } })),
    ).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        fs::read_dir(vault.join("Taxonomy/Anime")).unwrap().count(),
        0
    );
    let (status, _) = request_json(
        &app,
        Method::POST,
        "/api/import-jobs",
        Some(json!({ "source": "yamtrack", "input": { "csvText": YAMTRACK_CSV } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn config_creation_defaults_follow_status_roles_without_writing() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [
                { "id": "books", "label": "Books", "path": "Books", "fields": [
                    { "field": "lifecycle", "fieldType": "enum", "enumRole": "status",
                      "enumOptions": ["Reading", "Rereading", "Later"],
                      "statusValues": {"ongoing": ["Reading", "Rereading"], "planning": ["Later"]} },
                    { "field": "status", "fieldType": "text" }
                ]},
                { "id": "notes", "label": "Notes", "path": "Notes", "fields": [
                    { "field": "status", "fieldType": "text" }
                ]}
            ]
        }),
    );
    let app = inline_router(&vault, true, true);
    let (status, config) = request_json(&app, Method::GET, "/api/config", None).await;
    assert_eq!(status, StatusCode::OK, "{config}");
    assert_eq!(
        config["creationDefaults"],
        json!([
            {"type":"books", "canonicalStatus":"planning", "frontmatter":{"lifecycle":"Later"}},
            {"type":"books", "canonicalStatus":"ongoing", "frontmatter":{"lifecycle":"Reading"}}
        ])
    );
    let (_, entities) = request_json(&app, Method::GET, "/api/entities", None).await;
    assert_eq!(entities["total"], 0);
}

#[tokio::test]
async fn import_review_snapshot_survives_host_restart_but_not_commit_or_schema_change() {
    let (_, vault, _temp) = build_import_server(true);
    let host = |identity: &str| {
        build_inline_router_with_identity(
            &vault,
            true,
            true,
            true,
            Duration::from_secs(3600),
            Some(identity.to_owned()),
        )
    };
    let first = host("durable-import-vault");
    let (_, job) = request_json(
        &first,
        Method::POST,
        "/api/import-jobs",
        Some(json!({"source":"yamtrack", "input":{"csvText": YAMTRACK_CSV}})),
    )
    .await;
    let id = job["id"].as_str().unwrap();
    let planned = await_import_status(&first, id, "planned").await;
    let (status, snapshot) = request_json(
        &first,
        Method::GET,
        &format!("/api/import-jobs/{id}/snapshot"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snapshot}");
    drop(first);
    let second = host("durable-import-vault");
    let (status, restored) = request_json(
        &second,
        Method::POST,
        "/api/import-jobs/restore-plan",
        Some(snapshot.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored, planned);
    let (status, repeated) = request_json(
        &second,
        Method::POST,
        "/api/import-jobs/restore-plan",
        Some(snapshot.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated, planned);
    // A new job in a fresh engine must not overwrite the restored job's ID.
    let (_, newer) = request_json(
        &second,
        Method::POST,
        "/api/import-jobs",
        Some(json!({"source":"yamtrack", "input":{"csvText": YAMTRACK_CSV}})),
    )
    .await;
    assert_ne!(newer["id"], planned["id"]);
    await_import_status(&second, newer["id"].as_str().unwrap(), "planned").await;
    let (status, _) = request_json(
        &host("different-vault"),
        Method::POST,
        "/api/import-jobs/restore-plan",
        Some(snapshot.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = request_json(
        &inline_router(&vault, true, true),
        Method::POST,
        "/api/import-jobs/restore-plan",
        Some(snapshot.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = request_json(
        &second,
        Method::POST,
        "/api/import-jobs/restore-plan",
        Some(json!({"snapshot":"broken"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = request_json(
        &second,
        Method::POST,
        &format!("/api/import-jobs/{id}/commit"),
        Some(
            json!({"options":{"importUserData":true,"importEpisodes":false,"markProgress":false}}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let done = await_import_status(&second, id, "completed").await;
    assert_eq!(done["created"], 1);
    let (status, _) = request_json(
        &second,
        Method::POST,
        "/api/import-jobs/restore-plan",
        Some(snapshot.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = request_json(
        &second,
        Method::GET,
        &format!("/api/import-jobs/{id}/snapshot"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    // External config edits behind a long-lived cache cannot restore a stale plan.
    let config_path = vault.join("KizunaShelf/config.yaml");
    let raw = fs::read_to_string(&config_path).unwrap();
    fs::write(&config_path, raw.replace("Anime", "Changed")).unwrap();
    let (status, _) = request_json(
        &host("durable-import-vault"),
        Method::POST,
        "/api/import-jobs/restore-plan",
        Some(snapshot),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

fn edit_snapshot(entity: &Value) -> Value {
    json!({"basename": entity["basename"], "body": entity["body"], "frontmatter": entity["frontmatter"]})
}

#[tokio::test]
async fn edit_review_merges_only_local_changes_and_reports_overlap_without_writing() {
    let server = TestServer::new();
    let (status, created) = request_json(&server.app, Method::POST, "/api/entities", Some(json!({
        "type":"anime", "basename":"Review item", "body":"Base notes", "frontmatter": {
            "status":"Backlog", "franchise":["[[Base]]"], "opaque":{"x":false}, "cleared":"old", "number":2020
        }
    }))).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let base = &created["entity"];
    let path = format!(
        "/api/entities/{}",
        urlencoding::encode(base["id"].as_str().unwrap())
    );
    let (status, changed) = request_json(&server.app, Method::POST, &path, Some(json!({
        "revision":base["revision"], "frontmatter":{"status":"Watching","opaque":{"x":true},"newExtra":{"keep":[null,2]}},"body":"Remote notes"
    }))).await;
    assert_eq!(status, StatusCode::OK);
    let note = server
        .vault
        .join(changed["entity"]["path"].as_str().unwrap());
    let bytes = fs::read(&note).unwrap();
    let mut draft = edit_snapshot(base);
    draft["body"] = json!("Local notes");
    draft["frontmatter"]["status"] = json!(" Completed ");
    draft["frontmatter"]["franchise"] = json!([" New target "]);
    draft["frontmatter"]["number"] = json!("2020");
    draft["frontmatter"]
        .as_object_mut()
        .unwrap()
        .remove("cleared");
    let (status, review) = request_json(
        &server.app,
        Method::POST,
        &format!("{path}/edit/review"),
        Some(json!({"type":"anime","baseline":edit_snapshot(base),"draft":draft})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert_eq!(review["entity"]["revision"], changed["entity"]["revision"]);
    assert_eq!(review["conflictFields"], json!(["status"]));
    assert_eq!(review["bodyConflict"], true);
    assert_eq!(review["nameConflict"], false);
    assert_eq!(review["merged"]["body"], "Remote notes");
    assert_eq!(review["merged"]["frontmatter"]["status"], "Watching");
    assert_eq!(review["local"]["frontmatter"]["status"], "Completed");
    assert_eq!(
        review["merged"]["frontmatter"]["franchise"],
        json!(["[[New target]]"])
    );
    assert_eq!(review["merged"]["frontmatter"]["opaque"], json!({"x":true}));
    assert_eq!(
        review["merged"]["frontmatter"]["newExtra"],
        json!({"keep":[null,2]})
    );
    assert_eq!(review["merged"]["frontmatter"]["number"], 2020);
    assert!(review["merged"]["frontmatter"].get("cleared").is_none());
    assert_eq!(bytes, fs::read(&note).unwrap());
    // Explicitly choose the local status/body; save still goes through the guarded mutation.
    let mut fields = review["merged"]["frontmatter"].clone();
    fields["status"] = review["local"]["frontmatter"]["status"].clone();
    let (status, saved) = request_json(
        &server.app,
        Method::POST,
        &path,
        Some(json!({
            "revision":review["entity"]["revision"], "schemaRevision":review["schemaRevision"],
            "frontmatterDraft":fields, "body":review["local"]["body"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(
        saved["entity"]["frontmatter"]["newExtra"],
        json!({"keep":[null,2]})
    );
    assert_eq!(saved["entity"]["body"], "Local notes");
    let saved_bytes = fs::read(&note).unwrap();
    let (status, _) = request_json(&server.app, Method::POST, &path, Some(json!({
        "revision":review["entity"]["revision"],"schemaRevision":review["schemaRevision"],"body":"Stale review"
    }))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(saved_bytes, fs::read(&note).unwrap());
}

#[tokio::test]
async fn edit_review_is_fresh_read_only_and_schema_guard_rejects_changed_interpretation() {
    let server = TestServer::new();
    let path = format!(
        "/api/entities/{}",
        urlencoding::encode("anime:Steins;Gate 0 (Anime)")
    );
    let base = server.ok_json(&path).await["entity"].clone();
    let read_only = cached_inline_router(&server.vault, false, false);
    let _ = request_json(&read_only, Method::GET, &path, None).await;
    let file = server.vault.join(base["path"].as_str().unwrap());
    fs::write(&file, "---\ntitle: External title\n---\nExternal notes\n").unwrap();
    let bytes = fs::read(&file).unwrap();
    let request =
        json!({"type":"anime","baseline":edit_snapshot(&base),"draft":edit_snapshot(&base)});
    let (status, review) = request_json(
        &read_only,
        Method::POST,
        &format!("{path}/edit/review"),
        Some(request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert_eq!(review["merged"]["frontmatter"]["title"], "External title");
    assert_eq!(review["merged"]["body"], "External notes");
    assert_eq!(review["conflictFields"], json!([]));
    assert_eq!(bytes, fs::read(&file).unwrap());
    let payload = json!({"revision":review["entity"]["revision"],"schemaRevision":review["schemaRevision"],"frontmatterDraft":review["merged"]["frontmatter"],"body":"Reviewed"});
    assert_eq!(
        request_json(&read_only, Method::POST, &path, Some(payload.clone()))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let config_file = server.vault.join("KizunaShelf/config.yaml");
    let mut config: Value =
        serde_yaml::from_str(&fs::read_to_string(&config_file).unwrap()).unwrap();
    config["types"][0]["fields"][0]["displayName"] = json!("Changed field label");
    write_vault_config(&server.vault, &config);
    let (status, error) = request_json(&server.app, Method::POST, &path, Some(payload)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{error}");
    assert_eq!(bytes, fs::read(&file).unwrap());
    assert_eq!(
        request_json(
            &server.app,
            Method::POST,
            "/api/entities/missing/edit/review",
            Some(request.clone())
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let mut wrong = request;
    wrong["type"] = json!("game");
    assert_eq!(
        request_json(
            &server.app,
            Method::POST,
            &format!("{path}/edit/review"),
            Some(wrong)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn edit_review_preserves_atomic_unknown_values_and_distinguishes_concurrent_deletion() {
    let server = TestServer::new();
    let (_, created) = request_json(
        &server.app,
        Method::POST,
        "/api/entities",
        Some(json!({
            "type":"anime","basename":"Opaque review","body":"Base", "frontmatter":{
                "object":{"nested":[false,1]},"list":["a",2],"delete":"base","same":"base"
            }
        })),
    )
    .await;
    let base = &created["entity"];
    let path = format!(
        "/api/entities/{}",
        urlencoding::encode(base["id"].as_str().unwrap())
    );
    let (_,latest)=request_json(&server.app,Method::POST,&path,Some(json!({
        "revision":base["revision"],"frontmatter":{"object":{"nested":[true,2]},"list":null,"delete":"remote","same":"agreed"}
    }))).await;
    let mut draft = edit_snapshot(base);
    draft["frontmatter"] = json!({"object":{"nested":[false,9]},"list":["local"],"same":"agreed"});
    draft["body"] = json!("Local only");
    draft["basename"] = json!("New name");
    let bytes = fs::read(
        server
            .vault
            .join(latest["entity"]["path"].as_str().unwrap()),
    )
    .unwrap();
    let (status, review) = request_json(
        &server.app,
        Method::POST,
        &format!("{path}/edit/review"),
        Some(json!({"type":"anime","baseline":edit_snapshot(base),"draft":draft})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert_eq!(
        review["conflictFields"],
        json!(["delete", "list", "object"])
    );
    assert_eq!(review["local"]["frontmatter"].get("delete"), None);
    assert_eq!(review["merged"]["frontmatter"].get("list"), None);
    assert_eq!(review["merged"]["frontmatter"]["same"], "agreed");
    assert_eq!(
        review["merged"]["frontmatter"]["object"],
        json!({"nested":[true,2]})
    );
    assert_eq!(review["merged"]["body"], "Local only");
    assert_eq!(review["merged"]["basename"], "New name");
    assert_eq!(review["bodyConflict"], false);
    assert_eq!(
        bytes,
        fs::read(
            server
                .vault
                .join(latest["entity"]["path"].as_str().unwrap())
        )
        .unwrap()
    );
}

#[tokio::test]
async fn rating_mutations_preserve_other_data_and_restore_exact_legacy_values() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path();
    let mut config = json!({"taxonomyRoot":"Taxonomy", "types":[{
        "id":"books", "label":"Books", "path":"Books", "fields":[
            {"field":"感想", "fieldType":"rating", "displayName":"My score", "ratingMax":10},
            {"field":"critic", "fieldType":"rating"},
            {"field":"other", "fieldType":"text"}
        ]
    }]});
    write_vault_config(vault, &config);
    let file = vault.join("Taxonomy/Books/Example.md");
    write_file(&file, "---\n感想: '87.25'\ncritic: 0\nother: {nested: [one, two]}\nunknown: true\n---\n\nHandwritten **notes**.\n");
    let app = cached_inline_router(vault, true, true);
    let path = "/api/entities/books%3AExample";
    let rating_path = format!("{path}/rating");
    let original = request_json(&app, Method::GET, path, None).await.1["entity"].clone();
    assert_eq!(original["ratings"][0]["value"], 87.25);
    assert_eq!(original["ratings"][0]["max"], 10.0);
    assert_eq!(original["ratings"][1]["value"], 0.0);
    assert!(original["ratings"][1]["max"].is_null());
    let request = json!({"revision": original["revision"], "field":"感想", "max":10, "value":8.25});
    let (status, result) =
        request_json(&app, Method::POST, &rating_path, Some(request.clone())).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["entity"]["frontmatter"]["感想"], 8.25);
    assert_eq!(result["entity"]["body"], original["body"]);
    for key in ["critic", "other", "unknown"] {
        assert_eq!(
            result["entity"]["frontmatter"][key],
            original["frontmatter"][key]
        );
    }
    assert_eq!(result["previous"], json!({"present":true,"value":"87.25"}));
    assert_eq!(
        request_json(&app, Method::POST, &rating_path, Some(request))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (status, restored) = request_json(&app, Method::POST, &rating_path, Some(json!({
        "revision":result["entity"]["revision"], "field":"感想", "max":10, "restore":result["previous"]
    }))).await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["entity"]["frontmatter"], original["frontmatter"]);
    let revision = restored["entity"]["revision"].clone();
    for (field, value) in [
        ("感想", json!(11)),
        ("感想", json!(-1)),
        ("other", json!(5)),
    ] {
        assert_eq!(
            request_json(
                &app,
                Method::POST,
                &rating_path,
                Some(json!({"revision":revision, "field":field,"max":10,"value":value}))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let cleared = request_json(
        &app,
        Method::POST,
        &rating_path,
        Some(json!({"revision":revision, "field":"感想", "max":10, "value":null})),
    )
    .await;
    assert_eq!(cleared.0, StatusCode::OK, "{}", cleared.1);
    assert!(cleared.1["entity"]["frontmatter"].get("感想").is_none());
    assert!(cleared.1["entity"]["ratings"][0]["value"].is_null());
    let zero = request_json(&app, Method::POST, &rating_path, Some(json!({"revision":cleared.1["entity"]["revision"], "field":"感想", "max":10, "value":0}))).await;
    assert_eq!(zero.0, StatusCode::OK, "{}", zero.1);
    assert_eq!(zero.1["entity"]["ratings"][0]["value"], 0.0);
    assert_eq!(zero.1["previous"], json!({"present":false,"value":null}));
    // Undo cannot erase a later edit, even with a long-lived resident index.
    let undo = json!({"revision":zero.1["entity"]["revision"], "field":"感想", "max":10, "restore":zero.1["previous"]});
    let current = fs::read_to_string(&file).unwrap();
    write_file(&file, &format!("{current}\nAn external edit."));
    assert_eq!(
        request_json(&app, Method::POST, &rating_path, Some(undo))
            .await
            .0,
        StatusCode::CONFLICT
    );
    // Scale changes must also be checked from freshly read config.
    config["types"][0]["fields"][0]["ratingMax"] = json!(5);
    write_vault_config(vault, &config);
    assert_eq!(request_json(&app, Method::POST, &rating_path, Some(json!({"revision":zero.1["entity"]["revision"], "field":"感想", "max":10, "value":4}))).await.0, StatusCode::CONFLICT);
    let fresh = request_json(&app, Method::GET, path, None).await;
    assert_eq!(fresh.1["entity"]["ratings"][0]["max"], 5.0);
    let readonly = inline_router(vault, true, false);
    assert_eq!(
        request_json(
            &readonly,
            Method::POST,
            &rating_path,
            Some(json!({"revision":"unused", "field":"感想", "max":5,"value":4}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn activity_actions_share_the_log_effect_policy_without_requiring_a_journal() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path();
    write_vault_config(
        vault,
        &json!({"taxonomyRoot":"Taxonomy", "types":[
            {"id":"dates", "label":"Dates", "path":"Dates", "fields":[
                {"field":"始まり", "fieldType":"date", "dateRole":"started"},
                {"field":"fin", "fieldType":"date", "dateRole":"completed"}
            ]},
            {"id":"status", "label":"Status", "path":"Status", "fields":[
                {"field":"段階", "fieldType":"enum", "enumRole":"status", "statusValues":{
                    "planning":["Later"], "ongoing":["Doing"], "completed":["Done"], "dropped":["Stopped"]
                }}
            ]},
            {"id":"journal", "label":"Journal", "path":"Journal", "log":{}, "fields":[]},
            {"id":"plain", "label":"Plain", "path":"Plain", "fields":[
                {"field":"progress", "fieldType":"number"},
                {"field":"started", "fieldType":"text"}
            ]}
        ]}),
    );
    for (folder, content) in [
        ("Dates", "---\n---\nNotes"),
        ("Status", "---\n段階: Later\n---\nNotes"),
        ("Journal", "Notes"),
        ("Plain", "Notes"),
    ] {
        write_file(
            &vault.join(format!("Taxonomy/{folder}/Example.md")),
            content,
        );
    }
    let app = inline_router(vault, true, true);
    for (kind, expected) in [
        ("dates", json!(["started", "completed"])),
        ("status", json!(["started", "completed"])),
        ("journal", json!(["progress"])),
        ("plain", json!([])),
    ] {
        let detail = request_json(
            &app,
            Method::GET,
            &format!("/api/entities/{kind}%3AExample"),
            None,
        )
        .await
        .1;
        assert_eq!(detail["logActions"]["kinds"], expected, "{detail}");
        assert_eq!(detail["logActions"]["writesNote"], kind == "journal");
    }
    let read_only = inline_router(vault, true, false);
    let detail = request_json(
        &read_only,
        Method::GET,
        "/api/entities/dates%3AExample",
        None,
    )
    .await
    .1;
    assert_eq!(detail["logActions"]["kinds"], json!([]));
    let path = "/api/entities/status%3AExample";
    let detail = request_json(&app, Method::GET, path, None).await.1;
    let (status, started) = request_json(
        &app,
        Method::POST,
        &format!("{path}/log"),
        Some(json!({
            "kind":"started", "date":"2026-09-27", "revision":detail["entity"]["revision"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert!(started["notePath"].is_null());
    assert_eq!(
        started["entity"]["logActions"]["kinds"],
        json!(["completed"])
    );
    for status in ["Done", "Stopped", "Hand-edited custom value"] {
        write_file(
            &vault.join("Taxonomy/Status/Example.md"),
            &format!("---\n段階: {status}\n---\nNotes"),
        );
        let detail = request_json(&app, Method::GET, path, None).await.1;
        assert_eq!(detail["logActions"]["kinds"], json!([]), "{detail}");
    }
    let path = "/api/entities/dates%3AExample";
    let detail = request_json(&app, Method::GET, path, None).await.1;
    let (status, finished) = request_json(
        &app,
        Method::POST,
        &format!("{path}/log"),
        Some(json!({
            "kind":"completed", "date":"2026-09-20", "revision":detail["entity"]["revision"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{finished}");
    assert_eq!(
        finished["entity"]["entity"]["frontmatter"]["fin"],
        "2026-09-20"
    );
    assert!(finished["line"].is_null());
}
