use anyhow::Result;
use keyring::Entry;
use kizunashelf::secrets::SecretStore;

/// Keychain service name under which all KizunaShelf secrets are stored.
const SERVICE: &str = "me.mudkip.kizunashelf-desktop";

/// A [`SecretStore`] backed by the OS-native secret manager via `keyring`:
/// macOS Keychain, Windows Credential Manager, or the Linux Secret Service
/// (gnome-keyring / KWallet). Both the user-entered provider credentials and the
/// derived OAuth token cache live here, keyed by the `SECRET_*` constants.
///
/// If no secret service is available (e.g. a headless Linux box with no
/// gnome-keyring/KWallet running), reads return `None` and writes return an
/// error — surfaced to the user rather than silently dropped.
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    pub fn new() -> Self {
        Self
    }
}

impl SecretStore for KeyringSecretStore {
    fn get(&self, key: &str) -> Option<String> {
        let entry = Entry::new(SERVICE, key).ok()?;
        match entry.get_password() {
            Ok(value) => Some(value),
            Err(keyring::Error::NoEntry) => None,
            Err(_) => None,
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        let entry = Entry::new(SERVICE, key)?;
        // An empty value clears the secret (used when the user blanks a field).
        if value.is_empty() {
            return match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(error.into()),
            };
        }
        entry.set_password(value)?;
        Ok(())
    }
}
