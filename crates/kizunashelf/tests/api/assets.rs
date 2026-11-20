use crate::support::*;
use pretty_assertions::assert_eq;

/// Like [`inline_router`] but with host-driven asset ingest enabled — the iOS
/// in-process-host posture the `plan`/`ingest` endpoints require. The network
/// server uses [`inline_router`] (off), so the host-path surface is gated there.
fn host_inline_router(vault_root: &Path, content_writable: bool) -> Router {
    build_inline_router(vault_root, true, content_writable, true, Duration::ZERO)
}

/// Minimal valid 1x1 PNG.
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

async fn upload_asset(app: &Router, id: &str, body: Value) -> (StatusCode, Value) {
    request_json(
        app,
        Method::POST,
        &format!("/api/entities/{}/assets/upload", urlencoding::encode(id)),
        Some(body),
    )
    .await
}

async fn start_mock_image_server() -> std::net::SocketAddr {
    // The mock server binds to loopback, which the SSRF guard blocks by default.
    // Enable the documented escape hatch so the download path can reach it.
    std::env::set_var("KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS", "1");
    let app = Router::new()
        .route(
            "/image.png",
            axum::routing::get(|| async {
                ([(header::CONTENT_TYPE, "image/png")], PNG_1X1.to_vec())
            }),
        )
        .route(
            "/notimage",
            axum::routing::get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html")],
                    b"<html>nope</html>".to_vec(),
                )
            }),
        )
        .route(
            "/missing",
            axum::routing::get(|| async { StatusCode::NOT_FOUND }),
        )
        .route(
            "/slow.png",
            axum::routing::get(|| async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                ([(header::CONTENT_TYPE, "image/png")], PNG_1X1.to_vec())
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

fn asset_test_app(
    content_writable: bool,
    write_entities: impl FnOnce(&Path),
) -> (Router, TempDir, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir_all(vault.join("Taxonomy/Anime")).unwrap();
    write_entities(&vault);
    let config = json!({
        "taxonomyRoot": "Taxonomy",
        "assetRoot": "Assets",
        "types": [
            {
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                    { "field": "cover_url", "fieldType": "image", "displayName": "Cover" },
                    { "field": "shots", "fieldType": "imageList", "displayName": "Shots" }
                ]
            }
        ]
    });
    write_vault_config(&vault, &config);
    // Asset plan/ingest is the host-driven (iOS) flow, so the asset tests run with
    // host-path ingest enabled. A dedicated test covers the network posture (off).
    let app = host_inline_router(&vault, content_writable);
    (app, temp, vault)
}

async fn entity_revision(app: &Router, id: &str) -> String {
    let detail = request_json(
        app,
        Method::GET,
        &format!("/api/entities/{}", urlencoding::encode(id)),
        None,
    )
    .await;
    assert_eq!(detail.0, StatusCode::OK, "detail: {}", detail.1);
    detail.1["entity"]["revision"].as_str().unwrap().to_string()
}

async fn download_assets(app: &Router, id: &str, revision: &str) -> (StatusCode, Value) {
    request_json(
        app,
        Method::POST,
        &format!("/api/entities/{}/assets/download", urlencoding::encode(id)),
        Some(json!({ "revision": revision })),
    )
    .await
}

async fn request_raw(app: &Router, path: &str) -> (StatusCode, Option<String>, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(path)
                .body(body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, content_type, bytes)
}

#[tokio::test]
async fn asset_download_writes_local_file_and_serves_it() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/image.png");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nBody\n"),
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["field"], "cover_url");
    assert_eq!(results[0]["status"], "downloaded");
    let local_path = results[0]["path"].as_str().unwrap();
    assert_eq!(
        local_path,
        "Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png"
    );

    // Frontmatter (and therefore the summary cover) now points at the local file.
    assert_eq!(body["entity"]["image"], local_path);
    assert_eq!(body["entity"]["frontmatter"]["cover_url"], local_path);

    // The file is written under the vault.
    let written = vault.join(local_path);
    assert_eq!(fs::read(&written).unwrap(), PNG_1X1);

    // And it is served back through the asset route with an image content type.
    let serve_path = format!("/api/assets/{}", local_path.replace(' ', "%20"));
    let (serve_status, content_type, bytes) = request_raw(&app, &serve_path).await;
    assert_eq!(serve_status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("image/png"));
    assert_eq!(bytes, PNG_1X1);
}
#[tokio::test]
async fn asset_download_does_not_overwrite_an_edit_during_network_fetch() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/slow.png");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nOriginal body\n"),
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let task_app = app.clone();
    let task_revision = revision.clone();
    let download =
        tokio::spawn(async move { download_assets(&task_app, id, &task_revision).await });

    // The mock response stays in flight long enough for an Obsidian-style
    // external edit to land after the request has begun but before its commit.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let entity_path = vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md");
    write_file(
        &entity_path,
        &format!(
            "---\ntitle: Steins;Gate 0\ncover_url: {cover}\nexternal_note: keep me\n---\nExternally edited body\n"
        ),
    );

    let (status, body) = download.await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    let raw = fs::read_to_string(entity_path).unwrap();
    assert!(raw.contains("external_note: keep me"), "{raw}");
    assert!(raw.contains("Externally edited body"), "{raw}");
    assert!(raw.contains(&cover), "{raw}");
}

#[tokio::test]
async fn asset_download_failure_keeps_remote_url() {
    let addr = start_mock_image_server().await;
    let missing = format!("http://{addr}/missing");
    let not_image = format!("http://{addr}/notimage");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {missing}\nshots:\n  - {not_image}\n---\nBody\n"),
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let results = body["results"].as_array().unwrap();
    assert!(results.iter().all(|item| item["status"] == "failed"));

    // Remote URLs are left untouched so the user can retry.
    assert_eq!(body["entity"]["frontmatter"]["cover_url"], missing);
    assert_eq!(body["entity"]["frontmatter"]["shots"][0], not_image);

    // No asset directory was created for this entity.
    assert!(!vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
}

#[tokio::test]
async fn asset_download_handles_image_list_partially() {
    let addr = start_mock_image_server().await;
    let ok = format!("http://{addr}/image.png");
    let bad = format!("http://{addr}/missing");
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\nshots:\n  - {ok}\n  - {bad}\n---\nBody\n"),
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let shots = body["entity"]["frontmatter"]["shots"].as_array().unwrap();
    // First element became a local hashed path; the failed one keeps its URL.
    assert!(shots[0]
        .as_str()
        .unwrap()
        .starts_with("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/shots/"));
    assert!(shots[0].as_str().unwrap().ends_with(".png"));
    assert_eq!(shots[1], bad);
}

#[tokio::test]
async fn plan_lists_remote_image_fields_only() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.jpg\nshots:\n  - https://img.example/a.png\n  - Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/shots/local.png\n---\nBody\n",
        );
    });

    let (status, body) = request_json(&app, Method::GET, "/api/asset-downloads/plan", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["scope"], "all");

    let items = body["items"].as_array().unwrap();
    // cover_url (single) + one remote shots element; the local shot is excluded.
    assert_eq!(items.len(), 2, "{body}");

    let cover = items
        .iter()
        .find(|item| item["field"] == "cover_url")
        .unwrap();
    assert_eq!(cover["entityId"], "anime:Steins;Gate 0 (Anime)");
    assert_eq!(cover["sourceUrl"], "https://img.example/cover.jpg");
    assert!(cover["listKey"].is_null());

    let shot = items.iter().find(|item| item["field"] == "shots").unwrap();
    assert_eq!(shot["sourceUrl"], "https://img.example/a.png");
    // List fields carry the element URL as the list key.
    assert_eq!(shot["listKey"], "https://img.example/a.png");

    let (status, _) = request_json(
        &app,
        Method::GET,
        "/api/asset-downloads/plan?entityType=nope",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ingest_places_host_downloaded_file_and_rewrites_single_field() {
    let (app, temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.jpg\n---\nBody\n",
        );
    });

    // The host (iOS) already downloaded the bytes to an app-temp file outside the vault.
    let source = temp.path().join("ingest-cover.png");
    fs::write(&source, PNG_1X1).unwrap();

    let id = "anime:Steins;Gate 0 (Anime)";
    let (status, body) = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}/assets/ingest", urlencoding::encode(id)),
        Some(json!({
            "field": "cover_url",
            "sourceUrl": "https://img.example/cover.jpg",
            "sourcePath": source.to_string_lossy(),
            "contentType": "image/png",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["status"], "downloaded");
    assert_eq!(
        body["entity"]["frontmatter"]["cover_url"],
        "Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png"
    );
    assert!(vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png")
        .exists());
    // The host-temp source file is consumed (deleted) by the core.
    assert!(!source.exists());
}

#[tokio::test]
async fn ingest_checks_expected_revision_against_fresh_file_before_asset_writes() {
    let (_, temp, vault) = asset_test_app(true, |vault| {
        write_file(&vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.jpg\n---\nOriginal notes\n");
    });
    let app = build_inline_router(&vault, true, true, true, Duration::from_secs(3600));
    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let path = vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md");
    let changed = fs::read_to_string(&path)
        .unwrap()
        .replace("Original notes", "External edit to keep");
    fs::write(&path, &changed).unwrap();
    let source = temp.path().join("stale-cover.png");
    fs::write(&source, PNG_1X1).unwrap();
    let result = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}/assets/ingest", urlencoding::encode(id)),
        Some(
            json!({ "field": "cover_url", "sourceUrl": "https://img.example/cover.jpg",
            "sourcePath": source, "contentType": "image/png", "revision": revision }),
        ),
    )
    .await;
    assert_eq!(result.0, StatusCode::CONFLICT, "{}", result.1);
    assert_eq!(fs::read_to_string(&path).unwrap(), changed);
    assert!(
        !vault.join("Assets").exists(),
        "Rejected ingest must not stage vault assets"
    );
    assert!(!source.exists(), "The host staging file was consumed");
}

#[tokio::test]
async fn host_routers_with_one_identity_cannot_ingest_the_same_revision_twice() {
    let (_, temp, vault) = asset_test_app(true, |vault| {
        write_file(&vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.jpg\nshots: [https://img.example/a.png]\n---\nKeep notes\n");
    });
    let identity = format!("host-ingest:{}", temp.path().display());
    let front = build_inline_router_with_identity(
        &vault,
        true,
        true,
        true,
        Duration::from_secs(3600),
        Some(identity.clone()),
    );
    let back = build_inline_router_with_identity(
        &vault,
        true,
        true,
        true,
        Duration::from_secs(3600),
        Some(identity),
    );
    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&front, id).await;
    assert_eq!(entity_revision(&back, id).await, revision);
    let endpoint = format!("/api/entities/{}/assets/ingest", urlencoding::encode(id));
    let first = temp.path().join("front.png");
    let second = temp.path().join("back.png");
    fs::write(&first, PNG_1X1).unwrap();
    fs::write(&second, PNG_1X1).unwrap();
    let payload = |source: &Path| {
        json!({ "field": "cover_url", "sourceUrl": "https://img.example/cover.jpg",
        "sourcePath": source, "contentType": "image/png", "revision": revision })
    };
    let (a, b) = tokio::join!(
        request_json(&front, Method::POST, &endpoint, Some(payload(&first))),
        request_json(&back, Method::POST, &endpoint, Some(payload(&second)))
    );
    assert_eq!(
        [a.0, b.0].iter().filter(|s| **s == StatusCode::OK).count(),
        1,
        "{a:?} {b:?}"
    );
    assert_eq!(
        [a.0, b.0]
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1,
        "{a:?} {b:?}"
    );
    let successful = if a.0 == StatusCode::OK { a.1 } else { b.1 };
    let next_revision = successful["entity"]["revision"].as_str().unwrap();
    assert_ne!(next_revision, revision);
    // A later field in the same batch carries the revision returned by its own previous commit.
    let source = temp.path().join("next.png");
    fs::write(&source, PNG_1X1).unwrap();
    let result = request_json(
        &back,
        Method::POST,
        &endpoint,
        Some(json!({ "field": "shots",
        "listKey": "https://img.example/a.png", "sourceUrl": "https://img.example/a.png",
        "sourcePath": source, "revision": next_revision, "contentType": "image/png" })),
    )
    .await;
    assert_eq!(result.0, StatusCode::OK, "{}", result.1);
    assert_eq!(result.1["result"]["status"], "downloaded");
    assert_eq!(
        entity_revision(&front, id).await,
        result.1["entity"]["revision"].as_str().unwrap()
    );
    assert!(
        fs::read_to_string(vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"))
            .unwrap()
            .contains("Keep notes")
    );
}

#[tokio::test]
async fn ingest_preserves_an_existing_asset_referenced_by_the_same_entity() {
    let existing = "Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png";
    let old_bytes = [PNG_1X1, b"original"].concat();
    let (app, temp, vault) = asset_test_app(true, |vault| {
        write_file(&vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.png\nshots: [\"{existing}\"]\n---\nBody\n"));
        let path = vault.join(existing);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &old_bytes).unwrap();
    });
    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let source = temp.path().join("new.png");
    fs::write(&source, PNG_1X1).unwrap();
    let result = request_json(&app, Method::POST,
        &format!("/api/entities/{}/assets/ingest", urlencoding::encode(id)),
        Some(json!({ "field": "cover_url", "sourceUrl": "https://img.example/cover.png", "sourcePath": source, "revision": revision }))).await;
    assert_eq!(result.0, StatusCode::OK, "{}", result.1);
    assert_eq!(result.1["result"]["status"], "downloaded");
    assert_eq!(fs::read(vault.join(existing)).unwrap(), old_bytes);
    let saved = result.1["entity"]["frontmatter"]["cover_url"]
        .as_str()
        .unwrap();
    assert_ne!(saved, existing);
    assert_eq!(fs::read(vault.join(saved)).unwrap(), PNG_1X1);
    assert_eq!(
        result.1["entity"]["frontmatter"]["shots"],
        json!([existing])
    );
}

#[tokio::test]
async fn ingest_rewrites_one_list_element() {
    let (app, temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\nshots:\n  - https://img.example/a.png\n  - https://img.example/b.png\n---\nBody\n",
        );
    });

    let source = temp.path().join("shot-a.png");
    fs::write(&source, PNG_1X1).unwrap();

    let id = "anime:Steins;Gate 0 (Anime)";
    let (status, body) = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}/assets/ingest", urlencoding::encode(id)),
        Some(json!({
            "field": "shots",
            "listKey": "https://img.example/a.png",
            "sourceUrl": "https://img.example/a.png",
            "sourcePath": source.to_string_lossy(),
            "contentType": "image/png",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["status"], "downloaded");

    let shots = body["entity"]["frontmatter"]["shots"].as_array().unwrap();
    // The ingested element became a local hashed path; the other keeps its URL.
    assert!(shots[0]
        .as_str()
        .unwrap()
        .starts_with("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/shots/"));
    assert!(shots[0].as_str().unwrap().ends_with(".png"));
    assert_eq!(shots[1], "https://img.example/b.png");
}

#[tokio::test]
async fn ingest_skips_when_source_url_no_longer_matches() {
    let (app, temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/new.jpg\n---\nBody\n",
        );
    });

    let source = temp.path().join("stale.png");
    fs::write(&source, PNG_1X1).unwrap();

    let id = "anime:Steins;Gate 0 (Anime)";
    let (status, body) = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}/assets/ingest", urlencoding::encode(id)),
        Some(json!({
            "field": "cover_url",
            // A download that began against a URL the user has since changed.
            "sourceUrl": "https://img.example/old.jpg",
            "sourcePath": source.to_string_lossy(),
            "contentType": "image/png",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["status"], "skipped");
    // Frontmatter keeps the user's current value; nothing is written.
    assert_eq!(
        body["entity"]["frontmatter"]["cover_url"],
        "https://img.example/new.jpg"
    );
    assert!(!vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
    // Even on skip, the stale temp file is cleaned up.
    assert!(!source.exists());
}

#[tokio::test]
async fn upload_places_single_image_and_leaves_frontmatter_for_save() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: https://img.example/cover.jpg\n---\nBody\n",
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let (status, body) = upload_asset(
        &app,
        id,
        json!({
            "field": "cover_url",
            "dataBase64": b64(PNG_1X1),
            "contentType": "image/png",
            "filename": "cover.png",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let path = body["path"].as_str().unwrap();
    assert!(path.starts_with("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url-"));
    assert!(path.ends_with(".png"));
    assert_eq!(body["conflictResolved"], false);
    // The file is placed under the vault.
    assert_eq!(fs::read(vault.join(path)).unwrap(), PNG_1X1);

    // Upload stages into the editor draft only — frontmatter is untouched until the
    // normal save mutation runs.
    let detail = request_json(
        &app,
        Method::GET,
        &format!("/api/entities/{}", urlencoding::encode(id)),
        None,
    )
    .await;
    assert_eq!(
        detail.1["entity"]["frontmatter"]["cover_url"],
        "https://img.example/cover.jpg"
    );
}

#[tokio::test]
async fn upload_preserves_saved_and_pending_assets_and_rejects_path_collisions() {
    let old_path = "Assets/Taxonomy/Anime/Show/cover_url.png";
    let original = format!("---\ntitle: Show\ncover_url: {old_path}\n---\nKeep notes\n");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(&vault.join("Taxonomy/Anime/Show.md"), &original);
        write_file(&vault.join(old_path), "saved cover bytes");
    });
    let payload =
        json!({"field": "cover_url", "dataBase64": b64(PNG_1X1), "contentType": "image/png"});
    let (status, first) = upload_asset(&app, "anime:Show", payload.clone()).await;
    assert_eq!(status, StatusCode::OK);
    let path = first["path"].as_str().unwrap();
    assert_ne!(path, old_path);
    assert_eq!(
        fs::read(vault.join(old_path)).unwrap(),
        b"saved cover bytes"
    );
    assert_eq!(
        fs::read_to_string(vault.join("Taxonomy/Anime/Show.md")).unwrap(),
        original
    );
    let (_, repeated) = upload_asset(&app, "anime:Show", payload.clone()).await;
    assert_eq!(first["path"], repeated["path"]);
    let mut second_bytes = PNG_1X1.to_vec();
    second_bytes.extend_from_slice(b"another image payload");
    let (status, second) = upload_asset(
        &app,
        "anime:Show",
        json!({"field": "cover_url", "dataBase64": b64(&second_bytes), "contentType": "image/png"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(first["path"], second["path"]);
    assert_eq!(fs::read(vault.join(path)).unwrap(), PNG_1X1);
    fs::write(vault.join(path), b"unrelated existing file").unwrap();
    let (status, _) = upload_asset(&app, "anime:Show", payload).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        fs::read(vault.join(path)).unwrap(),
        b"unrelated existing file"
    );
}

#[tokio::test]
async fn upload_places_image_list_element_by_content_hash() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\n---\nBody\n",
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let (status, body) = upload_asset(
        &app,
        id,
        json!({
            "field": "shots",
            "dataBase64": b64(PNG_1X1),
            "contentType": "image/png",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let path = body["path"].as_str().unwrap();
    assert!(path.starts_with("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/shots/"));
    assert!(path.ends_with(".png"));
    assert!(vault.join(path).exists());
}

#[tokio::test]
async fn upload_rejects_non_image_bytes() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\n---\nBody\n",
        );
    });

    let (status, _body) = upload_asset(
        &app,
        "anime:Steins;Gate 0 (Anime)",
        json!({
            "field": "cover_url",
            "dataBase64": b64(b"<html>nope</html>"),
            "contentType": "text/html",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_rejects_non_image_field_and_bad_base64() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\n---\nBody\n",
        );
    });

    // `title` is not an image field.
    let (status, _body) = upload_asset(
        &app,
        "anime:Steins;Gate 0 (Anime)",
        json!({ "field": "title", "dataBase64": b64(PNG_1X1) }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Malformed base64 payload.
    let (status, _body) = upload_asset(
        &app,
        "anime:Steins;Gate 0 (Anime)",
        json!({ "field": "cover_url", "dataBase64": "not valid base64!!!" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_is_disabled_in_read_only_mode() {
    let (app, _temp, vault) = asset_test_app(false, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\n---\nBody\n",
        );
    });

    let (status, body) = upload_asset(
        &app,
        "anime:Steins;Gate 0 (Anime)",
        json!({
            "field": "cover_url",
            "dataBase64": b64(PNG_1X1),
            "contentType": "image/png",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "Content writes are disabled");
    // Nothing was written under the vault.
    assert!(!vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
}

#[tokio::test]
async fn host_driven_asset_endpoints_are_rejected_on_the_network_runtime() {
    // The web/desktop runtimes leave `host_asset_ingest` off (TestServer uses
    // `inline_router`), so the host-path `plan`/`ingest` endpoints — which read and
    // *delete* a client-supplied absolute path — must be unreachable by an
    // untrusted caller. (iOS opts in via the FFI host; the asset tests cover that.)
    let server = TestServer::new();

    // A file outside the vault that an attacker might target for deletion.
    let temp = TempDir::new().unwrap();
    let victim = temp.path().join("victim.txt");
    fs::write(&victim, b"do not delete").unwrap();

    let (status, body) = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}/assets/ingest",
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ),
        Some(json!({
            "field": "cover_url",
            "sourceUrl": "https://img.example/cover.jpg",
            "sourcePath": victim.to_string_lossy(),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    // The gate fires before any filesystem access — the host file is untouched.
    assert!(
        victim.exists(),
        "gated ingest must not read or delete host paths"
    );

    let (status, body) = server.json("/api/asset-downloads/plan").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

#[tokio::test]
async fn asset_download_avoids_overwriting_another_entitys_file() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/image.png");
    // "Old Show" references a local cover that sits inside "Steins;Gate 0 (Anime)"'s asset
    // directory (as if filenames were swapped outside the app).
    let collide = "Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png";
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nBody\n"),
        );
        write_file(
            &vault.join("Taxonomy/Anime/Old Show.md"),
            &format!("---\ntitle: Old Show\ncover_url: {collide}\n---\nBody\n"),
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let (status, body) = download_assets(&app, id, &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let new_path = body["entity"]["frontmatter"]["cover_url"].as_str().unwrap();
    assert_ne!(new_path, collide, "must not reuse another entity's file");
    assert!(new_path.starts_with("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url-"));
    assert_eq!(body["results"][0]["conflictResolved"], true);

    // The other entity's reference is untouched.
    let other_revision = entity_revision(&app, "anime:Old Show").await;
    assert!(!other_revision.is_empty());
}

#[tokio::test]
async fn asset_serve_route_rejects_path_escape() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\n---\nBody\n",
        );
        // A secret outside the asset root.
        write_file(&vault.join("secret.txt"), "top secret");
    });

    let (status, _content_type, _bytes) =
        request_raw(&app, "/api/assets/..%2f..%2fsecret.txt").await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
        "escape must not succeed, got {status}"
    );
}

#[tokio::test]
async fn asset_download_is_disabled_in_read_only_mode() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/image.png");
    let (app, _temp, _vault) = asset_test_app(false, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nBody\n"),
        );
    });

    let capabilities = request_json(&app, Method::GET, "/api/capabilities", None).await;
    assert_eq!(capabilities.1["assetDownloadEnabled"], false);

    let (status, body) = download_assets(&app, "anime:Steins;Gate 0 (Anime)", "any-revision").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "Content writes are disabled");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn asset_batch_job_downloads_remote_covers() {
    let addr = start_mock_image_server().await;
    let cover = format!("http://{addr}/image.png");
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nBody\n"),
        );
        write_file(
            &vault.join("Taxonomy/Anime/Robotics;Notes.md"),
            &format!("---\ntitle: Robotics;Notes\ncover_url: {cover}\n---\nBody\n"),
        );
    });

    let created = request_json(&app, Method::POST, "/api/asset-jobs", Some(json!({}))).await;
    assert_eq!(created.0, StatusCode::OK, "{}", created.1);
    assert_eq!(created.1["total"], 2);
    let job_id = created.1["id"].as_str().unwrap().to_string();

    let mut job = created.1;
    for _ in 0..100 {
        let polled = request_json(
            &app,
            Method::GET,
            &format!("/api/asset-jobs/{}", urlencoding::encode(&job_id)),
            None,
        )
        .await;
        assert_eq!(polled.0, StatusCode::OK);
        job = polled.1;
        if job["status"] == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(job["status"], "completed", "job did not finish: {job}");
    assert_eq!(job["processed"], 2);
    assert_eq!(job["downloaded"], 2);
    assert_eq!(job["failed"], 0);

    assert!(vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png")
        .exists());
    assert!(vault
        .join("Assets/Taxonomy/Anime/Robotics;Notes/cover_url.png")
        .exists());

    let list = request_json(&app, Method::GET, "/api/asset-jobs", None).await;
    assert!(list.1["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == job_id));
}

#[tokio::test]
async fn asset_batch_job_rejects_a_second_job_while_one_is_running() {
    let addr = start_mock_image_server().await;
    // The slow endpoint keeps the first job in flight long enough to fire a second.
    let cover = format!("http://{addr}/slow.png");
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            &format!("---\ntitle: Steins;Gate 0\ncover_url: {cover}\n---\nBody\n"),
        );
    });

    let first = request_json(&app, Method::POST, "/api/asset-jobs", Some(json!({}))).await;
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    let job_id = first.1["id"].as_str().unwrap().to_string();

    // The first job is inserted (queued/running) before its POST returns, so a
    // second request must be rejected while it is still in flight.
    let second = request_json(&app, Method::POST, "/api/asset-jobs", Some(json!({}))).await;
    assert_eq!(second.0, StatusCode::CONFLICT, "{}", second.1);

    // Once the first job finishes, a new job is allowed again.
    for _ in 0..100 {
        let polled = request_json(
            &app,
            Method::GET,
            &format!("/api/asset-jobs/{}", urlencoding::encode(&job_id)),
            None,
        )
        .await;
        if polled.1["status"] == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let third = request_json(&app, Method::POST, "/api/asset-jobs", Some(json!({}))).await;
    assert_eq!(third.0, StatusCode::OK, "{}", third.1);
}

#[tokio::test]
async fn rename_moves_asset_directory_and_rewrites_paths() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png\n---\nBody\n",
        );
        write_file(
            &vault.join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png"),
            "fake-bytes",
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let updated = request_json(
        &app,
        Method::POST,
        &format!("/api/entities/{}", urlencoding::encode(id)),
        Some(json!({ "revision": revision, "renameTo": "Steins;Gate 0 (Anime) 2" })),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);

    assert!(!vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
    assert!(vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime) 2/cover_url.png")
        .exists());
    assert_eq!(
        updated.1["entity"]["frontmatter"]["cover_url"],
        "Assets/Taxonomy/Anime/Steins;Gate 0 (Anime) 2/cover_url.png"
    );
}

#[tokio::test]
async fn delete_trashes_asset_directory() {
    let (app, _temp, vault) = asset_test_app(true, |vault| {
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png\n---\nBody\n",
        );
        write_file(
            &vault.join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png"),
            "fake-bytes",
        );
    });

    let id = "anime:Steins;Gate 0 (Anime)";
    let revision = entity_revision(&app, id).await;
    let deleted = request_json(
        &app,
        Method::DELETE,
        &format!("/api/entities/{}", urlencoding::encode(id)),
        Some(json!({ "revision": revision })),
    )
    .await;
    assert_eq!(deleted.0, StatusCode::OK, "{}", deleted.1);

    // The note and its asset directory are moved into the vault's `.trash`.
    assert!(!vault
        .join("Taxonomy/Anime/Steins;Gate 0 (Anime).md")
        .exists());
    assert!(vault.join(".trash/Steins;Gate 0 (Anime).md").exists());
    assert!(!vault
        .join("Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
    assert!(vault
        .join(".trash/Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)")
        .exists());
}

#[tokio::test]
async fn delete_rechecks_revision_against_fresh_file() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    let entity_path = vault.join("Taxonomy/Anime/Show.md");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "filename": { "titleLanguage": "en" },
                "fields": [{
                    "field": "title",
                    "fieldType": "title",
                    "titleLanguage": "en"
                }]
            }]
        }),
    );
    write_file(&entity_path, "---\ntitle: Show\n---\nOriginal\n");
    let app = cached_inline_router(&vault, true, true);

    // Prime the long-lived resident snapshot, then change the file without
    // notifying the core, as Files/iCloud/an external editor would.
    let revision = entity_revision(&app, "anime:Show").await;
    let externally_edited = "---\ntitle: Show\n---\nEdited elsewhere\n";
    write_file(&entity_path, externally_edited);

    let deleted = request_json(
        &app,
        Method::DELETE,
        "/api/entities/anime%3AShow",
        Some(json!({ "revision": revision })),
    )
    .await;
    assert_eq!(deleted.0, StatusCode::CONFLICT, "{}", deleted.1);
    assert_eq!(fs::read_to_string(&entity_path).unwrap(), externally_edited);
    assert!(!vault.join(".trash/Show.md").exists());
}

#[tokio::test]
async fn broken_asset_cleanup_queue_flags_missing_files() {
    let (app, _temp, _vault) = asset_test_app(true, |vault| {
        // References a local cover that does not exist on disk.
        write_file(
            &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
            "---\ntitle: Steins;Gate 0\ncover_url: Assets/Taxonomy/Anime/Steins;Gate 0 (Anime)/cover_url.png\n---\nBody\n",
        );
    });

    let cleanup = request_json(&app, Method::GET, "/api/cleanup-queues", None).await;
    assert_eq!(cleanup.0, StatusCode::OK);
    let broken = cleanup.1["brokenAssets"].as_array().unwrap();
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0]["title"], "Steins;Gate 0");
    assert!(cleanup.1["queues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|queue| queue["id"] == "broken-asset"));
}
