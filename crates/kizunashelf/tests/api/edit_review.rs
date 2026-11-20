use crate::support::*;
use pretty_assertions::assert_eq;

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
