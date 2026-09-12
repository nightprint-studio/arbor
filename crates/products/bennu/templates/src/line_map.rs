//! Which line of a template wrote each line of what it rendered — what lets a preview light up the lines the
//! caret's template line wrote, and the template lines behind a selection in the output.
//!
//! Jinja keeps no such record: a render is text in, text out. So the template is rendered a second time with
//! each line that writes text **marked** — a private-use character naming the line, placed where the line's
//! text starts — and the marks are read back out of the output and removed. A line inside a loop is marked
//! once and written many times, so every line it wrote carries it.
//!
//! ## What keeps a mark from changing the output
//!
//! A mark is text, and text beside a tag can change what the tag does. So three kinds of line are left
//! unmarked:
//!
//! - a line that **starts with a statement or a comment**: `lstrip_blocks` strips the indentation before one
//!   only when nothing else precedes it on the line;
//! - a line that **starts with `{{-`**, which strips the whitespace before it back into the line above;
//! - a **blank** line, which a `{%-` below would strip away.
//!
//! A mark goes after the indentation rather than before it, so a `-%}` above strips that indentation as it
//! would have. And whatever this does not foresee is caught anyway: the marked render with its marks removed
//! must *be* the render. When it is not — a `{% set %}` block whose text is then measured, say — there is no
//! map, rather than a wrong one.

use std::cell::RefCell;

/// Supplementary Private Use Area-A: one code point per line — a single character, so no filter or rule can
/// split one — and nothing a template or a class is ever written with.
const FIRST_MARK: u32 = 0xF0000;
const LAST_MARK: u32 = 0xFFFFD;

fn mark_of(line: u32) -> Option<char> {
    FIRST_MARK.checked_add(line).filter(|code| *code <= LAST_MARK).and_then(char::from_u32)
}

fn line_of(c: char) -> Option<u32> {
    let code = c as u32;
    (FIRST_MARK..=LAST_MARK).contains(&code).then(|| code - FIRST_MARK)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tag {
    Expression,
    Statement,
    Comment,
}

/// `template` with each line that writes text marked — `None` when it already holds a mark character, which
/// could not be told from one.
pub(crate) fn mark_lines(template: &str) -> Option<String> {
    if template.chars().any(|c| line_of(c).is_some()) {
        return None;
    }
    let mut out = String::with_capacity(template.len() + template.len() / 8);
    let mut tag = None;
    let mut quote = None;
    for (index, line) in template.split_inclusive('\n').enumerate() {
        let body = line.trim_start_matches([' ', '\t']);
        out.push_str(&line[..line.len() - body.len()]);
        if tag.is_none() && writes_text(body) {
            if let Some(mark) = u32::try_from(index + 1).ok().and_then(mark_of) {
                out.push(mark);
            }
        }
        out.push_str(body);
        scan(body, &mut tag, &mut quote);
    }
    Some(out)
}

/// A line, past its indentation, that a mark can go in front of — see the module notes.
fn writes_text(body: &str) -> bool {
    let content = body.trim_end();
    !content.is_empty() && !content.starts_with("{%") && !content.starts_with("{#") && !content.starts_with("{{-")
}

/// Follow `text` in and out of tags, so a line that begins inside one — a `{# … #}` over several lines — is
/// not marked. Bytes, not characters: every delimiter is ASCII, and no byte of a multi-byte character is.
fn scan(text: &str, tag: &mut Option<Tag>, quote: &mut Option<u8>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &bytes[i..];
        match (*tag, *quote) {
            (None, _) => {
                let opened = match rest {
                    [b'{', b'{', ..] => Some(Tag::Expression),
                    [b'{', b'%', ..] => Some(Tag::Statement),
                    [b'{', b'#', ..] => Some(Tag::Comment),
                    _ => None,
                };
                if opened.is_some() {
                    *tag = opened;
                    i += 2;
                    continue;
                }
            }
            (Some(Tag::Comment), _) => {
                if rest.starts_with(b"#}") {
                    *tag = None;
                    i += 2;
                    continue;
                }
            }
            (Some(_), Some(open)) => {
                if rest[0] == b'\\' {
                    i += 2;
                    continue;
                }
                if rest[0] == open {
                    *quote = None;
                }
            }
            (Some(open), None) => {
                let close: &[u8] = if open == Tag::Expression { b"}}" } else { b"%}" };
                if rest.starts_with(close) {
                    *tag = None;
                    i += 2;
                    continue;
                }
                if matches!(rest[0], b'"' | b'\'') {
                    *quote = Some(rest[0]);
                }
            }
        }
        i += 1;
    }
}

/// `output` without its marks, and for each of its lines the template line that wrote it — the first mark on
/// the line, `0` for a line with none.
pub(crate) fn unmark_lines(output: &str) -> (String, Vec<u32>) {
    let mut text = String::with_capacity(output.len());
    let mut lines = Vec::new();
    for line in output.split_inclusive('\n') {
        let mut source = 0;
        for c in line.chars() {
            match line_of(c) {
                Some(number) if source == 0 => source = number,
                Some(_) => {}
                None => text.push(c),
            }
        }
        lines.push(source);
    }
    (text, lines)
}

/// The map of `from` carried over to `to` — the same lines re-indented, or with lines of their own around
/// them, which is what a template's members look like once placed in a class.
///
/// Each non-blank line of `to` takes the map of the next line of `from` with the same content, so a line that
/// is not from the template maps to nothing and does not pull the rest out of step. A blank line maps to
/// nothing: there are too many of them to match one by content.
pub fn carry_lines(from: &str, lines: &[u32], to: &str) -> Vec<u32> {
    if from == to {
        return lines.to_vec();
    }
    let sources: Vec<&str> = from.split_inclusive('\n').map(str::trim).collect();
    let mut next = 0;
    to.split_inclusive('\n')
        .map(|line| {
            let wanted = line.trim();
            if wanted.is_empty() {
                return 0;
            }
            match sources[next.min(sources.len())..].iter().position(|source| *source == wanted) {
                Some(skipped) => {
                    let at = next + skipped;
                    next = at + 1;
                    lines.get(at).copied().unwrap_or(0)
                }
                None => 0,
            }
        })
        .collect()
}

/// One template rendered while lines were traced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineTrace {
    pub template: String,
    pub output: String,
    /// For each line of `output`, the 1-based template line that wrote it; `0` for none.
    pub lines: Vec<u32>,
}

thread_local! {
    static TRACES: RefCell<Option<Vec<LineTrace>>> = const { RefCell::new(None) };
}

/// Run `f`, tracing the lines of every template it renders on this thread.
///
/// A scope rather than an argument because the renders happen deep inside each kind — its data, its
/// placement in a class — and none of that has anything to say about lines. Only a preview asks, and it pays
/// a second render of each template for as long as it does.
pub fn trace_lines<R>(f: impl FnOnce() -> R) -> (R, Vec<LineTrace>) {
    /// Ends the trace however `f` ends, a panic included, so the thread does not keep rendering twice.
    struct Scope;
    impl Drop for Scope {
        fn drop(&mut self) {
            TRACES.with(|traces| {
                traces.borrow_mut().take();
            });
        }
    }
    TRACES.with(|traces| *traces.borrow_mut() = Some(Vec::new()));
    let scope = Scope;
    let result = f();
    let traces = TRACES.with(|traces| traces.borrow_mut().take()).unwrap_or_default();
    drop(scope);
    (result, traces)
}

pub(crate) fn tracing() -> bool {
    TRACES.with(|traces| traces.borrow().is_some())
}

pub(crate) fn record(trace: LineTrace) {
    TRACES.with(|traces| {
        if let Some(traces) = traces.borrow_mut().as_mut() {
            traces.push(trace);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::render;
    use serde_json::json;

    fn traced(template: &str, context: serde_json::Value) -> (String, Option<Vec<u32>>) {
        let (text, traces) = trace_lines(|| render(template, &context));
        let lines = traces.into_iter().find(|t| t.template == template).map(|t| t.lines);
        (text.unwrap(), lines)
    }

    #[test]
    fn a_line_in_a_loop_is_the_source_of_every_line_it_writes() {
        let template = "a\n{% for x in items %}\n  item {{ x }}\n{% endfor %}\nb\n";
        let (text, lines) = traced(template, json!({ "items": [1, 2] }));
        assert_eq!(text, "a\n  item 1\n  item 2\nb\n");
        assert_eq!(lines, Some(vec![1, 3, 3, 5]));
    }

    /// The trims are the reason marks go after the indentation and not on some lines at all.
    #[test]
    fn whitespace_control_renders_the_same_with_the_marks_in() {
        let template = "start\n{% if true -%}\n    inside\n{%- endif %}\nend {{ n }}\n";
        let (text, lines) = traced(template, json!({ "n": 1 }));
        assert_eq!(text, "start\ninsideend 1\n");
        assert_eq!(lines, Some(vec![1, 3]));

        let (text, lines) = traced("a\n\n{{- n }}\n", json!({ "n": 1 }));
        assert_eq!(text, "a1\n");
        assert_eq!(lines, Some(vec![1]));
    }

    #[test]
    fn a_render_the_marks_would_change_has_no_map_rather_than_a_wrong_one() {
        let template = "{% set body %}\nab\n{% endset %}{{ body | length }}\n";
        let (text, lines) = traced(template, json!({}));
        assert_eq!(text, "3\n");
        assert_eq!(lines, None);
    }

    #[test]
    fn a_line_that_begins_inside_a_tag_is_not_marked() {
        let marked = mark_lines("{{ a\n  ~ b }}\nc\n").unwrap();
        assert_eq!(marked, "\u{F0001}{{ a\n  ~ b }}\n\u{F0003}c\n");
        assert_eq!(mark_lines("already \u{F0002} here"), None);
    }

    #[test]
    fn the_map_follows_members_re_indented_into_a_class() {
        let from = "x {\n  y\n}\n";
        let to = "\n    x {\n      y\n    }\n";
        assert_eq!(carry_lines(from, &[1, 2, 3], to), [0, 1, 2, 3]);
    }

    #[test]
    fn nothing_is_traced_outside_a_trace() {
        assert!(!tracing());
        let _ = trace_lines(|| assert!(tracing()));
        assert!(!tracing());
    }
}
