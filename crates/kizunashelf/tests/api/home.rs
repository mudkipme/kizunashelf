use crate::support::*;
use pretty_assertions::assert_eq;

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
