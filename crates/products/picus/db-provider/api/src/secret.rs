//! [`SecretResolver`] — how a provider gets a password without ever owning one.
//!
//! Picus stores no password. The value lives in Arbor's keychain, shell-side, and
//! a provider asks for it at the moment it opens a session. This trait is the seam
//! that keeps the driver crates from knowing anything about the reverse channel,
//! the keychain, or Tauri: `picus-be` implements it over its host caller, and a
//! test implements it with a literal.

use zeroize::Zeroize;

use crate::error::DbResult;

/// Resolves a connection's secret on demand.
///
/// Implementations must be cheap enough to call per connect attempt and must not
/// cache the value anywhere it outlives the call — the whole point is that the
/// secret exists in this process for as long as it takes to authenticate.
pub trait SecretResolver: Send + Sync {
    /// The stored secret for `connection_id`, or `None` when none is stored (which
    /// is not an error: a trusted / peer-authenticated server needs no password).
    fn secret(&self, connection_id: &str) -> DbResult<Option<Secret>>;
}

/// A password held for the length of a connect attempt, zeroed on drop.
///
/// Deliberately not `Clone`, not `Serialize`, and its `Debug` prints a placeholder:
/// the type itself makes "and then it ended up in a log line" hard to do by
/// accident.
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// Borrow the raw value — call this as late as possible, at the driver.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // Overwrite before the allocation goes back to the allocator — `String`'s
        // own drop simply frees the bytes as they are. `zeroize` is the workspace's
        // existing answer for this (it already guards the credential-broker cache)
        // and does it without an `unsafe` block here.
        self.0.zeroize();
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// A resolver that knows no secrets — for engines that authenticate another way,
/// and for tests.
pub struct NoSecrets;

impl SecretResolver for NoSecrets {
    fn secret(&self, _connection_id: &str) -> DbResult<Option<Secret>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_value() {
        let s = Secret::new("hunter2".to_string());
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(s.expose(), "hunter2");
    }
}
