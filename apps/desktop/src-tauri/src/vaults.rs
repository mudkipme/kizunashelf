use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// A remembered vault: a display name and an absolute folder path. The path is
/// the identity (no separate id), so adding the same folder twice is idempotent.
#[derive(Clone, Serialize, Deserialize)]
pub struct VaultEntry {
    pub name: String,
    pub path: String,
}

/// Persisted desktop vault list + the active selection. Stored as
/// `vaults.json` in the app-data directory (never inside any vault, so it isn't
/// synced between machines).
#[derive(Default, Serialize, Deserialize)]
pub struct VaultStoreData {
    #[serde(default)]
    pub vaults: Vec<VaultEntry>,
    #[serde(default)]
    pub active: Option<String>,
}

pub fn load(path: &Path) -> VaultStoreData {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, data: &VaultStoreData) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(data)?)
        .with_context(|| format!("failed to write vault list to {}", path.display()))?;
    Ok(())
}

impl VaultStoreData {
    /// Adds (or updates the name of) a vault and makes it active.
    pub fn add(&mut self, name: String, path: String) {
        if let Some(existing) = self.vaults.iter_mut().find(|entry| entry.path == path) {
            existing.name = name;
        } else {
            self.vaults.push(VaultEntry {
                name,
                path: path.clone(),
            });
        }
        self.active = Some(path);
    }

    /// Removes a vault from the list (does not touch the folder on disk). If it
    /// was active, the most recently listed remaining vault becomes active.
    pub fn remove(&mut self, path: &str) {
        self.vaults.retain(|entry| entry.path != path);
        if self.active.as_deref() == Some(path) {
            self.active = self.vaults.last().map(|entry| entry.path.clone());
        }
    }

    pub fn set_active(&mut self, path: &str) {
        if self.vaults.iter().any(|entry| entry.path == path) {
            self.active = Some(path.to_string());
        }
    }
}

/// Creates a new vault folder at `parent/name`, seeds the starter schema, and
/// returns its absolute path. Fails if the folder already exists.
pub fn create_vault(parent: &str, name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Enter a vault name.");
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        anyhow::bail!("Vault name cannot contain path separators.");
    }
    let vault_path = Path::new(parent).join(trimmed);
    if vault_path.exists() {
        anyhow::bail!("A folder named “{trimmed}” already exists here.");
    }
    let config_dir = vault_path.join(".kizunashelf");
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("failed to create {}", config_dir.display()))?;
    fs::write(config_dir.join("config.yaml"), STARTER_VAULT_CONFIG)
        .context("failed to write starter schema")?;
    Ok(vault_path.to_string_lossy().into_owned())
}

/// Best-effort display name for a vault folder path.
pub fn name_for(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string())
}

/// Starter schema written into a newly-created vault: a ready-to-use media
/// tracker. Kept in sync with the iOS app's `VaultTemplate.starterYAML`.
const STARTER_VAULT_CONFIG: &str = r#"# KizunaShelf starter vault — a ready-to-use media tracker.
# Edit this schema anytime in Settings → Vault Schema.
taxonomyRoot: Library
assetRoot: Assets
types:
  - id: anime
    label: Anime
    icon: 📺
    path: Anime
    filename:
      defaultTitle: true
    fields:
      - field: title
        fieldType: title
        defaultTitle: true
      - field: cover
        fieldType: image
        displayName: Cover
      - field: status
        fieldType: enum
        displayName: Status
        enumOptions: [Planning, Watching, Completed, On Hold, Dropped]
      - field: rating
        fieldType: rating
        displayName: Rating
      - field: episodes
        fieldType: progress
        displayName: Episodes Watched
      - field: started
        fieldType: date
        displayName: Started
        dateRole: planning
      - field: finished
        fieldType: date
        displayName: Finished
        dateRole: completed
  - id: movie
    label: Movie
    icon: 🎬
    path: Movies
    filename:
      defaultTitle: true
    fields:
      - field: title
        fieldType: title
        defaultTitle: true
      - field: cover
        fieldType: image
        displayName: Poster
      - field: status
        fieldType: enum
        displayName: Status
        enumOptions: [Planning, Watching, Completed, Dropped]
      - field: rating
        fieldType: rating
        displayName: Rating
      - field: watched
        fieldType: date
        displayName: Watched
        dateRole: completed
  - id: book
    label: Book
    icon: 📚
    path: Books
    filename:
      defaultTitle: true
    fields:
      - field: title
        fieldType: title
        defaultTitle: true
      - field: cover
        fieldType: image
        displayName: Cover
      - field: author
        fieldType: text
        displayName: Author
      - field: status
        fieldType: enum
        displayName: Status
        enumOptions: [Planning, Reading, Completed, On Hold, Dropped]
      - field: rating
        fieldType: rating
        displayName: Rating
      - field: started
        fieldType: date
        displayName: Started
        dateRole: planning
      - field: finished
        fieldType: date
        displayName: Finished
        dateRole: completed
  - id: game
    label: Game
    icon: 🎮
    path: Games
    filename:
      defaultTitle: true
    fields:
      - field: title
        fieldType: title
        defaultTitle: true
      - field: cover
        fieldType: image
        displayName: Cover
      - field: platform
        fieldType: text
        displayName: Platform
      - field: status
        fieldType: enum
        displayName: Status
        enumOptions: [Planning, Playing, Completed, On Hold, Dropped]
      - field: rating
        fieldType: rating
        displayName: Rating
      - field: started
        fieldType: date
        displayName: Started
        dateRole: planning
      - field: finished
        fieldType: date
        displayName: Finished
        dateRole: completed
home:
  title: Home
  sections:
    - id: watching
      title: Currently Watching
      type: anime
      filters:
        - field: status
          values: [Watching]
      limit: 12
    - id: reading
      title: Reading Now
      type: book
      filters:
        - field: status
          values: [Reading]
      limit: 12
    - id: playing
      title: Now Playing
      type: game
      filters:
        - field: status
          values: [Playing]
      limit: 12
dailyNotes:
  paths:
    - Daily Notes
"#;
