use crate::support::*;
use pretty_assertions::assert_eq;

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

/// Wraps a vault config object in the `{ vault }` PUT `/api/settings/config` body
/// (the vault root + write mode are owned by the runtime and never sent).
fn vault_settings_body(config: &Value) -> Value {
    json!({ "vault": config.clone() })
}
