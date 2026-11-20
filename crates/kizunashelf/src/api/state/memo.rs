//! Memoized whole-library views, keyed on library content.

use super::super::smart_lists::SmartListSummaryCache;
use crate::contract::{AnalyticsResponse, CleanupQueuesResponse};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A value memoized on the library's `content_revision`, with a single-flight
/// build lock. Several endpoints derive an expensive whole-library view
/// (analytics, cleanup queues, the tag vocabulary) that only changes when the
/// library content does; this collapses the "check the memo → take the lock →
/// re-check → build → store" protocol they all share into one place. Keying on
/// the content fingerprint (not `generated_at`) means a warm reload that re-reads
/// identical content keeps the memoized result instead of rebuilding it.
pub(crate) struct RevisionMemo<T> {
    cache: Mutex<Option<(String, Arc<T>)>>,
    build_lock: Mutex<()>,
}

impl<T> RevisionMemo<T> {
    fn new() -> Self {
        Self {
            cache: Mutex::new(None),
            build_lock: Mutex::new(()),
        }
    }

    /// The memoized value if it was built for this `content_revision`.
    async fn get(&self, content_revision: &str) -> Option<Arc<T>> {
        let cache = self.cache.lock().await;
        cache
            .as_ref()
            .filter(|(revision, _)| revision == content_revision)
            .map(|(_, value)| Arc::clone(value))
    }

    async fn store(&self, content_revision: &str, value: Arc<T>) {
        *self.cache.lock().await = Some((content_revision.to_string(), value));
    }

    /// Returns the memoized value for `content_revision`, running `build` to
    /// produce it on a miss. The build is single-flighted: concurrent first hits
    /// serialize on the lock, and the re-check after taking it means a winner's
    /// freshly stored value is reused instead of the scan running again.
    pub(crate) async fn get_or_build<F, Fut>(&self, content_revision: &str, build: F) -> Arc<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Arc<T>>,
    {
        if let Some(value) = self.get(content_revision).await {
            return value;
        }
        let _build = self.build_lock.lock().await;
        if let Some(value) = self.get(content_revision).await {
            return value;
        }
        let value = build().await;
        self.store(content_revision, Arc::clone(&value)).await;
        value
    }

    /// Fallible counterpart of [`Self::get_or_build`] for derived views whose
    /// construction performs VFS I/O. Errors are never cached; a later request
    /// gets a fresh chance to build the value.
    pub(crate) async fn get_or_try_build<F, Fut, E>(
        &self,
        content_revision: &str,
        build: F,
    ) -> Result<Arc<T>, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Arc<T>, E>>,
    {
        if let Some(value) = self.get(content_revision).await {
            return Ok(value);
        }
        let _build = self.build_lock.lock().await;
        if let Some(value) = self.get(content_revision).await {
            return Ok(value);
        }
        let value = build().await?;
        self.store(content_revision, Arc::clone(&value)).await;
        Ok(value)
    }
}

/// The router's memoized derived views. Each is a whole-library pass that only
/// changes when library content does, so repeated hits over unchanged content
/// reuse one build (single-flighted under concurrent first hits).
pub(crate) struct DerivedViews {
    /// Analytics: an expensive whole-library scan.
    pub(crate) analytics: RevisionMemo<AnalyticsResponse>,
    /// Cleanup queues: a whole-library pass that additionally stats every
    /// entity's local cover through the VFS (one round trip per cover on iOS).
    pub(crate) cleanup: RevisionMemo<CleanupQueuesResponse>,
    /// The "all tags" vocabulary. Cheap to build; memoized so autocomplete and
    /// filter requests don't re-sort the set every time.
    pub(crate) tags: RevisionMemo<Vec<String>>,
    /// Parsed smart-list filters and summary counts. Unlike the other views, the
    /// key also includes the `.base` directory listing fingerprint and the
    /// client's local date: list definitions live outside the library revision,
    /// and `today()` filters change at the day boundary.
    pub(crate) smart_list_summaries: RevisionMemo<SmartListSummaryCache>,
}

impl DerivedViews {
    pub(super) fn new() -> Self {
        Self {
            analytics: RevisionMemo::new(),
            cleanup: RevisionMemo::new(),
            tags: RevisionMemo::new(),
            smart_list_summaries: RevisionMemo::new(),
        }
    }
}
