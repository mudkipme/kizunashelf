//! Exercises the bridge through its Rust API (the same `KizunaEngine::request`
//! UniFFI exports to Swift): build a minimal vault, then drive `GET /api/health`
//! and assert the response. Validates the bridge logic on the host before it is
//! cross-compiled for Apple targets.

use std::collections::HashMap;
use std::sync::Mutex;

use kizunashelf_ffi::{
    HostSecretStore, KizunaEngine, VaultFileSystem, VaultOptions, VfsDirEntry, VfsError, VfsFile,
    VfsMetadata,
};
use serde_json::Value;

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

    fn write_atomic(&self, path: String, data: Vec<u8>) -> Result<(), VfsError> {
        self.write(path, data)
    }

    fn read_files(&self, paths: Vec<String>) -> Result<Vec<VfsFile>, VfsError> {
        let files = self.files.lock().unwrap();
        Ok(paths
            .into_iter()
            .filter_map(|path| {
                files.get(&path).map(|data| VfsFile {
                    path,
                    data: data.clone(),
                })
            })
            .collect())
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
                        len: 0,
                        modified_unix_nanos: 0,
                    });
                }
                None => {
                    entries.insert(
                        rest.to_string(),
                        VfsDirEntry {
                            name: rest.to_string(),
                            is_dir: false,
                            is_file: true,
                            len: files.get(key).map(|bytes| bytes.len() as u64).unwrap_or(0),
                            modified_unix_nanos: 0,
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

/// In-memory `HostSecretStore` standing in for the Swift Keychain.
#[derive(Default)]
struct FakeSecretStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl FakeSecretStore {
    fn with(entries: &[(&str, &str)]) -> Self {
        let secrets = entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        Self {
            secrets: Mutex::new(secrets),
        }
    }
}

impl HostSecretStore for FakeSecretStore {
    fn get(&self, key: String) -> Option<String> {
        self.secrets.lock().unwrap().get(&key).cloned()
    }

    fn set(&self, key: String, value: String) {
        self.secrets.lock().unwrap().insert(key, value);
    }
}

#[test]
fn ios_engine_browses_a_vault_through_the_swift_filesystem() {
    let vault = FakeVault::default();
    vault.seed(
        "KizunaShelf/config.yaml",
        "taxonomyRoot: Taxonomy\nassetRoot: Assets\ntypes:\n- id: anime\n  label: Anime\n  path: Anime\n  fields:\n  - field: title\n    fieldType: title\n    displayName: Title\n    defaultTitle: true\n",
    );
    vault.seed(
        "Taxonomy/Anime/Star Voyager.md",
        "---\ntitle: Star Voyager\n---\n\nBody.\n",
    );

    let engine = KizunaEngine::with_vault(
        VaultOptions {
            vault_root_label: "My Vault".to_string(),
            vault_identity: "vault-my".to_string(),
            content_writable: false,
            cache_ttl_ms: Some(0),
            index_cache_dir: None,
        },
        Box::new(vault),
        Box::new(FakeSecretStore::default()),
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
fn ios_index_cache_separates_same_named_vaults_by_stable_identity() {
    const CONFIG: &str = "taxonomyRoot: Taxonomy\nassetRoot: Assets\ntypes:\n- id: anime\n  label: Anime\n  path: Anime\n  fields:\n  - field: title\n    fieldType: title\n    displayName: Title\n    defaultTitle: true\n";

    fn cached_title(cache_dir: &str, vault_identity: &str, title: &str) -> String {
        let vault = FakeVault::default();
        vault.seed("KizunaShelf/config.yaml", CONFIG);
        // Keep path, length, and synthetic mtime identical between vaults. If
        // identity is omitted from the cache namespace, the second engine will
        // incorrectly reuse the first vault's parsed entity.
        vault.seed(
            "Taxonomy/Anime/Entry.md",
            &format!("---\ntitle: {title}\n---\n"),
        );
        let engine = KizunaEngine::with_vault(
            VaultOptions {
                vault_root_label: "Same Name".to_string(),
                vault_identity: vault_identity.to_string(),
                content_writable: false,
                cache_ttl_ms: Some(0),
                index_cache_dir: Some(cache_dir.to_string()),
            },
            Box::new(vault),
            Box::new(FakeSecretStore::default()),
        )
        .expect("engine initializes");
        let response = futures::executor::block_on(engine.request(
            "GET".to_string(),
            "/api/entities".to_string(),
            None,
        ))
        .expect("entities request succeeds");
        assert_eq!(response.status, 200, "entities: {}", response.body);
        let body: Value = serde_json::from_str(&response.body).unwrap();
        body["items"][0]["title"]
            .as_str()
            .expect("entity title")
            .to_string()
    }

    let cache = tempfile::tempdir().expect("cache dir");
    let cache_dir = cache.path().to_string_lossy();
    assert_eq!(cached_title(&cache_dir, "vault-a", "Alpha"), "Alpha");
    assert_eq!(cached_title(&cache_dir, "vault-b", "Bravo"), "Bravo");
}

#[test]
fn ios_engine_writes_and_loads_assets_through_the_swift_filesystem() {
    let vault = FakeVault::default();
    vault.seed(
        "KizunaShelf/config.yaml",
        "taxonomyRoot: Taxonomy\nassetRoot: Assets\ntypes:\n- id: anime\n  label: Anime\n  path: Anime\n  fields:\n  - field: title\n    fieldType: title\n    displayName: Title\n    defaultTitle: true\n",
    );
    // An existing entity so the taxonomy directory exists for the first load.
    vault.seed("Taxonomy/Anime/Existing.md", "---\ntitle: Existing\n---\n");
    // A stand-in cover asset; the served content type comes from the extension.
    vault.seed("Assets/cover.png", "PNGDATA");

    let engine = KizunaEngine::with_vault(
        VaultOptions {
            vault_root_label: "My Vault".to_string(),
            vault_identity: "vault-my".to_string(),
            content_writable: true,
            cache_ttl_ms: Some(0),
            index_cache_dir: None,
        },
        Box::new(vault),
        Box::new(FakeSecretStore::default()),
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

const GAMES_CONFIG: &str = "taxonomyRoot: Taxonomy\nassetRoot: Assets\ntypes:\n- id: games\n  label: Games\n  path: Games\n  externalPriority:\n  - igdb\n  fields:\n  - field: title\n    fieldType: title\n    displayName: Title\n    defaultTitle: true\n  - field: igdb_url\n    fieldType: externalRef\n    displayName: IGDB\n    externalRef: igdb\n    externalTypes:\n    - game\n";

fn igdb_provider_summary(secrets: FakeSecretStore) -> Value {
    let vault = FakeVault::default();
    vault.seed("KizunaShelf/config.yaml", GAMES_CONFIG);
    vault.seed("Taxonomy/Games/Zelda.md", "---\ntitle: Zelda\n---\n");

    let engine = KizunaEngine::with_vault(
        VaultOptions {
            vault_root_label: "Vault".to_string(),
            vault_identity: "vault-provider".to_string(),
            content_writable: false,
            cache_ttl_ms: Some(0),
            index_cache_dir: None,
        },
        Box::new(vault),
        Box::new(secrets),
    )
    .expect("engine initializes");

    // No `q` → the handler returns provider availability summaries without making
    // any network call, so this exercises only the credential lookup.
    let response = futures::executor::block_on(engine.request(
        "GET".to_string(),
        "/api/external/search?provider=igdb&type=games".to_string(),
        None,
    ))
    .expect("external search request succeeds");
    assert_eq!(response.status, 200, "search: {}", response.body);

    let body: Value = serde_json::from_str(&response.body).unwrap();
    body["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|provider| provider["id"] == "igdb")
        .cloned()
        .expect("igdb provider summary present")
}

#[test]
fn ios_provider_availability_reads_credentials_from_the_secret_store() {
    use kizunashelf::secrets::{SECRET_IGDB_CLIENT_ID, SECRET_IGDB_CLIENT_SECRET};

    // No credentials in the store → IGDB is unavailable, with a "set credentials"
    // reason routed through the injected secret store.
    let without = igdb_provider_summary(FakeSecretStore::default());
    assert_eq!(without["enabled"], false, "summary: {without}");
    assert!(
        without["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("IGDB"),
        "reason: {without}"
    );

    // Credentials present in the store → IGDB becomes available.
    let with = igdb_provider_summary(FakeSecretStore::with(&[
        (SECRET_IGDB_CLIENT_ID, "client-id"),
        (SECRET_IGDB_CLIENT_SECRET, "client-secret"),
    ]));
    assert_eq!(with["enabled"], true, "summary: {with}");
}

#[test]
fn ios_settings_write_vault_config_through_the_vfs() {
    let vault = FakeVault::default();
    vault.seed(
        "KizunaShelf/config.yaml",
        "taxonomyRoot: Taxonomy\ntypes: []\n",
    );

    let engine = KizunaEngine::with_vault(
        VaultOptions {
            vault_root_label: "My Vault".to_string(),
            vault_identity: "vault-settings".to_string(),
            content_writable: true,
            cache_ttl_ms: Some(0),
            index_cache_dir: None,
        },
        Box::new(vault),
        Box::new(FakeSecretStore::default()),
    )
    .expect("engine initializes");

    // Edit the schema (rename the taxonomy root) via the settings endpoint.
    let saved = futures::executor::block_on(
        engine.request(
            "PUT".to_string(),
            "/api/settings/config".to_string(),
            Some(
                r#"{"app":{"vaultRoot":"My Vault"},"vault":{"taxonomyRoot":"Library","types":[]}}"#
                    .to_string(),
            ),
        ),
    )
    .expect("settings save succeeds");
    assert_eq!(saved.status, 200, "save: {}", saved.body);

    // The change is persisted to the vault config through the VFS.
    let loaded = futures::executor::block_on(engine.request(
        "GET".to_string(),
        "/api/settings/config".to_string(),
        None,
    ))
    .expect("settings read succeeds");
    let body: Value = serde_json::from_str(&loaded.body).unwrap();
    assert_eq!(body["vault"]["taxonomyRoot"], "Library", "settings: {body}");
}
