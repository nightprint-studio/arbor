//! The entry point: source text + an explicit dialect → [`ParsedFile`].

use picus_types::prelude::EngineKind;
use tree_sitter::{Language, Parser};

use crate::error::{ParseError, ParseErrorKind};
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
    /// The dialect is a parameter and never a field on the parser: the same
    /// `SqlParser` parses an Oracle file and then a PostgreSQL one, and mixing
    /// them up is impossible because there is nowhere to store the confusion
    /// (`docs/picus-design.md` §1).
    ///
    /// Never returns `Err` and never panics. Syntax problems come back inside
    /// [`ParsedFile::errors`].
    pub fn parse(&mut self, source: &str, engine: EngineKind) -> ParsedFile {
        let Some(parser) = self.inner.as_mut() else {
            return failed(source, engine, "the SQL grammar could not be loaded");
        };
        let Some(tree) = parser.parse(source.as_bytes(), None) else {
            return failed(source, engine, "the parser produced no tree");
        };
        walk::walk_file(tree.root_node(), source, engine)
    }
}

/// Parse once. Convenience for a caller with a single file; prefer
/// [`SqlParser`] in a loop.
pub fn parse(source: &str, engine: EngineKind) -> ParsedFile {
    SqlParser::new().parse(source, engine)
}

fn failed(source: &str, engine: EngineKind, reason: &str) -> ParsedFile {
    ParsedFile {
        engine,
        source_len: source.len(),
        statements: Vec::new(),
        errors: vec![ParseError {
            kind: ParseErrorKind::Syntax,
            range: ByteRange::new(0, source.len()),
            parent: String::new(),
            text: reason.to_string(),
            expected: None,
        }],
    }
}
