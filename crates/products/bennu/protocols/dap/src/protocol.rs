//! The DAP **envelope**: what every message on the wire looks like, and how an incoming one is
//! classified.
//!
//! Three kinds travel over the [`bennu_framed`] transport, and they are told apart by which fields
//! are present rather than by a discriminator we can trust — adapters are written by many hands and
//! `type` is occasionally the only field they all agree on:
//!
//! ```text
//! → {"seq":1,"type":"request","command":"initialize","arguments":{…}}
//! ← {"seq":1,"type":"response","request_seq":1,"success":true,"command":"initialize","body":{…}}
//! ← {"seq":2,"type":"event","event":"initialized"}
//! ```
//!
//! ## Two things that make DAP not-quite-JSON-RPC
//!
//! 1. **`seq` is per-sender and both sides count independently.** Ours starts at 1 and only ever
//!    increases; the adapter's is its own and means nothing to us except as the `request_seq` we
//!    have to echo. Correlating a response by the adapter's `seq` instead of by `request_seq` is the
//!    classic DAP client bug, and it appears to work until two requests are in flight.
//!
//! 2. **The adapter sends requests too** — `runInTerminal`, `startDebugging`. A client that treats
//!    every incoming message as a response or an event will hang the adapter, which is waiting for
//!    an answer it will never get. See [`Incoming::classify`].
//!
//! ## Failure is a response, not an error
//!
//! `success: false` is an ordinary response carrying a short `message` and often a richer
//! `body.error`. So "evaluate failed because the expression is nonsense" arrives the same way
//! "evaluate succeeded" does, and the caller decides what to do about it — which is why
//! [`Response::error_text`] exists rather than the reader throwing.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A message we are about to send.
///
/// Serialised with `seq` first because it costs nothing and makes a captured transcript readable in
/// the order the fields matter.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Outgoing {
    Request {
        seq: i64,
        command: String,
        /// Omitted entirely when there are none: a few adapters reject `"arguments": null`, and a
        /// request with no arguments is the spec's own shape for several commands.
        #[serde(skip_serializing_if = "Option::is_none")]
        arguments: Option<Value>,
    },
    /// Our answer to one of the adapter's own requests.
    Response {
        seq: i64,
        request_seq: i64,
        success: bool,
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<Value>,
    },
}

/// A message as it arrives — every field optional, because which ones are present is exactly what
/// says the kind.
#[derive(Debug, Clone, Deserialize)]
pub struct Incoming {
    /// The adapter's own counter. Kept for diagnostics and for nothing else: it is **not** how a
    /// response is matched to a request.
    #[serde(default)]
    pub seq: i64,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub request_seq: Option<i64>,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub body: Option<Value>,
    #[serde(default)]
    pub arguments: Option<Value>,
}

/// What an incoming message turned out to be.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// An answer to something we asked. `seq` is the **request's** seq, ours.
    Response(Response),
    /// Something happened in the debuggee.
    Event(Event),
    /// The adapter is asking *us* for something and is blocked until we answer.
    Request(AdapterRequest),
}

/// An answer to one of our requests.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    /// The seq of the request this answers — ours.
    pub request_seq: i64,
    pub command: String,
    pub success: bool,
    /// The adapter's short failure id, when it failed.
    pub message: Option<String>,
    pub body: Option<Value>,
}

impl Response {
    /// What to show a user when this failed, in one line.
    ///
    /// Adapters put the readable text in three different places and none of them is reliably
    /// present: `message` is often an id like `"cannotEvaluate"`, `body.error.format` is the
    /// spec's place for prose, and some adapters put prose in `message` and nothing else. So all
    /// three are tried and the first that is actually a sentence wins.
    pub fn error_text(&self) -> String {
        let format = self
            .body
            .as_ref()
            .and_then(|b| b.get("error"))
            .and_then(|e| e.get("format"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(text) = format {
            return text.to_string();
        }
        match self.message.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(m) => m.to_string(),
            None => format!("the adapter refused `{}` without saying why", self.command),
        }
    }

    /// The body, deserialised. A **successful** response with no body yields the type's default,
    /// which is what the spec means by an optional body — `continue` legitimately sends none.
    pub fn parse<T: serde::de::DeserializeOwned + Default>(&self) -> Result<T, String> {
        match &self.body {
            Some(body) => serde_json::from_value(body.clone())
                .map_err(|e| format!("the adapter's `{}` body did not fit: {e}", self.command)),
            None => Ok(T::default()),
        }
    }
}

/// Something the adapter reported without being asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub event: String,
    pub body: Option<Value>,
}

impl Event {
    /// The body, deserialised; the default when there is none.
    pub fn parse<T: serde::de::DeserializeOwned + Default>(&self) -> Result<T, String> {
        match &self.body {
            Some(body) => serde_json::from_value(body.clone())
                .map_err(|e| format!("the adapter's `{}` event body did not fit: {e}", self.event)),
            None => Ok(T::default()),
        }
    }
}

/// A request from the adapter to us. It is **blocked** until answered.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterRequest {
    /// The adapter's seq — what our answer must carry as `request_seq`.
    pub seq: i64,
    pub command: String,
    pub arguments: Option<Value>,
}

impl Incoming {
    /// Which of the three this is, or `None` when it is none of them.
    ///
    /// Field presence decides, and the order matters. `request_seq` is checked **first**: it is the
    /// only field unique to a response, and an adapter that also fills in `command` (all of them do,
    /// the spec requires it) would otherwise be misread as a request — which is the bug that makes
    /// every response look like a reverse request and hangs the session on the first one.
    ///
    /// `type` is consulted only as a tie-break, because it is the field most likely to be wrong.
    pub fn classify(self) -> Option<Message> {
        if let Some(request_seq) = self.request_seq {
            return Some(Message::Response(Response {
                request_seq,
                command: self.command.unwrap_or_default(),
                // Absent `success` on a response is a malformed message; treating it as failure is
                // the safe reading — a caller that believes a failed call succeeded goes on to
                // parse a body that is not there.
                success: self.success.unwrap_or(false),
                message: self.message,
                body: self.body,
            }));
        }
        if let Some(event) = self.event {
            return Some(Message::Event(Event { event, body: self.body }));
        }
        if let Some(command) = self.command {
            return Some(Message::Request(AdapterRequest {
                seq: self.seq,
                command,
                arguments: self.arguments,
            }));
        }
        None
    }
}

/// A monotonic `seq` source. Ours starts at 1, as the spec requires.
#[derive(Debug)]
pub struct Seq(std::sync::atomic::AtomicI64);

impl Default for Seq {
    fn default() -> Self {
        Seq(std::sync::atomic::AtomicI64::new(1))
    }
}

impl Seq {
    /// The next number, consumed. Never returns the same value twice.
    pub fn next(&self) -> i64 {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(json: &str) -> Option<Message> {
        serde_json::from_str::<Incoming>(json).expect("parses").classify()
    }

    #[test]
    fn a_response_is_recognised_by_its_request_seq() {
        let m = classify(r#"{"seq":7,"type":"response","request_seq":3,"success":true,"command":"threads","body":{"threads":[]}}"#);
        let Some(Message::Response(r)) = m else { panic!("{m:?}") };
        // Correlated by OUR seq, not the adapter's.
        assert_eq!(r.request_seq, 3);
        assert_eq!(r.command, "threads");
        assert!(r.success);
    }

    #[test]
    fn an_event_is_recognised_by_its_event_name() {
        let m = classify(r#"{"seq":2,"type":"event","event":"stopped","body":{"reason":"breakpoint"}}"#);
        let Some(Message::Event(e)) = m else { panic!("{m:?}") };
        assert_eq!(e.event, "stopped");
    }

    /// The adapter asks us things, and it is blocked until we answer. A client that reads this as an
    /// event drops it and the session stops dead.
    #[test]
    fn a_reverse_request_is_recognised_and_carries_the_seq_to_answer() {
        let m = classify(
            r#"{"seq":11,"type":"request","command":"runInTerminal","arguments":{"args":["./app"]}}"#,
        );
        let Some(Message::Request(r)) = m else { panic!("{m:?}") };
        assert_eq!(r.seq, 11, "our response must echo this as request_seq");
        assert_eq!(r.command, "runInTerminal");
        assert!(r.arguments.is_some());
    }

    /// The ordering that matters: a response carries `command` too (the spec requires it), so
    /// checking `command` first would read every response as a reverse request.
    #[test]
    fn a_response_is_not_mistaken_for_a_request_because_it_has_a_command() {
        let m = classify(
            r#"{"seq":4,"type":"response","request_seq":1,"success":true,"command":"initialize"}"#,
        );
        assert!(matches!(m, Some(Message::Response(_))), "{m:?}");
    }

    /// `type` is the field most likely to be wrong, so it is not what decides.
    #[test]
    fn classification_survives_a_wrong_type_field() {
        let m = classify(r#"{"seq":4,"type":"event","request_seq":2,"success":true,"command":"next"}"#);
        assert!(matches!(m, Some(Message::Response(_))), "request_seq wins: {m:?}");
    }

    #[test]
    fn a_message_that_is_none_of_the_three_is_rejected_rather_than_guessed() {
        assert!(classify(r#"{"seq":9,"type":"response"}"#).is_none());
        assert!(classify(r#"{}"#).is_none());
    }

    #[test]
    fn a_response_missing_success_is_read_as_failure() {
        let m = classify(r#"{"seq":4,"type":"response","request_seq":1,"command":"evaluate"}"#);
        let Some(Message::Response(r)) = m else { panic!("{m:?}") };
        assert!(!r.success, "believing a malformed response succeeded means parsing a missing body");
    }

    #[test]
    fn the_error_text_prefers_the_spec_s_place_for_prose() {
        let r = Response {
            request_seq: 1,
            command: "evaluate".into(),
            success: false,
            message: Some("cannotEvaluate".into()),
            body: Some(serde_json::json!({ "error": { "format": "no symbol named `foo`" } })),
        };
        assert_eq!(r.error_text(), "no symbol named `foo`");
    }

    #[test]
    fn the_error_text_falls_back_to_the_short_message_then_to_saying_so() {
        let mut r = Response {
            request_seq: 1,
            command: "evaluate".into(),
            success: false,
            message: Some("cannotEvaluate".into()),
            body: None,
        };
        assert_eq!(r.error_text(), "cannotEvaluate");
        r.message = None;
        assert!(r.error_text().contains("evaluate"), "names the command it was: {}", r.error_text());
        // An empty string is not prose, and reporting it as the reason shows a blank error.
        r.message = Some("   ".into());
        assert!(r.error_text().contains("evaluate"), "{}", r.error_text());
    }

    /// A successful response with no body is legal — `continue` and `next` send none — and must not
    /// read as a parse failure.
    #[test]
    fn a_bodiless_response_parses_to_the_default() {
        #[derive(Default, Deserialize, PartialEq, Debug)]
        struct Body {
            #[serde(default)]
            all_threads_continued: bool,
        }
        let r = Response {
            request_seq: 1,
            command: "continue".into(),
            success: true,
            message: None,
            body: None,
        };
        assert_eq!(r.parse::<Body>().unwrap(), Body { all_threads_continued: false });
    }

    #[test]
    fn a_body_of_the_wrong_shape_says_which_command_it_was() {
        #[derive(Debug, Default, Deserialize)]
        struct Body {
            #[allow(dead_code)]
            threads: Vec<i64>,
        }
        let r = Response {
            request_seq: 1,
            command: "threads".into(),
            success: true,
            message: None,
            body: Some(serde_json::json!({ "threads": "not a list" })),
        };
        let err = r.parse::<Body>().unwrap_err();
        assert!(err.contains("threads"), "{err}");
    }

    #[test]
    fn a_request_with_no_arguments_omits_the_field_entirely() {
        let out = Outgoing::Request { seq: 1, command: "threads".into(), arguments: None };
        let json = serde_json::to_string(&out).unwrap();
        assert!(!json.contains("arguments"), "some adapters reject a null: {json}");
        assert!(json.contains(r#""type":"request""#), "{json}");
    }

    #[test]
    fn our_seq_starts_at_one_and_never_repeats() {
        let seq = Seq::default();
        assert_eq!(seq.next(), 1, "the spec says the first is 1");
        assert_eq!(seq.next(), 2);
        assert_eq!(seq.next(), 3);
    }
}
