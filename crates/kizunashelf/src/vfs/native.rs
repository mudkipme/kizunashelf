//! `tokio::fs`-backed [`Vfs`] rooted at an absolute vault path. Used by the
//! desktop app and the web/api server; behavior is identical to the pre-VFS
//! direct `fs` calls, with containment enforced by [`normalize_relative`].

use super::{normalize_relative, DirEntry, Metadata, Vfs, VfsError, VfsResult};
use async_trait::async_trait;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use tokio::fs;

/// A vault filesystem rooted at an absolute path.
pub struct NativeVfs {
    root: PathBuf,
}

impl NativeVfs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Resolves a vault-relative path to an absolute path inside the root,
    /// rejecting traversal.
    fn resolve(&self, path: &str) -> VfsResult<PathBuf> {
        let normalized = normalize_relative(path)?;
        Ok(self.root.join(normalized))
    }
}

fn map_io(error: std::io::Error) -> VfsError {
    match error.kind() {
        ErrorKind::NotFound => VfsError::NotFound,
        ErrorKind::AlreadyExists => VfsError::AlreadyExists,
        _ => VfsError::Other(error.to_string()),
    }
}

#[async_trait]
impl Vfs for NativeVfs {
    async fn read(&self, path: &str) -> VfsResult<Vec<u8>> {
        let path = self.resolve(path)?;
        fs::read(&path).await.map_err(map_io)
    }

    async fn write(&self, path: &str, data: &[u8]) -> VfsResult<()> {
        let path = self.resolve(path)?;
        fs::write(&path, data).await.map_err(map_io)
    }

    async fn write_atomic(&self, path: &str, data: &[u8]) -> VfsResult<()> {
        let path = self.resolve(path)?;
        let tmp = path.with_extension(match path.extension() {
            Some(extension) => format!("{}.tmp", extension.to_string_lossy()),
            None => "tmp".to_string(),
        });
        fs::write(&tmp, data).await.map_err(map_io)?;
        fs::rename(&tmp, &path).await.map_err(map_io)
    }

    async fn create_dir_all(&self, path: &str) -> VfsResult<()> {
        let path = self.resolve(path)?;
        fs::create_dir_all(&path).await.map_err(map_io)
    }

    async fn read_dir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let path = self.resolve(path)?;
        let mut entries = fs::read_dir(&path).await.map_err(map_io)?;
        let mut result = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(map_io)? {
            let file_type = entry.file_type().await.map_err(map_io)?;
            result.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: file_type.is_dir(),
                is_file: file_type.is_file(),
            });
        }
        Ok(result)
    }

    async fn metadata(&self, path: &str) -> VfsResult<Metadata> {
        let path = self.resolve(path)?;
        let metadata = fs::metadata(&path).await.map_err(map_io)?;
        let modified_unix_nanos = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        Ok(Metadata {
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            len: metadata.len(),
            modified_unix_nanos,
        })
    }

    async fn rename(&self, from: &str, to: &str) -> VfsResult<()> {
        let from = self.resolve(from)?;
        let to = self.resolve(to)?;
        fs::rename(&from, &to).await.map_err(map_io)
    }

    async fn remove_file(&self, path: &str) -> VfsResult<()> {
        let path = self.resolve(path)?;
        fs::remove_file(&path).await.map_err(map_io)
    }
}
