//! Which source lines a diagnostic stands for.
//!
//! javac and Bennu agree on the statement but not always on the line inside it: for a call spread
//! over several lines javac points at the `.` before the method name, while an argument check may
//! point at the argument three lines down. Matching on the bare line would score that one mistake as
//! a false positive AND a miss. So a Bennu diagnostic claims the whole line range of its innermost
//! enclosing simple statement or declaration; on a single-line statement that is just its own line.

use tree_sitter::{Parser, Tree};

/// Nodes whose line range a diagnostic inside them claims.
const STATEMENTS: &[&str] = &[
    "expression_statement",
    "local_variable_declaration",
    "return_statement",
    "throw_statement",
    "yield_statement",
    "field_declaration",
    "explicit_constructor_invocation",
    "import_declaration",
];

/// Nodes the upward search stops at: past them, a statement would claim far more than one call.
const BOUNDARIES: &[&str] = &[
    "block",
    "program",
    "class_body",
    "interface_body",
    "enum_body",
    "annotation_type_body",
    "constructor_body",
    "switch_block",
];

/// A parsed source, answering "which lines does this offset stand for".
pub struct Anchors {
    tree: Option<Tree>,
    line_starts: Vec<usize>,
}

impl Anchors {
    pub fn new(source: &str) -> Self {
        let mut parser = Parser::new();
        let tree = parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .ok()
            .and_then(|()| parser.parse(source, None));
        let mut line_starts = vec![0];
        line_starts.extend(source.match_indices('\n').map(|(i, _)| i + 1));
        Self { tree, line_starts }
    }

    /// The 1-based line holding `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(index) => index + 1,
            Err(index) => index,
        }
    }

    /// The 1-based, inclusive line range a diagnostic starting at `offset` stands for.
    pub fn statement_lines(&self, offset: usize) -> (usize, usize) {
        let own = self.line_of(offset);
        let Some(tree) = &self.tree else { return (own, own) };
        let mut node = tree.root_node().descendant_for_byte_range(offset, offset);
        while let Some(current) = node {
            if STATEMENTS.contains(&current.kind()) {
                return (current.start_position().row + 1, current.end_position().row + 1);
            }
            if BOUNDARIES.contains(&current.kind()) {
                break;
            }
            node = current.parent();
        }
        (own, own)
    }
}
