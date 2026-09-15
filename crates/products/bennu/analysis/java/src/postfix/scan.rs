//! Where the expression a postfix template wraps begins.

/// The byte offset where the expression ending at the `.` at `dot` starts, or `None` when there is no
/// expression there.
///
/// Walks backwards over identifiers and dots, jumping balanced `()` and `[]` and string or character
/// literals whole, so `repo.find(a, b[i]).nn` takes the whole call and `"a.b".sout` the whole literal.
/// It stops at the first byte that cannot belong to the subject — an operator, a space, a brace — which
/// is what keeps `x = y.nn` from swallowing the `x =`.
///
/// A `new` in front of what it found is taken too: in `new Order().var` the subject is the new order,
/// not a call to a method named `Order`.
pub fn subject_start(source: &str, dot: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if dot > bytes.len() || !source.is_char_boundary(dot) {
        return None;
    }
    let mut start = dot;
    while start > 0 {
        start = match bytes[start - 1] {
            b')' | b']' => matching_open(bytes, start - 1)?,
            b'"' | b'\'' => literal_start(bytes, start - 1)?,
            b'.' => start - 1,
            byte if is_identifier_byte(byte) => start - 1,
            _ => break,
        };
    }
    // Nothing before the dot, or a chain that begins with one — neither is an expression.
    if start == dot || bytes[start] == b'.' {
        return None;
    }
    Some(with_new(bytes, start))
}

/// The offset of the bracket opening the one that closes at `close`, or `None` if it is unbalanced.
fn matching_open(bytes: &[u8], close: usize) -> Option<usize> {
    let closer = bytes[close];
    let opener = if closer == b')' { b'(' } else { b'[' };
    let mut depth = 0usize;
    let mut i = close + 1;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'"' | b'\'' => i = literal_start(bytes, i)?,
            byte if byte == closer => depth += 1,
            byte if byte == opener => {
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

/// The offset of the quote opening the literal that ends at `close`, or `None` if it is unterminated.
fn literal_start(bytes: &[u8], close: usize) -> Option<usize> {
    let quote = bytes[close];
    let mut i = close;
    while i > 0 {
        i -= 1;
        if bytes[i] != quote {
            continue;
        }
        // An odd run of backslashes in front of it means this quote is escaped.
        let slashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
        if slashes % 2 == 0 {
            return Some(i);
        }
    }
    None
}

/// `start`, moved back over a `new ` that precedes it.
fn with_new(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    if i == start || i < 3 || &bytes[i - 3..i] != b"new" {
        return start;
    }
    let keyword = i - 3;
    if keyword > 0 && is_identifier_byte(bytes[keyword - 1]) {
        return start;
    }
    keyword
}

/// A byte that can be part of a Java identifier. Every non-ASCII byte counts, because Java allows
/// letters from any script and none of the delimiters the scan stops at is outside ASCII.
fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The subject of the postfix template typed after the LAST dot of `src`.
    fn subject(src: &str) -> Option<&str> {
        let dot = src.rfind('.')?;
        subject_start(src, dot).map(|start| &src[start..dot])
    }

    #[test]
    fn a_call_chain_is_taken_whole_with_its_arguments() {
        assert_eq!(subject("x = repo.find(a, b[i]).nn"), Some("repo.find(a, b[i])"));
    }

    #[test]
    fn a_literal_is_taken_whole_dots_and_escapes_included() {
        assert_eq!(subject(r#"log("a.b".sout"#), Some(r#""a.b""#));
        assert_eq!(subject(r#"  "say \"hi\"".sout"#), Some(r#""say \"hi\"""#));
    }

    #[test]
    fn the_scan_stops_at_an_operator_or_a_space() {
        assert_eq!(subject("int x = y.nn"), Some("y"));
        assert_eq!(subject("if (!flag.not"), Some("flag"));
        assert_eq!(subject("return order.getTotal().var"), Some("order.getTotal()"));
    }

    #[test]
    fn a_parenthesised_expression_is_one_subject() {
        assert_eq!(subject("x = (a + b).par"), Some("(a + b)"));
    }

    /// `new Order().var` declares the new order; without the keyword the subject would read as a call.
    #[test]
    fn a_constructor_call_brings_its_new() {
        assert_eq!(subject("return new Order(id).var"), Some("new Order(id)"));
        // `renew` merely ends in the letters.
        assert_eq!(subject("renew Order().var"), Some("Order()"));
    }

    #[test]
    fn nothing_before_the_dot_is_no_subject() {
        assert_eq!(subject("  .nn"), None);
        assert_eq!(subject("foo(.nn"), None);
    }

    #[test]
    fn an_unbalanced_bracket_is_no_subject() {
        assert_eq!(subject("x = a).nn"), None);
    }

    #[test]
    fn a_dot_inside_a_character_is_refused_rather_than_split() {
        let src = "è.nn";
        assert_eq!(subject_start(src, 1), None);
        assert_eq!(subject_start(src, 2), Some(0));
    }
}
