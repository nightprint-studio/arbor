//! Literals javac rejects in its lexer although they parse: a number that does not fit its type, a
//! floating-point literal out of range, an escape sequence Java does not have.
//!
//! tree-sitter reads all of these as perfectly good literals — it has no idea what an `int` can hold
//! — so none of them reaches the syntax-error pass. javac reports them as syntax errors
//! (`int.number.too.large`, `fp.number.too.large`, `fp.number.too.small`, `illegal.esc.char`), and so
//! do we, under the same code.
//!
//! Only what can be decided from the text alone, so there is no way to be wrong about it:
//!
//! * an `int` / `long` literal is range-checked in its own radix. Decimal is the one subtle case:
//!   `2147483648` is out of range **except** directly under a unary minus, where it is `Integer.MIN_VALUE`;
//! * a decimal floating-point literal that rounds to infinity is too large, and one that rounds to zero
//!   although a non-zero digit was written is too small;
//! * a hexadecimal floating-point literal is left alone (rare, and its range rules differ);
//! * a text block is left alone: it accepts `\<newline>`, and the escape rules there are javac's to enforce.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;

/// The out-of-range and illegal-escape literals among `nodes`.
pub fn literal_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        let Ok(text) = n.utf8_text(bytes) else { continue };
        match n.kind() {
            "decimal_integer_literal" => integer(&mut out, n, text, 10),
            "hex_integer_literal" => integer(&mut out, n, text, 16),
            "octal_integer_literal" => integer(&mut out, n, text, 8),
            "binary_integer_literal" => integer(&mut out, n, text, 2),
            "decimal_floating_point_literal" => floating(&mut out, n, text),
            "string_literal" | "character_literal" => escapes(&mut out, n, text),
            _ => {}
        }
    }
    out
}

/// An `int` or `long` literal that does not fit.
fn integer(out: &mut Vec<Diagnostic>, n: Node, text: &str, radix: u32) {
    let is_long = text.ends_with(['l', 'L']);
    let digits: String = text
        .trim_end_matches(['l', 'L'])
        .chars()
        .filter(|c| *c != '_')
        .collect();
    let digits = match radix {
        16 | 2 => digits.get(2..).unwrap_or(""),
        8 => digits.trim_start_matches('0'),
        _ => digits.as_str(),
    };
    if digits.is_empty() {
        return; // `0`, `00`, or `0x` — the last is the parser's to report
    }
    let Ok(value) = u128::from_str_radix(digits, radix) else {
        out.push(too_large(n, is_long));
        return;
    };
    let fits = if radix == 10 {
        // Decimal literals are signed: one more value fits when a minus sits directly in front.
        let limit: u128 = if is_long { i64::MAX as u128 } else { i32::MAX as u128 };
        value <= limit || (value == limit + 1 && under_minus(n))
    } else {
        // The other radixes spell the bit pattern, so every bit of the type is usable.
        let limit: u128 = if is_long { u64::MAX as u128 } else { u32::MAX as u128 };
        value <= limit
    };
    if !fits {
        out.push(too_large(n, is_long));
    }
}

fn too_large(n: Node, is_long: bool) -> Diagnostic {
    let ty = if is_long { "long" } else { "int" };
    CheckId::SyntaxError.at(n, format!("Integer number too large for `{ty}`"))
}

/// Whether the literal is the operand of a unary minus.
fn under_minus(n: Node) -> bool {
    n.parent().is_some_and(|p| {
        p.kind() == "unary_expression" && p.child(0).is_some_and(|op| op.kind() == "-")
    })
}

/// A decimal floating-point literal that rounds to infinity or to zero.
fn floating(out: &mut Vec<Diagnostic>, n: Node, text: &str) {
    let is_float = text.ends_with(['f', 'F']);
    let body: String = text
        .trim_end_matches(['f', 'F', 'd', 'D'])
        .chars()
        .filter(|c| *c != '_')
        .collect();
    let value = if is_float { body.parse::<f32>().map(f64::from) } else { body.parse::<f64>() };
    let Ok(value) = value else { return };
    if value.is_infinite() {
        out.push(CheckId::SyntaxError.at(n, "Floating-point number too large"));
    } else if value == 0.0 && has_nonzero_mantissa_digit(&body) {
        out.push(CheckId::SyntaxError.at(n, "Floating-point number too small"));
    }
}

/// Whether a non-zero digit is written before the exponent: `0.0e500` is zero, `1e-400` is not.
fn has_nonzero_mantissa_digit(body: &str) -> bool {
    let mantissa = body.split(['e', 'E']).next().unwrap_or("");
    mantissa.chars().any(|c| c.is_ascii_digit() && c != '0')
}

/// A backslash followed by something Java has no escape for.
fn escapes(out: &mut Vec<Diagnostic>, n: Node, text: &str) {
    if text.starts_with("\"\"\"") {
        return;
    }
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '\\' {
            continue;
        }
        let Some((_, next)) = chars.next() else { return };
        // `u` is a unicode escape, resolved before the lexer sees the text; the others are Java's.
        if !matches!(next, 'b' | 't' | 'n' | 'f' | 'r' | 's' | '"' | '\'' | '\\' | '0'..='7' | 'u') {
            let at = n.start_byte() + i;
            out.push(CheckId::SyntaxError.span(at, at + 1 + next.len_utf8(), "Illegal escape character"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(body: &str) -> Vec<String> {
        let src = format!("class A {{ void m() {{ Object o = {body}; }} }}");
        let tree = bennu_java::prelude::parse_java(&src).unwrap();
        let root = tree.root_node();
        let mut nodes = Vec::new();
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            nodes.push(n);
            let mut c = n.walk();
            stack.extend(n.children(&mut c));
        }
        literal_errors_nodes(&nodes, &src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn an_int_that_does_not_fit_is_reported() {
        assert_eq!(errors("3000000000").len(), 1);
        assert_eq!(errors("2147483647").len(), 0);
        assert_eq!(errors("2147483648").len(), 1);
    }

    #[test]
    fn the_minimum_value_is_legal_under_a_minus() {
        assert!(errors("-2147483648").is_empty());
        assert!(errors("-9223372036854775808L").is_empty());
        assert_eq!(errors("-2147483649").len(), 1);
        assert_eq!(errors("9223372036854775808L").len(), 1);
    }

    #[test]
    fn other_radixes_use_every_bit_of_the_type() {
        assert!(errors("0xFFFFFFFF").is_empty());
        assert_eq!(errors("0x1FFFFFFFF").len(), 1);
        assert!(errors("0xFFFFFFFFFFFFFFFFL").is_empty());
        assert!(errors("037777777777").is_empty());
        assert_eq!(errors("040000000000").len(), 1);
        assert!(errors("0b11111111111111111111111111111111").is_empty());
        assert!(errors("1_000_000").is_empty());
    }

    #[test]
    fn a_float_out_of_range_is_reported_and_zero_is_not() {
        assert_eq!(errors("1e50f").len(), 1);
        assert_eq!(errors("1e400").len(), 1);
        assert_eq!(errors("1e-400").len(), 1);
        assert!(errors("0.0").is_empty());
        assert!(errors("0e500").is_empty());
        assert!(errors("1e38f").is_empty());
        assert!(errors("3.4028235e38f").is_empty());
    }

    #[test]
    fn only_javas_escapes_are_legal() {
        assert_eq!(errors("\"bad \\q\"").len(), 1);
        assert!(errors("\"ok \\n \\t \\\\ \\\" \\0 \\377 \\u0041 \\s\"").is_empty());
        assert!(errors("'\\''").is_empty());
        assert_eq!(errors("'\\x'").len(), 1);
    }
}
