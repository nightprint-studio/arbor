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
}

impl ParsedFile {
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
