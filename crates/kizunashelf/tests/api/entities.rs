use crate::support::*;
use pretty_assertions::assert_eq;

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
