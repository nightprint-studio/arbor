//! The loop: ask the model, run the tools it calls, tell it what they said, until it is done.
//!
//! Everything the loop touches that is not a decision is a trait the host plugs in — the transport
//! (an HTTP client), the tools and the question of whether one may run, and where the progress goes.
//! That is what makes the loop itself testable with a scripted stream and no key: a conversation that
//! calls two tools, a tool that is refused, a stream cut off mid-argument, a user who cancels.
//!
//! Three rules the loop keeps, because each is a way an agent goes wrong quietly:
//!
//!  * **A tool call that did not parse is never run.** The model is told its arguments were incomplete
//!    and decides what to do — running a tool on guessed arguments is how a half-sent path becomes an
//!    edit to the wrong file.
//!  * **Every tool call gets a result**, run or refused, so the next request is a valid one; a refusal
//!    is an answer the model can read ("the user declined"), not a hole in the conversation.
//!  * **It stops.** A model that keeps calling tools is bounded by `max_iterations`, and the user can
//!    cancel between any two chunks.

use serde_json::Value;

use crate::message::{Block, Message, Request, ToolSpec};
use crate::provider::{Event, Provider, StopReason, Usage};

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
/// connection and returns `Ok`. A non-success status is an `Err` carrying the provider's own message —
/// it is shown to the user as it is, because "invalid x-api-key" is the whole diagnosis.
pub trait Transport {
    fn stream(&self, request: &HttpRequest, on_chunk: &mut dyn FnMut(&str) -> bool) -> Result<(), String>;
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
    fn event(&self, event: AgentEvent);
    fn cancelled(&self) -> bool;
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub ended: Ended,
    /// Everything this run added to the conversation: the assistant's messages and the tool results.
    pub added: Vec<Message>,
    /// The whole conversation as the run left it. Differs from "what it was plus `added`" when
    /// [`Host::prepare`] shortened it, which is why a host saves this and not the sum.
    pub messages: Vec<Message>,
    /// What the run used, summed over its requests.
    pub usage: Usage,
    /// What each request used, in order. Cost is worked out from these, one by one: a long prompt is
    /// charged more, and that is a property of a request, not of a total.
    pub requests: Vec<Usage>,
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

/// What one request produced.
struct Turn {
    text: String,
    calls: Vec<(ToolCall, bool)>,
    stop: StopReason,
    usage: Usage,
}

fn read_turn(
    provider: &dyn Provider,
    transport: &dyn Transport,
    http: &HttpRequest,
    host: &dyn Host,
) -> Result<Turn, String> {
    let mut decoder = provider.decoder();
    let mut turn = Turn { text: String::new(), calls: Vec::new(), stop: StopReason::EndTurn, usage: Usage::default() };
    let mut failure: Option<String> = None;

    let mut absorb = |events: Vec<Event>, turn: &mut Turn, failure: &mut Option<String>| {
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
                Event::Stop(s) => turn.stop = s,
                Event::Error(e) => *failure = Some(e),
            }
        }
    };

    transport.stream(http, &mut |chunk| {
        if host.cancelled() {
            return false;
        }
        absorb(decoder.push(chunk), &mut turn, &mut failure);
        failure.is_none()
    })?;
    absorb(decoder.finish(), &mut turn, &mut failure);
    match failure {
        Some(e) => Err(e),
        None => Ok(turn),
    }
}

/// Run the conversation in `request` until the model is done.
///
/// `request.messages` ends with the user's message; the loop leaves it untouched and reports what it
/// added in [`Outcome::added`], so the host persists exactly the new part.
pub fn run(
    provider: &dyn Provider,
    transport: &dyn Transport,
    host: &dyn Host,
    key: &str,
    mut request: Request,
    max_iterations: u32,
) -> Result<Outcome, String> {
    let mut added: Vec<Message> = Vec::new();
    let mut usage = Usage::default();
    let mut requests: Vec<Usage> = Vec::new();

    for iteration in 1..=max_iterations {
        if host.cancelled() {
            return Ok(Outcome { ended: Ended::Cancelled, added, messages: request.messages, usage, requests });
        }
        request.tools = host.tools();
        host.prepare(&mut request);
        // The host may have decided, while preparing, that this request must not be made (a spending
        // limit): it says so by cancelling.
        if host.cancelled() {
            return Ok(Outcome { ended: Ended::Cancelled, added, messages: request.messages, usage, requests });
        }
        host.event(AgentEvent::Request { iteration });

        let endpoint = provider.endpoint();
        let mut headers = endpoint.headers;
        headers.push(provider.auth_header(key));
        let http = HttpRequest { url: endpoint.url, headers, body: provider.body(&request).to_string() };

        let turn = read_turn(provider, transport, &http, host)?;
        usage = add(usage, turn.usage);
        requests.push(turn.usage);

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
            added.push(message);
        }

        if host.cancelled() {
            return Ok(Outcome { ended: Ended::Cancelled, added, messages: request.messages, usage, requests });
        }
        match turn.stop {
            StopReason::ToolUse => {}
            StopReason::EndTurn => return Ok(Outcome { ended: Ended::Done, added, messages: request.messages, usage, requests }),
            other => return Ok(Outcome { ended: Ended::Incomplete(other), added, messages: request.messages, usage, requests }),
        }

        let mut results: Vec<Block> = Vec::new();
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
            results.push(match result {
                Ok(content) => Block::ToolResult { tool_use_id: call.id.clone(), content, is_error: false },
                Err(content) => Block::ToolResult { tool_use_id: call.id.clone(), content, is_error: true },
            });
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
        let message = Message::tool_results(results);
        request.messages.push(message.clone());
        added.push(message);
    }
    Ok(Outcome { ended: Ended::TooManySteps, added, messages: request.messages, usage, requests })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::Anthropic;
    use serde_json::json;
    use std::cell::{Cell, RefCell};

    /// Replies with the next scripted stream each time it is asked.
    struct Script {
        streams: RefCell<Vec<Result<String, String>>>,
        sent: RefCell<Vec<HttpRequest>>,
    }

    impl Script {
        fn new(streams: Vec<Result<String, String>>) -> Self {
            Self { streams: RefCell::new(streams), sent: RefCell::new(Vec::new()) }
        }
    }

    impl Transport for Script {
        fn stream(&self, request: &HttpRequest, on_chunk: &mut dyn FnMut(&str) -> bool) -> Result<(), String> {
            self.sent.borrow_mut().push(request.clone());
            let next = self.streams.borrow_mut().remove(0)?;
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
    }

    impl TestHost {
        fn new() -> Self {
            Self { events: RefCell::new(vec![]), ran: RefCell::new(vec![]), refuse: None, cancel_after_runs: None, cancelled: Cell::new(false) }
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
    }

    fn request() -> Request {
        Request {
            model: "claude-opus-5-5".into(),
            system: "s".into(),
            messages: vec![Message::user("hi")],
            tools: vec![],
            max_tokens: 1024,
            effort: None,
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
    }

    #[test]
    fn the_host_can_shorten_the_conversation_and_the_outcome_says_what_was_left() {
        let transport = Script::new(vec![Ok(text_stream("ok"))]);
        let host = Trimming(TestHost::new());
        let out = run(&Anthropic, &transport, &host, "k", request(), 8).unwrap();
        let sent = transport.sent.borrow()[0].body.clone();
        assert!(sent.contains("summary") && !sent.contains("\"hi\""), "{sent}");
        assert_eq!(out.messages.len(), 2, "the summary, then the answer");
        assert_eq!(out.added.len(), 1);
    }

    #[test]
    fn a_plain_answer_is_one_request() {
        let transport = Script::new(vec![Ok(text_stream("Hello"))]);
        let host = TestHost::new();
        let out = run(&Anthropic, &transport, &host, "k", request(), 8).unwrap();
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
        let out = run(&Anthropic, &transport, &host, "k", request(), 8).unwrap();
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
        run(&Anthropic, &transport, &TestHost::new(), "sk-ant-secret", request(), 8).unwrap();
        let sent = transport.sent.borrow();
        assert!(sent[0].headers.iter().any(|(k, v)| k == "x-api-key" && v == "sk-ant-secret"));
        assert!(!sent[0].body.contains("sk-ant-secret"));
    }

    #[test]
    fn a_refused_call_is_answered_not_skipped() {
        let transport = Script::new(vec![Ok(tool_stream("t1", "delete_file", r#"{"path":"A.java"}"#)), Ok(text_stream("Understood"))]);
        let mut host = TestHost::new();
        host.refuse = Some("the user declined");
        let out = run(&Anthropic, &transport, &host, "k", request(), 8).unwrap();
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
        let out = run(&Anthropic, &transport, &host, "k", request(), 8).unwrap();
        assert!(host.ran.borrow().is_empty());
        let Block::ToolResult { content, is_error, .. } = &out.added[1].blocks[0] else { panic!("a result") };
        assert!(*is_error && content.contains("not run"));
    }

    #[test]
    fn a_model_that_never_stops_calling_tools_is_stopped() {
        let transport = Script::new((0..3).map(|i| Ok(tool_stream(&format!("t{i}"), "read_file", "{}"))).collect());
        let out = run(&Anthropic, &transport, &TestHost::new(), "k", request(), 3).unwrap();
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
        let out = run(&Anthropic, &transport, &host, "k", request(), 8).unwrap();
        assert_eq!(out.ended, Ended::Cancelled);
        let Block::ToolResult { is_error, content, .. } = &out.added[1].blocks[1] else { panic!("both calls answered") };
        assert!(*is_error && content.contains("cancelled"));
    }

    #[test]
    fn an_error_in_the_stream_is_the_error_of_the_run() {
        let err = sse(&[json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}})]);
        let transport = Script::new(vec![Ok(err)]);
        let e = run(&Anthropic, &transport, &TestHost::new(), "k", request(), 8).unwrap_err();
        assert!(e.contains("Overloaded"), "{e}");
    }

    #[test]
    fn a_refused_http_request_is_reported_as_it_came() {
        let transport = Script::new(vec![Err("401: invalid x-api-key".into())]);
        let e = run(&Anthropic, &transport, &TestHost::new(), "k", request(), 8).unwrap_err();
        assert_eq!(e, "401: invalid x-api-key");
    }

    #[test]
    fn usage_is_summed_over_the_requests() {
        let transport = Script::new(vec![Ok(tool_stream("t1", "read_file", "{}")), Ok(text_stream("ok"))]);
        let out = run(&Anthropic, &transport, &TestHost::new(), "k", request(), 8).unwrap();
        assert_eq!(out.usage.output_tokens, 14);
    }
}
