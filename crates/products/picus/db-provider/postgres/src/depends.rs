//! The dependency graph of one schema, read out of `pg_catalog`.
//!
//! Several catalogues answer several different questions, and none of them answers
//! another's: `pg_constraint` knows about foreign keys, `pg_rewrite` (through
//! `pg_depend`) knows what a view reads, `pg_trigger` knows what a trigger sits on
//! and what it fires, `pg_depend` again knows which sequence a column draws from,
//! and — only sometimes — what a routine touches. So this module is seven reads and
//! one accumulator, rather than one heroic query nobody could change afterwards.
//!
//! ## What "could not resolve" means here, and why it is reported
//!
//! PostgreSQL does not record what a `plpgsql` body reads. The text is stored as a
//! string and never analysed, so a function that deletes from four tables has, in
//! the catalogue, no relationship with any of them. That is not a gap this module
//! can close — parsing arbitrary `plpgsql` to find out is a different product — so
//! every routine whose body the catalogue cannot see is named in
//! [`DependencyGraph::unresolved`].
//!
//! It is deliberately verbose. The whole point of the graph is to order a creation,
//! and a routine whose reads are unknown is exactly the one that can be ordered
//! wrongly; a graph that quietly dropped it would produce an order that looks
//! authoritative and is not.
//!
//! ## Node keys, and the one place a name is not enough
//!
//! Edges carry names, not object identifiers, so a name has to be unique across the
//! graph. Inside the read schema the unqualified name is (with the one exception
//! below); outside it, it is not — an `orders` in `audit` and an `orders` here are
//! two objects. So a node **outside** the session's schema is keyed
//! `schema.name`, with [`DependencyNode::schema`] carrying the schema and `name`
//! carrying the qualified form so that a reader joining an edge to a node by name
//! finds it. In-schema nodes keep the bare name, which is what the object tree and
//! every other panel already use.
//!
//! The exception is triggers: a trigger name is unique per table, not per schema.
//! This keys them by name alone anyway — the same choice the schema snapshot makes
//! ([`crate::catalog::read_trigger_detail`]) — so two tables carrying a trigger of
//! the same name become one node with both tables' edges on it. Diverging here
//! would mean the dependency panel and the object tree disagreed about what a
//! trigger is called, which is worse than the collision.
//!
//! Requires PostgreSQL 11 or later (`pg_proc.prokind`), which is a lower bar than
//! the schema read already sets with `pg_sequences` (10+).

use std::collections::{HashMap, HashSet};

use picus_db_api::prelude::*;
use tokio_postgres::Client;

use crate::error::map_pg;

/// Every object in `schema`, and every relationship between them the catalogue
/// records.
///
/// One round trip per catalogue rather than per object: the whole graph is eight
/// queries whatever the schema's size, which is what makes it affordable to ask for
/// on a schema with several hundred tables.
pub async fn read_dependencies(client: &Client, schema: &str) -> DbResult<DependencyGraph> {
    let mut graph = Builder::new(schema);

    read_objects(client, schema, &mut graph).await?;
    read_routines(client, schema, &mut graph).await?;
    read_foreign_keys(client, schema, &mut graph).await?;
    read_view_sources(client, schema, &mut graph).await?;
    read_triggers(client, schema, &mut graph).await?;
    read_sequence_defaults(client, schema, &mut graph).await?;
    read_routine_bodies(client, schema, &mut graph).await?;

    Ok(graph.finish())
}

// ── The accumulator ──────────────────────────────────────────────────────────

/// Collects nodes and edges while the reads run, keeping both free of
/// duplicates.
///
/// Duplicates are not hypothetical: a composite foreign key is one constraint over
/// several columns, `pg_depend` holds one row per *column* a view reads, and a
/// `serial` column records both an ownership dependency and a default dependency on
/// the same sequence. Each of those would otherwise arrive as the same edge two,
/// four or ten times — and an edge repeated is an edge that looks like several
/// reasons when it is one.
struct Builder {
    /// The session's own schema. Nodes in it are keyed by their bare name.
    schema: String,
    nodes: HashMap<String, DependencyNode>,
    edges: Vec<DependencyEdge>,
    /// `from`, `to`, kind and `via` flattened — [`DependencyKind`] is `Copy` but not
    /// `Hash`, and giving it a `Hash` impl for a local de-duplication would be
    /// putting a trait on the shared API to serve one caller.
    seen: HashSet<String>,
    unresolved: Vec<String>,
}

impl Builder {
    fn new(schema: &str) -> Self {
        Self {
            schema: schema.to_string(),
            nodes: HashMap::new(),
            edges: Vec::new(),
            seen: HashSet::new(),
            unresolved: Vec::new(),
        }
    }

    /// Register a node and return the key edges refer to it by.
    ///
    /// Idempotent, and idempotent in the right direction: the first registration
    /// wins, because it is the one that came from the catalogue of objects and
    /// therefore carries the true kind. A later mention — a foreign key pointing at
    /// a table, say — knows the name and only guesses the kind.
    fn node(&mut self, schema: &str, name: &str, kind: &str) -> String {
        let foreign = !schema.is_empty() && schema != self.schema;
        let key = if foreign { format!("{schema}.{name}") } else { name.to_string() };
        self.nodes.entry(key.clone()).or_insert_with(|| DependencyNode {
            name: key.clone(),
            kind: kind.to_string(),
            schema: if foreign { schema.to_string() } else { String::new() },
        });
        key
    }

    /// `from` needs `to`. Silently ignores an edge from an object to itself — a
    /// self-referencing foreign key is a real and legitimate thing (a tree stored in
    /// one table), and as an ordering constraint it is unsatisfiable noise.
    fn edge(&mut self, from: String, to: String, kind: DependencyKind, via: Option<String>) {
        if from == to {
            return;
        }
        let tag = format!("{from}\u{1}{to}\u{1}{kind:?}\u{1}{}", via.as_deref().unwrap_or(""));
        if !self.seen.insert(tag) {
            return;
        }
        self.edges.push(DependencyEdge { from, to, kind, via });
    }

    fn unresolved(&mut self, what: String) {
        self.unresolved.push(what);
    }

    /// Sorted, because the answer feeds a topological sort and a sort is only
    /// reproducible if its input is: two reads of an unchanged schema that produced
    /// two different creation orders would be a tool nobody could diff against.
    fn finish(self) -> DependencyGraph {
        let mut nodes: Vec<DependencyNode> = self.nodes.into_values().collect();
        nodes.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));

        let mut edges = self.edges;
        edges.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));

        let mut unresolved = self.unresolved;
        unresolved.sort();
        unresolved.dedup();

        DependencyGraph { nodes, edges, unresolved }
    }
}

/// `pg_class.relkind` in the vocabulary the schema browser uses.
///
/// A materialised view is a `view` here, exactly as it is in the object tree: it is
/// the same kind of thing to order and the same icon to draw, and inventing a sixth
/// word for it would make the two panels describe the same object differently.
fn relation_label(relkind: &str) -> &'static str {
    match relkind {
        "v" | "m" => "view",
        "S" => "sequence",
        _ => "table",
    }
}

// ── The reads ───────────────────────────────────────────────────────────────

/// Relations and sequences — the nodes that exist whether or not anything depends
/// on them.
///
/// Read first, and read in full: a table nothing references and that references
/// nothing is still an object the creation order has to contain. A graph built only
/// from edges would silently omit every isolated object, which on a young schema is
/// most of them.
async fn read_objects(client: &Client, schema: &str, graph: &mut Builder) -> DbResult<()> {
    const SQL: &str = "
        SELECT c.relname, c.relkind::text
          FROM pg_class c
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1
           AND c.relkind IN ('r', 'p', 'v', 'm', 'S')
           -- Partitions are an implementation detail of the partitioned table, as
           -- in the catalogue read: listing a thousand monthly children as a
           -- thousand nodes buries the schema in its own storage layout.
           AND NOT c.relispartition";

    for row in client.query(SQL, &[&schema]).await.map_err(map_pg)? {
        let name: String = row.get(0);
        let relkind: String = row.get(1);
        graph.node(schema, &name, relation_label(&relkind));
    }
    Ok(())
}

/// Functions and procedures.
///
/// Aggregates and window functions are left out (`prokind IN ('f','p')`): neither is
/// something a trigger fires or a migration orders around, and both would arrive
/// unlabelled in a vocabulary that has no word for them.
///
/// Overloads collapse into one node — a name has no signature here. Stated rather
/// than hidden: two functions called `audit` with different argument lists are one
/// row in this graph, and the edges of both hang off it.
async fn read_routines(client: &Client, schema: &str, graph: &mut Builder) -> DbResult<()> {
    const SQL: &str = "
        SELECT p.proname, p.prokind::text
          FROM pg_proc p
          JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = $1 AND p.prokind IN ('f', 'p')";

    for row in client.query(SQL, &[&schema]).await.map_err(map_pg)? {
        let name: String = row.get(0);
        let prokind: String = row.get(1);
        graph.node(schema, &name, if prokind == "p" { "procedure" } else { "function" });
    }
    Ok(())
}

/// Foreign keys: the referencing table needs the referenced one.
///
/// The direction is the whole reason this is worth reading — it is the one
/// relationship that decides an insertion order as well as a creation order.
///
/// `via` carries the **constraint** name rather than the columns. A reader who wants
/// to know which columns has the constraint name to look it up with; a reader
/// looking at the graph wants to know which rule, and `fk_ordini_clienti` is the
/// rule's name in every error message the server will ever print about it.
async fn read_foreign_keys(client: &Client, schema: &str, graph: &mut Builder) -> DbResult<()> {
    const SQL: &str = "
        SELECT c.relname, ref.relname, refn.nspname, con.conname, ref.relkind::text
          FROM pg_constraint con
          JOIN pg_class c ON c.oid = con.conrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
          JOIN pg_class ref ON ref.oid = con.confrelid
          JOIN pg_namespace refn ON refn.oid = ref.relnamespace
         WHERE n.nspname = $1 AND con.contype = 'f'";

    for row in client.query(SQL, &[&schema]).await.map_err(map_pg)? {
        let child: String = row.get(0);
        let parent: String = row.get(1);
        let parent_schema: String = row.get(2);
        let constraint: String = row.get(3);
        let parent_kind: String = row.get(4);

        let from = graph.node(schema, &child, "table");
        let to = graph.node(&parent_schema, &parent, relation_label(&parent_kind));
        graph.edge(from, to, DependencyKind::ForeignKey, Some(constraint));
    }
    Ok(())
}

/// What a view reads — through the rewrite rule its body is stored as.
///
/// There is no catalogue of "the tables of a view". What exists is the `_RETURN`
/// rewrite rule holding the parsed body, and the dependencies the planner recorded
/// for **that rule** — which is why this joins `pg_rewrite` to `pg_depend` rather
/// than reading any definition text. The consequence worth having: it is the
/// *parsed* body, so a table reached through a CTE, a sub-select or a join is found
/// exactly like one named in the `FROM`, and nothing here has to understand SQL.
///
/// `DISTINCT` because `pg_depend` records one row per column the view touches, and
/// a view reading eight columns of a table depends on that table once.
async fn read_view_sources(client: &Client, schema: &str, graph: &mut Builder) -> DbResult<()> {
    const SQL: &str = "
        SELECT DISTINCT v.relname, v.relkind::text, src.relname, srcn.nspname, src.relkind::text
          FROM pg_rewrite r
          JOIN pg_class v ON v.oid = r.ev_class
          JOIN pg_namespace vn ON vn.oid = v.relnamespace
          JOIN pg_depend d
            ON d.classid = 'pg_rewrite'::regclass
           AND d.objid = r.oid
           AND d.refclassid = 'pg_class'::regclass
          JOIN pg_class src ON src.oid = d.refobjid
          JOIN pg_namespace srcn ON srcn.oid = src.relnamespace
         WHERE vn.nspname = $1
           AND v.relkind IN ('v', 'm')
           -- Every view depends on itself through its own rule. That row is an
           -- artefact of how the rule is stored, not a relationship.
           AND src.oid <> v.oid
           AND src.relkind IN ('r', 'p', 'v', 'm', 'S', 'f')";

    for row in client.query(SQL, &[&schema]).await.map_err(map_pg)? {
        let view: String = row.get(0);
        let view_kind: String = row.get(1);
        let source: String = row.get(2);
        let source_schema: String = row.get(3);
        let source_kind: String = row.get(4);

        let from = graph.node(schema, &view, relation_label(&view_kind));
        let to = graph.node(&source_schema, &source, relation_label(&source_kind));
        graph.edge(from, to, DependencyKind::ViewSource, None);
    }
    Ok(())
}

/// Triggers: each one needs its table, and each one needs the routine it fires.
///
/// Two edges from one row, and they are genuinely two facts — dropping the table
/// takes the trigger with it, whereas the routine survives and can be shared by
/// several triggers. An install order has to satisfy both.
///
/// A trigger whose routine cannot be resolved is reported rather than dropped: the
/// only way to reach that state is a `pg_proc` entry that has gone (a dropped
/// extension), and it is precisely the case where an install would fail with a
/// message about something the user has never heard of.
async fn read_triggers(client: &Client, schema: &str, graph: &mut Builder) -> DbResult<()> {
    const SQL: &str = "
        SELECT t.tgname, c.relname, c.relkind::text, p.proname, pn.nspname, p.prokind::text
          FROM pg_trigger t
          JOIN pg_class c ON c.oid = t.tgrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
          LEFT JOIN pg_proc p ON p.oid = t.tgfoid
          LEFT JOIN pg_namespace pn ON pn.oid = p.pronamespace
         WHERE n.nspname = $1
           -- The constraint machinery installs triggers of its own. They are the
           -- implementation of a foreign key this graph already has an edge for.
           AND NOT t.tgisinternal";

    for row in client.query(SQL, &[&schema]).await.map_err(map_pg)? {
        let trigger: String = row.get(0);
        let table: String = row.get(1);
        let table_kind: String = row.get(2);
        let routine: Option<String> = row.get(3);
        let routine_schema: Option<String> = row.get(4);
        let routine_kind: Option<String> = row.get(5);

        let from = graph.node(schema, &trigger, "trigger");
        let table_key = graph.node(schema, &table, relation_label(&table_kind));
        graph.edge(from.clone(), table_key, DependencyKind::TriggerTable, None);

        match (routine, routine_schema) {
            (Some(name), Some(ns)) => {
                let kind = match routine_kind.as_deref() {
                    Some("p") => "procedure",
                    _ => "function",
                };
                let to = graph.node(&ns, &name, kind);
                graph.edge(from, to.clone(), DependencyKind::TriggerRoutine, Some(to));
            }
            _ => graph.unresolved(format!(
                "trigger {trigger} on {table} — the routine it fires is no longer in the \
                 catalogue, so what it runs is unknown"
            )),
        }
    }
    Ok(())
}

/// Columns that draw from a sequence — a `serial`, an identity, or a hand-written
/// `DEFAULT nextval(…)`.
///
/// Two shapes, one query each, because they are recorded differently and neither
/// covers the other:
///
/// * an **identity** column owns its sequence (`deptype = 'i'`), and has no entry in
///   `pg_attrdef` at all;
/// * a **default** — whether written by hand or produced by `serial` — is an
///   expression in `pg_attrdef` with a normal dependency on the sequence.
///
/// `serial` produces both, which is what the accumulator's de-duplication is for.
async fn read_sequence_defaults(
    client: &Client,
    schema: &str,
    graph: &mut Builder,
) -> DbResult<()> {
    // Identity columns: the dependency runs sequence → column, so the *sequence* is
    // the dependent object in the catalogue and the table is what it hangs off.
    // The graph's direction is the other way round — the table needs the sequence to
    // exist before a row can be inserted — which is why this is not a transcription
    // of `pg_depend` but a reading of it.
    const IDENTITY: &str = "
        SELECT DISTINCT c.relname, c.relkind::text, s.relname, sn.nspname, a.attname
          FROM pg_depend d
          JOIN pg_class s ON s.oid = d.objid AND s.relkind = 'S'
          JOIN pg_namespace sn ON sn.oid = s.relnamespace
          JOIN pg_class c ON c.oid = d.refobjid
          JOIN pg_namespace n ON n.oid = c.relnamespace
          JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = d.refobjsubid
         WHERE d.classid = 'pg_class'::regclass
           AND d.refclassid = 'pg_class'::regclass
           AND d.deptype IN ('a', 'i')
           AND d.refobjsubid > 0
           AND n.nspname = $1";

    const DEFAULTS: &str = "
        SELECT DISTINCT c.relname, c.relkind::text, s.relname, sn.nspname, a.attname
          FROM pg_attrdef ad
          JOIN pg_depend d
            ON d.classid = 'pg_attrdef'::regclass
           AND d.objid = ad.oid
           AND d.refclassid = 'pg_class'::regclass
          JOIN pg_class s ON s.oid = d.refobjid AND s.relkind = 'S'
          JOIN pg_namespace sn ON sn.oid = s.relnamespace
          JOIN pg_class c ON c.oid = ad.adrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
          JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ad.adnum
         WHERE n.nspname = $1";

    for sql in [IDENTITY, DEFAULTS] {
        for row in client.query(sql, &[&schema]).await.map_err(map_pg)? {
            let table: String = row.get(0);
            let table_kind: String = row.get(1);
            let sequence: String = row.get(2);
            let sequence_schema: String = row.get(3);
            let column: String = row.get(4);

            let from = graph.node(schema, &table, relation_label(&table_kind));
            let to = graph.node(&sequence_schema, &sequence, "sequence");
            graph.edge(from, to, DependencyKind::SequenceDefault, Some(column));
        }
    }
    Ok(())
}

/// What a routine's body touches — for the routines whose bodies the catalogue
/// actually analysed, and an honest admission for all the others.
///
/// PostgreSQL analyses a body only when it is written in SQL and defined with
/// `BEGIN ATOMIC` (14+). Everything else — every `plpgsql` function ever written —
/// is stored as text and never looked at, so the catalogue holds no relationship
/// between it and the tables it rewrites. There is no query that changes that.
///
/// So this reports both halves: the edges it can prove, and by name every routine it
/// cannot see into. See the module docs for why the second half is not filtered down
/// to something shorter.
async fn read_routine_bodies(client: &Client, schema: &str, graph: &mut Builder) -> DbResult<()> {
    const SQL: &str = "
        SELECT DISTINCT p.proname, p.prokind::text, ref.relname, refn.nspname, ref.relkind::text
          FROM pg_depend d
          JOIN pg_proc p ON p.oid = d.objid
          JOIN pg_namespace n ON n.oid = p.pronamespace
          JOIN pg_class ref ON ref.oid = d.refobjid
          JOIN pg_namespace refn ON refn.oid = ref.relnamespace
         WHERE d.classid = 'pg_proc'::regclass
           AND d.refclassid = 'pg_class'::regclass
           AND d.deptype = 'n'
           AND n.nspname = $1
           AND ref.relkind IN ('r', 'p', 'v', 'm', 'S')";

    let mut analysed: HashSet<String> = HashSet::new();
    for row in client.query(SQL, &[&schema]).await.map_err(map_pg)? {
        let routine: String = row.get(0);
        let prokind: String = row.get(1);
        let target: String = row.get(2);
        let target_schema: String = row.get(3);
        let target_kind: String = row.get(4);

        let from =
            graph.node(schema, &routine, if prokind == "p" { "procedure" } else { "function" });
        let to = graph.node(&target_schema, &target, relation_label(&target_kind));
        analysed.insert(from.clone());
        graph.edge(from, to, DependencyKind::RoutineBody, None);
    }

    const OPAQUE: &str = "
        SELECT p.proname, l.lanname::text
          FROM pg_proc p
          JOIN pg_namespace n ON n.oid = p.pronamespace
          JOIN pg_language l ON l.oid = p.prolang
         WHERE n.nspname = $1 AND p.prokind IN ('f', 'p')";

    for row in client.query(OPAQUE, &[&schema]).await.map_err(map_pg)? {
        let routine: String = row.get(0);
        let language: String = row.get(1);
        if analysed.contains(&routine) {
            continue;
        }
        graph.unresolved(format!(
            "{routine} — PostgreSQL stores a {language} body as text and never analyses it, so \
             what this routine reads or writes is not in the catalogue"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builder() -> Builder {
        Builder::new("public")
    }

    #[test]
    fn an_object_outside_the_read_schema_is_keyed_by_its_schema() {
        let mut graph = builder();
        // Two objects of the same name in two schemas are two objects, and edges
        // carry names — so without the qualification they would be one node with
        // both sets of edges hanging off it.
        let here = graph.node("public", "orders", "table");
        let there = graph.node("audit", "orders", "table");
        assert_eq!(here, "orders");
        assert_eq!(there, "audit.orders");

        let graph = graph.finish();
        assert_eq!(graph.nodes.len(), 2);
        let foreign = graph.nodes.iter().find(|n| n.name == "audit.orders").unwrap();
        // The schema is carried as well as spelled into the key: a reader that wants
        // to show the bare name can, and one joining an edge by name still finds it.
        assert_eq!(foreign.schema, "audit");
        assert!(graph.nodes.iter().find(|n| n.name == "orders").unwrap().schema.is_empty());
    }

    #[test]
    fn the_first_registration_of_a_node_decides_its_kind() {
        let mut graph = builder();
        graph.node("public", "v_totals", "view");
        // A later mention only knows the name — a foreign key pointing at something
        // assumes "table". It must not overwrite what the object read established.
        graph.node("public", "v_totals", "table");
        let graph = graph.finish();
        assert_eq!(graph.nodes[0].kind, "view");
    }

    #[test]
    fn the_same_edge_arriving_twice_is_one_edge() {
        let mut graph = builder();
        // A `serial` column records its sequence twice — once as ownership, once as
        // the default's dependency — and both reads run.
        for _ in 0..2 {
            graph.edge(
                "orders".into(),
                "orders_id_seq".into(),
                DependencyKind::SequenceDefault,
                Some("id".into()),
            );
        }
        // A different column of the same table on the same sequence is a different
        // fact, though, and both belong on the edge list.
        graph.edge(
            "orders".into(),
            "orders_id_seq".into(),
            DependencyKind::SequenceDefault,
            Some("legacy_id".into()),
        );
        assert_eq!(graph.finish().edges.len(), 2);
    }

    #[test]
    fn a_self_reference_is_not_an_ordering_constraint() {
        let mut graph = builder();
        // A tree stored in one table — legitimate, and unsatisfiable as an order.
        graph.edge("nodes".into(), "nodes".into(), DependencyKind::ForeignKey, Some("fk".into()));
        assert!(graph.finish().edges.is_empty());
    }

    #[test]
    fn a_materialised_view_is_a_view_in_this_vocabulary() {
        assert_eq!(relation_label("m"), "view");
        assert_eq!(relation_label("v"), "view");
        assert_eq!(relation_label("S"), "sequence");
        assert_eq!(relation_label("r"), "table");
        assert_eq!(relation_label("p"), "table");
    }

    #[test]
    fn the_answer_is_ordered_so_a_second_read_can_be_compared_with_the_first() {
        let mut graph = builder();
        graph.node("public", "zeta", "table");
        graph.node("public", "alpha", "table");
        graph.node("public", "a_view", "view");
        graph.edge("zeta".into(), "alpha".into(), DependencyKind::ForeignKey, None);
        graph.edge("a_view".into(), "alpha".into(), DependencyKind::ViewSource, None);

        let graph = graph.finish();
        assert_eq!(
            graph.nodes.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            ["alpha", "zeta", "a_view"],
        );
        assert_eq!(graph.edges[0].from, "a_view");
    }
}
