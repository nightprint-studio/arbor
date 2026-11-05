//! Output lines, a batch of them to an event.
//!
//! A program's console and a build's log are written one line at a time, and a line used to be one
//! event: backend → shell → webview, each its own frame. A Spring Boot start is thousands of lines in a
//! few seconds, and when the window is in the background the webview is power-throttled and this
//! process is not — the events queue, unread, and the whole backlog runs back-to-back on the way back.
//! The frontend already folds a burst into one flush per frame; what it cannot fold is the thousands of
//! messages it has to *receive* first.
//!
//! So the lines are gathered here for a few milliseconds and leave together, as one
//! [`OUTPUT_BATCH`] event carrying `{ events: [{ topic, payload }, …] }` — the payloads exactly as they
//! were, in the order they were emitted. Nothing else about them changes, which is why this is a sink
//! wrapper and not a change to the dozen places that emit a line.
//!
//! Order is the one thing that must not move. Any OTHER event (the exit of a run, a debug stop) flushes
//! what is pending before it goes, so "Process finished" can never overtake the last line the program
//! printed; and the batch is emitted while the buffer's lock is held, so the timer's flush and an
//! emitting thread cannot interleave.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arbor_ipc::prelude::EventSink;
use serde_json::{json, Value};

/// A line of a run's stdout / stderr.
const RUN_OUTPUT: &str = "arbor://bennu/run-output";
/// A line of a build's log.
const BUILD_OUTPUT: &str = "arbor://bennu/build-output";
/// What a batch is emitted as.
pub(crate) const OUTPUT_BATCH: &str = "arbor://bennu/output-batch";

/// How long the first line of a batch waits for company. Short enough to read as live, long enough
/// that a program printing as fast as it can sends dozens of lines at a time instead of one.
const WINDOW: Duration = Duration::from_millis(30);
/// A batch that fills up leaves at once: the window bounds the delay, this bounds the size of a frame.
const MAX_BATCH: usize = 400;

/// Gathers [`RUN_OUTPUT`] / [`BUILD_OUTPUT`] events and forwards everything else untouched.
pub(crate) struct OutputBatcher {
    inner: Arc<dyn EventSink>,
    pending: Arc<Mutex<Vec<Value>>>,
    /// Wakes the flusher when a batch starts. The flusher sleeps on this, so an idle process runs no
    /// timer at all.
    kick: Sender<()>,
}

impl OutputBatcher {
    /// Wrap `inner`. Starts the flusher thread, which lives as long as the batcher does.
    pub(crate) fn wrap(inner: Arc<dyn EventSink>) -> Arc<dyn EventSink> {
        let pending: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let (kick, wake) = mpsc::channel::<()>();
        let (sink, held) = (Arc::clone(&inner), Arc::clone(&pending));
        let started = std::thread::Builder::new().name("output-batcher".into()).spawn(move || {
            for () in wake {
                std::thread::sleep(WINDOW);
                let mut batch = held.lock().unwrap_or_else(|p| p.into_inner());
                flush(&*sink, &mut batch);
            }
        });
        if let Err(e) = started {
            // Without the timer a lone line would wait for the next event: say so rather than run
            // half-batched.
            eprintln!("bennu-be: output batcher thread did not start ({e}); output is sent line by line");
            return inner;
        }
        Arc::new(Self { inner, pending, kick })
    }
}

/// Emit what is pending as one event. The caller holds the buffer's lock.
fn flush(sink: &dyn EventSink, batch: &mut Vec<Value>) {
    if batch.is_empty() {
        return;
    }
    let events = std::mem::take(batch);
    sink.emit(OUTPUT_BATCH, json!({ "events": events }));
}

impl EventSink for OutputBatcher {
    fn emit(&self, topic: &str, payload: Value) {
        let batched = topic == RUN_OUTPUT || topic == BUILD_OUTPUT;
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        if !batched {
            // Whatever came before this event goes before it.
            flush(&*self.inner, &mut pending);
            self.inner.emit(topic, payload);
            return;
        }
        let first = pending.is_empty();
        pending.push(json!({ "topic": topic, "payload": payload }));
        if pending.len() >= MAX_BATCH {
            flush(&*self.inner, &mut pending);
        } else if first {
            let _ = self.kick.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recording(Mutex<Vec<(String, Value)>>);
    impl EventSink for Recording {
        fn emit(&self, topic: &str, payload: Value) {
            self.0.lock().unwrap().push((topic.to_string(), payload));
        }
    }

    fn wait_for(rec: &Recording, count: usize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while rec.0.lock().unwrap().len() < count && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_burst_of_lines_leaves_as_one_event_in_order() {
        let rec = Arc::new(Recording::default());
        let sink = OutputBatcher::wrap(rec.clone());
        for i in 0..50 {
            sink.emit(RUN_OUTPUT, json!({ "run_id": "r", "text": format!("line {i}") }));
        }
        wait_for(&rec, 1);
        let seen = rec.0.lock().unwrap();
        assert_eq!(seen.len(), 1, "fifty lines, one event");
        assert_eq!(seen[0].0, OUTPUT_BATCH);
        let events = seen[0].1["events"].as_array().unwrap();
        assert_eq!(events.len(), 50);
        assert_eq!(events[0]["payload"]["text"], "line 0");
        assert_eq!(events[49]["payload"]["text"], "line 49");
        assert_eq!(events[0]["topic"], RUN_OUTPUT);
    }

    #[test]
    fn another_event_goes_after_the_lines_before_it() {
        let rec = Arc::new(Recording::default());
        let sink = OutputBatcher::wrap(rec.clone());
        sink.emit(RUN_OUTPUT, json!({ "text": "last words" }));
        sink.emit("arbor://bennu/run-exit", json!({ "code": 0 }));
        let seen = rec.0.lock().unwrap();
        assert_eq!(seen.len(), 2, "no waiting for the timer: the exit flushes what precedes it");
        assert_eq!(seen[0].0, OUTPUT_BATCH);
        assert_eq!(seen[1].0, "arbor://bennu/run-exit");
    }

    #[test]
    fn a_full_batch_leaves_without_waiting() {
        let rec = Arc::new(Recording::default());
        let sink = OutputBatcher::wrap(rec.clone());
        for i in 0..MAX_BATCH {
            sink.emit(BUILD_OUTPUT, json!({ "text": format!("{i}") }));
        }
        assert_eq!(rec.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_lone_line_still_arrives() {
        let rec = Arc::new(Recording::default());
        let sink = OutputBatcher::wrap(rec.clone());
        sink.emit(RUN_OUTPUT, json!({ "text": "alone" }));
        wait_for(&rec, 1);
        assert_eq!(rec.0.lock().unwrap().len(), 1);
    }
}
