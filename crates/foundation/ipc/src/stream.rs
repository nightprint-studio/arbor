//! `Stream` — standardized sugar over [`EventSink`](crate::event::EventSink) for
//! the streaming seam (`docs/streaming-seam.md`).
//!
//! A streaming command returns an id synchronously, then pushes a sequence of
//! one-way, id-correlated events at its own pace. `Stream` centralizes the
//! **envelope** (`{ stream_id, seq }`) and the **lifecycle** (`started` / `chunk`
//! / `done` / `error`) so each command no longer hand-rolls its own topic
//! strings, envelope keys, and started/chunk/done/error quartet.
//!
//! It introduces **no new transport**: every method is still an ordinary
//! [`EventSink::emit`], so it works in-process (backed by `AppHandle::emit`) and
//! over the frame protocol (backed by `FrameEventSink`) unchanged.
//!
//! ## Envelope and topics
//!
//! For a base name `<base>`, a `Stream` emits on four derived topics:
//!
//! | Topic            | When                                  |
//! |------------------|---------------------------------------|
//! | `<base>-started` | once, synchronously, before returning |
//! | `<base>-chunk`   | once per item produced                |
//! | `<base>-done`    | once, on success                      |
//! | `<base>-error`   | once, on failure                      |
//!
//! Every payload carries the common envelope merged in:
//!
//! ```jsonc
//! { "stream_id": "<id>", "seq": 0 /* monotonic per stream, started=0 */ }
//! ```
//!
//! `seq` lets the FE detect drops / reordering; `stream_id` correlates the
//! quartet. When a command also registers a `JobInfo`, the **stream id is the
//! job id** — one identity addresses the Jobs overlay entry, the stream quartet,
//! and the cancel call (`docs/streaming-seam.md`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::event::EventSink;

/// Standardized streaming emitter over an [`EventSink`].
///
/// Owns the base topic, the `stream_id`, and the monotonic `seq` counter; the
/// handler never spells `-started` / `-chunk` / `-done` / `-error` itself. Clones
/// cheaply (`Arc` + atomic) into a background thread that outlives the call.
pub struct Stream {
    sink:      Arc<dyn EventSink>,
    base:      String,
    stream_id: String,
    seq:       Arc<AtomicU64>,
}

impl Clone for Stream {
    fn clone(&self) -> Self {
        Self {
            sink:      Arc::clone(&self.sink),
            base:      self.base.clone(),
            stream_id: self.stream_id.clone(),
            seq:       Arc::clone(&self.seq),
        }
    }
}

impl Stream {
    /// Create a stream emitting on `<base>-{started,chunk,done,error}` with the
    /// given `stream_id` correlating the quartet. `seq` starts at 0 (the
    /// `started` event).
    pub fn new(sink: Arc<dyn EventSink>, base: impl Into<String>, stream_id: impl Into<String>) -> Self {
        Self {
            sink,
            base:      base.into(),
            stream_id: stream_id.into(),
            seq:       Arc::new(AtomicU64::new(0)),
        }
    }

    /// The id correlating this stream's quartet (== job id where a job exists).
    pub fn id(&self) -> &str {
        &self.stream_id
    }

    /// Emit `<base>-started` synchronously, before the id is returned. `payload`
    /// is merged into the envelope (e.g. `{ "total": n, "files": [...] }`).
    pub fn started(&self, payload: Value) {
        self.emit("started", payload);
    }

    /// Emit `<base>-chunk` for one produced item. `payload` is merged into the
    /// envelope (the item plus optional `index` / `total`).
    pub fn chunk(&self, payload: Value) {
        self.emit("chunk", payload);
    }

    /// Emit `<base>-done` once, on success. `payload` is an optional summary.
    pub fn done(&self, payload: Value) {
        self.emit("done", payload);
    }

    /// Emit `<base>-error` once, on failure, carrying `{ "error": <message> }`.
    pub fn error(&self, message: &str) {
        self.emit("error", serde_json::json!({ "error": message }));
    }

    /// Merge the `{ stream_id, seq }` envelope into `payload` and emit it on the
    /// `<base>-<suffix>` topic, bumping `seq`.
    fn emit(&self, suffix: &str, payload: Value) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let mut obj: Map<String, Value> = match payload {
            Value::Object(m) => m,
            // A non-object producer payload still rides the envelope under "value"
            // rather than being dropped.
            Value::Null      => Map::new(),
            other            => {
                let mut m = Map::new();
                m.insert("value".to_string(), other);
                m
            }
        };
        obj.insert("stream_id".to_string(), Value::String(self.stream_id.clone()));
        obj.insert("seq".to_string(), Value::from(seq));
        let topic = format!("{}-{suffix}", self.base);
        self.sink.emit(&topic, Value::Object(obj));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Records every (topic, payload) the stream emits.
    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<(String, Value)>>,
    }

    impl EventSink for RecordingSink {
        fn emit(&self, topic: &str, payload: Value) {
            self.events.lock().unwrap().push((topic.to_string(), payload));
        }
    }

    #[test]
    fn lifecycle_topics_envelope_and_seq() {
        let sink = Arc::new(RecordingSink::default());
        let stream = Stream::new(sink.clone(), "arbor://demo", "id-1");

        stream.started(serde_json::json!({ "total": 2 }));
        stream.chunk(serde_json::json!({ "index": 0 }));
        stream.chunk(serde_json::json!({ "index": 1 }));
        stream.done(serde_json::json!({}));

        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 4);

        // Topics derive from the base, in order.
        let topics: Vec<&str> = events.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(
            topics,
            vec![
                "arbor://demo-started",
                "arbor://demo-chunk",
                "arbor://demo-chunk",
                "arbor://demo-done",
            ]
        );

        // Envelope: stream_id on every event, seq monotonic from 0.
        for (i, (_, payload)) in events.iter().enumerate() {
            assert_eq!(payload["stream_id"], serde_json::json!("id-1"));
            assert_eq!(payload["seq"], serde_json::json!(i as u64));
        }

        // Caller metadata is preserved alongside the envelope.
        assert_eq!(events[0].1["total"], serde_json::json!(2));
        assert_eq!(events[1].1["index"], serde_json::json!(0));
    }

    #[test]
    fn error_carries_message() {
        let sink = Arc::new(RecordingSink::default());
        let stream = Stream::new(sink.clone(), "arbor://demo", "id-2");
        stream.error("boom");

        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "arbor://demo-error");
        assert_eq!(events[0].1["error"], serde_json::json!("boom"));
        assert_eq!(events[0].1["stream_id"], serde_json::json!("id-2"));
        assert_eq!(events[0].1["seq"], serde_json::json!(0u64));
    }

    #[test]
    fn clone_shares_seq_counter() {
        let sink = Arc::new(RecordingSink::default());
        let stream = Stream::new(sink.clone(), "arbor://demo", "id-3");
        let cloned = stream.clone();

        stream.started(serde_json::json!({}));
        cloned.chunk(serde_json::json!({}));

        let events = sink.events.lock().unwrap();
        assert_eq!(events[0].1["seq"], serde_json::json!(0u64));
        // The clone shares the atomic, so the second emit is seq 1, not a reset 0.
        assert_eq!(events[1].1["seq"], serde_json::json!(1u64));
    }
}
