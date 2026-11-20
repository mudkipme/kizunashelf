use crate::support::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn calendar_endpoints_include_metadata_and_daily_notes_from_temp_vault() {
    let server = TestServer::new();

    let calendar = server.ok_json("/api/calendar?year=2025&month=4").await;
    let days = calendar["days"].as_array().unwrap();
    // Every source a date can come from lands on the same grid — there is no
    // filter that narrows it, so the month holds taxonomy dates and daily-note
    // mentions together.
    assert_eq!(
        days.iter()
            .filter(|day| !day["items"].as_array().unwrap().is_empty())
            .count(),
        3
    );
    let sum = |key: &str| -> i64 {
        days.iter()
            .map(|day| day["counts"][key].as_i64().unwrap_or_default())
            .sum()
    };
    assert_eq!(sum("total"), 4);
    assert_eq!(sum("taxonomy"), 2);
    assert_eq!(sum("dailyNotes"), 2);

    let april_21 = calendar["days"]
        .as_array()
        .unwrap()
        .iter()
        .find(|day| day["date"] == "2025-04-21")
        .unwrap();
    assert_eq!(april_21["counts"]["dailyNotes"], 2);
    assert!(has_entity_title(
        &april_21["items"],
        "シュタインズ・ゲート ゼロ"
    ));
    assert!(has_entity_title(&april_21["items"], "Robotics;Notes"));

    // The type filter is the one filter left, and it narrows every source at once.
    let anime_only = server
        .ok_json("/api/calendar?year=2025&month=4&type=anime")
        .await;
    assert_eq!(anime_only["filters"]["type"], "anime");
    assert!(anime_only["days"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|day| day["items"].as_array().unwrap())
        .all(|item| item["entity"]["type"] == "anime"));

    let games_only = server
        .ok_json("/api/calendar?year=2025&month=4&type=games")
        .await;
    assert_eq!(
        games_only["days"][20]["items"][0]["entries"][0]["notePath"],
        "Daily Notes/2025-04-21.md"
    );

    // The activity feed surfaces the same dated entities, grouped by (date, entity).
    let activity = server.ok_json("/api/activity").await;
    let items = activity["items"].as_array().unwrap();
    assert!(!items.is_empty());
    // Steins;Gate 0 (Anime)'s completed date stamp shows up among its activity items (it
    // appears in several — a daily-note mention and date stamps on other dates).
    assert!(items
        .iter()
        .filter(|item| item["entity"]["id"] == "anime:Steins;Gate 0 (Anime)")
        .flat_map(|item| item["entries"].as_array().unwrap())
        .any(|entry| entry["source"] == "taxonomy"
            && entry["dateField"] == "complete_date"
            && entry["role"] == "completed"));

    let game_activity = server.ok_json("/api/activity?type=games").await;
    assert!(game_activity["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["entity"]["type"] == "games"));

    let dates = server
        .ok_json(&format!(
            "/api/entities/{}/dates",
            urlencoding::encode("anime:Steins;Gate 0 (Anime)")
        ))
        .await;
    assert_eq!(dates["entityId"], "anime:Steins;Gate 0 (Anime)");
    assert_eq!(dates["totals"]["metadata"], 2);
    assert_eq!(dates["totals"]["dailyNotes"], 1);
    assert_eq!(dates["totals"]["snippets"], 1);
    assert_eq!(dates["metadata"][0]["date"], "2025-04-20");
    assert_eq!(dates["dailyNotes"][0]["date"], "2025-04-21");
    assert_eq!(dates["dailyNotes"][0]["snippets"][0]["heading"], "Watched");
}

#[tokio::test]
async fn upcoming_lists_future_dates_and_honors_client_today() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "games", "label": "Games", "path": "Games",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "titleLanguage": "zh" },
                    { "field": "cover_url", "fieldType": "image" },
                    { "field": "release_date", "fieldType": "date", "dateRole": "planning" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Games/PRAGMATA.md"),
        "---\ntitle: PRAGMATA\ncover_url: https://example.com/p.jpg\nrelease_date: 2030-06-01\n---\n",
    );
    let app = inline_router(&vault, true, true);

    // Before the release: it's upcoming, carrying the entity title, cover, and the
    // planning date field it came from. `today` is the client's local date.
    let ahead = request_json(
        &app,
        Method::GET,
        "/api/upcoming?today=2030-01-01&months=12",
        None,
    )
    .await;
    assert_eq!(ahead.0, StatusCode::OK, "{}", ahead.1);
    let items = ahead.1["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{}", ahead.1);
    assert_eq!(items[0]["date"], "2030-06-01");
    assert_eq!(items[0]["entity"]["title"], "PRAGMATA");
    assert_eq!(items[0]["entity"]["image"], "https://example.com/p.jpg");
    let entry = &items[0]["entries"][0];
    assert_eq!(entry["source"], "taxonomy");
    assert_eq!(entry["dateField"], "release_date");
    assert_eq!(entry["role"], "planning");

    // After that date, it's no longer upcoming — the client's `today` drives it.
    let after = request_json(
        &app,
        Method::GET,
        "/api/upcoming?today=2030-08-01&months=12",
        None,
    )
    .await;
    assert_eq!(after.1["items"].as_array().unwrap().len(), 0, "{}", after.1);
}

#[tokio::test]
async fn log_endpoint_writes_a_daily_note_line() {
    let server = TestServer::new();
    let path = format!(
        "/api/entities/{}/log",
        urlencoding::encode("anime:Steins;Gate 0 (Anime)")
    );

    // Dry run: previews the line + the completed-date it would stamp; writes nothing.
    let (status, preview) = request_json(
        &server.app,
        Method::POST,
        &format!("{path}?dryRun=true"),
        Some(json!({ "kind": "completed", "note": "rewatch done", "date": "2024-08-20" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["dryRun"], true);
    assert_eq!(preview["section"], "Log");
    let line = preview["line"].as_str().unwrap();
    assert!(line.contains("[[Steins;Gate 0 (Anime)]]"), "{line}");
    assert!(line.contains("rewatch done #Anime"), "{line}");
    assert_eq!(preview["willStampDate"]["field"], "complete_date");

    // Real write (kind=progress → daily-note line only, no entity mutation, so no
    // revision needed), then the same log again is idempotent.
    let (status, written) = request_json(
        &server.app,
        Method::POST,
        &path,
        Some(json!({ "kind": "progress", "note": "rewatch done", "date": "2024-08-20" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{written}");
    assert_eq!(written["dryRun"], false);
    assert_eq!(written["lineAlreadyPresent"], false);

    let (_, again) = request_json(
        &server.app,
        Method::POST,
        &path,
        Some(json!({ "kind": "progress", "note": "rewatch done", "date": "2024-08-20" })),
    )
    .await;
    assert_eq!(again["lineAlreadyPresent"], true, "{again}");
}

#[tokio::test]
async fn log_endpoint_applies_and_reverses_date_stamp() {
    let server = TestServer::new();
    let entity = urlencoding::encode("anime:Steins;Gate 0 (Anime)");
    let log = format!("/api/entities/{entity}/log");

    async fn revision_of(server: &TestServer, entity: &str) -> String {
        server.ok_json(&format!("/api/entities/{entity}")).await["entity"]["revision"]
            .as_str()
            .unwrap()
            .to_string()
    }

    // Complete it on a fixed date: stamps `complete_date`, writes a daily-note line.
    let revision = revision_of(&server, &entity).await;
    let (status, added) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "add", "kind": "completed", "date": "2024-08-20",
            "note": "fin", "revision": revision,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert_eq!(added["willStampDate"]["field"], "complete_date");
    assert_eq!(added["willStampDate"]["value"], "2024-08-20");
    // The returned entity detail carries the new stamp.
    assert_eq!(
        added["entity"]["entity"]["frontmatter"]["complete_date"], "2024-08-20",
        "{added}"
    );
    assert_eq!(added["lineAlreadyPresent"], false);
    let note_path = added["notePath"].as_str().unwrap().to_string();

    // Missing revision on a mutating log is rejected.
    let (status, _) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({ "op": "add", "kind": "completed", "date": "2024-08-20" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Remove: clears the stamp (it equals the log date) and removes the exact line.
    let revision = revision_of(&server, &entity).await;
    let (status, removed) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "remove", "kind": "completed", "date": "2024-08-20",
            "note": "fin", "revision": revision,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(removed["lineMatched"], true); // the exact line was found and removed
    assert!(
        removed["entity"]["entity"]["frontmatter"]["complete_date"].is_null(),
        "{removed}"
    );
    let _ = note_path;
}

#[tokio::test]
async fn log_endpoint_flips_status_monotonically_and_never_reverts_on_remove() {
    let server = TestServer::new();
    let entity = urlencoding::encode("anime:Steins;Gate 0 (Anime)");
    let log = format!("/api/entities/{entity}/log");

    async fn revision_of(server: &TestServer, entity: &str) -> String {
        server.ok_json(&format!("/api/entities/{entity}")).await["entity"]["revision"]
            .as_str()
            .unwrap()
            .to_string()
    }

    // Steins;Gate 0 (Anime) starts as `Watching` (ongoing). A dry-run `completed` log
    // previews the flip to the mapped write value `Completed` without writing.
    let (status, preview) = request_json(
        &server.app,
        Method::POST,
        &format!("{log}?dryRun=true"),
        Some(json!({ "op": "add", "kind": "completed", "date": "2024-08-20" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["willFlipStatus"]["field"], "status");
    assert_eq!(preview["willFlipStatus"]["value"], "Completed");
    assert_eq!(preview["willFlipStatus"]["canonical"], "completed");

    // Real completion flips the status field and returns the refreshed entity.
    let revision = revision_of(&server, &entity).await;
    let (status, done) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "add", "kind": "completed", "date": "2024-08-20", "revision": revision,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(
        done["entity"]["entity"]["frontmatter"]["status"], "Completed",
        "{done}"
    );

    // A `started` log now would demote completed→ongoing — it must not. No flip is
    // planned, and status stays `Completed`.
    let (status, started) = request_json(
        &server.app,
        Method::POST,
        &format!("{log}?dryRun=true"),
        Some(json!({ "op": "add", "kind": "started", "date": "2024-08-21" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert!(started["willFlipStatus"].is_null(), "{started}");

    // Removing the completion log clears the date stamp but leaves status alone —
    // a status flip has no safe inverse.
    let revision = revision_of(&server, &entity).await;
    let (status, removed) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "remove", "kind": "completed", "date": "2024-08-20", "revision": revision,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert!(removed["willFlipStatus"].is_null(), "{removed}");
    assert_eq!(
        removed["entity"]["entity"]["frontmatter"]["status"], "Completed",
        "status is not reverted by remove: {removed}"
    );
}

#[tokio::test]
async fn log_started_resumes_a_paused_entity() {
    let server = TestServer::new();
    let id = urlencoding::encode("anime:Steins;Gate 0 (Anime)");
    let entity_path = format!("/api/entities/{id}");
    let log = format!("{entity_path}/log");

    async fn revision_of(server: &TestServer, path: &str) -> String {
        server.ok_json(path).await["entity"]["revision"]
            .as_str()
            .unwrap()
            .to_string()
    }

    // Pause it first (Paused is off the progression chain).
    let revision = revision_of(&server, &entity_path).await;
    let (status, _) = request_json(
        &server.app,
        Method::POST,
        &entity_path,
        Some(json!({ "revision": revision, "frontmatter": { "status": "Paused" }, "body": "" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // A `started` log resumes it: paused promotes forward to the `ongoing` value.
    let revision = revision_of(&server, &entity_path).await;
    let (status, done) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "add", "kind": "started", "date": "2024-08-20", "revision": revision,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["willFlipStatus"]["canonical"], "ongoing", "{done}");
    assert_eq!(
        done["entity"]["entity"]["frontmatter"]["status"], "Watching",
        "{done}"
    );
}

#[tokio::test]
async fn log_conflict_leaves_no_partial_write() {
    let server = TestServer::new();
    let entity = urlencoding::encode("anime:Steins;Gate 0 (Anime)");
    let log = format!("/api/entities/{entity}/log");

    // A `completed` log stamps a frontmatter date *and* writes a daily-note line —
    // two files that can't be committed atomically. A stale revision must fail with
    // neither applied: the preflight rejects it before the note is touched, so the
    // request is all-or-nothing rather than leaving a half-written state.
    let (status, _) = request_json(
        &server.app,
        Method::POST,
        &log,
        Some(json!({
            "op": "add", "kind": "completed", "date": "2029-03-14",
            "note": "fin", "revision": "stale-revision",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // The daily note was never written: a dry-run still reports the note as new and
    // the line as not-yet-present — both would be false had the failed attempt
    // written anything.
    let (status, preview) = request_json(
        &server.app,
        Method::POST,
        &format!("{log}?dryRun=true"),
        Some(json!({ "op": "add", "kind": "completed", "date": "2029-03-14", "note": "fin" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["noteWillBeCreated"], true, "{preview}");
    assert_eq!(preview["lineAlreadyPresent"], false, "{preview}");
}
