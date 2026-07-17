//! The library: vault indexing and the typed view the API serves.
//!
//! This file wires the focused submodules together and re-exports the
//! crate-facing surface:
//! - [`config_io`]: vault-config file I/O (`KizunaShelf/config.yaml`) + path validation
//! - [`read`]: the full and index-cached library reads
//! - [`parse`]: parsing one entity, the on-demand full load, field-name lookups
//! - [`collation`], [`frontmatter`], [`index_cache`], [`relations`]: leaf helpers

mod collation;
mod config_io;
mod frontmatter;
mod index_cache;
mod parse;
mod read;
mod relations;
mod wikilink_rewrite;

pub(crate) use index_cache::{IndexCacheContext, MemoryIndexCache};

pub use collation::{compare_optional_string, compare_string, compare_string_for_title_language};
pub use config_io::{
    ensure_config_directories_via_vfs, inspect_vault_config_via_vfs, load_vault_config_via_vfs,
    parse_vault_config_strict, read_raw_vault_config_via_vfs, save_raw_vault_config_via_vfs,
    save_vault_config_via_vfs, VaultConfigInspection, VAULT_APP_DIR_NAME,
    VAULT_CONFIG_RELATIVE_PATH,
};
pub use frontmatter::{
    serialize_markdown_document, split_markdown_document, wikilink_regex, MarkdownDocument,
};
pub use read::read_library;
pub use relations::{find_target, normalized_entity_basename_index};

pub(crate) use parse::{file_revision, load_entity};
pub(crate) use read::{compute_listing_fingerprint, read_library_cached};
pub(crate) use relations::{parse_daily_note_source_id, DAILY_NOTE_RELATION_FIELD};
pub(crate) use wikilink_rewrite::{
    normalize_full_target, rewrite_backlink_wikilinks, rewrite_self_wikilinks,
    rewrite_wikilinks_matching,
};

#[cfg(test)]
mod tests;
