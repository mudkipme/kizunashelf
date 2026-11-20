use crate::support::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn tags_surface_on_summary_filter_and_vocabulary() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    // Tags are opt-in: `tags.field` enables the feature. A schema field sharing
    // the configured name (here a relation) must be IGNORED — the built-in tags
    // feature owns the name, so no relations are built from it.
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "tags": { "field": "tags" },
            "types": [{
                "id": "anime", "label": "Anime", "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                    { "field": "tags", "fieldType": "relation", "displayName": "Tags", "relationType": "tag" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Anime/Alpha.md"),
        "---\ntitle: Alpha\ntags: [action, rpg]\n---\nBody\n",
    );
    write_file(
        &vault.join("Taxonomy/Anime/Beta.md"),
        "---\ntitle: Beta\ntags: [rpg, drama]\n---\nBody\n",
    );
    write_file(
        &vault.join("Taxonomy/Anime/Gamma.md"),
        "---\ntitle: Gamma\n---\nBody\n",
    );
    let app = inline_router(&vault, true, true);

    // Tags surface on the entity summary (empty array when absent).
    let entities = request_json(&app, Method::GET, "/api/entities?type=anime", None).await;
    assert_eq!(entities.0, StatusCode::OK, "{}", entities.1);
    let items = entities.1["items"].as_array().unwrap();
    let alpha = items.iter().find(|e| e["title"] == "Alpha").unwrap();
    assert_eq!(alpha["tags"], json!(["action", "rpg"]));
    let gamma = items.iter().find(|e| e["title"] == "Gamma").unwrap();
    assert_eq!(gamma["tags"], json!([]));
    // The configured `tags` relation field was ignored — no relation built from it.
    assert_eq!(alpha["relationCount"], 0);

    // The vocabulary is the sorted union of every tag.
    let tags = request_json(&app, Method::GET, "/api/tags", None).await;
    assert_eq!(tags.0, StatusCode::OK, "{}", tags.1);
    assert_eq!(tags.1["tags"], json!(["action", "drama", "rpg"]));

    // Tags narrow a browse through the smart-list `hasTag` rule — the built-in
    // tags field is matched against the normalized tag list, not frontmatter.
    let titles = async |rules: Value| -> Vec<String> {
        let (status, value) = request_json(
            &app,
            Method::POST,
            "/api/smart-lists/preview",
            Some(json!({
                "scope": "anime",
                "filters": { "conjunction": "all", "rules": rules },
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{value}");
        let mut titles: Vec<String> = value["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entity| entity["title"].as_str().unwrap().to_string())
            .collect();
        titles.sort();
        titles
    };

    assert_eq!(
        titles(json!([{ "kind": "hasTag", "values": ["action"] }])).await,
        ["Alpha"]
    );
    // ANY/OR semantics within one rule: action OR drama → Alpha + Beta.
    assert_eq!(
        titles(json!([{ "kind": "hasTag", "values": ["action", "drama"] }])).await,
        ["Alpha", "Beta"]
    );
}

#[tokio::test]
async fn tags_are_disabled_without_a_configured_tags_field() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    // No `tags` block: the opt-in feature is off. A schema field named `tags`
    // is then an ordinary schema field — here a relation that must be honored.
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "types": [{
                "id": "anime", "label": "Anime", "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" },
                    { "field": "tags", "fieldType": "relation", "displayName": "Tags", "relationType": "tag" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Anime/Alpha.md"),
        "---\ntitle: Alpha\ntags: [Beta]\n---\nBody\n",
    );
    write_file(
        &vault.join("Taxonomy/Anime/Beta.md"),
        "---\ntitle: Beta\n---\nBody\n",
    );
    let app = inline_router(&vault, true, true);

    // The config carries no tags field — the signal for clients to hide tag UI.
    let config = request_json(&app, Method::GET, "/api/config", None).await;
    assert_eq!(config.0, StatusCode::OK, "{}", config.1);
    assert_eq!(config.1["tagsField"], serde_json::Value::Null);

    // No tags are derived, even from a frontmatter key literally named `tags`...
    let entities = request_json(&app, Method::GET, "/api/entities?type=anime", None).await;
    assert_eq!(entities.0, StatusCode::OK, "{}", entities.1);
    let items = entities.1["items"].as_array().unwrap();
    let alpha = items.iter().find(|e| e["title"] == "Alpha").unwrap();
    assert_eq!(alpha["tags"], json!([]));
    // ...and the schema relation field named `tags` is a real relation source.
    assert_eq!(alpha["relationCount"], 1);

    // The vocabulary is empty.
    let tags = request_json(&app, Method::GET, "/api/tags", None).await;
    assert_eq!(tags.0, StatusCode::OK, "{}", tags.1);
    assert_eq!(tags.1["tags"], json!([]));
}

#[tokio::test]
async fn tags_field_name_is_configurable() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    // Rename the built-in tags field to `labels` at the vault level.
    write_vault_config(
        &vault,
        &json!({
            "taxonomyRoot": "Taxonomy",
            "tags": { "field": "labels" },
            "types": [{
                "id": "anime", "label": "Anime", "path": "Anime",
                "filename": { "titleLanguage": "zh" },
                "fields": [
                    { "field": "title", "fieldType": "title", "displayName": "Title", "titleLanguage": "zh" }
                ]
            }]
        }),
    );
    write_file(
        &vault.join("Taxonomy/Anime/Alpha.md"),
        "---\ntitle: Alpha\nlabels: [action, rpg]\ntags: ignored\n---\nBody\n",
    );
    let app = inline_router(&vault, true, true);

    // The config surfaces the resolved name for clients.
    let config = request_json(&app, Method::GET, "/api/config", None).await;
    assert_eq!(config.0, StatusCode::OK, "{}", config.1);
    assert_eq!(config.1["tagsField"], "labels");

    // Tags are derived from `labels`, not the default `tags` key.
    let entities = request_json(&app, Method::GET, "/api/entities?type=anime", None).await;
    let alpha = entities.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["title"] == "Alpha")
        .unwrap();
    assert_eq!(alpha["tags"], json!(["action", "rpg"]));

    // The vocabulary + the `hasTag` rule use the configured field too.
    let tags = request_json(&app, Method::GET, "/api/tags", None).await;
    assert_eq!(tags.1["tags"], json!(["action", "rpg"]));
    let filtered = request_json(
        &app,
        Method::POST,
        "/api/smart-lists/preview",
        Some(json!({
            "scope": "anime",
            "filters": {
                "conjunction": "all",
                "rules": [{ "kind": "hasTag", "values": ["rpg"] }],
            },
        })),
    )
    .await;
    assert_eq!(filtered.0, StatusCode::OK, "{}", filtered.1);
    let titles: Vec<_> = filtered.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Alpha"]);
}
