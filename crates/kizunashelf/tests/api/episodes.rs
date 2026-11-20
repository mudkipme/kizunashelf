use crate::support::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn episodes_detail_progress_update_and_revision_guard() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "anime", "label": "Anime", "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "bodySections": [
                    { "heading": "Episodes", "kind": "episodes", "tracking": "checklist" }
                ],
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Anime/Show.md"),
        "---\ntitle: Show\n---\n## Episodes\n### Season 1\n- [x] 1 · Pilot\n- [ ] 12.5 · Recap\n",
    );
    let app = inline_router(&vault, true, true);

    // Resident progress reaches the list view.
    let list = request_json(&app, Method::GET, "/api/entities?type=anime", None).await;
    let item = &list.1["items"][0];
    assert_eq!(item["episodeProgress"], json!({ "watched": 1, "total": 2 }));

    // Detail parses the grouped episodes (including the 12.5 special).
    let detail = request_json(&app, Method::GET, "/api/entities/anime%3AShow", None).await;
    assert_eq!(detail.0, StatusCode::OK, "{}", detail.1);
    let episodes = &detail.1["episodes"];
    assert_eq!(episodes["total"], 2);
    assert_eq!(episodes["watched"], 1);
    assert_eq!(episodes["groups"][0]["label"], "Season 1");
    assert_eq!(episodes["groups"][0]["items"][1]["key"], "12.5");
    assert_eq!(episodes["groups"][0]["items"][1]["title"], "Recap");
    let revision = detail.1["entity"]["revision"].as_str().unwrap().to_string();

    // Check the recap (the second item in Season 1) through `/episodes/watch` —
    // the dedicated episode write path, independent of daily-note logging. The
    // completion date is the client's local date.
    let body = json!({
        "revision": revision, "group": "Season 1", "key": "12.5", "index": 1,
        "watched": true, "date": "2024-08-20",
    });
    let updated = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/episodes/watch",
        Some(body.clone()),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);
    assert_eq!(updated.1["episodes"]["watched"], 2);
    // The `✅` carries the client-supplied date.
    assert_eq!(
        updated.1["episodes"]["groups"][0]["items"][1]["done"],
        "2024-08-20"
    );

    // The stale revision is now rejected.
    let stale = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/episodes/watch",
        Some(body),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT, "{}", stale.1);
}

#[tokio::test]
async fn body_tasks_toggle_stamps_done_date_and_skips_the_episodes_section() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "anime", "label": "Anime", "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "bodySections": [
                    { "heading": "Episodes", "kind": "episodes", "tracking": "checklist" }
                ],
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" }
                ]
            }]
        }),
    );
    // Notes on both sides of the episodes section, and a blank line after the
    // frontmatter: the read path trims that leading whitespace off `entity.body`
    // while the write path keeps it, so a line the client counted only resolves
    // if the two coordinate spaces are reconciled.
    write_file(
        &vault.join("Taxonomy/Anime/Show.md"),
        "---\ntitle: Show\n---\n\n## Notes\n\n- [ ] rewatch with subs\n\n## Episodes\n\n- [ ] 1 · Pilot\n\n## More\n\n- [ ] read the manga\n",
    );
    let app = inline_router(&vault, true, true);

    let detail = request_json(&app, Method::GET, "/api/entities/anime%3AShow", None).await;
    assert_eq!(detail.0, StatusCode::OK, "{}", detail.1);
    // The notes view drops the episodes section and closes the gap, so the two
    // checkboxes the client renders sit on its lines 3 and 7 — the coordinate
    // space the toggle addresses.
    let notes = detail.1["notesBody"].as_str().unwrap().to_string();
    assert_eq!(
        notes,
        "## Notes\n\n- [ ] rewatch with subs\n\n## More\n\n- [ ] read the manga"
    );
    let revision = detail.1["entity"]["revision"].as_str().unwrap().to_string();

    let body = json!({
        "revision": revision, "line": 7, "text": "- [ ] read the manga",
        "done": true, "date": "2024-08-20",
    });
    let updated = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/tasks/toggle",
        Some(body.clone()),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK, "{}", updated.1);
    assert_eq!(
        updated.1["notesBody"],
        "## Notes\n\n- [ ] rewatch with subs\n\n## More\n\n- [x] read the manga ✅ 2024-08-20"
    );
    // Only that line changed: the episodes checkbox between the two notes
    // sections is untouched, and so is the blank line after the frontmatter.
    assert_eq!(updated.1["episodes"]["watched"], 0);
    assert_eq!(updated.1["episodes"]["total"], 1);
    let raw = std::fs::read_to_string(vault.join("Taxonomy/Anime/Show.md")).unwrap();
    assert_eq!(
        raw,
        "---\ntitle: Show\n---\n\n## Notes\n\n- [ ] rewatch with subs\n\n## Episodes\n\n- [ ] 1 · Pilot\n\n## More\n\n- [x] read the manga ✅ 2024-08-20\n"
    );

    // The stale revision is now rejected.
    let stale = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/tasks/toggle",
        Some(body),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT, "{}", stale.1);

    // A locator whose text no longer matches is refused rather than toggling a
    // neighbour.
    let revision = updated.1["entity"]["revision"]
        .as_str()
        .unwrap()
        .to_string();
    let drifted = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/tasks/toggle",
        Some(json!({
            "revision": revision, "line": 3, "text": "- [ ] read the manga",
            "done": true, "date": "2024-08-20",
        })),
    )
    .await;
    assert_eq!(drifted.0, StatusCode::NOT_FOUND, "{}", drifted.1);
}

#[tokio::test]
async fn episodes_import_merges_and_fetch_lists_sources() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "anime", "label": "Anime", "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "bodySections": [
                    { "heading": "Episodes", "kind": "episodes", "tracking": "checklist" }
                ],
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                    { "field": "igdb_url", "fieldType": "externalRef", "displayName": "IGDB", "externalRef": "igdb" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Anime/Show.md"),
        "---\ntitle: Show\nigdb_url: https://www.igdb.com/games/x\n---\n## Episodes\n- [x] 1 · Pilot\n- [ ] 2\n- [x] 99 · My Extra\n",
    );
    let app = inline_router(&vault, true, true);

    // Fetch lists only episode-capable sources — IGDB doesn't support episodes, so none.
    let fetched = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/episodes/fetch",
        Some(json!({})),
    )
    .await;
    assert_eq!(fetched.0, StatusCode::OK, "{}", fetched.1);
    assert_eq!(fetched.1["sources"], json!([]));
    assert_eq!(fetched.1["groups"], json!([]));

    let detail = request_json(&app, Method::GET, "/api/entities/anime%3AShow", None).await;
    let revision = detail.1["entity"]["revision"].as_str().unwrap().to_string();

    // Import (as a provider would supply): merge preserves watched + extras, fills only empty titles.
    let body = json!({
        "revision": revision,
        "groups": [{
            "label": "",
            "items": [
                { "key": "1", "title": "Pilot (provider)", "watched": false },
                { "key": "2", "title": "Journey", "watched": false },
                { "key": "3", "title": "Dawn", "watched": false }
            ]
        }]
    });
    let imported = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/episodes/import",
        Some(body.clone()),
    )
    .await;
    assert_eq!(imported.0, StatusCode::OK, "{}", imported.1);
    let group = &imported.1["episodes"]["groups"][0]["items"];
    assert_eq!(imported.1["episodes"]["total"], 4);
    assert_eq!(imported.1["episodes"]["watched"], 2); // ep1 + ep99 stay watched
    assert_eq!(
        group[0],
        json!({ "key": "1", "title": "Pilot", "watched": true })
    ); // title not overwritten
    assert_eq!(group[1]["title"], "Journey"); // empty title filled
    assert!(group
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["key"] == "99" && e["watched"] == true)); // extra kept
    assert!(group.as_array().unwrap().iter().any(|e| e["key"] == "3")); // new added

    // Stale revision rejected.
    let stale = request_json(
        &app,
        Method::POST,
        "/api/entities/anime%3AShow/episodes/import",
        Some(body),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT, "{}", stale.1);
}

/// Live, network-hitting smoke test for provider episode/track sync. Ignored by
/// default; run with real credentials exported from `.env`:
///
/// ```sh
/// set -a; . ./.env; set +a
/// cargo test -p kizunashelf --test api -- --ignored --nocapture provider_episode_sync_live
/// ```
///
/// TMDB and Discogs read their keys from `KIZUNASHELF_*` env vars (via the native
/// secret store); MusicBrainz is keyless.
#[tokio::test]
#[ignore = "hits live TMDB/Discogs/MusicBrainz APIs; needs credentials in env"]
async fn provider_episode_sync_live() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "media", "label": "Media", "path": "Media",
                "externalPriority": ["tmdb", "discogs", "musicbrainz", "myanimelist", "applepodcast", "comicvine"],
                "bodySections": [
                    { "heading": "Episodes", "kind": "episodes", "tracking": "checklist" }
                ],
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title" },
                    { "field": "tmdb_url", "fieldType": "externalRef", "displayName": "TMDB", "externalRef": "tmdb" },
                    { "field": "discogs_url", "fieldType": "externalRef", "displayName": "Discogs", "externalRef": "discogs" },
                    { "field": "musicbrainz_url", "fieldType": "externalRef", "displayName": "MusicBrainz", "externalRef": "musicbrainz" },
                    { "field": "mal_url", "fieldType": "externalRef", "displayName": "MyAnimeList", "externalRef": "myanimelist" },
                    { "field": "podcast_url", "fieldType": "externalRef", "displayName": "Apple Podcasts", "externalRef": "applepodcast" },
                    { "field": "comicvine_url", "fieldType": "externalRef", "displayName": "Comic Vine", "externalRef": "comicvine" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Media/Sample.md"),
        "---\ntitle: Sample\n\
         tmdb_url: https://www.themoviedb.org/tv/1399\n\
         discogs_url: https://www.discogs.com/release/249504\n\
         musicbrainz_url: https://musicbrainz.org/release/4b3d18cc-8937-36f4-8de0-481088be58e6\n\
         mal_url: https://myanimelist.net/anime/1\n\
         podcast_url: https://podcasts.apple.com/us/podcast/the-daily/id1200361736\n\
         comicvine_url: https://comicvine.gamespot.com/volume/4050-18166/\n\
         ---\n## Episodes\n",
    );
    let app = inline_router(&vault, true, true);

    // All three providers are linked and episode-capable, in priority order.
    let listed = request_json(
        &app,
        Method::POST,
        "/api/entities/media%3ASample/episodes/fetch",
        Some(json!({})),
    )
    .await;
    assert_eq!(listed.0, StatusCode::OK, "{}", listed.1);
    let providers: Vec<&str> = listed.1["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["provider"].as_str().unwrap())
        .collect();
    assert_eq!(
        providers,
        vec![
            "tmdb",
            "discogs",
            "musicbrainz",
            "myanimelist",
            "applepodcast",
            "comicvine"
        ]
    );

    let fetch = |provider: &'static str| {
        let app = app.clone();
        async move {
            let response = request_json(
                &app,
                Method::POST,
                "/api/entities/media%3ASample/episodes/fetch",
                Some(json!({ "provider": provider })),
            )
            .await;
            assert_eq!(response.0, StatusCode::OK, "{provider}: {}", response.1);
            response.1
        }
    };

    // TMDB — TV episodes grouped by season.
    let tmdb = fetch("tmdb").await;
    println!("TMDB groups: {:?}", group_summary(&tmdb["groups"]));
    let season1 = find_group(&tmdb["groups"], "Season 1");
    assert_eq!(season1[0]["key"], "1");
    assert_eq!(season1[0]["title"], "Winter Is Coming");
    assert!(
        tmdb["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| group["label"] == "Specials"),
        "expected a Specials group"
    );

    // Discogs — vinyl tracklist (positions A/B), one flat group.
    let discogs = fetch("discogs").await;
    println!("Discogs groups: {:?}", group_summary(&discogs["groups"]));
    let tracks = discogs["groups"][0]["items"].as_array().unwrap();
    assert_eq!(tracks[0]["key"], "A");
    assert!(tracks[0]["title"]
        .as_str()
        .unwrap()
        .contains("Never Gonna Give You Up"));

    // MusicBrainz — single-disc release, one flat group keyed by track number.
    let musicbrainz = fetch("musicbrainz").await;
    println!(
        "MusicBrainz groups: {:?}",
        group_summary(&musicbrainz["groups"])
    );
    let tracks = musicbrainz["groups"][0]["items"].as_array().unwrap();
    assert_eq!(musicbrainz["groups"][0]["label"], "");
    assert_eq!(tracks[0]["key"], "1");
    assert_eq!(tracks[0]["title"], "Airbag");

    // MyAnimeList — anime episodes (via keyless Jikan), one flat group keyed by number.
    let mal = fetch("myanimelist").await;
    println!("MyAnimeList groups: {:?}", group_summary(&mal["groups"]));
    let episodes = mal["groups"][0]["items"].as_array().unwrap();
    assert_eq!(mal["groups"][0]["label"], "");
    assert_eq!(episodes[0]["key"], "1");
    assert_eq!(episodes[0]["title"], "Asteroid Blues");

    // Apple Podcasts — one flat group, episodes numbered 1..N oldest-first. (Titles
    // change as the feed updates, so assert structure, not specific names.)
    let podcast = fetch("applepodcast").await;
    println!(
        "Apple Podcasts groups: {:?}",
        group_summary(&podcast["groups"])
    );
    let episodes = podcast["groups"][0]["items"].as_array().unwrap();
    assert_eq!(podcast["groups"][0]["label"], "");
    assert!(!episodes.is_empty(), "expected podcast episodes");
    for (index, episode) in episodes.iter().enumerate() {
        assert_eq!(episode["key"], (index + 1).to_string());
        assert!(!episode["title"].as_str().unwrap_or_default().is_empty());
    }

    // Comic Vine — a volume's issues as one flat list, keyed by issue number.
    let comicvine = fetch("comicvine").await;
    println!(
        "Comic Vine groups: {:?}",
        group_summary(&comicvine["groups"])
    );
    let issues = comicvine["groups"][0]["items"].as_array().unwrap();
    assert_eq!(comicvine["groups"][0]["label"], "");
    assert_eq!(issues[0]["key"], "1");
    assert_eq!(issues[0]["title"], "Days Gone Bye, Pt. 1");
}

/// Finds a fetched group by label and returns its items (panics if absent).
fn find_group<'a>(groups: &'a Value, label: &str) -> &'a Vec<Value> {
    groups
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["label"] == label)
        .unwrap_or_else(|| panic!("missing group {label:?}"))["items"]
        .as_array()
        .unwrap()
}

/// `[(label, item_count)]` for a fetched groups array — compact test output.
fn group_summary(groups: &Value) -> Vec<(String, usize)> {
    groups
        .as_array()
        .unwrap()
        .iter()
        .map(|group| {
            (
                group["label"].as_str().unwrap_or_default().to_string(),
                group["items"].as_array().map(Vec::len).unwrap_or(0),
            )
        })
        .collect()
}

// ---- Batch import ----------------------------------------------------------
