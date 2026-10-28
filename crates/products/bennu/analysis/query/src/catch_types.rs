//! The exceptions a `try` body can throw — offered first when a `catch` names its type.
//!
//! `catch (|` is a place where the right answer is not a matter of taste: the block above says what
//! it throws. A `throw new IllegalStateException(..)` throws that, and a call to a method declared
//! `throws SQLException` throws that. The type-name index knows none of this and offers every class
//! on the classpath that starts with the letters typed; with nothing typed it offers nothing, since
//! type names need a letter.
//!
//! ## What is read, and what is not
//!
//! The text of the `try` block directly above (walking back over earlier `catch` clauses, so the
//! second `catch` still sees the same body), read as text for the two shapes that name an exception
//! outright:
//!
//! * `throw new Name(` — the simple name, resolved through the file's imports;
//! * `name(` / `.name(` calls — the overload the call binds to ([`call_overload_at`]), whose
//!   `throws` clause is the answer. A call that does not bind to exactly one overload says nothing:
//!   guessing which overload throws would offer an exception the code cannot throw.
//!
//! Not read: a `throw e` of a variable, the constructors a `new` calls, the `close()` of a
//! try-with-resources, the exceptions of a lambda body. Each would need more of the program than
//! this reads, and the cost of leaving them out is a place in the list, not an answer.
//!
//! A hard cap on resolved calls keeps this inside the budget of one keystroke on a long body: each
//! call parses the buffer to bind its overload.

use std::collections::HashSet;

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_complete::prelude::{MatchCase, Typed};
use bennu_java::prelude::{call_overload_at, extract_symbols};
use bennu_proto::prelude::CompletionItem;

use crate::completion::split_prefix;
use crate::resolver::IndexResolver;

/// How many calls of the body are resolved. The rest are not read; see the module docs.
const MAX_RESOLVED_CALLS: usize = 8;
/// How many earlier `catch` clauses are walked back over to find the `try`.
const MAX_CATCH_CLAUSES: usize = 6;

/// Words that put a `(` after them without calling anything.
const NOT_CALLS: &[&str] = &[
    "if", "for", "while", "switch", "catch", "synchronized", "return", "new", "throw", "try", "assert",
    "super", "this",
];

/// The exception classes the `try` above the `catch` at `caret` can throw, as type-name completions
/// — the first preselected, each carrying its import. Empty unless the caret is where a `catch`
/// names its exception type, and empty when the user has switched the context-aware candidates off.
pub fn caught_exception_types<M: CpMemberIndex>(
    source: &str,
    caret: usize,
    resolver: &IndexResolver<M>,
    case: MatchCase,
) -> Vec<CompletionItem> {
    if !crate::tuning::current().context_filters {
        return Vec::new();
    }
    let mut caret = caret.min(source.len());
    while caret > 0 && !source.is_char_boundary(caret) {
        caret -= 1;
    }
    let (word_start, prefix) = split_prefix(source, caret);
    if prefix.starts_with(|c: char| !c.is_uppercase()) {
        return Vec::new();
    }
    let Some(site) = catch_site(&source[..word_start]) else {
        return Vec::new();
    };
    let Some(body) = try_body(source, site.catch_at) else {
        return Vec::new();
    };

    let imports = extract_symbols(source).imports;
    let mut thrown: Vec<String> = throw_new_names(&source[body.0..body.1])
        .iter()
        .filter_map(|name| bennu_java::prelude::TypeResolver::resolve_simple_name(resolver, name, &imports))
        .collect();
    // The buffer with the parameter list closed, so the call sites parse as calls. Nothing before the
    // caret moves, which is what keeps the offsets found in `source` valid in it.
    let sited = format!("{}x) {{}}{}", &source[..caret], &source[caret..]);
    for at in call_name_offsets(&source[body.0..body.1]).into_iter().take(MAX_RESOLVED_CALLS) {
        if let Some(member) = call_overload_at(&sited, body.0 + at, resolver) {
            thrown.extend(member.throws);
        }
    }

    let typed = Typed::new(&prefix, case);
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<CompletionItem> = Vec::new();
    for binary in thrown {
        let simple = binary.rsplit(['/', '$']).next().unwrap_or(&binary).to_string();
        if site.already.contains(&simple) || !typed.matches(&simple) || !seen.insert(binary.clone()) {
            continue;
        }
        let fqn = binary.replace(['/', '$'], ".");
        let package = fqn.rsplit_once('.').map_or("", |(p, _)| p);
        out.push(CompletionItem {
            label: simple,
            kind: "class".to_string(),
            detail: Some(fqn.clone()),
            auto_import: (package != "java.lang").then(|| fqn.clone()),
            preselect: out.is_empty(),
            owner: Some(binary.clone()),
            ..Default::default()
        });
    }
    out
}

/// Where a `catch`'s exception type is being written: the byte offset of the `catch` keyword, and the
/// simple names a multi-catch has already listed (`catch (IOException | |` must not offer
/// `IOException` again).
#[derive(Debug, PartialEq, Eq)]
struct CatchSite {
    catch_at: usize,
    already: Vec<String>,
}

/// `head` is everything before the word being typed. Recognises `catch (`, `catch (final ` and the
/// `|` of a multi-catch, and nothing once a type and a name have been written.
fn catch_site(head: &str) -> Option<CatchSite> {
    let catch_at = head.rfind("catch")?;
    // `mycatch (` and `catches (` are not the keyword.
    let before_ok = head[..catch_at].chars().next_back().map_or(true, |c| !(c.is_alphanumeric() || c == '_'));
    let inside = head[catch_at + "catch".len()..].trim_start().strip_prefix('(')?;
    if !before_ok || inside.contains(')') || inside.contains('{') {
        return None;
    }
    let inside = inside.trim_end();
    let inside = inside.strip_suffix("final").map_or(inside, str::trim_end);
    // Either nothing yet, or the `|` that asks for the next alternative.
    if !(inside.is_empty() || inside.ends_with('|')) {
        return None;
    }
    let already = inside
        .split('|')
        .map(|t| t.trim().rsplit('.').next().unwrap_or("").to_string())
        .filter(|t| !t.is_empty())
        .collect();
    Some(CatchSite { catch_at, already })
}

/// The byte range of the body of the `try` that the `catch` at `catch_at` belongs to — found by
/// matching braces backwards, then stepping over earlier `catch` clauses until a `try` is reached.
/// `None` when what precedes is anything else (a `finally`, a bare block).
fn try_body(source: &str, catch_at: usize) -> Option<(usize, usize)> {
    let mut end_search = catch_at;
    for _ in 0..=MAX_CATCH_CLAUSES {
        let block_end = source[..end_search].trim_end();
        if !block_end.ends_with('}') {
            return None;
        }
        let close = block_end.len() - 1;
        let open = matching_open_brace(block_end, close)?;
        let owner = source[..open].trim_end();
        match owner.chars().next_back()? {
            ')' => {
                let paren = matching_open_paren(owner, owner.len() - 1)?;
                let keyword = source[..paren].trim_end();
                if keyword.ends_with("try") {
                    return Some((open + 1, close));
                }
                if keyword.ends_with("catch") {
                    end_search = keyword.len() - "catch".len();
                    continue;
                }
                return None;
            }
            _ if owner.ends_with("try") => return Some((open + 1, close)),
            _ => return None,
        }
    }
    None
}

/// The `{` that closes at `close` in `text`. Braces inside string literals are not told apart — a
/// `"}"` in a `try` body would shift the match, and the result is a missing leader, not a wrong one.
fn matching_open_brace(text: &str, close: usize) -> Option<usize> {
    matching_open(text, close, b'{', b'}')
}

fn matching_open_paren(text: &str, close: usize) -> Option<usize> {
    matching_open(text, close, b'(', b')')
}

fn matching_open(text: &str, close: usize, open: u8, shut: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    for i in (0..=close).rev() {
        match bytes[i] {
            b if b == shut => depth += 1,
            b if b == open => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// The simple names after `throw new` in `body`, in order.
fn throw_new_names(body: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (at, _) in body.match_indices("throw") {
        let standalone = body[..at].chars().next_back().map_or(true, |c| !(c.is_alphanumeric() || c == '_'));
        let Some(rest) = body[at + "throw".len()..].trim_start().strip_prefix("new") else {
            continue;
        };
        if !standalone || !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let name: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
            .collect();
        // A qualified name (`java.io.IOException`) is kept by its last segment: the import census
        // and the file's imports are keyed by simple name.
        if let Some(simple) = name.rsplit('.').next().filter(|s| !s.is_empty()) {
            names.push(simple.to_string());
        }
    }
    names
}

/// The offsets (within `body`) of every identifier followed by `(` that is a method call — not a
/// keyword's parenthesis, not a `new` constructor — in source order.
fn call_name_offsets(body: &str) -> Vec<usize> {
    let bytes = body.as_bytes();
    let mut offsets = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' || bytes[i] == b'$') {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$') {
            i += 1;
        }
        let word = &body[start..i];
        let calls = body[i..].trim_start().starts_with('(');
        let constructs = body[..start].trim_end().ends_with("new");
        if calls && !constructs && !NOT_CALLS.contains(&word) {
            offsets.push(start);
        }
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_catch_is_recognised_before_its_type_is_written() {
        let site = catch_site("try { run(); } catch (").expect("a catch site");
        assert_eq!(site.catch_at, "try { run(); } ".len());
        assert!(site.already.is_empty());
        assert!(catch_site("try { run(); } catch (final ").is_some());
        assert!(catch_site("try { run(); } catch(").is_some());
    }

    #[test]
    fn a_multi_catch_remembers_the_alternatives_it_already_lists() {
        let site = catch_site("try { } catch (IOException | java.sql.SQLException | ").expect("a catch site");
        assert_eq!(site.already, ["IOException", "SQLException"]);
    }

    #[test]
    fn once_a_type_and_a_name_are_written_it_is_no_longer_the_type_position() {
        assert_eq!(catch_site("try { } catch (IOException "), None);
        assert_eq!(catch_site("try { } catch (IOException e) "), None);
        assert_eq!(catch_site("try { } catch (IOException e) { "), None);
    }

    #[test]
    fn the_word_catch_inside_a_longer_name_is_not_the_keyword() {
        assert_eq!(catch_site("void mycatch ("), None);
        assert_eq!(catch_site("void nothing ("), None);
    }

    #[test]
    fn the_body_of_the_try_is_found_by_matching_braces() {
        let src = "void m() { try { a(); if (x) { b(); } } catch (";
        let catch_at = src.rfind("catch").unwrap();
        let (from, to) = try_body(src, catch_at).expect("a try body");
        assert_eq!(&src[from..to], " a(); if (x) { b(); } ");
    }

    #[test]
    fn an_earlier_catch_is_stepped_over_to_reach_the_same_try() {
        let src = "try { a(); } catch (IOException e) { log(e); } catch (";
        let catch_at = src.rfind("catch").unwrap();
        let (from, to) = try_body(src, catch_at).expect("a try body");
        assert_eq!(&src[from..to], " a(); ");
    }

    #[test]
    fn try_with_resources_is_a_try() {
        let src = "try (Reader r = open()) { r.read(); } catch (";
        let (from, to) = try_body(src, src.rfind("catch").unwrap()).expect("a try body");
        assert_eq!(&src[from..to], " r.read(); ");
    }

    #[test]
    fn a_block_that_is_not_a_try_is_refused() {
        let bare = "{ a(); } catch (";
        assert_eq!(try_body(bare, bare.rfind("catch").unwrap()), None);
        let finally = "try { a(); } finally { b(); } catch (";
        assert_eq!(try_body(finally, finally.rfind("catch").unwrap()), None);
        assert_eq!(try_body("catch (", 0), None);
    }

    #[test]
    fn throw_new_names_are_read_by_their_last_segment() {
        let body = "if (x) throw new IllegalStateException(\"no\"); throw  new java.io.IOException(); throw e; rethrow new Foo();";
        assert_eq!(throw_new_names(body), ["IllegalStateException", "IOException"]);
    }

    #[test]
    fn only_method_calls_are_call_sites() {
        let body = "if (ok) { repo.save(x); helper(1); new Foo(2); while (go()) { } return build(); }";
        let names: Vec<&str> = call_name_offsets(body)
            .into_iter()
            .map(|at| {
                let end = body[at..].find(|c: char| !(c.is_alphanumeric() || c == '_')).map_or(body.len(), |e| at + e);
                &body[at..end]
            })
            .collect();
        assert_eq!(names, ["save", "helper", "go", "build"]);
    }

    #[test]
    fn an_empty_body_has_nothing_to_read() {
        assert!(throw_new_names("").is_empty());
        assert!(call_name_offsets("").is_empty());
    }
}
