use super::error::{ApiError, ApiResult};
use super::state::AppState;
use crate::contract::PathSuggestionsResponse;
use crate::library::compare_string;
use anyhow::Result;
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, JsonSchema)]
pub(crate) struct PathSuggestionsQuery {
    path: Option<String>,
    base: Option<String>,
}

pub(crate) async fn path_suggestions(
    State(state): State<AppState>,
    Query(query): Query<PathSuggestionsQuery>,
) -> ApiResult<PathSuggestionsResponse> {
    if !state.options.settings_writable {
        return Err(ApiError::forbidden("Path suggestions are disabled"));
    }
    let suggestions = suggest_directories(query.path.as_deref(), query.base.as_deref()).await?;
    Ok(Json(PathSuggestionsResponse { suggestions }))
}

async fn suggest_directories(path: Option<&str>, base: Option<&str>) -> Result<Vec<String>> {
    let input = path.unwrap_or_default().trim();
    let base_path = base.map(str::trim).filter(|value| !value.is_empty());
    let resolved_input = resolve_suggestion_input(input, base_path);
    let (search_dir, typed_prefix) = suggestion_search_dir(&resolved_input);
    let mut entries = match tokio::fs::read_dir(&search_dir).await {
        Ok(entries) => entries,
        Err(_) => return Ok(Vec::new()),
    };

    let mut suggestions = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        // Omit hidden (dotfile) directories such as `.git`/`.obsidian` unless the
        // user explicitly typed a leading dot to navigate into one.
        if name.starts_with('.') && !typed_prefix.starts_with('.') {
            continue;
        }
        if !typed_prefix.is_empty() && !name.to_lowercase().starts_with(&typed_prefix) {
            continue;
        }
        let path = entry.path();
        suggestions.push(format_suggestion_path(&path, base_path));
        if suggestions.len() >= 20 {
            break;
        }
    }
    suggestions.sort_by(|a, b| compare_string(a, b));
    Ok(suggestions)
}

fn resolve_suggestion_input(input: &str, base: Option<&str>) -> PathBuf {
    let expanded = expand_home(input);
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        return path;
    }
    base.map(PathBuf::from).unwrap_or_default().join(path)
}

fn suggestion_search_dir(path: &Path) -> (PathBuf, String) {
    if path.is_dir() {
        return (path.to_path_buf(), String::new());
    }
    let prefix = path
        .file_name()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    (parent, prefix)
}

fn format_suggestion_path(path: &Path, base: Option<&str>) -> String {
    if let Some(base) = base {
        let base_path = Path::new(base);
        if let Ok(relative) = path.strip_prefix(base_path) {
            return relative.to_string_lossy().to_string();
        }
    }
    path.to_string_lossy().to_string()
}

fn expand_home(input: &str) -> String {
    if input == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| input.to_string());
    }
    if let Some(rest) = input.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return Path::new(&home).join(rest).to_string_lossy().to_string();
        }
    }
    input.to_string()
}
