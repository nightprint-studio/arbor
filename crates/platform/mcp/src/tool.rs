//! The MCP wire shapes for tools and their results.
//!
//! These are the protocol's types, camelCased on the wire, and deliberately separate
//! from `arbor_rpc::ToolDescriptor` — that one describes a *handler* and travels the
//! internal seam; this one describes a *tool* and travels to a third party. The host
//! maps between them, which is where product policy (which tools are visible, what a
//! result may contain) belongs.

use serde::Serialize;
use serde_json::Value;

/// A tool as advertised by `tools/list`.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    pub annotations: ToolAnnotations,
}

/// Behavioural hints. **Hints**: a client may show them to the user or use them to
/// decide what to auto-approve, but nothing enforces them — the host's policy layer is
/// what actually gates a call.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ToolAnnotations {
    #[serde(rename = "readOnlyHint")]
    pub read_only_hint: bool,
    #[serde(rename = "destructiveHint")]
    pub destructive_hint: bool,
    #[serde(rename = "idempotentHint")]
    pub idempotent_hint: bool,
    #[serde(rename = "openWorldHint")]
    pub open_world_hint: bool,
}

/// One block of a tool's answer.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        /// Base64, no data-URI prefix.
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

impl Content {
    pub fn text(text: impl Into<String>) -> Self {
        Content::Text { text: text.into() }
    }

    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Content::Image { data: data.into(), mime_type: mime_type.into() }
    }
}

/// What `tools/call` returns.
///
/// A tool that fails answers here with `is_error: true` — not with a JSON-RPC error.
/// The distinction is load-bearing: a JSON-RPC error is a protocol fault the client
/// handles, while a failed tool is *information for the model*, which can read the
/// message and try something else.
#[derive(Debug, Clone, Serialize)]
pub struct CallToolResult {
    pub content: Vec<Content>,
    #[serde(rename = "isError", skip_serializing_if = "is_false")]
    pub is_error: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

impl CallToolResult {
    /// A successful call carrying one text block.
    pub fn text(text: impl Into<String>) -> Self {
        Self { content: vec![Content::text(text)], is_error: false }
    }

    /// A successful call carrying arbitrary blocks.
    pub fn blocks(content: Vec<Content>) -> Self {
        Self { content, is_error: false }
    }

    /// A failed call. The message is written for the model, so it should say what to
    /// do differently, not just what broke.
    pub fn error(message: impl Into<String>) -> Self {
        Self { content: vec![Content::text(message)], is_error: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_tool_serializes_with_the_protocol_spelling() {
        let tool = Tool {
            name: "bennu_read_file".into(),
            title: Some("Read a project file".into()),
            description: "…".into(),
            input_schema: json!({ "type": "object" }),
            annotations: ToolAnnotations {
                read_only_hint: true,
                destructive_hint: false,
                idempotent_hint: true,
                open_world_hint: false,
            },
        };
        let v = serde_json::to_value(&tool).unwrap();
        assert!(v.get("inputSchema").is_some(), "camelCase on the wire: {v}");
        assert_eq!(v["annotations"]["readOnlyHint"], json!(true));
    }

    #[test]
    fn a_successful_result_omits_is_error() {
        let v = serde_json::to_value(CallToolResult::text("ok")).unwrap();
        assert!(v.get("isError").is_none(), "{v}");
        assert_eq!(v["content"][0]["type"], "text");
    }

    #[test]
    fn a_failed_result_says_so() {
        let v = serde_json::to_value(CallToolResult::error("no project is open")).unwrap();
        assert_eq!(v["isError"], json!(true));
    }

    #[test]
    fn an_image_block_carries_mime_type() {
        let v = serde_json::to_value(CallToolResult::blocks(vec![Content::image("AAAA", "image/png")]))
            .unwrap();
        assert_eq!(v["content"][0]["type"], "image");
        assert_eq!(v["content"][0]["mimeType"], "image/png");
        assert_eq!(v["content"][0]["data"], "AAAA");
    }
}
