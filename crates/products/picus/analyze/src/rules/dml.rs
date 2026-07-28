//! `DML001` / `DML002` — two statements that do more than they look like they do.
//!
//! Both are `review` rather than `blocking`, and both are the rules the
//! suppression comment exists for: a full reload on install genuinely is a
//! `DELETE` with no `WHERE`, and saying so in the script is better than a rule
//! that pretends to know the difference.

use picus_parse::prelude::{DmlOperation, DmlShape};

use crate::context::Context;
use crate::finding::{Anchor, Finding};
use crate::report::Output;
use crate::rule::RuleId;

pub(crate) fn run(context: &Context<'_>, output: &mut Output) {
    let version_table = context.version_table.as_deref();
    for (script, _) in context.project.placed() {
        for statement in &script.parsed.statements {
            for shape in &statement.dml {
                let line = script.parsed.line_of(shape.table.range.start);
                let anchor = || Anchor::at(script.path, line);

                if unguarded_write(shape, version_table) {
                    output.findings.push(unguarded_write_finding(anchor(), shape));
                }
                if column_less_insert(shape) {
                    output.findings.push(column_less_insert_finding(anchor(), shape));
                }
            }
        }
    }
}

/// A `DELETE` or an `UPDATE` with no `WHERE`.
///
/// The `UPDATE` half is the one that costs more in practice. A `DELETE` that
/// empties a table is at least visible the next time anybody looks; an `UPDATE`
/// runs clean, touches every row, and the first sign of it is a report reading
/// the wrong numbers weeks later.
///
/// `TRUNCATE` is deliberately not included: it is a different statement, it says
/// what it does in its own name, and nobody writes it by accident.
fn unguarded_write(shape: &DmlShape, version_table: Option<&str>) -> bool {
    matches!(shape.operation, DmlOperation::Delete | DmlOperation::Update)
        && shape.where_clause.is_none()
        && !is_version_bump(shape, version_table)
}

/// The closing `UPDATE VERSIONE_DB SET VERSIONE = '4.13'` of an update script.
///
/// It has no `WHERE` and never will — the table holds one row — and writing it
/// is precisely what `VER002` requires of every update file. Without this
/// exemption, turning `DML001` on for `UPDATE` would put a finding on every
/// correctly written update script in the repository, which is the fastest way
/// to teach somebody that the report is wrong.
fn is_version_bump(shape: &DmlShape, version_table: Option<&str>) -> bool {
    shape.operation == DmlOperation::Update
        && version_table.is_some_and(|table| shape.table.folded_name() == table)
}

fn unguarded_write_finding(anchor: Anchor, shape: &DmlShape) -> Finding {
    let table = shape.table.folded_name();
    let (title, consequence) = match shape.operation {
        DmlOperation::Update => (
            format!("UPDATE on {table} with no WHERE"),
            format!(
                "This {what}, including rows an earlier script or another module put there. \
                 Nothing fails and nothing is logged: the script runs to the end, and the first \
                 sign of it is data that is quietly wrong. Deliberate mass updates are fine — say \
                 so with `-- picus: ignore DML001 — why`.",
                what = rewrites(shape, &table)
            ),
        ),
        _ => (
            format!("DELETE on {table} with no WHERE"),
            format!(
                "This empties {table} completely, including rows an earlier script or another \
                 module put there. On a database that has been in use, whatever is not re-inserted \
                 below is gone. Deliberate reloads are fine — say so with \
                 `-- picus: ignore DML001 — why`.",
            ),
        ),
    };
    Finding::new(RuleId::Dml001, anchor, title, consequence).build()
}

/// What the `UPDATE` does to every row, naming the columns when the parse has
/// them: "sets VALORE" is a sentence somebody can check against their intent,
/// "changes rows" is not.
fn rewrites(shape: &DmlShape, table: &str) -> String {
    let columns: Vec<String> = shape.assignments.iter().map(|a| a.column.folded_name()).collect();
    if columns.is_empty() {
        format!("rewrites every row of {table}")
    } else {
        format!("sets {} on every row of {table}", columns.join(", "))
    }
}

/// An `INSERT` with no column list.
///
/// A `MERGE` whose insert branch has no column list is the same hazard written
/// another way, and `picus-parse` deliberately reports it through the same field
/// so it is not lost to where the grammar happens to put it.
fn column_less_insert(shape: &DmlShape) -> bool {
    if shape.has_column_list {
        return false;
    }
    match shape.operation {
        DmlOperation::Insert => true,
        DmlOperation::Merge => !shape.rows.is_empty(),
        _ => false,
    }
}

fn column_less_insert_finding(anchor: Anchor, shape: &DmlShape) -> Finding {
    let table = shape.table.folded_name();
    Finding::new(
        RuleId::Dml002,
        anchor,
        format!("INSERT into {table} without a column list"),
        format!(
            "The values bind to {table}'s physical column order. Add a column upstream — for either \
             dialect — and every value in this statement shifts one place to the right: the script \
             still runs, and the data lands in the wrong columns.",
        ),
    )
    .fix("Spell out the columns")
    .build()
}
