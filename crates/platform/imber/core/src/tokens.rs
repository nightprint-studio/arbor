//! Where a request's tokens go, estimated.
//!
//! One arithmetic for every place that shows a size — the preview of what would be sent, the context
//! meter, the compaction trigger — so they cannot disagree. It is an estimate (four characters to a
//! token, rounded up) and every caller says so; the count the provider returns with its answer is the one
//! to trust, and the host shows both side by side.
//!
//! A request is read by the provider in a fixed order — tool definitions, instructions, then the
//! conversation — and that is the order [`parts`] lists them in, because it is also the order in which the
//! provider's cache is validated: a change in one part invalidates every part after it.

use crate::message::{Block, Message, ToolSpec};

/// Four characters to a token, rounded up — close enough to steer by.
pub fn estimate(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

/// One slice of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub id: &'static str,
    pub label: &'static str,
    pub tokens: u64,
}

/// What the conversation holds, by kind of content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HistoryTokens {
    /// What the user and the assistant wrote.
    pub text: u64,
    /// The arguments of the tool calls the assistant made.
    pub tool_calls: u64,
    /// What the tools answered (file reads, searches, build output …) — usually the bulk.
    pub tool_results: u64,
    /// Pictures a tool returned.
    pub images: u64,
}

impl HistoryTokens {
    pub fn total(&self) -> u64 {
        self.text + self.tool_calls + self.tool_results + self.images
    }
}

/// The conversation split by kind of content.
pub fn history_tokens(messages: &[Message]) -> HistoryTokens {
    let mut h = HistoryTokens::default();
    for m in messages {
        for b in &m.blocks {
            match b {
                Block::Text { text } => h.text += estimate(text),
                Block::ToolUse { name, input, .. } => h.tool_calls += estimate(name) + estimate(&input.to_string()),
                Block::ToolResult { content, .. } => h.tool_results += estimate(content),
                Block::ToolImage { tokens, .. } => h.images += u64::from(*tokens),
            }
        }
    }
    h
}

/// The part of a request that is the same from one message to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Standing {
    pub tools: u64,
    /// The assistant's own rules, less the two parts below.
    pub system: u64,
    /// The project's instruction files (`CLAUDE.md`, `AGENTS.md`).
    pub instructions: u64,
    /// The index of the skills the user enabled.
    pub skills: u64,
}

impl Standing {
    pub fn total(&self) -> u64 {
        self.tools + self.system + self.instructions + self.skills
    }
}

/// The tool definitions as the provider receives them, estimated.
pub fn tools_tokens(tools: &[ToolSpec]) -> u64 {
    estimate(&serde_json::to_string(tools).unwrap_or_default())
}

/// The standing part of a request. `instructions` and `skills` are the texts already inside `system`;
/// they are told apart so each can be seen on its own, and subtracted from the rest.
pub fn standing(system: &str, instructions: &str, skills: &str, tools: &[ToolSpec]) -> Standing {
    let (instructions_t, skills_t) = (estimate(instructions), estimate(skills));
    Standing {
        tools: tools_tokens(tools),
        system: estimate(system).saturating_sub(instructions_t + skills_t),
        instructions: instructions_t,
        skills: skills_t,
    }
}

/// Every non-empty slice of a request, in the order the provider reads them.
pub fn parts(standing: &Standing, history: &HistoryTokens) -> Vec<Part> {
    let all = [
        Part { id: "tools", label: "Tool definitions", tokens: standing.tools },
        Part { id: "system", label: "Imber rules", tokens: standing.system },
        Part { id: "instructions", label: "Project instructions", tokens: standing.instructions },
        Part { id: "skills", label: "Skills index", tokens: standing.skills },
        Part { id: "text", label: "Conversation", tokens: history.text },
        Part { id: "tool_calls", label: "Tool calls", tokens: history.tool_calls },
        Part { id: "tool_results", label: "Tool results", tokens: history.tool_results },
        Part { id: "images", label: "Pictures", tokens: history.images },
    ];
    all.into_iter().filter(|p| p.tokens > 0).collect()
}

/// The tools ordered by what their definition costs on every request, heaviest first.
pub fn tool_weights(tools: &[ToolSpec]) -> Vec<(String, u64)> {
    let mut w: Vec<(String, u64)> = tools.iter().map(|t| (t.name.clone(), tools_tokens(std::slice::from_ref(t)))).collect();
    w.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    w
}

/// What the provider's own report says about how much of a request came from the cache, as a share of the
/// whole prompt (0.0 to 1.0). `None` before the first answer.
pub fn cache_hit_share(input: u64, cache_read: u64, cache_write: u64) -> Option<f64> {
    let prompt = input + cache_read + cache_write;
    (prompt > 0).then(|| cache_read as f64 / prompt as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, description: &str) -> ToolSpec {
        ToolSpec { name: name.into(), description: description.into(), input_schema: json!({"type": "object"}) }
    }

    #[test]
    fn four_characters_make_a_token_rounded_up() {
        assert_eq!(estimate(""), 0);
        assert_eq!(estimate("abcd"), 1);
        assert_eq!(estimate("abcde"), 2);
    }

    #[test]
    fn the_history_is_split_by_what_it_is_made_of() {
        let messages = vec![
            Message::user("x".repeat(400)),
            Message::assistant(vec![Block::ToolUse { id: "a".into(), name: "read_file".into(), input: json!({"path": "A.java"}) }]),
            Message::tool_results(vec![
                Block::ToolResult { tool_use_id: "a".into(), content: "y".repeat(800), is_error: false },
                Block::ToolImage { tool_use_id: "a".into(), media_type: "image/png".into(), path: "s.png".into(), tokens: 900, data: None },
            ]),
        ];
        let h = history_tokens(&messages);
        assert_eq!(h.text, 100);
        assert_eq!(h.tool_results, 200);
        assert_eq!(h.images, 900);
        assert!(h.tool_calls > 0 && h.tool_calls < 20);
        assert_eq!(h.total(), h.text + h.tool_calls + h.tool_results + h.images);
    }

    #[test]
    fn instructions_and_skills_are_taken_out_of_the_rules_not_counted_twice() {
        let rules = "r".repeat(400);
        let instructions = "i".repeat(200);
        let skills = "s".repeat(80);
        let system = format!("{rules}{instructions}{skills}");
        let s = standing(&system, &instructions, &skills, &[]);
        assert_eq!((s.system, s.instructions, s.skills), (100, 50, 20));
        assert_eq!(s.total() - s.tools, estimate(&system), "the parts add up to the whole system text");
    }

    #[test]
    fn the_parts_come_in_the_order_the_provider_reads_them_and_skip_the_empty() {
        let s = Standing { tools: 10, system: 5, instructions: 0, skills: 0 };
        let h = HistoryTokens { text: 3, tool_calls: 0, tool_results: 7, images: 0 };
        let ids: Vec<&str> = parts(&s, &h).iter().map(|p| p.id).collect();
        assert_eq!(ids, ["tools", "system", "text", "tool_results"]);
    }

    #[test]
    fn the_heaviest_tool_is_listed_first() {
        let w = tool_weights(&[tool("a", "short"), tool("b", &"long ".repeat(100))]);
        assert_eq!(w[0].0, "b");
        assert!(w[0].1 > w[1].1);
    }

    #[test]
    fn the_cache_share_is_the_read_part_of_the_whole_prompt() {
        assert_eq!(cache_hit_share(0, 0, 0), None);
        assert_eq!(cache_hit_share(100, 900, 0), Some(0.9));
        assert_eq!(cache_hit_share(0, 0, 500), Some(0.0));
    }
}
