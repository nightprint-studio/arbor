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
    context_at, expand, parse, render, Case, ColumnMeta, CursorContext, ForeignKeyMeta,
    InsertColumn, Operator, RenderStyle, SchemaView, Statement, TableMeta, Value, ValueKind,
};
use picus_ast::prelude::{Column, DialectScope, DmlModel, DmlOperation, DmlRow, EngineKind};
use picus_core::prelude::PicusState;
use picus_db_api::prelude::{SchemaSnapshot, TableInfo};
use picus_emit::prelude::statement_for;
use serde::Serialize;

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
        .map_err(|error| error.to_string())
        .and_then(|statement| sql_for(&statement, &schema, DialectScope::One(dialect)));

    Ok(match expanded {
        Ok(sql) => Expansion::expanded(sql, context),
        Err(reason) => Expansion::refused(reason, context),
    })
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

/// The SQL for a statement, by whichever of the two routes applies.
fn sql_for(
    statement: &Statement,
    schema: &SchemaSnapshot,
    scope: DialectScope,
) -> Result<String, String> {
    match statement {
        Statement::Insert { table, columns, rows } if every_value_given(columns) => {
            emit_insert(table, columns, *rows, schema, scope)
        }
        Statement::Update { table, assignments, predicates } => {
            emit_update(table, assignments, predicates, schema, scope)
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

fn every_value_given(columns: &[InsertColumn]) -> bool {
    !columns.is_empty() && columns.iter().all(|c| c.value.is_some())
}

fn emit_insert(
    table: &str,
    columns: &[InsertColumn],
    rows: usize,
    schema: &SchemaSnapshot,
    scope: DialectScope,
) -> Result<String, String> {
    let named: Vec<(&str, &Value)> = columns
        .iter()
        .filter_map(|c| c.value.as_ref().map(|v| (c.column.name.as_str(), v)))
        .collect();
    let model = model_for(table, DmlOperation::Insert, &named, &[], schema)?;
    let row = row_of(&named);
    // Every row is a copy — see `Statement::Insert`. Emitted `rows` times so the
    // user has the block to edit rather than one line to duplicate by hand.
    let one = statement_for(&model, &row, scope, model.operation).map_err(str::to_string)?;
    Ok(std::iter::repeat_n(one, rows.max(1)).collect::<Vec<_>>().join("\n"))
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
        // `DmlModel` describes rows that exist — a column with no value is one it
        // leaves out — so a skeleton has to take the other route.
        let with = vec![InsertColumn {
            column: arbor_sql_abbrev::prelude::ColumnRef {
                name: "A".into(),
                table: "T".into(),
                alias: None,
                kind: ValueKind::Text,
            },
            value: Some(Value::Quoted("x".into())),
        }];
        let without = vec![InsertColumn { value: None, ..with[0].clone() }];
        assert!(every_value_given(&with));
        assert!(!every_value_given(&without));
        assert!(!every_value_given(&[]), "no columns at all is not a complete row");
    }
}
