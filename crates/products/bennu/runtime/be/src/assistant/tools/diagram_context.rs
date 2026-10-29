//! What the model keeps of the diagrams it drew.
//!
//! Getting a Mermaid source right can take a few tries, and every try is a call, an error message and a
//! corrected call — a lot of text that says nothing once the diagram is on screen. It would be sent with
//! every later request of the conversation. So when a `show_diagram` has gone through, the attempts that
//! failed before it are taken out of what is **sent** (the thread the user reads is untouched), and the one
//! that worked is kept in a short form.
//!
//! Two decisions. A failed attempt with no success after it stays whole: the model is in the middle of
//! fixing it and needs its source and its error. And a successful source is kept when it is small (a
//! follow-up like "add the cache to the diagram" needs it) and replaced by a one-line note when it is not.

use std::collections::{HashMap, HashSet};

use bennu_assistant::prelude::{Block, Message};
use serde_json::{json, Value};

use super::diagram::NAME;

/// The longest source kept word for word, in characters — about three hundred tokens.
const KEEP_SOURCE_CHARS: usize = 1_200;

/// The conversation as it is sent: the failed `show_diagram` calls that a later one made good for removed,
/// call and result together, and the successful ones in their short form. Everything else is as it was, and
/// every result that remains still has its call.
pub(super) fn prune_diagram_attempts(messages: &[Message]) -> Vec<Message> {
    let (dropped, kept) = classify(messages);
    if dropped.is_empty() && kept.is_empty() {
        return messages.to_vec();
    }
    let mut out: Vec<Message> = Vec::with_capacity(messages.len());
    // Something was removed since the last message pushed: two neighbours of one role may need joining.
    let mut gap = false;
    for m in messages {
        let blocks: Vec<Block> = m
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::ToolUse { id, .. } if dropped.contains(id.as_str()) => None,
                Block::ToolResult { tool_use_id, .. } if dropped.contains(tool_use_id.as_str()) => None,
                Block::ToolUse { id, name, input } if kept.contains(id.as_str()) => {
                    Some(Block::ToolUse { id: id.clone(), name: name.clone(), input: compact_input(input) })
                }
                other => Some(other.clone()),
            })
            .collect();
        if blocks.is_empty() {
            gap = true;
            continue;
        }
        gap |= blocks.len() != m.blocks.len();
        match out.last_mut() {
            // A provider wants roles to alternate: what is left of a turn joins the turn before it.
            Some(last) if gap && last.role == m.role => last.blocks.extend(blocks),
            _ => out.push(Message { role: m.role, blocks }),
        }
        gap = false;
    }
    out
}

/// The ids of the calls to remove, and of the successful ones to shorten. A call with no result (the run
/// ended under it) is left alone.
fn classify(messages: &[Message]) -> (HashSet<&str>, HashSet<&str>) {
    let mut failed: HashMap<&str, bool> = HashMap::new();
    for m in messages {
        for b in &m.blocks {
            if let Block::ToolResult { tool_use_id, is_error, .. } = b {
                failed.insert(tool_use_id.as_str(), *is_error);
            }
        }
    }
    // Every answered call, in the order it was made.
    let calls: Vec<(&str, bool)> = messages
        .iter()
        .flat_map(|m| m.tool_uses())
        .filter(|(_, name, _)| *name == NAME)
        .filter_map(|(id, _, _)| failed.get(id).map(|f| (id, *f)))
        .collect();
    let last_success = calls.iter().rposition(|(_, f)| !f);
    let dropped = calls.iter().enumerate().filter(|(i, (_, f))| *f && last_success.is_some_and(|s| *i < s)).map(|(_, (id, _))| *id).collect();
    let kept = calls.iter().filter(|(_, f)| !f).map(|(id, _)| *id).collect();
    (dropped, kept)
}

/// The call as it stays in the history: the title, and the source when it is small enough to be worth keeping.
fn compact_input(input: &Value) -> Value {
    let source = input.get("mermaid").and_then(Value::as_str).unwrap_or("");
    let mut out = json!({});
    if let Some(title) = input.get("title").filter(|t| t.is_string()) {
        out["title"] = title.clone();
    }
    out["mermaid"] = if source.chars().count() <= KEEP_SOURCE_CHARS {
        json!(source)
    } else {
        json!(format!("(source omitted: {} lines, already shown to the user)", source.lines().count()))
    };
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_assistant::prelude::Role;

    fn call(id: &str, name: &str, input: Value) -> Message {
        Message::assistant(vec![Block::ToolUse { id: id.into(), name: name.into(), input }])
    }
    fn answer(id: &str, content: &str, is_error: bool) -> Message {
        Message::tool_results(vec![Block::ToolResult { tool_use_id: id.into(), content: content.into(), is_error }])
    }
    fn diagram(id: &str, src: &str) -> Message {
        call(id, NAME, json!({ "title": "Flow", "mermaid": src }))
    }

    /// Every result has its call before it, and no two neighbours share a role.
    fn assert_valid(messages: &[Message]) {
        let mut seen: HashSet<String> = HashSet::new();
        for (i, m) in messages.iter().enumerate() {
            assert!(!m.blocks.is_empty(), "an empty message at {i}");
            if i > 0 {
                assert_ne!(messages[i - 1].role, m.role, "roles must alternate at {i}");
            }
            for b in &m.blocks {
                match b {
                    Block::ToolUse { id, .. } => {
                        seen.insert(id.clone());
                    }
                    Block::ToolResult { tool_use_id, .. } => assert!(seen.contains(tool_use_id), "orphan result for {tool_use_id}"),
                    Block::Text { .. } | Block::ToolImage { .. } => {}
                }
            }
        }
    }

    fn ids_of(messages: &[Message]) -> Vec<String> {
        messages.iter().flat_map(|m| m.tool_uses().map(|(id, _, _)| id.to_string()).collect::<Vec<_>>()).collect()
    }

    #[test]
    fn the_attempts_that_failed_before_a_success_are_removed_call_and_result_together() {
        let history = vec![
            Message::user("draw the flow"),
            diagram("a", "graph ((("),
            answer("a", "The diagram did not parse: line 1", true),
            diagram("b", "flowchart TD\n A-->"),
            answer("b", "The diagram did not parse: line 2", true),
            diagram("c", "flowchart TD\n A-->B"),
            answer("c", "Diagram shown to the user.", false),
            Message::assistant(vec![Block::Text { text: "There it is.".into() }]),
        ];
        let out = prune_diagram_attempts(&history);
        assert_valid(&out);
        assert_eq!(ids_of(&out), vec!["c"]);
        assert_eq!(out.len(), 4, "the request, the good call, its result, the closing text");
    }

    #[test]
    fn a_failure_with_no_success_after_it_stays_whole_so_it_can_be_fixed() {
        let history = vec![Message::user("draw"), diagram("a", "graph ((("), answer("a", "did not parse: line 1", true)];
        assert_eq!(prune_diagram_attempts(&history), history);
    }

    #[test]
    fn a_failure_after_the_last_success_is_kept() {
        let history = vec![
            Message::user("draw"),
            diagram("a", "flowchart TD\n A-->B"),
            answer("a", "Diagram shown to the user.", false),
            Message::assistant(vec![Block::Text { text: "Done.".into() }]),
            Message::user("now add C"),
            diagram("b", "flowchart TD\n A-->"),
            answer("b", "did not parse: line 2", true),
        ];
        let out = prune_diagram_attempts(&history);
        assert_valid(&out);
        assert_eq!(ids_of(&out), vec!["a", "b"]);
    }

    #[test]
    fn a_small_source_is_kept_and_a_large_one_becomes_a_note() {
        let big = format!("flowchart TD\n{}", " A-->B\n".repeat(400));
        let history = vec![
            Message::user("draw"),
            diagram("a", "flowchart TD\n A-->B"),
            answer("a", "Diagram shown to the user.", false),
            diagram("b", &big),
            answer("b", "Diagram shown to the user.", false),
        ];
        let out = prune_diagram_attempts(&history);
        assert_valid(&out);
        let inputs: Vec<&Value> = out.iter().flat_map(|m| m.tool_uses().map(|(_, _, i)| i).collect::<Vec<_>>()).collect();
        assert_eq!(inputs[0]["mermaid"], "flowchart TD\n A-->B");
        assert_eq!(inputs[0]["title"], "Flow");
        assert!(inputs[1]["mermaid"].as_str().unwrap().starts_with("(source omitted: 401 lines"));
    }

    #[test]
    fn other_tools_and_the_text_around_the_failures_are_not_touched() {
        let history = vec![
            Message::user("look at A, then draw it"),
            call("r", "search_text", json!({"pattern": "A"})),
            answer("r", "3 matches", false),
            Message::assistant(vec![
                Block::Text { text: "Let me draw it.".into() },
                Block::ToolUse { id: "a".into(), name: NAME.into(), input: json!({"mermaid": "bad"}) },
            ]),
            answer("a", "did not parse", true),
            diagram("b", "flowchart TD\n A-->B"),
            answer("b", "Diagram shown to the user.", false),
        ];
        let out = prune_diagram_attempts(&history);
        assert_valid(&out);
        assert_eq!(ids_of(&out), vec!["r", "b"]);
        assert_eq!(out[0], history[0]);
        assert_eq!(out[1], history[1]);
        assert_eq!(out[2], history[2]);
        // The text that came with the failed call joins the turn that made the good one.
        assert_eq!(out[3].role, Role::Assistant);
        assert!(matches!(&out[3].blocks[0], Block::Text { text } if text == "Let me draw it."));
        assert!(matches!(&out[3].blocks[1], Block::ToolUse { id, .. } if id == "b"));
    }

    #[test]
    fn a_result_shared_with_other_tools_loses_only_the_failed_one() {
        let history = vec![
            Message::user("go"),
            Message::assistant(vec![
                Block::ToolUse { id: "a".into(), name: NAME.into(), input: json!({"mermaid": "bad"}) },
                Block::ToolUse { id: "r".into(), name: "search_text".into(), input: json!({"pattern": "x"}) },
            ]),
            Message::tool_results(vec![
                Block::ToolResult { tool_use_id: "a".into(), content: "did not parse".into(), is_error: true },
                Block::ToolResult { tool_use_id: "r".into(), content: "1 match".into(), is_error: false },
            ]),
            diagram("b", "flowchart TD\n A-->B"),
            answer("b", "Diagram shown to the user.", false),
        ];
        let out = prune_diagram_attempts(&history);
        assert_valid(&out);
        assert_eq!(ids_of(&out), vec!["r", "b"]);
        assert_eq!(out[2].blocks.len(), 1);
    }

    #[test]
    fn a_history_without_diagrams_is_returned_as_it_is_and_pruning_twice_changes_nothing() {
        let plain = vec![Message::user("hi"), call("r", "search_text", json!({})), answer("r", "ok", false)];
        assert_eq!(prune_diagram_attempts(&plain), plain);
        let history = vec![
            Message::user("draw"),
            diagram("a", "bad"),
            answer("a", "did not parse", true),
            diagram("b", "flowchart TD\n A-->B"),
            answer("b", "Diagram shown to the user.", false),
        ];
        let once = prune_diagram_attempts(&history);
        assert_eq!(prune_diagram_attempts(&once), once);
    }
}
