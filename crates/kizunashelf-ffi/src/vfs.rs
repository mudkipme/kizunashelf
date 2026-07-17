//! The vault filesystem bridge: a UniFFI callback interface `VaultFileSystem`
//! that the iOS host implements in Swift (security-scoped bookmark +
//! `NSFileCoordinator`), plus [`FfiVfs`], an adapter implementing the core's
//! async [`Vfs`] trait by delegating to the Swift object.
//!
//! The callback methods are *synchronous* — Swift does coordinated, possibly
//! blocking I/O. [`FfiVfs`] runs each call inside `tokio::task::spawn_blocking`
//! so that slow, network-backed file access never stalls the async executor.
//! See ../kizunashelf-ios/docs/ios-port-plan.md §5.

use std::sync::Arc;

use async_trait::async_trait;
use kizunashelf::vfs::{self, Vfs};

/// One directory entry returned by [`VaultFileSystem::read_dir`].
///
/// `len` and `modified_unix_nanos` are the index-cache change-detection
/// fingerprint; the host fills them from the same enumeration that lists the
/// directory (no per-file read). `modified_unix_nanos` is `0` when the host can't
/// report a modification time during listing (nanoseconds since the Unix epoch
/// fit in `u64` until 2554) — the cache then re-parses that file rather than
/// risking a stale hit.
#[derive(uniffi::Record)]
pub struct VfsDirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub len: u64,
    pub modified_unix_nanos: u64,
}

/// File metadata. `modified_unix_nanos` is `0` when the host cannot report a
/// modification time (nanoseconds since the Unix epoch fit in `u64` until 2554).
#[derive(uniffi::Record)]
pub struct VfsMetadata {
    pub is_dir: bool,
    pub is_file: bool,
    pub len: u64,
    pub modified_unix_nanos: u64,
}

/// One file returned by [`VaultFileSystem::read_files`].
#[derive(uniffi::Record)]
pub struct VfsFile {
    pub path: String,
    pub data: Vec<u8>,
}

/// Error surface the Swift implementation reports. Mirrors the core's
/// `vfs::VfsError`.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum VfsError {
    #[error("not found")]
    NotFound,
    #[error("already exists")]
    AlreadyExists,
    #[error("invalid path: {path}")]
    InvalidPath { path: String },
    #[error("{message}")]
    Other { message: String },
}

/// Vault filesystem implemented by the host (Swift). Paths are vault-relative,
/// forward-slash; the empty string is the vault root.
#[uniffi::export(callback_interface)]
pub trait VaultFileSystem: Send + Sync {
    fn read(&self, path: String) -> Result<Vec<u8>, VfsError>;
    fn write(&self, path: String, data: Vec<u8>) -> Result<(), VfsError>;
    fn write_atomic(&self, path: String, data: Vec<u8>) -> Result<(), VfsError>;
    fn create_dir_all(&self, path: String) -> Result<(), VfsError>;
    fn read_dir(&self, path: String) -> Result<Vec<VfsDirEntry>, VfsError>;
    /// Batch-read many files in one call, returning the files that were read
    /// successfully (missing files are skipped). This is the load hot path: it
    /// avoids one FFI + file-coordination round trip per entity.
    fn read_files(&self, paths: Vec<String>) -> Result<Vec<VfsFile>, VfsError>;
    fn metadata(&self, path: String) -> Result<VfsMetadata, VfsError>;
    fn rename(&self, from: String, to: String) -> Result<(), VfsError>;
    fn remove_file(&self, path: String) -> Result<(), VfsError>;
}

/// Adapter: implements the core async [`Vfs`] trait over a Swift
/// [`VaultFileSystem`], running each (blocking) call on a blocking thread.
pub struct FfiVfs {
    inner: Arc<dyn VaultFileSystem>,
}

impl FfiVfs {
    pub fn new(inner: Box<dyn VaultFileSystem>) -> Self {
        Self {
            inner: inner.into(),
        }
    }
}

fn into_core(error: VfsError) -> vfs::VfsError {
    match error {
        VfsError::NotFound => vfs::VfsError::NotFound,
        VfsError::AlreadyExists => vfs::VfsError::AlreadyExists,
        VfsError::InvalidPath { path } => vfs::VfsError::InvalidPath(path),
        VfsError::Other { message } => vfs::VfsError::Other(message),
    }
}

/// Runs a synchronous host call on a blocking thread, mapping errors into the
/// core's `VfsError`.
async fn run_blocking<T, F>(f: F) -> vfs::VfsResult<T>
where
    F: FnOnce() -> Result<T, VfsError> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(into_core(error)),
        Err(join) => Err(vfs::VfsError::Other(join.to_string())),
    }
}

#[async_trait]
impl Vfs for FfiVfs {
    async fn read(&self, path: &str) -> vfs::VfsResult<Vec<u8>> {
        let inner = Arc::clone(&self.inner);
        let path = vfs::normalize_relative(path)?;
        run_blocking(move || inner.read(path)).await
    }

    async fn read_files(&self, paths: &[String]) -> vfs::VfsResult<Vec<(String, Vec<u8>)>> {
        let inner = Arc::clone(&self.inner);
        let paths = paths
            .iter()
            .map(|path| vfs::normalize_relative(path))
            .collect::<vfs::VfsResult<Vec<_>>>()?;
        let files = run_blocking(move || inner.read_files(paths)).await?;
        Ok(files
            .into_iter()
            .map(|file| (file.path, file.data))
            .collect())
    }

    async fn write(&self, path: &str, data: &[u8]) -> vfs::VfsResult<()> {
        let inner = Arc::clone(&self.inner);
        let path = vfs::normalize_relative(path)?;
        let data = data.to_vec();
        run_blocking(move || inner.write(path, data)).await
    }

    async fn write_atomic(&self, path: &str, data: &[u8]) -> vfs::VfsResult<()> {
        let inner = Arc::clone(&self.inner);
        let path = vfs::normalize_relative(path)?;
        let data = data.to_vec();
        run_blocking(move || inner.write_atomic(path, data)).await
    }

    async fn create_dir_all(&self, path: &str) -> vfs::VfsResult<()> {
        let inner = Arc::clone(&self.inner);
        let path = vfs::normalize_relative(path)?;
        run_blocking(move || inner.create_dir_all(path)).await
    }

    async fn read_dir(&self, path: &str) -> vfs::VfsResult<Vec<vfs::DirEntry>> {
        let inner = Arc::clone(&self.inner);
        let path = vfs::normalize_relative(path)?;
        let entries = run_blocking(move || inner.read_dir(path)).await?;
        Ok(entries
            .into_iter()
            .map(|entry| vfs::DirEntry {
                name: entry.name,
                is_dir: entry.is_dir,
                is_file: entry.is_file,
                len: entry.len,
                modified_unix_nanos: entry.modified_unix_nanos as u128,
            })
            .collect())
    }

    async fn metadata(&self, path: &str) -> vfs::VfsResult<vfs::Metadata> {
        let inner = Arc::clone(&self.inner);
        let path = vfs::normalize_relative(path)?;
        let metadata = run_blocking(move || inner.metadata(path)).await?;
        Ok(vfs::Metadata {
            is_dir: metadata.is_dir,
            is_file: metadata.is_file,
            len: metadata.len,
            modified_unix_nanos: metadata.modified_unix_nanos as u128,
        })
    }

    async fn rename(&self, from: &str, to: &str) -> vfs::VfsResult<()> {
        let inner = Arc::clone(&self.inner);
        let from = vfs::normalize_relative(from)?;
        let to = vfs::normalize_relative(to)?;
        run_blocking(move || inner.rename(from, to)).await
    }

    async fn remove_file(&self, path: &str) -> vfs::VfsResult<()> {
        let inner = Arc::clone(&self.inner);
        let path = vfs::normalize_relative(path)?;
        run_blocking(move || inner.remove_file(path)).await
    }
}
