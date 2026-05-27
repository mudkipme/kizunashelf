use anyhow::Result;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("packages/api-contract/openapi/kizunashelf.openapi.json"));
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let document = kizunashelf::api::openapi();
    tokio::fs::write(&output, serde_json::to_string_pretty(&document)? + "\n").await?;
    Ok(())
}
