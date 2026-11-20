//! Router state: the per-router context every handler receives.
//!
//! [`AppState`] is a cheap-to-clone bundle of `Arc`s. It holds the runtime's
//! inputs (options, app config, vault filesystem, secret store) and one handle
//! per subsystem, each in its own module:
//! - [`library_cache`]: the cached, generation-guarded [`Library`] snapshot
//!   ([`get_library`], [`AppState::invalidate_cache`])
//! - [`watch`]: the optional native vault watcher and change long-poll
//! - [`tokens`]: the provider access-token cache ([`AppState::tokens`])
//! - [`memo`]: memoized whole-library views ([`AppState::views`])
//! - [`jobs`]: asset-download and import job tables ([`AppState::jobs`])
//!
//! The vault-content mutation lock itself lives in [`crate::library`] with the
//! write primitives it guards; this module only decides which routers share it.

mod jobs;
mod library_cache;
mod memo;
mod tokens;
mod watch;

pub(crate) use jobs::{AssetJobRecord, JobRegistry};
pub(crate) use library_cache::{get_library, revalidate_library};
pub(crate) use memo::DerivedViews;
pub(crate) use tokens::{CachedAccessToken, TokenCache};

use super::error::ApiError;
use crate::library::{ContentMutationGuard, ContentMutationLock};
use crate::secrets::SecretStore;
use crate::types::{AppConfig, Library};
use crate::vfs::{NativeVfs, Vfs};
use library_cache::LibraryCache;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use watch::VaultWatcher;

/// Native hosts can open a foreground and a headless router for the same vault.
/// Coordinate their commits and invalidate cached reads by the stable host identity,
/// never the display name or index-cache directory. Weak entries don't retain closed vaults indefinitely.
#[derive(Default)]
struct VaultCoordination {
    /// Serializes vault content mutations across routers sharing a host
    /// identity. Atomic VFS writes prevent torn files; this lock additionally
    /// prevents two in-process read-check-write requests from both accepting the
    /// same revision and silently overwriting one another.
    mutation: ContentMutationLock,
    /// The library-cache generation, shared so one router's write invalidates
    /// every same-identity router's cached library.
    generation: Arc<AtomicU64>,
}

/// The coordination for `identity`: shared with every live router that has the
/// same identity, or router-local when there is none.
fn coordination_for_identity(identity: Option<&str>) -> Arc<VaultCoordination> {
    let Some(identity) = identity else {
        return Arc::new(VaultCoordination::default());
    };
    static LOCKS: OnceLock<std::sync::Mutex<HashMap<String, Weak<VaultCoordination>>>> =
        OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    locks.retain(|_, value| value.strong_count() > 0);
    if let Some(lock) = locks.get(identity).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(VaultCoordination::default());
    locks.insert(identity.to_owned(), Arc::downgrade(&lock));
    lock
}

#[derive(Clone)]
pub struct ApiOptions {
    pub cache_ttl: Duration,
    pub web_dist_path: Option<PathBuf>,
    pub settings_writable: bool,
    pub content_writable: bool,
    /// Whether host-driven asset ingest is permitted — the `plan`/`ingest`
    /// endpoints that read (and delete) a client-supplied *absolute host path*
    /// (`AssetIngestRequest::source_path`) with raw `std::fs`, bypassing the VFS.
    /// Only the in-process host (iOS), which hands the core a sandboxed temp-file
    /// path over the FFI tunnel, may set this. The network HTTP server and desktop
    /// leave it `false` so an untrusted caller can't reach that surface.
    pub host_asset_ingest: bool,
    /// Host directory for the *persistent* index cache (NOT inside the vault — a
    /// real app-container path the in-process core can touch with `std::fs`).
    /// `None` keeps the index cache in process memory instead (lost on restart,
    /// but still skips re-parsing unchanged files between reloads); a real cold
    /// start re-parses the whole vault. A pure optimization; see
    /// [`crate::library`]'s index cache.
    pub index_cache_dir: Option<PathBuf>,
    /// Durable host-private import journals, outside the vault and cache.
    /// Honored only for in-process hosts with a stable vault identity.
    pub import_state_dir: Option<PathBuf>,
    /// Stable host-provided identity for the active vault. Native hosts that use
    /// a display label in `AppConfig::vault_root` (notably iOS) set this to the
    /// remembered vault UUID so two same-named vaults never share an index cache.
    /// Also coordinates content commits across same-identity native routers.
    /// Network/desktop runtimes leave it `None` and use the real vault root.
    pub index_cache_identity: Option<String>,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) options: ApiOptions,
    /// One stable vault filesystem for the router's lifetime. Web/desktop own a
    /// [`NativeVfs`]; iOS injects its Swift-backed VFS. Keeping a stable instance
    /// gives the native watcher the same lifecycle as the active router/vault.
    vault_fs: Arc<dyn Vfs>,
    /// Native runtimes interpret `AppConfig::vault_root` as a real host path;
    /// injected runtimes such as iOS may use it only as a display label.
    native_vault: bool,
    /// Inline app config (the vault root + write mode). Every runtime owns this
    /// server-side and passes it in: env vars (web), the native vault switcher
    /// (desktop), or `@AppStorage` (iOS). There is no app config file.
    app_config: AppConfig,
    /// Provider credentials. Env vars + a file on web/desktop; iOS injects a
    /// Keychain-backed store.
    secret_store: Arc<dyn SecretStore>,
    http_client: reqwest::Client,
    coordination: Arc<VaultCoordination>,
    library: Arc<LibraryCache>,
    /// Dropped with the last state clone, which stops the watcher task.
    watcher: Arc<VaultWatcher>,
    tokens: Arc<TokenCache>,
    views: Arc<DerivedViews>,
    jobs: Arc<JobRegistry>,
}

/// Base builder both outbound HTTP clients start from, so shared settings
/// (connect timeout, user agent) can't drift apart. The two clients then set
/// their deliberate differences on top: [`AppState::http_client`] disables
/// redirects (SSRF re-validation) and has no overall timeout (streams assets);
/// the provider client (`external::external_client`) caps whole requests and
/// follows redirects.
pub(in crate::api) fn base_http_client() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("KizunaShelf/", env!("CARGO_PKG_VERSION")))
}

impl AppState {
    /// Builds state with an injected VFS (iOS) and inline app config.
    pub(crate) fn with_vault(
        options: ApiOptions,
        vault_fs: Arc<dyn Vfs>,
        app_config: AppConfig,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        Self::new(options, vault_fs, false, app_config, secret_store)
    }

    /// Native web/desktop constructor. The VFS is created once here rather than
    /// afresh for every request so its watch subscription follows router lifetime.
    pub(crate) fn with_native_vault(
        options: ApiOptions,
        app_config: AppConfig,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        let vault_fs: Arc<dyn Vfs> = Arc::new(NativeVfs::new(&app_config.vault_root));
        Self::new(options, vault_fs, true, app_config, secret_store)
    }

    fn new(
        options: ApiOptions,
        vault_fs: Arc<dyn Vfs>,
        native_vault: bool,
        app_config: AppConfig,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        let http_client = base_http_client()
            // Asset downloads follow redirects manually (see assets.rs) so each
            // hop's destination can be re-validated against the SSRF guard; never
            // let reqwest follow a redirect into an unvalidated host. No overall
            // request timeout either: this client streams large asset bodies.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let coordination = coordination_for_identity(options.index_cache_identity.as_deref());
        let library = LibraryCache::new(Arc::clone(&coordination.generation));
        let import_recovery = super::import::recovery::ImportRecovery::open(&options);
        Self {
            options,
            vault_fs,
            native_vault,
            app_config,
            tokens: Arc::new(TokenCache::new(Arc::clone(&secret_store))),
            secret_store,
            http_client,
            coordination,
            library: Arc::new(library),
            watcher: Arc::new(VaultWatcher::new()),
            views: Arc::new(DerivedViews::new()),
            jobs: Arc::new(JobRegistry::new(import_recovery)),
        }
    }

    /// Shared HTTP client (connection-pooled) for outbound requests such as
    /// asset downloads.
    pub(crate) fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Returns the router's stable vault filesystem. The parameter remains at
    /// call sites as a useful assertion that native requests did not drift to a
    /// different configured root without rebuilding the router.
    pub(crate) fn vault_vfs(&self, vault_root: &str) -> Arc<dyn Vfs> {
        debug_assert!(
            !self.native_vault || vault_root == self.app_config.vault_root,
            "native router requested a VFS for a different vault root"
        );
        Arc::clone(&self.vault_fs)
    }

    /// The secret store, used by external providers to read their credentials.
    pub(crate) fn secret_store(&self) -> &Arc<dyn SecretStore> {
        &self.secret_store
    }

    /// The inline app config (vault root + write mode), owned by the runtime.
    /// Unlike [`get_library`] it does not require a vault config to exist, so it
    /// is usable before a schema is created.
    pub(crate) fn app_config(&self) -> AppConfig {
        self.app_config.clone()
    }

    /// The provider access-token cache.
    pub(crate) fn tokens(&self) -> &TokenCache {
        &self.tokens
    }

    /// The memoized whole-library views (analytics, cleanup, tags, smart lists).
    pub(crate) fn views(&self) -> &DerivedViews {
        &self.views
    }

    /// The asset-download and import job tables.
    pub(crate) fn jobs(&self) -> &JobRegistry {
        &self.jobs
    }

    /// Drops the cached library (for every router sharing this vault's host
    /// identity) so the next read reloads. Call after every vault write.
    pub(crate) async fn invalidate_cache(&self) {
        self.library.invalidate().await;
    }

    /// Acquires the vault-content mutation lock (shared with every router that
    /// has the same host identity). Callers hold this across the authoritative
    /// fresh read, revision check, and every related write. Network/provider
    /// preparation should happen before taking it.
    ///
    /// The returned guard is also the proof the vault write helpers in
    /// [`crate::library`] (`write_entity_raw`, `write_new_entity_file`,
    /// `move_to_trash`, …) demand, so a handler that forgets the lock fails to
    /// compile instead of racing. The lock is not reentrant: release it before
    /// calling a helper that acquires it itself (e.g. `edit_entity_document`).
    pub(crate) async fn content_mutation_lock(&self) -> ContentMutationGuard<'_> {
        self.coordination.mutation.lock().await
    }

    /// Starts the optional vault watcher once. See [`VaultWatcher::ensure_started`].
    async fn ensure_vault_watch(&self) -> bool {
        self.watcher
            .ensure_started(self.vault_fs.as_ref(), &self.library)
            .await
    }

    pub(crate) fn vault_watch_enabled(&self) -> bool {
        self.vault_fs.supports_watch()
    }

    /// Long-polls for an external vault change after the client's `after`
    /// cursor. Returns `(watching, generation, changed)`; `watching` is `false`
    /// (and the call returns immediately) when this backend can't watch.
    pub(crate) async fn wait_for_vault_change(
        &self,
        after: u64,
        timeout: Duration,
    ) -> (bool, u64, bool) {
        if !self.ensure_vault_watch().await {
            return (false, 0, false);
        }
        let (generation, changed) = self.watcher.wait_for_change(after, timeout).await;
        (true, generation, changed)
    }
}

pub(crate) fn unix_seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

pub(crate) fn content_writes_enabled(state: &AppState, library: &Library) -> bool {
    state.options.content_writable && library.config.content_writable.unwrap_or(true)
}

/// Loads the library and rejects the request with 403 when content writes are
/// disabled. The single entry point every mutation/asset-download handler uses to
/// gate writes, so the read-only check (and its error) lives in one place.
pub(crate) async fn require_content_writes(state: &AppState) -> Result<Arc<Library>, ApiError> {
    let library = get_library(state).await?;
    if !content_writes_enabled(state, &library) {
        return Err(ApiError::forbidden("Content writes are disabled"));
    }
    Ok(library)
}

/// Gate for the host-driven download endpoints (`plan`/`ingest`), which read and
/// delete a client-supplied absolute host path. Permitted only on the in-process
/// host runtime (iOS); the network HTTP server and desktop reject it so that
/// raw-`std::fs` surface is never reachable by an untrusted client. See
/// [`ApiOptions::host_asset_ingest`].
pub(crate) fn require_host_asset_ingest(state: &AppState) -> Result<(), ApiError> {
    if state.options.host_asset_ingest {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "Host-driven asset ingest is not available on this runtime",
        ))
    }
}

#[cfg(test)]
mod test_support {
    use super::*;
    use crate::secrets::NativeSecretStore;
    use crate::vfs::InMemoryVfs;

    /// A one-type vault schema for state tests.
    pub(super) const NOTE_CONFIG: &str = "taxonomyRoot: Taxonomy\ntypes:\n  - id: note\n    label: Note\n    path: Notes\n    fields:\n      - field: title\n        fieldType: title\n";

    pub(super) fn test_options(identity: Option<String>) -> ApiOptions {
        ApiOptions {
            cache_ttl: Duration::from_secs(60 * 60),
            web_dist_path: None,
            settings_writable: true,
            content_writable: true,
            host_asset_ingest: false,
            index_cache_dir: None,
            import_state_dir: None,
            index_cache_identity: identity,
        }
    }

    pub(super) fn test_state(vfs: Arc<InMemoryVfs>) -> AppState {
        test_state_with_identity(vfs, None)
    }

    pub(super) fn test_state_with_identity(
        vfs: Arc<InMemoryVfs>,
        identity: Option<String>,
    ) -> AppState {
        AppState::with_vault(
            test_options(identity),
            vfs as Arc<dyn Vfs>,
            AppConfig {
                vault_root: "test-vault".to_string(),
                content_writable: Some(true),
            },
            Arc::new(NativeSecretStore::with_token_path(PathBuf::from(
                "/tmp/kizunashelf-state-test-tokens.json",
            ))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn host_identity_coordinates_only_the_matching_vault() {
        let first = coordination_for_identity(Some("host-lock-test:first"));
        let held = first.mutation.lock().await;
        let same = coordination_for_identity(Some("host-lock-test:first"));
        assert!(same.mutation.is_held());
        let other = coordination_for_identity(Some("host-lock-test:other"));
        assert!(!other.mutation.is_held());
        assert!(!coordination_for_identity(None).mutation.is_held());
        drop(held);
        assert!(!same.mutation.is_held());
    }
}
