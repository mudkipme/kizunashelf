use super::assets::entity_asset_dir;
use super::error::{ApiError, ApiResult};
use super::lists::list_file_paths;
use super::state::{get_library, require_content_writes, AppState};
use crate::contract::{
    CreateEntityRequest, DeleteEntityRequest, DeleteEntityResponse, EntityMutationResponse,
    RenameLinkUpdate, UpdateEntityRequest,
};
use crate::daily_notes::normalize_wikilink_target;
use crate::library::{
    file_revision, find_target, load_entity, normalize_full_target,
    normalized_entity_basename_index, parse_daily_note_source_id, rewrite_backlink_wikilinks,
    rewrite_self_wikilinks, rewrite_wikilinks_matching, serialize_markdown_document,
    split_markdown_document, MarkdownDocument,
};
use crate::types::{EntityTypeConfig, KizunaConfig, Library, RelationDirection};
use crate::vfs::Vfs;
use anyhow::Result;
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct EntityPath {
    pub(super) id: String,
}

pub(crate) async fn update_entity(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<UpdateEntityRequest>,
) -> ApiResult<EntityMutationResponse> {
    let library = require_content_writes(&state).await?;
    let _mutation = state.content_mutation_lock().await;
    let Some(entity) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    check_revision(&request.revision, &entity.revision)?;

    let vfs = state.vault_vfs(&library.config.vault_root);
    let source_rel = entity.summary.path.clone();
    let raw = vfs
        .read_to_string(&source_rel)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {source_rel}: {error}"))?;
    // Re-check the revision against the *freshly read* content, not just the
    // cached index. Otherwise an external edit landing between the cache snapshot
    // and this read would still match the client's revision and be silently
    // overwritten (TOCTOU).
    check_revision(&request.revision, &file_revision(&raw))?;
    let mut document = split_markdown_document(&raw);
    if let Some(frontmatter) = request.frontmatter {
        apply_frontmatter_patch(&mut document.frontmatter, frontmatter);
    }
    if let Some(draft) = request.frontmatter_draft {
        let type_config = type_config_or_err(&library.config, &entity.summary.entity_type)?;
        let patch =
            super::frontmatter_draft::draft_update_patch(draft, type_config, &document.frontmatter);
        apply_frontmatter_patch(&mut document.frontmatter, patch);
    }
    if let Some(body) = request.body {
        document.body = body;
    }
    let mut renamed_basename: Option<String> = None;
    let target_rel = if let Some(rename_to) = &request.rename_to {
        let basename = sanitize_basename(rename_to)
            .map_err(|error| ApiError::bad_request(&error.to_string()))?;
        let target = match source_rel.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/{basename}.md"),
            None => format!("{basename}.md"),
        };
        if target != source_rel
            && vfs
                .exists(&target)
                .await
                .map_err(|error| anyhow::anyhow!("failed to check target entity path: {error}"))?
        {
            return Err(ApiError::conflict("Target entity file already exists"));
        }
        // Move the entity's asset directory alongside the rename and rewrite any
        // frontmatter image paths that lived inside it (best-effort, non-fatal).
        if target != source_rel {
            move_entity_assets(
                vfs.as_ref(),
                library.config.resolved_asset_root(),
                &source_rel,
                &target,
                &mut document.frontmatter,
            )
            .await;
            renamed_basename = Some(basename);
        }
        target
    } else {
        source_rel.clone()
    };

    let raw = serialize_markdown_document(&document.frontmatter, &document.body);
    write_entity_raw(vfs.as_ref(), &target_rel, &raw).await?;
    if target_rel != source_rel {
        vfs.remove_file(&source_rel).await.map_err(|error| {
            anyhow::anyhow!("failed to remove old entity {source_rel}: {error}")
        })?;
    }
    // Repoint inbound wikilinks so a rename doesn't dangle them. Uses the loaded
    // relation graph to touch only the files that actually link this entity, and
    // must run before the cache is invalidated (it reads the pre-rename graph).
    let updated_links = if let Some(new_basename) = &renamed_basename {
        let update = update_rename_backlinks(
            &library,
            vfs.as_ref(),
            &path.id,
            &entity.summary.basename,
            new_basename,
            &target_rel,
        )
        .await;
        (update.links > 0).then_some(update)
    } else {
        None
    };
    // Reflect the write by invalidating the cache and reloading, like every other
    // mutation. The index cache keeps this cheap — only the edited file is a miss
    // and re-read; unchanged entities and daily notes are reused by fingerprint.
    state.invalidate_cache().await;
    let reloaded = get_library(&state).await?;
    let record = reloaded
        .record_by_path(&target_rel)
        .or_else(|| reloaded.record_by_id(&path.id))
        .ok_or_else(|| ApiError::not_found("Updated entity was not indexed"))?;
    let entity = load_entity(&reloaded.config, vfs.as_ref(), &record.summary).await?;
    Ok(Json(EntityMutationResponse {
        entity,
        updated_links,
    }))
}

pub(crate) async fn create_entity(
    State(state): State<AppState>,
    Json(request): Json<CreateEntityRequest>,
) -> ApiResult<EntityMutationResponse> {
    let library = require_content_writes(&state).await?;
    let _mutation = state.content_mutation_lock().await;
    let type_config = type_config_or_err(&library.config, &request.entity_type)?;
    let basename = sanitize_basename(&request.basename)
        .map_err(|error| ApiError::bad_request(&error.to_string()))?;
    let frontmatter = match request.frontmatter_draft {
        Some(draft) => super::frontmatter_draft::normalize_draft(draft, type_config),
        None => request.frontmatter,
    };
    let vfs = state.vault_vfs(&library.config.vault_root);
    let path = write_new_entity_file(
        vfs.as_ref(),
        &library.config.taxonomy_root,
        type_config,
        &basename,
        &frontmatter,
        request.body.as_deref().unwrap_or(""),
    )
    .await?;
    state.invalidate_cache().await;
    let reloaded = get_library(&state).await?;
    let record = reloaded
        .record_by_path(&path)
        .ok_or_else(|| ApiError::not_found("Created entity was not indexed"))?;
    let entity = load_entity(&reloaded.config, vfs.as_ref(), &record.summary).await?;
    Ok(Json(EntityMutationResponse {
        entity,
        updated_links: None,
    }))
}

pub(crate) async fn delete_entity(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
    Json(request): Json<DeleteEntityRequest>,
) -> ApiResult<DeleteEntityResponse> {
    let library = require_content_writes(&state).await?;
    let _mutation = state.content_mutation_lock().await;
    let Some(entity) = library.record_by_id(&path.id) else {
        return Err(ApiError::not_found("Entity not found"));
    };
    check_revision(&request.revision, &entity.revision)?;
    let vfs = state.vault_vfs(&library.config.vault_root);
    let source_rel = entity.summary.path.clone();
    let raw = vfs
        .read_to_string(&source_rel)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {source_rel}: {error}"))?;
    // Match `update_entity`'s TOCTOU guard: the resident index can still carry
    // the client's revision when another editor changed the file behind a long
    // cache TTL. Never trash those newer bytes under a stale confirmation.
    check_revision(&request.revision, &file_revision(&raw))?;
    let trash_path = move_to_trash(vfs.as_ref(), &source_rel).await?;
    let asset_dir = entity_asset_dir(library.config.resolved_asset_root(), &source_rel);
    trash_entity_assets(vfs.as_ref(), &asset_dir).await;
    state.invalidate_cache().await;
    Ok(Json(DeleteEntityResponse {
        deleted_id: path.id,
        backup_path: trash_path,
    }))
}

/// Moves an entity's asset directory to follow an in-app rename and rewrites
/// frontmatter image paths that pointed inside it. Best-effort: if the source is
/// absent or the destination already exists, nothing is moved and the existing
/// (still valid) paths are left in place.
async fn move_entity_assets(
    vfs: &dyn Vfs,
    asset_root: &str,
    old_relative: &str,
    new_relative: &str,
    frontmatter: &mut Map<String, Value>,
) {
    let old_dir = entity_asset_dir(asset_root, old_relative);
    let new_dir = entity_asset_dir(asset_root, new_relative);
    if old_dir == new_dir {
        return;
    }
    if !vfs.exists(&old_dir).await.unwrap_or(false) {
        return;
    }
    if vfs.exists(&new_dir).await.unwrap_or(false) {
        return;
    }
    if let Some(parent) = parent_dir(&new_dir) {
        if vfs.create_dir_all(parent).await.is_err() {
            return;
        }
    }
    if vfs.rename(&old_dir, &new_dir).await.is_err() {
        return;
    }
    rewrite_asset_prefix(frontmatter, &old_dir, &new_dir);
}

/// Rewrites string (and string-array) frontmatter values that begin with
/// `<old_dir>/` so they point at `<new_dir>/` instead.
///
/// `old_dir` is derived from the on-disk entity path, which is decomposed (NFD)
/// on Apple filesystems, whereas frontmatter cover paths are stored composed
/// (NFC). The prefix match therefore runs under NFC normalization — a byte-exact
/// compare would silently skip the rewrite and leave a dangling cover pointer
/// after the asset folder moved. The rewritten value is NFC, honoring the
/// stored-data-is-NFC invariant.
fn rewrite_asset_prefix(frontmatter: &mut Map<String, Value>, old_dir: &str, new_dir: &str) {
    use unicode_normalization::UnicodeNormalization;

    let old_prefix: String = format!("{old_dir}/").nfc().collect();
    let rewrite = |value: &mut Value| {
        if let Value::String(text) = value {
            let normalized: String = text.nfc().collect();
            if let Some(rest) = normalized.strip_prefix(&old_prefix) {
                *text = format!("{new_dir}/{rest}");
            }
        }
    };
    for value in frontmatter.values_mut() {
        match value {
            Value::Array(items) => items.iter_mut().for_each(rewrite),
            other => rewrite(other),
        }
    }
}

/// Moves an entity's Markdown file into the vault's `.trash` folder, mirroring
/// Obsidian's local trash. Returns the vault-relative trash path. Disambiguates
/// name collisions by appending ` 1`, ` 2`, … before the extension.
pub(super) async fn move_to_trash(vfs: &dyn Vfs, source_relative: &str) -> Result<String> {
    let file_name = source_relative
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow::anyhow!("entity path has no file name"))?;
    vfs.create_dir_all(".trash")
        .await
        .map_err(|error| anyhow::anyhow!("failed to create trash directory: {error}"))?;
    let dest = unique_path(vfs, ".trash", file_name).await;
    vfs.rename(source_relative, &dest)
        .await
        .map_err(|error| anyhow::anyhow!("failed to move entity to trash {dest}: {error}"))?;
    Ok(dest)
}

/// Returns a vault-relative path for `file_name` inside `dir`, appending ` 1`,
/// ` 2`, … before the extension if a file with that name already exists.
async fn unique_path(vfs: &dyn Vfs, dir: &str, file_name: &str) -> String {
    let candidate = format!("{dir}/{file_name}");
    if !vfs.exists(&candidate).await.unwrap_or(false) {
        return candidate;
    }
    let name = Path::new(file_name);
    let stem = name
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    let extension = name.extension().and_then(|s| s.to_str());
    let mut counter = 1;
    loop {
        let candidate_name = match extension {
            Some(extension) => format!("{stem} {counter}.{extension}"),
            None => format!("{stem} {counter}"),
        };
        let candidate = format!("{dir}/{candidate_name}");
        if !vfs.exists(&candidate).await.unwrap_or(false) {
            return candidate;
        }
        counter += 1;
    }
}

/// Moves an entity's asset directory into the vault's `.trash` folder, mirroring
/// its vault-relative path. Best-effort and non-fatal.
async fn trash_entity_assets(vfs: &dyn Vfs, asset_dir_relative: &str) {
    if !vfs.exists(asset_dir_relative).await.unwrap_or(false) {
        return;
    }
    let mut dest = format!(".trash/{asset_dir_relative}");
    if vfs.exists(&dest).await.unwrap_or(false) {
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
        dest = format!(".trash/{asset_dir_relative}-{timestamp}");
    }
    if let Some(parent) = parent_dir(&dest) {
        if vfs.create_dir_all(parent).await.is_err() {
            return;
        }
    }
    let _ = vfs.rename(asset_dir_relative, &dest).await;
}

/// Optimistic-concurrency guard: rejects a write whose client-supplied `expected`
/// revision no longer matches the entity's `actual` revision, with the uniform 409
/// shared by every entity/asset write path.
pub(super) fn check_revision(expected: &str, actual: &str) -> Result<(), ApiError> {
    if expected != actual {
        return Err(ApiError::conflict("Entity changed since it was loaded"));
    }
    Ok(())
}

/// Performs the shared entity read → fresh revision check → parse → mutate →
/// serialize → atomic write sequence under the router-wide content mutation
/// lock. The mutation closure is deliberately synchronous so provider/network
/// work cannot accidentally hold the lock.
pub(super) async fn edit_entity_document<T, F>(
    state: &AppState,
    vfs: &dyn Vfs,
    relative: &str,
    expected_revision: &str,
    edit: F,
) -> Result<T, ApiError>
where
    F: FnOnce(&mut MarkdownDocument) -> Result<T, ApiError>,
{
    let _mutation = state.content_mutation_lock().await;
    let (value, changed) =
        edit_entity_document_locked(vfs, relative, expected_revision, edit).await?;
    if changed {
        state.invalidate_cache().await;
    }
    Ok(value)
}

/// The guarded Markdown edit primitive for callers that already hold the
/// AppState content-mutation lock across a larger multi-file transaction.
pub(super) async fn edit_entity_document_locked<T, F>(
    vfs: &dyn Vfs,
    relative: &str,
    expected_revision: &str,
    edit: F,
) -> Result<(T, bool), ApiError>
where
    F: FnOnce(&mut MarkdownDocument) -> Result<T, ApiError>,
{
    let raw = vfs
        .read_to_string(relative)
        .await
        .map_err(|error| anyhow::anyhow!("failed to read entity {relative}: {error}"))?;
    check_revision(expected_revision, &file_revision(&raw))?;

    let mut document = split_markdown_document(&raw);
    let value = edit(&mut document)?;
    let new_raw = serialize_markdown_document(&document.frontmatter, &document.body);
    let changed = new_raw != raw;
    if changed {
        write_entity_raw(vfs, relative, &new_raw).await?;
    }

    Ok((value, changed))
}

/// Resolve an entity's declared type against the schema, with the uniform 400
/// every handler should return when the schema has no such type (e.g. a
/// hand-edited `type:` not present in `config.yaml`). Centralizing this keeps the
/// status and message consistent — some handlers previously returned a misleading
/// 404 "Entity not found" for this case.
pub(super) fn type_config_or_err<'a>(
    config: &'a KizunaConfig,
    entity_type: &str,
) -> Result<&'a EntityTypeConfig, ApiError> {
    config
        .type_config(entity_type)
        .ok_or_else(|| ApiError::bad_request("Unknown entity type"))
}

fn apply_frontmatter_patch(target: &mut Map<String, Value>, patch: Map<String, Value>) {
    for (key, value) in patch {
        if value.is_null() {
            target.remove(&key);
        } else {
            target.insert(key, value);
        }
    }
}

pub(super) fn sanitize_basename(value: &str) -> Result<String> {
    let basename = value.trim();
    if basename.is_empty() {
        anyhow::bail!("Entity filename cannot be empty");
    }
    if basename.to_lowercase().ends_with(".md") {
        anyhow::bail!("Entity filename must be a basename without .md");
    }
    let path = Path::new(basename);
    if path.components().count() != 1
        || basename.contains('/')
        || basename.contains('\\')
        || basename == "."
        || basename == ".."
    {
        anyhow::bail!("Entity filename must be a single Markdown basename");
    }
    if basename.chars().any(is_forbidden_obsidian_filename_char) {
        anyhow::bail!("Entity filename cannot contain / \\ : * ? \" < > | or control characters");
    }
    if is_reserved_windows_name(basename) {
        anyhow::bail!(
            "Entity filename cannot be a reserved Windows device name (CON, PRN, AUX, NUL, COM0-9, LPT0-9)"
        );
    }
    Ok(basename.to_string())
}

/// Maximum byte length of a derived basename (before `.md`). Kept well under the
/// common 255-*byte* filesystem limit so the `.md` suffix, a disambiguating
/// ` (2023)`/` (2)` suffix, and multi-byte characters all still fit.
const MAX_DERIVED_BASENAME_BYTES: usize = 200;

/// The full-width stand-in for a character Obsidian/Windows forbids in a
/// filename, or `None` when the character is allowed as-is. Used by
/// [`derive_basename`] so a title keeps its shape (`Fate/stay` → `Fate／stay`,
/// `Re:ZERO` → `Re：ZERO`) instead of being rejected outright.
fn fullwidth_forbidden_char(character: char) -> Option<char> {
    Some(match character {
        '/' => '／',
        '\\' => '＼',
        ':' => '：',
        '*' => '＊',
        '?' => '？',
        '"' => '＂',
        '<' => '＜',
        '>' => '＞',
        '|' => '｜',
        _ => return None,
    })
}

/// Derive a valid entity basename from an arbitrary title (typically an external
/// provider's title), *replacing* forbidden characters rather than rejecting the
/// whole title as [`sanitize_basename`] does. It NFC-normalizes (provider titles
/// can arrive decomposed), collapses whitespace, swaps each forbidden character
/// for its full-width equivalent, drops control characters, strips trailing dots
/// and spaces (which Windows disallows), sidesteps reserved device names, and
/// caps the length on a character boundary. Returns `None` only when nothing
/// usable remains (or the result still fails the strict validator), so the
/// caller can fall back to another source such as the provider id.
pub(super) fn derive_basename(title: &str) -> Option<String> {
    use unicode_normalization::UnicodeNormalization;

    // NFC + collapse whitespace to single spaces + drop control chars + swap
    // forbidden chars for full-width equivalents, in one pass.
    let mut normalized = String::new();
    let mut pending_space = false;
    for character in title.trim().nfc() {
        if character.is_whitespace() {
            pending_space = !normalized.is_empty();
            continue;
        }
        if character.is_control() {
            continue;
        }
        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        normalized.push(fullwidth_forbidden_char(character).unwrap_or(character));
    }

    // Cap on a char boundary, then strip trailing dots/spaces (Windows forbids
    // them, and the cut may have exposed one).
    let mut basename = String::new();
    for character in normalized.chars() {
        if basename.len() + character.len_utf8() > MAX_DERIVED_BASENAME_BYTES {
            break;
        }
        basename.push(character);
    }
    let mut basename = basename.trim_end_matches(['.', ' ']).to_string();
    if basename.is_empty() {
        return None;
    }

    // A reserved device name (CON, NUL, …) is refused by the OS regardless of
    // extension; suffix it so the derived title stays recognizable and valid.
    if is_reserved_windows_name(&basename) {
        basename.push('-');
    }

    // Final assertion: the result must satisfy the strict validator. A title
    // pathological enough to still fail (e.g. one that *is* `.md`) yields `None`
    // so the caller falls back.
    sanitize_basename(&basename).ok()
}

/// Basenames to try, in order, when placing a derived title that collides with a
/// *different* existing work (the same work is caught earlier by the external-ref
/// lookup): the bare title, then ` (year)`, then ` (provider)`, then numeric
/// suffixes as a guaranteed-terminating fallback. Each is re-validated by the
/// caller before use; the year/provider inputs are expected to be already clean
/// (a date fragment, a provider id).
pub(super) fn basename_disambiguation_candidates(
    base: &str,
    year: Option<&str>,
    provider: Option<&str>,
) -> Vec<String> {
    let mut candidates = vec![base.to_string()];
    if let Some(year) = year.map(str::trim).filter(|value| !value.is_empty()) {
        candidates.push(format!("{base} ({year})"));
    }
    if let Some(provider) = provider.map(str::trim).filter(|value| !value.is_empty()) {
        candidates.push(format!("{base} ({provider})"));
    }
    for suffix in 2..=20 {
        candidates.push(format!("{base} ({suffix})"));
    }
    candidates
}

fn is_forbidden_obsidian_filename_char(character: char) -> bool {
    matches!(
        character,
        '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
    ) || character.is_control()
}

/// Windows reserves a handful of device names and treats a file as reserved
/// regardless of its extension, so `CON.md` (and `CON.anything.md`) is refused
/// by the OS. We compare the stem before the first dot, case-insensitively.
fn is_reserved_windows_name(basename: &str) -> bool {
    let stem = basename.split('.').next().unwrap_or(basename);
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit())
}

/// Vault-relative path for a new entity file under its type directory.
fn entity_create_path(
    taxonomy_root: &str,
    type_config: &EntityTypeConfig,
    basename: &str,
) -> String {
    format!(
        "{}/{}/{basename}.md",
        taxonomy_root.trim_end_matches('/'),
        type_config.path
    )
}

/// Writes a brand-new entity file at `{taxonomy_root}/{type.path}/{basename}.md`,
/// returning its vault-relative path. Errors with 409 if the exact path already
/// exists. Shared by the manual create handler and quick-add (which resolves a
/// free basename first). Assumes `basename` is already validated.
pub(super) async fn write_new_entity_file(
    vfs: &dyn Vfs,
    taxonomy_root: &str,
    type_config: &EntityTypeConfig,
    basename: &str,
    frontmatter: &Map<String, Value>,
    body: &str,
) -> Result<String, ApiError> {
    let path = entity_create_path(taxonomy_root, type_config, basename);
    if vfs
        .exists(&path)
        .await
        .map_err(|error| anyhow::anyhow!("failed to check entity path: {error}"))?
    {
        return Err(ApiError::conflict("Entity file already exists"));
    }
    let raw = serialize_markdown_document(frontmatter, body);
    write_entity_raw(vfs, &path, &raw).await?;
    Ok(path)
}

/// Picks a free basename for a new entity: the derived title if its path is free
/// and not wikilink-ambiguous with an existing entity of the same type, else the
/// disambiguation candidates (` (year)`, ` (provider)`, numeric) in order. The
/// wikilink-normalized check mirrors the duplicate-filename cleanup queue so
/// quick-add never mints a pair that would immediately land there. Returns the
/// chosen basename and whether it was disambiguated (i.e. differs from `base`).
pub(super) async fn resolve_free_basename(
    vfs: &dyn Vfs,
    taxonomy_root: &str,
    type_config: &EntityTypeConfig,
    library: &Library,
    base: &str,
    year: Option<&str>,
    provider: Option<&str>,
) -> Result<(String, bool), ApiError> {
    let existing: HashSet<String> = library
        .summaries()
        .filter(|summary| summary.entity_type == type_config.id)
        .map(|summary| normalize_wikilink_target(&summary.basename))
        .collect();
    for (index, candidate) in basename_disambiguation_candidates(base, year, provider)
        .iter()
        .enumerate()
    {
        if sanitize_basename(candidate).is_err()
            || existing.contains(&normalize_wikilink_target(candidate))
        {
            continue;
        }
        let path = entity_create_path(taxonomy_root, type_config, candidate);
        let taken = vfs
            .exists(&path)
            .await
            .map_err(|error| anyhow::anyhow!("failed to check entity path: {error}"))?;
        if !taken {
            return Ok((candidate.clone(), index > 0));
        }
    }
    Err(ApiError::conflict(
        "Could not find a free filename for this entity",
    ))
}

/// Writes an entity's raw Markdown to a vault-relative path, creating parent
/// directories. Containment is enforced by the VFS's path normalization.
///
/// The active VFS supplies the atomic replacement mechanism, so a crash or a
/// concurrent reader never observes a truncated half-written Markdown file.
pub(super) async fn write_entity_raw(vfs: &dyn Vfs, relative: &str, raw: &str) -> Result<()> {
    if let Some(parent) = parent_dir(relative) {
        vfs.create_dir_all(parent).await.map_err(|error| {
            anyhow::anyhow!("failed to create entity directory {parent}: {error}")
        })?;
    }
    vfs.write_atomic(relative, raw.as_bytes())
        .await
        .map_err(|error| anyhow::anyhow!("failed to write entity {relative}: {error}"))
}

/// Parent directory of a vault-relative path, or `None` when it lives at the
/// vault root.
pub(super) fn parent_dir(relative: &str) -> Option<&str> {
    relative.rsplit_once('/').map(|(parent, _)| parent)
}

/// Repoints inbound `[[wikilinks]]` after an entity is renamed from
/// `old_basename` to `new_basename`. The loaded relation graph already knows
/// exactly which managed files link to the renamed entity (configured type
/// folders and daily notes), so only those are re-read — never the whole vault.
/// List pages aren't in the graph, so the (small) list directory is swept
/// directly at the end.
///
/// Best-effort and per-file atomic: a file that fails to read or write is
/// skipped (its link merely stays dangling, as it would have before this
/// feature) and no file is left half-written. The rename itself has already
/// committed, so the worst case degrades to today's behavior rather than error.
async fn update_rename_backlinks(
    library: &Library,
    vfs: &dyn Vfs,
    old_id: &str,
    old_basename: &str,
    new_basename: &str,
    renamed_file_path: &str,
) -> RenameLinkUpdate {
    // Group the target texts that resolved to the renamed entity by the file
    // containing them. Only `Out` edges are true inbound links — an `In` edge
    // here is the renamed entity's own outbound edge mirrored. Self-references
    // are handled separately against the freshly written file, so the renamed
    // entity is skipped as a source here.
    let mut per_file: HashMap<String, HashSet<String>> = HashMap::new();
    for relation in library.relations_to(old_id) {
        if relation.direction != RelationDirection::Out || relation.source_id == old_id {
            continue;
        }
        let Some(path) = rename_source_path(library, &relation.source_id) else {
            continue;
        };
        if path == renamed_file_path {
            continue;
        }
        per_file
            .entry(path)
            .or_default()
            .insert(normalize_full_target(&relation.target_title));
    }

    let mut files = 0u32;
    let mut links = 0u32;
    for (path, old_targets) in per_file {
        let changed = rewrite_managed_file(vfs, &path, |raw| {
            rewrite_backlink_wikilinks(raw, &old_targets, new_basename)
        })
        .await;
        if changed > 0 {
            files += 1;
            links += changed;
        }
    }

    // The renamed file's own self-references (e.g. `[[Old]]` in its body, which
    // the relation graph drops as a self-edge).
    let self_changed = rewrite_managed_file(vfs, renamed_file_path, |raw| {
        rewrite_self_wikilinks(raw, old_basename, new_basename)
    })
    .await;
    if self_changed > 0 {
        files += 1;
        links += self_changed;
    }

    // List pages (`KizunaShelf/Lists/`) are read on demand, never indexed into the
    // relation graph, so the cache can't point us at them. The directory is small
    // and user-curated and a rename is rare, so we sweep it directly: rewrite any
    // `[[wikilink]]` that *resolves* — by the same `find_target` rules used
    // everywhere else — to the renamed entity. Resolving (rather than matching a
    // bare basename) is what keeps a list's `[[Manga/Beta]]` untouched when a
    // different `Anime/Beta` is the one being renamed. The sweep is best-effort,
    // so a missing/unreadable list directory just yields no updates.
    let list_paths = list_file_paths(vfs).await.unwrap_or_default();
    if !list_paths.is_empty() {
        let by_basename = normalized_entity_basename_index(&library.records);
        for path in list_paths {
            let changed = rewrite_managed_file(vfs, &path, |raw| {
                rewrite_wikilinks_matching(raw, new_basename, |target| {
                    find_target(target, None, &by_basename)
                        .is_some_and(|record| record.summary.id == old_id)
                })
            })
            .await;
            if changed > 0 {
                files += 1;
                links += changed;
            }
        }
    }

    RenameLinkUpdate { files, links }
}

/// Vault-relative path of a relation's source file: a daily note's own path, or
/// a resident entity's path.
fn rename_source_path(library: &Library, source_id: &str) -> Option<String> {
    if let Some((_, relative_path)) = parse_daily_note_source_id(source_id) {
        Some(relative_path.to_string())
    } else {
        library
            .record_by_id(source_id)
            .map(|record| record.summary.path.clone())
    }
}

/// Reads `path`, applies `rewrite`, and atomically writes it back only if the
/// rewrite changed anything. Returns the number of links changed (0 on any I/O
/// error, keeping the caller best-effort).
async fn rewrite_managed_file(
    vfs: &dyn Vfs,
    path: &str,
    rewrite: impl FnOnce(&str) -> (String, usize),
) -> u32 {
    let Ok(raw) = vfs.read_to_string(path).await else {
        return 0;
    };
    let (rewritten, count) = rewrite(&raw);
    if count == 0 {
        return 0;
    }
    if vfs.write_atomic(path, rewritten.as_bytes()).await.is_err() {
        return 0;
    }
    count as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sanitize_basename_accepts_a_plain_name_and_trims() {
        assert_eq!(
            sanitize_basename("  Star Voyager  ").unwrap(),
            "Star Voyager"
        );
    }

    #[test]
    fn sanitize_basename_rejects_empty() {
        let error = sanitize_basename("   ").unwrap_err().to_string();
        assert!(error.contains("cannot be empty"));
    }

    #[test]
    fn sanitize_basename_rejects_the_md_extension() {
        assert!(sanitize_basename("Star.md").is_err());
        assert!(sanitize_basename("Star.MD").is_err());
    }

    #[test]
    fn sanitize_basename_rejects_path_traversal_and_separators() {
        for bad in ["..", ".", "a/b", "a\\b", "../escape", "sub/Note"] {
            assert!(
                sanitize_basename(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn sanitize_basename_rejects_forbidden_and_control_chars() {
        for bad in [
            "a:b",
            "a*b",
            "a?b",
            "a\"b",
            "a<b",
            "a>b",
            "a|b",
            "a\u{0007}b",
        ] {
            assert!(
                sanitize_basename(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn sanitize_basename_rejects_reserved_windows_names() {
        for bad in [
            "CON",
            "con",
            "PRN",
            "aux",
            "NUL",
            "COM1",
            "com9",
            "COM0",
            "LPT1",
            "lpt0",
            "CON.backup",
            "NUL.txt",
        ] {
            assert!(
                sanitize_basename(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn sanitize_basename_allows_names_that_only_resemble_reserved_ones() {
        for ok in [
            "CONSTANT",
            "COMET",
            "COM",
            "LPT",
            "COM10",
            "Console",
            "Aux Cable",
        ] {
            assert!(sanitize_basename(ok).is_ok(), "{ok:?} should be allowed");
        }
    }

    #[test]
    fn derive_basename_replaces_forbidden_chars_with_full_width() {
        assert_eq!(
            derive_basename("Fate/stay night").unwrap(),
            "Fate／stay night"
        );
        assert_eq!(derive_basename("Re:ZERO").unwrap(), "Re：ZERO");
        assert_eq!(
            derive_basename("What? A \"Test\" <of> it|all\\here*").unwrap(),
            "What？ A ＂Test＂ ＜of＞ it｜all＼here＊"
        );
    }

    #[test]
    fn derive_basename_normalizes_whitespace_and_control_chars() {
        assert_eq!(
            derive_basename("  Star   Voyager\t\u{0007}II \n").unwrap(),
            "Star Voyager II"
        );
    }

    #[test]
    fn derive_basename_nfc_normalizes() {
        // NFD "ず" (す + combining dakuten) composes to the single NFC codepoint.
        let nfd = "田所あ\u{3059}\u{3099}さ";
        let derived = derive_basename(nfd).unwrap();
        assert_eq!(derived, "田所あずさ");
        assert!(
            !derived.contains('\u{3099}'),
            "combining mark should be composed away"
        );
    }

    #[test]
    fn derive_basename_strips_trailing_dots_and_spaces() {
        assert_eq!(derive_basename("Title...  ").unwrap(), "Title");
        assert_eq!(derive_basename("Title. . .").unwrap(), "Title");
    }

    #[test]
    fn derive_basename_suffixes_reserved_windows_names() {
        assert_eq!(derive_basename("CON").unwrap(), "CON-");
        assert_eq!(derive_basename("nul").unwrap(), "nul-");
        // A colon that composes to a reserved stem is caught after replacement.
        assert_eq!(derive_basename("Console").unwrap(), "Console");
    }

    #[test]
    fn derive_basename_caps_length_on_char_boundary() {
        let long = "あ".repeat(200); // 600 bytes
        let derived = derive_basename(&long).unwrap();
        assert!(derived.len() <= MAX_DERIVED_BASENAME_BYTES);
        assert!(derived.chars().all(|character| character == 'あ'));
    }

    #[test]
    fn derive_basename_returns_none_when_nothing_usable_remains() {
        assert_eq!(derive_basename("   "), None);
        assert_eq!(derive_basename("\u{0007}\u{0008}"), None);
        // A title that reduces to the literal `.md` fails the strict validator.
        assert_eq!(derive_basename(".md"), None);
    }

    #[test]
    fn basename_disambiguation_candidates_orders_year_then_provider_then_numbers() {
        let candidates = basename_disambiguation_candidates("Akira", Some("1988"), Some("tmdb"));
        assert_eq!(candidates[0], "Akira");
        assert_eq!(candidates[1], "Akira (1988)");
        assert_eq!(candidates[2], "Akira (tmdb)");
        assert_eq!(candidates[3], "Akira (2)");
        // Blank year/provider are skipped.
        let sparse = basename_disambiguation_candidates("Akira", None, Some("  "));
        assert_eq!(sparse[0], "Akira");
        assert_eq!(sparse[1], "Akira (2)");
    }

    #[test]
    fn apply_frontmatter_patch_inserts_updates_and_removes_on_null() {
        let mut target = Map::new();
        target.insert("keep".to_string(), json!("old"));
        target.insert("drop".to_string(), json!("gone"));

        let mut patch = Map::new();
        patch.insert("keep".to_string(), json!("new"));
        patch.insert("add".to_string(), json!(5));
        patch.insert("drop".to_string(), Value::Null);
        apply_frontmatter_patch(&mut target, patch);

        assert_eq!(target.get("keep"), Some(&json!("new")));
        assert_eq!(target.get("add"), Some(&json!(5)));
        assert!(
            !target.contains_key("drop"),
            "a null patch value removes the key"
        );
    }

    #[test]
    fn parent_dir_returns_the_directory_or_none_at_root() {
        assert_eq!(parent_dir("Taxonomy/Anime/Foo.md"), Some("Taxonomy/Anime"));
        assert_eq!(parent_dir("Foo.md"), None);
    }

    #[test]
    fn rewrite_asset_prefix_repoints_strings_and_arrays() {
        let mut frontmatter = Map::new();
        frontmatter.insert("cover".to_string(), json!("Assets/Anime/A/cover.jpg"));
        frontmatter.insert(
            "gallery".to_string(),
            json!(["Assets/Anime/A/1.jpg", "https://cdn/x.jpg"]),
        );
        frontmatter.insert("title".to_string(), json!("A"));
        rewrite_asset_prefix(&mut frontmatter, "Assets/Anime/A", "Assets/Anime/B");

        assert_eq!(frontmatter["cover"], json!("Assets/Anime/B/cover.jpg"));
        assert_eq!(
            frontmatter["gallery"],
            json!(["Assets/Anime/B/1.jpg", "https://cdn/x.jpg"])
        );
        // A value that isn't under the old prefix is untouched.
        assert_eq!(frontmatter["title"], json!("A"));
    }

    #[test]
    fn rewrite_asset_prefix_matches_across_nfc_nfd_composition() {
        // `old_dir` carries the on-disk (NFD) folder name — decomposed katakana GA
        // (KA + combining voiced mark) — while the frontmatter cover path was
        // stored composed (NFC, single U+30AC). A byte-exact prefix compare would
        // miss and leave the cover pointer dangling after the folder moved.
        let nfd_dir = "Assets/Anime/\u{30AB}\u{3099}";
        let nfc_cover = "Assets/Anime/\u{30AC}/cover.jpg";
        let mut frontmatter = Map::new();
        frontmatter.insert("cover".to_string(), json!(nfc_cover));

        rewrite_asset_prefix(&mut frontmatter, nfd_dir, "Assets/Anime/Renamed");

        assert_eq!(
            frontmatter["cover"],
            json!("Assets/Anime/Renamed/cover.jpg")
        );
    }
}
