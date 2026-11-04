//! Server-Sent Events: the `text/event-stream` format, read incrementally.
//!
//! A stream arrives in arbitrary chunks — half a line, three events and a bit — so the parser keeps what it has
//! not finished and gives back the events completed so far ([`SseParser::feed`]). The rules are the WHATWG
//! ones: `field: value` lines, a blank line dispatches, `:` starts a comment, `data` lines join with `\n`,
//! `id` and `retry` update the stream's state, and a leading BOM is ignored. Nothing here touches the network.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SseEvent {
    /// The `id` in force when the event was dispatched (it persists across events, as `Last-Event-ID` does).
    pub id: Option<String>,
    /// `message` unless the stream named one.
    pub event: String,
    pub data: String,
    /// A `retry:` that arrived with (or before) this event: milliseconds to wait before reconnecting.
    pub retry: Option<u64>,
}

#[derive(Debug, Default)]
pub struct SseParser {
    /// Text received but not yet ended by a line break.
    pending: String,
    started: bool,
    /// A `\r` ended the last chunk: a following `\n` belongs to it.
    cr_last: bool,
    // The event being built.
    event: String,
    data: Vec<String>,
    // State that outlives an event.
    last_id: Option<String>,
    retry: Option<u64>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// The `Last-Event-ID` to send when reconnecting.
    pub fn last_event_id(&self) -> Option<&str> {
        self.last_id.as_deref()
    }

    /// The latest `retry` the server asked for, in milliseconds.
    pub fn retry_ms(&self) -> Option<u64> {
        self.retry
    }

    /// Feed a chunk; the events it completed come back in order.
    pub fn feed(&mut self, chunk: &str) -> Vec<SseEvent> {
        let mut text = chunk;
        if !self.started {
            text = text.strip_prefix('\u{feff}').unwrap_or(text);
            self.started = !chunk.is_empty();
        }
        if self.cr_last && text.starts_with('\n') {
            text = &text[1..];
        }
        if !text.is_empty() {
            self.cr_last = text.ends_with('\r');
        }
        self.pending.push_str(text);

        let mut events = Vec::new();
        loop {
            let Some(end) = self.pending.find(['\n', '\r']) else { break };
            let line = self.pending[..end].to_string();
            // `\r\n` is one break; a lone `\r` at the very end waits (its `\n` may come next chunk).
            let skip = if self.pending[end..].starts_with("\r\n") { 2 } else { 1 };
            self.pending.drain(..end + skip);
            if let Some(e) = self.line(&line) {
                events.push(e);
            }
        }
        events
    }

    fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = value.to_string(),
            "data" => self.data.push(value.to_string()),
            "id" if !value.contains('\0') => self.last_id = Some(value.to_string()),
            "retry" => {
                if let Ok(ms) = value.parse::<u64>() {
                    self.retry = Some(ms);
                }
            }
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        let event = std::mem::take(&mut self.event);
        let data = std::mem::take(&mut self.data);
        if data.is_empty() {
            return None;
        }
        Some(SseEvent { id: self.last_id.clone(), event: if event.is_empty() { "message".into() } else { event }, data: data.join("\n"), retry: self.retry })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(chunks: &[&str]) -> (Vec<SseEvent>, SseParser) {
        let mut p = SseParser::new();
        let events = chunks.iter().flat_map(|c| p.feed(c)).collect();
        (events, p)
    }

    #[test]
    fn a_blank_line_dispatches_and_the_default_event_is_message() {
        let (e, _) = all(&["data: hello\n\n"]);
        assert_eq!(e, vec![SseEvent { id: None, event: "message".into(), data: "hello".into(), retry: None }]);
    }

    #[test]
    fn data_lines_join_with_a_newline_and_the_event_name_and_id_are_kept() {
        let (e, p) = all(&["id: 7\nevent: tick\ndata: a\ndata: b\n\n"]);
        assert_eq!((e[0].event.as_str(), e[0].data.as_str(), e[0].id.as_deref()), ("tick", "a\nb", Some("7")));
        assert_eq!(p.last_event_id(), Some("7"));
    }

    #[test]
    fn an_event_may_arrive_in_pieces() {
        let (e, _) = all(&["da", "ta: par", "t one\n", "\n", "data: two\n\nda"]);
        assert_eq!(e.iter().map(|x| x.data.as_str()).collect::<Vec<_>>(), ["part one", "two"]);
    }

    #[test]
    fn all_three_line_endings_work_even_when_crlf_is_split_across_chunks() {
        let (e, _) = all(&["data: a\r\n\r\ndata: b\r\r", "\n", "data: c\n\n"]);
        assert_eq!(e.iter().map(|x| x.data.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
    }

    #[test]
    fn comments_unknown_fields_and_events_without_data_are_ignored() {
        let (e, _) = all(&[": keep-alive\n\nfoo: bar\n\nevent: only-a-name\n\ndata: x\n\n"]);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].event, "message", "the name of an event that never had data does not leak into the next");
    }

    #[test]
    fn the_id_persists_across_events_and_retry_is_remembered() {
        let (e, p) = all(&["id: 1\nretry: 5000\ndata: a\n\ndata: b\n\n"]);
        assert_eq!(e[1].id.as_deref(), Some("1"));
        assert_eq!((e[0].retry, p.retry_ms()), (Some(5000), Some(5000)));
        let (bad, p) = all(&["retry: soon\ndata: x\n\n"]);
        assert_eq!((bad[0].retry, p.retry_ms()), (None, None), "a retry that is not a number is ignored");
    }

    #[test]
    fn a_field_without_a_colon_is_the_field_with_an_empty_value_and_one_space_is_dropped() {
        let (e, _) = all(&["data\n\ndata:  two spaces\n\n"]);
        assert_eq!(e[0].data, "");
        assert_eq!(e[1].data, " two spaces");
    }

    #[test]
    fn a_byte_order_mark_at_the_start_is_skipped() {
        let (e, _) = all(&["\u{feff}data: x\n\n"]);
        assert_eq!(e[0].data, "x");
    }
}
