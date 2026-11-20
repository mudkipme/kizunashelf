//! The vault write path: the content-mutation lock and the primitives that
//! write entity files under it.
//!
//! Every write here keeps three properties together:
//! - **Locked**: each helper takes a [`ContentMutationGuard`], which only
//!   [`ContentMutationLock::lock`] can produce, so an unlocked write does not
//!   compile. Callers hold it across the fresh read → revision check → write,
//!   and never across provider/network calls.
//! - **Revision-guarded**: optimistic concurrency via [`check_revision`],
//!   re-checked against freshly read bytes rather than the cached index.
//! - **Atomic**: content lands through [`Vfs::write_atomic`] (temp file +
//!   rename), so a crash or a concurrent reader never sees a torn file.
//!
//! Errors are [`WriteError`], which the API layer maps to HTTP statuses; this
//! module knows nothing about HTTP.

use super::frontmatter::{serialize_markdown_document, split_markdown_document, MarkdownDocument};
use super::parse::file_revision;
use crate::types::EntityTypeConfig;
use crate::vfs::Vfs;
use anyhow::Result;
use serde_json::{Map, Value};
use std::fmt;
use std::path::Path;
use tokio::sync::{Mutex, MutexGuard};

/// The vault-content mutation lock. Not reentrant: release the guard before
/// calling anything that acquires the lock itself.
#[derive(Default)]
pub(crate) struct ContentMutationLock(Mutex<()>);

impl ContentMutationLock {
    pub(crate) async fn lock(&self) -> ContentMutationGuard<'_> {
        ContentMutationGuard(self.0.lock().await)
    }

    /// Whether some caller currently holds the lock.
    #[cfg(test)]
    pub(crate) fn is_held(&self) -> bool {
        self.0.try_lock().is_err()
    }
}

/// Proof that the vault-content mutation lock is held. Only
/// [`ContentMutationLock::lock`] can construct one (the field is private to
/// this module); every vault write helper takes `&ContentMutationGuard`.
pub(crate) struct ContentMutationGuard<'a>(#[allow(dead_code)] MutexGuard<'a, ()>);

/// Why a guarded vault write was refused or failed.
#[derive(Debug)]
pub(crate) enum WriteError {
    /// The caller's expected revision no longer matches the file's content.
    RevisionConflict,
    /// A create targeted a path that already exists.
    AlreadyExists,
    /// The underlying VFS operation failed.
    Io(anyhow::Error),
}

impl fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RevisionConflict => formatter.write_str("Entity changed since it was loaded"),
            Self::AlreadyExists => formatter.write_str("Entity file already exists"),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl From<anyhow::Error> for WriteError {
    fn from(error: anyhow::Error) -> Self {
        Self::Io(error)
    }
}

/// Optimistic-concurrency guard: rejects a write whose client-supplied
/// `expected` revision no longer matches the entity's `actual` revision, with
/// the uniform conflict shared by every entity/asset write path.
pub(crate) fn check_revision(expected: &str, actual: &str) -> Result<(), WriteError> {
    if expected != actual {
        return Err(WriteError::RevisionConflict);
    }
    Ok(())
}

/// Writes an entity's raw Markdown to a vault-relative path, creating parent
/// directories. Containment is enforced by the VFS's path normalization.
///
/// The active VFS supplies the atomic replacement mechanism, so a crash or a
/// concurrent reader never observes a truncated half-written Markdown file.
pub(crate) async fn write_entity_raw(
    _guard: &ContentMutationGuard<'_>,
    vfs: &dyn Vfs,
    relative: &str,
    raw: &str,
) -> Result<()> {
    if let Some(parent) = parent_dir(relative) {
        vfs.create_dir_all(parent).await.map_err(|error| {
            anyhow::anyhow!("failed to create entity directory {parent}: {error}")
        })?;
    }
    vfs.write_atomic(relative, raw.as_bytes())
        .await
        .map_err(|error| anyhow::anyhow!("failed to write entity {relative}: {error}"))
}

/// Vault-relative path for a new entity file under its type directory.
pub(crate) fn entity_create_path(
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
/// returning its vault-relative path. Fails with [`WriteError::AlreadyExists`]
/// if the exact path already exists. Shared by the manual create handler,
/// quick-add and import (which resolve a free basename first). Assumes
/// `basename` is already validated.
pub(crate) async fn write_new_entity_file(
    guard: &ContentMutationGuard<'_>,
    vfs: &dyn Vfs,
    taxonomy_root: &str,
    type_config: &EntityTypeConfig,
    basename: &str,
    frontmatter: &Map<String, Value>,
    body: &str,
) -> Result<String, WriteError> {
    let path = entity_create_path(taxonomy_root, type_config, basename);
    if vfs
        .exists(&path)
        .await
        .map_err(|error| anyhow::anyhow!("failed to check entity path: {error}"))?
    {
        return Err(WriteError::AlreadyExists);
    }
    let raw = serialize_markdown_document(frontmatter, body);
    write_entity_raw(guard, vfs, &path, &raw).await?;
    Ok(path)
}

/// The guarded Markdown edit primitive: read → fresh revision check → parse →
/// mutate → serialize → atomic write, for callers holding the content-mutation
/// lock (possibly across a larger multi-file transaction). The edit closure is
/// deliberately synchronous so provider/network work cannot run under the lock.
/// Returns the closure's value and whether the file actually changed (an edit
/// that serializes to identical bytes is not written).
pub(crate) async fn edit_entity_document_locked<T, E, F>(
    guard: &ContentMutationGuard<'_>,
    vfs: &dyn Vfs,
    relative: &str,
    expected_revision: &str,
    edit: F,
) -> Result<(T, bool), E>
where
    E: From<WriteError>,
    F: FnOnce(&mut MarkdownDocument) -> Result<T, E>,
{
    let raw = vfs.read_to_string(relative).await.map_err(|error| {
        WriteError::Io(anyhow::anyhow!("failed to read entity {relative}: {error}"))
    })?;
    check_revision(expected_revision, &file_revision(&raw))?;

    let mut document = split_markdown_document(&raw);
    let value = edit(&mut document)?;
    let new_raw = serialize_markdown_document(&document.frontmatter, &document.body);
    let changed = new_raw != raw;
    if changed {
        write_entity_raw(guard, vfs, relative, &new_raw)
            .await
            .map_err(WriteError::Io)?;
    }

    Ok((value, changed))
}

/// Reads `path`, applies `rewrite` (which returns the new text and how many
/// edits it made), and atomically writes it back only if something changed.
/// Returns the edit count, or 0 on any I/O error: callers use this for
/// best-effort follow-up rewrites (e.g. repointing backlinks after a rename)
/// where a skipped file degrades gracefully rather than failing the request.
pub(crate) async fn rewrite_file_best_effort(
    _guard: &ContentMutationGuard<'_>,
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

/// Moves a Markdown file into the vault's `.trash` folder, mirroring Obsidian's
/// local trash. Returns the vault-relative trash path. Disambiguates name
/// collisions by appending ` 1`, ` 2`, … before the extension.
pub(crate) async fn move_to_trash(
    _guard: &ContentMutationGuard<'_>,
    vfs: &dyn Vfs,
    source_relative: &str,
) -> Result<String> {
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

/// Parent directory of a vault-relative path, or `None` when it lives at the
/// vault root.
pub(crate) fn parent_dir(relative: &str) -> Option<&str> {
    relative.rsplit_once('/').map(|(parent, _)| parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::InMemoryVfs;

    #[test]
    fn parent_dir_returns_the_directory_or_none_at_root() {
        assert_eq!(parent_dir("Taxonomy/Anime/Foo.md"), Some("Taxonomy/Anime"));
        assert_eq!(parent_dir("Foo.md"), None);
    }

    #[test]
    fn check_revision_rejects_a_mismatch() {
        assert!(check_revision("a", "a").is_ok());
        assert!(matches!(
            check_revision("a", "b"),
            Err(WriteError::RevisionConflict)
        ));
    }

    #[tokio::test]
    async fn edit_refuses_a_stale_revision_and_skips_no_op_writes() {
        let vfs = InMemoryVfs::new();
        vfs.write("Note.md", b"---\ntitle: A\n---\nbody\n")
            .await
            .unwrap();
        let lock = ContentMutationLock::default();
        let guard = lock.lock().await;

        let stale: Result<((), bool), WriteError> =
            edit_entity_document_locked(&guard, &vfs, "Note.md", "stale", |_| Ok(())).await;
        assert!(matches!(stale, Err(WriteError::RevisionConflict)));

        let raw = vfs.read_to_string("Note.md").await.unwrap();
        let (_, changed) = edit_entity_document_locked::<_, WriteError, _>(
            &guard,
            &vfs,
            "Note.md",
            &file_revision(&raw),
            |_| Ok(()),
        )
        .await
        .unwrap();
        assert!(!changed, "an edit that changes nothing is not written");
    }

    #[tokio::test]
    async fn move_to_trash_disambiguates_collisions() {
        let vfs = InMemoryVfs::new();
        vfs.write("A/Note.md", b"one").await.unwrap();
        vfs.write("B/Note.md", b"two").await.unwrap();
        let lock = ContentMutationLock::default();
        let guard = lock.lock().await;

        assert_eq!(
            move_to_trash(&guard, &vfs, "A/Note.md").await.unwrap(),
            ".trash/Note.md"
        );
        assert_eq!(
            move_to_trash(&guard, &vfs, "B/Note.md").await.unwrap(),
            ".trash/Note 1.md"
        );
    }
}
