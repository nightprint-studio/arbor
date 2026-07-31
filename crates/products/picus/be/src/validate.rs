//! `validate` domain — live SQL validation against the connected database.
//!
//! The editor used to reimplement the catalogue in the frontend to say "no such
//! table", "no such column", "ambiguous column" — a pile of heuristics that had to
//! stay quiet whenever they were unsure, and were wrong whenever the schema had
//! moved. The database already knows all of this, exactly, so we ask it: each
//! statement is **prepared** (parsed and described) but never run, and whatever the
//! server rejects comes back as a squiggle where the server says the problem is.
//!
//! ## One statement at a time
//!
//! A prepare describes a single command, but a query tab is a scratchpad of several.
//! So the buffer is parsed here, and each statement that *can* be prepared is checked
//! on its own — DDL, blocks, `SET`, transaction control cannot be prepared and are
//! skipped, exactly as the old frontend checks skipped them. A statement the parser
//! already found a syntax error in is skipped too: tree-sitter squiggles that, and a
//! prepare would only repeat it in worse words.
//!
//! ## Positions
//!
//! PostgreSQL reports a 1-based **character** offset into the statement it rejected.
//! It is turned into an absolute **byte** range in the buffer — the coordinate the
//! editor's lint layer speaks — by finding the character, widening to the token that
//! starts there, and rebasing onto the statement's own offset. Both conversions are
//! pure and tested, because a bug in either silently shifts every squiggle in a file
//! with an accented character in it.
//!
//! The wire key is `connectionId`; `#[handler]` decodes each argument by its own
//! identifier, so the wire contract wins over the naming convention. Hence the
//! module-wide allow.
#![allow(non_snake_case)]

use picus_core::prelude::PicusState;
use picus_db_api::prelude::DbError;
use picus_parse::prelude::{parse, DialectScope, EngineKind, StatementKind};
use serde::Serialize;

use crate::connections::{find_spec, require_session};

/// One thing the server rejected, placed in the buffer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationFinding {
    /// Absolute UTF-8 byte offset of the start of the squiggle in the buffer.
    pub start: u32,
    /// Absolute UTF-8 byte offset of the end.
    pub end: u32,
    /// The server's own message.
    pub message: String,
    /// The SQLSTATE, when the server gave one.
    pub code: Option<String>,
}

/// Prepare every preparable statement in the buffer and report what the server
/// rejects.
///
/// Returns an **empty** list — never an error — when there is nothing to validate
/// against (no open session, an engine without the capability): a validation that
/// could not be performed must read as "no findings", never as a red buffer. A
/// *connection-level* failure mid-run is different, and is surfaced, so the frontend
/// can show "unavailable" rather than a stale green tick.
#[arbor_rpc::handler]
async fn picus_validate(
    state: &PicusState,
    id: String,
    sql: String,
) -> Result<Vec<ValidationFinding>, String> {
    if sql.trim().is_empty() {
        return Ok(Vec::new());
    }
    let scope = find_spec(&id)
        .map(|spec| DialectScope::One(spec.engine))
        .unwrap_or(DialectScope::One(EngineKind::Postgres));
    let Ok(session) = require_session(state, &id) else {
        return Ok(Vec::new());
    };

    let parsed = parse(&sql, scope);
    let mut findings = Vec::new();
    for statement in &parsed.statements {
        if !preparable(statement.kind) || statement.has_error {
            continue;
        }
        let text = &sql[statement.range.start..statement.range.end];
        match session.validate(text).await {
            Ok(()) => {}
            Err(DbError::Sql { message, code, position }) => {
                let (start, end) = locate(statement.range.start, text, position);
                findings.push(ValidationFinding {
                    start: start as u32,
                    end: end as u32,
                    message,
                    code,
                });
            }
            // The engine cannot validate at all — the capability was misread, or a
            // second engine has no prepare. Nothing to report, and nothing wrong.
            Err(DbError::Unsupported { .. }) => return Ok(Vec::new()),
            // The connection itself is gone/refusing — surface it so the editor shows
            // "unavailable" rather than squiggling valid SQL.
            Err(
                e @ (DbError::Disconnected(_)
                | DbError::Connect(_)
                | DbError::Cancelled
                | DbError::SecretMissing
                | DbError::NoDriver { .. }),
            ) => return Err(e.to_string()),
            // Any other rejection without a position: mark the whole statement.
            Err(other) => findings.push(ValidationFinding {
                start: statement.range.start as u32,
                end: statement.range.end as u32,
                message: other.to_string(),
                code: None,
            }),
        }
    }
    Ok(findings)
}

/// Statements a server can `PREPARE`. DDL, anonymous blocks, `SET`, transaction
/// control and the rest cannot be prepared, so they are left to the syntax parser —
/// the same statements the old frontend checks stood down on.
fn preparable(kind: StatementKind) -> bool {
    matches!(
        kind,
        StatementKind::Select
            | StatementKind::Insert
            | StatementKind::Update
            | StatementKind::Delete
            | StatementKind::Merge
    )
}

/// The buffer range to squiggle for a rejection at `position` (1-based character
/// offset into `stmt_text`, or `None`), given where the statement starts in the
/// buffer. `None` underlines the whole statement.
fn locate(stmt_start: usize, stmt_text: &str, position: Option<u32>) -> (usize, usize) {
    let Some(position) = position else {
        return (stmt_start, stmt_start + stmt_text.len());
    };
    let char_index = position.max(1) as usize - 1;
    let byte = stmt_text
        .char_indices()
        .nth(char_index)
        .map(|(b, _)| b)
        .unwrap_or(stmt_text.len());
    let (start, end) = widen_to_token(stmt_text, byte);
    (stmt_start + start, stmt_start + end)
}

/// Widen a byte offset to the identifier/token that starts there, so the squiggle
/// covers the whole `cliente` rather than the `c` the server pointed at. Falls back
/// to the single character at the offset when it is not on a token (an operator, a
/// comma). Byte offsets in, byte offsets out; token bytes include non-ASCII so an
/// accented or quoted identifier is not cut in half.
fn widen_to_token(stmt: &str, byte: usize) -> (usize, usize) {
    let bytes = stmt.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return (0, 0);
    }
    let is_token = |b: u8| {
        b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'"' | b'.') || b >= 0x80
    };
    let at = byte.min(len - 1);
    if !is_token(bytes[at]) {
        return (at, next_boundary(stmt, at));
    }
    let mut start = at;
    while start > 0 && is_token(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = at + 1;
    while end < len && is_token(bytes[end]) {
        end += 1;
    }
    (start, end)
}

/// The next char boundary after `at`, so a single-character underline never splits a
/// multibyte character.
fn next_boundary(s: &str, at: usize) -> usize {
    let mut end = (at + 1).min(s.len());
    while end < s.len() && !s.is_char_boundary(end) {
        end += 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_widens_to_the_whole_token() {
        // "SELECT * FROM clientee" — the server points at the 'c' of clientee (col 15).
        let stmt = "SELECT * FROM clientee";
        let at = stmt.find("clientee").unwrap();
        let (start, end) = locate(0, stmt, Some(at as u32 + 1));
        assert_eq!(&stmt[start..end], "clientee");
    }

    #[test]
    fn the_statement_offset_is_added_back() {
        let buffer = "SELECT 1;\nSELECT * FROM nope";
        let stmt_start = buffer.find("SELECT * FROM nope").unwrap();
        let stmt = &buffer[stmt_start..];
        let at_in_stmt = stmt.find("nope").unwrap();
        let (start, end) = locate(stmt_start, stmt, Some(at_in_stmt as u32 + 1));
        assert_eq!(&buffer[start..end], "nope");
    }

    #[test]
    fn a_position_inside_an_accented_statement_lands_right() {
        // The bug class this guards: a multibyte char before the target shifts every
        // byte offset, so a char→byte conversion that assumed 1:1 would miss. `città`
        // is 5 characters but 6 bytes.
        let stmt = "SELECT città, sbagliata FROM t";
        let byte = stmt.find("sbagliata").unwrap();
        // The server reports a 1-based CHARACTER column, not a byte one.
        let char_col = stmt[..byte].chars().count() as u32 + 1;
        let (start, end) = locate(0, stmt, Some(char_col));
        assert_eq!(&stmt[start..end], "sbagliata");
    }

    #[test]
    fn no_position_underlines_the_whole_statement() {
        let stmt = "SELECT foo(";
        assert_eq!(locate(10, stmt, None), (10, 10 + stmt.len()));
    }

    #[test]
    fn a_position_on_punctuation_underlines_one_character() {
        let stmt = "SELECT a , b";
        let comma = stmt.find(',').unwrap();
        let (start, end) = locate(0, stmt, Some(comma as u32 + 1));
        assert_eq!(&stmt[start..end], ",");
    }

    #[test]
    fn only_dml_and_select_are_preparable() {
        assert!(preparable(StatementKind::Select));
        assert!(preparable(StatementKind::Update));
        assert!(!preparable(StatementKind::Create));
        assert!(!preparable(StatementKind::Block));
        assert!(!preparable(StatementKind::Set));
    }
}
