//! `simplify` — small boolean/collection **simplification** intentions.
//!
//!   * [`simplify_size_check`]   — `x.size() == 0` / `x.length() == 0` → `x.isEmpty()`
//!                                 (`!= 0` / `> 0` → `!x.isEmpty()`).
//!   * [`simplify_boolean_compare`] — `flag == true` → `flag`, `flag == false` → `!flag`
//!                                    (and the `!=` mirrors).
//!   * [`simplify_negated_comparison`] — `!(a == b)` → `a != b`, `!(a != b)` → `a == b`.
//!
//! All caret-anchored, string/comment-aware byte scanners over the operand's postfix chain (via
//! [`chain_start`](crate::scan::chain_start)); each returns an [`Edit`] or `None`.

use crate::scan::{chain_start, is_ident_part, matching_paren, string_end};
use crate::Edit;

/// Methods whose `== 0` check means "empty" → `isEmpty()`. Both require empty `()` (so an array's
/// field `arr.length` — no parens, no `isEmpty()` — is never touched).
const SIZE_METHODS: &[&str] = &["length", "size"];

/// `recv.size()/length() == 0` → `recv.isEmpty()` (`!= 0` / `> 0` → `!recv.isEmpty()`).
pub fn simplify_size_check(source: &str, offset: usize) -> Option<Edit> {
    let b = source.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'.' {
            let name_start = i + 1;
            let mut j = name_start;
            while j < b.len() && is_ident_part(b[j]) {
                j += 1;
            }
            if SIZE_METHODS.contains(&&source[name_start..j]) {
                let mut k = j;
                while k < b.len() && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < b.len() && b[k] == b'(' {
                    if let Some(close) = matching_paren(b, k) {
                        if source[k + 1..close].trim().is_empty() {
                            if let Some((neg, end)) = parse_zero_compare(source, close + 1) {
                                if let Some(rstart) = chain_start(b, i) {
                                    let recv = source[rstart..i].trim();
                                    if !recv.is_empty() && offset >= rstart && offset <= end {
                                        let repl = if neg {
                                            format!("!{recv}.isEmpty()")
                                        } else {
                                            format!("{recv}.isEmpty()")
                                        };
                                        return Some(Edit { start: rstart, end, replacement: repl });
                                    }
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

/// Parse a ` == 0 | != 0 | > 0 ` comparison at `from`. Returns `(negate, end_byte)` where `negate`
/// means the `isEmpty()` should be `!`-negated (`!= 0` / `> 0`) and `end_byte` is just past the `0`.
fn parse_zero_compare(source: &str, from: usize) -> Option<(bool, usize)> {
    let b = source.as_bytes();
    let mut k = from;
    while k < b.len() && b[k].is_ascii_whitespace() {
        k += 1;
    }
    let (neg, oplen) = if source[k..].starts_with("==") {
        (false, 2)
    } else if source[k..].starts_with("!=") {
        (true, 2)
    } else if k < b.len() && b[k] == b'>' && !(k + 1 < b.len() && b[k + 1] == b'=') {
        (true, 1)
    } else {
        return None;
    };
    k += oplen;
    while k < b.len() && b[k].is_ascii_whitespace() {
        k += 1;
    }
    // A bare `0` token (not `00`, `0x…`, `0.5`, `0L`).
    if k < b.len() && b[k] == b'0' {
        let after = k + 1;
        let bounded = after >= b.len()
            || !(b[after].is_ascii_alphanumeric() || b[after] == b'.' || b[after] == b'_');
        if bounded {
            return Some((neg, after));
        }
    }
    None
}

/// `operand == true` → `operand`; `== false` → `!operand`; `!= true` → `!operand`; `!= false` →
/// `operand`. The boolean literal must be on the RIGHT; the operand is the postfix chain to its
/// left (with an optional leading `!`).
pub fn simplify_boolean_compare(source: &str, offset: usize) -> Option<Edit> {
    let b = source.as_bytes();
    let mut i = 0usize;
    while i + 1 < b.len() {
        let is_eq = b[i] == b'=' && b[i + 1] == b'=';
        let is_ne = b[i] == b'!' && b[i + 1] == b'=';
        if is_eq || is_ne {
            let mut k = i + 2;
            while k < b.len() && b[k].is_ascii_whitespace() {
                k += 1;
            }
            let lit = if word_at(source, k, "true") {
                Some((true, k + 4))
            } else if word_at(source, k, "false") {
                Some((false, k + 5))
            } else {
                None
            };
            if let Some((lit_true, lit_end)) = lit {
                if let Some(ostart) = chain_start(b, i) {
                    let operand = source[ostart..i].trim();
                    if !operand.is_empty() && offset >= ostart && offset <= lit_end {
                        // `== true` / `!= false` keep; `== false` / `!= true` negate.
                        let keep = (is_eq && lit_true) || (is_ne && !lit_true);
                        let repl = if keep { operand.to_string() } else { negate(operand) };
                        return Some(Edit { start: ostart, end: lit_end, replacement: repl });
                    }
                }
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    None
}

/// `!(a == b)` → `a != b`; `!(a != b)` → `a == b`. Fires only when the parenthesized body is a
/// single top-level equality comparison (no `&&`/`||`/relational).
pub fn simplify_negated_comparison(source: &str, offset: usize) -> Option<Edit> {
    let b = source.as_bytes();
    let mut i = 0usize;
    while i + 1 < b.len() {
        if b[i] == b'!' && b[i + 1] == b'(' {
            if let Some(close) = matching_paren(b, i + 1) {
                let inside = &source[i + 2..close];
                if let Some((a, op, rhs)) = single_top_comparison(inside) {
                    if offset >= i && offset <= close {
                        let flipped = if op == "==" { "!=" } else { "==" };
                        return Some(Edit {
                            start: i,
                            end: close + 1,
                            replacement: format!("{a} {flipped} {rhs}"),
                        });
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// Negate a boolean operand: strip a leading `!`, else prepend one (`!x`→`x`, `x`→`!x`).
fn negate(op: &str) -> String {
    match op.strip_prefix('!') {
        Some(rest) => rest.trim_start().to_string(),
        None => format!("!{op}"),
    }
}

/// True when `w` sits at byte `k` in `source` as a whole word (not followed by an identifier char).
fn word_at(source: &str, k: usize, w: &str) -> bool {
    source[k..].starts_with(w) && {
        let after = k + w.len();
        after >= source.len() || !is_ident_part(source.as_bytes()[after])
    }
}

/// If `s` is a single top-level `==`/`!=` comparison (no `&&`/`||`/relational/2nd comparison),
/// return `(lhs, op, rhs)` trimmed. `None` otherwise.
fn single_top_comparison(s: &str) -> Option<(&str, &'static str, &str)> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut found: Option<(usize, &'static str)> = None;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                i = string_end(b, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ if depth == 0 => {
                if b[i] == b'=' && i + 1 < b.len() && b[i + 1] == b'=' {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((i, "=="));
                    i += 2;
                    continue;
                }
                if b[i] == b'!' && i + 1 < b.len() && b[i + 1] == b'=' {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((i, "!="));
                    i += 2;
                    continue;
                }
                // Logical or relational operators at top level → not a lone equality; bail.
                if (b[i] == b'&' && i + 1 < b.len() && b[i + 1] == b'&')
                    || (b[i] == b'|' && i + 1 < b.len() && b[i + 1] == b'|')
                    || b[i] == b'<'
                    || b[i] == b'>'
                {
                    return None;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let (pos, op) = found?;
    let lhs = s[..pos].trim();
    let rhs = s[pos + 2..].trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    Some((lhs, op, rhs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(f: fn(&str, usize) -> Option<Edit>, src: &str, needle: &str) -> Option<String> {
        let off = src.find(needle)? + needle.len() / 2;
        let e = f(src, off)?;
        Some(format!("{}{}{}", &src[..e.start], e.replacement, &src[e.end..]))
    }

    // ── size / length == 0 → isEmpty ──
    #[test]
    fn size_zero_to_isempty() {
        assert_eq!(apply(simplify_size_check, "if (list.size() == 0) {", "size").unwrap(), "if (list.isEmpty()) {");
    }
    #[test]
    fn length_zero_to_isempty() {
        assert_eq!(apply(simplify_size_check, "if (s.length() == 0) {", "length").unwrap(), "if (s.isEmpty()) {");
    }
    #[test]
    fn size_not_zero_negates() {
        assert_eq!(apply(simplify_size_check, "if (list.size() != 0) {", "size").unwrap(), "if (!list.isEmpty()) {");
    }
    #[test]
    fn size_gt_zero_negates() {
        assert_eq!(apply(simplify_size_check, "if (list.size() > 0) {", "size").unwrap(), "if (!list.isEmpty()) {");
    }
    #[test]
    fn size_ge_zero_not_touched() {
        // `>= 0` is always true — not our fix.
        assert_eq!(simplify_size_check("list.size() >= 0", 5), None);
    }
    #[test]
    fn chained_receiver_size() {
        assert_eq!(apply(simplify_size_check, "if (a.b().items().size() == 0) x;", "items").unwrap(), "if (a.b().items().isEmpty()) x;");
    }
    #[test]
    fn array_length_field_not_touched() {
        // `arr.length` (no parens) is an array field, not a method → skipped.
        assert_eq!(simplify_size_check("arr.length == 0", 5), None);
    }
    #[test]
    fn size_compared_to_nonzero_not_touched() {
        assert_eq!(simplify_size_check("list.size() == 3", 6), None);
    }

    // ── boolean literal comparison ──
    #[test]
    fn eq_true_drops_it() {
        assert_eq!(apply(simplify_boolean_compare, "if (flag == true) {", "==").unwrap(), "if (flag) {");
    }
    #[test]
    fn eq_false_negates() {
        assert_eq!(apply(simplify_boolean_compare, "if (flag == false) {", "==").unwrap(), "if (!flag) {");
    }
    #[test]
    fn ne_true_negates() {
        assert_eq!(apply(simplify_boolean_compare, "if (flag != true) {", "!=").unwrap(), "if (!flag) {");
    }
    #[test]
    fn ne_false_drops_it() {
        assert_eq!(apply(simplify_boolean_compare, "if (flag != false) {", "!=").unwrap(), "if (flag) {");
    }
    #[test]
    fn method_call_operand() {
        assert_eq!(apply(simplify_boolean_compare, "if (user.isActive() == true) {", "==").unwrap(), "if (user.isActive()) {");
    }
    #[test]
    fn negated_operand_eq_false_double_negates_away() {
        // `!x == false` ≡ x
        assert_eq!(apply(simplify_boolean_compare, "if (!x == false) {", "==").unwrap(), "if (x) {");
    }
    #[test]
    fn true_word_boundary() {
        // `trueValue` is not the literal `true`.
        assert_eq!(simplify_boolean_compare("flag == trueValue", 5), None);
    }

    // ── negated comparison ──
    #[test]
    fn not_eq_to_ne() {
        assert_eq!(apply(simplify_negated_comparison, "if (!(a == b)) {", "!(").unwrap(), "if (a != b) {");
    }
    #[test]
    fn not_ne_to_eq() {
        assert_eq!(apply(simplify_negated_comparison, "while (!(x != y)) {", "!(").unwrap(), "while (x == y) {");
    }
    #[test]
    fn not_with_calls_and_fields() {
        assert_eq!(apply(simplify_negated_comparison, "if (!(a.getX() == b.y)) {", "!(").unwrap(), "if (a.getX() != b.y) {");
    }
    #[test]
    fn not_of_logical_and_bails() {
        assert_eq!(simplify_negated_comparison("!(a == b && c == d)", 1), None);
    }
    #[test]
    fn not_of_relational_bails() {
        assert_eq!(simplify_negated_comparison("!(a < b)", 1), None);
    }
    #[test]
    fn not_of_non_comparison_bails() {
        assert_eq!(simplify_negated_comparison("!(flag)", 1), None);
    }
    #[test]
    fn comparison_with_string_containing_operator() {
        // The `)` inside the string mustn't confuse paren matching, nor a `==` inside a string.
        assert_eq!(apply(simplify_negated_comparison, r#"if (!(s == ")")) {"#, "!(").unwrap(), r#"if (s != ")") {"#);
    }
}
