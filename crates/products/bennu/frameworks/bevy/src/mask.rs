//! Blanking out what a shape scan must not read — comments, and the inside of literals.
//!
//! Everything this crate knows about a Rust file it learns by **shape**: a `#[derive(Component)]`
//! before a `struct`, a `Query<&mut Transform>` in a parameter list, an identifier inside an
//! `add_systems(…)` call. That is only safe once the text that merely *looks* like code is out of
//! the way — a doc comment showing `#[derive(Component)]` in a fenced example would otherwise
//! declare a component nobody wrote, and the scanner would be wrong in the one direction a user
//! cannot debug: a row that names a type their code does not contain.
//!
//! So every scan runs against a **mask** of the source rather than the source: same bytes, same
//! length, same offsets — with each byte inside a comment or a literal replaced by a space. An
//! offset found in the mask is an offset in the file, which is what lets a catalog row jump.
//!
//! Length preservation is the whole contract here, and it is why the replacement is done byte by
//! byte rather than char by char: a multi-byte char inside a string becomes that many spaces. The
//! result is still valid UTF-8 because a literal's delimiters are ASCII, so a multi-byte char is
//! always wholly inside or wholly outside one.

/// `text` with every comment and literal body blanked to spaces, byte-for-byte.
///
/// Handles what a Rust file actually contains: line and (nested) block comments, ordinary and raw
/// strings of any hash depth, byte strings, char literals — and lifetimes, which open with a quote
/// and never close one.
pub fn mask(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        // A raw string, possibly byte-prefixed: `r"…"`, `r#"…"#`, `br##"…"##`.
        if let Some(end) = raw_string_end(b, i) {
            blank(&mut out, b, i, end);
            i = end;
            continue;
        }
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = b[i..].iter().position(|&c| c == b'\n').map_or(b.len(), |n| i + n);
                blank(&mut out, b, i, end);
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = block_comment_end(b, i);
                blank(&mut out, b, i, end);
                i = end;
            }
            b'"' => {
                let end = string_end(b, i);
                // The quotes themselves stay: a scan that counts brackets needs the delimiters to
                // remain balanced, and they carry no identifier.
                out.push(b'"');
                blank(&mut out, b, i + 1, end.saturating_sub(1));
                if end > i + 1 {
                    out.push(b'"');
                }
                i = end;
            }
            b'\'' => match char_literal_end(b, i) {
                Some(end) => {
                    blank(&mut out, b, i, end);
                    i = end;
                }
                // A lifetime. Nothing to hide, and treating it as an unterminated literal would
                // swallow the rest of the file.
                None => {
                    out.push(b'\'');
                    i += 1;
                }
            },
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| " ".repeat(text.len()))
}

/// Stand in for `b[start..end]` — one space per byte, **except newlines, which survive**.
///
/// Offsets are not the only thing the scan reports: a catalog row carries a line number, and it is
/// counted on the mask. Blanking the newlines inside a block comment or a raw string would keep
/// every offset right and move every line after it up.
fn blank(out: &mut Vec<u8>, b: &[u8], start: usize, end: usize) {
    let end = end.min(b.len());
    for k in start..end {
        out.push(if b[k] == b'\n' { b'\n' } else { b' ' });
    }
}

/// Whether an identifier can continue at `b[i]` — used to tell the `r` of `r"…"` from the one
/// ending `for`.
fn ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// End offset of the raw string starting at `i`, if one does.
fn raw_string_end(b: &[u8], i: usize) -> Option<usize> {
    if i > 0 && ident_byte(b[i - 1]) {
        return None;
    }
    let mut j = i;
    if b.get(j) == Some(&b'b') {
        j += 1;
    }
    if b.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let hash_start = j;
    while b.get(j) == Some(&b'#') {
        j += 1;
    }
    let hashes = j - hash_start;
    if b.get(j) != Some(&b'"') {
        return None;
    }
    j += 1;
    // Closing delimiter: a quote followed by exactly as many hashes as opened it.
    while j < b.len() {
        if b[j] == b'"' && b[j + 1..].iter().take(hashes).filter(|&&c| c == b'#').count() == hashes {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    Some(b.len())
}

/// End offset of the block comment opening at `i`, counting nesting the way rustc does.
fn block_comment_end(b: &[u8], i: usize) -> usize {
    let mut depth = 0usize;
    let mut j = i;
    while j + 1 < b.len() {
        if b[j] == b'/' && b[j + 1] == b'*' {
            depth += 1;
            j += 2;
        } else if b[j] == b'*' && b[j + 1] == b'/' {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return j;
            }
        } else {
            j += 1;
        }
    }
    b.len()
}

/// End offset (past the closing quote) of the string opening at `i`.
fn string_end(b: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => return j + 1,
            _ => j += 1,
        }
    }
    b.len()
}

/// End offset of the char literal at `i`, or `None` when the quote opens a lifetime.
///
/// The two are told apart by whether a closing quote arrives where a single char would put one —
/// which is the same rule the grammar uses, and the reason `'a'` and `'a` can sit in one file.
fn char_literal_end(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    if b.get(j) == Some(&b'\\') {
        j += 2;
        // An escape can be long (`'\u{1F600}'`), so scan on to the quote rather than assuming.
        while j < b.len() && b[j] != b'\'' && b[j] != b'\n' {
            j += 1;
        }
        return (b.get(j) == Some(&b'\'')).then_some(j + 1);
    }
    // One char — however many bytes it takes.
    if j >= b.len() {
        return None;
    }
    j += 1;
    while j < b.len() && (b[j] & 0xC0) == 0x80 {
        j += 1;
    }
    (b.get(j) == Some(&b'\'')).then_some(j + 1)
}

#[cfg(test)]
mod tests {
    use super::mask;

    /// The property everything else in the crate rests on.
    #[test]
    fn masking_preserves_every_offset() {
        for src in [
            "struct A; // #[derive(Component)]\nstruct B;",
            "let s = \"#[derive(Resource)]\"; struct C;",
            "/* /* nested */ #[derive(Event)] */ fn f() {}",
            "let r = r#\"fn fake(q: Query<&mut T>) {}\"#; fn real() {}",
            "let c = '\\''; let l: &'static str = \"x\";",
        ] {
            assert_eq!(mask(src).len(), src.len(), "offsets shifted for: {src}");
        }
    }

    #[test]
    fn masking_preserves_every_line_break() {
        for src in ["/* one\ntwo\nthree */ struct A;", "let s = r#\"a\nb\"#; struct B;"] {
            assert_eq!(
                mask(src).matches('\n').count(),
                src.matches('\n').count(),
                "lines shifted for: {src}"
            );
        }
    }

    #[test]
    fn a_derive_inside_a_comment_is_not_a_derive() {
        let m = mask("// #[derive(Component)]\n#[derive(Component)]\nstruct Real;");
        assert_eq!(m.matches("derive").count(), 1);
        assert!(m.contains("struct Real;"));
    }

    #[test]
    fn a_system_signature_inside_a_string_is_not_a_system() {
        let m = mask("let doc = \"fn moved(q: Query<&mut Transform>) {}\";\nfn real() {}");
        assert!(!m.contains("Query"));
        assert!(m.contains("fn real()"));
    }

    #[test]
    fn a_lifetime_does_not_swallow_the_file() {
        let m = mask("fn f<'w>(r: Res<'w, Score>) {}\nstruct After;");
        assert!(m.contains("struct After;"));
        assert!(m.contains("Res<'w, Score>"));
    }

    #[test]
    fn a_raw_string_ends_at_its_own_hashes() {
        let m = mask("let a = r#\"a \"quoted\" bit\"#; struct After;");
        assert!(!m.contains("quoted"));
        assert!(m.contains("struct After;"));
    }
}
