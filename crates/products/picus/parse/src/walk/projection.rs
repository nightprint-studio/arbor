//! Reading a [`Projection`] off a `select_statement` node.
//!
//! The counterpart of [`super::select`], and deliberately not a widening of it —
//! see [`crate::projection`] for why the two exist. This one descends: into derived
//! tables, into `WITH`, into both arms of a set operation, because a value traced
//! back through a stack of views passes through all three.
//!
//! Every branch that cannot be modelled is **marked**, never dropped. The caller is
//! following a value in order to decide where to write it, so "the trail ends here"
//! has to be distinguishable from "there was nothing here".

use tree_sitter::Node;

use crate::object::{fold, ObjectKind};
use crate::projection::{ColumnSource, Cte, FromItem, FromSource, Projected, Projection};

use super::names::{object_ref, text_of};

/// The projection of a `select_statement`, or `None` when this is not one.
pub(crate) fn projection_of(node: Node, source: &str) -> Option<Projection> {
    if node.kind() != "select_statement" {
        return None;
    }
    let mut out = Projection::default();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "with_clause" => out.ctes = ctes_of(child, source),
            "select_core" => core_into(child, source, &mut out),
            "set_operation" => arms_into(child, source, &mut out),
            // A bare `VALUES` as the whole statement: rows written out, reading from
            // nothing. Nothing to trace, and saying so is the honest answer.
            "values_clause" => out.opaque = true,
            _ => {}
        }
    }

    // A statement that turned out to be neither a core nor a set operation was
    // something this does not model — an error node, most likely.
    if out.items.is_empty() && out.arms.is_empty() && !out.opaque {
        out.opaque = true;
    }
    Some(out)
}

/// Fill `out` from a `select_core` — its projection list and its `FROM`.
fn core_into(core: Node, source: &str, out: &mut Projection) {
    if let Some(list) = core.child_by_field_name("list") {
        let mut cursor = list.walk();
        for item in list.children(&mut cursor) {
            if item.kind() == "select_item" {
                out.items.push(projected(item, source));
            }
        }
    }
    let mut cursor = core.walk();
    for child in core.children(&mut cursor) {
        if child.kind() == "from_clause" {
            from_into(child, source, out);
        }
    }
}

/// Both sides of a set operation, flattened.
///
/// Flattened rather than nested because `a UNION b UNION c` is three arms to a
/// reader and a right-leaning tree to the grammar, and nobody tracing a column
/// wants to walk a tree to find the third one.
fn arms_into(node: Node, source: &str, out: &mut Projection) {
    for field in ["left", "right"] {
        let Some(side) = node.child_by_field_name(field) else {
            out.opaque = true;
            continue;
        };
        match side.kind() {
            "set_operation" => arms_into(side, source, out),
            "select_core" => {
                let mut arm = Projection::default();
                core_into(side, source, &mut arm);
                out.arms.push(arm);
            }
            "subquery" => match select_in(side).and_then(|s| projection_of(s, source)) {
                Some(arm) => out.arms.push(arm),
                None => out.opaque = true,
            },
            // A `VALUES` arm produces rows from nothing; it contributes no source.
            _ => out.opaque = true,
        }
    }
}

/// One projected item.
fn projected(item: Node, source: &str) -> Projected {
    let alias = item.child_by_field_name("alias");
    let alias_name = alias.map(|a| fold(text_of(a, source)));

    // The projected expression is the first named child that is not the alias.
    let mut cursor = item.walk();
    let expr = item
        .named_children(&mut cursor)
        .find(|ch| alias.map(|a| a.id()) != Some(ch.id()));

    let Some(expr) = expr else {
        return Projected::Computed { output: alias_name.unwrap_or_default(), reads: Vec::new() };
    };

    if expr.kind() == "all_columns" {
        // `t.*` carries the qualifier as an `object_name` child; a bare `*` has none.
        let qualifier = expr
            .named_children(&mut expr.walk())
            .find(|c| c.kind() == "object_name")
            .and_then(|n| object_ref(n, source, ObjectKind::Table))
            .map(|r| r.folded_name());
        return Projected::Star { qualifier };
    }

    if expr.kind() == "object_name" {
        if let Some(reference) = column_source(expr, source) {
            let output = alias_name.unwrap_or_else(|| reference.column.clone());
            return Projected::Column { output, source: reference };
        }
    }

    // Anything else: an expression, a function, a CASE, a cast, a subquery. Its
    // value is not any one column, so every reference inside it is collected and the
    // item is reported as computed.
    let mut reads = Vec::new();
    collect_columns(expr, source, &mut reads);
    Projected::Computed { output: alias_name.unwrap_or_default(), reads }
}

/// An `object_name` read as a column reference: trailing part the column, the one
/// before it the qualifier.
///
/// A three-part `schema.table.column` keeps only `table` as the qualifier, because
/// that is what a `FROM` item's name can ever match — a source is called by its
/// alias or its bare name, never by a schema.
fn column_source(node: Node, source: &str) -> Option<ColumnSource> {
    let reference = object_ref(node, source, ObjectKind::Column)?;
    Some(ColumnSource::new(reference.folded_schema(), reference.folded_name()))
}

/// Every column reference inside an expression, in order of appearance.
///
/// Deliberately shallow about **subqueries**: a scalar subquery in the projection
/// reads from its own `FROM`, and its column names mean nothing in the outer
/// statement's scope. Attributing them to the outer sources would be inventing a
/// relationship, so the walk stops at the boundary.
fn collect_columns(node: Node, source: &str, out: &mut Vec<ColumnSource>) {
    if node.kind() == "subquery" {
        return;
    }
    if node.kind() == "object_name" {
        // A function's *name* is an `object_name` too. It is the child of a
        // `function_call`, which is handled below by skipping that field.
        if let Some(reference) = column_source(node, source) {
            if !out.contains(&reference) {
                out.push(reference);
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        // Skip the callee of a function call: `upper(nome)` reads `nome`, and
        // recording `upper` as a column would put a function in the lineage.
        if node.kind() == "function_call" && child.kind() == "object_name" {
            let first_name = node
                .named_children(&mut node.walk())
                .find(|c| c.kind() == "object_name")
                .map(|c| c.id());
            if first_name == Some(child.id()) {
                continue;
            }
        }
        collect_columns(child, source, out);
    }
}

/// The `FROM` items of a clause, joins flattened.
fn from_into(clause: Node, source: &str, out: &mut Projection) {
    let mut cursor = clause.walk();
    for child in clause.children(&mut cursor) {
        item_into(child, source, out);
    }
}

/// One `FROM` element, which may itself be a join tree.
fn item_into(node: Node, source: &str, out: &mut Projection) {
    match node.kind() {
        // A join contributes both its sides; the condition is not a source.
        "join_clause" => {
            for field in ["left", "right"] {
                if let Some(side) = node.child_by_field_name(field) {
                    item_into(side, source, out);
                }
            }
        }
        "table_reference" => {
            let Some(name_node) = node.child_by_field_name("name") else {
                out.opaque = true;
                return;
            };
            let Some(reference) = object_ref(name_node, source, ObjectKind::Table) else {
                out.opaque = true;
                return;
            };
            // The alias is what a qualifier matches; without one it is the bare
            // name, never the schema-qualified one.
            let called = node
                .child_by_field_name("alias")
                .map(|a| fold(text_of(a, source)))
                .unwrap_or_else(|| reference.folded_name());
            out.from.push(FromItem {
                name: called,
                source: FromSource::Relation { name: reference.folded_qualified() },
                column_aliases: Vec::new(),
            });
        }
        "derived_table" | "parenthesized_table" | "lateral_table" => {
            let inner = node
                .named_children(&mut node.walk())
                .find(|c| c.kind() == "subquery")
                .and_then(select_in)
                .and_then(|s| projection_of(s, source));
            let called = node
                .child_by_field_name("alias")
                .map(|a| fold(text_of(a, source)))
                .unwrap_or_default();
            let column_aliases = aliases_of(node, source);

            // A `parenthesized_table` wrapping a join is not a derived table at all —
            // it is grouping. Its contents are contributed to the same scope.
            if inner.is_none() && node.kind() == "parenthesized_table" {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    item_into(child, source, out);
                }
                return;
            }

            match inner {
                Some(projection) => out.from.push(FromItem {
                    name: called,
                    source: FromSource::Derived { projection: Box::new(projection) },
                    column_aliases,
                }),
                // A `LATERAL f(x)` or anything else with no query inside it.
                None => out.from.push(FromItem {
                    name: called,
                    source: FromSource::Opaque,
                    column_aliases,
                }),
            }
        }
        // A set-returning function, Oracle's `dual`, or a construct not modelled.
        // Recorded rather than skipped so a trail that reaches it can say so.
        "function_table" | "dual_reference" => out.from.push(FromItem {
            name: String::new(),
            source: FromSource::Opaque,
            column_aliases: Vec::new(),
        }),
        _ => {}
    }
}

/// `AS x(a, b, c)` — the names imposed on an item's columns, folded, in order.
fn aliases_of(node: Node, source: &str) -> Vec<String> {
    let Some(list) = node.named_children(&mut node.walk()).find(|c| c.kind() == "column_aliases")
    else {
        return Vec::new();
    };
    let mut cursor = list.walk();
    list.children(&mut cursor)
        .filter(|c| matches!(c.kind(), "identifier" | "quoted_identifier"))
        .map(|c| fold(text_of(c, source)))
        .collect()
}

/// The `WITH` names of a clause.
fn ctes_of(clause: Node, source: &str) -> Vec<Cte> {
    let mut out = Vec::new();
    let mut cursor = clause.walk();
    for child in clause.children(&mut cursor) {
        if child.kind() != "common_table_expression" {
            continue;
        }
        let Some(name) = child.child_by_field_name("name") else { continue };
        let query = child
            .child_by_field_name("query")
            .filter(|q| q.kind() == "select_statement")
            .and_then(|q| projection_of(q, source));
        out.push(Cte {
            name: fold(text_of(name, source)),
            projection: query,
            column_aliases: aliases_of(child, source),
        });
    }
    out
}

/// The `select_statement` inside a `subquery` node.
fn select_in(subquery: Node) -> Option<Node> {
    subquery.named_children(&mut subquery.walk()).find(|c| c.kind() == "select_statement")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::project;
    use picus_types::prelude::{DialectScope, EngineKind};

    fn p(sql: &str) -> Projection {
        project(sql, DialectScope::One(EngineKind::Postgres))
            .unwrap_or_else(|| panic!("no projection for {sql:?}"))
    }

    #[test]
    fn an_alias_keeps_the_column_it_stands_for() {
        // The case the whole feature exists for: the output name says nothing about
        // where the value is, and the qualifier is the only way back.
        let projection = p("SELECT t.cenint AS codsa FROM tab_tipi t");
        let Projected::Column { output, source } = &projection.items[0] else {
            panic!("expected a plain column, got {:?}", projection.items[0]);
        };
        assert_eq!(output, "CODSA");
        assert_eq!(source.qualifier.as_deref(), Some("T"));
        assert_eq!(source.column, "CENINT");

        let from = projection.source_named(Some("T")).expect("the alias names the source");
        assert_eq!(from.source, FromSource::Relation { name: "TAB_TIPI".into() });
    }

    #[test]
    fn an_unqualified_column_is_attributed_only_when_there_is_one_source() {
        let one = p("SELECT nome FROM documenti");
        assert!(one.source_named(None).is_some());

        // Two sources and a bare name: which of them has `nome` takes a catalogue,
        // so the parser refuses rather than picking the first.
        let two = p("SELECT nome FROM documenti d JOIN allegati a ON a.doc = d.id");
        assert!(two.source_named(None).is_none());
        assert_eq!(two.from.len(), 2, "a join contributes both sides");
    }

    #[test]
    fn a_computed_item_reads_but_does_not_come_from() {
        let projection = p("SELECT a.inizio || '-' || b.fine AS periodo FROM a, b");
        let Projected::Computed { output, reads } = &projection.items[0] else {
            panic!("expected a computed item, got {:?}", projection.items[0]);
        };
        assert_eq!(output, "PERIODO");
        assert_eq!(reads.len(), 2, "both halves are read");
        assert_eq!(reads[0].column, "INIZIO");
        assert_eq!(reads[1].column, "FINE");
    }

    #[test]
    fn a_function_name_is_not_a_column() {
        let projection = p("SELECT upper(nome) AS n FROM t");
        let Projected::Computed { reads, .. } = &projection.items[0] else {
            panic!("expected a computed item");
        };
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].column, "NOME", "`upper` must not enter the lineage");
    }

    #[test]
    fn a_star_is_carried_unexpanded() {
        assert_eq!(p("SELECT * FROM t").items[0], Projected::Star { qualifier: None });
        assert_eq!(
            p("SELECT t.* FROM tab t").items[0],
            Projected::Star { qualifier: Some("T".into()) },
        );
    }

    #[test]
    fn a_derived_table_carries_its_own_projection() {
        let projection = p("SELECT x.n FROM (SELECT nome AS n FROM documenti) x");
        let from = projection.source_named(Some("X")).expect("the derived table is named x");
        let FromSource::Derived { projection: inner } = &from.source else {
            panic!("expected a derived table, got {:?}", from.source);
        };
        assert_eq!(inner.items[0].output(), "N");
        assert!(inner.source_named(None).is_some());
    }

    #[test]
    fn a_union_becomes_arms_and_no_projection_of_its_own() {
        let projection = p("SELECT a FROM t1 UNION SELECT b FROM t2 UNION SELECT c FROM t3");
        assert!(projection.items.is_empty(), "a union has no single projection");
        assert_eq!(projection.arms.len(), 3, "the arms are flattened, not nested");
        assert_eq!(projection.arms[2].items[0].output(), "C");
    }

    #[test]
    fn a_with_name_is_recorded_so_it_can_shadow_a_table() {
        let projection = p("WITH recenti AS (SELECT id FROM ordini) SELECT id FROM recenti");
        assert_eq!(projection.ctes.len(), 1);
        assert_eq!(projection.ctes[0].name, "RECENTI");
        assert!(projection.ctes[0].projection.is_some());
        assert_eq!(projection.from[0].name, "RECENTI");
    }

    #[test]
    fn a_scalar_subquery_does_not_leak_its_columns_outward() {
        // `(SELECT max(x) FROM altro)` reads `altro.x`, which means nothing in the
        // outer scope — attributing it to `t` would invent a relationship.
        let projection = p("SELECT (SELECT max(x) FROM altro) AS m FROM t");
        let Projected::Computed { reads, .. } = &projection.items[0] else {
            panic!("expected a computed item");
        };
        assert!(reads.is_empty(), "the subquery's own columns stay inside it");
    }

    #[test]
    fn a_values_statement_says_it_has_no_trail() {
        assert!(p("VALUES (1, 2)").opaque);
    }
}
