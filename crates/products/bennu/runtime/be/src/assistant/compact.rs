//! Making a long conversation shorter without losing what it was for.
//!
//! Two steps, cheapest first. **Clearing** blanks the output of old tool calls — what was read and
//! searched many turns ago is the bulk of a long conversation and the part the model needs least, since
//! the conclusions it drew are still in the text. It costs nothing and loses no decision. **Summarising**
//! asks the model to fold everything before the last turn into a note, for when clearing is not enough or
//! the user asked for it; it costs a request and can leave out a detail, which is why it is the second
//! step and why the user can say what it must keep.
//!
//! The last user turn is never folded: it is what the model is about to answer.

use bennu_assistant::prelude::{
    run, AgentEvent, Anthropic, Block, Gate, Host, Message, Request, Role, ToolCall, ToolSpec, Transport, Usage,
};

/// The marker left where an old tool result was.
pub(super) const CLEARED: &str = "[older tool output cleared to save room]";

pub(super) fn tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

/// About how many input tokens `request` would use: instructions, tool definitions and messages.
pub(super) fn estimate_request(request: &Request) -> u64 {
    let tools = serde_json::to_string(&request.tools).unwrap_or_default();
    let messages = serde_json::to_string(&request.messages).unwrap_or_default();
    tokens(&request.system) + tokens(&tools) + tokens(&messages)
}

/// Blank the bulky tool results that come before the last `keep_turns` assistant turns. The calls
/// themselves stay, so every result still pairs with its use. Returns the new transcript and the
/// tokens it freed.
pub(super) fn clear_old_tool_results(transcript: &[Message], keep_turns: usize) -> (Vec<Message>, u64) {
    let assistant_at: Vec<usize> =
        transcript.iter().enumerate().filter(|(_, m)| m.role == Role::Assistant).map(|(i, _)| i).collect();
    let cutoff = if assistant_at.len() > keep_turns { assistant_at[assistant_at.len() - keep_turns] } else { 0 };
    let mut freed = 0u64;
    let out = transcript
        .iter()
        .enumerate()
        .map(|(i, m)| {
            if i >= cutoff {
                return m.clone();
            }
            let blocks = m
                .blocks
                .iter()
                .map(|b| match b {
                    Block::ToolResult { tool_use_id, content, is_error } if content.chars().count() > 200 => {
                        freed += tokens(content).saturating_sub(tokens(CLEARED));
                        Block::ToolResult { tool_use_id: tool_use_id.clone(), content: CLEARED.into(), is_error: *is_error }
                    }
                    other => other.clone(),
                })
                .collect();
            Message { role: m.role, blocks }
        })
        .collect();
    (out, freed)
}

/// Whether this message is something the user wrote (as opposed to carrying tool results back).
fn is_user_turn(m: &Message) -> bool {
    m.role == Role::User && m.blocks.iter().any(|b| matches!(b, Block::Text { .. }))
}

/// Split at the start of the last `keep_turns` user turns: what is folded, and what is kept as it is.
/// Nothing is folded when the conversation has no more turns than that.
pub(super) fn split_for_summary(transcript: &[Message], keep_turns: usize) -> (Vec<Message>, Vec<Message>) {
    let turns: Vec<usize> = transcript.iter().enumerate().filter(|(_, m)| is_user_turn(m)).map(|(i, _)| i).collect();
    if turns.len() <= keep_turns {
        return (Vec::new(), transcript.to_vec());
    }
    let at = turns[turns.len() - keep_turns];
    (transcript[..at].to_vec(), transcript[at..].to_vec())
}

/// The folded part as plain text for the summariser to read. Tool output is cut short: the summary is
/// of what happened, not a copy of what was read.
pub(super) fn render_for_summary(messages: &[Message]) -> String {
    const RESULT_CHARS: usize = 400;
    let mut out = String::new();
    for m in messages {
        for b in &m.blocks {
            match b {
                Block::Text { text } => {
                    out.push_str(if m.role == Role::User { "User: " } else { "Assistant: " });
                    out.push_str(text);
                }
                Block::ToolUse { name, input, .. } => out.push_str(&format!("[called {name} {input}]")),
                Block::ToolResult { content, is_error, .. } => {
                    let head: String = content.chars().take(RESULT_CHARS).collect();
                    let more = if content.chars().count() > RESULT_CHARS { " …" } else { "" };
                    out.push_str(&format!("[{} {head}{more}]", if *is_error { "error" } else { "result" }));
                }
            }
            out.push_str("\n\n");
        }
    }
    out
}

const SUMMARY_SYSTEM: &str = "You write the note that replaces the early part of a long conversation between a developer and a coding assistant, so the conversation can go on in less room. Be factual and dense. Keep: what the user wants overall; decisions made and why; files read, changed or proposed (with paths); symbols and names that matter; things tried that did not work; what is still to do. Leave out pleasantries and anything that can be looked up again with a tool. Write plain text, no preamble.";

pub(super) fn summary_request(older: &[Message], instructions: Option<&str>, model: &str) -> Request {
    let mut ask = format!("Here is the part of the conversation to fold into a note:\n\n{}", render_for_summary(older));
    if let Some(extra) = instructions.map(str::trim).filter(|s| !s.is_empty()) {
        ask.push_str(&format!("\n\nThe user asks that the note especially keep or emphasise: {extra}"));
    }
    ask.push_str("\n\nWrite the note now.");
    Request {
        model: model.to_string(),
        system: SUMMARY_SYSTEM.to_string(),
        messages: vec![Message::user(ask)],
        tools: Vec::new(),
        max_tokens: 4096,
        effort: None,
    }
}

/// Put the note in front of what is kept: the kept part starts with a user turn, so the note joins it
/// rather than adding a second consecutive user message.
pub(super) fn fold(summary: &str, mut kept: Vec<Message>) -> Vec<Message> {
    let note = Block::Text { text: format!("[Summary of the earlier conversation]\n{}\n[End of summary]\n\n", summary.trim()) };
    match kept.first_mut() {
        Some(first) if first.role == Role::User => first.blocks.insert(0, note),
        _ => kept.insert(0, Message { role: Role::User, blocks: vec![note] }),
    }
    kept
}

/// A host for a request that uses no tools: it only collects the answer.
struct NoTools;

impl Host for NoTools {
    fn tools(&self) -> Vec<ToolSpec> {
        Vec::new()
    }
    fn gate(&self, _call: &ToolCall) -> Gate {
        Gate::Refuse("no tools here".into())
    }
    fn run(&self, _call: &ToolCall) -> Result<String, String> {
        Err("no tools here".into())
    }
    fn event(&self, _event: AgentEvent) {}
    fn cancelled(&self) -> bool {
        false
    }
}

/// Ask the model for the note. Returns it and what the request used.
pub(super) fn summarize(
    transport: &dyn Transport,
    key: &str,
    model: &str,
    older: &[Message],
    instructions: Option<&str>,
) -> Result<(String, Usage), String> {
    let outcome = run(&Anthropic, transport, &NoTools, key, summary_request(older, instructions, model), 1)?;
    let text = outcome.added.iter().map(Message::text).collect::<Vec<_>>().join("\n");
    if text.trim().is_empty() {
        return Err("the model returned an empty summary".into());
    }
    Ok((text, outcome.usage))
}

/// The result of compacting a conversation.
pub(super) struct Compacted {
    pub transcript: Vec<Message>,
    pub summarized: bool,
    pub usage: Usage,
}

/// Compact `transcript`: clear old tool output, then — when `always_summarize` or the result is still
/// over `limit_tokens` (measured by `size_of`) — fold everything before the last turn into a note.
pub(super) fn compact(
    transcript: &[Message],
    transport: &dyn Transport,
    key: &str,
    model: &str,
    instructions: Option<&str>,
    always_summarize: bool,
    size_of: &dyn Fn(&[Message]) -> u64,
    limit_tokens: u64,
) -> Result<Compacted, String> {
    let (cleared, _) = clear_old_tool_results(transcript, 2);
    if !always_summarize && size_of(&cleared) < limit_tokens {
        return Ok(Compacted { transcript: cleared, summarized: false, usage: Usage::default() });
    }
    let (older, kept) = split_for_summary(&cleared, 1);
    if older.is_empty() {
        return Ok(Compacted { transcript: cleared, summarized: false, usage: Usage::default() });
    }
    let (summary, usage) = summarize(transport, key, model, &older, instructions)?;
    Ok(Compacted { transcript: fold(&summary, kept), summarized: true, usage })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_assistant::prelude::HttpRequest;
    use serde_json::json;
    use std::cell::RefCell;

    fn exchange(id: &str, size: usize) -> [Message; 2] {
        [
            Message::assistant(vec![Block::ToolUse { id: id.into(), name: "read".into(), input: json!({"f": id}) }]),
            Message { role: Role::User, blocks: vec![Block::ToolResult { tool_use_id: id.into(), content: "x".repeat(size), is_error: false }] },
        ]
    }

    fn long() -> Vec<Message> {
        let mut t = vec![Message::user("first ask")];
        for i in 0..3 {
            t.extend(exchange(&format!("a{i}"), 4000));
        }
        t.push(Message::assistant(vec![Block::Text { text: "done with the first".into() }]));
        t.push(Message::user("second ask"));
        for i in 0..2 {
            t.extend(exchange(&format!("b{i}"), 4000));
        }
        t
    }

    #[test]
    fn old_tool_output_is_cleared_and_recent_output_kept() {
        let mut t = vec![Message::user("go")];
        for i in 0..4 {
            t.extend(exchange(&format!("t{i}"), 4000));
        }
        let (out, freed) = clear_old_tool_results(&t, 2);
        let contents: Vec<&str> = out
            .iter()
            .flat_map(|m| m.blocks.iter())
            .filter_map(|b| if let Block::ToolResult { content, .. } = b { Some(content.as_str()) } else { None })
            .collect();
        assert_eq!(contents.iter().filter(|c| **c == CLEARED).count(), 2);
        assert_eq!(contents.iter().filter(|c| c.len() == 4000).count(), 2);
        assert!(freed > 1500, "{freed}");
        assert_eq!(out.len(), t.len(), "every call still has its result");
    }

    #[test]
    fn a_short_conversation_is_left_alone() {
        let mut t = vec![Message::user("go")];
        t.extend(exchange("t0", 4000));
        assert_eq!(clear_old_tool_results(&t, 2).1, 0);
    }

    #[test]
    fn the_split_keeps_the_last_user_turn_whole() {
        let (older, kept) = split_for_summary(&long(), 1);
        assert_eq!(kept[0].text(), "second ask");
        assert_eq!(older.len() + kept.len(), long().len());
        assert!(older.iter().any(|m| m.text() == "first ask"));
    }

    #[test]
    fn a_conversation_of_one_turn_has_nothing_to_fold() {
        let (older, kept) = split_for_summary(&[Message::user("only")], 1);
        assert!(older.is_empty());
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn tool_tool_results_do_not_count_as_turns() {
        // Results travel in user-role messages without text; they must not be taken for a new ask.
        let t = long();
        let turns = t.iter().filter(|m| is_user_turn(m)).count();
        assert_eq!(turns, 2);
    }

    #[test]
    fn the_rendering_cuts_long_results_and_marks_errors() {
        let t = vec![
            Message::assistant(vec![Block::ToolUse { id: "1".into(), name: "read".into(), input: json!({"f": "A"}) }]),
            Message { role: Role::User, blocks: vec![Block::ToolResult { tool_use_id: "1".into(), content: "y".repeat(2000), is_error: false }] },
            Message { role: Role::User, blocks: vec![Block::ToolResult { tool_use_id: "2".into(), content: "boom".into(), is_error: true }] },
        ];
        let text = render_for_summary(&t);
        assert!(text.contains("[called read") && text.contains("[error boom]"));
        assert!(text.len() < 700, "{}", text.len());
    }

    #[test]
    fn the_request_names_what_the_user_wants_kept() {
        let r = summary_request(&long()[..2], Some("the decision about EUR"), "m");
        assert!(r.messages[0].text().contains("the decision about EUR"));
        assert!(r.tools.is_empty());
        assert!(!summary_request(&long()[..2], Some("  "), "m").messages[0].text().contains("especially"));
    }

    #[test]
    fn the_note_joins_the_kept_user_turn() {
        let (_, kept) = split_for_summary(&long(), 1);
        let folded = fold("they wanted EUR", kept);
        assert_eq!(folded[0].role, Role::User);
        assert!(folded[0].text().starts_with("[Summary of the earlier conversation]"));
        assert!(folded[0].text().contains("second ask"));
    }

    /// Answers every request with one scripted text reply.
    struct Reply(&'static str, RefCell<usize>);
    impl Transport for Reply {
        fn stream(&self, _r: &HttpRequest, on_chunk: &mut dyn FnMut(&str) -> bool) -> Result<(), String> {
            *self.1.borrow_mut() += 1;
            let sse = format!(
                "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: {}\n\n",
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":self.0}}),
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}),
            );
            on_chunk(&sse);
            Ok(())
        }
    }

    fn size(m: &[Message]) -> u64 {
        tokens(&serde_json::to_string(m).unwrap_or_default())
    }

    #[test]
    fn clearing_is_enough_when_it_gets_under_the_limit() {
        let transport = Reply("S", RefCell::new(0));
        let out = compact(&long(), &transport, "k", "m", None, false, &size, 100_000).unwrap();
        assert!(!out.summarized);
        assert_eq!(*transport.1.borrow(), 0, "no request was made");
    }

    #[test]
    fn it_summarises_when_clearing_is_not_enough() {
        let transport = Reply("They want EUR as the default.", RefCell::new(0));
        let out = compact(&long(), &transport, "k", "m", None, false, &size, 10).unwrap();
        assert!(out.summarized);
        assert!(out.transcript[0].text().contains("They want EUR"));
        assert_eq!(out.usage.output_tokens, 7);
        assert!(out.transcript.len() < long().len());
    }

    #[test]
    fn a_manual_compact_always_summarises() {
        let transport = Reply("note", RefCell::new(0));
        assert!(compact(&long(), &transport, "k", "m", Some("keep X"), true, &size, u64::MAX).unwrap().summarized);
    }
}
