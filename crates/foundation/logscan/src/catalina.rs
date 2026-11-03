//! Tomcat's own logs: `catalina.*.log`, `localhost.*.log`, `manager`, `host-manager`, and the
//! `localhost_access_log`.
//!
//! Tomcat writes through `java.util.logging` with its `OneLineFormatter`, so a line is
//!
//! ```text
//! 03-Nov-2026 13:11:14.490 GRAVE [main] org.apache.catalina.core.StandardContext.listenerStart Exception …
//! ```
//!
//! — a timestamp, a level, a bracketed thread, and **`class.method`** rather than a logger. Two
//! things made it unreadable to the rules in [`crate::common`]:
//!
//! * The level is the JVM's **localised** one. `java.util.logging` translates `SEVERE`, `WARNING` and
//!   `INFO` into the machine's language (`GRAVE`, `AVVERTENZA`, `INFORMAZIONI` on an Italian one), so a
//!   Tomcat run on a localised JVM had no level at all and its errors were painted like its chatter.
//!   Those words are in [`crate::common::level_of`].
//! * The timestamp's shape is the formatter's `timeFormat`, which the installation may change in its
//!   `logging.properties`. Rather than model one format, the prefix is recognised by what it **is**: whatever
//!   precedes a level word and a bracketed thread. That also keeps working when the format changes.
//!
//! The access log is Common Log Format, whose only mark worth drawing is the bracketed date.

use crate::common::level_of;
use crate::model::{Link, Token};
use crate::rule::Hit;

/// How far into a line the thread's bracket may be. A timestamp is under thirty characters; a line
/// with a `" ["` fifty characters in is a message that happens to mention one.
const MAX_PREFIX: usize = 48;
const MAX_THREAD: usize = 64;

/// `<timestamp> <LEVEL> [<thread>] <class>.<method> <message>`, at the start of a line.
pub(crate) fn catalina_rule(text: &str, at: usize) -> Option<Hit> {
    if at != 0 {
        return None;
    }
    let bracket = text.find(" [").filter(|&i| i <= MAX_PREFIX)?;
    let head = &text[..bracket];
    let level_start = head.rfind(char::is_whitespace).map(|i| i + head[i..].chars().next().map_or(1, char::len_utf8))?;
    level_of(&head[level_start..])?;
    let stamp = head[..level_start].trim_end();
    // Something that has digits in it: a clock. Without this, any `WARNING [x]` sentence would be a log line.
    if stamp.is_empty() || !stamp.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }

    let thread_open = bracket + 1;
    let thread_close = text[thread_open..].find(']').map(|i| i + thread_open)?;
    if thread_close - thread_open > MAX_THREAD {
        return None;
    }

    let mut hit = Hit::spanning(thread_close + 1)
        .part(0, stamp.len(), Token::Timestamp, None)
        .part(level_start, bracket, Token::Level, None)
        .part(thread_open, thread_close + 1, Token::Thread, None);

    // `org.apache.catalina.core.StandardContext.listenerStart`: the class, and the method inside it.
    let rest = &text[thread_close + 1..];
    let name_start = thread_close + 1 + (rest.len() - rest.trim_start().len());
    let name_len = text[name_start..].find(char::is_whitespace).unwrap_or(text.len() - name_start);
    let name = &text[name_start..name_start + name_len];
    if let Some((class, method)) = split_callable(name) {
        let link = Link::Source { class: class.to_string(), method: Some(method.to_string()), file: None, line: None };
        hit = hit.part(name_start, name_start + name_len, Token::Logger, Some(link));
        hit.end = name_start + name_len;
    }
    Some(hit)
}

/// `a.b.C.method` → (`a.b.C`, `method`). `None` unless every segment is an identifier.
fn split_callable(name: &str) -> Option<(&str, &str)> {
    let (class, method) = name.rsplit_once('.')?;
    let ident = |s: &str| {
        let mut chars = s.chars();
        chars.next().is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '$'))
            && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '<' | '>'))
    };
    (class.split('.').all(ident) && ident(method)).then_some((class, method))
}

/// `[03/Nov/2026:14:18:34 +0100]` — the date of an access-log line, brackets included.
pub(crate) fn access_time_rule(text: &str, at: usize) -> Option<Hit> {
    let rest = &text[at..];
    if !rest.starts_with('[') {
        return None;
    }
    let close = rest.find(']')?;
    let b = rest[1..close].as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let shaped = b.len() == 26
        && digit(0) && digit(1) && b[2] == b'/'
        && b[3..6].iter().all(u8::is_ascii_alphabetic) && b[6] == b'/'
        && (7..11).all(digit) && b[11] == b':'
        && digit(12) && digit(13) && b[14] == b':' && digit(15) && digit(16) && b[17] == b':'
        && digit(18) && digit(19) && b[20] == b' ' && matches!(b[21], b'+' | b'-')
        && (22..26).all(digit);
    shaped.then(|| Hit::one(at, at + close + 1, Token::Timestamp))
}

#[cfg(test)]
mod tests {
    use crate::model::{Level, Token};
    use crate::reader::interpret;
    use crate::rule::RuleSet;

    fn read(line: &str) -> crate::model::Line {
        interpret(&RuleSet::java(), line)
    }

    fn token_text(line: &crate::model::Line, token: Token) -> Option<&str> {
        line.spans.iter().find(|s| s.token == token).map(|s| &line.text[s.start..s.end])
    }

    #[test]
    fn a_localised_severe_is_an_error() {
        let line = read("03-Nov-2026 13:11:14.490 GRAVE [main] org.apache.catalina.core.StandardContext.listenerStart Exception sending event");
        assert_eq!(line.level, Some(Level::Error));
        assert_eq!(token_text(&line, Token::Timestamp), Some("03-Nov-2026 13:11:14.490"));
        assert_eq!(token_text(&line, Token::Thread), Some("[main]"));
        assert_eq!(token_text(&line, Token::Logger), Some("org.apache.catalina.core.StandardContext.listenerStart"));
    }

    #[test]
    fn the_other_localised_levels_read_too() {
        assert_eq!(read("03-Nov-2026 13:11:14.490 AVVERTENZA [main] a.B.c x").level, Some(Level::Warn));
        assert_eq!(read("03-Nov-2026 13:11:14.490 INFORMAZIONI [main] a.B.c x").level, Some(Level::Info));
        assert_eq!(read("03-Nov-2026 13:11:14.490 WARNUNG [main] a.B.c x").level, Some(Level::Warn));
    }

    #[test]
    fn an_english_line_and_a_changed_time_format_read_the_same() {
        let english = read("03-Nov-2026 13:11:14.490 INFO [main] org.apache.catalina.startup.Catalina.start Server startup in 1234 ms");
        assert_eq!(english.level, Some(Level::Info));
        let short = read("13:11:14.490 SEVERE [http-nio-8080-exec-3] a.B.run boom");
        assert_eq!(short.level, Some(Level::Error));
        assert_eq!(token_text(&short, Token::Timestamp), Some("13:11:14.490"));
    }

    #[test]
    fn a_sentence_that_mentions_a_bracket_is_not_a_log_line() {
        assert_eq!(read("the WARNING [x] sign was lit").level, None);
        assert_eq!(read("INFO [main] without a clock").spans.iter().find(|s| s.token == Token::Timestamp), None);
    }

    #[test]
    fn the_stack_trace_under_a_tomcat_error_keeps_its_level() {
        let mut reader = crate::reader::LogReader::new(RuleSet::java());
        reader.read("03-Nov-2026 13:11:14.490 GRAVE [main] a.B.c failed");
        let frame = reader.read("\tat com.acme.Order.total(Order.java:118)");
        assert_eq!(frame.level, Some(Level::Error));
    }

    #[test]
    fn an_access_log_date_is_a_timestamp() {
        let line = read(r#"127.0.0.1 - - [03/Nov/2026:14:18:34 +0100] "GET /x HTTP/1.1" 200 1234"#);
        assert_eq!(token_text(&line, Token::Timestamp), Some("[03/Nov/2026:14:18:34 +0100]"));
    }
}
