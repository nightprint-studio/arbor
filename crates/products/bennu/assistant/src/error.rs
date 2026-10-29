//! What went wrong with a request to the provider, in terms the rest of the assistant can act on.
//!
//! The transport hands over raw facts — an HTTP status and body, a header, a class of network failure,
//! an `error` event inside the stream — and this turns them into one [`ProviderError`]: a [`ErrorKind`]
//! (which decides whether trying again can help), the provider's own wording kept verbatim as `detail`,
//! the wait the provider asked for, and a sentence a person can act on ("the API key was rejected — open
//! Imber settings"). Pure, so the whole table is tested without a network.

use std::fmt;
use std::time::Duration;

/// Why a request failed, as far as what to do next is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The API could not be reached at all: no network, DNS failure, connection refused.
    Offline,
    /// The connection went silent for longer than a live one does.
    Timeout,
    /// The connection broke (or ended early) while the answer was streaming.
    StreamBroke,
    /// 429: too many requests or tokens for now.
    RateLimited,
    /// 529 / `overloaded_error`: the provider has no capacity right now.
    Overloaded,
    /// 5xx / `api_error`: the provider failed on its side.
    Server,
    /// 401 / 403: the key is invalid or may not do this.
    Auth,
    /// No credit left, or a usage limit of the account was reached.
    Billing,
    /// 413, or a prompt past the model's window.
    ContextTooLong,
    /// 404: the model does not exist or the key has no access to it.
    ModelNotFound,
    /// The provider refused the request as malformed (any other 400).
    BadRequest,
    /// The user stopped the run.
    Cancelled,
    /// Something on this machine, before a request could be made.
    Local,
}

impl ErrorKind {
    /// A stable word for the panel and for logs.
    pub fn code(self) -> &'static str {
        match self {
            ErrorKind::Offline => "offline",
            ErrorKind::Timeout => "timeout",
            ErrorKind::StreamBroke => "stream_broke",
            ErrorKind::RateLimited => "rate_limited",
            ErrorKind::Overloaded => "overloaded",
            ErrorKind::Server => "server",
            ErrorKind::Auth => "auth",
            ErrorKind::Billing => "billing",
            ErrorKind::ContextTooLong => "context_too_long",
            ErrorKind::ModelNotFound => "model_not_found",
            ErrorKind::BadRequest => "bad_request",
            ErrorKind::Cancelled => "cancelled",
            ErrorKind::Local => "local",
        }
    }

    /// Whether sending the same request again can succeed without the user changing anything.
    pub fn retryable(self) -> bool {
        matches!(
            self,
            ErrorKind::Offline | ErrorKind::Timeout | ErrorKind::StreamBroke | ErrorKind::RateLimited | ErrorKind::Overloaded | ErrorKind::Server
        )
    }

    /// A failure of the connection itself (as opposed to the provider answering "not now"): these are waited
    /// out against a time budget, because how long a network stays down is not up to the provider.
    pub fn is_network(self) -> bool {
        matches!(self, ErrorKind::Offline | ErrorKind::Timeout | ErrorKind::StreamBroke)
    }

    /// The few words for a status line while the request is being retried.
    pub fn status_text(self) -> &'static str {
        match self {
            ErrorKind::Offline => "No connection",
            ErrorKind::Timeout => "The API is not answering",
            ErrorKind::StreamBroke => "The connection dropped",
            ErrorKind::RateLimited => "Rate limit reached",
            ErrorKind::Overloaded => "The API is overloaded",
            ErrorKind::Server => "The API had a server error",
            ErrorKind::Auth => "The API key was rejected",
            ErrorKind::Billing => "No credit or usage limit reached",
            ErrorKind::ContextTooLong => "The conversation is too long",
            ErrorKind::ModelNotFound => "Model not available",
            ErrorKind::BadRequest => "Request refused",
            ErrorKind::Cancelled => "Stopped",
            ErrorKind::Local => "Could not start the request",
        }
    }

    /// What happened, in plain words for a reader that is not the user (the model reading a tool result).
    pub fn what_happened(self) -> &'static str {
        match self {
            ErrorKind::Offline => "it could not reach the API",
            ErrorKind::Timeout => "the API stopped answering",
            ErrorKind::StreamBroke => "the connection dropped while the answer was streaming",
            ErrorKind::RateLimited => "the API rate limit was reached",
            ErrorKind::Overloaded => "the API was overloaded",
            ErrorKind::Server => "the provider had an internal error",
            ErrorKind::Auth => "the API key was rejected",
            ErrorKind::Billing => "the account has no credit or reached a usage limit",
            ErrorKind::ContextTooLong => "its conversation no longer fit the model's window",
            ErrorKind::ModelNotFound => "the model is not available to this key",
            ErrorKind::BadRequest => "the provider refused the request as invalid",
            ErrorKind::Cancelled => "it was stopped",
            ErrorKind::Local => "the request could not be made from this machine",
        }
    }

    /// What happened and what to do about it, for the failure the user is left with.
    pub fn advice(self) -> &'static str {
        match self {
            ErrorKind::Offline => "You are offline: the API could not be reached. Check your internet connection (or VPN/proxy), then press Retry.",
            ErrorKind::Timeout => "The API stopped answering and the request timed out. Press Retry.",
            ErrorKind::StreamBroke => "The connection dropped while the answer was streaming. Press Retry.",
            ErrorKind::RateLimited => "The rate limit was reached (too many requests or tokens per minute). Wait a little and press Retry, or use a lighter model or effort.",
            ErrorKind::Overloaded => "The API is overloaded at the moment. Press Retry in a minute.",
            ErrorKind::Server => "The provider had an internal error. Press Retry; if it keeps happening, try again later.",
            ErrorKind::Auth => "The API key was rejected (invalid, revoked, or without permission). Open Imber settings and check the key.",
            ErrorKind::Billing => "The account has no credit left or reached a usage limit. Check the plan and billing of the provider account, then press Retry.",
            ErrorKind::ContextTooLong => "The conversation no longer fits the model's window. Compact the conversation (or start a new one), then press Retry.",
            ErrorKind::ModelNotFound => "The model was not found, or the key has no access to it. Pick another model in Imber settings.",
            ErrorKind::BadRequest => "The provider refused the request as invalid. The detail below says why.",
            ErrorKind::Cancelled => "Stopped.",
            ErrorKind::Local => "The request could not be made from this machine.",
        }
    }
}

/// A failed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    pub kind: ErrorKind,
    /// The provider's own words (or the network error's), kept as they came: "invalid x-api-key" is the diagnosis.
    pub detail: String,
    /// How long the provider asked to wait before trying again.
    pub retry_after: Option<Duration>,
    /// Part of the answer had already streamed when this happened.
    pub partial: bool,
}

/// `retry-after` is a number of seconds (the date form is not used by the providers this talks to).
fn parse_retry_after(value: &str) -> Option<Duration> {
    let secs: f64 = value.trim().parse().ok()?;
    (secs.is_finite() && secs >= 0.0).then(|| Duration::from_secs_f64(secs.min(3600.0)))
}

fn mentions_billing(lower: &str) -> bool {
    ["credit balance", "billing", "usage limit", "insufficient", "out of credit"].iter().any(|w| lower.contains(w))
}

fn mentions_too_long(lower: &str) -> bool {
    lower.contains("prompt is too long") || lower.contains("too many tokens") || lower.contains("context window") || lower.contains("request_too_large") || (lower.contains("exceed") && lower.contains("context"))
}

/// The kind of a failure from whatever the provider said: the error type of its body first (it is the most
/// precise), then the status.
fn classify(status: Option<u16>, error_type: Option<&str>, message: &str) -> ErrorKind {
    let lower = message.to_lowercase();
    let invalid_request = |billing: bool, too_long: bool| {
        if billing {
            ErrorKind::Billing
        } else if too_long {
            ErrorKind::ContextTooLong
        } else {
            ErrorKind::BadRequest
        }
    };
    match error_type {
        Some("overloaded_error") => return ErrorKind::Overloaded,
        Some("rate_limit_error") => return ErrorKind::RateLimited,
        Some("authentication_error") | Some("permission_error") => return ErrorKind::Auth,
        Some("billing_error") => return ErrorKind::Billing,
        Some("not_found_error") => return ErrorKind::ModelNotFound,
        Some("request_too_large") => return ErrorKind::ContextTooLong,
        Some("timeout_error") => return ErrorKind::Timeout,
        Some("api_error") => return ErrorKind::Server,
        Some("unreadable_event") => return ErrorKind::StreamBroke,
        Some("invalid_request_error") => return invalid_request(mentions_billing(&lower), mentions_too_long(&lower)),
        _ => {}
    }
    match status {
        Some(401) | Some(403) => ErrorKind::Auth,
        Some(402) => ErrorKind::Billing,
        Some(404) => ErrorKind::ModelNotFound,
        Some(408) => ErrorKind::Timeout,
        Some(413) => ErrorKind::ContextTooLong,
        Some(429) => ErrorKind::RateLimited,
        Some(529) => ErrorKind::Overloaded,
        Some(s) if (500..600).contains(&s) => ErrorKind::Server,
        Some(s) if (400..500).contains(&s) => invalid_request(mentions_billing(&lower), mentions_too_long(&lower)),
        // An error event with a type this does not know: the provider failed in a way it did not describe.
        _ => ErrorKind::Server,
    }
}

impl ProviderError {
    pub fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self { kind, detail: detail.into(), retry_after: None, partial: false }
    }

    pub fn offline(detail: impl Into<String>) -> Self {
        Self::new(ErrorKind::Offline, detail)
    }

    pub fn timeout(detail: impl Into<String>) -> Self {
        Self::new(ErrorKind::Timeout, detail)
    }

    pub fn stream_broke(detail: impl Into<String>) -> Self {
        Self::new(ErrorKind::StreamBroke, detail)
    }

    pub fn local(detail: impl Into<String>) -> Self {
        Self::new(ErrorKind::Local, detail)
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorKind::Cancelled, "")
    }

    pub fn with_retry_after(mut self, wait: Option<Duration>) -> Self {
        self.retry_after = wait;
        self
    }

    /// An HTTP answer that was not a success: its status, its `retry-after` header if any, and its body.
    pub fn from_response(status: u16, retry_after: Option<&str>, body: &str) -> Self {
        let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
        let error_type = parsed.as_ref().and_then(|v| v.pointer("/error/type")).and_then(|t| t.as_str()).map(str::to_string);
        let message = parsed
            .as_ref()
            .and_then(|v| v.pointer("/error/message"))
            .and_then(|m| m.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| body.trim().chars().take(400).collect());
        let kind = classify(Some(status), error_type.as_deref(), &message);
        let detail = if message.is_empty() { status.to_string() } else { format!("{status}: {message}") };
        Self::new(kind, detail).with_retry_after(retry_after.and_then(parse_retry_after))
    }

    /// An `error` event inside a stream that had started well (`overloaded_error` arrives this way).
    pub fn from_stream_event(error_type: Option<&str>, message: &str) -> Self {
        Self::new(classify(None, error_type, message), message)
    }

    pub fn retryable(&self) -> bool {
        self.kind.retryable()
    }

    /// The sentence a person reads: what happened and what to do.
    pub fn friendly(&self) -> String {
        self.kind.advice().to_string()
    }

    /// The friendly sentence followed by the provider's words, in one string (for places with one line to spend).
    pub fn full(&self) -> String {
        if self.detail.trim().is_empty() {
            self.friendly()
        } else {
            format!("{} ({})", self.friendly(), self.detail.trim())
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.full())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(kind: &str, message: &str) -> String {
        format!(r#"{{"type":"error","error":{{"type":"{kind}","message":"{message}"}}}}"#)
    }

    #[test]
    fn statuses_and_bodies_map_to_kinds() {
        let cases: &[(u16, &str, &str, ErrorKind, bool)] = &[
            (401, "authentication_error", "invalid x-api-key", ErrorKind::Auth, false),
            (403, "permission_error", "not allowed", ErrorKind::Auth, false),
            (404, "not_found_error", "model: nope", ErrorKind::ModelNotFound, false),
            (413, "request_too_large", "Request exceeds the maximum allowed number of bytes", ErrorKind::ContextTooLong, false),
            (429, "rate_limit_error", "slow down", ErrorKind::RateLimited, true),
            (500, "api_error", "Internal server error", ErrorKind::Server, true),
            (529, "overloaded_error", "Overloaded", ErrorKind::Overloaded, true),
            (400, "invalid_request_error", "prompt is too long: 250000 tokens > 200000 maximum", ErrorKind::ContextTooLong, false),
            (400, "invalid_request_error", "Your credit balance is too low to access the API", ErrorKind::Billing, false),
            (400, "invalid_request_error", "messages.0.content: required", ErrorKind::BadRequest, false),
        ];
        for (status, ty, message, kind, retryable) in cases {
            let e = ProviderError::from_response(*status, None, &body(ty, message));
            assert_eq!(e.kind, *kind, "{status} {ty}");
            assert_eq!(e.retryable(), *retryable, "{status} {ty}");
            assert!(e.detail.contains(message), "the provider's wording is kept: {}", e.detail);
        }
    }

    #[test]
    fn a_body_that_is_not_json_is_classified_by_its_status() {
        assert_eq!(ProviderError::from_response(502, None, "<html>Bad Gateway</html>").kind, ErrorKind::Server);
        assert_eq!(ProviderError::from_response(503, None, "").kind, ErrorKind::Server);
        assert_eq!(ProviderError::from_response(408, None, "").kind, ErrorKind::Timeout);
        assert_eq!(ProviderError::from_response(402, None, "pay up").kind, ErrorKind::Billing);
        assert_eq!(ProviderError::from_response(418, None, "teapot").kind, ErrorKind::BadRequest);
        assert!(ProviderError::from_response(503, None, "").detail.starts_with("503"));
    }

    #[test]
    fn the_type_in_the_body_wins_over_the_status() {
        // A gateway can answer an overloaded provider with 500.
        let e = ProviderError::from_response(500, None, &body("overloaded_error", "Overloaded"));
        assert_eq!(e.kind, ErrorKind::Overloaded);
    }

    #[test]
    fn retry_after_is_read_in_seconds() {
        let e = ProviderError::from_response(429, Some(" 7 "), &body("rate_limit_error", "x"));
        assert_eq!(e.retry_after, Some(Duration::from_secs(7)));
        assert_eq!(ProviderError::from_response(429, Some("soon"), "").retry_after, None);
        assert_eq!(ProviderError::from_response(429, Some("-3"), "").retry_after, None);
        assert_eq!(ProviderError::from_response(429, Some("999999"), "").retry_after, Some(Duration::from_secs(3600)));
    }

    #[test]
    fn errors_inside_a_stream_are_mapped_by_their_type() {
        assert_eq!(ProviderError::from_stream_event(Some("overloaded_error"), "Overloaded").kind, ErrorKind::Overloaded);
        assert_eq!(ProviderError::from_stream_event(Some("api_error"), "boom").kind, ErrorKind::Server);
        assert_eq!(ProviderError::from_stream_event(Some("unreadable_event"), "garbled").kind, ErrorKind::StreamBroke);
        assert_eq!(ProviderError::from_stream_event(None, "something").kind, ErrorKind::Server);
        assert_eq!(ProviderError::from_stream_event(Some("authentication_error"), "invalid x-api-key").kind, ErrorKind::Auth);
    }

    #[test]
    fn only_transient_kinds_are_retryable() {
        use ErrorKind::*;
        for k in [Offline, Timeout, StreamBroke, RateLimited, Overloaded, Server] {
            assert!(k.retryable(), "{k:?}");
        }
        for k in [Auth, Billing, ContextTooLong, ModelNotFound, BadRequest, Cancelled, Local] {
            assert!(!k.retryable(), "{k:?}");
        }
        assert!(Offline.is_network() && Timeout.is_network() && StreamBroke.is_network());
        assert!(!Server.is_network() && !RateLimited.is_network());
    }

    #[test]
    fn every_kind_has_a_status_line_a_next_step_and_a_code() {
        use ErrorKind::*;
        let all = [Offline, Timeout, StreamBroke, RateLimited, Overloaded, Server, Auth, Billing, ContextTooLong, ModelNotFound, BadRequest, Cancelled, Local];
        let mut codes = std::collections::HashSet::new();
        for k in all {
            assert!(!k.status_text().is_empty() && !k.advice().is_empty());
            assert!(codes.insert(k.code()), "codes are distinct");
        }
        assert!(Auth.advice().contains("Imber settings"));
        assert!(ContextTooLong.advice().to_lowercase().contains("compact"));
        assert!(Offline.advice().contains("offline"));
    }

    #[test]
    fn the_full_text_carries_the_provider_wording() {
        let e = ProviderError::from_response(401, None, &body("authentication_error", "invalid x-api-key"));
        assert!(e.full().contains("Imber settings") && e.full().contains("invalid x-api-key"), "{}", e.full());
        assert_eq!(ProviderError::cancelled().full(), "Stopped.");
    }
}
