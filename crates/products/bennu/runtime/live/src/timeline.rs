//! One chronological stream of everything that happened in a live session: what each process
//! printed, each HTTP request and its response, and markers (process started / exited, scenario
//! steps). Hooks for later sources (BRP state, DB rows) are just more [`EventKind`]s.
//!
//! The point is correlation. A request carries an `X-Correlation-Id`; services that log it (MDC,
//! a request filter) make every line of that request findable across processes with [`trace`], which
//! is how "B returned 500 because A sent a null field" becomes a query instead of a guess.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::clock::now_ms;

/// An event's text is cut here; the full line stays in the process's own log buffer.
const MAX_EVENT_CHARS: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A line a managed process wrote.
    Log,
    HttpRequest,
    HttpResponse,
    ProcessStarted,
    ProcessReady,
    ProcessExited,
    /// A scenario step, a note: anything that orients a reader.
    Marker,
}

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub seq: u64,
    pub ts_ms: u64,
    pub kind: EventKind,
    /// Who produced it: a process name, or `http`.
    pub source: String,
    pub process_id: Option<String>,
    pub corr: Option<String>,
    pub text: String,
    /// Structured detail (status, duration, ...), for a view that wants more than the text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// What a reader asks for.
#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub since_seq: Option<u64>,
    pub after_ms: Option<u64>,
    pub kinds: Vec<EventKind>,
    pub source: Option<String>,
    pub text: Option<Regex>,
    pub corr: Option<String>,
    /// The newest `limit` matches.
    pub limit: Option<usize>,
}

impl Filter {
    pub fn matches(&self, e: &Event) -> bool {
        self.since_seq.is_none_or(|s| e.seq > s)
            && self.after_ms.is_none_or(|t| e.ts_ms >= t)
            && (self.kinds.is_empty() || self.kinds.contains(&e.kind))
            && self.source.as_ref().is_none_or(|s| e.source.eq_ignore_ascii_case(s))
            && self.corr.as_ref().is_none_or(|c| mentions(e, c))
            && self.text.as_ref().is_none_or(|re| re.is_match(&e.text))
    }
}

/// Whether the event belongs to `corr`: tagged with it, or its text carries it.
fn mentions(e: &Event, corr: &str) -> bool {
    e.corr.as_deref() == Some(corr) || e.text.contains(corr)
}

type Observer = Arc<dyn Fn() + Send + Sync>;

pub struct Timeline {
    events: Mutex<VecDeque<Event>>,
    cap: usize,
    next_seq: AtomicU64,
    observer: Mutex<Option<Observer>>,
}

impl Timeline {
    pub fn new(cap: usize) -> Self {
        Self { events: Mutex::new(VecDeque::new()), cap: cap.max(100), next_seq: AtomicU64::new(1), observer: Mutex::new(None) }
    }

    /// Called after every push (must be cheap: it runs on the reader threads). The Live panel uses it
    /// to schedule a throttled refresh.
    pub fn set_observer(&self, f: Observer) {
        *self.observer.lock().unwrap_or_else(|p| p.into_inner()) = Some(f);
    }

    pub fn push(&self, kind: EventKind, source: &str, process_id: Option<&str>, corr: Option<&str>, text: &str, data: Option<Value>) -> u64 {
        self.push_at(now_ms(), kind, source, process_id, corr, text, data)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_at(&self, ts_ms: u64, kind: EventKind, source: &str, process_id: Option<&str>, corr: Option<&str>, text: &str, data: Option<Value>) -> u64 {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let text: String = if text.chars().count() > MAX_EVENT_CHARS { text.chars().take(MAX_EVENT_CHARS).collect() } else { text.to_string() };
        let event = Event { seq, ts_ms, kind, source: source.to_string(), process_id: process_id.map(str::to_string), corr: corr.map(str::to_string), text, data };
        {
            let mut events = self.events.lock().unwrap_or_else(|p| p.into_inner());
            events.push_back(event);
            while events.len() > self.cap {
                events.pop_front();
            }
        }
        if let Some(f) = self.observer.lock().unwrap_or_else(|p| p.into_inner()).clone() {
            f();
        }
        seq
    }

    pub fn last_seq(&self) -> u64 {
        self.next_seq.load(Ordering::SeqCst) - 1
    }

    pub fn query(&self, filter: &Filter) -> Vec<Event> {
        let events = self.events.lock().unwrap_or_else(|p| p.into_inner());
        let mut out: Vec<Event> = events.iter().filter(|e| filter.matches(e)).cloned().collect();
        if let Some(n) = filter.limit {
            if out.len() > n {
                out.drain(..out.len() - n);
            }
        }
        out
    }

    pub fn clear(&self) {
        self.events.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }
}

/// Merge several already-ordered streams into one, by time then by sequence. Stable: events with the
/// same timestamp from the same stream keep their order.
pub fn merge(streams: Vec<Vec<Event>>) -> Vec<Event> {
    let mut all: Vec<Event> = streams.into_iter().flatten().collect();
    all.sort_by_key(|e| (e.ts_ms, e.seq));
    all
}

/// Everything one correlation id touched.
#[derive(Debug, Clone, Serialize)]
pub struct Trace {
    pub corr: String,
    /// Request, response and the log lines, in time order.
    pub events: Vec<Event>,
    /// Which sources contributed (process names and `http`).
    pub sources: Vec<String>,
}

/// Every event that mentions `corr`, plus up to `context` continuation lines after each log line that
/// does — a stack trace is the lines *after* the one carrying the id, and without them a trace
/// stops at "Exception in handler" and says nothing about where.
pub fn trace(events: &[Event], corr: &str, context: usize) -> Trace {
    let mut picked = vec![false; events.len()];
    for (i, e) in events.iter().enumerate() {
        if !mentions(e, corr) {
            continue;
        }
        picked[i] = true;
        if e.kind != EventKind::Log || context == 0 {
            continue;
        }
        let mut taken = 0;
        for (j, next) in events.iter().enumerate().skip(i + 1) {
            if taken >= context {
                break;
            }
            if next.process_id != e.process_id || next.kind != EventKind::Log {
                // Other processes interleave freely; only this process's own lines continue the entry.
                continue;
            }
            if !looks_like_continuation(&next.text) {
                break;
            }
            picked[j] = true;
            taken += 1;
        }
    }
    let events: Vec<Event> = events.iter().zip(&picked).filter(|(_, p)| **p).map(|(e, _)| e.clone()).collect();
    let mut sources: Vec<String> = Vec::new();
    for e in &events {
        if !sources.contains(&e.source) {
            sources.push(e.source.clone());
        }
    }
    Trace { corr: corr.to_string(), events, sources }
}

/// A line that belongs to the entry above it: an indented frame, `Caused by`, `... N more`.
fn looks_like_continuation(text: &str) -> bool {
    let t = text.trim_start();
    text.starts_with([' ', '\t']) || t.starts_with("at ") || t.starts_with("Caused by") || t.starts_with("... ") || t.starts_with("Suppressed:")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(tl: &Timeline, ts: u64, pid: &str, text: &str) {
        tl.push_at(ts, EventKind::Log, pid, Some(pid), None, text, None);
    }

    #[test]
    fn events_are_ordered_capped_and_filterable() {
        let tl = Timeline::new(100);
        for i in 0..150 {
            log(&tl, i, "api", &format!("line {i}"));
        }
        let all = tl.query(&Filter::default());
        assert_eq!(all.len(), 100);
        assert_eq!(all[0].text, "line 50");
        let newest = tl.query(&Filter { limit: Some(3), ..Default::default() });
        assert_eq!(newest.last().unwrap().text, "line 149");
        let after = tl.query(&Filter { since_seq: Some(148), ..Default::default() });
        assert_eq!(after.len(), 2);
    }

    #[test]
    fn merge_orders_by_time_across_streams() {
        let a = Timeline::new(100);
        let b = Timeline::new(100);
        log(&a, 10, "a", "a1");
        log(&a, 30, "a", "a2");
        log(&b, 20, "b", "b1");
        let merged = merge(vec![a.query(&Filter::default()), b.query(&Filter::default())]);
        assert_eq!(merged.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), vec!["a1", "b1", "a2"]);
    }

    #[test]
    fn a_trace_follows_one_id_across_processes_and_takes_the_stack_after_it() {
        let tl = Timeline::new(100);
        tl.push_at(1, EventKind::HttpRequest, "http", None, Some("bl-1"), "GET /orders/7", None);
        log(&tl, 2, "gateway", "INFO forwarding bl-1 to orders");
        log(&tl, 3, "orders", "ERROR bl-1 failed to load order");
        log(&tl, 4, "gateway", "INFO unrelated bl-2");
        log(&tl, 5, "orders", "java.lang.NullPointerException: customer");
        log(&tl, 6, "orders", "\tat com.acme.Orders.load(Orders.java:41)");
        log(&tl, 7, "orders", "\tat com.acme.Api.get(Api.java:12)");
        log(&tl, 8, "orders", "INFO next request");
        tl.push_at(9, EventKind::HttpResponse, "http", None, Some("bl-1"), "500 in 8 ms", None);
        let events = tl.query(&Filter::default());
        let t = trace(&events, "bl-1", 5);
        let texts: Vec<&str> = t.events.iter().map(|e| e.text.as_str()).collect();
        assert!(texts.contains(&"GET /orders/7") && texts.contains(&"500 in 8 ms"));
        assert!(texts.iter().any(|s| s.contains("forwarding bl-1")));
        assert!(!texts.iter().any(|s| s.contains("bl-2")));
        // The exception line is not a continuation (no indent), so the trace stops before it; the frames
        // belong to a line that does mention the id only when they follow it directly.
        assert!(!texts.iter().any(|s| s.contains("NullPointerException")));
        assert_eq!(t.sources, vec!["http", "gateway", "orders"]);
    }

    #[test]
    fn indented_frames_directly_after_a_matching_line_are_included() {
        let tl = Timeline::new(100);
        log(&tl, 1, "orders", "ERROR bl-9 boom");
        log(&tl, 2, "gateway", "noise from another process");
        log(&tl, 3, "orders", "    at com.acme.A.b(A.java:1)");
        log(&tl, 4, "orders", "Caused by: java.io.IOException");
        log(&tl, 5, "orders", "plain line ends it");
        log(&tl, 6, "orders", "    at too.late(X.java:1)");
        let t = trace(&tl.query(&Filter::default()), "bl-9", 10);
        assert_eq!(t.events.len(), 3, "{:?}", t.events.iter().map(|e| &e.text).collect::<Vec<_>>());
    }

    #[test]
    fn filter_by_corr_matches_tags_and_text() {
        let tl = Timeline::new(100);
        tl.push_at(1, EventKind::HttpRequest, "http", None, Some("c1"), "req", None);
        log(&tl, 2, "api", "handling c1");
        log(&tl, 3, "api", "other");
        let got = tl.query(&Filter { corr: Some("c1".into()), ..Default::default() });
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn the_observer_fires_on_every_push() {
        let tl = Timeline::new(100);
        let n = Arc::new(AtomicU64::new(0));
        let n2 = Arc::clone(&n);
        tl.set_observer(Arc::new(move || {
            n2.fetch_add(1, Ordering::SeqCst);
        }));
        log(&tl, 1, "a", "x");
        log(&tl, 2, "a", "y");
        assert_eq!(n.load(Ordering::SeqCst), 2);
    }
}
