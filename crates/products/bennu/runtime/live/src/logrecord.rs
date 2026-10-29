//! From lines to RECORDS: a log line together with what belongs to it (its stack trace, the `Caused by:`
//! chain, a Rust backtrace, a multi-line SQL), with a stable id and the facts read off its head.
//!
//! The per-line interpretation is `arbor-logscan` — the same one that colours the Run console — so a
//! level, a timestamp or an exception means here what it means there. What this module adds is the
//! grouping across lines and the formats the console has no use for (JSON logs, `level=warn` pairs).
//!
//! The id of a record is the sequence number of its first line (a process, a console run) or the byte
//! offset it starts at (a file): stable as long as the source keeps the line, which is what lets a
//! digest point at "record 1842" and `log_record` fetch it later.

use arbor_logscan::prelude::{interpret, level_of, Level, Line, RuleSet, Token};
use serde_json::Value;

use crate::logbuf::Stream;
use crate::logtime::parse_iso;

/// Continuation lines kept per record; the rest are counted, not stored.
pub const MAX_REST_LINES: usize = 400;

/// A line as the buffer holds it.
#[derive(Debug, Clone)]
pub struct RawLine {
    pub seq: u64,
    pub ts_ms: Option<u64>,
    pub stream: Stream,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Record {
    pub id: u64,
    /// The sequence number of its last line.
    pub last_seq: u64,
    pub ts_ms: Option<u64>,
    /// `Fatal` is folded into `Error`: nobody asks for one and not the other.
    pub level: Option<Level>,
    pub stream: Stream,
    pub logger: Option<String>,
    /// The head without its timestamp / level / thread / logger prefix.
    pub message: String,
    pub head: String,
    pub rest: Vec<String>,
    /// Continuation lines that were not stored (past [`MAX_REST_LINES`]).
    pub dropped_rest: usize,
}

impl Record {
    pub fn lines(&self) -> usize {
        1 + self.rest.len() + self.dropped_rest
    }

    /// The whole record as text, one line per line.
    pub fn text(&self) -> String {
        let mut out = self.head.clone();
        for l in &self.rest {
            out.push('\n');
            out.push_str(l);
        }
        out
    }
}

/// The severity as filters and counters see it.
pub fn fold(level: Level) -> Level {
    if level == Level::Fatal { Level::Error } else { level }
}

pub fn level_name(level: Level) -> &'static str {
    match level {
        Level::Trace => "TRACE",
        Level::Debug => "DEBUG",
        Level::Info => "INFO",
        Level::Warn => "WARN",
        Level::Error | Level::Fatal => "ERROR",
    }
}

/// `error`, `WARN`, `warning`, `info`... as a level.
pub fn parse_level(word: &str) -> Result<Level, String> {
    let w = word.trim().to_ascii_uppercase();
    level_of(&w).map(fold).ok_or_else(|| format!("`{word}` is not a level: use ERROR, WARN, INFO, DEBUG or TRACE"))
}

fn stream_slot(s: Stream) -> usize {
    match s {
        Stream::Stdout => 0,
        Stream::Stderr => 1,
        Stream::System => 2,
    }
}

/// Group `lines` (in order) into records.
///
/// Per stream: a Java trace goes to stderr while the log goes to stdout, and the OS interleaves them,
/// so a trace is attached to the last record *of its own stream*.
pub fn build_records(rules: &RuleSet, lines: &[RawLine]) -> Vec<Record> {
    let mut out: Vec<Record> = Vec::new();
    let mut last: [Option<usize>; 3] = [None; 3];
    for raw in lines {
        let line = interpret(rules, &raw.text);
        if line.text.trim().is_empty() {
            continue;
        }
        let slot = stream_slot(raw.stream);
        if let Some(idx) = last[slot].filter(|&i| attaches(rules, &line, &out[i])) {
            let rec = &mut out[idx];
            rec.last_seq = raw.seq;
            if rec.rest.len() < MAX_REST_LINES {
                rec.rest.push(line.text);
            } else {
                rec.dropped_rest += 1;
            }
            continue;
        }
        out.push(head_record(&line, raw));
        last[slot] = Some(out.len() - 1);
    }
    out
}

fn has_token(line: &Line, token: Token) -> bool {
    line.spans.iter().any(|s| s.token == token)
}

/// Whether `line` belongs to `prev` rather than starting a record.
fn attaches(rules: &RuleSet, line: &Line, prev: &Record) -> bool {
    let text = line.text.as_str();
    // p6spy lines (`1785933181000|3|statement|...`) look like a compiler gutter (`35 | code`) to the continuation rule.
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && text.as_bytes().get(digits) == Some(&b'|') {
        return false;
    }
    if rules.is_continuation(text) || text.starts_with("stack backtrace:") || (text.starts_with("note:") && text.contains("RUST_BACKTRACE")) {
        return true;
    }
    let own_head = has_token(line, Token::Level) || has_token(line, Token::Timestamp);
    // The message line of a Rust panic: `thread 'main' panicked at src/x.rs:3:5:` then the text.
    if prev.head.contains("panicked at") && prev.rest.is_empty() && !own_head {
        return true;
    }
    // `ERROR ... failed` followed by an unindented `java.lang.IllegalStateException: boom`.
    prev.level.is_some() && !own_head && starts_with_throwable(text)
}

/// `java.lang.IllegalStateException: boom`, `OrderNotFoundException`, `Exception in thread "main" ...`.
pub fn starts_with_throwable(text: &str) -> bool {
    let first = text.split(|c: char| c.is_whitespace() || c == ':').next().unwrap_or("");
    let shape = first.chars().next().is_some_and(char::is_alphabetic) && first.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '$' | '_'));
    let named = ["Exception", "Error", "Throwable"].iter().any(|s| first.ends_with(s)) && (first.contains('.') || first.len() > 9);
    (shape && named) || text.starts_with("Exception in thread")
}

fn head_record(line: &Line, raw: &RawLine) -> Record {
    let head = line.text.clone();
    let parsed = parse_json_head(&head).unwrap_or_else(|| parse_text_head(line));
    let mut rest = Vec::new();
    let mut dropped_rest = 0;
    for l in parsed.extra_lines {
        if rest.len() < MAX_REST_LINES { rest.push(l) } else { dropped_rest += 1 }
    }
    Record {
        id: raw.seq,
        last_seq: raw.seq,
        ts_ms: raw.ts_ms.or(parsed.ts_ms),
        level: parsed.level.map(fold),
        stream: raw.stream,
        logger: parsed.logger,
        message: parsed.message,
        head,
        rest,
        dropped_rest,
    }
}

struct Head {
    level: Option<Level>,
    ts_ms: Option<u64>,
    logger: Option<String>,
    message: String,
    extra_lines: Vec<String>,
}

fn parse_text_head(line: &Line) -> Head {
    let text = line.text.as_str();
    let mut level = line.level.or_else(|| logfmt_level(text));
    if level.is_none() && text.contains("panicked at") {
        level = Some(Level::Error);
    }
    let ts_ms = line.spans.iter().find(|s| s.token == Token::Timestamp).and_then(|s| parse_iso(&text[s.start..s.end]));
    let (message, split_logger) = message_of(line);
    let logger = line.spans.iter().find(|s| s.token == Token::Logger).map(|s| text[s.start..s.end].to_string()).or(split_logger);
    Head { level, ts_ms, logger, message, extra_lines: Vec::new() }
}

/// What the line says, without the prefix every line of the format repeats.
fn message_of(line: &Line) -> (String, Option<String>) {
    let text = line.text.as_str();
    // The prefix is the run of prefix-like spans at the start; a date further in belongs to the message.
    let mut prefix_end = 0;
    for s in &line.spans {
        if s.start > prefix_end + 48 {
            break;
        }
        if matches!(s.token, Token::Timestamp | Token::Level | Token::Thread | Token::Logger | Token::Mdc | Token::MdcKey) {
            prefix_end = s.end;
        }
    }
    let mut rest = text[prefix_end..].trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ':' | '-' | '|' | ']'));
    // env_logger: `[2026-08-05T12:33:01Z ERROR app::db] message` — the module name is inside the bracket.
    if text.starts_with('[') {
        if let Some(close) = rest.find(']').filter(|&i| i < 80 && !rest[..i].contains('[')) {
            rest = rest[close + 1..].trim_start();
        }
    }
    // Spring Boot: `77 --- [thread] c.a.Foo : message` — the pid marker, then the logger before ` : `.
    if let Some((pid, after)) = rest.split_once(" --- ") {
        if pid.bytes().all(|c| c.is_ascii_digit()) {
            rest = after.trim_start();
        }
    }
    let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ']').trim_start();
    // A head that is only the logger (the statement is on the lines below).
    if let Some(name) = rest.strip_suffix(" :").map(str::trim).filter(|n| looks_like_logger(n)) {
        return (String::new(), Some(name.to_string()));
    }
    for sep in [" : ", " - "] {
        if let Some((name, message)) = rest.split_once(sep) {
            if looks_like_logger(name) {
                return (message.trim().to_string(), Some(name.to_string()));
            }
        }
    }
    (rest.to_string(), None)
}

fn looks_like_logger(name: &str) -> bool {
    !name.is_empty() && !name.contains(char::is_whitespace) && (name.contains('.') || name.contains("::") || name.starts_with(char::is_uppercase))
}

/// `level=warn`, `lvl=ERROR` in a logfmt line.
fn logfmt_level(text: &str) -> Option<Level> {
    let at = text.find("level=").map(|i| i + 6).or_else(|| text.find("lvl=").map(|i| i + 4))?;
    let word: String = text[at..].trim_start_matches('"').chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    level_of(&word.to_ascii_uppercase())
}

fn json_str<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v.get(*k).and_then(Value::as_str))
}

/// A JSON log line (`{"level":"ERROR","message":...}`), or `None` if it is not one.
fn parse_json_head(text: &str) -> Option<Head> {
    let t = text.trim();
    if !(t.starts_with('{') && t.ends_with('}')) {
        return None;
    }
    let v: Value = serde_json::from_str(t).ok()?;
    let level = json_str(&v, &["level", "severity", "lvl", "levelname", "log.level", "log_level"])
        .and_then(|w| level_of(&w.to_ascii_uppercase()))
        .or_else(|| {
            // pino / bunyan: 10 trace, 20 debug, 30 info, 40 warn, 50 error, 60 fatal.
            let n = v.get("level").and_then(Value::as_u64)?;
            Some(match n {
                0..=10 => Level::Trace,
                11..=20 => Level::Debug,
                21..=30 => Level::Info,
                31..=40 => Level::Warn,
                _ => Level::Error,
            })
        });
    let ts_ms = ["timestamp", "@timestamp", "time", "ts", "asctime"].iter().find_map(|k| match v.get(*k)? {
        Value::String(s) => parse_iso(s),
        Value::Number(n) => n.as_f64().map(|f| if f < 1e11 { (f * 1000.0) as u64 } else { f as u64 }),
        _ => None,
    });
    let message = json_str(&v, &["message", "msg", "@message", "event"]).unwrap_or("").to_string();
    let logger = json_str(&v, &["logger", "logger_name", "target", "name"]).map(str::to_string);
    let extra_lines = json_str(&v, &["stack_trace", "exception", "stack", "error.stack_trace"])
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default();
    Some(Head { level, ts_ms, logger, message, extra_lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(texts: &[&str]) -> Vec<RawLine> {
        texts.iter().enumerate().map(|(i, t)| RawLine { seq: i as u64 + 1, ts_ms: None, stream: Stream::Stdout, text: t.to_string() }).collect()
    }

    fn records(texts: &[&str]) -> Vec<Record> {
        build_records(&RuleSet::java(), &lines(texts))
    }

    #[test]
    fn a_java_trace_with_its_cause_chain_is_one_record() {
        let r = records(&[
            "2026-08-05 12:33:01.123 ERROR 77 --- [nio-8080-exec-1] c.a.OrderController : Request failed",
            "org.springframework.web.util.NestedServletException: Request processing failed",
            "\tat org.springframework.web.servlet.FrameworkServlet.processRequest(FrameworkServlet.java:1014)",
            "Caused by: java.lang.NullPointerException: order is null",
            "\tat com.acme.OrderService.total(OrderService.java:42)",
            "\t... 31 common frames omitted",
            "2026-08-05 12:33:02.000 INFO 77 --- [main] c.a.Boot : fine",
        ]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].rest.len(), 5);
        assert_eq!(r[0].level, Some(Level::Error));
        assert_eq!(r[0].message, "Request failed");
        assert_eq!(r[1].level, Some(Level::Info));
        assert_eq!(r[1].id, 7);
    }

    #[test]
    fn a_rust_panic_keeps_its_message_and_backtrace() {
        let r = records(&[
            "thread 'main' panicked at src/main.rs:12:5:",
            "called `Option::unwrap()` on a `None` value",
            "stack backtrace:",
            "   0: rust_begin_unwind",
            "             at /rustc/abc/library/std/src/panicking.rs:645:5",
            "   5: app::handler",
            "             at ./src/handler.rs:44:9",
            "note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.",
            "[2026-08-05T12:33:01Z INFO  app] next",
        ]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].level, Some(Level::Error));
        assert_eq!(r[0].rest.len(), 7);
        assert_eq!(r[1].message, "next");
    }

    #[test]
    fn json_and_logfmt_lines_get_their_level_time_and_message() {
        let r = records(&[
            r#"{"@timestamp":"2026-08-05T12:33:01.000Z","level":"WARN","logger_name":"c.a.Pool","message":"pool low","stack_trace":"x.Y: z\n\tat a.B.c(B.java:1)"}"#,
            r#"{"level":50,"msg":"pino error","time":1785933181000}"#,
            "ts=2026-08-05T12:33:01Z level=error msg=\"db down\"",
        ]);
        assert_eq!(r[0].level, Some(Level::Warn));
        assert_eq!(r[0].message, "pool low");
        assert_eq!(r[0].rest.len(), 2);
        assert_eq!(r[0].ts_ms, parse_iso("2026-08-05T12:33:01Z"));
        assert_eq!(r[1].level, Some(Level::Error));
        assert_eq!(r[2].level, Some(Level::Error));
    }

    #[test]
    fn stderr_traces_attach_to_the_stderr_record_not_to_interleaved_stdout() {
        let mut ls = lines(&["ERROR boom", "INFO chatter", "\tat a.B.c(B.java:1)"]);
        ls[2].stream = Stream::Stderr;
        ls[0].stream = Stream::Stderr;
        let r = build_records(&RuleSet::java(), &ls);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].rest.len(), 1);
    }

    #[test]
    fn env_logger_prefix_is_not_part_of_the_message() {
        let r = records(&["[2026-08-05T12:33:01Z ERROR app::db] connection refused"]);
        assert_eq!(r[0].level, Some(Level::Error));
        assert_eq!(r[0].message, "connection refused");
    }

    #[test]
    fn levels_parse_by_name() {
        assert_eq!(parse_level("warning"), Ok(Level::Warn));
        assert_eq!(parse_level("fatal"), Ok(Level::Error));
        assert!(parse_level("loud").is_err());
    }
}
