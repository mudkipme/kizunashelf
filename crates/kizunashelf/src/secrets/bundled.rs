//! Compiled-in default provider credentials for official builds.
//!
//! Only safe, read-only application identities belong in [`ENV_VALUES`] — never
//! user-scoped tokens or tightly per-key-rate-limited keys. Values can be
//! supplied at compile time through `KIZUNASHELF_BUNDLED_<UPPER_KEY>` env vars
//! (e.g. in GitHub Actions) and/or the optional gitignored
//! `src/secrets/bundled/values.rs` (copied from `values.rs.example`). Env vars
//! take precedence over the file; at runtime, any user-configured credential
//! always overrides a bundled default.

use super::{
    SECRET_BGG_API_TOKEN, SECRET_DISCOGS_CONSUMER_KEY, SECRET_DISCOGS_CONSUMER_SECRET,
    SECRET_GOOGLE_BOOKS_API_KEY, SECRET_IGDB_CLIENT_ID, SECRET_IGDB_CLIENT_SECRET,
    SECRET_MAL_CLIENT_ID, SECRET_TMDB_API_KEY, SECRET_TRAKT_CLIENT_ID, SECRET_TVDB_API_KEY,
};

mod file_values {
    include!(concat!(env!("OUT_DIR"), "/bundled_values.rs"));
}

const ENV_VALUES: &[(&str, Option<&str>)] = &[
    (
        SECRET_TMDB_API_KEY,
        option_env!("KIZUNASHELF_BUNDLED_TMDB_API_KEY"),
    ),
    (
        SECRET_TVDB_API_KEY,
        option_env!("KIZUNASHELF_BUNDLED_TVDB_API_KEY"),
    ),
    (
        SECRET_MAL_CLIENT_ID,
        option_env!("KIZUNASHELF_BUNDLED_MAL_CLIENT_ID"),
    ),
    (
        SECRET_IGDB_CLIENT_ID,
        option_env!("KIZUNASHELF_BUNDLED_IGDB_CLIENT_ID"),
    ),
    (
        SECRET_IGDB_CLIENT_SECRET,
        option_env!("KIZUNASHELF_BUNDLED_IGDB_CLIENT_SECRET"),
    ),
    (
        SECRET_BGG_API_TOKEN,
        option_env!("KIZUNASHELF_BUNDLED_BGG_API_TOKEN"),
    ),
    (
        SECRET_DISCOGS_CONSUMER_KEY,
        option_env!("KIZUNASHELF_BUNDLED_DISCOGS_CONSUMER_KEY"),
    ),
    (
        SECRET_DISCOGS_CONSUMER_SECRET,
        option_env!("KIZUNASHELF_BUNDLED_DISCOGS_CONSUMER_SECRET"),
    ),
    (
        SECRET_GOOGLE_BOOKS_API_KEY,
        option_env!("KIZUNASHELF_BUNDLED_GOOGLE_BOOKS_API_KEY"),
    ),
    (
        SECRET_TRAKT_CLIENT_ID,
        option_env!("KIZUNASHELF_BUNDLED_TRAKT_CLIENT_ID"),
    ),
];

fn file_entries() -> &'static [(&'static str, &'static str)] {
    file_values::VALUES
}

fn resolve(
    key: &str,
    env_entries: &[(&str, Option<&'static str>)],
    file_entries: &[(&str, &'static str)],
) -> Option<&'static str> {
    let (_, env_value) = env_entries
        .iter()
        .find(|(entry_key, _)| *entry_key == key)?;
    if let Some(value) = env_value.map(str::trim).filter(|value| !value.is_empty()) {
        return Some(value);
    }
    file_entries
        .iter()
        .find(|(entry_key, _)| *entry_key == key)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

pub fn get(key: &str) -> Option<&'static str> {
    resolve(key, ENV_VALUES, file_entries())
}

pub fn has_default(key: &str) -> bool {
    get(key).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{SECRET_DISCOGS_TOKEN, SECRET_HARDCOVER_API_KEY, SECRET_TVDB_PIN};

    #[test]
    fn env_overrides_file_and_ignores_blank_values() {
        let env = [
            (SECRET_TMDB_API_KEY, Some("  env-tmdb  ")),
            (SECRET_TVDB_API_KEY, Some("   ")),
            (SECRET_MAL_CLIENT_ID, None),
        ];
        let file = [
            (SECRET_TMDB_API_KEY, "file-tmdb"),
            (SECRET_TVDB_API_KEY, "file-tvdb"),
            (SECRET_MAL_CLIENT_ID, ""),
        ];

        assert_eq!(resolve(SECRET_TMDB_API_KEY, &env, &file), Some("env-tmdb"));
        assert_eq!(resolve(SECRET_TVDB_API_KEY, &env, &file), Some("file-tvdb"));
        assert_eq!(resolve(SECRET_MAL_CLIENT_ID, &env, &file), None);
    }

    #[test]
    fn non_allowlisted_keys_are_never_returned_even_if_present_in_file() {
        let env = [(SECRET_TMDB_API_KEY, None)];
        let file = [
            (SECRET_HARDCOVER_API_KEY, "secret-token"),
            (SECRET_DISCOGS_TOKEN, "personal-token"),
            (SECRET_TVDB_PIN, "subscriber-pin"),
        ];

        assert_eq!(resolve(SECRET_HARDCOVER_API_KEY, &env, &file), None);
        assert_eq!(resolve(SECRET_DISCOGS_TOKEN, &env, &file), None);
        assert_eq!(resolve(SECRET_TVDB_PIN, &env, &file), None);
    }
}
