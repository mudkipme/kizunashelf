//! Vault-config file I/O through the VFS (read/write/validate
//! `KizunaShelf/config.yaml`), plus the relative-path validation shared by the
//! config writers and the library loader.
//!
//! The config lives in a *visible* folder so it travels with the vault under
//! sync tools that skip dot-folders (most Obsidian sync methods, including
//! official Obsidian Sync — which additionally needs its per-device "Sync all
//! other types" toggle enabled, since `.yaml` is a non-Markdown extension).

use crate::types::{EnumRole, FieldType, KizunaConfig, VaultConfig};
use crate::vfs::{Vfs, VfsError};
use anyhow::{Context, Result};
use std::path::{Component, Path};

/// Result of inspecting the vault config without conflating absence with an
/// unreadable or malformed file. Onboarding is safe only for `Missing`.
pub enum VaultConfigInspection {
    Missing,
    Ready(VaultConfig),
    Invalid(String),
}

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
        Err(VfsError::NotFound) => {
            // File Providers can occasionally surface an unavailable placeholder
            // read as `NotFound`. If metadata still sees the file, preserve it as
            // an I/O failure. Otherwise confirm the vault root itself is reachable
            // before declaring the config absent; only that shape may enter setup.
            match vfs.metadata(VAULT_CONFIG_RELATIVE_PATH).await {
                Ok(_) => anyhow::bail!(
                    "vault config exists at {VAULT_CONFIG_RELATIVE_PATH} but could not be read"
                ),
                Err(VfsError::NotFound) => {}
                Err(error) => anyhow::bail!("failed to inspect vault config: {error}"),
            }
            let root = vfs
                .metadata("")
                .await
                .map_err(|error| anyhow::anyhow!("failed to access vault root: {error}"))?;
            if !root.is_dir {
                anyhow::bail!("vault root is not a directory");
            }
            Ok(None)
        }
        Err(other) => Err(anyhow::anyhow!("failed to read vault config: {other}")),
    }
}

/// Reads and validates the vault config for a settings/onboarding gate.
///
/// A missing file is a normal first-run state. Invalid YAML/schema/path data is
/// returned as an existing-but-invalid state so clients can offer repair without
/// overwriting it. Actual VFS failures remain `Err` and must be retried rather
/// than treated as absence.
pub async fn inspect_vault_config_via_vfs(
    vfs: &dyn Vfs,
    app: &crate::types::AppConfig,
) -> Result<VaultConfigInspection> {
    let Some(raw) = read_raw_vault_config_via_vfs(vfs).await? else {
        return Ok(VaultConfigInspection::Missing);
    };
    let vault = match parse_vault_config(&raw) {
        Ok(vault) => vault,
        Err(error) => return Ok(VaultConfigInspection::Invalid(format!("{error:#}"))),
    };
    let merged = KizunaConfig::from_parts(app.clone(), vault.clone());
    if let Err(error) = validate_config_paths(&merged) {
        return Ok(VaultConfigInspection::Invalid(error.to_string()));
    }
    Ok(VaultConfigInspection::Ready(vault))
}

fn parse_vault_config(content: &str) -> Result<VaultConfig> {
    serde_yaml::from_str(content).context("invalid vault config")
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
    parse_vault_config(&raw)
}

pub(super) fn validate_config_paths(config: &KizunaConfig) -> Result<()> {
    if config.vault_root.trim().is_empty() {
        anyhow::bail!("vaultRoot cannot be empty");
    }
    validate_config_schema(config)?;
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

/// Schema-level (non-path) validation: the structural rules for field roles.
/// Kept intentionally narrow — it rejects only contradictions the engine cannot
/// act on (an `enumRole: status` on a non-enum field, or more than one status
/// field per type). It does **not** reject a `statusValues` option that isn't in
/// `enumOptions`: hand-edited/legacy values must be preserved (a stale mapping is
/// harmless — it just never matches), per the preserve-unknown-values invariant.
fn validate_config_schema(config: &KizunaConfig) -> Result<()> {
    for type_config in &config.types {
        let mut status_fields = 0usize;
        for field in &type_config.fields {
            if field.enum_role == Some(EnumRole::Status) {
                status_fields += 1;
                if field.field_type != FieldType::Enum {
                    anyhow::bail!(
                        "field '{}' on type '{}' has enumRole: status but is not an enum field",
                        field.field,
                        type_config.id
                    );
                }
            }
        }
        if status_fields > 1 {
            anyhow::bail!(
                "type '{}' declares {status_fields} status fields; only one enumRole: status field is allowed",
                type_config.id
            );
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

#[cfg(test)]
mod schema_tests {
    use super::*;
    use crate::types::{EntityTypeConfig, FieldConfig};

    fn field(name: &str, field_type: FieldType, enum_role: Option<EnumRole>) -> FieldConfig {
        FieldConfig {
            field: name.to_string(),
            field_type,
            display_name: None,
            title_language: None,
            title_role: None,
            external_fields: Vec::new(),
            enum_options: Vec::new(),
            enum_role,
            status_values: None,
            total_progress_field: None,
            date_role: None,
            season_language: None,
            external_ref: None,
            external_types: Vec::new(),
            relation_type: None,
        }
    }

    fn config_with(fields: Vec<FieldConfig>) -> KizunaConfig {
        KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            tags: None,
            types: vec![EntityTypeConfig {
                id: "anime".to_string(),
                label: "Anime".to_string(),
                icon: None,
                path: "Anime".to_string(),
                external_priority: Vec::new(),
                filename: None,
                body_sections: Vec::new(),
                log: None,
                fields,
            }],
        }
    }

    #[test]
    fn accepts_a_single_enum_status_field() {
        let config = config_with(vec![field("状态", FieldType::Enum, Some(EnumRole::Status))]);
        assert!(validate_config_schema(&config).is_ok());
    }

    #[test]
    fn rejects_status_role_on_a_non_enum_field() {
        let config = config_with(vec![field("状态", FieldType::Text, Some(EnumRole::Status))]);
        let error = validate_config_schema(&config).unwrap_err().to_string();
        assert!(error.contains("not an enum field"), "{error}");
    }

    #[test]
    fn rejects_more_than_one_status_field_per_type() {
        let config = config_with(vec![
            field("a", FieldType::Enum, Some(EnumRole::Status)),
            field("b", FieldType::Enum, Some(EnumRole::Status)),
        ]);
        let error = validate_config_schema(&config).unwrap_err().to_string();
        assert!(error.contains("only one enumRole: status field"), "{error}");
    }
}
