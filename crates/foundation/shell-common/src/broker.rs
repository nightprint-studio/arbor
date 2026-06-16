//! Credential broker — the **sole** keyring holder (Model D, §D.5).
//!
//! FE, product backends and plugins never touch the keyring; they ask the
//! broker over the router. The broker keeps short-lived **access** tokens in an
//! in-memory cache (the long-lived **refresh** secret stays in the keyring),
//! with a TTL, invalidation on 401/403, and `zeroize`-on-drop. Tokens never
//! leave the broker — it hands out scoped results, not the raw secret, wherever
//! it can (token boundary, F4/D2).
//!
//! M1c scope: a compiling broker that caches one keyring entry. The real
//! refresh→access exchange (an HTTP round-trip to the provider) plugs into
//! [`CredentialBroker::access_token`] where this skeleton currently returns the
//! stored secret directly.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("no credential stored for '{0}'")]
    NotFound(String),
    #[error("keyring: {0}")]
    Keyring(String),
}

pub type Result<T> = std::result::Result<T, BrokerError>;

/// A cached short-lived access token. Wiped from memory on drop; the expiry is
/// not a secret, so it's skipped by the zeroize pass (and isn't `Zeroize`).
#[derive(Zeroize, ZeroizeOnDrop)]
struct CachedToken {
    access: String,
    #[zeroize(skip)]
    expires_at: Instant,
}

/// The keyring-backed credential broker with an in-memory access-token cache.
pub struct CredentialBroker {
    /// Keyring service namespace (e.g. `"arbor"`).
    service: String,
    /// Per-account cache of short-lived access tokens.
    cache:   Mutex<HashMap<String, CachedToken>>,
    /// How long a cached access token is considered fresh.
    ttl:     Duration,
}

impl CredentialBroker {
    pub fn new(service: impl Into<String>, ttl: Duration) -> Self {
        Self { service: service.into(), cache: Mutex::new(HashMap::new()), ttl }
    }

    /// Return a fresh access token for `account`: from the in-memory cache when
    /// still within its TTL, otherwise refreshed from the keyring and re-cached.
    ///
    /// The keyring read is where the real refresh→access exchange will live; the
    /// skeleton caches the stored secret directly.
    pub fn access_token(&self, account: &str) -> Result<String> {
        if let Ok(cache) = self.cache.lock() {
            if let Some(tok) = cache.get(account) {
                if tok.expires_at > Instant::now() {
                    return Ok(tok.access.clone());
                }
            }
        }
        let secret = self.read_keyring(account)?;
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(
                account.to_string(),
                CachedToken { access: secret.clone(), expires_at: Instant::now() + self.ttl },
            );
        }
        Ok(secret)
    }

    /// Store the long-lived refresh secret for `account` in the keyring. Drops
    /// any cached access token so the next read goes through the new secret.
    pub fn store_refresh(&self, account: &str, secret: &str) -> Result<()> {
        let entry = keyring::Entry::new(&self.service, account)
            .map_err(|e| BrokerError::Keyring(e.to_string()))?;
        entry.set_password(secret).map_err(|e| BrokerError::Keyring(e.to_string()))?;
        self.invalidate(account);
        Ok(())
    }

    /// Drop the cached access token for `account` (call on a 401/403, i.e. the
    /// token was revoked / rotated). The keyring secret is left untouched.
    pub fn invalidate(&self, account: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(account);
        }
    }

    fn read_keyring(&self, account: &str) -> Result<String> {
        let entry = keyring::Entry::new(&self.service, account)
            .map_err(|e| BrokerError::Keyring(e.to_string()))?;
        match entry.get_password() {
            Ok(secret) => Ok(secret),
            Err(keyring::Error::NoEntry) => Err(BrokerError::NotFound(account.to_string())),
            Err(e) => Err(BrokerError::Keyring(e.to_string())),
        }
    }
}
