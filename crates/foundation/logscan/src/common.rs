//! The rules every log has: levels, timestamps, threads, URLs, filesystem paths.
//!
//! Nothing here knows a language or a framework — a Maven log, a `cargo` log, a Python
//! traceback and a shell script all print these. Language-specific recognition lives in its
//! own module ([`crate::java`]) and is layered on top.
//!
//! Each rule is written to **decline** rather than guess. A log viewer that underlines
//! `and/or` as a path or paints the word `Error` in a sentence red teaches the reader to
//! ignore its own highlighting, at which point it has made the log harder to read than
//! plain text would have been.

use crate::model::{Level, Link, Token};
use crate::rule::{FnRule, Hit, RuleSet};
use crate::scan::token_end;

impl RuleSet {
    /// Levels, timestamps, threads, URLs and paths — the set for a log of no particular
    /// language.
    pub fn common() -> Self {
        RuleSet::empty()
            .with(FnRule::new("level", level_rule))
            .with(FnRule::new("timestamp", timestamp_rule))
            .with(FnRule::new("thread", thread_rule))
            .with(FnRule::new("url", url_rule))
            .with(FnRule::new("path", path_rule))
    }
}

// ── levels ──────────────────────────────────────────────────────────────────────

/// The severity a level word names, or `None` when it names none.
///
/// **Upper case only**, and on purpose: `error`, `Error` and `warning` are ordinary English
/// words that appear in messages all the time, and a viewer that paints the whole line red
/// because someone wrote "Error handling enabled" is a viewer whose colours mean nothing. A
/// host whose logs use another dialect adds a rule for it — that is what the rule set is
/// for.
pub fn level_of(word: &str) -> Option<Level> {
    Some(match word {
        "TRACE" | "FINEST" | "FINER" | "VERBOSE" => Level::Trace,
        "DEBUG" | "FINE" | "CONFIG" => Level::Debug,
        "INFO" | "NOTICE" => Level::Info,
        "WARN" | "WARNING" => Level::Warn,
        "ERROR" | "SEVERE" => Level::Error,
        "FATAL" | "CRITICAL" => Level::Fatal,
        _ => return None,
    })
}

/// `ERROR`, `ERROR:` or `[ERROR]` — bare, punctuated, or bracketed the way Maven writes it.
fn level_rule(text: &str, at: usize) -> Option<Hit> {
    let rest = &text[at..];
    if rest.starts_with('[') {
        let close = rest.find(']')?;
        level_of(&rest[1..close])?;
        // The brackets are part of the level here: `[ERROR]` is one thing on the page.
        return Some(Hit::one(at, at + close + 1, Token::Level));
    }
    let mut end = token_end(text, at);
    // A trailing `:` is punctuation around the word, not part of it.
    while end > at && text[..end].ends_with(':') {
        end -= 1;
    }
    level_of(&text[at..end])?;
    Some(Hit::one(at, end, Token::Level))
}

// ── timestamps ──────────────────────────────────────────────────────────────────

/// `2026-08-05 12:33:01,123`, `2026-08-05T12:33:01.123Z`, or a bare `12:33:01.123`.
fn timestamp_rule(text: &str, at: usize) -> Option<Hit> {
    let b = text.as_bytes();
    let end = date_at(b, at).or_else(|| time_at(b, at))?;
    // `12:33:01x` is not a time; something else starts there.
    if b.get(end).is_some_and(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(Hit::one(at, end, Token::Timestamp))
}

/// `dddd-dd-dd`, plus the time when one follows after `T` or a space.
fn date_at(b: &[u8], at: usize) -> Option<usize> {
    if !(digits(b, at, 4) && b.get(at + 4) == Some(&b'-') && digits(b, at + 5, 2)
        && b.get(at + 7) == Some(&b'-') && digits(b, at + 8, 2))
    {
        return None;
    }
    let end = at + 10;
    match b.get(end) {
        Some(b'T') | Some(b' ') => Some(time_at(b, end + 1).unwrap_or(end)),
        _ => Some(end),
    }
}

/// `dd:dd:dd`, with optional fractional seconds and zone.
fn time_at(b: &[u8], at: usize) -> Option<usize> {
    if !(digits(b, at, 2) && b.get(at + 2) == Some(&b':') && digits(b, at + 3, 2)
        && b.get(at + 5) == Some(&b':') && digits(b, at + 6, 2))
    {
        return None;
    }
    let mut end = at + 8;
    // Fractional seconds: `.123` or the comma logback prints.
    if matches!(b.get(end), Some(b'.') | Some(b',')) && digits(b, end + 1, 1) {
        end += 1;
        while digits(b, end, 1) {
            end += 1;
        }
    }
    // Zone: `Z`, `+02:00` or `+0200`.
    match b.get(end) {
        Some(b'Z') => end += 1,
        Some(b'+') | Some(b'-') => {
            if digits(b, end + 1, 2) && b.get(end + 3) == Some(&b':') && digits(b, end + 4, 2) {
                end += 6;
            } else if digits(b, end + 1, 4) {
                end += 5;
            }
        }
        _ => {}
    }
    Some(end)
}

fn digits(b: &[u8], at: usize, n: usize) -> bool {
    at + n <= b.len() && b[at..at + n].iter().all(u8::is_ascii_digit)
}

// ── threads ─────────────────────────────────────────────────────────────────────

/// `[main]`, `[http-nio-8080-exec-3]` — a bracketed name with nothing structural in it.
///
/// Declines anything dotted or slashed so a bracketed logger (`[com.acme.Foo]`) or a jar
/// coordinate (`~[spring-core-5.3.jar:5.3]`) falls through to the rules that understand it.
fn thread_rule(text: &str, at: usize) -> Option<Hit> {
    let rest = &text[at..];
    if !rest.starts_with('[') {
        return None;
    }
    let close = rest.find(']')?;
    let inner = &rest[1..close];
    if inner.is_empty() || inner.len() > 64 {
        return None;
    }
    if inner.chars().any(|c| c.is_whitespace() || matches!(c, '.' | '/' | '\\' | '[' | ':')) {
        return None;
    }
    // `[42]` is an index, `[x]` is a marker. Neither is a thread.
    if inner.chars().all(|c| c.is_ascii_digit()) || inner.len() < 2 {
        return None;
    }
    Some(Hit::one(at, at + close + 1, Token::Thread))
}

// ── URLs ────────────────────────────────────────────────────────────────────────

/// The schemes worth recognising, and whether the host can be asked to *open* one. A `ws://`
/// or a JDBC URL is worth colouring and pointless to click.
const SCHEMES: &[(&str, bool)] = &[
    ("https://", true),
    ("http://", true),
    ("ftps://", true),
    ("ftp://", true),
    ("file://", true),
    ("mailto:", true),
    ("wss://", false),
    ("ws://", false),
    ("jdbc:", false),
];

fn url_rule(text: &str, at: usize) -> Option<Hit> {
    let rest = &text[at..];
    let (scheme, openable) = SCHEMES.iter().find(|(s, _)| rest.starts_with(*s))?;
    let mut end = at + rest
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '`' | '|'))
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    // A URL at the end of a sentence does not include the full stop. The closing bracket of
    // a URL someone wrapped in one is not part of it either.
    while end > at + scheme.len()
        && matches!(
            text[..end].chars().next_back(),
            Some('.') | Some(',') | Some(';') | Some(':') | Some(')') | Some(']') | Some('}')
        )
    {
        end -= 1;
    }
    if end <= at + scheme.len() {
        return None; // a bare scheme is not a URL
    }
    let url = text[at..end].to_string();
    Some(if *openable {
        Hit::linked(at, end, Token::Url, Link::Url { url })
    } else {
        Hit::one(at, end, Token::Url)
    })
}

// ── filesystem paths ────────────────────────────────────────────────────────────

/// A path, with the `:42` (or `:42:9`) suffix compilers append folded into its link.
///
/// Stops at whitespace, which means a Windows path containing a space is seen only as far
/// as the space — so those are required to still look like a path after the cut (an
/// extension, or two more separators) rather than turning `C:\Program Files\…` into a link
/// to `C:\Program`.
fn path_rule(text: &str, at: usize) -> Option<Hit> {
    let rest = &text[at..];
    let raw_end = at + rest
        .char_indices()
        .find(|(_, c)| {
            c.is_whitespace()
                || matches!(c, '"' | '\'' | '<' | '>' | '|' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '=')
        })
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    let mut end = raw_end;
    // Sentence punctuation, and the `:` of "path: message".
    while end > at && matches!(text[..end].chars().next_back(), Some('.') | Some(':') | Some('-')) {
        end -= 1;
    }
    let whole = &text[at..end];
    let (path, line) = match whole.rsplit_once(':') {
        Some((p, n)) if !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()) && looks_like_path(p) => {
            (p, n.parse::<u32>().ok())
        }
        _ => (whole, None),
    };
    if !looks_like_path(path) {
        return None;
    }
    Some(Hit::linked(at, end, Token::Path, Link::File { path: path.to_string(), line }))
}

/// Whether `s` is shaped like a path rather than like a word that happens to contain a
/// slash. Every branch requires structure: a drive, a leading `/`, or enough separators
/// that no English phrase would produce it.
fn looks_like_path(s: &str) -> bool {
    if s.len() < 3 {
        return false;
    }
    let seps = s.chars().filter(|c| *c == '/' || *c == '\\').count();
    let b = s.as_bytes();
    let drive = b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/');
    if drive {
        // See the note on spaces above: a truncated `C:\Program` is not a link.
        return seps >= 2 || has_extension(s);
    }
    if seps == 0 {
        return false;
    }
    if s.starts_with("./") || s.starts_with("../") || s.starts_with("~/") {
        return true;
    }
    if s.starts_with('/') {
        return seps >= 2 || has_extension(s);
    }
    seps >= 2 || has_extension(s)
}

/// A short alphanumeric tail after a dot — `.java`, `.jar`, `.yml`, `.class`.
fn has_extension(s: &str) -> bool {
    let name = s.rsplit(['/', '\\']).next().unwrap_or(s);
    match name.rsplit_once('.') {
        Some((head, ext)) => {
            !head.is_empty()
                && !ext.is_empty()
                && ext.len() <= 6
                && ext.bytes().all(|c| c.is_ascii_alphanumeric())
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::interpret;

    /// The tokens of a line, as `(text, token)` pairs — only the annotated ones.
    fn tokens(text: &str) -> Vec<(String, Token)> {
        let line = interpret(&RuleSet::common(), text);
        line.spans.iter().map(|s| (line.text[s.start..s.end].to_string(), s.token)).collect()
    }

    #[test]
    fn a_logback_line_reads_as_its_parts() {
        let got = tokens("2026-08-05 12:33:01,123 INFO  [main] com.acme.Boot - started");
        assert_eq!(got[0], ("2026-08-05 12:33:01,123".into(), Token::Timestamp));
        assert_eq!(got[1], ("INFO".into(), Token::Level));
        assert_eq!(got[2], ("[main]".into(), Token::Thread));
    }

    #[test]
    fn maven_brackets_are_part_of_the_level() {
        assert_eq!(tokens("[ERROR] Failed to execute goal")[0], ("[ERROR]".into(), Token::Level));
    }

    #[test]
    fn a_level_word_with_a_colon_keeps_the_colon_out() {
        assert_eq!(tokens("WARN: disk almost full")[0], ("WARN".into(), Token::Level));
    }

    #[test]
    fn lower_case_prose_is_not_a_level() {
        assert!(tokens("an error occurred while parsing").is_empty());
        assert!(tokens("Error handling is enabled").is_empty());
    }

    #[test]
    fn a_level_inside_a_word_is_not_a_level() {
        assert!(tokens("myERRORflag=1").is_empty());
    }

    #[test]
    fn timestamps_come_in_several_dialects() {
        for stamp in [
            "2026-08-05T12:33:01.123Z",
            "2026-08-05 12:33:01",
            "2026-08-05",
            "12:33:01,123",
            "2026-08-05T12:33:01+02:00",
        ] {
            let got = tokens(&format!("{stamp} hello"));
            assert_eq!(got.first().map(|t| t.0.as_str()), Some(stamp), "for {stamp}");
        }
    }

    #[test]
    fn a_version_is_not_a_timestamp() {
        assert!(tokens("build 12:33:01x done").is_empty());
    }

    #[test]
    fn a_url_is_linked_and_loses_the_full_stop() {
        let line = interpret(&RuleSet::common(), "see https://acme.test/docs/a.html.");
        let span = &line.spans[0];
        assert_eq!(&line.text[span.start..span.end], "https://acme.test/docs/a.html");
        assert_eq!(
            span.link,
            Some(Link::Url { url: "https://acme.test/docs/a.html".into() })
        );
    }

    #[test]
    fn a_jdbc_url_is_coloured_but_not_clickable() {
        let line = interpret(&RuleSet::common(), "url=jdbc:postgresql://db:5432/acme");
        assert_eq!(line.spans[0].token, Token::Url);
        assert_eq!(line.spans[0].link, None);
    }

    #[test]
    fn paths_carry_their_line_number() {
        let line = interpret(&RuleSet::common(), "/home/u/src/Foo.java:42: error: bad");
        assert_eq!(
            line.spans[0].link,
            Some(Link::File { path: "/home/u/src/Foo.java".into(), line: Some(42) })
        );
        assert_eq!(&line.text[line.spans[0].start..line.spans[0].end], "/home/u/src/Foo.java:42");
    }

    #[test]
    fn a_windows_path_survives_its_drive_colon() {
        let line = interpret(&RuleSet::common(), "wrote C:/build/out/app.jar ok");
        assert_eq!(
            line.spans[0].link,
            Some(Link::File { path: "C:/build/out/app.jar".into(), line: None })
        );
    }

    #[test]
    fn a_truncated_windows_path_is_not_offered_as_a_link() {
        // A path with a space in it is only seen as far as the space. `C:\Program` is not a
        // file, so it is not offered as one — better no link than a link that cannot open.
        assert!(tokens("spawn C:\\Program Files\\Java").is_empty());
    }

    #[test]
    fn a_word_with_a_slash_is_not_a_path() {
        assert!(tokens("choose one and/or the other").is_empty());
        assert!(tokens("ratio 3/4 of the heap").is_empty());
    }

    #[test]
    fn a_jar_coordinate_is_not_a_path() {
        assert!(tokens("~[spring-core-5.3.jar:5.3]").is_empty());
    }
}
