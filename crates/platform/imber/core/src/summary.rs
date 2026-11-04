//! The note that replaces the early part of a long conversation.
//!
//! A free-form summary loses what matters in unpredictable ways. This one has fixed sections the model
//! must fill — what the user wants, decisions and why, open questions, preferences — and two parts that
//! are **not left to the model at all** because they are facts it would only copy: the files the
//! conversation changed (read off the edit calls) and the current step plan (the last `update_plan`,
//! verbatim). The note also says, in the thread, what was dropped, so a compaction is never silent.
//!
//! Also when to do it: not only when the window is nearly full (compacting under pressure is the worst
//! moment, and every request until then carried the bulk), but at a natural boundary — the end of a run,
//! the user about to type the next thing — once the conversation is past [`BOUNDARY_SHARE`] of the window.
//!
//! Pure prompt and text code over the neutral messages; the request and the model are the host's.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use crate::message::{Block, Message};
use crate::tokens::estimate;

/// Past this share of the window, the end of a run is a good moment to compact.
pub const BOUNDARY_SHARE: f64 = 0.60;
/// Compacting a conversation shorter than this saves too little to be worth a request.
pub const MIN_TOKENS_TO_COMPACT: u64 = 20_000;

/// Whether the end of a run is the moment: the window is configured, and the conversation is past the share.
pub fn due_at_boundary(used_tokens: u64, max_context: u64) -> bool {
    max_context > 0 && used_tokens >= MIN_TOKENS_TO_COMPACT && (used_tokens as f64) >= BOUNDARY_SHARE * max_context as f64
}

/// The instructions to the model that writes the note.
pub const SYSTEM: &str = "You write the note that replaces the early part of a long conversation between a developer and a coding assistant, so the conversation can go on in less room. Be factual and dense; no preamble, no pleasantries. Use exactly these sections, in this order, as Markdown headings, and write `- none` under one that has nothing:\n\n## Goal\nWhat the user wants overall, in a few lines.\n## Decisions\nWhat was decided and why, one bullet each. Include approaches that were tried and rejected, so they are not tried again.\n## State\nWhat is done and what is half done, with the symbols, names and paths that matter. Do not list the files that were changed: a list is added after your note from the edit record.\n## Open questions and to-do\nWhat is still unknown or still to do, in the order it should be done.\n## User preferences\nHow the user wants the work done: style, tools to avoid, things they corrected.\n\nDo not copy tool output or file contents; the assistant can read them again. Do not write a plan: the current plan is kept separately, word for word.";

/// The calls that change files, and what to call what they did.
const FILE_ACTIONS: [(&str, &str); 6] = [
    ("edit_file", "edited"),
    ("create_file", "created"),
    ("delete_file", "deleted"),
    ("rename_symbol", "renamed a symbol in"),
    ("add_parameter", "added a parameter in"),
    ("structural_replace", "rewrote by pattern in"),
];

/// The files the conversation changed (or proposed to change), with what was done, in the order they were
/// first touched. Calls that failed are left out.
pub fn files_touched(messages: &[Message]) -> Vec<(String, String)> {
    let failed: HashSet<&str> = messages
        .iter()
        .flat_map(|m| m.blocks.iter())
        .filter_map(|b| if let Block::ToolResult { tool_use_id, is_error: true, .. } = b { Some(tool_use_id.as_str()) } else { None })
        .collect();
    let mut order: Vec<String> = Vec::new();
    let mut actions: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    let mut note = |file: &str, action: &'static str| {
        if file.is_empty() {
            return;
        }
        if !actions.contains_key(file) {
            order.push(file.to_string());
        }
        let list = actions.entry(file.to_string()).or_default();
        if !list.contains(&action) {
            list.push(action);
        }
    };
    for m in messages {
        for (_, name, input) in m.tool_uses().filter(|(id, _, _)| !failed.contains(id)) {
            if name == "edit_files" {
                for e in input.get("edits").and_then(Value::as_array).into_iter().flatten() {
                    note(e.get("file").and_then(Value::as_str).unwrap_or(""), "edited");
                }
            } else if name == "delete_file" {
                for p in input.get("paths").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    note(p, "deleted");
                }
            } else if let Some((_, action)) = FILE_ACTIONS.iter().find(|(n, _)| *n == name) {
                note(input.get("file").and_then(Value::as_str).unwrap_or(""), action);
            }
        }
    }
    order.into_iter().map(|f| { let what = actions.remove(&f).unwrap_or_default().join(", "); (f, what) }).collect()
}

/// The step plan as the model last sent it, one line per step, or `None` when there never was one.
pub fn current_plan(messages: &[Message]) -> Option<String> {
    let steps = messages.iter().flat_map(Message::tool_uses).filter(|(_, name, _)| *name == "update_plan").last()?.2.get("steps")?.as_array()?;
    let lines: Vec<String> = steps
        .iter()
        .map(|s| {
            let mark = match s.get("status").and_then(Value::as_str).unwrap_or("pending") {
                "completed" => "[x]",
                "in_progress" => "[~]",
                _ => "[ ]",
            };
            format!("- {mark} {}", s.get("text").and_then(Value::as_str).unwrap_or(""))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The full note: what the model wrote, then the two mechanical parts. The files are those of the part being
/// folded (`older`); the plan is the newest one anywhere in the conversation, the part that stays (`kept`)
/// included, so a plan updated after the fold point is not contradicted by an older one in the note.
pub fn compose_note(model_text: &str, older: &[Message], kept: &[Message]) -> String {
    let mut note = model_text.trim().to_string();
    let files = files_touched(older);
    if !files.is_empty() {
        note.push_str("\n\n## Files changed (from the edit record)\n");
        note.push_str(&files.iter().map(|(f, what)| format!("- {f} — {what}")).collect::<Vec<_>>().join("\n"));
    }
    let everything: Vec<Message> = older.iter().chain(kept).cloned().collect();
    if let Some(plan) = current_plan(&everything) {
        note.push_str("\n\n## Current plan (verbatim)\n");
        note.push_str(&plan);
    }
    note
}

/// How many tool results of each kind the folded part held: what the note does not carry.
fn dropped_results(older: &[Message]) -> Vec<(String, usize)> {
    let names: BTreeMap<&str, &str> = older.iter().flat_map(Message::tool_uses).map(|(id, name, _)| (id, name)).collect();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for b in older.iter().flat_map(|m| m.blocks.iter()) {
        if let Block::ToolResult { tool_use_id, .. } = b {
            *counts.entry(names.get(tool_use_id.as_str()).copied().unwrap_or("tool").to_string()).or_default() += 1;
        }
    }
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    v
}

/// What to tell the user was dropped. Plain text for the notice in the thread.
pub fn dropped_report(older: &[Message], note: &str) -> String {
    let before: u64 = older.iter().map(|m| m.blocks.iter().map(block_tokens).sum::<u64>()).sum();
    let after = estimate(note);
    let results = dropped_results(older);
    let kinds = if results.is_empty() {
        String::new()
    } else {
        let top: Vec<String> = results.iter().take(4).map(|(n, c)| format!("{c} × {n}")).collect();
        format!(" The output of {} tool calls ({}) is not kept; the assistant reads it again when it needs it.", results.iter().map(|(_, c)| c).sum::<usize>(), top.join(", "))
    };
    format!("Conversation compacted: {} messages (~{before} tokens) became a note of ~{after} tokens.{kinds} Kept: the goal, decisions, state, open questions, your preferences, the files changed and the current plan.", older.len())
}

fn block_tokens(b: &Block) -> u64 {
    match b {
        Block::Text { text } => estimate(text),
        Block::ToolUse { input, .. } => estimate(&input.to_string()),
        Block::ToolResult { content, .. } => estimate(content),
        Block::ToolImage { tokens, .. } => u64::from(*tokens),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str, name: &str, input: Value) -> Message {
        Message::assistant(vec![Block::ToolUse { id: id.into(), name: name.into(), input }])
    }
    fn ok(id: &str, text: &str) -> Message {
        Message::tool_results(vec![Block::ToolResult { tool_use_id: id.into(), content: text.into(), is_error: false }])
    }
    fn failed(id: &str) -> Message {
        Message::tool_results(vec![Block::ToolResult { tool_use_id: id.into(), content: "no".into(), is_error: true }])
    }

    #[test]
    fn it_is_due_past_the_share_of_the_window_and_only_with_a_window() {
        assert!(!due_at_boundary(50_000, 0), "no window, no automatic compaction");
        assert!(!due_at_boundary(50_000, 200_000), "25%");
        assert!(due_at_boundary(130_000, 200_000), "65%");
        assert!(!due_at_boundary(10_000, 12_000), "past the share, but too small to be worth a request");
    }

    #[test]
    fn the_files_changed_are_read_off_the_edit_calls_not_remembered_by_the_model() {
        let m = vec![
            call("1", "edit_file", json!({"file": "A.java", "old_text": "x", "new_text": "y"})),
            ok("1", "applied"),
            call("2", "edit_files", json!({"edits": [{"file": "B.java"}, {"file": "A.java"}]})),
            ok("2", "applied"),
            call("3", "create_file", json!({"file": "C.java", "text": ""})),
            failed("3"),
            call("4", "delete_file", json!({"paths": ["D.java", "E.java"]})),
            ok("4", "deleted"),
            call("5", "bennu_read_file", json!({"file": "F.java"})),
            ok("5", "text"),
        ];
        let files = files_touched(&m);
        let names: Vec<&str> = files.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(names, ["A.java", "B.java", "D.java", "E.java"], "first-touched order; the failed create and the read are left out");
        assert_eq!(files[0].1, "edited");
        assert_eq!(files[2].1, "deleted");
    }

    #[test]
    fn the_last_plan_is_kept_verbatim() {
        let first = json!({"steps": [{"text": "Read", "status": "in_progress"}]});
        let last = json!({"steps": [{"text": "Read", "status": "completed"}, {"text": "Fix", "status": "in_progress"}, {"text": "Test", "status": "pending"}]});
        let m = vec![call("1", "update_plan", first), ok("1", "plan updated"), call("2", "update_plan", last), ok("2", "plan updated")];
        assert_eq!(current_plan(&m).unwrap(), "- [x] Read\n- [~] Fix\n- [ ] Test");
        assert!(current_plan(&[Message::user("hi")]).is_none());
    }

    #[test]
    fn the_note_is_the_models_text_then_the_two_mechanical_parts() {
        let m = vec![call("1", "edit_file", json!({"file": "A.java"})), ok("1", "applied"), call("2", "update_plan", json!({"steps": [{"text": "Fix", "status": "pending"}]})), ok("2", "ok")];
        let note = compose_note("## Goal\nrename it\n", &m, &[]);
        assert!(note.starts_with("## Goal\nrename it"));
        assert!(note.contains("## Files changed (from the edit record)\n- A.java — edited"));
        assert!(note.ends_with("## Current plan (verbatim)\n- [ ] Fix"));
        assert_eq!(compose_note("## Goal\nx", &[Message::user("hi")], &[]), "## Goal\nx", "nothing mechanical to add");
        // A newer plan in the part that stays wins over the one being folded.
        let newer = vec![call("9", "update_plan", json!({"steps": [{"text": "Ship", "status": "in_progress"}]})), ok("9", "ok")];
        assert!(compose_note("## Goal\nx", &m, &newer).ends_with("- [~] Ship"));
    }

    #[test]
    fn the_prompt_names_every_section_and_leaves_the_plan_and_the_files_out() {
        for section in ["## Goal", "## Decisions", "## State", "## Open questions and to-do", "## User preferences"] {
            assert!(SYSTEM.contains(section), "{section}");
        }
        assert!(SYSTEM.contains("Do not write a plan") && SYSTEM.contains("added after your note"));
    }

    #[test]
    fn the_user_is_told_what_was_dropped() {
        let m = vec![
            Message::user("go"),
            call("1", "bennu_read_file", json!({})),
            ok("1", &"x".repeat(4000)),
            call("2", "bennu_read_file", json!({})),
            ok("2", &"x".repeat(4000)),
            call("3", "search_text", json!({})),
            ok("3", "hits"),
        ];
        let report = dropped_report(&m, "## Goal\nshort");
        assert!(report.starts_with("Conversation compacted: 7 messages (~"), "{report}");
        assert!(report.contains("3 tool calls (2 × bennu_read_file, 1 × search_text)") && report.contains("current plan"));
    }
}
