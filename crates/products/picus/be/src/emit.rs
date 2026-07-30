//! `emit` domain — deterministic SQL generation.
//!
//! The handlers are thin on purpose: all the logic is in `picus-emit`, where it is
//! testable without a process, and where the golden tests live. What crosses the
//! seam here is a dialect-free model in and one string per target out.
//!
//! This is the backend half of the product requirement that generation involves **no
//! language model anywhere in the flow**. Same input, byte-identical output, every
//! time — which is what makes a generated block reviewable in a diff.

use picus_ast::prelude::{
    Column, DialectScope, DmlModel, DmlOperation, DmlRow, EngineKind, Target,
};
use picus_core::prelude::PicusState;
use picus_emit::prelude::{emit_for_target, insert_rows, validate_value};
use serde::Serialize;

/// One target's generated SQL, plus anything about the target's own rules that
/// would make it wrong.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmittedTarget {
    pub target_id: String,
    pub sql: String,
    /// Why this target's rules can't all apply — e.g. a version guard on a target
    /// that writes bare statements, which has nothing to return from. Reported
    /// rather than silently dropped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_conflict: Option<String>,
}

/// Generate the SQL for every target of one model.
///
/// One call, N results: the point of the product is that a change is described once
/// and lands correctly in each destination, so emitting per target in a loop on the
/// frontend would be re-implementing that guarantee client-side.
#[arbor_rpc::handler]
fn picus_emit(
    _state: &PicusState,
    model: DmlModel,
    targets: Vec<Target>,
) -> Result<Vec<EmittedTarget>, String> {
    Ok(targets
        .iter()
        .map(|t| {
            // The emitter's own refusal and the target's stated conflict are one
            // field on purpose: to the user they are the same sentence — "this
            // destination cannot take this" — and splitting them would put two
            // half-explanations in two places. `Target::refuses` covers both, so
            // the preview and the write cannot disagree about why.
            match emit_for_target(&model, t) {
                Ok(sql) => EmittedTarget {
                    target_id: t.id.clone(),
                    sql,
                    rule_conflict: t.rule_conflict().map(str::to_string),
                },
                Err(refusal) => EmittedTarget {
                    target_id: t.id.clone(),
                    sql: format!("-- nothing generated: {refusal}"),
                    rule_conflict: Some(refusal.to_string()),
                },
            }
        })
        .collect())
}

/// One cell that cannot be written as typed.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueProblem {
    /// Index into the model's `rows`.
    pub row: usize,
    pub column: String,
    /// Why, in the user's terms — never a rule identifier.
    pub reason: String,
}

/// Check every supplied value against its column.
///
/// Batched rather than per keystroke: the whole grid is validated in one round
/// trip, so the caller can mark every offending cell at once instead of discovering
/// them one at a time.
#[arbor_rpc::handler]
fn picus_validate_rows(
    _state: &PicusState,
    model: DmlModel,
) -> Result<Vec<ValueProblem>, String> {
    let mut out = Vec::new();
    for (i, row) in model.rows.iter().enumerate() {
        for column in &model.columns {
            let value = row.get(&column.name).map(String::as_str).unwrap_or("");
            if let Some(reason) = validate_value(value, column) {
                out.push(ValueProblem { row: i, column: column.name.clone(), reason });
            }
        }
    }
    Ok(out)
}

/// Check one value — for the live per-cell feedback the value grid gives while
/// typing, where sending the whole model would be wasteful.
#[arbor_rpc::handler]
fn picus_validate_value(
    _state: &PicusState,
    value: String,
    column: Column,
) -> Result<Option<String>, String> {
    Ok(validate_value(&value, &column))
}

/// Rows out of a result grid, as `INSERT` statements for one connection's engine.
///
/// The point of doing this here rather than joining strings in the interface is
/// **quoting**: whether `007` keeps its quotes and `15` loses them is decided by
/// the column's declared type, and the declared type is something only the
/// connection's schema knows. A frontend that guessed would produce SQL that is
/// right until the first account code with a leading zero.
///
/// The column list is the grid's, and it may be a subset of the table's: a user
/// exporting three columns of a twenty-column table gets an `INSERT` naming three.
/// Columns the schema does not have are refused rather than guessed at — an
/// `INSERT` into a column that does not exist is not a useful thing to hand back.
#[arbor_rpc::handler]
fn picus_rows_to_insert(
    state: &PicusState,
    id: String,
    table: String,
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    dialect: EngineKind,
) -> Result<String, String> {
    let schema = state
        .schemas()
        .get(&id)
        .ok_or("the schema of this connection has not been read yet")?;
    let info = schema
        .tables
        .iter()
        .chain(schema.views.iter())
        .find(|t| t.name.eq_ignore_ascii_case(&table))
        .ok_or_else(|| format!("{table} is not a table on this connection"))?;

    let described: Vec<Column> = columns
        .iter()
        .map(|name| {
            info.columns
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
                .cloned()
                .ok_or_else(|| format!("{table} has no column {name}"))
        })
        .collect::<Result<_, _>>()?;

    let model = DmlModel {
        table: info.name.clone(),
        operation: DmlOperation::Insert,
        columns: described.clone(),
        key_columns: Vec::new(),
        rows: Vec::new(),
        where_clause: None,
        lowercase_postgres: false,
        version_table: Default::default(),
    };

    // A cell the grid reports as NULL is left out of the row, which is how the
    // emitter is told "this one is NULL" — the alternative, the empty string, is a
    // different value and on a text column a perfectly ordinary one.
    let emitted: Vec<DmlRow> = rows
        .iter()
        .map(|row| {
            described
                .iter()
                .zip(row.iter())
                .filter_map(|(column, value)| {
                    value.as_ref().map(|v| (column.name.clone(), v.clone()))
                })
                .collect()
        })
        .collect();

    insert_rows(&model, &emitted, DialectScope::One(dialect)).map_err(str::to_string)
}
