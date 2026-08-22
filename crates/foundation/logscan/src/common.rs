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
            // Before `level`: both can match at column zero, and only this one knows that a
            // lower-case word there is a diagnostic rather than prose.
            .with(FnRule::new("diagnostic", diagnostic_rule))
            .with(FnRule::new("panic", panic_rule))
            .with(FnRule::new("level", level_rule))
            .with(FnRule::new("timestamp", timestamp_rule))
            .with(FnRule::new("thread", thread_rule))
            .with(FnRule::new("url", url_rule))
            .with(FnRule::new("path", path_rule))
            .continued_by(common_continues)
    }
}

/// Whether a line continues the one above it, in no particular language.
///
/// Indentation, which is what [`crate::rule::indented`] already said — plus the **source frame a
/// compiler draws under a diagnostic**, which is the one continuation shape that does not start
/// with a space:
///
/// ```text
/// warning: unused import: `spec_from_name`
///   --> src/gallery_scene/spawn.rs:35:58
///    |
/// 35 |     use crystal::{gem_prepare, spec_from_name};
///    |                                ^^^^^^^^^^^^^^
/// ```
///
/// That `35 |` is a gutter, not a new message. Without it the numbered line falls out of the
/// diagnostic and is coloured on its own — so a warning block comes out amber, amber, **red**,
/// amber, which reads as an error inside a warning.
pub fn common_continues(text: &str) -> bool {
    crate::rule::indented(text) || is_source_gutter(text)
}

/// `35 | …` — a line number, spaces, and the bar a compiler rules its source excerpt with.
fn is_source_gutter(text: &str) -> bool {
    let digits = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
    if digits == 0 {
        return false;
    }
    text[digits..].trim_start().starts_with('|')
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

/// The compiler-diagnostic form: a **lower-case** level word at the very **start of a line**,
/// followed by a colon, with an optional error code in between.
///
/// This exists because [`level_of`] is upper-case only, and that rule — right for a log, where
/// `error` is an ordinary English word — makes a compiler's entire output unreadable. `rustc`,
/// `cargo`, `clippy`, `javac`, `gcc`, `clang` and `tsc` all write `warning: …` and `error: …` in
/// lower case, so none of it was interpreted at all: every line arrived with no level, and a
/// console that paints uninterpreted stderr red painted **warnings the same colour as errors**.
///
/// The position is what makes this safe where a bare word is not. "Error handling enabled" is a
/// sentence; `error:` at column zero is a diagnostic in every toolchain that emits one, and a line
/// of prose does not begin that way.
///
/// | Shape | Level |
/// |---|---|
/// | `warning: unused import` | [`Level::Warn`] |
/// | `warning[E0123]: …`, `error[E0432]: …` | the word's, code included in the token |
/// | `error: could not compile` | [`Level::Error`] |
/// | `fatal error: 'x.h' file not found` | [`Level::Fatal`] |
/// | `note: …` / `help: …` | [`Level::Info`] — rustc's advisory halves, which are not warnings |
fn diagnostic_rule(text: &str, at: usize) -> Option<Hit> {
    // Column zero only. A `warning:` inside a message is a message about a warning.
    if at != 0 {
        return None;
    }
    // The leading run of lower-case letters, NOT `token_end`: `:` is deliberately not a token
    // boundary in this crate (it is inside timestamps, packages and URLs), so that would hand back
    // `warning:` — colon included — and no word in the table matches it.
    //
    // `fatal error:` is two words, and it is the one that must not read as an ordinary error.
    let lower = text.to_ascii_lowercase();
    let word_end = if lower.starts_with("fatal error") {
        "fatal error".len()
    } else {
        lower.find(|c: char| !c.is_ascii_lowercase()).unwrap_or(lower.len())
    };
    // Case-insensitive **because the position already did the work**. `Error:` at column zero is
    // the JVM's own most common failure — `Error: Could not find or load main class` — and it is
    // not a level word by `level_of`'s rule, which wants `ERROR`. What keeps this safe is the same
    // thing that makes the lower-case forms safe: a colon, at the start of a line. "Error handling
    // enabled" has no colon and is not a diagnostic.
    diagnostic_level(&lower[..word_end])?;

    // An optional `[E0432]`, then the colon that makes this a diagnostic rather than a word.
    let mut end = word_end;
    if text.as_bytes().get(end) == Some(&b'[') {
        end = text[end..].find(']')? + end + 1;
    }
    if text.as_bytes().get(end) != Some(&b':') {
        return None;
    }
    // The **word** is the token, not the code and not the colon: those are punctuation around it,
    // which is the same choice `level_rule` makes so `ERROR:` and `error:` come out shaped alike.
    Some(Hit::one(at, word_end, Token::Level))
}

/// `thread 'main' panicked at src/main.rs:5:5:` — Rust's failure, which carries no level word.
///
/// Here and not in a Rust module because the console that reads this runs every language Bennu
/// builds, with one rule set. Java's equivalent — a bare `com.acme.FooException: …` — is already
/// levelled by the exception rule for exactly the same reason: a line that IS the failure should
/// not need a program to have prefixed it politely.
fn panic_rule(text: &str, at: usize) -> Option<Hit> {
    if !text[at..].starts_with("panicked") {
        return None;
    }
    // Only inside the shape, so the word in a sentence is still a word.
    if !text.starts_with("thread ") || !text.contains(" panicked at") {
        return None;
    }
    Some(Hit::one(at, at + "panicked".len(), Token::Level))
}

/// The severity a **lower-case diagnostic word** names.
///
/// Separate from [`level_of`] and deliberately not merged into it: that one is asked about any
/// word anywhere in a line, and teaching it lower case would make "Error handling enabled" an
/// error. This one is only ever asked about a span some rule has already decided is a level —
/// today only [`diagnostic_rule`], which requires column zero and a colon.
pub fn diagnostic_level(word: &str) -> Option<Level> {
    // Case-folded here rather than at the call sites, and there are two that matter: the rule,
    // which reads a lower-cased copy of the line, and the reader, which reads the span's ORIGINAL
    // text. `Error:` at column zero has to answer the same in both, and having only the rule fold
    // it is how a capitalised diagnostic gets tagged as a level and then resolves to none.
    let word = word.to_ascii_lowercase();
    Some(match word.as_str() {
        "warning" => Level::Warn,
        "error" => Level::Error,
        "fatal error" => Level::Fatal,
        // rustc's advisory halves. Not warnings — painting them as one is how a single
        // diagnostic reads as four problems.
        "note" | "help" => Level::Info,
        // Rust's failure — see `panic_rule`, the only thing that tags this word.
        "panicked" => Level::Error,
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

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    use crate::reader::interpret;

    fn level(line: &str) -> Option<Level> {
        interpret(&RuleSet::common(), line).level
    }

    #[test]
    fn a_compilers_warning_is_a_warning_and_not_an_error() {
        // The bug this exists for: none of these was interpreted at all, so every one of them
        // arrived with no level — and a console that paints uninterpreted stderr red painted a
        // warning exactly the colour of an error.
        assert_eq!(level("warning: unused import: `spec_from_name`"), Some(Level::Warn));
        assert_eq!(level("warning: unused doc comment"), Some(Level::Warn));
        assert_eq!(level("warning[E0170]: pattern binding used as a constant"), Some(Level::Warn));
        assert_eq!(level("error[E0432]: unresolved import `foo::bar`"), Some(Level::Error));
        assert_eq!(level("error: could not compile `arbor` (lib) due to 1 error"), Some(Level::Error));
        // `javac`, which writes the same shapes — so this fixes the Java console too.
        assert_eq!(level("warning: [deprecation] Foo in Bar has been deprecated"), Some(Level::Warn));
        // gcc / clang, where the fatal one must not read as an ordinary error.
        assert_eq!(level("fatal error: 'stdio.h' file not found"), Some(Level::Fatal));
    }

    #[test]
    fn rustcs_advisory_halves_are_not_warnings() {
        // One diagnostic is a `warning:` plus its `note:`s and `help:`s. Levelling those as
        // warnings turns every diagnostic into four problems.
        assert_eq!(level("note: `#[warn(unused_imports)]` on by default"), Some(Level::Info));
        assert_eq!(level("help: remove the whole `use` item"), Some(Level::Info));
    }

    #[test]
    fn a_lowercase_level_word_is_only_a_level_at_the_start_of_a_line() {
        // The whole safety argument. A lower-case level word is an ordinary English word, and the
        // POSITION plus the colon is what tells a diagnostic from prose.
        assert_eq!(level("Error handling enabled"), None);
        assert_eq!(level("Started with error recovery on"), None);
        assert_eq!(level("  warning: indented, so it is a continuation not a head"), None);
        assert_eq!(level("the build printed a warning: see above"), None);
        // A word at column zero with no colon is still just a word.
        assert_eq!(level("warning about the cache"), None);
        assert_eq!(level("errors: 3"), None);
    }

    #[test]
    fn the_uppercase_forms_still_win_and_still_work() {
        // The Java dialect this crate was written for, unchanged.
        assert_eq!(level("[ERROR] Failed to execute goal"), Some(Level::Error));
        assert_eq!(level("2026-08-05 12:33:01 WARN  c.a.Order - retrying"), Some(Level::Warn));
        assert_eq!(level("SEVERE: context failed"), Some(Level::Error));
    }

    #[test]
    fn the_two_failures_that_carry_no_level_word_are_still_errors() {
        // Rust's, which is what a `cargo run` prints when it dies.
        assert_eq!(
            level("thread 'main' panicked at crates/app/src/main.rs:5:5:"),
            Some(Level::Error),
        );
        // The JVM's most common one. `Error:` is not `ERROR`, so `level_of` never saw it — and
        // once uninterpreted standard error stopped being red, it would have gone quiet.
        assert_eq!(level("Error: Could not find or load main class Foo"), Some(Level::Error));
        assert_eq!(level("Warning: deprecated option"), Some(Level::Warn));

        // The word outside its shape is still a word.
        assert_eq!(level("the process panicked and we restarted it"), None);
        assert_eq!(level("panicked"), None);
    }

    #[test]
    fn a_compilers_source_frame_stays_inside_its_diagnostic() {
        // The whole block a compiler draws, and the one line in it that does not start with a
        // space. Without the gutter shape it falls out of the diagnostic and is coloured on its
        // own — a red line in the middle of an amber warning.
        let rules = RuleSet::common();
        assert!(rules.is_continuation("  --> src/gallery_scene/spawn.rs:35:58"));
        assert!(rules.is_continuation("   |"));
        assert!(rules.is_continuation("35 |     use crystal::{gem_prepare, spec_from_name};"));
        assert!(rules.is_continuation("962 | /   /// **La barra in alto è UN nodo**"));

        // …and a line that merely begins with a number is not a gutter.
        assert!(!rules.is_continuation("35 files changed"));
        assert!(!rules.is_continuation("2026-08-05 12:33:01 INFO started"));
        assert!(!rules.is_continuation("warning: unused import"));
    }

    #[test]
    fn the_token_is_the_word_alone() {
        // The code and the colon are punctuation around it — the same shape `[ERROR]` comes out
        // as, so a renderer needs no second case.
        let line = interpret(&RuleSet::common(), "warning[E0170]: pattern binding");
        let first = line.spans.first().expect("the diagnostic head is a span");
        assert_eq!(first.token, Token::Level);
        assert_eq!((first.start, first.end), (0, "warning".len()));
    }
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
