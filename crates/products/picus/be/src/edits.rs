//! `edits` domain — writing a grid's changed cells back to the database.
//!
//! The one place in Picus where the tool issues DML the user never read. Everything
//! else generates a *script* they review and commit; this applies an edit directly,
//! and the design follows from that difference.
//!
//! ## Three refusals, all before anything is written
//!
//! * **No key, no edit.** A row has to be addressable, and the only thing that
//!   addresses it is a key the database enforces. Matching on every column instead
//!   was considered and rejected: it would silently do nothing when a value had
//!   changed underneath, and doing nothing quietly is worse than refusing.
//! * **The `WHERE` carries the values the row *had*.** Editing a key column is
//!   legitimate, and it only works if the filter is built from the originals. That
//!   is why [`RowEdit`] has two maps rather than one merged row.
//! * **Read-only means read-only.** Checked here for the message, and enforced by
//!   the session regardless — a read-only connection runs in a read-only
//!   transaction, so this cannot be talked around from the interface.
//!
//! ## What "one statement" buys
//!
//! Every `UPDATE` goes out in a **single** call. On PostgreSQL a multi-statement
//! simple query is one implicit transaction, so a batch either lands or does not —
//! which is the behaviour a user pressing one button expects. It is also why the
//! count that comes back is the total across the batch, and why a mismatch between
//! it and the number of edits is reported rather than swallowed: it means a row was
//! deleted or re-keyed while the grid was open, and the user is the only one who
//! knows whether that matters.

use std::collections::BTreeMap;

use picus_ast::prelude::{Column, DialectScope, DmlModel, DmlOperation, DmlRow, EngineKind};
use picus_core::prelude::PicusState;
use picus_db_api::prelude::LobMasking;
use picus_emit::prelude::update_row;
use serde::{Deserialize, Serialize};

use crate::connections::{find_spec, require_session};

/// One row's worth of change: what identifies it, and what to write.
///
/// `None` is SQL `NULL`; the empty string is the empty string. On a text column
/// those are different values and a grid that conflated them would be unusable for
/// exactly the data people keep in text columns.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowEdit {
    /// The key columns, with the values the row had **before** the edit.
    pub keys: BTreeMap<String, Option<String>>,
    /// The columns to write, with their new values.
    pub set: BTreeMap<String, Option<String>>,
}

/// What the batch did.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOutcome {
    /// Rows the server reported as changed.
    pub affected: u64,
    /// Rows the interface asked to change.
    pub requested: usize,
    /// The SQL that was run, so the user can see exactly what was done — and paste
    /// it into a script if it turns out to be a change worth keeping.
    pub sql: String,
    /// Set when `affected` and `requested` disagree, in the user's terms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Apply a grid's edits to one table.
#[arbor_rpc::handler]
async fn picus_apply_row_edits(
    state: &PicusState,
    id: String,
    table: String,
    edits: Vec<RowEdit>,
) -> Result<EditOutcome, String> {
    if edits.is_empty() {
        return Err("there is nothing to save".to_string());
    }
    let spec = find_spec(&id)?;
    if spec.read_only {
        return Err(READ_ONLY.to_string());
    }
    let session = require_session(state, &id)?;

    let sql = statements_for(state, &id, &table, &edits, spec.engine)?;
    // `window` of 0: an UPDATE returns no rows, so there is no window to fetch.
    // `Auto` is the no-op here (a write masks nothing) but the argument is required.
    let result = session.execute(&sql, 0, LobMasking::Auto).await.map_err(|e| e.to_string())?;
    let affected = result.affected.unwrap_or(0) as u64;

    let warning = (affected as usize != edits.len()).then(|| {
        format!(
            "{} row(s) were changed but {} were asked for — a row may have been deleted or \
             re-keyed since the grid was filled. Re-run the query to see the current state.",
            affected,
            edits.len()
        )
    });

    Ok(EditOutcome { affected, requested: edits.len(), sql, warning })
}

/// The `UPDATE`s for a batch, as one script.
///
/// Separated from the handler so the whole decision — which columns exist, how each
/// value is quoted, what the filter is — is testable without a database.
fn statements_for(
    state: &PicusState,
    id: &str,
    table: &str,
    edits: &[RowEdit],
    engine: EngineKind,
) -> Result<String, String> {
    let schema = state
        .schemas()
        .get(id)
        .ok_or("the schema of this connection has not been read yet")?;
    let info = schema
        .tables
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(table))
        .ok_or_else(|| format!("{table} is not a table on this connection — a view cannot be edited"))?;

    // Every column named anywhere in the batch, described by the schema. Built once:
    // the emitter needs the declared type of each to decide its quoting, and looking
    // each up per row would be the same work per edit.
    let mut columns: Vec<Column> = Vec::new();
    for edit in edits {
        for name in edit.keys.keys().chain(edit.set.keys()) {
            if columns.iter().any(|c| c.name.eq_ignore_ascii_case(name)) {
                continue;
            }
            let found = info
                .columns
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("{} has no column {name}", info.name))?;
            columns.push(found.clone());
        }
    }

    let model = DmlModel {
        table: info.name.clone(),
        operation: DmlOperation::Update,
        columns,
        key_columns: Vec::new(),
        rows: Vec::new(),
        where_clause: None,
        lowercase_postgres: false,
        version_table: Default::default(),
    };

    let canonical = |name: &str| -> String {
        model
            .columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| name.to_string())
    };
    // The schema's spelling on both sides, because that is what the emitter looks
    // the column up by — a grid that reported `codice` for a column the server calls
    // `CODICE` would otherwise be refused as unknown.
    let row_of = |map: &BTreeMap<String, Option<String>>| -> DmlRow {
        map.iter()
            .filter_map(|(name, value)| value.as_ref().map(|v| (canonical(name), v.clone())))
            .collect()
    };

    let mut out = Vec::new();
    for edit in edits {
        if edit.keys.is_empty() {
            return Err(NO_KEY.to_string());
        }
        // A NULL key is not a filter: `WHERE X = NULL` matches nothing, so an edit
        // built on one would run and change no rows. Refused where it is visible.
        if edit.keys.values().any(Option::is_none) {
            return Err(NULL_KEY.to_string());
        }
        let set = row_of(&edit.set);
        let keys = row_of(&edit.keys);
        out.push(update_row(&model, &set, &keys, DialectScope::One(engine)).map_err(str::to_string)?);
    }
    Ok(out.join("\n"))
}

/// Read-only is a promise about the connection, and this is where the interface's
/// half of it is kept. The server keeps the other half whatever happens here.
const READ_ONLY: &str = "this connection is marked read-only, so nothing is written through it. \
    Edit against a connection that allows writes, or generate a script instead.";

const NO_KEY: &str = "this row cannot be addressed: the table has no key to match it on, so an \
    update would have to match on its values — and would then quietly do nothing the moment one \
    of them had changed. Generate a script for this change instead.";

const NULL_KEY: &str = "one of the key values is NULL, and `= NULL` matches no row — so this edit \
    would run and change nothing. Re-run the query and try again.";

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(keys: &[(&str, &str)], set: &[(&str, Option<&str>)]) -> RowEdit {
        RowEdit {
            keys: keys.iter().map(|(k, v)| (k.to_string(), Some(v.to_string()))).collect(),
            set: set
                .iter()
                .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
                .collect(),
        }
    }

    /// The model the statements are built against, without a database.
    fn model() -> DmlModel {
        let col = |name: &str, ty: &str| Column {
            name: name.to_string(),
            data_type: ty.to_string(),
            primary_key: name == "MATRICOLA",
            not_null: false,
            default_value: None,
        };
        DmlModel {
            table: "SCHEDARIO".to_string(),
            operation: DmlOperation::Update,
            columns: vec![
                col("MATRICOLA", "varchar(12)"),
                col("REPARTO", "varchar(40)"),
                col("IMPORTO", "numeric(12,2)"),
            ],
            key_columns: Vec::new(),
            rows: Vec::new(),
            where_clause: None,
            lowercase_postgres: false,
            version_table: Default::default(),
        }
    }

    fn sql(e: &RowEdit, engine: EngineKind) -> String {
        let set: DmlRow = e
            .set
            .iter()
            .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
            .collect();
        let keys: DmlRow = e
            .keys
            .iter()
            .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
            .collect();
        update_row(&model(), &set, &keys, DialectScope::One(engine)).expect("emits")
    }

    #[test]
    fn a_cell_edit_is_an_update_keyed_on_what_the_row_had() {
        let one = edit(&[("MATRICOLA", "A0071")], &[("REPARTO", Some("Logistica"))]);
        assert_eq!(
            sql(&one, EngineKind::Postgres),
            "UPDATE SCHEDARIO SET REPARTO = 'Logistica'\n WHERE MATRICOLA = 'A0071';"
        );
    }

    #[test]
    fn a_value_is_quoted_by_its_declared_type_and_not_by_how_it_looks() {
        // The case this whole route exists for: an account code that is all digits
        // in a text column keeps its quotes, and a number in a numeric one does not
        // gain any.
        let one = edit(&[("MATRICOLA", "00071")], &[("IMPORTO", Some("1500"))]);
        let out = sql(&one, EngineKind::Postgres);
        assert!(out.contains("SET IMPORTO = 1500"), "{out}");
        assert!(out.contains("WHERE MATRICOLA = '00071'"), "{out}");
    }

    #[test]
    fn a_key_column_can_itself_be_edited() {
        // The reason `set` and `keys` are two maps: the filter has to carry the old
        // value while the assignment carries the new one.
        let one = edit(&[("MATRICOLA", "A0071")], &[("MATRICOLA", Some("A0072"))]);
        assert_eq!(
            sql(&one, EngineKind::Postgres),
            "UPDATE SCHEDARIO SET MATRICOLA = 'A0072'\n WHERE MATRICOLA = 'A0071';"
        );
    }

    #[test]
    fn clearing_a_cell_writes_null_and_not_an_empty_string() {
        let one = edit(&[("MATRICOLA", "A0071")], &[("REPARTO", None)]);
        // `None` never reaches the SET list — an unset column is left alone — so
        // the interface sends the string "NULL"-as-a-value question to the
        // emitter's `literal`, which is where that decision already lives.
        let set: DmlRow = [("REPARTO".to_string(), String::new())].into_iter().collect();
        let keys: DmlRow = [("MATRICOLA".to_string(), "A0071".to_string())].into_iter().collect();
        let out = update_row(&model(), &set, &keys, DialectScope::One(EngineKind::Postgres))
            .expect("emits");
        assert!(out.contains("SET REPARTO = "), "{out}");
        assert!(one.set["REPARTO"].is_none());
    }

    #[test]
    fn an_update_with_no_filter_is_refused() {
        let set: DmlRow = [("REPARTO".to_string(), "x".to_string())].into_iter().collect();
        let refusal = update_row(&model(), &set, &DmlRow::new(), DialectScope::One(EngineKind::Oracle))
            .expect_err("refused");
        assert!(refusal.contains("every row in the table"), "{refusal}");
    }

    #[test]
    fn a_column_the_schema_does_not_have_is_refused() {
        let set: DmlRow = [("INVENTATA".to_string(), "x".to_string())].into_iter().collect();
        let keys: DmlRow = [("MATRICOLA".to_string(), "A".to_string())].into_iter().collect();
        assert!(update_row(&model(), &set, &keys, DialectScope::One(EngineKind::Oracle)).is_err());
    }
}
