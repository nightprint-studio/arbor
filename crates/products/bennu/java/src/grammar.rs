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
    /// One `Parser` per thread, reused.
    ///
    /// Creating one and setting its language is not free, and a caller that walks a whole project
    /// pays it per file. That cost is exactly why batch callers used to keep a parser of their own
    /// and so bypassed this module entirely — taking with them the cache, the single grammar pin,
    /// and the recovery of a construct the grammar cannot parse. Holding the parser here removes
    /// the reason to go around.
    static PARSER: RefCell<Option<Parser>> = const { RefCell::new(None) };
}

/// Run `f` with this thread's parser. `None` when the grammar cannot be loaded at all.
fn with_parser<T>(f: impl FnOnce(&mut Parser) -> Option<T>) -> Option<T> {
    PARSER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let mut p = Parser::new();
            p.set_language(&language()).ok()?;
            *slot = Some(p);
        }
        f(slot.as_mut()?)
    })
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
    let tree = with_parser(|parser| {
        let mut tree = parser.parse(source, None)?;
        // A failed parse is not always the source's fault. Retry once on a masked copy — see
        // `mask_varargs_annotations` — and keep the retry ONLY if it comes back clean, so a file
        // with a genuine syntax error still reports it.
        if tree.root_node().has_error() {
            if let Some(masked) = mask_varargs_annotations(source) {
                if let Some(retry) = parser.parse(&masked, None) {
                    if !retry.root_node().has_error() {
                        tree = retry;
                    }
                }
            }
        }
        Some(tree)
    })?;
    PARSE_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        // Newest first, oldest dropped: the buffer just asked about is the one most likely to be
        // asked about again.
        cache.insert(0, (key, len, tree.clone()));
        cache.truncate(PARSE_CACHE_SIZE);
    });
    Some(tree)
}

/// Blank the annotations that sit on a VARARGS marker — the `@Nullable` of
/// `Object @Nullable ... args` — leaving every other byte, and every line, exactly where it was.
///
/// A **type-use** annotation there is legal Java (JSR-308) and the one position tree-sitter-java
/// 0.23 cannot parse; of eighteen type-use positions tested it is the only failure, so this is a
/// single upstream bug rather than a gap worth forking a grammar over. Guava writes it in eight
/// files, `Preconditions` and `Objects` among them.
///
/// Masking rather than merely suppressing the error is the point: the parameter is then parsed like
/// any other, so it has a type, a name, and a binding. Reporting nothing would have kept the file
/// quiet and kept it blind.
///
/// Spaces of the same length keep every byte offset valid — the entire product addresses source by
/// offset — and newlines are preserved so line numbers do not move either. `@` inside a comment or
/// a string literal is not an annotation and is skipped. `None` when there is nothing to mask.
fn mask_varargs_annotations(source: &str) -> Option<String> {
    let b = source.as_bytes();
    let mut annotations: Vec<(usize, usize)> = Vec::new();
    let mut varargs: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(b.len());
            }
            b'"' | b'\'' => i = skip_literal(b, i),
            b'@' => match annotation_end(b, i) {
                Some(end) => {
                    annotations.push((i, end));
                    i = end;
                }
                None => i += 1,
            },
            b'.' if b[i..].starts_with(b"...") => {
                varargs.push(i);
                i += 3;
            }
            _ => i += 1,
        }
    }

    // Every annotation that reaches a `...` across nothing but whitespace, however many there are.
    let mut masked: Vec<(usize, usize)> = Vec::new();
    for &dots in &varargs {
        let mut boundary = dots;
        loop {
            let mut j = boundary;
            while j > 0 && b[j - 1].is_ascii_whitespace() {
                j -= 1;
            }
            match annotations.iter().rev().find(|(_, e)| *e == j) {
                Some(&(start, end)) => {
                    masked.push((start, end));
                    boundary = start;
                }
                None => break,
            }
        }
    }
    if masked.is_empty() {
        return None;
    }
    let mut out = b.to_vec();
    for (start, end) in masked {
        for byte in &mut out[start..end] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(out).ok()
}

/// The byte just past a string / char literal starting at `i` (a text block included).
fn skip_literal(b: &[u8], i: usize) -> usize {
    let quote = b[i];
    if quote == b'"' && b[i..].starts_with(b"\"\"\"") {
        let mut j = i + 3;
        while j + 2 < b.len() && !b[j..].starts_with(b"\"\"\"") {
            j += if b[j] == b'\\' { 2 } else { 1 };
        }
        return (j + 3).min(b.len());
    }
    let mut j = i + 1;
    while j < b.len() && b[j] != quote {
        j += if b[j] == b'\\' { 2 } else { 1 };
    }
    (j + 1).min(b.len())
}

/// The byte just past the annotation starting at the `@` at `i`, or `None` if that `@` does not
/// begin one. Accepts a qualified name and an optional balanced argument list.
fn annotation_end(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < b.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    let name_start = j;
    while j < b.len() && (is_ident_byte(b[j]) || (b[j] == b'.' && j > name_start)) {
        j += 1;
    }
    if j == name_start {
        return None;
    }
    let after_name = j;
    while j < b.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    if b.get(j) != Some(&b'(') {
        return Some(after_name);
    }
    let mut depth = 0usize;
    while j < b.len() {
        match b[j] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            b'"' | b'\'' => {
                j = skip_literal(b, j);
                continue;
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn is_ident_byte(x: u8) -> bool {
    x.is_ascii_alphanumeric() || x == b'_' || x == b'$'
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
