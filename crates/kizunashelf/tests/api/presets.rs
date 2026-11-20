use crate::support::*;
use pretty_assertions::assert_eq;

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
async fn duplicate_type_endpoint_uses_draft_and_does_not_save_it() {
    let server = TestServer::new();
    let before = server.ok_json("/api/settings/config").await;
    let source = json!({
        "id": "children_tv", "label": "Children’s TV", "path": "Children’s TV",
        "fields": [{"field": "name", "fieldType": "title", "titleLanguage": "ja"}],
        "bodySections": [{"heading": "Episodes", "kind": "episodes", "tracking": "checklist"}]
    });
    let (status, result) = request_json(
        &server.app,
        Method::POST,
        "/api/settings/entity-types/duplicate",
        Some(json!({
            "currentTypes": [source], "sourceTypeId": "children_tv"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["entityType"]["id"], "children_tv-2");
    assert_eq!(result["entityType"]["path"], "Children’s TV 2");
    assert_eq!(result["entityType"]["fields"], source["fields"]);
    assert_eq!(result["entityType"]["bodySections"], source["bodySections"]);
    assert_eq!(server.ok_json("/api/settings/config").await, before);

    let (status, error) = request_json(
        &server.app,
        Method::POST,
        "/api/settings/entity-types/duplicate",
        Some(json!({
            "currentTypes": [], "sourceTypeId": "missing"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert!(error["error"]
        .as_str()
        .unwrap()
        .contains("no longer exists"));
}
