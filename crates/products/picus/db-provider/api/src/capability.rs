//! [`EngineCapabilities`] — what an engine actually has, as data.
//!
//! The frontend reads this instead of branching on the engine name. That is the
//! whole point: a third engine should be a crate plus a descriptor, not an edit to
//! six `if (dialect === 'oracle')` in six components.

use serde::{Deserialize, Serialize};

use crate::tx::TxCapability;

/// Which schema groups an engine's browser offers, in display order. Mirrors the
/// frontend's `SchemaGroup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaGroup {
    Tables,
    Views,
    Sequences,
    Triggers,
}

/// Per-engine capability matrix.
///
/// A field is `true` when the engine has the concept **and** the provider reports
/// it. `false` means the UI should not offer it at all — not that it will fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCapabilities {
    /// Can a live session be opened at all? `false` for an engine Picus supports
    /// on the script side only — today, Oracle.
    pub connect: bool,
    /// Sequences are first-class objects (both engines, but not every engine).
    pub sequences: bool,
    /// Materialised views.
    pub materialized_views: bool,
    /// Stored packages (Oracle) as distinct from standalone routines.
    pub packages: bool,
    /// `INSTEAD OF` triggers on views.
    pub instead_of_triggers: bool,
    /// Bitmap indexes (Oracle).
    pub bitmap_indexes: bool,
    /// Function-based / expression indexes.
    pub expression_indexes: bool,
    /// A running statement can be cancelled server-side.
    pub cancel_query: bool,
    /// The server can give a cheap approximate row count for a table.
    pub estimated_rows: bool,
    /// The engine namespaces objects by *schema* inside one database (PostgreSQL)
    /// rather than treating the user as the schema (Oracle). Drives whether the
    /// connection form asks for a schema separately from the database.
    pub schemas: bool,
    /// The server can be asked what every connected session is doing, and which of
    /// them are blocked behind which. Drives whether the monitor exists at all.
    pub session_activity: bool,
    /// A statement's plan can be asked for without running it.
    pub explain: bool,
    /// A statement can be prepared (parsed + described) without running it, so the
    /// server's own rejection — message and position — stands in for hand-rolled
    /// semantic checks. Drives the editor's live validation.
    pub validate: bool,
    /// Statements can carry bound parameters rather than interpolated values.
    pub bind_parameters: bool,
    /// The catalogue can be walked into a dependency graph.
    pub dependency_graph: bool,
    /// What an explicit transaction covers here — including whether DDL is inside
    /// it, which is the one that decides whether a failed install can be undone.
    pub transactions: TxCapability,
}

impl EngineCapabilities {
    /// All-false starting point — the honest default for a new provider.
    pub const fn none() -> Self {
        Self {
            connect: false,
            sequences: false,
            materialized_views: false,
            packages: false,
            instead_of_triggers: false,
            bitmap_indexes: false,
            expression_indexes: false,
            cancel_query: false,
            estimated_rows: false,
            schemas: false,
            session_activity: false,
            explain: false,
            validate: false,
            bind_parameters: false,
            dependency_graph: false,
            transactions: TxCapability::none(),
        }
    }
}
