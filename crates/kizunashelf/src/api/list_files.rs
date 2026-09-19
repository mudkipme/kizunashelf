//! File plumbing shared by the two list kinds under [`LISTS_DIR`]: static lists
//! (`.md`) and smart lists (`.base`). Both are one file per list, addressed by
//! basename, so id ↔ path mapping, reads, the free-name check, rename-target
//! resolution, the revision-guarded write-then-remove, and trashing are the same
//! steps with a different extension and noun. Handlers own only what differs:
//! how the file's content is built.

use super::error::ApiError;
use super::mutations::{check_revision, move_to_trash, sanitize_basename, write_entity_raw};
use super::state::ContentMutationGuard;
use crate::library::file_revision;
use crate::lists::LISTS_DIR;
use crate::smart_lists::SMART_LIST_EXTENSION;
use crate::vfs::Vfs;

/// One list kind: its file extension and the noun its errors use.
pub(super) struct ListFileKind {
    extension: &'static str,
    /// Sentence-case noun for messages (`"List"`, `"Smart list"`).
    noun: &'static str,
}

pub(super) const STATIC_LIST: ListFileKind = ListFileKind {
    extension: "md",
    noun: "List",
};

pub(super) const SMART_LIST: ListFileKind = ListFileKind {
    extension: SMART_LIST_EXTENSION,
    noun: "Smart list",
};

/// Every kind, for the cross-kind free-name check.
const ALL_KINDS: [&ListFileKind; 2] = [&STATIC_LIST, &SMART_LIST];

impl ListFileKind {
    fn lower_noun(&self) -> String {
        self.noun.to_lowercase()
    }

    fn path_for(&self, basename: &str) -> String {
        format!("{LISTS_DIR}/{basename}.{}", self.extension)
    }

    /// Vault-relative path of an existing list from its URL id, validated for
    /// containment.
    pub(super) fn path(&self, id: &str) -> Result<String, ApiError> {
        let basename = sanitize_basename(id)
            .map_err(|_| ApiError::bad_request(&format!("Invalid {} id", self.lower_noun())))?;
        Ok(self.path_for(&basename))
    }

    /// The list id (basename without the extension) from a vault-relative path.
    pub(super) fn id(&self, path: &str) -> String {
        let name = path.rsplit('/').next().unwrap_or(path);
        name.strip_suffix(&format!(".{}", self.extension))
            .unwrap_or(name)
            .to_string()
    }

    /// Reads a list file, mapping absence to this kind's 404.
    pub(super) async fn read_raw(&self, vfs: &dyn Vfs, path: &str) -> Result<String, ApiError> {
        match vfs.read_to_string(path).await {
            Ok(raw) => Ok(raw),
            Err(err) if err.is_not_found() => {
                Err(ApiError::not_found(&format!("{} not found", self.noun)))
            }
            Err(err) => {
                Err(anyhow::anyhow!("failed to read {} {path}: {err}", self.lower_noun()).into())
            }
        }
    }

    /// The path a user-supplied `name` would create, or 409 when that basename
    /// is taken by a list of **either** kind. The kinds share one namespace:
    /// item edits refuse an id that also names a smart list, so a same-named
    /// static list could be created but never edited.
    pub(super) async fn new_path(&self, vfs: &dyn Vfs, name: &str) -> Result<String, ApiError> {
        let basename =
            sanitize_basename(name).map_err(|error| ApiError::bad_request(&error.to_string()))?;
        if !basename_is_free(vfs, &basename, None).await? {
            return Err(ApiError::conflict(&format!("{} already exists", self.noun)));
        }
        Ok(self.path_for(&basename))
    }

    /// Where an update writes: `source_path` itself, or the path `rename_to`
    /// names — 409 when another list of either kind already holds that name.
    pub(super) async fn rename_target(
        &self,
        vfs: &dyn Vfs,
        source_path: &str,
        rename_to: Option<&str>,
    ) -> Result<String, ApiError> {
        let Some(rename_to) = rename_to else {
            return Ok(source_path.to_string());
        };
        let basename = sanitize_basename(rename_to)
            .map_err(|error| ApiError::bad_request(&error.to_string()))?;
        let target = self.path_for(&basename);
        if target != source_path && !basename_is_free(vfs, &basename, Some(source_path)).await? {
            return Err(ApiError::conflict(&format!(
                "Target {} already exists",
                self.lower_noun()
            )));
        }
        Ok(target)
    }

    /// Writes `raw` to `target_path` and, for a rename, removes `source_path` —
    /// after re-checking `expected_revision` against a fresh read of the source,
    /// so an external edit made while the update was being built is never
    /// overwritten.
    pub(super) async fn write_replacing(
        &self,
        guard: &ContentMutationGuard<'_>,
        vfs: &dyn Vfs,
        source_path: &str,
        target_path: &str,
        expected_revision: &str,
        raw: &str,
    ) -> Result<(), ApiError> {
        let latest = self.read_raw(vfs, source_path).await?;
        check_revision(expected_revision, &file_revision(&latest))?;
        write_entity_raw(guard, vfs, target_path, raw).await?;
        if target_path != source_path {
            vfs.remove_file(source_path).await.map_err(|err| {
                anyhow::anyhow!(
                    "failed to remove old {} {source_path}: {err}",
                    self.lower_noun()
                )
            })?;
        }
        Ok(())
    }

    /// Moves an existing list to the vault trash, returning the trash path;
    /// 404 when there is no such list.
    pub(super) async fn trash(
        &self,
        guard: &ContentMutationGuard<'_>,
        vfs: &dyn Vfs,
        path: &str,
    ) -> Result<String, ApiError> {
        if !exists(vfs, path).await? {
            return Err(ApiError::not_found(&format!("{} not found", self.noun)));
        }
        Ok(move_to_trash(guard, vfs, path).await?)
    }
}

/// Whether no list of any kind uses `basename`. `ignore` exempts one path: the
/// list being renamed, which doesn't collide with itself.
pub(super) async fn basename_is_free(
    vfs: &dyn Vfs,
    basename: &str,
    ignore: Option<&str>,
) -> Result<bool, ApiError> {
    for kind in ALL_KINDS {
        let path = kind.path_for(basename);
        if Some(path.as_str()) != ignore && exists(vfs, &path).await? {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn exists(vfs: &dyn Vfs, path: &str) -> Result<bool, ApiError> {
    Ok(vfs
        .exists(path)
        .await
        .map_err(|err| anyhow::anyhow!("failed to check list path {path}: {err}"))?)
}
