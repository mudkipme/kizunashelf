//! The secret-store bridge: a UniFFI callback interface `HostSecretStore` that
//! the iOS host implements in Swift (backed by the Keychain), plus
//! [`FfiSecretStore`], an adapter to the core [`CoreSecretStore`] trait.
//!
//! Holds external-provider credentials (entered by the user in Settings) and the
//! cached OAuth tokens (written by the core). Both are fast, synchronous lookups.

use std::sync::Arc;

use kizunashelf::secrets::SecretStore as CoreSecretStore;

/// Secret key-value store implemented by the host (Swift Keychain). Keys are the
/// `kizunashelf::secrets::SECRET_*` constants.
#[uniffi::export(callback_interface)]
pub trait HostSecretStore: Send + Sync {
    fn get(&self, key: String) -> Option<String>;
    fn set(&self, key: String, value: String);
}

/// Adapter: implements the core's [`CoreSecretStore`] over a Swift
/// [`HostSecretStore`].
pub struct FfiSecretStore {
    inner: Arc<dyn HostSecretStore>,
}

impl FfiSecretStore {
    pub fn new(inner: Box<dyn HostSecretStore>) -> Self {
        Self {
            inner: inner.into(),
        }
    }
}

impl CoreSecretStore for FfiSecretStore {
    fn get(&self, key: &str) -> Option<String> {
        self.inner.get(key.to_string())
    }

    fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        // Keychain writes are best-effort from the core's view (the token cache is
        // an optimization); the host swallows/raises its own errors.
        self.inner.set(key.to_string(), value.to_string());
        Ok(())
    }
}
