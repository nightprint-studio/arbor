//! The live Bevy inspector — Bennu talking to a running game over the Bevy Remote Protocol.
//!
//! The protocol client, the method-name dialects, the type registry and the entity snapshot are the
//! `arbor-brp` crate's; this module owns the one thing that crate cannot: **which game Bennu is
//! connected to right now**, and what to do when that game goes away.
//!
//! | File | Domain |
//! |---|---|
//! | `session.rs` | connect / disconnect / status, and the silent-reconnect monitor |
//! | `entities.rs` | the entity snapshot, tree pages, one entity's components |
//! | `watch.rs` | `+watch` streams for the selected entity (no polling for what is on screen) |
//! | `resources.rs` | resources: list and read |
//! | `types.rs` | the registered types, and the editor shape of one |
//! | `edit.rs` | mutate / insert / remove / spawn / despawn / reparent |
//! | `assist.rs` | component values across many entities in one query — what the assistant's `brp_query` calls |
//!
//! ## One connection, process-wide
//!
//! A Bennu window inspects one game at a time, so the connection is a single static guarded by a
//! `std::sync::Mutex`. The lock is only ever held to copy an `Arc` out or to swap a field — never
//! across an `.await` — which is what keeps a slow game from stalling every other handler.
//!
//! ## What "reconnect" means here
//!
//! A game restarted by `cargo run` is the common case, not an error. The monitor (see
//! `session.rs`) notices, retries with backoff, and re-probes the new process from scratch
//! (methods, registry): the old registry describes a binary that no longer exists. It bumps the
//! connection's **generation**, which every event carries, so a frontend can tell a frame from
//! the old game from one of the new.

mod assist;
mod edit;
mod entities;
mod resources;
mod session;
mod types;
mod watch;

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use arbor_brp::prelude::{
    BrpCapabilities, BrpClient, BrpError, MethodMap, Op, Snapshot, TypeRegistry,
};
use serde_json::Value;
use tokio::task::AbortHandle;

/// Connection state changes (connect, reconnecting, lost, regained) — payload built in `session.rs`.
const TOPIC_STATE: &str = "arbor://bennu/brp-state";
/// Frames of the watched entity — payload built in `watch.rs`.
const TOPIC_ENTITY: &str = "arbor://bennu/brp-entity";

/// Everything learned about one game at connect time. Immutable once built: a game that restarts
/// gets a *new* `Live`, never an edited one.
struct Live {
    client: Arc<BrpClient>,
    endpoint: String,
    /// Per-request timeout, kept so a reconnect probes with the same patience.
    timeout: std::time::Duration,
    caps: BrpCapabilities,
    methods: MethodMap,
    registry: Arc<TypeRegistry>,
    /// Type paths to ask for as `option`s so a snapshot carries names and parents.
    name_types: Vec<String>,
    parent_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum Link {
    Connected,
    /// The game stopped answering; the monitor is retrying.
    Reconnecting,
    Disconnected,
}

/// A component-presence query's answer, kept for as long as the world has the same entities.
struct FilterCache {
    revision: u64,
    key: String,
    ids: Arc<std::collections::HashSet<u64>>,
}

struct Conn {
    live: Option<Arc<Live>>,
    link: Link,
    /// Bumped on every (re)connect and disconnect. Background tasks compare theirs and exit when
    /// it no longer matches.
    generation: u64,
    snapshot: Option<Arc<Snapshot>>,
    filter_cache: Option<FilterCache>,
    /// The reconnect monitor of the current connection.
    monitor: Option<AbortHandle>,
    /// The `+watch` streams of the current generation (the selected entity's).
    watches: Vec<AbortHandle>,
    /// When a call last succeeded — the monitor only pings a connection that has been quiet.
    last_ok: Instant,
}

fn conn() -> &'static Mutex<Conn> {
    static CONN: OnceLock<Mutex<Conn>> = OnceLock::new();
    CONN.get_or_init(|| {
        Mutex::new(Conn {
            live: None,
            link: Link::Disconnected,
            generation: 0,
            snapshot: None,
            filter_cache: None,
            monitor: None,
            watches: Vec::new(),
            last_ok: Instant::now(),
        })
    })
}

/// A poisoned lock means a handler panicked while holding it; the data is plain state with no
/// invariant spanning fields, so carrying on beats wedging the inspector until restart.
fn lock() -> MutexGuard<'static, Conn> {
    conn().lock().unwrap_or_else(|e| e.into_inner())
}

fn live() -> Result<Arc<Live>, String> {
    lock().live.clone().ok_or_else(|| "Not connected to a Bevy game.".to_string())
}

fn touch() {
    lock().last_ok = Instant::now();
}

/// A transport/protocol failure as the single string that crosses the seam. The game's own message
/// is kept verbatim (it names the offending type or path); only the transport prefix is trimmed.
fn err_string(e: BrpError) -> String {
    match e {
        BrpError::Rpc { message, .. } => message,
        BrpError::Transport(m) => format!("Cannot reach the game: {m}"),
        other => other.to_string(),
    }
}

/// Issue `op` against the connected game, with the method name its Bevy version actually uses.
async fn call(op: Op, params: Option<Value>) -> Result<Value, String> {
    let live = live()?;
    let method = live
        .methods
        .get(op)
        .ok_or_else(|| format!("This game's Remote Protocol does not offer {op:?}."))?;
    match live.client.call(method, params).await {
        Ok(v) => {
            touch();
            Ok(v)
        }
        Err(e) => Err(err_string(e)),
    }
}

/// Drop everything derived from the previous world: the snapshot, the filter cache and the watch
/// streams. The monitor is left alone — it is the thing doing the resetting on a reconnect.
/// Returns the new generation.
fn reset_world(conn: &mut Conn) -> u64 {
    abort_watches(conn);
    conn.snapshot = None;
    conn.filter_cache = None;
    conn.generation += 1;
    conn.generation
}

fn abort_watches(conn: &mut Conn) {
    for t in conn.watches.drain(..) {
        t.abort();
    }
}

/// Everything, monitor included — a disconnect or a connect to somewhere else.
fn reset_all(conn: &mut Conn) -> u64 {
    if let Some(m) = conn.monitor.take() {
        m.abort();
    }
    reset_world(conn)
}
