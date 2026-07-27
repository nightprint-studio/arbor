//! [`DbError`] — the one error type every provider operation returns.
//!
//! These strings cross the Model-D seam as `Display` (`docs/backend-architecture.md`:
//! "the error strings ARE the contract"), so they are written to be read by a
//! person in a toast, not parsed by a machine. A variant exists when the frontend
//! would *do something different* about it — otherwise it is [`DbError::Internal`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Failure of any [`DbProvider`](crate::provider::DbProvider) /
/// [`DbSession`](crate::provider::DbSession) operation.
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum DbError {
    /// The engine has no such concept (Oracle packages on PostgreSQL, and so on).
    /// Read the descriptor's capabilities instead of discovering this at runtime.
    #[error("{engine} does not support {feature}")]
    Unsupported { engine: String, feature: String },

    /// No engine is registered for this kind — today: any attempt to *connect* to
    /// Oracle. Deliberately distinct from `Unsupported`: the engine is real and
    /// fully supported for scripts, it just has no driver yet.
    #[error("no driver for {engine} — Picus reads and writes its scripts, but cannot connect to it yet")]
    NoDriver { engine: String },

    /// The connection has no stored secret and one is required.
    #[error("no password stored for this connection")]
    SecretMissing,

    /// The server rejected the credentials.
    #[error("authentication failed: {0}")]
    AuthFailed(String),

    /// Could not reach or open the database.
    #[error("cannot connect: {0}")]
    Connect(String),

    /// A write was attempted on a connection the user marked read-only.
    ///
    /// This is refused in the **backend**, never merely hidden in the UI — the
    /// point of the flag is that it holds even when the statement came from a
    /// pasted script or a plugin.
    #[error("this connection is read-only: {statement} was refused")]
    ReadOnly { statement: String },

    /// The object does not exist (or is not visible to this user).
    #[error("not found: {0}")]
    NotFound(String),

    /// The server rejected the SQL. `position` is a 1-based byte offset into the
    /// statement when the server reports one, so the editor can put the squiggle
    /// in the right place instead of underlining everything.
    #[error("{message}")]
    Sql { message: String, code: Option<String>, position: Option<u32> },

    /// The session is gone (server restarted, network dropped, idle timeout).
    #[error("the connection was lost: {0}")]
    Disconnected(String),

    /// The user cancelled the running statement.
    #[error("cancelled")]
    Cancelled,

    /// Anything with no better home.
    #[error("internal error: {0}")]
    Internal(String),
}

impl DbError {
    /// Convenience for the common "this engine has no such concept" case.
    pub fn unsupported(engine: impl std::fmt::Display, feature: impl std::fmt::Display) -> Self {
        Self::Unsupported { engine: engine.to_string(), feature: feature.to_string() }
    }
}

/// Result alias used across the provider surface.
pub type DbResult<T> = Result<T, DbError>;
