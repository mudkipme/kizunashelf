//! Exercises the bridge through its Rust API (the same `KizunaEngine::request`
//! UniFFI exports to Swift): build a minimal vault, then drive `GET /api/health`
//! and assert the response. Validates the bridge logic on the host before it is
//! cross-compiled for Apple targets.

use std::fs;

use kizunashelf_ffi::{ApiOptions, KizunaEngine};
use serde_json::Value;
use tempfile::TempDir;

fn write(path: std::path::PathBuf, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn health_request_round_trips() {
    futures::executor::block_on(health_request_round_trips_inner());
}

async fn health_request_round_trips_inner() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    let config_path = temp.path().join("kizunashelf.yaml");

    write(
        config_path.clone(),
        &format!(
            "vaultRoot: {}\ncontentWritable: true\n",
            vault.to_string_lossy()
        ),
    );
    write(
        vault.join(".kizunashelf/config.yaml"),
        "taxonomyRoot: Taxonomy\ntypes: []\n",
    );
    fs::create_dir_all(vault.join("Taxonomy")).unwrap();

    let core = KizunaEngine::new(ApiOptions {
        config_path: config_path.to_string_lossy().into_owned(),
        cache_ttl_ms: Some(0),
        settings_writable: Some(true),
        content_writable: Some(true),
    })
    .expect("core initializes");

    let response = core
        .request("GET".to_string(), "/api/health".to_string(), None)
        .await
        .expect("request succeeds");

    assert_eq!(response.status, 200);
    assert_eq!(response.content_type.as_deref(), Some("application/json"));

    let body: Value = serde_json::from_str(&response.body).expect("body is JSON");
    assert_eq!(body["ok"], true, "health body: {body}");
    assert_eq!(body["entityCount"], 0);
}

#[test]
fn invalid_config_path_still_initializes() {
    // Init builds the runtime + router without touching the vault; errors only
    // surface when a request actually reads the (missing) config.
    let core = KizunaEngine::new(ApiOptions {
        config_path: "/nonexistent/kizunashelf.yaml".to_string(),
        cache_ttl_ms: Some(0),
        settings_writable: Some(true),
        content_writable: Some(true),
    });
    assert!(core.is_ok());
}
