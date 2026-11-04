//! Old tool results: what the model read many turns ago, and no longer needs word for word.
//!
//! A file read, a search, a listing is the bulk of a long conversation, and after a few turns the
//! conclusions drawn from it are in the assistant's own text. Each such result is replaced by a one-line
//! stub that says what it was and how to fetch it again — the call is untouched, so every result still
//! pairs with its use.
//!
//! **Only in batches.** The provider caches the conversation as a prefix: changing a message invalidates
//! the cache from that message on, so stubbing one result per request would pay a cache rewrite every
//! time and save nothing. The host asks [`maybe_stub`], which does nothing until the stale bytes are worth
//! one rewrite ([`StalePolicy::trigger_tokens`] and a share of the history), then stubs them all at once;
//! afterwards the history is stable again and the cache is rebuilt once, in the next request.
//!
//! Pure: a function over the message list, idempotent (a stub is small and marked, so it is never
//! stubbed again).

use std::collections::HashMap;

use serde_json::Value;

use crate::message::{Block, Message, Role};
use crate::tokens::estimate;

/// Starts every stub. Also how a stub is recognised, so applying the function twice changes nothing.
pub const STUB_PREFIX: &str = "[result dropped to save room";

/// When and what to stub.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StalePolicy {
    /// Results of the last this-many assistant turns are kept whole.
    pub keep_turns: usize,
    /// A result shorter than this (characters) is not worth a stub.
    pub min_chars: usize,
    /// [`maybe_stub`] acts only once this many tokens would be freed …
    pub trigger_tokens: u64,
    /// … and that is at least this share (0.0 to 1.0) of the whole history.
    pub trigger_share: f64,
}

impl Default for StalePolicy {
    fn default() -> Self {
        Self { keep_turns: 6, min_chars: 1_500, trigger_tokens: 12_000, trigger_share: 0.25 }
    }
}

/// What a stubbing pass did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StaleReport {
    /// The ids of the calls whose result became a stub — whoever remembers "you already have it above" for
    /// one of them must forget it.
    pub stubbed: Vec<String>,
    pub freed_tokens: u64,
}

/// How to get a tool's output back, or `None` when the output cannot be reproduced (it depends on the
/// moment, on the user, or on a side effect) and must stay in the conversation.
fn refetch_hint(tool: &str) -> Option<&'static str> {
    const KEEP_EXACT: [&str; 12] = [
        "ask_user", "update_plan", "load_skill", "show_diagram", "open_ui", "http_request", "trace_request", "wait_ready", "timeline", "page_diagnostics", "bennu_test_run", "bennu_build",
    ];
    const KEEP_PREFIX: [&str; 13] = [
        "run_", "debug_", "brp_", "screenshot", "edit_", "create_", "delete_", "replace_", "rename_", "add_", "structural_", "corvus_", "spawn_",
    ];
    if KEEP_EXACT.contains(&tool) || KEEP_PREFIX.iter().any(|p| tool.starts_with(p)) {
        return None;
    }
    Some("call the tool again with the same arguments to read it")
}

pub fn is_stub(content: &str) -> bool {
    content.starts_with(STUB_PREFIX)
}

fn stub_text(tool: &str, input: &Value, content: &str, hint: &str) -> String {
    let args = input.to_string();
    let args: String = if args.chars().count() > 160 { format!("{}…", args.chars().take(160).collect::<String>()) } else { args };
    format!("{STUB_PREFIX}: {tool} {args} returned ~{} lines, ~{} tokens; {hint}]", content.lines().count(), estimate(content))
}

/// The call that each result answers, by id.
fn calls_by_id(messages: &[Message]) -> HashMap<&str, (&str, &Value)> {
    messages.iter().flat_map(Message::tool_uses).map(|(id, name, input)| (id, (name, input))).collect()
}

/// The index of the first message whose results are kept whole.
fn cutoff(messages: &[Message], keep_turns: usize) -> usize {
    let assistant_at: Vec<usize> = messages.iter().enumerate().filter(|(_, m)| m.role == Role::Assistant).map(|(i, _)| i).collect();
    if assistant_at.len() > keep_turns { assistant_at[assistant_at.len() - keep_turns] } else { 0 }
}

/// A result that would become a stub: its message index, block index and the stub.
fn candidates(messages: &[Message], policy: &StalePolicy) -> Vec<(usize, usize, String, String, u64)> {
    let calls = calls_by_id(messages);
    let end = cutoff(messages, policy.keep_turns);
    let mut found = Vec::new();
    for (mi, m) in messages.iter().enumerate().take(end) {
        for (bi, b) in m.blocks.iter().enumerate() {
            let Block::ToolResult { tool_use_id, content, is_error: false } = b else { continue };
            if content.chars().count() < policy.min_chars || is_stub(content) {
                continue;
            }
            let Some((name, input)) = calls.get(tool_use_id.as_str()) else { continue };
            let Some(hint) = refetch_hint(name) else { continue };
            let stub = stub_text(name, input, content, hint);
            let freed = estimate(content).saturating_sub(estimate(&stub));
            found.push((mi, bi, tool_use_id.clone(), stub, freed));
        }
    }
    found
}

/// How many tokens a stubbing pass would free right now.
pub fn stale_tokens(messages: &[Message], policy: &StalePolicy) -> u64 {
    candidates(messages, policy).iter().map(|c| c.4).sum()
}

/// Stub every stale result, whatever the thresholds. Returns the new list and what it did.
pub fn stub_stale_results(messages: &[Message], policy: &StalePolicy) -> (Vec<Message>, StaleReport) {
    let found = candidates(messages, policy);
    let mut out = messages.to_vec();
    let mut report = StaleReport::default();
    for (mi, bi, id, stub, freed) in found {
        if let Block::ToolResult { content, .. } = &mut out[mi].blocks[bi] {
            *content = stub;
        }
        report.stubbed.push(id);
        report.freed_tokens += freed;
    }
    (out, report)
}

/// Stub the stale results if, and only if, doing it once is worth a cache rewrite: enough tokens, and a big
/// enough share of the history. `None` leaves the conversation byte-for-byte as it was.
pub fn maybe_stub(messages: &[Message], policy: &StalePolicy) -> Option<(Vec<Message>, StaleReport)> {
    let freed = stale_tokens(messages, policy);
    let history = crate::tokens::history_tokens(messages).total();
    if freed < policy.trigger_tokens || (freed as f64) < policy.trigger_share * history as f64 {
        return None;
    }
    Some(stub_stale_results(messages, policy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str, name: &str) -> Message {
        Message::assistant(vec![Block::ToolUse { id: id.into(), name: name.into(), input: json!({"file": format!("{id}.java")}) }])
    }
    fn answer(id: &str, size: usize) -> Message {
        Message::tool_results(vec![Block::ToolResult { tool_use_id: id.into(), content: "line\n".repeat(size / 5), is_error: false }])
    }
    fn conversation(reads: usize, size: usize) -> Vec<Message> {
        let mut m = vec![Message::user("go")];
        for i in 0..reads {
            m.push(call(&format!("r{i}"), "bennu_read_file"));
            m.push(answer(&format!("r{i}"), size));
        }
        m
    }
    fn contents(m: &[Message]) -> Vec<String> {
        m.iter().flat_map(|m| m.blocks.iter()).filter_map(|b| if let Block::ToolResult { content, .. } = b { Some(content.clone()) } else { None }).collect()
    }
    fn small() -> StalePolicy {
        StalePolicy { keep_turns: 2, min_chars: 100, trigger_tokens: 100, trigger_share: 0.1 }
    }

    #[test]
    fn old_big_results_become_a_stub_that_says_how_to_read_them_again() {
        let (out, report) = stub_stale_results(&conversation(5, 4_000), &small());
        let c = contents(&out);
        assert_eq!(c.iter().filter(|c| is_stub(c)).count(), 3, "all but the last two turns");
        assert!(c[0].contains("bennu_read_file") && c[0].contains("r0.java") && c[0].contains("call the tool again"));
        assert_eq!(report.stubbed, ["r0", "r1", "r2"]);
        assert!(report.freed_tokens > 2_000);
        assert_eq!(c[4].len(), 4_000 / 5 * 5, "the recent ones stay whole");
    }

    #[test]
    fn every_result_still_pairs_with_its_call() {
        let (out, _) = stub_stale_results(&conversation(5, 4_000), &small());
        assert_eq!(out.len(), conversation(5, 4_000).len());
        let uses: Vec<String> = out.iter().flat_map(Message::tool_uses).map(|(id, _, _)| id.to_string()).collect();
        let results: Vec<String> = out.iter().flat_map(|m| m.blocks.iter()).filter_map(|b| if let Block::ToolResult { tool_use_id, .. } = b { Some(tool_use_id.clone()) } else { None }).collect();
        assert_eq!(uses, results);
    }

    #[test]
    fn applying_it_twice_changes_nothing() {
        let (once, _) = stub_stale_results(&conversation(6, 4_000), &small());
        let (twice, report) = stub_stale_results(&once, &small());
        assert_eq!(once, twice);
        assert!(report.stubbed.is_empty() && report.freed_tokens == 0);
    }

    #[test]
    fn small_results_errors_and_results_that_cannot_be_reproduced_stay() {
        let mut m = vec![Message::user("go"), call("a", "bennu_read_file"), answer("a", 40)];
        m.push(call("b", "run_logs"));
        m.push(answer("b", 4_000));
        m.push(call("c", "ask_user"));
        m.push(answer("c", 4_000));
        m.push(call("d", "edit_file"));
        m.push(answer("d", 4_000));
        m.extend(conversation(3, 90).into_iter().skip(1));
        let (out, report) = stub_stale_results(&m, &small());
        assert!(report.stubbed.is_empty(), "{:?}", report.stubbed);
        assert_eq!(out, m);
    }

    #[test]
    fn it_waits_until_one_rewrite_is_worth_it() {
        let policy = StalePolicy { keep_turns: 2, min_chars: 100, trigger_tokens: 5_000, trigger_share: 0.25 };
        assert!(maybe_stub(&conversation(4, 4_000), &policy).is_none(), "~1.6k stale tokens: not yet");
        let (_, report) = maybe_stub(&conversation(14, 4_000), &policy).expect("~9.6k stale tokens of 14k history: now");
        assert_eq!(report.stubbed.len(), 12);
        // A history that is mostly recent has little stale: the share rule keeps the cache untouched.
        let mut recent = conversation(2, 40_000);
        recent.splice(1..1, conversation(3, 4_000).into_iter().skip(1));
        assert!(maybe_stub(&recent, &StalePolicy { trigger_tokens: 100, ..policy }).is_none());
    }

    #[test]
    fn a_short_conversation_has_nothing_stale() {
        assert_eq!(stale_tokens(&conversation(2, 4_000), &small()), 0);
    }
}
