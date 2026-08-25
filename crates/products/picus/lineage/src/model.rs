//! What a trace says, and the vocabulary it says it in.
//!
//! The shapes here are the whole contract: they cross the wire to the interface
//! unchanged, and they are what stops a lineage from ever being read as more certain
//! than it is.
//!
//! ## Three verdicts, and the difference between them is the point
//!
//! A reader traces a column in order to decide **where to write**. So the answer has
//! to distinguish, without any squinting:
//!
//! * [`Verdict::Resolved`] — an unbroken chain of plain column references ending on
//!   a base table. You may write there.
//! * [`Verdict::Derived`] — the value is computed. Its ingredients are named, and
//!   none of them *is* the value; there is nothing to write back through.
//! * [`Verdict::Unresolved`] — the walk stopped, and [`Trace::stopped`] says where
//!   and why.
//!
//! Collapsing the last two into "unknown" would be the easy shape and the wrong one:
//! *"this is `a.inizio` concatenated with `b.fine`"* is a useful answer, and *"the
//! trail ends at a table-valued function"* is a different useful answer, and neither
//! is *"we don't know"*.

use serde::Serialize;

/// One step of the journey a value made, outermost first.
///
/// Every hop names the relation **and** the column as that relation calls it, which
/// is what makes a rename visible: `CODSA ← V_TIPI.CENINT` says the name changed
/// here, and a chain of these is the answer to "why is this column called that".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hop {
    /// The relation this hop reads from — folded, schema-qualified when the SQL
    /// qualified it. Empty for a derived table, which has no name of its own.
    pub relation: String,
    /// What the relation calls the column.
    pub column: String,
    /// This relation is a view, so the trail continues through it. `false` on the
    /// last hop of a resolved trace, which is a table.
    pub is_view: bool,
}

/// How a trace ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// The chain reached a base table. [`Trace::hops`] ends on it.
    Resolved,
    /// The value is computed from the columns in [`Trace::reads`]. It is not any of
    /// them, and nothing can be written back through it.
    Derived,
    /// The value **is** one of the columns in [`Trace::reads`] — which one depends on
    /// the row.
    ///
    /// A set operation whose arms read different tables: `GARE` for some rows,
    /// `GARE_STORICO` for others. Separate from [`Derived`](Self::Derived) because
    /// collapsing the two says something false. "Computed, nothing to write back
    /// through" is exactly wrong here: there is a real column behind every row, and
    /// there are two writable tables, not none. The reader has to be able to tell
    /// *"we cannot say which of these"* from *"it is not any of these"*.
    Split,
    /// The walk stopped before reaching a table. [`Trace::stopped`] says why.
    Unresolved,
}

/// A column reference the trace could name but not follow — the ingredients of a
/// computed value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ingredient {
    /// The relation, when the reference could be attributed to one. Empty when the
    /// column was written bare and several sources could have carried it.
    pub relation: String,
    pub column: String,
}

/// Where one result column comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trace {
    /// The column as the result presents it.
    pub output: String,
    pub verdict: Verdict,
    /// The journey, outermost first. Empty for a trace that never took a step.
    pub hops: Vec<Hop>,
    /// For [`Verdict::Derived`], what the value is computed **from**; for
    /// [`Verdict::Split`], the columns it **is**, one per row.
    ///
    /// Deliberately **not** traced onward. Each ingredient is a lineage of its own,
    /// and following all of them would turn one question into a forest — the reader
    /// asked about this column, and the honest answer is "it is made of these", with
    /// the offer to ask about one of them next.
    pub reads: Vec<Ingredient>,
    /// Why the walk stopped, in the user's terms. Empty unless the verdict is
    /// [`Verdict::Unresolved`].
    pub stopped: String,
}

impl Trace {
    /// The base table this column is read from, or `""` when there is not one.
    ///
    /// The last hop of a resolved trace. This is what a grid colours by, which is
    /// exactly why it is a method and not a field: a caller that wants "the table"
    /// gets it from the chain rather than from a second field that could drift out
    /// of step with the chain it is supposed to summarise.
    pub fn base_relation(&self) -> &str {
        match self.verdict {
            Verdict::Resolved => self.hops.last().map(|h| h.relation.as_str()).unwrap_or(""),
            _ => "",
        }
    }

    /// The column's name on the base table — `""` when unresolved.
    pub fn base_column(&self) -> &str {
        match self.verdict {
            Verdict::Resolved => self.hops.last().map(|h| h.column.as_str()).unwrap_or(""),
            _ => "",
        }
    }

    /// Did the name change on the way? What makes a trace worth reading at a glance.
    pub fn renamed(&self) -> bool {
        !self.base_column().is_empty() && self.base_column() != self.output
    }
}

/// Everything one relation's columns trace back to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Lineage {
    /// What was traced — a view's name, or empty when a statement was traced.
    pub relation: String,
    pub columns: Vec<Trace>,
    /// Every view the walk passed through, in the order it met them. What the
    /// interface draws the stack from, and what a caller caches against.
    pub through: Vec<String>,
    /// The walk hit the depth limit somewhere. The traces are still true as far as
    /// they go; some end [`Verdict::Unresolved`] for this reason.
    pub truncated: bool,
}

impl Lineage {
    /// The distinct base tables the resolved columns come from, first seen first.
    ///
    /// The legend of a grid coloured by lineage. Unresolved and derived columns
    /// contribute nothing, which is right: they have no one table.
    pub fn base_relations(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for trace in &self.columns {
            let base = trace.base_relation();
            if !base.is_empty() && !out.iter().any(|held| held == base) {
                out.push(base.to_string());
            }
        }
        out
    }
}
