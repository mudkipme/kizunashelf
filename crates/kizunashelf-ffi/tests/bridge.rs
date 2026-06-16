//! Exercises the bridge through its Rust API (the same `KizunaEngine::request`
//! UniFFI exports to Swift): build a minimal vault, then drive `GET /api/health`
//! and assert the response. Validates the bridge logic on the host before it is
//! cross-compiled for Apple targets.

use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

use kizunashelf_ffi::{
    ApiOptions, KizunaEngine, VaultFileSystem, VaultOptions, VfsDirEntry, VfsError, VfsMetadata,
};
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

/// In-memory `VaultFileSystem` standing in for the Swift implementation, so the
/// iOS engine path (`KizunaEngine::with_vault` → injected VFS → vault config +
/// entities read through callbacks) can be exercised on the host.
#[derive(Default)]
struct FakeVault {
    files: Mutex<HashMap<String, Vec<u8>>>,
}

impl FakeVault {
    fn seed(&self, path: &str, contents: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), contents.as_bytes().to_vec());
    }

    fn is_dir(files: &HashMap<String, Vec<u8>>, path: &str) -> bool {
        path.is_empty() || files.keys().any(|key| key.starts_with(&format!("{path}/")))
    }
}

impl VaultFileSystem for FakeVault {
    fn read(&self, path: String) -> Result<Vec<u8>, VfsError> {
        self.files
            .lock()
            .unwrap()
            .get(&path)
            .cloned()
            .ok_or(VfsError::NotFound)
    }

    fn write(&self, path: String, data: Vec<u8>) -> Result<(), VfsError> {
        self.files.lock().unwrap().insert(path, data);
        Ok(())
    }

    fn create_dir_all(&self, _path: String) -> Result<(), VfsError> {
        Ok(())
    }

    fn read_dir(&self, path: String) -> Result<Vec<VfsDirEntry>, VfsError> {
        let files = self.files.lock().unwrap();
        if !Self::is_dir(&files, &path) {
            return Err(VfsError::NotFound);
        }
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        let mut entries: HashMap<String, VfsDirEntry> = HashMap::new();
        for key in files.keys() {
            let Some(rest) = key.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            match rest.split_once('/') {
                Some((dir, _)) => {
                    entries.entry(dir.to_string()).or_insert(VfsDirEntry {
                        name: dir.to_string(),
                        is_dir: true,
                        is_file: false,
                    });
                }
                None => {
                    entries.insert(
                        rest.to_string(),
                        VfsDirEntry {
                            name: rest.to_string(),
                            is_dir: false,
                            is_file: true,
                        },
                    );
                }
            }
        }
        Ok(entries.into_values().collect())
    }

    fn metadata(&self, path: String) -> Result<VfsMetadata, VfsError> {
        let files = self.files.lock().unwrap();
        if let Some(bytes) = files.get(&path) {
            return Ok(VfsMetadata {
                is_dir: false,
                is_file: true,
                len: bytes.len() as u64,
                modified_unix_nanos: 1,
            });
        }
        if Self::is_dir(&files, &path) {
            return Ok(VfsMetadata {
                is_dir: true,
                is_file: false,
                len: 0,
                modified_unix_nanos: 0,
            });
        }
        Err(VfsError::NotFound)
    }

    fn rename(&self, from: String, to: String) -> Result<(), VfsError> {
        let mut files = self.files.lock().unwrap();
        let bytes = files.remove(&from).ok_or(VfsError::NotFound)?;
        files.insert(to, bytes);
        Ok(())
    }

    fn remove_file(&self, path: String) -> Result<(), VfsError> {
        self.files
            .lock()
            .unwrap()
            .remove(&path)
            .map(|_| ())
            .ok_or(VfsError::NotFound)
    }
}

#[test]
fn ios_engine_browses_a_vault_through_the_swift_filesystem() {
    let vault = FakeVault::default();
    vault.seed(
        ".kizunashelf/config.yaml",
        "taxonomyRoot: Taxonomy\nassetRoot: Assets\ntypes:\n- id: anime\n  label: Anime\n  path: Anime\n  fields:\n  - field: title\n    fieldType: title\n    displayName: Title\n    defaultTitle: true\n",
    );
    vault.seed(
        "Taxonomy/Anime/Star Voyager.md",
        "---\ntitle: Star Voyager\n---\n\nBody.\n",
    );

    let engine = KizunaEngine::with_vault(
        VaultOptions {
            vault_root_label: "My Vault".to_string(),
            content_writable: false,
            cache_ttl_ms: Some(0),
            read_concurrency: None,
        },
        Box::new(vault),
    )
    .expect("engine initializes from the injected vault filesystem");

    let health = futures::executor::block_on(engine.request(
        "GET".to_string(),
        "/api/health".to_string(),
        None,
    ))
    .expect("health request succeeds");
    assert_eq!(health.status, 200);
    let body: Value = serde_json::from_str(&health.body).unwrap();
    assert_eq!(body["ok"], true, "health: {body}");
    assert_eq!(body["entityCount"], 1);

    let entities = futures::executor::block_on(engine.request(
        "GET".to_string(),
        "/api/entities".to_string(),
        None,
    ))
    .expect("entities request succeeds");
    assert_eq!(entities.status, 200);
    let body: Value = serde_json::from_str(&entities.body).unwrap();
    let titles: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["title"].as_str())
        .collect();
    assert!(titles.contains(&"Star Voyager"), "entities: {body}");
}

#[test]
fn ios_engine_writes_and_loads_assets_through_the_swift_filesystem() {
    let vault = FakeVault::default();
    vault.seed(
        ".kizunashelf/config.yaml",
        "taxonomyRoot: Taxonomy\nassetRoot: Assets\ntypes:\n- id: anime\n  label: Anime\n  path: Anime\n  fields:\n  - field: title\n    fieldType: title\n    displayName: Title\n    defaultTitle: true\n",
    );
    // An existing entity so the taxonomy directory exists for the first load.
    vault.seed("Taxonomy/Anime/Existing.md", "---\ntitle: Existing\n---\n");
    // A stand-in cover asset; the served content type comes from the extension.
    vault.seed("Assets/cover.png", "PNGDATA");

    let engine = KizunaEngine::with_vault(
        VaultOptions {
            vault_root_label: "My Vault".to_string(),
            content_writable: true,
            cache_ttl_ms: Some(0),
            read_concurrency: None,
        },
        Box::new(vault),
    )
    .expect("engine initializes");

    // Create a new entity through the write path (mutations -> VFS).
    let created = futures::executor::block_on(
        engine.request(
            "POST".to_string(),
            "/api/entities".to_string(),
            Some(
                r#"{"type":"anime","basename":"New Show","frontmatter":{"title":"New Show"}}"#
                    .to_string(),
            ),
        ),
    )
    .expect("create request succeeds");
    assert_eq!(created.status, 200, "create: {}", created.body);
    // `Entity` flattens its `EntitySummary`, so the title is at `entity.title`.
    let body: Value = serde_json::from_str(&created.body).unwrap();
    assert_eq!(body["entity"]["title"], "New Show", "create: {body}");

    // It is now indexed alongside the existing entity.
    let list = futures::executor::block_on(engine.request(
        "GET".to_string(),
        "/api/entities".to_string(),
        None,
    ))
    .expect("list request succeeds");
    let body: Value = serde_json::from_str(&list.body).unwrap();
    let titles: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["title"].as_str())
        .collect();
    assert!(titles.contains(&"Existing"), "list: {body}");
    assert!(titles.contains(&"New Show"), "list: {body}");

    // Binary asset loading returns raw bytes + a content type from the extension.
    let asset = futures::executor::block_on(engine.asset("Assets/cover.png".to_string()))
        .expect("asset loads");
    assert_eq!(asset.bytes, b"PNGDATA");
    assert_eq!(asset.content_type.as_deref(), Some("image/png"));

    // A missing asset surfaces as an error rather than empty bytes.
    let missing = futures::executor::block_on(engine.asset("Assets/missing.png".to_string()));
    assert!(missing.is_err());
}
