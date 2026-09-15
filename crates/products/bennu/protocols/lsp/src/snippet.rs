//! LSP snippet bodies, reduced to plain text plus the positions of their tab stops.
//!
//! ## Why here and not in the editor
//!
//! A completion body like `Some(${1:value})$0` has to become *something an editor can insert*, and
//! there are two ways to split that work. Handing the frontend the raw body means writing a parser
//! for LSP's snippet grammar in TypeScript, where this repository has no test runner — for a
//! function whose failure mode is putting `${1:value}` into the user's source. So the parse happens
//! here, where it is tested, and what crosses the wire is the **plain text** and a list of
//! `[start, end)` byte ranges.
//!
//! That shape is also the right one for the wire: it says nothing about CodeMirror. Rebuilding a
//! CodeMirror template from text-plus-ranges is a mechanical inverse the frontend can do totally;
//! parsing the grammar is not.
//!
//! ## The grammar, and what is dropped
//!
//! | Written | Means | Here |
//! |---|---|---|
//! | `$1` `${1}` | an empty tab stop | a zero-width stop |
//! | `${1:value}` | a stop pre-filled with `value` | `value`, with a stop over it |
//! | `${1\|a,b,c\|}` | a stop offering a choice | `a`, with a stop over it — the first choice |
//! | `$0` | where the caret ends up | the **last** stop, whatever number it was written as |
//! | `\$` `\}` `\\` | a literal `$`, `}`, `\` | the character |
//! | `$TM_FILENAME` | an editor variable | **dropped** |
//!
//! Variables are dropped rather than guessed at: resolving one needs editor context this crate does
//! not have, and rust-analyzer does not emit them. A body that used one would be missing a word;
//! substituting the variable's *name* would look like working output and be wrong.
//!
//! Nesting (`${1:Some(${2:x})}`) is flattened: the inner stop keeps its own range inside the outer
//! one's text. Both stops survive, which is what matters — a reader tabs through them in order.

/// One tab stop, as a byte range into the plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stop {
    /// The stop's ordering index as the server wrote it, with `$0` already moved to the end.
    pub index: u32,
    /// Byte offset in the plain text where the stop starts.
    pub start: usize,
    /// Byte offset where it ends. Equal to `start` for an empty stop, which is a caret position
    /// rather than a selection.
    pub end: usize,
}

/// A snippet body reduced to what an editor needs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snippet {
    /// The text to insert, with every placeholder replaced by its default content.
    pub text: String,
    /// The tab stops, in the order they are visited.
    pub stops: Vec<Stop>,
}

impl Snippet {
    /// Whether there is anything to navigate. A body with no stops is a plain insertion, and saying
    /// so lets the caller skip the editor's snippet machinery entirely.
    pub fn is_plain(&self) -> bool {
        self.stops.is_empty()
    }
}

/// Parse an LSP snippet body.
///
/// Never fails: an unterminated or malformed placeholder is emitted as the literal text it is, which
/// is the same thing every editor does and is far better than refusing the completion.
pub fn parse(body: &str) -> Snippet {
    let mut text = String::with_capacity(body.len());
    // `(index, start, end)` — collected in source order, ordered at the end.
    let mut stops: Vec<(u32, usize, usize)> = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            // An escape. LSP escapes `$`, `}` and `\`; anything else after a backslash is a literal
            // backslash followed by that character, which is what a regex or a path in a snippet
            // relies on.
            b'\\' if i + 1 < bytes.len() => {
                let next = bytes[i + 1];
                if matches!(next, b'$' | b'}' | b'\\') {
                    text.push(next as char);
                    i += 2;
                } else {
                    text.push('\\');
                    i += 1;
                }
            }
            // `$1` — a bare numbered stop.
            b'$' if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() => {
                let (index, after) = read_number(bytes, i + 1);
                let at = text.len();
                stops.push((index, at, at));
                i = after;
            }
            // `${…}` — a stop with content, a choice, or a variable.
            b'$' if bytes.get(i + 1) == Some(&b'{') => match read_braced(body, i + 2) {
                Some((inner, after)) => {
                    consume_braced(inner, &mut text, &mut stops);
                    i = after;
                }
                // Unterminated — the rest of the body is literal. This is the state a hand-written
                // snippet is in while it is being typed, and a server can emit it too.
                None => {
                    text.push('$');
                    i += 1;
                }
            },
            b => {
                // Push the whole UTF-8 character, not the byte: `text` is a String and a byte from
                // the middle of a multi-byte character is not a `char`.
                let ch_len = utf8_len(b);
                let end = (i + ch_len).min(body.len());
                text.push_str(&body[i..end]);
                i = end;
            }
        }
    }

    Snippet { text, stops: order(stops) }
}

/// The contents of one `${…}`, appended to `text` with any stops it contributes.
fn consume_braced(inner: &str, text: &mut String, stops: &mut Vec<(u32, usize, usize)>) {
    let bytes = inner.as_bytes();
    let (index, after) = read_number(bytes, 0);
    // A `${name}` or `${name:default}` — an editor variable, which this crate cannot resolve. See
    // the module doc for why it is dropped rather than substituted with its own name.
    if after == 0 {
        return;
    }
    let rest = &inner[after..];
    let start = text.len();

    if let Some(default) = rest.strip_prefix(':') {
        // Nested placeholders inside the default are parsed too, so `${1:Some(${2:x})}` keeps both
        // stops. Their offsets are relative to the nested text, so they are shifted into place.
        let nested = parse(default);
        text.push_str(&nested.text);
        for stop in nested.stops {
            stops.push((stop.index, start + stop.start, start + stop.end));
        }
    } else if let Some(choices) = rest.strip_prefix('|') {
        // `${1|a,b,c|}` — the first choice is the default. The editor has no choice widget, and a
        // stop over the first option is what lets the user replace it with another.
        let list = choices.strip_suffix('|').unwrap_or(choices);
        let first = split_choice(list).into_iter().next().unwrap_or_default();
        text.push_str(&first);
    }
    // else: a bare `${1}` — an empty stop.

    stops.push((index, start, text.len()));
}

/// Split a choice list on unescaped commas, unescaping `\,` and `\|`.
fn split_choice(list: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut escaped = false;
    for ch in list.chars() {
        match ch {
            '\\' if !escaped => escaped = true,
            ',' if !escaped => out.push(String::new()),
            c => {
                out.last_mut().expect("always one element").push(c);
                escaped = false;
            }
        }
    }
    out
}

/// Read the digits at `from`, returning the number and the offset past them. `(0, from)` when there
/// are none — the caller reads an unmoved offset as "not a numbered stop".
fn read_number(bytes: &[u8], from: usize) -> (u32, usize) {
    let mut i = from;
    let mut n: u32 = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        // Saturating: a stop numbered beyond u32 is nobody's snippet, and overflowing would panic.
        n = n.saturating_mul(10).saturating_add((bytes[i] - b'0') as u32);
        i += 1;
    }
    (n, i)
}

/// The text between `${` at `from` and its matching `}`, plus the offset past that brace.
///
/// Brace-counting rather than a search for the next `}`, because a nested placeholder has one of its
/// own: `${1:Some(${2:x})}` closes at the last brace and not the first.
fn read_braced(body: &str, from: usize) -> Option<(&str, usize)> {
    let bytes = body.as_bytes();
    let mut depth = 1usize;
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&body[from..i], i + 1));
                }
                i += 1;
            }
            b => i += utf8_len(b),
        }
    }
    None
}

/// How many bytes the UTF-8 character starting with `b` occupies.
fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Put the stops in visiting order.
///
/// By index, with **`0` last**: in LSP `$0` is where the caret ends up, so ordering numerically
/// would visit it first and leave the caret at the start of what was just inserted. Stops sharing an
/// index (a server repeating `$1` to mirror a value) keep their source order, which is why the sort
/// is stable.
fn order(mut stops: Vec<(u32, usize, usize)>) -> Vec<Stop> {
    stops.sort_by_key(|(index, _, _)| if *index == 0 { u32::MAX } else { *index });
    stops.into_iter().map(|(index, start, end)| Stop { index, start, end }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(body: &str) -> (String, Vec<(usize, usize)>) {
        let s = parse(body);
        (s.text.clone(), s.stops.iter().map(|t| (t.start, t.end)).collect())
    }

    #[test]
    fn a_body_with_no_placeholders_is_plain_text() {
        let s = parse("HashMap::new()");
        assert_eq!(s.text, "HashMap::new()");
        assert!(s.is_plain(), "nothing to navigate — the caller can skip the snippet machinery");
    }

    /// The shape rust-analyzer emits for a function call.
    #[test]
    fn a_placeholder_leaves_its_default_and_a_stop_over_it() {
        let (text, stops) = spans("with_capacity(${1:cap})");
        assert_eq!(text, "with_capacity(cap)");
        assert_eq!(stops, vec![(14, 17)], "the stop covers `cap`, ready to be typed over");
        assert_eq!(&text[14..17], "cap");
    }

    #[test]
    fn a_bare_stop_is_a_caret_position_not_a_selection() {
        let (text, stops) = spans("Some($1)");
        assert_eq!(text, "Some()");
        assert_eq!(stops, vec![(5, 5)]);
        let (text, stops) = spans("Some(${1})");
        assert_eq!(text, "Some()");
        assert_eq!(stops, vec![(5, 5)]);
    }

    /// The one rule a numeric sort gets backwards: `$0` is where the caret *ends up*.
    #[test]
    fn the_final_stop_is_visited_last_however_it_is_numbered() {
        let s = parse("match ${1:expr} {\n    ${2:pattern} => $0,\n}");
        let indices: Vec<u32> = s.stops.iter().map(|t| t.index).collect();
        assert_eq!(indices, vec![1, 2, 0], "0 goes to the end");
        // And the offsets are still the real ones in the text.
        assert_eq!(&s.text[s.stops[0].start..s.stops[0].end], "expr");
        assert_eq!(&s.text[s.stops[1].start..s.stops[1].end], "pattern");
        assert_eq!(s.stops[2].start, s.stops[2].end, "the final stop is a caret");
    }

    #[test]
    fn stops_come_back_in_visiting_order_not_source_order() {
        let s = parse("${2:second} ${1:first}");
        assert_eq!(s.text, "second first");
        let visited: Vec<&str> =
            s.stops.iter().map(|t| &s.text[t.start..t.end]).collect();
        assert_eq!(visited, vec!["first", "second"]);
    }

    /// A server mirroring one value into two places writes the same number twice. Both stops
    /// survive, in source order.
    #[test]
    fn a_repeated_index_keeps_both_stops() {
        let s = parse("let ${1:name} = ${1:name};");
        assert_eq!(s.text, "let name = name;");
        assert_eq!(s.stops.len(), 2);
        assert_eq!((s.stops[0].start, s.stops[1].start), (4, 11));
    }

    #[test]
    fn a_choice_becomes_its_first_option() {
        let (text, stops) = spans("pub${1| , (crate) |}fn");
        assert_eq!(text, "pub fn", "the first choice, with a stop over it");
        assert_eq!(stops, vec![(3, 4)]);
        // An escaped comma is part of one option rather than a separator.
        let (text, _) = spans("${1:x}${2|a\\,b,c|}");
        assert_eq!(text, "xa,b");
    }

    #[test]
    fn escapes_produce_the_literal_character() {
        assert_eq!(parse(r"cost: \$5").text, "cost: $5");
        assert_eq!(parse(r"a\}b").text, "a}b");
        assert_eq!(parse(r"a\\b").text, r"a\b");
        // A backslash before anything else is a literal backslash — what a regex in a snippet
        // depends on.
        assert_eq!(parse(r"\d+").text, r"\d+");
        // And an escaped `$` is not a stop.
        assert!(parse(r"\$1").stops.is_empty());
    }

    /// Braces in the body are the normal case for Rust, and none of them is a placeholder.
    #[test]
    fn literal_braces_survive_untouched() {
        let (text, stops) = spans("impl ${1:Trait} for ${2:Type} {\n    $0\n}");
        assert_eq!(text, "impl Trait for Type {\n    \n}");
        assert_eq!(stops.len(), 3);
        assert!(text.ends_with("\n}"));
    }

    #[test]
    fn a_nested_placeholder_keeps_both_stops_with_the_inner_one_inside() {
        let s = parse("${1:Some(${2:value})}");
        assert_eq!(s.text, "Some(value)");
        let outer = &s.stops[0];
        let inner = &s.stops[1];
        assert_eq!(&s.text[outer.start..outer.end], "Some(value)");
        assert_eq!(&s.text[inner.start..inner.end], "value");
        assert!(inner.start >= outer.start && inner.end <= outer.end, "nested inside");
    }

    /// An editor variable cannot be resolved here. Dropping it loses a word; substituting its NAME
    /// would look like working output and be wrong.
    #[test]
    fn an_editor_variable_is_dropped_rather_than_guessed_at() {
        let s = parse("// ${TM_FILENAME}\nfn ${1:name}() {}");
        assert_eq!(s.text, "// \nfn name() {}");
        assert_eq!(s.stops.len(), 1, "the variable contributes no stop");
    }

    /// Every malformed shape must produce text rather than an error — including the ones a body is
    /// in while somebody is typing it.
    #[test]
    fn a_malformed_body_degrades_to_literal_text() {
        assert_eq!(parse("${1:unterminated").text, "${1:unterminated");
        assert_eq!(parse("$").text, "$");
        assert_eq!(parse("${").text, "${");
        assert_eq!(parse("").text, "");
        assert_eq!(parse("a$").text, "a$");
        // A trailing backslash is a literal backslash.
        assert_eq!(parse(r"a\").text, r"a\");
    }

    /// Offsets are BYTE offsets, so a multi-byte character before a stop must shift it.
    #[test]
    fn offsets_are_byte_offsets_and_survive_multibyte_text() {
        let s = parse("// è\nfn ${1:name}()");
        // `è` is two bytes, so `name` starts two later than a char count would say.
        assert_eq!(&s.text[s.stops[0].start..s.stops[0].end], "name");
        assert_eq!(s.text.as_bytes()[s.stops[0].start], b'n');
        // And a multi-byte character INSIDE a placeholder's default is intact.
        let s = parse("${1:città}");
        assert_eq!(s.text, "città");
        assert_eq!(&s.text[s.stops[0].start..s.stops[0].end], "città");
    }

    /// A stop number beyond `u32` is nobody's snippet, and overflowing would panic.
    #[test]
    fn an_absurd_stop_number_does_not_panic() {
        let s = parse("${99999999999999999999:x}");
        assert_eq!(s.text, "x");
        assert_eq!(s.stops.len(), 1);
    }
}
