//! Anthropic's Messages API, as a [`Provider`].
//!
//! **Out**: one JSON body per request, streamed (`stream: true`). The standing instructions and the
//! tool list are marked cacheable — they are the same on every request of a conversation, and the cache
//! turns re-reading them from full price to a fraction. The provider's prefix order is tools, then
//! instructions, then messages, and a change anywhere invalidates everything after it — so what is stable
//! goes first, and the conversation, which only ever grows at its end, carries a third mark on its last
//! block: each request then pays full price only for what the previous one did not already send.
//!
//! **Back**: a server-sent-event stream, decoded by [`AnthropicDecoder`]. Text and reasoning arrive as
//! deltas; a tool call arrives as a start (its id and name) followed by pieces of its JSON arguments,
//! which are assembled and parsed when the block ends — and flagged invalid, not guessed at, when they do
//! not parse.
//!
//! Which models take an effort setting, and whether to ask for adaptive thinking, are facts about the
//! model's name and live in [`efforts_for`] and [`wants_adaptive_thinking`], next to the tests that
//! pin them.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::effort::Effort;
use crate::message::{Block, Message, Request, Role};
use crate::provider::{Decoder, Endpoint, Event, Provider, StopReason, Usage};

const URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

pub struct Anthropic;

/// The effort levels a model accepts; empty when it has none (or is not one this knows).
pub fn efforts_for(model: &str) -> Vec<Effort> {
    use Effort::*;
    let m = model;
    if m.starts_with("claude-haiku-4") || m.starts_with("claude-haiku-3") || m.starts_with("claude-3") {
        return Vec::new();
    }
    // The older 4.x models predate the `xhigh` level (it arrived with 4.7).
    if m.starts_with("claude-opus-4-6") || m.starts_with("claude-sonnet-4-6") {
        return vec![Low, Medium, High, Max];
    }
    if m.starts_with("claude-opus-4-5") {
        return vec![Low, Medium, High];
    }
    let known = ["claude-fable-", "claude-mythos-", "claude-opus-5", "claude-opus-4-7", "claude-opus-4-8", "claude-sonnet-5", "claude-haiku-5"];
    if known.iter().any(|p| m.starts_with(p)) {
        return Effort::ALL.to_vec();
    }
    Vec::new()
}

/// Whether the request should say `thinking: adaptive`. Every current model thinks adaptively; the ones
/// that predate it (Haiku 4.5 and older) take a different parameter that this does not send.
pub fn wants_adaptive_thinking(model: &str) -> bool {
    !(model.starts_with("claude-haiku-4") || model.starts_with("claude-haiku-3") || model.starts_with("claude-3"))
}

impl Provider for Anthropic {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    fn endpoint(&self) -> Endpoint {
        Endpoint {
            url: URL.to_string(),
            headers: vec![
                ("anthropic-version".to_string(), API_VERSION.to_string()),
                ("content-type".to_string(), "application/json".to_string()),
            ],
        }
    }

    fn auth_header(&self, key: &str) -> (String, String) {
        ("x-api-key".to_string(), key.trim().to_string())
    }

    fn efforts(&self, model: &str) -> Vec<Effort> {
        efforts_for(model)
    }

    fn body(&self, request: &Request) -> Value {
        let mut messages: Vec<Value> = request.messages.iter().map(message).collect();
        mark_conversation_tail(&mut messages);
        let mut body = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "stream": true,
            "messages": messages,
        });
        if !request.system.trim().is_empty() {
            body["system"] = json!([{
                "type": "text",
                "text": request.system,
                "cache_control": standing_mark(request.long_cache),
            }]);
        }
        if !request.tools.is_empty() {
            let last = request.tools.len() - 1;
            body["tools"] = Value::Array(
                request
                    .tools
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let mut tool = json!({
                            "name": t.name,
                            "description": t.description,
                            "input_schema": t.input_schema,
                            // Large arguments (a file's new text) stream as they are written instead of
                            // arriving in one burst — and are validated when the block ends.
                            "eager_input_streaming": true,
                        });
                        // The cache mark on the LAST tool covers the whole list before it.
                        if i == last {
                            tool["cache_control"] = standing_mark(request.long_cache);
                        }
                        tool
                    })
                    .collect(),
            );
        }
        if wants_adaptive_thinking(&request.model) {
            body["thinking"] = json!({ "type": "adaptive" });
        }
        if let Some(effort) = request.effort.and_then(|e| e.clamp_to(&efforts_for(&request.model))) {
            body["output_config"] = json!({ "effort": effort.wire() });
        }
        body
    }

    fn decoder(&self) -> Box<dyn Decoder + Send> {
        Box::new(AnthropicDecoder::default())
    }
}

/// The cache mark on the standing prefix (tools, then instructions). With `long` it asks for the one-hour cache:
/// written once at twice the input price, read at the usual fraction, and shared by every conversation that sends
/// the same prefix. The provider requires a one-hour entry to come before any five-minute one, which is why the
/// conversation's own mark (see [`mark_conversation_tail`]) stays on the short cache.
fn standing_mark(long: bool) -> Value {
    if long {
        json!({ "type": "ephemeral", "ttl": "1h" })
    } else {
        json!({ "type": "ephemeral" })
    }
}

/// The third and last cache mark: on the final block of the final message, so the whole conversation so far is
/// read from the cache by the next request instead of being paid for again. Only the few tokens added since
/// the previous request are new. The provider looks back from a mark for an earlier write, at most twenty
/// blocks, and a request adds two messages, so the previous mark is always within reach.
fn mark_conversation_tail(messages: &mut [Value]) {
    let Some(last) = messages.last_mut().and_then(|m| m.get_mut("content")).and_then(Value::as_array_mut).and_then(|c| c.last_mut()) else {
        return;
    };
    last["cache_control"] = json!({ "type": "ephemeral" });
}

fn message(m: &Message) -> Value {
    json!({
        "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
        "content": m.blocks.iter().filter_map(|b| block(b, m)).collect::<Vec<_>>(),
    })
}

/// One block of `m` as the API wants it. A picture is not a block of its own: the API takes it *inside*
/// the `tool_result` it belongs to, so the result gathers the pictures that follow it in the message.
fn block(b: &Block, m: &Message) -> Option<Value> {
    Some(match b {
        Block::Text { text } => json!({ "type": "text", "text": text }),
        Block::ToolUse { id, name, input } => json!({ "type": "tool_use", "id": id, "name": name, "input": input }),
        Block::ToolResult { tool_use_id, content, is_error } => {
            let pictures: Vec<Value> = m
                .blocks
                .iter()
                .filter_map(|x| match x {
                    Block::ToolImage { tool_use_id: id, media_type, data: Some(data), .. } if id == tool_use_id => {
                        Some(json!({ "type": "image", "source": { "type": "base64", "media_type": media_type, "data": data } }))
                    }
                    _ => None,
                })
                .collect();
            let body = if pictures.is_empty() {
                json!(content)
            } else {
                let mut parts = vec![json!({ "type": "text", "text": content })];
                parts.extend(pictures);
                Value::Array(parts)
            };
            let mut v = json!({ "type": "tool_result", "tool_use_id": tool_use_id, "content": body });
            if *is_error {
                v["is_error"] = json!(true);
            }
            v
        }
        // Already inside its result when it has bytes. One without them (never loaded, or its file is gone)
        // is said in words by the host before the request; a stray one is dropped rather than sent broken.
        Block::ToolImage { .. } => return None,
    })
}

// ── the stream ────────────────────────────────────────────────────────────────────────────────

/// A tool call being assembled: its identity, and the pieces of its arguments so far.
struct ToolAcc {
    id: String,
    name: String,
    json: String,
}

#[derive(Default)]
pub struct AnthropicDecoder {
    /// Text received but not yet a whole line.
    buffer: String,
    /// The `event:` of the SSE event being read.
    event: Option<String>,
    /// The `data:` lines of the SSE event being read.
    data: Vec<String>,
    /// Tool calls in progress, by content-block index.
    tools: HashMap<u64, ToolAcc>,
}

impl AnthropicDecoder {
    fn line(&mut self, raw: &str, out: &mut Vec<Event>) {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() {
            self.dispatch(out);
        } else if line.starts_with(':') {
            // A comment — keep-alives arrive this way.
        } else if let Some((field, value)) = line.split_once(':') {
            let value = value.strip_prefix(' ').unwrap_or(value);
            match field {
                "event" => self.event = Some(value.to_string()),
                "data" => self.data.push(value.to_string()),
                _ => {}
            }
        }
    }

    fn dispatch(&mut self, out: &mut Vec<Event>) {
        let name = self.event.take();
        let data = std::mem::take(&mut self.data).join("\n");
        if data.is_empty() {
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(&data) else {
            // A line that is not JSON cannot be an event; saying so beats skipping it silently.
            out.push(Event::Error {
                error_type: Some("unreadable_event".into()),
                message: format!("unreadable event from the provider: {}", truncate(&data, 120)),
            });
            return;
        };
        // The JSON's own `type` is authoritative; the SSE `event:` line repeats it.
        let kind = v.get("type").and_then(Value::as_str).or(name.as_deref()).unwrap_or("");
        self.event_of(kind, &v, out);
    }

    fn event_of(&mut self, kind: &str, v: &Value, out: &mut Vec<Event>) {
        match kind {
            "message_start" => {
                if let Some(u) = v.pointer("/message/usage") {
                    out.push(Event::Usage(usage(u)));
                }
            }
            "content_block_start" => {
                let index = v.get("index").and_then(Value::as_u64).unwrap_or(0);
                let b = v.get("content_block").unwrap_or(&Value::Null);
                if b.get("type").and_then(Value::as_str) == Some("tool_use") {
                    let id = str_of(b, "id");
                    let name = str_of(b, "name");
                    out.push(Event::ToolStart { id: id.clone(), name: name.clone() });
                    self.tools.insert(index, ToolAcc { id, name, json: String::new() });
                }
            }
            "content_block_delta" => {
                let index = v.get("index").and_then(Value::as_u64).unwrap_or(0);
                let d = v.get("delta").unwrap_or(&Value::Null);
                match d.get("type").and_then(Value::as_str) {
                    Some("text_delta") => out.push(Event::Text(str_of(d, "text"))),
                    Some("thinking_delta") => out.push(Event::Thinking(str_of(d, "thinking"))),
                    Some("input_json_delta") => {
                        if let Some(acc) = self.tools.get_mut(&index) {
                            acc.json.push_str(d.get("partial_json").and_then(Value::as_str).unwrap_or(""));
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = v.get("index").and_then(Value::as_u64).unwrap_or(0);
                if let Some(acc) = self.tools.remove(&index) {
                    // A tool with no arguments sends none: that is `{}`, not a failure.
                    let text = if acc.json.trim().is_empty() { "{}" } else { acc.json.as_str() };
                    match serde_json::from_str::<Value>(text) {
                        Ok(input) if input.is_object() => {
                            out.push(Event::ToolDone { id: acc.id, name: acc.name, input, valid: true })
                        }
                        _ => out.push(Event::ToolDone { id: acc.id, name: acc.name, input: Value::Null, valid: false }),
                    }
                }
            }
            "message_delta" => {
                if let Some(u) = v.get("usage") {
                    out.push(Event::Usage(usage(u)));
                }
                if let Some(reason) = v.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    out.push(Event::Stop(stop_reason(reason)));
                }
            }
            "error" => {
                let message = v.pointer("/error/message").and_then(Value::as_str).unwrap_or("the provider reported an error");
                let error_type = v.pointer("/error/type").and_then(Value::as_str).map(str::to_string);
                out.push(Event::Error { error_type, message: message.to_string() });
            }
            // `message_stop` and `ping` carry nothing the neutral model needs.
            _ => {}
        }
    }
}

impl Decoder for AnthropicDecoder {
    fn push(&mut self, chunk: &str) -> Vec<Event> {
        let mut out = Vec::new();
        self.buffer.push_str(chunk);
        while let Some(at) = self.buffer.find('\n') {
            let line: String = self.buffer.drain(..=at).collect();
            self.line(line.trim_end_matches('\n'), &mut out);
        }
        out
    }

    fn finish(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        if !self.buffer.is_empty() {
            let rest = std::mem::take(&mut self.buffer);
            self.line(&rest, &mut out);
        }
        self.dispatch(&mut out);
        out
    }
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn num(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn usage(u: &Value) -> Usage {
    Usage {
        input_tokens: num(u, "input_tokens"),
        output_tokens: num(u, "output_tokens"),
        cache_read_tokens: num(u, "cache_read_input_tokens"),
        cache_write_tokens: num(u, "cache_creation_input_tokens"),
        // The provider breaks a cache write down by lifetime; only the one-hour part is priced differently.
        cache_write_1h_tokens: u.get("cache_creation").map(|c| num(c, "ephemeral_1h_input_tokens")).unwrap_or(0),
    }
}

fn stop_reason(reason: &str) -> StopReason {
    match reason {
        "end_turn" | "stop_sequence" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "refusal" => StopReason::Refusal,
        _ => StopReason::Other,
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::ToolSpec;

    fn request() -> Request {
        Request {
            model: "claude-opus-5-5".into(),
            system: "You are Bennu's assistant.".into(),
            messages: vec![Message::user("rename total")],
            tools: vec![
                ToolSpec { name: "read_file".into(), description: "Read a file".into(), input_schema: json!({"type": "object"}) },
                ToolSpec { name: "find_usages".into(), description: "Find usages".into(), input_schema: json!({"type": "object"}) },
            ],
            max_tokens: 4096,
            effort: Some(Effort::High),
            long_cache: false,
        }
    }

    // ── going out ──

    #[test]
    fn the_conversation_itself_is_cached_up_to_its_last_block() {
        let mut r = request();
        r.messages = vec![
            Message::user("go"),
            Message::assistant(vec![Block::ToolUse { id: "t".into(), name: "read_file".into(), input: json!({}) }]),
            Message::tool_results(vec![Block::ToolResult { tool_use_id: "t".into(), content: "x".into(), is_error: false }]),
        ];
        let body = Anthropic.body(&r);
        let marks: usize = body["messages"].as_array().unwrap().iter().flat_map(|m| m["content"].as_array().unwrap()).filter(|b| b.get("cache_control").is_some()).count();
        assert_eq!(marks, 1, "one mark, on the end of the conversation");
        assert_eq!(body["messages"][2]["content"][0]["cache_control"]["type"], "ephemeral");
        // 3 of the provider's 4 allowed marks: tools, instructions, conversation.
        assert!(body["tools"][1].get("cache_control").is_some() && body["system"][0].get("cache_control").is_some());
    }

    #[test]
    fn the_long_cache_applies_to_the_standing_prefix_only_and_comes_first() {
        let mut r = request();
        r.long_cache = true;
        let body = Anthropic.body(&r);
        assert_eq!(body["system"][0]["cache_control"]["ttl"], "1h");
        assert_eq!(body["tools"][1]["cache_control"]["ttl"], "1h");
        // A one-hour entry must precede any five-minute one: the conversation's mark has no ttl (the short default).
        assert!(body["messages"][0]["content"][0]["cache_control"].get("ttl").is_none());
        assert!(Anthropic.body(&request())["system"][0]["cache_control"].get("ttl").is_none());
    }

    #[test]
    fn the_cacheable_prefix_is_the_same_for_requests_that_differ_only_after_it() {
        let a = Anthropic.body(&request());
        let mut r = request();
        r.messages.push(Message::assistant(vec![Block::Text { text: "ok".into() }]));
        r.messages.push(Message::user("and now?"));
        let b = Anthropic.body(&r);
        assert_eq!(a["system"], b["system"]);
        assert_eq!(a["tools"], b["tools"]);
        assert_eq!(a["messages"][0]["content"][0]["text"], b["messages"][0]["content"][0]["text"], "the earlier turn is word for word the same");
    }

    #[test]
    fn the_request_asks_for_a_stream_and_marks_what_is_stable_cacheable() {
        let body = Anthropic.body(&request());
        assert_eq!(body["stream"], json!(true));
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        // The mark is on the last tool only: one breakpoint covers the list before it.
        assert!(body["tools"][0].get("cache_control").is_none());
        assert_eq!(body["tools"][1]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["tools"][0]["eager_input_streaming"], json!(true));
    }

    #[test]
    fn effort_and_adaptive_thinking_go_to_a_model_that_has_them() {
        let body = Anthropic.body(&request());
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["output_config"]["effort"], "high");
    }

    #[test]
    fn a_model_without_the_setting_is_never_sent_one() {
        let mut r = request();
        r.model = "claude-haiku-4-5".into();
        let body = Anthropic.body(&r);
        assert!(body.get("output_config").is_none(), "Haiku 4.5 rejects an effort");
        assert!(body.get("thinking").is_none(), "and takes a different thinking parameter");
    }

    #[test]
    fn an_effort_the_model_lacks_is_lowered_to_one_it_has() {
        let mut r = request();
        r.model = "claude-opus-4-6".into();
        r.effort = Some(Effort::XHigh); // 4.6 has no xhigh
        let body = Anthropic.body(&r);
        assert_eq!(body["output_config"]["effort"], "high");
    }

    #[test]
    fn no_system_and_no_tools_leave_those_fields_out() {
        let mut r = request();
        r.system = "  ".into();
        r.tools.clear();
        r.effort = None;
        let body = Anthropic.body(&r);
        assert!(body.get("system").is_none() && body.get("tools").is_none() && body.get("output_config").is_none());
    }

    #[test]
    fn tool_calls_and_their_results_keep_their_ids() {
        let mut r = request();
        r.messages = vec![
            Message::user("go"),
            Message::assistant(vec![Block::ToolUse { id: "toolu_1".into(), name: "read_file".into(), input: json!({"path": "A.java"}) }]),
            Message::tool_results(vec![
                Block::ToolResult { tool_use_id: "toolu_1".into(), content: "412 lines".into(), is_error: false },
                Block::ToolResult { tool_use_id: "toolu_2".into(), content: "denied".into(), is_error: true },
            ]),
        ];
        let body = Anthropic.body(&r);
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][1]["content"][0]["input"]["path"], "A.java");
        let results = &body["messages"][2]["content"];
        assert!(results[0].get("is_error").is_none(), "a success does not say so");
        assert_eq!(results[1]["is_error"], json!(true));
    }

    #[test]
    fn a_picture_travels_inside_the_result_it_belongs_to() {
        let mut r = request();
        let image = |id: &str, data: Option<&str>| Block::ToolImage {
            tool_use_id: id.into(),
            media_type: "image/png".into(),
            path: "s.png".into(),
            tokens: 900,
            data: data.map(String::from),
        };
        r.messages = vec![
            Message::tool_results(vec![
                Block::ToolResult { tool_use_id: "a".into(), content: "shot taken".into(), is_error: false },
                Block::ToolResult { tool_use_id: "b".into(), content: "plain".into(), is_error: false },
                image("a", Some("QUJD")),
                image("b", None),
            ]),
        ];
        let content = &Anthropic.body(&r)["messages"][0]["content"];
        assert_eq!(content.as_array().unwrap().len(), 2, "pictures are not blocks of their own");
        assert_eq!(content[0]["content"][0]["text"], "shot taken");
        assert_eq!(content[0]["content"][1]["type"], "image");
        assert_eq!(content[0]["content"][1]["source"]["data"], "QUJD");
        assert_eq!(content[1]["content"], "plain", "a result without a picture is still a plain string");
    }

    #[test]
    fn the_key_goes_in_its_own_header_and_is_trimmed() {
        assert_eq!(Anthropic.auth_header("  sk-ant-abc\n"), ("x-api-key".to_string(), "sk-ant-abc".to_string()));
        let e = Anthropic.endpoint();
        assert!(e.url.ends_with("/v1/messages"));
        assert!(e.headers.iter().any(|(k, _)| k == "anthropic-version"));
        assert!(!e.headers.iter().any(|(k, _)| k.contains("key")), "the credential is not among the plain headers");
    }

    #[test]
    fn which_models_have_which_levels() {
        assert_eq!(efforts_for("claude-opus-5-5").len(), 5);
        assert_eq!(efforts_for("claude-haiku-4-5"), Vec::<Effort>::new());
        assert_eq!(efforts_for("claude-opus-4-6"), vec![Effort::Low, Effort::Medium, Effort::High, Effort::Max]);
        assert_eq!(efforts_for("some-other-model"), Vec::<Effort>::new(), "an unknown model gets none rather than a guess");
    }

    // ── coming back ──

    const STREAM: &str = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":1200,\"cache_read_input_tokens\":1000,\"output_tokens\":1}}}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}

event: ping
data: {\"type\":\"ping\"}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Let me \"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"look.\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_9\",\"name\":\"read_file\",\"input\":{}}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\": \\\"src/\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"A.java\\\"}\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":1}

event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":57}}

event: message_stop
data: {\"type\":\"message_stop\"}

";

    fn decode_all(chunks: &[&str]) -> Vec<Event> {
        let mut d = AnthropicDecoder::default();
        let mut out: Vec<Event> = chunks.iter().flat_map(|c| d.push(c)).collect();
        out.extend(d.finish());
        out
    }

    #[test]
    fn a_stream_becomes_text_a_tool_call_usage_and_a_stop() {
        let events = decode_all(&[STREAM]);
        let text: String = events.iter().filter_map(|e| if let Event::Text(t) = e { Some(t.as_str()) } else { None }).collect();
        assert_eq!(text, "Let me look.");
        assert!(events.contains(&Event::ToolStart { id: "toolu_9".into(), name: "read_file".into() }));
        assert!(events.contains(&Event::ToolDone {
            id: "toolu_9".into(),
            name: "read_file".into(),
            input: json!({"path": "src/A.java"}),
            valid: true,
        }), "{events:?}");
        assert!(events.contains(&Event::Usage(Usage { input_tokens: 1200, output_tokens: 1, cache_read_tokens: 1000, ..Default::default() })));
        assert_eq!(events.last(), Some(&Event::Stop(StopReason::ToolUse)));
    }

    #[test]
    fn the_events_are_the_same_however_the_stream_is_cut_up() {
        let whole = decode_all(&[STREAM]);
        let by_character: Vec<String> = STREAM.chars().map(|c| c.to_string()).collect();
        let refs: Vec<&str> = by_character.iter().map(String::as_str).collect();
        assert_eq!(decode_all(&refs), whole, "a chunk can end anywhere, mid-line included");
    }

    #[test]
    fn carriage_returns_and_comments_are_ignored() {
        let events = decode_all(&[": keep-alive\r\n\r\nevent: content_block_delta\r\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\r\n\r\n"]);
        assert_eq!(events, vec![Event::Text("hi".into())]);
    }

    #[test]
    fn a_tool_with_no_arguments_is_valid_and_empty() {
        let s = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t\",\"name\":\"list\",\"input\":{}}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n";
        let events = decode_all(&[s]);
        assert!(events.contains(&Event::ToolDone { id: "t".into(), name: "list".into(), input: json!({}), valid: true }));
    }

    #[test]
    fn arguments_that_do_not_parse_are_flagged_not_guessed() {
        // A stream cut short mid-argument: the JSON never closes.
        let s = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t\",\"name\":\"edit\",\"input\":{}}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\": \\\"A.ja\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n";
        let events = decode_all(&[s]);
        assert!(events.contains(&Event::ToolDone { id: "t".into(), name: "edit".into(), input: Value::Null, valid: false }));
    }

    #[test]
    fn a_refusal_is_a_stop_not_an_error() {
        let s = "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\"},\"usage\":{\"output_tokens\":3}}\n\n";
        assert_eq!(decode_all(&[s]).last(), Some(&Event::Stop(StopReason::Refusal)));
    }

    #[test]
    fn an_error_in_the_stream_is_reported_with_its_message() {
        let s = "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
        assert_eq!(decode_all(&[s]), vec![Event::Error { error_type: Some("overloaded_error".into()), message: "Overloaded".into() }]);
    }

    #[test]
    fn a_final_event_without_its_blank_line_is_still_read() {
        let s = "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"end\"}}";
        assert_eq!(decode_all(&[s]), vec![Event::Text("end".into())]);
    }

    #[test]
    fn reasoning_is_its_own_event() {
        let s = "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"hmm\"}}\n\n";
        assert_eq!(decode_all(&[s]), vec![Event::Thinking("hmm".into())]);
    }
}
