//! Recognising a line by the **layout that produced it** — a Logback / Log4j conversion pattern.
//!
//! The shape rules in [`crate::common`] and [`crate::java`] recognise a log line by what its
//! words look like, which is all anyone can do when the format is unknown, and which is why they
//! decline so much: `OrderService` on its own is a word, and `tenant=acme` is nothing. A project
//! that *says* its format — `%d{yyyy-MM-dd HH:mm:ss} | %-5level | %logger{1} | MDC:[tenant=%X{tenant.id}] | %msg%n`
//! — has said where the date is, where the level is, which word is the class and which values are
//! MDC. Reading the pattern turns every one of those from a guess into a fact, so a bare
//! `OrderService` in the logger column is a [`Token::Logger`] and what follows `tenant=` is a
//! [`Token::Mdc`].
//!
//! ## How a line is matched
//!
//! The pattern is cut into literals (`" | "`, `"MDC:[tenant="`) and fields (`%d`, `%-5level`,
//! `%X{tenant.id}`). A field has no fixed width, so it runs up to the next literal — except a
//! date whose format has a fixed length, and a level, which is a word. Padding (`%-5level`) is
//! whitespace the literal around it absorbs. The `%msg` field ends the match: **what the layout
//! recognises is the prefix**, and the message is left to the ordinary rules so that a URL or a
//! class name inside it is still found.
//!
//! The matcher is only asked at the start of a line and **declines** whenever the line is not in
//! the layout — a stack frame, a continuation, a line some other library printed — leaving it to
//! the shape rules. A layout that uses something this does not understand (`%replace`, a
//! conditional) does not parse at all, and the console falls back to the shape rules entirely.

use crate::model::{Link, Token};
use crate::rule::{Hit, Part, Rule, RuleSet};

/// A parsed conversion pattern, usable as a [`Rule`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternLayout {
    elements: Vec<Element>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Element {
    Literal(String),
    Field(Kind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A date; the length is known when the format has one.
    Date(Option<usize>),
    Level,
    Thread,
    Logger,
    Mdc,
    /// `%msg` — where the recognised prefix ends.
    Message,
    /// Anything else the layout prints (`%line`, `%relative`, …): skipped, not coloured.
    Other,
}

/// Wrappers that only colour what is inside them; their content is the layout.
const COLOUR_WRAPPERS: &[&str] = &[
    "highlight", "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "gray",
    "boldRed", "boldGreen", "boldYellow", "boldBlue", "boldMagenta", "boldCyan", "boldWhite",
];

impl PatternLayout {
    /// Parse a Logback / Log4j conversion pattern. `None` when it uses something this does not
    /// model, or has no field worth recognising.
    pub fn parse(pattern: &str) -> Option<PatternLayout> {
        let mut elements = parse_elements(pattern)?;
        merge_literals(&mut elements);
        let useful = elements
            .iter()
            .any(|e| matches!(e, Element::Field(Kind::Date(_) | Kind::Level | Kind::Thread | Kind::Logger | Kind::Mdc)));
        useful.then_some(PatternLayout { elements })
    }

    /// [`RuleSet::java`] with this layout tried first at the start of every line.
    pub fn over_java(self) -> RuleSet {
        RuleSet::java().first(self)
    }

    /// The parts this layout finds in the prefix of `text`, and where the prefix ends.
    pub fn match_prefix(&self, text: &str) -> Option<(usize, Vec<Part>)> {
        let mut parts: Vec<Part> = Vec::new();
        let mut at = 0usize;
        for (i, element) in self.elements.iter().enumerate() {
            match element {
                Element::Literal(literal) => {
                    at = skip_literal(text, at, literal)?;
                    // `tenant=` in front of an MDC field: the key is as much the layout's as the value.
                    if matches!(self.elements.get(i + 1), Some(Element::Field(Kind::Mdc))) {
                        parts.extend(key_before(text, at));
                    }
                }
                Element::Field(Kind::Message) => break,
                Element::Field(kind) => {
                    let next = self.elements.get(i + 1);
                    let (start, end) = field_extent(text, at, *kind, next)?;
                    if let Some(part) = part_of(text, start, end, *kind) {
                        parts.push(part);
                    }
                    at = end;
                }
            }
        }
        (at > 0).then_some((at, parts))
    }
}

impl Rule for PatternLayout {
    fn name(&self) -> &'static str {
        "layout"
    }

    fn match_at(&self, text: &str, at: usize) -> Option<Hit> {
        if at != 0 {
            return None;
        }
        let (end, parts) = self.match_prefix(text)?;
        Some(Hit { end, parts })
    }
}

// ── parsing ─────────────────────────────────────────────────────────────────────

fn parse_elements(pattern: &str) -> Option<Vec<Element>> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out: Vec<Element> = Vec::new();
    let mut literal = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            literal.push(chars[i]);
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'%') {
            literal.push('%');
            i += 2;
            continue;
        }
        if !literal.is_empty() {
            out.push(Element::Literal(std::mem::take(&mut literal)));
        }
        i += 1;
        // Format modifiers: `-5`, `.30`, `-5.30` — padding and truncation, which change the width
        // of the value and nothing about where it is.
        while i < chars.len() && (chars[i] == '-' || chars[i] == '.' || chars[i].is_ascii_digit()) {
            i += 1;
        }
        let word_start = i;
        while i < chars.len() && chars[i].is_alphabetic() {
            i += 1;
        }
        let word: String = chars[word_start..i].iter().collect();
        if word.is_empty() {
            return None;
        }
        let mut option: Option<String> = None;
        while chars.get(i) == Some(&'{') {
            let close = matching(&chars, i, '{', '}')?;
            if option.is_none() {
                option = Some(chars[i + 1..close].iter().collect());
            }
            i = close + 1;
        }
        if chars.get(i) == Some(&'(') {
            let close = matching(&chars, i, '(', ')')?;
            if !COLOUR_WRAPPERS.contains(&word.as_str()) {
                return None; // `%replace(…)`, `%style(…)`: it changes the text, so it cannot be matched
            }
            let inner: String = chars[i + 1..close].iter().collect();
            out.extend(parse_elements(&inner)?);
            i = close + 1;
            continue;
        }
        match kind_of(&word, option.as_deref()) {
            Some(Some(kind)) => out.push(Element::Field(kind)),
            Some(None) => {} // `%n`: the end of the line
            None => return None,
        }
    }
    if !literal.is_empty() {
        out.push(Element::Literal(literal));
    }
    Some(out)
}

/// The field a conversion word stands for. `Some(None)` is a word that prints nothing here
/// (`%n`); `None` is one this refuses to guess about.
fn kind_of(word: &str, option: Option<&str>) -> Option<Option<Kind>> {
    Some(Some(match word {
        "d" | "date" => Kind::Date(option.and_then(date_length).or_else(|| option.is_none().then_some(23))),
        "p" | "le" | "level" => Kind::Level,
        "t" | "thread" => Kind::Thread,
        "c" | "lo" | "logger" | "C" | "class" => Kind::Logger,
        "X" | "mdc" | "MDC" => Kind::Mdc,
        "m" | "msg" | "message" => Kind::Message,
        "n" => return Some(None),
        "r" | "relative" | "L" | "line" | "F" | "file" | "M" | "method" | "l" | "caller" | "u" | "sn"
        | "sequenceNumber" | "pid" | "marker" | "K" | "map" | "i" | "processId" => Kind::Other,
        // The exception: printed after the message, so past the recognised prefix anyway.
        "ex" | "exception" | "throwable" | "rEx" | "rootException" | "xEx" | "xException" | "xThrowable" => {
            Kind::Other
        }
        _ => return None,
    }))
}

/// The printed length of a `SimpleDateFormat` pattern, when it is fixed. `None` for a month name,
/// a zone, or anything whose width depends on the value.
fn date_length(format: &str) -> Option<usize> {
    match format {
        "ISO8601" => return Some(23),
        "ABSOLUTE" => return Some(12),
        "DATE" | "COMPACT" | "DEFAULT" => return None,
        _ => {}
    }
    let chars: Vec<char> = format.chars().collect();
    let mut len = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            // `''` is one quote; `'…'` is literal text, counted without its quotes.
            if chars.get(i + 1) == Some(&'\'') {
                len += 1;
                i += 2;
                continue;
            }
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                len += 1;
                i += 1;
            }
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() {
            let run = chars[i..].iter().take_while(|&&x| x == c).count();
            // Text months, zones and weekdays vary in width.
            if matches!(c, 'X' | 'Z' | 'z' | 'E' | 'a') || (c == 'M' && run >= 3) {
                return None;
            }
            len += run;
            i += run;
            continue;
        }
        len += 1;
        i += 1;
    }
    (len > 0).then_some(len)
}

fn matching(chars: &[char], open_at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, &c) in chars[open_at..].iter().enumerate() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(open_at + offset);
            }
        }
    }
    None
}

/// Two literals side by side (what a skipped `%n` or a dropped wrapper leaves) are one.
fn merge_literals(elements: &mut Vec<Element>) {
    let mut merged: Vec<Element> = Vec::with_capacity(elements.len());
    for element in elements.drain(..) {
        match (merged.last_mut(), element) {
            (Some(Element::Literal(prev)), Element::Literal(next)) => prev.push_str(&next),
            (_, element) => merged.push(element),
        }
    }
    *elements = merged;
}

// ── matching ────────────────────────────────────────────────────────────────────

/// Step over `literal` at `at`. Whitespace around it is flexible, because a padded field
/// (`%-5level`) leaves a variable amount of it; the visible text between must match exactly.
fn skip_literal(text: &str, at: usize, literal: &str) -> Option<usize> {
    let core = literal.trim();
    let mut q = at;
    while text[q..].starts_with(' ') {
        q += 1;
    }
    if core.is_empty() {
        // A literal of only spaces needs at least one — or the padding the field ended on.
        return (q > at || literal.is_empty()).then_some(q);
    }
    if !text[q..].starts_with(core) {
        return None;
    }
    q += core.len();
    if literal.ends_with(' ') {
        while text[q..].starts_with(' ') {
            q += 1;
        }
    }
    Some(q)
}

/// Where the value of a field of `kind` starts and ends, `next` being what follows it.
fn field_extent(text: &str, at: usize, kind: Kind, next: Option<&Element>) -> Option<(usize, usize)> {
    let mut start = at;
    // Padding in front of a right-aligned value. Not for MDC: its value may be empty, and the
    // space in front of the NEXT literal must stay where `value_end` will look for it.
    if !matches!(kind, Kind::Mdc | Kind::Other) {
        while text[start..].starts_with(' ') {
            start += 1;
        }
    }
    let end = match kind {
        Kind::Date(Some(len)) => {
            let end = text[start..].char_indices().nth(len).map_or(text.len(), |(i, _)| start + i);
            let shown = &text[start..end];
            if shown.chars().count() < len || !shown.chars().any(|c| c.is_ascii_digit()) {
                return None;
            }
            end
        }
        Kind::Level => {
            let word_len = text[start..].bytes().take_while(u8::is_ascii_alphabetic).count();
            if word_len == 0 || crate::common::level_of(&text[start..start + word_len]).is_none() {
                return None;
            }
            start + word_len
        }
        _ => value_end(text, start, next)?,
    };
    let value = &text[start..end];
    let end = start + value.trim_end().len();
    // An empty MDC value is normal; an empty anything else means the line is not in this layout.
    (end > start || matches!(kind, Kind::Mdc | Kind::Other)).then_some((start, end))
}

/// A value runs up to the delimiter that follows it; with no delimiter it runs to the first
/// whitespace, and as the last element to the end of the line.
fn value_end(text: &str, start: usize, next: Option<&Element>) -> Option<usize> {
    match next {
        Some(Element::Literal(literal)) => {
            let core = literal.trim();
            if core.is_empty() {
                let rest = &text[start..];
                return Some(start + rest.find(char::is_whitespace).unwrap_or(rest.len()));
            }
            // The literal as written first (`" | "` is far less likely to occur inside a value
            // than `"|"`), then without its leading padding.
            text[start..]
                .find(literal.as_str())
                .or_else(|| text[start..].find(literal.trim_start()))
                .map(|i| start + i)
        }
        Some(Element::Field(_)) => {
            let rest = &text[start..];
            Some(start + rest.find(char::is_whitespace).unwrap_or(rest.len()))
        }
        None => Some(text.len()),
    }
}

fn part_of(text: &str, start: usize, end: usize, kind: Kind) -> Option<Part> {
    if end <= start {
        return None;
    }
    let value = &text[start..end];
    let (token, link) = match kind {
        Kind::Date(_) => (Token::Timestamp, None),
        Kind::Level => (Token::Level, None),
        Kind::Thread => (Token::Thread, None),
        Kind::Mdc => (Token::Mdc, None),
        Kind::Logger => (
            Token::Logger,
            is_class_name(value).then(|| Link::Source {
                class: value.to_string(),
                method: None,
                file: None,
                line: None,
            }),
        ),
        Kind::Message | Kind::Other => return None,
    };
    Some(Part { start, end, token, link })
}

/// The MDC key written right before `at`, when what precedes is `key=`.
fn key_before(text: &str, at: usize) -> Option<Part> {
    let head = &text[..at];
    let equals = head.len().checked_sub(1).filter(|&i| head.as_bytes()[i] == b'=')?;
    let start = head[..equals]
        .rfind(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.' | '-')))
        .map_or(0, |i| i + head[i..].chars().next().map_or(1, char::len_utf8));
    (start < equals).then(|| Part { start, end: equals, token: Token::MdcKey, link: None })
}

/// `OrderService`, `com.acme.OrderService`, `c.a.o.OrderService` — dotted identifiers.
fn is_class_name(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|segment| {
            let mut chars = segment.chars();
            chars.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
                && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::interpret;

    const PATTERN: &str = "%d{yyyy-MM-dd HH:mm:ss} | %-5level | %logger{1} | MDC:[tenant=%X{tenant.id},trace_id=%X{Trace-Id},username=%X{X-USERNAME},delegate=%X{X-DELEGATE}] | %msg%n";
    const LINE: &str = "2026-10-28 14:03:22 | ERROR | OrderService | MDC:[tenant=acme,trace_id=abc123,username=mario,delegate=] | Could not place order";

    fn layout() -> PatternLayout {
        PatternLayout::parse(PATTERN).expect("the example layout parses")
    }

    fn found(line: &str) -> Vec<(String, Token)> {
        let parsed = interpret(&layout().over_java(), line);
        parsed.spans.iter().map(|s| (parsed.text[s.start..s.end].to_string(), s.token)).collect()
    }

    #[test]
    fn the_example_layout_says_where_everything_is() {
        let got = found(LINE);
        assert_eq!(got[0], ("2026-10-28 14:03:22".into(), Token::Timestamp));
        assert_eq!(got[1], ("ERROR".into(), Token::Level));
        assert_eq!(got[2], ("OrderService".into(), Token::Logger));
        let mdc: Vec<&str> = got.iter().filter(|(_, t)| *t == Token::Mdc).map(|(s, _)| s.as_str()).collect();
        assert_eq!(mdc, ["acme", "abc123", "mario"], "the empty delegate has nothing to colour");
    }

    #[test]
    fn the_keys_in_front_of_the_mdc_values_are_marked_too() {
        let keys: Vec<String> =
            found(LINE).into_iter().filter(|(_, t)| *t == Token::MdcKey).map(|(s, _)| s).collect();
        assert_eq!(keys, ["tenant", "trace_id", "username", "delegate"]);
    }

    #[test]
    fn the_level_of_the_line_comes_from_the_layout() {
        let parsed = interpret(&layout().over_java(), LINE);
        assert_eq!(parsed.level, Some(crate::model::Level::Error));
    }

    /// A bare class name is not something the shape rules would ever mark; the layout knows.
    #[test]
    fn a_bare_logger_is_linked_to_its_class() {
        let parsed = interpret(&layout().over_java(), LINE);
        let link = parsed.spans.iter().find(|s| s.token == Token::Logger).and_then(|s| s.link.clone());
        assert_eq!(
            link,
            Some(Link::Source { class: "OrderService".into(), method: None, file: None, line: None })
        );
    }

    /// What follows the prefix is still read by the ordinary rules.
    #[test]
    fn the_message_is_still_scanned_for_urls_and_names() {
        let line = LINE.replace("Could not place order", "see https://acme.test/x and com.acme.Order#total");
        let got = found(&line);
        assert!(got.iter().any(|(s, t)| s == "https://acme.test/x" && *t == Token::Url), "{got:?}");
        assert!(got.iter().any(|(s, t)| s == "com.acme.Order#total" && *t == Token::Package), "{got:?}");
    }

    #[test]
    fn a_line_outside_the_layout_falls_back_to_the_shape_rules() {
        let got = found("\tat com.acme.Order.total(Order.java:118)");
        assert_eq!(got[0].1, Token::Package);
        assert!(got.iter().all(|(_, t)| !matches!(t, Token::Logger | Token::Mdc)));
        assert!(found("just some words").is_empty());
    }

    #[test]
    fn padding_around_a_short_level_is_absorbed() {
        let got = found("2026-10-28 14:03:22 | INFO  | OrderService | MDC:[tenant=a,trace_id=b,username=c,delegate=d] | hi");
        assert_eq!(got[1], ("INFO".into(), Token::Level));
        assert_eq!(got[2], ("OrderService".into(), Token::Logger));
    }

    #[test]
    fn a_word_that_is_not_a_level_rejects_the_line() {
        let got = found("2026-10-28 14:03:22 | BANANA | OrderService | MDC:[tenant=a,trace_id=b,username=c,delegate=d] | hi");
        assert!(got.iter().all(|(_, t)| *t != Token::Logger));
    }

    #[test]
    fn spring_boots_default_shape_parses_and_matches() {
        let layout = PatternLayout::parse(
            "%d{yyyy-MM-dd HH:mm:ss.SSS} %5p %property{PID} --- [%15.15t] %-40.40logger{39} : %m%n",
        );
        // `%property` is something this does not model, so the whole layout declines rather than
        // mis-reading it.
        assert!(layout.is_none());
    }

    #[test]
    fn a_plain_console_layout_matches_by_whitespace() {
        let layout = PatternLayout::parse("%d{HH:mm:ss.SSS} [%thread] %-5level %logger{36} - %msg%n").unwrap();
        let line = interpret(&layout.over_java(), "14:03:22.123 [main] WARN  c.a.o.OrderService - slow");
        let got: Vec<(String, Token)> =
            line.spans.iter().map(|s| (line.text[s.start..s.end].to_string(), s.token)).collect();
        assert_eq!(got[0], ("14:03:22.123".into(), Token::Timestamp));
        assert_eq!(got[1], ("main".into(), Token::Thread));
        assert_eq!(got[2], ("WARN".into(), Token::Level));
        assert_eq!(got[3], ("c.a.o.OrderService".into(), Token::Logger));
    }

    #[test]
    fn colour_wrappers_are_looked_through() {
        let layout = PatternLayout::parse("%d{HH:mm:ss} %highlight(%-5level) %cyan(%logger{15}) - %msg%n").unwrap();
        let line = interpret(&layout.over_java(), "14:03:22 ERROR OrderService - boom");
        assert!(line.spans.iter().any(|s| s.token == Token::Logger));
        assert_eq!(line.level, Some(crate::model::Level::Error));
    }

    #[test]
    fn date_formats_know_their_width() {
        assert_eq!(date_length("yyyy-MM-dd HH:mm:ss"), Some(19));
        assert_eq!(date_length("yyyy-MM-dd'T'HH:mm:ss.SSS"), Some(23));
        assert_eq!(date_length("HH:mm:ss.SSS"), Some(12));
        assert_eq!(date_length("ISO8601"), Some(23));
        assert_eq!(date_length("dd MMM yyyy"), None, "a month name varies");
        assert_eq!(date_length("yyyy-MM-dd HH:mm:ss.SSSXXX"), None, "a zone varies");
    }

    #[test]
    fn a_pattern_with_nothing_to_recognise_does_not_parse() {
        assert!(PatternLayout::parse("%msg%n").is_none());
        assert!(PatternLayout::parse("").is_none());
        assert!(PatternLayout::parse("%replace(%msg){'a','b'}%n").is_none());
    }

    #[test]
    fn a_percent_literal_is_kept() {
        let layout = PatternLayout::parse("%level 100%% %msg").unwrap();
        let line = interpret(&layout.over_java(), "INFO 100% done");
        assert_eq!(line.spans[0].token, Token::Level);
    }

    #[test]
    fn an_empty_prefix_match_is_declined() {
        // The message first: nothing before it to recognise.
        let layout = PatternLayout::parse("%msg %level").unwrap();
        assert!(layout.match_prefix("hello INFO").is_none());
    }

    #[test]
    fn non_ascii_text_does_not_panic() {
        let line = "2026-10-28 14:03:22 | ERROR | Città | MDC:[tenant=è,trace_id=ü,username=ö,delegate=à] | però";
        let got = found(line);
        assert!(got.iter().any(|(s, t)| s == "Città" && *t == Token::Logger));
    }
}
