//! ANSI escapes → plain text plus the style runs over it.
//!
//! A program's stdout is not plain text. Anything using jansi, logback's colour layout, or
//! a logger someone forced colour on writes SGR escapes, and a console that renders them
//! literally shows `[32m` in front of every line — which reads as the program being broken.
//! So the sequences are either turned into style or dropped, never printed.
//!
//! **Deliberately partial.** SGR (the colour/weight codes) becomes [`Style`]; every other
//! escape is *discarded*. Honouring the rest means being a terminal emulator: a screen
//! buffer, a cursor, reflow on resize. What this produces is a transcript of what a program
//! printed, and a transcript has no cursor to move — a progress bar that redraws itself with
//! `\r` therefore appears as its successive states rather than animating in place, which is
//! the honest rendering of the same bytes.
//!
//! Style does not carry across the call: a colour opened on one line and never closed does
//! not leak into the rest of the console. Opening a colour, printing a level and closing it
//! is the overwhelmingly common shape and is fully served by that.

use crate::model::{Colour, Style};

/// One run of [`Style::default`]-or-otherwise over the *stripped* text. Runs are
/// contiguous, in order, and cover the whole string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleRun {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

/// The escape character. Never written literally in this file — a raw control byte in a
/// source file is invisible in every diff it ever appears in.
const ESC: char = '\u{1b}';
/// The bell, which is one of the two things that can end an OSC sequence.
const BEL: char = '\u{7}';

/// Strip every escape from `raw`, returning the plain text and the style runs over it.
///
/// The runs cover the text completely (there is always at least one when the text is
/// non-empty), so a caller can index into them without a fallback case.
pub fn strip(raw: &str) -> (String, Vec<StyleRun>) {
    // Fast path — the overwhelming majority of lines carry no escapes at all.
    if !raw.contains(ESC) {
        let runs = if raw.is_empty() {
            Vec::new()
        } else {
            vec![StyleRun { start: 0, end: raw.len(), style: Style::default() }]
        };
        return (raw.to_string(), runs);
    }

    let mut text = String::with_capacity(raw.len());
    let mut runs: Vec<StyleRun> = Vec::new();
    let mut style = Style::default();
    let mut run_start = 0usize;

    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < raw.len() {
        if bytes[i] != 0x1b {
            // Copy one character (not one byte — `raw` is UTF-8 and may be anything).
            let c = raw[i..].chars().next().unwrap_or(ESC);
            text.push(c);
            i += c.len_utf8();
            continue;
        }
        // An escape ends the current run before it changes anything.
        let (next, sgr) = read_escape(raw, i);
        if let Some(params) = sgr {
            let new_style = apply(style, params);
            if new_style != style {
                if text.len() > run_start {
                    runs.push(StyleRun { start: run_start, end: text.len(), style });
                }
                run_start = text.len();
                style = new_style;
            }
        }
        i = next;
    }
    if text.len() > run_start {
        runs.push(StyleRun { start: run_start, end: text.len(), style });
    }
    (text, runs)
}

/// Consume the escape sequence starting at `at` (which is an ESC byte). Returns where it
/// ends, and its parameter string when it was an SGR (`ESC [ … m`) — the only kind that
/// carries style. Everything else is matched purely so it can be removed.
fn read_escape(raw: &str, at: usize) -> (usize, Option<&str>) {
    let rest = &raw[at + 1..];
    let mut chars = rest.char_indices();
    let Some((_, first)) = chars.next() else {
        // A trailing lone ESC. Swallow it.
        return (raw.len(), None);
    };
    match first {
        // CSI — `ESC [ params letter`.
        '[' => {
            let params_at = at + 2;
            for (off, c) in raw[params_at..].char_indices() {
                if c.is_ascii_alphabetic() {
                    let end = params_at + off + c.len_utf8();
                    let params = &raw[params_at..params_at + off];
                    return (end, (c == 'm').then_some(params));
                }
                // A parameter byte. Anything outside the CSI parameter/intermediate range
                // means the sequence was never terminated — bail rather than eat the line.
                if !matches!(c, '0'..='9' | ';' | ':' | '?' | '<' | '=' | '>' | ' ' | '!') {
                    return (params_at + off, None);
                }
            }
            (raw.len(), None)
        }
        // OSC — `ESC ] … BEL` or `ESC ] … ESC \`.
        ']' => {
            let body_at = at + 2;
            let mut i = body_at;
            while i < raw.len() {
                let c = raw[i..].chars().next().unwrap_or(BEL);
                if c == BEL {
                    return (i + c.len_utf8(), None);
                }
                if c == ESC && raw[i + 1..].starts_with('\\') {
                    return (i + 2, None);
                }
                i += c.len_utf8();
            }
            (raw.len(), None)
        }
        // A two-character escape.
        _ => (at + 1 + first.len_utf8(), None),
    }
}

/// Fold one SGR parameter list onto a style. Unknown codes are ignored — a background
/// colour is dropped on purpose (a console with its own background is not a terminal, and a
/// program painting one over the theme is the thing you least want honoured).
fn apply(mut style: Style, params: &str) -> Style {
    // An empty parameter list means `0` — reset.
    if params.is_empty() {
        return Style::default();
    }
    for raw in params.split(';') {
        let code: u16 = match raw.trim().parse() {
            Ok(n) => n,
            // `ESC[m` and `ESC[;m` both mean reset.
            Err(_) if raw.trim().is_empty() => 0,
            Err(_) => continue,
        };
        match code {
            0 => style = Style::default(),
            1 => style.bold = true,
            22 => style.bold = false,
            39 => style.colour = None,
            30..=37 => style.colour = colour_of(code - 30),
            90..=97 => style.colour = colour_of(code - 90),
            _ => {}
        }
    }
    style
}

fn colour_of(n: u16) -> Option<Colour> {
    Some(match n {
        0 => Colour::Black,
        1 => Colour::Red,
        2 => Colour::Green,
        3 => Colour::Yellow,
        4 => Colour::Blue,
        5 => Colour::Magenta,
        6 => Colour::Cyan,
        7 => Colour::White,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Escapes are built from char codes rather than written literally: a raw control byte
    /// in a source file is invisible in every diff, and a test you cannot read is a test
    /// nobody maintains.
    fn esc(body: &str) -> String {
        format!("{ESC}{body}")
    }

    #[test]
    fn plain_text_is_one_default_run() {
        let (text, runs) = strip("hello");
        assert_eq!(text, "hello");
        assert_eq!(runs, vec![StyleRun { start: 0, end: 5, style: Style::default() }]);
    }

    #[test]
    fn a_colour_becomes_a_run_and_the_codes_disappear() {
        let raw = format!("{}INFO{} ready", esc("[32m"), esc("[0m"));
        let (text, runs) = strip(&raw);
        assert_eq!(text, "INFO ready");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], StyleRun { start: 0, end: 4, style: Style { colour: Some(Colour::Green), bold: false } });
        assert_eq!(runs[1].style, Style::default());
    }

    #[test]
    fn bright_and_normal_are_the_same_hue() {
        let (_, bright) = strip(&format!("{}x", esc("[91m")));
        let (_, normal) = strip(&format!("{}x", esc("[31m")));
        assert_eq!(bright[0].style, normal[0].style);
    }

    #[test]
    fn bold_composes_with_colour_and_clears_on_reset() {
        let raw = format!("{}{}loud{}quiet", esc("[1m"), esc("[31m"), esc("[0m"));
        let (text, runs) = strip(&raw);
        assert_eq!(text, "loudquiet");
        assert_eq!(runs[0].style, Style { colour: Some(Colour::Red), bold: true });
        assert_eq!(runs[1].style, Style::default());
    }

    #[test]
    fn a_background_colour_is_dropped_rather_than_honoured() {
        let (text, runs) = strip(&format!("{}x", esc("[41m")));
        assert_eq!(text, "x");
        assert_eq!(runs[0].style, Style::default());
    }

    #[test]
    fn cursor_movement_and_titles_are_removed_without_styling_anything() {
        let raw = format!("{}{}done", esc("[2K"), esc("]0;a title\u{7}"));
        let (text, runs) = strip(&raw);
        assert_eq!(text, "done");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].style, Style::default());
    }

    #[test]
    fn an_unterminated_escape_does_not_eat_the_line() {
        // A CSI cut off by the end of the chunk. What came before it is still a line.
        let raw = format!("oops{}", esc("[3"));
        let (text, _) = strip(&raw);
        assert_eq!(text, "oops");
    }

    #[test]
    fn non_ascii_text_survives_intact() {
        let raw = format!("{}città più però{}", esc("[33m"), esc("[0m"));
        let (text, runs) = strip(&raw);
        assert_eq!(text, "città più però");
        assert_eq!(runs[0].style.colour, Some(Colour::Yellow));
        // The run must end on a char boundary, or slicing it would panic.
        assert!(text.is_char_boundary(runs[0].end));
    }
}
