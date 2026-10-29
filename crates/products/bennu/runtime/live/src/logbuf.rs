//! A bounded, sequenced log: the last lines a process wrote, each with a timestamp and a number.
//!
//! Bounded twice — by line count and by bytes — because a chatty service left running for an hour
//! must not grow the backend. The sequence number never repeats, so a reader that remembers the last
//! one it saw gets exactly what is new (`since`), even after older lines were dropped.

use std::collections::VecDeque;

use regex::Regex;
use serde::Serialize;

/// A single line is cut here: a minified payload on one line is not worth its memory.
pub const MAX_LINE_CHARS: usize = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
    /// Written by the manager itself: "started", "stopped by user".
    System,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub seq: u64,
    pub ts_ms: u64,
    pub stream: Stream,
    pub text: String,
}

/// What a reader asks for.
#[derive(Debug, Default, Clone)]
pub struct LogQuery {
    /// Only lines with a sequence number above this.
    pub since: Option<u64>,
    /// Only lines at or after this time.
    pub after_ms: Option<u64>,
    /// Only lines matching this.
    pub pattern: Option<Regex>,
    pub stream: Option<Stream>,
    /// The last `tail` lines of the matches.
    pub tail: Option<usize>,
    /// Hard cap on what is returned; the newest are kept.
    pub limit: Option<usize>,
}

pub struct LogBuffer {
    lines: VecDeque<LogLine>,
    bytes: usize,
    max_lines: usize,
    max_bytes: usize,
    next_seq: u64,
    dropped: u64,
}

impl LogBuffer {
    pub fn new(max_lines: usize, max_bytes: usize) -> Self {
        Self { lines: VecDeque::new(), bytes: 0, max_lines: max_lines.max(1), max_bytes: max_bytes.max(1024), next_seq: 1, dropped: 0 }
    }

    /// Append a line; returns its sequence number.
    pub fn push(&mut self, ts_ms: u64, stream: Stream, text: &str) -> u64 {
        let text = clip_line(text);
        let seq = self.next_seq;
        self.next_seq += 1;
        self.bytes += text.len() + 24;
        self.lines.push_back(LogLine { seq, ts_ms, stream, text });
        while self.lines.len() > self.max_lines || (self.bytes > self.max_bytes && self.lines.len() > 1) {
            if let Some(old) = self.lines.pop_front() {
                self.bytes = self.bytes.saturating_sub(old.text.len() + 24);
                self.dropped += 1;
            }
        }
        seq
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// How many lines were pushed out by the caps since the start.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// The newest sequence number, 0 when nothing was ever written.
    pub fn last_seq(&self) -> u64 {
        self.next_seq - 1
    }

    pub fn query(&self, q: &LogQuery) -> Vec<LogLine> {
        let mut out: Vec<LogLine> = self
            .lines
            .iter()
            .filter(|l| q.since.is_none_or(|s| l.seq > s))
            .filter(|l| q.after_ms.is_none_or(|t| l.ts_ms >= t))
            .filter(|l| q.stream.is_none_or(|s| l.stream == s))
            .filter(|l| q.pattern.as_ref().is_none_or(|re| re.is_match(&l.text)))
            .cloned()
            .collect();
        let keep = [q.tail, q.limit].into_iter().flatten().min();
        if let Some(n) = keep {
            if out.len() > n {
                out.drain(..out.len() - n);
            }
        }
        out
    }

    pub fn tail(&self, n: usize) -> Vec<LogLine> {
        self.query(&LogQuery { tail: Some(n), ..Default::default() })
    }
}

fn clip_line(text: &str) -> String {
    let text = text.trim_end_matches(['\r', '\n']);
    if text.chars().count() <= MAX_LINE_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_LINE_CHARS).collect();
    format!("{kept}… [line cut]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_numbered_and_dropped_oldest_first() {
        let mut b = LogBuffer::new(3, 1 << 20);
        for i in 0..5 {
            b.push(i, Stream::Stdout, &format!("line {i}"));
        }
        let all = b.query(&LogQuery::default());
        assert_eq!(all.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![3, 4, 5]);
        assert_eq!(b.dropped(), 2);
        assert_eq!(b.last_seq(), 5);
    }

    #[test]
    fn since_returns_only_what_is_new_even_after_drops() {
        let mut b = LogBuffer::new(2, 1 << 20);
        for i in 0..6 {
            b.push(0, Stream::Stdout, &format!("{i}"));
        }
        let got = b.query(&LogQuery { since: Some(4), ..Default::default() });
        assert_eq!(got.len(), 2);
        assert!(b.query(&LogQuery { since: Some(6), ..Default::default() }).is_empty());
    }

    #[test]
    fn the_byte_cap_drops_lines_too() {
        let mut b = LogBuffer::new(1000, 2048);
        for _ in 0..100 {
            b.push(0, Stream::Stdout, &"x".repeat(200));
        }
        assert!(b.len() < 20, "kept {}", b.len());
        assert!(b.len() >= 1);
    }

    #[test]
    fn filters_combine_and_tail_keeps_the_newest_matches() {
        let mut b = LogBuffer::new(100, 1 << 20);
        b.push(10, Stream::Stdout, "ok one");
        b.push(20, Stream::Stderr, "ERROR a");
        b.push(30, Stream::Stdout, "ERROR b");
        b.push(40, Stream::Stdout, "ERROR c");
        let q = LogQuery { pattern: Some(Regex::new("ERROR").unwrap()), tail: Some(2), ..Default::default() };
        let got = b.query(&q);
        assert_eq!(got.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), vec!["ERROR b", "ERROR c"]);
        let q = LogQuery { stream: Some(Stream::Stderr), ..Default::default() };
        assert_eq!(b.query(&q).len(), 1);
        let q = LogQuery { after_ms: Some(30), ..Default::default() };
        assert_eq!(b.query(&q).len(), 2);
    }

    #[test]
    fn a_huge_line_is_cut_and_line_endings_are_stripped() {
        let mut b = LogBuffer::new(10, 1 << 20);
        b.push(0, Stream::Stdout, &format!("{}\r\n", "y".repeat(MAX_LINE_CHARS + 50)));
        let line = &b.tail(1)[0];
        assert!(line.text.ends_with("[line cut]") && !line.text.contains('\r'));
        b.push(0, Stream::Stdout, "short\n");
        assert_eq!(b.tail(1)[0].text, "short");
    }
}
