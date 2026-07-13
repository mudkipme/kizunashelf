//! Secret storage for external-provider credentials and cached OAuth tokens.
//!
//! Two kinds of secrets flow through here:
//!
//! - **Credentials** the user supplies (IGDB client id/secret, TheTVDB API key +
//!   PIN). Web reads these from `KIZUNASHELF_*` env vars; desktop and iOS read
//!   them from the OS keychain (the user enters them in Settings).
//! - The **provider token cache** (derived OAuth access tokens, managed by the
//!   core). Web persists it to a `0600` JSON file outside the vault; desktop and
//!   iOS store it in the keychain.
//!
//! Reads/writes are synchronous and expected to be cheap (Keychain access or a
//! tiny local file). The iOS implementation is a Swift-backed FFI callback.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// The cached provider tokens, as one JSON blob (a `provider -> token` map).
pub const SECRET_PROVIDER_TOKENS: &str = "provider_tokens";
pub const SECRET_IGDB_CLIENT_ID: &str = "igdb_client_id";
pub const SECRET_IGDB_CLIENT_SECRET: &str = "igdb_client_secret";
pub const SECRET_TVDB_API_KEY: &str = "tvdb_api_key";
pub const SECRET_TVDB_PIN: &str = "tvdb_pin";
pub const SECRET_TMDB_API_KEY: &str = "tmdb_api_key";
pub const SECRET_DISCOGS_TOKEN: &str = "discogs_token";
pub const SECRET_DISCOGS_CONSUMER_KEY: &str = "discogs_consumer_key";
pub const SECRET_DISCOGS_CONSUMER_SECRET: &str = "discogs_consumer_secret";
pub const SECRET_MAL_CLIENT_ID: &str = "mal_client_id";
pub const SECRET_COMICVINE_API_KEY: &str = "comicvine_api_key";
pub const SECRET_HARDCOVER_API_KEY: &str = "hardcover_api_key";
pub const SECRET_GOOGLE_BOOKS_API_KEY: &str = "google_books_api_key";
/// Batch-import source credentials (not tied to a search provider): the Trakt
/// client id (its `trakt-api-key`) and a Steam Web API key (for `GetOwnedGames`,
/// distinct from the keyless store API the `steam` provider uses).
pub const SECRET_TRAKT_CLIENT_ID: &str = "trakt_client_id";
pub const SECRET_STEAM_API_KEY: &str = "steam_api_key";

/// Maps a credential key to its `KIZUNASHELF_*` env var, e.g. `igdb_client_id` →
/// `KIZUNASHELF_IGDB_CLIENT_ID`. The provider catalog ([`crate::api`]) is the
/// single source of which keys exist; the web/desktop env reads derive the var
/// name here so adding a provider needs no change to this file.
pub fn credential_env_var(key: &str) -> String {
    format!("KIZUNASHELF_{}", key.to_ascii_uppercase())
}

/// Key-value secret store. Keys are the `SECRET_*` constants above.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
}

/// Web/native test store: credentials from `KIZUNASHELF_*` env vars (read-only),
/// with the token cache in a `0600` JSON file outside the vault. Desktop and iOS
/// inject keychain-backed stores instead.
pub struct NativeSecretStore {
    token_path: PathBuf,
}

impl NativeSecretStore {
    /// Legacy helper for callers that want a token cache derived from a host
    /// config path. Modern runtimes usually pass an explicit path with
    /// [`Self::with_token_path`].
    pub fn new(config_path: &Path) -> Self {
        let token_path = config_path
            .parent()
            .map(|parent| parent.join(".kizunashelf.tokens.json"))
            .unwrap_or_else(|| PathBuf::from(".kizunashelf.tokens.json"));
        Self { token_path }
    }

    /// Builds a store with an explicit token-cache path. Used by the env-only web
    /// runtime, which has no app config file to anchor the cache beside.
    pub fn with_token_path(token_path: PathBuf) -> Self {
        Self { token_path }
    }
}

impl SecretStore for NativeSecretStore {
    fn get(&self, key: &str) -> Option<String> {
        // The token cache is the only persisted secret in this store; every
        // other key is a web provider credential sourced from its
        // `KIZUNASHELF_*` env var (derived from the key, so new providers need no
        // edit here).
        if key == SECRET_PROVIDER_TOKENS {
            return std::fs::read_to_string(&self.token_path).ok();
        }
        env_nonempty(&credential_env_var(key))
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        // Credentials are read-only in this store (env vars); only the token
        // cache is persisted.
        if key != SECRET_PROVIDER_TOKENS {
            return Ok(());
        }
        if let Some(parent) = self.token_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.token_path, format!("{value}\n"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&self.token_path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn token_cache_round_trips_to_a_file() {
        let temp = TempDir::new().unwrap();
        let store = NativeSecretStore::new(&temp.path().join("kizunashelf.yaml"));

        assert!(store.get(SECRET_PROVIDER_TOKENS).is_none());
        store
            .set(SECRET_PROVIDER_TOKENS, "{\"igdb\":\"x\"}")
            .unwrap();
        assert_eq!(
            store.get(SECRET_PROVIDER_TOKENS).as_deref().map(str::trim),
            Some("{\"igdb\":\"x\"}")
        );
    }

    #[test]
    fn credential_keys_are_read_only_in_env_store() {
        let temp = TempDir::new().unwrap();
        let store = NativeSecretStore::new(&temp.path().join("kizunashelf.yaml"));

        // No env var set -> None, and setting a credential key is a no-op:
        // credentials come from env vars only in this store.
        assert!(store.get(SECRET_IGDB_CLIENT_ID).is_none());
        store.set(SECRET_IGDB_CLIENT_ID, "abc").unwrap();
        assert!(store.get(SECRET_IGDB_CLIENT_ID).is_none());
    }
}
