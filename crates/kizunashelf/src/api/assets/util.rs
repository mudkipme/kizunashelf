//! Pure helpers for asset paths, URLs, and image content types — no I/O.

use crate::types::{EntityTypeConfig, FieldType, Library};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Vault-relative asset directory for an entity: `<assetRoot>/<entity path minus .md>`.
pub(crate) fn entity_asset_dir(asset_root: &str, entity_relative_path: &str) -> String {
    let stem = entity_relative_path
        .strip_suffix(".md")
        .unwrap_or(entity_relative_path);
    format!("{}/{stem}", asset_root.trim_end_matches('/'))
}

/// All local (non-remote) asset paths referenced anywhere in the library, used
/// for cross-entity collision detection.
pub(super) fn all_local_asset_paths(library: &Library) -> HashSet<String> {
    let mut set = HashSet::new();
    for entity in &library.records {
        if let Some(type_config) = library.config.type_config(&entity.summary.entity_type) {
            collect_local_asset_paths(&entity.frontmatter, type_config, &mut set);
        }
    }
    set
}

/// Local asset paths referenced by a single entity.
pub(super) fn entity_local_asset_paths(
    frontmatter: &Map<String, Value>,
    type_config: &EntityTypeConfig,
) -> HashSet<String> {
    let mut set = HashSet::new();
    collect_local_asset_paths(frontmatter, type_config, &mut set);
    set
}

fn collect_local_asset_paths(
    frontmatter: &Map<String, Value>,
    type_config: &EntityTypeConfig,
    set: &mut HashSet<String>,
) {
    for field in &type_config.fields {
        if !matches!(field.field_type, FieldType::Image | FieldType::ImageList) {
            continue;
        }
        for value in value_to_list(frontmatter.get(&field.field)) {
            let value = value.trim();
            if !value.is_empty() && !is_remote_url(value) {
                set.insert(value.to_string());
            }
        }
    }
}

/// Whether an entity has at least one remote image URL eligible for download.
pub(super) fn entity_has_remote_image(
    frontmatter: &Map<String, Value>,
    type_config: &EntityTypeConfig,
) -> bool {
    type_config
        .fields
        .iter()
        .filter(|field| matches!(field.field_type, FieldType::Image | FieldType::ImageList))
        .any(|field| {
            value_to_list(frontmatter.get(&field.field))
                .iter()
                .any(|value| is_remote_url(value))
        })
}

pub(super) fn value_to_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

pub(super) fn is_remote_url(value: &str) -> bool {
    let lowered = value.trim().to_ascii_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

pub(super) fn skip_reason(value: &str) -> &'static str {
    if value.contains("://") || value.starts_with("data:") {
        "Unsupported source URL"
    } else {
        "Already a local asset"
    }
}

pub(super) fn short_hash(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())[..12].to_string()
}

pub(super) fn extension_for_content_type(content_type: &str) -> Option<&'static str> {
    match content_type {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        "image/avif" => Some("avif"),
        "image/svg+xml" => Some("svg"),
        "image/bmp" | "image/x-ms-bmp" => Some("bmp"),
        "image/tiff" => Some("tiff"),
        _ => None,
    }
}

pub(super) fn content_type_for_extension(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "tiff" | "tif" => "image/tiff",
        _ => "application/octet-stream",
    }
}

pub(super) fn extension_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = Path::new(path).extension()?.to_str()?.to_lowercase();
    if ext.is_empty() || ext.len() > 5 {
        return None;
    }
    Some(ext)
}

pub(super) fn sniff_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && matches!(&bytes[8..12], b"avif" | b"avis") {
        return Some("avif");
    }
    if bytes.starts_with(b"BM") {
        return Some("bmp");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_asset_dir_strips_md_suffix() {
        assert_eq!(
            entity_asset_dir("Assets", "Taxonomy/Anime/Foo.md"),
            "Assets/Taxonomy/Anime/Foo"
        );
    }

    #[test]
    fn remote_url_detection() {
        assert!(is_remote_url("https://example.com/a.jpg"));
        assert!(is_remote_url("HTTP://example.com/a.jpg"));
        assert!(!is_remote_url("Assets/Anime/Foo/cover.jpg"));
        assert!(!is_remote_url("data:image/png;base64,AAAA"));
    }

    #[test]
    fn sniffs_common_image_signatures() {
        assert_eq!(
            sniff_image_ext(&[0x89, b'P', b'N', b'G', 0x0D]),
            Some("png")
        );
        assert_eq!(sniff_image_ext(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(sniff_image_ext(b"GIF89a"), Some("gif"));
        assert_eq!(sniff_image_ext(b"not an image"), None);
    }

    #[test]
    fn short_hash_is_stable_and_short() {
        let a = short_hash("https://example.com/a.jpg");
        let b = short_hash("https://example.com/a.jpg");
        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
    }
}
