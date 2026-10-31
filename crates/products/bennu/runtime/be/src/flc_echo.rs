//! The next element of a list that repeats itself: `List.of(tabellato("1", "ITALIA"), |` → `tabellato("2", "")`.
//!
//! Right after a comma, if the element before it is a call (or `new`), the same call is drawn again with its string
//! literals emptied and a number held in a string counted up — the way a person writes the second row of a table.
//! Pure text, no index: it reads the sibling that is right there, and only that. Calls only: repeating a variable or a
//! literal would be noise.

/// What to draw after the caret.
pub(crate) struct Echo {
    pub(crate) text: String,
    /// 0..1.
    pub(crate) confidence: f32,
}

/// Longest sibling worth repeating.
const MAX_LEN: usize = 160;

/// The text of the element that ends right before `head`'s last character (a comma's position): everything since the
/// previous comma at the same depth, or the opening bracket.
fn previous_element(head: &str) -> Option<&str> {
    let b = head.as_bytes();
    let mut starts: Vec<usize> = vec![0];
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' | b'[' | b'{' => starts.push(i + 1),
            b')' | b']' | b'}' => {
                starts.pop();
                if starts.is_empty() {
                    return None;
                }
            }
            b',' => {
                if let Some(last) = starts.last_mut() {
                    *last = i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    Some(head[*starts.last()?..].trim())
}

/// A call or a `new`: `name(…)` / `a.b.c(…)` / `new T<>(…)`, whole, ending in its closing parenthesis.
fn is_call(element: &str) -> bool {
    let head: String = element.chars().take_while(|c| *c != '(').collect();
    let name = head.trim().strip_prefix("new ").unwrap_or(head.trim());
    !name.is_empty()
        && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$' | '<' | '>' | ' ' | ',' | '?'))
        && (!name.contains(' ') || name.contains('<'))
        && element.ends_with(')')
        && element.contains('(')
}

/// `element` with every string literal emptied, except a number, which is counted up by one.
fn next_of(element: &str) -> String {
    let b = element.as_bytes();
    let (mut out, mut i) = (String::new(), 0usize);
    while i < b.len() {
        match b[i] {
            b'"' => {
                let start = i + 1;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                let inner = element.get(start..i.min(element.len())).unwrap_or("");
                match inner.parse::<u64>() {
                    Ok(n) if !inner.starts_with('0') || inner == "0" => out.push_str(&format!("\"{}\"", n + 1)),
                    _ => out.push_str("\"\""),
                }
                i += 1;
            }
            _ => {
                // copy one whole character
                let ch = element[i..].chars().next().unwrap_or(' ');
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    out
}

/// The echo of the call before the comma the caret follows, minus what is already typed of it.
pub(crate) fn suggest(source: &str, offset: usize) -> Option<Echo> {
    let before = source.get(..offset)?;
    let typed_len = before.bytes().rev().take_while(|&c| c.is_ascii_alphanumeric() || c == b'_').count();
    let typed = &before[before.len() - typed_len..];
    let up_to_comma = before[..before.len() - typed_len].trim_end();
    let head = up_to_comma.strip_suffix(',')?;
    // a string or a comment around the comma: nothing here is code
    let line = &head[head.rfind('\n').map_or(0, |p| p + 1)..];
    if line.matches('"').count() % 2 == 1 || line.contains("//") {
        return None;
    }
    let element = previous_element(head)?;
    if element.len() > MAX_LEN || !is_call(element) {
        return None;
    }
    let next = next_of(element);
    let rest = next.strip_prefix(typed).filter(|r| !r.is_empty())?;
    Some(Echo { text: rest.to_string(), confidence: 0.6 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo(code: &str) -> Option<String> {
        suggest(code, code.len()).map(|e| e.text)
    }

    #[test]
    fn a_call_is_drawn_again_with_its_strings_emptied_and_its_number_counted_up() {
        assert_eq!(echo("f(tabellato(\"1\", \"ITALIA\"), ").as_deref(), Some("tabellato(\"2\", \"\")"));
        assert_eq!(echo("List.of(row(\"9\"),\n            ").as_deref(), Some("row(\"10\")"));
        assert_eq!(echo("x(new Item(\"a\", 3), ").as_deref(), Some("new Item(\"\", 3)"));
    }

    #[test]
    fn the_typed_start_is_taken_off() {
        assert_eq!(echo("f(tabellato(\"1\", \"A\"), tab").as_deref(), Some("ellato(\"2\", \"\")"));
        assert_eq!(echo("f(tabellato(\"1\", \"A\"), zz"), None);
    }

    #[test]
    fn only_a_call_is_repeated_and_only_after_a_comma() {
        assert_eq!(echo("f(a, "), None, "a variable");
        assert_eq!(echo("f(\"1\", "), None, "a literal");
        assert_eq!(echo("f(tabellato(\"1\") "), None, "no comma");
        assert_eq!(echo("f(g(x, y"), None, "inside the call, not after it");
        assert_eq!(echo("String s = \"a, row(\\\"1\\\"), "), None, "inside a string");
    }

    #[test]
    fn a_leading_zero_is_not_a_count() {
        assert_eq!(echo("f(row(\"007\"), ").as_deref(), Some("row(\"\")"));
    }
}
