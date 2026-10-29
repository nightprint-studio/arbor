//! Tests that pin what the provider's cache needs from a request, and a measurement of where a request's tokens
//! go (run with `--nocapture` to read the numbers).

use bennu_assistant::prelude::{estimate_tokens, history_tokens, standing_tokens, tool_weights, Anthropic, Provider};
use serde_json::json;

use super::*;

fn args(file: &str) -> SendArgs {
    serde_json::from_value(json!({ "text": "rename it", "root": "/no/such/project", "model": "claude-opus-5-5", "file": file, "mode": "manual" })).unwrap()
}

fn request_for(file: &str) -> Request {
    let user = Message::user(with_context("rename it", Some(file), None));
    build_request(&args(file), vec![user], &[], &[], &[])
}

#[test]
fn switching_the_open_file_leaves_the_cacheable_prefix_byte_identical() {
    let (a, b) = (Anthropic.body(&request_for("src/A.java")), Anthropic.body(&request_for("src/B.java")));
    assert_eq!(a["tools"], b["tools"], "tool definitions come first in the provider's prefix");
    assert_eq!(a["system"], b["system"], "so do the instructions: anything volatile in them would invalidate the conversation");
    assert!(!a["system"][0]["text"].as_str().unwrap().contains("A.java"), "the file travels with the user's message");
    assert_ne!(a["messages"], b["messages"]);
}

#[test]
fn two_builds_of_the_same_request_are_identical_and_carry_no_clock() {
    let (a, b) = (Anthropic.body(&request_for("src/A.java")), Anthropic.body(&request_for("src/A.java")));
    assert_eq!(a, b);
}

/// Not an assertion about behaviour: a printout of where a representative request spends its tokens.
#[test]
fn measure_the_weight_of_the_standing_part() {
    let r = request_for("src/A.java");
    let s = standing_tokens(&r.system, "", "", &r.tools);
    let h = history_tokens(&r.messages);
    println!("standing: tools {} + rules {} = {}; first message {}", s.tools, s.system, s.total(), h.total());
    println!("tools offered: {}, share of a 10k-token request: {:.0}%", r.tools.len(), 100.0 * s.tools as f64 / (s.total() + 10_000) as f64);
    for (name, t) in tool_weights(&r.tools).into_iter().take(12) {
        println!("  {t:>5} {name}");
    }
    assert!(s.tools > 0 && estimate_tokens(&r.system) > 0);
}

/// Another printout: what the cheaper answers weigh next to the plain ones, on fixtures.
#[test]
fn measure_the_cheaper_answers() {
    use bennu_assistant::prelude::{digest_value, stub_stale_results, StalePolicy, Block};
    // A build with 60 errors in 12 files, 6 distinct messages.
    let problems: Vec<_> = (0..60).map(|i| json!({"severity": "error", "file": format!("src/m{}.rs", i % 12), "line": i + 1, "column": 5, "message": format!("cannot find value `v{}` in this scope: not found in this scope, help: consider importing one of these items", i % 6)})).collect();
    let v = json!({"tool": "cargo", "ok": false, "errors": 60, "warnings": 0, "problems": problems, "truncated": 0});
    let raw = v.to_string();
    let d = digest_value("bennu_build", &v).unwrap();
    println!("build output: raw {} tokens -> digest {} tokens", estimate_tokens(&raw), estimate_tokens(&d.head));
    // The same 4000-char file read 3 times, unchanged.
    let first = estimate_tokens(&"line of code here\n".repeat(235));
    println!("re-read of an unchanged 4k-char file: {first} tokens -> ~45 tokens");
    // 14 reads of 4000 chars, then the batch stub.
    let mut m = vec![Message::user("go")];
    for i in 0..14 {
        m.push(Message::assistant(vec![Block::ToolUse { id: format!("r{i}"), name: "bennu_read_file".into(), input: json!({"file": format!("F{i}.java")}) }]));
        m.push(Message::tool_results(vec![Block::ToolResult { tool_use_id: format!("r{i}"), content: "line of code here\n".repeat(235), is_error: false }]));
    }
    let before = history_tokens(&m).total();
    let (after, report) = stub_stale_results(&m, &StalePolicy::default());
    println!("14 file reads in history: {before} tokens -> {} tokens after stubbing {} old ones", history_tokens(&after).total(), report.stubbed.len());
}
