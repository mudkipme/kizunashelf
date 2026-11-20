use crate::support::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn rating_mutations_preserve_other_data_and_restore_exact_legacy_values() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path();
    let mut config = json!({"taxonomyRoot":"Taxonomy", "types":[{
        "id":"books", "label":"Books", "path":"Books", "fields":[
            {"field":"感想", "fieldType":"rating", "displayName":"My score", "ratingMax":10},
            {"field":"critic", "fieldType":"rating"},
            {"field":"other", "fieldType":"text"}
        ]
    }]});
    write_vault_config(vault, &config);
    let file = vault.join("Taxonomy/Books/Example.md");
    write_file(&file, "---\n感想: '87.25'\ncritic: 0\nother: {nested: [one, two]}\nunknown: true\n---\n\nHandwritten **notes**.\n");
    let app = cached_inline_router(vault, true, true);
    let path = "/api/entities/books%3AExample";
    let rating_path = format!("{path}/rating");
    let original = request_json(&app, Method::GET, path, None).await.1["entity"].clone();
    assert_eq!(original["ratings"][0]["value"], 87.25);
    assert_eq!(original["ratings"][0]["max"], 10.0);
    assert_eq!(original["ratings"][1]["value"], 0.0);
    assert!(original["ratings"][1]["max"].is_null());
    let request = json!({"revision": original["revision"], "field":"感想", "max":10, "value":8.25});
    let (status, result) =
        request_json(&app, Method::POST, &rating_path, Some(request.clone())).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["entity"]["frontmatter"]["感想"], 8.25);
    assert_eq!(result["entity"]["body"], original["body"]);
    for key in ["critic", "other", "unknown"] {
        assert_eq!(
            result["entity"]["frontmatter"][key],
            original["frontmatter"][key]
        );
    }
    assert_eq!(result["previous"], json!({"present":true,"value":"87.25"}));
    assert_eq!(
        request_json(&app, Method::POST, &rating_path, Some(request))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (status, restored) = request_json(&app, Method::POST, &rating_path, Some(json!({
        "revision":result["entity"]["revision"], "field":"感想", "max":10, "restore":result["previous"]
    }))).await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["entity"]["frontmatter"], original["frontmatter"]);
    let revision = restored["entity"]["revision"].clone();
    for (field, value) in [
        ("感想", json!(11)),
        ("感想", json!(-1)),
        ("other", json!(5)),
    ] {
        assert_eq!(
            request_json(
                &app,
                Method::POST,
                &rating_path,
                Some(json!({"revision":revision, "field":field,"max":10,"value":value}))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let cleared = request_json(
        &app,
        Method::POST,
        &rating_path,
        Some(json!({"revision":revision, "field":"感想", "max":10, "value":null})),
    )
    .await;
    assert_eq!(cleared.0, StatusCode::OK, "{}", cleared.1);
    assert!(cleared.1["entity"]["frontmatter"].get("感想").is_none());
    assert!(cleared.1["entity"]["ratings"][0]["value"].is_null());
    let zero = request_json(&app, Method::POST, &rating_path, Some(json!({"revision":cleared.1["entity"]["revision"], "field":"感想", "max":10, "value":0}))).await;
    assert_eq!(zero.0, StatusCode::OK, "{}", zero.1);
    assert_eq!(zero.1["entity"]["ratings"][0]["value"], 0.0);
    assert_eq!(zero.1["previous"], json!({"present":false,"value":null}));
    // Undo cannot erase a later edit, even with a long-lived resident index.
    let undo = json!({"revision":zero.1["entity"]["revision"], "field":"感想", "max":10, "restore":zero.1["previous"]});
    let current = fs::read_to_string(&file).unwrap();
    write_file(&file, &format!("{current}\nAn external edit."));
    assert_eq!(
        request_json(&app, Method::POST, &rating_path, Some(undo))
            .await
            .0,
        StatusCode::CONFLICT
    );
    // Scale changes must also be checked from freshly read config.
    config["types"][0]["fields"][0]["ratingMax"] = json!(5);
    write_vault_config(vault, &config);
    assert_eq!(request_json(&app, Method::POST, &rating_path, Some(json!({"revision":zero.1["entity"]["revision"], "field":"感想", "max":10, "value":4}))).await.0, StatusCode::CONFLICT);
    let fresh = request_json(&app, Method::GET, path, None).await;
    assert_eq!(fresh.1["entity"]["ratings"][0]["max"], 5.0);
    let readonly = inline_router(vault, true, false);
    assert_eq!(
        request_json(
            &readonly,
            Method::POST,
            &rating_path,
            Some(json!({"revision":"unused", "field":"感想", "max":5,"value":4}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn activity_actions_share_the_log_effect_policy_without_requiring_a_journal() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path();
    write_vault_config(
        vault,
        &json!({"taxonomyRoot":"Taxonomy", "types":[
            {"id":"dates", "label":"Dates", "path":"Dates", "fields":[
                {"field":"始まり", "fieldType":"date", "dateRole":"started"},
                {"field":"fin", "fieldType":"date", "dateRole":"completed"}
            ]},
            {"id":"status", "label":"Status", "path":"Status", "fields":[
                {"field":"段階", "fieldType":"enum", "enumRole":"status", "statusValues":{
                    "planning":["Later"], "ongoing":["Doing"], "completed":["Done"], "dropped":["Stopped"]
                }}
            ]},
            {"id":"journal", "label":"Journal", "path":"Journal", "log":{}, "fields":[]},
            {"id":"plain", "label":"Plain", "path":"Plain", "fields":[
                {"field":"progress", "fieldType":"number"},
                {"field":"started", "fieldType":"text"}
            ]}
        ]}),
    );
    for (folder, content) in [
        ("Dates", "---\n---\nNotes"),
        ("Status", "---\n段階: Later\n---\nNotes"),
        ("Journal", "Notes"),
        ("Plain", "Notes"),
    ] {
        write_file(
            &vault.join(format!("Taxonomy/{folder}/Example.md")),
            content,
        );
    }
    let app = inline_router(vault, true, true);
    for (kind, expected) in [
        ("dates", json!(["started", "completed"])),
        ("status", json!(["started", "completed"])),
        ("journal", json!(["progress"])),
        ("plain", json!([])),
    ] {
        let detail = request_json(
            &app,
            Method::GET,
            &format!("/api/entities/{kind}%3AExample"),
            None,
        )
        .await
        .1;
        assert_eq!(detail["logActions"]["kinds"], expected, "{detail}");
        assert_eq!(detail["logActions"]["writesNote"], kind == "journal");
    }
    let read_only = inline_router(vault, true, false);
    let detail = request_json(
        &read_only,
        Method::GET,
        "/api/entities/dates%3AExample",
        None,
    )
    .await
    .1;
    assert_eq!(detail["logActions"]["kinds"], json!([]));
    let path = "/api/entities/status%3AExample";
    let detail = request_json(&app, Method::GET, path, None).await.1;
    let (status, started) = request_json(
        &app,
        Method::POST,
        &format!("{path}/log"),
        Some(json!({
            "kind":"started", "date":"2026-09-27", "revision":detail["entity"]["revision"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert!(started["notePath"].is_null());
    assert_eq!(
        started["entity"]["logActions"]["kinds"],
        json!(["completed"])
    );
    for status in ["Done", "Stopped", "Hand-edited custom value"] {
        write_file(
            &vault.join("Taxonomy/Status/Example.md"),
            &format!("---\n段階: {status}\n---\nNotes"),
        );
        let detail = request_json(&app, Method::GET, path, None).await.1;
        assert_eq!(detail["logActions"]["kinds"], json!([]), "{detail}");
    }
    let path = "/api/entities/dates%3AExample";
    let detail = request_json(&app, Method::GET, path, None).await.1;
    let (status, finished) = request_json(
        &app,
        Method::POST,
        &format!("{path}/log"),
        Some(json!({
            "kind":"completed", "date":"2026-09-20", "revision":detail["entity"]["revision"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{finished}");
    assert_eq!(
        finished["entity"]["entity"]["frontmatter"]["fin"],
        "2026-09-20"
    );
    assert!(finished["line"].is_null());
}
