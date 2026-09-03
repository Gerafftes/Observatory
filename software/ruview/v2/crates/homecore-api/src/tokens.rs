//! Long-lived bearer-token store.
//!
//! Closes audit findings **HC-01** and **HC-02** by replacing the
//! former "any non-empty bearer" P1 placeholder with a real token whitelist.
//!
//! P2 scope (this commit):
//! - Token set held in memory; populated at boot from env / config /
//!   programmatic registration
//! - `O(1)` `is_valid(&str) -> bool` lookup via `HashSet`
//! - No expiry, no rotation, no per-user attribution yet — P3
//!
//! Boot-time provisioning paths supported:
//! - `HOMECORE_TOKENS` env var: comma-separated bearer tokens
//! - `LongLivedTokenStore::register(token)` for programmatic insert
//!
//! Provided constructors:
//! - `LongLivedTokenStore::empty()` → no tokens accepted (use after
//!   boot to add tokens manually)
//! - `LongLivedTokenStore::from_tokens(...)` → synchronously provisions
//!   an explicit token list (useful for adapters and tests)
//! - `LongLivedTokenStore::from_env()` → reads `HOMECORE_TOKENS`,
//!   splits on commas, trims, drops empties. If the variable is unset
//!   or empty, the store remains locked (no bearer is accepted).

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Clone)]
pub struct LongLivedTokenStore {
    inner: Arc<RwLock<LongLivedTokenStoreInner>>,
}

struct LongLivedTokenStoreInner {
    tokens: HashSet<String>,
}

impl LongLivedTokenStore {
    /// Empty store. No tokens accepted. Register tokens explicitly
    /// via [`Self::register`] before exposing the API to the network.
    pub fn empty() -> Self {
        Self {
            inner: Arc::new(RwLock::new(LongLivedTokenStoreInner {
                tokens: HashSet::new(),
            })),
        }
    }

    /// Build a store from an explicit list of bearer tokens.
    ///
    /// Empty and whitespace-only values are ignored. Values not present in
    /// this list are always rejected; there is no wildcard mode.
    pub fn from_tokens<I, S>(tokens: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let store = Self::empty();
        if let Ok(mut guard) = store.inner.try_write() {
            for token in tokens {
                let token = token.as_ref().trim();
                if !token.is_empty() {
                    guard.tokens.insert(token.to_string());
                }
            }
        }
        store
    }

    /// Reads `HOMECORE_TOKENS` from the environment and registers
    /// each comma-separated value. Trims whitespace; drops empty
    /// values. If the env var is unset / empty, the store starts empty
    /// and rejects every bearer token.
    pub fn from_env() -> Self {
        std::env::var("HOMECORE_TOKENS")
            .map(|raw| Self::from_tokens(raw.split(',')))
            .unwrap_or_else(|_| Self::empty())
    }

    /// Register a token. Idempotent. Returns true if the token was
    /// new, false if it was already in the set.
    pub async fn register(&self, token: impl Into<String>) -> bool {
        let mut guard = self.inner.write().await;
        guard.tokens.insert(token.into())
    }

    /// Revoke a token. Returns true if the token was in the set.
    pub async fn revoke(&self, token: &str) -> bool {
        let mut guard = self.inner.write().await;
        guard.tokens.remove(token)
    }

    /// Check a token against the explicit store. Fast O(1) hashset lookup.
    pub async fn is_valid(&self, token: &str) -> bool {
        if token.is_empty() {
            return false;
        }
        let guard = self.inner.read().await;
        guard.tokens.contains(token)
    }

    /// Number of registered tokens. Useful for boot log lines.
    pub async fn len(&self) -> usize {
        self.inner.read().await.tokens.len()
    }
}

impl Default for LongLivedTokenStore {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_store_rejects_everything() {
        let s = LongLivedTokenStore::empty();
        assert!(!s.is_valid("anything").await);
        assert!(!s.is_valid("").await);
    }

    #[tokio::test]
    async fn registered_token_is_valid() {
        let s = LongLivedTokenStore::empty();
        s.register("hc_abc_123").await;
        assert!(s.is_valid("hc_abc_123").await);
        assert!(!s.is_valid("hc_abc_124").await);
    }

    #[tokio::test]
    async fn revoke_invalidates() {
        let s = LongLivedTokenStore::empty();
        s.register("t1").await;
        s.register("t2").await;
        assert!(s.is_valid("t1").await);
        assert!(s.revoke("t1").await);
        assert!(!s.is_valid("t1").await);
        assert!(s.is_valid("t2").await);
        assert_eq!(s.len().await, 1);
    }

    #[tokio::test]
    async fn register_is_idempotent() {
        let s = LongLivedTokenStore::empty();
        assert!(s.register("t").await);
        assert!(!s.register("t").await);
        assert_eq!(s.len().await, 1);
    }

    #[tokio::test]
    async fn empty_token_always_rejected() {
        let s = LongLivedTokenStore::empty();
        assert!(!s.is_valid("").await);
    }

    #[tokio::test]
    async fn from_tokens_accepts_only_explicit_values() {
        let s = LongLivedTokenStore::from_tokens(["listed", "  ", ""]);
        assert!(s.is_valid("listed").await);
        assert!(!s.is_valid("literally-anything").await);
    }

    #[tokio::test]
    async fn from_env_unset_is_empty() {
        // Don't set HOMECORE_TOKENS for this test
        std::env::remove_var("HOMECORE_TOKENS");
        let s = LongLivedTokenStore::from_env();
        assert_eq!(s.len().await, 0);
        assert!(!s.is_valid("anything").await);
    }
}
