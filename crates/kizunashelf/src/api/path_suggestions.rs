use super::error::{ApiError, ApiResult};
use super::state::AppState;
use crate::contract::PathSuggestionsResponse;
use crate::library::{compare_string, VAULT_APP_DIR_NAME};
use crate::vfs::{normalize_relative, Vfs};
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;

/// Maximum directory suggestions returned per request.
const MAX_SUGGESTIONS: usize = 20;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct PathSuggestionsQuery {
    /// A path prefix being typed, relative to `base` (e.g. `An`). Suggestions are
    /// directories inside the vault only.
    path: Option<String>,
    /// A vault-relative directory the suggestions are rooted at and returned
    /// relative to — e.g. the taxonomy root for a type's folder path, which is
    /// stored relative to it. Defaults to the vault root.
    base: Option<String>,
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
    let app = state.app_config();
    let vfs = state.vault_vfs(&app.vault_root);
    let suggestions = suggest_directories(
        vfs.as_ref(),
        query.base.as_deref().unwrap_or_default(),
        query.path.as_deref().unwrap_or_default(),
    )
    .await;
    Ok(Json(PathSuggestionsResponse { suggestions }))
}

/// Lists directories under `<base>/<parent-of-typed>` matching the typed prefix,
/// returning paths **relative to `base`** (the coordinate system the field stores
/// its value in — vault-relative for the taxonomy/asset roots, taxonomy-relative
/// for a type's folder path). Never escapes the vault: both `base` and the typed
/// parent run through [`normalize_relative`] (rejecting `..`/absolute/drive
/// prefixes) and all I/O is through the `Vfs`.
async fn suggest_directories(vfs: &dyn Vfs, base: &str, input: &str) -> Vec<String> {
    let Ok(base) = normalize_relative(base) else {
        return Vec::new();
    };
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
    let list_dir = join_relative(&base, &parent);
    let typed_lower = typed_prefix.to_lowercase();

    let Ok(entries) = vfs.read_dir(&list_dir).await else {
        return Vec::new();
    };
    let mut suggestions: Vec<String> = entries
        .into_iter()
        .filter(|entry| entry.is_dir)
        .filter(|entry| {
            // Hide dot-directories (`.git`, `.obsidian`, `.trash`) unless the user
            // explicitly typed a leading dot to navigate into one.
            if entry.name.starts_with('.') && !typed_prefix.starts_with('.') {
                return false;
            }
            // Hide the app folder (`KizunaShelf`, which holds the config) from the
            // default listing so users don't nest entity collections inside it; it
            // still appears once the user starts typing a matching prefix.
            if typed_lower.is_empty() && entry.name.eq_ignore_ascii_case(VAULT_APP_DIR_NAME) {
                return false;
            }
            typed_lower.is_empty() || entry.name.to_lowercase().starts_with(&typed_lower)
        })
        // Returned relative to `base` (i.e. excluding the base prefix).
        .map(|entry| join_relative(&parent, &entry.name))
        .collect();
    suggestions.sort_by(|a, b| compare_string(a, b));
    suggestions.truncate(MAX_SUGGESTIONS);
    suggestions
}

/// Joins two already-normalized vault-relative path segments with `/`, handling
/// either side being empty (the vault root).
fn join_relative(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{a}/{b}"),
    }
}
