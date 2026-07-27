//! Connection storage and the live-session pool.
//!
//! Two different things, deliberately separate:
//!
//! * the **store** is what the user configured — persisted to
//!   `arbor/profiles/<active>/picus/connections.toml`, and safe to write there
//!   precisely because a [`ConnectionSpec`] never holds a password;
//! * the **pool** is what is currently open — in memory only, gone when the backend
//!   stops, which is the correct lifetime for a database session.
//!
//! Configuring a connection and opening it are separate acts, and the UI reflects
//! that: a connection can exist, be listed and be edited without any server being
//! reachable.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use picus_db_api::prelude::{ConnectionSpec, DbSession};
use serde::{Deserialize, Serialize};

/// The persisted file's shape. A wrapper struct rather than a bare array because
/// TOML has no top-level array document, and because it leaves room for a
/// `last_used` alongside the list later.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionFile {
    /// Every configured connection, in display order.
    #[serde(rename = "connection")]
    pub connections: Vec<ConnectionSpec>,
}

/// picus's connection list: `arbor/profiles/<active>/picus/connections.toml`.
///
/// Separate from `config.toml` on purpose: settings are preferences, connections
/// are inventory, and a corrupted one must not take the other down with it.
pub fn connections_path() -> PathBuf {
    arbor_core::prelude::picus_config_path("connections.toml")
}

/// Read the configured connections. A missing or unparseable file yields an empty
/// list rather than an error — the studio still opens, and the user can re-add.
pub fn load_connections() -> Vec<ConnectionSpec> {
    let Ok(text) = std::fs::read_to_string(connections_path()) else {
        return Vec::new();
    };
    toml::from_str::<ConnectionFile>(&text).map(|f| f.connections).unwrap_or_default()
}

/// Persist the connection list (pretty TOML), creating the directory if needed.
///
/// Guaranteed by construction not to write a secret: [`ConnectionSpec`] has no
/// field for one.
pub fn save_connections(connections: &[ConnectionSpec]) -> Result<(), String> {
    let path = connections_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = ConnectionFile { connections: connections.to_vec() };
    let text = toml::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// The live sessions, keyed by connection id.
///
/// Sessions are handed out as `Arc` clones so a handler serving a long query and
/// one serving its cancellation hold the same session at the same time — the lock
/// is never held across an await.
#[derive(Default)]
pub struct SessionPool {
    open: Mutex<HashMap<String, Arc<dyn DbSession>>>,
}

impl SessionPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// The open session for a connection, if any.
    pub fn get(&self, id: &str) -> Option<Arc<dyn DbSession>> {
        self.open.lock().unwrap_or_else(|p| p.into_inner()).get(id).cloned()
    }

    /// Store a freshly opened session, replacing (and returning) any previous one
    /// so the caller can close it.
    pub fn insert(&self, id: &str, session: Arc<dyn DbSession>) -> Option<Arc<dyn DbSession>> {
        self.open.lock().unwrap_or_else(|p| p.into_inner()).insert(id.to_string(), session)
    }

    /// Drop a session from the pool, returning it so the caller can close it.
    pub fn remove(&self, id: &str) -> Option<Arc<dyn DbSession>> {
        self.open.lock().unwrap_or_else(|p| p.into_inner()).remove(id)
    }

    /// Ids of every open session.
    pub fn open_ids(&self) -> Vec<String> {
        self.open.lock().unwrap_or_else(|p| p.into_inner()).keys().cloned().collect()
    }

    pub fn is_open(&self, id: &str) -> bool {
        self.open.lock().unwrap_or_else(|p| p.into_inner()).contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_db_api::prelude::EngineKind;

    fn spec(id: &str) -> ConnectionSpec {
        ConnectionSpec {
            id: id.to_string(),
            name: "Local".to_string(),
            alias: "development".to_string(),
            engine: EngineKind::Postgres,
            host: "localhost".to_string(),
            port: 5432,
            database: "appdb".to_string(),
            user: "app".to_string(),
            schema: "public".to_string(),
            color_idx: 3,
            read_only: true,
            tls: false,
            params: Default::default(),
        }
    }

    #[test]
    fn the_connection_file_round_trips_through_toml() {
        let file = ConnectionFile { connections: vec![spec("a"), spec("b")] };
        let text = toml::to_string_pretty(&file).expect("serialize");
        let back: ConnectionFile = toml::from_str(&text).expect("deserialize");

        assert_eq!(back.connections.len(), 2);
        assert_eq!(back.connections[0].id, "a");
        assert!(back.connections[0].read_only);
        assert_eq!(back.connections[0].engine, EngineKind::Postgres);
    }

    #[test]
    fn the_persisted_form_contains_no_secret() {
        // Not a style check: the file lives on disk in the user's profile, and the
        // product's promise is that a password is never in it.
        let text = toml::to_string_pretty(&ConnectionFile { connections: vec![spec("a")] })
            .expect("serialize");
        let lower = text.to_lowercase();
        assert!(!lower.contains("password"), "connections.toml must never hold a secret");
        assert!(!lower.contains("secret"));
    }

    #[test]
    fn an_unparseable_file_is_an_empty_list_not_a_crash() {
        assert!(toml::from_str::<ConnectionFile>("this is not toml {{{").is_err());
        // `load_connections` maps that error to an empty list; asserted here on the
        // same path it uses.
        let recovered: Vec<ConnectionSpec> =
            toml::from_str::<ConnectionFile>("nonsense").map(|f| f.connections).unwrap_or_default();
        assert!(recovered.is_empty());
    }
}
