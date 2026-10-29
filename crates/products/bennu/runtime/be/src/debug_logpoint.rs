//! Logpoints: a breakpoint that prints instead of stopping.
//!
//! A hit of a breakpoint with a `log_message` does not suspend the program for the user to look at —
//! it renders the message from the frame it stopped in, writes it to the run console and lets the
//! VM go on. The point is to observe a hot line (a loop, a request handler under load) without
//! freezing it, which is also what makes it the assistant's default way to look at something it
//! does not need to stand in front of.
//!
//! The message is plain text with `{path}` placeholders. A path is the same thing a watch accepts
//! (`order.id`, `items[0].price`) and is read the same way, so there is one grammar for "a value in
//! this frame". `{{` and `}}` are literal braces. A placeholder that cannot be read prints the
//! reason in its place (`{order.id: no variable named `order` here}`) rather than hiding it: a
//! logpoint that silently prints nothing is indistinguishable from a line that never runs.

use bennu_jdwp::prelude::{Frame, Id};

use crate::debug::Session;

/// How much of one rendered message is kept. A logpoint on a hot line must not be able to flood the
/// console with a megabyte per hit because a placeholder resolved to a big string.
const MAX_MESSAGE_CHARS: usize = 400;

/// A piece of a message template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    Text(String),
    Path(String),
}

/// Cut a template into text and `{path}` placeholders.
pub(crate) fn split_template(template: &str) -> Vec<Piece> {
    let chars: Vec<char> = template.chars().collect();
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' if chars.get(i + 1) == Some(&'{') => {
                text.push('{');
                i += 2;
            }
            '}' if chars.get(i + 1) == Some(&'}') => {
                text.push('}');
                i += 2;
            }
            '{' => match chars[i + 1..].iter().position(|c| *c == '}') {
                Some(len) => {
                    let inner: String = chars[i + 1..i + 1 + len].iter().collect();
                    let inner = inner.trim();
                    if inner.is_empty() || inner.contains('{') {
                        // Not a placeholder: keep it as written.
                        text.push('{');
                        i += 1;
                        continue;
                    }
                    if !text.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut text)));
                    }
                    pieces.push(Piece::Path(inner.to_string()));
                    i += len + 2;
                }
                None => {
                    text.push('{');
                    i += 1;
                }
            },
            c => {
                text.push(c);
                i += 1;
            }
        }
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    pieces
}

/// Cut a rendered line to [`MAX_MESSAGE_CHARS`], saying so.
pub(crate) fn clip_message(line: String) -> String {
    if line.chars().count() <= MAX_MESSAGE_CHARS {
        return line;
    }
    let kept: String = line.chars().take(MAX_MESSAGE_CHARS).collect();
    format!("{kept}…")
}

/// Render `template` against `frame` of `thread`.
pub(crate) fn render(session: &Session, thread: Id, frame: &Frame, template: &str) -> String {
    let mut out = String::new();
    for piece in split_template(template) {
        match piece {
            Piece::Text(t) => out.push_str(&t),
            Piece::Path(path) => match crate::debug_value::render_path(session, thread, frame, &path) {
                Ok(value) => out.push_str(&value),
                Err(why) => out.push_str(&format!("{{{path}: {why}}}")),
            },
        }
    }
    clip_message(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Piece {
        Piece::Text(s.to_string())
    }
    fn path(s: &str) -> Piece {
        Piece::Path(s.to_string())
    }

    #[test]
    fn plain_text_is_one_piece() {
        assert_eq!(split_template("reached the loop"), vec![text("reached the loop")]);
        assert!(split_template("").is_empty());
    }

    #[test]
    fn placeholders_are_cut_out_of_the_text() {
        assert_eq!(
            split_template("order {order.id} has {items[0].qty} items"),
            vec![text("order "), path("order.id"), text(" has "), path("items[0].qty"), text(" items")]
        );
        assert_eq!(split_template("{ x }"), vec![path("x")], "spaces inside braces are not part of the path");
    }

    #[test]
    fn doubled_braces_are_literal() {
        assert_eq!(split_template("a {{b}} c"), vec![text("a {b} c")]);
        assert_eq!(split_template("{{{x}}}"), vec![text("{"), path("x"), text("}")]);
    }

    #[test]
    fn a_brace_that_opens_nothing_stays_as_written() {
        assert_eq!(split_template("open { never closed"), vec![text("open { never closed")]);
        assert_eq!(split_template("empty {} here"), vec![text("empty {} here")]);
        assert_eq!(split_template("{a{b}"), vec![text("{a"), path("b")]);
    }

    #[test]
    fn a_long_message_is_cut_and_says_so() {
        let long = "x".repeat(MAX_MESSAGE_CHARS + 50);
        let cut = clip_message(long);
        assert_eq!(cut.chars().count(), MAX_MESSAGE_CHARS + 1);
        assert!(cut.ends_with('…'));
        assert_eq!(clip_message("short".into()), "short");
    }
}
