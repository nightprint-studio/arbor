//! `ast` domain — the syntax tree of one script, as the panel draws it.
//!
//! Thin on purpose: everything here is `arbor-syntax` pointed at Picus's grammar
//! and Picus's already-decoded text. The panel is language-agnostic and so is the
//! crate behind it; this module supplies the two things only Picus can — *which*
//! grammar, and *which bytes*.
//!
//! ## The bytes come from the snapshot, never from a fresh read
//!
//! A script was decoded once, out of whatever encoding its folder expects, and the
//! editor is showing **that** string. Re-reading the file here would re-decode it,
//! and a byte range computed against a second decoding is a range that selects the
//! wrong characters in a file with accents in it — which is every file in the
//! repositories this product exists for.

use picus_core::prelude::PicusState;
use picus_parse::prelude::{parse, DialectScope, EngineKind, ParseErrorKind};
use serde::{Deserialize, Serialize};

use arbor_syntax::prelude::{
    node_path_at_with, outline_with, ByteRange, Injection, OutlineOptions, SyntaxTree,
};

use crate::scripts::snapshot_for;

/// One thing the grammar could not read, placed where the editor can underline it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseFault {
    /// UTF-8 byte offsets, as everything the backend reports is.
    pub start: usize,
    pub end: usize,
    /// Written for the person who has to fix it, not for the person who wrote the
    /// grammar. It names what is wrong and, where the parser knows, what is
    /// missing — never a node kind, which is an answer to a question nobody asked.
    pub message: String,
}

/// What the parser could not read in this text.
///
/// ## Why the editor did not already know
///
/// The syntax-tree panel has always shown these — it marks the node red and says
/// `invented` on a token the parser had to supply — while the editor beside it
/// showed nothing at all. So a statement that will certainly fail to run looked
/// perfectly fine until you ran it, and the one panel that knew better was the one
/// nobody had open. The live diagnostics are a *semantic* scan (unknown table,
/// unknown column, a write on a read-only connection) and had no way to say
/// "this is not SQL"; that is a question only the grammar can answer.
///
/// ## A zero-width range is a real answer
///
/// A `Missing` error is a token the grammar required and the source did not have,
/// so tree-sitter inserts it with no width. The range then marks the *position* it
/// should have occupied rather than any text — which is exactly right for "a `)` is
/// missing here", and which the editor widens to one character so the mark has
/// somewhere to go.
#[arbor_rpc::handler]
fn picus_parse_faults(
    _state: &PicusState,
    text: String,
    engine: Option<EngineKind>,
) -> Result<Vec<ParseFault>, String> {
    let scope = engine.map(DialectScope::One).unwrap_or(DialectScope::Portable);
    Ok(parse(&text, scope)
        .errors
        .into_iter()
        .map(|error| {
            let message = match (error.kind, error.expected.as_deref()) {
                (ParseErrorKind::Missing, Some(token)) => {
                    format!("`{token}` is missing here.")
                }
                (ParseErrorKind::Missing, None) => {
                    "Something the statement needs is missing here.".to_string()
                }
                (ParseErrorKind::Syntax, _) if error.text.is_empty() => {
                    "The parser could not read this.".to_string()
                }
                (ParseErrorKind::Syntax, _) => {
                    // The offending text, and where it sits. `parent` is a grammar
                    // node kind, so it is only offered when it reads as English.
                    format!("`{}` does not belong here.", error.text.trim())
                }
            };
            ParseFault { start: error.range.start, end: error.range.end, message }
        })
        .collect())
}

#[cfg(test)]
mod fault_tests {
    use super::*;

    fn faults(sql: &str) -> Vec<String> {
        parse(sql, DialectScope::One(EngineKind::Postgres))
            .errors
            .iter()
            .map(|e| format!("{:?}", e.kind))
            .collect()
    }

    #[test]
    fn a_statement_the_grammar_reads_has_nothing_to_report() {
        assert!(faults("SELECT * FROM archivio WHERE stato = 'EV';").is_empty());
        assert!(faults("-- una nota\nINSERT INTO archivio (protocollo) VALUES (1);").is_empty());
    }

    #[test]
    fn a_loop_outside_a_routine_is_reported() {
        // The case that started this: procedural code at the top level is not a
        // statement PostgreSQL will accept, the syntax-tree panel said so, and the
        // editor beside it showed nothing.
        let sql = "FOR r IN SELECT * FROM localstrings LOOP\n  NULL;\nEND LOOP;";
        assert!(!faults(sql).is_empty(), "the parser should refuse this");
    }

    #[test]
    fn an_unclosed_construct_is_reported() {
        assert!(!faults("SELECT * FROM (SELECT 1").is_empty());
        assert!(!faults("INSERT INTO archivio (protocollo VALUES (1);").is_empty());
    }
}

/// The one island in SQL: a routine's `$$ … $$` body.
///
/// PostgreSQL's grammar hands the body back as **one string token**, so a tree
/// that stops there stops exactly where an update script does its work — the
/// INSERT that has to be looked at is inside it, three levels down. This is the
/// same descent `picus-parse` already makes for the analysis; the panel makes it
/// too, so the two describe the same file.
///
/// The parents are load-bearing. `$$ … $$` is only a body under `do_statement` or
/// `routine_body`; anywhere else it is an ordinary string literal, and re-parsing
/// one of those would invent structure the author never wrote — a tree that says
/// something false is worse than one that stops.
fn injections() -> Vec<Injection> {
    vec![Injection {
        kind: "dollar_quoted_string".to_string(),
        parents: vec!["do_statement".to_string(), "routine_body".to_string()],
        inner: dollar_quoted_body,
        language: picus_parse::prelude::language(),
    }]
}

/// The source inside `$tag$ … $tag$`, as a range into the token's own text.
///
/// The tag may be empty (`$$`) or a name (`$body$`), and the two delimiters are
/// identical by definition — which is what makes this a slice rather than a
/// search: whatever opens it, the same run closes it.
fn dollar_quoted_body(text: &str) -> Option<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'$') {
        return None;
    }
    let open = 1 + bytes[1..].iter().position(|&b| b == b'$')? + 1;
    let tag = &text[..open];
    if text.len() < open * 2 || !text.ends_with(tag) {
        return None;
    }
    Some(open..text.len() - tag.len())
}

/// What the panel asks for. Every field optional: the defaults are the ones a
/// panel opening on an unknown file wants.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRequest {
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub max_nodes: Option<usize>,
    /// Hide the commas and the keywords. Off by default — see `arbor-syntax`: the
    /// panel exists to explain a parse, and a stray comma is very often the
    /// explanation.
    #[serde(default)]
    pub named_only: bool,
}

impl TreeRequest {
    fn options(&self) -> OutlineOptions {
        let defaults = OutlineOptions::default();
        OutlineOptions {
            max_depth: self.max_depth,
            max_nodes: self.max_nodes.or(defaults.max_nodes),
            named_only: self.named_only,
            text_preview: defaults.text_preview,
        }
    }
}

/// The syntax tree of one script.
#[arbor_rpc::handler]
fn picus_syntax_tree(
    state: &PicusState,
    root: String,
    path: String,
    request: Option<TreeRequest>,
) -> Result<SyntaxTree, String> {
    let text = script_text(state, &root, &path)?;
    outline_with(
        &picus_parse::prelude::language(),
        &text,
        &request.unwrap_or_default().options(),
        &injections(),
    )
    .map_err(|e| e.to_string())
}

/// The same, for text the user is editing but has not saved.
///
/// A separate handler rather than an optional field on the one above, because the
/// two answer different questions: this one is about a buffer, and a buffer has no
/// path to be stale against.
#[arbor_rpc::handler]
fn picus_syntax_tree_of(
    _state: &PicusState,
    text: String,
    request: Option<TreeRequest>,
) -> Result<SyntaxTree, String> {
    outline_with(
        &picus_parse::prelude::language(),
        &text,
        &request.unwrap_or_default().options(),
        &injections(),
    )
    .map_err(|e| e.to_string())
}

/// The root-to-leaf chain of nodes holding a byte offset — "reveal what the cursor
/// is in".
///
/// Answers with ranges rather than with node ids: the panel already has the tree
/// keyed by range, and an id would be a second identity for the same node that
/// could drift from the first.
#[arbor_rpc::handler]
fn picus_syntax_path_at(
    _state: &PicusState,
    text: String,
    offset: usize,
) -> Result<Vec<ByteRange>, String> {
    node_path_at_with(&picus_parse::prelude::language(), &text, offset, &injections())
        .map_err(|e| e.to_string())
}

/// The decoded text of one script, as the editor has it.
fn script_text(state: &PicusState, root: &str, path: &str) -> Result<String, String> {
    let snapshot = snapshot_for(state, root)?;
    snapshot
        .source(path)
        .map(|source| source.text.clone())
        .ok_or_else(|| format!("{path} is not one of this project's scripts"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_syntax::prelude::SyntaxNode;

    const SQL: &str = "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('A', 'Alfa');";

    fn tree_of(source: &str) -> arbor_syntax::prelude::SyntaxTree {
        tree_with(source, TreeRequest::default())
    }

    fn tree_with(source: &str, request: TreeRequest) -> arbor_syntax::prelude::SyntaxTree {
        outline_with(
            &picus_parse::prelude::language(),
            source,
            &request.options(),
            &injections(),
        )
        .expect("outlines")
    }

    fn path_at(source: &str, at: usize) -> Vec<ByteRange> {
        node_path_at_with(&picus_parse::prelude::language(), source, at, &injections())
            .expect("walks")
    }

    /// Every claim a range makes, checked against the source it came from.
    ///
    /// This is the whole contract of the panel: a click selects `source[range]`,
    /// so a range that does not slice, escapes its parent, or overlaps its
    /// sibling is a click that selects the wrong text. Asserted structurally
    /// rather than against the grammar's node names, which are `picus-parse`'s
    /// business — naming them here would make this a change-detector for a
    /// grammar that is still moving.
    fn assert_ranges_are_sound(node: &SyntaxNode, source: &str, path: &str) {
        let here = format!("{path} > {}", node.kind);
        assert!(node.range.slice(source).is_some(), "{here}: range does not slice the source");

        let mut previous: Option<&SyntaxNode> = None;
        for child in &node.children {
            assert!(
                node.range.contains(&child.range),
                "{here}: child {} at {:?} escapes its parent at {:?}",
                child.kind,
                child.range,
                node.range
            );
            if let Some(before) = previous {
                assert!(
                    before.range.end <= child.range.start,
                    "{here}: {} at {:?} overlaps {} at {:?}",
                    before.kind,
                    before.range,
                    child.kind,
                    child.range
                );
            }
            previous = Some(child);
            assert_ranges_are_sound(child, source, &here);
        }
    }

    /// The line a node reports must be the line its range is actually on.
    ///
    /// A separate invariant from the ranges, and the one that catches a
    /// byte-versus-character confusion: they agree on a pure-ASCII file and
    /// diverge the moment the file has an accent in it.
    fn assert_lines_agree(node: &SyntaxNode, source: &str) {
        let before = &source[..node.range.start];
        let expected = before.matches('\n').count() + 1;
        assert_eq!(
            node.line, expected,
            "{} at {:?} says line {} but its bytes are on line {expected}",
            node.kind, node.range, node.line
        );
        for child in &node.children {
            assert_lines_agree(child, source);
        }
    }

    /// The node whose text is exactly `needle`, if the tree has one.
    fn node_holding<'a>(node: &'a SyntaxNode, source: &str, needle: &str) -> Option<&'a SyntaxNode> {
        if node.range.slice(source) == Some(needle) {
            return Some(node);
        }
        node.children.iter().find_map(|c| node_holding(c, source, needle))
    }

    #[test]
    fn the_tree_of_a_statement_reaches_its_table_and_its_values() {
        let tree = tree_of(SQL);
        assert!(!tree.has_errors, "the fixture must parse");
        assert_ranges_are_sound(&tree.root, SQL, "");
        assert_lines_agree(&tree.root, SQL);
        assert!(node_holding(&tree.root, SQL, "CATALOGO_WIDGET").is_some());
    }

    #[test]
    fn the_path_to_a_column_name_ends_on_that_column_name() {
        let at = SQL.find("ETICHETTA").expect("present");
        let path = path_at(SQL, at);
        assert_eq!(path.last().and_then(|r| r.slice(SQL)), Some("ETICHETTA"));
    }

    // ── Procedural bodies ─────────────────────────────────────────────────────
    //
    // The shape every update script in these repositories is in, and the one the
    // panel was reported wrong on. Each fixture asserts the same two invariants —
    // sound ranges, honest lines — plus that a name *inside* the body is reachable
    // and slices to itself.

    const ORACLE_BLOCK: &str = "\
-- 2.4 -> 2.5
DECLARE
  v_presenti NUMBER;
BEGIN
  SELECT COUNT(*) INTO v_presenti FROM CATALOGO_WIDGET WHERE CHIAVE = 'A';
  IF v_presenti = 0 THEN
    INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('A', 'Alfa');
  END IF;
END;
/
";

    #[test]
    fn an_oracle_anonymous_block_keeps_every_range_honest() {
        let tree = tree_of(ORACLE_BLOCK);
        assert_ranges_are_sound(&tree.root, ORACLE_BLOCK, "");
        assert_lines_agree(&tree.root, ORACLE_BLOCK);

        // The INSERT is three levels down — inside IF, inside BEGIN, inside the
        // block — which is exactly where a walker that gave up on procedural
        // bodies would have stopped.
        let inner = "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('A', 'Alfa')";
        let at = ORACLE_BLOCK.find(inner).expect("present");
        let path = path_at(ORACLE_BLOCK, at + 20);
        assert!(path.len() > 3, "the path stops at the block: {path:?}");
        assert_eq!(path.last().and_then(|r| r.slice(ORACLE_BLOCK)), Some("CATALOGO_WIDGET"));
    }

    const PLPGSQL_FUNCTION: &str = "\
CREATE OR REPLACE FUNCTION aggiorna_catalogo() RETURNS void AS $$
DECLARE
  v_presenti int;
BEGIN
  SELECT COUNT(*) INTO v_presenti FROM catalogo_widget WHERE chiave = 'A';
  IF v_presenti = 0 THEN
    INSERT INTO catalogo_widget (chiave, etichetta) VALUES ('A', 'Alfa');
  END IF;
END;
$$ LANGUAGE plpgsql;
SELECT aggiorna_catalogo();
DROP FUNCTION aggiorna_catalogo();
";

    #[test]
    fn a_dollar_quoted_function_body_keeps_every_range_honest() {
        // The dollar-quoted body is read by the external scanner, which is the one
        // place a byte offset could plausibly be handed back from a different
        // string than the one it indexes.
        let tree = tree_of(PLPGSQL_FUNCTION);
        assert_ranges_are_sound(&tree.root, PLPGSQL_FUNCTION, "");
        assert_lines_agree(&tree.root, PLPGSQL_FUNCTION);

        let at = PLPGSQL_FUNCTION.find("catalogo_widget (chiave").expect("present");
        let path = path_at(PLPGSQL_FUNCTION, at);
        assert_eq!(path.last().and_then(|r| r.slice(PLPGSQL_FUNCTION)), Some("catalogo_widget"));
    }

    const ORACLE_PROCEDURE: &str = "\
CREATE OR REPLACE PROCEDURE allinea_schedario (p_chiave IN VARCHAR2) AS
  v_conta NUMBER;
BEGIN
  SELECT COUNT(*) INTO v_conta FROM STAGING_IMPORT WHERE CHIAVE = p_chiave;
  IF v_conta > 0 THEN
    DELETE FROM STAGING_IMPORT WHERE CHIAVE = p_chiave;
  END IF;
EXCEPTION
  WHEN OTHERS THEN
    NULL;
END allinea_schedario;
/
";

    #[test]
    fn a_stored_procedure_with_an_exception_section_keeps_every_range_honest() {
        let tree = tree_of(ORACLE_PROCEDURE);
        assert_ranges_are_sound(&tree.root, ORACLE_PROCEDURE, "");
        assert_lines_agree(&tree.root, ORACLE_PROCEDURE);
        assert!(node_holding(&tree.root, ORACLE_PROCEDURE, "STAGING_IMPORT").is_some());
    }

    #[test]
    fn a_block_whose_comments_are_accented_does_not_shift_by_a_byte() {
        // The failure this exists to catch, and the reason it uses a comment
        // rather than a value: an accented character is two bytes, so any offset
        // computed in characters is short by one from here on — and everything
        // below the comment selects one character to the left.
        let sql = "\
-- perché la soglia è cambiata
DECLARE
  v_conta NUMBER;
BEGIN
  INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('A', 'Città');
END;
/
";
        let tree = tree_of(sql);
        assert_ranges_are_sound(&tree.root, sql, "");
        assert_lines_agree(&tree.root, sql);

        let table = node_holding(&tree.root, sql, "CATALOGO_WIDGET").expect("the table is a node");
        // Not "a node exists whose text is that" — the range must land on it in
        // the ORIGINAL string, which is what a click selects.
        assert_eq!(&sql[table.range.start..table.range.end], "CATALOGO_WIDGET");

        let at = sql.find("Città").expect("present");
        let path = path_at(sql, at);
        assert_eq!(path.last().and_then(|r| r.slice(sql)), Some("'Città'"));
    }

    #[test]
    fn a_crlf_block_reports_the_lines_the_editor_shows() {
        // The offsets a CRLF file produces are one byte per preceding line larger
        // than an LF one. They are still correct *against these bytes* — which is
        // the whole point, and why the panel must be fed the same string the
        // ranges were computed from.
        let lf = ORACLE_BLOCK.to_string();
        let crlf = lf.replace('\n', "\r\n");

        let tree = tree_of(&crlf);
        assert_ranges_are_sound(&tree.root, &crlf, "");
        assert_lines_agree(&tree.root, &crlf);

        let table = node_holding(&tree.root, &crlf, "CATALOGO_WIDGET").expect("present");
        assert_eq!(&crlf[table.range.start..table.range.end], "CATALOGO_WIDGET");

        // …and the same node in the LF text is at a *different* offset. Stated as
        // an assertion because it is the trap: feeding the panel LF text while
        // slicing CRLF bytes shifts everything below line one.
        let lf_tree = tree_of(&lf);
        let same = node_holding(&lf_tree.root, &lf, "CATALOGO_WIDGET").expect("present");
        assert_ne!(table.range.start, same.range.start, "CRLF must not agree with LF here");
    }

    #[test]
    fn a_script_that_will_not_parse_still_answers() {
        // The panel's whole reason for existing on a bad file.
        let broken = "DECLARE BEGIN INSERT INTO CATALOGO_WIDGET (CHIAVE VALUES ;";
        let tree = tree_of(broken);
        assert!(tree.has_errors);
        // Recovery nodes are still nodes, and their ranges are still clicked on.
        assert_ranges_are_sound(&tree.root, broken, "");
        assert_lines_agree(&tree.root, broken);
    }

    #[test]
    fn hiding_the_punctuation_does_not_move_what_is_left() {
        // The toggle changes which nodes are reported, never their ranges — a
        // filter that shifted offsets would be the worst kind of wrong, since the
        // tree would still look plausible.
        let tidy = tree_with(ORACLE_BLOCK, TreeRequest { named_only: true, ..TreeRequest::default() });
        assert_ranges_are_sound(&tidy.root, ORACLE_BLOCK, "");
        assert_lines_agree(&tidy.root, ORACLE_BLOCK);

        let everything = tree_of(ORACLE_BLOCK);
        let full = node_holding(&everything.root, ORACLE_BLOCK, "CATALOGO_WIDGET");
        let named = node_holding(&tidy.root, ORACLE_BLOCK, "CATALOGO_WIDGET");
        assert_eq!(full.map(|n| n.range), named.map(|n| n.range));
    }
}
