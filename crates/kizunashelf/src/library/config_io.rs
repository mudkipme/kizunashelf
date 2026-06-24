//! Vault-config file I/O through the VFS (read/write/validate
//! `KizunaShelf/config.yaml`), plus the relative-path validation shared by the
//! config writers and the library loader.
//!
//! The config lives in a *visible* folder so it travels with the vault under
//! sync tools that skip dot-folders (most Obsidian sync methods, including
//! official Obsidian Sync — which additionally needs its per-device "Sync all
//! other types" toggle enabled, since `.yaml` is a non-Markdown extension).

use crate::types::{KizunaConfig, VaultConfig};
use crate::vfs::{Vfs, VfsError};
use anyhow::{Context, Result};
use std::path::{Component, Path};

/// Name of the visible app folder at the vault root. Holds the config — and, in
/// future, other app-owned artifacts meant to sync with the vault (e.g. saved
/// lists). Hidden from directory autocomplete so users don't nest entity
/// collections inside it. Kept in sync with [`VAULT_CONFIG_RELATIVE_PATH`]'s
/// first segment.
pub const VAULT_APP_DIR_NAME: &str = "KizunaShelf";

/// Vault-relative location of the vault config file inside `<vaultRoot>`. Visible
/// (non-hidden) so it syncs with the vault under tools that skip dot-folders.
pub const VAULT_CONFIG_RELATIVE_PATH: &str = "KizunaShelf/config.yaml";

/// Writes config bytes to the config path, creating its parent directory first.
async fn write_config_bytes(vfs: &dyn Vfs, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = Path::new(VAULT_CONFIG_RELATIVE_PATH)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        vfs.create_dir_all(&parent.to_string_lossy())
            .await
            .map_err(|error| anyhow::anyhow!("failed to create vault config directory: {error}"))?;
    }
    vfs.write_atomic(VAULT_CONFIG_RELATIVE_PATH, bytes)
        .await
        .map_err(|error| anyhow::anyhow!("failed to write vault config: {error}"))
}

/// Writes the vault config to `KizunaShelf/config.yaml` inside the vault through
/// the VFS (the iOS settings path).
pub async fn save_vault_config_via_vfs(vfs: &dyn Vfs, config: &VaultConfig) -> Result<()> {
    let raw = serde_yaml::to_string(config).context("failed to serialize vault config")?;
    write_config_bytes(vfs, format!("{raw}\n").as_bytes()).await
}

/// Creates the taxonomy / entity-type / daily-note directories through the VFS,
/// so editing the schema from iOS can add new type paths. Containment is enforced
/// by the VFS path normalization.
pub async fn ensure_config_directories_via_vfs(config: &KizunaConfig, vfs: &dyn Vfs) -> Result<()> {
    validate_config_paths(config)?;
    let create = |path: String| async move {
        vfs.create_dir_all(&path)
            .await
            .map_err(|error| anyhow::anyhow!("failed to create directory {path}: {error}"))
    };
    create(config.taxonomy_root.clone()).await?;
    for type_config in &config.types {
        create(format!(
            "{}/{}",
            config.taxonomy_root.trim_end_matches('/'),
            type_config.path
        ))
        .await?;
    }
    if let Some(daily_notes) = &config.daily_notes {
        for path in &daily_notes.paths {
            create(path.clone()).await?;
        }
    }
    Ok(())
}

/// Writes raw vault-config YAML verbatim to `KizunaShelf/config.yaml` through the
/// VFS, preserving the user's exact formatting and comments. This is the
/// raw-editor counterpart to [`save_vault_config_via_vfs`] (which re-serializes a
/// typed config). Callers MUST validate the text with [`parse_vault_config_strict`]
/// before writing — this helper writes whatever it is given.
pub async fn save_raw_vault_config_via_vfs(vfs: &dyn Vfs, content: &str) -> Result<()> {
    write_config_bytes(vfs, content.as_bytes()).await
}

/// Reads the raw vault-config YAML text verbatim through the VFS (the raw-editor
/// path). Returns `None` when the config file doesn't exist yet.
pub async fn read_raw_vault_config_via_vfs(vfs: &dyn Vfs) -> Result<Option<String>> {
    match vfs.read_to_string(VAULT_CONFIG_RELATIVE_PATH).await {
        Ok(raw) => Ok(Some(raw)),
        Err(VfsError::NotFound) => Ok(None),
        Err(other) => Err(anyhow::anyhow!("failed to read vault config: {other}")),
    }
}

/// Strictly parses raw vault-config YAML into a [`VaultConfig`]. Unlike
/// [`load_vault_config_via_vfs`] — which tolerates extra keys for
/// forward-compatibility — this is the raw-editor validation path: it surfaces
/// type errors, missing required fields, invalid enum values, AND any field the
/// schema doesn't recognize as an error, so a typo or stray key is rejected
/// rather than silently dropped on the next save.
pub fn parse_vault_config_strict(content: &str) -> Result<VaultConfig> {
    let de = serde_yaml::Deserializer::from_str(content);
    let mut unknown = Vec::new();
    let config: VaultConfig = serde_ignored::deserialize(de, |path| unknown.push(path.to_string()))
        .context("invalid vault config")?;
    if !unknown.is_empty() {
        anyhow::bail!("unknown config field(s): {}", unknown.join(", "));
    }
    Ok(config)
}

/// Reads the vault config from inside the vault (`KizunaShelf/config.yaml`)
/// through the VFS. Works for both `NativeVfs` (desktop) and the injected iOS
/// VFS, so library loading never needs an absolute vault-config path.
pub async fn load_vault_config_via_vfs(vfs: &dyn Vfs) -> Result<VaultConfig> {
    let raw = vfs
        .read_to_string(VAULT_CONFIG_RELATIVE_PATH)
        .await
        .map_err(|error| match error {
            VfsError::NotFound => anyhow::anyhow!(
                "vault config not found at {VAULT_CONFIG_RELATIVE_PATH}; run onboarding to create one"
            ),
            other => anyhow::anyhow!("failed to read vault config: {other}"),
        })?;
    serde_yaml::from_str(&raw).context("invalid vault config")
}

pub(super) fn validate_config_paths(config: &KizunaConfig) -> Result<()> {
    if config.vault_root.trim().is_empty() {
        anyhow::bail!("vaultRoot cannot be empty");
    }
    validate_relative_config_path("taxonomyRoot", &config.taxonomy_root)?;
    for type_config in &config.types {
        validate_relative_config_path(
            &format!("type path for {}", type_config.id),
            &type_config.path,
        )?;
    }
    if let Some(daily_notes) = &config.daily_notes {
        for path in &daily_notes.paths {
            validate_relative_config_path("daily notes path", path)?;
        }
    }
    Ok(())
}

fn validate_relative_config_path(label: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("{label} cannot be empty");
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        anyhow::bail!("{label} must be relative to vaultRoot");
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        anyhow::bail!("{label} cannot contain parent directory components");
    }
    Ok(())
}
