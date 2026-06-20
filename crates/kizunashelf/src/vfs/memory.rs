//! In-memory [`Vfs`] for tests — no disk, deterministic mtimes (a monotonic
//! counter). Directories are tracked explicitly so empty dirs and `read_dir`
//! behave like a real filesystem.

use super::{normalize_relative, DirEntry, Metadata, Vfs, VfsError, VfsResult};
use async_trait::async_trait;
use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

#[derive(Default)]
struct State {
    files: HashMap<String, (Vec<u8>, u128)>,
    dirs: BTreeSet<String>,
    clock: u128,
}

#[derive(Default)]
pub struct InMemoryVfs {
    state: Mutex<State>,
}

impl InMemoryVfs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Test helper: seed a file (and its parent dirs) from a string.
    pub fn insert_file(&self, path: &str, contents: &str) {
        let path = normalize_relative(path).expect("valid seed path");
        let mut state = self.state.lock().unwrap();
        add_parent_dirs(&mut state.dirs, &path);
        state.clock += 1;
        let stamp = state.clock;
        state
            .files
            .insert(path, (contents.as_bytes().to_vec(), stamp));
    }

    /// Test helper: create an (empty) directory.
    pub fn insert_dir(&self, path: &str) {
        let path = normalize_relative(path).expect("valid seed path");
        let mut state = self.state.lock().unwrap();
        if !path.is_empty() {
            add_parent_dirs(&mut state.dirs, &path);
            state.dirs.insert(path);
        }
    }
}

fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

fn add_parent_dirs(dirs: &mut BTreeSet<String>, path: &str) {
    let mut current = parent_of(path);
    while !current.is_empty() {
        dirs.insert(current.to_string());
        current = parent_of(current);
    }
}

#[async_trait]
impl Vfs for InMemoryVfs {
    async fn read(&self, path: &str) -> VfsResult<Vec<u8>> {
        let path = normalize_relative(path)?;
        let state = self.state.lock().unwrap();
        state
            .files
            .get(&path)
            .map(|(bytes, _)| bytes.clone())
            .ok_or(VfsError::NotFound)
    }

    async fn write(&self, path: &str, data: &[u8]) -> VfsResult<()> {
        let path = normalize_relative(path)?;
        let mut state = self.state.lock().unwrap();
        add_parent_dirs(&mut state.dirs, &path);
        state.clock += 1;
        let stamp = state.clock;
        state.files.insert(path, (data.to_vec(), stamp));
        Ok(())
    }

    async fn write_atomic(&self, path: &str, data: &[u8]) -> VfsResult<()> {
        self.write(path, data).await
    }

    async fn create_dir_all(&self, path: &str) -> VfsResult<()> {
        let path = normalize_relative(path)?;
        let mut state = self.state.lock().unwrap();
        if !path.is_empty() {
            add_parent_dirs(&mut state.dirs, &path);
            state.dirs.insert(path);
        }
        Ok(())
    }

    async fn read_dir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let dir = normalize_relative(path)?;
        let state = self.state.lock().unwrap();
        let exists = dir.is_empty()
            || state.dirs.contains(&dir)
            || state.files.keys().any(|file| parent_of(file) == dir)
            || state.dirs.iter().any(|item| parent_of(item) == dir);
        if !exists {
            return Err(VfsError::NotFound);
        }

        let mut names: HashMap<String, DirEntry> = HashMap::new();
        for file in state.files.keys() {
            if parent_of(file) == dir {
                let name = file.rsplit('/').next().unwrap_or(file).to_string();
                names.insert(
                    name.clone(),
                    DirEntry {
                        name,
                        is_dir: false,
                        is_file: true,
                    },
                );
            }
        }
        for item in &state.dirs {
            if parent_of(item) == dir {
                let name = item.rsplit('/').next().unwrap_or(item).to_string();
                names.insert(
                    name.clone(),
                    DirEntry {
                        name,
                        is_dir: true,
                        is_file: false,
                    },
                );
            }
        }
        Ok(names.into_values().collect())
    }

    async fn metadata(&self, path: &str) -> VfsResult<Metadata> {
        let path = normalize_relative(path)?;
        let state = self.state.lock().unwrap();
        if let Some((bytes, modified)) = state.files.get(&path) {
            return Ok(Metadata {
                is_dir: false,
                is_file: true,
                len: bytes.len() as u64,
                modified_unix_nanos: *modified,
            });
        }
        let is_dir = path.is_empty()
            || state.dirs.contains(&path)
            || state.files.keys().any(|file| parent_of(file) == path);
        if is_dir {
            return Ok(Metadata {
                is_dir: true,
                is_file: false,
                len: 0,
                modified_unix_nanos: 0,
            });
        }
        Err(VfsError::NotFound)
    }

    async fn rename(&self, from: &str, to: &str) -> VfsResult<()> {
        let from = normalize_relative(from)?;
        let to = normalize_relative(to)?;
        let mut state = self.state.lock().unwrap();
        // Move a single file, or a directory subtree (prefix move).
        if let Some(entry) = state.files.remove(&from) {
            add_parent_dirs(&mut state.dirs, &to);
            state.files.insert(to, entry);
            return Ok(());
        }
        let prefix = format!("{from}/");
        let moved: Vec<String> = state
            .files
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        if moved.is_empty() && !state.dirs.contains(&from) {
            return Err(VfsError::NotFound);
        }
        for key in moved {
            let entry = state.files.remove(&key).unwrap();
            let suffix = &key[from.len()..];
            let new_key = format!("{to}{suffix}");
            add_parent_dirs(&mut state.dirs, &new_key);
            state.files.insert(new_key, entry);
        }
        state.dirs.remove(&from);
        add_parent_dirs(&mut state.dirs, &to);
        state.dirs.insert(to);
        Ok(())
    }

    async fn remove_file(&self, path: &str) -> VfsResult<()> {
        let path = normalize_relative(path)?;
        let mut state = self.state.lock().unwrap();
        state
            .files
            .remove(&path)
            .map(|_| ())
            .ok_or(VfsError::NotFound)
    }
}
