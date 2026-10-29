//! Where a request's tokens go, for the preview and the context meter.
//!
//! The arithmetic is `bennu_assistant::tokens`; this module knows the project: which part of the system text is
//! the project's instruction files and which is the skills index (so each can be shown on its own), and how the
//! numbers are put on the wire. Every figure is an estimate, and the panel says so; the provider's own count for
//! the last request is shown beside it.

use std::path::Path;

use bennu_assistant::prelude::{history_tokens, standing_tokens, token_parts, Message, Request, ToolSpec, Usage};
use serde_json::{json, Value};

use super::skills::{self, Skill};
use super::store::Context;
use super::{estimate_context, instructions};

/// The project's instruction files, rendered, and the skills index — the two texts the system prompt carries
/// that the user controls. Empty when instructions are off or no skill is enabled.
pub(super) fn standing_texts(use_instructions: bool, root: &str, skills: &[Skill]) -> (String, String) {
    let rendered = if use_instructions { instructions::render(&instructions::load(Path::new(root))) } else { String::new() };
    (rendered, skills::render_index(skills))
}

/// The tokens of a finished conversation by part, for the meter. The instruction files and the skills index
/// are counted only if they are really inside `system` (a chat, or a run with instructions off, has none).
pub(super) fn context_of(system: &str, root: &str, skills_index: &str, tools: &[ToolSpec], transcript: &[Message]) -> Context {
    let (instr, _) = standing_texts(true, root, &[]);
    let inside = |t: &str| if !t.is_empty() && system.contains(t) { t.to_string() } else { String::new() };
    let standing = standing_tokens(system, &inside(&instr), &inside(skills_index), tools);
    Context { tools: standing.tools, instructions: standing.instructions, skills: standing.skills, system: standing.total(), ..estimate_context("", &[], transcript) }
}

/// The parts of a request, in the order the provider reads them, for the preview.
pub(super) fn preview_parts(request: &Request, instructions: &str, skills: &str) -> Vec<Value> {
    let standing = standing_tokens(&request.system, instructions, skills, &request.tools);
    token_parts(&standing, &history_tokens(&request.messages))
        .into_iter()
        .map(|p| json!({ "id": p.id, "label": p.label, "tokens": p.tokens }))
        .collect()
}

/// The meter's numbers as the panel receives them.
pub(super) fn context_json(c: &Context) -> Value {
    json!({
        "system": c.system, "project": c.project, "conversation": c.conversation,
        "tools": c.tools, "instructions": c.instructions, "skills": c.skills,
    })
}

/// What the provider billed for one request, as the panel shows it: uncached input, cache reads and writes.
pub(super) fn usage_json(u: &Usage) -> Value {
    json!({
        "input": u.input_tokens, "output": u.output_tokens,
        "cacheRead": u.cache_read_tokens, "cacheWrite": u.cache_write_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_assistant::prelude::Block;

    #[test]
    fn the_context_is_split_into_what_stands_and_what_grows() {
        let transcript = vec![
            Message::user("x".repeat(400)),
            Message { role: bennu_assistant::prelude::Role::User, blocks: vec![Block::ToolResult { tool_use_id: "t".into(), content: "y".repeat(800), is_error: false }] },
        ];
        let tools = vec![ToolSpec { name: "n".into(), description: "d".repeat(100), input_schema: json!({}) }];
        let c = context_of("the rules", "/no/such/project", "", &tools, &transcript);
        assert_eq!((c.conversation, c.project), (100, 200));
        assert!(c.tools > 25 && c.instructions == 0 && c.skills == 0);
        assert_eq!(c.system, c.tools + tokens_of("the rules"), "the standing part is the tools and the rules");
    }

    fn tokens_of(s: &str) -> u64 {
        bennu_assistant::prelude::estimate_tokens(s)
    }

    #[test]
    fn the_preview_lists_the_parts_in_the_order_the_provider_reads_them() {
        let request = Request {
            model: "m".into(),
            system: format!("rules{}", "i".repeat(40)),
            messages: vec![Message::user("hello there")],
            tools: vec![ToolSpec { name: "n".into(), description: "d".into(), input_schema: json!({}) }],
            max_tokens: 10,
            effort: None,
            long_cache: false,
        };
        let parts = preview_parts(&request, &"i".repeat(40), "");
        let ids: Vec<&str> = parts.iter().map(|p| p["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["tools", "system", "instructions", "text"]);
        assert_eq!(parts[2]["tokens"], 10);
    }

    #[test]
    fn the_usage_travels_with_the_cache_figures() {
        let u = Usage { input_tokens: 10, output_tokens: 5, cache_read_tokens: 900, cache_write_tokens: 90, cache_write_1h_tokens: 0 };
        assert_eq!(usage_json(&u), json!({ "input": 10, "output": 5, "cacheRead": 900, "cacheWrite": 90 }));
    }
}
