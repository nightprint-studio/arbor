//! Tiny byte-level Java scanning helpers shared by the light source transforms (`log_param`,
//! `np_equals`, …). String/char/comment-aware forward scanning without a full parse — enough for
//! caret-anchored quick-fixes that only need to find a call's parens or a literal's bounds.

/// Is `c` a valid first char of a Java identifier?
pub(crate) fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$'
}

/// Is `c` a valid non-first char of a Java identifier?
pub(crate) fn is_ident_part(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// Index just past the closing quote of the string/char literal starting at `open` (`"`/`'`).
/// Handles `\`-escapes; runs to end-of-slice on an unterminated literal.
pub(crate) fn string_end(b: &[u8], open: usize) -> usize {
    let quote = b[open];
    let mut i = open + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// Index of the newline ending the `//` line comment at `i` (or end-of-slice).
pub(crate) fn line_comment_end(b: &[u8], i: usize) -> usize {
    let mut k = i + 2;
    while k < b.len() && b[k] != b'\n' {
        k += 1;
    }
    k
}

/// Index just past the `*/` closing the block comment at `i` (or end-of-slice).
pub(crate) fn block_comment_end(b: &[u8], i: usize) -> usize {
    let mut k = i + 2;
    while k + 1 < b.len() {
        if b[k] == b'*' && b[k + 1] == b'/' {
            return k + 2;
        }
        k += 1;
    }
    b.len()
}

/// The `)` matching the `(` at `open`, respecting nested parens, strings and comments. `None` on
/// an unbalanced tail.
pub(crate) fn matching_paren(b: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = open;
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
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// If `op` is *exactly* one string literal, return its inner content (between the quotes, escapes
/// preserved). `"a"` → `Some("a")`; `"a".trim()` / `x` / `'c'` → `None`.
pub(crate) fn pure_string_literal(op: &str) -> Option<&str> {
    let b = op.as_bytes();
    if b.first() != Some(&b'"') {
        return None;
    }
    let end = string_end(b, 0);
    if end == b.len() {
        Some(&op[1..end - 1])
    } else {
        None
    }
}

/// Start byte of the postfix-chain expression ending just before byte `end` (exclusive). Walks
/// **left** over identifiers, `.` separators, balanced `(...)` calls and `[...]` indexes, plus an
/// optional leading unary `!`; stops at the first char that can't be part of the chain (a binary
/// operator, `(`, `,`, `{`, `;`, `=`, a keyword boundary, …). `None` if nothing chain-like precedes
/// `end`. Finds a call receiver (`a.b().c` before `.equals`) or a comparison's left operand.
pub(crate) fn chain_start(b: &[u8], end: usize) -> Option<usize> {
    let mut i = end;
    let mut consumed = false;
    loop {
        while i > 0 && b[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        if i == 0 {
            break;
        }
        match b[i - 1] {
            b')' => {
                i = match_back(b, i - 1, b'(', b')')?;
                consumed = true;
            }
            b']' => {
                i = match_back(b, i - 1, b'[', b']')?;
                consumed = true;
            }
            c if is_ident_part(c) => {
                while i > 0 && is_ident_part(b[i - 1]) {
                    i -= 1;
                }
                consumed = true;
            }
            b'.' => {
                i -= 1; // chain separator — keep walking left
            }
            _ => break,
        }
    }
    // Absorb a single leading unary `!` (skipping whitespace) so `!x`'s operand includes the `!`.
    if consumed {
        let mut k = i;
        while k > 0 && b[k - 1].is_ascii_whitespace() {
            k -= 1;
        }
        if k > 0 && b[k - 1] == b'!' && !(k >= 2 && b[k - 2] == b'!') {
            i = k - 1;
        }
    }
    if consumed && i < end {
        Some(i)
    } else {
        None
    }
}

/// The `open` bracket matching the `close`-type bracket at index `close`, scanning **left**. Best
/// effort — does not skip string literals (rare inside a receiver chain / balanced operand).
pub(crate) fn match_back(b: &[u8], close: usize, open: u8, closec: u8) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = close;
    loop {
        if b[i] == closec {
            depth += 1;
        } else if b[i] == open {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}
