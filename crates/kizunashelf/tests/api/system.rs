use crate::support::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn manual_example_schema_and_entity_work_together() {
    // Execute the published examples themselves so renamed fields and roles
    // cannot silently drift between the introductory guide and reference.
    let config_page = include_str!("../../../../manual/content/reference/config.md");
    let entity_page = include_str!("../../../../manual/content/concepts/anatomy-of-an-entity.md");
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
