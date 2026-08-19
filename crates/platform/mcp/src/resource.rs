//! Resources — context a client can read **without calling a tool**.
//!
//! The distinction MCP draws is about who decides. A *tool* is the model choosing to act;
//! a *resource* is context the application offers and the client (or the user) attaches.
//! That makes resources the right shape for the one question every session opens with —
//! *what is this user working on right now* — which as a tool would be a wasted call,
//! and as an instruction string would be stale the moment they opened something else.
//!
//! Read-only by construction: there is no `resources/write`, and nothing here can change
//! anything. That is why resources need no consent prompt while tools do.

use async_trait::async_trait;
use serde::Serialize;

/// One offered resource, as `resources/list` advertises it.
#[derive(Debug, Clone, Serialize)]
pub struct Resource {
    /// Stable identifier the client passes back to `resources/read`. Arbor uses
    /// `arbor://…` so a URI is traceable to what produced it.
    pub uri: String,
    /// Programmatic name.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// The body of a resource.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceContents {
    pub uri: String,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub text: String,
}

/// What the host offers to read.
///
/// Separate from [`ToolCatalog`](crate::ToolCatalog) rather than folded into it: a server
/// may have tools and no resources, and the `initialize` handshake has to say which —
/// advertising a capability nothing answers is how a client ends up showing an empty
/// resource picker.
#[async_trait]
pub trait ResourceCatalog: Send + Sync + 'static {
    /// Everything currently on offer. Called per `resources/list`, so it may legitimately
    /// differ between calls — that is the point.
    async fn list(&self) -> Vec<Resource>;

    /// Read one. `Err` carries a message for the client; an unknown URI belongs here
    /// rather than as an empty success.
    async fn read(&self, uri: &str) -> Result<Vec<ResourceContents>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resource_serializes_with_the_protocol_spelling() {
        let r = Resource {
            uri: "arbor://project/a".into(),
            name: "a".into(),
            title: Some("A".into()),
            description: None,
            mime_type: Some("text/markdown".into()),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["mimeType"], "text/markdown");
        // Absent optionals stay absent rather than serializing as null.
        assert!(v.get("description").is_none());
    }
}
