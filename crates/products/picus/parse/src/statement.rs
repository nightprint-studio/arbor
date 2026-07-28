//! [`ParsedFile`] and [`Statement`] — the shape a caller actually reads.
//!
//! A parsed file is a **map of a string the caller still owns**: statements in
//! source order, each with the byte range it covers, and the gaps between them.
//! [`ParsedFile::segments`] walks statements and gaps together, and the invariant
//! it guarantees is the one the whole script half depends on — concatenating
//! every segment reproduces the input byte for byte.

// `Serialize` only, deliberately. A `ParsedFile` is *derived* data: it is
// produced by parsing a string and is meaningless without that string, so
// reading one back from JSON would be reconstructing a map of a territory that
// is not there. It serialises because the backend hands findings to the
// frontend; nothing ever deserialises it.
use serde::Serialize;

use crate::dialect::ForeignConstruct;
use crate::dml::DmlShape;
use crate::error::ParseError;
use crate::object::ObjectRef;
use crate::range::ByteRange;

/// What a statement does, coarsely. The *object* it acts on is in
/// [`Statement::defines`] / [`Statement::references`], so this stays a flat enum
/// rather than a product of verb × object type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StatementKind {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
    Truncate,
    Create,
    Alter,
    Drop,
    Comment,
    Grant,
    Revoke,
    /// An anonymous block: Oracle `DECLARE … BEGIN … END;` or PostgreSQL
    /// `DO $$ … $$`.
    Block,
    /// COMMIT / ROLLBACK / SAVEPOINT / BEGIN / LOCK.
    Transaction,
    Set,
    Call,
    /// A lone Oracle `/` that did not follow a statement.
    Terminator,
    /// Parsed, but of a kind this crate does not classify.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Statement {
    pub kind: StatementKind,
    /// The exact bytes of the statement, terminator included. Deleting this
    /// range leaves no orphan `;` behind.
    pub range: ByteRange,
    /// The grammar node kind of the statement body — `create_table_statement`,
    /// `merge_statement`, … Finer than [`StatementKind`] when a caller needs it.
    pub node_kind: String,
    /// Objects this statement creates or redefines (CREATE / ALTER).
    pub defines: Vec<ObjectRef>,
    /// Every other object it names: FROM tables, INSERT targets, DROP targets,
    /// foreign-key parents.
    pub references: Vec<ObjectRef>,
    /// Every INSERT / UPDATE / DELETE / MERGE in this statement, in source
    /// order.
    ///
    /// A list rather than an option, and that is the point: in a real Oracle
    /// upgrade script the INSERT lives inside `DECLARE … BEGIN … END`, so the
    /// top-level statement is a *block* and its DML is nested. A consumer that
    /// only looked at top-level INSERTs would see nothing at all in the files
    /// this product exists to maintain.
    pub dml: Vec<DmlShape>,
    /// The statement was written `CREATE OR REPLACE …`.
    ///
    /// Worth a field of its own because it is a **statement of intent**, not a
    /// spelling: the author has said "whatever was there, this is the definition
    /// now". Two files that both create an object are usually a race whose winner
    /// is decided by file order; two files that both `CREATE OR REPLACE` it are
    /// doing exactly what the syntax is for.
    pub replaces: bool,
    /// Constructs inside this statement that do not belong to the file's
    /// declared dialect.
    pub foreign: Vec<ForeignConstruct>,
    /// True when the statement contains an error node.
    pub has_error: bool,
}

/// A statement, or the bytes between two statements.
///
/// The gaps are not noise: they hold the comments, blank lines and BOM that a
/// rewriter must preserve, and enumerating them is how the byte-identical
/// round trip is checked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment<'a> {
    Statement(&'a Statement),
    Gap(ByteRange),
}

impl Segment<'_> {
    pub fn range(&self) -> ByteRange {
        match self {
            Segment::Statement(s) => s.range,
            Segment::Gap(r) => *r,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedFile {
    /// What the file was parsed **as** — one dialect, or portable. Always
    /// supplied by the caller; there is no ambient dialect anywhere in Picus
    /// (`docs/picus-design.md` §1).
    ///
    /// Under `Portable` the `foreign` list inverts: a construct belonging to
    /// *either* dialect is recorded, because the file promised to run on both.
    pub scope: picus_types::prelude::DialectScope,
    /// Length in bytes of the source this file maps. Kept so the segment walk
    /// and the round-trip check work without the source.
    pub source_len: usize,
    pub statements: Vec<Statement>,
    pub errors: Vec<ParseError>,
    /// Byte offset of the start of every line, `[0, …]`, ascending.
    ///
    /// The one piece of derived state this crate keeps, and it is here for a
    /// measured reason. Line numbers are only ever wanted when a message is being
    /// written for a human, so the obvious design is to count newlines on demand
    /// — which is what [`line_col`](crate::range::line_col) does, in time linear
    /// in the offset. Every inventory site, every finding and every suppression
    /// asks for one, so "on demand" turned into **O(bytes²) per file**: on a real
    /// repository whose 11 MB sat in a few large scripts, indexing took over five
    /// minutes, of which twenty-five seconds out of twenty-nine were this.
    ///
    /// Built once here, it is a binary search per question and a `Vec<u32>` per
    /// file — a few kilobytes against the megabytes of source it maps.
    ///
    /// Skipped by serde: it is reconstructible from the source in one pass, and
    /// shipping it over the wire would double a `ParsedFile` for no reader.
    #[serde(skip)]
    pub line_starts: Vec<u32>,
}

impl ParsedFile {
    /// The line-start index for a source. Ascending, always beginning with `0`,
    /// so line `n` (1-based) starts at `line_starts[n - 1]`.
    /// A parse that found nothing, for a source that could not be handed to the
    /// grammar at all. Not an error: the caller that needs one of these is the
    /// nested-body walk, and a body it cannot read is simply a body it does not
    /// report on.
    pub fn empty(source: &str, scope: picus_types::prelude::DialectScope) -> ParsedFile {
        ParsedFile {
            scope,
            source_len: source.len(),
            statements: Vec::new(),
            errors: Vec::new(),
            line_starts: ParsedFile::index_lines(source),
        }
    }

    pub fn index_lines(source: &str) -> Vec<u32> {
        let mut out = Vec::with_capacity(source.len() / 32 + 1);
        out.push(0);
        out.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| (offset + 1) as u32),
        );
        out
    }

    /// 1-based line and column (column in bytes) for a byte offset.
    ///
    /// The same answer [`line_col`](crate::range::line_col) gives, in a binary
    /// search instead of a scan from byte zero — see [`ParsedFile::line_starts`]
    /// for what that cost. Every caller that has a `ParsedFile` should use this;
    /// `line_col` remains for the ones that do not.
    ///
    /// Degrades rather than panicking on an offset past the end or on a file
    /// whose index was never built (a hand-constructed `ParsedFile` in a test):
    /// the answer is line 1, which is wrong but harmless, and a parser must never
    /// be the thing that panics.
    pub fn line_col_at(&self, offset: usize) -> (usize, usize) {
        let clamped = offset.min(self.source_len) as u32;
        // `partition_point` gives the count of starts at or before the offset,
        // which is the 1-based line number.
        let line = self.line_starts.partition_point(|start| *start <= clamped).max(1);
        let start = self.line_starts.get(line - 1).copied().unwrap_or(0);
        (line, (clamped.saturating_sub(start) + 1) as usize)
    }

    /// The 1-based line a byte offset falls on.
    pub fn line_of(&self, offset: usize) -> usize {
        self.line_col_at(offset).0
    }

    /// Statements and gaps, in source order, covering `[0, source_len)` exactly
    /// once and with no overlap.
    pub fn segments(&self) -> Vec<Segment<'_>> {
        let mut out = Vec::with_capacity(self.statements.len() * 2 + 1);
        let mut cursor = 0usize;
        for statement in &self.statements {
            if statement.range.start > cursor {
                out.push(Segment::Gap(ByteRange::new(cursor, statement.range.start)));
            }
            out.push(Segment::Statement(statement));
            cursor = statement.range.end.max(cursor);
        }
        if cursor < self.source_len {
            out.push(Segment::Gap(ByteRange::new(cursor, self.source_len)));
        }
        out
    }

    /// Rebuild the source from `source` by walking the segments. Equal to the
    /// input for every input — that is the property, and the test suite asserts
    /// it over the whole corpus.
    pub fn reassemble(&self, source: &str) -> String {
        self.segments().iter().map(|s| s.range().slice(source)).collect()
    }

    /// The statement containing `offset`, if any. Used by the editor to answer
    /// "what am I inside" without re-parsing.
    pub fn statement_at(&self, offset: usize) -> Option<&Statement> {
        self.statements.iter().find(|s| s.range.contains(offset))
    }

    /// Every foreign construct in the file, in source order.
    pub fn foreign(&self) -> impl Iterator<Item = &ForeignConstruct> {
        self.statements.iter().flat_map(|s| s.foreign.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::range::line_col;

    /// The index and the scan must give the same answer for every offset, or the
    /// speed-up is a silent renumbering of every finding in the report.
    #[test]
    fn the_line_index_agrees_with_counting_from_the_start() {
        for source in [
            "",
            "\n",
            "SELECT 1;",
            "a\nbc\n",
            "-- perché\r\nINSERT INTO T VALUES ('x');\r\n\r\nUPDATE T SET A = 1;\r\n",
            "no trailing newline\nlast line",
        ] {
            let parsed = ParsedFile {
                scope: picus_types::prelude::DialectScope::Portable,
                source_len: source.len(),
                statements: Vec::new(),
                errors: Vec::new(),
                line_starts: ParsedFile::index_lines(source),
            };
            // Past the end as well: both clamp, and callers do hand it ranges
            // from a file that was truncated under them.
            for offset in 0..=source.len() + 3 {
                assert_eq!(
                    parsed.line_col_at(offset),
                    line_col(source, offset),
                    "{source:?} at {offset}"
                );
            }
        }
    }

    #[test]
    fn a_parsed_file_with_no_index_answers_line_one_rather_than_panicking() {
        // Hand-constructed in a test, or arriving from serde, which skips the
        // index. Wrong but harmless beats a panic in a parser.
        let bare = ParsedFile {
            scope: picus_types::prelude::DialectScope::Portable,
            source_len: 100,
            statements: Vec::new(),
            errors: Vec::new(),
            line_starts: Vec::new(),
        };
        assert_eq!(bare.line_of(0), 1);
        assert_eq!(bare.line_of(99), 1);
    }

    #[test]
    fn the_index_is_one_entry_per_line() {
        assert_eq!(ParsedFile::index_lines(""), [0]);
        assert_eq!(ParsedFile::index_lines("a\nb"), [0, 2]);
        assert_eq!(ParsedFile::index_lines("a\r\nb"), [0, 3]);
        // A trailing newline opens a line that happens to be empty.
        assert_eq!(ParsedFile::index_lines("a\n"), [0, 2]);
    }
}
