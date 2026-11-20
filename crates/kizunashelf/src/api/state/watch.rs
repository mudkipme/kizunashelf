//! The optional vault watcher: turns external edits (Obsidian, sync clients)
//! into library invalidations and a change generation clients can long-poll.

use super::library_cache::LibraryCache;
use crate::vfs::{Vfs, VfsChange, VfsChangeKind, VfsWatch};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex};

/// Coalesces the create/write/rename bursts emitted by editors that save through
/// a temporary file. This bounds refresh latency while avoiding several cache
/// invalidations for one logical save.
const VAULT_WATCH_DEBOUNCE: Duration = Duration::from_millis(250);

pub(super) struct VaultWatcher {
    /// Serializes lazy watcher startup. The first library request runs inside a
    /// Tokio runtime in both Axum and Tauri, unlike synchronous router creation.
    started: Mutex<bool>,
    /// Aborts the watcher when the router (or desktop's active vault) is dropped.
    task: VaultWatchTask,
    /// Monotonic external-change generation exposed through the vault-changes
    /// long-poll endpoint, with the watch channel waking connected clients.
    change_generation: Arc<AtomicU64>,
    changes: watch::Sender<u64>,
}

impl VaultWatcher {
    pub(super) fn new() -> Self {
        let (changes, _) = watch::channel(0);
        Self {
            started: Mutex::new(false),
            task: VaultWatchTask::new(),
            change_generation: Arc::new(AtomicU64::new(0)),
            changes,
        }
    }

    /// Starts watching `vfs` once, from an async request context. A backend that
    /// does not support watching (injected mobile and in-memory VFSes) remains a
    /// no-op. Failure is non-fatal: TTL/manual refresh still works. Returns
    /// whether the watcher is running.
    pub(super) async fn ensure_started(&self, vfs: &dyn Vfs, library: &Arc<LibraryCache>) -> bool {
        if !vfs.supports_watch() {
            return false;
        }
        let mut started = self.started.lock().await;
        if *started {
            return true;
        }
        let watch = match vfs.watch() {
            Ok(watch) => watch,
            Err(error) => {
                tracing::warn!(%error, "could not watch the vault for external changes");
                return false;
            }
        };
        let library = Arc::clone(library);
        let change_generation = Arc::clone(&self.change_generation);
        let changes = self.changes.clone();
        self.task.set(tokio::spawn(async move {
            consume_vault_changes(watch, library, change_generation, changes).await;
        }));
        *started = true;
        true
    }

    /// Waits until the change generation differs from the client's cursor, or
    /// until `timeout` elapses. A cursor greater than the current generation
    /// means the router/vault was replaced, so it also returns as a change
    /// immediately and lets the client reset to the new generation. Returns
    /// `(generation, changed)`. The caller must have started the watcher.
    pub(super) async fn wait_for_change(&self, after: u64, timeout: Duration) -> (u64, bool) {
        let mut receiver = self.changes.subscribe();
        let current = *receiver.borrow_and_update();
        if current != after {
            return (current, true);
        }
        let changed = tokio::time::timeout(timeout, receiver.changed())
            .await
            .is_ok_and(|result| result.is_ok());
        let generation = *receiver.borrow_and_update();
        (generation, changed && generation != after)
    }

    #[cfg(all(test, feature = "native-vfs-watch"))]
    fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }
}

/// Owns the detached watcher task without creating a reference cycle back to
/// [`super::AppState`]. Dropping the last router-state clone drops this, which
/// aborts the task, which also drops its [`VfsWatch`] guard and releases the
/// native OS watch handles. This is particularly important on desktop, where
/// switching vaults replaces a router.
struct VaultWatchTask {
    handle: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl VaultWatchTask {
    fn new() -> Self {
        Self {
            handle: std::sync::Mutex::new(None),
        }
    }

    fn set(&self, handle: tokio::task::JoinHandle<()>) {
        *self
            .handle
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(handle);
    }
}

impl Drop for VaultWatchTask {
    fn drop(&mut self) {
        if let Some(handle) = self
            .handle
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            handle.abort();
        }
    }
}

async fn consume_vault_changes(
    mut watch: VfsWatch,
    library: Arc<LibraryCache>,
    change_generation: Arc<AtomicU64>,
    changes: watch::Sender<u64>,
) {
    while let Some(first) = watch.recv().await {
        let mut relevant = change_affects_library(&first);
        let debounce = tokio::time::sleep(VAULT_WATCH_DEBOUNCE);
        tokio::pin!(debounce);
        loop {
            tokio::select! {
                _ = &mut debounce => break,
                next = watch.recv() => match next {
                    Some(change) => relevant |= change_affects_library(&change),
                    None => break,
                },
            }
        }
        if !relevant {
            continue;
        }
        // Publish only after invalidation, so a listener awakened by this
        // generation can never refetch the just-invalidated old cache entry.
        library.invalidate().await;
        let generation = change_generation.fetch_add(1, Ordering::AcqRel) + 1;
        tracing::debug!(generation, "external vault change invalidated the library");
        changes.send_replace(generation);
    }
    // The stream only ends when the watcher itself stops, which leaves the vault
    // silently un-watched: clients stop being woken and fall back to the cache
    // TTL. Worth a line, since nothing else in the system reports it.
    tracing::warn!("the vault watcher stopped; external changes will no longer be detected");
}

/// Obsidian updates its private workspace state frequently; those files are not
/// KizunaShelf inputs. Everything else remains conservative, including unknown
/// paths and directories, because a directory rename may move an entire entity
/// subtree without producing one event per child on every native backend.
fn change_affects_library(change: &VfsChange) -> bool {
    if change.kind == VfsChangeKind::Rescan || change.paths.is_empty() {
        return true;
    }
    change.paths.iter().any(|path| {
        path != ".obsidian"
            && !path.starts_with(".obsidian/")
            && path != ".git"
            && !path.starts_with(".git/")
    })
}

#[cfg(all(test, feature = "native-vfs-watch"))]
mod tests {
    use super::super::test_support::{test_options, NOTE_CONFIG};
    use super::super::{get_library, AppState};
    use crate::secrets::NativeSecretStore;
    use crate::types::AppConfig;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn native_watch_invalidates_a_long_lived_library_cache() {
        let temp = tempfile::tempdir().unwrap();
        let config_dir = temp.path().join("KizunaShelf");
        let entity_dir = temp.path().join("Taxonomy/Notes");
        tokio::fs::create_dir_all(&config_dir).await.unwrap();
        tokio::fs::create_dir_all(&entity_dir).await.unwrap();
        tokio::fs::write(config_dir.join("config.yaml"), NOTE_CONFIG)
            .await
            .unwrap();
        let entity_path = entity_dir.join("Example.md");
        tokio::fs::write(&entity_path, "---\ntitle: Before\n---\n")
            .await
            .unwrap();

        let state = AppState::with_native_vault(
            test_options(None),
            AppConfig {
                vault_root: temp.path().to_string_lossy().to_string(),
                content_writable: Some(true),
            },
            Arc::new(NativeSecretStore::with_token_path(
                temp.path().join("tokens.json"),
            )),
        );
        let before = get_library(&state).await.unwrap();
        assert_eq!(before.records[0].frontmatter["title"], "Before");
        let mut changes = state.watcher.subscribe();

        // Bypass the VFS to model Obsidian or another editor changing the vault.
        tokio::fs::write(&entity_path, "---\ntitle: After\n---\n")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                changes.changed().await.expect("watch channel remains open");
                if *changes.borrow_and_update() > 0 {
                    break;
                }
            }
        })
        .await
        .expect("external edit advanced the vault change generation");

        let after = get_library(&state).await.unwrap();
        assert_eq!(after.records[0].frontmatter["title"], "After");
    }
}
