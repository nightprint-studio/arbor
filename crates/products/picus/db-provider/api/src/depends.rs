//! What depends on what — the object dependency graph.
//!
//! Nodes and edges rather than a tree, because dependency in a database is not a
//! tree: a view reads three tables, two of which reference each other, and a
//! trigger on one of them calls a routine that reads a fourth. Flattening that
//! into a tree means picking a root and lying about the rest.
//!
//! Every edge states **why** it exists ([`DependencyKind`]). That is what makes the
//! graph readable: "`ORDINI` depends on `CLIENTI`" is a fact with several possible
//! causes, and a foreign key, a view body and a trigger are three different things
//! to do about it.
//!
//! ## What this is for beyond looking at it
//!
//! A topological order. Anything that emits objects — a migration generated from a
//! diff, a repository install run in one transaction — has to create them in an
//! order that works, and the order is this graph sorted. That is why the edges
//! carry direction with a fixed meaning: **`from` needs `to` to exist first.**

use serde::{Deserialize, Serialize};

/// The whole graph for one schema.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyGraph {
    pub nodes: Vec<DependencyNode>,
    pub edges: Vec<DependencyEdge>,
    /// Objects the engine could not resolve — a view whose body references
    /// something outside the read schema, a routine whose source is unavailable.
    ///
    /// Reported rather than dropped: a graph that silently omits what it could not
    /// work out is a graph you cannot trust to order anything.
    pub unresolved: Vec<String>,
}

/// One object in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyNode {
    /// Name as the catalogue holds it, unqualified.
    pub name: String,
    /// `table`, `view`, `sequence`, `trigger`, `function`, `procedure` — the same
    /// vocabulary the schema browser uses, so one icon set serves both.
    pub kind: String,
    /// Schema the object lives in; empty when it is the session's own.
    pub schema: String,
}

/// `from` needs `to`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub kind: DependencyKind,
    /// The specific thing that ties them — a constraint name, a column, a routine.
    /// Shown on the edge so the answer to "why?" needs no second query.
    pub via: Option<String>,
}

/// Why one object depends on another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DependencyKind {
    /// A foreign key: `from` references `to`.
    ForeignKey,
    /// `from` is a view (or a materialised view) whose body reads `to`.
    ViewSource,
    /// `from` is a trigger installed on table `to`.
    TriggerTable,
    /// `from` is a trigger that fires routine `to`.
    TriggerRoutine,
    /// `from` is a column whose default draws from sequence `to`, or an identity.
    SequenceDefault,
    /// `from` is a routine whose body reads or writes `to`.
    RoutineBody,
}
