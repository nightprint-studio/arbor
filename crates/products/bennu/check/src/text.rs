//! Source text quoted **into a diagnostic message**.
//!
//! A message that names the offending code reads far better than one that doesn't —
//! "`foo.bar` is not a statement" beats "not a statement". The trap is that the node
//! it quotes is whatever the parser found there, and that can be a whole chained
//! expression, an array initializer, or a string literal holding a pasted 250 KB
//! document. Quoted whole, the message stops being a sentence and becomes the file,
//! and the tooltip that shows it covers the editor.
//!
//! So text going into a message goes through [`excerpt`]. It is one line, it is
//! short, and it is enough to recognise what is being talked about — which is all a
//! message needs, since the diagnostic already carries the span that points at it.

/// One flat, short line standing for `s`, with an ellipsis when it was cut.
///
/// Whitespace is collapsed first: a quoted expression spanning lines would otherwise
/// spend the whole budget on indentation, and a message is a sentence rather than a
/// reproduction. The cut counts **characters**, so it never lands inside one.
pub fn excerpt(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    // Bytes ≥ chars, so this rules out the common case without counting.
    if flat.len() <= max {
        return flat;
    }
    match flat.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &flat[..cut]),
        None => flat,
    }
}

/// The budget for text quoted inside a message — long enough to recognise an
/// expression, short enough that the message stays one.
pub const EXCERPT_CHARS: usize = 60;

/// [`excerpt`] at the standard budget.
pub fn short(s: &str) -> String {
    excerpt(s, EXCERPT_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_short_text_verbatim() {
        assert_eq!(short("foo.bar"), "foo.bar");
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(short("foo\n    .bar()\n    .baz()"), "foo .bar() .baz()");
    }

    #[test]
    fn cuts_long_text() {
        let out = short(&"x".repeat(500));
        assert_eq!(out.chars().count(), EXCERPT_CHARS + 1); // + the ellipsis
        assert!(out.ends_with('…'));
    }

    /// The cut is by character, so it can never land inside one — the panic this
    /// would otherwise be is the whole reason it does not slice by byte.
    #[test]
    fn cuts_multibyte_text_without_panicking() {
        let out = excerpt(&"è".repeat(100), 10);
        assert_eq!(out, format!("{}…", "è".repeat(10)));
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(short(""), "");
    }
}
