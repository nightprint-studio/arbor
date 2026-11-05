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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tree_sitter::{Language, Parser, Tree};

/// The Java grammar this workspace is built against.
///
/// Cheap: `LANGUAGE` is a static, and `into()` wraps a pointer. Callers that parse in a loop
/// should still hold a `Parser` rather than re-setting the language per file — that part is
/// not free.
pub fn language() -> Language {
    tree_sitter_java::LANGUAGE.into()
}

/// How many distinct sources the process keeps parsed.
///
/// One is not enough, and that is the whole reason this is a number. A single query alternates
/// between buffers — the file the caret is in, then the file that declares what it resolved to,
/// then back — and a one-entry cache misses on every switch, re-parsing both each time. Eight
/// covers that interleaving across the few buffers an editing session touches at once.
const PARSE_CACHE_SIZE: usize = 8;

/// Upper bound on the summed source length of the cached entries (a tree is a small multiple of
/// its source). A single huge file may still be cached alone; it just evicts everything else.
const PARSE_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

/// One cached parse: `(content hash, byte length, tree)`.
type CacheEntry = (u64, usize, Tree);

/// Process-wide parse cache, newest first.
///
/// Process-wide rather than per-thread because the IPC dispatch pool hands each request to an
/// arbitrary idle worker: with a per-thread cache, completion, hover, go-to and validation of the
/// same buffer mostly landed on different threads and re-parsed. `Tree` is `Send + Sync` and its
/// `clone` is a cheap reference-count bump, so every caller gets its own handle — `Tree::edit` on a
/// clone never touches the cached one. The lock is held only for the scan and the clone, never
/// across a parse.
static PARSE_CACHE: Mutex<Vec<CacheEntry>> = Mutex::new(Vec::new());

static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

/// `(hits, misses)` of [`parse_java`] since process start — for `ARBOR_BE_PERF`-style diagnostics.
pub fn parse_cache_stats() -> (u64, u64) {
    (CACHE_HITS.load(Ordering::Relaxed), CACHE_MISSES.load(Ordering::Relaxed))
}

fn cache_lookup(key: u64, len: usize) -> Option<Tree> {
    // A poisoned lock only means another thread panicked mid-scan; the Vec is still consistent.
    let cache = PARSE_CACHE.lock().unwrap_or_else(|p| p.into_inner());
    cache
        .iter()
        .find(|(k, l, _)| *k == key && *l == len)
        .map(|(_, _, tree)| tree.clone())
}

fn cache_insert(cache: &mut Vec<CacheEntry>, key: u64, len: usize, tree: Tree) {
    // A concurrent miss on the same text may have inserted it already — keep one copy.
    cache.retain(|(k, l, _)| !(*k == key && *l == len));
    // Newest first, oldest dropped: the buffer just asked about is the one most likely to be
    // asked about again.
    cache.insert(0, (key, len, tree));
    cache.truncate(PARSE_CACHE_SIZE);
    let mut total = 0usize;
    let mut keep = cache.len();
    for (i, (_, l, _)) in cache.iter().enumerate() {
        total += *l;
        if total > PARSE_CACHE_MAX_BYTES && i > 0 {
            keep = i;
            break;
        }
    }
    cache.truncate(keep);
}

thread_local! {
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
    if let Some(tree) = cache_lookup(key, len) {
        CACHE_HITS.fetch_add(1, Ordering::Relaxed);
        return Some(tree);
    }
    CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
    let tree = with_parser(|parser| {
        let mut tree = parser.parse(source, None)?;
        // A failed parse is not always the source's fault. Retry once on a masked copy — see
        // `mask_varargs_annotations` — and keep the retry ONLY if it comes back clean, so a file
        // with a genuine syntax error still reports it.
        if tree.root_node().has_error() {
            let masks = [mask_varargs_annotations(source), mask_nul_in_literals(source)];
            for masked in masks.into_iter().flatten() {
                if let Some(retry) = parser.parse(&masked, None) {
                    if !retry.root_node().has_error() {
                        tree = retry;
                        break;
                    }
                }
            }
        }
        Some(tree)
    })?;
    {
        let mut cache = PARSE_CACHE.lock().unwrap_or_else(|p| p.into_inner());
        cache_insert(&mut cache, key, len, tree.clone());
    }
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

/// Blank the NUL bytes that sit inside a string / char literal or a comment.
///
/// javac takes a `\0` there as any other character (OSS-Fuzz inputs are written that way), but
/// tree-sitter's scanner reads it as the end of the input, and the whole file comes back as one
/// `ERROR`. Same length, so every offset stays valid; a NUL anywhere else is left alone, because
/// there it is a genuine illegal character. `None` when there is nothing to mask.
fn mask_nul_in_literals(source: &str) -> Option<String> {
    if !source.contains('\0') {
        return None;
    }
    let b = source.as_bytes();
    let mut out = b.to_vec();
    let mut masked = false;
    let mut blank = |from: usize, to: usize, out: &mut Vec<u8>| {
        for byte in &mut out[from..to.min(b.len())] {
            if *byte == 0 {
                *byte = b' ';
                masked = true;
            }
        }
    };
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let start = i;
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                blank(start, i, &mut out);
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let start = i;
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(b.len());
                blank(start, i, &mut out);
            }
            b'"' | b'\'' => {
                let end = skip_literal(b, i);
                blank(i, end, &mut out);
                i = end;
            }
            _ => i += 1,
        }
    }
    if !masked {
        return None;
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
    /// OSS-Fuzz inputs put NUL bytes inside string literals; javac accepts them, tree-sitter's scanner
    /// reads one as the end of the input.
    #[test]
    fn a_nul_inside_a_string_literal_or_comment_still_parses() {
        let inside_string = "class A { String s = \"a\0b\"; }";
        let inside_comment = "class A { /* a\0b */ int x; }";
        for source in [inside_string, inside_comment] {
            let tree = parse_java(source).expect("a parse");
            assert!(!tree.root_node().has_error(), "{source:?}");
        }
    }

    #[test]
    fn a_nul_outside_a_literal_is_still_an_error() {
        let tree = parse_java("class A { int \0x; }").expect("a parse");
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn the_cache_is_shared_across_threads_and_counts_hits() {
        // Unique text so parallel tests cannot have cached it already.
        let src = "class SharedAcrossThreadsProbe { int unique_probe_field_91137; }";
        let first = parse_java(src).expect("a parse");
        let (h0, _) = parse_cache_stats();
        let second = std::thread::spawn(move || parse_java(src).expect("a parse"))
            .join()
            .unwrap();
        let (h1, _) = parse_cache_stats();
        assert!(h1 > h0, "the other thread must hit the shared cache");
        assert_eq!(first.root_node().to_sexp(), second.root_node().to_sexp());
    }

    #[test]
    fn editing_a_returned_tree_does_not_touch_the_cached_one() {
        let src = "class EditIsolationProbe { int a_unique_field_44021; }";
        let mut mine = parse_java(src).expect("a parse");
        let before = mine.root_node().end_byte();
        mine.edit(&tree_sitter::InputEdit {
            start_byte: 0,
            old_end_byte: 0,
            new_end_byte: 4,
            start_position: tree_sitter::Point::new(0, 0),
            old_end_position: tree_sitter::Point::new(0, 0),
            new_end_position: tree_sitter::Point::new(0, 4),
        });
        let again = parse_java(src).expect("a parse");
        assert_eq!(again.root_node().end_byte(), before);
    }

    #[test]
    fn the_cache_is_bounded_by_slot_count() {
        let mut cache: Vec<CacheEntry> = Vec::new();
        let tree = parse_java("class Bound {}").expect("a parse");
        let total = PARSE_CACHE_SIZE as u64 * 3;
        for i in 0..total {
            cache_insert(&mut cache, i, 10, tree.clone());
        }
        assert_eq!(cache.len(), PARSE_CACHE_SIZE);
        // Re-inserting a known key keeps one copy, newest first.
        cache_insert(&mut cache, total - 2, 10, tree.clone());
        assert_eq!(cache.len(), PARSE_CACHE_SIZE);
        assert_eq!(cache[0].0, total - 2);
    }

    #[test]
    fn the_cache_is_bounded_by_total_bytes() {
        let mut cache: Vec<CacheEntry> = Vec::new();
        let tree = parse_java("class Bytes {}").expect("a parse");
        let big = PARSE_CACHE_MAX_BYTES / 2 + 1;
        cache_insert(&mut cache, 1, big, tree.clone());
        cache_insert(&mut cache, 2, big, tree.clone());
        assert_eq!(cache.len(), 1, "two entries over the byte budget cannot both stay");
        assert_eq!(cache[0].0, 2);
    }

    #[test]
    fn the_grammar_is_one_this_runtime_can_use() {
        let mut parser = tree_sitter::Parser::new();
        assert!(parser.set_language(&language()).is_ok());
        let tree = parser.parse("class A {}", None).expect("a parse");
        assert_eq!(tree.root_node().kind(), "program");
        assert!(!tree.root_node().has_error());
    }
}
