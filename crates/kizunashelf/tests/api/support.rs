//! Shared fixtures for the API integration tests: router builders, the
//! `TestServer` fixture vault, request helpers, and the external crates every
//! test module uses (re-exported so modules can `use crate::support::*`).

#[allow(unused_imports)]
pub(crate) use axum::body;
#[allow(unused_imports)]
pub(crate) use axum::http::{header, Method, Request, StatusCode};
#[allow(unused_imports)]
pub(crate) use axum::Router;
#[allow(unused_imports)]
pub(crate) use kizunashelf::api::{router_native, ApiOptions};
#[allow(unused_imports)]
pub(crate) use kizunashelf::secrets::NativeSecretStore;
#[allow(unused_imports)]
pub(crate) use kizunashelf::types::AppConfig;
#[allow(unused_imports)]
use pretty_assertions::assert_eq;
#[allow(unused_imports)]
pub(crate) use serde_json::{json, Value};
#[allow(unused_imports)]
pub(crate) use std::collections::HashSet;
#[allow(unused_imports)]
pub(crate) use std::fs;
#[allow(unused_imports)]
pub(crate) use std::path::{Path, PathBuf};
#[allow(unused_imports)]
pub(crate) use std::sync::Arc;
#[allow(unused_imports)]
pub(crate) use std::time::Duration;
#[allow(unused_imports)]
pub(crate) use tempfile::TempDir;
#[allow(unused_imports)]
pub(crate) use tower::ServiceExt;

/// Builds the production-shape (inline) router for tests: the app config (vault
/// root + write mode) is passed inline like every real runtime (web/desktop/iOS),
/// and only the vault config lives on disk inside the vault.
pub(crate) fn inline_router(
    vault_root: &Path,
    settings_writable: bool,
    content_writable: bool,
) -> Router {
    build_inline_router(
        vault_root,
        settings_writable,
        content_writable,
        false,
        Duration::ZERO,
    )
}

/// Keeps the resident library snapshot alive so mutation tests can reproduce an
/// external edit landing behind the native runtimes' long cache TTL.
pub(crate) fn cached_inline_router(
    vault_root: &Path,
    settings_writable: bool,
    content_writable: bool,
) -> Router {
    build_inline_router(
        vault_root,
        settings_writable,
        content_writable,
        false,
        Duration::from_secs(60 * 60),
    )
}

pub(crate) fn build_inline_router(
    vault_root: &Path,
    settings_writable: bool,
    content_writable: bool,
    host_asset_ingest: bool,
    cache_ttl: Duration,
) -> Router {
    build_inline_router_with_identity(
        vault_root,
        settings_writable,
        content_writable,
        host_asset_ingest,
        cache_ttl,
        None,
    )
}

pub(crate) fn build_inline_router_with_identity(
    vault_root: &Path,
    settings_writable: bool,
    content_writable: bool,
    host_asset_ingest: bool,
    cache_ttl: Duration,
    identity: Option<String>,
) -> Router {
    let token_path = vault_root
        .parent()
        .map(|parent| parent.join(".tokens.json"))
        .unwrap_or_else(|| PathBuf::from(".tokens.json"));
    router_native(
        ApiOptions {
            cache_ttl,
            web_dist_path: None,
            settings_writable,
            content_writable,
            index_cache_dir: None,
            import_state_dir: None,
            index_cache_identity: identity,
            host_asset_ingest,
        },
        AppConfig {
            vault_root: vault_root.to_string_lossy().to_string(),
            content_writable: Some(content_writable),
        },
        Arc::new(NativeSecretStore::with_token_path(token_path)),
    )
}

pub(crate) struct TestServer {
    pub(crate) app: Router,
    pub(crate) vault: PathBuf,
    pub(crate) _temp: TempDir,
}

impl TestServer {
    pub(crate) fn new() -> Self {
        Self::build(true)
    }

    pub(crate) fn read_only() -> Self {
        Self::build(false)
    }

    pub(crate) fn build(content_writable: bool) -> Self {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        write_fixture_vault(&vault);
        let config = json!({
                "taxonomyRoot": "Taxonomy",
            "dailyNotes": {
                "paths": ["Daily Notes"],
                "dateFormat": "YYYY-MM-DD"
            },
            "home": {
                "sections": [
                    {
                        "id": "recent-anime",
                        "title": "Recent Anime",
                        "type": "anime",
                        "criteria": {
                            "conjunction": "all",
                            "rules": [{ "kind": "compare", "field": "status", "op": "eq", "value": "Watching" }]
                        },
                        "limit": 4,
                        "sort": "title",
                        "direction": "asc"
                    },
                    {
                        "id": "games",
                        "title": "Games",
                        "type": "games",
                        "criteria": {
                            "conjunction": "all",
                            "rules": [{ "kind": "compare", "field": "status", "op": "eq", "value": "Playing" }]
                        },
                        "limit": 4,
                        "sort": "title",
                        "direction": "asc"
                    },
                    {
                        "id": "completed-anime",
                        "title": "Completed Anime",
                        "type": "anime",
                        "criteria": {
                            "conjunction": "all",
                            "rules": [{ "kind": "compare", "field": "status", "op": "eq", "value": "Completed" }]
                        },
                        "limit": 4,
                        "sort": "title",
                        "direction": "asc"
                    }
                ]
            },
            "types": [
                {
                    "id": "anime",
                    "label": "Anime",
                    "icon": "📺",
                    "path": "Anime",
                    "filename": { "titleLanguage": "zh" },
                    "fields": [
                        { "field": "id", "fieldType": "id", "displayName": "ID" },
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                        { "field": "title_en", "fieldType": "title", "displayName": "Title (English)", "titleLanguage": "en" },
                        { "field": "title_original", "fieldType": "title", "displayName": "Title (Original)", "titleRole": "original" },
                        { "field": "cover_url", "fieldType": "image", "displayName": "Cover" },
                        { "field": "status", "fieldType": "enum", "displayName": "Status", "enumOptions": ["Backlog", "Watching", "Completed", "Paused", "Dropped"], "enumRole": "status", "statusValues": { "ongoing": ["Watching"], "paused": ["Paused"], "completed": ["Completed"], "dropped": ["Dropped"] } },
                        { "field": "season", "fieldType": "season", "displayName": "Season", "dateRole": "planning", "seasonLanguage": "zh" },
                        { "field": "complete_date", "fieldType": "date", "displayName": "Completed date", "dateRole": "completed" },
                        { "field": "favorite", "fieldType": "bool", "displayName": "Favorite" },
                        { "field": "bgm_url", "fieldType": "externalRef", "displayName": "BGM", "externalRef": "bangumi" },
                        { "field": "franchise", "fieldType": "relation", "displayName": "Franchise", "relationType": "franchise" },
                        { "field": "studio", "fieldType": "relation", "displayName": "Studio", "relationType": "studio" }
                    ],
                    "log": { "lineFormat": "- [[{title}]] {note} #Anime" }
                },
                {
                    "id": "games",
                    "label": "Games",
                    "path": "Games",
                    "filename": { "titleLanguage": "zh" },
                    "fields": [
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                        { "field": "title_en", "fieldType": "title", "displayName": "Title (English)", "titleLanguage": "en" },
                        { "field": "cover_url", "fieldType": "image", "displayName": "Cover" },
                        { "field": "status", "fieldType": "enum", "displayName": "Status", "enumOptions": ["Backlog", "Playing", "Completed", "Paused", "Dropped"] },
                        { "field": "favorite", "fieldType": "bool", "displayName": "Favorite" },
                        { "field": "release_date", "fieldType": "date", "displayName": "Release date", "dateRole": "planning" },
                        { "field": "igdb_url", "fieldType": "externalRef", "displayName": "IGDB", "externalRef": "igdb" },
                        { "field": "franchise", "fieldType": "relation", "displayName": "Franchise", "relationType": "franchise" },
                        { "field": "developer", "fieldType": "relation", "displayName": "Developer", "relationType": "developer" },
                        { "field": "genres", "fieldType": "enumList", "displayName": "Genres", "enumOptions": ["Adventure", "Strategy", "RPG"] }
                    ]
                },
                {
                    "id": "franchise",
                    "label": "Franchise",
                    "path": "Franchise",
                    "filename": { "titleLanguage": "zh" },
                    "fields": [
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                        { "field": "related", "fieldType": "relation", "displayName": "Related", "relationType": "related" }
                    ]
                },
                {
                    "id": "music",
                    "label": "Music",
                    "path": "Music",
                    "filename": {},
                    "fields": [
                        { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                        { "field": "release_date", "fieldType": "date", "displayName": "Release date", "dateRole": "planning" },
                        { "field": "musicbrainz_url", "fieldType": "externalRef", "displayName": "MusicBrainz", "externalRef": "musicbrainz" }
                    ]
                }
            ]
        });
        write_vault_config(&vault, &config);

        Self {
            app: inline_router(&vault, true, content_writable),
            vault,
            _temp: temp,
        }
    }

    pub(crate) async fn ok_json(&self, path: &str) -> Value {
        let (status, value) = self.json(path).await;
        assert_eq!(status, StatusCode::OK, "{path}: {value}");
        value
    }

    pub(crate) async fn json(&self, path: &str) -> (StatusCode, Value) {
        request_json(&self.app, Method::GET, path, None).await
    }

    /// Evaluates an unsaved smart-list definition — how criteria-shaped browsing
    /// is answered (the library browser is an unsaved smart list). `rules` are
    /// AND-ed; `scope` is a type id or `None` for the whole library.
    pub(crate) async fn preview(&self, scope: Option<&str>, rules: Value) -> Value {
        let mut body = json!({ "filters": { "conjunction": "all", "rules": rules } });
        if let Some(scope) = scope {
            body["scope"] = json!(scope);
        }
        let (status, value) = request_json(
            &self.app,
            Method::POST,
            "/api/smart-lists/preview",
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "preview: {value}");
        value
    }

    /// The response status alone, for rejections whose body isn't JSON (e.g. a
    /// missing required query parameter, rejected by the extractor).
    pub(crate) async fn status(&self, path: &str) -> StatusCode {
        let response = self
            .app
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
        response.status()
    }
}

/// Writes a vault config (the schema) to `<vault_root>/KizunaShelf/config.yaml`.
/// The vault root is owned by the runtime, so it is passed separately rather than
/// embedded in the config object.
pub(crate) fn write_vault_config(vault_root: &Path, config: &Value) {
    write_yaml_file(&vault_root.join("KizunaShelf/config.yaml"), config);
}

pub(crate) fn write_yaml_file(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_yaml::to_string(value).unwrap()).unwrap();
}

pub(crate) async fn request_json(
    app: &Router,
    method: Method,
    path: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let body = payload
        .map(|value| body::Body::from(serde_json::to_vec(&value).unwrap()))
        .unwrap_or_else(body::Body::empty);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "failed to parse JSON from {path}: {error}\n{}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, value)
}

// Titles, dates, relations, and provider hosts below are synthetic fixtures for
// search, filtering, and network isolation; they are not release metadata.
pub(crate) fn write_fixture_vault(vault: &Path) {
    write_file(
        &vault.join("Taxonomy/Anime/Steins;Gate 0 (Anime).md"),
        r#"---
title: Steins;Gate 0
title_en: Amadeus of Zero
title_original: シュタインズ・ゲート ゼロ
status: Watching
favorite: true
season: "2025"
complete_date: 2025-04-20
cover_url: https://img.example/star.jpg
bgm_url: https://bgm.example/star
franchise: "[[Steins;Gate]]"
studio: "[[Nova Studio]]"
---
## Summary
Steins;Gate 0 (Anime) follows Rintaro across the beta worldline.

It shares continuity with [[Robotics;Notes]].
"#,
    );
    write_file(
        &vault.join("Taxonomy/Games/Robotics;Notes.md"),
        r#"---
title: Robotics;Notes
title_en: Robot Club Diary
status: Playing
favorite: false
genres: [Adventure, Strategy]
release_date: 2025-04-05
igdb_url: https://igdb.example/moon
franchise: "[[Steins;Gate]]"
developer: "[[Orbit Dev]]"
---
Robotics;Notes is a robotics-club adventure that references [[Steins;Gate 0 (Anime)]].
"#,
    );
    write_file(
        &vault.join("Taxonomy/Franchise/Steins;Gate.md"),
        r#"---
title: Steins;Gate
related: "[[Robotics;Notes]]"
---
The shared setting for the fixture.
"#,
    );
    write_file(
        &vault.join("Taxonomy/Music/Opening Theme.md"),
        r#"---
title: Opening Theme
release_date: 2024-01-01
---
"#,
    );
    write_file(
        &vault.join("Daily Notes/2025-04-21.md"),
        r#"---
tags: [daily]
---
## Watched
Revisited [[Steins;Gate 0 (Anime)]] and [[Robotics;Notes|the quest]] after dinner.

```text
[[Opening Theme]] inside a code fence should not count.
```
"#,
    );
}

pub(crate) fn write_file(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

pub(crate) fn has_entity_title(items: &Value, title: &str) -> bool {
    items
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["title"] == title || item["entity"]["title"] == title)
}
