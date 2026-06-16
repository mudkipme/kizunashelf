use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `--collapse-nullable-refs` rewrites `anyOf: [X, {type: null}]` (schemars'
    // encoding of `Option<NamedType>` in OpenAPI 3.1) down to `X`. The field is
    // already non-required, so optionality is preserved. swift-openapi-generator
    // rejects the bare `{type: null}` branch and would otherwise drop the whole
    // property, so the iOS client is generated from a collapsed spec. The
    // canonical spec (consumed by the web/orval client) keeps the null branch.
    let collapse = args.iter().any(|arg| arg == "--collapse-nullable-refs");
    let output = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("packages/api-contract/openapi/kizunashelf.openapi.json"));
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let document = kizunashelf::api::openapi();
    let mut value = serde_json::to_value(&document)?;
    if collapse {
        collapse_nullable_anyof(&mut value);
    }
    tokio::fs::write(&output, serde_json::to_string_pretty(&value)? + "\n").await?;
    Ok(())
}

/// Recursively collapses `anyOf` schemas whose only "extra" branch is
/// `{"type": "null"}`: drops the null branch, and if a single schema remains,
/// merges it into the parent (so `{"anyOf": [{"$ref": X}, null]}` becomes
/// `{"$ref": X}`).
fn collapse_nullable_anyof(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for child in map.values_mut() {
                collapse_nullable_anyof(child);
            }
            let has_null = map
                .get("anyOf")
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().any(is_null_type_schema));
            if !has_null {
                return;
            }
            let non_null: Vec<Value> = map
                .remove("anyOf")
                .and_then(|value| match value {
                    Value::Array(items) => Some(items),
                    _ => None,
                })
                .unwrap_or_default()
                .into_iter()
                .filter(|item| !is_null_type_schema(item))
                .collect();
            match non_null.len() {
                // Only a single concrete schema remains: merge it in place.
                1 => {
                    if let Value::Object(inner) = non_null.into_iter().next().unwrap() {
                        for (key, val) in inner {
                            map.entry(key).or_insert(val);
                        }
                    }
                }
                // Either nothing or multiple remain: keep an `anyOf` without null.
                _ => {
                    map.insert("anyOf".to_string(), Value::Array(non_null));
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collapse_nullable_anyof(item);
            }
        }
        _ => {}
    }
}

fn is_null_type_schema(value: &Value) -> bool {
    matches!(value, Value::Object(map)
        if map.len() == 1 && map.get("type") == Some(&Value::String("null".to_string())))
}

#[cfg(test)]
mod tests {
    use super::collapse_nullable_anyof;
    use serde_json::json;

    #[test]
    fn collapses_nullable_ref_to_plain_ref() {
        let mut value = json!({
            "properties": {
                "titleRole": { "anyOf": [{ "$ref": "#/c/TitleRole" }, { "type": "null" }] }
            }
        });
        collapse_nullable_anyof(&mut value);
        assert_eq!(
            value["properties"]["titleRole"],
            json!({ "$ref": "#/c/TitleRole" })
        );
    }

    #[test]
    fn keeps_real_multi_branch_anyof_but_drops_null() {
        let mut value = json!({
            "anyOf": [{ "type": "string" }, { "type": "integer" }, { "type": "null" }]
        });
        collapse_nullable_anyof(&mut value);
        assert_eq!(
            value,
            json!({ "anyOf": [{ "type": "string" }, { "type": "integer" }] })
        );
    }

    #[test]
    fn leaves_non_nullable_anyof_untouched() {
        let original = json!({ "anyOf": [{ "$ref": "#/c/A" }, { "$ref": "#/c/B" }] });
        let mut value = original.clone();
        collapse_nullable_anyof(&mut value);
        assert_eq!(value, original);
    }
}
