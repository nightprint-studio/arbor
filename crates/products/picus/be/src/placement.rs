//! Where a generated block goes in a destination file — and, when Picus has been
//! there before, which bytes it replaces.
//!
//! Pure: a source string, its parse, the table being written, the rule in force
//! and the project's marker template in; one byte range and a sentence out. No
//! filesystem, no config lookup, no I/O — the resolution of *which* rule applies
//! is the caller's, and it is one line ([`crate::apply::insertion_rule_for`]).
//!
//! ## Two decisions the rest of the apply rests on
//!
//! **Replacing beats appending.** A block Picus wrote carries a marker, and
//! regenerating it must land in the same place rather than leaving the old copy
//! above the new one. `MarkerTemplate::recognises` finds the marker line; the
//! statements underneath it say which table that block is about, so a file holding
//! a block for `PARAMETRI` and one for `LISTINI` regenerates the right one.
//!
//! **The extent of an existing block is decided by the statements, never by a
//! distance.** A block runs from its marker line to the end of the last
//! *consecutive* statement that touches the same table — so a hand-written
//! statement that follows a generated block is never swallowed by a regeneration,
//! which is the failure mode that would make this whole feature untrustworthy.
//! There is deliberately no end marker: the file format is the user's, and a tool
//! that needs two comments to find its own work has already lost the ability to
//! survive someone editing one of them.

use std::ops::Range;

use picus_parse::prelude::{ObjectRef, ParsedFile, Statement, StatementKind};
use picus_project::prelude::{InsertionRule, MarkerTemplate};

/// Where a block lands, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPlacement {
    /// The bytes of the original text this block occupies. Empty for a pure
    /// insertion; non-empty when an earlier generated block is being replaced.
    pub range: Range<usize>,
    /// Written into the diff's hunk header — the rule that put it there, in
    /// words, so a reviewer never has to guess.
    pub reason: String,
    /// `true` when a block Picus wrote before was found and is being regenerated
    /// in place.
    pub replaces_existing: bool,
}

/// Decide where the generated block for `table` goes in `source`.
///
/// `table` is the name in comparison form (`ObjectRef::folded_name` — unquoted
/// names upper-cased), because that is the only form in which an Oracle
/// `PARAMETRI` and a PostgreSQL `parametri` are the same table.
pub fn place(
    source: &str,
    parsed: &ParsedFile,
    table: &str,
    rule: InsertionRule,
    marker: &MarkerTemplate,
) -> BlockPlacement {
    if let Some(range) = existing_block(source, parsed, table, marker) {
        return BlockPlacement {
            range,
            reason: format!("regenerated in place, replacing the block Picus wrote for {table}"),
            replaces_existing: true,
        };
    }

    let (at, reason) = insertion_point(source, parsed, table, rule);
    BlockPlacement { range: at..at, reason, replaces_existing: false }
}

/// The offset a new block is inserted at, and the sentence that explains it.
fn insertion_point(
    source: &str,
    parsed: &ParsedFile,
    table: &str,
    rule: InsertionRule,
) -> (usize, String) {
    match rule {
        InsertionRule::EndOfFile => (end_of_last_statement(source, parsed), rule.describe().to_string()),

        InsertionRule::AfterLastOnTable => {
            match parsed.statements.iter().rev().find(|s| touches(s, table)) {
                Some(statement) => (
                    after(source, statement.range.end),
                    format!("{}, {table}", rule.describe()),
                ),
                // The fallback is stated rather than silent: a reader who
                // configured "group by table" and finds the block at the bottom
                // deserves to be told why it is there.
                None => (
                    end_of_last_statement(source, parsed),
                    format!(
                        "nothing in this file touches {table}, so it goes {}",
                        InsertionRule::EndOfFile.describe()
                    ),
                ),
            }
        }

        InsertionRule::BeforeFinalCommit => match final_commit(source, parsed) {
            Some(statement) => (
                line_start(source, statement.range.start),
                rule.describe().to_string(),
            ),
            None => (
                end_of_last_statement(source, parsed),
                format!(
                    "this file has no final COMMIT, so it goes {}",
                    InsertionRule::EndOfFile.describe()
                ),
            ),
        },
    }
}

/// The bytes an earlier generated block for `table` occupies, if there is one.
fn existing_block(
    source: &str,
    parsed: &ParsedFile,
    table: &str,
    marker: &MarkerTemplate,
) -> Option<Range<usize>> {
    if marker.is_disabled() {
        // Marking is off, so there is nothing to recognise and every generation
        // appends. The settings screen says so out loud, because this is the one
        // thing emptying the template costs.
        return None;
    }

    let markers = marker_lines(source, marker);
    for (index, start) in markers.iter().enumerate() {
        let body_from = after(source, line_end(source, *start));
        let stop_at = markers.get(index + 1).copied().unwrap_or(source.len());

        let mut end = None;
        for statement in &parsed.statements {
            if statement.range.start < body_from {
                continue;
            }
            if statement.range.start >= stop_at || !touches(statement, table) {
                break;
            }
            end = Some(statement.range.end);
        }

        // A marker whose statements are about another table belongs to another
        // block. Skip it rather than claiming it.
        if let Some(end) = end {
            return Some(*start..after(source, end));
        }
    }
    None
}

/// The byte offset of every line that reads like one of this project's markers.
fn marker_lines(source: &str, marker: &MarkerTemplate) -> Vec<usize> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for line in source.split_inclusive('\n') {
        if marker.recognises(line.trim_end_matches(['\r', '\n'])) {
            out.push(at);
        }
        at += line.len();
    }
    out
}

/// Does this statement act on `table`, in any of the ways a statement can?
fn touches(statement: &Statement, table: &str) -> bool {
    let named = statement.defines.iter().chain(statement.references.iter());
    named.chain(statement.dml.iter().map(|d| &d.table)).any(|o| is(o, table))
}

fn is(object: &ObjectRef, table: &str) -> bool {
    object.folded_name() == table
}

/// The file's last `COMMIT`, if it ends with one.
fn final_commit<'a>(source: &str, parsed: &'a ParsedFile) -> Option<&'a Statement> {
    parsed.statements.iter().rev().find(|s| {
        s.kind == StatementKind::Transaction
            && s.range.slice(source).trim_start().to_ascii_uppercase().starts_with("COMMIT")
    })
}

/// Just past the last complete statement — or the end of the file when there is
/// none, which is what an empty or comment-only file is.
fn end_of_last_statement(source: &str, parsed: &ParsedFile) -> usize {
    match parsed.statements.last() {
        Some(statement) => after(source, statement.range.end),
        None => source.len(),
    }
}

/// Advance past the rest of the line `at` sits on: trailing spaces, then one line
/// break. This is what makes an insertion land on a line of its own and a
/// replacement give back exactly the bytes it took, so regenerating an unchanged
/// block produces a byte-identical file.
fn after(source: &str, at: usize) -> usize {
    let mut cursor = at;
    let bytes = source.as_bytes();
    while cursor < bytes.len() && (bytes[cursor] == b' ' || bytes[cursor] == b'\t') {
        cursor += 1;
    }
    if cursor < bytes.len() && bytes[cursor] == b'\r' {
        cursor += 1;
    }
    if cursor < bytes.len() && bytes[cursor] == b'\n' {
        return cursor + 1;
    }
    // No line break to consume (the last line of a file with no trailing
    // newline): stop where the statement did rather than eating the spaces.
    at
}

/// The end of the line `at` sits on, before its line break.
fn line_end(source: &str, at: usize) -> usize {
    match source[at..].find('\n') {
        Some(offset) => at + offset,
        None => source.len(),
    }
}

/// The start of the line `at` sits on.
fn line_start(source: &str, at: usize) -> usize {
    match source[..at].rfind('\n') {
        Some(offset) => offset + 1,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_parse::prelude::{DialectScope, EngineKind, SqlParser};

    fn parse(source: &str) -> ParsedFile {
        SqlParser::new().parse(source, DialectScope::One(EngineKind::Oracle))
    }

    fn marker() -> MarkerTemplate {
        MarkerTemplate::default()
    }

    /// Place a block and return the text that results from splicing `block` in —
    /// asserting on the resulting file rather than on an offset, because the
    /// offset is an implementation detail and the file is the contract.
    fn spliced(source: &str, table: &str, rule: InsertionRule, block: &str) -> String {
        let parsed = parse(source);
        let placement = place(source, &parsed, table, rule, &marker());
        let mut out = String::new();
        out.push_str(&source[..placement.range.start]);
        out.push_str(block);
        out.push_str(&source[placement.range.end..]);
        out
    }

    const INIT: &str = "-- tabelle\r\n\
                        CREATE TABLE PARAMETRI (COD VARCHAR2(30));\r\n\
                        INSERT INTO PARAMETRI (COD) VALUES ('A');\r\n\
                        CREATE TABLE LISTINI (COD VARCHAR2(30));\r\n\
                        INSERT INTO LISTINI (COD) VALUES ('L');\r\n";

    #[test]
    fn an_update_folder_appends_after_the_last_statement() {
        let out = spliced(INIT, "PARAMETRI", InsertionRule::EndOfFile, "-- NEW\r\n");
        assert!(out.ends_with("INSERT INTO LISTINI (COD) VALUES ('L');\r\n-- NEW\r\n"), "{out}");
    }

    #[test]
    fn an_init_folder_groups_with_the_statements_on_the_same_table() {
        // The block for PARAMETRI lands under the last PARAMETRI statement, not
        // at the bottom of the file — which is the whole point of the rule.
        let out = spliced(INIT, "PARAMETRI", InsertionRule::AfterLastOnTable, "-- NEW\r\n");
        assert!(
            out.contains("VALUES ('A');\r\n-- NEW\r\nCREATE TABLE LISTINI"),
            "{out}"
        );
    }

    #[test]
    fn grouping_by_table_falls_back_to_the_end_and_says_so() {
        let parsed = parse(INIT);
        let placement = place(INIT, &parsed, "SCONTI", InsertionRule::AfterLastOnTable, &marker());
        assert_eq!(placement.range.start, INIT.len());
        assert!(placement.reason.contains("nothing in this file touches SCONTI"), "{placement:?}");
    }

    #[test]
    fn before_the_final_commit_means_above_its_line() {
        let source = "INSERT INTO PARAMETRI (COD) VALUES ('A');\r\nCOMMIT;\r\n";
        let out = spliced(source, "PARAMETRI", InsertionRule::BeforeFinalCommit, "-- NEW\r\n");
        assert_eq!(out, "INSERT INTO PARAMETRI (COD) VALUES ('A');\r\n-- NEW\r\nCOMMIT;\r\n");
    }

    #[test]
    fn a_file_with_no_commit_still_gets_its_block() {
        let parsed = parse(INIT);
        let placement = place(INIT, &parsed, "PARAMETRI", InsertionRule::BeforeFinalCommit, &marker());
        assert_eq!(placement.range.start, INIT.len());
        assert!(placement.reason.contains("no final COMMIT"), "{placement:?}");
    }

    #[test]
    fn an_empty_file_takes_the_block_at_the_start() {
        for source in ["", "-- only a comment\r\n"] {
            let parsed = parse(source);
            let placement =
                place(source, &parsed, "PARAMETRI", InsertionRule::EndOfFile, &marker());
            assert_eq!(placement.range, source.len()..source.len());
            assert!(!placement.replaces_existing);
        }
    }

    #[test]
    fn a_file_with_no_trailing_newline_keeps_its_last_line_intact() {
        let source = "INSERT INTO PARAMETRI (COD) VALUES ('A');";
        let out = spliced(source, "PARAMETRI", InsertionRule::EndOfFile, "\r\n-- NEW\r\n");
        assert_eq!(out, "INSERT INTO PARAMETRI (COD) VALUES ('A');\r\n-- NEW\r\n");
    }

    // ── Replacing a block Picus wrote before ──────────────────────────────────

    const GENERATED: &str = "-- header\r\n\
                             -- picus: generated PARAMETRI\r\n\
                             INSERT INTO PARAMETRI (COD) VALUES ('A');\r\n\
                             INSERT INTO PARAMETRI (COD) VALUES ('B');\r\n\
                             -- hand-written, do not touch\r\n\
                             INSERT INTO LISTINI (COD) VALUES ('L');\r\n";

    #[test]
    fn a_block_picus_wrote_is_replaced_rather_than_duplicated() {
        let parsed = parse(GENERATED);
        let placement =
            place(GENERATED, &parsed, "PARAMETRI", InsertionRule::EndOfFile, &marker());
        assert!(placement.replaces_existing, "{placement:?}");

        let out = spliced(GENERATED, "PARAMETRI", InsertionRule::EndOfFile, "-- REGENERATED\r\n");
        assert_eq!(
            out,
            "-- header\r\n-- REGENERATED\r\n-- hand-written, do not touch\r\n\
             INSERT INTO LISTINI (COD) VALUES ('L');\r\n"
        );
    }

    #[test]
    fn a_regeneration_that_changes_nothing_reproduces_the_file_byte_for_byte() {
        // The idempotence the whole marker mechanism exists for: the replaced
        // range and the replacement have to agree down to the trailing newline,
        // or every re-run would show a diff.
        let block = "-- picus: generated PARAMETRI\r\n\
                     INSERT INTO PARAMETRI (COD) VALUES ('A');\r\n\
                     INSERT INTO PARAMETRI (COD) VALUES ('B');\r\n";
        let out = spliced(GENERATED, "PARAMETRI", InsertionRule::EndOfFile, block);
        assert_eq!(out, GENERATED);
    }

    #[test]
    fn a_hand_written_statement_below_a_block_is_never_swallowed() {
        // The extent stops at the first statement that is about something else.
        let parsed = parse(GENERATED);
        let placement =
            place(GENERATED, &parsed, "PARAMETRI", InsertionRule::EndOfFile, &marker());
        assert!(!GENERATED[placement.range].contains("LISTINI"));
    }

    #[test]
    fn the_block_for_another_table_is_left_alone() {
        let source = "-- picus: generated LISTINI\r\n\
                      INSERT INTO LISTINI (COD) VALUES ('L');\r\n\
                      -- picus: generated PARAMETRI\r\n\
                      INSERT INTO PARAMETRI (COD) VALUES ('A');\r\n";
        let out = spliced(source, "PARAMETRI", InsertionRule::EndOfFile, "-- REGENERATED\r\n");
        assert_eq!(
            out,
            "-- picus: generated LISTINI\r\n\
             INSERT INTO LISTINI (COD) VALUES ('L');\r\n\
             -- REGENERATED\r\n"
        );
    }

    #[test]
    fn a_marker_with_nothing_of_ours_under_it_is_not_claimed() {
        // Somebody deleted the statements but left the comment. Replacing "it"
        // would mean replacing a bare comment and losing the marker's meaning;
        // the block is placed by the rule instead.
        let source = "-- picus: generated PARAMETRI\r\nINSERT INTO LISTINI (COD) VALUES ('L');\r\n";
        let parsed = parse(source);
        let placement = place(source, &parsed, "PARAMETRI", InsertionRule::EndOfFile, &marker());
        assert!(!placement.replaces_existing);
        assert_eq!(placement.range.start, source.len());
    }

    #[test]
    fn an_emptied_marker_template_means_every_generation_appends() {
        // Emptying the template switches marking off, and the documented cost is
        // exactly this: a previous block can no longer be found.
        let none = MarkerTemplate(String::new());
        let parsed = parse(GENERATED);
        let placement = place(GENERATED, &parsed, "PARAMETRI", InsertionRule::EndOfFile, &none);
        assert!(!placement.replaces_existing);
        assert_eq!(placement.range.start, GENERATED.len());
    }

    #[test]
    fn a_block_wrapped_generation_is_one_statement_and_is_found_again() {
        // The update-script shape: a marker over a PL/SQL block. Its DML is three
        // levels down, which is why `touches` reads `Statement::dml` as well as
        // the references.
        let source = "-- picus: generated PARAMETRI (4.12 -> 4.13)\r\n\
                      DECLARE v NUMBER; BEGIN\r\n\
                      INSERT INTO PARAMETRI (COD) VALUES ('A');\r\n\
                      END;\r\n";
        let parsed = parse(source);
        let placement = place(source, &parsed, "PARAMETRI", InsertionRule::EndOfFile, &marker());
        assert!(placement.replaces_existing, "{placement:?}");
        assert_eq!(placement.range, 0..source.len());
    }
}
