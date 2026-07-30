//! `source relation` — which relation a result's rows came from, and what kind of
//! thing it is.
//!
//! Three features need this answer and none of them can work without it: exporting
//! a result as `INSERT`s, editing a cell in place, and reading a large object one
//! row at a time. All three write or address a *row*, and a row belongs to a table.
//!
//! ## Why the parser, and not a look at the text
//!
//! The frontend used to work it out with a regular expression, and the failure that
//! ended that arrangement is instructive: it counted every `from` in the statement,
//! so `EXTRACT(YEAR FROM data)` — or a subquery in the select list, or
//! `SUBSTRING(codice FROM 1 FOR 3)` — made a plain single-table query report itself
//! as "not from a single table". The user was then told they could not edit rows
//! that were plainly editable, and the `bytea` cells of that query would not open,
//! because opening one needs a key and a key needs a table.
//!
//! Picus already has a SQL parser, and it already records this: every statement it
//! parses carries the objects it *references*, which is where a `FROM` target ends
//! up. Asking it is both correct and less code than approximating it.
//!
//! ## It also answers "is that a view"
//!
//! Which the text cannot say at all — it is a property of the database, not of the
//! statement. Knowing it here is what lets the interface say *"this is a view, and a
//! view has no rows to update"* rather than the previous, misleading *"it is not a
//! table on this connection"*, which is what a caller saw for every view.
//!
//! The wire key is `connectionId`, and `#[handler]` decodes each argument by its own
//! identifier — so the wire contract wins over the naming convention, as it does in
//! [`crate::query`]. Hence the module-wide allow; it is the only reason for it.
#![allow(non_snake_case)]

use picus_core::prelude::PicusState;
use picus_db_api::prelude::RelationKind;
use picus_parse::prelude::{parse, DialectScope, EngineKind, ObjectKind, StatementKind};
use serde::Serialize;

use crate::connections::find_spec;

/// What one result's rows can be traced back to.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRelation {
    /// The relation's name, unqualified and as the catalogue spells it. Empty when
    /// the statement does not read from exactly one.
    pub relation: String,
    /// `true` when the connection's catalogue calls it a view. A view is a source
    /// with no rows of its own, so nothing can be written back through it.
    pub is_view: bool,
    /// `true` when the catalogue has never heard of it — a CTE, a temporary table,
    /// or a schema that has not been read yet.
    pub unknown: bool,
    /// Why there is no single relation, in the user's terms. Empty when there is
    /// one. The interface shows this verbatim, so it is written to be read.
    pub reason: String,
}

fn refused(reason: &str) -> SourceRelation {
    SourceRelation { reason: reason.to_string(), ..Default::default() }
}

/// The single relation a statement reads from, and what the catalogue calls it.
///
/// Deliberately strict, and it says no with a sentence rather than an empty string:
/// the caller disables a feature on the strength of this, and "you cannot edit
/// these rows" without a reason is the kind of refusal people file bugs about.
#[arbor_rpc::handler]
fn picus_source_relation(
    state: &PicusState,
    connectionId: String,
    sql: String,
) -> Result<SourceRelation, String> {
    let scope = find_spec(&connectionId)
        .map(|spec| DialectScope::One(spec.engine))
        .unwrap_or(DialectScope::One(EngineKind::Postgres));

    let parsed = parse(&sql, scope);
    let mut statements = parsed.statements.iter();
    let Some(statement) = statements.next() else {
        return Ok(refused("There is no statement here to trace back to a table."));
    };
    if statements.next().is_some() {
        return Ok(refused(
            "This tab ran more than one statement, so these rows have no single source.",
        ));
    }
    if statement.kind != StatementKind::Select {
        return Ok(refused("These rows did not come from a SELECT."));
    }

    // `references` is every object the statement names; a `FROM` target is one of
    // them. Columns and the rest are filtered out, and the relation kinds are taken
    // together because the parser reads a name, not a catalogue — what it calls a
    // table may well be a view.
    let mut sources: Vec<String> = statement
        .references
        .iter()
        .filter(|r| {
            matches!(r.kind, ObjectKind::Table | ObjectKind::View | ObjectKind::MaterializedView)
        })
        .map(|r| r.folded_name())
        .collect();
    sources.dedup();

    let [relation] = sources.as_slice() else {
        return Ok(refused(if sources.is_empty() {
            "These rows are not from a table — a computed result has no row to update."
        } else {
            "These rows are from more than one table, so there is no single row to update. \
             A join can be exported, but not edited."
        }));
    };

    // The catalogue decides what it *is*. Nothing in the text can.
    let Some(schema) = state.schemas().get(&connectionId) else {
        return Ok(SourceRelation {
            relation: relation.clone(),
            unknown: true,
            reason: "This connection's schema has not been read yet.".to_string(),
            ..Default::default()
        });
    };
    let found = schema
        .tables
        .iter()
        .chain(schema.views.iter())
        .find(|t| t.name.eq_ignore_ascii_case(relation));

    Ok(match found {
        Some(info) if info.kind == RelationKind::View => SourceRelation {
            relation: relation.clone(),
            is_view: true,
            reason: format!("{relation} is a view, and a view has no rows of its own to update."),
            ..Default::default()
        },
        // The **catalogue's** spelling, not the parser's folded one: the caller
        // looks the relation up in its own copy of the schema, and a name that has
        // been case-folded on the way past would miss a mixed-case table.
        Some(info) => SourceRelation { relation: info.name.clone(), ..Default::default() },
        None => SourceRelation {
            relation: relation.clone(),
            unknown: true,
            reason: format!(
                "{relation} is not a relation on this connection — a CTE or a temporary table \
                 has no rows to write back to."
            ),
            ..Default::default()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The relations a statement reads, as the handler derives them — without the
    /// catalogue half, which needs a connection.
    fn sources_of(sql: &str) -> Vec<String> {
        let parsed = parse(sql, DialectScope::One(EngineKind::Postgres));
        let Some(statement) = parsed.statements.first() else { return vec![] };
        let mut out: Vec<String> = statement
            .references
            .iter()
            .filter(|r| {
                matches!(r.kind, ObjectKind::Table | ObjectKind::View | ObjectKind::MaterializedView)
            })
            .map(|r| r.folded_name())
            .collect();
        out.dedup();
        out
    }

    #[test]
    fn a_plain_read_names_its_table() {
        assert_eq!(sources_of("SELECT * FROM archivio"), vec!["ARCHIVIO".to_string()]);
        assert_eq!(sources_of("select * from ARCHIVIO where stato = 'EV'"), vec!["ARCHIVIO".to_string()]);
    }

    #[test]
    fn a_from_that_is_not_a_clause_does_not_count_as_a_source() {
        // THE regression this module exists for. Every one of these reads from one
        // table, and the regular expression this replaced refused all of them
        // because it counted the word `from` wherever it appeared.
        for sql in [
            "SELECT EXTRACT(YEAR FROM data_ordine) FROM archivio",
            "SELECT SUBSTRING(codice FROM 1 FOR 3) FROM archivio",
            "SELECT TRIM(BOTH ' ' FROM nota) FROM archivio",
            "SELECT * FROM archivio WHERE nota = 'FROM somewhere else'",
            "SELECT * FROM archivio -- select * from altrove\n",
        ] {
            assert_eq!(sources_of(sql), vec!["ARCHIVIO".to_string()], "{sql}");
        }
    }

    #[test]
    fn a_subquery_in_the_projection_is_still_one_source() {
        assert_eq!(
            sources_of("SELECT (SELECT 1) AS uno, protocollo FROM archivio"),
            vec!["ARCHIVIO".to_string()],
        );
    }

    #[test]
    fn more_than_one_source_is_more_than_one() {
        for sql in [
            "SELECT * FROM archivio JOIN protocolli USING (protocollo)",
            "SELECT * FROM archivio, protocolli",
            "SELECT * FROM archivio LEFT JOIN protocolli ON true",
        ] {
            assert!(sources_of(sql).len() > 1, "{sql} — {:?}", sources_of(sql));
        }
    }

    /// The handler's own answer, for the half that needs no catalogue: which
    /// relation, or which refusal.
    fn traced(sql: &str) -> Result<String, String> {
        let parsed = parse(sql, DialectScope::One(EngineKind::Postgres));
        let mut statements = parsed.statements.iter();
        let Some(statement) = statements.next() else { return Err("no statement".into()) };
        if statements.next().is_some() {
            return Err("more than one statement".into());
        }
        if statement.kind != StatementKind::Select {
            return Err("not a select".into());
        }
        match sources_of(sql).as_slice() {
            [one] => Ok(one.clone()),
            [] => Err("no source".into()),
            _ => Err("more than one source".into()),
        }
    }

    #[test]
    fn a_scratchpad_is_traced_by_the_statement_that_ran_not_by_the_tab() {
        // THE bug. A query tab holds several statements — that is what a scratchpad
        // is for — and the old code asked "which table is this?" of the **whole
        // buffer**, which reads from all of them. A perfectly ordinary single-table
        // query was therefore reported as a join, its rows refused editing, and its
        // `bytea` cells refused to open.
        //
        // The buffer, asked as a whole, is more than one statement:
        let buffer = "SELECT * FROM protocolli;\n\nSELECT * FROM archivio WHERE stato = 'EV';\n";
        assert!(traced(buffer).is_err(), "the buffer is not one statement");

        // …and each statement in it, asked on its own, names exactly one table.
        assert_eq!(traced("SELECT * FROM protocolli").as_deref(), Ok("PROTOCOLLI"));
        assert_eq!(
            traced("SELECT * FROM archivio WHERE stato = 'EV'").as_deref(),
            Ok("ARCHIVIO"),
        );
    }

    #[test]
    fn a_trailing_terminator_or_a_comment_does_not_make_a_second_statement() {
        // A statement pasted with its `;`, or with a note above it, is still one
        // statement — and the caller sends exactly what it ran, terminator included.
        assert_eq!(traced("SELECT * FROM archivio;").as_deref(), Ok("ARCHIVIO"));
        assert_eq!(traced("-- gli evasi\nSELECT * FROM archivio;").as_deref(), Ok("ARCHIVIO"));
        assert_eq!(traced("SELECT * FROM archivio; -- fine\n").as_deref(), Ok("ARCHIVIO"));
    }

    #[test]
    fn the_shapes_that_have_no_single_row_to_write_back_to() {
        for sql in [
            "SELECT * FROM archivio JOIN protocolli USING (protocollo)",
            "SELECT * FROM archivio UNION SELECT * FROM storico",
            "SELECT count(*) FROM archivio, protocolli",
            "SELECT 1",
        ] {
            assert!(traced(sql).is_err(), "{sql} should have no single source");
        }
    }

    #[test]
    fn a_write_is_not_a_source_to_edit_rows_of() {
        assert!(traced("UPDATE archivio SET stato = 'EV'").is_err());
        assert!(traced("INSERT INTO archivio (protocollo) VALUES (1)").is_err());
    }

    #[test]
    fn a_function_named_like_a_join_word_is_not_a_join() {
        // `left(…)` is an ordinary string function, and a scan that treated the word
        // as a join keyword would refuse a perfectly editable result.
        assert_eq!(
            sources_of("SELECT left(etichetta, 3) FROM archivio"),
            vec!["ARCHIVIO".to_string()],
        );
    }
}
