//! What a sub-agent's thread keeps on disk, and how a kept thread is picked up again.
//!
//! An agent's thread is mostly what it read: every file it opened is a tool result with the whole text in it.
//! Writing that to disk would copy the project into the assistant's data folder for every agent, so the kept
//! copy ([`slim_for_storage`]) says what was read instead — `[read_files {"files":["A.java"]} — 412 lines, not
//! kept; read it again if you need it]` — and keeps everything the agent *said and decided*: its own words, the
//! calls it made, the short results. Resuming an agent ([`continue_with`]) hands it that thread with a new
//! instruction; it knows what it had looked at and concluded, and reads again what it needs.
//!
//! Pure functions over messages: the files and the lifecycle (deleted with the conversation, at a compaction, or
//! on request) are the host's.

use std::collections::HashMap;

use crate::message::{Block, Message, Role};

/// A tool result up to this long is kept as it is: it is a decision, a path, a count, not a file's text.
const KEEP_RESULT_CHARS: usize = 600;
/// How much of a call's arguments the note that stands in for a result repeats.
const ARGS_CHARS: usize = 160;
/// How much of the start of a result is kept when the tool did not read a file's text.
const HEAD_CHARS: usize = 240;

fn clip(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('…');
    }
    out
}

/// Whether a tool hands back a file's text (a read, an outline): for those the note names the file and keeps no
/// excerpt, because an excerpt of a file is the file.
fn gives_file_text(tool: &str) -> bool {
    tool.contains("read") || tool.contains("outline")
}

fn stand_in(tool: &str, args: &str, content: &str) -> String {
    let chars = content.chars().count();
    let call = if args.is_empty() { tool.to_string() } else { format!("{tool} {args}") };
    if gives_file_text(tool) {
        format!("[{call} — {} lines, {chars} characters, not kept; read it again if you need it]", content.lines().count())
    } else {
        format!("[{call} — {chars} characters, not kept. It began: {}]", clip(content, HEAD_CHARS))
    }
}

/// The thread as it is kept: long tool results replaced by a note of what the call was and how much it returned,
/// pictures dropped (their files belong to the conversation that took them), everything else untouched.
pub fn slim_for_storage(messages: &[Message]) -> Vec<Message> {
    let mut calls: HashMap<&str, (&str, String)> = HashMap::new();
    for block in messages.iter().flat_map(|m| m.blocks.iter()) {
        if let Block::ToolUse { id, name, input } = block {
            calls.insert(id.as_str(), (name.as_str(), clip(&input.to_string(), ARGS_CHARS)));
        }
    }
    messages
        .iter()
        .map(|m| Message {
            role: m.role,
            blocks: m
                .blocks
                .iter()
                .filter_map(|b| match b {
                    Block::ToolImage { .. } => None,
                    Block::ToolResult { tool_use_id, content, is_error } if content.chars().count() > KEEP_RESULT_CHARS => {
                        let (tool, args) = calls.get(tool_use_id.as_str()).map_or(("tool", String::new()), |(t, a)| (*t, a.clone()));
                        Some(Block::ToolResult { tool_use_id: tool_use_id.clone(), content: stand_in(tool, &args, content), is_error: *is_error })
                    }
                    other => Some(other.clone()),
                })
                .collect(),
        })
        .collect()
}

/// A kept thread, ready to be sent again with `follow_up` as the next instruction.
///
/// A thread that was cut off mid-turn ends in a call nobody answered, which the provider refuses: that last turn
/// is dropped. The instruction joins a user message if the thread ends on one (a thread cut off before its first
/// answer), so two user messages never follow each other.
pub fn continue_with(mut messages: Vec<Message>, follow_up: &str) -> Vec<Message> {
    while messages.last().is_some_and(|m| m.role == Role::Assistant && m.blocks.iter().any(|b| matches!(b, Block::ToolUse { .. }))) {
        messages.pop();
    }
    match messages.last_mut() {
        Some(last) if last.role == Role::User => last.blocks.push(Block::Text { text: follow_up.to_string() }),
        _ => messages.push(Message::user(follow_up)),
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str, name: &str, input: serde_json::Value) -> Message {
        Message::assistant(vec![Block::ToolUse { id: id.into(), name: name.into(), input }])
    }

    fn result(id: &str, content: &str) -> Message {
        Message { role: Role::User, blocks: vec![Block::ToolResult { tool_use_id: id.into(), content: content.into(), is_error: false }] }
    }

    fn content_of(m: &Message) -> &str {
        match &m.blocks[0] {
            Block::ToolResult { content, .. } => content,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_file_that_was_read_is_named_not_copied() {
        let file = "line of code\n".repeat(400);
        let thread = vec![Message::user("find the total"), call("1", "read_files", json!({"files": ["/p/Cart.java"]})), result("1", &file)];
        let kept = slim_for_storage(&thread);
        let text = content_of(&kept[2]);
        assert!(text.starts_with("[read_files "), "{text}");
        assert!(text.contains("/p/Cart.java") && text.contains("400 lines") && text.contains("not kept"), "{text}");
        assert!(!text.contains("line of code"), "no excerpt of a file");
        assert!(text.len() < 300);
    }

    #[test]
    fn a_long_search_keeps_its_beginning_and_a_short_answer_is_untouched() {
        let hits = "A.java:12: total()\n".repeat(100);
        let thread = vec![call("1", "search_text", json!({"pattern": "total"})), result("1", &hits), call("2", "file_owner", json!({})), result("2", "nobody holds it")];
        let kept = slim_for_storage(&thread);
        assert!(content_of(&kept[1]).contains("It began: A.java:12: total()"));
        assert_eq!(content_of(&kept[3]), "nobody holds it");
    }

    #[test]
    fn what_the_agent_said_and_asked_is_kept_and_pictures_are_dropped() {
        let thread = vec![
            Message::user("task"),
            Message::assistant(vec![Block::Text { text: "I will look at Cart.".into() }, Block::ToolUse { id: "1".into(), name: "screenshot".into(), input: json!({}) }]),
            Message { role: Role::User, blocks: vec![Block::ToolResult { tool_use_id: "1".into(), content: "ok".into(), is_error: false }, Block::ToolImage { tool_use_id: "1".into(), media_type: "image/png".into(), path: "x.png".into(), tokens: 900, data: None }] },
        ];
        let kept = slim_for_storage(&thread);
        assert_eq!(kept[1], thread[1], "its words and its call are as they were");
        assert_eq!(kept[2].blocks.len(), 1, "the picture is gone, the result stays");
    }

    #[test]
    fn a_result_with_no_call_in_the_thread_is_still_replaced() {
        let kept = slim_for_storage(&[result("lost", &"x".repeat(2_000))]);
        assert!(content_of(&kept[0]).contains("2000 characters, not kept"));
    }

    #[test]
    fn resuming_adds_the_instruction_after_the_last_answer() {
        let thread = vec![Message::user("task"), Message::assistant(vec![Block::Text { text: "done: it is in Cart".into() }])];
        let next = continue_with(thread, "now check Order too");
        assert_eq!(next.len(), 3);
        assert_eq!(next[2], Message::user("now check Order too"));
    }

    #[test]
    fn a_thread_cut_off_mid_turn_is_repaired_before_it_is_sent_again() {
        // Ends in a call nobody answered: the provider refuses that, so the last turn is dropped.
        let cut = vec![Message::user("task"), call("1", "read_files", json!({}))];
        let next = continue_with(cut, "go on");
        assert_eq!(next.len(), 1, "the dangling call is gone and the instruction joins the task's message");
        assert_eq!(next[0].blocks.len(), 2);
        // Cut off before it answered at all: the instruction joins the task rather than following it.
        let fresh = continue_with(vec![Message::user("task")], "and also this");
        assert_eq!(fresh.len(), 1);
        assert!(matches!(&fresh[0].blocks[1], Block::Text { text } if text == "and also this"));
        assert_eq!(continue_with(Vec::new(), "hi"), vec![Message::user("hi")]);
    }
}
