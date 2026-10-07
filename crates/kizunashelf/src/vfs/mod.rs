//! Filesystem abstraction for vault content.
//!
//! All vault I/O (entities, assets, daily notes, the vault config) goes through
//! the [`Vfs`] trait using **vault-relative, forward-slash paths**. Desktop and
//! the web/api server use [`NativeVfs`] (a thin `tokio::fs` wrapper rooted at the
//! vault). The iOS host supplies a Swift-backed implementation (security-scoped
//! bookmarks + `NSFileCoordinator`) through `kizunashelf-ffi`.
//!
//! App-private I/O (desktop vault lists, web token-cache files, keychain-backed
//! secrets, iOS host state) is *not* part of this abstraction. It lives outside
//! the vault and is owned by the runtime.
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
use tokio::sync::mpsc;

pub type VfsResult<T> = Result<T, VfsError>;

/// Error surface of the VFS. Maps the `std::io::ErrorKind`s the core actually
/// branches on (`NotFound`, `AlreadyExists`), optional-capability support, and a
/// catch-all.
#[derive(Debug)]
pub enum VfsError {
    NotFound,
    AlreadyExists,
    /// The backend does not implement an optional VFS capability.
    Unsupported(String),
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
            VfsError::Unsupported(capability) => {
                write!(f, "unsupported VFS capability: {capability}")
            }
            VfsError::InvalidPath(path) => write!(f, "invalid vault path: {path}"),
            VfsError::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for VfsError {}

/// One entry returned by [`Vfs::read_dir`].
///
/// `len` and `modified_unix_nanos` are the cheap change-detection fingerprint
/// for the persistent index cache: they come from the *same* directory
/// enumeration pass (no per-file read), so a cold start can decide which entities
/// to re-parse without reading their contents. `modified_unix_nanos` is `0` when
/// the backend can't report a modification time during listing; the cache treats
/// a `0` mtime as "always re-parse" (never a cache hit), so a backend that can't
/// supply it stays correct, just without the speedup.
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub len: u64,
    pub modified_unix_nanos: u128,
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

/// Kind of change reported by a [`Vfs`] watch subscription. Backends may report
/// [`Rescan`](VfsChangeKind::Rescan) when their native event stream overflows or
/// cannot describe a change precisely; consumers must then assume anything in
/// the vault may have changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VfsChangeKind {
    Create,
    Modify,
    Remove,
    Rename,
    Rescan,
}

/// One filesystem notification. Paths use the same normalized, vault-relative
/// representation as every other VFS operation. A rescan event may have no
/// paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VfsChange {
    pub paths: Vec<String>,
    pub kind: VfsChangeKind,
}

/// Active VFS watch subscription. The private guard owns the backend watcher;
/// dropping this value stops event delivery and releases native watch handles.
pub struct VfsWatch {
    events: mpsc::UnboundedReceiver<VfsChange>,
    _guard: Box<dyn Send>,
}

impl VfsWatch {
    // Only a backend built with native watch support constructs subscriptions.
    // Keep the type and `recv` in the cross-platform contract for injected VFSes.
    #[cfg(feature = "native-vfs-watch")]
    pub(crate) fn new(
        events: mpsc::UnboundedReceiver<VfsChange>,
        guard: impl Send + 'static,
    ) -> Self {
        Self {
            events,
            _guard: Box::new(guard),
        }
    }

    /// Waits for the next change, returning `None` if the backend watcher stops.
    pub async fn recv(&mut self) -> Option<VfsChange> {
        self.events.recv().await
    }
}

/// Vault filesystem. Implementations operate on vault-relative, forward-slash
/// paths (the empty string denotes the vault root).
#[async_trait]
pub trait Vfs: Send + Sync {
    async fn read(&self, path: &str) -> VfsResult<Vec<u8>>;
    async fn write(&self, path: &str, data: &[u8]) -> VfsResult<()>;
    /// Atomically writes a complete file, replacing an existing destination.
    /// Each backend owns the replacement mechanism appropriate to its
    /// filesystem (for example, temp + rename on native filesystems or a
    /// coordinated atomic write in an iOS File Provider).
    async fn write_atomic(&self, path: &str, data: &[u8]) -> VfsResult<()>;
    async fn create_dir_all(&self, path: &str) -> VfsResult<()>;
    async fn read_dir(&self, path: &str) -> VfsResult<Vec<DirEntry>>;
    async fn metadata(&self, path: &str) -> VfsResult<Metadata>;
    async fn rename(&self, from: &str, to: &str) -> VfsResult<()>;
    async fn remove_file(&self, path: &str) -> VfsResult<()>;

    /// Whether this backend can subscribe to external vault changes. Watching is
    /// optional: injected mobile VFSes deliberately keep the default `false`.
    fn supports_watch(&self) -> bool {
        false
    }

    /// Starts a recursive watch of the vault root. The default keeps backends
    /// source-compatible while making unsupported runtimes explicit.
    fn watch(&self) -> VfsResult<VfsWatch> {
        Err(VfsError::Unsupported("filesystem watching".to_string()))
    }

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

/// Resolves a vault-relative path to the actual on-disk path when the stored
/// path and the backing filesystem disagree on Unicode composition (NFC vs NFD).
///
/// Apple filesystems return directory entries in decomposed (NFD) form, while
/// paths written into frontmatter (asset covers, entity folders derived from a
/// title) are composed (NFC). A byte-exact read of an NFC path against an NFD
/// directory entry misses even though the file exists. Entity *loading* sidesteps
/// this by enumerating the directory and reading the real on-disk name; asset
/// paths are reconstructed from frontmatter, so they need this fallback.
///
/// Walks the path segment by segment, matching each component against the
/// directory listing under NFC normalization (the same chokepoint entity/wikilink
/// matching uses). Returns the reassembled real path, or `None` if the path can't
/// be resolved to an existing entry. Case is preserved — only composition is
/// normalized.
///
/// Resolution **backtracks**: an NFC/NFD collision can leave two byte-distinct
/// directories with the same composed name (e.g. one created NFC by an asset
/// download, one NFD by the Apple filesystem), only one of which contains the
/// file. At each segment every NFC-equal candidate is tried — byte-exact matches
/// first for determinism — and the search descends into each until the *full*
/// remaining path resolves, so a dead (empty) sibling doesn't shadow the live one.
pub async fn resolve_nfc_path(vfs: &dyn Vfs, path: &str) -> VfsResult<Option<String>> {
    let normalized = normalize_relative(path)?;
    if normalized.is_empty() {
        return Ok(Some(String::new()));
    }
    let segments: Vec<String> = normalized.split('/').map(str::to_string).collect();
    resolve_nfc_segments(vfs, String::new(), &segments, 0).await
}

/// Recursive backtracking worker for [`resolve_nfc_path`]. Resolves `segments[idx..]`
/// under the already-resolved real directory `current`.
fn resolve_nfc_segments<'a>(
    vfs: &'a dyn Vfs,
    current: String,
    segments: &'a [String],
    idx: usize,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = VfsResult<Option<String>>> + Send + 'a>> {
    use unicode_normalization::UnicodeNormalization;

    Box::pin(async move {
        let entries = match vfs.read_dir(&current).await {
            Ok(entries) => entries,
            Err(VfsError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        };
        let segment = &segments[idx];
        let target: String = segment.nfc().collect();
        let last = idx + 1 == segments.len();

        let mut candidates: Vec<DirEntry> = entries
            .into_iter()
            .filter(|entry| entry.name.nfc().collect::<String>() == target)
            .collect();
        // Try a byte-exact match before composition-only matches, so an
        // unambiguous path resolves to its own bytes.
        candidates.sort_by_key(|entry| entry.name != *segment);

        for entry in candidates {
            let child = if current.is_empty() {
                entry.name.clone()
            } else {
                format!("{current}/{}", entry.name)
            };
            if last {
                return Ok(Some(child));
            }
            if entry.is_dir {
                if let Some(found) = resolve_nfc_segments(vfs, child, segments, idx + 1).await? {
                    return Ok(Some(found));
                }
            }
        }
        Ok(None)
    })
}

/// Reads a file, tolerating an NFC/NFD composition mismatch between the stored
/// path and the backing filesystem (see [`resolve_nfc_path`]). Tries a byte-exact
/// read first (the fast path) and only falls back to segment-by-segment NFC
/// resolution on `NotFound`, so the common case pays nothing.
pub async fn read_nfc_tolerant(vfs: &dyn Vfs, path: &str) -> VfsResult<Vec<u8>> {
    match vfs.read(path).await {
        Err(VfsError::NotFound) => {}
        other => return other,
    }
    match resolve_nfc_path(vfs, path).await? {
        Some(resolved) => vfs.read(&resolved).await,
        None => Err(VfsError::NotFound),
    }
}

/// Like [`Vfs::exists`] but tolerant of an NFC/NFD composition mismatch (see
/// [`resolve_nfc_path`]). Tries a byte-exact `exists` first and only walks the
/// directory listing on a miss.
pub async fn exists_nfc_tolerant(vfs: &dyn Vfs, path: &str) -> VfsResult<bool> {
    if vfs.exists(path).await? {
        return Ok(true);
    }
    Ok(resolve_nfc_path(vfs, path).await?.is_some())
}

/// Recursively collects vault-relative paths of `.md` files under `root` (a
/// vault-relative directory). A missing directory yields an empty list. Used for
/// the daily-notes walk.
pub async fn walk_markdown_files(vfs: &dyn Vfs, root: &str) -> VfsResult<Vec<String>> {
    Ok(walk_markdown_files_with_meta(vfs, root)
        .await?
        .into_iter()
        .map(|(path, _, _)| path)
        .collect())
}

/// Like [`walk_markdown_files`] but also returns each file's `(len,
/// modified_unix_nanos)` fingerprint — taken from the same `read_dir`
/// enumeration, no extra stat — so callers (the daily-note index cache) can
/// decide what to re-read without reading contents.
pub async fn walk_markdown_files_with_meta(
    vfs: &dyn Vfs,
    root: &str,
) -> VfsResult<Vec<(String, u64, u128)>> {
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
                files.push((child, entry.len, entry.modified_unix_nanos));
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

    // Katakana GA: composed (NFC, U+30AC) vs decomposed (NFD, KA + combining
    // voiced sound mark) — the shape Apple filesystems store on disk.
    const NFC_GA: &str = "\u{30AC}";
    const NFD_GA: &str = "\u{30AB}\u{3099}";

    #[tokio::test]
    async fn resolve_nfc_path_bridges_composition_across_segments() {
        let vfs = InMemoryVfs::new();
        // On-disk names are decomposed (NFD) in both a directory and the filename.
        let on_disk = format!("Assets/{NFD_GA}/cover{NFD_GA}.jpg");
        vfs.insert_file(&on_disk, "bytes");

        // The stored/requested path is composed (NFC) and byte-differs from disk.
        let requested = format!("Assets/{NFC_GA}/cover{NFC_GA}.jpg");
        assert_ne!(requested, on_disk);
        assert!(vfs.read(&requested).await.is_err());

        let resolved = resolve_nfc_path(&vfs, &requested).await.unwrap();
        assert_eq!(resolved.as_deref(), Some(on_disk.as_str()));
    }

    #[tokio::test]
    async fn resolve_nfc_path_backtracks_past_a_dead_colliding_dir() {
        // An NFC/NFD collision: two byte-distinct directories share the same
        // composed name. The NFC one is empty; the real file lives in the NFD one.
        let vfs = InMemoryVfs::new();
        let nfc_dir = format!("Assets/{NFC_GA}");
        let nfd_file = format!("Assets/{NFD_GA}/cover_url.jpg");
        vfs.insert_dir(&nfc_dir); // the dead, empty sibling
        vfs.insert_file(&nfd_file, "bytes");

        // The requested path is the composed (NFC) form — byte-exact to the empty
        // dir, so a greedy walk would dead-end there. Backtracking finds the file.
        let requested = format!("Assets/{NFC_GA}/cover_url.jpg");
        let resolved = resolve_nfc_path(&vfs, &requested).await.unwrap();
        assert_eq!(resolved.as_deref(), Some(nfd_file.as_str()));
        assert_eq!(
            read_nfc_tolerant(&vfs, &requested).await.unwrap(),
            b"bytes".to_vec()
        );
        assert!(exists_nfc_tolerant(&vfs, &requested).await.unwrap());
    }

    #[tokio::test]
    async fn resolve_nfc_path_none_when_missing() {
        let vfs = InMemoryVfs::new();
        vfs.insert_file("Assets/a/cover.jpg", "bytes");
        assert_eq!(
            resolve_nfc_path(&vfs, "Assets/a/other.jpg").await.unwrap(),
            None
        );
        // The empty (root) path always resolves to itself.
        assert_eq!(
            resolve_nfc_path(&vfs, "").await.unwrap(),
            Some(String::new())
        );
    }

    #[tokio::test]
    async fn read_and_exists_tolerate_nfc_nfd_mismatch() {
        let vfs = InMemoryVfs::new();
        let on_disk = format!("Assets/{NFD_GA}/cover.jpg");
        vfs.insert_file(&on_disk, "bytes");
        let requested = format!("Assets/{NFC_GA}/cover.jpg");

        // Byte-exact fails; the tolerant variants succeed.
        assert!(vfs.read(&requested).await.is_err());
        assert_eq!(
            read_nfc_tolerant(&vfs, &requested).await.unwrap(),
            b"bytes".to_vec()
        );
        assert!(!vfs.exists(&requested).await.unwrap());
        assert!(exists_nfc_tolerant(&vfs, &requested).await.unwrap());

        // A genuinely absent file still reports missing.
        let missing = format!("Assets/{NFC_GA}/missing.jpg");
        assert!(read_nfc_tolerant(&vfs, &missing).await.is_err());
        assert!(!exists_nfc_tolerant(&vfs, &missing).await.unwrap());
    }
}
