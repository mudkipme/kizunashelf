//! The library: vault indexing and the typed view the API serves.
//!
//! This file wires the focused submodules together and re-exports the
//! crate-facing surface:
//! - [`config_io`]: vault-config file I/O (`KizunaShelf/config.yaml`) + path validation
//! - [`read`]: the full and index-cached library reads
//! - [`parse`]: parsing one entity, the on-demand full load, field-name lookups
//! - [`write`]: the content-mutation lock and the guarded, atomic write primitives
//! - [`collation`], [`frontmatter`], [`index_cache`], [`relations`]: leaf helpers

mod collation;
mod config_io;
mod frontmatter;
mod index_cache;
mod parse;
mod read;
mod relations;
mod wikilink_rewrite;
mod write;

pub(crate) use config_io::{inspect_vault_config_text, load_vault_config_with_raw_via_vfs};
pub(crate) use index_cache::{IndexCacheContext, MemoryIndexCache};

pub use collation::{compare_string, compare_string_for_title_language};
pub use config_io::{
    ensure_config_directories_via_vfs, load_vault_config_via_vfs, parse_vault_config_strict,
    read_raw_vault_config_via_vfs, save_raw_vault_config_via_vfs, save_vault_config_via_vfs,
    VaultConfigInspection, VAULT_APP_DIR_NAME, VAULT_CONFIG_RELATIVE_PATH,
};
pub use frontmatter::{
    serialize_markdown_document, split_markdown_document, wikilink_regex, MarkdownDocument,
};
pub use read::read_library;
pub use relations::{find_target, normalized_entity_basename_index};

pub(crate) use parse::{file_revision, load_entity};
#[cfg(test)]
pub(crate) use read::read_library_cached;
pub(crate) use read::{compute_listing_fingerprint, read_library_with_fingerprint};
pub(crate) use relations::{parse_daily_note_source_id, DAILY_NOTE_RELATION_FIELD};
pub(crate) use wikilink_rewrite::{
    normalize_full_target, rewrite_backlink_wikilinks, rewrite_self_wikilinks,
    rewrite_wikilinks_matching,
};
pub(crate) use write::{
    check_revision, edit_entity_document_locked, entity_create_path, move_to_trash, parent_dir,
    rewrite_file_best_effort, write_entity_raw, write_new_entity_file, ContentMutationGuard,
    ContentMutationLock, WriteError,
};

#[cfg(test)]
mod tests;
