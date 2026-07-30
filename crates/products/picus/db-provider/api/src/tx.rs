//! Explicit transactions — open one, look at what you did, then decide.
//!
//! The session already runs statements inside implicit transactions the driver
//! commits for it. What this adds is the pause: `BEGIN`, then any number of
//! statements, then a decision that is the user's rather than the driver's. On a
//! production database with the read-only flag off, that pause is the difference
//! between a mistake and an incident.
//!
//! ## The honest part
//!
//! **DDL is not always transactional.** PostgreSQL wraps `CREATE TABLE` and
//! `ALTER TABLE` in the open transaction like anything else, so a rollback really
//! does undo them. Oracle commits implicitly before and after every DDL statement,
//! so an open transaction is closed by the first `ALTER` whatever the client
//! wanted — no driver can prevent it.
//!
//! [`TxCapability`] is how that is stated rather than discovered: an engine says
//! up front whether DDL is covered, the interface says so before anything runs,
//! and nobody is promised a rollback that the server will not honour.

use serde::{Deserialize, Serialize};

/// Where the session's transaction stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TxState {
    /// No explicit transaction; statements commit as they run.
    #[default]
    None,
    /// Open, and accepting statements.
    Active,
    /// Open, but a statement failed: the engine will accept nothing further until
    /// it is rolled back. Distinct from `Active` because the only honest thing the
    /// interface can offer here is the rollback.
    Failed,
    /// Open, with at least one savepoint set.
    Savepoint,
}

/// What an engine's transactions actually cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxCapability {
    /// Explicit transactions are available at all.
    pub supported: bool,
    /// DDL participates in the transaction and is undone by a rollback.
    ///
    /// `false` on Oracle, and it is not a detail: an install script that adds a
    /// column and then populates it leaves the column behind when the population
    /// fails. Stated, so the interface can warn before the run rather than explain
    /// afterwards.
    pub transactional_ddl: bool,
    /// Named savepoints inside an open transaction.
    pub savepoints: bool,
}

impl TxCapability {
    /// The honest default for an engine that has not said otherwise.
    pub const fn none() -> Self {
        Self { supported: false, transactional_ddl: false, savepoints: false }
    }
}

/// The result of asking a session to begin, commit or roll back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxOutcome {
    pub state: TxState,
    /// What the engine said, when it said anything worth repeating.
    pub message: String,
    /// Statements that ran inside the transaction that just ended. Zero for a
    /// `BEGIN`; the reason a commit is worth confirming for anything else.
    pub statements: u32,
}
