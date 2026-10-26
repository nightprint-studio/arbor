//! A forgiving scan of a mapper's markup into tags and character data.
//!
//! Not an XML parser: the buffer is being edited, so it is routinely unbalanced, and a colouring
//! pass that gave up on the first unclosed tag would blank the whole file mid-keystroke. This reads
//! what is there and stops caring about what is not. Spans are **byte offsets** into the text.

/// A byte range, `start..end`.
pub(crate) type Span = (usize, usize);

/// One attribute: its name, and where its value sits (the quotes excluded).
pub(crate) struct Attr<'a> {
    pub name: &'a str,
    pub value: Span,
}

pub(crate) enum Event<'a> {
    /// `<name attr="…">`, or `<name … />` when `self_closing`.
    Open { name: &'a str, name_span: Span, attrs: Vec<Attr<'a>>, self_closing: bool },
    /// `</name>`.
    Close { name: &'a str, name_span: Span },
    /// Character data, or the inside of a CDATA section — the pieces SQL is written in.
    Text(Span),
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'.' | b'-')
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|i| from + i)
}

/// Every tag and text run of `source`, in order.
pub(crate) fn scan(source: &str) -> Vec<Event<'_>> {
    let b = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut text_from = 0;
    fn flush_text(out: &mut Vec<Event<'_>>, from: usize, to: usize) {
        if to > from {
            out.push(Event::Text((from, to)));
        }
    }

    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if b[i..].starts_with(b"<!--") {
            flush_text(&mut out, text_from, i);
            i = find(b, i + 4, b"-->").map_or(b.len(), |e| e + 3);
            text_from = i;
        } else if b[i..].starts_with(b"<![CDATA[") {
            flush_text(&mut out, text_from, i);
            let inner = i + 9;
            let end = find(b, inner, b"]]>");
            flush_text(&mut out, inner, end.unwrap_or(b.len()));
            i = end.map_or(b.len(), |e| e + 3);
            text_from = i;
        } else if b[i..].starts_with(b"<?") {
            flush_text(&mut out, text_from, i);
            i = find(b, i + 2, b"?>").map_or(b.len(), |e| e + 2);
            text_from = i;
        } else if b[i..].starts_with(b"<!") {
            // A DOCTYPE, whose internal subset (`[ … ]`) can hold `>`s of its own.
            flush_text(&mut out, text_from, i);
            let gt = find(b, i, b">");
            let bracket = find(b, i, b"[");
            i = match (gt, bracket) {
                (Some(g), Some(k)) if k < g => find(b, k, b"]>").map_or(b.len(), |e| e + 2),
                (Some(g), _) => g + 1,
                (None, _) => b.len(),
            };
            text_from = i;
        } else if b.get(i + 1) == Some(&b'/') {
            flush_text(&mut out, text_from, i);
            let name_start = i + 2;
            let mut j = name_start;
            while j < b.len() && is_name_byte(b[j]) {
                j += 1;
            }
            out.push(Event::Close { name: &source[name_start..j], name_span: (name_start, j) });
            i = find(b, j, b">").map_or(b.len(), |e| e + 1);
            text_from = i;
        } else if b.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
            flush_text(&mut out, text_from, i);
            let (event, next) = open_tag(source, i);
            out.push(event);
            i = next;
            text_from = i;
        } else {
            // A lone `<` — in SQL text it is a comparison that should have been `&lt;`.
            i += 1;
        }
    }
    flush_text(&mut out, text_from, b.len());
    out
}

/// The open tag starting at `at` (a `<`), and where the scan resumes after it.
fn open_tag(source: &str, at: usize) -> (Event<'_>, usize) {
    let b = source.as_bytes();
    let name_start = at + 1;
    let mut j = name_start;
    while j < b.len() && is_name_byte(b[j]) {
        j += 1;
    }
    let name_span = (name_start, j);
    let mut attrs = Vec::new();
    let mut self_closing = false;

    while j < b.len() {
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        match b.get(j) {
            None => break,
            Some(b'>') => {
                j += 1;
                break;
            }
            Some(b'/') if b.get(j + 1) == Some(&b'>') => {
                self_closing = true;
                j += 2;
                break;
            }
            Some(b'/') => j += 1,
            Some(_) => {
                let attr_start = j;
                while j < b.len() && is_name_byte(b[j]) {
                    j += 1;
                }
                if j == attr_start {
                    j += 1; // junk byte: step over it rather than loop forever
                    continue;
                }
                let name = &source[attr_start..j];
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if b.get(j) != Some(&b'=') {
                    continue;
                }
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if let Some(&q) = b.get(j).filter(|c| matches!(**c, b'"' | b'\'')) {
                    let value_start = j + 1;
                    let value_end = find(b, value_start, &[q]).unwrap_or(b.len());
                    attrs.push(Attr { name, value: (value_start, value_end) });
                    j = (value_end + 1).min(b.len());
                }
            }
        }
    }
    (Event::Open { name: &source[name_span.0..name_span.1], name_span, attrs, self_closing }, j)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_names(src: &str) -> Vec<String> {
        scan(src)
            .into_iter()
            .filter_map(|e| match e {
                Event::Open { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn attributes_carry_the_span_of_their_value_without_quotes() {
        let src = r#"<select id="findById" resultMap='orderMap'>"#;
        let events = scan(src);
        let Event::Open { attrs, .. } = &events[0] else { panic!("an open tag") };
        assert_eq!(attrs.len(), 2);
        assert_eq!(&src[attrs[0].value.0..attrs[0].value.1], "findById");
        assert_eq!(attrs[1].name, "resultMap");
        assert_eq!(&src[attrs[1].value.0..attrs[1].value.1], "orderMap");
    }

    #[test]
    fn a_gt_inside_an_attribute_value_does_not_end_the_tag() {
        let src = r#"<if test="a > 1">x</if>"#;
        assert_eq!(open_names(src), ["if"]);
        let texts: Vec<&str> = scan(src)
            .into_iter()
            .filter_map(|e| match e {
                Event::Text((s, e)) => Some(&src[s..e]),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["x"]);
    }

    #[test]
    fn cdata_content_is_text_and_its_markers_are_not() {
        let src = "<select><![CDATA[ a < b ]]></select>";
        let texts: Vec<&str> = scan(src)
            .into_iter()
            .filter_map(|e| match e {
                Event::Text((s, e)) => Some(&src[s..e]),
                _ => None,
            })
            .collect();
        assert_eq!(texts, [" a < b "]);
    }

    #[test]
    fn comments_doctypes_and_declarations_produce_no_events() {
        let src = "<?xml version=\"1.0\"?><!DOCTYPE mapper PUBLIC \"-//x\" \"y.dtd\"><!-- <select> --><mapper/>";
        assert_eq!(open_names(src), ["mapper"]);
        assert!(scan(src).iter().all(|e| !matches!(e, Event::Text(_))));
    }

    #[test]
    fn an_unterminated_tag_ends_the_scan_instead_of_looping() {
        assert_eq!(open_names("<select id=\"a"), ["select"]);
        assert!(scan("<select").len() == 1);
    }

    #[test]
    fn a_self_closing_tag_is_reported_as_such() {
        let events = scan("<include refid=\"c\"/>");
        let Event::Open { self_closing, .. } = &events[0] else { panic!() };
        assert!(*self_closing);
    }
}
