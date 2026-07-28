//! [`ConnectionSpec`] — everything needed to open a session **except the secret**.
//!
//! That exclusion is the design, not an omission. The spec is safe to persist, to
//! log and to show: it never holds a password. The password lives in Arbor's
//! keychain under this connection's id and is fetched at the moment of use through
//! a [`SecretResolver`](crate::secret::SecretResolver).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::kind::EngineKind;

/// A connection as the user configured it. Persisted; never contains a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSpec {
    /// Stable id — also the keychain key under which the secret is stored.
    pub id: String,
    /// Display name (`Production`).
    pub name: String,
    /// Human-readable role (`production`, `staging`). Shown next to the name.
    #[serde(default)]
    pub alias: String,
    pub engine: EngineKind,
    pub host: String,
    pub port: u16,
    /// Database / service name.
    pub database: String,
    pub user: String,
    /// Schema (or `search_path`) the session is pinned to. Empty = the server's
    /// default for this user.
    #[serde(default)]
    pub schema: String,
    /// Index into the shared workspace colour ramp (`--ws-color-N`) — the same
    /// identification mechanism Corvus workspaces use.
    #[serde(default)]
    pub color_idx: u8,
    /// Refuse every non-read statement **in the backend**. Not a UI hint: the
    /// session is opened in a read-only transaction mode so the *server* enforces
    /// it, and the check holds for pasted scripts and plugins too.
    #[serde(default)]
    pub read_only: bool,
    /// Whether to require TLS. `false` is normal for an on-prem server on a
    /// trusted network; a managed cloud database will refuse a plaintext session.
    #[serde(default)]
    pub tls: bool,
    /// The repository of install scripts this database is built from — an
    /// absolute path, in the platform's own form. Absent when the connection has
    /// none yet.
    ///
    /// This is the product's spine rather than a convenience: those scripts
    /// install *this* database, and the generator only makes sense with that
    /// schema in front of it. Opening the connection is what puts its repository
    /// in view, which is why the path belongs to the connection and not to a
    /// separate "recent projects" list that could point somewhere else.
    ///
    /// Safe to persist for the same reason as everything else here — a path is
    /// not a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_root: Option<String>,
    /// Engine-specific extras declared by the descriptor but with no named field
    /// here — so a new engine can ask for something without changing this type.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

impl ConnectionSpec {
    /// The keychain account under which this connection's secret is stored. The
    /// shell prefixes the namespace itself; this is the id half.
    pub fn secret_key(&self) -> &str {
        &self.id
    }
}

/// Liveness of a session, as the frontend renders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionState {
    Connected,
    /// Connected, and the server itself will refuse writes.
    ReadOnly,
    Disconnected,
    Connecting,
}

/// What a live session reports about itself once opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub id: String,
    pub state: ConnectionState,
    /// Server version banner (`PostgreSQL 16.2`), when readable.
    #[serde(default)]
    pub server_version: String,
    /// The application version read from the project's version table, when one is
    /// configured and readable. Empty otherwise — never a guess.
    #[serde(default)]
    pub db_version: String,
    /// Why the session is down, when it is. Empty while connected.
    #[serde(default)]
    pub message: String,
}
