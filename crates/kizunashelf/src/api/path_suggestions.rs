use super::error::{ApiError, ApiResult};
use super::state::AppState;
use crate::contract::PathSuggestionsResponse;
use crate::library::compare_string;
use crate::vfs::{normalize_relative, Vfs};
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;

/// Maximum directory suggestions returned per request.
const MAX_SUGGESTIONS: usize = 20;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct PathSuggestionsQuery {
    /// A vault-relative path prefix being typed (e.g. `Taxonomy/An`). Suggestions
    /// are directories inside the vault only.
    path: Option<String>,
}

pub(crate) async fn path_suggestions(
    State(state): State<AppState>,
    Query(query): Query<PathSuggestionsQuery>,
) -> ApiResult<PathSuggestionsResponse> {
    if !state.options.settings_writable {
        return Err(ApiError::forbidden("Path suggestions are disabled"));
    }
    // Resolve the vault filesystem from the app config alone (no vault config /
    // full library needed, so this also works during onboarding before a schema
    // exists). All listing goes through the VFS, so it is confined to the vault
    // and works on hosts with no real filesystem at the vault path (iOS).
    let app = state.app_config().await?;
    let vfs = state.vault_vfs(&app.vault_root);
    let suggestions =
        suggest_directories(vfs.as_ref(), query.path.as_deref().unwrap_or_default()).await;
    Ok(Json(PathSuggestionsResponse { suggestions }))
}

/// Lists directories under the vault matching a typed, vault-relative prefix.
/// Returns vault-relative paths. Never escapes the vault: the parent is run
/// through [`normalize_relative`] (rejecting `..`/absolute/drive prefixes) and all
/// I/O is through the `Vfs`.
async fn suggest_directories(vfs: &dyn Vfs, input: &str) -> Vec<String> {
    let input = input.trim();
    // Split into the directory to list and the partial name being typed. A
    // trailing slash means "list this directory" (empty typed prefix).
    let (parent_raw, typed_prefix) = match input.rsplit_once('/') {
        Some((parent, name)) => (parent, name),
        None => ("", input),
    };
    let Ok(parent) = normalize_relative(parent_raw) else {
        return Vec::new();
    };
    let typed_lower = typed_prefix.to_lowercase();

    let Ok(entries) = vfs.read_dir(&parent).await else {
        return Vec::new();
    };
    let mut suggestions: Vec<String> = entries
        .into_iter()
        .filter(|entry| entry.is_dir)
        .filter(|entry| {
            // Hide dot-directories (`.git`, `.obsidian`, `.kizunashelf`, `.trash`)
            // unless the user explicitly typed a leading dot to navigate into one.
            if entry.name.starts_with('.') && !typed_prefix.starts_with('.') {
                return false;
            }
            typed_lower.is_empty() || entry.name.to_lowercase().starts_with(&typed_lower)
        })
        .map(|entry| {
            if parent.is_empty() {
                entry.name
            } else {
                format!("{parent}/{}", entry.name)
            }
        })
        .collect();
    suggestions.sort_by(|a, b| compare_string(a, b));
    suggestions.truncate(MAX_SUGGESTIONS);
    suggestions
}
