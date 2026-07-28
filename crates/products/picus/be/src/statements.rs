//! `statements` domain — where one statement ends and the next begins.
//!
//! One handler, and it exists so that pressing Run means something precise. A
//! query buffer is not one statement: it is a scratchpad with a `SELECT` at the
//! top, three `INSERT`s from yesterday and a `COMMIT` at the bottom. Sending the
//! whole thing to the server is what the editor used to do, and it fails in the
//! worst available way — PostgreSQL runs a multi-statement string over the simple
//! protocol, which materialises **every** result in memory at once and holds
//! nothing that can be scrolled. A buffer with one large `SELECT` in it therefore
//! looks like the application has hung, because in every way that matters it has.
//!
//! So the caller asks where the statements are, and then runs one.
//!
//! ## Why the backend answers and not the editor
//!
//! Because a semicolon is not a statement boundary. It is a boundary *unless* it
//! is inside a string literal, a comment, a dollar-quoted PostgreSQL body or an
//! Oracle `DECLARE … BEGIN … END;` block — and in that last case there are
//! several of them, none of which ends anything. A regular expression over the
//! text gets every one of those wrong, and the wrong answer is not a formatting
//! glitch: it is half a statement sent to a production database.
//!
//! `picus-parse` already knows, in both dialects, because the whole product rests
//! on it knowing. Asking it here costs one parse of an editor buffer.
//!
//! ## Offsets are the editor's, not Rust's
//!
//! Everything crossing this seam is measured in **UTF-16 code units**, which is
//! what a JavaScript string index and a CodeMirror position are. Rust measures in
//! bytes. On a pure-ASCII buffer the two agree, which is exactly what makes the
//! difference so expensive to leave implicit: it works until somebody writes an
//! accented word in a comment above their SQL, and then Run silently executes a
//! range shifted by one character per accent. Converted here, once, at the edge.

use picus_parse::prelude::{DialectScope, EngineKind, SqlParser, StatementKind};
use serde::Serialize;

use picus_core::prelude::PicusState;

/// One statement in a buffer, addressed the way the editor addresses text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementSpan {
    /// First code unit of the statement, terminator included.
    pub start: usize,
    /// One past the last.
    pub end: usize,
    /// 1-based line the statement starts on — for the log line that says what ran.
    pub line: usize,
    /// `select`, `insert`, `block`, … The interface uses it to label a run and to
    /// decide nothing: what a statement *does* is the server's business.
    pub kind: StatementKind,
}

/// Where the statements are in a buffer.
///
/// Never fails on bad SQL: half-typed text is the normal state of an editor, and
/// a Run that refused to find its statement because the *next* one is incomplete
/// would be a Run that stops working while you type. Whatever the parser could
/// make out is returned, and the server remains the authority on whether it runs.
#[arbor_rpc::handler]
fn picus_sql_statements(
    _state: &PicusState,
    sql: String,
    dialect: EngineKind,
) -> Result<Vec<StatementSpan>, String> {
    Ok(spans_of(&sql, dialect))
}

fn spans_of(sql: &str, dialect: EngineKind) -> Vec<StatementSpan> {
    let parsed = SqlParser::new().parse(sql, DialectScope::One(dialect));
    let index = Utf16Index::of(sql);
    parsed
        .statements
        .iter()
        // A lone Oracle `/` terminates the statement above it and is not one. Run
        // would otherwise offer to execute a slash.
        .filter(|s| s.kind != StatementKind::Terminator)
        .filter(|s| !sql[s.range.start..s.range.end].trim().is_empty())
        .map(|s| StatementSpan {
            start: index.at(s.range.start),
            end: index.at(s.range.end),
            line: parsed.line_of(s.range.start),
            kind: s.kind,
        })
        .collect()
}

/// Byte offset → UTF-16 code unit offset, for one string.
///
/// Built once per buffer rather than counted per statement: counting from zero
/// for each answer is the shape that made indexing a repository quadratic once
/// already (see `picus-parse`'s `line_col`), and there is no reason to learn it
/// twice.
struct Utf16Index {
    /// Running UTF-16 length at each character boundary, and the byte offset it
    /// belongs to. Ascending in both, so a lookup is a binary search.
    marks: Vec<(usize, usize)>,
}

impl Utf16Index {
    fn of(text: &str) -> Utf16Index {
        let mut marks = Vec::new();
        let mut units = 0usize;
        for (offset, ch) in text.char_indices() {
            marks.push((offset, units));
            units += ch.len_utf16();
        }
        marks.push((text.len(), units));
        Utf16Index { marks }
    }

    /// The code-unit offset for a byte offset. An offset in the middle of a
    /// character — which a range from the parser never is — rounds down to the
    /// character it is inside, which keeps the answer inside the buffer.
    fn at(&self, byte: usize) -> usize {
        match self.marks.binary_search_by_key(&byte, |(offset, _)| *offset) {
            Ok(i) => self.marks[i].1,
            Err(i) => self.marks[i.saturating_sub(1)].1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oracle(sql: &str) -> Vec<StatementSpan> {
        spans_of(sql, EngineKind::Oracle)
    }

    /// The text a span addresses, sliced the way the editor would slice it.
    fn sliced(sql: &str, span: &StatementSpan) -> String {
        let units: Vec<u16> = sql.encode_utf16().collect();
        String::from_utf16_lossy(&units[span.start..span.end]).trim().to_string()
    }

    #[test]
    fn a_buffer_of_several_statements_is_split_at_the_real_boundaries() {
        let sql = "SELECT * FROM ORDINI;\nINSERT INTO T (A) VALUES (1);\nCOMMIT;";
        let spans = oracle(sql);
        assert_eq!(spans.len(), 3, "{spans:?}");
        assert_eq!(spans[0].kind, StatementKind::Select);
        assert_eq!(sliced(sql, &spans[0]), "SELECT * FROM ORDINI;");
        assert_eq!(sliced(sql, &spans[1]), "INSERT INTO T (A) VALUES (1);");
        assert_eq!(spans[1].line, 2);
    }

    #[test]
    fn a_semicolon_inside_a_literal_is_not_a_boundary() {
        // The failure a regular expression over the text produces, and it is not a
        // formatting glitch — it is half a statement sent to a database.
        let sql = "INSERT INTO T (D) VALUES ('a;b');";
        let spans = oracle(sql);
        assert_eq!(spans.len(), 1, "{spans:?}");
        assert_eq!(sliced(sql, &spans[0]), sql);
    }

    #[test]
    fn a_block_is_one_statement_however_many_semicolons_it_holds() {
        // The shape every Oracle upgrade script in the corpus has. Splitting on
        // `;` here would produce four fragments, none of which runs.
        let sql = "DECLARE n NUMBER;\nBEGIN\n  n := 1;\n  INSERT INTO T (A) VALUES (n);\nEND;";
        let spans = oracle(sql);
        assert_eq!(spans.len(), 1, "{spans:?}");
        assert_eq!(spans[0].kind, StatementKind::Block);
    }

    #[test]
    fn the_offsets_are_the_editors_and_not_rusts() {
        // One accented character before the statement is enough to shift every
        // byte offset past it. The symptom would be Run executing a range one
        // character short — silently, and only in files written in Italian.
        let sql = "-- però\nSELECT 1;";
        assert!(sql.len() > sql.encode_utf16().count(), "the fixture has to be multibyte");

        let spans = oracle(sql);
        assert_eq!(spans.len(), 1, "{spans:?}");
        // Sliced in code units, as the editor would. With byte offsets on the wire
        // this comes back as `ELECT 1;` — one character short, only in files
        // written in Italian, and with no error anywhere.
        assert_eq!(sliced(sql, &spans[0]), "SELECT 1;");
    }

    #[test]
    fn a_leading_comment_is_not_part_of_the_statement_it_sits_above() {
        // Stated because the caller has to handle it: a cursor parked on the
        // comment line is inside no span, and Run has to reach for the statement
        // below rather than concluding there is nothing to run.
        let sql = "-- the daily list\nSELECT 1;";
        let spans = oracle(sql);
        assert_eq!(sliced(sql, &spans[0]), "SELECT 1;");
        assert!(spans[0].start > 0);
    }

    #[test]
    fn half_typed_text_still_answers() {
        // The normal state of an editor. A Run that stopped finding its statement
        // because the next one is incomplete would stop working while you type.
        let spans = oracle("SELECT * FROM ORDINI;\nINSERT INTO ");
        assert!(!spans.is_empty());
        assert_eq!(spans[0].kind, StatementKind::Select);
    }

    #[test]
    fn an_empty_buffer_has_no_statements() {
        assert!(oracle("").is_empty());
        assert!(oracle("   \n\n  ").is_empty());
        assert!(oracle("-- just a note").iter().all(|s| s.kind != StatementKind::Select));
    }
}
