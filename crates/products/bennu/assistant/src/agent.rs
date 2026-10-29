//! The loop: ask the model, run the tools it calls, tell it what they said, until it is done.
//!
//! Everything the loop touches that is not a decision is a trait the host plugs in — the transport
//! (an HTTP client), the tools and the question of whether one may run, and where the progress goes.
//! That is what makes the loop itself testable with a scripted stream and no key: a conversation that
//! calls two tools, a tool that is refused, a stream cut off mid-argument, a user who cancels, a network
//! that drops and comes back.
//!
//! Four rules the loop keeps, because each is a way an agent goes wrong quietly:
//!
//!  * **A tool call that did not parse is never run.** The model is told its arguments were incomplete
//!    and decides what to do — running a tool on guessed arguments is how a half-sent path becomes an
//!    edit to the wrong file.
//!  * **Every tool call gets a result**, run or refused, so the next request is a valid one; a refusal
//!    is an answer the model can read ("the user declined"), not a hole in the conversation.
//!  * **It stops.** A model that keeps calling tools is bounded by `max_iterations`, and the user can
//!    cancel between any two chunks.
//!  * **A request that failed for a passing reason is sent again** (see [`crate::retry`]). That is safe
//!    because a turn's tools only run once the whole turn has arrived: a stream that broke has changed
//!    nothing, so the attempt is simply thrown away — its partial text is withdrawn from the thread —
//!    and made again. What the failed attempt was billed for still counts, once.

use std::time::Duration;

use serde_json::Value;

use crate::error::{ErrorKind, ProviderError};
use crate::message::{Block, ImageRef, Message, Request, ToolSpec};
use crate::provider::{Event, Provider, StopReason, Usage};
use crate::retry::{entropy_jitter, sleep_interruptible, Backoff, Pause, RetryPolicy};

/// A request ready to send: everything but the credential's value is already in `headers`, and the
/// credential header is appended by the host's transport from the key it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Sends a request and hands back the response as it arrives.
///
/// `on_chunk` returns `false` to stop reading (the user cancelled); the transport then drops the
/// connection and returns `Ok`. Anything that goes wrong is a [`ProviderError`] — the transport knows the
/// status, the headers and the class of network failure, and says which it was; the provider's own words
/// stay in its `detail`, because "invalid x-api-key" is the whole diagnosis.
pub trait Transport {
    fn stream(&self, request: &HttpRequest, on_chunk: &mut dyn FnMut(&str) -> bool) -> Result<(), ProviderError>;
}

/// A tool call the model made, with arguments that parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// Whether a call may go ahead. The host decides — including asking the user, which blocks here.
#[derive(Debug, Clone, PartialEq)]
pub enum Gate {
    Run,
    /// Not run; the reason is the tool result the model reads.
    Refuse(String),
}

/// What the loop reports as it goes, in the order it happens.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// A request is about to go out (1 for the first; more after tool results).
    Request { iteration: u32 },
    Text(String),
    Thinking(String),
    ToolStart { id: String, name: String },
    /// The call finished: `ok` is false for a refusal or an error; `summary` is a line for the thread.
    ToolDone { id: String, name: String, input: Value, ok: bool, summary: String },
    Usage(Usage),
    /// The request failed for a reason that can pass; it is sent again after `wait_secs`. `attempt` of about
    /// `max`. What the failed attempt billed has already been reported as [`AgentEvent::Usage`].
    Retry { attempt: u32, max: u32, wait_secs: u64, kind: ErrorKind, detail: String },
    /// What the failed attempt streamed is void: the thread takes back the text of this request.
    RetryDiscard,
}

/// What the host provides to the loop.
pub trait Host {
    /// The tools offered to the model for the next request.
    fn tools(&self) -> Vec<ToolSpec>;
    /// A last look at the request before it goes out — the place to make a conversation that has grown
    /// too large smaller. Called before every request of the run, the first included.
    fn prepare(&self, _request: &mut Request) {}
    /// May this call run? Blocks while the user is asked.
    fn gate(&self, call: &ToolCall) -> Gate;
    /// Run it. `Err` becomes an error result the model reads.
    fn run(&self, call: &ToolCall) -> Result<String, String>;
    /// One line for the thread describing a finished call.
    fn summarize(&self, call: &ToolCall, result: &Result<String, String>) -> String {
        match result {
            Ok(_) => call.name.clone(),
            Err(e) => format!("{}: {e}", call.name),
        }
    }
    /// The pictures the call that just ran produced, handed over once: the loop puts them beside its result.
    /// Asked after [`AgentEvent::ToolDone`], so the thread shows the receipt before the picture.
    fn take_images(&self, _call: &ToolCall) -> Vec<ImageRef> {
        Vec::new()
    }
    fn event(&self, event: AgentEvent);
    fn cancelled(&self) -> bool;

    /// How a failed request is retried. A run gets the full schedule; a helper job nobody watches, less.
    fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy::standard()
    }
    /// Wait before a retry. Ends early when the run is cancelled, or when the user asks to try now.
    fn pause(&self, wait: Duration) -> Pause {
        sleep_interruptible(wait, &|| self.cancelled(), &|| false)
    }
    /// A number in `[0, 1)` that spreads the waits of clients that failed together.
    fn jitter(&self) -> f64 {
        entropy_jitter()
    }
    /// Called after the wait, before the request is sent again: the place to hold a retry to the same limits
    /// as a first attempt (spending). The host stops the run by cancelling.
    fn before_retry(&self) {}
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// The model finished its answer.
    Done,
    /// The user cancelled.
    Cancelled,
    /// `max_iterations` requests were made and the model still wanted tools.
    TooManySteps,
    /// It reached the length it was allowed, or declined — the answer is not a complete one.
    Incomplete(StopReason),
    /// A request failed and could not be retried (or the retries ran out): [`Outcome::error`] says how.
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub ended: Ended,
    /// Everything this run added to the conversation: the assistant's messages and the tool results.
    pub added: Vec<Message>,
    /// The whole conversation as the run left it. Differs from "what it was plus `added`" when
    /// [`Host::prepare`] shortened it, which is why a host saves this and not the sum. After a failure it
    /// is every turn that did complete, so the same request can be sent again from it.
    pub messages: Vec<Message>,
    /// What the run used, summed over its requests (failed attempts that were billed included).
    pub usage: Usage,
    /// What each request used, in order. Cost is worked out from these, one by one: a long prompt is
    /// charged more, and that is a property of a request, not of a total.
    pub requests: Vec<Usage>,
    /// Why the run failed, when [`Outcome::ended`] is [`Ended::Failed`].
    pub error: Option<ProviderError>,
}

impl Outcome {
    /// A run that could not even start (the network stack failed to come up): nothing was added.
    pub fn failed_to_start(messages: Vec<Message>, error: ProviderError) -> Self {
        Self { ended: Ended::Failed, added: Vec::new(), messages, usage: Usage::default(), requests: Vec::new(), error: Some(error) }
    }
}

fn add(a: Usage, b: Usage) -> Usage {
    Usage {
        input_tokens: a.input_tokens + b.input_tokens,
        output_tokens: a.output_tokens + b.output_tokens,
        cache_read_tokens: a.cache_read_tokens + b.cache_read_tokens,
        cache_write_tokens: a.cache_write_tokens + b.cache_write_tokens,
        cache_write_1h_tokens: a.cache_write_1h_tokens + b.cache_write_1h_tokens,
    }
}

/// What the run has produced and used so far; what is left when it ends, however it ends.
#[derive(Default)]
struct Tally {
    added: Vec<Message>,
    usage: Usage,
    requests: Vec<Usage>,
}

impl Tally {
    fn spend(&mut self, used: Usage) {
        self.usage = add(self.usage, used);
        self.requests.push(used);
    }

    fn end(self, ended: Ended, messages: Vec<Message>) -> Outcome {
        Outcome { ended, added: self.added, messages, usage: self.usage, requests: self.requests, error: None }
    }

    fn fail(self, messages: Vec<Message>, error: ProviderError) -> Outcome {
        Outcome { ended: Ended::Failed, added: self.added, messages, usage: self.usage, requests: self.requests, error: Some(error) }
    }
}

/// What one request produced.
struct Turn {
    text: String,
    calls: Vec<(ToolCall, bool)>,
    stop: StopReason,
    usage: Usage,
    /// An `error` event came in the stream.
    failure: Option<ProviderError>,
    /// The provider said how the answer ended (a stream that just stops did not finish).
    stopped: bool,
}

/// A request that failed, and what it had used before it did.
struct Failed {
    error: ProviderError,
    usage: Usage,
}

fn absorb(events: Vec<Event>, turn: &mut Turn, host: &dyn Host) {
    for event in events {
        match event {
            Event::Text(t) => {
                turn.text.push_str(&t);
                host.event(AgentEvent::Text(t));
            }
            Event::Thinking(t) => host.event(AgentEvent::Thinking(t)),
            Event::ToolStart { id, name } => host.event(AgentEvent::ToolStart { id, name }),
            Event::ToolDone { id, name, input, valid } => {
                turn.calls.push((ToolCall { id, name, input }, valid));
            }
            Event::Usage(u) => {
                turn.usage = turn.usage.merge(u);
                host.event(AgentEvent::Usage(u));
            }
            Event::Stop(s) => {
                turn.stop = s;
                turn.stopped = true;
            }
            Event::Error { error_type, message } => turn.failure = Some(ProviderError::from_stream_event(error_type.as_deref(), &message)),
        }
    }
}

/// One attempt at one request.
fn read_turn(provider: &dyn Provider, transport: &dyn Transport, http: &HttpRequest, host: &dyn Host) -> Result<Turn, Failed> {
    let mut decoder = provider.decoder();
    let mut turn = Turn { text: String::new(), calls: Vec::new(), stop: StopReason::EndTurn, usage: Usage::default(), failure: None, stopped: false };

    let sent = transport.stream(http, &mut |chunk| {
        if host.cancelled() {
            return false;
        }
        absorb(decoder.push(chunk), &mut turn, host);
        turn.failure.is_none()
    });
    let failed = |turn: &Turn, mut error: ProviderError| {
        error.partial = !turn.text.is_empty();
        Failed { error, usage: turn.usage }
    };
    if let Err(error) = sent {
        return Err(failed(&turn, error));
    }
    absorb(decoder.finish(), &mut turn, host);
    if let Some(error) = turn.failure.take() {
        return Err(failed(&turn, error));
    }
    // A connection that closes without the provider saying the answer is over was cut, whatever it delivered.
    if !turn.stopped && !host.cancelled() {
        return Err(failed(&turn, ProviderError::stream_broke("the response ended before the answer was complete")));
    }
    Ok(turn)
}

/// Why a request will not be answered.
enum Stop {
    Cancelled,
    Failed(ProviderError),
}

/// [`read_turn`], again after a wait whenever the failure is one that can pass.
fn read_turn_retrying(provider: &dyn Provider, transport: &dyn Transport, http: &HttpRequest, host: &dyn Host, tally: &mut Tally) -> Result<Turn, Stop> {
    let mut backoff = Backoff::new(host.retry_policy());
    loop {
        let Failed { error, usage } = match read_turn(provider, transport, http, host) {
            Ok(turn) => return Ok(turn),
            Err(failed) => failed,
        };
        // Billed even though the answer was lost: counted here, once, as a request of its own.
        if usage != Usage::default() {
            tally.spend(usage);
        }
        if error.partial {
            host.event(AgentEvent::RetryDiscard);
        }
        if host.cancelled() {
            return Err(Stop::Cancelled);
        }
        let Some(plan) = backoff.next(&error, host.jitter()) else {
            return Err(Stop::Failed(error));
        };
        host.event(AgentEvent::Retry {
            attempt: plan.attempt,
            max: plan.max,
            wait_secs: plan.wait.as_secs_f64().ceil() as u64,
            kind: error.kind,
            detail: error.detail.clone(),
        });
        if host.pause(plan.wait) == Pause::Cancelled {
            return Err(Stop::Cancelled);
        }
        host.before_retry();
        if host.cancelled() {
            return Err(Stop::Cancelled);
        }
    }
}

/// Run the conversation in `request` until the model is done.
///
/// `request.messages` ends with the user's message; the loop leaves it untouched and reports what it
/// added in [`Outcome::added`], so the host persists exactly the new part.
pub fn run(provider: &dyn Provider, transport: &dyn Transport, host: &dyn Host, key: &str, mut request: Request, max_iterations: u32) -> Outcome {
    let mut tally = Tally::default();

    for iteration in 1..=max_iterations {
        if host.cancelled() {
            return tally.end(Ended::Cancelled, request.messages);
        }
        request.tools = host.tools();
        host.prepare(&mut request);
        // The host may have decided, while preparing, that this request must not be made (a spending
        // limit): it says so by cancelling.
        if host.cancelled() {
            return tally.end(Ended::Cancelled, request.messages);
        }
        host.event(AgentEvent::Request { iteration });

        let endpoint = provider.endpoint();
        let mut headers = endpoint.headers;
        headers.push(provider.auth_header(key));
        let http = HttpRequest { url: endpoint.url, headers, body: provider.body(&request).to_string() };

        let turn = match read_turn_retrying(provider, transport, &http, host, &mut tally) {
            Ok(turn) => turn,
            Err(Stop::Cancelled) => return tally.end(Ended::Cancelled, request.messages),
            Err(Stop::Failed(error)) => return tally.fail(request.messages, error),
        };
        tally.spend(turn.usage);

        let mut blocks: Vec<Block> = Vec::new();
        if !turn.text.is_empty() {
            blocks.push(Block::Text { text: turn.text.clone() });
        }
        for (call, _) in &turn.calls {
            blocks.push(Block::ToolUse { id: call.id.clone(), name: call.name.clone(), input: call.input.clone() });
        }
        if !blocks.is_empty() {
            let message = Message::assistant(blocks);
            request.messages.push(message.clone());
            tally.added.push(message);
        }

        if host.cancelled() {
            return tally.end(Ended::Cancelled, request.messages);
        }
        match turn.stop {
            StopReason::ToolUse => {}
            StopReason::EndTurn => return tally.end(Ended::Done, request.messages),
            other => return tally.end(Ended::Incomplete(other), request.messages),
        }

        let mut results: Vec<Block> = Vec::new();
        // Placed after every result: a provider wants the answers to a turn's calls first.
        let mut pictures: Vec<Block> = Vec::new();
        for (call, valid) in &turn.calls {
            let result = if !valid {
                Err("the arguments of this call were incomplete or not valid JSON, so it was not run — send it again".to_string())
            } else {
                match host.gate(call) {
                    Gate::Refuse(reason) => Err(reason),
                    Gate::Run => host.run(call),
                }
            };
            let summary = host.summarize(call, &result);
            host.event(AgentEvent::ToolDone {
                id: call.id.clone(),
                name: call.name.clone(),
                input: call.input.clone(),
                ok: result.is_ok(),
                summary,
            });
            let images = if result.is_ok() { host.take_images(call) } else { Vec::new() };
            results.push(match result {
                Ok(content) => Block::ToolResult { tool_use_id: call.id.clone(), content, is_error: false },
                Err(content) => Block::ToolResult { tool_use_id: call.id.clone(), content, is_error: true },
            });
            pictures.extend(images.into_iter().map(|i| Block::ToolImage {
                tool_use_id: call.id.clone(),
                media_type: i.media_type,
                path: i.path,
                tokens: i.tokens,
                data: None,
            }));
            if host.cancelled() {
                break;
            }
        }
        // A call the loop stopped before reaching still needs a result, or the next request is invalid.
        for (call, _) in &turn.calls {
            let answered = results.iter().any(|b| matches!(b, Block::ToolResult { tool_use_id, .. } if *tool_use_id == call.id));
            if !answered {
                results.push(Block::ToolResult { tool_use_id: call.id.clone(), content: "cancelled before it ran".into(), is_error: true });
            }
        }
        results.extend(pictures);
        let message = Message::tool_results(results);
        request.messages.push(message.clone());
        tally.added.push(message);
    }
    tally.end(Ended::TooManySteps, request.messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::Anthropic;
    use crate::error::ErrorKind;
    use serde_json::json;
    use std::cell::{Cell, RefCell};

    /// One scripted reply.
    enum Reply {
        /// A whole stream, delivered in two pieces.
        Stream(String),
        /// The request fails before anything arrives.
        Fail(ProviderError),
        /// Some of a stream arrives, then the connection breaks.
        Cut(String, ProviderError),
    }

    impl From<Result<String, ProviderError>> for Reply {
        fn from(r: Result<String, ProviderError>) -> Self {
            match r {
                Ok(s) => Reply::Stream(s),
                Err(e) => Reply::Fail(e),
            }
        }
    }

    /// Replies with the next scripted reply each time it is asked.
    struct Script {
        streams: RefCell<Vec<Reply>>,
        sent: RefCell<Vec<HttpRequest>>,
    }

    impl Script {
        fn new(streams: Vec<Result<String, ProviderError>>) -> Self {
            Self::replies(streams.into_iter().map(Reply::from).collect())
        }

        fn replies(streams: Vec<Reply>) -> Self {
            Self { streams: RefCell::new(streams), sent: RefCell::new(Vec::new()) }
        }
    }

    impl Transport for Script {
        fn stream(&self, request: &HttpRequest, on_chunk: &mut dyn FnMut(&str) -> bool) -> Result<(), ProviderError> {
            self.sent.borrow_mut().push(request.clone());
            let next = match self.streams.borrow_mut().remove(0) {
                Reply::Stream(s) => s,
                Reply::Fail(e) => return Err(e),
                Reply::Cut(s, e) => {
                    on_chunk(&s);
                    return Err(e);
                }
            };
            // Two pieces, cut mid-line: the loop must not care.
            let mid = next.len() / 2;
            let mid = (0..=mid).rev().find(|i| next.is_char_boundary(*i)).unwrap_or(0);
            if !on_chunk(&next[..mid]) {
                return Ok(());
            }
            on_chunk(&next[mid..]);
            Ok(())
        }
    }

    fn sse(events: &[Value]) -> String {
        events.iter().map(|e| format!("data: {e}\n\n")).collect()
    }

    fn text_stream(text: &str) -> String {
        sse(&[
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}),
            json!({"type":"message_stop"}),
        ])
    }

    fn tool_stream(id: &str, name: &str, args: &str) -> String {
        sse(&[
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":id,"name":name,"input":{}}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":args}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":9}}),
            json!({"type":"message_stop"}),
        ])
    }

    struct TestHost {
        events: RefCell<Vec<AgentEvent>>,
        ran: RefCell<Vec<String>>,
        refuse: Option<&'static str>,
        cancel_after_runs: Option<usize>,
        cancelled: Cell<bool>,
        /// The waits the loop asked for, in order (nothing really sleeps).
        pauses: RefCell<Vec<Duration>>,
        /// The user stops the run during this wait (1 = the first).
        cancel_in_pause: Option<usize>,
    }

    impl TestHost {
        fn new() -> Self {
            Self {
                events: RefCell::new(vec![]),
                ran: RefCell::new(vec![]),
                refuse: None,
                cancel_after_runs: None,
                cancelled: Cell::new(false),
                pauses: RefCell::new(vec![]),
                cancel_in_pause: None,
            }
        }

        fn retries(&self) -> Vec<(u32, ErrorKind)> {
            self.events.borrow().iter().filter_map(|e| if let AgentEvent::Retry { attempt, kind, .. } = e { Some((*attempt, *kind)) } else { None }).collect()
        }
    }

    impl Host for TestHost {
        fn tools(&self) -> Vec<ToolSpec> {
            vec![ToolSpec { name: "read_file".into(), description: "Read".into(), input_schema: json!({"type":"object"}) }]
        }
        fn gate(&self, call: &ToolCall) -> Gate {
            match self.refuse {
                Some(r) if call.name == "delete_file" => Gate::Refuse(r.into()),
                _ => Gate::Run,
            }
        }
        fn run(&self, call: &ToolCall) -> Result<String, String> {
            self.ran.borrow_mut().push(call.name.clone());
            if let Some(n) = self.cancel_after_runs {
                if self.ran.borrow().len() >= n {
                    self.cancelled.set(true);
                }
            }
            Ok(format!("ran {}", call.name))
        }
        fn event(&self, event: AgentEvent) {
            self.events.borrow_mut().push(event);
        }
        fn cancelled(&self) -> bool {
            self.cancelled.get()
        }
        fn pause(&self, wait: Duration) -> Pause {
            self.pauses.borrow_mut().push(wait);
            if self.cancel_in_pause == Some(self.pauses.borrow().len()) {
                self.cancelled.set(true);
                return Pause::Cancelled;
            }
            Pause::Elapsed
        }
        fn jitter(&self) -> f64 {
            1.0
        }
    }

    fn request() -> Request {
        Request {
            model: "claude-opus-5-5".into(),
            system: "s".into(),
            messages: vec![Message::user("hi")],
            tools: vec![],
            max_tokens: 1024,
            effort: None,
            long_cache: false,
        }
    }

    /// A host that shortens the conversation before every request.
    struct Trimming(TestHost);
    impl Host for Trimming {
        fn tools(&self) -> Vec<ToolSpec> { self.0.tools() }
        fn prepare(&self, request: &mut Request) {
            request.messages = vec![Message::user("summary")];
        }
        fn gate(&self, call: &ToolCall) -> Gate { self.0.gate(call) }
        fn run(&self, call: &ToolCall) -> Result<String, String> { self.0.run(call) }
        fn event(&self, event: AgentEvent) { self.0.event(event) }
        fn cancelled(&self) -> bool { self.0.cancelled() }
        fn pause(&self, wait: Duration) -> Pause { self.0.pause(wait) }
        fn jitter(&self) -> f64 { self.0.jitter() }
    }

    /// A host whose tool leaves a picture behind.
    struct Pictures(TestHost);
    impl Host for Pictures {
        fn tools(&self) -> Vec<ToolSpec> { self.0.tools() }
        fn gate(&self, call: &ToolCall) -> Gate { self.0.gate(call) }
        fn run(&self, call: &ToolCall) -> Result<String, String> { self.0.run(call) }
        fn take_images(&self, _call: &ToolCall) -> Vec<ImageRef> {
            vec![ImageRef { media_type: "image/png".into(), path: "s.png".into(), tokens: 900 }]
        }
        fn event(&self, event: AgentEvent) { self.0.event(event) }
        fn cancelled(&self) -> bool { self.0.cancelled() }
        fn pause(&self, wait: Duration) -> Pause { self.0.pause(wait) }
        fn jitter(&self) -> f64 { self.0.jitter() }
    }

    #[test]
    fn a_picture_a_tool_left_follows_the_results_of_its_turn() {
        let transport = Script::new(vec![Ok(tool_stream("t1", "read_file", "{}")), Ok(text_stream("Seen"))]);
        let out = run(&Anthropic, &transport, &Pictures(TestHost::new()), "k", request(), 8);
        let blocks = &out.added[1].blocks;
        assert!(matches!(&blocks[0], Block::ToolResult { tool_use_id, .. } if tool_use_id == "t1"));
        assert!(matches!(&blocks[1], Block::ToolImage { tool_use_id, tokens: 900, data: None, .. } if tool_use_id == "t1"));
    }

    #[test]
    fn the_host_can_shorten_the_conversation_and_the_outcome_says_what_was_left() {
        let transport = Script::new(vec![Ok(text_stream("ok"))]);
        let host = Trimming(TestHost::new());
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        let sent = transport.sent.borrow()[0].body.clone();
        assert!(sent.contains("summary") && !sent.contains("\"hi\""), "{sent}");
        assert_eq!(out.messages.len(), 2, "the summary, then the answer");
        assert_eq!(out.added.len(), 1);
    }

    #[test]
    fn a_plain_answer_is_one_request() {
        let transport = Script::new(vec![Ok(text_stream("Hello"))]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Done);
        assert_eq!(out.added.len(), 1);
        assert_eq!(out.added[0].text(), "Hello");
        assert_eq!(transport.sent.borrow().len(), 1);
        assert!(host.events.borrow().contains(&AgentEvent::Text("Hello".into())));
    }

    #[test]
    fn a_tool_call_is_run_and_its_result_goes_back() {
        let transport = Script::new(vec![Ok(tool_stream("t1", "read_file", r#"{"path":"A.java"}"#)), Ok(text_stream("Done"))]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Done);
        assert_eq!(*host.ran.borrow(), vec!["read_file".to_string()]);
        // assistant(tool_use) · user(tool_result) · assistant(text)
        assert_eq!(out.added.len(), 3);
        let second = transport.sent.borrow()[1].body.clone();
        assert!(second.contains("tool_result") && second.contains("ran read_file"), "{second}");
        assert!(second.contains("tool_use"), "the call it answers is in the conversation too");
    }

    #[test]
    fn the_key_goes_out_in_its_header_and_in_nothing_else() {
        let transport = Script::new(vec![Ok(text_stream("x"))]);
        run(&Anthropic, &transport, &TestHost::new(), "sk-ant-secret", request(), 8);
        let sent = transport.sent.borrow();
        assert!(sent[0].headers.iter().any(|(k, v)| k == "x-api-key" && v == "sk-ant-secret"));
        assert!(!sent[0].body.contains("sk-ant-secret"));
    }

    #[test]
    fn a_refused_call_is_answered_not_skipped() {
        let transport = Script::new(vec![Ok(tool_stream("t1", "delete_file", r#"{"path":"A.java"}"#)), Ok(text_stream("Understood"))]);
        let mut host = TestHost::new();
        host.refuse = Some("the user declined");
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert!(host.ran.borrow().is_empty(), "a refused call never runs");
        let Block::ToolResult { content, is_error, .. } = &out.added[1].blocks[0] else { panic!("a result") };
        assert!(*is_error);
        assert_eq!(content, "the user declined");
        assert_eq!(out.ended, Ended::Done);
    }

    #[test]
    fn a_call_with_broken_arguments_is_not_run() {
        let transport = Script::new(vec![Ok(tool_stream("t1", "read_file", r#"{"path": "A.ja"#)), Ok(text_stream("Sorry"))]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert!(host.ran.borrow().is_empty());
        let Block::ToolResult { content, is_error, .. } = &out.added[1].blocks[0] else { panic!("a result") };
        assert!(*is_error && content.contains("not run"));
    }

    #[test]
    fn a_model_that_never_stops_calling_tools_is_stopped() {
        let transport = Script::new((0..3).map(|i| Ok(tool_stream(&format!("t{i}"), "read_file", "{}"))).collect());
        let out = run(&Anthropic, &transport, &TestHost::new(), "k", request(), 3);
        assert_eq!(out.ended, Ended::TooManySteps);
    }

    #[test]
    fn cancelling_leaves_a_valid_conversation() {
        let two_calls = sse(&[
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"a","name":"read_file","input":{}}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"b","name":"read_file","input":{}}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":9}}),
            json!({"type":"message_stop"}),
        ]);
        let transport = Script::new(vec![Ok(two_calls)]);
        let mut host = TestHost::new();
        host.cancel_after_runs = Some(1);
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Cancelled);
        let Block::ToolResult { is_error, content, .. } = &out.added[1].blocks[1] else { panic!("both calls answered") };
        assert!(*is_error && content.contains("cancelled"));
    }

    fn offline() -> ProviderError {
        ProviderError::offline("could not reach https://api.anthropic.com: dns error")
    }

    fn overloaded_event() -> String {
        sse(&[json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}})])
    }

    /// The start of an answer and what the request has already been billed for, with no ending.
    fn cut_text_stream(text: &str) -> String {
        sse(&[
            json!({"type":"message_start","message":{"id":"m","usage":{"input_tokens":100,"output_tokens":1}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}),
        ])
    }

    /// A host that holds a retry to a limit: it cancels in `before_retry`.
    struct Capped(TestHost);
    impl Host for Capped {
        fn tools(&self) -> Vec<ToolSpec> { self.0.tools() }
        fn gate(&self, call: &ToolCall) -> Gate { self.0.gate(call) }
        fn run(&self, call: &ToolCall) -> Result<String, String> { self.0.run(call) }
        fn event(&self, event: AgentEvent) { self.0.event(event) }
        fn cancelled(&self) -> bool { self.0.cancelled() }
        fn pause(&self, wait: Duration) -> Pause { self.0.pause(wait) }
        fn jitter(&self) -> f64 { self.0.jitter() }
        fn before_retry(&self) { self.0.cancelled.set(true); }
    }

    #[test]
    fn an_error_event_that_can_pass_is_asked_again() {
        let transport = Script::new(vec![Ok(overloaded_event()), Ok(text_stream("Back"))]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Done);
        assert_eq!(out.added[0].text(), "Back");
        assert_eq!(host.retries(), vec![(1, ErrorKind::Overloaded)]);
        assert_eq!(transport.sent.borrow().len(), 2);
        assert_eq!(transport.sent.borrow()[0].body, transport.sent.borrow()[1].body, "the same request goes again");
    }

    #[test]
    fn a_refused_key_ends_the_run_at_once_with_the_providers_words() {
        let err = ProviderError::from_response(401, None, r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#);
        let transport = Script::new(vec![Err(err)]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Failed);
        let error = out.error.expect("the failure");
        assert_eq!(error.kind, ErrorKind::Auth);
        assert!(error.detail.contains("invalid x-api-key"), "{}", error.detail);
        assert!(error.friendly().contains("Imber settings"));
        assert!(host.pauses.borrow().is_empty(), "nothing to wait for");
        assert_eq!(transport.sent.borrow().len(), 1);
    }

    #[test]
    fn kinds_that_cannot_pass_are_never_retried() {
        for status in [400u16, 402, 404, 413] {
            let transport = Script::new(vec![Err(ProviderError::from_response(status, None, "nope")), Ok(text_stream("x"))]);
            let host = TestHost::new();
            let out = run(&Anthropic, &transport, &host, "k", request(), 8);
            assert_eq!(out.ended, Ended::Failed, "{status}");
            assert_eq!(transport.sent.borrow().len(), 1, "{status}");
            assert!(host.retries().is_empty());
        }
    }

    #[test]
    fn a_network_that_comes_back_lets_the_answer_through() {
        let transport = Script::new(vec![Err(offline()), Err(offline()), Err(offline()), Ok(text_stream("Hello"))]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Done);
        assert_eq!(out.added[0].text(), "Hello");
        // Doubling, with the jitter the test host pins to its top.
        assert_eq!(*host.pauses.borrow(), vec![Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(4)]);
        let attempts: Vec<u32> = host.retries().iter().map(|r| r.0).collect();
        assert_eq!(attempts, vec![1, 2, 3]);
        assert!(host.events.borrow().iter().any(|e| matches!(e, AgentEvent::Retry { kind: ErrorKind::Offline, wait_secs: 4, .. })));
    }

    #[test]
    fn a_network_that_stays_down_gives_up_within_its_budget() {
        let transport = Script::new((0..60).map(|_| Err(offline())).collect());
        let host = TestHost::new();
        let before = request().messages;
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Failed);
        assert_eq!(out.error.as_ref().map(|e| e.kind), Some(ErrorKind::Offline));
        let waited: Duration = host.pauses.borrow().iter().sum();
        assert!(waited <= crate::retry::NETWORK_BUDGET, "{waited:?}");
        assert!(host.pauses.borrow().len() >= 8);
        assert_eq!(out.messages, before, "the conversation is as it was, ready to be sent again");
        assert!(out.added.is_empty());
    }

    #[test]
    fn a_provider_that_keeps_saying_not_now_gets_a_handful_of_attempts() {
        let transport = Script::new((0..10).map(|_| Ok(overloaded_event())).collect());
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Failed);
        assert_eq!(out.error.map(|e| e.kind), Some(ErrorKind::Overloaded));
        assert_eq!(host.pauses.borrow().len(), crate::retry::SERVER_ATTEMPTS as usize);
        assert_eq!(transport.sent.borrow().len(), crate::retry::SERVER_ATTEMPTS as usize + 1);
    }

    #[test]
    fn the_wait_the_provider_asked_for_is_honoured() {
        let limited = ProviderError::from_response(429, Some("12"), r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#);
        let transport = Script::new(vec![Err(limited), Ok(text_stream("ok"))]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Done);
        assert!(host.pauses.borrow()[0] >= Duration::from_secs(12), "{:?}", host.pauses.borrow());
    }

    #[test]
    fn a_stream_that_broke_mid_answer_is_thrown_away_and_made_again() {
        let transport = Script::replies(vec![
            Reply::Cut(cut_text_stream("Half an ans"), ProviderError::stream_broke("the connection broke")),
            Reply::Stream(text_stream("The whole answer")),
        ]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Done);
        let events = host.events.borrow();
        let discard = events.iter().position(|e| *e == AgentEvent::RetryDiscard).expect("the partial text is withdrawn");
        let retry = events.iter().position(|e| matches!(e, AgentEvent::Retry { .. })).expect("then the retry is announced");
        let partial = events.iter().position(|e| *e == AgentEvent::Text("Half an ans".into())).unwrap();
        assert!(partial < discard && discard < retry);
        assert_eq!(out.added.len(), 1, "the half answer never reaches the conversation");
        assert_eq!(out.added[0].text(), "The whole answer");
    }

    #[test]
    fn what_a_failed_attempt_was_billed_is_counted_once() {
        let transport = Script::replies(vec![
            Reply::Cut(cut_text_stream("Half"), ProviderError::stream_broke("the connection broke")),
            Reply::Stream(text_stream("Whole")),
        ]);
        let out = run(&Anthropic, &transport, &TestHost::new(), "k", request(), 8);
        assert_eq!(out.requests.len(), 2, "the lost attempt is a request of its own");
        assert_eq!(out.requests[0].input_tokens, 100);
        assert_eq!(out.usage.input_tokens, 100, "not billed twice");
        assert_eq!(out.usage.output_tokens, 1 + 5);
    }

    #[test]
    fn a_stream_that_just_stops_is_not_a_finished_answer() {
        let cut = sse(&[
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Half"}}),
        ]);
        let transport = Script::new(vec![Ok(cut), Ok(text_stream("Whole"))]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.added[0].text(), "Whole");
        assert_eq!(host.retries(), vec![(1, ErrorKind::StreamBroke)]);
    }

    #[test]
    fn stopping_during_the_wait_ends_the_run_at_once() {
        let transport = Script::new(vec![Err(offline()), Ok(text_stream("never asked"))]);
        let mut host = TestHost::new();
        host.cancel_in_pause = Some(1);
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Cancelled);
        assert!(out.error.is_none());
        assert_eq!(transport.sent.borrow().len(), 1, "no second request after the stop");
    }

    #[test]
    fn a_retry_is_held_to_the_same_limits_as_a_first_request() {
        let transport = Script::new(vec![Err(offline()), Ok(text_stream("never asked"))]);
        let out = run(&Anthropic, &transport, &Capped(TestHost::new()), "k", request(), 8);
        assert_eq!(out.ended, Ended::Cancelled);
        assert_eq!(transport.sent.borrow().len(), 1);
    }

    #[test]
    fn turns_that_completed_survive_a_later_failure() {
        let transport = Script::new(vec![Ok(tool_stream("t1", "read_file", "{}")), Err(ProviderError::from_response(401, None, "bad key"))]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8);
        assert_eq!(out.ended, Ended::Failed);
        assert_eq!(*host.ran.borrow(), vec!["read_file".to_string()]);
        assert_eq!(out.added.len(), 2, "the call and its result are kept");
        assert_eq!(out.messages.len(), 3);
        assert!(matches!(&out.messages.last().unwrap().blocks[0], Block::ToolResult { .. }), "sendable again from here");
    }

    #[test]
    fn usage_is_summed_over_the_requests() {
        let transport = Script::new(vec![Ok(tool_stream("t1", "read_file", "{}")), Ok(text_stream("ok"))]);
        let out = run(&Anthropic, &transport, &TestHost::new(), "k", request(), 8);
        assert_eq!(out.usage.output_tokens, 14);
    }
}
