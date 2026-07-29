//! `columns` domain — what a table's columns are when **no database says**.
//!
//! Picus maintains scripts, and a great many of the tables in them are not in any
//! database anybody has connected to: another repository installs them, the
//! customer's instance is not reachable, the schema is on a laptop somewhere. The
//! generator still has to be usable against those, and until now it was not — the
//! table could be picked and the form came up with no fields at all.
//!
//! ## The scripts are the schema, near enough
//!
//! The grammar does not read `CREATE TABLE` column definitions, and teaching it to
//! is a large piece of work for a smaller payoff than this: the columns worth
//! offering are the ones the repository **actually writes**, and those are in every
//! `INSERT` in the file. A table with forty columns that the scripts only ever seed
//! six of should offer six.
//!
//! Order is first-appearance, which is the order the scripts write them in and
//! therefore the order the person filling the form is thinking in.
//!
//! ## Types are inferred from the literals, and that is not a guess about the schema
//!
//! It is a record of **how the value is written**: a column every script writes as
//! a bare number is emitted bare, one written quoted is re-quoted. So a row typed
//! into the form comes out in the same shape as the rows already in the file, which
//! is the only property that matters here. What is lost is the checking — no length
//! limits, no `NOT NULL` — and the interface says so.
//!
//! A column written both ways widens to textual: quoting is the safe direction.

use std::collections::BTreeMap;

use picus_analyze::prelude::fold_identifier;
use picus_ast::prelude::Column;
use picus_core::prelude::PicusState;
use picus_parse::prelude::{DmlShape, LiteralValue, SqlParser};

use crate::scripts::snapshot_for;

/// How a column's values are written across the repository.
#[derive(Default)]
struct Seen {
    /// First position it appeared at, so the answer keeps the scripts' own order.
    first: usize,
    /// How many statements name it — the tie-break for the spelling to hand back.
    mentions: usize,
    /// Every literal seen for it was a bare number.
    numeric: bool,
    /// A literal was seen at all. A column only ever written `NULL` says nothing
    /// about its type, and `numeric` on its own would then be vacuously true.
    literal: bool,
    /// The spelling as first written, which is what the user recognises.
    name: String,
}

/// The columns this repository's scripts write into `table`.
///
/// Empty when nothing writes to it — an object the scripts only read, or a name
/// that is not in this repository at all. The caller shows that as "nothing is
/// known about this table's columns" rather than as an error: it is a true
/// statement about the repository, not a failure.
#[arbor_rpc::handler]
fn picus_script_columns(
    state: &PicusState,
    root: String,
    table: String,
) -> Result<Vec<Column>, String> {
    let snapshot = snapshot_for(state, &root)?;
    let wanted = fold_identifier(&table);
    let mut parser = SqlParser::new();
    let mut seen: BTreeMap<String, Seen> = BTreeMap::new();
    let mut at = 0usize;

    for folder in snapshot.project.walk() {
        if folder.is_excluded() || folder.engine_is_unsupported() {
            continue;
        }
        let Some(scope) = folder.scope() else { continue };
        for file in &folder.files {
            if file.is_out_of_scope() {
                continue;
            }
            let Some(source) = snapshot.source(&file.path) else { continue };
            let parsed = parser.parse(&source.text, scope);
            for statement in &parsed.statements {
                for shape in &statement.dml {
                    if shape.table.folded_name() != wanted {
                        continue;
                    }
                    absorb(&mut seen, shape, &mut at);
                }
            }
        }
    }

    let mut columns: Vec<(&Seen, Column)> = seen
        .values()
        .map(|s| {
            (
                s,
                Column {
                    name: s.name.clone(),
                    // A column nobody ever wrote a literal into is textual: quoting
                    // is the safe direction, and re-quoting a number is valid SQL
                    // where emitting a string bare is not.
                    data_type: if s.literal && s.numeric { "numeric" } else { "text" }.to_string(),
                    primary_key: false,
                    not_null: false,
                    default_value: None,
                },
            )
        })
        .collect();
    columns.sort_by_key(|(s, _)| s.first);
    Ok(columns.into_iter().map(|(_, c)| c).collect())
}

fn absorb(seen: &mut BTreeMap<String, Seen>, shape: &DmlShape, at: &mut usize) {
    // Positional `INSERT INTO t VALUES (…)` names nothing, so it teaches nothing.
    // It is also `DML002`, which the report already raises.
    if !shape.has_column_list && shape.assignments.is_empty() {
        return;
    }

    let mut note = |name: String, literal: Option<&LiteralValue>| {
        let key = name.to_uppercase();
        let entry = seen.entry(key).or_insert_with(|| {
            *at += 1;
            Seen { first: *at, numeric: true, name: name.clone(), ..Seen::default() }
        });
        entry.mentions += 1;
        match literal {
            Some(LiteralValue::Number(_)) => entry.literal = true,
            // A string, a boolean — anything that is not a bare number — settles it
            // the other way and cannot be widened back.
            Some(LiteralValue::String(_) | LiteralValue::Bool(_)) => {
                entry.literal = true;
                entry.numeric = false;
            }
            // `NULL`, and an expression the parser could not reduce to a literal,
            // say nothing about how the column is written.
            _ => {}
        }
    };

    for (index, column) in shape.columns.iter().enumerate() {
        // The first row is enough to type a column: a repository that writes the
        // same column as a number in one row and a string in the next has a
        // problem this is not the place to discover.
        let literal =
            shape.rows.first().and_then(|row| row.values.get(index)).and_then(|c| c.literal.as_ref());
        note(column.name.clone(), literal);
    }
    for assignment in &shape.assignments {
        note(assignment.column.name.clone(), assignment.value.literal.as_ref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_parse::prelude::{DialectScope, EngineKind};

    /// Read one file's worth of statements the way the handler does, without a
    /// repository behind it — the folding is what is being asserted, not the walk.
    fn names_of(source: &str, table: &str) -> Vec<String> {
        columns_of(source, table).into_iter().map(|c| c.name).collect()
    }

    fn columns_of(source: &str, table: &str) -> Vec<Column> {
        let parsed = SqlParser::new().parse(source, DialectScope::One(EngineKind::Oracle));
        let wanted = fold_identifier(table);
        let mut seen: BTreeMap<String, Seen> = BTreeMap::new();
        let mut at = 0usize;
        for statement in &parsed.statements {
            for shape in &statement.dml {
                if shape.table.folded_name() == wanted {
                    absorb(&mut seen, shape, &mut at);
                }
            }
        }
        let mut out: Vec<(&Seen, Column)> = seen
            .values()
            .map(|s| {
                (
                    s,
                    Column {
                        name: s.name.clone(),
                        data_type: if s.literal && s.numeric { "numeric" } else { "text" }
                            .to_string(),
                        primary_key: false,
                        not_null: false,
                        default_value: None,
                    },
                )
            })
            .collect();
        out.sort_by_key(|(s, _)| s.first);
        out.into_iter().map(|(_, c)| c).collect()
    }

    #[test]
    fn the_columns_are_the_ones_the_scripts_write_in_the_order_they_write_them() {
        let source = "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA, ORDINE) \
                      VALUES ('A', 'Alfa', 1);\r\n";
        let names = names_of(source, "CATALOGO_WIDGET");
        assert_eq!(names, ["CHIAVE", "ETICHETTA", "ORDINE"]);
    }

    #[test]
    fn a_type_records_how_the_value_is_written() {
        let source = "INSERT INTO CATALOGO_WIDGET (CHIAVE, ORDINE) VALUES ('A', 1);\r\n";
        let columns = columns_of(source, "CATALOGO_WIDGET");
        assert_eq!(columns[0].data_type, "text", "written quoted, so re-quoted");
        assert_eq!(columns[1].data_type, "numeric", "written bare, so emitted bare");
    }

    #[test]
    fn a_column_written_both_ways_widens_to_text() {
        // Quoting is the safe direction: re-quoting a number is valid SQL, and
        // emitting a string bare is not.
        let source = "INSERT INTO CATALOGO_WIDGET (ORDINE) VALUES (1);\r\n\
                      INSERT INTO CATALOGO_WIDGET (ORDINE) VALUES ('primo');\r\n";
        assert_eq!(columns_of(source, "CATALOGO_WIDGET")[0].data_type, "text");
    }

    #[test]
    fn a_column_only_ever_written_null_is_text_rather_than_numeric() {
        // The trap in the fold: "every literal seen was a number" is vacuously
        // true when none was seen, which would emit the value unquoted.
        let source = "INSERT INTO CATALOGO_WIDGET (NOTA) VALUES (NULL);\r\n";
        assert_eq!(columns_of(source, "CATALOGO_WIDGET")[0].data_type, "text");
    }

    #[test]
    fn an_update_contributes_its_assigned_columns() {
        // A table the repository only ever updates still has columns worth
        // offering.
        let source = "UPDATE CATALOGO_WIDGET SET ETICHETTA = 'Alfa', ORDINE = 2 \
                      WHERE CHIAVE = 'A';\r\n";
        let names = names_of(source, "CATALOGO_WIDGET");
        assert!(names.iter().any(|n| n == "ETICHETTA"), "{names:?}");
        assert!(names.iter().any(|n| n == "ORDINE"), "{names:?}");
    }

    #[test]
    fn an_insert_with_no_column_list_teaches_nothing() {
        // It binds to the table's current column order and names nothing — which
        // is `DML002`, and is exactly why it cannot be read as a column list.
        let source = "INSERT INTO CATALOGO_WIDGET VALUES ('A', 'Alfa', 1);\r\n";
        assert!(columns_of(source, "CATALOGO_WIDGET").is_empty());
    }

    #[test]
    fn another_tables_statements_are_not_folded_in() {
        let source = "INSERT INTO CATALOGO_WIDGET (CHIAVE) VALUES ('A');\r\n\
                      INSERT INTO STAGING_IMPORT (RIGA) VALUES ('x');\r\n";
        let names = names_of(source, "CATALOGO_WIDGET");
        assert_eq!(names, ["CHIAVE"]);
    }

    #[test]
    fn one_column_spelled_two_ways_is_one_column() {
        // PostgreSQL folds an unquoted name down and Oracle up, so a repository
        // writing both has written one column twice.
        let source = "INSERT INTO CATALOGO_WIDGET (chiave) VALUES ('A');\r\n\
                      INSERT INTO CATALOGO_WIDGET (CHIAVE) VALUES ('B');\r\n";
        assert_eq!(columns_of(source, "CATALOGO_WIDGET").len(), 1);
    }
}
