//! The entry point: source text + an explicit dialect → [`ParsedFile`].

use picus_types::prelude::DialectScope;
use tree_sitter::{Language, Parser};

use crate::error::{ParseError, ParseErrorKind};
use crate::projection::Projection;
use crate::range::ByteRange;
use crate::statement::ParsedFile;
use crate::walk;

extern "C" {
    fn tree_sitter_picus_sql() -> *const ();
}

/// The compiled grammar, for a caller that wants to drive Tree-sitter directly
/// (a syntax highlighter, a query).
pub fn language() -> Language {
    unsafe { Language::from_raw(tree_sitter_picus_sql().cast()) }
}

/// A reusable parser.
///
/// Loading the language and allocating the parser is the expensive part, so a
/// caller that parses a whole folder should keep one of these and call
/// [`SqlParser::parse`] per file.
pub struct SqlParser {
    // `None` only if the grammar failed to load, which cannot happen with a
    // matching `tree-sitter` version — but "cannot happen" is not a reason to
    // panic inside a parser. The failure surfaces as a parse error instead.
    inner: Option<Parser>,
}

// `tree_sitter::Parser` is not `Debug` and the workspace lint wants one; there
// is nothing useful to print anyway.
impl std::fmt::Debug for SqlParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlParser").finish_non_exhaustive()
    }
}

impl Default for SqlParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SqlParser {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        match parser.set_language(&language()) {
            Ok(()) => Self { inner: Some(parser) },
            Err(_) => Self { inner: None },
        }
    }

    /// Parse `source` as `engine`.
    ///
    /// The scope is a parameter and never a field on the parser: the same
    /// `SqlParser` parses an Oracle file, then a PostgreSQL one, then a portable
    /// one, and mixing them up is impossible because there is nowhere to store
    /// the confusion (`docs/picus-design.md` §1).
    ///
    /// `DialectScope::Portable` is not a third grammar — it is the same permissive
    /// superset with the *acceptance* rule inverted: nothing dialect-specific is
    /// allowed, so both engines' constructs come back in `foreign`.
    ///
    /// Never returns `Err` and never panics. Syntax problems come back inside
    /// [`ParsedFile::errors`].
    pub fn parse(&mut self, source: &str, scope: DialectScope) -> ParsedFile {
        let Some(parser) = self.inner.as_mut() else {
            return failed(source, scope, "the SQL grammar could not be loaded");
        };
        let Some(tree) = parser.parse(source.as_bytes(), None) else {
            return failed(source, scope, "the parser produced no tree");
        };
        walk::walk_file(tree.root_node(), source, scope)
    }
}

/// Parse once. Convenience for a caller with a single file; prefer
/// [`SqlParser`] in a loop.
pub fn parse(source: &str, scope: DialectScope) -> ParsedFile {
    SqlParser::new().parse(source, scope)
}

/// The [`Projection`] of the first `SELECT` in `source` — what it produces and out
/// of what.
///
/// A **separate door** rather than a field on [`ParsedFile`], and that is the whole
/// design: this walks derived tables, `WITH` and both arms of every set operation,
/// which is far more than the editor's per-keystroke parse should ever pay for.
/// Only the one caller that is tracing a value back through a stack of views asks
/// for it, and it pays for itself.
///
/// `None` when `source` holds no `SELECT` — a `CREATE VIEW` body should be handed in
/// as its `SELECT`, which is what `pg_get_viewdef` returns.
pub fn project(source: &str, scope: DialectScope) -> Option<Projection> {
    let mut parser = SqlParser::new();
    let inner = parser.inner.as_mut()?;
    let tree = inner.parse(source.as_bytes(), None)?;
    // The first `select_statement` anywhere in the file, so a body wrapped in
    // parentheses or preceded by a comment still answers. Depth-first, so the
    // outermost one wins over a subquery inside it.
    let node = first_select(tree.root_node())?;
    walk::projection_of(node, source)
}

/// The outermost `select_statement` in a tree, or `None`.
fn first_select(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.kind() == "select_statement" {
        return Some(node);
    }
    let mut cursor = node.walk();
    // Collected first so the cursor is not borrowed across the recursion.
    let children: Vec<_> = node.named_children(&mut cursor).collect();
    children.into_iter().find_map(first_select)
}

fn failed(source: &str, scope: DialectScope, reason: &str) -> ParsedFile {
    ParsedFile {
        scope,
        source_len: source.len(),
        statements: Vec::new(),
        errors: vec![ParseError {
            kind: ParseErrorKind::Syntax,
            range: ByteRange::new(0, source.len()),
            parent: String::new(),
            text: reason.to_string(),
            expected: None,
        }],
        // Indexed even here: a file that failed to parse still gets its errors
        // rendered with a line number.
        line_starts: ParsedFile::index_lines(source),
    }
}
