//! The Java grammar itself, for callers that parse rather than ask.
//!
//! Everything else in this crate answers a *question* about Java source — what it declares,
//! what a name resolves to, where a span is. A syntax-tree view asks nothing: it wants the
//! grammar and walks the parse for itself.
//!
//! It lives here rather than being a `tree-sitter-java` dependency of every such caller,
//! because the grammar's version is a workspace-wide fact. `tree-sitter` is a `links` native
//! library: exactly one version can be linked, and the ABI shim `tree-sitter-java` is built
//! against must match it. A second crate picking its own is not a different choice, it is a
//! build failure — so the pin lives in one Cargo.toml and the rest ask for it here.

use std::cell::RefCell;

use tree_sitter::{Language, Parser, Tree};

/// The Java grammar this workspace is built against.
///
/// Cheap: `LANGUAGE` is a static, and `into()` wraps a pointer. Callers that parse in a loop
/// should still hold a `Parser` rather than re-setting the language per file — that part is
/// not free.
pub fn language() -> Language {
    tree_sitter_java::LANGUAGE.into()
}

/// How many distinct sources a thread keeps parsed.
///
/// One is not enough, and that is the whole reason this is a number. A single query alternates
/// between buffers — the file the caret is in, then the file that declares what it resolved to,
/// then back — and a one-entry cache misses on every switch, re-parsing both each time. Four
/// covers that interleaving with room to spare while holding only a handful of trees per thread.
const PARSE_CACHE_SIZE: usize = 4;

thread_local! {
    /// Recently parsed sources on this thread: `(content hash, byte length, tree)`.
    static PARSE_CACHE: RefCell<Vec<(u64, usize, Tree)>> = const { RefCell::new(Vec::new()) };
}

/// Parse `source` as Java, reusing the previous tree when it is the same text.
///
/// ## Why a cache at all
///
/// Almost nothing in this crate parses once and keeps the tree: each question — what does this
/// declare, where is this name written, what is the caret on — parses for itself, which is the
/// right shape for a single query and the wrong one for a batch. A bulk naming fix asks four or
/// five of those questions **per violation**, and a file with twenty violations was parsed eighty
/// times instead of once. That is the dominant cost of planning a project-wide fix.
///
/// The cache is small and bounded on purpose: it exists to collapse the repeated questions asked
/// about the same few buffers, not to remember the project. See [`PARSE_CACHE_SIZE`].
///
/// Keyed by a content hash rather than the string's address: an address is only unique while the
/// buffer lives, and a freed one reused by a different file of the same length would serve the
/// wrong tree — a wrong answer to save a hash of a few microseconds.
pub fn parse_java(source: &str) -> Option<Tree> {
    let key = source_fingerprint(source);
    let len = source.len();
    let hit = PARSE_CACHE.with(|cell| {
        cell.borrow()
            .iter()
            .find(|(k, l, _)| *k == key && *l == len)
            .map(|(_, _, tree)| tree.clone())
    });
    if let Some(tree) = hit {
        return Some(tree);
    }
    let mut parser = Parser::new();
    parser.set_language(&language()).ok()?;
    let tree = parser.parse(source, None)?;
    PARSE_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        // Newest first, oldest dropped: the buffer just asked about is the one most likely to be
        // asked about again.
        cache.insert(0, (key, len, tree.clone()));
        cache.truncate(PARSE_CACHE_SIZE);
    });
    Some(tree)
}

/// FNV-1a over the source bytes. Local rather than shared: this is a hash function, not a concept,
/// and the crates that own the other one sit above this in the dependency order.
fn source_fingerprint(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one thing worth asserting: a `Parser` accepts it. A grammar built against a
    /// different ABI than the runtime fails exactly here, and nowhere earlier.
    #[test]
    fn the_grammar_is_one_this_runtime_can_use() {
        let mut parser = tree_sitter::Parser::new();
        assert!(parser.set_language(&language()).is_ok());
        let tree = parser.parse("class A {}", None).expect("a parse");
        assert_eq!(tree.root_node().kind(), "program");
        assert!(!tree.root_node().has_error());
    }
}
