//! The conversation, as every provider sees it: neutral, and nothing like any one wire format.
//!
//! A provider translates this into its own request (see [`crate::provider::Provider::body`]) and
//! translates its own stream back into [`crate::provider::Event`]s. Keeping the shape here — roles, a
//! message as a list of blocks, tool calls as a block with an id — is what lets the loop, the tools and
//! the persisted conversation stay the same when a second provider arrives.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::effort::Effort;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// One piece of a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text { text: String },
    /// The model asking for a tool. `input` is the arguments, already parsed.
    ToolUse { id: String, name: String, input: Value },
    /// What a tool answered, for the `ToolUse` with this id. An error is an answer too: the model is
    /// told what went wrong and decides what to do, rather than the loop guessing.
    ToolResult { tool_use_id: String, content: String, is_error: bool },
    /// A picture a tool returned along with its result (a screenshot), for the `ToolResult` with this id.
    /// Only the file's *name* is persisted: the bytes live on disk, and the host loads them into `data`
    /// just before a request goes out (this crate has no filesystem). A provider that finds no `data`
    /// says what was there in words instead of sending nothing.
    ToolImage {
        tool_use_id: String,
        /// `image/png` or `image/jpeg`.
        media_type: String,
        /// Where the host keeps the file; opaque here.
        path: String,
        /// What the picture costs the model, estimated when it was taken — counted by the context meter.
        tokens: u32,
        /// Base64 of the file, filled in for one request and never saved.
        #[serde(skip)]
        data: Option<String>,
    },
}

/// A picture a tool produced, as the tool host hands it to the loop: where it is stored and what it costs.
/// The loop turns it into a [`Block::ToolImage`] beside the result it belongs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageRef {
    pub media_type: String,
    pub path: String,
    pub tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub blocks: Vec<Block>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, blocks: vec![Block::Text { text: text.into() }] }
    }

    pub fn assistant(blocks: Vec<Block>) -> Self {
        Self { role: Role::Assistant, blocks }
    }

    /// The results of the tools a turn called, as the one user message that carries them all — a
    /// provider wants every result of a turn together, and a message per result teaches the model to
    /// stop calling tools in parallel.
    pub fn tool_results(results: Vec<Block>) -> Self {
        Self { role: Role::User, blocks: results }
    }

    /// The tool calls this message makes, in order.
    pub fn tool_uses(&self) -> impl Iterator<Item = (&str, &str, &Value)> {
        self.blocks.iter().filter_map(|b| match b {
            Block::ToolUse { id, name, input } => Some((id.as_str(), name.as_str(), input)),
            _ => None,
        })
    }

    /// All the text of the message, joined.
    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

/// A tool the model may call, as a provider needs to be told about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// A JSON Schema for the arguments.
    pub input_schema: Value,
}

/// Everything a provider needs to make one request.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub model: String,
    /// The standing instructions. Identical from one request to the next, which is what lets a provider
    /// cache them — anything that changes per request belongs in a message, not here.
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
    /// `None` for a model without the setting, or to leave it at the provider's own default.
    pub effort: Option<Effort>,
    /// Keep the standing prefix (tools and instructions) in the provider's long-lived cache, for a conversation whose
    /// user pauses longer than the short cache lasts. Costs more to write, nothing to read.
    pub long_cache: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_message_reports_its_tool_calls_in_order() {
        let m = Message::assistant(vec![
            Block::Text { text: "Looking".into() },
            Block::ToolUse { id: "a".into(), name: "read_file".into(), input: json!({"path": "A.java"}) },
            Block::ToolUse { id: "b".into(), name: "find_usages".into(), input: json!({"symbol": "total"}) },
        ]);
        let calls: Vec<_> = m.tool_uses().map(|(id, name, _)| (id, name)).collect();
        assert_eq!(calls, [("a", "read_file"), ("b", "find_usages")]);
        assert_eq!(m.text(), "Looking");
    }

    #[test]
    fn blocks_round_trip_through_json_with_a_type_tag() {
        let b = Block::ToolResult { tool_use_id: "a".into(), content: "412 lines".into(), is_error: false };
        let text = serde_json::to_string(&b).unwrap();
        assert!(text.contains("\"type\":\"tool_result\""), "{text}");
        assert_eq!(serde_json::from_str::<Block>(&text).unwrap(), b);
    }

    #[test]
    fn a_tool_image_is_saved_without_its_bytes() {
        let b = Block::ToolImage { tool_use_id: "a".into(), media_type: "image/png".into(), path: "s1.png".into(), tokens: 900, data: Some("AAAA".into()) };
        let text = serde_json::to_string(&b).unwrap();
        assert!(text.contains("\"type\":\"tool_image\"") && !text.contains("AAAA"), "{text}");
        let Block::ToolImage { data, tokens, .. } = serde_json::from_str::<Block>(&text).unwrap() else { panic!("an image") };
        assert_eq!((data, tokens), (None, 900));
    }

    #[test]
    fn all_the_results_of_a_turn_travel_in_one_message() {
        let m = Message::tool_results(vec![
            Block::ToolResult { tool_use_id: "a".into(), content: "x".into(), is_error: false },
            Block::ToolResult { tool_use_id: "b".into(), content: "y".into(), is_error: true },
        ]);
        assert_eq!(m.role, Role::User);
        assert_eq!(m.blocks.len(), 2);
    }
}
