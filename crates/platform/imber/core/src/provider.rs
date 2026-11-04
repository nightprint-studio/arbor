//! What a provider is, as far as the rest of the assistant is concerned.
//!
//! Two jobs, and neither touches the network. **Going out**: where a request is sent, how it is
//! authenticated, and what its body is for a given [`Request`]. **Coming back**: a decoder that is fed the
//! response's text as it arrives and hands back neutral [`Event`]s. A host owns the HTTP client and the
//! credential; it asks the provider what to send and what the answer means.
//!
//! The split is deliberate: the part that differs between providers is only ever a translation, and a
//! translation is a pure function — the one kind of code that can be tested exhaustively without a key,
//! a connection or a bill.

use serde_json::Value;

use crate::message::Request;

/// Why the model stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// It finished its answer.
    EndTurn,
    /// It wants tools run before it continues.
    ToolUse,
    /// It reached the length it was allowed.
    MaxTokens,
    /// It declined — a safety classifier, not an error. The answer so far is not a complete one.
    Refusal,
    /// A reason this provider names that the neutral model does not.
    Other,
}

/// What a request used, as the provider reports it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Input tokens that were served from the provider's cache — billed at a fraction.
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// The part of `cache_write_tokens` written to the one-hour cache; the rest went to the five-minute
    /// one. It costs more, so it is counted apart.
    pub cache_write_1h_tokens: u64,
}

impl Usage {
    /// Combines two reports of one request. A streamed response gives its numbers in parts and each is
    /// a running total, so the larger of the two is the later one — the input count comes with the first
    /// report and the output count grows with the answer.
    pub fn merge(self, later: Usage) -> Usage {
        Usage {
            input_tokens: later.input_tokens.max(self.input_tokens),
            output_tokens: later.output_tokens.max(self.output_tokens),
            cache_read_tokens: later.cache_read_tokens.max(self.cache_read_tokens),
            cache_write_tokens: later.cache_write_tokens.max(self.cache_write_tokens),
            cache_write_1h_tokens: later.cache_write_1h_tokens.max(self.cache_write_1h_tokens),
        }
    }
}

/// One thing a response stream said, in the provider-neutral form.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// More of the answer's text.
    Text(String),
    /// More of the model's reasoning, when the provider shows it. Never part of the answer.
    Thinking(String),
    /// The model began a tool call. Arguments are still arriving.
    ToolStart { id: String, name: String },
    /// A tool call is complete. `input` is parsed; `valid` is false when the arguments did not parse as
    /// JSON (a truncated stream, a model slip) — the host answers that call with an error result rather
    /// than running a tool on guessed arguments.
    ToolDone { id: String, name: String, input: Value, valid: bool },
    /// The provider's running count of what the request used.
    Usage(Usage),
    /// The response is over.
    Stop(StopReason),
    /// The provider reported an error in the stream itself: its error type (`overloaded_error`, …) when it
    /// named one, and its message.
    Error { error_type: Option<String>, message: String },
}

/// Where and how to send a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub url: String,
    /// Headers that do not depend on the credential (versions, content type).
    pub headers: Vec<(String, String)>,
}

/// Turns the bytes of a streamed response into [`Event`]s. Fed in whatever pieces they arrive in — a
/// chunk can end mid-line — and remembers what it has not finished reading.
pub trait Decoder {
    fn push(&mut self, chunk: &str) -> Vec<Event>;
    /// The stream ended: whatever was left unterminated is read now.
    fn finish(&mut self) -> Vec<Event>;
}

pub trait Provider {
    /// A stable name — what a settings file or a conversation refers to a provider by.
    fn id(&self) -> &'static str;

    fn endpoint(&self) -> Endpoint;

    /// The one header that carries the credential, as `(name, value)`. Separate from
    /// [`Self::endpoint`] because it is the part that must never be logged or stored.
    fn auth_header(&self, key: &str) -> (String, String);

    /// The JSON body for `request`, asking for a streamed response.
    fn body(&self, request: &Request) -> Value;

    fn decoder(&self) -> Box<dyn Decoder + Send>;

    /// Whether this model accepts an effort setting, and which levels.
    fn efforts(&self, model: &str) -> Vec<crate::effort::Effort>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_later_usage_report_only_ever_raises_a_count() {
        let early = Usage { input_tokens: 1200, output_tokens: 1, cache_read_tokens: 1000, ..Default::default() };
        let late = Usage { output_tokens: 340, ..Default::default() };
        let merged = early.merge(late);
        assert_eq!(merged.input_tokens, 1200, "the input count came with the first report");
        assert_eq!(merged.output_tokens, 340, "the output count grows as the answer does");
        assert_eq!(merged.cache_read_tokens, 1000);
    }
}
