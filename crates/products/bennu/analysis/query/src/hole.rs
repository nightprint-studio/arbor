//! The NAME of the thing a caret is filling — `customer` in `Customer customer = |`, `total` in
//! `this.total = |`, `findByCustomerId` in `repo.findByCustomerId(|)`.
//!
//! The expected TYPE of a hole ([`bennu_java::prelude::expected_type`]) answers "what can be
//! written here"; this answers "what is it being written FOR". A parameter's own name is the best
//! hint there is, and an argument slot cannot give it without overload resolution (see the notes in
//! `expected.rs`) — but the *callee's* name can be read from the text, and `findByCustomerId` says
//! `customerId` as well as a signature would.
//!
//! ## Text, not a parse
//!
//! The buffer around a caret is by definition half-written, and a parse of it is exactly what
//! recovers worst. What is needed here is one identifier out of the current statement, so this reads
//! the statement's text — bounded by the previous `;`, `{` or `}`, skipping string and char
//! literals — and keeps two facts: the innermost unclosed `(` (the call whose argument is being
//! written), and the first assignment at the statement's own level.
//!
//! The innermost call wins over the assignment around it: in `Customer c = repo.find(|)` the hole is
//! the argument of `find`, and the `c` belongs to a bigger expression.

/// Words that open a parenthesis without calling anything.
const NOT_CALLS: &[&str] = &[
    "if", "while", "for", "switch", "catch", "synchronized", "return", "try", "assert", "throw",
];

/// The name the hole at `caret` is being filled for, or `None` when it is not being filled for
/// anything named (a bare statement, a `return`, a grouping parenthesis).
pub fn hole_name(source: &str, caret: usize) -> Option<String> {
    let mut caret = caret.min(source.len());
    while caret > 0 && !source.is_char_boundary(caret) {
        caret -= 1;
    }
    let statement = statement_before(&source[..caret]);
    let scan = scan_statement(statement);

    if let Some(open) = scan.innermost_call {
        let head = statement[..open].trim_end();
        match trailing_identifier(head) {
            Some(name) if NOT_CALLS.contains(&name) => return None,
            Some(name) => return Some(name.to_string()),
            // `x = (cu|` — a grouping parenthesis; the assignment around it still applies.
            None => {}
        }
    }
    let eq = scan.assignment?;
    let target = statement[..eq].trim_end_matches(|c: char| "+-*/%&|^<>".contains(c)).trim_end();
    trailing_identifier(target).map(str::to_string)
}

/// The text of the statement the caret is in: everything after the last `;`, `{` or `}`.
pub(crate) fn statement_before(before: &str) -> &str {
    let start = before.rfind(|c| c == ';' || c == '{' || c == '}').map_or(0, |i| i + 1);
    &before[start..]
}

/// What one pass over a statement found.
struct StatementScan {
    /// Byte offset of the innermost `(` still open at the end of the text.
    innermost_call: Option<usize>,
    /// Byte offset of the first `=` at the statement's own level that is an assignment (not `==`,
    /// `!=`, `<=`, `>=`).
    assignment: Option<usize>,
}

fn scan_statement(statement: &str) -> StatementScan {
    let bytes = statement.as_bytes();
    let mut open: Vec<(usize, u8)> = Vec::new();
    let mut assignment = None;
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
            b'(' | b'[' => open.push((i, b)),
            b')' | b']' => {
                open.pop();
            }
            b'=' if open.is_empty() && assignment.is_none() => {
                let previous = if i > 0 { bytes[i - 1] } else { 0 };
                let next = bytes.get(i + 1).copied().unwrap_or(0);
                if next == b'=' {
                    i += 1; // `==` is one operator
                } else if !matches!(previous, b'=' | b'!' | b'<' | b'>') {
                    assignment = Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    let innermost_call = open.iter().rev().find(|(_, kind)| *kind == b'(').map(|(at, _)| *at);
    StatementScan { innermost_call, assignment }
}

/// The identifier that ends `text`, if one does: `Customer customer` → `customer`, `this.total` →
/// `total`, `arr[0]` → `None`.
pub(crate) fn trailing_identifier(text: &str) -> Option<&str> {
    let start = text
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .map_or(0, |i| i + text[i..].chars().next().map_or(1, char::len_utf8));
    let name = &text[start..];
    let first = name.chars().next()?;
    (first.is_alphabetic() || first == '_' || first == '$').then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_end(source: &str) -> Option<String> {
        hole_name(source, source.len())
    }

    #[test]
    fn a_declaration_names_its_variable() {
        assert_eq!(at_end("void m() { Customer customer = cu").as_deref(), Some("customer"));
        assert_eq!(at_end("void m() { final List<Order> orders = ").as_deref(), Some("orders"));
    }

    #[test]
    fn an_assignment_names_its_target_through_this_and_compound_operators() {
        assert_eq!(at_end("void m() { this.total = ").as_deref(), Some("total"));
        assert_eq!(at_end("void m() { count += ").as_deref(), Some("count"));
    }

    #[test]
    fn a_comparison_is_not_an_assignment() {
        assert_eq!(at_end("void m() { boolean b = a == "), Some("b".to_string()));
        assert_eq!(at_end("void m() { x == "), None);
        assert_eq!(at_end("void m() { x <= "), None);
        assert_eq!(at_end("void m() { x != "), None);
    }

    #[test]
    fn an_argument_is_named_by_its_callee() {
        assert_eq!(at_end("void m() { repo.findByCustomerId(cu").as_deref(), Some("findByCustomerId"));
        assert_eq!(at_end("void m() { new Order(").as_deref(), Some("Order"));
    }

    /// `Customer c = repo.find(|)`: the hole is `find`'s argument, not `c`'s value.
    #[test]
    fn the_innermost_call_wins_over_the_assignment_around_it() {
        assert_eq!(at_end("void m() { Customer c = repo.find(").as_deref(), Some("find"));
        // Once the call is closed, the assignment is what is being filled again.
        assert_eq!(at_end("void m() { Customer c = repo.find(id) + ").as_deref(), Some("c"));
    }

    #[test]
    fn control_flow_parentheses_name_nothing() {
        assert_eq!(at_end("void m() { if (x"), None);
        assert_eq!(at_end("void m() { while ("), None);
        assert_eq!(at_end("void m() { for (int i = 0; "), None);
        assert_eq!(at_end("void m() { return "), None);
    }

    #[test]
    fn a_grouping_parenthesis_defers_to_the_assignment() {
        assert_eq!(at_end("void m() { total = (").as_deref(), Some("total"));
    }

    #[test]
    fn only_the_current_statement_counts() {
        assert_eq!(at_end("void m() { int a = 1; foo(").as_deref(), Some("foo"));
        assert_eq!(at_end("void m() { int a = 1; bar").as_deref(), None);
        assert_eq!(at_end("void m() { int a = 1; }  x = ").as_deref(), Some("x"));
    }

    #[test]
    fn string_and_char_literals_are_skipped() {
        // The `(` and `=` inside the literal must not be taken for the call or the assignment.
        assert_eq!(at_end("void m() { foo(\"a=b(\", ").as_deref(), Some("foo"));
        assert_eq!(at_end("void m() { bar('=', ").as_deref(), Some("bar"));
        assert_eq!(at_end("void m() { String t = \"say \\\"=\\\" (\" + ").as_deref(), Some("t"));
    }

    #[test]
    fn an_array_element_target_names_nothing() {
        assert_eq!(at_end("void m() { arr[0] = "), None);
    }

    #[test]
    fn the_caret_in_the_middle_reads_only_what_is_before_it() {
        let source = "void m() { Customer customer = x; other = y; }";
        let caret = source.find(" x;").unwrap() + 1;
        assert_eq!(hole_name(source, caret).as_deref(), Some("customer"));
    }

    #[test]
    fn a_caret_past_the_end_or_inside_a_char_does_not_panic() {
        assert_eq!(hole_name("a = é", 1000).as_deref(), Some("a"));
        assert_eq!(hole_name("é = ", 1), None);
    }
}
