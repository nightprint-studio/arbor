//! Putting it together: escapes off, rules over, level decided.
//!
//! [`interpret`] answers about one line on its own. [`LogReader`] is the same thing with a
//! memory of the previous line, which is what a stack trace needs: the twenty frames under
//! an `ERROR` say nothing about their own severity, and reading them as twenty ordinary
//! lines is how a console ends up with one red line and a wall of grey underneath it.
//!
//! One reader per **stream**, not per process: a program's stdout and stderr are interleaved
//! by the operating system in an order neither of them agreed to, and a shared reader would
//! have stdout's chatter interrupting stderr's trace. Two readers cost two `Option<Level>`.

use crate::ansi::{strip, StyleRun};
use crate::common::level_of;
use crate::model::{Level, Line, Span, Token};
use crate::rule::{Part, RuleSet};
use crate::scan::scan;

/// Interpret one line on its own — escapes stripped, rules applied, level read off it.
///
/// The level is what the line itself says: a level word, or — when an exception name is on a
/// line that is not part of a trace already — [`Level::Error`], because a bare
/// `com.acme.FooException: connection refused` is an error whatever the program neglected to
/// prefix it with.
pub fn interpret(rules: &RuleSet, raw: &str) -> Line {
    let (text, runs) = strip(raw);
    let parts = scan(rules, &text);
    let level = level_from(rules, &text, &parts);
    let spans = merge(&runs, parts);
    Line { text, level, spans }
}

fn level_from(rules: &RuleSet, text: &str, parts: &[Part]) -> Option<Level> {
    if let Some(explicit) =
        parts.iter().filter(|p| p.token == Token::Level).find_map(|p| {
            level_of(text[p.start..p.end].trim_matches(|c| c == '[' || c == ']' || c == ':'))
        })
    {
        return Some(explicit);
    }
    // An exception named on a line that starts something is that line's severity. On a
    // continuation it is not: an exception mentioned inside an `INFO` block ("handled
    // FooException gracefully") is still information.
    let names_exception = parts.iter().any(|p| p.token == Token::Exception);
    (names_exception && !rules.is_continuation(text)).then_some(Level::Error)
}

/// Fold the style runs and the recognised parts into one sorted, non-overlapping, **sparse**
/// list of spans: a range appears only when a rule claimed it or the program coloured it.
///
/// Sparse because most of a log is unremarkable text, and a span per word would triple the
/// size of every line for nothing.
fn merge(runs: &[StyleRun], parts: Vec<Part>) -> Vec<Span> {
    // Every boundary either list introduces. Walking the union means a part that straddles a
    // colour change comes out as two spans rather than losing one of the two answers.
    let mut cuts: Vec<usize> = Vec::with_capacity(runs.len() * 2 + parts.len() * 2);
    for r in runs {
        cuts.push(r.start);
        cuts.push(r.end);
    }
    for p in &parts {
        cuts.push(p.start);
        cuts.push(p.end);
    }
    cuts.sort_unstable();
    cuts.dedup();

    let mut out: Vec<Span> = Vec::new();
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let style = runs
            .iter()
            .find(|r| r.start <= start && start < r.end)
            .map(|r| r.style)
            .unwrap_or_default();
        let part = parts.iter().find(|p| p.start <= start && start < p.end);
        let token = part.map(|p| p.token).unwrap_or_default();
        if style.is_plain() && token.is_text() {
            continue; // nothing to say about this stretch
        }
        let link = part.and_then(|p| p.link.clone());
        // Coalesce with the previous span when they are the same answer — a colour that
        // spans a whole line should not arrive as one span per word.
        match out.last_mut() {
            Some(last) if last.end == start && last.token == token && last.style == style && last.link == link => {
                last.end = end;
            }
            _ => out.push(Span { start, end, token, style, link }),
        }
    }
    out
}

/// A stateful line reader: [`interpret`], plus the previous line's level for continuations.
///
/// Cheap to make and cheap to hold — the rules are shared, and the state is one
/// `Option<Level>`.
#[derive(Debug, Clone)]
pub struct LogReader {
    rules: RuleSet,
    level: Option<Level>,
}

impl LogReader {
    pub fn new(rules: RuleSet) -> Self {
        LogReader { rules, level: None }
    }

    /// Interpret the next line of the stream.
    pub fn read(&mut self, raw: &str) -> Line {
        let mut line = interpret(&self.rules, raw);
        if line.level.is_none() && self.rules.is_continuation(&line.text) {
            line.level = self.level;
        }
        // Remembered including the `None`: a plain line ends the trace above it, and a frame
        // fifty lines later must not inherit an error nobody is still looking at.
        self.level = line.level;
        line
    }

    /// Forget the previous line — for a reader being reused across runs.
    pub fn reset(&mut self) {
        self.level = None;
    }

    pub fn rules(&self) -> &RuleSet {
        &self.rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Colour;

    const ESC: char = '\u{1b}';

    #[test]
    fn an_unremarkable_line_has_no_spans_at_all() {
        let line = interpret(&RuleSet::java(), "just some words");
        assert!(line.spans.is_empty());
        assert_eq!(line.pieces().len(), 1);
    }

    #[test]
    fn colour_and_token_live_on_the_same_span() {
        let raw = format!("{ESC}[31mERROR{ESC}[0m boom");
        let line = interpret(&RuleSet::common(), &raw);
        assert_eq!(line.text, "ERROR boom");
        assert_eq!(line.spans[0].token, Token::Level);
        assert_eq!(line.spans[0].style.colour, Some(Colour::Red));
        assert_eq!(line.level, Some(Level::Error));
    }

    #[test]
    fn a_colour_change_inside_a_token_splits_it() {
        // Pathological, but it must not lose either answer.
        let raw = format!("{ESC}[31mERR{ESC}[32mOR{ESC}[0m");
        let line = interpret(&RuleSet::common(), &raw);
        assert_eq!(line.text, "ERROR");
        assert_eq!(line.spans.len(), 2);
        assert!(line.spans.iter().all(|s| s.token == Token::Level));
        assert_eq!(line.spans[0].style.colour, Some(Colour::Red));
        assert_eq!(line.spans[1].style.colour, Some(Colour::Green));
    }

    #[test]
    fn a_colour_over_plain_text_is_one_span_not_one_per_word() {
        let raw = format!("{ESC}[33mthree plain words{ESC}[0m");
        let line = interpret(&RuleSet::common(), &raw);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].token, Token::Text);
    }

    #[test]
    fn pieces_of_any_line_reproduce_it() {
        let rules = RuleSet::java();
        for raw in [
            "",
            "plain",
            "2026-08-05 12:33:01,123 ERROR [main] com.acme.Boot - see https://acme.test/x",
            "\tat com.acme.Order.total(Order.java:118)",
            "città: /tmp/più/x.txt:9 però",
            &format!("{ESC}[32mgreen{ESC}[0m and not"),
        ] {
            let line = interpret(&rules, raw);
            let rebuilt: String = line.pieces().iter().map(|p| p.text).collect();
            assert_eq!(rebuilt, line.text, "for {raw:?}");
        }
    }

    #[test]
    fn spans_never_overlap_and_stay_in_order() {
        let line = interpret(
            &RuleSet::java(),
            "2026-08-05 12:33:01 ERROR [pool-1] com.acme.Svc failed /tmp/a/b.log:3 https://acme.test",
        );
        for pair in line.spans.windows(2) {
            assert!(pair[0].end <= pair[1].start, "{:?}", line.spans);
        }
    }

    #[test]
    fn resetting_forgets_the_trace() {
        let mut reader = LogReader::new(RuleSet::java());
        reader.read("ERROR boom");
        reader.reset();
        assert_eq!(reader.read("\tat com.acme.Order.total(Order.java:118)").level, None);
    }
}
