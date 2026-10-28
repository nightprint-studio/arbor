//! What the expression to the left of a dot has ALREADY called — the chain being extended.
//!
//! `Order.builder().customer(c).|` is a builder with `customer` already set. Offering `customer`
//! again at the top of the list is correct about the type and wrong about the program: nobody sets
//! the same property twice in a chain, and the member they want is one of the others — or `build()`.
//! Two facts follow from reading the chain and need no type at all:
//!
//! * [`ChainContext::called`] — every method already called in this chain, so the ranking can sink
//!   a repeat;
//! * [`ChainContext::previous`] — the call immediately before the dot, so a project that calls
//!   `close()` right after `open()` can have that sequence count.
//!
//! ## What "this chain" means
//!
//! The text of the current statement, read forward with a frame per open parenthesis. A call
//! written as `.name(` is recorded in the frame it sits in, and its arguments get a frame of their
//! own — so for `foo(a.x().y().|` the chain is `x`, `y`, not whatever surrounds `foo`. Literals are
//! skipped, so a `".fake("` inside a string never counts.

use std::collections::HashSet;

use crate::hole::statement_before;

/// The calls already made in the chain a caret is extending.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChainContext {
    /// Every method name called with `.name(` in this chain.
    pub called: HashSet<String>,
    /// The last of them — the call the caret directly follows.
    pub previous: Option<String>,
}

#[derive(Default)]
struct Frame {
    called: HashSet<String>,
    last: Option<String>,
}

/// Read the chain that ends at `caret` (the position of the `.` being completed, or of the typed
/// prefix after it).
pub fn chain_context(source: &str, caret: usize) -> ChainContext {
    let mut caret = caret.min(source.len());
    while caret > 0 && !source.is_char_boundary(caret) {
        caret -= 1;
    }
    let before = &source[..caret];
    let statement = statement_before(before);
    let earlier = &before[..before.len() - statement.len()];
    let bytes = statement.as_bytes();

    let mut frames: Vec<Frame> = vec![Frame::default()];
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => quote = Some(b),
            b'(' => frames.push(Frame::default()),
            b')' => {
                if frames.len() > 1 {
                    frames.pop();
                }
            }
            b'.' => {
                if let Some((name, after)) = call_after_dot(statement, i + 1) {
                    if let Some(top) = frames.last_mut() {
                        top.called.insert(name.to_string());
                        top.last = Some(name.to_string());
                    }
                    // Resume AT the `(`, so it opens the argument frame like any other.
                    i = after;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let top = frames.pop().unwrap_or_default();
    // `x.open(); x.|` — a plain receiver with no calls of its own yet continues what the previous
    // statements did to the same variable.
    let previous = top.last.or_else(|| {
        let receiver = plain_receiver(statement)?;
        last_call_on(earlier, receiver)
    });
    ChainContext { called: top.called, previous }
}

/// How far back the previous-statement search reads. A variable's last use matters while it is
/// close; reading a whole file for it would make completion pay for the length of the method.
const PREVIOUS_STATEMENT_WINDOW: usize = 1500;

/// The identifier a statement ends on when it ends `receiver.` — and nothing else of a chain, so
/// `x.` qualifies and `x.open().` does not (its previous call is already known).
fn plain_receiver(statement: &str) -> Option<&str> {
    let head = statement.trim_end().strip_suffix('.')?.trim_end();
    let receiver = crate::hole::trailing_identifier(head)?;
    // `a.b.` is a field access chain; the variable whose history matters is not `b`.
    let before_receiver = head[..head.len() - receiver.len()].trim_end();
    (!before_receiver.ends_with('.')).then_some(receiver)
}

/// The method last called on `receiver` as `receiver.name(` in `earlier`, searching backwards inside
/// [`PREVIOUS_STATEMENT_WINDOW`].
fn last_call_on(earlier: &str, receiver: &str) -> Option<String> {
    let mut start = earlier.len().saturating_sub(PREVIOUS_STATEMENT_WINDOW);
    while !earlier.is_char_boundary(start) {
        start += 1;
    }
    let window = &earlier[start..];
    window.rmatch_indices(receiver).find_map(|(at, _)| {
        let standalone = window[..at]
            .chars()
            .next_back()
            .map_or(true, |c| !(c.is_alphanumeric() || c == '_' || c == '$' || c == '.'));
        if !standalone {
            return None;
        }
        let after = window[at + receiver.len()..].trim_start().strip_prefix('.')?;
        let (name, _) = call_after_dot(after, 0)?;
        Some(name.to_string())
    })
}

/// `.name(` — the identifier after the dot at `from`, when a `(` follows it. Returns the name and
/// the offset of that `(`.
fn call_after_dot(statement: &str, from: usize) -> Option<(&str, usize)> {
    let rest = &statement[from..];
    let ws = rest.len() - rest.trim_start().len();
    let rest = &rest[ws..];
    let first = rest.chars().next()?;
    if !(first.is_alphabetic() || first == '_' || first == '$') {
        return None;
    }
    let len = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .unwrap_or(rest.len());
    let name = &rest[..len];
    let after_name = &rest[len..];
    let gap = after_name.len() - after_name.trim_start().len();
    after_name[gap..].starts_with('(').then_some((name, from + ws + len + gap))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(source: &str) -> ChainContext {
        chain_context(source, source.len())
    }

    fn names(c: &ChainContext) -> Vec<&str> {
        let mut v: Vec<&str> = c.called.iter().map(String::as_str).collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn a_builder_chain_lists_what_it_has_set() {
        let c = chain("void m() { return Order.builder().customer(c).total(t).");
        assert_eq!(names(&c), ["builder", "customer", "total"]);
        assert_eq!(c.previous.as_deref(), Some("total"));
    }

    #[test]
    fn a_fresh_receiver_has_no_chain() {
        let c = chain("void m() { order.");
        assert!(c.called.is_empty());
        assert_eq!(c.previous, None);
    }

    /// Field accesses are part of the receiver, not calls.
    #[test]
    fn a_field_access_is_not_a_call() {
        let c = chain("void m() { this.items.stream().");
        assert_eq!(names(&c), ["stream"]);
    }

    #[test]
    fn an_argument_chain_is_its_own() {
        let c = chain("void m() { foo(a.x().y().");
        assert_eq!(names(&c), ["x", "y"]);
        assert_eq!(c.previous.as_deref(), Some("y"));
    }

    /// Back at the outer level the chain is the outer one again.
    #[test]
    fn a_closed_argument_does_not_leak_into_the_chain_around_it() {
        let c = chain("void m() { a.b(c.d()).e(f.g()).");
        assert_eq!(names(&c), ["b", "e"]);
        assert_eq!(c.previous.as_deref(), Some("e"));
    }

    #[test]
    fn only_the_current_statement_is_read() {
        let c = chain("void m() { list.add(x); list.");
        assert!(c.called.is_empty());
    }

    #[test]
    fn calls_inside_literals_do_not_count() {
        let c = chain("void m() { s = \"a.fake(1)\" + other.real().");
        assert_eq!(names(&c), ["real"]);
    }

    #[test]
    fn a_chain_broken_over_lines_and_spaces_is_still_one_chain() {
        let c = chain("void m() {\n  return Order.builder()\n      .customer (c)\n      .");
        assert_eq!(names(&c), ["builder", "customer"]);
    }

    #[test]
    fn a_number_with_a_dot_is_not_a_call() {
        let c = chain("void m() { double d = 1.5 * x.");
        assert!(c.called.is_empty());
    }

    #[test]
    fn a_plain_receiver_continues_what_earlier_statements_did_to_it() {
        let c = chain("void m() { x.open(); y.read(); x.");
        assert_eq!(c.previous.as_deref(), Some("open"));
        assert!(c.called.is_empty(), "nothing is called in THIS chain yet");
    }

    #[test]
    fn the_latest_call_on_the_receiver_wins() {
        let c = chain("void m() { x.open(); x.write(1); x.");
        assert_eq!(c.previous.as_deref(), Some("write"));
    }

    #[test]
    fn a_chain_with_its_own_calls_ignores_earlier_statements() {
        let c = chain("void m() { x.open(); x.flush().");
        assert_eq!(c.previous.as_deref(), Some("flush"));
    }

    #[test]
    fn a_field_path_is_not_a_plain_receiver() {
        let c = chain("void m() { x.open(); this.x.");
        assert_eq!(c.previous, None);
    }

    #[test]
    fn a_longer_name_is_not_mistaken_for_the_receiver() {
        let c = chain("void m() { xs.open(); x.");
        assert_eq!(c.previous, None);
    }

    #[test]
    fn a_receiver_never_used_before_has_no_previous_call() {
        let c = chain("void m() { y.read(); x.");
        assert_eq!(c.previous, None);
    }

    #[test]
    fn a_caret_outside_the_text_does_not_panic() {
        let c = chain_context("a.b().", 999);
        assert_eq!(names(&c), ["b"]);
        assert_eq!(chain_context("é.b().", 1), ChainContext::default());
    }
}
