//! `connections` domain — configuring, opening and closing database sessions.
//!
//! Configuring a connection and opening it are separate acts, and these handlers
//! keep them separate: the list is readable, editable and complete with no server
//! reachable anywhere. That matters for the product, where a project routinely has
//! an Oracle branch nobody can connect to.

use std::sync::Arc;

use picus_core::prelude::{load_connections, save_connections, PicusState};
use picus_db_api::prelude::{
    ConnectionSpec, ConnectionState, ConnectionStatus, DbError, SecretResolver,
};
use serde::Serialize;

use crate::secrets::HostSecrets;

/// A configured connection plus whether it is currently open — what the sidebar
/// renders. Flattened so the frontend reads one object, not a pair.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionRow {
    #[serde(flatten)]
    pub spec: ConnectionSpec,
    pub state: ConnectionState,
    /// Server banner while connected, empty otherwise.
    pub server_version: String,
    /// Whether a password is stored for this connection. The value itself never
    /// leaves the shell — this is only so the form can say "a password is saved"
    /// instead of showing an empty field that looks like data loss.
    pub has_secret: bool,
}

/// Every configured connection with its current state.
#[arbor_rpc::handler]
async fn picus_list_connections(state: &PicusState) -> Result<Vec<ConnectionRow>, String> {
    let secrets = HostSecrets::new(state.host_caller());
    let mut out = Vec::new();

    for spec in load_connections() {
        let session = state.sessions().get(&spec.id);
        let (connection_state, server_version) = match &session {
            Some(s) => {
                let status = s.status().await;
                (status.state, status.server_version)
            }
            None => (ConnectionState::Disconnected, String::new()),
        };
        let has_secret = secrets.secret(&spec.id).ok().flatten().is_some();
        out.push(ConnectionRow { spec, state: connection_state, server_version, has_secret });
    }
    Ok(out)
}

/// Create or update a connection. The id decides which: an existing one is
/// replaced in place, keeping its position in the list.
#[arbor_rpc::handler]
async fn picus_save_connection(
    _state: &PicusState,
    connection: ConnectionSpec,
) -> Result<(), String> {
    let mut all = load_connections();
    match all.iter_mut().find(|c| c.id == connection.id) {
        Some(existing) => *existing = connection,
        None => all.push(connection),
    }
    save_connections(&all)
}

/// Forget a connection: close its session if open, drop it from the list, and ask
/// the shell to delete its stored secret.
///
/// The secret deletion is the part worth doing carefully — leaving an orphaned
/// password in the keychain for a connection the user believes they removed is
/// exactly the sort of thing that erodes trust in "Picus stores no password".
#[arbor_rpc::handler]
async fn picus_delete_connection(state: &PicusState, id: String) -> Result<(), String> {
    if let Some(session) = state.sessions().remove(&id) {
        let _ = session.close().await;
    }
    let mut all = load_connections();
    all.retain(|c| c.id != id);
    save_connections(&all)?;
    let _ = state.host_call("__picus_delete_secret", serde_json::json!(id));
    Ok(())
}

/// Open a session.
#[arbor_rpc::handler]
async fn picus_connect(state: &PicusState, id: String) -> Result<ConnectionStatus, String> {
    let spec = find_spec(&id)?;
    let provider = state.providers().require(spec.engine).map_err(|e| e.to_string())?;

    let secret = HostSecrets::new(state.host_caller())
        .secret(&spec.id)
        .map_err(|e| e.to_string())?;

    let session = provider.connect(&spec, secret).await.map_err(|e| e.to_string())?;
    let session: Arc<dyn picus_db_api::prelude::DbSession> = Arc::from(session);

    // Replacing an existing session closes the old one — reconnecting must not
    // leak the previous socket.
    if let Some(previous) = state.sessions().insert(&spec.id, Arc::clone(&session)) {
        let _ = previous.close().await;
    }

    let status = session.status().await;
    state.emit("picus://connection-changed", serde_json::json!({ "id": spec.id }));
    Ok(status)
}

/// Abandon a session and open a new one — the way out of a connection that has
/// stopped answering.
///
/// ## Why this is not just `picus_connect`
///
/// A session is one database connection. When a statement on it will not stop —
/// PostgreSQL refuses a cancel while it is inside an uninterruptible wait, and a
/// wedged index page is exactly that — then *everything* on that connection queues
/// behind it: the next query, the `CLOSE` of the result the user is looking at, and
/// the polite close that reconnecting would begin with. Which is to say the ordinary
/// reconnect needs the connection to be working, and the moment you want it is the
/// moment it is not.
///
/// So this one **drops** the old session rather than closing it. No SQL is sent to a
/// connection that has already proved it will not answer; the reference is released,
/// the socket goes with it, and a fresh connection is opened alongside.
///
/// What it cannot do is stated rather than implied: if the server is still executing
/// the old statement, it goes on doing so until it finishes or the backend is ended
/// from outside. Picus is usable again; the server's own housekeeping is the server's.
#[arbor_rpc::handler]
async fn picus_reset_connection(state: &PicusState, id: String) -> Result<ConnectionStatus, String> {
    // Dropped, deliberately, without `close()` — see above.
    drop(state.sessions().remove(&id));

    let spec = find_spec(&id)?;
    let provider = state.providers().require(spec.engine).map_err(|e| e.to_string())?;
    let secret =
        HostSecrets::new(state.host_caller()).secret(&spec.id).map_err(|e| e.to_string())?;

    let session = provider.connect(&spec, secret).await.map_err(|e| e.to_string())?;
    let session: Arc<dyn picus_db_api::prelude::DbSession> = Arc::from(session);
    // Anything the pool picked up meanwhile is dropped the same way, for the same
    // reason.
    drop(state.sessions().insert(&spec.id, Arc::clone(&session)));

    let status = session.status().await;
    state.emit("picus://connection-changed", serde_json::json!({ "id": spec.id }));
    Ok(status)
}

/// Close a session. Idempotent: disconnecting something already disconnected is a
/// no-op, not an error.
#[arbor_rpc::handler]
async fn picus_disconnect(state: &PicusState, id: String) -> Result<(), String> {
    if let Some(session) = state.sessions().remove(&id) {
        session.close().await.map_err(|e| e.to_string())?;
    }
    state.emit("picus://connection-changed", serde_json::json!({ "id": id }));
    Ok(())
}

/// Open a session, report what happened, and close it again.
///
/// Used by the connection form's **Test** button. Deliberately does not touch the
/// pool: testing a connection must not silently leave one open, and must not
/// disturb a session the user already has.
#[arbor_rpc::handler]
async fn picus_test_connection(
    state: &PicusState,
    connection: ConnectionSpec,
) -> Result<ConnectionStatus, String> {
    let provider = state.providers().require(connection.engine).map_err(|e| e.to_string())?;
    let secret =
        HostSecrets::new(state.host_caller()).secret(&connection.id).map_err(|e| e.to_string())?;

    let session = provider.connect(&connection, secret).await.map_err(|e| e.to_string())?;
    let status = session.status().await;
    let _ = session.close().await;
    Ok(status)
}

/// Read the application version from a connection's version table.
///
/// Table, column and filter are configuration rather than constants — projects name
/// them differently and some stamp no date at all. A database that isn't this
/// project's simply has no such table, which comes back as an empty string rather
/// than an error.
#[arbor_rpc::handler]
async fn picus_read_db_version(
    state: &PicusState,
    id: String,
    table: String,
    column: String,
    filter: String,
) -> Result<String, String> {
    let session = require_session(state, &id)?;
    let version = session
        .read_db_version(&table, &column, &filter)
        .await
        .map_err(|e| e.to_string())?;
    Ok(version.unwrap_or_default())
}

/// Look up a configured connection by id.
pub(crate) fn find_spec(id: &str) -> Result<ConnectionSpec, String> {
    load_connections()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| DbError::NotFound(format!("connection {id}")).to_string())
}

/// The open session for a connection, or the error that tells the frontend to
/// connect first rather than reporting a mysterious failure.
pub(crate) fn require_session(
    state: &PicusState,
    id: &str,
) -> Result<Arc<dyn picus_db_api::prelude::DbSession>, String> {
    state
        .sessions()
        .get(id)
        .ok_or_else(|| "this connection is not open — connect first".to_string())
}
