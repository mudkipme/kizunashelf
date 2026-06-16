//! Filesystem abstraction for vault content.
//!
//! All vault I/O (entities, assets, daily notes, the vault config) goes through
//! the [`Vfs`] trait using **vault-relative, forward-slash paths**. Desktop and
//! the web/api server use [`NativeVfs`] (a thin `tokio::fs` wrapper rooted at the
//! vault). iOS will supply a Swift-backed implementation (security-scoped
//! bookmarks + `NSFileCoordinator`) — see docs/ios-port-plan.md §5.
//!
//! App-private I/O (the app config file, the provider token cache) is *not* part
//! of this abstraction: it lives outside the vault and stays on direct fs (or, on
//! iOS, in `@AppStorage`/Keychain).
//!
//! Path containment is enforced lexically by [`normalize_relative`] (reject `..`
//! and absolute components) rather than by `canonicalize()` + prefix checks, so
//! the same guarantee holds on a non-local filesystem where canonicalization is
//! unavailable.

mod native;

#[cfg(test)]
mod memory;

pub use native::NativeVfs;

#[cfg(test)]
pub use memory::InMemoryVfs;

use async_trait::async_trait;
use std::fmt;

pub type VfsResult<T> = Result<T, VfsError>;

/// Error surface of the VFS. Maps the `std::io::ErrorKind`s the core actually
/// branches on (`NotFound`, `AlreadyExists`) plus a catch-all.
#[derive(Debug)]
pub enum VfsError {
    NotFound,
    AlreadyExists,
    /// The relative path was absolute or escaped the vault root.
    InvalidPath(String),
    Other(String),
}

impl VfsError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, VfsError::NotFound)
    }
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfsError::NotFound => write!(f, "not found"),
            VfsError::AlreadyExists => write!(f, "already exists"),
            VfsError::InvalidPath(path) => write!(f, "invalid vault path: {path}"),
            VfsError::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for VfsError {}

/// One entry returned by [`Vfs::read_dir`].
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_file: bool,
}

/// File metadata. `modified_unix_nanos` is `0` when the backend cannot report a
/// modification time; the revision hash folds it in but also hashes content, so a
/// missing mtime only weakens (never breaks) change detection.
#[derive(Clone, Debug)]
pub struct Metadata {
    pub is_dir: bool,
    pub is_file: bool,
    pub len: u64,
    pub modified_unix_nanos: u128,
}

/// Vault filesystem. Implementations operate on vault-relative, forward-slash
/// paths (the empty string denotes the vault root).
#[async_trait]
pub trait Vfs: Send + Sync {
    async fn read(&self, path: &str) -> VfsResult<Vec<u8>>;
    async fn write(&self, path: &str, data: &[u8]) -> VfsResult<()>;
    async fn create_dir_all(&self, path: &str) -> VfsResult<()>;
    async fn read_dir(&self, path: &str) -> VfsResult<Vec<DirEntry>>;
    async fn metadata(&self, path: &str) -> VfsResult<Metadata>;
    async fn rename(&self, from: &str, to: &str) -> VfsResult<()>;
    async fn remove_file(&self, path: &str) -> VfsResult<()>;

    /// Batch-reads many files in one call, returning `(path, contents)` for each
    /// that was read successfully (missing files are skipped; order is not
    /// guaranteed). This is the hot path for loading the library: hosts with
    /// expensive per-call overhead (the iOS FFI + file coordination) override it
    /// to avoid one round trip per file. The default reads sequentially.
    async fn read_files(&self, paths: &[String]) -> VfsResult<Vec<(String, Vec<u8>)>> {
        let mut out = Vec::with_capacity(paths.len());
        for path in paths {
            match self.read(path).await {
                Ok(bytes) => out.push((path.clone(), bytes)),
                Err(VfsError::NotFound) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(out)
    }

    /// Convenience: read a file as UTF-8.
    async fn read_to_string(&self, path: &str) -> VfsResult<String> {
        let bytes = self.read(path).await?;
        String::from_utf8(bytes).map_err(|error| VfsError::Other(error.to_string()))
    }

    /// Convenience: does the path exist? (`metadata`, mapping `NotFound` to false.)
    async fn exists(&self, path: &str) -> VfsResult<bool> {
        match self.metadata(path).await {
            Ok(_) => Ok(true),
            Err(VfsError::NotFound) => Ok(false),
            Err(error) => Err(error),
        }
    }
}

/// Normalizes a vault-relative path: collapses `.` and empty segments, rejects
/// `..` and absolute/drive components, and joins with `/`. The vault root is the
/// empty string. This is the single containment guarantee for vault I/O.
pub fn normalize_relative(path: &str) -> VfsResult<String> {
    let mut parts: Vec<&str> = Vec::new();
    for segment in path.split(['/', '\\']) {
        match segment {
            "" | "." => continue,
            ".." => {
                return Err(VfsError::InvalidPath(path.to_string()));
            }
            // Reject Windows drive prefixes like `C:`; bare `:` is also illegal in
            // vault paths.
            segment if segment.contains(':') => {
                return Err(VfsError::InvalidPath(path.to_string()));
            }
            segment => parts.push(segment),
        }
    }
    Ok(parts.join("/"))
}

/// Recursively collects vault-relative paths of `.md` files under `root` (a
/// vault-relative directory). A missing directory yields an empty list. Used for
/// the daily-notes walk.
pub async fn walk_markdown_files(vfs: &dyn Vfs, root: &str) -> VfsResult<Vec<String>> {
    let mut files = Vec::new();
    let mut stack = vec![normalize_relative(root)?];
    while let Some(dir) = stack.pop() {
        let entries = match vfs.read_dir(&dir).await {
            Ok(entries) => entries,
            Err(VfsError::NotFound) => continue,
            Err(error) => return Err(error),
        };
        for entry in entries {
            let child = if dir.is_empty() {
                entry.name.clone()
            } else {
                format!("{dir}/{}", entry.name)
            };
            if entry.is_dir {
                stack.push(child);
            } else if entry.is_file && child.ends_with(".md") {
                files.push(child);
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_and_rejects() {
        assert_eq!(normalize_relative("").unwrap(), "");
        assert_eq!(normalize_relative("a/b/c.md").unwrap(), "a/b/c.md");
        assert_eq!(normalize_relative("./a//b/").unwrap(), "a/b");
        assert_eq!(normalize_relative("a\\b").unwrap(), "a/b");
        assert!(normalize_relative("../escape").is_err());
        assert!(normalize_relative("a/../../b").is_err());
        assert!(normalize_relative("C:/Windows").is_err());
    }
}
