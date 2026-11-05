//! A node's parent in constant time, for the duration of one file's checks.
//!
//! `tree_sitter::Node::parent()` is not a pointer read: the library finds the parent by descending from
//! the ROOT of the tree, so one call costs the depth of the node. Validation climbs ancestors in well
//! over a hundred places ("which method am I in", "is this name a local", "is there a `try` around
//! me"), each climb makes one call per ancestor, and a builder chain — `.a(..).b(..).c(..)…` — nests
//! one call per link. A name in the hundredth link therefore paid a hundred calls of a hundred steps
//! each, and every name in the chain paid it: the time grew with the cube of the chain.
//!
//! [`ParentTable::arm`] walks the tree once and remembers every node's parent; [`parent_of`] answers
//! from it, and from the library when no table is armed (a node of some other tree, a unit test, a
//! caller that never armed one), so it is a drop-in for `node.parent()`.
//!
//! ## Why there is one `unsafe`
//!
//! The table has to live somewhere every check can reach without threading a parameter through a
//! hundred call sites, which means a thread-local, which means `'static`. A `Node<'tree>` is a plain
//! copy of a pointer into the tree plus a marker for the borrow, so the lifetime is erased on the way in
//! and restored on the way out ([`parent_of`] hands back a node borrowed for as long as the caller's own
//! node is). That is sound exactly as long as no node outlives its tree, which [`ParentGuard`] enforces:
//! it borrows the root for `'t` (so it cannot outlive the tree it was built from) and empties the table
//! when dropped. Do not `mem::forget` it.

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;

use tree_sitter::Node;

type Table = HashMap<usize, Node<'static>>;

thread_local! {
    /// The armed table, innermost last: a guard created while another is alive (a check that parses a
    /// second tree) restores the first one when it drops.
    static TABLES: RefCell<Vec<Table>> = const { RefCell::new(Vec::new()) };
}

/// Marker for arming the parent table — see [`ParentTable::arm`].
pub struct ParentTable;

impl ParentTable {
    /// Remember the parent of every node under `root` until the returned guard is dropped.
    ///
    /// One walk, linear in the size of the tree. Bind the guard to a name that lives as long as the
    /// checks do (`let _parents = …`), and declare it after the tree so it drops first.
    pub fn arm<'t>(root: Node<'t>) -> ParentGuard<'t> {
        let mut table: Table = HashMap::new();
        let mut cursor = root.walk();
        // Iterative pre-order walk: a chain is thousands of levels deep and recursion would not survive it.
        'walk: loop {
            let node = cursor.node();
            if cursor.goto_first_child() {
                // SAFETY: lifetime erasure under `ParentGuard`'s contract (module docs).
                let parent: Node<'static> = unsafe { std::mem::transmute::<Node<'t>, Node<'static>>(node) };
                loop {
                    table.insert(cursor.node().id(), parent);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
                // Back on the first child, to go down into it next.
                cursor.goto_parent();
                cursor.goto_first_child();
                continue 'walk;
            }
            // A leaf: move on to the next sibling, climbing until one exists.
            loop {
                if cursor.goto_next_sibling() {
                    continue 'walk;
                }
                if !cursor.goto_parent() {
                    break 'walk;
                }
            }
        }
        TABLES.with(|t| t.borrow_mut().push(table));
        ParentGuard { _tree: PhantomData }
    }
}

/// Keeps the armed table alive; dropping it disarms the table. Borrows the root's lifetime so it cannot
/// be kept past the tree.
#[must_use = "the table is disarmed when the guard is dropped"]
pub struct ParentGuard<'t> {
    _tree: PhantomData<&'t ()>,
}

impl Drop for ParentGuard<'_> {
    fn drop(&mut self) {
        TABLES.with(|t| {
            t.borrow_mut().pop();
        });
    }
}

/// `node.parent()`, answered from the armed table when it knows the node.
///
/// Takes a `Node` or a `&Node` so it replaces `x.parent()` whatever `x` was. A node the table does not
/// know (the root has no parent; a node of another tree) falls back to the library, so the answer is
/// always the same as `node.parent()` — only the cost differs.
pub fn parent_of<'t>(node: impl Borrow<Node<'t>>) -> Option<Node<'t>> {
    let node: &Node<'t> = node.borrow();
    let hit = TABLES.with(|t| t.borrow().last().and_then(|table| table.get(&node.id()).copied()));
    match hit {
        // A guard against an id that belongs to another tree: the parent must enclose the node.
        Some(p) if p.start_byte() <= node.start_byte() && p.end_byte() >= node.end_byte() => Some(p),
        _ => node.parent(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "class A { int f(int x) { return g(x).h(x + 1).i(); } int g(int y) { return y; } }";

    fn all_nodes<'t>(root: Node<'t>) -> Vec<Node<'t>> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            let mut c = n.walk();
            stack.extend(n.children(&mut c));
            out.push(n);
        }
        out
    }

    #[test]
    fn the_table_answers_what_the_library_would() {
        let tree = crate::grammar::parse_java(SOURCE).expect("parses");
        let root = tree.root_node();
        let nodes = all_nodes(root);
        let _parents = ParentTable::arm(root);
        for n in &nodes {
            assert_eq!(parent_of(*n).map(|p| p.id()), n.parent().map(|p| p.id()), "{}", n.kind());
        }
        assert!(parent_of(root).is_none());
    }

    #[test]
    fn without_a_table_it_is_the_library() {
        let tree = crate::grammar::parse_java(SOURCE).expect("parses");
        let root = tree.root_node();
        for n in all_nodes(root) {
            assert_eq!(parent_of(&n).map(|p| p.id()), n.parent().map(|p| p.id()));
        }
    }

    #[test]
    fn a_second_guard_restores_the_first_when_it_drops() {
        let a = crate::grammar::parse_java(SOURCE).expect("parses");
        let b = crate::grammar::parse_java("class B {}").expect("parses");
        let _outer = ParentTable::arm(a.root_node());
        {
            let _inner = ParentTable::arm(b.root_node());
        }
        let leaf = all_nodes(a.root_node()).into_iter().find(|n| n.kind() == "identifier").expect("an identifier");
        assert_eq!(parent_of(leaf).map(|p| p.id()), leaf.parent().map(|p| p.id()));
        TABLES.with(|t| assert_eq!(t.borrow().len(), 1));
    }
}
