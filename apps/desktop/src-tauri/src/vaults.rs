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
    // Pre-create the taxonomy root so the library loads cleanly and the vault has
    // a visible structure before any entity is written (matches the starter
    // schema's `taxonomyRoot: Library`).
    let taxonomy_root = vault_path.join(STARTER_TAXONOMY_ROOT);
    fs::create_dir_all(&taxonomy_root)
        .with_context(|| format!("failed to create {}", taxonomy_root.display()))?;
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

/// Taxonomy root of the starter schema below; pre-created on vault creation.
const STARTER_TAXONOMY_ROOT: &str = "Taxonomy";

/// Starter schema written into a newly-created vault: a media tracker wired to
/// Bangumi, IGDB & TheTVDB. Kept in sync with the iOS app's
/// `VaultTemplate.starterYAML` and the web onboarding's "Media Library" preset.
const STARTER_VAULT_CONFIG: &str = r#"# KizunaShelf starter vault — a media tracker wired to Bangumi, IGDB & TheTVDB.
# Edit this schema anytime in Settings → Vault Schema.
taxonomyRoot: Taxonomy
assetRoot: Assets
types:
  - id: anime
    label: Anime
    icon: 📺
    path: Anime
    externalPriority: [bangumi]
    filename:
      titleLanguage: zh
      defaultTitle: true
    bodyMappings:
      - source: bangumi
        field: summary
        heading: Summary
    fields:
      - field: uid
        fieldType: id
        displayName: UID
      - field: id
        fieldType: id
        displayName: ID
      - field: title
        fieldType: title
        displayName: Title
        titleLanguage: zh
        externalFields:
          - source: bangumi
            field: name_cn
      - field: title_original
        fieldType: title
        displayName: Title (Original)
        titleRole: original
        externalFields:
          - source: bangumi
            field: name
      - field: title_en
        fieldType: title
        displayName: Title (English)
        titleLanguage: en
      - field: title_ja
        fieldType: title
        displayName: Title (Japanese)
        titleLanguage: ja
        externalFields:
          - source: bangumi
            field: name
      - field: cover_url
        fieldType: image
        displayName: Cover
        externalFields:
          - source: bangumi
            field: cover_url
      - field: state
        fieldType: enum
        displayName: State
        enumOptions: [Backlog, Watching, Playing, Reading, Completed, Paused, Dropped]
      - field: progress
        fieldType: progress
        displayName: Progress
        totalProgressField: episodes
      - field: episodes
        fieldType: totalProgress
        displayName: Episodes
      - field: rating
        fieldType: rating
        displayName: Rating
      - field: season
        fieldType: season
        displayName: Season
        dateRole: planning
        seasonLanguage: zh
      - field: release_date
        fieldType: date
        displayName: Release date
        dateRole: planning
        externalFields:
          - source: bangumi
            field: date
      - field: complete_date
        fieldType: date
        displayName: Completed date
        dateRole: completed
      - field: bangumi_url
        fieldType: externalRef
        displayName: bangumi_url
        externalRef: bangumi
      - field: franchise
        fieldType: relation
        displayName: Franchise
        relationType: franchise
  - id: drama
    label: Drama
    icon: 🎭
    path: Drama
    externalPriority: [thetvdb]
    filename:
      titleLanguage: zh
      defaultTitle: true
    bodyMappings:
      - source: thetvdb
        field: overview
        heading: Summary
    fields:
      - field: uid
        fieldType: id
        displayName: UID
      - field: id
        fieldType: id
        displayName: ID
      - field: title
        fieldType: title
        displayName: Title
        titleLanguage: zh
        externalFields:
          - source: thetvdb
            field: name
      - field: title_original
        fieldType: title
        displayName: Title (Original)
        titleRole: original
        externalFields:
          - source: thetvdb
            field: name
      - field: title_en
        fieldType: title
        displayName: Title (English)
        titleLanguage: en
      - field: title_ja
        fieldType: title
        displayName: Title (Japanese)
        titleLanguage: ja
      - field: cover_url
        fieldType: image
        displayName: Cover
        externalFields:
          - source: thetvdb
            field: cover_url
      - field: state
        fieldType: enum
        displayName: State
        enumOptions: [Backlog, Watching, Playing, Reading, Completed, Paused, Dropped]
      - field: progress
        fieldType: progress
        displayName: Progress
        totalProgressField: episodes
      - field: episodes
        fieldType: totalProgress
        displayName: Episodes
      - field: rating
        fieldType: rating
        displayName: Rating
      - field: season
        fieldType: season
        displayName: Season
        dateRole: planning
        seasonLanguage: zh
      - field: release_date
        fieldType: date
        displayName: Release date
        dateRole: planning
        externalFields:
          - source: thetvdb
            field: first_air_time
      - field: complete_date
        fieldType: date
        displayName: Completed date
        dateRole: completed
      - field: thetvdb_url
        fieldType: externalRef
        displayName: thetvdb_url
        externalRef: thetvdb
      - field: franchise
        fieldType: relation
        displayName: Franchise
        relationType: franchise
  - id: movie
    label: Movie
    icon: 🎬
    path: Movie
    externalPriority: [bangumi, thetvdb]
    filename:
      titleLanguage: zh
      defaultTitle: true
    bodyMappings:
      - source: bangumi
        field: summary
        heading: Summary
    fields:
      - field: uid
        fieldType: id
        displayName: UID
      - field: id
        fieldType: id
        displayName: ID
      - field: title
        fieldType: title
        displayName: Title
        titleLanguage: zh
        externalFields:
          - source: bangumi
            field: name_cn
      - field: title_original
        fieldType: title
        displayName: Title (Original)
        titleRole: original
        externalFields:
          - source: bangumi
            field: name
      - field: title_en
        fieldType: title
        displayName: Title (English)
        titleLanguage: en
      - field: title_ja
        fieldType: title
        displayName: Title (Japanese)
        titleLanguage: ja
        externalFields:
          - source: bangumi
            field: name
      - field: cover_url
        fieldType: image
        displayName: Cover
        externalFields:
          - source: bangumi
            field: cover_url
      - field: state
        fieldType: enum
        displayName: State
        enumOptions: [Backlog, Watching, Playing, Reading, Completed, Paused, Dropped]
      - field: progress
        fieldType: progress
        displayName: Progress
        totalProgressField: episodes
      - field: episodes
        fieldType: totalProgress
        displayName: Episodes
      - field: rating
        fieldType: rating
        displayName: Rating
      - field: release_date
        fieldType: date
        displayName: Release date
        dateRole: planning
        externalFields:
          - source: bangumi
            field: date
      - field: complete_date
        fieldType: date
        displayName: Completed date
        dateRole: completed
      - field: bangumi_url
        fieldType: externalRef
        displayName: bangumi_url
        externalRef: bangumi
      - field: thetvdb_url
        fieldType: externalRef
        displayName: thetvdb_url
        externalRef: thetvdb
      - field: franchise
        fieldType: relation
        displayName: Franchise
        relationType: franchise
  - id: games
    label: Games
    icon: 🎮
    path: Games
    externalPriority: [igdb]
    filename:
      titleLanguage: zh
      defaultTitle: true
    bodyMappings:
      - source: igdb
        field: summary
        heading: Summary
    fields:
      - field: uid
        fieldType: id
        displayName: UID
      - field: id
        fieldType: id
        displayName: ID
      - field: title
        fieldType: title
        displayName: Title
        titleLanguage: zh
        externalFields:
          - source: igdb
            field: name
      - field: title_original
        fieldType: title
        displayName: Title (Original)
        titleRole: original
        externalFields:
          - source: igdb
            field: name
      - field: title_en
        fieldType: title
        displayName: Title (English)
        titleLanguage: en
      - field: title_ja
        fieldType: title
        displayName: Title (Japanese)
        titleLanguage: ja
      - field: cover_url
        fieldType: image
        displayName: Cover
        externalFields:
          - source: igdb
            field: cover_url
      - field: state
        fieldType: enum
        displayName: State
        enumOptions: [Backlog, Watching, Playing, Reading, Completed, Paused, Dropped]
      - field: progress
        fieldType: progress
        displayName: Progress
        totalProgressField: episodes
      - field: episodes
        fieldType: totalProgress
        displayName: Episodes
      - field: rating
        fieldType: rating
        displayName: Rating
      - field: release_date
        fieldType: date
        displayName: Release date
        dateRole: planning
        externalFields:
          - source: igdb
            field: first_release_date
      - field: complete_date
        fieldType: date
        displayName: Completed date
        dateRole: completed
      - field: igdb_url
        fieldType: externalRef
        displayName: igdb_url
        externalRef: igdb
        externalTypes: [game]
      - field: franchise
        fieldType: relation
        displayName: Franchise
        relationType: franchise
home:
  title: Home
  sections:
    - id: recent-anime
      title: Recent Anime
      type: anime
      limit: 12
      sort: date:season
      direction: desc
    - id: games
      title: Games
      type: games
      limit: 12
      sort: title
      direction: asc
dailyNotes:
  paths:
    - Daily Notes
  datePattern: ^(\d{4}-\d{2}-\d{2})\.md$
  snippetMaxLength: 260
"#;
