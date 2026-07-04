//! `np_equals` — the "flip to null-safe equals" intention.
//!
//! Rewrites `receiver.equals("literal")` into `"literal".equals(receiver)` (and the same for
//! `equalsIgnoreCase`), the classic defensive form that never NPEs when `receiver` is null. Fires
//! only when the single argument is a **string literal** and the receiver is not already one, with
//! the caret inside the call.
//!
//! Like the other intentions this is a light byte scanner: it finds the `.equals(` call, checks the
//! argument is one string literal, then walks **left** over the receiver's postfix chain
//! (`a.b().c[0]`) to find its start. Best effort on pathological receivers (a string literal that
//! itself contains an unbalanced `)`); those simply don't fire.

use crate::scan::{chain_start, is_ident_part, matching_paren, pure_string_literal};

/// The edit: replace `source[start..end]` with `replacement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqualsSwap {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// The equality methods worth flipping (both take a single `Object`/`String` arg).
const METHODS: &[&str] = &["equals", "equalsIgnoreCase"];

/// Flip the `x.equals("lit")` call under the caret to `"lit".equals(x)`. `None` when the caret
/// isn't inside such a call (arg not a lone string literal, receiver already a literal, …).
pub fn np_safe_equals(source: &str, offset: usize) -> Option<EqualsSwap> {
    let b = source.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'.' {
            let name_start = i + 1;
            let mut j = name_start;
            while j < b.len() && is_ident_part(b[j]) {
                j += 1;
            }
            let name = &source[name_start..j];
            if METHODS.contains(&name) {
                // Skip whitespace to the `(`.
                let mut k = j;
                while k < b.len() && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < b.len() && b[k] == b'(' {
                    if let Some(close) = matching_paren(b, k) {
                        let arg = source[k + 1..close].trim();
                        if pure_string_literal(arg).is_some() {
                            if let Some(rstart) = chain_start(b, i) {
                                let receiver = source[rstart..i].trim();
                                if !receiver.is_empty()
                                    && !receiver.starts_with('"')
                                    && offset >= rstart
                                    && offset <= close
                                {
                                    return Some(EqualsSwap {
                                        start: rstart,
                                        end: close + 1,
                                        replacement: format!("{arg}.{name}({receiver})"),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply at a caret sitting on the string-literal argument (unique across these snippets).
    fn apply(src: &str) -> Option<String> {
        let off = src.find('"').map(|i| i + 1)?; // inside the first string literal
        let rw = np_safe_equals(src, off)?;
        Some(format!("{}{}{}", &src[..rw.start], rw.replacement, &src[rw.end..]))
    }

    #[test]
    fn simple_variable_receiver() {
        assert_eq!(apply(r#"s.equals("x")"#).unwrap(), r#""x".equals(s)"#);
    }

    #[test]
    fn field_chain_receiver() {
        assert_eq!(apply(r#"this.name.equals("x")"#).unwrap(), r#""x".equals(this.name)"#);
    }

    #[test]
    fn method_call_receiver() {
        assert_eq!(
            apply(r#"user.getName().equals("hello")"#).unwrap(),
            r#""hello".equals(user.getName())"#
        );
    }

    #[test]
    fn indexed_receiver() {
        assert_eq!(apply(r#"arr[0].equals("x")"#).unwrap(), r#""x".equals(arr[0])"#);
    }

    #[test]
    fn equals_ignore_case() {
        assert_eq!(apply(r#"s.equalsIgnoreCase("Yes")"#).unwrap(), r#""Yes".equalsIgnoreCase(s)"#);
    }

    #[test]
    fn inside_an_if_condition() {
        let src = r#"if (status.equals("OPEN")) {"#;
        let off = src.find("\"OPEN\"").unwrap() + 1;
        let rw = np_safe_equals(src, off).unwrap();
        let out = format!("{}{}{}", &src[..rw.start], rw.replacement, &src[rw.end..]);
        assert_eq!(out, r#"if ("OPEN".equals(status)) {"#);
    }

    #[test]
    fn receiver_already_a_literal_is_skipped() {
        assert_eq!(np_safe_equals(r#""x".equals(s)"#, 12), None);
    }

    #[test]
    fn non_literal_argument_is_skipped() {
        assert_eq!(np_safe_equals(r#"a.equals(b)"#, 3), None);
    }

    #[test]
    fn concatenated_argument_is_skipped() {
        // Arg isn't a *single* string literal.
        assert_eq!(np_safe_equals(r#"a.equals("x" + y)"#, 10), None);
    }

    #[test]
    fn other_method_name_is_skipped() {
        assert_eq!(np_safe_equals(r#"a.equalsish("x")"#, 13), None);
    }

    #[test]
    fn caret_outside_the_call_returns_none() {
        let src = r#"int z = 1; s.equals("x")"#;
        assert_eq!(np_safe_equals(src, 0), None);
    }

    #[test]
    fn escaped_quote_in_literal_preserved() {
        assert_eq!(apply(r#"s.equals("a\"b")"#).unwrap(), r#""a\"b".equals(s)"#);
    }

    #[test]
    fn picks_the_inner_call_at_caret() {
        // Outer arg is not a literal; the inner call is the one to flip.
        let src = r#"a.equals(b.equalsIgnoreCase("x"))"#;
        let off = src.find("\"x\"").unwrap() + 1;
        let rw = np_safe_equals(src, off).unwrap();
        let out = format!("{}{}{}", &src[..rw.start], rw.replacement, &src[rw.end..]);
        assert_eq!(out, r#"a.equals("x".equalsIgnoreCase(b))"#);
    }
}
