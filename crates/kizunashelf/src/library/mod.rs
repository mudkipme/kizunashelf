//! The library: vault indexing and the typed view the API serves.
//!
//! This file wires the focused submodules together and re-exports the
//! crate-facing surface:
//! - [`config_io`]: vault-config file I/O (`.kizunashelf/config.yaml`) + path validation
//! - [`read`]: the full and index-cached library reads
//! - [`parse`]: parsing one entity, the on-demand full load, field-name lookups
//! - [`edit`]: the surgical rebuild after a single in-place edit
//! - [`collation`], [`frontmatter`], [`index_cache`], [`relations`]: leaf helpers

mod collation;
mod config_io;
mod edit;
mod frontmatter;
mod index_cache;
mod parse;
mod read;
mod relations;

pub(crate) use index_cache::IndexCacheContext;

pub use collation::{compare_optional_string, compare_string, compare_string_for_title_language};
pub use config_io::{
    ensure_config_directories_via_vfs, load_vault_config_via_vfs, parse_vault_config_strict,
    read_raw_vault_config_via_vfs, save_raw_vault_config_via_vfs, save_vault_config_via_vfs,
    VAULT_CONFIG_RELATIVE_PATH,
};
pub use frontmatter::{
    serialize_markdown_document, split_markdown_document, wikilink_regex, MarkdownDocument,
};
pub use read::read_library;

pub(crate) use edit::rebuild_for_edited_entity;
pub(crate) use parse::{file_revision, load_entity};
pub(crate) use read::read_library_cached;

#[cfg(test)]
mod tests;
