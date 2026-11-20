use crate::support::*;
use pretty_assertions::assert_eq;

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
