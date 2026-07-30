//! What a server is doing **right now** — the session monitor's vocabulary.
//!
//! This is the one part of the API that describes the server rather than the
//! schema, and it exists because of a failure that actually happened: a statement
//! stopped responding, Cancel did nothing, and from inside Picus there was no way
//! to see whether the backend was still running, waiting on a lock, or already
//! gone. A tool that opens sessions on other people's databases has to be able to
//! answer "what is stuck, and what is holding it".
//!
//! Two deliberate shapes here:
//!
//! * **Blocking is an edge list, not a flag.** "This session is blocked" is not
//!   actionable; "this session is blocked by pid 4412, which is idle in a
//!   transaction" is. The graph is what lets the interface draw the chain to the
//!   session at the root of it, which is the only one worth acting on.
//! * **Times are ages in milliseconds, not timestamps.** The server's clock and
//!   the client's disagree, sometimes by hours, and a duration computed by
//!   subtracting one from the other is wrong in a way nobody notices. The server
//!   computes the age; we carry it.

use serde::{Deserialize, Serialize};

/// Everything the monitor shows in one read.
///
/// One read, not two: the sessions and the blocking edges have to describe the
/// same instant or the interface draws an arrow from a session to one that has
/// already finished.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySnapshot {
    pub sessions: Vec<SessionActivity>,
    /// Who is waiting for whom. Empty when nothing is blocked.
    pub blocked: Vec<BlockEdge>,
    /// The server's own idea of when this was read, as it formats it. Displayed,
    /// never computed with.
    pub read_at: String,
}

/// One backend connected to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionActivity {
    /// Server-side process id. The handle every action takes.
    pub pid: i32,
    pub user: String,
    pub database: String,
    /// What the client called itself — `psql`, `Picus`, an application name.
    pub application: String,
    /// Client address, or empty for a local socket.
    pub client: String,
    /// `active`, `idle`, `idle in transaction`, … as the server words it.
    pub state: String,
    /// What it is waiting on, when it is waiting. `None` means it is running.
    pub wait_event: Option<String>,
    /// The statement, as the server holds it. May be truncated by the server's own
    /// `track_activity_query_size`; that is the server's truncation, not ours.
    pub query: String,
    /// How long the current statement has been running, in milliseconds.
    pub query_age_ms: Option<i64>,
    /// How long the session has been in its current state, in milliseconds. This is
    /// the number that identifies an abandoned `idle in transaction`.
    pub state_age_ms: Option<i64>,
    /// How long the transaction has been open, in milliseconds.
    pub transaction_age_ms: Option<i64>,
    /// This is Picus's own session. Killing it is legal and occasionally what the
    /// user wants, but it has to be labelled rather than discovered.
    pub is_self: bool,
    /// Pids this session is waiting for. Denormalised from [`ActivitySnapshot::blocked`]
    /// so a row can be rendered without a lookup.
    pub blocked_by: Vec<i32>,
}

/// One "waiter is stuck behind blocker" relationship.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockEdge {
    pub waiter: i32,
    pub blocker: i32,
    /// The object being contended, when the server names one.
    pub relation: Option<String>,
    /// The lock mode being waited for.
    pub mode: Option<String>,
}

/// How firmly to ask a session to stop.
///
/// Two verbs and not one because they are genuinely different acts, and merging
/// them would make the gentler one unreachable: cancelling asks the *statement* to
/// stop and leaves the connection alive, which is almost always what is wanted;
/// terminating drops the connection, rolls its transaction back, and is the answer
/// only for a session that is not running anything at all — the classic abandoned
/// `idle in transaction` holding a lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StopKind {
    /// Cancel the running statement; the session survives.
    Cancel,
    /// Close the connection outright.
    Terminate,
}
