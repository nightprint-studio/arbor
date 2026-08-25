//! `abbrev` domain — the SQL abbreviation expander, wired to a live connection.
//!
//! `s#localstrings(keycode,value)[keycode='ita']` becomes
//! `SELECT KEYCODE, VALUE FROM LOCALSTRINGS WHERE KEYCODE = 'ita'`. The language
//! itself is [`arbor_sql_abbrev`], which knows nothing about Picus and lives in
//! `foundation/` so a second product can take it. This module is the three things
//! only Picus can supply: **the schema**, **the dialect**, and **the emitter**.
//!
//! ## Why the schema is the whole point
//!
//! A text expander that turned `s#t(a)` into `SELECT a FROM t` would be a snippet,
//! and every editor already has snippets. What is worth building is the expansion
//! that could only come from a tool holding a live connection: the column's type
//! decides whether its value is quoted, and the **foreign key** decides what a join
//! is `ON`. Both come out of the schema this connection reported, held in
//! [`picus_core::prelude::SchemaCache`] because this is asked on every keystroke.
//!
//! ## Two ways out, and the rule for choosing
//!
//! `INSERT` and `UPDATE` with a complete set of values go through Picus's own
//! [`picus_emit`] — the same deterministic path the generator uses — so quoting,
//! identifier casing and the Oracle/PostgreSQL differences are decided in exactly
//! one place in this product. Everything else is rendered by the crate's own
//! renderer.
//!
//! The rule is not arbitrary. `DmlModel` describes *rows that exist*: a column with
//! no value is one the statement leaves out, which is right when generating from a
//! grid and wrong when the user asked for a skeleton. `i#t(a,b)` wants
//! `INSERT INTO T (A, B) VALUES (?, ?)` — and a skeleton has no literals in it, so
//! there is nothing dialect-specific for the emitter to get right anyway. The
//! moment a real value appears, the emitter takes over.

use arbor_sql_abbrev::prelude::{
    context_at, expand, parse, render, Case, ColumnMeta, ColumnRef, CursorContext, ForeignKeyMeta,
    InsertRow, Operator, RenderStyle, SchemaView, Statement, TableMeta, Value, ValueKind,
};
use picus_ast::prelude::{Column, DialectScope, DmlModel, DmlOperation, DmlRow, EngineKind};
use picus_core::prelude::PicusState;
use picus_db_api::prelude::{SchemaSnapshot, TableInfo};
use picus_emit::prelude::{insert_rows, statement_for};
use serde::Serialize;

use crate::abbrev_render::{alter_sql, for_cursor_sql, merge_sql};

/// What the editor gets back for the line it is on.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Expansion {
    /// Does this text even look like an abbreviation?
    ///
    /// The first thing the editor asks, because the answer decides whether it
    /// shows anything at all. Ordinary SQL is not an abbreviation and must not
    /// light up as a broken one — the editor is full of ordinary SQL.
    pub is_abbreviation: bool,
    /// The SQL, when it expanded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    /// Why it did not, in the user's words. A refusal is never silent: the whole
    /// design of the language is to refuse rather than guess, which only works if
    /// the refusal is visible while typing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// What is under the caret, so completion can offer tables here and columns
    /// there. From the **same parse** as the expansion.
    pub context: CursorContext,
}

impl Expansion {
    /// Not an abbreviation — which is what most of a SQL buffer is.
    fn plain(context: CursorContext) -> Expansion {
        Expansion { is_abbreviation: false, sql: None, error: None, context }
    }

    fn expanded(sql: String, context: CursorContext) -> Expansion {
        Expansion { is_abbreviation: true, sql: Some(sql), error: None, context }
    }

    fn refused(reason: String, context: CursorContext) -> Expansion {
        Expansion { is_abbreviation: true, sql: None, error: Some(reason), context }
    }
}

/// Expand what the user is typing, and say what is under the caret.
///
/// One verb for both because they are one parse: asking twice would mean two
/// parsers or two round trips, and the two answers have to agree about where the
/// caret is or the completion offers columns of a table the expansion does not
/// think is there.
///
/// Answers rather than fails when the text is not an abbreviation: this is called
/// on every keystroke in a SQL editor, and the common case is that the user is
/// writing SQL.
///
/// `dialect` is the connection's engine, and it comes from the caller rather than
/// being looked up here on purpose: a session carries no engine and the connection
/// file is on disk, so answering it in the backend would mean a file read on every
/// keystroke to learn something the interface already has on screen. It decides
/// nothing but spelling — a wrong one produces visibly wrong SQL in a preview,
/// never a wrong write.
#[arbor_rpc::handler]
fn picus_expand_sql(
    state: &PicusState,
    id: String,
    input: String,
    cursor: usize,
    dialect: EngineKind,
) -> Result<Expansion, String> {
    let context = context_at(&input, cursor);
    if !looks_like_abbreviation(&input) {
        return Ok(Expansion::plain(context));
    }

    let Some(schema) = state.schemas().get(&id) else {
        return Ok(Expansion::refused(
            "the schema of this connection has not been read yet — open the object tree once"
                .to_string(),
            context,
        ));
    };

    let view = view_of(&schema);
    let expanded = expand(&input, &view)
        .map_err(|error| refusal_text(&error, &view))
        .and_then(|statement| sql_for(&statement, &schema, DialectScope::One(dialect)));

    Ok(match expanded {
        Ok(sql) => Expansion::expanded(sql, context),
        Err(reason) => Expansion::refused(reason, context),
    })
}

/// The language's refusal, plus the one fact only the host can supply.
///
/// "the schema has no table called `TORN`" is a complete sentence and still leaves the reader
/// stuck, because the question it raises is *which* catalogue was consulted. A connection reads
/// **one** schema, and a relation outside it — another schema, or one the catalogue read skips —
/// is simply not in what the abbreviation can see. That is indistinguishable, from the editor, from
/// the feature being broken: the preview does not appear and neither does a reason the user can act
/// on. So the refusal says how much was searched.
///
/// Only for an unknown table with **no near-miss to offer**. When the language has a suggestion,
/// that is the actionable answer and this would bury it; and every other refusal is about the
/// abbreviation itself, where the size of the catalogue is noise.
fn refusal_text(error: &arbor_sql_abbrev::prelude::AbbrevError, view: &SchemaView) -> String {
    use arbor_sql_abbrev::prelude::AbbrevError;
    let said = error.to_string();
    match error {
        AbbrevError::UnknownTable { suggestion: None, .. } => format!(
            "{said}. This connection's catalogue holds {} relation(s), all from the one schema the \
             connection is pinned to — a table in another schema is not in it. Re-read the object \
             tree if the table is newer than the catalogue.",
            view.tables.len()
        ),
        _ => said,
    }
}

/// Is this text an abbreviation at all?
///
/// A verb followed by `#`, which is the shape nothing valid in SQL has. Asked of
/// the crate's own tolerant parser rather than by looking for a `#`, so the answer
/// agrees with what `expand` will do — and deliberately **before** the schema is
/// consulted, so a connection with no schema read yet still leaves ordinary SQL
/// alone.
fn looks_like_abbreviation(input: &str) -> bool {
    let parsed = parse(input);
    parsed.hash.is_some() && !parsed.verb.text.trim().is_empty()
}

// ── The schema, in the language's own terms ──────────────────────────────────

/// Map what the connection reported into what the language reads.
///
/// Rebuilt per call rather than cached alongside the snapshot: it is a few
/// thousand small allocations for a large schema, which is nothing beside the
/// round trip this call replaced, and a second cache would be a second thing that
/// can disagree with the first about what the database contains.
fn view_of(schema: &SchemaSnapshot) -> SchemaView {
    SchemaView {
        tables: schema.tables.iter().chain(schema.views.iter()).map(table_of).collect(),
    }
}

fn table_of(table: &TableInfo) -> TableMeta {
    TableMeta {
        name: table.name.clone(),
        columns: table
            .columns
            .iter()
            .map(|c| ColumnMeta { name: c.name.clone(), kind: kind_of(&c.data_type) })
            .collect(),
        foreign_keys: table
            .foreign_keys
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|fk| ForeignKeyMeta {
                columns: fk.columns.clone(),
                referenced_table: fk.referenced_table.clone(),
                referenced_columns: fk.referenced_columns.clone(),
            })
            .collect(),
    }
}

/// A database's own type name, reduced to the one question the language asks:
/// does a value in this column need quotes?
///
/// Deliberately coarse and deliberately substring-matched. The alternative is an
/// exhaustive table of every type name two engines can produce, which would be
/// wrong the first time somebody used a domain type — and being wrong here costs
/// only a pair of quotes in a preview the user is looking at, not a written file.
fn kind_of(data_type: &str) -> ValueKind {
    let t = data_type.to_ascii_lowercase();
    let has = |needle: &str| t.contains(needle);
    if has("bool") {
        ValueKind::Boolean
    } else if has("int") || has("numeric") || has("decimal") || has("number") || has("float")
        || has("double") || has("real") || has("money")
    {
        ValueKind::Number
    } else if has("timestamp") || has("date") || has("time") {
        ValueKind::Date
    } else if has("char") || has("text") || has("clob") || has("string") {
        ValueKind::Text
    } else {
        ValueKind::Other
    }
}

// ── Rendering ────────────────────────────────────────────────────────────────

/// The SQL for a statement, by whichever route applies.
///
/// Three of them now, and the rule for choosing has not changed: **anything with
/// literals in it goes through the emitter**, because that is where quoting and the
/// two dialects are decided; anything that is a skeleton is rendered as text. What
/// `m#`, `a#` and `fc#` add is a third case — skeletons that the two engines
/// nonetheless *spell* differently — and those go through [`crate::abbrev_render`],
/// which is the same seam under a different name.
fn sql_for(
    statement: &Statement,
    schema: &SchemaSnapshot,
    scope: DialectScope,
) -> Result<String, String> {
    match statement {
        Statement::Insert { table, columns, rows } if every_value_given(rows) => {
            emit_insert(table, columns, rows, schema, scope)
        }
        Statement::Update { table, assignments, predicates } => {
            emit_update(table, assignments, predicates, schema, scope)
        }
        Statement::Merge { table, columns, keys } => merge_sql(table, columns, keys, scope),
        Statement::Alter { table, changes } => alter_sql(table, changes, scope),
        Statement::ForCursor { variable, query } => {
            for_cursor_sql(variable, query, &style(), scope)
        }
        other => Ok(render(other, &style())),
    }
}

/// How the crate's own renderer writes for Picus: keywords in upper case,
/// identifiers exactly as the schema spells them.
///
/// Identifiers are left alone on purpose. `expand` has already replaced whatever
/// the user typed with the schema's own spelling, so the database's answer about
/// its own names is the last word — re-casing it here would be this layer
/// overruling the server.
fn style() -> RenderStyle {
    RenderStyle { keywords: Case::Upper, identifiers: Case::AsIs, ..RenderStyle::default() }
}

/// Is every cell of every row filled in?
///
/// The question that decides the route: a skeleton (`i#t(a,b)`) has nothing for
/// the emitter to quote and goes out as text, and the first real value sends the
/// whole thing through `picus-emit`.
fn every_value_given(rows: &[InsertRow]) -> bool {
    !rows.is_empty()
        && rows.iter().all(|row| !row.is_empty() && row.iter().all(Option::is_some))
}

fn emit_insert(
    table: &str,
    columns: &[ColumnRef],
    rows: &[InsertRow],
    schema: &SchemaSnapshot,
    scope: DialectScope,
) -> Result<String, String> {
    let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
    // The model describes the shape, which every row shares; the rows carry the
    // values. Built from the first row only because the column list is the same
    // for all of them by construction.
    let first: Vec<(&str, &Value)> = pairs(&names, &rows[0]);
    let model = model_for(table, DmlOperation::Insert, &first, &[], schema)?;
    let emitted: Vec<DmlRow> = rows.iter().map(|row| row_of(&pairs(&names, row))).collect();
    // `insert_rows` owns the one dialect fact here — Oracle has no multi-row
    // `VALUES`, so `*3` is one statement there and three here.
    insert_rows(&model, &emitted, scope).map_err(str::to_string)
}

/// Column names paired with the row's values, skipping the cells with none.
fn pairs<'a>(names: &[&'a str], row: &'a InsertRow) -> Vec<(&'a str, &'a Value)> {
    names
        .iter()
        .zip(row.iter())
        .filter_map(|(name, value)| value.as_ref().map(|v| (*name, v)))
        .collect()
}

fn emit_update(
    table: &str,
    assignments: &[arbor_sql_abbrev::prelude::Assignment],
    predicates: &[arbor_sql_abbrev::prelude::Predicate],
    schema: &SchemaSnapshot,
    scope: DialectScope,
) -> Result<String, String> {
    // The one place Picus's model is narrower than the language, and the refusal
    // belongs here rather than in the grammar: `DmlModel` keys an update by
    // equality, which is a fact about this product's generator and not about SQL.
    if let Some(bad) = predicates.iter().find(|p| !matches!(p.op, Operator::Eq)) {
        return Err(format!(
            "Picus writes an UPDATE keyed by equality, and `{}` is compared with `{}`. \
             Use `=` here, or write the statement out.",
            bad.column.name,
            bad.op.sql()
        ));
    }
    let set: Vec<(&str, &Value)> =
        assignments.iter().map(|a| (a.column.name.as_str(), &a.value)).collect();
    let keys: Vec<(&str, &Value)> =
        predicates.iter().map(|p| (p.column.name.as_str(), &p.value)).collect();

    let model = model_for(table, DmlOperation::Update, &set, &keys, schema)?;
    let mut row = row_of(&set);
    row.extend(row_of(&keys));
    statement_for(&model, &row, scope, model.operation).map_err(str::to_string)
}

/// Build the generator's model for one abbreviation.
///
/// The column metadata comes from the **schema**, not from the abbreviation: the
/// emitter decides quoting from `data_type`, so handing it anything less than what
/// the server said would put the decision back where this feature exists to take
/// it from.
fn model_for(
    table: &str,
    operation: DmlOperation,
    supplied: &[(&str, &Value)],
    keys: &[(&str, &Value)],
    schema: &SchemaSnapshot,
) -> Result<DmlModel, String> {
    let info = schema
        .tables
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(table))
        .ok_or_else(|| format!("{table} is not a table on this connection"))?;

    let column = |name: &str| -> Result<Column, String> {
        info.columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .cloned()
            .ok_or_else(|| format!("{table} has no column {name}"))
    };

    Ok(DmlModel {
        table: info.name.clone(),
        operation,
        columns: supplied
            .iter()
            .chain(keys.iter())
            .map(|(name, _)| column(name))
            .collect::<Result<Vec<_>, _>>()?,
        key_columns: keys.iter().map(|(name, _)| column(name)).collect::<Result<Vec<_>, _>>()?,
        rows: Vec::new(),
        where_clause: None,
        lowercase_postgres: false,
        version_table: Default::default(),
    })
}

/// The values, keyed by column, in the shape the emitter reads.
///
/// A value the user quoted stays quoted whatever the column says — the crate
/// already folded that decision into `Value`, and re-deciding it here would be the
/// second implementation of the one rule this feature is about.
fn row_of(values: &[(&str, &Value)]) -> DmlRow {
    values
        .iter()
        .map(|(name, value)| (name.to_string(), value.text().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_name_reduces_to_the_one_question_that_matters() {
        assert_eq!(kind_of("integer"), ValueKind::Number);
        assert_eq!(kind_of("NUMBER(10,2)"), ValueKind::Number);
        assert_eq!(kind_of("character varying(30)"), ValueKind::Text);
        assert_eq!(kind_of("VARCHAR2(30)"), ValueKind::Text);
        assert_eq!(kind_of("timestamp with time zone"), ValueKind::Date);
        assert_eq!(kind_of("boolean"), ValueKind::Boolean);
        // Anything unrecognised is quoted, which is the safe direction: a quoted
        // number is a visible mistake in a preview, an unquoted string is a
        // syntax error the user has to work out.
        assert_eq!(kind_of("geometry"), ValueKind::Other);
    }

    #[test]
    fn an_unknown_table_says_how_much_was_searched() {
        use arbor_sql_abbrev::prelude::AbbrevError;
        let view = SchemaView::new(vec![TableMeta::new("ORDINI", Vec::new())]);
        let said = refusal_text(
            &AbbrevError::UnknownTable { name: "TORN".into(), suggestion: None },
            &view,
        );
        assert!(said.contains("TORN"), "it still says what was not found: {said}");
        assert!(said.contains("1 relation"), "…and how much there was to find it in: {said}");
        assert!(said.contains("another schema"), "…and the reason it is most often missing");

        // A near miss IS the answer — the catalogue's size would only bury it.
        let near = refusal_text(
            &AbbrevError::UnknownTable { name: "ORDNI".into(), suggestion: Some("ORDINI".into()) },
            &view,
        );
        assert!(near.contains("ORDINI") && !near.contains("catalogue"), "{near}");

        // Every other refusal is about the abbreviation, where the catalogue's size says nothing.
        let other = refusal_text(&AbbrevError::MissingValue { column: "ID".into() }, &view);
        assert!(!other.contains("catalogue"), "{other}");
    }

    #[test]
    fn ordinary_sql_is_not_an_abbreviation() {
        // The property the whole editor integration rests on: this runs on every
        // keystroke in a buffer full of real SQL, and must stay silent there.
        for sql in [
            "select * from localstrings",
            "SELECT keycode FROM t WHERE a = 1",
            "-- picus: ignore DML001",
            "",
            "   ",
            "insert into t values (1)",
        ] {
            assert!(!looks_like_abbreviation(sql), "{sql:?}");
        }
    }

    #[test]
    fn an_abbreviation_is_recognised_while_it_is_still_being_typed() {
        // Half-typed input has to read as an abbreviation, or the preview only
        // appears once the line is already finished — which is when it is useless.
        for input in ["s#loc", "s#localstrings(keycode", "i#t(a='x')", "u#t(a=1)[b=2]", "d#t[a=1]"] {
            assert!(looks_like_abbreviation(input), "{input:?}");
        }
        // …but a verb with no `#` is just a word someone is typing.
        assert!(!looks_like_abbreviation("s"));
        assert!(!looks_like_abbreviation("select"));
    }

    #[test]
    fn an_insert_with_no_values_is_not_sent_through_the_generators_model() {
        // `DmlModel` describes rows that exist — a cell with no value is one it
        // leaves out — so a skeleton has to take the other route.
        let filled: InsertRow = vec![Some(Value::Quoted("x".into()))];
        let empty: InsertRow = vec![None];
        assert!(every_value_given(&[filled.clone()]));
        assert!(!every_value_given(&[empty.clone()]));
        // One good row does not redeem a bad one: a `{…}` template with a gap in
        // it would otherwise emit some rows through the emitter and lose the rest.
        assert!(!every_value_given(&[filled, empty]));
        assert!(!every_value_given(&[]), "no rows at all is not a complete row");
        assert!(!every_value_given(&[vec![]]), "a row with no cells is not one either");
    }
}
