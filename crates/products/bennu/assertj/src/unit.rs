//! One Java buffer, parsed once and read by every check.
//!
//! The tree and the import facts travel together because every question here needs both: *which*
//! call this is comes from the tree, *whose* method it is comes from the imports. Handing them over
//! as a pair keeps a check from parsing the file a second time to answer the other half.

use bennu_facts::prelude::{scan_java, JavaFacts};
use bennu_java::prelude::{node_text, parse_java};
use tree_sitter::{Node, Tree};

use crate::syntax::descendants;

pub(crate) struct Unit<'s> {
    pub source: &'s str,
    pub facts: JavaFacts,
    tree: Tree,
}

impl<'s> Unit<'s> {
    /// `None` only when the grammar cannot be loaded — a file with syntax errors still parses.
    pub fn parse(path: &str, source: &'s str) -> Option<Self> {
        let tree = parse_java(source)?;
        let facts = scan_java(path, source)?;
        Some(Self { source, facts, tree })
    }

    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// Every named node of the file, in source order.
    pub fn nodes(&self) -> Vec<Node<'_>> {
        descendants(self.root())
    }

    pub fn text(&self, node: Node<'_>) -> &'s str {
        node_text(&node, self.source)
    }

    /// The node's text with its whitespace removed — how a written type is compared, so that
    /// `List< String >` and `List<String>` are the same answer.
    pub fn compact(&self, node: Node<'_>) -> String {
        self.text(node).chars().filter(|c| !c.is_whitespace()).collect()
    }
}
