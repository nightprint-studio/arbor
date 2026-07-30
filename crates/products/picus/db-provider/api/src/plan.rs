//! The query plan — what the server says it will do, or what it just did.
//!
//! Carried as a **tree flattened to a depth-tagged list** rather than as nested
//! nodes. Two reasons, and the second is the real one: the interface renders it as
//! an indented list either way, and a flat list serialises across the RPC seam
//! without a recursive type that every consumer then has to walk.
//!
//! ## `ANALYZE` is not a display option
//!
//! `EXPLAIN` describes; `EXPLAIN ANALYZE` **runs the statement**. On a `SELECT`
//! that is merely slow; on a `DELETE` it is the delete. So it is a separate,
//! explicit request ([`PlanRequest::analyze`]), an implementation must refuse it
//! for anything that is not a read, and the answer says which of the two it is
//! ([`QueryPlan::analyzed`]) so the interface never labels an estimate as a
//! measurement.

use serde::{Deserialize, Serialize};

/// What to explain, and how.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    /// Actually run the statement and report real times and row counts.
    ///
    /// Must be refused for anything that is not a read, and on a read-only
    /// connection is the only form that can be refused at all — which is the point:
    /// the refusal belongs in the engine, where it cannot be bypassed.
    pub analyze: bool,
    /// Include buffer accounting where the engine has it.
    pub buffers: bool,
}

/// A plan, as text and as structure.
///
/// Both, deliberately. The structure is what the interface draws; the text is what
/// gets pasted into a ticket, and a tool that can only show you its own rendering
/// of a plan is a tool you cannot ask anyone else about.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPlan {
    /// The engine's own textual plan, verbatim.
    pub text: String,
    /// The same plan, one entry per node, in execution-tree order.
    pub nodes: Vec<PlanNode>,
    /// The statement was executed. `false` means every number below is an estimate.
    pub analyzed: bool,
    /// Total estimated cost of the root node, in the engine's own units.
    pub total_cost: Option<f64>,
    /// Wall time of the whole plan when it was analysed.
    pub actual_ms: Option<f64>,
    /// How long producing the plan took.
    pub elapsed_ms: u64,
}

/// One step of the plan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNode {
    /// Indentation depth; 0 is the root.
    pub depth: u32,
    /// `Seq Scan`, `Index Scan using …`, `Hash Join`, …
    pub label: String,
    /// The relation this node reads, when it reads one.
    pub relation: Option<String>,
    /// Estimated total cost.
    pub cost: Option<f64>,
    /// Estimated rows out.
    pub rows: Option<f64>,
    /// Rows actually produced — only when analysed.
    pub actual_rows: Option<f64>,
    /// Time actually spent — only when analysed.
    pub actual_ms: Option<f64>,
    /// Extra lines the engine attached: filters, sort keys, index conditions.
    pub detail: Vec<String>,
    /// A remark worth surfacing — a sequential scan of a large relation, an
    /// estimate that missed by an order of magnitude. Stated as prose because it
    /// is advice, and advice with no reasoning attached is noise.
    pub warning: Option<String>,
}
