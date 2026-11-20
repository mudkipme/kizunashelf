use crate::support::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn external_search_routes_a_pasted_url_to_the_owning_provider() {
    // In the fixture, `anime` maps only bangumi and `games` maps only igdb.
    let server = TestServer::new();

    // An IGDB URL under `type=anime` routes to igdb — which anime doesn't map — so
    // nothing is searched: no results, and bangumi (configured + keyless) is never
    // queried, so no provider records an error. Proves the URL overrides the
    // configured provider set while still honoring the type filter.
    let igdb_under_anime = server
        .ok_json("/api/external/search?type=anime&q=https://www.igdb.com/games/celeste")
        .await;
    assert_eq!(igdb_under_anime["items"].as_array().unwrap().len(), 0);
    for provider in igdb_under_anime["providers"].as_array().unwrap() {
        assert!(
            provider["error"].is_null(),
            "no provider should be queried for a routed-away URL: {provider}"
        );
    }

    // A URL no known provider claims returns an empty list without any request.
    let unknown = server
        .ok_json("/api/external/search?type=anime&q=https://example.com/foo/123456")
        .await;
    assert_eq!(unknown["items"].as_array().unwrap().len(), 0);

    // A Bangumi URL under `type=games` routes to bangumi, which games doesn't map:
    // igdb (the configured provider) is ignored and nothing is searched.
    let bangumi_under_games = server
        .ok_json("/api/external/search?type=games&q=https://bgm.tv/subject/998877")
        .await;
    assert_eq!(bangumi_under_games["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn external_search_lists_providers_without_querying_network_for_empty_searches() {
    let server = TestServer::new();

    let providers = server.ok_json("/api/external/search?type=anime&q=").await;
    assert_eq!(providers["items"].as_array().unwrap().len(), 0);
    assert!(providers["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["id"] == "bangumi" && provider["enabled"] == true));
    assert!(providers["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["id"] == "igdb"));

    let unknown = server
        .json("/api/external/search?provider=missing&type=anime&q=Star")
        .await;
    assert_eq!(unknown.0, StatusCode::BAD_REQUEST);
    assert_eq!(unknown.1["error"], "Unknown external provider");

    // Cross-type search is gone: every search is scoped to one type, so `type=all`
    // resolves like any other unknown type id and an omitted type is rejected at
    // deserialization.
    let all_type = server.json("/api/external/search?type=all&q=").await;
    assert_eq!(all_type.0, StatusCode::BAD_REQUEST);
    assert_eq!(all_type.1["error"], "Unknown entity type");

    let missing_type = server.status("/api/external/search?q=").await;
    assert_eq!(missing_type, StatusCode::BAD_REQUEST);

    let unknown_type = server
        .json("/api/external/search?type=animation&q=Star")
        .await;
    assert_eq!(unknown_type.0, StatusCode::BAD_REQUEST);
    assert_eq!(unknown_type.1["error"], "Unknown entity type");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_quick_adds_of_one_candidate_create_one_entity() {
    // A double-click (or the share extension racing the app) must not mint a
    // disambiguated duplicate: the "already exists?" check and the write happen
    // under one lock, so every other request returns the first one's entity.
    let server = TestServer::new();
    let candidate = json!({
        "provider": "bangumi",
        "sourceId": "556677",
        "url": "https://bgm.tv/subject/556677",
        "title": "Raced Show",
        "titles": { "zh": "竞速番" },
        "metadata": {}
    });
    let adds: Vec<_> = (0..4)
        .map(|_| {
            let app = server.app.clone();
            let payload = json!({ "type": "anime", "candidate": candidate.clone() });
            tokio::spawn(async move {
                request_json(&app, Method::POST, "/api/external/quick-add", Some(payload)).await
            })
        })
        .collect();
    let mut ids = HashSet::new();
    let mut created = 0;
    for add in adds {
        let (status, body) = add.await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        ids.insert(body["entity"]["id"].as_str().unwrap().to_string());
        if body["alreadyExisted"] == false {
            created += 1;
        }
    }
    assert_eq!(created, 1, "exactly one request creates the entity");
    assert_eq!(
        ids.len(),
        1,
        "every request resolves to that entity: {ids:?}"
    );
}

#[tokio::test]
async fn quick_add_creates_entity_from_candidate_and_dedupes_on_second_add() {
    let server = TestServer::new();

    // A Bangumi candidate for the anime type. No `coverUrl` and the anime type has
    // no episodes section, so the whole flow stays offline. The zh title carries a
    // forbidden `/` to exercise full-width filename derivation.
    let candidate = json!({
        "provider": "bangumi",
        "sourceId": "998877",
        "url": "https://bgm.tv/subject/998877",
        "title": "Fate/stay night",
        "titles": { "zh": "命运之夜/UBW", "en": "Fate stay night" },
        "metadata": {}
    });

    let (status, created) = request_json(
        &server.app,
        Method::POST,
        "/api/external/quick-add",
        Some(json!({ "type": "anime", "candidate": candidate.clone() })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["alreadyExisted"], false);
    assert!(
        created["episodes"].is_null(),
        "anime has no episodes section"
    );
    assert_eq!(created["cover"].as_array().map(Vec::len).unwrap_or(0), 0);

    // The externalRef field is filled with the candidate URL, the title comes from
    // the zh title (the filename language), and the basename has the `/` replaced
    // by its full-width form.
    let entity = &created["entity"];
    assert_eq!(
        entity["frontmatter"]["bgm_url"],
        "https://bgm.tv/subject/998877"
    );
    assert_eq!(entity["frontmatter"]["title"], "命运之夜/UBW");
    assert_eq!(entity["basename"], "命运之夜／UBW");
    let entity_id = entity["id"].as_str().unwrap().to_string();

    // Re-adding the same candidate finds it via the stored bgm_url and returns the
    // existing entity instead of creating a duplicate.
    let (status, again) = request_json(
        &server.app,
        Method::POST,
        "/api/external/quick-add",
        Some(json!({ "type": "anime", "candidate": candidate })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["alreadyExisted"], true);
    assert_eq!(again["entity"]["id"], entity_id);
}

#[tokio::test]
async fn quick_add_defaults_status_to_first_option_for_requested_canonical() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir_all(vault.join("Taxonomy/Anime")).unwrap();
    // Anime maps the bangumi provider (so a candidate maps offline) and models a
    // status field whose planning canonical lists two options — the *first*
    // ("Backlog") is the write target the default should pick.
    let config = json!({
        "taxonomyRoot": "Taxonomy",
        "dailyNotes": { "paths": ["Daily Notes"], "dateFormat": "YYYY-MM-DD" },
        "types": [
            {
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "filename": { "titleLanguage": "en" },
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "en" },
                    { "field": "status", "fieldType": "enum",
                      "enumOptions": ["Backlog", "Planned", "Watching", "Completed"],
                      "enumRole": "status",
                      "statusValues": {
                          "planning": ["Backlog", "Planned"], "ongoing": ["Watching"],
                          "completed": ["Completed"]
                      } },
                    { "field": "bgm_url", "fieldType": "externalRef",
                      "externalRef": "bangumi", "externalTypes": ["anime"] }
                ]
            }
        ]
    });
    write_vault_config(&vault, &config);
    let app = inline_router(&vault, true, true);

    // No `coverUrl` and no episodes section, so the whole flow stays offline.
    let candidate = json!({
        "provider": "bangumi",
        "sourceId": "112233",
        "url": "https://bgm.tv/subject/112233",
        "title": "Some Show",
        "titles": { "en": "Some Show" },
        "metadata": {}
    });
    let (status, created) = request_json(
        &app,
        Method::POST,
        "/api/external/quick-add",
        Some(json!({ "type": "anime", "candidate": candidate.clone() })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    // The candidate mapped no status, so quick-add seeds the first planning option.
    assert_eq!(created["entity"]["frontmatter"]["status"], "Backlog");

    // A status-specific entry point can opt into the first ongoing write target.
    let mut ongoing_candidate = candidate;
    ongoing_candidate["sourceId"] = json!("445566");
    ongoing_candidate["url"] = json!("https://bgm.tv/subject/445566");
    ongoing_candidate["title"] = json!("Another Show");
    ongoing_candidate["titles"] = json!({ "en": "Another Show" });
    let (status, ongoing) = request_json(
        &app,
        Method::POST,
        "/api/external/quick-add",
        Some(json!({
            "type": "anime",
            "candidate": ongoing_candidate,
            "defaultStatus": "ongoing"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ongoing}");
    assert_eq!(ongoing["entity"]["frontmatter"]["status"], "Watching");
}

#[tokio::test]
async fn quick_add_is_forbidden_in_read_only_mode() {
    let server = TestServer::read_only();
    let (status, body) = request_json(
        &server.app,
        Method::POST,
        "/api/external/quick-add",
        Some(json!({
            "type": "anime",
            "candidate": {
                "provider": "bangumi",
                "sourceId": "1",
                "url": "https://bgm.tv/subject/1",
                "title": "Whatever",
                "titles": {},
                "metadata": {}
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

/// A vault whose anime type maps a bangumi text field and an external body
/// section, plus one entity with hand-written Summary content — the review/apply
/// fixture.
fn external_apply_fixture() -> (Router, PathBuf, TempDir) {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir_all(vault.join("Taxonomy/Anime")).unwrap();
    fs::write(
        vault.join("Taxonomy/Anime/Show.md"),
        r#"---
title: Show
---
## Summary

Old words.

## Keep

Kept.
"#,
    )
    .unwrap();
    let config = json!({
        "taxonomyRoot": "Taxonomy",
        "dailyNotes": { "paths": ["Daily Notes"], "dateFormat": "YYYY-MM-DD" },
        "types": [
            {
                "id": "anime",
                "label": "Anime",
                "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                    { "field": "note", "fieldType": "text",
                      "externalFields": [{ "source": "bangumi", "field": "note" }] },
                    { "field": "bgm_url", "fieldType": "externalRef", "externalRef": "bangumi" }
                ],
                "bodySections": [
                    { "heading": "Summary", "kind": "external",
                      "externalFields": [{ "source": "bangumi", "field": "summary" }] }
                ]
            }
        ]
    });
    write_vault_config(&vault, &config);
    let app = inline_router(&vault, true, true);
    (app, vault, temp)
}

fn bangumi_candidate() -> Value {
    json!({
        "provider": "bangumi",
        "sourceId": "5",
        "url": "https://bgm.tv/subject/5",
        "title": "Show",
        "titles": { "zh": "Show" },
        "metadata": { "note": "Great.", "summary": "From provider." }
    })
}

/// Exercises the thin-search → detail → Markdown path against TMDB. Run with
/// KIZUNASHELF_TMDB_API_KEY set; ordinary tests never require network access.
#[tokio::test]
#[ignore = "hits live TMDB API; needs KIZUNASHELF_TMDB_API_KEY"]
async fn external_match_preserves_chinese_summary_live() {
    for language in ["zh-Hans", "zh-Hant"] {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        write_vault_config(
            &vault,
            &json!({
                "taxonomyRoot": "Taxonomy",
                "types": [{
                    "id": "movie", "label": "Movie", "path": "Movies",
                    "fields": [
                        { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                        { "field": "source", "fieldType": "externalRef",
                          "externalRef": "tmdb", "externalTypes": ["movie"] }
                    ],
                    "bodySections": [{ "heading": "Summary", "kind": "external",
                        "externalFields": [{ "source": "tmdb", "field": "overview" }] }]
                }]
            }),
        );
        write_file(
            &vault.join("Taxonomy/Movies/Film.md"),
            "---\ntitle: Film\n---\n",
        );
        let app = inline_router(&vault, true, true);
        let (status, search) = request_json(
            &app,
            Method::GET,
            &format!(
                "/api/external/search?type=movie&provider=tmdb&q=Inception&language={language}"
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let candidate = search["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| &item["candidate"])
            .find(|candidate| {
                candidate["url"]
                    .as_str()
                    .is_some_and(|url| url.ends_with("/27205"))
            })
            .expect("TMDB should return Inception");
        assert_eq!(candidate["needsDetail"], true);
        let overview = candidate["metadata"]["overview"].as_str().unwrap();
        assert!(overview
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
        let path = "/api/entities/movie%3AFilm";
        let (status, review) = request_json(
            &app,
            Method::POST,
            &format!("{path}/external/review"),
            Some(json!({ "candidate": candidate, "language": language })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(review["sections"][0]["markdown"], overview);
        assert_ne!(review["candidate"]["needsDetail"], true);

        // Both the normal reviewed-candidate path and direct apply of a thin
        // candidate must preserve the language. Each apply gets a fresh revision.
        for incoming in [&review["candidate"], candidate] {
            let (_, detail) = request_json(&app, Method::GET, path, None).await;
            let (status, applied) = request_json(
                &app,
                Method::POST,
                &format!("{path}/external/apply"),
                Some(json!({
                    "revision": detail["entity"]["revision"],
                    "candidate": incoming, "language": language,
                    "fields": [], "sections": [review["sections"][0]["key"]]
                })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                applied["entity"]["body"].as_str().unwrap().trim(),
                format!("## Summary\n\n{overview}")
            );
        }
        let saved = fs::read_to_string(vault.join("Taxonomy/Movies/Film.md")).unwrap();
        assert!(saved.contains(overview));
    }
}

#[tokio::test]
async fn external_review_reports_defaults_against_the_entity() {
    let (app, _vault, _temp) = external_apply_fixture();
    let (status, review) = request_json(
        &app,
        Method::POST,
        &format!(
            "/api/entities/{}/external/review",
            urlencoding::encode("anime:Show")
        ),
        Some(json!({ "candidate": bangumi_candidate() })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert_eq!(review["entityType"], "anime");

    let field = |name: &str| {
        review["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["field"] == name)
            .unwrap_or_else(|| panic!("field {name} missing: {review}"))
            .clone()
    };
    // The candidate title equals the current one: a no-op, locked off.
    let title = field("title");
    assert_eq!(title["current"], "Show");
    assert_eq!(title["selected"], false);
    assert_eq!(title["locked"], true);
    // An empty target defaults on.
    let note = field("note");
    assert_eq!(note["value"], "Great.");
    assert!(note.get("current").is_none());
    assert_eq!(note["selected"], true);
    assert_eq!(note["locked"], false);
    // The external ref is the refresh anchor: locked on.
    let bgm = field("bgm_url");
    assert_eq!(bgm["value"], "https://bgm.tv/subject/5");
    assert_eq!(bgm["selected"], true);
    assert_eq!(bgm["locked"], true);

    // The Summary section exists with different content: off but editable, and
    // applying would replace it rather than append.
    let section = &review["sections"][0];
    assert_eq!(section["key"], "Summary:bangumi:summary");
    assert_eq!(section["markdown"], "From provider.");
    assert_eq!(section["selected"], false);
    assert_eq!(section["locked"], false);
    assert_eq!(section["exists"], true);
}

#[tokio::test]
async fn external_apply_merges_fields_and_splices_sections() {
    let (app, _vault, _temp) = external_apply_fixture();
    let path = format!("/api/entities/{}", urlencoding::encode("anime:Show"));
    let detail = request_json(&app, Method::GET, &path, None).await.1;
    let revision = detail["entity"]["revision"].as_str().unwrap();

    // Nothing selected is rejected before any write.
    let (status, body) = request_json(
        &app,
        Method::POST,
        &format!("{path}/external/apply"),
        Some(json!({
            "revision": revision,
            "candidate": bangumi_candidate(),
            "fields": [],
            "sections": []
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, applied) = request_json(
        &app,
        Method::POST,
        &format!("{path}/external/apply"),
        Some(json!({
            "revision": revision,
            "candidate": bangumi_candidate(),
            "fields": ["bgm_url", "note"],
            "sections": ["Summary:bangumi:summary"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    let frontmatter = &applied["entity"]["frontmatter"];
    assert_eq!(frontmatter["bgm_url"], "https://bgm.tv/subject/5");
    assert_eq!(frontmatter["note"], "Great.");
    // The unselected title stays untouched, the Summary content is replaced, and
    // the neighboring section survives byte-for-byte.
    assert_eq!(frontmatter["title"], "Show");
    assert_eq!(
        applied["entity"]["body"],
        "## Summary\n\nFrom provider.\n\n## Keep\n\nKept."
    );

    // The pre-apply revision is now stale: a second apply must 409.
    let (status, conflict) = request_json(
        &app,
        Method::POST,
        &format!("{path}/external/apply"),
        Some(json!({
            "revision": revision,
            "candidate": bangumi_candidate(),
            "fields": ["note"],
            "sections": []
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
}

#[tokio::test]
async fn external_apply_is_forbidden_in_read_only_mode() {
    let server = TestServer::read_only();
    let (status, body) = request_json(
        &server.app,
        Method::POST,
        &format!(
            "/api/entities/{}/external/apply",
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ),
        Some(json!({
            "revision": "whatever",
            "candidate": bangumi_candidate(),
            "fields": ["bgm_url"],
            "sections": []
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}
