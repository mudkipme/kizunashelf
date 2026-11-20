use crate::support::*;
use pretty_assertions::assert_eq;

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
