//! The resolution, branch by branch, against a catalogue made of maps.
//!
//! Every one of these is a shape a real schema of a couple of hundred views
//! actually contains — a rename, a view on a view, a computed column, a union of an
//! archive with the live table — and the reason they can be tested at all is that
//! the database is a trait.

use std::collections::HashMap;

use picus_types::prelude::EngineKind;

use crate::prelude::*;

/// A catalogue built from literals.
#[derive(Default)]
struct Fake {
    /// relation → its columns.
    columns: HashMap<String, Vec<String>>,
    /// view → its SELECT.
    views: HashMap<String, String>,
}

impl Fake {
    fn table(mut self, name: &str, columns: &[&str]) -> Self {
        self.columns
            .insert(name.into(), columns.iter().map(|c| c.to_string()).collect());
        self
    }

    fn view(mut self, name: &str, columns: &[&str], sql: &str) -> Self {
        self.views.insert(name.into(), sql.into());
        self.table(name, columns)
    }
}

impl Catalogue for Fake {
    fn is_view(&self, relation: &str) -> Option<bool> {
        if self.views.contains_key(relation) {
            return Some(true);
        }
        self.columns.contains_key(relation).then_some(false)
    }

    fn definition(&self, view: &str) -> Option<String> {
        self.views.get(view).cloned()
    }

    fn columns(&self, relation: &str) -> Option<Vec<String>> {
        self.columns.get(relation).cloned()
    }
}

fn trace(catalogue: &Fake, relation: &str, column: &str) -> Trace {
    let lineage = trace_relation(catalogue, relation, EngineKind::Postgres);
    lineage
        .columns
        .into_iter()
        .find(|t| t.output == column)
        .unwrap_or_else(|| panic!("`{relation}` produced no column `{column}`"))
}

#[test]
fn a_rename_one_level_deep_names_the_table_and_the_original_column() {
    // The case the feature exists for, at its smallest.
    let catalogue = Fake::default()
        .table("TAB_TIPI", &["CENINT", "TAB1TIP"])
        .view("V_TIPI", &["CODSA"], "SELECT t.cenint AS codsa FROM tab_tipi t");

    let traced = trace(&catalogue, "V_TIPI", "CODSA");
    assert_eq!(traced.verdict, Verdict::Resolved);
    assert_eq!(traced.base_relation(), "TAB_TIPI");
    assert_eq!(traced.base_column(), "CENINT");
    assert!(traced.renamed(), "CODSA is not what the table calls it");
    assert_eq!(traced.hops.len(), 1);
}

#[test]
fn a_view_on_a_view_keeps_every_hop() {
    // Views on views is the situation; the chain is the answer, not just its end.
    let catalogue = Fake::default()
        .table("TAB_TIPI", &["CENINT"])
        .view("V_TIPI", &["CODSA"], "SELECT t.cenint AS codsa FROM tab_tipi t")
        .view("V_ELENCHI", &["CODICE_SA"], "SELECT v.codsa AS codice_sa FROM v_tipi v");

    let traced = trace(&catalogue, "V_ELENCHI", "CODICE_SA");
    assert_eq!(traced.verdict, Verdict::Resolved);
    assert_eq!(traced.base_relation(), "TAB_TIPI");
    assert_eq!(traced.base_column(), "CENINT");

    let chain: Vec<(&str, &str)> =
        traced.hops.iter().map(|h| (h.relation.as_str(), h.column.as_str())).collect();
    assert_eq!(chain, vec![("V_TIPI", "CODSA"), ("TAB_TIPI", "CENINT")]);
    assert!(traced.hops[0].is_view, "the middle hop is a view and must say so");
    assert!(!traced.hops[1].is_view, "the last hop is the table the trail was after");
}

#[test]
fn a_star_is_expanded_through_the_catalogue() {
    let catalogue = Fake::default()
        .table("ORDINI", &["ID", "CODICE"])
        .view("V_ORDINI", &["ID", "CODICE"], "SELECT * FROM ordini");

    let lineage = trace_relation(&catalogue, "V_ORDINI", EngineKind::Postgres);
    assert_eq!(lineage.columns.len(), 2, "a star is as many columns as its source has");
    assert!(lineage.columns.iter().all(|t| t.base_relation() == "ORDINI"));
    assert!(lineage.columns.iter().all(|t| !t.renamed()));
}

#[test]
fn a_computed_column_names_its_ingredients_and_resolves_to_nothing() {
    let catalogue = Fake::default()
        .table("GARE", &["DINIZVAL", "DTERMVAL"])
        .view(
            "V_PERIODI",
            &["PERIODO"],
            "SELECT g.dinizval || ' - ' || g.dtermval AS periodo FROM gare g",
        );

    let traced = trace(&catalogue, "V_PERIODI", "PERIODO");
    assert_eq!(traced.verdict, Verdict::Derived);
    assert_eq!(traced.base_relation(), "", "a computed value has no one table");
    let reads: Vec<(&str, &str)> =
        traced.reads.iter().map(|r| (r.relation.as_str(), r.column.as_str())).collect();
    assert_eq!(reads, vec![("GARE", "DINIZVAL"), ("GARE", "DTERMVAL")]);
}

#[test]
fn an_ambiguous_bare_column_stops_rather_than_picking_one() {
    // Both tables have `CODICE` and the SQL does not say which. Guessing here would
    // be a plausible wrong table for someone about to write an UPDATE.
    let catalogue = Fake::default()
        .table("GARE", &["CODICE", "ID"])
        .table("LOTTI", &["CODICE", "GARA"])
        .view("V_AMB", &["CODICE"], "SELECT codice FROM gare, lotti");

    let traced = trace(&catalogue, "V_AMB", "CODICE");
    assert_eq!(traced.verdict, Verdict::Unresolved);
    assert!(traced.stopped.contains("without a table"), "got: {}", traced.stopped);
    assert!(traced.stopped.contains('2'), "the reason names how many could have it");
}

#[test]
fn a_single_source_is_not_a_default_answer_for_every_name() {
    // THE regression this file exists for. A projection Picus cannot read, plus one
    // readable `FROM` item, used to attribute every column of the relation to that
    // item under its own name — a full page of `X ← TORN.X` that reads like a
    // lineage and is an echo of the column list. A name is attributed because the
    // source *carries* it, never because it was the only one to hand.
    let catalogue = Fake::default()
        .table("TORN", &["CODGAR", "TIPTOR"])
        .table("ALTRA", &["NPROAT"])
        // The projection names nothing this can follow; only the FROM is legible.
        .view("V_ECO", &["NPROAT"], "SELECT (SELECT 1) FROM torn");

    let traced = trace(&catalogue, "V_ECO", "NPROAT");
    assert_ne!(traced.verdict, Verdict::Resolved, "nothing here justifies a table");
    assert_eq!(traced.base_relation(), "", "TORN must not be handed out as a default");
}

#[test]
fn a_bare_column_the_only_source_does_not_have_is_refused() {
    // Same rule seen from the other side: one source, one bare name, and the
    // catalogue says that source has no such column. Naming it anyway would be the
    // shortcut that produced the echo.
    let catalogue = Fake::default()
        .table("TORN", &["CODGAR"])
        .view("V_NOPE", &["NPROAT"], "SELECT nproat FROM torn");

    let traced = trace(&catalogue, "V_NOPE", "NPROAT");
    assert_eq!(traced.verdict, Verdict::Unresolved);
    assert!(traced.stopped.contains("without a table"), "got: {}", traced.stopped);
}

#[test]
fn a_bare_column_only_one_source_has_is_attributed_to_it() {
    let catalogue = Fake::default()
        .table("GARE", &["OGGETTO", "ID"])
        .table("LOTTI", &["GARA"])
        .view("V_OK", &["OGGETTO"], "SELECT oggetto FROM gare, lotti");

    let traced = trace(&catalogue, "V_OK", "OGGETTO");
    assert_eq!(traced.verdict, Verdict::Resolved);
    assert_eq!(traced.base_relation(), "GARE");
}

#[test]
fn a_derived_table_is_walked_through_without_becoming_a_hop() {
    // A subquery in FROM has no name of its own to report; the chain must not gain a
    // step for something the reader cannot go and look at.
    let catalogue = Fake::default().table("GARE", &["NGARA"]).view(
        "V_DER",
        &["N"],
        "SELECT x.n FROM (SELECT ngara AS n FROM gare) x",
    );

    let traced = trace(&catalogue, "V_DER", "N");
    assert_eq!(traced.verdict, Verdict::Resolved);
    assert_eq!(traced.base_relation(), "GARE");
    assert_eq!(traced.base_column(), "NGARA");
    assert_eq!(traced.hops.len(), 1, "the derived table is not a hop");
}

#[test]
fn a_union_of_one_table_resolves_to_it() {
    // The live-plus-archive view, which is the union worth resolving.
    let catalogue = Fake::default().table("GARE", &["NGARA", "STATO"]).view(
        "V_TUTTE",
        &["NGARA"],
        "SELECT ngara FROM gare WHERE stato = 'A' UNION ALL SELECT ngara FROM gare WHERE stato = 'C'",
    );

    let traced = trace(&catalogue, "V_TUTTE", "NGARA");
    assert_eq!(traced.verdict, Verdict::Resolved);
    assert_eq!(traced.base_relation(), "GARE");
}

#[test]
fn a_union_of_two_tables_is_reported_as_having_both() {
    let catalogue = Fake::default()
        .table("GARE", &["NGARA"])
        .table("GARE_STORICO", &["NGARA"])
        .view(
            "V_TUTTE",
            &["NGARA"],
            "SELECT ngara FROM gare UNION ALL SELECT ngara FROM gare_storico",
        );

    let traced = trace(&catalogue, "V_TUTTE", "NGARA");
    // `Split`, never `Derived`. The value IS a real column — of one table for some
    // rows and the other for the rest — and calling that "computed, nothing to write
    // back through" is false twice over.
    assert_eq!(traced.verdict, Verdict::Split, "two origins is not one origin, nor a computation");
    let mut bases: Vec<&str> = traced.reads.iter().map(|r| r.relation.as_str()).collect();
    bases.sort_unstable();
    assert_eq!(bases, vec!["GARE", "GARE_STORICO"]);
    assert!(traced.reads.iter().all(|r| r.column == "NGARA"), "each names its own column");
}

#[test]
fn a_union_with_one_computed_arm_is_computed_and_names_every_arm() {
    // Mixed: one arm reads a column, the other builds one. The column as a whole
    // cannot be written back through, so `Derived` is right — but the arm that *did*
    // resolve is still part of what this value is, and used to be dropped because the
    // walk returned on the first computed arm it met.
    let catalogue = Fake::default()
        .table("GARE", &["NGARA", "PREFISSO"])
        .table("GARE_STORICO", &["NGARA"])
        .view(
            "V_MISTA",
            &["NGARA"],
            "SELECT ngara FROM gare_storico UNION ALL SELECT prefisso || ngara FROM gare",
        );

    let traced = trace(&catalogue, "V_MISTA", "NGARA");
    assert_eq!(traced.verdict, Verdict::Derived);
    let mut named: Vec<&str> = traced.reads.iter().map(|r| r.relation.as_str()).collect();
    named.sort_unstable();
    named.dedup();
    assert_eq!(named, vec!["GARE", "GARE_STORICO"], "the resolved arm is not dropped");
}

#[test]
fn a_relation_outside_the_catalogue_says_so_rather_than_failing() {
    let catalogue = Fake::default()
        .view("V_ALTRO", &["X"], "SELECT a.x FROM altro_schema.tabella a");

    let traced = trace(&catalogue, "V_ALTRO", "X");
    assert_eq!(traced.verdict, Verdict::Unresolved);
    assert!(traced.stopped.contains("catalogue"), "got: {}", traced.stopped);
}

#[test]
fn a_with_name_shadows_a_table_of_the_same_name() {
    // `WITH gare AS (…)` is not the table `GARE`, and resolving it to one would be a
    // confident wrong answer about a real database.
    let catalogue = Fake::default()
        .table("GARE", &["NGARA"])
        .table("STORICO", &["NGARA"])
        .view(
            "V_CTE",
            &["N"],
            "WITH gare AS (SELECT ngara FROM storico) SELECT g.ngara AS n FROM gare g",
        );

    let traced = trace(&catalogue, "V_CTE", "N");
    assert_eq!(traced.verdict, Verdict::Resolved);
    assert_eq!(traced.base_relation(), "STORICO", "the CTE shadows the table");
}

#[test]
fn a_statement_is_traced_through_the_view_it_reads() {
    // Tracing the result on screen: the statement's own FROM is the first step.
    let catalogue = Fake::default()
        .table("TAB_TIPI", &["CENINT"])
        .view("V_TIPI", &["CODSA"], "SELECT t.cenint AS codsa FROM tab_tipi t");

    let lineage =
        trace_statement(&catalogue, "SELECT v.codsa AS sa FROM v_tipi v", EngineKind::Postgres);
    assert_eq!(lineage.columns.len(), 1);
    assert_eq!(lineage.columns[0].output, "SA");
    assert_eq!(lineage.columns[0].base_relation(), "TAB_TIPI");
    assert_eq!(lineage.through, vec!["V_TIPI".to_string()], "the stack passed through");
}

#[test]
fn the_legend_of_a_lineage_is_its_distinct_base_tables() {
    let catalogue = Fake::default()
        .table("GARE", &["NGARA", "OGGETTO"])
        .table("TAB_TIPI", &["CENINT"])
        .view(
            "V_MISTA",
            &["NGARA", "OGGETTO", "CODSA", "CALCOLATO"],
            "SELECT g.ngara, g.oggetto, t.cenint AS codsa, upper(g.oggetto) AS calcolato \
             FROM gare g, tab_tipi t",
        );

    let lineage = trace_relation(&catalogue, "V_MISTA", EngineKind::Postgres);
    assert_eq!(lineage.base_relations(), vec!["GARE".to_string(), "TAB_TIPI".to_string()]);
    // The computed one contributes no table, which is what stops a legend from
    // claiming a column comes from somewhere it does not.
    let computed = lineage.columns.iter().find(|t| t.output == "CALCOLATO").unwrap();
    assert_eq!(computed.verdict, Verdict::Derived);
}

#[test]
fn a_table_traces_to_itself_in_one_hop() {
    // Not a special case in the code, and it must not become one: asking about a
    // table is a reasonable thing to do and deserves a true answer.
    let catalogue = Fake::default().table("GARE", &["NGARA"]);
    let lineage = trace_relation(&catalogue, "GARE", EngineKind::Postgres);
    assert!(lineage.columns.is_empty(), "a table has no projection to trace");
    assert!(lineage.through.is_empty());
}

#[test]
fn the_wire_names_are_the_ones_the_panel_reads() {
    let catalogue = Fake::default()
        .table("TAB_TIPI", &["CENINT"])
        .view("V_TIPI", &["CODSA"], "SELECT t.cenint AS codsa FROM tab_tipi t");
    let lineage = trace_relation(&catalogue, "V_TIPI", EngineKind::Postgres);
    let json = serde_json::to_value(&lineage).unwrap();

    for key in ["relation", "columns", "through", "truncated"] {
        assert!(json.get(key).is_some(), "missing `{key}`");
    }
    let column = &json["columns"][0];
    for key in ["output", "verdict", "hops", "reads", "stopped"] {
        assert!(column.get(key).is_some(), "missing `columns[].{key}`");
    }
    assert_eq!(column["verdict"], "resolved");
    assert_eq!(column["hops"][0]["isView"], true);
}
