//! Secret storage for external-provider credentials and cached OAuth tokens.
//!
//! Two kinds of secrets flow through here:
//!
//! - **Credentials** the user supplies (IGDB client id/secret, TheTVDB API key +
//!   PIN). Desktop reads these from `KIZUNASHELF_*` env vars; iOS reads them from
//!   the Keychain (the user enters them in Settings).
//! - The **provider token cache** (derived OAuth access tokens, managed by the
//!   core). Desktop persists it to a `0600` JSON file next to the app config; iOS
//!   stores it in the Keychain.
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

/// Key-value secret store. Keys are the `SECRET_*` constants above.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
}

/// Desktop/web store: credentials from `KIZUNASHELF_*` env vars (read-only), the
/// token cache in a `0600` JSON file next to the app config. Behavior matches the
/// pre-`SecretStore` token file and env-var credential reads.
pub struct NativeSecretStore {
    token_path: PathBuf,
}

impl NativeSecretStore {
    /// `config_path` is the app config file; the token cache lives beside it as
    /// `.kizunashelf.tokens.json`.
    pub fn new(config_path: &Path) -> Self {
        let token_path = config_path
            .parent()
            .map(|parent| parent.join(".kizunashelf.tokens.json"))
            .unwrap_or_else(|| PathBuf::from(".kizunashelf.tokens.json"));
        Self { token_path }
    }
}

impl SecretStore for NativeSecretStore {
    fn get(&self, key: &str) -> Option<String> {
        match key {
            SECRET_PROVIDER_TOKENS => std::fs::read_to_string(&self.token_path).ok(),
            SECRET_IGDB_CLIENT_ID => env_nonempty("KIZUNASHELF_IGDB_CLIENT_ID"),
            SECRET_IGDB_CLIENT_SECRET => env_nonempty("KIZUNASHELF_IGDB_CLIENT_SECRET"),
            SECRET_TVDB_API_KEY => env_nonempty("KIZUNASHELF_TVDB_API_KEY"),
            SECRET_TVDB_PIN => env_nonempty("KIZUNASHELF_TVDB_PIN"),
            _ => None,
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        // Credentials are read-only on desktop (env vars); only the token cache is
        // persisted.
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
    fn credential_keys_are_read_only_on_desktop() {
        let temp = TempDir::new().unwrap();
        let store = NativeSecretStore::new(&temp.path().join("kizunashelf.yaml"));

        // No env var set → None, and setting a credential key is a no-op (desktop
        // credentials come from env vars only).
        assert!(store.get(SECRET_IGDB_CLIENT_ID).is_none());
        store.set(SECRET_IGDB_CLIENT_ID, "abc").unwrap();
        assert!(store.get(SECRET_IGDB_CLIENT_ID).is_none());
    }
}
