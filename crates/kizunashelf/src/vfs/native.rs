//! `tokio::fs`-backed [`Vfs`] rooted at an absolute vault path. Used by the
//! desktop app and the web/api server. With the `native-vfs-watch` feature,
//! native runtimes also expose recursive change notifications through `notify`.
//! File operations keep containment enforced by [`normalize_relative`].

use super::{normalize_relative, DirEntry, Metadata, Vfs, VfsError, VfsResult};
#[cfg(feature = "native-vfs-watch")]
use super::{VfsChange, VfsChangeKind, VfsWatch};
use async_trait::async_trait;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use tokio::fs;

#[cfg(feature = "native-vfs-watch")]
use notify::event::{ModifyKind, RenameMode};
#[cfg(feature = "native-vfs-watch")]
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

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

    #[cfg(feature = "native-vfs-watch")]
    fn watch_root(&self) -> VfsResult<PathBuf> {
        let root = if self.root.is_absolute() {
            self.root.clone()
        } else {
            std::env::current_dir().map_err(map_io)?.join(&self.root)
        };
        // FSEvents reports physical paths: /var/... becomes /private/var/...,
        // and a symlinked vault uses its target. Match that spelling both when
        // subscribing and when stripping the event's root. This resolves only
        // the native watch root, not vault I/O or its lexical containment rules.
        // Never canonicalize event paths: removed/renamed files may be gone.
        #[cfg(target_os = "macos")]
        {
            root.canonicalize().map_err(map_io)
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(root)
        }
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
            // Fetch size + mtime in the same pass so the index cache has its
            // change-detection fingerprint without a separate stat per file.
            let (len, modified_unix_nanos) = match entry.metadata().await {
                Ok(metadata) => (
                    metadata.len(),
                    metadata
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                        .map(|duration| duration.as_nanos())
                        .unwrap_or_default(),
                ),
                Err(_) => (0, 0),
            };
            result.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: file_type.is_dir(),
                is_file: file_type.is_file(),
                len,
                modified_unix_nanos,
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

    #[cfg(feature = "native-vfs-watch")]
    fn supports_watch(&self) -> bool {
        true
    }

    #[cfg(feature = "native-vfs-watch")]
    fn watch(&self) -> VfsResult<VfsWatch> {
        let root = self.watch_root()?;
        let callback_root = root.clone();
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher: RecommendedWatcher =
            notify::recommended_watcher(move |result: notify::Result<Event>| {
                let change = match result {
                    Ok(event) => event_to_vfs_change(&callback_root, event),
                    // An error can mean native events were lost. Keep the stream
                    // alive and ask the consumer for a conservative rescan.
                    Err(_) => Some(VfsChange {
                        paths: Vec::new(),
                        kind: VfsChangeKind::Rescan,
                    }),
                };
                if let Some(change) = change {
                    let _ = sender.send(change);
                }
            })
            .map_err(|error| VfsError::Other(format!("failed to create vault watcher: {error}")))?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|error| VfsError::Other(format!("failed to watch vault: {error}")))?;
        Ok(VfsWatch::new(receiver, watcher))
    }
}

#[cfg(feature = "native-vfs-watch")]
fn event_to_vfs_change(root: &std::path::Path, event: Event) -> Option<VfsChange> {
    let kind = match event.kind {
        EventKind::Access(_) => return None,
        EventKind::Create(_) => VfsChangeKind::Create,
        EventKind::Remove(_) => VfsChangeKind::Remove,
        EventKind::Modify(ModifyKind::Name(
            RenameMode::Any
            | RenameMode::Both
            | RenameMode::From
            | RenameMode::To
            | RenameMode::Other,
        )) => VfsChangeKind::Rename,
        EventKind::Modify(_) => VfsChangeKind::Modify,
        EventKind::Any | EventKind::Other => VfsChangeKind::Rescan,
    };
    let mut paths: Vec<String> = event
        .paths
        .into_iter()
        .filter_map(|path| {
            path.strip_prefix(root)
                .ok()
                .map(std::path::Path::to_path_buf)
        })
        .filter_map(|path| normalize_relative(&path.to_string_lossy()).ok())
        .collect();
    paths.sort();
    paths.dedup();
    if paths.is_empty() && kind != VfsChangeKind::Rescan {
        return None;
    }
    Some(VfsChange { paths, kind })
}

#[cfg(all(test, feature = "native-vfs-watch"))]
mod tests {
    use super::*;
    use std::time::Duration;

    #[cfg(unix)]
    #[tokio::test]
    async fn watch_reports_changes_through_a_symlinked_vault_root() {
        let temp = tempfile::tempdir().unwrap();
        let physical = temp.path().join("vault");
        let alias = temp.path().join("vault-link");
        let physical_vfs = NativeVfs::new(&physical);
        physical_vfs.create_dir_all("Taxonomy/Notes").await.unwrap();
        physical_vfs
            .write("Taxonomy/Notes/Example.md", b"before")
            .await
            .unwrap();
        std::os::unix::fs::symlink(&physical, &alias).unwrap();
        let vfs = NativeVfs::new(alias);
        let mut watch = vfs.watch().unwrap();

        // An external editor may reach the same file via its physical path.
        physical_vfs
            .write("Taxonomy/Notes/Example.md", b"after")
            .await
            .unwrap();
        expect_change(&mut watch, "Taxonomy/Notes/Example.md").await;
    }

    #[test]
    fn removed_paths_are_relative_without_requiring_the_file_to_exist() {
        let root = std::path::Path::new("/vault");
        let event = Event::new(EventKind::Remove(notify::event::RemoveKind::File))
            .add_path(root.join("Taxonomy/Notes/Removed.md"))
            .add_path(PathBuf::from("/vault-other/Outside.md"));
        let change = event_to_vfs_change(root, event).unwrap();
        assert_eq!(change.paths, ["Taxonomy/Notes/Removed.md"]);
        assert_eq!(change.kind, VfsChangeKind::Remove);
    }

    async fn expect_change(watch: &mut VfsWatch, expected: &str) -> VfsChange {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let change = watch.recv().await.expect("watch remains open");
                if change.paths.iter().any(|path| path == expected) {
                    break change;
                }
            }
        })
        .await
        .expect("native watcher delivered a file event")
    }

    #[tokio::test]
    async fn watch_reports_normalized_vault_relative_changes() {
        let temp = tempfile::tempdir().unwrap();
        let vfs = NativeVfs::new(temp.path());
        vfs.create_dir_all("Taxonomy/Notes").await.unwrap();
        assert!(vfs.supports_watch());
        let mut watch = vfs.watch().unwrap();

        vfs.write("Taxonomy/Notes/Example.md", b"example")
            .await
            .unwrap();

        let changed = expect_change(&mut watch, "Taxonomy/Notes/Example.md").await;

        assert!(matches!(
            changed.kind,
            VfsChangeKind::Create | VfsChangeKind::Modify | VfsChangeKind::Rename
        ));
    }
}
