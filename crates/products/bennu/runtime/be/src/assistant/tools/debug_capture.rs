//! What the program a debug launch started has printed, kept for the assistant to read.
//!
//! A run started with `bennu_run` streams its output to the panel as events and keeps nothing: the
//! console is the only reader. The assistant has no console, so the launch is given a sink that
//! passes every event on untouched (the panel behaves exactly as for a run the user started) and
//! also keeps the program's lines in a bounded buffer, together with how it ended.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::EventSink;
use serde_json::Value;

const RUN_OUTPUT: &str = "arbor://bennu/run-output";
const RUN_EXIT: &str = "arbor://bennu/run-exit";

/// The most lines kept; the oldest go first.
const MAX_LINES: usize = 2_000;
/// The most characters of one line kept — a single enormous line must not fill the buffer.
const MAX_LINE_CHARS: usize = 400;

/// The program's output and exit, filled by [`TeeSink`] and read by the tools.
#[derive(Default)]
pub(super) struct Capture {
    inner: Mutex<CaptureInner>,
}

#[derive(Default)]
struct CaptureInner {
    lines: VecDeque<(u64, String)>,
    next_seq: u64,
    /// `Some(code)` once the program ended; the code is absent when it was killed.
    exit: Option<Option<i32>>,
}

impl Capture {
    pub(super) fn push(&self, stream: &str, text: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.next_seq += 1;
        let seq = inner.next_seq;
        let body: String = text.chars().take(MAX_LINE_CHARS).collect();
        let line = if stream == "stderr" { format!("[err] {body}") } else { body };
        inner.lines.push_back((seq, line));
        while inner.lines.len() > MAX_LINES {
            inner.lines.pop_front();
        }
    }

    pub(super) fn set_exit(&self, code: Option<i32>) {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).exit = Some(code);
    }

    /// `Some(code)` once the program ended.
    pub(super) fn exit(&self) -> Option<Option<i32>> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).exit
    }

    /// The newest `tail` lines after `since`, each with its sequence number, and the last sequence
    /// number seen (the `since` of the next call).
    pub(super) fn read(&self, since: u64, tail: usize) -> (Vec<(u64, String)>, u64) {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let fresh: Vec<&(u64, String)> = inner.lines.iter().filter(|(s, _)| *s > since).collect();
        let skip = fresh.len().saturating_sub(tail);
        let shown: Vec<(u64, String)> = fresh.into_iter().skip(skip).cloned().collect();
        (shown, inner.next_seq)
    }

    /// How many lines arrived after `since`.
    pub(super) fn count_after(&self, since: u64) -> u64 {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).next_seq.saturating_sub(since)
    }
}

/// An event sink that forwards everything and keeps the run's output.
pub(super) struct TeeSink {
    inner: Arc<dyn EventSink>,
    capture: Arc<Capture>,
}

impl TeeSink {
    pub(super) fn new(inner: Arc<dyn EventSink>, capture: Arc<Capture>) -> Self {
        Self { inner, capture }
    }
}

impl EventSink for TeeSink {
    fn emit(&self, topic: &str, payload: Value) {
        match topic {
            RUN_OUTPUT => {
                let stream = payload.get("stream").and_then(Value::as_str).unwrap_or("stdout");
                if let Some(text) = payload.get("text").and_then(Value::as_str) {
                    self.capture.push(stream, text);
                }
            }
            RUN_EXIT => self.capture.set_exit(payload.get("code").and_then(Value::as_i64).map(|c| c as i32)),
            _ => {}
        }
        self.inner.emit(topic, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Default)]
    struct Seen(Mutex<Vec<String>>);
    impl EventSink for Seen {
        fn emit(&self, topic: &str, _payload: Value) {
            self.0.lock().unwrap().push(topic.to_string());
        }
    }

    #[test]
    fn output_is_kept_with_sequence_numbers_and_the_tail_is_the_newest() {
        let c = Capture::default();
        for i in 1..=5 {
            c.push("stdout", &format!("line {i}"));
        }
        c.push("stderr", "boom");
        let (lines, last) = c.read(0, 3);
        assert_eq!(last, 6);
        assert_eq!(lines.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), vec!["line 4", "line 5", "[err] boom"]);
        let (fresh, _) = c.read(5, 10);
        assert_eq!(fresh.len(), 1);
        assert_eq!(c.count_after(2), 4);
    }

    #[test]
    fn the_buffer_and_each_line_are_bounded() {
        let c = Capture::default();
        for i in 0..(MAX_LINES + 10) {
            c.push("stdout", &format!("{i}"));
        }
        let (all, _) = c.read(0, usize::MAX);
        assert_eq!(all.len(), MAX_LINES);
        assert_eq!(all.first().unwrap().1, "10", "the oldest ten were dropped");
        let c = Capture::default();
        c.push("stdout", &"x".repeat(MAX_LINE_CHARS * 3));
        assert_eq!(c.read(0, 1).0[0].1.chars().count(), MAX_LINE_CHARS);
    }

    #[test]
    fn the_sink_forwards_everything_and_keeps_output_and_exit() {
        let seen = Arc::new(Seen::default());
        let capture = Arc::new(Capture::default());
        let tee = TeeSink::new(seen.clone(), capture.clone());
        tee.emit(RUN_OUTPUT, json!({ "run_id": "r", "stream": "stdout", "text": "hello" }));
        tee.emit("arbor://bennu/debug-status", json!({}));
        tee.emit(RUN_EXIT, json!({ "run_id": "r", "code": 3 }));
        assert_eq!(seen.0.lock().unwrap().len(), 3, "nothing is swallowed");
        assert_eq!(capture.read(0, 5).0[0].1, "hello");
        assert_eq!(capture.exit(), Some(Some(3)));
        // A process that was killed reports no code: it ended, but not with one.
        let killed = Arc::new(Capture::default());
        TeeSink::new(Arc::new(Seen::default()), killed.clone()).emit(RUN_EXIT, json!({ "code": null }));
        assert_eq!(killed.exit(), Some(None));
    }
}
