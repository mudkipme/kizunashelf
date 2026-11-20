//! The provider OAuth/access-token cache: a process-memory layer over a
//! durable blob in the host's secret store.

use super::unix_seconds_now;
use crate::secrets::{SecretStore, SECRET_PROVIDER_TOKENS};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Clone)]
pub(crate) struct CachedAccessToken {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_at: Instant,
    pub(crate) expires_at_unix_seconds: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskCachedAccessToken {
    access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    expires_at_unix_seconds: u64,
}

/// Provider access tokens, cached in memory and persisted through the secret
/// store (Keychain on iOS, a `0600` JSON file on desktop) so a restart doesn't
/// force a re-auth.
pub(crate) struct TokenCache {
    secret_store: Arc<dyn SecretStore>,
    memory: Mutex<HashMap<String, CachedAccessToken>>,
    /// Per-provider locks that single-flight token acquisition so a cold cache
    /// under concurrent searches does not stampede the upstream token endpoint.
    fetch_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Serializes the read-modify-write of the *shared* persisted token blob
    /// across providers. `fetch_locks` is per provider, so it does not stop
    /// provider A and provider B from both reading the same blob and the second
    /// write clobbering the first's token (a lost update that forces a needless
    /// re-auth after restart). Every persisted-token mutation holds this lock
    /// around its whole read→modify→write.
    disk_lock: Mutex<()>,
}

impl TokenCache {
    pub(super) fn new(secret_store: Arc<dyn SecretStore>) -> Self {
        Self {
            secret_store,
            memory: Mutex::new(HashMap::new()),
            fetch_locks: Mutex::new(HashMap::new()),
            disk_lock: Mutex::new(()),
        }
    }

    /// Returns the per-provider lock used to single-flight token acquisition.
    /// Callers acquire it, re-check the token cache, and only then fetch.
    pub(crate) async fn fetch_lock(&self, key: &str) -> Arc<Mutex<()>> {
        let mut locks = self.fetch_locks.lock().await;
        Arc::clone(
            locks
                .entry(key.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    /// A token for `key` with more than a minute of life left, from memory or
    /// else the persisted blob (an expired persisted token is pruned).
    pub(crate) async fn get(&self, key: &str) -> Option<CachedAccessToken> {
        {
            let tokens = self.memory.lock().await;
            if let Some(token) = tokens
                .get(key)
                .filter(|token| token.expires_at > Instant::now() + Duration::from_secs(60))
                .cloned()
            {
                return Some(token);
            }
        }
        let token = {
            // Hold the disk lock across the read and the prune-write so a
            // concurrent store for another provider can't be lost.
            let _disk = self.disk_lock.lock().await;
            let mut disk_tokens = self.read_disk_tokens().ok()?;
            let disk_token = disk_tokens.remove(key)?;
            match cached_token_from_disk(disk_token) {
                Some(token) => token,
                None => {
                    let _ = self.write_disk_tokens(&disk_tokens);
                    return None;
                }
            }
        };
        self.store_memory(key, token.clone()).await;
        Some(token)
    }

    pub(crate) async fn store(&self, key: &str, token: CachedAccessToken) -> Result<()> {
        self.store_memory(key, token.clone()).await;
        self.store_disk(key, &token).await
    }

    pub(crate) async fn invalidate(&self, key: &str) {
        self.memory.lock().await.remove(key);
        let _ = self.remove_disk(key).await;
    }

    async fn store_memory(&self, key: &str, token: CachedAccessToken) {
        self.memory.lock().await.insert(key.to_string(), token);
    }

    async fn store_disk(&self, key: &str, token: &CachedAccessToken) -> Result<()> {
        // Hold the disk lock across read→modify→write so a concurrent mutation for
        // another provider can't clobber this token in the shared blob.
        let _disk = self.disk_lock.lock().await;
        let mut tokens = self.read_disk_tokens().unwrap_or_default();
        tokens.insert(
            key.to_string(),
            DiskCachedAccessToken {
                access_token: token.access_token.clone(),
                refresh_token: token.refresh_token.clone(),
                expires_at_unix_seconds: token.expires_at_unix_seconds,
            },
        );
        self.write_disk_tokens(&tokens)
    }

    async fn remove_disk(&self, key: &str) -> Result<()> {
        let _disk = self.disk_lock.lock().await;
        let mut tokens = self.read_disk_tokens().unwrap_or_default();
        tokens.remove(key);
        self.write_disk_tokens(&tokens)
    }

    /// The persisted provider tokens. A missing/unset value is an empty map.
    fn read_disk_tokens(&self) -> Result<HashMap<String, DiskCachedAccessToken>> {
        match self.secret_store.get(SECRET_PROVIDER_TOKENS) {
            Some(raw) => {
                serde_json::from_str(&raw).context("invalid provider token cache in secret store")
            }
            None => Ok(HashMap::new()),
        }
    }

    fn write_disk_tokens(&self, tokens: &HashMap<String, DiskCachedAccessToken>) -> Result<()> {
        let raw =
            serde_json::to_string_pretty(tokens).context("failed to serialize token cache")?;
        self.secret_store.set(SECRET_PROVIDER_TOKENS, &raw)
    }
}

fn cached_token_from_disk(token: DiskCachedAccessToken) -> Option<CachedAccessToken> {
    let now = unix_seconds_now();
    if token.expires_at_unix_seconds <= now + 60 {
        return None;
    }
    Some(CachedAccessToken {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: Instant::now() + Duration::from_secs(token.expires_at_unix_seconds - now),
        expires_at_unix_seconds: token.expires_at_unix_seconds,
    })
}
