//! Notices a session the server has closed, and says so.
//!
//! A database drops an idle connection without telling anybody: the server's idle timeout, a
//! restart, a firewall forgetting the socket. The driver learns of it the moment the socket
//! ends, but nothing here *asked*, so the sidebar kept its green dot until a statement
//! failed — a connection shown as open that was not.
//!
//! One slow poll of the pool answers that. It reads `status()`, which is a flag check and
//! never a round trip, so an idle pool costs nothing. A session found closed is **removed**
//! from the pool: it can never run another statement, and leaving it there would make every
//! later call fail with a driver message instead of the plain "connect first".

use std::sync::Arc;
use std::time::Duration;

use picus_core::prelude::{load_connections, PicusState};
use picus_db_api::prelude::ConnectionState;
use serde_json::json;

const POLL_EVERY: Duration = Duration::from_secs(5);

/// Start the poll on the backend's runtime. Lives as long as the process.
pub fn spawn(state: Arc<PicusState>, rt: &tokio::runtime::Handle) {
    rt.spawn(async move {
        loop {
            tokio::time::sleep(POLL_EVERY).await;
            for id in state.sessions().open_ids() {
                report_if_lost(&state, &id).await;
            }
        }
    });
}

async fn report_if_lost(state: &PicusState, id: &str) {
    let Some(session) = state.sessions().get(id) else { return };
    if session.status().await.state != ConnectionState::Disconnected {
        return;
    }
    // `remove` returning `None` means a reconnect or an explicit disconnect got there first —
    // that path emits its own event, and announcing a loss now would be false.
    if state.sessions().remove(id).is_none() {
        return;
    }
    let name = load_connections()
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.name)
        .unwrap_or_else(|| id.to_string());
    state.emit("picus://connection-lost", json!({ "id": id, "name": name }));
    state.emit("picus://connection-changed", json!({ "id": id }));
}
