//! `log_param` — the "parameterize a logging call" quick-fix transform.
//!
//! Rewrites a SLF4J/Log4j/JUL logging call whose message is built by **string concatenation**
//! into the parameterized form the logging APIs prefer:
//!
//! ```text
//! logger.info("user " + id + " logged in")   →   logger.info("user {} logged in", id)
//! logger.error("failed for " + name, e)       →   logger.error("failed for {}", name, e)
//! ```
//!
//! This is pure, tree-sitter-free text surgery: a small brace/string/comment-aware scanner finds
//! the logging call enclosing the caret, splits its first argument on top-level `+`, folds the
//! string-literal pieces into one format string (with `{}` where a non-literal expression was) and
//! moves those expressions into the argument list — keeping any trailing argument (a `Throwable`)
//! last. Returns the byte range of the argument list to replace + the replacement text, so the
//! caller (the Alt+Enter intention) applies it as a single edit.
//!
//! Deliberately conservative — it only fires when the method name is a known logging level, the
//! receiver is a method call (`x.info(`) and there is at least one **non-literal** operand to
//! parameterize (so it never touches a plain `logger.info("literal")`). Literal `{}` inside a
//! message is left as-is (SLF4J escaping is a rare legacy case); the transform is offered as a
//! caret intention, never applied automatically.

use crate::scan::{
    block_comment_end, is_ident_part, is_ident_start, line_comment_end, matching_paren,
    pure_string_literal, string_end,
};

/// Known logging-level method names. Requiring one of these (plus a `.` receiver) is the gate that
/// keeps the transform from firing on arbitrary `foo("a" + b)` calls.
const LOG_LEVELS: &[&str] = &[
    "trace", "debug", "info", "warn", "warning", "error", "fatal", "severe",
];

/// The edit a [`parameterize_log_call`] produces: replace `source[start..end]` (the argument-list
/// text **between** the parentheses, parens excluded) with `replacement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogParamRewrite {
    /// Byte offset of the first character of the argument list (just after `(`).
    pub start: usize,
    /// Byte offset just past the last character of the argument list (the `)` position).
    pub end: usize,
    /// The rewritten argument list (format literal + parameterized args + trailing args).
    pub replacement: String,
}

/// Try to build a parameterized-logging rewrite for the logging call enclosing byte `offset`.
/// Returns `None` when the caret isn't inside a qualifying call (see the module docs for the gate).
pub fn parameterize_log_call(source: &str, offset: usize) -> Option<LogParamRewrite> {
    let b = source.as_bytes();
    // Find every `\.<level>\s*\(` call and pick the one whose span contains the caret.
    let mut i = 0usize;
    while i < b.len() {
        // Start of an identifier?
        if is_ident_start(b[i]) && (i == 0 || !is_ident_part(b[i - 1])) {
            let name_start = i;
            let mut j = i + 1;
            while j < b.len() && is_ident_part(b[j]) {
                j += 1;
            }
            let name = &source[name_start..j];
            if is_level(name) && preceded_by_dot(b, name_start) {
                // Skip whitespace to the `(`.
                let mut k = j;
                while k < b.len() && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < b.len() && b[k] == b'(' {
                    if let Some(close) = matching_paren(b, k) {
                        // Caret must sit within [receiver-dot start .. close].
                        if offset >= name_start && offset <= close {
                            if let Some(rw) = rewrite_call(source, k + 1, close) {
                                return Some(rw);
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

/// Build the rewrite for the argument list `source[args_start..args_end]` (between the parens).
fn rewrite_call(source: &str, args_start: usize, args_end: usize) -> Option<LogParamRewrite> {
    let args_text = &source[args_start..args_end];
    let args = split_top_level(args_text, b',', false);
    let first = args.first()?.trim();
    if first.is_empty() {
        return None;
    }
    // The first argument must be a `+` concatenation with at least one string literal.
    let operands = split_top_level(first, b'+', true);
    if operands.len() < 2 {
        return None;
    }

    let mut fmt = String::new();
    let mut extracted: Vec<String> = Vec::new();
    let mut saw_literal = false;
    for op in &operands {
        let op = op.trim();
        if let Some(content) = pure_string_literal(op) {
            fmt.push_str(content);
            saw_literal = true;
        } else {
            fmt.push_str("{}");
            extracted.push(op.to_string());
        }
    }
    // Only offer the fix when it actually parameterizes something (≥1 literal AND ≥1 expression).
    if !saw_literal || extracted.is_empty() {
        return None;
    }

    let mut new_args: Vec<String> = Vec::with_capacity(1 + extracted.len() + args.len());
    new_args.push(format!("\"{fmt}\""));
    new_args.extend(extracted);
    // Keep any trailing original arguments (typically a Throwable) after the parameterized ones.
    for a in args.iter().skip(1) {
        let a = a.trim();
        if !a.is_empty() {
            new_args.push(a.to_string());
        }
    }

    Some(LogParamRewrite {
        start: args_start,
        end: args_end,
        replacement: new_args.join(", "),
    })
}

/// Split `s` at top-level occurrences of `sep` (depth 0, outside strings/chars/comments). Returns
/// the raw segments (not trimmed). When `plus` is set, `sep` is `+` and `++` sequences are skipped
/// so an increment inside an operand doesn't split it.
fn split_top_level(s: &str, sep: u8, plus: bool) -> Vec<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut seg_start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                i = string_end(b, i);
                continue;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                i = line_comment_end(b, i);
                continue;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                i = block_comment_end(b, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            c if c == sep && depth == 0 => {
                // For `+`, skip `++` / `+=` (and the second half of a `++`).
                if plus
                    && ((i + 1 < b.len() && b[i + 1] == b'+')
                        || (i > 0 && b[i - 1] == b'+')
                        || (i + 1 < b.len() && b[i + 1] == b'='))
                {
                    i += 1;
                    continue;
                }
                out.push(s[seg_start..i].to_string());
                seg_start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(s[seg_start..].to_string());
    out
}

/// True when the identifier at `start` is immediately preceded (skipping spaces) by a `.` — i.e.
/// it's a method call on a receiver (`logger.info`), not a bare name.
fn preceded_by_dot(b: &[u8], start: usize) -> bool {
    let mut i = start;
    while i > 0 {
        i -= 1;
        if b[i].is_ascii_whitespace() {
            continue;
        }
        return b[i] == b'.';
    }
    false
}

fn is_level(name: &str) -> bool {
    LOG_LEVELS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply the rewrite to `src` and return the resulting source (for readable assertions).
    fn apply(src: &str) -> Option<String> {
        // Caret inside the first argument (just after the first `(`).
        let off = src.find('(').map(|i| i + 1).unwrap_or(0);
        let rw = parameterize_log_call(src, off)?;
        let mut out = String::new();
        out.push_str(&src[..rw.start]);
        out.push_str(&rw.replacement);
        out.push_str(&src[rw.end..]);
        Some(out)
    }

    #[test]
    fn single_suffix_var() {
        let src = r#"logger.info("user: " + id)"#;
        assert_eq!(apply(src).unwrap(), r#"logger.info("user: {}", id)"#);
    }

    #[test]
    fn prefix_and_suffix() {
        let src = r#"log.info("a=" + a + " b=" + b)"#;
        assert_eq!(apply(src).unwrap(), r#"log.info("a={} b={}", a, b)"#);
    }

    #[test]
    fn keeps_trailing_throwable() {
        let src = r#"log.error("failed " + name, e)"#;
        assert_eq!(apply(src).unwrap(), r#"log.error("failed {}", name, e)"#);
    }

    #[test]
    fn method_call_operand_with_inner_comma() {
        let src = r#"log.debug("v=" + fmt(a, b))"#;
        assert_eq!(apply(src).unwrap(), r#"log.debug("v={}", fmt(a, b))"#);
    }

    #[test]
    fn expression_first_then_literal() {
        let src = r#"log.warn(name + " not found")"#;
        assert_eq!(apply(src).unwrap(), r#"log.warn("{} not found", name)"#);
    }

    #[test]
    fn escaped_quote_in_literal_preserved() {
        let src = r#"log.info("say \"hi\" to " + name)"#;
        assert_eq!(apply(src).unwrap(), r#"log.info("say \"hi\" to {}", name)"#);
    }

    #[test]
    fn uppercase_logger_receiver() {
        let src = r#"LOG.warn("x=" + x)"#;
        assert_eq!(apply(src).unwrap(), r#"LOG.warn("x={}", x)"#);
    }

    #[test]
    fn not_a_logging_call() {
        assert_eq!(parameterize_log_call(r#"foo.bar("x" + y)"#, 8), None);
    }

    #[test]
    fn plain_literal_message_untouched() {
        assert_eq!(parameterize_log_call(r#"log.info("hello world")"#, 10), None);
    }

    #[test]
    fn no_string_literal_operand() {
        assert_eq!(parameterize_log_call(r#"log.info(a + b)"#, 10), None);
    }

    #[test]
    fn all_literals_no_expression() {
        // Concatenated literals but nothing to parameterize → not offered.
        assert_eq!(parameterize_log_call(r#"log.info("a" + "b")"#, 10), None);
    }

    #[test]
    fn caret_outside_call_returns_none() {
        let src = r#"int z = 1; log.info("u " + id)"#;
        // Caret at the very start (offset 0), outside the logging call span.
        assert_eq!(parameterize_log_call(src, 0), None);
    }

    #[test]
    fn caret_inside_call_span_found() {
        let src = r#"int z = 1; log.info("u " + id)"#;
        let off = src.find("id").unwrap();
        let rw = parameterize_log_call(src, off).expect("should resolve at caret on the arg");
        let out = format!("{}{}{}", &src[..rw.start], rw.replacement, &src[rw.end..]);
        assert_eq!(out, r#"int z = 1; log.info("u {}", id)"#);
    }

    #[test]
    fn picks_the_call_the_caret_is_in() {
        // Two logging calls; the caret is inside the SECOND. Only that one is rewritten.
        let src = r#"log.info("a=" + a); log.info("b=" + b);"#;
        let off = src.rfind('b').unwrap();
        let rw = parameterize_log_call(src, off).unwrap();
        let out = format!("{}{}{}", &src[..rw.start], rw.replacement, &src[rw.end..]);
        assert_eq!(out, r#"log.info("a=" + a); log.info("b={}", b);"#);
    }

    #[test]
    fn concatenation_with_trailing_literal_only_between_exprs() {
        let src = r#"log.info("start " + a + b + " end")"#;
        // `a` and `b` are both expressions → two placeholders back to back.
        assert_eq!(apply(src).unwrap(), r#"log.info("start {}{} end", a, b)"#);
    }
}
